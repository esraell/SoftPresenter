//! Pictures that slide objects show, by the file they name. The app says how a file becomes a
//! picture; each is read once and kept, so every screen shares it. What is kept is held to a
//! size: past it, the pictures drawn longest ago are let go, and read again if they are drawn
//! again.
//!
//! The thread drawing the screens never reads a file: a picture it doesn't have yet is read by
//! a loader thread of its own while the screens show its small copy (or nothing), and the
//! screens are told to draw again once it is there. Reading a big photo, or taking a video's
//! still, can take a good part of a second, which the audience would see as a freeze.
//!
//! Something drawn small (a thumbnail, a preview) gets a small copy of the picture instead.
//! The renderer keeps every picture it draws in a texture of its own until that fills, and a
//! grid of thumbnails of full-size pages would fill it with full-size pictures: a 60-page deck
//! took a gigabyte that way.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Sender, channel};
use std::sync::{LockResult, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use crate::Picture;

/// Reads the picture a file names, or `None` if it can't.
pub type Loader = fn(&str) -> Option<Picture>;

/// Reads the picture a file names no larger than the given number of pixels on its longer side
/// (decoded at that size, so a big photo is never held whole for a thumbnail), or `None` if it
/// can't.
pub type SmallLoader = fn(&str, u32) -> Option<Picture>;

/// How many bytes of pixels are kept at most: room for about sixty full-screen 1080p pictures.
const KEEP_BYTES: usize = 512 * 1024 * 1024;

/// A picture drawn no larger than this (its longer side, in pixels) is drawn from a copy this
/// size.
pub(crate) const SMALL_SIDE: u32 = 640;

/// The name a file's small copy is kept under.
fn small_name(file: &str) -> String {
    format!("{file}\u{0}small")
}

static LOADER: OnceLock<Loader> = OnceLock::new();
static SMALL_LOADER: OnceLock<SmallLoader> = OnceLock::new();
static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
/// Files for the loader thread to read, and what to do once one is read.
static QUEUE: OnceLock<Mutex<Sender<String>>> = OnceLock::new();
type Loaded = Box<dyn Fn() + Send + Sync>;
static ON_LOADED: Mutex<Option<Loaded>> = Mutex::new(None);

thread_local! {
    /// Whether this thread only draws pictures already kept (the screens' thread).
    static KEPT_ONLY: Cell<bool> = const { Cell::new(false) };
    /// How long this thread has waited, in all, for the locks every thread that draws shares
    /// (the pictures kept, the font files read).
    static LOCK_WAITS: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// Takes one of the locks every thread that draws shares, adding the time it waited for it to
/// this thread's [`lock_waits`].
pub(crate) fn timed_lock<T>(lock: &Mutex<T>) -> LockResult<MutexGuard<'_, T>> {
    let asked = Instant::now();
    let taken = lock.lock();
    LOCK_WAITS.with(|waits| waits.set(waits.get() + asked.elapsed()));
    taken
}

/// How long the calling thread has waited, in all, for the locks every thread that draws
/// shares: the pictures kept and the font files read. Another thread holding one while it does
/// something slow would show here; the screens' thread reads it to say where a hold-up went.
pub fn lock_waits() -> Duration {
    LOCK_WAITS.with(Cell::get)
}

/// Says how a picture object's file is read. Set once, when the app starts.
pub fn set_picture_loader(loader: Loader) {
    let _ = LOADER.set(loader);
}

/// Says how a picture object's file is read for a small copy. Set once, when the app starts;
/// without it a file is read whole and shrunk.
pub fn set_small_picture_loader(loader: SmallLoader) {
    let _ = SMALL_LOADER.set(loader);
}

/// Reads a file's picture for a small copy: at that size if the app says how, else whole and
/// shrunk, the whole one let go at once.
fn read_small(file: &str) -> Option<Picture> {
    match SMALL_LOADER.get() {
        Some(load) => load(file, SMALL_SIDE).map(|small| small.shrunk(SMALL_SIDE)),
        None => LOADER.get().and_then(|load| load(file)).map(|full| full.shrunk(SMALL_SIDE)),
    }
}

/// Says this thread draws only pictures and fonts already read, never reading a file itself: a
/// picture or a font not read yet is read by a loader thread meanwhile, and `loaded` is called
/// once it is there, so the thread can draw again. For the thread that draws the screens.
pub fn draw_kept_only(loaded: impl Fn() + Send + Sync + 'static) {
    KEPT_ONLY.with(|only| only.set(true));
    let loaded: crate::fonts::Loaded = std::sync::Arc::new(loaded);
    crate::fonts::read_fonts_elsewhere(loaded.clone());
    if let Ok(mut on_loaded) = ON_LOADED.lock() {
        *on_loaded = Some(Box::new(move || loaded()));
    }
}

/// Has the pictures a slide shows read ahead, on the loader thread, so they are kept by the
/// time it goes on screen: the screens' thread asks for the next slide's.
pub fn read_ahead(slide: &sp_model::Slide) {
    for object in &slide.objects {
        if let sp_model::ObjectKind::Image { file, .. } = &object.kind {
            let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
            if let Ok(mut kept) = timed_lock(kept)
                && !kept.pictures.contains_key(file.as_str())
                && !kept.live.contains(file.as_str())
            {
                queue_read(&mut kept, file);
            }
        }
    }
}

/// True while a file's picture is being read by the loader thread (so it is drawn as nothing
/// for now, not as a picture that can't be read).
pub(crate) fn coming(file: &str) -> bool {
    KEPT.get().and_then(|kept| timed_lock(kept).ok()).is_some_and(|kept| kept.reading.contains(file))
}

/// Has the loader thread read a file, unless it is reading it already.
fn queue_read(kept: &mut Kept, file: &str) {
    if !kept.reading.insert(file.to_owned()) {
        return;
    }
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = channel::<String>();
        let started = std::thread::Builder::new().name("pictures".to_owned()).spawn(move || {
            for file in rx {
                read_into_kept(&file);
            }
        });
        if let Err(e) = started {
            eprintln!("could not start the picture loader: {e}");
        }
        Mutex::new(tx)
    });
    if queue.lock().map(|tx| tx.send(file.to_owned())).is_err() {
        kept.reading.remove(file);
    }
}

