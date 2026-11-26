pub mod credentials;
pub mod security;

mod stats;
pub use stats::process_stats;

use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE,
    HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, LUID, POINT, RECT,
};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    SetStdHandle,
};
use windows_sys::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_EXPAND_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowThreadProcessId, HWND_BROADCAST, IsIconic,
    IsWindowVisible, SMTO_ABORTIFHUNG, SW_RESTORE, SendMessageTimeoutW, SetForegroundWindow, ShowWindow,
    WM_SETTINGCHANGE,
};

fn usable(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn attach_parent_console() {
    let which = [(STD_INPUT_HANDLE, "CONIN$"), (STD_OUTPUT_HANDLE, "CONOUT$"), (STD_ERROR_HANDLE, "CONOUT$")];
    // SAFETY: GetStdHandle only reads the process's own standard handle slots.
    let missing: Vec<_> = which.iter().filter(|(slot, _)| !usable(unsafe { GetStdHandle(*slot) })).collect();
    if missing.is_empty() {
        return;
    }
    // SAFETY: AttachConsole takes a process id (here the constant meaning "my parent") and
    // fails harmlessly when the parent has no console.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }
    for (slot, device) in missing {
        let name = wide(device);
        // SAFETY: `name` is a NUL-terminated wide string that outlives the call; the other
        // arguments are plain values or null, as CreateFileW allows for the console devices.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if usable(handle) {
            // SAFETY: `handle` is a console handle just opened, which stays open for the life
            // of the process (it is the standard handle from here on).
            unsafe { SetStdHandle(*slot, handle) };
        }
    }
}

pub fn work_area_at(x: i32, y: i32) -> Option<(i32, i32, u32, u32)> {
    // SAFETY: MonitorFromPoint takes a point by value; with NEAREST it always answers.
    let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let empty = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, rcMonitor: empty, rcWork: empty, dwFlags: 0 };
    // SAFETY: `info` is a MONITORINFO with its size field set, valid for the call to fill in.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let work = info.rcWork;
    Some((work.left, work.top, (work.right - work.left).max(0) as u32, (work.bottom - work.top).max(0) as u32))
}

/// The user's own PATH as saved (`%…%` left as written), or an empty one when they have none.
fn user_path() -> Result<String, String> {
    let (key, name) = (wide("Environment"), wide("Path"));
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let mut room = 0u32;
    // The value may grow between asking its size and reading it: then ask again.
    for _ in 0..3 {
        let mut data = vec![0u16; (room as usize).div_ceil(2).max(1)];
        let mut bytes = (data.len() * 2) as u32;
        // SAFETY: the key and value names are NUL-terminated and live through the call; `data`
        // has room for `bytes` bytes, which the call is told and writes no more than.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                null_mut(),
                data.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        match status {
            ERROR_SUCCESS => {
                data.truncate(bytes as usize / 2);
                while data.last() == Some(&0) {
                    data.pop();
                }
                return Ok(String::from_utf16_lossy(&data));
            }
            ERROR_FILE_NOT_FOUND => return Ok(String::new()),
            ERROR_MORE_DATA => room = bytes,
            other => return Err(format!("the PATH could not be read (error {other})")),
        }
    }
    Err("the PATH kept changing while it was read".to_owned())
}

pub fn add_to_user_path(folder: &str) -> Result<bool, String> {
    let Some(longer) = crate::path_with(&user_path()?, folder) else { return Ok(false) };
    save_user_path(&longer)?;
    Ok(true)
}

pub fn remove_from_user_path(folder: &str) -> Result<bool, String> {
    let Some(shorter) = crate::path_without(&user_path()?, folder) else { return Ok(false) };
    save_user_path(&shorter)?;
    Ok(true)
}

pub fn on_user_path(folder: &str) -> bool {
    user_path().is_ok_and(|path| crate::path_with(&path, folder).is_none())
}

