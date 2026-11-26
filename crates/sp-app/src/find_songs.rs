//! Settings › Song files › Find songs on this computer (the owner, 26 November 2026: "a button
//! somewhere in the settings to search computer for existing songs … show maybe the song books
//! and just library and the name of it and how much in there and then how many duplicates it
//! found and not gonna import them").
//!
//! A search looks, on a thread of its own, for what the song import already reads:
//! SoftProjector's songbooks (`.sps`, its database `spData.sqlite`, and zips holding them),
//! folders of song documents (`.pro`, as the setup's Look for a Song Library finds them), and
//! folders of song files (ChordPro and lyric files, SoftPresenter's own, and `.txt` whose words
//! read as a song). It looks in Documents, Downloads, Desktop and SoftProjector's own folder,
//! and in folders added, bounded: no hidden folder, no app or library bundle, no
//! `node_modules` or cache, no link followed, so many folders deep, files and seconds at most,
//! and never the app's own library or data folder.
//!
//! Each source found is read there and then, as a dry run: its songs counted, and those the
//! library has already, by the import's own rule (word for word under the same title, after the
//! tidy an import gives them: `load::as_imported`, `load::import_key`). Nothing is written until
//! the operator ticks some and imports them (`find_songs_cli`), each through the import's own
//! path, a songbook into a library of its name, with only its new songs.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use sp_model::{Presentation, Uuid};

use crate::i18n::{count, t, tf, tn};
use crate::load;

/// How many folders deep a place is looked through.
pub const MOST_DEPTH: usize = 8;
/// How many files and folders a search looks at, at most.
pub const MOST_ENTRIES: usize = 200_000;
/// How long a search looks, at most.
pub const MOST_TIME: Duration = Duration::from_secs(90);

/// How far a search goes: how many files and folders it looks at, and for how long, at most.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub entries: usize,
    pub time: Duration,
}

impl Limits {
    pub const NORMAL: Limits = Limits { entries: MOST_ENTRIES, time: MOST_TIME };
}

/// How a search ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ended {
    /// It looked everywhere it was to look.
    #[default]
    Everywhere,
    /// It was stopped: Stop, or another search started.
    Stopped,
    /// It had looked at as many files and folders as a search looks at.
    TooMany,
    /// It had looked for as long as a search looks.
    TooLong,
    /// Something went wrong on its thread (the log says what).
    Failed,
}

impl Ended {
    /// What the card says of a search that ended before it had looked everywhere.
    pub fn words(self) -> Option<String> {
        Some(match self {
            Ended::Everywhere => return None,
            Ended::Stopped => {
                t("settings.find.ended.stopped", "It was stopped before it had looked everywhere.").to_owned()
            }
            Ended::TooMany => tn(
                "settings.find.ended.files",
                MOST_ENTRIES as u64,
                "It stopped after looking at {n} file or folder, the most it looks at, before it had looked everywhere.",
                "It stopped after looking at {n} files and folders, the most it looks at, before it had looked everywhere.",
                &[],
            ),
            Ended::TooLong => tn(
                "settings.find.ended.time",
                MOST_TIME.as_secs(),
                "It stopped after {n} second, the longest it looks, before it had looked everywhere.",
                "It stopped after {n} seconds, the longest it looks, before it had looked everywhere.",
                &[],
            ),
            Ended::Failed => t(
                "settings.find.ended.failed",
                "Something went wrong, so it stopped before it had looked everywhere (the log says what).",
            )
            .to_owned(),
        })
    }

    /// The word `sp songs find --json` gives it.
    pub fn english(self) -> &'static str {
        match self {
            Ended::Everywhere => "everywhere",
            Ended::Stopped => "stopped",
            Ended::TooMany => "files",
            Ended::TooLong => "time",
            Ended::Failed => "failed",
        }
    }
}

/// A search or a dry run stopped part way: what it was reading is not listed in part.
#[derive(Debug, PartialEq)]
pub struct Cut;
/// The largest `.txt` whose words are read to tell whether it is a song.
const LARGEST_TEXT: u64 = 256 * 1024;

/// Folders never looked in, by name: other programs' stores and caches, and the system's own.
const PASSED_OVER: &[&str] = &[
    "node_modules",
    "bower_components",
    "__pycache__",
    "site-packages",
    "Pods",
    "DerivedData",
    "__MACOSX",
    "$RECYCLE.BIN",
    "System Volume Information",
];

/// The endings of folders that are bundles or libraries of another program (a Mac shows them as
/// one file): never looked into.
const BUNDLES: &[&str] = &[
    "app",
    "bundle",
    "framework",
    "plugin",
    "kext",
    "photoslibrary",
    "musiclibrary",
    "tvlibrary",
    "imovielibrary",
    "fcpbundle",
    "lrlibrary",
    "aplibrary",
    "logicx",
    "band",
    "xcodeproj",
    "xcworkspace",
    "xcassets",
    "playground",
    "sparsebundle",
    "pkg",
    "mpkg",
    "rtfd",
    "pages",
    "numbers",
    "key",
    "lproj",
    // Safari's download under way: a folder holding the part fetched so far.
    "download",
];

/// The places a search always looks in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Usual {
    Documents,
    Downloads,
    Desktop,
    SoftProjector,
}

impl Usual {
    pub const ALL: [Usual; 4] = [Usual::Documents, Usual::Downloads, Usual::Desktop, Usual::SoftProjector];

    /// Its name, as the card's chips and rows say it.
    pub fn name(self) -> &'static str {
        match self {
            Usual::Documents => t("settings.find.documents", "Documents"),
            Usual::Downloads => t("settings.find.downloads", "Downloads"),
            Usual::Desktop => t("settings.find.desktop", "Desktop"),
            Usual::SoftProjector => t("settings.find.softprojector", "SoftProjector\u{2019}s folder"),
        }
    }
}

/// A place a search looks in, by name: a usual one, or a folder added (by its own name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceName {
    Usual(Usual),
    Folder(String),
}

impl PlaceName {
    /// Its name in the language set.
    pub fn words(&self) -> String {
        match self {
            PlaceName::Usual(usual) => usual.name().to_owned(),
            PlaceName::Folder(name) => name.clone(),
        }
    }
}

/// Where a source is: its place, and the folders inside it on the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Where {
    pub place: PlaceName,
    pub parts: Vec<String>,
}

impl Where {
    /// As the card says it: "SoftProjector", "Documents › Worship".
    pub fn words(&self) -> String {
        let first = match &self.place {
            PlaceName::Usual(Usual::SoftProjector) => "SoftProjector".to_owned(),
            other => other.words(),
        };
        std::iter::once(first).chain(self.parts.iter().cloned()).collect::<Vec<_>>().join(" \u{203a} ")
    }
}

/// What a source found is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A SoftProjector songbook, or one songbook of its database.
    Songbook,
    /// A folder of song documents.
    Library,
    /// A folder of song files of other kinds.
    SongFiles,
    /// A place that couldn't be looked in.
    Place,
}

impl Kind {
    /// The word the card says it with.
    pub fn name(self) -> &'static str {
        match self {
            // The owner (26 November): what SoftProjector keeps shows as a library, as it comes in
            // as one ("Library · from SoftProjector").
            Kind::Songbook | Kind::Library => t("settings.find.kind.library", "Library"),
            Kind::SongFiles => t("settings.find.kind.song_files", "Song files"),
            Kind::Place => t("settings.find.kind.place", "Folder"),
        }
    }

    /// The word `sp` says it with.
    pub fn english(self) -> &'static str {
        match self {
            Kind::Songbook | Kind::Library => "library",
            Kind::SongFiles => "song files",
            Kind::Place => "folder",
        }
    }
}

/// How a source is read again to import it.
#[derive(Clone, Debug, PartialEq)]
pub enum Read {
    /// A songbook file: its songs of this songbook (`None`: those naming no songbook).
    Songbook { file: PathBuf, book: Option<String> },
    /// These song files, all in one folder.
    Files(Vec<PathBuf>),
    /// Nothing: a place that couldn't be looked in.
    Nothing,
}

/// A songbook, library or folder of song files found, as its dry run read it.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    /// Its name: the songbook's own, or the folder's (a place that couldn't be looked in:
    /// the place's, from `place`).
    pub name: String,
    pub kind: Kind,
    /// Where it is (a place that couldn't be looked in: the place itself).
    pub place: Where,
    /// The file or folder it is.
    pub path: PathBuf,
    pub read: Read,
    /// How many songs it has.
    pub songs: usize,
    /// How many of them the library has already.
    pub here: usize,
    /// How many of them are in it twice over (or more): each such song comes in once.
    pub twice: usize,
    /// Why it couldn't be read: the importer's own reason, said when it is shown.
    pub why: Option<load::Why>,
    /// What the operator should know of one that can be read, in a few words under its row: that
    /// it was copied while SoftProjector had it open, or ends part way through a song (the
    /// importer's own cautions, `load::Caution`), or that more of its song files are in the
    /// cloud.
    pub caution: Option<String>,
    /// It is only in the cloud (iCloud Drive, OneDrive), not on this computer: never read, which
    /// would download it.
    pub cloud: bool,
    /// Its songs new to the library, by their import key's hash, so songs new in two sources
    /// ticked together are counted once.
    pub new: Vec<u64>,
}

impl Source {
    /// How many of its songs are new to the library.
    pub fn fresh(&self) -> usize {
        self.new.len()
    }

    /// Whether it can be ticked: read, and with something new.
    pub fn can_import(&self) -> bool {
        self.why.is_none() && !self.cloud && self.fresh() > 0
    }
}

/// How many songs new to the library these sources have together, a song new in two of them
/// counted once.
pub fn new_in<'a>(sources: impl Iterator<Item = &'a Source>) -> usize {
    let mut all: HashSet<u64> = HashSet::new();
    sources.for_each(|source| all.extend(source.new.iter().copied()));
    all.len()
}

/// The library a search compares what it finds with: the songs an import looks in (all but
/// the deleted and the app's own), and every id given out. Arcs of the songs, so it is made on
/// the window's thread at no cost; their keys are worked out on the search's thread.
#[derive(Clone, Default)]
pub struct Library {
    pub songs: Vec<Arc<Presentation>>,
    pub ids: HashSet<Uuid>,
}

/// The hash a song's import key is known by.
pub fn key_hash(key: &(String, String)) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// What the library has, as a dry run looks a song up.
struct Known {
    keys: HashSet<u64>,
    ids: HashSet<Uuid>,
}

