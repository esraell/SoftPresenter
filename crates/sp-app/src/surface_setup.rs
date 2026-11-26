//! How the surface of a window drawn with the interface toolkit (the operator's window, Screens,
//! Settings) is really set up, kept apart from the size its window has.
//!
//! Setting a surface up waits for the GPU to come idle, and is lost when the screens' thread
//! hands in work at that moment (see `output::configure`). A lost set-up leaves the surface as
//! it was: at the size before. The graphics library still gives the size asked for as the size
//! of the next picture, so nothing it says tells the two apart; whether the last set-up worked
//! is the only way to know, and it is kept here. A window is drawn only while its surface is set
//! up at the size the drawing is made for, and a set-up that was lost is tried again at a later
//! drawing, as the screens' thread does for its own windows.
//!
//! Without this the window's size was taken on before its surface had it: after a lost set-up
//! every frame was drawn for the new size onto a surface of the old one, which the GPU refuses
//! (a scissor rectangle larger than the picture) when the window had grown, and shows at the
//! wrong size when it had shrunk, until the window was next resized.
//!
//! How often it is tried again. Each try waits for the GPU with the device held for reading,
//! and a screen's window showing its next picture takes the device for writing, so a try that
//! waits can hold the audience's pictures up. A try is therefore never made sooner than a frame
//! after the one before, and each failure in a row doubles the wait, up to a second
//! ([`SetUp::next_try`]): a set-up that keeps failing (the graphics device was lost, the driver
//! refuses the surface) costs a try a second, not a window laid out sixty times a second. The
//! count begins again once the surface gives a picture, or says the window is only covered. A
//! surface that says it must be set up again (its pictures no longer fit the window, or its
//! set-up is not valid) is counted the same way, so a device that was lost, whose set-ups all
//! seem to work and whose surface then refuses every picture, backs off too.
//!
//! A surface can be no larger than the device's largest texture. A window larger than that has
//! its surface set up at that size, laid out for it, and the system stretches the picture.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use sp_render::wgpu;

/// The least time between two tries: a frame at sixty a second.
const FRAME: Duration = Duration::from_micros(16_667);
/// The longest wait between two tries of a set-up that keeps failing.
const LONGEST_WAIT: Duration = Duration::from_secs(1);

/// A surface's settings, and whether the surface really has them.
pub(crate) struct SetUp {
    config: wgpu::SurfaceConfiguration,
    /// The longest side a surface may have (the device's largest texture).
    longest_side: u32,
    done: bool,
    /// Failures in a row: set-ups that did not work, and pictures the surface refused. Nought
    /// again once the surface answers as it should.
    failures: u32,
    /// When a set-up was last tried.
    tried: Option<Instant>,
}

impl SetUp {
    /// Sets the surface up for the first time. `set_up` does it and says whether it worked.
    pub(crate) fn new(
        mut config: wgpu::SurfaceConfiguration,
        longest_side: u32,
        now: Instant,
        set_up: impl FnOnce(&wgpu::SurfaceConfiguration) -> bool,
    ) -> Self {
        let longest_side = longest_side.max(1);
        config.width = config.width.clamp(1, longest_side);
        config.height = config.height.clamp(1, longest_side);
        let done = set_up(&config);
        Self { config, longest_side, done, failures: u32::from(!done), tried: Some(now) }
    }

