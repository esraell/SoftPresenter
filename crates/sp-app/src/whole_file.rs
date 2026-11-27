//! Settings files read whole or kept aside, and files written whole.
//!
//! - **A settings file that is there but can't be read** (cut short by a power cut, filled with
//!   zeros, not text, damaged, or made by a newer SoftPresenter) is kept beside itself, as
//!   `screens (could not be read).json` (then `screens (could not be read 2).json`, and so on,
//!   never over a copy kept before), and the app starts from the usual settings. So what was in
//!   it is never saved over. The operator is told once, in the notices ([`tell`]), and the log
//!   says why. A file that isn't there is the usual settings, and nothing is said.
//! - **A file is written whole** ([`replace`]): under a name of its own beside it, flushed to the
//!   disk, then renamed into place (on Windows by a rename written through to the disk,
//!   `sp_winapi::move_written_through`), and its folder flushed after (on a Mac and Linux:
//!   Windows can't open a folder to flush it), so a power cut leaves the old file or the new one,
//!   never an empty or half-written one. A save that fails takes its half-written file away.
//!
//! A file the app isn't allowed to open (macOS kept it out of Documents) that can't be moved
//! either is left where it is, and only the log says so at the start: what keeps the app out is
//! said once elsewhere (`library_folder.rs`), not once for each settings file. One that can't be
//! opened for another reason (another program has it open), or can't be read, and can't be moved
//! is told, saying the old file couldn't be kept. Either way **no save goes over it for the rest
//! of the run** ([`refuses`]), even once the app could write there (the operator lets it into
//! Documents, the other program lets the file go): what is in it may be the church's real
//! settings, and the app is running on the usual ones. The first save refused of a file not told
//! at the start says so ([`refusal`]); starting the app again reads the file.
//!
//! These files are small, but flushing one waits for the disk: on an Apple silicon Mac about
//! 5.4 ms a file, 10.8 ms with its folder. So none is written on the window's thread or the
//! screens': every one goes to the library's writer (`writer.rs`), which flushes a folder once
//! after a run of files into it, and the small settings files are paced (`writer::Paced`), so a
//! slider dragged doesn't write one at every step.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::i18n::{t, tf};

/// Which settings file it is, which says what the operator is told when it can't be read or
/// saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    Screens,
    Settings,
    MediaList,
    Audio,
    Macros,
    Timers,
    Playlists,
    Libraries,
    Messages,
    Props,
    StageLayouts,
    BiblePage,
    BibleHistory,
}

impl What {
    /// What the window says when the file couldn't be written: a plain sentence with the
    /// system's reason.
    pub fn not_saved(self, error: &std::io::Error) -> String {
        let error = crate::notices::write_problem(error);
        let error = [("error", error.as_str())];
        match self {
            What::Screens => tf("screens.not_saved", "Could not save the screens: {error}", &error),
            What::Settings => tf("notices.save.settings", "Could not save the settings: {error}", &error),
            What::MediaList => tf("media.not_saved", "Could not save the media list: {error}", &error),
            What::Audio => tf("tools.audio.not_saved", "Could not save the audio list: {error}", &error),
            What::Macros => tf("tools.macros.not_saved", "Could not save the macros: {error}", &error),
            What::Timers => tf("tools.timers.not_saved", "Could not save the timers: {error}", &error),
            What::Playlists => tf("notices.save.services", "Could not save the playlists: {error}", &error),
            What::Libraries => tf("notices.save.libraries", "Could not save the libraries: {error}", &error),
            What::Messages => tf("tools.messages.not_saved", "Could not save the messages: {error}", &error),
            What::Props => tf("tools.props.not_saved", "Could not save the props: {error}", &error),
            What::StageLayouts => tf("tools.stage.not_saved", "Could not save the stage layouts: {error}", &error),
            What::BiblePage => {
                tf("notices.save.bible_page", "Could not save the Bible page's settings: {error}", &error)
            }
            What::BibleHistory => tf("notices.save.bible_history", "Could not save the Bible history: {error}", &error),
        }
    }
}

/// A settings file that was there but couldn't be read, and where it was kept.
#[derive(Debug)]
pub struct Unread {
    pub what: What,
    /// The copy kept, or why none could be kept (a plain sentence).
    pub kept: Result<PathBuf, String>,
}

