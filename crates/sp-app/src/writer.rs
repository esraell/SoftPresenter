//! The library's files, written on a thread of their own. Each song is a file, and on some
//! computers every new file is slow to write (Windows Defender looks at each one, the disk is
//! busy, the library is on a network folder): the 2,001 songs of a free songbook took minutes,
//! and the window answered nothing meanwhile. Now the main thread only hands each file over;
//! this thread writes them, one at a time, in the order they were handed over:
//!
//! - **Whole or not at all:** each file is written under a name of its own beside it
//!   (`.<name>.saving` for a song, `<name>.part` for the rest, which the library tidy knows as
//!   half-written) and then renamed into place.
//! - **In order:** a song is on disk before a playlist or library handed over after it, so a
//!   crash never leaves a playlist naming a song that has no file. A save of a file whose last
//!   save is still waiting at the end of the queue takes that save's place, so a song saved
//!   again and again while the disk is busy is written once; with anything handed over in
//!   between, both are written, in order.
//! - **Never over a change made outside the app:** a save carries how the app last saw the file
//!   ([`Guard`]), and is left undone, and said, if the file has changed since.
//! - **At quit** the app waits until everything handed over is written. If it is killed, what
//!   was still waiting is lost, as a save under way would have been before.
//!
//! What happened comes back as [`Done`]s, which the main thread takes with
//! [`Writer::finished`]; the thread wakes it when a save fails or the queue empties.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use crate::outside::{self, Stamp};

/// What a file is, which says how its half-written copy is named and what the window says if it
/// can't be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Song,
    Playlists,
    Libraries,
    MediaList,
}

impl Kind {
    /// The name the file is written under before it is renamed into place.
    fn temporary(self, path: &Path) -> PathBuf {
        let name = path.file_name().map_or_else(|| "file".into(), |name| name.to_string_lossy().into_owned());
        match self {
            Kind::Song => path.with_file_name(format!(".{name}.saving")),
            Kind::Playlists | Kind::Libraries | Kind::MediaList => path.with_file_name(format!("{name}.part")),
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
    /// Couldn't be written.
    Failed { path: PathBuf, seq: u64, kind: Kind, error: std::io::Error },
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
            let done = write_one(seq, save, &mut HashMap::new(), None);
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
        let mut state = self.shared.lock();
        while self.thread.is_some() && (!state.queue.is_empty() || state.busy.is_some()) {
            state = self.shared.wait(state);
        }
    }

    /// Writes a file's waiting saves before anything else, and waits for them (and for one of it
    /// being written now): for reading the file straight after.
    pub fn flush(&self, path: &Path) {
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
            Task::Save { seq, save } => Some(write_one(seq, save, &mut left, slow)),
            Task::Job(job) => job().err().map(Done::Problem),
        };
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
        std::fs::write(&temporary, &bytes)?;
        std::fs::rename(&temporary, &path)
    })();
    let stamp = outside::stamp(&path);
    left.insert(path.clone(), Left::As(stamp));
    match written {
        Ok(()) => Done::Written { path, seq, stamp },
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            crate::applog::problem("saving", &format!("could not write {}: {error}", path.display()));
            Done::Failed { path, seq, kind, error }
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
            Done::Failed { path, seq, kind, error } => Some((path.clone(), *seq, *kind, error.kind())),
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
        assert!(matches!(writer.finished().as_slice(), [_, Done::Failed { .. }]));
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
