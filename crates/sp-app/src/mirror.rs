//! AirPlay: showing an iPhone's, iPad's or Mac's screen. Someone picks SoftPresenter in Screen
//! Mirroring (AirPlay) on their device, and its screen shows in the AirPlay boxes of the slide on
//! screen.
//!
//! Everything that speaks to the device is in a separate program, `softpresenter-mirror`, which
//! sits beside SoftPresenter's own as FFmpeg does: this module only starts it while the feature
//! is switched on (Settings › AirPlay; off until someone switches it on), tells it who may
//! connect, and reads what it says. No network connection is opened here. Between the two:
//!
//! - its standard input takes one JSON object a line: `{"hello":{…}}` first (the name devices
//!   see, a made-up device id, whether to ask), then `{"allow":N}`, `{"deny":N}`,
//!   `{"stop":true}`. Closing it makes the program leave, so it never outlives the app;
//! - its standard error gives one JSON object a line ([`Event`]): listening, a device asks,
//!   connected, stopped, refused, a line for the log, a problem in plain words;
//! - its standard output gives pictures only: a 16-byte header ([`picture_header`]) and the
//!   picture's RGBA bytes. A thread reads them and hands each to the renderer through the door
//!   NDI sources use (`sp_render::show_mirror`); nothing is queued, the newest wins.
//!
//! A device can never put itself on a screen: its picture shows only where the operator has put
//! a box and taken the slide live. With "Ask me each time" (the default) the operator is asked
//! in the notices before a device's picture is taken at all. In a quiet run
//! (`SOFTPRESENTER_QUIET`, or one that only saves a picture: the app's own `quiet`) the program
//! is never told to announce itself and listens on this computer only.
//!
//! Unofficial: this works with Apple's Screen Mirroring but isn't made or supported by Apple,
//! and an Apple update may stop it working; Settings says so where it is switched on.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::control::Reply;
use crate::i18n::{t, tf};
use crate::outside::Waiting;
use crate::quick_help::Hint;

/// The helper program's name, beside SoftPresenter's own.
const PROGRAM: &str = "softpresenter-mirror";
/// The longest name of a device that is shown or logged, in characters.
const NAME_MOST: usize = 60;
/// The longest line read from the helper; a longer one is skipped.
const LINE_MOST: u64 = 64 * 1024;
/// The first bytes of a picture's header.
const PICTURE_MARK: &[u8; 4] = b"SPMP";
/// The smallest and largest side of a picture taken from the helper, and the most bytes.
const SIDE_LEAST: u32 = 16;
const SIDE_MOST: u32 = 4096;
const BYTES_MOST: u64 = 4096 * 2304 * 4;
/// How often a connected device's last picture is kept from fading while its screen is still.
const KEEP_EVERY: Duration = Duration::from_millis(500);
/// How long the helper is given to leave by itself once its input is closed.
const LEAVE_WITHIN: Duration = Duration::from_millis(1500);
/// A helper that ran this long before it stopped is started again at once, as if it never failed.
const RAN_WELL: Duration = Duration::from_secs(60);

/// What is kept in `mirror.json` in the data folder. Nothing secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// Switched on: the helper runs, and (outside a quiet run) devices can find this computer.
    #[serde(default)]
    pub on: bool,
    /// The name devices see; empty for "SoftPresenter — <this computer's name>".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Ask the operator each time a device wants to show its screen.
    #[serde(default = "yes")]
    pub ask: bool,
    /// A made-up device id (shaped like a network card's address, never the real one), made
    /// once so that devices see the same receiver from one day to the next.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_id: String,
}

fn yes() -> bool {
    true
}

impl Default for Saved {
    fn default() -> Saved {
        Saved { on: false, name: String::new(), ask: true, device_id: String::new() }
    }
}

/// A made-up device id: six random bytes written as a network card's address, marked as one
/// that no maker gave out (locally administered, for one device).
fn new_device_id() -> String {
    let random = sp_model::Uuid::new_v4();
    let mut bytes = [0_u8; 6];
    bytes.copy_from_slice(&random.as_bytes()[..6]);
    bytes[0] = (bytes[0] | 0x02) & 0xfe;
    bytes.map(|byte| format!("{byte:02X}")).join(":")
}

/// Whether text is a device id as [`new_device_id`] writes one.
fn is_device_id(text: &str) -> bool {
    let parts: Vec<&str> = text.split(':').collect();
    parts.len() == 6 && parts.iter().all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A name as it is shown, logged or sent: without control characters, cut to [`NAME_MOST`]
/// characters, with no space at either end. A device's name is only something it says.
pub fn clean_name(name: &str) -> String {
    let kept: String = name.chars().filter(|c| !c.is_control()).take(NAME_MOST).collect();
    kept.trim().to_owned()
}

/// This computer's name as its owner wrote it ("Front Mac"), for the name devices see.
fn computer_name() -> Option<String> {
    let raw = if cfg!(target_os = "macos") {
        Command::new("/usr/sbin/scutil")
            .args(["--get", "ComputerName"])
            .stdin(Stdio::null())
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        std::env::var("COMPUTERNAME")
            .ok()
            .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
            .map(|name| name.trim().split('.').next().unwrap_or("").to_owned())
    }?;
    let name = clean_name(&raw);
    (!name.is_empty()).then_some(name)
}

/// The name devices see when none was written: "SoftPresenter — Front Mac".
fn default_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| match computer_name() {
        Some(computer) => clean_name(&format!("SoftPresenter \u{2014} {computer}")),
        None => "SoftPresenter".to_owned(),
    })
}

fn settings_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("mirror.json"))
}

/// Where the helper program is: beside SoftPresenter's own program (in the app on a Mac, in the
/// folder on Windows and Linux, in `target/<profile>` for a build from source). Nowhere else:
/// it is only ever SoftPresenter's own.
pub fn helper_program() -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{PROGRAM}.exe") } else { PROGRAM.to_owned() };
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [dir.join(&file), dir.join("../Resources").join(&file)].into_iter().find(|place| place.is_file())
}

/// The command line the helper is started with. Nothing a device says, and nothing the operator
/// typed, is ever part of it: the name goes in on its standard input.
fn helper_arguments(ffmpeg: &Path, quiet: bool, port: Option<u16>) -> Vec<std::ffi::OsString> {
    let mut arguments: Vec<std::ffi::OsString> = vec!["--ffmpeg".into(), ffmpeg.into()];
    if quiet {
        arguments.push("--port".into());
        arguments.push(port.unwrap_or(0).to_string().into());
    } else {
        arguments.push("--announce".into());
    }
    arguments
}

/// The first line the helper is given.
fn hello_line(name: &str, device_id: &str, ask: bool) -> String {
    let mut line = serde_json::json!({ "hello": { "name": name, "device_id": device_id, "ask": ask } }).to_string();
    line.push('\n');
    line
}

/// What the helper says, a line at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// It listens: the control port, the video port, and whether it announced itself.
    Listening { port: u16, video_port: u16, announced: bool },
    /// A device wants to show its screen and waits for an answer.
    Asks { id: u64, name: String },
    /// A device was let in.
    Connected { id: u64, name: String },
    /// The size of the pictures now coming.
    PictureSize { width: u32, height: u32, codec: String },
    /// A device that was showing its screen has gone.
    Stopped { id: u64, name: String, why: String },
    /// A device was not let in: `busy`, `denied` or `timeout`.
    Refused { name: String, why: String },
    /// A line for the log.
    Log(String),
    /// A problem, in plain words, for the operator.
    Problem(String),
    /// Anything else: logged and otherwise ignored.
    Unknown(String),
}

