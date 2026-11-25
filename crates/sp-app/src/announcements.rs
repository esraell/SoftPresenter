//! The Announcements layer: something that goes round by itself over the slides and media, and
//! under props and messages, until it is cleared or a slide goes on screen. Anything that loops
//! can be on it:
//!
//! - a song, its slides in turn (each for its Go to Next time, or 8 seconds), round to the first;
//! - a picture from the media bin, which stays;
//! - a video from the media bin, which loops;
//! - a media playlist, its pictures (8 seconds each) and videos (to their end) in turn, round
//!   and round; one video alone loops.
//!
//! A picture or a video is a slide of one box filling the screen, drawn as its media bin item
//! is set to fill it (on black, which covers what is under the layer). A video's pictures are
//! shown in its box as they come, as a video in a box on a slide is.
//!
//! A slide going on screen (a song's, a Bible verse, a playlist item's: by a click, a key, Enter,
//! `sp`, a macro or a slide's action) takes the announcements off every screen that shows
//! slides, and they stay off until someone starts them again (Announce, a macro step,
//! `sp announce`): clearing the slide doesn't bring them back. A lobby screen showing
//! announcements and no slides keeps them; with none, the layer stops. A picture or video on
//! the media layer, a prop, a message or a timer leaves them on.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sp_model::{Slide, Theme, Uuid};
use sp_video::Lend;

use crate::i18n::t;
use crate::output::{Announcement, OutputMsg};

/// Something asked to play on the Announcements layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Announce {
    /// A song, by its place among the presentations.
    Song(usize),
    /// A picture or a video from the media bin, by its id.
    Media(Uuid),
    /// A media playlist, by its id.
    MediaPlaylist(Uuid),
}

/// How long an announcement slide, or a picture in a media playlist, stays up when nothing
/// says otherwise.
pub const SECONDS: f32 = 8.0;

/// What plays on the Announcements layer now.
pub struct Announcing {
    pub what: Announce,
    /// The song's slide shown, or the place in the media playlist.
    pub at: usize,
    /// The media bin item shown, for a picture, a video or a media playlist.
    pub item: Option<Uuid>,
    pub since: Instant,
    pub shown: Announcement,
    /// The video playing, when the item shown is one.
    pub video: Option<sp_video::Player>,
    /// False once a slide has gone on screen (see the module's notes).
    pub on_slide_screens: bool,
}

/// The picture or video the layer shows, as the window shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// Its media bin item.
    pub item: Uuid,
    /// Its video, how far in and how it plays, when it is one.
    pub video: Option<crate::home::VideoLive>,
    /// False once a slide has taken the layer off the screens that show slides, so that only a
    /// lobby screen shows it.
    pub over_slides: bool,
}

impl Announcing {
    /// The song on the layer, if a song is.
    pub fn song(&self) -> Option<usize> {
        match self.what {
            Announce::Song(song) => Some(song),
            Announce::Media(_) | Announce::MediaPlaylist(_) => None,
        }
    }
}

/// What has been chosen to play on the Announcements layer, so that a picture, video or media
/// playlist among it plays there when it is clicked: what plays there now, or last played there
/// (it stays chosen after a clear, or a slide taking it off); what the macros' steps play there
/// (a start-up macro's loop among them); and the media playlist that follows Canva Sync's folder
/// (the church's announcements video). `canva` is that folder.
fn chosen<'a>(
    last: Option<Announce>,
    macros: &'a [crate::macros::Macro],
    playlists: &'a [crate::media::Playlist],
    canva: &'a Path,
) -> impl Iterator<Item = Announce> + 'a {
    use sp_model::{Announced, ShowAction};
    let steps = macros.iter().flat_map(|made| &made.actions).filter_map(|action| match action {
        ShowAction::Announce { what: Announced::Media { id } } => Some(Announce::Media(*id)),
        ShowAction::Announce { what: Announced::MediaPlaylist { id } } => Some(Announce::MediaPlaylist(*id)),
        _ => None,
    });
    let canva = playlists
        .iter()
        .filter(move |playlist| playlist.folder.as_deref().is_some_and(|folder| same_folder(folder, canva)))
        .map(|playlist| Announce::MediaPlaylist(playlist.id));
    last.into_iter().chain(steps).chain(canva)
}

/// The folder Canva Sync puts the video in, by its full path when it is there.
fn canva_folder() -> std::path::PathBuf {
    let folder = crate::canva::folder();
    folder.canonicalize().unwrap_or(folder)
}

/// Whether a folder a media playlist follows is `canva` (by its full path when it is there: one
/// chosen from the command line is kept by its full path).
fn same_folder(folder: &Path, canva: &Path) -> bool {
    folder == canva || folder.canonicalize().is_ok_and(|full| full == canva)
}