/// On the loader thread: reads a file, keeps its picture and small copy, and says it is there.
fn read_into_kept(file: &str) {
    let read = LOADER.get().and_then(|load| load(file));
    let small = read.as_ref().map(|full| full.shrunk(SMALL_SIDE));
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    if let Ok(mut kept) = timed_lock(kept) {
        kept.reading.remove(file);
        // A video's newest frame shown in its place meanwhile wins over its still.
        if !kept.live.contains(file) {
            if let Some(small) = small {
                kept.insert(&small_name(file), Some(small));
            }
            kept.insert(file, read);
        }
    }
    if let Some(loaded) = ON_LOADED.lock().ok().as_ref().and_then(|on| on.as_ref()) {
        loaded();
    }
}

fn kept_only() -> bool {
    KEPT_ONLY.with(Cell::get)
}

/// The picture a file names: read the first time it is asked for, then kept. A file that
/// can't be read is remembered as missing, so it is not tried again on every frame. On the
/// screens' thread, one not kept yet is read meanwhile (see [`draw_kept_only`]), and
/// its small copy, if kept, stands in for it.
pub(crate) fn picture(file: &str) -> Option<Picture> {
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    if let Ok(mut kept) = timed_lock(kept) {
        if let Some(found) = kept.get(file) {
            return found;
        }
        if kept_only() {
            queue_read(&mut kept, file);
            return kept.get(&small_name(file)).flatten();
        }
    }
    // Read without holding the lock, so another screen isn't kept waiting.
    let read = LOADER.get().and_then(|load| load(file));
    if let Ok(mut kept) = timed_lock(kept) {
        kept.insert(file, read.clone());
    }
    read
}