/// Reads one line of the helper's. A line that is not what is expected is [`Event::Unknown`],
/// never a stop; names are cleaned and every text is cut to a length.
pub fn parse_event(line: &str) -> Event {
    let cut = |text: &str, most: usize| -> String { text.chars().filter(|c| !c.is_control()).take(most).collect() };
    let unknown = || Event::Unknown(cut(line, 200));
    let Ok(serde_json::Value::Object(said)) = serde_json::from_str::<serde_json::Value>(line) else {
        return unknown();
    };
    let text = |key: &str, most: usize| said.get(key).and_then(|v| v.as_str()).map(|text| cut(text, most));
    let number = |key: &str| said.get(key).and_then(serde_json::Value::as_u64);
    let name = || {
        let name = clean_name(said.get("name").and_then(|v| v.as_str()).unwrap_or(""));
        if name.is_empty() { t("mirror.a_device", "A device").to_owned() } else { name }
    };
    let port = |key: &str| number(key).and_then(|n| u16::try_from(n).ok());
    let side = |key: &str| number(key).and_then(|n| u32::try_from(n).ok());
    let event = match said.get("event").and_then(|v| v.as_str()) {
        Some("listening") => port("port").zip(port("video_port")).map(|(port, video_port)| Event::Listening {
            port,
            video_port,
            announced: said.get("announced").and_then(serde_json::Value::as_bool).unwrap_or(false),
        }),
        Some("asks") => number("id").map(|id| Event::Asks { id, name: name() }),
        Some("connected") => number("id").map(|id| Event::Connected { id, name: name() }),
        Some("picture_size") => side("width").zip(side("height")).map(|(width, height)| Event::PictureSize {
            width,
            height,
            codec: text("codec", 16).unwrap_or_default(),
        }),
        Some("stopped") => {
            number("id").map(|id| Event::Stopped { id, name: name(), why: text("why", 200).unwrap_or_default() })
        }
        Some("refused") => Some(Event::Refused { name: name(), why: text("why", 40).unwrap_or_default() }),
        Some("log") => text("text", 400).map(Event::Log),
        Some("problem") => text("text", 300).map(Event::Problem),
        _ => None,
    };
    event.unwrap_or_else(unknown)
}

/// What a picture's 16-byte header says: its width, height and session, or why the bytes are
/// not a header this app takes. Checked before anything is set aside for the picture.
pub fn picture_header(header: &[u8; 16]) -> Result<(u32, u32, u32), &'static str> {
    if &header[..4] != PICTURE_MARK {
        return Err("not a picture's header");
    }
    let number = |at: usize| u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]);
    let (width, height, session) = (number(4), number(8), number(12));
    if !(SIDE_LEAST..=SIDE_MOST).contains(&width) || !(SIDE_LEAST..=SIDE_MOST).contains(&height) {
        return Err("a picture of a size that is not taken");
    }
    if u64::from(width) * u64::from(height) * 4 > BYTES_MOST {
        return Err("a picture too large to take");
    }
    Ok((width, height, session))
}

/// One line of at most [`LINE_MOST`] bytes; the rest of a longer one is read and left out.
/// `None` at the end.
fn read_line(reader: &mut impl BufRead) -> Option<String> {
    let mut line = Vec::new();
    let read = reader.by_ref().take(LINE_MOST).read_until(b'\n', &mut line).ok()?;
    if read == 0 {
        return None;
    }
    if line.last() != Some(&b'\n') && read as u64 == LINE_MOST {
        // Too long: the rest of it is thrown away, in pieces.
        let mut rest = Vec::new();
        loop {
            rest.clear();
            match reader.by_ref().take(LINE_MOST).read_until(b'\n', &mut rest) {
                Ok(0) | Err(_) => break,
                Ok(_) if rest.last() == Some(&b'\n') => break,
                Ok(_) => {}
            }
        }
        return Some(String::new());
    }
    Some(String::from_utf8_lossy(&line).trim_end().to_owned())
}

/// What the picture thread and the app share.
#[derive(Default)]
struct Shared {
    /// A slide on screen or being edited has a box for the pictures: each is shown as it comes.
    wanted: AtomicBool,
    /// How many pictures have come.
    pictures: AtomicU64,
    /// The next picture is a device's first: the app is woken for it.
    first_due: AtomicBool,
    /// A device's first picture has come and the thumbnails have not been told.
    first_came: AtomicBool,
    /// The newest picture, kept while no box wants it, for the moment one does (a still screen
    /// sends no other).
    latest: Mutex<Option<sp_render::Picture>>,
    /// The helper sent something that is no picture: it is stopped and started again.
    broken: AtomicBool,
}

type Waker = Arc<dyn Fn() + Send + Sync>;

/// The helper while it runs.
struct Helper {
    child: Child,
    input: ChildStdin,
    events: Receiver<Event>,
    since: Instant,
}

impl Helper {
    fn say(&mut self, line: &serde_json::Value) -> bool {
        let mut text = line.to_string();
        text.push('\n');
        self.input.write_all(text.as_bytes()).and_then(|()| self.input.flush()).is_ok()
    }

    /// Closes its input, which tells it to leave, and makes sure it has, on a thread of its own.
    fn leave(self) {
        let Helper { mut child, input, .. } = self;
        drop(input);
        std::thread::spawn(move || {
            let until = Instant::now() + LEAVE_WITHIN;
            while Instant::now() < until {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    }
}

fn read_events(errors: impl Read, to: &Sender<Event>, wake: &Waker) {
    let mut reader = BufReader::new(errors);
    while let Some(line) = read_line(&mut reader) {
        if line.trim().is_empty() {
            continue;
        }
        if to.send(parse_event(&line)).is_err() {
            return;
        }
        wake();
    }
}

fn read_pictures(mut pictures: impl Read, shared: &Shared, wake: &Waker) {
    loop {
        let mut header = [0_u8; 16];
        if pictures.read_exact(&mut header).is_err() {
            return;
        }
        let Ok((width, height, _session)) = picture_header(&header) else {
            shared.broken.store(true, Ordering::Release);
            wake();
            return;
        };
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        if pictures.read_exact(&mut pixels).is_err() {
            return;
        }
        let Some(picture) = sp_render::Picture::from_rgba(width, height, pixels) else { continue };
        if shared.wanted.load(Ordering::Acquire) {
            sp_render::show_mirror(picture.clone());
        }
        if let Ok(mut latest) = shared.latest.lock() {
            *latest = Some(picture);
        }
        shared.pictures.fetch_add(1, Ordering::AcqRel);
        if shared.first_due.swap(false, Ordering::AcqRel) {
            shared.first_came.store(true, Ordering::Release);
            wake();
        }
    }
}

/// What the app does with mirrored screens.
pub struct Mirror {
    saved: Saved,
    /// A quiet run: the helper is never told to announce itself.
    quiet: bool,
    shared: Arc<Shared>,
    helper: Option<Helper>,
    /// How many times running it has failed in a row, and when to try again.
    failures: u32,
    retry_at: Option<Instant>,
    /// The devices waiting for an answer.
    asking: Vec<(u64, String)>,
    /// The device showing its screen.
    connected: Option<(u64, String)>,
    /// The control and video ports it listens on, and whether it announced itself.
    listening: Option<(u16, u16, bool)>,
    /// The size of the pictures now coming, and their codec.
    size: Option<(u32, u32, String)>,
    /// Why it isn't running though switched on.
    problem: Option<String>,
    /// What the renderer was last told: whether a box wants pictures, and whether nobody shows any.
    told: Option<(bool, bool)>,
    /// How many pictures had come when the Edit view's slide was last drawn for one, and when.
    drawn: (u64, Option<Instant>),
    /// The switch was looked at as the app opened.
    started: bool,
    /// The helper program is beside the app (looked for as the app opens, and at each start).
    found: bool,
}

/// What Settings › AirPlay and the inspector show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub on: bool,
    /// The name field, and the name devices see now.
    pub name: String,
    pub name_now: String,
    pub ask: bool,
    /// The helper program is there.
    pub found: bool,
    /// The device showing its screen.
    pub connected: Option<String>,
    /// The control and video ports, once it listens.
    pub ports: Option<(u16, u16)>,
    /// It listens on this computer only (a quiet run).
    pub local_only: bool,
    pub problem: Option<String>,
    pub request: Option<Request>,
}

/// What Settings › AirPlay asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Switch(bool),
    Name(String),
    Ask(bool),
    Stop,
}

impl Mirror {
    /// Reads the settings; nothing runs yet. `quiet` is the app's own word for a run that must
    /// stay out of the way (`SOFTPRESENTER_QUIET`, or a run that only saves a picture): its
    /// helper listens on this computer only (127.0.0.1) and announces nothing, so a test copy
    /// never shows up on anyone's phone.
    pub fn open(quiet: bool) -> Mirror {
        let saved = settings_file()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| serde_json::from_str::<Saved>(&text).ok())
            .map(|mut saved| {
                saved.name = clean_name(&saved.name);
                if !is_device_id(&saved.device_id) {
                    saved.device_id.clear();
                }
                saved
            })
            .unwrap_or_default();
        Mirror {
            saved,
            quiet,
            shared: Arc::default(),
            helper: None,
            failures: 0,
            retry_at: None,
            asking: Vec::new(),
            connected: None,
            listening: None,
            size: None,
            problem: None,
            told: None,
            drawn: (0, None),
            started: false,
            found: helper_program().is_some(),
        }
    }

