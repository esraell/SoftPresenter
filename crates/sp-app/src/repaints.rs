//! What asks for the operator's window to be drawn, how long each drawing takes and where, and
//! what wakes the app's loop, said in the log once a minute (a testing aid): so a window drawn
//! over and over while nothing on it changes shows why, even from a log sent from someone else's
//! computer.
//!
//! Testing aid: `SOFTPRESENTER_LOG_REPAINTS=1`, or a file named `log-repaints` in the data
//! folder (for a computer where setting a variable is a bother), switches it on; read once. Off,
//! a request to draw costs one look at that flag.
//!
//! Every drawing asked for is put down with what asked: a call in the app's code (by its place:
//! the window is a [`HomeWindow`], whose `request_redraw` notes its caller), something the window
//! was told that the toolkit draws again for (the pointer moving, a key), or a time the window
//! set itself to be drawn at and why. For a time the toolkit asked for, the places in the
//! interface's code that asked for the soonest one stand in for it, as the toolkit names them
//! (the clock asks for a drawing a second ahead on every drawing, so it is named only when it was
//! the soonest). Requests that come before one drawing are one drawing, so a drawing is counted
//! once for each different thing that asked for it; one nothing asked for is the window system's
//! own.

use std::collections::BTreeMap;
use std::panic::Location;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use winit::window::Window;

/// Whether the log says what asks for the drawings (see the module's notes).
// Testing aid: SOFTPRESENTER_LOG_REPAINTS=1, or a file named `log-repaints` in the data folder.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| match std::env::var("SOFTPRESENTER_LOG_REPAINTS") {
        Ok(asked) => !matches!(asked.trim().to_ascii_lowercase().as_str(), "" | "0" | "off" | "no" | "false"),
        Err(_) => crate::bible_store::data_dir().is_some_and(|folder| folder.join("log-repaints").exists()),
    })
}

/// What asked for a drawing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Why {
    /// A call in the app's code, by its place.
    Call(&'static str, u32),
    /// A new picture in the Program preview, drawn by the screens' thread.
    LivePreview,
    /// Something the window was told that the toolkit draws the window again for, by the name of
    /// the event (`CursorMoved`, `KeyboardInput`…).
    Told(String),
    /// A time the window set itself to be drawn at.
    Timer(Timer),
    /// A time the toolkit asked for in the drawing before, by the place in the code that asked
    /// (an animation, a tooltip, the clock); empty for the second drawing the toolkit always
    /// makes after one asked for at once.
    Toolkit(String),
    /// Nothing in the app asked: the window system did.
    System,
}

/// Why the window set itself a time to be drawn at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Timer {
    /// The toolkit asked for it (an animation, a tooltip, the clock, a timer's readout).
    #[default]
    Toolkit,
    /// A new size or display scale is settling.
    Settling,
    /// Thumbnails of the size before are still shown.
    OldThumbnails,
    /// A slide that moves on by itself.
    Advance,
    /// An NDI or DeckLink output is starting.
    OutputStarting,
}

impl Timer {
    fn words(self) -> &'static str {
        match self {
            Timer::Toolkit => "a time the toolkit asked for",
            Timer::Settling => "a new size settling",
            Timer::OldThumbnails => "thumbnails of the size before still shown",
            Timer::Advance => "a slide that moves on by itself",
            Timer::OutputStarting => "an output starting",
        }
    }
}

impl Why {
    fn words(&self) -> String {
        match self {
            Why::Call(file, line) => format!("{}:{line}", short_place(file)),
            Why::LivePreview => "a new Program preview picture".to_owned(),
            Why::Told(event) => format!("told {event}"),
            Why::Timer(timer) => timer.words().to_owned(),
            Why::Toolkit(place) if place.is_empty() => "the toolkit's second drawing".to_owned(),
            Why::Toolkit(place) => format!("the toolkit for {place}"),
            Why::System => "the window system".to_owned(),
        }
    }
}

/// A source file's place as the log says it: from the crate's folder on (`sp-app/src/main.rs`,
/// `egui-0.37.0/src/containers/area.rs`), never the folders of the computer it was built on.
fn short_place(file: &str) -> String {
    let file = file.replace('\\', "/");
    let parts: Vec<&str> = file.split('/').collect();
    match parts.iter().rposition(|part| *part == "src") {
        Some(at) if at > 0 => parts[at - 1..].join("/"),
        _ => parts.last().copied().unwrap_or_default().to_owned(),
    }
}

