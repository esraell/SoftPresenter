//! Remote Control: a web page any computer or tablet on the church's network can open to run the
//! show, served by the app itself over HTTPS while switched on in Settings › Remote Control.
//!
//! - `server`: the HTTPS and WebSocket server, on a thread and tokio runtime of its own.
//! - `auth`: the password (argon2id), the devices remembered, the guard against guessing.
//! - `cert`: the certificate made on this computer, and its fingerprint.
//! - `policy`: which of `sp`'s commands a page may send.
//! - `hub`: the state pages are sent, in versioned parts.
//! - `state`: what the app publishes into the hub, and the pictures it draws for pages.
//! - `pane`: Settings › Remote Control.
//!
//! The page sends the same commands `sp` does; each reaches the main thread as `sp`'s do
//! (`AppEvent::Remote`) and is carried out by `App::control`. docs/remote-api.md describes the
//! API, and the security model.

pub mod auth;
pub mod cert;
pub mod hub;
pub mod pane;
pub mod policy;
pub mod server;
pub mod state;

use std::collections::VecDeque;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use sp_model::Uuid;
use tokio::sync::oneshot;

use crate::control::Reply;

/// What the server asks of the app's main thread.
pub enum ToApp {
    Command {
        caller: server::Caller,
        args: Vec<String>,
        reply: oneshot::Sender<Reply>,
    },
    Thumb {
        slide: Uuid,
        version: Option<Uuid>,
        /// The look the address asked for: drawn only when it is the current one.
        look: String,
        reply: oneshot::Sender<server::Thumbed>,
    },
    Refresh {
        reply: oneshot::Sender<()>,
    },
    /// A page connected or went.
    Pages,
}

/// The server's way to the app: each request wakes the event loop.
pub struct Bridge {
    send: Box<dyn Fn(ToApp) -> bool + Send + Sync>,
}

impl Bridge {
    pub fn new(send: impl Fn(ToApp) -> bool + Send + Sync + 'static) -> Bridge {
        Bridge { send: Box::new(send) }
    }
}

impl server::Backend for Bridge {
    fn command(&self, caller: server::Caller, args: Vec<String>) -> oneshot::Receiver<Reply> {
        let (reply, answer) = oneshot::channel();
        (self.send)(ToApp::Command { caller, args, reply });
        answer
    }

    fn thumb(&self, slide: Uuid, version: Option<Uuid>, look: String) -> oneshot::Receiver<server::Thumbed> {
        let (reply, answer) = oneshot::channel();
        (self.send)(ToApp::Thumb { slide, version, look, reply });
        answer
    }

    fn refresh(&self) -> oneshot::Receiver<()> {
        let (reply, answer) = oneshot::channel();
        (self.send)(ToApp::Refresh { reply });
        answer
    }

    fn pages_changed(&self) {
        (self.send)(ToApp::Pages);
    }

    fn log(&self, line: &str) {
        crate::applog::event("remote", line);
    }
}

/// Where Remote Control keeps its settings, devices and certificate: `remote` in the data
/// folder, a folder only this user may enter.
pub fn folder() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("remote"))
}

/// The address to bind: every network the computer is on; this computer only in a quiet test
/// copy, so a test never asks the system's firewall to let the network in.
fn bind_address() -> IpAddr {
    // Testing aid: SOFTPRESENTER_QUIET=1 listens on this computer only (127.0.0.1).
    if std::env::var_os("SOFTPRESENTER_QUIET").is_some_and(|v| !v.is_empty()) {
        IpAddr::from([127, 0, 0, 1])
    } else {
        IpAddr::from([0, 0, 0, 0])
    }
}

/// The computer's address on its network, as other devices reach it: the one the system would
/// send from (no packet is sent to find it).
pub fn lan_address() -> Option<IpAddr> {
    if bind_address().is_loopback() {
        return Some(bind_address());
    }
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("192.0.2.1", 9)).ok()?;
    socket.local_addr().ok().map(|addr| addr.ip()).filter(|ip| !ip.is_unspecified())
}

/// Turns `Instant`s into milliseconds since 1970 by one pair of readings taken once, so the same
/// instant always gives the same time (and the state doesn't change every time it is read).
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    at: Instant,
    unix_ms: u64,
}

impl Clock {
    pub fn new() -> Clock {
        Clock { at: Instant::now(), unix_ms: server::now_ms() }
    }

    pub fn ms(&self, instant: Instant) -> u64 {
        match instant.checked_duration_since(self.at) {
            Some(after) => self.unix_ms + after.as_millis() as u64,
            None => self.unix_ms.saturating_sub(self.at.duration_since(instant).as_millis() as u64),
        }
    }
}

impl Default for Clock {
    fn default() -> Clock {
        Clock::new()
    }
}

