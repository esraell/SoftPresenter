//! Videos the system's player can't play (MKV, WebM, AVI and the like, or a codec the computer
//! lacks), played with FFmpeg run as a separate program: one copy decodes the pictures, another
//! the sound. Nothing is converted first, so a video brought in at the last minute plays at
//! once. The pictures are paced here, by a clock that the sound starts, so they keep in step;
//! pausing simply stops reading, and FFmpeg waits on its full pipe. Only a seek starts FFmpeg
//! again. Where the codec allows, FFmpeg decodes on the graphics chip: the Mac's video hardware,
//! or Direct3D 11's video decoder on Windows (see [`HARDWARE`]).
//!
//! The video behind the slide comes as the decoder's own two planes (NV12), which the GPU turns
//! into colours, where the video allows (see [`colours_of`]); everything else comes as RGBA
//! pixels FFmpeg converts on the processor. Planes are three eighths of RGBA's bytes, and the
//! decoder makes them anyway: with the graphics chip decoding, FFmpeg does nothing to them at
//! all but hand them over.

use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{Colours, Frame, Lending, Matrix, Planes};

/// The largest picture asked of FFmpeg: the screens are 1080p.
const MAX_W: u32 = 1920;
const MAX_H: u32 = 1080;
/// The sound is asked for as stereo at 48 kHz, as 32-bit floats.
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// At most half a second of sound waits to be played; FFmpeg is held back beyond that.
const SOUND_AHEAD: usize = RATE as usize * CHANNELS as usize / 2;

/// The graphics chip's video decoder FFmpeg is asked to use (`-hwaccel`): the Mac's video
/// hardware, or Direct3D 11's decoder on Windows (DXVA2, the older one, can't make its device
/// outside the desktop session, as for a program started by a service). FFmpeg decodes with
/// its own decoder where the chip has none for the codec, but stops if the decoder can't be set
/// up at all (no graphics driver, say): a still that gets nothing that way, or a run that stops
/// with an error before its first picture, is asked for again without it.
const HARDWARE: Option<&str> = if cfg!(target_os = "macos") {
    Some("videotoolbox")
} else if cfg!(windows) {
    Some("d3d11va")
} else {
    None
};

/// Where an FFmpeg program is: in the app, then where Homebrew and others put it. An app opened
/// from the Finder isn't given the shell's search path, so the places are named in full.
pub fn tool(name: &str) -> Option<PathBuf> {
    places(name).into_iter().find(|place| place.is_file())
}

/// The folder the app keeps its own FFmpeg in, when it was told one (a test copy's own, see
/// [`fetched_folder`]).
static FETCHED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The app has checked that the folder holds its own FFmpeg (see [`set_fetched_checked`]).
static CHECKED: AtomicBool = AtomicBool::new(false);

/// Tells the lookup where the app keeps its own FFmpeg, in place of the usual folder: a test
/// copy keeps it in its own data folder. The first one set stays.
pub fn set_fetched_folder(folder: PathBuf) {
    let _ = FETCHED.set(folder);
}

/// Tells the lookup that the app has checked the folder holds its own FFmpeg (its SHA-256s), so
/// it is looked in first, before the program's own folder: the app then runs that copy wherever
/// the program was started from.
pub fn set_fetched_checked(checked: bool) {
    CHECKED.store(checked, Ordering::Relaxed);
}

/// The folder the app keeps its own FFmpeg in on Windows, copied there from beside the program
/// or fetched (the app's `ffmpeg_fetch`): `%LOCALAPPDATA%\SoftPresenter\ffmpeg`, unless the app
/// named another; elsewhere only one the app named.
pub fn fetched_folder() -> Option<PathBuf> {
    if let Some(folder) = FETCHED.get() {
        return Some(folder.clone());
    }
    if !cfg!(windows) {
        return None;
    }
    std::env::var_os("LOCALAPPDATA")
        .filter(|local| !local.is_empty())
        .map(|local| PathBuf::from(local).join("SoftPresenter").join("ffmpeg"))
}

/// The places an FFmpeg program is looked for, in order. First beside the app's own program (and
/// in a Mac app's Resources), where the app's own FFmpeg is; but once the app has checked that
/// the folder it keeps its own FFmpeg in ([`fetched_folder`], on Windows
/// `%LOCALAPPDATA%\SoftPresenter\ffmpeg`) holds it, that folder comes first. Then, on Windows,
/// the folders on the search path: a tester's copy was started on its own from the Desktop,
/// without the app's FFmpeg beside it, and a video it played said FFmpeg wasn't on the computer.
/// Elsewhere, where Homebrew and the system put it.
pub fn places(name: &str) -> Vec<PathBuf> {
    places_with(name, fetched_folder().as_deref(), CHECKED.load(Ordering::Relaxed))
}

/// [`places`], with `fetched` as the folder the app keeps its own FFmpeg in (none: no such
/// folder), checked to hold it or not. What is kept there is always the Windows build, so the
/// programs there are looked for by their Windows names on any system (a test fetches on any
/// system).
pub fn places_with(name: &str, fetched: Option<&Path>, checked: bool) -> Vec<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    let kept = fetched.map(|folder| folder.join(format!("{name}.exe")));
    let mut places: Vec<PathBuf> = Vec::new();
    if checked {
        places.extend(kept.clone());
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        places.push(dir.join(&file));
        places.push(dir.join("../Resources").join(&file));
    }
    if !checked {
        places.extend(kept);
    }
    if cfg!(windows) {
        if let Some(path) = std::env::var_os("PATH") {
            places.extend(std::env::split_paths(&path).filter(|dir| dir.is_absolute()).map(|dir| dir.join(&file)));
        }
    } else {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
            places.push(Path::new(dir).join(&file));
        }
    }
    places
}

fn shown_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn missing(path: &Path) -> String {
    format!(
        "{}: this video needs FFmpeg to play, and FFmpeg isn't on this computer. MP4 and MOV videos play without it.",
        shown_name(path)
    )
}

/// What FFmpeg says a video is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    /// Pictures a second, between 1 and 60.
    pub fps: f64,
    pub duration: Option<f64>,
    pub sound: bool,
    /// How its pictures stand for colours, when they can go to the GPU as their own planes
    /// (see [`colours_of`]).
    pub colours: Option<Colours>,
}