/// Whether the operator's window has been asked to draw since [`redraw_asked`] last said: a
/// change found by a loop pass that no event woke (a thread's flag, a clock) asks for one, and
/// Remote Control counts that as something that happened.
static REDRAW_ASKED: AtomicBool = AtomicBool::new(false);

/// Whether the operator's window was asked to draw since this was last called.
pub fn redraw_asked() -> bool {
    REDRAW_ASKED.swap(false, Ordering::Relaxed)
}

/// The operator's window. Asked to draw, it notes what asked when the log is to say it (see
/// [`on`]); it is the window in every other way.
pub struct HomeWindow(Arc<Window>);

impl HomeWindow {
    pub fn new(window: Arc<Window>) -> HomeWindow {
        HomeWindow(window)
    }

    /// Asks for the window to be drawn, noting the caller's place.
    #[track_caller]
    pub fn request_redraw(&self) {
        REDRAW_ASKED.store(true, Ordering::Relaxed);
        if on() {
            let at = Location::caller();
            asked(Why::Call(at.file(), at.line()));
        }
        self.0.request_redraw();
    }

    /// Asks for the window to be drawn, for the reason given (made only when it is noted).
    pub fn request_redraw_for(&self, why: impl FnOnce() -> Why) {
        REDRAW_ASKED.store(true, Ordering::Relaxed);
        if on() {
            asked(why());
        }
        self.0.request_redraw();
    }
}

impl std::ops::Deref for HomeWindow {
    type Target = Window;

    fn deref(&self) -> &Window {
        &self.0
    }
}

/// The name of an event the window was told, for [`Why::Told`].
pub fn event_name(event: &winit::event::WindowEvent) -> String {
    let said = format!("{event:?}");
    said.split(|c: char| !c.is_alphanumeric()).next().unwrap_or_default().to_owned()
}

/// Where the time of painting a drawing went, after it was laid out.
#[derive(Clone, Copy, Debug, Default)]
pub struct PaintTimes {
    /// Turning what was laid out into triangles.
    pub tessellate: Duration,
    /// Handing the GPU the toolkit's new textures and the triangles.
    pub upload: Duration,
    /// Waiting for the window system's next picture to draw into.
    pub acquire: Duration,
    /// Recording the drawing and handing it to the GPU.
    pub submit: Duration,
    /// Handing the picture back to the window system to show.
    pub present: Duration,
}

/// Where the time of the drawings went, in all.
#[derive(Clone, Copy, Debug, Default)]
struct Times {
    layout: Duration,
    paint: PaintTimes,
}

/// What asked since the last drawing, the time the window set itself and why, where this
/// drawing's time has gone so far, the toolkit's requests (their delays, in the order it took
/// them) in this pass and the one before, and the minute's tally.
#[derive(Default)]
struct Book {
    pending: Vec<Why>,
    timer: Timer,
    times: Times,
    delays: Vec<Duration>,
    delays_before: Vec<Duration>,
    minute: Minute,
}

#[derive(Default)]
struct Minute {
    since: Option<Instant>,
    drawings: u64,
    total: Duration,
    times: Times,
    asked: BTreeMap<Why, u64>,
    /// Each time the loop went to sleep: which of its clocks was the soonest.
    wakes: u64,
    slept_for: BTreeMap<&'static str, u64>,
    /// Clocks that asked for a time already past when the loop went to sleep.
    past: u64,
    slept_on_nothing: u64,
    /// The loop's wake-ups that came at a clock's time, and the events that came, by kind.
    by_clock: u64,
    events: BTreeMap<String, u64>,
}

