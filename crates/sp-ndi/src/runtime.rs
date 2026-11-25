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
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use libloading::Library;

/// What the operator is told when the runtime can't be found. NDI Tools, which installs the
/// runtime on a Mac or on Windows, doesn't exist for Linux: there the app downloads NDI itself.
pub const NOT_INSTALLED: &str = not_installed(cfg!(target_os = "linux"));

/// The words of [`NOT_INSTALLED`], on Linux or elsewhere.
const fn not_installed(linux: bool) -> &'static str {
    if linux {
        "NDI isn't installed on this computer. Download NDI\u{2026} in the Screens window gets it (free, from NDI)."
    } else {
        "NDI isn't installed on this computer. Install NDI Tools (free, from https://ndi.video/), then turn this screen on again."
    }
}

/// The folder of the app's own where it keeps the runtime it downloaded (Linux only).
static OWN_FOLDER: OnceLock<PathBuf> = OnceLock::new();

/// Says where the app keeps the runtime it downloads itself on Linux, where NDI Tools doesn't
/// exist. It is looked in right after the folder `NDI_RUNTIME_DIR_V6` names. Only the first
/// call counts.
pub fn set_own_folder(folder: PathBuf) {
    let _ = OWN_FOLDER.set(folder);
}

/// The file the runtime was opened from, once it has been. It stays open until the app quits.
pub fn opened_from() -> Option<&'static Path> {
    RUNTIME.get().map(|runtime| runtime.path.as_path())
}

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

/// `NDIlib_audio_frame_v2_t`: a block of sound, each channel's samples together (planar), as
/// 32-bit floats.
#[repr(C)]
struct AudioFrame {
    sample_rate: c_int,
    no_channels: c_int,
    no_samples: c_int,
    timecode: i64,
    data: *mut f32,
    channel_stride_in_bytes: c_int,
    metadata: *const c_char,
    timestamp: i64,
}

/// `NDIlib_source_t`: a source's name and, in the second pointer, its address (which this app
/// leaves to NDI).
#[repr(C)]
struct SourceT {
    name: *const c_char,
    url: *const c_char,
}

/// `NDIlib_find_create_t`.
#[repr(C)]
struct FindCreate {
    show_local_sources: bool,
    groups: *const c_char,
    extra_ips: *const c_char,
}

/// `NDIlib_recv_create_v3_t`.
#[repr(C)]
struct RecvCreate {
    source: SourceT,
    color_format: c_int,
    bandwidth: c_int,
    allow_video_fields: bool,
    name: *const c_char,
}

/// `NDIlib_recv_color_format_RGBX_RGBA`: red, green, blue first, with an ignored fourth byte for
/// a source with no alpha and the alpha for one with.
const COLOR_RGBX_RGBA: c_int = 2;
/// `NDIlib_recv_bandwidth_video_only_highest` and `_lowest`: the picture only, full or lighter.
const BANDWIDTH_FULL: c_int = 101;
const BANDWIDTH_LIGHT: c_int = 1;
/// `NDIlib_frame_type_video` and `NDIlib_frame_type_error`.
const FRAME_VIDEO: c_int = 1;
const FRAME_ERROR: c_int = 4;

type FindCreateFn = unsafe extern "C" fn(*const FindCreate) -> *mut c_void;
type FindDestroy = unsafe extern "C" fn(*mut c_void);
type FindWait = unsafe extern "C" fn(*mut c_void, u32) -> bool;
type FindCurrent = unsafe extern "C" fn(*mut c_void, *mut u32) -> *const SourceT;
type RecvCreateFn = unsafe extern "C" fn(*const RecvCreate) -> *mut c_void;
type RecvDestroy = unsafe extern "C" fn(*mut c_void);
type RecvCapture = unsafe extern "C" fn(*mut c_void, *mut VideoFrame, *mut c_void, *mut c_void, u32) -> c_int;
type RecvFreeVideo = unsafe extern "C" fn(*mut c_void, *const VideoFrame);
type RecvConnections = unsafe extern "C" fn(*mut c_void) -> c_int;

