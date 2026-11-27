//! Pictures and videos playing from a folder a media playlist follows, kept going as the files
//! in the folder come and go. A script that puts a fresh announcements video in the folder
//! under a new name and then deletes the old one (or deletes first and puts the new one in a
//! moment later) must never leave the screens blank.
//!
//! The same rules hold wherever such a file plays: a media playlist going round on the media
//! layer or on the Announcements layer, and a video the folder brought in, shown on its own on
//! either layer.
//!
//! - A file new to the folder joins the playlist and plays when its turn comes. A video that
//!   was the playlist's only item, looping, gives way to it at the end of the pass under way.
//! - When the file on screen leaves the folder, the next item goes on: a video still playing
//!   (macOS keeps a deleted file readable while it is open) plays to the end of its pass and
//!   the next takes over there, through the same pictures, with no black between; otherwise the
//!   next goes on at once.
//! - With nothing left, the last picture stays up for a few seconds, and the folder is looked
//!   at every second meanwhile: a new file that comes takes over. Only when none comes does the
//!   layer stop, and the operator is told why.
//! - A file lined up that hasn't given its first picture when the most a video's first picture
//!   is waited for has passed (`video_wait::LinedUp`): the video playing goes round again
//!   meanwhile, as when the file lined up leaves the folder, so the screens never stay on its
//!   last picture, and the operator is told, until it starts or isn't lined up any more.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sp_engine::Command;
use sp_model::Uuid;

use crate::announcements::Announce;
use crate::i18n::tf;
use crate::{applog, file_name, media};

/// How long the last picture stays up when the file on screen has left its folder and nothing
/// is left to go on to.
pub const GRACE: Duration = Duration::from_secs(8);

/// How often the followed folders are looked at while a layer holds its last picture.
pub const LOOK_WHILE_HOLDING: Duration = Duration::from_secs(1);

/// What plays from a followed folder, as [`Following::step`] sees it.
pub struct Playing<'a> {
    /// The followed playlist's files that are in the media bin, in its order (the folder's, by
    /// name).
    pub ready: &'a [PathBuf],
    /// The file of the item on screen.
    pub shown: &'a Path,
    /// True for the playlist going round; false for a video of it shown on its own.
    pub round: bool,
    /// A video playing from its file, which can play on to the end of its pass.
    pub open: bool,
    /// The file lined up to take over at the end of the pass, if one is.
    pub lined_up: Option<&'a Path>,
    /// A file of the playlist is being read, to join it in a moment.
    pub coming: bool,
}

/// What a layer playing from a followed folder does next.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing changes.
    Stay,
    /// The file at this place in `ready` takes over at the end of the pass under way.
    LineUp(usize),
    /// The item at this place in `ready` goes on now.
    Now(usize),
    /// The file on screen has gone and there is nothing to go on to yet: its last picture stays
    /// up. Looked at again by then.
    Hold(Instant),
    /// Nothing came in time: the layer stops.
    Stop,
}

/// What a layer playing from a followed folder remembers between looks.
#[derive(Debug, Default)]
pub struct Following {
    /// The followed playlist it plays from.
    playlist: Option<Uuid>,
    /// The playlist had one item, or none, at the last look.
    alone: bool,
    /// Since when the file on screen has been gone with nothing to go on to.
    holding: Option<Instant>,
}

