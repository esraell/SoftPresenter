//! The Remote Control server: HTTPS and one WebSocket a page, on a thread of its own with a small
//! tokio runtime of its own, running only while Remote Control is switched on. Switching it off
//! ends the runtime: the port is let go and every connection closes.
//!
//! It never touches the windows or the screens. Everything it asks of the app goes through the
//! [`Backend`] (in the app, the event loop's proxy: a command is carried out on the main thread
//! by the same handler `sp` reaches, and its reply comes back), and the state it sends is what
//! the app publishes into the [`Hub`].
//!
//! What every request goes through, in order:
//! 1. Connections: at most [`MOST_CONNECTIONS`], [`MOST_FROM_ONE_ADDRESS`] from one address; the
//!    TLS handshake must finish within [`HANDSHAKE_WITHIN`]; HTTP/1.1 only (no h2), headers within
//!    [`HEADERS_WITHIN`] and [`MOST_HEADER_BYTES`]; a connection lasts [`LONGEST_CONNECTION`] (a
//!    WebSocket, once upgraded, lives on its own).
//! 2. `Host` must be the address the connection came in on, with the port (or `localhost` on
//!    this computer): a page from another site that got its name pointed here (DNS rebinding)
//!    is refused.
//! 3. A device token (the cookie given at sign-in) for everything but the sign-in page, its
//!    script and the style sheet.
//! 4. `Origin` must be this server's on the WebSocket and on every POST: a page elsewhere can't
//!    use a signed-in browser's cookie (cross-site requests; the cookie is SameSite=Strict too).
//! 5. No file is served by a path from a request: the pages are compiled into the app, and
//!    pictures are found by slide id.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{HeaderValue, Method, Response, StatusCode, header, request::Parts};
use axum::response::IntoResponse;
use axum::routing::any;
use sp_model::Uuid;
use tokio::sync::{Notify, Semaphore, mpsc, oneshot, watch};
use tokio_rustls::rustls;

use super::auth;
use super::hub::{self, Hub};
use super::policy::{self, Role};
use crate::control::Reply;

/// Connections at once, from everywhere.
pub const MOST_CONNECTIONS: usize = 64;
/// Connections at once from one address.
pub const MOST_FROM_ONE_ADDRESS: usize = 16;
/// Pages connected at once (WebSockets), and from one device.
pub const MOST_PAGES: usize = 16;
pub const MOST_PAGES_A_DEVICE: usize = 4;
pub const HANDSHAKE_WITHIN: Duration = Duration::from_secs(10);
pub const HEADERS_WITHIN: Duration = Duration::from_secs(10);
pub const MOST_HEADER_BYTES: usize = 16 * 1024;
pub const LONGEST_CONNECTION: Duration = Duration::from_secs(120);
/// The longest sign-in request.
pub const MOST_LOGIN_BYTES: usize = 4 * 1024;
/// The longest message a page may send (as the control socket's request).
pub const MOST_MESSAGE_BYTES: usize = 64 * 1024;
/// A page is sent a heartbeat this often, and let go when nothing has come from it for
/// [`SILENT_FOR`].
pub const BEAT_EVERY: Duration = Duration::from_secs(15);
pub const SILENT_FOR: Duration = Duration::from_secs(45);
/// A page must say hello within this.
pub const HELLO_WITHIN: Duration = Duration::from_secs(10);
/// A message to a page must be written within this, or the page is let go (it reconnects and
/// catches up).
pub const WRITE_WITHIN: Duration = Duration::from_secs(10);
/// Commands from one page waiting for the app at once; and how many a second it may send,
/// with a burst of twice that.
pub const MOST_WAITING_COMMANDS: usize = 4;
pub const COMMANDS_A_SECOND: f64 = 20.0;
/// Commands from every page together, a second (twice that at once): the app's main thread
/// carries each out, so all the pages at once can't keep it busy.
pub const ALL_COMMANDS_A_SECOND: f64 = 40.0;
/// Pages one device may connect, a minute (ten at once): a page reconnecting over and over
/// can't keep the app publishing fresh snapshots.
pub const CONNECTS_A_MINUTE: f64 = 12.0;
/// How long a command may take before the page is told the app didn't answer (as `sp`).
pub const ANSWER_WITHIN: Duration = Duration::from_secs(15);
/// The live preview is sent at most this often.
pub const PREVIEW_EVERY: Duration = Duration::from_millis(100);
/// Slide pictures being drawn for pages at once, and the bytes of them kept.
pub const MOST_DRAWING: usize = 2;
pub const THUMB_CACHE_BYTES: usize = 24 * 1024 * 1024;
/// Slide pictures kept at once, whatever their size.
pub const MOST_THUMBS_KEPT: usize = 4096;

/// The cookie a remembered device carries. `__Host-` makes browsers insist on Secure, path /
/// and no domain.
pub const COOKIE: &str = "__Host-sp_device";

/// WebSocket close codes the page reads.
pub mod close {
    /// Signed out: the device was forgotten, or the password changed. Sign in again.
    pub const SIGNED_OUT: u16 = 4401;
    /// Disconnected at the computer, in Settings › Remote Control. Don't reconnect by itself.
    pub const DISCONNECTED: u16 = 4000;
    /// Remote Control was switched off.
    pub const SWITCHED_OFF: u16 = 1001;
    /// Too slow to take what it was sent, silent too long, or no hello: reconnect and catch up.
    pub const TOO_SLOW: u16 = 4008;
    /// Too many pages connected.
    pub const FULL: u16 = 1013;
}

const LOGIN_HTML: &str = include_str!("page/login.html");
const LOGIN_JS: &str = include_str!("page/login.js");
const INDEX_HTML: &str = include_str!("page/index.html");
const APP_JS: &str = include_str!("page/app.js");
const STYLE_CSS: &str = include_str!("page/style.css");

/// The design work's page, served beside the built-in one under `/next/` (the owner has not
/// decided how it should look on narrow screens). A fixed table of exact names: nothing is
/// ever looked up by a path from a request.
const NEXT_INDEX_HTML: &str = include_str!("page/next/index.html");
const NEXT_APP_JS: &str = include_str!("page/next/app.js");
const NEXT_STYLE_CSS: &str = include_str!("page/next/style.css");

/// The files under `/next/`: content type (from the extension) and body. The page's own HTML
/// names its script and style as `/app.js` and `/style.css`, which are the built-in page's;
/// here those two names are read as the files beside it. The files themselves are the design
/// work's, unchanged.
fn next_file(path: &str) -> Option<(&'static str, &'static str)> {
    static INDEX: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let kind = |name: &str| match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    };
    match path {
        "/next/" => Some((
            kind("index.html"),
            INDEX.get_or_init(|| {
                NEXT_INDEX_HTML
                    .replace("href=\"/style.css\"", "href=\"/next/style.css\"")
                    .replace("src=\"/app.js\"", "src=\"/next/app.js\"")
            }),
        )),
        "/next/app.js" => Some((kind("app.js"), NEXT_APP_JS)),
        "/next/style.css" => Some((kind("style.css"), NEXT_STYLE_CSS)),
        _ => None,
    }
}

/// A picture read back from the GPU: tightly packed RGBA rows.
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A slide drawn for a page: how to read it back, which blocks and so is done on a thread of
/// the server's own.
pub struct Drawn {
    pub read: Box<dyn FnOnce() -> Option<Rgba> + Send>,
}

