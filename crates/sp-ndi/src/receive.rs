//! Finding NDI sources on the network and receiving their pictures, on threads of their own:
//! nothing here waits on NDI or the network on the thread that asks. The NDI runtime is the one
//! [`crate::installed`] looks for and sending opens, found the same way.
//!
//! A receiver asks NDI for the newest picture and hands it to a function of the caller's, which
//! copies what it wants and returns; the picture is freed straight after. Nothing is queued, so
//! what is shown is never older than the one picture being handled.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::runtime;

/// One received picture, lent for the length of the call it is handed to.
pub use runtime::VideoRef as Video;

/// What a [`Finder`] is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinderState {
    Starting,
    /// Looking; `sources` is what it has found so far.
    Looking,
    /// It could not start (no runtime, say), in plain words.
    Failed(String),
}

#[derive(Default)]
struct FinderShared {
    sources: Mutex<Vec<String>>,
    state: Mutex<Option<FinderState>>,
    stop: AtomicBool,
    again: AtomicBool,
}

/// Looks for NDI sources on the network while it is held. Dropping it stops looking.
pub struct Finder {
    shared: Arc<FinderShared>,
    _thread: JoinHandle<()>,
}

impl Finder {
    pub fn start() -> Finder {
        let shared = Arc::new(FinderShared::default());
        let own = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("ndi-find".to_owned())
            .spawn(move || find_loop(&own))
            .unwrap_or_else(|_| std::thread::spawn(|| {}));
        Finder { shared, _thread: thread }
    }

    /// The sources found, as NDI names them (`"COMPUTER (Source)"`), in order. Never waits.
    pub fn sources(&self) -> Vec<String> {
        self.shared.sources.lock().map(|sources| sources.clone()).unwrap_or_default()
    }

    pub fn state(&self) -> FinderState {
        self.shared.state.lock().ok().and_then(|state| state.clone()).unwrap_or(FinderState::Starting)
    }

    /// Starts looking afresh, as if the finder had just been made.
    pub fn look_again(&self) {
        self.shared.again.store(true, Ordering::Relaxed);
    }
}

impl Drop for Finder {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn find_loop(shared: &FinderShared) {
    let set_state = |now: FinderState| {
        if let Ok(mut state) = shared.state.lock() {
            *state = Some(now);
        }
    };
    let runtime = match runtime::get() {
        Ok(runtime) => runtime,
        Err(problem) => return set_state(FinderState::Failed(problem)),
    };
    while !shared.stop.load(Ordering::Relaxed) {
        let finder = match runtime::Finder::new(runtime) {
            Ok(finder) => finder,
            Err(problem) => return set_state(FinderState::Failed(problem)),
        };
        set_state(FinderState::Looking);
        if let Ok(mut sources) = shared.sources.lock() {
            sources.clear();
        }
        loop {
            // A short wait, so a stop or a look again is noticed within a moment.
            let changed = finder.wait(250);
            if shared.stop.load(Ordering::Relaxed) {
                return;
            }
            if shared.again.swap(false, Ordering::Relaxed) {
                break;
            }
            if changed {
                let mut found = finder.sources();
                found.sort();
                if let Ok(mut sources) = shared.sources.lock() {
                    *sources = found;
                }
            }
        }
    }
}

/// What an [`Incoming`] is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IncomingState {
    /// Looking for the runtime and connecting.
    Starting,
    /// Connected to something that is sending.
    Receiving,
    /// No longer connected: the source went away, or never answered.
    Waiting,
    /// It could not start, in plain words.
    Failed(String),
}

struct IncomingShared {
    state: Mutex<IncomingState>,
    stop: AtomicBool,
}

/// One source's pictures, received on a thread of its own while this is held. Dropping it lets
/// the source go.
pub struct Incoming {
    shared: Arc<IncomingShared>,
}

