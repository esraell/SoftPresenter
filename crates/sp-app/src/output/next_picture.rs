//! A screen window's next picture to draw into, asked for on a thread of the window's own.
//!
//! The window system lends a window the few pictures it draws into and takes each back once it
//! has shown it. When it isn't showing the window for a moment, it doesn't take them back, and
//! asking for the next one waits until it does: on a Mac, up to about a second (the graphics
//! library only checks first that the window is not covered). Asked on the screens' thread,
//! that wait held every screen with the one window: NDI, the stage, DeckLink and the operator's
//! preview all stood still for the second. So each window asks on a thread of its own, and the
//! screens' thread waits a short while at most. A window whose picture doesn't come in that
//! time is passed over, keeping the picture it shows (as it would have anyway), and is given
//! the newest picture once its next one has come; meanwhile the screens' thread looks for it
//! with a short pause each drawing, as it does for a window that is covered.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

use sp_render::wgpu;

/// How long the screens' thread waits for a window's next picture when it asks for it. Longer
/// than a refresh of the slowest screen (a television at 24 Hz refreshes every 42 ms), so that a
/// window shown at its own pace is never passed over, and the wait is what paces a fade, as it
/// always was; short enough that NDI, the stage and DeckLink lose a picture or two at most.
pub(super) const ASKED_WAIT: Duration = Duration::from_millis(50);

/// How long it looks again, each drawing, for a picture that didn't come in [`ASKED_WAIT`]: the
/// pause a covered window has, so that a fade doesn't spin meanwhile.
pub(super) const STILL_WAIT: Duration = Duration::from_millis(8);

/// What came of looking for a window's next picture.
pub(super) enum Next {
    /// What the window system gave: a picture, or why there is none. `None` when the window's
    /// surface isn't set up (see `super::next_texture`).
    Came(Option<wgpu::CurrentSurfaceTexture>),
    /// Asked for, and not come yet: the window's thread is still waiting for it.
    NotYet,
}

/// A window's surface, and its next picture, asked for on a thread of its own.
pub(super) struct NextPicture {
    surface: Arc<wgpu::Surface<'static>>,
    asker: Asker<Option<wgpu::CurrentSurfaceTexture>>,
}

impl NextPicture {
    /// Starts the window's thread. It ends once this is let go and any picture it is waiting
    /// for has come (and is let go too).
    pub(super) fn new(surface: Arc<wgpu::Surface<'static>>) -> NextPicture {
        let theirs = surface.clone();
        NextPicture { asker: Asker::new("window pictures", move || super::next_screen_texture(&theirs)), surface }
    }

    /// The surface the window is drawn on.
    pub(super) fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    /// True while a picture asked for hasn't come. The surface is not set up again meanwhile:
    /// that would wait for it.
    pub(super) fn waiting(&self) -> bool {
        self.asker.waiting()
    }

    /// Asks for the window's next picture, unless one is asked for already, and waits for it
    /// at most `wait`.
    pub(super) fn take(&mut self, wait: Duration) -> Next {
        match self.asker.take(wait) {
            Some(came) => Next::Came(came),
            None => Next::NotYet,
        }
    }
}

/// Something asked for on a thread of its own, one at a time, and waited for a while at most.
struct Asker<T> {
    ask: Arc<dyn Fn() -> T + Send + Sync>,
    /// Where the thread is asked, and where it gives what came; `None` when the thread could not
    /// be started, and it is asked here.
    thread: Option<(Sender<()>, Receiver<T>)>,
    /// One has been asked for and hasn't come.
    asked: bool,
}

impl<T: Send + 'static> Asker<T> {
    fn new(name: &str, ask: impl Fn() -> T + Send + Sync + 'static) -> Asker<T> {
        let ask: Arc<dyn Fn() -> T + Send + Sync> = Arc::new(ask);
        let (asks, asked) = channel::<()>();
        let (give, given) = channel();
        let theirs = ask.clone();
        let started = std::thread::Builder::new().name(name.to_owned()).spawn(move || {
            for () in asked {
                if give.send(theirs()).is_err() {
                    return;
                }
            }
        });
        let thread = match started {
            Ok(_) => Some((asks, given)),
            Err(e) => {
                crate::applog::problem(
                    "output",
                    &format!("Could not start the {name} thread, so they are asked here: {e}"),
                );
                None
            }
        };
        Asker { ask, thread, asked: false }
    }

    fn waiting(&self) -> bool {
        self.asked
    }

    /// What was asked for, once it has come; asked for now unless it is already, and waited for
    /// at most `wait`. `None` while it hasn't come.
    fn take(&mut self, wait: Duration) -> Option<T> {
        let Some((asks, given)) = &self.thread else { return Some((self.ask)()) };
        if !self.asked {
            if asks.send(()).is_err() {
                // The thread has gone (it never ends while this is kept): asked here.
                return Some((self.ask)());
            }
            self.asked = true;
        }
        match given.recv_timeout(wait) {
            Ok(came) => {
                self.asked = false;
                Some(came)
            }
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                self.asked = false;
                Some((self.ask)())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;

    #[test]
    fn a_picture_that_does_not_come_holds_the_asker_a_short_while_and_comes_later() {
        // Each picture asked for comes only once the test lets it, numbered.
        let (let_go, held_back) = channel::<()>();
        let held_back = Mutex::new(held_back);
        let count = Mutex::new(0);
        let mut asker = Asker::new("test pictures", move || {
            let _ = held_back.lock().unwrap().recv();
            let mut count = count.lock().unwrap();
            *count += 1;
            *count
        });
        // Not come: the asker is held for the wait it gave, and no longer.
        let began = Instant::now();
        assert_eq!(asker.take(Duration::from_millis(50)), None);
        let held = began.elapsed();
        assert!(held >= Duration::from_millis(50) && held < Duration::from_millis(500), "held {held:?}");
        assert!(asker.waiting(), "still asked for");
        // Looked for again: the same one is waited for, not another asked.
        assert_eq!(asker.take(Duration::from_millis(8)), None);
        // It comes: taken at the next look, and the asker is free to ask again.
        let_go.send(()).unwrap();
        assert_eq!(asker.take(Duration::from_secs(5)), Some(1));
        assert!(!asker.waiting());
        let_go.send(()).unwrap();
        assert_eq!(asker.take(Duration::from_secs(5)), Some(2), "one asked for at a time");
    }

    #[test]
    fn a_picture_that_is_ready_comes_at_once() {
        let mut asker = Asker::new("test pictures", || 7);
        let began = Instant::now();
        assert_eq!(asker.take(Duration::from_secs(5)), Some(7));
        assert!(began.elapsed() < Duration::from_secs(1));
        assert_eq!(asker.take(Duration::from_secs(5)), Some(7));
    }
}
