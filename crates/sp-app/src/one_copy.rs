//! One copy of SoftPresenter per data folder. A start that finds another copy already running on
//! the same folder opens nothing (no window, screen, NDI sender or control socket) and touches
//! nothing in the library: it asks the copy running to come to the front, through the control
//! socket (`sp front --start`), and ends. Clicking the icon a few times while an update installed
//! once started three copies on one library within 0.2 s, each with its own audience window and
//! NDI sender, any of which could save over the others' changes.
//!
//! The check is a hold the operating system keeps for the program, taken before anything heavy
//! in `main`, so it holds however close together the starts come (the control socket comes up a
//! moment after a start, so starts that only asked it would all miss each other), and the system
//! lets it go when the program ends, however it ends (a crash, Task Manager, Force Quit):
//!
//! - On Windows, a named mutex in the signed-in session's own namespace
//!   (`Local\SoftPresenter-…`, named from the data folder's path), held through `sp-winapi`. No
//!   file: a file locked on Windows can't be read by other programs (`applog/last_run.rs`).
//! - On a Mac and Linux, an exclusive lock on the file `one-copy.lock` beside the log and its
//!   `running` note: in the data folder of a copy that has one of its own (`SOFTPRESENTER_DATA`);
//!   on a Mac otherwise in `~/Library/Logs/SoftPresenter`, this computer's own place, which
//!   stays put when the library moves to Documents and is never synced; on Linux in the data
//!   folder. The file is never removed: a copy that removed it as it quit could let the next two
//!   starts each lock a file of that name.
//!
//! Copies with different data folders still run side by side (the tests' quiet copies each have
//! their own). Anything that keeps the hold from being asked (no folder, a disk that can't lock
//! files) lets the app start as before: only another copy holding it turns a start away.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// How long a start turned away goes on asking the copy running to come to the front: that copy
/// may still be starting, its control socket not up yet.
const ASK_WITHIN: Duration = Duration::from_secs(10);
/// The wait between two tries.
const PAUSE: Duration = Duration::from_millis(200);
/// The file locked on a Mac and Linux.
#[cfg(unix)]
const LOCK_FILE: &str = "one-copy.lock";
/// What a start turned away says first, on its standard error (in English, as the command line
/// speaks).
const ALREADY_OPEN: &str = "SoftPresenter is already open with this data folder";

/// This copy's hold on its data folder, kept for as long as the program runs.
pub struct Held {
    #[cfg(unix)]
    _file: std::fs::File,
    #[cfg(windows)]
    _name: sp_winapi::HeldName,
}

/// How asking for the hold went.
pub enum Hold {
    /// This copy holds it, for as long as the value is kept.
    Held(Held),
    /// Another copy running on this folder holds it.
    Taken,
    /// It couldn't be asked, and why: the app starts as before.
    Unknown(String),
}

/// What `main` does about other copies as it starts.
pub enum Start {
    /// Goes on starting, holding the folder (none when it couldn't be asked, with why, for the
    /// log once it is open).
    Go(Option<Held>, Option<String>),
    /// Ends here, with this exit code: another copy has the folder.
    End(ExitCode),
}

/// Takes the hold on this copy's data folder, or, when another copy running has it, asks that one
/// to come to the front and says to end. One that never answers is said on the standard error,
/// in its log, and on Windows (where a start from the icon has no console) in a message. A run
/// that only saves a picture (`--capture-home`) asks nothing of the copy running and ends with a
/// failure, as it saved no picture. `files`: files were named to open, which a start turned away
/// doesn't open.
pub fn at_start(capturing: bool, files: bool) -> Start {
    let folder = crate::applog::folder();
    let take = || match folder.as_deref() {
        Some(folder) => take_for(folder),
        None => Hold::Unknown("there is no folder to hold".to_owned()),
    };
    let unsure =
        |why: String| format!("could not make sure no other copy runs with this folder ({why}); started anyway");
    if capturing {
        return match take() {
            Hold::Held(held) => Start::Go(Some(held), None),
            Hold::Unknown(why) => Start::Go(None, Some(unsure(why))),
            Hold::Taken => {
                eprintln!(
                    "{ALREADY_OPEN}, so no picture was taken. Give a capture a data folder of its own (SOFTPRESENTER_DATA)."
                );
                Start::End(ExitCode::from(1))
            }
        };
    }
    let ask = |within| {
        // Windows lets a program bring its window to the front only when the program in front
        // (or one the person has just started, as this one) lets it.
        sp_winapi::let_others_come_forward();
        crate::control::ask_front(within)
    };
    let not_opened = if files {
        " The files named weren't opened: add them in its window, or with sp song import or sp media add."
    } else {
        ""
    };
    match settle(take, ask, ASK_WITHIN, PAUSE) {
        Settled::Go(held, why) => Start::Go(held, why.map(unsure)),
        Settled::Handed => {
            eprintln!("{ALREADY_OPEN}; it was asked to bring its window to the front.{not_opened}");
            Start::End(ExitCode::SUCCESS)
        }
        Settled::NotAnswered => {
            eprintln!(
                "{ALREADY_OPEN}. It didn't answer when asked to come to the front; no second copy was opened.{not_opened}"
            );
            // Said in that copy's log too, which is all a person's report may hold of it.
            crate::applog::add_from_outside(
                "start",
                &format!(
                    "a start of SoftPresenter turned away by this copy got no answer when it asked this window to come to the front for {} s; it opened nothing",
                    ASK_WITHIN.as_secs()
                ),
            );
            let quiet = std::env::var_os("SOFTPRESENTER_QUIET").is_some();
            learn_language();
            if let Some(words) = message_box_words(&Settled::NotAnswered, std::env::consts::OS, quiet) {
                show_message(words);
            }
            Start::End(ExitCode::SUCCESS)
        }
    }
}

