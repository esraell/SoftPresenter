//! The receiver: the control channel's listeners and requests, the one session, and the video
//! stream.
//!
//! The rules it keeps:
//! - One sender at a time. A second is told the receiver is busy and never replaces the first.
//! - With "ask" on, the answer to a sender's first `SETUP` is held until the operator allows
//!   it, so nothing flows before then; no answer in [`ASK_TIME`] is a refusal.
//! - The video port takes one connection, from the address of the sender that was allowed, and
//!   only once that sender has asked for the stream.
//! - A few connections at once, a few a minute from one address, and a time limit on every
//!   request; a flood costs this feature, never the service.
//!
//! Follows UxPlay's `lib/raop.c`, `lib/raop_handlers.h`, `lib/raop_rtp_mirror.c` and
//! `lib/raop_ntp.c` (copyright (C) 2011-2012 Juho Vähä-Herttua, (C) 2019 dsafa22, modified by
//! Florian Draschbacher, (C) 2021-2026 F. Duncanh; LGPL-2.1-or-later).

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::decode::{Decoder, Pictures};
use crate::events::{clean, log, say};
use crate::fairplay::{self, FairPlay};
use crate::keys::{Identity, Pairing, VideoCipher};
use crate::request::{Bad, Request, Response, read_request};
use crate::{discovery, plists, stream};

/// How long the operator has to answer before a sender is refused.
pub const ASK_TIME: Duration = Duration::from_secs(30);
/// How long a connection with no session may say nothing.
const IDLE_TIME: Duration = Duration::from_secs(30);
/// How long the sender of the session may go without a request (it sends one about every two
/// seconds) before it is taken to have gone.
const HEARTBEAT_TIME: Duration = Duration::from_secs(20);
/// How many control connections are served at once.
const MAX_CONNECTIONS: usize = 8;
/// How many of them may come from one address, so that one device cannot take them all.
const MAX_CONNECTIONS_EACH: usize = 3;
/// How many connections one address may open in a minute.
const MAX_PER_MINUTE: usize = 30;
/// How long an address the operator refused (Don't Allow) is not asked about again, so that
/// nobody can keep a question in front of the operator. An address that was only not answered
/// in time may try again at once.
const REFUSED_TIME: Duration = Duration::from_secs(60);
/// How many times in ten seconds a sender's picture may change size (each is a new FFmpeg).
const MAX_SIZE_CHANGES: usize = 8;
/// The size and rate asked of a sender.
const ASK_FPS: u32 = 30;

/// How the program was started.
pub struct Config {
    pub name: String,
    pub device_id: String,
    pub ask: bool,
    pub ffmpeg: PathBuf,
    pub model: String,
    pub features_2: u32,
}

struct Session {
    id: u32,
    name: String,
    peer: IpAddr,
    /// The operator allowed it (or nobody is asked).
    allowed: bool,
    /// While the operator is being asked: where the answer goes.
    answer: Option<mpsc::Sender<bool>>,
    /// The video stream's cipher, from when the sender asks for the stream until it connects.
    video: Option<VideoCipher>,
    control: Option<TcpStream>,
    video_stream: Option<TcpStream>,
    ended: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    session: Option<Session>,
    /// When each address lately opened a connection.
    recent: Vec<(IpAddr, Instant)>,
    /// The addresses of the control connections being served.
    serving: Vec<IpAddr>,
    /// The addresses refused lately, when, and whether the operator has been told that one
    /// tried again.
    refused: Vec<(IpAddr, Instant, bool)>,
}

pub struct Shared {
    config: Config,
    identity: Identity,
    public_key: [u8; 32],
    pub pictures: Arc<Pictures>,
    state: Mutex<State>,
    next_id: AtomicU32,
    pub control_port: u16,
    pub video_port: u16,
}

/// The listeners, bound and not yet served.
pub struct Listeners {
    control: Vec<TcpListener>,
    video: Vec<TcpListener>,
}

fn listen(address: IpAddr, port: u16) -> std::io::Result<TcpListener> {
    let socket = socket2::Socket::new(
        if address.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 },
        socket2::Type::STREAM,
        None,
    )?;
    if address.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::new(address, port).into())?;
    socket.listen(16)?;
    Ok(socket.into())
}

