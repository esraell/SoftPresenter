//! The library's files, written on a thread of their own. Each song is a file, and on some
//! computers every new file is slow to write (Windows Defender looks at each one, the disk is
//! busy, the library is on a network folder): the 2,001 songs of a free songbook took minutes,
//! and the window answered nothing meanwhile. Now the main thread only hands each file over;
//! this thread writes them, one at a time, in the order they were handed over:
//!
//! - **Whole or not at all:** each file is written under a name of its own beside it
//!   (`.<name>.saving` for a song, `<name>.part` for the rest, which the library tidy knows as
//!   half-written), flushed to the disk and then renamed into place (`whole_file::replace`), so a
//!   power cut leaves the old file or the new one, never an empty or half-written one. The folder
//!   is flushed once a run of files into it is written (a big import's songs flush the Songs
//!   folder once, not once a song), before anything else is written.
//! - **In order:** a song is on disk before a playlist or library handed over after it, so a
//!   crash never leaves a playlist naming a song that has no file. A save of a file whose last
//!   save is still waiting at the end of the queue takes that save's place, so a song saved
//!   again and again while the disk is busy is written once; with anything handed over in
//!   between, both are written, in order.
//! - **Never over a change made outside the app:** a save carries how the app last saw the file
//!   ([`Guard`]), and is left undone, and said, if the file has changed since.
//! - **Never over a settings file the start couldn't read** and couldn't keep aside
//!   (`whole_file::refuses`): it is left as it was until the app starts again ([`Done::NotRead`]).
//! - **At quit** the app waits until everything handed over is written. If it is killed, what
//!   was still waiting is lost, as a save under way would have been before.
//!
//! What happened comes back as [`Done`]s, which the main thread takes with
//! [`Writer::finished`]; the thread wakes it when a save fails or the queue empties.
//!
//! Themes and the note of the theme in use are written here too, as songs are. The small
//! settings files (the preferences, the screens, the audio list, macros, timers…) are
//! written here too, so the window never waits for the disk: flushing one takes about 11 ms on an
//! Apple silicon Mac, and a slider dragged changes the settings at every step. [`Paced`] hands
//! each one over at most every [`PACE`]: the first change at once, the last always, and those
//! between them not at all.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::outside::{self, Stamp};

/// What a file is, which says how its half-written copy is named and what the window says if it
/// can't be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Song,
    Playlists,
    Libraries,
    MediaList,
    /// A theme's file in the Themes folder.
    Theme,
    /// The note of the theme in use and the order of the list (`themes-in-use.txt`).
    ThemesInUse,
    /// One of the small settings files.
    Settings(crate::whole_file::What),
}

impl Kind {
    /// The name the file is written under before it is renamed into place.
    fn temporary(self, path: &Path) -> PathBuf {
        let name = path.file_name().map_or_else(|| "file".into(), |name| name.to_string_lossy().into_owned());
        match self {
            Kind::Song => path.with_file_name(format!(".{name}.saving")),
            Kind::Playlists
            | Kind::Libraries
            | Kind::MediaList
            | Kind::Theme
            | Kind::ThemesInUse
            | Kind::Settings(_) => path.with_file_name(format!("{name}.part")),
        }
    }

    /// What the window says when a file of this kind couldn't be written.
    pub fn problem(self, path: &Path, error: &std::io::Error) -> String {
        match self {
            Kind::Song => crate::i18n::tf(
                "notices.song.change_not_saved",
                "The change was not saved. {error}",
                &[("error", &crate::load::not_saved(path, error))],
            ),
            Kind::Playlists => crate::service::not_saved(error),
            Kind::Libraries => crate::libraries::not_saved(error),
            Kind::MediaList => crate::media::list_not_saved(error),
            Kind::Theme => crate::theme_store::not_saved(error),
            Kind::ThemesInUse => crate::theme_store::in_use_not_saved(error),
            Kind::Settings(what) => what.not_saved(error),
        }
    }
}

/// When a save may go ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    /// Over whatever is there (Keep mine, a settings file).
    Any,
    /// Only if the file is still as this says (`None`: there was none).
    Unchanged(Option<Stamp>),
    /// Only if the file is as this writer's last save of it left it: a save handed over while
    /// another of the same file was still on its way.
    AsLeft,
}

/// The bytes a file holds, made on the writer's thread (a song is turned into its file there,
/// not on the main thread).
pub type Content = Box<dyn FnOnce() -> Vec<u8> + Send>;

/// A file to write.
pub struct Save {
    pub path: PathBuf,
    pub kind: Kind,
    pub guard: Guard,
    /// Make the folder it goes in, if it isn't there (the app's own folders only: a song kept on
    /// a drive that is unplugged must not be saved into a folder made for it on another disk).
    pub make_folder: bool,
    /// A folder the file as it was is copied into first (the library tidy's backup); the save is
    /// left undone if the copy can't be made.
    pub keep_old_in: Option<PathBuf>,
    pub content: Content,
}