    fn save(&self) {
        let Some(file) = settings_file() else { return };
        let Ok(text) = serde_json::to_vec_pretty(&self.saved) else { return };
        let beside = file.with_extension("json.new");
        if std::fs::write(&beside, text).and_then(|()| std::fs::rename(&beside, &file)).is_err() {
            crate::applog::problem("mirror", "Could not save the AirPlay settings.");
        }
    }

    /// The name devices see.
    pub fn name(&self) -> String {
        if self.saved.name.is_empty() { default_name().to_owned() } else { self.saved.name.clone() }
    }

    /// The device showing its screen now.
    pub fn connected(&self) -> Option<&str> {
        self.connected.as_ref().map(|(_, name)| name.as_str())
    }

    /// Starts the helper. A problem (it isn't there, FFmpeg isn't, it would not start) is kept
    /// for the pane and returned.
    fn start(&mut self, wake: &Waker) -> Result<(), String> {
        if self.helper.is_some() {
            return Ok(());
        }
        let started = self.spawn(wake);
        if let Err(problem) = &started {
            crate::applog::problem("mirror", problem);
        }
        self.problem = started.as_ref().err().cloned();
        started
    }

    fn spawn(&mut self, wake: &Waker) -> Result<(), String> {
        let program = helper_program();
        self.found = program.is_some();
        let program = program.ok_or_else(|| {
            t(
                "mirror.problem.no_program",
                "The part of SoftPresenter that receives AirPlay (softpresenter-mirror) isn\u{2019}t beside the app.",
            )
            .to_owned()
        })?;
        let ffmpeg = sp_video::ffmpeg_program().ok_or_else(|| {
            t("mirror.problem.no_ffmpeg", "AirPlay needs FFmpeg, and FFmpeg isn\u{2019}t on this computer.").to_owned()
        })?;
        if self.saved.device_id.is_empty() {
            self.saved.device_id = new_device_id();
            self.save();
        }
        // Testing aid: SOFTPRESENTER_MIRROR_PORT is the port a quiet run's helper listens on
        // (on this computer only), so a test's own sender knows where to connect; any free
        // port without it.
        let port = std::env::var("SOFTPRESENTER_MIRROR_PORT").ok().and_then(|port| port.parse::<u16>().ok());
        let mut command = Command::new(&program);
        command
            .args(helper_arguments(&ffmpeg, self.quiet, port))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let could_not = |why: &dyn std::fmt::Display| {
            tf("mirror.problem.start", "AirPlay could not start: {why}.", &[("why", &why.to_string())])
        };
        let mut child = command.spawn().map_err(|e| could_not(&e))?;
        let (Some(mut input), Some(output), Some(errors)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            return Err(could_not(&"no pipes"));
        };
        if input.write_all(hello_line(&self.name(), &self.saved.device_id, self.saved.ask).as_bytes()).is_err() {
            let _ = child.kill();
            return Err(could_not(&"it left at once"));
        }
        let _ = input.flush();
        let (to, events) = std::sync::mpsc::channel();
        let waker = wake.clone();
        std::thread::Builder::new()
            .name("mirror events".to_owned())
            .spawn(move || {
                read_events(errors, &to, &waker);
                // The end of its words: the app looks, and finds it gone.
                waker();
            })
            .map_err(|e| could_not(&e))?;
        let (shared, waker) = (self.shared.clone(), wake.clone());
        self.shared.broken.store(false, Ordering::Release);
        std::thread::Builder::new()
            .name("mirror pictures".to_owned())
            .spawn(move || read_pictures(output, &shared, &waker))
            .map_err(|e| could_not(&e))?;
        crate::applog::event(
            "mirror",
            &format!(
                "started the helper{}",
                if self.quiet { " (quiet: this computer only, not announced)" } else { "" }
            ),
        );
        self.helper = Some(Helper { child, input, events, since: Instant::now() });
        Ok(())
    }

    /// The app is leaving: the helper is told to leave too (it would anyway, its input closing
    /// with the app).
    pub fn quit(&mut self) {
        self.stop();
    }

    /// Stops the helper and forgets who was there.
    fn stop(&mut self) {
        if let Some(helper) = self.helper.take() {
            helper.leave();
            crate::applog::event("mirror", "stopped the helper");
        }
        self.asking.clear();
        self.connected = None;
        self.listening = None;
        self.size = None;
        if let Ok(mut latest) = self.shared.latest.lock() {
            *latest = None;
        }
    }

    fn say(&mut self, line: &serde_json::Value) -> bool {
        self.helper.as_mut().is_some_and(|helper| helper.say(line))
    }

    /// What the helper has said since last looked at; `None` in place of the events when it has
    /// gone (its words ended, or it sent what is no picture).
    fn take_events(&mut self) -> (Vec<Event>, bool) {
        let mut events = Vec::new();
        let mut gone = false;
        if let Some(helper) = self.helper.as_mut() {
            loop {
                match helper.events.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        gone = true;
                        break;
                    }
                }
            }
            gone |= self.shared.broken.load(Ordering::Acquire) || matches!(helper.child.try_wait(), Ok(Some(_)));
        }
        (events, gone)
    }

    fn view(&self, view: &mut View) -> bool {
        let before = view.clone();
        view.on = self.saved.on;
        view.name_now = self.name();
        if view.name.is_empty() {
            view.name = view.name_now.clone();
        }
        view.ask = self.saved.ask;
        view.found = self.found;
        view.connected = self.connected().map(str::to_owned);
        view.ports = self.listening.map(|(port, video, _)| (port, video));
        view.local_only = self.listening.is_some_and(|(_, _, announced)| !announced);
        view.problem = self.problem.clone();
        *view != before
    }

    /// The state as `sp mirror` says it.
    fn status(&self) -> Reply {
        let name = self.name();
        let text = match (self.saved.on, self.connected(), &self.problem) {
            (false, _, _) => "AirPlay is switched off.".to_owned(),
            (true, _, Some(problem)) => format!("Switched on, but not running: {problem}"),
            (true, Some(device), _) => format!("{device} is showing its screen. Devices see \"{name}\"."),
            (true, None, _) => match self.listening {
                Some((port, _, true)) => {
                    format!("Switched on: devices see \"{name}\" (port {port}). Nobody is connected.")
                }
                Some((port, _, false)) => {
                    format!("Switched on, on this computer only (port {port}, not announced). Nobody is connected.")
                }
                None => "Switched on; starting.".to_owned(),
            },
        };
        let asking: Vec<serde_json::Value> =
            self.asking.iter().map(|(id, name)| serde_json::json!({ "id": id, "name": name })).collect();
        let data = serde_json::json!({
            "on": self.saved.on,
            "name": name,
            "ask": self.saved.ask,
            "running": self.helper.is_some(),
            "port": self.listening.map(|(port, _, _)| port),
            "video_port": self.listening.map(|(_, video, _)| video),
            "announced": self.listening.is_some_and(|(_, _, announced)| announced),
            "connected": self.connected(),
            "asking": asking,
            "picture": self.size.as_ref().map(|(width, height, codec)| serde_json::json!({ "width": width, "height": height, "codec": codec })),
            "pictures": self.shared.pictures.load(Ordering::Acquire),
            "problem": self.problem,
        });
        Reply::with(text, data)
    }
}

