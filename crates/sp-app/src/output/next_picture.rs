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
//! the newest picture once its next one has come: the window's thread says so when it does
//! (`came`), and the screens' thread, woken by that, copies the picture the window is owed into
//! it. While something moves, each drawing looks for it too, with a short pause, as it does
//! for a window that is covered.
//!
//! How long the screens' thread waits depends on who else is being drawn for. With windows
//! only, up to [`ASKED_WAIT`] for each, which is what paces a fade to the display. With an NDI
//! source somebody watches, or a DeckLink card, [`FED_WAIT`] in all for the windows of one
//! drawing: those are owed a picture every thirtieth of a second or oftener, by their own
//! clocks, and a display whose pictures come slowly (behind a splitter, on a slow connection,
//! at 24 or 30 Hz) must not make theirs late. Such a window is then passed over more often,
//! and shown its picture the moment its next one comes, as often as its display takes them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use sp_render::wgpu;

/// How long the screens' thread waits for a window's next picture when it asks for it. Longer
/// than a refresh of the slowest screen (a television at 24 Hz refreshes every 42 ms), so that a
/// window shown at its own pace is never passed over, and the wait is what paces a fade, as it
/// always was; short enough that NDI, the stage and DeckLink lose a picture or two at most.
pub(super) const ASKED_WAIT: Duration = Duration::from_millis(50);

/// How long it looks again, each drawing, for a picture that didn't come in [`ASKED_WAIT`]: the
/// pause a covered window has, so that a fade doesn't spin meanwhile.
pub(super) const STILL_WAIT: Duration = Duration::from_millis(8);

/// How long, from when it began, one pass of the screens' thread waits for its windows' next
/// pictures in all while NDI sources somebody watches or DeckLink cards are drawn for too: one
/// refresh of a 60 Hz display, which is all a window on one needs, and half a picture's time at
/// the thirty a second NDI sends, so a pass held this long still gives NDI and DeckLink each
/// picture well within two pictures' times of the one before.
pub(super) const FED_WAIT: Duration = Duration::from_millis(16);

/// The least a window not yet asked is waited for then, however long the pass has taken
/// already: a picture that is ready comes within this, so a window whose display is keeping
/// up is not passed over only because drawing took long.
pub(super) const LEAST_WAIT: Duration = Duration::from_millis(2);

/// How long the screens' thread waits now for a window's next picture: one asked for before
/// (`waiting`) or asked for now, in a pass that may wait for its windows until `until` (`None`
/// when only windows are drawn for, and the wait is what it always was).
pub(super) fn wait_for(waiting: bool, until: Option<Instant>, now: Instant) -> Duration {
    let usual = if waiting { STILL_WAIT } else { ASKED_WAIT };
    let Some(until) = until else { return usual };
    let least = if waiting { Duration::ZERO } else { LEAST_WAIT };
    usual.min(until.saturating_duration_since(now)).max(least)
}

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
    /// for has come (and is let go too). With `slow`, every picture asked for comes that much
    /// later (a testing aid: see `super::slow_present`). `came` is called on the window's
    /// thread when a picture the screens' thread stopped waiting for has come: it should only
    /// pass the word on and return.
    pub(super) fn new(
        surface: Arc<wgpu::Surface<'static>>,
        slow: Option<Duration>,
        came: impl Fn() + Send + 'static,
    ) -> NextPicture {
        let theirs = surface.clone();
        let ask = move || {
            if let Some(slow) = slow {
                std::thread::sleep(slow);
            }
            super::next_screen_texture(&theirs)
        };
        NextPicture { asker: Asker::new("window pictures", ask, came), surface }
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
    /// The asker stopped waiting for the one asked for: the thread says when it has come.
    tell: Arc<AtomicBool>,
}

