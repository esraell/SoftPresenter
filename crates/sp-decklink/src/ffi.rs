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

use std::ffi::{CString, c_char, c_void};
use std::path::Path;
use std::ptr::NonNull;
use std::time::Duration;

use crate::player::Finished;
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

/// `SpDlCompletion` in the bridge.
#[repr(C)]
#[derive(Clone, Copy)]
struct RawCompletion {
    frame: *mut c_void,
    result: u32,
}

/// The bridge's open output and device notifications: never looked inside on this side.
#[repr(C)]
struct RawOut {
    _private: [u8; 0],
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
    fn sp_dl_out_open(id: i64, name: *const c_char, result: *mut i32) -> *mut RawOut;
    fn sp_dl_out_enable(out: *mut RawOut, mode: u32) -> i32;
    fn sp_dl_out_row_bytes(out: *mut RawOut, pixel_format: u32, width: i32) -> i32;
    fn sp_dl_out_keyer(out: *mut RawOut, on: i32, external: i32) -> i32;
    fn sp_dl_frame_new(out: *mut RawOut, width: i32, height: i32, row_bytes: i32, pixel_format: u32) -> *mut c_void;
    fn sp_dl_frame_write(frame: *mut c_void, bytes: *const u8, len: usize) -> i32;
    fn sp_dl_frame_release(frame: *mut c_void);
    fn sp_dl_out_schedule(out: *mut RawOut, frame: *mut c_void, time: i64, duration: i64, scale: i64) -> i32;
    fn sp_dl_out_start(out: *mut RawOut, time: i64, scale: i64) -> i32;
    fn sp_dl_out_stream_time(out: *mut RawOut, scale: i64, time: *mut i64) -> i32;
    fn sp_dl_out_wait(out: *mut RawOut, done: *mut RawCompletion, room: i32, timeout_ms: i32, flags: *mut u32) -> i32;
    fn sp_dl_out_close(out: *mut RawOut);
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

/// What a call into the driver gave, in plain words for the operator.
pub fn in_words(result: i32) -> String {
    // The COM results the driver gives, as both systems' headers define them.
    match result as u32 {
        0x8000_0009 => "it is in use by another program".to_owned(),
        0x8000_0002 => "the card is out of memory".to_owned(),
        0x8000_0003 => "the card refused the settings".to_owned(),
        0x8000_0004 => "the driver doesn't offer it (Desktop Video may be too old)".to_owned(),
        0x8000_0008 => "the driver said it failed".to_owned(),
        other => format!("the driver said {other:#010x}"),
    }
}

/// Flags the card gives beside the frames it finished: see the bridge's `SP_DL_*`.
pub const STOPPED: u32 = 1;
pub const PROFILE_CHANGING: u32 = 2;
pub const PROFILE_CHANGED: u32 = 4;

/// An output that is open. Dropping it stops playback, turns the keyer and the output off and
/// lets the device go.
pub struct Out(NonNull<RawOut>);

/// A frame of the card's own memory, to fill and schedule. Its identity (`key`) is what the
/// card names when it has finished with it.
pub struct Frame(NonNull<c_void>);

impl Frame {
    pub fn key(&self) -> usize {
        self.0.as_ptr().addr()
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: the frame came from sp_dl_frame_new and is released only here, once. The
        // driver keeps its own reference to a frame while it is scheduled, so releasing ours
        // then is sound too.
        unsafe { sp_dl_frame_release(self.0.as_ptr()) };
    }
}

impl Out {
    /// Opens the device with this persistent id (or, without one, this name). The error is
    /// `None` when there is no such device now, or the driver's result.
    pub fn open(id: Option<i64>, name: &str) -> Result<Out, Option<i32>> {
        let name = CString::new(name).map_err(|_| Some(0x8000_0003_u32 as i32))?;
        let mut result = 0i32;
        // SAFETY: `name` is a NUL-terminated string that outlives the call, and `result` is a
        // place for one number. The handle returned is owned by the `Out` made from it.
        let out = unsafe { sp_dl_out_open(id.unwrap_or(-1), name.as_ptr(), &mut result) };
        NonNull::new(out).map(Out).ok_or(if result == 1 { None } else { Some(result) })
    }

    /// Turns video output on in this mode. The error is the driver's result.
    pub fn enable(&mut self, mode: u32) -> Result<(), i32> {
        // SAFETY: the handle is live; a mode the device doesn't have is refused by the driver.
        match unsafe { sp_dl_out_enable(self.0.as_ptr(), mode) } {
            0 => Ok(()),
            error => Err(error),
        }
    }