/// Binds the control and video ports: on 127.0.0.1 only, or on every interface (IPv4 and IPv6,
/// which senders often use). The ports asked for, or the next free ones.
pub fn bind(open: bool, port: u16) -> std::io::Result<(Listeners, u16, u16)> {
    let addresses: &[IpAddr] = if open {
        &[IpAddr::V4(Ipv4Addr::UNSPECIFIED), IpAddr::V6(Ipv6Addr::UNSPECIFIED)]
    } else {
        &[IpAddr::V4(Ipv4Addr::LOCALHOST)]
    };
    let attempt = |control: u16, video: u16| -> std::io::Result<(Listeners, u16, u16)> {
        let first_control = listen(addresses[0], control)?;
        let control = first_control.local_addr()?.port();
        let first_video = listen(addresses[0], video)?;
        let video = first_video.local_addr()?.port();
        let mut listeners = Listeners { control: vec![first_control], video: vec![first_video] };
        for address in &addresses[1..] {
            // IPv6 may be switched off on a computer; the receiver then works over IPv4 alone.
            match (listen(*address, control), listen(*address, video)) {
                (Ok(c), Ok(v)) => {
                    listeners.control.push(c);
                    listeners.video.push(v);
                }
                (Err(error), _) | (_, Err(error)) if error.kind() == std::io::ErrorKind::AddrInUse => {
                    return Err(error);
                }
                _ => log("IPv6 is not available on this computer; listening on IPv4 only"),
            }
        }
        Ok((listeners, control, video))
    };
    let mut last = None;
    if port != 0 {
        for step in 0..5_u16 {
            let Some(control) = port.checked_add(step * 10) else { break };
            let Some(video) = control.checked_add(1) else { break };
            match attempt(control, video) {
                Ok(bound) => return Ok(bound),
                Err(error) => last = Some(error),
            }
        }
    }
    for _ in 0..5 {
        match attempt(0, 0) {
            Ok(bound) => return Ok(bound),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| std::io::ErrorKind::AddrInUse.into()))
}

impl Shared {
    pub fn new(config: Config, identity: Identity, control_port: u16, video_port: u16) -> Arc<Shared> {
        let public_key = identity.public();
        Arc::new(Shared {
            config,
            identity,
            public_key,
            pictures: Arc::new(Pictures::default()),
            state: Mutex::new(State::default()),
            next_id: AtomicU32::new(1),
            control_port,
            video_port,
        })
    }

    pub fn record(&self) -> discovery::Record<'_> {
        discovery::Record {
            name: &self.config.name,
            device_id: &self.config.device_id,
            public_key: self.public_key(),
            model: &self.config.model,
            features_2: self.config.features_2,
        }
    }

    fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// Whether the operator is asked about each sender.
    pub fn asks(&self) -> bool {
        self.config.ask
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// The operator's answer to a sender that asked.
    pub fn answer(&self, id: u32, allow: bool) {
        if let Ok(state) = self.state.lock()
            && let Some(session) = state.session.as_ref().filter(|session| session.id == id)
            && let Some(answer) = &session.answer
        {
            let _ = answer.send(allow);
        }
    }

    /// Ends the session now connected or asking, as the operator's Stop does.
    pub fn stop(&self) {
        let id = self.state.lock().ok().and_then(|state| state.session.as_ref().map(|session| session.id));
        if let Some(id) = id {
            self.end(id, "stopped here");
        }
    }

    /// Ends a session, if it is still the one: its connections close, its threads stop, and
    /// SoftPresenter is told.
    fn end(&self, id: u32, why: &str) {
        self.close(id, why, true);
    }

    /// As [`Shared::end`]; with `hang_up` false the control connection is left open, for the
    /// answer still to be written on it.
    fn close(&self, id: u32, why: &str, hang_up: bool) {
        let session = {
            let Ok(mut state) = self.state.lock() else { return };
            if state.session.as_ref().is_some_and(|session| session.id == id) { state.session.take() } else { None }
        };
        let Some(session) = session else { return };
        session.ended.store(true, Ordering::Relaxed);
        let control = if hang_up { &session.control } else { &None };
        for stream in [control, &session.video_stream].into_iter().flatten() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if session.allowed {
            log(format!("session {id} ended: {why}"));
            say(json!({ "event": "stopped", "id": id, "name": session.name, "why": why }));
        } else {
            log(format!("session {id} was refused: {why}"));
            say(json!({ "event": "refused", "id": id, "name": session.name, "why": why }));
        }
    }

    /// True when this address may open another connection now.
    fn admit(&self, peer: IpAddr) -> bool {
        let Ok(mut state) = self.state.lock() else { return false };
        let now = Instant::now();
        state.recent.retain(|(_, at)| now.duration_since(*at) < Duration::from_secs(60));
        if state.recent.iter().filter(|(address, _)| *address == peer).count() >= MAX_PER_MINUTE {
            return false;
        }
        // The list itself is bounded: a flood from many addresses is refused too.
        if state.recent.len() >= MAX_PER_MINUTE * 16 {
            return false;
        }
        state.recent.push((peer, now));
        true
    }

    /// Takes a place among the control connections served, if there is one for this address.
    fn take_place(&self, peer: IpAddr) -> bool {
        let Ok(mut state) = self.state.lock() else { return false };
        if state.serving.len() >= MAX_CONNECTIONS
            || state.serving.iter().filter(|address| **address == peer).count() >= MAX_CONNECTIONS_EACH
        {
            return false;
        }
        state.serving.push(peer);
        true
    }

    fn give_place(&self, peer: IpAddr) {
        if let Ok(mut state) = self.state.lock()
            && let Some(at) = state.serving.iter().position(|address| *address == peer)
        {
            state.serving.swap_remove(at);
        }
    }

    /// Remembers that this address was refused, for [`REFUSED_TIME`].
    fn remember_refused(&self, peer: IpAddr) {
        if let Ok(mut state) = self.state.lock() {
            let now = Instant::now();
            state.refused.retain(|(_, at, _)| now.duration_since(*at) < REFUSED_TIME);
            if state.refused.len() < 256 {
                state.refused.push((peer, now, false));
            }
        }
    }

    /// Whether this address was refused less than [`REFUSED_TIME`] ago, and if so whether this
    /// is the first time it has tried again (the operator is told of that once).
    fn refused_lately(&self, peer: IpAddr) -> Option<bool> {
        let mut state = self.state.lock().ok()?;
        let entry =
            state.refused.iter_mut().find(|(address, at, _)| *address == peer && at.elapsed() < REFUSED_TIME)?;
        Some(!std::mem::replace(&mut entry.2, true))
    }
}

