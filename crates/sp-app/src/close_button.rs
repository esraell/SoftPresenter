//! The big X a screen's window shows when it fills a display and the pointer moves over it. A
//! screen can end up on the operator's own display: chosen for it by mistake, or put there by
//! the app on a computer that doesn't say which display the operator works on (Wayland, a
//! mirrored display, an operator who works on a display Windows doesn't call the main one). The
//! window then covers everything, with no frame to close. Moving the pointer over it shows the X
//! in its top-right corner, and clicking that switches the screen off.
//!
//! This is the X's clock and where it is: plain data, each call given the time, so the tests need
//! no waiting. The window loop feeds it the pointer (`App::window_event`), and as the clock runs
//! (`App::tick_close_buttons`) tells the screens' thread how strongly to draw it
//! (`OutputMsg::CloseButton`). That thread lays it over the window's own picture as the picture
//! goes into the window (`sp_render::CloseMark`), never into anything NDI, a DeckLink card, a
//! key, a stage screen, the operator's preview or a capture is made from.

use std::time::{Duration, Instant};

use sp_render::{MarkLook, Square};

use crate::screens::Placed;

/// How long the pointer must keep moving over the window before the X shows: a pointer that
/// only crosses the church's television on its way somewhere shows nothing.
pub const SHOW_AFTER: Duration = Duration::from_millis(500);
/// A pause in the pointer's moving longer than this starts that count again.
pub const PAUSE: Duration = Duration::from_millis(250);
/// How long the X stays once the pointer stops moving over the window.
pub const STAYS: Duration = Duration::from_secs(3);
/// How long the X stays once the pointer leaves the window.
pub const STAYS_AFTER_LEAVING: Duration = Duration::from_secs(1);
/// How long it takes to come and to go.
pub const FADE_IN: Duration = Duration::from_millis(200);
pub const FADE_OUT: Duration = Duration::from_millis(600);
/// How long the X must have been shown before a press on it counts: a double click in the corner
/// of a screen, whose first press shows the X, never switches the screen off.
pub const ARMED_AFTER: Duration = Duration::from_millis(400);
/// How often it is drawn while it comes or goes.
pub const STEP: Duration = Duration::from_millis(16);
/// Whether a press anywhere on the window shows the X at once: only on a Mac, where a window
/// that isn't the one with the keyboard may not hear of the pointer moving over it until it is
/// clicked. Elsewhere only half a second of moving does, so a stray click on the church's TV
/// shows nothing.
pub const SHOWS_ON_PRESS: bool = cfg!(target_os = "macos");

/// The X's plate across, how far it is from the window's top and right edges, and how far
/// outside its plate a press still counts, in points.
pub const SIDE: f64 = 72.0;
pub const MARGIN: f64 = 24.0;
pub const REACH: f64 = 8.0;

/// Whether a screen's window placed so has the X: only one that fills a display, where there is
/// no other way to close it. A screen in an ordinary window has its own frame (only with the
/// testing aid below); NDI, DeckLink and a screen that is off have no window.
pub fn has_close_button(placed: &Placed, in_windows_too: bool) -> bool {
    match placed {
        Placed::Fullscreen(_) => true,
        Placed::Window => in_windows_too,
        Placed::Ndi(_) | Placed::DeckLink(_) | Placed::Nothing(_) => false,
    }
}

/// Testing aid: SOFTPRESENTER_CLOSE_BUTTON_IN_WINDOWS=1 gives the screens shown in an ordinary
/// window the X too, so it can be tried without filling a display.
pub fn in_windows_too() -> bool {
    static ASKED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    // Testing aid: SOFTPRESENTER_CLOSE_BUTTON_IN_WINDOWS (see above).
    *ASKED.get_or_init(|| std::env::var_os("SOFTPRESENTER_CLOSE_BUTTON_IN_WINDOWS").is_some())
}

/// Where the X is in a window `w` pixels wide with `scale` pixels to the point: its square, in
/// pixels from the window's top-left corner.
pub fn square(w: u32, scale: f64) -> Square {
    let (side, margin) = (SIDE * scale, MARGIN * scale);
    Square { x: (f64::from(w) - margin - side) as f32, y: margin as f32, side: side as f32 }
}

/// True when a point (pixels from the window's top-left corner) is on the X's plate, or just
/// outside it.
fn on_plate(square: Square, x: f64, y: f64, scale: f64) -> bool {
    let radius = f64::from(square.side) / 2.0;
    let (centre_x, centre_y) = (f64::from(square.x) + radius, f64::from(square.y) + radius);
    (x - centre_x).hypot(y - centre_y) <= radius + REACH * scale
}

