//! The fonts slide text is laid out and drawn in, and where their data comes from.
//!
//! The computer's fonts are looked for once, by the first thread that needs them (never the
//! thread that draws the screens): their names and the files each family is in, kept in one
//! collection that every [`Fonts`] starts from. A font file's data is had the first time text is
//! laid out in that font, and kept for every thread, so each file is opened once and only the
//! fonts slides use are opened at all.
//!
//! A font file is never mapped where another program could change it. A mapped file that
//! another program cuts short or writes over while the app runs (a font manager, a system
//! update, installing a font) kills the app with a bus error the next time a letter is shaped
//! or drawn from it, and nothing can catch that. So a font file's data is had in the first of
//! these ways that can be done:
//!
//! - On a Mac, a file on the system volume the computer started from is mapped where it is:
//!   that volume is a read-only snapshot, which nothing can write to while the system runs.
//!   The large fonts the shaper falls back on are there (the emoji font is 188 MB).
//! - A file of [`COPY_FROM`] or more anywhere else is copied, on a Mac or Linux, into a folder
//!   only this user may enter (see [`copies_folder_on`]); the copy is opened, its name removed,
//!   and the copy mapped. On a Mac's own disk the copy is a clone, made at once and taking no
//!   room until the original changes; whatever then happens to the original leaves the copy as
//!   it was.
//! - Anything else (most fonts, all of them on Windows, or when a copy can't be made) is read
//!   whole into memory the app owns.
//!
//! A map costs memory only for the parts of a font that are used, and the system can let those
//! go again; a font read whole costs its whole size until the app quits. The font library was
//! changed for this: `third_party/fontique/PROVENANCE.md`.
//!
//! The thread that draws the screens never opens a font file and never looks for the fonts
//! (see [`draw_kept_only`](crate::draw_kept_only)). A font it doesn't have yet is had by a
//! loader thread of its own, and the screens are told to draw again once it is there.
//! Meanwhile they draw the words in the fonts they do have; with none that suits, the words
//! are left out until the font is there. Measured on a Mac with the files in the disk cache,
//! the first slide with an emoji is drawn in about 1 ms with the emoji font mapped (about 20 ms
//! when it was read whole); ordinary font files (up to 4 MB) are read in 1 to 6 ms, and a
//! private copy of a 20 MB font takes about 1.5 ms as a clone and 12 to 25 ms copied whole from
//! another disk.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

use parley::fontique::{
    Blob, COPY_SUFFIX, Collection, CollectionOptions, FontInfo, SourceCache, SourceId, SourceInfo, SourceKind,
    SourceLoader, map_unchanging,
};
use parley::{FontContext, LayoutContext};
use sp_model::DEFAULT_FONT_FAMILY;

/// SoftPresenter Sans, the default typeface for slides, built into the program so that every
/// computer has it: Regular, Bold, Italic and Bold Italic. It is Libertinus Sans under another
/// name, licensed under the SIL Open Font License 1.1 (`fonts/OFL.txt`).
const BUNDLED: [&[u8]; 4] = [
    include_bytes!("../fonts/SoftPresenterSans-Regular.ttf"),
    include_bytes!("../fonts/SoftPresenterSans-Bold.ttf"),
    include_bytes!("../fonts/SoftPresenterSans-Italic.ttf"),
    include_bytes!("../fonts/SoftPresenterSans-BoldItalic.ttf"),
];

/// SoftPresenter Sans Regular's file, for the operator's window to write the marks for the end
/// of a song in (its own font may lack some of them, and a system's fall-back draws another).
pub fn bundled_regular() -> &'static [u8] {
    BUNDLED[0]
}

/// Adds the bundled fonts to a collection. Their data is the program's own, so nothing is read
/// from a file and nothing can change under it.
fn add_bundled(collection: &mut Collection) {
    for data in BUNDLED {
        collection.register_fonts(Blob::new(Arc::new(data)), None);
    }
}

/// Font lookup and text-layout scratch space. Make one per thread that draws and keep it.
pub struct Fonts {
    pub(crate) font_cx: FontContext,
    pub(crate) layout_cx: LayoutContext<crate::scene::Ink>,
    /// The sizes text was last fitted to its boxes at, newest last: fitting tries size after
    /// size, laying the text out each time, and a slide behind a playing video is drawn again
    /// for every picture of it.
    fitted: Vec<Fitted>,
    /// Whether this has the computer's fonts: one made on the screens' thread before they were
    /// found has none, until they are.
    found: bool,
    /// [`READ_SO_FAR`] when this last caught up with the font files read.
    seen: u64,
    /// This thread's [`REFUSED`] count when this last forgot what it was refused.
    refused: u64,
}

/// A size text was fitted to a box at: the same text, style and box always fit the same way.
struct Fitted {
    text: String,
    spans: Vec<sp_model::TextSpan>,
    style: sp_model::BoxStyle,
    box_size: (f32, f32),
    size: f32,
}

/// How many fitted sizes are remembered (a few screens' worth of boxes, several slides each).
const MOST_FITTED: usize = 64;

/// What a thread that never reads a font file itself has called once something it was refused
/// is there, so that it draws again.
pub(crate) type Loaded = Arc<dyn Fn() + Send + Sync>;

/// The computer's fonts: their names, and the files each family is in. Looked for once; every
/// [`Fonts`] starts from a copy, which shares what was found.
static FOUND: OnceLock<Collection> = OnceLock::new();
/// Every font file read so far, for every thread.
static FILES: OnceLock<Mutex<Files>> = OnceLock::new();
/// Goes up each time a font file has been read, or the fonts have been found, so that each
/// [`Fonts`] catches up.
static READ_SO_FAR: AtomicU64 = AtomicU64::new(0);
/// Work for the loader thread.
static JOBS: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();

#[derive(Default)]
struct Files {
    /// Each file's data; `None` for one that can't be read or isn't a font (as if missing).
    read: HashMap<PathBuf, Option<Blob<u8>>>,
    /// Files the loader thread has been asked for and hasn't read yet.
    reading: HashSet<PathBuf>,
}

/// What the loader thread is asked to do, and whom to tell when it has.
enum Job {
    Find(Loaded),
    Read(PathBuf, Loaded),
}

