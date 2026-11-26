//! Says in the log when the window's thread was busy for long, and with what.
//!
//! A tester's log once went quiet for minutes while a file window held the window's thread, and
//! nothing in it told that from the app sitting idle. The event loop marks when it begins each
//! piece of work it is handed (an event, a drawing, the look round before it sleeps) and when it
//! is done; a thread of its own looks four times a second. Work that lasted over half a second
//! is said once it is over:
//!
//! `[stall] the window's thread was busy 4.3 s (inside: importing songs)`
//!
//! and work still going after five seconds is said every five seconds while it lasts, so a
//! freeze shows in the log as it happens:
//!
//! `[stall] the window's thread has been busy 10 s so far (inside: importing songs)`
//!
//! "Inside" is the longest of the known slow steps marked with [`inside`] (importing, installing
//! a Bible, waiting for the library's writer…) when it was at least half of the work, or
//! "unknown". The marks are a clock reading and a
//! few atomic stores, on the window's thread only: the screens' thread never marks or logs here,
//! and an idle app (the loop asleep) costs the watcher's four wake-ups a second.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use crate::applog;

/// Work shorter than this is never said.
const SAY_OVER_MS: u64 = 500;
/// Work still going is said after this long, and again each time this much more has passed.
const SO_FAR_EVERY_MS: u64 = 5_000;
/// How often the watcher looks.
const LOOK_EVERY: Duration = Duration::from_millis(250);

/// A known slow step of the window's thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Doing {
    Unknown = 0,
    Starting,
    Drawing,
    Command,
    Importing,
    Bible,
    Media,
    Saving,
    Dialog,
}

impl Doing {
    const ALL: [Doing; 9] = [
        Doing::Unknown,
        Doing::Starting,
        Doing::Drawing,
        Doing::Command,
        Doing::Importing,
        Doing::Bible,
        Doing::Media,
        Doing::Saving,
        Doing::Dialog,
    ];

    fn of(code: u8) -> Doing {
        Doing::ALL.get(usize::from(code)).copied().unwrap_or(Doing::Unknown)
    }

    fn words(self) -> &'static str {
        match self {
            Doing::Unknown => "unknown",
            Doing::Starting => "starting up",
            Doing::Drawing => "drawing the window",
            Doing::Command => "a command from sp, the phone remote or an assistant",
            Doing::Importing => "importing songs",
            Doing::Bible => "installing a Bible",
            Doing::Media => "adding pictures and videos",
            Doing::Saving => "waiting for the library's writer",
            Doing::Dialog => "opening a file window or question",
        }
    }
}

/// Where the window's thread is. Times are milliseconds since the app started, plus one, so
/// that nought means "none".
struct Marks {
    /// When the work going on began; nought while the loop waits.
    since: AtomicU64,
    /// How many pieces of work are open inside one another (an event handled within another).
    depth: AtomicU32,
    /// The slow step going on now.
    doing: AtomicU8,
    /// The longest slow step of the work going on, and how long it took.
    longest: AtomicU8,
    longest_ms: AtomicU64,
    /// The last piece of work over half a second, once it was over (taken by the watcher): how
    /// long it took, and its longest slow step.
    ended_ms: AtomicU64,
    ended_doing: AtomicU8,
}

impl Marks {
    const fn new() -> Marks {
        Marks {
            since: AtomicU64::new(0),
            depth: AtomicU32::new(0),
            doing: AtomicU8::new(0),
            longest: AtomicU8::new(0),
            longest_ms: AtomicU64::new(0),
            ended_ms: AtomicU64::new(0),
            ended_doing: AtomicU8::new(0),
        }
    }

    fn begin(&self, now: u64) {
        if self.depth.fetch_add(1, Relaxed) == 0 {
            self.since.store(now, Relaxed);
            self.longest.store(0, Relaxed);
            self.longest_ms.store(0, Relaxed);
        }
    }