/// What a click on a picture or video of the media bin plays on the Announcements layer, when it
/// belongs to the announcements (see [`chosen`]): the item itself, or a media playlist it is in,
/// from it. `None` puts it on the media layer, as usual.
fn of_media(
    item: &sp_model::MediaItem,
    last: Option<Announce>,
    macros: &[crate::macros::Macro],
    playlists: &[crate::media::Playlist],
    canva: &Path,
) -> Option<(Announce, Option<Uuid>)> {
    let holds = |id: Uuid| playlists.iter().any(|playlist| playlist.id == id && playlist.has(&item.path));
    chosen(last, macros, playlists, canva)
        .find(|what| match what {
            Announce::Media(id) => *id == item.id,
            Announce::MediaPlaylist(id) => holds(*id),
            Announce::Song(_) => false,
        })
        .map(|what| (what, Some(item.id)))
}

/// What a click on a picture or video of the media bin (in the media strip or a playlist) plays
/// on the Announcements layer, when it belongs to the announcements; `None` for the media layer.
pub fn for_media(state: &crate::home::HomeState, item: &sp_model::MediaItem) -> Option<(Announce, Option<Uuid>)> {
    of_media(item, state.announced_last, &state.macros, &state.media_playlists, &canva_folder())
}

/// The same for a media playlist clicked in a playlist, or started by the keys: it plays round on
/// the Announcements layer when it belongs to the announcements; `None` for the media layer.
pub fn for_media_playlist(state: &crate::home::HomeState, id: Uuid) -> Option<(Announce, Option<Uuid>)> {
    of_playlist(id, state.announced_last, &state.macros, &state.media_playlists, &canva_folder())
}

/// What a click on a media playlist plays on the Announcements layer, when it belongs to the
/// announcements (see [`chosen`]).
fn of_playlist(
    id: Uuid,
    last: Option<Announce>,
    macros: &[crate::macros::Macro],
    playlists: &[crate::media::Playlist],
    canva: &Path,
) -> Option<(Announce, Option<Uuid>)> {
    let what = Announce::MediaPlaylist(id);
    chosen(last, macros, playlists, canva).any(|chosen| chosen == what).then_some((what, None))
}

/// Whether the Announcements layer shows this media bin item now (the media strip rings it, a
/// playlist lights it, as for the media layer).
pub fn shows_item(state: &crate::home::HomeState, id: Uuid) -> bool {
    state.announcement_media.as_ref().is_some_and(|shown| shown.item == id)
}

/// Whether the Announcements layer plays this media playlist now.
pub fn plays_playlist(state: &crate::home::HomeState, id: Uuid) -> bool {
    state.announcement_live.as_ref().is_some_and(|(what, _)| *what == Announce::MediaPlaylist(id))
}

/// Whether the layer shows what a click asks for already, over the slides (the item clicked, or
/// any of a media playlist's when no item was): then the click changes nothing, as a click on the
/// picture behind the words changes nothing, and a video isn't started again.
fn shows_already(on: Option<(Announce, Option<Uuid>, bool)>, what: Announce, from: Option<Uuid>) -> bool {
    on.is_some_and(|(playing, item, over_slides)| over_slides && playing == what && (from.is_none() || from == item))
}

/// True when a screen that is in use shows announcements and no slides (a lobby screen), so
/// the layer goes on for it after a slide goes on screen.
pub fn lobby_screen(screens: &[crate::screens::Screen]) -> bool {
    screens.iter().any(|screen| {
        screen.kind == crate::screens::Kind::Audience
            && screen.output != crate::screens::Output::Off
            && screen.look.announcements
            && !screen.look.slides
            && screen.look.key_of.is_none()
    })
}

/// Said once a run, the first time a slide takes the announcements off.
static OFF_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The theme pictures and videos are drawn with on the layer: black behind them, so a picture
/// fitted inside the screen has black bars, not the slides showing through.
fn media_theme() -> Arc<Theme> {
    Arc::new(Theme { background: Some(sp_model::Rgba::BLACK), background_picture: None, ..Theme::builtin() })
}

/// A slide of one box filling the screen, showing a file as it is set to fill it.
pub fn media_slide(file: &str, name: &str, fit: sp_model::ImageFit) -> Slide {
    let mut object = sp_model::Object::image(file.to_owned(), name.to_owned(), 16, 9);
    object.frame = Some(sp_model::SlideRect { x: 0.0, y: 0.0, w: sp_model::SLIDE_WIDTH, h: sp_model::SLIDE_HEIGHT });
    object.kind = sp_model::ObjectKind::Image { file: file.to_owned(), fit };
    Slide::new(vec![object])
}

