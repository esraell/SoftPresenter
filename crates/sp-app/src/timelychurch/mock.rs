//! A stand-in for a church's TimelyChurch, for the tests and for trying the app: a small HTTP
//! server on this computer (std::net only) serving a demo church in the shapes TimelyChurch
//! described for its Worship API, until its OpenAPI file and examples come.
//!
//! The demo church has two services (a setlist today and a plan in a week), a two-language
//! song whose chorus is sung three times, a song in one language, and a song it has deleted
//! (404). It answers "nothing changed" (304) to the ETag it gave, records each song use once
//! by its idempotency key, and can be changed while it runs:
//!
//! - `POST /_mock/next`: the next version of the church (first: a song's words changed, the
//!   plan removed, a new setlist; then: the two-language song's bridge changed);
//! - `POST /_mock/disconnect`: every token refused from now on (401);
//! - `POST /_mock/wait`: the next request is asked to wait (429);
//! - `GET /_mock/uses`: the song uses recorded, and how many were sent in all.
//!
//! To try the app against it: `cargo test -p sp-app timelychurch::mock::serve -- --ignored
//! --nocapture` serves it at <http://127.0.0.1:4320> until stopped (the pairing code is
//! `DEMO-2026`); start a test copy with `SOFTPRESENTER_TIMELYCHURCH_URL=http://127.0.0.1:4320`.
//! Every word in it was written for it.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// The pairing code the demo church takes.
pub const CODE: &str = "DEMO-2026";

/// The demo church's state.
#[derive(Debug)]
pub struct Demo {
    /// 0 as it starts; each `next` adds one.
    pub version: u32,
    /// Tokens given and still taken.
    pub tokens: Vec<String>,
    /// Song uses recorded: each idempotency key once, with what was sent.
    pub uses: Vec<(String, serde_json::Value)>,
    /// Every song use sent, retries included.
    pub use_posts: usize,
    /// Each request: its line and the headers that matter.
    pub requests: Vec<String>,
    /// The next this many requests are asked to wait.
    pub wait_next: u32,
    /// The day the services are dated from.
    pub today: chrono::NaiveDate,
}

/// The demo church, serving.
pub struct MockChurch {
    pub address: String,
    state: Arc<Mutex<Demo>>,
}

impl MockChurch {
    /// On a free port of this computer.
    pub fn start() -> MockChurch {
        MockChurch::start_at(0).expect("a port of this computer")
    }

    pub fn start_at(port: u16) -> std::io::Result<MockChurch> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
        let address = format!("http://{}", listener.local_addr()?);
        let state = Arc::new(Mutex::new(Demo {
            version: 0,
            tokens: Vec::new(),
            uses: Vec::new(),
            use_posts: 0,
            requests: Vec::new(),
            wait_next: 0,
            today: chrono::Local::now().date_naive(),
        }));
        let serving = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let state = serving.clone();
                std::thread::spawn(move || {
                    if let Some(request) = read_request(&mut stream) {
                        let reply = answer(&state, &request);
                        let _ = stream.write_all(&reply);
                    }
                });
            }
        });
        Ok(MockChurch { address, state })
    }

    /// Looks at or changes the church.
    pub fn with<R>(&self, change: impl FnOnce(&mut Demo) -> R) -> R {
        change(&mut self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(have, _)| have == name).map(|(_, value)| value.as_str())
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<Request> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let head_end = loop {
        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&bytes[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let (method, target) = (first.next()?.to_owned(), first.next()?.to_owned());
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_owned()))
        .collect();
    let length = headers.iter().find(|(name, _)| name == "content-length").and_then(|(_, v)| v.parse().ok());
    let length: usize = length.unwrap_or(0).min(1 << 20);
    while bytes.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
        }
    }
    let body = String::from_utf8_lossy(&bytes[head_end..(head_end + length).min(bytes.len())]).into_owned();
    let (path, query) =
        target.split_once('?').map_or((target.clone(), String::new()), |(p, q)| (p.to_owned(), q.to_owned()));
    Some(Request { method, path, query, headers, body })
}

fn reply(status: &str, headers: &[(&str, String)], body: &str) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
    if !body.is_empty() {
        out.push_str("Content-Type: application/json\r\n");
    }
    for (name, value) in headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out.into_bytes()
}

fn json(status: &str, value: &serde_json::Value) -> Vec<u8> {
    reply(status, &[], &value.to_string())
}