fn book() -> std::sync::MutexGuard<'static, Book> {
    static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();
    BOOK.get_or_init(Mutex::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn asked(why: Why) {
    let mut book = book();
    if !book.pending.contains(&why) {
        book.pending.push(why);
    }
}

/// Has the toolkit tell this module the delay of each drawing asked of it, in the order it takes
/// them, so the one that comes soonest can be told from the others.
pub fn watch(ctx: &egui::Context) {
    if on() {
        ctx.set_repaint_observer(|info| book().delays.push(info.delay));
    }
}

/// Sets the time the window is next drawn at, `due`, unless one already set is sooner, noting
/// why when this one is kept.
pub fn wake_at(next: &mut Option<Instant>, due: Instant, why: Timer) {
    if next.is_none_or(|at| due < at) {
        *next = Some(due);
        if on() {
            book().timer = why;
        }
    }
}

/// Sets the time the window is next drawn at to `delay` from now (none, for `Duration::MAX`),
/// whatever was set before, noting why.
pub fn wake_after(next: &mut Option<Instant>, delay: Duration, why: Timer) {
    *next = Instant::now().checked_add(delay);
    if on() {
        book().timer = why;
    }
}

/// Why the time set for the window to be drawn at was set.
pub fn timer() -> Timer {
    if on() { book().timer } else { Timer::Toolkit }
}

/// Before the toolkit lays the window out: its requests so far are the ones it will name as the
/// pass before's, as it begins the pass.
pub fn laying_out() {
    if on() {
        let mut book = book();
        book.delays_before = std::mem::take(&mut book.delays);
    }
}

/// After the toolkit has laid the window out, which took `layout`: when this drawing is one the
/// toolkit asked for, the places it names as having asked for the soonest drawing (in the pass
/// before) stand in for "a time the toolkit asked for".
pub fn laid_out(ctx: &egui::Context, layout: Duration) {
    if on() {
        note_layout(ctx, layout);
    }
}

fn note_layout(ctx: &egui::Context, layout: Duration) {
    // The toolkit's causes are read before the book is taken, never while it is held: the
    // toolkit calls the observer, which takes the book, with its own context locked, so taking
    // the two the other way round here could leave this thread and the asking one waiting on
    // each other for good.
    let causes = ctx.repaint_causes();
    let mut book = book();
    book.times.layout += layout;
    let Some(at) = book.pending.iter().position(|why| *why == Why::Timer(Timer::Toolkit)) else { return };
    book.pending.remove(at);
    let places: Vec<String> = soonest(&causes, &book.delays_before)
        .into_iter()
        .map(|cause| {
            let place = format!("{}:{}", short_place(cause.file), cause.line);
            if cause.reason.is_empty() { place } else { format!("{place} ({})", cause.reason) }
        })
        .collect();
    let places = if places.is_empty() { vec![String::new()] } else { places };
    for place in places {
        let why = Why::Toolkit(place);
        if !book.pending.contains(&why) {
            book.pending.push(why);
        }
    }
}

/// The requests that asked for the soonest drawing, paired with their delays in the order the
/// toolkit took them; all of them when they can't be paired (a pass the toolkit did twice).
fn soonest<'a, T>(causes: &'a [T], delays: &[Duration]) -> Vec<&'a T> {
    let first = delays.iter().min();
    if causes.len() != delays.len() {
        return causes.iter().collect();
    }
    causes.iter().zip(delays).filter(|(_, delay)| Some(*delay) == first).map(|(cause, _)| cause).collect()
}

/// Notes where this drawing's painting time went.
pub fn painted(times: PaintTimes) {
    if on() {
        let mut book = book();
        let paint = &mut book.times.paint;
        paint.tessellate += times.tessellate;
        paint.upload += times.upload;
        paint.acquire += times.acquire;
        paint.submit += times.submit;
        paint.present += times.present;
    }
}

/// A drawing of the window is done, having taken `took` in all: what asked for it is counted.
pub fn drawn(took: Duration) {
    if !on() {
        return;
    }
    let mut book = book();
    let pending = std::mem::take(&mut book.pending);
    let times = std::mem::take(&mut book.times);
    let minute = &mut book.minute;
    minute.since.get_or_insert_with(Instant::now);
    minute.drawings += 1;
    minute.total += took;
    minute.times.layout += times.layout;
    let paint = &mut minute.times.paint;
    paint.tessellate += times.paint.tessellate;
    paint.upload += times.paint.upload;
    paint.acquire += times.paint.acquire;
    paint.submit += times.paint.submit;
    paint.present += times.paint.present;
    if pending.is_empty() {
        *minute.asked.entry(Why::System).or_default() += 1;
    }
    for why in pending {
        *minute.asked.entry(why).or_default() += 1;
    }
}