/// The app's language, as well as a start that opens nothing can tell it: the Language setting
/// saved, else the computer's (Windows' own display languages, asked at once; elsewhere none,
/// which is English, as nothing here is shown elsewhere).
fn learn_language() {
    // Only looked at: the copy running has the folder, and a file it can't read is its to keep.
    crate::i18n::set_choice(crate::i18n::Choice::from_code(&crate::service::peek_prefs().language));
    // Testing aid: SOFTPRESENTER_LANGUAGE stands in for the computer's language, as for the app
    // (`system_language`).
    match std::env::var("SOFTPRESENTER_LANGUAGE") {
        Ok(tags) => crate::i18n::set_computer(&tags.split(',').map(str::trim).collect::<Vec<_>>()),
        Err(_) => crate::i18n::set_computer(&sp_winapi::ui_languages()),
    }
}

/// Shows the words in a message of the system's own (the app's file windows' way), and waits for
/// it to be closed: what a start turned away on Windows has instead of a console.
fn show_message(words: &str) {
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("SoftPresenter")
        .set_description(words)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// How a start that found the folder held settled it.
enum Settled {
    /// Go on: the folder held after all (the copy holding it went meanwhile), or it couldn't be
    /// asked.
    Go(Option<Held>, Option<String>),
    /// The copy holding it answered that its window is in front.
    Handed,
    /// It never answered in time.
    NotAnswered,
}

/// Takes the hold, and while another copy has it asks that one (`ask`, given how long it may
/// wait for the answer) to come to the front, every `pause`, for at most `within`. The time is
/// what bounds it: an answer is waited for only as long as is left.
fn settle(
    mut take: impl FnMut() -> Hold,
    mut ask: impl FnMut(Duration) -> bool,
    within: Duration,
    pause: Duration,
) -> Settled {
    let began = Instant::now();
    loop {
        match take() {
            Hold::Held(held) => return Settled::Go(Some(held), None),
            Hold::Unknown(why) => return Settled::Go(None, Some(why)),
            Hold::Taken => {}
        }
        let left = within.saturating_sub(began.elapsed());
        if left.is_zero() {
            return Settled::NotAnswered;
        }
        if ask(left) {
            return Settled::Handed;
        }
        std::thread::sleep(pause.min(within.saturating_sub(began.elapsed())));
    }
}

/// The words a start that got no answer shows in a message of the system's own, when it shows one:
/// only on Windows, where a start from its icon has no console to say anything in, and never in a
/// quiet run (a test's), where nobody would close it.
fn message_box_words(settled: &Settled, system: &str, quiet: bool) -> Option<&'static str> {
    (matches!(settled, Settled::NotAnswered) && system == "windows" && !quiet).then(|| {
        crate::i18n::t(
            "dialogs.already_running",
            "SoftPresenter is already running, or still closing: wait a moment, and if you still can't see its window, end it in Task Manager.",
        )
    })
}

