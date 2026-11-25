//! What the app, the programs it started (FFmpeg) and the computer are using, for the anonymous
//! reports. Plain reads of the system's counters; nothing is changed.

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, GetSystemTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// A time as Windows counts it, in tenths of a microsecond.
fn filetime(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

const EMPTY_TIME: FILETIME = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };

/// The processor time (kernel and user) and private memory of a process, if it can be asked.
fn process_figures(process: HANDLE) -> Option<(f64, u64)> {
    let (mut created, mut exited, mut kernel, mut user) = (EMPTY_TIME, EMPTY_TIME, EMPTY_TIME, EMPTY_TIME);
    // SAFETY: the four FILETIMEs are valid for the call to fill in; `process` is a handle the
    // caller holds open.
    if unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let seconds = (filetime(kernel) + filetime(user)) as f64 / 1e7;
    // SAFETY: an all-zero PROCESS_MEMORY_COUNTERS_EX is valid (plain numbers).
    let mut memory: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    memory.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    // SAFETY: `memory` has its size field set and is valid for the call to fill in. The _EX
    // struct begins with the plain struct, which is what the call takes a pointer to.
    let got =
        unsafe { K32GetProcessMemoryInfo(process, (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(), memory.cb) };
    Some((seconds, if got == 0 { 0 } else { memory.PrivateUsage as u64 }))
}

/// This process and the processes it started, and the whole computer.
pub fn process_stats() -> Option<crate::ProcessStats> {
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
    let me = unsafe { GetCurrentProcess() };
    let (app_cpu_s, app_private_bytes) = process_figures(me)?;
    let (mut idle, mut kernel, mut user) = (EMPTY_TIME, EMPTY_TIME, EMPTY_TIME);
    // SAFETY: the three FILETIMEs are valid for the call to fill in.
    let times = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } != 0;
    // Kernel time includes the idle time.
    let busy = (filetime(kernel) + filetime(user)).saturating_sub(filetime(idle)) as f64 / 1e7;
    // SAFETY: an all-zero MEMORYSTATUSEX is valid (plain numbers); its length is set next.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `status` has its length field set and is valid for the call to fill in.
    let memory = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    let (kids_cpu_s, kids_private_bytes) = children_figures();
    Some(crate::ProcessStats {
        app_cpu_s,
        app_private_bytes,
        kids_cpu_s,
        kids_private_bytes,
        system_busy_s: times.then_some(busy),
        free_bytes: memory.then_some(status.ullAvailPhys),
        total_bytes: memory.then_some(status.ullTotalPhys),
    })
}

/// The processor time and private memory of every process this one started, and theirs in turn,
/// added up (FFmpeg, mostly).
fn children_figures() -> (f64, u64) {
    // SAFETY: a snapshot of the process list takes plain values; the handle is closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
        return (0.0, 0);
    }
    let mut listed: Vec<(u32, u32)> = Vec::new();
    // SAFETY: an all-zero PROCESSENTRY32W is valid (plain numbers and an array of them).
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `snapshot` is open and `entry` has its size field set and is valid to fill in.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        listed.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as for Process32FirstW.
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: `snapshot` is a handle just opened and not used again.
    unsafe { CloseHandle(snapshot) };
    // Everything that descends from this process.
    let mut family = vec![std::process::id()];
    let mut grew = true;
    while grew {
        grew = false;
        for (pid, parent) in &listed {
            if family.contains(parent) && !family.contains(pid) {
                family.push(*pid);
                grew = true;
            }
        }
    }
    let (mut seconds, mut bytes) = (0.0, 0u64);
    for pid in family.into_iter().skip(1) {
        // SAFETY: OpenProcess takes plain values and answers null when it may not open it.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            continue;
        }
        if let Some((cpu, private)) = process_figures(process) {
            seconds += cpu;
            bytes += private;
        }
        // SAFETY: `process` is a handle just opened and not used again.
        unsafe { CloseHandle(process) };
    }
    (seconds, bytes)
}
