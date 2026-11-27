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
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER,
    ERROR_MORE_DATA, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, HWND, INVALID_HANDLE_VALUE,
    LPARAM, LRESULT, LUID, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    SetStdHandle,
};
use windows_sys::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_EXPAND_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId, GetCurrentThreadId};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, ENDSESSION_LOGOFF, EnumThreadWindows, GW_OWNER, GetForegroundWindow, GetWindow,
    GetWindowThreadProcessId, HWND_BROADCAST, IsIconic, IsWindowVisible, SMTO_ABORTIFHUNG, SW_RESTORE,
    SendMessageTimeoutW, SendMessageW, SetForegroundWindow, ShowWindow, WM_CLOSE, WM_ENDSESSION, WM_NCDESTROY,
    WM_QUERYENDSESSION, WM_SETTINGCHANGE,
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

/// A named mutex this program has open. Its name is held while any handle to it is open: this
/// one is closed when the value is dropped, or by Windows when the program ends, however it ends.
pub struct HeldName(HANDLE);

impl Drop for HeldName {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by CreateMutexW in `hold_name`, belongs to this value
        // alone and is closed only here, once.
        unsafe { CloseHandle(self.0) };
    }
}

pub fn hold_name(name: &str) -> Result<Option<HeldName>, String> {
    let wide_name = wide(name);
    // SAFETY: `wide_name` is NUL-terminated and outlives the call. Null security attributes give
    // the mutex the default security and a handle no program this one starts inherits (FFmpeg,
    // the updater); FALSE: nobody owns the mutex, only its name is held.
    let handle = unsafe { CreateMutexW(null(), 0, wide_name.as_ptr()) };
    // Read at once, before any other call can change it: CreateMutexW opens the mutex another
    // program made with that name, and says so only here.
    // SAFETY: GetLastError takes nothing and reads the calling thread's last error.
    let error = unsafe { GetLastError() };
    // A mutex of that name that this program may not open with full access, as CreateMutexW
    // asks for one that exists (one a copy started as administrator made, say), is held too.
    if handle.is_null() && error == ERROR_ACCESS_DENIED {
        return Ok(None);
    }
    if handle.is_null() {
        return Err(format!("Windows couldn't make the mutex {name} (error {error})"));
    }
    if error == ERROR_ALREADY_EXISTS {
        // SAFETY: the handle was opened just now by CreateMutexW and is not used again.
        unsafe { CloseHandle(handle) };
        return Ok(None);
    }
    Ok(Some(HeldName(handle)))
}

pub fn move_written_through(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide_path = |path: &std::path::Path| path.as_os_str().encode_wide().collect::<Vec<u16>>();
    let (mut from, mut to) = (wide_path(from), wide_path(to));
    // A NUL inside a name would end it early, so another file would be moved: refused instead.
    if from.contains(&0) || to.contains(&0) {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    from.push(0);
    to.push(0);
    // SAFETY: `from` and `to` are NUL-terminated, hold no other NUL and outlive the call, which
    // only reads them; the flags are plain values (replace a file there, and return only once
    // the move is written to the disk).
    let moved = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) };
    if moved == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
}

pub fn let_others_come_forward() {
    // SAFETY: AllowSetForegroundWindow takes a process number by value (ASFW_ANY: any process)
    // and fails harmlessly when this program may not bring a window to the front itself.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}

pub fn ui_languages() -> Vec<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let (mut count, mut room) = (0u32, 0u32);
    // SAFETY: with no buffer the call only writes how many languages there are, and the room
    // their names need, into the two u32s, which outlive it.
    let sized = unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, null_mut(), &mut room) };
    if sized == 0 || room == 0 {
        return Vec::new();
    }
    let mut names = vec![0u16; room as usize];
    // SAFETY: `names` has room for `room` wide characters, which the call is told and writes no
    // more than; it fails (and is passed over) if the list grew since.
    if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, names.as_mut_ptr(), &mut room) } == 0 {
        return Vec::new();
    }
    names.truncate(room as usize);
    names.split(|&c| c == 0).filter(|name| !name.is_empty()).map(String::from_utf16_lossy).collect()
}