/// What the app says to a request for a slide's picture.
pub enum Thumbed {
    /// Drawn: the look asked for is the current one.
    Drawn(Drawn),
    /// The slide looks different now: its current look, for the address to go to instead. Nothing
    /// was drawn.
    Moved(String),
    /// There is no such slide.
    Missing,
}

/// Reads the newest live preview back (blocks; done on a thread of the server's own).
pub type PreviewRead = Arc<dyn Fn() -> Option<Rgba> + Send + Sync>;

/// Who sent a command: for the log, and for what the role allows.
#[derive(Clone, Debug)]
pub struct Caller {
    pub client: u64,
    pub device: String,
    pub name: String,
    pub ip: IpAddr,
    pub role: Role,
}

/// What the server asks of the app. Each answer comes back on its channel; none blocks.
pub trait Backend: Send + Sync + 'static {
    /// Carries a command out as `sp`'s are, and replies.
    fn command(&self, caller: Caller, args: Vec<String>) -> oneshot::Receiver<Reply>;
    /// Draws a slide (by id, in a theme version or the usual look) as the window's thumbnails
    /// are drawn, but only when `look` is the look it has now: the app compares first, so an
    /// address with a made-up look draws nothing.
    fn thumb(&self, slide: Uuid, version: Option<Uuid>, look: String) -> oneshot::Receiver<Thumbed>;
    /// Publishes the state now (a page has just connected); answers once it has.
    fn refresh(&self) -> oneshot::Receiver<()>;
    /// A page connected or went: Settings and the status line show how many.
    fn pages_changed(&self);
    /// A line for the app's log.
    fn log(&self, line: &str);
}

/// One page connected, as Settings lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub id: u64,
    pub device: String,
    pub name: String,
    pub ip: IpAddr,
    pub since: SystemTime,
}

/// Asks a page's connection to close, with a code from [`close`].
#[derive(Default)]
struct Kick {
    notify: Notify,
    code: AtomicU16,
}

impl Kick {
    fn kick(&self, code: u16) {
        self.code.store(code, Ordering::Release);
        self.notify.notify_one();
    }
}

struct Connected {
    page: Page,
    kick: Arc<Kick>,
}

/// A slide picture kept, by slide and version: its look and its JPEG.
struct Kept {
    look: String,
    jpeg: Bytes,
    used: u64,
}

#[derive(Default)]
struct ThumbCache {
    kept: HashMap<(Uuid, Option<Uuid>), Kept>,
    bytes: usize,
    tick: u64,
}

impl ThumbCache {
    fn get(&mut self, key: &(Uuid, Option<Uuid>), look: &str) -> Option<Bytes> {
        self.tick += 1;
        let tick = self.tick;
        let kept = self.kept.get_mut(key).filter(|kept| kept.look == look)?;
        kept.used = tick;
        Some(kept.jpeg.clone())
    }

    fn put(&mut self, key: (Uuid, Option<Uuid>), look: String, jpeg: Bytes) {
        self.tick += 1;
        if let Some(old) = self.kept.remove(&key) {
            self.bytes -= old.jpeg.len();
        }
        self.bytes += jpeg.len();
        self.kept.insert(key, Kept { look, jpeg, used: self.tick });
        while self.bytes > THUMB_CACHE_BYTES || self.kept.len() > MOST_THUMBS_KEPT {
            let Some(oldest) = self.kept.iter().min_by_key(|(_, kept)| kept.used).map(|(key, _)| *key) else { break };
            if let Some(gone) = self.kept.remove(&oldest) {
                self.bytes -= gone.jpeg.len();
            }
        }
    }
}

/// What the server and the app share while Remote Control is on.
pub struct Shared {
    pub store: Arc<Mutex<auth::Store>>,
    hub: Mutex<Hub>,
    hub_tx: watch::Sender<u64>,
    pages: Mutex<Vec<Connected>>,
    page_count: AtomicUsize,
    next_page: AtomicU64,
    guard: Mutex<auth::Guard>,
    hashing: Semaphore,
    drawing: Semaphore,
    thumbs: Mutex<ThumbCache>,
    preview_watchers: AtomicUsize,
    preview_wanted: Notify,
    preview_frame: watch::Sender<Option<Bytes>>,
    preview_drawn: watch::Sender<u64>,
    preview_read: Mutex<Option<PreviewRead>>,
    backend: Box<dyn Backend>,
    connections: Mutex<HashMap<IpAddr, usize>>,
    connection_count: AtomicUsize,
    closing: watch::Sender<bool>,
    /// Commands from every page together.
    commands: Mutex<Bucket>,
    /// Connects by each device.
    connects: Mutex<HashMap<String, Bucket>>,
    /// The theme versions there are: a picture asked for in any other is drawn in the usual
    /// look, so made-up version ids can't make the app draw again and again.
    versions: Mutex<std::collections::HashSet<Uuid>>,
    /// The computer's own name on the network (`church-mac.local`), answered as a Host as its
    /// address is.
    local_name: Mutex<Option<String>>,
}

impl Shared {
    pub fn new(store: Arc<Mutex<auth::Store>>, backend: Box<dyn Backend>) -> Result<Arc<Shared>, String> {
        let epoch = auth::random_word(9)?;
        Ok(Arc::new(Shared {
            store,
            hub: Mutex::new(Hub::new(epoch)),
            hub_tx: watch::channel(0).0,
            pages: Mutex::new(Vec::new()),
            page_count: AtomicUsize::new(0),
            next_page: AtomicU64::new(1),
            guard: Mutex::new(auth::Guard::default()),
            hashing: Semaphore::new(1),
            drawing: Semaphore::new(MOST_DRAWING),
            thumbs: Mutex::new(ThumbCache::default()),
            preview_watchers: AtomicUsize::new(0),
            preview_wanted: Notify::new(),
            preview_frame: watch::channel(None).0,
            preview_drawn: watch::channel(0).0,
            preview_read: Mutex::new(None),
            backend,
            connections: Mutex::new(HashMap::new()),
            connection_count: AtomicUsize::new(0),
            closing: watch::channel(false).0,
            commands: Mutex::new(Bucket::new(ALL_COMMANDS_A_SECOND, ALL_COMMANDS_A_SECOND * 2.0)),
            connects: Mutex::new(HashMap::new()),
            versions: Mutex::new(std::collections::HashSet::new()),
            local_name: Mutex::new(None),
        }))
    }