    fn end(&self, now: u64) {
        let before = self.depth.fetch_sub(1, Relaxed);
        if before != 1 {
            // Not the outermost (or, should one end twice, never below nought).
            if before == 0 {
                self.depth.store(0, Relaxed);
            }
            return;
        }
        let since = self.since.swap(0, Relaxed);
        let took = now.saturating_sub(since);
        if since > 0 && took >= SAY_OVER_MS {
            // A step is named only when it was at least half of the work.
            let named = self.longest_ms.load(Relaxed) * 2 >= took;
            self.ended_doing.store(if named { self.longest.load(Relaxed) } else { Doing::Unknown as u8 }, Relaxed);
            self.ended_ms.store(took, Relaxed);
        }
    }

    /// A slow step begins; returns the one it is inside, to go back to.
    fn step(&self, doing: Doing) -> u8 {
        self.doing.swap(doing as u8, Relaxed)
    }

    /// The slow step that began at `began` is over: the one before it goes on.
    fn step_done(&self, doing: Doing, before: u8, began: u64, now: u64) {
        self.doing.store(before, Relaxed);
        let took = now.saturating_sub(began);
        if took >= self.longest_ms.load(Relaxed) {
            self.longest_ms.store(took, Relaxed);
            self.longest.store(doing as u8, Relaxed);
        }
    }

    /// What the watcher sees: when the work going on began (nought for none), the slow step
    /// going on, and the last long piece of work if it has ended since the last look.
    fn seen(&self) -> (u64, Doing, Option<(u64, Doing)>) {
        let ended = self.ended_ms.swap(0, Relaxed);
        let ended = (ended > 0).then(|| (ended, Doing::of(self.ended_doing.load(Relaxed))));
        (self.since.load(Relaxed), Doing::of(self.doing.load(Relaxed)), ended)
    }
}

static MARKS: Marks = Marks::new();
static STARTED: OnceLock<Instant> = OnceLock::new();