/// The loop has woken: at the time a clock asked for (`by_clock`), or for an event.
pub fn woke(by_clock: bool) {
    if on() && by_clock {
        book().minute.by_clock += 1;
    }
}

/// An event has come to the loop: what it was (`the operator's window told CursorMoved`, `the
/// app was told LivePreview`). Made only when it is noted.
pub fn event(what: impl FnOnce() -> String) {
    if on() {
        *book().minute.events.entry(what()).or_default() += 1;
    }
}

/// The loop goes to sleep until its soonest clock, `soonest` (named), or until something
/// happens (`None`); `past` when that clock asked for a time already gone. Once a minute has
/// passed, the log says the minute's drawings and wake-ups.
pub fn sleeping(soonest: Option<&'static str>, past: bool) {
    if !on() {
        return;
    }
    let mut book = book();
    let minute = &mut book.minute;
    let now = Instant::now();
    let since = *minute.since.get_or_insert(now);
    minute.wakes += 1;
    match soonest {
        Some(name) => *minute.slept_for.entry(name).or_default() += 1,
        None => minute.slept_on_nothing += 1,
    }
    minute.past += u64::from(past);
    let elapsed = now.duration_since(since);
    if elapsed >= Duration::from_secs(60) {
        let said = [minute.drawings_words(elapsed), minute.wakes_words(elapsed)];
        *minute = Minute { since: Some(now), ..Minute::default() };
        drop(book);
        for line in said {
            crate::applog::event("repaints", &line);
        }
    }
}

impl Minute {
    /// The minute's drawings: how many, how long they took and where, and what asked for them,
    /// most first.
    fn drawings_words(&self, elapsed: Duration) -> String {
        let seconds = elapsed.as_secs_f64();
        let each = |total: Duration| total.as_secs_f64() * 1000.0 / self.drawings.max(1) as f64;
        let paint = &self.times.paint;
        let parts = self.times.layout + paint.tessellate + paint.upload + paint.acquire + paint.submit + paint.present;
        let mut asked: Vec<(&Why, &u64)> = self.asked.iter().collect();
        asked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let asked: Vec<String> = asked.iter().map(|(why, count)| format!("{count} {}", why.words())).collect();
        format!(
            "{} drawings of the operator's window in {seconds:.0}s ({:.1}/s), {:.1} ms each (laying out {:.1}, \
             tessellating {:.1}, handing over textures and triangles {:.1}, waiting for the window's picture {:.1}, \
             recording and submitting {:.1}, presenting {:.1}, the rest {:.1}); asked for by {}",
            self.drawings,
            self.drawings as f64 / seconds.max(0.001),
            each(self.total),
            each(self.times.layout),
            each(paint.tessellate),
            each(paint.upload),
            each(paint.acquire),
            each(paint.submit),
            each(paint.present),
            each(self.total.saturating_sub(parts)),
            if asked.is_empty() { "nothing".to_owned() } else { asked.join(", ") },
        )
    }

