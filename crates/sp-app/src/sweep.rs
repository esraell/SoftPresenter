//! Tidying the library folder. Pictures and videos in `Images/` and `Media/` that nothing in the
//! library uses any more (a deleted sermon deck's pages, a picture taken off a slide), and files
//! a crash or a quit left half-written, go to the Trash. It runs on a thread of its own at
//! start-up, at most once a day.
//!
//! These are the owner's files, so it is careful:
//!
//! - **Every file that can name a picture is read first:** songs, themes, every settings file
//!   (the media bin and media playlists, playlists, props, stage layouts, macros, preferences,
//!   screens…), `Not saved/` and the backups, files left half-written, and songs kept outside the
//!   library that a playlist, library, macro or slide names by path (or that are open this run).
//!   Every file the app puts in `Images/` or `Media/` is named by a new id and its ending, and
//!   whatever names it (a bare name, `Media/<name>`, a whole path from wherever the library was)
//!   holds that id, so a file is in use if its id appears anywhere in them.
//! - **If any of them can't be read or isn't whole,** nothing moves that run and the log says why.
//! - **Only files named the way the app names them** (an id and an ending) ever move; anything
//!   put there by hand stays, as do folders and links.
//! - **Nothing changed in the last 7 days moves,** by the newest of a file's modified, created and
//!   status-changed times: a picture copied in keeps its original's modified and created times,
//!   so only the last says it is new.
//! - **It stops** if a song pack is being opened, or a song, theme or settings file is saved while
//!   it works (a file it judged unused may just have become used).
//! - **Files go to the Trash,** never deleted outright, and the log names each one.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::i18n::{tf, tn};
use serde_json::Value;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// A picture or video unused and unchanged for this long goes to the Trash.
const UNUSED_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A half-written file or an abandoned song pack folder this old is left over, not in use.
const LEFT_FOR: Duration = DAY;
/// How long after start-up the tidy begins, so the files the start writes are written first.
const START_AFTER: Duration = Duration::from_secs(5);
/// The file, in the library folder, that says when the library was last tidied.
pub const LAST_RUN_FILE: &str = ".last-tidy";
/// The folder a song pack is unpacked into while it is opened (see `open_pack` in main.rs).
const OPENING_PACK: &str = ".opening-pack-";
/// Folders at the top of the library that hold no names of pictures: the two tidied, the
/// Bibles, the command line's socket, presentations being turned into slides, and the NDI
/// downloaded on Linux.
const NOT_READ: [&str; 6] = ["Images", "Media", "Bibles", "control", "Converting", "NDI"];
/// The two folders tidied.
const TIDIED: [&str; 2] = ["Images", "Media"];
/// Endings of the app's own files, which must read whole or the tidy stops.
const OWN_ENDINGS: [&str; 5] = ["json", "sppres", "pcpres", "sptheme", "pctheme"];
/// A file of another kind larger than this is not read for names; none of the app's are.
const LARGEST_OTHER: u64 = 16 * 1024 * 1024;
/// One of the app's own files larger than this can't be read, and stops the tidy.
const LARGEST_OWN: u64 = 64 * 1024 * 1024;
/// How deep into folders inside the library the reading goes.
const DEEPEST: usize = 12;

/// Starts the day's tidy on a thread of its own, if it is due. `outside_songs` are the files of
/// the songs open this run (those kept outside the library are read for names too). `told` hears
/// what to tell the operator, only when something went to the Trash. A quiet run (a test, a
/// capture) doesn't tidy, unless the testing aid asks.
pub fn start(quiet: bool, outside_songs: Vec<PathBuf>, told: impl FnOnce(String) + Send + 'static) {
    // Testing aid: SOFTPRESENTER_SWEEP=<days> tidies a quiet copy too, as if that many more days
    // had passed since each file changed. Only for a copy with its own data folder.
    let own_folder = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
    let ahead = std::env::var("SOFTPRESENTER_SWEEP")
        .ok()
        .and_then(|days| days.trim().parse::<u64>().ok())
        .filter(|_| own_folder)
        .map(|days| DAY * u32::try_from(days.min(3650)).unwrap_or(0));
    if quiet && ahead.is_none() {
        return;
    }
    let Some(library) = crate::bible_store::data_dir() else { return };
    let work = move || {
        std::thread::sleep(START_AFTER);
        let now = SystemTime::now();
        if !due(&library, now) {
            return;
        }
        // Noted before it starts, so a tidy that fails is tried again tomorrow, not at every start.
        if let Err(e) = std::fs::write(library.join(LAST_RUN_FILE), seconds(now).to_string()) {
            crate::applog::problem(
                "tidy",
                &format!("didn't tidy the library: could not note when it was tidied ({e})"),
            );
            return;
        }
        // Each move is logged as it is made, so the log says where every file came from even if
        // the app quits part of the way through.
        let mut trash = |path: &Path| -> Result<(), String> {
            let size = std::fs::symlink_metadata(path).ok().filter(|meta| meta.is_file()).map(|meta| meta.len());
            to_trash(path)?;
            crate::applog::event("tidy", &moved_line(&library, path, size));
            Ok(())
        };
        let outcome = run(&library, &outside_songs, now + ahead.unwrap_or_default(), &mut trash);
        for line in outcome.log_lines() {
            crate::applog::event("tidy", &line);
        }
        if let Some(said) = outcome.notice() {
            told(said);
        }
    };
    if let Err(e) = std::thread::Builder::new().name("library tidy".to_owned()).spawn(work) {
        crate::applog::problem("tidy", &format!("didn't tidy the library: {e}"));
    }
}

/// Moves a file or folder to the Trash. The `trash` crate's usual way on a Mac asks the Finder,
/// which can ask the operator's permission, plays the Trash sound, and holds the caller up for
/// as long as the Finder takes to answer (minutes, when it doesn't). The system's own call is
/// used instead, which does none of that; everything the app puts in the Trash goes this way.
/// What it costs: the Finder offers no "Put Back" for a file moved so.
pub fn to_trash(path: &Path) -> Result<(), String> {
    #[allow(unused_mut)]
    let mut context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }
    context.delete(path).map_err(|e| e.to_string())
}

