//! How large the operator's windows draw everything (Settings › Slides › In your window ›
//! Interface size). Automatic, the usual, draws a window bigger than the size the layout was
//! drawn for (1440 × 900 points) larger in proportion, up to 125%, so a big screen shows the same
//! layout, bigger, instead of the same narrow side columns round a huge middle; a fixed 100% to 175%
//! draws everything that much larger, as far as the window has room.
//!
//! The toolkit's own zoom does it: every window is laid out in points as before, at more pixels
//! to the point, and the words are drawn afresh at that size, never stretched. The pictures of
//! slides in the window (thumbnails, the previews) are drawn larger with it, so they stay sharp.
//! The screens never change: this is the operator's windows only.

use std::time::{Duration, Instant};

use egui::{Vec2, vec2};

/// The window the layout was drawn for, in points: the design's 1440 × 900.
pub const BASE: Vec2 = vec2(1440.0, 900.0);
/// The home window's least size, in points: no factor lays it out smaller than this.
pub const HOME_LEAST: Vec2 = vec2(1100.0, 700.0);
/// The most Automatic goes to. The owner, on the 5K iMac full screen at 160%: "the scaling is a
/// bit too much, there needs to be a limit on how much the ui should scale, i think we move it
/// down". Larger stays a choice (150%, 175%).
pub const AUTOMATIC_LARGEST: f32 = 1.25;
/// The factor moves in steps of 5%, so a window a little bigger than the base keeps 100%, and a
/// drag across a large screen changes it a few times, not at every point.
const STEP: f32 = 0.05;
/// How long a window keeps a new size or scale before the factor follows it: a drag of its edge
/// changes the factor once, when it stops, never at each step (each new factor draws the words
/// and the pictures of slides again, the pictures on the GPU the screens share).
pub const SETTLE: Duration = Duration::from_millis(150);

/// The Interface size setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InterfaceSize {
    /// Larger in proportion in a window bigger than the base size.
    #[default]
    Automatic,
    /// Always this many percent (100, 125, 150 or 175), as far as the window has room.
    Percent(u16),
}

impl InterfaceSize {
    /// Every choice, in the order the setting offers them.
    pub const ALL: [InterfaceSize; 5] = [
        InterfaceSize::Automatic,
        InterfaceSize::Percent(100),
        InterfaceSize::Percent(125),
        InterfaceSize::Percent(150),
        InterfaceSize::Percent(175),
    ];

    /// How the settings file keeps it: `auto`, or the percentage (`125`).
    pub fn code(self) -> String {
        match self {
            InterfaceSize::Automatic => "auto".to_owned(),
            InterfaceSize::Percent(percent) => percent.to_string(),
        }
    }

    /// The choice a settings file names; Automatic for anything else (a file from before the
    /// setting says nothing).
    pub fn from_code(code: &str) -> InterfaceSize {
        match code.trim().parse::<u16>() {
            Ok(percent) if InterfaceSize::ALL.contains(&InterfaceSize::Percent(percent)) => {
                InterfaceSize::Percent(percent)
            }
            _ => InterfaceSize::Automatic,
        }
    }

    /// A small number standing for the choice, for [`Seen`].
    fn number(self) -> u32 {
        match self {
            InterfaceSize::Automatic => 0,
            InterfaceSize::Percent(percent) => u32::from(percent),
        }
    }
}

/// A factor rounded down to a whole step, never under 1.
fn stepped(factor: f32) -> f32 {
    // A little over, so that 1440 × 900 ÷ the base, 1.6 as near as floats come, is 1.6.
    (((factor / STEP) + 0.001).floor() * STEP).max(1.0)
}

/// The largest factor at which a window of `window` points is still laid out at least `least`:
/// a window never shows less room than its least size, whatever is chosen.
pub fn room(window: Vec2, least: Vec2) -> f32 {
    stepped((window.x / least.x).min(window.y / least.y))
}

