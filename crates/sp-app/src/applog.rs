//! The app's log: what happened while it ran, each line with the time and how long the app had
//! been running, so slow spots and problems can be found afterwards. It is kept in
//! `~/Library/Logs/SoftPresenter/SoftPresenter.log` on a Mac (a run with its own data folder,
//! such as a test, logs there instead). Past 5 MB the file is moved to `SoftPresenter.1.log`
//! and a new one begun, so it never grows without end. Nothing in it leaves the computer.

pub mod file_kind;
pub mod last_run;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// A log larger than this is moved aside and a new one begun.
const LIMIT_BYTES: u64 = 5 * 1024 * 1024;

static FILE: OnceLock<Mutex<Option<(File, PathBuf)>>> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();
/// The log's own thread, which writes the lines of threads that must never wait for the disk.
static WRITER: OnceLock<std::sync::mpsc::Sender<String>> = OnceLock::new();

/// Where the log is kept: a place of this computer's own, never the library's folder on a Mac.
pub(crate) fn folder() -> Option<PathBuf> {
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

/// Adds one line to the log of the copy running with this data folder, from a start it turned
/// away, which opens no log of its own (`one_copy`): written to the file's end in one go, as that
/// copy's own lines are, so neither is cut into the other. Nothing when there is no log there
/// yet: none is made.
pub(crate) fn add_from_outside(area: &str, message: &str) {
    if let Some(dir) = folder() {
        add_from_outside_in(&dir, area, message);
    }
}

fn add_from_outside_in(dir: &std::path::Path, area: &str, message: &str) {
    let Ok(mut file) = OpenOptions::new().append(true).open(dir.join("SoftPresenter.log")) else { return };
    let mut said = line(area, message);
    said.push('\n');
    let _ = file.write_all(said.as_bytes());
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
        &format!("SoftPresenter {} on {} {}", crate::updates::VERSION, std::env::consts::OS, std::env::consts::ARCH),
    );
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Written at once, whatever thread it is: the app may be about to stop.
        write_line(&line("crash", &info.to_string()));
        last_run::crashed();
        crate::reports::on_panic(info);
        before(info);
    }));
}

thread_local! {
    /// How long this thread has spent writing to the log, in all, the wait for the file
    /// included.
    static WRITING: std::cell::Cell<std::time::Duration> = const { std::cell::Cell::new(std::time::Duration::ZERO) };
    /// This thread hands its lines to the log's own thread instead of writing them.
    static NEVER_WAITS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// From now on the calling thread never waits for the log: its lines go to the log's own thread,
/// which writes them in order, so a slow disk (or another thread writing) can't hold it. The
/// screens' thread calls it as it starts: a line written there once held the screens 486 ms.
/// Other threads write at once, so nothing they say is lost if the app stops just after.
pub fn never_wait_on_this_thread() {
    NEVER_WAITS.with(|never| never.set(true));
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
/// about, and what happened (handed to the log's own thread on a thread that never waits).
pub fn event(area: &str, message: &str) {
    let began = Instant::now();
    // The time is the moment it is said, wherever the line is written.
    let line = line(area, message);
    if NEVER_WAITS.with(std::cell::Cell::get) {
        if let Err(std::sync::mpsc::SendError(line)) = writer().send(line) {
            // No thread of its own could be started: written here after all.
            write_line(&line);
        }
    } else {
        write_line(&line);
    }
    WRITING.with(|writing| writing.set(writing.get() + began.elapsed()));
}

/// The log's own thread, started the first time a thread that never waits says something.
fn writer() -> &'static std::sync::mpsc::Sender<String> {
    WRITER.get_or_init(|| {
        let (lines, coming) = std::sync::mpsc::channel::<String>();
        // If the thread can't start, `coming` is dropped with it and each send fails.
        let _ = std::thread::Builder::new().name("log".to_owned()).spawn(move || {
            for line in coming {
                write_line(&line);
            }
        });
        lines
    })
}

/// One line of the log: the clock time, the seconds since the start, the part of the app, and
/// what happened.
fn line(area: &str, message: &str) -> String {
    // The log is read by developers: in English whatever language the window speaks.
    let english = crate::i18n::english_of(message);
    let message = plain_paths(&english);
    let since = STARTED.get().map_or(0.0, |started| started.elapsed().as_secs_f64());
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    format!("{now} +{since:.1}s [{area}] {message}")
}

fn write_line(line: &str) {
    let Some(Ok(mut guard)) = FILE.get().map(Mutex::lock) else { return };
    let Some((file, path)) = guard.as_mut() else { return };
    let _ = writeln!(file, "{line}");
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
    eprintln!("{}", crate::i18n::english_of(message));
    event(area, message);
    crate::reports::stats::problem_in(area, message);
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

/// How much memory the app holds, in megabytes, for the log, as the system reports it
/// ("480 MB resident"; on Windows "480 MB private"), or `None` when it can't be asked.
pub fn memory_line() -> Option<String> {
    #[cfg(windows)]
    {
        // Windows has no `ps`: its private bytes, the figure Task Manager's Memory column shows.
        sp_winapi::process_stats().map(|stats| format!("{} MB private", stats.app_private_bytes >> 20))
    }
    #[cfg(not(windows))]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().map(|kb| format!("{} MB resident", kb / 1024))
    }
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
    fn a_line_from_a_start_turned_away_goes_at_the_end_of_the_log_and_makes_none() {
        let dir = std::env::temp_dir().join(format!("softpresenter-applog-outside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("SoftPresenter.log");
        add_from_outside_in(&dir, "start", "said from outside");
        assert!(!log.exists(), "no log is made");
        std::fs::write(&log, "a line of the copy running\n").unwrap();
        add_from_outside_in(&dir, "start", "said from outside");
        let written = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(lines.len(), 2, "{written:?}");
        assert_eq!(lines[0], "a line of the copy running");
        assert!(lines[1].ends_with(" [start] said from outside"), "{}", lines[1]);
        assert!(written.ends_with('\n'), "a whole line");
        let _ = std::fs::remove_dir_all(&dir);
    }

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
    fn a_thread_that_never_waits_is_not_held_by_the_log() {
        let _ = FILE.set(Mutex::new(None));
        // Another thread is writing, slowly: it holds the file.
        let held = FILE.get().unwrap().lock().unwrap();
        let took = std::thread::spawn(|| {
            never_wait_on_this_thread();
            let began = Instant::now();
            event("test", "a line said while the log is held");
            began.elapsed()
        })
        .join()
        .unwrap();
        drop(held);
        assert!(took < std::time::Duration::from_millis(200), "the line waited {took:?}");
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
