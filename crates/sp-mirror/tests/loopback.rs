//! The whole receiver, end to end over 127.0.0.1: the program is started as SoftPresenter starts
//! it, and a sender written here does what an iPhone does: asks what the receiver is, goes
//! through the FairPlay messages, sets up a session and a video stream, and sends encrypted
//! H.264 or HEVC pictures. Nothing here leaves this computer: the program is started without
//! `--announce`, so it listens on 127.0.0.1 only and announces nothing.
//!
//! The sender does not use the receiver's code. Its keys are worked out here from the
//! protocol's formulas, and its cipher is plain AES-128-CTR. The one thing it cannot do itself
//! is make a FairPlay message (only a real device can), so it sends a second message and a
//! scrambled key whose plain key is known from the original C (`src/fairplay/vectors.txt`).
//!
//! The pictures are a test pattern, encoded once with Apple's own encoder (as a phone's are)
//! and kept in `tests/data`. Decoding needs FFmpeg: the one SoftPresenter bundles
//! (`target/ffmpeg/bin/ffmpeg`, made by `scripts/build-ffmpeg.sh`), the one named by
//! `SOFTPRESENTER_TEST_FFMPEG`, or one on the PATH. Without any, the tests that decode say so
//! and pass; the ones that only speak the protocol always run.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use aes::cipher::{KeyIvInit, StreamCipher};
use plist::{Dictionary, Value};
use sha2::{Digest, Sha512};

const WAIT: Duration = Duration::from_secs(15);

fn ffmpeg() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SOFTPRESENTER_TEST_FFMPEG") {
        return Some(PathBuf::from(path));
    }
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    // The bundled build, in this checkout's `target` or (from a worktree) the main checkout's.
    for folder in std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors() {
        let bundled = folder.join("target").join("ffmpeg").join("bin").join(name);
        if bundled.is_file() {
            return Some(bundled);
        }
    }
    std::env::split_paths(&std::env::var_os("PATH")?).map(|folder| folder.join(name)).find(|path| path.is_file())
}

/// A picture as it came out on standard output.
struct Shown {
    width: u32,
    height: u32,
    session: u32,
    at: Instant,
    /// Not all one colour: something was really decoded.
    varied: bool,
}

/// The program, started as SoftPresenter starts it.
struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<serde_json::Value>,
    pictures: Receiver<Shown>,
    control: u16,
    video: u16,
}

impl Helper {
    fn start(ask: bool, ffmpeg: Option<PathBuf>) -> Helper {
        let mut child = Command::new(env!("CARGO_BIN_EXE_softpresenter-mirror"))
            .args(["--port", "0", "--ffmpeg"])
            .arg(ffmpeg.unwrap_or_else(|| PathBuf::from("no-ffmpeg-here")))
            .env_remove("SOFTPRESENTER_MIRROR_MODEL")
            .env_remove("SOFTPRESENTER_MIRROR_H264_ONLY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, r#"{{"hello":{{"name":"Test Receiver","device_id":"AA:BB:CC:DD:EE:01","ask":{ask}}}}}"#)
            .unwrap();
        let (say, events) = channel();
        let stderr = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            // No line may carry a key: not the plain key of any FairPlay vector used here.
            let keys: Vec<&str> = include_str!("../src/fairplay/vectors.txt")
                .lines()
                .filter_map(|line| line.rsplit(' ').next())
                .collect();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                assert!(!keys.iter().any(|key| line.to_lowercase().contains(key)), "a key was said: {line}");
                let event: serde_json::Value = serde_json::from_str(&line).expect("every line is JSON");
                if say.send(event).is_err() {
                    break;
                }
            }
        });
        let (show, pictures) = channel();
        let mut stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            loop {
                let mut header = [0_u8; 16];
                if stdout.read_exact(&mut header).is_err() {
                    break;
                }
                assert_eq!(&header[..4], b"SPMP");
                let number = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
                let (width, height, session) = (number(4), number(8), number(12));
                let mut bytes = vec![0_u8; width as usize * height as usize * 4];
                if stdout.read_exact(&mut bytes).is_err() {
                    break;
                }
                let at = Instant::now();
                let varied = bytes.chunks(4).any(|pixel| pixel[..3] != bytes[..3]);
                if show.send(Shown { width, height, session, at, varied }).is_err() {
                    break;
                }
            }
        });
        let mut helper = Helper { child, stdin: Some(stdin), events, pictures, control: 0, video: 0 };
        let listening = helper.expect("listening");
        // Never announced, in any test.
        assert_eq!(listening["announced"], false);
        helper.control = listening["port"].as_u64().unwrap() as u16;
        helper.video = listening["video_port"].as_u64().unwrap() as u16;
        helper
    }

    /// The next event of this kind; the events on the way are printed (they show under
    /// `--nocapture`).
    fn expect(&self, kind: &str) -> serde_json::Value {
        let until = Instant::now() + WAIT;
        loop {
            let left = until.checked_duration_since(Instant::now()).unwrap_or_else(|| panic!("no {kind} event came"));
            let event = self.events.recv_timeout(left).unwrap_or_else(|_| panic!("no {kind} event came"));
            if event["event"] == kind {
                return event;
            }
            println!("    {event}");
        }
    }

    fn say(&mut self, line: &str) {
        writeln!(self.stdin.as_mut().unwrap(), "{line}").unwrap();
    }

    /// Closes standard input and waits for the program to stop by itself.
    fn finish(mut self) {
        self.stdin = None;
        let until = Instant::now() + WAIT;
        while self.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until, "the program outlived its standard input");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2).map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()).collect()
}

