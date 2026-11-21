//! On a Mac the library (songs, themes, pictures, playlists, Bibles, settings: everything the
//! app keeps but its log) is in ~/Documents/SoftPresenter, where its owner can see it, back it
//! up and take it to another computer. It was kept in ~/Library/Application Support before.
//!
//! The first start of a build that keeps it in Documents moves it there:
//! 1. Never into a folder already there that isn't SoftPresenter's library: then the library
//!    stays where it is, and the app says so.
//! 2. The old folder is copied into a folder beside the new place, and the copy is checked:
//!    every file there, the same size.
//! 3. In the copy, paths that pointed into the old folder are pointed into the new one, and songs
//!    named by path or title are named by id (see `song_ids`).
//! 4. Only then is the copy renamed into place, and the old folder renamed
//!    "SoftPresenter (moved to Documents)", which is kept as the backup.
//!
//! A crash at any point leaves the old folder whole and in use. A test run with a data folder of
//! its own (`SOFTPRESENTER_DATA`) is never moved.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A file in the library in Documents that says it is SoftPresenter's, so that a folder of that
/// name the owner made for something else is never taken for it.
pub const MARKER: &str = ".softpresenter-library";
/// A file in the copy while it is being made, so a copy left by a crash is known to be ours.
const COPYING: &str = ".softpresenter-copying";
/// What the old folder is renamed to once the library has moved.
pub const BACKUP_NAME: &str = "SoftPresenter (moved to Documents)";

/// Where the library was kept before, in a home folder.
pub fn old_place(home: &Path) -> PathBuf {
    home.join("Library/Application Support/SoftPresenter")
}

/// Where the library is kept now, in a home folder.
pub fn new_place(home: &Path) -> PathBuf {
    home.join("Documents/SoftPresenter")
}

/// The library folder in use on a Mac: the one in Documents once it is SoftPresenter's, else
/// the old one while it is there (not moved yet, or the move couldn't be made), else Documents.
pub fn folder_in(home: &Path) -> PathBuf {
    let (old, new) = (old_place(home), new_place(home));
    if new.join(MARKER).is_file() || !old.is_dir() { new } else { old }
}

/// What the window says once it opens, about the move made at this start.
static NOTICE: Mutex<Option<String>> = Mutex::new(None);

/// What the window should say about the library's folder, once.
pub fn take_notice() -> Option<String> {
    NOTICE.lock().ok()?.take()
}

/// Moves the library to Documents on a Mac, when it is still in the old place, and marks the
/// library in Documents as SoftPresenter's. Returns what was done, for the log.
pub fn move_to_documents() -> Vec<String> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if !cfg!(target_os = "macos") || var("SOFTPRESENTER_DATA").is_some() {
        return Vec::new();
    }
    let Some(home) = var("HOME") else { return Vec::new() };
    let mut done = Vec::new();
    match move_library(&old_place(&home), &new_place(&home)) {
        Ok(Moved::Nothing) => {}
        Ok(Moved::Done(lines)) => {
            done.extend(lines);
            // Short, to fit where notices show; the log says where the old folder is kept.
            set_notice("Your library moved to Documents \u{203a} SoftPresenter; the old one is kept.".to_owned());
        }
        Ok(Moved::Blocked(why)) | Err(why) => {
            done.push(why.clone());
            set_notice(why);
        }
    }
    // A library made in Documents from the start is marked as SoftPresenter's too.
    let new = new_place(&home);
    if folder_in(&home) == new && !new.join(MARKER).is_file() {
        let marked = std::fs::create_dir_all(&new).and_then(|()| std::fs::write(new.join(MARKER), MARKER_TEXT));
        if let Err(e) = marked {
            done.push(format!("could not mark the library in Documents as SoftPresenter's: {e}"));
        }
    }
    // A library the app may not open (the owner said no when macOS asked about Documents).
    if folder_in(&home) == new
        && let Err(e) = std::fs::read_dir(&new)
        && e.kind() == std::io::ErrorKind::PermissionDenied
    {
        let why = "SoftPresenter isn't allowed into your Documents folder, where its library is. Allow it in System Settings \u{203a} Privacy & Security \u{203a} Files and Folders, then open SoftPresenter again.";
        done.push(why.to_owned());
        set_notice(why.to_owned());
    }
    done
}

