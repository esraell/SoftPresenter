//! The calls into the DeckLink driver, through SoftPresenter's bridge in `bridge/bridge.cpp`.
//! The only module of this crate with `unsafe` code; everything else uses the safe wrappers
//! here. Built on a Mac and on Linux only (see `build.rs`).
//!
//! What makes the calls sound, once for all of them:
//! - Every function declared below is the bridge's own, compiled into this crate from
//!   `bridge/bridge.cpp`, with exactly the C types written here (checked against that file).
//! - The bridge never lets a C++ exception out, and never calls back into Rust: the driver's
//!   callbacks only note what happened, and Rust asks for it (`Out::wait`, `Discovery::wait`).
//! - Each object the bridge hands out (`Out`, `Frame`, `Discovery`) is owned by one Rust value
//!   and freed once, by that value's `Drop`, with the bridge function named for it.
//! - Buffers handed to the bridge are this side's own, with their true lengths.

use std::ffi::{CString, c_char};
use std::path::Path;
use std::ptr::NonNull;
use std::time::Duration;

use crate::{Device, Mode};

/// `SpDlDevice` in the bridge.
#[repr(C)]
struct RawDevice {
    display_name: [c_char; 256],
    model_name: [c_char; 256],
    persistent_id: i64,
    sub_device: i64,
    sub_devices: i64,
    profile: i64,
    duplex: i64,
    connections: i64,
    can_play: i32,
    internal_keying: i32,
    external_keying: i32,
    modes: i32,
}

/// `SpDlMode` in the bridge.
#[repr(C)]
struct RawMode {
    mode: u32,
    field_dominance: u32,
    width: i32,
    height: i32,
    frame_duration: i64,
    time_scale: i64,
    bgra: i32,
    yuv: i32,
    keying: i32,
    name: [c_char; 64],
}

#[repr(C)]
struct RawDiscovery {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn sp_dl_load(path: *const c_char, err: *mut c_char, err_len: usize) -> i32;
    fn sp_dl_api_version() -> i64;
    fn sp_dl_list(
        devices: *mut RawDevice,
        device_room: i32,
        modes: *mut RawMode,
        mode_room: i32,
        device_count: *mut i32,
        mode_count: *mut i32,
    ) -> i32;
    fn sp_dl_discovery_start() -> *mut RawDiscovery;
    fn sp_dl_discovery_wait(handle: *mut RawDiscovery, seen: u64, timeout_ms: i32) -> u64;
    fn sp_dl_discovery_stop(handle: *mut RawDiscovery);
}

/// The text in a fixed buffer the bridge filled: up to its first zero byte, which the bridge
/// always writes.
fn text(bytes: &[c_char]) -> String {
    let bytes: Vec<u8> = bytes.iter().take_while(|byte| **byte != 0).map(|byte| *byte as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Opens the driver at `path`, if it isn't open already. The error is a plain sentence.
pub fn load(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(crate::NOT_INSTALLED.to_owned());
    }
    let path = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "The DeckLink driver's path has a zero byte in it.".to_owned())?;
    let mut err = [0 as c_char; 512];
    // SAFETY: `path` is a NUL-terminated string that outlives the call; `err` is a buffer of
    // the length given, which the bridge fills with a NUL-terminated sentence at most.
    let loaded = unsafe { sp_dl_load(path.as_ptr(), err.as_mut_ptr(), err.len()) };
    if loaded == 1 { Ok(()) } else { Err(text(&err)) }
}

/// The driver's version as BMDDeckLinkAPIVersion gives it (0x0F030100 for 15.3.1), once it is
/// open; `None` when it can't say.
pub fn api_version() -> Option<u32> {
    // SAFETY: takes nothing; the bridge returns 0 when the driver isn't open or can't say.
    let version = unsafe { sp_dl_api_version() };
    u32::try_from(version).ok().filter(|version| *version != 0)
}

/// Room for this many devices, and modes in all, in one listing: more than any computer has.
const DEVICE_ROOM: usize = 32;
const MODE_ROOM: usize = 2048;

