//! The fonts slide text is laid out and drawn in, and where their data comes from.
//!
//! The computer's fonts are looked for once, by the first thread that needs them (never the
//! thread that draws the screens): their names and the files each family is in, kept in one
//! collection that every [`Fonts`] starts from. A font file's data is read into memory the app
//! owns the first time text is laid out in that font, and kept for every thread, so each file is
//! read once and only the fonts slides use are read at all.
//!
//! A font file's data is never mapped. A mapped file that another program cuts short or writes
//! over while the app runs (a font manager, a system update, installing a font) kills the app
//! with a bus error the next time a letter is shaped or drawn from it, and nothing can catch
//! that. The font library was changed for this: `third_party/fontique/PROVENANCE.md`.
//!
//! The thread that draws the screens never reads a font file and never looks for the fonts
//! (see [`draw_kept_only`](crate::draw_kept_only)). A font it doesn't have yet is read by a
//! loader thread of its own, and the screens are told to draw again once it is there.
//! Meanwhile they draw the words in the fonts they do have; with none that suits, the words
//! are left out until the font is there, which is a few milliseconds.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

use parley::fontique::{
    Blob, Collection, CollectionOptions, FontInfo, SourceCache, SourceId, SourceInfo, SourceKind, SourceLoader,
};
use parley::{FontContext, LayoutContext};

/// Font lookup and text-layout scratch space. Make one per thread that draws and keep it.
pub struct Fonts {
    pub(crate) font_cx: FontContext,
    pub(crate) layout_cx: LayoutContext<()>,
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
        let collection = Collection::new(CollectionOptions { shared: false, system_fonts: true });
        READ_SO_FAR.fetch_add(1, Ordering::AcqRel);
        collection
    })
}

/// Reads a font file into memory of its own. `None` if it can't be read, or what is there
/// isn't a font (cut short or written over before it was first read): the font is then drawn
/// as a missing one is.
fn read_font_file(path: &Path) -> Option<Blob<u8>> {
    let blob = Blob::new(Arc::new(std::fs::read(path).ok()?));
    FontInfo::from_source(SourceInfo::new(SourceId::new(), SourceKind::Memory(blob.clone())), 0)?;
    Some(blob)
}