impl Incoming {
    /// Starts receiving `source` (an NDI name as the finder gives it): the full stream, or with
    /// `light` the sender's lighter one. `on_video` is called on the receiver's thread with each
    /// picture as it arrives, and should only copy what it needs and return; pictures that arrive
    /// while it works are not kept.
    pub fn start(source: &str, light: bool, mut on_video: impl FnMut(&Video<'_>) + Send + 'static) -> Incoming {
        let shared =
            Arc::new(IncomingShared { state: Mutex::new(IncomingState::Starting), stop: AtomicBool::new(false) });
        let own = Arc::clone(&shared);
        let source = source.to_owned();
        let set_state = move |own: &IncomingShared, now: IncomingState| {
            if let Ok(mut state) = own.state.lock()
                && *state != now
            {
                *state = now;
            }
        };
        let _ = std::thread::Builder::new().name("ndi-receive".to_owned()).spawn(move || {
            let receiver = runtime::get().and_then(|runtime| runtime::Receiver::new(runtime, &source, light));
            let receiver = match receiver {
                Ok(receiver) => receiver,
                Err(problem) => return set_state(&own, IncomingState::Failed(problem)),
            };
            while !own.stop.load(Ordering::Relaxed) {
                match receiver.capture(100, &mut on_video) {
                    runtime::Captured::Video => set_state(&own, IncomingState::Receiving),
                    runtime::Captured::Nothing | runtime::Captured::Error => {
                        if receiver.connections() <= 0 {
                            set_state(&own, IncomingState::Waiting);
                        } else {
                            // Connected, with nothing yet: wait without spinning.
                            std::thread::sleep(Duration::from_millis(2));
                        }
                    }
                }
            }
        });
        Incoming { shared }
    }

    pub fn state(&self) -> IncomingState {
        self.shared.state.lock().map_or(IncomingState::Waiting, |state| state.clone())
    }
}

impl Drop for Incoming {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Needs the NDI runtime and a few seconds of discovery on this computer's own network, so
    /// it is run by hand: `cargo test -p sp-ndi -- --ignored a_source_sent`.
    #[test]
    #[ignore = "needs the NDI runtime and the network"]
    fn a_source_sent_here_is_found_received_and_lost() {
        if !crate::installed() {
            return;
        }
        let name = format!("sp-ndi-test-{}", std::process::id());
        let sender = crate::NdiOutput::start(&name).expect("starts");
        let finder = Finder::start();
        let began = Instant::now();
        let found = loop {
            if let Some(found) = finder.sources().into_iter().find(|n| n.ends_with(&format!("({name})"))) {
                break found;
            }
            assert!(began.elapsed() < Duration::from_secs(15), "the source is never found");
            std::thread::sleep(Duration::from_millis(100));
        };
        let seen = Arc::new(Mutex::new((0u32, 0u32, 0u32)));
        let into = Arc::clone(&seen);
        let incoming = Incoming::start(&found, false, move |video| {
            if let Ok(mut seen) = into.lock() {
                *seen = (seen.0 + 1, video.width, video.height);
            }
        });
        let began = Instant::now();
        while seen.lock().unwrap().0 == 0 {
            // A source sends only while someone watches, and then what it is handed.
            sender.show(64, 36, vec![200; 64 * 36 * 4], crate::Sent::Opaque);
            assert!(began.elapsed() < Duration::from_secs(15), "no picture arrives");
            std::thread::sleep(Duration::from_millis(50));
        }
        let (count, width, height) = *seen.lock().unwrap();
        assert!(count > 0 && (width, height) == (64, 36));
        drop(incoming);
    }

    /// Testing aid: SOFTPRESENTER_NDI_WATCH=NAME or NAME:SECONDS watches the source on the
    /// network whose name holds NAME (a SoftPresenter screen sent over NDI, say) for that many
    /// seconds (20 without), as a receiver does, and says how its pictures arrived: how many,
    /// how many of them differed from the one before, and how long the waits between them were.
    /// For measuring by hand what a receiver sees while the sender's windows are held up:
    /// `SOFTPRESENTER_NDI_WATCH="Stream" cargo test -p sp-ndi -- --ignored --nocapture a_named_source`.
    #[test]
    #[ignore = "needs the NDI runtime, the network and a source to watch"]
    fn a_named_source_is_watched_and_how_its_pictures_arrive_is_said() {
        let Ok(asked) = std::env::var("SOFTPRESENTER_NDI_WATCH") else { return };
        let (name, seconds) = match asked.rsplit_once(':') {
            Some((name, seconds)) if seconds.trim().parse::<f32>().is_ok() => (name, seconds.trim().parse().unwrap()),
            _ => (asked.as_str(), 20.0),
        };
        let finder = Finder::start();
        let began = Instant::now();
        let found = loop {
            if let Some(found) = finder.sources().into_iter().find(|source| source.contains(name)) {
                break found;
            }
            assert!(began.elapsed() < Duration::from_secs(30), "no source named {name:?}: {:?}", finder.sources());
            std::thread::sleep(Duration::from_millis(100));
        };
        // When each picture arrived, and a sum of some of its pixels to tell a new one by.
        let seen: Arc<Mutex<Vec<(Instant, u64)>>> = Arc::default();
        let into = Arc::clone(&seen);
        let incoming = Incoming::start(&found, false, move |video| {
            let rows = video.rows;
            let sum =
                rows.iter().step_by(1009).fold(0u64, |sum, byte| sum.wrapping_mul(31).wrapping_add(u64::from(*byte)));
            if let Ok(mut seen) = into.lock() {
                seen.push((Instant::now(), sum));
            }
        });
        let began = Instant::now();
        while seen.lock().unwrap().is_empty() {
            assert!(began.elapsed() < Duration::from_secs(30), "no picture arrives from {found:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_secs_f32(seconds));
        drop(incoming);
        let seen = seen.lock().unwrap().clone();
        let ms = |took: Duration| took.as_secs_f64() * 1000.0;
        let gaps: Vec<Duration> = seen.windows(2).map(|pair| pair[1].0 - pair[0].0).collect();
        let new: Vec<Instant> = seen.windows(2).filter(|pair| pair[1].1 != pair[0].1).map(|pair| pair[1].0).collect();
        let longest = gaps.iter().max().copied().unwrap_or_default();
        let over = |than: f64| gaps.iter().filter(|gap| ms(**gap) > than).count();
        let mut sorted = gaps.clone();
        sorted.sort();
        let middle = sorted.get(sorted.len() / 2).copied().unwrap_or_default();
        println!(
            "watched {found:?} for {seconds} s: {} pictures arrived ({:.1} a second), {} of them new; waits between \
             pictures: middle {:.1} ms, longest {:.1} ms, {} over 50 ms, {} over 67 ms, {} over 100 ms, {} over 500 ms",
            seen.len(),
            seen.len() as f64 / f64::from(seconds),
            new.len(),
            ms(middle),
            ms(longest),
            over(50.0),
            over(67.0),
            over(100.0),
            over(500.0),
        );
    }
}