/// The picture a file names, for drawing with its longer side `drawn_side` pixels long: a small
/// copy when that is no more than [`SMALL_SIDE`], made once and kept (the full picture is not
/// kept for it). A picture shown in place of a file's, such as a video's newest frame, is
/// always handed back as it is, so it never stops moving in a thumbnail.
pub(crate) fn picture_at(file: &str, drawn_side: f64) -> Option<Picture> {
    if drawn_side > f64::from(SMALL_SIDE) {
        return picture(file);
    }
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    let small = small_name(file);
    if let Ok(mut kept) = timed_lock(kept) {
        if kept.live.contains(file) {
            return kept.get(file).flatten();
        }
        if let Some(found) = kept.get(&small) {
            return found;
        }
        // The full picture, if it is kept already, makes the copy without reading the file
        // (the screens' thread draws it as it is rather than spend time shrinking it).
        if let Some(Some(full)) = kept.get(file) {
            if kept_only() {
                return Some(full);
            }
            let copy = full.shrunk(SMALL_SIDE);
            kept.insert(&small, Some(copy.clone()));
            return Some(copy);
        }
        if kept_only() {
            queue_read(&mut kept, file);
            return None;
        }
    }
    let read = read_small(file);
    if let Ok(mut kept) = timed_lock(kept) {
        kept.insert(&small, read.clone());
    }
    read
}

/// A picture put in place by [`show_picture`] (a web page's newest frame), and nothing else:
/// never read from a file.
pub(crate) fn shown(name: &str) -> Option<Picture> {
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    let mut kept = timed_lock(kept).ok()?;
    if !kept.live.contains(name) {
        return None;
    }
    kept.get(name).flatten()
}

/// The name a web page box's frames are shown under: its address and its size in slide units
/// (a page is laid out at its box's size, so two boxes of different sizes are two pages).
pub fn web_key(url: &str, width: f32, height: f32) -> String {
    format!("web:{}x{}:{url}", width.round() as i64, height.round() as i64)
}

/// Puts a picture in place of a file's, until it is forgotten: how a video on a slide shows its
/// newest frame where its still would be.
pub fn show_picture(file: &str, picture: Picture) {
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    if let Ok(mut kept) = timed_lock(kept) {
        kept.insert(file, Some(picture));
        kept.live.insert(file.to_owned());
        kept.remove(&small_name(file));
    }
}

/// Forgets a file's picture, so it is read again the next time it is drawn.
pub fn forget_picture(file: &str) {
    if let Some(Ok(mut kept)) = KEPT.get().map(timed_lock) {
        kept.remove(file);
        kept.remove(&small_name(file));
        kept.live.remove(file);
    }
}

/// The pictures kept, each with when it was last asked for, and how many bytes they come to.
struct Kept {
    limit: usize,
    bytes: usize,
    clock: u64,
    pictures: HashMap<String, (Option<Picture>, u64)>,
    /// Files whose picture is shown in place of theirs (see [`show_picture`]).
    live: HashSet<String>,
    /// Files the loader thread is reading.
    reading: HashSet<String>,
}

fn bytes_of(picture: Option<&Picture>) -> usize {
    picture.map_or(0, |p| (p.width() as usize) * (p.height() as usize) * 4)
}

impl Kept {
    fn new(limit: usize) -> Kept {
        Kept { limit, bytes: 0, clock: 0, pictures: HashMap::new(), live: HashSet::new(), reading: HashSet::new() }
    }

    /// The picture kept for a file (`Some(None)` for one known to be missing), marked as just used.
    fn get(&mut self, file: &str) -> Option<Option<Picture>> {
        self.clock += 1;
        let clock = self.clock;
        self.pictures.get_mut(file).map(|(picture, used)| {
            *used = clock;
            picture.clone()
        })
    }

    /// Keeps a picture, letting go of the ones used longest ago while there are too many bytes.
    /// The one just kept always stays, however big it is.
    fn insert(&mut self, file: &str, picture: Option<Picture>) {
        self.remove(file);
        self.clock += 1;
        self.bytes += bytes_of(picture.as_ref());
        self.pictures.insert(file.to_owned(), (picture, self.clock));
        while self.bytes > self.limit {
            let oldest = self
                .pictures
                .iter()
                .filter(|(name, (picture, _))| name.as_str() != file && picture.is_some())
                .min_by_key(|(_, (_, used))| *used)
                .map(|(name, _)| name.clone());
            let Some(oldest) = oldest else { break };
            self.remove(&oldest);
        }
    }