impl Unread {
    /// What the operator is told, in the language set.
    pub fn told(&self) -> String {
        let first = match self.what {
            What::Screens => t(
                "notices.unread.screens",
                "SoftPresenter couldn't read its screen settings, so it started with new ones.",
            ),
            What::Settings => t(
                "notices.unread.settings",
                "SoftPresenter couldn't read its settings, so it started with the usual ones.",
            ),
            What::MediaList => t(
                "notices.unread.media",
                "SoftPresenter couldn't read its media bin and media playlists, so they start empty.",
            ),
            What::Audio => {
                t("notices.unread.audio", "SoftPresenter couldn't read its audio playlists, so they start empty.")
            }
            What::Macros => {
                t("notices.unread.macros", "SoftPresenter couldn't read its macros, so it started with none.")
            }
            What::Timers => {
                t("notices.unread.timers", "SoftPresenter couldn't read its timers, so it started with none.")
            }
            What::Playlists => t(
                "notices.unread.playlists",
                "SoftPresenter couldn't read its playlists, so it started with an empty one.",
            ),
            What::Libraries => {
                t("notices.unread.libraries", "SoftPresenter couldn't read its libraries, so it started with none.")
            }
            What::Messages => {
                t("notices.unread.messages", "SoftPresenter couldn't read its messages, so it started with none.")
            }
            What::Props => t("notices.unread.props", "SoftPresenter couldn't read its props, so it started with none."),
            What::StageLayouts => t(
                "notices.unread.stage_layouts",
                "SoftPresenter couldn't read its stage layouts, so it started with the usual ones.",
            ),
            What::BiblePage => t(
                "notices.unread.bible_page",
                "SoftPresenter couldn't read how the Bible page was set up, so it opens as usual.",
            ),
            What::BibleHistory => t(
                "notices.unread.bible_history",
                "SoftPresenter couldn't read the Bible passages looked up before, so the history starts empty.",
            ),
        };
        let then = match &self.kept {
            Ok(kept) => tf(
                "notices.unread.kept",
                "The old file is kept as {file}.",
                &[("file", &crate::applog::plain_paths(&kept.display().to_string()))],
            ),
            Err(error) => tf(
                "notices.unread.not_kept",
                "The old file could not be kept ({error}), so SoftPresenter won't save over it until it is started again.",
                &[("error", error)],
            ),
        };
        format!("{first} {then}")
    }
}

/// A settings file's save, for the library's writer to write (`writer`), paced
/// (`writer::Paced`): over whatever is there, its folder made if it isn't.
pub fn settings_save(path: PathBuf, what: What, bytes: Vec<u8>) -> crate::writer::Save {
    let mut save =
        crate::writer::Save::new(path, crate::writer::Kind::Settings(what), crate::writer::Guard::Any, || bytes);
    save.make_folder = true;
    save
}

/// Tells the operator of each settings file kept aside: trouble in the notices, once each (and
/// so in the log too).
pub fn tell(notices: &mut crate::notices::Notices, unread: impl IntoIterator<Item = Unread>) {
    for unread in unread {
        notices.trouble(unread.told());
    }
}

/// Reads a settings file whole: `Ok(None)` when there is none. One that is there but can't be
/// read, isn't text, or that `parse` refuses (with a reason, for the log) is kept aside and comes
/// back as the [`Unread`] to tell the operator. One the app isn't allowed to open that can't be
/// moved either is `Ok(None)` too, said only in the log (see the module's notes). One that is
/// there and neither read nor kept aside is never saved over this run ([`refuses`]).
pub fn read<T>(path: &Path, what: What, parse: impl FnOnce(&str) -> Result<T, String>) -> Result<Option<T>, Unread> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            let refused = e.kind() == std::io::ErrorKind::PermissionDenied;
            return match set_aside(path, None, &format!("{name} could not be read ({e}).")) {
                Err(_) if refused => {
                    leave_unread(path, what, false);
                    Ok(None)
                }
                kept => {
                    if kept.is_err() {
                        leave_unread(path, what, true);
                    }
                    Err(Unread { what, kept })
                }
            };
        }
    };
    let parsed = match std::str::from_utf8(&bytes) {
        Ok(text) => parse(text),
        Err(e) => Err(format!("it is not text (byte {} is not UTF-8)", e.valid_up_to() + 1)),
    };
    parsed.map(Some).map_err(|why| {
        let kept = set_aside(path, Some(&bytes), &format!("{name}: {}.", why.trim_end_matches('.')));
        if kept.is_err() {
            leave_unread(path, what, true);
        }
        Unread { what, kept }
    })
}

