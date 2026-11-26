//! The system's windows for choosing files, a folder or where to save, and its questions with
//! three answers, shown without holding up the window's thread.
//!
//! Shown the old way (`rfd::FileDialog::pick_files()` called from an event), such a window
//! stopped the app's event loop until it was closed: the operator's window stood still, `sp`,
//! the phone remote and every other command waited, and a video dragged from the file window
//! onto the media bin came in only once the file window was closed. A tester on Windows found
//! the app unusable until he closed it.
//!
//! Here each is shown off the loop, and its answer comes back through it
//! (`AppEvent::Dialog`):
//! - On a Mac, as a sheet on the operator's window. AppKit's panels belong to the main thread,
//!   and a sheet runs on it without stopping it; a thread of its own only waits for the answer.
//! - On Windows and Linux, on a thread of its own with no owner window. A file window owned by
//!   the app's window would switch that window off while it is open (it would take no clicks,
//!   and Windows gives a switched-off window no drops), and an owner on another thread ties
//!   the two threads' keyboards and mice together.
//!
//! One at a time. What the operator asks for while one is open brings the open one to the
//! front (on Windows, where it may have gone behind the app's window) and is otherwise let go.
//! A question that work in the background asks (a song pack's songs that are here already)
//! waits for the open one to close.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sp_model::Uuid;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::{applog, pack};

/// What a file window lets the operator choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choose {
    File,
    Files,
    Folder,
    /// (Only where a window can't take files and folders at once.)
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Folders,
    /// Files and folders in one window, which only a Mac has.
    #[cfg(target_os = "macos")]
    FilesOrFolders,
    /// Where to save a file.
    Save,
}

/// A file window: its title, what it chooses, the kinds of file it offers (a name and the
/// endings), and where it opens.
#[derive(Clone, Debug, PartialEq)]
pub struct Files {
    pub title: String,
    pub choose: Choose,
    pub filters: Vec<(String, Vec<String>)>,
    pub directory: Option<PathBuf>,
    pub file_name: Option<String>,
}

impl Files {
    pub fn new(choose: Choose, title: &str) -> Files {
        Files { title: title.to_owned(), choose, filters: Vec::new(), directory: None, file_name: None }
    }

    pub fn filter(mut self, name: &str, endings: &[impl ToString]) -> Files {
        self.filters.push((name.to_owned(), endings.iter().map(ToString::to_string).collect()));
        self
    }

    pub fn directory(mut self, directory: PathBuf) -> Files {
        self.directory = Some(directory);
        self
    }

    pub fn file_name(mut self, name: String) -> Files {
        self.file_name = Some(name);
        self
    }

    /// The same window choosing something else (a folder instead of files, say).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn choosing(mut self, choose: Choose) -> Files {
        self.choose = choose;
        self
    }
}

/// A question with three buttons; the third is the one closing the window gives.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub title: String,
    pub words: String,
    pub buttons: [String; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Window {
    Files(Files),
    Question(Question),
}

#[cfg(test)]
impl Window {
    pub fn title(&self) -> &str {
        match self {
            Window::Files(files) => &files.title,
            Window::Question(question) => &question.title,
        }
    }
}

/// What came of a window: the files or folder chosen (none when it was cancelled), or the
/// button pressed (0, 1 or 2; `None` when it was closed).
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Paths(Vec<PathBuf>),
    Button(Option<usize>),
}

impl Answer {
    pub fn paths(self) -> Vec<PathBuf> {
        match self {
            Answer::Paths(paths) => paths,
            Answer::Button(_) => Vec::new(),
        }
    }

    pub fn button(&self) -> Option<usize> {
        match self {
            Answer::Button(button) => *button,
            Answer::Paths(_) => None,
        }
    }

    /// For the log: how many were chosen, or the button's place; never a file's name.
    fn said(&self) -> String {
        match self {
            Answer::Paths(paths) if paths.is_empty() => "cancelled".to_owned(),
            Answer::Paths(paths) if paths.len() == 1 => "1 file chosen".to_owned(),
            Answer::Paths(paths) => format!("{} files chosen", paths.len()),
            Answer::Button(None) => "closed".to_owned(),
            Answer::Button(Some(button)) => format!("button {} pressed", button + 1),
        }
    }
}

/// An answer on its way back to the window's thread, with the number of the window it is for.
#[derive(Debug)]
pub struct Answered {
    id: u64,
    answer: Answer,
}

/// Hands an answer to the window's thread, from whichever thread the window ended on (in the
/// app, an `AppEvent::Dialog` through the event loop).
pub type Deliver = Arc<dyn Fn(Answered) + Send + Sync>;

/// The window the system's windows belong to (a Mac's sheet is put on it).
pub trait Parent: HasWindowHandle + HasDisplayHandle {}
impl<T: HasWindowHandle + HasDisplayHandle + ?Sized> Parent for T {}

/// Shows the windows: the system's own in the app, a stand-in in the tests.
pub trait Shows {
    /// Shows `window`, and calls `done` once with what came of it, on any thread, at any time
    /// (at once too, when it can't be shown).
    fn show(&mut self, window: Window, parent: Option<&dyn Parent>, done: Box<dyn FnOnce(Answer) + Send>);

