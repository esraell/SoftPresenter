//! NDI for Linux, downloaded by the app.
//!
//! NDI Tools, which installs NDI's runtime on a Mac and on Windows, doesn't exist for Linux:
//! there the runtime comes only inside NDI's SDK for Linux. So on Linux, where NDI isn't
//! installed, Download NDI… fetches that SDK over HTTPS from NDI's own server, on a thread of
//! its own that says how far it has got and can be cancelled. The download is read through
//! ([`sdk`]) and NDI's licence in it is shown. Only when the operator presses Accept is NDI's
//! runtime for this computer's processor taken out, with the licence and notices that go with
//! it, into `NDI/` in the app's data folder: never a system folder, and no administrator is
//! needed. Cancel deletes the download, as a failure does, and an install deletes it once it is
//! done. sp-ndi looks in that folder too ([`sp_ndi::set_own_folder`]), so NDI screens start
//! without a restart. Remove deletes the folder.
//!
//! The download isn't checked against a checksum: NDI replaces the file in place with each
//! release. It is trusted for coming over HTTPS from NDI's own server at a fixed address, for
//! its size, and for being laid out as NDI's SDK is, with a runtime that is a shared library
//! for this processor.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};

use crate::i18n::{t, tf};

pub mod sdk;
use sdk::{Contents, Problem};

/// NDI's SDK for Linux, from NDI's own server.
const SDK_URL: &str = "https://downloads.ndi.tv/SDK/NDI_SDK_Linux/Install_NDI_SDK_v6_Linux.tar.gz";
/// What the app notes about the runtime it installed.
const INFO_FILE: &str = "installed.json";
/// The folder, in the app's NDI folder, that holds the download until it is installed.
const DOWNLOAD_FOLDER: &str = "download";
const DOWNLOAD_FILE: &str = "Install_NDI_SDK_v6_Linux.tar.gz";

/// The folder the app keeps the NDI it downloaded in.
pub fn folder() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("NDI"))
}

/// Where the SDK is downloaded from, and whether that is a test copy's stand-in.
fn source() -> &'static (String, bool) {
    static SOURCE: OnceLock<(String, bool)> = OnceLock::new();
    SOURCE.get_or_init(|| {
        // Testing aid: SOFTPRESENTER_NDI_SDK_URL=<address> downloads NDI's SDK from there
        // instead of NDI's server (plain http only from this computer), and offers Download
        // NDI… on any system, so a test copy can go the whole way through with a made-up SDK.
        // Only for a copy with a data folder of its own.
        let own_data = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
        match std::env::var("SOFTPRESENTER_NDI_SDK_URL") {
            Ok(address) if own_data && !address.is_empty() => (address, true),
            _ => (SDK_URL.to_owned(), false),
        }
    })
}

/// True where the app offers to download NDI: on Linux, where NDI Tools doesn't exist.
pub fn offered() -> bool {
    cfg!(target_os = "linux") || source().1
}

/// The runtime the app installed, and when.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    /// The day it was downloaded, as year-month-day.
    pub downloaded: String,
}

