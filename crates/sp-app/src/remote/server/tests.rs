//! The server end to end, on this computer's loopback address only: a real TLS connection, real
//! HTTP and a real WebSocket, with a stand-in for the app behind it.

use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use super::*;

const PASSWORD: &str = "right password";

/// The app's part, played: commands are answered at once, every slide is a small grey picture.
#[derive(Default)]
struct Fake {
    commands: Mutex<Vec<Vec<String>>>,
    refreshed: AtomicUsize,
    drawn: AtomicUsize,
}

struct FakeBackend(Arc<Fake>);

/// What the stand-in app answers a command with.
fn answer_for(args: &[String]) -> Reply {
    Reply::with(format!("Did {}.", args.join(" ")), serde_json::json!({"args": args, "mark": "\u{2713} \"quoted\""}))
}

impl Backend for FakeBackend {
    fn command(&self, _caller: Caller, args: Vec<String>) -> oneshot::Receiver<Reply> {
        let (reply, answer) = oneshot::channel();
        let _ = reply.send(answer_for(&args));
        if let Ok(mut commands) = self.0.commands.lock() {
            commands.push(args);
        }
        answer
    }

    fn thumb(&self, slide: Uuid, _version: Option<Uuid>) -> oneshot::Receiver<Option<Drawn>> {
        let (reply, answer) = oneshot::channel();
        let drawn = (!slide.is_nil()).then(|| {
            self.0.drawn.fetch_add(1, Ordering::AcqRel);
            Drawn {
                look: "00ff00ff00ff00ff".to_owned(),
                read: Box::new(|| Some(Rgba { width: 16, height: 9, pixels: vec![128; 16 * 9 * 4] })),
            }
        });
        let _ = reply.send(drawn);
        answer
    }

    fn refresh(&self) -> oneshot::Receiver<()> {
        self.0.refreshed.fetch_add(1, Ordering::AcqRel);
        let (reply, answer) = oneshot::channel();
        let _ = reply.send(());
        answer
    }

    fn pages_changed(&self) {}

    fn log(&self, _line: &str) {}
}

struct Test {
    running: Option<Running>,
    shared: Arc<Shared>,
    cert: crate::remote::cert::Cert,
    port: u16,
    folder: PathBuf,
    fake: Arc<Fake>,
    runtime: tokio::runtime::Runtime,
}

impl Drop for Test {
    fn drop(&mut self) {
        if let Some(thread) = self.running.take().and_then(Running::stop) {
            let _ = thread.join();
        }
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn start_test(name: &str) -> Test {
    let folder = std::env::temp_dir().join(format!("sp-remote-server-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut store = auth::Store::open(&folder);
    store.set_password(auth::hash_password(PASSWORD).expect("hashed")).expect("kept");
    let cert = crate::remote::cert::make(&folder).expect("certificate");
    let fake = Arc::new(Fake::default());
    let shared = Shared::new(Arc::new(Mutex::new(store)), Box::new(FakeBackend(fake.clone()))).expect("shared");
    let config = Config { bind: IpAddr::from([127, 0, 0, 1]), port: 0, cert: cert.clone() };
    let running = start(config, shared.clone()).expect("listening");
    let port = running.local.port();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    Test { running: Some(running), shared, cert, port, folder, fake, runtime }
}

impl Test {
    fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn origin(&self) -> String {
        format!("https://{}", self.host())
    }

    async fn tls(&self) -> std::io::Result<TlsStream<TcpStream>> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.cert.chain[0].clone()).map_err(std::io::Error::other)?;
        let mut config =
            rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(std::io::Error::other)?
                .with_root_certificates(roots)
                .with_no_client_auth();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let tcp = TcpStream::connect(("127.0.0.1", self.port)).await?;
        let name = rustls::pki_types::ServerName::try_from("localhost").map_err(std::io::Error::other)?.to_owned();
        connector.connect(name, tcp).await
    }

    /// One HTTP request on a connection of its own; the answer, read to its end.
    async fn http(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Answer {
        let mut stream = self.tls().await.expect("connected");
        let mut request =
            format!("{method} {path} HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
        if !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("host")) {
            request.push_str(&format!("Host: {}\r\n", self.host()));
        }
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes()).await.expect("sent");
        stream.write_all(body).await.expect("sent");
        stream.flush().await.expect("sent");
        let mut bytes = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut bytes)).await;
        Answer::parse(&bytes)
    }