/// True when the library hasn't been tidied in the last day (or the note of when can't be read,
/// or says a time to come, as after the clock was put back).
fn due(library: &Path, now: SystemTime) -> bool {
    let last =
        std::fs::read_to_string(library.join(LAST_RUN_FILE)).ok().and_then(|text| text.trim().parse::<u64>().ok());
    match last {
        Some(last) => {
            let now = seconds(now);
            last > now || now - last >= DAY.as_secs()
        }
        None => true,
    }
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// What one tidy did.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Pictures and videos moved to the Trash, with their sizes.
    pub moved: Vec<(PathBuf, u64)>,
    /// Half-written files and abandoned song pack folders moved to the Trash.
    pub leftovers: Vec<PathBuf>,
    /// Half-written files kept because they may hold the only copy of something, and why.
    pub kept: Vec<String>,
    /// Why the tidy stopped before the end, if it did.
    pub stopped: Option<String>,
    /// True when it stopped before moving anything, because the library couldn't be read whole.
    pub unread: bool,
}

/// The log's line for a file or folder moved to the Trash, with its size if it is a file.
fn moved_line(library: &Path, path: &Path, size: Option<u64>) -> String {
    let tidied = path.parent().is_some_and(|folder| TIDIED.iter().any(|tidied| folder == library.join(tidied)));
    let size = size.map_or_else(String::new, |size| format!(" ({})", size_words(size)));
    let why = if tidied { "nothing uses it" } else { "it was left over" };
    format!("moved {} to the Trash{size}; {why}", shown(library, path))
}

impl Outcome {
    /// What the log says once the tidy is done (each move is logged as it is made): what was
    /// kept, why it stopped, and the total.
    pub fn log_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.unread {
            lines.push(format!(
                "nothing moved: {}",
                self.stopped.as_deref().unwrap_or("the library could not be read whole")
            ));
            return lines;
        }
        lines.extend(self.kept.iter().cloned());
        let count = self.moved.len() + self.leftovers.len();
        match (&self.stopped, count) {
            (Some(why), 0) => lines.push(format!("nothing moved: {why}")),
            (Some(why), _) => lines.push(format!("stopped: {why}")),
            (None, _) => {}
        }
        if count == 0 {
            if self.stopped.is_none() {
                lines.push("nothing to move".to_owned());
            }
        } else {
            let bytes: u64 = self.moved.iter().map(|(_, size)| size).sum();
            lines.push(format!(
                "moved {count} file{} to the Trash ({} of pictures and videos)",
                if count == 1 { "" } else { "s" },
                size_words(bytes)
            ));
        }
        lines
    }

    /// A short line for the window, only when something went to the Trash.
    pub fn notice(&self) -> Option<String> {
        let count = self.moved.len();
        let videos = self.moved.iter().filter(|(path, _)| crate::media::is_video(path)).count();
        let bytes: u64 = self.moved.iter().map(|(_, size)| size).sum();
        let shown = size_words(bytes);
        let size: &[(&str, &str)] = &[("size", &shown)];
        let media = match (count - videos, videos) {
            (0, 0) => None,
            (n, 0) => Some(tn(
                "notices.tidy.pictures",
                n as u64,
                "{n} unused picture ({size})",
                "{n} unused pictures ({size})",
                size,
            )),
            (0, n) => Some(tn(
                "notices.tidy.videos",
                n as u64,
                "{n} unused video ({size})",
                "{n} unused videos ({size})",
                size,
            )),
            _ => Some(tn(
                "notices.tidy.media",
                count as u64,
                "{n} unused pictures and videos ({size})",
                "{n} unused pictures and videos ({size})",
                size,
            )),
        };
        let left = match self.leftovers.len() {
            0 => None,
            n => Some(tn("notices.tidy.leftovers", n as u64, "{n} leftover file", "{n} leftover files", &[])),
        };
        match (media, left) {
            (Some(media), Some(left)) => Some(tf(
                "notices.tidy.both",
                "Tidied the library: {media} and {left} went to the Trash.",
                &[("media", &media), ("left", &left)],
            )),
            (Some(what), None) | (None, Some(what)) => {
                Some(tf("notices.tidy.one", "Tidied the library: {what} went to the Trash.", &[("what", &what)]))
            }
            (None, None) => None,
        }
    }
}

/// A size in words: "820 KB", "34 MB", "1.2 GB".
fn size_words(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        format!("{:.0} KB", kb.max(1.0))
    } else if kb < 1024.0 * 1024.0 {
        format!("{:.0} MB", kb / 1024.0)
    } else {
        format!("{} GB", crate::i18n::number(kb / (1024.0 * 1024.0), 1))
    }
}

/// Tidies the library in `library`, with `now` as the time, moving each file with `trash`.
/// `outside_songs` are song files open this run, read for names when they are outside the
/// library. Reads nothing else from the app, so a test can give it a folder of its own.
pub fn run(
    library: &Path,
    outside_songs: &[PathBuf],
    now: SystemTime,
    trash: &mut dyn FnMut(&Path) -> Result<(), String>,
) -> Outcome {
    let mut outcome = Outcome::default();
    // How the folders saves land in stand before anything is read: a save while the tidy works
    // changes one, and the tidy then stops.
    let before = stamps(library);
    let names = match read_names(library, outside_songs) {
        Ok(names) => names,
        Err(why) => {
            outcome.stopped = Some(why);
            outcome.unread = true;
            return outcome;
        }
    };
    // Pictures and videos nothing names.
    for folder in TIDIED {
        let folder = library.join(folder);
        if names.whole_folder(&folder) {
            continue;
        }
        for (path, id) in named_files(&folder) {
            if names.ids.contains(&id) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if !meta.is_file() || !unchanged_for(&meta, now, UNUSED_FOR) {
                continue;
            }
            if let Some(why) = busy(library, &before, now) {
                outcome.stopped = Some(why);
                return outcome;
            }
            match trash(&path) {
                Ok(()) => outcome.moved.push((path, meta.len())),
                Err(e) => {
                    outcome.stopped = Some(format!("could not move {} to the Trash ({e})", file_name(&path)));
                    return outcome;
                }
            }
        }
    }
    if let Some(why) = busy(library, &before, now) {
        outcome.stopped = Some(why);
        return outcome;
    }
    tidy_leftovers(library, now, trash, &mut outcome);
    outcome
}