impl Known {
    fn of(library: &Library) -> Known {
        Known {
            keys: library.songs.iter().map(|song| key_hash(&load::import_key(song))).collect(),
            ids: library.ids.clone(),
        }
    }
}

/// A dry run's count of songs read for an import (each with the file it came from), against the
/// library and against one another: how many, how many are here already, how many are a song
/// before them over again (it comes in once), and the new ones' hashes. Only the hashes are
/// kept, never the songs.
#[derive(Default)]
struct Counted {
    all: usize,
    here: usize,
    twice: usize,
    seen: HashSet<u64>,
    new: Vec<u64>,
}

impl Counted {
    fn count(&mut self, song: Presentation, path: &Path, known: &Known) {
        let song = load::as_imported(song, path);
        let hash = key_hash(&load::import_key(&song));
        self.all += 1;
        if known.keys.contains(&hash) || known.ids.contains(&song.id) {
            self.here += 1;
        } else if !self.seen.insert(hash) {
            self.twice += 1;
        } else {
            self.new.push(hash);
        }
    }
}

/// What a search's thread says, as it goes.
#[derive(Debug)]
pub enum Found {
    /// It is looking in this place now.
    Looking(PlaceName),
    Source(Box<Source>),
    /// It has ended, and how.
    Done(Ended),
}

/// A search under way: its messages, its stop, and its thread.
pub struct Search {
    /// Its number, so the messages of one stopped are never taken for a later one's.
    pub id: u64,
    stop: Arc<AtomicBool>,
    rx: Receiver<(u64, Found)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Search {
    /// Stops it at once: its thread stops at the next file it looks at, or the next song of the
    /// file it is reading.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Stops it, and gives its thread, for the next search to wait for: two searches never look
    /// at once.
    pub fn into_thread(mut self) -> Option<std::thread::JoinHandle<()>> {
        self.stop();
        self.thread.take()
    }

    /// What it has said since the last look. A thread that ended without saying how went
    /// wrong: the search ended as failed.
    pub fn said(&self) -> Vec<Found> {
        let mut said = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok((id, found)) if id == self.id => said.push(found),
                Ok(_) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if !matches!(said.last(), Some(Found::Done(_))) {
                        said.push(Found::Done(Ended::Failed));
                    }
                    break;
                }
            }
        }
        said
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.stop();
    }
}

impl std::fmt::Debug for Search {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Search({})", self.id)
    }
}

/// Where a search looks: each place's name and its folders. A usual place has no folder on a
/// computer without it (no SoftProjector), or in a test copy without
/// `SOFTPRESENTER_FIND_SONGS_HOME`.
#[derive(Clone, Debug, PartialEq)]
pub struct Looked {
    pub name: PlaceName,
    pub folders: Vec<PathBuf>,
    /// SoftProjector's folder: looked in for songbooks only, not through.
    pub shallow: bool,
}

/// The places a search looks in: the usual ones, then the folders added.
pub fn places(added: &[PathBuf]) -> Vec<Looked> {
    let home = search_home();
    let mut places: Vec<Looked> = Usual::ALL
        .iter()
        .map(|usual| Looked {
            name: PlaceName::Usual(*usual),
            folders: home.as_ref().map(|home| usual_folders(*usual, home)).unwrap_or_default(),
            shallow: *usual == Usual::SoftProjector,
        })
        .collect();
    places.extend(added.iter().map(|folder| Looked {
        name: PlaceName::Folder(folder_name(folder)),
        folders: vec![folder.clone()],
        shallow: false,
    }));
    places
}

/// A folder's name as a chip shows it: its last part (the whole path for a drive's top).
pub fn folder_name(folder: &Path) -> String {
    folder.file_name().map_or_else(|| folder.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// True in a test copy: quiet, with a data folder of its own, or saving a picture.
fn test_copy() -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    set("SOFTPRESENTER_QUIET")
        || set("SOFTPRESENTER_DATA")
        || std::env::vars_os().any(|(name, _)| name.to_string_lossy().starts_with("SOFTPRESENTER_CAPTURE"))
        || std::env::args().any(|word| word == "--capture-home")
}

/// The home folder the usual places are in: the person's own, or in a test copy only the one
/// `SOFTPRESENTER_FIND_SONGS_HOME` names (none without it).
fn search_home() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    // Testing aid: SOFTPRESENTER_FIND_SONGS_HOME=<folder> is the home folder a test copy's
    // search looks in (its Documents, Downloads and Desktop, and SoftProjector for
    // SoftProjector's folder). A test copy without it looks in no usual place, so it never reads
    // the owner's folders; folders added are still looked in.
    if test_copy() {
        return var("SOFTPRESENTER_FIND_SONGS_HOME");
    }
    var("HOME").or_else(|| var("USERPROFILE"))
}

/// A usual place's folders under a home folder, on this system.
fn usual_folders(usual: Usual, home: &Path) -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    // A test's home folder holds each place under its plain name.
    if test_copy() {
        return vec![match usual {
            Usual::Documents => home.join("Documents"),
            Usual::Downloads => home.join("Downloads"),
            Usual::Desktop => home.join("Desktop"),
            Usual::SoftProjector => home.join("SoftProjector"),
        }];
    }
    let own = |name: &str, xdg: &str| -> Vec<PathBuf> {
        let mut folders = Vec::new();
        if cfg!(target_os = "linux")
            && let Some(folder) = xdg_folder(home, xdg)
        {
            folders.push(folder);
        }
        folders.push(home.join(name));
        // On Windows, OneDrive may keep Documents and Desktop in a folder of its own.
        if cfg!(windows)
            && let Some(one_drive) = var("OneDrive")
        {
            folders.push(one_drive.join(name));
        }
        folders.dedup();
        folders
    };
    match usual {
        Usual::Documents => own("Documents", "XDG_DOCUMENTS_DIR"),
        Usual::Downloads => own("Downloads", "XDG_DOWNLOAD_DIR"),
        Usual::Desktop => own("Desktop", "XDG_DESKTOP_DIR"),
        Usual::SoftProjector => softprojector_folders(home),
    }
}

/// A folder the Linux desktop names in `~/.config/user-dirs.dirs` (`XDG_DOCUMENTS_DIR="$HOME/Документы"`).
fn xdg_folder(home: &Path, name: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(home.join(".config/user-dirs.dirs")).ok()?;
    let line = text.lines().find(|line| line.trim_start().starts_with(&format!("{name}=")))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    (path != home).then_some(path)
}

/// Where SoftProjector keeps its database on this system: beside its program (its Qt 5 builds
/// keep `spData.sqlite` in the program's own folder), which on Windows may have been moved to
/// the user's VirtualStore, or in ProgramData (older builds); its songbooks are often saved
/// there too.
fn softprojector_folders(home: &Path) -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    // The folders in `parent` whose names start with SoftProjector's, in any case.
    let named_in = |parent: &Path, ending: &str| -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                name.starts_with("softprojector") && name.ends_with(ending)
            })
            .map(|entry| entry.path())
            .collect();
        found.sort();
        found
    };
    let mut folders = Vec::new();
    if cfg!(target_os = "macos") {
        for applications in [PathBuf::from("/Applications"), home.join("Applications")] {
            folders.extend(named_in(&applications, ".app").into_iter().map(|app| app.join("Contents/MacOS")));
        }
    } else if cfg!(windows) {
        folders.extend(var("ProgramData").map(|data| data.join("SoftProjector")));
        folders.extend(var("PUBLIC").map(|public| public.join("SoftProjector")));
        for programs in [var("ProgramFiles"), var("ProgramFiles(x86)")].into_iter().flatten() {
            folders.extend(named_in(&programs, ""));
        }
        if let Some(local) = var("LOCALAPPDATA") {
            for programs in ["Program Files", "Program Files (x86)"] {
                folders.extend(named_in(&local.join("VirtualStore").join(programs), ""));
            }
        }
    } else {
        for folder in
            ["/opt/softprojector", "/opt/SoftProjector", "/usr/share/softprojector", "/usr/local/share/softprojector"]
        {
            folders.push(PathBuf::from(folder));
        }
        folders.push(home.join("softprojector"));
        folders.push(home.join("SoftProjector"));
        folders.push(PathBuf::from("/usr/bin"));
        folders.push(PathBuf::from("/usr/local/bin"));
    }
    folders.retain(|folder| folder.is_dir());
    folders.dedup();
    folders
}

/// The folders a search never lists: the app's own library and data folder (wherever it is
/// kept now, and where a Mac kept it before), and any folder marked as SoftPresenter's library.
pub fn never_listed() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = crate::bible_store::data_dir().into_iter().collect();
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()).map(PathBuf::from) {
        folders.push(crate::library_folder::new_place(&home));
        folders.push(crate::library_folder::old_place(&home));
    }
    folders.into_iter().map(|folder| folder.canonicalize().unwrap_or(folder)).collect()
}

/// Starts a search on a thread of its own, once the thread of the search before (`after`, if
/// it is still stopping) has ended. `wake` is called after each thing it says, so the app looks
/// at once.
pub fn start(
    id: u64,
    places: Vec<Looked>,
    library: Library,
    skip: Vec<PathBuf>,
    after: Option<std::thread::JoinHandle<()>>,
    wake: impl Fn() + Send + 'static,
) -> Result<Search, String> {
    let search = Begin { id, places, library, skip, limits: Limits::NORMAL, after, marks };
    start_with(Arc::new(AtomicBool::new(false)), search, wake)
}

/// What a search is to do.
struct Begin {
    id: u64,
    places: Vec<Looked>,
    library: Library,
    skip: Vec<PathBuf>,
    limits: Limits,
    after: Option<std::thread::JoinHandle<()>>,
    /// What the system says of a file or folder ([`marks`]; tests give their own).
    marks: fn(&std::fs::DirEntry) -> Marks,
}