impl Following {
    /// Works out what the layer does now the folder's files are as `playing` says.
    pub fn step(&mut self, playlist: Uuid, playing: &Playing<'_>, now: Instant) -> Step {
        if self.playlist != Some(playlist) {
            *self = Following { playlist: Some(playlist), ..Following::default() };
        }
        let ready = playing.ready;
        // A file lined up that has left the folder too counts as none.
        let lined_up = playing.lined_up.filter(|file| ready.iter().any(|have| have == file));
        let was_alone = std::mem::replace(&mut self.alone, ready.len() <= 1);
        let step = match ready.iter().position(|file| file == playing.shown) {
            Some(at) => {
                let next = (at + 1) % ready.len();
                if lined_up.is_some() {
                    Step::Stay
                } else if playing.round && was_alone && ready.len() > 1 && playing.open && media::is_video(&ready[next])
                {
                    // The only video, looping, has company: it gives way at the end of its pass.
                    Step::LineUp(next)
                } else if playing.lined_up.is_some() && playing.open {
                    // The file lined up has left the folder: the one playing goes round again.
                    Step::LineUp(at)
                } else {
                    Step::Stay
                }
            }
            None if ready.is_empty() => {
                let until = *self.holding.get_or_insert(now) + GRACE;
                match (now < until, playing.coming) {
                    (true, _) => Step::Hold(until),
                    // A file being read gets the time it takes.
                    (false, true) => Step::Hold(now + Duration::from_millis(250)),
                    (false, false) => Step::Stop,
                }
            }
            None if lined_up.is_some() && playing.open => Step::Stay,
            None => {
                // The item after the one gone: the folder's files are in order by name.
                let next = ready.iter().filter(|file| file.as_path() < playing.shown).count() % ready.len();
                if playing.open && media::is_video(&ready[next]) { Step::LineUp(next) } else { Step::Now(next) }
            }
        };
        if !matches!(step, Step::Hold(_)) {
            self.holding = None;
        }
        step
    }

    /// True while the layer holds its last picture, waiting for a new file.
    pub fn holding(&self) -> bool {
        self.holding.is_some()
    }
}

/// A layer something from a followed folder can play on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnLayer {
    Media,
    Announcements,
}

impl OnLayer {
    const BOTH: [OnLayer; 2] = [OnLayer::Media, OnLayer::Announcements];

    /// The log's name for what happens on it.
    fn area(self) -> &'static str {
        match self {
            OnLayer::Media => "media",
            OnLayer::Announcements => "announcements",
        }
    }
}

/// What plays on a layer from a followed folder: the playlist that follows it, whether the
/// playlist goes round or one video of it is shown on its own, and the file on screen.
struct FromFolder {
    playlist: Uuid,
    round: bool,
    path: PathBuf,
}

impl crate::App {
    /// Keeps what plays from a followed folder going as its files come and go (see the
    /// module's notes), on the media layer and on the Announcements layer alike. Says when to
    /// look again.
    pub(crate) fn tick_followed(&mut self) -> Option<Instant> {
        let mut wake: Option<Instant> = None;
        for layer in OnLayer::BOTH {
            if let Some(at) = self.look_at_lined_up(layer) {
                wake = Some(wake.map_or(at, |soonest| soonest.min(at)));
            }
        }
        for layer in OnLayer::BOTH {
            let Some(from) = self.playing_from_folder(layer) else {
                self.following[layer as usize] = Following::default();
                continue;
            };
            let items = self.playlist_media(from.playlist);
            let ready: Vec<PathBuf> =
                items.iter().filter_map(|at| self.show.media().get(*at)).map(|item| item.path.clone()).collect();
            let coming = self.running.as_ref().is_some_and(|running| {
                let playlists = &running.home_state.media_playlists;
                playlists
                    .iter()
                    .find(|p| p.id == from.playlist)
                    .is_some_and(|playlist| self.folder_notes.coming.values().any(|(path, _)| playlist.has(path)))
            });
            let (open, lined_up) = self
                .layer_player(layer)
                .map_or((false, None), |player| (player.problem().is_none(), player.next_file().map(Path::to_owned)));
            let was_holding = self.following[layer as usize].holding();
            let playing = Playing {
                ready: &ready,
                shown: &from.path,
                round: from.round,
                open,
                lined_up: lined_up.as_deref(),
                coming,
            };
            match self.following[layer as usize].step(from.playlist, &playing, Instant::now()) {
                Step::Stay => {}
                Step::Hold(until) => {
                    if !was_holding {
                        applog::event(
                            layer.area(),
                            &format!(
                                "{} left its followed folder and nothing is there to go on to: its last picture \
                                 stays up for {} seconds in case a new file comes",
                                file_name(&from.path),
                                GRACE.as_secs()
                            ),
                        );
                    }
                    wake = Some(wake.map_or(until, |at| at.min(until)));
                }
                Step::LineUp(at) => {
                    let lined = self.layer_player(layer).map(|player| player.play_next(&ready[at]));
                    match lined {
                        Some(Ok(())) => applog::event(
                            layer.area(),
                            &format!("{} takes over at the end of the pass", file_name(&ready[at])),
                        ),
                        Some(Err(problem)) => {
                            applog::problem(layer.area(), &problem);
                            self.followed_now(layer, &from, items[at], at);
                        }
                        None => self.followed_now(layer, &from, items[at], at),
                    }
                }
                Step::Now(at) => self.followed_now(layer, &from, items[at], at),
                Step::Stop => {
                    self.stop_followed(layer, &from);
                    continue;
                }
            }
            if from.round {
                self.loop_as_playlist_asks(layer);
            }
        }
        wake
    }