/// Why the tidy must stop now, if it must: a song pack is being opened (it may be about to use a
/// picture already here), or a song, theme or settings file was saved since the names were read.
fn busy(library: &Path, before: &[Option<SystemTime>], now: SystemTime) -> Option<String> {
    if stamps(library) != before {
        return Some("the library's files changed while it was being tidied".to_owned());
    }
    let Ok(entries) = std::fs::read_dir(library) else {
        return Some("the library folder could not be read".to_owned());
    };
    let opening = entries.flatten().any(|entry| {
        entry.file_name().to_string_lossy().starts_with(OPENING_PACK)
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
            && !folder_unchanged_for(&entry.path(), now, LEFT_FOR)
    });
    opening.then(|| "a song pack is being opened".to_owned())
}

/// When each folder that saves land in last changed. Every save writes beside its file and
/// renames it into place, which changes its folder.
fn stamps(library: &Path) -> Vec<Option<SystemTime>> {
    [library.to_path_buf(), library.join("Songs"), library.join("Themes"), library.join("Not saved")]
        .iter()
        .map(|folder| std::fs::metadata(folder).and_then(|meta| meta.modified()).ok())
        .collect()
}

/// When a file last changed: the newest of its modified, created and status-changed times.
fn last_changed(meta: &std::fs::Metadata) -> Option<SystemTime> {
    let mut newest = meta.modified().ok()?;
    if let Ok(created) = meta.created() {
        newest = newest.max(created);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(secs), Ok(nanos)) = (u64::try_from(meta.ctime()), u32::try_from(meta.ctime_nsec())) {
            newest = newest.max(SystemTime::UNIX_EPOCH + Duration::new(secs, nanos));
        }
    }
    Some(newest)
}

/// True when the file hasn't changed for at least `age` before `now`. A time to come, or one
/// that can't be read, is never old.
fn unchanged_for(meta: &std::fs::Metadata, now: SystemTime, age: Duration) -> bool {
    last_changed(meta).is_some_and(|changed| now.duration_since(changed).is_ok_and(|since| since >= age))
}

/// The same for a folder and everything in it.
fn folder_unchanged_for(folder: &Path, now: SystemTime, age: Duration) -> bool {
    fn walk(path: &Path, now: SystemTime, age: Duration, depth: usize) -> bool {
        let Ok(meta) = std::fs::symlink_metadata(path) else { return false };
        if !unchanged_for(&meta, now, age) {
            return false;
        }
        if !meta.is_dir() {
            return true;
        }
        if depth > DEEPEST {
            return false;
        }
        let Ok(mut entries) = std::fs::read_dir(path) else { return false };
        entries.all(|entry| entry.is_ok_and(|entry| walk(&entry.path(), now, age, depth + 1)))
    }
    walk(folder, now, age, 0)
}

/// The files directly in a folder named as the app names them, an id and an ending, with the id.
fn named_files(folder: &Path) -> Vec<(PathBuf, u128)> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<(PathBuf, u128)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = app_named(name.to_str()?)?;
            Some((entry.path(), id))
        })
        .collect();
    files.sort();
    files
}

/// The id in a file name the app made: 36 characters of id, then nothing or a dot and an ending.
fn app_named(name: &str) -> Option<u128> {
    let bytes = name.as_bytes();
    let id = id_at(bytes.get(..36)?)?;
    match bytes.get(36) {
        None => Some(id),
        Some(b'.') if bytes.len() > 37 && bytes[37..].iter().all(u8::is_ascii_alphanumeric) => Some(id),
        _ => None,
    }
}

/// The id written in 36 characters such as `0f5f6b0e-1111-4c4c-8888-0000000000aa` (either case).
fn id_at(text: &[u8]) -> Option<u128> {
    if text.len() != 36 {
        return None;
    }
    let mut id = 0u128;
    for (at, &c) in text.iter().enumerate() {
        if matches!(at, 8 | 13 | 18 | 23) {
            if c != b'-' {
                return None;
            }
            continue;
        }
        id = id << 4 | u128::from(char::from(c).to_digit(16)?);
    }
    Some(id)
}

/// Notes every id written anywhere in some text.
fn note_ids(text: &[u8], ids: &mut HashSet<u128>) {
    let mut at = 0;
    while at + 36 <= text.len() {
        match id_at(&text[at..at + 36]) {
            Some(id) => {
                ids.insert(id);
                at += 36;
            }
            None => at += 1,
        }
    }
}

/// Everything the library's files name.
#[derive(Default)]
struct Names {
    /// Every id written in them.
    ids: HashSet<u128>,
    /// Folders named as wholes (a media playlist follows a folder): every file in one is in use.
    folders: Vec<PathBuf>,
    /// Songs named by their file's path (without its ending), still to read.
    song_keys: Vec<String>,
}

impl Names {
    /// True when something names this folder as a whole, so nothing in it may move.
    fn whole_folder(&self, folder: &Path) -> bool {
        let Ok(folder) = std::fs::canonicalize(folder) else { return false };
        let folder = folder.to_string_lossy().to_lowercase();
        self.folders.iter().any(|named| {
            std::fs::canonicalize(named).is_ok_and(|named| named.to_string_lossy().to_lowercase() == folder)
        })
    }

    /// Notes what a JSON value names: ids (from the text, separately), followed folders and
    /// songs named by path. `field` is the name the value is kept under.
    fn note_value(&mut self, value: &Value, field: Option<&str>) {
        match value {
            Value::String(text) => self.note_string(text, field),
            Value::Array(items) => items.iter().for_each(|item| self.note_value(item, field)),
            Value::Object(fields) => fields.iter().for_each(|(name, item)| self.note_value(item, Some(name))),
            _ => {}
        }
    }

