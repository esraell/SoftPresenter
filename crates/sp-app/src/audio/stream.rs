//! One track decoded ahead of where it plays, on a thread of its own, into a ring the sound
//! card's thread takes it from: the sound card's thread never decodes, opens a file or waits.
//!
//! A track is decoded by the app itself where it can (MP3, AAC in `.m4a`, FLAC, WAV, AIFF,
//! Ogg Vorbis): nothing else is started, and it works where FFmpeg isn't installed. Anything
//! else (Apple Lossless, Opus, WMA, AC-3, CAF and the rest) goes to FFmpeg, the copy in the app
//! or one on the computer, run as the video player runs it for a video's sound
//! ([`sp_video::SoundReader`]). Either way the sound comes out stereo at the sound card's rate,
//! made so once, here ([`super::resample`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::resample::{Frame, Resampler, fold};
use super::ring::Ring;

/// How much sound is decoded ahead: about two and a half seconds at 48 kHz.
pub const AHEAD: usize = 1 << 17;

/// Decoding rests once the ring is this full, and goes on when it has room for a good piece
/// again: it wakes some twenty times a second, not at every block the card plays.
const FULL_ENOUGH: usize = AHEAD * 3 / 4;

/// How long the decoding thread rests while the ring is full enough.
const REST: Duration = Duration::from_millis(50);

/// Frames decoded at a time.
const PIECE: usize = 4096;

/// How the track is being decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// By the app itself.
    Native,
    /// By FFmpeg.
    Ffmpeg,
}

impl Way {
    pub fn word(self) -> &'static str {
        match self {
            Way::Native => "native",
            Way::Ffmpeg => "FFmpeg",
        }
    }
}

const OPENING: u8 = 0;
const DECODING: u8 = 1;
const DECODED: u8 = 2;
const FAILED: u8 = 3;

/// What the decoding thread, the sound card's thread and the window share of one track.
#[derive(Debug)]
pub struct Shared {
    pub ring: Ring,
    /// Where in the track the decoding began.
    pub from: Duration,
    /// The sound card's rate the sound is made at.
    pub rate: u32,
    stop: AtomicBool,
    state: AtomicU8,
    way: AtomicU8,
    /// The track's length in milliseconds, once known (`u64::MAX` before, or never known).
    length_ms: AtomicU64,
    problem: Mutex<Option<String>>,
    /// FFmpeg, while it decodes: ended from here when the track is let go, which ends the
    /// thread reading it.
    child: Mutex<Option<std::process::Child>>,
    /// How long the first frames took to be ready, in microseconds (`u64::MAX` before).
    ready_after_us: AtomicU64,
    /// Frames the sound card's thread has played of it.
    pub played: AtomicU64,
    /// Set by the sound card's thread: its first frame has been played; its last has (or it
    /// was let go).
    pub began: AtomicBool,
    pub ended: AtomicBool,
    /// Set by the sound card's thread once it holds this track no longer.
    pub released: AtomicBool,
}

impl Shared {
    pub fn new(from: Duration, rate: u32) -> Shared {
        Shared {
            ring: Ring::new(AHEAD),
            from,
            rate,
            stop: AtomicBool::new(false),
            state: AtomicU8::new(OPENING),
            way: AtomicU8::new(0),
            length_ms: AtomicU64::new(u64::MAX),
            problem: Mutex::new(None),
            child: Mutex::new(None),
            ready_after_us: AtomicU64::new(u64::MAX),
            played: AtomicU64::new(0),
            began: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            released: AtomicBool::new(false),
        }
    }

    /// True once every frame has been decoded into the ring: what is in it is all there is.
    pub fn decoded(&self) -> bool {
        self.state.load(Ordering::Acquire) == DECODED
    }

    /// Why it can't be played, once that is known.
    pub fn failed(&self) -> Option<String> {
        (self.state.load(Ordering::Acquire) == FAILED)
            .then(|| self.problem.lock().ok().and_then(|problem| problem.clone()).unwrap_or_default())
    }

    /// True once it can't be played (asked on the sound card's thread: no lock).
    pub fn has_failed(&self) -> bool {
        self.state.load(Ordering::Acquire) == FAILED
    }