    /// Takes the state as it is now (from the app's main thread); pages are sent what changed.
    pub fn publish<'a>(&self, parts: impl IntoIterator<Item = (&'a str, String)>) {
        let version = {
            let Ok(mut hub) = self.hub.lock() else { return };
            if !hub.publish(parts) {
                return;
            }
            hub.version()
        };
        self.hub_tx.send_replace(version);
    }

    /// How many pages are connected now.
    pub fn page_count(&self) -> usize {
        self.page_count.load(Ordering::Acquire)
    }

    /// The pages connected now.
    pub fn pages(&self) -> Vec<Page> {
        self.pages.lock().map(|pages| pages.iter().map(|c| c.page.clone()).collect()).unwrap_or_default()
    }

    /// Closes one page's connection (its device stays remembered).
    pub fn disconnect(&self, id: u64) {
        self.kick_where(|page| page.id == id, close::DISCONNECTED);
    }

    /// Closes the connections of a device that was forgotten.
    pub fn sign_out(&self, device: &str) {
        self.kick_where(|page| page.device == device, close::SIGNED_OUT);
    }

    /// Closes every connection, every device having been forgotten.
    pub fn sign_out_all(&self) {
        self.kick_where(|_| true, close::SIGNED_OUT);
    }

    fn kick_where(&self, which: impl Fn(&Page) -> bool, code: u16) {
        if let Ok(pages) = self.pages.lock() {
            for connected in pages.iter().filter(|c| which(&c.page)) {
                connected.kick.kick(code);
            }
        }
    }

    /// The live preview has a new picture (from the app's main thread).
    pub fn preview_drawn(&self, drawn: u64) {
        if self.preview_watchers.load(Ordering::Acquire) > 0 {
            self.preview_drawn.send_replace(drawn);
        }
    }

    /// How to read the live preview back, once the app has one.
    pub fn set_preview_read(&self, read: Option<PreviewRead>) {
        if let Ok(mut slot) = self.preview_read.lock() {
            *slot = read;
        }
    }

    /// The theme versions there are now (from the app's main thread).
    pub fn set_versions(&self, versions: impl IntoIterator<Item = Uuid>) {
        if let Ok(mut known) = self.versions.lock() {
            *known = versions.into_iter().collect();
        }
    }

    /// The computer's own name on the network, answered as a Host too.
    pub fn set_local_name(&self, name: Option<String>) {
        if let Ok(mut slot) = self.local_name.lock() {
            *slot = name.map(|name| name.to_ascii_lowercase());
        }
    }

    /// Wrong passwords in the last hour, for Settings and the status line.
    pub fn wrong_passwords_lately(&self) -> usize {
        self.guard.lock().map_or(0, |guard| guard.wrong_lately(Instant::now()))
    }

    /// Whether a device is still remembered.
    fn remembered(&self, device: &str) -> bool {
        self.store.lock().is_ok_and(|store| store.saved.devices.iter().any(|d| d.id == device))
    }

    /// The pictures of slides kept are let go (the themes changed, say): they are drawn again
    /// when next asked for. Their looks would differ anyway; this only frees the memory sooner.
    pub fn forget_pictures(&self) {
        if let Ok(mut thumbs) = self.thumbs.lock() {
            *thumbs = ThumbCache::default();
        }
    }
}

/// Where Remote Control listens, and with what certificate.
pub struct Config {
    pub bind: IpAddr,
    pub port: u16,
    pub cert: super::cert::Cert,
}

/// The server while it runs. Dropping it (or `stop`) switches it off.
pub struct Running {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub shared: Arc<Shared>,
    pub local: SocketAddr,
}

impl Running {
    /// Switches the server off: every connection closes and the port is let go once the
    /// returned thread ends (within about a second).
    pub fn stop(mut self) -> Option<std::thread::JoinHandle<()>> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.thread.take()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// Starts listening (at once, so a port in use is said now) and serving on a thread of its own.
pub fn start(config: Config, shared: Arc<Shared>) -> Result<Running, String> {
    if (1..1024).contains(&config.port) {
        return Err("Choose a port from 1024 up.".to_owned());
    }
    let listener = std::net::TcpListener::bind((config.bind, config.port)).map_err(|e| match e.kind() {
        std::io::ErrorKind::AddrInUse => format!("Port {} is in use by another program; choose another.", config.port),
        _ => format!("Could not listen on port {}: {e}", config.port),
    })?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let local = listener.local_addr().map_err(|e| e.to_string())?;
    let tls = tls_config(&config.cert)?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let serving = shared.clone();
    let thread = std::thread::Builder::new()
        .name("remote".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .max_blocking_threads(4)
                .thread_name("remote-work")
                .build()
            {
                Ok(runtime) => runtime,
                Err(e) => {
                    serving.backend.log(&format!("Remote Control could not start: {e}"));
                    return;
                }
            };
            runtime.block_on(serve(listener, tls, serving, stop_rx));
            // What is still reading a picture back or checking a password is given a moment.
            runtime.shutdown_timeout(Duration::from_secs(1));
        })
        .map_err(|e| format!("Could not start Remote Control: {e}"))?;
    Ok(Running { stop: Some(stop_tx), thread: Some(thread), shared, local })
}

/// TLS 1.2 and 1.3 as rustls has them (ring), HTTP/1.1 only.
fn tls_config(cert: &super::cert::Cert) -> Result<Arc<rustls::ServerConfig>, String> {
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(cert.chain.clone(), cert.key.clone_key())
        .map_err(|e| format!("The certificate can't be used: {e}"))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Where a request came in: the address the page reached, and the page's own.
#[derive(Clone, Copy, Debug)]
struct Conn {
    local: SocketAddr,
    peer: SocketAddr,
}

/// A place among the connections, given back when the connection ends.
struct Slot {
    shared: Arc<Shared>,
    ip: IpAddr,
}

impl Slot {
    fn take(shared: &Arc<Shared>, ip: IpAddr) -> Option<Slot> {
        let mut connections = shared.connections.lock().ok()?;
        let from_here = connections.get(&ip).copied().unwrap_or(0);
        if shared.connection_count.load(Ordering::Acquire) >= MOST_CONNECTIONS || from_here >= MOST_FROM_ONE_ADDRESS {
            return None;
        }
        connections.insert(ip, from_here + 1);
        shared.connection_count.fetch_add(1, Ordering::AcqRel);
        Some(Slot { shared: shared.clone(), ip })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if let Ok(mut connections) = self.shared.connections.lock()
            && let Some(count) = connections.get_mut(&self.ip)
        {
            *count -= 1;
            if *count == 0 {
                connections.remove(&self.ip);
            }
        }
        self.shared.connection_count.fetch_sub(1, Ordering::AcqRel);
    }
}

async fn serve(
    listener: std::net::TcpListener,
    tls: Arc<rustls::ServerConfig>,
    shared: Arc<Shared>,
    mut stop: oneshot::Receiver<()>,
) {
    let listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(listener) => listener,
        Err(e) => {
            shared.backend.log(&format!("Remote Control could not listen: {e}"));
            return;
        }
    };
    let acceptor = tokio_rustls::TlsAcceptor::from(tls);
    let router = router(shared.clone());
    let feed = tokio::spawn(preview_feed(shared.clone()));
    loop {
        let accepted = tokio::select! {
            _ = &mut stop => break,
            accepted = listener.accept() => accepted,
        };
        let Ok((tcp, peer)) = accepted else {
            // Out of file handles, say: a moment's rest rather than a busy loop.
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        };
        // Only the local network: a computer reached from the internet (a forwarded port, a
        // public address) answers nobody out there.
        if !local_network(peer.ip()) {
            continue;
        }
        let Some(slot) = Slot::take(&shared, peer.ip()) else { continue };
        let Ok(local) = tcp.local_addr() else { continue };
        let _ = tcp.set_nodelay(true);
        let (acceptor, router) = (acceptor.clone(), router.clone());
        tokio::spawn(async move {
            let _slot = slot;
            let Ok(Ok(stream)) = tokio::time::timeout(HANDSHAKE_WITHIN, acceptor.accept(tcp)).await else { return };
            let conn = Conn { local, peer };
            let service = hyper::service::service_fn(move |mut request: Request<hyper::body::Incoming>| {
                request.extensions_mut().insert(conn);
                tower::ServiceExt::oneshot(router.clone(), request)
            });
            let mut http = hyper::server::conn::http1::Builder::new();
            http.timer(hyper_util::rt::TokioTimer::new())
                .header_read_timeout(HEADERS_WITHIN)
                .max_buf_size(MOST_HEADER_BYTES.max(8192))
                .max_headers(64)
                .keep_alive(true);
            let serving = http.serve_connection(hyper_util::rt::TokioIo::new(stream), service).with_upgrades();
            let _ = tokio::time::timeout(LONGEST_CONNECTION, serving).await;
        });
    }
    // Every page is told Remote Control is switching off, and given a moment to hear it.
    shared.closing.send_replace(true);
    drop(listener);
    feed.abort();
    tokio::time::sleep(Duration::from_millis(300)).await;
}

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/", any(root))
        .route("/login", any(login_page))
        .route("/login.js", any(login_js))
        .route("/style.css", any(style_css))
        .route("/app.js", any(app_js))
        .route("/favicon.ico", any(favicon))
        .route("/next/", any(next_page))
        .route("/next/{file}", any(next_page))
        .route("/api/login", any(login))
        .route("/api/logout", any(logout))
        .route("/api/ws", any(socket))
        .route("/thumb/{slide}/{version}/{file}", any(thumb))
        .fallback(not_found)
        .with_state(shared)
}

