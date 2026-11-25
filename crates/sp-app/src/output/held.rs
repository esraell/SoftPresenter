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
//!
//! The last few passes of [`LONG`] or more are kept too, with the other things the thread does
//! that can keep an NDI picture from coming in time (a fade, a picture the GPU was too slow
//! with, one lost on its way back, a video that gave no new picture): when an NDI source finds
//! a picture came late, the line saying so tells what the thread was doing then
//! ([`Held::then`]). Kept in fixed lists, so nothing is allocated for them either.

use std::time::{Duration, Instant};

use sp_model::Uuid;
use sp_render::vello;

/// A pass held this long or longer is written to the log with where its time went: the
/// audience would have seen the picture stand still for three pictures of a 30 fps video.
pub(super) const HELD: Duration = Duration::from_millis(100);

/// A pass this long already costs a 30 fps video a picture or so. Counted each minute, not
/// written one by one.
pub(super) const SLOW: Duration = Duration::from_millis(50);

/// A pass this long or longer is kept a while, with where its time went, for the line that
/// says what held up an NDI picture: one picture's time at the rate NDI sends.
pub(super) const LONG: Duration = sp_ndi::FRAME;

/// How many long passes are kept, and how many of the other things that can hold up a picture.
const KEPT: usize = 4;
const HAPPENINGS: usize = 16;

/// Something the thread does, other than taking long over a pass, that can keep an NDI source
/// from getting its next picture in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Happened {
    /// The GPU was too slow with a picture: it is drawn again shortly.
    GpuTooSlow,
    /// An NDI picture was lost on its way back from the GPU.
    NdiLost,
    /// An NDI drawing was put off, the GPU still reading back the one before.
    NdiPutOff,
    /// A fade began, or ended. Its steps draw the windows at their pace, and give NDI and
    /// DeckLink a picture for each picture of a playing video, or at the NDI screens' own rate.
    FadeBegan,
    FadeEnded,
    /// The video gave its new picture this long after the one before.
    VideoStill(Duration),
}

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
    /// Where the painter's own time has gone (see `paint_work`).
    pub work: vello::PaintWork,
}

/// One pass's steps: when it began, each part's time and count, its longest step and what that
/// was for, and the running totals at its start.
#[derive(Clone, Copy, Debug)]
struct Pass {
    began: Instant,
    took: [Duration; PARTS],
    times: [u32; PARTS],
    longest: (Part, For, Duration),
    at_begin: Totals,
}

/// A long pass kept: its steps, how long it took, and the running totals at its end.
#[derive(Clone, Copy, Debug)]
struct Kept {
    pass: Pass,
    took: Duration,
    at_end: Totals,
}

/// The pass under way, the counts for the per-minute line, and what is kept for saying what
/// held up an NDI picture.
#[derive(Debug)]
pub(super) struct Held {
    pass: Pass,
    /// How deep the thread is in building a scene: a stage screen's picture of another screen
    /// is built inside its own, and the time counts once.
    building: u8,
    minute: Minute,
    /// The last few passes of [`LONG`] or more, and the next place to keep one (the oldest).
    kept: [Option<Kept>; KEPT],
    next_kept: usize,
    /// The last few other things that can hold a picture up, with when, and the next place.
    happened: [Option<(Instant, Happened)>; HAPPENINGS],
    next_happened: usize,
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

impl Pass {
    fn new(now: Instant, totals: Totals) -> Pass {
        Pass {
            began: now,
            took: [Duration::ZERO; PARTS],
            times: [0; PARTS],
            longest: (Part::Acquire, For::Nothing, Duration::ZERO),
            at_begin: totals,
        }
    }