    /// Ready to begin: frames are waiting (or it is all decoded, or has failed).
    pub fn ready(&self) -> bool {
        self.ring.len() > 0 || matches!(self.state.load(Ordering::Acquire), DECODED | FAILED)
    }

    pub fn way(&self) -> Option<Way> {
        match self.way.load(Ordering::Relaxed) {
            1 => Some(Way::Native),
            2 => Some(Way::Ffmpeg),
            _ => None,
        }
    }

    pub fn length(&self) -> Option<Duration> {
        let ms = self.length_ms.load(Ordering::Relaxed);
        (ms != u64::MAX).then(|| Duration::from_millis(ms))
    }

    /// How long the first frames took to be ready, from the start.
    pub fn ready_after(&self) -> Option<Duration> {
        let us = self.ready_after_us.load(Ordering::Relaxed);
        (us != u64::MAX).then(|| Duration::from_micros(us))
    }

    /// Where in the track it has played to.
    pub fn position(&self) -> Duration {
        let played = self.played.load(Ordering::Relaxed) as f64 / f64::from(self.rate.max(1));
        self.from + Duration::from_secs_f64(played)
    }

    /// How much is left to play, when that is known: exactly once it is all decoded, from its
    /// length before.
    pub fn left(&self) -> Option<Duration> {
        if self.decoded() {
            return Some(Duration::from_secs_f64(self.ring.len() as f64 / f64::from(self.rate.max(1))));
        }
        self.length().map(|length| length.saturating_sub(self.position()))
    }

    /// Stops the decoding, and FFmpeg if it runs.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.child.lock().ok().and_then(|mut child| child.take()) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn fail(&self, problem: String) {
        if let Ok(mut said) = self.problem.lock() {
            *said = Some(problem);
        }
        self.state.store(FAILED, Ordering::Release);
    }
}

#[cfg(test)]
impl Shared {
    /// For tests: everything is in the ring.
    pub fn mark_decoded(&self) {
        self.state.store(DECODED, Ordering::Release);
    }

    /// For tests: more is still to come.
    pub fn mark_decoding(&self) {
        self.state.store(DECODING, Ordering::Release);
    }

    /// For tests: it can't be played.
    pub fn mark_failed(&self) {
        self.fail("broken".to_owned());
    }
}

/// A track decoding: what the window keeps. Dropping it stops the decoding (once the sound
/// card's thread holds it no longer: see `Player`).
#[derive(Debug)]
pub struct Stream {
    pub shared: Arc<Shared>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.shared.stop();
    }
}

/// Starts decoding a track from `from`, stereo at `rate`, on a thread of its own. `name` is
/// how the operator knows it, for what is said when it can't be played.
pub fn start(path: PathBuf, name: String, from: Duration, rate: u32) -> Stream {
    let shared = Arc::new(Shared::new(from, rate));
    let decoding = shared.clone();
    let named = name.clone();
    let started = std::thread::Builder::new().name("audio decode".to_owned()).spawn(move || {
        decode(&path, &named, &decoding, Instant::now());
    });
    if let Err(problem) = started {
        shared.fail(format!("{name}: the audio could not be started ({problem})."));
    }
    Stream { shared }
}

/// What decodes a track into frames: the app's own decoder, or FFmpeg.
enum Decoder {
    Native { decoder: Box<rodio::Decoder<std::io::BufReader<std::fs::File>>>, resampler: Resampler, channels: u16 },
    Ffmpeg { reader: sp_video::SoundReader },
}

fn decode(path: &Path, name: &str, shared: &Shared, began: Instant) {
    if let Err(problem) = std::fs::File::open(path) {
        return shared.fail(not_opened(name, &problem));
    }
    // The app's own decoder first; FFmpeg if it can't open the file, or opens it and finds no
    // sound it can decode.
    let native_problem = match open_native(path, shared) {
        Ok(decoder) => match run(decoder, shared, began) {
            Ran::Played | Ran::Stopped => return,
            Ran::Nothing => "it found no sound it could decode".to_owned(),
        },
        Err(problem) => problem,
    };
    if shared.stopped() {
        return;
    }
    let decoder = match open_ffmpeg(path, name, &native_problem, shared) {
        Ok(decoder) => decoder,
        Err(problem) => return shared.fail(problem),
    };
    if let Ran::Nothing = run(decoder, shared, began) {
        shared.fail(crate::i18n::tf(
            "tools.audio.ffmpeg_nothing",
            "{name}: FFmpeg could not decode its sound.",
            &[("name", name)],
        ));
    }
}