/// A settings file this run found there but could neither read nor keep aside.
struct NotRead {
    path: PathBuf,
    /// Whether the operator has been told no save goes over it (at the start, or at the first
    /// save refused).
    told: bool,
}

/// The settings files this run found there but could neither read nor keep aside, for the whole
/// run: one process, one library, one run (the one-copy lock keeps other copies out).
static NOT_READ: Mutex<Vec<NotRead>> = Mutex::new(Vec::new());

fn not_read() -> std::sync::MutexGuard<'static, Vec<NotRead>> {
    NOT_READ.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Notes a file neither read nor kept aside, so no save goes over it this run; `told` when the
/// start's notice says so already.
fn leave_unread(path: &Path, what: What, told: bool) {
    crate::applog::problem(
        "settings",
        &format!("{} ({what:?}) is not saved over until SoftPresenter starts again.", path.display()),
    );
    let mut left = not_read();
    if !left.iter().any(|left| left.path == path) {
        left.push(NotRead { path: path.to_path_buf(), told });
    }
}

/// Whether a save of `path` is refused this run: the file was there at the start and could be
/// neither read nor kept aside, so what is in it may be the church's real settings. The library's
/// writer asks before every save (`writer`).
pub fn refuses(path: &Path) -> bool {
    not_read().iter().any(|left| left.path == path)
}

/// Why a save of `path` was refused, for the end of a notice ("Could not save the screens: …"),
/// the first time only, and never when the start's notice said so already (`None` then; the log
/// has each).
pub fn refusal(path: &Path) -> Option<String> {
    let mut left = not_read();
    let left = left.iter_mut().find(|left| left.path == path && !left.told)?;
    left.told = true;
    Some(
        t(
            "notices.reason.not_read",
            "the file couldn't be read when SoftPresenter started, so it won't be saved over until SoftPresenter is started again",
        )
        .to_owned(),
    )
}

/// [`read`], with the usual value when there is no file or it was kept aside.
pub fn read_or_default<T: Default>(
    path: &Path,
    what: What,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> (T, Option<Unread>) {
    match read(path, what, parse) {
        Ok(value) => (value.unwrap_or_default(), None),
        Err(unread) => (T::default(), Some(unread)),
    }
}

/// How many copies of one file are kept at most; past that, the next isn't.
const KEPT_MOST: u32 = 999;

/// Keeps a settings file the app can't read beside itself, under a name no file has yet
/// (`screens (could not be read).json`, then `screens (could not be read 2).json`…), so what the
/// app uses instead is never saved over it, and logs why (`why`, a sentence). The file is renamed;
/// where it can't be (Windows won't rename a file another program has open), the `bytes` read
/// from it are written there instead. Returns the copy kept, or why none could be.
pub fn set_aside(path: &Path, bytes: Option<&[u8]>, why: &str) -> Result<PathBuf, String> {
    let stem = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let ending = path.extension().map_or_else(|| "json".to_owned(), |e| e.to_string_lossy().into_owned());
    let free = (1..=KEPT_MOST)
        .map(|n| {
            let more = if n == 1 { String::new() } else { format!(" {n}") };
            path.with_file_name(format!("{stem} (could not be read{more}).{ending}"))
        })
        // Looked at without following it: a link of that name is a file there too.
        .find(|candidate| std::fs::symlink_metadata(candidate).is_err());
    let Some(aside) = free else {
        crate::applog::problem(
            "settings",
            &format!("{why} It could not be kept: {KEPT_MOST} copies of it are kept already."),
        );
        let n = KEPT_MOST.to_string();
        return Err(tf("notices.reason.kept_most", "{n} copies of it are kept already", &[("n", &n)]));
    };
    let kept = std::fs::rename(path, &aside).or_else(|renaming| match bytes {
        Some(bytes) => write_new(&aside, bytes).map_err(|_| renaming),
        None => Err(renaming),
    });
    match kept {
        Ok(()) => {
            if let Some(folder) = path.parent() {
                flush_folder(folder);
            }
            crate::applog::problem("settings", &format!("{why} It was kept as {}.", aside.display()));
            Ok(aside)
        }
        Err(e) => {
            crate::applog::problem("settings", &format!("{why} It could not be kept: {e}"));
            Err(crate::notices::write_problem(&e))
        }
    }
}

/// Writes `bytes` to a new file at `path`, flushed to the disk, never over a file there (one that
/// came meanwhile). A copy that can't be finished is taken away.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = std::io::Write::write_all(&mut file, bytes).and_then(|()| flush(&file));
    if written.is_err() {
        drop(file);
        let _ = std::fs::remove_file(path);
    }
    written
}