/// What the checks found out about a request that passed them.
struct Gate {
    host: String,
    ip: IpAddr,
    /// The device signed in, by id and name, when the request carried a token that is one.
    device: Option<(String, String)>,
}

/// Checks a request's `Host` against the address its connection came in on, and reads its
/// device token. `Err` is the answer to send instead.
fn gate(shared: &Shared, parts: &Parts) -> Result<Gate, Box<Response<Body>>> {
    let Some(conn) = parts.extensions.get::<Conn>().copied() else {
        return Err(Box::new(plain(StatusCode::INTERNAL_SERVER_ERROR, "", "Something went wrong.")));
    };
    let host = parts.headers.get(header::HOST).and_then(|value| value.to_str().ok()).unwrap_or("");
    let named = shared.local_name.lock().ok().and_then(|name| name.clone());
    if !host_allowed(host, conn.local)
        && !named.is_some_and(|name| host.eq_ignore_ascii_case(&format!("{name}:{}", conn.local.port())))
    {
        return Err(Box::new(plain(
            StatusCode::MISDIRECTED_REQUEST,
            "",
            "This address isn't one SoftPresenter serves.",
        )));
    }
    // A request naming another place in its first line is refused too.
    if parts.uri.authority().is_some_and(|authority| !authority.as_str().eq_ignore_ascii_case(host)) {
        return Err(Box::new(plain(
            StatusCode::MISDIRECTED_REQUEST,
            "",
            "This address isn't one SoftPresenter serves.",
        )));
    }
    let host = host.to_ascii_lowercase();
    let ip = conn.peer.ip();
    let mut save = false;
    let device = cookie(parts).and_then(|token| {
        let mut store = shared.store.lock().ok()?;
        let device = store.device_of(&token).map(|d| (d.id.clone(), d.name.clone()))?;
        store.seen(&device.0, ip);
        save = store.seen_due();
        Some(device)
    });
    // When devices were last seen is written at most once a minute, on a thread for such work.
    if save {
        let store = shared.store.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(mut store) = store.lock() {
                store.save_seen();
            }
        });
    }
    Ok(Gate { host, ip, device })
}

/// Whether an address is on a local network: this computer, the private ranges (10/8,
/// 172.16/12, 192.168/16), link-local (169.254/16), and the shared range some networks and
/// VPNs hand out (100.64/10); for IPv6, this computer, unique local (fc00::/7) and link-local
/// (fe80::/10). Anything else is the internet.
pub fn local_network(ip: IpAddr) -> bool {
    let v4 = match ip {
        IpAddr::V4(v4) => Some(v4),
        IpAddr::V6(v6) => v6.to_ipv4_mapped(),
    };
    match (v4, ip) {
        (Some(v4), _) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || (a == 100 && (64..128).contains(&b))
        }
        (None, IpAddr::V6(v6)) => {
            let first = v6.segments()[0];
            v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
        (None, IpAddr::V4(_)) => false,
    }
}

/// Whether a `Host` is the address a connection came in on, with the server's port: the
/// address as figures (IPv6 in brackets), or `localhost` on this computer.
fn host_allowed(host: &str, local: SocketAddr) -> bool {
    let port = local.port();
    let ip = match local.ip() {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    };
    let figures = match ip {
        IpAddr::V4(v4) => format!("{v4}:{port}"),
        IpAddr::V6(v6) => format!("[{v6}]:{port}"),
    };
    host.eq_ignore_ascii_case(&figures) || (ip.is_loopback() && host.eq_ignore_ascii_case(&format!("localhost:{port}")))
}

/// Whether a request's `Origin` is this server's own page.
fn origin_allowed(parts: &Parts, host: &str) -> bool {
    parts
        .headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| origin.eq_ignore_ascii_case(&format!("https://{host}")))
}

/// The device token in a request's cookies.
fn cookie(parts: &Parts) -> Option<String> {
    let prefix = format!("{COOKIE}=");
    parts
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .map(str::trim)
        .find_map(|pair| pair.strip_prefix(&prefix))
        .filter(|token| !token.is_empty() && token.len() <= 128)
        .map(str::to_owned)
}

/// The headers every answer carries: what the page may load (only its own scripts and styles,
/// pictures from here, the WebSocket here), and never in a frame, sniffed or cached.
fn finish(mut response: Response<Body>, host: &str) -> Response<Body> {
    let headers = response.headers_mut();
    let policy = format!(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; connect-src 'self' wss://{host}; \
         base-uri 'none'; form-action 'self'; frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'"
    );
    if let Ok(policy) = HeaderValue::from_str(&policy) {
        headers.insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    headers.insert("cross-origin-resource-policy", HeaderValue::from_static("same-origin"));
    headers.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("no-store"));
    response
}

fn plain(status: StatusCode, host: &str, words: &str) -> Response<Body> {
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(words.to_owned()))
        .unwrap_or_default();
    finish(response, host)
}

fn json(status: StatusCode, host: &str, value: serde_json::Value) -> Response<Body> {
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(value.to_string()))
        .unwrap_or_default();
    finish(response, host)
}

fn asset(host: &str, kind: &'static str, text: &'static str) -> Response<Body> {
    let response = Response::builder()
        .header(header::CONTENT_TYPE, kind)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(text))
        .unwrap_or_default();
    finish(response, host)
}

fn see_other(host: &str, to: &'static str) -> Response<Body> {
    let response = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, to)
        .body(Body::empty())
        .unwrap_or_default();
    finish(response, host)
}

/// Only GET (and HEAD) for a page or picture.
fn only_get(parts: &Parts, host: &str) -> Option<Response<Body>> {
    (parts.method != Method::GET && parts.method != Method::HEAD)
        .then(|| plain(StatusCode::METHOD_NOT_ALLOWED, host, "Not here."))
}

async fn root(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, _) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    if let Some(answer) = only_get(&parts, &gate.host) {
        return answer;
    }
    match gate.device {
        Some(_) => asset(&gate.host, "text/html; charset=utf-8", INDEX_HTML),
        None => see_other(&gate.host, "/login"),
    }
}

