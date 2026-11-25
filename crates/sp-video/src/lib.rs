//! Videos, and pictures in formats beyond the common few. macOS's own player (AVFoundation)
//! plays what it can, decoding in hardware: MP4, MOV, HEVC, ProRes and the like. Everything
//! else (MKV, WebM, AVI, or a codec this Mac's player lacks) is played by FFmpeg, run as a
//! separate program, without converting it first. Pictures are read with ImageIO, which reads
//! what Preview reads (HEIC, TIFF, PSD and others). On Windows, PDF pages are drawn by Windows'
//! own PDF reader (`pdf_windows`).
//!
//! A [`Player`] is made and controlled on the main thread. The pictures it makes are taken from
//! a [`Frames`], which can go to another thread: the one drawing the screens.
//!
//! Web pages are drawn the same way: a [`WebPage`] made on the main thread, its pictures taken
//! from a [`Frames`] (see the `web` module).
//!
//! Whatever plays sound here hands it to the [`sound`] module's mix as well, for the screens
//! that send sound beside their pictures (NDI).
//!
//! This is the one crate in SoftPresenter that may use `unsafe` code, for calls into the
//! system's frameworks, and only in its two macOS modules (`mac` for videos and pictures, with its
//! `gpu` and `cover` parts, `web_mac` for web pages); each such call says why it is sound.

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod web_mac;

pub mod web;
pub use web::WebPage;

mod ffmpeg;
pub use ffmpeg::{SoundInfo, SoundProblem, SoundReader, decode_sound, sound_info};

pub mod sound;

#[cfg(target_os = "macos")]
pub use mac::{
    DisplayWatch, GpuImport, GpuNext, cover_display, display_names, home_folder, pdf_draw, pdf_sizes, picture,
};

#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(not(target_os = "macos"))]
pub use other::{display_names, home_folder, picture};

#[cfg(all(not(target_os = "macos"), not(windows)))]
pub use other::{pdf_draw, pdf_sizes};

// PDF pages on Windows, by Windows' own PDF reader (asked through PowerShell; no unsafe code).
#[cfg(any(windows, test))]
mod pdf_windows;

#[cfg(windows)]
pub use pdf_windows::{pdf_draw, pdf_sizes, powershell};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Videos the system's player is asked to play first; anything else goes to FFmpeg.
const SYSTEM_VIDEOS: [&str; 4] = ["mp4", "mov", "m4v", "3gp"];

fn system_first(path: &Path) -> bool {
    cfg!(target_os = "macos")
        && path.extension().is_some_and(|ext| SYSTEM_VIDEOS.contains(&ext.to_string_lossy().to_lowercase().as_str()))
}

/// True when FFmpeg is on this computer, for the videos the system's player can't play.
pub fn ffmpeg_found() -> bool {
    ffmpeg::tool("ffmpeg").is_some()
}

/// Where the FFmpeg program is (the app's own, then one installed on the computer), for the
/// separate program that receives phone screens, which is handed it to decode with.
pub fn ffmpeg_program() -> Option<std::path::PathBuf> {
    ffmpeg::tool("ffmpeg")
}

/// A still from a video, no larger than `max_side` on its longer side: from the system's player
/// if it can read the video, otherwise from FFmpeg. Many videos open on black or fade in, so a
/// dark picture half a second in is passed over for one a little later.
pub fn still(path: &Path, max_side: u32) -> Result<Frame, String> {
    let mut last = None;
    for at in [0.5, 2.0, 5.0] {
        match still_at(path, max_side, at) {
            Ok(frame) if !dark(&frame) => return Ok(frame),
            Ok(frame) => last = Some(frame),
            Err(problem) => return last.ok_or(problem),
        }
    }
    last.ok_or_else(|| "The video has no picture.".to_owned())
}

fn still_at(path: &Path, max_side: u32, at: f64) -> Result<Frame, String> {
    #[cfg(target_os = "macos")]
    let system = mac::still(path, max_side, at);
    #[cfg(not(target_os = "macos"))]
    let system: Result<Frame, String> = Err(String::new());
    match system {
        Ok(frame) => Ok(frame),
        Err(system_error) => ffmpeg::still(path, max_side, at).map_err(|ffmpeg_error| {
            // A video the system read fine but FFmpeg can't is still the system's problem to report.
            if system_first(path) && !system_error.is_empty() { system_error } else { ffmpeg_error }
        }),
    }
}

/// How long a video is, in seconds: as the system's player reads it if it can, otherwise as
/// FFmpeg says. `None` when neither can say. It reads the file, so it is for a thread that may
/// wait (the media bin's readers), not the screens'.
pub fn length(path: &Path) -> Option<f64> {
    #[cfg(target_os = "macos")]
    if let Some(seconds) = mac::length(path) {
        return Some(seconds);
    }
    ffmpeg::probe(path).ok()?.duration.filter(|seconds| seconds.is_finite() && *seconds > 0.0)
}

/// True for a picture that is nearly all black: a video's opening before its first scene.
fn dark(frame: &Frame) -> bool {
    let pixels = frame.rgba.as_chunks::<4>().0;
    let step = (pixels.len() / 2000).max(1);
    let (sum, count) = pixels.iter().step_by(step).fold((0u64, 0u64), |(sum, count), [r, g, b, _]| {
        (sum + u64::from(*r) * 3 + u64::from(*g) * 6 + u64::from(*b), count + 10)
    });
    count == 0 || sum / count < 20
}

/// Where a playing video's (or a web page's) pictures are taken from. It can be sent to
/// another thread.
#[derive(Clone)]
pub enum Frames {
    Video(VideoFrames),
    Web(web::WebFrames),
}