/// The runtime the app installed in `folder`, if there is one.
pub fn installed_in(folder: &Path) -> Option<Installed> {
    if !folder.join(sdk::RUNTIME_FILE).is_file() {
        return None;
    }
    let info = std::fs::read(folder.join(INFO_FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
    Some(info.unwrap_or_else(|| Installed { version: "6".to_owned(), downloaded: String::new() }))
}

/// Deletes a download a quit left behind part of the way through, on a thread of its own.
pub fn tidy(folder: &Path) {
    if folder.join(DOWNLOAD_FOLDER).exists() {
        let folder = folder.to_path_buf();
        let _ = std::thread::Builder::new().name("ndi tidy".to_owned()).spawn(move || forget_download(&folder));
    }
}

/// Where Download NDI… has got to.
enum Stage {
    Idle,
    /// Downloading, then reading the download through (how far is in [`Shared`]).
    Fetching,
    /// Waiting for the operator to accept NDI's licence, or cancel.
    Licence(Box<Ready>),
    Installing,
    Removing,
    Failed(Problem),
}

/// A download read through, waiting for its licence to be accepted.
struct Ready {
    download: PathBuf,
    contents: Contents,
    /// True once the window has scrolled to show Accept and Cancel.
    scrolled: bool,
}

/// How far a download has got, shared with its thread.
#[derive(Default)]
struct Shared {
    done: AtomicU64,
    /// 0 while the size isn't known.
    total: AtomicU64,
    /// True once the download is whole and being read through.
    checking: AtomicBool,
    cancel: AtomicBool,
}

/// How a thread's work ended.
enum Finished {
    Fetched(Result<Ready, Problem>),
    Installed(Result<Installed, Problem>),
    Removed(Result<(), String>),
}

/// What the operator is told after an install or a removal.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Note {
    Ready,
    Removed,
    /// Removed, but the runtime stays open until the app quits.
    RemovedInUse,
    /// It couldn't all be removed, and why.
    NotRemoved(String),
}

/// Download NDI…, as far as it has got. Kept on `HomeState`, so the Screens window and the
/// inspector show the same.
pub struct NdiDownload {
    stage: Stage,
    shared: Arc<Shared>,
    tx: Sender<Finished>,
    rx: Receiver<Finished>,
    /// The runtime the app installed: looked for on disk the first time it is asked for, then
    /// kept up to date.
    installed: Option<Option<Installed>>,
    note: Option<Note>,
    /// Set when an install or a removal has finished, for NDI to be looked for again.
    changed: bool,
    /// Set when an install has finished, for the screens that found no NDI to start again.
    restart: bool,
}

impl Default for NdiDownload {
    fn default() -> NdiDownload {
        let (tx, rx) = channel();
        NdiDownload {
            stage: Stage::Idle,
            shared: Arc::default(),
            tx,
            rx,
            installed: None,
            note: None,
            changed: false,
            restart: false,
        }
    }
}

impl std::fmt::Debug for NdiDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NdiDownload(installed: {:?}, note: {:?})", self.installed, self.note)
    }
}

/// What the operator asked for, done once the drawing is over.
enum Asked {
    Start,
    Cancel,
    Accept,
    Remove,
}

impl NdiDownload {
    fn busy(&self) -> bool {
        matches!(self.stage, Stage::Fetching | Stage::Installing | Stage::Removing)
    }

    /// True from Download NDI… until it is done with: downloading, at the licence, installing,
    /// or failed.
    pub fn under_way(&self) -> bool {
        !matches!(self.stage, Stage::Idle)
    }

    /// The runtime the app installed, if it did.
    pub fn installed(&mut self) -> Option<&Installed> {
        if self.installed.is_none() {
            self.installed = Some(folder().and_then(|folder| installed_in(&folder)));
        }
        self.installed.as_ref().and_then(Option::as_ref)
    }

    /// True once after an install or a removal has finished: NDI is to be looked for again.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// True once after an install has finished: the screens that found no NDI start again.
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    /// Starts downloading NDI's SDK. `wake` is called when it has been read through (or has
    /// failed), to redraw the window.
    fn start(&mut self, wake: impl FnOnce() + Send + 'static) {
        if self.busy() || matches!(self.stage, Stage::Licence(_)) {
            return;
        }
        let Some(folder) = folder() else {
            self.stage = Stage::Failed(Problem::Disk("there is no data folder".to_owned()));
            return;
        };
        self.shared = Arc::default();
        self.note = None;
        let (shared, tx) = (self.shared.clone(), self.tx.clone());
        let (address, stand_in) = source().clone();
        crate::applog::event("ndi", "downloading NDI's SDK for Linux");
        let work = move || {
            let fetched = fetch_and_read(&address, stand_in, &folder, &shared);
            let _ = tx.send(Finished::Fetched(fetched));
            wake();
        };
        self.stage = match std::thread::Builder::new().name("ndi download".to_owned()).spawn(work) {
            Ok(_) => Stage::Fetching,
            Err(e) => Stage::Failed(Problem::Disk(e.to_string())),
        };
    }