impl Save {
    pub fn new(path: PathBuf, kind: Kind, guard: Guard, content: impl FnOnce() -> Vec<u8> + Send + 'static) -> Save {
        Save { path, kind, guard, make_folder: false, keep_old_in: None, content: Box::new(content) }
    }
}

/// What became of something handed over.
#[derive(Debug)]
pub enum Done {
    /// Written; how the file is, as this thread found it straight after.
    Written { path: PathBuf, seq: u64, stamp: Option<Stamp> },
    /// Left undone: the file changed outside the app since it was last read or written.
    ChangedOutside { path: PathBuf, seq: u64 },
    /// Left undone: a settings file there at the start that could be neither read nor kept
    /// aside, never saved over this run (`whole_file::refuses`).
    NotRead { path: PathBuf, seq: u64, kind: Kind },
    /// Couldn't be written. `keep_old_in` is the folder the file as it was was to be copied
    /// into first (the library tidy's backup), for a save tried again to keep it too.
    Failed { path: PathBuf, seq: u64, kind: Kind, error: std::io::Error, keep_old_in: Option<PathBuf> },
    /// A job handed over with [`Writer::then`] went wrong; what to tell the operator.
    Problem(String),
}

type Job = Box<dyn FnOnce() -> Result<(), String> + Send>;

enum Task {
    Save { seq: u64, save: Save },
    Job(Job),
}

#[derive(Default)]
struct State {
    queue: VecDeque<Task>,
    /// Something is being done now, out of the queue: the path, for a save.
    busy: Option<Option<PathBuf>>,
    done: Vec<Done>,
    /// The number given to the last save handed over.
    seq: u64,
    closing: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Told whenever the queue or what is being done changes.
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn wait<'a>(&self, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The thread that writes the library's files, and the way to it.
pub struct Writer {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Writer {
    /// Starts the thread. `wake` is called (on it) when a save fails or the queue empties, so the
    /// main thread takes what happened.
    pub fn start(wake: impl Fn() + Send + 'static) -> Writer {
        // Testing aid: SOFTPRESENTER_SLOW_WRITES=<ms> makes each file take that much longer to
        // write, as on a computer whose disk or virus scanner is slow (Windows Defender looks at
        // every new file). Only for a copy with its own data folder.
        let own_folder = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
        let slow = std::env::var("SOFTPRESENTER_SLOW_WRITES")
            .ok()
            .and_then(|ms| ms.trim().parse::<u64>().ok())
            .filter(|_| own_folder)
            .map(|ms| Duration::from_millis(ms.min(60_000)));
        Writer::spawn(Box::new(wake), slow)
    }

    fn spawn(wake: Box<dyn Fn() + Send>, slow: Option<Duration>) -> Writer {
        let shared = Arc::new(Shared::default());
        let working = shared.clone();
        let thread = std::thread::Builder::new()
            .name("library writer".to_owned())
            .spawn(move || work(&working, &*wake, slow))
            .map_err(|e| crate::applog::problem("saving", &format!("could not start the library's writer: {e}")))
            .ok();
        Writer { shared, thread }
    }

    /// Hands a file over to be written, and returns the number it was given. A save of the same
    /// file waiting last in the queue is replaced by this one, which takes its number's place
    /// (and, if that save went over anything, or kept the old file, so does this one), unless
    /// this one may only go over the file as the app last saw it ([`Guard::Unchanged`]): the
    /// save waiting isn't the app's own then (a song pack's version written over a song in use),
    /// and this one must find what that one leaves, and be left undone.
    pub fn write(&self, mut save: Save) -> u64 {
        let mut state = self.shared.lock();
        state.seq += 1;
        let seq = state.seq;
        if self.thread.is_none() {
            // No thread to write it (it couldn't be started): written here, as before there was one.
            drop(state);
            let folder = save.path.parent().map(Path::to_path_buf);
            let done = write_one(seq, save, &mut HashMap::new(), None);
            if let Some(folder) = folder.filter(|_| matches!(done, Done::Written { .. })) {
                crate::whole_file::flush_folder(&folder);
            }
            self.shared.lock().done.push(done);
            return seq;
        }
        match state.queue.back_mut() {
            Some(Task::Save { seq: last_seq, save: last })
                if last.path == save.path && !matches!(save.guard, Guard::Unchanged(_)) =>
            {
                if save.guard != Guard::Any {
                    save.guard = last.guard;
                }
                save.keep_old_in = save.keep_old_in.take().or(last.keep_old_in.take());
                save.make_folder |= last.make_folder;
                *last_seq = seq;
                *last = save;
            }
            _ => state.queue.push_back(Task::Save { seq, save }),
        }
        self.shared.changed.notify_all();
        seq
    }