/// [`start`], with the stop given.
fn start_with(stop: Arc<AtomicBool>, begin: Begin, wake: impl Fn() + Send + 'static) -> Result<Search, String> {
    let (tx, rx) = channel();
    let stopped = stop.clone();
    let id = begin.id;
    let thread = std::thread::Builder::new()
        .name("find songs".to_owned())
        .spawn(move || {
            let Begin { id, places, library, skip, limits, after, marks } = begin;
            // The search before has stopped looking, and its thread ends at once.
            if let Some(before) = after {
                let _ = before.join();
            }
            // A panic in reading a file (ours, or a library's) ends the search as failed, never
            // the app, and never as if it had looked everywhere.
            let began = Instant::now();
            let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let known = Known::of(&library);
                let mut walk = Walk::new(&stopped, &known, &skip, &tx, id, limits, &wake);
                walk.marks = marks;
                for place in &places {
                    walk.place(place);
                }
                (walk.ended(), walk.entries)
            }));
            let (ended, entries) = walked.unwrap_or((Ended::Failed, 0));
            let _ = tx.send((id, Found::Done(ended)));
            wake();
            let took = began.elapsed().as_millis();
            let said = match ended {
                // How far it got went with the panic, which the log has above this.
                Ended::Failed => format!("went wrong after {took} ms (the panic above says where)"),
                Ended::Everywhere => format!("looked at {entries} files and folders in {took} ms"),
                ended => format!("looked at {entries} files and folders in {took} ms (ended: {})", ended.english()),
            };
            crate::applog::event("find", &said);
        })
        .map_err(|e| e.to_string())?;
    Ok(Search { id, stop, rx, thread: Some(thread) })
}

/// One search's walk through its places.
struct Walk<'a> {
    stop: &'a AtomicBool,
    known: &'a Known,
    skip: &'a [PathBuf],
    tx: &'a Sender<(u64, Found)>,
    id: u64,
    wake: &'a dyn Fn(),
    began: Instant,
    limits: Limits,
    entries: usize,
    /// The folders looked in already (a folder added inside Documents is looked in once).
    walked: HashSet<PathBuf>,
    /// Each folder's look takes this much longer (`SOFTPRESENTER_FIND_SONGS_SLOW`).
    slow: Duration,
    marks: fn(&std::fs::DirEntry) -> Marks,
    /// How many sources it has listed, and after how many it goes wrong
    /// (`SOFTPRESENTER_FIND_SONGS_FAIL`).
    listed: std::cell::Cell<usize>,
    fail_after: Option<usize>,
}

/// Testing aid: SOFTPRESENTER_FIND_SONGS_SLOW=<ms> makes each folder a search looks in, and
/// each song an import of what it found reads, take that many milliseconds longer, so a picture
/// can catch the card while it looks and while it brings songs in. Only in a test copy.
pub fn slow() -> Duration {
    if !test_copy() {
        return Duration::ZERO;
    }
    std::env::var("SOFTPRESENTER_FIND_SONGS_SLOW")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or_default()
}

impl<'a> Walk<'a> {
    fn new(
        stop: &'a AtomicBool,
        known: &'a Known,
        skip: &'a [PathBuf],
        tx: &'a Sender<(u64, Found)>,
        id: u64,
        limits: Limits,
        wake: &'a dyn Fn(),
    ) -> Walk<'a> {
        Walk {
            stop,
            known,
            skip,
            tx,
            id,
            wake,
            began: Instant::now(),
            limits,
            entries: 0,
            walked: HashSet::new(),
            slow: slow(),
            marks,
            listed: std::cell::Cell::new(0),
            fail_after: fail_after(),
        }
    }

    /// Whether to look no further: stopped, or past a limit. Asked between files, and between
    /// the songs of a file being read.
    fn cut(&self) -> bool {
        self.ended() != Ended::Everywhere
    }

    /// How it has ended so far: `Everywhere` while it may go on.
    fn ended(&self) -> Ended {
        if self.stop.load(Ordering::Relaxed) {
            Ended::Stopped
        } else if self.entries >= self.limits.entries {
            Ended::TooMany
        } else if self.began.elapsed() >= self.limits.time {
            Ended::TooLong
        } else {
            Ended::Everywhere
        }
    }

    fn say(&self, found: Found) {
        let source = matches!(found, Found::Source(_));
        let _ = self.tx.send((self.id, found));
        (self.wake)();
        if source {
            self.listed.set(self.listed.get() + 1);
            if self.fail_after.is_some_and(|after| self.listed.get() >= after) {
                panic!("SOFTPRESENTER_FIND_SONGS_FAIL: the search goes wrong, as a test asked");
            }
        }
    }

    fn place(&mut self, place: &Looked) {
        if self.cut() {
            return;
        }
        self.say(Found::Looking(place.name.clone()));
        for folder in &place.folders {
            let root = folder.canonicalize().unwrap_or_else(|_| folder.clone());
            let at = Where { place: place.name.clone(), parts: Vec::new() };
            match self.folder(&root, &at, 0, place.shallow) {
                Ok(()) => {}
                // A place that isn't there is nothing found; one the system keeps closed is said.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => self.say(Found::Source(Box::new(Source {
                    name: String::new(),
                    kind: Kind::Place,
                    place: at,
                    path: folder.clone(),
                    read: Read::Nothing,
                    songs: 0,
                    here: 0,
                    twice: 0,
                    why: Some(not_allowed(&e)),
                    caution: None,
                    cloud: false,
                    new: Vec::new(),
                }))),
            }
        }
    }

    /// Looks in a folder: its songbooks, the folder itself as a library or folder of song files,
    /// then the folders in it. `at` is where it is; `shallow` looks for songbooks only, in it
    /// alone. An error only for a folder that couldn't be read.
    fn folder(&mut self, dir: &Path, at: &Where, depth: usize, shallow: bool) -> std::io::Result<()> {
        if self.cut() || !self.walked.insert(dir.to_path_buf()) {
            return Ok(());
        }
        if self.skip.iter().any(|skip| skip == dir) {
            return Ok(());
        }
        if !self.slow.is_zero() {
            std::thread::sleep(self.slow);
        }
        // Each entry is counted as it is listed, and the listing stops at the most a search looks
        // at, so a folder of millions is never all held and sorted.
        let mut entries: Vec<std::fs::DirEntry> = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            self.entries += 1;
            if self.cut() {
                return Ok(());
            }
            if let Ok(entry) = entry {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|entry| entry.file_name());
        // A cache (a build's, say) or SoftPresenter's own library: nothing in it is listed.
        let marked = |name: &str| name == "CACHEDIR.TAG" || name == crate::library_folder::MARKER;
        if entries.iter().any(|entry| marked(&entry.file_name().to_string_lossy())) {
            return Ok(());
        }
        let (mut books, mut documents, mut lyrics, mut texts, mut inside) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        // Files only in the cloud, never opened (opening one downloads it): songbooks, and how
        // many song files and whether a song document is among them.
        let (mut cloud_books, mut cloud_files, mut cloud_documents) = (Vec::new(), 0, false);
        for entry in &entries {
            if self.cut() {
                return Ok(());
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            // Never through a link: it may lead anywhere. On Windows the system counts every
            // name surrogate (a symbolic link, a junction, a mounted drive) as one; a cloud
            // placeholder is no link, and a OneDrive folder is looked through.
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                // A folder Windows marks hidden is passed over, as one named with a dot is.
                if !shallow && !passed_over(&name) && !(self.marks)(entry).hidden {
                    inside.push((path, name));
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            // Only by its name first: the system is asked about the files that may hold songs.
            let Some(ending) = ending_of(&path, shallow) else { continue };
            let marks = (self.marks)(entry);
            if marks.hidden {
                continue;
            }
            if marks.cloud {
                match ending {
                    // A zip, or any program's database, may be no songbook: not said.
                    Ending::Songbook if !has_ending(&path, "zip") && listed_unread(&path) => cloud_books.push(path),
                    Ending::Document => (cloud_files, cloud_documents) = (cloud_files + 1, true),
                    Ending::Lyrics => cloud_files += 1,
                    // Text is a song only when its words read as one: not said.
                    _ => {}
                }
                continue;
            }
            match ending {
                Ending::Songbook if is_songbook_file(&path) => books.push(path),
                Ending::Songbook => {}
                Ending::Document => documents.push(path),
                Ending::Lyrics => lyrics.push(path),
                Ending::Text => texts.push(path),
            }
        }
        for book in cloud_books {
            let name = book.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            self.say(Found::Source(Box::new(Source {
                name,
                kind: Kind::Songbook,
                place: at.clone(),
                path: book,
                read: Read::Nothing,
                songs: 0,
                here: 0,
                twice: 0,
                why: None,
                caution: None,
                cloud: true,
                new: Vec::new(),
            })));
        }
        // A file being read when the search is stopped (or runs out of time) is left part way,
        // and nothing of it is listed.
        let until = || self.cut();
        for book in books {
            let Ok(sources) = songbook_sources(&book, at, self.known, &until) else { return Ok(()) };
            for source in sources {
                self.say(Found::Source(Box::new(source)));
            }
        }
        let files = Files { documents, lyrics, texts, cloud: cloud_files, cloud_documents };
        let Ok(found) = folder_source(dir, at, files, self.known, &until) else { return Ok(()) };
        if let Some(source) = found {
            self.say(Found::Source(Box::new(source)));
        }
        if depth < MOST_DEPTH {
            for (folder, name) in inside {
                let mut deeper = at.clone();
                deeper.parts.push(name);
                // A folder inside that can't be read is passed over.
                let _ = self.folder(&folder, &deeper, depth + 1, false);
            }
        }
        Ok(())
    }
}

/// Why a place couldn't be looked in, in the card's words.
fn not_allowed(e: &std::io::Error) -> load::Why {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        return load::Why::NotAllowed;
    }
    load::Why::System(e.to_string())
}

/// Whether a folder is passed over by its name: another program's store, a cache, or a bundle.
fn passed_over(name: &str) -> bool {
    if PASSED_OVER.iter().any(|over| over.eq_ignore_ascii_case(name)) {
        return true;
    }
    Path::new(name).extension().is_some_and(|ending| BUNDLES.iter().any(|bundle| ending.eq_ignore_ascii_case(bundle)))
}

/// What a file may hold, by its name alone.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ending {
    /// `.sps`, `.sqlite` or `.zip`.
    Songbook,
    /// A song document (`.pro`).
    Document,
    /// A ChordPro or lyric file, or SoftPresenter's own.
    Lyrics,
    /// Plain text, a song only when its words read as one.
    Text,
}

/// What a file may hold, by its name (`shallow`: songbooks only).
fn ending_of(path: &Path, shallow: bool) -> Option<Ending> {
    if ["sps", "sqlite", "zip"].iter().any(|ending| has_ending(path, ending)) {
        return Some(Ending::Songbook);
    }
    if shallow {
        return None;
    }
    if load::is_song_document(path) {
        Some(Ending::Document)
    } else if load::is_lyrics_file(path) || load::is_song_file(path) {
        Some(Ending::Lyrics)
    } else if load::is_plain_text(path) {
        Some(Ending::Text)
    } else {
        None
    }
}