    /// Brings the window open now back in front of the app's, where the system lets the app.
    fn bring_forward(&mut self) {}
}

/// What a window is for. Its name, in English, is what the log calls the window (never what
/// was chosen in it).
pub trait Purpose {
    fn name(&self) -> &'static str;
}

/// Whether a window asked for was shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    Shown,
    /// Another was open: it was brought forward, and this one let go.
    Busy,
    /// Another was open: this one is shown once it closes.
    Waiting,
}

/// A window that closed, with what it was for.
pub struct Closed<P> {
    pub purpose: P,
    pub answer: Answer,
}

struct Open<P> {
    id: u64,
    purpose: P,
    since: Instant,
    /// How many times the operator's window had been drawn when it opened.
    drawn: u32,
}

/// The one window open (and those waiting their turn), and what each is for.
pub struct Dialogs<P> {
    shows: Box<dyn Shows>,
    deliver: Deliver,
    open: Option<Open<P>>,
    waiting: VecDeque<(P, Window)>,
    next: u64,
}

impl<P: Purpose> Dialogs<P> {
    pub fn new(shows: Box<dyn Shows>, deliver: Deliver) -> Dialogs<P> {
        Dialogs { shows, deliver, open: None, waiting: VecDeque::new(), next: 1 }
    }

    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Shows a window the operator asked for, unless one is open already: that one is brought
    /// forward instead. `drawn` is how many times the operator's window has been drawn (the
    /// log says how many times it was drawn while the window was open).
    pub fn ask(&mut self, purpose: P, window: Window, parent: Option<&dyn Parent>, drawn: u32) -> Asked {
        if let Some(open) = &self.open {
            applog::event(
                "dialog",
                &format!(
                    "{} asked for while {} is open: that one is brought forward",
                    purpose.name(),
                    open.purpose.name()
                ),
            );
            self.shows.bring_forward();
            return Asked::Busy;
        }
        self.show(purpose, window, parent, drawn);
        Asked::Shown
    }

    /// Shows a window asked for by work done in the background, now if none is open, else
    /// once the open one (and any waiting before it) has closed.
    pub fn ask_in_turn(&mut self, purpose: P, window: Window, parent: Option<&dyn Parent>, drawn: u32) -> Asked {
        if self.open.is_some() {
            applog::event("dialog", &format!("{} waits for the window open to close", purpose.name()));
            self.waiting.push_back((purpose, window));
            return Asked::Waiting;
        }
        self.show(purpose, window, parent, drawn);
        Asked::Shown
    }

    fn show(&mut self, purpose: P, window: Window, parent: Option<&dyn Parent>, drawn: u32) {
        let id = self.next;
        self.next += 1;
        applog::event("dialog", &format!("{} opened", purpose.name()));
        self.open = Some(Open { id, purpose, since: Instant::now(), drawn });
        let deliver = self.deliver.clone();
        let _stall = crate::stall::inside(crate::stall::Doing::Dialog);
        self.shows.show(window, parent, Box::new(move |answer| deliver(Answered { id, answer })));
    }

    /// Takes in an answer: what the window was for and what came of it, or `None` for an
    /// answer to no window open (which can't happen, but is let go). A window waiting its turn
    /// isn't shown here: what the answer is for is done first, and may ask for the next window
    /// of its own (Export's videos question, then where to save), which then goes first. The
    /// caller shows the next one waiting after ([`Dialogs::show_waiting`]).
    pub fn answered(&mut self, answered: Answered, drawn: u32) -> Option<Closed<P>> {
        if self.open.as_ref().is_none_or(|open| open.id != answered.id) {
            return None;
        }
        let open = self.open.take()?;
        let took: Duration = open.since.elapsed();
        applog::event(
            "dialog",
            &format!(
                "{} closed after {:.1} s: {}; the window was drawn {} times meanwhile",
                open.purpose.name(),
                took.as_secs_f64(),
                answered.answer.said(),
                drawn.wrapping_sub(open.drawn),
            ),
        );
        Some(Closed { purpose: open.purpose, answer: answered.answer })
    }

    /// Shows the next window waiting its turn, if none is open.
    pub fn show_waiting(&mut self, parent: Option<&dyn Parent>, drawn: u32) {
        if self.open.is_none()
            && let Some((purpose, window)) = self.waiting.pop_front()
        {
            self.show(purpose, window, parent, drawn);
        }
    }
}

/// The system's own windows, through rfd.
#[derive(Default)]
pub struct System {
    /// The thread showing the window open now (Windows' own number for it; nought when none
    /// is open), to bring that window forward.
    #[cfg(windows)]
    thread: Arc<std::sync::atomic::AtomicU32>,
}

impl Shows for System {
    #[cfg(target_os = "macos")]
    fn show(&mut self, window: Window, parent: Option<&dyn Parent>, done: Box<dyn FnOnce(Answer) + Send>) {
        // The sheet is begun here, on the main thread, when its future is made; a thread of its
        // own only waits for the answer. Without the operator's window, rfd would put it on
        // whichever window AppKit calls main (a screen's, perhaps): none is shown then.
        let Some(parent) = parent else {
            done(Answer::cancelled(&window));
            return;
        };
        let answering = Answering::new(done, Answer::cancelled(&window));
        let waiting = mac_sheet(window, parent);
        on_its_own_thread(answering, move || wait_for(waiting));
    }