/// The public pages: the sign-in page, its script and the style sheet.
async fn public(shared: Arc<Shared>, request: Request, kind: &'static str, text: &'static str) -> Response<Body> {
    let (parts, _) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    only_get(&parts, &gate.host).unwrap_or_else(|| asset(&gate.host, kind, text))
}

async fn login_page(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    public(shared, request, "text/html; charset=utf-8", LOGIN_HTML).await
}

async fn login_js(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    public(shared, request, "text/javascript; charset=utf-8", LOGIN_JS).await
}

async fn style_css(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    public(shared, request, "text/css; charset=utf-8", STYLE_CSS).await
}

async fn app_js(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, _) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    if gate.device.is_none() {
        return plain(StatusCode::UNAUTHORIZED, &gate.host, "Sign in first.");
    }
    only_get(&parts, &gate.host).unwrap_or_else(|| asset(&gate.host, "text/javascript; charset=utf-8", APP_JS))
}

/// The design work's page and its two files, for a signed-in device only; any other name
/// under `/next/` is not here.
async fn next_page(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, _) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    let path = parts.uri.path();
    if gate.device.is_none() {
        return if path == "/next/" {
            see_other(&gate.host, "/login")
        } else {
            plain(StatusCode::UNAUTHORIZED, &gate.host, "Sign in first.")
        };
    }
    if let Some(answer) = only_get(&parts, &gate.host) {
        return answer;
    }
    match next_file(path) {
        Some((kind, body)) => asset(&gate.host, kind, body),
        None => plain(StatusCode::NOT_FOUND, &gate.host, "Not here."),
    }
}

async fn favicon(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, _) = request.into_parts();
    match gate(&shared, &parts) {
        Ok(gate) => finish(
            Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap_or_default(),
            &gate.host,
        ),
        Err(answer) => *answer,
    }
}

async fn not_found(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, _) = request.into_parts();
    match gate(&shared, &parts) {
        Ok(gate) => plain(StatusCode::NOT_FOUND, &gate.host, "Not here."),
        Err(answer) => *answer,
    }
}

#[derive(serde::Deserialize)]
struct LoginBody {
    password: String,
}

/// POST /api/login `{"password": "…"}`: a right password remembers the device and gives it its
/// token as a cookie; a wrong one is counted against the address.
async fn login(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    let host = gate.host.as_str();
    if parts.method != Method::POST {
        return plain(StatusCode::METHOD_NOT_ALLOWED, host, "Sign in with POST.");
    }
    if !origin_allowed(&parts, host) {
        return plain(StatusCode::FORBIDDEN, host, "Not from this page.");
    }
    let is_json = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return plain(StatusCode::UNSUPPORTED_MEDIA_TYPE, host, "Send JSON.");
    }
    let Some(hash) = shared.store.lock().ok().and_then(|store| store.saved.password.clone()) else {
        return json(
            StatusCode::SERVICE_UNAVAILABLE,
            host,
            serde_json::json!({"ok": false, "text": "No password is set: set one in Settings › Remote Control."}),
        );
    };
    let refuse_wait = |wait: Duration| {
        let seconds = wait.as_secs().max(1);
        let mut answer = json(
            StatusCode::TOO_MANY_REQUESTS,
            host,
            serde_json::json!({"ok": false, "text": format!("Too many wrong passwords. Try again in {seconds} s."), "wait": seconds}),
        );
        if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
            answer.headers_mut().insert(header::RETRY_AFTER, value);
        }
        answer
    };
    // An address that must wait isn't even looked at: no hashing for it.
    if let Some(wait) = shared.guard.lock().ok().and_then(|guard| guard.wait(gate.ip, Instant::now())) {
        return refuse_wait(wait);
    }
    let Ok(body) = axum::body::to_bytes(body, MOST_LOGIN_BYTES).await else {
        return plain(StatusCode::PAYLOAD_TOO_LARGE, host, "Too long.");
    };
    let Ok(LoginBody { password }) = serde_json::from_slice(&body) else {
        return plain(StatusCode::BAD_REQUEST, host, "Send {\"password\": \"…\"}.");
    };
    let agent = parts.headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or("");
    let name = auth::device_name(agent);
    // One password checked at a time, off the runtime's own thread.
    let Ok(Ok(permit)) = tokio::time::timeout(Duration::from_secs(5), shared.hashing.acquire()).await else {
        return plain(StatusCode::SERVICE_UNAVAILABLE, host, "Busy; try again in a moment.");
    };
    // Looked at again now it is this one's turn: several sent at once from one address all
    // passed the first look before any was counted.
    if let Some(wait) = shared.guard.lock().ok().and_then(|guard| guard.wait(gate.ip, Instant::now())) {
        return refuse_wait(wait);
    }
    let verified = hash.clone();
    let typed_chars = password.chars().count();
    let right = tokio::task::spawn_blocking(move || auth::password_is(&hash, &password)).await.unwrap_or(false);
    if !right {
        // Counted before the next sign-in may be checked, whatever runs the server's tasks.
        let wait = shared.guard.lock().ok().and_then(|mut guard| guard.wrong(gate.ip, Instant::now()));
        drop(permit);
        shared.backend.log(&format!("A wrong password from {}.", gate.ip));
        // Settings and the status line say when wrong passwords keep coming.
        shared.backend.pages_changed();
        return match wait {
            Some(wait) => {
                if wait >= Duration::from_secs(60) {
                    shared.backend.log(&format!("{} must wait {} s before trying again.", gate.ip, wait.as_secs()));
                }
                refuse_wait(wait)
            }
            None => json(
                StatusCode::UNAUTHORIZED,
                host,
                serde_json::json!({"ok": false, "text": "That password isn't right."}),
            ),
        };
    }
    if let Ok(mut guard) = shared.guard.lock() {
        guard.right(gate.ip);
    }
    drop(permit);
    let store = shared.store.clone();
    let (ip, named) = (gate.ip, name.clone());
    let remembered = tokio::task::spawn_blocking(move || {
        let mut store = store.lock().map_err(|_| "busy".to_owned())?;
        // Now the password is known, so is whether it is long enough (Settings says if not).
        store.note_password_length(&verified, typed_chars);
        store.remember(&named, ip, &verified)
    })
    .await
    .unwrap_or_else(|_| Err("Could not remember the device.".to_owned()));
    match remembered {
        Ok((token, id)) => {
            shared.backend.log(&format!("{name} ({id}) signed in from {}.", gate.ip));
            let mut answer = json(StatusCode::OK, host, serde_json::json!({"ok": true, "device": name}));
            let cookie = format!("{COOKIE}={token}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=31536000");
            if let Ok(value) = HeaderValue::from_str(&cookie) {
                answer.headers_mut().insert(header::SET_COOKIE, value);
            }
            answer
        }
        Err(problem) => json(StatusCode::SERVICE_UNAVAILABLE, host, serde_json::json!({"ok": false, "text": problem})),
    }
}

/// POST /api/logout: this device is forgotten, and its pages close.
async fn logout(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, _) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    let host = gate.host.as_str();
    if parts.method != Method::POST {
        return plain(StatusCode::METHOD_NOT_ALLOWED, host, "Sign out with POST.");
    }
    if !origin_allowed(&parts, host) {
        return plain(StatusCode::FORBIDDEN, host, "Not from this page.");
    }
    let Some((id, name)) = gate.device else {
        return plain(StatusCode::UNAUTHORIZED, host, "Not signed in.");
    };
    let store = shared.store.clone();
    let forgot = id.clone();
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(mut store) = store.lock() {
            store.forget(&forgot);
        }
    })
    .await;
    shared.sign_out(&id);
    shared.backend.log(&format!("{name} ({id}) signed out."));
    let mut answer = json(StatusCode::OK, host, serde_json::json!({"ok": true}));
    let cookie = format!("{COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        answer.headers_mut().insert(header::SET_COOKIE, value);
    }
    answer
}

