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
}