    /// The licence is accepted: installs the runtime, on a thread of its own.
    fn accept(&mut self, wake: impl FnOnce() + Send + 'static) {
        let Stage::Licence(ready) = std::mem::replace(&mut self.stage, Stage::Installing) else { return };
        let Some(folder) = folder() else {
            self.stage = Stage::Failed(Problem::Disk("there is no data folder".to_owned()));
            return;
        };
        let (shared, tx) = (self.shared.clone(), self.tx.clone());
        crate::applog::event("ndi", &format!("NDI's licence was accepted; installing NDI {}", ready.contents.version));
        let work = move || {
            let installed = install(&ready, &folder, &shared.cancel);
            forget_download(&folder);
            let _ = tx.send(Finished::Installed(installed));
            wake();
        };
        if let Err(e) = std::thread::Builder::new().name("ndi install".to_owned()).spawn(work) {
            self.stage = Stage::Failed(Problem::Disk(e.to_string()));
        }
    }

    /// Cancel: stops a download, or turns the licence down and deletes the download.
    fn cancel(&mut self) {
        match std::mem::replace(&mut self.stage, Stage::Idle) {
            // The thread stops, deletes what it downloaded, and says so.
            Stage::Fetching => {
                self.shared.cancel.store(true, Ordering::Relaxed);
                self.stage = Stage::Fetching;
            }
            Stage::Licence(_) => {
                crate::applog::event("ndi", "NDI's licence was not accepted; the download was deleted");
                // One file: deleted at once, so a new download can't meet it being deleted.
                if let Some(folder) = folder() {
                    forget_download(&folder);
                }
            }
            other => self.stage = other,
        }
    }

    /// Deletes the NDI the app installed, on a thread of its own.
    fn remove(&mut self, wake: impl FnOnce() + Send + 'static) {
        if self.busy() {
            return;
        }
        let Some(folder) = folder() else { return };
        let tx = self.tx.clone();
        self.note = None;
        let work = move || {
            let _ = tx.send(Finished::Removed(remove_in(&folder)));
            wake();
        };
        if std::thread::Builder::new().name("ndi remove".to_owned()).spawn(work).is_ok() {
            self.stage = Stage::Removing;
        }
    }

    /// Takes in what the threads have finished. Cheap: called every frame.
    pub fn poll(&mut self) {
        while let Ok(finished) = self.rx.try_recv() {
            self.stage = match finished {
                Finished::Fetched(Ok(ready)) => {
                    crate::applog::event(
                        "ndi",
                        &format!("NDI {} downloaded; showing NDI's licence", ready.contents.version),
                    );
                    Stage::Licence(Box::new(ready))
                }
                Finished::Fetched(Err(Problem::Cancelled)) => {
                    crate::applog::event("ndi", "the NDI download was cancelled");
                    Stage::Idle
                }
                Finished::Fetched(Err(problem)) | Finished::Installed(Err(problem)) => {
                    crate::applog::problem("ndi", &format!("NDI's SDK for Linux {problem}"));
                    Stage::Failed(problem)
                }
                Finished::Installed(Ok(installed)) => {
                    crate::applog::event("ndi", &format!("NDI {} installed for SoftPresenter", installed.version));
                    self.installed = Some(Some(installed));
                    self.note = Some(Note::Ready);
                    self.changed = true;
                    self.restart = true;
                    Stage::Idle
                }
                Finished::Removed(Ok(())) => {
                    let in_use = folder().zip(sp_ndi::opened_from()).is_some_and(|(own, open)| open.starts_with(own));
                    crate::applog::event("ndi", "the NDI SoftPresenter downloaded was removed");
                    self.installed = Some(None);
                    self.note = Some(if in_use { Note::RemovedInUse } else { Note::Removed });
                    self.changed = true;
                    Stage::Idle
                }
                Finished::Removed(Err(why)) => {
                    crate::applog::problem(
                        "ndi",
                        &format!("the NDI SoftPresenter downloaded could not be removed: {why}"),
                    );
                    // What is left is looked at again, so Remove is there to try again.
                    self.installed = None;
                    self.note = Some(Note::NotRemoved(why));
                    self.changed = true;
                    Stage::Idle
                }
            };
        }
    }
}