/// GET /thumb/{slide}/{version|usual}/{look}.jpg: a slide drawn as the window draws it, as a
/// JPEG, kept for as long as its look stays the same.
async fn thumb(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (parts, _) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    let host = gate.host.as_str();
    if gate.device.is_none() {
        return plain(StatusCode::UNAUTHORIZED, host, "Sign in first.");
    }
    if let Some(answer) = only_get(&parts, host) {
        return answer;
    }
    let Some((slide, version, look)) = thumb_path(parts.uri.path()) else {
        return plain(StatusCode::NOT_FOUND, host, "Not here.");
    };
    // A version no theme has is the usual look: made-up versions can't make the app draw anew.
    let version = version.filter(|v| shared.versions.lock().is_ok_and(|known| known.contains(v)));
    let key = (slide, version);
    let kept = shared.thumbs.lock().ok().and_then(|mut thumbs| thumbs.get(&key, &look));
    if let Some(jpeg) = kept {
        return picture(&parts, host, &look, jpeg);
    }
    let Ok(Ok(permit)) = tokio::time::timeout(Duration::from_secs(10), shared.drawing.acquire()).await else {
        return plain(StatusCode::SERVICE_UNAVAILABLE, host, "Busy; try again in a moment.");
    };
    let drawn =
        match tokio::time::timeout(Duration::from_secs(10), shared.backend.thumb(slide, version, look.clone())).await {
            Ok(Ok(Thumbed::Drawn(drawn))) => drawn,
            Ok(Ok(Thumbed::Moved(current))) => {
                // The slide looks different now: on to the address of the picture it has, which
                // costs nothing to say (the app drew nothing for this one).
                let version = version.map_or_else(|| "usual".to_owned(), |v| v.to_string());
                let to = format!("/thumb/{slide}/{version}/{current}.jpg");
                let response = Response::builder()
                    .status(StatusCode::TEMPORARY_REDIRECT)
                    .header(header::LOCATION, to)
                    .header(header::CACHE_CONTROL, "no-store")
                    .body(Body::empty());
                return finish(response.unwrap_or_default(), host);
            }
            Ok(Ok(Thumbed::Missing)) => return plain(StatusCode::NOT_FOUND, host, "No such slide."),
            _ => return plain(StatusCode::SERVICE_UNAVAILABLE, host, "SoftPresenter did not answer in time."),
        };
    let read = drawn.read;
    let jpeg = tokio::task::spawn_blocking(move || read().and_then(|rgba| jpeg(&rgba, 82))).await.ok().flatten();
    drop(permit);
    let Some(jpeg) = jpeg.map(Bytes::from) else {
        return plain(StatusCode::SERVICE_UNAVAILABLE, host, "The slide couldn't be drawn.");
    };
    if let Ok(mut thumbs) = shared.thumbs.lock() {
        thumbs.put(key, look.clone(), jpeg.clone());
    }
    picture(&parts, host, &look, jpeg)
}

/// A slide picture's address: its slide, its version (`usual` for none) and its look.
fn thumb_path(path: &str) -> Option<(Uuid, Option<Uuid>, String)> {
    let mut parts = path.strip_prefix("/thumb/")?.split('/');
    let slide = Uuid::parse_str(parts.next()?).ok()?;
    let version = match parts.next()? {
        "usual" => None,
        id => Some(Uuid::parse_str(id).ok()?),
    };
    let look = parts.next()?.strip_suffix(".jpg")?;
    if parts.next().is_some() || look.is_empty() || look.len() > 32 || !look.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((slide, version, look.to_ascii_lowercase()))
}

/// A slide picture, kept by the browser for good: its address carries its look.
fn picture(parts: &Parts, host: &str, look: &str, jpeg: Bytes) -> Response<Body> {
    let tag = format!("\"{look}\"");
    let fresh = parts.headers.get(header::IF_NONE_MATCH).and_then(|value| value.to_str().ok()) == Some(tag.as_str());
    let builder = Response::builder()
        .header(header::ETAG, tag)
        .header(header::CACHE_CONTROL, "private, max-age=31536000, immutable");
    let response = if fresh {
        builder.status(StatusCode::NOT_MODIFIED).body(Body::empty())
    } else {
        builder.header(header::CONTENT_TYPE, "image/jpeg").body(Body::from(jpeg))
    };
    finish(response.unwrap_or_default(), host)
}

/// RGBA as a JPEG (the alpha is let go: pictures are drawn on black).
pub fn jpeg(rgba: &Rgba, quality: u8) -> Option<Vec<u8>> {
    let pixels = (rgba.width as usize).checked_mul(rgba.height as usize)?;
    if rgba.pixels.len() < pixels * 4 || pixels == 0 {
        return None;
    }
    let rgb: Vec<u8> =
        rgba.pixels.as_chunks::<4>().0.iter().take(pixels).flat_map(|px| [px[0], px[1], px[2]]).collect();
    let mut out = Vec::with_capacity(pixels / 4);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode(&rgb, rgba.width, rgba.height, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(out)
}

/// Reads the live preview back and turns it into a JPEG, at most every [`PREVIEW_EVERY`] and
/// only while a page watches it; pages are sent only the newest.
async fn preview_feed(shared: Arc<Shared>) {
    let mut drawn = shared.preview_drawn.subscribe();
    let mut last = Instant::now() - PREVIEW_EVERY;
    loop {
        tokio::select! {
            changed = drawn.changed() => if changed.is_err() { return },
            _ = shared.preview_wanted.notified() => {}
        }
        if shared.preview_watchers.load(Ordering::Acquire) == 0 {
            continue;
        }
        let wait = PREVIEW_EVERY.saturating_sub(last.elapsed());
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        last = Instant::now();
        let Some(read) = shared.preview_read.lock().ok().and_then(|read| read.clone()) else { continue };
        let frame = tokio::task::spawn_blocking(move || read().and_then(|rgba| jpeg(&rgba, 72))).await.ok().flatten();
        if let Some(frame) = frame {
            let mut tagged = Vec::with_capacity(frame.len() + 1);
            tagged.push(1);
            tagged.extend_from_slice(&frame);
            shared.preview_frame.send_replace(Some(Bytes::from(tagged)));
        }
    }
}

/// GET /api/ws: the page's one WebSocket.
async fn socket(State(shared): State<Arc<Shared>>, request: Request) -> Response<Body> {
    let (mut parts, _) = request.into_parts();
    let gate = match gate(&shared, &parts) {
        Ok(gate) => gate,
        Err(answer) => return *answer,
    };
    let host = gate.host.clone();
    let Some((device, name)) = gate.device else {
        return plain(StatusCode::UNAUTHORIZED, &host, "Sign in first.");
    };
    if !origin_allowed(&parts, &host) {
        return plain(StatusCode::FORBIDDEN, &host, "Not from this page.");
    }
    if let Some(full) = full(&shared, &device) {
        return plain(StatusCode::SERVICE_UNAVAILABLE, &host, full);
    }
    let may_connect = shared.connects.lock().is_ok_and(|mut connects| {
        if connects.len() > 4 * auth::MOST_DEVICES {
            connects.clear();
        }
        connects.entry(device.clone()).or_insert_with(|| Bucket::new(CONNECTS_A_MINUTE / 60.0, 10.0)).take()
    });
    if !may_connect {
        return plain(StatusCode::TOO_MANY_REQUESTS, &host, "This device has connected too often; wait a minute.");
    }
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(upgrade) => upgrade,
        Err(rejection) => return finish(rejection.into_response(), &host),
    };
    let caller = Caller {
        client: shared.next_page.fetch_add(1, Ordering::AcqRel),
        device,
        name,
        ip: gate.ip,
        role: Role::Control,
    };
    let response = upgrade
        .max_message_size(MOST_MESSAGE_BYTES)
        .max_frame_size(MOST_MESSAGE_BYTES)
        .write_buffer_size(64 * 1024)
        .max_write_buffer_size(4 * 1024 * 1024)
        .on_upgrade(move |socket| page(shared, socket, caller));
    finish(response, &host)
}

