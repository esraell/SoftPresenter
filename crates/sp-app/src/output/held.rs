//! Where the screens' thread spends its time, so that the next time the screens are held up the
//! log says where it went.
//!
//! The thread's loop is timed pass by pass: a pass is the work between two waits (the messages
//! taken, a video picture fetched, a drawing of the screens, NDI pictures handed over). Each
//! step of it is timed by the clock alone, into a fixed list of parts: nothing is allocated and
//! nothing is written while passes are quick. A pass held for [`HELD`] or more writes one line:
//! when it began, the longest single step and what it was for, every part's share, and what the
//! DeckLink and NDI threads were doing. The per-minute line counts the passes over [`SLOW`] and
//! over [`HELD`], the pictures the windows showed and didn't, and how late the thread woke.

use std::time::{Duration, Instant};

use sp_model::Uuid;

/// A pass held this long or longer is written to the log with where its time went: the
/// audience would have seen the picture stand still for three pictures of a 30 fps video.
pub(super) const HELD: Duration = Duration::from_millis(100);

/// A pass this long already costs a 30 fps video a picture or so. Counted each minute, not
/// written one by one.
pub(super) const SLOW: Duration = Duration::from_millis(50);

/// A kind of step the thread takes. The parts never overlap one another, so with what no part
/// covers ("other") they add up to the whole pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Part {
    /// Asking a video player (or a web page) for its newest picture.
    VideoFetch,
    /// Copying that picture, or sending it to its texture on the GPU.
    VideoCopy,
    /// Building what a screen shows: the layout, the words, a stage screen's parts, a fade's mix.
    Scene,
    /// Vello painting a picture on the GPU, with its wait for the GPU when it has to check it.
    Paint,
    /// Asking the GPU to copy an NDI or DeckLink picture back.
    Readback,
    /// Copying a picture that came back out of the GPU's buffer.
    Rows,
    /// Handing pictures to the NDI sources and DeckLink outputs (never waits on them).
    HandOver,
    /// Waiting for a window's next picture to draw into (the window system hands these out).
    Acquire,
    /// Copying a painted picture into a window's picture.
    Blit,
    /// Giving a window its picture to show.
    Present,
    /// Setting a window up: its surface, its textures, its copy pipelines.
    Setup,
    /// Asking the GPU how far it has got (never waits).
    Poll,
    /// The painter giving back GPU memory a heavy picture took.
    Rest,
    /// Locks the thread shares with others: the operator's status of NDI and DeckLink, whether an
    /// NDI picture is back, the pictures the slides show.
    Locks,
    /// Waking the operator's window.
    Wake,
    /// Pauses taken on purpose (a covered window, a picture that could not be painted).
    Pause,
    /// Testing aid: saving a picture of the output.
    Capture,
}

const PARTS: usize = 17;

impl Part {
    const ALL: [Part; PARTS] = [
        Part::Acquire,
        Part::Present,
        Part::Blit,
        Part::Paint,
        Part::Scene,
        Part::VideoFetch,
        Part::VideoCopy,
        Part::Readback,
        Part::Rows,
        Part::HandOver,
        Part::Setup,
        Part::Poll,
        Part::Rest,
        Part::Locks,
        Part::Wake,
        Part::Pause,
        Part::Capture,
    ];

    fn at(self) -> usize {
        self as usize
    }

    fn words(self) -> &'static str {
        match self {
            Part::VideoFetch => "video fetch",
            Part::VideoCopy => "video copy",
            Part::Scene => "scene build",
            Part::Paint => "vello paint",
            Part::Readback => "NDI readback",
            Part::Rows => "NDI rows",
            Part::HandOver => "hand-over",
            Part::Acquire => "surface acquire",
            Part::Blit => "blit",
            Part::Present => "present",
            Part::Setup => "window set-up",
            Part::Poll => "GPU poll",
            Part::Rest => "painter rest",
            Part::Locks => "locks",
            Part::Wake => "waking the window",
            Part::Pause => "pauses",
            Part::Capture => "capture",
        }
    }
}

/// What a step was for, to say what the longest one was doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum For {
    #[default]
    Nothing,
    /// A screen's window.
    Window(Uuid),
    /// A picture drawn for NDI and DeckLink, as this screen shows the show.
    Ndi(Uuid),
    /// What this screen shows (a scene built for it, wherever it goes).
    Screen(Uuid),
    /// The operator's live preview.
    Preview,
}

/// Running totals kept elsewhere, read at the start and end of a pass to say what part of it
/// went on them: the painter's waits for the GPU and its runs, the time this thread waited for
/// the locks the renderer shares between threads, and the time it spent writing to the log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Totals {
    pub gpu_wait: Duration,
    pub runs: u64,
    pub reruns: u64,
    pub shared_locks: Duration,
    pub log: Duration,
}

