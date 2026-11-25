//! What the app publishes for remote pages, from the data it already keeps (never by looking at
//! the window), and the slide pictures it draws for them with the window's own painter.
//!
//! Times are milliseconds since 1970 on the server's clock (the snapshot says what that clock
//! read when it was sent): when a timer was started and what it had counted before, when a
//! video or a track would have been at its start, when the slide on screen goes on by itself.
//! Nothing that ticks is ever sent: the page works the time shown out for itself.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use sp_model::{Slide, Theme, Uuid};

use super::server::{Drawn, Rgba, Thumbed};
use crate::i18n::Language;
use crate::service::Services;

/// The state is published at most this often while pages are connected.
pub const PUBLISH_EVERY: Duration = Duration::from_millis(50);
/// The parts that change seldom (the library, the playlists, the media bin, the theme
/// versions) at most this often.
pub const SLOW_EVERY: Duration = Duration::from_secs(1);

/// The app's own part: the language its words are in now (the one chosen, or the computer's
/// when the setting follows it: never "auto"), and how that language writes numbers and
/// quotation marks (`locales/<language>/format.json`).
pub fn app_part(language: Language) -> Value {
    json!({"language": language.code(), "format": crate::i18n::number_format(language)})
}

/// A built-in name as pages are sent it, under `keys` (its name, the mark, the label): the name
/// kept, in English as the files keep it; `true`; and the name as the window shows it now, only
/// when that differs. The page shows the label, else the name.
fn put_built_in(object: &mut Map<String, Value>, keys: [&str; 3], name: &str, shown: &str) {
    let [name_key, mark, label] = keys;
    object.insert(name_key.to_owned(), json!(name));
    object.insert(mark.to_owned(), json!(true));
    if shown == name {
        object.remove(label);
    } else {
        object.insert(label.to_owned(), json!(shown));
    }
}

/// A playlist's name as pages are sent it, put in `object`: its own name; but the untouched
/// first playlist of a first start ([`Services::first_is_the_apps`]) by the app's name for it,
/// "Service", `built_in`, with the label the window shows ("Culto"). Taken from the mark, not
/// from the name kept, which follows the language a moment later.
pub fn put_playlist_name(services: &Services, at: usize, object: &mut Map<String, Value>) {
    let Some(list) = services.list.get(at) else { return };
    if at == 0 && services.first_is_the_apps() {
        let (_, english) = crate::service::FIRST_PLAYLIST;
        put_built_in(object, ["name", "built_in", "label"], english, crate::service::first_playlist_name());
    } else {
        object.insert("name".to_owned(), json!(list.name));
    }
}

/// The `playlists` part: each playlist with its number, name, how many items, rev and whether it
/// is in front.
pub fn playlists_part(services: &Services) -> Value {
    let front = services.front_at();
    let lists: Vec<Value> = services
        .list
        .iter()
        .enumerate()
        .map(|(at, list)| {
            let mut entry = Map::new();
            entry.insert("number".to_owned(), json!(at + 1));
            put_playlist_name(services, at, &mut entry);
            entry.insert("items".to_owned(), json!(list.items.len()));
            entry.insert("rev".to_owned(), json!(list.rev()));
            entry.insert("front".to_owned(), json!(at == front));
            Value::Object(entry)
        })
        .collect();
    json!(lists)
}

/// Beside `live.main_theme`: when the main theme is the built-in one under the app's name for it,
/// `main_theme_built_in` and, as the window shows it ("Incluido"), `main_theme_label`.
pub fn put_main_theme(theme: Option<&Theme>, live: &mut Map<String, Value>) {
    if let Some(theme) = theme.filter(|theme| (theme.id, theme.name.as_str()) == sp_model::BUILTIN_THEME) {
        let shown = crate::theme_store::shown_theme_name(theme);
        put_built_in(live, ["main_theme", "main_theme_built_in", "main_theme_label"], &theme.name, &shown);
    }
}

/// Whether the seldom-changing parts go with this publish: when they are due, for a page just
/// connected (`force`), and when the app's language changed since the last publish, so every part
/// with a label goes in the same change as the `app` part.
fn slow_now(force: bool, slow_due: Instant, now: Instant, language_changed: bool) -> bool {
    force || slow_due <= now || language_changed
}

