//! Tests of the anonymous reports: that what the app makes has exactly the format's shape (the
//! fixtures the server's own test reads), that what the fixtures say must be refused is left
//! out by the app before it would ever be sent, that nothing is written or sent before the
//! person has been told, and that off stops at once. Sending goes to a server on this computer
//! (no network beyond 127.0.0.1).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use serde_json::{Value, json};

use super::format::{About, Collector, CrashRecord};
use super::stats::{Count, Taken};
use super::*;

const FIXTURES: &str = include_str!("../testdata/anonymous-report-fixtures.json");

/// The counters are the program's own, one set for every test: tests that count take turns,
/// with the counting on.
pub fn counting() -> std::sync::MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    let guard = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    stats::switch_on();
    guard
}

fn fixtures() -> Value {
    serde_json::from_str(FIXTURES).expect("the fixtures are JSON")
}

fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "sp-reports-{name}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn gunzip(body: &[u8]) -> Value {
    serde_json::from_reader(flate2::read::GzDecoder::new(body)).expect("a report is gzipped JSON")
}

fn scrubber() -> Scrubber {
    Scrubber::new(Some("/Users/janesmith"), Some("janesmith"))
}

/// A fixture's invalid report: the valid one it names, with the value at its path replaced or
/// removed.
fn spoiled(fixtures: &Value, case: &Value) -> Value {
    let mut report = fixtures["valid"][case["from"].as_str().unwrap()].clone();
    let path = case["path"].as_array().unwrap();
    let (last, parents) = path.split_last().unwrap();
    let mut at = &mut report;
    for step in parents {
        at = match step {
            Value::String(key) => &mut at[key.as_str()],
            step => &mut at[step.as_u64().unwrap() as usize],
        };
    }
    let remove = case["remove"].as_bool().unwrap_or(false);
    match last {
        Value::String(key) if remove => {
            at.as_object_mut().unwrap().remove(key);
        }
        Value::String(key) => at[key.as_str()] = case["value"].clone(),
        step => at[step.as_u64().unwrap() as usize] = case["value"].clone(),
    }
    report
}

// ---------------------------------------------------------------------------------------------
// The format

#[test]
fn the_check_takes_the_valid_fixtures_and_refuses_every_invalid_one() {
    let fixtures = fixtures();
    for (name, report) in fixtures["valid"].as_object().unwrap() {
        assert_eq!(check::report(report), Ok(()), "{name} must be taken");
    }
    let cases = fixtures["invalid"].as_array().unwrap();
    assert!(cases.len() >= 50, "the fixtures hold the reports to refuse");
    for case in cases {
        let report = spoiled(&fixtures, case);
        let said = check::report(&report);
        assert!(said.is_err(), "{} must be refused", case["name"]);
        // What is wrong is said without repeating what was sent.
        if let Some(value) = case["value"].as_str().filter(|value| value.len() > 8) {
            assert!(!said.unwrap_err().contains(value), "{}", case["name"]);
        }
    }
}

/// Every key the app's report has, at any depth, as `minutes[].cpu.app`.
fn keys(value: &Value, path: &str, out: &mut std::collections::BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, inner) in object {
                // The names of features are codes, not fields.
                let here = if path == "features" { "features.*".to_owned() } else { format!("{path}.{key}") };
                out.insert(here.trim_start_matches('.').to_owned());
                keys(inner, here.trim_start_matches('.'), out);
            }
        }
        Value::Array(items) => {
            for inner in items {
                keys(inner, &format!("{path}[]"), out);
            }
        }
        _ => {}
    }
}

fn hardware() -> info::Hardware {
    info::Hardware {
        os: "windows".to_owned(),
        // A name the format doesn't take as it is: the app cleans it.
        os_version: "Windows 11 Pro «Церковь» (build 26100)".to_owned(),
        arch: "x86_64".to_owned(),
        cpu_model: "Intel(R) Core(TM) i7-12700 CPU @ 2.10GHz ".repeat(4),
        cores: 20,
        ram_mb: 16_258,
    }
}

fn facts() -> Facts {
    Facts {
        view: "service",
        displays: vec![
            facts::DisplayFact { w: 1920, h: 1080, scale: 1.0, hz: Some(60.0), main: true },
            facts::DisplayFact { w: 3840, h: 2160, scale: 1.5, hz: Some(59.94), main: false },
        ],
        outputs: facts::OutputCounts { audience: 1, stage: 1, on: 2, display: 1, window: 0, ndi: 1, decklink: 0 },
        ndi: true,
        decklink: false,
        language: "ru",
        scene: vec!["video", "player_ffmpeg", "song"],
        video_size: Some((1920, 1080, 29.97)),
        video_file: Some(PathBuf::from("/Users/janesmith/Movies/Easter Service.mp4")),
    }
}

fn gpu() -> GpuFacts {
    GpuFacts {
        name: "Intel(R) UHD Graphics 770".to_owned(),
        backend: "Dx12".to_owned(),
        driver: "Intel Corporation 31.0.101.2125".to_owned(),
        device_type: "IntegratedGpu".to_owned(),
    }
}

