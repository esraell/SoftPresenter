//! How the computer writes the time of day, for the clock in the bar along the top: on a
//! 12-hour or a 24-hour clock, with which words for morning and afternoon, and where they go.
//!
//! The computer's own setting is read, not only its language's habit. Reading it means running
//! one of the system's small programs, so it is done on a thread of its own: once when the
//! window opens, then at most every ten minutes, and when the window comes to the front again. The
//! clock is drawn with what was last read; nothing waits for the reading.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How a time of day is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeForm {
    /// A 12-hour clock, with a word for morning or afternoon.
    pub twelve: bool,
    /// The hour always has two figures (`03`, not `3`).
    pub pad_hour: bool,
    /// What stands between the hours, the minutes and the seconds.
    pub separator: String,
    /// The words for before and after noon, as the computer writes them.
    pub am: String,
    pub pm: String,
    /// The word stands before the figures, not after them.
    pub marker_first: bool,
}

impl Default for TimeForm {
    /// A 24-hour clock, until the computer's setting has been read (or where it can't be).
    fn default() -> TimeForm {
        TimeForm {
            twelve: false,
            pad_hour: true,
            separator: ":".to_owned(),
            am: "AM".to_owned(),
            pm: "PM".to_owned(),
            marker_first: false,
        }
    }
}

impl TimeForm {
    /// The figures of a time of day: `15:04:05`, or `3:04:05` on a 12-hour clock.
    pub fn figures(&self, hour: u32, minute: u32, second: u32) -> String {
        let hour = match (self.twelve, hour % 12) {
            (false, _) => hour,
            (true, 0) => 12,
            (true, hour) => hour,
        };
        let sep = &self.separator;
        if self.pad_hour {
            format!("{hour:02}{sep}{minute:02}{sep}{second:02}")
        } else {
            format!("{hour}{sep}{minute:02}{sep}{second:02}")
        }
    }
}

/// One time as the computer wrote it, taken apart: the words before its figures, the figures
/// of its hour, what stands between the figures, and the words after them.
#[cfg(any(test, target_os = "macos"))]
fn parts(written: &str) -> Option<(String, &str, String, String)> {
    let digit = |c: char| c.is_ascii_digit();
    let start = written.find(digit)?;
    let rest = &written[start..];
    let hour = &rest[..rest.find(|c| !digit(c))?];
    let after_hour = &rest[hour.len()..];
    let separator = &after_hour[..after_hour.find(digit)?];
    // The minutes, the separator again and the seconds; what is left are the words after.
    let mut tail = &after_hour[separator.len()..];
    for _ in 0..2 {
        tail = tail.trim_start_matches(digit);
        tail = tail.strip_prefix(separator).unwrap_or(tail);
    }
    let tail = tail.trim_start_matches(digit);
    Some((written[..start].trim().to_owned(), hour, separator.to_owned(), tail.trim().to_owned()))
}

/// The way of writing the time, worked out from one o'clock at night and one o'clock in the
/// afternoon as the computer wrote them (`1:00:00 AM` and `1:00:00 PM`, or `01:00:00` and
/// `13:00:00`). `None` when they can't be read (figures of another script, say).
#[cfg(any(test, target_os = "macos"))]
pub fn from_written(night: &str, afternoon: &str) -> Option<TimeForm> {
    let (before_am, night_hour, separator, after_am) = parts(night)?;
    let (before_pm, afternoon_hour, _, after_pm) = parts(afternoon)?;
    let twelve = match afternoon_hour.parse::<u32>().ok()? {
        1 => true,
        13 => false,
        _ => return None,
    };
    let marker_first = !before_am.is_empty();
    let (am, pm) = if marker_first { (before_am, before_pm) } else { (after_am, after_pm) };
    let usual = TimeForm::default();
    Some(TimeForm {
        twelve,
        pad_hour: night_hour.len() == 2,
        separator,
        am: if am.is_empty() { usual.am } else { am },
        pm: if pm.is_empty() { usual.pm } else { pm },
        marker_first,
    })
}