/// Serves the listeners on threads of their own, for as long as the program runs.
pub fn serve(shared: &Arc<Shared>, listeners: Listeners) {
    for listener in listeners.control {
        let shared = shared.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let Ok(peer) = stream.peer_addr() else { continue };
                if !shared.admit(peer.ip()) || !shared.take_place(peer.ip()) {
                    // Too many: closed without a word.
                    continue;
                }
                let place = Place { shared: shared.clone(), peer: peer.ip() };
                let shared = shared.clone();
                std::thread::spawn(move || control_connection(&shared, stream, peer, place));
            }
        });
    }
    for listener in listeners.video {
        let shared = shared.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let Ok(peer) = stream.peer_addr() else { continue };
                if !shared.admit(peer.ip()) {
                    continue;
                }
                let shared = shared.clone();
                // Only the expected connection gets past the first lines of this; one at a time.
                std::thread::spawn(move || video_connection(&shared, stream, peer));
            }
        });
    }
}

/// A control connection's place among those served ([`Shared::take_place`]), given back when
/// it is dropped: before the sender can know the connection is over (before an answer that
/// closes it is written, and before it is closed), so that a sender may connect again at once,
/// and also when the connection's thread panics. Given back after, a sender quick to connect
/// again found its address still holding every place, and was closed without a word. A
/// session ended from elsewhere (the operator's Stop, its video stream failing) is shut by
/// that thread ([`Shared::end`]), and its place is given back a moment later, when the
/// connection's own thread finds it shut.
struct Place {
    shared: Arc<Shared>,
    peer: IpAddr,
}

impl Drop for Place {
    fn drop(&mut self) {
        self.shared.give_place(self.peer);
    }
}

/// What one control connection knows.
struct Connection {
    fairplay: FairPlay,
    pairing: Pairing,
    /// The session this connection began, its key and its timing port.
    session: Option<(u32, [u8; 16], u16)>,
}