    fn note_string(&mut self, text: &str, field: Option<&str>) {
        let path = Path::new(text);
        if !path.is_absolute() {
            return;
        }
        // A song named by its file's path: a playlist's or library's song, a macro's or a slide's
        // "go to song", the song the announcements start with.
        if matches!(field, Some("key" | "song" | "songs")) {
            self.song_keys.push(text.to_owned());
        }
        // A folder a media playlist follows, or anything naming one of the tidied folders itself.
        let names_tidied =
            path.file_name().is_some_and(|name| TIDIED.iter().any(|tidied| name.eq_ignore_ascii_case(tidied)));
        if field == Some("folder") || names_tidied {
            self.folders.push(path.to_path_buf());
        }
    }
}

/// Reads every file in the library that can name a picture, and the songs outside it that they
/// name or that are open. The error says which file couldn't be read, and why.
fn read_names(library: &Path, outside_songs: &[PathBuf]) -> Result<Names, String> {
    let mut names = Names::default();
    let mut files = Vec::new();
    list_files(library, library, 0, &mut files)?;
    for file in &files {
        read_file(library, file, &mut names)?;
    }
    // Songs outside the library: those named by path (which may name more), and those open.
    let mut read: HashSet<PathBuf> = HashSet::new();
    let songs_folder = library.join("Songs");
    for song in outside_songs {
        if !song.starts_with(library) && song.is_file() && read.insert(song.clone()) {
            read_song(song, &mut names).map_err(|why| format!("{}: {why}", song.display()))?;
        }
    }
    while let Some(key) = names.song_keys.pop() {
        let key = PathBuf::from(key);
        let Some(folder) = key.parent() else { continue };
        if folder == songs_folder {
            continue;
        }
        // A folder that can't be read may hold the song's file (a drive not plugged in, say).
        std::fs::read_dir(folder).map_err(|e| {
            format!("a song the library names is kept in {}, which can't be read now ({e})", folder.display())
        })?;
        for file in song_files_for(&key) {
            if file.is_file() && read.insert(file.clone()) {
                read_song(&file, &mut names).map_err(|why| format!("{}: {why}", file.display()))?;
            }
        }
    }
    Ok(names)
}

/// The files a song named by `key` may be saved in: beside its text file, with either ending
/// added to the key or put in place of its ending (as older files named songs with theirs).
fn song_files_for(key: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for ending in [crate::load::SONG_EXTENSION, crate::load::OLD_SONG_EXTENSION] {
        let mut added = key.as_os_str().to_owned();
        added.push(format!(".{ending}"));
        files.push(PathBuf::from(added));
        files.push(key.with_extension(ending));
    }
    if crate::load::is_song_file(key) {
        files.push(key.to_path_buf());
    }
    files.dedup();
    files
}

/// Every file under `folder` to read for names, leaving out the folders that hold none.
fn list_files(library: &Path, folder: &Path, depth: usize, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && folder != library => return Ok(()),
        Err(e) => return Err(format!("{} could not be read ({e})", shown(library, folder))),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("{} could not be read ({e})", shown(library, folder)))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // Followed through a link, as reading it would be. A link to nothing names nothing; any
        // other file that can't be looked at stops the tidy (it may have been saved over just now).
        let meta = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(_) if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) => continue,
            Err(e) => return Err(format!("{} could not be read ({e})", shown(library, &path))),
        };
        if meta.is_dir() {
            let at_top = folder == library;
            let skipped = at_top
                && (NOT_READ.iter().any(|kept| name.eq_ignore_ascii_case(kept)) || name.starts_with(OPENING_PACK));
            if skipped {
                continue;
            }
            // A folder reached through a link isn't followed (it could lead back here), and one
            // nested deeper than the app ever makes isn't read: either stops the tidy, since a
            // song or theme kept in it would go unread and its pictures would look unused (a
            // Songs folder linked to another place, say).
            let linked = entry.file_type().is_ok_and(|kind| kind.is_symlink());
            if linked {
                return Err(format!("{} is a link to another folder", shown(library, &path)));
            }
            if depth >= DEEPEST {
                return Err(format!("{} is nested too deep to read", shown(library, &path)));
            }
            list_files(library, &path, depth + 1, files)?;
        } else if meta.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn shown(library: &Path, path: &Path) -> String {
    match path.strip_prefix(library) {
        Ok(inside) if inside.as_os_str().is_empty() => "The library folder".to_owned(),
        Ok(inside) => inside.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// What kind of file a file in the library is, for reading it for names.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    /// Half-written by a save that was cut short: read as far as it goes, never stops the tidy.
    Leftover,
    /// One of the app's own (a song, a theme, a settings file): must read whole.
    Own,
    /// The log, a picture, a video: names nothing.
    Skipped,
    /// Anything else: read for ids, if it isn't large.
    Other,
}

fn kind_of(path: &Path) -> Kind {
    let name = file_name(path);
    let ending = path.extension().map(|ext| ext.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ["part", "saving", "ids-part", "renaming"].contains(&ending.as_str()) {
        Kind::Leftover
    } else if OWN_ENDINGS.contains(&ending.as_str()) {
        Kind::Own
    } else if ending == "log" || name == LAST_RUN_FILE || crate::media::is_media(path) {
        Kind::Skipped
    } else {
        Kind::Other
    }
}

/// Reads one of the library's files for names. Fails for one of the app's own files that can't
/// be read whole; a song in `Songs/` or a theme in `Themes/` must also open as one.
fn read_file(library: &Path, path: &Path, names: &mut Names) -> Result<(), String> {
    let failed = |why: String| format!("{} {why}", shown(library, path));
    let meta = std::fs::metadata(path).map_err(|e| failed(format!("could not be read ({e})")))?;
    match kind_of(path) {
        Kind::Skipped => Ok(()),
        Kind::Other if meta.len() > LARGEST_OTHER => Ok(()),
        Kind::Other => {
            let bytes = std::fs::read(path).map_err(|e| failed(format!("could not be read ({e})")))?;
            note_ids(&bytes, &mut names.ids);
            Ok(())
        }
        Kind::Leftover => {
            let bytes = std::fs::read(path).map_err(|e| failed(format!("could not be read ({e})")))?;
            note_ids(&bytes, &mut names.ids);
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                names.note_value(&value, None);
            }
            Ok(())
        }
        Kind::Own => {
            if meta.len() > LARGEST_OWN {
                return Err(failed("is too large to read".to_owned()));
            }
            let in_folder = |folder: &str| path.parent() == Some(library.join(folder).as_path());
            if in_folder("Songs") && crate::load::is_song_file(path) {
                return read_song(path, names).map_err(failed);
            }
            let text = std::fs::read_to_string(path).map_err(|e| failed(format!("could not be read ({e})")))?;
            if in_folder("Themes") {
                sp_model::Theme::from_json(&text).map_err(|why| failed(format!("does not open: {why}")))?;
            }
            note_text(&text, names).map_err(failed)
        }
    }
}

/// Reads a song file for names; it must open as a song.
fn read_song(path: &Path, names: &mut Names) -> Result<(), String> {
    crate::load::read_song_file(path).map_err(|why| format!("does not open as a song: {why}"))?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("could not be read ({e})"))?;
    note_text(&text, names)
}

