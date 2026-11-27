//! Installing, updating and removing NDI from inside the app.
//!
//! NDI's runtime is NDI's own software: SoftPresenter never carries it. Where it isn't
//! installed, Install NDI (Settings › Integrations › NDI, under a screen sent over NDI, beside
//! an NDI Source box, in the setup's Screens step) opens one window ([`window`]) that downloads
//! NDI from NDI's own server over HTTPS, on a thread of its own that says how far it has got
//! and can be cancelled, and then installs it the way that system's download allows ([`Way`]):
//!
//! - **Linux** ([`sdk`]): the runtime comes only inside NDI's SDK for Linux. The download is
//!   read through, NDI's licence in it is shown, and only when the operator presses Accept is
//!   the runtime for this computer's processor taken out, with the licence and notices that go
//!   with it, into `NDI/` in the app's data folder.
//! - **Mac** ([`mac`]): NDI's runtime package is checked (NDI's signature, by macOS's own
//!   tools), laid out without installing it, and its library copied into the same folder. The
//!   package holds no licence text, so the Licence step names NDI's licence and links to it.
//! - **Windows** ([`windows`]): NDI's runtime can only be installed by NDI's own installer, as
//!   an administrator. Its signature is checked, then it is opened as it is; Windows asks for
//!   the administrator's yes, never SoftPresenter.
//!
//! Nothing is ever written to a system folder by SoftPresenter, and it never asks for a
//! password. sp-ndi looks in the app's folder too ([`sp_ndi::set_own_folder`]), so NDI screens
//! start without a restart. Cancel deletes the download, as a failure does, and an install
//! deletes it once it is done. Remove deletes the folder (on Windows, opens NDI's uninstaller).
//!
//! **Updates** ([`check`]) run in the card's row with no window, unless NDI's licence changed
//! since it was accepted. The new runtime is unpacked and checked in a folder of its own
//! (`NDI/next`) and only then takes the old one's place: at once when NDI isn't in use, or
//! else the next time SoftPresenter starts ([`settle`]), because the runtime that is open
//! stays open until the app quits and live NDI screens must keep working.
//!
//! The download isn't checked against a checksum: NDI replaces each file in place with every
//! release. It is trusted for coming over HTTPS from NDI's own server at a fixed address, for
//! its size, and then: on Linux for being laid out as NDI's SDK is, with a runtime that is a
//! shared library for this processor; on a Mac and Windows for NDI's signature on it, which
//! the system checks.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod check;
pub mod mac;
pub mod row;
pub mod sdk;
pub mod window;
pub mod windows;

use check::{Checked, Marks};
use sdk::Problem;

/// NDI's SDK for Linux, from NDI's own server.
const SDK_URL: &str = "https://downloads.ndi.tv/SDK/NDI_SDK_Linux/Install_NDI_SDK_v6_Linux.tar.gz";
/// NDI's licence on NDI's own site, which a Mac's Licence step links to (the address NDI's
/// own notices file gives).
pub const LICENCE_URL: &str = "https://ndi.link/ndisdk_license";
/// What the app notes about the runtime it installed.
const INFO_FILE: &str = "installed.json";
/// The folder, in the app's NDI folder, that holds the download until it is installed.
const DOWNLOAD_FOLDER: &str = "download";
/// The folder, in the app's NDI folder, a new runtime is unpacked and checked in before it
/// takes the old one's place.
const NEXT_FOLDER: &str = "next";
const SDK_FILE: &str = "Install_NDI_SDK_v6_Linux.tar.gz";
const PACKAGE_FILE: &str = "libNDI_for_Mac.pkg";

/// How NDI gets onto this computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// Unpacked from NDI's SDK for Linux into the app's folder.
    Sdk,
    /// Taken out of NDI's Mac package into the app's folder.
    Package,
    /// NDI's own installer for Windows, opened as it is.
    Installer,
}

impl Way {
    /// True where NDI's own installer does the installing: two steps, no licence step (the
    /// installer shows the licence), and the "installer was closed" failure.
    pub fn installer(self) -> bool {
        self == Way::Installer
    }

    fn address(self) -> &'static str {
        match self {
            Way::Sdk => SDK_URL,
            Way::Package => mac::ADDRESS,
            Way::Installer => windows::ADDRESS,
        }
    }

    fn file(self) -> &'static str {
        match self {
            Way::Sdk => SDK_FILE,
            Way::Package => PACKAGE_FILE,
            Way::Installer => windows::DOWNLOAD_FILE,
        }
    }

    fn largest(self) -> u64 {
        match self {
            Way::Sdk => sdk::LARGEST_DOWNLOAD,
            Way::Package => mac::LARGEST_DOWNLOAD,
            Way::Installer => windows::LARGEST_DOWNLOAD,
        }
    }

    /// About how large the download is, in megabytes, for the window's first step (NDI's
    /// files as they were in November 2026).
    pub fn megabytes(self) -> u64 {
        match self {
            Way::Sdk => 68,
            Way::Package => 5,
            Way::Installer => 12,
        }
    }
}

/// The names a runtime is kept under in the app's folder, whichever way it came.
const RUNTIME_FILES: [&str; 2] = [sdk::RUNTIME_FILE, mac::RUNTIME_FILE];

/// The folder the app keeps the NDI it downloaded in.
pub fn folder() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("NDI"))
}

/// Where NDI is downloaded from and how it is installed here, and whether that is a test
/// copy's stand-in.
fn source() -> &'static (String, Way, bool) {
    static SOURCE: OnceLock<(String, Way, bool)> = OnceLock::new();
    SOURCE.get_or_init(|| {
        // Testing aid: SOFTPRESENTER_NDI_SDK_URL=<address> downloads NDI's SDK for Linux from
        // there instead of NDI's server (plain http only from this computer), and installs the
        // Linux way on any system, so a test copy can go the whole way through with a made-up
        // SDK. Only for a copy with a data folder of its own.
        let own_data = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
        match std::env::var("SOFTPRESENTER_NDI_SDK_URL") {
            Ok(address) if own_data && !address.is_empty() => (address, Way::Sdk, true),
            _ => {
                let way = if cfg!(target_os = "macos") {
                    Way::Package
                } else if cfg!(windows) {
                    Way::Installer
                } else {
                    Way::Sdk
                };
                (way.address().to_owned(), way, false)
            }
        }
    })
}

/// How NDI is installed on this computer.
pub fn way() -> Way {
    source().1
}

/// The key that opens the Install NDI window from the places that need NDI: "Install NDI…".
pub fn install_key() -> &'static str {
    crate::i18n::t("integrations.ndi.key.install_more", "Install NDI\u{2026}")
}

/// True in a test copy: quiet, with a data folder of its own, or saving a picture.
fn test_copy() -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    set("SOFTPRESENTER_QUIET")
        || set("SOFTPRESENTER_DATA")
        || std::env::vars_os().any(|(name, _)| name.to_string_lossy().starts_with("SOFTPRESENTER_CAPTURE"))
        || std::env::args().any(|word| word == "--capture-home")
}

/// The runtime the app installed, and when.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    /// The day it was downloaded, as year-month-day.
    pub downloaded: String,
    /// What NDI's server said of the download it came from, to tell a newer one by.
    #[serde(default)]
    pub marks: Marks,
    /// The SHA-256 of the licence the operator accepted for it (as hexadecimal), to tell when
    /// NDI's licence has changed. Empty for an install from before it was noted.
    #[serde(default)]
    pub licence: String,
}

