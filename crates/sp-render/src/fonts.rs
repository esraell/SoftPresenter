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
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use parley::PositionedLayoutItem;
    use parley::fontique::{FallbackKey, Script};
    use sp_model::{Object, Role, Slide, Theme};

    use super::*;
    use crate::scene::{build_scene, lay_out};

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
        FILES
            .get()
            .and_then(|files| files.lock().ok())
            .is_some_and(|files| files.read.get(path).is_some_and(Option::is_some) || files.reading.contains(path))
    }

    /// Every glyph of a text laid out in a family, with its place.
    fn glyphs(fonts: &mut Fonts, family: &str, text: &str) -> Vec<(u32, f32, f32)> {
        let mut style = Theme::builtin().templates[0].boxes[0].clone();
        style.font_family = family.to_owned();
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

    /// Run by the test below in a process of its own, because where this goes wrong the
    /// process is killed, and that must not be the one running the tests.
    #[test]
    fn font_file_changes_helper() {
        let Some(folder) = std::env::var_os("SP_TEST_FONT_FOLDER").map(PathBuf::from) else { return };
        let how = std::env::var("SP_TEST_FONT_CHANGE").unwrap_or_default();
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
            assert_eq!(laid_out, glyphs(&mut fonts, missing, AFTER));
            assert!(drawn(&mut fonts, &family, AFTER) == drawn(&mut fonts, missing, AFTER));
            let letters = laid_out.iter().filter(|glyph| glyph.0 != 0).count();
            println!(
                "helper: drawn as a missing font ({letters} letters) after the file was {how} before its first use"
            );
            return;
        }
        let mut fonts = Fonts::only_from(&[&spoilt]);
        assert!(!glyphs(&mut fonts, &family, BEFORE).is_empty());
        let _ = drawn(&mut fonts, &family, BEFORE);
        spoil(&file, &how);
        let after = glyphs(&mut fonts, &family, AFTER);
        let after_drawn = drawn(&mut fonts, &family, AFTER);
        let mut whole = Fonts::only_from(&[&folder.join("whole")]);
        assert!(after.len() >= AFTER.chars().filter(|c| !c.is_whitespace()).count());
        assert!(after.iter().all(|glyph| glyph.0 != 0), "a letter was laid out as the no-such-letter box");
        assert_eq!(after, glyphs(&mut whole, &family, AFTER), "laid out differently once the file changed");
        assert!(after_drawn == drawn(&mut whole, &family, AFTER), "drawn differently once the file changed");
        println!("helper: the same after the file was {how}");
    }

    /// A font file that another program cuts short, writes over or takes away while text is
    /// drawn in it changes nothing: the text is laid out and drawn as it would have been. One
    /// spoilt before its first use is drawn as a missing font is. (With the published font
    /// library, which maps font files, cutting the file short or writing over it kills the
    /// helper process with a bus error.)
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
        for how in ways {
            let folder = program.parent().expect("its folder").join(format!("font-test-{}-{how}", std::process::id()));
            for part in ["spoilt", "whole"] {
                std::fs::create_dir_all(folder.join(part)).expect("a folder for the copies");
                std::fs::copy(font, folder.join(part).join(font.file_name().expect("a name"))).expect("a copy");
            }
            let ran = std::process::Command::new(&program)
                .args(["fonts::tests::font_file_changes_helper", "--exact", "--nocapture", "--test-threads=1"])
                .env("SP_TEST_FONT_FOLDER", &folder)
                .env("SP_TEST_FONT_CHANGE", how)
                .env("SP_TEST_FONT_OTHER", other)
                .output()
                .expect("the helper ran");
            let _ = std::fs::remove_dir_all(&folder);
            let said = format!("{}\n{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr));
            assert!(
                ran.status.success() && said.contains("helper: "),
                "after the font file was {how}, the helper ended with {:?}:\n{said}",
                ran.status
            );
        }
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
}