fn has_ending(path: &Path, ending: &str) -> bool {
    path.extension().is_some_and(|have| have.eq_ignore_ascii_case(ending))
}

/// What the system says of a file or folder, without opening it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Marks {
    /// Windows marks it hidden.
    pub hidden: bool,
    /// Its data is only in the cloud: iCloud Drive's "dataless" file on a Mac, a OneDrive
    /// placeholder on Windows. Reading it would download it.
    pub cloud: bool,
}

/// What the system says of an entry of a folder ([`system_marks`]); in a test copy with
/// `SOFTPRESENTER_FIND_SONGS_CLOUD`, a file whose name holds one of its words is only in the
/// cloud.
fn marks(entry: &std::fs::DirEntry) -> Marks {
    let marks = system_marks(entry);
    let name = entry.file_name().to_string_lossy().into_owned();
    if cloud_words().iter().any(|word| name.contains(word.as_str())) { Marks { cloud: true, ..marks } } else { marks }
}

/// Testing aid: SOFTPRESENTER_FIND_SONGS_CLOUD=<words, by commas> makes a test copy's search
/// take a file whose name holds one of the words as only in the cloud, as iCloud Drive or
/// OneDrive keeps one (nothing on this computer can make one), so a picture can show the card's
/// rows for such files. Only in a test copy.
fn cloud_words() -> &'static [String] {
    static WORDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        let words = if test_copy() {
            std::env::var("SOFTPRESENTER_FIND_SONGS_CLOUD").unwrap_or_default()
        } else {
            String::new()
        };
        words.split(',').map(str::trim).filter(|word| !word.is_empty()).map(str::to_owned).collect()
    })
}

/// Testing aid: SOFTPRESENTER_FIND_SONGS_FAIL=<n> makes a test copy's search thread panic once it
/// has listed n sources, so a picture can show a search that went wrong. Only in a test copy.
fn fail_after() -> Option<usize> {
    if !test_copy() {
        return None;
    }
    std::env::var("SOFTPRESENTER_FIND_SONGS_FAIL").ok()?.parse().ok()
}

/// What the system says of an entry of a folder: on Windows, from the folder's own listing; on
/// a Mac, from the file's flags, asked without following a link or opening the file.
fn system_marks(entry: &std::fs::DirEntry) -> Marks {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        windows_marks(entry.metadata().map_or(0, |meta| meta.file_attributes()))
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        Marks { hidden: false, cloud: mac_dataless(entry.metadata().map_or(0, |meta| meta.st_flags())) }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = entry;
        Marks::default()
    }
}

/// A file's Windows attributes (`FILE_ATTRIBUTE_*`, winnt.h): hidden; and a cloud placeholder
/// whose data isn't on the computer: offline, or fetched when it is opened or read. A file
/// OneDrive keeps on the computer is a reparse point too, with none of those, and is read.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn windows_marks(attributes: u32) -> Marks {
    const HIDDEN: u32 = 0x2;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x4_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    Marks {
        hidden: attributes & HIDDEN != 0,
        cloud: attributes & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0,
    }
}

/// Whether a Mac's file flags (`st_flags`) say its data is in the cloud: `SF_DATALESS`, as
/// sys/stat.h defines it.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn mac_dataless(flags: u32) -> bool {
    const SF_DATALESS: u32 = 0x4000_0000;
    flags & SF_DATALESS != 0
}

/// Whether a file may be a songbook: `.sps`, `.sqlite` and zips that list one.
fn is_songbook_file(path: &Path) -> bool {
    let ending = path.extension().map(|ending| ending.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ending.as_str() {
        "sps" | "sqlite" => true,
        // Only a zip's list of names is read, and only for a zip small enough to be one.
        "zip" => load::is_songbook(path),
        _ => false,
    }
}

/// Whether a songbook file that can't be read is listed: always, but for a `.sqlite` other than
/// SoftProjector's own database, which may be any program's.
fn listed_unread(path: &Path) -> bool {
    let name = path.file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default();
    !name.ends_with(".sqlite") || name == "spdata.sqlite"
}

/// The sources in a songbook file, read through the import's own reader: one for each songbook
/// in it (a database holds several), or one saying why it couldn't be read. `Cut` when `until`
/// said to stop part way.
fn songbook_sources(path: &Path, place: &Where, known: &Known, until: &dyn Fn() -> bool) -> Result<Vec<Source>, Cut> {
    let file_name = path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    // Songs naming no songbook are named after the file: SoftProjector's own database's are its
    // songs, so they are SoftProjector's.
    let stem = if file_name.eq_ignore_ascii_case("spData.sqlite") {
        "SoftProjector".to_owned()
    } else {
        path.file_stem().map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    };
    let base = |name: String, book: Option<String>| Source {
        name,
        kind: Kind::Songbook,
        place: place.clone(),
        path: path.to_owned(),
        read: Read::Songbook { file: path.to_owned(), book },
        songs: 0,
        here: 0,
        twice: 0,
        why: None,
        caution: None,
        cloud: false,
        new: Vec::new(),
    };
    // Each songbook in it, in the order its songs come (looked up by name: a database may name
    // thousands), each song counted as it is made and let go, so the file's songs are never all
    // held at once.
    let mut books: Vec<(Option<String>, Counted)> = Vec::new();
    let mut by_name: std::collections::HashMap<Option<String>, usize> = std::collections::HashMap::new();
    let read = load::read_songbook_file_each(path, until, &mut |song, name| {
        let at = *by_name.entry(name.clone()).or_insert_with(|| {
            books.push((name, Counted::default()));
            books.len() - 1
        });
        books[at].1.count(song, path, known);
    });
    if until() {
        return Err(Cut);
    }
    match read {
        Ok(cautions) => {
            let said = rows_cautions(cautions, books.len());
            Ok(books
                .into_iter()
                .zip(said)
                .map(|((name, counted), caution)| Source {
                    songs: counted.all,
                    here: counted.here,
                    twice: counted.twice,
                    new: counted.new,
                    caution,
                    ..base(name.clone().unwrap_or_else(|| stem.clone()), name)
                })
                .collect())
        }
        Err(refused) if listed_unread(path) => {
            crate::applog::event("find", &unread_line(path, &refused.why));
            Ok(vec![Source { why: Some(refused.why), read: Read::Nothing, ..base(file_name, None) }])
        }
        Err(_) => Ok(Vec::new()),
    }
}

/// What the log says of a songbook found that couldn't be read: its ending, size and what it
/// looks like, never its name (the search comes across files nobody chose), and why, in English
/// whatever the window speaks.
fn unread_line(path: &Path, why: &load::Why) -> String {
    let ending = path.extension().map_or_else(String::new, |ending| format!(".{} ", ending.to_string_lossy()));
    let kind = crate::applog::file_kind::of(path).map_or_else(String::new, |kind| format!(" ({kind})"));
    format!("a {ending}file it found couldn't be read{kind}: {}", why.english())
}

/// What the rows of a file of `count` songbooks say of it, first to last: what is about the
/// whole file (copied while it was open) once, on the first row, worded for the file's
/// libraries when it has more than one; the rest on each row.
fn rows_cautions(cautions: Vec<load::Caution>, count: usize) -> Vec<Option<String>> {
    let (whole, each): (Vec<_>, Vec<_>) = cautions.into_iter().partition(|caution| caution.whole_file);
    let whole: Vec<String> = whole
        .into_iter()
        .map(|caution| {
            if count > 1 {
                tn(
                    "settings.find.caution.unsaved_file",
                    count as u64,
                    "Copied while it was open in SoftProjector: the latest changes to this file\u{2019}s {n} library may be missing",
                    "Copied while it was open in SoftProjector: the latest changes to this file\u{2019}s {n} libraries may be missing",
                    &[],
                )
            } else {
                caution.short
            }
        })
        .collect();
    let each: Vec<String> = each.into_iter().map(|caution| caution.short).collect();
    (0..count)
        .map(|at| {
            let said: Vec<&str> = whole.iter().filter(|_| at == 0).chain(&each).map(String::as_str).collect();
            (!said.is_empty()).then(|| said.join(" "))
        })
        .collect()
}

/// The files of a folder that may hold songs, by what they are, and how many song files (and
/// whether a song document among them) are only in the cloud.
struct Files {
    documents: Vec<PathBuf>,
    lyrics: Vec<PathBuf>,
    texts: Vec<PathBuf>,
    cloud: usize,
    cloud_documents: bool,
}

/// A folder as a source, when it holds songs: song documents make it a library; ChordPro and
/// lyric files, SoftPresenter's own song files and text that reads as a song make it a folder of
/// song files (two at least, unless one is of a kind only songs are kept in). Each file is read
/// as the import reads it; one that can't be read isn't counted. `Cut` when `until` said to stop
/// part way.
fn folder_source(
    dir: &Path,
    at: &Where,
    files: Files,
    known: &Known,
    until: &dyn Fn() -> bool,
) -> Result<Option<Source>, Cut> {
    let Files { documents, lyrics, texts, cloud, cloud_documents } = files;
    let mut songs_in_texts = Vec::new();
    for text in texts {
        if until() {
            return Err(Cut);
        }
        if text_reads_as_song(&text) {
            songs_in_texts.push(text);
        }
    }
    let texts = songs_in_texts;
    let library = !documents.is_empty() || cloud_documents;
    let distinctive = documents.len() + lyrics.len() + cloud;
    if distinctive == 0 && texts.len() < 2 {
        return Ok(None);
    }
    let mut files: Vec<PathBuf> = documents.into_iter().chain(lyrics).chain(texts).collect();
    files.sort();
    // Every file is counted against the same songs seen, as one import takes them in, as it is
    // read.
    let mut counted = Counted::default();
    let mut readable = Vec::new();
    for file in files {
        if until() {
            return Err(Cut);
        }
        let song = if load::is_song_file(&file) { load::read_song_file(&file) } else { load::import_file(&file) };
        if let Ok(song) = song {
            counted.count(song, &file, known);
            readable.push(file);
        }
    }
    // Song files only in the cloud aren't read: a folder of nothing else says so, and one with
    // songs read says how many more there are.
    if readable.is_empty() && cloud == 0 {
        return Ok(None);
    }
    let caution = (cloud > 0 && !readable.is_empty()).then(|| {
        tn(
            "settings.find.caution.cloud",
            cloud as u64,
            "{n} more song file here is only in the cloud, so it wasn\u{2019}t looked at",
            "{n} more song files here are only in the cloud, so they weren\u{2019}t looked at",
            &[],
        )
    });
    let cloud = readable.is_empty();
    let Counted { all: songs, here, twice, new, .. } = counted;
    // A folder inside a place is named by itself, and is in the place's folders above it; a
    // place's own folder is named by the place and is in it.
    let (name, place) = match at.parts.split_last() {
        Some((name, above)) => (name.clone(), Where { place: at.place.clone(), parts: above.to_vec() }),
        None => (at.place.words(), at.clone()),
    };
    Ok(Some(Source {
        name,
        kind: if library { Kind::Library } else { Kind::SongFiles },
        place,
        path: dir.to_owned(),
        read: Read::Files(readable),
        songs,
        here,
        twice,
        why: None,
        caution,
        cloud,
        new,
    }))
}

/// Whether a `.txt` holds a song: small, and its words read as one (`load::reads_as_song`).
fn text_reads_as_song(path: &Path) -> bool {
    let small = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= LARGEST_TEXT);
    small && std::fs::read(path).is_ok_and(|bytes| load::reads_as_song(&sp_model::decode_text(&bytes)))
}