/// Where a playing video's pictures are taken from: whichever engine plays it now, the same
/// handle after a file lined up to follow has taken over.
#[derive(Clone)]
pub struct VideoFrames(Arc<Mutex<Source>>);

impl VideoFrames {
    /// The engine's pictures now. Only a hand-over holds the lock, for as long as it takes to
    /// put the new engine's in.
    fn now(&self) -> Source {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

impl Frames {
    /// True when both take pictures from the same video (the same player, whichever file it
    /// plays now).
    pub fn same(&self, other: &Frames) -> bool {
        match (self, other) {
            (Frames::Video(a), Frames::Video(b)) => Arc::ptr_eq(&a.0, &b.0),
            (Frames::Web(a), Frames::Web(b)) => a.same(b),
            _ => false,
        }
    }

    /// The picture to show now, if there is a new one since the last was taken.
    pub fn next(&self) -> Option<Frame> {
        match self {
            Frames::Video(video) => match video.now() {
                #[cfg(target_os = "macos")]
                Source::System(frames) => frames.next(),
                Source::Ffmpeg(frames) => frames.next(),
            },
            Frames::Web(frames) => frames.next(),
        }
    }

    /// The picture to show now as a GPU texture made of the decoder's own memory, with nothing
    /// copied, for a video the system plays whose pictures can go that way (see
    /// [`GpuNext`]). The thread drawing the screens asks this first, and copies with
    /// [`Frames::next_with`] when it is answered [`GpuNext::Unavailable`].
    #[cfg(target_os = "macos")]
    pub fn next_texture(&self, import: &GpuImport, device: &wgpu::Device) -> GpuNext {
        match self {
            Frames::Video(video) => match video.now() {
                Source::System(frames) => frames.next_texture(import, device),
                Source::Ffmpeg(_) => GpuNext::Unavailable,
            },
            Frames::Web(_) => GpuNext::Unavailable,
        }
    }

    /// From now on this video's pictures are copied, as [`Frames::next_with`] does, and none goes
    /// to the GPU as the decoder's own memory: for when one taken that way couldn't be shown.
    #[cfg(target_os = "macos")]
    pub fn copy_from_now(&self) {
        if let Frames::Video(video) = self
            && let Source::System(frames) = video.now()
        {
            frames.copy_from_now();
        }
    }

    /// Lends the picture to show now to `show`, if there is a new one since the last was taken,
    /// and hands back what `show` made of it. Nothing is copied for a video the system plays
    /// (one wider than 2048 pixels is lent at half size), and FFmpeg's picture goes back to be
    /// filled again: for the thread drawing the screens, which sends the pixels straight to the
    /// GPU. A player opened to lend [`Lend::Rows`] lends nothing but rows.
    pub fn next_with<R>(&self, show: impl FnOnce(Lending<'_>) -> R) -> Option<R> {
        match self {
            Frames::Video(video) => match video.now() {
                #[cfg(target_os = "macos")]
                Source::System(frames) => frames.next_with(|lent| show(Lending::Rows(lent))),
                Source::Ffmpeg(frames) => frames.next_with(show),
            },
            Frames::Web(frames) => {
                frames.next().map(|frame| show(Lending::Rows(&Lent::rgba(frame.width, frame.height, &frame.rgba))))
            }
        }
    }
}

/// How a player lends its pictures (see [`Frames::next_with`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lend {
    /// Always as rows of four-byte pixels: for pictures copied for every thread that draws (a
    /// video in a box on a slide, or on the Announcements layer).
    #[default]
    Rows,
    /// As the video's own two planes where FFmpeg plays it and the video allows (see
    /// [`Planes`]), otherwise as rows: for the thread that sends the video behind the slide
    /// to the GPU, which turns the planes into colours there.
    Planes,
}

/// A picture lent for the length of a call (see [`Frames::next_with`]).
pub enum Lending<'a> {
    /// Rows of four-byte pixels.
    Rows(&'a Lent<'a>),
    /// A video's own two planes, for the GPU to turn into colours.
    Planes(&'a Planes<'a>),
}

impl<'a> Lending<'a> {
    /// The rows, for a caller that only takes pictures that way: a player opened to lend
    /// [`Lend::Rows`] never lends planes.
    pub fn rows(&self) -> Option<&'a Lent<'a>> {
        match self {
            Lending::Rows(lent) => Some(lent),
            Lending::Planes(_) => None,
        }
    }
}

/// A video's picture as FFmpeg decodes most videos (NV12), lent for the length of a call:
/// every pixel's brightness, a byte each, and the colour of each square of four pixels, its Cb
/// and Cr bytes side by side, in a plane half the width and half the height (rounded up, so an
/// odd side's edge has samples of its own). Each plane's rows follow one another with nothing
/// between. [`Colours`] says how they stand for red, green and blue.
pub struct Planes<'a> {
    pub width: u32,
    pub height: u32,
    /// `width` bytes a row.
    pub luma: &'a [u8],
    /// [`Planes::chroma_row`] bytes a row, `ceil(height / 2)` rows.
    pub chroma: &'a [u8],
    pub colours: Colours,
}

impl Planes<'_> {
    /// The bytes a picture of this size takes: both planes.
    pub fn bytes(width: u32, height: u32) -> usize {
        let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
        width as usize * height as usize + cw * ch * 2
    }

    /// How many bytes each row of the colour plane takes: `2 * ceil(width / 2)`.
    pub fn chroma_row(&self) -> usize {
        self.width.div_ceil(2) as usize * 2
    }
}