    #[cfg(not(target_os = "macos"))]
    fn show(&mut self, window: Window, parent: Option<&dyn Parent>, done: Box<dyn FnOnce(Answer) + Send>) {
        let answering = Answering::new(done, Answer::cancelled(&window));
        #[cfg(windows)]
        let answering = answering.on_windows_thread(self.thread.clone());
        // Testing aid: SOFTPRESENTER_DIALOG_OWNED=1 gives a file window the app's window as its
        // owner on Windows, as the system's own way has it, to compare: Windows then switches
        // the app's window off while the file window is open (no clicks, no drops on it).
        let owned = cfg!(windows) && std::env::var_os("SOFTPRESENTER_DIALOG_OWNED").is_some();
        let built = Built::new(window, parent.filter(|_| owned));
        on_its_own_thread(answering, move || built.show());
    }

    fn bring_forward(&mut self) {
        #[cfg(windows)]
        {
            let thread = self.thread.load(std::sync::atomic::Ordering::Relaxed);
            if thread != 0 && !sp_winapi::bring_thread_window_forward(thread) {
                applog::event("dialog", "the open window couldn't be brought forward");
            }
        }
    }
}

/// Hands a window's answer back, once: the answer the window gave, or, when the thread showing
/// it ends without one (the system's window code panicked, or the thread couldn't start), the
/// answer of a window cancelled. Otherwise the app would wait for that window for ever, and
/// refuse every other until it was restarted.
struct Answering {
    done: Option<Box<dyn FnOnce(Answer) + Send>>,
    /// What to answer for a window that gave none.
    cancelled: Option<Answer>,
    /// Windows' number for the thread showing the window, kept while it shows one (to bring it
    /// forward).
    #[cfg(windows)]
    thread: Option<Arc<std::sync::atomic::AtomicU32>>,
}

impl Answering {
    fn new(done: Box<dyn FnOnce(Answer) + Send>, cancelled: Answer) -> Answering {
        Answering {
            done: Some(done),
            cancelled: Some(cancelled),
            #[cfg(windows)]
            thread: None,
        }
    }

    #[cfg(windows)]
    fn on_windows_thread(mut self, thread: Arc<std::sync::atomic::AtomicU32>) -> Answering {
        self.thread = Some(thread);
        self
    }

