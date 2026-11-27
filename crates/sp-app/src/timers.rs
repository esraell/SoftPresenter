//! Timers: counting down from a length of time, counting down to a time of day, or counting
//! up. Their settings are kept in `timers.json`; whether each is running is not kept.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sp_model::Uuid;

use crate::i18n::{t, tf};

/// What a timer counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    /// Down from this many seconds.
    Countdown { seconds: u32 },
    /// Down to this time of day, on a 24-hour clock.
    CountdownTo { hour: u8, minute: u8, second: u8 },
    /// Up from `start` seconds, stopping at `end` if there is one.
    Elapsed { start: u32, end: Option<u32> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timer {
    pub id: Uuid,
    pub name: String,
    pub kind: Kind,
    /// Keep counting past the end (shown with a minus sign), rather than stopping there.
    #[serde(default)]
    pub allow_overrun: bool,
    /// Running or not, and the time counted so far. Not kept between runs.
    #[serde(skip)]
    pub run: Run,
}

/// Whether a timer is running, and how much it has counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Run {
    /// When it was last started, while it is running.
    pub started: Option<Instant>,
    /// What it had counted before it was last started.
    pub banked: Duration,
}

impl Timer {
    /// A new timer, as the + adds one: a five-minute countdown named "Timer".
    pub fn new(name: String) -> Timer {
        Timer {
            id: Uuid::new_v4(),
            name,
            kind: Kind::Countdown { seconds: 5 * 60 },
            allow_overrun: false,
            run: Run::default(),
        }
    }

    pub fn running(&self) -> bool {
        self.run.started.is_some()
    }

    /// The time counted at `now`.
    fn counted(&self, now: Instant) -> Duration {
        self.run.banked + self.run.started.map_or(Duration::ZERO, |started| now.saturating_duration_since(started))
    }

    pub fn start(&mut self, now: Instant) {
        if self.run.started.is_none() {
            self.run.started = Some(now);
        }
    }

    pub fn stop(&mut self, now: Instant) {
        self.run.banked = self.counted(now);
        self.run.started = None;
    }

    /// Back to the start, stopped.
    pub fn reset(&mut self) {
        self.run = Run::default();
    }

    /// The timer's value in seconds at `now`: what is left for a countdown (below zero once it
    /// has run over, if it may), or what has passed for a count up. `seconds_into_day` is the
    /// wall clock, for counting down to a time of day.
    pub fn value(&self, now: Instant, seconds_into_day: u32) -> i64 {
        let counted = i64::try_from(self.counted(now).as_secs()).unwrap_or(i64::MAX);
        let value = match self.kind {
            Kind::Countdown { seconds } => i64::from(seconds) - counted,
            Kind::CountdownTo { hour, minute, second } => {
                let target = i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
                target - i64::from(seconds_into_day)
            }
            Kind::Elapsed { start, end } => {
                let value = i64::from(start) + counted;
                return match end {
                    Some(end) if !self.allow_overrun => value.min(i64::from(end)),
                    _ => value,
                };
            }
        };
        if self.allow_overrun { value } else { value.max(0) }
    }

    /// The value as the timer shows it: `00:05:00`, or `-00:00:12` once run over.
    pub fn text(&self, now: Instant, seconds_into_day: u32) -> String {
        clock_text(self.value(now, seconds_into_day))
    }

    /// What the timer counts, in words, as the tooltip on its card says it: "Counts down from
    /// 30:00", "Counts down to 11:00 AM", "Counts up".
    pub fn describe(&self) -> String {
        // A length under an hour is said in minutes and seconds.
        let short = |seconds: u32| {
            let text = clock_text(i64::from(seconds));
            text.strip_prefix("00:").map_or(text.clone(), str::to_owned)
        };
        match self.kind {
            Kind::Countdown { seconds } => {
                tf("tools.timer.down_from", "Counts down from {time}", &[("time", &short(seconds))])
            }
            Kind::CountdownTo { hour, minute, .. } => {
                let (am, pm) = (t("tools.timer.am", "AM"), t("tools.timer.pm", "PM"));
                let (twelve, half) = match hour {
                    0 => (12, am),
                    1..=11 => (hour, am),
                    12 => (12, pm),
                    _ => (hour - 12, pm),
                };
                let time = format!("{twelve}:{minute:02} {half}");
                // One o'clock has a word of its own in some languages ("la 1:00", "las 11:00").
                if twelve == 1 {
                    tf("tools.timer.down_to.one", "Counts down to {time}", &[("time", &time)])
                } else {
                    tf("tools.timer.down_to", "Counts down to {time}", &[("time", &time)])
                }
            }
            Kind::Elapsed { start: 0, end: None } => t("tools.timer.up", "Counts up").to_owned(),
            Kind::Elapsed { start, end: None } => {
                tf("tools.timer.up_from", "Counts up from {start}", &[("start", &short(start))])
            }
            Kind::Elapsed { start, end: Some(end) } => tf(
                "tools.timer.up_from_to",
                "Counts up from {start}, stopping at {end}",
                &[("start", &short(start)), ("end", &short(end))],
            ),
        }
    }
}