/// The runtime the app installed in `folder`, if there is one.
pub fn installed_in(folder: &Path) -> Option<Installed> {
    if !RUNTIME_FILES.iter().any(|name| folder.join(name).is_file()) {
        return None;
    }
    let info = std::fs::read(folder.join(INFO_FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
    Some(info.unwrap_or_else(|| Installed { version: "6".to_owned(), ..Installed::default() }))
}

/// A new runtime waiting in `folder` to take the old one's place at the next start.
fn waiting_in(folder: &Path) -> Option<Installed> {
    installed_in(&folder.join(NEXT_FOLDER)).filter(|_| folder.join(NEXT_FOLDER).join(INFO_FILE).is_file())
}

/// As the app starts, before NDI is looked for: a new runtime that was unpacked and checked
/// while the old one was in use takes its place (a few renames), anything half-done is
/// deleted, and a download a quit left behind is deleted on a thread of its own.
pub fn settle(folder: &Path) {
    let next = folder.join(NEXT_FOLDER);
    if next.exists() {
        match move_in(folder) {
            Ok(Some(installed)) => {
                crate::applog::event("ndi", &format!("NDI {} took the old one's place", installed.version));
            }
            Ok(None) => {}
            Err(problem) => crate::applog::problem("ndi", &format!("the new NDI could not take its place: {problem}")),
        }
        let _ = std::fs::remove_dir_all(&next);
    }
    if folder.join(DOWNLOAD_FOLDER).exists() {
        let folder = folder.to_path_buf();
        let _ = std::thread::Builder::new().name("ndi tidy".to_owned()).spawn(move || forget_download(&folder));
    }
}

/// Moves a whole new runtime from `folder/next` into `folder`: the licence and notices, the
/// note of what it is, and the runtime last, each in one step, so the runtime is never found
/// half-written or without its note. Nothing moves unless the new one is whole (its note is
/// written last when it is unpacked). The runtime that was there is replaced, never written
/// through: a copy of it that is open stays as it was.
fn move_in(folder: &Path) -> Result<Option<Installed>, Problem> {
    let next = folder.join(NEXT_FOLDER);
    let Some(installed) = waiting_in(folder) else { return Ok(None) };
    for name in [sdk::LICENCE_FILE, sdk::NOTICES_FILE, INFO_FILE, RUNTIME_FILES[0], RUNTIME_FILES[1]] {
        let from = next.join(name);
        if std::fs::symlink_metadata(&from).is_ok_and(|about| about.is_file()) {
            std::fs::rename(&from, folder.join(name)).map_err(Problem::disk)?;
        }
    }
    let _ = std::fs::remove_dir_all(&next);
    Ok(Some(installed))
}

/// The window's steps, for where a failure happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Download,
    Install,
}

/// Where the Settings window and the home window each show the Install NDI window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Home,
    Settings,
    /// The Screens window, where a screen's own settings are.
    Screens,
}

/// How far installing NDI has got.
enum Stage {
    Idle,
    /// The window's first step: what NDI is, before anything is downloaded.
    About,
    /// Downloading, then reading the download through (how far is in [`Shared`]).
    Fetching,
    /// Waiting for the operator to accept NDI's licence, or cancel.
    Licence(Box<Ready>),
    /// Unpacking into the app's folder, or NDI's own installer open.
    Installing,
    /// The window's last step. The version is NDI's, where the download says it.
    Done {
        version: Option<String>,
        waits: bool,
    },
    Failed {
        problem: Problem,
        step: Step,
    },
    Removing,
}

/// What a download holds that SoftPresenter takes, by the way it is installed.
enum Taken {
    Sdk(sdk::Contents),
    Package(mac::Contents),
    /// NDI's installer, its signature checked.
    Installer,
}

/// A download read through, waiting for its licence to be accepted.
struct Ready {
    download: PathBuf,
    taken: Taken,
    marks: Marks,
}

impl Ready {
    /// NDI's licence text, where the download carries it.
    fn licence(&self) -> Option<&str> {
        match &self.taken {
            Taken::Sdk(contents) => Some(&contents.licence),
            Taken::Package(contents) => contents.licence.as_deref(),
            Taken::Installer => None,
        }
    }

    fn version(&self) -> Option<&str> {
        match &self.taken {
            Taken::Sdk(contents) => Some(&contents.version),
            Taken::Package(contents) => Some(&contents.version),
            Taken::Installer => None,
        }
    }
}

/// The SHA-256 of a licence's text as hexadecimal; of nothing, where the download has no text
/// and the Licence step links to NDI's own page.
fn licence_mark(licence: Option<&str>) -> String {
    Sha256::digest(licence.unwrap_or("").as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// How far a download has got, shared with its thread.
#[derive(Default)]
struct Shared {
    done: AtomicU64,
    /// 0 while the size isn't known.
    total: AtomicU64,
    /// True once the download is whole and being read through.
    checking: AtomicBool,
    /// Cancel, and Stop Waiting.
    cancel: AtomicBool,
}

/// How a thread's work ended.
enum Finished {
    Fetched(Result<Ready, Problem>),
    /// What was installed (none where NDI's own installer did it), and whether it waits for
    /// the next start.
    Installed(Result<(Option<Installed>, bool), Problem>),
    Removed(Result<(), String>),
    /// What NDI's server says of its download, and whether that is newer than what is here.
    Checked(Result<(Marks, bool), Problem>),
    Version(Option<String>),
}

/// What the operator is told once, under the card's rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// "NDI wasn't installed. What was downloaded has been deleted."
    NotInstalled,
    /// "The update was cancelled. What was downloaded has been deleted."
    UpdateCancelled,
    /// "NDI is updated to 6.2.0. Screens sent over NDI use it from the next start."
    Updated(Option<String>),
    Removed,
    /// Removed, but the runtime stays open until the app quits.
    RemovedInUse,
    /// It couldn't all be removed, and why.
    NotRemoved(String),
    /// NDI's server couldn't be asked whether there is a newer NDI.
    NotChecked,
}

/// Installing NDI, as far as it has got. Kept on `HomeState`, so every place that offers it
/// shows the same.
pub struct NdiDownload {
    stage: Stage,
    /// True while what is under way is an update, which runs in the card's row.
    update: bool,
    /// Where the Install NDI window is shown, while it is.
    shown: Option<Place>,
    /// True once the window's licence box has been given the keyboard.
    licence_focused: bool,
    shared: Arc<Shared>,
    tx: Sender<Finished>,
    rx: Receiver<Finished>,
    /// The runtime the app installed: looked for on disk the first time it is asked for, then
    /// kept up to date.
    installed: Option<Option<Installed>>,
    /// A new runtime waiting for the next start, looked for the same way.
    waiting: Option<Option<Installed>>,
    /// The last check for a newer NDI, read from the data folder the first time.
    checked: Option<Option<Checked>>,
    checking: bool,
    /// When an update was put in this run, as the row says it ("2:07").
    updated_at: Option<String>,
    /// The version the runtime on this computer says it is, once a thread has asked it.
    asked_version: Option<Option<String>>,
    note: Option<Note>,
    /// Set when an install or a removal has finished, for NDI to be looked for again.
    changed: bool,
    /// Set when an install has finished, for the screens that found no NDI to start again.
    restart: bool,
    /// A test's own data folder, in place of the app's.
    data: Option<PathBuf>,
    /// Whether NDI counts as installed while a made-up state is shown ([`NdiDownload::demo`]).
    demo_found: Option<bool>,
}

impl Default for NdiDownload {
    fn default() -> NdiDownload {
        let (tx, rx) = channel();
        NdiDownload {
            stage: Stage::Idle,
            update: false,
            shown: None,
            licence_focused: false,
            shared: Arc::default(),
            tx,
            rx,
            installed: None,
            waiting: None,
            checked: None,
            checking: false,
            updated_at: None,
            asked_version: None,
            note: None,
            changed: false,
            restart: false,
            data: None,
            demo_found: None,
        }
    }
}

impl std::fmt::Debug for NdiDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NdiDownload(installed: {:?}, note: {:?})", self.installed, self.note)
    }
}