    /// Called on the thread that shows the window, as it begins.
    fn begun(&self) {
        #[cfg(windows)]
        if let Some(thread) = &self.thread {
            thread.store(sp_winapi::current_thread_id(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn give(mut self, answer: Answer) {
        self.answer(answer);
    }

    fn answer(&mut self, answer: Answer) {
        #[cfg(windows)]
        if let Some(thread) = &self.thread {
            thread.store(0, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(done) = self.done.take() {
            done(answer);
        }
    }
}

impl Drop for Answering {
    fn drop(&mut self) {
        if self.done.is_some() {
            applog::event("dialog", "the window's thread ended without an answer: taken as cancelled");
            let cancelled = self.cancelled.take().unwrap_or(Answer::Button(None));
            self.answer(cancelled);
        }
    }
}

/// Shows a window on a thread of its own (`show` shows it and waits for it), and hands back its
/// answer. If the thread panics, or can't start (the closure, and `answering` with it, is then
/// dropped here), `answering` answers as for a window cancelled.
fn on_its_own_thread(answering: Answering, show: impl FnOnce() -> Answer + Send + 'static) {
    let started = std::thread::Builder::new().name("file window".to_owned()).spawn(move || {
        answering.begun();
        let answer = {
            // Windows asks for OLE on a thread that drags, and a file can be dragged out of the
            // file window onto the app's (the window's own thread had it, set up by the window
            // library, when the file windows were shown there). Taken down before the answer
            // goes, after rfd's own COM set-up within it has been.
            #[cfg(windows)]
            let _ole = sp_winapi::ole_on_this_thread();
            show()
        };
        answering.give(answer);
    });
    if let Err(problem) = started {
        applog::event("dialog", &format!("couldn't start the window's thread: {problem}"));
    }
}

impl Answer {
    /// What a window that couldn't be shown answers: nothing chosen, or closed.
    fn cancelled(window: &Window) -> Answer {
        match window {
            Window::Files(_) => Answer::Paths(Vec::new()),
            Window::Question(_) => Answer::Button(None),
        }
    }
}

/// A window made ready on the window's thread, to be shown on a thread of its own (Windows and
/// Linux).
#[cfg(not(target_os = "macos"))]
enum Built {
    Files(rfd::FileDialog, Choose),
    Question(rfd::MessageDialog, [String; 3]),
}

#[cfg(not(target_os = "macos"))]
impl Built {
    fn new(window: Window, owner: Option<&dyn Parent>) -> Built {
        match window {
            Window::Files(files) => {
                let mut dialog = rfd::FileDialog::new().set_title(&files.title);
                for (name, endings) in &files.filters {
                    dialog = dialog.add_filter(name, endings);
                }
                if let Some(directory) = &files.directory {
                    dialog = dialog.set_directory(directory);
                }
                if let Some(name) = &files.file_name {
                    dialog = dialog.set_file_name(name);
                }
                if let Some(owner) = owner {
                    dialog = dialog.set_parent(owner);
                }
                Built::Files(dialog, files.choose)
            }
            Window::Question(question) => {
                let mut dialog = rfd::MessageDialog::new()
                    .set_title(&question.title)
                    .set_description(&question.words)
                    .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                        question.buttons[0].clone(),
                        question.buttons[1].clone(),
                        question.buttons[2].clone(),
                    ));
                if let Some(owner) = owner {
                    dialog = dialog.set_parent(owner);
                }
                Built::Question(dialog, question.buttons)
            }
        }
    }

    /// Shows it on this thread, and waits for it.
    fn show(self) -> Answer {
        match self {
            Built::Files(dialog, choose) => Answer::Paths(match choose {
                Choose::File => dialog.pick_file().into_iter().collect(),
                Choose::Files => dialog.pick_files().unwrap_or_default(),
                Choose::Folder => dialog.pick_folder().into_iter().collect(),
                Choose::Folders => dialog.pick_folders().unwrap_or_default(),
                Choose::Save => dialog.save_file().into_iter().collect(),
            }),
            Built::Question(dialog, buttons) => {
                let result = dialog.show();
                let [first, second, third] = &buttons;
                Answer::Button(pack::three_way(result, [first, second, third]))
            }
        }
    }
}

/// Begins a Mac's sheet on the operator's window, and returns what waits for its answer.
#[cfg(target_os = "macos")]
fn mac_sheet(window: Window, parent: &dyn Parent) -> std::pin::Pin<Box<dyn Future<Output = Answer> + Send>> {
    fn handles(chosen: Option<Vec<rfd::FileHandle>>) -> Answer {
        Answer::Paths(chosen.unwrap_or_default().iter().map(|handle| handle.path().to_owned()).collect())
    }
    fn handle(chosen: Option<rfd::FileHandle>) -> Answer {
        Answer::Paths(chosen.iter().map(|handle| handle.path().to_owned()).collect())
    }
    match window {
        Window::Files(files) => {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(&files.title).set_parent(parent);
            for (name, endings) in &files.filters {
                dialog = dialog.add_filter(name, endings);
            }
            if let Some(directory) = &files.directory {
                dialog = dialog.set_directory(directory);
            }
            if let Some(name) = &files.file_name {
                dialog = dialog.set_file_name(name);
            }
            match files.choose {
                Choose::File => {
                    let chosen = dialog.pick_file();
                    Box::pin(async move { handle(chosen.await) })
                }
                Choose::Files => {
                    let chosen = dialog.pick_files();
                    Box::pin(async move { handles(chosen.await) })
                }
                Choose::Folder => {
                    let chosen = dialog.pick_folder();
                    Box::pin(async move { handle(chosen.await) })
                }
                Choose::Folders => {
                    let chosen = dialog.pick_folders();
                    Box::pin(async move { handles(chosen.await) })
                }
                Choose::FilesOrFolders => {
                    let chosen = dialog.pick_files_or_folders();
                    Box::pin(async move { handles(chosen.await) })
                }
                Choose::Save => {
                    let chosen = dialog.save_file();
                    Box::pin(async move { handle(chosen.await) })
                }
            }
        }
        Window::Question(question) => {
            let result = rfd::AsyncMessageDialog::new()
                .set_title(&question.title)
                .set_description(&question.words)
                .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                    question.buttons[0].clone(),
                    question.buttons[1].clone(),
                    question.buttons[2].clone(),
                ))
                .set_parent(parent)
                .show();
            Box::pin(async move {
                let result = result.await;
                let [first, second, third] = &question.buttons;
                Answer::Button(pack::three_way(result, [first, second, third]))
            })
        }
    }
}

/// Waits on this thread for what a sheet answers (AppKit calls back on the main thread, which
/// wakes this one).
#[cfg(target_os = "macos")]
fn wait_for<T>(future: impl Future<Output = T>) -> T {
    struct Unpark(std::thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(answer) = future.as_mut().poll(&mut context) {
            return answer;
        }
        std::thread::park();
    }
}

// ---------------------------------------------------------------------------------------------
// What the app asks, and what it does with the answers

/// What a window the app showed is for: what is done with its answer. Each carries what it
/// applies to, as it was when asked, since the operator can go on working while it is open;
/// an answer for something that has gone since is let go.
pub enum For {
    /// Add Audio: tracks for this audio playlist.
    AddAudio {
        playlist: Uuid,
    },
    /// The Bibles window's Install from File….
    BibleFile,
    /// A picture behind the theme open in the theme editor (by its id).
    ThemePicture {
        theme: Uuid,
    },
    /// A picture or video for the slide (or layout) open in the editor (its draft's number).
    SlidePicture {
        draft: Option<u64>,
    },
    /// A folder for this media playlist to follow.
    FollowFolder {
        playlist: Uuid,
    },
    /// The media bin's Add: pictures and videos, for the bin and for the media playlist shown
    /// when Add was clicked (by its id), if any.
    AddMedia {
        playlist: Option<Uuid>,
    },
    /// Whether to put a song pack's videos in it, then where to save it.
    ExportVideos {
        export: Box<pack::Export>,
        file: String,
    },
    ExportTo {
        export: Box<pack::Export>,
    },
    /// What to do with a song pack's songs that are here already (asked once for all of them),
    /// with the command line's job for it, if any.
    PackSame {
        opened: Box<pack::Opened>,
        library: Option<Uuid>,
        job: Option<u64>,
    },
    /// Import › Slides › Pictures.
    ImportPictures,
    /// Import › Slides › A presentation.
    ImportPresentations,
    /// Song files, bundles, words or library folders to import; `setup` when the first-start
    /// setup asked (its page then says how it went).
    ImportSongs {
        setup: bool,
    },
    /// Where a file window takes files or a folder but not both: which, then that window.
    #[cfg(not(target_os = "macos"))]
    LibraryFilesOrFolder(Files),
}

/// What a theme's picture is asked for: the theme open in the theme editor, by its id (its place
/// in the list changes when a theme before it is deleted, or the list is put in another order).
pub fn theme_asked(edit: Option<&crate::home::ThemeEdit>) -> Option<Uuid> {
    edit.map(|edit| edit.theme.id)
}

/// Whether the theme open now is the one a picture was asked for.
fn same_theme(edit: Option<&crate::home::ThemeEdit>, asked: Uuid) -> bool {
    edit.is_some_and(|edit| edit.theme.id == asked)
}

impl Purpose for For {
    fn name(&self) -> &'static str {
        match self {
            For::AddAudio { .. } => "Add Audio",
            For::BibleFile => "Install Bible from File",
            For::ThemePicture { .. } => "Theme Picture",
            For::SlidePicture { .. } => "Slide Picture or Video",
            For::FollowFolder { .. } => "Folder to Follow",
            For::AddMedia { .. } => "Add Media",
            For::ExportVideos { .. } => "Export Songs (videos?)",
            For::ExportTo { .. } => "Export Songs (where?)",
            For::PackSame { .. } => "Import Song Pack (songs here already?)",
            For::ImportPictures => "Import Pictures as Slides",
            For::ImportPresentations => "Import a Presentation",
            For::ImportSongs { .. } => "Import Songs",
            #[cfg(not(target_os = "macos"))]
            For::LibraryFilesOrFolder(_) => "Import a Library Folder (folder or files?)",
        }
    }
}

impl crate::App {
    /// Shows a window the operator asked for (none if one is open: that one is brought forward).
    pub(crate) fn ask(&mut self, purpose: For, window: Window) {
        let Some(running) = self.running.as_ref() else { return };
        let parent: &dyn Parent = &*running.home_window;
        self.dialogs.ask(purpose, window, Some(parent), running.frames);
        running.home_window.request_redraw();
    }

