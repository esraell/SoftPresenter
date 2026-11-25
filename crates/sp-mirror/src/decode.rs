//! Decoding: an FFmpeg child for each size of picture a sender sends, and handing the newest
//! decoded picture to SoftPresenter on standard output.
//!
//! FFmpeg is told exactly what it reads (`-f h264` or `-f hevc`, never guessed), and no word of
//! its command line comes from a sender: only the codec, and two numbers worked out here.
//!
//! Two things keep the newest picture from waiting for the next one (docs/airplay-plan.md has
//! the measurements): an access unit delimiter is written after every picture
//! ([`crate::stream::annex_b`]), so FFmpeg's reader knows the picture has ended; and FFmpeg's
//! raw-video writer is given one thread (`-threads 1` as an *output* option), because with more
//! it keeps each picture until the next arrives.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use crate::stream::{Codec, Settings};

/// The largest picture handed on. A sender's picture is scaled down to fit, never up.
pub const MAX_WIDTH: u32 = 1920;
pub const MAX_HEIGHT: u32 = 1080;

/// The words in front of each picture on standard output.
pub const MAGIC: &[u8; 4] = b"SPMP";

/// The 16 bytes in front of a picture: the magic, then the width, the height and the session,
/// each as four little-endian bytes. `width * height * 4` bytes of RGBA follow.
pub fn picture_header(session: u32, width: u32, height: u32) -> [u8; 16] {
    let mut header = [0_u8; 16];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&width.to_le_bytes());
    header[8..12].copy_from_slice(&height.to_le_bytes());
    header[12..].copy_from_slice(&session.to_le_bytes());
    header
}

/// FFmpeg's command line for a stream. Everything is fixed but the codec and the size shown.
pub fn ffmpeg_args(codec: Codec, width: u32, height: u32) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-flags",
        "low_delay",
        "-probesize",
        "32",
        "-analyzeduration",
        "0",
        "-f",
        codec.name(),
        "-i",
        "pipe:0",
        "-an",
        "-sn",
        "-dn",
        "-threads",
        "1",
        "-vf",
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();
    args.push(format!("scale={width}:{height}"));
    args.extend(["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"].iter().map(|word| (*word).to_owned()));
    args
}

struct Picture {
    session: u32,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct Slot {
    newest: Option<Picture>,
    spare: Option<Vec<u8>>,
}

/// The newest decoded picture, waiting to be written out. Nothing is queued: a picture that
/// arrives while the one before has not been taken replaces it.
#[derive(Default)]
pub struct Pictures {
    slot: Mutex<Slot>,
    ready: Condvar,
    /// Pictures decoded, and pictures replaced before they were written.
    pub decoded: AtomicU64,
    pub replaced: AtomicU64,
}

impl Pictures {
    fn put(&self, picture: Picture) {
        self.decoded.fetch_add(1, Ordering::Relaxed);
        let Ok(mut slot) = self.slot.lock() else { return };
        if let Some(old) = slot.newest.replace(picture) {
            self.replaced.fetch_add(1, Ordering::Relaxed);
            slot.spare = Some(old.bytes);
        }
        self.ready.notify_one();
    }

    /// A buffer of `len` bytes: the one last written out, if there is one.
    fn buffer(&self, len: usize) -> Vec<u8> {
        let mut buffer = self.slot.lock().ok().and_then(|mut slot| slot.spare.take()).unwrap_or_default();
        buffer.resize(len, 0);
        buffer
    }