/// Reads a source again to import it, through the import's own readers: its songs, each with
/// the file it came from, and anything the operator should be told of it.
pub fn read_again(source: &Source) -> (Vec<(Presentation, PathBuf)>, Vec<String>) {
    match &source.read {
        Read::Songbook { file, book } => match load::read_songbook_file(file) {
            Ok(read) => (
                read.songs.into_iter().filter(|(_, name)| name == book).map(|(song, _)| (song, file.clone())).collect(),
                read.cautions,
            ),
            Err(refused) => (Vec::new(), vec![refused.said]),
        },
        Read::Files(files) => {
            let mut songs = Vec::new();
            let mut problems = Vec::new();
            for file in files {
                let name = file.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let song = if load::is_song_file(file) {
                    load::read_song_file(file).map_err(|reason| format!("{name}: {reason}"))
                } else {
                    load::import_file(file)
                };
                match song {
                    Ok(song) => songs.push((song, file.clone())),
                    Err(problem) => problems.push(problem),
                }
            }
            (songs, problems)
        }
        Read::Nothing => (Vec::new(), Vec::new()),
    }
}

/// A row's words, in the language set: its name, the line under it ("Library · from
/// SoftProjector · Downloads · 7 songs"), and what it says at its right ("7 new · 2 already here
/// · 1 found twice", "Everything here is already in your library", "Couldn't be read: …").
pub fn row_words(source: &Source) -> (String, String, String) {
    let place = match source.kind {
        Kind::Songbook if source.place.place == PlaceName::Usual(Usual::SoftProjector) => {
            t("settings.find.from_softprojector", "from SoftProjector").to_owned()
        }
        Kind::Songbook => tf(
            "settings.find.from_softprojector_at",
            "from SoftProjector \u{b7} {where}",
            &[("where", &source.place.words())],
        ),
        _ => source.place.words(),
    };
    let (name, line) = match source.kind {
        // A place that couldn't be looked in: the place, and its folder.
        Kind::Place => (place, source.path.display().to_string()),
        kind if source.why.is_some() || source.cloud => (
            source.name.clone(),
            tf("settings.find.line_unread", "{kind} \u{b7} {where}", &[("kind", kind.name()), ("where", &place)]),
        ),
        kind => {
            // A count and its word are never parted at a line's end.
            let songs =
                tn("settings.find.songs", source.songs as u64, "{n} song", "{n} songs", &[]).replace(' ', "\u{a0}");
            (
                source.name.clone(),
                tf(
                    "settings.find.line",
                    "{kind} \u{b7} {where} \u{b7} {songs}",
                    &[("kind", kind.name()), ("where", &place), ("songs", &songs)],
                ),
            )
        }
    };
    let right = if let Some(why) = &source.why {
        tf("settings.find.unreadable", "Couldn\u{2019}t be read: {why}", &[("why", &why.words())])
    } else if source.cloud {
        t("settings.find.cloud", "In the cloud, not on this computer").to_owned()
    } else if source.fresh() == 0 {
        t("settings.find.all_here", "Everything here is already in your library").to_owned()
    } else {
        let mut counts = tn("settings.find.new", source.fresh() as u64, "{n} new", "{n} new", &[]);
        if source.here > 0 {
            let here = tn("settings.find.here", source.here as u64, "{n} already here", "{n} already here", &[]);
            counts = tf("settings.find.new_and_here", "{new} \u{b7} {here}", &[("new", &counts), ("here", &here)]);
        }
        if source.twice > 0 {
            let twice = tn("settings.find.twice", source.twice as u64, "{n} found twice", "{n} found twice", &[]);
            counts = tf("settings.find.new_and_here", "{new} \u{b7} {here}", &[("new", &counts), ("here", &twice)]);
        }
        counts
    };
    (name, line, right)
}

/// What came of an import, in the language set: the sentence in three pieces, the middle one
/// (how many songs came in) for the card to set in bold. "**7 songs came in**: Zpěvník pro
/// zkoušku (7). 2 already in your library were left out. 7 found in more than one place came
/// in once." A source none came in from isn't named.
pub fn brought_words(brought: &Brought) -> [String; 3] {
    let came = tn("settings.find.came", brought.came as u64, "{n} song came in", "{n} songs came in", &[]);
    let list: Vec<String> = brought
        .from
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(name, n)| tf("settings.find.came_from", "{name} ({n})", &[("name", name), ("n", &count(*n as u64))]))
        .collect();
    let sentence = if list.is_empty() {
        format!("{came}.")
    } else {
        tf("settings.find.done", "{came}: {list}.", &[("came", &came), ("list", &list.join(", "))])
    };
    let left = (brought.left_out > 0).then(|| {
        tn(
            "settings.find.left_out",
            brought.left_out as u64,
            "{n} already in your library was left out.",
            "{n} already in your library were left out.",
            &[],
        )
    });
    // Songs in two of the sources ticked, or twice in one, came in once: they were never in the
    // library before.
    let twice = (brought.twice > 0).then(|| {
        tn(
            "settings.find.came_once",
            brought.twice as u64,
            "{n} found in more than one place came in once.",
            "{n} found in more than one place came in once.",
            &[],
        )
    });
    let failed = brought.failed.then(|| {
        t(
            "settings.find.import_failed",
            "Something went wrong, so it stopped before every song had come in (the log says what).",
        )
        .to_owned()
    });
    let left: String = [left, twice, failed].into_iter().flatten().map(|words| format!(" {words}")).collect();
    match sentence.find(&came) {
        Some(at) => [sentence[..at].to_owned(), came.clone(), format!("{}{left}", &sentence[at + came.len()..])],
        None => [format!("{sentence}{left}"), String::new(), String::new()],
    }
}

/// How far the card has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// Before a search: where it looks, and Find.
    #[default]
    Before,
    Searching,
    /// The sources found (none: Found nothing).
    Found,
    Importing,
    Done,
}

/// What the card asks the app for, carried out after the Settings window draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Find,
    Stop,
    /// Search another folder: the system's folder window.
    AnotherFolder,
    Import,
}

/// What came of an import, as the card and the notice say it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Brought {
    /// The songs that came in.
    pub came: usize,
    /// Each source imported, with the songs that came in from it.
    pub from: Vec<(String, usize)>,
    /// The songs left out, the library having them already.
    pub left_out: usize,
    /// The songs read again after one of them had come in (found in two of the sources, or
    /// twice in one): it came in once.
    pub twice: usize,
    /// Something went wrong on the reading thread, so not every song came in.
    pub failed: bool,
}

/// The card's state, kept with the window's (`HomeState::find_songs`); the search and the import
/// themselves are the app's (`find_songs_cli`).
#[derive(Clone, Debug, Default)]
pub struct Card {
    pub phase: Phase,
    /// The folders added to where it looks.
    pub added: Vec<PathBuf>,
    /// The places the last search looked in.
    pub looked: Vec<PlaceName>,
    /// How the last search ended.
    pub ended: Ended,
    /// The place being looked in.
    pub looking: Option<PlaceName>,
    pub sources: Vec<Source>,
    /// Which sources are ticked, in the same order.
    pub ticked: Vec<bool>,
    /// While importing: how many songs it brings in, and how far it has got (0 to 1).
    pub bringing: usize,
    pub progress: f32,
    pub brought: Option<Brought>,
    /// What was clicked, for the app.
    pub asks: Vec<Ask>,
    /// The rows' heights, so only those in view are laid out (see `find_card`).
    pub heights: RowHeights,
    /// How many new songs the sources ticked bring in, kept with what it was worked out for.
    new_ticked: std::cell::Cell<Option<(u64, usize)>>,
}

/// How tall each row of the card's list is, at the width it was last laid out at; rows not yet
/// laid out at that width are taken to be as tall as the last one that was.
#[derive(Clone, Debug, Default)]
pub struct RowHeights {
    pub width: f32,
    pub heights: Vec<Option<f32>>,
    pub typical: f32,
}

impl Card {
    /// A source found: listed, ticked when it has something new.
    pub fn add(&mut self, source: Source) {
        self.ticked.push(source.can_import());
        self.sources.push(source);
    }

    /// The sources ticked that have something new.
    pub fn chosen(&self) -> Vec<usize> {
        (0..self.sources.len())
            .filter(|at| self.ticked.get(*at).copied().unwrap_or(false) && self.sources[*at].can_import())
            .collect()
    }

    /// How many new songs the sources ticked bring in, each counted once. Worked out again only
    /// when the sources or the ticks change, not each time the card is drawn (a search may find
    /// thousands of sources).
    pub fn new_ticked(&self) -> usize {
        let mut hasher = std::hash::DefaultHasher::new();
        for (source, ticked) in self.sources.iter().zip(&self.ticked) {
            (ticked, source.new.len(), source.new.first(), source.new.last()).hash(&mut hasher);
        }
        self.sources.len().hash(&mut hasher);
        let key = hasher.finish();
        if let Some((have, new)) = self.new_ticked.get()
            && have == key
        {
            return new;
        }
        let new = new_in(self.chosen().into_iter().map(|at| &self.sources[at]));
        self.new_ticked.set(Some((key, new)));
        new
    }

