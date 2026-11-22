//! Canva Sync against a stand-in for Canva on 127.0.0.1: never the real API, and never a real
//! sign-in. Every secret here holds the word SENTINEL, which must never come out in words.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use super::*;

const CLIENT_ID: &str = "OC-test-client";
const CLIENT_SECRET: &str = "cnvca-SECRET-SENTINEL";
const DESIGN: &str = "DAtestDesign1";

/// What the stand-in knows and has seen.
#[derive(Default)]
struct Canva {
    /// The one refresh token that works now, and how many have been given.
    refresh: String,
    access: String,
    given: u32,
    /// The sign-in under way: the code it gave, and the challenge it was given.
    code: Option<String>,
    challenge: Option<String>,
    pages: Vec<(u32, Option<String>)>,
    /// How many times an export says it is still going before it is done, and whether it fails.
    polls_before_done: u32,
    polls: u32,
    export_fails: bool,
    exported_pages: Vec<u32>,
    video: Vec<u8>,
    video_status: u16,
    /// The file the sign-in is kept in: on each API call after a refresh, whether it held the
    /// new refresh token by then.
    store: Option<PathBuf>,
    kept_before_next_call: Vec<bool>,
    seen: Vec<String>,
    /// A refresh answered 200 with something that isn't tokens.
    token_garbage: bool,
}

struct Fake {
    port: u16,
    canva: Arc<Mutex<Canva>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn fake(canva: Canva) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let canva = Arc::new(Mutex::new(canva));
    let stop = Arc::new(AtomicBool::new(false));
    let (shared, stopped) = (canva.clone(), stop.clone());
    std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => answer(stream, port, &shared),
                Err(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    });
    Fake { port, canva, stop }
}

/// Reads one request: its first line, headers and body.
fn read_request(stream: &mut TcpStream) -> (String, HashMap<String, String>, Vec<u8>) {
    stream.set_nonblocking(false).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_owned());
        }
    }
    let length: usize = headers.get("content-length").and_then(|l| l.parse().ok()).unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (line.trim().to_owned(), headers, body)
}

fn respond(stream: &mut TcpStream, status: u16, body: &[u8], kind: &str) {
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json(stream: &mut TcpStream, status: u16, value: serde_json::Value) {
    respond(stream, status, value.to_string().as_bytes(), "application/json");
}

fn form_value(body: &[u8], name: &str) -> Option<String> {
    query_value(&String::from_utf8_lossy(body), name)
}

fn answer(mut stream: TcpStream, port: u16, shared: &Mutex<Canva>) {
    let (line, headers, body) = read_request(&mut stream);
    let mut canva = shared.lock().unwrap();
    canva.seen.push(line.clone());
    let target = line.split(' ').nth(1).unwrap_or("").to_owned();
    let (path, _) = target.split_once('?').unwrap_or((&target, ""));
    let bearer = headers.get("authorization").and_then(|a| a.strip_prefix("Bearer ")).unwrap_or("").to_owned();
    if path.starts_with("/rest/v1/") && path != "/rest/v1/oauth/token" {
        // Each API call after a refresh: the rotated refresh token is kept by now.
        if let Some(store) = canva.store.clone() {
            let kept = Store::File(store).load().ok().flatten().is_some_and(|s| s.refresh_token == canva.refresh);
            canva.kept_before_next_call.push(kept);
        }
        if bearer != canva.access || bearer.is_empty() {
            return json(&mut stream, 401, serde_json::json!({"code": "invalid_access_token"}));
        }
    }
    let give_tokens = |canva: &mut Canva, stream: &mut TcpStream| {
        canva.given += 1;
        canva.refresh = format!("REFRESH-SENTINEL-{}", canva.given);
        canva.access = format!("ACCESS-SENTINEL-{}", canva.given);
        let reply = serde_json::json!({
            "access_token": canva.access, "refresh_token": canva.refresh, "token_type": "Bearer", "expires_in": 14400
        });
        json(stream, 200, reply);
    };
    match path {
        "/rest/v1/oauth/token" => {
            let basic = format!("Basic {}", base64(format!("{CLIENT_ID}:{CLIENT_SECRET}").as_bytes(), STANDARD));
            if headers.get("authorization") != Some(&basic) {
                return json(&mut stream, 401, serde_json::json!({"code": "invalid_client"}));
            }
            match form_value(&body, "grant_type").as_deref() {
                Some("authorization_code") => {
                    let verifier = form_value(&body, "code_verifier").unwrap_or_default();
                    let good = form_value(&body, "code") == canva.code
                        && canva.challenge.as_deref() == Some(challenge_of(&verifier).as_str())
                        && form_value(&body, "redirect_uri").is_some_and(|r| r.starts_with("http://127.0.0.1:"));
                    if good {
                        canva.code = None;
                        give_tokens(&mut canva, &mut stream);
                    } else {
                        json(&mut stream, 400, serde_json::json!({"code": "invalid_grant", "message": "SENTINEL"}));
                    }
                }
                Some("refresh_token") if canva.token_garbage => {
                    respond(&mut stream, 200, b"<html>SENTINEL</html>", "text/html");
                }
                Some("refresh_token")
                    if form_value(&body, "refresh_token").as_deref() == Some(canva.refresh.as_str()) =>
                {
                    give_tokens(&mut canva, &mut stream);
                }
                _ => json(&mut stream, 400, serde_json::json!({"code": "invalid_grant", "message": "SENTINEL"})),
            }
        }
        _ if path == format!("/rest/v1/designs/{DESIGN}/pages") => {
            let items: Vec<serde_json::Value> = canva
                .pages
                .iter()
                .map(|(number, id)| {
                    let thumbnail = serde_json::json!({"width": 2, "height": 2, "url": format!("http://127.0.0.1:{port}/thumb/{number}?sig=SENTINEL")});
                    match id {
                        Some(id) => serde_json::json!({"page_number": number, "id": id, "thumbnail": thumbnail}),
                        None => serde_json::json!({"page_number": number}),
                    }
                })
                .collect();
            json(&mut stream, 200, serde_json::json!({"items": items}));
        }
        "/rest/v1/exports" => {
            let asked: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            assert_eq!(asked["design_id"], DESIGN);
            assert_eq!(asked["format"]["type"], "mp4");
            assert_eq!(asked["format"]["quality"], "horizontal_1080p");
            canva.exported_pages =
                asked["format"]["pages"].as_array().unwrap().iter().map(|n| n.as_u64().unwrap() as u32).collect();
            json(&mut stream, 200, serde_json::json!({"job": {"id": "job-1", "status": "in_progress"}}));
        }
        "/rest/v1/exports/job-1" => {
            canva.polls += 1;
            let reply = if canva.polls <= canva.polls_before_done {
                serde_json::json!({"job": {"id": "job-1", "status": "in_progress"}})
            } else if canva.export_fails {
                serde_json::json!({"job": {"id": "job-1", "status": "failed", "error": {"code": "internal_failure", "message": "SENTINEL"}}})
            } else {
                serde_json::json!({"job": {"id": "job-1", "status": "success", "urls": [format!("http://127.0.0.1:{port}/download/video.mp4?sig=SIG-SENTINEL")]}})
            };
            json(&mut stream, 200, reply);
        }
        "/download/video.mp4" => {
            let (status, video) = (canva.video_status, canva.video.clone());
            drop(canva);
            respond(&mut stream, status, &video, "video/mp4");
        }
        _ if path.starts_with("/thumb/") => {
            let mut png = Vec::new();
            image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 30, 30, 255]))
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            respond(&mut stream, 200, &png, "image/png");
        }
        _ => respond(&mut stream, 404, b"", "text/plain"),
    }
}