/// How a video's pictures stand for colours, when they can go to the GPU as their own planes:
/// pictures in 4:2:0 YUV with no alpha (as nearly every video is: H.264, HEVC, VP9, AV1 and the
/// rest, 8-bit or deeper, which FFmpeg brings down to 8 bits), in a standard FFmpeg's converter
/// has numbers for. Its standard is the one the stream names, or BT.601 where it names none,
/// as FFmpeg's converter takes it then; its range is the whole of each byte only where the
/// stream says so (or its pixel format is the JPEG one), limited otherwise, as FFmpeg takes it.
///
/// `None` for anything else: pictures with clear parts or of RGB pixels, colour kept for every
/// pixel or every two (which planes of a quarter would lose), or a standard of another kind.
/// Those come as RGBA, converted by FFmpeg, as before.
fn colours_of(video: &serde_json::Value) -> Option<Colours> {
    let format = video["pix_fmt"].as_str()?;
    let quarter = ["yuv420p", "yuvj420p", "nv12", "nv21", "p010", "p012", "p016"];
    if !quarter.iter().any(|kind| format.starts_with(kind)) {
        return None;
    }
    let matrix = match video["color_space"].as_str().unwrap_or("unknown") {
        "bt709" => Matrix::Bt709,
        "fcc" => Matrix::Fcc,
        "smpte240m" => Matrix::Smpte240,
        "bt2020nc" | "bt2020c" => Matrix::Bt2020,
        "bt470bg" | "smpte170m" | "unknown" | "unspecified" | "reserved" => Matrix::Bt601,
        _ => return None,
    };
    let full_range = format.starts_with("yuvj") || matches!(video["color_range"].as_str(), Some("pc" | "jpeg"));
    Some(Colours { matrix, full_range })
}

/// A rate written as FFmpeg writes it, "30000/1001", as a number.
fn rate(text: &str) -> Option<f64> {
    let (top, bottom) = text.split_once('/')?;
    let (top, bottom) = (top.parse::<f64>().ok()?, bottom.parse::<f64>().ok()?);
    (bottom > 0.0 && top > 0.0).then(|| top / bottom)
}

/// Reads what ffprobe says of a video, as JSON.
fn info_from(json: &serde_json::Value) -> Option<Info> {
    let streams = json["streams"].as_array()?;
    let video =
        streams.iter().find(|stream| stream["codec_type"] == "video" && stream["disposition"]["attached_pic"] != 1)?;
    let width = u32::try_from(video["width"].as_u64()?).ok()?;
    let height = u32::try_from(video["height"].as_u64()?).ok()?;
    let fps = video["avg_frame_rate"]
        .as_str()
        .and_then(rate)
        .or_else(|| video["r_frame_rate"].as_str().and_then(rate))
        .unwrap_or(30.0)
        .clamp(1.0, 60.0);
    let duration = json["format"]["duration"].as_str().and_then(|text| text.parse::<f64>().ok());
    let sound = streams.iter().any(|stream| stream["codec_type"] == "audio");
    let colours = colours_of(video);
    (width > 0 && height > 0).then_some(Info { width, height, fps, duration, sound, colours })
}

/// FFmpeg or ffprobe, to be started without a console window. SoftPresenter is a window
/// program on Windows, and a console program it starts would otherwise open a black window of
/// its own on the operator's screen, every time a video plays.
fn quiet(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// What a video is: its size, frame rate, length and whether it has sound.
pub fn probe(path: &Path) -> Result<Info, String> {
    let ffprobe = tool("ffprobe").ok_or_else(|| missing(path))?;
    let out = quiet(ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: FFmpeg could not be started ({e}).", shown_name(path)))?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    info_from(&json).ok_or_else(|| format!("{}: not a video SoftPresenter can open.", shown_name(path)))
}

/// FFmpeg asked to decode a file's sound from `from` seconds (going round and round with
/// `looping`): stereo 32-bit floats at `rate` samples a second, to its output. The video
/// player's sound and the audio player's tracks are both decoded this way.
fn sound_command(ffmpeg: &Path, path: &Path, from: f64, looping: bool, rate: u32) -> Command {
    let mut command = quiet(ffmpeg);
    command.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);
    if looping {
        command.args(["-stream_loop", "-1"]);
    }
    command.args(["-ss", &format!("{from:.3}"), "-i"]).arg(path).stdin(Stdio::null());
    command
        .args(["-vn", "-sn", "-ac", &CHANNELS.to_string(), "-ar", &rate.to_string(), "-f", "f32le", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

/// A file's sound as FFmpeg decodes it: stereo 32-bit floats, read off its output a piece at a
/// time, whole frames only.
pub struct SoundReader {
    out: std::process::ChildStdout,
    bytes: Box<[u8]>,
    carry: Vec<u8>,
}

/// The bytes of one stereo frame of 32-bit floats.
const FRAME_BYTES: usize = 4 * CHANNELS as usize;

impl SoundReader {
    fn new(out: std::process::ChildStdout) -> SoundReader {
        SoundReader { out, bytes: vec![0u8; 16384].into_boxed_slice(), carry: Vec::new() }
    }

    /// Reads what FFmpeg has decoded next onto the end of `samples`, left and right side by
    /// side: how many samples were added, 0 once its sound has all been read. Waits for FFmpeg.
    pub fn read(&mut self, samples: &mut Vec<f32>) -> std::io::Result<usize> {
        loop {
            let read = self.out.read(&mut self.bytes)?;
            if read == 0 {
                return Ok(0);
            }
            self.carry.extend_from_slice(&self.bytes[..read]);
            let whole = self.carry.len() / FRAME_BYTES * FRAME_BYTES;
            if whole == 0 {
                continue;
            }
            let before = samples.len();
            samples.extend(self.carry[..whole].as_chunks::<4>().0.iter().map(|four| f32::from_le_bytes(*four)));
            self.carry.drain(..whole);
            return Ok(samples.len() - before);
        }
    }
}

/// Starts FFmpeg decoding a file's sound from `from` seconds, stereo at `rate`: the program,
/// to end when the sound is no longer wanted (which ends a read waiting on it), and its sound.
pub fn decode_sound(path: &Path, from: f64, rate: u32) -> Result<(Child, SoundReader), String> {
    let ffmpeg = tool("ffmpeg").ok_or_else(|| "FFmpeg isn't on this computer.".to_owned())?;
    let mut child = sound_command(&ffmpeg, path, from.max(0.0), false, rate)
        .spawn()
        .map_err(|e| format!("FFmpeg could not be started: {e}"))?;
    let out = child.stdout.take().ok_or_else(|| "FFmpeg could not be started.".to_owned())?;
    Ok((child, SoundReader::new(out)))
}

/// What FFmpeg says a sound file is.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundInfo {
    /// Seconds, when known.
    pub duration: Option<f64>,
    /// FFmpeg's name for how the sound is coded ("alac", "opus", "wmav2"…).
    pub codec: String,
    pub channels: u32,
    pub rate: u32,
}

/// Why FFmpeg can't play a sound file.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundProblem {
    /// FFmpeg isn't on this computer.
    NoFfmpeg,
    /// It is locked to the shop it came from (Apple Music and the iTunes Store's FairPlay).
    Protected,
    /// It has no sound in it (a video with none, say).
    NoSound,
    /// FFmpeg can't read it: what FFmpeg says, or how it fell short.
    Unreadable(String),
}

/// What FFmpeg (its `ffprobe`) says a sound file is, or why it can't be played.
pub fn sound_info(path: &Path) -> Result<SoundInfo, SoundProblem> {
    let ffprobe = tool("ffprobe").ok_or(SoundProblem::NoFfmpeg)?;
    let out = quiet(ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| SoundProblem::Unreadable(format!("FFmpeg could not be started ({e})")))?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    // What it said, less the file's name it begins with.
    let said = String::from_utf8_lossy(&out.stderr);
    let said = said.lines().rev().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    let said = said.rsplit(": ").next().unwrap_or(said).to_owned();
    sound_info_from(&json, &said)
}

/// Reads what ffprobe says of a sound file, as JSON, and the last thing it said wrong.
fn sound_info_from(json: &serde_json::Value, said: &str) -> Result<SoundInfo, SoundProblem> {
    let streams = json["streams"].as_array().map(Vec::as_slice).unwrap_or_default();
    // Sound locked to its shop: Apple's FairPlay ("drms"; "drmi" for a video's), or common
    // encryption's ("enca").
    let locked = ["drms", "drmi", "enca", "drmz"];
    if streams.iter().any(|stream| stream["codec_tag_string"].as_str().is_some_and(|tag| locked.contains(&tag))) {
        return Err(SoundProblem::Protected);
    }
    let Some(sound) = streams.iter().find(|stream| stream["codec_type"] == "audio") else {
        return Err(if streams.is_empty() {
            SoundProblem::Unreadable(if said.is_empty() {
                "not a kind of file FFmpeg knows".to_owned()
            } else {
                said.to_owned()
            })
        } else {
            SoundProblem::NoSound
        });
    };
    let codec = sound["codec_name"].as_str().unwrap_or("").to_owned();
    if codec.is_empty() || codec == "none" {
        let tag = sound["codec_tag_string"].as_str().unwrap_or("?");
        return Err(SoundProblem::Unreadable(format!("FFmpeg has no decoder for its sound ({tag})")));
    }
    let seconds = |value: &serde_json::Value| value.as_str().and_then(|text| text.parse::<f64>().ok());
    let duration = seconds(&sound["duration"])
        .or_else(|| seconds(&json["format"]["duration"]))
        .filter(|s| s.is_finite() && *s > 0.0);
    let number = |value: &serde_json::Value| {
        value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok())).unwrap_or(0) as u32
    };
    Ok(SoundInfo { duration, codec, channels: number(&sound["channels"]), rate: number(&sound["sample_rate"]) })
}