    /// The bytes in each row of a frame of this format and width.
    pub fn row_bytes(&mut self, pixel_format: u32, width: u32) -> Option<usize> {
        let width = i32::try_from(width).ok()?;
        // SAFETY: the handle is live; the driver answers 0 for a format it doesn't know.
        let row = unsafe { sp_dl_out_row_bytes(self.0.as_ptr(), pixel_format, width) };
        usize::try_from(row).ok().filter(|row| *row > 0)
    }

    /// Turns the keyer on (inside the card over its input, or out as fill and key) or off.
    pub fn keyer(&mut self, on: bool, external: bool) -> Result<(), i32> {
        // SAFETY: the handle is live; a device without a keyer is answered with 1.
        match unsafe { sp_dl_out_keyer(self.0.as_ptr(), i32::from(on), i32::from(external)) } {
            0 => Ok(()),
            error => Err(error),
        }
    }

    pub fn new_frame(&mut self, width: u32, height: u32, row_bytes: usize, pixel_format: u32) -> Option<Frame> {
        let (width, height, row) =
            (i32::try_from(width).ok()?, i32::try_from(height).ok()?, i32::try_from(row_bytes).ok()?);
        // SAFETY: the handle is live; the frame returned (or null) is owned by the `Frame`
        // made from it.
        NonNull::new(unsafe { sp_dl_frame_new(self.0.as_ptr(), width, height, row, pixel_format) }).map(Frame)
    }

    /// Copies `bytes` into a frame that is not scheduled.
    pub fn write(frame: &mut Frame, bytes: &[u8]) -> Result<(), i32> {
        // SAFETY: the frame is live and ours alone to change (`&mut`: the caller writes only
        // into frames the card is not showing or holding); `bytes` is valid for its length,
        // and the bridge refuses (with 1) a frame smaller than that.
        match unsafe { sp_dl_frame_write(frame.0.as_ptr(), bytes.as_ptr(), bytes.len()) } {
            0 => Ok(()),
            error => Err(error),
        }
    }

    pub fn schedule(&mut self, frame: &Frame, at: i64, duration: i64, scale: i64) -> Result<(), i32> {
        // SAFETY: the handle and the frame are live; the driver keeps a reference to the frame
        // of its own until it has finished with it.
        match unsafe { sp_dl_out_schedule(self.0.as_ptr(), frame.0.as_ptr(), at, duration, scale) } {
            0 => Ok(()),
            error => Err(error),
        }
    }

    pub fn start(&mut self, at: i64, scale: i64) -> Result<(), i32> {
        // SAFETY: the handle is live.
        match unsafe { sp_dl_out_start(self.0.as_ptr(), at, scale) } {
            0 => Ok(()),
            error => Err(error),
        }
    }

    /// Where playback is now, in units of `scale` a second.
    pub fn stream_time(&mut self, scale: i64) -> Option<i64> {
        let mut time = 0i64;
        // SAFETY: the handle is live; `time` is a place for one number.
        let result = unsafe { sp_dl_out_stream_time(self.0.as_ptr(), scale, &mut time) };
        (result == 0).then_some(time)
    }

    /// Waits up to `timeout` for frames the card has finished with: each frame's key and how
    /// it went, and the flags noted since the last call.
    pub fn wait(&mut self, timeout: Duration) -> (Vec<(usize, Finished)>, u32) {
        let mut done = [RawCompletion { frame: std::ptr::null_mut(), result: 0 }; 32];
        let mut flags = 0u32;
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: the handle is live; `done` has room for the count given and the bridge writes
        // at most that many records, saying how many; `flags` is a place for one number.
        let count = unsafe { sp_dl_out_wait(self.0.as_ptr(), done.as_mut_ptr(), done.len() as i32, ms, &mut flags) };
        let count = usize::try_from(count).unwrap_or(0).min(done.len());
        let finished = done[..count]
            .iter()
            .map(|completion| {
                let how = match completion.result {
                    0 => Finished::Shown,
                    1 => Finished::Late,
                    2 => Finished::Dropped,
                    _ => Finished::Flushed,
                };
                (completion.frame.addr(), how)
            })
            .collect();
        (finished, flags)
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        // SAFETY: the handle came from sp_dl_out_open and is closed only here, once.
        unsafe { sp_dl_out_close(self.0.as_ptr()) };
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

    #[test]
    fn the_drivers_results_are_said_in_plain_words() {
        assert!(in_words(0x8000_0009_u32 as i32).contains("in use"));
        assert!(in_words(0x1234).contains("0x00001234"));
    }
}
