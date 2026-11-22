//! The NDI runtime, opened when it is first needed.
//!
//! The declarations below follow the NDI SDK's headers `Processing.NDI.Lib.h`,
//! `Processing.NDI.structs.h` and `Processing.NDI.Send.h`, which carry this licence:
//!
//! Copyright (C) 2023-2026 Vizrt NDI AB. All rights reserved.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! and associated documentation files (the "Software"), to deal in the Software without
//! restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use libloading::Library;

/// What the operator is told when the runtime can't be found.
pub const NOT_INSTALLED: &str = "NDI isn't installed on this computer. Install NDI Tools (free, from https://ndi.video/), then turn this screen on again.";

/// `NDIlib_send_create_t`.
#[repr(C)]
struct SendCreate {
    name: *const c_char,
    groups: *const c_char,
    clock_video: bool,
    clock_audio: bool,
}

/// `NDIlib_video_frame_v2_t`.
#[repr(C)]
struct VideoFrame {
    xres: c_int,
    yres: c_int,
    four_cc: u32,
    frame_rate_n: c_int,
    frame_rate_d: c_int,
    picture_aspect_ratio: f32,
    frame_format_type: c_int,
    timecode: i64,
    data: *mut u8,
    line_stride_in_bytes: c_int,
    metadata: *const c_char,
    timestamp: i64,
}

/// `NDI_LIB_FOURCC('R', 'G', 'B', 'X')`: 8-bit red, green, blue and an ignored fourth byte.
const FOUR_CC_RGBX: u32 = u32::from_le_bytes(*b"RGBX");
/// `NDI_LIB_FOURCC('R', 'G', 'B', 'A')`: 8-bit red, green, blue and alpha. NDI's SDK
/// documentation says of it, as of RGBX: "This data is not pre-multiplied."
const FOUR_CC_RGBA: u32 = u32::from_le_bytes(*b"RGBA");
/// `NDIlib_frame_format_type_progressive`.
const PROGRESSIVE: c_int = 1;
/// `NDIlib_send_timecode_synthesize`: NDI makes the timecode.
const SYNTHESIZE_TIMECODE: i64 = i64::MAX;

type Initialize = unsafe extern "C" fn() -> bool;
type SendCreateFn = unsafe extern "C" fn(*const SendCreate) -> *mut c_void;
type SendDestroy = unsafe extern "C" fn(*mut c_void);
type SendVideo = unsafe extern "C" fn(*mut c_void, *const VideoFrame);
type SendConnections = unsafe extern "C" fn(*mut c_void, u32) -> c_int;

/// The runtime's functions this app uses, with the library they live in kept open for as long
/// as the app runs.
pub struct Runtime {
    _library: Library,
    send_create: SendCreateFn,
    send_destroy: SendDestroy,
    send_video: SendVideo,
    /// `NDIlib_send_get_no_connections`, if this runtime has it. Without it a source can't tell
    /// whether anyone is watching, and sends all the time.
    send_connections: Option<SendConnections>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static OPENING: Mutex<()> = Mutex::new(());

/// The runtime, opened the first time it is asked for (which may take a moment: it is asked
/// for on NDI's own threads, never on the screens' or the window's). Not finding it is not
/// remembered, so NDI installed while the app runs is found the next time a source starts. The
/// error is a plain sentence for the operator.
pub fn get() -> Result<&'static Runtime, String> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let _one_at_a_time = OPENING.lock();
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = open(&places())?;
    Ok(RUNTIME.get_or_init(|| runtime))
}

/// Where the runtime is on this computer, if it is: only looks, opens nothing.
/// The runtime inside each of NDI's own apps in a folder of applications, in name order.
fn ndi_tools_runtimes(applications: &std::path::Path, name: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(applications)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|app| {
                    app.file_name().is_some_and(|n| {
                        n.to_string_lossy().starts_with("NDI ") && n.to_string_lossy().ends_with(".app")
                    })
                })
                .map(|app| app.join("Contents/Frameworks").join(name))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

pub fn locate() -> Option<PathBuf> {
    places().into_iter().find(|path| path.is_file())
}