/// How a video's planes stand for colours: the standard they were written in, and whether
/// their bytes use the whole range or broadcast video's limited one (brightness 16 to 235), as
/// nearly every video does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colours {
    pub matrix: Matrix,
    pub full_range: bool,
}

/// The standard a video's colours were written in: the ones FFmpeg's own converter has numbers
/// for, any other (or none named) being taken as BT.601, as FFmpeg takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matrix {
    Bt601,
    Bt709,
    Fcc,
    Smpte240,
    Bt2020,
}

/// The order of the four bytes of a lent picture's pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelOrder {
    Rgba,
    Bgra,
}

/// A picture lent for the length of a call (see [`Frames::next_with`]): rows of four-byte
/// pixels, each row starting `bytes_per_row` after the last, which may be more than its pixels
/// fill.
pub struct Lent<'a> {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: usize,
    pub bytes: &'a [u8],
    pub order: PixelOrder,
    /// True when the picture is shown opaque whatever its pixels' fourth bytes say (a video the
    /// system plays); otherwise they are its straight alpha.
    pub opaque: bool,
}

impl<'a> Lent<'a> {
    /// True when every pixel's fourth byte is 255: the picture is opaque as its bytes stand,
    /// with nothing to put right before the GPU draws them. Every pixel is looked at, which
    /// takes far less time than copying them; the first row with a pixel that isn't ends it.
    pub fn fourth_bytes_full(&self) -> bool {
        let row = self.width as usize * 4;
        if row == 0 || self.bytes_per_row < row {
            return false;
        }
        let mut rows = self.bytes.chunks(self.bytes_per_row).take(self.height as usize);
        rows.len() == self.height as usize
            && rows.all(|line| {
                line.len() >= row && {
                    let all = line[..row]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .fold(u32::MAX, |all, pixel| all & u32::from_ne_bytes(*pixel));
                    all.to_ne_bytes()[3] == 255
                }
            })
    }

    fn rgba(width: u32, height: u32, bytes: &'a [u8]) -> Lent<'a> {
        Lent { width, height, bytes_per_row: width as usize * 4, bytes, order: PixelOrder::Rgba, opaque: false }
    }
}

/// A video engine: the system's player, or FFmpeg.
enum Engine {
    #[cfg(target_os = "macos")]
    System(mac::Player),
    Ffmpeg(Box<ffmpeg::Player>),
}

/// Runs the same call on whichever engine it is.
macro_rules! either {
    ($self:expr, $engine:ident => $call:expr) => {
        match $self {
            #[cfg(target_os = "macos")]
            Engine::System($engine) => $call,
            Engine::Ffmpeg($engine) => $call,
        }
    };
}

impl Engine {
    /// Opens a video, paused at its start: with the system's player for an MP4 or MOV (on the
    /// main thread), otherwise with FFmpeg.
    fn open(path: &Path, lend: Lend) -> Result<Engine, String> {
        #[cfg(target_os = "macos")]
        if system_first(path) {
            return mac::Player::open(path).map(Engine::System);
        }
        Engine::open_with_ffmpeg(path, lend)
    }

    fn open_with_ffmpeg(path: &Path, lend: Lend) -> Result<Engine, String> {
        ffmpeg::Player::open(path, lend == Lend::Planes).map(|player| Engine::Ffmpeg(Box::new(player)))
    }

    fn source(&self) -> Source {
        match self {
            #[cfg(target_os = "macos")]
            Engine::System(player) => Source::System(player.frames()),
            Engine::Ffmpeg(player) => Source::Ffmpeg(player.frames()),
        }
    }

    fn play(&mut self) {
        either!(self, engine => engine.play())
    }

    fn pause(&mut self) {
        either!(self, engine => engine.pause())
    }

    fn playing(&self) -> bool {
        either!(self, engine => engine.playing())
    }

    fn seek(&mut self, to: f64) {
        either!(self, engine => engine.seek(to))
    }

    fn position(&self) -> f64 {
        either!(self, engine => engine.position())
    }

    fn duration(&self) -> Option<f64> {
        either!(self, engine => engine.duration())
    }

    fn set_looping(&mut self, looping: bool) {
        either!(self, engine => engine.set_looping(looping))
    }

    fn set_volume(&mut self, volume: f32) {
        either!(self, engine => engine.set_volume(volume))
    }

    fn ended(&self) -> bool {
        either!(self, engine => engine.ended())
    }

    fn problem(&self) -> Option<String> {
        either!(self, engine => engine.problem())
    }

    fn tick(&mut self) -> Duration {
        either!(self, engine => engine.tick())
    }
}

/// Where one engine's pictures are taken from.
#[derive(Clone)]
enum Source {
    #[cfg(target_os = "macos")]
    System(mac::Frames),
    Ffmpeg(ffmpeg::Frames),
}

/// How close to the end of a pass a file lined up to follow takes over: the system's player
/// starts a looping video again this close to its end too.
const END_OF_PASS: f64 = 0.02;

/// A video playing, with its sound: through the system's player, or through FFmpeg.
///
/// Another file can be lined up to follow it ([`Player::play_next`]): it is opened at once, so
/// it is ready, and takes over exactly at the end of the pass under way, where this one would
/// start again or end. The screens take the pictures through the same [`Frames`] before and
/// after, so they hold the last picture of the one until the first of the other comes, never
/// starting afresh (and never black).
pub struct Player {
    engine: Engine,
    path: PathBuf,
    /// Where the screens take the pictures from: the engine playing now, swapped for the next
    /// file's when it takes over, so frames handed out before go on working.
    source: Arc<Mutex<Source>>,
    /// The file lined up to follow, opened and waiting at its start.
    next: Option<(Engine, PathBuf)>,
    /// How its pictures are lent, the next file's too.
    lend: Lend,
    looping: bool,
    volume: f32,
    /// How far in it was when last looked at, to see a looping video go round between two looks.
    last_position: f64,
    /// A file that took over since this was last asked, or why one lined up couldn't.
    handed_over: Option<Result<PathBuf, String>>,
}

impl Player {
    /// Opens a video, paused at its start: with the system's player for an MP4 or MOV (on the
    /// main thread), otherwise with FFmpeg. Its pictures are lent as `lend` says, and so are
    /// those of files lined up to follow it. The error is a sentence for the operator.
    pub fn open(path: &Path, lend: Lend) -> Result<Player, String> {
        Engine::open(path, lend).map(|engine| Player::with(engine, path, lend))
    }