/// Remote Control as the app holds it: switched on or off, its settings and devices, the
/// server while it runs, and what is waiting for the main thread.
pub struct Remote {
    store: Option<Arc<Mutex<auth::Store>>>,
    running: Option<server::Running>,
    /// The thread of a server switched off, while it lets its port go.
    stopping: Option<std::thread::JoinHandle<()>>,
    fingerprint: Option<String>,
    /// The computer's own name on its network (`church-mac.local`), as the certificate names it.
    local_name: Option<String>,
    /// What went wrong at the last switch, to show in Settings.
    pub problem: Option<String>,
    /// What a password being set came to, from its thread.
    password_said: Arc<Mutex<Option<Result<String, String>>>>,
    hashing: bool,
    /// Slides waiting to be drawn for pages, a few at a time.
    pub(crate) thumbs_waiting: VecDeque<(Uuid, Option<Uuid>, String, oneshot::Sender<server::Thumbed>)>,
    /// When the state was last published, and its seldom-changing parts.
    pub(crate) published: Option<Instant>,
    pub(crate) published_slow: Option<Instant>,
    pub clock: Clock,
    pub(crate) looks: state::Looks,
    /// A new certificate the operator should hear of, once.
    pub cert_news: Option<String>,
    /// When it was last looked whether the certificate must be made again.
    cert_checked: Option<Instant>,
    /// When devices not seen for half a year were last looked for.
    devices_checked: Option<Instant>,
    /// Wrong passwords have been said in the notices, and not yet stopped for the hour.
    wrong_said: bool,
}

/// How many wrong passwords in the hour are said in the notices.
const WRONG_TO_SAY: usize = 3;

/// Slides drawn for pages each time the event loop goes round, at most.
pub const DRAW_AT_ONCE: usize = 3;
/// The most slides waiting to be drawn; past that a page is told the app is busy.
pub const MOST_WAITING_THUMBS: usize = 64;

impl Default for Remote {
    fn default() -> Remote {
        Remote::open()
    }
}

impl Remote {
    /// Reads Remote Control's settings; nothing listens yet.
    pub fn open() -> Remote {
        let store = folder().map(|folder| Arc::new(Mutex::new(auth::Store::open(&folder))));
        Remote {
            store,
            running: None,
            stopping: None,
            fingerprint: None,
            local_name: None,
            problem: None,
            password_said: Arc::default(),
            hashing: false,
            thumbs_waiting: VecDeque::new(),
            published: None,
            published_slow: None,
            clock: Clock::new(),
            looks: state::Looks::default(),
            cert_news: None,
            cert_checked: None,
            devices_checked: None,
            wrong_said: false,
        }
    }

    fn saved(&self) -> Option<auth::Saved> {
        self.store.as_ref().and_then(|store| store.lock().ok().map(|store| store.saved.clone()))
    }

    /// Whether it was left switched on (to switch on again when the app opens).
    pub fn wanted_on(&self) -> bool {
        self.saved().is_some_and(|saved| saved.on && saved.password.is_some())
    }

    pub fn is_on(&self) -> bool {
        self.running.is_some()
    }

    pub fn shared(&self) -> Option<&Arc<server::Shared>> {
        self.running.as_ref().map(|running| &running.shared)
    }

    /// Pages connected now.
    pub fn page_count(&self) -> usize {
        self.shared().map_or(0, |shared| shared.page_count())
    }

    pub fn port(&self) -> u16 {
        self.saved().map_or(auth::DEFAULT_PORT, |saved| saved.port)
    }

    /// The address to open on another device, while on: by the computer's own name
    /// (`https://church-mac.local:4316`) when it has one, which the certificate names and which
    /// stays the same when the network gives the computer another address; otherwise by address.
    pub fn address(&self) -> Option<String> {
        let running = self.running.as_ref()?;
        match &self.local_name {
            Some(name) => Some(format!("https://{name}:{}", running.local.port())),
            None => self.address_by_ip(),
        }
    }

    /// The address as figures, while on: it works too, but a browser warns about it, as the
    /// certificate doesn't name it.
    pub fn address_by_ip(&self) -> Option<String> {
        let running = self.running.as_ref()?;
        let ip = lan_address().unwrap_or(running.local.ip());
        Some(match ip {
            IpAddr::V4(v4) => format!("https://{v4}:{}", running.local.port()),
            IpAddr::V6(v6) => format!("https://[{v6}]:{}", running.local.port()),
        })
    }