/// Writes `bytes` into `temporary`, flushes it to the disk and renames it to `path`. The folder
/// is left for the caller to flush ([`flush_folder`]): the library's writer flushes it once after
/// a run of files into it. A failure takes `temporary` away.
pub fn replace(path: &Path, temporary: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let written = (|| {
        let mut file = std::fs::File::create(temporary)?;
        std::io::Write::write_all(&mut file, bytes)?;
        flush(&file)?;
        drop(file);
        rename_into_place(temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    written
}

/// Renames a file just written into place. On Windows the rename itself is written through to
/// the disk, as its folder can't be flushed; one it doesn't take (a path longer than Windows' old
/// limit) is renamed as before, and the log says so the first time, so a computer where it never
/// takes is seen.
fn rename_into_place(temporary: &Path, path: &Path) -> std::io::Result<()> {
    if !cfg!(windows) {
        return std::fs::rename(temporary, path);
    }
    sp_winapi::move_written_through(temporary, path).or_else(|error| {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        #[cfg(test)]
        FELL_BACK_HERE.with(|count| count.set(count.get() + 1));
        if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::applog::problem(
                "saving",
                &format!(
                    "{} couldn't be renamed into place written through to the disk ({error}); renamed as before \
                     (said once a run)",
                    path.display()
                ),
            );
        }
        std::fs::rename(temporary, path)
    })
}

// How many renames on this thread the written-through rename didn't take: for the test that it
// is used.
#[cfg(test)]
thread_local! {
    static FELL_BACK_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Flushes a file's bytes to the disk (on a Mac, past the disk's own cache too). A disk that
/// can't be asked to (some network folders) has the file as it was written.
fn flush(file: &std::fs::File) -> std::io::Result<()> {
    match file.sync_all() {
        Err(e) if matches!(e.kind(), std::io::ErrorKind::Unsupported | std::io::ErrorKind::InvalidInput) => Ok(()),
        flushed => flushed,
    }
}

/// Flushes a folder's list of files to the disk, so a file just renamed into it is there after a
/// power cut. Where the system can (a Mac, Linux); Windows can't open a folder as a file to flush
/// it. A disk that can't be asked to (some network folders) is left as it is.
pub fn flush_folder(folder: &Path) {
    #[cfg(unix)]
    {
        let _ = std::fs::File::open(folder).and_then(|folder| folder.sync_all());
    }
    #[cfg(not(unix))]
    let _ = folder;
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::notices::{Kind, Notices};

    /// Writes a save as the library's writer does, and waits for it to be written.
    pub fn written(save: crate::writer::Save) {
        let writer = crate::writer::Writer::start(|| {});
        writer.write(save);
        writer.drain();
        let done = writer.finished();
        assert!(done.iter().all(|done| matches!(done, crate::writer::Done::Written { .. })), "{done:?}");
    }

    /// A folder of a test's own, removed after.
    pub struct TestDir(pub PathBuf);

    impl TestDir {
        pub fn new(name: &str) -> TestDir {
            static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("softpresenter-whole-{}-{n}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TestDir(dir)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The ways a settings file is found damaged: cut short (a power cut part way through a
    /// save), empty or filled with zeros (a power cut before its blocks were written), and not
    /// UTF-8 (saved again as UTF-16 by a text editor).
    pub fn damaged(good: &[u8]) -> [(&'static str, Vec<u8>); 4] {
        let text = std::str::from_utf8(good).expect("a good file is text");
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        [
            ("cut short", good[..good.len() / 2].to_vec()),
            ("empty", Vec::new()),
            ("zeros", vec![0; good.len()]),
            ("UTF-16", utf16),
        ]
    }

    /// Checks one settings file's loader and saver: each damaged file is kept aside whole, as
    /// `<stem> (could not be read).json` beside it, a save afterwards never goes over the copy
    /// kept, and the operator is told where it is. `good` is a file the saver wrote, with
    /// something in it; `load` reads the file at a path and says what it couldn't read; `save`
    /// saves the usual settings there.
    pub fn check_loader(
        file: &str,
        what: What,
        good: &[u8],
        load: impl Fn(&Path) -> Option<Unread>,
        save: impl Fn(&Path),
    ) {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            for (how, bytes) in damaged(good) {
                let dir = TestDir::new(file);
                let path = dir.0.join(file);
                // A good file reads, and says nothing.
                std::fs::write(&path, good).unwrap();
                assert!(load(&path).is_none(), "{file}: a good file reads");
                assert_eq!(std::fs::read(&path).unwrap(), good, "{file}: a good file is left as it is");
                // A damaged one is kept aside, whole.
                std::fs::write(&path, &bytes).unwrap();
                let unread = load(&path).unwrap_or_else(|| panic!("{file} {how}: not kept aside"));
                assert_eq!(unread.what, what, "{file} {how}");
                let stem = Path::new(file).file_stem().unwrap().to_string_lossy().into_owned();
                let kept = dir.0.join(format!("{stem} (could not be read).json"));
                assert_eq!(unread.kept.as_ref().ok(), Some(&kept), "{file} {how}: {:?}", unread.kept);
                assert_eq!(std::fs::read(&kept).unwrap(), bytes, "{file} {how}: the bytes are kept");
                // The usual settings are saved over the file, never over the copy kept.
                save(&path);
                assert_eq!(std::fs::read(&kept).unwrap(), bytes, "{file} {how}: the copy survives a save");
                assert!(load(&path).is_none(), "{file} {how}: what was saved reads");
                assert!(!dir.0.join(format!("{file}.part")).exists(), "{file} {how}: no half-written file is left");
                // The operator is told, once, where it is.
                let mut notices = Notices::default();
                tell(&mut notices, [unread]);
                let said: Vec<_> = notices.entries().collect();
                assert_eq!(said.len(), 1, "{file} {how}");
                assert_eq!(said[0].kind, Kind::Trouble, "{file} {how}");
                assert!(said[0].text.starts_with("SoftPresenter couldn't read "), "{}", said[0].text);
                assert!(
                    said[0].text.ends_with(&format!(" The old file is kept as {}.", kept.display())),
                    "{}",
                    said[0].text
                );
            }
        });
    }

    #[test]
    fn a_file_set_aside_takes_a_name_of_its_own_and_never_one_kept_before() {
        let dir = TestDir::new("aside");
        let path = dir.0.join("screens.json");
        for n in 1..=3 {
            std::fs::write(&path, format!("damaged {n}")).unwrap();
            let kept = set_aside(&path, None, "screens.json is damaged.").unwrap();
            let more = if n == 1 { String::new() } else { format!(" {n}") };
            assert_eq!(kept, dir.0.join(format!("screens (could not be read{more}).json")));
            assert!(!path.exists(), "the file is moved, not copied");
        }
        // Each copy kept before is as it was.
        for (n, name) in ["screens (could not be read).json", "screens (could not be read 2).json"].iter().enumerate() {
            assert_eq!(std::fs::read_to_string(dir.0.join(name)).unwrap(), format!("damaged {}", n + 1));
        }
        // A gap is filled first: only a name no file has is taken.
        std::fs::remove_file(dir.0.join("screens (could not be read 2).json")).unwrap();
        std::fs::write(&path, "damaged 4").unwrap();
        assert_eq!(set_aside(&path, None, "again.").unwrap(), dir.0.join("screens (could not be read 2).json"));
        assert_eq!(std::fs::read_to_string(dir.0.join("screens (could not be read 3).json")).unwrap(), "damaged 3");
        // Nothing to move: said, and nothing is made.
        assert!(set_aside(&path, None, "gone.").is_err());
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 3);
    }

    #[test]
    fn a_copy_written_where_the_file_cant_be_moved_never_goes_over_another() {
        let dir = TestDir::new("copy");
        let kept = dir.0.join("screens (could not be read).json");
        write_new(&kept, b"first").unwrap();
        assert!(write_new(&kept, b"second").is_err());
        assert_eq!(std::fs::read(&kept).unwrap(), b"first");
    }

    #[test]
    fn a_file_that_cant_be_opened_is_kept_aside_too() {
        // A folder where the file should be: it can't be read as a file, so it is moved aside.
        let dir = TestDir::new("folder");
        let path = dir.0.join("screens.json");
        std::fs::create_dir_all(path.join("inside")).unwrap();
        let unread = read(&path, What::Screens, |_| -> Result<(), String> { panic!("nothing to parse") })
            .expect_err("kept aside");
        let kept = dir.0.join("screens (could not be read).json");
        assert_eq!(unread.kept.as_ref().ok(), Some(&kept));
        assert!(kept.join("inside").is_dir(), "moved whole");
        assert!(!path.exists());
    }

    /// A file the app isn't allowed to open that can't be moved either (its folder can't be
    /// changed) is left as it is, and not told: no save can go over it. One that can't be opened
    /// for another reason is told, saying it couldn't be kept.
    #[cfg(unix)]
    #[test]
    fn a_file_that_can_be_neither_opened_nor_moved_is_left_and_not_told() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("shut");
        let folder = dir.0.join("shut");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("timers.json");
        std::fs::write(&path, "[]").unwrap();
        // A folder where the macros should be can't be opened as a file either, but not for want
        // of leave.
        let other = folder.join("macros.json");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        // Run as the system's administrator, which every file lets in, there is nothing to check.
        let checked = std::fs::read(&path).is_err().then(|| {
            let timers = read(&path, What::Timers, |_| -> Result<u32, String> { Ok(1) });
            let saved = replace(&path, &folder.join("timers.json.part"), b"[1]");
            let other = read(&other, What::Macros, |_| -> Result<u32, String> { Ok(1) });
            (
                matches!(timers, Ok(None)),
                saved.is_err(),
                other.err().map(|unread| crate::i18n::with_language(crate::i18n::Language::English, || unread.told())),
            )
        });
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        if let Some((usual, not_saved, other)) = checked {
            assert!(usual, "the usual settings, said only in the log");
            assert!(not_saved, "and no save can go over it");
            assert_eq!(std::fs::read(&path).unwrap(), b"[]", "left as it was");
            assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2, "nothing else made there");
            let told = other.expect("the other is told");
            assert_eq!(
                told,
                "SoftPresenter couldn't read its macros, so it started with none. The old file could not be kept \
                 (SoftPresenter isn't allowed to write there), so SoftPresenter won't save over it until it is \
                 started again."
            );
        }
    }

    /// Hands one save to a writer of its own and says what became of it.
    fn saved(save: crate::writer::Save) -> Vec<crate::writer::Done> {
        let writer = crate::writer::Writer::start(|| {});
        writer.write(save);
        writer.drain();
        writer.finished()
    }

    fn refused_as_not_read(done: &[crate::writer::Done], what: What) -> bool {
        matches!(done, [crate::writer::Done::NotRead { kind: crate::writer::Kind::Settings(w), .. }] if *w == what)
    }

    /// A damaged file that can't be kept aside (every name a copy could take is taken) is never
    /// saved over this run: the save is refused, the file is left as it was, and the start's
    /// notice says so, so the refusal adds none. Another file there is saved as usual.
    #[test]
    fn a_file_neither_read_nor_kept_aside_is_never_saved_over_this_run() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let dir = TestDir::new("not-kept");
            let path = dir.0.join("macros.json");
            for n in 1..=KEPT_MOST {
                let more = if n == 1 { String::new() } else { format!(" {n}") };
                std::fs::write(dir.0.join(format!("macros (could not be read{more}).json")), "older").unwrap();
            }
            let damaged = br#"[{"name":"Before the serv"#;
            std::fs::write(&path, damaged).unwrap();
            let parse = |text: &str| serde_json::from_str::<serde_json::Value>(text).map_err(|e| e.to_string());
            let unread = read(&path, What::Macros, parse).expect_err("not read");
            assert_eq!(unread.kept, Err("999 copies of it are kept already".to_owned()));
            assert_eq!(
                unread.told(),
                "SoftPresenter couldn't read its macros, so it started with none. The old file could not be kept \
                 (999 copies of it are kept already), so SoftPresenter won't save over it until it is started again."
            );
            assert!(refuses(&path));
            let done = saved(settings_save(path.clone(), What::Macros, b"[]".to_vec()));
            assert!(refused_as_not_read(&done, What::Macros), "{done:?}");
            assert_eq!(std::fs::read(&path).unwrap(), damaged, "left as it was");
            assert!(!dir.0.join("macros.json.part").exists());
            assert_eq!(refusal(&path), None, "the start's notice said so already");
            let timers = dir.0.join("timers.json");
            assert!(!refuses(&timers));
            written(settings_save(timers.clone(), What::Timers, b"[]".to_vec()));
            assert_eq!(std::fs::read(&timers).unwrap(), b"[]");
        });
    }

    /// A file the app was kept out of at the start (macOS kept it out of Documents) is not
    /// saved over once it is let in while it runs: the start read none of it, so it runs on the
    /// usual settings, and the file may hold the church's real ones. The first save refused says
    /// so, once.
    #[cfg(unix)]
    #[test]
    fn a_file_the_app_was_kept_out_of_is_not_saved_over_once_it_is_let_in() {
        use std::os::unix::fs::PermissionsExt;
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let dir = TestDir::new("let-in");
            let folder = dir.0.join("shut");
            std::fs::create_dir_all(&folder).unwrap();
            let path = folder.join("screens.json");
            std::fs::write(&path, "the church's screens").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
            // Run as the system's administrator, which every file lets in, there is nothing to
            // check.
            let shut = std::fs::read(&path).is_err();
            let read = shut.then(|| read(&path, What::Screens, |_| -> Result<u32, String> { Ok(1) }));
            // The operator lets the app in.
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            let Some(read) = read else { return };
            assert!(matches!(read, Ok(None)), "the usual settings, said only in the log");
            let done = saved(settings_save(path.clone(), What::Screens, b"{}".to_vec()));
            assert!(refused_as_not_read(&done, What::Screens), "{done:?}");
            assert_eq!(std::fs::read(&path).unwrap(), b"the church's screens", "never saved over");
            let said = refusal(&path).expect("said the first time");
            assert_eq!(
                crate::writer::Kind::Settings(What::Screens).problem(&path, &std::io::Error::other(said)),
                "Could not save the screens: the file couldn't be read when SoftPresenter started, so it won't be \
                 saved over until SoftPresenter is started again"
            );
            assert_eq!(refusal(&path), None, "once");
            assert!(refused_as_not_read(
                &saved(settings_save(path.clone(), What::Screens, b"{}".to_vec())),
                What::Screens
            ));
        });
    }

    /// A file another program had open at the start (Windows won't read or rename it then) is
    /// told, in plain words, and not saved over once that program lets it go.
    #[cfg(windows)]
    #[test]
    fn a_file_another_program_held_at_the_start_is_not_saved_over_once_it_is_let_go() {
        use std::os::windows::fs::OpenOptionsExt;
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let dir = TestDir::new("held");
            let path = dir.0.join("screens.json");
            std::fs::write(&path, "the church's screens").unwrap();
            // Opened sharing nothing, as some sync tools and virus scanners open a file.
            let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
            let unread = read(&path, What::Screens, |_| -> Result<u32, String> { Ok(1) }).expect_err("told");
            assert_eq!(unread.kept, Err("another program has the file open".to_owned()));
            drop(held);
            let done = saved(settings_save(path.clone(), What::Screens, b"{}".to_vec()));
            assert!(refused_as_not_read(&done, What::Screens), "{done:?}");
            assert_eq!(std::fs::read(&path).unwrap(), b"the church's screens", "never saved over");
            assert_eq!(refusal(&path), None, "the start's notice said so already");
        });
    }

    #[test]
    fn no_file_is_the_usual_settings_and_nothing_is_kept_or_said() {
        let dir = TestDir::new("none");
        let path = dir.0.join("timers.json");
        let read = read(&path, What::Timers, |_| -> Result<u32, String> { panic!("nothing to parse") });
        assert!(matches!(read, Ok(None)));
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0, "nothing is made");
    }

    #[test]
    fn a_file_the_parser_refuses_is_kept_aside_with_its_reason() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let dir = TestDir::new("refused");
            let path = dir.0.join("screens.json");
            std::fs::write(&path, r#"{"format":"softpresenter-screens","version":2,"screens":[]}"#).unwrap();
            let read =
                read(&path, What::Screens, |_| -> Result<(), String> { Err("made by a newer SoftPresenter".into()) });
            let unread = read.expect_err("refused");
            let kept = dir.0.join("screens (could not be read).json");
            assert_eq!(unread.kept.as_ref().ok(), Some(&kept));
            assert_eq!(
                unread.told(),
                format!(
                    "SoftPresenter couldn't read its screen settings, so it started with new ones. The old file is kept as {}.",
                    kept.display()
                )
            );
        });
    }

    /// Not a check: how long one settings file's save takes on this computer, flushed as the
    /// writer flushes it, and 300 songs through the library's writer. In the system's temporary
    /// folder (`TEMP` on Windows, `TMPDIR` elsewhere):
    /// `cargo test -p sp-app whole_file::tests::how_long_flushed_saves_take -- --ignored --nocapture`
    #[test]
    #[ignore = "a timing, not a check"]
    fn how_long_flushed_saves_take() {
        let dir = TestDir::new("timing");
        let body = vec![b' '; 3000];
        let (path, part) = (dir.0.join("preferences.json"), dir.0.join("preferences.json.part"));
        const RUNS: u32 = 50;
        let began = std::time::Instant::now();
        for _ in 0..RUNS {
            replace(&path, &part, &body).unwrap();
            flush_folder(&dir.0);
        }
        let one = began.elapsed() / RUNS;
        let songs = dir.0.join("Songs");
        std::fs::create_dir_all(&songs).unwrap();
        let writer = crate::writer::Writer::start(|| {});
        let began = std::time::Instant::now();
        for n in 0..300 {
            let body = body.clone();
            let song = songs.join(format!("{n}.sppres"));
            writer.write(crate::writer::Save::new(
                song,
                crate::writer::Kind::Song,
                crate::writer::Guard::Any,
                move || body,
            ));
        }
        writer.drain();
        let all = began.elapsed();
        eprintln!(
            "TIMING one settings save: {:.1} ms; 300 songs through the writer: {:.2} s ({:.1} ms a song)",
            one.as_secs_f64() * 1000.0,
            all.as_secs_f64(),
            all.as_secs_f64() * 1000.0 / 300.0
        );
    }

    /// On Windows a save takes the place of the file there by the rename written through to the
    /// disk, never the plain rename it falls back on.
    #[cfg(windows)]
    #[test]
    fn on_windows_a_save_is_renamed_over_the_file_there_written_through() {
        let dir = TestDir::new("through");
        let path = dir.0.join("preferences.json");
        std::fs::write(&path, "before").unwrap();
        let before = FELL_BACK_HERE.with(std::cell::Cell::get);
        replace(&path, &dir.0.join("preferences.json.part"), b"after").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"after");
        assert_eq!(FELL_BACK_HERE.with(std::cell::Cell::get), before, "renamed written through, not as before");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1, "only the file");
        // A song's save, into a folder of its own, the same way.
        let songs = dir.0.join("Songs");
        std::fs::create_dir_all(&songs).unwrap();
        let song = songs.join("a.sppres");
        replace(&song, &songs.join(".a.sppres.saving"), b"{}").unwrap();
        replace(&song, &songs.join(".a.sppres.saving"), b"{ }").unwrap();
        assert_eq!(std::fs::read(&song).unwrap(), b"{ }");
        assert_eq!(FELL_BACK_HERE.with(std::cell::Cell::get), before);
    }

    #[test]
    fn a_file_is_written_whole_and_a_failed_save_leaves_nothing_half_written() {
        let dir = TestDir::new("write");
        let path = dir.0.join("macros.json");
        let part = |path: &Path| path.with_file_name(format!("{}.part", path.file_name().unwrap().to_string_lossy()));
        replace(&path, &part(&path), b"[1]").unwrap();
        replace(&path, &part(&path), b"[1, 2]").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"[1, 2]");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1, "only the file");
        // A save that can't be renamed into place (a folder is in the way) takes its half-written
        // file away.
        let blocked = dir.0.join("timers.json");
        std::fs::create_dir_all(blocked.join("inside")).unwrap();
        assert!(replace(&blocked, &part(&blocked), b"[3]").is_err());
        assert!(!dir.0.join("timers.json.part").exists(), "no half-written file is left");
        // So does a song's.
        let saving = dir.0.join(".song.sppres.saving");
        std::fs::create_dir_all(dir.0.join("Songs")).unwrap();
        assert!(replace(&dir.0.join("Songs"), &saving, b"{}").is_err());
        assert!(!saving.exists(), "no half-written file is left");
        flush_folder(&dir.0);
    }
}
