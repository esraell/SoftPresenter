//! How the last run ended. While the app runs, a small file named `running` beside the log says
//! which version started when; quitting removes it. A start that finds one left behind says once,
//! in the log, that the last run ended without quitting: it crashed, when a panic was noted in
//! the file (the panic hook notes it there as it writes the crash to the log and the crash
//! report), or else it crashed in a way no panic notes, or was closed from outside (Task Manager,
//! Force Quit, a shutdown).
//!
//! The file is kept beside the log, as the anonymous reports' folder is: in the data folder of a
//! copy that has one of its own, and on a Mac in `~/Library/Logs/SoftPresenter`, this computer's
//! own place, not in the library in Documents, which may be synced to another computer (on Windows
//! and Linux the log and the file are in the data folder). On a Mac and Linux the copy that wrote
//! it holds it locked as long as it runs, so a second copy started on the same folder sees it is
//! in use and leaves it alone rather than taking it for one left behind. It is all done on a
//! thread of its own, and whatever goes wrong with it is passed over: the app never waits for it.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Local};

/// The file's name, in the log's folder.
const NAME: &str = "running";

/// This run's file, held (and locked) from the start until it quits, and where it is.
static HELD: Mutex<Option<(File, PathBuf)>> = Mutex::new(None);
/// The thread that looks at how the last run ended and notes this one.
static LOOKING: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// Notes, when it goes, that the app quit: kept by `main` for as long as the app runs. One that
/// goes as a panic unwinds leaves the file, which says the run crashed.
pub struct Running(());

impl Drop for Running {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            ended();
        }
    }
}

/// As the app starts, on a thread of its own: says in the log how the last run ended, if it
/// didn't quit, and notes this run in its place.
pub fn begin() -> Running {
    let Some(folder) = super::folder() else { return Running(()) };
    let looking = std::thread::Builder::new().name("last run".to_owned()).spawn(move || {
        match begin_in(&folder, crate::updates::VERSION, Local::now()) {
            Begun::Held(file, said) => {
                if let Some(said) = said {
                    super::event("start", &said);
                }
                *HELD.lock().unwrap_or_else(PoisonError::into_inner) = Some((file, folder.join(NAME)));
            }
            Begun::InUse => super::event(
                "start",
                "another SoftPresenter is running with this folder, so how this run ends isn't noted",
            ),
            Begun::Failed => {}
        }
    });
    if let Ok(looking) = looking {
        *LOOKING.lock().unwrap_or_else(PoisonError::into_inner) = Some(looking);
    }
    Running(())
}

/// A panic, noted in this run's file. Called from the panic hook: it never waits for the file,
/// and passes over every failure. A panic caught where a file couldn't be read is noted too, as
/// it is in the crash report; a run that goes on to quit removes the note with the file.
pub fn crashed() {
    let Ok(mut held) = HELD.try_lock() else { return };
    if let Some((file, _)) = held.as_mut() {
        note_crash(file);
    }
}

/// The app quits: this run's file goes. The thread that wrote it is waited for, but it was done
/// long before.
pub fn ended() {
    let looking = LOOKING.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(looking) = looking {
        let _ = looking.join();
    }
    let held = HELD.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some((file, path)) = held {
        end(file, &path);
    }
}

/// How the start went with the file.
enum Begun {
    /// This run's file, held, and what the log says of the last run if it didn't quit.
    Held(File, Option<String>),
    /// Another copy running on the same folder holds it.
    InUse,
    /// It couldn't be opened or made.
    Failed,
}

/// Opens the file in `folder` (made if it isn't there), takes it if no other copy holds it, says
/// how the run that left it ended, and writes this run's version and time in it.
fn begin_in(folder: &Path, version: &str, now: DateTime<Local>) -> Begun {
    let opened = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(folder.join(NAME));
    let Ok(mut file) = opened else { return Begun::Failed };
    if !take(&file) {
        return Begun::InUse;
    }
    let mut left = Vec::new();
    let _ = (&file).take(4096).read_to_end(&mut left);
    let said = how_it_ended(&String::from_utf8_lossy(&left), now);
    let _ = file.set_len(0);
    let _ = file.seek(SeekFrom::Start(0));
    let _ = write!(file, "SoftPresenter {version}\nstarted {}\n", now.to_rfc3339());
    let _ = file.sync_data();
    Begun::Held(file, said)
}

/// Takes the file for this copy, unless another copy running holds it. On a Mac and Linux it is
/// locked for as long as this copy runs. Not on Windows, where a lock would keep every other
/// program from reading the file, and so copying the data folder while the app is open would fail
/// on it: two copies on one folder there aren't told apart. A disk that can't lock files is taken
/// all the same.
fn take(file: &File) -> bool {
    #[cfg(unix)]
    if let Err(std::fs::TryLockError::WouldBlock) = file.try_lock() {
        return false;
    }
    #[cfg(not(unix))]
    let _ = file;
    true
}

/// Adds the note of a panic to the file.
fn note_crash(file: &mut File) {
    let _ = file.seek(SeekFrom::End(0));
    let _ = file.write_all(b"crashed\n");
}

/// Removes this run's file. Emptied first, so a copy that opens it at that moment finds nothing
/// left behind.
fn end(file: File, path: &Path) {
    let _ = file.set_len(0);
    let removed = std::fs::remove_file(path).is_ok();
    drop(file);
    if !removed {
        let _ = std::fs::remove_file(path);
    }
}