thread_local! {
    /// On a thread that never reads a font file itself (the screens'), what to call once a font
    /// it was refused is there.
    static ELSEWHERE: RefCell<Option<Loaded>> = const { RefCell::new(None) };
    /// Whether a font file has been asked for during the layout under way. Only the first one
    /// refused is: the shaper goes on to fonts further down its list (fallbacks for other
    /// alphabets, then any font at all), and those are read only if they turn out to be needed
    /// once the first is there.
    static ASKED: Cell<bool> = const { Cell::new(false) };
    /// How many times this thread has been refused a font file.
    static REFUSED: Cell<u64> = const { Cell::new(0) };
    /// Whether this thread has asked the loader thread to look for the fonts.
    static ASKED_TO_FIND: Cell<bool> = const { Cell::new(false) };
}

/// Says this thread never reads a font file itself; `loaded` is called once a font it was
/// refused is there. See [`draw_kept_only`](crate::draw_kept_only).
pub(crate) fn read_fonts_elsewhere(loaded: Loaded) {
    ELSEWHERE.with_borrow_mut(|elsewhere| *elsewhere = Some(loaded));
}

/// What to call once something is there, if this thread never reads a font file itself.
fn elsewhere() -> Option<Loaded> {
    ELSEWHERE.with_borrow(Clone::clone)
}

/// The computer's fonts, looked for now if no thread has yet. Not on the screens' thread.
fn found() -> &'static Collection {
    FOUND.get_or_init(|| {
        let mut collection = Collection::new(CollectionOptions { shared: false, system_fonts: true });
        add_bundled(&mut collection);
        READ_SO_FAR.fetch_add(1, Ordering::AcqRel);
        collection
    })
}

/// A font file's data, mapped where nothing can change it, or read into memory of its own.
/// `None` if it can't be had, or what is there isn't a font (cut short or written over before it
/// was first opened): the font is then drawn as a missing one is.
fn read_font_file(path: &Path) -> Option<Blob<u8>> {
    let big = std::fs::metadata(path).is_ok_and(|meta| meta.len() >= copy_from());
    let mapped = map_unchanging(path, if big { copies_folder() } else { None });
    #[cfg(test)]
    tests::note_how(path, mapped.as_ref().map(|(_, from)| *from));
    let blob = match mapped {
        Some((blob, _)) => blob,
        None => Blob::new(Arc::new(std::fs::read(path).ok()?)),
    };
    // What is checked is what is handed out: the map, or the bytes read.
    FontInfo::from_source(SourceInfo::new(SourceId::new(), SourceKind::Memory(blob.clone())), 0)?;
    Some(blob)
}

/// Font files at least this big that aren't on a Mac's system volume are mapped from a private
/// copy rather than read whole. Most fonts are smaller and are read: a song in three families
/// and a Bible template come to about 7 MB, and copying them would only add a file to write for
/// each. Large ones (fonts for Chinese or Japanese, emoji) are what reading made costly.
pub(crate) const COPY_FROM: u64 = 4 * 1024 * 1024;

fn copy_from() -> u64 {
    // In a test's own process smaller files may be copied, so that copies are tested on fonts
    // every computer has.
    #[cfg(test)]
    if let Some(bytes) = std::env::var("SP_TEST_COPY_FROM").ok().and_then(|bytes| bytes.parse().ok()) {
        return bytes;
    }
    COPY_FROM
}

/// Whether the app has said private copies of font files may be made in its caches folder.
static COPIES_IN_CACHES: AtomicBool = AtomicBool::new(false);

/// Lets large font files be mapped from private copies made in the app's caches folder (see
/// [`copies_folder_on`]). The app says so once, as it starts. Without it (tests, tools), the
/// only copies made are in a folder `SOFTPRESENTER_FONT_COPIES` names, and other large fonts
/// not on a Mac's system volume are read whole.
pub fn copy_large_fonts_into_caches() {
    COPIES_IN_CACHES.store(true, Ordering::Release);
}

/// The folder private copies of font files are made in, made and kept to this user the first
/// time a copy is wanted, with any copies left behind cleared; `None` where there is none or it
/// can't be kept private (fonts are then read whole).
fn copies_folder() -> Option<&'static Path> {
    static COPIES: OnceLock<Option<PathBuf>> = OnceLock::new();
    COPIES
        .get_or_init(|| {
            let in_caches = COPIES_IN_CACHES.load(Ordering::Acquire);
            let var = |name: &str| {
                // Tests and tools make copies only where they say, never in the user's folders.
                if !in_caches && name != "SOFTPRESENTER_FONT_COPIES" {
                    return None;
                }
                std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
            };
            let folder = copies_folder_on(std::env::consts::OS, var)?;
            keep_private(&folder).ok()?;
            clear_left_copies(&folder);
            Some(folder)
        })
        .as_deref()
}

/// Where private copies of font files are made on a system (`macos`, `windows`, or another),
/// given the environment's variables; never in the library in Documents, which may be synced.
/// On a Mac, `font copies` in the user's Library/Caches/SoftPresenter, beside the control
/// socket's folder; on Linux, in the user's caches folder (`$XDG_CACHE_HOME`, or `.cache` in
/// the home folder); for a test copy with a data folder of its own (`SOFTPRESENTER_DATA`), in
/// that folder. None on Windows, where font files are read whole. A copy has a name there only
/// for a moment, so the folder is empty almost all the time.
fn copies_folder_on(system: &str, var: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    // Testing aid: SOFTPRESENTER_FONT_COPIES=<folder> has the private copies of font files made
    // in that folder.
    if let Some(folder) = var("SOFTPRESENTER_FONT_COPIES") {
        return Some(folder);
    }
    if system == "windows" {
        return None;
    }
    if let Some(data) = var("SOFTPRESENTER_DATA") {
        return Some(data.join("font copies"));
    }
    if system == "macos" {
        return Some(var("HOME")?.join("Library/Caches/SoftPresenter/font copies"));
    }
    // A caches folder given relative to wherever the app was started from is not used.
    let caches = var("XDG_CACHE_HOME")
        .filter(|caches| caches.has_root())
        .or_else(|| var("HOME").map(|home| home.join(".cache")))?;
    Some(caches.join("SoftPresenter").join("font copies"))
}

/// Makes the folder if it isn't there, and lets only this user in, as the control socket's
/// folder is kept.
#[cfg(unix)]
fn keep_private(folder: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(folder)?;
    // A real folder, not a link to somewhere else; and setting its permissions fails unless
    // this user owns it.
    if !std::fs::symlink_metadata(folder)?.is_dir() {
        return Err(std::io::Error::other("not a folder"));
    }
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn keep_private(_folder: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("no private copies of font files on this system"))
}