    /// The line for a pass that was held `took`, the running totals being `now` at its end:
    /// when it began (by the clock), its longest step, every part it had, and what was left
    /// over. `name` says what a step was for, in words.
    fn words(&self, took: Duration, began_at: &str, now: Totals, name: impl Fn(For) -> String) -> String {
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
                said.push_str(&format!(" (GPU wait {}, {runs} runs, {reruns} again for room", ms(waited)));
                said.push_str(&paint_work(self.at_begin.work, now.work));
                said.push(')');
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
}

/// What the painter's own time in a pass was made of, from its running totals at the pass's
/// start and end: working out what to send, sending pictures' pixels to the GPU from memory,
/// copying pictures already there, making the texture the renderer keeps pictures in, handing
/// the commands over, and the rest. Parts that did not happen are left out.
fn paint_work(began: vello::PaintWork, now: vello::PaintWork) -> String {
    let megabytes = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    let mut said = format!("; working out {}", ms(now.resolving.saturating_sub(began.resolving)));
    let sent = now.pictures_sent - began.pictures_sent;
    if sent > 0 {
        said.push_str(&format!(
            ", {sent} pictures sent from memory ({:.0} MB) {}",
            megabytes(now.bytes_sent - began.bytes_sent),
            ms(now.sending.saturating_sub(began.sending))
        ));
    }
    let copied = now.pictures_copied - began.pictures_copied;
    if copied > 0 {
        said.push_str(&format!(", {copied} pictures copied on the GPU"));
    }
    let made = now.atlases_made - began.atlases_made;
    if made > 0 {
        said.push_str(&format!(
            ", pictures' texture made ×{made} ({:.0} MB) {}",
            megabytes(now.atlas_bytes_made - began.atlas_bytes_made),
            ms(now.atlas_making.saturating_sub(began.atlas_making))
        ));
    }
    said.push_str(&format!(
        ", handing over {}, commands {}",
        ms(now.handing_over.saturating_sub(began.handing_over)),
        ms(now.recording.saturating_sub(began.recording))
    ));
    said
}

impl Held {
    pub(super) fn new(now: Instant) -> Held {
        Held {
            pass: Pass::new(now, Totals::default()),
            building: 0,
            minute: Minute::default(),
            kept: [None; KEPT],
            next_kept: 0,
            happened: [None; HAPPENINGS],
            next_happened: 0,
        }
    }

    /// A pass begins, with the running totals as they stand.
    pub(super) fn begin(&mut self, now: Instant, totals: Totals) {
        crate::reports::stats::busy(crate::reports::stats::Who::Screens);
        self.pass = Pass::new(now, totals);
        self.building = 0;
    }

    /// One step of a part took this long.
    pub(super) fn add(&mut self, part: Part, what: For, took: Duration) {
        // For the anonymous reports: the number of the step the thread last finished.
        crate::reports::stats::phase(crate::reports::stats::Who::Screens, part.at() as u32 + 1);
        let pass = &mut self.pass;
        pass.took[part.at()] += took;
        pass.times[part.at()] += 1;
        if took > pass.longest.2 {
            pass.longest = (part, what, took);
        }
    }

    /// Whether the pass under way, ending now, is long enough to keep ([`Held::keep`]).
    pub(super) fn is_long(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.pass.began) >= LONG
    }

    /// Keeps the pass under way, ending now with the running totals at `at_end`, in place of
    /// the oldest kept.
    pub(super) fn keep(&mut self, now: Instant, at_end: Totals) {
        let took = now.saturating_duration_since(self.pass.began);
        self.kept[self.next_kept] = Some(Kept { pass: self.pass, took, at_end });
        self.next_kept = (self.next_kept + 1) % KEPT;
    }

    /// Something that can hold a picture up happened now, in place of the oldest such.
    pub(super) fn happened(&mut self, now: Instant, what: Happened) {
        self.happened[self.next_happened] = Some((now, what));
        self.next_happened = (self.next_happened + 1) % HAPPENINGS;
    }