/// Where the runtime may be, in the order to try: the folder NDI's own variable names, a copy
/// inside the app, where NDI's runtime installer puts it, the SDK's copy, and on a Mac with NDI
/// Tools but not the runtime installer, the copy inside one of NDI's own apps.
fn places() -> Vec<PathBuf> {
    let name = if cfg!(target_os = "macos") {
        "libndi.dylib"
    } else if cfg!(windows) {
        "Processing.NDI.Lib.x64.dll"
    } else {
        "libndi.so.6"
    };
    let mut places = Vec::new();
    // Testing aid: behave as on a computer without NDI.
    if std::env::var_os("SOFTPRESENTER_HIDE_NDI").is_some() {
        return places;
    }
    if let Some(folder) = std::env::var_os("NDI_RUNTIME_DIR_V6") {
        places.push(PathBuf::from(folder).join(name));
    }
    if let Some(app) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.to_path_buf())) {
        places.push(app.join("../Frameworks").join(name));
        places.push(app.join(name));
    }
    if cfg!(target_os = "macos") {
        places.push(PathBuf::from("/usr/local/lib").join(name));
        places.push(PathBuf::from("/Library/NDI SDK for Apple/lib/macOS").join(name));
        // NDI Tools for Mac (NDI Virtual Input, NDI Scan Converter…) carry the runtime inside
        // each app and install nothing in /usr/local/lib, so a Mac with NDI Tools would
        // otherwise count as having no NDI. Only NDI's own apps are looked in.
        places.extend(ndi_tools_runtimes(std::path::Path::new("/Applications"), name));
    } else if !cfg!(windows) {
        places.push(PathBuf::from("/usr/lib").join(name));
        places.push(PathBuf::from("/usr/local/lib").join(name));
    }
    places
}

/// Opens the first runtime found in `places` and starts it.
fn open(places: &[PathBuf]) -> Result<Runtime, String> {
    let Some(path) = places.iter().find(|path| path.is_file()) else {
        return Err(NOT_INSTALLED.to_owned());
    };
    // SAFETY: the file is NDI's runtime, found where NDI installs it (or where the operator's
    // NDI_RUNTIME_DIR_V6 points); opening it runs only its own set-up, which NDI documents as
    // safe to do on any thread.
    let library = unsafe { Library::new(path) }
        .map_err(|e| format!("The NDI runtime at {} could not be opened: {e}", path.display()))?;
    let missing = |name: &str, e: libloading::Error| format!("The NDI runtime is too old or damaged (no {name}): {e}");
    // SAFETY: each name is a function the NDI runtime exports, and the type it is read as is
    // that function's signature in NDI's headers (declared above). The pointers are used only
    // while the library stays open, which is for as long as the app runs.
    let (initialize, send_create, send_destroy, send_video) = unsafe {
        (
            *library.get::<Initialize>(b"NDIlib_initialize\0").map_err(|e| missing("NDIlib_initialize", e))?,
            *library.get::<SendCreateFn>(b"NDIlib_send_create\0").map_err(|e| missing("NDIlib_send_create", e))?,
            *library.get::<SendDestroy>(b"NDIlib_send_destroy\0").map_err(|e| missing("NDIlib_send_destroy", e))?,
            *library
                .get::<SendVideo>(b"NDIlib_send_send_video_v2\0")
                .map_err(|e| missing("NDIlib_send_send_video_v2", e))?,
        )
    };
    // SAFETY: as above: a function the runtime exports, read as its signature in
    // `Processing.NDI.Send.h`. A runtime without it is still used; its sources send all the time.
    let send_connections =
        unsafe { library.get::<SendConnections>(b"NDIlib_send_get_no_connections\0") }.ok().map(|function| *function);
    // SAFETY: NDIlib_initialize takes nothing and may be called more than once; the library it
    // lives in is open.
    if !unsafe { initialize() } {
        return Err("NDI can't run on this computer's processor.".to_owned());
    }
    Ok(Runtime { _library: library, send_create, send_destroy, send_video, send_connections })
}

/// An NDI source on the network. Dropping it takes the source away.
pub struct Source {
    runtime: &'static Runtime,
    instance: *mut c_void,
    // Kept alive for as long as NDI may read the name.
    _name: CString,
}