    pub fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }

    /// Switches Remote Control on (listening at once) or off (every connection closed).
    pub fn switch(&mut self, on: bool, bridge: impl FnOnce() -> Bridge) -> Result<(), String> {
        let store = self.store.clone().ok_or("There is no data folder to keep Remote Control's settings in.")?;
        if !on {
            if let Some(running) = self.running.take() {
                self.stopping = running.stop();
                crate::applog::event("remote", "Switched off.");
            }
            self.set_on(false);
            return Ok(());
        }
        if self.running.is_some() {
            return Ok(());
        }
        let (port, has_password, folder) = {
            let store = store.lock().map_err(|_| "Busy.")?;
            (store.saved.port, store.saved.password.is_some(), store.folder().to_owned())
        };
        if !has_password {
            return Err("Set a password first.".to_owned());
        }
        // A server switched off a moment ago is let finish letting its port go.
        if let Some(stopping) = self.stopping.take() {
            let _ = stopping.join();
        }
        // Devices not seen for half a year are forgotten.
        self.forget_unused_devices();
        let names = cert::Names::now(cert::own_name());
        let had = self.fingerprint.clone().or_else(|| cert::kept_fingerprint(&folder));
        let (cert, made) = cert::load_or_make(&folder, &names)?;
        let fingerprint = cert.fingerprint.clone();
        if made {
            crate::applog::event("remote", &format!("Made a certificate, for {}.", names.names.join(", ")));
            // A certificate made again keeps its key, so what the operator checked still holds.
            // Only a different key is news: each device must be checked again.
            if had.is_some_and(|had| had != fingerprint) {
                self.cert_news = Some(
                    "Remote Control has a new certificate key. Each device warns once more: check the key fingerprint it shows against Settings › Remote Control."
                        .to_owned(),
                );
            }
        }
        let shared = server::Shared::new(store, Box::new(bridge()))?;
        self.local_name = names.local_name();
        shared.set_local_name(self.local_name.clone());
        let running = server::start(server::Config { bind: bind_address(), port, cert }, shared)?;
        crate::applog::event("remote", &format!("Switched on, on port {}.", running.local.port()));
        self.cert_checked = Some(Instant::now());
        self.running = Some(running);
        self.fingerprint = Some(fingerprint);
        self.published = None;
        self.published_slow = None;
        self.set_on(true);
        Ok(())
    }

    fn set_on(&mut self, on: bool) {
        if let Some(mut store) = self.store.as_ref().and_then(|store| store.lock().ok())
            && store.saved.on != on
        {
            store.saved.on = on;
            let _ = store.save();
        }
    }

    /// A new port: kept, and listened on at once if Remote Control is on.
    pub fn set_port(&mut self, port: u16, bridge: impl FnOnce() -> Bridge) -> Result<(), String> {
        if port < 1024 {
            return Err("Choose a port from 1024 up: those below are the system's.".to_owned());
        }
        let store = self.store.clone().ok_or("There is no data folder.")?;
        {
            let mut store = store.lock().map_err(|_| "Busy.")?;
            if store.saved.port == port {
                return Ok(());
            }
            store.saved.port = port;
            store.save()?;
        }
        if self.running.is_some() {
            self.switch(false, unreachable_bridge)?;
            self.switch(true, bridge)?;
        }
        Ok(())
    }

    /// Sets a new password, hashed on a thread of its own; every device is forgotten (and its
    /// pages closed) once it is kept. `password_news` says how it went.
    pub fn set_password(&mut self, password: String) {
        let Some(store) = self.store.clone() else { return };
        let said = self.password_said.clone();
        let shared = self.running.as_ref().map(|running| running.shared.clone());
        self.hashing = true;
        let _ = std::thread::Builder::new().name("remote-password".to_owned()).spawn(move || {
            let outcome = auth::hash_password(&password).and_then(|hash| {
                store.lock().map_err(|_| "Busy.".to_owned())?.set_password(hash)?;
                if let Some(shared) = &shared {
                    shared.sign_out_all();
                }
                crate::applog::event("remote", "A new password was set; every device was forgotten.");
                Ok("The password is set. Devices sign in again with it.".to_owned())
            });
            if let Ok(mut said) = said.lock() {
                *said = Some(outcome);
            }
        });
    }

    /// How a password being set went, once it has.
    pub fn password_news(&mut self) -> Option<Result<String, String>> {
        let news = self.password_said.lock().ok()?.take();
        if news.is_some() {
            self.hashing = false;
        }
        news
    }

    pub fn hashing(&self) -> bool {
        self.hashing
    }

    pub fn has_password(&self) -> bool {
        self.saved().is_some_and(|saved| saved.password.is_some())
    }

    pub fn devices(&self) -> Vec<auth::Device> {
        self.saved().map(|saved| saved.devices).unwrap_or_default()
    }

    pub fn pages(&self) -> Vec<server::Page> {
        self.shared().map(|shared| shared.pages()).unwrap_or_default()
    }

    /// Forgets a device: it signs in again with the password, and its pages close now.
    pub fn forget(&mut self, id: &str) -> bool {
        let gone = self.store.as_ref().and_then(|store| store.lock().ok()).is_some_and(|mut store| store.forget(id));
        if let Some(shared) = self.shared() {
            shared.sign_out(id);
        }
        if gone {
            crate::applog::event("remote", &format!("Forgot the device {id}."));
        }
        gone
    }

    pub fn forget_all(&mut self) {
        if let Some(mut store) = self.store.as_ref().and_then(|store| store.lock().ok()) {
            store.forget_all();
        }
        if let Some(shared) = self.shared() {
            shared.sign_out_all();
        }
        crate::applog::event("remote", "Forgot every device.");
    }

    /// Forgets the devices not seen for half a year, and closes their pages.
    pub fn forget_unused_devices(&mut self) {
        let gone = self
            .store
            .as_ref()
            .and_then(|store| store.lock().ok())
            .map(|mut store| store.forget_unused(auth::now_seconds()));
        for id in gone.unwrap_or_default() {
            if let Some(shared) = self.shared() {
                shared.sign_out(&id);
            }
            crate::applog::event("remote", &format!("Forgot the device {id}: not seen for half a year."));
        }
    }

    /// Closes one page's connection; its device stays remembered.
    pub fn disconnect(&self, page: u64) {
        if let Some(shared) = self.shared() {
            shared.disconnect(page);
        }
    }

    /// Makes a new certificate with a new key; with Remote Control on, it listens again with it
    /// (pages reconnect, and browsers warn again: the fingerprint is new).
    pub fn new_certificate(&mut self, bridge: impl FnOnce() -> Bridge) -> Result<(), String> {
        let folder =
            self.store.as_ref().and_then(|s| s.lock().ok().map(|s| s.folder().to_owned())).ok_or("No data folder.")?;
        // On request the key is new too: the operator asked for what an old key can't give.
        let cert = cert::make(&folder, &cert::Names::now(cert::own_name()), true)?;
        self.fingerprint = Some(cert.fingerprint);
        crate::applog::event("remote", "Made a new certificate.");
        if self.running.is_some() {
            self.switch(false, unreachable_bridge)?;
            self.switch(true, bridge)?;
        }
        Ok(())
    }
}