/// A second FairPlay message, a scrambled key, and the plain key the original C gives.
fn fairplay_vector(line: usize) -> (Vec<u8>, Vec<u8>, [u8; 16]) {
    let text = include_str!("../src/fairplay/vectors.txt").lines().nth(line).unwrap();
    let mut parts = text.split(' ').map(hex);
    (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap().try_into().unwrap())
}

fn dict(pairs: Vec<(&str, Value)>) -> Value {
    let mut dictionary = Dictionary::new();
    for (key, value) in pairs {
        dictionary.insert(key.into(), value);
    }
    Value::Dictionary(dictionary)
}

fn plist_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    value.to_writer_binary(&mut out).unwrap();
    out
}

/// The sender's side of the control channel.
struct Sender {
    stream: TcpStream,
    cseq: u32,
}

impl Sender {
    fn connect(port: u16) -> Sender {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.set_read_timeout(Some(WAIT * 3)).unwrap();
        Sender { stream, cseq: 0 }
    }

    fn send(&mut self, method: &str, url: &str, content_type: Option<&str>, body: &[u8]) {
        self.cseq += 1;
        let mut head = format!("{method} {url} RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: AirPlay/test\r\n", self.cseq);
        if let Some(content_type) = content_type {
            head.push_str(&format!("Content-Type: {content_type}\r\n"));
        }
        head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
        self.stream.write_all(head.as_bytes()).unwrap();
        self.stream.write_all(body).unwrap();
    }