/// The size pictures are asked for in: within a box, keeping the shape, with even sides as
/// the pixel formats need.
fn fitted(info: &Info, max_w: u32, max_h: u32) -> (u32, u32) {
    let (w, h) = (f64::from(info.width), f64::from(info.height));
    let scale = (f64::from(max_w) / w).min(f64::from(max_h) / h).min(1.0);
    let even = |side: f64| ((side.round() as u32).max(2)) & !1;
    (even(w * scale), even(h * scale))
}

/// A still from a video, about `at` seconds in (sooner in a short video), no larger than
/// `max_side` on its longer side.
pub fn still(path: &Path, max_side: u32, at: f64) -> Result<Frame, String> {
    let info = probe(path)?;
    let ffmpeg = tool("ffmpeg").ok_or_else(|| missing(path))?;
    let (w, h) = fitted(&info, max_side, max_side);
    let at = info.duration.map_or(0.0, |length| at.min(length * 0.5));
    // As for playing: the graphics chip's decoder where the codec allows (and FFmpeg's own
    // decoder where it doesn't). AV1, which FFmpeg alone decodes only through the hardware,
    // needs it. One that can't be set up gets nothing, and the picture is asked for again
    // without it.
    let read = |hardware: Option<&str>| {
        let mut command = quiet(&ffmpeg);
        command.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);
        if let Some(decoder) = hardware {
            command.args(["-hwaccel", decoder]);
        }
        command
            .args(["-ss", &format!("{at:.3}"), "-i"])
            .arg(path)
            .args(["-frames:v", "1", "-an", "-sn", "-vf", &format!("scale={w}:{h},setsar=1")])
            .args(["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("{}: FFmpeg could not be started ({e}).", shown_name(path)))
    };
    let wanted = (w as usize) * (h as usize) * 4;
    let mut out = read(HARDWARE)?;
    if out.stdout.len() != wanted && HARDWARE.is_some() {
        out = read(None)?;
    }
    if out.stdout.len() != wanted {
        return Err(format!("{}: FFmpeg could not read a picture from this video.", shown_name(path)));
    }
    Ok(Frame { width: w, height: h, rgba: out.stdout })
}

/// The time a run of the video has played for: started by the first sound played (or, with no
/// sound, the first picture ready), stopped while paused.
#[derive(Default)]
struct Clock {
    state: Mutex<ClockState>,
}

#[derive(Default)]
struct ClockState {
    started: Option<Instant>,
    paused: bool,
    paused_at: Option<Instant>,
    paused_for: Duration,
}

impl Clock {
    fn start(&self) {
        if let Ok(mut state) = self.state.lock()
            && state.started.is_none()
        {
            let now = Instant::now();
            state.started = Some(now);
            if state.paused {
                state.paused_at = Some(now);
            }
        }
    }

    /// True while paused.
    fn paused(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.paused)
    }

    /// Seconds played, or `None` before it starts.
    fn elapsed(&self) -> Option<f64> {
        let state = self.state.lock().ok()?;
        let started = state.started?;
        let now = state.paused_at.unwrap_or_else(Instant::now);
        Some(now.saturating_duration_since(started).saturating_sub(state.paused_for).as_secs_f64())
    }

    fn set_paused(&self, paused: bool) {
        let Ok(mut state) = self.state.lock() else { return };
        if state.paused == paused {
            return;
        }
        state.paused = paused;
        if paused {
            if state.started.is_some() {
                state.paused_at = Some(Instant::now());
            }
        } else if let Some(at) = state.paused_at.take() {
            state.paused_for += at.elapsed();
        }
    }
}

