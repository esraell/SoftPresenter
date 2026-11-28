//! Whether the NDI in the app's own folder is still the one the app put there, asked each time
//! before it counts as installed or is opened.
//!
//! That folder is in the user's data, often in Documents, which other programs can write to and
//! which a syncing service may bring in from another computer. A library there is code the app
//! is about to run, so it is never taken on trust:
//!
//! - **Mac:** macOS itself is asked (`codesign --verify --strict`) whether the library's code
//!   signature is whole, and the signature must be NDI's team's, as when it was installed.
//! - **Linux:** NDI's library for Linux isn't signed, so the app notes its SHA-256 when it
//!   installs it, and the file must still have it. (An install from before the hash was noted
//!   has it noted the first time it is looked at.)
//!
//! A library that doesn't pass is as if it weren't there: NDI "isn't installed", the log says
//! why, and Install NDI puts a good one in its place. The answer is kept only for as long as
//! the file is the very same one, unchanged, as the system itself tells it (its inode and
//! change time, which a program can't set, with its size): so the file is read through once,
//! on a thread of its own or on NDI's, never on the window's, and again the moment anything
//! replaces it or writes to it, whatever size and date the new one is given.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use super::{INFO_FILE, Installed};

/// What tells one file from another under the same name, by what a program that writes
/// files can't choose: which file it is on its disk and when the system last saw it change
/// (its inode and change time), with its size. A file put in its place, or written again, has
/// another, whatever size and date it is given. (Where the system has no such thing, its size
/// and date.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seen {
    size: u64,
    file: (u64, u64),
    changed: (i64, i64),
    written: Option<SystemTime>,
}

fn seen(about: &std::fs::Metadata) -> Seen {
    #[cfg(unix)]
    let (file, changed) = {
        use std::os::unix::fs::MetadataExt;
        ((about.dev(), about.ino()), (about.ctime(), about.ctime_nsec()))
    };
    #[cfg(not(unix))]
    let (file, changed) = ((0, 0), (0, 0));
    Seen { size: about.len(), file, changed, written: about.modified().ok() }
}

/// What has been found out, and what is being found out now.
#[derive(Default)]
struct Known {
    answers: HashMap<PathBuf, (Seen, bool)>,
    asking: HashSet<PathBuf>,
}

fn known() -> &'static Mutex<Known> {
    static KNOWN: OnceLock<Mutex<Known>> = OnceLock::new();
    KNOWN.get_or_init(Mutex::default)
}

/// A file's SHA-256 as hexadecimal.
pub fn file_mark(file: &Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(file)?, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Whether `runtime` is the file noted beside it when the app installed it: its SHA-256 is the
/// one in the note. A note from before the hash was kept gets it now.
pub fn matches_note(runtime: &Path) -> Result<(), String> {
    let folder = runtime.parent().ok_or("it is in no folder")?;
    let note = folder.join(INFO_FILE);
    let bytes = std::fs::read(&note).map_err(|_| "there is no note of what SoftPresenter installed".to_owned())?;
    let mut installed: Installed = serde_json::from_slice(&bytes)
        .map_err(|_| "the note of what SoftPresenter installed can't be read".to_owned())?;
    let mark = file_mark(runtime).map_err(|e| format!("it couldn't be read: {e}"))?;
    if installed.runtime.is_empty() {
        installed.runtime = mark;
        let noted = serde_json::to_vec_pretty(&installed).map_err(|e| e.to_string())?;
        super::sdk::write_whole(folder, INFO_FILE, &noted).map_err(|problem| problem.to_string())?;
        crate::applog::event("ndi", "noted the hash of the NDI SoftPresenter installed, to check it by from now on");
        return Ok(());
    }
    if installed.runtime == mark { Ok(()) } else { Err("it is not the file SoftPresenter installed".to_owned()) }
}

/// Reads the library through and says whether it passes.
fn verify(runtime: &Path) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        super::mac::signed_by_ndi(&super::mac::Tools::system().codesign, runtime)
    } else {
        matches_note(runtime)
    }
}

/// The check sp-ndi asks before a runtime in the app's own folder counts or is opened: yes, no,
/// or, when it may not wait and nothing is known of this file yet, not known (a thread of its
/// own then finds out).
pub fn check(runtime: &Path, may_wait: bool) -> Option<bool> {
    check_with(runtime, may_wait, verify)
}