    /// The answer's status and body. `None` when the connection closed instead.
    fn answer(&mut self) -> Option<(u16, Vec<u8>)> {
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if self.stream.read(&mut byte).ok()? == 0 {
                return None;
            }
            head.push(byte[0]);
        }
        let head = String::from_utf8(head).unwrap();
        let status: u16 = head.split(' ').nth(1).unwrap().parse().unwrap();
        assert!(head.contains(&format!("CSeq: {}\r\n", self.cseq)), "{head}");
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .map_or(0, |length| length.parse::<usize>().unwrap());
        let mut body = vec![0_u8; length];
        self.stream.read_exact(&mut body).unwrap();
        Some((status, body))
    }

    fn request(&mut self, method: &str, url: &str, content_type: Option<&str>, body: &[u8]) -> (u16, Vec<u8>) {
        self.send(method, url, content_type, body);
        self.answer().expect("an answer")
    }

    /// `GET /info` and the two FairPlay messages; gives the stream's plain key.
    fn begin(&mut self, vector: usize) -> (Vec<u8>, [u8; 16]) {
        let (ekey, key, name, device_id) = self.begin_with_any(vector);
        assert_eq!(name, "Test Receiver");
        assert_eq!(device_id, "AA:BB:CC:DD:EE:01");
        (ekey, key)
    }

    /// As [`Sender::begin`], with a receiver of any name: gives the name and device id it says.
    fn begin_with_any(&mut self, vector: usize) -> (Vec<u8>, [u8; 16], String, String) {
        let (status, body) = self.request("GET", "/info", None, &[]);
        assert_eq!(status, 200);
        let info = Value::from_reader(std::io::Cursor::new(body)).unwrap();
        let info = info.as_dictionary().unwrap();
        let name = info.get("name").unwrap().as_string().unwrap().to_owned();
        let device_id = info.get("deviceID").unwrap().as_string().unwrap().to_owned();
        assert_eq!(info.get("model").unwrap().as_string(), Some("SoftPresenter1,1"));
        // Mirroring (bit 7), audio (bit 9), several codecs (bit 42); no legacy pairing (bit 27).
        let features = info.get("features").unwrap().as_unsigned_integer().unwrap();
        assert_eq!(features & (1 << 7 | 1 << 9 | 1 << 27 | 1 << 42), 1 << 7 | 1 << 9 | 1 << 42);
        let (message, ekey, key) = fairplay_vector(vector);
        let mut first = [0_u8; 16];
        first[..4].copy_from_slice(b"FPLY");
        first[4] = 3;
        first[5] = 1;
        first[6] = 1;
        first[14] = message[12];
        let (status, body) = self.request("POST", "/fp-setup", Some("application/octet-stream"), &first);
        assert_eq!((status, body.len()), (200, 142));
        let (status, body) = self.request("POST", "/fp-setup", Some("application/octet-stream"), &message);
        assert_eq!((status, body.len()), (200, 32));
        assert_eq!(&body[12..], &message[144..]);
        (ekey, key, name, device_id)
    }

    fn setup_session(&mut self, ekey: &[u8], name: &str) {
        let body = plist_bytes(&dict(vec![
            ("ekey", Value::Data(ekey.to_vec())),
            ("eiv", Value::Data(vec![3; 16])),
            ("deviceID", Value::String("11:22:33:44:55:66".into())),
            ("name", Value::String(name.into())),
            ("model", Value::String("iPhone99,9".into())),
            ("timingProtocol", Value::String("NTP".into())),
            ("timingPort", Value::Integer(0_u64.into())),
        ]));
        self.send("SETUP", "rtsp://127.0.0.1/1", Some("application/x-apple-binary-plist"), &body);
    }

    /// Asks for the video and sound streams; gives the video port answered.
    fn setup_streams(&mut self, connection_id: i64) -> u16 {
        let body = plist_bytes(&dict(vec![(
            "streams",
            Value::Array(vec![
                dict(vec![
                    ("type", Value::Integer(110_u64.into())),
                    ("streamConnectionID", Value::Integer(connection_id.into())),
                ]),
                dict(vec![
                    ("type", Value::Integer(96_u64.into())),
                    ("ct", Value::Integer(8_u64.into())),
                    ("spf", Value::Integer(480_u64.into())),
                    ("controlPort", Value::Integer(0_u64.into())),
                ]),
            ]),
        )]));
        let (status, body) =
            self.request("SETUP", "rtsp://127.0.0.1/1", Some("application/x-apple-binary-plist"), &body);
        assert_eq!(status, 200);
        let answer = Value::from_reader(std::io::Cursor::new(body)).unwrap();
        let streams = answer.as_dictionary().unwrap().get("streams").unwrap().as_array().unwrap();
        assert_eq!(streams.len(), 2);
        let sound = streams[1].as_dictionary().unwrap();
        assert_ne!(sound.get("dataPort").unwrap().as_unsigned_integer(), Some(0));
        streams[0].as_dictionary().unwrap().get("dataPort").unwrap().as_unsigned_integer().unwrap() as u16
    }
}

/// A test stream cut into its parameter sets and its pictures.
struct Clip {
    hevc: bool,
    width: u32,
    height: u32,
    parameter_sets: Vec<Vec<u8>>,
    pictures: Vec<Vec<Vec<u8>>>,
}

fn clip(bytes: &[u8], hevc: bool, width: u32, height: u32) -> Clip {
    // The NAL units, between start codes.
    let mut starts = Vec::new();
    let mut at = 0;
    while at + 3 <= bytes.len() {
        if bytes[at..at + 3] == [0, 0, 1] {
            starts.push(at + 3);
            at += 3;
        } else {
            at += 1;
        }
    }
    let mut clip = Clip { hevc, width, height, parameter_sets: Vec::new(), pictures: Vec::new() };
    let mut picture: Vec<Vec<u8>> = Vec::new();
    let mut has_slice = false;
    for (n, start) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(bytes.len(), |next| next - 3);
        while end > *start && bytes[end - 1] == 0 {
            end -= 1;
        }
        let unit = &bytes[*start..end];
        let (kind, is_set, is_slice, first_of_picture) = if hevc {
            let kind = (unit[0] >> 1) & 0x3f;
            (kind, (32..=34).contains(&kind), kind < 32, unit[2] & 0x80 != 0)
        } else {
            let kind = unit[0] & 0x1f;
            (kind, kind == 7 || kind == 8, kind == 1 || kind == 5, unit[1] & 0x80 != 0)
        };
        if has_slice && (!is_slice || first_of_picture) {
            clip.pictures.push(std::mem::take(&mut picture));
            has_slice = false;
        }
        if is_set {
            if clip.pictures.is_empty() {
                clip.parameter_sets.push(unit.to_vec());
            }
            continue;
        }
        // Delimiters are left out, as a phone's stream has none of its own.
        if (!hevc && kind == 9) || (hevc && kind == 35) {
            continue;
        }
        has_slice |= is_slice;
        picture.push(unit.to_vec());
    }
    if has_slice {
        clip.pictures.push(picture);
    }
    assert_eq!(clip.parameter_sets.len(), if hevc { 3 } else { 2 });
    clip
}