/// Removes copies left in the folder by a run of the app that ended in the moment between making
/// one and removing its name; only plain files named as copies are touched. One that another
/// copy of the app running now is about to open goes too, and that copy reads the font whole
/// instead.
fn clear_left_copies(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let plain = std::fs::symlink_metadata(entry.path()).is_ok_and(|meta| meta.is_file());
        if plain && entry.file_name().to_string_lossy().ends_with(COPY_SUFFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The data of a font file, for the shaper and the renderer: kept from the first time it was
/// read, or read now. On a thread that never reads (the screens'), one not read yet is asked of
/// the loader thread and refused for now.
fn font_file(path: &Path) -> Option<Blob<u8>> {
    {
        let mut files = files();
        if let Some(read) = files.read.get(path) {
            return read.clone();
        }
        if let Some(loaded) = elsewhere() {
            REFUSED.set(REFUSED.get() + 1);
            if !ASKED.replace(true)
                && files.reading.insert(path.to_owned())
                && !send(Job::Read(path.to_owned(), loaded))
            {
                files.reading.remove(path);
            }
            return None;
        }
    }
    // Read without holding the lock, so a screen's thread isn't kept waiting.
    let read = read_font_file(path);
    // Another thread may have read it meanwhile: its copy is kept, so all draw from one.
    let kept = files().read.entry(path.to_owned()).or_insert(read).clone();
    READ_SO_FAR.fetch_add(1, Ordering::AcqRel);
    kept
}

/// The font files read so far. A thread that panicked while it held them left them whole (each
/// change is a single insert or remove), so they are used as they are.
fn files() -> std::sync::MutexGuard<'static, Files> {
    crate::pictures::timed_lock(FILES.get_or_init(Mutex::default)).unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Hands the loader thread a job, starting it the first time. False if it couldn't be.
fn send(job: Job) -> bool {
    let jobs = JOBS.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        let started = std::thread::Builder::new().name("fonts".to_owned()).spawn(move || {
            for job in rx {
                match job {
                    Job::Find(loaded) => {
                        found();
                        loaded();
                    }
                    Job::Read(path, loaded) => {
                        let read = read_font_file(&path);
                        let mut files = files();
                        files.reading.remove(&path);
                        files.read.entry(path).or_insert(read);
                        drop(files);
                        READ_SO_FAR.fetch_add(1, Ordering::AcqRel);
                        loaded();
                    }
                }
            }
        });
        if let Err(e) = started {
            eprintln!("could not start the font loader: {e}");
        }
        Mutex::new(tx)
    });
    jobs.lock().is_ok_and(|tx| tx.send(job).is_ok())
}

/// A collection of the bundled fonts alone.
fn bundled_only() -> Collection {
    let mut collection = Collection::new(CollectionOptions { shared: false, system_fonts: false });
    add_bundled(&mut collection);
    collection
}

impl Fonts {
    /// Uses the fonts installed on this computer. The first one made looks for them, which
    /// takes a moment; on the screens' thread the loader thread does that instead.
    pub fn new() -> Fonts {
        let collection = if elsewhere().is_some() { FOUND.get().cloned() } else { Some(found().clone()) };
        let found = collection.is_some();
        let collection = collection.unwrap_or_else(bundled_only);
        Fonts::with(collection, found)
    }

    /// Only the bundled fonts, none of the computer's: text in SoftPresenter Sans is laid out
    /// and drawn the same on every computer. For pictures that are compared with stored ones.
    pub fn bundled_only() -> Fonts {
        Fonts::with(bundled_only(), true)
    }

    fn with(collection: Collection, found: bool) -> Fonts {
        let mut source_cache = SourceCache::default();
        let loader: Arc<SourceLoader> = Arc::new(font_file);
        source_cache.set_loader(Some(loader));
        Fonts {
            font_cx: FontContext { collection, source_cache },
            layout_cx: LayoutContext::new(),
            fitted: Vec::new(),
            found,
            seen: READ_SO_FAR.load(Ordering::Acquire),
            refused: REFUSED.get(),
        }
    }

    /// Before text is laid out: takes up the computer's fonts once they are found, and once a
    /// font file this thread was refused is there, forgets the refusal and the sizes text was
    /// fitted at without it.
    fn catch_up(&mut self) {
        if !self.found {
            if let Some(collection) = FOUND.get() {
                self.font_cx.collection = collection.clone();
                self.found = true;
                self.fitted.clear();
            } else if let Some(loaded) = elsewhere()
                && !ASKED_TO_FIND.replace(true)
                && !send(Job::Find(loaded))
            {
                ASKED_TO_FIND.set(false);
            }
        }
        let now = READ_SO_FAR.load(Ordering::Acquire);
        if now != self.seen {
            self.seen = now;
            let refused = REFUSED.get();
            if refused != self.refused {
                self.refused = refused;
                self.fitted.clear();
                // Forgets only the files remembered as failed, so they are asked for again.
                self.font_cx.source_cache.prune(u64::MAX, true);
            }
        }
    }

    /// Called as a layout starts: catches up, and lets this layout ask for a font file.
    pub(crate) fn starting_layout(&mut self) {
        self.catch_up();
        ASKED.set(false);
    }

    /// The size this text was fitted to a box of this size at before, in this style.
    pub(crate) fn fitted_size(
        &mut self,
        text: &str,
        spans: &[sp_model::TextSpan],
        style: &sp_model::BoxStyle,
        box_size: (f32, f32),
    ) -> Option<f32> {
        self.catch_up();
        let at = self
            .fitted
            .iter()
            .rposition(|f| f.box_size == box_size && f.text == text && f.spans == spans && f.style == *style)?;
        // Used again: moved to the end, so the ones not used for longest are let go first.
        let found = self.fitted.remove(at);
        let size = found.size;
        self.fitted.push(found);
        Some(size)
    }

    /// Remembers the size text was fitted to a box at.
    pub(crate) fn remember_fit(
        &mut self,
        text: &str,
        spans: &[sp_model::TextSpan],
        style: &sp_model::BoxStyle,
        box_size: (f32, f32),
        size: f32,
    ) {
        if self.fitted.len() >= MOST_FITTED {
            self.fitted.remove(0);
        }
        self.fitted.push(Fitted { text: text.to_owned(), spans: spans.to_vec(), style: style.clone(), box_size, size });
    }
}