/// How the X is drawn at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    /// How strongly, above 0 and up to 1, in steps of 1/64, so the same look is not sent twice.
    pub strength: f32,
    /// The pointer is on it: drawn brighter.
    pub lit: bool,
    /// Pixels to the point, for its size.
    pub scale: f64,
}

impl Drawn {
    /// Its colours, from the app's own (`studio`): a dark plate the picture shows through, a
    /// faint light ring round it, so it can be seen on a black screen, and a light X; all a
    /// little brighter under the pointer.
    pub fn look(&self) -> MarkLook {
        use crate::studio::{BUTTON_HOVER, PANEL, TEXT, TEXT_DIM};
        let rgba = |colour: egui::Color32, alpha: f32| {
            [f32::from(colour.r()) / 255.0, f32::from(colour.g()) / 255.0, f32::from(colour.b()) / 255.0, alpha]
        };
        let (plate, ring, cross) = if self.lit {
            (rgba(BUTTON_HOVER, 0.88), rgba(TEXT, 0.4), rgba(TEXT, 1.0))
        } else {
            (rgba(PANEL, 0.72), rgba(TEXT, 0.22), rgba(TEXT_DIM, 0.95))
        };
        MarkLook { plate, ring, cross, strength: self.strength }
    }
}

/// What a press on the window did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Nothing more than keep the X shown.
    Nothing,
    /// The X was not shown, and shows now: on a Mac a press anywhere shows it at once (see
    /// [`SHOWS_ON_PRESS`]).
    Showed,
    /// The press was on the X, shown long enough to have been seen: switch the screen off.
    SwitchOff,
}

/// One window's X: where the pointer is, how it has moved, and when the X showed.
#[derive(Clone, Debug, Default)]
pub struct CloseButton {
    /// Where the pointer is, in pixels from the window's top-left corner; `None` once it has left.
    pointer: Option<(f64, f64)>,
    /// The pointer is on the X's plate (or just outside it).
    on_it: bool,
    /// Since when the pointer has kept moving without a pause, and when it last moved.
    moving_since: Option<Instant>,
    moved_at: Option<Instant>,
    /// When the pointer left the window, while it is away.
    left_at: Option<Instant>,
    /// When the X began to show, while it shows.
    shown_at: Option<Instant>,
}

impl CloseButton {
    /// The pointer moved to `x`, `y` (pixels) in a window `w` pixels wide with `scale` pixels to
    /// the point. Only a move to another place counts.
    pub fn moved(&mut self, now: Instant, (x, y): (f64, f64), w: u32, scale: f64) {
        self.settle(now);
        if self.pointer == Some((x, y)) {
            return;
        }
        let was = self.strength(now);
        self.pointer = Some((x, y));
        self.on_it = on_plate(square(w, scale), x, y, scale);
        self.left_at = None;
        if !self.moved_at.is_some_and(|at| now.saturating_duration_since(at) <= PAUSE) {
            self.moving_since = Some(now);
        }
        self.moved_at = Some(now);
        self.come_back(now, was);
        if self.shown_at.is_none()
            && self.moving_since.is_some_and(|since| now.saturating_duration_since(since) >= SHOW_AFTER)
        {
            self.shown_at = Some(now);
        }
    }

    /// The pointer left the window.
    pub fn left(&mut self, now: Instant) {
        self.settle(now);
        if self.pointer.take().is_some() {
            self.left_at = Some(now);
        }
        self.on_it = false;
    }

    /// The main button was pressed over the window, where the pointer last moved to. With
    /// `shows` (see [`SHOWS_ON_PRESS`]) a press while the X is gone shows it.
    pub fn pressed(&mut self, now: Instant, shows: bool) -> Press {
        self.settle(now);
        match self.shown_at {
            Some(shown) if self.on_it && now.saturating_duration_since(shown) >= ARMED_AFTER => Press::SwitchOff,
            Some(_) => {
                let was = self.strength(now);
                self.moved_at = Some(now);
                self.come_back(now, was);
                Press::Nothing
            }
            None if shows => {
                self.shown_at = Some(now);
                self.moved_at = Some(now);
                Press::Showed
            }
            None => Press::Nothing,
        }
    }

    /// Lets the X go once it has faded out.
    pub fn settle(&mut self, now: Instant) {
        if self.shown_at.is_some() && self.fade_from().is_some_and(|from| now >= from + FADE_OUT) {
            self.shown_at = None;
        }
    }