/// Why another page can't connect now, if it can't.
fn full(shared: &Shared, device: &str) -> Option<&'static str> {
    let pages = shared.pages.lock().ok()?;
    if pages.len() >= MOST_PAGES {
        return Some("Too many pages are connected; close one first.");
    }
    if pages.iter().filter(|c| c.page.device == device).count() >= MOST_PAGES_A_DEVICE {
        return Some("This device has too many pages open; close one first.");
    }
    None
}

/// A page's place among those connected, given back when it goes.
struct Registered {
    shared: Arc<Shared>,
    id: u64,
    watching: bool,
}

impl Registered {
    fn watch(&mut self, on: bool) {
        if on == self.watching {
            return;
        }
        self.watching = on;
        if on {
            if self.shared.preview_watchers.fetch_add(1, Ordering::AcqRel) == 0 {
                self.shared.preview_wanted.notify_one();
            }
        } else {
            self.shared.preview_watchers.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.watch(false);
        if let Ok(mut pages) = self.shared.pages.lock() {
            pages.retain(|c| c.page.id != self.id);
            self.shared.page_count.store(pages.len(), Ordering::Release);
        }
        self.shared.backend.pages_changed();
    }
}

fn register(shared: &Arc<Shared>, caller: &Caller, kick: Arc<Kick>) -> Option<Registered> {
    if full(shared, &caller.device).is_some() {
        return None;
    }
    let mut pages = shared.pages.lock().ok()?;
    pages.push(Connected {
        page: Page {
            id: caller.client,
            device: caller.device.clone(),
            name: caller.name.clone(),
            ip: caller.ip,
            since: SystemTime::now(),
        },
        kick,
    });
    shared.page_count.store(pages.len(), Ordering::Release);
    drop(pages);
    let registered = Registered { shared: shared.clone(), id: caller.client, watching: false };
    // Listed first, then looked at: a device forgotten from here on is closed by the forget's
    // own sign-out; one forgotten since its connection was let in is caught now.
    if !shared.remembered(&caller.device) {
        return None;
    }
    shared.backend.pages_changed();
    Some(registered)
}

/// Milliseconds since 1970, the clock every time in the state is on.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// How many of something may still be done now: `rate` a second, at most `most` at once.
struct Bucket {
    tokens: f64,
    at: Instant,
    rate: f64,
    most: f64,
}

impl Bucket {
    fn new(rate: f64, most: f64) -> Bucket {
        Bucket { tokens: most, at: Instant::now(), rate, most }
    }

    fn take(&mut self) -> bool {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.at).as_secs_f64() * self.rate).min(self.most);
        self.at = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// A reply to a command: the page's id for it, and the reply exactly as `sp` gets it.
pub fn reply_message(id: &serde_json::Value, reply: &Reply) -> String {
    format!(
        "{{\"type\":\"reply\",\"id\":{},\"reply\":{}}}",
        serde_json::to_string(id).unwrap_or_else(|_| "null".to_owned()),
        serde_json::to_string(reply).unwrap_or_else(|_| "{}".to_owned())
    )
}

fn snapshot_message(hub: &Hub, caller: &Caller) -> String {
    let you = serde_json::json!({"client": caller.client, "device": caller.name, "role": caller.role.word()});
    format!(
        "{{\"type\":\"snapshot\",\"epoch\":{},\"version\":{},\"now\":{},\"you\":{},\"parts\":{}}}",
        serde_json::to_string(hub.epoch()).unwrap_or_default(),
        hub.version(),
        now_ms(),
        you,
        hub::parts_object(&hub.all())
    )
}

fn change_message(version: u64, parts: &[(String, Arc<str>)]) -> String {
    format!("{{\"type\":\"change\",\"version\":{version},\"now\":{},\"parts\":{}}}", now_ms(), hub::parts_object(parts))
}

fn error_message(text: &str) -> String {
    serde_json::json!({"type": "error", "text": text}).to_string()
}

async fn say(socket: &mut WebSocket, message: Message) -> bool {
    matches!(tokio::time::timeout(WRITE_WITHIN, socket.send(message)).await, Ok(Ok(())))
}

async fn close_with(socket: &mut WebSocket, code: u16, reason: &str) {
    let frame = CloseFrame { code, reason: reason.into() };
    let _ = tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Close(Some(frame)))).await;
}

/// What a page has said it wants, between its messages.
struct PageState {
    epoch: String,
    version: Option<u64>,
    bucket: Bucket,
    waiting: Arc<Semaphore>,
}