/// Every device the driver knows, with the ways each can play out. `None` when the driver
/// isn't open.
pub fn list() -> Option<Vec<Device>> {
    let mut devices: Vec<RawDevice> = Vec::with_capacity(DEVICE_ROOM);
    let mut modes: Vec<RawMode> = Vec::with_capacity(MODE_ROOM);
    let (mut device_count, mut mode_count) = (0i32, 0i32);
    // SAFETY: both buffers have room for the counts given (their capacities), the bridge
    // writes at most that many whole records into each and says how many in the two counts,
    // and every field of a record is a plain number or byte, valid whatever it holds.
    let listed = unsafe {
        sp_dl_list(
            devices.as_mut_ptr(),
            DEVICE_ROOM as i32,
            modes.as_mut_ptr(),
            MODE_ROOM as i32,
            &mut device_count,
            &mut mode_count,
        )
    };
    if listed != 1 {
        return None;
    }
    let device_count = usize::try_from(device_count).unwrap_or(0).min(DEVICE_ROOM);
    let mode_count = usize::try_from(mode_count).unwrap_or(0).min(MODE_ROOM);
    // SAFETY: the bridge wrote `device_count` and `mode_count` whole records, from the start of
    // each buffer, within their capacities.
    unsafe {
        devices.set_len(device_count);
        modes.set_len(mode_count);
    }
    let mut modes = modes.into_iter();
    Some(
        devices
            .iter()
            .map(|raw| {
                let own: Vec<Mode> = modes
                    .by_ref()
                    .take(usize::try_from(raw.modes).unwrap_or(0))
                    .map(|mode| {
                        Mode::new(
                            mode.mode,
                            text(&mode.name),
                            (mode.width.max(0) as u32, mode.height.max(0) as u32),
                            (mode.frame_duration, mode.time_scale),
                            mode.field_dominance,
                            (mode.bgra == 1, mode.yuv == 1, mode.keying == 1),
                        )
                    })
                    .collect();
                Device {
                    name: text(&raw.display_name),
                    model: text(&raw.model_name),
                    id: (raw.persistent_id >= 0).then_some(raw.persistent_id),
                    sub_device: u32::try_from(raw.sub_device).ok(),
                    sub_devices: u32::try_from(raw.sub_devices).ok(),
                    profile: u32::try_from(raw.profile).unwrap_or(0),
                    duplex: u32::try_from(raw.duplex).unwrap_or(0),
                    connections: u32::try_from(raw.connections).unwrap_or(0),
                    can_play: raw.can_play == 1,
                    internal_keying: raw.internal_keying == 1,
                    external_keying: raw.external_keying == 1,
                    modes: own,
                }
            })
            .collect(),
    )
}

/// The driver telling of devices plugged in and taken away. Stops telling when dropped.
pub struct Discovery(NonNull<RawDiscovery>);

impl Discovery {
    /// Starts listening; `None` when the driver isn't open or can't tell.
    pub fn start() -> Option<Discovery> {
        // SAFETY: takes nothing; returns a handle this value then owns, or null.
        NonNull::new(unsafe { sp_dl_discovery_start() }).map(Discovery)
    }

    /// Waits up to `timeout` for the count of changes to differ from `seen`, and gives it.
    pub fn wait(&self, seen: u64, timeout: Duration) -> u64 {
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: the handle is live (it is freed only by `drop`).
        unsafe { sp_dl_discovery_wait(self.0.as_ptr(), seen, ms) }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        // SAFETY: the handle came from sp_dl_discovery_start and is freed only here, once.
        unsafe { sp_dl_discovery_stop(self.0.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_driver_where_there_is_none_is_said_not_to_be_installed() {
        let nowhere = Path::new("/nonexistent/DeckLinkAPI.framework");
        let problem = load(nowhere).unwrap_err();
        assert!(problem.contains("isn't installed"), "{problem}");
    }

    #[test]
    fn the_buffers_text_ends_at_its_first_zero() {
        let mut buffer = [0 as c_char; 8];
        for (at, byte) in b"Duo".iter().enumerate() {
            buffer[at] = *byte as c_char;
        }
        assert_eq!(text(&buffer), "Duo");
    }
}