/// A collector as full as the app can make it: more rows than the format takes, of every kind.
fn full_collector(minutes: usize) -> Collector {
    let mut collector = Collector::default();
    let clock = stats::Clock::new();
    for ms in [3, 4, 5, 70, 120] {
        clock.add(Duration::from_millis(ms));
    }
    let mut taken = Taken { pass: clock.take(), ..Taken::default() };
    for (at, count) in taken.counts.iter_mut().enumerate() {
        *count = 1000 + at as u64;
    }
    let _ = Count::Keys;
    let facts = facts();
    for minute in 0..minutes {
        for tick in 0..format::TICKS_PER_MINUTE {
            let sample = sample::Sample {
                app_cpu: 40.5 + minute as f32,
                kids_cpu: 100.25,
                system_cpu: 30.0,
                app_mb: 900.4,
                kids_mb: 200.0,
                free_mb: Some(4000.0 - tick as f32),
            };
            collector.tick(f64::from(tick) * 5.0, 1_795_600_000 + minute as i64 * 60, Some(sample), &taken, &facts);
        }
    }
    let events = (0..260)
        .map(|at| stats::Event {
            t: at as f64 * 1.37,
            kind: ["slow_pass", "freeze_window", "display_added", "notice", "problem"][at % 5],
            n: (at % 2 == 0).then_some(at as f64),
            ms: (at % 3 == 0).then_some(61.54),
        })
        // A code the format would refuse never reaches a report.
        .chain([stats::Event { t: 1.0, kind: "Amazing Grace", n: None, ms: None }])
        .collect();
    collector.take_events(events);
    collector.take_features((0..130).map(|at| (format!("menu_tool_{at}"), at)).collect());
    collector.take_features(vec![("Amazing Grace".to_owned(), 1), ("view_bible".to_owned(), 7)]);
    collector.take_problems(
        (0..30).map(|at| format!("[video] the decoder stopped: error -{at} after a long wait for a picture")).collect(),
    );
    collector
}

fn about<'a>(hardware: &'a info::Hardware, gpu: &'a GpuFacts, facts: &'a Facts) -> About<'a> {
    About {
        install_id: "5f0c1e7a9b3d4c2e8a6f0b1d2c3e4f5a",
        session: "0a1b2c3d4e5f60718293a4b5c6d7e8f9",
        hardware,
        gpu: Some(gpu),
        facts: Some(facts),
        ffmpeg: true,
        uptime_s: 905,
        now_unix: 1_795_608_000,
    }
}

#[test]
fn what_the_app_makes_has_the_fixtures_shape_and_passes_the_check() {
    let _guard = counting();
    let fixtures = fixtures();
    let (hardware, gpu, facts) = (hardware(), gpu(), facts());
    let mut collector = full_collector(40);
    let report = format::running(&about(&hardware, &gpu, &facts), &mut collector, true);
    let made: Value = serde_json::from_str(&format::json(&report)).unwrap();
    assert_eq!(check::report(&made), Ok(()), "{made:#}");
    // Every field the app sends is one the valid fixture has: the app adds nothing of its own.
    let (mut ours, mut theirs) = (Default::default(), Default::default());
    keys(&made, "", &mut ours);
    keys(&fixtures["valid"]["full"], "", &mut theirs);
    let extra: Vec<&String> = ours.difference(&theirs).collect();
    assert!(extra.is_empty(), "the app sends fields the format's fixture doesn't have: {extra:?}");
    // And every field the fixture has, the app can fill.
    let missing: Vec<&String> = theirs.difference(&ours).filter(|key| !key.ends_with(".id")).collect();
    assert!(missing.is_empty(), "the app never fills {missing:?}");
    // The lists are within the format's limits, and what went is counted.
    assert_eq!(made["minutes"].as_array().unwrap().len(), check::MINUTES);
    assert_eq!(made["events"].as_array().unwrap().len(), check::EVENTS);
    assert_eq!(made["features"].as_object().unwrap().len(), check::FEATURES);
    assert_eq!(made["problems"].as_array().unwrap().len(), check::PROBLEMS);
    assert_eq!(made["dropped"], json!({ "minutes": 10, "events": 60, "problems": 10 }));
    // Names the format doesn't take as they are were cleaned or left out, never sent as they were.
    assert_eq!(made["os"]["version"], json!("Windows 11 Pro (build 26100)"));
    assert_eq!(made["cpu"]["model"].as_str().unwrap().len(), 80);
    assert_eq!(made["gpu"]["backend"], json!("dx12"));
    assert_eq!(made["gpu"]["type"], json!("integrated"));
    assert_eq!(made["ram_gb"], json!(16));
    assert_eq!(made["minutes"][0]["scene"], json!(["video_1080p30", "player_ffmpeg", "song"]));
    let text = made.to_string();
    for never in ["Amazing", "Easter", "janesmith", "Movies", "Церковь"] {
        assert!(!text.contains(never), "{never} is in the report");
    }
    // Taken: the next report starts empty.
    let next = format::running(&about(&hardware, &gpu, &facts), &mut collector, true);
    assert!(next.minutes.is_empty() && next.events.is_empty() && next.problems.is_empty());
}

