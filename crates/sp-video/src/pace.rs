//! How evenly a playing video's pictures reach the screens, counted where the screens take each
//! one from the player (whichever engine plays it), for the app to say once a minute while a
//! video plays and when it stops (`Player::take_pace`), and in all, over the minutes taken that
//! way, for anything else that reports on the app (`take_pace_totals`).
//!
//! Each picture carries its time in the video and the moment it was due on screen. When the
//! screens take one:
//! - it is **shown**;
//! - pictures of the video between it and the one taken before were never taken: **dropped**
//!   (a newer one was there first, so the screens didn't look in time);
//! - taken more than one picture's time after it was due, it is **late** (the decoder or the
//!   screens fell behind);
//! - taken more than one and a half pictures' times after the one before, the picture before was
//!   on screen for longer than its time: each whole picture's time over one is **repeated**.
//!
//! What a pause, a seek, a loop going round or another file taking over leaves between two
//! pictures counts as none of these: the next picture begins afresh. **Expected** is the time it
//! played (not paused) times its pictures a second, so a video that kept up shows about as many
//! as expected.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A minute (or the part of one a video played before it stopped) of a video's pictures
/// reaching the screens, or (from [`take_pace_totals`]) all of them since the last look.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaceMinute {
    /// Its pictures a second, as the file says.
    pub fps: f64,
    /// How long it played (not paused).
    pub played: Duration,
    /// The pictures it played in that time, at its pictures a second.
    pub expected: u64,
    /// The pictures FFmpeg gave; `None` for the system's player, which gives one only when the
    /// screens ask.
    pub decoded: Option<u64>,
    pub shown: u64,
    pub dropped: u64,
    pub repeated: u64,
    pub late: u64,
    /// The longest time between two pictures taken while it played.
    pub longest_gap: Duration,
}

impl PaceMinute {
    /// The log's words for it, for the video of this name.
    pub fn words(&self, name: &str) -> String {
        let decoded = self.decoded.map_or(String::new(), |decoded| format!("{decoded} decoded, "));
        let gap = if self.longest_gap.is_zero() {
            String::new()
        } else {
            format!(", longest gap {:.1} ms", self.longest_gap.as_secs_f64() * 1000.0)
        };
        format!(
            "{name} at {} fps, in the {:.0} s it played: {} pictures expected, {decoded}{} shown, {} dropped, {} \
             repeated, {} late{gap}",
            fps_words(self.fps),
            self.played.as_secs_f64(),
            self.expected,
            self.shown,
            self.dropped,
            self.repeated,
            self.late,
        )
    }

    /// Adds another's counts to these (the rate becomes the other's).
    fn add(&mut self, other: &PaceMinute) {
        self.fps = other.fps;
        self.played += other.played;
        self.expected += other.expected;
        self.decoded = match (self.decoded, other.decoded) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
        self.shown += other.shown;
        self.dropped += other.dropped;
        self.repeated += other.repeated;
        self.late += other.late;
        self.longest_gap = self.longest_gap.max(other.longest_gap);
    }
}