/// Whether any of these objects is an AirPlay box that is shown.
pub fn wanted_in(objects: &[sp_model::Object]) -> bool {
    objects.iter().any(|object| !object.hidden && matches!(object.kind, sp_model::ObjectKind::Mirror { .. }))
}

/// The line the inspector shows under an AirPlay box's name.
pub fn line(view: &View) -> String {
    match (view.on, &view.connected) {
        (false, _) => {
            t("mirror.box.off", "AirPlay is switched off. Switch it on in Settings \u{203a} AirPlay.").to_owned()
        }
        (true, Some(name)) => tf(
            "mirror.box.connected",
            "{name} is showing its screen. It shows here, live, while the slide is on screen.",
            &[("name", name)],
        ),
        (true, None) => tf(
            "mirror.box.waiting",
            "Nobody is showing a screen yet. On the iPhone or iPad, open Control Centre, tap Screen Mirroring and choose {name}.",
            &[("name", &view.name_now)],
        ),
    }
}

/// Settings › AirPlay.
pub fn pane(ui: &mut egui::Ui, view: &mut View) {
    use crate::studio::{CARE, TEXT, TEXT_DIM};
    use egui::RichText;
    let width = ui.available_width().min(600.0);
    ui.set_max_width(width);
    crate::studio::caption(ui, t("mirror.caption", "AIRPLAY"));
    let mut on = view.on;
    ui.add_enabled_ui(view.found || view.on, |ui| {
        if ui
            .checkbox(&mut on, t("mirror.turn_on", "Show an iPhone, iPad or Mac screen"))
            .hint(t("quickhelp.mirror.on", "Let a phone, iPad or Mac show its screen through SoftPresenter."))
            .changed()
        {
            view.request = Some(Request::Switch(on));
        }
    });
    // The owner's words, always in view where this is switched on.
    ui.label(
        RichText::new(t(
            "mirror.unofficial",
            "Unofficial: works with Screen Mirroring (AirPlay) from an iPhone, iPad or Mac, but isn\u{2019}t made or supported by Apple, and an Apple update may stop it working.",
        ))
        .color(TEXT),
    );
    if !view.found {
        ui.add_space(6.0);
        ui.label(
            RichText::new(t(
                "mirror.problem.no_program",
                "The part of SoftPresenter that receives AirPlay (softpresenter-mirror) isn\u{2019}t beside the app.",
            ))
            .color(CARE),
        );
    } else if let Some(problem) = &view.problem {
        ui.add_space(6.0);
        ui.label(RichText::new(problem).color(CARE));
    }
    ui.add_space(10.0);
    ui.label(
        RichText::new(tf(
            "mirror.how",
            "On the iPhone or iPad, open Control Centre, tap Screen Mirroring and choose {name}. The screen then shows in an AirPlay box on the slide that is on screen, and nowhere else.",
            &[("name", &view.name_now)],
        ))
        .color(TEXT_DIM),
    );
    ui.add_space(12.0);

    crate::studio::caption(ui, t("mirror.name.caption", "THE NAME DEVICES SEE"));
    ui.horizontal(|ui| {
        let field = ui.add(egui::TextEdit::singleline(&mut view.name).desired_width(300.0).char_limit(NAME_MOST)).hint(
            t("quickhelp.mirror.name", "The name phones and iPads see when they look for a screen to mirror to."),
        );
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let changed = clean_name(&view.name) != view.name_now;
        let use_it = ui
            .add_enabled(changed, egui::Button::new(t("mirror.name.use", "Use")))
            .hint(t("quickhelp.mirror.use", "Use the name typed."))
            .reason(t("quickhelp.mirror.use.same", "Type a new name first"));
        if (use_it.clicked() || (entered && changed)) && !clean_name(&view.name).is_empty() {
            view.request = Some(Request::Name(clean_name(&view.name)));
        }
    });
    ui.add_space(12.0);

    crate::studio::caption(ui, t("mirror.who.caption", "WHO MAY CONNECT"));
    let mut ask = view.ask;
    if ui
        .checkbox(&mut ask, t("mirror.ask", "Ask me each time"))
        .hint(t(
            "mirror.ask.hint",
            "When a device wants to show its screen, SoftPresenter asks you in the notices before taking its picture",
        ))
        .changed()
    {
        view.request = Some(Request::Ask(ask));
    }
    if !view.ask {
        ui.label(
            RichText::new(t(
                "mirror.ask.off",
                "Anyone on this network can connect without being asked. Their screen still shows only where you put a box and take the slide live.",
            ))
            .color(CARE),
        );
    }
    ui.add_space(12.0);

    crate::studio::caption(ui, t("mirror.now.caption", "CONNECTED NOW"));
    match &view.connected {
        Some(name) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(TEXT));
                if ui
                    .button(t("mirror.stop", "Stop"))
                    .hint(t("quickhelp.mirror.stop", "End this device\u{2019}s screen mirroring."))
                    .clicked()
                {
                    view.request = Some(Request::Stop);
                }
            });
        }
        None => {
            ui.label(RichText::new(t("mirror.nobody", "Nobody")).color(TEXT_DIM));
        }
    }
    ui.add_space(12.0);

    ui.label(
        RichText::new(t(
            "mirror.network",
            "The device and this computer must be on the same network. A guest Wi-Fi that keeps devices apart won\u{2019}t let the device find this computer.",
        ))
        .color(TEXT_DIM),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(t(
            "mirror.everything",
            "Whatever is on the device\u{2019}s screen is shown, messages that arrive too. There is no sound yet, and films from streaming apps may show black.",
        ))
        .color(TEXT_DIM),
    );
    if let Some((port, video)) = view.ports {
        ui.add_space(4.0);
        let (control, video) = (port.to_string(), video.to_string());
        let ports = if view.local_only {
            tf(
                "mirror.ports.local",
                "Listening on this computer only (a test run): ports {control} and {video}.",
                &[("control", &control), ("video", &video)],
            )
        } else {
            tf(
                "mirror.ports",
                "Listening on ports {control} and {video} (TCP), and a few UDP ports beside them; a firewall must let them in.",
                &[("control", &control), ("video", &video)],
            )
        };
        ui.label(RichText::new(ports).color(TEXT_DIM));
    }
}

impl crate::App {
    fn mirror_waker(&self) -> Waker {
        let proxy = self.waker.clone();
        Arc::new(move || {
            if let Some(proxy) = &proxy {
                let _ = proxy.send_event(crate::AppEvent::Mirror);
            }
        })
    }