/// Notes what the text of one of the app's own files names; it must be whole JSON.
fn note_text(text: &str, names: &mut Names) -> Result<(), String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("is damaged or cut short ({e})"))?;
    note_ids(text.as_bytes(), &mut names.ids);
    names.note_value(&value, None);
    Ok(())
}

/// Moves to the Trash what a crash or a quit left behind, more than a day old: song pack folders
/// that were being opened, and files half-written by a save that was cut short, when the file
/// they were to become is there and was saved after them. One that may hold the only copy of
/// something stays, and the log says so.
fn tidy_leftovers(
    library: &Path,
    now: SystemTime,
    trash: &mut dyn FnMut(&Path) -> Result<(), String>,
    outcome: &mut Outcome,
) {
    let mut found: Vec<PathBuf> = Vec::new();
    // Song pack folders, at the top of the library.
    let listed = |folder: &Path| -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> =
            std::fs::read_dir(folder).into_iter().flatten().flatten().map(|entry| entry.path()).collect();
        paths.sort();
        paths
    };
    for path in listed(library) {
        let is_pack = file_name(&path).starts_with(OPENING_PACK)
            && std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir());
        if is_pack && folder_unchanged_for(&path, now, LEFT_FOR) {
            found.push(path);
        }
    }
    // Half-written files: settings at the top, themes, Bibles, and songs.
    for folder in [library.to_path_buf(), library.join("Themes"), library.join("Bibles"), library.join("Songs")] {
        for path in listed(&folder) {
            let Some(target) = becomes(&path) else { continue };
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if !meta.is_file() || !unchanged_for(&meta, now, LEFT_FOR) {
                continue;
            }
            let written = |path: &Path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
            let saved_since = matches!((written(&target), written(&path)), (Some(target), Some(left)) if target > left);
            if target.is_file() && saved_since {
                found.push(path);
            } else {
                outcome.kept.push(format!(
                    "kept {}: a save of {} was cut short, and it may hold the only copy of what was being saved",
                    shown(library, &path),
                    shown(library, &target)
                ));
            }
        }
    }
    for path in found {
        match trash(&path) {
            Ok(()) => outcome.leftovers.push(path),
            Err(e) => {
                outcome.stopped = Some(format!("could not move {} to the Trash ({e})", file_name(&path)));
                return;
            }
        }
    }
}

/// The file a half-written one was to become: `services.json` for `services.json.part`, and
/// `<song>.sppres` for `.<song>.sppres.saving`. `None` for any other file.
fn becomes(path: &Path) -> Option<PathBuf> {
    let name = file_name(path);
    let real = match name.strip_suffix(".saving") {
        Some(saving) => saving.strip_prefix('.')?,
        None => name.strip_suffix(".part")?,
    };
    (!real.is_empty()).then(|| path.with_file_name(real))
}

#[cfg(test)]
mod tests {
    use super::*;

    const USED: &str = "0f5f6b0e-1111-4c4c-8888-000000000001";
    const USED_BY_PATH: &str = "0f5f6b0e-1111-4c4c-8888-000000000002";
    const VIDEO: &str = "0f5f6b0e-1111-4c4c-8888-000000000003";
    const UNUSED: &str = "0f5f6b0e-1111-4c4c-8888-000000000004";
    const RECENT: &str = "0f5f6b0e-1111-4c4c-8888-000000000005";

    /// A library folder of its own for one test, empty, with Images and Media.
    fn library(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-test-sweep-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for inside in ["Images", "Media", "Songs", "Themes"] {
            std::fs::create_dir_all(folder.join(inside)).unwrap();
        }
        folder
    }