impl Fonts {
    /// The names of the font families installed on this computer, sorted, with the default
    /// typeface first.
    pub fn family_names(&mut self) -> Vec<String> {
        // A name beginning with a dot is one of the system's own private faces (a Mac lists
        // ".Aqua Kana", ".Apple Color Emoji UI"…), not for choosing.
        let mut names: Vec<String> =
            self.font_cx.collection.family_names().filter(|name| !name.starts_with('.')).map(str::to_owned).collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        if let Some(at) = names.iter().position(|name| name == DEFAULT_FONT_FAMILY) {
            let default = names.remove(at);
            names.insert(0, default);
        }
        names
    }
}

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts::new()
    }
}

#[cfg(test)]
impl Fonts {
    /// Only the fonts in these files and folders, and none of the computer's own, read as the
    /// computer's are.
    pub(crate) fn only_from(paths: &[&Path]) -> Fonts {
        let mut collection = Collection::new(CollectionOptions { shared: false, system_fonts: false });
        collection.load_fonts_from_paths(paths);
        Fonts::with(collection, true)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use parley::PositionedLayoutItem;
    use parley::fontique::{FallbackKey, MappedFrom, Script};
    use sp_model::{Object, Role, Slide, Theme};

    use super::*;
    use crate::scene::{build_scene, lay_out};

    /// How each font file was had in this process: mapped (from where), or read whole (`None`).
    static HOW: Mutex<Option<HashMap<PathBuf, Option<MappedFrom>>>> = Mutex::new(None);

    /// Notes how a font file was had, for the tests to look at.
    pub(super) fn note_how(path: &Path, how: Option<MappedFrom>) {
        if let Ok(mut all) = HOW.lock() {
            all.get_or_insert_with(HashMap::new).insert(path.to_owned(), how);
        }
    }

    /// How a font file was had: `None` if it never was, `Some(None)` if it was read whole.
    fn how(path: &Path) -> Option<Option<MappedFrom>> {
        HOW.lock().ok()?.as_ref()?.get(path).copied()
    }

    /// SoftPresenter Sans is in every collection, with all four of its faces, and comes first in
    /// the list of families, whatever fonts the computer has.
    #[test]
    fn the_bundled_typeface_is_always_there() {
        let mut only = Fonts::bundled_only();
        assert_eq!(only.family_names(), [DEFAULT_FONT_FAMILY]);
        let family = only.font_cx.collection.family_by_name(DEFAULT_FONT_FAMILY).expect("the family");
        assert_eq!(family.fonts().len(), 4);
        // Before the computer's fonts are found, as on the screens' thread at the start.
        let mut first = Fonts::with(bundled_only(), false);
        assert_eq!(first.family_names()[0], DEFAULT_FONT_FAMILY);
        // Letters are laid out in it, not drawn as missing ones.
        let letters = glyphs(&mut only, DEFAULT_FONT_FAMILY, "Свет ґ ї Light");
        assert!(letters.iter().all(|glyph| glyph.0 != 0));
    }

    /// Each of the seven marks for the end of a song is in SoftPresenter Sans itself, in all four
    /// of its faces, so a theme set in it draws the same mark on every computer: none is left to
    /// whatever font a system falls back to (the small square once was: Libertinus Sans has only
    /// the large one, so the family's build adds it).
    #[test]
    fn the_bundled_typeface_has_every_mark_for_the_end_of_a_song() {
        let mut only = Fonts::bundled_only();
        for (weight, italic) in [(400, false), (700, false), (400, true), (700, true)] {
            let mut style = Theme::builtin().templates[0].boxes[0].clone();
            style.font_family = DEFAULT_FONT_FAMILY.to_owned();
            (style.weight, style.italic) = (weight, italic);
            let mut widths = Vec::new();
            for mark in sp_model::EndMark::ALL {
                let layout = lay_out(&mut only, &mark.text(), &[], &style, 60.0, 10_000.0);
                let mut ids = Vec::new();
                for line in layout.lines() {
                    for item in line.items() {
                        if let PositionedLayoutItem::GlyphRun(run) = item {
                            ids.extend(run.positioned_glyphs().map(|glyph| glyph.id));
                        }
                    }
                }
                assert_eq!(ids.len(), 11, "{mark:?}: three marks and eight spaces, on one line");
                assert!(ids.iter().all(|id| *id != 0), "{mark:?} at {weight}, italic {italic}: {ids:?}");
                assert!(ids[0] == ids[5] && ids[5] == ids[10] && ids[0] != ids[1], "{mark:?}: {ids:?}");
                widths.push((mark, ids[0], layout.width()));
            }
            // Seven marks of their own: the small square is not the large one, nor the dot the
            // bullet, and each small one is narrower than its large one.
            let mut own: Vec<u32> = widths.iter().map(|(_, id, _)| *id).collect();
            own.sort_unstable();
            own.dedup();
            assert_eq!(own.len(), 7, "{widths:?}");
            let wide = |mark: sp_model::EndMark| widths.iter().find(|(have, _, _)| *have == mark).expect("laid out").2;
            assert!(wide(sp_model::EndMark::SmallSquare) < wide(sp_model::EndMark::Square), "{widths:?}");
            assert!(wide(sp_model::EndMark::Bullet) < wide(sp_model::EndMark::Dot), "{widths:?}");
        }
    }

    /// Fonts this computer has, with pair kerning, to copy and spoil. Empty on a computer with
    /// none of them.
    fn fonts_to_copy() -> Vec<PathBuf> {
        [
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
            "C:\\Windows\\Fonts\\times.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSerif.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect()
    }

    /// A folder of its own for a test, beside the test program, with a copy of `font` in it.
    fn copy_into(name: &str, font: &Path) -> PathBuf {
        let program = std::env::current_exe().expect("this test program");
        let folder = program.parent().expect("its folder").join(format!("font-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder for the copy");
        std::fs::copy(font, folder.join(font.file_name().expect("a name"))).expect("a copy");
        folder
    }

    /// The one font file in a folder.
    fn the_file_in(folder: &Path) -> PathBuf {
        std::fs::read_dir(folder).expect("the folder").flatten().next().expect("the copy").path()
    }

    /// The family of the fonts in a folder (only the folder is looked at; nothing is read).
    fn family_in(folder: &Path) -> String {
        Fonts::only_from(&[folder]).family_names().into_iter().next().expect("a family in the copy")
    }

    /// Whether a file has been read for the shaper, or is being read.
    fn read_or_reading(path: &Path) -> bool {
        let files = files();
        files.read.get(path).is_some_and(Option::is_some) || files.reading.contains(path)
    }

    /// Every glyph of a text laid out in a family, with its place.
    fn glyphs(fonts: &mut Fonts, family: &str, text: &str) -> Vec<(u32, f32, f32)> {
        let mut style = Theme::builtin().templates[0].boxes[0].clone();
        style.font_family = family.to_owned();
        let layout = lay_out(fonts, text, &[], &style, 60.0, 10_000.0);
        let mut found = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    found.extend(run.positioned_glyphs().map(|glyph| (glyph.id, glyph.x, glyph.y)));
                }
            }
        }
        found
    }