/// Takes the hold on `folder`, unless another copy running has it: an exclusive lock on its
/// file, made if it isn't there (and the folder with it), for this user alone, so no other user
/// can lock it and keep the app from starting.
#[cfg(unix)]
fn take_for(folder: &Path) -> Hold {
    use std::os::unix::fs::OpenOptionsExt;
    if let Err(e) = std::fs::create_dir_all(folder) {
        return Hold::Unknown(format!("its folder couldn't be made: {e}"));
    }
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(folder.join(LOCK_FILE));
    let file = match opened {
        Ok(file) => file,
        Err(e) => return Hold::Unknown(format!("{LOCK_FILE} couldn't be opened: {e}")),
    };
    match file.try_lock() {
        Ok(()) => Hold::Held(Held { _file: file }),
        Err(std::fs::TryLockError::WouldBlock) => Hold::Taken,
        Err(std::fs::TryLockError::Error(e)) => Hold::Unknown(format!("{LOCK_FILE} couldn't be locked: {e}")),
    }
}

/// Takes the hold on `folder`, unless another copy running has it: the mutex named from its whole
/// path as written, never looked up on the disk (the folder may not be made yet).
#[cfg(windows)]
fn take_for(folder: &Path) -> Hold {
    let whole = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
    match sp_winapi::hold_name(&mutex_name(&whole.to_string_lossy())) {
        Ok(Some(name)) => Hold::Held(Held { _name: name }),
        Ok(None) => Hold::Taken,
        Err(why) => Hold::Unknown(why),
    }
}

#[cfg(not(any(unix, windows)))]
fn take_for(_folder: &Path) -> Hold {
    Hold::Unknown("this system has no way to hold it".to_owned())
}