    /// Shows a window that work done in the background needs answered, once none is open.
    pub(crate) fn ask_in_turn(&mut self, purpose: For, window: Window) {
        let Some(running) = self.running.as_ref() else { return };
        let parent: &dyn Parent = &*running.home_window;
        self.dialogs.ask_in_turn(purpose, window, Some(parent), running.frames);
    }

    /// Carries out what a window was answered with.
    pub(crate) fn dialog_answered(&mut self, answered: Answered) {
        let Some(running) = self.running.as_ref() else { return };
        let Some(Closed { purpose, answer }) = self.dialogs.answered(answered, running.frames) else {
            return;
        };
        match purpose {
            For::AddAudio { playlist } => self.add_audio_chosen(playlist, answer.paths()),
            For::BibleFile => {
                if let Some(path) = answer.paths().first() {
                    self.install_bible(path);
                }
            }
            For::ThemePicture { theme } => self.theme_picture_chosen(theme, answer.paths()),
            For::SlidePicture { draft } => self.slide_picture_chosen(draft, answer.paths()),
            For::FollowFolder { playlist } => {
                if let Some(folder) = answer.paths().into_iter().next() {
                    self.follow_folder(playlist, Some(folder));
                }
            }
            For::AddMedia { playlist } => {
                for path in answer.paths() {
                    self.add_dropped_file_to(&path, playlist);
                }
            }
            For::ExportVideos { mut export, file } => match answer.button() {
                Some(0) => self.ask_where_to_save(export, file),
                Some(1) => {
                    export.leave_out_videos();
                    self.ask_where_to_save(export, file);
                }
                _ => {}
            },
            For::ExportTo { export } => {
                if let Some(to) = answer.paths().into_iter().next() {
                    let said = self.write_pack(*export, to);
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.notices.outcome(said);
                    }
                }
            }
            For::PackSame { opened, library, job } => {
                let same = match answer.button() {
                    Some(0) => pack::Same::Replace,
                    Some(1) => pack::Same::KeepBoth,
                    _ => pack::Same::Skip,
                };
                let result = self.apply_pack(*opened, Some(same), library);
                self.pack_finished(job, result, false);
            }
            For::ImportPictures => {
                let pictures: Vec<PathBuf> = answer
                    .paths()
                    .into_iter()
                    .filter(|path| crate::media::is_media(path) && !crate::media::is_video(path))
                    .collect();
                if !pictures.is_empty() {
                    self.start_deck(crate::deck::chosen_pictures(pictures));
                }
            }
            For::ImportPresentations => {
                let files = answer.paths();
                if !files.is_empty() {
                    self.import_slides_asking(&files);
                }
            }
            For::ImportSongs { setup } => {
                let paths = answer.paths();
                if !paths.is_empty() {
                    self.import_songs_chosen(&paths, setup);
                }
            }
            #[cfg(not(target_os = "macos"))]
            For::LibraryFilesOrFolder(files) => match answer.button() {
                Some(0) => self.ask(For::ImportSongs { setup: false }, Window::Files(files.choosing(Choose::Folders))),
                Some(1) => self.ask(For::ImportSongs { setup: false }, Window::Files(files.choosing(Choose::Files))),
                _ => {}
            },
        }
        // On Windows, a file window of the app's own (none owns it) gives the front, as it
        // closes, to the next window in line, which may be a screen's on the projector: the
        // keys pressed next would go there. The operator's window takes it back, if it is
        // still the app's (one of another program in front is left alone). Not in a test copy,
        // which never takes the keyboard.
        #[cfg(windows)]
        if !self.quiet()
            && let Some(running) = self.running.as_ref()
            && let Ok(handle) = winit::raw_window_handle::HasWindowHandle::window_handle(&*running.home_window)
            && let winit::raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw()
            && sp_winapi::front_back_to(handle.hwnd.get())
        {
            applog::event("dialog", "the operator's window took the front back from another of the app's windows");
        }
        // Then a question that waited its turn, unless what was done asked for a window of its
        // own (that one goes first; the question comes once it closes).
        if let Some(running) = self.running.as_ref() {
            let parent: &dyn Parent = &*running.home_window;
            self.dialogs.show_waiting(Some(parent), running.frames);
            running.home_window.request_redraw();
        }
    }