/// Saves the user's own PATH and tells running programs it changed.
fn save_user_path(value: &str) -> Result<(), String> {
    let (key, name, data) = (wide("Environment"), wide("Path"), wide(value));
    // SAFETY: the key and value names are NUL-terminated; `data` is the NUL-terminated value and
    // its length in bytes, NUL included, is what the call is told.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_EXPAND_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("the PATH could not be saved (error {status})"));
    }
    // Tell running programs (Explorer, which starts new terminals) on a thread of its own: each
    // window may take a moment to answer, and the person who pressed the button needn't wait.
    std::thread::spawn(|| {
        let what = wide("Environment");
        let mut answer = 0usize;
        // SAFETY: a broadcast whose text is NUL-terminated and lives through the call, which
        // returns within the time given; a window that has stopped answering is passed over.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                what.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                2000,
                &mut answer,
            );
        }
    });
    Ok(())
}

pub fn monitor_names() -> Vec<(String, String)> {
    let Some(paths) = active_paths() else { return Vec::new() };
    paths
        .iter()
        .filter_map(|path| {
            let device = device_name(path.sourceInfo.adapterId, path.sourceInfo.id)?;
            let monitor = monitor_name(path.targetInfo.adapterId, path.targetInfo.id)?;
            let monitor = monitor.trim();
            (!device.is_empty() && !monitor.is_empty()).then(|| (device, monitor.to_owned()))
        })
        .collect()
}

/// The displays in use now, each joining a display Windows draws (the path's source) to the
/// monitor that shows it (its target).
fn active_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // The displays can change between asking how much room the answer needs and asking for it;
    // Windows then says the room is too small, and it is asked again.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0u32, 0u32);
        // SAFETY: the two counts are u32s that outlive the call, which only writes them.
        if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) }
            != ERROR_SUCCESS
        {
            return None;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: `paths` and `modes` hold as many elements as the counts say, which is as many
        // as the call may write, and it lowers the counts to how many it wrote. The topology id
        // is null, as QDC_ONLY_ACTIVE_PATHS requires.
        let asked = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        match asked {
            ERROR_SUCCESS => {
                paths.truncate(path_count as usize);
                return Some(paths);
            }
            ERROR_INSUFFICIENT_BUFFER => {}
            _ => return None,
        }
    }
    None
}

/// The text of a fixed-size wide string, up to its first NUL.
/// The user's location as Windows names it: `DE`, or `XX` when none is set.
pub fn user_geo_name() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultGeoName;
    // A geo name is two letters or three digits; Windows asks for room for a few more.
    let mut name = [0u16; 16];
    // SAFETY: `name` is valid for the call to fill in, and its length is given with it; the call
    // writes no more than that, ending the text with a zero.
    let written = unsafe { GetUserDefaultGeoName(name.as_mut_ptr(), name.len() as i32) };
    (written > 0).then(|| from_wide(&name))
}

fn from_wide(text: &[u16]) -> String {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

/// The device name of a path's source (`\\.\DISPLAY2`), as GetMonitorInfoW gives it too.
fn device_name(adapter: LUID, id: u32) -> Option<String> {
    let mut request = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: adapter,
            id,
        },
        ..Default::default()
    };
    // SAFETY: the pointer is to the whole request, whose first field is the header (both are
    // repr(C)); the header names this kind of request and gives its whole size, and Windows
    // writes only within that.
    let answered = unsafe { DisplayConfigGetDeviceInfo((&raw mut request).cast()) } == 0;
    answered.then(|| from_wide(&request.viewGdiDeviceName))
}

/// The name a path's target, the monitor, gives itself ("Mi TV"), empty when it gives none.
fn monitor_name(adapter: LUID, id: u32) -> Option<String> {
    let mut request = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: adapter,
            id,
        },
        ..Default::default()
    };
    // SAFETY: as in `device_name`: a pointer to the whole request, whose header comes first,
    // names this kind of request and gives its whole size.
    let answered = unsafe { DisplayConfigGetDeviceInfo((&raw mut request).cast()) } == 0;
    answered.then(|| from_wide(&request.monitorFriendlyDeviceName))
}