/// How decoding went.
enum Ran {
    /// To the end, with sound.
    Played,
    /// Told to stop.
    Stopped,
    /// To the end with no sound at all.
    Nothing,
}

/// Decodes into the ring until the end, resting while it is full enough.
fn run(mut decoder: Decoder, shared: &Shared, began: Instant) -> Ran {
    shared.state.store(DECODING, Ordering::Release);
    let mut frames: Vec<Frame> = Vec::with_capacity(PIECE * 2);
    let mut samples: Vec<f32> = Vec::with_capacity(PIECE * 8);
    let mut out: Vec<Frame> = Vec::with_capacity(PIECE * 2);
    let mut first = true;
    loop {
        if shared.stopped() {
            return Ran::Stopped;
        }
        if shared.ring.len() >= FULL_ENOUGH {
            std::thread::sleep(REST);
            continue;
        }
        out.clear();
        let more = match &mut decoder {
            Decoder::Native { decoder, resampler, channels } => {
                samples.clear();
                let wanted = PIECE * usize::from(*channels);
                samples.extend(decoder.by_ref().take(wanted));
                frames.clear();
                frames.extend(samples.chunks_exact(usize::from(*channels)).map(fold));
                resampler.push(&frames, &mut out);
                let more = samples.len() == wanted;
                if !more && !(first && out.is_empty()) {
                    resampler.finish(&mut out);
                }
                more
            }
            Decoder::Ffmpeg { reader } => {
                samples.clear();
                match reader.read(&mut samples) {
                    Ok(0) | Err(_) => false,
                    Ok(_) => {
                        out.extend(samples.as_chunks::<2>().0.iter().copied());
                        true
                    }
                }
            }
        };
        if first && !out.is_empty() {
            first = false;
            let us = u64::try_from(began.elapsed().as_micros()).unwrap_or(u64::MAX - 1);
            shared.ready_after_us.store(us, Ordering::Relaxed);
        }
        if !send(shared, &out) {
            return Ran::Stopped;
        }
        if !more {
            if first {
                return Ran::Nothing;
            }
            shared.state.store(DECODED, Ordering::Release);
            return Ran::Played;
        }
    }
}

/// Puts frames in the ring, waiting for room as it is played. False when told to stop.
fn send(shared: &Shared, frames: &[Frame]) -> bool {
    let mut sent = 0;
    while sent < frames.len() {
        if shared.stopped() {
            return false;
        }
        sent += shared.ring.put(&frames[sent..]);
        if sent < frames.len() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    true
}

/// Opens a track with the app's own decoder, from where it is to start.
fn open_native(path: &Path, shared: &Shared) -> Result<Decoder, String> {
    use rodio::Source;
    let mut decoder = native(path)?;
    if !shared.from.is_zero() && decoder.try_seek(shared.from).is_err() {
        return Err("it could not be played from that place".to_owned());
    }
    let (channels, rate) = (decoder.channels().max(1), decoder.sample_rate().max(1));
    shared.way.store(1, Ordering::Relaxed);
    if let Some(length) = decoder.total_duration() {
        shared.length_ms.store(length.as_millis() as u64, Ordering::Relaxed);
    }
    Ok(Decoder::Native { decoder: Box::new(decoder), resampler: Resampler::new(rate, shared.rate), channels })
}

/// Opens a track with FFmpeg, from where it is to start. The problem is a sentence for the
/// operator, saying why neither the app nor FFmpeg can play it.
fn open_ffmpeg(path: &Path, name: &str, native_problem: &str, shared: &Shared) -> Result<Decoder, String> {
    let info = sp_video::sound_info(path).map_err(|problem| not_playable(name, native_problem, problem))?;
    if let Some(length) = info.duration {
        shared.length_ms.store((length * 1000.0) as u64, Ordering::Relaxed);
    }
    let (child, reader) = sp_video::decode_sound(path, shared.from.as_secs_f64(), shared.rate)
        .map_err(|problem| format!("{name}: {problem}"))?;
    if let Ok(mut kept) = shared.child.lock() {
        *kept = Some(child);
    }
    // Let go already (the track was stopped while it opened): FFmpeg ends with it.
    if shared.stopped() {
        shared.stop();
    }
    shared.way.store(2, Ordering::Relaxed);
    Ok(Decoder::Ffmpeg { reader })
}

/// The app's own decoder for a file.
fn native(path: &Path) -> Result<rodio::Decoder<std::io::BufReader<std::fs::File>>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let mut builder =
        rodio::Decoder::builder().with_data(std::io::BufReader::new(file)).with_byte_len(length).with_seekable(true);
    if let Some(ending) = path.extension().and_then(|ending| ending.to_str()) {
        builder = builder.with_hint(&ending.to_lowercase());
    }
    builder.build().map_err(|e| e.to_string())
}