/// Sound decoded and waiting to be played, shared by the thread that reads it from FFmpeg and
/// the source the sound output plays.
struct Sound {
    waiting: Mutex<VecDeque<f32>>,
    room: Condvar,
    done: AtomicBool,
    clock: Arc<Clock>,
}

/// What the sound output plays: the decoded sound, as it comes. It starts the clock with its
/// first sample, and plays silence for a moment if FFmpeg falls behind.
struct SoundSource {
    sound: Arc<Sound>,
    taken: VecDeque<f32>,
    started: bool,
    /// Samples played, for the sync trace.
    played: u64,
}

/// Testing aid: SOFTPRESENTER_TRACE_SYNC prints, once a second, how far the sound and the
/// pictures have got against the clock, to check they keep in step.
pub(crate) fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SOFTPRESENTER_TRACE_SYNC").is_some())
}

impl Iterator for SoundSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.taken.is_empty()
            && let Ok(mut waiting) = self.sound.waiting.lock()
        {
            let take = waiting.len().min(4096);
            self.taken.extend(waiting.drain(..take));
            self.sound.room.notify_one();
        }
        match self.taken.pop_front() {
            Some(sample) => {
                if !self.started {
                    self.started = true;
                    self.sound.clock.start();
                }
                self.played += 1;
                let per_second = u64::from(RATE) * u64::from(CHANNELS);
                if tracing() && self.played.is_multiple_of(per_second) {
                    let clock = self.sound.clock.elapsed().unwrap_or(0.0);
                    eprintln!("sync sound {:.3}s clock {clock:.3}s", self.played as f64 / per_second as f64);
                }
                Some(sample)
            }
            None if self.sound.done.load(Ordering::Relaxed) => None,
            None => Some(0.0),
        }
    }
}

impl rodio::Source for SoundSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        CHANNELS
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Where a playing video's newest picture waits for the screens. It can be sent to another
/// thread.
#[derive(Clone)]
pub struct Frames {
    slot: Arc<Mutex<Slot>>,
    pace: Arc<crate::pace::Pace>,
}

/// A picture as FFmpeg gave it: RGBA pixels, or (with its colours) the two planes; its time in
/// the video, and when it was due on screen.
struct Decoded {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
    planes: Option<Colours>,
    at: f64,
    due: Instant,
}

/// The newest picture, waiting to be taken, and the room of one already shown, to fill again.
#[derive(Default)]
struct Slot {
    frame: Option<Decoded>,
    spare: Option<Vec<u8>>,
}

impl Slot {
    /// Puts the newest picture in, keeping the room of one never taken to fill again.
    fn put(&mut self, frame: Decoded) {
        if let Some(missed) = self.frame.replace(frame) {
            self.spare = Some(missed.bytes);
        }
    }

    /// Room for a picture of `len` bytes: a picture's that has been shown, or new.
    fn room(&mut self, len: usize) -> Vec<u8> {
        self.spare.take().filter(|spare| spare.len() == len).unwrap_or_else(|| vec![0u8; len])
    }
}

impl Frames {
    /// The newest picture, if one has come since the last was taken, as RGBA pixels. Pictures
    /// that come as planes (a player opened to lend them) are only lent, by
    /// [`Frames::next_with`]: this takes them and gives nothing.
    pub fn next(&self) -> Option<Frame> {
        let decoded = self.slot.lock().ok()?.frame.take()?;
        self.pace.taken(decoded.at, decoded.due);
        decoded.planes.is_none().then_some(Frame { width: decoded.width, height: decoded.height, rgba: decoded.bytes })
    }

    /// Lends the newest picture, as [`crate::Frames::next_with`] says; its room is then filled
    /// with a later picture, not let go.
    pub fn next_with<R>(&self, show: impl FnOnce(Lending<'_>) -> R) -> Option<R> {
        let decoded = self.slot.lock().ok()?.frame.take()?;
        self.pace.taken(decoded.at, decoded.due);
        let (width, height) = (decoded.width, decoded.height);
        let made = match decoded.planes {
            None => show(Lending::Rows(&crate::Lent::rgba(width, height, &decoded.bytes))),
            Some(colours) => {
                let (luma, chroma) = decoded.bytes.split_at_checked(width as usize * height as usize)?;
                show(Lending::Planes(&Planes { width, height, luma, chroma, colours }))
            }
        };
        if let Ok(mut slot) = self.slot.lock() {
            slot.spare = Some(decoded.bytes);
        }
        Some(made)
    }
}

/// Testing aid: SOFTPRESENTER_VIDEO_RGBA=1 has FFmpeg give the video behind the slide as RGBA
/// pixels it converts on the processor, as before, rather than as planes the GPU turns into
/// colours: to compare the two ways, or to go back to the old one with no new build.
fn rgba_forced() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| std::env::var_os("SOFTPRESENTER_VIDEO_RGBA").is_some_and(|value| value != "0"))
}