    /// Opens a video with FFmpeg, for when the system's player has failed on it (an AV1 video
    /// on a Mac without AV1 hardware, say).
    pub fn open_with_ffmpeg(path: &Path, lend: Lend) -> Result<Player, String> {
        Engine::open_with_ffmpeg(path, lend).map(|engine| Player::with(engine, path, lend))
    }

    fn with(engine: Engine, path: &Path, lend: Lend) -> Player {
        Player {
            source: Arc::new(Mutex::new(engine.source())),
            engine,
            path: path.to_owned(),
            next: None,
            lend,
            looping: false,
            volume: 1.0,
            last_position: 0.0,
            handed_over: None,
        }
    }

    pub fn uses_ffmpeg(&self) -> bool {
        matches!(self.engine, Engine::Ffmpeg(_))
    }

    /// What FFmpeg was asked for, for the log: how to decode, and the pictures it gives (what
    /// it was asked, not what it used: it says nothing more). `None` for the system's player.
    pub fn ffmpeg_asked(&self) -> Option<String> {
        match &self.engine {
            #[cfg(target_os = "macos")]
            Engine::System(_) => None,
            Engine::Ffmpeg(player) => Some(player.asked()),
        }
    }

    /// Something to say in the log once, if there is something new: that FFmpeg couldn't decode
    /// on the graphics chip, say.
    pub fn take_note(&mut self) -> Option<String> {
        match &mut self.engine {
            #[cfg(target_os = "macos")]
            Engine::System(_) => None,
            Engine::Ffmpeg(player) => player.take_note(),
        }
    }

    /// The file playing now.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where its pictures are taken from, on any thread: the same before and after a file lined
    /// up takes over.
    pub fn frames(&self) -> Frames {
        Frames::Video(VideoFrames(self.source.clone()))
    }

    pub fn play(&mut self) {
        self.engine.play();
    }

    pub fn pause(&mut self) {
        self.engine.pause();
    }

    pub fn playing(&self) -> bool {
        self.engine.playing()
    }

    /// Goes to a time, in seconds from the start.
    pub fn seek(&mut self, to: f64) {
        self.engine.seek(to);
        self.last_position = to.max(0.0);
    }

    /// How far in it is, in seconds.
    pub fn position(&self) -> f64 {
        self.engine.position()
    }

    /// How long it is, once that is known.
    pub fn duration(&self) -> Option<f64> {
        self.engine.duration()
    }

    /// Whether it starts again at its end.
    pub fn looping(&self) -> bool {
        self.looping
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
        self.engine.set_looping(looping);
        if let Some((next, _)) = self.next.as_mut() {
            next.set_looping(looping);
        }
    }

    /// From 0 (silent) to 1.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        self.engine.set_volume(volume);
        if let Some((next, _)) = self.next.as_mut() {
            next.set_volume(volume);
        }
    }

    /// True once a video that doesn't loop has played to its end (and no file lined up is about
    /// to take over).
    pub fn ended(&self) -> bool {
        self.next.is_none() && self.engine.ended()
    }

    /// Why the video can't play, once the player has tried and failed.
    pub fn problem(&self) -> Option<String> {
        self.engine.problem()
    }

    /// Lines up another file to follow this one: it is opened now, waiting at its start, and
    /// takes over at the end of the pass under way (where this one would start again or end),
    /// looping as this one does, at its volume. One lined up before is let go. The error is a
    /// sentence for the operator; this one plays on as it was.
    pub fn play_next(&mut self, path: &Path) -> Result<(), String> {
        let mut next = Engine::open(path, self.lend)?;
        next.set_looping(self.looping);
        next.set_volume(self.volume);
        // The system's player decodes the first picture once asked for a time: it is ready
        // when the file takes over.
        next.seek(0.0);
        self.next = Some((next, path.to_owned()));
        self.last_position = self.engine.position();
        Ok(())
    }

    /// The file lined up to follow, if one is.
    pub fn next_file(&self) -> Option<&Path> {
        self.next.as_ref().map(|(_, path)| path.as_path())
    }

    /// A file lined up that has taken over since this was last asked (its path), or why it
    /// couldn't (this one then plays on).
    pub fn take_handed_over(&mut self) -> Option<Result<PathBuf, String>> {
        self.handed_over.take()
    }

    /// Keeps it going (a looping video starts again at its end), and hands over to a file lined
    /// up at the end of the pass. Says how soon to call again.
    pub fn tick(&mut self) -> Duration {
        if self.next.is_some()
            && let Some(wait) = self.hand_over_when_due()
        {
            return wait;
        }
        self.engine.tick()
    }