/// Says that a track's file can't be opened (it isn't there, say), in words for the operator.
fn not_opened(name: &str, problem: &std::io::Error) -> String {
    crate::i18n::tf(
        "tools.audio.not_opened",
        "Could not open {name}: {error}",
        &[("name", name), ("error", &problem.to_string())],
    )
}

/// Says why a track can't be played, in words for the operator.
fn not_playable(name: &str, native: &str, ffmpeg: sp_video::SoundProblem) -> String {
    use crate::i18n::tf;
    match ffmpeg {
        sp_video::SoundProblem::Protected => tf(
            "tools.audio.protected",
            "{name} is protected by Apple Music or the iTunes Store, and can't be played outside them.",
            &[("name", name)],
        ),
        sp_video::SoundProblem::NoSound => {
            tf("tools.audio.no_sound", "{name} has no sound in it to play.", &[("name", name)])
        }
        sp_video::SoundProblem::Unreadable(reason) => tf(
            "tools.audio.not_playable",
            "{name} is not audio SoftPresenter can play ({error}).",
            &[("name", name), ("error", &reason)],
        ),
        sp_video::SoundProblem::NoFfmpeg => tf(
            "tools.audio.needs_ffmpeg",
            "{name} can't be played: SoftPresenter can't read this kind of file by itself ({error}), and FFmpeg, which plays the rest, isn't on this computer.",
            &[("name", name), ("error", native)],
        ),
    }
}