/// The functions that find and receive sources. A runtime without them can still send.
struct Receiving {
    find_create: FindCreateFn,
    find_destroy: FindDestroy,
    find_wait: FindWait,
    find_current: FindCurrent,
    recv_create: RecvCreateFn,
    recv_destroy: RecvDestroy,
    recv_capture: RecvCapture,
    recv_free_video: RecvFreeVideo,
    recv_connections: RecvConnections,
}

type Initialize = unsafe extern "C" fn() -> bool;
type SendCreateFn = unsafe extern "C" fn(*const SendCreate) -> *mut c_void;
type SendDestroy = unsafe extern "C" fn(*mut c_void);
type SendVideo = unsafe extern "C" fn(*mut c_void, *const VideoFrame);
type SendAudio = unsafe extern "C" fn(*mut c_void, *const AudioFrame);
type SendConnections = unsafe extern "C" fn(*mut c_void, u32) -> c_int;

/// The runtime's functions this app uses, with the library they live in kept open for as long
/// as the app runs.
pub struct Runtime {
    _library: Library,
    /// Where the library was opened from.
    path: PathBuf,
    send_create: SendCreateFn,
    send_destroy: SendDestroy,
    send_video: SendVideo,
    /// `NDIlib_send_get_no_connections`, if this runtime has it. Without it a source can't tell
    /// whether anyone is watching, and sends all the time.
    send_connections: Option<SendConnections>,
    /// `NDIlib_send_send_audio_v2`, if this runtime has it. Without it a source is silent.
    send_audio: Option<SendAudio>,
    /// The functions for finding and receiving, if this runtime has them all.
    receiving: Option<Receiving>,
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

/// Where the runtime may be, in the order to try: the folder NDI's own variable names, on Linux
/// the copy the app downloaded, a copy inside the app, where NDI's runtime installer puts it,
/// the SDK's copy, and on a Mac with NDI Tools but not the runtime installer, the copy inside
/// one of NDI's own apps.
fn places() -> Vec<PathBuf> {
    let name = if cfg!(target_os = "macos") {
        "libndi.dylib"
    } else if cfg!(windows) {
        "Processing.NDI.Lib.x64.dll"
    } else {
        "libndi.so.6"
    };
    // NDI 5's runtime has the same functions this app uses (create, send, destroy, the
    // connection count) and the same frame layouts, so it works as well as NDI 6's.
    let name_v5 = if cfg!(target_os = "linux") { "libndi.so.5" } else { name };
    let mut places = Vec::new();
    // Testing aid: behave as on a computer without NDI.
    if std::env::var_os("SOFTPRESENTER_HIDE_NDI").is_some() {
        return places;
    }
    if let Some(folder) = std::env::var_os("NDI_RUNTIME_DIR_V6") {
        places.push(PathBuf::from(folder).join(name));
    }
    if cfg!(target_os = "linux")
        && let Some(folder) = OWN_FOLDER.get()
    {
        places.push(folder.join(name));
    }
    if let Some(folder) = std::env::var_os("NDI_RUNTIME_DIR_V5") {
        places.push(PathBuf::from(folder).join(name_v5));
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
    } else if cfg!(windows) {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            places.extend(windows_runtimes(&PathBuf::from(program_files).join("NDI"), name));
        }
    } else {
        places.push(PathBuf::from("/usr/lib").join(name));
        places.push(PathBuf::from("/usr/local/lib").join(name));
        places.push(PathBuf::from("/usr/lib").join(name_v5));
        places.push(PathBuf::from("/usr/local/lib").join(name_v5));
    }
    places
}

