//! What the app counts about how it runs, for the anonymous reports (`crate::reports`).
//!
//! The rule is *sample, don't trace*: the threads that matter (the operator's window, the
//! screens) only add to a few atomic numbers and set a number for where they are; the
//! reporter's own thread reads and empties them every few seconds. Nothing here allocates,
//! locks or waits on the thread that calls it, except the rare event and a view or tool being
//! counted (a lock that is tried, never waited for). While reports are off, or have not been
//! told of yet, every call does nothing.
//!
//! Nothing here holds a word of the person's own: an event is a code the program made up and
//! one or two numbers. The one place that takes text is a problem's line (`problem_in`), which
//! is kept as it was said and looked at only by `scrub::problem_line`, which throws away any
//! line that could hold a name.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

static ON: AtomicBool = AtomicBool::new(false);
static START: OnceLock<Instant> = OnceLock::new();

/// Turns the counting on: reports are on, and the person has been told of them.
pub fn switch_on() {
    START.get_or_init(Instant::now);
    ON.store(true, Relaxed);
}

/// Turns the counting off, and forgets what was counted and not yet taken.
pub fn switch_off() {
    ON.store(false, Relaxed);
    let _ = take();
    let _ = take_events();
    let _ = take_problems();
    let _ = take_features();
}

#[inline]
pub fn is_on() -> bool {
    ON.load(Relaxed)
}

/// Milliseconds since the program started counting.
pub fn since_start_ms() -> u64 {
    START.get().map_or(0, |start| start.elapsed().as_millis() as u64)
}

// ---------------------------------------------------------------------------------------------
// Counters

/// Things counted.
#[derive(Clone, Copy, Debug)]
pub enum Count {
    /// Passes of the screens' thread, and those over 50 ms and over 100 ms.
    Passes,
    PassesOver50,
    PassesOver100,
    /// Pictures that reached a window, and those that did not.
    WindowShown,
    WindowNotShown,
    /// Draws of the operator's window, and those over 50 ms and over 100 ms.
    UiFrames,
    UiOver50,
    UiOver100,
    /// Keys pressed, mouse buttons pressed, and commands that came from a remote device.
    Keys,
    Clicks,
    RemoteActions,
}

pub const COUNTS: usize = 11;
static COUNTERS: [AtomicU64; COUNTS] = [const { AtomicU64::new(0) }; COUNTS];

/// Counts one.
#[inline]
pub fn bump(what: Count) {
    if is_on() {
        COUNTERS[what as usize].fetch_add(1, Relaxed);
    }
}

/// The counters as they stand and then reset, in the order of [`Count`].
fn take_counters() -> [u64; COUNTS] {
    std::array::from_fn(|at| COUNTERS[at].swap(0, Relaxed))
}

// ---------------------------------------------------------------------------------------------
// Times

/// Where a time is kept: the upper edge of each bucket, in microseconds (the last is open).
const EDGES_US: [u32; 29] = [
    500, 1_000, 1_500, 2_000, 3_000, 4_000, 5_000, 6_000, 8_000, 10_000, 12_000, 14_000, 16_000, 18_000, 20_000,
    25_000, 30_000, 35_000, 40_000, 50_000, 60_000, 80_000, 100_000, 150_000, 200_000, 300_000, 500_000, 1_000_000,
    2_000_000,
];
const BUCKETS: usize = EDGES_US.len() + 1;

/// How times fell into the buckets, and the longest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Times {
    counts: [u32; BUCKETS],
    max_us: u32,
}

impl Default for Times {
    fn default() -> Times {
        Times { counts: [0; BUCKETS], max_us: 0 }
    }
}

/// A set of times that threads add to without waiting.
pub struct Clock {
    counts: [AtomicU32; BUCKETS],
    max_us: AtomicU32,
}

impl Clock {
    pub const fn new() -> Clock {
        Clock { counts: [const { AtomicU32::new(0) }; BUCKETS], max_us: AtomicU32::new(0) }
    }