fn now_ms() -> u64 {
    STARTED.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// Marks the window's thread busy until it is dropped: held for each event the loop hands over.
pub struct Busy(());

pub fn busy() -> Busy {
    MARKS.begin(now_ms());
    Busy(())
}

impl Drop for Busy {
    fn drop(&mut self) {
        MARKS.end(now_ms());
    }
}

/// Marks a known slow step until it is dropped.
pub struct Inside {
    doing: Doing,
    before: u8,
    began: u64,
}

pub fn inside(doing: Doing) -> Inside {
    Inside { doing, before: MARKS.step(doing), began: now_ms() }
}

impl Drop for Inside {
    fn drop(&mut self) {
        MARKS.step_done(self.doing, self.before, self.began, now_ms());
    }
}

/// Starts the watcher (once).
pub fn watch() {
    static STARTED_WATCH: OnceLock<()> = OnceLock::new();
    STARTED_WATCH.get_or_init(|| {
        now_ms();
        let _ = std::thread::Builder::new().name("stall watch".to_owned()).spawn(|| {
            let mut watch = Watch::default();
            loop {
                std::thread::sleep(LOOK_EVERY);
                let (since, doing, ended) = MARKS.seen();
                for line in watch.look(now_ms(), since, doing, ended) {
                    applog::event("stall", &line);
                }
            }
        });
    });
}

/// What the watcher has said of the work going on.
#[derive(Default)]
struct Watch {
    /// The work it last said "so far" of (when it began), and how long it had been going then.
    told: Option<(u64, u64)>,
}

impl Watch {
    /// What to say now, at `now`, of the work that began at `since` (nought: none), inside
    /// `doing`, and of the long work that `ended` since the last look.
    fn look(&mut self, now: u64, since: u64, doing: Doing, ended: Option<(u64, Doing)>) -> Vec<String> {
        let mut said = Vec::new();
        if let Some((took, inside)) = ended {
            said.push(format!(
                "the window's thread was busy {:.1} s (inside: {})",
                took as f64 / 1000.0,
                inside.words()
            ));
        }
        if since == 0 {
            self.told = None;
            return said;
        }
        let going = now.saturating_sub(since);
        let due = match self.told {
            Some((began, at)) if began == since => going >= at + SO_FAR_EVERY_MS,
            _ => going >= SO_FAR_EVERY_MS,
        };
        if due {
            // Said in whole steps of five seconds, as they pass.
            let step = going / SO_FAR_EVERY_MS * SO_FAR_EVERY_MS;
            self.told = Some((since, step));
            said.push(format!(
                "the window's thread has been busy {} s so far (inside: {})",
                step / 1000,
                doing.words()
            ));
        }
        said
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_over_half_a_second_is_said_once_it_is_over_with_its_slow_step() {
        let marks = Marks::new();
        let mut watch = Watch::default();
        // A short piece of work says nothing.
        marks.begin(1_000);
        marks.end(1_200);
        let (since, doing, ended) = marks.seen();
        assert!(watch.look(1_250, since, doing, ended).is_empty());
        // A long one, mostly an import, is said as it ends.
        marks.begin(2_000);
        let before = marks.step(Doing::Drawing);
        marks.step_done(Doing::Drawing, before, 2_000, 2_100);
        let before = marks.step(Doing::Importing);
        let (since, doing, ended) = marks.seen();
        assert_eq!((since, doing, ended), (2_000, Doing::Importing, None));
        assert!(watch.look(3_000, since, doing, ended).is_empty(), "under five seconds, nothing yet");
        marks.step_done(Doing::Importing, before, 2_100, 6_200);
        marks.end(6_300);
        let (since, doing, ended) = marks.seen();
        assert_eq!(since, 0);
        assert_eq!(doing, Doing::Unknown);
        assert_eq!(
            watch.look(6_400, since, doing, ended),
            ["the window's thread was busy 4.3 s (inside: importing songs)"]
        );
        // Said once.
        let (since, doing, ended) = marks.seen();
        assert!(watch.look(6_650, since, doing, ended).is_empty());
    }

    #[test]
    fn a_step_that_was_a_small_part_of_long_work_is_not_named() {
        let marks = Marks::new();
        let mut watch = Watch::default();
        marks.begin(1_000);
        let before = marks.step(Doing::Drawing);
        marks.step_done(Doing::Drawing, before, 1_000, 1_050);
        marks.end(2_000);
        let (since, doing, ended) = marks.seen();
        assert_eq!(watch.look(2_100, since, doing, ended), ["the window's thread was busy 1.0 s (inside: unknown)"]);
    }

    #[test]
    fn work_still_going_is_said_every_five_seconds() {
        let mut watch = Watch::default();
        let began = 10_000;
        let mut said = Vec::new();
        // The watcher looks every 250 ms for 16 seconds.
        for at in (0..=16_000).step_by(250) {
            said.extend(watch.look(began + at, began, Doing::Unknown, None));
        }
        assert_eq!(
            said,
            [
                "the window's thread has been busy 5 s so far (inside: unknown)",
                "the window's thread has been busy 10 s so far (inside: unknown)",
                "the window's thread has been busy 15 s so far (inside: unknown)",
            ]
        );
        // Over, and then another long piece of work: counted afresh.
        assert_eq!(
            watch.look(began + 16_250, 0, Doing::Unknown, Some((16_100, Doing::Saving))),
            ["the window's thread was busy 16.1 s (inside: waiting for the library's writer)"]
        );
        assert!(watch.look(40_000, 39_000, Doing::Bible, None).is_empty());
        assert_eq!(
            watch.look(44_000, 39_000, Doing::Bible, None),
            ["the window's thread has been busy 5 s so far (inside: installing a Bible)"]
        );
    }

    #[test]
    fn work_inside_work_counts_once_from_the_outermost() {
        let marks = Marks::new();
        marks.begin(100);
        marks.begin(150);
        marks.end(900);
        assert_eq!(marks.seen().0, 100, "the inner end doesn't end the work");
        marks.end(1_000);
        let (since, _, ended) = marks.seen();
        assert_eq!((since, ended), (0, Some((900, Doing::Unknown))));
        // An end too many is let go.
        marks.end(1_100);
        marks.begin(2_000);
        assert_eq!(marks.seen().0, 2_000);
    }
}