/// Downloads NDI's SDK into the download folder and reads it through. Whatever goes wrong, the
/// download is deleted.
fn fetch_and_read(address: &str, stand_in: bool, folder: &Path, shared: &Shared) -> Result<Ready, Problem> {
    let disk = |e: std::io::Error| Problem::Disk(e.to_string());
    let downloads = folder.join(DOWNLOAD_FOLDER);
    match std::fs::remove_dir_all(&downloads) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(disk(e)),
        _ => {}
    }
    std::fs::create_dir_all(&downloads).map_err(disk)?;
    let download = downloads.join(DOWNLOAD_FILE);
    let read = fetch(address, stand_in, &download, shared).and_then(|()| {
        shared.checking.store(true, Ordering::Relaxed);
        sdk::read(&download, std::env::consts::ARCH, &shared.cancel)
    });
    match read {
        Ok(contents) => Ok(Ready { download, contents, scrolled: false }),
        Err(problem) => {
            forget_download(folder);
            Err(problem)
        }
    }
}

/// Deletes the NDI the app installed in `folder`, and the folder.
fn remove_in(folder: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(folder) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// Deletes the download, and the app's NDI folder too if nothing else is in it.
fn forget_download(folder: &Path) {
    let _ = std::fs::remove_dir_all(folder.join(DOWNLOAD_FOLDER));
    let _ = std::fs::remove_dir(folder);
}

/// True for an address on NDI's own servers.
fn from_ndi(host: Option<&str>) -> bool {
    host.is_some_and(|host| {
        ["ndi.tv", "ndi.video"].iter().any(|own| host == *own || host.ends_with(&format!(".{own}")))
    })
}

/// True for a plain http address on this computer, which only a test copy's stand-in may use.
fn on_this_computer(address: &str) -> bool {
    let host = address.strip_prefix("http://").and_then(|rest| rest.split(['/', ':']).next());
    matches!(host, Some("127.0.0.1" | "localhost"))
}

/// Downloads `address` into the new file `into`, saying how far it has got in `shared`.
fn fetch(address: &str, stand_in: bool, into: &Path, shared: &Shared) -> Result<(), Problem> {
    use ureq::ResponseExt;
    let disk = |e: std::io::Error| Problem::Disk(e.to_string());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .https_only(!(stand_in && on_this_computer(address)))
        .max_redirects(3)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        // A download that stalls for a minute has failed; a slow one has up to an hour.
        .timeout_per_read(Some(Duration::from_secs(60)))
        .timeout_recv_body(Some(Duration::from_secs(60 * 60)))
        .build()
        .into();
    let mut response = agent.get(address).call().map_err(|e| Problem::Network(plain(&e).to_owned()))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Problem::Network(format!("NDI's server answered {status}")));
    }
    // A redirect may move it within NDI's servers, never elsewhere.
    if !stand_in && !from_ndi(response.get_uri().host()) {
        return Err(Problem::Damaged("the download came from somewhere other than NDI's server".to_owned()));
    }
    let total = response.body().content_length();
    if total.is_some_and(|total| total > sdk::LARGEST_DOWNLOAD) {
        return Err(Problem::TooLarge);
    }
    shared.total.store(total.unwrap_or(0), Ordering::Relaxed);
    let mut reader = response.body_mut().with_config().limit(sdk::LARGEST_DOWNLOAD + 1).reader();
    // A new file (never written through something already at that name).
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(into).map_err(disk)?;
    let (mut done, mut buffer) = (0u64, vec![0u8; 256 * 1024]);
    loop {
        if shared.cancel.load(Ordering::Relaxed) {
            return Err(Problem::Cancelled);
        }
        let read = reader.read(&mut buffer).map_err(|_| Problem::Network("the download broke off".to_owned()))?;
        if read == 0 {
            break;
        }
        done += read as u64;
        if done > sdk::LARGEST_DOWNLOAD {
            return Err(Problem::TooLarge);
        }
        file.write_all(&buffer[..read]).map_err(disk)?;
        shared.done.store(done, Ordering::Relaxed);
    }
    if total.is_some_and(|total| total != done) {
        return Err(Problem::Damaged(format!("cut short: {done} bytes of {}", total.unwrap_or(0))));
    }
    file.sync_all().map_err(disk)
}