/// A folder of a test's own.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("softpresenter-canva-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn places(dir: &Path, port: u16) -> Places {
    let jobs = dir.join("jobs");
    Places {
        api: format!("http://127.0.0.1:{port}/rest/v1"),
        authorize: format!("http://127.0.0.1:{port}/api/oauth/authorize"),
        redirect_port: 0,
        store: Store::File(dir.join("sign-in.json")),
        old_script: dir.join("old-script"),
        old_jobs: vec![
            (OldKind::Daemon, jobs.join("LaunchDaemons")),
            (OldKind::Agent, jobs.join("LaunchAgents")),
            (OldKind::Plugin, jobs.join("Plugins")),
        ],
        staging: dir.join("staging"),
        lock: dir.join(".canva-sync.lock"),
        stand_in: true,
        settings: dir.join("canva-sync.json"),
        folder: dir.join("Media").join(FOLDER),
        poll_every: Duration::from_millis(10),
        export_for: Duration::from_secs(5),
    }
}

/// A video as Canva sends one: large enough, with an MP4 header.
fn mp4() -> Vec<u8> {
    let mut video = vec![0, 0, 0, 0x20];
    video.extend_from_slice(b"ftypisom");
    video.resize(1_200_000, 7);
    video
}

fn signed_in(refresh: &str) -> Secrets {
    Secrets {
        client_id: CLIENT_ID.to_owned(),
        client_secret: CLIENT_SECRET.to_owned(),
        refresh_token: refresh.to_owned(),
        ..Secrets::default()
    }
}

fn page_list(numbers: &[(u32, &str)]) -> Vec<(u32, Option<String>)> {
    numbers.iter().map(|(n, id)| (*n, Some((*id).to_owned()))).collect()
}

/// True when no download is left in the staging folder.
fn nothing_staged(places: &Places) -> bool {
    std::fs::read_dir(&places.staging)
        .map(|entries| {
            entries.filter_map(Result::ok).all(|e| !e.file_name().to_string_lossy().starts_with(DOWNLOADING))
        })
        .unwrap_or(true)
}

fn no_sentinel(words: &str) {
    assert!(!words.contains("SENTINEL"), "a secret came out in words: {words}");
}