fn wide() -> Clip {
    clip(include_bytes!("data/wide.h264"), false, 320, 180)
}

fn tall() -> Clip {
    clip(include_bytes!("data/tall.h264"), false, 180, 320)
}

fn wide_hevc() -> Clip {
    clip(include_bytes!("data/wide.hevc"), true, 320, 180)
}

/// The sender's side of the video stream.
struct VideoOut {
    stream: TcpStream,
    cipher: ctr::Ctr128BE<aes::Aes128>,
}

impl VideoOut {
    /// The stream's key and IV are the first 16 bytes of SHA-512 of a label, the connection's
    /// number in decimal, and the session's key.
    fn connect(port: u16, session_key: &[u8; 16], connection_id: u64) -> VideoOut {
        let derive = |label: &str| {
            let mut hash = Sha512::new();
            hash.update(format!("{label}{connection_id}"));
            hash.update(session_key);
            let mut out = [0_u8; 16];
            out.copy_from_slice(&hash.finalize()[..16]);
            out
        };
        let (key, iv) = (derive("AirPlayStreamKey"), derive("AirPlayStreamIV"));
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        VideoOut { stream, cipher: ctr::Ctr128BE::<aes::Aes128>::new(&key.into(), &iv.into()) }
    }

    fn packet(&mut self, kind: u8, width: u32, height: u32, payload: &[u8]) {
        let mut header = [0_u8; 128];
        header[..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        header[4] = kind;
        for at in [16, 40, 56] {
            header[at..at + 4].copy_from_slice(&(width as f32).to_le_bytes());
            header[at + 4..at + 8].copy_from_slice(&(height as f32).to_le_bytes());
        }
        self.stream.write_all(&header).unwrap();
        self.stream.write_all(payload).unwrap();
    }

    /// The codec's settings, as a sender sends them: `avcC` for H.264, `hvc1` for HEVC.
    fn settings(&mut self, clip: &Clip) {
        let sets = &clip.parameter_sets;
        let payload = if clip.hevc {
            let mut out = vec![0_u8; 0x75];
            out[4..8].copy_from_slice(b"hvc1");
            for (marker, set) in [0xa0_u8, 0xa1, 0xa2].iter().zip(sets) {
                out.extend([*marker, 0, 1]);
                out.extend((set.len() as u16).to_be_bytes());
                out.extend(set);
            }
            out
        } else {
            let mut out = vec![1, sets[0][1], sets[0][2], sets[0][3], 0xff, 0xe1];
            out.extend((sets[0].len() as u16).to_be_bytes());
            out.extend(&sets[0]);
            out.push(1);
            out.extend((sets[1].len() as u16).to_be_bytes());
            out.extend(&sets[1]);
            out
        };
        self.packet(1, clip.width, clip.height, &payload);
    }

    /// One picture: its NAL units behind their lengths, encrypted.
    fn picture(&mut self, units: &[Vec<u8>]) {
        let mut payload = Vec::new();
        for unit in units {
            payload.extend((unit.len() as u32).to_be_bytes());
            payload.extend(unit);
        }
        self.cipher.apply_keystream(&mut payload);
        self.packet(0, 0, 0, &payload);
    }

    /// A once-a-second report, as a phone sends between pictures: read and dropped.
    fn report(&mut self) {
        self.packet(5, 0, 0, &vec![0x55; 25_000]);
    }
}

/// Sends a clip's pictures one every `gap`, and says how long each took to come back out, in
/// milliseconds. Every picture sent must come out, the last one too, with nothing after it.
fn play(helper: &Helper, video: &mut VideoOut, clip: &Clip, session: u32, gap: Duration) -> Vec<f64> {
    video.settings(clip);
    let size = helper.expect("picture_size");
    assert_eq!(size["codec"], if clip.hevc { "hevc" } else { "h264" });
    assert_eq!((size["width"].as_u64(), size["height"].as_u64()), (Some(clip.width.into()), Some(clip.height.into())));
    let mut delays = Vec::new();
    for (n, picture) in clip.pictures.iter().enumerate() {
        if n == 3 {
            video.report();
        }
        let sent = Instant::now();
        video.picture(picture);
        // The picture comes out before the next is sent: none is held back for a successor.
        let shown = helper.pictures.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| {
            panic!("picture {} of {} did not come out while it was the newest", n + 1, clip.pictures.len())
        });
        assert_eq!((shown.width, shown.height, shown.session), (clip.width, clip.height, session));
        assert!(shown.varied, "the picture is one flat colour");
        delays.push(shown.at.duration_since(sent).as_secs_f64() * 1000.0);
        std::thread::sleep(gap.saturating_sub(sent.elapsed()));
    }
    // And nothing more: one picture in, one picture out.
    assert!(helper.pictures.recv_timeout(Duration::from_millis(300)).is_err());
    delays
}