/// Automatic: as much larger as the window is than the base size, in the direction it has least
/// of (a wide, short window is not drawn taller than it is), from 100% to [`AUTOMATIC_LARGEST`].
pub fn automatic(window: Vec2) -> f32 {
    stepped((window.x / BASE.x).min(window.y / BASE.y)).min(AUTOMATIC_LARGEST)
}

/// The factor the home window draws at, for its size in points (the window's own, not the
/// toolkit's, which shrinks as the factor grows).
pub fn home_zoom(size: InterfaceSize, window: Vec2) -> f32 {
    let wanted = match size {
        InterfaceSize::Automatic => automatic(window),
        InterfaceSize::Percent(percent) => f32::from(percent) / 100.0,
    };
    wanted.min(room(window, HOME_LEAST)).max(1.0)
}

/// The factor a window of its own (Settings, Screens) draws at: the home window's, so every
/// window looks the same, as far as it has room for its least size.
pub fn panel_zoom(home: f32, window: Vec2, least: Vec2) -> f32 {
    home.min(room(window, least)).max(1.0)
}

/// How much larger than their usual size the window's pictures of slides are drawn (thumbnails
/// and the previews), for its pixels to the point (the display's scale times the factor). Their
/// usual sizes were chosen for a Retina display at 100%, so they grow only past that, in
/// quarters, up to twice: a display with fewer pixels never draws them larger than today.
pub fn picture_scale(pixels_per_point: f32) -> f32 {
    ((pixels_per_point / 2.0 * 4.0).round() / 4.0).clamp(1.0, 2.0)
}

/// A picture's usual size, `scale` times larger, in even numbers of pixels.
pub fn scaled((w, h): (u32, u32), scale: f32) -> (u32, u32) {
    let grow = |n: u32| ((n as f32 * scale / 2.0).round() as u32 * 2).max(2);
    (grow(w), grow(h))
}

/// What a window's factor is worked out from: its size in pixels, its display's scale, and what
/// is asked for (the setting, or for a window of its own, the home window's factor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    size: (u32, u32),
    scale: u64,
    asked: u32,
}

impl Seen {
    pub fn home(size: (u32, u32), scale: f64, setting: InterfaceSize) -> Seen {
        Seen { size, scale: scale.to_bits(), asked: setting.number() }
    }

    pub fn panel(size: (u32, u32), scale: f64, home: f32) -> Seen {
        Seen { size, scale: scale.to_bits(), asked: (home * 1000.0).round() as u32 }
    }
}

/// When a window's factor is to be worked out again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    /// Now.
    Now,
    /// Not yet: look again after this long.
    After(Duration),
    /// Nothing has changed.
    Done,
}

/// Watches what a window's factor is worked out from: it is worked out at once the first time
/// and when only what is asked for changes (a choice in Settings shows at once), and for a new
/// size or display scale once that has stayed the same for [`SETTLE`].
#[derive(Debug, Default)]
pub struct Watch {
    applied: Option<Seen>,
    pending: Option<(Seen, Instant)>,
}

impl Watch {
    pub fn due(&mut self, seen: Seen, now: Instant) -> Due {
        match self.applied {
            Some(applied) if applied == seen => {
                self.pending = None;
                Due::Done
            }
            None => Due::Now,
            Some(applied) if applied.size == seen.size && applied.scale == seen.scale => Due::Now,
            Some(_) => {
                let since = match self.pending {
                    Some((pending, at)) if pending == seen => at,
                    _ => {
                        self.pending = Some((seen, now));
                        now
                    }
                };
                let waited = now.saturating_duration_since(since);
                if waited >= SETTLE { Due::Now } else { Due::After(SETTLE - waited) }
            }
        }
    }

    /// The factor has been worked out from `seen`.
    pub fn applied(&mut self, seen: Seen) {
        self.applied = Some(seen);
        self.pending = None;
    }
}