    fn get(&self, path: &str, token: Option<&str>) -> Answer {
        let cookie = token.map(|token| format!("{COOKIE}={token}"));
        let mut headers = Vec::new();
        if let Some(cookie) = &cookie {
            headers.push(("Cookie", cookie.as_str()));
        }
        self.runtime.block_on(self.http("GET", path, &headers, b""))
    }

    fn sign_in(&self, password: &str) -> Answer {
        let origin = self.origin();
        let body = serde_json::json!({"password": password}).to_string();
        self.runtime.block_on(self.http(
            "POST",
            "/api/login",
            &[
                ("Origin", origin.as_str()),
                ("Content-Type", "application/json"),
                ("User-Agent", "Mozilla/5.0 (iPad) Safari/604.1"),
            ],
            body.as_bytes(),
        ))
    }

    fn token(&self) -> String {
        let answer = self.sign_in(PASSWORD);
        assert_eq!(answer.status, 200, "{}", answer.text());
        let cookie = answer.header("set-cookie").expect("a cookie").to_owned();
        for part in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
            assert!(cookie.contains(part), "{cookie} lacks {part}");
        }
        cookie.strip_prefix(&format!("{COOKIE}=")).and_then(|rest| rest.split(';').next()).expect("token").to_owned()
    }

    async fn socket(
        &self,
        token: &str,
        origin: Option<&str>,
    ) -> Result<WebSocketStream<TlsStream<TcpStream>>, tungstenite::Error> {
        let stream = self.tls().await.map_err(tungstenite::Error::Io)?;
        let mut request = format!("wss://{}/api/ws", self.host()).into_client_request()?;
        if let Some(origin) = origin {
            request.headers_mut().insert("Origin", origin.parse().expect("header"));
        }
        request.headers_mut().insert("Cookie", format!("{COOKIE}={token}").parse().expect("header"));
        tokio_tungstenite::client_async(request, stream).await.map(|(socket, _)| socket)
    }
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn parse(bytes: &[u8]) -> Answer {
        let split = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(bytes.len());
        let head = String::from_utf8_lossy(&bytes[..split]).into_owned();
        let body = bytes.get(split + 4..).unwrap_or_default().to_vec();
        let mut lines = head.lines();
        let status =
            lines.next().and_then(|line| line.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .collect();
        Answer { status, headers, body }
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(have, _)| have == name).map(|(_, value)| value.as_str())
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// The next text message from a page's socket, as JSON (heartbeats and pictures passed over).
async fn next_json(socket: &mut WebSocketStream<TlsStream<TcpStream>>) -> serde_json::Value {
    serde_json::from_str(&next_text(socket).await).expect("JSON")
}

async fn next_text(socket: &mut WebSocketStream<TlsStream<TcpStream>>) -> String {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next()).await.expect("in time");
        match message.expect("open").expect("a message") {
            tungstenite::Message::Text(text) if !text.contains("\"type\":\"beat\"") => return text.to_string(),
            tungstenite::Message::Close(frame) => panic!("closed: {frame:?}"),
            _ => {}
        }
    }
}

/// How a page's socket was closed: its close code, or `None` if it wasn't.
async fn closed_with(socket: &mut WebSocketStream<TlsStream<TcpStream>>) -> Option<u16> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), socket.next()).await.ok()? {
            Some(Ok(tungstenite::Message::Close(frame))) => return frame.map(|f| u16::from(f.code)),
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return Some(0),
        }
    }
}

