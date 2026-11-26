//! How the surface of a window drawn with the interface toolkit (the operator's window, Screens,
//! Settings) is really set up, kept apart from the size its window has.
//!
//! Setting a surface up waits for the GPU to come idle, and is lost when the screens' thread
//! hands in work at that moment (see `output::configure`). A lost set-up leaves the surface as
//! it was: at the size before. The graphics library still gives the size asked for as the size
//! of the next picture, so nothing it says tells the two apart; whether the last set-up worked
//! is the only way to know, and it is kept here. A window is drawn only while its surface is set
//! up at the size the drawing is made for, and a set-up that was lost is tried again before the
//! next frame, as the screens' thread does for its own windows: once a frame at most, since each
//! try waits for the GPU the screens draw with (the window's next drawing is set a frame on).
//!
//! Without this the window's size was taken on before its surface had it: after a lost set-up
//! every frame was drawn for the new size onto a surface of the old one, which the GPU refuses
//! (a scissor rectangle larger than the picture) when the window had grown, and shows at the
//! wrong size when it had shrunk, until the window was next resized.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use sp_render::wgpu;

/// A surface's settings, and whether the surface really has them.
pub(crate) struct SetUp {
    config: wgpu::SurfaceConfiguration,
    done: bool,
}

impl SetUp {
    /// Sets the surface up for the first time. `set_up` does it and says whether it worked.
    pub(crate) fn new(
        config: wgpu::SurfaceConfiguration,
        set_up: impl FnOnce(&wgpu::SurfaceConfiguration) -> bool,
    ) -> Self {
        let done = set_up(&config);
        Self { config, done }
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

    /// True when the surface has to be set up before a window of this size is drawn: the window
    /// has another size, or the last set-up was lost. (A minimised window reports nought; the
    /// last real size is kept until it comes back.)
    pub(crate) fn behind(&self, window: (u32, u32)) -> bool {
        !self.done || (window.0 > 0 && window.1 > 0 && window != self.size())
    }

    /// Sets the surface up at its window's size if it is [`SetUp::behind`] it, once. Says
    /// whether the window can be drawn now; when not, the next frame tries again.
    pub(crate) fn follow(
        &mut self,
        window: (u32, u32),
        set_up: impl FnOnce(&wgpu::SurfaceConfiguration) -> bool,
    ) -> bool {
        if !self.behind(window) {
            return true;
        }
        if window.0 > 0 && window.1 > 0 {
            (self.config.width, self.config.height) = window;
        }
        self.again(set_up)
    }

    /// Sets the surface up again as it is (the system said its pictures no longer fit the
    /// window, or took it away). Says whether that worked.
    pub(crate) fn again(&mut self, set_up: impl FnOnce(&wgpu::SurfaceConfiguration) -> bool) -> bool {
        self.done = set_up(&self.config);
        self.done
    }

    /// The surface gave no picture and said its set-up is not valid: set up again before the
    /// next frame.
    pub(crate) fn lost(&mut self) {
        self.done = false;
    }
}

/// Sets up the surface of a window drawn with the interface toolkit, saying whether that worked.
pub(crate) fn set_up(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'static>,
    config: &wgpu::SurfaceConfiguration,
) -> bool {
    if race_aid_loses() {
        crate::applog::event("window", "Testing aid: a set-up of the operator's window's surface was lost");
        return false;
    }
    let done = crate::output::configure(device, surface, config);
    if done && race_aid_asked() > 0 && !RACE_OVER.swap(true, Ordering::Relaxed) {
        crate::applog::event(
            "window",
            &format!(
                "Testing aid: the operator's window's surface follows its window again ({} × {})",
                config.width, config.height
            ),
        );
    }
    done
}

/// Testing aid: SOFTPRESENTER_SURFACE_RACE=<n> has the operator's window's surface start at half
/// its window's size, as on Windows where a new window takes its size a moment after it opens,
/// and loses the first n set-ups that would bring it to the window's size (they are not made,
/// and are said to have failed), as when the screens' thread hands the GPU work at that moment
/// time after time. How many were asked for.
pub(crate) fn race_aid_asked() -> u32 {
    static ASKED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ASKED.get_or_init(|| {
        let asked = std::env::var("SOFTPRESENTER_SURFACE_RACE").ok().and_then(|n| n.trim().parse().ok()).unwrap_or(0);
        if asked > 0 {
            crate::applog::event(
                "window",
                &format!(
                    "Testing aid: the operator's window's surface starts at half its window's size, and its \
                     next {asked} set-ups are lost."
                ),
            );
        }
        asked
    })
}

/// Testing aid: says once, for SOFTPRESENTER_SURFACE_RACE, that a frame was drawn into the
/// operator's window's surface after its lost set-ups, and at what size, so a test knows the
/// window was really drawn (a window covered by others is given no picture on a Mac).
pub(crate) fn race_aid_drew(size: [u32; 2]) {
    if race_aid_asked() > 0 && RACE_OVER.load(Ordering::Relaxed) && !RACE_DREW.swap(true, Ordering::Relaxed) {
        crate::applog::event(
            "window",
            &format!(
                "Testing aid: the operator's window was drawn at {} × {} after its lost set-ups",
                size[0], size[1]
            ),
        );
    }
}

static RACE_DREW: AtomicBool = AtomicBool::new(false);
static RACE_LOST: AtomicU32 = AtomicU32::new(0);
static RACE_OVER: AtomicBool = AtomicBool::new(false);
/// False until the surface's first set-up, at half the window's size, has been made: that one
/// is never lost.
static RACE_STARTED: AtomicBool = AtomicBool::new(false);

/// True when the testing aid loses this set-up (see [`race_aid_asked`]).
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