/// A rate as the log says it: `30`, `29.97`, `23.976`.
pub fn fps_words(fps: f64) -> String {
    let said = format!("{fps:.3}");
    said.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The sum of every minute taken with `Player::take_pace` since [`take_pace_totals`] was last
/// asked (the app takes them for the video behind the words), for whatever reports on the app;
/// nothing in the app itself asks.
static TOTALS: Mutex<Option<PaceMinute>> = Mutex::new(None);

/// What the pictures of the videos whose minutes were taken (see `Player::take_pace`) did since
/// this was last asked (see [`PaceMinute`]), and begins counting again; `None` when none has
/// played since.
pub fn take_pace_totals() -> Option<PaceMinute> {
    TOTALS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
}

/// One player's counts: its engines hand out the pictures (and FFmpeg's says what it decoded),
/// the player says when it plays.
#[derive(Debug, Default)]
pub(crate) struct Pace(Mutex<Counts>);

#[derive(Debug, Default)]
struct Counts {
    minute: PaceMinute,
    /// When this minute's counting began.
    since: Option<Instant>,
    /// The picture taken last, while this run of pictures lasts: its time in the video, and
    /// when it was taken.
    last: Option<(f64, Instant)>,
    /// When the player was last seen playing, to add the time since.
    playing_since: Option<Instant>,
}

impl Pace {
    fn counts(&self) -> std::sync::MutexGuard<'_, Counts> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The video's pictures a second (another file's, once it takes over).
    pub(crate) fn set_fps(&self, fps: f64) {
        self.counts().minute.fps = fps;
    }

    /// FFmpeg has given a picture.
    pub(crate) fn decoded(&self) {
        let mut counts = self.counts();
        *counts.minute.decoded.get_or_insert(0) += 1;
    }

    /// The screens have taken a picture: `at` is its time in the video, in seconds, and `due` when
    /// it was due on screen.
    pub(crate) fn taken(&self, at: f64, due: Instant) {
        self.taken_now(at, due, Instant::now());
    }

    fn taken_now(&self, at: f64, due: Instant, now: Instant) {
        let mut counts = self.counts();
        counts.since.get_or_insert(now);
        let fps = counts.minute.fps;
        let minute = &mut counts.minute;
        minute.shown += 1;
        if fps > 0.0 {
            let picture = 1.0 / fps;
            if now.saturating_duration_since(due).as_secs_f64() > picture {
                minute.late += 1;
            }
            // Later in the same run of pictures (a step back, or a long way on, is a new run).
            if let Some((before, taken)) = counts.last
                && at > before
                && at - before < 2.0
            {
                let minute = &mut counts.minute;
                let skipped = ((at - before) * fps).round() as u64;
                minute.dropped += skipped.saturating_sub(1);
                let gap = now.saturating_duration_since(taken);
                minute.longest_gap = minute.longest_gap.max(gap);
                if gap.as_secs_f64() > 1.5 * picture {
                    minute.repeated += ((gap.as_secs_f64() * fps).round() as u64).saturating_sub(1);
                }
            }
        }
        counts.last = Some((at, now));
    }

    /// A pause, a seek or a start: the next picture begins a new run, with nothing counted for
    /// what lies between.
    pub(crate) fn break_run(&self) {
        self.counts().last = None;
    }

    /// The player is (or isn't) playing now: the time it has played since it was last seen
    /// playing is added. Called as it is kept going, a few times a second.
    pub(crate) fn tick(&self, playing: bool) {
        self.tick_now(playing, Instant::now());
    }

    fn tick_now(&self, playing: bool, now: Instant) {
        let mut counts = self.counts();
        counts.since.get_or_insert(now);
        if let Some(since) = counts.playing_since {
            counts.minute.played += now.saturating_duration_since(since);
        }
        counts.playing_since = playing.then_some(now);
        if !playing {
            counts.last = None;
        }
    }

    /// Once a minute of counting has passed, or at once with `now_anyway` (the video is
    /// stopping): what was counted, and counting begins again; `None` before then, or when it
    /// didn't play at all. Every minute taken is added to [`take_pace_totals`]'s.
    pub(crate) fn take(&self, now_anyway: bool) -> Option<PaceMinute> {
        self.take_at(now_anyway, Instant::now())
    }

    fn take_at(&self, now_anyway: bool, now: Instant) -> Option<PaceMinute> {
        let mut counts = self.counts();
        let since = *counts.since.get_or_insert(now);
        if !now_anyway && now.saturating_duration_since(since) < Duration::from_secs(60) {
            return None;
        }
        // The time played up to now belongs to this minute.
        if let Some(playing) = counts.playing_since {
            counts.minute.played += now.saturating_duration_since(playing);
            counts.playing_since = Some(now);
        }
        let fps = counts.minute.fps;
        let mut minute = std::mem::replace(&mut counts.minute, PaceMinute { fps, ..PaceMinute::default() });
        counts.since = Some(now);
        drop(counts);
        if minute.played.is_zero() && minute.shown == 0 {
            return None;
        }
        minute.expected = (minute.played.as_secs_f64() * minute.fps).round() as u64;
        let mut totals = TOTALS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        totals.get_or_insert_with(PaceMinute::default).add(&minute);
        Some(minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pace(fps: f64) -> Pace {
        let pace = Pace::default();
        pace.set_fps(fps);
        pace
    }

    #[test]
    fn a_video_that_keeps_up_shows_every_picture_once_and_none_late() {
        let pace = pace(30.0);
        let start = Instant::now();
        pace.tick_now(true, start);
        for n in 0..300u32 {
            let due = start + Duration::from_secs_f64(f64::from(n) / 30.0);
            // Taken 5 ms after it was due, as the screens look every few milliseconds.
            pace.taken_now(f64::from(n) / 30.0, due, due + Duration::from_millis(5));
        }
        let minute = pace.take_at(true, start + Duration::from_secs(10)).expect("it played");
        assert_eq!((minute.expected, minute.shown, minute.dropped, minute.repeated, minute.late), (300, 300, 0, 0, 0));
        assert!((minute.longest_gap.as_secs_f64() - 1.0 / 30.0).abs() < 0.001);
        assert_eq!(
            minute.words("clip.mkv"),
            "clip.mkv at 30 fps, in the 10 s it played: 300 pictures expected, 300 shown, 0 dropped, 0 repeated, \
             0 late, longest gap 33.3 ms"
        );
    }

    #[test]
    fn screens_that_take_every_other_picture_drop_one_and_repeat_one_for_each() {
        let pace = pace(60.0);
        let start = Instant::now();
        pace.tick_now(true, start);
        for n in (0..120u32).step_by(2) {
            let due = start + Duration::from_secs_f64(f64::from(n) / 60.0);
            pace.taken_now(f64::from(n) / 60.0, due, due + Duration::from_millis(2));
        }
        let minute = pace.take_at(true, start + Duration::from_secs(2)).expect("it played");
        assert_eq!((minute.expected, minute.shown, minute.dropped, minute.repeated, minute.late), (120, 60, 59, 59, 0));
    }

    #[test]
    fn a_decoder_that_falls_behind_makes_its_pictures_late_and_the_ones_before_repeated() {
        let pace = pace(30.0);
        let start = Instant::now();
        pace.tick_now(true, start);
        // Every picture comes 100 ms or more after it was due, 60 ms apart: each is late, and
        // each before it stays up for a picture's time too long.
        for n in 0..10u32 {
            let due = start + Duration::from_secs_f64(f64::from(n) / 30.0);
            let came = start + Duration::from_millis(100 + 60 * u64::from(n));
            pace.taken_now(f64::from(n) / 30.0, due, came);
        }
        let minute = pace.take_at(true, start + Duration::from_millis(700)).expect("it played");
        assert_eq!((minute.shown, minute.dropped, minute.late), (10, 0, 10));
        assert_eq!(minute.repeated, 9);
    }

    #[test]
    fn a_pause_a_loop_and_a_seek_count_as_nothing_and_paused_time_expects_no_pictures() {
        let pace = pace(25.0);
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        pace.tick_now(true, start);
        pace.taken_now(0.0, start, start);
        pace.taken_now(0.04, at(40), at(40));
        // Paused for two seconds: no time played, and the picture after it begins afresh.
        pace.tick_now(false, at(1000));
        pace.tick_now(false, at(2000));
        pace.tick_now(true, at(3000));
        pace.taken_now(0.08, at(3000), at(3000));
        // Round to the start (a step back), and a seek far on: new runs.
        pace.taken_now(0.0, at(3040), at(3040));
        pace.break_run();
        pace.taken_now(9.0, at(3080), at(3080));
        let minute = pace.take_at(true, at(4000)).expect("it played");
        assert_eq!(minute.played, Duration::from_secs(2));
        assert_eq!((minute.expected, minute.shown, minute.dropped, minute.repeated, minute.late), (50, 5, 0, 0, 0));
    }

    #[test]
    fn a_minute_comes_once_a_minute_and_counting_begins_again() {
        let pace = pace(30.0);
        let start = Instant::now();
        pace.tick_now(true, start);
        assert!(pace.take_at(false, start + Duration::from_secs(30)).is_none());
        let first = pace.take_at(false, start + Duration::from_secs(61)).expect("a minute");
        assert_eq!(first.expected, 1830);
        let second = pace.take_at(true, start + Duration::from_secs(62)).expect("it plays on");
        assert_eq!((second.played, second.expected), (Duration::from_secs(1), 30));
        // Stopped: nothing more played, so no minute.
        pace.tick_now(false, start + Duration::from_secs(62));
        assert!(pace.take_at(true, start + Duration::from_secs(200)).is_none());
    }

    #[test]
    fn rates_are_said_as_people_say_them() {
        assert_eq!(fps_words(30.0), "30");
        assert_eq!(fps_words(30000.0 / 1001.0), "29.97");
        assert_eq!(fps_words(24000.0 / 1001.0), "23.976");
        assert_eq!(fps_words(25.0), "25");
    }
}