/// The data of a font file, for the shaper and the renderer: kept from the first time it was
/// read, or read now. On a thread that never reads (the screens'), one not read yet is asked of
/// the loader thread and refused for now.
fn font_file(path: &Path) -> Option<Blob<u8>> {
    let files = FILES.get_or_init(Mutex::default);
    if let Ok(mut files) = files.lock() {
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
    let Ok(mut files) = files.lock() else { return read };
    // Another thread may have read it meanwhile: its copy is kept, so all draw from one.
    let kept = files.read.entry(path.to_owned()).or_insert(read).clone();
    READ_SO_FAR.fetch_add(1, Ordering::AcqRel);
    kept
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
                        if let Ok(mut files) = FILES.get_or_init(Mutex::default).lock() {
                            files.reading.remove(&path);
                            files.read.entry(path).or_insert(read);
                        }
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

impl Fonts {
    /// Uses the fonts installed on this computer. The first one made looks for them, which
    /// takes a moment; on the screens' thread the loader thread does that instead.
    pub fn new() -> Fonts {
        let collection = if elsewhere().is_some() { FOUND.get().cloned() } else { Some(found().clone()) };
        let found = collection.is_some();
        let collection =
            collection.unwrap_or_else(|| Collection::new(CollectionOptions { shared: false, system_fonts: false }));
        Fonts::with(collection, found)
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
    pub(crate) fn fitted_size(&mut self, text: &str, style: &sp_model::BoxStyle, box_size: (f32, f32)) -> Option<f32> {
        self.catch_up();
        let at = self.fitted.iter().rposition(|f| f.box_size == box_size && f.text == text && f.style == *style)?;
        // Used again: moved to the end, so the ones not used for longest are let go first.
        let found = self.fitted.remove(at);
        let size = found.size;
        self.fitted.push(found);
        Some(size)
    }

    /// Remembers the size text was fitted to a box at.
    pub(crate) fn remember_fit(&mut self, text: &str, style: &sp_model::BoxStyle, box_size: (f32, f32), size: f32) {
        if self.fitted.len() >= MOST_FITTED {
            self.fitted.remove(0);
        }
        self.fitted.push(Fitted { text: text.to_owned(), style: style.clone(), box_size, size });
    }
}

impl Fonts {
    /// The names of the font families installed on this computer, sorted.
    pub fn family_names(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self.font_cx.collection.family_names().map(str::to_owned).collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
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
    use std::path::PathBuf;

    use parley::PositionedLayoutItem;

    use super::Fonts;
    use crate::scene::lay_out;

    /// A font with pair kerning that this computer has, to copy and spoil. `None` on a computer
    /// with none of them.
    fn a_font_with_kerning() -> Option<PathBuf> {
        [
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
    }

    /// Every glyph of a text laid out in the one family these fonts have, with its place.
    fn glyphs(fonts: &mut Fonts, text: &str) -> Vec<(u32, f32, f32)> {
        let mut style = sp_model::Theme::builtin().templates[0].boxes[0].clone();
        style.font_family = fonts.family_names().into_iter().next().expect("a family in the copy");
        let layout = lay_out(fonts, text, &style, 60.0, 10_000.0);
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

    const BEFORE: &str = "mm";
    /// Letters not laid out before the file changes, in pairs a font kerns.
    const AFTER: &str = "AVATAR Type WAVE To. LTV 0123456789 fjord";

    /// Run by the test below in a process of its own, because where this goes wrong the
    /// process is killed, and that must not be the one running the tests.
    #[test]
    fn font_file_changes_helper() {
        let Some(folder) = std::env::var_os("SP_TEST_FONT_FOLDER").map(PathBuf::from) else { return };
        let how = std::env::var("SP_TEST_FONT_CHANGE").unwrap_or_default();
        let spoilt = folder.join("spoilt");
        let file = std::fs::read_dir(&spoilt).expect("the folder").flatten().next().expect("the copy").path();
        let mut fonts = Fonts::only_from(&[&spoilt]);
        assert!(!glyphs(&mut fonts, BEFORE).is_empty());
        match how.as_str() {
            "cut" => {
                let open = std::fs::OpenOptions::new().write(true).open(&file).expect("the copy opens");
                open.set_len(4096).expect("the copy is cut short");
            }
            "rewritten" => std::fs::write(&file, vec![0u8; 20_000]).expect("the copy is written over"),
            "gone" => std::fs::remove_file(&file).expect("the copy is taken away"),
            other => panic!("no such change: {other}"),
        }
        let after = glyphs(&mut fonts, AFTER);
        let whole = glyphs(&mut Fonts::only_from(&[&folder.join("whole")]), AFTER);
        assert!(after.len() >= AFTER.chars().filter(|c| !c.is_whitespace()).count());
        assert!(after.iter().all(|glyph| glyph.0 != 0), "a letter was laid out as the no-such-letter box");
        assert_eq!(after, whole, "the text is laid out differently once the file has changed");
        println!("helper: the same after the file was {how}");
    }

    /// A font file that another program cuts short, writes over or takes away while text is
    /// being drawn in it changes nothing: the text is laid out as it would have been.
    #[test]
    fn text_is_laid_out_the_same_after_its_font_file_changes() {
        let Some(font) = a_font_with_kerning() else {
            eprintln!("NOTHING LEARNT: no font to copy on this computer");
            return;
        };
        let program = std::env::current_exe().expect("this test program");
        for how in ["cut", "rewritten", "gone"] {
            let folder = program.parent().expect("its folder").join(format!("font-test-{}-{how}", std::process::id()));
            for part in ["spoilt", "whole"] {
                std::fs::create_dir_all(folder.join(part)).expect("a folder for the copies");
                std::fs::copy(&font, folder.join(part).join(font.file_name().expect("a name"))).expect("a copy");
            }
            let ran = std::process::Command::new(&program)
                .args(["fonts::tests::font_file_changes_helper", "--exact", "--nocapture", "--test-threads=1"])
                .env("SP_TEST_FONT_FOLDER", &folder)
                .env("SP_TEST_FONT_CHANGE", how)
                .output()
                .expect("the helper ran");
            let _ = std::fs::remove_dir_all(&folder);
            let said = format!("{}\n{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr));
            assert!(
                ran.status.success() && said.contains("helper: the same"),
                "after the font file was {how}, the helper ended with {:?}:\n{said}",
                ran.status
            );
        }
    }
}