/// The runtime inside each of NDI's folders in `<Program Files>\NDI` ("NDI 6 Runtime\v6",
/// "NDI 6 Tools\Runtime", "NDI 5 Tools\Runtime"…), newest name first, whatever the version is
/// called: the folder names are read, not listed.
fn windows_runtimes(ndi: &std::path::Path, name: &str) -> Vec<PathBuf> {
    let subfolders = |dir: &std::path::Path| -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default();
        found.sort();
        found.reverse();
        found
    };
    let mut found = Vec::new();
    for folder in subfolders(ndi) {
        if !folder.file_name().is_some_and(|n| n.to_string_lossy().starts_with("NDI ")) {
            continue;
        }
        found.push(folder.join("Runtime").join(name));
        // The runtime installer's own folder is "v6" (or "v5") inside.
        for inner in subfolders(&folder) {
            let versioned = inner.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.strip_prefix('v').and_then(|rest| rest.chars().next()).is_some_and(|c| c.is_ascii_digit())
            });
            if versioned {
                found.push(inner.join(name));
            }
        }
        found.push(folder.join(name));
    }
    found
}

#[cfg(test)]
mod places_tests {
    use super::*;

    #[test]
    fn finds_the_runtime_in_every_ndi_folder_newest_first() {
        let root = std::env::temp_dir().join(format!("sp-ndi-places-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["NDI 5 Tools/Runtime", "NDI 6 Runtime/v6", "NDI 6.1 Tools/Runtime", "Other/Runtime"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let found = windows_runtimes(&root, "Processing.NDI.Lib.x64.dll");
        let shown: Vec<String> =
            found.iter().map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")).collect();
        let at = |s: &str| shown.iter().position(|p| p == s).unwrap_or_else(|| panic!("{s} missing in {shown:?}"));
        assert!(
            at("NDI 6.1 Tools/Runtime/Processing.NDI.Lib.x64.dll") < at("NDI 6 Runtime/v6/Processing.NDI.Lib.x64.dll")
        );
        assert!(
            at("NDI 6 Runtime/v6/Processing.NDI.Lib.x64.dll") < at("NDI 5 Tools/Runtime/Processing.NDI.Lib.x64.dll")
        );
        assert!(shown.iter().all(|p| !p.starts_with("Other")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_folder_gives_nothing() {
        assert!(windows_runtimes(std::path::Path::new("/no/such/folder"), "x").is_empty());
    }
}

/// Opens the first runtime found in `places` and starts it.
fn open(places: &[PathBuf]) -> Result<Runtime, String> {
    let Some(path) = places.iter().find(|path| path.is_file()) else {
        return Err(NOT_INSTALLED.to_owned());
    };
    // SAFETY: the file is NDI's runtime, found where NDI installs it, where the operator's
    // NDI_RUNTIME_DIR_V6 points, or on Linux where the app put it, taken unchanged from NDI's
    // own SDK download once the operator accepted NDI's licence; opening it runs only its own
    // set-up, which NDI documents as safe to do on any thread.
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
    // SAFETY: as above: a function the runtime exports, read as its signature in
    // `Processing.NDI.Send.h`. A runtime without it is still used; its sources are silent.
    let send_audio = unsafe { library.get::<SendAudio>(b"NDIlib_send_send_audio_v2\0") }.ok().map(|function| *function);
    // SAFETY: as above: each is a function the runtime exports, read as its signature in
    // `Processing.NDI.Find.h` or `Processing.NDI.Recv.h`. A runtime without one of them still
    // sends; it can't receive.
    let receiving = unsafe {
        (|| {
            Some(Receiving {
                find_create: *library.get::<FindCreateFn>(b"NDIlib_find_create_v2\0").ok()?,
                find_destroy: *library.get::<FindDestroy>(b"NDIlib_find_destroy\0").ok()?,
                find_wait: *library.get::<FindWait>(b"NDIlib_find_wait_for_sources\0").ok()?,
                find_current: *library.get::<FindCurrent>(b"NDIlib_find_get_current_sources\0").ok()?,
                recv_create: *library.get::<RecvCreateFn>(b"NDIlib_recv_create_v3\0").ok()?,
                recv_destroy: *library.get::<RecvDestroy>(b"NDIlib_recv_destroy\0").ok()?,
                recv_capture: *library.get::<RecvCapture>(b"NDIlib_recv_capture_v2\0").ok()?,
                recv_free_video: *library.get::<RecvFreeVideo>(b"NDIlib_recv_free_video_v2\0").ok()?,
                recv_connections: *library.get::<RecvConnections>(b"NDIlib_recv_get_no_connections\0").ok()?,
            })
        })()
    };
    // SAFETY: NDIlib_initialize takes nothing and may be called more than once; the library it
    // lives in is open.
    if !unsafe { initialize() } {
        return Err("NDI can't run on this computer's processor.".to_owned());
    }
    Ok(Runtime {
        _library: library,
        path: path.clone(),
        send_create,
        send_destroy,
        send_video,
        send_connections,
        send_audio,
        receiving,
    })
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

impl Source {
    /// Sends one block of sound: `planar` holds every channel's samples one channel after
    /// another, each `planar.len() / channels` long, at `rate` samples a second. Returns at
    /// once (the source doesn't pace its sound; whoever hands it over does, by the clock).
    /// Its timecode is the time its first sample is heard, `heard`, as NDI counts time: in
    /// 100 ns steps of the computer's clock since 1970, which is what NDI makes the pictures'
    /// timecodes from as they are sent (and, its documentation says, goes on making them
    /// from beside timecodes given for the sound). Given, not left to NDI: NDI's own are made
    /// from when each block is handed over, and stay out by however long the thread handing
    /// them over was once held up. Nothing is sent by a runtime too old to send sound, or
    /// for a block that doesn't divide into its channels.
    pub fn send_sound(&self, rate: u32, channels: u32, planar: &[f32], heard: std::time::SystemTime) {
        let Some(send_audio) = self.runtime.send_audio else { return };
        if channels == 0 || planar.is_empty() || !planar.len().is_multiple_of(channels as usize) {
            return;
        }
        let each = planar.len() / channels as usize;
        let (Ok(rate), Ok(channels), Ok(samples), Ok(stride)) = (
            c_int::try_from(rate),
            c_int::try_from(channels),
            c_int::try_from(each),
            c_int::try_from(each * size_of::<f32>()),
        ) else {
            return;
        };
        let since_1970 = heard.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let frame = AudioFrame {
            sample_rate: rate,
            no_channels: channels,
            no_samples: samples,
            timecode: i64::try_from(since_1970.as_nanos() / 100).unwrap_or(SYNTHESIZE_TIMECODE),
            // NDI's frame holds a mutable pointer, but sending only reads through it.
            data: planar.as_ptr().cast_mut(),
            channel_stride_in_bytes: stride,
            metadata: std::ptr::null(),
            timestamp: 0,
        };
        // SAFETY: the instance is a live source from NDIlib_send_create; `frame` describes
        // `planar` exactly (`channels` runs of `each` floats, `stride` bytes apart, checked
        // above to fill it), and the send has copied the samples into NDI's own buffers when
        // it returns, only reading them (see `send` for why the pointer isn't `const`). It may
        // be called while another thread sends a picture on the same instance: see the `Sync`
        // note below.
        unsafe { send_audio(self.instance, &frame) };
    }
}

// SAFETY: a source is a handle to an NDI sender, which NDI's SDK documentation says may be sent
// "audio, video, or metadata frames […] at any time, off any thread, and in any order"
// (Sending, after NDIlib_send_create), and whose connection count is likewise asked from any
// thread. Nothing else is kept in it but the runtime (shared, never changed once opened) and
// its name (never changed). So it may move to another thread, and two threads may use it at
// once: one sending its pictures, another its sound. It is destroyed once, when the last of
// them lets it go.
unsafe impl Send for Source {}
// SAFETY: as above.
unsafe impl Sync for Source {}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: the instance came from NDIlib_send_create and is destroyed only here, once.
        unsafe { (self.runtime.send_destroy)(self.instance) };
    }
}

impl Runtime {
    fn receiving(&self) -> Result<&Receiving, String> {
        self.receiving.as_ref().ok_or_else(|| "The NDI runtime is too old to receive from other sources.".to_owned())
    }
}

/// Looks for sources on the network. Dropping it stops looking.
pub struct Finder {
    runtime: &'static Runtime,
    instance: *mut c_void,
}

impl Finder {
    pub fn new(runtime: &'static Runtime) -> Result<Finder, String> {
        let receiving = runtime.receiving()?;
        let settings = FindCreate { show_local_sources: true, groups: std::ptr::null(), extra_ips: std::ptr::null() };
        // SAFETY: `settings` is a valid NDIlib_find_create_t for the length of the call; the
        // groups and extra addresses may be null.
        let instance = unsafe { (receiving.find_create)(&settings) };
        if instance.is_null() {
            return Err("NDI could not start looking for sources.".to_owned());
        }
        Ok(Finder { runtime, instance })
    }

    /// Waits up to `ms` for the list to change; true when it has.
    pub fn wait(&self, ms: u32) -> bool {
        let Ok(receiving) = self.runtime.receiving() else { return false };
        // SAFETY: the instance is a live finder from NDIlib_find_create_v2.
        unsafe { (receiving.find_wait)(self.instance, ms) }
    }

    /// The names of the sources found now (`"COMPUTER (Source)"`).
    pub fn sources(&self) -> Vec<String> {
        let Ok(receiving) = self.runtime.receiving() else { return Vec::new() };
        let mut count: u32 = 0;
        // SAFETY: the instance is a live finder; `count` is written. The list it returns stays
        // valid until the next call on this finder, and is only read here, before any other.
        let list = unsafe { (receiving.find_current)(self.instance, &mut count) };
        if list.is_null() {
            return Vec::new();
        }
        (0..count as usize)
            .filter_map(|at| {
                // SAFETY: `list` holds `count` sources (see above); each name, when set, is a
                // NUL-terminated string NDI owns for as long as the list stays valid.
                let name = unsafe { (*list.add(at)).name };
                if name.is_null() {
                    return None;
                }
                // SAFETY: `name` is non-null, so a NUL-terminated string (as above).
                Some(unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned())
            })
            .collect()
    }
}

impl Drop for Finder {
    fn drop(&mut self) {
        if let Ok(receiving) = self.runtime.receiving() {
            // SAFETY: the instance came from NDIlib_find_create_v2 and is destroyed only here.
            unsafe { (receiving.find_destroy)(self.instance) };
        }
    }
}

/// One picture received, valid only inside the call that is handed it.
pub struct VideoRef<'a> {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: usize,
    /// Rows of red, green, blue and a fourth byte: the alpha when `alpha`, else to be ignored.
    pub rows: &'a [u8],
    pub alpha: bool,
}

/// What a wait for the next thing from a source brought.
pub enum Captured {
    Video,
    Nothing,
    Error,
}

/// Receives one source's pictures. Dropping it lets go of the connection.
pub struct Receiver {
    runtime: &'static Runtime,
    instance: *mut c_void,
    _name: CString,
}

impl Receiver {
    /// Connects to a source by its name, for the picture only: the full stream, or `light`.
    pub fn new(runtime: &'static Runtime, source: &str, light: bool) -> Result<Receiver, String> {
        let receiving = runtime.receiving()?;
        let name = CString::new(source).map_err(|_| "An NDI name can't contain a zero byte.".to_owned())?;
        let settings = RecvCreate {
            source: SourceT { name: name.as_ptr(), url: std::ptr::null() },
            color_format: COLOR_RGBX_RGBA,
            bandwidth: if light { BANDWIDTH_LIGHT } else { BANDWIDTH_FULL },
            allow_video_fields: false,
            name: std::ptr::null(),
        };
        // SAFETY: `settings` is a valid NDIlib_recv_create_v3_t for the call; the source's name
        // is a NUL-terminated string (kept in `_name`); its address and the receiver's own name
        // may be null.
        let instance = unsafe { (receiving.recv_create)(&settings) };
        if instance.is_null() {
            return Err("NDI could not connect to the source.".to_owned());
        }
        Ok(Receiver { runtime, instance, _name: name })
    }

    /// Waits up to `ms` for the next picture, and lends it to `with`.
    pub fn capture(&self, ms: u32, with: impl FnOnce(&VideoRef<'_>)) -> Captured {
        let Ok(receiving) = self.runtime.receiving() else { return Captured::Error };
        let mut frame = VideoFrame {
            xres: 0,
            yres: 0,
            four_cc: 0,
            frame_rate_n: 0,
            frame_rate_d: 0,
            picture_aspect_ratio: 0.0,
            frame_format_type: 0,
            timecode: 0,
            data: std::ptr::null_mut(),
            line_stride_in_bytes: 0,
            metadata: std::ptr::null(),
            timestamp: 0,
        };
        // SAFETY: the instance is a live receiver; `frame` is a valid NDIlib_video_frame_v2_t
        // for NDI to fill; audio and metadata are not asked for (null).
        let kind = unsafe {
            (receiving.recv_capture)(self.instance, &mut frame, std::ptr::null_mut(), std::ptr::null_mut(), ms)
        };
        match kind {
            FRAME_VIDEO => {
                if let (Ok(width), Ok(height), Ok(stride)) =
                    (u32::try_from(frame.xres), u32::try_from(frame.yres), usize::try_from(frame.line_stride_in_bytes))
                    && !frame.data.is_null()
                    && width > 0
                    && height > 0
                    && stride >= width as usize * 4
                    && matches!(frame.four_cc, FOUR_CC_RGBX | FOUR_CC_RGBA)
                {
                    // The last row needs only its pixels, not the padding after them.
                    let len = stride * (height as usize - 1) + width as usize * 4;
                    // SAFETY: NDI says `data` holds `height` rows `stride` bytes apart, each
                    // with `width` four-byte pixels (checked above to fit within them); it
                    // stays valid until the frame is freed just below, after `with` is done.
                    let rows = unsafe { std::slice::from_raw_parts(frame.data, len) };
                    with(&VideoRef {
                        width,
                        height,
                        bytes_per_row: stride,
                        rows,
                        alpha: frame.four_cc == FOUR_CC_RGBA,
                    });
                }
                // SAFETY: the frame was filled by this receiver's capture and is freed once.
                unsafe { (receiving.recv_free_video)(self.instance, &frame) };
                Captured::Video
            }
            FRAME_ERROR => Captured::Error,
            _ => Captured::Nothing,
        }
    }

    /// How many senders this receiver is connected to (0 or 1): 0 while the source is away.
    pub fn connections(&self) -> i32 {
        let Ok(receiving) = self.runtime.receiving() else { return 0 };
        // SAFETY: the instance is a live receiver; the call only reads a count.
        unsafe { (receiving.recv_connections)(self.instance) }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        if let Ok(receiving) = self.runtime.receiving() {
            // SAFETY: the instance came from NDIlib_recv_create_v3 and is destroyed only here.
            unsafe { (receiving.recv_destroy)(self.instance) };
        }
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
        assert_eq!(problem, NOT_INSTALLED);
        // NDI Tools on a Mac and on Windows; on Linux, where NDI Tools doesn't exist, the app's
        // own download.
        let elsewhere = not_installed(false);
        assert!(elsewhere.contains("isn't installed") && elsewhere.contains("https://ndi.video/"));
        let linux = not_installed(true);
        assert!(linux.contains("isn't installed") && linux.contains("Download NDI\u{2026}"));
        assert!(!linux.contains("NDI Tools"));
    }

    #[test]
    fn the_rgbx_and_rgba_codes_are_ndis() {
        // NDI_LIB_FOURCC('R','G','B','X'): the first letter in the lowest byte.
        let code = |last: u8| u32::from(b'R') | u32::from(b'G') << 8 | u32::from(b'B') << 16 | u32::from(last) << 24;
        assert_eq!(FOUR_CC_RGBX, code(b'X'));
        assert_eq!(FOUR_CC_RGBA, code(b'A'));
    }
}