/// The folders a path written in words starts in. Only these are recognised inside words, so a
/// song's own words ("C /E /G", "Medley /Part 1/ Part 2") are never cut.
const PATH_ROOTS: [&str; 14] = [
    "/Users/",
    "/home/",
    "/Volumes/",
    "/private/",
    "/var/",
    "/tmp/",
    "/Library/",
    "/Applications/",
    "/opt/",
    "/mnt/",
    "/media/",
    "/root/",
    "file://",
    "\\\\",
];

/// Whether a path written in words starts at `at`: a home, volume or system folder (`/Users/`,
/// `file://`, a network share), or a drive letter (`C:\`), at the start of the words or after a
/// space, a bracket or a quote.
fn path_starts(text: &str, at: usize) -> bool {
    let bytes = text.as_bytes();
    let before = at.checked_sub(1).map(|b| bytes[b]);
    let starts = before.is_none_or(|b| matches!(b, b' ' | b'\t' | b'\n' | b'(' | b'[' | b'"' | b'\'' | b'='));
    let rest = &text[at..];
    let windows =
        bytes[at].is_ascii_alphabetic() && bytes.get(at + 1) == Some(&b':') && bytes.get(at + 2) == Some(&b'\\');
    // The stage's own description of a picture names its file whatever folder it is in.
    let picture = text[..at].ends_with("a picture (") && (bytes[at] == b'/' || windows);
    starts && (windows || picture || PATH_ROOTS.iter().any(|root| rest.starts_with(root)))
}

/// The file name a whole path ends in.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).find(|part| !part.is_empty()).unwrap_or("")
}

/// Whether a whole string is an absolute path (`/…` or `C:\…` with a folder and a name in it).
fn is_whole_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    let absolute = bytes.first() == Some(&b'/')
        || text.starts_with("file://")
        || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\');
    absolute && !text.contains('\n') && text.matches(['/', '\\']).count() >= 2 && !text.starts_with("/thumb/")
}

/// Words with every absolute path that starts in a known folder cut to its file name, for what
/// a remote page is sent: the computer's folders (and the user's name in them) stay on the
/// computer. A path ends at a closing bracket, a quote or the end of the line. Anything else
/// that has slashes in it (a song's own words) is left alone.
pub fn without_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if path_starts(text, at) {
            let end = text[at..].find([')', ']', '"', '\'', '\n']).map_or(text.len(), |len| at + len);
            out.push_str(file_name(&text[at..end]));
            at = end;
            continue;
        }
        let next = text[at..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&text[at..at + next]);
        at += next;
    }
    out
}

/// The same for every string in a piece of JSON, except in what is a slide's own (`raw`): its
/// words are never touched, only the file a picture in it names.
pub fn json_without_paths(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(text) if text.contains(['/', '\\']) => *text = without_paths(text),
        serde_json::Value::Array(items) => items.iter_mut().for_each(json_without_paths),
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "raw" {
                    raw_without_paths(item);
                } else {
                    json_without_paths(item);
                }
            }
        }
        _ => {}
    }
}

/// A slide's raw document: only a picture's `file` is cut to its name, when it is a path.
fn raw_without_paths(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => items.iter_mut().for_each(raw_without_paths),
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                match item {
                    serde_json::Value::String(file) if key == "file" && is_whole_path(file) => {
                        *file = file_name(file).to_owned();
                    }
                    _ => raw_without_paths(item),
                }
            }
        }
        _ => {}
    }
}