#[test]
fn the_proof_is_the_standard_s256_and_each_sign_in_has_its_own() {
    // RFC 7636's own example.
    assert_eq!(
        challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let (one, two) = (Pkce::new(), Pkce::new());
    assert!((43..=128).contains(&one.verifier.len()), "{}", one.verifier.len());
    assert!(one.verifier.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
    assert_eq!(one.challenge, challenge_of(&one.verifier));
    assert!(one.verifier != two.verifier && one.state != two.state && one.state.len() >= 32);
    assert_eq!(base64(b"user:pass", STANDARD), "dXNlcjpwYXNz");
    assert_eq!(
        (base64(b"a", STANDARD), base64(b"ab", STANDARD), base64(b"a", URL_SAFE)),
        ("YQ==".into(), "YWI=".into(), "YQ".into())
    );
    assert_eq!(decoded("a%2Fb+c%zz"), "a/b c%zz");
    assert_eq!(encoded("design:content:read"), "design%3Acontent%3Aread");
}

#[test]
fn a_designs_link_gives_its_id_and_the_video_a_name_volunteers_read() {
    assert_eq!(design_id("https://www.canva.com/design/DAFabc123/xyz/edit?utm=1").as_deref(), Some("DAFabc123"));
    assert_eq!(design_id(" DAFabc123 ").as_deref(), Some("DAFabc123"));
    assert_eq!(design_id("https://www.canva.com/design/a b/edit"), None);
    assert_eq!(design_id("short"), None);
    use chrono::TimeZone;
    let friday = chrono::Local.with_ymd_and_hms(2026, 11, 20, 6, 0, 0).single().unwrap();
    assert_eq!(video_name(friday), "Friday 6.00 AM - Announcements 11-20-26.mp4");
    let evening = chrono::Local.with_ymd_and_hms(2026, 11, 22, 18, 5, 0).single().unwrap();
    assert_eq!(video_name(evening), "Sunday 6.05 PM - Announcements 11-22-26.mp4");
}

#[test]
fn only_the_pages_above_the_divider_are_exported_and_a_missing_divider_exports_nothing() {
    let listed: Vec<Page> = [(1, "p1"), (2, "p2"), (3, "divider"), (4, "old")]
        .iter()
        .map(|(n, id)| Page { number: *n, id: Some((*id).to_owned()), thumbnail: None })
        .collect();
    assert_eq!(pages_above(&listed, "divider"), Ok(vec![1, 2]));
    assert!(pages_above(&listed, "gone").unwrap_err().contains("divider page isn't in the design"));
    assert!(pages_above(&listed, "p1").unwrap_err().contains("no pages above"));
}

#[test]
fn signing_in_takes_one_answer_on_this_computer_with_the_right_state_and_keeps_the_sign_in() {
    let canva = fake(Canva::default());
    let dir = folder("sign-in");
    let places = places(&dir, canva.port);
    let shared = canva.canva.clone();
    let cancel = AtomicBool::new(false);
    let answered: Arc<Mutex<Vec<String>>> = Arc::default();
    let said = answered.clone();
    // The browser: Canva's page asks, the operator agrees, and Canva sends the browser back.
    let browser = move |url: &str| -> Result<(), String> {
        let query = url.split_once('?').unwrap().1;
        let value = |name| query_value(query, name).unwrap();
        assert_eq!(value("response_type"), "code");
        assert_eq!(value("client_id"), CLIENT_ID);
        assert_eq!(value("code_challenge_method"), "S256");
        assert_eq!(value("scope"), SCOPE);
        let redirect = value("redirect_uri");
        assert!(redirect.starts_with("http://127.0.0.1:") && redirect.ends_with(REDIRECT_PATH));
        {
            let mut canva = shared.lock().unwrap();
            canva.challenge = Some(value("code_challenge"));
            canva.code = Some("CODE-SENTINEL".to_owned());
        }
        let state = value("state");
        std::thread::spawn(move || {
            let port = redirect.trim_start_matches("http://127.0.0.1:").split('/').next().unwrap().to_owned();
            let get = |target: &str| {
                let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                write!(stream, "GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
                let mut reply = String::new();
                let _ = stream.read_to_string(&mut reply);
                reply.lines().next().unwrap_or("").to_owned()
            };
            let mut said = said.lock().unwrap();
            said.push(get("/favicon.ico"));
            said.push(get(&format!("{REDIRECT_PATH}?code=CODE-SENTINEL&state={state}")));
            // Closed once it has its answer.
            std::thread::sleep(Duration::from_millis(50));
            said.push(TcpStream::connect(format!("127.0.0.1:{port}")).map_or("closed", |_| "open").to_owned());
        });
        Ok(())
    };
    let secrets = sign_in(&places, CLIENT_ID, CLIENT_SECRET, browser, &cancel).expect("signed in");
    std::thread::sleep(Duration::from_millis(200));
    let answered = answered.lock().unwrap().clone();
    assert!(answered[0].contains("404"), "anything else is told it isn't there: {answered:?}");
    assert!(answered[1].contains("200"), "{answered:?}");
    assert_eq!(answered[2], "closed", "the listener lets go once it has its answer");
    assert_eq!(secrets.refresh_token, "REFRESH-SENTINEL-1");
    assert_eq!(places.store.load().unwrap(), Some(secrets.clone()), "kept before it counts");
    no_sentinel(&format!("{secrets:?}"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_answer_with_another_state_is_passed_over_and_the_right_one_still_signs_in() {
    let canva = fake(Canva::default());
    let dir = folder("wrong-state");
    let places = places(&dir, canva.port);
    let shared = canva.canva.clone();
    let answers: Arc<Mutex<Vec<String>>> = Arc::default();
    let said = answers.clone();
    let browser = move |url: &str| -> Result<(), String> {
        let query = url.split_once('?').unwrap().1;
        let (redirect, state) = (query_value(query, "redirect_uri").unwrap(), query_value(query, "state").unwrap());
        {
            let mut canva = shared.lock().unwrap();
            canva.challenge = query_value(query, "code_challenge");
            canva.code = Some("CODE-SENTINEL".to_owned());
        }
        std::thread::spawn(move || {
            let port = redirect.trim_start_matches("http://127.0.0.1:").split('/').next().unwrap().to_owned();
            let get = |pieces: &[String]| {
                let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                for piece in pieces {
                    stream.write_all(piece.as_bytes()).unwrap();
                    std::thread::sleep(Duration::from_millis(30));
                }
                let mut reply = String::new();
                let _ = stream.read_to_string(&mut reply);
                reply.lines().next().unwrap_or("").to_owned()
            };
            let mut said = said.lock().unwrap();
            // Someone else's answer, and one with no state: told so, and the sign-in goes on.
            said.push(get(&[format!("GET {REDIRECT_PATH}?code=CODE-SENTINEL&state=someone-else HTTP/1.1\r\n\r\n")]));
            said.push(get(&[format!("GET {REDIRECT_PATH}?code=CODE-SENTINEL HTTP/1.1\r\n\r\n")]));
            // The right one, its first line in two pieces.
            let first = format!("GET {REDIRECT_PATH}?code=CODE-SENTI");
            said.push(get(&[first, format!("NEL&state={state} HTTP/1.1\r\n\r\n")]));
        });
        Ok(())
    };
    let secrets = sign_in(&places, CLIENT_ID, CLIENT_SECRET, browser, &AtomicBool::new(false)).expect("signed in");
    std::thread::sleep(Duration::from_millis(200));
    let answers = answers.lock().unwrap().clone();
    assert!(answers[0].contains("400") && answers[1].contains("400"), "{answers:?}");
    assert!(answers[2].contains("200"), "{answers:?}");
    assert_eq!(places.store.load().unwrap(), Some(secrets));
    // A sign-in stopped while it waits ends at once.
    let stopped = AtomicBool::new(true);
    let why = sign_in(&places, CLIENT_ID, CLIENT_SECRET, |_| Ok(()), &stopped).unwrap_err();
    assert!(why.contains("stopped"), "{why}");
    let _ = std::fs::remove_dir_all(dir);
}

/// The name of the video the app made before, in these tests.
const OLD_VIDEO: &str = "Sunday 6.00 AM - Announcements 11-15-26.mp4";

/// A sync's job against the stand-in, with the sign-in kept as given.
fn job(places: &Places, kept: Option<Secrets>) -> SyncJob {
    if let Some(kept) = &kept {
        places.store.save(kept).unwrap();
    }
    SyncJob {
        places: places.clone(),
        secrets: kept,
        unsaved: false,
        design: DESIGN.to_owned(),
        divider: "divider".to_owned(),
        made: vec![OLD_VIDEO.to_owned(), "../outside.mp4".to_owned()],
        cancel: Arc::new(AtomicBool::new(false)),
        progress: channel().0,
        wake: None,
    }
}

/// The stand-in with a design of five pages, the divider fourth, and a good video.
fn usual() -> Canva {
    Canva {
        refresh: "REFRESH-SENTINEL-0".to_owned(),
        pages: page_list(&[(1, "p1"), (2, "p2"), (3, "p3"), (4, "divider"), (5, "old")]),
        polls_before_done: 2,
        video: mp4(),
        video_status: 200,
        ..Canva::default()
    }
}

#[test]
fn a_sync_keeps_the_rotated_token_before_anything_else_and_puts_the_video_in_place() {
    let dir = folder("sync");
    let canva = fake(Canva { store: Some(dir.join("sign-in.json")), ..usual() });
    let places = places(&dir, canva.port);
    std::fs::create_dir_all(&places.folder).unwrap();
    let old = places.folder.join(OLD_VIDEO);
    std::fs::write(&old, mp4()).unwrap();
    // A video the church put there itself, and one named as the app names its own: neither is
    // the app's to delete.
    let theirs = places.folder.join("Welcome loop.mp4");
    std::fs::write(&theirs, mp4()).unwrap();
    std::fs::write(places.folder.join("Friday 6.00 AM - Announcements 11-13-26.mp4"), mp4()).unwrap();
    std::fs::write(dir.join("Media").join("outside.mp4"), mp4()).unwrap();
    let (secrets, unsaved, outcome) = run_sync(job(&places, Some(signed_in("REFRESH-SENTINEL-0"))));
    let synced = outcome.expect("synced");
    let seen = canva.canva.lock().unwrap();
    assert_eq!(seen.exported_pages, [1, 2, 3], "only the pages above the divider");
    assert!(seen.kept_before_next_call.iter().all(|kept| *kept), "the new refresh token was kept before each call");
    assert!(!seen.kept_before_next_call.is_empty());
    assert_eq!(seen.given, 1, "one refresh");
    assert!(!unsaved);
    let kept = places.store.load().unwrap().unwrap();
    assert_eq!(kept.refresh_token, "REFRESH-SENTINEL-1");
    assert_eq!(secrets, Some(kept.clone()));
    assert!(kept.fresh_access(60).is_some(), "the access token is kept with its expiry");
    let name = crate::file_name(&synced.path);
    assert!(name.ends_with(".mp4") && name.contains(" - Announcements "), "{name}");
    assert_eq!(std::fs::read(&synced.path).unwrap(), mp4());
    assert_eq!(synced.others, vec![old.clone()], "only the one the app made, to go once nothing plays it");
    assert!(old.exists(), "the old one stays until then");
    assert!(nothing_staged(&places), "nothing left half-downloaded");
    drop(seen);
    // A second sync uses the access token kept, without refreshing.
    let (_, _, again) = run_sync(job(&places, Some(kept)));
    again.expect("synced again");
    assert_eq!(canva.canva.lock().unwrap().given, 1, "no second refresh while the access token has time");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_failure_keeps_the_video_there_as_it_was_and_names_no_secret() {
    type Change = Box<dyn Fn(&mut Canva)>;
    let cases: Vec<(&str, Change, &str)> = vec![
        (
            "divider-gone",
            Box::new(|c: &mut Canva| c.pages = page_list(&[(1, "p1"), (2, "p2")])),
            "divider page isn't in the design",
        ),
        (
            "divider-first",
            Box::new(|c: &mut Canva| c.pages = page_list(&[(1, "divider"), (2, "p2")])),
            "no pages above",
        ),
        ("export-fails", Box::new(|c: &mut Canva| c.export_fails = true), "export failed"),
        ("too-small", Box::new(|c: &mut Canva| c.video = b"\0\0\0\x20ftypisom".to_vec()), "isn't a whole video"),
        ("not-mp4", Box::new(|c: &mut Canva| c.video = vec![1; 1_200_000]), "isn't a whole video"),
        ("download-refused", Box::new(|c: &mut Canva| c.video_status = 403), "couldn't be downloaded"),
        (
            "sign-in-gone",
            Box::new(|c: &mut Canva| c.refresh = "REFRESH-SENTINEL-elsewhere".to_owned()),
            "no longer takes",
        ),
        ("export-too-long", Box::new(|c: &mut Canva| c.polls_before_done = 10_000), "didn't finish in time"),
        ("answer-unreadable", Box::new(|c: &mut Canva| c.token_garbage = true), "may have used up the sign-in"),
    ];
    for (name, change, said) in cases {
        let dir = folder(name);
        let mut canva = usual();
        change(&mut canva);
        let canva = fake(canva);
        let mut places = places(&dir, canva.port);
        places.export_for = Duration::from_millis(300);
        std::fs::create_dir_all(&places.folder).unwrap();
        let old = places.folder.join(OLD_VIDEO);
        std::fs::write(&old, b"the video on screen").unwrap();
        let (_, _, outcome) = run_sync(job(&places, Some(signed_in("REFRESH-SENTINEL-0"))));
        let failure = outcome.expect_err(name);
        assert!(failure.words().contains(said), "{name}: {}", failure.words());
        no_sentinel(&failure.words());
        no_sentinel(&format!("{failure:?}"));
        let left: Vec<PathBuf> = std::fs::read_dir(&places.folder).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(left, vec![old.clone()], "{name}: the folder holds the old video alone");
        assert_eq!(std::fs::read(&old).unwrap(), b"the video on screen", "{name}: untouched");
        assert!(nothing_staged(&places), "{name}: nothing half-downloaded left");
        if name == "sign-in-gone" {
            assert!(matches!(failure, Failure::SignInGone(_)), "a refused sign-in stops the asking");
        }
        if name == "answer-unreadable" {
            assert!(matches!(failure, Failure::Doubtful(_)), "a refresh answered but unreadable is in doubt");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_sync_stopped_while_canva_renders_leaves_nothing_behind() {
    let dir = folder("cancel");
    let canva = fake(Canva { polls_before_done: 10_000, ..usual() });
    let places = places(&dir, canva.port);
    let job = job(&places, Some(signed_in("REFRESH-SENTINEL-0")));
    let cancel = job.cancel.clone();
    let running = std::thread::spawn(move || run_sync(job));
    std::thread::sleep(Duration::from_millis(150));
    cancel.store(true, Ordering::Relaxed);
    let (secrets, _, outcome) = running.join().unwrap();
    assert_eq!(outcome, Err(Failure::Cancelled));
    assert_eq!(secrets.map(|s| s.refresh_token), Some("REFRESH-SENTINEL-1".to_owned()), "the refresh it made is kept");
    assert_eq!(places.store.load().unwrap().map(|s| s.refresh_token), Some("REFRESH-SENTINEL-1".to_owned()));
    assert!(!places.folder.exists() || std::fs::read_dir(&places.folder).unwrap().next().is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_new_sign_in_that_cant_be_kept_is_held_and_kept_first_next_time() {
    let dir = folder("unkept");
    let canva = fake(usual());
    let mut places = places(&dir, canva.port);
    places.store.save(&signed_in("REFRESH-SENTINEL-0")).unwrap();
    // The store can't be written (a folder where the file would go).
    let good_store = places.store.clone();
    places.store = Store::File(dir.join("no-such-dir").join("x").join("sign-in.json"));
    std::fs::create_dir_all(dir.join("no-such-dir")).unwrap();
    std::fs::write(dir.join("no-such-dir").join("x"), b"a file, not a folder").unwrap();
    let mut secrets = Some(signed_in("REFRESH-SENTINEL-0"));
    let mut unsaved = false;
    let failure = access_token(&places, &mut secrets, &mut unsaved, false).unwrap_err();
    assert!(failure.words().contains("works only here"), "{}", failure.words());
    no_sentinel(&failure.words());
    assert!(unsaved);
    assert_eq!(secrets.as_ref().map(|s| s.refresh_token.as_str()), Some("REFRESH-SENTINEL-1"), "held here");
    // With the store working again, the held sign-in is kept before anything is asked.
    places.store = good_store;
    let token = access_token(&places, &mut secrets, &mut unsaved, false).unwrap();
    assert_eq!(token, "ACCESS-SENTINEL-1");
    assert!(!unsaved);
    assert_eq!(places.store.load().unwrap().map(|s| s.refresh_token), Some("REFRESH-SENTINEL-1".to_owned()));
    assert_eq!(canva.canva.lock().unwrap().given, 1, "no second refresh");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_old_scripts_sign_in_is_brought_in_and_its_token_file_taken_away() {
    let dir = folder("old-script");
    let places = places(&dir, 1);
    std::fs::create_dir_all(&places.old_script).unwrap();
    std::fs::write(
        places.old_script.join("creds.env"),
        format!("# the integration\nCANVA_CLIENT_ID={CLIENT_ID}\nCANVA_CLIENT_SECRET={CLIENT_SECRET}\n"),
    )
    .unwrap();
    std::fs::write(
        places.old_script.join("tokens.json"),
        r#"{"refresh_token": "REFRESH-SENTINEL-old", "access_token": "ACCESS-SENTINEL-old"}"#,
    )
    .unwrap();
    assert!(old_script_here(&places));
    // What runs it: a system job and a menu bar plugin name it; another job doesn't.
    let daemons = dir.join("jobs").join("LaunchDaemons");
    let plugins = dir.join("jobs").join("Plugins");
    std::fs::create_dir_all(&daemons).unwrap();
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(
        daemons.join("org.example.canva-sync.plist"),
        "<dict>\n<key>Label</key>\n  <string>org.example.canva-sync</string>\n<string>/scripts/canva_sync.py</string>",
    )
    .unwrap();
    std::fs::write(daemons.join("org.example.other.plist"), "<string>/scripts/other.sh</string>").unwrap();
    std::fs::write(plugins.join("sync.5s.py"), "STATUS = 'canva-sync/status.json'").unwrap();
    let jobs = old_jobs(&places);
    assert_eq!(jobs.len(), 2, "{jobs:?}");
    let steps = switch_off_steps(&places, &jobs);
    assert!(steps.contains(&"sudo launchctl bootout 'system/org.example.canva-sync'".to_owned()), "{steps:?}");
    assert!(steps.iter().any(|step| step.starts_with("rm '") && step.ends_with("sync.5s.py'")), "{steps:?}");
    assert!(steps.last().unwrap().contains("old-script"), "{steps:?}");
    let imported = import_old(&places).expect("brought in");
    assert!(!imported.token_file_left && imported.notes.is_empty());
    let secrets = imported.secrets;
    assert_eq!((secrets.client_id.as_str(), secrets.refresh_token.as_str()), (CLIENT_ID, "REFRESH-SENTINEL-old"));
    assert_eq!(secrets.access_token, None, "its access token isn't trusted: it is refreshed when needed");
    assert_eq!(places.store.load().unwrap(), Some(secrets), "kept, and read back");
    assert!(!places.old_script.join("tokens.json").exists(), "the old script can't refresh it any more");
    assert!(places.old_script.join("creds.env").exists(), "the rest is left for the operator");
    assert!(!old_script_here(&places));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_pages_come_with_their_pictures_and_a_design_without_page_ids_is_refused() {
    let dir = folder("pages");
    let canva = fake(Canva { access: "ACCESS-SENTINEL-0".to_owned(), ..usual() });
    let places = places(&dir, canva.port);
    let listed = pages(&places, "ACCESS-SENTINEL-0", DESIGN).unwrap();
    assert_eq!(listed.len(), 5);
    let picture = thumbnail(&places, &listed[0]).expect("its small picture");
    assert_eq!((picture.width, picture.height), (4, 4));
    let wrong = pages(&places, "nonsense", DESIGN).unwrap_err();
    no_sentinel(&wrong.words());
    canva.canva.lock().unwrap().pages = vec![(1, None)];
    assert!(pages(&places, "ACCESS-SENTINEL-0", DESIGN).unwrap()[0].id.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_connection_ends_a_sync_it_cant_start_and_wont_forget_a_sign_in_being_kept() {
    let dir = folder("client");
    let canva = fake(Canva { polls_before_done: 10_000, ..usual() });
    let mut places = places(&dir, canva.port);
    places.settings = dir.join("canva-sync.json");
    // Asked for while the kept sign-in is read (there is none): the sync ends, as a failure.
    let mut client = Client::new(places.clone(), None);
    client.sync_now().expect("waits for the sign-in to be read");
    let until = Instant::now() + Duration::from_secs(5);
    while client.finished.is_none() && Instant::now() < until {
        client.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(&client.finished, Some(Err(why)) if why.contains("isn't signed in")), "{:?}", client.finished);
    // Signed in, a sync running: Disconnect waits until it is stopped.
    places.store.save(&signed_in("REFRESH-SENTINEL-0")).unwrap();
    Settings { design: Some(DESIGN.to_owned()), divider: Some("divider".to_owned()), ..Settings::default() }
        .save(&places.settings)
        .unwrap();
    let mut client = Client::new(places.clone(), None);
    while client.status == Status::Reading {
        client.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    client.sync_now().expect("a sync starts");
    std::thread::sleep(Duration::from_millis(150));
    client.tick();
    assert!(client.phase.busy());
    assert!(client.disconnect().unwrap_err().contains("busy"));
    client.stop();
    let until = Instant::now() + Duration::from_secs(5);
    while client.phase.busy() && Instant::now() < until {
        client.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    client.disconnect().expect("forgotten once stopped");
    assert_eq!(places.store.load().unwrap(), None, "and it stays forgotten");
    assert_eq!(client.status, Status::NotConnected);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn only_plain_http_to_127_0_0_1_and_a_port_is_a_stand_in() {
    for good in ["http://127.0.0.1:8977", "http://127.0.0.1:8977/", " http://127.0.0.1:1/api "] {
        assert!(is_stand_in(good), "{good}");
    }
    for bad in [
        "http://127.0.0.1:1@evil.com",
        "http://127.0.0.1:80.evil.com",
        "http://127.0.0.1:8080@x",
        "http://127.0.0.1.evil.com:1",
        "http://127.0.0.1:1?x@evil.com",
        "http://127.0.0.1:",
        "http://127.0.0.1:0",
        "http://127.0.0.1:123456",
        "http://127.0.0.1:99999",
        "https://127.0.0.1:443",
        "http://localhost:80",
    ] {
        assert!(!is_stand_in(bad), "{bad}");
    }
}

/// The old script's folder with its sign-in in it.
fn old_script(places: &Places) {
    std::fs::create_dir_all(&places.old_script).unwrap();
    std::fs::write(
        places.old_script.join("creds.env"),
        format!("CANVA_CLIENT_ID={CLIENT_ID}\nCANVA_CLIENT_SECRET={CLIENT_SECRET}\n"),
    )
    .unwrap();
    std::fs::write(places.old_script.join("tokens.json"), r#"{"refresh_token": "REFRESH-SENTINEL-old"}"#).unwrap();
}

#[test]
fn the_import_holds_the_old_scripts_lock_and_refuses_while_it_runs() {
    let dir = folder("old-lock");
    let places = places(&dir, 1);
    old_script(&places);
    // The script running holds its lock: nothing is read or kept.
    let held = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(places.old_script.join("lock"))
        .unwrap();
    held.lock().unwrap();
    let why = import_old(&places).err().expect("refused while the script runs");
    assert!(why.contains("running right now"), "{why}");
    assert_eq!(places.store.load().unwrap(), None);
    drop(held);
    // A lock that can't be opened is a refusal too.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(places.old_script.join("lock"), std::fs::Permissions::from_mode(0o000)).unwrap();
        let why = import_old(&places).err().expect("refused");
        assert!(why.contains("couldn't be opened"), "{why}");
        std::fs::set_permissions(places.old_script.join("lock"), std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    // A half-saved token file is left alone, and said.
    std::fs::write(places.old_script.join("tokens.json.tmp"), r#"{"refresh_token": "REFRESH-SENTINEL-newer"}"#)
        .unwrap();
    let imported = import_old(&places).expect("brought in");
    assert_eq!(imported.secrets.refresh_token, "REFRESH-SENTINEL-old", "the half-saved file isn't read");
    assert!(imported.notes.iter().any(|note| note.contains("tokens.json.tmp")), "{:?}", imported.notes);
    assert!(places.old_script.join("tokens.json.tmp").exists());
    for note in &imported.notes {
        no_sentinel(note);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn a_token_file_that_cant_be_taken_away_holds_syncs_until_the_script_is_switched_off() {
    use std::os::unix::fs::PermissionsExt;
    let dir = folder("old-left");
    let mut places = places(&dir, 1);
    places.settings = dir.join("canva-sync.json");
    old_script(&places);
    std::fs::write(places.old_script.join("lock"), b"").unwrap();
    // The folder can't be changed, so its token file can't be removed.
    std::fs::set_permissions(&places.old_script, std::fs::Permissions::from_mode(0o555)).unwrap();
    let imported = import_old(&places);
    std::fs::set_permissions(&places.old_script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let imported = imported.expect("brought in all the same");
    assert!(imported.token_file_left);
    assert!(imported.notes.iter().any(|note| note.contains("couldn't be taken away")), "{:?}", imported.notes);
    assert!(places.old_script.join("tokens.json").exists());
    // The connection then syncs nothing until It's Switched Off.
    Settings { design: Some(DESIGN.to_owned()), divider: Some("divider".to_owned()), ..Settings::default() }
        .save(&places.settings)
        .unwrap();
    let mut client = Client::new(places.clone(), None);
    let _ = client.tx.send(Event::Imported(Ok(imported)));
    while client.status == Status::Reading || client.phase.busy() || !client.settings.old_script_warning {
        client.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(client.settings.hold_syncs);
    assert!(client.sync_now().unwrap_err().contains("couldn't be taken away"));
    assert!(client.read_pages().is_err());
    client.old_script_off();
    assert!(!client.settings.hold_syncs);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_held_sign_in_is_kept_at_quitting_and_a_late_read_never_replaces_a_newer_one() {
    let dir = folder("shut-down");
    let places = places(&dir, 1);
    let mut client = Client::new(places.clone(), None);
    while client.status == Status::Reading {
        client.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    // A sign-in newer than the kept one, which couldn't be kept: kept at quitting.
    client.secrets = Some(signed_in("REFRESH-SENTINEL-newest"));
    client.unsaved = true;
    client.status = Status::Connected;
    // An older read of the store coming in late changes nothing.
    let _ = client.tx.send(Event::SecretsRead(Ok(Some(signed_in("REFRESH-SENTINEL-older")))));
    client.tick();
    assert_eq!(client.secrets.as_ref().map(|s| s.refresh_token.as_str()), Some("REFRESH-SENTINEL-newest"));
    client.shut_down();
    assert_eq!(places.store.load().unwrap().map(|s| s.refresh_token), Some("REFRESH-SENTINEL-newest".to_owned()));
    assert!(!client.unsaved);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn small_safeguards_hold() {
    // Lines for the Terminal quote everything, a quote too.
    assert_eq!(quoted("/Users/o'brien/Library/x y.plist"), r#"'/Users/o'\''brien/Library/x y.plist'"#);
    assert_eq!(
        plist_label(b"<key>Label</key>\n\t<string>org.example.job</string>"),
        Some("org.example.job".to_owned())
    );
    assert_eq!(plist_label(b"<key>Label</key><string>bad; rm -rf /</string>"), None);
    assert_eq!(plist_label(b"<string>no label</string>"), None);
    // A page's signed picture address is never printed.
    let page = Page { number: 3, id: Some("p3".to_owned()), thumbnail: Some("https://x/?sig=SENTINEL".to_owned()) };
    no_sentinel(&format!("{page:?}"));
    // A design id kept is checked as a typed one is.
    let dir = folder("safeguards");
    let file = dir.join("canva-sync.json");
    std::fs::write(&file, r#"{"design": "DA../../x?y", "made": ["ok.mp4", "../up.mp4", ".hidden.mp4", "a/b.mp4"]}"#)
        .unwrap();
    let settings = Settings::load(&file);
    assert_eq!(settings.design, None);
    assert_eq!(settings.made, ["ok.mp4"]);
    // The sign-in file is new and private, whatever was left at its name.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let target = dir.join("elsewhere");
        std::fs::write(&target, b"someone else's").unwrap();
        let part = dir.join("sign-in.json.part");
        std::os::unix::fs::symlink(&target, &part).unwrap();
        write_private(&part, b"{}").unwrap();
        let meta = std::fs::symlink_metadata(&part).unwrap();
        assert!(meta.is_file() && meta.permissions().mode() & 0o777 == 0o600);
        assert_eq!(std::fs::read(&target).unwrap(), b"someone else's", "not written through the link");
    }
    let _ = std::fs::remove_dir_all(dir);
}