pub fn current_thread_id() -> u32 {
    // SAFETY: GetCurrentThreadId takes nothing and only reads the calling thread's own number.
    unsafe { GetCurrentThreadId() }
}

/// Called by EnumThreadWindows for each window of the thread: keeps the first that is shown and
/// owned by no other (the file window itself, not a part of it), and stops there.
unsafe extern "system" fn first_shown(window: HWND, found: LPARAM) -> windows_sys::core::BOOL {
    // SAFETY: `window` is a window handle Windows hands over for this call; the two calls only
    // ask about it.
    let shown = unsafe { IsWindowVisible(window) != 0 && GetWindow(window, GW_OWNER).is_null() };
    if !shown {
        return 1;
    }
    // SAFETY: `found` is the address of the `HWND` that `bring_thread_window_forward` passes,
    // which outlives the enumeration (EnumThreadWindows returns only once every call is done).
    unsafe { *(found as *mut HWND) = window };
    0
}

pub fn bring_thread_window_forward(thread: u32) -> bool {
    let mut found: HWND = null_mut();
    // SAFETY: `first_shown` matches WNDENUMPROC and writes only to `found`, whose address is
    // passed as the LPARAM and stays valid for the whole call. A thread with no windows, or one
    // that has ended, makes EnumThreadWindows return false having called nothing.
    unsafe { EnumThreadWindows(thread, Some(first_shown), &raw mut found as LPARAM) };
    if found.is_null() {
        return false;
    }
    // SAFETY: `found` is a window handle Windows gave just now; if the window has closed since,
    // these calls fail harmlessly (they only take the handle by value).
    unsafe {
        if IsIconic(found) != 0 {
            ShowWindow(found, SW_RESTORE);
        }
        SetForegroundWindow(found) != 0
    }
}

/// OLE set up on the thread that made it, until it is dropped there (it can't leave the thread).
pub struct OleOnThisThread {
    /// Whether OleInitialize succeeded (it says so for a thread that had it already, too), so
    /// it is to be balanced.
    begun: bool,
    _here: std::marker::PhantomData<*const ()>,
}

pub fn ole_on_this_thread() -> OleOnThisThread {
    // SAFETY: OleInitialize's one argument is reserved and must be null. It sets up COM (as a
    // single-threaded apartment) and OLE for the calling thread only, and fails harmlessly
    // (returning an error, which is then not balanced) if the thread's COM is set up otherwise.
    let begun = unsafe { OleInitialize(null()) } >= 0;
    OleOnThisThread { begun, _here: std::marker::PhantomData }
}

impl Drop for OleOnThisThread {
    fn drop(&mut self) {
        if self.begun {
            // SAFETY: balances the successful OleInitialize above, on the same thread (the value
            // is neither Send nor Sync, so it is dropped where it was made).
            unsafe { OleUninitialize() };
        }
    }
}

pub fn front_back_to(window: isize) -> bool {
    let window = window as HWND;
    // SAFETY: GetForegroundWindow takes nothing and only answers which window is in front.
    let front = unsafe { GetForegroundWindow() };
    if front == window {
        return false;
    }
    if !front.is_null() {
        let mut process = 0;
        // SAFETY: `front` is a window handle Windows just gave; `process` is a local the call
        // writes the owning process's number into. A window closed since makes it write nothing.
        unsafe { GetWindowThreadProcessId(front, &mut process) };
        // SAFETY: GetCurrentProcessId takes nothing and reads this process's own number.
        if process != unsafe { GetCurrentProcessId() } {
            return false;
        }
    }
    // SAFETY: `window` is the app's own top-level window, handed over by the window library on
    // the thread that made it; the window in front is this program's, or none, so Windows lets
    // the program choose which of its windows is in front. A bad handle makes it fail harmlessly.
    unsafe { SetForegroundWindow(window) != 0 }
}