    #[test]
    fn a_lost_set_up_is_not_drawn_into_and_is_tried_again_until_it_works() {
        // As on Windows' check: the window opened at 752 × 480 and took 960 × 632 a moment later.
        let mut surface = Surface { real: None, tries: 0 };
        let mut set_up = SetUp::new(config(752, 480), surface.set_up(true));
        assert!(set_up.drawable());
        assert!(!set_up.behind((752, 480)));
        assert!(set_up.behind((960, 632)));
        // The set-up at the new size is lost: the surface is still 752 × 480, so nothing may be
        // drawn for 960 × 632.
        assert!(!set_up.follow((960, 632), surface.set_up(false)));
        assert_eq!(surface.real, Some((752, 480)));
        assert!(!set_up.drawable());
        // The window has the size asked for, and the surface is still behind it: the next frame
        // tries again (this is what was missing).
        assert!(set_up.behind((960, 632)));
        assert!(!set_up.follow((960, 632), surface.set_up(false)));
        assert!(set_up.follow((960, 632), surface.set_up(true)));
        assert!(set_up.drawable());
        assert_eq!((set_up.size(), surface.real), ((960, 632), Some((960, 632))));
        // Nothing more to do: the surface is not set up again for a frame at the same size.
        let tries = surface.tries;
        assert!(set_up.follow((960, 632), surface.set_up(true)));
        assert_eq!(surface.tries, tries);
    }

    #[test]
    fn whenever_a_frame_may_be_drawn_the_surface_has_the_size_it_is_drawn_at() {
        // Sizes and outcomes in every order a short run of them can come in: windows growing,
        // shrinking, minimised (nought) and the same again, each set-up working or lost.
        let sizes = [(752, 480), (960, 632), (640, 400), (0, 0), (960, 632)];
        for first_works in [true, false] {
            for outcomes in 0..(1u32 << 6) {
                let mut surface = Surface { real: None, tries: 0 };
                let mut set_up = SetUp::new(config(752, 480), surface.set_up(first_works));
                for step in 0..6 {
                    let window = sizes[(step + outcomes as usize) % sizes.len()];
                    let works = outcomes & (1 << step) != 0;
                    let drawable = set_up.follow(window, surface.set_up(works));
                    assert_eq!(drawable, set_up.drawable());
                    if drawable {
                        assert_eq!(Some(set_up.size()), surface.real, "step {step} of {outcomes:#b}");
                        if window != (0, 0) {
                            assert_eq!(set_up.size(), window, "step {step} of {outcomes:#b}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_surface_that_says_its_set_up_is_not_valid_is_set_up_again() {
        let mut surface = Surface { real: None, tries: 0 };
        let mut set_up = SetUp::new(config(960, 632), surface.set_up(true));
        set_up.lost();
        assert!(!set_up.drawable());
        assert!(set_up.behind((960, 632)));
        assert!(set_up.follow((960, 632), surface.set_up(true)));
        assert_eq!(surface.tries, 2);
        // A minimised window keeps the size it had.
        set_up.lost();
        assert!(set_up.follow((0, 0), surface.set_up(true)));
        assert_eq!(set_up.size(), (960, 632));
    }
}
