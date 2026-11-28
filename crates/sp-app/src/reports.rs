//! Anonymous reports: how SoftPresenter runs on the computers it is used on, and what goes
//! wrong, sent to the SoftPresenter project so problems are found and fixed.
//! `docs/anonymous-reports.md` lists every field, and `PRIVACY.md` says it in plain words.
//!
//! **What is sent:** the app's version, the computer (system, processor, memory, graphics, the
//! displays' sizes), how many screens are set up and on, a row for each minute of how the app
//! ran (processor and memory, how long the operator's window and the screens took to draw, what
//! kinds of thing were on screen), events as codes, how many times each view and tool was used,
//! problem lines that can't hold a name, and any crash. **Never sent:** the words or titles of
//! songs, Bible text, the names of files, folders, displays, screens or people, the computer's
//! name, addresses, passwords, the log, anything typed. The report is built from the closed
//! types of `format` and checked by `check` before it is kept for sending.
//!
//! **When:** reports are on unless the person turns them off (in the European Union, the rest
//! of the European Economic Area and the United Kingdom they are off unless the person turns
//! them on: `region`), but nothing is counted, written or sent until they have been told: the setup's step has been passed, or the one-time notice
//! shown (`should_tell`). Then one report every fifteen minutes, one as the app quits and one
//! for a crash, none sooner than a minute after being told (in a later run, a crash that waits
//! and the report at quit go at once). Turning them off stops at once and deletes what was
//! waiting. A test copy never sends (`environment`).
//!
//! All of it is done on a thread of its own. The threads that draw only add to a few numbers
//! (`stats`); the reporter reads them every five seconds. Nothing waits on the network except
//! that thread; at quit the app gives it two seconds at the most, from its event loop.

mod app_facts;
pub mod check;
pub mod facts;
pub mod format;
pub mod info;
pub mod region;
pub mod sample;
pub mod scrub;
pub mod stats;
pub mod ui;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::log_scrub::Scrubber;
pub use facts::{Facts, GpuFacts};

/// Where the reports of a public build go: the SoftPresenter project's own server. Not a
/// secret: the route takes a report from anyone, and checks it strictly. Integration requests
/// go there too (`integration_request`).
pub(crate) const SERVER: &str = "https://reports.softpresenter.com";
const ROUTE: &str = "/v1/public/report";

/// How much waits on disk for the network to come back, at the most; the oldest go.
pub const QUEUE_LIMIT: u64 = 256 * 1024;
const TICK: Duration = Duration::from_secs(5);
const EVERY: Duration = Duration::from_secs(15 * 60);
/// Nothing is sent sooner than this after the person was told (or after the app started), so
/// that turning reports off in the first minute means nothing ever left.
const NOT_BEFORE: Duration = Duration::from_secs(60);
const QUIT_BUDGET: Duration = Duration::from_secs(2);
/// How many reports one round sends at the most (the server takes six a minute from an install).
const A_ROUND: usize = 4;

// ---------------------------------------------------------------------------------------------
// Where to send, and whether to

/// Where reports go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub url: String,
    /// A test copy pointed at a server on this computer, which may use plain http.
    pub local: bool,
}

/// Where this copy of the app sends its reports, or nowhere. A test copy never sends: one run
/// with `SOFTPRESENTER_QUIET`, with a data folder of its own (`SOFTPRESENTER_DATA`), or only to
/// save a picture (`quiet_run`). The one exception is the testing aid below.
fn environment(quiet_run: bool, var: impl Fn(&str) -> Option<String>) -> Option<Target> {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    // Testing aid: SOFTPRESENTER_REPORTS_TEST_URL=http://127.0.0.1:<port> sends a test copy's
    // reports to a server on this computer, for the end-to-end test. Honoured only together
    // with SOFTPRESENTER_DATA, and only for 127.0.0.1 or localhost.
    if let Some(address) = var("SOFTPRESENTER_REPORTS_TEST_URL").filter(|address| !address.is_empty()) {
        let address = address.trim().trim_end_matches('/').to_owned();
        let host = address.strip_prefix("http://").map(|rest| rest.split(['/', ':']).next().unwrap_or(""));
        return (set("SOFTPRESENTER_DATA") && matches!(host, Some("127.0.0.1" | "localhost")))
            .then(|| Target { url: format!("{address}{ROUTE}"), local: true });
    }
    if quiet_run || set("SOFTPRESENTER_QUIET") || set("SOFTPRESENTER_DATA") {
        return None;
    }
    Some(Target { url: format!("{SERVER}{ROUTE}"), local: false })
}