    /// Whether the audience or stage screens show an AirPlay box now (the slide on
    /// screen, its layout behind it, the stage layouts).
    pub(crate) fn live_mirror_wanted(&self) -> bool {
        let mut wanted = false;
        if let Some((_, slide)) = self.show.live_slide() {
            wanted |= wanted_in(&slide.objects);
            wanted |= self.live_layout_objects().is_some_and(|behind| wanted_in(&behind.objects));
        }
        if let Some(running) = self.running.as_ref() {
            wanted |= wanted_in(&crate::stage::shown_objects(&running.home_state));
        }
        wanted
    }

    fn editing_mirror(&self) -> bool {
        self.running
            .as_ref()
            .and_then(|running| running.home_state.editing.as_ref())
            .is_some_and(|draft| wanted_in(&draft.slide.objects))
    }

    fn mirror_switch(&mut self, on: bool) -> Result<(), String> {
        self.mirror.saved.on = on;
        self.mirror.save();
        self.mirror.failures = 0;
        self.mirror.retry_at = None;
        if on {
            crate::applog::event("mirror", "Switched on.");
            let wake = self.mirror_waker();
            self.mirror.start(&wake)
        } else {
            crate::applog::event("mirror", "Switched off.");
            self.mirror_settle_all();
            self.mirror.stop();
            self.mirror.problem = None;
            Ok(())
        }
    }

    /// The helper is started afresh, for a new name or a new rule about asking.
    fn mirror_restart(&mut self) {
        if self.mirror.helper.is_some() {
            self.mirror_settle_all();
            self.mirror.stop();
            let wake = self.mirror_waker();
            let _ = self.mirror.start(&wake);
        }
    }

    fn mirror_settle_all(&mut self) {
        if let Some(running) = self.running.as_mut() {
            for (id, _) in &self.mirror.asking {
                running.home_state.notices.settle(Waiting::Mirror(*id));
            }
        }
    }