    #[inline]
    pub fn add(&self, took: Duration) {
        if !is_on() {
            return;
        }
        let us = took.as_micros().min(u128::from(u32::MAX)) as u32;
        let at = EDGES_US.partition_point(|edge| *edge < us);
        self.counts[at].fetch_add(1, Relaxed);
        self.max_us.fetch_max(us, Relaxed);
    }

    /// What was added since the last time, and the set emptied.
    pub fn take(&self) -> Times {
        Times {
            counts: std::array::from_fn(|at| self.counts[at].swap(0, Relaxed)),
            max_us: self.max_us.swap(0, Relaxed),
        }
    }
}

impl Times {
    /// Puts more times with these (the pieces of a minute).
    pub fn merge(&mut self, more: &Times) {
        for (mine, other) in self.counts.iter_mut().zip(more.counts) {
            *mine += other;
        }
        self.max_us = self.max_us.max(more.max_us);
    }

    pub fn count(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// How long the time is that this share (0 to 1) of them were under, to the bucket's edge, in
    /// milliseconds; the longest, when the share reaches the last bucket.
    fn under(&self, share: f64) -> f32 {
        let n = self.count();
        if n == 0 {
            return 0.0;
        }
        let want = (f64::from(n) * share).ceil().max(1.0) as u32;
        let mut seen = 0;
        for (at, count) in self.counts.iter().enumerate() {
            seen += count;
            if seen >= want {
                let edge = EDGES_US.get(at).copied().unwrap_or(self.max_us);
                return edge.min(self.max_us.max(1)) as f32 / 1000.0;
            }
        }
        self.max_us as f32 / 1000.0
    }

    pub fn summary(&self) -> TimeSummary {
        TimeSummary {
            n: self.count(),
            p50: round1(self.under(0.5)),
            p95: round1(self.under(0.95)),
            max: round1(self.max_us as f32 / 1000.0),
        }
    }
}

/// A set of times in words: how many, the middle one, the one 19 in 20 were under, the longest
/// (milliseconds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct TimeSummary {
    pub n: u32,
    pub p50: f32,
    pub p95: f32,
    pub max: f32,
}

fn round1(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

/// The operator's window drew once, taking this long.
pub fn ui_frame(took: Duration) {
    if !is_on() {
        return;
    }
    UI_FRAME.add(took);
    bump(Count::UiFrames);
    if took >= Duration::from_millis(50) {
        bump(Count::UiOver50);
    }
    if took >= Duration::from_millis(100) {
        bump(Count::UiOver100);
        static LAST: AtomicU64 = AtomicU64::new(0);
        if once_a_second(&LAST) {
            event_ms("slow_frame", took.as_secs_f64() * 1000.0);
        }
    }
}

/// True at most once a second for each `last`, so a thread that stays slow says so once a second,
/// not at every pass.
fn once_a_second(last: &AtomicU64) -> bool {
    let now = since_start_ms();
    let before = last.load(Relaxed);
    if before != 0 && now.saturating_sub(before) < 1000 {
        return false;
    }
    last.store(now.max(1), Relaxed);
    true
}

/// One pass of the screens' thread took this long; a pass over 100 ms is an event (a code and
/// the time: nothing is put into words on that thread).
pub fn screens_pass(took: Duration) {
    if !is_on() {
        return;
    }
    SCREEN_PASS.add(took);
    bump(Count::Passes);
    if took >= Duration::from_millis(50) {
        bump(Count::PassesOver50);
    }
    if took >= Duration::from_millis(100) {
        bump(Count::PassesOver100);
        static LAST: AtomicU64 = AtomicU64::new(0);
        if once_a_second(&LAST) {
            event_ms("slow_pass", took.as_secs_f64() * 1000.0);
        }
    }
}

/// The operator's window drawing, whole, and the screens' passes, whole.
pub static UI_FRAME: Clock = Clock::new();
pub static SCREEN_PASS: Clock = Clock::new();

/// One tick's worth of everything counted: taken and reset together.
#[derive(Clone, Debug, Default)]
pub struct Taken {
    pub counts: [u64; COUNTS],
    pub ui: Times,
    pub pass: Times,
}

pub fn take() -> Taken {
    Taken { counts: take_counters(), ui: UI_FRAME.take(), pass: SCREEN_PASS.take() }
}

// ---------------------------------------------------------------------------------------------
// Where the busy threads are (for the freeze watchdog)

/// A thread the watchdog watches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Ui,
    Screens,
}