#[test]
fn a_report_is_made_to_fit_what_the_server_takes() {
    let _guard = counting();
    let (hardware, gpu, facts) = (hardware(), gpu(), facts());
    let mut collector = full_collector(40);
    let mut report = format::running(&about(&hardware, &gpu, &facts), &mut collector, false);
    // Rows that don't compress: every number different.
    for (at, row) in report.minutes.iter_mut().enumerate() {
        let odd = at as f64 * 7.3;
        row.cpu.app = [odd, odd * 1.7];
        row.mem_mb.app = [900.0 + odd * 3.1, 1000.0 + odd * 5.9];
        row.ui.frames = 1000 + at as u64 * 977;
        row.screens.passes = 3600 + at as u64 * 613;
        row.screens.max = odd * 2.3;
    }
    for (at, event) in report.events.iter_mut().enumerate() {
        event.t = at as f64 * 13.77;
        event.ms = Some(at as f64 * 3.91);
        event.n = Some(at as f64 * 17.0);
    }
    let body = format::fit(report).expect("a full report can be made to fit");
    assert!(body.len() <= format::MAX_SENT, "{} bytes", body.len());
    let made = gunzip(&body);
    assert!(made.to_string().len() <= format::MAX_RAW);
    assert_eq!(check::report(&made), Ok(()));
    // A report that fails the check is never kept, whatever its size.
    let mut bad = format::running(&about(&hardware, &gpu, &facts), &mut collector, false);
    bad.problems = vec!["[media] could not open /Users/maria/Movies/intro.mp4".to_owned()];
    assert!(format::fit(bad).is_none());
}

// ---------------------------------------------------------------------------------------------
// What is left out

#[test]
fn what_the_fixtures_refuse_the_app_leaves_out_first() {
    let fixtures = fixtures();
    let scrubber = scrubber();
    let mut tried = 0;
    for case in fixtures["invalid"].as_array().unwrap() {
        let Some(value) = case["value"].as_str() else { continue };
        let path: Vec<String> =
            case["path"].as_array().unwrap().iter().map(|step| step.to_string().replace('"', "")).collect();
        let name = case["name"].as_str().unwrap();
        match path.join(".").as_str() {
            "problems.0" => {
                tried += 1;
                // As a line from an area whose problems are sent, so that only what the line
                // holds decides.
                let words = value.split_once("] ").map_or(value, |(_, words)| words);
                let kept = scrub::problem_line(&format!("[video] {words}"), &scrubber);
                if name.contains("too long") {
                    let kept = kept.expect("a long plain line is cut, not dropped");
                    assert_eq!(check::free_text(&kept, check::PROBLEM_LENGTH, false, true), Ok(()));
                } else {
                    assert_eq!(kept, None, "{name}");
                }
                // And from its own area (media, sync, remote…), whose problems are never sent.
                if value.starts_with('[') {
                    assert_eq!(scrub::problem_line(value, &scrubber), None, "{name}");
                }
            }
            "crash.message" => {
                tried += 1;
                assert_eq!(scrub::crash_message(value, &scrubber), None, "{name}");
            }
            "crash.location" => {
                tried += 1;
                // Cut down to the program's source, or left out: never sent as it was.
                if let Some(place) = scrub::location(value) {
                    assert!(check::is_location(&place) && !place.contains("Users") && !place.contains("opt"), "{name}");
                    assert_eq!(place, "crates/sp-app/src/deck.rs:412:17", "{name}");
                }
            }
            "crash.backtrace" => {
                tried += 1;
                let kept = scrub::backtrace(value).expect("the frames are kept");
                assert_eq!(kept, "0: main\n             at wgpu-27.0.1/src/lib.rs:1:1", "{name}");
            }
            "crash.thread" => {
                tried += 1;
                assert_eq!(scrub::thread(value), None, "{name}");
            }
            _ => {}
        }
    }
    assert!(tried >= 20, "the fixtures' free text was tried: {tried}");
}

#[test]
fn a_problem_line_goes_only_if_it_could_not_hold_a_name() {
    let scrubber = scrubber();
    let kept = |line: &str| scrub::problem_line(line, &scrubber);
    // About the computer and the drawing: sent as they are.
    for line in [
        "[video] the decoder stopped: error -1094995529",
        "[output] The output could not draw a frame, and keeps what it shows: the device was lost",
        "[gpu] Graphics error (3 more times in the last minute): out of memory",
        "[window] Could not draw the preview: the surface is outdated",
    ] {
        assert_eq!(kept(line).as_deref(), Some(line));
    }
    for line in [
        // Another area: its lines name songs, files and devices.
        "[catalog] Amazing Grace could not join the library: the disk is full",
        "[outside] Harbour Lights: changes not saved",
        "[remote] Could not switch Remote Control on: the port is taken",
        "[ndi] the sender stopped",
        // A quotation, a file's name, a path, the account's name, another script.
        "[ndi] NDI \"Sanctuary\" stood still: 2 s between two new pictures",
        "[video] could not open intro.mp4",
        "[video] could not open Movies/intro",
        "[video] could not open ~/Movies",
        "[video] janesmith has no room left",
        "[video] не найдено",
        "[video] the file 'Easter' is missing",
        "no area at all",
    ] {
        assert_eq!(kept(line), None, "{line}");
    }
    // An apostrophe inside a word is no quotation.
    assert!(kept("[video] the new file couldn't take over: it is shorter").is_some());
}