    /// Hands over a job to be done once everything handed over before it is written (moving a
    /// deleted song's file to the Trash: after its last save, never before). What it returns
    /// wrong is told to the operator.
    pub fn then(&self, job: impl FnOnce() -> Result<(), String> + Send + 'static) {
        if self.thread.is_none() {
            if let Err(problem) = job() {
                self.shared.lock().done.push(Done::Problem(problem));
            }
            return;
        }
        self.shared.lock().queue.push_back(Task::Job(Box::new(job)));
        self.shared.changed.notify_all();
    }

    /// What became of what was handed over, since this was last asked.
    pub fn finished(&self) -> Vec<Done> {
        std::mem::take(&mut self.shared.lock().done)
    }

    /// How many files and jobs are waiting or being done.
    pub fn left(&self) -> usize {
        let state = self.shared.lock();
        state.queue.len() + usize::from(state.busy.is_some())
    }

    pub fn is_idle(&self) -> bool {
        self.left() == 0
    }

    /// Waits until everything handed over is done.
    pub fn drain(&self) {
        let _stall = crate::stall::inside(crate::stall::Doing::Saving);
        let mut state = self.shared.lock();
        while self.thread.is_some() && (!state.queue.is_empty() || state.busy.is_some()) {
            state = self.shared.wait(state);
        }
    }

    /// Writes a file's waiting saves before anything else, and waits for them (and for one of it
    /// being written now): for reading the file straight after.
    pub fn flush(&self, path: &Path) {
        let _stall = crate::stall::inside(crate::stall::Doing::Saving);
        let mut state = self.shared.lock();
        let names = |task: &Task| matches!(task, Task::Save { save, .. } if save.path == path);
        let (mut first, rest): (VecDeque<Task>, VecDeque<Task>) = state.queue.drain(..).partition(names);
        first.extend(rest);
        state.queue = first;
        self.shared.changed.notify_all();
        while self.thread.is_some()
            && (state.queue.iter().any(names) || state.busy.as_ref().is_some_and(|busy| busy.as_deref() == Some(path)))
        {
            state = self.shared.wait(state);
        }
    }
}

/// How far apart [`Paced`] hands over saves of one settings file at most.
pub const PACE: Duration = Duration::from_millis(300);

/// Saves of the small settings files, each handed to the [`Writer`] at most once every [`PACE`].
/// The first change of a file goes at once; a change within [`PACE`] of the last save handed over
/// waits, taking the place of any waiting before it, and goes when its time comes
/// ([`Paced::tick`], which says when that is). At quit, [`Paced::flush`] hands over everything
/// waiting.
#[derive(Default)]
pub struct Paced {
    /// When each file's last save was handed over.
    last: HashMap<PathBuf, Instant>,
    /// The newest save of each file waiting for its time.
    waiting: HashMap<PathBuf, Save>,
}

impl Paced {
    /// A settings file changed: its save is handed over now, or waits.
    pub fn save(&mut self, writer: &Writer, save: Save, now: Instant) {
        if self.last.get(&save.path).is_some_and(|last| now < *last + PACE) {
            self.waiting.insert(save.path.clone(), save);
            return;
        }
        self.waiting.remove(&save.path);
        self.last.insert(save.path.clone(), now);
        writer.write(save);
    }

    /// Hands over the saves whose time has come, and says when the next one waiting is due.
    pub fn tick(&mut self, writer: &Writer, now: Instant) -> Option<Instant> {
        let due = |last: Option<&Instant>| last.map(|last| *last + PACE);
        let ready: Vec<PathBuf> =
            self.waiting.keys().filter(|path| due(self.last.get(*path)).is_none_or(|at| at <= now)).cloned().collect();
        for path in ready {
            if let Some(save) = self.waiting.remove(&path) {
                self.last.insert(path, now);
                writer.write(save);
            }
        }
        self.waiting.keys().filter_map(|path| due(self.last.get(path))).min()
    }

    /// Hands over every save waiting, whatever the time: at quit, before the writer is waited for.
    pub fn flush(&mut self, writer: &Writer) {
        let now = Instant::now();
        for (path, save) in std::mem::take(&mut self.waiting) {
            self.last.insert(path, now);
            writer.write(save);
        }
    }
}

/// The longest wait between two tries of a save that keeps failing ([`Retries`]).
pub const RETRY_MOST: Duration = Duration::from_secs(60);

/// How long a save that has failed `tries` times waits before it is tried again: a second, then
/// twice as long each time, up to [`RETRY_MOST`].
pub fn retry_after(tries: u32) -> Duration {
    Duration::from_secs(1u64 << tries.saturating_sub(1).min(6)).min(RETRY_MOST)
}

/// The files whose save failed (songs, themes, settings, playlists, libraries, the media bin and
/// the note of the theme in use), each tried again after [`retry_after`] until a save of it is
/// written: a file another program held for a moment (a virus scanner, a sync tool, the search
/// indexer) is saved once it lets go, and the operator is told once, not at every try. What is
/// still not saved at quit is kept in the library's "Not saved" folder (`main.rs`).
#[derive(Default)]
pub struct Retries {
    files: HashMap<PathBuf, Retry>,
}

/// A file whose save failed: how many saves of it have failed in a row, when the next try is due
/// (`None` while one is on its way), what it is (which says how the app makes its save again),
/// and the folder the file as it was is to be copied into first (the library tidy's backup),
/// which every try keeps.
struct Retry {
    tries: u32,
    due: Option<Instant>,
    kind: Kind,
    keep_old_in: Option<PathBuf>,
}

/// A file whose next try is due ([`Retries::due`]).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Due {
    pub path: PathBuf,
    pub kind: Kind,
    pub keep_old_in: Option<PathBuf>,
}

impl Retries {
    /// A save of `path`, of this `kind`, failed at `now`: it is tried again later, keeping the
    /// file as it was in `keep_old_in` first if that save was to (and if an earlier one of the
    /// run was). True the first time in a row, to tell the operator.
    pub fn failed(&mut self, path: &Path, now: Instant, kind: Kind, keep_old_in: Option<PathBuf>) -> bool {
        let retry =
            self.files.entry(path.to_path_buf()).or_insert(Retry { tries: 0, due: None, kind, keep_old_in: None });
        retry.tries += 1;
        retry.due = Some(now + retry_after(retry.tries));
        retry.kind = kind;
        if keep_old_in.is_some() {
            retry.keep_old_in = keep_old_in;
        }
        retry.tries == 1
    }