/// One run of FFmpeg from a point in the video: its programs and the threads reading them.
struct Run {
    from: f64,
    clock: Arc<Clock>,
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    /// A looping video's pictures ran out: FFmpeg could not start the file again by itself
    /// (it can't for one it can't seek in, such as many FLV files), so the player does.
    ran_out: Arc<AtomicBool>,
    /// FFmpeg stopped before its first picture, asked to decode on the graphics chip: most
    /// likely the chip's decoder couldn't be set up, so the player asks again without it.
    hardware_failed: Arc<AtomicBool>,
    problem: Arc<Mutex<Option<String>>>,
    children: Vec<Child>,
    threads: Vec<JoinHandle<()>>,
    sink: Option<rodio::Sink>,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        // Ending the programs closes their pipes, which ends the threads reading them.
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// A video playing through FFmpeg, with its sound.
pub struct Player {
    path: PathBuf,
    info: Info,
    size: (u32, u32),
    /// The colours of its pictures when they come as planes; `None` when they come as RGBA.
    planes: Option<Colours>,
    looping: bool,
    paused: bool,
    volume: f32,
    /// The graphics chip's decoder FFmpeg is asked to use: [`HARDWARE`], until it has failed
    /// for this video.
    hardware: Option<&'static str>,
    /// Something to say in the log once, such as that the graphics chip couldn't decode it.
    note: Option<String>,
    slot: Arc<Mutex<Slot>>,
    /// How its pictures reach the screens (see [`crate::pace`]).
    pace: Arc<crate::pace::Pace>,
    /// The sound output, tapped for the screens that send sound (see [`crate::sound`]).
    stream: Option<crate::sound::Output>,
    run: Option<Run>,
}

impl Player {
    /// Opens a video, playing from its start, its pictures as planes where `planes` asks for
    /// them and the video allows (see [`colours_of`]). The error is a sentence for the operator.
    pub fn open(path: &Path, planes: bool, pace: Arc<crate::pace::Pace>) -> Result<Player, String> {
        tool("ffmpeg").ok_or_else(|| missing(path))?;
        let info = probe(path)?;
        let stream = if info.sound { crate::sound::Output::open().ok() } else { None };
        let planes = info.colours.filter(|_| planes && !rgba_forced());
        // Planes take any size, odd sides too; one that fits is asked for as it is, so FFmpeg
        // has nothing to scale.
        let size = match planes {
            Some(_) if info.width <= MAX_W && info.height <= MAX_H => (info.width, info.height),
            _ => fitted(&info, MAX_W, MAX_H),
        };
        let mut player = Player {
            path: path.to_owned(),
            info,
            size,
            planes,
            looping: false,
            paused: true,
            volume: 1.0,
            hardware: HARDWARE,
            note: None,
            slot: Arc::default(),
            pace,
            stream,
            run: None,
        };
        player.start_at(0.0)?;
        Ok(player)
    }

    /// Its pictures a second, as the file says (0 when it doesn't).
    pub fn fps(&self) -> f64 {
        self.info.fps
    }

    /// Starts FFmpeg from a point, in place of any run before.
    fn start_at(&mut self, from: f64) -> Result<(), String> {
        self.run = None;
        self.pace.break_run();
        let ffmpeg = tool("ffmpeg").ok_or_else(|| missing(&self.path))?;
        let from = self.info.duration.map_or(from, |length| from.clamp(0.0, (length - 0.05).max(0.0)));
        let clock = Arc::new(Clock::default());
        clock.set_paused(self.paused);
        let stop = Arc::new(AtomicBool::new(false));
        let ended = Arc::new(AtomicBool::new(false));
        let ran_out = Arc::new(AtomicBool::new(false));
        let hardware_failed = Arc::new(AtomicBool::new(false));
        let problem = Arc::new(Mutex::new(None));
        let mut run = Run {
            from,
            clock: clock.clone(),
            stop: stop.clone(),
            ended: ended.clone(),
            ran_out: ran_out.clone(),
            hardware_failed: hardware_failed.clone(),
            problem: problem.clone(),
            children: Vec::new(),
            threads: Vec::new(),
            sink: None,
        };
        // The arguments both programs share: where to start, whether to loop, the file.
        let start = |command: &mut Command, looping: bool| {
            command.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);
            if looping {
                command.args(["-stream_loop", "-1"]);
            }
            command.args(["-ss", &format!("{from:.3}"), "-i"]).arg(&self.path).stdin(Stdio::null());
        };