/// What makes a slide's picture what it is, cached per theme: hashing a theme's JSON each time
/// would cost more than drawing.
#[derive(Default)]
pub struct Looks {
    themes: Vec<(Arc<Theme>, u64)>,
    /// When a playing video or track would have been at its start, as last published; kept
    /// while a new reading is within a quarter second of it, so a position read a moment later
    /// doesn't make a change.
    anchors: [Option<u64>; 2],
}

const VIDEO: usize = 0;
const AUDIO: usize = 1;

fn hash_of(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

impl Looks {
    fn theme(&mut self, theme: &Arc<Theme>) -> u64 {
        if let Some((_, hash)) = self.themes.iter().find(|(kept, _)| Arc::ptr_eq(kept, theme)) {
            return *hash;
        }
        let hash = hash_of(&serde_json::to_string(&**theme).unwrap_or_default());
        if self.themes.len() >= 32 {
            self.themes.remove(0);
        }
        self.themes.push((theme.clone(), hash));
        hash
    }

    /// When something playing `position` in would have been at its start, steady from reading to
    /// reading.
    fn anchor(&mut self, which: usize, now_ms: u64, position: Duration) -> u64 {
        let anchor = now_ms.saturating_sub(position.as_millis() as u64);
        match self.anchors[which] {
            Some(kept) if kept.abs_diff(anchor) < 250 => kept,
            _ => {
                self.anchors[which] = Some(anchor);
                anchor
            }
        }
    }
}

impl crate::App {
    /// What makes a slide's picture what it is (its objects, its theme, the version it is drawn
    /// in, the media behind it and whether that picture has been read yet), as a short word for
    /// its address: the address changes when the picture would.
    pub(crate) fn slide_look(&mut self, at: usize, slide: &Slide, version: Option<Uuid>) -> String {
        let theme = self.theme_of(at).clone();
        let theme_hash = self.remote.looks.theme(&theme);
        let media = crate::home::media_of(slide).map(|id| {
            let fit = self.show.media().iter().find(|item| item.id == id).map(|item| format!("{:?}", item.fit));
            (id, self.bin_pictures.small(id).is_some(), fit)
        });
        let slide_json = serde_json::to_string(slide).unwrap_or_default();
        format!("{:016x}", hash_of(&format!("{slide_json}|{theme_hash}|{version:?}|{media:?}")))
    }

    /// The address of a slide's picture for pages.
    pub(crate) fn thumb_address(&mut self, at: usize, slide: &Slide, version: Option<Uuid>) -> String {
        let look = self.slide_look(at, slide, version);
        let version = version.map_or_else(|| "usual".to_owned(), |v| v.to_string());
        format!("/thumb/{}/{version}/{look}.jpg", slide.id)
    }

    /// A media bin item's picture's look: its id, how it fills, and whether it has been read.
    fn media_look(&self, id: Uuid) -> String {
        let fit = self.show.media().iter().find(|item| item.id == id).map(|item| format!("{:?}", item.fit));
        format!("{:016x}", hash_of(&format!("media|{id}|{fit:?}|{}", self.bin_pictures.small(id).is_some())))
    }

    /// Draws a slide (or a media bin item's picture), by id, as the window's thumbnails are drawn,
    /// with the window's own painter, into a texture of its own; how to read it back goes to the
    /// server, which does so on its own thread. Only when `asked` is the look it has now: any
    /// other look (an old one, or one made up) is answered with the current look and nothing is
    /// drawn, so an address can't make the window's thread work.
    pub(crate) fn remote_draw(&mut self, id: Uuid, version: Option<Uuid>, asked: &str) -> Thumbed {
        let found = self.show.presentations().iter().enumerate().find_map(|(at, song)| {
            song.slides().find(|(_, slide)| slide.id == id).map(|(_, slide)| (at, slide.clone()))
        });
        let look = match &found {
            Some((at, slide)) => self.slide_look(*at, slide, version),
            None if self.show.media().iter().any(|item| item.id == id) => self.media_look(id),
            None => return Thumbed::Missing,
        };
        if look != asked {
            return Thumbed::Moved(look);
        }
        match self.draw_for_page(id, version, found) {
            Some(drawn) => Thumbed::Drawn(drawn),
            None => Thumbed::Missing,
        }
    }

    /// The drawing of [`App::remote_draw`], for a look already known to be the current one.
    fn draw_for_page(&mut self, id: Uuid, version: Option<Uuid>, found: Option<(usize, Slide)>) -> Option<Drawn> {
        let (theme, shown, media) = match &found {
            Some((at, slide)) => {
                let theme = self.theme_of(*at).clone();
                let shown = version.and_then(|v| theme.slide_in_version(slide, v)).unwrap_or_else(|| slide.clone());
                let media = crate::home::media_of(slide).and_then(|media| {
                    let item = self.show.media().iter().find(|item| item.id == media)?;
                    Some((media, item.fit))
                });
                (theme, Some(shown), media)
            }
            None => {
                self.bin_pictures.small(id)?;
                (self.theme.clone(), None, Some((id, sp_model::ImageFit::Fill)))
            }
        };
        let running = self.running.as_mut()?;
        let picture = media.and_then(|(media, fit)| Some((self.bin_pictures.small(media)?, fit)));
        let layers = sp_render::Layers {
            media_fit: picture.map_or(sp_model::ImageFit::Fill, |(_, fit)| fit),
            media: picture.map(|(picture, _)| picture),
            slide: shown.as_ref(),
            overlay: None,
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
            announcement: None,
        };
        let texture = running.thumbs.draw_alone(&running.gpu, &layers, &theme)?;
        let (device, queue) = (running.gpu.device.clone(), running.gpu.queue.clone());
        Some(Drawn {
            read: Box::new(move || {
                let pixels = sp_render::read_pixels_gently(&device, &queue, &texture, Duration::from_secs(2)).ok()?;
                Some(Rgba { width: texture.width(), height: texture.height(), pixels })
            }),
        })
    }

    /// The state as pages are sent it, part by part, each written as JSON once.
    /// With `slow`, the parts that change seldom and cost the most in a large library too (the
    /// library, the playlists, the media bin, the theme versions), which are published at most
    /// every [`SLOW_EVERY`].
    pub(crate) fn remote_parts(&mut self, slow: bool) -> Vec<(&'static str, String)> {
        let mut parts = vec![
            ("app", app_part(crate::i18n::current())),
            ("live", self.remote_live()),
            ("playlist", self.remote_playlist()),
            ("media", self.remote_media()),
            ("audio", self.remote_audio()),
            ("timers", self.remote_timers()),
            ("messages", self.ctl_messages().data),
            ("props", self.ctl_props().data),
            ("macros", self.ctl_macros().data),
            ("screens", self.remote_screens()),
            ("notices", self.remote_notices()),
        ];
        if slow {
            parts.extend([
                ("playlists", self.remote_playlists()),
                ("library", self.remote_library()),
                ("bin", self.remote_bin()),
                ("versions", self.remote_versions()),
            ]);
        }
        // The computer's folders stay on it: any path in the state is cut to its file name.
        parts
            .into_iter()
            .map(|(name, mut value)| {
                super::json_without_paths(&mut value);
                (name, value.to_string())
            })
            .collect()
    }

    /// What is on screen, and the switches beside it: `sp status` less what ticks, and more.
    fn remote_live(&mut self) -> Value {
        let mut live = self.ctl_status().data;
        if let Some(said) = live.as_object_mut() {
            said.remove("timers");
            said.remove("audio");
            said.remove("token");
        }
        let clock = self.remote.clock;
        let on_screen = self.show.live();
        let version_on = self.running.as_ref().and_then(|r| r.home_state.version_switch.version());
        let slide = on_screen.and_then(|at| {
            let slide = self.show.presentations().get(at.presentation)?.slide(at.slide)?.1.clone();
            Some((at.presentation, slide))
        });
        let (slide_id, thumb) = match slide {
            Some((at, slide)) => (Some(slide.id), Some(self.thumb_address(at, &slide, version_on))),
            None => (None, None),
        };
        let advance_at = self.live_since.and_then(|(_, since)| {
            let seconds = self.show.live_slide().and_then(|(_, slide)| slide.advance).filter(|s| *s > 0.0)?;
            Some(clock.ms(since + Duration::from_secs_f32(seconds)))
        });
        let state = self.running.as_ref().map(|r| &r.home_state);
        let extra = json!({
            "build": self.show.build_step(),
            "slide_id": slide_id,
            "thumb": thumb,
            "live_item": state.and_then(|s| s.live_item).map(|at| at + 1),
            "advance_at": advance_at,
            "layers": {
                "slides": on_screen.is_some(),
                "media": self.show.live_media().is_some(),
                "props": state.is_some_and(|s| !s.props_live.is_empty()),
                "messages": state.is_some_and(|s| s.message_live.is_some()),
                "audio": state.is_some_and(|s| s.audio_playing.track.is_some() && !s.audio_playing.paused),
                "announcements": self.announcing.is_some(),
            },
            "test_pattern": state.and_then(|s| s.test_pattern).map(|pattern| format!("{pattern:?}")),
            "stage_message": state.map(|s| json!({"text": s.stage_message, "on": s.stage_message_live})),
            "captions": on_screen.is_some_and(|at| at.presentation == self.voxlive_slot),
            "version_id": version_on,
        });
        if let (Some(live), Value::Object(extra)) = (live.as_object_mut(), extra) {
            live.extend(extra);
            // Built-in names with the words the window shows for them: the main theme, and the
            // playlist in front while it is the untouched first playlist (which has no items, so
            // nothing on screen came from it: `live.live.playlist` never names it).
            put_main_theme(self.themes.get(self.songs_theme), live);
            if let Some(services) = self.running.as_ref().map(|r| &r.home_state.services)
                && let Some(Value::Object(playlist)) = live.get_mut("playlist")
            {
                put_playlist_name(services, services.front_at(), playlist);
            }
        }
        live
    }

    /// The playlist in front, with its items as `sp playlist` lists them, and its rev (for
    /// `--aim ITEM --rev REV`).
    fn remote_playlist(&self) -> Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Value::Null };
        let front = state.services.front();
        let at = state.services.list.iter().position(|p| p.name == front.name).unwrap_or(0);
        // Each item's text in the window's language, as everything the app says to a page is.
        let (_, items) = self.playlist_listing(at, true);
        let mut part = Map::new();
        put_playlist_name(&state.services, at, &mut part);
        part.insert("number".to_owned(), json!(at + 1));
        part.insert("rev".to_owned(), json!(front.rev()));
        part.insert("live_item".to_owned(), json!(state.live_item.map(|at| at + 1)));
        part.insert("items".to_owned(), json!(items));
        Value::Object(part)
    }

    fn remote_playlists(&self) -> Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Value::Null };
        playlists_part(&state.services)
    }

    /// The library in short: how many songs, and a rev that changes with them. The songs come on
    /// demand (`songs`, as `sp` has them).
    fn remote_library(&self) -> Value {
        let state = self.running.as_ref().map(|r| &r.home_state);
        json!({
            "songs": self.listed_songs().len(),
            "rev": format!("{}-{}", self.show.version(), state.map_or(0, |s| s.removed_songs.len())),
            "libraries": self.ctl_libraries().data,
        })
    }

    /// The media bin, each item with its poster's address, and the media playlists.
    fn remote_bin(&mut self) -> Value {
        let items: Vec<(Uuid, String, sp_model::ImageFit, bool, bool)> = self
            .show
            .media()
            .iter()
            .map(|item| (item.id, item.name.clone(), item.fit, item.looping, crate::media::is_video(&item.path)))
            .collect();
        let entries: Vec<Value> = items
            .into_iter()
            .enumerate()
            .map(|(at, (id, name, fit, looping, video))| {
                let poster = format!("/thumb/{id}/usual/{}.jpg", self.media_look(id));
                json!({"number": at + 1, "id": id, "name": name, "kind": if video { "video" } else { "picture" },
                       "fit": format!("{fit:?}").to_lowercase(), "loop": looping, "poster": poster})
            })
            .collect();
        let playlists: Vec<Value> = self
            .running
            .as_ref()
            .map(|r| r.home_state.media_playlists.iter().map(|p| json!({"id": p.id, "name": p.name})).collect())
            .unwrap_or_default();
        json!({"items": entries, "playlists": playlists})
    }

    /// What plays on the media layer: the video (when it would have been at its start, or where it
    /// is paused), the media playlist playing and its step, the announcements.
    fn remote_media(&mut self) -> Value {
        let now = super::server::now_ms();
        let state = self.running.as_ref().map(|r| &r.home_state);
        let showing =
            self.show.live_media().and_then(|at| self.show.media().get(at)).map(|item| (item.id, item.name.clone()));
        let video = state.and_then(|s| s.video_live.clone());
        let video = video.map(|video| {
            let position = Duration::from_secs_f64(video.position.max(0.0));
            let started_at = video.playing.then(|| self.remote.looks.anchor(VIDEO, now, position));
            json!({
                "name": video.name, "playing": video.playing, "loop": video.looping, "mute": video.muted,
                "duration_ms": video.duration.map(|d| (d * 1000.0) as u64),
                "started_at": started_at,
                "position_ms": (!video.playing).then_some(position.as_millis() as u64),
            })
        });
        let cycle = self.media_cycle.as_ref().map(|cycle| (cycle.playlist, cycle.at, cycle.since));
        let state = self.running.as_ref().map(|r| &r.home_state);
        let media_playlist = cycle.map(|(id, at, since)| {
            let name = state.and_then(|s| s.media_playlists.iter().find(|p| p.id == id)).map(|p| p.name.clone());
            json!({"id": id, "name": name, "step": at + 1, "since": self.remote.clock.ms(since)})
        });
        let announcements = self.announcing.as_ref().map(|on| {
            json!({
                "words": self.announcing_words(),
                "step": on.at + 1,
                "item": on.item,
                "since": self.remote.clock.ms(on.since),
                "on_slide_screens": on.on_slide_screens,
            })
        });
        json!({
            "showing": showing.map(|(id, name)| json!({"id": id, "name": name})),
            "video": video,
            "media_playlist": media_playlist,
            "announcements": announcements,
        })
    }

    /// The audio player: the track, playing or paused, where (when it would have been at its
    /// start, or where it is paused), the volume and the playlist's switches.
    fn remote_audio(&mut self) -> Value {
        let now = super::server::now_ms();
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Value::Null };
        let playing = state.audio_playing;
        let track = playing.track.and_then(|id| state.audio.track(id)).map(|t| t.name.clone());
        let (shuffle, through, crossfade) = (state.audio.shuffle, state.audio.through, state.audio.crossfade);
        let volume = (state.audio_volume * 100.0).round() as u32;
        let going = playing.track.is_some() && !playing.paused;
        let started_at = going.then(|| self.remote.looks.anchor(AUDIO, now, playing.position));
        json!({
            "track": track,
            "track_id": playing.track,
            "playing": going,
            "paused": playing.paused,
            "length_ms": playing.length.map(|l| l.as_millis() as u64),
            "started_at": started_at,
            "position_ms": (!going).then_some(playing.position.as_millis() as u64),
            "volume": volume,
            "mute": volume == 0,
            "shuffle": shuffle,
            "through": through,
            "crossfade": crossfade,
        })
    }

    /// Each timer, with when it was last started and what it had counted before: the page works
    /// out what it shows.
    fn remote_timers(&self) -> Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return json!([]) };
        let clock = self.remote.clock;
        let timers: Vec<Value> = state
            .timers
            .iter()
            .map(|timer| {
                json!({
                    "id": timer.id,
                    "name": timer.name,
                    "kind": timer.kind,
                    "words": crate::make_cli::kind_words(&timer.kind),
                    "overrun": timer.allow_overrun,
                    "running": timer.run.started.is_some(),
                    "counted_ms": timer.run.banked.as_millis() as u64,
                    "started_at": timer.run.started.map(|at| clock.ms(at)),
                })
            })
            .collect();
        json!(timers)
    }

    /// Each screen as `sp screens` has it, with a tone for its words: `good` while it shows what
    /// it should, `trouble` when it doesn't.
    fn remote_screens(&self) -> Value {
        let mut screens = self.ctl_screens().data;
        if let Some(list) = screens.as_array_mut() {
            for screen in list.iter_mut() {
                let shown = screen.get("shown").and_then(Value::as_bool).unwrap_or(false);
                let trouble = screen.get("key_trouble").is_some_and(|t| !t.is_null());
                screen["tone"] = json!(if shown && !trouble { "good" } else { "trouble" });
            }
        }
        let state = self.running.as_ref().map(|r| &r.home_state);
        json!({
            "screens": screens,
            "audience_on": self.show.output_enabled(),
            "stage_on": state.is_some_and(|s| s.stage_on),
            "identify": state.is_some_and(|s| s.identify),
        })
    }

    /// The app's notices as its status button lists them (`notices.rs`): each with its id, kind
    /// (`info`, `trouble`, `ask`), text, time (seconds since 1970), whether it has been seen, and
    /// a question's answers (`["notices", "answer", ID, ACTION]` gives one).
    fn remote_notices(&self) -> Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return json!([]) };
        serde_json::to_value(state.notices.to_vec()).unwrap_or_else(|_| json!([]))
    }

    /// The theme versions, and the one on.
    fn remote_versions(&self) -> Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Value::Null };
        let on =
            state.version_switch.on.map(|on| json!({"id": on.version, "keep": on.keep, "waiting": on.from.is_none()}));
        let versions: Vec<Value> = state.versions.iter().map(|v| json!({"id": v.id, "name": v.name})).collect();
        json!({"versions": versions, "on": on})
    }

    /// Publishes the state for pages, if any are connected and it is due (`force`: now, for a page
    /// just connected); says when to look again. Draws a few slides pages asked for, too.
    pub(crate) fn tick_remote_state(&mut self, force: bool) -> Option<Instant> {
        let now = Instant::now();
        let mut wake = None;
        // Slides waiting to be drawn: a few each time round, so the window never waits long.
        for _ in 0..super::DRAW_AT_ONCE {
            let Some((slide, version, look, reply)) = self.remote.thumbs_waiting.pop_front() else { break };
            let _ = reply.send(self.remote_draw(slide, version, &look));
        }
        if !self.remote.thumbs_waiting.is_empty() {
            wake = Some(now + Duration::from_millis(5));
        }
        let Some(shared) = self.remote.shared().cloned() else { return wake };
        // What the server says (the sign-in page, refusals) follows the app's language, signed
        // in or not.
        let language = crate::i18n::current();
        shared.set_language(language);
        if shared.page_count() == 0 && !force {
            return wake;
        }
        let due = self.remote.published.map_or(now, |at| at + PUBLISH_EVERY);
        if force || due <= now {
            let slow_due = self.remote.published_slow.map_or(now, |at| at + SLOW_EVERY);
            let slow = slow_now(force, slow_due, now, self.remote.published_language != Some(language));
            let parts = self.remote_parts(slow);
            shared.publish(parts);
            self.remote.published = Some(now);
            self.remote.published_language = Some(language);
            if slow {
                self.remote.published_slow = Some(now);
                // The versions a picture may be asked for in (any other is drawn as usual).
                if let Some(state) = self.running.as_ref().map(|r| &r.home_state) {
                    shared.set_versions(state.versions.iter().map(|version| version.id));
                }
                return wake;
            }
            // The seldom-changing parts are looked at once more when they are due, in case
            // whatever just happened changed them.
            return [wake, Some(slow_due)].into_iter().flatten().min();
        }
        // Something happened since it was last published: look again when it is due.
        [wake, Some(due)].into_iter().flatten().min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::with_language;
    use crate::service::Item;

    #[test]
    fn the_app_part_says_the_language_and_how_it_writes_numbers() {
        assert_eq!(
            app_part(Language::English),
            json!({"language": "en", "format": {"decimal": ".", "group": ",", "group_from": 4, "quote_open": "\u{201c}", "quote_close": "\u{201d}"}})
        );
        assert_eq!(
            app_part(Language::Spanish),
            json!({"language": "es", "format": {"decimal": ",", "group": ".", "group_from": 5, "quote_open": "\u{201c}", "quote_close": "\u{201d}"}})
        );
        assert_eq!(app_part(Language::Russian)["format"]["quote_open"], "\u{ab}");
        assert_eq!(app_part(Language::Ukrainian)["language"], "uk");
    }

    fn first_playlist(services: &Services) -> Value {
        playlists_part(services)[0].clone()
    }

    #[test]
    fn the_untouched_first_playlist_is_sent_by_the_apps_name_with_the_windows_label() {
        let services = with_language(Language::English, Services::default);
        // English: the name, built in, and no label (it would say the same).
        let sent = with_language(Language::English, || first_playlist(&services));
        assert_eq!((sent["name"].as_str(), sent["built_in"].as_bool()), (Some("Service"), Some(true)));
        assert!(sent.get("label").is_none(), "{sent}");
        // Spanish: the label the window shows; the name stays the app's English one.
        let sent = with_language(Language::Spanish, || first_playlist(&services));
        assert_eq!((sent["name"].as_str(), sent["label"].as_str()), (Some("Service"), Some("Culto")));
        // Made while the app spoke Spanish (named "Culto" where it is kept), sent the same way.
        let made_in_spanish = with_language(Language::Spanish, Services::default);
        assert_eq!(made_in_spanish.front().name, "Culto");
        let sent = with_language(Language::English, || first_playlist(&made_in_spanish));
        assert_eq!((sent["name"].as_str(), sent["built_in"].as_bool()), (Some("Service"), Some(true)));
        assert!(sent.get("label").is_none(), "{sent}");
        // The playlist part and live.playlist carry the same three.
        let mut part = Map::new();
        with_language(Language::Spanish, || put_playlist_name(&services, 0, &mut part));
        assert_eq!(Value::Object(part), json!({"name": "Service", "built_in": true, "label": "Culto"}));

        // Renamed by the church: its own name, nothing built in, in any language (before it is
        // written, and after).
        let mut renamed = services.clone();
        renamed.rename_at(0, "Sunday");
        for services in [renamed.clone(), Services { changed: false, fresh: false, ..renamed }] {
            let sent = with_language(Language::Spanish, || first_playlist(&services));
            assert_eq!(
                sent,
                json!({"number": 1, "name": "Sunday", "items": 0, "rev": services.list[0].rev(), "front": true})
            );
        }
        // A song added makes it the church's too, and so does a second playlist.
        let mut added = services.clone();
        added.add(Item::song("a", "Alpha"));
        assert!(with_language(Language::Spanish, || first_playlist(&added)).get("built_in").is_none());
        // Playlists read from their file are never the app's, whatever they are called.
        let read = Services::from_json(&services.to_json()).unwrap();
        let sent = with_language(Language::Spanish, || first_playlist(&read));
        assert_eq!(sent["name"], "Service");
        assert!(sent.get("built_in").is_none() && sent.get("label").is_none(), "{sent}");
    }

    #[test]
    fn the_built_in_main_theme_carries_the_windows_label() {
        let builtin = Theme::builtin();
        let live = |theme: &Theme, language| {
            let mut live = Map::new();
            live.insert("main_theme".to_owned(), json!(theme.name));
            with_language(language, || put_main_theme(Some(theme), &mut live));
            Value::Object(live)
        };
        assert_eq!(live(&builtin, Language::English), json!({"main_theme": "Built-in", "main_theme_built_in": true}));
        assert_eq!(
            live(&builtin, Language::Spanish),
            json!({"main_theme": "Built-in", "main_theme_built_in": true, "main_theme_label": "Incluido"})
        );
        // Renamed, or another theme of the same name: nothing more.
        let renamed = Theme { name: "Domingo".to_owned(), ..builtin.clone() };
        assert_eq!(live(&renamed, Language::Spanish), json!({"main_theme": "Domingo"}));
        let other = Theme { id: Uuid::new_v4(), ..builtin.clone() };
        assert_eq!(live(&other, Language::Spanish), json!({"main_theme": "Built-in"}));
    }

    #[test]
    fn a_language_switch_is_one_change_with_the_app_part_and_every_labelled_part() {
        let services = with_language(Language::English, Services::default);
        let theme = Theme::builtin();
        // The parts that carry a label, made as `remote_parts` makes them, in a language.
        let parts = |language| {
            with_language(language, || {
                let mut live = Map::new();
                live.insert("main_theme".to_owned(), json!(theme.name));
                put_main_theme(Some(&theme), &mut live);
                let mut front = Map::new();
                put_playlist_name(&services, 0, &mut front);
                live.insert("playlist".to_owned(), Value::Object(front.clone()));
                vec![
                    ("app", app_part(language).to_string()),
                    ("live", Value::Object(live).to_string()),
                    ("playlist", Value::Object(front).to_string()),
                    ("playlists", playlists_part(&services).to_string()),
                    ("timers", "[]".to_owned()),
                ]
            })
        };
        let mut hub = super::super::hub::Hub::new("e".to_owned());
        hub.publish(parts(Language::English));
        let english = hub.version();
        // A snapshot carries the app part with the rest.
        let snapshot: Value = serde_json::from_str(&super::super::hub::parts_object(&hub.all())).unwrap();
        assert_eq!(snapshot["app"]["language"], "en");
        assert_eq!(snapshot["playlists"][0]["built_in"], true);
        assert!(!hub.publish(parts(Language::English)), "nothing changed");
        // Switched to Spanish: one version, with the app part and each labelled part, and
        // nothing else.
        assert!(hub.publish(parts(Language::Spanish)));
        assert_eq!(hub.version(), english + 1);
        let changed = hub.since("e", english).unwrap();
        let names: Vec<&str> = changed.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["app", "live", "playlist", "playlists"]);
        let sent: Value = serde_json::from_str(&super::super::hub::parts_object(&changed)).unwrap();
        assert_eq!(sent["app"]["language"], "es");
        assert_eq!(sent["live"]["main_theme_label"], "Incluido");
        assert_eq!(sent["live"]["playlist"]["label"], "Culto");
        assert_eq!(sent["playlists"][0]["label"], "Culto");
        // And the seldom-sent parts (the playlists) go in that same publish: a change of
        // language publishes them whether or not they are due.
        let now = Instant::now();
        let later = now + SLOW_EVERY;
        assert!(slow_now(false, later, now, true));
        assert!(!slow_now(false, later, now, false));
        assert!(slow_now(false, now, now, false) && slow_now(true, later, now, false));
        // Back in English, the labels go again, together.
        assert!(hub.publish(parts(Language::English)));
        let back: Value =
            serde_json::from_str(&super::super::hub::parts_object(&hub.since("e", english + 1).unwrap())).unwrap();
        assert!(back["live"].get("main_theme_label").is_none() && back["playlists"][0].get("label").is_none());
        assert_eq!(back["app"]["language"], "en");
    }

    #[test]
    fn notices_and_their_answers_are_sent_as_the_window_shows_them() {
        let mut notices = crate::notices::Notices::default();
        with_language(Language::Spanish, || {
            notices.ask(crate::outside::Waiting::Song(0), crate::notices::changed_outside());
        });
        // Read with the app in English (another thread's language), still as the window said it.
        let sent = with_language(Language::English, || serde_json::to_value(notices.to_vec()).unwrap());
        assert!(sent[0]["text"].as_str().is_some_and(|text| text.starts_with("Esta canción se cambió")), "{sent}");
        assert_eq!(sent[0]["actions"][0]["label"], "Recargar");
        assert_eq!(sent[0]["actions"][1]["label"], "Mantener la mía");
    }

    #[test]
    fn a_position_read_a_moment_later_keeps_the_same_start() {
        let mut looks = Looks::default();
        let first = looks.anchor(AUDIO, 10_000, Duration::from_millis(4_000));
        assert_eq!(first, 6_000);
        // Read 120 ms later, 100 ms further in: the same start, so no change is sent.
        assert_eq!(looks.anchor(AUDIO, 10_120, Duration::from_millis(4_100)), first);
        // Moved on by a seek: a new start.
        assert_eq!(looks.anchor(AUDIO, 10_200, Duration::from_millis(30_000)), 0);
        // The video's start is its own.
        assert_eq!(looks.anchor(VIDEO, 10_000, Duration::ZERO), 10_000);
    }
}