#[test]
fn the_sign_in_page_is_public_and_everything_else_needs_a_device() {
    let test = start_test("public");
    assert_eq!(test.get("/login", None).status, 200);
    assert_eq!(test.get("/login.js", None).status, 200);
    assert_eq!(test.get("/style.css", None).status, 200);
    let root = test.get("/", None);
    assert_eq!(root.status, 303);
    assert_eq!(root.header("location"), Some("/login"));
    assert_eq!(test.get("/app.js", None).status, 401);
    assert_eq!(test.get("/thumb/7f6a1c2e-0000-4000-8000-000000000001/usual/00ff00ff00ff00ff.jpg", None).status, 401);
    assert_eq!(test.get("/", Some("a-token-nobody-was-given")).status, 303);
    // Every answer says what the page may load, and that it may not be framed or sniffed.
    let page = test.get("/login", None);
    let policy = page.header("content-security-policy").expect("a policy");
    assert!(policy.contains("script-src 'self'") && policy.contains("frame-ancestors 'none'"), "{policy}");
    assert!(!policy.contains("unsafe-inline"));
    assert_eq!(page.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(page.header("x-frame-options"), Some("DENY"));
    // The pages themselves never put words in as HTML.
    for script in [LOGIN_JS, APP_JS] {
        assert!(
            !script.contains("innerHTML") && !script.contains("outerHTML") && !script.contains("insertAdjacentHTML")
        );
        assert!(!script.contains("eval("));
    }
    for page in [LOGIN_HTML, INDEX_HTML] {
        assert!(!page.contains("<script>"), "no inline script");
        let handler = page.match_indices(" on").any(|(at, _)| {
            let rest = &page[at + 3..];
            let name: String = rest.chars().take_while(char::is_ascii_lowercase).collect();
            !name.is_empty() && rest[name.len()..].starts_with('=')
        });
        assert!(!handler, "no inline event handlers");
    }
}

#[test]
fn a_wrong_password_is_refused_and_slowed_and_the_right_one_remembers_the_device() {
    let test = start_test("passwords");
    for _ in 0..3 {
        let answer = test.sign_in("wrong password");
        assert_eq!(answer.status, 401, "{}", answer.text());
        assert!(answer.header("set-cookie").is_none());
    }
    // The fourth wrong one in a row must wait, and says how long.
    let slowed = test.sign_in("wrong password");
    assert_eq!(slowed.status, 429);
    assert_eq!(slowed.header("retry-after"), Some("1"));
    // While it waits, even the right password isn't looked at.
    assert_eq!(test.sign_in(PASSWORD).status, 429);
    std::thread::sleep(Duration::from_millis(1100));
    let token = test.token();
    assert_eq!(test.get("/", Some(&token)).status, 200);
    assert_eq!(test.get("/app.js", Some(&token)).status, 200);
    let devices = test.shared.store.lock().unwrap().saved.devices.clone();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "Safari on an iPad");
    assert_ne!(devices[0].token, token, "only the token's hash is kept");
}