/// The name of the mutex that holds a data folder on Windows, the same however the folder's path
/// is written (Windows doesn't mind the case of a path, or which slash it uses): the session's
/// own namespace, and the start of the path's SHA-256, as a name may have no `\` of its own.
#[cfg(any(windows, test))]
fn mutex_name(folder: &str) -> String {
    use sha2::{Digest, Sha256};
    let plain = folder.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let digest = Sha256::digest(plain.as_bytes());
    let hex: String = digest.iter().take(16).map(|byte| format!("{byte:02x}")).collect();
    format!("Local\\SoftPresenter-{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-one-copy-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_folder_is_held_by_one_copy_at_a_time_and_is_free_again_once_let_go() {
        let folder = folder("one");
        let first = take_for(&folder);
        assert!(matches!(first, Hold::Held(_)), "the first takes it");
        assert!(matches!(take_for(&folder), Hold::Taken), "a second is turned away while the first holds it");
        assert!(matches!(take_for(&folder), Hold::Taken), "and a third");
        drop(first);
        assert!(matches!(take_for(&folder), Hold::Held(_)), "the next takes it once the first let it go");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn copies_on_different_folders_each_hold_their_own() {
        let (one, two) = (folder("first"), folder("second"));
        let first = take_for(&one);
        let second = take_for(&two);
        assert!(matches!(first, Hold::Held(_)) && matches!(second, Hold::Held(_)), "both are held");
        drop((first, second));
        let _ = std::fs::remove_dir_all(&one);
        let _ = std::fs::remove_dir_all(&two);
    }

    /// The file stays when it is let go: a copy removing it as it quit could let the next two
    /// starts each lock a file of that name.
    #[cfg(unix)]
    #[test]
    fn the_lock_file_is_kept_after_it_is_let_go() {
        let folder = folder("kept");
        let held = take_for(&folder);
        assert!(matches!(held, Hold::Held(_)));
        drop(held);
        assert!(folder.join("one-copy.lock").is_file(), "the file is still there");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A folder that can't be made (a file is where it would go) can't be held: the app starts as
    /// before, and says why.
    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_made_is_not_held_and_says_why() {
        let folder = folder("blocked");
        std::fs::write(folder.join("in-the-way"), b"").unwrap();
        let Hold::Unknown(why) = take_for(&folder.join("in-the-way").join("data")) else {
            panic!("not held, and not taken by another");
        };
        assert!(!why.is_empty());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_start_turned_away_asks_the_copy_running_until_it_answers() {
        let folder = folder("asks");
        let first = take_for(&folder);
        assert!(matches!(first, Hold::Held(_)));
        let mut asked = Vec::new();
        let settled = settle(
            || take_for(&folder),
            |within| {
                asked.push(within);
                asked.len() == 3
            },
            Duration::from_secs(30),
            Duration::from_millis(1),
        );
        assert!(matches!(settled, Settled::Handed), "handed over");
        assert_eq!(asked.len(), 3, "asked until it answered");
        // Each try may wait for the answer for the time left, which a busy copy may need.
        assert!(asked.iter().all(|within| *within > Duration::from_secs(20)), "{asked:?}");
        drop(first);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_start_turned_away_gives_up_once_its_time_is_up_and_holds_nothing() {
        let folder = folder("gives-up");
        let first = take_for(&folder);
        assert!(matches!(first, Hold::Held(_)));
        let began = std::time::Instant::now();
        let mut asked = 0;
        let settled = settle(
            || take_for(&folder),
            |_| {
                asked += 1;
                false
            },
            Duration::from_millis(300),
            Duration::from_millis(20),
        );
        assert!(matches!(settled, Settled::NotAnswered), "gave up");
        assert!(asked >= 2, "asked more than once: {asked}");
        assert!(began.elapsed() < Duration::from_secs(5), "{:?}", began.elapsed());
        // The first still holds it.
        assert!(matches!(take_for(&folder), Hold::Taken));
        drop(first);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The copy holding it was quitting: once it has gone, the start takes the folder and opens
    /// the app, rather than ending with nothing open.
    #[test]
    fn a_start_takes_the_folder_after_all_when_the_copy_holding_it_goes() {
        let folder = folder("goes");
        let mut first = Some(take_for(&folder));
        assert!(matches!(first, Some(Hold::Held(_))));
        let settled = settle(
            || take_for(&folder),
            |_| {
                first.take();
                false
            },
            Duration::from_secs(30),
            Duration::from_millis(1),
        );
        assert!(matches!(settled, Settled::Go(Some(_), None)), "held after all");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_hold_that_cannot_be_asked_lets_the_app_start_as_before() {
        let mut asked = false;
        let settled = settle(
            || Hold::Unknown("this disk can't lock files".to_owned()),
            |_| {
                asked = true;
                true
            },
            Duration::from_secs(30),
            Duration::from_millis(1),
        );
        assert!(matches!(settled, Settled::Go(None, Some(ref why)) if why.contains("can't lock")), "goes on");
        assert!(!asked, "nothing was asked of any other copy");
    }

    /// A start on Windows that got no answer from the copy running says so in a message (it has no
    /// console), in the person's language; nothing else shows one.
    #[test]
    fn only_a_start_on_windows_that_got_no_answer_shows_a_message() {
        let words = message_box_words(&Settled::NotAnswered, "windows", false).expect("a message on Windows");
        assert!(words.contains("Task Manager"), "{words}");
        let others = [
            (Settled::NotAnswered, "macos", false, "a Mac says it on the standard error"),
            (Settled::NotAnswered, "linux", false, "so does Linux"),
            (Settled::NotAnswered, "windows", true, "a quiet run shows nothing"),
            (Settled::Handed, "windows", false, "the copy running answered"),
            (Settled::Go(None, None), "windows", false, "the app opens"),
        ];
        for (settled, system, quiet, why) in others {
            assert!(message_box_words(&settled, system, quiet).is_none(), "{why}");
        }
        for (language, words) in [
            (crate::i18n::Language::Spanish, "Administrador de tareas"),
            (crate::i18n::Language::Russian, "Диспетчере задач"),
            (crate::i18n::Language::Ukrainian, "Диспетчері завдань"),
            (crate::i18n::Language::German, "Task-Manager"),
        ] {
            let said =
                crate::i18n::with_language(language, || message_box_words(&Settled::NotAnswered, "windows", false));
            assert!(said.is_some_and(|said| said.contains(words)), "{said:?}");
        }
    }

    #[test]
    fn the_mutex_name_is_the_same_however_the_folder_is_written() {
        let name = mutex_name(r"C:\Users\Ann\AppData\Roaming\SoftPresenter");
        assert!(name.starts_with(r"Local\SoftPresenter-"), "{name}");
        // Windows keeps `\` for the namespace alone, and a name is at most MAX_PATH long.
        assert_eq!(name.matches('\\').count(), 1, "{name}");
        assert!(name.len() < 260, "{name}");
        for same in [
            r"c:\users\ann\appdata\roaming\softpresenter",
            r"C:\Users\Ann\AppData\Roaming\SoftPresenter\",
            "C:/Users/Ann/AppData/Roaming/SoftPresenter",
        ] {
            assert_eq!(mutex_name(same), name, "{same}");
        }
        for other in [r"C:\Users\Ann\AppData\Roaming\SoftPresenter2", r"C:\Users\Bob\AppData\Roaming\SoftPresenter"] {
            assert_ne!(mutex_name(other), name, "{other}");
        }
    }
}