/// A Mac's way of writing the time. The two switches in its settings decide when one is on
/// (`force_24` is "24-hour time" switched on where the language's habit is 12, `force_12` the
/// other way about); otherwise it is as the system wrote a time (`written`), which also gives
/// the words for morning and afternoon.
#[cfg(any(test, target_os = "macos"))]
pub fn on_a_mac(force_24: bool, force_12: bool, written: Option<TimeForm>) -> TimeForm {
    let mut form = written.unwrap_or_default();
    if force_24 && form.twelve {
        form.twelve = false;
        form.pad_hour = true;
    } else if force_12 && !force_24 && !form.twelve {
        form.twelve = true;
        form.pad_hour = false;
    }
    form
}

/// Windows' way of writing the time, from what `reg query` prints of the user's
/// `Control Panel\International` key: `sTimeFormat` (a picture such as `h:mm:ss tt` or
/// `HH:mm:ss`), with `s1159` and `s2359` for the words; without a picture, the older `iTime`
/// (0 for a 12-hour clock) and `iTLZero`. `None` when neither is there.
#[cfg(any(test, target_os = "windows"))]
pub fn on_windows(printed: &str) -> Option<TimeForm> {
    let value = |name: &str| {
        printed.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == name && words.next()?.starts_with("REG_")).then(|| {
                let at = line.find("REG_").unwrap_or(0);
                line[at..].split_once(char::is_whitespace).map_or("", |(_, value)| value).trim().to_owned()
            })
        })
    };
    let usual = TimeForm::default();
    // A word that came through whole; one in letters the program could not print is left out.
    let word = |name: &str, otherwise: &str| {
        value(name).filter(|word| !word.is_empty() && !word.contains(['\u{fffd}', '?'])).unwrap_or(otherwise.to_owned())
    };
    let (am, pm) = (word("s1159", &usual.am), word("s2359", &usual.pm));
    if let Some(picture) = value("sTimeFormat").filter(|picture| picture.contains(['h', 'H'])) {
        // Words in quotes are written as they are; they are not part of the picture.
        let mut plain = String::new();
        let mut quoted = false;
        for c in picture.chars() {
            match c {
                '\'' => quoted = !quoted,
                c if !quoted => plain.push(c),
                _ => {}
            }
        }
        let hour_at = plain.find(['h', 'H'])?;
        let hour = plain[hour_at..].chars().next()?;
        let run = plain[hour_at..].chars().take_while(|c| *c == hour).count();
        let after = &plain[hour_at + run..];
        let separator = after[..after.find('m').unwrap_or(0)].to_owned();
        return Some(TimeForm {
            twelve: hour == 'h',
            pad_hour: run >= 2,
            separator: if separator.is_empty() { usual.separator } else { separator },
            am,
            pm,
            marker_first: plain.find('t').is_some_and(|at| at < hour_at),
        });
    }
    let twelve = value("iTime")? == "0";
    Some(TimeForm { twelve, pad_hour: value("iTLZero").is_none_or(|zero| zero == "1"), am, pm, ..usual })
}