// Windows ending the session: a log-off, a restart or a shut-down. Windows asks every top-level
// window whether the session may end (WM_QUERYENDSESSION, which the window's own procedure
// answers yes), then tells each it is ending (WM_ENDSESSION, wParam TRUE), and may end the
// program any time after that answer. The window library handles neither, so the app's window
// is subclassed: on the second, the window is asked to close (a WM_CLOSE handed to the window
// library's procedure, which tells the app at once), and only then is the answer given.

/// Set once Windows has said the session is ending, before the window is asked to close.
static SESSION_ENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set by the app once it has saved and stopped for it.
static SESSION_END_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set when the app hadn't saved and stopped by the time Windows was answered (it was busy, and
/// the window library kept the close for later).
static SESSION_END_LATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// This module's number for its subclass of a window, told apart from any other's.
const SESSION_SUBCLASS: usize = 0x5350_5345;

unsafe extern "system" fn session_end_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass: usize,
    _data: usize,
) -> LRESULT {
    use std::sync::atomic::Ordering::SeqCst;
    if message == WM_ENDSESSION && wparam != 0 {
        SESSION_ENDING.store(true, SeqCst);
        // SAFETY: `window` is the window Windows calls this subclass for, on its own thread; the
        // close request carries no pointers, and goes to the next procedure in line (the window
        // library's), which tells the app before it returns unless the app is busy.
        unsafe { DefSubclassProc(window, WM_CLOSE, 0, 0) };
        if !SESSION_END_DONE.load(SeqCst) {
            SESSION_END_LATE.store(true, SeqCst);
        }
        return 0;
    }
    if message == WM_NCDESTROY {
        // SAFETY: the window being destroyed, on its own thread: this subclass is taken off it
        // before it goes, as Microsoft asks, with the procedure and number it was put on with.
        unsafe { RemoveWindowSubclass(window, Some(session_end_proc), SESSION_SUBCLASS) };
    }
    // SAFETY: every other message goes on to the window's own procedure as it came, as a
    // subclass must pass it: the values are the ones Windows gave this call.
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

pub fn on_session_end(window: isize) -> bool {
    // SAFETY: `window` is the app's own top-level window, handed over by the window library on
    // the thread that made it (Windows refuses a subclass from any other thread, or for a bad
    // handle, and the call then fails harmlessly); `session_end_proc` matches SUBCLASSPROC and
    // lives as long as the program; the data passed is a plain zero.
    unsafe { SetWindowSubclass(window as HWND, Some(session_end_proc), SESSION_SUBCLASS, 0) != 0 }
}

pub fn session_ending() -> bool {
    SESSION_ENDING.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn session_end_done() {
    SESSION_END_DONE.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub fn session_end_late() -> bool {
    SESSION_END_LATE.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn pretend_session_end(window: isize) {
    let window = window as HWND;
    let logoff = ENDSESSION_LOGOFF as LPARAM;
    // SAFETY: SendMessageW sends plain numbers (no pointers) to a window handle and waits for
    // its answer; for a window gone since, the call fails harmlessly.
    unsafe {
        SendMessageW(window, WM_QUERYENDSESSION, 0, logoff);
        SendMessageW(window, WM_ENDSESSION, 1, logoff);
    }
}

/// Windows asked to keep the display on, and the computer awake, while held: a power request.
pub struct DisplayKeptOn {
    request: HANDLE,
    /// Whether the computer is kept awake too, not only the display.
    system: bool,
}

impl DisplayKeptOn {
    pub fn system_too(&self) -> bool {
        self.system
    }
}

impl Drop for DisplayKeptOn {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Power::{
            PowerClearRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired,
        };
        // SAFETY: `request` was made by PowerCreateRequest in `keep_display_on`, belongs to this
        // value alone, and is cleared and closed only here, once; the request types cleared are
        // the ones that were set on it.
        unsafe {
            PowerClearRequest(self.request, PowerRequestDisplayRequired);
            if self.system {
                PowerClearRequest(self.request, PowerRequestSystemRequired);
            }
            CloseHandle(self.request);
        }
    }
}

pub fn keep_display_on(reason: &str) -> Result<DisplayKeptOn, String> {
    use windows_sys::Win32::System::Power::{
        PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired, PowerSetRequest,
    };
    use windows_sys::Win32::System::Threading::{
        POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
    };
    // POWER_REQUEST_CONTEXT_VERSION, which windows-sys keeps with the system services, a part of
    // it this crate doesn't otherwise need.
    const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;
    let mut reason = wide(reason);
    let context = REASON_CONTEXT {
        Version: POWER_REQUEST_CONTEXT_VERSION,
        Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
        Reason: REASON_CONTEXT_0 { SimpleReasonString: reason.as_mut_ptr() },
    };
    // SAFETY: `context` is a whole REASON_CONTEXT whose reason is a NUL-terminated wide string;
    // both outlive the call, and Windows keeps a copy of the reason with the request.
    let request = unsafe { PowerCreateRequest(&context) };
    if !usable(request) {
        // SAFETY: GetLastError takes nothing and reads the calling thread's last error.
        let error = unsafe { GetLastError() };
        return Err(format!("Windows couldn't make a power request (error {error})"));
    }
    // SAFETY: `request` is the request just made, open until it is closed here or by the value's
    // Drop.
    if unsafe { PowerSetRequest(request, PowerRequestDisplayRequired) } == 0 {
        // SAFETY: GetLastError takes nothing and reads the calling thread's last error.
        let error = unsafe { GetLastError() };
        // SAFETY: the request was made just now, has nothing set, and is not used again.
        unsafe { CloseHandle(request) };
        return Err(format!("Windows wouldn't keep the display on (error {error})"));
    }
    // SAFETY: as above; a computer kept awake as well is asked for on the same request.
    let system = unsafe { PowerSetRequest(request, PowerRequestSystemRequired) } != 0;
    Ok(DisplayKeptOn { request, system })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW;

    /// How many times the test's window was asked to close; the test's window marks the app's
    /// saving done when it is.
    static CLOSES: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn plain_window(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if message == WM_CLOSE {
            CLOSES.fetch_add(1, SeqCst);
            session_end_done();
            return 0;
        }
        // SAFETY: the values Windows gave this call, passed on to its usual handling.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    /// A window of the test's own, never shown, with the subclass the app's window gets: asked
    /// whether the session may end, it says yes; told the session isn't ending after all, it does
    /// nothing; told it is, it is asked to close before the answer goes back.
    #[test]
    fn a_window_is_asked_to_close_before_windows_is_told_the_session_may_end() {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, RegisterClassW, WNDCLASSW, WS_OVERLAPPED,
        };
        let name = wide(&format!("SoftPresenterSessionEndTest{}", std::process::id()));
        // SAFETY: GetModuleHandleW with no name answers this program's own module.
        let module = unsafe { GetModuleHandleW(null()) };
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(plain_window),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: module,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: name.as_ptr(),
        };
        // SAFETY: the class names this module's procedure (matching WNDPROC) and a NUL-terminated
        // name that outlives the class's use in this test; the call only reads it.
        assert_ne!(unsafe { RegisterClassW(&class) }, 0, "the window class");
        // SAFETY: a window of that class, never shown, made on this thread with no parent, menu
        // or data; the strings are NUL-terminated (or null) and outlive the call.
        let window = unsafe {
            CreateWindowExW(
                0,
                name.as_ptr(),
                null(),
                WS_OVERLAPPED,
                0,
                0,
                10,
                10,
                null_mut(),
                null_mut(),
                module,
                null(),
            )
        };
        assert!(!window.is_null(), "the window");
        assert!(on_session_end(window as isize), "subclassed");
        // SAFETY: plain numbers sent to the test's own window, on its thread.
        let may_end = unsafe { SendMessageW(window, WM_QUERYENDSESSION, 0, ENDSESSION_LOGOFF as LPARAM) };
        assert_ne!(may_end, 0, "the session may end");
        // SAFETY: as above.
        unsafe { SendMessageW(window, WM_ENDSESSION, 0, ENDSESSION_LOGOFF as LPARAM) };
        assert!(!session_ending() && CLOSES.load(SeqCst) == 0, "not ending after all: nothing");
        pretend_session_end(window as isize);
        assert!(session_ending(), "ending");
        assert_eq!(CLOSES.load(SeqCst), 1, "asked to close, once");
        assert!(!session_end_late(), "saved before Windows was answered");
        // SAFETY: the test's own window, on its thread; the subclass comes off as it goes.
        assert_ne!(unsafe { DestroyWindow(window) }, 0);
    }
}