#[test]
fn a_crash_says_where_in_the_program_and_nothing_of_the_builder_or_the_person() {
    let _guard = counting();
    let record = CrashRecord {
        message: "byte index 5 is not a char boundary; it is inside 'я' of `Благодать`".to_owned(),
        location: "/Users/builder/dev/SoftPresenter/crates/sp-app/src/deck.rs:412:17".to_owned(),
        thread: "main".to_owned(),
        backtrace: "   0: std::backtrace::Backtrace::force_capture\n   \
                    1: sp_app::reports::on_panic\n             \
                    at /Users/builder/dev/SoftPresenter/crates/sp-app/src/reports.rs:210:24\n   \
                    2: core::panicking::panic_bounds_check\n             \
                    at /rustc/17067e9ac6d7e6f9c0d6ab1e0e5d5e8f3a2b1c0d/library/core/src/panicking.rs:274:5\n   \
                    3: wgpu::api::device::Device::poll\n             \
                    at C:\\Users\\Builder\\.cargo\\registry\\src\\index.crates.io-1949cf8c6b5b557f\\wgpu-27.0.1\\src\\api\\device.rs:88:9\n   \
                    4: песня::слайд\n             \
                    at /somewhere/else/entirely.rs:1:1\n   \
                    5: vello::render\n             \
                    at ~/dev/SoftPresenter/third_party/vello/src/render.rs:7:3"
            .to_owned(),
        uptime_s: 1320,
    };
    let (hardware, gpu, facts) = (hardware(), gpu(), facts());
    let report = format::crash(&about(&hardware, &gpu, &facts), &record, &scrubber());
    let made: Value = serde_json::from_str(&format::json(&report)).unwrap();
    assert_eq!(check::report(&made), Ok(()), "{made:#}");
    let crash = &made["crash"];
    // The message quoted a title: it is left out whole.
    assert!(crash.get("message").is_none());
    assert_eq!(crash["location"], json!("crates/sp-app/src/deck.rs:412:17"));
    assert_eq!(
        crash["backtrace"].as_str().unwrap(),
        "   0: std::backtrace::Backtrace::force_capture\n   \
         1: sp_app::reports::on_panic\n             \
         at crates/sp-app/src/reports.rs:210:24\n   \
         2: core::panicking::panic_bounds_check\n             \
         at /rustc/17067e9ac6d7e6f9c0d6ab1e0e5d5e8f3a2b1c0d/library/core/src/panicking.rs:274:5\n   \
         3: wgpu::api::device::Device::poll\n             \
         at wgpu-27.0.1/src/api/device.rs:88:9\n   \
         5: vello::render\n             \
         at third_party/vello/src/render.rs:7:3"
    );
    // Every field of a crash is one the fixture has.
    let fixtures = fixtures();
    let (mut ours, mut theirs) = (Default::default(), Default::default());
    keys(&made, "", &mut ours);
    keys(&fixtures["valid"]["crash"], "", &mut theirs);
    keys(&fixtures["valid"]["full"], "", &mut theirs);
    assert!(ours.is_subset(&theirs), "{:?}", ours.difference(&theirs).collect::<Vec<_>>());
    assert!(format::fit(report).is_some());
    // Rust's own messages go as they are; one that quotes anything else does not.
    let scrubber = scrubber();
    for message in [
        "called `Option::unwrap()` on a `None` value",
        "index out of bounds: the len is 3 but the index is 7",
        "attempt to subtract with overflow",
    ] {
        assert_eq!(scrub::crash_message(message, &scrubber).as_deref(), Some(message));
    }
    for message in ["no song called \"Grace\"", "could not read `Grace`", "bad file intro.mp4", ""] {
        assert_eq!(scrub::crash_message(message, &scrubber), None, "{message}");
    }
    assert_eq!(
        scrub::location("/rustc/17067e9ac6d7e6f9c0d6ab1e0e5d5e8f3a2b1c0d/library/core/src/panicking.rs:274:5")
            .as_deref(),
        Some("/rustc/17067e9ac6d7e6f9c0d6ab1e0e5d5e8f3a2b1c0d/library/core/src/panicking.rs:274:5")
    );
    assert_eq!(scrub::location("src/main.rs:1:1"), None);
    assert_eq!(scrub::hardware_name("  Apple M2 «Pro»\u{7}  "), Some("Apple M2 Pro".to_owned()));
    assert_eq!(scrub::hardware_name("«»"), None);
}

// ---------------------------------------------------------------------------------------------
// Whether, and where

