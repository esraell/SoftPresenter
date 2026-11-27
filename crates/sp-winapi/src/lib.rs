//! The few Windows calls SoftPresenter makes itself. On other systems they do nothing (and the
//! ones about who may open a file exist only on Windows).

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

/// The app is a window program on Windows (no console opens beside it), so a command typed in
/// a console (`SoftPresenter.exe song list`) has nowhere to print. This joins the console it
/// was typed in, for whichever of the standard input, output and error is not already a pipe
/// or a file: output sent to a file or through a pipe (PowerShell, ssh) is left alone and
/// nothing is attached. Without a parent console (started from Explorer) nothing happens.
pub fn attach_parent_console() {
    #[cfg(windows)]
    windows::attach_parent_console();
}

/// Renames the file `from` to `to`, over a file there, and on Windows returns only once the
/// rename is written to the disk (MoveFileExW, written through), so a power cut just after leaves
/// the new file in place: Windows can't flush a folder as a Mac and Linux do. Elsewhere, a plain
/// rename.
pub fn move_written_through(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        windows::move_written_through(from, to)
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
}

/// The part of the monitor under this point that the taskbar leaves free, as `(x, y, width,
/// height)` in pixels, or `None` where it can't be asked.
pub fn work_area_at(x: i32, y: i32) -> Option<(i32, i32, u32, u32)> {
    #[cfg(windows)]
    {
        windows::work_area_at(x, y)
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        None
    }
}

/// Puts `folder` on the signed-in user's own PATH (their saved settings; no administrator
/// needed) unless it is there already, and tells running programs the settings changed, so a
/// terminal opened afterwards finds what is in the folder. `Ok(true)` when it was added,
/// `Ok(false)` when it was there. On other systems it does nothing.
pub fn add_to_user_path(folder: &str) -> Result<bool, String> {
    #[cfg(windows)]
    {
        windows::add_to_user_path(folder)
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Ok(false)
    }
}

/// Takes `folder` off the signed-in user's own PATH, if it is there, and tells running programs
/// the settings changed. `Ok(true)` when it was taken off, `Ok(false)` when it wasn't there. On
/// other systems it does nothing.
pub fn remove_from_user_path(folder: &str) -> Result<bool, String> {
    #[cfg(windows)]
    {
        windows::remove_from_user_path(folder)
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Ok(false)
    }
}

/// Whether `folder` is on the signed-in user's own PATH (false where it can't be read, and on
/// other systems).
pub fn on_user_path(folder: &str) -> bool {
    #[cfg(windows)]
    {
        windows::on_user_path(folder)
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        false
    }
}

/// `path` (a PATH as Windows saves it: folders between `;`) with `folder` added at its end, or
/// `None` when it holds that folder already (with or without a last `\`, in any case).
pub fn path_with(path: &str, folder: &str) -> Option<String> {
    let bare = |entry: &str| entry.trim().trim_end_matches('\\').to_lowercase();
    let wanted = bare(folder);
    if path.split(';').any(|entry| bare(entry) == wanted) {
        return None;
    }
    let mut longer = path.trim_end_matches(';').to_owned();
    if !longer.is_empty() {
        longer.push(';');
    }
    longer.push_str(folder);
    Some(longer)
}

/// `path` without `folder` (written with or without a last `\`, in any case), or `None` when
/// it doesn't hold that folder.
pub fn path_without(path: &str, folder: &str) -> Option<String> {
    let bare = |entry: &str| entry.trim().trim_end_matches('\\').to_lowercase();
    let wanted = bare(folder);
    let entries: Vec<&str> = path.split(';').collect();
    let kept: Vec<&str> = entries.iter().copied().filter(|entry| bare(entry) != wanted).collect();
    (kept.len() < entries.len()).then(|| kept.join(";"))
}

