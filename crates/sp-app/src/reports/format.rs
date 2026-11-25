//! The anonymous report as it is sent: every field `docs/anonymous-reports.md` lists, and no
//! other. The report is built from these types and nothing else, so a field that is not here
//! can't be sent; views, tools, events and what is on screen are codes the program made up,
//! never words. `Collector` turns the readings taken every five seconds into a row for each
//! minute and keeps the lists within the format's limits; `fit` makes the report small enough
//! to send, leaving the oldest rows out, and checks it (`check::report`) as the server will.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write;

use serde::Serialize;

use super::check;
use super::facts::{Facts, GpuFacts};
use super::info::Hardware;
use super::sample::Sample;
use super::scrub;
use super::stats::{Count, Event, Taken, Times};

/// How large a report may be as it is sent (gzip), and before it is compressed.
pub const MAX_SENT: usize = 12 * 1024;
pub const MAX_RAW: usize = 64 * 1024;

fn empty<T>(list: &[T]) -> bool {
    list.is_empty()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Report {
    pub v: u8,
    pub kind: &'static str,
    pub install_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    pub sent_at: i64,
    pub app: AppInfo,
    pub os: Os,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<Cpu>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ram_gb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<Gpu>,
    #[serde(skip_serializing_if = "empty")]
    pub displays: Vec<Display>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Outputs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrations: Option<Integrations>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_s: Option<u64>,
    #[serde(skip_serializing_if = "empty")]
    pub minutes: Vec<MinuteRow>,
    #[serde(skip_serializing_if = "empty")]
    pub events: Vec<EventRow>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub features: BTreeMap<String, u32>,
    #[serde(skip_serializing_if = "empty")]
    pub problems: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash: Option<Crash>,
    #[serde(skip_serializing_if = "Dropped::none")]
    pub dropped: Dropped,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AppInfo {
    pub version: String,
    pub commit: String,
    pub channel: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Os {
    pub name: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Cpu {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub cores: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Gpu {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    pub backend: &'static str,
    #[serde(rename = "type")]
    pub kind: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Display {
    pub w: u32,
    pub h: u32,
    pub scale: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hz: Option<f64>,
    pub main: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Outputs {
    pub audience: u32,
    pub stage: u32,
    pub on: u32,
    pub display: u32,
    pub window: u32,
    pub ndi: u32,
    pub decklink: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Integrations {
    pub ndi: bool,
    pub decklink: bool,
    pub ffmpeg: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MinuteRow {
    pub at: i64,
    pub up_s: u64,
    pub ticks: u32,
    pub cpu: CpuUse,
    pub mem_mb: MemUse,
    pub ui: UiTimes,
    pub screens: ScreenTimes,
    #[serde(skip_serializing_if = "empty")]
    pub scene: Vec<String>,
    pub actions: Actions,
    pub outputs_on: u32,
}

/// Each `[average, highest]`, in percent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct CpuUse {
    pub app: [f64; 2],
    pub video: [f64; 2],
    pub system: [f64; 2],
}

/// Each `[average, highest]`, in megabytes, and the least free memory seen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct MemUse {
    pub app: [f64; 2],
    pub video: [f64; 2],
    pub free_least: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct UiTimes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<&'static str>,
    pub frames: u64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    pub over50: u64,
    pub over100: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct ScreenTimes {
    pub passes: u64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    pub over50: u64,
    pub over100: u64,
    pub shown: u64,
    pub not_shown: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Actions {
    pub keys: u64,
    pub clicks: u64,
    pub remote: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct EventRow {
    pub t: f64,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Crash {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backtrace: Option<String>,
    pub uptime_s: u64,
}

/// How many rows were left out to fit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Dropped {
    pub minutes: u64,
    pub events: u64,
    pub problems: u64,
}

impl Dropped {
    fn none(&self) -> bool {
        *self == Dropped::default()
    }
}

fn round1(value: f64) -> f64 {
    if value.is_finite() { (value.max(0.0) * 10.0).round() / 10.0 } else { 0.0 }
}

// ---------------------------------------------------------------------------------------------
// From readings to rows

#[derive(Clone, Copy, Debug, Default)]
struct Stat {
    sum: f64,
    max: f64,
    n: u32,
}

impl Stat {
    fn add(&mut self, value: f32) {
        let value = f64::from(value);
        self.sum += value;
        self.max = self.max.max(value);
        self.n += 1;
    }

    /// `[average, highest]`, within `most`.
    fn row(&self, most: f64) -> [f64; 2] {
        let average = if self.n == 0 { 0.0 } else { self.sum / f64::from(self.n) };
        [round1(average).min(most), round1(self.max).min(most)]
    }
}

/// What a minute has gathered so far.
#[derive(Default)]
struct Minute {
    ticks: u32,
    app: Stat,
    kids: Stat,
    system: Stat,
    app_mb: Stat,
    kids_mb: Stat,
    free_mb_least: Option<f32>,
    ui: Times,
    pass: Times,
    counts: [u64; super::stats::COUNTS],
    scene: Vec<String>,
    view: &'static str,
    outputs_on: u32,
}

/// How many five-second readings make a minute.
pub const TICKS_PER_MINUTE: u32 = 12;

/// The rows waiting for the next report, each list within the format's limit (the oldest go,
/// and are counted), and what is being gathered into the next minute.
#[derive(Default)]
pub struct Collector {
    minute: Minute,
    minutes: VecDeque<MinuteRow>,
    events: VecDeque<EventRow>,
    features: BTreeMap<String, u32>,
    problems: VecDeque<String>,
    dropped: Dropped,
}

impl Collector {
    /// One reading, taken `up` seconds after the app started.
    pub fn tick(&mut self, up: f64, now_unix: i64, sample: Option<Sample>, taken: &Taken, facts: &Facts) {
        let sample = sample.unwrap_or_default();
        let minute = &mut self.minute;
        minute.ticks += 1;
        minute.app.add(sample.app_cpu);
        minute.kids.add(sample.kids_cpu);
        minute.system.add(sample.system_cpu);
        minute.app_mb.add(sample.app_mb);
        minute.kids_mb.add(sample.kids_mb);
        if let Some(free) = sample.free_mb {
            minute.free_mb_least = Some(minute.free_mb_least.map_or(free, |least| least.min(free)));
        }
        minute.ui.merge(&taken.ui);
        minute.pass.merge(&taken.pass);
        for (total, add) in minute.counts.iter_mut().zip(taken.counts) {
            *total += add;
        }
        for code in facts.scene_codes() {
            if check::is_code(&code) && !minute.scene.contains(&code) && minute.scene.len() < check::SCENE_CODES {
                minute.scene.push(code);
            }
        }
        minute.view = facts.view;
        minute.outputs_on = facts.outputs.on;
        if minute.ticks >= TICKS_PER_MINUTE {
            self.close_minute(up, now_unix);
        }
    }

    /// Ends the minute now, with the readings it has (for a report sent before the minute is up).
    pub fn close_minute(&mut self, up: f64, now_unix: i64) {
        let minute = std::mem::take(&mut self.minute);
        if minute.ticks == 0 {
            return;
        }
        let count = |what: Count| minute.counts[what as usize];
        let (ui, pass) = (minute.ui.summary(), minute.pass.summary());
        let row = MinuteRow {
            at: now_unix.max(0),
            up_s: up.max(0.0).round() as u64,
            ticks: minute.ticks.min(60),
            cpu: CpuUse { app: minute.app.row(1e5), video: minute.kids.row(1e5), system: minute.system.row(1e5) },
            mem_mb: MemUse {
                app: minute.app_mb.row(1e8),
                video: minute.kids_mb.row(1e8),
                free_least: minute.free_mb_least.map(|free| round1(f64::from(free))),
            },
            ui: UiTimes {
                view: Some(minute.view).filter(|view| check::is_code(view)),
                frames: count(Count::UiFrames),
                p50: round1(f64::from(ui.p50)),
                p95: round1(f64::from(ui.p95)),
                max: round1(f64::from(ui.max)),
                over50: count(Count::UiOver50),
                over100: count(Count::UiOver100),
            },
            screens: ScreenTimes {
                passes: count(Count::Passes),
                p50: round1(f64::from(pass.p50)),
                p95: round1(f64::from(pass.p95)),
                max: round1(f64::from(pass.max)),
                over50: count(Count::PassesOver50),
                over100: count(Count::PassesOver100),
                shown: count(Count::WindowShown),
                not_shown: count(Count::WindowNotShown),
            },
            scene: minute.scene,
            actions: Actions {
                keys: count(Count::Keys),
                clicks: count(Count::Clicks),
                remote: count(Count::RemoteActions),
            },
            outputs_on: minute.outputs_on.min(64),
        };
        if self.minutes.len() >= check::MINUTES {
            self.minutes.pop_front();
            self.dropped.minutes += 1;
        }
        self.minutes.push_back(row);
    }

    /// Events as the app noted them. One whose code doesn't have a code's shape is left out.
    pub fn take_events(&mut self, events: Vec<Event>) {
        for event in events.into_iter().filter(|event| check::is_code(event.kind)) {
            if self.events.len() >= check::EVENTS {
                self.events.pop_front();
                self.dropped.events += 1;
            }
            self.events.push_back(EventRow {
                t: round1(event.t),
                kind: event.kind,
                n: event.n.map(|n| n.clamp(0.0, 1e9)),
                ms: event.ms.map(|ms| round1(ms).min(1e7)),
            });
        }
    }

    /// How many times views and tools were used.
    pub fn take_features(&mut self, used: Vec<(String, u32)>) {
        for (code, times) in used.into_iter().filter(|(code, _)| check::is_code(code)) {
            if self.features.len() < check::FEATURES || self.features.contains_key(&code) {
                let count = self.features.entry(code).or_insert(0);
                *count = count.saturating_add(times).min(10_000_000);
            }
        }
    }

    /// Problem lines that passed `scrub::problem_line`.
    pub fn take_problems(&mut self, lines: Vec<String>) {
        for line in lines {
            if self.problems.contains(&line) {
                continue;
            }
            if self.problems.len() >= check::PROBLEMS {
                self.problems.pop_front();
                self.dropped.problems += 1;
            }
            self.problems.push_back(line);
        }
    }

    /// Puts what waits into a report. `taking` empties the collector (the report is being sent);
    /// without it the collector is only looked at (See what's sent).
    fn fill(&mut self, report: &mut Report, taking: bool) {
        report.minutes = self.minutes.iter().cloned().collect();
        report.events = self.events.iter().cloned().collect();
        report.features = self.features.clone();
        report.problems = self.problems.iter().cloned().collect();
        report.dropped = self.dropped;
        if taking {
            let minute = std::mem::take(&mut self.minute);
            *self = Collector { minute, ..Collector::default() };
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Building a report

/// The commit the build was made from, where the build said (`SOFTPRESENTER_GIT_COMMIT`).
const COMMIT: &str = match option_env!("SOFTPRESENTER_GIT_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};

/// What every report says first: which install, which build, which computer.
pub struct About<'a> {
    pub install_id: &'a str,
    pub session: &'a str,
    pub hardware: &'a Hardware,
    pub gpu: Option<&'a GpuFacts>,
    pub facts: Option<&'a Facts>,
    /// Whether FFmpeg is on this computer.
    pub ffmpeg: bool,
    pub uptime_s: u64,
    pub now_unix: i64,
}

fn backend_code(backend: &str) -> &'static str {
    match backend.to_ascii_lowercase().as_str() {
        "metal" => "metal",
        "vulkan" => "vulkan",
        "dx12" => "dx12",
        "gl" => "gl",
        _ => "other",
    }
}

fn device_code(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "integratedgpu" => "integrated",
        "discretegpu" => "discrete",
        "virtualgpu" => "virtual",
        "cpu" => "cpu",
        _ => "other",
    }
}

fn base(kind: &'static str, about: &About<'_>) -> Report {
    let hardware = about.hardware;
    let version = env!("CARGO_PKG_VERSION");
    Report {
        v: 1,
        kind,
        install_id: about.install_id.to_owned(),
        session: Some(about.session.to_owned()).filter(|session| check::is_hex32(session)),
        sent_at: about.now_unix.max(0),
        app: AppInfo {
            version: if check::is_version(version) { version.to_owned() } else { "0".to_owned() },
            commit: if check::is_commit(COMMIT) { COMMIT.to_owned() } else { "unknown".to_owned() },
            // A build made for trying things out says so; what people install says "release".
            channel: if cfg!(debug_assertions) { "dev" } else { "release" },
        },
        os: Os {
            name: match hardware.os.as_str() {
                "macos" => "macos",
                "windows" => "windows",
                _ => "linux",
            },
            version: scrub::hardware_name(&hardware.os_version),
            arch: Some(hardware.arch.clone()).filter(|arch| {
                (1..=16).contains(&arch.len())
                    && arch.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            }),
        },
        cpu: Some(Cpu { model: scrub::hardware_name(&hardware.cpu_model), cores: hardware.cores.min(4096) as u32 }),
        ram_gb: (hardware.ram_mb > 0).then(|| ((hardware.ram_mb + 512) / 1024).min(65_536) as u32),
        gpu: about.gpu.map(|gpu| Gpu {
            name: scrub::hardware_name(&gpu.name),
            driver: scrub::hardware_name(&gpu.driver),
            backend: backend_code(&gpu.backend),
            kind: device_code(&gpu.device_type),
        }),
        displays: about.facts.map_or_else(Vec::new, |facts| {
            facts
                .displays
                .iter()
                .filter(|display| display.w >= 1 && display.h >= 1)
                .take(check::DISPLAYS)
                .map(|display| Display {
                    w: display.w.min(65_536),
                    h: display.h.min(65_536),
                    scale: ((display.scale * 100.0).round() / 100.0).clamp(0.0, 16.0),
                    hz: display.hz.map(|hz| ((f64::from(hz) * 100.0).round() / 100.0).clamp(0.0, 1000.0)),
                    main: display.main,
                })
                .collect()
        }),
        outputs: about.facts.map(|facts| {
            let most = |n: u32| n.min(64);
            let o = facts.outputs;
            Outputs {
                audience: most(o.audience),
                stage: most(o.stage),
                on: most(o.on),
                display: most(o.display),
                window: most(o.window),
                ndi: most(o.ndi),
                decklink: most(o.decklink),
            }
        }),
        integrations: about.facts.map(|facts| Integrations {
            ndi: facts.ndi,
            decklink: facts.decklink,
            ffmpeg: about.ffmpeg,
        }),
        language: about.facts.map(|facts| facts.language.to_owned()).filter(|language| check::is_language(language)),
        uptime_s: Some(about.uptime_s.min(100_000_000)),
        ..Report::default()
    }
}

/// A report of how the app has been running, with the rows that wait. `taking` empties them.
pub fn running(about: &About<'_>, collector: &mut Collector, taking: bool) -> Report {
    let mut report = base("report", about);
    collector.fill(&mut report, taking);
    report
}

/// What a crash left behind, as the panic hook wrote it down.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, Serialize)]
#[serde(default)]
pub struct CrashRecord {
    pub message: String,
    pub location: String,
    pub thread: String,
    pub backtrace: String,
    pub uptime_s: u64,
}

/// A crash's report. Its message goes only if it could not hold a name; its place and the
/// paths of its backtrace are cut down to the program's source (`scrub`).
pub fn crash(about: &About<'_>, record: &CrashRecord, scrubber: &crate::log_scrub::Scrubber) -> Report {
    let mut report = base("crash", about);
    report.session = None;
    report.uptime_s = None;
    report.crash = Some(Crash {
        message: scrub::crash_message(&record.message, scrubber),
        location: scrub::location(&record.location),
        thread: scrub::thread(&record.thread),
        backtrace: scrub::backtrace(&record.backtrace),
        uptime_s: record.uptime_s.min(100_000_000),
    });
    report
}

// ---------------------------------------------------------------------------------------------
// Fitting and compressing

pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// The report as JSON, as it is sent.
pub fn json(report: &Report) -> String {
    serde_json::to_string(report).unwrap_or_default()
}

/// The report compressed and within the sizes the server takes, the oldest rows left out (and
/// counted in `dropped`) until it is. Nothing when it doesn't pass the format's check, or can't
/// be made to fit: such a report is never sent.
pub fn fit(mut report: Report) -> Option<Vec<u8>> {
    loop {
        let text = json(&report);
        let body = gzip(text.as_bytes());
        if text.len() <= MAX_RAW && body.len() <= MAX_SENT {
            let value: serde_json::Value = serde_json::from_str(&text).ok()?;
            return check::report(&value).ok().map(|()| body);
        }
        // The oldest rows go first: a fifth of the longest list at a time.
        let part = |len: usize| len.div_ceil(5);
        if !report.events.is_empty() && report.events.len() >= report.minutes.len() * 4 {
            let gone = part(report.events.len());
            report.events.drain(..gone);
            report.dropped.events += gone as u64;
        } else if !report.minutes.is_empty() {
            let gone = part(report.minutes.len());
            report.minutes.drain(..gone);
            report.dropped.minutes += gone as u64;
        } else if !report.events.is_empty() {
            let gone = part(report.events.len());
            report.events.drain(..gone);
            report.dropped.events += gone as u64;
        } else if !report.problems.is_empty() {
            report.problems.remove(0);
            report.dropped.problems += 1;
        } else if let Some(backtrace) = report.crash.as_mut().and_then(|crash| crash.backtrace.as_mut())
            && backtrace.lines().count() > 8
        {
            // A backtrace's top says where the crash was: its lower half goes.
            let lines: Vec<&str> = backtrace.lines().collect();
            *backtrace = lines[..lines.len() / 2].join("\n");
        } else {
            return None;
        }
    }
}