#[test]
fn a_test_copy_never_sends_and_the_switch_reaches_only_this_computer() {
    let with = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| pairs.iter().find(|(have, _)| *have == name).map(|(_, value)| (*value).to_owned())
    };
    // A person's own copy: the project's server, over https.
    let real = environment(false, with(&[])).expect("a real copy sends");
    assert_eq!((real.url.as_str(), real.local), ("https://reports.softpresenter.com/v1/public/report", false));
    // Test copies: quiet, with a data folder of their own, or only saving a picture.
    assert_eq!(environment(false, with(&[("SOFTPRESENTER_QUIET", "1")])), None);
    assert_eq!(environment(false, with(&[("SOFTPRESENTER_DATA", "/tmp/x")])), None);
    assert_eq!(environment(true, with(&[])), None);
    // The testing switch: only with a data folder of its own, and only to this computer.
    let local = &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://127.0.0.1:8791/"), ("SOFTPRESENTER_DATA", "/tmp/x")];
    let target = environment(false, with(local)).expect("the test switch sends to this computer");
    assert_eq!((target.url.as_str(), target.local), ("http://127.0.0.1:8791/v1/public/report", true));
    let quiet = &[
        ("SOFTPRESENTER_REPORTS_TEST_URL", "http://localhost:8791"),
        ("SOFTPRESENTER_DATA", "/tmp/x"),
        ("SOFTPRESENTER_QUIET", "1"),
    ];
    assert!(environment(false, with(quiet)).is_some_and(|target| target.local));
    for refused in [
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://127.0.0.1:8791")][..],
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "https://reports.example"), ("SOFTPRESENTER_DATA", "/tmp/x")],
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://127.0.0.1.example:80"), ("SOFTPRESENTER_DATA", "/tmp/x")],
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://reports.example"), ("SOFTPRESENTER_DATA", "/tmp/x")],
        // The host is what follows the @: elsewhere, not this computer.
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://127.0.0.1:1@reports.example"), ("SOFTPRESENTER_DATA", "/tmp/x")],
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://localhost@reports.example"), ("SOFTPRESENTER_DATA", "/tmp/x")],
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://127.0.0.1:80/elsewhere"), ("SOFTPRESENTER_DATA", "/tmp/x")],
        &[("SOFTPRESENTER_REPORTS_TEST_URL", "http://127.0.0.1:"), ("SOFTPRESENTER_DATA", "/tmp/x")],
    ] {
        let pairs: Vec<(String, String)> = refused.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect();
        let var = move |name: &str| pairs.iter().find(|(have, _)| have == name).map(|(_, value)| value.clone());
        // A switch that can't be honoured sends nowhere: never to the project's server instead.
        assert_eq!(environment(false, var), None, "{refused:?}");
    }
}

#[test]
fn no_choice_is_kept_until_one_is_made_and_nobody_has_been_told_yet() {
    let root = folder("saved");
    // No file: no choice made (reports start as they do where the computer is), not told, a
    // random id.
    let first = read_saved(&root);
    assert!(first.enabled.is_none() && !first.seen && check::is_hex32(&first.install_id));
    // The id is random each time until it is kept, and never made from the computer.
    assert_ne!(first.install_id, read_saved(&root).install_id);
    write_saved(&root, &Saved { enabled: Some(false), seen: true, install_id: first.install_id.clone() });
    assert_eq!(read_saved(&root), Saved { enabled: Some(false), seen: true, install_id: first.install_id });
    // A file that can't be read: no choice, and not told, so the person is told again first.
    std::fs::write(root.join("settings.json"), "{ not json").unwrap();
    let broken = read_saved(&root);
    assert!(broken.enabled.is_none() && !broken.seen);
    let _ = std::fs::remove_dir_all(root);
}

// ---------------------------------------------------------------------------------------------
// Sending

struct Received {
    headers: std::collections::HashMap<String, String>,
    body: Vec<u8>,
}

/// A server on this computer that answers each request with the next status in `replies`.
fn serve(replies: Vec<u16>) -> (String, Arc<Mutex<Vec<Received>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}{ROUTE}", listener.local_addr().unwrap().port());
    let got = Arc::new(Mutex::new(Vec::new()));
    let kept = got.clone();
    std::thread::spawn(move || {
        for status in replies {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut headers = std::collections::HashMap::new();
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.trim_end().split_once(':') {
                    headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
                }
            }
            let length: usize = headers.get("content-length").and_then(|n| n.parse().ok()).unwrap_or(0);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            kept.lock().unwrap().push(Received { headers, body });
            let mut stream = stream;
            let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}");
        }
    });
    (url, got)
}

fn reporter(url: Option<&str>, root: &Path, enabled: bool, seen: bool) -> Reporter {
    let saved = Saved { enabled: Some(enabled), seen, install_id: random_hex() };
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            enabled,
            chosen: Some(enabled),
            starts_on: Some(true),
            seen,
            install_id: saved.install_id.clone(),
            ..State::default()
        }),
        wake: Condvar::new(),
        target: url.map(|url| Target { url: url.to_owned(), local: true }),
        folder: root.to_path_buf(),
    });
    let mut reporter = Reporter::new(shared, saved);
    // The tests don't wait out the first minute.
    reporter.grace = Duration::ZERO;
    reporter.told_before = false;
    reporter.hardware = Some(hardware());
    reporter.ffmpeg = Some(false);
    reporter
}