impl<T: Send + 'static> Asker<T> {
    /// `came` is called on the thread, after it has given what was asked for, when the asker
    /// had stopped waiting for it.
    fn new(name: &str, ask: impl Fn() -> T + Send + Sync + 'static, came: impl Fn() + Send + 'static) -> Asker<T> {
        let ask: Arc<dyn Fn() -> T + Send + Sync> = Arc::new(ask);
        let (asks, asked) = channel::<()>();
        let (give, given) = channel();
        let theirs = ask.clone();
        let tell = Arc::new(AtomicBool::new(false));
        let told = tell.clone();
        let started = std::thread::Builder::new().name(name.to_owned()).spawn(move || {
            for () in asked {
                if give.send(theirs()).is_err() {
                    return;
                }
                // Looked at after it is given: an asker that stops waiting at this very moment
                // either finds it (it looks once more after saying so) or is told.
                if told.swap(false, Ordering::AcqRel) {
                    came();
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
        Asker { ask, thread, asked: false, tell }
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
                // Taken here: nobody need be told of it, if it was looked for before.
                self.tell.store(false, Ordering::Release);
                Some(came)
            }
            Err(RecvTimeoutError::Timeout) => {
                // Not waited for any longer: the thread is to say when it comes. It may have
                // come this instant, before the thread could see that: looked for once more.
                self.tell.store(true, Ordering::Release);
                let came = given.try_recv().ok()?;
                self.tell.store(false, Ordering::Release);
                self.asked = false;
                Some(came)
            }
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
        let (tell, told) = channel::<()>();
        let ask = move || {
            let _ = held_back.lock().unwrap().recv();
            let mut count = count.lock().unwrap();
            *count += 1;
            *count
        };
        let mut asker = Asker::new("test pictures", ask, move || {
            let _ = tell.send(());
        });
        // Not come: the asker is held for the wait it gave, and no longer.
        let began = Instant::now();
        assert_eq!(asker.take(Duration::from_millis(50)), None);
        let held = began.elapsed();
        assert!(held >= Duration::from_millis(50) && held < Duration::from_millis(500), "held {held:?}");
        assert!(asker.waiting(), "still asked for");
        // Looked for again: the same one is waited for, not another asked.
        assert_eq!(asker.take(Duration::from_millis(8)), None);
        // It comes: the asker, which had stopped waiting, is told, and takes it without
        // waiting at all; it is then free to ask again.
        assert!(told.try_recv().is_err(), "nothing to tell of yet");
        let_go.send(()).unwrap();
        told.recv_timeout(Duration::from_secs(5)).expect("told that it has come");
        assert_eq!(asker.take(Duration::ZERO), Some(1));
        assert!(!asker.waiting());
        // One that comes while it is waited for is only taken: nobody is told.
        let_go.send(()).unwrap();
        assert_eq!(asker.take(Duration::from_secs(5)), Some(2), "one asked for at a time");
        assert!(told.recv_timeout(Duration::from_millis(50)).is_err(), "it was waited for");
    }

    #[test]
    fn windows_are_waited_for_less_while_ndi_or_decklink_are_drawn_for_too() {
        let now = Instant::now();
        let ms = Duration::from_millis;
        // Windows only: as long as ever.
        assert_eq!(wait_for(false, None, now), ASKED_WAIT);
        assert_eq!(wait_for(true, None, now), STILL_WAIT);
        // With others drawn for: until the pass has waited its share, however many windows.
        assert_eq!(wait_for(false, Some(now + FED_WAIT), now), FED_WAIT);
        assert_eq!(wait_for(false, Some(now + ms(5)), now), ms(5));
        assert_eq!(wait_for(true, Some(now + FED_WAIT), now), STILL_WAIT);
        assert_eq!(wait_for(true, Some(now + ms(3)), now), ms(3));
        // Past that, one not asked yet is still given a moment (a picture that is ready comes
        // within it), and one asked for before is only looked for.
        assert_eq!(wait_for(false, Some(now), now + ms(30)), LEAST_WAIT);
        assert_eq!(wait_for(true, Some(now), now + ms(30)), Duration::ZERO);
        assert!(FED_WAIT >= STILL_WAIT, "a pass with a window to look for always pauses, so a fade never spins");
    }

    #[test]
    fn a_picture_that_is_ready_comes_at_once() {
        let mut asker = Asker::new("test pictures", || 7, || {});
        let began = Instant::now();
        assert_eq!(asker.take(Duration::from_secs(5)), Some(7));
        assert!(began.elapsed() < Duration::from_secs(1));
        assert_eq!(asker.take(Duration::from_secs(5)), Some(7));
    }
}