const MARKER_TEXT: &str = "This folder is SoftPresenter's library: songs, themes, pictures, playlists and settings.\n";

fn set_notice(text: String) {
    if let Ok(mut notice) = NOTICE.lock() {
        *notice = Some(text);
    }
}

/// What `move_library` did.
#[derive(Debug, PartialEq)]
pub enum Moved {
    /// Nothing to move: no old folder, or the library is in Documents already.
    Nothing,
    /// Moved; what was done, a line each.
    Done(Vec<String>),
    /// Not moved, and why, in words for the owner.
    Blocked(String),
}

/// Moves the library from `old` to `new` as the module's notes say.
pub fn move_library(old: &Path, new: &Path) -> Result<Moved, String> {
    let is_folder = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir());
    if !is_folder(old) {
        return Ok(Moved::Nothing);
    }
    if std::fs::symlink_metadata(new).is_ok() {
        if new.join(MARKER).is_file() {
            return Ok(Moved::Nothing);
        }
        return Ok(Moved::Blocked(format!(
            "There is already a folder called SoftPresenter in Documents that isn't SoftPresenter's library, so the library stays in {}. Rename that folder, then open SoftPresenter again to move the library there.",
            shown(old)
        )));
    }
    let parent = new.parent().ok_or("The library's new place has no folder around it.")?;
    let copy = parent.join("SoftPresenter (moving)");
    // A copy left by a crash is ours to clear; anything else of that name is left alone.
    if std::fs::symlink_metadata(&copy).is_ok() {
        if !copy.join(COPYING).is_file() {
            return Ok(Moved::Blocked(format!(
                "A folder called \u{201c}SoftPresenter (moving)\u{201d} is in the way in Documents, so the library stays in {}.",
                shown(old)
            )));
        }
        std::fs::remove_dir_all(&copy)
            .map_err(|e| format!("Could not clear an unfinished copy of the library: {e}"))?;
    }
    let result = make_copy(old, new, &copy);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&copy);
    }
    let mut done = result?;
    std::fs::rename(&copy, new).map_err(|e| {
        let _ = std::fs::remove_dir_all(&copy);
        format!("Could not put the library's copy in place in Documents ({e}); it stays in {}.", shown(old))
    })?;
    done.push(format!("moved the library from {} to {}", shown(old), shown(new)));
    // The old folder is kept, renamed, as the backup.
    let keep = (1..)
        .map(|n| {
            let more = if n == 1 { String::new() } else { format!(" {n}") };
            old.with_file_name(format!("{}{more}", BACKUP_NAME))
        })
        .find(|candidate| std::fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| old.with_extension("moved"));
    match std::fs::rename(old, &keep) {
        Ok(()) => done.push(format!("kept the old folder as {}", shown(&keep))),
        Err(e) => done.push(format!("could not rename the old folder ({e}); it stays as it was, unused")),
    }
    Ok(Moved::Done(done))
}

/// Copies the library into `copy`, checks the copy, points its paths at `new`, names its songs
/// by id and marks it as SoftPresenter's.
fn make_copy(old: &Path, new: &Path, copy: &Path) -> Result<Vec<String>, String> {
    let could_not =
        |e: std::io::Error| format!("Could not copy the library to Documents ({e}); it stays where it was.");
    std::fs::create_dir_all(copy).map_err(could_not)?;
    std::fs::write(copy.join(COPYING), "").map_err(could_not)?;
    let mut done = Vec::new();
    let files = copy_tree(old, copy, &mut done).map_err(could_not)?;
    // Every file there, the same size.
    for (from, size) in &files {
        let inside = from.strip_prefix(old).map_err(|e| e.to_string())?;
        let there = std::fs::metadata(copy.join(inside)).map(|meta| meta.len()).ok();
        if there != Some(*size) {
            return Err(format!(
                "The copy of the library didn't match ({} is missing or a different size); the library stays where it was.",
                inside.display()
            ));
        }
    }
    done.push(format!("copied {} files", files.len()));
    let changed = crate::renamed::repoint(copy, &old.to_string_lossy(), &new.to_string_lossy());
    if changed > 0 {
        done.push(format!("pointed the paths in {changed} files at the new place"));
    }
    done.extend(crate::song_ids::settle(copy, false));
    std::fs::write(copy.join(MARKER), MARKER_TEXT).map_err(could_not)?;
    std::fs::remove_file(copy.join(COPYING)).map_err(could_not)?;
    Ok(done)
}