impl Who {
    /// The code of its freeze, and of the freeze being over.
    pub fn freeze_codes(self) -> (&'static str, &'static str) {
        match self {
            Who::Ui => ("freeze_window", "freeze_window_over"),
            Who::Screens => ("freeze_screens", "freeze_screens_over"),
        }
    }
}

/// Where a thread is: when it began what it is doing (milliseconds since the start, plus one;
/// nought when it waits for something to do) and a code for the step it last finished.
struct Busy {
    since: AtomicU64,
    phase: AtomicU32,
    /// How long the last stretch of work lasted, when it was over three seconds.
    long_ms: AtomicU64,
    /// How many stretches of work are open inside one another (a handler that calls another).
    depth: AtomicU32,
}

impl Busy {
    const fn new() -> Busy {
        Busy {
            since: AtomicU64::new(0),
            phase: AtomicU32::new(0),
            long_ms: AtomicU64::new(0),
            depth: AtomicU32::new(0),
        }
    }
}

/// Marks a thread busy for as long as it is held (and idle after).
pub struct BusyGuard(Who);

impl BusyGuard {
    #[inline]
    pub fn new(who: Who) -> BusyGuard {
        busy(who);
        BusyGuard(who)
    }
}

impl Drop for BusyGuard {
    #[inline]
    fn drop(&mut self) {
        idle(self.0);
    }
}

static UI: Busy = Busy::new();
static SCREENS: Busy = Busy::new();

fn busy_of(who: Who) -> &'static Busy {
    match who {
        Who::Ui => &UI,
        Who::Screens => &SCREENS,
    }
}

/// How long work may last before the watchdog says the thread is frozen.
pub const FREEZE_AFTER: Duration = Duration::from_secs(3);

/// The thread begins work (a pass, an event, a frame).
#[inline]
pub fn busy(who: Who) {
    if is_on() {
        let state = busy_of(who);
        if state.depth.fetch_add(1, Relaxed) == 0 {
            state.since.store(since_start_ms() + 1, Relaxed);
            state.phase.store(0, Relaxed);
        }
    }
}

/// The thread has finished a step of its work; a code that [`phase_name`] says in words.
#[inline]
pub fn phase(who: Who, code: u32) {
    if is_on() {
        busy_of(who).phase.store(code, Relaxed);
    }
}

/// The thread's work is done, and it waits.
#[inline]
pub fn idle(who: Who) {
    if is_on() {
        let state = busy_of(who);
        // Only the outermost stretch ends the work.
        let before = state.depth.try_update(Relaxed, Relaxed, |depth| Some(depth.saturating_sub(1))).unwrap_or(0);
        if before > 1 {
            return;
        }
        let since = state.since.swap(0, Relaxed);
        if since > 0 {
            let took = (since_start_ms() + 1).saturating_sub(since);
            if took >= FREEZE_AFTER.as_millis() as u64 {
                state.long_ms.store(took, Relaxed);
            }
        }
    }
}

/// The step of the operator's window that says it has drawn. The screens' thread says its steps
/// by the parts of its pass, each one more than the part's place.
pub const UI_DRAWN: u32 = 2;