/// What the log says of a run that left its file behind (what the file holds), or `None` when
/// it is empty (nothing was left).
fn how_it_ended(left: &str, now: DateTime<Local>) -> Option<String> {
    if left.trim().is_empty() {
        return None;
    }
    let (mut version, mut started, mut crashed) = (None, None, false);
    for line in left.lines() {
        if let Some(said) = line.strip_prefix("SoftPresenter ") {
            version = Some(said.trim().chars().take(40).collect::<String>());
        } else if let Some(at) = line.strip_prefix("started ") {
            started = DateTime::parse_from_rfc3339(at.trim()).ok().map(|at| at.with_timezone(&Local));
        } else if line.trim() == "crashed" {
            crashed = true;
        }
    }
    let when = started.map(|at| {
        let format = if at.date_naive() == now.date_naive() { "%H:%M" } else { "%Y-%m-%d %H:%M" };
        format!("started {}", at.format(format))
    });
    let about = match (version, when) {
        (Some(version), Some(when)) => format!(" ({version}, {when})"),
        (Some(said), None) | (None, Some(said)) => format!(" ({said})"),
        (None, None) => String::new(),
    };
    let how = if crashed {
        "it crashed".to_owned()
    } else {
        let outside = if cfg!(windows) {
            "Task Manager"
        } else if cfg!(target_os = "macos") {
            "Force Quit"
        } else {
            "a kill"
        };
        format!("it crashed, or was closed from outside ({outside}, a shutdown)")
    };
    Some(format!("the last run{about} ended without quitting: {how}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-last-run-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn at(time: &str) -> DateTime<Local> {
        DateTime::parse_from_rfc3339(time).unwrap().with_timezone(&Local)
    }

    /// The start: the file held, and what the log said of the last run.
    fn start(folder: &Path, time: &str) -> (File, Option<String>) {
        match begin_in(folder, "0.9.0-dev.7", at(time)) {
            Begun::Held(file, said) => (file, said),
            Begun::InUse => panic!("the file was in use"),
            Begun::Failed => panic!("the file couldn't be made"),
        }
    }

    /// What the log says of the run started at that time and left without quitting.
    fn said_of(time: &str, how: &str) -> String {
        format!("the last run (0.9.0-dev.7, started {}) ended without quitting: {how}", at(time).format("%H:%M"))
    }

    const OUTSIDE: &str = if cfg!(windows) {
        "it crashed, or was closed from outside (Task Manager, a shutdown)"
    } else if cfg!(target_os = "macos") {
        "it crashed, or was closed from outside (Force Quit, a shutdown)"
    } else {
        "it crashed, or was closed from outside (a kill, a shutdown)"
    };

    #[test]
    fn a_run_left_behind_is_said_once_and_a_quit_leaves_nothing() {
        let folder = folder("left");
        let marker = folder.join(NAME);
        let (file, said) = start(&folder, "2026-11-26T10:15:00+01:00");
        assert_eq!(said, None, "the first run: nothing before it");
        assert!(std::fs::read_to_string(&marker).unwrap().starts_with("SoftPresenter 0.9.0-dev.7\nstarted 2026-11-"));
        end(file, &marker);
        assert!(!marker.exists(), "quitting leaves no file");
        let (file, said) = start(&folder, "2026-11-26T11:00:00+01:00");
        assert_eq!(said, None, "the run before quit");
        // Closed from outside: the file is let go without being removed, as when the app is killed.
        drop(file);
        let (file, said) = start(&folder, "2026-11-26T11:30:00+01:00");
        assert_eq!(said.as_deref(), Some(said_of("2026-11-26T11:00:00+01:00", OUTSIDE).as_str()));
        end(file, &marker);
        let (file, said) = start(&folder, "2026-11-26T12:00:00+01:00");
        assert_eq!(said, None, "said once: the run that said it quit");
        end(file, &marker);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_run_that_panicked_crashed_and_one_from_another_day_says_its_date() {
        let folder = folder("crashed");
        let (mut file, _) = start(&folder, "2026-11-26T10:15:00+01:00");
        note_crash(&mut file);
        drop(file);
        let (file, said) = start(&folder, "2026-11-26T10:20:00+01:00");
        assert_eq!(said.as_deref(), Some(said_of("2026-11-26T10:15:00+01:00", "it crashed").as_str()));
        drop(file);
        let started = at("2026-11-26T10:20:00+01:00");
        let next_day = started + chrono::Duration::days(1);
        let said = match begin_in(&folder, "0.9.0-dev.8", next_day) {
            Begun::Held(_, said) => said.unwrap(),
            _ => panic!("the file was taken"),
        };
        assert_eq!(
            said,
            format!(
                "the last run (0.9.0-dev.7, started {}) ended without quitting: {OUTSIDE}",
                started.format("%Y-%m-%d %H:%M")
            )
        );
        // A file that holds something else still says the run didn't quit; an empty one says nothing.
        assert_eq!(
            how_it_ended("garbled\n", next_day).as_deref(),
            Some(format!("the last run ended without quitting: {OUTSIDE}").as_str())
        );
        assert_eq!(how_it_ended("", next_day), None);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[cfg(unix)]
    #[test]
    fn a_second_copy_on_the_same_folder_leaves_the_first_ones_file_alone() {
        let folder = folder("two");
        let other = self::folder("two-other");
        let (first, _) = start(&folder, "2026-11-26T10:15:00+01:00");
        assert!(matches!(begin_in(&folder, "0.9.0-dev.99", Local::now()), Begun::InUse), "the first copy holds it");
        // Another data folder has a file of its own.
        let (elsewhere, said) = start(&other, "2026-11-26T10:16:00+01:00");
        assert_eq!(said, None);
        let held = std::fs::read_to_string(folder.join(NAME)).unwrap_or_default();
        assert!(!held.contains("dev.99"), "unchanged by the second copy: {held}");
        end(first, &folder.join(NAME));
        assert!(!folder.join(NAME).exists());
        end(elsewhere, &other.join(NAME));
        assert!(!other.join(NAME).exists());
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&other);
    }
}