/// A reply as a remote page is sent it: `sp`'s, with the computer's paths cut to file names.
pub fn reply_for_page(mut reply: Reply) -> Reply {
    if reply.text.contains(['/', '\\']) {
        reply.text = without_paths(&reply.text);
    }
    json_without_paths(&mut reply.data);
    reply
}

/// How often it is looked, while Remote Control is on, whether its certificate must be made
/// again (it is running out, or the computer's own name changed).
const CERT_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(600);

/// Switching off needs no bridge.
fn unreachable_bridge() -> Bridge {
    Bridge::new(|_| false)
}

impl crate::App {
    /// Refuses, while a remote page's command runs, what would open a file or folder on the
    /// computer: called where each such command opens it, behind the policy's own refusal.
    pub(crate) fn only_at_the_computer(&self, what: &str) -> Result<(), String> {
        if self.remote_call {
            return Err(format!("{what} is done at the computer, not from the remote page."));
        }
        Ok(())
    }

    /// `remote [status] | on | off | port N | password | forget ID|all | disconnect N`: Remote
    /// Control from the command line (never from a remote page: see `policy`). The password
    /// comes on the standard input (`sp` reads it, unshown at a terminal), never as an argument
    /// typed, and the log leaves it out.
    pub(crate) fn ctl_remote(&mut self, args: &[String]) -> Result<Reply, String> {
        let word = args.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
        let said = match word.as_str() {
            "" | "status" => None,
            "on" => {
                self.remote_switch(true)?;
                None
            }
            "off" => {
                self.remote_switch(false)?;
                None
            }
            "port" => {
                let port = args.get(2).and_then(|w| w.parse::<u16>().ok()).ok_or("remote port N   (1024 to 65535)")?;
                let bridge = self.remote_bridge();
                self.remote.set_port(port, move || bridge)?;
                self.remote_preview_source();
                Some(format!("Remote Control's port is {port}."))
            }
            "password" => {
                let password = args.get(2).ok_or("Give the password on the standard input: sp remote password")?;
                // Hashed here: a command typed once when setting up, not while running a service.
                let hash = auth::hash_password(password.trim_end_matches(['\r', '\n']))?;
                let store = self.remote.store.clone().ok_or("There is no data folder.")?;
                store.lock().map_err(|_| "Busy.")?.set_password(hash)?;
                if let Some(shared) = self.remote.shared() {
                    shared.sign_out_all();
                }
                crate::applog::event("remote", "A new password was set; every device was forgotten.");
                Some("The password is set; every device signs in again with it.".to_owned())
            }
            "forget" => match args.get(2).map(String::as_str) {
                Some("all") => {
                    self.remote.forget_all();
                    Some("Forgot every device.".to_owned())
                }
                Some(id) if self.remote.forget(id) => Some(format!("Forgot the device {id}.")),
                Some(id) => return Err(format!("No device {id} is remembered (sp remote lists them).")),
                None => return Err("remote forget ID|all".to_owned()),
            },
            "disconnect" => {
                let id = args
                    .get(2)
                    .and_then(|w| w.parse::<u64>().ok())
                    .ok_or("remote disconnect N   (N from sp remote)")?;
                if !self.remote.pages().iter().any(|page| page.id == id) {
                    return Err(format!("No page {id} is connected."));
                }
                self.remote.disconnect(id);
                Some(format!("Disconnected page {id}."))
            }
            _ => return Err("remote [status] | on | off | port N | password | forget ID|all | disconnect N".to_owned()),
        };
        let pages: Vec<serde_json::Value> = self
            .remote
            .pages()
            .iter()
            .map(|page| {
                let since = page.since.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                serde_json::json!({"id": page.id, "device": page.device, "name": page.name, "ip": page.ip.to_string(), "since": since})
            })
            .collect();
        let devices: Vec<serde_json::Value> = self
            .remote
            .devices()
            .iter()
            .map(|d| serde_json::json!({"id": d.id, "name": d.name, "first_seen": d.first_seen, "last_seen": d.last_seen, "ip": d.ip}))
            .collect();
        let address = self.remote.address();
        let address_ip = self.remote.address_by_ip().filter(|ip| address.as_ref() != Some(ip));
        let mut lines = vec![match (&address, said) {
            (_, Some(said)) => said,
            (Some(address), None) => format!("Remote Control is on: open {address}"),
            (None, None) => "Remote Control is off.".to_owned(),
        }];
        if let Some(ip) = &address_ip {
            lines.push(format!("By address: {ip} (the browser may warn again if the address changes)"));
        }
        if let Some(fingerprint) = self.remote.fingerprint().filter(|_| self.remote.is_on()) {
            lines.push(format!("Key fingerprint (SHA-256): {fingerprint}"));
        }
        if !self.remote.has_password() {
            lines.push("No password is set: echo \"$PASSWORD\" | sp remote password".to_owned());
        }
        lines.push(format!("Pages connected: {}   Devices remembered: {}", pages.len(), devices.len()));
        Ok(Reply::with(
            lines.join("\n"),
            serde_json::json!({
                "on": self.remote.is_on(),
                "port": self.remote.port(),
                "address": address,
                "address_by_ip": address_ip,
                "fingerprint": self.remote.fingerprint().filter(|_| self.remote.is_on()),
                "password_set": self.remote.has_password(),
                "pages": pages,
                "devices": devices,
            }),
        ))
    }