    /// The settings asked for. The surface has them only while [`SetUp::drawable`].
    pub(crate) fn config(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    /// The size in pixels a frame is drawn at.
    pub(crate) fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// True while the surface is set up at [`SetUp::size`]: only then is a frame drawn into it.
    pub(crate) fn drawable(&self) -> bool {
        self.done
    }

    /// The size the surface of a window of this size has: the window's, each side no longer
    /// than a surface may be. `None` for a minimised window, which reports nought: the last
    /// real size is kept until it comes back.
    fn wanted(&self, window: (u32, u32)) -> Option<(u32, u32)> {
        (window.0 > 0 && window.1 > 0).then(|| (window.0.min(self.longest_side), window.1.min(self.longest_side)))
    }

    /// True when the surface has to be set up before a window of this size is drawn: the window
    /// has another size, or the surface is not set up.
    pub(crate) fn behind(&self, window: (u32, u32)) -> bool {
        !self.done || self.wanted(window).is_some_and(|wanted| wanted != self.size())
    }

    /// How long after the last try the next may be made.
    fn wait(&self, window: (u32, u32)) -> Duration {
        match self.failures {
            0 => Duration::ZERO,
            // A size not tried yet is tried a frame on, so a window being dragged follows.
            _ if self.wanted(window).is_some_and(|wanted| wanted != self.size()) => FRAME,
            failures => FRAME.saturating_mul(1 << (failures - 1).min(6)).min(LONGEST_WAIT),
        }
    }

    /// When the surface of a window of this size is next to be set up, if it is
    /// [`SetUp::behind`]: for setting the window's next drawing by. Never sooner than a frame
    /// from now, so whatever goes wrong the window is not laid out time after time.
    pub(crate) fn next_try(&self, window: (u32, u32), now: Instant) -> Option<Instant> {
        if !self.behind(window) {
            return None;
        }
        let due = self.tried.and_then(|tried| tried.checked_add(self.wait(window))).unwrap_or(now);
        Some(due.max(now + FRAME))
    }

    /// True when the surface is [`SetUp::behind`] a window of this size and a set-up may be
    /// tried now.
    pub(crate) fn due(&self, window: (u32, u32), now: Instant) -> bool {
        self.behind(window) && self.tried.is_none_or(|tried| now.saturating_duration_since(tried) >= self.wait(window))
    }

    /// Sets the surface up at its window's size if that is [`SetUp::due`], once. Says whether a
    /// frame laid out for the window's size can be drawn now.
    pub(crate) fn follow(
        &mut self,
        window: (u32, u32),
        now: Instant,
        set_up: impl FnOnce(&wgpu::SurfaceConfiguration) -> bool,
    ) -> bool {
        if !self.behind(window) {
            return true;
        }
        if !self.due(window, now) {
            return false;
        }
        if let Some(wanted) = self.wanted(window) {
            (self.config.width, self.config.height) = wanted;
        }
        self.tried = Some(now);
        self.done = set_up(&self.config);
        if !self.done {
            self.failures = self.failures.saturating_add(1);
        }
        self.done
    }

    /// The surface gave no picture and said why it must be set up again: its pictures no longer
    /// fit the window, the system took it away, or its set-up is not valid. It is set up again
    /// at a later drawing.
    pub(crate) fn refused(&mut self) {
        self.done = false;
        self.failures = self.failures.saturating_add(1);
    }

    /// The surface gave a picture, or said only that the window is covered: it is as it should
    /// be, and what went wrong before is over.
    pub(crate) fn answered(&mut self) {
        self.failures = 0;
    }
}

/// Which window a surface belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Of {
    /// The operator's (home) window.
    Operator,
    /// Screens or Settings.
    Panel,
}

/// Sets up the surface of a window drawn with the interface toolkit, saying whether that worked.
pub(crate) fn set_up(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'static>,
    config: &wgpu::SurfaceConfiguration,
    of: Of,
) -> bool {
    if of == Of::Panel {
        return crate::output::configure(device, surface, config);
    }
    if race_aid_loses() {
        crate::applog::event("window", "Testing aid: a set-up of the operator's window's surface was lost");
        return false;
    }
    let done = crate::output::configure(device, surface, config);
    // Said only for a set-up that worked after the ones the aid lost, never for the first.
    if done
        && race_aid_asked() > 0
        && RACE_LOST.load(Ordering::Relaxed) >= race_aid_asked()
        && !RACE_OVER.swap(true, Ordering::Relaxed)
    {
        crate::applog::event(
            "window",
            &format!(
                "Testing aid: the operator's window's surface follows its window again, at {}x{}",
                config.width, config.height
            ),
        );
    }
    done
}

/// Testing aid: SOFTPRESENTER_SURFACE_RACE=<n> has the operator's window's surface start at half
/// its window's size, as on Windows where a new window takes its size a moment after it opens,
/// and loses the next n set-ups of that window's surface (they are not made, and are said to
/// have failed), as when the screens' thread hands the GPU work at that moment time after
/// time. Screens and Settings are left alone. How many were asked for.
pub(crate) fn race_aid_asked() -> u32 {
    static ASKED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ASKED.get_or_init(|| {
        std::env::var("SOFTPRESENTER_SURFACE_RACE").ok().and_then(|n| n.trim().parse().ok()).unwrap_or(0)
    })
}

/// Testing aid: the size the operator's window's surface starts at for
/// SOFTPRESENTER_SURFACE_RACE, half its window's, said in the log; the window's own size
/// without the aid.
pub(crate) fn race_aid_start(window: (u32, u32)) -> (u32, u32) {
    let asked = race_aid_asked();
    if asked == 0 {
        return window;
    }
    let half = ((window.0 / 2).max(1), (window.1 / 2).max(1));
    crate::applog::event(
        "window",
        &format!(
            "Testing aid: the operator's window's surface starts at {}x{}, half its window's {}x{}, and its next \
             {asked} set-ups are lost.",
            half.0, half.1, window.0, window.1
        ),
    );
    half
}

