//! The few Windows calls SoftPresenter makes itself. On other systems they do nothing.

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