    /// The server's way to this app: each request wakes the event loop with it.
    pub(crate) fn remote_bridge(&self) -> Bridge {
        let proxy = self.waker.clone().map(Mutex::new);
        Bridge::new(move |asked| {
            proxy
                .as_ref()
                .and_then(|proxy| proxy.lock().ok())
                .is_some_and(|proxy| proxy.send_event(crate::AppEvent::Remote(asked)).is_ok())
        })
    }

    /// Switches Remote Control on or off, and says so in the log; with it on, the live preview
    /// is read for pages from the window's own textures.
    pub(crate) fn remote_switch(&mut self, on: bool) -> Result<(), String> {
        let bridge = self.remote_bridge();
        self.remote.switch(on, move || bridge)?;
        self.remote_preview_source();
        Ok(())
    }

    /// Gives the server the way to read the live preview back: the two textures the output thread
    /// draws it into, turn about, and which holds the newest.
    pub(crate) fn remote_preview_source(&mut self) {
        let Some(shared) = self.remote.shared().cloned() else { return };
        let Some(running) = self.running.as_ref() else { return };
        let Some(output) = running.output.as_ref() else { return };
        let newest = output.live_preview_shared();
        let textures = running.live_textures.clone();
        let (device, queue) = (running.gpu.device.clone(), running.gpu.queue.clone());
        shared.set_preview_read(Some(Arc::new(move || {
            let (front, drawn) = newest.newest();
            if drawn == 0 {
                return None;
            }
            let texture = textures.get(front)?;
            let pixels =
                sp_render::read_pixels_gently(&device, &queue, texture, std::time::Duration::from_secs(1)).ok()?;
            Some(server::Rgba { width: texture.width(), height: texture.height(), pixels })
        })));
    }

    /// Something the server asked of the main thread.
    pub(crate) fn remote_event(&mut self, el: &winit::event_loop::ActiveEventLoop, asked: ToApp) {
        match asked {
            ToApp::Command { caller, args, reply } => {
                // The policy was checked by the server; checked again here, where it counts.
                match policy::refusal(caller.role, &args) {
                    Some(refused) => {
                        let _ = reply.send(Reply::failed(refused));
                    }
                    None => {
                        let from = format!("{} ({}, {})", caller.name, caller.device, caller.ip);
                        // Files and folders on the computer stay closed while it runs, at the
                        // places they would be opened, whatever words reached the handler.
                        self.remote_call = true;
                        self.run_control(el, &args, Some(&from), |answer| {
                            let _ = reply.send(reply_for_page(answer));
                        });
                        self.remote_call = false;
                    }
                }
            }
            ToApp::Thumb { slide, version, look, reply } => {
                if self.remote.thumbs_waiting.len() >= MOST_WAITING_THUMBS {
                    let _ = reply.send(server::Thumbed::Missing);
                } else {
                    self.remote.thumbs_waiting.push_back((slide, version, look, reply));
                }
            }
            ToApp::Refresh { reply } => {
                self.tick_remote_state(true);
                let _ = reply.send(());
            }
            // Seen by `tick_remote`, which runs once the event loop has nothing else to do.
            ToApp::Pages => {}
        }
    }