    /// Allow or Don't Allow, answered in the notices (or by `sp`): the helper is told, and the
    /// question goes.
    pub(crate) fn mirror_answer(&mut self, id: u64, allow: bool) -> Result<String, String> {
        let Some(at) = self.mirror.asking.iter().position(|(asking, _)| *asking == id) else {
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.settle(Waiting::Mirror(id));
            }
            return Err(t("mirror.answer.gone", "That device is no longer waiting.").to_owned());
        };
        let (_, name) = self.mirror.asking.remove(at);
        let sent = self.mirror.say(&if allow {
            serde_json::json!({ "allow": id })
        } else {
            serde_json::json!({ "deny": id })
        });
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.settle(Waiting::Mirror(id));
            running.home_window.request_redraw();
        }
        crate::applog::event("mirror", &format!("{name}: {}", if allow { "allowed" } else { "not allowed" }));
        if !sent {
            return Err(t("mirror.answer.gone", "That device is no longer waiting.").to_owned());
        }
        Ok(if allow {
            tf("mirror.answer.allowed", "{name} may show its screen.", &[("name", &name)])
        } else {
            tf("mirror.answer.denied", "{name} was not let in.", &[("name", &name)])
        })
    }

    fn mirror_event(&mut self, event: Event) {
        let Some(running) = self.running.as_mut() else { return };
        let notices = &mut running.home_state.notices;
        match event {
            Event::Listening { port, video_port, announced } => {
                self.mirror.listening = Some((port, video_port, announced));
                crate::applog::event(
                    "mirror",
                    &format!(
                        "listening on port {port} (video {video_port}), {}",
                        if announced { "announced on the network" } else { "this computer only, not announced" }
                    ),
                );
            }
            Event::Asks { id, name } => {
                if !self.mirror.asking.iter().any(|(asking, _)| *asking == id) {
                    self.mirror.asking.push((id, name.clone()));
                }
                notices.ask_to_show(id, tf("mirror.asks", "{name} wants to show its screen.", &[("name", &name)]));
            }
            Event::Connected { id, name } => {
                self.mirror.asking.retain(|(asking, _)| *asking != id);
                notices.settle(Waiting::Mirror(id));
                crate::applog::event("mirror", &format!("{name} is connected"));
                // Nobody was asked, so this happened out of sight.
                if !self.mirror.saved.ask {
                    notices.info(tf("mirror.connected", "{name} is showing its screen.", &[("name", &name)]));
                }
                // The last device's picture is not this one's.
                sp_render::forget_mirror();
                if let Ok(mut latest) = self.mirror.shared.latest.lock() {
                    *latest = None;
                }
                self.mirror.told = None;
                self.mirror.shared.first_due.store(true, Ordering::Release);
                self.mirror.connected = Some((id, name));
            }
            Event::PictureSize { width, height, codec } => {
                crate::applog::event("mirror", &format!("pictures of {width} x {height} ({codec})"));
                self.mirror.size = Some((width, height, codec));
            }
            Event::Stopped { id, name, why } => {
                self.mirror.asking.retain(|(asking, _)| *asking != id);
                notices.settle(Waiting::Mirror(id));
                crate::applog::event("mirror", &format!("{name} stopped ({why})"));
                if self.mirror.connected.as_ref().is_some_and(|(connected, _)| *connected == id) {
                    self.mirror.connected = None;
                    self.mirror.size = None;
                    notices.info(tf("mirror.stopped", "{name} stopped showing its screen.", &[("name", &name)]));
                }
            }
            Event::Refused { name, why } => {
                // The question about a device that gave up, or was not answered in time, goes.
                let waited: Vec<u64> =
                    self.mirror.asking.iter().filter(|(_, asking)| *asking == name).map(|(id, _)| *id).collect();
                if why != "busy" {
                    for id in &waited {
                        notices.settle(Waiting::Mirror(*id));
                    }
                    self.mirror.asking.retain(|(id, _)| !waited.contains(id));
                }
                crate::applog::event("mirror", &format!("{name} was not let in ({why})"));
                match why.as_str() {
                    "busy" => notices.info(tf(
                        "mirror.refused.busy",
                        "{name} tried to show its screen, but another device is showing one.",
                        &[("name", &name)],
                    )),
                    "timeout" => notices.info(tf(
                        "mirror.refused.timeout",
                        "{name} wanted to show its screen and wasn\u{2019}t answered in time. It can try again.",
                        &[("name", &name)],
                    )),
                    "denied_lately" => notices.info(tf(
                        "mirror.refused.lately",
                        "{name} tried again and was turned away: you chose Don\u{2019}t Allow less than a minute ago.",
                        &[("name", &name)],
                    )),
                    _ => {}
                }
            }
            Event::Log(text) => crate::applog::event("mirror", &text),
            Event::Problem(text) => {
                crate::applog::problem("mirror", &text);
                notices.trouble(text);
            }
            Event::Unknown(line) => {
                if !line.is_empty() {
                    crate::applog::event("mirror", &format!("the helper said: {line}"));
                }
            }
        }
        running.home_window.request_redraw();
    }

    /// Does what Settings › AirPlay asked, takes in what the helper said, starts it again
    /// if it stopped, tells the renderer whether a box wants its pictures, and keeps a still
    /// screen's picture from fading. Says when to look again.
    pub(crate) fn tick_mirror(&mut self) -> Option<Instant> {
        // Left on when the app last closed: on again, once the window is there.
        if !self.mirror.started && self.running.is_some() {
            self.mirror.started = true;
            if self.mirror.saved.on {
                let wake = self.mirror_waker();
                let _ = self.mirror.start(&wake);
            }
        }
        if let Some(asked) = self.running.as_mut().and_then(|r| r.home_state.mirror.request.take()) {
            match asked {
                Request::Switch(on) => {
                    let _ = self.mirror_switch(on);
                }
                Request::Name(name) => self.mirror_name(&name),
                Request::Ask(ask) => self.mirror_ask(ask),
                Request::Stop => {
                    self.mirror.say(&serde_json::json!({ "stop": true }));
                }
            }
        }
        let now = Instant::now();
        let (events, gone) = self.mirror.take_events();
        for event in events {
            self.mirror_event(event);
        }
        if gone {
            self.mirror_gone(now);
        }
        if self.mirror.saved.on && self.mirror.helper.is_none() && self.mirror.retry_at.is_some_and(|at| at <= now) {
            self.mirror.retry_at = None;
            let wake = self.mirror_waker();
            if self.mirror.start(&wake).is_err() {
                self.mirror.failures += 1;
                self.mirror.retry_at = Some(now + retry_after(self.mirror.failures));
            }
        }
        // Whether a box wants the pictures, and whether anybody is sending any.
        let (live, editing) = (self.live_mirror_wanted(), self.editing_mirror());
        let wanted = live || editing;
        let connected = self.mirror.connected.is_some();
        let telling = (wanted, !connected);
        if self.mirror.told != Some(telling) {
            let began = wanted && !self.mirror.told.is_some_and(|(was, _)| was);
            self.mirror.told = Some(telling);
            self.mirror.shared.wanted.store(wanted, Ordering::Release);
            sp_render::set_mirror_receiving(wanted, !connected);
            // The picture that came while no box wanted one: a still screen sends no other.
            if began
                && connected
                && let Some(picture) = self.mirror.shared.latest.lock().ok().and_then(|latest| latest.clone())
            {
                sp_render::show_mirror(picture);
            }
        }
        if wanted && connected {
            sp_render::keep_mirror();
        }
        if let Some(running) = self.running.as_mut()
            && self.mirror.view(&mut running.home_state.mirror)
        {
            running.home_window.request_redraw();
        }
        let keep_at = (wanted && connected).then(|| now + if editing { Duration::from_millis(33) } else { KEEP_EVERY });
        [keep_at, self.mirror.retry_at].into_iter().flatten().min()
    }

    /// The helper stopped by itself while switched on: said once, and started again after a wait
    /// that grows while it keeps failing.
    fn mirror_gone(&mut self, now: Instant) {
        let ran = self.mirror.helper.as_ref().map(|helper| now.duration_since(helper.since));
        self.mirror_settle_all();
        let was_showing = self.mirror.connected.take();
        self.mirror.stop();
        if !self.mirror.saved.on {
            return;
        }
        self.mirror.failures = if ran.is_some_and(|ran| ran >= RAN_WELL) { 1 } else { self.mirror.failures + 1 };
        let wait = retry_after(self.mirror.failures);
        self.mirror.retry_at = Some(now + wait);
        crate::applog::problem(
            "mirror",
            &format!("the helper stopped by itself; starting it again in {} s", wait.as_secs()),
        );
        if let Some(running) = self.running.as_mut() {
            if self.mirror.failures == 1 || was_showing.is_some() {
                running.home_state.notices.trouble(t(
                    "mirror.problem.stopped",
                    "AirPlay stopped by itself and is starting again. A device that was showing its screen has to connect again.",
                ));
            }
            running.home_window.request_redraw();
        }
    }

    fn mirror_name(&mut self, name: &str) {
        let name = clean_name(name);
        self.mirror.saved.name = if name == default_name() { String::new() } else { name };
        self.mirror.save();
        if let Some(running) = self.running.as_mut() {
            running.home_state.mirror.name = self.mirror.name();
        }
        crate::applog::event("mirror", "The name devices see was changed.");
        self.mirror_restart();
    }

    fn mirror_ask(&mut self, ask: bool) {
        self.mirror.saved.ask = ask;
        self.mirror.save();
        crate::applog::event("mirror", if ask { "Asks each time." } else { "Lets devices in without asking." });
        self.mirror_restart();
    }

    /// Whether a device's first picture has just come (the thumbnails that show the box are
    /// drawn again), and whether a new picture has come that the Edit view's slide shows (at
    /// most thirty times a second).
    pub(crate) fn mirror_pictures(&mut self) -> (bool, bool) {
        let first = self.mirror.shared.first_came.swap(false, Ordering::AcqRel);
        let pictures = self.mirror.shared.pictures.load(Ordering::Acquire);
        let now = Instant::now();
        let (seen, drawn) = self.mirror.drawn;
        let fresh = pictures != seen
            && self.editing_mirror()
            && drawn.is_none_or(|at| now.duration_since(at) >= Duration::from_millis(33));
        if fresh {
            self.mirror.drawn = (pictures, Some(now));
        }
        (first, fresh)
    }

    /// `sp mirror`: the state, the switch, the name, the answer to a device that asks, and Stop.
    pub(crate) fn ctl_mirror(&mut self, args: &[String]) -> Result<Reply, String> {
        const USAGE: &str = "mirror [status] | on | off | name NAME | ask on|off | allow [ID] | deny [ID] | stop";
        let word = args.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
        let said = match word.as_str() {
            "" | "status" if args.len() <= 2 => None,
            "on" | "off" if args.len() == 2 => {
                self.mirror_switch(word == "on")?;
                None
            }
            "name" => {
                let name = clean_name(&args.get(2..).unwrap_or_default().join(" "));
                if name.is_empty() {
                    return Err("mirror name NAME".to_owned());
                }
                self.mirror_name(&name);
                Some(format!("Devices now see \"{}\".", self.mirror.name()))
            }
            "ask" => {
                let ask = match args.get(2).map(|w| w.to_lowercase()).as_deref() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err("mirror ask on|off".to_owned()),
                };
                self.mirror_ask(ask);
                Some(if ask { "Asks each time." } else { "Lets devices in without asking." }.to_owned())
            }
            "allow" | "deny" => {
                let id = match args.get(2) {
                    Some(id) => id.parse::<u64>().map_err(|_| USAGE.to_owned())?,
                    None => match self.mirror.asking.as_slice() {
                        [(id, _)] => *id,
                        [] => return Err("No device is waiting for an answer.".to_owned()),
                        _ => {
                            return Err(
                                "Several devices are waiting: name one by its id (sp mirror lists them).".to_owned()
                            );
                        }
                    },
                };
                let said = crate::i18n::with_language(crate::i18n::Language::English, || {
                    self.mirror_answer(id, word == "allow")
                })?;
                return Ok(Reply::with(said, self.mirror.status().data));
            }
            "stop" if args.len() == 2 => {
                let Some(name) = self.mirror.connected().map(str::to_owned) else {
                    return Err("Nobody is showing a screen.".to_owned());
                };
                self.mirror.say(&serde_json::json!({ "stop": true }));
                Some(format!("Stopping {name}."))
            }
            _ => return Err(USAGE.to_owned()),
        };
        let status = self.mirror.status();
        Ok(match said {
            Some(said) => Reply::with(said, status.data),
            None => status,
        })
    }
}