// ---------------------------------------------------------------------------------------------
// What the window and the thread share

#[derive(Default)]
struct State {
    /// Whether reports are on: the person's choice, or where they have made none, how reports
    /// start where the computer is (off until that is known).
    enabled: bool,
    /// The choice the person made, if they have made one. It always wins.
    chosen: Option<bool>,
    /// How reports start where the computer is (`region`), once the reporter's thread has asked.
    starts_on: Option<bool>,
    /// The person has been told that reports are on (the setup's step, or the notice).
    seen: bool,
    config_changed: bool,
    install_id: String,
    new_id_asked: bool,
    facts: Option<Facts>,
    gpu: Option<GpuFacts>,
    /// See what's sent: asked for, and the report as JSON once the thread has made it.
    preview_asked: bool,
    preview: Option<String>,
    quit: bool,
    quit_done: bool,
    /// How many reports have gone this run, and why the last try failed, if it did.
    sent: u64,
    last_error: Option<String>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    /// Where reports go; nowhere in a test copy.
    target: Option<Target>,
    folder: PathBuf,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
/// True while reports are on, the person has been told, and this copy may send: the panic hook
/// reads it, and writes nothing down otherwise.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Reports were switched on or off (in Settings, the setup or `sp reports`): the one-time
/// notice's question, if it still waits, has been answered there.
static CHOSEN: AtomicBool = AtomicBool::new(false);

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where the reporter keeps its files: its settings (on or off, told or not, the install's
/// random number), the reports waiting for the network, a crash waiting to be sent. Beside the
/// log, which is this computer's own place, and not in the library: a library copied or synced
/// to another computer must not bring "already told" and the same number with it.
fn reports_folder() -> Option<PathBuf> {
    crate::applog::folder().map(|dir| dir.join("anonymous-reports"))
}

/// What is kept between runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// The person's choice, on or off; nothing while they have made none, and then reports
    /// start as they do where the computer is (`region::starts_on`).
    pub enabled: Option<bool>,
    pub seen: bool,
    pub install_id: String,
}

impl Default for Saved {
    /// No choice made, and nobody has been told yet.
    fn default() -> Saved {
        Saved { enabled: None, seen: false, install_id: String::new() }
    }
}

/// The settings as they were left. A file that is missing or can't be read means no choice
/// made and not yet told, so the person is told (again) before anything is sent.
pub fn read_saved(folder: &Path) -> Saved {
    let found: Option<Saved> =
        std::fs::read_to_string(folder.join("settings.json")).ok().and_then(|text| serde_json::from_str(&text).ok());
    let mut saved = found.unwrap_or_default();
    if !check::is_hex32(&saved.install_id) {
        saved.install_id = random_hex();
    }
    saved
}

pub fn write_saved(folder: &Path, saved: &Saved) {
    let _ = std::fs::create_dir_all(folder);
    let part = folder.join("settings.json.part");
    if std::fs::write(&part, serde_json::to_string_pretty(saved).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(part, folder.join("settings.json"));
    }
}

/// A random number as 32 hex digits: made from nothing about the computer or the person.
pub fn random_hex() -> String {
    sp_model::Uuid::new_v4().simple().to_string()
}

// ---------------------------------------------------------------------------------------------
// The calls the app makes (each cheap, and none waiting)

/// Reads the settings and starts the reporter's thread, which sleeps until there is something
/// to do. `quiet_run` is a run that only saves a picture.
pub fn start(quiet_run: bool) {
    let Some(folder) = reports_folder() else { return };
    let target = environment(quiet_run, |name| std::env::var(name).ok());
    let saved = read_saved(&folder);
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            // With no choice made, off until the reporter's thread has asked where the computer is.
            enabled: saved.enabled.unwrap_or(false),
            chosen: saved.enabled,
            seen: saved.seen,
            install_id: saved.install_id.clone(),
            ..State::default()
        }),
        wake: Condvar::new(),
        target,
        folder,
    });
    if SHARED.set(shared.clone()).is_err() {
        return;
    }
    let reporter = Reporter::new(shared, saved);
    let _ = std::thread::Builder::new().name("reports".to_owned()).spawn(move || reporter.run());
    let _ = std::thread::Builder::new().name("reports watchdog".to_owned()).spawn(watchdog);
}

