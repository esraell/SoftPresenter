//! What the app publishes for remote pages, from the data it already keeps (never by looking at
//! the window), and the slide pictures it draws for them with the window's own painter.
//!
//! Times are milliseconds since 1970 on the server's clock (the snapshot says what that clock
//! read when it was sent): when a timer was started and what it had counted before, when a
//! video or a track would have been at its start, when the slide on screen goes on by itself.
//! Nothing that ticks is ever sent: the page works the time shown out for itself.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sp_model::{Slide, Theme, Uuid};

use super::server::{Drawn, Rgba};

/// The state is published at most this often while pages are connected.
pub const PUBLISH_EVERY: Duration = Duration::from_millis(50);
/// The parts that change seldom (the library, the playlists, the media bin, the theme
/// versions) at most this often.
pub const SLOW_EVERY: Duration = Duration::from_secs(1);

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
    /// server, which does so on its own thread.
    pub(crate) fn remote_draw(&mut self, id: Uuid, version: Option<Uuid>) -> Option<Drawn> {
        let found = self.show.presentations().iter().enumerate().find_map(|(at, song)| {
            song.slides().find(|(_, slide)| slide.id == id).map(|(_, slide)| (at, slide.clone()))
        });
        let (look, theme, shown, media) = match &found {
            Some((at, slide)) => {
                let look = self.slide_look(*at, slide, version);
                let theme = self.theme_of(*at).clone();
                let shown = version.and_then(|v| theme.slide_in_version(slide, v)).unwrap_or_else(|| slide.clone());
                let media = crate::home::media_of(slide).and_then(|media| {
                    let item = self.show.media().iter().find(|item| item.id == media)?;
                    Some((media, item.fit))
                });
                (look, theme, Some(shown), media)
            }
            None => {
                self.show.media().iter().find(|item| item.id == id)?;
                self.bin_pictures.small(id)?;
                (self.media_look(id), self.theme.clone(), None, Some((id, sp_model::ImageFit::Fill)))
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
            look,
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
        parts.into_iter().map(|(name, value)| (name, value.to_string())).collect()
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
        }
        live
    }

    /// The playlist in front, with its items as `sp playlist` lists them, and its rev (for
    /// `--aim ITEM --rev REV`).
    fn remote_playlist(&self) -> Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Value::Null };
        let front = state.services.front();
        let at = state.services.list.iter().position(|p| p.name == front.name).unwrap_or(0);
        let (_, items) = self.playlist_listing(at);
        json!({
            "name": front.name,
            "number": at + 1,
            "rev": front.rev(),
            "live_item": state.live_item.map(|at| at + 1),
            "items": items,
        })
    }

    fn remote_playlists(&self) -> Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Value::Null };
        let front = state.services.front().name.clone();
        let lists: Vec<Value> = state
            .services
            .list
            .iter()
            .enumerate()
            .map(|(at, list)| json!({"number": at + 1, "name": list.name, "items": list.items.len(), "rev": list.rev(), "front": list.name == front}))
            .collect();
        json!(lists)
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
            let Some((slide, version, reply)) = self.remote.thumbs_waiting.pop_front() else { break };
            let drawn = self.remote_draw(slide, version);
            let _ = reply.send(drawn);
        }
        if !self.remote.thumbs_waiting.is_empty() {
            wake = Some(now + Duration::from_millis(5));
        }
        let Some(shared) = self.remote.shared().cloned() else { return wake };
        if shared.page_count() == 0 && !force {
            return wake;
        }
        let due = self.remote.published.map_or(now, |at| at + PUBLISH_EVERY);
        if force || due <= now {
            let slow_due = self.remote.published_slow.map_or(now, |at| at + SLOW_EVERY);
            let slow = force || slow_due <= now;
            let parts = self.remote_parts(slow);
            shared.publish(parts);
            self.remote.published = Some(now);
            if slow {
                self.remote.published_slow = Some(now);
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
