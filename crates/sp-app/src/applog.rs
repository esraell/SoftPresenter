//! The app's log: what happened while it ran, each line with the time and how long the app had
//! been running, so slow spots and problems can be found afterwards. It is kept in
//! `~/Library/Logs/SoftPresenter/SoftPresenter.log` on a Mac (a run with its own data folder,
//! such as a test, logs there instead). Past 5 MB the file is moved to `SoftPresenter.1.log`
//! and a new one begun, so it never grows without end. Nothing in it leaves the computer.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// A log larger than this is moved aside and a new one begun.
const LIMIT_BYTES: u64 = 5 * 1024 * 1024;

static FILE: OnceLock<Mutex<Option<(File, PathBuf)>>> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Where the log is kept.
fn folder() -> Option<PathBuf> {
    if let Some(data) = std::env::var_os("SOFTPRESENTER_DATA") {
        return Some(PathBuf::from(data));
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs/SoftPresenter"));
    }
    crate::bible_store::data_dir()
}

/// Opens the log for adding to, readable by this user only (it names Remote Control's devices
/// and the addresses they came from): a new log is made so, and one already there is set so.
fn open(path: &PathBuf) -> Option<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    Some(file)
}

/// Opens the log and says the app has started. Also writes any crash to it.
pub fn start() {
    STARTED.get_or_init(Instant::now);
    let opened = folder().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("SoftPresenter.log");
        open(&path).map(|file| (file, path))
    });
    let _ = FILE.set(Mutex::new(opened));
    event(
        "start",
        &format!("SoftPresenter {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH),
    );
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        event("crash", &info.to_string());
        before(info);
    }));
}

thread_local! {
    /// How long this thread has spent writing to the log, in all, the wait for the file
    /// included.
    static WRITING: std::cell::Cell<std::time::Duration> = const { std::cell::Cell::new(std::time::Duration::ZERO) };
}

/// How long the calling thread has spent writing to the log, in all: the screens' thread reads
/// it to say how much of a hold-up that was.
pub fn time_writing() -> std::time::Duration {
    WRITING.with(std::cell::Cell::get)
}

/// Words with every path in them as a person writes it. Windows hands back full paths in its
/// "verbatim" form (`\\?\C:\Music\a.mp3`, `\\?\UNC\server\share`), which means the same but
/// looks like a mistake. Used wherever a path is written for people (the log, notices); a path
/// used to open a file is never changed.
pub fn plain_paths(text: &str) -> std::borrow::Cow<'_, str> {
    const VERBATIM: &str = "\\\\?\\";
    const VERBATIM_UNC: &str = "\\\\?\\UNC\\";
    // The same, as a quoted string writes it (each backslash doubled): a command's words are
    // logged that way.
    const QUOTED: &str = "\\\\\\\\?\\\\";
    const QUOTED_UNC: &str = "\\\\\\\\?\\\\UNC\\\\";
    if !text.contains(VERBATIM) {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(
        text.replace(QUOTED_UNC, "\\\\\\\\").replace(QUOTED, "").replace(VERBATIM_UNC, "\\\\").replace(VERBATIM, ""),
    )
}

/// Writes one line: the clock time, the seconds since the start, what part of the app it is
/// about, and what happened.
pub fn event(area: &str, message: &str) {
    let began = Instant::now();
    write_event(area, message);
    WRITING.with(|writing| writing.set(writing.get() + began.elapsed()));
}

fn write_event(area: &str, message: &str) {
    let message = plain_paths(message);
    let Some(Ok(mut guard)) = FILE.get().map(Mutex::lock) else { return };
    let Some((file, path)) = guard.as_mut() else { return };
    let since = STARTED.get().map_or(0.0, |started| started.elapsed().as_secs_f64());
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let _ = writeln!(file, "{now} +{since:.1}s [{area}] {message}");
    // Too big: move it aside and begin again.
    if file.metadata().is_ok_and(|meta| meta.len() > LIMIT_BYTES) {
        let older = path.with_file_name("SoftPresenter.1.log");
        let _ = std::fs::rename(&*path, older);
        if let Some(fresh) = open(path) {
            *file = fresh;
        }
    }
}

/// Writes a problem to the log, and to the terminal for someone running the app from one.
pub fn problem(area: &str, message: &str) {
    eprintln!("{message}");
    event(area, message);
}

/// A problem that may come back with every picture drawn (thirty times a second, under a
/// playing video): said once when it starts, and once more when it is over, never for every
/// picture.
#[derive(Debug, Default)]
pub struct Lasting {
    said: Option<String>,
    repeats: u64,
}