/// Whether this build has the anonymous reports at all: their reporter was started. Where it
/// wasn't, the setup has no Reports step and nothing here is drawn or counted.
pub fn offered() -> bool {
    SHARED.get().is_some()
}

/// The graphics the app chose, once it has.
pub fn gpu_chosen(chosen: &sp_render::wgpu::AdapterInfo) {
    if let Some(shared) = SHARED.get() {
        lock(shared).gpu = Some(GpuFacts {
            name: chosen.name.clone(),
            backend: format!("{:?}", chosen.backend),
            driver: format!("{} {}", chosen.driver, chosen.driver_info).trim().to_owned(),
            device_type: format!("{:?}", chosen.device_type),
        });
    }
}

/// The window's latest facts. Never waits: if the reporter holds the lock this time, the next
/// call brings the same.
pub fn set_facts(facts: Facts) {
    if let Some(shared) = SHARED.get()
        && let Ok(mut state) = shared.state.try_lock()
    {
        state.facts = Some(facts);
    }
}

/// How reporting stands, for Settings, the setup and `sp reports`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Standing {
    pub enabled: bool,
    /// How reports start where the computer is: on, or (the EU, the EEA and the UK) off.
    pub starts_on: bool,
    pub seen: bool,
    /// False in a test copy, which never sends whatever the switch says.
    pub may_send: bool,
    pub install_id: String,
    pub sent: u64,
    pub last_error: Option<String>,
}

/// How reporting stands; nothing where there is no reporter (or it holds the lock this moment).
pub fn standing() -> Option<Standing> {
    let shared = SHARED.get()?;
    let state = shared.state.try_lock().ok()?;
    Some(Standing {
        enabled: state.enabled,
        starts_on: state.starts_on.unwrap_or(true),
        seen: state.seen,
        may_send: shared.target.is_some(),
        install_id: state.install_id.clone(),
        sent: state.sent,
        last_error: state.last_error.clone(),
    })
}

/// Turns reports on or off, as the person chose (which also means they have been told). Off
/// stops sending at once and deletes what was waiting; the reporter's thread does both.
pub fn set_enabled(enabled: bool) {
    if let Some(shared) = SHARED.get() {
        let mut state = lock(shared);
        CHOSEN.store(true, Ordering::Relaxed);
        if state.enabled != enabled || !state.seen || state.chosen != Some(enabled) {
            state.enabled = enabled;
            state.chosen = Some(enabled);
            state.seen = true;
            state.config_changed = true;
            if !enabled {
                // At once: the panic hook and the counters stop here, not at the thread's next round.
                ACTIVE.store(false, Ordering::Relaxed);
                stats::switch_off();
            }
            shared.wake.notify_all();
        }
    }
}

/// The person has been told that reports are on: the setup's step was passed, or the notice
/// shown. From now (and no sooner than a minute from now) reports may go.
pub fn mark_seen() {
    if let Some(shared) = SHARED.get() {
        let mut state = lock(shared);
        if !state.seen {
            state.seen = true;
            state.config_changed = true;
            shared.wake.notify_all();
        }
    }
}

/// The setup's Reports step was passed with Continue: the person has seen how reports stand
/// (on, or off where they start off) and left them so. That is kept as their choice, so a
/// computer taken to another region later doesn't change it unasked.
pub fn confirm_shown() {
    if let Some(shared) = SHARED.get()
        && lock(shared).confirm()
    {
        shared.wake.notify_all();
    }
}

impl State {
    /// How reports stand is kept as the person's choice, and they have been told. Nothing
    /// while where the computer is has not been asked yet. True when anything changed.
    fn confirm(&mut self) -> bool {
        if self.starts_on.is_none() || (self.seen && self.chosen.is_some()) {
            return false;
        }
        self.chosen = Some(self.enabled);
        self.seen = true;
        self.config_changed = true;
        true
    }
}

/// True once after reports were switched on or off: the notice's question, if it waits, goes.
pub fn chosen_since() -> bool {
    CHOSEN.swap(false, Ordering::Relaxed)
}