    /// With a file lined up: hands over to it at the end of the pass, or says how soon to look
    /// again, just before then. `None` while paused (nothing is due).
    fn hand_over_when_due(&mut self) -> Option<Duration> {
        let position = self.engine.position();
        // A looping video FFmpeg plays goes round by itself: one that went round since the last
        // look is handed over at once.
        let went_round = position + 0.25 < self.last_position;
        self.last_position = position;
        let left = self.engine.duration().map(|length| length - position);
        let playing = self.engine.playing();
        if self.engine.ended() || (playing && (went_round || left.is_some_and(|left| left <= END_OF_PASS))) {
            self.hand_over();
            return Some(Duration::from_millis(10));
        }
        if !playing {
            return None;
        }
        left.map(|left| Duration::from_secs_f64((left - END_OF_PASS).clamp(0.002, 0.25)))
    }

    /// The file lined up takes over: it plays, the screens take its pictures, and this one is
    /// let go. One the system's player can't play is tried with FFmpeg; if that can't either,
    /// this one plays on.
    fn hand_over(&mut self) {
        let Some((mut next, path)) = self.next.take() else { return };
        if let Some(problem) = next.problem() {
            match Engine::open_with_ffmpeg(&path, self.lend) {
                Ok(mut ffmpeg) if !matches!(next, Engine::Ffmpeg(_)) => {
                    ffmpeg.set_looping(self.looping);
                    ffmpeg.set_volume(self.volume);
                    next = ffmpeg;
                }
                _ => {
                    self.handed_over = Some(Err(problem));
                    return;
                }
            }
        }
        next.play();
        let old = std::mem::replace(&mut self.engine, next);
        *self.source.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = self.engine.source();
        self.path = path.clone();
        self.last_position = 0.0;
        self.handed_over = Some(Ok(path));
        drop(old);
    }
}

/// A decoded picture: tightly packed RGBA rows, straight (not premultiplied) alpha.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame({}x{})", self.width, self.height)
    }
}

/// Turns premultiplied RGBA into straight RGBA, in place.
#[cfg(any(target_os = "macos", test))]
fn unpremultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
}

/// Copies rows of BGRA pixels into RGBA, opaque. Every second pixel and row is taken when the
/// picture is wider than `max_width`, so a 4K video is copied at 1080p.
#[cfg(any(target_os = "macos", test))]
fn bgra_rows_to_rgba(bytes: &[u8], width: usize, height: usize, bytes_per_row: usize, max_width: usize) -> Frame {
    let step = if width > max_width { 2 } else { 1 };
    let (out_w, out_h) = (width / step, height / step);
    let mut rgba = Vec::with_capacity(out_w * out_h * 4);
    for row in (0..out_h).map(|row| row * step) {
        let start = row * bytes_per_row;
        let line = &bytes[start..start + width * 4];
        for pixel in line.chunks_exact(4 * step) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    // Rows of an odd width taken every second pixel can come out one short or long; trim or pad.
    rgba.resize(out_w * out_h * 4, 255);
    Frame { width: out_w as u32, height: out_h as u32, rgba }
}

/// Copies every second pixel of every second row of four-byte pixels into `out`, tightly
/// packed, in the order they came. Returns the size of the copy.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn halved_rows(bytes: &[u8], width: usize, height: usize, bytes_per_row: usize, out: &mut Vec<u8>) -> (usize, usize) {
    let (out_w, out_h) = (width / 2, height / 2);
    out.resize(out_w * out_h * 4, 0);
    if out_w == 0 {
        return (out_w, out_h);
    }
    for (row, line) in out.chunks_exact_mut(out_w * 4).enumerate() {
        let start = row * 2 * bytes_per_row;
        let pairs = bytes[start..start + out_w * 8].as_chunks::<8>().0;
        for (pixel, pair) in line.as_chunks_mut::<4>().0.iter_mut().zip(pairs) {
            *pixel = [pair[0], pair[1], pair[2], pair[3]];
        }
    }
    (out_w, out_h)
}