impl Lasting {
    /// Notes the problem. True when it should be said: it is new, or not the one said last.
    pub fn is_news(&mut self, message: &str) -> bool {
        if self.said.as_deref() == Some(message) {
            self.repeats += 1;
            return false;
        }
        self.said = Some(message.to_owned());
        self.repeats = 0;
        true
    }

    /// Notes that the problem did not happen this time. If one was said, gives how many more
    /// times it happened after that, for saying that it is over.
    pub fn over(&mut self) -> Option<u64> {
        self.said.take().map(|_| std::mem::take(&mut self.repeats))
    }
}

/// How the app's own resident memory stands, in megabytes, as the system reports it.
pub fn memory_mb() -> Option<u64> {
    let out =
        std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().map(|kb| kb / 1024)
}

/// Counts and times something done again and again, such as drawing a window, and says how it
/// went once a minute.
#[derive(Debug)]
pub struct Timing {
    what: &'static str,
    count: u64,
    total: f64,
    longest: f64,
    since: Instant,
}

impl Timing {
    pub fn new(what: &'static str) -> Timing {
        Timing { what, count: 0, total: 0.0, longest: 0.0, since: Instant::now() }
    }

    /// Counts one, which took this long.
    pub fn add(&mut self, took: std::time::Duration) {
        let ms = took.as_secs_f64() * 1000.0;
        self.count += 1;
        self.total += ms;
        self.longest = self.longest.max(ms);
    }

    /// Once a minute has passed: writes how many there were and how long they took, with
    /// anything else worth saying, and begins counting again. True when it wrote.
    pub fn report_each_minute(&mut self, area: &str, extra: impl FnOnce() -> String) -> bool {
        let elapsed = self.since.elapsed().as_secs_f64();
        if elapsed < 60.0 {
            return false;
        }
        let average = if self.count == 0 { 0.0 } else { self.total / self.count as f64 };
        let extra = extra();
        event(
            area,
            &format!(
                "{} {} in {elapsed:.0}s ({:.1}/s), {average:.1} ms each, longest {:.1} ms{}{extra}",
                self.count,
                self.what,
                self.count as f64 / elapsed,
                self.longest,
                if extra.is_empty() { "" } else { "; " }
            ),
        );
        *self = Timing::new(self.what);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_written_without_windows_verbatim_prefix() {
        assert_eq!(plain_paths("added \\\\?\\C:\\Music\\a.mp3 in 3ms"), "added C:\\Music\\a.mp3 in 3ms");
        assert_eq!(plain_paths("\\\\?\\UNC\\server\\share\\a.mp3"), "\\\\server\\share\\a.mp3");
        assert_eq!(plain_paths("/Users/a/b.mp3 and C:\\x"), "/Users/a/b.mp3 and C:\\x");
        // A command's words, logged as quoted strings, each backslash doubled.
        assert_eq!(
            plain_paths(r#"["song", "import", "\\\\?\\C:\\Songs\\a.txt"] -> done"#),
            r#"["song", "import", "C:\\Songs\\a.txt"] -> done"#
        );
        assert_eq!(
            plain_paths(r#"["media", "add", "\\\\?\\UNC\\server\\a.mp4"]"#),
            r#"["media", "add", "\\\\server\\a.mp4"]"#
        );
    }

    #[test]
    fn timings_count_average_and_keep_the_longest() {
        let mut timing = Timing::new("frames");
        timing.add(std::time::Duration::from_millis(2));
        timing.add(std::time::Duration::from_millis(6));
        assert_eq!((timing.count, timing.total.round(), timing.longest.round()), (2, 8.0, 6.0));
        // Under a minute nothing is written and the counts stay.
        assert!(!timing.report_each_minute("test", String::new));
        assert_eq!(timing.count, 2);
    }

    #[test]
    fn a_lasting_problem_is_said_when_it_starts_and_when_it_is_over() {
        let mut trouble = Lasting::default();
        assert_eq!(trouble.over(), None, "nothing was wrong");
        assert!(trouble.is_news("too heavy"));
        for _ in 0..1800 {
            assert!(!trouble.is_news("too heavy"), "a minute of video says it once");
        }
        assert!(trouble.is_news("too slow"), "another problem is news");
        assert!(!trouble.is_news("too slow"));
        assert_eq!(trouble.over(), Some(1), "over, after happening once more than was said");
        assert_eq!(trouble.over(), None, "and that is said once too");
        assert!(trouble.is_news("too slow"), "coming back, it is news again");
    }
}