fn waiting(root: &Path) -> usize {
    queue_files(&queue_folder(root)).len() + crash_files(root).len()
}

#[test]
fn nothing_is_counted_written_or_sent_before_the_person_has_been_told() {
    let _guard = counting();
    let root = folder("untold");
    let (url, got) = serve(vec![201; 4]);
    stats::switch_off();
    let mut reporter = reporter(Some(&url), &root, true, false);
    reporter.grace = NOT_BEFORE;
    for _ in 0..3 {
        reporter.round(false);
    }
    // Quitting before being told sends nothing either.
    reporter.round(true);
    assert!(!stats::is_on(), "nothing is counted before the person has been told");
    assert!(!ACTIVE.load(Ordering::Relaxed), "a crash writes nothing down before then");
    assert_eq!((got.lock().unwrap().len(), waiting(&root)), (0, 0));
    // Told (the setup's step passed, or the notice shown): the next round counts, and a report goes.
    lock(&reporter.shared).seen = true;
    reporter.round(false);
    assert!(stats::is_on() && ACTIVE.load(Ordering::Relaxed));
    // ...but not within the first minute after being told.
    assert_eq!(got.lock().unwrap().len(), 0, "nothing goes in the first minute");
    reporter.not_before = Instant::now();
    reporter.round(false);
    let received = got.lock().unwrap();
    assert_eq!(received.len(), 1);
    // Only the report: compressed JSON and the two headers saying so. No key, no install header.
    let headers = &received[0].headers;
    assert_eq!(headers.get("content-type").map(String::as_str), Some("application/json"));
    assert_eq!(headers.get("content-encoding").map(String::as_str), Some("gzip"));
    assert!(headers.keys().all(|name| !name.starts_with("x-sp") && name != "cookie" && name != "authorization"));
    let report = gunzip(&received[0].body);
    assert_eq!(check::report(&report), Ok(()), "{report:#}");
    assert_eq!(report["kind"], json!("report"));
    assert_eq!(report["install_id"], json!(reporter.saved.install_id));
    assert_eq!(waiting(&root), 0, "what was sent no longer waits");
    drop(received);
    ACTIVE.store(false, Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_install_told_in_an_earlier_run_starts_counting_with_nothing_asked() {
    let _guard = counting();
    stats::switch_off();
    let root = folder("told");
    let reporter = reporter(None, &root, true, true);
    // Nowhere to send (a test copy), so nothing is counted however long it runs...
    let shared = reporter.shared.clone();
    let thread = std::thread::spawn(move || reporter.run());
    std::thread::sleep(Duration::from_millis(200));
    assert!(!stats::is_on());
    {
        let mut state = lock(&shared);
        state.quit = true;
        shared.wake.notify_all();
    }
    thread.join().unwrap();
    // ...and with somewhere to send, its thread takes its first round at once: it was told
    // before, so it counts from the start without the window asking for anything.
    let reporter = self::reporter(Some("http://127.0.0.1:9/v1/public/report"), &root, true, true);
    let shared = reporter.shared.clone();
    let thread = std::thread::spawn(move || reporter.run());
    let began = Instant::now();
    while !stats::is_on() && began.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(stats::is_on() && ACTIVE.load(Ordering::Relaxed), "the reporter never took its first round");
    {
        let mut state = lock(&shared);
        state.enabled = false;
        state.config_changed = true;
        state.quit = true;
        shared.wake.notify_all();
    }
    thread.join().unwrap();
    assert!(!stats::is_on() && waiting(&root) == 0);
    let _ = std::fs::remove_dir_all(root);
}

/// A reporter for someone who has made no choice, on a computer where reports start on or off,
/// run on its own thread until it has taken its first round; then asked to quit.
fn with_no_choice(starts_on: bool, seen: bool, root: &Path, then: impl FnOnce(&Arc<Shared>)) -> Saved {
    let shared = Arc::new(Shared {
        state: Mutex::new(State { starts_on: Some(starts_on), seen, install_id: random_hex(), ..State::default() }),
        wake: Condvar::new(),
        target: Some(Target { url: "http://127.0.0.1:9/v1/public/report".to_owned(), local: true }),
        folder: root.to_path_buf(),
    });
    let saved = Saved { enabled: None, seen, install_id: lock(&shared).install_id.clone() };
    let reporter = Reporter::new(shared.clone(), saved);
    let thread = std::thread::spawn(move || reporter.run());
    std::thread::sleep(Duration::from_millis(150));
    then(&shared);
    std::thread::sleep(Duration::from_millis(150));
    {
        let mut state = lock(&shared);
        state.enabled = false;
        state.config_changed = true;
        state.quit = true;
        shared.wake.notify_all();
    }
    thread.join().unwrap();
    read_saved(root)
}

#[test]
fn where_reports_start_off_nothing_is_on_until_the_person_turns_it_on() {
    let _guard = counting();
    stats::switch_off();
    // The EU, the EEA and the UK: off, though the person has been "told" nothing (there is
    // nothing to tell), and no notice is owed.
    let root = folder("starts-off");
    with_no_choice(false, false, &root, |shared| {
        let state = lock(shared);
        assert!(!state.enabled && state.chosen.is_none());
        assert!(!(state.starts_on.is_some() && state.enabled && !state.seen), "no notice is owed");
        assert!(!stats::is_on() && !ACTIVE.load(Ordering::Relaxed));
    });
    assert_eq!(waiting(&root), 0);
    // Even one marked as told (an old file) stays off there with no choice made.
    with_no_choice(false, true, &root, |shared| {
        assert!(!lock(shared).enabled && !stats::is_on());
    });
    // Passing the setup's step with the switch left off keeps Off as the person's choice, so
    // taking the computer elsewhere later turns nothing on.
    let mut state = State { starts_on: Some(false), ..State::default() };
    assert!(state.confirm());
    assert_eq!((state.enabled, state.chosen, state.seen), (false, Some(false), true));
    // Everywhere else: on once the person has been told, and the notice is owed until then.
    let root = folder("starts-on");
    with_no_choice(true, false, &root, |shared| {
        let state = lock(shared);
        assert!(state.enabled && state.chosen.is_none());
        assert!(state.starts_on.is_some() && state.enabled && !state.seen, "the notice is owed");
        assert!(!stats::is_on(), "and nothing is counted before it is shown");
    });
    let mut state = State { starts_on: Some(true), enabled: true, ..State::default() };
    assert!(state.confirm());
    assert_eq!((state.enabled, state.chosen, state.seen), (true, Some(true), true));
    // Before the computer has said where it is, passing the step keeps nothing.
    assert!(!State::default().confirm());
    // A choice made always wins over how reports start: On chosen where they start off...
    let root = folder("chosen");
    write_saved(&root, &Saved { enabled: Some(true), seen: true, install_id: random_hex() });
    let saved = read_saved(&root);
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            enabled: saved.enabled.unwrap_or(false),
            chosen: saved.enabled,
            starts_on: Some(false),
            seen: saved.seen,
            ..State::default()
        }),
        wake: Condvar::new(),
        target: Some(Target { url: "http://127.0.0.1:9/v1/public/report".to_owned(), local: true }),
        folder: root.clone(),
    });
    let mut reporter = Reporter::new(shared, saved);
    reporter.round(false);
    assert!(reporter.active && stats::is_on(), "On, chosen, is on wherever the computer is");
    // ...and the choice is what is kept, not how reports start.
    lock(&reporter.shared).enabled = false;
    reporter.round(false);
    assert_eq!(read_saved(&root).enabled, Some(true), "only a choice changes what is kept");
    ACTIVE.store(false, Ordering::Relaxed);
    for root in [root, folder("x")] {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn off_stops_at_once_and_deletes_what_waited() {
    let _guard = counting();
    let root = folder("off");
    // The server is away: reports wait on disk.
    let mut reporter = reporter(Some("http://127.0.0.1:9/v1/public/report"), &root, true, true);
    reporter.round(false);
    std::fs::write(root.join("crash-1.json"), serde_json::to_string(&CrashRecord::default()).unwrap()).unwrap();
    assert!(waiting(&root) >= 2, "a report and a crash wait");
    stats::event("problem");
    // Turned off: the queue, the crash and what was counted all go, and nothing is counted on.
    lock(&reporter.shared).enabled = false;
    reporter.round(false);
    assert_eq!(waiting(&root), 0);
    assert!(!stats::is_on() && !ACTIVE.load(Ordering::Relaxed));
    assert!(stats::take_events().is_empty());
    // And nothing goes at quit.
    let (url, got) = serve(vec![201; 2]);
    let mut reporter = self::reporter(Some(&url), &root, false, true);
    reporter.round(true);
    assert_eq!((got.lock().unwrap().len(), waiting(&root)), (0, 0));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_report_the_server_refuses_is_dropped_and_one_it_cant_take_now_waits() {
    let _guard = counting();
    let root = folder("refused");
    assert_eq!(outcome_of(201), Outcome::Sent);
    assert!(matches!(outcome_of(400), Outcome::Rejected(_)) && matches!(outcome_of(413), Outcome::Rejected(_)));
    for later in [408, 429, 500, 503] {
        assert!(matches!(outcome_of(later), Outcome::Failed(_)), "{later}");
    }
    // Refused: dropped, never tried again.
    let (url, got) = serve(vec![400, 201]);
    let mut reporter = reporter(Some(&url), &root, true, true);
    reporter.round(false);
    assert_eq!((got.lock().unwrap().len(), waiting(&root)), (1, 0));
    // Too many, or the store is full: it waits, and the next try is put off.
    let (url, got) = serve(vec![429, 201]);
    let mut reporter = self::reporter(Some(&url), &root, true, true);
    reporter.round(false);
    assert_eq!((got.lock().unwrap().len(), waiting(&root)), (1, 1));
    assert!(reporter.backoff.is_some());
    // A test copy (nowhere to send) keeps nothing and counts nothing.
    let mut quiet = self::reporter(None, &folder("quiet"), true, true);
    quiet.round(true);
    assert!(!quiet.active && waiting(&quiet.shared.folder) == 0);
    ACTIVE.store(false, Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_crash_is_written_down_only_while_reports_may_go_and_sent_cleaned() {
    let _guard = counting();
    let root = folder("crash");
    let (url, got) = serve(vec![201; 3]);
    let mut reporter = reporter(Some(&url), &root, true, true);
    reporter.round(false);
    let record = CrashRecord {
        message: "index out of bounds: the len is 3 but the index is 7".to_owned(),
        location: "/Users/janesmith/dev/app/crates/sp-app/src/deck.rs:412:17".to_owned(),
        thread: "main".to_owned(),
        backtrace:
            "   0: sp_app::deck::Deck::show\n             at /Users/janesmith/dev/app/crates/sp-app/src/deck.rs:412:17"
                .to_owned(),
        uptime_s: 12,
    };
    std::fs::write(root.join("crash-5.json"), serde_json::to_string(&record).unwrap()).unwrap();
    reporter.round(false);
    let received = got.lock().unwrap();
    let crash = received.iter().map(|sent| gunzip(&sent.body)).find(|report| report["kind"] == json!("crash"));
    let crash = crash.expect("the crash was sent");
    assert_eq!(check::report(&crash), Ok(()));
    assert_eq!(crash["crash"]["location"], json!("crates/sp-app/src/deck.rs:412:17"));
    assert!(!crash.to_string().contains("janesmith"));
    assert_eq!(waiting(&root), 0);
    drop(received);
    ACTIVE.store(false, Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn told_in_an_earlier_run_a_waiting_crash_and_the_quit_report_go_within_the_first_minute() {
    let _guard = counting();
    let record = serde_json::to_string(&CrashRecord {
        location: "crates/sp-app/src/deck.rs:1:1".to_owned(),
        ..CrashRecord::default()
    })
    .unwrap();
    // Told in this run: nothing goes in the first minute, not even a crash or at quit.
    let root = folder("first-minute");
    let (url, got) = serve(vec![201; 4]);
    let mut reporter = reporter(Some(&url), &root, true, true);
    reporter.grace = NOT_BEFORE;
    reporter.round(false);
    std::fs::write(root.join("crash-1.json"), &record).unwrap();
    reporter.round(false);
    reporter.round(true);
    assert_eq!(got.lock().unwrap().len(), 0);
    // Told in an earlier run: the crash that waited goes at once, and so does the quit report;
    // the report of how the app runs still waits for the minute.
    let mut reporter = self::reporter(Some(&url), &root, true, true);
    reporter.grace = NOT_BEFORE;
    reporter.told_before = true;
    reporter.round(false);
    let kinds = |got: &Arc<Mutex<Vec<Received>>>| -> Vec<String> {
        got.lock().unwrap().iter().map(|sent| gunzip(&sent.body)["kind"].as_str().unwrap().to_owned()).collect()
    };
    assert_eq!(kinds(&got), ["crash"], "an app that stops in its first minute still says so");
    reporter.round(true);
    assert_eq!(kinds(&got), ["crash", "report"]);
    ACTIVE.store(false, Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn see_whats_sent_shows_the_next_report_without_taking_it() {
    let _guard = counting();
    let root = folder("preview");
    let mut reporter = reporter(None, &root, true, true);
    lock(&reporter.shared).preview_asked = true;
    reporter.round(false);
    let shown = lock(&reporter.shared).preview.clone().expect("the report was made");
    let report: Value = serde_json::from_str(&shown).unwrap();
    assert_eq!(check::report(&report), Ok(()), "{report:#}");
    assert_eq!(report["install_id"], json!(reporter.saved.install_id));
    // A new id on request, kept, and the old one gone from what is shown.
    let before = reporter.saved.install_id.clone();
    {
        let mut state = lock(&reporter.shared);
        state.new_id_asked = true;
        state.preview_asked = true;
    }
    reporter.round(false);
    assert_ne!(reporter.saved.install_id, before);
    assert_eq!(read_saved(&root).install_id, reporter.saved.install_id);
    assert!(!lock(&reporter.shared).preview.clone().unwrap().contains(&before));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_queue_keeps_its_limit_and_drops_the_oldest() {
    let root = folder("queue");
    let queue = queue_folder(&root);
    std::fs::create_dir_all(&queue).unwrap();
    for at in 0..40 {
        std::fs::write(queue.join(format!("{at:016}-aaaaaa.json.gz")), vec![0u8; 10 * 1024]).unwrap();
    }
    trim_queue(&queue, QUEUE_LIMIT);
    let left = queue_files(&queue);
    assert_eq!(left.len(), 25);
    assert!(left[0].ends_with("0000000000000015-aaaaaa.json.gz"), "{:?}", left[0]);
    clear_waiting(&root);
    assert_eq!(waiting(&root), 0);
    let _ = std::fs::remove_dir_all(root);
}