    /// What the thread was doing from `from` to `to`, for the line that says an NDI picture came
    /// late then: a fade, pictures the GPU was too slow with, NDI pictures lost or put off, the
    /// video's pause between pictures, and the longest pass of [`LONG`] or more that ran then,
    /// written as a held pass is. `name` says what a step was for, in words.
    pub(super) fn then(&self, from: Instant, to: Instant, name: impl Fn(For) -> String) -> String {
        let overlap = |began: Instant, ended: Instant| ended.min(to).saturating_duration_since(began.max(from));
        let (mut fading_since, mut faded) = (None, Duration::ZERO);
        let (mut slow, mut lost, mut put_off, mut still) = (0, 0, 0, Duration::ZERO);
        // Oldest first: from the place after the newest.
        let order = (0..HAPPENINGS).map(|at| self.happened[(self.next_happened + at) % HAPPENINGS]);
        for (at, what) in order.flatten() {
            match what {
                Happened::FadeBegan => fading_since = Some(at),
                Happened::FadeEnded => {
                    faded += fading_since.take().map_or(Duration::ZERO, |began| overlap(began, at));
                }
                Happened::VideoStill(gap) => {
                    if !overlap(at.checked_sub(gap).unwrap_or(at), at).is_zero() {
                        still = still.max(gap);
                    }
                }
                _ if at < from || at > to => {}
                Happened::GpuTooSlow => slow += 1,
                Happened::NdiLost => lost += 1,
                Happened::NdiPutOff => put_off += 1,
            }
        }
        // A fade still under way.
        faded += fading_since.map_or(Duration::ZERO, |began| overlap(began, to));
        let mut facts = Vec::new();
        if !faded.is_zero() {
            facts.push(format!("a fade ran in the windows for {} of it", ms(faded)));
        }
        if slow > 0 {
            facts.push(format!("the GPU was too slow with {}", count(slow, "picture", "pictures")));
        }
        if lost > 0 {
            facts.push(format!("{} lost on the way back from the GPU", count(lost, "NDI picture", "NDI pictures")));
        }
        if put_off > 0 {
            let drawings = count(put_off, "NDI drawing", "NDI drawings");
            facts.push(format!("{drawings} put off, the GPU still reading back the one before"));
        }
        if !still.is_zero() {
            facts.push(format!("the video gave no new picture for {}", ms(still)));
        }
        let longest = self
            .kept
            .iter()
            .flatten()
            .filter(|kept| !overlap(kept.pass.began, kept.pass.began + kept.took).is_zero())
            .max_by_key(|kept| kept.took);
        let mut words = facts.join("; ");
        match longest {
            Some(kept) => {
                if !words.is_empty() {
                    words.push_str(". ");
                }
                words.push_str(&kept.pass.words(kept.took, &clock(kept.pass.began), kept.at_end, name));
            }
            None if words.is_empty() => {
                words = format!("nothing that holds a picture up (no pass took {} or more).", ms(LONG));
            }
            None => words.push_str(&format!("; no pass took {} or more.", ms(LONG))),
        }
        words
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
        use crate::reports::stats::{Count, bump};
        bump(if shown { Count::WindowShown } else { Count::WindowNotShown });
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
        let took = now.saturating_duration_since(self.pass.began);
        crate::reports::stats::screens_pass(took);
        crate::reports::stats::idle(crate::reports::stats::Who::Screens);
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

    /// The line for the pass under way, held `took` (see `Pass::words`).
    pub(super) fn words(&self, took: Duration, began_at: &str, now: Totals, name: impl Fn(For) -> String) -> String {
        self.pass.words(took, began_at, now, name)
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

/// How many of something, in words: "1 picture", "3 pictures".
fn count(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The time of day an instant was, to the thousandth of a second, as the log writes times.
pub(super) fn clock(at: Instant) -> String {
    let ago = Instant::now().saturating_duration_since(at);
    (chrono::Local::now() - chrono::Duration::from_std(ago).unwrap_or_default()).format("%H:%M:%S%.3f").to_string()
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
        let work = vello::PaintWork {
            resolving: Duration::from_millis(1),
            sending: Duration::from_millis(2),
            pictures_sent: 1,
            bytes_sent: 32 * 1024 * 1024,
            atlas_making: Duration::from_micros(500),
            atlases_made: 1,
            atlas_bytes_made: 64 * 1024 * 1024,
            handing_over: Duration::from_micros(300),
            recording: Duration::from_micros(200),
            ..Default::default()
        };
        let at_end = Totals { gpu_wait: Duration::from_millis(3), runs: 12, reruns: 1, work, ..Totals::default() };
        let line = held.words(took, "10:00:16.976", at_end, named);
        assert!(line.starts_with("The screens' thread was held 1019.0 ms from 10:00:16.976."), "{line}");
        assert!(line.contains("Longest step: surface acquire for the window TV, 1003.0 ms."), "{line}");
        assert!(line.contains("vello paint 8.0 ms ×2 (GPU wait 2.0 ms, 2 runs, 1 again for room; "), "{line}");
        // What the painter's own time was made of; a part that did not happen is left out.
        assert!(
            line.contains(
                "working out 1.0 ms, 1 pictures sent from memory (32 MB) 2.0 ms, pictures' texture made ×1 (64 MB) \
                 0.5 ms, handing over 0.3 ms, commands 0.2 ms)"
            ),
            "{line}"
        );
        assert!(!line.contains("copied on the GPU"), "{line}");
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
        assert_eq!(held.pass.times[Part::Scene.at()], 1);
        // And the next one counts again.
        let next = held.scene_begins();
        held.scene_built(next, For::Nothing);
        assert_eq!(held.pass.times[Part::Scene.at()], 2);
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

    #[test]
    fn what_held_up_an_ndi_picture_is_what_happened_in_its_time_and_the_longest_pass_then() {
        let ms = Duration::from_millis;
        let start = Instant::now();
        let mut held = Held::new(start);
        // A pass of 120 ms (a window's picture slow to come), kept; a quick one, not.
        held.begin(start + ms(1000), Totals::default());
        assert!(!held.is_long(start + ms(1010)));
        held.add(Part::Acquire, For::Window(Uuid::nil()), ms(110));
        assert!(held.is_long(start + ms(1120)));
        held.keep(start + ms(1120), Totals::default());
        // A fade from 900 ms to 1300 ms, two pictures the GPU was too slow with, an NDI
        // picture lost, and the video still for 150 ms up to 1250 ms. Some of it is later.
        held.happened(start + ms(900), Happened::FadeBegan);
        held.happened(start + ms(1100), Happened::GpuTooSlow);
        held.happened(start + ms(1150), Happened::GpuTooSlow);
        held.happened(start + ms(1200), Happened::NdiLost);
        held.happened(start + ms(1250), Happened::VideoStill(ms(150)));
        held.happened(start + ms(1300), Happened::FadeEnded);
        held.happened(start + ms(3000), Happened::GpuTooSlow);
        let then = held.then(start + ms(1000), start + ms(1400), named);
        assert!(
            then.starts_with(
                "a fade ran in the windows for 300.0 ms of it; the GPU was too slow with 2 pictures; 1 NDI picture \
                 lost on the way back from the GPU; the video gave no new picture for 150.0 ms. The screens' thread \
                 was held 120.0 ms from "
            ),
            "{then}"
        );
        assert!(then.contains("Longest step: surface acquire for the window TV, 110.0 ms."), "{then}");
        // Before all of it: nothing, and no pass.
        assert_eq!(
            held.then(start, start + ms(500), named),
            "nothing that holds a picture up (no pass took 33.3 ms or more)."
        );
        // A fade still under way counts up to the end of the time asked about.
        held.happened(start + ms(4000), Happened::FadeBegan);
        assert_eq!(
            held.then(start + ms(4100), start + ms(4500), named),
            "a fade ran in the windows for 400.0 ms of it; no pass took 33.3 ms or more."
        );
        // Only the last sixteen are kept: sixteen more push the rest out.
        for at in 0..HAPPENINGS as u64 {
            held.happened(start + ms(5000 + at), Happened::NdiPutOff);
        }
        assert!(held.then(start + ms(5000), start + ms(6000), named).starts_with("16 NDI drawings put off"));
        assert!(!held.then(start + ms(1000), start + ms(1400), named).contains("GPU was too slow"));
    }
}