#[test]
fn a_forgotten_device_or_a_new_password_is_refused_at_once_even_on_an_open_page() {
    let test = start_test("revoked");
    let token = test.token();
    let other = test.token();
    let id = test.shared.store.lock().unwrap().device_of(&token).expect("remembered").id.clone();
    test.runtime.block_on(async {
        let mut page = test.socket(&token, Some(&test.origin())).await.expect("connected");
        page.send(tungstenite::Message::text(r#"{"type":"hello"}"#)).await.expect("sent");
        assert_eq!(next_json(&mut page).await["type"], "snapshot");
        // Forgotten in Settings: its open page closes, signed out, and its token is refused.
        test.shared.store.lock().unwrap().forget(&id);
        test.shared.sign_out(&id);
        assert_eq!(closed_with(&mut page).await, Some(close::SIGNED_OUT));
    });
    assert_eq!(test.get("/", Some(&token)).status, 303);
    assert!(test.runtime.block_on(test.socket(&token, Some(&test.origin()))).is_err());
    // The other device is still let in, until a new password forgets every device.
    assert_eq!(test.get("/", Some(&other)).status, 200);
    test.runtime.block_on(async {
        let mut page = test.socket(&other, Some(&test.origin())).await.expect("connected");
        page.send(tungstenite::Message::text(r#"{"type":"hello"}"#)).await.expect("sent");
        next_json(&mut page).await;
        test.shared.store.lock().unwrap().set_password(auth::hash_password("a new password").unwrap()).unwrap();
        test.shared.sign_out_all();
        assert_eq!(closed_with(&mut page).await, Some(close::SIGNED_OUT));
    });
    assert_eq!(test.get("/", Some(&other)).status, 303);
    assert_eq!(test.sign_in(PASSWORD).status, 401, "the old password is gone too");
}

#[test]
fn host_and_origin_are_checked() {
    let test = start_test("host");
    let token = test.token();
    // Another name pointed at this computer (DNS rebinding) is refused, signed in or not.
    let cookie = format!("{COOKIE}={token}");
    let rebound = test.runtime.block_on(test.http("GET", "/", &[("Host", "evil.example"), ("Cookie", &cookie)], b""));
    assert_eq!(rebound.status, 421);
    let other_port = test.runtime.block_on(test.http("GET", "/", &[("Host", "127.0.0.1:1"), ("Cookie", &cookie)], b""));
    assert_eq!(other_port.status, 421);
    let named = format!("localhost:{}", test.port);
    assert_eq!(test.runtime.block_on(test.http("GET", "/", &[("Host", &named), ("Cookie", &cookie)], b"")).status, 200);
    // A sign-in from another site's page, or with no Origin at all, is refused.
    let body = br#"{"password":"right password"}"#;
    for origin in [Some("https://evil.example"), Some("http://127.0.0.1"), None] {
        let mut headers = vec![("Content-Type", "application/json")];
        if let Some(origin) = origin {
            headers.push(("Origin", origin));
        }
        let answer = test.runtime.block_on(test.http("POST", "/api/login", &headers, body));
        assert_eq!(answer.status, 403, "{origin:?}");
    }
    // A form post (no JSON) is refused even from the right page.
    let origin = test.origin();
    let form = test.runtime.block_on(test.http(
        "POST",
        "/api/login",
        &[("Origin", &origin), ("Content-Type", "application/x-www-form-urlencoded")],
        b"password=right+password",
    ));
    assert_eq!(form.status, 415);
    // The WebSocket needs this server's Origin.
    test.runtime.block_on(async {
        assert!(test.socket(&token, None).await.is_err());
        assert!(test.socket(&token, Some("https://evil.example")).await.is_err());
        assert!(test.socket(&token, Some(&test.origin())).await.is_ok());
    });
}

#[test]
fn no_file_is_served_by_a_path_from_a_request() {
    let test = start_test("paths");
    let token = test.token();
    for path in [
        "/../../etc/passwd",
        "/%2e%2e/%2e%2e/etc/passwd",
        "/static/../Cargo.toml",
        "/login/../../etc/hosts",
        "/thumb/../../etc/passwd",
        "/thumb/not-a-uuid/usual/00ff.jpg",
        "/thumb/7f6a1c2e-0000-4000-8000-000000000001/usual/..%2f..%2fetc.jpg",
        "/thumb/7f6a1c2e-0000-4000-8000-000000000001/usual/00ff.png",
        "/app.js/../../secret",
        "/settings.json",
        "/cert.der",
        "/key.der",
    ] {
        let answer = test.get(path, Some(&token));
        assert!(matches!(answer.status, 400 | 404), "{path}: {}", answer.status);
        assert!(!answer.text().contains("root:") && !answer.text().contains("argon2"), "{path}");
    }
}

#[test]
fn a_slide_picture_is_drawn_once_and_kept_with_its_look() {
    let test = start_test("pictures");
    let token = test.token();
    let path = "/thumb/7f6a1c2e-0000-4000-8000-000000000001/usual/00ff00ff00ff00ff.jpg";
    let first = test.get(path, Some(&token));
    assert_eq!(first.status, 200);
    assert_eq!(first.header("content-type"), Some("image/jpeg"));
    assert_eq!(&first.body[..2], &[0xFF, 0xD8], "a JPEG");
    assert_eq!(first.header("etag"), Some("\"00ff00ff00ff00ff\""));
    assert!(first.header("cache-control").is_some_and(|c| c.contains("immutable")));
    let again = test.get(path, Some(&token));
    assert_eq!(again.body, first.body);
    assert_eq!(test.fake.drawn.load(Ordering::Acquire), 1, "drawn once, then kept");
    // An address with an old look still gets the picture as it is now, not to be kept.
    let stale = test.get("/thumb/7f6a1c2e-0000-4000-8000-000000000001/usual/0000000000000001.jpg", Some(&token));
    assert_eq!(stale.status, 200);
    assert!(stale.header("cache-control").is_some_and(|c| c.contains("no-cache")));
    // No such slide.
    assert_eq!(test.get("/thumb/00000000-0000-0000-0000-000000000000/usual/00ff.jpg", Some(&token)).status, 404);
}

#[test]
fn a_page_gets_a_snapshot_then_the_changes_and_catches_up_after_reconnecting() {
    let test = start_test("state");
    let token = test.token();
    let publish =
        |parts: &[(&str, &str)]| test.shared.publish(parts.iter().map(|(name, json)| (*name, (*json).to_owned())));
    publish(&[("live", r#"{"slide":1}"#), ("timers", "[]"), ("playlist", r#"{"name":"Sunday"}"#)]);
    let (epoch, had) = test.runtime.block_on(async {
        let mut page = test.socket(&token, Some(&test.origin())).await.expect("connected");
        page.send(tungstenite::Message::text(r#"{"type":"hello"}"#)).await.expect("sent");
        let snapshot = next_json(&mut page).await;
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["parts"]["live"]["slide"], 1);
        assert_eq!(snapshot["parts"]["playlist"]["name"], "Sunday");
        assert_eq!(snapshot["you"]["role"], "control");
        assert_eq!(snapshot["you"]["device"], "Safari on an iPad");
        assert!(snapshot["now"].as_u64().is_some_and(|now| now > 1_700_000_000_000));
        let version = snapshot["version"].as_u64().expect("a version");
        // A change sends only the part that changed, with the next version.
        publish(&[("live", r#"{"slide":2}"#), ("timers", "[]"), ("playlist", r#"{"name":"Sunday"}"#)]);
        let change = next_json(&mut page).await;
        assert_eq!(change["type"], "change");
        assert_eq!(change["version"].as_u64(), Some(version + 1));
        assert_eq!(change["parts"].as_object().map(|parts| parts.len()), Some(1));
        assert_eq!(change["parts"]["live"]["slide"], 2);
        // Publishing the same again sends nothing.
        publish(&[("live", r#"{"slide":2}"#), ("timers", "[]"), ("playlist", r#"{"name":"Sunday"}"#)]);
        assert!(tokio::time::timeout(Duration::from_millis(300), next_text(&mut page)).await.is_err());
        (snapshot["epoch"].as_str().unwrap().to_owned(), version + 1)
    });
    // While the page is away, two parts change; coming back it is sent just those.
    publish(&[("live", r#"{"slide":3}"#), ("timers", "[1]"), ("playlist", r#"{"name":"Sunday"}"#)]);
    test.runtime.block_on(async {
        let mut page = test.socket(&token, Some(&test.origin())).await.expect("connected");
        let hello = serde_json::json!({"type": "hello", "resume": {"epoch": epoch, "version": had}});
        page.send(tungstenite::Message::text(hello.to_string())).await.expect("sent");
        let caught_up = next_json(&mut page).await;
        assert_eq!(caught_up["type"], "change");
        let parts = caught_up["parts"].as_object().expect("parts");
        assert_eq!(parts.keys().collect::<Vec<_>>(), ["live", "timers"]);
        // From another start (or too far ahead): a snapshot.
        let mut stale = test.socket(&token, Some(&test.origin())).await.expect("connected");
        let hello = serde_json::json!({"type": "hello", "resume": {"epoch": "another", "version": 1}});
        stale.send(tungstenite::Message::text(hello.to_string())).await.expect("sent");
        assert_eq!(next_json(&mut stale).await["type"], "snapshot");
    });
    assert!(test.fake.refreshed.load(Ordering::Acquire) >= 3, "the app is asked for a fresh state at each hello");
}

#[test]
fn commands_come_back_as_the_same_reply_sp_gets_and_closed_ones_never_reach_the_app() {
    let test = start_test("commands");
    let token = test.token();
    test.runtime.block_on(async {
        let mut page = test.socket(&token, Some(&test.origin())).await.expect("connected");
        page.send(tungstenite::Message::text(r#"{"type":"hello"}"#)).await.expect("sent");
        next_json(&mut page).await;
        let command = serde_json::json!({"type": "command", "id": 7, "args": ["next", "--aim", "2"]});
        page.send(tungstenite::Message::text(command.to_string())).await.expect("sent");
        let text = next_text(&mut page).await;
        // Byte for byte what the control socket writes for the same reply.
        let args: Vec<String> = ["next", "--aim", "2"].iter().map(|s| (*s).to_owned()).collect();
        let sp = serde_json::to_string(&answer_for(&args)).expect("written");
        assert_eq!(text, format!("{{\"type\":\"reply\",\"id\":7,\"reply\":{sp}}}"));
        // A closed command is refused in words, and the app never hears of it.
        let closed = serde_json::json!({"type": "command", "id": "q", "args": ["menu", "Quit"]});
        page.send(tungstenite::Message::text(closed.to_string())).await.expect("sent");
        let refused = next_json(&mut page).await;
        assert_eq!(refused["id"], "q");
        assert_eq!(refused["reply"]["ok"], false);
        assert!(refused["reply"]["text"].as_str().is_some_and(|t| t.contains("computer")), "{refused}");
        for args in [r#"["song","import","/etc/hosts"]"#, r#"["remote","off"]"#, r#"["status","--wait","5"]"#] {
            let closed = format!(r#"{{"type":"command","id":1,"args":{args}}}"#);
            page.send(tungstenite::Message::text(closed)).await.expect("sent");
            assert_eq!(next_json(&mut page).await["reply"]["ok"], false, "{args}");
        }
        // A command without an id, or with args that aren't words, is told so.
        page.send(tungstenite::Message::text(r#"{"type":"command","args":["next"]}"#)).await.expect("sent");
        assert_eq!(next_json(&mut page).await["type"], "error");
        page.send(tungstenite::Message::text(r#"{"type":"command","id":2,"args":[1,2]}"#)).await.expect("sent");
        assert_eq!(next_json(&mut page).await["reply"]["ok"], false);
    });
    let commands = test.fake.commands.lock().unwrap().clone();
    assert_eq!(commands, vec![vec!["next".to_owned(), "--aim".to_owned(), "2".to_owned()]]);
}

#[test]
fn switching_off_closes_every_page_and_stops_listening() {
    let mut test = start_test("off");
    let token = test.token();
    let port = test.port;
    let running = test.running.take().expect("running");
    test.runtime.block_on(async {
        let mut page = test.socket(&token, Some(&test.origin())).await.expect("connected");
        page.send(tungstenite::Message::text(r#"{"type":"hello"}"#)).await.expect("sent");
        next_json(&mut page).await;
        let thread = running.stop().expect("its thread");
        assert_eq!(closed_with(&mut page).await, Some(close::SWITCHED_OFF));
        tokio::task::spawn_blocking(move || thread.join()).await.expect("joined").expect("ended");
    });
    // Nothing listens on the port any more.
    assert!(
        std::net::TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_secs(1))
            .is_err()
    );
}

#[test]
fn limits_hold() {
    let test = start_test("limits");
    let token = test.token();
    // A sign-in longer than a few kilobytes isn't read.
    let origin = test.origin();
    let long = format!(r#"{{"password":"{}"}}"#, "x".repeat(MOST_LOGIN_BYTES * 2));
    let answer = test.runtime.block_on(test.http(
        "POST",
        "/api/login",
        &[("Origin", &origin), ("Content-Type", "application/json")],
        long.as_bytes(),
    ));
    assert_eq!(answer.status, 413);
    test.runtime.block_on(async {
        // A message past the limit closes the page.
        let mut page = test.socket(&token, Some(&test.origin())).await.expect("connected");
        let big = format!(r#"{{"type":"command","id":1,"args":["{}"]}}"#, "x".repeat(MOST_MESSAGE_BYTES + 10));
        let _ = page.send(tungstenite::Message::text(big)).await;
        assert!(closed_with(&mut page).await.is_some());
        drop(page);
        tokio::time::sleep(Duration::from_millis(300)).await;
        // One device has at most a few pages open at once.
        let mut pages = Vec::new();
        for _ in 0..MOST_PAGES_A_DEVICE {
            let mut page = test.socket(&token, Some(&test.origin())).await.expect("connected");
            page.send(tungstenite::Message::text(r#"{"type":"hello"}"#)).await.expect("sent");
            next_json(&mut page).await;
            pages.push(page);
        }
        assert!(test.socket(&token, Some(&test.origin())).await.is_err());
        // Commands past twenty a second (forty at once) are refused, not queued without end.
        let page = &mut pages[0];
        let mut refused = 0;
        for id in 0..60 {
            let command = serde_json::json!({"type": "command", "id": id, "args": ["status"]});
            page.send(tungstenite::Message::text(command.to_string())).await.expect("sent");
        }
        for _ in 0..60 {
            let reply = next_json(page).await;
            if reply["reply"]["ok"] == false {
                refused += 1;
            }
        }
        assert!(refused >= 10, "{refused} refused");
    });
    // A connection that never finishes its TLS handshake is let go.
    let mut idle = std::net::TcpStream::connect(("127.0.0.1", test.port)).expect("connected");
    idle.set_read_timeout(Some(HANDSHAKE_WITHIN + Duration::from_secs(5))).unwrap();
    let mut byte = [0u8; 1];
    let read = std::io::Read::read(&mut idle, &mut byte);
    assert!(matches!(read, Ok(0) | Err(_)), "closed by the server");
}

#[test]
fn only_the_local_network_is_answered() {
    for local in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.9",
        "172.31.255.1",
        "192.168.1.20",
        "169.254.3.4",
        "100.64.0.1",
        "100.127.9.9",
        "::1",
        "fd12::5",
        "fe80::1",
        "::ffff:192.168.0.2",
    ] {
        assert!(local_network(local.parse().unwrap()), "{local}");
    }
    for public in ["8.8.8.8", "172.32.0.1", "100.128.0.1", "1.1.1.1", "2001:db8::1", "::ffff:8.8.4.4"] {
        assert!(!local_network(public.parse().unwrap()), "{public}");
    }
}

#[test]
fn hosts_are_the_connections_own_address() {
    let local: SocketAddr = "192.168.1.5:4316".parse().unwrap();
    assert!(host_allowed("192.168.1.5:4316", local));
    assert!(!host_allowed("192.168.1.5", local));
    assert!(!host_allowed("192.168.1.6:4316", local));
    assert!(!host_allowed("localhost:4316", local), "localhost only on this computer");
    assert!(!host_allowed("evil.example:4316", local));
    let loopback: SocketAddr = "127.0.0.1:4316".parse().unwrap();
    assert!(host_allowed("localhost:4316", loopback));
    assert!(host_allowed("LOCALHOST:4316", loopback));
    let mapped: SocketAddr = "[::ffff:10.0.0.2]:4316".parse().unwrap();
    assert!(host_allowed("10.0.0.2:4316", mapped));
    let v6: SocketAddr = "[fe80::1]:4316".parse().unwrap();
    assert!(host_allowed("[fe80::1]:4316", v6));
}