    /// True while the X shows (or is coming or going).
    #[cfg(test)]
    fn shown(&self) -> bool {
        self.shown_at.is_some()
    }

    /// How the X is drawn now in a window with `scale` pixels to the point; `None` when it isn't.
    pub fn drawn(&self, now: Instant, scale: f64) -> Option<Drawn> {
        let strength = self.strength(now);
        (strength > 0.0).then(|| Drawn { strength: (strength * 64.0).ceil() / 64.0, lit: self.on_it, scale })
    }

    /// When the X will next look different without the pointer doing anything: a step later
    /// while it comes or goes, when it starts to go while it shows, never while it is gone or
    /// the pointer rests on it.
    pub fn next_change(&self, now: Instant) -> Option<Instant> {
        let shown = self.shown_at?;
        if now < shown + FADE_IN {
            return Some(now + STEP);
        }
        let from = self.fade_from()?;
        Some(if now < from { from } else { (now + STEP).min(from + FADE_OUT) })
    }

    /// When the X starts to go: a while after the pointer last moved, or left; never while the
    /// pointer is on it.
    fn fade_from(&self) -> Option<Instant> {
        match (self.pointer, self.left_at) {
            (None, Some(left)) => Some(left + STAYS_AFTER_LEAVING),
            (Some(_), _) if self.on_it => None,
            _ => self.moved_at.map(|at| at + STAYS),
        }
    }

    /// How strongly it is drawn now, 0 to 1.
    fn strength(&self, now: Instant) -> f32 {
        let Some(shown) = self.shown_at else { return 0.0 };
        let coming = share(now.saturating_duration_since(shown), FADE_IN);
        let going = self.fade_from().map_or(0.0, |from| share(now.saturating_duration_since(from), FADE_OUT));
        coming.min(1.0 - going)
    }

    /// An X on its way out that the pointer wakes (it moved, or pressed) comes back from as strong
    /// as it was then, not from nothing and not at once.
    fn come_back(&mut self, now: Instant, was: f32) {
        if let Some(shown) = self.shown_at
            && now >= shown + FADE_IN
            && was < 1.0
        {
            self.shown_at = Some(now.checked_sub(FADE_IN.mul_f32(was)).unwrap_or(now));
        }
    }
}