/// Whether the one-time notice is owed: reports are on, this copy may send, and the person has
/// not been told. (A test copy never sends, so it says nothing; where reports start off,
/// nothing is on, so there is nothing to announce.)
pub fn should_tell() -> bool {
    SHARED.get().is_some_and(|shared| {
        shared.target.is_some()
            && shared.state.try_lock().is_ok_and(|state| state.starts_on.is_some() && state.enabled && !state.seen)
    })
}

/// Asks for a new random install number. What was waiting to be sent under the old one goes.
pub fn new_install_id() {
    if let Some(shared) = SHARED.get() {
        lock(shared).new_id_asked = true;
        shared.wake.notify_all();
    }
}

/// See what's sent: asks the reporter's thread for the next report as it stands now.
pub fn ask_preview() {
    if let Some(shared) = SHARED.get() {
        let mut state = lock(shared);
        state.preview_asked = true;
        state.preview = None;
        shared.wake.notify_all();
    }
}

/// The report asked for with `ask_preview`, as JSON, once it has been made.
pub fn preview() -> Option<String> {
    SHARED.get()?.state.try_lock().ok()?.preview.clone()
}

/// The same, waited for (the command line's `sp reports show`): two seconds at the most.
pub fn preview_now() -> Option<String> {
    let shared = SHARED.get()?;
    ask_preview();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut state = lock(shared);
    while state.preview.is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        state = shared.wake.wait_timeout(state, left).map_or_else(|e| e.into_inner().0, |(state, _)| state);
    }
    state.preview.clone()
}

/// `sp reports`, `reports show`, `reports on`, `reports off`, `reports reset-id`: what Settings ›
/// About does, from the command line.
pub fn ctl(args: &[String]) -> Result<crate::control::Reply, String> {
    use crate::control::Reply;
    let usage = "reports | reports show | reports on | reports off | reports reset-id";
    let said = |standing: &Standing| {
        let mut text =
            if standing.enabled { "Anonymous reports are on." } else { "Anonymous reports are off." }.to_owned();
        if !standing.may_send {
            text.push_str(" This is a test copy: it never sends them.");
        } else if standing.enabled && !standing.seen {
            text.push_str(" Nothing is sent until the notice that says so has been shown.");
        }
        if standing.sent > 0 {
            text.push_str(&format!(" Sent since the app started: {}.", standing.sent));
        }
        if let Some(why) = &standing.last_error {
            text.push_str(&format!(" The last try did not go: {why}."));
        }
        let data = serde_json::json!({
            "on": standing.enabled,
            "starts_on": standing.starts_on,
            "told": standing.seen,
            "may_send": standing.may_send,
            "anonymous_id": standing.install_id,
            "sent": standing.sent,
        });
        Reply::with(text, data)
    };
    let standing = || standing().ok_or_else(|| "The reports aren't ready; try again.".to_owned());
    match args.get(1).map(|word| word.to_lowercase()).as_deref() {
        None => Ok(said(&standing()?)),
        Some("show") if args.len() == 2 => {
            let report = preview_now().ok_or("The report could not be made in time; try again.")?;
            let data = serde_json::from_str(&report).unwrap_or_default();
            Ok(Reply::with(report, data))
        }
        Some(word @ ("on" | "off")) if args.len() == 2 => {
            set_enabled(word == "on");
            Ok(said(&Standing { enabled: word == "on", seen: true, ..standing()? }))
        }
        Some("reset-id") if args.len() == 2 => {
            new_install_id();
            Ok(Reply::done("The anonymous id is a new random number. What waited under the old one was deleted."))
        }
        _ => Err(format!("Usage: {usage}")),
    }
}

/// When the app began to quit, if reports were on then.
static QUIT_BEGAN: OnceLock<Instant> = OnceLock::new();

/// The app begins to quit (its screens have closed): the reporter sends its last report now, on
/// its own thread, while the app does the rest of its leaving (the library's writer writes what
/// it holds). Never waits.
pub fn quit_begin() {
    let Some(shared) = SHARED.get() else { return };
    if !ACTIVE.load(Ordering::Relaxed) || QUIT_BEGAN.set(Instant::now()).is_err() {
        return;
    }
    lock(shared).quit = true;
    shared.wake.notify_all();
}