    /// Does what Settings › Remote Control asked, keeps its pane up to date while it shows,
    /// publishes the state for pages and draws the slides they asked for. Says when to look again.
    pub(crate) fn tick_remote(&mut self) -> Option<Instant> {
        // Left on when the app last closed: on again, once the window is there.
        if !self.remote_started && self.running.is_some() {
            self.remote_started = true;
            if self.remote.wanted_on()
                && let Err(problem) = self.remote_switch(true)
            {
                self.remote.problem = Some(problem.clone());
                crate::applog::problem("remote", &format!("Could not switch Remote Control on: {problem}"));
            }
        }
        let asked = self.running.as_mut().and_then(|r| r.home_state.remote.request.take());
        if let Some(asked) = asked {
            let outcome = match asked {
                Request::Switch(on) => self.remote_switch(on).map(|()| None),
                Request::Port(port) => {
                    let bridge = self.remote_bridge();
                    let changed =
                        self.remote.set_port(port, move || bridge).map(|()| Some(format!("Port {port} it is.")));
                    self.remote_preview_source();
                    changed
                }
                Request::Password(password) => {
                    self.remote.set_password(password);
                    Ok(None)
                }
                Request::Forget(id) => {
                    self.remote.forget(&id);
                    Ok(None)
                }
                Request::ForgetAll => {
                    self.remote.forget_all();
                    Ok(None)
                }
                Request::Disconnect(page) => {
                    self.remote.disconnect(page);
                    Ok(None)
                }
                Request::NewCertificate => {
                    let bridge = self.remote_bridge();
                    let made = self
                        .remote
                        .new_certificate(move || bridge)
                        .map(|()| Some("A new certificate is made.".to_owned()));
                    self.remote_preview_source();
                    made
                }
            };
            let said = match outcome {
                Ok(said) => said.map(|said| (said, false)),
                Err(problem) => Some((problem, true)),
            };
            if let Some(running) = self.running.as_mut() {
                running.home_state.remote.said = said;
                running.home_window.request_redraw();
            }
        }
        if let Some(news) = self.remote.password_news()
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.remote.said = Some(match news {
                Ok(said) => (said, false),
                Err(problem) => (problem, true),
            });
            running.home_window.request_redraw();
        }
        // The pane shows the settings, devices and pages as they are now, while it is open; the
        // window is drawn again when the pages connected change.
        let pages = self.remote.page_count();
        let wrong = self.remote.shared().map_or(0, |shared| shared.wrong_passwords_lately());
        // Wrong passwords coming again and again are said once in the notices, which a remote
        // page can't clear; said again only after they stopped for the hour.
        if wrong >= WRONG_TO_SAY && !self.remote.wrong_said {
            self.remote.wrong_said = true;
            if let Some(running) = self.running.as_mut() {
                running.home_state.notices.secure(
                    crate::notices::Kind::Trouble,
                    "Wrong Remote Control passwords were tried several times in the last hour. See Settings › Remote Control.",
                );
                running.home_window.request_redraw();
            }
        } else if wrong == 0 {
            self.remote.wrong_said = false;
        }
        let showing = self.running.as_ref().is_some_and(|r| {
            r.home_state.preferences_open
                && crate::home::PANES.get(r.home_state.preferences_pane) == Some(&"Remote Control")
        });
        let view_at = showing.then(|| Instant::now() + std::time::Duration::from_secs(1));
        if showing
            || self.running.as_ref().is_some_and(|r| r.home_state.remote.on != self.remote.is_on())
            || pages != self.remote_pages_seen
            || self.running.as_ref().is_some_and(|r| r.home_state.remote.wrong != wrong)
        {
            self.remote_pages_seen = pages;
            let (on, port, address, address_ip, fingerprint) = (
                self.remote.is_on(),
                self.remote.port(),
                self.remote.address(),
                self.remote.address_by_ip(),
                self.remote.fingerprint().map(str::to_owned),
            );
            let (has_password, hashing, devices, pages) =
                (self.remote.has_password(), self.remote.hashing(), self.remote.devices(), self.remote.pages());
            if let Some(running) = self.running.as_mut() {
                let view = &mut running.home_state.remote;
                let changed = view.on != on || view.pages != pages || view.devices != devices || view.wrong != wrong;
                view.wrong = wrong;
                view.on = on;
                if view.port.is_empty() || changed {
                    view.port = port.to_string();
                }
                view.address_ip = address_ip.filter(|ip| address.as_ref() != Some(ip));
                view.address = address;
                view.fingerprint = fingerprint;
                view.has_password = has_password;
                view.hashing = hashing;
                view.devices = devices;
                view.pages = pages;
                if changed {
                    running.home_window.request_redraw();
                }
            }
        }
        // Devices not seen for half a year are forgotten, while on, every ten minutes too.
        if self.remote.is_on() && self.remote.devices_checked.is_none_or(|at| at.elapsed() >= CERT_CHECK_EVERY) {
            self.remote.devices_checked = Some(Instant::now());
            self.remote.forget_unused_devices();
        }
        // The certificate made again while nobody is connected, when it must be (a page
        // connected would have to accept the new one in the middle of a service).
        if self.remote.is_on()
            && pages == 0
            && self.remote.cert_checked.is_none_or(|at| at.elapsed() >= CERT_CHECK_EVERY)
        {
            self.remote.cert_checked = Some(Instant::now());
            let folder = self.remote.store.as_ref().and_then(|s| s.lock().ok().map(|s| s.folder().to_owned()));
            if let Some(folder) = folder
                && cert::needs_making(&folder, &cert::Names::now(cert::own_name()))
            {
                let bridge = self.remote_bridge();
                let off = self.remote.switch(false, unreachable_bridge);
                if let Err(problem) = off.and_then(|()| self.remote.switch(true, move || bridge)) {
                    crate::applog::problem("remote", &format!("Could not make a new certificate: {problem}"));
                }
                self.remote_preview_source();
            }
        }
        if let Some(news) = self.remote.cert_news.take()
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.remote.said = Some((news.clone(), false));
            running.home_state.notices.secure(crate::notices::Kind::Info, news);
            running.home_window.request_redraw();
        }
        let state_at = self.tick_remote_state(false);
        [state_at, view_at].into_iter().flatten().min()
    }
}