    /// The minute's wake-ups of the loop, and which of its clocks it slept until, most first.
    fn wakes_words(&self, elapsed: Duration) -> String {
        let mut slept: Vec<(&&str, &u64)> = self.slept_for.iter().collect();
        slept.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let mut slept: Vec<String> = slept.iter().map(|(name, count)| format!("{count} {name}")).collect();
        if self.slept_on_nothing > 0 {
            slept.push(format!("{} nothing (until something happened)", self.slept_on_nothing));
        }
        let mut events: Vec<(&String, &u64)> = self.events.iter().collect();
        events.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let events: Vec<String> = events.iter().map(|(what, count)| format!("{count} {what}")).collect();
        format!(
            "the loop woke {} times in {:.0}s ({:.1}/s), {} at a clock's time, {} of them with a clock already past; \
             it slept until {}; the events that came: {}",
            self.wakes,
            elapsed.as_secs_f64(),
            self.wakes as f64 / elapsed.as_secs_f64().max(0.001),
            self.by_clock,
            self.past,
            slept.join(", "),
            if events.is_empty() { "none".to_owned() } else { events.join(", ") },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The toolkit calls the observer (which notes into the book) while its context is locked,
    /// from whichever thread asked for the drawing; noting the layout reads the toolkit's causes.
    /// A thread asking for drawings over and over while the window's thread notes its layouts
    /// must never leave the two waiting on each other.
    #[test]
    fn a_drawing_asked_from_another_thread_never_stops_the_window() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let ctx = egui::Context::default();
        ctx.set_repaint_observer(|info| book().delays.push(info.delay));
        let stop = Arc::new(AtomicBool::new(false));
        let asker = {
            let (ctx, stop) = (ctx.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    ctx.request_repaint_after(Duration::from_millis(5));
                }
            })
        };
        let (done, finished) = std::sync::mpsc::channel();
        let noter = {
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                for _ in 0..20_000 {
                    asked(Why::Timer(Timer::Toolkit));
                    note_layout(&ctx, Duration::from_micros(1));
                    book().delays.clear();
                }
                let _ = done.send(());
            })
        };
        let finished = finished.recv_timeout(Duration::from_secs(30)).is_ok();
        stop.store(true, Ordering::Relaxed);
        assert!(finished, "the window's thread and the asking thread were left waiting on each other");
        noter.join().unwrap();
        asker.join().unwrap();
    }

    #[test]
    fn places_are_said_from_the_crates_folder_on() {
        assert_eq!(short_place("crates/sp-app/src/main.rs"), "sp-app/src/main.rs");
        assert_eq!(
            short_place("~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.37.0/src/containers/area.rs"),
            "egui-0.37.0/src/containers/area.rs"
        );
        assert_eq!(short_place(r"crates\sp-app\src\home\slides.rs"), "sp-app/src/home/slides.rs");
        assert_eq!(short_place("lib.rs"), "lib.rs");
    }

    #[test]
    fn a_minute_says_what_asked_most_first_and_where_the_time_went() {
        let mut minute = Minute { drawings: 4, total: Duration::from_millis(40), ..Minute::default() };
        minute.times.layout = Duration::from_millis(20);
        minute.times.paint.acquire = Duration::from_millis(8);
        minute.times.paint.present = Duration::from_millis(4);
        minute.asked.insert(Why::Toolkit("sp-app/src/studio.rs:1221".to_owned()), 3);
        minute.asked.insert(Why::LivePreview, 1);
        minute.asked.insert(Why::Told("CursorMoved".to_owned()), 1);
        assert_eq!(
            minute.drawings_words(Duration::from_secs(60)),
            "4 drawings of the operator's window in 60s (0.1/s), 10.0 ms each (laying out 5.0, tessellating 0.0, \
             handing over textures and triangles 0.0, waiting for the window's picture 2.0, recording and \
             submitting 0.0, presenting 1.0, the rest 2.0); asked for by 3 the toolkit for \
             sp-app/src/studio.rs:1221, 1 a new Program preview picture, 1 told CursorMoved"
        );
        minute.wakes = 120;
        minute.by_clock = 100;
        minute.slept_for.insert("next drawing", 60);
        minute.slept_for.insert("video", 59);
        minute.slept_on_nothing = 1;
        minute.past = 2;
        minute.events.insert("the operator's window told CursorMoved".to_owned(), 15);
        minute.events.insert("the app was told LivePreview".to_owned(), 5);
        assert_eq!(
            minute.wakes_words(Duration::from_secs(60)),
            "the loop woke 120 times in 60s (2.0/s), 100 at a clock's time, 2 of them with a clock already past; it \
             slept until 60 next drawing, 59 video, 1 nothing (until something happened); the events that came: 15 \
             the operator's window told CursorMoved, 5 the app was told LivePreview"
        );
    }

    #[test]
    fn the_toolkit_is_named_by_the_soonest_of_its_requests() {
        let causes = ["clock", "tooltip", "fade"];
        let ms = Duration::from_millis;
        assert_eq!(soonest(&causes, &[ms(900), ms(0), ms(0)]), [&"tooltip", &"fade"]);
        assert_eq!(soonest(&causes, &[ms(900), ms(400), ms(500)]), [&"tooltip"]);
        // Not paired: all of them.
        assert_eq!(soonest(&causes, &[ms(900)]), [&"clock", &"tooltip", &"fade"]);
        assert!(soonest::<&str>(&[], &[]).is_empty());
    }
}
