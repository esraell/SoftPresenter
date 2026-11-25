//! The window's side of the anonymous reports: every couple of seconds, while reports are on, it
//! hands the reporter what is on screen and how the screens stand, as kinds, counts and sizes
//! (see `facts`), and notes a display plugged in or taken out. It also shows the one-time notice
//! that reports are on. It reads the app's own state and makes a few small values; nothing in
//! it waits, and it reads no name.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

use super::facts::{DisplayFact, Facts, OutputCounts};
use super::stats;
use crate::App;
use crate::screens::Output;

/// How often the window hands over its facts, in milliseconds.
const EVERY_MS: u64 = 2000;

impl App {
    /// Called as the event loop is about to wait: tells the person once that reports are on, and
    /// now and then, while they are, hands the reporter the facts.
    pub(crate) fn reports_tick(&mut self) {
        self.reports_notice();
        if !super::offered() || !stats::is_on() {
            return;
        }
        static LAST: AtomicU64 = AtomicU64::new(0);
        let now = stats::since_start_ms();
        if now != 0 && now.saturating_sub(LAST.load(Relaxed)) < EVERY_MS {
            return;
        }
        LAST.store(now.max(1), Relaxed);
        let facts = self.reports_facts();
        // A display plugged in or taken out: how many there are now.
        static DISPLAYS: AtomicU32 = AtomicU32::new(u32::MAX);
        let count = facts.displays.len() as u32;
        let before = DISPLAYS.swap(count, Relaxed);
        if before != u32::MAX && before != count {
            stats::event_n(if count > before { "display_added" } else { "display_removed" }, f64::from(count));
        }
        // The view opened: counted once each time it changes.
        static VIEW: std::sync::Mutex<&'static str> = std::sync::Mutex::new("");
        if let Ok(mut view) = VIEW.try_lock()
            && *view != facts.view
        {
            *view = facts.view;
            stats::feature(match facts.view {
                "bible" => "view_bible",
                "edit" => "view_edit",
                "themes" => "view_themes",
                _ => "view_service",
            });
        }
        super::set_facts(facts);
    }

    /// The one-time notice, for an install that never passed the setup's step (one updated from
    /// an earlier version, or whose setup was skipped): said once the setup is out of the way,
    /// in the app's own notices, opened so that it is read, with Turn off beside it. From then
    /// the person has been told, and reports may go (no sooner than a minute later).
    fn reports_notice(&mut self) {
        // Switched on or off in Settings, the setup or `sp reports`: the question is answered.
        if super::chosen_since()
            && let Some(running) = self.running.as_mut()
            && running.home_state.notices.asks_about(crate::outside::Waiting::Reports)
        {
            running.home_state.notices.settle(crate::outside::Waiting::Reports);
            running.home_window.request_redraw();
        }
        if !super::should_tell() {
            return;
        }
        let Some(running) = self.running.as_mut() else { return };
        if running.home_state.setup.is_some() {
            return;
        }
        running.home_state.notices.ask_reports(super::ui::notice_words());
        egui::Popup::open_id(&running.egui_ctx, crate::notices::list_id());
        running.home_window.request_redraw();
        super::mark_seen();
    }

    /// What the reports say of the show now, as kinds and counts.
    fn reports_facts(&self) -> Facts {
        let Some(running) = self.running.as_ref() else { return Facts::default() };
        let state = &running.home_state;
        let monitors = running.displays_seen.as_ref().map(|seen| seen.monitors.as_slice()).unwrap_or_default();
        let displays: Vec<DisplayFact> = state
            .displays
            .iter()
            .enumerate()
            .map(|(at, display)| {
                let monitor = monitors.get(at);
                DisplayFact {
                    w: display.w,
                    h: display.h,
                    scale: monitor.map_or(1.0, |m| m.scale_factor()),
                    hz: monitor.and_then(|m| m.refresh_rate_millihertz()).map(|mhz| mhz as f32 / 1000.0),
                    main: display.main,
                }
            })
            .collect();
        let audience_on = self.show.output_enabled();
        let mut outputs = OutputCounts::default();
        for (at, screen) in state.screens.iter().enumerate() {
            let shown = state.screen_status.get(at).is_some_and(|(_, shown)| *shown);
            let stage = screen.kind == crate::screens::Kind::Stage;
            if stage {
                outputs.stage += 1;
            } else {
                outputs.audience += 1;
            }
            if shown && if stage { state.stage_on } else { audience_on } {
                outputs.on += 1;
            }
            match &screen.output {
                Output::Automatic | Output::Display(_) => outputs.display += 1,
                Output::Window => outputs.window += 1,
                Output::Ndi(_) => outputs.ndi += 1,
                Output::DeckLink(_) => outputs.decklink += 1,
                Output::Off => {}
            }
        }
        let mut scene: Vec<&'static str> = Vec::new();
        let mut video_file = None;
        if let Some(item) = self.show.live_media().and_then(|at| self.show.media().get(at)) {
            if crate::media::is_video(&item.path) {
                scene.push("video");
                scene.push(match self.video.as_ref() {
                    Some((_, player)) if player.uses_ffmpeg() => "player_ffmpeg",
                    _ => "player_system",
                });
                if item.looping {
                    scene.push("video_loop");
                }
                video_file = Some(item.path.clone());
            } else {
                scene.push("picture");
            }
        }
        if let Some(at) = self.show.live() {
            scene.push(if self.bible_slots.contains(&at.presentation) {
                "bible"
            } else if at.presentation == self.voxlive_slot {
                "captions"
            } else {
                "song"
            });
        }
        if let Some(announcing) = self.announcing.as_ref() {
            scene.push(match announcing.what {
                crate::announcements::Announce::Song(_) => "announcement_song",
                crate::announcements::Announce::Media(_) if announcing.video.is_some() => "announcement_video",
                crate::announcements::Announce::Media(_) => "announcement_picture",
                crate::announcements::Announce::MediaPlaylist(_) => "announcement_playlist",
            });
        }
        if !state.props_live.is_empty() {
            scene.push("prop");
        }
        if state.message_live.is_some() {
            scene.push("message");
        }
        if !self.web_pages.is_empty() {
            scene.push("web_page");
        }
        let view = if state.bible_open {
            "bible"
        } else if state.theme_edit.is_some() {
            "themes"
        } else if state.editing.is_some() {
            "edit"
        } else {
            "service"
        };
        Facts {
            view,
            displays,
            outputs,
            ndi: state.ndi_available,
            decklink: !state.decklink.devices.is_empty(),
            language: crate::i18n::current().code(),
            scene,
            video_size: None,
            video_file,
        }
    }
}