/// The names the connected monitors give themselves ("Mi TV", "Philips FTV"), as Windows'
/// display settings show them, each with the device name of the display it shows
/// (`\\.\DISPLAY2`, the name the window library gives that display). A monitor that gives no
/// name is left out. A display shown on two monitors at once (duplicated) is listed once for
/// each, in the order Windows gives them. Empty where it can't be asked, and on other systems.
pub fn monitor_names() -> Vec<(String, String)> {
    #[cfg(windows)]
    {
        windows::monitor_names()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// The user's location as Windows names it (Settings › Time & language › Region › Country or
/// region): a country's two-letter code such as `DE`, `XX` when none is set, or a number for a
/// region that is no country. `None` where it can't be asked, and on other systems.
pub fn user_geo_name() -> Option<String> {
    #[cfg(windows)]
    {
        windows::user_geo_name()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The number Windows gives the thread this is called on, and, given that number for a thread
/// that shows a window of the app's own (a file window, shown on a thread of its own so the
/// app's window goes on meanwhile), puts that window back in front: restored if it was
/// minimised, and made the one in use when Windows allows it (it does when the app is the one
/// in use, as when the operator has just clicked in it); false when the thread shows none.
#[cfg(windows)]
pub use windows::{bring_thread_window_forward, current_thread_id};

/// OLE (what dragging and dropping and the clipboard use) set up for the calling thread while
/// the value is held, and taken down when it is dropped, on that thread. Windows asks for it on
/// any thread that drags something: a file window shown on a thread of its own is one, as a file
/// can be dragged out of it onto the app's window. Made where OLE is set up already, it is set
/// up once more and taken down once (Windows counts them).
#[cfg(windows)]
pub use windows::{OleOnThisThread, ole_on_this_thread};

/// Puts the app's window `window` (its handle as the window library gives it) in front and gives
/// it the keyboard, but only when the window in front now is another of this program's own, or
/// none: when a file window of the app's closes, Windows puts in front the next window in line,
/// which may be one of the app's screens (on the projector), and keys pressed then would go to
/// it. A window of another program in front is left alone. True when it was put in front.
#[cfg(windows)]
pub use windows::front_back_to;

/// A name this program holds in the signed-in user's session (a named mutex, `Local\...`), for as
/// long as the value is kept: Windows lets it go when the program ends, however it ends (a crash,
/// Task Manager). `hold_name` gives `Ok(None)` when another program holds that name already,
/// one this program may not open included (a copy started as administrator). What keeps the app
/// to one copy per data folder.
#[cfg(windows)]
pub use windows::{HeldName, hold_name};

/// Lets the program that asks next bring its window to the front, as this one may: Windows lets
/// only the program in front, or one the person has just started, do that. A start of the app
/// turned away by the copy running calls it before asking that copy to come to the front. It
/// does nothing on other systems.
pub fn let_others_come_forward() {
    #[cfg(windows)]
    windows::let_others_come_forward();
}

/// The languages Windows shows its own words in for the signed-in user, its first one first, as
/// tags (`es-ES`): asked at once, as the app's start can't wait for PowerShell. Empty where they
/// can't be asked, and on other systems.
pub fn ui_languages() -> Vec<String> {
    #[cfg(windows)]
    {
        windows::ui_languages()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Who owns a file or folder and who may open it, read and set with Windows' own security
/// calls, and the user this program runs as: what keeps the control socket's folder to one
/// user. Access lists go in and out written down as Windows writes them (SDDL); what they mean
/// is for the caller to decide.
#[cfg(windows)]
pub use windows::security::{HeldFolder, Security, current_user, make_folder, security_of};

/// The signed-in user's entries in Windows' Credential Manager, where the app keeps keys and
/// tokens: bytes under a name, encrypted by Windows with the user's sign-in, for this user on
/// this computer. One entry holds at most `ROOM` bytes.
#[cfg(windows)]
pub mod credentials {
    pub use crate::windows::credentials::{ROOM, read, remove, write};
}

/// What this program, the programs it started and the whole computer have used, as Windows
/// counts it: processor time in seconds since each began (take two and divide by the time
/// between), private memory in bytes, and the memory free now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProcessStats {
    pub app_cpu_s: f64,
    pub app_private_bytes: u64,
    /// The programs this one started (FFmpeg), and theirs in turn, added up.
    pub kids_cpu_s: f64,
    pub kids_private_bytes: u64,
    /// Time every processor has spent on anything but idling, added up, in seconds.
    pub system_busy_s: Option<f64>,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

/// The figures above, or `None` where they can't be asked (every system but Windows).
pub fn process_stats() -> Option<ProcessStats> {
    #[cfg(windows)]
    {
        windows::process_stats()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{path_with, path_without};

    #[test]
    fn a_folder_goes_on_the_end_of_the_path_once() {
        let bin = r"C:\Users\a\AppData\Local\SoftPresenter\bin";
        assert_eq!(path_with("", bin).as_deref(), Some(bin));
        assert_eq!(
            path_with(r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;", bin).as_deref(),
            Some(r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Users\a\AppData\Local\SoftPresenter\bin")
        );
        // Already there, however it was written.
        assert_eq!(path_with(r"C:\Tools;c:\users\A\AppData\Local\SoftPresenter\BIN\", bin), None);
        assert_eq!(path_with(&format!("{bin};C:\\Tools"), bin), None);
    }

    #[test]
    fn a_folder_comes_off_the_path_however_it_was_written() {
        let bin = r"C:\Users\a\AppData\Local\SoftPresenter\bin";
        assert_eq!(path_without(bin, bin).as_deref(), Some(""));
        assert_eq!(
            path_without(r"C:\Tools;c:\users\A\AppData\Local\SoftPresenter\BIN\;%USERPROFILE%\x", bin).as_deref(),
            Some(r"C:\Tools;%USERPROFILE%\x")
        );
        // What went on comes off, and the rest is as it was.
        let before = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps";
        assert_eq!(path_without(&path_with(before, bin).expect("added"), bin).as_deref(), Some(before));
        assert_eq!(path_without(r"C:\Tools", bin), None);
    }

    /// The rename a save takes the place of a file by: over a file there, onto a name no file has,
    /// and refused, with the system's reason, when there is nothing to move or a name holds a NUL.
    #[test]
    fn a_file_is_moved_over_the_one_there_written_through() {
        let dir = std::env::temp_dir().join(format!("sp-winapi-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (part, file) = (dir.join("preferences.json.part"), dir.join("preferences.json"));
        std::fs::write(&file, "before").unwrap();
        std::fs::write(&part, "after").unwrap();
        super::move_written_through(&part, &file).expect("moved over the file there");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "after");
        assert!(!part.exists(), "moved, not copied");
        let new = dir.join("timers.json");
        std::fs::write(&part, "[]").unwrap();
        super::move_written_through(&part, &new).expect("moved to a new name");
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "[]");
        let gone = super::move_written_through(&part, &file).expect_err("nothing to move");
        assert_eq!(gone.kind(), std::io::ErrorKind::NotFound, "{gone}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "after", "left as it was");
        #[cfg(windows)]
        {
            let nul = super::move_written_through(&new, &dir.join("a\0b")).expect_err("a NUL in a name");
            assert_eq!(nul.kind(), std::io::ErrorKind::InvalidInput);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_name_is_held_by_one_holder_at_a_time_and_is_free_again_once_let_go() {
        let name = format!(r"Local\SoftPresenter-test-{}", std::process::id());
        let first = super::hold_name(&name).expect("asked").expect("held");
        assert!(super::hold_name(&name).expect("asked").is_none(), "held already");
        drop(first);
        assert!(super::hold_name(&name).expect("asked").is_some(), "free again");
    }
}
