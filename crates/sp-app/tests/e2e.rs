//! End to end: the app Cargo built, started as a quiet test copy and driven through `sp` as a
//! person or a script drives it, then quit, and its files read back from the disk.
//!
//! Each test starts a copy of its own with `SOFTPRESENTER_QUIET=1`, a data folder of its own
//! (`SOFTPRESENTER_DATA`) and a control socket of its own (`SOFTPRESENTER_CONTROL_SOCKET`), in a
//! temporary folder named `spe2e-…`. Only one copy runs at a time, whatever `--test-threads`
//! says. A copy is always quit when its test ends, passed or failed: `menu Quit` through its own
//! socket, and killed if it hasn't gone 30 seconds later. A failed test keeps its folder (the
//! path is printed), with the copy's log and what it wrote to its standard output and error.
//!
//! `SOFTPRESENTER_E2E=off` skips them all, with the reason printed; so does a Linux computer with
//! no display (CI runs them under `xvfb-run`). A part a computer can't do (sound, where it has no
//! sound output) is skipped with its reason printed, never silently.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The program: the app, and `sp` too (the same program with a command after it). The one
/// Cargo built, unless `SOFTPRESENTER_E2E_APP` names another.
// Testing aid: SOFTPRESENTER_E2E_APP=<program> runs the tests against that program instead, for
// tests built on one computer and run on another, where the build's own path doesn't exist (the
// Mac's Intel build, tried on an Intel Mac).
static APP: LazyLock<String> = LazyLock::new(|| {
    std::env::var("SOFTPRESENTER_E2E_APP")
        .ok()
        .filter(|program| !program.is_empty())
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_softpresenter").to_owned())
});

/// One copy at a time (AGENTS.md: run one instance at a time), and honest timings.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// How long a copy may take to answer its first command: generous, for a cold start on a
/// computer that draws in software (CI's Linux and Windows machines).
const START_WITHIN: Duration = Duration::from_secs(120);
/// How long a copy may take to close after `menu Quit` before it is killed.
const QUIT_WITHIN: Duration = Duration::from_secs(30);
/// How long one `sp` command may take (the app answers within 15 seconds, or says it didn't).
const COMMAND_WITHIN: Duration = Duration::from_secs(45);
/// The longest a socket's path may be: the app refuses a longer one (`control::listen_at`).
const SOCKET_PATH_MOST: usize = 100;

// ---------------------------------------------------------------------------------------------
// Skipping, said out loud

/// Why every end-to-end test is skipped on this computer, if they are, and whether somebody
/// chose that (`SOFTPRESENTER_E2E=off`) or the computer simply can't (no display).
struct Skip {
    why: String,
    chosen: bool,
}

/// The skip, if there is one: `said` is `SOFTPRESENTER_E2E`, `no_display` that a Linux computer
/// has none.
fn skip_for(said: Option<&str>, no_display: bool) -> Option<Skip> {
    if let Some(said) = said
        && switched_off(said)
    {
        return Some(Skip { why: format!("SOFTPRESENTER_E2E={said}"), chosen: true });
    }
    no_display.then(|| Skip {
        why: "no display here (DISPLAY and WAYLAND_DISPLAY are unset); run them under xvfb-run".to_owned(),
        chosen: false,
    })
}

/// Why every end-to-end test is skipped on this computer, if they are.
fn skipped_here() -> Option<Skip> {
    let no_display = cfg!(all(unix, not(target_os = "macos")))
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none();
    skip_for(std::env::var("SOFTPRESENTER_E2E").ok().as_deref(), no_display)
}

/// Whether a skip is a failure: on a GitHub run (`GITHUB_ACTIONS`) only a skip somebody chose, in
/// the job's own settings with its reason, is allowed. A job that lost its display would
/// otherwise pass having tested nothing.
fn skip_fails(skip: &Skip, on_github: bool) -> bool {
    on_github && !skip.chosen
}

#[test]
fn on_github_only_a_skip_the_job_chose_passes() {
    let off = skip_for(Some("off (the end-to-end jobs run them)"), true).expect("chosen");
    assert!(off.chosen && !skip_fails(&off, true) && !skip_fails(&off, false));
    let lost = skip_for(None, true).expect("no display");
    assert!(!lost.chosen && skip_fails(&lost, true) && !skip_fails(&lost, false));
    assert!(skip_for(None, false).is_none() && skip_for(Some("on"), false).is_none());
}

/// Whether `SOFTPRESENTER_E2E` says the tests are off: `off`, `0`, `no` or `false`, with the
/// reason after it if one is given (`off (why)`), which is printed with the skip. A workflow
/// once gave a reason, and as the whole value was compared the tests ran after all.
fn switched_off(said: &str) -> bool {
    let first = said.split(|c: char| c.is_whitespace() || c == '(' || c == ':').find(|word| !word.is_empty());
    first.is_some_and(|word| matches!(word.to_ascii_lowercase().as_str(), "off" | "0" | "no" | "false"))
}

#[test]
fn the_switch_that_turns_these_tests_off_may_carry_its_reason() {
    for off in ["off", " OFF ", "0", "no", "false", "off (the app crashes on WARP)", "off: being fixed", "off(why)"] {
        assert!(switched_off(off), "{off:?}");
    }
    for on in ["", "on", "1", "yes", "offline", "official (for now)"] {
        assert!(!switched_off(on), "{on:?}");
    }
}

/// Says that something was skipped and why, where it is seen: written straight to the process's
/// standard error, which the test harness doesn't hold back as it holds back `eprintln!` from a
/// test that passes; and on GitHub as a notice on the run.
fn say_skipped(what: &str, why: &str) {
    let _ = writeln!(std::io::stderr(), "e2e: skipped {what}: {why}");
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        let _ = writeln!(std::io::stdout(), "::notice title=End-to-end test skipped::{what}: {why}");
    }
}

/// Says something worth seeing in the output of a test that passes (timings).
fn say(line: &str) {
    let _ = writeln!(std::io::stderr(), "e2e: {line}");
}

// ---------------------------------------------------------------------------------------------
// A run: a test's folder, and its turn

/// One test's folder (its copy's data folder, socket and files) and its turn: no other test's
/// copy runs while it is held. The folder is removed when the test passes and kept, with its
/// path printed, when it fails.
struct Run {
    name: &'static str,
    root: PathBuf,
    data: PathBuf,
    socket: PathBuf,
    /// Copies started so far in this run (a test may start a second on the same data).
    started: AtomicUsize,
    /// None for a folder beside another run's, which has the turn.
    _turn: Option<MutexGuard<'static, ()>>,
}

impl Run {
    /// A run for the test `name`, or `None` (said) when the tests are skipped here.
    fn new(name: &'static str) -> Option<Run> {
        if let Some(skip) = skipped_here() {
            let why = &skip.why;
            assert!(
                !skip_fails(&skip, std::env::var_os("GITHUB_ACTIONS").is_some()),
                "{name}: end-to-end tests can't run here, and on GitHub a skip is a failure unless the job \
                 sets SOFTPRESENTER_E2E=off (why): {why}"
            );
            static SAID: Once = Once::new();
            SAID.call_once(|| say_skipped("every end-to-end test", why));
            let _ = writeln!(std::io::stderr(), "e2e: skipped {name} (see above)");
            return None;
        }
        let turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        // A short path, so the socket's stays under the limit: /tmp on a Mac and Linux (a Mac's
        // own temporary folder is long), the user's temporary folder on Windows.
        // Testing aid: SOFTPRESENTER_E2E_FOLDER=<folder> puts the tests' folders in that one
        // instead, on a computer that isn't ours to write all over.
        let base = match std::env::var_os("SOFTPRESENTER_E2E_FOLDER").filter(|folder| !folder.is_empty()) {
            Some(folder) => PathBuf::from(folder),
            None if cfg!(unix) => PathBuf::from("/tmp"),
            None => std::env::temp_dir(),
        };
        let root = base.join(format!("spe2e-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("files")).expect("the test's folder");
        // The socket's own folder is left for the app to make: it makes it for this user only,
        // and on Windows it looks hard at one that was there already.
        let socket = root.join("s").join("ctl.sock");
        assert!(
            socket.as_os_str().len() <= SOCKET_PATH_MOST,
            "the socket's path is too long for the app: {}",
            socket.display()
        );
        Some(Run { name, data: root.join("data"), root, socket, started: AtomicUsize::new(0), _turn: Some(turn) })
    }

    /// Another folder, inside this run's and in its turn: a data folder and socket of its own,
    /// for a copy that runs beside this run's.
    fn beside(&self) -> Run {
        let root = self.root.join("b");
        std::fs::create_dir_all(root.join("files")).expect("the other folder");
        let socket = root.join("s").join("ctl.sock");
        assert!(socket.as_os_str().len() <= SOCKET_PATH_MOST, "the socket's path is too long: {}", socket.display());
        Run { name: self.name, data: root.join("data"), root, socket, started: AtomicUsize::new(0), _turn: None }
    }

    /// A file of the test's own, outside the copy's data folder.
    fn file(&self, name: &str) -> PathBuf {
        self.root.join("files").join(name)
    }

    /// The program with this run's copy's variables, and nothing else of SoftPresenter's from the
    /// environment the tests run in: never the owner's library, log or socket.
    fn program(&self) -> Command {
        let mut command = Command::new(&*APP);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("SOFTPRESENTER_") {
                command.env_remove(&name);
            }
        }
        command
            .env("SOFTPRESENTER_QUIET", "1")
            .env("SOFTPRESENTER_DATA", &self.data)
            .env("SOFTPRESENTER_CONTROL_SOCKET", &self.socket)
            .env("SOFTPRESENTER_LANGUAGE", "en")
            .stdin(Stdio::null());
        command
    }

    /// Starts a copy, with any more variables given, and waits until it answers.
    fn start(&self, more: &[(&str, &str)]) -> Copy<'_> {
        let (child, err) = self.spawn(more);
        let mut copy = Copy { run: self, child, exit: None, err };
        copy.wait_until_it_answers();
        copy
    }

    /// Starts the app as `start` does, without waiting for it to answer: a later start on a
    /// folder a copy already has, which is to end by itself.
    fn launch(&self, more: &[(&str, &str)]) -> Started {
        let (child, err) = self.spawn(more);
        Started { child, exit: None, err }
    }

    /// The program started, with its standard output and error going to files of their own, and
    /// the error's.
    fn spawn(&self, more: &[(&str, &str)]) -> (Child, PathBuf) {
        let number = self.started.fetch_add(1, Ordering::Relaxed);
        let out = self.root.join(format!("app-{number}.out"));
        let err = self.root.join(format!("app-{number}.err"));
        let mut command = self.program();
        for (name, value) in more {
            command.env(name, value);
        }
        command
            .stdout(std::fs::File::create(&out).expect("the copy's output file"))
            .stderr(std::fs::File::create(&err).expect("the copy's error file"));
        let child = command.spawn().unwrap_or_else(|e| panic!("could not start {}: {e}", *APP));
        (child, err)
    }

    /// The copy's log, as far as it has been written.
    fn log(&self) -> String {
        std::fs::read_to_string(self.data.join("SoftPresenter.log")).unwrap_or_default()
    }

    /// Runs `sp` with these words and `--json`, as a script would, and reads its reply.
    fn sp(&self, args: &[&str]) -> Reply {
        self.sp_with_input(args, None)
    }