fn answer(state: &Mutex<Demo>, request: &Request) -> Vec<u8> {
    let mut demo = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let none_match = request.header("if-none-match").unwrap_or("-");
    let idempotency = request.header("idempotency-key").unwrap_or("-");
    demo.requests.push(format!(
        "{} {}{} | if-none-match: {none_match} | idempotency-key: {idempotency}",
        request.method,
        request.path,
        if request.query.is_empty() { String::new() } else { format!("?{}", request.query) }
    ));
    let api = "/api/v1/worship";
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/_mock/next") => {
            demo.version += 1;
            return json("200 OK", &serde_json::json!({"version": demo.version}));
        }
        ("POST", "/_mock/disconnect") => {
            demo.tokens.clear();
            return json("200 OK", &serde_json::json!({}));
        }
        ("POST", "/_mock/wait") => {
            demo.wait_next = 1;
            return json("200 OK", &serde_json::json!({}));
        }
        ("GET", "/_mock/uses") => {
            let uses: Vec<serde_json::Value> =
                demo.uses.iter().map(|(key, used)| serde_json::json!({"idempotency_key": key, "use": used})).collect();
            return json("200 OK", &serde_json::json!({"uses": uses, "posts": demo.use_posts}));
        }
        _ => {}
    }
    if demo.wait_next > 0 {
        demo.wait_next -= 1;
        return reply("429 Too Many Requests", &[("Retry-After", "1".to_owned())], r#"{"error":"rate_limited"}"#);
    }
    if request.method == "POST" && request.path == format!("{api}/devices/pair") {
        let sent: serde_json::Value = serde_json::from_str(&request.body).unwrap_or_default();
        if sent.get("pairing_code").and_then(serde_json::Value::as_str) != Some(CODE) {
            return json(
                "422 Unprocessable Entity",
                &serde_json::json!({"error": "invalid_code", "message": "That pairing code isn't valid."}),
            );
        }
        let token = format!("demo-token-{}", demo.tokens.len() + 1);
        demo.tokens.push(token.clone());
        return json("201 Created", &serde_json::json!({"token": token, "church": {"id": 7, "name": "Demo Church"}}));
    }
    let token = request.header("authorization").and_then(|value| value.strip_prefix("Bearer ")).unwrap_or("");
    if !demo.tokens.iter().any(|have| have == token) {
        return json("401 Unauthorized", &serde_json::json!({"error": "unauthorized"}));
    }
    if request.method == "GET" && request.path == format!("{api}/services") {
        let etag = format!("\"demo-v{}\"", demo.version);
        if request.header("if-none-match") == Some(etag.as_str()) {
            return reply("304 Not Modified", &[("ETag", etag)], "");
        }
        let body = services(demo.version, demo.today).to_string();
        return reply("200 OK", &[("ETag", etag)], &body);
    }
    if request.method == "GET"
        && let Some(id) = request.path.strip_prefix(&format!("{api}/songs/"))
    {
        return match song(id, demo.version) {
            Some(song) => json("200 OK", &song),
            None => json("404 Not Found", &serde_json::json!({"error": "not_found"})),
        };
    }
    if request.method == "POST" && request.path == format!("{api}/song-uses") {
        demo.use_posts += 1;
        let Some(key) = request.header("idempotency-key").map(str::to_owned) else {
            return json("400 Bad Request", &serde_json::json!({"error": "idempotency_key_required"}));
        };
        if demo.uses.iter().any(|(have, _)| *have == key) {
            return json("200 OK", &serde_json::json!({"status": "already_recorded"}));
        }
        let used: serde_json::Value = serde_json::from_str(&request.body).unwrap_or_default();
        demo.uses.push((key, used));
        return json("201 Created", &serde_json::json!({"id": demo.uses.len()}));
    }
    json("404 Not Found", &serde_json::json!({"error": "not_found"}))
}

fn day(today: chrono::NaiveDate, after: i64) -> String {
    (today + chrono::Duration::days(after)).format("%Y-%m-%d").to_string()
}

fn song_item(position: u32, song: u64, title: &str, key: &str, spot: &str) -> serde_json::Value {
    serde_json::json!({"position": position, "type": "song", "title": title, "song_id": song,
        "arrangement_id": null, "key": key, "notes": null, "worship_spot": spot})
}