/// The pass under way, and the counts for the per-minute line.
#[derive(Debug)]
pub(super) struct Held {
    began: Instant,
    took: [Duration; PARTS],
    times: [u32; PARTS],
    /// The longest single step of the pass, and what it was for.
    longest: (Part, For, Duration),
    /// How deep the thread is in building a scene: a stage screen's picture of another screen
    /// is built inside its own, and the time counts once.
    building: u8,
    at_begin: Totals,
    minute: Minute,
}

/// What the per-minute line says of the passes and the windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Minute {
    slow: u32,
    held: u32,
    longest: Duration,
    shown: u32,
    not_shown: u32,
    late: Duration,
}

impl Held {
    pub(super) fn new(now: Instant) -> Held {
        Held {
            began: now,
            took: [Duration::ZERO; PARTS],
            times: [0; PARTS],
            longest: (Part::Acquire, For::Nothing, Duration::ZERO),
            building: 0,
            at_begin: Totals::default(),
            minute: Minute::default(),
        }
    }

    /// A pass begins, with the running totals as they stand.
    pub(super) fn begin(&mut self, now: Instant, totals: Totals) {
        self.began = now;
        self.took = [Duration::ZERO; PARTS];
        self.times = [0; PARTS];
        self.longest = (Part::Acquire, For::Nothing, Duration::ZERO);
        self.building = 0;
        self.at_begin = totals;
    }

    /// One step of a part took this long.
    pub(super) fn add(&mut self, part: Part, what: For, took: Duration) {
        self.took[part.at()] += took;
        self.times[part.at()] += 1;
        if took > self.longest.2 {
            self.longest = (part, what, took);
        }
    }

    /// One step of a part began at `from` and ends now. Gives the time now, for a step that
    /// follows straight on.
    pub(super) fn since(&mut self, part: Part, what: For, from: Instant) -> Instant {
        let now = Instant::now();
        self.add(part, what, now.saturating_duration_since(from));
        now
    }

    /// A scene is begun. The time, for the outermost scene only (one built inside another is
    /// part of it).
    pub(super) fn scene_begins(&mut self) -> Option<Instant> {
        self.building = self.building.saturating_add(1);
        (self.building == 1).then(Instant::now)
    }

    /// The scene begun at `began` (if outermost) is built.
    pub(super) fn scene_built(&mut self, began: Option<Instant>, what: For) {
        self.building = self.building.saturating_sub(1);
        if let Some(began) = began {
            self.since(Part::Scene, what, began);
        }
    }

    /// A picture reached a window, or did not (covered, hidden, or its surface being set up).
    pub(super) fn window_shown(&mut self, shown: bool) {
        if shown {
            self.minute.shown += 1;
        } else {
            self.minute.not_shown += 1;
        }
    }

    /// The thread woke `late` after it meant to (it asked to wait a while and slept longer).
    pub(super) fn woke_late(&mut self, late: Duration) {
        self.minute.late = self.minute.late.max(late);
    }

    /// The pass ends now. Its length when it was held for [`HELD`] or more (it should then be
    /// written, with [`Held::words`]); counted for the minute either way.
    pub(super) fn end(&mut self, now: Instant) -> Option<Duration> {
        let took = now.saturating_duration_since(self.began);
        if took >= SLOW {
            self.minute.slow += 1;
        }
        self.minute.longest = self.minute.longest.max(took);
        if took < HELD {
            return None;
        }
        self.minute.held += 1;
        Some(took)
    }

    /// The line for a pass that was held `took`, the running totals being `now` at its end:
    /// when it began (by the clock), its longest step, every part it had, and what was left
    /// over. `name` says what a step was for, in words.
    pub(super) fn words(&self, took: Duration, began_at: &str, now: Totals, name: impl Fn(For) -> String) -> String {
        let (part, what, longest) = self.longest;
        let mut line = format!("The screens' thread was held {} from {began_at}.", ms(took));
        if longest > Duration::ZERO {
            let what = name(what);
            let what = if what.is_empty() { String::new() } else { format!(" for {what}") };
            line.push_str(&format!(" Longest step: {}{what}, {}.", part.words(), ms(longest)));
        }
        let mut parts = Vec::new();
        let mut counted = Duration::ZERO;
        for part in Part::ALL {
            let times = self.times[part.at()];
            if times == 0 {
                continue;
            }
            let spent = self.took[part.at()];
            counted += spent;
            let mut said = format!("{} {} ×{times}", part.words(), ms(spent));
            if part == Part::Paint {
                let waited = now.gpu_wait.saturating_sub(self.at_begin.gpu_wait);
                let runs = now.runs.saturating_sub(self.at_begin.runs);
                let reruns = now.reruns.saturating_sub(self.at_begin.reruns);
                said.push_str(&format!(" (GPU wait {}, {runs} runs, {reruns} again for room)", ms(waited)));
            }
            parts.push(said);
        }
        parts.push(format!("other {}", ms(took.saturating_sub(counted))));
        line.push_str(&format!(" All steps: {}.", parts.join("; ")));
        let locks = now.shared_locks.saturating_sub(self.at_begin.shared_locks);
        let log = now.log.saturating_sub(self.at_begin.log);
        line.push_str(&format!(
            " Within them: waiting for the pictures' and fonts' locks {}, writing the log {}.",
            ms(locks),
            ms(log)
        ));
        line
    }