    fn remove(&mut self, file: &str) {
        if let Some((picture, _)) = self.pictures.remove(file) {
            self.bytes -= bytes_of(picture.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: u32) -> Option<Picture> {
        Picture::from_rgba(side, side, vec![0; (side * side * 4) as usize])
    }

    fn wide_picture(file: &str) -> Option<Picture> {
        file.starts_with("wide").then(|| Picture::from_rgba(1280, 720, vec![255; 1280 * 720 * 4])).flatten()
    }

    #[test]
    fn something_drawn_small_gets_a_small_copy_and_a_live_picture_comes_as_it_is() {
        set_picture_loader(wide_picture);
        let small = picture_at("wide-a", 300.0).unwrap();
        assert_eq!((small.width(), small.height()), (SMALL_SIDE, 360));
        assert_eq!(picture_at("wide-a", 1920.0).unwrap().width(), 1280, "drawn large, the whole picture");
        show_picture("wide-live", Picture::from_rgba(1280, 720, vec![0; 1280 * 720 * 4]).unwrap());
        assert_eq!(picture_at("wide-live", 300.0).unwrap().width(), 1280, "a video's frame is never a stale copy");
        forget_picture("wide-live");
        assert!(picture_at("missing", 300.0).is_none());
    }

    static ASKED: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

    fn small_of_wide(file: &str, side: u32) -> Option<Picture> {
        ASKED.lock().unwrap().push((file.to_owned(), side));
        file.starts_with("wide")
            .then(|| Picture::from_rgba(side, side * 9 / 16, vec![255; (side * (side * 9 / 16) * 4) as usize]))
            .flatten()
    }

    #[test]
    fn a_small_copy_is_read_at_the_size_it_needs_not_whole() {
        set_small_picture_loader(small_of_wide);
        let small = picture_at("wide-asks", 300.0).unwrap();
        assert_eq!((small.width(), small.height()), (SMALL_SIDE, 360));
        let asked = ASKED.lock().unwrap().iter().filter(|(file, _)| file == "wide-asks").cloned().collect::<Vec<_>>();
        assert_eq!(asked, [("wide-asks".to_owned(), SMALL_SIDE)], "asked once, for the small side only");
    }

    #[test]
    fn the_screens_thread_never_reads_a_file_itself() {
        set_picture_loader(wide_picture);
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let drawn = std::thread::spawn(move || {
            draw_kept_only(move || {
                let _ = tx.lock().map(|tx| tx.send(()));
            });
            // Not kept yet: nothing for now, and it is coming, not missing.
            let first = picture("wide-screens");
            let coming_then = coming("wide-screens");
            // Once the loader has read it, it is there, full size; and a small copy too.
            rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the loader said it was read");
            (
                first.is_none(),
                coming_then,
                picture("wide-screens").map(|p| p.width()),
                picture_at("wide-screens", 300.0).map(|p| p.width()),
            )
        });
        let (first_none, coming_then, after, small) = drawn.join().unwrap();
        assert!(first_none && coming_then);
        assert_eq!(after, Some(1280));
        assert_eq!(small, Some(SMALL_SIDE), "the loader made the small copy as well");
        assert!(!coming("wide-screens"));
    }

    #[test]
    fn the_pictures_used_longest_ago_are_let_go_past_the_limit() {
        // Room for two 10×10 pictures (400 bytes each).
        let mut kept = Kept::new(800);
        kept.insert("a", square(10));
        kept.insert("b", square(10));
        kept.insert("missing", None);
        assert_eq!(kept.bytes, 800);
        // "a" is drawn again, so "b" is now the one used longest ago.
        assert!(kept.get("a").is_some_and(|p| p.is_some()));
        kept.insert("c", square(10));
        assert_eq!(kept.bytes, 800);
        assert!(kept.get("b").is_none(), "b was let go");
        assert!(kept.get("a").is_some() && kept.get("c").is_some());
        // A missing file is still remembered as missing; it takes no room.
        assert!(matches!(kept.get("missing"), Some(None)));
        // One picture bigger than the limit is kept on its own.
        kept.insert("huge", square(20));
        assert_eq!(kept.bytes, 1600);
        assert!(kept.get("huge").is_some() && kept.get("a").is_none() && kept.get("c").is_none());
        kept.remove("huge");
        assert_eq!(kept.bytes, 0);
    }
}