/// An HTTP request's failure in plain words.
fn plain(e: &ureq::Error) -> &'static str {
    match e {
        ureq::Error::Timeout(_) => "it took too long",
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            "NDI's server couldn't be reached (is this computer online?)"
        }
        ureq::Error::Tls(_) => "the secure connection failed",
        _ => "the request failed",
    }
}

/// Takes the runtime out of the download into `folder`, the note of what was installed first,
/// so the runtime, put in place last, is never found without it.
fn install(ready: &Ready, folder: &Path, cancel: &AtomicBool) -> Result<Installed, Problem> {
    let installed = Installed {
        version: ready.contents.version.clone(),
        downloaded: chrono::Local::now().format("%Y-%m-%d").to_string(),
    };
    std::fs::create_dir_all(folder).map_err(|e| Problem::Disk(e.to_string()))?;
    let note = serde_json::to_vec_pretty(&installed).map_err(|e| Problem::Disk(e.to_string()))?;
    sdk::write_whole(folder, INFO_FILE, &note)?;
    let taken = sdk::install(&ready.download, &ready.contents, std::env::consts::ARCH, folder, cancel);
    if taken.is_err() {
        let _ = std::fs::remove_file(folder.join(INFO_FILE));
    }
    taken.map(|()| installed)
}

/// "NDI 6.3.2 installed (downloaded 24 Nov 2026)".
fn installed_words(installed: &Installed) -> String {
    let date = chrono::NaiveDate::parse_from_str(&installed.downloaded, "%Y-%m-%d")
        .map(|date| date.format("%-d %b %Y").to_string());
    match date {
        Ok(date) => tf(
            "screens.ndi.installed",
            "NDI {version} installed (downloaded {date})",
            &[("version", &installed.version), ("date", &date)],
        ),
        Err(_) => tf("screens.ndi.installed_undated", "NDI {version} installed", &[("version", &installed.version)]),
    }
}

/// What the operator is told when Download NDI… didn't work.
fn problem_words(problem: &Problem) -> String {
    match problem {
        Problem::Layout(_) => t(
            "screens.ndi.problem.layout",
            "NDI's download isn't laid out the way this version of SoftPresenter knows (NDI may have changed it), so nothing was installed.",
        )
        .to_owned(),
        Problem::Damaged(_) => t(
            "screens.ndi.problem.damaged",
            "The download is damaged or was cut short, so nothing was installed. Try again.",
        )
        .to_owned(),
        Problem::NoRuntime(processor) => tf(
            "screens.ndi.problem.processor",
            "NDI's download has no NDI for this computer's processor ({processor}).",
            &[("processor", processor)],
        ),
        Problem::TooLarge => t(
            "screens.ndi.problem.too_large",
            "The download is larger than NDI's should be, so it was stopped and deleted.",
        )
        .to_owned(),
        Problem::Disk(why) => {
            tf("screens.ndi.problem.disk", "NDI couldn't be saved on this computer: {why}.", &[("why", why)])
        }
        Problem::Network(why) => {
            tf("screens.ndi.problem.network", "NDI couldn't be downloaded: {why}.", &[("why", why)])
        }
        Problem::Cancelled => String::new(),
    }
}

/// Download NDI… and what follows it: how far the download has got (with Cancel), NDI's
/// licence (Accept, Cancel), installing; then which NDI the app installed and when, with
/// Remove. Shows nothing where the app doesn't offer the download, and only what it installed
/// where NDI was installed some other way.
pub fn ui(ui: &mut egui::Ui, download: &mut NdiDownload, ndi_found: bool) {
    if offered() {
        draw(ui, download, ndi_found);
    }
}