/// The time of day as the card says it: "2:07".
fn clock() -> String {
    chrono::Local::now().format("%-I:%M").to_string()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl NdiDownload {
    /// For a test: one that keeps its files in `data`, never in the app's own data folder.
    #[cfg(test)]
    pub fn in_folder(data: PathBuf) -> NdiDownload {
        NdiDownload { data: Some(data), ..NdiDownload::default() }
    }

    /// The app's data folder, where the last check is noted.
    fn data(&self) -> Option<PathBuf> {
        self.data.clone().or_else(crate::bible_store::data_dir)
    }

    /// The folder the app keeps the NDI it downloaded in.
    fn folder(&self) -> Option<PathBuf> {
        self.data().map(|data| data.join("NDI"))
    }

    fn busy(&self) -> bool {
        matches!(self.stage, Stage::Fetching | Stage::Installing | Stage::Removing)
    }

    /// The runtime the app installed, if it did.
    pub fn installed(&mut self) -> Option<&Installed> {
        if self.installed.is_none() {
            self.installed = Some(self.folder().and_then(|folder| installed_in(&folder)));
        }
        self.installed.as_ref().and_then(Option::as_ref)
    }

    /// A new runtime that takes the old one's place the next time SoftPresenter starts.
    fn waiting(&mut self) -> Option<&Installed> {
        if self.waiting.is_none() {
            self.waiting = Some(self.folder().and_then(|folder| waiting_in(&folder)));
        }
        self.waiting.as_ref().and_then(Option::as_ref)
    }

    fn checked(&mut self) -> Option<&Checked> {
        if self.checked.is_none() {
            self.checked = Some(self.data().and_then(|data| Checked::read(&data)));
        }
        self.checked.as_ref().and_then(Option::as_ref)
    }

    /// The version of the NDI on this computer, as far as it is known: of a new one waiting
    /// for the next start, else what the open runtime says, else what the app noted when it
    /// installed it, else what a thread asked the runtime ([`NdiDownload::ask_version`]).
    pub fn version(&mut self) -> Option<String> {
        if let Some(waiting) = self.waiting() {
            return Some(waiting.version.clone());
        }
        if let Some(open) = sp_ndi::version() {
            return Some(open.to_owned());
        }
        if let Some(installed) = self.installed() {
            return Some(installed.version.clone());
        }
        self.asked_version.clone().flatten()
    }

    /// Asks the runtime its version on a thread of its own, once, where NDI is installed and
    /// nothing says its version yet. Opening the runtime may take a moment, so never on the
    /// window's thread.
    pub fn ask_version(&mut self, ndi_found: bool, wake: impl FnOnce() + Send + 'static) {
        if !ndi_found || self.asked_version.is_some() || self.version().is_some() || cfg!(test) {
            return;
        }
        self.asked_version = Some(None);
        let tx = self.tx.clone();
        let _ = std::thread::Builder::new().name("ndi version".to_owned()).spawn(move || {
            let _ = tx.send(Finished::Version(sp_ndi::ask_version()));
            wake();
        });
    }

    /// Whether NDI counts as installed, while a made-up state is shown.
    pub fn demo_found(&self) -> Option<bool> {
        self.demo_found
    }

    /// Testing aid: `SOFTPRESENTER_CAPTURE_PREFERENCES=ndi-<state>` (in Settings, on NDI's
    /// page) and `SOFTPRESENTER_NDI_DEMO=<state>` (in the home window) put the card or the
    /// Install NDI window in a state with made-up numbers, to see it in a picture without NDI's
    /// server: `none`, `installed`, `newer`, `updating`, `updatefail`, `updated`, `install`,
    /// `downloading`, `licence`, `licencelink`, `installing`, `done`, `update-done`, and
    /// `fail-network`, `-blocked`, `-disk`, `-file`, `-closed`. Nothing is downloaded, written
    /// or installed. Only in a copy with a data folder of its own. Whether NDI counts as
    /// installed in that state.
    pub fn demo(&mut self, name: &str, place: Place) -> bool {
        let own_data = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
        if !own_data {
            return sp_ndi::installed();
        }
        let here = |version: &str| Some(Some(Installed { version: version.to_owned(), ..Installed::default() }));
        let found = !matches!(name, "none" | "install" | "downloading" | "licence" | "licencelink" | "installing")
            && !name.starts_with("fail-");
        self.installed = if found { here("6.1.1") } else { Some(None) };
        self.waiting = Some(None);
        self.checked = Some(None);
        self.asked_version = Some(None);
        self.shared = Arc::default();
        self.shown = Some(place);
        self.update = false;
        let failed = |problem, step| Stage::Failed { problem, step };
        self.stage = match name {
            "newer" => {
                self.checked = Some(Some(Checked { at: now(), newer: true, marks: Marks::default() }));
                Stage::Idle
            }
            "installed" => {
                self.checked = Some(Some(Checked { at: now(), newer: false, marks: Marks::default() }));
                Stage::Idle
            }
            "updating" | "downloading" => {
                self.update = name == "updating";
                let part: u64 = if self.update { 30_600_000 } else { 41_480_000 };
                self.shared.total.store(68_000_000, Ordering::Relaxed);
                self.shared.done.store(part, Ordering::Relaxed);
                Stage::Fetching
            }
            "updatefail" => {
                self.update = true;
                failed(Problem::Blocked, Step::Download)
            }
            "updated" => {
                self.installed = here("6.2.0");
                self.updated_at = Some("2:07".to_owned());
                self.note = Some(Note::Updated(Some("6.2.0".to_owned())));
                Stage::Idle
            }
            "install" => Stage::About,
            "licence" | "licencelink" => {
                let sample = "(NDI\u{2019}s licence shows here word for word, read from the download. This sample stands in for it.)";
                let text = (1..=12).map(|n| format!("\u{a7} {n}. {sample}")).collect::<Vec<_>>().join("\n\n");
                let taken = if name == "licence" { Taken::Sdk(sdk::Contents::sample(text)) } else { Taken::Installer };
                Stage::Licence(Box::new(Ready { download: PathBuf::new(), taken, marks: Marks::default() }))
            }
            "installing" => Stage::Installing,
            "done" => Stage::Done { version: Some("6.1.1".to_owned()), waits: false },
            "update-done" => Stage::Done { version: Some("6.2.0".to_owned()), waits: true },
            "fail-network" => failed(Problem::Offline, Step::Download),
            "fail-blocked" => failed(Problem::Blocked, Step::Download),
            "fail-disk" => failed(Problem::DiskFull, Step::Download),
            "fail-file" => failed(Problem::Damaged("a made-up failure".to_owned()), Step::Download),
            "fail-closed" => failed(Problem::Closed, Step::Install),
            _ => Stage::Idle,
        };
        // An update, and the card's own states, have no window.
        if self.update || matches!(self.stage, Stage::Idle) {
            self.shown = None;
        }
        self.demo_found = Some(found);
        found
    }

    /// True once after an install or a removal has finished: NDI is to be looked for again.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// True once after an install has finished: the screens that found no NDI start again.
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    /// Where the Install NDI window is shown, while it is.
    pub fn shown_in(&self) -> Option<Place> {
        self.shown
    }

    /// Install NDI…, from any of its places: the window opens there at its first step, or
    /// where it has got to if an install is already under way.
    pub fn open(&mut self, place: Place) {
        if matches!(self.stage, Stage::Idle) {
            self.stage = Stage::About;
            self.update = false;
            self.note = None;
        }
        if !matches!(self.stage, Stage::Removing) {
            self.shown = Some(place);
            self.licence_focused = false;
        }
    }

    /// Esc: the window goes away and what is under way carries on; at a step where nothing is
    /// under way, it is the step's own way out.
    pub fn hide(&mut self) {
        match self.stage {
            Stage::About => self.cancel(),
            Stage::Done { .. } => self.finish(),
            Stage::Failed { .. } => self.cancel(),
            _ => self.shown = None,
        }
    }

    /// Done, on the window's last step.
    pub fn finish(&mut self) {
        if matches!(self.stage, Stage::Done { .. }) {
            self.stage = Stage::Idle;
        }
        self.shown = None;
    }

    /// Download, and Try Again: starts downloading NDI. `wake` is called when it has been
    /// read through (or has failed), to redraw the window.
    pub fn begin(&mut self, wake: impl FnOnce() + Send + 'static) {
        if self.busy() || matches!(self.stage, Stage::Licence(_)) {
            return;
        }
        let Some(folder) = self.folder() else {
            self.stage =
                Stage::Failed { problem: Problem::Disk("there is no data folder".to_owned()), step: Step::Download };
            return;
        };
        self.shared = Arc::default();
        self.note = None;
        let (shared, tx) = (self.shared.clone(), self.tx.clone());
        let (address, way, stand_in) = source().clone();
        crate::applog::event("ndi", "downloading NDI from NDI's server");
        let work = move || {
            let fetched = fetch_and_read(&address, way, stand_in, &folder, &shared);
            let _ = tx.send(Finished::Fetched(fetched));
            wake();
        };
        self.stage = match std::thread::Builder::new().name("ndi download".to_owned()).spawn(work) {
            Ok(_) => Stage::Fetching,
            Err(e) => Stage::Failed { problem: Problem::Disk(e.to_string()), step: Step::Download },
        };
    }

    /// Install Update, and its Try Again: the same download, in the card's row with no window.
    pub fn update(&mut self, wake: impl FnOnce() + Send + 'static) {
        if self.busy() || matches!(self.stage, Stage::Licence(_) | Stage::About) {
            return;
        }
        self.update = true;
        self.shown = None;
        self.stage = Stage::Idle;
        self.begin(wake);
    }

    /// The licence is accepted (or, for an update, is the one accepted before; or NDI's own
    /// installer shows it): installs, on a thread of its own.
    pub fn accept(&mut self, wake: impl FnOnce() + Send + 'static) {
        let Stage::Licence(ready) = std::mem::replace(&mut self.stage, Stage::Installing) else { return };
        self.install(*ready, wake);
    }

    fn install(&mut self, ready: Ready, wake: impl FnOnce() + Send + 'static) {
        self.stage = Stage::Installing;
        let Some(folder) = self.folder() else {
            self.stage =
                Stage::Failed { problem: Problem::Disk("there is no data folder".to_owned()), step: Step::Install };
            return;
        };
        let (shared, tx) = (self.shared.clone(), self.tx.clone());
        let version = ready.version().unwrap_or("").to_owned();
        crate::applog::event("ndi", &format!("NDI's licence was accepted; installing NDI {version}"));
        let in_use = sp_ndi::opened_from().is_some_and(|open| open.starts_with(&folder));
        let work = move || {
            let installed = install(&ready, &folder, in_use, &shared.cancel);
            forget_download(&folder);
            let _ = tx.send(Finished::Installed(installed));
            wake();
        };
        if let Err(e) = std::thread::Builder::new().name("ndi install".to_owned()).spawn(work) {
            self.stage = Stage::Failed { problem: Problem::Disk(e.to_string()), step: Step::Install };
        }
    }

    /// Cancel, Close and Stop Waiting: stops a download, turns the licence down, or gives up
    /// waiting for NDI's installer; what was downloaded is deleted.
    pub fn cancel(&mut self) {
        match std::mem::replace(&mut self.stage, Stage::Idle) {
            // The thread stops, deletes what it downloaded, and says so.
            Stage::Fetching => {
                self.shared.cancel.store(true, Ordering::Relaxed);
                self.stage = Stage::Fetching;
            }
            // NDI's installer is left to itself; SoftPresenter stops waiting for it.
            Stage::Installing if way().installer() => {
                self.shared.cancel.store(true, Ordering::Relaxed);
                self.stage = Stage::Installing;
            }
            Stage::Licence(_) => {
                crate::applog::event("ndi", "NDI's licence was not accepted; the download was deleted");
                // One file: deleted at once, so a new download can't meet it being deleted.
                if let Some(folder) = self.folder() {
                    forget_download(&folder);
                }
                self.cancelled();
            }
            Stage::About => self.shown = None,
            Stage::Failed { .. } => {
                self.shown = None;
                self.update = false;
            }
            other => self.stage = other,
        }
    }

    /// What a cancelled install or update leaves: nothing under way, and one line saying so.
    fn cancelled(&mut self) {
        self.note = Some(if self.update { Note::UpdateCancelled } else { Note::NotInstalled });
        self.update = false;
        self.shown = None;
    }

    /// Remove: deletes the NDI the app installed (on Windows, opens NDI's own uninstaller), on
    /// a thread of its own.
    pub fn remove(&mut self, wake: impl FnOnce() + Send + 'static) {
        if self.busy() {
            return;
        }
        let Some(folder) = self.folder() else { return };
        let tx = self.tx.clone();
        self.note = None;
        let shared = self.shared.clone();
        let work = move || {
            let removed = if way().installer() {
                match sp_ndi::locate().and_then(|runtime| windows::uninstaller_for(&runtime)) {
                    Some(uninstaller) => {
                        windows::open_and_wait(&uninstaller, &[], &shared.cancel).map_err(|problem| problem.to_string())
                    }
                    None => Err("NDI's uninstaller isn't where NDI's installer puts it".to_owned()),
                }
            } else {
                remove_in(&folder)
            };
            let _ = tx.send(Finished::Removed(removed));
            wake();
        };
        if std::thread::Builder::new().name("ndi remove".to_owned()).spawn(work).is_ok() {
            self.stage = Stage::Removing;
        }
    }

    /// True where Remove… has something of the app's to remove: the NDI in its own folder, or
    /// on Windows the NDI that NDI's runtime installer put there (never NDI Tools).
    pub fn removable(&mut self) -> bool {
        if way().installer() {
            return sp_ndi::locate().is_some_and(|runtime| windows::uninstaller_for(&runtime).is_some());
        }
        self.installed().is_some() || self.waiting().is_some()
    }

    /// Check for Updates: asks NDI's server about its download, on a thread of its own.
    pub fn check(&mut self, wake: impl FnOnce() + Send + 'static) {
        if self.checking || self.busy() {
            return;
        }
        let (address, _, stand_in) = source().clone();
        // What is here to compare with: the marks noted for the app's own install (a new one
        // waiting for the next start counts as here), else the runtime file's date.
        let noted = self.waiting().map(|waiting| waiting.marks.clone());
        let noted = noted.or_else(|| self.installed().map(|installed| installed.marks.clone()));
        self.checking = true;
        self.note = None;
        let tx = self.tx.clone();
        let work = move || {
            let found = check::head(&address, stand_in).map(|marks| {
                let newer = noted
                    .as_ref()
                    .and_then(|noted| check::newer_than_installed(noted, &marks))
                    .or_else(|| {
                        let file = sp_ndi::locate().and_then(|runtime| check::file_date(&runtime))?;
                        check::newer_than_file(file, &marks)
                    })
                    .unwrap_or(false);
                (marks, newer)
            });
            let _ = tx.send(Finished::Checked(found));
            wake();
        };
        if std::thread::Builder::new().name("ndi check".to_owned()).spawn(work).is_err() {
            self.checking = false;
        }
    }

    /// The check made when the NDI card is opened: at most once a day, only where NDI is
    /// installed, and never from a test copy unless it asks its own stand-in.
    pub fn check_if_due(&mut self, ndi_found: bool, wake: impl FnOnce() + Send + 'static) {
        if !ndi_found || cfg!(test) || (test_copy() && !source().2) {
            return;
        }
        if Checked::due(self.checked(), now()) {
            self.check(wake);
        }
    }

    /// Takes in what the threads have finished. Cheap: called every frame.
    pub fn poll(&mut self) {
        while let Ok(finished) = self.rx.try_recv() {
            match finished {
                Finished::Fetched(Ok(ready)) => self.fetched(ready),
                Finished::Fetched(Err(Problem::Cancelled)) => {
                    crate::applog::event("ndi", "the NDI download was cancelled");
                    self.stage = Stage::Idle;
                    self.cancelled();
                }
                Finished::Fetched(Err(problem)) => self.failed(problem, Step::Download),
                Finished::Installed(Err(Problem::Cancelled)) => {
                    crate::applog::event("ndi", "stopped waiting for NDI's installer");
                    self.stage = Stage::Idle;
                    self.cancelled();
                }
                Finished::Installed(Err(problem)) => self.failed(problem, Step::Install),
                Finished::Installed(Ok((installed, waits))) => self.installed_now(installed, waits),
                Finished::Removed(removed) => self.removed(removed),
                Finished::Checked(found) => self.checked_now(found),
                Finished::Version(version) => self.asked_version = Some(version),
            }
        }
    }

    /// A download is read through: its licence is shown, or, where it is the one accepted
    /// before (an update) or NDI's own installer shows it, the install starts at once.
    fn fetched(&mut self, ready: Ready) {
        crate::applog::event("ndi", &format!("NDI {} downloaded", ready.version().unwrap_or("")));
        let accepted = self.installed().map(|installed| installed.licence.clone()).unwrap_or_default();
        let same = self.update && !accepted.is_empty() && accepted == licence_mark(ready.licence());
        if way().installer() || same {
            // The install's thread wakes the window that the download's thread woke last.
            self.install(ready, || {});
        } else {
            // An update whose licence changed opens the window at its Licence step.
            if self.update && self.shown.is_none() {
                self.shown = Some(Place::Settings);
            }
            self.licence_focused = false;
            self.stage = Stage::Licence(Box::new(ready));
        }
    }

    fn failed(&mut self, problem: Problem, step: Step) {
        crate::applog::problem("ndi", &format!("NDI's download {problem}"));
        self.stage = Stage::Failed { problem, step };
    }

    fn installed_now(&mut self, installed: Option<Installed>, waits: bool) {
        let version = installed.as_ref().map(|installed| installed.version.clone());
        crate::applog::event(
            "ndi",
            &format!(
                "NDI {} installed{}",
                version.as_deref().unwrap_or(""),
                if waits { "; it takes the old one's place at the next start" } else { "" }
            ),
        );
        // What is on disk is looked at again: in the app's folder, or waiting beside it.
        self.installed = None;
        self.waiting = None;
        self.asked_version = None;
        self.changed = true;
        self.restart = true;
        // Nothing newer than what was just downloaded.
        if let Some(data) = self.data() {
            let marks = installed.map(|installed| installed.marks).unwrap_or_default();
            let checked = Checked { at: now(), newer: false, marks };
            checked.write(&data);
            self.checked = Some(Some(checked));
        }
        if self.update {
            self.update = false;
            self.updated_at = Some(clock());
            self.note = Some(Note::Updated(version));
            self.stage = Stage::Idle;
            self.shown = None;
        } else {
            self.stage = Stage::Done { version, waits };
        }
    }

    fn removed(&mut self, removed: Result<(), String>) {
        self.stage = Stage::Idle;
        self.changed = true;
        self.asked_version = None;
        self.updated_at = None;
        match removed {
            Ok(()) => {
                let in_use = sp_ndi::opened_from()
                    .is_some_and(|open| way().installer() || self.folder().is_some_and(|own| open.starts_with(own)));
                crate::applog::event("ndi", "NDI was removed");
                self.installed = Some(None);
                self.waiting = Some(None);
                self.checked = Some(None);
                if let Some(data) = self.data() {
                    Checked::forget(&data);
                }
                self.note = Some(if in_use { Note::RemovedInUse } else { Note::Removed });
            }
            Err(why) => {
                crate::applog::problem("ndi", &format!("NDI could not be removed: {why}"));
                // What is left is looked at again, so Remove is there to try again.
                self.installed = None;
                self.waiting = None;
                self.note = Some(Note::NotRemoved(why));
            }
        }
    }

    fn checked_now(&mut self, found: Result<(Marks, bool), Problem>) {
        self.checking = false;
        self.updated_at = None;
        match found {
            Ok((marks, newer)) => {
                crate::applog::event(
                    "ndi",
                    if newer { "NDI's download has changed: a newer NDI may be out" } else { "no newer NDI was found" },
                );
                let checked = Checked { at: now(), newer, marks };
                if let Some(data) = self.data() {
                    checked.write(&data);
                }
                self.checked = Some(Some(checked));
            }
            Err(problem) => {
                crate::applog::problem("ndi", &format!("NDI's server couldn't be asked for a newer NDI: {problem}"));
                self.note = Some(Note::NotChecked);
            }
        }
    }
}

/// Downloads NDI into the download folder and reads it through. Whatever goes wrong, the
/// download is deleted.
fn fetch_and_read(address: &str, way: Way, stand_in: bool, folder: &Path, shared: &Shared) -> Result<Ready, Problem> {
    let downloads = folder.join(DOWNLOAD_FOLDER);
    match std::fs::remove_dir_all(&downloads) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(Problem::disk(e)),
        _ => {}
    }
    std::fs::create_dir_all(&downloads).map_err(Problem::disk)?;
    let download = downloads.join(way.file());
    let read = fetch(address, way, stand_in, &download, shared).and_then(|marks| {
        shared.checking.store(true, Ordering::Relaxed);
        let taken = match way {
            Way::Sdk => Taken::Sdk(sdk::read(&download, std::env::consts::ARCH, &shared.cancel)?),
            Way::Package => {
                let scratch = downloads.join("package");
                std::fs::create_dir_all(&scratch).map_err(Problem::disk)?;
                let tools = mac::Tools::system();
                Taken::Package(mac::read(&tools, &download, &scratch, std::env::consts::ARCH, &shared.cancel)?)
            }
            Way::Installer => {
                let powershell = std::env::var_os("SystemRoot")
                    .map(|windows| PathBuf::from(windows).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
                    .unwrap_or_else(|| PathBuf::from("powershell.exe"));
                windows::check_signature(&powershell, &download)?;
                Taken::Installer
            }
        };
        Ok((taken, marks))
    });
    match read {
        Ok((taken, marks)) => Ok(Ready { download, taken, marks }),
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

/// What asks NDI's server: HTTPS only (plain http only for a test copy's stand-in on this
/// computer), a few redirects at most, and times after which it gives up.
fn agent(address: &str, stand_in: bool, whole: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .https_only(!(stand_in && on_this_computer(address)))
        .max_redirects(3)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        // A download that stalls for a minute has failed; a slow one has up to an hour.
        .timeout_per_read(Some(Duration::from_secs(60)))
        .timeout_recv_body(Some(whole))
        .build()
        .into()
}

/// True when an error is the secure connection's certificate being refused: what a network
/// that looks into secure connections with a certificate of its own makes of NDI's server.
/// (The handshake's error reaches ureq inside an input-output error; a test makes a real one.)
fn certificate_refused(error: &ureq::Error) -> bool {
    use tokio_rustls::rustls;
    let refused = |error: &rustls::Error| matches!(error, rustls::Error::InvalidCertificate(_));
    match error {
        ureq::Error::Rustls(error) => refused(error),
        ureq::Error::Io(io) => {
            io.get_ref().and_then(|inner| inner.downcast_ref::<rustls::Error>()).is_some_and(refused)
        }
        _ => false,
    }
}

/// Why a request to NDI's server failed: no way to it, its certificate refused, or anything
/// else (in plain words, for the log).
fn request_problem(error: &ureq::Error) -> Problem {
    if certificate_refused(error) {
        return Problem::Blocked;
    }
    match error {
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => Problem::Offline,
        ureq::Error::Timeout(_) => Problem::Network("NDI's server took too long".to_owned()),
        other => Problem::Network(format!("the request failed ({other})").chars().take(200).collect()),
    }
}

/// Downloads `address` into the new file `into`, saying how far it has got in `shared`, and
/// gives what the server said of the download.
fn fetch(address: &str, way: Way, stand_in: bool, into: &Path, shared: &Shared) -> Result<Marks, Problem> {
    use ureq::ResponseExt;
    let largest = way.largest();
    let mut response =
        agent(address, stand_in, Duration::from_secs(60 * 60)).get(address).call().map_err(|e| request_problem(&e))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Problem::Network(format!("NDI's server answered {status}")));
    }
    // A redirect may move it within NDI's servers, never elsewhere.
    if !stand_in && !from_ndi(response.get_uri().host()) {
        return Err(Problem::Damaged("the download came from somewhere other than NDI's server".to_owned()));
    }
    let total = response.body().content_length();
    if total.is_some_and(|total| total > largest) {
        return Err(Problem::TooLarge);
    }
    let header = |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok());
    let marks = Marks::of(header("etag"), header("last-modified"), total);
    shared.total.store(total.unwrap_or(0), Ordering::Relaxed);
    let mut reader = response.body_mut().with_config().limit(largest + 1).reader();
    // A new file (never written through something already at that name).
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(into).map_err(Problem::disk)?;
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
        if done > largest {
            return Err(Problem::TooLarge);
        }
        file.write_all(&buffer[..read]).map_err(Problem::disk)?;
        shared.done.store(done, Ordering::Relaxed);
    }
    if total.is_some_and(|total| total != done) {
        return Err(Problem::Damaged(format!("cut short: {done} bytes of {}", total.unwrap_or(0))));
    }
    file.sync_all().map_err(Problem::disk)?;
    Ok(marks)
}