/// The name a file's box shows it under on the layer: its path, so a file not read yet is read
/// for the screens as any picture on a slide is.
fn shown_name(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

impl crate::App {
    /// Starts something on the Announcements layer, or stops it, as the window or a macro asked.
    pub(crate) fn handle_announcement(&mut self) {
        let Some(state) = self.running.as_mut().map(|r| &mut r.home_state) else { return };
        let (asked, clicked) = (state.announcement_request.take(), state.announcement_click.take());
        match asked {
            Some(Some(what)) => self.start_announcement(what),
            Some(None) => self.apply(vec![sp_engine::Command::Clear(sp_engine::Layer::Announcements)]),
            None => {}
        }
        if let Some((what, from)) = clicked {
            self.announce_clicked(what, from);
        }
    }

    /// Plays on the layer what a click on a picture, video or media playlist that belongs to the
    /// announcements asked for (a media playlist from the item clicked, when one was), unless the
    /// layer shows it already.
    pub(crate) fn announce_clicked(&mut self, what: Announce, from: Option<Uuid>) {
        let on = self.announcing.as_ref().map(|on| (on.what, on.item, on.on_slide_screens));
        if shows_already(on, what, from) {
            return;
        }
        let start_at = match (what, from) {
            (Announce::MediaPlaylist(id), Some(item)) => {
                let items = self.playlist_media(id);
                let index = self.show.media().iter().position(|have| have.id == item);
                index.and_then(|index| items.iter().position(|at| *at == index).map(|at| (at, index)))
            }
            _ => None,
        };
        match start_at {
            Some((at, index)) => {
                // A fresh start, so it is over the slides again.
                self.announcing = None;
                self.show_media_announcement(what, at, index);
            }
            None => self.start_announcement(what),
        }
    }

    /// Puts something on the Announcements layer in place of what was there.
    fn start_announcement(&mut self, what: Announce) {
        self.announcing = None;
        let said = match what {
            Announce::Song(song) => match self.next_announcement_slide(song, None) {
                Some(first) => {
                    self.show_announcement_slide(song, first);
                    None
                }
                None => Some(t("tools.announce.no_slide", "That song has no slide that is on.").to_owned()),
            },
            Announce::Media(id) => match self.show.media().iter().position(|item| item.id == id) {
                Some(index) => {
                    self.show_media_announcement(what, 0, index);
                    None
                }
                None => Some(
                    t("tools.announce.media_gone", "That picture or video isn't in the media bin any more.").to_owned(),
                ),
            },
            Announce::MediaPlaylist(id) => match self.playlist_media(id).first() {
                Some(first) => {
                    self.show_media_announcement(what, 0, *first);
                    None
                }
                None => Some(
                    t("tools.announce.playlist_empty", "That media playlist has nothing in it to play.").to_owned(),
                ),
            },
        };
        if let Some(said) = said
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.notices.refuse(said);
            running.home_window.request_redraw();
        }
    }

    /// The slide that follows `after` on the Announcements layer: the next one that is on, round
    /// to the first after the last. `None` when the song has no slide that is on.
    fn next_announcement_slide(&self, song: usize, after: Option<usize>) -> Option<usize> {
        let song = self.show.presentations().get(song)?;
        let enabled: Vec<usize> =
            song.slides().enumerate().filter(|(_, (_, slide))| slide.enabled).map(|(at, _)| at).collect();
        let start = after.map_or(0, |after| after + 1);
        enabled.iter().copied().find(|at| *at >= start).or_else(|| enabled.first().copied())
    }

    /// Puts one slide of a song on the Announcements layer.
    fn show_announcement_slide(&mut self, song: usize, slide: usize) {
        let Some((_, shown)) = self.show.presentations().get(song).and_then(|s| s.slide(slide)) else {
            self.announcing = None;
            return;
        };
        let theme = self.theme_of(song).clone();
        let on_slide_screens = self.still_on_slide_screens(Announce::Song(song));
        let shown = Announcement { slide: Arc::new(shown.clone()), theme, video: None, on_slide_screens };
        let title = self.show.presentations().get(song).map_or_else(String::new, |s| s.title.clone());
        if self.announcing.as_ref().is_none_or(|was| was.what != Announce::Song(song)) {
            crate::applog::event("announcements", &format!("playing {title}"));
        }
        self.send_announcement(Announce::Song(song), &shown, title);
        self.announcing = Some(Announcing {
            what: Announce::Song(song),
            at: slide,
            item: None,
            since: Instant::now(),
            shown,
            video: None,
            on_slide_screens,
        });
        self.refresh_preview(false);
    }

    /// Puts a picture or a video from the media bin (the item at `index`) on the Announcements
    /// layer, for `what` (the item itself, or a media playlist at its place `at`). A video
    /// plays, looping unless a media playlist of several items goes on after it.
    pub(crate) fn show_media_announcement(&mut self, what: Announce, at: usize, index: usize) {
        let Some(item) = self.show.media().get(index).cloned() else {
            self.announcing = None;
            return;
        };
        let name = shown_name(&item.path);
        let slide = Arc::new(media_slide(&name, &item.name, item.fit));
        let many = match what {
            Announce::MediaPlaylist(id) => self.playlist_media(id).len() > 1,
            Announce::Song(_) | Announce::Media(_) => false,
        };
        let mut video = None;
        if crate::media::is_video(&item.path) {
            // Its still stands in until the video's first picture comes.
            if let Some(still) = self.bin_pictures.small(item.id) {
                sp_render::show_picture(&name, still.clone());
            }
            match self.open_announcement_video(&item.path, !many, false) {
                Ok(player) => video = Some(player),
                Err(problem) => {
                    crate::applog::problem("announcements", &problem);
                    // A file just taken out of a followed folder: the layer goes on to what is
                    // left (see `followed`), with nothing to tell the operator.
                    if !self.gone_from_followed_folder(&item.path)
                        && let Some(running) = self.running.as_mut()
                    {
                        running.home_state.notices.trouble(problem);
                    }
                }
            }
        } else {
            // A picture is read for the screens at once, so it goes on as soon as it can.
            sp_render::read_ahead(&slide);
        }
        let on_slide_screens = self.still_on_slide_screens(what);
        let shown = Announcement {
            slide,
            theme: media_theme(),
            video: video.as_ref().map(|player: &sp_video::Player| (name.clone(), player.frames())),
            on_slide_screens,
        };
        let title = self.media_announcement_title(what, &item.name);
        if self.announcing.as_ref().is_none_or(|was| was.what != what) {
            crate::applog::event("announcements", &format!("playing {title}"));
        }
        self.send_announcement(what, &shown, title);
        self.announcing =
            Some(Announcing { what, at, item: Some(item.id), since: Instant::now(), shown, video, on_slide_screens });
        self.refresh_preview(false);
        self.read_ahead_on_layer();
    }

    /// Whether what goes on round on the layer is still on the screens that show slides: a
    /// fresh start is; the same thing going on to its next slide or item keeps what it had.
    fn still_on_slide_screens(&self, what: Announce) -> bool {
        self.announcing.as_ref().filter(|on| on.what == what).is_none_or(|on| on.on_slide_screens)
    }

    /// A slide has gone on screen: the announcements go off every screen that shows slides, and
    /// stay off until started again. A lobby screen (announcements, no slides) keeps them; with
    /// none, the layer stops. Said once a run.
    pub(crate) fn announcements_off_for_slide(&mut self) {
        if !self.announcing.as_ref().is_some_and(|on| on.on_slide_screens) {
            return;
        }
        let lobby = self.running.as_ref().is_some_and(|running| lobby_screen(&running.home_state.screens));
        if lobby {
            let Some(on) = self.announcing.as_mut() else { return };
            on.on_slide_screens = false;
            on.shown.on_slide_screens = false;
            let shown = on.shown.clone();
            if let Some(output) = self.running.as_ref().and_then(|running| running.output.as_ref()) {
                output.send(OutputMsg::Announcement(Some(shown)));
            }
            crate::applog::event("announcements", "a slide went on screen: off the screens that show slides");
        } else {
            self.announcing = None;
            if let Some(running) = self.running.as_mut() {
                running.home_state.announcement_live = None;
                if let Some(output) = &running.output {
                    output.send(OutputMsg::Announcement(None));
                }
            }
            crate::applog::event("announcements", "a slide went on screen: the announcements stopped");
        }
        if !OFF_SAID.swap(true, std::sync::atomic::Ordering::Relaxed)
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.notices.info(t(
                "tools.announce.off",
                "Announcements are off while a slide is on screen. Announce starts them again.",
            ));
        }
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// What the window calls a picture or video on the layer: its name, after its media
    /// playlist's when it plays from one.
    pub(crate) fn media_announcement_title(&self, what: Announce, name: &str) -> String {
        match what {
            Announce::MediaPlaylist(id) => {
                let playlist = self.running.as_ref().and_then(|running| {
                    running.home_state.media_playlists.iter().find(|p| p.id == id).map(|p| p.name.clone())
                });
                format!("{} ({name})", playlist.unwrap_or_default())
            }
            Announce::Song(_) | Announce::Media(_) => name.to_owned(),
        }
    }

    /// Opens a video for the layer: playing, looping or not, silent on a quiet run (a test).
    fn open_announcement_video(
        &self,
        path: &std::path::Path,
        looping: bool,
        with_ffmpeg: bool,
    ) -> Result<sp_video::Player, String> {
        let mut player = if with_ffmpeg {
            sp_video::Player::open_with_ffmpeg(path, Lend::Rows)
        } else {
            sp_video::Player::open(path, Lend::Rows)
        }?;
        player.set_looping(looping);
        if self.quiet() {
            player.set_volume(0.0);
        }
        player.play();
        crate::applog::event(
            "announcements",
            &format!(
                "playing the video {} with {}",
                crate::file_name(path),
                if player.uses_ffmpeg() { "FFmpeg" } else { "the system's player" }
            ),
        );
        Ok(player)
    }

    /// Tells the screens what the layer shows, and the window what plays there.
    fn send_announcement(&mut self, what: Announce, shown: &Announcement, title: String) {
        if let Some(running) = self.running.as_mut() {
            if let Some(output) = &running.output {
                output.send(OutputMsg::Announcement(Some(shown.clone())));
            }
            running.home_state.announcement_live = Some((what, title));
            running.home_state.announced_last = Some(what);
            running.home_window.request_redraw();
        }
    }

    /// Has the next picture of a media playlist on the layer read for the screens ahead, so it
    /// goes on at once.
    fn read_ahead_on_layer(&mut self) {
        let Some(Announce::MediaPlaylist(id)) = self.announcing.as_ref().map(|on| on.what) else { return };
        let Some((at, shown)) = self.announcing.as_ref().map(|on| (on.at, on.item)) else { return };
        let items = self.playlist_media(id);
        if items.len() < 2 {
            return;
        }
        let shown = shown.and_then(|item| self.show.media().iter().position(|have| have.id == item));
        let next = shown.map_or((at + 1) % items.len(), |shown| crate::media::next_in_cycle(&items, at, shown));
        if let Some(item) = self.show.media().get(items[next])
            && !crate::media::is_video(&item.path)
        {
            sp_render::read_ahead(&media_slide(&shown_name(&item.path), &item.name, item.fit));
        }
    }

    /// Moves the Announcements layer on: a song's slide or a playlist's picture when it has been
    /// up long enough, a playlist's video when it ends; keeps a video going, and has FFmpeg take
    /// over one the system's player can't play. Says when to look again.
    pub(crate) fn tick_announcements(&mut self) -> Option<Instant> {
        let (what, at, item, since) = self.announcing.as_ref().map(|on| (on.what, on.at, on.item, on.since))?;
        match what {
            Announce::Song(song) => {
                let seconds = self
                    .show
                    .presentations()
                    .get(song)
                    .and_then(|s| s.slide(at))
                    .and_then(|(_, s)| s.advance)
                    .filter(|seconds| *seconds > 0.0)
                    .unwrap_or(SECONDS);
                let due = since + Duration::from_secs_f32(seconds);
                if Instant::now() < due {
                    return Some(due);
                }
                match self.next_announcement_slide(song, Some(at)) {
                    Some(next) => self.show_announcement_slide(song, next),
                    None => self.apply(vec![sp_engine::Command::Clear(sp_engine::Layer::Announcements)]),
                }
                // Looked at again straight away, to work out when the new slide is due.
                self.announcing.as_ref().map(|_| Instant::now() + Duration::from_millis(50))
            }
            Announce::Media(id) => {
                // A picture or video that has left the media bin leaves the layer.
                if !self.show.media().iter().any(|have| have.id == id) {
                    self.apply(vec![sp_engine::Command::Clear(sp_engine::Layer::Announcements)]);
                    return None;
                }
                self.tick_announcement_video()
            }
            Announce::MediaPlaylist(id) => {
                let items = self.playlist_media(id);
                let index = item.and_then(|item| self.show.media().iter().position(|have| have.id == item));
                // The file on screen has left the folder the playlist follows: what goes on next,
                // and when, is worked out with the folder's files (`tick_followed`), which holds
                // the last picture rather than clearing while a new file may be on its way.
                if index.is_some_and(|index| !items.contains(&index)) && self.follows_folder(id) {
                    let soon = Instant::now() + Duration::from_millis(250);
                    return Some(self.tick_announcement_video().map_or(soon, |at| at.min(soon)));
                }
                if items.is_empty() {
                    self.apply(vec![sp_engine::Command::Clear(sp_engine::Layer::Announcements)]);
                    return None;
                }
                let is_video = index
                    .and_then(|index| self.show.media().get(index))
                    .is_some_and(|i| crate::media::is_video(&i.path));
                let ended = self.announcing.as_ref().and_then(|on| on.video.as_ref()).is_some_and(|v| v.ended());
                // The item shown has gone from the playlist, or has had its time: the next one.
                let done = match index {
                    Some(index) if items.contains(&index) => {
                        if is_video {
                            items.len() > 1 && ended
                        } else {
                            items.len() > 1 && since.elapsed() >= Duration::from_secs_f32(SECONDS)
                        }
                    }
                    _ => true,
                };
                if done {
                    // After the one shown, wherever it is now (the playlist may have been
                    // reordered while it played); if it was taken out, the one in its place.
                    let next_at =
                        index.map_or(at % items.len(), |index| crate::media::next_in_cycle(&items, at, index));
                    self.show_media_announcement(what, next_at, items[next_at]);
                    return Some(Instant::now() + Duration::from_millis(50));
                }
                let video_due = self.tick_announcement_video();
                let picture_due = (!is_video && items.len() > 1).then(|| since + Duration::from_secs_f32(SECONDS));
                [video_due, picture_due].into_iter().flatten().min()
            }
        }
    }

    /// Keeps the video on the layer going, if one plays: FFmpeg takes over a video the system's
    /// player can't play. Says when to look at it again.
    fn tick_announcement_video(&mut self) -> Option<Instant> {
        let on = self.announcing.as_mut()?;
        let what = on.what;
        let player = on.video.as_mut()?;
        let wait = player.tick();
        if let Some(note) = player.take_note() {
            crate::applog::event("announcements", &note);
        }
        let handed = player.take_handed_over();
        let problem = player.problem();
        match handed {
            Some(Ok(path)) => {
                crate::applog::event(
                    "announcements",
                    &format!("{} took over at the end of the pass", crate::file_name(&path)),
                );
                let item = self.announcing.as_ref().and_then(|on| on.item);
                let item = item.and_then(|id| self.show.media().iter().find(|i| i.id == id));
                if item.is_some_and(|item| item.path != path) {
                    // Another item, from a followed folder: it is the one on screen now.
                    self.followed_took_over(crate::followed::OnLayer::Announcements, &path);
                } else if let Some(name) = item.map(|i| i.name.clone()) {
                    // The window names the new file (its media bin item has its name now).
                    let title = self.media_announcement_title(what, &name);
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.announcement_live = Some((what, title));
                        running.home_window.request_redraw();
                    }
                }
            }
            Some(Err(problem)) => {
                crate::applog::problem("announcements", &format!("the new file couldn't take over: {problem}"))
            }
            None => {}
        }
        let Some(problem) = problem else {
            return Some(Instant::now() + wait);
        };
        let on = self.announcing.as_mut()?;
        let player = on.video.as_mut()?;
        crate::applog::problem("announcements", &problem);
        let uses_ffmpeg = player.uses_ffmpeg();
        let looping = !matches!(what, Announce::MediaPlaylist(_)) || self.announcing_items() <= 1;
        let path = self
            .announcing
            .as_ref()
            .and_then(|on| on.item)
            .and_then(|id| self.show.media().iter().find(|item| item.id == id))
            .map(|item| item.path.clone());
        let fallback = match path {
            Some(path) if !uses_ffmpeg => self.open_announcement_video(&path, looping, true),
            _ => Err(problem),
        };
        let on = self.announcing.as_mut()?;
        match fallback {
            Ok(player) => {
                // The screens take the pictures from FFmpeg now; the same slide, so no dissolve.
                if let Some((_, frames)) = on.shown.video.as_mut() {
                    *frames = player.frames();
                }
                on.video = Some(player);
                let shown = on.shown.clone();
                if let Some(output) = self.running.as_ref().and_then(|running| running.output.as_ref()) {
                    output.send(OutputMsg::Announcement(Some(shown)));
                }
                Some(Instant::now() + Duration::from_millis(100))
            }
            Err(problem) => {
                on.video = None;
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.trouble(problem);
                }
                None
            }
        }
    }

    /// Tells the window which picture or video the layer shows and how its video plays. Run on
    /// every pass of the event loop, so the window follows the layer however it changed (cleared,
    /// taken off by a slide, on to a media playlist's next item, FFmpeg taking over a video); it
    /// is drawn again only when what it shows changes (the time left, by whole seconds).
    pub(crate) fn show_announcement_in_window(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let was = running.home_state.announcement_media.take();
        let now = self.announcing.as_ref().and_then(|on| {
            let item = on.item?;
            let video = on.video.as_ref().map(|player| {
                // The player doesn't say whether it is muted: the window remembers, for the same
                // video.
                let muted = was.as_ref().filter(|was| was.item == item).and_then(|was| was.video.as_ref());
                crate::home::VideoLive {
                    name: self
                        .show
                        .media()
                        .iter()
                        .find(|have| have.id == item)
                        .map(|have| have.name.clone())
                        .unwrap_or_default(),
                    position: player.position(),
                    duration: player.duration(),
                    playing: player.playing(),
                    looping: player.looping(),
                    muted: muted.is_some_and(|video| video.muted),
                }
            });
            Some(Shown { item, video, over_slides: on.on_slide_screens })
        });
        // The window shows whole seconds.
        let drawn = |shown: &Option<Shown>| {
            shown.clone().map(|mut shown| {
                if let Some(video) = shown.video.as_mut() {
                    video.position = video.position.floor();
                }
                shown
            })
        };
        if drawn(&was) != drawn(&now) {
            running.home_window.request_redraw();
        }
        running.home_state.announcement_media = now;
    }

    /// Carries out what the operator asked of the video on the layer, from the keys under the
    /// preview.
    pub(crate) fn handle_announcement_video(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let Some(request) = running.home_state.announcement_video_request.take() else { return };
        let player = self.announcing.as_mut().and_then(|on| on.video.as_mut());
        let shown = running.home_state.announcement_media.as_mut().and_then(|shown| shown.video.as_mut());
        let (Some(player), Some(shown)) = (player, shown) else { return };
        crate::ask_of_video(player, shown, request);
        running.home_window.request_redraw();
    }

    /// The video of a media bin item playing on the layer has a new file: it is opened now and
    /// takes over at the end of the pass under way (see `play_new_file_next`). Nothing for an
    /// item not playing there, or one of several in a media playlist going round (the playlist
    /// opens the new file next time round).
    pub(crate) fn play_next_on_layer(&mut self, id: Uuid, path: &std::path::Path) -> Result<(), String> {
        let one_of_many = matches!(self.announcing.as_ref().map(|on| on.what), Some(Announce::MediaPlaylist(_)))
            && self.announcing_items() > 1;
        let Some(on) = self.announcing.as_mut().filter(|on| on.item == Some(id)) else { return Ok(()) };
        let Some(player) = on.video.as_mut().filter(|_| !one_of_many) else { return Ok(()) };
        player.play_next(path)?;
        crate::applog::event("announcements", &format!("{} takes over at the end of the pass", crate::file_name(path)));
        Ok(())
    }

    /// How many items the media playlist on the layer has (none for anything else).
    pub(crate) fn announcing_items(&self) -> usize {
        match self.announcing.as_ref().map(|on| on.what) {
            Some(Announce::MediaPlaylist(id)) => self.playlist_media(id).len(),
            _ => 0,
        }
    }

    /// The media bin items on the Announcements layer, or that will be: the picture or video
    /// itself, or every item of the media playlist (so a followed folder doesn't take one out
    /// of the bin from under it).
    pub(crate) fn announced_media(&self) -> Vec<Uuid> {
        let Some(on) = self.announcing.as_ref() else { return Vec::new() };
        let mut items: Vec<Uuid> = on.item.into_iter().collect();
        if let Announce::MediaPlaylist(id) = on.what {
            items.extend(self.playlist_media(id).into_iter().filter_map(|at| self.show.media().get(at).map(|i| i.id)));
        }
        items
    }

    /// What the Announcements layer plays, in words, for `sp status`: the song and its slide
    /// (from 1), the picture or video, or the media playlist and the item it is on.
    pub(crate) fn announcing_words(&self) -> Option<String> {
        let on = self.announcing.as_ref()?;
        match on.what {
            Announce::Song(song) => {
                let song = self.show.presentations().get(song)?;
                Some(format!("{}, slide {}", song.title, on.at + 1))
            }
            Announce::Media(_) | Announce::MediaPlaylist(_) => self
                .running
                .as_ref()
                .and_then(|running| running.home_state.announcement_live.as_ref())
                .map(|(_, title)| title.clone()),
        }
        .map(|words| {
            if on.on_slide_screens { words } else { format!("{words} (lobby screens only: a slide went on screen)") }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_or_video_on_the_layer_is_one_box_filling_the_screen_as_its_item_is_set() {
        let slide = media_slide("/media/notices.mp4", "notices.mp4", sp_model::ImageFit::Fit);
        assert_eq!(slide.objects.len(), 1);
        let object = &slide.objects[0];
        assert_eq!(
            object.frame,
            Some(sp_model::SlideRect { x: 0.0, y: 0.0, w: sp_model::SLIDE_WIDTH, h: sp_model::SLIDE_HEIGHT })
        );
        assert!(matches!(
            &object.kind,
            sp_model::ObjectKind::Image { file, fit: sp_model::ImageFit::Fit } if file == "/media/notices.mp4"
        ));
        assert!(object.fill.is_none() && object.stroke.is_none(), "nothing drawn around it");
        assert_eq!(media_theme().background, Some(sp_model::Rgba::BLACK), "black covers what is under the layer");
    }

    #[test]
    fn a_picture_or_video_chosen_for_the_announcements_plays_there_when_clicked_every_time() {
        use sp_model::{Announced, MediaItem, ShowAction};
        let canva = Path::new("/library/Media/Canva Sync");
        let video = MediaItem::new("Notices.mp4".to_owned(), "/library/Media/Canva Sync/Notices.mp4".into());
        let picture = MediaItem::new("Welcome.png".to_owned(), "/pictures/Welcome.png".into());
        let mut loop_list = crate::media::Playlist::new("Loop".to_owned());
        loop_list.files = vec![video.path.clone(), picture.path.clone()];
        let mut synced = crate::media::Playlist::new("Announcements".to_owned());
        synced.folder = Some(canva.to_owned());
        synced.files = vec![video.path.clone()];
        let other = crate::media::Playlist::new("Backgrounds".to_owned());
        let playlists = vec![loop_list.clone(), synced.clone(), other.clone()];
        let step = |what| crate::macros::Macro {
            actions: vec![ShowAction::Announce { what }],
            ..crate::macros::Macro::new("Before the service".to_owned())
        };
        let nowhere = Path::new("/elsewhere");
        let plain = vec![loop_list.clone(), other.clone()];

        // Nothing chosen for the layer: a click shows it behind the words, as before.
        assert_eq!(of_media(&video, None, &[], &plain, nowhere), None);
        assert_eq!(of_playlist(loop_list.id, None, &[], &plain, nowhere), None);
        // Played there (now, or before and cleared since, or taken off by a slide): there again.
        assert_eq!(
            of_media(&video, Some(Announce::Media(video.id)), &[], &plain, nowhere),
            Some((Announce::Media(video.id), Some(video.id)))
        );
        assert_eq!(of_media(&picture, Some(Announce::Media(video.id)), &[], &plain, nowhere), None, "only that one");
        // A media playlist played there: any of its items plays it, from that item.
        let last = Some(Announce::MediaPlaylist(loop_list.id));
        assert_eq!(
            of_media(&picture, last, &[], &plain, nowhere),
            Some((Announce::MediaPlaylist(loop_list.id), Some(picture.id)))
        );
        assert_eq!(
            of_playlist(loop_list.id, last, &[], &plain, nowhere),
            Some((Announce::MediaPlaylist(loop_list.id), None))
        );
        assert_eq!(of_playlist(other.id, last, &[], &plain, nowhere), None);
        // A macro's step plays it there (a start-up macro's loop): the first click too.
        let macros = [step(Announced::MediaPlaylist { id: loop_list.id })];
        assert_eq!(
            of_media(&picture, None, &macros, &plain, nowhere),
            Some((Announce::MediaPlaylist(loop_list.id), Some(picture.id)))
        );
        let macros = [step(Announced::Media { id: picture.id })];
        assert_eq!(
            of_media(&picture, None, &macros, &plain, nowhere),
            Some((Announce::Media(picture.id), Some(picture.id)))
        );
        assert_eq!(of_media(&video, None, &macros, &plain, nowhere), None);
        // The media playlist following Canva Sync's folder is the church's announcements.
        assert_eq!(
            of_media(&video, None, &[], &playlists, canva),
            Some((Announce::MediaPlaylist(synced.id), Some(video.id)))
        );
        assert_eq!(
            of_playlist(synced.id, None, &[], &playlists, canva),
            Some((Announce::MediaPlaylist(synced.id), None))
        );
        assert_eq!(of_media(&picture, None, &[], &playlists, canva), None, "not in it");
        // What last played there comes first: the video plays on in the loop it was playing in.
        let last = Some(Announce::MediaPlaylist(loop_list.id));
        assert_eq!(
            of_media(&video, last, &[], &playlists, canva),
            Some((Announce::MediaPlaylist(loop_list.id), Some(video.id)))
        );
    }

    #[test]
    fn a_click_on_what_the_layer_shows_already_changes_nothing() {
        let (a, b, list) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let video = Announce::Media(a);
        assert!(shows_already(Some((video, Some(a), true)), video, Some(a)), "the video isn't started again");
        assert!(
            !shows_already(Some((video, Some(a), false)), video, Some(a)),
            "back over the slides after a slide took it off"
        );
        assert!(!shows_already(None, video, Some(a)));
        let round = Announce::MediaPlaylist(list);
        assert!(shows_already(Some((round, Some(a), true)), round, Some(a)));
        assert!(!shows_already(Some((round, Some(a), true)), round, Some(b)), "another of its items: on to it");
        assert!(shows_already(Some((round, Some(a), true)), round, None), "the playlist itself, playing");
        assert!(!shows_already(Some((video, Some(a), true)), round, Some(a)), "the same video, from a playlist");
    }

    #[test]
    fn only_a_screen_in_use_showing_announcements_and_no_slides_keeps_them_when_a_slide_goes_up() {
        use crate::screens::{Kind, Look, Output, Screen};
        let screen = |look: Look, output: Output, kind: Kind| Screen {
            look,
            output,
            kind,
            ..Screen::new("S".to_owned(), Output::Window)
        };
        let usual = screen(Look::default(), Output::Window, Kind::Audience);
        let lobby_look = Look { slides: false, ..Look::default() };
        assert!(!lobby_screen(std::slice::from_ref(&usual)), "screens that show slides take them off");
        assert!(
            lobby_screen(&[usual.clone(), screen(lobby_look, Output::Window, Kind::Audience)]),
            "a lobby keeps them"
        );
        assert!(!lobby_screen(&[screen(lobby_look, Output::Off, Kind::Audience)]), "one switched off doesn't count");
        assert!(!lobby_screen(&[screen(lobby_look, Output::Window, Kind::Stage)]), "nor a stage screen");
        let no_announcements = Look { slides: false, announcements: false, ..Look::default() };
        assert!(!lobby_screen(&[screen(no_announcements, Output::Window, Kind::Audience)]), "nor one without them");
    }
}