/// How far `gone` is through `whole`, from 0 to 1.
fn share(gone: Duration, whole: Duration) -> f32 {
    (gone.as_secs_f32() / whole.as_secs_f32()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 1920;
    const SCALE: f64 = 1.0;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// The pointer moving across the window every 16 ms from `start` for `long`, away from the X.
    fn sweep(button: &mut CloseButton, start: Instant, long: Duration) -> Instant {
        let mut at = start;
        let mut x = 100.0;
        while at <= start + long {
            button.moved(at, (x, 500.0), W, SCALE);
            x += 3.0;
            at += ms(16);
        }
        at - ms(16)
    }

    /// The middle of the X in a 1920-pixel window at one pixel to the point.
    fn on_x() -> (f64, f64) {
        let square = square(W, SCALE);
        (f64::from(square.x + square.side / 2.0), f64::from(square.y + square.side / 2.0))
    }

    #[test]
    fn only_a_window_that_fills_a_display_has_the_x() {
        assert!(has_close_button(&Placed::Fullscreen(1), false));
        assert!(!has_close_button(&Placed::Window, false), "an ordinary window has its own frame");
        assert!(has_close_button(&Placed::Window, true), "unless the testing aid asks");
        assert!(!has_close_button(&Placed::Ndi("Main".to_owned()), true));
        assert!(!has_close_button(&Placed::Nothing(String::new()), true));
        let card = crate::screens::DeckLinkOut {
            device: "DeckLink Duo (1)".to_owned(),
            id: None,
            mode: "1080i59.94".to_owned(),
            keyer: crate::screens::Keyer::Off,
        };
        assert!(!has_close_button(&Placed::DeckLink(card), true));
    }

    #[test]
    fn the_x_sits_in_the_top_right_corner_and_grows_with_the_display_scale() {
        assert_eq!(square(1920, 1.0), Square { x: 1824.0, y: 24.0, side: 72.0 });
        assert_eq!(square(3840, 2.0), Square { x: 3648.0, y: 48.0, side: 144.0 });
    }

    #[test]
    fn a_pointer_crossing_the_window_quickly_shows_nothing() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let end = sweep(&mut button, start, ms(400));
        assert!(!button.shown());
        assert_eq!(button.drawn(end, SCALE), None);
        assert_eq!(button.next_change(end), None, "nothing to wake for");
        // It left; coming back later starts the count again.
        button.left(end);
        let back = end + ms(300);
        sweep(&mut button, back, ms(400));
        assert!(!button.shown(), "the count started again: {:?}", button.drawn(back + ms(400), SCALE));
    }

    #[test]
    fn the_x_shows_after_half_a_second_of_moving_and_fades_in() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let before = sweep(&mut button, start, ms(480));
        assert!(!button.shown(), "not before half a second");
        let shown = sweep(&mut button, before + ms(16), ms(40));
        assert!(button.shown());
        let first = button.drawn(shown, SCALE).map(|drawn| drawn.strength);
        assert!(first.is_none_or(|strength| strength < 0.5), "it fades in: {first:?}");
        assert_eq!(button.next_change(shown), Some(shown + STEP), "drawn again in a step");
        let full = button.drawn(shown + FADE_IN, SCALE).expect("shown");
        assert_eq!((full.strength, full.lit), (1.0, false));
    }

    #[test]
    fn a_pause_in_the_moving_starts_the_count_again() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let paused = sweep(&mut button, start, ms(300));
        let end = sweep(&mut button, paused + ms(400), ms(300));
        assert!(!button.shown(), "300 ms, a pause, 300 ms: never half a second on end");
        sweep(&mut button, end + ms(16), ms(300));
        assert!(button.shown(), "and on");
    }

    #[test]
    fn a_pointer_that_reports_the_same_place_is_not_moving() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let mut at = start;
        while at <= start + ms(1000) {
            button.moved(at, (300.0, 300.0), W, SCALE);
            at += ms(16);
        }
        assert!(!button.shown());
    }

    #[test]
    fn the_x_fades_out_a_few_seconds_after_the_pointer_stops() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let last = sweep(&mut button, start, ms(700));
        assert_eq!(button.drawn(last + STAYS, SCALE).map(|drawn| drawn.strength), Some(1.0));
        assert_eq!(button.next_change(last + ms(500)), Some(last + STAYS), "one wake when it starts to go");
        let half = button.drawn(last + STAYS + FADE_OUT / 2, SCALE).expect("half gone");
        assert!((half.strength - 0.5).abs() < 0.05, "{half:?}");
        assert_eq!(button.next_change(last + STAYS + ms(100)), Some(last + STAYS + ms(116)));
        assert_eq!(button.drawn(last + STAYS + FADE_OUT, SCALE), None);
        button.settle(last + STAYS + FADE_OUT);
        assert!(!button.shown());
        assert_eq!(button.next_change(last + STAYS + FADE_OUT), None);
    }

    #[test]
    fn the_x_fades_out_sooner_once_the_pointer_leaves() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let last = sweep(&mut button, start, ms(700));
        button.left(last + ms(100));
        let gone = last + ms(100) + STAYS_AFTER_LEAVING + FADE_OUT;
        assert!(button.drawn(gone - ms(300), SCALE).is_some());
        assert_eq!(button.drawn(gone, SCALE), None);
        // Left and back: moving again keeps it, for a pointer just passing back over.
        let mut back = CloseButton::default();
        let last = sweep(&mut back, start, ms(700));
        back.left(last);
        back.moved(last + ms(200), (900.0, 400.0), W, SCALE);
        assert_eq!(back.drawn(last + ms(200) + STAYS_AFTER_LEAVING + FADE_OUT, SCALE).map(|d| d.strength), Some(1.0));
    }

    #[test]
    fn moving_again_as_it_fades_brings_it_back_from_where_it_was() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let last = sweep(&mut button, start, ms(700));
        let fading = last + STAYS + FADE_OUT / 2;
        button.moved(fading, (1000.0, 600.0), W, SCALE);
        let now = button.drawn(fading, SCALE).expect("still shown").strength;
        assert!((now - 0.5).abs() < 0.05, "no jump: {now}");
        assert_eq!(button.drawn(fading + FADE_IN, SCALE).map(|drawn| drawn.strength), Some(1.0));
        assert_eq!(button.drawn(fading + STAYS, SCALE).map(|drawn| drawn.strength), Some(1.0), "and stays again");
    }

    #[test]
    fn the_x_stays_while_the_pointer_rests_on_it_and_lights_up() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let last = sweep(&mut button, start, ms(700));
        button.moved(last + ms(16), on_x(), W, SCALE);
        let drawn = button.drawn(last + ms(16) + STAYS * 10, SCALE).expect("stays");
        assert!(drawn.lit);
        assert_eq!(button.next_change(last + ms(500)), None, "nothing to wake for while it rests there");
        // Off it again: unlit, and it goes in time.
        button.moved(last + ms(1000), (900.0, 400.0), W, SCALE);
        assert!(!button.drawn(last + ms(1000), SCALE).expect("shown").lit);
        assert_eq!(button.drawn(last + ms(1000) + STAYS + FADE_OUT, SCALE), None);
    }

    #[test]
    fn a_press_on_the_x_while_it_shows_switches_the_screen_off() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let last = sweep(&mut button, start, ms(1200));
        button.moved(last + ms(16), on_x(), W, SCALE);
        assert_eq!(button.pressed(last + ms(32), false), Press::SwitchOff);
        // Just outside the plate still counts; well outside it does not.
        let square = square(W, SCALE);
        let near = (f64::from(square.x) - 4.0, f64::from(square.y + square.side / 2.0));
        button.moved(last + ms(48), near, W, SCALE);
        assert_eq!(button.pressed(last + ms(64), false), Press::SwitchOff);
        button.moved(last + ms(80), (f64::from(square.x) - 30.0, 60.0), W, SCALE);
        assert_eq!(button.pressed(last + ms(96), false), Press::Nothing);
    }

    #[test]
    fn a_stray_press_in_the_corner_never_switches_the_screen_off() {
        let start = Instant::now();
        // The pointer comes to the corner without the X showing, and presses (a double click):
        // nothing, and no X.
        let mut button = CloseButton::default();
        button.moved(start, on_x(), W, SCALE);
        assert_eq!(button.pressed(start + ms(10), false), Press::Nothing);
        assert_eq!(button.pressed(start + ms(250), false), Press::Nothing);
        assert!(!button.shown(), "a click shows nothing where moving is how it comes");
        // On a Mac the first press shows it, and that is all: the second of a double click does
        // nothing either.
        let mut mac = CloseButton::default();
        mac.moved(start, on_x(), W, SCALE);
        assert_eq!(mac.pressed(start + ms(10), true), Press::Showed);
        assert!(mac.shown());
        assert_eq!(mac.pressed(start + ms(250), true), Press::Nothing, "a double click");
        // Once it has been on show a moment, the press counts.
        assert_eq!(mac.pressed(start + ms(10) + ARMED_AFTER, true), Press::SwitchOff);
        // Gone again (faded), a press shows it, nothing more.
        let mut gone = CloseButton::default();
        let last = sweep(&mut gone, start, ms(700));
        gone.moved(last + ms(16), on_x(), W, SCALE);
        gone.left(last + ms(32));
        let later = last + ms(32) + STAYS_AFTER_LEAVING + FADE_OUT;
        gone.moved(later, on_x(), W, SCALE);
        assert_eq!(gone.pressed(later + ms(5), false), Press::Nothing);
        assert_eq!(gone.pressed(later + ms(5), true), Press::Showed);
    }

    #[test]
    fn a_press_elsewhere_while_the_x_shows_keeps_it() {
        let start = Instant::now();
        let mut button = CloseButton::default();
        let last = sweep(&mut button, start, ms(700));
        let pressed = last + STAYS - ms(100);
        assert_eq!(button.pressed(pressed, false), Press::Nothing);
        assert_eq!(button.drawn(pressed + STAYS - ms(10), SCALE).map(|drawn| drawn.strength), Some(1.0));
    }

    #[test]
    fn the_x_is_brighter_under_the_pointer_and_never_see_through_at_full_strength() {
        let start = Instant::now();
        let drawn = Drawn { strength: 1.0, lit: false, scale: 1.0 };
        let lit = Drawn { lit: true, ..drawn };
        assert!(lit.look().cross[0] > drawn.look().cross[0]);
        assert!(lit.look().plate[3] > drawn.look().plate[3]);
        assert_eq!(drawn.look().strength, 1.0);
        // And strengths come in steps, so a fade sends each look once.
        let mut button = CloseButton::default();
        let last = sweep(&mut button, start, ms(700));
        let a = button.drawn(last + STAYS + ms(1), SCALE).expect("shown").strength;
        let b = button.drawn(last + STAYS + ms(2), SCALE).expect("shown").strength;
        assert_eq!(a, b);
    }
}