    /// Tracks chosen with Add Audio join the playlist they were chosen for, if it is still
    /// there, each once.
    fn add_audio_chosen(&mut self, playlist: Uuid, chosen: Vec<PathBuf>) {
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        let Some(tracks) =
            state.audio.playlists.iter_mut().find(|list| list.id == playlist).map(|list| &mut list.tracks)
        else {
            return;
        };
        for path in chosen {
            if !tracks.iter().any(|track| track.path == path) {
                tracks.push(crate::audio::Track::of(path));
                state.audio_changed = true;
            }
        }
        // Saved, and the tab told, as for any change asked of the audio.
        self.handle_audio();
    }

    /// A picture chosen for a theme goes behind it, copied into the Images folder, if that theme
    /// is still the one open.
    fn theme_picture_chosen(&mut self, theme: Uuid, chosen: Vec<PathBuf>) {
        let Some(path) = chosen.first() else { return };
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        if !same_theme(state.theme_edit.as_ref(), theme) {
            return;
        }
        match crate::images::import(path) {
            Ok(picture) => {
                if let Some(edit) = state.theme_edit.as_mut() {
                    edit.theme.background_picture = Some(picture.file);
                    edit.changed = true;
                }
            }
            Err(problem) => state.notices.trouble(problem),
        }
    }

    /// A picture or video chosen for a slide goes on it, copied into the Images folder, if that
    /// slide is still the one open in the editor.
    fn slide_picture_chosen(&mut self, draft: Option<u64>, chosen: Vec<PathBuf>) {
        let Some(path) = chosen.first() else { return };
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        if state.editing.as_ref().map(|editing| editing.serial) != draft {
            return;
        }
        match crate::images::import(path) {
            Ok(picture) => state.picture_to_add.push(picture),
            Err(problem) => state.notices.trouble(problem),
        }
    }

    /// Asks where to save a planned song pack.
    pub(crate) fn ask_where_to_save(&mut self, export: Box<pack::Export>, file: String) {
        let title = crate::i18n::t("notices.export.title", "Export Songs");
        let window = Files::new(Choose::Save, title)
            .file_name(file)
            .filter(crate::i18n::t("notices.dialog.filter.pack", "Song pack"), &[pack::ENDING]);
        self.ask(For::ExportTo { export }, Window::Files(window));
    }