        // The pictures: a fixed rate and size, so each picture's time and length are known.
        // Planes are asked for in the range their colours are taken in: the one the video says,
        // so FFmpeg has nothing to convert (a picture in another is converted to it).
        let (w, h) = self.size;
        let (scale, format, frame_len) = match self.planes {
            Some(colours) => {
                let range = if colours.full_range { "pc" } else { "tv" };
                (format!("scale={w}:{h}:out_range={range}"), "nv12", Planes::bytes(w, h))
            }
            None => (format!("scale={w}:{h}"), "rgba", (w as usize) * (h as usize) * 4),
        };
        let fps = self.info.fps;
        let mut pictures = quiet(&ffmpeg);
        if let Some(decoder) = self.hardware {
            pictures.args(["-hwaccel", decoder]);
        }
        // Up to 1080p, two decoding threads keep up and hold far less memory than FFmpeg's own
        // choice (a 1080p VP9 video: about 190 MB, against about 270 MB). Larger videos need
        // FFmpeg's choice to keep up.
        if self.info.height <= 1080 {
            pictures.args(["-threads", "2"]);
        }
        start(&mut pictures, self.looping);
        pictures
            .args(["-an", "-sn", "-vf", &format!("fps={fps:.4},{scale},setsar=1")])
            .args(["-pix_fmt", format, "-f", "rawvideo", "pipe:1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = pictures.spawn().map_err(|e| format!("FFmpeg could not be started: {e}"))?;
        let (Some(mut out), Some(errors)) = (child.stdout.take(), child.stderr.take()) else {
            return Err("FFmpeg could not be started.".to_owned());
        };
        run.children.push(child);
        // What FFmpeg says is read as it says it, keeping only the first line: left unread, a
        // video that keeps logging errors would fill the pipe and stop FFmpeg, and the video.
        let first_error: Arc<Mutex<String>> = Arc::default();
        let kept_error = first_error.clone();
        let drained = std::thread::spawn(move || {
            let mut errors = std::io::BufReader::new(errors);
            let mut line = Vec::new();
            while std::io::BufRead::read_until(&mut errors, b'\n', &mut line).is_ok_and(|read| read > 0) {
                let said = String::from_utf8_lossy(&line);
                if let Ok(mut first) = kept_error.lock()
                    && first.is_empty()
                    && !said.trim().is_empty()
                {
                    *first = said.trim().chars().take(300).collect();
                }
                line.clear();
            }
        });
        let sound_drives = self.info.sound && self.stream.is_some();
        let slot = self.slot.clone();
        let pace = self.pace.clone();
        let (looping, planes) = (self.looping, self.planes);
        let (clock_v, stop_v, ended_v) = (clock.clone(), stop.clone(), ended.clone());
        let on_hardware = self.hardware.is_some();
        run.threads.push(std::thread::spawn(move || {
            let mut shown: u64 = 0;
            loop {
                let mut bytes = slot.lock().map_or_else(|_| vec![0u8; frame_len], |mut slot| slot.room(frame_len));
                if out.read_exact(&mut bytes).is_err() {
                    if shown == 0 && !stop_v.load(Ordering::Relaxed) {
                        // FFmpeg has stopped, so what it said is all read in a moment.
                        let _ = drained.join();
                        let said = first_error.lock().map(|first| first.clone()).unwrap_or_default();
                        if on_hardware && !said.is_empty() {
                            // Most likely the graphics chip's decoder couldn't be set up: neither
                            // ended nor a problem yet, as the player asks again without it.
                            hardware_failed.store(true, Ordering::Relaxed);
                            return;
                        }
                        let first = if said.is_empty() { "it could not decode the video".to_owned() } else { said };
                        if let Ok(mut problem) = problem.lock() {
                            *problem = Some(format!("FFmpeg could not play the video: {first}"));
                        }
                    }
                    if !looping {
                        ended_v.store(true, Ordering::Relaxed);
                    } else if shown > 0 && !stop_v.load(Ordering::Relaxed) {
                        // Meant to go round, and it has stopped after showing pictures.
                        ran_out.store(true, Ordering::Relaxed);
                    }
                    return;
                }
                if !sound_drives {
                    clock_v.start();
                }
                // Each picture waits for its time on the clock. The first is put out at once, before
                // the clock starts (with the sound, once it is played): a paused video shows it,
                // and one lined up to follow another has it ready when it takes over.
                let due = shown as f64 / fps;
                shown += 1;
                loop {
                    if stop_v.load(Ordering::Relaxed) {
                        return;
                    }
                    match clock_v.elapsed() {
                        _ if due <= 0.0 => break,
                        Some(now) if now + 0.002 >= due => break,
                        // Paused: nothing is due; look again a little later.
                        Some(_) if clock_v.paused() => std::thread::sleep(Duration::from_millis(30)),
                        Some(now) => std::thread::sleep(Duration::from_secs_f64((due - now).min(0.01))),
                        None => std::thread::sleep(Duration::from_millis(5)),
                    }
                }
                if tracing() && shown.is_multiple_of(fps.round().max(1.0) as u64) {
                    let clock = clock_v.elapsed().unwrap_or(0.0);
                    eprintln!("sync picture {due:.3}s clock {clock:.3}s");
                }
                // When it was due on screen: as long ago as the clock is past its time.
                let late = clock_v.elapsed().map_or(0.0, |now| (now - due).max(0.0));
                let due_at = Instant::now().checked_sub(Duration::from_secs_f64(late)).unwrap_or_else(Instant::now);
                pace.decoded();
                if let Ok(mut slot) = slot.lock() {
                    slot.put(Decoded { width: w, height: h, bytes, planes, at: from + due, due: due_at });
                }
            }
        }));

        // The sound: stereo at 48 kHz, into a queue the sound output drains.
        if let Some(stream) = self.stream.as_ref().filter(|_| self.info.sound) {
            let mut child = sound_command(&ffmpeg, &self.path, from, self.looping, RATE)
                .spawn()
                .map_err(|e| format!("FFmpeg could not be started: {e}"))?;
            let Some(out) = child.stdout.take() else {
                return Err("FFmpeg could not be started.".to_owned());
            };
            let mut out = SoundReader::new(out);
            run.children.push(child);
            let sound = Arc::new(Sound {
                waiting: Mutex::new(VecDeque::with_capacity(SOUND_AHEAD)),
                room: Condvar::new(),
                done: AtomicBool::new(false),
                clock: clock.clone(),
            });
            let reader = sound.clone();
            let stop_a = stop.clone();
            run.threads.push(std::thread::spawn(move || {
                let mut samples: Vec<f32> = Vec::new();
                loop {
                    samples.clear();
                    match out.read(&mut samples) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let Ok(mut waiting) = reader.waiting.lock() else { break };
                    // Held back while half a second is already waiting; FFmpeg then waits too.
                    let mut waiting = loop {
                        if stop_a.load(Ordering::Relaxed) {
                            return;
                        }
                        if waiting.len() < SOUND_AHEAD {
                            break waiting;
                        }
                        match reader.room.wait_timeout(waiting, Duration::from_millis(50)) {
                            Ok((still_waiting, _)) => waiting = still_waiting,
                            Err(_) => return,
                        }
                    };
                    waiting.extend(samples.drain(..));
                }
                reader.done.store(true, Ordering::Relaxed);
            }));
            let sink = rodio::Sink::connect_new(stream.mixer());
            sink.set_volume(self.volume);
            if self.paused {
                sink.pause();
            }
            sink.append(SoundSource { sound, taken: VecDeque::new(), started: false, played: 0 });
            run.sink = Some(sink);
        }
        self.run = Some(run);
        Ok(())
    }

    pub fn frames(&self) -> Frames {
        Frames { slot: self.slot.clone(), pace: self.pace.clone() }
    }

    pub fn play(&mut self) {
        if self.ended() {
            self.paused = false;
            let _ = self.start_at(0.0);
            return;
        }
        self.paused = false;
        if let Some(run) = &self.run {
            run.clock.set_paused(false);
            if let Some(sink) = &run.sink {
                sink.play();
            }
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
        if let Some(run) = &self.run {
            run.clock.set_paused(true);
            if let Some(sink) = &run.sink {
                sink.pause();
            }
        }
    }

    pub fn playing(&self) -> bool {
        !self.paused && !self.ended()
    }

    pub fn seek(&mut self, to: f64) {
        let _ = self.start_at(to.max(0.0));
    }

    /// How far in it is, in seconds.
    pub fn position(&self) -> f64 {
        let Some(run) = &self.run else { return 0.0 };
        let at = run.from + run.clock.elapsed().unwrap_or(0.0);
        match self.info.duration {
            Some(length) if self.looping && length > 0.0 => at % length,
            Some(length) => at.min(length),
            None => at,
        }
    }

    pub fn duration(&self) -> Option<f64> {
        self.info.duration
    }

    /// Looping is part of how FFmpeg is started, so a change starts it again where it is.
    pub fn set_looping(&mut self, looping: bool) {
        if self.looping != looping {
            self.looping = looping;
            let at = self.position();
            let _ = self.start_at(at);
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = self.run.as_ref().and_then(|run| run.sink.as_ref()) {
            sink.set_volume(self.volume);
        }
    }

    pub fn ended(&self) -> bool {
        self.run.as_ref().is_some_and(|run| run.ended.load(Ordering::Relaxed))
    }

    pub fn problem(&self) -> Option<String> {
        self.run.as_ref().and_then(|run| run.problem.lock().ok().and_then(|problem| problem.clone()))
    }

    /// What FFmpeg is asked for, for the log: to decode on the graphics chip (where the codec
    /// allows) or with its own decoder on the processor, and the pictures it gives.
    pub fn asked(&self) -> String {
        let decoding = match self.hardware {
            Some(decoder) => format!("decoding asked of the graphics chip ({decoder}) where the codec allows"),
            None => "decoding on the processor".to_owned(),
        };
        let (w, h) = self.size;
        let pictures = match self.planes {
            Some(Colours { matrix, full_range }) => {
                let range = if full_range { "full" } else { "limited" };
                format!("pictures {w} x {h} as NV12 planes the GPU turns into colours ({matrix:?}, {range} range)")
            }
            None => format!("pictures {w} x {h} as RGBA, converted by FFmpeg"),
        };
        format!("{decoding}; {pictures}")
    }

    /// Something to say in the log once, if there is something new.
    pub fn take_note(&mut self) -> Option<String> {
        self.note.take()
    }

    /// FFmpeg loops by itself; nothing to do but say when to look again.
    pub fn tick(&mut self) -> Duration {
        // FFmpeg stopped before its first picture, asked to decode on the graphics chip: asked
        // again, from the same place, with its own decoder.
        if let Some(run) = self.run.as_ref().filter(|run| run.hardware_failed.load(Ordering::Relaxed)) {
            let from = run.from;
            let decoder = self.hardware.take().unwrap_or_default();
            self.note = Some(format!(
                "FFmpeg couldn't decode {} on the graphics chip ({decoder}); it decodes on the processor instead",
                shown_name(&self.path)
            ));
            let _ = self.start_at(from);
            return Duration::from_millis(50);
        }
        // A looping video FFmpeg couldn't start again by itself is started again from its
        // beginning here (one that gave no pictures at all is left with its problem said).
        if self.looping && self.run.as_ref().is_some_and(|run| run.ran_out.load(Ordering::Relaxed)) {
            let _ = self.start_at(0.0);
            return Duration::from_millis(50);
        }
        Duration::from_millis(200)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_sizes_and_what_ffprobe_says_are_read() {
        assert_eq!(rate("30000/1001").map(|r| (r * 100.0).round()), Some(2997.0));
        assert_eq!(rate("0/0"), None);
        let json: serde_json::Value = serde_json::json!({
            "streams": [
                {"codec_type": "video", "width": 3840, "height": 2160, "avg_frame_rate": "0/0", "r_frame_rate": "25/1"},
                {"codec_type": "audio"}
            ],
            "format": {"duration": "12.5"}
        });
        let info = info_from(&json).unwrap();
        assert_eq!(
            info,
            Info { width: 3840, height: 2160, fps: 25.0, duration: Some(12.5), sound: true, colours: None }
        );
        assert_eq!(fitted(&info, MAX_W, MAX_H), (1920, 1080));
        // A tall video fits the box by its height, with even sides.
        let tall = Info { width: 1081, height: 1921, ..info };
        assert_eq!(fitted(&tall, MAX_W, MAX_H), (608, 1080));
        // A cover picture is not the video.
        let cover = serde_json::json!({"streams": [{"codec_type": "video", "width": 600, "height": 600, "disposition": {"attached_pic": 1}}]});
        assert!(info_from(&cover).is_none());
    }

    /// A video FFmpeg can't decode with the graphics chip asked for (here a decoder that doesn't
    /// exist, as when the chip's can't be set up) is played all the same, with FFmpeg's own
    /// decoder, and the log is told once.
    #[test]
    fn a_run_whose_hardware_decoder_fails_is_asked_for_again_without_it() {
        let Some(ffmpeg) = tool("ffmpeg") else {
            eprintln!("FFmpeg isn't on this computer: the fallback wasn't checked");
            return;
        };
        let dir = std::env::temp_dir().join(format!("softpresenter-hwfallback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("grey.mkv");
        let made = Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=gray:s=64x36:r=30:d=1", "-c:v", "ffv1"])
            .arg(&path)
            .status()
            .expect("FFmpeg runs");
        assert!(made.success(), "the video is made");
        let mut player = Player::open(&path, false, Arc::default()).expect("FFmpeg opens the video");
        player.hardware = Some("no-such-decoder");
        player.start_at(0.0).unwrap();
        let (started, mut note, mut picture) = (Instant::now(), None, None);
        while started.elapsed() < Duration::from_secs(5) && picture.is_none() {
            std::thread::sleep(player.tick().min(Duration::from_millis(20)));
            note = note.or_else(|| player.take_note());
            picture = player.frames().next();
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(picture.is_some(), "a picture came with FFmpeg's own decoder");
        assert_eq!(player.problem(), None);
        assert!(note.is_some_and(|note| note.contains("no-such-decoder")), "the log was told");
        assert!(player.asked().starts_with("decoding on the processor;"), "{}", player.asked());
    }

    /// Which videos come as planes, and in what colours: as FFmpeg's converter takes them.
    #[test]
    fn videos_of_quarter_colour_with_no_alpha_come_as_planes_in_the_colours_they_name() {
        let colours = |stream: serde_json::Value| colours_of(&stream);
        let limited = |matrix| Some(Colours { matrix, full_range: false });
        assert_eq!(
            colours(serde_json::json!({"pix_fmt": "yuv420p", "color_space": "bt709", "color_range": "tv"})),
            limited(Matrix::Bt709)
        );
        // Named nothing: BT.601 and limited range, as FFmpeg's converter takes it, HD or not.
        assert_eq!(colours(serde_json::json!({"pix_fmt": "yuv420p"})), limited(Matrix::Bt601));
        assert_eq!(
            colours(serde_json::json!({"pix_fmt": "yuv420p", "color_space": "unknown"})),
            limited(Matrix::Bt601)
        );
        assert_eq!(colours(serde_json::json!({"pix_fmt": "nv12", "color_space": "smpte170m"})), limited(Matrix::Bt601));
        assert_eq!(
            colours(serde_json::json!({"pix_fmt": "yuv420p10le", "color_space": "bt2020nc"})),
            limited(Matrix::Bt2020)
        );
        assert_eq!(colours(serde_json::json!({"pix_fmt": "p010le", "color_space": "fcc"})), limited(Matrix::Fcc));
        // Full range where the stream says so, or the pixel format is the JPEG one.
        let full = Some(Colours { matrix: Matrix::Bt709, full_range: true });
        assert_eq!(
            colours(serde_json::json!({"pix_fmt": "yuv420p", "color_space": "bt709", "color_range": "pc"})),
            full
        );
        assert_eq!(
            colours(serde_json::json!({"pix_fmt": "yuvj420p"})),
            Some(Colours { matrix: Matrix::Bt601, full_range: true })
        );
        // Clear parts, RGB pixels, more colour than a quarter, or another kind of standard: RGBA.
        for format in ["yuva420p", "rgb24", "gbrp", "bgra", "yuv422p10le", "yuv444p", "pal8", "gray"] {
            assert_eq!(colours(serde_json::json!({"pix_fmt": format})), None, "{format}");
        }
        assert_eq!(colours(serde_json::json!({"pix_fmt": "yuv420p", "color_space": "ycgco"})), None);
        assert_eq!(colours(serde_json::json!({"pix_fmt": "yuv420p", "color_space": "gbr"})), None);
        assert_eq!(colours(serde_json::json!({})), None, "a format FFmpeg doesn't name");
    }

    #[test]
    fn what_ffprobe_says_of_a_sound_file_is_read_and_a_protected_one_is_known() {
        let alac = serde_json::json!({
            "streams": [{"codec_type": "audio", "codec_name": "alac", "codec_tag_string": "alac",
                         "channels": 2, "sample_rate": "48000", "duration": "6.000000"}],
            "format": {"duration": "6.000000"}
        });
        assert_eq!(
            sound_info_from(&alac, ""),
            Ok(SoundInfo { duration: Some(6.0), codec: "alac".to_owned(), channels: 2, rate: 48_000 })
        );
        // Bought from the iTunes Store or kept from Apple Music: FairPlay's sample entry.
        let locked = serde_json::json!({
            "streams": [{"codec_type": "audio", "codec_name": "aac", "codec_tag_string": "drms"}]
        });
        assert_eq!(sound_info_from(&locked, ""), Err(SoundProblem::Protected));
        // A video with no sound, a file FFmpeg can't read (what it said), and a codec it lacks.
        let silent = serde_json::json!({"streams": [{"codec_type": "video", "codec_name": "h264"}]});
        assert_eq!(sound_info_from(&silent, ""), Err(SoundProblem::NoSound));
        assert_eq!(
            sound_info_from(&serde_json::json!({}), "Invalid data found when processing input"),
            Err(SoundProblem::Unreadable("Invalid data found when processing input".to_owned()))
        );
        let unknown = serde_json::json!({"streams": [{"codec_type": "audio", "codec_tag_string": "xyzw"}]});
        assert!(matches!(sound_info_from(&unknown, ""), Err(SoundProblem::Unreadable(said)) if said.contains("xyzw")));
        // The length from the format when the stream doesn't say it.
        let mp3 = serde_json::json!({
            "streams": [{"codec_type": "audio", "codec_name": "mp3", "channels": 1, "sample_rate": "44100"}],
            "format": {"duration": "183.5"}
        });
        assert_eq!(sound_info_from(&mp3, "").map(|info| info.duration), Ok(Some(183.5)));
    }

    /// FFmpeg decodes a sound file's sound to stereo at the rate asked, whole frames at a time,
    /// from a place in it.
    #[test]
    fn a_sound_file_is_decoded_to_stereo_at_the_rate_asked() {
        let Some(ffmpeg) = tool("ffmpeg") else {
            eprintln!("FFmpeg isn't on this computer: decoding sound wasn't checked");
            return;
        };
        let dir = std::env::temp_dir().join(format!("softpresenter-sound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        // The app's own FFmpeg has no encoders but WAV's needs none: one second, mono.
        let made = Command::new(&ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=1000:duration=1:sample_rate=44100"])
            .args(["-c:a", "pcm_f32le"])
            .arg(&path)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("This FFmpeg can't make a test tone: decoding sound wasn't checked");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let info = sound_info(&path).expect("ffprobe reads it");
        assert_eq!((info.channels, info.rate), (1, 44_100));
        assert!(info.duration.is_some_and(|seconds| (seconds - 1.0).abs() < 0.01));
        let (mut child, mut reader) = decode_sound(&path, 0.5, 48_000).expect("FFmpeg starts");
        let mut samples = Vec::new();
        while reader.read(&mut samples).unwrap() > 0 {}
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);
        // Half a second, stereo, at 48 kHz: the two sides the same.
        assert_eq!(samples.len() % 2, 0);
        let frames = samples.len() / 2;
        assert!(frames.abs_diff(24_000) < 200, "{frames} frames");
        assert!(samples.as_chunks::<2>().0.iter().all(|[left, right]| (left - right).abs() < 1e-6));
        // The tone (at an eighth of full scale, less 3 dB in each side from one channel).
        assert!(samples.iter().any(|sample| sample.abs() > 0.05));
    }

    /// A video with sound played through FFmpeg: its sound (decoded as the audio player's tracks
    /// are) starts its clock, and it moves on. Needs a sound output (it plays at no volume), and
    /// an FFmpeg that can make the video: `cargo test -p sp-video a_video_with -- --ignored`.
    #[test]
    #[ignore = "needs the computer's sound output"]
    fn a_video_with_sound_moves_on_by_its_sound() {
        let Some(ffmpeg) = tool("ffmpeg") else { return };
        let dir = std::env::temp_dir().join(format!("softpresenter-video-sound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.mkv");
        let made = Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=gray:s=64x36:r=30:d=4"])
            .args(["-f", "lavfi", "-i", "sine=frequency=500:sample_rate=48000:duration=4"])
            .args(["-c:v", "ffv1", "-c:a", "flac", "-shortest"])
            .arg(&path)
            .status()
            .expect("FFmpeg runs");
        assert!(made.success(), "the video is made");
        let mut player = Player::open(&path, false, Arc::default()).expect("FFmpeg opens the video");
        assert!(player.info.sound && player.stream.is_some(), "it has sound, and an output to play it");
        player.set_volume(0.0);
        player.play();
        std::thread::sleep(Duration::from_millis(1500));
        let position = player.position();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(position > 1.0 && position < 2.0, "{position} s");
        assert_eq!(player.problem(), None);
    }

    #[test]
    fn the_clock_waits_to_start_and_stops_while_paused() {
        let clock = Clock::default();
        assert_eq!(clock.elapsed(), None);
        clock.start();
        std::thread::sleep(Duration::from_millis(30));
        clock.set_paused(true);
        let at = clock.elapsed().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert!((clock.elapsed().unwrap() - at).abs() < 0.001, "paused");
        clock.set_paused(false);
        std::thread::sleep(Duration::from_millis(20));
        assert!(clock.elapsed().unwrap() > at + 0.015);
    }
}