    /// A slide of a text in a family as the GPU would be handed it: the scene, with every
    /// letter's outline read from the font, which the renderer does as it draws.
    fn drawn(fonts: &mut Fonts, family: &str, text: &str) -> Vec<u8> {
        let mut theme = Theme::builtin();
        for template in &mut theme.templates {
            for style in &mut template.boxes {
                style.font_family = family.to_owned();
            }
        }
        let slide = Slide::new(vec![Object::text(Role::primary(), text)]);
        let built = build_scene(fonts, Some(&slide), &theme, 640, 360);
        let mut packed = Vec::new();
        let _ = vello::low_level::vello_encoding::Resolver::new().resolve(built.scene.encoding(), &mut packed);
        packed
    }

    /// What is laid out and drawn before the file changes.
    const BEFORE: &str = "mm";
    /// Letters not laid out before the file changes, in pairs a font kerns.
    const AFTER: &str = "AVATAR Type WAVE To. LTV 0123456789 fjord";

    /// Changes a font file in place, as another program might.
    fn spoil(file: &Path, how: &str) {
        match how {
            "cut" => {
                let open = std::fs::OpenOptions::new().write(true).open(file).expect("the copy opens");
                open.set_len(4096).expect("the copy is cut short");
            }
            "zeroed" => std::fs::write(file, vec![0u8; 20_000]).expect("the copy is written over"),
            "replaced" => {
                let other = std::env::var_os("SP_TEST_FONT_OTHER").expect("another font");
                let bytes = std::fs::read(other).expect("the other font");
                std::fs::write(file, bytes).expect("the copy is written over with another font");
            }
            "gone" => std::fs::remove_file(file).expect("the copy is taken away"),
            other => panic!("no such change: {other}"),
        }
    }