/// The way of writing the time on Linux and the like, from what `locale t_fmt am_pm` prints:
/// the time's pattern (`%r`, `%T`, `%I:%M:%S %p`…) on one line and the two words, with a
/// semicolon between, on the next. `None` when there is no pattern.
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
pub fn from_locale(printed: &str) -> Option<TimeForm> {
    let mut lines = printed.lines();
    let pattern = lines.next().map(str::trim).filter(|pattern| pattern.contains('%'))?;
    let words = lines.next().unwrap_or("");
    // Each `%` thing in the pattern: its letter, whether it is written without a leading
    // nought (`%-I`, `%l`, `%k`), and where it stands.
    let mut things = Vec::new();
    let mut chars = pattern.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c != '%' {
            continue;
        }
        let mut bare = false;
        while let Some((_, flag)) = chars.next_if(|&(_, c)| matches!(c, '-' | '_' | '0' | '^' | '#' | 'E' | 'O')) {
            bare |= flag == '-';
        }
        if let Some((_, letter)) = chars.next() {
            things.push((letter, bare || matches!(letter, 'l' | 'k'), at));
        }
    }
    let hour = things.iter().find(|(letter, ..)| matches!(letter, 'r' | 'I' | 'l' | 'T' | 'H' | 'k' | 'R' | 'X'))?;
    let twelve = matches!(hour.0, 'r' | 'I' | 'l') || things.iter().any(|(letter, ..)| matches!(letter, 'p' | 'P'));
    let marker = things.iter().find(|(letter, ..)| matches!(letter, 'p' | 'P'));
    let usual = TimeForm::default();
    let (am, pm) = words.split_once(';').map_or(("", ""), |(am, pm)| (am.trim(), pm.trim()));
    Some(TimeForm {
        twelve,
        pad_hour: !hour.1,
        am: if am.is_empty() { usual.am.clone() } else { am.to_owned() },
        pm: if pm.is_empty() { usual.pm.clone() } else { pm.to_owned() },
        marker_first: marker.is_some_and(|marker| marker.2 < hour.2),
        ..usual
    })
}