fn control_connection(shared: &Arc<Shared>, stream: TcpStream, peer: SocketAddr, place: Place) {
    let mut place = Some(place);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_nodelay(true);
    log(format!("a connection from {}", peer.ip()));
    let mut connection = Connection { fairplay: FairPlay::default(), pairing: Pairing::default(), session: None };
    let mut pending = Vec::new();
    let why = loop {
        let quiet = if connection.session.is_some() { HEARTBEAT_TIME } else { IDLE_TIME };
        let request = match read_request(&mut &stream, &mut pending, quiet) {
            Ok(request) => request,
            Err(Bad::Closed) => break "the sender closed the connection".to_owned(),
            Err(Bad::Quiet) => break "nothing came from the sender for too long".to_owned(),
            Err(Bad::Refused(reason)) => {
                log(format!("a request from {} was refused: {reason}", peer.ip()));
                break "a request was not understood".to_owned();
            }
            Err(Bad::Io(kind)) => break format!("the connection failed ({kind:?})"),
        };
        if request.url != "/feedback" {
            log(format!("{} {} ({} bytes)", request.method, clean(&request.url, "?"), request.body.len()));
        }
        let mut response = handle(shared, &stream, peer, &mut connection, &request);
        if request.cseq.is_some() && request.method != "RECORD" {
            response = response.header("Audio-Jack-Status", "connected; type=digital");
        }
        if response.code() != 200 {
            log(format!("answered {}", response.code()));
        }
        if response.close {
            drop(place.take());
        }
        if (&stream).write_all(&response.to_bytes(&request.protocol, request.cseq)).is_err() {
            break "the connection failed while answering".to_owned();
        }
        if response.close {
            break "the session was closed".to_owned();
        }
    };
    drop(place);
    let _ = stream.shutdown(Shutdown::Both);
    if let Some((id, ..)) = connection.session {
        shared.end(id, &why);
    }
}

fn bad_request() -> Response {
    Response::status(400, "Bad Request")
}

fn octets(bytes: &[u8]) -> Response {
    Response::with_body("application/octet-stream", bytes.to_vec())
}

fn handle(
    shared: &Arc<Shared>,
    stream: &TcpStream,
    peer: SocketAddr,
    connection: &mut Connection,
    request: &Request,
) -> Response {
    if request.protocol != "RTSP/1.0" {
        return Response::status(501, "Not Implemented");
    }
    match (request.method.as_str(), request.url.as_str()) {
        ("GET", url) if url == "/info" || url.starts_with("/info?") => {
            let asks_txt = request.content_type.is_some()
                && plists::parse(&request.body).is_some_and(|body| plists::asks_for_txt(&body));
            let answer = if asks_txt {
                plists::info_txt(&shared.record().airplay_txt_bytes())
            } else {
                plists::info(&plists::Info {
                    name: &shared.config.name,
                    device_id: &shared.config.device_id,
                    public_key: shared.public_key(),
                    features: discovery::features(shared.config.features_2),
                    width: crate::decode::MAX_WIDTH,
                    height: crate::decode::MAX_HEIGHT,
                    fps: ASK_FPS,
                })
            };
            Response::with_body("application/x-apple-binary-plist", plists::to_bytes(&answer))
        }
        ("POST", "/pair-setup") => {
            if request.body.len() != 32 {
                return bad_request();
            }
            octets(shared.public_key())
        }
        ("POST", "/pair-verify") => pair_verify(shared, connection, &request.body),
        ("POST", "/fp-setup") => {
            if let Ok(message) = <&[u8; fairplay::SETUP_LEN]>::try_from(request.body.as_slice()) {
                match connection.fairplay.setup(message) {
                    Ok(answer) => {
                        log(format!("FairPlay: first message, mode {}", message[14]));
                        octets(&answer)
                    }
                    Err(_) => Response::status(501, "Not Implemented"),
                }
            } else if let Ok(message) = <&[u8; fairplay::HANDSHAKE_LEN]>::try_from(request.body.as_slice()) {
                match connection.fairplay.handshake(message) {
                    Ok(answer) => {
                        log("FairPlay: second message");
                        octets(&answer)
                    }
                    Err(_) => Response::status(501, "Not Implemented"),
                }
            } else {
                bad_request()
            }
        }
        ("SETUP", _) => setup(shared, stream, peer, connection, &request.body),
        ("RECORD", _) => {
            Response::ok().header("Audio-Latency", "0").header("Audio-Jack-Status", "connected; type=analog")
        }
        ("GET_PARAMETER", _) => {
            if request.content_type.is_none() {
                return Response::status(451, "Parameter not understood");
            }
            if request.content_type.as_deref() == Some("text/parameters") && request.body.starts_with(b"volume\r\n") {
                return Response::with_body("text/parameters", b"volume: 0.000000\r\n".to_vec());
            }
            Response::ok()
        }
        ("SET_PARAMETER", _) => {
            if request.content_type.is_none() {
                return Response::status(451, "Parameter not understood");
            }
            Response::ok()
        }
        ("OPTIONS", _) => {
            Response::ok().header("Public", "SETUP, RECORD, FLUSH, TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER")
        }
        ("POST", "/feedback" | "/audioMode") | ("FLUSH", _) => Response::ok(),
        ("TEARDOWN", _) => {
            let (sound, video) = plists::teardown(&request.body);
            let response = Response::ok().header("Connection", "close");
            if sound && !video {
                // Only the sound stream stops; nothing here plays it yet.
                return response;
            }
            // The video stream, or (as iOS 27 sends it) everything: the session is over.
            if let Some((id, ..)) = connection.session.take() {
                shared.close(id, "the sender stopped", false);
            }
            response.closing()
        }
        _ => Response::status(501, "Not Implemented"),
    }
}