/// Installs what a download holds. In the app's folder (Linux, a Mac): the runtime is
/// unpacked and checked in `folder/next`, its note written last, and only then takes its
/// place in `folder`; while the runtime there is `in_use` it waits for the next start instead
/// (true is given back). On Windows NDI's own installer is opened and waited for.
fn install(
    ready: &Ready,
    folder: &Path,
    in_use: bool,
    cancel: &AtomicBool,
) -> Result<(Option<Installed>, bool), Problem> {
    let next = folder.join(NEXT_FOLDER);
    match std::fs::remove_dir_all(&next) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(Problem::disk(e)),
        _ => {}
    }
    let unpacked = match &ready.taken {
        Taken::Installer => return windows::open_and_wait(&ready.download, &[], cancel).map(|()| (None, false)),
        Taken::Sdk(contents) => sdk::install(&ready.download, contents, std::env::consts::ARCH, &next, cancel),
        Taken::Package(contents) => mac::install(contents, &next),
    };
    let installed = Installed {
        version: ready.version().unwrap_or("6").to_owned(),
        downloaded: chrono::Local::now().format("%Y-%m-%d").to_string(),
        marks: ready.marks.clone(),
        licence: licence_mark(ready.licence()),
    };
    let moved = unpacked
        .and_then(|()| serde_json::to_vec_pretty(&installed).map_err(|e| Problem::Disk(e.to_string())))
        .and_then(|note| sdk::write_whole(&next, INFO_FILE, &note))
        .and_then(|()| if in_use { Ok(()) } else { move_in(folder).map(|_| ()) });
    match moved {
        Ok(()) => Ok((Some(installed), in_use)),
        Err(problem) => {
            // What was there before stays as it was.
            let _ = std::fs::remove_dir_all(&next);
            Err(problem)
        }
    }
}