fn median(delays: &[f64]) -> f64 {
    let mut sorted = delays.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

macro_rules! need_ffmpeg {
    () => {
        match ffmpeg() {
            Some(path) => path,
            None => {
                println!("skipped: no FFmpeg (run scripts/build-ffmpeg.sh, or set SOFTPRESENTER_TEST_FFMPEG)");
                return;
            }
        }
    };
}

#[test]
fn a_sender_is_asked_about_then_mirrors_turns_and_stops() {
    let ffmpeg = need_ffmpeg!();
    let mut helper = Helper::start(true, Some(ffmpeg));
    let mut sender = Sender::connect(helper.control);
    let (ekey, key) = sender.begin(0);
    sender.setup_session(&ekey, "Anna\u{2019}s iPhone\r\n{\"event\":\"connected\"}");
    // The receiver holds its answer and asks; the name is cleaned and cannot pose as an event.
    let asks = helper.expect("asks");
    assert_eq!(asks["name"], "Anna\u{2019}s iPhone{\"event\":\"connected\"}");
    assert_eq!(asks["model"], "iPhone99,9");
    let id = asks["id"].as_u64().unwrap() as u32;
    // Nothing is answered, and the video port takes nobody, until the operator allows it.
    sender.stream.set_read_timeout(Some(Duration::from_millis(400))).unwrap();
    assert!(sender.answer().is_none());
    let mut early = TcpStream::connect((Ipv4Addr::LOCALHOST, helper.video)).unwrap();
    early.set_read_timeout(Some(WAIT)).unwrap();
    assert_eq!(early.read(&mut [0_u8; 1]).unwrap_or(0), 0, "the video port closes on a sender not yet allowed");
    sender.stream.set_read_timeout(Some(WAIT)).unwrap();
    helper.say(&format!(r#"{{"allow":{id}}}"#));
    let (status, _) = sender.answer().unwrap();
    assert_eq!(status, 200);
    assert_eq!(helper.expect("connected")["id"], id);
    // The stream's number may be negative; it is read as an unsigned one.
    let connection_id = -6_172_839_455_512_345_678_i64;
    let port = sender.setup_streams(connection_id);
    assert_eq!(port, helper.video);
    assert_eq!(sender.request("RECORD", "rtsp://127.0.0.1/1", None, &[]).0, 200);
    let mut video = VideoOut::connect(port, &key, connection_id as u64);
    let delays = play(&helper, &mut video, &wide(), id, Duration::from_millis(33));
    println!("    320x180 H.264: median {:.1} ms from sent to shown", median(&delays));
    // The phone is turned: new settings, a new size, and the pictures after it at that size.
    assert_eq!(sender.request("POST", "/feedback", None, &[]).0, 200);
    play(&helper, &mut video, &tall(), id, Duration::from_millis(33));
    // The sender stops, the way iOS 27 does: a TEARDOWN that names no stream.
    assert_eq!(sender.request("TEARDOWN", "rtsp://127.0.0.1/1", None, &[]).0, 200);
    let stopped = helper.expect("stopped");
    assert_eq!(stopped["id"], id);
    assert_eq!(stopped["name"], "Anna\u{2019}s iPhone{\"event\":\"connected\"}");
    helper.finish();
}

#[test]
fn hevc_pictures_are_shown_too() {
    let ffmpeg = need_ffmpeg!();
    let helper = Helper::start(false, Some(ffmpeg));
    let mut sender = Sender::connect(helper.control);
    let (ekey, key) = sender.begin(5);
    sender.setup_session(&ekey, "An iPad");
    assert_eq!(sender.answer().unwrap().0, 200);
    let id = helper.expect("connected")["id"].as_u64().unwrap() as u32;
    let port = sender.setup_streams(77);
    let mut video = VideoOut::connect(port, &key, 77);
    let delays = play(&helper, &mut video, &wide_hevc(), id, Duration::from_millis(33));
    println!("    320x180 HEVC: median {:.1} ms from sent to shown", median(&delays));
}

/// A sender that pairs first (as one does when the receiver says it supports legacy pairing,
/// and as some do anyway) mixes the shared secret into the key; the pictures still decode.
#[test]
fn a_sender_that_pairs_first_is_understood() {
    let ffmpeg = need_ffmpeg!();
    let helper = Helper::start(false, Some(ffmpeg));
    let mut sender = Sender::connect(helper.control);
    let random = ring::rand::SystemRandom::new();
    let identity = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
    let our_identity = ring::signature::KeyPair::public_key(&identity).as_ref().to_vec();
    let (status, their_identity) =
        sender.request("POST", "/pair-setup", Some("application/octet-stream"), &our_identity);
    assert_eq!((status, their_identity.len()), (200, 32));
    let private = ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::X25519, &random).unwrap();
    let our_key = private.compute_public_key().unwrap().as_ref().to_vec();
    let mut first = vec![1, 0, 0, 0];
    first.extend(&our_key);
    first.extend(&our_identity);
    let (status, answer) = sender.request("POST", "/pair-verify", Some("application/octet-stream"), &first);
    assert_eq!((status, answer.len()), (200, 96));
    let their_key = answer[..32].to_vec();
    let secret = ring::agreement::agree_ephemeral(
        private,
        &ring::agreement::UnparsedPublicKey::new(&ring::agreement::X25519, &their_key),
        |secret| secret.to_vec(),
    )
    .unwrap();
    let derive = |label: &[u8]| {
        let mut out = [0_u8; 16];
        out.copy_from_slice(&Sha512::digest([label, &secret].concat())[..16]);
        out
    };
    let mut cipher = ctr::Ctr128BE::<aes::Aes128>::new(
        &derive(b"Pair-Verify-AES-Key").into(),
        &derive(b"Pair-Verify-AES-IV").into(),
    );
    let mut their_signature = answer[32..].to_vec();
    cipher.apply_keystream(&mut their_signature);
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &their_identity)
        .verify(&[their_key.clone(), our_key.clone()].concat(), &their_signature)
        .expect("the receiver's signature");
    let mut signature = identity.sign(&[our_key, their_key].concat()).as_ref().to_vec();
    cipher.apply_keystream(&mut signature);
    let mut second = vec![0, 0, 0, 0];
    second.extend(&signature);
    assert_eq!(sender.request("POST", "/pair-verify", Some("application/octet-stream"), &second).0, 200);

    let (ekey, key) = sender.begin(9);
    sender.setup_session(&ekey, "A paired Mac");
    assert_eq!(sender.answer().unwrap().0, 200);
    let id = helper.expect("connected")["id"].as_u64().unwrap() as u32;
    let port = sender.setup_streams(4242);
    // The stream's key is made from SHA-512(key ‖ secret), not from the key itself.
    let mut paired = [0_u8; 16];
    paired.copy_from_slice(&Sha512::digest([key.as_slice(), &secret].concat())[..16]);
    let mut video = VideoOut::connect(port, &paired, 4242);
    play(&helper, &mut video, &wide(), id, Duration::from_millis(10));
}

#[test]
fn a_sender_the_operator_does_not_allow_gets_nothing() {
    let mut helper = Helper::start(true, None);
    for (answer, why) in [(None, "gone"), (Some("deny"), "denied")] {
        let mut sender = Sender::connect(helper.control);
        let (ekey, _) = sender.begin(1);
        sender.setup_session(&ekey, "Someone");
        let id = helper.expect("asks")["id"].as_u64().unwrap();
        match answer {
            Some(word) => {
                helper.say(&format!(r#"{{"{word}":{id}}}"#));
                assert_eq!(sender.answer().unwrap().0, 401);
            }
            // The sender gives up and hangs up: the receiver stops asking soon after.
            None => drop(sender),
        }
        let refused = helper.expect("refused");
        assert_eq!((refused["why"].as_str(), refused["id"].as_u64()), (Some(why), Some(id)));
        // The video port takes nobody.
        let mut video = TcpStream::connect((Ipv4Addr::LOCALHOST, helper.video)).unwrap();
        video.set_read_timeout(Some(WAIT)).unwrap();
        assert_eq!(video.read(&mut [0_u8; 1]).unwrap_or(0), 0);
    }
    // An address the operator has just refused is not asked about again for a minute: nobody
    // can keep a question in front of the operator.
    let mut sender = Sender::connect(helper.control);
    let (ekey, _) = sender.begin(1);
    sender.setup_session(&ekey, "Someone again");
    assert_eq!(sender.answer().unwrap().0, 453);
    // The operator is told once that it tried again, and not after that.
    let again = helper.expect("refused");
    assert_eq!((again["name"].as_str(), again["why"].as_str()), (Some("Someone again"), Some("denied_lately")));
    let mut sender = Sender::connect(helper.control);
    let (ekey, _) = sender.begin(1);
    sender.setup_session(&ekey, "Someone yet again");
    assert_eq!(sender.answer().unwrap().0, 453);
    assert!(helper.events.try_iter().all(|event| event["event"] == "log"));
    helper.finish();
}

#[test]
fn a_second_sender_is_told_it_is_busy_and_does_not_replace_the_first() {
    let mut helper = Helper::start(true, None);
    let mut first = Sender::connect(helper.control);
    let (ekey, _) = first.begin(2);
    first.setup_session(&ekey, "First");
    let id = helper.expect("asks")["id"].as_u64().unwrap();
    // While the first is still being asked about, and after it is allowed.
    for allowed in [false, true] {
        if allowed {
            helper.say(&format!(r#"{{"allow":{id}}}"#));
            assert_eq!(first.answer().unwrap().0, 200);
            helper.expect("connected");
        }
        let mut second = Sender::connect(helper.control);
        let (ekey, _) = second.begin(3);
        second.setup_session(&ekey, "Second");
        assert_eq!(second.answer().unwrap().0, 453);
        let refused = helper.expect("refused");
        assert_eq!((refused["name"].as_str(), refused["why"].as_str()), (Some("Second"), Some("busy")));
    }
    // The first is still there: it can ask for its stream.
    assert_eq!(first.setup_streams(1), helper.video);
    // The operator's Stop ends it.
    helper.say(r#"{"stop":true}"#);
    assert_eq!(helper.expect("stopped")["id"].as_u64(), Some(id));
    helper.finish();
}

#[test]
fn what_is_out_of_order_too_long_or_not_a_request_is_refused() {
    let helper = Helper::start(false, None);
    // SETUP before the FairPlay messages.
    let mut sender = Sender::connect(helper.control);
    sender.setup_session(&[1; 72], "Hasty");
    assert_eq!(sender.answer().unwrap().0, 400);
    // Streams before a session.
    let mut sender = Sender::connect(helper.control);
    sender.begin(4);
    let body =
        plist_bytes(&dict(vec![("streams", Value::Array(vec![dict(vec![("type", Value::Integer(110_u64.into()))])]))]));
    assert_eq!(sender.request("SETUP", "rtsp://x/1", Some("application/x-apple-binary-plist"), &body).0, 400);
    // FairPlay messages of the wrong length, version or mode.
    let mut sender = Sender::connect(helper.control);
    assert_eq!(sender.request("POST", "/fp-setup", None, &[0; 15]).0, 400);
    assert_eq!(sender.request("POST", "/fp-setup", None, &[2; 16]).0, 501);
    assert_eq!(sender.request("POST", "/fp-setup", None, &[9; 164]).0, 501);
    assert_eq!(sender.request("POST", "/pair-verify", None, &[1; 5]).0, 400);
    assert_eq!(sender.request("POST", "/pair-setup", None, &[1; 31]).0, 400);
    assert_eq!(sender.request("DESCRIBE", "rtsp://x/1", None, &[]).0, 501);
    assert_eq!(sender.request("SETUP", "rtsp://x/1", None, b"bplist00 not a list").0, 400);
    // A body longer than any may be: the connection closes without an answer.
    let mut sender = Sender::connect(helper.control);
    sender.stream.write_all(b"POST /fp-setup RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 100000000\r\n\r\n").unwrap();
    sender.cseq = 1;
    assert!(sender.answer().is_none());
    // Headers that never end.
    let mut sender = Sender::connect(helper.control);
    let _ = sender.stream.write_all(&vec![b'a'; 20_000]);
    assert!(sender.answer().is_none());
    // The receiver is still well: a proper sender is answered.
    let mut sender = Sender::connect(helper.control);
    sender.begin(4);
    helper.finish();
}

#[test]
fn a_video_packet_that_claims_too_much_ends_the_session() {
    let helper = Helper::start(false, None);
    let mut sender = Sender::connect(helper.control);
    let (ekey, key) = sender.begin(6);
    sender.setup_session(&ekey, "Greedy");
    assert_eq!(sender.answer().unwrap().0, 200);
    let id = helper.expect("connected")["id"].as_u64();
    let port = sender.setup_streams(5);
    let mut video = VideoOut::connect(port, &key, 5);
    // Settings that are not settings are passed over; then a header claiming four gigabytes.
    video.packet(1, 320, 180, &[0; 5]);
    let mut header = [0_u8; 128];
    header[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    video.stream.write_all(&header).unwrap();
    let stopped = helper.expect("stopped");
    assert_eq!(stopped["id"].as_u64(), id);
    assert_eq!(stopped["why"], "the video stream was not understood");
}

/// Not part of the usual run: measures how long a 1080p picture takes from the sender's socket
/// to SoftPresenter's side of the pipe, with a stream of your own.
///
/// `SOFTPRESENTER_MIRROR_CLIP=/path/clip.h264 cargo test -p sp-mirror --release -- --ignored --nocapture measure`
/// (a raw H.264 or `.hevc` stream of 1920×1080 with no B-frames; `SOFTPRESENTER_MIRROR_CLIP_FPS`
/// sets the pace, 30 if not given).
#[test]
#[ignore = "a measurement, with a stream that is not kept in the repository"]
fn measure_the_delay_of_a_1080p_stream() {
    let ffmpeg = need_ffmpeg!();
    let Some(path) = std::env::var_os("SOFTPRESENTER_MIRROR_CLIP").map(PathBuf::from) else {
        println!("skipped: SOFTPRESENTER_MIRROR_CLIP is not set");
        return;
    };
    let fps: u64 = std::env::var("SOFTPRESENTER_MIRROR_CLIP_FPS").ok().and_then(|fps| fps.parse().ok()).unwrap_or(30);
    let hevc = path.extension().is_some_and(|extension| extension == "hevc");
    let clip = clip(&std::fs::read(&path).unwrap(), hevc, 1920, 1080);
    let helper = Helper::start(false, Some(ffmpeg));
    let mut sender = Sender::connect(helper.control);
    let (ekey, key) = sender.begin(7);
    sender.setup_session(&ekey, "A measuring sender");
    assert_eq!(sender.answer().unwrap().0, 200);
    let id = helper.expect("connected")["id"].as_u64().unwrap() as u32;
    let port = sender.setup_streams(9);
    let mut video = VideoOut::connect(port, &key, 9);
    let delays = play(&helper, &mut video, &clip, id, Duration::from_micros(1_000_000 / fps));
    let mut sorted = delays[5..].to_vec();
    sorted.sort_by(f64::total_cmp);
    println!(
        "    {} pictures of 1920x1080 {} at {fps} a second: median {:.1} ms, 95th {:.1} ms, worst {:.1} ms from sent to shown; the last one came out with none after it",
        delays.len(),
        if hevc { "HEVC" } else { "H.264" },
        sorted[sorted.len() / 2],
        sorted[sorted.len() * 95 / 100],
        sorted[sorted.len() - 1],
    );
}

/// Not part of the usual run: a sender for a receiver that is already running, to try the whole
/// way through SoftPresenter itself (a quiet test copy with `sp mirror on`; `sp mirror` says
/// its port). It asks, waits to be allowed, then sends the test pattern over and over.
///
/// `SOFTPRESENTER_MIRROR_SEND_TO=<port> cargo test -p sp-mirror -- --ignored --nocapture send_to`
/// (`SOFTPRESENTER_MIRROR_SEND_SECONDS` is how long, 6 if not given; `SOFTPRESENTER_MIRROR_CLIP`
/// is a 1920×1080 stream of your own to send instead of the small one, as for the measuring
/// test). 127.0.0.1 only.
#[test]
#[ignore = "a sender for a receiver started by hand"]
fn send_to_a_receiver_that_is_running() {
    let Some(port) = std::env::var("SOFTPRESENTER_MIRROR_SEND_TO").ok().and_then(|port| port.parse::<u16>().ok())
    else {
        println!("skipped: SOFTPRESENTER_MIRROR_SEND_TO is not set");
        return;
    };
    let seconds: u64 =
        std::env::var("SOFTPRESENTER_MIRROR_SEND_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
    let mut sender = Sender::connect(port);
    let (ekey, key, name, _) = sender.begin_with_any(8);
    println!("    the receiver calls itself {name:?}; asking to show a screen");
    sender.setup_session(&ekey, "A test sender");
    let (status, _) = sender.answer().expect("an answer to SETUP");
    assert_eq!(status, 200, "the receiver did not allow the sender");
    let video_port = sender.setup_streams(31);
    assert_eq!(sender.request("RECORD", "rtsp://127.0.0.1/1", None, &[]).0, 200);
    let mut video = VideoOut::connect(video_port, &key, 31);
    let clip = match std::env::var_os("SOFTPRESENTER_MIRROR_CLIP").map(PathBuf::from) {
        Some(path) => {
            let hevc = path.extension().is_some_and(|extension| extension == "hevc");
            clip(&std::fs::read(&path).unwrap(), hevc, 1920, 1080)
        }
        None => wide(),
    };
    video.settings(&clip);
    let until = Instant::now() + Duration::from_secs(seconds);
    let mut sent = 0;
    let mut last_feedback = Instant::now();
    while Instant::now() < until {
        for picture in &clip.pictures {
            let started = Instant::now();
            video.picture(picture);
            sent += 1;
            std::thread::sleep(Duration::from_millis(33).saturating_sub(started.elapsed()));
        }
        if last_feedback.elapsed() > Duration::from_secs(2) {
            last_feedback = Instant::now();
            assert_eq!(sender.request("POST", "/feedback", None, &[]).0, 200);
        }
    }
    assert_eq!(sender.request("TEARDOWN", "rtsp://127.0.0.1/1", None, &[]).0, 200);
    println!("    sent {sent} pictures of {}x{} in {seconds} s and stopped", clip.width, clip.height);
}