    /// A save of `path` was written, or it is no longer the app's to write (deleted, read again
    /// from its file, held for a change made outside): no more tries. How many saves of it had
    /// failed, if any.
    pub fn done(&mut self, path: &Path) -> Option<u32> {
        self.files.remove(path).map(|retry| retry.tries)
    }

    /// The files whose next try is due at `now` (all of them, `at_once`, as a quit begins); each
    /// is counted as on its way until it is written or fails again.
    pub fn due(&mut self, now: Instant, at_once: bool) -> Vec<Due> {
        let mut due = Vec::new();
        for (path, retry) in &mut self.files {
            if retry.due.is_some_and(|at| at_once || at <= now) {
                retry.due = None;
                due.push(Due { path: path.clone(), kind: retry.kind, keep_old_in: retry.keep_old_in.clone() });
            }
        }
        due.sort();
        due
    }

    /// When the next try is due.
    pub fn next(&self) -> Option<Instant> {
        self.files.values().filter_map(|retry| retry.due).min()
    }
}

impl Drop for Writer {
    /// What is still waiting is written before the thread ends, and the app with it.
    fn drop(&mut self) {
        self.shared.lock().closing = true;
        self.shared.changed.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// How this thread's last save of a file left it, for a save of it handed over meanwhile
/// ([`Guard::AsLeft`]).
enum Left {
    As(Option<Stamp>),
    /// Left undone because the file changed outside: a later save of it waits on the operator too.
    Refused,
}

fn work(shared: &Shared, wake: &dyn Fn(), slow: Option<Duration>) {
    let mut left: HashMap<PathBuf, Left> = HashMap::new();
    // A folder files were renamed into whose list isn't flushed to the disk yet: flushed once
    // the next thing to do isn't another file into it, while this thread is still busy (so
    // quitting waits for it too).
    let mut unflushed: Option<PathBuf> = None;
    loop {
        let task = {
            let mut state = shared.lock();
            loop {
                if let Some(task) = state.queue.pop_front() {
                    state.busy = Some(match &task {
                        Task::Save { save, .. } => Some(save.path.clone()),
                        Task::Job(_) => None,
                    });
                    break task;
                }
                if state.closing {
                    return;
                }
                state = shared.wait(state);
            }
        };
        let done = match task {
            Task::Save { seq, save } => {
                let folder = save.path.parent().map(Path::to_path_buf);
                let done = write_one(seq, save, &mut left, slow);
                if matches!(done, Done::Written { .. }) && folder != unflushed {
                    if let Some(other) = unflushed.take() {
                        crate::whole_file::flush_folder(&other);
                    }
                    unflushed = folder;
                }
                Some(done)
            }
            Task::Job(job) => job().err().map(Done::Problem),
        };
        if let Some(folder) = &unflushed {
            let more_into_it = matches!(
                shared.lock().queue.front(),
                Some(Task::Save { save, .. }) if save.path.parent() == Some(folder.as_path())
            );
            if !more_into_it {
                crate::whole_file::flush_folder(folder);
                unflushed = None;
            }
        }
        let tell = {
            let mut state = shared.lock();
            state.busy = None;
            let failed = done.as_ref().is_some_and(|done| !matches!(done, Done::Written { .. }));
            state.done.extend(done);
            shared.changed.notify_all();
            failed || state.queue.is_empty()
        };
        if tell {
            wake();
        }
    }
}

/// Writes one file, unless its guard says it changed outside the app.
fn write_one(seq: u64, save: Save, left: &mut HashMap<PathBuf, Left>, slow: Option<Duration>) -> Done {
    let Save { path, kind, guard, make_folder, keep_old_in, content } = save;
    if crate::whole_file::refuses(&path) {
        crate::applog::problem(
            "saving",
            &format!("did not write {}: it couldn't be read at the start", path.display()),
        );
        return Done::NotRead { path, seq, kind };
    }
    let expected = match guard {
        Guard::Any => None,
        Guard::Unchanged(then) => Some(then),
        Guard::AsLeft => match left.get(&path) {
            Some(Left::As(then)) => Some(*then),
            Some(Left::Refused) => return Done::ChangedOutside { path, seq },
            None => None,
        },
    };
    if expected.is_some_and(|then| outside::stamp(&path) != then) {
        left.insert(path.clone(), Left::Refused);
        return Done::ChangedOutside { path, seq };
    }
    if let Some(slow) = slow {
        std::thread::sleep(slow);
    }
    let bytes = content();
    let temporary = kind.temporary(&path);
    let written = (|| {
        if let Some(folder) = &keep_old_in
            && path.is_file()
        {
            keep_copy(&path, folder)?;
        }
        if make_folder && let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        crate::whole_file::replace(&path, &temporary, &bytes)
    })();
    let stamp = outside::stamp(&path);
    left.insert(path.clone(), Left::As(stamp));
    match written {
        Ok(()) => Done::Written { path, seq, stamp },
        Err(error) => {
            // `replace` took its half-written file away.
            crate::applog::problem("saving", &format!("could not write {}: {error}", path.display()));
            Done::Failed { path, seq, kind, error, keep_old_in }
        }
    }
}

/// Copies a file into a folder (made if need be), never over a file there: `song 2.sppres` when
/// `song.sppres` is there already.
fn keep_copy(file: &Path, folder: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let name = file.file_name().map_or_else(|| "file".into(), |name| name.to_string_lossy().into_owned());
    let (stem, ending) = name.rsplit_once('.').unwrap_or((name.as_str(), ""));
    let target = std::iter::once(folder.join(&name))
        .chain((2..).map(|n| folder.join(format!("{stem} {n}.{ending}"))))
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .ok_or_else(|| std::io::Error::other("no free name"))?;
    std::fs::copy(file, &target).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("softpresenter-writer-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn quiet() -> Writer {
        Writer::spawn(Box::new(|| {}), None)
    }

    #[test]
    fn a_burst_of_settings_changes_is_written_a_few_times_and_the_last_always() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let dir = TempDir::new("paced");
        let path = dir.0.join("preferences.json");
        let writer = quiet();
        let written = Arc::new(AtomicUsize::new(0));
        let save = |n: u64| {
            let written = written.clone();
            Save::new(path.clone(), Kind::Settings(crate::whole_file::What::Settings), Guard::Any, move || {
                written.fetch_add(1, SeqCst);
                n.to_string().into_bytes()
            })
        };
        let on_disk = || std::fs::read_to_string(&path).unwrap_or_default();
        let mut paced = Paced::default();
        // Thirty changes in a second, as a slider dragged makes them, the loop looking at the
        // clock after each, and the writer keeping up (so each save handed over is a file
        // written, not one the writer's queue let a later one take the place of).
        let t0 = Instant::now();
        for n in 0..30 {
            let now = t0 + Duration::from_millis(n * 1000 / 30);
            paced.save(&writer, save(n), now);
            assert!(paced.tick(&writer, now).is_none_or(|due| due > now), "nothing due is left waiting");
            writer.drain();
        }
        // The last change goes once its time comes, and then nothing waits.
        let end = t0 + Duration::from_secs(1);
        let due = paced.tick(&writer, end).expect("the last change waits for its time");
        assert!(due <= end + PACE);
        assert_eq!(paced.tick(&writer, due), None);
        writer.drain();
        assert_eq!(on_disk(), "29", "the last change is on disk");
        let times = written.load(SeqCst);
        let most = 1 + (1000 / PACE.as_millis()) as usize + 1;
        assert!((2..=most).contains(&times), "written {times} times, not at most {most}");
        // At quit, a change waiting for its time is handed over at once.
        paced.save(&writer, save(30), due + Duration::from_millis(1));
        assert_eq!(on_disk(), "29", "it waits");
        paced.flush(&writer);
        writer.drain();
        assert_eq!(on_disk(), "30", "on disk at quit");
        assert_eq!(paced.tick(&writer, Instant::now()), None);
    }

    #[test]
    fn a_failed_save_is_tried_again_later_and_later_until_one_is_written() {
        // The waits: a second, then twice as long each time, never more than a minute.
        let waits: Vec<u64> = (1..=9).map(|tries| retry_after(tries).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(retry_after(u32::MAX), RETRY_MOST);
        let (song, theme) = (Path::new("/l/Songs/a.sppres"), Path::new("/l/Themes/b.sptheme"));
        let mut retries = Retries::default();
        let t0 = Instant::now();
        let backup = PathBuf::from("/l/Backup before tidying 2026-11-27");
        let due =
            |path: &Path, kind: Kind, keep_old_in: Option<PathBuf>| Due { path: path.to_path_buf(), kind, keep_old_in };
        assert!(retries.failed(song, t0, Kind::Song, Some(backup.clone())), "the first failure is told");
        assert_eq!(retries.next(), Some(t0 + Duration::from_secs(1)));
        assert!(retries.due(t0, false).is_empty(), "not yet");
        let at = t0 + Duration::from_secs(1);
        assert_eq!(
            retries.due(at, false),
            [due(song, Kind::Song, Some(backup.clone()))],
            "tried again after a second, keeping the old file where the first save was to"
        );
        assert_eq!(retries.next(), None, "on its way, nothing else is due");
        assert!(retries.due(at + Duration::from_secs(5), false).is_empty(), "never twice at once");
        // It fails again: not told again, and it waits twice as long.
        assert!(!retries.failed(song, at, Kind::Song, None));
        assert_eq!(retries.next(), Some(at + Duration::from_secs(2)));
        // Another file fails meanwhile; a quit tries every file at once, whatever its wait.
        // A settings file too, which the app makes its save of again from what it has then.
        let timers = Path::new("/l/timers.json");
        let what = Kind::Settings(crate::whole_file::What::Timers);
        assert!(retries.failed(theme, at, Kind::Theme, None));
        assert!(retries.failed(timers, at, what, None));
        assert_eq!(
            retries.due(at, true),
            [due(song, Kind::Song, Some(backup)), due(theme, Kind::Theme, None), due(timers, what, None)],
            "the backup folder kept across tries, and each file's kind"
        );
        // Written at last: no more tries, and how many failed is said.
        assert_eq!(retries.done(song), Some(2));
        assert_eq!(retries.done(song), None);
        // Failing again later starts from a second, and is told again.
        assert!(retries.failed(song, at, Kind::Song, None));
        assert_eq!(retries.next(), Some(at + Duration::from_secs(1)));
        assert_eq!(retries.due(at + Duration::from_secs(1), false)[0], due(song, Kind::Song, None));
    }

    fn save(path: &Path, text: &str, guard: Guard) -> Save {
        let text = text.to_owned();
        Save::new(path.to_owned(), Kind::Song, guard, move || text.into_bytes())
    }

    /// Holds the writer at a job until the sender is dropped or sends, so what is handed over
    /// meanwhile waits in the queue.
    fn hold(writer: &Writer) -> mpsc::Sender<()> {
        let (go, wait) = mpsc::channel::<()>();
        let (held, is_held) = mpsc::channel::<()>();
        writer.then(move || {
            let _ = held.send(());
            let _ = wait.recv();
            Ok(())
        });
        is_held.recv().unwrap();
        go
    }

    fn written(done: &[Done]) -> Vec<(String, u64)> {
        done.iter()
            .filter_map(|done| match done {
                Done::Written { path, seq, .. } => Some((path.file_name()?.to_string_lossy().into_owned(), *seq)),
                _ => None,
            })
            .collect()
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn files_are_written_whole_in_the_order_they_were_handed_over() {
        let dir = TempDir::new("order");
        let writer = quiet();
        let go = hold(&writer);
        let a = writer.write(save(&dir.0.join("a.sppres"), "A", Guard::Unchanged(None)));
        let playlists = dir.0.join("services.json");
        let p = writer.write(Save::new(playlists.clone(), Kind::Playlists, Guard::Any, || b"P".to_vec()));
        let b = writer.write(save(&dir.0.join("b.sppres"), "B", Guard::Any));
        assert_eq!(writer.left(), 4, "three saves wait behind the held job");
        drop(go);
        writer.drain();
        assert!(writer.is_idle());
        let done = writer.finished();
        assert_eq!(
            written(&done),
            [("a.sppres".to_owned(), a), ("services.json".to_owned(), p), ("b.sppres".to_owned(), b)]
        );
        assert_eq!(std::fs::read_to_string(&playlists).unwrap(), "P");
        // Nothing half-written is left beside them.
        assert_eq!(files_in(&dir.0), ["a.sppres", "b.sppres", "services.json"]);
        // A half-written song is named as the library tidy knows it, and so is a settings file.
        assert_eq!(Kind::Song.temporary(Path::new("/l/Songs/a.sppres")), Path::new("/l/Songs/.a.sppres.saving"));
        assert_eq!(Kind::Playlists.temporary(Path::new("/l/services.json")), Path::new("/l/services.json.part"));
    }

    #[test]
    fn a_file_saved_again_while_its_save_waits_last_is_written_once_and_last_wins() {
        let dir = TempDir::new("coalesce");
        let (a, b) = (dir.0.join("a.sppres"), dir.0.join("b.sppres"));
        let writer = quiet();
        let go = hold(&writer);
        writer.write(save(&a, "one", Guard::Any));
        let second = writer.write(save(&a, "two", Guard::Any));
        assert_eq!(writer.left(), 2, "the second took the first's place");
        drop(go);
        writer.drain();
        assert_eq!(written(&writer.finished()), [("a.sppres".to_owned(), second)]);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "two");
        // With another file between them, both are written, in order: the other file never
        // lands before the first save it followed.
        let go = hold(&writer);
        let one = writer.write(save(&a, "three", Guard::Any));
        let other = writer.write(save(&b, "B", Guard::Any));
        let four = writer.write(save(&a, "four", Guard::Any));
        drop(go);
        writer.drain();
        assert_eq!(
            written(&writer.finished()),
            [("a.sppres".to_owned(), one), ("b.sppres".to_owned(), other), ("a.sppres".to_owned(), four)]
        );
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "four");
    }

    #[test]
    fn a_job_handed_over_after_a_save_finds_it_written() {
        // As a deleted song's file goes to the Trash after its last save, never before (when the
        // save would make the file again afterwards).
        let dir = TempDir::new("after");
        let a = dir.0.join("a.sppres");
        let writer = quiet();
        let go = hold(&writer);
        writer.write(save(&a, "new song", Guard::Unchanged(None)));
        let (seen_tx, seen) = mpsc::channel();
        let gone = a.clone();
        writer.then(move || {
            let _ = seen_tx.send(std::fs::read_to_string(&gone).ok());
            std::fs::remove_file(&gone).map_err(|e| e.to_string())
        });
        writer.then(|| Err("the second job says so".to_owned()));
        drop(go);
        writer.drain();
        assert_eq!(seen.recv().unwrap().as_deref(), Some("new song"));
        assert!(!a.exists(), "the job came after the save");
        let done = writer.finished();
        assert!(matches!(done.last(), Some(Done::Problem(said)) if said == "the second job says so"), "{done:?}");
    }

    #[test]
    fn everything_handed_over_is_written_before_the_app_closes() {
        let dir = TempDir::new("drain");
        let writer = Writer::spawn(Box::new(|| {}), Some(Duration::from_millis(5)));
        for n in 0..20 {
            writer.write(save(&dir.0.join(format!("{n:02}.sppres")), &n.to_string(), Guard::Any));
        }
        assert!(writer.left() > 0, "slow writes are still waiting");
        // Quitting waits (`drain`); a writer let go writes what is left before it goes.
        drop(writer);
        assert_eq!(files_in(&dir.0).len(), 20);
        assert_eq!(std::fs::read_to_string(dir.0.join("19.sppres")).unwrap(), "19");
        let writer = Writer::spawn(Box::new(|| {}), Some(Duration::from_millis(5)));
        for n in 20..30 {
            writer.write(save(&dir.0.join(format!("{n:02}.sppres")), "x", Guard::Any));
        }
        writer.drain();
        assert!(writer.is_idle());
        assert_eq!(files_in(&dir.0).len(), 30);
    }

    #[test]
    fn a_file_that_cannot_be_written_is_said_and_the_rest_still_are() {
        let dir = TempDir::new("failed");
        let (woken_tx, woken) = mpsc::channel();
        let writer = Writer::spawn(
            Box::new(move || {
                let _ = woken_tx.send(());
            }),
            None,
        );
        let nowhere = dir.0.join("gone").join("song.sppres");
        let bad = writer.write(save(&nowhere, "x", Guard::Any));
        let fine = dir.0.join("fine.sppres");
        writer.write(save(&fine, "fine", Guard::Any));
        writer.drain();
        woken.recv_timeout(Duration::from_secs(5)).expect("the main thread is woken");
        let done = writer.finished();
        let failed = done.iter().find_map(|done| match done {
            Done::Failed { path, seq, kind, error, .. } => Some((path.clone(), *seq, *kind, error.kind())),
            _ => None,
        });
        assert_eq!(failed, Some((nowhere.clone(), bad, Kind::Song, std::io::ErrorKind::NotFound)));
        assert_eq!(std::fs::read_to_string(&fine).unwrap(), "fine");
        assert_eq!(files_in(&dir.0), ["fine.sppres"], "no half-written file is left");
        // A folder of the app's own is made for its file.
        let mut made = save(&nowhere, "made", Guard::Any);
        made.make_folder = true;
        writer.write(made);
        writer.drain();
        assert_eq!(std::fs::read_to_string(&nowhere).unwrap(), "made");
    }

    /// What a quit leans on (`App::quit`): once the writer says it is idle, everything it did is
    /// there to take, a save that failed as the last one ended included, never handed over a
    /// moment after.
    #[test]
    fn once_the_writer_is_idle_a_save_that_failed_last_is_there_to_take() {
        let dir = TempDir::new("idle");
        let writer = quiet();
        for n in 0..200 {
            writer.write(save(&dir.0.join(format!("{n}.sppres")), "fine", Guard::Any));
            writer.write(save(&dir.0.join("gone").join(format!("{n}.sppres")), "x", Guard::Any));
            while !writer.is_idle() {
                std::thread::yield_now();
            }
            let done = writer.finished();
            assert!(
                matches!(done.as_slice(), [.., Done::Failed { .. }]),
                "try {n}: the failure is there once the writer is idle: {done:?}"
            );
        }
    }

    #[test]
    fn a_save_kept_from_a_file_changed_outside_waits_on_the_operator() {
        let dir = TempDir::new("outside");
        let a = dir.0.join("a.sppres");
        std::fs::write(&a, "read").unwrap();
        let seen = outside::stamp(&a);
        let writer = quiet();
        let go = hold(&writer);
        let first = writer.write(save(&a, "mine", Guard::Unchanged(seen)));
        // Another save of it, with another file between (so not merged): it follows the first.
        writer.write(save(&dir.0.join("b.sppres"), "b", Guard::Any));
        let second = writer.write(save(&a, "mine again", Guard::AsLeft));
        // Someone changes the file while the saves wait.
        std::fs::write(&a, "changed outside, longer").unwrap();
        drop(go);
        writer.drain();
        let refused: Vec<u64> = writer
            .finished()
            .iter()
            .filter_map(|done| match done {
                Done::ChangedOutside { seq, .. } => Some(*seq),
                _ => None,
            })
            .collect();
        assert_eq!(refused, [first, second]);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "changed outside, longer", "never saved over");
        // Keep mine goes over it.
        writer.write(save(&a, "kept", Guard::Any));
        writer.drain();
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "kept");
        // A save that follows one of ours compares with what ours left, not with what the app
        // saw before it.
        let go = hold(&writer);
        writer.write(save(&a, "one", Guard::Unchanged(outside::stamp(&a))));
        writer.write(save(&dir.0.join("b.sppres"), "b", Guard::Any));
        writer.write(save(&a, "two, longer", Guard::AsLeft));
        drop(go);
        writer.drain();
        assert!(writer.finished().iter().all(|done| matches!(done, Done::Written { .. })));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "two, longer");
    }

    #[test]
    fn a_save_of_the_file_as_last_seen_behind_another_programs_version_is_kept_from_it() {
        // A song pack's version written over a song in use (not the app's own save), then the
        // app's save of the song, made from the file as the app last read it: never merged, and
        // left undone once the pack's version is there, so the song waits on Reload or Keep mine.
        let dir = TempDir::new("behind");
        let a = dir.0.join("a.sppres");
        std::fs::write(&a, "read").unwrap();
        let seen = outside::stamp(&a);
        let writer = quiet();
        let go = hold(&writer);
        writer.write(save(&a, "the pack's version", Guard::Any));
        let mine = writer.write(save(&a, "mine", Guard::Unchanged(seen)));
        assert_eq!(writer.left(), 3, "not merged");
        drop(go);
        writer.drain();
        let done = writer.finished();
        assert!(done.iter().any(|done| matches!(done, Done::ChangedOutside { seq, .. } if *seq == mine)), "{done:?}");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "the pack's version");
    }

    #[test]
    fn the_old_file_is_kept_before_it_is_written_over() {
        let dir = TempDir::new("backup");
        let a = dir.0.join("a.sppres");
        std::fs::write(&a, "before").unwrap();
        std::fs::create_dir_all(dir.0.join("Backup")).unwrap();
        std::fs::write(dir.0.join("Backup").join("a.sppres"), "older backup").unwrap();
        let writer = quiet();
        let mut tidied = save(&a, "after", Guard::Any);
        tidied.keep_old_in = Some(dir.0.join("Backup"));
        writer.write(tidied);
        writer.drain();
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "after");
        assert_eq!(files_in(&dir.0.join("Backup")), ["a 2.sppres", "a.sppres"]);
        assert_eq!(std::fs::read_to_string(dir.0.join("Backup").join("a 2.sppres")).unwrap(), "before");
        // A copy that can't be made leaves the file as it was.
        std::fs::write(dir.0.join("not a folder"), "").unwrap();
        let mut tidied = save(&a, "again", Guard::Any);
        tidied.keep_old_in = Some(dir.0.join("not a folder"));
        writer.write(tidied);
        writer.drain();
        // It says where the old file was to be kept, so a save tried again keeps it too.
        let backup = dir.0.join("not a folder");
        assert!(matches!(
            writer.finished().as_slice(),
            [_, Done::Failed { keep_old_in: Some(folder), .. }] if *folder == backup
        ));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "after");
    }

    #[test]
    fn a_file_flushed_is_written_before_what_waits_ahead_of_it() {
        let dir = TempDir::new("flush");
        let writer = Writer::spawn(Box::new(|| {}), Some(Duration::from_millis(20)));
        for n in 0..10 {
            writer.write(save(&dir.0.join(format!("{n}.sppres")), "x", Guard::Any));
        }
        let wanted = dir.0.join("wanted.sppres");
        writer.write(save(&wanted, "read me", Guard::Any));
        writer.flush(&wanted);
        assert_eq!(std::fs::read_to_string(&wanted).unwrap(), "read me");
        assert!(writer.left() > 0, "the others still wait");
        writer.drain();
    }
}