    /// The same, with a line given on its standard input (a password).
    fn sp_with_input(&self, args: &[&str], input: Option<&str>) -> Reply {
        let mut command = self.program();
        command.args(args).arg("--json");
        let (status, stdout, stderr, took) = run_to_end(command, input, COMMAND_WITHIN);
        let words = args.join(" ");
        let Some(status) = status else {
            panic!("sp {words} took longer than {COMMAND_WITHIN:?}{}", self.context());
        };
        let reply: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!("sp {words} didn't answer in JSON ({e}): {stdout:?} {stderr:?}{}", self.context())
        });
        let ok = reply["ok"].as_bool().unwrap_or_else(|| panic!("sp {words}: no ok in {reply}"));
        let text = reply["text"].as_str().unwrap_or_default().to_owned();
        // The exit code says the same as the reply: 0 done, 1 refused.
        let code = status.code();
        assert_eq!(code, Some(if ok { 0 } else { 1 }), "sp {words} exited {status} with {reply}{}", self.context());
        Reply { words, ok, text, data: reply["data"].clone(), took }
    }

    /// The end of the log and of the copy's standard error, for a failure's message.
    fn context(&self) -> String {
        let tail = |text: &str, lines: usize| {
            let all: Vec<&str> = text.lines().collect();
            all[all.len().saturating_sub(lines)..].join("\n")
        };
        let mut said = format!("\n\n--- the test's folder: {}", self.root.display());
        said.push_str(&format!("\n--- the end of the copy's log:\n{}", tail(&self.log(), 40)));
        for number in 0..self.started.load(Ordering::Relaxed) {
            let err = std::fs::read_to_string(self.root.join(format!("app-{number}.err"))).unwrap_or_default();
            if !err.trim().is_empty() {
                said.push_str(&format!("\n--- the end of copy {number}'s standard error:\n{}", tail(&err, 20)));
            }
        }
        said
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ =
                writeln!(std::io::stderr(), "e2e: {} failed; its folder is kept: {}", self.name, self.root.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A copy of the app, running

/// A copy that is running. However its test ends, it is quit: `menu Quit` through its own
/// socket, then killed if it hasn't gone in time.
struct Copy<'a> {
    run: &'a Run,
    child: Child,
    exit: Option<ExitStatus>,
    err: PathBuf,
}

/// What a copy left when it quit.
struct Quitted {
    status: ExitStatus,
    log: String,
    stderr: String,
}

impl Copy<'_> {
    fn wait_until_it_answers(&mut self) {
        let began = Instant::now();
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exit = Some(status);
                panic!("the copy stopped before it answered ({status}){}", self.run.context());
            }
            let mut command = self.run.program();
            command.args(["status", "--json"]);
            let (status, stdout, _, _) = run_to_end(command, None, COMMAND_WITHIN);
            if status.is_some_and(|status| status.success()) && stdout.contains("\"ok\": true") {
                say(&format!("{}: copy answered after {:.1} s", self.run.name, began.elapsed().as_secs_f64()));
                return;
            }
            if began.elapsed() > START_WITHIN {
                self.kill();
                panic!("the copy didn't answer within {START_WITHIN:?}{}", self.run.context());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn sp(&self, args: &[&str]) -> Reply {
        self.run.sp(args)
    }

    /// `sp` with these words, which must be done.
    fn ok(&self, args: &[&str]) -> Reply {
        let reply = self.run.sp(args);
        assert!(reply.ok, "sp {} was refused: {}{}", reply.words, reply.text, self.run.context());
        reply
    }

    /// `sp` with these words, which must be refused, with words containing `why`.
    fn refused(&self, args: &[&str], why: &str) -> Reply {
        let reply = self.run.sp(args);
        assert!(!reply.ok, "sp {} was done, and should have been refused: {}", reply.words, reply.text);
        assert!(reply.text.contains(why), "sp {} was refused saying {:?}, not {why:?}", reply.words, reply.text);
        reply
    }

    /// `status --json`'s facts.
    fn status(&self) -> Value {
        self.ok(&["status"]).data
    }

    /// What is on screen: the song (or passage) and slide, or `None`.
    fn live(&self) -> Option<(String, u64)> {
        let status = self.status();
        let live = &status["live"];
        if live.is_null() {
            return None;
        }
        Some((live["song"].as_str().unwrap_or_default().to_owned(), live["slide"].as_u64().unwrap_or_default()))
    }

    /// Asserts what is on screen.
    fn assert_live(&self, song: &str, slide: u64) {
        let live = self.live();
        assert_eq!(live, Some((song.to_owned(), slide)), "on screen{}", self.run.context());
    }

    /// Waits until `ready` holds for the status, or fails after `within`.
    fn wait_for(&self, what: &str, within: Duration, ready: impl Fn(&Value) -> bool) -> Value {
        let began = Instant::now();
        loop {
            let status = self.status();
            if ready(&status) {
                return status;
            }
            assert!(began.elapsed() < within, "{what} didn't happen within {within:?}: {status}{}", self.run.context());
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Quits the copy as a person would (`menu Quit`), and waits for it to go.
    fn quit(self) -> Quitted {
        self.quit_within(QUIT_WITHIN)
    }

    fn quit_within(mut self, within: Duration) -> Quitted {
        self.ok(&["menu", "Quit"]);
        let status = self.wait_exit(within).unwrap_or_else(|| {
            self.kill();
            panic!("the copy hadn't quit {within:?} after menu Quit; it was killed{}", self.run.context())
        });
        Quitted { status, log: self.run.log(), stderr: std::fs::read_to_string(&self.err).unwrap_or_default() }
    }

    /// Waits for a copy that ends without being asked through `sp` (the system ends it).
    #[cfg(target_os = "macos")]
    fn ended_within(mut self, within: Duration) -> Quitted {
        let status = self.wait_exit(within).unwrap_or_else(|| {
            self.kill();
            panic!("the copy hadn't ended after {within:?}; it was killed{}", self.run.context())
        });
        Quitted { status, log: self.run.log(), stderr: std::fs::read_to_string(&self.err).unwrap_or_default() }
    }

    fn wait_exit(&mut self, within: Duration) -> Option<ExitStatus> {
        if let Some(status) = self.exit {
            return Some(status);
        }
        let began = Instant::now();
        while began.elapsed() < within {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exit = Some(status);
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        self.exit = self.child.wait().ok();
    }
}

impl Drop for Copy<'_> {
    /// A copy still running when its test ends (a failed assertion) is quit, then killed.
    fn drop(&mut self) {
        if self.exit.is_some() {
            return;
        }
        let mut command = self.run.program();
        command.args(["menu", "Quit"]);
        let _ = run_to_end(command, None, Duration::from_secs(20));
        if self.wait_exit(QUIT_WITHIN).is_none() {
            let _ = writeln!(std::io::stderr(), "e2e: {}: the copy didn't quit; killed it", self.run.name);
            self.kill();
        }
    }
}

/// A start of the app that is neither waited for nor quit through the socket, which, on a folder
/// another copy has, reaches that copy: a later start, which is to end by itself. Killed if it is
/// still running when it is dropped.
struct Started {
    child: Child,
    exit: Option<ExitStatus>,
    err: PathBuf,
}

impl Started {
    /// How it ended, if it has.
    fn ended(&mut self) -> Option<ExitStatus> {
        if self.exit.is_none()
            && let Ok(Some(status)) = self.child.try_wait()
        {
            self.exit = Some(status);
        }
        self.exit
    }

    /// Waits for it to end by itself, for at most `within`.
    fn ended_within(&mut self, within: Duration) -> Option<ExitStatus> {
        let began = Instant::now();
        loop {
            if let Some(status) = self.ended() {
                return Some(status);
            }
            if began.elapsed() >= within {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// What it wrote to its standard error.
    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.err).unwrap_or_default()
    }
}

impl Drop for Started {
    fn drop(&mut self) {
        if self.ended().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Quitted {
    /// It closed by itself with nothing wrong: exit code 0, no crash or graphics error in its
    /// log or on its standard error, no reply to sp left unwritten. (The log has no tag of its own for a problem: a crash is
    /// `[crash]`, and every problem logged is also written to the standard error.)
    fn assert_clean(&self, run: &Run) {
        assert!(self.status.success(), "the copy exited with {}{}", self.status, run.context());
        for bad in ["[crash]", "panicked", "Graphics error"] {
            assert!(!self.log.contains(bad), "the copy's log says {bad:?}{}", run.context());
            assert!(!self.stderr.contains(bad), "the copy's standard error says {bad:?}{}", run.context());
        }
        // Every reply to sp was written before it went (`control::finish_replies`).
        assert!(!self.log.contains("still unwritten"), "the copy went with replies unwritten{}", run.context());
        // Looked at without following it: on Windows a socket is a stand-in file `exists` can't
        // follow.
        let socket = std::fs::symlink_metadata(&run.socket);
        assert!(socket.is_err(), "the copy left its socket behind: {}", run.socket.display());
        // The note that it was running, which says at the next start that it didn't quit.
        assert!(!run.data.join("running").exists(), "the copy left its running note behind{}", run.context());
    }
}

/// A reply from `sp … --json`.
#[derive(Debug)]
struct Reply {
    words: String,
    ok: bool,
    text: String,
    data: Value,
    took: Duration,
}

/// Runs a command to its end, with a line on its standard input if given; gives up (and kills
/// it) after `within`. The exit status (`None` when it took too long), what it wrote, and how
/// long it took.
fn run_to_end(
    mut command: Command,
    input: Option<&str>,
    within: Duration,
) -> (Option<ExitStatus>, String, String, Duration) {
    let began = Instant::now();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap_or_else(|e| panic!("could not run {}: {e}", *APP));
    if let (Some(line), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = writeln!(stdin, "{line}");
    }
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let out = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if began.elapsed() < within => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let took = began.elapsed();
    (status, out.join().unwrap_or_default(), err.join().unwrap_or_default(), took)
}

// ---------------------------------------------------------------------------------------------
// Files the tests make

/// Song text for a test song: its groups, a slide each, with words written for the test.
fn song_text(groups: &[(&str, &str)]) -> String {
    groups.iter().map(|(group, words)| format!("[{group}]\n{words}\n")).collect::<Vec<_>>().join("\n")
}

/// A small PNG of one colour.
fn write_png(path: &Path, rgb: [u8; 3]) {
    let picture = image::RgbImage::from_pixel(64, 36, image::Rgb(rgb));
    picture.save_with_format(path, image::ImageFormat::Png).expect("the picture");
}

/// A WAV file of `seconds` of silence (mono, 16 bits, 8000 samples a second).
fn write_wav(path: &Path, seconds: u32) {
    let rate = 8000u32;
    let bytes = rate * seconds * 2;
    let mut wav = Vec::with_capacity(44 + bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // one channel
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    wav.resize(44 + bytes as usize, 0);
    std::fs::write(path, wav).expect("the sound file");
}

/// The song files in a data folder's Songs folder, and any half-written files beside them.
fn song_files(data: &Path) -> (usize, Vec<String>) {
    let mut songs = 0;
    let mut half = Vec::new();
    for folder in [data.to_path_buf(), data.join("Songs")] {
        for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".saving") || name.ends_with(".part") {
                half.push(name);
            } else if name.ends_with(".sppres") {
                songs += 1;
            }
        }
    }
    (songs, half)
}

/// A song file's text, found by its title in the copy's Songs folder.
fn song_file_with(data: &Path, words: &str) -> Option<String> {
    std::fs::read_dir(data.join("Songs"))
        .ok()?
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .find(|text| text.contains(words))
}

// ---------------------------------------------------------------------------------------------
// The tests

#[test]
fn starts_answers_and_quits_cleanly() {
    let Some(run) = Run::new("starts_answers_and_quits_cleanly") else { return };
    let copy = run.start(&[]);
    let status = copy.status();
    assert!(status["live"].is_null(), "nothing on screen at the start: {status}");
    assert_eq!(status["playlist"]["name"], "Service", "the first playlist");
    assert_eq!(status["selected_song"], "Sample song", "the sample song is chosen");
    assert!(status["token"].as_str().is_some_and(|token| token.split('-').count() == 3), "a token: {status}");
    let songs = copy.ok(&["songs"]).data;
    assert_eq!(songs.as_array().map(Vec::len), Some(1), "the sample song alone: {songs}");
    // Refusals carry exit code 1 (`sp` checks every reply's exit code against `ok`).
    copy.refused(&["go", "nosuch"], "No song called");
    // A first word that is neither a command nor a file is a mistake, exit code 2, said by sp.
    let mut mistake = run.program();
    mistake.arg("nosuch");
    let (code, _, stderr, _) = run_to_end(mistake, None, COMMAND_WITHIN);
    assert_eq!(code.and_then(|c| c.code()), Some(2), "{stderr}");
    let quitted = copy.quit();
    quitted.assert_clean(&run);
    assert!(quitted.log.contains("[control] listening for commands"), "{}", quitted.log);
    // On a Wayland desktop the window library says nothing of files dragged onto the window, so
    // the app hears of them itself, through its windows' clipboard (and has stopped, without a
    // crash, by the time it has quit). Anywhere else it leaves them to the library.
    // Which of the two the window is on is the window library's choice (it takes X11 when a
    // Wayland desktop is named but can't be reached), so the copy's own log is asked, not the
    // environment: on Linux it says one or the other.
    let listened = quitted.log.contains("[drop] listening for files dragged in on Wayland");
    let not_wayland = quitted.log.contains("[drop] the window is not on Wayland");
    let one = cfg!(target_os = "linux") && (listened != not_wayland);
    let neither = !cfg!(target_os = "linux") && !listened && !not_wayland;
    assert!(one || neither, "listening for dragged files on Wayland, and only there{}", run.context());
    if listened {
        say("the copy listened for files dragged in on Wayland");
    } else if not_wayland {
        say("the copy's window was not on Wayland: listening for dragged files was not checked");
    }
    let gpu = quitted.log.lines().find_map(|line| line.split_once("[gpu] ").map(|(_, said)| said.to_owned()));
    let gpu = gpu.unwrap_or_else(|| panic!("the log names the graphics it drew with{}", run.context()));
    // Said in the output, so a CI run shows what it drew with (WARP, Mesa's lavapipe…).
    say(&format!("the copy drew with {gpu}"));
    // With the copy gone, sp says so (exit code 1) and starts nothing.
    run.sp(&["status"]).ok.then(|| panic!("a copy still answers after quitting{}", run.context()));
}

/// The operator's window's surface is set up again at the window's size whenever the two differ,
/// which waits for the GPU to come idle and is lost when the screens' thread hands in work at
/// that moment. A lost set-up left the window drawing for its own size onto a surface of the
/// size before, which the GPU refused on every frame from then on (seen on Windows, where a
/// new window takes its size a moment after it opens). The testing aid starts the surface at
/// half the window's size and loses the next eight set-ups: more than a new window's first
/// events bring (it is told its size a few times, and each time set its surface up again, which
/// hid a lost set-up on a Mac). Each is tried again later, the wait doubling, about three
/// seconds in all.
///
/// What it shows everywhere: every lost set-up was tried again, the surface ended at the
/// window's size (not the half it began at), and nothing the GPU refused was drawn meanwhile.
/// What it shows only where the window is given pictures (not on a Mac whose quiet copy is
/// covered or whose display sleeps, which it says): frames drawn after it, at that size.
#[test]
fn a_window_whose_surface_set_up_is_lost_draws_nothing_wrong_and_follows_its_size_after() {
    let Some(run) = Run::new("a_window_whose_surface_set_up_is_lost_draws_nothing_wrong_and_follows_its_size_after")
    else {
        return;
    };
    const FOLLOWS: &str = "Testing aid: the operator's window's surface follows its window again, at ";
    const DREW: &str = "Testing aid: the operator's window was drawn at ";
    let copy = run.start(&[("SOFTPRESENTER_SURFACE_RACE", "8")]);
    // Something to draw, until the set-ups are all lost, one has worked and a frame was drawn.
    let said = |what: &str| run.log().contains(what);
    let began = Instant::now();
    let mut followed_at = None;
    let drew = loop {
        copy.ok(&["go", "Sample song"]);
        copy.ok(&["next"]);
        copy.ok(&["clear", "all"]);
        if followed_at.is_none() && said(FOLLOWS) {
            followed_at = Some(Instant::now());
        }
        // A window that is given no picture (below) still sets its surface up; its first frame
        // is waited for a little longer than that.
        let over = match followed_at {
            Some(at) => at.elapsed() > Duration::from_secs(5),
            None => began.elapsed() > Duration::from_secs(60),
        };
        if said(DREW) || over {
            break said(DREW);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let quitted = copy.quit();
    let log = &quitted.log;
    let lost = log.matches("Testing aid: a set-up of the operator's window's surface was lost").count();
    assert_eq!(lost, 8, "the eight set-ups were lost, each tried again later{}", run.context());
    // "1440x806" after `before` in the first log line that has it.
    let size_after = |before: &str| -> Option<(u32, u32)> {
        let rest = log.split_once(before)?.1;
        let (w, rest) = rest.split_once('x')?;
        let h: String = rest.chars().take_while(char::is_ascii_digit).collect();
        Some((w.parse().ok()?, h.parse().ok()?))
    };
    let began_at = size_after("Testing aid: the operator's window's surface starts at ");
    let window = size_after(", half its window's ");
    let followed = size_after(FOLLOWS);
    assert!(began_at.is_some() && window.is_some(), "the aid said the sizes it began with{}", run.context());
    assert_ne!(began_at, window, "the surface began smaller than its window{}", run.context());
    assert_eq!(followed, window, "the surface was set up at the window's size in the end{}", run.context());
    quitted.assert_clean(&run);
    if drew {
        assert_eq!(size_after(DREW), window, "the frames after were drawn at the window's size{}", run.context());
    } else {
        // A Mac gives a window covered by others no picture to draw into, and a quiet copy's
        // windows open behind everything.
        say_skipped(
            "the frames after a lost set-up",
            "the copy's window was given no picture to draw into (covered, or the display is asleep)",
        );
    }
}

#[test]
fn sp_is_answered_menu_quit_however_late_the_reply_is_written() {
    let Some(run) = Run::new("sp_is_answered_menu_quit_however_late_the_reply_is_written") else { return };
    // Each reply is written half a second late (SOFTPRESENTER_SLOW_REPLIES), as when the thread
    // that writes it runs late on a busy computer. `menu Quit`'s is handed over as the app begins
    // to leave, and the app had ended by then: sp read nothing ("EOF while parsing").
    let copy = run.start(&[("SOFTPRESENTER_SLOW_REPLIES", "500")]);
    copy.quit().assert_clean(&run);
}

#[test]
fn a_run_that_was_closed_from_outside_is_said_once_at_the_next_start() {
    let Some(run) = Run::new("a_run_that_was_closed_from_outside_is_said_once_at_the_next_start") else { return };
    let mut copy = run.start(&[]);
    // Killed, as Task Manager or a shutdown closes it: it leaves its note that it was running.
    copy.kill();
    assert!(run.data.join("running").exists(), "a copy that didn't quit leaves its note{}", run.context());
    let copy = run.start(&[]);
    let quitted = copy.quit();
    quitted.assert_clean(&run);
    let said: Vec<&str> = quitted.log.lines().filter(|line| line.contains("[start] the last run")).collect();
    assert_eq!(said.len(), 1, "said once, by the second copy: {said:?}{}", run.context());
    assert!(
        said[0].contains(", started ")
            && said[0].contains(") ended without quitting: it crashed, or was closed from outside ("),
        "{}",
        said[0]
    );
}

// ---------------------------------------------------------------------------------------------
// One copy per data folder

/// What a start turned away by a copy already running on its folder says on its standard error.
const TURNED_AWAY: &str = "SoftPresenter is already open with this data folder";
/// What the copy running says in its log when a start turned away asks for it.
const ASKED_FORWARD: &str = "[start] another start of SoftPresenter";
/// What a start turned away by a copy that never answered adds to that copy's log (only the test
/// on a Mac and Linux, where it can hold the folder itself, reads it).
#[cfg(unix)]
const NO_ANSWER: &str = "[start] a start of SoftPresenter turned away by this copy got no answer";
/// How long a start turned away may take to end: it asks the copy running to come to the front,
/// for ten seconds at most if that one doesn't answer; the rest is for a slow, busy computer.
const TURNED_AWAY_WITHIN: Duration = Duration::from_secs(40);

/// How many starts opened the log (and so went on to open anything at all).
fn starts_logged(log: &str) -> usize {
    log.lines().filter(|line| line.contains("[start] SoftPresenter ")).count()
}

/// Waits for a start on a folder a copy already has to end by itself, and says how long it took.
fn turned_away(run: &Run, start: &mut Started, what: &str) -> ExitStatus {
    let began = Instant::now();
    let status = start.ended_within(TURNED_AWAY_WITHIN).unwrap_or_else(|| {
        panic!("{what} was still running {TURNED_AWAY_WITHIN:?} later: a second copy opened{}", run.context())
    });
    say(&format!("{}: {what} ended after {:.1} s", run.name, began.elapsed().as_secs_f64()));
    status
}

#[test]
fn a_second_start_on_the_same_folder_brings_the_first_to_the_front_and_opens_nothing() {
    let Some(run) = Run::new("a_second_start_on_the_same_folder_brings_the_first_to_the_front_and_opens_nothing")
    else {
        return;
    };
    let first = run.start(&[]);
    let mut second = run.launch(&[]);
    let status = turned_away(&run, &mut second, "the second start");
    assert!(status.success(), "the second start ended with {status}{}", run.context());
    assert!(second.stderr().contains(TURNED_AWAY), "it said why: {:?}{}", second.stderr(), run.context());
    // The first was asked to come to the front (a quiet copy stays behind, and says so), and
    // still answers.
    first.status();
    let log = run.log();
    assert_eq!(log.matches(ASKED_FORWARD).count(), 1, "the first was asked once{}", run.context());
    // The second never opened the log, which comes before anything else it would open.
    assert_eq!(starts_logged(&log), 1, "only the first start opened the log{}", run.context());
    // Nothing of the second's was left behind: the first's quit leaves no socket and no note.
    first.quit().assert_clean(&run);
}

#[test]
fn three_starts_at_once_on_one_folder_leave_exactly_one_copy_running() {
    let Some(run) = Run::new("three_starts_at_once_on_one_folder_leave_exactly_one_copy_running") else { return };
    let mut starts: Vec<Started> = (0..3).map(|_| run.launch(&[])).collect();
    // Two end by themselves, with success; one goes on.
    let began = Instant::now();
    loop {
        let ended = starts.iter_mut().filter_map(Started::ended).count();
        if ended >= 2 {
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "only {ended} of the three starts had ended after a minute{}",
            run.context()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    say(&format!("{}: two starts ended after {:.1} s", run.name, began.elapsed().as_secs_f64()));
    let mut ended: Vec<ExitStatus> = Vec::new();
    let mut going: Vec<&mut Started> = Vec::new();
    for start in &mut starts {
        match start.ended() {
            Some(status) => ended.push(status),
            None => going.push(start),
        }
    }
    assert_eq!(ended.len(), 2, "two ended: {ended:?}{}", run.context());
    assert!(ended.iter().all(ExitStatus::success), "both with success: {ended:?}{}", run.context());
    assert_eq!(going.len(), 1, "one copy is running{}", run.context());
    let copy = going.pop().expect("the copy running");
    // It answers, and is the only one: quitting it ends the last of the three.
    let answers = Instant::now();
    while !run.sp(&["status"]).ok {
        assert!(answers.elapsed() < START_WITHIN, "the copy running didn't answer{}", run.context());
        assert!(copy.ended().is_none(), "the copy running stopped{}", run.context());
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(starts_logged(&run.log()), 1, "only one start opened the log{}", run.context());
    assert!(run.sp(&["menu", "Quit"]).ok, "menu Quit{}", run.context());
    let status = copy.ended_within(QUIT_WITHIN).unwrap_or_else(|| panic!("it didn't quit{}", run.context()));
    let quitted = Quitted { status, log: run.log(), stderr: copy.stderr() };
    quitted.assert_clean(&run);
}

#[test]
fn starts_on_two_data_folders_both_run() {
    let Some(run) = Run::new("starts_on_two_data_folders_both_run") else { return };
    let other = run.beside();
    let first = run.start(&[]);
    let second = other.start(&[]);
    // Each answers on its own socket, and neither was asked to come to the front.
    first.status();
    second.status();
    assert!(!run.log().contains(ASKED_FORWARD) && !other.log().contains(ASKED_FORWARD), "{}", run.context());
    second.quit().assert_clean(&other);
    first.quit().assert_clean(&run);
}

#[test]
fn the_command_line_works_as_before_while_a_copy_runs() {
    let Some(run) = Run::new("the_command_line_works_as_before_while_a_copy_runs") else { return };
    let copy = run.start(&[]);
    // sp's commands, with ctl and without, and the help sp prints itself.
    assert!(run.sp(&["ctl", "status"]).ok, "ctl status{}", run.context());
    assert!(run.sp(&["status"]).ok, "status{}", run.context());
    let help = run.sp(&["help"]).text;
    assert!(help.contains("sp COMMAND"), "sp help{}", run.context());
    // `front --start` is for a start turned away, and not offered to people.
    assert!(help.contains("  front ") && !help.contains("--start"), "{help}");
    // The app's own help, and a list of the displays, run and end as before.
    let mut help = run.program();
    help.arg("--help");
    let (status, stdout, _, _) = run_to_end(help, None, COMMAND_WITHIN);
    assert!(status.is_some_and(|status| status.success()), "--help ended with {status:?}");
    assert!(stdout.contains("Usage: softpresenter"), "--help printed the help: {stdout:?}");
    let mut displays = run.program();
    displays.arg("--list-displays");
    let (status, _, stderr, _) = run_to_end(displays, None, COMMAND_WITHIN);
    assert!(status.is_some_and(|status| status.success()), "--list-displays ended with {status:?}: {stderr}");
    assert!(!stderr.contains(TURNED_AWAY), "--list-displays wasn't turned away: {stderr}");
    let log = run.log();
    assert_eq!(starts_logged(&log), 2, "the list of the displays opened the log, as before{}", run.context());
    assert!(!log.contains(ASKED_FORWARD), "nothing asked the copy to come to the front{}", run.context());
    // It leaves the copy's note that it is running alone: on Windows, where the note isn't
    // locked, it took it for one a crash left behind, said so, and removed it as it ended.
    for said in ["[start] the last run", "another SoftPresenter is running with this folder"] {
        assert!(!log.contains(said), "the list of the displays left the running note alone{}", run.context());
    }
    // `sp front` itself: a quiet copy's window stays behind, and says so; a word it doesn't take is
    // refused with how it goes.
    assert_eq!(copy.ok(&["front"]).text, "A quiet copy's window stays behind everything.");
    copy.refused(&["front", "now"], "front takes nothing after it");
    copy.quit().assert_clean(&run);
}

/// A run that only saves a picture leaves the copy running alone: it asks nothing of it, and ends
/// with a failure, having saved no picture.
#[test]
fn a_capture_on_a_folder_in_use_saves_no_picture_and_leaves_the_copy_alone() {
    let Some(run) = Run::new("a_capture_on_a_folder_in_use_saves_no_picture_and_leaves_the_copy_alone") else {
        return;
    };
    let copy = run.start(&[]);
    let picture = run.file("home.png");
    let mut capture = run.program();
    capture.arg("--capture-home").arg(&picture);
    let (status, _, stderr, _) = run_to_end(capture, None, TURNED_AWAY_WITHIN);
    assert_eq!(status.and_then(|status| status.code()), Some(1), "ended with {status:?}: {stderr}{}", run.context());
    assert!(stderr.contains(TURNED_AWAY), "it said why: {stderr:?}");
    assert!(!picture.exists(), "no picture was saved");
    let log = run.log();
    assert!(!log.contains(ASKED_FORWARD), "the copy running wasn't asked to come forward{}", run.context());
    assert_eq!(starts_logged(&log), 1, "the capture never opened the log{}", run.context());
    copy.quit().assert_clean(&run);
}

/// A copy that is quitting (here, waiting for its songs to be written to a slow disk) refuses to
/// come to the front, so a start made meanwhile, as a person restarting it makes, waits for it to
/// go and then opens the app, rather than ending with nothing open.
///
/// The copy must still be quitting when it is asked. Its songs are written behind the import, so a
/// Quit it takes late quits at once: on GitHub's Windows machines, which draw in software, its
/// window's thread was held drawing for 3.3 s after the import, the Quit was taken only once the
/// 2.4 s of writing then asked for were done, and the copy had gone before it could be asked (`sp`
/// found nothing listening; a test PC drawing the same way held it 7.3 s). So the copy is first
/// waited for until it answers promptly, the writing is long (16 s), the copy is asked only once
/// its log says the Quit waits for it, and the second start is made once four seconds or less of it
/// are left, so the first has gone within the ten seconds that start goes on asking.
#[test]
fn a_start_while_the_copy_running_quits_opens_the_app_once_that_copy_has_gone() {
    /// Each written 100 ms late (SOFTPRESENTER_SLOW_WRITES): 16 s of writing after the import.
    const SONGS: usize = 160;
    const SLOW_MS: u64 = 100;
    /// The second start is made once this many songs or fewer are left to write.
    const LEFT_AT_SECOND_START: usize = 40;
    let Some(run) = Run::new("a_start_while_the_copy_running_quits_opens_the_app_once_that_copy_has_gone") else {
        return;
    };
    let folder = run.file("Songs to write");
    std::fs::create_dir_all(&folder).expect("the folder of songs");
    for n in 1..=SONGS {
        let words = song_text(&[("Verse 1", &format!("The verse of closing test song {n}"))]);
        std::fs::write(folder.join(format!("Closing Song {n:03}.txt")), words).expect("a song");
    }
    let mut first = run.start(&[("SOFTPRESENTER_SLOW_WRITES", &SLOW_MS.to_string())]);
    // Past its first drawings, which on a computer that draws in software hold its window's
    // thread, and so its answers, for seconds (as in the big import's test).
    let settled = Instant::now();
    let mut prompt = 0;
    while prompt < 3 {
        let took = first.ok(&["status"]).took;
        prompt = if took < Duration::from_millis(500) { prompt + 1 } else { 0 };
        assert!(settled.elapsed() < START_WITHIN, "the copy never answered promptly{}", run.context());
    }
    let imported = Instant::now();
    first.ok(&["song", "import", &folder.to_string_lossy()]);
    assert!(run.sp(&["menu", "Quit"]).ok, "menu Quit{}", run.context());
    say(&format!("{}: Quit answered {:.1} s after the import", run.name, imported.elapsed().as_secs_f64()));
    // The quit waits for the songs to be written: its log says so after a second.
    let waiting = Instant::now();
    while !run.log().contains("[saving] waiting for") {
        assert!(
            first.child.try_wait().ok().flatten().is_none() && waiting.elapsed() < Duration::from_secs(30),
            "the copy quit without waiting for its songs ({} of {SONGS} written): the Quit came after they were{}",
            song_files(&run.data).0,
            run.context()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // While it writes them it still answers, and refuses to come to the front.
    let refused = run.sp(&["front", "--start"]);
    assert!(
        !refused.ok && refused.text.contains("closing"),
        "{refused:?} ({} of {SONGS} songs written){}",
        song_files(&run.data).0,
        run.context()
    );
    while song_files(&run.data).0 + LEFT_AT_SECOND_START < SONGS && first.child.try_wait().ok().flatten().is_none() {
        std::thread::sleep(Duration::from_millis(50));
    }
    say(&format!(
        "{}: the second start made {:.1} s after the import, {} of {SONGS} songs written",
        run.name,
        imported.elapsed().as_secs_f64(),
        song_files(&run.data).0
    ));
    let mut second = run.launch(&[]);
    let status = first.wait_exit(Duration::from_secs(60)).unwrap_or_else(|| panic!("it didn't quit{}", run.context()));
    assert!(status.success(), "the first ended with {status}{}", run.context());
    // The second takes the folder once the first has gone, and opens.
    let answers = Instant::now();
    while !run.sp(&["status"]).ok {
        assert!(
            second.ended().is_none(),
            "the second start ended instead of opening: {:?}{}",
            second.stderr(),
            run.context()
        );
        assert!(answers.elapsed() < START_WITHIN, "the second start never answered{}", run.context());
        std::thread::sleep(Duration::from_millis(100));
    }
    let log = run.log();
    assert!(!log.contains(ASKED_FORWARD), "nothing was brought to the front{}", run.context());
    assert_eq!(starts_logged(&log), 2, "both starts opened the log{}", run.context());
    assert!(run.sp(&["menu", "Quit"]).ok, "menu Quit{}", run.context());
    let status = second.ended_within(QUIT_WITHIN).unwrap_or_else(|| panic!("it didn't quit{}", run.context()));
    let quitted = Quitted { status, log: run.log(), stderr: second.stderr() };
    quitted.assert_clean(&run);
}

/// A start whose folder a copy holds that never answers (one that hangs, or whose control socket
/// never came up) ends once its ten seconds are up, without opening anything, and says so: on its
/// standard error, and in that copy's log. (On Windows it says so in a message as well, which a
/// quiet run never shows.)
#[cfg(unix)]
#[test]
fn a_start_held_off_by_a_copy_that_never_answers_ends_and_says_so() {
    let Some(run) = Run::new("a_start_held_off_by_a_copy_that_never_answers_ends_and_says_so") else { return };
    // The test holds the folder as a copy would, and answers nothing; the log stands in for that
    // copy's.
    std::fs::create_dir_all(&run.data).expect("the data folder");
    let hold = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(run.data.join("one-copy.lock"))
        .expect("the lock file");
    hold.try_lock().expect("the folder held");
    std::fs::write(run.data.join("SoftPresenter.log"), "").expect("a log");
    let began = Instant::now();
    let mut start = run.launch(&[]);
    let status = turned_away(&run, &mut start, "the start");
    assert!(status.success(), "it ended with {status}{}", run.context());
    assert!(began.elapsed() >= Duration::from_secs(9), "it asked for its ten seconds: {:?}", began.elapsed());
    assert!(start.stderr().contains("didn't answer"), "it said why: {:?}", start.stderr());
    let log = run.log();
    assert_eq!(log.matches(NO_ANSWER).count(), 1, "said once in the log: {log:?}");
    assert_eq!(starts_logged(&log), 0, "it opened nothing{}", run.context());
    drop(hold);
}

/// The system lets the hold go when the copy holding it ends without quitting (killed, as Task
/// Manager or a crash ends it), so the next start opens the app.
#[test]
fn once_the_copy_running_is_killed_the_next_start_opens_the_app() {
    let Some(run) = Run::new("once_the_copy_running_is_killed_the_next_start_opens_the_app") else { return };
    let mut first = run.start(&[]);
    let mut turned = run.launch(&[]);
    let status = turned_away(&run, &mut turned, "a start while the first runs");
    assert!(status.success(), "it ended with {status}{}", run.context());
    first.kill();
    let second = run.start(&[]);
    second.status();
    let quitted = second.quit();
    quitted.assert_clean(&run);
    assert_eq!(starts_logged(&quitted.log), 2, "the first and the last start opened the log{}", run.context());
}

#[test]
fn a_file_an_import_refuses_is_logged_with_its_size_and_kind_but_not_its_words() {
    let Some(run) = Run::new("a_file_an_import_refuses_is_logged_with_its_size_and_kind_but_not_its_words") else {
        return;
    };
    // Words written for this test, which the log must never hold.
    const WORDS: &str = "Plinth morrow quaver";
    let bible = run.file("Made Up.bbl");
    let mut sqlite = b"SQLite format 3\0".to_vec();
    sqlite.extend(WORDS.as_bytes());
    sqlite.resize(2048, 0);
    std::fs::write(&bible, &sqlite).unwrap();
    let song = run.file("Made Up.txt");
    std::fs::write(&song, [WORDS.as_bytes(), &[0, 1, 2, 3, 0, 0, 7, 0, 0, 1, 0, 0]].concat()).unwrap();
    let picture = run.file("Made Up.png");
    std::fs::write(&picture, format!("{WORDS}\n")).unwrap();
    let copy = run.start(&[]);
    for (words, file) in [(["bible", "install"], &bible), (["song", "import"], &song), (["media", "add"], &picture)] {
        let reply = copy.sp(&[words[0], words[1], &file.to_string_lossy()]);
        assert!(!reply.ok, "sp {} took a file it can't read: {}", reply.words, reply.text);
        assert!(
            !reply.text.contains(" (2 KB") && !reply.text.contains("bytes, "),
            "only the log says more: {}",
            reply.text
        );
    }
    let quitted = copy.quit();
    quitted.assert_clean(&run);
    let line = |area: &str, ending: &str| {
        quitted.log.lines().find(|line| line.contains(area) && line.ends_with(ending)).map(str::to_owned)
    };
    let found = [
        line("[bible] Made Up.bbl ", " (2 KB, SQLite)"),
        line("[songs] not imported: Made Up.txt ", " (32 bytes, binary)"),
        line("[songs] imported 0 songs from 1 file in ", " ms (1 file not imported)"),
        line("[media] could not add ", " (21 bytes, UTF-8 text)"),
    ];
    for (found, what) in found.iter().zip(["the Bible", "the song", "the import", "the picture"]) {
        assert!(found.is_some(), "the log says why {what} was refused{}", run.context());
    }
    assert!(!quitted.log.contains(WORDS), "the log holds a file's words{}", run.context());
}

/// The six layers `clear` takes, in the order of [`filled`].
const LAYERS: [&str; 6] = ["slide", "media", "props", "messages", "audio", "announcements"];

/// Which of the six layers have something on them, from `status --json`.
fn filled(status: &Value) -> [bool; 6] {
    [
        !status["live"].is_null(),
        !status["media"].is_null(),
        status["props"].as_array().is_some_and(|props| !props.is_empty()),
        !status["message"].is_null(),
        !status["audio"]["track"].is_null(),
        !status["announcements"].is_null(),
    ]
}

/// Plays a track. `false`, said, when this computer has no sound output to play it through (CI's
/// machines have none); any other refusal fails the test.
fn play_or_skip(copy: &Copy, track: &str, what: &str) -> bool {
    let reply = copy.sp(&["audio", "play", track]);
    if reply.ok {
        return true;
    }
    if reply.text.contains("sound output") {
        say_skipped(what, &format!("this computer has no sound output ({})", reply.text));
        return false;
    }
    panic!("audio play {track} was refused: {}{}", reply.text, copy.run.context());
}

#[test]
fn a_song_from_text_runs_slide_by_slide_and_each_layer_clears() {
    let Some(run) = Run::new("a_song_from_text_runs_slide_by_slide_and_each_layer_clears") else { return };
    // Sung as written: its chorus comes after its only verse (a chorus written once before a
    // second verse is sung after that one too: `a_chorus_written_once_is_sung_after_each_verse`).
    let words = song_text(&[
        ("Verse 1", "Morning light on the hill\nWe walk the quiet road"),
        ("Chorus", "Sing out, the day is new"),
        ("Ending", "Evening comes with gentle rain"),
    ]);
    let file = run.file("Hill Road.txt");
    std::fs::write(&file, words).expect("the song's file");
    let picture = run.file("Blue.png");
    write_png(&picture, [30, 90, 200]);
    let tone = run.file("Tone.wav");
    write_wav(&tone, 30);
    let copy = run.start(&[]);

    let imported = copy.ok(&["song", "import", &file.to_string_lossy()]);
    assert!(imported.text.contains("Hill Road"), "{}", imported.text);
    let slides = copy.ok(&["slides", "Hill Road"]).data;
    let groups: Vec<&str> = slides.as_array().into_iter().flatten().filter_map(|s| s["group"].as_str()).collect();
    assert_eq!(groups, ["Verse 1", "Chorus", "Ending"], "{slides}");

    // Through the song, and back, as the arrow keys, Home and End do.
    copy.ok(&["go", "Hill Road"]);
    copy.assert_live("Hill Road", 1);
    let status = copy.status();
    assert_eq!(status["live"]["words"], "Morning light on the hill / We walk the quiet road", "{status}");
    assert_eq!(status["live"]["kind"], "song");
    assert_eq!(status["next_slide"], 2, "{status}");
    copy.ok(&["next"]);
    copy.assert_live("Hill Road", 2);
    copy.ok(&["next"]);
    copy.assert_live("Hill Road", 3);
    let end = copy.ok(&["next"]);
    assert_eq!(end.data["at_end"], true, "at the end Next says so: {end:?}");
    copy.assert_live("Hill Road", 3);
    copy.ok(&["previous"]);
    copy.assert_live("Hill Road", 2);
    copy.ok(&["first"]);
    copy.assert_live("Hill Road", 1);
    copy.ok(&["last"]);
    copy.assert_live("Hill Road", 3);
    copy.ok(&["go", "Hill Road", "2"]);
    copy.assert_live("Hill Road", 2);
    copy.refused(&["go", "Hill Road", "9"], "there is no slide 9");
    copy.assert_live("Hill Road", 2);
    copy.ok(&["go", "Hill Road", "1"]);
    copy.ok(&["group", "chorus"]);
    copy.assert_live("Hill Road", 2);
    // A slide's words changed while it is on screen: shown at once, and saved.
    copy.ok(&["slide", "text", "Hill Road", "2", "Sing out, the day is here"]);
    assert_eq!(copy.status()["live"]["words"], "Sing out, the day is here");

    // Something on every layer.
    copy.ok(&["media", "add", &picture.to_string_lossy()]);
    copy.ok(&["media", "show", "blue"]);
    copy.ok(&["prop", "new", "Banner"]);
    copy.ok(&["prop", "show", "Banner"]);
    copy.ok(&["message", "show", "--text", "Welcome, friends"]);
    copy.ok(&["announce", "Sample song"]);
    copy.ok(&["audio", "add", &tone.to_string_lossy()]);
    let sound =
        play_or_skip(&copy, "tone", "a_song_from_text_runs_slide_by_slide_and_each_layer_clears: the audio layer");
    let mut expected = [true, true, true, true, sound, true];
    assert_eq!(filled(&copy.status()), expected, "every layer has something: {}", copy.status());
    // Each clear takes its own layer, and leaves the others.
    for (at, layer) in LAYERS.iter().enumerate() {
        let cleared = copy.ok(&["clear", layer]);
        assert_eq!(cleared.text, format!("Cleared {layer}."));
        expected[at] = false;
        let status = copy.status();
        assert_eq!(filled(&status), expected, "after clear {layer} ({LAYERS:?}): {status}");
    }
    copy.refused(&["clear", "bogus"], "isn't a layer");
    // Everything again, then Clear All.
    copy.ok(&["go", "Hill Road", "3"]);
    copy.ok(&["media", "show", "blue"]);
    copy.ok(&["prop", "show", "Banner"]);
    copy.ok(&["message", "show", "--text", "Welcome, friends"]);
    copy.ok(&["announce", "Sample song"]);
    if sound {
        copy.ok(&["audio", "play", "tone"]);
    }
    assert_eq!(filled(&copy.status()), [true, true, true, true, sound, true]);
    copy.ok(&["clear"]);
    assert_eq!(filled(&copy.status()), [false; 6], "Clear All: {}", copy.status());

    copy.quit().assert_clean(&run);
    let saved = song_file_with(&run.data, "Morning light on the hill").expect("the song's file in Songs");
    assert!(saved.contains("Sing out, the day is here"), "the changed words were saved: {saved}");
    assert!(!saved.contains("the day is new"), "{saved}");
}

#[test]
fn a_bible_passage_goes_on_screen_and_steps_verse_by_verse() {
    let Some(run) = Run::new("a_bible_passage_goes_on_screen_and_steps_verse_by_verse") else { return };
    let copy = run.start(&[]);
    // The King James Version comes with the app (public domain).
    let bibles = copy.ok(&["bibles"]).data;
    let shipped: Vec<&str> =
        bibles["installed"].as_array().into_iter().flatten().filter_map(|b| b["short"].as_str()).collect();
    assert!(shipped.contains(&"KJV"), "the Bibles that come with the app: {bibles}");
    copy.ok(&["bible", "rows", "KJV"]);
    let shown = copy.ok(&["bible", "John 3:16", "--show"]);
    assert!(shown.text.contains("slide 16 of 36"), "{}", shown.text);
    let status = copy.status();
    assert_eq!(status["live"]["kind"], "passage", "{status}");
    copy.assert_live("John 3:1-36", 16);
    let words = status["live"]["words"].as_str().unwrap_or_default();
    assert!(words.starts_with("For God so loved the world"), "{words}");
    copy.ok(&["next"]);
    copy.assert_live("John 3:1-36", 17);
    let words = copy.status()["live"]["words"].as_str().unwrap_or_default().to_owned();
    assert!(words.starts_with("For God sent not his Son into the world"), "{words}");
    copy.ok(&["next"]);
    copy.assert_live("John 3:1-36", 18);
    copy.ok(&["previous"]);
    copy.ok(&["previous"]);
    copy.assert_live("John 3:1-36", 16);
    // Read without touching the page or the screens.
    let text = copy.ok(&["bible", "text", "Psalm 117"]).data;
    assert_eq!(text["slides"].as_array().map(Vec::len), Some(2), "{text}");
    copy.assert_live("John 3:1-36", 16);
    let found = copy.ok(&["bible", "search", "everlasting life"]).data;
    assert!(found.as_array().into_iter().flatten().any(|hit| hit["verse"] == "John 3:16"), "{found}");
    copy.refused(&["bible", "John 99:1"], "John has 21 chapters");
    copy.ok(&["clear"]);
    assert!(copy.live().is_none());

    copy.quit().assert_clean(&run);
    let history = std::fs::read_to_string(run.data.join("bible-history.json")).unwrap_or_default();
    assert!(history.contains("John 3:16"), "the passage looked up is in the history: {history}");
}

/// The figures of a source's line saying how its pictures went out (`sp_ndi::Minute::words`): how
/// many new pictures it sent, how many came late, and the longest wait between two while they
/// moved, in milliseconds (`None` when nothing moved).
fn ndi_lines(log: &str, name: &str) -> Vec<(u32, u32, Option<f64>)> {
    let number = |text: &str, before: &str| -> Option<f64> {
        let upto = &text[..text.find(before)?];
        upto.rsplit(|c: char| !(c.is_ascii_digit() || c == '.')).next()?.parse().ok()
    };
    log.lines()
        .filter(|line| line.contains(&format!("NDI \"{name}\" at ")))
        .filter_map(|line| {
            let sent = number(line, " pictures sent")? as u32;
            let late = number(line, " late,")? as u32;
            Some((sent, late, number(line, " ms, ").filter(|_| line.contains("longest gap"))))
        })
        .collect()
}

#[test]
fn a_window_whose_pictures_come_late_never_holds_up_the_pictures_sent_over_ndi() {
    let name = "a_window_whose_pictures_come_late_never_holds_up_the_pictures_sent_over_ndi";
    // Pictures are timed to a few hundredths of a second here: not on the shared, slow
    // computers the automatic checks run on, where anything may come late.
    if std::env::var_os("GITHUB_ACTIONS").is_some() || std::env::var_os("CI").is_some() {
        say_skipped(name, "pictures aren't timed on the computers of the automatic checks");
        return;
    }
    let Some(run) = Run::new(name) else { return };
    let words = song_text(&[
        ("Verse 1", "Morning light on the hill"),
        ("Verse 2", "We walk the quiet road"),
        ("Verse 3", "Evening comes with gentle rain"),
        ("Verse 4", "Night falls soft on the town"),
        ("Verse 5", "Stars keep watch till the dawn"),
    ]);
    let file = run.file("Hill Road.txt");
    std::fs::write(&file, words).expect("the song's file");
    // A second screen goes to a stand-in for NDI that somebody is always watching, and says how
    // its pictures went out every two seconds; every picture the first screen's window asks
    // for comes 150 ms late, as when the window system holds a window's pictures back.
    let copy = run.start(&[
        ("SOFTPRESENTER_NDI", "Late window"),
        ("SOFTPRESENTER_PRETEND_NDI", "2"),
        ("SOFTPRESENTER_SLOW_PRESENT", "150"),
    ]);
    let gpu = run.log().lines().find(|line| line.contains("[gpu]")).unwrap_or_default().to_owned();
    if gpu.contains("software graphics") || gpu.contains(", Cpu,") {
        say_skipped(name, "this computer draws in software, too slowly to time pictures by");
        copy.quit().assert_clean(&run);
        return;
    }
    copy.ok(&["song", "import", &file.to_string_lossy()]);
    copy.ok(&["settings", "set", "transition=dissolve:2"]);
    // How busy this computer is meanwhile: the longest a sleep of 5 ms takes here. On one
    // building and testing other things at once it takes several times that, and so does
    // every wait in the copy, whatever the copy does.
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let theirs = done.clone();
    let busy = std::thread::spawn(move || {
        let mut longest = Duration::ZERO;
        while !theirs.load(Ordering::Relaxed) {
            let began = Instant::now();
            std::thread::sleep(Duration::from_millis(5));
            longest = longest.max(began.elapsed());
        }
        longest
    });
    copy.ok(&["go", "Hill Road"]);
    std::thread::sleep(Duration::from_millis(2600));
    // Four dissolves of two seconds, each some sixty pictures at NDI's pace, with a rest of
    // half a second or so on each verse, as an operator reading along rests.
    for _ in 0..4 {
        copy.ok(&["next"]);
        std::thread::sleep(Duration::from_millis(2600));
    }
    // The source's last line, and a moment for the window's last picture to come.
    std::thread::sleep(Duration::from_millis(2500));
    done.store(true, Ordering::Relaxed);
    let slept = busy.join().unwrap_or_default();
    let quitted = copy.quit();
    quitted.assert_clean(&run);
    let log = &quitted.log;
    // The window was held up, time after time, and said so...
    assert!(log.contains("comes 150 ms late"), "the switch was not taken{}", run.context());
    assert!(log.contains("was given its next picture after"), "the window was never passed over{}", run.context());
    // ...and NDI kept its own pace beside it. As it was, each time the window fell behind the
    // screens waited 50 ms for it, so a fade's pictures went out 20 a second with one in ten
    // late and one in fifteen standing still for two pictures' times or more, and every rest
    // between two fades was taken for a picture that stood still half a second. (A computer
    // busy with other tests may still be late with a picture or two: one is let pass.)
    let lines = ndi_lines(log, "Late window");
    let sent: u32 = lines.iter().map(|(sent, ..)| sent).sum();
    let late: u32 = lines.iter().map(|(_, late, _)| late).sum();
    let longest = lines.iter().filter_map(|(.., gap)| *gap).fold(0.0, f64::max);
    let stood: Vec<&str> = log.lines().filter(|line| line.contains("stood still")).collect();
    let more: u32 =
        stood.iter().filter_map(|line| line.split_once(" more since")?.0.rsplit('(').next()?.parse::<u32>().ok()).sum();
    let stood_still = stood.len() as u32 + more;
    say(&format!(
        "NDI beside a window 150 ms late: {sent} new pictures, {late} late, {stood_still} stood still, longest gap \
         {longest:.1} ms"
    ));
    assert!(longest > 0.0, "no line said how the pictures went out{}", run.context());
    if slept > Duration::from_millis(30) && (sent < 250 || late > 6 || stood_still > 1 || longest > 150.0) {
        let why = format!("this computer was too busy to time pictures by (a sleep of 5 ms took {slept:.0?})");
        say_skipped(name, &why);
        return;
    }
    assert!(sent >= 250, "only {sent} new pictures went out in five fades of two seconds{}", run.context());
    assert!(late <= 6, "{late} pictures went out late{}", run.context());
    assert!(stood_still <= 1, "NDI pictures stood still {stood_still} times{}", run.context());
    assert!(longest <= 150.0, "a picture came {longest:.1} ms after the one before{}", run.context());
}

#[test]
fn quitting_while_an_ndi_screen_is_watched_stops_its_source_first_every_time() {
    let Some(run) = Run::new("quitting_while_an_ndi_screen_is_watched_stops_its_source_first_every_time") else {
        return;
    };
    let file = run.file("Hill Road.txt");
    std::fs::write(&file, song_text(&[("Verse 1", "Morning light on the hill")])).expect("the song's file");
    // A watched NDI screen sends its picture again thirty times a second, so a quit nearly
    // always comes while its thread is sending one. A copy that ended then, the thread still
    // inside NDI, crashed as it went (one quit in three, with a receiver connected). Five
    // quits, each with a picture going out to a stand-in somebody always watches: each stops
    // the source first, and ends cleanly.
    for quit in 1..=5 {
        let copy = run.start(&[("SOFTPRESENTER_NDI", "Quit watched"), ("SOFTPRESENTER_PRETEND_NDI", "1")]);
        if quit == 1 {
            copy.ok(&["song", "import", &file.to_string_lossy()]);
        }
        copy.ok(&["go", "Hill Road"]);
        copy.wait_for("the NDI screen to be watched", Duration::from_secs(20), |_| {
            let screens = copy.ok(&["screens"]).data.to_string();
            screens.contains("to 1 receiver")
        });
        // A different moment of a picture's time each round.
        std::thread::sleep(Duration::from_millis(200 + 7 * quit));
        let quitted = copy.quit();
        quitted.assert_clean(&run);
        // (This start's part of the log: a copy started again in the same folder writes on.)
        let this_start = quitted.log.rsplit("[start] SoftPresenter").next().unwrap_or_default();
        let stopped = this_start.lines().filter(|line| line.contains("was stopped before closing")).count();
        assert_eq!(stopped, 1, "quit {quit}: the source was not stopped before the copy closed{}", run.context());
    }
}

/// On a Mac, the Dock's Quit, a log-out, a restart and a shut-down end the app without its own
/// Quit: macOS says it will terminate, and ends the program. The app must do then what its Quit
/// does: save what was changed a moment before, and stop its NDI sources before the program
/// goes (one still sending as the program ended crashed it).
#[cfg(target_os = "macos")]
#[test]
fn terminated_by_the_system_the_app_saves_and_stops_its_ndi_sources_as_its_own_quit_does() {
    let name = "terminated_by_the_system_the_app_saves_and_stops_its_ndi_sources_as_its_own_quit_does";
    let Some(run) = Run::new(name) else { return };
    let file = run.file("Hill Road.txt");
    let words = song_text(&[("Verse 1", "Morning light on the hill"), ("Chorus", "Sing out, the day is new")]);
    std::fs::write(&file, words).expect("the song's file");
    let began = Instant::now();
    let copy = run.start(&[
        ("SOFTPRESENTER_NDI", "Terminated"),
        ("SOFTPRESENTER_PRETEND_NDI", "1"),
        ("SOFTPRESENTER_TERMINATE_AFTER", "6"),
    ]);
    copy.ok(&["song", "import", &file.to_string_lossy()]);
    copy.ok(&["go", "Hill Road"]);
    // Words changed a moment before the system ends the app.
    std::thread::sleep(Duration::from_millis(5300).saturating_sub(began.elapsed()));
    copy.ok(&["slide", "text", "Hill Road", "2", "Sing out, the day is here"]);
    let ended = copy.ended_within(Duration::from_secs(30));
    ended.assert_clean(&run);
    assert!(ended.log.contains("asking macOS to terminate the app"), "the switch was not taken{}", run.context());
    assert!(ended.log.contains("was stopped before closing"), "the NDI source was still running{}", run.context());
    let saved = song_file_with(&run.data, "Morning light on the hill").expect("the song's file in Songs");
    assert!(saved.contains("Sing out, the day is here"), "the changed words were not saved: {saved}");
}

#[test]
fn a_picture_from_the_media_bin_shows_behind_the_words_and_clears() {
    let Some(run) = Run::new("a_picture_from_the_media_bin_shows_behind_the_words_and_clears") else { return };
    let blue = run.file("Blue.png");
    let green = run.file("Green.png");
    write_png(&blue, [30, 90, 200]);
    write_png(&green, [40, 160, 60]);
    let copy = run.start(&[]);
    let added = copy.ok(&["media", "add", &blue.to_string_lossy(), &green.to_string_lossy()]);
    assert!(added.text.contains("Added 2 to the media bin"), "{}", added.text);
    assert_eq!(copy.ok(&["media"]).data, serde_json::json!(["Blue.png", "Green.png"]));
    // Words on screen, the picture behind them.
    copy.ok(&["go", "Sample song", "1"]);
    copy.ok(&["media", "show", "blue"]);
    let status = copy.status();
    assert_eq!(status["media"], "Blue.png", "{status}");
    copy.assert_live("Sample song", 1);
    copy.ok(&["media", "next"]);
    assert_eq!(copy.status()["media"], "Green.png");
    copy.ok(&["media", "set", "green", "scaling=fit"]);
    copy.refused(&["media", "set", "green", "loop=on"], "loop is for videos");
    // Cleared: the picture goes, the words stay.
    copy.ok(&["clear", "media"]);
    let status = copy.status();
    assert!(status["media"].is_null(), "{status}");
    copy.assert_live("Sample song", 1);
    copy.ok(&["media", "show", "1"]);
    assert_eq!(copy.status()["media"], "Blue.png");
    copy.ok(&["next"]);
    assert_eq!(copy.status()["media"], "Blue.png", "the picture stays behind the next slide");
    copy.ok(&["clear"]);
    assert_eq!(filled(&copy.status()), [false; 6]);

    copy.quit().assert_clean(&run);
    let bin = std::fs::read_to_string(run.data.join("media.json")).unwrap_or_default();
    assert!(bin.contains("Blue.png") && bin.contains("Green.png"), "the media bin was saved: {bin}");
}

#[test]
fn a_playlist_is_made_and_run_from_item_to_item() {
    let Some(run) = Run::new("a_playlist_is_made_and_run_from_item_to_item") else { return };
    let alpha = run.file("Alpha.txt");
    let delta = run.file("Delta.txt");
    std::fs::write(&alpha, song_text(&[("Verse 1", "Alpha one"), ("Chorus", "Alpha two"), ("Verse 2", "Alpha three")]))
        .expect("Alpha");
    std::fs::write(&delta, song_text(&[("Verse 1", "Delta one"), ("Verse 2", "Delta two")])).expect("Delta");
    let copy = run.start(&[]);
    copy.ok(&["song", "import", &alpha.to_string_lossy(), &delta.to_string_lossy()]);
    copy.ok(&["bible", "rows", "KJV"]);
    copy.ok(&["playlist", "new", "Evening Test"]);
    copy.ok(&["playlist", "header", "Worship", "in", "Evening Test"]);
    copy.ok(&["playlist", "add", "Alpha", "to", "Evening Test"]);
    copy.ok(&["playlist", "add", "Delta", "to", "Evening Test"]);
    copy.ok(&["playlist", "passage", "John 3:16-17", "in", "Evening Test"]);
    let items = copy.ok(&["playlist", "Evening Test"]).data;
    let kinds: Vec<&str> = items.as_array().into_iter().flatten().filter_map(|item| item["kind"].as_str()).collect();
    assert_eq!(kinds, ["header", "song", "song", "passage"], "{items}");
    copy.ok(&["playlist", "open", "Evening Test"]);
    copy.ok(&["continuous", "on"]);

    // From the first song's first slide to the end of the playlist, and back.
    copy.ok(&["playlist", "go", "2", "1"]);
    copy.assert_live("Alpha", 1);
    let status = copy.status();
    assert_eq!(status["live"]["playlist"], "Evening Test", "opened from the playlist: {status}");
    assert_eq!(status["live"]["item"], 2, "{status}");
    let mut seen = vec![copy.live().expect("on screen")];
    loop {
        let next = copy.ok(&["next"]);
        if next.data["at_end"] == true {
            assert!(next.text.contains("last slide of the playlist"), "{}", next.text);
            break;
        }
        seen.push(copy.live().unwrap_or_else(|| panic!("nothing on screen after {next:?}")));
        assert!(seen.len() < 20, "Next never came to the end: {seen:?}");
    }
    let names: Vec<String> = seen.iter().map(|(song, slide)| format!("{song} {slide}")).collect();
    // Alpha's chorus, written once, is sung again after its second verse.
    assert_eq!(names[..6], ["Alpha 1", "Alpha 2", "Alpha 3", "Alpha 2", "Delta 1", "Delta 2"], "{names:?}");
    assert_eq!(names.len(), 8, "the passage's two verses after the songs: {names:?}");
    let status = copy.status();
    assert_eq!(status["live"]["kind"], "passage", "{status}");
    assert_eq!(status["live"]["item"], 4, "{status}");
    assert!(status["live"]["words"].as_str().unwrap_or_default().starts_with("For God sent not his Son"), "{status}");
    // Back, into the last slide of the item before.
    copy.ok(&["previous"]);
    copy.ok(&["previous"]);
    copy.assert_live("Delta", 2);
    assert_eq!(copy.status()["live"]["item"], 3);
    // With Continuous off, Next stays at the end of the song, and says why.
    copy.ok(&["continuous", "off"]);
    let stayed = copy.ok(&["next"]);
    assert_eq!(stayed.data["at_end"], true, "{stayed:?}");
    assert!(stayed.text.contains("Continuous is off"), "{}", stayed.text);
    copy.assert_live("Delta", 2);
    // Items moved and taken out.
    copy.ok(&["playlist", "move", "4", "to", "2", "in", "Evening Test"]);
    copy.ok(&["playlist", "remove", "Delta", "from", "Evening Test"]);
    let items = copy.ok(&["playlist", "Evening Test"]).data;
    let titles: Vec<&str> = items.as_array().into_iter().flatten().filter_map(|item| item["title"].as_str()).collect();
    assert_eq!(titles, ["Worship", "John 3:16-17", "Alpha"], "{items}");

    copy.quit().assert_clean(&run);
    let saved = std::fs::read_to_string(run.data.join("services.json")).unwrap_or_default();
    assert!(saved.contains("Evening Test") && saved.contains("John 3:16-17"), "the playlist was saved: {saved}");
}

/// The slides a song shows, by their numbers, from its first as Next goes until it says it is at
/// the end.
fn sung_order(copy: &Copy, song: &str) -> Vec<u64> {
    copy.ok(&["go", song]);
    let mut seen = vec![copy.live().expect("on screen").1];
    loop {
        let next = copy.ok(&["next"]);
        if next.data["at_end"] == true {
            break seen;
        }
        seen.push(copy.live().expect("on screen").1);
        assert!(seen.len() < 12, "Next never came to the end: {seen:?}");
    }
}

/// A song whose chorus is written once, as SoftProjector keeps a song, is sung with it after
/// each verse: imported, chosen again after As written, followed when its words change as
/// text, and left alone once it is set by hand.
#[test]
fn a_chorus_written_once_is_sung_after_each_verse() {
    let Some(run) = Run::new("a_chorus_written_once_is_sung_after_each_verse") else { return };
    // Written for this test.
    let two = "Куплет 1\nСвет над рекой\n\nПрипев\nДомой, домой\n\nКуплет 2\nВечер настал\n";
    let file = run.file("River Road.txt");
    std::fs::write(&file, two).expect("the song");
    // `song text` takes song text, as Edit Words shows a song's words.
    let words = [("Куплет 1", "Свет над рекой"), ("Припев", "Домой, домой"), ("Куплет 2", "Вечер настал")];
    let back = run.file("two verses.txt");
    std::fs::write(&back, song_text(&words)).expect("the words");
    let three = run.file("three verses.txt");
    std::fs::write(&three, song_text(&[words[0], words[1], words[2], ("Куплет 3", "Ночь пришла")])).expect("the words");
    let copy = run.start(&[]);
    copy.ok(&["song", "import", &file.to_string_lossy()]);
    let listed = copy.ok(&["song", "arrangements", "River Road"]);
    assert!(
        listed
            .text
            .contains("Chorus after each verse (in use, follows the song's parts): Куплет 1, Припев, Куплет 2, Припев"),
        "{}",
        listed.text
    );
    // Next goes Verse 1, Chorus, Verse 2, Chorus, and then says it is at the end.
    let sung = |copy: &Copy| sung_order(copy, "River Road");
    assert_eq!(sung(&copy), [1, 2, 3, 2]);
    // As written, then Chorus After Each Verse again.
    copy.ok(&["song", "arrangement", "River Road", "use", "none"]);
    assert_eq!(sung(&copy), [1, 2, 3]);
    let said = copy.ok(&["song", "arrangement", "River Road", "chorus-after"]);
    assert!(said.text.contains("sings the chorus after each verse"), "{}", said.text);
    assert_eq!(sung(&copy), [1, 2, 3, 2]);
    // Its words changed as text: the arrangement follows the new verse.
    copy.ok(&["song", "text", "River Road", &three.to_string_lossy()]);
    assert_eq!(sung(&copy), [1, 2, 3, 2, 4, 2]);
    // Set by hand, it is the operator's: changing the words again leaves it as set.
    copy.ok(&["song", "arrangement", "River Road", "set", "Chorus after each verse", "Куплет 1", "Припев"]);
    copy.ok(&["song", "text", "River Road", &back.to_string_lossy()]);
    let listed = copy.ok(&["song", "arrangements", "River Road"]);
    assert_eq!(listed.data.as_array().map(Vec::len), Some(1), "no second one made: {}", listed.text);
    assert!(
        listed.text.lines().any(|line| line == "  Chorus after each verse (in use): Куплет 1, Припев"),
        "{}",
        listed.text
    );
    // A new song from pasted words: the same, at once.
    let pasted = "Verse 1\nOne for a test\n\nChorus\nSung for a test\n\nVerse 2\nTwo for a test\n";
    copy.ok(&["song", "new", "Pasted Song", "--words", pasted]);
    assert_eq!(sung_order(&copy, "Pasted Song"), [1, 2, 3, 2]);
    // A song with no chorus can't be sung so, and says why.
    let verses = run.file("Verses Only.txt");
    std::fs::write(&verses, song_text(&[("Verse 1", "One for a test"), ("Verse 2", "Two for a test")]))
        .expect("verses");
    copy.ok(&["song", "import", &verses.to_string_lossy()]);
    copy.refused(&["song", "arrangement", "Verses Only", "chorus-after"], "has no verse and chorus");
    copy.quit().assert_clean(&run);
}

/// The owner's rule for the keys (docs/cli.md, `running.rs`): once another item of the playlist
/// is opened, the next Next starts it at its first slide. After a verse from the Bible page was
/// shown and cleared, that is the song opened, not the next verse (the owner, 2026-11-25).
#[test]
fn after_a_reading_is_cleared_next_starts_the_song_opened_in_the_playlist() {
    let Some(run) = Run::new("after_a_reading_is_cleared_next_starts_the_song_opened_in_the_playlist") else { return };
    let song = run.file("Alpha.txt");
    std::fs::write(&song, song_text(&[("Verse 1", "Alpha one"), ("Verse 2", "Alpha two")])).expect("Alpha");
    let copy = run.start(&[]);
    copy.ok(&["song", "import", &song.to_string_lossy()]);
    copy.ok(&["playlist", "add", "Alpha"]);
    copy.ok(&["bible", "rows", "KJV"]);
    copy.ok(&["bible", "John 3:16", "--show"]);
    copy.ok(&["clear"]);
    copy.ok(&["view", "service"]);
    copy.ok(&["playlist", "go", "Alpha"]);
    copy.ok(&["next"]);
    copy.assert_live("Alpha", 1);
    copy.quit().assert_clean(&run);
}

/// The owner's rule (2026-11-25): Next always moves what the audience sees, and opening an item
/// only lines it up. A reading from the Bible page still on screen at its last verse: Next starts
/// the song opened in the playlist at its first slide.
#[test]
fn at_a_readings_last_verse_next_starts_the_song_opened_in_the_playlist() {
    let Some(run) = Run::new("at_a_readings_last_verse_next_starts_the_song_opened_in_the_playlist") else { return };
    let song = run.file("Alpha.txt");
    std::fs::write(&song, song_text(&[("Verse 1", "Alpha one"), ("Verse 2", "Alpha two")])).expect("Alpha");
    let copy = run.start(&[]);
    copy.ok(&["song", "import", &song.to_string_lossy()]);
    copy.ok(&["playlist", "add", "Alpha"]);
    copy.ok(&["bible", "rows", "KJV"]);
    // The Bible page makes the slides of the passage's whole chapter; Psalm 117 has two verses.
    copy.ok(&["bible", "Psalm 117:1", "--show"]);
    let (reading, first) = copy.live().expect("the reading on screen");
    assert_eq!(first, 1, "{reading}");
    copy.ok(&["next"]);
    copy.assert_live(&reading, 2);
    let status = copy.status();
    assert_eq!(status["live"]["slides"], 2, "{status}");
    assert!(status["next_slide"].is_null(), "at the reading's last verse: {status}");
    // What `status` says Next will do is what the line under the window's preview says, and
    // what Next then does: nothing more of the reading, then the song lined up.
    let said = copy.ok(&["status"]).text;
    assert!(!said.contains("Next:"), "{said}");
    copy.ok(&["view", "service"]);
    copy.ok(&["playlist", "go", "Alpha"]);
    copy.assert_live(&reading, 2);
    let said = copy.ok(&["status"]).text;
    assert!(said.contains("\nNext: Alpha, slide 1\n"), "{said}");
    copy.ok(&["next"]);
    copy.assert_live("Alpha", 1);
    let said = copy.ok(&["status"]).text;
    assert!(said.contains("\nNext: Verse 2\n"), "{said}");
    copy.quit().assert_clean(&run);
}

#[test]
fn audio_is_added_played_moved_through_and_stopped() {
    let Some(run) = Run::new("audio_is_added_played_moved_through_and_stopped") else { return };
    let tone = run.file("Tone.wav");
    write_wav(&tone, 6);
    let copy = run.start(&[]);
    let added = copy.ok(&["audio", "add", &tone.to_string_lossy()]);
    assert!(added.text.contains("Tone"), "{}", added.text);
    assert_eq!(copy.ok(&["audio"]).data, serde_json::json!(["Tone"]));
    if play_or_skip(&copy, "tone", "audio_is_added_played_moved_through_and_stopped: playing, seeking and stopping") {
        let status = copy.wait_for("the track playing", Duration::from_secs(10), |s| {
            s["audio"]["track"] == "Tone" && s["audio"]["seconds"].as_u64().is_some_and(|at| at >= 1)
        });
        assert_eq!(status["audio"]["length"], 6, "{status}");
        let sought = copy.ok(&["audio", "seek", "3"]);
        assert_eq!(sought.text, "Tone: at 0:03 of 0:06.");
        let at = copy.status()["audio"]["seconds"].as_u64().unwrap_or_default();
        assert!((3..=4).contains(&at), "at {at} s after a seek to 3 s");
        copy.refused(&["audio", "seek", "9"], "past its end");
        copy.ok(&["audio", "pause"]);
        let paused = copy.status()["audio"].clone();
        assert_eq!(paused["paused"], true, "{paused}");
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(copy.status()["audio"]["seconds"], paused["seconds"], "the place is held while paused");
        copy.ok(&["audio", "pause"]);
        assert_eq!(copy.status()["audio"]["paused"], false);
        copy.ok(&["audio", "stop"]);
        let stopped = copy.status()["audio"].clone();
        assert!(stopped["track"].is_null(), "{stopped}");
    }
    copy.ok(&["audio", "volume", "40"]);
    copy.refused(&["audio", "volume", "150"], "goes from 0 to 100");
    assert_eq!(copy.status()["audio"]["volume"], 40);

    copy.quit().assert_clean(&run);
    let saved = std::fs::read_to_string(run.data.join("audio.json")).unwrap_or_default();
    assert!(saved.contains("Tone.wav"), "the audio playlist was saved: {saved}");
}

#[test]
fn a_big_import_on_a_slow_disk_keeps_answering_and_quitting_keeps_every_song() {
    const SONGS: usize = 300;
    /// Each file the library's writer writes takes this much longer (SOFTPRESENTER_SLOW_WRITES),
    /// so writing them all takes 12 s at the least.
    const SLOW_MS: u64 = 40;
    let Some(run) = Run::new("a_big_import_on_a_slow_disk_keeps_answering_and_quitting_keeps_every_song") else {
        return;
    };
    let folder = run.file("Import Test");
    std::fs::create_dir_all(&folder).expect("the folder of songs");
    for n in 1..=SONGS {
        let words = song_text(&[
            ("Verse 1", &format!("The first verse of test song {n}")),
            ("Verse 2", &format!("The second verse of test song {n}")),
        ]);
        std::fs::write(folder.join(format!("Test Song {n:03}.txt")), words).expect("a song");
    }
    let copy = run.start(&[("SOFTPRESENTER_SLOW_WRITES", &SLOW_MS.to_string())]);
    // What is timed here is the import's files, so the copy first finishes drawing its first
    // pictures. On a computer that draws in software (GitHub's Windows machines, on WARP) those
    // take seconds after it first answers: the window's first drawings took 8.7, 1.4 and 2.9 s
    // there, each beside the screens' thread's first paints of the operator's preview, and sp
    // waited behind them; an import sent at once took one of those waits and the status after
    // it the next. On a Mac it answers at once from the start.
    let settled = Instant::now();
    let mut prompt = 0;
    while prompt < 3 {
        let took = copy.ok(&["status"]).took;
        prompt = if took < Duration::from_millis(500) { prompt + 1 } else { 0 };
        assert!(settled.elapsed() < START_WITHIN, "the copy never answered promptly{}", run.context());
    }
    say(&format!("the copy answered promptly after another {:.1} s", settled.elapsed().as_secs_f64()));
    let imported = copy.ok(&["song", "import", &folder.to_string_lossy()]);
    assert_eq!(imported.data["imported"].as_array().map(Vec::len), Some(SONGS), "{}", imported.text);
    // The import doesn't wait for its files: they are written one by one, behind it.
    let whole_save = Duration::from_millis(SLOW_MS * SONGS as u64);
    assert!(imported.took < whole_save / 2, "the import took {:?}: it waited for its files", imported.took);
    say(&format!("importing {SONGS} songs took {:.2} s to answer", imported.took.as_secs_f64()));
    // While they are written, the copy answers at once.
    let mut slowest = Duration::ZERO;
    for _ in 0..10 {
        let status = copy.ok(&["status"]);
        slowest = slowest.max(status.took);
        assert_eq!(status.data["selected_song"].as_str().map(|s| s.starts_with("Test Song")), Some(true));
        std::thread::sleep(Duration::from_millis(100));
    }
    let (written, _) = song_files(&run.data);
    say(&format!("slowest status while saving: {} ms; {written} of {SONGS} songs written", slowest.as_millis()));
    // Far less than waiting for the files still to be written (some 250, 40 ms each) and far more
    // than a status takes at rest (some 75 ms on GitHub's Windows machine).
    assert!(slowest < Duration::from_secs(2), "status took {slowest:?} while the songs were saved{}", run.context());
    // Quit part way through: it waits until every song is written.
    assert!(
        written < SONGS,
        "all {SONGS} songs were written before quitting, so quitting part way wasn't tried (SLOW_MS too small?)"
    );
    let quitted = copy.quit_within(Duration::from_secs(300));
    quitted.assert_clean(&run);
    assert!(
        quitted.log.contains("files to be written before quitting"),
        "the log says the quit waited for the files: {}",
        run.context()
    );
    let (written, half) = song_files(&run.data);
    assert_eq!(written, SONGS, "every song was written before the copy went{}", run.context());
    assert!(half.is_empty(), "half-written files left: {half:?}");
    let libraries = std::fs::read_to_string(run.data.join("libraries.json")).unwrap_or_default();
    assert!(libraries.contains("Import Test"), "the folder's library was saved: {libraries}");
    // Started again, every song is there, in its library.
    let copy = run.start(&[]);
    let songs = copy.ok(&["songs"]).data;
    assert_eq!(songs.as_array().map(Vec::len), Some(SONGS + 1), "the songs and the sample");
    let libraries = copy.ok(&["libraries"]).data;
    let library = libraries.as_array().into_iter().flatten().find(|l| l["name"] == "Import Test");
    assert_eq!(library, Some(&serde_json::json!({"name": "Import Test", "songs": SONGS, "missing": 0})), "{libraries}");
    copy.ok(&["go", "Test Song 300", "2"]);
    assert_eq!(copy.status()["live"]["words"], "The second verse of test song 300");
    copy.quit().assert_clean(&run);
}

#[test]
fn songs_found_on_the_computer_are_listed_and_only_their_new_songs_come_in() {
    let Some(run) = Run::new("songs_found_on_the_computer_are_listed_and_only_their_new_songs_come_in") else {
        return;
    };
    // The copy's home folder for the search (SOFTPRESENTER_FIND_SONGS_HOME): a songbook in
    // Documents, a copy of it in Downloads beside a damaged one, a folder of songs on the
    // Desktop, and a songbook in a hidden folder; and a folder of the test's own, added.
    let songbook: &[u8] = include_bytes!("../../sp-model/tests/songbooks/songbook.sps");
    let home = run.file("home");
    let put = |path: &str, bytes: &[u8]| {
        let path = home.join(path);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
        std::fs::write(path, bytes).expect("the file");
    };
    put("Documents/Old laptop/Zkouška.sps", songbook);
    put("Downloads/Zkouška.sps", songbook);
    put("Downloads/Broken.sps", &songbook[..5 * 1024]);
    put("Documents/.hidden/Secret.sps", songbook);
    for title in ["Morning Light", "River Song", "Evening Hymn"] {
        let words = song_text(&[("Verse 1", &format!("A line written for {title}"))]);
        put(&format!("Desktop/Hymns/{title}.txt"), words.as_bytes());
    }
    let added = run.file("Added");
    std::fs::create_dir_all(&added).expect("the folder added");
    std::fs::write(
        added.join("One.cho"),
        "{title: Lantern}\n{start_of_verse}\nA lantern in the window\n{end_of_verse}\n",
    )
    .expect("a ChordPro song");
    let copy = run.start(&[("SOFTPRESENTER_FIND_SONGS_HOME", &home.to_string_lossy())]);
    let songs_now = || copy.ok(&["songs"]).data.as_array().map_or(0, Vec::len);
    // One of the hymns is in the library already, imported by hand.
    copy.ok(&["song", "import", &home.join("Desktop/Hymns/River Song.txt").to_string_lossy()]);
    let before = songs_now();
    // The dry run: every source, numbered, with what is new; nothing comes in.
    let found = copy.ok(&["songs", "find", "--in", &added.to_string_lossy()]);
    let rows = found.data["sources"].as_array().cloned().unwrap_or_default();
    let row = |rows: &[Value], name: &str, place: &str| -> Value {
        rows.iter()
            .find(|row| row["name"] == name && row["where"] == place)
            .cloned()
            .unwrap_or_else(|| panic!("{name} in {place}: {}", found.text))
    };
    assert_eq!(rows.len(), 5, "{}", found.text);
    let first = row(&rows, "Zpěvník pro zkoušku", "Documents \u{203a} Old laptop");
    assert_eq!(
        (first["kind"].as_str(), first["songs"].as_u64(), first["new"].as_u64()),
        (Some("library"), Some(7), Some(7))
    );
    assert_eq!(first["softprojector"], true);
    let shown = "Library \u{b7} from SoftProjector \u{b7} Documents \u{203a} Old laptop \u{b7} 7 songs";
    assert!(found.text.contains(shown), "{}", found.text);
    let copied = row(&rows, "Zpěvník pro zkoušku", "Downloads");
    assert_eq!(copied["new"].as_u64(), Some(7));
    let broken = row(&rows, "Broken.sps", "Downloads");
    assert!(broken["why"].as_str().is_some_and(|why| why.contains("cut short or damaged")), "{broken}");
    let hymns = row(&rows, "Hymns", "Desktop");
    let counts = |row: &Value| (row["songs"].as_u64(), row["new"].as_u64(), row["here"].as_u64());
    assert_eq!(hymns["kind"].as_str(), Some("song files"));
    assert_eq!(counts(&hymns), (Some(3), Some(2), Some(1)), "River Song is here already");
    let lantern = row(&rows, "Added", "Added");
    assert_eq!(lantern["new"].as_u64(), Some(1));
    assert!(!found.text.contains("Secret"), "the hidden folder is passed over: {}", found.text);
    assert!(found.text.contains("be read: the file is cut short or damaged"), "{}", found.text);
    assert_eq!(songs_now(), before, "nothing came in");
    // A source that can't be read can't be brought in.
    let number = |row: &Value| row["number"].as_u64().expect("a number").to_string();
    copy.refused(&["songs", "find", "--import", &number(&broken)], "couldn't be read");
    // Bringing in the songbook and its copy together brings its seven songs once, into a
    // library of its name; the copy's are the songs just brought, never "already in your
    // library".
    let brought = copy.ok(&["songs", "find", "--import", &number(&first), &number(&copied)]);
    assert_eq!(
        brought.text, "7 songs came in: Zpěvník pro zkoušku (7). 7 found in more than one place came in once.",
        "{}",
        brought.text
    );
    assert_eq!(songs_now(), before + 7);
    let libraries = copy.ok(&["libraries"]).data;
    let library = libraries.as_array().into_iter().flatten().find(|l| l["name"] == "Zpěvník pro zkoušku");
    assert_eq!(library.and_then(|l| l["songs"].as_u64()), Some(7), "{libraries}");
    // Found again, the copy has nothing new, and can't be brought in.
    let again = copy.ok(&["songs", "find"]);
    let rows = again.data["sources"].as_array().cloned().unwrap_or_default();
    let copied = row(&rows, "Zpěvník pro zkoušku", "Downloads");
    assert_eq!((copied["new"].as_u64(), copied["here"].as_u64()), (Some(0), Some(7)), "{}", again.text);
    copy.refused(&["songs", "find", "--import", &number(&copied)], "already in your library");
    // The hymns, by name, bring their two new ones only, into a library of their own; the one
    // here already stays where it was.
    let brought = copy.ok(&["songs", "find", "--import", "Hymns"]);
    assert_eq!(brought.text, "2 songs came in: Hymns (2). 1 already in your library was left out.", "{}", brought.text);
    assert_eq!(songs_now(), before + 9);
    let libraries = copy.ok(&["libraries"]).data;
    let library = libraries.as_array().into_iter().flatten().find(|l| l["name"] == "Hymns");
    assert_eq!(library.and_then(|l| l["songs"].as_u64()), Some(2), "{libraries}");
    // With the window in Russian, `sp` and the log still say in English why the damaged one
    // couldn't be read (the window's own words are checked in find_songs' tests).
    copy.ok(&["settings", "set", "language=ru"]);
    let russian = copy.ok(&["songs", "find"]);
    let rows = russian.data["sources"].as_array().cloned().unwrap_or_default();
    let broken = row(&rows, "Broken.sps", "Downloads");
    let why = "the file is cut short or damaged (a row is longer than the file)";
    assert_eq!(broken["why"].as_str(), Some(why), "{}", russian.text);
    assert!(russian.text.contains(&format!("be read: {why}")), "{}", russian.text);
    // The log names neither the file nor its folder, which the search only came across.
    let log = run.log();
    let said: Vec<&str> = log.lines().filter(|line| line.contains("[find] a .sps file it found")).collect();
    assert!(said.len() >= 2, "a line each time it was found: {log}");
    for line in said {
        assert!(line.ends_with(&format!("couldn't be read (5 KB, SQLite): {why}")), "{line}");
    }
    assert!(!log.lines().any(|line| line.contains("[find]") && line.contains("Broken")), "{log}");
    copy.quit().assert_clean(&run);
}

// ---------------------------------------------------------------------------------------------
// Remote Control, over HTTPS on 127.0.0.1 (a quiet copy listens there only)

mod remote {
    use std::sync::Arc;
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_rustls::client::TlsStream;
    use tokio_rustls::rustls;
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

    pub const COOKIE: &str = "__Host-sp_device";

    /// A page's connection to the copy, trusting only the certificate the copy made.
    pub struct Page {
        pub port: u16,
        tls: Arc<rustls::ClientConfig>,
        pub runtime: tokio::runtime::Runtime,
    }

    /// An HTTP answer.
    pub struct Answer {
        pub status: u16,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Answer {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find(|(have, _)| have == name).map(|(_, value)| value.as_str())
        }
    }

    impl Page {
        pub fn new(port: u16, certificate: Vec<u8>) -> Page {
            let mut roots = rustls::RootCertStore::empty();
            roots.add(rustls::pki_types::CertificateDer::from(certificate)).expect("the copy's certificate");
            let mut config =
                rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                    .with_safe_default_protocol_versions()
                    .expect("TLS versions")
                    .with_root_certificates(roots)
                    .with_no_client_auth();
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
            Page { port, tls: Arc::new(config), runtime }
        }

        pub fn host(&self) -> String {
            format!("127.0.0.1:{}", self.port)
        }

        pub fn origin(&self) -> String {
            format!("https://{}", self.host())
        }

        async fn connect(&self) -> std::io::Result<TlsStream<TcpStream>> {
            let tcp = TcpStream::connect(("127.0.0.1", self.port)).await?;
            // The certificate names localhost (and the computer's name), never an address.
            let name = rustls::pki_types::ServerName::try_from("localhost").map_err(std::io::Error::other)?.to_owned();
            tokio_rustls::TlsConnector::from(self.tls.clone()).connect(name, tcp).await
        }

        /// One HTTP request, on a connection of its own, answered to its end.
        pub fn http(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Answer {
            self.runtime.block_on(async {
                let mut stream = self.connect().await.expect("a TLS connection to the copy");
                let mut request = format!(
                    "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n",
                    self.host(),
                    body.len()
                );
                for (name, value) in headers {
                    request.push_str(&format!("{name}: {value}\r\n"));
                }
                request.push_str("\r\n");
                request.push_str(body);
                stream.write_all(request.as_bytes()).await.expect("sent");
                stream.flush().await.expect("sent");
                let mut bytes = Vec::new();
                let _ = tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut bytes)).await;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
                let mut lines = head.lines();
                let status = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok());
                let headers = lines
                    .filter_map(|line| line.split_once(':'))
                    .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
                    .collect();
                Answer { status: status.unwrap_or(0), headers, body: body.to_owned() }
            })
        }

        /// The page's WebSocket, with its device's cookie.
        pub fn socket(&self, token: &str) -> WebSocketStream<TlsStream<TcpStream>> {
            self.runtime.block_on(async {
                let stream = self.connect().await.expect("a TLS connection to the copy");
                let mut request = format!("wss://{}/api/ws", self.host()).into_client_request().expect("request");
                request.headers_mut().insert("Origin", self.origin().parse().expect("header"));
                request.headers_mut().insert("Cookie", format!("{COOKIE}={token}").parse().expect("header"));
                tokio_tungstenite::client_async(request, stream).await.expect("the WebSocket").0
            })
        }

        pub fn send(&self, socket: &mut WebSocketStream<TlsStream<TcpStream>>, message: Value) {
            self.runtime.block_on(socket.send(Message::text(message.to_string()))).expect("sent");
        }

        /// The next JSON message the server sends (pictures passed over), or how it closed.
        pub fn receive(&self, socket: &mut WebSocketStream<TlsStream<TcpStream>>) -> Result<Value, Option<u16>> {
            self.runtime.block_on(async {
                loop {
                    let next = tokio::time::timeout(Duration::from_secs(20), socket.next()).await;
                    match next {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            return Ok(serde_json::from_str(&text).expect("JSON from the server"));
                        }
                        Ok(Some(Ok(Message::Close(frame)))) => return Err(frame.map(|f| u16::from(f.code))),
                        Ok(Some(Ok(_))) => {}
                        Ok(Some(Err(_)) | None) => return Err(None),
                        Err(_) => panic!("the server sent nothing for 20 s"),
                    }
                }
            })
        }

        /// The next message of this type (beats and changes in between passed over).
        pub fn receive_type(&self, socket: &mut WebSocketStream<TlsStream<TcpStream>>, kind: &str) -> Value {
            for _ in 0..200 {
                let message = self.receive(socket).unwrap_or_else(|code| panic!("closed ({code:?}) before a {kind}"));
                if message["type"] == kind {
                    return message;
                }
            }
            panic!("no {kind} came");
        }
    }
}

#[test]
fn remote_control_signs_in_takes_a_snapshot_runs_a_command_and_signs_out() {
    const PASSWORD: &str = "a password for this test";
    let Some(run) = Run::new("remote_control_signs_in_takes_a_snapshot_runs_a_command_and_signs_out") else { return };
    let copy = run.start(&[]);
    copy.refused(&["remote", "on"], "password");
    let set = run.sp_with_input(&["remote", "password"], Some(PASSWORD));
    assert!(set.ok, "{}", set.text);
    // A free port of this computer's, never the usual 4316 (the owner's copy may use it).
    let port = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port").local_addr().expect("port").port();
    copy.ok(&["remote", "port", &port.to_string()]);
    let on = copy.ok(&["remote", "on"]);
    assert_eq!(on.data["on"], true, "{on:?}");
    assert_eq!(on.data["address_by_ip"], format!("https://127.0.0.1:{port}"), "a quiet copy listens on 127.0.0.1");
    let certificate = std::fs::read(run.data.join("remote").join("cert.der")).expect("the certificate it made");
    let page = remote::Page::new(port, certificate);
    let origin = page.origin();

    // Not signed in: the page sends the browser to sign in.
    let answer = page.http("GET", "/", &[], "");
    assert_eq!((answer.status, answer.header("location")), (303, Some("/login")), "{}", answer.body);
    // A wrong password, then the right one.
    let sign_in = |password: &str| {
        let body = serde_json::json!({ "password": password }).to_string();
        page.http(
            "POST",
            "/api/login",
            &[
                ("Origin", &origin),
                ("Content-Type", "application/json"),
                ("User-Agent", "Mozilla/5.0 (iPad) Safari/604.1"),
            ],
            &body,
        )
    };
    let wrong = sign_in("not the password at all");
    assert_eq!(wrong.status, 401, "{}", wrong.body);
    let right = sign_in(PASSWORD);
    assert_eq!(right.status, 200, "{}", right.body);
    let cookie = right.header("set-cookie").expect("a cookie").to_owned();
    let token = cookie
        .strip_prefix(&format!("{}=", remote::COOKIE))
        .and_then(|rest| rest.split(';').next())
        .expect("the device's token")
        .to_owned();
    let with_cookie = format!("{}={token}", remote::COOKIE);
    assert_eq!(page.http("GET", "/", &[("Cookie", &with_cookie)], "").status, 200, "the page, signed in");
    let devices = copy.ok(&["remote"]).data["devices"].clone();
    assert_eq!(devices.as_array().map(Vec::len), Some(1), "the device is remembered: {devices}");

    // The page's WebSocket: hello, the whole state, then a command and the change it makes.
    let mut socket = page.socket(&token);
    page.send(&mut socket, serde_json::json!({"type": "hello"}));
    let snapshot = page.receive_type(&mut socket, "snapshot");
    for part in ["app", "live", "playlist", "playlists", "library", "bin", "audio", "timers", "messages", "props"] {
        assert!(snapshot["parts"].get(part).is_some(), "the snapshot has {part}: {snapshot}");
    }
    assert_eq!(snapshot["you"]["role"], "control", "{snapshot}");
    page.send(&mut socket, serde_json::json!({"type": "command", "id": 7, "args": ["go", "Sample song", "2"]}));
    // Its reply, and the change it made to what is on screen, in whichever order they come.
    let (mut reply, mut change) = (None, None);
    for _ in 0..200 {
        let message = page.receive(&mut socket).unwrap_or_else(|code| panic!("closed ({code:?}) before the reply"));
        match message["type"].as_str() {
            Some("reply") => reply = Some(message),
            Some("change") if message["parts"].get("live").is_some() => change = Some(message),
            _ => {}
        }
        if reply.is_some() && change.is_some() {
            break;
        }
    }
    let reply = reply.expect("the command's reply");
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["reply"]["ok"], true, "{reply}");
    assert!(change.is_some(), "a change carrying what is on screen came");
    copy.assert_live("Sample song", 2);
    // What a page may not do is refused before it reaches the app.
    page.send(&mut socket, serde_json::json!({"type": "command", "id": 8, "args": ["remote", "off"]}));
    let refused = page.receive_type(&mut socket, "reply");
    assert_eq!(refused["reply"]["ok"], false, "{refused}");
    assert_eq!(copy.ok(&["remote"]).data["on"], true);

    // Signing out forgets the device, closes its pages, and clears the cookie.
    let out = page.http("POST", "/api/logout", &[("Origin", &origin), ("Cookie", &with_cookie)], "");
    assert_eq!(out.status, 200, "{}", out.body);
    assert!(out.header("set-cookie").is_some_and(|c| c.contains("Max-Age=0")), "{:?}", out.headers);
    let closed = loop {
        match page.receive(&mut socket) {
            Ok(_) => continue,
            Err(code) => break code,
        }
    };
    assert_eq!(closed, Some(4401), "the page's socket closes with 4401 (signed out)");
    let answer = page.http("GET", "/", &[("Cookie", &with_cookie)], "");
    assert_eq!(answer.status, 303, "the old cookie no longer signs in: {}", answer.body);
    let devices = copy.ok(&["remote"]).data["devices"].clone();
    assert_eq!(devices, serde_json::json!([]), "no device is remembered after signing out");

    copy.ok(&["remote", "off"]);
    let off = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
        assert!(off.elapsed() < Duration::from_secs(10), "something still listens on {port} after remote off");
        std::thread::sleep(Duration::from_millis(100));
    }
    copy.quit().assert_clean(&run);
}

#[test]
fn remote_control_sends_a_change_at_once_and_one_that_happens_by_itself_with_it() {
    const PASSWORD: &str = "a password for this test";
    let name = "remote_control_sends_a_change_at_once_and_one_that_happens_by_itself_with_it";
    let Some(run) = Run::new(name) else { return };
    let copy = run.start(&[]);
    assert!(run.sp_with_input(&["remote", "password"], Some(PASSWORD)).ok);
    let port = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port").local_addr().expect("port").port();
    copy.ok(&["remote", "port", &port.to_string()]);
    copy.ok(&["remote", "on"]);
    let certificate = std::fs::read(run.data.join("remote").join("cert.der")).expect("the certificate it made");
    let page = remote::Page::new(port, certificate);
    let body = serde_json::json!({ "password": PASSWORD }).to_string();
    let signed = page.http(
        "POST",
        "/api/login",
        &[
            ("Origin", &page.origin()),
            ("Content-Type", "application/json"),
            ("User-Agent", "Mozilla/5.0 (iPad) Safari/604.1"),
        ],
        &body,
    );
    assert_eq!(signed.status, 200, "{}", signed.body);
    let cookie = signed.header("set-cookie").expect("a cookie").to_owned();
    let token = cookie
        .strip_prefix(&format!("{}=", remote::COOKIE))
        .and_then(|rest| rest.split(';').next())
        .expect("the device's token")
        .to_owned();
    let mut socket = page.socket(&token);
    page.send(&mut socket, serde_json::json!({"type": "hello"}));
    page.receive_type(&mut socket, "snapshot");

    // The next change that says whether a message is on the screens (`want`), and how long after
    // `since` it came.
    let message_layer = |socket: &mut _, want: bool, since: Instant| {
        for _ in 0..400 {
            let said = page.receive(socket).unwrap_or_else(|code| panic!("closed ({code:?})"));
            let layer = &said["parts"]["live"]["layers"]["messages"];
            if said["type"] == "change" && layer.as_bool() == Some(want) {
                return since.elapsed();
            }
        }
        panic!("no change said a message was {}", if want { "on" } else { "off" });
    };
    // A command, which comes to the app as an event: its change is sent at once, not on the
    // backstop's time (a second).
    copy.ok(&["message", "new", "Parking", "Car with lights on"]);
    copy.ok(&["message", "set", "Parking", "leaves=1"]);
    let shown = Instant::now();
    copy.ok(&["message", "show", "Parking"]);
    let on = message_layer(&mut socket, true, shown);
    assert!(on < Duration::from_millis(400), "the message reached the page after {on:?}");
    // The message leaves by itself after a second, on a pass of the loop that no event asked
    // for: the page is told with the screens, not a backstop's second later.
    let gone = message_layer(&mut socket, false, shown);
    assert!(gone >= Duration::from_millis(900), "the page was told the message left after {gone:?}");
    assert!(gone < Duration::from_millis(1_800), "the page was told the message left after {gone:?}");
    copy.quit().assert_clean(&run);
}

// ---------------------------------------------------------------------------------------------
// Check for Updates, against a stand-in for GitHub on this computer

/// A stand-in for GitHub's list of releases: answers each request with the next of its answers,
/// and keeps what it was asked.
struct StandIn {
    port: u16,
    asked: std::sync::Arc<Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StandIn {
    fn serve(answers: Vec<(u16, String)>) -> StandIn {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port for the stand-in");
        listener.set_nonblocking(true).expect("non-blocking");
        let port = listener.local_addr().expect("its port").port();
        let asked = std::sync::Arc::new(Mutex::new(Vec::new()));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (kept, stopping) = (asked.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let mut answers = answers.into_iter();
            while !stopping.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                };
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                    head.push(byte[0]);
                }
                if let Ok(mut asked) = kept.lock() {
                    asked.push(String::from_utf8_lossy(&head).into_owned());
                }
                let (status, body) = answers.next().unwrap_or((500, "no more answers".to_owned()));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Answer\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        StandIn { port, asked, stop, thread: Some(thread) }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().map(|asked| asked.clone()).unwrap_or_default()
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A release as GitHub lists it.
fn release(tag: &str, page: &str) -> Value {
    serde_json::json!({ "tag_name": tag, "html_url": page, "draft": false, "prerelease": tag.contains('-') })
}

#[test]
fn check_for_updates_asks_for_the_releases_and_says_what_it_found() {
    let Some(run) = Run::new("check_for_updates_asks_for_the_releases_and_says_what_it_found") else { return };
    const PAGES: &str = "https://github.com/esraell/SoftPresenter/releases/tag/";
    let answers = vec![
        // Only older versions, and the Bibles' release, which isn't a version.
        (200, serde_json::json!([release("v0.1.0", &format!("{PAGES}v0.1.0")), release("bibles-4", "")]).to_string()),
        // A newer one.
        (200, serde_json::json!([release("v99.0.0", &format!("{PAGES}v99.0.0"))]).to_string()),
        // Not a list of releases.
        (200, "<html>not this</html>".to_owned()),
        // A newer one whose page is somewhere else: its page is made from its tag instead.
        (200, serde_json::json!([release("v99.1.0", "https://example.com/elsewhere")]).to_string()),
        // GitHub taking fewer checks.
        (429, "{}".to_owned()),
    ];
    let stand_in = StandIn::serve(answers);
    let address = format!("http://127.0.0.1:{}/repos/releases", stand_in.port);
    let copy = run.start(&[("SOFTPRESENTER_RELEASES_URL", &address)]);
    let version = copy.ok(&["update"]).data["version"].as_str().unwrap_or_default().to_owned();
    let newest = copy.sp(&["update", "check"]);
    if stand_in.asked().is_empty() {
        // A development build (SOFTPRESENTER_DEV_VERSION) may ask its own updater instead.
        let why = format!("this build ({version}) didn't ask for the releases: {}", newest.text);
        say_skipped("check_for_updates_asks_for_the_releases_and_says_what_it_found", &why);
        return;
    }
    assert!(newest.ok, "{}", newest.text);
    assert_eq!(newest.text, format!("You have the newest version ({version})"));
    let newer = copy.ok(&["update", "check"]);
    assert_eq!(newer.text, format!("SoftPresenter 99.0.0 is available: {PAGES}v99.0.0"));
    let status = copy.ok(&["update"]).data;
    assert_eq!(
        (status["newer"].as_str(), status["page"].as_str()),
        (Some("99.0.0"), Some(&*format!("{PAGES}v99.0.0")))
    );
    let notices = copy.ok(&["notices"]).text;
    assert!(notices.contains("SoftPresenter 99.0.0 is available"), "the answer is in the notices too: {notices}");
    copy.refused(&["update", "check"], "list of releases");
    let elsewhere = copy.ok(&["update", "check"]);
    assert_eq!(elsewhere.text, format!("SoftPresenter 99.1.0 is available: {PAGES}v99.1.0"));
    copy.refused(&["update", "check"], "fewer checks");
    let asked = stand_in.asked();
    assert_eq!(asked.len(), 5, "one request for each check: {asked:?}");
    for head in &asked {
        let lower = head.to_ascii_lowercase();
        assert!(head.starts_with("GET /repos/releases HTTP/1.1"), "{head}");
        assert!(
            lower.contains(&format!("user-agent: softpresenter/{version}\r\n")),
            "only the app and its version: {head}"
        );
        assert!(!lower.contains("cookie") && !lower.contains("authorization"), "nothing that tells who asks: {head}");
    }
    copy.quit().assert_clean(&run);
}

#[test]
fn capture_home_saves_a_picture_of_the_window_and_quits() {
    let Some(run) = Run::new("capture_home_saves_a_picture_of_the_window_and_quits") else { return };
    let picture = run.file("home.png");
    let mut command = run.program();
    command.arg("--capture-home").arg(&picture);
    let (status, stdout, stderr, took) = run_to_end(command, None, START_WITHIN);
    let Some(status) = status else { panic!("the capture run didn't finish within {START_WITHIN:?}{}", run.context()) };
    say(&format!("the capture run took {:.1} s", took.as_secs_f64()));
    assert!(status.success(), "the capture run exited with {status}: {stderr}{}", run.context());
    assert!(stdout.contains("saved"), "{stdout}");
    let image =
        image::open(&picture).unwrap_or_else(|e| panic!("the picture it saved: {e}{}", run.context())).to_rgba8();
    assert!(image.width() >= 800 && image.height() >= 500, "a picture of the whole window: {:?}", image.dimensions());
    let colours: std::collections::HashSet<[u8; 4]> = image.pixels().step_by(7).map(|p| p.0).collect();
    assert!(colours.len() > 50, "the window, drawn ({} colours), not a blank picture", colours.len());
    // What the home window must hold, whatever its size: the accent orange of the live slide's
    // outline and of the Clear All button (about 1.7% of the picture), the slides' black
    // pictures (about 20%) and the white of their words (about 0.4%). A window showing an error
    // dialog, or a picture of the wrong thing, has many colours and none of these.
    let total = (image.width() * image.height()) as usize;
    let share = |count: usize| count as f64 / total as f64;
    let (mut orange, mut black, mut white) = (0, 0, 0);
    for p in image.pixels() {
        let [r, g, b, _] = p.0;
        orange += usize::from(r > 200 && (40..120).contains(&g) && b < 70);
        black += usize::from(r < 6 && g < 6 && b < 6);
        white += usize::from(r > 235 && g > 235 && b > 235);
    }
    assert!(
        share(orange) > 0.005,
        "the accent orange (Clear All, the live slide) is in the picture: {:.3}",
        share(orange)
    );
    assert!(share(black) > 0.05, "the slides' black pictures are in it: {:.3}", share(black));
    assert!(share(white) > 0.001, "and the white of their words: {:.4}", share(white));
    let log = run.log();
    for bad in ["[crash]", "panicked", "Graphics error"] {
        assert!(!log.contains(bad) && !stderr.contains(bad), "the capture run said {bad:?}{}", run.context());
    }
    assert!(!run.data.join("running").exists(), "the capture run left its running note behind{}", run.context());
}