/// Whether the app may go: the last report has gone (or been kept on disk for the next start),
/// or two seconds have passed since the app began to quit, or reports were not on. Never
/// waits: the event loop asks again while it is not so, so the window still answers.
pub fn quit_done() -> bool {
    let (Some(shared), Some(began)) = (SHARED.get(), QUIT_BEGAN.get()) else { return true };
    began.elapsed() >= QUIT_BUDGET || shared.state.try_lock().is_ok_and(|state| state.quit_done)
}

/// A panic: writes what a crash report needs to a file in the app's own folder, which the
/// reporter sends (at once if the app lives on, else at the next start). Only while reports are
/// on and the person has been told; otherwise nothing is written. Called from the panic hook,
/// so it takes care to need nothing that may be locked and to ignore every failure.
pub fn on_panic(info: &std::panic::PanicHookInfo<'_>) {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let Some(folder) = SHARED.get().map(|shared| shared.folder.clone()) else { return };
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_default();
    let record = format::CrashRecord {
        message,
        location: info.location().map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column())).unwrap_or_default(),
        thread: std::thread::current().name().unwrap_or("unnamed").to_owned(),
        backtrace: std::backtrace::Backtrace::force_capture().to_string(),
        uptime_s: stats::since_start_ms() / 1000,
    };
    let _ = std::fs::create_dir_all(&folder);
    let name = format!("crash-{}.json", chrono::Utc::now().timestamp_millis());
    let part = folder.join(format!("{name}.part"));
    if write_private(&part, serde_json::to_string(&record).unwrap_or_default().as_bytes()).is_ok() {
        let _ = std::fs::rename(part, folder.join(name));
    }
}

/// Writes a file only this user can read: a crash's own words, before they are looked at, may
/// quote what was on screen (as the log may, which is kept the same way).
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}

