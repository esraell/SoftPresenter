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
        reply: oneshot::Sender<Option<server::Drawn>>,
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

    fn thumb(&self, slide: Uuid, version: Option<Uuid>) -> oneshot::Receiver<Option<server::Drawn>> {
        let (reply, answer) = oneshot::channel();
        (self.send)(ToApp::Thumb { slide, version, reply });
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
    /// What went wrong at the last switch, to show in Settings.
    pub problem: Option<String>,
    /// What a password being set came to, from its thread.
    password_said: Arc<Mutex<Option<Result<String, String>>>>,
    hashing: bool,
    /// Slides waiting to be drawn for pages, a few at a time.
    pub(crate) thumbs_waiting: VecDeque<(Uuid, Option<Uuid>, oneshot::Sender<Option<server::Drawn>>)>,
    /// When the state was last published, and whether something may have changed since.
    pub(crate) published: Option<Instant>,
    pub clock: Clock,
    pub(crate) looks: state::Looks,
}

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
            problem: None,
            password_said: Arc::default(),
            hashing: false,
            thumbs_waiting: VecDeque::new(),
            published: None,
            clock: Clock::new(),
            looks: state::Looks::default(),
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

    /// The address to open on another device, while on.
    pub fn address(&self) -> Option<String> {
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
        let cert = cert::load_or_make(&folder)?;
        let fingerprint = cert.fingerprint.clone();
        let shared = server::Shared::new(store, Box::new(bridge()))?;
        let running = server::start(server::Config { bind: bind_address(), port, cert }, shared)?;
        crate::applog::event("remote", &format!("Switched on, on port {}.", running.local.port()));
        self.running = Some(running);
        self.fingerprint = Some(fingerprint);
        self.published = None;
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

    /// Closes one page's connection; its device stays remembered.
    pub fn disconnect(&self, page: u64) {
        if let Some(shared) = self.shared() {
            shared.disconnect(page);
        }
    }

    /// Makes a new certificate; with Remote Control on, it listens again with it (pages
    /// reconnect, and browsers warn again: the fingerprint is new).
    pub fn new_certificate(&mut self, bridge: impl FnOnce() -> Bridge) -> Result<(), String> {
        let folder =
            self.store.as_ref().and_then(|s| s.lock().ok().map(|s| s.folder().to_owned())).ok_or("No data folder.")?;
        let cert = cert::make(&folder)?;
        self.fingerprint = Some(cert.fingerprint);
        crate::applog::event("remote", "Made a new certificate.");
        if self.running.is_some() {
            self.switch(false, unreachable_bridge)?;
            self.switch(true, bridge)?;
        }
        Ok(())
    }

    /// Writes the devices' last-seen times, at most once a minute.
    pub fn keep_seen(&self) {
        if let Some(mut store) = self.store.as_ref().and_then(|store| store.lock().ok()) {
            store.save_seen();
        }
    }
}

/// Switching off needs no bridge.
fn unreachable_bridge() -> Bridge {
    Bridge::new(|_| false)
}

impl crate::App {
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
        let mut lines = vec![match (&address, said) {
            (_, Some(said)) => said,
            (Some(address), None) => format!("Remote Control is on: open {address}"),
            (None, None) => "Remote Control is off.".to_owned(),
        }];
        if let Some(fingerprint) = self.remote.fingerprint().filter(|_| self.remote.is_on()) {
            lines.push(format!("Certificate fingerprint (SHA-256): {fingerprint}"));
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
            let pixels = sp_render::read_pixels(&device, &queue, texture).ok()?;
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
                        self.run_control(el, &args, Some(&from), |answer| {
                            let _ = reply.send(answer);
                        });
                    }
                }
            }
            ToApp::Thumb { slide, version, reply } => {
                if self.remote.thumbs_waiting.len() >= MOST_WAITING_THUMBS {
                    let _ = reply.send(None);
                } else {
                    self.remote.thumbs_waiting.push_back((slide, version, reply));
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
        let showing = self.running.as_ref().is_some_and(|r| {
            r.home_state.preferences_open
                && crate::home::PANES.get(r.home_state.preferences_pane) == Some(&"Remote Control")
        });
        let view_at = showing.then(|| Instant::now() + std::time::Duration::from_secs(1));
        if showing
            || self.running.as_ref().is_some_and(|r| r.home_state.remote.on != self.remote.is_on())
            || pages != self.remote_pages_seen
        {
            self.remote_pages_seen = pages;
            let (on, port, address, fingerprint) = (
                self.remote.is_on(),
                self.remote.port(),
                self.remote.address(),
                self.remote.fingerprint().map(str::to_owned),
            );
            let (has_password, hashing, devices, pages) =
                (self.remote.has_password(), self.remote.hashing(), self.remote.devices(), self.remote.pages());
            if let Some(running) = self.running.as_mut() {
                let view = &mut running.home_state.remote;
                let changed = view.on != on || view.pages != pages || view.devices != devices;
                view.on = on;
                if view.port.is_empty() || changed {
                    view.port = port.to_string();
                }
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
    pub fingerprint: Option<String>,
    pub has_password: bool,
    pub hashing: bool,
    pub password: String,
    pub password_again: String,
    pub devices: Vec<auth::Device>,
    pub pages: Vec<server::Page>,
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
    fn the_clock_gives_one_time_for_one_instant() {
        let clock = Clock::new();
        let later = Instant::now() + std::time::Duration::from_millis(1500);
        assert_eq!(clock.ms(later), clock.ms(later));
        assert!(clock.ms(later) >= clock.unix_ms + 1500);
        let earlier = clock.at - std::time::Duration::from_millis(250);
        assert_eq!(clock.ms(earlier), clock.unix_ms - 250);
    }
}