/// Testing aid: says once, for SOFTPRESENTER_SURFACE_RACE, that a frame was drawn into the
/// operator's window's surface after its lost set-ups, and at what size, so a test knows the
/// window was really drawn (a window covered by others is given no picture on a Mac).
pub(crate) fn race_aid_drew(of: Of, size: [u32; 2]) {
    if of == Of::Operator
        && race_aid_asked() > 0
        && RACE_OVER.load(Ordering::Relaxed)
        && !RACE_DREW.swap(true, Ordering::Relaxed)
    {
        crate::applog::event(
            "window",
            &format!("Testing aid: the operator's window was drawn at {}x{} after its lost set-ups", size[0], size[1]),
        );
    }
}

static RACE_DREW: AtomicBool = AtomicBool::new(false);
static RACE_LOST: AtomicU32 = AtomicU32::new(0);
static RACE_OVER: AtomicBool = AtomicBool::new(false);
/// False until the surface's first set-up, at half the window's size, has been made: that one
/// is never lost.
static RACE_STARTED: AtomicBool = AtomicBool::new(false);

/// True when the testing aid loses this set-up of the operator's window's surface (see
/// [`race_aid_asked`]).
fn race_aid_loses() -> bool {
    let asked = race_aid_asked();
    if asked == 0 || !RACE_STARTED.swap(true, Ordering::Relaxed) {
        return false;
    }
    RACE_LOST.try_update(Ordering::Relaxed, Ordering::Relaxed, |lost| (lost < asked).then_some(lost + 1)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(w: u32, h: u32) -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::AutoVsync,
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        }
    }

    /// A stand-in for the surface: the size it really has, which only a set-up that works
    /// changes.
    struct Surface {
        real: Option<(u32, u32)>,
        tries: u32,
    }

    impl Surface {
        fn set_up(&mut self, works: bool) -> impl FnOnce(&wgpu::SurfaceConfiguration) -> bool + '_ {
            move |config| {
                self.tries += 1;
                if works {
                    self.real = Some((config.width, config.height));
                }
                works
            }
        }
    }

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn a_lost_set_up_is_not_drawn_into_and_is_tried_again_until_it_works() {
        // As on Windows' check: the window opened at 752 × 480 and took 960 × 632 a moment later.
        let start = Instant::now();
        let mut surface = Surface { real: None, tries: 0 };
        let mut set_up = SetUp::new(config(752, 480), 8192, start, surface.set_up(true));
        assert!(set_up.drawable());
        assert!(!set_up.behind((752, 480)));
        assert_eq!(set_up.next_try((752, 480), start), None);
        assert!(set_up.behind((960, 632)));
        // The set-up at the new size is lost: the surface is still 752 × 480, so nothing may be
        // drawn for 960 × 632.
        assert!(!set_up.follow((960, 632), start, surface.set_up(false)));
        assert_eq!(surface.real, Some((752, 480)));
        assert!(!set_up.drawable());
        // The window has the size asked for, and the surface is still behind it: it is tried
        // again (this is what was missing), a frame later and no sooner.
        assert!(set_up.behind((960, 632)));
        assert!(!set_up.follow((960, 632), start + FRAME / 2, surface.set_up(true)));
        assert_eq!(surface.tries, 2, "not tried within the frame");
        assert_eq!(set_up.next_try((960, 632), start), Some(start + FRAME));
        assert!(set_up.follow((960, 632), start + FRAME, surface.set_up(true)));
        assert!(set_up.drawable());
        assert_eq!((set_up.size(), surface.real), ((960, 632), Some((960, 632))));
        // Nothing more to do: the surface is not set up again for a frame at the same size.
        let tries = surface.tries;
        assert!(set_up.follow((960, 632), start + SECOND, surface.set_up(true)));
        assert_eq!(surface.tries, tries);
    }

    #[test]
    fn whenever_a_frame_may_be_drawn_the_surface_has_the_size_it_is_drawn_at() {
        // Sizes and outcomes in every order a short run of them can come in: windows growing,
        // shrinking, minimised (nought), larger than a surface may be and the same again, each
        // set-up working or lost.
        let sizes = [(752, 480), (960, 632), (640, 400), (0, 0), (960, 632), (9000, 500)];
        let start = Instant::now();
        for first_works in [true, false] {
            for outcomes in 0..(1u32 << 7) {
                let mut surface = Surface { real: None, tries: 0 };
                let mut set_up = SetUp::new(config(752, 480), 8192, start, surface.set_up(first_works));
                for step in 0..7 {
                    let window = sizes[(step + outcomes as usize) % sizes.len()];
                    let works = outcomes & (1 << step) != 0;
                    // Long enough after the step before that a try is always due.
                    let now = start + SECOND * (step as u32 + 1) * 2;
                    let follows = set_up.follow(window, now, surface.set_up(works));
                    if set_up.drawable() {
                        assert_eq!(Some(set_up.size()), surface.real, "step {step} of {outcomes:#b}");
                    }
                    if follows {
                        assert!(set_up.drawable());
                        if window != (0, 0) {
                            let fits = (window.0.min(8192), window.1.min(8192));
                            assert_eq!(set_up.size(), fits, "step {step} of {outcomes:#b}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_set_up_that_keeps_failing_is_tried_less_and_less_often_up_to_once_a_second() {
        let start = Instant::now();
        let mut surface = Surface { real: None, tries: 0 };
        let mut set_up = SetUp::new(config(960, 632), 8192, start, surface.set_up(false));
        // Looked at every millisecond for ten seconds, as a window drawn flat out would.
        let mut tried_at = Vec::new();
        for ms in 1..=10_000 {
            let now = start + Duration::from_millis(ms);
            let before = surface.tries;
            set_up.follow((960, 632), now, surface.set_up(false));
            if surface.tries > before {
                tried_at.push(ms);
            }
            // The next drawing is never asked for sooner than a frame on.
            assert!(set_up.next_try((960, 632), now).is_some_and(|at| at >= now + FRAME));
        }
        assert_eq!(&tried_at[..7], &[17, 51, 118, 252, 519, 1053, 2053], "each wait twice the one before");
        assert!(tried_at.len() <= 16, "about one a second after that: {tried_at:?}");
        assert!(tried_at.windows(2).all(|pair| pair[1] - pair[0] >= 16), "never within a frame: {tried_at:?}");
        // A new size is tried a frame after the last try, however long the wait had grown: a
        // window being dragged is followed.
        let last = start + Duration::from_millis(*tried_at.last().unwrap());
        let now = last + SECOND;
        assert!(set_up.follow((960, 632), now, surface.set_up(false)) || !set_up.drawable());
        assert!(!set_up.due((960, 632), now + SECOND / 2), "the same size waits its second");
        assert!(!set_up.due((1000, 700), now + FRAME / 2));
        assert!(set_up.due((1000, 700), now + FRAME));
        // Once it works and the surface gives a picture, the count begins again.
        assert!(set_up.follow((1000, 700), now + SECOND, surface.set_up(true)));
        set_up.answered();
        assert!(set_up.due((1100, 700), now + SECOND), "a healthy surface follows its window at once");
    }

    #[test]
    fn a_surface_that_refuses_every_picture_backs_off_though_its_set_ups_seem_to_work() {
        // A graphics device that was lost: every set-up says it worked (the graphics library
        // keeps that error back), and the surface then gives no picture.
        let start = Instant::now();
        let mut surface = Surface { real: None, tries: 0 };
        let mut set_up = SetUp::new(config(960, 632), 8192, start, surface.set_up(true));
        set_up.answered();
        let mut tried_at = Vec::new();
        for ms in 1..=10_000 {
            let now = start + Duration::from_millis(ms);
            let before = surface.tries;
            if set_up.follow((960, 632), now, surface.set_up(true)) {
                // Drawn: the surface is asked for its picture and refuses.
                set_up.refused();
            }
            if surface.tries > before {
                tried_at.push(ms);
            }
            assert!(set_up.next_try((960, 632), now).is_some_and(|at| at >= now + FRAME));
        }
        assert!(tried_at.len() <= 18, "{tried_at:?}");
        assert!(tried_at[tried_at.len() - 2..].windows(2).all(|pair| pair[1] - pair[0] >= 1000), "{tried_at:?}");
    }

    #[test]
    fn a_window_larger_than_a_surface_may_be_is_drawn_at_the_largest_size() {
        let start = Instant::now();
        let mut surface = Surface { real: None, tries: 0 };
        let mut set_up = SetUp::new(config(960, 632), 8192, start, surface.set_up(true));
        assert!(set_up.follow((11_520, 2160), start, surface.set_up(true)));
        assert_eq!((set_up.size(), surface.real), ((8192, 2160), Some((8192, 2160))));
        // It has what it can have: not behind its window, and not set up again.
        assert!(!set_up.behind((11_520, 2160)));
        assert_eq!(surface.tries, 2);
        // A minimised window keeps the size it had.
        set_up.refused();
        assert!(set_up.follow((0, 0), start + SECOND, surface.set_up(true)));
        assert_eq!(set_up.size(), (8192, 2160));
    }
}