/// Testing aid: SOFTPRESENTER_CAPTURE_SIZE=2560x1440 lays a capture run's home window out as if
/// it were that big (points), whatever room the display has, and saves its picture at that size
/// (at the display's scale), to see the window at sizes this computer's display can't hold.
pub fn capture_size() -> Option<Vec2> {
    let asked = std::env::var("SOFTPRESENTER_CAPTURE_SIZE").ok()?;
    let (w, h) = asked.split_once('x')?;
    let (w, h) = (w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?);
    (w >= 1.0 && h >= 1.0).then(|| vec2(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn automatic_keeps_100_percent_up_to_the_base_size_and_grows_in_proportion_beyond_it() {
        let auto = |w: f32, h: f32| home_zoom(InterfaceSize::Automatic, vec2(w, h));
        // At or under the size the layout was drawn for: as today, small windows keeping their
        // least sizes.
        for (w, h) in [(1100.0, 700.0), (1440.0, 774.0), (1440.0, 900.0), (1512.0, 900.0), (800.0, 600.0)] {
            assert!(close(auto(w, h), 1.0), "{w} × {h}: {}", auto(w, h));
        }
        // Bigger, by the side it has least of, in steps of 5%, rounded down.
        assert!(close(auto(1920.0, 1080.0), 1.2), "{}", auto(1920.0, 1080.0));
        assert!(close(auto(1800.0, 1125.0), 1.25), "{}", auto(1800.0, 1125.0));
        // No larger than 125%: the 5K iMac full screen would be 160% in proportion.
        assert!(close(auto(2560.0, 1440.0), AUTOMATIC_LARGEST), "the 5K iMac full screen: {}", auto(2560.0, 1440.0));
        assert!(close(auto(2560.0, 1415.0), AUTOMATIC_LARGEST), "under its menu bar: {}", auto(2560.0, 1415.0));
        assert!(close(auto(1728.0, 1117.0), 1.2), "{}", auto(1728.0, 1117.0));
        // A wide, short window grows only as much as its height allows.
        assert!(close(auto(3440.0, 1000.0), 1.1), "{}", auto(3440.0, 1000.0));
        assert!(close(auto(3840.0, 2160.0), AUTOMATIC_LARGEST), "{}", auto(3840.0, 2160.0));
        // Just over the base is still 100%: no step for a few points.
        assert!(close(auto(1480.0, 930.0), 1.0));
    }

    #[test]
    fn a_fixed_size_overrides_automatic_as_far_as_the_window_has_room() {
        let fixed = |percent: u16, w: f32, h: f32| home_zoom(InterfaceSize::Percent(percent), vec2(w, h));
        // In a big window, what was chosen, larger or smaller than Automatic would be.
        assert!(close(fixed(125, 2560.0, 1440.0), 1.25));
        assert!(close(fixed(100, 2560.0, 1440.0), 1.0), "100% is today's size, even on a big screen");
        assert!(close(fixed(175, 2560.0, 1440.0), 1.75));
        assert!(close(fixed(150, 1920.0, 1080.0), 1.5));
        // In a smaller one, no more than keeps its least size: 1100 × 700 points of room.
        assert!(close(fixed(150, 1440.0, 774.0), 1.1), "{}", fixed(150, 1440.0, 774.0));
        assert!(close(fixed(175, 1100.0, 700.0), 1.0));
        let zoom = fixed(175, 1920.0, 1080.0);
        assert!(1920.0 / zoom >= HOME_LEAST.x && 1080.0 / zoom >= HOME_LEAST.y, "{zoom}");
    }

    #[test]
    fn no_factor_lays_a_window_out_smaller_than_its_least_size() {
        for size in InterfaceSize::ALL {
            for w in (900..=4000).step_by(37) {
                for h in (600..=2400).step_by(29) {
                    let window = vec2(w as f32, h as f32);
                    let zoom = home_zoom(size, window);
                    assert!((1.0..=1.751).contains(&zoom), "{size:?} {window:?}: {zoom}");
                    if zoom > 1.0 {
                        let laid_out = window / zoom;
                        assert!(laid_out.x >= HOME_LEAST.x - 0.1 && laid_out.y >= HOME_LEAST.y - 0.1);
                    }
                    if size == InterfaceSize::Automatic && zoom > 1.0 {
                        let laid_out = window / zoom;
                        assert!(laid_out.x >= BASE.x - 0.1 && laid_out.y >= BASE.y - 0.1, "{window:?}: {zoom}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_window_of_its_own_follows_the_home_window_as_far_as_it_has_room() {
        let least = vec2(820.0, 560.0);
        assert!(close(panel_zoom(1.6, vec2(1664.0, 1152.0), least), 1.6));
        assert!(close(panel_zoom(1.6, vec2(1040.0, 720.0), least), 1.25), "made smaller: as much as fits");
        assert!(close(panel_zoom(1.0, vec2(2000.0, 1400.0), least), 1.0), "never larger than the home window");
        assert!(close(panel_zoom(1.6, vec2(700.0, 500.0), least), 1.0));
    }

    #[test]
    fn the_setting_is_kept_as_auto_or_its_percentage() {
        for size in InterfaceSize::ALL {
            assert_eq!(InterfaceSize::from_code(&size.code()), size);
        }
        assert_eq!(InterfaceSize::Automatic.code(), "auto");
        assert_eq!(InterfaceSize::Percent(150).code(), "150");
        for odd in ["", "auto", "90", "200", "1.5", "large"] {
            assert_eq!(InterfaceSize::from_code(odd), InterfaceSize::Automatic, "{odd:?}");
        }
    }

    #[test]
    fn pictures_grow_only_past_a_retina_display_at_100_percent() {
        assert!(close(picture_scale(1.0), 1.0), "a display with fewer pixels: as today");
        assert!(close(picture_scale(1.6), 1.0));
        assert!(close(picture_scale(2.0), 1.0), "Retina at 100%: as today");
        assert!(close(picture_scale(2.4), 1.25));
        assert!(close(picture_scale(3.2), 1.5), "a Retina display at 160%");
        assert!(close(picture_scale(3.5), 1.75));
        assert!(close(picture_scale(6.0), 2.0));
        assert_eq!(scaled((640, 360), 1.0), (640, 360));
        assert_eq!(scaled((640, 360), 1.5), (960, 540));
        assert_eq!(scaled((480, 270), 1.25), (600, 338));
    }

    #[test]
    fn a_new_size_waits_to_settle_and_a_new_choice_shows_at_once() {
        let start = Instant::now();
        let ms = |n: u64| start + Duration::from_millis(n);
        let mut watch = Watch::default();
        let first = Seen::home((2880, 1548), 2.0, InterfaceSize::Automatic);
        assert_eq!(watch.due(first, ms(0)), Due::Now, "the first frame: at once");
        watch.applied(first);
        assert_eq!(watch.due(first, ms(10)), Due::Done);
        // A drag: each new size starts the wait again.
        let dragged = Seen::home((3000, 1600), 2.0, InterfaceSize::Automatic);
        assert_eq!(watch.due(dragged, ms(20)), Due::After(SETTLE));
        let further = Seen::home((3200, 1700), 2.0, InterfaceSize::Automatic);
        assert_eq!(watch.due(further, ms(100)), Due::After(SETTLE));
        assert_eq!(watch.due(further, ms(200)), Due::After(Duration::from_millis(50)));
        assert_eq!(watch.due(further, ms(250)), Due::Now);
        watch.applied(further);
        // Another display's scale waits too.
        let moved = Seen::home((3200, 1700), 1.0, InterfaceSize::Automatic);
        assert_eq!(watch.due(moved, ms(300)), Due::After(SETTLE));
        watch.applied(moved);
        // A choice in Settings: at once.
        let chosen = Seen::home((3200, 1700), 1.0, InterfaceSize::Percent(125));
        assert_eq!(watch.due(chosen, ms(400)), Due::Now);
    }
}