    /// The places where it looks: the usual ones, then the folders added.
    pub fn places(&self) -> Vec<PlaceName> {
        Usual::ALL
            .iter()
            .map(|usual| PlaceName::Usual(*usual))
            .chain(self.added.iter().map(|folder| PlaceName::Folder(folder_name(folder))))
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh folder under the system's temporary folder, removed when dropped.
    pub(crate) struct Tree(pub PathBuf);

    impl Tree {
        pub(crate) fn new(name: &str) -> Tree {
            let dir = std::env::temp_dir().join(format!("sp-find-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Tree(dir.canonicalize().unwrap())
        }

        pub(crate) fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A song written for these tests, as text a lyric file would hold.
    pub(crate) fn song_text(title: &str) -> String {
        format!(
            "{title}\n\nVerse 1\nA line written for {title}\nAnd another for the test\n\nChorus\nSing it again for {title}\n"
        )
    }

    /// The test tree: the fixture songbook, a second copy of it in another folder, a folder of
    /// text songs (and one note that is no song), a damaged songbook, a songbook in a hidden
    /// folder, a note on its own, and a library folder of SoftPresenter's own.
    pub(crate) fn tree(name: &str) -> Tree {
        use sp_model::songbook_examples::SONGBOOK;
        let tree = Tree::new(name);
        tree.write("Documents/Old laptop/Zkouška.sps", SONGBOOK);
        tree.write("Downloads/Zkouška.sps", SONGBOOK);
        for title in ["Morning Light", "River Song", "Evening Hymn"] {
            tree.write(&format!("Desktop/Hymns/{title}.txt"), song_text(title).as_bytes());
        }
        tree.write("Desktop/Hymns/Shopping list.txt", b"Milk\nBread\nEggs\n");
        tree.write("Desktop/Notes.txt", b"Call Sam on Tuesday.\n");
        let whole = SONGBOOK;
        tree.write("Downloads/Broken.sps", &whole[..5 * 1024]);
        tree.write("Documents/.hidden/Secret.sps", SONGBOOK);
        tree.write("Documents/SoftPresenter/.softpresenter-library", b"");
        tree.write("Documents/SoftPresenter/Songbooks/Kept.sps", SONGBOOK);
        tree
    }

    /// Runs a search through these places to its end and gives what it found.
    pub(crate) fn search(places: Vec<Looked>, library: Library, skip: Vec<PathBuf>) -> Vec<Source> {
        let search = start(1, places, library, skip, None, || {}).unwrap();
        let (found, ended) = listen(&search);
        assert_eq!(ended, Ended::Everywhere);
        found
    }

    /// What a search says until it ends, and how it ended.
    fn listen(search: &Search) -> (Vec<Source>, Ended) {
        let mut found = Vec::new();
        let began = Instant::now();
        loop {
            for said in search.said() {
                match said {
                    Found::Source(source) => found.push(*source),
                    Found::Looking(_) => {}
                    Found::Done(ended) => return (found, ended),
                }
            }
            assert!(began.elapsed() < Duration::from_secs(30), "the search ended");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A search of these places within these limits, with this stop.
    fn search_within(
        stop: Arc<AtomicBool>,
        places: Vec<Looked>,
        limits: Limits,
        wake: impl Fn() + Send + 'static,
    ) -> Search {
        let begin = Begin { id: 7, places, library: Library::default(), skip: Vec::new(), limits, after: None, marks };
        start_with(stop, begin, wake).unwrap()
    }

    /// The system's word, as these tests give it: a name with "(cloud)" is only in the cloud,
    /// and one with "(hidden)" is marked hidden.
    fn marked_by_name(entry: &std::fs::DirEntry) -> Marks {
        let name = entry.file_name().to_string_lossy().into_owned();
        Marks { hidden: name.contains("(hidden)"), cloud: name.contains("(cloud)") }
    }

    /// Searches these places, the system's word given by [`marked_by_name`].
    fn search_marked(places: Vec<Looked>) -> Vec<Source> {
        let begin = Begin {
            id: 9,
            places,
            library: Library::default(),
            skip: Vec::new(),
            limits: Limits::NORMAL,
            after: None,
            marks: marked_by_name,
        };
        let (found, ended) = listen(&start_with(Arc::new(AtomicBool::new(false)), begin, || {}).unwrap());
        assert_eq!(ended, Ended::Everywhere);
        found
    }

    fn at(tree: &Tree, names: &[&str]) -> Vec<Looked> {
        names
            .iter()
            .map(|name| Looked {
                name: PlaceName::Folder((*name).to_owned()),
                folders: vec![tree.0.join(name)],
                shallow: false,
            })
            .collect()
    }

    #[test]
    fn a_dry_run_lists_each_source_with_its_counts_and_why_one_couldnt_be_read() {
        let tree = tree("dry");
        let found = search(at(&tree, &["Documents", "Downloads", "Desktop"]), Library::default(), Vec::new());
        let row = |name: &str, place: &str| {
            found
                .iter()
                .find(|source| source.name == name && source.place.words() == place)
                .unwrap_or_else(|| panic!("{name} in {place}: {found:#?}"))
        };
        let first = row("Zpěvník pro zkoušku", "Documents \u{203a} Old laptop");
        assert_eq!((first.kind, first.songs, first.here, first.fresh()), (Kind::Songbook, 7, 0, 7));
        let second = row("Zpěvník pro zkoušku", "Downloads");
        assert_eq!((second.songs, second.fresh()), (7, 7));
        assert_eq!(first.new, second.new, "the two copies have the same new songs");
        let hymns = row("Hymns", "Desktop");
        assert_eq!((hymns.kind, hymns.songs, hymns.here), (Kind::SongFiles, 3, 0), "the shopping list is no song");
        let broken = row("Broken.sps", "Downloads");
        assert_eq!(broken.kind, Kind::Songbook);
        assert_eq!(broken.why, Some(load::Why::Reader(sp_model::Reason::RowLongerThanFile)), "{broken:?}");
        assert!(!broken.can_import());
        // Nothing else: not the hidden folder, not a note alone, not SoftPresenter's library.
        assert_eq!(found.len(), 4, "{found:#?}");
        assert!(found.iter().all(|source| !source.path.starts_with(tree.0.join("Documents/.hidden"))));
        assert!(found.iter().all(|source| !source.path.starts_with(tree.0.join("Documents/SoftPresenter"))));
    }

    #[test]
    fn a_songbook_that_couldnt_be_read_is_said_in_the_language_set_and_in_english_for_sp_and_the_log() {
        use crate::i18n::{Language, with_language};
        let tree = tree("ru");
        let found = search(at(&tree, &["Downloads"]), Library::default(), Vec::new());
        let broken = found.iter().find(|source| source.name == "Broken.sps").expect("the damaged songbook");
        // Found on the search's own thread, said in the window's language when it is drawn.
        let (name, line, right) = with_language(Language::Russian, || row_words(broken));
        assert_eq!(name, "Broken.sps");
        assert_eq!(
            line, "Библиотека \u{b7} из SoftProjector \u{b7} Downloads",
            "Downloads is the folder's own name here"
        );
        assert_eq!(right, "Не удалось прочитать: файл обрезан или повреждён (одна из записей длиннее всего файла)");
        assert!(!right.chars().any(|c| c.is_ascii_alphabetic()), "{right}");
        // The log's line says it in English, whatever the window speaks.
        let logged = with_language(Language::Russian, || unread_line(&broken.path, broken.why.as_ref().unwrap()));
        assert_eq!(
            logged,
            "a .sps file it found couldn't be read (5 KB, SQLite): the file is cut short or damaged (a row is longer than the file)",
            "its kind and size, never its name"
        );
        // So does `sp`'s listing.
        let english = with_language(Language::English, || row_words(broken));
        assert_eq!(
            english.2,
            "Couldn\u{2019}t be read: the file is cut short or damaged (a row is longer than the file)"
        );
    }

    #[test]
    fn songs_the_library_has_are_counted_as_here_and_a_source_with_nothing_new_cant_be_ticked() {
        let tree = tree("here");
        let book = load::import_songbook(&tree.0.join("Downloads/Zkouška.sps")).unwrap();
        // The library has three of the songbook's songs, as an import would have taken them in,
        // and one of the hymns.
        let mut songs: Vec<Arc<Presentation>> = book
            .songs
            .iter()
            .take(3)
            .map(|(song, _)| Arc::new(load::as_imported(song.clone(), Path::new("Zkouška.sps"))))
            .collect();
        let hymn = load::import_file(&tree.0.join("Desktop/Hymns/River Song.txt")).unwrap();
        songs.push(Arc::new(load::as_imported(hymn, Path::new("River Song.txt"))));
        let library = Library { ids: songs.iter().map(|song| song.id).collect(), songs };
        let found = search(at(&tree, &["Downloads", "Desktop"]), library, Vec::new());
        let book = found.iter().find(|source| source.name == "Zpěvník pro zkoušku").unwrap();
        assert_eq!((book.songs, book.here, book.fresh()), (7, 3, 4));
        let hymns = found.iter().find(|source| source.name == "Hymns").unwrap();
        assert_eq!((hymns.songs, hymns.here, hymns.fresh()), (3, 1, 2));
        // A library with every song: nothing new, nothing to tick.
        let all = load::import_songbook(&tree.0.join("Downloads/Zkouška.sps")).unwrap();
        let songs: Vec<Arc<Presentation>> = all
            .songs
            .into_iter()
            .map(|(song, _)| Arc::new(load::as_imported(song, Path::new("Zkouška.sps"))))
            .collect();
        let found = search(at(&tree, &["Downloads"]), Library { songs, ids: HashSet::new() }, Vec::new());
        let book = found.iter().find(|source| source.name == "Zpěvník pro zkoušku").unwrap();
        assert_eq!((book.here, book.fresh(), book.can_import()), (7, 0, false));
        let mut card = Card::default();
        card.add(book.clone());
        assert_eq!(card.ticked, [false], "a source with nothing new starts unticked");
    }

    #[test]
    fn two_copies_ticked_together_count_their_new_songs_once() {
        let tree = tree("twice");
        let mut card = Card::default();
        for source in search(at(&tree, &["Documents", "Downloads"]), Library::default(), Vec::new()) {
            card.add(source);
        }
        let copies = card.chosen();
        assert_eq!(copies.len(), 2, "both copies start ticked, the damaged songbook not: {:?}", card.ticked);
        assert_eq!(card.new_ticked(), 7, "the same seven songs");
        card.ticked[copies[0]] = false;
        assert_eq!(card.new_ticked(), 7);
        card.ticked[copies[1]] = false;
        assert_eq!(card.new_ticked(), 0);
        // A source that can't be read can't be ticked into the count.
        let broken = card.sources.iter().position(|source| source.why.is_some()).unwrap();
        card.ticked[broken] = true;
        assert_eq!((card.chosen().len(), card.new_ticked()), (0, 0));
    }

    #[test]
    fn the_library_and_data_folder_itself_is_never_listed() {
        let tree = tree("own");
        // The data folder named as one never to list, though it holds a songbook and no marker.
        tree.write("Documents/Data/Songbooks/Ours.sps", sp_model::songbook_examples::SONGBOOK);
        let skip = vec![tree.0.join("Documents/Data")];
        let found = search(at(&tree, &["Documents"]), Library::default(), skip.clone());
        assert!(found.iter().all(|source| !source.path.starts_with(&skip[0])), "{found:#?}");
        assert_eq!(found.len(), 1, "the old laptop's songbook alone: {found:#?}");
        // Looked for directly, as a folder added, it still isn't.
        let added = vec![Looked { name: PlaceName::Folder("Data".to_owned()), folders: skip.clone(), shallow: false }];
        assert!(search(added, Library::default(), skip).is_empty());
    }

    #[test]
    fn stop_ends_the_search_at_once_and_keeps_what_was_found() {
        let tree = tree("stop");
        // Stop is pressed as the first source found is told of (the second wake: the first says
        // where it is looking), on the search's own thread, so nothing races it.
        let stop = Arc::new(AtomicBool::new(false));
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (pressed, seen) = (stop.clone(), wakes.clone());
        let wake = move || {
            if seen.fetch_add(1, Ordering::Relaxed) == 1 {
                pressed.store(true, Ordering::Relaxed);
            }
        };
        let search = search_within(stop, at(&tree, &["Downloads", "Documents"]), Limits::NORMAL, wake);
        let (found, ended) = listen(&search);
        assert_eq!(ended, Ended::Stopped, "it says it was stopped");
        // Downloads' first songbook (the damaged one, by name) is kept; nothing after it.
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].name, "Broken.sps");
        // A stopped search's late messages are never taken for another's.
        let (tx, rx) = channel();
        tx.send((6, Found::Done(Ended::Everywhere))).unwrap();
        tx.send((7, Found::Looking(PlaceName::Folder("Here".to_owned())))).unwrap();
        let other = Search { id: 7, stop: Arc::new(AtomicBool::new(false)), rx, thread: None };
        assert!(matches!(other.said().as_slice(), [Found::Looking(_)]));
        // A thread that ended without saying how went wrong: the search failed, never finished.
        drop(tx);
        assert!(matches!(other.said().as_slice(), [Found::Done(Ended::Failed)]));
    }

    #[test]
    fn a_songbook_copied_while_open_or_cut_short_says_so_in_its_row() {
        let tree = Tree::new("cautions");
        tree.write("Old laptop/spData.sqlite", sp_model::songbook_examples::DATABASE);
        tree.write("Old laptop/spData.sqlite-wal", b"changes not yet in the database");
        // A text songbook in SoftProjector's layout, cut part way through its third song.
        let lines: Vec<String> = (1..=3)
            .map(|n| format!("{n}#$#Song {n}#$#0#$##$##$##$#Verse 1@%Line {n} for the test#$##$#left#$#"))
            .collect();
        let whole = format!("##4\n##Cut book\n##\n{}", lines.join("\n"));
        tree.write("Downloads/Cut.sps", &whole.as_bytes()[..whole.len() - 12]);
        let found = search(at(&tree, &["Old laptop", "Downloads"]), Library::default(), Vec::new());
        // Copied while open is about the file: said once, on its first row, for all its libraries.
        let open: Vec<&Source> = found.iter().filter(|source| source.path.ends_with("spData.sqlite")).collect();
        assert!(open.len() > 1, "the database holds more than one songbook: {open:#?}");
        let said = format!(
            "Copied while it was open in SoftProjector: the latest changes to this file\u{2019}s {} libraries may be missing",
            open.len()
        );
        assert_eq!(open[0].caution.as_deref(), Some(said.as_str()), "{}", open[0].name);
        for source in &open[1..] {
            assert_eq!(source.caution, None, "{} says it again", source.name);
        }
        assert!(open.iter().all(|source| source.can_import()), "they can still be brought in");
        // A file of one songbook keeps the row's own words; what is about one songbook is said
        // on each row, after what is about the file.
        let open = load::Caution { said: String::new(), short: "Open.".to_owned(), whole_file: true };
        let cut = load::Caution { said: String::new(), short: "Cut.".to_owned(), whole_file: false };
        assert_eq!(rows_cautions(vec![open.clone()], 1), [Some("Open.".to_owned())]);
        assert_eq!(
            rows_cautions(vec![cut, open], 3),
            [
                Some(
                    "Copied while it was open in SoftProjector: the latest changes to this file\u{2019}s 3 libraries may be missing Cut."
                        .to_owned()
                ),
                Some("Cut.".to_owned()),
                Some("Cut.".to_owned()),
            ]
        );
        assert_eq!(rows_cautions(Vec::new(), 2), [None, None]);
        let cut = found.iter().find(|source| source.name == "Cut book").unwrap();
        assert_eq!(cut.songs, 2, "the song it ends in is left out");
        assert_eq!(
            cut.caution.as_deref(),
            Some("Cut short when it was copied or downloaded: its last songs are missing"),
            "{cut:#?}"
        );
        // A count and its word stay together under the row.
        assert_eq!(row_words(cut).1, "Library \u{b7} from SoftProjector \u{b7} Downloads \u{b7} 2\u{a0}songs");
    }

    #[test]
    fn a_song_twice_in_one_source_is_counted_once_and_said() {
        // Two files of one song (its title and words) and another: three songs, two new, one
        // found twice, and nothing "already here" (the library has neither).
        let tree = Tree::new("twice-in-one");
        tree.write("Songs/Morning Light.cho", song_text("Morning Light").as_bytes());
        tree.write("Songs/Morning Light copy.cho", song_text("Morning Light").as_bytes());
        tree.write("Songs/River Song.cho", song_text("River Song").as_bytes());
        let found = search(at(&tree, &["Songs"]), Library::default(), Vec::new());
        assert_eq!(found.len(), 1, "{found:#?}");
        let songs = &found[0];
        assert_eq!((songs.songs, songs.fresh(), songs.here, songs.twice), (3, 2, 0, 1), "{songs:#?}");
        assert_eq!(row_words(songs).2, "2 new \u{b7} 1 found twice");
        // A song here already and found twice is here, not twice.
        let river = load::import_file(&tree.0.join("Songs/River Song.cho")).unwrap();
        let library = Library {
            songs: vec![Arc::new(load::as_imported(river, Path::new("River Song.cho")))],
            ..Library::default()
        };
        tree.write("Songs/River Song copy.cho", song_text("River Song").as_bytes());
        let found = search(at(&tree, &["Songs"]), library, Vec::new());
        let songs = &found[0];
        assert_eq!((songs.songs, songs.fresh(), songs.here, songs.twice), (4, 1, 2, 1), "{songs:#?}");
        assert_eq!(row_words(songs).2, "1 new \u{b7} 2 already here \u{b7} 1 found twice");
        // What came of an import names only the sources songs came in from, and says the songs
        // found in more than one place came in once.
        let brought = Brought {
            came: 7,
            from: vec![("Zkouška".to_owned(), 7), ("Zkouška".to_owned(), 0)],
            left_out: 0,
            twice: 7,
            failed: false,
        };
        assert_eq!(
            brought_words(&brought).concat(),
            "7 songs came in: Zkouška (7). 7 found in more than one place came in once."
        );
    }

    #[test]
    fn files_only_in_the_cloud_are_never_opened_and_are_said() {
        let tree = Tree::new("cloud");
        // None of them is a songbook or a song: read, each would be said to be unreadable.
        tree.write("Downloads/Hymns (cloud).sps", b"not read");
        tree.write("Downloads/Other (cloud).zip", b"not read");
        tree.write("Downloads/Mail (cloud).sqlite", b"not read");
        tree.write("Mixed/Morning Light.cho", song_text("Morning Light").as_bytes());
        tree.write("Mixed/River Song (cloud).cho", b"not read");
        tree.write("Mixed/Evening (cloud).cho", b"not read");
        tree.write("Away/Lantern (cloud).pro", b"not read");
        tree.write("Away/Notes (cloud).txt", b"not read");
        let found = search_marked(at(&tree, &["Downloads", "Mixed", "Away"]));
        // The songbook only in the cloud is a row saying so, never read; a zip or another
        // program's database there may be no songbook, and isn't one.
        let downloads: Vec<&Source> =
            found.iter().filter(|source| source.place.place == PlaceName::Folder("Downloads".to_owned())).collect();
        assert_eq!(downloads.len(), 1, "{found:#?}");
        let hymns = downloads[0];
        assert_eq!((hymns.name.as_str(), hymns.cloud, hymns.why.as_ref()), ("Hymns (cloud).sps", true, None));
        assert!(!hymns.can_import());
        let (_, line, right) = row_words(hymns);
        assert_eq!(
            (line.as_str(), right.as_str()),
            ("Library \u{b7} from SoftProjector \u{b7} Downloads", "In the cloud, not on this computer")
        );
        // A folder with a song read and two only in the cloud: the one, and a caution.
        let mixed = found.iter().find(|source| source.name == "Mixed").unwrap();
        assert_eq!((mixed.songs, mixed.cloud), (1, false));
        let caution = "2 more song files here are only in the cloud, so they weren\u{2019}t looked at";
        assert_eq!(mixed.caution.as_deref(), Some(caution));
        // A folder of song documents all in the cloud: a library in the cloud, not to tick.
        let away = found.iter().find(|source| source.name == "Away").unwrap();
        assert_eq!((away.kind, away.cloud, away.songs, away.can_import()), (Kind::Library, true, 0, false));
        assert_eq!(found.len(), 3, "{found:#?}");
    }

    #[test]
    fn folders_marked_hidden_and_safaris_downloads_under_way_are_passed_over() {
        use sp_model::songbook_examples::SONGBOOK;
        let tree = Tree::new("hidden");
        tree.write("Docs/Kept (hidden)/Zkouška.sps", SONGBOOK);
        tree.write("Docs/Zkouška.sps.download/Zkouška.sps", SONGBOOK);
        tree.write("Docs/Secret (hidden).sps", SONGBOOK);
        tree.write("Docs/Shown/Zkouška.sps", SONGBOOK);
        let found = search_marked(at(&tree, &["Docs"]));
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].place.words(), "Docs \u{203a} Shown");
    }

    #[test]
    fn a_panic_on_the_searchs_thread_ends_it_as_failed() {
        let tree = tree("panic");
        let begin = Begin {
            id: 10,
            places: at(&tree, &["Downloads"]),
            library: Library::default(),
            skip: Vec::new(),
            limits: Limits::NORMAL,
            after: None,
            marks: |_| panic!("a file the search couldn't take (a test)"),
        };
        let (found, ended) = listen(&start_with(Arc::new(AtomicBool::new(false)), begin, || {}).unwrap());
        assert_eq!(ended, Ended::Failed);
        assert!(found.is_empty());
        // And an import that went wrong says so.
        let brought = Brought { came: 3, from: vec![("Zkouška".to_owned(), 3)], left_out: 0, twice: 0, failed: true };
        assert_eq!(
            brought_words(&brought).concat(),
            "3 songs came in: Zkouška (3). Something went wrong, so it stopped before every song had come in (the log says what)."
        );
    }

    #[test]
    fn the_systems_marks_are_read_as_they_are_defined() {
        // Windows' attributes (winnt.h): hidden; offline, recalled on open or on reading.
        assert_eq!(windows_marks(0), Marks::default());
        assert_eq!(windows_marks(0x2), Marks { hidden: true, cloud: false });
        for cloud in [0x1000, 0x4_0000, 0x40_0000, 0x40_0000 | 0x400 | 0x10_0000] {
            assert!(windows_marks(cloud).cloud, "{cloud:#x}");
        }
        // A file OneDrive keeps on the computer: a reparse point, archived, pinned. It is read.
        assert_eq!(windows_marks(0x400 | 0x20 | 0x8_0000), Marks::default());
        // A Mac's flags (sys/stat.h): SF_DATALESS is the cloud's; UF_HIDDEN is not.
        assert!(mac_dataless(0x4000_0000));
        assert!(!mac_dataless(0x8000));
        assert!(!mac_dataless(0));
    }

    #[test]
    fn a_search_cut_short_by_its_limits_says_which() {
        let tree = tree("limits");
        let places = || at(&tree, &["Downloads", "Documents", "Desktop"]);
        let no_stop = || Arc::new(AtomicBool::new(false));
        let (all, ended) = listen(&search_within(no_stop(), places(), Limits::NORMAL, || {}));
        assert_eq!(ended, Ended::Everywhere);
        let (few, ended) = listen(&search_within(no_stop(), places(), Limits { entries: 3, ..Limits::NORMAL }, || {}));
        assert_eq!(ended, Ended::TooMany, "it says it looked at as many files as it looks at");
        assert!(few.len() < all.len(), "{} of {}", few.len(), all.len());
        let (none, ended) =
            listen(&search_within(no_stop(), places(), Limits { time: Duration::ZERO, ..Limits::NORMAL }, || {}));
        assert_eq!(ended, Ended::TooLong, "it says it ran out of time");
        assert!(none.is_empty());
        // Each says so in words; one that looked everywhere says nothing more.
        assert_eq!(Ended::Everywhere.words(), None);
        assert_eq!(
            Ended::TooLong.words().unwrap(),
            "It stopped after 90 seconds, the longest it looks, before it had looked everywhere."
        );
        assert_eq!(
            Ended::TooMany.words().unwrap(),
            "It stopped after looking at 200,000 files and folders, the most it looks at, before it had looked everywhere."
        );
    }

    #[test]
    fn stop_reaches_into_the_file_being_read_and_lists_nothing_of_it() {
        // A songbook of 2,000 songs and a folder of 40 song files, each stopped part way: the
        // reading ends within a few songs or files, and no row is made of what was read.
        let tree = Tree::new("stop-inside");
        let mut text = String::from("##1\nBig\n##\n");
        for n in 1..=2000 {
            text.push_str(&format!("{n}#$#Song {n}#$#0#$##$##$##$#Verse 1@%Line {n} for the test\n"));
        }
        let book = tree.write("Big.sps", text.as_bytes());
        for n in 0..40 {
            tree.write(&format!("Folder/Song {n}.cho"), song_text(&format!("Song {n}")).as_bytes());
        }
        let place = Where { place: PlaceName::Folder("Here".to_owned()), parts: Vec::new() };
        let known = Known::of(&Library::default());
        let asked = std::cell::Cell::new(0);
        let until = || {
            asked.set(asked.get() + 1);
            asked.get() > 10
        };
        assert_eq!(songbook_sources(&book, &place, &known, &until), Err(Cut));
        assert!(asked.get() < 20, "it stopped within a few songs, not after {} of them", asked.get());
        asked.set(0);
        let files: Vec<PathBuf> = (0..40).map(|n| tree.0.join(format!("Folder/Song {n}.cho"))).collect();
        let lyrics = |files: Vec<PathBuf>| Files {
            documents: Vec::new(),
            lyrics: files,
            texts: Vec::new(),
            cloud: 0,
            cloud_documents: false,
        };
        let cut = folder_source(&tree.0.join("Folder"), &place, lyrics(files.clone()), &known, &until);
        assert_eq!(cut, Err(Cut));
        assert!(asked.get() < 20, "{}", asked.get());
        // Unstopped, both are read whole.
        let whole = songbook_sources(&book, &place, &known, &|| false).unwrap();
        assert_eq!(whole[0].songs, 2000);
        let whole = folder_source(&tree.0.join("Folder"), &place, lyrics(files), &known, &|| false);
        assert_eq!(whole.unwrap().unwrap().songs, 40);
    }

    #[test]
    fn find_again_waits_for_the_search_before_to_end() {
        // The search before is still stopping (its thread is held here): the next one doesn't
        // look anywhere until it has ended, so two searches never look at once.
        let tree = tree("one-at-a-time");
        let (hold, held) = channel::<()>();
        let before = std::thread::spawn(move || {
            let _ = held.recv();
        });
        let begin = Begin {
            id: 8,
            places: at(&tree, &["Downloads"]),
            library: Library::default(),
            skip: Vec::new(),
            limits: Limits::NORMAL,
            after: Some(before),
            marks,
        };
        let search = start_with(Arc::new(AtomicBool::new(false)), begin, || {}).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(search.said().is_empty(), "it waits for the search before");
        hold.send(()).unwrap();
        let (found, ended) = listen(&search);
        assert_eq!(ended, Ended::Everywhere);
        assert!(!found.is_empty());
    }

    #[test]
    fn a_database_gives_a_row_for_each_songbook_in_it_and_a_stray_database_none() {
        let tree = Tree::new("database");
        tree.write("SoftProjector/spData.sqlite", sp_model::songbook_examples::DATABASE);
        tree.write("SoftProjector/Mail.sqlite", b"SQLite format 3\0 but not a songbook at all");
        let places = vec![Looked {
            name: PlaceName::Usual(Usual::SoftProjector),
            folders: vec![tree.0.join("SoftProjector")],
            shallow: true,
        }];
        let found = search(places, Library::default(), Vec::new());
        assert!(found.len() >= 2, "{found:#?}");
        assert!(found.iter().all(|source| source.place.words() == "SoftProjector" && source.kind == Kind::Songbook));
        assert!(found.iter().all(|source| source.why.is_none()), "the stray database isn't listed");
        let all = load::import_songbook(&tree.0.join("SoftProjector/spData.sqlite")).unwrap().songs.len();
        assert_eq!(found.iter().map(|source| source.songs).sum::<usize>(), all, "every song in one row or another");
        // Read again, each row gives its own songbook's songs.
        for source in &found {
            assert_eq!(read_again(source).0.len(), source.songs, "{}", source.name);
        }
    }

    #[test]
    fn a_folder_of_song_documents_is_a_library_and_bundles_and_caches_are_passed_over() {
        let tree = Tree::new("documents");
        tree.write("Documents/Libraries/Youth/Lantern Song.pro", &sp_model::pro_example::document());
        tree.write("Documents/Libraries/Youth/Notes.txt", b"Bring the capo.\n");
        tree.write("Documents/Photos.photoslibrary/Inside.sps", sp_model::songbook_examples::SONGBOOK);
        tree.write("Documents/Project/target/CACHEDIR.TAG", b"Signature: 8a477f597d28d172789f06886806bc55");
        tree.write("Documents/Project/target/Song.cho", b"{title: Built}\n{start_of_verse}\nNo\n{end_of_verse}\n");
        tree.write("Documents/node_modules/Song.cho", b"{title: Kept away}\n");
        tree.write(
            "Documents/Chords/One.cho",
            b"{title: One}\n{start_of_verse}\nA single ChordPro song\n{end_of_verse}\n",
        );
        let found = search(at(&tree, &["Documents"]), Library::default(), Vec::new());
        let names: Vec<(&str, Kind, String)> =
            found.iter().map(|source| (source.name.as_str(), source.kind, source.place.words())).collect();
        assert_eq!(
            names,
            [
                ("Chords", Kind::SongFiles, "Documents".to_owned()),
                ("Youth", Kind::Library, "Documents \u{203a} Libraries".to_owned())
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_never_followed_and_a_folder_that_cant_be_read_is_said() {
        use std::os::unix::fs::PermissionsExt;
        let tree = tree("links");
        let elsewhere = Tree::new("elsewhere");
        elsewhere.write("Far/Away.sps", sp_model::songbook_examples::SONGBOOK);
        std::os::unix::fs::symlink(&elsewhere.0, tree.0.join("Desktop/Link")).unwrap();
        let found = search(at(&tree, &["Desktop"]), Library::default(), Vec::new());
        assert!(found.iter().all(|source| !source.path.starts_with(&elsewhere.0)), "{found:#?}");
        // A place closed to the app is a row saying so.
        let closed = tree.0.join("Closed");
        std::fs::create_dir_all(&closed).unwrap();
        std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable = std::fs::read_dir(&closed).is_ok();
        let found = search(at(&tree, &["Closed"]), Library::default(), Vec::new());
        std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o755)).unwrap();
        if !readable {
            assert_eq!(found.len(), 1);
            assert_eq!((found[0].kind, found[0].can_import()), (Kind::Place, false));
            assert!(found[0].why.is_some());
        }
    }
}