/// Watches the operator's window and the screens' thread: a thread that stays in one stretch of
/// work for three seconds is frozen, and this notes it (which thread, and the number of the step
/// it last finished), and notes again when it recovers, with how long it was. It looks once a
/// second while reports are on, and does nothing else.
fn watchdog() {
    let mut frozen: [bool; 2] = [false; 2];
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if !stats::is_on() {
            continue;
        }
        for (at, who) in [stats::Who::Ui, stats::Who::Screens].into_iter().enumerate() {
            let (busy_for, phase, ended) = stats::watch(who);
            let (freeze, over) = who.freeze_codes();
            if let Some(took) = ended
                && frozen[at]
            {
                frozen[at] = false;
                stats::event_ms(over, took.as_secs_f64() * 1000.0);
            }
            if let Some(busy_for) = busy_for
                && busy_for >= stats::FREEZE_AFTER
                && !frozen[at]
            {
                frozen[at] = true;
                stats::event_n(freeze, f64::from(phase));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The reporter

struct Reporter {
    shared: Arc<Shared>,
    saved: Saved,
    session: String,
    scrubber: Scrubber,
    hardware: Option<info::Hardware>,
    /// Whether FFmpeg is on this computer, looked for once.
    ffmpeg: Option<bool>,
    sampler: sample::Sampler,
    collector: format::Collector,
    began: Instant,
    /// Whether reports were on, told of and allowed at the last round.
    active: bool,
    /// No report of how the app runs goes before then: `grace` after the person was told, or
    /// after the app started.
    not_before: Instant,
    grace: Duration,
    /// The person was told in an earlier run. Then a crash that waits, and the report at quit,
    /// go without the wait: an app that stops in its first minute at every start would
    /// otherwise never say so. (Told in this run, nothing at all goes in the first minute.)
    told_before: bool,
    last_report: Option<Instant>,
    last_tick: Option<Instant>,
    last_footprint: Option<Instant>,
    /// After a failure: not before then, and the wait to use next time.
    backoff: Option<(Instant, Duration)>,
    video_sizes: std::collections::HashMap<PathBuf, Option<(u32, u32, f64)>>,
}

impl Reporter {
    fn new(shared: Arc<Shared>, saved: Saved) -> Reporter {
        Reporter {
            shared,
            told_before: saved.seen,
            saved,
            session: random_hex(),
            scrubber: Scrubber::from_env(),
            hardware: None,
            ffmpeg: None,
            sampler: sample::Sampler::new(),
            collector: format::Collector::default(),
            began: Instant::now(),
            active: false,
            not_before: Instant::now() + NOT_BEFORE,
            grace: NOT_BEFORE,
            last_report: None,
            last_tick: None,
            last_footprint: None,
            backoff: None,
            video_sizes: std::collections::HashMap::new(),
        }
    }

    /// The thread's whole life. While reports are on: every five seconds a reading and, when it
    /// is time, a report. While they are off: asleep until the window asks for something.
    fn run(mut self) {
        // Where the computer is decides how reports start for someone who has made no choice.
        // Asking may run a small program, so it is done here, once.
        {
            let known = lock(&self.shared).starts_on;
            let starts_on = known.unwrap_or_else(|| region::starts_on(region::read().as_deref()));
            let mut state = lock(&self.shared);
            state.starts_on = Some(starts_on);
            if state.chosen.is_none() {
                state.enabled = starts_on;
            }
        }
        // The first round is taken at once: an install that was told in an earlier run starts
        // counting now, with nothing having to ask.
        let mut first = true;
        loop {
            let quit = {
                let mut state = lock(&self.shared);
                let asked =
                    |state: &State| state.quit || state.config_changed || state.preview_asked || state.new_id_asked;
                if !std::mem::take(&mut first) && !asked(&state) {
                    state = if self.active {
                        self.shared.wake.wait_timeout(state, TICK).map_or_else(|e| e.into_inner().0, |(state, _)| state)
                    } else {
                        self.shared.wake.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner)
                    };
                }
                state.quit
            };
            self.round(quit);
            if quit {
                let mut state = lock(&self.shared);
                state.quit_done = true;
                self.shared.wake.notify_all();
                return;
            }
        }
    }

    /// One round: what the window asked for, the readings, and a report if one is due.
    fn round(&mut self, quit: bool) {
        let (enabled, chosen, seen, facts, changed, new_id, preview) = {
            let mut state = lock(&self.shared);
            (
                state.enabled,
                state.chosen,
                state.seen,
                state.facts.clone(),
                std::mem::take(&mut state.config_changed),
                std::mem::take(&mut state.new_id_asked),
                std::mem::take(&mut state.preview_asked),
            )
        };
        if changed || new_id {
            self.saved.enabled = chosen;
            self.saved.seen = seen;
            if new_id {
                self.saved.install_id = random_hex();
                // What waited was made under the old number: it goes, so that number is never sent again.
                clear_waiting(&self.shared.folder);
                lock(&self.shared).install_id = self.saved.install_id.clone();
            }
            write_saved(&self.shared.folder, &self.saved);
        }
        let active = enabled && seen && self.shared.target.is_some();
        if active != self.active {
            self.active = active;
            ACTIVE.store(active, Ordering::Relaxed);
            if active {
                stats::switch_on();
                self.not_before = Instant::now() + self.grace;
            } else {
                // Off: nothing is counted, and everything that waited is deleted.
                stats::switch_off();
                self.collector = format::Collector::default();
                self.last_tick = None;
                clear_waiting(&self.shared.folder);
            }
        }
        let mut facts = facts.unwrap_or_default();
        if active {
            self.gather(&mut facts);
        }
        if preview {
            let report = self.running_report(&facts, false);
            let text = serde_json::to_string_pretty(&report).unwrap_or_default();
            let mut state = lock(&self.shared);
            state.preview = Some(text);
            self.shared.wake.notify_all();
        }
        let waited = Instant::now() >= self.not_before;
        if !active || !(waited || self.told_before) {
            return;
        }
        let crashed = crash_files(&self.shared.folder);
        let due = waited && self.last_report.is_none_or(|at| at.elapsed() >= EVERY);
        if !(quit || due || !crashed.is_empty()) {
            return;
        }
        let started = Instant::now();
        for file in crashed {
            self.queue_crash(&file, &facts);
        }
        if quit || due {
            self.last_report = Some(Instant::now());
            let up = self.began.elapsed().as_secs_f64();
            self.collector.close_minute(up, chrono::Utc::now().timestamp());
            let report = self.running_report(&facts, true);
            self.enqueue(report);
        }
        let budget = if quit { QUIT_BUDGET } else { Duration::from_secs(20) };
        let result = self.drain(budget.saturating_sub(started.elapsed()));
        let mut state = lock(&self.shared);
        match result {
            Ok(sent) => {
                state.sent += sent as u64;
                state.last_error = None;
            }
            Err(why) => state.last_error = Some(why),
        }
    }

    /// The readings of this round, into the rows that wait for the next report.
    fn gather(&mut self, facts: &mut Facts) {
        // A round woken between two readings (the window asked for something) takes none.
        let tick = self.last_tick.is_none_or(|at| at.elapsed() >= TICK - Duration::from_millis(500));
        // Five seconds became half a minute or more: the computer slept, or the app was stopped.
        if let Some(at) = self.last_tick.filter(|at| at.elapsed() > Duration::from_secs(30)) {
            stats::event_n("sleep", at.elapsed().as_secs_f64().round());
        }
        if let Some(file) = facts.video_file.clone() {
            facts.video_size = *self.video_sizes.entry(file.clone()).or_insert_with(|| sp_video::video_summary(&file));
        }
        if tick {
            self.last_tick = Some(Instant::now());
            let taken = stats::take();
            let sample = self.sampler.sample();
            let up = self.began.elapsed().as_secs_f64();
            self.collector.tick(up, chrono::Utc::now().timestamp(), sample, &taken, facts);
            if self.last_footprint.is_none_or(|at| at.elapsed() >= Duration::from_secs(60)) {
                self.sampler.ask_footprint();
                self.last_footprint = Some(Instant::now());
            }
        }
        self.collector.take_events(stats::take_events());
        self.collector.take_features(stats::take_features());
        let scrubber = &self.scrubber;
        let lines = stats::take_problems().iter().filter_map(|line| scrub::problem_line(line, scrubber)).collect();
        self.collector.take_problems(lines);
    }

    fn about<'a>(
        &'a self,
        hardware: &'a info::Hardware,
        gpu: Option<&'a GpuFacts>,
        facts: &'a Facts,
    ) -> format::About<'a> {
        format::About {
            install_id: &self.saved.install_id,
            session: &self.session,
            hardware,
            gpu,
            facts: Some(facts),
            ffmpeg: self.ffmpeg.unwrap_or(false),
            uptime_s: self.began.elapsed().as_secs(),
            now_unix: chrono::Utc::now().timestamp(),
        }
    }

    fn hardware(&mut self) -> info::Hardware {
        self.ffmpeg.get_or_insert_with(sp_video::ffmpeg_found);
        self.hardware.get_or_insert_with(info::Hardware::read).clone()
    }

    /// The report of how the app has been running. `taking` empties the rows (it is being
    /// sent); without it they are only looked at (See what's sent).
    fn running_report(&mut self, facts: &Facts, taking: bool) -> format::Report {
        let hardware = self.hardware();
        let gpu = lock(&self.shared).gpu.clone();
        let mut collector = std::mem::take(&mut self.collector);
        let report = format::running(&self.about(&hardware, gpu.as_ref(), facts), &mut collector, taking);
        self.collector = collector;
        report
    }

    fn queue_crash(&mut self, file: &Path, facts: &Facts) {
        let text = std::fs::read_to_string(file).ok();
        let _ = std::fs::remove_file(file);
        let Some(record) = text.and_then(|text| serde_json::from_str::<format::CrashRecord>(&text).ok()) else {
            return;
        };
        let hardware = self.hardware();
        let gpu = lock(&self.shared).gpu.clone();
        let report = format::crash(&self.about(&hardware, gpu.as_ref(), facts), &record, &self.scrubber);
        self.enqueue(report);
    }

    /// Keeps a report for sending, if it passes the format's check and fits.
    fn enqueue(&mut self, report: format::Report) {
        let Some(body) = format::fit(report) else {
            crate::applog::event("reports", "a report did not pass its own check and was not kept");
            return;
        };
        let folder = queue_folder(&self.shared.folder);
        if std::fs::create_dir_all(&folder).is_err() {
            return;
        }
        let name = format!("{:016}-{}.json.gz", chrono::Utc::now().timestamp_micros(), &random_hex()[..6]);
        let part = folder.join(format!("{name}.part"));
        if std::fs::write(&part, body).is_ok() {
            let _ = std::fs::rename(part, folder.join(name));
        }
        trim_queue(&folder, QUEUE_LIMIT);
    }

    /// Sends what waits, oldest first, within the time given. How many went; an error says why
    /// the first that did not.
    fn drain(&mut self, budget: Duration) -> Result<usize, String> {
        let Some(target) = self.shared.target.clone() else { return Ok(0) };
        let started = Instant::now();
        let mut sent = 0;
        let mut refused = None;
        for file in queue_files(&queue_folder(&self.shared.folder)).into_iter().take(A_ROUND) {
            if self.backoff.is_some_and(|(until, _)| Instant::now() < until) {
                return if sent > 0 { Ok(sent) } else { Err("waiting to try again".to_owned()) };
            }
            let left = budget.saturating_sub(started.elapsed());
            if left.is_zero() {
                break;
            }
            // Turned off since the round began: nothing more goes.
            if !lock(&self.shared).enabled {
                break;
            }
            let Ok(body) = std::fs::read(&file) else { continue };
            match post(&target, &body, left.min(Duration::from_secs(20))) {
                Outcome::Sent => {
                    let _ = std::fs::remove_file(&file);
                    self.backoff = None;
                    sent += 1;
                }
                // The server won't take this one (too big, badly made): trying it again would
                // never end, so it is dropped.
                Outcome::Rejected(why) => {
                    let _ = std::fs::remove_file(&file);
                    crate::applog::event("reports", &format!("a report was refused and dropped: {why}"));
                    refused = Some(why);
                }
                Outcome::Failed(why) => {
                    let step = self
                        .backoff
                        .map_or(Duration::from_secs(60), |(_, step)| (step * 2).min(Duration::from_secs(60 * 60)));
                    self.backoff = Some((Instant::now() + step, step));
                    return if sent > 0 { Ok(sent) } else { Err(why) };
                }
            }
        }
        if sent > 0 {
            crate::applog::event("reports", &format!("sent {sent} anonymous report(s)"));
        }
        match refused {
            Some(why) if sent == 0 => Err(why),
            _ => Ok(sent),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The files that wait

pub fn queue_folder(folder: &Path) -> PathBuf {
    folder.join("queue")
}

/// Crash records waiting in the folder, oldest first; more than a few are old news.
pub fn crash_files(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy().starts_with("crash-") && name.to_string_lossy().ends_with(".json")
            })
        })
        .collect();
    files.sort();
    while files.len() > 3 {
        let _ = std::fs::remove_file(files.remove(0));
    }
    files
}