/// Seconds as `HH:MM:SS`, with a minus sign before a negative value.
pub fn clock_text(seconds: i64) -> String {
    let sign = if seconds < 0 { "-" } else { "" };
    let s = seconds.unsigned_abs();
    format!("{sign}{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Reads `HH:MM:SS`, `MM:SS` or a number of seconds.
pub fn parse_clock(text: &str) -> Option<u32> {
    let parts: Vec<u32> = text.trim().split(':').map(|part| part.trim().parse().ok()).collect::<Option<_>>()?;
    match parts.as_slice() {
        [s] => Some(*s),
        [m, s] if *s < 60 => Some(m * 60 + s),
        [h, m, s] if *m < 60 && *s < 60 => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

/// The seconds since midnight on the computer's clock.
pub fn seconds_into_day() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().num_seconds_from_midnight()
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("timers.json"))
}

/// The timers as last saved; and a file there that couldn't be read, kept aside (`whole_file`).
pub fn load() -> (Vec<Timer>, Option<crate::whole_file::Unread>) {
    file().map_or((Vec::new(), None), |path| load_from(&path))
}

fn load_from(path: &std::path::Path) -> (Vec<Timer>, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::Timers, |text| {
        serde_json::from_str(text).map_err(|e| e.to_string())
    })
}

/// The timers' file and what goes in it, for the library's writer to write (`writer`), paced (`writer::Paced`). The
/// error is a plain sentence.
pub fn to_save(timers: &[Timer]) -> Result<crate::writer::Save, String> {
    let path = file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(to_save_at(path, timers))
}

fn to_save_at(path: std::path::PathBuf, timers: &[Timer]) -> crate::writer::Save {
    crate::whole_file::settings_save(
        path,
        crate::whole_file::What::Timers,
        serde_json::to_string_pretty(timers).unwrap_or_default().into_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timers_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let good = serde_json::to_string_pretty(&vec![Timer::new("Sermon".to_owned())]).unwrap();
        crate::whole_file::tests::check_loader(
            "timers.json",
            crate::whole_file::What::Timers,
            good.as_bytes(),
            |path| load_from(path).1,
            |path| crate::whole_file::tests::written(to_save_at(path.to_owned(), &[])),
        );
    }

    #[test]
    fn a_countdown_runs_pauses_resets_and_stops_at_zero_unless_it_may_run_over() {
        let t0 = Instant::now();
        let mut timer = Timer::new("Sermon".to_owned());
        assert_eq!(timer.text(t0, 0), "00:05:00");
        timer.start(t0);
        assert_eq!(timer.text(t0 + Duration::from_secs(61), 0), "00:03:59");
        timer.stop(t0 + Duration::from_secs(61));
        // Paused, it holds its value.
        assert_eq!(timer.text(t0 + Duration::from_secs(500), 0), "00:03:59");
        timer.start(t0 + Duration::from_secs(500));
        assert_eq!(timer.text(t0 + Duration::from_secs(1000), 0), "00:00:00");
        timer.allow_overrun = true;
        assert_eq!(timer.text(t0 + Duration::from_secs(500 + 239 + 12), 0), "-00:00:12");
        timer.reset();
        assert!(!timer.running());
        assert_eq!(timer.text(t0, 0), "00:05:00");
    }

    #[test]
    fn counting_down_to_a_time_of_day_and_counting_up() {
        let t0 = Instant::now();
        let mut to =
            Timer { kind: Kind::CountdownTo { hour: 11, minute: 0, second: 0 }, ..Timer::new("Start".to_owned()) };
        // At 10:58:30, an eleven o'clock countdown has a minute and a half left.
        assert_eq!(to.text(t0, 10 * 3600 + 58 * 60 + 30), "00:01:30");
        assert_eq!(to.text(t0, 11 * 3600 + 5), "00:00:00");
        to.allow_overrun = true;
        assert_eq!(to.text(t0, 11 * 3600 + 5), "-00:00:05");
        assert_eq!(to.describe(), "Counts down to 11:00 AM");
        assert_eq!(Timer::new("T".to_owned()).describe(), "Counts down from 05:00");
        assert_eq!(
            Timer { kind: Kind::Elapsed { start: 0, end: None }, ..Timer::new("U".to_owned()) }.describe(),
            "Counts up"
        );
        let mut up = Timer { kind: Kind::Elapsed { start: 0, end: Some(90) }, ..Timer::new("Up".to_owned()) };
        up.start(t0);
        assert_eq!(up.text(t0 + Duration::from_secs(75), 0), "00:01:15");
        assert_eq!(up.text(t0 + Duration::from_secs(200), 0), "00:01:30", "stops at its end");
    }

    #[test]
    fn clock_text_reads_back() {
        assert_eq!(parse_clock("00:05:00"), Some(300));
        assert_eq!(parse_clock("5:00"), Some(300));
        assert_eq!(parse_clock("90"), Some(90));
        assert_eq!(parse_clock("1:75"), None);
        assert_eq!(parse_clock("words"), None);
        assert_eq!(clock_text(3725), "01:02:05");
        // Saved and read back, a timer keeps its settings but not whether it was running.
        let mut timer = Timer::new("T".to_owned());
        timer.start(Instant::now());
        let again: Vec<Timer> = serde_json::from_str(&serde_json::to_string(&vec![timer.clone()]).unwrap()).unwrap();
        assert_eq!((again[0].name.as_str(), again[0].kind), ("T", timer.kind));
        assert!(!again[0].running());
    }
}