/// Whether a file can be played, and how, as decoding it would find (for adding tracks): the
/// app's own decoder reads its first sound, or FFmpeg says what it is.
pub fn check(path: &Path, name: &str) -> Result<Way, String> {
    if let Err(problem) = std::fs::File::open(path) {
        return Err(not_opened(name, &problem));
    }
    let native_problem = match native(path).map(|mut decoder| decoder.next().is_some()) {
        Ok(true) => return Ok(Way::Native),
        Ok(false) => "it found no sound it could decode".to_owned(),
        Err(problem) => problem,
    };
    match sp_video::sound_info(path) {
        Ok(_) => Ok(Way::Ffmpeg),
        Err(problem) => Err(not_playable(name, &native_problem, problem)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Takes every frame a track decodes to, as fast as it is decoded, or the problem if it
    /// can't be played; how it was decoded; and how long that took.
    fn decode_all(path: &Path, from: Duration, rate: u32) -> (Result<Vec<Frame>, String>, Option<Way>, Duration) {
        let began = Instant::now();
        let name = path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let stream = start(path.to_owned(), name, from, rate);
        let shared = stream.shared.clone();
        let mut frames = Vec::new();
        let mut piece = vec![[0.0f32; 2]; 8192];
        loop {
            let all_there = shared.decoded();
            let got = shared.ring.take(&mut piece);
            frames.extend_from_slice(&piece[..got]);
            if let Some(problem) = shared.failed() {
                return (Err(problem), shared.way(), began.elapsed());
            }
            if all_there && got == 0 {
                return (Ok(frames), shared.way(), began.elapsed());
            }
            if got == 0 {
                std::thread::sleep(Duration::from_micros(200));
            }
            assert!(began.elapsed() < Duration::from_secs(60), "{} took too long", path.display());
        }
    }

    /// The pitch of a tone, from where it crosses zero going up, and its loudest sample.
    fn pitch_and_peak(frames: &[Frame], rate: u32) -> (f64, f32) {
        let skip = (rate / 10) as usize;
        let body = &frames[skip.min(frames.len())..frames.len().saturating_sub(skip).max(skip.min(frames.len()))];
        let ups: Vec<usize> = body
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0][0] <= 0.0 && pair[1][0] > 0.0)
            .map(|(at, _)| at)
            .collect();
        let pitch = match (ups.first(), ups.last()) {
            (Some(first), Some(last)) if ups.len() > 1 => {
                (ups.len() - 1) as f64 * f64::from(rate) / (last - first) as f64
            }
            _ => 0.0,
        };
        let peak = body.iter().map(|frame| frame[0].abs().max(frame[1].abs())).fold(0.0, f32::max);
        (pitch, peak)
    }

    /// A WAV file of a tone, written for a test: stereo, 16 bits, at half of full scale.
    fn wav(path: &Path, rate: u32, seconds: f64, hz: f64) {
        let count = (f64::from(rate) * seconds) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + count * 4).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 4).to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(count * 4).to_le_bytes());
        for n in 0..count {
            let value = ((2.0 * std::f64::consts::PI * hz * f64::from(n) / f64::from(rate)).sin() * 16384.0) as i16;
            bytes.extend_from_slice(&value.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_wave_file_is_decoded_ahead_at_the_cards_rate_at_its_pitch_and_from_a_place() {
        let dir = std::env::temp_dir().join(format!("sp-audio-stream-{}", sp_model::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Tone.wav");
        wav(&path, 44_100, 2.0, 1000.0);
        let (frames, way, _) = decode_all(&path, Duration::ZERO, 48_000);
        let frames = frames.unwrap();
        assert_eq!(way, Some(Way::Native));
        // Two seconds at 48 kHz, give or take a frame.
        assert!(frames.len().abs_diff(96_000) <= 2, "{}", frames.len());
        let (pitch, peak) = pitch_and_peak(&frames, 48_000);
        assert!((pitch - 1000.0).abs() < 0.5, "{pitch} Hz");
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
        // From a place: what is left.
        let (frames, _, _) = decode_all(&path, Duration::from_millis(1500), 48_000);
        let frames = frames.unwrap();
        assert!(frames.len().abs_diff(24_000) <= 600, "{}", frames.len());
        // A file that isn't there says so, in words.
        let (missing, _, _) = decode_all(&dir.join("None.wav"), Duration::ZERO, 48_000);
        assert!(missing.is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Plays every file of a folder through the decoding the player uses, and says for each
    /// how it was decoded, its length, its pitch and level (the files are 1 kHz tones), and how
    /// much faster than it plays it was decoded, or why it can't be played. Run by hand:
    /// `SOFTPRESENTER_AUDIO_MATRIX=<folder> cargo test -p sp-app matrix -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a folder of sound files (SOFTPRESENTER_AUDIO_MATRIX) and FFmpeg"]
    fn every_file_in_the_matrix_plays_at_its_pitch_and_length_or_says_why() {
        let Some(dir) = std::env::var_os("SOFTPRESENTER_AUDIO_MATRIX") else { return };
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(dir).unwrap().filter_map(|entry| entry.ok().map(|entry| entry.path())).collect();
        files.sort();
        let rate = 48_000;
        println!("| File | Decoded by | Seconds | Pitch | Peak | Took | Faster than it plays |");
        println!("|---|---|---|---|---|---|---|");
        for path in files {
            let name = path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let (frames, way, took) = decode_all(&path, Duration::ZERO, rate);
            let way = way.map_or("-", Way::word);
            match frames {
                Ok(frames) => {
                    let seconds = frames.len() as f64 / f64::from(rate);
                    let (pitch, peak) = pitch_and_peak(&frames, rate);
                    let speed = seconds / took.as_secs_f64().max(1e-6);
                    println!(
                        "| {name} | {way} | {seconds:.3} | {pitch:.1} Hz | {peak:.3} | {} ms | {speed:.0}x |",
                        took.as_millis()
                    );
                }
                Err(problem) => {
                    println!("| {name} | {way} | can't be played: {problem} | | | {} ms | |", took.as_millis())
                }
            }
        }
    }
}