    /// The file lined up to follow the video on the layer, looked at again (see
    /// `video_wait::LinedUp`): once it has waited the most a video's first picture is waited for,
    /// the video playing goes round meanwhile and the operator is told, naming it; that notice is
    /// taken back as soon as it isn't true (the file has given its picture, another is lined up
    /// in its place, or the layer has gone on to something else). Says when to look again.
    fn look_at_lined_up(&mut self, layer: OnLayer) -> Option<Instant> {
        let limit = crate::video_wait::limit();
        let (next, playing) = self.layer_player(layer).map_or((None, None), |player| {
            let next = player.next_file().map(|file| (file.to_owned(), player.next_waiting()));
            (next, Some(file_name(player.path())))
        });
        let look = self.lined_up_wait[layer as usize]
            .look(next.as_ref().map(|(file, waiting)| (file.as_path(), *waiting)), limit);
        match look {
            crate::video_wait::LinedUpLook::Quiet(soon) => soon.map(|soon| Instant::now() + soon),
            crate::video_wait::LinedUpLook::Overdue => {
                let (file, _) = next?;
                let name = file_name(&file);
                let playing = playing.unwrap_or_default();
                if let Some(player) = self.layer_player(layer) {
                    player.go_round_until_next();
                }
                applog::event(
                    layer.area(),
                    &format!(
                        "no picture of {name}, lined up to follow {playing}, came in {:.1} s, the most a video's \
                         first picture is waited for: {playing} goes round meanwhile, and {name} takes over at the \
                         end of a pass once its first picture has come",
                        limit.as_secs_f64()
                    ),
                );
                let seconds = limit.as_secs_f64().round().max(1.0) as u64;
                let notice = format!(
                    "{name}: {}",
                    crate::i18n::tn(
                        "notices.media.next_not_started",
                        seconds,
                        "this video hasn\u{2019}t started after {n} second, so {playing} goes on playing. It will \
                         take over once it starts.",
                        "this video hasn\u{2019}t started after {n} seconds, so {playing} goes on playing. It will \
                         take over once it starts.",
                        &[("playing", &playing)],
                    )
                );
                self.lined_up_wait[layer as usize].told(file, notice.clone());
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.trouble(notice);
                }
                None
            }
            crate::video_wait::LinedUpLook::TakeBack(notice, started) => {
                applog::event(
                    layer.area(),
                    if started {
                        "the file lined up has given its first picture after all: the notice about it is taken back"
                    } else {
                        "the file the notice said hadn't started isn't lined up any more: the notice is taken back"
                    },
                );
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.withdraw(&notice);
                }
                None
            }
        }
    }

    /// How often the followed folders are looked at: every few seconds, and every second while
    /// a layer holds its last picture waiting for a new file.
    pub(crate) fn folder_look_every(&self) -> Duration {
        if self.following.iter().any(Following::holding) { LOOK_WHILE_HOLDING } else { Duration::from_secs(5) }
    }

    /// A video that couldn't be opened because its file has just been taken out of a followed
    /// folder (its playlist, not having looked since, still lists it): the folders are looked at
    /// now, and the layer goes on from what is left (`tick_followed`) in a moment, with nothing
    /// to tell the operator. False for anything else.
    pub(crate) fn gone_from_followed_folder(&mut self, path: &Path) -> bool {
        let listed = self.running.as_ref().is_some_and(|running| {
            running.home_state.media_playlists.iter().any(|playlist| playlist.folder.is_some() && playlist.has(path))
        });
        // Asked once, after the file couldn't be opened, never on every look at the layer.
        if !listed || path.exists() {
            return false;
        }
        applog::event("media", &format!("{} has just left its followed folder", file_name(path)));
        self.check_folders(true);
        true
    }

    /// True when the media playlist follows a folder.
    pub(crate) fn follows_folder(&self, playlist: Uuid) -> bool {
        self.running.as_ref().is_some_and(|running| {
            running.home_state.media_playlists.iter().any(|p| p.id == playlist && p.folder.is_some())
        })
    }

    /// What plays on the layer from a followed folder, if anything does: an item of a media
    /// playlist that follows one, going round; or a video the folder brought in, shown on its
    /// own.
    fn playing_from_folder(&self, layer: OnLayer) -> Option<FromFolder> {
        let (item, playlist) = match layer {
            OnLayer::Media => {
                let index = self.show.live_media()?;
                let cycle = self.media_cycle.as_ref().filter(|cycle| cycle.shown == index).map(|c| c.playlist);
                (self.show.media().get(index)?, cycle)
            }
            OnLayer::Announcements => {
                let on = self.announcing.as_ref()?;
                let item = on.item.and_then(|id| self.show.media().iter().find(|item| item.id == id))?;
                match on.what {
                    Announce::MediaPlaylist(id) => (item, Some(id)),
                    Announce::Media(_) => (item, None),
                    Announce::Song(_) => return None,
                }
            }
        };
        if let Some(playlist) = playlist {
            return self.follows_folder(playlist).then(|| FromFolder {
                playlist,
                round: true,
                path: item.path.clone(),
            });
        }
        if !self.followed_items.contains(&item.id) || !media::is_video(&item.path) {
            return None;
        }
        let folder = item.path.parent()?;
        let playlists = &self.running.as_ref()?.home_state.media_playlists;
        let playlist = playlists.iter().find(|p| p.folder.as_deref() == Some(folder))?;
        Some(FromFolder { playlist: playlist.id, round: false, path: item.path.clone() })
    }

    /// The video playing on the layer, if one is.
    fn layer_player(&mut self, layer: OnLayer) -> Option<&mut sp_video::Player> {
        match layer {
            OnLayer::Media => {
                let live = self.show.live_media().and_then(|index| self.show.media().get(index)).map(|item| item.id);
                self.video.as_mut().filter(|(id, _)| Some(*id) == live).map(|(_, player)| player)
            }
            OnLayer::Announcements => self.announcing.as_mut()?.video.as_mut(),
        }
    }

    /// Puts the media bin item at `index` (at the place `at` in the playlist) on the layer now,
    /// in place of the one from a followed folder, the way the playlist's next item goes on.
    fn followed_now(&mut self, layer: OnLayer, from: &FromFolder, index: usize, at: usize) {
        let Some(item) = self.show.media().get(index).cloned() else { return };
        applog::event(layer.area(), &format!("{} goes on in place of {}", item.name, file_name(&from.path)));
        match layer {
            OnLayer::Media => {
                if from.round
                    && let Some(cycle) = self.media_cycle.as_mut()
                {
                    *cycle = crate::MediaCycle { playlist: cycle.playlist, at, since: Instant::now(), shown: index };
                }
                self.apply(vec![Command::ShowMedia(index)]);
                if from.round {
                    self.read_ahead_in_cycle();
                }
            }
            OnLayer::Announcements => {
                let what = match self.announcing.as_ref().map(|on| on.what) {
                    Some(what @ Announce::MediaPlaylist(_)) => what,
                    _ => Announce::Media(item.id),
                };
                self.show_media_announcement(what, if from.round { at } else { 0 }, index);
            }
        }
    }

    /// A file lined up by `tick_followed` has taken over on the layer, at the end of the pass:
    /// its media bin item is the one on screen now. Nothing is started again on the screens;
    /// the window and `sp status` name the new item.
    pub(crate) fn followed_took_over(&mut self, layer: OnLayer, path: &Path) {
        let Some(index) = self.show.media().iter().position(|item| item.path == path) else { return };
        let (id, name) = (self.show.media()[index].id, self.show.media()[index].name.clone());
        let now = Instant::now();
        match layer {
            OnLayer::Media => {
                let cycle = self.media_cycle.as_ref().filter(|cycle| Some(cycle.shown) == self.show.live_media());
                let at = cycle.and_then(|cycle| self.playlist_media(cycle.playlist).iter().position(|i| *i == index));
                if let (Some(cycle), Some(at)) = (self.media_cycle.as_mut(), at) {
                    *cycle = crate::MediaCycle { playlist: cycle.playlist, at, since: now, shown: index };
                }
                // The player stays as it is: it is this item's now.
                if let Some((playing, _)) = self.video.as_mut() {
                    *playing = id;
                }
                if let Some(live) = self.running.as_mut().and_then(|running| running.home_state.video_live.as_mut()) {
                    live.name = name;
                }
                self.apply(vec![Command::ShowMedia(index)]);
            }
            OnLayer::Announcements => {
                let Some(what) = self.announcing.as_ref().map(|on| on.what) else { return };
                let (what, at) = match what {
                    Announce::MediaPlaylist(playlist) => {
                        (what, self.playlist_media(playlist).iter().position(|i| *i == index).unwrap_or(0))
                    }
                    Announce::Media(_) | Announce::Song(_) => (Announce::Media(id), 0),
                };
                let title = self.media_announcement_title(what, &name);
                if let Some(on) = self.announcing.as_mut() {
                    on.what = what;
                    on.item = Some(id);
                    on.at = at;
                    on.since = now;
                }
                if let Some(running) = self.running.as_mut() {
                    running.home_state.announcement_live = Some((what, title));
                    running.home_window.request_redraw();
                }
            }
        }
    }

    /// Nothing came in time to take the place of the file that left: the layer stops, and the
    /// operator is told why.
    fn stop_followed(&mut self, layer: OnLayer, from: &FromFolder) {
        let playlist = self
            .running
            .as_ref()
            .and_then(|running| running.home_state.media_playlists.iter().find(|p| p.id == from.playlist))
            .map(|playlist| playlist.name.clone())
            .unwrap_or_default();
        let file = file_name(&from.path);
        let said = match layer {
            OnLayer::Media => tf(
                "notices.followed.media_stopped",
                "The media layer stopped: {file} left the folder that {playlist} follows, and nothing came in its place.",
                &[("file", &file), ("playlist", &playlist)],
            ),
            OnLayer::Announcements => tf(
                "notices.followed.announcements_stopped",
                "The Announcements layer stopped: {file} left the folder that {playlist} follows, and nothing came in its place.",
                &[("file", &file), ("playlist", &playlist)],
            ),
        };
        applog::event(layer.area(), &said);
        match layer {
            OnLayer::Media => {
                if from.round {
                    self.stop_media_playlist();
                }
                self.apply(vec![Command::Clear(sp_engine::Layer::Media)]);
            }
            OnLayer::Announcements => self.apply(vec![Command::Clear(sp_engine::Layer::Announcements)]),
        }
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.trouble(said);
            running.home_window.request_redraw();
        }
    }

    /// A video of a media playlist going round loops while it is the playlist's only item, and
    /// plays to its end when there are others, as the playlist's items come and go.
    fn loop_as_playlist_asks(&mut self, layer: OnLayer) {
        match layer {
            OnLayer::Media => {
                let Some(item) = self.show.live_media().and_then(|index| self.show.media().get(index)).cloned() else {
                    return;
                };
                let loops = self.loops_now(&item);
                if let Some((id, player)) = self.video.as_mut()
                    && *id == item.id
                    && player.looping() != loops
                {
                    player.set_looping(loops);
                    if let Some(live) = self.running.as_mut().and_then(|running| running.home_state.video_live.as_mut())
                    {
                        live.looping = loops;
                    }
                }
            }
            OnLayer::Announcements => {
                let loops = self.announcing_items() <= 1;
                if let Some(player) = self.announcing.as_mut().and_then(|on| on.video.as_mut())
                    && player.looping() != loops
                {
                    player.set_looping(loops);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of the followed folder.
    fn file(name: &str) -> PathBuf {
        PathBuf::from("/church/Canva Sync").join(name)
    }

    const OLD: &str = "Sunday 6.00 AM - Announcements 11-22-26.mp4";
    const NEW: &str = "Sunday 9.43 AM - Announcements 11-22-26.mp4";

    /// The layer's look at its playlist: what is ready, the file on screen and what its video
    /// player is doing.
    fn look<'a>(ready: &'a [PathBuf], shown: &'a Path, lined_up: Option<&'a Path>) -> Playing<'a> {
        Playing { ready, shown, round: true, open: true, lined_up, coming: false }
    }

    #[test]
    fn a_new_video_then_the_old_one_deleted_hands_over_at_the_end_of_the_pass() {
        let (playlist, start) = (Uuid::new_v4(), Instant::now());
        let (old, new) = (file(OLD), file(NEW));
        let mut following = Following::default();
        assert_eq!(
            following.step(playlist, &look(std::slice::from_ref(&old), &old, None), start),
            Step::Stay,
            "one video loops"
        );
        // The new video has been read and joins the playlist: it takes over at the end of the
        // old one's pass.
        let both = [old.clone(), new.clone()];
        assert_eq!(following.step(playlist, &look(&both, &old, None), start), Step::LineUp(1));
        assert_eq!(following.step(playlist, &look(&both, &old, Some(&new)), start), Step::Stay, "lined up once");
        // The old one is deleted before its pass ends: the new one is lined up already.
        let only_new = [new.clone()];
        assert_eq!(following.step(playlist, &look(&only_new, &old, Some(&new)), start), Step::Stay);
        assert!(!following.holding(), "the old one's going leaves nothing blank to wait out");
        // It has taken over, and loops on its own.
        assert_eq!(following.step(playlist, &look(&only_new, &new, None), start), Step::Stay);
    }

    #[test]
    fn a_new_video_and_the_old_one_deleted_in_the_same_look_hold_the_picture_until_the_new_one_is_read() {
        let (playlist, start) = (Uuid::new_v4(), Instant::now());
        let (old, new) = (file(OLD), file(NEW));
        let mut following = Following::default();
        following.step(playlist, &look(std::slice::from_ref(&old), &old, None), start);
        // The folder lists only the new file, which is still being read: nothing is ready.
        let reading = Playing { coming: true, ..look(&[], &old, None) };
        assert_eq!(following.step(playlist, &reading, start), Step::Hold(start + GRACE));
        assert!(following.holding());
        // Read: the old video plays to the end of its pass (its file is still open), then the
        // new one takes over.
        let only_new = [new.clone()];
        let later = start + Duration::from_millis(400);
        assert_eq!(following.step(playlist, &look(&only_new, &old, None), later), Step::LineUp(0));
        assert!(!following.holding());
        // A video that can't play on (its file couldn't be read any more) gives way at once.
        let mut again = Following::default();
        let closed = Playing { open: false, ..look(&only_new, &old, None) };
        assert_eq!(again.step(playlist, &closed, later), Step::Now(0));
    }

    #[test]
    fn the_old_video_deleted_first_holds_its_last_picture_and_a_new_one_within_the_grace_takes_over() {
        let (playlist, start) = (Uuid::new_v4(), Instant::now());
        let (old, new) = (file(OLD), file(NEW));
        let mut following = Following::default();
        following.step(playlist, &look(std::slice::from_ref(&old), &old, None), start);
        assert_eq!(following.step(playlist, &look(&[], &old, None), start), Step::Hold(start + GRACE));
        // Looked at again: still nothing, and the time it holds for doesn't start again.
        let three = start + Duration::from_secs(3);
        assert_eq!(following.step(playlist, &look(&[], &old, None), three), Step::Hold(start + GRACE));
        // The new one comes within the grace and takes over at the end of the pass.
        let five = start + Duration::from_secs(5);
        let only_new = [new.clone()];
        assert_eq!(following.step(playlist, &look(&only_new, &old, None), five), Step::LineUp(0));
        assert_eq!(following.step(playlist, &look(&only_new, &old, Some(&new)), five), Step::Stay);
    }

    #[test]
    fn a_video_deleted_with_nothing_left_stops_once_the_grace_is_over() {
        let (playlist, start) = (Uuid::new_v4(), Instant::now());
        let old = file(OLD);
        let mut following = Following::default();
        following.step(playlist, &look(std::slice::from_ref(&old), &old, None), start);
        assert_eq!(following.step(playlist, &look(&[], &old, None), start), Step::Hold(start + GRACE));
        let almost = start + GRACE - Duration::from_millis(10);
        assert_eq!(following.step(playlist, &look(&[], &old, None), almost), Step::Hold(start + GRACE));
        // A file still being read when the time is up gets the time it takes.
        let over = start + GRACE;
        let reading = Playing { coming: true, ..look(&[], &old, None) };
        assert_eq!(following.step(playlist, &reading, over), Step::Hold(over + Duration::from_millis(250)));
        // It couldn't be read: nothing is left, and the layer stops.
        assert_eq!(following.step(playlist, &look(&[], &old, None), over), Step::Stop);
        assert!(!following.holding());
    }

    #[test]
    fn with_several_items_a_new_one_joins_and_one_gone_gives_way_to_the_next() {
        let (playlist, start) = (Uuid::new_v4(), Instant::now());
        let (a, b, c, d) = (file("a.mp4"), file("b.mp4"), file("c.png"), file("d.mp4"));
        let mut following = Following::default();
        let three = [a.clone(), b.clone(), c.clone()];
        assert_eq!(following.step(playlist, &look(&three, &b, None), start), Step::Stay);
        // A new file joins: it plays when its turn comes, nothing is lined up for it.
        let four = [a.clone(), b.clone(), c.clone(), d.clone()];
        assert_eq!(following.step(playlist, &look(&four, &b, None), start), Step::Stay);
        // b is deleted while it plays: the item after it, a picture, goes on at once.
        let no_b = [a.clone(), c.clone(), d.clone()];
        assert_eq!(following.step(playlist, &look(&no_b, &b, None), start), Step::Now(1));
        // d is deleted while it plays: round to the first, a video, lined up for the end of
        // the pass.
        let no_d = [a.clone(), c.clone()];
        assert_eq!(following.step(playlist, &look(&no_d, &d, None), start), Step::LineUp(0));
        // A file lined up that is deleted too before it takes over: the one playing goes round
        // again instead.
        assert_eq!(following.step(playlist, &look(std::slice::from_ref(&a), &a, Some(&d)), start), Step::LineUp(0));
    }

    #[test]
    fn a_video_shown_on_its_own_keeps_playing_as_files_come_and_gives_way_when_it_goes() {
        let (playlist, start) = (Uuid::new_v4(), Instant::now());
        let (old, new) = (file(OLD), file(NEW));
        let mut following = Following::default();
        let one = [old.clone()];
        assert_eq!(following.step(playlist, &Playing { round: false, ..look(&one, &old, None) }, start), Step::Stay);
        let both = [old.clone(), new.clone()];
        let joined = Playing { round: false, ..look(&both, &old, None) };
        assert_eq!(following.step(playlist, &joined, start), Step::Stay, "a new file doesn't take its place");
        let only_new = [new.clone()];
        let gone = Playing { round: false, ..look(&only_new, &old, None) };
        assert_eq!(following.step(playlist, &gone, start), Step::LineUp(0));
    }
}
