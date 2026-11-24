use std::ptr::null;

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    SetStdHandle,
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