/// What a small program of the system's prints, or `None` when it can't be run or fails.
fn printed(mut command: std::process::Command) -> Option<String> {
    let output = command.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Reads the computer's way of writing the time. Slow (it runs programs): never on the thread
/// that draws.
#[cfg(target_os = "macos")]
fn read_system() -> Option<TimeForm> {
    use std::process::Command;
    // The switch in System Settings, kept among the settings every app shares.
    let switched = |name: &str| {
        let mut command = Command::new("/usr/bin/defaults");
        command.args(["read", "-g", name]);
        printed(command).is_some_and(|value| value.trim() == "1")
    };
    // One o'clock at night and in the afternoon, as the system writes them for this user.
    let mut script = Command::new("/usr/bin/osascript");
    script.args([
        "-e",
        "set d to current date",
        "-e",
        "set time of d to 3600",
        "-e",
        "set a to time string of d",
        "-e",
        "set time of d to 46800",
        "-e",
        "a & linefeed & time string of d",
    ]);
    let written = printed(script).and_then(|both| {
        let mut lines = both.lines();
        from_written(lines.next()?, lines.next()?)
    });
    Some(on_a_mac(switched("AppleICUForce24HourTime"), switched("AppleICUForce12HourTime"), written))
}

#[cfg(target_os = "windows")]
fn read_system() -> Option<TimeForm> {
    use std::os::windows::process::CommandExt;
    // Run without a console window flashing up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new("reg");
    command.args(["query", r"HKCU\Control Panel\International"]).creation_flags(CREATE_NO_WINDOW);
    on_windows(&printed(command)?)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_system() -> Option<TimeForm> {
    // `locale` answers for LC_ALL, LC_TIME or LANG, whichever is set, as every program does.
    let mut command = std::process::Command::new("locale");
    command.args(["t_fmt", "am_pm"]);
    from_locale(&printed(command)?)
}

/// How often the computer's setting is read again while the window stays open.
const READ_AGAIN: Duration = Duration::from_secs(600);
/// The least time between two readings, however often the window comes to the front.
const NOT_SOONER: Duration = Duration::from_secs(5);

/// The clock's way of writing the time, kept up with the computer's setting.
#[derive(Clone, Default)]
pub struct SystemClock {
    form: TimeForm,
    /// What a reading found, until the window takes it.
    read: Arc<Mutex<Option<TimeForm>>>,
    /// True while a reading is under way: there is never more than one.
    reading: Arc<AtomicBool>,
    asked: Option<Instant>,
    in_front: bool,
}

impl SystemClock {
    /// The way to write the time now. Called as the window is drawn (`in_front` says whether it
    /// has the keyboard): it never waits, and starts a reading on a thread of its own when one
    /// is due.
    pub fn form(&mut self, in_front: bool) -> &TimeForm {
        if let Some(read) = self.read.lock().ok().and_then(|mut read| read.take()) {
            self.form = read;
        }
        let came_forward = in_front && !self.in_front;
        self.in_front = in_front;
        let since = self.asked.map(|at| at.elapsed());
        let due = since.is_none_or(|since| since >= READ_AGAIN || (came_forward && since >= NOT_SOONER));
        // Tests draw the window too; they read nothing of the computer they run on.
        if due && !cfg!(test) && !self.reading.swap(true, Ordering::SeqCst) {
            self.asked = Some(Instant::now());
            let (read, reading) = (self.read.clone(), self.reading.clone());
            let spawned = std::thread::Builder::new().name("clock setting".to_owned()).spawn(move || {
                let found = read_system();
                if let (Some(found), Ok(mut read)) = (found, read.lock()) {
                    *read = Some(found);
                }
                reading.store(false, Ordering::SeqCst);
            });
            if spawned.is_err() {
                self.reading.store(false, Ordering::SeqCst);
            }
        }
        &self.form
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_figures_are_written_on_either_clock() {
        let day = TimeForm::default();
        assert_eq!(day.figures(15, 4, 5), "15:04:05");
        assert_eq!(day.figures(0, 0, 0), "00:00:00");
        let half = TimeForm { twelve: true, pad_hour: false, ..TimeForm::default() };
        assert_eq!(half.figures(15, 4, 5), "3:04:05");
        assert_eq!(half.figures(0, 30, 0), "12:30:00");
        assert_eq!(half.figures(12, 0, 9), "12:00:09");
        assert_eq!(half.figures(11, 59, 59), "11:59:59");
        let padded = TimeForm { pad_hour: true, ..half };
        assert_eq!(padded.figures(15, 4, 5), "03:04:05");
    }

    #[test]
    fn a_time_the_system_wrote_says_which_clock_and_which_words() {
        // As macOS writes them in American English: a narrow space before the word.
        let american = from_written("1:00:00\u{202f}AM", "1:00:00\u{202f}PM").expect("read");
        assert!(american.twelve && !american.pad_hour && !american.marker_first);
        assert_eq!((american.am.as_str(), american.pm.as_str(), american.separator.as_str()), ("AM", "PM", ":"));
        let british = from_written("01:00:00", "13:00:00").expect("read");
        assert!(!british.twelve && british.pad_hour);
        // Words before the figures, and another separator.
        let before = from_written("오전 1:00:00", "오후 1:00:00").expect("read");
        assert!(before.twelve && before.marker_first);
        assert_eq!((before.am.as_str(), before.pm.as_str()), ("오전", "오후"));
        let dotted = from_written("1.00.00", "13.00.00").expect("read");
        assert_eq!((dotted.twelve, dotted.pad_hour, dotted.separator.as_str()), (false, false, "."));
        assert_eq!(dotted.figures(9, 5, 0), "9.05.00");
        // Figures of another script, or nonsense, are not guessed at.
        assert_eq!(from_written("١:٠٠:٠٠ ص", "١:٠٠:٠٠ م"), None);
        assert_eq!(from_written("", ""), None);
        assert_eq!(from_written("1:00:00", "7:00:00"), None);
    }

    #[test]
    fn a_macs_switches_decide_over_its_languages_habit() {
        let twelve = from_written("1:00:00\u{202f}AM", "1:00:00\u{202f}PM");
        let day = from_written("01:00:00", "13:00:00");
        // Neither switch: as the system wrote it.
        assert!(on_a_mac(false, false, twelve.clone()).twelve);
        assert!(!on_a_mac(false, false, day.clone()).twelve);
        // "24-hour time" switched on over a 12-hour habit, and off over a 24-hour one.
        let forced = on_a_mac(true, false, twelve.clone());
        assert_eq!((forced.twelve, forced.figures(15, 4, 5)), (false, "15:04:05".to_owned()));
        let forced = on_a_mac(false, true, day.clone());
        assert_eq!((forced.twelve, forced.figures(15, 4, 5)), (true, "3:04:05".to_owned()));
        // A switch that agrees with what was written changes nothing; nothing read is 24 hours.
        assert_eq!(on_a_mac(true, false, day.clone()), day.clone().unwrap_or_default());
        assert_eq!(on_a_mac(false, true, twelve.clone()), twelve.unwrap_or_default());
        assert_eq!(on_a_mac(false, false, None), TimeForm::default());
        assert!(on_a_mac(false, true, None).twelve);
    }

    #[test]
    fn windows_time_picture_says_which_clock() {
        let printed = "\r\nHKEY_CURRENT_USER\\Control Panel\\International\r\n    iTime    REG_SZ    0\r\n    \
                       s1159    REG_SZ    AM\r\n    s2359    REG_SZ    PM\r\n    sTime    REG_SZ    :\r\n    \
                       sTimeFormat    REG_SZ    h:mm:ss tt\r\n    sShortTime    REG_SZ    h:mm tt\r\n";
        let american = on_windows(printed).expect("read");
        assert!(american.twelve && !american.pad_hour && !american.marker_first);
        assert_eq!(american.figures(15, 4, 5), "3:04:05");
        let day = on_windows("    sTimeFormat    REG_SZ    HH:mm:ss\r\n    s1159    REG_SZ    \r\n").expect("read");
        assert!(!day.twelve && day.pad_hour);
        assert_eq!(day.am, "AM", "an empty word is replaced");
        let before =
            on_windows("    sTimeFormat    REG_SZ    tt hh.mm.ss\n    s1159    REG_SZ    vorm.\n").expect("read");
        assert!(before.twelve && before.pad_hour && before.marker_first);
        assert_eq!((before.separator.as_str(), before.am.as_str()), (".", "vorm."));
        // Quoted words in the picture are not the picture.
        let quoted = on_windows("    sTimeFormat    REG_SZ    'the hour' H:mm:ss\n").expect("read");
        assert!(!quoted.twelve && !quoted.pad_hour);
        // Without a picture, the older values.
        let older = on_windows("    iTime    REG_SZ    0\n    iTLZero    REG_SZ    0\n").expect("read");
        assert!(older.twelve && !older.pad_hour);
        assert!(!on_windows("    iTime    REG_SZ    1\n").expect("read").twelve);
        assert_eq!(on_windows("ERROR: The system was unable to find the specified registry key or value.\n"), None);
    }

    #[test]
    fn a_locales_time_pattern_says_which_clock() {
        let american = from_locale("%r\nAM;PM\n").expect("read");
        assert!(american.twelve && american.pad_hour);
        assert_eq!((american.am.as_str(), american.pm.as_str()), ("AM", "PM"));
        let british = from_locale("%T\n;\n").expect("read");
        assert!(!british.twelve && british.pad_hour);
        assert_eq!(british.am, "AM");
        let spelt = from_locale("%I:%M:%S %p\nAM;PM\n").expect("read");
        assert!(spelt.twelve && !spelt.marker_first);
        let before = from_locale("%p %l:%M:%S\n오전;오후\n").expect("read");
        assert!(before.twelve && before.marker_first && !before.pad_hour);
        assert_eq!(before.pm, "오후");
        assert!(!from_locale("%H:%M:%S\n").expect("read").twelve);
        assert!(!from_locale("%-H:%M:%S\n;\n").expect("read").pad_hour);
        assert_eq!(from_locale(""), None);
        assert_eq!(from_locale("locale: unknown name\n"), None);
    }

    #[test]
    fn drawing_never_reads_the_computer_in_a_test() {
        let mut clock = SystemClock::default();
        assert_eq!(clock.form(true), &TimeForm::default());
        assert!(clock.asked.is_none() && !clock.reading.load(Ordering::SeqCst));
        // What a reading found is taken up the next time the clock is drawn.
        let found = TimeForm { twelve: true, ..TimeForm::default() };
        *clock.read.lock().expect("lock") = Some(found.clone());
        assert_eq!(clock.form(true), &found);
    }
}