/// For tests of the places that open the Install NDI window.
#[cfg(test)]
pub mod testing {
    use egui::{Event, PointerButton, Rect, pos2, vec2};

    fn texts(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
        match shape {
            egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| texts(shape, into)),
            _ => {}
        }
    }

    /// Draws with `draw` in a window 1100 by 700 and gives every text drawn and where.
    pub fn drawn(ctx: &egui::Context, events: Vec<Event>, draw: &mut dyn FnMut(&mut egui::Ui)) -> Vec<(String, Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 700.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| draw(ui));
        output.textures_delta.clear();
        let mut found = Vec::new();
        output.shapes.iter().for_each(|clipped| texts(&clipped.shape, &mut found));
        found
    }

    /// Draws, then clicks the middle of the text that is exactly `words`. False when no such
    /// text was drawn.
    pub fn click(ctx: &egui::Context, words: &str, draw: &mut dyn FnMut(&mut egui::Ui)) -> bool {
        drawn(ctx, Vec::new(), draw);
        let found = drawn(ctx, Vec::new(), draw);
        let Some((_, place)) = found.iter().find(|(text, _)| text == words) else { return false };
        let at = place.center();
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        drawn(ctx, vec![Event::PointerMoved(at)], draw);
        drawn(ctx, vec![press(true)], draw);
        drawn(ctx, vec![press(false)], draw);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::sdk::fixture;
    use super::*;

    /// Answers one request on this computer with the status, headers and body given, and
    /// returns its address.
    fn serve_once(status: &str, length: Option<usize>, body: Vec<u8>) -> String {
        serve_with(status, length, "", body)
    }

    fn serve_with(status: &str, length: Option<usize>, more: &str, body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/Install_NDI_SDK_v6_Linux.tar.gz", listener.local_addr().unwrap());
        let (status, more) = (status.to_owned(), more.to_owned());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let length = length.map(|n| format!("Content-Length: {n}\r\n")).unwrap_or_default();
            let head = format!("HTTP/1.1 {status}\r\n{length}{more}Connection: close\r\n\r\n");
            let _ = stream.write_all(head.as_bytes());
            if !request[..read].starts_with(b"HEAD") {
                let _ = stream.write_all(&body);
            }
        });
        address
    }

    fn empty_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-download-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    const MARKS: &str = "ETag: \"abc123\"\r\nLast-Modified: Wed, 18 Nov 2026 15:58:50 GMT\r\n";

    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .map(|entries| entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    fn this_processor() -> bool {
        matches!(std::env::consts::ARCH, "x86_64" | "aarch64")
    }

    #[test]
    fn a_download_is_fetched_read_through_and_installed_only_when_asked() {
        let body = fixture::download(&fixture::sdk_items());
        let address = serve_with("200 OK", Some(body.len()), MARKS, body.clone());
        let folder = empty_folder("whole").join("NDI");
        let shared = Shared::default();
        let ready = fetch_and_read(&address, Way::Sdk, true, &folder, &shared).unwrap();
        assert_eq!(shared.done.load(Ordering::Relaxed), body.len() as u64);
        assert_eq!(shared.total.load(Ordering::Relaxed), body.len() as u64);
        assert!(ready.licence().unwrap().starts_with("TEST LICENCE"));
        // What the server said of the download is noted, to tell a newer one by.
        assert_eq!(ready.marks.etag.as_deref(), Some("\"abc123\""));
        assert_eq!(ready.marks.size, Some(body.len() as u64));
        // Read through, nothing is installed yet: only the download is there.
        assert_eq!(installed_in(&folder), None);
        assert_eq!(names(&folder), [DOWNLOAD_FOLDER]);
        if !this_processor() {
            return;
        }
        let (installed, waits) = install(&ready, &folder, false, &AtomicBool::new(false)).unwrap();
        let installed = installed.unwrap();
        assert!(!waits);
        assert_eq!(installed.version, "6.3.2");
        assert_eq!(installed.licence, licence_mark(ready.licence()));
        assert_eq!(installed_in(&folder), Some(installed.clone()));
        assert!(folder.join(sdk::LICENCE_FILE).is_file() && folder.join(sdk::NOTICES_FILE).is_file());
        assert!(!folder.join(NEXT_FOLDER).exists(), "nothing is left where it was unpacked");
        // Remove takes it all away, the folder too; again, there is nothing to do.
        remove_in(&folder).unwrap();
        assert_eq!(installed_in(&folder), None);
        assert!(!folder.exists());
        assert_eq!(remove_in(&folder), Ok(()));
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn an_update_takes_the_old_ones_place_only_once_it_is_whole_and_waits_while_ndi_is_in_use() {
        if !this_processor() {
            return;
        }
        let none = AtomicBool::new(false);
        let folder = empty_folder("update").join("NDI");
        let fetch = |items: &[fixture::Item<'_>]| {
            let body = fixture::download(items);
            let address = serve_with("200 OK", Some(body.len()), MARKS, body);
            fetch_and_read(&address, Way::Sdk, true, &folder, &Shared::default()).unwrap()
        };
        let old = fetch(&fixture::sdk_items());
        install(&old, &folder, false, &none).unwrap();
        let old_runtime = std::fs::read(folder.join(sdk::RUNTIME_FILE)).unwrap();
        // A newer SDK: the same, with a library named for another version and other bytes.
        let processor = if std::env::consts::ARCH == "x86_64" {
            (62, "x86_64-linux-gnu")
        } else {
            (183, "aarch64-rpi4-linux-gnueabi")
        };
        let name = format!("NDI SDK for Linux/lib/{}/libndi.so.6.4.0", processor.1);
        let library = fixture::library(processor.0, 90_000);
        let mut newer: Vec<fixture::Item<'_>> =
            fixture::sdk_items().into_iter().filter(|item| !item.name.contains("/lib/")).collect();
        newer.push(fixture::file(&name, &library));

        // A download that breaks while it is unpacked changes nothing that was there.
        let broken = fetch(&newer);
        std::fs::write(&broken.download, b"no longer NDI's download").unwrap();
        assert!(install(&broken, &folder, false, &none).is_err());
        assert_eq!(std::fs::read(folder.join(sdk::RUNTIME_FILE)).unwrap(), old_runtime);
        assert_eq!(installed_in(&folder).unwrap().version, "6.3.2");
        assert!(!folder.join(NEXT_FOLDER).exists());

        // While NDI is in use, the new one is unpacked whole beside the old and waits.
        let waiting = fetch(&newer);
        let (installed, waits) = install(&waiting, &folder, true, &none).unwrap();
        assert!(waits);
        assert_eq!(installed.unwrap().version, "6.4.0");
        assert_eq!(std::fs::read(folder.join(sdk::RUNTIME_FILE)).unwrap(), old_runtime, "live screens keep theirs");
        assert_eq!(installed_in(&folder).unwrap().version, "6.3.2");
        assert_eq!(waiting_in(&folder).unwrap().version, "6.4.0");
        // The next start puts it in place, before NDI is looked for.
        settle(&folder);
        assert_eq!(installed_in(&folder).unwrap().version, "6.4.0");
        assert_eq!(std::fs::read(folder.join(sdk::RUNTIME_FILE)).unwrap(), library);
        assert_eq!(waiting_in(&folder), None);
        assert!(!folder.join(NEXT_FOLDER).exists());

        // A new one left half-unpacked by a quit (no note yet) is deleted, never moved in.
        std::fs::create_dir_all(folder.join(NEXT_FOLDER)).unwrap();
        std::fs::write(folder.join(NEXT_FOLDER).join(sdk::RUNTIME_FILE), b"half").unwrap();
        settle(&folder);
        assert_eq!(std::fs::read(folder.join(sdk::RUNTIME_FILE)).unwrap(), library);
        assert!(!folder.join(NEXT_FOLDER).exists());

        // Not in use, an update goes in at once.
        let again = fetch(&fixture::sdk_items());
        let (_, waits) = install(&again, &folder, false, &none).unwrap();
        assert!(!waits);
        assert_eq!(installed_in(&folder).unwrap().version, "6.3.2");
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn a_download_cut_short_refused_or_too_large_is_deleted_and_said() {
        let body = fixture::download(&fixture::sdk_items());
        let folder = empty_folder("cut").join("NDI");
        let address = serve_once("200 OK", Some(body.len() + 1000), body.clone());
        let problem = fetch_and_read(&address, Way::Sdk, true, &folder, &Shared::default()).err().unwrap();
        assert!(matches!(problem, Problem::Damaged(_) | Problem::Network(_)), "{problem:?}");
        assert!(!folder.join(DOWNLOAD_FOLDER).exists(), "the download is deleted");
        let address = serve_once("404 Not Found", Some(9), b"not here!".to_vec());
        assert_eq!(
            fetch_and_read(&address, Way::Sdk, true, &folder, &Shared::default()).err(),
            Some(Problem::Network("NDI's server answered 404".to_owned()))
        );
        let address = serve_once("200 OK", Some(usize::try_from(sdk::LARGEST_DOWNLOAD).unwrap() + 1), Vec::new());
        assert_eq!(
            fetch_and_read(&address, Way::Sdk, true, &folder, &Shared::default()).err(),
            Some(Problem::TooLarge)
        );
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn a_cancelled_download_stops_and_is_deleted() {
        let body = fixture::download(&fixture::sdk_items());
        let address = serve_once("200 OK", Some(body.len()), body);
        let folder = empty_folder("cancel").join("NDI");
        let shared = Shared::default();
        shared.cancel.store(true, Ordering::Relaxed);
        assert_eq!(fetch_and_read(&address, Way::Sdk, true, &folder, &shared).err(), Some(Problem::Cancelled));
        assert!(!folder.join(DOWNLOAD_FOLDER).exists());
        assert!(!folder.exists(), "the app's NDI folder too, with nothing else in it");
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn no_way_to_the_server_is_said_as_no_internet() {
        // Nothing listens there: the connection is refused.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/sdk.tar.gz", listener.local_addr().unwrap());
        drop(listener);
        let folder = empty_folder("offline").join("NDI");
        assert_eq!(fetch_and_read(&address, Way::Sdk, true, &folder, &Shared::default()).err(), Some(Problem::Offline));
        assert_eq!(check::head(&address, true).err(), Some(Problem::Offline));
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    /// A secure server on this computer whose certificate nobody vouches for, as a network
    /// that looks into secure connections presents in NDI's place. Answers one connection.
    fn serve_untrusted() -> String {
        use tokio_rustls::rustls;
        let key = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
        let certificate = params.self_signed(&key).unwrap();
        let key =
            rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()));
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![certificate.der().clone()], key)
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("https://localhost:{}/sdk.tar.gz", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut connection = rustls::ServerConnection::new(Arc::new(config)).unwrap();
            let mut secure = rustls::Stream::new(&mut connection, &mut stream);
            let mut request = [0u8; 1024];
            let _ = secure.read(&mut request);
        });
        address
    }

    #[test]
    fn a_certificate_nobody_vouches_for_is_said_as_blocked_not_as_no_internet() {
        let folder = empty_folder("blocked").join("NDI");
        let problem = fetch_and_read(&serve_untrusted(), Way::Sdk, true, &folder, &Shared::default()).err();
        assert_eq!(problem, Some(Problem::Blocked));
        assert_eq!(check::head(&serve_untrusted(), true).err(), Some(Problem::Blocked));
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn a_full_disk_is_told_from_other_trouble_writing() {
        assert_eq!(Problem::disk(std::io::Error::from(std::io::ErrorKind::StorageFull)), Problem::DiskFull);
        assert!(matches!(Problem::disk(std::io::Error::from(std::io::ErrorKind::PermissionDenied)), Problem::Disk(_)));
    }

    #[test]
    fn the_servers_marks_come_from_a_head_request_without_the_download() {
        let address = serve_with("200 OK", Some(67_470_104), MARKS, Vec::new());
        let marks = check::head(&address, true).unwrap();
        assert_eq!(
            marks,
            Marks {
                etag: Some("\"abc123\"".to_owned()),
                last_modified: Some("Wed, 18 Nov 2026 15:58:50 GMT".to_owned()),
                size: Some(67_470_104)
            }
        );
        let address = serve_once("503 Service Unavailable", Some(0), Vec::new());
        assert_eq!(check::head(&address, true).err(), Some(Problem::Network("NDI's server answered 503".to_owned())));
        // Anywhere but NDI's own servers (or a test copy's stand-in), nothing is asked in the clear.
        assert!(check::head("http://127.0.0.1:9/x", false).is_err());
    }

    #[test]
    fn only_ndis_own_servers_count_as_ndis() {
        assert!(from_ndi(Some("downloads.ndi.tv")));
        assert!(from_ndi(Some("ndi.video")));
        for other in ["ndi.tv.example.com", "notndi.tv", "example.com", "ndi.link"] {
            assert!(!from_ndi(Some(other)), "{other}");
        }
        assert!(!from_ndi(None));
        // Every way's fixed address is on NDI's own server, over HTTPS.
        for way in [Way::Sdk, Way::Package, Way::Installer] {
            let host = way.address().strip_prefix("https://").and_then(|rest| rest.split('/').next());
            assert!(from_ndi(host), "{way:?}");
        }
        // Plain http, for a test copy's stand-in, only on this computer.
        assert!(on_this_computer("http://127.0.0.1:47811/sdk.tar.gz"));
        assert!(on_this_computer("http://localhost/sdk.tar.gz"));
        for other in ["http://127.0.0.1.example.com/x", "http://example.com/x", "https://127.0.0.1/x"] {
            assert!(!on_this_computer(other), "{other}");
        }
    }

    #[test]
    fn the_licence_accepted_is_noted_by_its_hash() {
        let mark = licence_mark(Some("TEST LICENCE"));
        assert_eq!(mark.len(), 64);
        assert_eq!(mark, licence_mark(Some("TEST LICENCE")));
        assert_ne!(mark, licence_mark(Some("TEST LICENCE, changed")));
        // An install noted before licences were, read back, has none: its update asks again.
        let old: Installed = serde_json::from_str(r#"{"version":"6.3.2","downloaded":"2026-11-24"}"#).unwrap();
        assert_eq!((old.licence.as_str(), old.marks.is_empty()), ("", true));
    }

    #[test]
    fn cancel_at_the_licence_deletes_the_download_and_says_so_once() {
        let folder = empty_folder("state");
        let mut download = NdiDownload { data: Some(folder.clone()), ..NdiDownload::default() };
        download.open(Place::Settings);
        assert!(matches!(download.stage, Stage::About));
        assert_eq!(download.shown_in(), Some(Place::Settings));
        // Cancel before anything was downloaded says nothing.
        download.cancel();
        assert!(matches!(download.stage, Stage::Idle) && download.note.is_none() && download.shown_in().is_none());
        // At the licence, it says once that nothing was installed.
        download.open(Place::Home);
        let contents = {
            let path = folder.join("sdk.tar.gz");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(&path, fixture::download(&fixture::sdk_items())).unwrap();
            sdk::read(&path, "x86_64", &AtomicBool::new(false)).unwrap()
        };
        let ready = Ready { download: PathBuf::from("/nowhere"), taken: Taken::Sdk(contents), marks: Marks::default() };
        download.stage = Stage::Licence(Box::new(ready));
        // Esc at the licence only puts the window away: it is still there to show again.
        download.hide();
        assert!(matches!(download.stage, Stage::Licence(_)) && download.shown_in().is_none());
        download.open(Place::Settings);
        assert!(matches!(download.stage, Stage::Licence(_)));
        download.cancel();
        assert!(matches!(download.stage, Stage::Idle));
        assert_eq!(download.note, Some(Note::NotInstalled));
        assert_eq!(download.shown_in(), None);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