    /// What the per-minute line adds, and the counts begun again.
    pub(super) fn minute_words(&mut self) -> String {
        let minute = std::mem::take(&mut self.minute);
        let mut words = format!(
            "longest pass {}, {} over {} ms, {} over {} ms; window pictures {} shown, {} not",
            ms(minute.longest),
            minute.slow,
            SLOW.as_millis(),
            minute.held,
            HELD.as_millis(),
            minute.shown,
            minute.not_shown,
        );
        if minute.late >= SLOW {
            words.push_str(&format!("; woke up to {} late", ms(minute.late)));
        }
        words
    }
}

/// A length of time in milliseconds, to a tenth.
fn ms(took: Duration) -> String {
    format!("{:.1} ms", took.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(what: For) -> String {
        match what {
            For::Window(_) => "the window TV".to_owned(),
            For::Nothing => String::new(),
            _ => "something".to_owned(),
        }
    }

    #[test]
    fn a_quick_pass_says_nothing_and_a_held_one_says_where_its_time_went() {
        let start = Instant::now();
        let mut held = Held::new(start);
        held.begin(start, Totals::default());
        held.add(Part::Paint, For::Nothing, Duration::from_millis(4));
        assert_eq!(held.end(start + Duration::from_millis(8)), None, "a quick pass is not written");

        let window = For::Window(Uuid::nil());
        held.begin(start, Totals { gpu_wait: Duration::from_millis(1), runs: 10, reruns: 0, ..Totals::default() });
        held.add(Part::Paint, For::Nothing, Duration::from_millis(3));
        held.add(Part::Paint, For::Nothing, Duration::from_millis(5));
        held.add(Part::Acquire, window, Duration::from_millis(1003));
        held.add(Part::Present, window, Duration::from_millis(1));
        let took = held.end(start + Duration::from_millis(1019)).expect("held over 100 ms");
        let at_end = Totals { gpu_wait: Duration::from_millis(3), runs: 12, reruns: 1, ..Totals::default() };
        let line = held.words(took, "10:00:16.976", at_end, named);
        assert!(line.starts_with("The screens' thread was held 1019.0 ms from 10:00:16.976."), "{line}");
        assert!(line.contains("Longest step: surface acquire for the window TV, 1003.0 ms."), "{line}");
        assert!(line.contains("vello paint 8.0 ms ×2 (GPU wait 2.0 ms, 2 runs, 1 again for room)"), "{line}");
        assert!(line.contains("present 1.0 ms ×1"), "{line}");
        // What no part covers is said, so the parts add up to the whole.
        assert!(line.contains("other 7.0 ms."), "{line}");
        assert!(!line.contains("scene build"), "parts that did not happen are left out: {line}");
    }

    #[test]
    fn a_scene_built_inside_another_counts_once() {
        let start = Instant::now();
        let mut held = Held::new(start);
        held.begin(start, Totals::default());
        let outer = held.scene_begins();
        let inner = held.scene_begins();
        assert!(outer.is_some() && inner.is_none());
        held.scene_built(inner, For::Nothing);
        held.scene_built(outer, For::Nothing);
        assert_eq!(held.times[Part::Scene.at()], 1);
        // And the next one counts again.
        let next = held.scene_begins();
        held.scene_built(next, For::Nothing);
        assert_eq!(held.times[Part::Scene.at()], 2);
    }

    #[test]
    fn the_minute_counts_slow_and_held_passes_and_the_windows() {
        let start = Instant::now();
        let mut held = Held::new(start);
        for took in [10, 60, 70, 150] {
            held.begin(start, Totals::default());
            held.end(start + Duration::from_millis(took));
        }
        held.window_shown(true);
        held.window_shown(true);
        held.window_shown(false);
        held.woke_late(Duration::from_millis(20));
        assert_eq!(
            held.minute_words(),
            "longest pass 150.0 ms, 3 over 50 ms, 1 over 100 ms; window pictures 2 shown, 1 not"
        );
        held.woke_late(Duration::from_millis(400));
        assert!(held.minute_words().ends_with("not; woke up to 400.0 ms late"), "counted afresh, late said");
    }
}