/// One page's connection: its messages in, the state's changes, replies and preview pictures
/// out, and a heartbeat.
async fn page(shared: Arc<Shared>, mut socket: WebSocket, caller: Caller) {
    let kick = Arc::new(Kick::default());
    let Some(mut registered) = register(&shared, &caller, kick.clone()) else {
        let (code, reason) = if shared.remembered(&caller.device) {
            (close::FULL, "Too many pages are connected.")
        } else {
            (close::SIGNED_OUT, "Signed out.")
        };
        close_with(&mut socket, code, reason).await;
        return;
    };
    let mut changes = shared.hub_tx.subscribe();
    let mut frames = shared.preview_frame.subscribe();
    let mut closing = shared.closing.subscribe();
    let (replies_tx, mut replies) = mpsc::channel::<String>(32);
    let mut state = PageState {
        epoch: String::new(),
        version: None,
        bucket: Bucket::new(COMMANDS_A_SECOND, COMMANDS_A_SECOND * 2.0),
        waiting: Arc::new(Semaphore::new(MOST_WAITING_COMMANDS)),
    };
    let started = Instant::now();
    let mut heard = Instant::now();
    let mut beat = tokio::time::interval(BEAT_EVERY);
    beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    beat.tick().await;
    let hello_check = tokio::time::sleep(HELLO_WITHIN);
    tokio::pin!(hello_check);
    loop {
        tokio::select! {
            _ = kick.notify.notified() => {
                let code = kick.code.load(Ordering::Acquire);
                let reason = if code == close::SIGNED_OUT { "Signed out." } else { "Disconnected at the computer." };
                close_with(&mut socket, code, reason).await;
                break;
            }
            _ = closing.changed() => {
                close_with(&mut socket, close::SWITCHED_OFF, "Remote Control was switched off.").await;
                break;
            }
            message = socket.recv() => {
                let Some(Ok(message)) = message else { break };
                heard = Instant::now();
                match message {
                    Message::Text(text) => {
                        let watched = registered.watching;
                        let out = take_message(&shared, &caller, &mut state, &mut registered, &replies_tx, text.as_str()).await;
                        if let Some(out) = out
                            && !say(&mut socket, Message::text(out)).await
                        {
                            break;
                        }
                        // A page that starts watching is sent the picture there is now (another
                        // page may have been watching a still slide, so none new is coming).
                        if !watched && registered.watching {
                            frames.mark_changed();
                        }
                    }
                    Message::Binary(_) => {
                        // Counted as any message is; past the limit, not even answered.
                        if state.bucket.take()
                            && !say(&mut socket, Message::text(error_message("Send messages as text (JSON)."))).await
                        {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            Some(reply) = replies.recv() => {
                if !say(&mut socket, Message::text(reply)).await {
                    break;
                }
            }
            changed = changes.changed(), if state.version.is_some() => {
                if changed.is_err() {
                    break;
                }
                if !shared.remembered(&caller.device) {
                    close_with(&mut socket, close::SIGNED_OUT, "Signed out.").await;
                    break;
                }
                let out = {
                    let Ok(hub) = shared.hub.lock() else { break };
                    let had = state.version.unwrap_or(0);
                    let out = match hub.since(&state.epoch, had) {
                        Some(parts) if parts.is_empty() => None,
                        Some(parts) => Some(change_message(hub.version(), &parts)),
                        None => Some(snapshot_message(&hub, &caller)),
                    };
                    state.version = Some(hub.version());
                    state.epoch = hub.epoch().to_owned();
                    out
                };
                if let Some(out) = out
                    && !say(&mut socket, Message::text(out)).await
                {
                    break;
                }
            }
            changed = frames.changed(), if registered.watching => {
                if changed.is_err() {
                    break;
                }
                if !shared.remembered(&caller.device) {
                    close_with(&mut socket, close::SIGNED_OUT, "Signed out.").await;
                    break;
                }
                let frame = frames.borrow_and_update().clone();
                if let Some(frame) = frame
                    && !say(&mut socket, Message::Binary(frame)).await
                {
                    break;
                }
            }
            _ = &mut hello_check, if state.version.is_none() => {
                close_with(&mut socket, close::TOO_SLOW, "Say hello first.").await;
                break;
            }
            _ = beat.tick() => {
                if heard.elapsed() > SILENT_FOR {
                    close_with(&mut socket, close::TOO_SLOW, "Nothing heard for too long.").await;
                    break;
                }
                let beat = format!("{{\"type\":\"beat\",\"now\":{}}}", now_ms());
                if !say(&mut socket, Message::Ping(Bytes::new())).await || !say(&mut socket, Message::text(beat)).await {
                    break;
                }
            }
        }
    }
    let _ = started;
}

/// Acts on one message from a page; what to send it back at once, if anything.
async fn take_message(
    shared: &Arc<Shared>,
    caller: &Caller,
    state: &mut PageState,
    registered: &mut Registered,
    replies: &mpsc::Sender<String>,
    text: &str,
) -> Option<String> {
    // Every message counts, whatever it is, before anything else is done for it: a page sending
    // too many gets a refusal only for a command (with its id); anything else is not answered,
    // and nothing is locked.
    let allowed = state.bucket.take();
    let Ok(message) = serde_json::from_str::<serde_json::Value>(text) else {
        return allowed.then(|| error_message("That isn't JSON."));
    };
    let kind = message.get("type").and_then(serde_json::Value::as_str);
    if !allowed {
        return match (kind, message.get("id")) {
            (Some("command"), Some(id)) if id.is_number() || id.as_str().is_some_and(|id| id.len() <= 64) => {
                Some(reply_message(id, &Reply::failed("Too many commands at once; slow down.")))
            }
            _ => None,
        };
    }
    match kind {
        Some("hello") => {
            // A device forgotten since it connected is told so, and closed.
            if !shared.remembered(&caller.device) {
                shared.sign_out(&caller.device);
                return Some(error_message("This device has been signed out."));
            }
            if let Some(on) = message.get("preview").and_then(serde_json::Value::as_bool) {
                registered.watch(on);
            }
            // The app publishes the state as it is now, so the first snapshot is fresh; a hello
            // after that is answered from what was published (it is never more than a moment old).
            if state.version.is_none() {
                let _ = tokio::time::timeout(Duration::from_secs(2), shared.backend.refresh()).await;
            }
            let resume = message
                .get("resume")
                .and_then(|resume| Some((resume.get("epoch")?.as_str()?.to_owned(), resume.get("version")?.as_u64()?)));
            let hub = shared.hub.lock().ok()?;
            let out = match resume.and_then(|(epoch, version)| hub.since(&epoch, version)) {
                Some(parts) => change_message(hub.version(), &parts),
                None => snapshot_message(&hub, caller),
            };
            state.version = Some(hub.version());
            state.epoch = hub.epoch().to_owned();
            Some(out)
        }
        Some("preview") => {
            registered.watch(message.get("on").and_then(serde_json::Value::as_bool).unwrap_or(false));
            None
        }
        Some("command") => {
            let id = message.get("id").cloned().unwrap_or(serde_json::Value::Null);
            let id_ok = match &id {
                serde_json::Value::Number(_) => true,
                serde_json::Value::String(id) => id.len() <= 64,
                _ => false,
            };
            if !id_ok {
                return Some(error_message("A command needs an id: a number, or a short word."));
            }
            let args: Option<Vec<String>> =
                message.get("args").and_then(serde_json::Value::as_array).and_then(|args| {
                    (args.len() <= 64).then(|| args.iter().map(|arg| arg.as_str().map(str::to_owned)).collect())?
                });
            let Some(args) = args else {
                return Some(reply_message(&id, &Reply::failed("A command's args are a list of words.")));
            };
            // The device is still remembered (it is closed as soon as it is forgotten; this is
            // the check at the moment it counts).
            if !shared.remembered(&caller.device) {
                shared.sign_out(&caller.device);
                return Some(reply_message(&id, &Reply::failed("This device has been signed out.")));
            }
            if let Some(refused) = policy::refusal(caller.role, &args) {
                return Some(reply_message(&id, &Reply::failed(refused)));
            }
            // Every page's commands together are counted too: the app carries each out.
            if !shared.commands.lock().is_ok_and(|mut all| all.take()) {
                return Some(reply_message(
                    &id,
                    &Reply::failed("Remote Control is busy with other pages' commands; try again in a moment."),
                ));
            }
            let Ok(permit) = state.waiting.clone().try_acquire_owned() else {
                return Some(reply_message(&id, &Reply::failed("Still busy with the commands before; try again.")));
            };
            let answer = shared.backend.command(caller.clone(), args);
            let replies = replies.clone();
            tokio::spawn(async move {
                let reply = match tokio::time::timeout(ANSWER_WITHIN, answer).await {
                    Ok(Ok(reply)) => reply,
                    _ => Reply::failed("SoftPresenter did not answer in time."),
                };
                let _ = replies.send(reply_message(&id, &reply)).await;
                drop(permit);
            });
            None
        }
        _ => Some(error_message("Unknown message: send hello, command or preview.")),
    }
}

#[cfg(test)]
mod tests;