fn check_with(runtime: &Path, may_wait: bool, verify: fn(&Path) -> Result<(), String>) -> Option<bool> {
    let about = std::fs::metadata(runtime).ok()?;
    let now = seen(&about);
    let path = runtime.to_path_buf();
    {
        let mut known = known().lock().ok()?;
        if let Some((then, passed)) = known.answers.get(&path)
            && *then == now
        {
            return Some(*passed);
        }
        if !may_wait && !known.asking.insert(path.clone()) {
            return None;
        }
    }
    let find_out = move || {
        let found = verify(&path);
        if let Err(why) = &found {
            crate::applog::problem("ndi", &format!("the NDI in SoftPresenter's own folder is not used: {why}"));
        }
        if let Ok(mut known) = known().lock() {
            known.asking.remove(&path);
            known.answers.insert(path, (now, found.is_ok()));
        }
        found.is_ok()
    };
    if may_wait {
        return Some(find_out());
    }
    let _ = std::thread::Builder::new().name("ndi trust".to_owned()).spawn(find_out);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-trust-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn note(folder: &Path, runtime: &str) {
        let installed = Installed { version: "6.3.2".to_owned(), runtime: runtime.to_owned(), ..Installed::default() };
        std::fs::write(folder.join(INFO_FILE), serde_json::to_vec(&installed).unwrap()).unwrap();
    }

    #[test]
    fn only_the_file_that_was_installed_matches_its_note() {
        let folder = folder("note");
        let runtime = folder.join("libndi.so.6");
        std::fs::write(&runtime, b"the library as it was installed").unwrap();
        note(&folder, &file_mark(&runtime).unwrap());
        assert_eq!(matches_note(&runtime), Ok(()));
        // Another file under its name, whoever put it there, is refused.
        std::fs::write(&runtime, b"something else under its name").unwrap();
        assert_eq!(matches_note(&runtime), Err("it is not the file SoftPresenter installed".to_owned()));
        // So is a library with no note at all.
        std::fs::remove_file(folder.join(INFO_FILE)).unwrap();
        assert!(matches_note(&runtime).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_install_from_before_the_hash_was_kept_has_it_noted_once() {
        let folder = folder("old");
        let runtime = folder.join("libndi.so.6");
        std::fs::write(&runtime, b"installed by an earlier version").unwrap();
        note(&folder, "");
        assert_eq!(matches_note(&runtime), Ok(()));
        let noted: Installed = serde_json::from_slice(&std::fs::read(folder.join(INFO_FILE)).unwrap()).unwrap();
        assert_eq!(noted.runtime, file_mark(&runtime).unwrap());
        assert_eq!(noted.version, "6.3.2", "the rest of the note is kept");
        // From then on it is held to that.
        std::fs::write(&runtime, b"changed since").unwrap();
        assert!(matches_note(&runtime).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_answer_is_kept_while_the_file_stays_and_found_again_when_it_changes() {
        static ASKED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn by_size(file: &Path) -> Result<(), String> {
            ASKED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if std::fs::metadata(file).map(|about| about.len()).unwrap_or(0) == 4 {
                Ok(())
            } else {
                Err("not four".into())
            }
        }
        let folder = folder("kept");
        let runtime = folder.join("libndi");
        std::fs::write(&runtime, b"good").unwrap();
        let asked = || ASKED.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(check_with(&runtime, true, by_size), Some(true));
        assert_eq!(check_with(&runtime, true, by_size), Some(true));
        assert_eq!(check_with(&runtime, false, by_size), Some(true), "known already: answered at once");
        assert_eq!(asked(), 1, "read through once");
        // Changed: found out again, and refused.
        std::fs::write(&runtime, b"not good").unwrap();
        assert_eq!(check_with(&runtime, true, by_size), Some(false));
        assert_eq!(asked(), 2);
        // A file that isn't there has no answer.
        assert_eq!(check_with(&folder.join("missing"), true, by_size), None);
        // Asked without waiting about a file nothing is known of: not known yet, then known.
        let other = folder.join("other");
        std::fs::write(&other, b"good").unwrap();
        assert_eq!(check_with(&other, false, by_size), None);
        for _ in 0..200 {
            if check_with(&other, false, by_size) == Some(true) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(check_with(&other, false, by_size), Some(true));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_swapped_for_one_of_the_same_size_and_date_is_checked_again() {
        fn is_good(file: &Path) -> Result<(), String> {
            if std::fs::read(file).is_ok_and(|bytes| bytes == b"good") {
                Ok(())
            } else {
                Err("not the good one".into())
            }
        }
        let folder = folder("swapped");
        let runtime = folder.join("libndi");
        std::fs::write(&runtime, b"good").unwrap();
        let date = std::fs::metadata(&runtime).unwrap().modified().unwrap();
        assert_eq!(check_with(&runtime, true, is_good), Some(true));
        // Another file of the same size, given the same date, put in its place.
        let other = folder.join("other");
        std::fs::write(&other, b"evil").unwrap();
        std::fs::File::options().write(true).open(&other).unwrap().set_modified(date).unwrap();
        std::fs::rename(&other, &runtime).unwrap();
        let about = std::fs::metadata(&runtime).unwrap();
        assert_eq!((about.len(), about.modified().unwrap()), (4, date), "the same size and date");
        assert_eq!(check_with(&runtime, true, is_good), Some(false), "it is checked again, and refused");
        // The same file written again in place, with its date put back: checked again too.
        std::fs::write(&runtime, b"good").unwrap();
        std::fs::File::options().write(true).open(&runtime).unwrap().set_modified(date).unwrap();
        assert_eq!(check_with(&runtime, true, is_good), Some(true));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&runtime, b"evil").unwrap();
        std::fs::File::options().write(true).open(&runtime).unwrap().set_modified(date).unwrap();
        assert_eq!(check_with(&runtime, true, is_good), Some(false));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