/// The demo church's services, in the version it is at.
fn services(version: u32, today: chrono::NaiveDate) -> serde_json::Value {
    let changed = version >= 1;
    let mut list = vec![serde_json::json!({
        "id": 501, "type": "setlist", "name": "Sunday Morning (Demo)", "date": day(today, 0), "time": "10:00",
        "updated_at": match version {
            0 => "2026-11-20T10:00:00Z",
            1 => "2026-11-24T08:00:00Z",
            _ => "2026-11-24T09:00:00Z",
        },
        "items": [
            song_item(1, 101, "Morning Light", "G", "Opening"),
            song_item(2, 102, "Quiet Evening", if changed { "E" } else { "D" }, "Opening"),
            song_item(3, 103, "An Old Song", "C", "Response")
        ]
    })];
    if changed {
        list.push(serde_json::json!({
            "id": 502, "type": "setlist", "name": "Midweek Prayer (Demo)", "date": day(today, 3), "time": "19:00",
            "updated_at": "2026-11-24T08:00:00Z",
            "items": [song_item(1, 101, "Morning Light", "F", "Worship")]
        }));
    } else {
        list.push(serde_json::json!({
            "id": 601, "type": "plan", "name": "Evening Service (Demo)", "date": day(today, 7), "time": "18:00",
            "updated_at": "2026-11-21T09:00:00Z",
            "items": [
                {"position": 1, "type": "heading", "title": "Welcome"},
                song_item(2, 102, "Quiet Evening", "D", "Worship"),
                {"position": 3, "type": "reading", "title": "Reading", "reference": "Psalm 23"},
                {"position": 4, "type": "sermon", "title": "Message"},
                song_item(5, 101, "Morning Light", "G", "Worship")
            ]
        }));
    }
    serde_json::json!({"services": list})
}

/// A demo song by its id (and `?arrangement=`, not asked for here), in the version the church
/// is at; `None` for one it doesn't have.
fn song(id: &str, version: u32) -> Option<serde_json::Value> {
    match id {
        "101" => Some(serde_json::json!({
            "id": 101,
            "title": "Morning Light",
            "authors": ["Demo Writer"],
            "ccli_number": "0000101",
            "copyright": "© 2026 Demo Songs",
            "key": "G",
            "arrangement": {"id": 1011, "name": "Default", "key": "G", "is_default": true},
            "sections": [
                {"name": "Verse 1", "lines": ["Morning light is on the hill", "Every field is waking still", "",
                                              "Open doors and open hands", "Walk together through the lands"]},
                {"name": "Chorus", "lines": ["Sing, sing the morning song", "Sing it all the day long"]},
                {"name": "Verse 2", "lines": ["Noon is high and work is near", "Every voice is ringing clear"]},
                {"name": "Bridge", "lines": [if version >= 2 { "Hold on, hold fast" } else { "Hold on, hold on" }]}
            ],
            "order": ["Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus"],
            "translations": [{
                "language": "es",
                "title": "Luz de la mañana",
                "sections": [
                    {"name": "Verse 1", "lines": ["La luz ya está en el monte", "Cada campo se despierta", null,
                                                  "Puertas y manos abiertas", "Caminamos por las tierras"]},
                    {"name": "Chorus", "lines": ["Canta, canta la canción", "Cántala todo el día"]},
                    {"name": "Verse 2", "lines": ["El mediodía está alto", null]},
                    {"name": "Bridge", "lines": ["Espera, espera"]}
                ]
            }],
            "updated_at": if version >= 2 { "2026-11-24T09:00:00Z" } else { "2026-11-20T10:00:00Z" }
        })),
        "102" => Some(serde_json::json!({
            "id": 102,
            "title": "Quiet Evening",
            "authors": "Demo Writer, Second Writer",
            "copyright": "2025 Demo Songs",
            "key": "D",
            "arrangement": {"id": 1021, "name": "Default Arrangement", "is_default": true},
            "sections": [
                {"name": "Verse 1", "lines": [
                    if version >= 1 { "Quiet evening, gentle light" } else { "Quiet evening, gentle rest" },
                    "Every road is ending best"
                ]},
                {"name": "Chorus", "lines": ["Rest now, rest now"]}
            ],
            "order": ["Verse 1", "Chorus", "Verse 1", "Chorus"],
            "translations": [],
            "updated_at": if version >= 1 { "2026-11-24T08:00:00Z" } else { "2026-11-19T12:00:00Z" }
        })),
        _ => None,
    }
}

#[test]
#[ignore = "serves the demo church until stopped, for trying the app by hand"]
fn serve() {
    let church = MockChurch::start_at(4320).unwrap_or_else(|_| MockChurch::start());
    println!("The demo church is at {} (pairing code {CODE}).", church.address);
    println!("Start a test copy with SOFTPRESENTER_TIMELYCHURCH_URL={}", church.address);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