    /// A song with a picture box naming `file`, as the app saves it.
    fn song_naming(file: &str) -> String {
        let mut song = sp_model::parse_song_text("Song", "Light on the hills\n");
        song.groups[0].slides[0].objects.push(sp_model::Object::image(file.to_owned(), "Picture".to_owned(), 64, 36));
        sp_model::to_json(&song)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A month from now: every file a test makes is old by then.
    fn month_on() -> SystemTime {
        SystemTime::now() + Duration::from_secs(30 * 24 * 60 * 60)
    }

    /// Tidies `library` at `now`, moving files into a folder beside it instead of the Trash.
    /// Returns what happened and the names of the files moved.
    fn tidy(library: &Path, now: SystemTime) -> (Outcome, Vec<String>) {
        let bin = library.with_extension("trash");
        let _ = std::fs::remove_dir_all(&bin);
        std::fs::create_dir_all(&bin).unwrap();
        let mut moved = Vec::new();
        let mut trash = |path: &Path| -> Result<(), String> {
            let name = file_name(path);
            moved.push(name.clone());
            std::fs::rename(path, bin.join(name)).map_err(|e| e.to_string())
        };
        let outcome = run(library, &[], now, &mut trash);
        moved.sort();
        (outcome, moved)
    }

    // Making a link needs a special right on Windows, so this runs where links are ordinary.
    #[cfg(unix)]
    #[test]
    fn a_folder_linked_from_elsewhere_stops_the_tidy() {
        // A Songs folder that is a link would go unread, and every picture its songs show would
        // look unused: nothing may move.
        let base = std::env::temp_dir().join(format!("sweep-linked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let library = base.join("library");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(library.join("Images")).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, library.join("Songs")).unwrap();
        let mut files = Vec::new();
        let result = list_files(&library, &library, 0, &mut files);
        assert!(result.is_err_and(|why| why.contains("link")), "a linked folder stops it");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_picture_a_song_uses_stays_and_an_old_unused_one_goes_to_the_trash() {
        let library = library("used");
        write(&library.join("Songs/a.sppres"), &song_naming(&format!("{USED}.png")));
        write(&library.join(format!("Images/{USED}.png")), "picture");
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        // A link to nothing in the library names nothing, and doesn't stop the tidy.
        #[cfg(unix)]
        std::os::unix::fs::symlink("/nowhere/gone.json", library.join("gone.json")).unwrap();
        let (outcome, moved) = tidy(&library, month_on());
        assert_eq!(moved, [format!("{UNUSED}.png")], "{outcome:?}");
        assert!(library.join(format!("Images/{USED}.png")).is_file());
        assert!(library.with_extension("trash").join(format!("{UNUSED}.png")).is_file(), "moved, not deleted");
        assert_eq!(outcome.notice().as_deref(), Some("Tidied the library: 1 unused picture (1 KB) went to the Trash."));
        assert_eq!(outcome.log_lines(), ["moved 1 file to the Trash (1 KB of pictures and videos)"]);
        // Each move's own line, logged as it is made.
        assert_eq!(
            moved_line(&library, &library.join(format!("Images/{UNUSED}.png")), Some(2048)),
            format!("moved Images/{UNUSED}.png to the Trash (2 KB); nothing uses it")
        );
        assert_eq!(
            moved_line(&library, &library.join(".opening-pack-1"), None),
            "moved .opening-pack-1 to the Trash; it was left over"
        );
    }

    #[test]
    fn a_bare_name_a_whole_path_and_a_media_name_all_count_whatever_their_case() {
        let library = library("names");
        // A bare name in a song, in capitals; a whole path in the media bin, from where the
        // library used to be; a video named `Media/<name>` in a theme's layout.
        write(&library.join("Songs/a.sppres"), &song_naming(&format!("{}.PNG", USED.to_uppercase())));
        let media = serde_json::json!({
            "entries": [{
                "id": "0f5f6b0e-1111-4c4c-8888-000000000099",
                "path": format!("/somewhere/else/SoftPresenter/Images/{USED_BY_PATH}.jpg")
            }],
            "playlists": []
        });
        write(&library.join("media.json"), &media.to_string());
        let mut theme = sp_model::Theme::builtin();
        theme.id = sp_model::Uuid::new_v4();
        theme.templates[0].objects.push(sp_model::Object::image(
            format!("Media/{VIDEO}.mp4"),
            "Loop".to_owned(),
            64,
            36,
        ));
        write(&library.join(format!("Themes/{}.sptheme", theme.id)), &theme.to_json());
        write(&library.join(format!("Images/{USED}.png")), "picture");
        write(&library.join(format!("Images/{USED_BY_PATH}.jpg")), "picture");
        write(&library.join(format!("Media/{VIDEO}.mp4")), "video");
        write(&library.join(format!("Media/{UNUSED}.mp4")), "video");
        let (outcome, moved) = tidy(&library, month_on());
        assert_eq!(moved, [format!("{UNUSED}.mp4")], "{outcome:?}");
        assert_eq!(outcome.notice().as_deref(), Some("Tidied the library: 1 unused video (1 KB) went to the Trash."));
    }

    #[test]
    fn a_picture_only_a_themes_version_shows_stays() {
        let library = library("version");
        // The giving picture is in the Offering version's template, nowhere else.
        let mut theme = sp_model::Theme::builtin();
        theme.id = sp_model::Uuid::new_v4();
        let offering = sp_model::Version { id: sp_model::Uuid::new_v4(), name: "Offering".to_owned() };
        assert!(theme.add_version(&offering, &[0]));
        let picture = sp_model::Object::image(format!("{USED}.png"), "Give".to_owned(), 64, 64);
        theme.versions[0].templates[0].objects.push(picture);
        write(&library.join(format!("Themes/{}.sptheme", theme.id)), &theme.to_json());
        write(&library.join(format!("Images/{USED}.png")), "picture");
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        let (outcome, moved) = tidy(&library, month_on());
        assert_eq!(moved, [format!("{UNUSED}.png")], "{outcome:?}");
    }

    #[test]
    fn a_file_changed_in_the_last_week_stays() {
        let library = library("recent");
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        write(&library.join(format!("Images/{RECENT}.png")), "picture");
        // Today, both were made just now: neither goes.
        let (outcome, moved) = tidy(&library, SystemTime::now());
        assert!(moved.is_empty() && outcome.notice().is_none(), "{outcome:?}");
        assert!(outcome.log_lines().contains(&"nothing to move".to_owned()));
        // A month on, the one changed since (its modified time three weeks on) is still recent.
        let file = std::fs::File::options().write(true).open(library.join(format!("Images/{RECENT}.png"))).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(25 * 24 * 60 * 60)).unwrap();
        let (_, moved) = tidy(&library, month_on());
        assert_eq!(moved, [format!("{UNUSED}.png")]);
    }

    #[test]
    fn a_song_that_does_not_open_stops_the_whole_tidy() {
        let library = library("unreadable");
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        write(&library.join("services.json.part"), "{\"half");
        write(&library.join("Songs/cut-short.sppres"), "{\"format\": \"softpresenter-song\", \"groups\": [");
        let (outcome, moved) = tidy(&library, month_on());
        assert!(moved.is_empty(), "nothing moves, leftovers included: {moved:?}");
        assert!(outcome.unread && outcome.notice().is_none());
        let log = outcome.log_lines().join("\n");
        let song = Path::new("Songs").join("cut-short.sppres");
        assert!(log.starts_with(&format!("nothing moved: {} does not open as a song", song.display())), "{log}");
        // So does a settings file that isn't whole, anywhere in the library.
        std::fs::remove_file(library.join("Songs/cut-short.sppres")).unwrap();
        write(&library.join("Not saved/Song 2026-11-20.sppres"), "{\"title\": ");
        let (outcome, moved) = tidy(&library, month_on());
        assert!(moved.is_empty() && outcome.unread, "{outcome:?}");
        // And a folder in the library that can't be read.
        std::fs::remove_dir_all(library.join("Not saved")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let shut = library.join("Shut");
            std::fs::create_dir_all(&shut).unwrap();
            std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
            let (outcome, moved) = tidy(&library, month_on());
            std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(moved.is_empty() && outcome.unread, "{outcome:?}");
            assert!(outcome.stopped.unwrap().starts_with("Shut could not be read"));
        }
        // With all of it readable, the tidy goes ahead.
        // The half-saved services file stays: there is no services file it could be older than.
        let (outcome, moved) = tidy(&library, month_on());
        assert_eq!(moved, [format!("{UNUSED}.png")], "{outcome:?}");
        assert!(outcome.kept[0].starts_with("kept services.json.part:"), "{outcome:?}");
    }

    #[test]
    fn only_files_named_as_the_app_names_them_go() {
        let library = library("hand-placed");
        for name in ["sweep-test-kept.png", "Easter.jpg", &format!("{UNUSED}-copy.png"), &format!("{UNUSED}.png.part")]
        {
            write(&library.join("Images").join(name), "picture");
        }
        std::fs::create_dir_all(library.join(format!("Images/{USED}"))).unwrap();
        let (outcome, moved) = tidy(&library, month_on());
        assert!(moved.is_empty(), "{outcome:?}");
    }

    #[test]
    fn a_followed_folder_keeps_all_of_itself() {
        let library = library("followed");
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        let media = serde_json::json!({
            "entries": [],
            "playlists": [{"id": USED, "name": "Lobby", "files": [], "folder": library.join("Images")}]
        });
        write(&library.join("media.json"), &media.to_string());
        let (outcome, moved) = tidy(&library, month_on());
        assert!(moved.is_empty(), "{outcome:?}");
    }

    #[test]
    fn a_song_kept_outside_the_library_counts_and_one_out_of_reach_stops_the_tidy() {
        let library = library("outside");
        let elsewhere = library.with_extension("elsewhere");
        let _ = std::fs::remove_dir_all(&elsewhere);
        // A playlist names a song by its path; its saved file beside it names a picture.
        write(&elsewhere.join("Song.sppres"), &song_naming(&format!("{USED}.png")));
        let services = serde_json::json!({"list": [{"name": "Sunday", "items": [{"key": elsewhere.join("Song"), "title": "Song"}]}]});
        write(&library.join("services.json"), &services.to_string());
        write(&library.join(format!("Images/{USED}.png")), "picture");
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        let (_, moved) = tidy(&library, month_on());
        assert_eq!(moved, [format!("{UNUSED}.png")]);
        // One open this run counts too.
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        write(&elsewhere.join("Open.sppres"), &song_naming(&format!("{UNUSED}.png")));
        let outcome = run(&library, &[elsewhere.join("Open.sppres")], month_on(), &mut |_| Err("moved".to_owned()));
        assert!(outcome.moved.is_empty() && outcome.stopped.is_none(), "{outcome:?}");
        // A song in a folder that can't be reached now (a drive not plugged in) stops it.
        let gone_song = if cfg!(windows) { r"Q:\NotHere\Songs\Song" } else { "/Volumes/NotHere/Songs/Song" };
        let gone = serde_json::json!({"libraries": [{"name": "Stick", "songs": [gone_song]}]});
        write(&library.join("libraries.json"), &gone.to_string());
        let (outcome, moved) = tidy(&library, month_on());
        assert!(moved.is_empty() && outcome.unread, "{outcome:?}");
        let gone_folder = Path::new(gone_song).parent().unwrap().display().to_string();
        assert!(outcome.stopped.unwrap().contains(&format!("{gone_folder}, which can't be read now")));
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    #[test]
    fn a_pack_being_opened_or_a_save_while_it_works_stops_it() {
        let library = library("busy");
        write(&library.join(format!("Images/{UNUSED}.png")), "picture");
        write(&library.join(format!("Images/{RECENT}.png")), "picture");
        // A pack being opened now (a file in it written within the day): nothing moves.
        let unpacked = library.join(".opening-pack-1/p.png");
        write(&unpacked, "picture");
        let file = std::fs::File::options().write(true).open(&unpacked).unwrap();
        file.set_modified(month_on() - DAY / 2).unwrap();
        let (outcome, moved) = tidy(&library, month_on());
        assert!(moved.is_empty(), "{outcome:?}");
        assert_eq!(outcome.stopped.as_deref(), Some("a song pack is being opened"));
        assert_eq!(outcome.log_lines(), ["nothing moved: a song pack is being opened"]);
        std::fs::remove_dir_all(library.join(".opening-pack-1")).unwrap();
        // A song saved after the names were read: it stops after the file it was moving.
        let mut moved = Vec::new();
        let outcome = run(&library, &[], month_on(), &mut |path: &Path| {
            moved.push(file_name(path));
            write(&library.join("Songs/new.sppres"), &song_naming(&format!("{RECENT}.png")));
            Ok(())
        });
        assert_eq!(moved, [format!("{UNUSED}.png")]);
        assert_eq!(outcome.stopped.as_deref(), Some("the library's files changed while it was being tidied"));
    }

    #[test]
    fn leftovers_go_unless_they_may_hold_the_only_copy() {
        let library = library("leftovers");
        let old = |path: &Path| {
            let file = std::fs::File::options().write(true).open(path).unwrap();
            file.set_modified(SystemTime::now() - Duration::from_secs(3 * 24 * 60 * 60)).unwrap();
        };
        // A pack folder left by a quit, and saves cut short whose real file was saved since.
        write(&library.join(".opening-pack-2/abc.png"), "picture");
        write(&library.join("Songs/.a.sppres.saving"), "{\"half");
        old(&library.join("Songs/.a.sppres.saving"));
        write(&library.join("Songs/a.sppres"), &song_naming("nothing.png"));
        write(&library.join("services.json.part"), "{");
        old(&library.join("services.json.part"));
        write(&library.join("services.json"), "{}");
        // A save cut short with no real file, and one newer than its real file: both stay.
        write(&library.join("Songs/.b.sppres.saving"), &song_naming(&format!("{USED}.png")));
        write(&library.join("Themes/x.sptheme"), &sp_model::Theme::builtin().to_json());
        write(&library.join("Themes/x.sptheme.part"), "{");
        write(&library.join(format!("Images/{USED}.png")), "picture");
        let (outcome, moved) = tidy(&library, month_on());
        assert_eq!(moved, [".a.sppres.saving", ".opening-pack-2", "services.json.part"], "{outcome:?}");
        assert!(library.join(format!("Images/{USED}.png")).is_file(), "named by a kept save cut short");
        let log = outcome.log_lines().join("\n");
        let (songs, themes) = (Path::new("Songs"), Path::new("Themes"));
        let kept = format!(
            "kept {}: a save of {} was cut short",
            songs.join(".b.sppres.saving").display(),
            songs.join("b.sppres").display()
        );
        assert!(log.contains(&kept), "{log}");
        assert!(log.contains(&format!("kept {}", themes.join("x.sptheme.part").display())), "{log}");
        assert_eq!(outcome.notice().as_deref(), Some("Tidied the library: 3 leftover files went to the Trash."));
        // A day or less old, they are left alone.
        let library = self::library("leftovers-new");
        std::fs::create_dir_all(library.join(".opening-pack-3")).unwrap();
        write(&library.join("services.json.part"), "{");
        write(&library.join("services.json"), "{}");
        let (outcome, moved) = tidy(&library, SystemTime::now());
        assert!(moved.is_empty(), "{outcome:?}");
    }

    #[test]
    fn the_notice_is_short_and_only_when_something_went() {
        let mut outcome = Outcome::default();
        assert_eq!(outcome.notice(), None);
        outcome.stopped = Some("a song pack is being opened".to_owned());
        assert_eq!(outcome.notice(), None, "nothing moved, nothing said in the window");
        outcome.moved = vec![
            (PathBuf::from("/l/Images/a.png"), 3 * 1024 * 1024),
            (PathBuf::from("/l/Images/b.jpg"), 30 * 1024 * 1024),
            (PathBuf::from("/l/Media/c.mp4"), 1024 * 1024),
        ];
        outcome.leftovers = vec![PathBuf::from("/l/.opening-pack-1")];
        assert_eq!(
            outcome.notice().as_deref(),
            Some("Tidied the library: 3 unused pictures and videos (34 MB) and 1 leftover file went to the Trash.")
        );
        assert_eq!(
            outcome.log_lines(),
            ["stopped: a song pack is being opened", "moved 4 files to the Trash (34 MB of pictures and videos)"]
        );
    }

    #[test]
    fn it_runs_at_most_once_a_day() {
        let library = library("daily");
        let now = SystemTime::now();
        assert!(due(&library, now), "never tidied");
        write(&library.join(LAST_RUN_FILE), &seconds(now - Duration::from_secs(60 * 60)).to_string());
        assert!(!due(&library, now), "an hour ago");
        write(&library.join(LAST_RUN_FILE), &seconds(now - DAY - Duration::from_secs(1)).to_string());
        assert!(due(&library, now), "over a day ago");
        write(&library.join(LAST_RUN_FILE), &seconds(now + DAY * 30).to_string());
        assert!(due(&library, now), "a time to come, as after the clock was put back");
        write(&library.join(LAST_RUN_FILE), "rubbish");
        assert!(due(&library, now));
    }

    #[test]
    fn ids_are_found_in_any_text_and_names_the_app_makes_are_known() {
        let mut ids = HashSet::new();
        let text = format!(
            "{{\"file\":\"Media/{}.mp4\"}} /x/{USED_BY_PATH}.png 1{UNUSED}9 0f5f6b0e-1111-4c4c-8888",
            USED.to_uppercase()
        );
        note_ids(text.as_bytes(), &mut ids);
        let id = |text: &str| id_at(text.as_bytes()).unwrap();
        assert_eq!(ids, HashSet::from([id(USED), id(USED_BY_PATH), id(UNUSED)]));
        assert_eq!(app_named(&format!("{USED}.png")), Some(id(USED)));
        assert_eq!(app_named(USED), Some(id(USED)));
        for name in [format!("{USED}."), format!("{USED}-1.png"), format!("{USED}.png.part"), "abc.png".to_owned()] {
            assert_eq!(app_named(&name), None, "{name}");
        }
        assert_eq!(becomes(Path::new("/l/Songs/.a.sppres.saving")), Some(PathBuf::from("/l/Songs/a.sppres")));
        assert_eq!(becomes(Path::new("/l/media.json.part")), Some(PathBuf::from("/l/media.json")));
        assert_eq!(becomes(Path::new("/l/a.saving")), None);
        assert_eq!(size_words(500), "1 KB");
        assert_eq!(size_words(34 * 1024 * 1024), "34 MB");
    }
}