/// Deletes everything that waits to be sent: the queue, and any crash written down.
pub fn clear_waiting(folder: &Path) {
    let _ = std::fs::remove_dir_all(queue_folder(folder));
    for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with("crash-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The files of the queue, oldest first.
pub fn queue_files(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "gz"))
        .collect();
    files.sort();
    files
}

/// Keeps the queue to `limit` bytes: the oldest go.
pub fn trim_queue(folder: &Path, limit: u64) {
    let mut files: Vec<(PathBuf, u64)> = queue_files(folder)
        .into_iter()
        .map(|path| {
            let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
            (path, size)
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size)| size).sum();
    while total > limit && !files.is_empty() {
        let (path, size) = files.remove(0);
        let _ = std::fs::remove_file(path);
        total -= size;
    }
}

// ---------------------------------------------------------------------------------------------
// Sending

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Sent,
    /// The server answered, and will not take it (400, 413 and the like).
    Rejected(String),
    /// Not delivered; worth trying again later.
    Failed(String),
}

/// What the server's answer means for the report that was sent.
pub fn outcome_of(status: u16) -> Outcome {
    match status {
        200..=299 => Outcome::Sent,
        408 | 429 => Outcome::Failed("the server asked to wait".to_owned()),
        code @ 400..=499 => Outcome::Rejected(format!("the server refused the report ({code})")),
        code => Outcome::Failed(format!("the server answered {code}")),
    }
}

/// Sends one report: the compressed JSON and two headers saying so, and nothing else (no key,
/// no cookie, no install header). Connecting may take five seconds and the whole exchange
/// `total`, at the most.
pub fn post(target: &Target, body: &[u8], total: Duration) -> Outcome {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5).min(total)))
        .timeout_global(Some(total.min(Duration::from_secs(20))))
        .https_only(!target.local)
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent("SoftPresenter")
        .build()
        .into();
    let response = agent
        .post(&target.url)
        .header("Content-Type", "application/json")
        .header("Content-Encoding", "gzip")
        .send(body);
    match response {
        Ok(response) => outcome_of(response.status().as_u16()),
        Err(_) => Outcome::Failed("no network".to_owned()),
    }
}

#[cfg(test)]
pub mod tests;