/// How long to wait before starting the helper again after it has failed this many times in a
/// row: two seconds, then twice as long each time, up to a minute.
fn retry_after(failures: u32) -> Duration {
    Duration::from_secs((1_u64 << failures.clamp(1, 6)).min(60))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Mac bundle lists the Bonjour services the app uses, and macOS blocks any it leaves
    /// out: without NDI's, no NDI screen could be found (it happened on the church's Mac).
    #[test]
    fn the_mac_bundle_lets_every_bonjour_service_through() {
        let script = include_str!("../../../scripts/macos-app.sh");
        for service in ["_ndi._tcp", "_airplay._tcp", "_raop._tcp"] {
            assert!(script.contains(&format!("<string>{service}</string>")), "{service} is not in NSBonjourServices");
        }
    }

    #[test]
    fn the_helpers_lines_are_read_and_anything_else_is_only_logged() {
        assert_eq!(
            parse_event(r#"{"event":"listening","port":7100,"video_port":7101,"announced":true}"#),
            Event::Listening { port: 7100, video_port: 7101, announced: true }
        );
        assert_eq!(
            parse_event(r#"{"event":"asks","id":3,"name":"Anna's iPhone","model":"iPhone15,2"}"#),
            Event::Asks { id: 3, name: "Anna's iPhone".to_owned() }
        );
        assert_eq!(
            parse_event(r#"{"event":"connected","id":3,"name":"Anna's iPhone"}"#),
            Event::Connected { id: 3, name: "Anna's iPhone".to_owned() }
        );
        assert_eq!(
            parse_event(r#"{"event":"picture_size","id":3,"width":1920,"height":1080,"codec":"h264"}"#),
            Event::PictureSize { width: 1920, height: 1080, codec: "h264".to_owned() }
        );
        assert_eq!(
            parse_event(r#"{"event":"stopped","id":3,"name":"Anna's iPhone","why":"it said goodbye"}"#),
            Event::Stopped { id: 3, name: "Anna's iPhone".to_owned(), why: "it said goodbye".to_owned() }
        );
        assert_eq!(
            parse_event(r#"{"event":"refused","name":"Tom's iPad","why":"busy"}"#),
            Event::Refused { name: "Tom's iPad".to_owned(), why: "busy".to_owned() }
        );
        assert_eq!(parse_event(r#"{"event":"log","text":"fp-setup 1"}"#), Event::Log("fp-setup 1".to_owned()));
        assert_eq!(
            parse_event(r#"{"event":"problem","text":"FFmpeg stopped."}"#),
            Event::Problem("FFmpeg stopped.".to_owned())
        );
        // Not known, not JSON, not an object, lacking what it needs, numbers out of range.
        for line in [
            r#"{"event":"dancing","id":1}"#,
            "thread 'main' panicked",
            "[1,2,3]",
            "",
            r#"{"event":"asks","name":"no id"}"#,
            r#"{"event":"asks","id":-4,"name":"x"}"#,
            r#"{"event":"listening","port":70000,"video_port":1}"#,
            r#"{"event":"picture_size","width":99999999999,"height":2}"#,
            r#"{"event":7}"#,
        ] {
            assert!(matches!(parse_event(line), Event::Unknown(_)), "{line}");
        }
    }

    #[test]
    fn a_devices_name_is_only_text_cut_short_and_without_control_characters() {
        let long = format!(r#"{{"event":"asks","id":1,"name":"{}"}}"#, "N".repeat(5000));
        let Event::Asks { name, .. } = parse_event(&long) else { panic!() };
        assert_eq!(name.chars().count(), NAME_MOST);
        let odd = r#"{"event":"asks","id":1,"name":"  Eve\n\u0007\u001b[31m's iPhone\t "}"#;
        let Event::Asks { name, .. } = parse_event(odd) else { panic!() };
        assert!(!name.chars().any(char::is_control), "{name:?}");
        assert!(name.starts_with("Eve") && name.ends_with("iPhone"));
        // No name, or one that isn't text: a device all the same.
        for line in [
            r#"{"event":"asks","id":1}"#,
            r#"{"event":"asks","id":1,"name":{"a":1}}"#,
            r#"{"event":"asks","id":1,"name":"\n"}"#,
        ] {
            assert_eq!(parse_event(line), Event::Asks { id: 1, name: "A device".to_owned() });
        }
        assert_eq!(clean_name("  Front\u{0} Mac \r\n"), "Front Mac");
        // A long log line is cut too.
        let Event::Log(text) = parse_event(&format!(r#"{{"event":"log","text":"{}"}}"#, "x".repeat(9000))) else {
            panic!()
        };
        assert_eq!(text.len(), 400);
    }

    fn header(mark: &[u8; 4], width: u32, height: u32, session: u32) -> [u8; 16] {
        let mut bytes = [0_u8; 16];
        bytes[..4].copy_from_slice(mark);
        bytes[4..8].copy_from_slice(&width.to_le_bytes());
        bytes[8..12].copy_from_slice(&height.to_le_bytes());
        bytes[12..].copy_from_slice(&session.to_le_bytes());
        bytes
    }

    #[test]
    fn a_pictures_header_is_checked_before_room_is_made_for_it() {
        assert_eq!(picture_header(&header(b"SPMP", 1920, 1080, 7)), Ok((1920, 1080, 7)));
        assert_eq!(picture_header(&header(b"SPMP", 16, 16, 0)), Ok((16, 16, 0)));
        assert_eq!(picture_header(&header(b"SPMP", 4096, 2304, 0)), Ok((4096, 2304, 0)));
        assert!(picture_header(&header(b"SPMQ", 1920, 1080, 7)).is_err());
        assert!(picture_header(&header(b"\0\0\0\0", 1920, 1080, 7)).is_err());
        for (width, height) in [(0, 0), (0, 1080), (1920, 0), (15, 1080), (4097, 16), (16, 4097), (u32::MAX, u32::MAX)]
        {
            assert!(picture_header(&header(b"SPMP", width, height, 1)).is_err(), "{width} x {height}");
        }
        // Each side allowed, but more bytes than a picture may have.
        assert!(picture_header(&header(b"SPMP", 4096, 4096, 1)).is_err());
        assert!(picture_header(&[0xff; 16]).is_err());
    }

    #[test]
    fn pictures_are_read_one_after_another_and_a_bad_header_marks_the_helper_broken() {
        let shared = Shared::default();
        shared.first_due.store(true, Ordering::Release);
        let woken = Arc::new(AtomicU64::new(0));
        let count = woken.clone();
        let wake: Waker = Arc::new(move || {
            count.fetch_add(1, Ordering::AcqRel);
        });
        let mut bytes = Vec::new();
        for shade in [10_u8, 200] {
            bytes.extend_from_slice(&header(b"SPMP", 16, 16, 1));
            bytes.extend_from_slice(&vec![shade; 16 * 16 * 4]);
        }
        // Cut off in the middle of a third: the end, and nothing breaks.
        bytes.extend_from_slice(&header(b"SPMP", 16, 16, 1));
        bytes.extend_from_slice(&[0; 100]);
        read_pictures(&bytes[..], &shared, &wake);
        assert_eq!(shared.pictures.load(Ordering::Acquire), 2);
        assert!(shared.first_came.load(Ordering::Acquire) && !shared.first_due.load(Ordering::Acquire));
        assert_eq!(woken.load(Ordering::Acquire), 1, "woken for the first picture only");
        let latest = shared.latest.lock().unwrap().clone().unwrap();
        assert_eq!((latest.width(), latest.height()), (16, 16));
        assert!(!shared.broken.load(Ordering::Acquire));
        // What is no picture: nothing is set aside for the size it claims, and the app is told.
        let mut bad = header(b"SPMP", 60000, 60000, 1).to_vec();
        bad.extend_from_slice(&[0; 64]);
        read_pictures(&bad[..], &shared, &wake);
        assert!(shared.broken.load(Ordering::Acquire));
        assert_eq!(shared.pictures.load(Ordering::Acquire), 2);
    }

    #[test]
    fn a_line_too_long_is_skipped_and_the_next_is_read() {
        let mut text = vec![b'x'; LINE_MOST as usize * 2 + 17];
        text.extend_from_slice(b"\n{\"event\":\"log\",\"text\":\"after\"}\nlast");
        let mut reader = BufReader::new(&text[..]);
        assert_eq!(read_line(&mut reader).as_deref(), Some(""));
        assert_eq!(read_line(&mut reader).as_deref(), Some(r#"{"event":"log","text":"after"}"#));
        assert_eq!(read_line(&mut reader).as_deref(), Some("last"));
        assert_eq!(read_line(&mut reader), None);
    }

    #[test]
    fn it_is_off_and_asks_until_someone_says_otherwise() {
        let saved = Saved::default();
        assert!(!saved.on && saved.ask && saved.name.is_empty() && saved.device_id.is_empty());
        let read: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(read, saved);
        let read: Saved =
            serde_json::from_str(r#"{"on":true,"ask":false,"name":"Hall","device_id":"02:AA:BB:CC:DD:EE"}"#).unwrap();
        assert!(read.on && !read.ask);
        assert_eq!(serde_json::from_str::<Saved>(&serde_json::to_string(&read).unwrap()).unwrap(), read);
    }

    #[test]
    fn the_device_id_is_made_up_and_marked_as_nobodys() {
        let (one, two) = (new_device_id(), new_device_id());
        assert!(is_device_id(&one) && is_device_id(&two));
        assert_ne!(one, two);
        let first = u8::from_str_radix(&one[..2], 16).unwrap();
        assert_eq!(first & 0x03, 0x02, "locally administered, for one device");
        for bad in [
            "",
            "02:AA:BB:CC:DD",
            "02:AA:BB:CC:DD:EE:FF",
            "02-AA-BB-CC-DD-EE",
            "0G:AA:BB:CC:DD:EE",
            "2:AA:BB:CC:DD:EEE",
        ] {
            assert!(!is_device_id(bad), "{bad}");
        }
    }

    #[test]
    fn the_helper_is_never_told_to_announce_in_a_quiet_run_and_gets_no_name_on_its_command_line() {
        let ffmpeg = Path::new("/somewhere/ffmpeg");
        let words = |quiet, port| -> Vec<String> {
            helper_arguments(ffmpeg, quiet, port).iter().map(|word| word.to_string_lossy().into_owned()).collect()
        };
        assert_eq!(words(false, None), ["--ffmpeg", "/somewhere/ffmpeg", "--announce"]);
        assert_eq!(words(false, Some(7300)), ["--ffmpeg", "/somewhere/ffmpeg", "--announce"]);
        assert_eq!(words(true, None), ["--ffmpeg", "/somewhere/ffmpeg", "--port", "0"]);
        assert_eq!(words(true, Some(7300)), ["--ffmpeg", "/somewhere/ffmpeg", "--port", "7300"]);
        // The name, whatever is in it, travels as JSON text on the standard input.
        let line = hello_line("Hall \"A\"\n--announce", "02:AA:BB:CC:DD:EE", true);
        assert_eq!(line.matches('\n').count(), 1, "one line");
        let read: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(read["hello"]["name"], "Hall \"A\"\n--announce");
        assert_eq!(read["hello"]["device_id"], "02:AA:BB:CC:DD:EE");
        assert_eq!(read["hello"]["ask"], true);
    }

    #[test]
    fn a_helper_that_keeps_failing_is_tried_less_and_less_often() {
        let waits: Vec<u64> = (0..9).map(|failures| retry_after(failures).as_secs()).collect();
        assert_eq!(waits, [2, 2, 4, 8, 16, 32, 60, 60, 60]);
    }

    #[test]
    fn a_box_that_is_hidden_wants_no_pictures() {
        let shown = sp_model::Object::mirror();
        let mut hidden = sp_model::Object::mirror();
        hidden.hidden = true;
        assert!(wanted_in(&[sp_model::Object::ndi(String::new()), shown]));
        assert!(!wanted_in(&[hidden, sp_model::Object::ndi("A (B)".to_owned())]));
        assert!(!wanted_in(&[]));
    }

    #[test]
    fn the_inspectors_line_says_what_is_so() {
        let mut view = View { name_now: "SoftPresenter \u{2014} Hall".to_owned(), ..View::default() };
        assert!(line(&view).contains("switched off"));
        view.on = true;
        assert!(line(&view).contains("choose SoftPresenter \u{2014} Hall"));
        view.connected = Some("Anna's iPhone".to_owned());
        assert!(line(&view).starts_with("Anna's iPhone is showing its screen."));
    }

    /// The owner, 26 November: "we should just call it airplay". Its name in every language is
    /// AirPlay, never translated; the line saying it is unofficial keeps the owner's words.
    #[test]
    fn the_feature_is_called_airplay_in_every_language() {
        use crate::i18n::Language;
        let wrapped = crate::i18n::wrapped_keys();
        let english = |key: &str| wrapped.get(key).cloned().unwrap_or_else(|| panic!("no code asks for `{key}`"));
        let named: &[(&'static str, &str)] = &[
            ("settings.pane.mirror", "AirPlay"),
            ("mirror.caption", "AIRPLAY"),
            ("editor.add.key.mirror", "AirPlay"),
            ("editor.add.mirror", "AirPlay"),
            ("editor.mirror.name", "AirPlay"),
            ("editor.mirror.numbered", "AirPlay {n}"),
            ("inspector.tab.mirror", "AirPlay"),
            ("inspector.kind.mirror", "AirPlay"),
            ("mirror.box.off", "AirPlay is switched off. Switch it on in Settings \u{203a} AirPlay."),
            ("mirror.problem.start", "AirPlay could not start: {why}."),
            ("mirror.problem.no_ffmpeg", "AirPlay needs FFmpeg, and FFmpeg isn\u{2019}t on this computer."),
            (
                "mirror.problem.no_program",
                "The part of SoftPresenter that receives AirPlay (softpresenter-mirror) isn\u{2019}t beside the app.",
            ),
            (
                "mirror.problem.stopped",
                "AirPlay stopped by itself and is starting again. A device that was showing its screen has to connect again.",
            ),
        ];
        for (key, words) in named {
            assert_eq!(english(key), *words, "`{key}`");
        }
        assert!(english("mirror.how").contains("shows in an AirPlay box on the slide"));
        assert_eq!(
            english("mirror.unofficial"),
            "Unofficial: works with Screen Mirroring (AirPlay) from an iPhone, iPad or Mac, but isn\u{2019}t made or supported by Apple, and an Apple update may stop it working."
        );
        // Every language still has its words for each, and they say AirPlay too.
        let keys = named.iter().map(|(key, _)| *key).chain(["mirror.how", "mirror.unofficial"]);
        for key in keys {
            for language in [Language::Spanish, Language::Russian, Language::Ukrainian, Language::German] {
                // `tg`, not `t`: it remembers nothing, so the stand-in English never reaches
                // the table that puts replies back into English for the other tests.
                let said = crate::i18n::with_language(language, || crate::i18n::tg(key, "\u{0}"));
                assert_ne!(said, "\u{0}", "{language:?} has no words for `{key}`");
                assert!(said.to_lowercase().contains("airplay"), "{language:?} `{key}`: {said}");
            }
        }
        // Nothing the program wraps calls it by its old names any more.
        for (key, words) in &wrapped {
            for old in
                ["Phone Screens", "PHONE SCREENS", "Phone or iPad Screen", "phone screens", "a phone\u{2019}s screen"]
            {
                assert!(!words.contains(old), "`{key}` still says {old:?}: {words}");
            }
        }
    }

    /// `sp mirror` calls it AirPlay too.
    #[test]
    fn sp_mirror_says_airplay() {
        let mirror = Mirror {
            saved: Saved::default(),
            quiet: true,
            shared: Arc::default(),
            helper: None,
            failures: 0,
            retry_at: None,
            asking: Vec::new(),
            connected: None,
            listening: None,
            size: None,
            problem: None,
            told: None,
            drawn: (0, None),
            started: false,
            found: false,
        };
        assert_eq!(mirror.status().text, "AirPlay is switched off.");
    }
}