fn pair_verify(shared: &Arc<Shared>, connection: &mut Connection, body: &[u8]) -> Response {
    if body.len() != 68 {
        return bad_request();
    }
    match body[0] {
        1 => {
            let (Ok(their_key), Ok(their_identity)) =
                (<&[u8; 32]>::try_from(&body[4..36]), <&[u8; 32]>::try_from(&body[36..68]))
            else {
                return bad_request();
            };
            match connection.pairing.start(&shared.identity, their_key, their_identity) {
                Some(answer) => {
                    log("pairing: keys exchanged");
                    octets(&answer)
                }
                None => bad_request(),
            }
        }
        0 => {
            let Ok(signature) = <&[u8; 64]>::try_from(&body[4..68]) else { return bad_request() };
            if connection.pairing.finish(signature) {
                log("pairing: the sender's signature checked");
                Response::with_body("application/octet-stream", Vec::new())
            } else {
                log("pairing: the sender's signature was wrong");
                bad_request().closing()
            }
        }
        _ => bad_request(),
    }
}

fn setup(
    shared: &Arc<Shared>,
    stream: &TcpStream,
    peer: SocketAddr,
    connection: &mut Connection,
    body: &[u8],
) -> Response {
    let Some(setup) = plists::parse(body).as_ref().and_then(plists::setup) else { return bad_request() };
    if setup.bad_keys {
        return bad_request();
    }
    if let Some(asked) = &setup.session {
        if connection.session.is_some() {
            return bad_request();
        }
        let Ok(key) = connection.fairplay.decrypt(&asked.ekey) else {
            log("SETUP came before the FairPlay messages");
            return bad_request();
        };
        let key = connection.pairing.session_key(key);
        let name = clean(&asked.name, "A device");
        let model = clean(&asked.model, "unknown");
        let ended = Arc::new(AtomicBool::new(false));
        if shared.config.ask
            && let Some(first_again) = shared.refused_lately(peer.ip())
        {
            log(format!("{name} was not asked about: its address was refused less than a minute ago"));
            if first_again {
                say(json!({ "event": "refused", "name": name, "why": "denied_lately" }));
            }
            return Response::status(453, "Not Enough Bandwidth").closing();
        }
        let (answer, answers) = mpsc::channel();
        let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
        {
            let Ok(mut state) = shared.state.lock() else { return bad_request() };
            if state.session.is_some() {
                drop(state);
                log(format!("{name} was refused: another device is connected"));
                say(json!({ "event": "refused", "name": name, "why": "busy" }));
                return Response::status(453, "Not Enough Bandwidth").closing();
            }
            state.session = Some(Session {
                id,
                name: name.clone(),
                peer: peer.ip(),
                allowed: !shared.config.ask,
                answer: shared.config.ask.then_some(answer),
                video: None,
                control: stream.try_clone().ok(),
                video_stream: None,
                ended: ended.clone(),
            });
        }
        if shared.config.ask {
            log(format!("{name} ({model}) asks to show its screen; holding the answer for the operator"));
            say(json!({ "event": "asks", "id": id, "name": name, "model": model }));
            let asked_at = Instant::now();
            let verdict = wait_for_answer(stream, &answers, &ended);
            let waited = asked_at.elapsed().as_millis();
            let why = match verdict {
                Some(true) => None,
                Some(false) => Some("denied"),
                None if ended.load(Ordering::Relaxed) => Some("stopped"),
                None if asked_at.elapsed() >= ASK_TIME => Some("timeout"),
                None => Some("gone"),
            };
            if let Some(why) = why {
                log(format!("the sender waited {waited} ms for an answer"));
                if why == "denied" {
                    shared.remember_refused(peer.ip());
                }
                shared.close(id, why, false);
                connection.session = None;
                return Response::status(401, "Unauthorized").closing();
            }
            let still = {
                let Ok(mut state) = shared.state.lock() else { return bad_request() };
                match state.session.as_mut().filter(|session| session.id == id) {
                    Some(session) => {
                        session.allowed = true;
                        session.answer = None;
                        true
                    }
                    None => false,
                }
            };
            if !still {
                return Response::status(401, "Unauthorized").closing();
            }
            log(format!("allowed; the sender waited {waited} ms for the answer"));
        }
        say(json!({ "event": "connected", "id": id, "name": name, "model": model }));
        let timing = start_timing(peer.ip(), asked.timing_port, shared.control_port, ended);
        connection.session = Some((id, key, timing));
    }
    let mut ports = plists::Ports { timing: 0, video: shared.video_port, sound_data: 0, sound_control: 0 };
    if !setup.streams.is_empty() {
        let Some((id, key, _)) = connection.session else {
            log("SETUP asked for streams before a session");
            return bad_request().closing();
        };
        for wanted in &setup.streams {
            match wanted {
                plists::Stream::Video { connection_id } => {
                    let Ok(mut state) = shared.state.lock() else { return bad_request() };
                    let Some(session) = state.session.as_mut().filter(|session| session.id == id) else {
                        return bad_request().closing();
                    };
                    session.video = Some(VideoCipher::new(&key, *connection_id));
                    log("the sender asked for the video stream");
                }
                plists::Stream::Sound => {
                    let ended = shared
                        .state
                        .lock()
                        .ok()
                        .and_then(|state| state.session.as_ref().filter(|s| s.id == id).map(|s| s.ended.clone()));
                    if let Some(ended) = ended {
                        (ports.sound_data, ports.sound_control) = start_sound(peer.ip(), shared.control_port, ended);
                    }
                    log("the sender asked for the sound stream (accepted; not played yet)");
                }
                plists::Stream::Other(kind) => log(format!("the sender asked for a stream of kind {kind}; ignored")),
            }
        }
    }
    if let Some((_, _, timing)) = connection.session {
        ports.timing = timing;
    }
    Response::with_body("application/x-apple-binary-plist", plists::to_bytes(&plists::setup_answer(&setup, &ports)))
}