    /// Asks what to do with an opened song pack's songs that are here already, once for all of
    /// them; they come in with the answer.
    pub(crate) fn ask_pack_same(
        &mut self,
        opened: pack::Opened,
        library: Option<Uuid>,
        job: Option<u64>,
        already: usize,
    ) {
        let buttons = [
            crate::i18n::t("notices.pack.replace", "Replace").to_owned(),
            crate::i18n::t("notices.pack.keep_both", "Keep Both").to_owned(),
            crate::i18n::t("notices.pack.skip", "Skip").to_owned(),
        ];
        let question = Question {
            title: crate::i18n::t("notices.pack.import_title", "Import Song Pack").to_owned(),
            words: crate::i18n::tn(
                "notices.pack.ask_same",
                already as u64,
                "{n} song in the pack is already here. Replace them with the pack's, keep both, or skip them?",
                "{n} songs in the pack are already here. Replace them with the pack's, keep both, or skip them?",
                &[],
            ),
            buttons,
        };
        self.ask_in_turn(For::PackSame { opened: Box::new(opened), library, job }, Window::Question(question));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::mpsc;

    /// A window shown: its title, and what to call with its answer.
    type Shown = (String, Box<dyn FnOnce(Answer) + Send>);

    /// Shows nothing: keeps what it was asked to show, and the answer to give, until told.
    #[derive(Default, Clone)]
    struct Stand {
        shown: Arc<Mutex<Vec<Shown>>>,
        forward: Arc<Mutex<u32>>,
    }

    impl Stand {
        fn titles(&self) -> Vec<String> {
            self.shown.lock().unwrap().iter().map(|(title, _)| title.clone()).collect()
        }

        /// The person answers the window shown `at`, on a thread of its own, as a real one does.
        fn answer(&self, at: usize, answer: Answer) {
            let (_, done) = self.shown.lock().unwrap().remove(at);
            std::thread::spawn(move || done(answer)).join().unwrap();
        }
    }

    impl Shows for Stand {
        fn show(&mut self, window: Window, _parent: Option<&dyn Parent>, done: Box<dyn FnOnce(Answer) + Send>) {
            self.shown.lock().unwrap().push((window.title().to_owned(), done));
        }

        fn bring_forward(&mut self) {
            *self.forward.lock().unwrap() += 1;
        }
    }

    impl Purpose for &'static str {
        fn name(&self) -> &'static str {
            self
        }
    }

    fn dialogs() -> (Dialogs<&'static str>, Stand, mpsc::Receiver<Answered>) {
        let stand = Stand::default();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let deliver: Deliver = Arc::new(move |answered| {
            let _ = tx.lock().unwrap().send(answered);
        });
        (Dialogs::new(Box::new(stand.clone()), deliver), stand, rx)
    }

    fn add_media() -> Window {
        Window::Files(
            Files::new(Choose::Files, "Add Pictures and Videos").filter("Pictures and Videos", &["jpg", "mp4"]),
        )
    }

    #[test]
    fn an_answer_comes_back_through_the_loop_with_what_it_was_for() {
        let (mut dialogs, stand, answers) = dialogs();
        assert_eq!(dialogs.ask("add media", add_media(), None, 10), Asked::Shown);
        assert!(dialogs.is_open());
        assert_eq!(stand.titles(), ["Add Pictures and Videos"]);
        // The window's thread is free meanwhile: nothing has come back yet.
        assert!(answers.try_recv().is_err());
        let chosen = vec![PathBuf::from("drift.mp4"), PathBuf::from("hills.jpg")];
        stand.answer(0, Answer::Paths(chosen.clone()));
        let answered = answers.recv_timeout(Duration::from_secs(5)).unwrap();
        let closed = dialogs.answered(answered, 25).expect("the window open's answer");
        assert_eq!(closed.purpose, "add media");
        assert_eq!(closed.answer.paths(), chosen);
        assert!(!dialogs.is_open());
    }

    #[test]
    fn asked_twice_while_open_shows_one_window_and_brings_it_forward() {
        let (mut dialogs, stand, answers) = dialogs();
        assert_eq!(dialogs.ask("add media", add_media(), None, 0), Asked::Shown);
        assert_eq!(dialogs.ask("add media", add_media(), None, 3), Asked::Busy);
        let audio = Window::Files(Files::new(Choose::Files, "Add Audio"));
        assert_eq!(dialogs.ask("add audio", audio.clone(), None, 4), Asked::Busy);
        assert_eq!(stand.titles(), ["Add Pictures and Videos"], "never two at once");
        assert_eq!(*stand.forward.lock().unwrap(), 2);
        // Once it is closed, another may open.
        stand.answer(0, Answer::Paths(Vec::new()));
        let closed = dialogs.answered(answers.recv_timeout(Duration::from_secs(5)).unwrap(), 9).unwrap();
        assert_eq!(closed.answer, Answer::Paths(Vec::new()));
        assert_eq!(dialogs.ask("add audio", audio, None, 9), Asked::Shown);
        assert_eq!(stand.titles(), ["Add Audio"]);
    }

    /// A window whose answer asks for the next one (Export's videos question, then where to
    /// save) gets it, even when a question from the background is waiting: that one comes after.
    #[test]
    fn a_follow_up_window_goes_before_a_waiting_question() {
        let (mut dialogs, stand, answers) = dialogs();
        let videos = Window::Question(Question {
            title: "Export Songs".to_owned(),
            words: "Put the videos in the song pack?".to_owned(),
            buttons: ["Include Videos".to_owned(), "Leave Them Out".to_owned(), "Cancel".to_owned()],
        });
        dialogs.ask("export videos", videos, None, 0);
        let pack = Window::Question(Question {
            title: "Import Song Pack".to_owned(),
            words: "2 songs in the pack are already here.".to_owned(),
            buttons: ["Replace".to_owned(), "Keep Both".to_owned(), "Skip".to_owned()],
        });
        assert_eq!(dialogs.ask_in_turn("pack", pack, None, 1), Asked::Waiting);
        stand.answer(0, Answer::Button(Some(0)));
        let closed = dialogs.answered(answers.recv_timeout(Duration::from_secs(5)).unwrap(), 2).unwrap();
        assert_eq!(closed.purpose, "export videos");
        // What the answer is for asks where to save: shown now, not refused.
        let save = Window::Files(Files::new(Choose::Save, "Export Songs"));
        assert_eq!(dialogs.ask("export to", save, None, 2), Asked::Shown, "the save window was refused");
        assert_eq!(stand.titles(), ["Export Songs"]);
        // The question still waits while the save window is open, and comes once it closes.
        dialogs.show_waiting(None, 3);
        assert_eq!(stand.titles(), ["Export Songs"]);
        stand.answer(0, Answer::Paths(vec![PathBuf::from("Sunday.sppack")]));
        let closed = dialogs.answered(answers.recv_timeout(Duration::from_secs(5)).unwrap(), 4).unwrap();
        assert_eq!(closed.purpose, "export to");
        dialogs.show_waiting(None, 4);
        assert_eq!(stand.titles(), ["Import Song Pack"]);
    }

    #[test]
    fn a_question_from_the_background_waits_its_turn() {
        let (mut dialogs, stand, answers) = dialogs();
        dialogs.ask("add media", add_media(), None, 0);
        let question = Window::Question(Question {
            title: "Import Song Pack".to_owned(),
            words: "2 songs in the pack are already here.".to_owned(),
            buttons: ["Replace".to_owned(), "Keep Both".to_owned(), "Skip".to_owned()],
        });
        assert_eq!(dialogs.ask_in_turn("pack", question, None, 1), Asked::Waiting);
        assert_eq!(stand.titles(), ["Add Pictures and Videos"]);
        stand.answer(0, Answer::Paths(vec![PathBuf::from("hills.jpg")]));
        let closed = dialogs.answered(answers.recv_timeout(Duration::from_secs(5)).unwrap(), 2).unwrap();
        assert_eq!(closed.purpose, "add media");
        // The question is shown once what the file window was for is done, and answered in its
        // turn.
        assert!(stand.titles().is_empty());
        dialogs.show_waiting(None, 2);
        assert_eq!(stand.titles(), ["Import Song Pack"]);
        stand.answer(0, Answer::Button(Some(1)));
        let closed = dialogs.answered(answers.recv_timeout(Duration::from_secs(5)).unwrap(), 3).unwrap();
        assert_eq!((closed.purpose, closed.answer.button()), ("pack", Some(1)));
        assert!(!dialogs.is_open());
    }

    /// The system's window code can panic on its thread (rfd's Windows code unwraps a file name
    /// that isn't whole UTF-16): the app is still answered, as for a window cancelled, so the
    /// next window can open.
    #[test]
    fn a_window_whose_thread_panics_still_answers() {
        let (tx, rx) = mpsc::channel();
        let answering = Answering::new(
            Box::new(move |answer| {
                let _ = tx.send(answer);
            }),
            Answer::Paths(Vec::new()),
        );
        on_its_own_thread(answering, || panic!("a file name that isn't whole UTF-16"));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(Answer::Paths(Vec::new())));
    }

    /// The app goes on while the window is open: the theme open in the editor at the answer may
    /// be another one at the same place in the list (the first deleted, or the list reordered),
    /// or the same one at another place.
    #[test]
    fn a_themes_picture_goes_to_that_theme_wherever_it_is_in_the_list() {
        use crate::home::ThemeEdit;
        let edit = |index, theme: &sp_model::Theme| ThemeEdit {
            index,
            theme: theme.clone(),
            version: None,
            template: 0,
            changed: false,
            leave: false,
        };
        let (mut one, mut two) = (sp_model::Theme::builtin(), sp_model::Theme::builtin());
        one.id = Uuid::from_u128(1);
        two.id = Uuid::from_u128(2);
        let asked = theme_asked(Some(&edit(2, &one))).unwrap();
        assert!(!same_theme(Some(&edit(2, &two)), asked), "another theme now at that place");
        assert!(same_theme(Some(&edit(1, &one)), asked), "the same theme, moved");
        assert!(!same_theme(None, asked), "the editor closed");
    }

    #[test]
    fn an_answer_for_no_window_open_is_let_go() {
        let (mut dialogs, _stand, _answers) = dialogs();
        assert!(dialogs.answered(Answered { id: 7, answer: Answer::Button(Some(0)) }, 0).is_none());
    }

    #[test]
    fn the_log_says_how_many_were_chosen_never_which() {
        assert_eq!(Answer::Paths(Vec::new()).said(), "cancelled");
        assert_eq!(Answer::Paths(vec![PathBuf::from("/secret/name.mp4")]).said(), "1 file chosen");
        assert_eq!(Answer::Paths(vec![PathBuf::from("a"), PathBuf::from("b")]).said(), "2 files chosen");
        assert_eq!(Answer::Button(None).said(), "closed");
        assert_eq!(Answer::Button(Some(0)).said(), "button 1 pressed");
    }
}