    /// Writes each newest picture to `out` until writing fails (SoftPresenter has gone).
    pub fn write_to(&self, mut out: impl Write) {
        loop {
            let picture = {
                let Ok(mut slot) = self.slot.lock() else { return };
                loop {
                    if let Some(picture) = slot.newest.take() {
                        break picture;
                    }
                    let Ok(next) = self.ready.wait(slot) else { return };
                    slot = next;
                }
            };
            let header = picture_header(picture.session, picture.width, picture.height);
            if out.write_all(&header).and_then(|()| out.write_all(&picture.bytes)).and_then(|()| out.flush()).is_err() {
                return;
            }
            if let Ok(mut slot) = self.slot.lock() {
                slot.spare = Some(picture.bytes);
            }
        }
    }
}

/// One FFmpeg child, decoding one stream at one size.
pub struct Decoder {
    child: Child,
    stdin: Option<ChildStdin>,
    threads: Vec<std::thread::JoinHandle<()>>,
    alive: Arc<AtomicBool>,
    pub width: u32,
    pub height: u32,
}

impl Decoder {
    /// Starts FFmpeg for a stream with these settings and writes the settings to it. Its
    /// pictures go to `pictures` under the session's number.
    pub fn start(
        ffmpeg: &Path,
        settings: &Settings,
        session: u32,
        pictures: Arc<Pictures>,
    ) -> std::io::Result<Decoder> {
        let (width, height) = crate::stream::shown_size(settings.width, settings.height, MAX_WIDTH, MAX_HEIGHT);
        let mut command = Command::new(ffmpeg);
        command
            .args(ffmpeg_args(settings.codec, width, height))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            // No console window for the child.
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let alive = Arc::new(AtomicBool::new(true));
        let mut threads = Vec::new();
        if let Some(mut stdout) = stdout {
            let alive = alive.clone();
            let started = Instant::now();
            threads.push(std::thread::spawn(move || {
                let len = width as usize * height as usize * 4;
                let mut first = true;
                loop {
                    let mut bytes = pictures.buffer(len);
                    if stdout.read_exact(&mut bytes).is_err() {
                        break;
                    }
                    if first {
                        first = false;
                        crate::events::log(format!(
                            "the first picture came out of FFmpeg {} ms after it started",
                            started.elapsed().as_millis()
                        ));
                    }
                    pictures.put(Picture { session, width, height, bytes });
                }
                alive.store(false, Ordering::Relaxed);
            }));
        }
        if let Some(stderr) = stderr {
            threads.push(std::thread::spawn(move || say_ffmpeg_lines(stderr)));
        }
        if let Some(stdin) = stdin.as_mut() {
            stdin.write_all(&settings.parameter_sets)?;
        }
        Ok(Decoder { child, stdin, threads, alive, width, height })
    }

    /// Gives FFmpeg a picture. `false` when FFmpeg has stopped.
    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        if !self.alive.load(Ordering::Relaxed) {
            return false;
        }
        match self.stdin.as_mut() {
            Some(stdin) => stdin.write_all(bytes).and_then(|()| stdin.flush()).is_ok(),
            None => false,
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// Passes on the first few things FFmpeg says, cleaned and cut, and reads the rest away.
fn say_ffmpeg_lines(stderr: impl Read) {
    use std::io::BufRead;
    let mut said = 0;
    let mut reader = std::io::BufReader::new(stderr.take(1 << 20));
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.by_ref().take(512).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if said < 12 {
            said += 1;
            let text = String::from_utf8_lossy(&line);
            crate::events::log(format!("FFmpeg: {}", crate::events::clean_to(&text, 200, "(nothing)")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_is_told_the_format_and_given_one_output_thread() {
        let args = ffmpeg_args(Codec::Hevc, 498, 1080);
        let at = |word: &str| args.iter().position(|arg| arg == word).unwrap();
        // The format is named, before the input.
        assert_eq!(args[at("-f") + 1], "hevc");
        assert!(at("-f") < at("-i"));
        assert_eq!(args[at("-i") + 1], "pipe:0");
        // One thread for the raw-video writer, after the input: the held picture's cure.
        assert!(at("-threads") > at("-i"));
        assert_eq!(args[at("-threads") + 1], "1");
        assert_eq!(args[at("-vf") + 1], "scale=498:1080");
        assert_eq!(&args[args.len() - 3..], ["-f", "rawvideo", "pipe:1"]);
        assert_eq!(ffmpeg_args(Codec::H264, 16, 16)[at("-f") + 1], "h264");
        // Nothing but fixed words and the two numbers.
        assert!(args.iter().all(|arg| arg.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_=:".contains(&b))));
    }

    #[test]
    fn a_picture_that_was_not_taken_is_replaced_by_the_next() {
        let pictures = Arc::new(Pictures::default());
        for n in 0..3_u8 {
            pictures.put(Picture { session: 7, width: 2, height: 1, bytes: vec![n; 8] });
        }
        assert_eq!(pictures.decoded.load(Ordering::Relaxed), 3);
        assert_eq!(pictures.replaced.load(Ordering::Relaxed), 2);
        // The writer writes the newest, then fails on the next write and stops.
        struct Once(Vec<u8>, usize);
        impl Write for Once {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.1 == 0 {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.1 -= 1;
                Ok(())
            }
        }
        let mut out = Once(Vec::new(), 1);
        pictures.put(Picture { session: 7, width: 2, height: 1, bytes: vec![9; 8] });
        let writer = pictures.clone();
        std::thread::scope(|scope| {
            scope.spawn(|| writer.write_to(&mut out));
            std::thread::sleep(std::time::Duration::from_millis(50));
            pictures.put(Picture { session: 7, width: 2, height: 1, bytes: vec![1; 8] });
        });
        let mut expected = picture_header(7, 2, 1).to_vec();
        expected.extend([9; 8]);
        assert_eq!(out.0, expected);
        assert_eq!(&out.0[..4], b"SPMP");
    }
}