/// Waits for the operator: `Some(allow)` when answered, `None` when the time ran out, the
/// sender went away, or the session was ended from here.
fn wait_for_answer(stream: &TcpStream, answers: &mpsc::Receiver<bool>, ended: &AtomicBool) -> Option<bool> {
    let started = Instant::now();
    let mut probe = [0_u8; 1];
    while started.elapsed() < ASK_TIME && !ended.load(Ordering::Relaxed) {
        match answers.recv_timeout(Duration::from_millis(200)) {
            Ok(allow) => return Some(allow),
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        // A sender that hangs up while waiting is not waited for.
        let _ = stream.set_nonblocking(true);
        let gone = matches!(stream.peek(&mut probe), Ok(0));
        let _ = stream.set_nonblocking(false);
        if gone {
            return None;
        }
    }
    None
}

/// Where a session's own sockets are bound: every interface of the sender's kind of address,
/// or this computer only when the sender is on it (test runs).
fn our_side(peer: IpAddr) -> IpAddr {
    match peer {
        IpAddr::V4(address) if address.is_loopback() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(address) if address.is_loopback() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    }
}

/// A UDP socket for a session, of the sender's address family: on the port wished for (when the
/// control port was fixed), or any.
fn session_socket(peer: IpAddr, wish: Option<u16>) -> Option<UdpSocket> {
    let address = our_side(peer);
    wish.and_then(|port| UdpSocket::bind((address, port)).ok()).or_else(|| UdpSocket::bind((address, 0)).ok())
}

fn port_after(control_port: u16, step: u16) -> Option<u16> {
    // Fixed ports only beside the usual control port, so a firewall rule can name them.
    control_port.is_multiple_of(10).then(|| control_port.checked_add(step)).flatten()
}

/// Asks the sender the time every three seconds, as a receiver is expected to, until the
/// session ends. The answers are read and dropped: nothing is kept in step with them yet.
fn start_timing(peer: IpAddr, their_port: u16, control_port: u16, ended: Arc<AtomicBool>) -> u16 {
    let Some(socket) = session_socket(peer, port_after(control_port, 2)) else { return 0 };
    let port = socket.local_addr().map(|address| address.port()).unwrap_or(0);
    if their_port == 0 {
        return port;
    }
    let _ = socket.set_read_timeout(Some(Duration::from_millis(200)));
    std::thread::spawn(move || {
        let mut answer = [0_u8; 128];
        let mut last: Option<Instant> = None;
        while !ended.load(Ordering::Relaxed) {
            if last.is_none_or(|at| at.elapsed() >= Duration::from_secs(3)) {
                last = Some(Instant::now());
                let _ = socket.send_to(&timing_request(SystemTime::now()), (peer, their_port));
            }
            let _ = socket.recv_from(&mut answer);
        }
    });
    port
}

/// A request for the sender's time: 32 bytes, with ours (seconds since 1900 and a fraction, big
/// end first) in the last eight.
fn timing_request(now: SystemTime) -> [u8; 32] {
    let mut request = [0_u8; 32];
    request[..4].copy_from_slice(&[0x80, 0xd2, 0x00, 0x07]);
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs().wrapping_add(2_208_988_800) as u32;
    let fraction = ((u64::from(since.subsec_nanos()) << 32) / 1_000_000_000) as u32;
    request[24..28].copy_from_slice(&seconds.to_be_bytes());
    request[28..].copy_from_slice(&fraction.to_be_bytes());
    request
}

/// Takes the sound stream's packets and drops them, until the session ends. (Sound is the
/// next milestone; a sender leaves if the stream is refused.)
fn start_sound(peer: IpAddr, control_port: u16, ended: Arc<AtomicBool>) -> (u16, u16) {
    let (Some(data), Some(control)) =
        (session_socket(peer, port_after(control_port, 3)), session_socket(peer, port_after(control_port, 4)))
    else {
        return (0, 0);
    };
    let ports = (
        data.local_addr().map(|address| address.port()).unwrap_or(0),
        control.local_addr().map(|address| address.port()).unwrap_or(0),
    );
    let _ = data.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = control.set_nonblocking(true);
    std::thread::spawn(move || {
        // A sound packet is under 2 KB; anything longer is cut by the read and dropped with it.
        let mut packet = [0_u8; 2048];
        while !ended.load(Ordering::Relaxed) {
            let _ = data.recv_from(&mut packet);
            for _ in 0..8 {
                if control.recv_from(&mut packet).is_err() {
                    break;
                }
            }
        }
    });
    ports
}

fn video_connection(shared: &Arc<Shared>, stream: TcpStream, peer: SocketAddr) {
    // Only the sender that was allowed, and only once it has asked for the stream.
    let expected = {
        let Ok(mut state) = shared.state.lock() else { return };
        match state.session.as_mut() {
            Some(session) if session.allowed && session.peer == peer.ip() && session.video.is_some() => {
                session.video_stream = stream.try_clone().ok();
                session.video.take().map(|cipher| (session.id, cipher, session.ended.clone()))
            }
            _ => None,
        }
    };
    let Some((id, mut cipher, ended)) = expected else {
        log(format!("a video connection from {} that nobody was expecting was closed", peer.ip()));
        return;
    };
    log("the video stream connected");
    let _ = stream.set_nodelay(true);
    let mut reader = &stream;
    let mut decoder: Option<Decoder> = None;
    let mut settings: Option<stream::Settings> = None;
    let mut payload = Vec::new();
    let mut annex_b = Vec::new();
    let (mut pictures, mut dropped) = (0_u64, 0_u64);
    let mut size_changes: Vec<Instant> = Vec::new();
    let why = loop {
        let mut head = [0_u8; stream::HEADER_LEN];
        if reader.read_exact(&mut head).is_err() {
            break "the video stream closed";
        }
        let Some(head) = stream::header(&head) else {
            log("a video packet claimed to be longer than any may be");
            break "the video stream was not understood";
        };
        payload.resize(head.payload_len, 0);
        if reader.read_exact(&mut payload).is_err() {
            break "the video stream closed";
        }
        match head.kind {
            stream::Kind::Settings => {
                let Some(new) = stream::settings(&head, &payload) else {
                    log(format!("the codec's settings were not understood ({} bytes)", payload.len()));
                    continue;
                };
                if settings.as_ref() == Some(&new) && decoder.is_some() {
                    continue;
                }
                // A new size (a phone turned on its side) is a new FFmpeg: it would stretch the
                // new pictures to the old size otherwise. Not without end, though.
                size_changes.retain(|at| at.elapsed() < Duration::from_secs(10));
                if size_changes.len() >= MAX_SIZE_CHANGES {
                    break "the picture changed size too often";
                }
                size_changes.push(Instant::now());
                decoder = None;
                match Decoder::start(&shared.config.ffmpeg, &new, id, shared.pictures.clone()) {
                    Ok(started) => {
                        log(format!(
                            "{} at {}x{}, shown at {}x{}",
                            new.codec.name(),
                            new.width,
                            new.height,
                            started.width,
                            started.height
                        ));
                        say(json!({
                            "event": "picture_size", "id": id, "width": started.width, "height": started.height,
                            "codec": new.codec.name(),
                        }));
                        decoder = Some(started);
                        settings = Some(new);
                    }
                    Err(error) => {
                        crate::events::problem(format!("FFmpeg could not be started ({:?})", error.kind()));
                        break "FFmpeg could not be started";
                    }
                }
            }
            stream::Kind::Picture => {
                // Every picture is decrypted, shown or not: the cipher runs across them all.
                cipher.decrypt(&mut payload);
                let (Some(decoder), Some(settings)) = (decoder.as_mut(), settings.as_ref()) else {
                    dropped += 1;
                    continue;
                };
                if !stream::annex_b(settings.codec, &payload, &mut annex_b) {
                    dropped += 1;
                    if dropped == 1 {
                        log("a picture could not be read after decrypting; dropped");
                    }
                    continue;
                }
                pictures += 1;
                if !decoder.feed(&annex_b) {
                    break "FFmpeg stopped";
                }
            }
            stream::Kind::Other(_) => {}
        }
        if ended.load(Ordering::Relaxed) {
            break "the session ended";
        }
    };
    drop(decoder);
    log(format!(
        "the video stream ended ({why}): {pictures} pictures in, {dropped} dropped, {} decoded, {} replaced by a newer one before SoftPresenter took them",
        shared.pictures.decoded.load(Ordering::Relaxed),
        shared.pictures.replaced.load(Ordering::Relaxed),
    ));
    shared.end(id, why);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timing_request_carries_the_time_since_1900() {
        let request = timing_request(UNIX_EPOCH + Duration::new(1_000_000_000, 500_000_000));
        assert_eq!(&request[..4], &[0x80, 0xd2, 0x00, 0x07]);
        assert!(request[4..24].iter().all(|byte| *byte == 0));
        assert_eq!(u32::from_be_bytes(request[24..28].try_into().unwrap()), 3_208_988_800);
        assert_eq!(u32::from_be_bytes(request[28..].try_into().unwrap()), 0x8000_0000);
    }

    #[test]
    fn fixed_ports_follow_only_the_usual_control_port() {
        assert_eq!(port_after(7100, 2), Some(7102));
        assert_eq!(port_after(7110, 4), Some(7114));
        assert_eq!(port_after(51_237, 2), None);
        assert_eq!(port_after(65_530, 9), None);
    }

    #[test]
    fn one_address_is_admitted_only_so_often_in_a_minute() {
        let (_, control, video) = bind(false, 0).unwrap();
        let config = Config {
            name: "Test".into(),
            device_id: "AA:BB:CC:DD:EE:FF".into(),
            ask: true,
            ffmpeg: PathBuf::from("ffmpeg"),
            model: discovery::MODEL.into(),
            features_2: discovery::FEATURES_2,
        };
        let shared = Shared::new(config, Identity::from_seed(&[1; 32]).unwrap(), control, video);
        let one = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
        for _ in 0..MAX_PER_MINUTE {
            assert!(shared.admit(one));
        }
        assert!(!shared.admit(one));
        assert!(shared.admit(other));
        // One address takes only so many of the places, and all of them together only so many.
        for _ in 0..MAX_CONNECTIONS_EACH {
            assert!(shared.take_place(one));
        }
        assert!(!shared.take_place(one));
        shared.give_place(one);
        assert!(shared.take_place(one));
        for n in 0..(MAX_CONNECTIONS - MAX_CONNECTIONS_EACH) as u8 {
            assert!(shared.take_place(IpAddr::V4(Ipv4Addr::new(10, 0, 1, n))));
        }
        assert!(!shared.take_place(other));
        // A refused address is remembered; another is not.
        assert_eq!(shared.refused_lately(one), None);
        shared.remember_refused(one);
        // The first try again is said to the operator; the ones after are not.
        assert_eq!(shared.refused_lately(one), Some(true));
        assert_eq!(shared.refused_lately(one), Some(false));
        assert_eq!(shared.refused_lately(other), None);
        // Stopping with nobody connected, or answering a sender that is not there, does nothing.
        shared.stop();
        shared.answer(99, true);
    }

    #[test]
    fn a_loopback_bind_is_on_127_0_0_1_only() {
        let (listeners, control, video) = bind(false, 0).unwrap();
        assert_eq!(listeners.control.len(), 1);
        assert_eq!(listeners.control[0].local_addr().unwrap().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(listeners.video[0].local_addr().unwrap().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_ne!(control, 0);
        assert_ne!(video, 0);
    }
}