/// What a thread looks like to the watchdog now: how long it has been in the same stretch of
/// work (None when it waits), where it last was, and for how long its last stretch lasted if
/// that was a freeze that has ended (read once).
pub fn watch(who: Who) -> (Option<Duration>, u32, Option<Duration>) {
    let state = busy_of(who);
    let since = state.since.load(Relaxed);
    let for_how_long = (since > 0).then(|| Duration::from_millis((since_start_ms() + 1).saturating_sub(since)));
    let ended = state.long_ms.swap(0, Relaxed);
    (for_how_long, state.phase.load(Relaxed), (ended > 0).then(|| Duration::from_millis(ended)))
}

// ---------------------------------------------------------------------------------------------
// The events, the problems and what was used

/// Something that happened: a code the program made up, and a number or a time where one
/// belongs to it. Never words.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// Seconds since the app started.
    pub t: f64,
    pub kind: &'static str,
    pub n: Option<f64>,
    pub ms: Option<f64>,
}

const EVENTS_KEPT: usize = 400;
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());

fn note(kind: &'static str, n: Option<f64>, ms: Option<f64>) {
    if !is_on() {
        return;
    }
    // A lock someone holds is not waited for: the event is lost rather than the caller held up.
    if let Ok(mut events) = EVENTS.try_lock() {
        if events.len() >= EVENTS_KEPT {
            events.remove(0);
        }
        events.push(Event { t: since_start_ms() as f64 / 1000.0, kind, n, ms });
    }
}

/// Notes that something happened.
pub fn event(kind: &'static str) {
    note(kind, None, None);
}

/// Notes that something happened, with a count.
pub fn event_n(kind: &'static str, n: f64) {
    note(kind, Some(n), None);
}

/// Notes that something happened, with how long it took.
pub fn event_ms(kind: &'static str, ms: f64) {
    note(kind, None, Some(ms));
}

/// The events since the last call.
pub fn take_events() -> Vec<Event> {
    EVENTS.lock().map(|mut events| std::mem::take(&mut *events)).unwrap_or_default()
}

const PROBLEMS_KEPT: usize = 40;
static PROBLEMS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A problem the app logged (`applog::problem`): an event, and its line kept as it was said, in
/// English, for `scrub::problem_line` to look at (which throws away any line that could hold a
/// name; nothing else ever reads these).
pub fn problem_in(area: &str, message: &str) {
    if !is_on() {
        return;
    }
    // The same problem again within a minute is counted, not kept again: a problem that comes
    // on every frame would otherwise fill the report.
    static LAST: Mutex<Option<(String, u32, u64)>> = Mutex::new(None);
    let what = format!("[{area}] {}", crate::i18n::english_of(message));
    let now = since_start_ms();
    let Ok(mut last) = LAST.lock() else { return };
    match last.as_mut() {
        Some((said, again, at)) if *said == what && now.saturating_sub(*at) < 60_000 => *again += 1,
        _ => {
            if let Some((_, again, _)) = last.as_ref()
                && *again > 0
            {
                event_n("problem_repeated", f64::from(*again));
            }
            *last = Some((what.clone(), 0, now));
            drop(last);
            event("problem");
            if let Ok(mut problems) = PROBLEMS.try_lock() {
                if problems.len() >= PROBLEMS_KEPT {
                    problems.remove(0);
                }
                problems.push(what);
            }
        }
    }
}

/// The problem lines since the last call, as they were said.
pub fn take_problems() -> Vec<String> {
    PROBLEMS.lock().map(|mut problems| std::mem::take(&mut *problems)).unwrap_or_default()
}

/// A notice the operator was shown: that there was one, and whether it was trouble. Never its
/// words.
pub fn notice(trouble: bool) {
    event(if trouble { "notice_trouble" } else { "notice" });
}

const FEATURES_KEPT: usize = 100;
static FEATURES: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

/// A view or a tool was used once: counted under a code the program made up.
pub fn feature(code: &str) {
    if !is_on() {
        return;
    }
    if let Ok(mut features) = FEATURES.try_lock() {
        if let Some((_, count)) = features.iter_mut().find(|(have, _)| have == code) {
            *count = count.saturating_add(1);
        } else if features.len() < FEATURES_KEPT {
            features.push((code.to_owned(), 1));
        }
    }
}