fn draw(ui: &mut egui::Ui, download: &mut NdiDownload, ndi_found: bool) {
    download.poll();
    let dim = crate::studio::TEXT_DIM;
    let small = |text: &str| RichText::new(text).size(11.5);
    let mut asked = None;
    let mut scrolled = false;
    let installed = if matches!(download.stage, Stage::Idle) { download.installed().cloned() } else { None };
    match &download.stage {
        Stage::Idle => {
            if let Some(installed) = installed {
                ui.horizontal_wrapped(|ui| {
                    ui.label(small(&installed_words(&installed)).color(dim));
                    if ui
                        .small_button(t("screens.ndi.remove", "Remove"))
                        .on_hover_text(t("screens.ndi.remove.hover", "Deletes the NDI that SoftPresenter downloaded"))
                        .clicked()
                    {
                        asked = Some(Asked::Remove);
                    }
                });
            } else if !ndi_found
                && ui
                    .button(t("screens.ndi.download", "Download NDI\u{2026}"))
                    .on_hover_text(t(
                        "screens.ndi.download.hover",
                        "Downloads NDI's SDK for Linux from NDI's website, shows NDI's licence, and installs NDI for SoftPresenter only",
                    ))
                    .clicked()
            {
                asked = Some(Asked::Start);
            }
            let (note, ink) = match &download.note {
                Some(Note::Ready) => {
                    (t("screens.ndi.ready", "NDI is ready: screens sent over NDI start now.").to_owned(), dim)
                }
                Some(Note::Removed) => (t("screens.ndi.removed", "NDI was removed.").to_owned(), dim),
                Some(Note::RemovedInUse) => (
                    t("screens.ndi.removed_in_use", "NDI was removed. Restart SoftPresenter to stop using it.")
                        .to_owned(),
                    dim,
                ),
                Some(Note::NotRemoved(why)) => (
                    tf("screens.ndi.not_removed", "NDI couldn't all be removed: {why}.", &[("why", why)]),
                    crate::studio::CARE,
                ),
                None => (String::new(), dim),
            };
            if !note.is_empty() {
                ui.label(small(&note).color(ink));
            }
        }
        Stage::Fetching => {
            let shared = &download.shared;
            ui.horizontal_wrapped(|ui| {
                if shared.checking.load(Ordering::Relaxed) {
                    ui.add(egui::Spinner::new().size(12.0));
                    ui.label(small(t("screens.ndi.checking", "Checking the download\u{2026}")).color(dim));
                } else {
                    let (done, total) = (shared.done.load(Ordering::Relaxed), shared.total.load(Ordering::Relaxed));
                    let megabytes = |bytes: u64| crate::i18n::count(bytes.div_ceil(1_000_000));
                    let words = if total > 0 {
                        tf(
                            "screens.ndi.downloading",
                            "Downloading NDI\u{2026} {done} of {total} MB",
                            &[("done", &megabytes(done)), ("total", &megabytes(total))],
                        )
                    } else {
                        tf(
                            "screens.ndi.downloading_unsized",
                            "Downloading NDI\u{2026} {done} MB",
                            &[("done", &megabytes(done))],
                        )
                    };
                    ui.label(small(&words).color(dim));
                }
                if ui.small_button(t("screens.ndi.cancel", "Cancel")).clicked() {
                    asked = Some(Asked::Cancel);
                }
            });
            if !shared.checking.load(Ordering::Relaxed) {
                let (done, total) = (shared.done.load(Ordering::Relaxed), shared.total.load(Ordering::Relaxed));
                let part = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                ui.add(egui::ProgressBar::new(part.clamp(0.0, 1.0)).desired_height(6.0));
            }
        }
        Stage::Licence(ready) => {
            ui.label(
                small(&tf(
                    "screens.ndi.licence.about",
                    "NDI {version} is downloaded. SoftPresenter installs it only if you accept NDI's licence, below. Cancel deletes the download.",
                    &[("version", &ready.contents.version)],
                ))
                .color(dim),
            );
            egui::Frame::new().fill(Color32::from_black_alpha(70)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("ndi licence")
                    .min_scrolled_height(220.0)
                    .max_height(220.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&ready.contents.licence).size(11.0).color(crate::studio::TEXT),
                            )
                            .wrap(),
                        );
                    });
            });
            ui.horizontal(|ui| {
                if ui.button(t("screens.ndi.cancel", "Cancel")).clicked() {
                    asked = Some(Asked::Cancel);
                }
                if ui.button(t("screens.ndi.accept", "Accept")).clicked() {
                    asked = Some(Asked::Accept);
                }
            });
            // The first time it shows, the window scrolls to bring Accept and Cancel into view.
            if !ready.scrolled {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                scrolled = true;
            }
        }
        Stage::Installing | Stage::Removing => {
            let words = if matches!(download.stage, Stage::Installing) {
                t("screens.ndi.installing", "Installing NDI\u{2026}")
            } else {
                t("screens.ndi.removing", "Removing NDI\u{2026}")
            };
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(small(words).color(dim));
            });
        }
        Stage::Failed(problem) => {
            ui.label(small(&problem_words(problem)).color(crate::studio::CARE));
            if !ndi_found && ui.button(t("screens.ndi.try_again", "Try Again")).clicked() {
                asked = Some(Asked::Start);
            }
        }
    }
    if scrolled && let Stage::Licence(ready) = &mut download.stage {
        ready.scrolled = true;
    }
    let wake = {
        let ctx = ui.ctx().clone();
        move || ctx.request_repaint()
    };
    match asked {
        Some(Asked::Start) => download.start(wake),
        Some(Asked::Cancel) => download.cancel(),
        Some(Asked::Accept) => download.accept(wake),
        Some(Asked::Remove) => download.remove(wake),
        None => {}
    }
    if download.busy() {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::sdk::fixture;
    use super::*;

    /// Answers one request on this computer with the status, headers and body given, and
    /// returns its address.
    fn serve_once(status: &str, length: Option<usize>, body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/Install_NDI_SDK_v6_Linux.tar.gz", listener.local_addr().unwrap());
        let status = status.to_owned();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let length = length.map(|n| format!("Content-Length: {n}\r\n")).unwrap_or_default();
            let head = format!("HTTP/1.1 {status}\r\n{length}Connection: close\r\n\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        });
        address
    }

    fn empty_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-download-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_download_is_fetched_read_through_and_installed_only_when_asked() {
        let body = fixture::download(&fixture::sdk_items());
        let address = serve_once("200 OK", Some(body.len()), body.clone());
        let folder = empty_folder("whole").join("NDI");
        let shared = Shared::default();
        let ready = fetch_and_read(&address, true, &folder, &shared).unwrap();
        assert_eq!(shared.done.load(Ordering::Relaxed), body.len() as u64);
        assert_eq!(shared.total.load(Ordering::Relaxed), body.len() as u64);
        assert!(ready.contents.licence.starts_with("TEST LICENCE"));
        // Read through, nothing is installed yet: only the download is there.
        assert_eq!(installed_in(&folder), None);
        let names: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [DOWNLOAD_FOLDER]);
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let installed = install(&ready, &folder, &AtomicBool::new(false)).unwrap();
        assert_eq!(installed.version, "6.3.2");
        assert_eq!(installed_in(&folder), Some(installed.clone()));
        assert!(folder.join(sdk::LICENCE_FILE).is_file() && folder.join(sdk::NOTICES_FILE).is_file());
        let words = installed_words(&Installed { version: "6.3.2".to_owned(), downloaded: "2026-11-24".to_owned() });
        assert_eq!(words, "NDI 6.3.2 installed (downloaded 24 Nov 2026)");
        // Remove takes it all away, the folder too; again, there is nothing to do.
        remove_in(&folder).unwrap();
        assert_eq!(installed_in(&folder), None);
        assert!(!folder.exists());
        assert_eq!(remove_in(&folder), Ok(()));
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn a_download_cut_short_refused_or_too_large_is_deleted_and_said() {
        let body = fixture::download(&fixture::sdk_items());
        let folder = empty_folder("cut").join("NDI");
        let address = serve_once("200 OK", Some(body.len() + 1000), body.clone());
        let problem = fetch_and_read(&address, true, &folder, &Shared::default()).err().unwrap();
        assert!(matches!(problem, Problem::Damaged(_) | Problem::Network(_)), "{problem:?}");
        assert!(!folder.join(DOWNLOAD_FOLDER).exists(), "the download is deleted");
        let address = serve_once("404 Not Found", Some(9), b"not here!".to_vec());
        assert_eq!(
            fetch_and_read(&address, true, &folder, &Shared::default()).err(),
            Some(Problem::Network("NDI's server answered 404".to_owned()))
        );
        let address = serve_once("200 OK", Some(usize::try_from(sdk::LARGEST_DOWNLOAD).unwrap() + 1), Vec::new());
        assert_eq!(fetch_and_read(&address, true, &folder, &Shared::default()).err(), Some(Problem::TooLarge));
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn a_cancelled_download_stops_and_is_deleted() {
        let body = fixture::download(&fixture::sdk_items());
        let address = serve_once("200 OK", Some(body.len()), body);
        let folder = empty_folder("cancel").join("NDI");
        let shared = Shared::default();
        shared.cancel.store(true, Ordering::Relaxed);
        assert_eq!(fetch_and_read(&address, true, &folder, &shared).err(), Some(Problem::Cancelled));
        assert!(!folder.join(DOWNLOAD_FOLDER).exists());
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn only_ndis_own_servers_count_as_ndis() {
        assert!(from_ndi(Some("downloads.ndi.tv")));
        assert!(from_ndi(Some("ndi.video")));
        for other in ["ndi.tv.example.com", "notndi.tv", "example.com"] {
            assert!(!from_ndi(Some(other)), "{other}");
        }
        assert!(!from_ndi(None));
        // Plain http, for a test copy's stand-in, only on this computer.
        assert!(on_this_computer("http://127.0.0.1:47811/sdk.tar.gz"));
        assert!(on_this_computer("http://localhost/sdk.tar.gz"));
        for other in ["http://127.0.0.1.example.com/x", "http://example.com/x", "https://127.0.0.1/x"] {
            assert!(!on_this_computer(other), "{other}");
        }
    }

    #[test]
    fn each_step_draws_without_trouble() {
        let ctx = egui::Context::default();
        let mut download = NdiDownload { installed: Some(None), ..NdiDownload::default() };
        // Drawn as on Linux, whatever this computer is.
        let draw = |download: &mut NdiDownload| {
            let mut drawn = ctx.run_ui(egui::RawInput::default(), |ui| super::draw(ui, download, false));
            drawn.textures_delta.clear();
        };
        draw(&mut download);
        download.stage = Stage::Fetching;
        download.shared.total.store(67_470_104, Ordering::Relaxed);
        download.shared.done.store(12_000_000, Ordering::Relaxed);
        draw(&mut download);
        download.shared.checking.store(true, Ordering::Relaxed);
        draw(&mut download);
        let contents = sdk::read(
            &{
                let path = empty_folder("draw").join("sdk.tar.gz");
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, fixture::download(&fixture::sdk_items())).unwrap();
                path
            },
            "x86_64",
            &AtomicBool::new(false),
        )
        .unwrap();
        download.stage =
            Stage::Licence(Box::new(Ready { download: PathBuf::from("/nowhere"), contents, scrolled: false }));
        draw(&mut download);
        download.stage = Stage::Failed(Problem::Layout("test".to_owned()));
        draw(&mut download);
        download.stage = Stage::Idle;
        download.installed = Some(Some(Installed { version: "6.3.2".to_owned(), downloaded: "2026-11-24".to_owned() }));
        draw(&mut download);
        let _ = std::fs::remove_dir_all(empty_folder("draw"));
    }

    #[test]
    fn the_words_for_each_problem_say_nothing_was_installed_or_why() {
        assert!(problem_words(&Problem::Layout(String::new())).contains("nothing was installed"));
        assert!(problem_words(&Problem::Damaged(String::new())).contains("nothing was installed"));
        assert!(problem_words(&Problem::NoRuntime("riscv64".to_owned())).contains("riscv64"));
        assert_eq!(problem_words(&Problem::Cancelled), "");
    }
}