impl Source {
    /// Starts a source with this name.
    pub fn new(runtime: &'static Runtime, name: &str) -> Result<Source, String> {
        let name = CString::new(name).map_err(|_| "An NDI name can't contain a zero byte.".to_owned())?;
        let settings =
            SendCreate { name: name.as_ptr(), groups: std::ptr::null(), clock_video: true, clock_audio: false };
        // SAFETY: `settings` is a valid NDIlib_send_create_t for the length of the call, its name
        // a NUL-terminated string that outlives the source (kept in `_name`); groups may be null.
        let instance = unsafe { (runtime.send_create)(&settings) };
        if instance.is_null() {
            return Err("The NDI source could not be created.".to_owned());
        }
        Ok(Source { runtime, instance, _name: name })
    }

    /// How many receivers are connected to the source now, or `None` when this runtime can't
    /// say. Never waits.
    pub fn receivers(&self) -> Option<u32> {
        let count = self.runtime.send_connections?;
        // SAFETY: the instance is a live source from NDIlib_send_create, and a zero timeout asks
        // only for the current count, so the call returns at once.
        let receivers = unsafe { count(self.instance, 0) };
        u32::try_from(receivers).ok()
    }

    /// Sends one picture of tightly packed RGBA rows: opaque (the fourth byte is ignored), or
    /// with `alpha`, the fourth byte saying how solid each pixel is. Waits, when NDI paces the
    /// source, until it is time for the next picture: call it only from the source's own thread.
    pub fn send(&self, width: u32, height: u32, rgba: &[u8], alpha: bool) {
        let (Ok(w), Ok(h)) = (c_int::try_from(width), c_int::try_from(height)) else { return };
        if rgba.len() != width as usize * height as usize * 4 {
            return;
        }
        let frame = VideoFrame {
            xres: w,
            yres: h,
            four_cc: if alpha { FOUR_CC_RGBA } else { FOUR_CC_RGBX },
            frame_rate_n: 30,
            frame_rate_d: 1,
            picture_aspect_ratio: 0.0,
            frame_format_type: PROGRESSIVE,
            timecode: SYNTHESIZE_TIMECODE,
            // NDI's frame holds a mutable pointer, but sending only reads through it.
            data: rgba.as_ptr().cast_mut(),
            line_stride_in_bytes: w * 4,
            metadata: std::ptr::null(),
            timestamp: 0,
        };
        // SAFETY: the instance is a live source from NDIlib_send_create; `frame` describes
        // `rgba` exactly (its size was checked), and the synchronous send has finished with the
        // data when it returns. NDI's header declares the frame's data pointer without
        // `const` (only the frame itself is handed over as `const`), so the declaration promises
        // nothing; this rests on what a synchronous send does, which is only to read the
        // picture, converting and compressing it into NDI's own buffers. So nothing is written
        // through a pointer made from a shared slice, and other sources reading the same
        // picture on their own threads at the same time is sound.
        unsafe { (self.runtime.send_video)(self.instance, &frame) };
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: the instance came from NDIlib_send_create and is destroyed only here, once.
        unsafe { (self.runtime.send_destroy)(self.instance) };
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_runtime_inside_ndis_own_apps_is_found_and_other_apps_are_left_alone() {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for app in ["NDI Virtual Input.app", "NDI Scan Converter.app", "Other Show.app"] {
            std::fs::create_dir_all(folder.join(app).join("Contents/Frameworks")).unwrap();
        }
        let found = ndi_tools_runtimes(&folder, "libndi.dylib");
        let apps: Vec<String> = found
            .iter()
            .map(|p| p.components().rev().nth(3).unwrap().as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(apps, ["NDI Scan Converter.app", "NDI Virtual Input.app"]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    use super::*;

    #[test]
    fn with_no_runtime_anywhere_it_says_ndi_isnt_installed_and_where_to_get_it() {
        let nowhere = vec![PathBuf::from("/nonexistent/libndi.dylib")];
        let problem = open(&nowhere).err().unwrap();
        assert!(problem.contains("isn't installed") && problem.contains("https://ndi.video/"));
    }

    #[test]
    fn the_rgbx_and_rgba_codes_are_ndis() {
        // NDI_LIB_FOURCC('R','G','B','X'): the first letter in the lowest byte.
        let code = |last: u8| u32::from(b'R') | u32::from(b'G') << 8 | u32::from(b'B') << 16 | u32::from(last) << 24;
        assert_eq!(FOUR_CC_RGBX, code(b'X'));
        assert_eq!(FOUR_CC_RGBA, code(b'A'));
    }
}