/// Copies the folders and files in `from` into `to`: not the control socket's folder, and not
/// links (each said in `done`). Returns each file copied, with its size.
fn copy_tree(from: &Path, to: &Path, done: &mut Vec<String>) -> std::io::Result<Vec<(PathBuf, u64)>> {
    let mut files = Vec::new();
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((dir, into)) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path)?;
            let target = into.join(entry.file_name());
            if meta.is_dir() {
                if dir == from && entry.file_name() == "control" {
                    continue;
                }
                std::fs::create_dir_all(&target)?;
                stack.push((path, target));
            } else if meta.is_file() {
                std::fs::copy(&path, &target)?;
                files.push((path, meta.len()));
            } else if meta.file_type().is_symlink() {
                done.push(format!("left out the link {}", entry.file_name().to_string_lossy()));
            }
        }
    }
    Ok(files)
}

/// A folder as the log and the window show it, with the home folder as ~.
fn shown(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|home| path.strip_prefix(home).ok()) {
        Some(inside) => format!("~/{}", inside.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sp-library-move-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const RIVER: &str = "11111111-1111-4111-8111-111111111111";

    /// A library in the old place, as the app left it: a song, a playlist naming it by path, a
    /// picture, a Bible, and the control socket's folder.
    fn old_library(home: &Path, old: &Path) {
        std::fs::create_dir_all(old.join("Songs")).unwrap();
        std::fs::create_dir_all(old.join("Images")).unwrap();
        std::fs::create_dir_all(old.join("Bibles")).unwrap();
        std::fs::create_dir_all(old.join("control")).unwrap();
        std::fs::write(
            old.join("Songs").join(format!("{RIVER}.sppres")),
            format!(r#"{{"format":"softpresenter-presentation","version":1,"id":"{RIVER}","title":"River Song","groups":[]}}"#),
        )
        .unwrap();
        let services = serde_json::json!({"format": "softpresenter-services", "version": 1, "current": 0,
            "list": [{"name": "Sunday", "items": [{"kind": "song", "key": format!("{}/Songs/{RIVER}", old.display()), "title": "River Song"}]}]});
        std::fs::write(old.join("services.json"), services.to_string()).unwrap();
        let media = serde_json::json!([{"path": format!("{}/Images/a.png", old.display())}, {"path": format!("{}/Videos/b.mp4", home.display())}]);
        std::fs::write(old.join("media.json"), media.to_string()).unwrap();
        std::fs::write(old.join("Images/a.png"), [1u8, 2, 3]).unwrap();
        std::fs::write(old.join("Bibles/kjv.spb"), vec![7u8; 5000]).unwrap();
        std::fs::write(old.join("control/note"), "socket folder").unwrap();
    }

    // The move is from where a Mac kept the library before; its paths are written a Mac's way.
    #[cfg(unix)]
    #[test]
    fn the_library_moves_with_its_paths_and_song_names_and_the_old_folder_is_kept() {
        let home = temp("move");
        let (old, new) = (old_place(&home), new_place(&home));
        old_library(&home, &old);
        std::fs::create_dir_all(home.join("Documents")).unwrap();
        assert_eq!(folder_in(&home), old, "before the move, the old folder is in use");
        let Ok(Moved::Done(done)) = move_library(&old, &new) else { panic!("not moved") };
        assert!(done.iter().any(|line| line.contains("copied 5 files")), "{done:?}");
        assert_eq!(folder_in(&home), new);
        assert!(new.join(MARKER).is_file() && !new.join(COPYING).exists());
        assert!(!new.join("control").exists(), "the socket's folder is not copied");
        assert_eq!(std::fs::read(new.join("Bibles/kjv.spb")).unwrap().len(), 5000);
        let services = std::fs::read_to_string(new.join("services.json")).unwrap();
        assert!(services.contains(&format!("\"key\": \"{RIVER}\"")), "{services}");
        assert!(services.contains("\"version\": 2"), "{services}");
        let media = std::fs::read_to_string(new.join("media.json")).unwrap();
        assert!(media.contains(&format!("{}/Images/a.png", new.display())), "{media}");
        assert!(media.contains(&format!("{}/Videos/b.mp4", home.display())), "paths elsewhere untouched");
        // The old folder is kept whole, renamed, and nothing is left half made.
        let kept = old.with_file_name(BACKUP_NAME);
        assert!(!old.exists() && kept.join("services.json").is_file() && kept.join("control/note").is_file());
        assert!(std::fs::read_to_string(kept.join("services.json")).unwrap().contains("\"version\":1"));
        assert!(!home.join("Documents/SoftPresenter (moving)").exists());
        assert!(!new.join(crate::song_ids::BACKUP_FOLDER).exists(), "the old folder is the backup");
        // A second start: nothing to do.
        assert_eq!(move_library(&old, &new).unwrap(), Moved::Nothing);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_folder_of_that_name_that_isnt_the_library_is_never_moved_into() {
        let home = temp("taken");
        let (old, new) = (old_place(&home), new_place(&home));
        old_library(&home, &old);
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("my notes.txt"), "mine").unwrap();
        let Ok(Moved::Blocked(why)) = move_library(&old, &new) else { panic!("should be blocked") };
        assert!(why.contains("isn't SoftPresenter's library"), "{why}");
        assert_eq!(folder_in(&home), old, "the library stays in use where it is");
        assert!(old.join("services.json").is_file() && !new.join("services.json").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_copy_left_by_a_crash_is_cleared_and_made_again() {
        let home = temp("crash");
        let (old, new) = (old_place(&home), new_place(&home));
        old_library(&home, &old);
        let left = home.join("Documents/SoftPresenter (moving)");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join(COPYING), "").unwrap();
        std::fs::write(left.join("half.json"), "{").unwrap();
        assert_eq!(folder_in(&home), old, "a half-made copy is never used");
        assert!(matches!(move_library(&old, &new), Ok(Moved::Done(_))));
        assert!(!new.join("half.json").exists() && new.join("services.json").is_file());
        // Something else of that name is left alone.
        let home2 = temp("crash-other");
        let (old2, new2) = (old_place(&home2), new_place(&home2));
        old_library(&home2, &old2);
        let other = home2.join("Documents/SoftPresenter (moving)");
        std::fs::create_dir_all(&other).unwrap();
        assert!(matches!(move_library(&old2, &new2), Ok(Moved::Blocked(_))));
        assert!(other.is_dir() && old2.is_dir());
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&home2);
    }

    #[test]
    fn an_old_folder_made_again_later_doesnt_take_the_library_back() {
        let home = temp("again");
        let (old, new) = (old_place(&home), new_place(&home));
        old_library(&home, &old);
        assert!(matches!(move_library(&old, &new), Ok(Moved::Done(_))));
        // An older build started afterwards makes the old folder again.
        std::fs::create_dir_all(old.join("Songs")).unwrap();
        assert_eq!(folder_in(&home), new);
        assert_eq!(move_library(&old, &new).unwrap(), Moved::Nothing);
        // A new computer: no old folder, so Documents from the start.
        let fresh = temp("fresh");
        assert_eq!(folder_in(&fresh), new_place(&fresh));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&fresh);
    }

    #[test]
    fn a_library_still_under_the_projects_old_name_comes_to_documents_too() {
        let home = temp("three");
        let support = home.join("Library/Application Support");
        let oldest = support.join("PresenterCraft");
        old_library(&home, &oldest);
        crate::renamed::move_folder(&oldest, &old_place(&home)).unwrap();
        let (old, new) = (old_place(&home), new_place(&home));
        assert!(matches!(move_library(&old, &new), Ok(Moved::Done(_))));
        let services = std::fs::read_to_string(new.join("services.json")).unwrap();
        assert!(services.contains(&format!("\"key\": \"{RIVER}\"")), "{services}");
        assert!(!oldest.exists() && support.join(BACKUP_NAME).is_dir());
        let _ = std::fs::remove_dir_all(&home);
    }
}