    /// After the font in `file` was first had: checks it was had the way this run is about (from
    /// a private copy, which left no name behind, or read whole), and says which.
    fn check_how_it_was_had(file: &Path, copied: bool) -> &'static str {
        let had = how(file).expect("the font file was had");
        if !copied {
            assert_eq!(had, None, "a font file anybody may change was mapped");
            return "read whole";
        }
        let copies = std::env::var_os("SOFTPRESENTER_FONT_COPIES").map(PathBuf::from).expect("a folder for copies");
        // (Never made where there are no copies.)
        let left: Vec<_> = std::fs::read_dir(&copies).map(|found| found.flatten().collect()).unwrap_or_default();
        assert!(left.is_empty(), "a copy was left with a name: {left:?}");
        match had {
            Some(from) => {
                assert_eq!(from, MappedFrom::PrivateCopy);
                "mapped from a private copy"
            }
            // Only on a Mac is a copy certain (Windows makes none, and a Linux disk may be
            // one copies aren't kept on); there the test would have tested nothing.
            None if cfg!(target_os = "macos") => panic!("the font was read whole, not mapped from a private copy"),
            None => "read whole (NOTHING LEARNT about private copies here)",
        }
    }

    /// Run by the test below in a process of its own, because where this goes wrong the
    /// process is killed, and that must not be the one running the tests.
    #[test]
    fn font_file_changes_helper() {
        let Some(folder) = std::env::var_os("SP_TEST_FONT_FOLDER").map(PathBuf::from) else { return };
        let how = std::env::var("SP_TEST_FONT_CHANGE").unwrap_or_default();
        let (how, copied) = match how.strip_prefix("copy-") {
            Some(how) => (how.to_owned(), true),
            None => (how, false),
        };
        let spoilt = folder.join("spoilt");
        let file = the_file_in(&spoilt);
        let family = family_in(&spoilt);
        if let Some(how) = how.strip_prefix("before-first-use-") {
            // Spoilt after the fonts were looked for, before anything was laid out in it:
            // drawn as a family the computer doesn't have is.
            let mut fonts = Fonts::new();
            fonts.font_cx.collection.load_fonts_from_paths([&spoilt]);
            spoil(&file, how);
            let missing = "A Family Nobody Has";
            let laid_out = glyphs(&mut fonts, &family, AFTER);
            let had = check_how_it_was_had(&file, copied);
            assert_eq!(laid_out, glyphs(&mut fonts, missing, AFTER));
            assert!(drawn(&mut fonts, &family, AFTER) == drawn(&mut fonts, missing, AFTER));
            let letters = laid_out.iter().filter(|glyph| glyph.0 != 0).count();
            println!(
                "helper: drawn as a missing font ({letters} letters, {had}) after the file was {how} before its first use"
            );
            return;
        }
        let mut fonts = Fonts::only_from(&[&spoilt]);
        assert!(!glyphs(&mut fonts, &family, BEFORE).is_empty());
        let _ = drawn(&mut fonts, &family, BEFORE);
        let had = check_how_it_was_had(&file, copied);
        spoil(&file, &how);
        let after = glyphs(&mut fonts, &family, AFTER);
        let after_drawn = drawn(&mut fonts, &family, AFTER);
        let mut whole = Fonts::only_from(&[&folder.join("whole")]);
        assert!(after.len() >= AFTER.chars().filter(|c| !c.is_whitespace()).count());
        assert!(after.iter().all(|glyph| glyph.0 != 0), "a letter was laid out as the no-such-letter box");
        assert_eq!(after, glyphs(&mut whole, &family, AFTER), "laid out differently once the file changed");
        assert!(after_drawn == drawn(&mut whole, &family, AFTER), "drawn differently once the file changed");
        println!("helper: the same ({had}) after the file was {how}");
    }

    /// A font file that another program cuts short, writes over or takes away while text is
    /// drawn in it changes nothing: the text is laid out and drawn as it would have been,
    /// whether the font was read whole or is mapped from a private copy (the `copy-` ways, where
    /// even an ordinary font is copied). One spoilt before its first use is drawn as a missing
    /// font is. (With the published font library, which maps the font file itself, cutting the
    /// file short or writing over it kills the helper process with a bus error.)
    #[test]
    fn text_is_laid_out_and_drawn_the_same_after_its_font_file_changes() {
        let fonts = fonts_to_copy();
        let Some(font) = fonts.first() else {
            eprintln!("NOTHING LEARNT: no font to copy on this computer");
            return;
        };
        let other = fonts.get(1).unwrap_or(font);
        let program = std::env::current_exe().expect("this test program");
        let ways = ["cut", "zeroed", "replaced", "gone", "before-first-use-cut", "before-first-use-zeroed"];
        let copied = ways.map(|how| format!("copy-{how}"));
        for how in ways.iter().copied().chain(copied.iter().map(String::as_str)) {
            let folder = program.parent().expect("its folder").join(format!("font-test-{}-{how}", std::process::id()));
            for part in ["spoilt", "whole"] {
                std::fs::create_dir_all(folder.join(part)).expect("a folder for the copies");
                std::fs::copy(font, folder.join(part).join(font.file_name().expect("a name"))).expect("a copy");
            }
            let mut helper = std::process::Command::new(&program);
            helper
                .args(["fonts::tests::font_file_changes_helper", "--exact", "--nocapture", "--test-threads=1"])
                .env("SP_TEST_FONT_FOLDER", &folder)
                .env("SP_TEST_FONT_CHANGE", how)
                .env("SP_TEST_FONT_OTHER", other)
                .env_remove("SOFTPRESENTER_FONT_COPIES")
                .env_remove("SP_TEST_COPY_FROM");
            if how.starts_with("copy-") {
                helper.env("SOFTPRESENTER_FONT_COPIES", folder.join("copies")).env("SP_TEST_COPY_FROM", "0");
            }
            let ran = helper.output().expect("the helper ran");
            let _ = std::fs::remove_dir_all(&folder);
            let said = format!("{}\n{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr));
            assert!(
                ran.status.success() && said.contains("helper: "),
                "after the font file was {how}, the helper ended with {:?}:\n{said}",
                ran.status
            );
        }
    }

    /// Only a font file nothing else can change is mapped: on a Mac, one on the system volume,
    /// where it is; a file anybody may write to only from a private copy, which keeps no name.
    /// (No system file is written to here.)
    #[test]
    fn only_a_font_file_nothing_else_can_change_is_mapped() {
        let Some(font) = fonts_to_copy().into_iter().next() else { return };
        let folder = copy_into("mapped", &font);
        let file = the_file_in(&folder);
        let copies = folder.join("copies");
        std::fs::create_dir(&copies).expect("a folder for copies");
        let in_place = map_unchanging(&file, None);
        let copied = map_unchanging(&file, Some(&copies));
        let left = std::fs::read_dir(&copies).expect("the folder for copies").count();
        let whole = std::fs::read(&file).expect("the font");
        // A Mac's copy takes on a lock put on the file (in the Finder, or with chflags), and
        // could then never lose its name: a locked file is not copied at all.
        let chflags = |how: &str| std::process::Command::new("/usr/bin/chflags").arg(how).arg(&file).status();
        let locked = cfg!(target_os = "macos").then(|| {
            chflags("uchg").expect("locked");
            let mapped = map_unchanging(&file, Some(&copies)).is_some();
            chflags("nouchg").expect("unlocked");
            (mapped, std::fs::read_dir(&copies).expect("the folder for copies").count())
        });
        let _ = std::fs::remove_dir_all(&folder);
        assert!(in_place.is_none(), "a font file anybody may change was mapped where it is");
        assert_eq!(left, 0, "a copy was left with a name");
        match copied {
            Some((blob, from)) => {
                assert_eq!(from, MappedFrom::PrivateCopy);
                assert!(blob.data() == whole.as_slice(), "the copy's data is not the font's");
            }
            None if cfg!(target_os = "macos") => panic!("no private copy was mapped"),
            None => eprintln!("NOTHING LEARNT: no private copy is mapped on this computer"),
        }
        assert_eq!(locked.unwrap_or_default(), (false, 0), "a locked font file was copied, or its copy left");
        if cfg!(target_os = "macos") {
            for system in ["/System/Library/Fonts/Supplemental/Arial.ttf", "/System/Library/Fonts/Helvetica.ttc"] {
                let system = Path::new(system);
                if !system.is_file() {
                    continue;
                }
                let mapped = map_unchanging(system, None).map(|(_, from)| from);
                if root_is_a_sealed_snapshot() {
                    assert_eq!(mapped, Some(MappedFrom::SystemVolume), "{system:?}");
                } else {
                    eprintln!("NOTHING LEARNT: this Mac's system volume isn't a sealed snapshot ({mapped:?})");
                }
            }
        }
    }

    /// Whether `mount` says the system volume this Mac started from is sealed and read-only, as
    /// it is on macOS 11 and later unless its protection was turned off.
    fn root_is_a_sealed_snapshot() -> bool {
        let Ok(said) = std::process::Command::new("/sbin/mount").output() else { return false };
        String::from_utf8_lossy(&said.stdout)
            .lines()
            .any(|line| line.contains(" on / (") && line.contains("sealed") && line.contains("read-only"))
    }

    #[test]
    fn copies_of_font_files_are_made_in_the_caches_never_in_documents() {
        let vars = |set: &[(&str, &str)]| {
            let set: Vec<(String, PathBuf)> =
                set.iter().map(|(var, value)| ((*var).to_owned(), PathBuf::from(value))).collect();
            move |name: &str| set.iter().find(|(var, _)| var == name).map(|(_, value)| value.clone())
        };
        let home = ("HOME", "/home/someone");
        let data = ("SOFTPRESENTER_DATA", "/tests/data");
        let aid = ("SOFTPRESENTER_FONT_COPIES", "/tests/copies");
        let ends = |found: Option<PathBuf>, parts: &[&str]| {
            let found = found.expect("a folder");
            let tail: Vec<String> = found
                .components()
                .rev()
                .take(parts.len())
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            let tail: Vec<&str> = tail.iter().rev().map(String::as_str).collect();
            assert_eq!(tail, parts, "{found:?}");
        };
        ends(copies_folder_on("macos", vars(&[home])), &["Library", "Caches", "SoftPresenter", "font copies"]);
        ends(copies_folder_on("linux", vars(&[home])), &["someone", ".cache", "SoftPresenter", "font copies"]);
        ends(
            copies_folder_on("linux", vars(&[home, ("XDG_CACHE_HOME", "/elsewhere/caches")])),
            &["elsewhere", "caches", "SoftPresenter", "font copies"],
        );
        // A caches folder named relative to nowhere in particular is not used.
        ends(
            copies_folder_on("linux", vars(&[home, ("XDG_CACHE_HOME", "caches")])),
            &["someone", ".cache", "SoftPresenter", "font copies"],
        );
        // A test copy keeps them in its own data folder; the testing aid wins over everything.
        ends(copies_folder_on("macos", vars(&[home, data])), &["data", "font copies"]);
        ends(copies_folder_on("macos", vars(&[home, data, aid])), &["tests", "copies"]);
        // None on Windows, where font files are read whole, and none without a home.
        assert_eq!(copies_folder_on("windows", vars(&[home, data])), None);
        assert_eq!(copies_folder_on("macos", vars(&[])), None);
    }

    /// The folder is made for this user alone, and copies left in it by a run that ended in
    /// the moment one had a name are cleared, but nothing else.
    #[cfg(unix)]
    #[test]
    fn the_folder_for_copies_is_private_and_left_copies_are_cleared() {
        use std::os::unix::fs::PermissionsExt;
        let program = std::env::current_exe().expect("this test program");
        let base = program.parent().expect("its folder").join(format!("font-test-{}-private", std::process::id()));
        let folder = base.join("caches").join("font copies");
        keep_private(&folder).expect("made");
        let mode = |path: &Path| std::fs::metadata(path).expect("there").permissions().mode() & 0o777;
        assert_eq!(mode(&folder), 0o700);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("opened up");
        keep_private(&folder).expect("kept");
        assert_eq!(mode(&folder), 0o700);
        let left = folder.join(format!("1-2-3-Font.ttf{COPY_SUFFIX}"));
        std::fs::write(&left, b"left").expect("a left copy");
        std::fs::write(folder.join("notes.txt"), b"keep").expect("another file");
        std::fs::create_dir(folder.join(format!("folder{COPY_SUFFIX}"))).expect("a folder");
        std::os::unix::fs::symlink(folder.join("notes.txt"), folder.join(format!("link{COPY_SUFFIX}")))
            .expect("a link");
        clear_left_copies(&folder);
        let mut there: Vec<String> = std::fs::read_dir(&folder)
            .expect("the folder")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        there.sort();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(there, [format!("folder{COPY_SUFFIX}"), format!("link{COPY_SUFFIX}"), "notes.txt".to_owned()]);
    }

    /// On the screens' thread a font not read yet is refused, never read there: the loader
    /// thread reads it, says so, and the slide is then drawn as anywhere else, fitted at the
    /// size its own font needs rather than the one it came to without it.
    #[test]
    fn the_screens_thread_never_reads_a_font_file_and_draws_again_once_it_is_read() {
        let Some(font) = fonts_to_copy().into_iter().next() else { return };
        let folder = copy_into("screens", &font);
        let family = family_in(&folder);
        // Nine lines: shrunk to fit the box.
        let text = ["A line of plain words"; 9].join("\n");
        let screens = {
            let (folder, family, text) = (folder.clone(), family.clone(), text.clone());
            std::thread::spawn(move || {
                let (tx, rx) = channel();
                let tx = Mutex::new(tx);
                read_fonts_elsewhere(Arc::new(move || {
                    let _ = tx.lock().map(|tx| tx.send(()));
                }));
                let mut fonts = Fonts::only_from(&[&folder]);
                let first = drawn(&mut fonts, &family, &text);
                let fitted_then = fonts.fitted.len();
                rx.recv_timeout(Duration::from_secs(10)).expect("the loader said the font was read");
                fonts.catch_up();
                let forgotten = fonts.fitted.is_empty();
                (first, fitted_then, forgotten, drawn(&mut fonts, &family, &text))
            })
        };
        let (first, fitted_then, forgotten, second) = screens.join().expect("the screens' thread");
        let expected = drawn(&mut Fonts::only_from(&[&folder]), &family, &text);
        let _ = std::fs::remove_dir_all(&folder);
        assert!(first != expected, "the screens drew the font before anything read it");
        assert!(fitted_then > 0 && forgotten, "the size fitted without the font was kept");
        assert!(second == expected, "drawn differently on the screens once the font was there");
    }

    /// Run by the test below in a process of its own, where nothing has looked for the fonts.
    #[test]
    fn screens_first_helper() {
        let Some(family) = std::env::var_os("SP_TEST_SCREENS_FIRST") else { return };
        let family = family.to_string_lossy().into_owned();
        let text = "Words on the screens";
        let screens = {
            let family = family.clone();
            std::thread::spawn(move || {
                let (tx, rx) = channel();
                let tx = Mutex::new(tx);
                read_fonts_elsewhere(Arc::new(move || {
                    let _ = tx.lock().map(|tx| tx.send(()));
                }));
                let mut fonts = Fonts::new();
                let there_at_first =
                    FOUND.get().is_some() || fonts.found || fonts.family_names() != [DEFAULT_FONT_FAMILY];
                let empty_at_first = glyphs(&mut fonts, &family, text).is_empty() && !fonts.found;
                // Each time the loader says something is there, again: first the fonts are
                // found, then the family's file is read.
                let mut laid_out = Vec::new();
                for _ in 0..5 {
                    if rx.recv_timeout(Duration::from_secs(10)).is_err() {
                        break;
                    }
                    laid_out = glyphs(&mut fonts, &family, text);
                    if !laid_out.is_empty() {
                        break;
                    }
                }
                (there_at_first, empty_at_first, laid_out)
            })
        };
        let (there_at_first, empty_at_first, laid_out) = screens.join().expect("the screens' thread");
        assert!(!there_at_first, "the fonts were there before anything looked for them");
        assert!(empty_at_first, "the screens' thread looked for the fonts itself");
        assert!(!laid_out.is_empty(), "never laid out once the fonts were found");
        assert_eq!(laid_out, glyphs(&mut Fonts::new(), &family, text), "laid out differently on the screens");
        println!("helper: the fonts were found and read for the screens' thread");
    }

    /// The screens' thread never looks for the computer's fonts itself, even when it is the
    /// first to want them: the loader thread looks, then reads the font, and the screens draw
    /// again each time.
    #[test]
    fn the_screens_thread_never_looks_for_the_fonts_itself() {
        let Some(font) = fonts_to_copy().into_iter().next() else { return };
        let folder = copy_into("screens-first", &font);
        let family = family_in(&folder);
        let _ = std::fs::remove_dir_all(&folder);
        let program = std::env::current_exe().expect("this test program");
        let ran = std::process::Command::new(&program)
            .args(["fonts::tests::screens_first_helper", "--exact", "--nocapture", "--test-threads=1"])
            .env("SP_TEST_SCREENS_FIRST", &family)
            .output()
            .expect("the helper ran");
        let said = format!("{}\n{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr));
        assert!(ran.status.success() && said.contains("helper: "), "the helper ended with {:?}:\n{said}", ran.status);
    }

    /// A font not read yet asks only for itself: the fonts the shaper falls back on meanwhile
    /// are refused too but not read, unless they turn out to be needed once it is there.
    #[test]
    fn a_font_not_read_yet_asks_for_itself_only_not_every_fallback_too() {
        let fonts = fonts_to_copy();
        let [first, second, ..] = fonts.as_slice() else {
            eprintln!("NOTHING LEARNT: no two fonts to copy on this computer");
            return;
        };
        let (first, second) = (copy_into("cascade-first", first), copy_into("cascade-second", second));
        let (first_family, second_family) = (family_in(&first), family_in(&second));
        let (first_file, second_file) = (the_file_in(&first), the_file_in(&second));
        let (asked_first, asked_second, refused, laid_out) = {
            let (first, second) = (first.clone(), second.clone());
            std::thread::spawn(move || {
                let (tx, rx) = channel();
                let tx = Mutex::new(tx);
                read_fonts_elsewhere(Arc::new(move || {
                    let _ = tx.lock().map(|tx| tx.send(()));
                }));
                let mut fonts = Fonts::only_from(&[&first, &second]);
                let collection = &mut fonts.font_cx.collection;
                let fallback = collection.family_id(&second_family).expect("the second family");
                collection.set_fallbacks(FallbackKey::new(Script::from_bytes(*b"Latn"), None), [fallback].into_iter());
                let before = REFUSED.get();
                assert!(glyphs(&mut fonts, &first_family, "Words").is_empty(), "drawn before it was read");
                let refused = REFUSED.get() - before;
                rx.recv_timeout(Duration::from_secs(10)).expect("the loader said the font was read");
                let laid_out = !glyphs(&mut fonts, &first_family, "Words").is_empty();
                (read_or_reading(&the_file_in(&first)), read_or_reading(&the_file_in(&second)), refused, laid_out)
            })
            .join()
            .expect("the screens' thread")
        };
        let _ = (std::fs::remove_dir_all(&first), std::fs::remove_dir_all(&second));
        assert!(refused >= 2, "the fallback wasn't even asked for, so this tested nothing ({refused})");
        assert!(asked_first && laid_out, "the font itself was never read ({first_file:?})");
        assert!(!asked_second, "the fallback was read though it wasn't needed ({second_file:?})");
    }

    /// Run by the measurement below, in a process of its own so that no font is read yet.
    #[test]
    fn first_drawing_helper() {
        let Some(text) = std::env::var_os("SP_TEST_FIRST_DRAWING") else { return };
        let text = text.to_string_lossy().into_owned();
        let screens = std::env::var_os("SP_TEST_FIRST_DRAWING_SCREENS").is_some();
        let family = "Helvetica Neue";
        // The fonts are found, and the family's own font read, before the clock starts.
        let _ = drawn(&mut Fonts::new(), family, "Words");
        let took = if screens {
            std::thread::spawn(move || {
                let (tx, rx) = channel();
                let tx = Mutex::new(tx);
                read_fonts_elsewhere(Arc::new(move || {
                    let _ = tx.lock().map(|tx| tx.send(()));
                }));
                let mut fonts = Fonts::new();
                let _ = drawn(&mut fonts, family, "Words");
                let start = std::time::Instant::now();
                let _ = drawn(&mut fonts, family, &text);
                let mut last = start.elapsed();
                // Each font the shaper goes on to is asked for in turn, and drawn again once
                // it is there.
                while rx.recv_timeout(Duration::from_millis(500)).is_ok() {
                    let _ = drawn(&mut fonts, family, &text);
                    last = start.elapsed();
                }
                last
            })
            .join()
            .expect("the screens' thread")
        } else {
            let start = std::time::Instant::now();
            let _ = drawn(&mut Fonts::new(), family, &text);
            start.elapsed()
        };
        let (mut read, mut mapped) = (0, 0);
        for (path, blob) in &files().read {
            let size = blob.as_ref().map_or(0, |blob| blob.len() as u64);
            if how(path).flatten().is_some() { mapped += size } else { read += size }
        }
        let (took, read, mapped) = (took.as_secs_f64() * 1000.0, read as f64 / 1e6, mapped as f64 / 1e6);
        println!("helper: {took:.2} ms; fonts read whole {read:.1} MB, mapped {mapped:.1} MB");
    }

    /// How long the first drawing of a slide with a letter the theme's font lacks takes, on the
    /// operator's thread and (through the loader thread) on the screens'. Prints; run by hand.
    #[test]
    #[ignore = "a measurement, run by hand with --nocapture"]
    fn how_long_the_first_drawing_of_a_letter_from_a_fallback_takes() {
        let program = std::env::current_exe().expect("this test program");
        for (what, text) in
            [("plain", "A line of plain words"), ("emoji", "Joy \u{1F600}"), ("cross", "\u{271D} Cross")]
        {
            for screens in [false, true] {
                let mut times = Vec::new();
                for _ in 0..5 {
                    let mut helper = std::process::Command::new(&program);
                    helper
                        .args(["fonts::tests::first_drawing_helper", "--exact", "--nocapture", "--test-threads=1"])
                        .env("SP_TEST_FIRST_DRAWING", text);
                    if screens {
                        helper.env("SP_TEST_FIRST_DRAWING_SCREENS", "1");
                    }
                    let ran = helper.output().expect("the helper ran");
                    let said = String::from_utf8_lossy(&ran.stdout).into_owned();
                    let line = said.lines().find_map(|line| line.split_once("helper: ")).map(|(_, rest)| rest);
                    times.push(line.unwrap_or("failed").to_owned());
                }
                let place = if screens { "screens'" } else { "operator's" };
                println!("{what} on the {place} thread: {}", times.join(" | "));
            }
        }
    }
}