/// Settings › Remote Control as the window shows it: copied from the app while the pane is open,
/// and what the operator asked, for the app to do.
#[derive(Default)]
pub struct View {
    pub on: bool,
    pub port: String,
    pub address: Option<String>,
    /// The address as figures, when the one above is by name.
    pub address_ip: Option<String>,
    pub fingerprint: Option<String>,
    pub has_password: bool,
    pub hashing: bool,
    pub password: String,
    pub password_again: String,
    pub devices: Vec<auth::Device>,
    pub pages: Vec<server::Page>,
    /// Wrong passwords in the last hour.
    pub wrong: usize,
    /// What to tell the operator: a problem, or how a change went.
    pub said: Option<(String, bool)>,
    pub request: Option<Request>,
}

/// What Settings › Remote Control asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Switch(bool),
    Port(u16),
    Password(String),
    Forget(String),
    ForgetAll,
    Disconnect(u64),
    NewCertificate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_cut_to_file_names_and_nothing_else_is_touched() {
        assert_eq!(
            without_paths("a picture (/Users/pat/Documents/SoftPresenter/Images/x 1.png)"),
            "a picture (x 1.png)"
        );
        assert_eq!(without_paths("/Users/pat/Movies/Walk in.mp4"), "Walk in.mp4");
        assert_eq!(
            without_paths("Run: launchctl unload /Users/pat/Library/LaunchAgents/com.old.plist\nthen"),
            "Run: launchctl unload com.old.plist\nthen"
        );
        assert_eq!(without_paths("Saved C:\\Users\\pat\\song.sppack, done"), "Saved song.sppack, done");
        // Words that only look like paths in part are left as they are.
        for text in [
            "slide 2/3",
            "https://example.org/a/b",
            "1/2 of the way",
            "On screen: River Song.",
            "and/or",
            "/",
            "Psalm 23 /Gospel version",
            "C /E /G",
            "Medley /Part 1/ Part 2",
            "Verse 1 /Chorus/ Verse 2 /Bridge/ end",
            "(/a/b/c)",
            "/thumb/7f6a1c2e-0000-4000-8000-000000000001/usual/00ff00ff00ff00ff.jpg",
        ] {
            assert_eq!(without_paths(text), text);
        }
        // The state's own picture addresses stay whole.
        let mut part = serde_json::json!({"thumb": "/thumb/a/usual/b.jpg", "poster": "/thumb/c/usual/d.jpg"});
        json_without_paths(&mut part);
        assert_eq!(part, serde_json::json!({"thumb": "/thumb/a/usual/b.jpg", "poster": "/thumb/c/usual/d.jpg"}));
        let mut value = serde_json::json!({"raw": {"objects": [{"file": "/Users/pat/x.png", "words": "a/b"}]}, "n": 1});
        json_without_paths(&mut value);
        assert_eq!(value, serde_json::json!({"raw": {"objects": [{"file": "x.png", "words": "a/b"}]}, "n": 1}));
        // A slide's words with slashes read back whole, in its raw document and beside it.
        let words = serde_json::json!({
            "words": "C /E /G\nMedley /Part 1/ Part 2",
            "title": "Medley /Part 1/ Part 2",
            "raw": {"objects": [{"file": "", "text": "C /E /G /Users/ ok"}, {"file": "/Volumes/Church/Sky.png"}]},
        });
        let mut cut = words.clone();
        json_without_paths(&mut cut);
        assert_eq!(cut["words"], words["words"]);
        assert_eq!(cut["title"], words["title"]);
        assert_eq!(cut["raw"]["objects"][0], words["raw"]["objects"][0]);
        assert_eq!(cut["raw"]["objects"][1]["file"], "Sky.png");
        assert_eq!(without_paths("a picture (/Library/Pictures/Sky 1.png)"), "a picture (Sky 1.png)");
        assert_eq!(without_paths("Cannot open file:///Volumes/Church/a.mp4"), "Cannot open a.mp4");
        assert_eq!(without_paths("Cannot read \\\\server\\share\\a.png"), "Cannot read a.png");
        let reply = reply_for_page(Reply::with("Made /Users/pat/a.txt", serde_json::json!(["/private/tmp/b.png"])));
        assert_eq!(reply.text, "Made a.txt");
        assert_eq!(reply.data, serde_json::json!(["b.png"]));
    }

    #[test]
    fn the_clock_gives_one_time_for_one_instant() {
        let clock = Clock::new();
        let later = Instant::now() + std::time::Duration::from_millis(1500);
        assert_eq!(clock.ms(later), clock.ms(later));
        assert!(clock.ms(later) >= clock.unix_ms + 1500);
        let earlier = clock.at - std::time::Duration::from_millis(250);
        assert_eq!(clock.ms(earlier), clock.unix_ms - 250);
    }
}