/// A menu's command was chosen: counted under its own name in the program, `menu_new_song` for
/// `NewSong`. Only the name: nothing a command carries is looked at.
pub fn feature_of(action: &impl std::fmt::Debug) {
    if is_on() {
        feature(&code_of(&format!("{action:?}")));
    }
}

/// `menu_import_songs` for `ImportSongs`: the name up to anything that isn't a letter or digit,
/// in small letters with `_` before each capital.
fn code_of(name: &str) -> String {
    let mut code = String::from("menu");
    for c in name.chars().take_while(char::is_ascii_alphanumeric).take(26) {
        if c.is_ascii_uppercase() {
            code.push('_');
        }
        code.push(c.to_ascii_lowercase());
    }
    code
}

/// What was used since the last call, and how many times.
pub fn take_features() -> Vec<(String, u32)> {
    FEATURES.lock().map(|mut features| std::mem::take(&mut *features)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_summed_into_a_middle_a_95th_and_a_longest() {
        let clock = Clock::new();
        // The clock only counts while reports are on; the test turns them on.
        let _guard = crate::reports::tests::counting();
        for ms in [2, 2, 2, 2, 2, 2, 2, 2, 2, 90] {
            clock.add(Duration::from_millis(ms));
        }
        let times = clock.take();
        let summary = times.summary();
        assert_eq!(summary.n, 10);
        assert_eq!(summary.p50, 2.0);
        assert_eq!(summary.max, 90.0);
        assert!(summary.p95 >= 90.0 - 0.1 && summary.p95 <= 90.0, "{summary:?}");
        // Taken: the clock begins again empty.
        assert_eq!(clock.take().count(), 0);
        // The pieces of a minute put together give the minute's.
        let mut minute = Times::default();
        for _ in 0..12 {
            minute.merge(&times);
        }
        assert_eq!(minute.summary().n, 120);
        assert_eq!(minute.summary().p50, 2.0);
    }

    #[test]
    fn a_stretch_of_work_over_three_seconds_is_remembered_when_it_ends() {
        let _guard = crate::reports::tests::counting();
        // The operator's window's marks: no other test touches them (the screens' are set by
        // every test that draws a pass).
        busy(Who::Ui);
        phase(Who::Ui, 3);
        let (for_how_long, code, ended) = watch(Who::Ui);
        assert!(for_how_long.is_some() && code == 3 && ended.is_none());
        idle(Who::Ui);
        let (for_how_long, _, ended) = watch(Who::Ui);
        assert!(for_how_long.is_none() && ended.is_none(), "a short one leaves nothing");
    }

    #[test]
    fn events_wait_to_be_taken_and_the_oldest_go() {
        let _guard = crate::reports::tests::counting();
        take_events();
        for at in 0..EVENTS_KEPT + 5 {
            event_n("test", at as f64);
        }
        // Other tests may add an event of their own meanwhile.
        let taken: Vec<Event> = take_events().into_iter().filter(|event| event.kind == "test").collect();
        assert!(taken.len() >= EVENTS_KEPT - 20 && taken.len() <= EVENTS_KEPT, "{}", taken.len());
        assert_eq!(taken.last().and_then(|event| event.n), Some((EVENTS_KEPT + 4) as f64));
    }

    #[test]
    fn what_is_used_is_counted_by_its_code() {
        let _guard = crate::reports::tests::counting();
        take_features();
        for _ in 0..3 {
            feature("test_tool");
        }
        feature("test_view");
        let taken = take_features();
        let has = |code: &str, times: u32| taken.iter().any(|(have, count)| have == code && *count == times);
        assert!(has("test_tool", 3) && has("test_view", 1), "{taken:?}");
        assert_eq!(code_of("ImportSongs"), "menu_import_songs");
        assert_eq!(code_of("Open(\"Amazing Grace\")"), "menu_open");
    }
}