/// A size divided by the smallest whole number that makes it no wider than `most`; `None` when
/// it is no wider already, or isn't a size.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn smaller_than_wide(width: f64, height: f64, most: f64) -> Option<(u32, u32)> {
    if !(width.is_finite() && height.is_finite() && height >= 1.0 && width > most) {
        return None;
    }
    let by = (width / most).ceil();
    Some((((width / by).round() as u32).max(1), ((height / by).round() as u32).max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A video for the hand-over check: a second long at 30 pictures a second, every picture
    /// one colour (red, or blue) as bright as its number, so a picture says which video it is
    /// from and how far in.
    fn marker_video(ffmpeg: &Path, path: &Path, red: bool) {
        let colour = if red { "r='40+N*6':g='0':b='0'" } else { "r='0':g='0':b='40+N*6'" };
        let made = std::process::Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("color=c=black:s=64x36:r=30:d=1,format=rgb24,geq={colour}"))
            .args(["-c:v", "ffv1"])
            .arg(path)
            .status()
            .expect("FFmpeg runs");
        assert!(made.success(), "the marker video is made");
    }

    /// The red, green and blue of a lent picture's middle pixel.
    fn middle(lent: &Lent<'_>) -> [u8; 3] {
        let at = (lent.height as usize / 2) * lent.bytes_per_row + (lent.width as usize / 2) * 4;
        let pixel = &lent.bytes[at..at + 4];
        match lent.order {
            PixelOrder::Rgba => [pixel[0], pixel[1], pixel[2]],
            PixelOrder::Bgra => [pixel[2], pixel[1], pixel[0]],
        }
    }

    /// A file lined up takes over where the video playing would go round: the screens' pictures
    /// go from the end of the one to the start of the other, through the same frames, with no
    /// black picture, no gap past a picture or two, and the first not started again. (FFmpeg
    /// plays both here; `examples/handover.rs` checks the system's player, which must be on the
    /// main thread.)
    #[test]
    fn a_file_lined_up_takes_over_at_the_end_of_the_pass_with_no_black_picture() {
        let Some(ffmpeg) = ffmpeg::tool("ffmpeg") else {
            eprintln!("FFmpeg isn't on this computer: the hand-over wasn't checked");
            return;
        };
        let dir = std::env::temp_dir().join(format!("softpresenter-handover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (red, blue) = (dir.join("red.mkv"), dir.join("blue.mkv"));
        marker_video(&ffmpeg, &red, true);
        marker_video(&ffmpeg, &blue, false);
        let mut player = Player::open(&red, Lend::Rows).expect("FFmpeg opens the red video");
        player.set_looping(true);
        player.set_volume(0.0);
        let frames = player.frames();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let (frames, stop) = (frames.clone(), stop.clone());
            std::thread::spawn(move || {
                let (started, mut seen) = (std::time::Instant::now(), Vec::new());
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Some(pixel) = frames.next_with(|lending| lending.rows().map(middle)).flatten() {
                        seen.push((started.elapsed(), pixel));
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                seen
            })
        };
        player.play();
        let started = std::time::Instant::now();
        let (mut lined_up, mut took_over) = (false, None);
        while started.elapsed() < Duration::from_millis(2300) {
            if !lined_up && started.elapsed() > Duration::from_millis(300) {
                player.play_next(&blue).expect("the blue video opens");
                assert_eq!(player.next_file(), Some(blue.as_path()));
                lined_up = true;
            }
            let wait = player.tick();
            if let Some(handed) = player.take_handed_over() {
                took_over = Some((started.elapsed(), handed));
            }
            std::thread::sleep(wait.min(Duration::from_millis(5)));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let seen = reader.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let (when, handed) = took_over.expect("the blue video took over");
        assert_eq!(handed, Ok(blue.clone()));
        assert_eq!(player.path(), blue.as_path());
        assert!(
            when > Duration::from_millis(850) && when < Duration::from_millis(1300),
            "at the end of the red video's first pass, not when it was lined up: {when:?}"
        );
        assert!(frames.same(&player.frames()), "the screens' frames are the same handle after the hand-over");
        assert!(seen.len() > 40, "pictures came all the way through: {}", seen.len());
        for (at, [r, g, b]) in &seen {
            assert!((*r).max(*g).max(*b) >= 30, "no black picture: {r},{g},{b} at {at:?}");
        }
        let is_red = |[r, _, b]: &[u8; 3]| r > b;
        let first_blue = seen.iter().position(|(_, pixel)| !is_red(pixel)).expect("blue pictures came");
        assert!(first_blue > 0, "red pictures came first");
        assert!(seen[first_blue..].iter().all(|(_, pixel)| !is_red(pixel)), "no red picture after the first blue");
        let reds: Vec<u8> = seen[..first_blue].iter().map(|(_, [r, _, _])| *r).collect();
        assert!(reds.windows(2).all(|two| two[1] + 8 >= two[0]), "the red video wasn't started again: {reds:?}");
        let (last_red_at, [last_red, _, _]) = seen[first_blue - 1];
        let (first_blue_at, [_, _, first_blue_value]) = seen[first_blue];
        assert!(last_red >= 180, "the red video played to the end of its pass: its last picture {last_red}");
        assert!(first_blue_value <= 76, "the blue video started at its start: {first_blue_value}");
        let gap = first_blue_at - last_red_at;
        eprintln!(
            "hand-over at {when:?}: last red picture {last_red}, first blue {first_blue_value}, {gap:?} between them, \
             {} pictures, none black",
            seen.len()
        );
        assert!(gap < Duration::from_millis(120), "the last red picture held only a moment: {gap:?}");
    }

    /// The first picture of a video, lent as `lend` asks, in red, green and blue: as FFmpeg
    /// converted it, or for planes, as the GPU sums them (`sp_render::YuvColours`, which
    /// sp-render's golden test checks the GPU against), with the colours they came with.
    fn first_picture(path: &Path, lend: Lend) -> (u32, u32, Vec<[u8; 3]>, Option<Colours>) {
        let player = Player::open(path, lend).expect("FFmpeg opens the video");
        let frames = player.frames();
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            let taken = frames.next_with(|lending| match lending {
                Lending::Rows(lent) => {
                    assert_eq!(lent.bytes_per_row, lent.width as usize * 4);
                    let rgb = lent.bytes.as_chunks::<4>().0.iter().map(|[r, g, b, _]| [*r, *g, *b]).collect();
                    (lent.width, lent.height, rgb, None)
                }
                Lending::Planes(planes) => {
                    let matrix = match planes.colours.matrix {
                        Matrix::Bt601 => sp_render::YuvMatrix::Bt601,
                        Matrix::Bt709 => sp_render::YuvMatrix::Bt709,
                        Matrix::Fcc => sp_render::YuvMatrix::Fcc,
                        Matrix::Smpte240 => sp_render::YuvMatrix::Smpte240,
                        Matrix::Bt2020 => sp_render::YuvMatrix::Bt2020,
                    };
                    let sums = sp_render::YuvColours { matrix, full_range: planes.colours.full_range };
                    let (w, h) = (planes.width as usize, planes.height as usize);
                    assert_eq!(planes.luma.len(), w * h);
                    assert_eq!(planes.chroma.len(), planes.chroma_row() * h.div_ceil(2));
                    let rgb = (0..h)
                        .flat_map(|y| (0..w).map(move |x| (x, y)))
                        .map(|(x, y)| {
                            let at = y / 2 * planes.chroma_row() + x / 2 * 2;
                            sums.rgb(planes.luma[y * w + x], planes.chroma[at], planes.chroma[at + 1])
                        })
                        .collect();
                    (planes.width, planes.height, rgb, Some(planes.colours))
                }
            });
            if let Some(taken) = taken {
                return taken;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no picture came from {}", path.display());
    }

    /// The video behind the slide comes from FFmpeg as planes, and the colours the GPU makes of
    /// them are the ones FFmpeg itself made of the same video as RGBA, as it was asked before
    /// (so nothing shifts colour): for video named BT.709 or BT.601, video naming nothing (taken
    /// as BT.601 and limited range, at HD size too), and full range; and for an H.264 video
    /// (where FFmpeg has an H.264 encoder to make one), which the graphics chip decodes, so
    /// that FFmpeg's RGBA is made from the chip's planes rather than its own decoder's. Two videos FFmpeg must
    /// scale for RGBA (an odd size, which planes keep, and one larger than the screens, which
    /// both ways scale) are one flat colour, which the planes keep: FFmpeg's RGBA of a scaled
    /// picture comes out a step or three darker than its own unscaled conversion.
    #[test]
    fn ffmpegs_planes_make_the_colours_ffmpeg_made_as_rgba() {
        let Some(ffmpeg) = ffmpeg::tool("ffmpeg") else {
            eprintln!("FFmpeg isn't on this computer: the colours weren't checked");
            return;
        };
        let dir = std::env::temp_dir().join(format!("softpresenter-planes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let limited = |matrix| Colours { matrix, full_range: false };
        let lossless: &[&str] = &["-c:v", "ffv1"];
        let h264: &[&str] = &["-c:v", "libx264", "-preset", "ultrafast", "-crf", "12"];
        let clips: [(&str, &str, &[&str], Colours); 7] = [
            ("bt709", "testsrc2=s=1280x720", &["-colorspace", "bt709", "-color_range", "tv"], limited(Matrix::Bt709)),
            (
                "bt601",
                "testsrc2=s=720x480",
                &["-colorspace", "smpte170m", "-color_range", "tv"],
                limited(Matrix::Bt601),
            ),
            ("named-nothing", "testsrc2=s=1280x720", &[], limited(Matrix::Bt601)),
            (
                "full",
                "testsrc2=s=640x360",
                &["-colorspace", "bt709", "-color_range", "pc"],
                Colours { matrix: Matrix::Bt709, full_range: true },
            ),
            ("odd", "color=c=0x3080C0:s=321x181", &["-colorspace", "bt709"], limited(Matrix::Bt709)),
            ("larger", "color=c=0x3080C0:s=2560x1440", &["-colorspace", "bt709"], limited(Matrix::Bt709)),
            ("h264", "testsrc2=s=1280x720", &["-colorspace", "bt709", "-color_range", "tv"], limited(Matrix::Bt709)),
        ];
        for (name, source, tags, colours) in clips {
            let path = dir.join(format!("{name}.mkv"));
            let made = std::process::Command::new(&ffmpeg)
                .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
                .arg(format!("{source}:r=30:d=1"))
                .args(["-pix_fmt", "yuv420p"])
                .args(tags)
                .args(if name == "h264" { h264 } else { lossless })
                .arg(&path)
                .status()
                .expect("FFmpeg runs");
            if name == "h264" && !made.success() {
                eprintln!("this FFmpeg has no H.264 encoder: the H.264 video wasn't checked");
                continue;
            }
            assert!(made.success(), "the {name} video is made");
            let (pw, ph, from_planes, came) = first_picture(&path, Lend::Planes);
            assert_eq!(came, Some(colours), "{name}: planes, in the colours the video names");
            let (rw, rh, from_rgba, _) = first_picture(&path, Lend::Rows);
            if source.starts_with("color=") {
                let sizes = if name == "odd" { (321, 181, 320, 180) } else { (1920, 1080, 1920, 1080) };
                assert_eq!(
                    (pw, ph, rw, rh),
                    sizes,
                    "{name}: planes keep an odd size; RGBA is asked for with even sides"
                );
                let colour = [0x30, 0x80, 0xC0];
                let off = from_planes.iter().position(|rgb| rgb.iter().zip(colour).any(|(a, b)| a.abs_diff(b) > 2));
                assert!(off.is_none(), "{name}: pixel {off:?} is {:?}, not {colour:?}", off.map(|i| from_planes[i]));
                eprintln!("{name}: planes {:?}, FFmpeg's RGBA {:?}, of {colour:?}", from_planes[0], from_rgba[0]);
                continue;
            }
            assert_eq!((pw, ph), (rw, rh), "{name}: the same size either way");
            let apart = |(a, b): (&[u8; 3], &[u8; 3])| a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
            let worst = from_planes.iter().zip(&from_rgba).map(apart).max().unwrap_or(0);
            let shades: std::collections::HashSet<&[u8; 3]> = from_rgba.iter().collect();
            eprintln!("{name}: at most {worst} apart over {} colours", shades.len());
            assert!(shades.len() > 100, "{name}: a picture of many colours");
            // Within 3: FFmpeg's own conversion rounds a little differently from one version to
            // the next (Ubuntu's FFmpeg 6 comes 3 apart on bt709, the bundled 9 within 2), while
            // the wrong colour matrix would be 10 or more apart.
            assert!(worst <= 3, "{name}: the planes' colours are FFmpeg's, within 3: {worst} apart");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wide_video_is_asked_for_smaller_in_the_same_shape() {
        assert_eq!(smaller_than_wide(3840.0, 2160.0, 2048.0), Some((1920, 1080)));
        assert_eq!(smaller_than_wide(7680.0, 4320.0, 2048.0), Some((1920, 1080)));
        assert_eq!(smaller_than_wide(4096.0, 1716.0, 2048.0), Some((2048, 858)));
        assert_eq!(smaller_than_wide(5120.0, 1.0, 2048.0), Some((1707, 1)));
        // No wider than the most, or not a size at all: as it is.
        assert_eq!(smaller_than_wide(2048.0, 1152.0, 2048.0), None);
        assert_eq!(smaller_than_wide(1280.0, 720.0, 2048.0), None);
        assert_eq!(smaller_than_wide(f64::NAN, 720.0, 2048.0), None);
        assert_eq!(smaller_than_wide(f64::INFINITY, 720.0, 2048.0), None);
        assert_eq!(smaller_than_wide(3840.0, 0.0, 2048.0), None);
    }

    #[test]
    fn halved_rows_keep_every_second_pixel_in_the_order_it_came() {
        // Four pixels a row, two rows, three bytes of padding after each row; the buffer starts
        // with other bytes, which a copy of the right size replaces.
        let row = |first: u8| -> Vec<u8> { (first..first + 16).chain([99, 99, 99]).collect() };
        let rows = [row(0), row(100)].concat();
        let mut out = vec![7; 3];
        assert_eq!(halved_rows(&rows, 4, 2, 19, &mut out), (2, 1));
        assert_eq!(out, [0, 1, 2, 3, 8, 9, 10, 11]);
        // An odd width leaves its last pixel out; nothing at all is an empty copy.
        assert_eq!(halved_rows(&rows, 3, 2, 19, &mut out), (1, 1));
        assert_eq!(out, [0, 1, 2, 3]);
        assert_eq!(halved_rows(&rows, 1, 1, 19, &mut out), (0, 0));
        assert!(out.is_empty());
    }

    #[test]
    fn a_lent_picture_is_opaque_only_when_every_fourth_byte_is_full() {
        // Three pixels a row, two rows, two bytes of padding (which aren't pixels) after each.
        let rows = [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 0, 0, 1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 0, 0];
        fn lend(bytes: &[u8]) -> Lent<'_> {
            Lent { width: 3, height: 2, bytes_per_row: 14, bytes, order: PixelOrder::Bgra, opaque: true }
        }
        assert!(lend(&rows).fourth_bytes_full());
        // The last row may stop where its pixels do.
        assert!(lend(&rows[..26]).fourth_bytes_full());
        for at in [3, 11, 17, 25] {
            let mut clear = rows;
            clear[at] = 254;
            assert!(!lend(&clear).fourth_bytes_full(), "a fourth byte of 254 at {at}");
        }
        // Colours of 255 don't stand in for a fourth byte, and too few rows are not opaque.
        let mut bright = [255u8; 28];
        bright[7] = 0;
        assert!(!lend(&bright).fourth_bytes_full());
        assert!(!lend(&rows[..20]).fourth_bytes_full());
    }

    #[test]
    fn a_web_page_lends_its_picture_once_with_its_own_alpha() {
        let page = web::WebFrames::default();
        page.put(Frame { width: 1, height: 2, rgba: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        let frames = Frames::Web(page);
        let seen = frames.next_with(|lending| {
            let lent = lending.rows().expect("a web page lends rows");
            (lent.width, lent.height, lent.bytes_per_row, lent.bytes.to_vec(), lent.order, lent.opaque)
        });
        assert_eq!(seen, Some((1, 2, 4, vec![1, 2, 3, 4, 5, 6, 7, 8], PixelOrder::Rgba, false)));
        assert!(frames.next_with(|_| ()).is_none(), "the same picture is not lent twice");
    }

    #[test]
    fn a_black_opening_counts_as_dark_and_a_scene_does_not() {
        let black = Frame { width: 4, height: 4, rgba: [0, 0, 0, 255].repeat(16) };
        let scene = Frame { width: 4, height: 4, rgba: [90, 120, 60, 255].repeat(16) };
        assert!(dark(&black) && !dark(&scene));
    }

    #[test]
    fn premultiplied_colours_come_back_straight() {
        let mut pixels = [128, 64, 0, 128, 10, 20, 30, 255, 9, 9, 9, 0];
        unpremultiply(&mut pixels);
        assert_eq!(pixels, [255, 128, 0, 128, 10, 20, 30, 255, 9, 9, 9, 0]);
    }

    #[test]
    fn bgra_rows_become_rgba_and_wide_pictures_are_halved() {
        // Two pixels a row, two rows, with two bytes of padding at the end of each row.
        let rows = [1, 2, 3, 0, 4, 5, 6, 0, 99, 99, 7, 8, 9, 0, 10, 11, 12, 0, 99, 99];
        let frame = bgra_rows_to_rgba(&rows, 2, 2, 10, 1920);
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(frame.rgba, [3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]);
        let halved = bgra_rows_to_rgba(&rows, 2, 2, 10, 1);
        assert_eq!((halved.width, halved.height, halved.rgba), (1, 1, vec![3, 2, 1, 255]));
    }
}
