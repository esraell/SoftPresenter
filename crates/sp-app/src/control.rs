//! Controlling a running SoftPresenter from outside its window: `softpresenter ctl …`.
//!
//! While it runs, the app listens on a socket file that only the user it runs as can reach (it
//! sits in a folder only that user may use: `private_folder` says how that is kept on each
//! system). The command line sends one request, a line of
//! JSON with the words typed, and gets one reply. Each command is carried out on the app's
//! main thread by the same code the window's buttons use, so the two can never disagree.
//!
//! Who can use it: programs running as the same user, who could change the app's files anyway.
//! Nothing listens on the network, so no web page or other computer can reach it.

use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sp_engine::{Command, Layer, SlideRef};
// Windows has the same kind of socket (a file, reached only from this computer); the standard
// library doesn't offer it there yet.
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};
use winit::event_loop::ActiveEventLoop;

use crate::private_folder;

/// Changes when requests or replies change shape, so an older command line is told so. 2: a
/// request may carry `mcp` (it comes from an AI assistant); an app from before refuses it, rather
/// than carrying it out unchecked, as it would if it only ignored the field. This app still takes
/// version 1 requests without the mark, as they come from an older `sp` at the computer.
pub const PROTOCOL: u32 = 2;

/// Why a slide named by its number isn't there: "River Song has 6 slides; there is no slide 9.",
/// in the window's language for a remote page (`sp`'s replies are given back in English).
pub(crate) fn no_such_slide(title: &str, count: usize, number: &str) -> String {
    crate::i18n::tn(
        "remote.refused.no_slide",
        count as u64,
        "{title} has {n} slide; there is no slide {number}.",
        "{title} has {n} slides; there is no slide {number}.",
        &[("title", title), ("number", number)],
    )
}

/// The status's first line: what is on screen (`App::live_words`), and while the slide's words
/// are hidden (Hide the Words), that they are: "On screen: River Song, slide 2 of 3 (Verse 2)
/// (words hidden)."
pub(crate) fn on_screen_line(live_words: String, words_hidden: bool) -> String {
    if !words_hidden {
        return live_words;
    }
    let said = live_words.strip_suffix('.').unwrap_or(&live_words);
    format!("{said} {}.", crate::i18n::t("notices.status.words_hidden", "(words hidden)"))
}

/// The most a request may be; a longer one is refused.
const MAX_REQUEST: u64 = 64 * 1024;

/// One request: the words typed after `ctl`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub args: Vec<String>,
    /// Set by `sp mcp` on each request it sends for an AI assistant: the app then checks the
    /// request against Settings › AI assistants and `mcp::policy`, not as the
    /// computer's own user's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpCall>,
}

/// What `sp mcp` says about a request it sends: which of its tools sent it, whether the person
/// said yes to its question, and whether the app is only to say if it would allow it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCall {
    /// The tool (or `resource …`, `tasks/get`), for the log.
    pub tool: String,
    /// True when the person said yes to the question the server asked first; a change to the
    /// screens, or a removal, is refused without it.
    #[serde(default)]
    pub asked: bool,
    /// True to be told whether it would be allowed, without carrying it out.
    #[serde(default)]
    pub check: bool,
}

/// One reply: whether it worked, what to tell the person, and the same as data for programs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub text: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
}

impl Reply {
    pub(crate) fn done(text: impl Into<String>) -> Reply {
        Reply { ok: true, text: text.into(), data: serde_json::Value::Null }
    }

    pub(crate) fn with(text: impl Into<String>, data: serde_json::Value) -> Reply {
        Reply { ok: true, text: text.into(), data }
    }

    pub(crate) fn failed(text: impl Into<String>) -> Reply {
        Reply { ok: false, text: text.into(), data: serde_json::Value::Null }
    }
}

/// A request that has come in, with where its reply goes.
pub struct Incoming {
    pub request: Request,
    pub reply: ReplyTo,
}

/// Where a request's reply goes: the main thread hands it over here, and the thread that read
/// the request writes it back. A reply handed over counts as unwritten until it has been written
/// and its connection closed, or dropped unwritten, and the app doesn't end before then
/// ([`finish_replies`]): `menu Quit`'s reply is handed over as the app begins to leave, and the
/// program could end before it was written, so that `sp` read nothing ("EOF while parsing").
/// A request still waiting for its reply (`status --wait`, held) counts for nothing.
pub struct ReplyTo(Sender<Handed>);

impl ReplyTo {
    /// Hands the reply over to be written. Gives it back when nobody waits for it any more (the
    /// connection gave up).
    pub fn send(&self, reply: Reply) -> Result<(), Reply> {
        self.0.send(Handed { reply, _unwritten: Unwritten::counted() }).map_err(|handed| handed.0.reply)
    }
}

/// A reply handed over, with its place in the count of replies not yet written.
struct Handed {
    reply: Reply,
    _unwritten: Unwritten,
}

/// Replies handed over and not yet written and closed, or dropped.
static UNWRITTEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// One of [`UNWRITTEN`], for as long as it is kept.
struct Unwritten;

impl Unwritten {
    fn counted() -> Unwritten {
        UNWRITTEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Unwritten
    }
}

impl Drop for Unwritten {
    fn drop(&mut self) {
        UNWRITTEN.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Waits, as the app ends, until every reply handed over has been written and its connection
/// closed, for at most `within`. Returns how many were still unwritten then.
pub fn finish_replies(within: Duration) -> usize {
    let began = std::time::Instant::now();
    loop {
        let left = UNWRITTEN.load(std::sync::atomic::Ordering::SeqCst);
        if left == 0 || began.elapsed() >= within {
            return left;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Where the socket is: `SOFTPRESENTER_CONTROL_SOCKET` if set (for tests, whose folders have
/// paths too long for a socket); else, on a Mac, in ~/Library/Caches/SoftPresenter/control,
/// outside the library in Documents (which macOS guards, and may sync), so `sp` reaches the app
/// from any terminal or over ssh; on Windows, in SoftPresenter\control in the user's local app
/// data folder (the one that stays on this computer, not the one a network account carries from
/// computer to computer); else the `control` folder in the app's data folder (as for a test copy
/// with a data folder of its own).
pub fn socket_path() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    socket_path_on(std::env::consts::OS, var, crate::bible_store::data_dir)
}

/// Where the socket is on a system (`macos`, `windows`, or another), given the environment's
/// variables and the app's data folder.
fn socket_path_on(
    system: &str,
    var: impl Fn(&str) -> Option<PathBuf>,
    data_dir: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(path) = var("SOFTPRESENTER_CONTROL_SOCKET") {
        return Some(path);
    }
    if system == "macos"
        && var("SOFTPRESENTER_DATA").is_none()
        && let Some(home) = var("HOME")
    {
        return Some(home.join("Library/Caches/SoftPresenter/control/ctl.sock"));
    }
    // Never the app data folder a network account carries about: without the local one there
    // is no socket.
    if system == "windows" && var("SOFTPRESENTER_DATA").is_none() {
        return Some(var("LOCALAPPDATA")?.join("SoftPresenter").join("control").join("ctl.sock"));
    }
    data_dir().map(|dir| dir.join("control").join("ctl.sock"))
}

/// The socket file while the app listens; dropping it removes the file.
pub struct Listening {
    path: PathBuf,
}

impl Drop for Listening {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Starts listening, on a thread of its own. Each request is handed to `deliver`, which wakes
/// the app's main thread with it; the reply is written back when the main thread sends it.
pub fn listen(deliver: impl Fn(Incoming) + Send + Sync + 'static) -> Result<Listening, String> {
    let path = socket_path().ok_or("Could not find where to keep the control socket.")?;
    listen_at(path, deliver)
}

/// Starts listening at a socket file of that path, in a folder kept to this user. If the folder
/// can't be kept private, nothing listens.
pub(crate) fn listen_at(
    path: PathBuf,
    deliver: impl Fn(Incoming) + Send + Sync + 'static,
) -> Result<Listening, String> {
    // A socket path must be short (about 104 bytes on macOS).
    if path.as_os_str().len() > 100 {
        return Err(format!("The control socket's path is too long: {}", path.display()));
    }
    let folder = path.parent().ok_or("The control socket has no folder.")?;
    let private = private_folder::keep_private(folder)?;
    // A socket left by an app that stopped is removed; one another running copy is listening on
    // is left alone.
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if !private_folder::is_socket(&meta) {
            return Err(format!("{} is in the way of the control socket.", path.display()));
        }
        // On Windows a socket keeps the access list it was made with: one that lets others in
        // is in the way too, and is neither asked nor replaced. (Nothing to look at on Unix.)
        private.check_existing(&path)?;
        if UnixStream::connect(&path).is_ok() {
            return Err("Another SoftPresenter is already listening for commands.".to_owned());
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path).map_err(|e| format!("Could not listen at {}: {e}", path.display()))?;
    if let Err(problem) = private.check_socket(&path) {
        drop(listener);
        let _ = std::fs::remove_file(&path);
        return Err(problem);
    }
    let deliver: Deliver = std::sync::Arc::new(deliver);
    std::thread::Builder::new()
        .name("control".to_owned())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                answer(stream, &deliver);
            }
        })
        .map_err(|e| format!("Could not start listening for commands: {e}"))?;
    Ok(Listening { path })
}

/// What hands a request to the app's main thread; shared with the threads that wait for a
/// change.
type Deliver = std::sync::Arc<dyn Fn(Incoming) + Send + Sync>;

/// The longest `status --wait` waits for a change, in seconds.
pub const MOST_WAIT: f64 = 30.0;

/// The most connections waiting for a change at once; one more is refused at once, without
/// waking the app, so a page that keeps asking can't pile up threads.
const MOST_WAITING: usize = 8;

/// How many connections are waiting for a change now.
static WAITING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// One connection fewer waiting, when it is dropped: when its thread ends, or when the thread
/// couldn't be started.
struct Leaving;

impl Drop for Leaving {
    fn drop(&mut self) {
        WAITING.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// `status --wait SECONDS [--since TOKEN]`: how long to wait for a change, and the token the
/// change is from (`None`: from now). `None` for any other command; an error for a wait asked
/// for wrongly.
pub fn wait_for(args: &[String]) -> Option<Result<(Duration, Option<String>), String>> {
    if !args.first().is_some_and(|word| word.eq_ignore_ascii_case("status")) {
        return None;
    }
    let at = args.iter().position(|word| word == "--wait")?;
    let usage = format!("status --wait SECONDS [--since TOKEN]   (SECONDS from 0 to {MOST_WAIT})");
    let seconds = match args.get(at + 1).and_then(|word| word.parse::<f64>().ok()) {
        Some(seconds) if (0.0..=MOST_WAIT).contains(&seconds) => seconds,
        Some(seconds) if seconds.is_finite() => {
            return Some(Err(format!("A wait is 0 to {MOST_WAIT} seconds, not {seconds}.")));
        }
        _ => return Some(Err(usage)),
    };
    let since = match args.iter().position(|word| word == "--since") {
        Some(at) => match args.get(at + 1).filter(|token| !token.starts_with("--")) {
            Some(token) => Some(token.clone()),
            None => return Some(Err(usage)),
        },
        None => None,
    };
    Some(Ok((Duration::from_secs_f64(seconds), since)))
}

/// Reads one request from a connection, hands it to the app and writes back its reply. A wait
/// for a change is answered on a thread of its own, so the commands after it are answered
/// meanwhile.
fn answer(stream: UnixStream, deliver: &Deliver) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    let read = BufReader::new((&stream).take(MAX_REQUEST)).read_line(&mut line);
    let too_long = line.len() as u64 >= MAX_REQUEST && !line.ends_with('\n');
    let (reply, unwritten) = match read.map_err(|e| e.to_string()).and_then(|_| {
        if too_long {
            return Err(format!("A command may be at most {} KB.", MAX_REQUEST / 1024));
        }
        serde_json::from_str::<Request>(line.trim())
            .map_err(|e| format!("Not a request SoftPresenter understands: {e}"))
    }) {
        Err(problem) => (Reply::failed(problem), None),
        // An older command line (version 1) can't carry an assistant's mark, so its requests are
        // the computer's own and still welcome: `sp menu Quit` from the build before an update.
        // Only a version 1 request that carries the mark is refused, with any other version.
        Ok(request) if request.v != PROTOCOL && !(request.v == 1 && request.mcp.is_none()) => (
            Reply::failed(format!(
                "This command line speaks version {} of the protocol, the running app version {PROTOCOL}. Use the same build for both.",
                request.v
            )),
            None,
        ),
        Ok(request) => match wait_for(&request.args) {
            Some(Ok((wait, _))) => {
                let leaving = Leaving;
                if WAITING.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= MOST_WAITING {
                    drop(leaving);
                    (
                        Reply::failed(format!(
                            "{MOST_WAITING} connections are already waiting for a change; ask again in a moment, or without --wait."
                        )),
                        None,
                    )
                } else {
                    let deliver = deliver.clone();
                    // The app answers at the change or when the wait is up; past that, as for
                    // any command, it is given 15 seconds more. A thread that can't be started
                    // drops the connection unanswered, and `leaving` with it.
                    let _ = std::thread::Builder::new().name("control-wait".to_owned()).spawn(move || {
                        let _leaving = leaving;
                        let (reply, unwritten) = ask(&deliver, request, wait + Duration::from_secs(15));
                        write_reply(&stream, reply);
                        drop(stream);
                        drop(unwritten);
                    });
                    return;
                }
            }
            _ => ask(deliver, request, Duration::from_secs(15)),
        },
    };
    write_reply(&stream, reply);
    // Closed before the reply stops counting as unwritten: all of it is the other end's then.
    drop(stream);
    drop(unwritten);
}

/// A `status --wait` the app holds: answered when something changes from its token, or when
/// its time is up.
pub struct Waiter {
    reply: ReplyTo,
    since: String,
    until: std::time::Instant,
    seconds: f64,
}

/// The parts of a change token, in its order: what `status` says, the playlists, the library.
const TOKEN_PARTS: [&str; 3] = ["status", "playlists", "library"];

/// Which parts of the state changed between two tokens. A token that isn't one (from an older
/// build, say) has changed in every part.
fn changed_parts(since: &str, now: &str) -> Vec<&'static str> {
    let since: Vec<&str> = since.split('-').collect();
    let now: Vec<&str> = now.split('-').collect();
    if since.len() != TOKEN_PARTS.len() || now.len() != TOKEN_PARTS.len() {
        return TOKEN_PARTS.to_vec();
    }
    TOKEN_PARTS.iter().zip(since.iter().zip(&now)).filter(|(_, (was, is))| was != is).map(|(part, _)| *part).collect()
}

/// A status reply with the parts that changed since a token, as `status --wait` answers.
fn with_changes(mut reply: Reply, since: Option<&str>) -> Reply {
    let token = reply.data["token"].as_str().unwrap_or_default().to_owned();
    reply.data["changed"] = serde_json::json!(since.map_or_else(Vec::new, |since| changed_parts(since, &token)));
    reply
}

/// What `status` says, less what changes every second by itself (the time each timer shows,
/// how far the audio has played) and the token: what a change token's first part is made of.
fn without_clocks(status: &serde_json::Value) -> serde_json::Value {
    let mut said = status.clone();
    if let Some(timers) = said.get_mut("timers").and_then(serde_json::Value::as_array_mut) {
        for timer in timers.iter_mut().filter_map(serde_json::Value::as_object_mut) {
            timer.remove("shows");
        }
    }
    if let Some(audio) = said.get_mut("audio").and_then(serde_json::Value::as_object_mut) {
        audio.remove("seconds");
    }
    if let Some(said) = said.as_object_mut() {
        said.remove("token");
    }
    said
}

/// The hash of something as text, for a change token.
fn hash_of(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Hands a request to the app and waits that long for its reply.
/// Hands a request to the app and waits for its reply, with its place among those unwritten.
fn ask(deliver: &Deliver, request: Request, within: Duration) -> (Reply, Option<Unwritten>) {
    let (tx, rx) = std::sync::mpsc::channel();
    deliver(Incoming { request, reply: ReplyTo(tx) });
    match rx.recv_timeout(within) {
        Ok(Handed { reply, _unwritten }) => (reply, Some(_unwritten)),
        Err(_) => (Reply::failed("SoftPresenter did not answer in time."), None),
    }
}

fn write_reply(stream: &UnixStream, mut reply: Reply) {
    // Testing aid: SOFTPRESENTER_SLOW_REPLIES=<ms> waits that long before each reply is written,
    // as when the thread that writes it runs late on a busy computer, to see that the app doesn't
    // end before its reply to `menu Quit` is written. Only for a copy with its own data folder.
    static SLOW: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    let slow = SLOW.get_or_init(|| {
        let own_folder = std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|folder| !folder.is_empty());
        std::env::var("SOFTPRESENTER_SLOW_REPLIES")
            .ok()
            .and_then(|ms| ms.trim().parse::<u64>().ok())
            .filter(|_| own_folder)
            .map(|ms| Duration::from_millis(ms.min(10_000)))
    });
    if let Some(slow) = slow {
        std::thread::sleep(*slow);
    }
    // Scripts read the replies: in English whatever language the window speaks, even where a
    // reply repeats what the window said.
    reply.text = crate::i18n::english_of(&reply.text);
    let mut text = serde_json::to_string(&reply).unwrap_or_default();
    text.push('\n');
    let mut stream = stream;
    let _ = stream.write_all(text.as_bytes());
}

/// The command line: sends the words to the running app and prints its reply. `--json` prints
/// the reply as JSON. Returns the process's exit code.
/// Prints a reply as println! does, but a reader that stops early (sp … | head) ends the
/// printing quietly rather than with a "broken pipe" panic.
macro_rules! say {
    ($($words:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($words)*);
    }};
}

/// The same, for the error stream.
macro_rules! say_err {
    ($($words:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($words)*);
    }};
}

pub fn run_client(args: Vec<String>) -> std::process::ExitCode {
    let json = args.iter().any(|a| a == "--json");
    // `status --wait SECONDS` is the app's to carry out (wait for a change); after any other
    // command, `--wait` waits here for the job it started.
    let status = args.first().is_some_and(|word| word.eq_ignore_ascii_case("status"));
    let wait = !status && args.iter().any(|a| a == "--wait");
    let args = with_full_paths(args.into_iter().filter(|a| a != "--json" && (status || a != "--wait")).collect());
    // `voxlive connect`: the key is read from the standard input (piped in, or pasted), never
    // taken as an argument, where it would stay in the shell's history and show in the list of
    // processes.
    let connect = args.len() >= 2 && args[0].eq_ignore_ascii_case("voxlive") && args[1].eq_ignore_ascii_case("connect");
    if connect && args.len() > 2 {
        say_err!("Give the key on the standard input, not as an argument: echo \"$KEY\" | sp voxlive connect");
        return std::process::ExitCode::FAILURE;
    }
    // `canva connect` the same way: the client ID and secret on one line of the standard input.
    let canva = args.len() >= 2 && args[0].eq_ignore_ascii_case("canva") && args[1].eq_ignore_ascii_case("connect");
    if canva && args.len() > 2 {
        say_err!(
            "Give the client ID and secret on the standard input, not as arguments: echo \"$ID $SECRET\" | sp canva connect"
        );
        return std::process::ExitCode::FAILURE;
    }
    // `timelychurch connect [ADDRESS]` the same way: the pairing code on the standard input.
    let pairing =
        args.len() >= 2 && args[0].eq_ignore_ascii_case("timelychurch") && args[1].eq_ignore_ascii_case("connect");
    if pairing && args.len() > 3 {
        say_err!("Give the pairing code on the standard input: echo \"$CODE\" | sp timelychurch connect [ADDRESS]");
        return std::process::ExitCode::FAILURE;
    }
    // `remote password` the same way: the new password on one line of the standard input,
    // unshown when typed at a terminal.
    let password =
        args.len() >= 2 && args[0].eq_ignore_ascii_case("remote") && args[1].eq_ignore_ascii_case("password");
    if password && args.len() > 2 {
        say_err!("Give the password on the standard input, not as an argument: sp remote password");
        return std::process::ExitCode::FAILURE;
    }
    let args = if password {
        use std::io::{BufRead, IsTerminal};
        let typed = if std::io::stdin().is_terminal() {
            read_unshown("New Remote Control password (not shown): ")
        } else {
            None
        };
        let line = typed.unwrap_or_else(|| {
            let mut line = String::new();
            let _ = std::io::stdin().lock().read_line(&mut line);
            line
        });
        let mut args = args;
        args.push(line.trim_end_matches(['\r', '\n']).to_owned());
        args
    } else {
        args
    };
    let args = if connect || canva || pairing {
        use std::io::{BufRead, IsTerminal};
        let terminal = std::io::stdin().is_terminal();
        let mut line = String::new();
        if canva && terminal {
            // Typed at a terminal: the ID, then the secret, which isn't shown as it is typed.
            let mut id = String::new();
            eprint!("Client ID: ");
            let _ = std::io::stdin().lock().read_line(&mut id);
            let secret = match read_unshown("Client secret (not shown): ") {
                Some(secret) => secret,
                None => {
                    eprint!("Client secret: ");
                    let mut secret = String::new();
                    let _ = std::io::stdin().lock().read_line(&mut secret);
                    secret
                }
            };
            line = format!("{} {}", id.trim(), secret.trim());
        } else {
            if terminal && pairing {
                eprint!("Type the pairing code from TimelyChurch, then press Return: ");
            } else if terminal {
                eprint!("Paste the VoxLive API key, then press Return: ");
            }
            let _ = std::io::stdin().lock().read_line(&mut line);
        }
        let mut args = args;
        args.push(line.trim().to_owned());
        args
    } else {
        args
    };
    // Help needs no app: it is printed here; `help COMMAND` prints that command's lines.
    let reply = if args.first().is_none_or(|w| w.eq_ignore_ascii_case("help")) {
        match args.get(1) {
            Some(word) => match help_for(word) {
                Some(text) => Reply::done(text),
                None => Reply::failed(format!("No command {word}. sp help lists them all.")),
            },
            None => Reply::done(HELP.trim_end()),
        }
    } else {
        send(&args).unwrap_or_else(Reply::failed)
    };
    // `--wait`: a job the app does in the background (a song pack with videos) is waited for, and
    // its own reply printed.
    let reply = match reply.data.get("job").and_then(serde_json::Value::as_u64) {
        // A reply may ask to be waited for without --wait: a Bible file installed from the
        // command line answers with what came in, as it did before it was read in the
        // background.
        Some(job)
            if (wait || reply.data.get("wait").and_then(serde_json::Value::as_bool) == Some(true)) && reply.ok =>
        {
            loop {
                std::thread::sleep(Duration::from_millis(500));
                match send(&["job".to_owned(), job.to_string()]) {
                    Ok(now) if now.data.get("done").and_then(serde_json::Value::as_bool) == Some(false) => {}
                    Ok(now) => break now,
                    Err(problem) => break Reply::failed(problem),
                }
            }
        }
        _ => reply,
    };
    // A command that did what it was asked always says so.
    let reply =
        if reply.ok && reply.text.trim().is_empty() { Reply { text: "Done.".to_owned(), ..reply } } else { reply };
    if json {
        say!("{}", serde_json::to_string_pretty(&reply).unwrap_or_default());
    } else if reply.ok {
        say!("{}", reply.text);
    } else {
        say_err!("{}", reply.text);
    }
    if reply.ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::from(1) }
}

/// Reads a line typed at the terminal without showing it, through the system's shell: it turns
/// the terminal's echo off for the line and back on afterwards, and on Ctrl-C, Ctrl-Z or a hang-up
/// too (its own trap; nothing needs installing here). The line comes back through a pipe, never
/// as an argument. `None` where there is no such shell (the caller then reads it shown).
fn read_unshown(prompt: &str) -> Option<String> {
    if !cfg!(unix) {
        return None;
    }
    const READ: &str = "trap 'stty echo 2>/dev/null; echo >&2; exit 130' INT TSTP TERM HUP; \
                        stty -echo 2>/dev/null; IFS= read -r line; status=$?; stty echo 2>/dev/null; echo >&2; \
                        printf '%s' \"$line\"; exit $status";
    eprint!("{prompt}");
    let read = std::process::Command::new("/bin/sh")
        .args(["-c", READ])
        .stdin(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .output()
        .ok()?;
    // Stopped by Ctrl-C: the terminal is as it was, and so is everything else.
    if !read.status.success() {
        std::process::exit(130);
    }
    Some(String::from_utf8_lossy(&read.stdout).into_owned())
}

/// Whether the program was started through a link (`sp`), not by its own path or as an app.
/// Only Unix systems have such links (Windows runs the program by its own name).
#[cfg(unix)]
pub fn started_through_link() -> bool {
    let Some(started_as) = std::env::args_os().next().map(PathBuf::from) else { return false };
    let is_link = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    if started_as.components().count() > 1 {
        return is_link(&started_as);
    }
    // Started by name: found on the PATH.
    std::env::var_os("PATH")
        .and_then(|path| std::env::split_paths(&path).map(|dir| dir.join(&started_as)).find(|c| c.exists()))
        .is_some_and(|found| is_link(&found))
}

/// `sp` with nothing after it: what is on screen, when the app answers. Otherwise, with a test
/// socket named (SOFTPRESENTER_CONTROL_SOCKET), it says nothing answers and starts nothing;
/// else it starts the app: the bundle the link points into, when there is one. `None` means
/// the app is to be started in this process (a link to the program itself, outside a bundle).
#[cfg(unix)]
pub fn bare(bundle: Option<&Path>) -> Option<std::process::ExitCode> {
    const MORE: &str = "sp help lists the commands.";
    let answers = socket_path().is_some_and(|path| std::os::unix::net::UnixStream::connect(path).is_ok());
    if answers {
        return Some(match send(&["status".to_owned()]) {
            Ok(reply) if reply.ok => {
                say!("{}\n\n{MORE}", reply.text);
                std::process::ExitCode::SUCCESS
            }
            Ok(reply) => {
                say_err!("{}", reply.text);
                std::process::ExitCode::from(1)
            }
            Err(problem) => {
                say_err!("{problem}");
                std::process::ExitCode::from(1)
            }
        });
    }
    if let Some(own) = std::env::var_os("SOFTPRESENTER_CONTROL_SOCKET").filter(|p| !p.is_empty()) {
        say_err!("Nothing answers at {} (SOFTPRESENTER_CONTROL_SOCKET). {MORE}", Path::new(&own).display());
        return Some(std::process::ExitCode::from(1));
    }
    say!("SoftPresenter isn't running; starting it. {MORE}");
    let bundle = bundle?;
    Some(match std::process::Command::new("/usr/bin/open").arg(bundle).status() {
        Ok(status) if status.success() => std::process::ExitCode::SUCCESS,
        _ => {
            say_err!("Could not start {}.", bundle.display());
            std::process::ExitCode::from(1)
        }
    })
}

/// `groups`: each group name's colour and the letter key that jumps to it, as Settings › Groups
/// shows them (they are fixed, not settings).
fn ctl_groups() -> Reply {
    let mut lines = Vec::new();
    let mut data = Vec::new();
    for (key, names) in sp_model::GROUP_KEYS {
        let name = names.iter().find(|name| name.contains(' ')).or(names.first()).copied().unwrap_or("");
        let color = sp_model::group_color(name);
        let hex = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
        let mut shown = name.to_owned();
        if let Some(first) = shown.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        lines.push(format!("{}  {shown}  {hex}", key.to_ascii_uppercase()));
        data.push(serde_json::json!({"key": key.to_ascii_uppercase().to_string(), "group": shown, "names": names, "color": hex}));
    }
    Reply::with(lines.join("\n"), serde_json::json!(data))
}

/// The lines of the help for one command (the lines that begin with it, and those that go on
/// from them), or `None` when no line does.
fn help_for(word: &str) -> Option<String> {
    let word = word.to_lowercase();
    let mut lines = Vec::new();
    let mut taking = false;
    for line in HELP.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent == 2 {
            let first = trimmed.split(|c: char| c.is_whitespace() || c == '|').next().unwrap_or("").to_lowercase();
            taking =
                first == word || trimmed.to_lowercase().split(" | ").any(|part| part.starts_with(&format!("{word} ")));
            if taking {
                lines.push(line.to_owned());
            }
        } else if taking && indent > 2 {
            lines.push(line.to_owned());
        } else {
            taking = false;
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The words after `playlist` that change something; any other word names a playlist to list.
pub(crate) const PLAYLIST_CHANGES: &[&str] = &[
    "open",
    "add",
    "new",
    "go",
    "rename",
    "delete",
    "remove",
    "move",
    "header",
    "media",
    "media-playlist",
    "placeholder",
    "passage",
    "fill",
    "item",
];

/// The words after `slide` that change something, rather than name a song to read.
const SLIDE_CHANGES: &[&str] = &[
    "text",
    "style",
    "frame",
    "add",
    "delete",
    "duplicate",
    "move",
    "group",
    "label",
    "notes",
    "enable",
    "disable",
    "checked",
    "hotkey",
    "transition",
    "template",
    "media",
    "web",
];

/// Whether a command only reads: it changes nothing, so it isn't written to the log, and a
/// script or page asking every second or two (`status`) doesn't fill it. Only these shapes
/// count; any other command, a new one included, is logged.
pub fn only_reads(args: &[String]) -> bool {
    let word = |at: usize| args.get(at).map(|w| w.to_lowercase()).unwrap_or_default();
    let alone = args.len() == 1;
    match word(0).as_str() {
        // `songs find` looks, `songs find --import` brings songs in.
        "songs" => alone || (word(1) == "find" && !args.iter().any(|w| w == "--import")),
        "" | "help" | "status" | "slides" | "playlists" | "libraries" | "themes" | "screens" | "timers"
        | "messages" | "props" | "macros" | "groups" | "displays" | "decklink" | "bibles" | "search" | "job"
        | "songbooks" => true,
        "media" | "media-playlist" | "settings" | "menu" => alone,
        "version" => alone || (args.len() == 2 && word(1) == "list"),
        "audio" => alone || (args.len() == 2 && word(1) == "playlists"),
        "voxlive" => alone || (args.len() == 2 && word(1) == "screens"),
        "canva" => alone || (args.len() == 2 && word(1) == "status"),
        "timelychurch" => alone || (args.len() == 2 && matches!(word(1).as_str(), "services" | "newer")),
        "catalog" => {
            alone || matches!(word(1).as_str(), "status" | "languages" | "search" | "list" | "newer" | "sizes")
        }
        "remote" | "mirror" | "update" => alone || (args.len() == 2 && word(1) == "status"),
        "notices" => alone,
        // A start turned away asking (`one_copy`) is said in the log by `ctl_front`, as a start.
        "front" => args.len() == 2 && word(1) == "--start",
        "reports" => alone || (args.len() == 2 && word(1) == "show"),
        // `bible text REFERENCE` and `bible search WORDS` leave the Bible page as it is.
        "bible" => matches!(word(1).as_str(), "text" | "search"),
        // `playlist [NAME]`, not `playlist add …`.
        "playlist" => alone || !PLAYLIST_CHANGES.contains(&word(1).as_str()),
        // `slide SONG N` and `slide item N SLIDE …`, not `slide text …`.
        "slide" => {
            word(1) == "item"
                || (args.len() == 3 && args[2].parse::<usize>().is_ok() && !SLIDE_CHANGES.contains(&word(1).as_str()))
        }
        _ => false,
    }
}

/// A command's words as the log shows them: the key given to `voxlive connect`, the client ID
/// and secret given to `canva connect`, and the password given to `remote password`, are left
/// out.
pub fn shown_args(args: &[String]) -> Vec<String> {
    let secret = args.len() > 2
        && (((args[0].eq_ignore_ascii_case("voxlive")
            || args[0].eq_ignore_ascii_case("canva")
            || args[0].eq_ignore_ascii_case("timelychurch"))
            && args[1].eq_ignore_ascii_case("connect"))
            || (args[0].eq_ignore_ascii_case("remote") && args[1].eq_ignore_ascii_case("password")));
    if secret { vec![args[0].clone(), args[1].clone(), "…".to_owned()] } else { args.to_vec() }
}

/// The app doesn't share the terminal's folder, so a file or folder named in `song import`,
/// `song new` or `bible install` is sent as a full path. Only those commands: elsewhere a word
/// that happens to match a file in the folder (a song called "notes") is meant as words.
fn with_full_paths(args: Vec<String>) -> Vec<String> {
    let second = |words: &[&str]| args.get(1).is_some_and(|w| words.iter().any(|word| w.eq_ignore_ascii_case(word)));
    let first = |word: &str| args.first().is_some_and(|w| w.eq_ignore_ascii_case(word));
    // `song export … --to FILE`: the file isn't there yet, so it is made whole from the
    // terminal's folder.
    if first("song") && second(&["export"]) {
        let mut args = args;
        if let Some(at) = args.iter().position(|w| w == "--to")
            && let Some(word) = args.get(at + 1)
            && Path::new(word).is_relative()
            && let Ok(here) = std::env::current_dir()
        {
            args[at + 1] = here.join(word).to_string_lossy().into_owned();
        }
        return args;
    }
    // `bible install FILE [--name …]`: only the file is made whole, never a name given after it,
    // nor the name of a Bible offered (a folder called "rob" in the terminal's folder is not
    // the Russian Open Bible).
    if first("bible") && second(&["install"]) {
        let mut args = args;
        if let Some(word) = args.get(2)
            && crate::bible::offered_named(word).is_none()
            && Path::new(word).exists()
            && let Ok(full) = Path::new(word).canonicalize()
        {
            args[2] = full.to_string_lossy().into_owned();
        }
        return args;
    }
    // `songs find --in FOLDER`: each folder made whole.
    if first("songs") && second(&["find"]) {
        let mut args = args;
        for at in 2..args.len() {
            if args[at - 1] == "--in"
                && Path::new(&args[at]).is_relative()
                && let Ok(here) = std::env::current_dir()
            {
                args[at] = here.join(&args[at]).to_string_lossy().into_owned();
            }
        }
        return args;
    }
    let names_files = (first("song") && second(&["import", "new", "text"]))
        || (first("media") && second(&["add"]))
        || (first("audio") && second(&["add"]))
        || (first("prop") && second(&["new"]))
        || (first("media-playlist") && second(&["follow"]));
    if !names_files {
        return args;
    }
    let args_were = args.clone();
    args.into_iter()
        .enumerate()
        .map(|(at, word)| match Path::new(&word) {
            // The word after `--as` says how, and is never a file.
            _ if at > 2 && args_were.get(at - 1).is_some_and(|w| w == "--as") => word,
            path if at >= 2 && path.exists() => {
                path.canonicalize().map_or(word.clone(), |full| full.to_string_lossy().into_owned())
            }
            _ => word,
        })
        .collect()
}

fn send(args: &[String]) -> Result<Reply, String> {
    send_to(socket_path, args)
}

/// Sends one request to the app listening at the socket file `at` finds, and reads its reply.
pub(crate) fn send_to(at: impl FnOnce() -> Option<PathBuf>, args: &[String]) -> Result<Reply, String> {
    send_as(at, args, None)
}

/// The same, marked as coming from an AI assistant through `sp mcp` when `mcp` is given.
pub(crate) fn send_as(
    at: impl FnOnce() -> Option<PathBuf>,
    args: &[String],
    mcp: Option<&McpCall>,
) -> Result<Reply, String> {
    send_within(at, args, mcp, None)
}

/// Asks the app running to bring its window to the front, as a start turned away by it does
/// (`one_copy`): `front --start`, waiting at most `within` for the answer, which a copy still
/// starting may be slow to give. True when it answered that it did.
pub(crate) fn ask_front(within: Duration) -> bool {
    let args = ["front".to_owned(), "--start".to_owned()];
    send_within(socket_path, &args, None, Some(within)).is_ok_and(|reply| reply.ok)
}

/// `send_as`, waiting for the reply at most `most` when it is given (else 20 seconds, and the
/// time a wait for a change asks for).
fn send_within(
    at: impl FnOnce() -> Option<PathBuf>,
    args: &[String],
    mcp: Option<&McpCall>,
    most: Option<Duration>,
) -> Result<Reply, String> {
    let request = Request { v: PROTOCOL, args: args.to_vec(), mcp: mcp.cloned() };
    let probe = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    if probe.len() as u64 >= MAX_REQUEST {
        return Err(format!(
            "That command is {} KB; one may be at most {} KB. Give long words in a file (song text SONG FILE).",
            probe.len() / 1024,
            MAX_REQUEST / 1024
        ));
    }
    let path = at().ok_or("Could not find where the control socket is.")?;
    let mut stream = UnixStream::connect(&path).map_err(|_| {
        "SoftPresenter isn't running (or isn't listening for commands). Open it, then try again.".to_owned()
    })?;
    // A wait for a change is answered when it is up at the latest.
    let waiting = wait_for(args).and_then(Result::ok).map_or(Duration::ZERO, |(wait, _)| wait);
    let within = most.map_or(Duration::from_secs(20) + waiting, |most| most.max(Duration::from_millis(100)));
    let _ = stream.set_read_timeout(Some(within));
    if most.is_some() {
        let _ = stream.set_write_timeout(Some(within));
    }
    let mut request = probe;
    request.push('\n');
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| e.to_string())?;
    serde_json::from_str(line.trim()).map_err(|e| format!("Could not read SoftPresenter's answer: {e}"))
}

/// The first words of the commands, so `softpresenter next` can be typed without `ctl`.
const COMMANDS: &[&str] = &[
    "help",
    "status",
    "songs",
    "slides",
    "slide",
    "playlists",
    "libraries",
    "library",
    "playlist",
    "media",
    "media-playlist",
    "voxlive",
    "canva",
    "timelychurch",
    "themes",
    "screens",
    "timers",
    "messages",
    "props",
    "macros",
    "audio",
    "next",
    "previous",
    "prev",
    "back",
    "first",
    "last",
    "go",
    "select",
    "clear",
    "audience",
    "stage",
    "identify",
    "view",
    "message",
    "prop",
    "timer",
    "macro",
    "announce",
    "bible",
    "song",
    "undo",
    "redo",
    "job",
    "search",
    "group",
    "hotkey",
    "continuous",
    "video",
    "test",
    "bibles",
    "screen",
    "settings",
    "groups",
    "displays",
    "decklink",
    "theme",
    "version",
    "menu",
    "front",
    "remote",
    "mirror",
    "notices",
    "reports",
    "catalog",
    "songbooks",
    "songbook",
    "update",
];

/// True when `word` starts a command.
pub fn is_command(word: &str) -> bool {
    COMMANDS.contains(&word.to_lowercase().as_str())
}

/// The app bundle this program is in, when it was started through a link from outside it (the
/// installed `softpresenter` command): opening the bundle names and places the app properly.
#[cfg(target_os = "macos")]
pub fn bundle_opened_through_link() -> Option<PathBuf> {
    let started_as = PathBuf::from(std::env::args_os().next()?);
    let link = std::fs::symlink_metadata(&started_as).ok().filter(|meta| meta.file_type().is_symlink());
    let started_as = match link {
        Some(_) => started_as,
        // Started by name: find it on the PATH.
        None if started_as.components().count() == 1 => {
            let path = std::env::var_os("PATH")?;
            std::env::split_paths(&path).map(|dir| dir.join(&started_as)).find(|candidate| {
                std::fs::symlink_metadata(candidate).is_ok_and(|meta| meta.file_type().is_symlink())
            })?
        }
        None => return None,
    };
    let program = std::fs::canonicalize(&started_as).ok()?;
    let bundle = program.ancestors().find(|dir| dir.extension().is_some_and(|ext| ext == "app"))?;
    Some(bundle.to_path_buf())
}

/// What `ctl help` says.
pub const HELP: &str = "\
sp COMMAND …   (the same as softpresenter COMMAND …; add --json for the reply as JSON)
sp             what is on screen; starts SoftPresenter first if it isn't running

What is going on
  status                       what is on screen, what is next, what is playing
  status --wait SECONDS [--since TOKEN]
                               the same once something changes from TOKEN (status --json gives
                               one; without it, from now), or when SECONDS (up to 30) are up
  songs | slides SONG | playlists | playlist [NAME] | media | themes | screens
  timers | messages | props | macros | audio | menu
  slides item N [in PLAYLIST]  the slides of a playlist's item N: a song, or a saved Bible passage
  slides SONG --full           every slide with its objects and looks (as slide SONG N), in one go

Running the show
  next | previous (or back) | first | last
  next|previous --aim ITEM [--rev REV]   as if ITEM of the playlist in front were the open item,
                               nothing picked (a remote page's); the window's own stay as they are
  go --aim ITEM [SLIDE] [--rev REV]      a slide of that item on screen (its first without
                               SLIDE), leaving the window's open item alone; REV refuses it if
                               the playlist changed since the page saw it
  group NAME|LETTER [in SONG]  a group's first slide on screen (C for the chorus…)
  hotkey KEY                   the slide with that hot key on screen
  playlist go ITEM [SLIDE] [in PLAYLIST]   open a playlist item as a click does (and a slide
                               on screen, a passage's too; 1 shows a picture or video, or
                               plays a media playlist)
  continuous on|off            after a song's last slide, go on to the playlist's next item
  video play|pause|restart | video seek SECONDS | video loop on|off | video mute on|off
  media next|previous          the next or previous picture or video behind the words
  media set ITEM scaling=fit|fill|stretch loop=on|off
  stage message TEXT | stage message clear      words for the stage screens only
  stage layouts | stage layout SCREEN LAYOUT    the stage layouts; which one a screen shows
  stage objects LAYOUT         a stage layout's objects, what each shows and where
  stage picture LAYOUT main|SCREEN|off   a picture of what an audience screen shows, live
  stage frame LAYOUT OBJECT x= y= w= h=  where one of a stage layout's objects sits
  test bars|grid|white|off     a test pattern on the audience screens
  go SONG [SLIDE]              put a slide on screen (SONG by name or number; SLIDE from 1)
  select SONG                  show a song's slides without putting anything on screen
  clear [all|slide|media|props|messages|audio|announcements]
  audience on|off              stage on|off           identify on|off
  view service|bible|edit|themes

Layers and controls
  media show ITEM | media add FILE… | media samples | media delete ITEM --yes
  media-playlist play NAME | media-playlist stop
  message show NAME | message hide
  prop show NAME | prop hide NAME
  timer start|stop|reset NAME
  timer new NAME countdown M:SS | to HH:MM | up [to M:SS]     (30 alone is 30 minutes)
  timer set NAME countdown …|to …|up … | overrun on|off | name NEW
  timer rename OLD to NEW
  timer delete NAME
  message new NAME [WORDS] | message set NAME text=… leaves=SECONDS|never name=…
  message delete NAME | message show --text WORDS   (words once, as Quick message)
  prop new NAME [PICTURE] | prop delete NAME
  macro new NAME | macro add NAME STEP [then STEP…] | macro remove NAME N | macro delete NAME
                               steps: clear all|slide|media|props|messages, show|hide message
                               NAME, show|hide prop NAME, start|stop|reset timer NAME,
                               audience on|off, song SONG, media ITEM, announce SONG|media
                               ITEM|media-playlist NAME, canva sync, version NAME [keep]|usual
  macro step NAME N STEP       step N does STEP instead, at the same point
  macro move NAME N to M       step N becomes step M
  macro startup NAME on|off    run it when SoftPresenter opens, or not
  macro run NAME
  announce SONG | announce media ITEM | announce media-playlist NAME | announce stop
  audio play TRACK | audio pause | audio stop | audio volume 0-100 | audio mute on|off
  audio next|previous | audio shuffle on|off | audio through on|off | audio crossfade SECONDS
  audio loop on|off            after the last track, the first again (Play through off: the same one)
  audio seek TIME              go to 1:23, 83 (seconds), +15 or -15 in the track playing or paused
  audio add FILE… [to PLAYLIST] | audio playlists | audio playlist new NAME | delete NAME
  media-playlist new NAME | media-playlist add ITEM… to NAME | remove ITEM… from NAME
  media-playlist follow NAME FOLDER|off | media-playlist delete NAME --yes
  voxlive                      whether VoxLive is connected, and how many screens it has
  voxlive screens              the church's VoxLive screens
  voxlive connect              connect with an API key read from the standard input
                               (paste it, or pipe it in; never as an argument)
  voxlive show N|NAME          that VoxLive screen's captions on screen, in place of a verse
  voxlive look N|NAME [setting=value …]   a screen's look, or settings of it saved to VoxLive
  voxlive refresh | voxlive disconnect --yes      find the screens again | forget the key
  canva | canva status         Canva Sync: signed in or not, the presentation and its divider
                               page, a sync under way, the last video made
  canva sync [--wait]          fetch a fresh announcements video from Canva now (a background
                               job: add --wait to wait for it, which takes minutes)
  canva connect                sign in, with the integration's client ID and secret read from
                               the standard input on one line (Canva's page opens in the browser)
  canva import                 take over the old sync script's sign-in on this computer
  canva presentation LINK | canva pages | canva divider N      the presentation, its pages, and
                               the divider page (the pages above it go in the video)
  canva disconnect --yes       forget the sign-in on this computer
  bible REFERENCE [--show]     look a passage up (and put its first verse on screen)
  bible REFERENCE --add [to PLAYLIST]
                               look it up and put it in a playlist, with the page's rows
                               and options, as Add to does
  bible text REFERENCE         a passage's verses in the page's translations, leaving the page
                               as it is
  bible search WORDS           verses with those words, in the first translation
  bible rows [BIBLE…]          the translations shown (KJV RST …), first to last
  bible options [verses=on|off break=on|off translation=on|off reference=verse|passage|last|none]
  bible history                the passages looked up
  bible view [page|slides]     the chapter as a page to read, or the passage's slides
  bible books [keypad|lists]   the books as a keypad, or as lists
  bible language [auto|en|ru|uk]   the language the operator reads the page in (never the slides)
  bibles | bible install NAME|FILE | bible remove NAME --yes

Songs, libraries and playlists
  song new TITLE [FILE.txt]    a new song, its words from a song text file
  song new TITLE --words TEXT  a new song, its words given as song text (one word, quoted)
  song import FILE… | FOLDER…  import songs (text, lyric, ChordPro, rich text, Word, .pro,
                               .proBundle); a folder becomes a library of its name
  songs find [--in FOLDER]…    look for songbooks, libraries and song files on this computer
                               (Documents, Downloads, Desktop, SoftProjector's folder, and
                               each FOLDER), as Settings › Song files does: lists each,
                               numbered, with its songs new and already here. Nothing comes in
  songs find --import N…|NAME  bring in the new songs of those numbered (or the one named),
                               each into a library of its name; songs already here stay out
  libraries                    the libraries, with how many songs each has
  library NAME                 list a library's songs
  library open NAME | all      show a library's songs, or all songs
  song delete SONG --yes
  playlist open NAME
  playlist add SONG [to PLAYLIST]               (VoxLive names the VoxLive item)
  playlist new NAME | playlist rename OLD to NEW | playlist delete NAME
  playlist remove ITEM [from PLAYLIST]          (ITEM by number, title or uid)
  playlist move ITEM to N [in PLAYLIST]
  playlist header TEXT [at N] [in PLAYLIST]
  playlist media ITEM [at N] [in PLAYLIST]      a picture or video from the media bin
  playlist media-playlist NAME [at N] [in PLAYLIST]   a media playlist, to play round
  playlist placeholder NAME [at N] [in PLAYLIST]      a place kept for something to come
  playlist passage REFERENCE [at N] [in PLAYLIST]     a Bible passage, as bible --add keeps it,
                               leaving the Bible page as it is
  playlist fill PLACEHOLDER with SONG | with media ITEM [in PLAYLIST]
  library new NAME | library rename OLD to NEW | library delete NAME (songs stay)
  library add SONG… to LIBRARY | library remove SONG… from LIBRARY
  library tidy [--apply] [--keep-numbers] [--titles-from FOLDER]   put songs' titles and words right,
                               as an import does (lists every change; --apply makes them, keeping a
                               copy of each file; FOLDER: where they were imported from, to name
                               them after their files)
  song rename SONG to TITLE | song duplicate SONG [as TITLE]
  song theme SONG THEME|main | song transition SONG KIND [SECONDS] | none
  song info SONG [artist=… author=… publisher=… year=… ccli=…]
  search WORDS                 songs with those words in their title or slides
  song arrangements SONG | song arrangement SONG use NAME|none
  song arrangement SONG set NAME GROUP… | song arrangement SONG delete NAME
  song arrangement SONG chorus-after   sing the chorus after each verse (made or brought up to
                               date, and used), as the arrangement menu's Chorus After Each Verse

Song catalog (free songs from Worship Leader App, kept apart from the library)
  catalog                      the languages downloaded: songs, sizes, when fetched and checked
  catalog search WORDS [--limit N]   songs of the catalog with those words in their title or
                               first lines (not the ones already in the library), with keys
  catalog list LANG [FROM [COUNT]]   a language's songs in title order
  catalog add KEY [--second LANG]    put a song in the library (the library Worship Leader App),
                               with its version in LANG under its words; KEY may be its start
  catalog newer                songs of the library with a newer version waiting, or dropped
  catalog use-newer SONG | catalog keep-mine SONG   take the newer version (Undo brings yours
                               back), or keep yours and not be offered it again
  catalog download LANG | catalog check | catalog remove LANG --yes
                               fetch a language (a job: --wait waits), ask the source now
                               whether the ones here changed, or take a language off this computer
  catalog sizes [LANG…] [--ask]      songs, download size and room on disk, per language

Free songbooks (shared by churches, downloaded from the page that shares them when installed)
  songbooks                    every one offered: language, songs, size, installed or not
  songbook install NAME        download one and bring its songs into the library, in a library
                               of its own (a job: --wait waits); NAME as songbooks lists it
  songbook remove NAME --yes   take its songs back out (to the Trash), but those you changed
                               and those a playlist or another library uses, which stay as yours

Themes
  themes | theme templates THEME | theme main THEME
  theme duplicate THEME [as NAME] | theme rename THEME to NAME | theme delete THEME --yes
  theme box THEME TEMPLATE BOX key=value…   a template box's look (slide style keys) and place
                               (x= y= w= h=); quote names with spaces
  theme template THEME TEMPLATE for=songs|bible|both|none
  version list                 the theme versions (other templates for a moment in the service,
                               such as the offering), the themes that have each, and the one on
  version add NAME to THEME [from TEMPLATE…]
                               give a theme a version, starting from copies of its templates
                               (its song templates if none are named); the name another theme's
                               version has makes it that version, so the screens switch together
  version rename OLD NEW | version remove NAME from THEME
  version use NAME [--keep] | version off
                               every screen whose theme has it in that version, until the next
                               item goes on screen (with --keep, until switched back); off is
                               the usual look. With nothing on screen it waits for the next item
  version keep on|off          the version on stays after the next item, until switched back
  playlist item N show-with NAME|usual [--keep] [in PLAYLIST]
                               a song or passage of a playlist goes on screen in that version
                               (the song itself doesn't change)
  song text SONG [FILE]        the song as song text, or its words replaced from a file (its
                               slides keep their looks, labels and notes)
  song reload [SONG [--now]]   read songs again from their files, if changed outside the app
                               (all of them, and songs put in the Songs folder, with no SONG);
                               one on screen or being edited waits unless --now
  song keep SONG               save SoftPresenter's version over a file changed outside it
  song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]
                               a song pack of the songs (and the playlist), with the themes
                               they set, their pictures and videos, to open on another computer
  song import FILE.sppack [--skip|--replace|--keep-both]
                               a song pack's songs; ones already here are skipped unless told
                               (a pack with videos goes on in the background: add --wait to
                               wait for it, or ask with job N)
  song import FILE.pptx [--as slides|pictures]
                               a presentation as editable slides (read from the file, the usual
                               way for .pptx), or as pictures of its slides (exact look; needs an
                               app on this computer that opens it)

Changing a slide (each change is saved, can be undone, and shows on screen at once)
  slide SONG N [--json]        the slide's objects: words, place and look (* = the slide's own)
  slide item N SLIDE [in PLAYLIST]   the same for a slide of a playlist's item N
  slide text SONG N [OBJECT] WORDS
                               an object's words (\\n starts a new line); OBJECT is a number,
                               name or role, the main text if left out
  slide style SONG N|all [OBJECT] key=value…
                               an object's look: size=90 font=… weight=bold italic=on
                               color=#ffffff align=center …; key=reset, or reset alone, for the
                               theme's look; `slide style keys` lists them all
  slide frame SONG N|all OBJECT x= y= w= h=
                               where an object sits, in the slide's pixels (reset: the theme's)
  slide add SONG [after N] [group NAME] WORDS   a new slide (at the end unless after N)
  slide delete SONG N | slide duplicate SONG N | slide move SONG N to M
  slide group SONG N NAME      slide N starts a group of that name (or renames its group)
  slide label SONG N LABEL|none | slide notes SONG N TEXT|none
  slide enable SONG N | slide disable SONG N     (a disabled slide is passed over)
  slide checked SONG N         take the tag off a slide that came in from a presentation needing a look
  slide hotkey SONG N KEY|none | slide transition SONG N KIND [SECONDS] | none
  slide template SONG N TEMPLATE|auto | slide media SONG N MEDIA-ITEM|none
  slide web SONG N URL|off     a live web page on the slide (a box filling it, behind the
                               words, if it has none)

  Put a song name with spaces in quotes: slide style \"Blessed Assurance\" all size=90

Screens and settings
  screens                      each screen and what it is doing
  displays                     the displays connected now (names for screen add … on display)
  decklink                     the DeckLink outputs, their formats and keying (numbered)
  screen add audience|stage NAME [on display NAME|window|ndi NAME|decklink NAME|automatic]
  screen set SCREEN key=value… name= output=display:NAME|window|ndi:NAME|decklink:NAME|
                               automatic|off mode=FORMAT (a DeckLink output's: 1080i59.94…)
                               keyer=off|external|internal (a DeckLink card's keyer: keyed,
                               see-through and premultiplied unless alpha= says; off, solid)
                               kind=audience|stage layers=slides,media,… color=#RRGGBB
                               edges=L,T,R,B theme=THEME|main layout=STAGE-LAYOUT
                               background=solid|see-through alpha=straight|premultiplied
                               key=SCREEN|none (shows that screen's key: white where solid)
                               sound=on|off (an NDI screen sends what the app plays; on)
  screen delete SCREEN --yes
  settings                     the choices kept between runs
  groups                       each group's colour, and the letter key that jumps to it
  settings set key=value…      transition=KIND[:SECONDS] media_transition=… copyright=never|first|
                               last|every ccli_licence=N start_audience=on|off continuous=on|off
                               end_mark=on|off (a mark under a song's last slide as sung)
                               start_with=nothing|media-playlist:NAME|announcements:SONG
                               view=grid|words|table columns=1-12
                               language=auto|en|es|ru|uk (the app's own words)
  view words [SONG]            Edit Words: a song's words as text to change (view reflow too)
  help COMMAND                 the help for one command

Undo
  undo                         undo the last change to a song, whatever the window shows
  redo                         do it again

Everything on the menu bar
  menu                         list the menu items
  menu ITEM                    choose one, as a click on it does

The window
  front                        bring it to the front, restored if it was minimised, as starting
                               SoftPresenter again while it runs does (a quiet copy's stays behind)

Updates (asked only when told to; closed to remote pages and AI assistants)
  update [status]              what the last check found, and when
  update check                 ask now whether there is a newer version, and wait for the answer:
                               the newest, or the newer version and its download page

Notices (the list under the status button)
  notices                      the entries: information, trouble and questions, each with its
                               number, time and, for a question, the answers it takes
  notices answer N ANSWER      answer question N with one of its answers (outside:reload,
                               outside:keep, mirror:allow, mirror:deny), as its button does
  notices seen | notices clear mark every entry seen, as closing the list does | Clear All
                               (a question stays until it is answered)

Anonymous reports (Settings › About)
  reports                      whether they are on, and how many have gone this run
  reports show                 the next report, exactly as it would be sent
  reports on | reports off     switch them (off stops at once and deletes what was waiting)
  reports reset-id             a new random anonymous id

Remote Control (a web page on the church's network that runs the show)
  remote [status]              on or off, the address to open, the certificate's fingerprint,
                               the pages connected and the devices remembered
  remote on | remote off       listen, or stop (every page's connection closes)
  remote password              set the password, read from the standard input (unshown when
                               typed); every device remembered is forgotten
  remote port N                the port (1024 up; 4316 unless changed)
  remote forget ID|all         forget a device: it signs in again, and its pages close now
  remote disconnect N          close page N's connection (its device stays remembered)

AirPlay (an iPhone, iPad or Mac shows its screen in a box on a slide; unofficial)
  mirror [status]              on or off, the name devices see, who is showing a screen, who
                               is waiting for an answer
  mirror on | mirror off       start receiving, or stop (off until switched on)
  mirror name NAME             the name devices see
  mirror ask on|off            ask before each device is let in (on unless changed)
  mirror allow [ID] | mirror deny [ID]   answer a device that is waiting
  mirror stop                  end the screen being shown now

AI assistants (answered by sp itself, without the app; docs/mcp.md)
  mcp                          the MCP server an assistant starts; not for typing
  mcp status                   the two switches, and whether each assistant app found here has
                               SoftPresenter (Claude Desktop, Claude Code, ChatGPT / Codex,
                               Cursor, VS Code / GitHub Copilot, Antigravity, Grok Build)
  mcp connect APP [--replace]  add SoftPresenter to an app, as the card's Add does: desktop, code,
                               chatgpt, cursor, copilot, antigravity or grok (--replace points an
                               entry that starts another program at this copy)
  mcp disconnect APP           take it out again

Names can be typed in part (\"amazing\" finds \"Amazing Grace\"); a name that fits several
things lists them instead of guessing. Numbers are as the lists show them, from 1. A song can
also be named by its id, as song info and the JSON replies give it.
";

/// `job N`'s reply: how the job went, that it is still going, or that this run has no such job.
/// Each carries the job's number and the run's word (`run`), so a job of an earlier run, whose
/// number this run has given again, can be told from this run's; `done` is there only when the
/// job is this run's.
fn job_reply(job: u64, kept: Option<&Option<Result<String, String>>>, run: &str) -> Reply {
    let facts = |done: Option<bool>| match done {
        Some(done) => serde_json::json!({"job": job, "run": run, "done": done}),
        None => serde_json::json!({"job": job, "run": run}),
    };
    match kept {
        None => Reply { ok: false, text: format!("There is no job {job}."), data: facts(None) },
        Some(None) => Reply::with(format!("Job {job} is still going."), facts(Some(false))),
        Some(Some(Ok(said))) => Reply::with(said.clone(), facts(Some(true))),
        Some(Some(Err(problem))) => Reply { ok: false, text: problem.clone(), data: facts(Some(true)) },
    }
}

/// A thing found by what was typed: its place in the list.
pub(crate) fn find(kind: &str, names: &[String], typed: &str) -> Result<usize, String> {
    let wanted = typed.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(format!("Which {kind}?"));
    }
    let exact: Vec<usize> = (0..names.len()).filter(|at| names[*at].to_lowercase() == wanted).collect();
    match exact.as_slice() {
        [one] => return Ok(*one),
        [] => {}
        many => {
            return Err(format!(
                "Several {kind}s are called \u{201c}{typed}\u{201d}: {}. Give its number instead.",
                many.iter().map(|at| format!("{} ({})", names[*at], at + 1)).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    if let Ok(number) = wanted.parse::<usize>()
        && (1..=names.len()).contains(&number)
    {
        return Ok(number - 1);
    }
    let close: Vec<usize> = (0..names.len()).filter(|at| names[*at].to_lowercase().contains(&wanted)).collect();
    match close.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("No {kind} called \u{201c}{typed}\u{201d}.")),
        many => Err(format!(
            "\u{201c}{typed}\u{201d} fits several {kind}s: {}. Type more of the name, or its number.",
            many.iter().map(|at| format!("{} ({})", names[*at], at + 1)).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Which slides differ between two versions of a song, as words to follow its name:
/// ", slide 2", ", slides 2 and 5", ", its slides (now 6)", or nothing when only the song's
/// other settings changed.
fn changed_slides(before: &sp_model::Presentation, after: &sp_model::Presentation) -> String {
    if before.slide_count() != after.slide_count() {
        return format!(", its slides (now {})", after.slide_count());
    }
    let changed: Vec<String> = (0..after.slide_count())
        .filter(|at| before.slide(*at).map(|(_, slide)| slide) != after.slide(*at).map(|(_, slide)| slide))
        .map(|at| (at + 1).to_string())
        .collect();
    match changed.as_slice() {
        [] => String::new(),
        [one] => format!(", slide {one}"),
        [rest @ .., last] => format!(", slides {} and {last}", rest.join(", ")),
    }
}

/// Lists names, numbered from 1, one to a line.
pub(crate) fn numbered(names: &[String]) -> String {
    if names.is_empty() {
        return "(none)".to_owned();
    }
    names.iter().enumerate().map(|(at, name)| format!("{:>3}. {name}", at + 1)).collect::<Vec<_>>().join("\n")
}

/// Words joined back up, for names with spaces typed without quotes.
fn rest(args: &[String], from: usize) -> String {
    args.get(from..).map(|words| words.join(" ")).unwrap_or_default()
}

/// An item a remote page has open, named in `next|previous|go --aim ITEM [--rev REV]`: the
/// page keeps its own open item, so it says which one each time. ITEM is the item's number in
/// the playlist in front, or its title; REV, when given, is the playlist's `rev` as the page
/// last saw it, so a playlist changed meanwhile is refused rather than the wrong item shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aim {
    pub item: String,
    pub rev: Option<String>,
}

/// Takes `--aim ITEM` and `--rev REV` out of a command's words: the words left, and the aim.
fn aim_words(words: &[String]) -> Result<(Vec<String>, Option<Aim>), String> {
    let mut left = Vec::new();
    let (mut item, mut rev) = (None, None);
    let mut words = words.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--aim" => {
                item = Some(words.next().ok_or("--aim ITEM: the item's number in the playlist in front")?.clone())
            }
            "--rev" => rev = Some(words.next().ok_or("--rev REV: the playlist's rev as last seen")?.clone()),
            _ => left.push(word.clone()),
        }
    }
    match (item, rev) {
        (Some(item), rev) => Ok((left, Some(Aim { item, rev }))),
        (None, Some(_)) => Err("--rev goes with --aim ITEM.".to_owned()),
        (None, None) => Ok((left, None)),
    }
}

fn on_off(word: Option<&String>) -> Result<bool, String> {
    match word.map(|w| w.to_lowercase()).as_deref() {
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        _ => Err("Say on or off.".to_owned()),
    }
}

impl crate::App {
    /// Carries out one command from outside the window, as the window's own controls do, and
    /// says what happened.
    pub(crate) fn control(&mut self, el: &ActiveEventLoop, args: &[String]) -> Reply {
        let held_before = self.held_songs.clone();
        self.last_job = None;
        let word = args.first().map(|w| w.to_lowercase()).unwrap_or_default();
        let result = match word.as_str() {
            "" | "help" => Ok(Reply::done(HELP.trim_end())),
            // `status --wait …` comes here only to be answered at once: something changed since
            // its token (or it waits 0 seconds). A wait still to come is held by `hold_wait`.
            "status" => match wait_for(args) {
                None => Ok(self.ctl_status()),
                Some(Err(usage)) => Err(usage),
                Some(Ok((_, since))) => Ok(with_changes(self.ctl_status(), since.as_deref())),
            },
            "songs" => {
                if args.get(1).is_some_and(|w| w.eq_ignore_ascii_case("find")) {
                    self.ctl_songs_find(args)
                } else {
                    Ok(self.ctl_songs())
                }
            }
            "slides" => self.ctl_slides(args),
            "slide" => self.ctl_slide(args),
            "playlists" => Ok(self.ctl_playlists()),
            "libraries" => Ok(self.ctl_libraries()),
            "library" => self.ctl_library(args),
            "playlist" => self.ctl_playlist(args),
            "media" => self.ctl_media(args),
            "media-playlist" => self.ctl_media_playlist(args),
            "voxlive" => self.ctl_voxlive(args),
            "canva" => self.ctl_canva(args),
            "timelychurch" => self.ctl_timelychurch(args),
            "catalog" => self.ctl_catalog(args),
            "songbooks" => self.ctl_songbooks(),
            "songbook" => self.ctl_songbook(args),
            "themes" => Ok(self.ctl_named_list("themes", |s| s.theme_names.clone())),
            "screens" => Ok(self.ctl_screens()),
            "timers" => Ok(self.ctl_timers()),
            "messages" => Ok(self.ctl_messages()),
            "props" => Ok(self.ctl_props()),
            "macros" => Ok(self.ctl_macros()),
            "audio" => self.ctl_audio(args),
            "next" | "previous" | "prev" | "back" | "first" | "last" => (|| {
                // Next and Previous as the keys do them: on the open item, on into the next
                // song or passage, or staying put with a word why. `--aim ITEM` acts on that
                // item of the playlist in front as if it were the open one (a remote page's).
                let (words, aim) = aim_words(args.get(1..).unwrap_or_default())?;
                let stepping = matches!(word.as_str(), "next" | "previous" | "prev" | "back");
                if !words.is_empty() || (aim.is_some() && !stepping) {
                    return Err(format!("{word}{}", if stepping { " [--aim ITEM [--rev REV]]" } else { "" }));
                }
                let aim = aim.map(|aim| self.aimed_item(&aim)).transpose()?;
                let said = match word.as_str() {
                    "next" => self.step_aimed(true, aim),
                    "first" => {
                        self.apply(vec![Command::First]);
                        None
                    }
                    "last" => {
                        self.apply(vec![Command::Last]);
                        None
                    }
                    _ => self.step_aimed(false, aim),
                };
                // Staying at an end is marked in the data, so a page can leave it unsaid as the
                // window does.
                Ok(match said {
                    Some(said) if said.at_end => Reply::with(
                        format!("{} {}", said.words, self.live_words()),
                        serde_json::json!({ "at_end": true }),
                    ),
                    Some(said) => Reply::done(format!("{} {}", said.words, self.live_words())),
                    None => Reply::done(self.live_words()),
                })
            })(),
            "go" => match aim_words(args.get(1..).unwrap_or_default()) {
                Ok((words, Some(aim))) => self.aimed_item(&aim).and_then(|item| self.go_aimed(item, &words)),
                Ok((_, None)) => self.ctl_go(args),
                Err(usage) => Err(usage),
            },
            "select" => self.ctl_select(&rest(args, 1)),
            "clear" => self.ctl_clear(args.get(1).map(String::as_str).unwrap_or("all")),
            "audience" => on_off(args.get(1)).map(|on| {
                self.apply(vec![Command::SetOutputEnabled(on)]);
                Reply::done(format!("The audience screens are {}.", if on { "on" } else { "off" }))
            }),
            "stage" if self.ctl_stage_more_applies(args) => self.ctl_stage_more(args).unwrap_or_else(|| {
                Err("stage on|off | stage message TEXT|clear | stage layout SCREEN LAYOUT | stage objects LAYOUT | \
                     stage picture LAYOUT main|SCREEN|off | stage frame LAYOUT OBJECT x= y= w= h="
                    .to_owned())
            }),
            "stage" => on_off(args.get(1)).map(|on| {
                self.with_state(|state| state.stage_on = on);
                Reply::done(format!("The stage screens are {}.", if on { "on" } else { "off" }))
            }),
            "identify" => on_off(args.get(1)).map(|on| {
                self.with_state(|state| state.identify = on);
                Reply::done(if on { "Identifying the screens." } else { "Stopped identifying the screens." })
            }),
            // `view reflow` is the old word for it, kept for scripts.
            "view"
                if args.get(1).is_some_and(|w| w.eq_ignore_ascii_case("words") || w.eq_ignore_ascii_case("reflow")) =>
            {
                self.view_words(&rest(args, 2))
            }
            "view" => self.ctl_view(args.get(1).map(String::as_str).unwrap_or("")),
            "message" => self.ctl_message(args),
            "prop" => self.ctl_prop(args),
            "timer" => self.ctl_timer(args),
            "macro" => self.ctl_macro(args),
            "announce" => self.ctl_announce(args),
            "bible" => self.ctl_bible(args),
            "song" => self.ctl_song(args),
            "job" => self.ctl_job(args),
            "search" => self.ctl_search(&rest(args, 1)),
            "group" => self.ctl_group(args),
            "hotkey" => self.ctl_hotkey(args),
            "continuous" => self.ctl_continuous(args),
            "video" => self.ctl_video(args),
            "test" => self.ctl_test(args),
            "bibles" => self.ctl_bibles(),
            "screen" => self.ctl_screen(args),
            "settings" => self.ctl_settings(args),
            "groups" => Ok(ctl_groups()),
            "displays" => self.ctl_displays(),
            "decklink" => self.ctl_decklink(),
            "theme" => self.ctl_theme(args),
            "version" => self.ctl_version(args),
            "undo" => self.ctl_undo(el, false),
            "redo" => self.ctl_undo(el, true),
            "menu" => self.ctl_menu(el, args),
            "front" => self.ctl_front(args),
            "remote" => self.ctl_remote(args),
            "mirror" => self.ctl_mirror(args),
            "notices" => self.ctl_notices(args),
            "update" => self.ctl_update(args),
            "reports" => crate::reports::ctl(args),
            other => Err(format!("SoftPresenter doesn't know the command \u{201c}{other}\u{201d}. See: sp help")),
        };
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
        // A change that couldn't be saved, because the song's file changed outside the app, is
        // said.
        let mut reply = result.unwrap_or_else(Reply::failed);
        // A song pack job started in the background: its number, for `job N` and `--wait`, and
        // this run's word (job numbers start again with the app).
        if let Some(job) = self.last_job.take() {
            match &mut reply.data {
                serde_json::Value::Object(map) => {
                    map.insert("job".to_owned(), serde_json::json!(job));
                    map.insert("run".to_owned(), serde_json::json!(self.run_word));
                }
                data => *data = serde_json::json!({"job": job, "run": self.run_word}),
            }
            reply.text.push_str(&format!(" (job {job}; sp job {job} says how it went)"));
        }
        for at in self.held_songs.iter().filter(|at| !held_before.contains(at)) {
            // The change is made in the app but not saved: a script must be able to tell.
            reply.ok = false;
            let title = self.show.presentations().get(*at).map_or("The song", |song| song.title.as_str());
            reply.text.push_str(&format!(
                "\nNot saved: {title}'s file was changed outside SoftPresenter. sp song reload {title} --now takes the file's version; sp song keep {title} saves this one."
            ));
        }
        reply
    }

    pub(crate) fn with_state(&mut self, change: impl FnOnce(&mut crate::home::HomeState)) {
        if let Some(running) = self.running.as_mut() {
            change(&mut running.home_state);
        }
    }

    /// The songs as `ctl songs` lists them (Bible pages and deleted songs left out): each one's
    /// place in the show, and its title.
    pub(crate) fn listed_songs(&self) -> Vec<(usize, String)> {
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        self.show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(at, _)| !self.app_made(*at) && !removed.contains(at))
            .map(|(at, song)| (at, song.title.clone()))
            .collect()
    }

    /// The song a name or number stands for, read again first if its file changed outside the
    /// app (unless it is in use; then the window offers Reload).
    pub(crate) fn find_song(&mut self, typed: &str) -> Result<usize, String> {
        let at = self.find_song_as_is(typed)?;
        self.freshen(at, false);
        Ok(at)
    }

    /// The song a name, number or id stands for, as the app has it. Its id (as `song info` and
    /// the JSON replies give it) tells apart songs that share a title.
    pub(crate) fn find_song_as_is(&self, typed: &str) -> Result<usize, String> {
        if let Ok(id) = sp_model::Uuid::parse_str(typed.trim())
            && let Some(at) = (0..self.show.presentations().len())
                .find(|at| self.show.presentations()[*at].id == id && self.is_listed(*at))
        {
            return Ok(at);
        }
        let songs = self.listed_songs();
        let names: Vec<String> = songs.iter().map(|(_, title)| title.clone()).collect();
        find("song", &names, typed).map(|at| songs[at].0)
    }

    /// `song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]`.
    fn ctl_song_export(&mut self, args: &[String]) -> Result<Reply, String> {
        const USAGE: &str = "song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]";
        self.only_at_the_computer(crate::remote::policy::AtComputer::ExportPack)?;
        let mut songs = Vec::new();
        let (mut to, mut playlist, mut videos) = (None, None, true);
        let mut words = args.iter();
        while let Some(word) = words.next() {
            match word.as_str() {
                "--to" => to = Some(words.next().ok_or(USAGE)?.clone()),
                "--playlist" => playlist = Some(words.next().ok_or(USAGE)?.clone()),
                "--no-videos" => videos = false,
                song => songs.push(song.to_owned()),
            }
        }
        let to = to.ok_or_else(|| format!("Where should the pack go? {USAGE}"))?;
        let mut to = std::path::PathBuf::from(to);
        if !crate::pack::is_pack(&to) {
            to.set_extension(crate::pack::ENDING);
        }
        if !to.parent().is_some_and(|dir| dir.as_os_str().is_empty() || dir.is_dir()) {
            return Err(format!(
                "There is no folder {}.",
                to.parent().map_or_else(String::new, |d| d.display().to_string())
            ));
        }
        let names: Vec<String> = self
            .running
            .as_ref()
            .map(|r| r.home_state.services.list.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();
        let playlist = playlist.map(|typed| find("playlist", &names, &typed)).transpose()?;
        if songs.is_empty() && playlist.is_none() {
            return Err(format!("Which songs? {USAGE}"));
        }
        let mut indices = Vec::new();
        for song in &songs {
            indices.push(self.find_song(song)?);
        }
        let mut export = self.plan_pack(&indices, playlist)?;
        if !videos {
            export.leave_out_videos();
        }
        let said = self.write_pack(export, to)?;
        Ok(Reply::done(said))
    }

    /// `job N`: how a song pack written or opened in the background went, or that it is still
    /// going.
    fn ctl_job(&self, args: &[String]) -> Result<Reply, String> {
        let job: u64 = args
            .get(1)
            .and_then(|w| w.parse().ok())
            .ok_or("job N (the number a background song pack job was given)")?;
        let kept = self.jobs.iter().find(|(have, _)| *have == job).map(|(_, how)| how);
        let mut reply = job_reply(job, kept, &self.run_word);
        // A search for songs (`songs find`) gives what it found as data too.
        if let (Some(more), serde_json::Value::Object(map)) = (self.finding.job_data(job), &mut reply.data)
            && let serde_json::Value::Object(more) = more
        {
            map.extend(more.clone());
        }
        Ok(reply)
    }

    /// `song reload [SONG [--now]]`.
    fn ctl_song_reload(&mut self, args: &[String]) -> Result<Reply, String> {
        let now = args.iter().any(|w| w == "--now");
        let typed = args.iter().filter(|w| *w != "--now").cloned().collect::<Vec<_>>().join(" ");
        if typed.trim().is_empty() || typed.trim().eq_ignore_ascii_case("all") {
            let done = self.reload_all();
            let data = serde_json::json!({"read": done.read, "added": done.added, "waiting": done.waiting, "problems": done.problems});
            return Ok(Reply::with(done.summary(), data));
        }
        let at = self.find_song_as_is(&typed)?;
        let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
        if now {
            let title = self.reload_song(at)?;
            crate::applog::event("outside", &format!("read {title} again (sp song reload --now)"));
            return Ok(Reply::done(format!("Read {title} again from its file.")));
        }
        if self.freshen(at, false) {
            return Ok(Reply::done(format!("Read {title} again from its file.")));
        }
        let waiting =
            self.running.as_ref().is_some_and(|r| r.home_state.notices.asks_about(crate::outside::Waiting::Song(at)));
        if waiting {
            return Err(format!(
                "{title} is on screen, being edited, holds changes not saved, or its file is gone, so it wasn't read again; the window offers Reload. \
                 sp song reload {title} --now takes the file's version; sp song keep {title} saves SoftPresenter's."
            ));
        }
        Ok(Reply::done(format!("{title} is as its file has it.")))
    }

    /// What is on screen, in words: "Sample song, slide 3 of 8 (Chorus)".
    pub(crate) fn live_words(&self) -> String {
        match self.show.live() {
            None => "Nothing is on screen.".to_owned(),
            Some(at) => {
                let song = self.show.presentations().get(at.presentation);
                let title = song.map_or("?", |s| s.title.as_str());
                let count = song.map_or(0, |s| s.slide_count());
                let group = song.and_then(|s| s.slide(at.slide)).map(|(g, _)| g.name.clone()).filter(|n| !n.is_empty());
                let group = group.map(|g| format!(" ({g})")).unwrap_or_default();
                format!("On screen: {title}, slide {} of {count}{group}.", at.slide + 1)
            }
        }
    }

    /// What a presentation is, in replies: a song, a Bible passage (the Bible page's), the
    /// VoxLive item, or a song of the song catalog being looked at (not in the library).
    pub(crate) fn kind_of(&self, at: usize) -> &'static str {
        if self.bible_slots.contains(&at) {
            "passage"
        } else if at == self.voxlive_slot {
            "voxlive"
        } else if self.catalog_slots.contains(&at) {
            "catalog"
        } else {
            "song"
        }
    }

    /// The playlist item what is on screen was opened from: the playlist in front and the
    /// item's number, when the item last opened there is on screen. A song Next went on into
    /// counts as its own item, further down the playlist (or else above), as the window shows
    /// it chosen. Nothing when no item was opened (a song put on screen from the library).
    fn live_item(&self) -> Option<(String, usize)> {
        use crate::service::Item;
        let live = self.show.live()?.presentation;
        let state = &self.running.as_ref()?.home_state;
        let chosen = state.services.chosen?;
        let list = state.services.front();
        let title = self.show.presentations().get(live).map(|song| song.title.as_str());
        let is_live = |item: &Item| match item {
            Item::Song { key, .. } => crate::studio::playlist_song(&self.show, state, key) == Some(live),
            Item::Passage(saved) => {
                self.bible_slots.contains(&live)
                    && title.is_some_and(|title| state.bible.holds_kept(live, title, &saved.lookup))
            }
            _ => false,
        };
        let at = if list.items.get(chosen).is_some_and(is_live) {
            Some(chosen)
        } else {
            let after = list.items.iter().skip(chosen).position(is_live).map(|at| at + chosen);
            after.or_else(|| list.items.iter().position(is_live))
        };
        at.map(|at| (list.name.clone(), at + 1))
    }

    pub(crate) fn ctl_status(&self) -> Reply {
        self.status_reply(true)
    }

    /// `status`, with its change `token` or (for Remote Control's pages, which have no use for
    /// it) without: the token writes out every playlist, library and media playlist to hash
    /// them, which a page connected would otherwise pay for each time the state is published.
    pub(crate) fn status_reply(&self, token: bool) -> Reply {
        let live = self.show.live();
        let song = live.and_then(|at| self.show.presentations().get(at.presentation));
        let words = live.and_then(|at| song?.slide(at.slide)).map(|(_, slide)| {
            slide.texts().into_iter().map(|(_, text)| text.replace('\n', " / ")).collect::<Vec<_>>().join(" / ")
        });
        let next = live.and(self.show.upcoming()).map(|at| at.slide + 1);
        let media = self.show.live_media().and_then(|at| self.show.media().get(at)).map(|item| item.name.clone());
        let state = self.running.as_ref().map(|r| &r.home_state);
        let message = state.and_then(|s| {
            let (id, _) = s.message_live?;
            s.messages.iter().find(|m| m.id == id).map(|m| m.name.clone())
        });
        let props: Vec<String> = state
            .map(|s| {
                s.props_live
                    .iter()
                    .filter_map(|(id, _)| s.props.iter().find(|p| p.id == *id))
                    .map(|p| p.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let selected = self.show.presentations().get(self.show.selected()).map(|s| s.title.clone());
        let mut lines = vec![on_screen_line(self.live_words(), self.show.words_hidden())];
        if let Some(words) = &words {
            lines.push(format!("Words: {words}"));
        }
        // What Next would put on screen, decided as the key decides it: the words of the line
        // under the window's preview.
        if let Some(next_up) = self.next_up().words() {
            lines.push(next_up);
        }
        lines.push(format!("Media: {}", media.clone().unwrap_or_else(|| "none".to_owned())));
        if let Some(message) = &message {
            lines.push(format!("Message: {message}"));
        }
        if !props.is_empty() {
            lines.push(format!("Props: {}", props.join(", ")));
        }
        lines.push(format!(
            "Audience screens: {}   Stage screens: {}",
            if self.show.output_enabled() { "on" } else { "off" },
            if state.is_some_and(|s| s.stage_on) { "on" } else { "off" }
        ));
        // Whether the system is asked to keep the display awake (while a screen is on), and how.
        let (kept_awake, kept_how) = self.keep_awake.status();
        lines.push(format!("Display kept awake: {} ({kept_how})", if kept_awake { "yes" } else { "no" }));
        if let Some(selected) = &selected {
            lines.push(format!("Song shown in the window: {selected}"));
        }
        // What else is going on: the announcements, a media playlist playing, the timers running,
        // the audio, and the window's own state.
        let announcing = self.announcing_words();
        if let Some(announcing) = &announcing {
            lines.push(format!("Announcements: {announcing}"));
        }
        let media_playlist = self.media_playlist_playing();
        if let Some(name) = &media_playlist {
            lines.push(format!("Media playlist playing: {name}"));
        }
        let timers = self.timer_states();
        let running: Vec<String> = timers
            .iter()
            .filter(|t| t["running"].as_bool() == Some(true))
            .map(|t| format!("{} {}", t["name"].as_str().unwrap_or(""), t["shows"].as_str().unwrap_or("")))
            .collect();
        if !running.is_empty() {
            lines.push(format!("Timers running: {}", running.join(", ")));
        }
        let audio = self.audio_state();
        if let Some(track) = audio["track"].as_str() {
            let how = if audio["paused"].as_bool() == Some(true) { "paused" } else { "playing" };
            let looping = if audio["loop"].as_bool() == Some(true) { ", loop on" } else { "" };
            lines.push(format!("Audio: {track} ({how}), volume {}{looping}", audio["volume"]));
        }
        let open = state.map(|s| {
            let list = s.services.front();
            let item = s.services.chosen.and_then(|at| list.items.get(at)).map(|item| item.title().to_owned());
            let number = s.services.chosen.filter(|at| *at < list.items.len()).map(|at| at + 1);
            (list.name.clone(), item, s.playlist_open, number)
        });
        if let Some((list, item, shown, _)) = &open {
            lines.push(format!(
                "Playlist in front: {list}{}{}",
                if *shown { " (listed in the left column)" } else { "" },
                item.as_ref().map_or_else(String::new, |item| format!("; item last opened from it: {item}"))
            ));
        }
        // The theme version the screens are in, said once any theme has one.
        let version = state.and_then(|s| {
            let on = s.version_switch.on?;
            let name = s.versions.iter().find(|version| version.id == on.version)?.name.clone();
            Some((name, on.keep, on.from.is_none()))
        });
        if let Some((name, kept, waiting)) = &version {
            let how = match (kept, waiting) {
                (_, true) => " (from the next item to go on screen)",
                (true, false) => " (kept until switched back)",
                (false, false) => " (until the next item)",
            };
            lines.push(format!("Theme version: {name}{how}"));
        } else if state.is_some_and(|s| !s.versions.is_empty()) {
            lines.push("Theme version: the usual look".to_owned());
        }
        let theme = self.themes.get(self.songs_theme).map(|t| t.name.clone());
        lines.push(format!(
            "Continuous: {}   Identify screens: {}   Main theme: {}",
            if state.is_some_and(|s| s.continuous) { "on" } else { "off" },
            if state.is_some_and(|s| s.identify) { "on" } else { "off" },
            theme.clone().unwrap_or_default()
        ));
        let data = serde_json::json!({
            "announcements": announcing,
            "media_playlist": media_playlist,
            "timers": timers,
            "audio": audio,
            "playlist": open.as_ref().map(|(list, item, shown, number)| serde_json::json!({"name": list, "item_last_opened": item, "item": number, "listed": shown})),
            "continuous": state.is_some_and(|s| s.continuous),
            "identify": state.is_some_and(|s| s.identify),
            "main_theme": theme,
            "version": version.as_ref().map(|(name, kept, waiting)| serde_json::json!({"name": name, "kept": kept, "waiting": waiting})),
            "live": live.map(|at| {
                // Which song exactly (titles can repeat), and the playlist item it came from.
                let number = self.song_number(at.presentation);
                let from = self.live_item();
                serde_json::json!({
                    "song": song.map(|s| s.title.clone()),
                    "slide": at.slide + 1,
                    "slides": song.map(|s| s.slide_count()),
                    "words": words,
                    "kind": self.kind_of(at.presentation),
                    "id": song.filter(|_| !self.app_made(at.presentation)).map(|s| s.id),
                    "number": number,
                    "playlist": from.as_ref().map(|(list, _)| list),
                    "item": from.as_ref().map(|(_, item)| item),
                })
            }),
            "next_slide": next,
            "media": media,
            "message": message,
            "props": props,
            "audience_on": self.show.output_enabled(),
            "stage_on": state.is_some_and(|s| s.stage_on),
            "display_kept_awake": {"kept": kept_awake, "how": kept_how},
            "selected_song": selected,
        });
        let mut data = data;
        if token {
            data["token"] = serde_json::json!(self.change_token(&data));
        }
        Reply::with(lines.join("\n"), data)
    }

    /// What `status --wait` compares: three parts, each changing when something of its kind
    /// does. What `status` says, leaving out the time each timer shows and how far the audio has
    /// played, which change every second; the playlists and their items; and the library (the
    /// songs and their slides, the libraries, the media bin and the media playlists).
    fn change_token(&self, status: &serde_json::Value) -> String {
        let said = without_clocks(status);
        let state = self.running.as_ref().map(|r| &r.home_state);
        let playlists = state.map(|s| serde_json::to_string(&s.services.list).unwrap_or_default()).unwrap_or_default();
        // The songs change only by being added or replaced, which the show counts.
        let media: Vec<String> = self
            .show
            .media()
            .iter()
            .map(|item| format!("{} {} {:?} {}", item.id, item.name, item.fit, item.looping))
            .collect();
        let library = state.map_or_else(String::new, |s| {
            format!(
                "{} {:?} {:?} {} {}",
                self.show.version(),
                s.removed_songs,
                media,
                serde_json::to_string(&s.libraries).unwrap_or_default(),
                serde_json::to_string(&s.media_playlists).unwrap_or_default()
            )
        });
        format!("{:016x}-{:016x}-{:016x}", hash_of(&said.to_string()), hash_of(&playlists), hash_of(&library))
    }

    /// Holds a `status --wait` when nothing has changed since its token (or, with no token,
    /// to wait for the next change), for `answer_waiting` to answer. Anything else is given
    /// back, to be carried out now.
    pub(crate) fn hold_wait(&mut self, incoming: Incoming) -> Option<Incoming> {
        let Some(Ok((wait, since))) = wait_for(&incoming.request.args) else { return Some(incoming) };
        let now = self.ctl_status().data["token"].as_str().unwrap_or_default().to_owned();
        let since = since.unwrap_or_else(|| now.clone());
        if since != now || wait.is_zero() {
            return Some(incoming);
        }
        let until = std::time::Instant::now() + wait;
        self.waiting.push(Waiter { reply: incoming.reply, since, until, seconds: wait.as_secs_f64() });
        None
    }

    /// Answers each `status --wait` held whose token no longer matches, or whose time is up,
    /// and says when the next one's time is up, for the event loop to wake then. Done after
    /// everything else the event loop does, so whatever changed it is seen.
    pub(crate) fn answer_waiting(&mut self) -> Option<std::time::Instant> {
        if self.waiting.is_empty() {
            return None;
        }
        let status = self.ctl_status();
        let token = status.data["token"].as_str().unwrap_or_default().to_owned();
        let now = std::time::Instant::now();
        for waiter in std::mem::take(&mut self.waiting) {
            if waiter.since != token {
                let _ = waiter.reply.send(with_changes(status.clone(), Some(&waiter.since)));
            } else if waiter.until <= now {
                let mut reply = with_changes(status.clone(), Some(&waiter.since));
                reply.text = format!("Nothing changed in {} s.", waiter.seconds);
                let _ = waiter.reply.send(reply);
            } else {
                self.waiting.push(waiter);
            }
        }
        self.waiting.iter().map(|waiter| waiter.until).min()
    }

    /// Whether a `stage` command is one of the newer ones (message, layout).
    fn ctl_stage_more_applies(&self, args: &[String]) -> bool {
        let newer = ["message", "layout", "layouts", "objects", "picture", "frame"];
        args.get(1).is_some_and(|w| newer.iter().any(|word| w.eq_ignore_ascii_case(word)))
    }

    /// What a slide or macro action does, in words.
    pub(crate) fn action_words(&self, action: &sp_model::ShowAction) -> String {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return String::new() };
        crate::macros::describe(
            action,
            |id| state.messages.iter().find(|x| x.id == id).map(|x| x.name.clone()),
            |id| state.props.iter().find(|x| x.id == id).map(|x| x.name.clone()),
            |id| state.timers.iter().find(|x| x.id == id).map(|x| x.name.clone()),
            |_| crate::macros::step_song_title(&self.show, state, action),
            |id| self.show.media().iter().find(|x| x.id == id).map(|x| x.name.clone()),
            |id| state.media_playlists.iter().find(|p| p.id == id).map(|p| p.name.clone()),
            |id| state.versions.iter().find(|v| v.id == id).map(|v| v.name.clone()),
        )
    }

    /// The media playlist playing item after item, by name.
    fn media_playlist_playing(&self) -> Option<String> {
        let id = self.media_cycle.as_ref()?.playlist;
        let state = &self.running.as_ref()?.home_state;
        state.media_playlists.iter().find(|p| p.id == id).map(|p| p.name.clone())
    }

    /// Each timer: its name, kind, whether it runs, and what it shows now.
    fn timer_states(&self) -> Vec<serde_json::Value> {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Vec::new() };
        let (now, day) = (std::time::Instant::now(), crate::timers::seconds_into_day());
        state
            .timers
            .iter()
            .map(|timer| {
                let kind = crate::make_cli::kind_words(&timer.kind);
                serde_json::json!({
                    "name": timer.name,
                    "kind": kind,
                    "running": timer.running(),
                    "shows": timer.text(now, day),
                })
            })
            .collect()
    }

    /// The track the audio player has, whether it is paused, and the volume (0 to 100).
    fn audio_state(&self) -> serde_json::Value {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return serde_json::json!({}) };
        let playing = state.audio_playing;
        // The one playing even when it has been taken out of its playlist.
        let track = playing.track.and_then(|id| state.audio.track(id)).map(|t| t.name.clone());
        serde_json::json!({
            "track": track,
            "paused": playing.paused,
            "seconds": playing.position.as_secs(),
            "length": playing.length.map(|l| l.as_secs()),
            "volume": (state.audio_volume * 100.0).round() as u32,
            "through": state.audio.through,
            "shuffle": state.audio.shuffle,
            "loop": state.audio.looping,
        })
    }

    /// `timers`: each with its kind, whether it runs, and what it shows.
    fn ctl_timers(&self) -> Reply {
        let timers = self.timer_states();
        let lines: Vec<String> = timers
            .iter()
            .map(|t| {
                format!(
                    "{}: {}, {} ({})",
                    t["name"].as_str().unwrap_or(""),
                    t["kind"].as_str().unwrap_or(""),
                    t["shows"].as_str().unwrap_or(""),
                    if t["running"].as_bool() == Some(true) { "running" } else { "stopped" }
                )
            })
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(timers))
    }

    /// `messages`: each with its words (tokens such as {timer:Sermon} as written), and the one
    /// showing.
    pub(crate) fn ctl_messages(&self) -> Reply {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Reply::done("(none)") };
        let live = state.message_live.map(|(id, _)| id);
        let lines: Vec<String> = state
            .messages
            .iter()
            .map(|m| {
                let words = if m.text.trim().is_empty() {
                    "no words".to_owned()
                } else {
                    format!("\u{201c}{}\u{201d}", m.text)
                };
                format!("{}: {words}{}", m.name, if live == Some(m.id) { " (showing)" } else { "" })
            })
            .collect();
        let data: Vec<serde_json::Value> = state
            .messages
            .iter()
            .map(|m| serde_json::json!({"name": m.name, "text": m.text, "showing": live == Some(m.id), "leaves_after": m.dismiss_after}))
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(data))
    }

    /// `props`: each, and whether it is on screen.
    pub(crate) fn ctl_props(&self) -> Reply {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Reply::done("(none)") };
        let on = |id: sp_model::Uuid| state.props_live.iter().any(|(have, _)| *have == id);
        let lines: Vec<String> =
            state.props.iter().map(|p| format!("{}{}", p.name, if on(p.id) { " (on screen)" } else { "" })).collect();
        let data: Vec<serde_json::Value> = state
            .props
            .iter()
            .map(|p| serde_json::json!({"name": p.name, "on": on(p.id), "clears_after": p.auto_clear}))
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(data))
    }

    /// `macros`: each with what it does, step by step.
    pub(crate) fn ctl_macros(&self) -> Reply {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Reply::done("(none)") };
        let steps = |m: &crate::macros::Macro| -> Vec<String> {
            m.actions.iter().map(|action| self.action_words(action)).collect()
        };
        let lines: Vec<String> = state
            .macros
            .iter()
            .map(|m| {
                let when = if m.on_startup { " (runs when SoftPresenter opens)" } else { "" };
                match steps(m) {
                    steps if steps.is_empty() => format!("{}{when}: no steps", m.name),
                    steps => format!("{}{when}: {}", m.name, steps.join("; ")),
                }
            })
            .collect();
        let data: Vec<serde_json::Value> = state
            .macros
            .iter()
            .map(|m| serde_json::json!({"name": m.name, "steps": steps(m), "runs_when_opening": m.on_startup}))
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(data))
    }

    fn ctl_songs(&self) -> Reply {
        // A song whose file changed outside the app (looked at now), or that waits on Reload or
        // Keep mine, is marked.
        let waiting = |at: usize| {
            let file_changed =
                self.sources.get(at).cloned().flatten().is_some_and(|path| {
                    self.stamps.check(&crate::load::song_file_for(&path)) != crate::outside::Now::Same
                });
            file_changed
                || self
                    .running
                    .as_ref()
                    .is_some_and(|r| r.home_state.notices.asks_about(crate::outside::Waiting::Song(at)))
        };
        let listed = self.listed_songs();
        let names: Vec<String> = listed
            .iter()
            .map(
                |(at, title)| {
                    if waiting(*at) { format!("{title} (changed outside SoftPresenter)") } else { title.clone() }
                },
            )
            .collect();
        // As data, each song has its place in this list and its stable id: what `go`, `slides` and
        // `slide` take in place of the title, and what a playlist item's `id` is.
        let data: Vec<serde_json::Value> = listed
            .iter()
            .enumerate()
            .map(|(place, (at, title))| {
                serde_json::json!({
                    "number": place + 1,
                    "title": title,
                    "id": self.show.presentations().get(*at).map(|song| song.id),
                    "changed_outside": waiting(*at),
                })
            })
            .collect();
        Reply::with(numbered(&names), serde_json::json!(data))
    }

    fn ctl_named_list(&self, _kind: &str, names: impl Fn(&crate::home::HomeState) -> Vec<String>) -> Reply {
        let names = self.running.as_ref().map(|r| names(&r.home_state)).unwrap_or_default();
        Reply::with(numbered(&names), serde_json::json!(names))
    }

    fn ctl_go(&mut self, args: &[String]) -> Result<Reply, String> {
        // The last word is the slide's number, when it is a number and there is a song before it.
        let (song_words, slide) = match args.last().and_then(|w| w.parse::<usize>().ok()) {
            Some(number) if args.len() > 2 => (rest(&args[..args.len() - 1], 1), Some(number)),
            _ => (rest(args, 1), None),
        };
        let at = self.find_song(&song_words)?;
        let song = self.show.presentations().get(at).ok_or("That song is gone.")?.clone();
        if song.slide_count() == 0 {
            return Err(format!("{} has no slides.", song.title));
        }
        let slide = match slide {
            Some(number) if (1..=song.slide_count()).contains(&number) => number - 1,
            Some(number) => return Err(no_such_slide(&song.title, song.slide_count(), &number.to_string())),
            None => song.order().into_iter().find(|s| song.slide(*s).is_some_and(|(_, sl)| sl.enabled)).unwrap_or(0),
        };
        self.apply(vec![Command::Select(at), Command::GoTo(SlideRef { presentation: at, slide })]);
        Ok(Reply::done(self.live_words()))
    }

    /// The place in the playlist in front of the item an aim names, if the playlist is as the
    /// aim last saw it.
    fn aimed_item(&self, aim: &Aim) -> Result<usize, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let list = state.services.front();
        // An item's `uid` names it exactly, wherever it has been moved to since: no `--rev` needed.
        if let Ok(uid) = sp_model::Uuid::parse_str(aim.item.trim()) {
            return list.place_of_uid(uid).ok_or_else(|| {
                crate::i18n::tf(
                    "remote.refused.item_gone",
                    "That item is no longer in {playlist}; look again and try once more.",
                    &[("playlist", &list.name)],
                )
            });
        }
        if aim.rev.as_ref().is_some_and(|rev| *rev != list.rev()) {
            return Err(crate::i18n::tf(
                "remote.refused.playlist_changed",
                "{playlist} has changed since it was last looked at; look again and try once more.",
                &[("playlist", &list.name)],
            ));
        }
        let titles: Vec<String> = list.items.iter().map(|item| item.title().to_owned()).collect();
        find("item", &titles, &aim.item)
            .map_err(|problem| format!("{problem} {} has {} items.", list.name, titles.len()))
    }

    /// `go --aim ITEM [SLIDE]`: a slide of an item of the playlist in front on screen (its first
    /// without SLIDE; a picture, video or media playlist shown, as 1), for a remote page that
    /// keeps its own open item: the window's open item, chosen song and page stay as they are.
    fn go_aimed(&mut self, item: usize, words: &[String]) -> Result<Reply, String> {
        let usage = "go --aim ITEM [SLIDE] [--rev REV]";
        let slide = match words {
            [] => None,
            [number] => Some(number.parse::<usize>().map_err(|_| usage.to_owned())?),
            _ => return Err(usage.to_owned()),
        };
        let (stops, titles) = self.playlist_stops();
        let title = titles.get(item).cloned().unwrap_or_default();
        let presentation = match (stops.get(item), slide) {
            (Some(crate::running::Stop::Song { at, .. }), Some(_)) => Some(*at),
            (Some(crate::running::Stop::Passage), Some(_)) => {
                Some(self.open_passage_now(item, false).ok_or_else(|| format!("Could not open {title}."))?.presentation)
            }
            (Some(crate::running::Stop::Media(Some(_)) | crate::running::Stop::MediaPlaylist(true)), Some(n))
                if n != 1 =>
            {
                return Err(format!("{title} has one thing to show: say 1 to show it."));
            }
            (Some(crate::running::Stop::Nothing) | None, _)
            | (Some(crate::running::Stop::Media(None) | crate::running::Stop::MediaPlaylist(false)), _) => {
                return Err(format!("{title} has nothing to show."));
            }
            _ => None,
        };
        match (presentation, slide) {
            (Some(at), Some(number)) => {
                let count = self.show.presentations().get(at).map_or(0, |song| song.slide_count());
                if !(1..=count).contains(&number) {
                    return Err(no_such_slide(&title, count, &number.to_string()));
                }
                self.apply(vec![Command::GoTo(SlideRef { presentation: at, slide: number - 1 })]);
                self.with_state(|state| state.live_item = Some(item));
            }
            _ => self.start_item_as(item, false, false),
        }
        Ok(Reply::done(self.live_words()))
    }

    /// `notices`, `notices seen`, `notices clear`, `notices answer N ANSWER`: the list under the
    /// status button, as the window has it (`notices.rs`). An answer is one the question itself
    /// offers, carried out as its button carries it out.
    fn ctl_notices(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "notices | notices seen | notices clear | notices answer N ANSWER";
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            None => {
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let entries = state.notices.to_vec();
                let lines: Vec<String> = entries
                    .iter()
                    .map(|notice| {
                        let kind = serde_json::to_value(notice.kind)
                            .ok()
                            .and_then(|k| k.as_str().map(str::to_owned))
                            .unwrap_or_default();
                        let answers: Vec<&str> = notice.actions.iter().map(|action| action.id.as_str()).collect();
                        let answers = if answers.is_empty() {
                            String::new()
                        } else {
                            format!("   (answer: {})", answers.join(", "))
                        };
                        format!(
                            "{:>3}. [{kind}] {}{}{answers}",
                            notice.id,
                            notice.text,
                            if notice.seen { "" } else { "  (new)" }
                        )
                    })
                    .collect();
                let text = if lines.is_empty() { "No notices.".to_owned() } else { lines.join("\n") };
                Ok(Reply::with(text, serde_json::to_value(&entries).unwrap_or_default()))
            }
            Some("seen") => {
                // A remote page leaves the notices about security unseen: the operator sees them.
                let page = self.outside_call();
                self.with_state(
                    |state| if page { state.notices.mark_seen_for_page() } else { state.notices.mark_seen() },
                );
                Ok(Reply::done("Every notice is marked seen."))
            }
            Some("clear") => {
                let page = self.outside_call();
                self.with_state(
                    |state| if page { state.notices.clear_all_for_page() } else { state.notices.clear_all() },
                );
                Ok(Reply::done("Cleared the notices (questions stay until they are answered)."))
            }
            Some("answer") => {
                let (Some(id), Some(answer), None) =
                    (args.get(2).and_then(|n| n.parse::<u64>().ok()), args.get(3), args.get(4))
                else {
                    return Err(usage.to_owned());
                };
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let (what, reload) = state
                    .notices
                    .answer(id, answer)
                    .ok_or_else(|| format!("Notice {id} isn't a question that takes the answer {answer}."))?;
                let mark = state.notices.mark();
                self.choose_outside(what, reload);
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let said = state.notices.said_since(mark).unwrap_or("Answered.").to_owned();
                Ok(Reply::done(said))
            }
            Some(_) => Err(usage.to_owned()),
        }
    }

    fn ctl_select(&mut self, typed: &str) -> Result<Reply, String> {
        let at = self.find_song(typed)?;
        self.apply(vec![Command::Select(at)]);
        self.with_state(|state| {
            state.bible_open = false;
            state.services.chosen = None;
        });
        let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
        Ok(Reply::done(format!("Showing {title}'s slides.")))
    }

    fn ctl_clear(&mut self, what: &str) -> Result<Reply, String> {
        match what.to_lowercase().as_str() {
            "all" => self.apply(vec![Command::ClearAll]),
            "slide" | "slides" => self.apply(vec![Command::Clear(Layer::Slides)]),
            "media" => self.apply(vec![Command::Clear(Layer::Media)]),
            "props" => self.apply(vec![Command::Clear(Layer::Props)]),
            "messages" | "message" => self.apply(vec![Command::Clear(Layer::Messages)]),
            "audio" => self.apply(vec![Command::Clear(Layer::Audio)]),
            "announcements" | "announcement" => {
                self.with_state(|state| state.announcement_request = Some(None));
                self.handle_announcement();
            }
            other => return Err(format!("Clear what? \u{201c}{other}\u{201d} isn't a layer.")),
        }
        Ok(Reply::done(format!("Cleared {what}.")))
    }

    fn ctl_view(&mut self, which: &str) -> Result<Reply, String> {
        let at = match which.to_lowercase().as_str() {
            // "show" is the Service view's old name, kept for scripts written with it.
            "service" | "show" => 0,
            "edit" => 1,
            "bible" => 2,
            "themes" => 3,
            _ => return Err("Which view: service, bible, edit or themes?".to_owned()),
        };
        self.with_state(|state| state.view_request = Some(at));
        let name = ["Service", "Edit", "Bible", "Themes"][at];
        Ok(Reply::done(format!("Switching to the {name} view.")))
    }

    /// Each screen, and what it is doing: filling a display, in a window, sending over NDI, or
    /// off and why; and a see-through screen's alpha, or whose key a key screen shows (and
    /// why it shows black, when it can't).
    pub(crate) fn ctl_screens(&self) -> Reply {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else {
            return Reply::with("", serde_json::json!([]));
        };
        let shown: Vec<bool> = state.screen_status.iter().map(|(_, shown)| *shown).collect();
        // Scripts read this: in English whatever the window speaks. What each screen is doing
        // was said in the window's language; the key's trouble is said again in English.
        let statuses: Vec<(String, bool)> = state
            .screen_status
            .iter()
            .map(|(doing, shown)| (crate::screens::status_in_english(doing), *shown))
            .collect();
        let trouble = |at: usize| {
            crate::i18n::with_language(crate::i18n::Language::English, || {
                crate::screens::key_trouble(&state.screens, at, &shown)
            })
        };
        let lines: Vec<String> = state
            .screens
            .iter()
            .enumerate()
            .map(|(at, screen)| {
                let mut line = match statuses.get(at) {
                    Some((doing, _)) => format!("{}: {doing}", screen.name),
                    None => screen.name.clone(),
                };
                if let Some((format, _)) = crate::screens::decklink_format(screen, &state.decklink) {
                    line.push_str(&format!("; format {format}"));
                }
                if let Some(background) = crate::screens::background_words(&state.screens, at) {
                    line.push_str(&format!("; {background}"));
                }
                if matches!(screen.output, crate::screens::Output::Ndi(_)) && !screen.sound {
                    line.push_str("; no sound");
                }
                if matches!(screen.output, crate::screens::Output::Ndi(_)) && screen.rate != Default::default() {
                    line.push_str(&format!("; {} pictures a second", screen.rate.name()));
                }
                if let Some(trouble) = trouble(at) {
                    line.push_str(&format!(" ({trouble})"));
                }
                line
            })
            .collect();
        let data: Vec<serde_json::Value> = state
            .screens
            .iter()
            .enumerate()
            .map(|(at, screen)| {
                let (doing, shown_now) = statuses.get(at).cloned().unwrap_or_default();
                let audience = screen.kind == crate::screens::Kind::Audience;
                let key_of = screen.look.key_of.filter(|_| audience).and_then(|fill| {
                    state.screens.iter().find(|other| other.id == fill).map(|other| other.name.clone())
                });
                let see_through = screen.look.see_through.filter(|_| audience && screen.look.key_of.is_none());
                let format = crate::screens::decklink_format(screen, &state.decklink);
                serde_json::json!({
                    "name": screen.name,
                    "kind": if audience { "audience" } else { "stage" },
                    "doing": doing,
                    "shown": shown_now,
                    // The screen's own switch: off, it keeps where it goes.
                    "on": screen.on,
                    "format": match &screen.output {
                        crate::screens::Output::DeckLink(out) => Some(out.mode.clone()),
                        _ => None,
                    },
                    "format_from": format.as_ref().map(|(_, from)| *from),
                    "format_words": format.map(|(words, _)| words),
                    "background": if see_through.is_some() { "see-through" } else { "solid" },
                    "alpha": see_through.map(crate::screens::Alpha::name),
                    "key_of": key_of,
                    "sound": matches!(screen.output, crate::screens::Output::Ndi(_)).then_some(screen.sound),
                    "rate": matches!(screen.output, crate::screens::Output::Ndi(_)).then(|| screen.rate.name()),
                    "key_trouble": trouble(at),
                })
            })
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(data))
    }

    pub(crate) fn ctl_libraries(&self) -> Reply {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Reply::done("(none)") };
        // Each library's songs, and how many of them can't be found.
        let finder = crate::studio::SongFinder::new(&self.show, state);
        let counts: Vec<(usize, usize)> = state
            .libraries
            .iter()
            .map(|l| {
                let missing = l.songs.iter().filter(|key| finder.find(key).is_none()).count();
                (l.songs.len(), missing)
            })
            .collect();
        let lines: Vec<String> = state
            .libraries
            .iter()
            .zip(&counts)
            .map(|(l, (songs, missing))| {
                let plural = if *songs == 1 { "" } else { "s" };
                match missing {
                    0 => format!("{} ({songs} song{plural})", l.name),
                    _ => format!("{} ({songs} song{plural}, {missing} missing)", l.name),
                }
            })
            .collect();
        let data: Vec<serde_json::Value> = state
            .libraries
            .iter()
            .zip(&counts)
            .map(|(l, (songs, missing))| serde_json::json!({"name": l.name, "songs": songs, "missing": missing}))
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(data))
    }

    fn ctl_library(&mut self, args: &[String]) -> Result<Reply, String> {
        if args.get(1).is_some_and(|word| word.eq_ignore_ascii_case("tidy")) {
            return self.ctl_library_tidy(args.get(2..).unwrap_or_default());
        }
        if let Some(result) = self.ctl_library_change(args) {
            return result;
        }
        let names: Vec<String> = self
            .running
            .as_ref()
            .map(|r| r.home_state.libraries.iter().map(|l| l.name.clone()).collect())
            .unwrap_or_default();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("open") if rest(args, 2).eq_ignore_ascii_case("all") => {
                self.with_state(|state| {
                    state.library_shown = None;
                    state.playlist_open = false;
                });
                Ok(Reply::done("Showing all songs."))
            }
            Some("open") => {
                let at = find("library", &names, &rest(args, 2))?;
                self.with_state(|state| {
                    state.library_shown = state.libraries.get(at).map(|l| l.id);
                    state.playlist_open = false;
                });
                Ok(Reply::done(format!("Showing the library {}.", names[at])))
            }
            Some(_) => {
                // `library NAME`: its songs, one that can't be found marked as missing.
                let at = find("library", &names, &rest(args, 1))?;
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let library = state.libraries.get(at).ok_or("No such library.")?;
                // As data, each song has its place in the library, its title and its stable id
                // (what `go`, `slides` and `slide` take), or none when it can't be found.
                let finder = crate::studio::SongFinder::new(&self.show, state);
                let found: Vec<(String, Option<sp_model::Uuid>)> = library
                    .songs
                    .iter()
                    .map(|key| match finder.find(key) {
                        Some(index) => {
                            let song = &self.show.presentations()[index];
                            (song.title.clone(), Some(song.id))
                        }
                        None => (format!("{} (missing)", key.strip_prefix("title:").unwrap_or("a song")), None),
                    })
                    .collect();
                let songs: Vec<String> = found.iter().map(|(title, _)| title.clone()).collect();
                let data: Vec<serde_json::Value> = found
                    .iter()
                    .enumerate()
                    .map(|(place, (title, id))| {
                        serde_json::json!({"number": place + 1, "title": title, "id": id, "missing": id.is_none()})
                    })
                    .collect();
                Ok(Reply::with(format!("{}\n{}", library.name, numbered(&songs)), serde_json::json!(data)))
            }
            None => Err("library NAME, library open NAME, or library open all".to_owned()),
        }
    }

    fn ctl_playlists(&self) -> Reply {
        let state = self.running.as_ref().map(|r| &r.home_state);
        let names: Vec<String> =
            state.map(|s| s.services.list.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
        Reply::with(numbered(&names), serde_json::json!(names))
    }

    fn ctl_playlist(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_playlist_change(args) {
            return result;
        }
        let names: Vec<String> = self
            .running
            .as_ref()
            .map(|r| r.home_state.services.list.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("open") => {
                let at = find("playlist", &names, &rest(args, 2))?;
                self.with_state(|state| {
                    state.services.show(at);
                    state.playlist_open = true;
                });
                Ok(Reply::done(format!("Showing the playlist {}.", names[at])))
            }
            Some("add") => {
                // `playlist add SONG [to PLAYLIST]`, or the playlist in front.
                let words = args.get(2..).unwrap_or_default();
                let (song_words, list_words) = match words.iter().rposition(|w| w.eq_ignore_ascii_case("to")) {
                    Some(at) => (words[..at].join(" "), Some(words[at + 1..].join(" "))),
                    None => (words.join(" "), None),
                };
                // "VoxLive" names the VoxLive item, which isn't among the songs, once it has screens.
                let voxlive = song_words.trim().eq_ignore_ascii_case("voxlive")
                    && self.show.presentations().get(self.voxlive_slot).is_some_and(|item| item.slide_count() > 0);
                let song = match (voxlive, self.find_song(&song_words)) {
                    (true, _) => self.voxlive_slot,
                    (false, Ok(song)) => song,
                    // Not a song: VoxLive's item, before VoxLive has screens, says how to get them.
                    (false, Err(_)) if song_words.trim().eq_ignore_ascii_case("voxlive") => {
                        return Err(
                            "VoxLive isn't connected yet, or has no screens: echo \"$KEY\" | sp voxlive connect"
                                .to_owned(),
                        );
                    }
                    (false, Err(problem)) => return Err(problem),
                };
                let list = list_words.map(|typed| find("playlist", &names, &typed)).transpose()?;
                let running = self.running.as_mut().ok_or("The app isn't ready.")?;
                let state = &mut running.home_state;
                let key = state.song_keys.get(song).cloned().ok_or("That song can't go in a playlist.")?;
                let title = self.show.presentations().get(song).map(|s| s.title.clone()).unwrap_or_default();
                let item = crate::service::Item::song(key, title.clone());
                let at = list.unwrap_or_else(|| {
                    state.services.list.iter().position(|p| p.name == state.services.front().name).unwrap_or(0)
                });
                let into = names.get(at).cloned().unwrap_or_default();
                if state.services.add_to(at, item) {
                    Ok(Reply::done(format!("Added {title} to {into}.")))
                } else {
                    Ok(Reply::done(format!("{title} is already in {into}.")))
                }
            }
            // `playlist [NAME]`: the items of a playlist, or of the one in front.
            _ => {
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let at = match rest(args, 1) {
                    typed if typed.trim().is_empty() => {
                        state.services.list.iter().position(|p| p.name == state.services.front().name).unwrap_or(0)
                    }
                    typed => find("playlist", &names, &typed)?,
                };
                let name = state.services.list.get(at).ok_or("No such playlist.")?.name.clone();
                // Each item says what it is; a song, picture, video or media playlist that can't
                // be found is kept, and shown as missing. A remote page has each item's text in
                // the window's language, as the playlist part gives it.
                let (lines, data) = self.playlist_listing(at, self.remote_call);
                Ok(Reply::with(format!("{name}\n{}", numbered(&lines)), serde_json::json!(data)))
            }
        }
    }

    /// `canva [status]`, `canva sync`: Canva Sync. A sync takes minutes, so `canva sync` answers
    /// at once with a job (`sp job N`, or `--wait`). No reply has a secret or token in it.
    fn ctl_canva(&mut self, args: &[String]) -> Result<Reply, String> {
        use crate::canva::Status;
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            None | Some("status") => {}
            Some("sync") => {
                self.canva.sync_now()?;
                let job = self.start_job();
                self.canva.job = Some(job);
                return Ok(Reply::done("Syncing from Canva: the export takes a few minutes."));
            }
            // The client ID and secret come on the standard input, never as arguments.
            Some("connect") => {
                let given = args.get(2).cloned().unwrap_or_default();
                let mut words = given.split_whitespace();
                let (Some(id), Some(secret), None) = (words.next(), words.next(), words.next()) else {
                    return Err("Give the integration's client ID and secret on the standard input, on one line: \
                                echo \"$CLIENT_ID $CLIENT_SECRET\" | sp canva connect"
                        .to_owned());
                };
                self.canva.sign_in(id, secret)?;
                return Ok(Reply::done(
                    "Canva's sign-in is open in the browser: agree there, then sp canva says how it went.",
                ));
            }
            Some("import") => {
                if !self.canva.old_here {
                    return Err("The old sync script's sign-in isn't on this computer.".to_owned());
                }
                self.canva.import_old()?;
                return Ok(Reply::done(
                    "Bringing in the old sync script's sign-in; then switch the old script off (sp canva says how).",
                ));
            }
            Some("presentation") if args.len() > 2 => {
                self.canva.set_design(&rest(args, 2))?;
                return Ok(Reply::done("Reading the presentation's pages; sp canva pages lists them."));
            }
            Some("pages") => {
                let divider = self.canva.settings.divider.clone();
                let lines: Vec<String> = self
                    .canva
                    .pages
                    .iter()
                    .map(|page| {
                        let mark = if page.id.is_some() && page.id == divider { " (divider)" } else { "" };
                        format!("page {}{mark}", page.number)
                    })
                    .collect();
                if lines.is_empty() {
                    return Err("No pages read yet: sp canva presentation LINK.".to_owned());
                }
                return Ok(Reply::with(lines.join("\n"), serde_json::json!(lines)));
            }
            Some("divider") => {
                let number: u32 = args.get(2).and_then(|n| n.parse().ok()).ok_or("canva divider N (a page number)")?;
                let id = self
                    .canva
                    .pages
                    .iter()
                    .find(|page| page.number == number)
                    .and_then(|page| page.id.clone())
                    .ok_or_else(|| format!("There is no page {number} read: sp canva pages lists them."))?;
                self.with_state(|state| state.canva.request = Some(crate::canva::Request::Divider(id)));
                self.tick_canva();
                return Ok(Reply::done(format!(
                    "Page {number} is the divider: pages 1 to {} go in the video.",
                    number.saturating_sub(1)
                )));
            }
            Some("disconnect") => {
                if !args.iter().any(|w| w == "--yes") {
                    return Err("This forgets Canva Sync's sign-in on this computer. Add --yes to do it.".to_owned());
                }
                self.canva.disconnect()?;
                return Ok(Reply::done("Forgot Canva Sync's sign-in on this computer."));
            }
            _ => {
                return Err("canva | canva sync [--wait] | canva connect (ID and secret on the standard input) | \
                            canva import | canva presentation LINK | canva pages | canva divider N | canva \
                            disconnect --yes"
                    .to_owned());
            }
        }
        let canva = &self.canva;
        let settings = &canva.settings;
        let mut lines = vec![match &canva.status {
            Status::Reading => "Canva Sync: reading its sign-in…".to_owned(),
            Status::NotConnected => "Canva Sync: not signed in (Settings › Integrations › Canva Sync).".to_owned(),
            Status::Connected => {
                format!("Canva Sync: signed in (integration {}).", canva.client_hint().unwrap_or_default())
            }
            Status::Doubtful(why) | Status::Refused(why) => format!("Canva Sync: {why}"),
        }];
        lines.push(match (&settings.design, settings.divider_number) {
            (Some(design), Some(number)) if settings.divider.is_some() => {
                format!(
                    "Presentation {design}; divider page {number}, so pages 1 to {} are exported.",
                    number.saturating_sub(1)
                )
            }
            (Some(design), _) => format!("Presentation {design}; no divider page chosen yet."),
            (None, _) => "No presentation chosen yet.".to_owned(),
        });
        lines.push(format!("The video goes in Media › {} in the library.", crate::canva::FOLDER));
        if canva.phase.busy() {
            lines.push(format!("Now: {}", canva.phase.words()));
        }
        if let (Some(at), Some(file)) = (&settings.updated_at, &settings.updated_file) {
            lines.push(format!("Last video: {file} ({at})."));
        }
        let problem = canva.said.as_ref().filter(|(_, good, _)| !good).map(|(words, _, _)| words.clone());
        if let Some(problem) = &problem {
            lines.push(format!("Last problem: {problem}"));
        }
        if canva.old_here && canva.status != Status::Connected {
            lines.push("The old sync script's sign-in is on this computer: sp canva import takes it over.".to_owned());
        }
        // The steps name places in the user's home folder: said at the computer only.
        if settings.old_script_warning && !self.outside_call() {
            lines.push(
                "Switch the old sync script off now (whichever refreshes the sign-in next breaks the other's):"
                    .to_owned(),
            );
            lines.extend(
                crate::canva::switch_off_steps(&canva.places, &canva.old_jobs)
                    .into_iter()
                    .map(|step| format!("  {step}")),
            );
        }
        let data = serde_json::json!({
            "signed_in": canva.status == Status::Connected,
            "presentation": settings.design,
            "divider_page": settings.divider_number,
            "busy": canva.phase.busy(),
            "now": canva.phase.words(),
            "last_video": settings.updated_file,
            "updated_at": settings.updated_at,
            "problem": problem,
        });
        Ok(Reply::with(lines.join("\n"), data))
    }

    /// `voxlive [screens | connect KEY | refresh | disconnect --yes]`: the connection to the
    /// church's VoxLive account. The key comes from `sp voxlive connect`'s standard input, and
    /// no reply ever has more of it than its first characters, VoxLive's own handle for it.
    fn ctl_voxlive(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "voxlive | voxlive screens | voxlive connect (key on the standard input) | voxlive refresh | voxlive disconnect --yes";
        match args.get(1).map(|word| word.to_lowercase()).as_deref() {
            None => {
                let connected = self.voxlive.status == crate::voxlive::Status::Connected;
                // A remote page is told only the kind of key, never its characters.
                let hint = self.voxlive.key_hint().map(|hint| {
                    if self.outside_call() { format!("{}…", hint.chars().take(4).collect::<String>()) } else { hint }
                });
                let count = self.voxlive.screens.len();
                // A refusal or a problem is a sentence that names VoxLive already.
                let mut text = match &self.voxlive.status {
                    crate::voxlive::Status::Refused(why) | crate::voxlive::Status::Trouble(why) => {
                        why.trim_end_matches('.').to_owned()
                    }
                    status => format!("VoxLive: {}", status.words()),
                };
                if let Some(hint) = &hint {
                    text.push_str(&format!(" (key {hint})"));
                }
                if connected {
                    text.push_str(&format!(", {count} screen{}", if count == 1 { "" } else { "s" }));
                }
                // How the last look saved went, once one has been.
                if let Some(said) = &self.voxlive.said {
                    text.push_str(&format!(". {}", said.words.trim_end_matches('.')));
                }
                Ok(Reply::with(
                    format!("{text}."),
                    serde_json::json!({"connected": connected, "status": self.voxlive.status.words(), "key": hint, "screens": count}),
                ))
            }
            Some("screens") => {
                if !self.voxlive.has_key() {
                    return Err("VoxLive isn't connected: echo \"$KEY\" | sp voxlive connect".to_owned());
                }
                let lines: Vec<String> = self
                    .voxlive
                    .screens
                    .iter()
                    .map(|screen| {
                        // A screen whose page isn't at VoxLive's own display can't be shown.
                        let page = if screen.url.is_empty() { ", no page to show" } else { "" };
                        format!("{} ({}{page})", screen.label(), screen.kind())
                    })
                    .collect();
                let data: Vec<serde_json::Value> = self
                    .voxlive
                    .screens
                    .iter()
                    .map(|screen| serde_json::json!({"id": screen.id, "name": screen.name, "language": screen.language, "mode": screen.mode, "can_show": !screen.url.is_empty()}))
                    .collect();
                let text = if lines.is_empty() { "(none found yet)".to_owned() } else { numbered(&lines) };
                Ok(Reply::with(text, serde_json::json!(data)))
            }
            Some("connect") => {
                let key = args.get(2).map(String::as_str).unwrap_or("");
                self.voxlive.connect(key)?;
                crate::applog::event("voxlive", "a key was given; checking it");
                Ok(Reply::done("Checking the key with VoxLive; sp voxlive says how it went."))
            }
            // `voxlive show N|NAME`: that screen's captions on the screens, in place of what is
            // there, as a click in the VoxLive tab does.
            Some("show") => {
                // By its number or name among all the screens `voxlive screens` lists; a screen
                // with no page SoftPresenter can show says so.
                let screens = self.voxlive.screens.clone();
                if screens.is_empty() {
                    return Err("VoxLive has no screens yet (sp voxlive screens).".to_owned());
                }
                let names: Vec<String> = screens.iter().map(crate::voxlive::Screen::label).collect();
                let at = find("VoxLive screen", &names, &rest(args, 2))?;
                if screens[at].url.is_empty() {
                    return Err(format!(
                        "{} has no page SoftPresenter can show: its address isn't at VoxLive's own display.",
                        names[at]
                    ));
                }
                // Its slide: the screens before it that have a page.
                let slide = screens[..at].iter().filter(|screen| !screen.url.is_empty()).count();
                self.apply(vec![Command::GoTo(SlideRef { presentation: self.voxlive_slot, slide })]);
                Ok(Reply::done(format!("On screen: VoxLive, {}.", names[at])))
            }
            // `voxlive look N|NAME [setting=value …]`: a screen's look, or settings of it saved
            // to VoxLive (checked against VoxLive's own description of each setting first).
            Some("look") => self.voxlive_look(args.get(2..).unwrap_or_default()),
            Some("refresh") => {
                if !self.voxlive.has_key() {
                    return Err("VoxLive isn't connected.".to_owned());
                }
                self.voxlive.refresh();
                Ok(Reply::done("Finding the VoxLive screens again."))
            }
            Some("disconnect") => {
                if !args.iter().any(|word| word == "--yes") {
                    return Err("This forgets the VoxLive key on this computer. Add --yes to do it.".to_owned());
                }
                self.voxlive.disconnect();
                crate::applog::event("voxlive", "the key was forgotten on this computer");
                Ok(Reply::done("Forgot the VoxLive key on this computer (it stays in VoxLive until revoked there)."))
            }
            _ => Err(usage.to_owned()),
        }
    }

    fn voxlive_look(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "voxlive look N|NAME [setting=value …]   (sp voxlive look N lists the settings)";
        let split = words.iter().position(|word| word.contains('=')).unwrap_or(words.len());
        let (name, settings) = words.split_at(split);
        if name.is_empty() {
            return Err(usage.to_owned());
        }
        let screens = self.voxlive.screens.clone();
        let names: Vec<String> = screens.iter().map(crate::voxlive::Screen::label).collect();
        let screen = &screens[find("VoxLive screen", &names, &name.join(" "))?];
        let style = screen.style.as_object().cloned().unwrap_or_default();
        if settings.is_empty() {
            // Every setting VoxLive describes, with the screen's value (or its default).
            let schema = &self.voxlive.schema;
            let lines: Vec<String> = if schema.is_empty() {
                style.iter().map(|(key, value)| format!("{key} = {value}")).collect()
            } else {
                schema
                    .iter()
                    .map(|(key, field)| {
                        let value = style.get(key).or_else(|| field.get("default")).cloned().unwrap_or_default();
                        let mut line = format!("{key} = {value}");
                        if let Some(values) = field.get("values").and_then(serde_json::Value::as_array) {
                            let choices: Vec<String> = values
                                .iter()
                                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                                .collect();
                            line.push_str(&format!("   ({})", choices.join(", ")));
                        } else if let (Some(min), Some(max)) = (field.get("min"), field.get("max")) {
                            line.push_str(&format!("   ({min} to {max})"));
                        }
                        line
                    })
                    .collect()
            };
            return Ok(Reply::with(format!("{}\n{}", screen.label(), lines.join("\n")), serde_json::json!(style)));
        }
        let mut changes = serde_json::Map::new();
        for setting in settings {
            let (key, typed) = setting.split_once('=').ok_or(usage)?;
            let field = self.voxlive.schema.iter().find(|(name, _)| name == key).map(|(_, field)| field.clone());
            let Some(field) = field else {
                let known: Vec<&str> = self.voxlive.schema.iter().map(|(name, _)| name.as_str()).collect();
                return Err(format!("VoxLive has no setting {key}: {}.", known.join(", ")));
            };
            changes.insert(key.to_owned(), setting_value(&field, key, typed)?);
        }
        let version = (!screen.version.is_empty()).then(|| screen.version.clone());
        self.voxlive.save_look(&screen.id, version, changes)?;
        Ok(Reply::done(format!("Saving the look of {} to VoxLive; sp voxlive says how it went.", screen.label())))
    }

    fn media_names(&self) -> Vec<String> {
        self.show.media().iter().map(|item| item.name.clone()).collect()
    }

    fn ctl_media(&mut self, args: &[String]) -> Result<Reply, String> {
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("next") => return self.media_step(true),
            Some("previous" | "prev" | "back") => return self.media_step(false),
            Some("set") => return self.media_set(args.get(2..).unwrap_or_default()),
            _ => {}
        }
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("show") => {
                let at = find("media item", &self.media_names(), &rest(args, 2))?;
                self.apply(vec![Command::ShowMedia(at)]);
                Ok(Reply::done(format!("Showing {}.", self.show.media()[at].name)))
            }
            Some("add") => {
                self.only_at_the_computer(crate::remote::policy::AtComputer::AddMedia)?;
                let files: Vec<PathBuf> = args.get(2..).unwrap_or_default().iter().map(PathBuf::from).collect();
                if files.is_empty() {
                    return Err("Add which files?".to_owned());
                }
                for file in &files {
                    if !file.is_file() {
                        return Err(format!("There is no file {}.", file.display()));
                    }
                }
                // Pictures and videos only: songs are imported with song import. Each file not
                // added says why.
                let mut added = Vec::new();
                let mut problems = Vec::new();
                for file in &files {
                    let name = file
                        .file_name()
                        .map_or_else(|| file.display().to_string(), |n| n.to_string_lossy().into_owned());
                    if !crate::media::is_media(file) {
                        problems.push(if crate::audio::is_audio(file) {
                            format!("{name} is a sound: sounds go in an audio playlist (sp audio add {name}).")
                        } else {
                            format!("{name} isn't a picture or video (songs come in with song import).")
                        });
                        continue;
                    }
                    match self.add_media_file(file) {
                        Ok(true) => added.push(name),
                        Ok(false) => problems.push(format!("{name} is already in the media bin.")),
                        Err(problem) if problem.starts_with(&format!("{name}: ")) => problems.push(problem),
                        Err(problem) => problems.push(format!("{name}: {problem}")),
                    }
                }
                let data = serde_json::json!({"added": added, "problems": problems});
                if added.is_empty() {
                    return Err(problems.join("\n"));
                }
                let mut text = match added.as_slice() {
                    [one] => format!("Added {one} to the media bin."),
                    many => format!("Added {} to the media bin: {}.", many.len(), many.join(", ")),
                };
                for problem in &problems {
                    text.push_str(&format!("\n{problem}"));
                }
                Ok(Reply::with(text, data))
            }
            Some("samples") => {
                self.only_at_the_computer(crate::remote::policy::AtComputer::Samples)?;
                let added = self.add_sample_backgrounds()?;
                let names: Vec<&str> = added.items.iter().map(|item| item.name.as_str()).collect();
                let playlist = self
                    .running
                    .as_ref()
                    .and_then(|r| r.home_state.media_playlists.iter().find(|p| p.id == added.playlist))
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                let skipped = crate::samples::needing_ffmpeg(self.show.media());
                let mut text = format!(
                    "Added {} sample background{} to the media bin, in the playlist {playlist}.",
                    names.len(),
                    if names.len() == 1 { "" } else { "s" }
                );
                if !skipped.is_empty() {
                    text.push_str(&format!(
                        "\nNot added: {}. It is a video, and FFmpeg isn't on this computer. {}",
                        skipped.join(", "),
                        crate::media::ffmpeg_advice()
                    ));
                }
                Ok(Reply::with(text, serde_json::json!({"added": names, "playlist": playlist, "skipped": skipped})))
            }
            Some("delete") => {
                let yes = args.iter().any(|w| w == "--yes");
                let words: Vec<String> =
                    args.get(2..).unwrap_or_default().iter().filter(|w| *w != "--yes").cloned().collect();
                let at = find("media item", &self.media_names(), &words.join(" "))?;
                let name = self.show.media()[at].name.clone();
                if !yes {
                    return Err(format!("Deleting {name} from the media bin can't be undone. Add --yes to do it."));
                }
                self.with_state(|state| state.media_request = Some(crate::home::MediaRequest::Delete(vec![at])));
                self.handle_media_request();
                Ok(Reply::done(format!("Deleted {name} from the media bin (the file stays where it is).")))
            }
            _ => {
                let names = self.media_names();
                Ok(Reply::with(numbered(&names), serde_json::json!(names)))
            }
        }
    }

    fn ctl_media_playlist(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_media_playlist_make(args) {
            return result;
        }
        let lists: Vec<(sp_model::Uuid, String)> = self
            .running
            .as_ref()
            .map(|r| r.home_state.media_playlists.iter().map(|p| (p.id, p.name.clone())).collect())
            .unwrap_or_default();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("play") => {
                let names: Vec<String> = lists.iter().map(|(_, name)| name.clone()).collect();
                let at = find("media playlist", &names, &rest(args, 2))?;
                let has_items = self.running.as_ref().is_some_and(|r| {
                    r.home_state
                        .media_playlists
                        .iter()
                        .find(|p| p.id == lists[at].0)
                        .is_some_and(|p| self.show.media().iter().any(|item| p.has(&item.path)))
                });
                if !has_items {
                    return Err(format!("The media playlist {} has nothing in it to play.", names[at]));
                }
                self.play_media_playlist(lists[at].0);
                Ok(Reply::done(format!("Playing the media playlist {}.", names[at])))
            }
            Some("stop") => {
                self.stop_media_playlist();
                Ok(Reply::done("Stopped the media playlist."))
            }
            _ => {
                let names: Vec<String> = lists.into_iter().map(|(_, name)| name).collect();
                Ok(Reply::with(numbered(&names), serde_json::json!(names)))
            }
        }
    }

    fn ctl_message(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_message_make(args) {
            return result;
        }
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.messages.iter().map(|m| m.name.clone()).collect();
        let ids: Vec<sp_model::Uuid> = state.messages.iter().map(|m| m.id).collect();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("show") => {
                let at = find("message", &names, &rest(args, 2))?;
                self.run_action(sp_model::ShowAction::Message { id: ids[at], show: true });
                Ok(Reply::done(format!("Showing the message {}.", names[at])))
            }
            Some("hide") => {
                self.apply(vec![Command::Clear(Layer::Messages)]);
                Ok(Reply::done("The message is off the screens."))
            }
            _ => Err("message show NAME, or message hide".to_owned()),
        }
    }

    fn ctl_prop(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_prop_make(args) {
            return result;
        }
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.props.iter().map(|p| p.name.clone()).collect();
        let ids: Vec<sp_model::Uuid> = state.props.iter().map(|p| p.id).collect();
        let show = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("show") => true,
            Some("hide") => false,
            _ => return Err("prop show NAME, or prop hide NAME".to_owned()),
        };
        let at = find("prop", &names, &rest(args, 2))?;
        self.run_action(sp_model::ShowAction::Prop { id: ids[at], show });
        Ok(Reply::done(format!("{} the prop {}.", if show { "Showing" } else { "Hid" }, names[at])))
    }

    fn ctl_timer(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_timer_make(args) {
            return result;
        }
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.timers.iter().map(|t| t.name.clone()).collect();
        let ids: Vec<sp_model::Uuid> = state.timers.iter().map(|t| t.id).collect();
        let act = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("start") => sp_model::TimerDo::Start,
            Some("stop") => sp_model::TimerDo::Stop,
            Some("reset") => sp_model::TimerDo::Reset,
            _ => return Err("timer start|stop|reset NAME".to_owned()),
        };
        let at = find("timer", &names, &rest(args, 2))?;
        self.run_action(sp_model::ShowAction::Timer { id: ids[at], act });
        let done = match act {
            sp_model::TimerDo::Start => "Started",
            sp_model::TimerDo::Stop => "Stopped",
            sp_model::TimerDo::Reset => "Reset",
        };
        Ok(Reply::done(format!("{done} the timer {}.", names[at])))
    }

    fn ctl_macro(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_macro_make(args) {
            return result;
        }
        if args.get(1).map(|w| w.to_lowercase()).as_deref() != Some("run") {
            return Err("macro run NAME".to_owned());
        }
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.macros.iter().map(|m| m.name.clone()).collect();
        let at = find("macro", &names, &rest(args, 2))?;
        let actions = state.macros[at].actions.clone();
        // A "go to song" step whose song can't be found is said, not passed over in silence.
        let lost = actions
            .iter()
            .filter(|action| match action {
                sp_model::ShowAction::Song { .. }
                | sp_model::ShowAction::Announce { what: sp_model::Announced::Song { .. } } => {
                    crate::macros::step_song(&self.show, state, action).is_none()
                }
                _ => false,
            })
            .count();
        self.with_state(|state| state.actions_to_run.extend(actions));
        self.run_actions();
        let mut text = format!("Ran the macro {}. {}", names[at], self.live_words());
        if lost > 0 {
            text.push_str(match lost {
                1 => "\nIts \"go to song\" step found no song.",
                _ => "\nSome of its \"go to song\" steps found no song.",
            });
        }
        Ok(Reply::done(text))
    }

    /// Does one slide action now, as a slide or a macro does it.
    pub(crate) fn run_action(&mut self, action: sp_model::ShowAction) {
        self.with_state(|state| state.actions_to_run.push(action));
        self.run_actions();
    }

    /// `announce SONG | announce media ITEM | announce media-playlist NAME | announce stop`:
    /// something going round on the Announcements layer, or nothing.
    fn ctl_announce(&mut self, args: &[String]) -> Result<Reply, String> {
        use crate::announcements::Announce;
        let typed = rest(args, 1);
        if typed.eq_ignore_ascii_case("stop") || typed.trim().is_empty() {
            self.with_state(|state| state.announcement_request = Some(None));
            self.handle_announcement();
            return Ok(Reply::done("The Announcements layer is clear."));
        }
        let (what, name) = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("media") if args.len() > 2 => {
                let at = find("media item", &self.media_names(), &rest(args, 2))?;
                let item = &self.show.media()[at];
                (Announce::Media(item.id), item.name.clone())
            }
            Some("media-playlist") if args.len() > 2 => {
                let lists: Vec<(sp_model::Uuid, String)> = self
                    .running
                    .as_ref()
                    .map(|r| r.home_state.media_playlists.iter().map(|p| (p.id, p.name.clone())).collect())
                    .unwrap_or_default();
                let names: Vec<String> = lists.iter().map(|(_, name)| name.clone()).collect();
                let at = find("media playlist", &names, &rest(args, 2))?;
                if self.playlist_media(lists[at].0).is_empty() {
                    return Err(format!("The media playlist {} has nothing in it to play.", names[at]));
                }
                (Announce::MediaPlaylist(lists[at].0), format!("the media playlist {}", names[at]))
            }
            _ => {
                let at = self.find_song(&typed)?;
                let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
                (Announce::Song(at), title)
            }
        };
        let mark = self.running.as_ref().map_or(0, |r| r.home_state.notices.mark());
        self.with_state(|state| state.announcement_request = Some(Some(what)));
        self.handle_announcement();
        if self.announcing.as_ref().is_none_or(|on| on.what != what) {
            // What the window said why, if it did.
            let why = self.running.as_ref().and_then(|r| r.home_state.notices.said_since(mark).map(str::to_owned));
            return Err(why.unwrap_or_else(|| format!("{name} could not be played on the Announcements layer.")));
        }
        Ok(Reply::done(format!("Playing {name} on the Announcements layer.")))
    }

    fn ctl_audio(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_audio_make(args) {
            return result;
        }
        use crate::home::AudioRequest;
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let tracks: Vec<(sp_model::Uuid, String)> = state
            .audio
            .playlists
            .iter()
            .flat_map(|playlist| playlist.tracks.iter().map(|track| (track.id, track.name.clone())))
            .collect();
        let (request, done) = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("play") => {
                let names: Vec<String> = tracks.iter().map(|(_, name)| name.clone()).collect();
                let at = find("track", &names, &rest(args, 2))?;
                (AudioRequest::Play(tracks[at].0), format!("Playing {}.", names[at]))
            }
            Some("pause") => {
                (AudioRequest::TogglePause, "Paused the audio, or played it on if it was paused.".to_owned())
            }
            Some("stop") => (AudioRequest::Stop, "Stopped the audio.".to_owned()),
            Some("seek") => {
                let playing = state.audio_playing;
                let id = playing.track.ok_or("No track is playing or paused.")?;
                let typed = rest(args, 2);
                if typed.trim().is_empty() {
                    return Err("audio seek TIME   (1:23, 83 for seconds, +15 or -15)".to_owned());
                }
                let to = crate::audio::seek_to(&typed, playing.position, playing.length)?;
                let name = state.audio.track(id).map_or_else(String::new, |track| track.name.clone());
                let of =
                    playing.length.map_or_else(String::new, |length| format!(" of {}", crate::audio::clock(length)));
                (AudioRequest::Seek(to), format!("{name}: at {}{of}.", crate::audio::clock(to)))
            }
            Some("mute") => match args.get(2).map(String::as_str) {
                Some("on") if state.audio_volume == 0.0 => {
                    (AudioRequest::Mute(true), "The audio is muted already.".to_owned())
                }
                Some("on") => (AudioRequest::Mute(true), "Muted the audio.".to_owned()),
                Some("off") if state.audio_volume > 0.0 => {
                    (AudioRequest::Mute(false), "The audio isn't muted.".to_owned())
                }
                Some("off") => {
                    let back = state.audio_muted_from.unwrap_or(1.0);
                    (AudioRequest::Mute(false), format!("The audio is back, at volume {}.", (back * 100.0).round()))
                }
                _ => return Err("audio mute on|off".to_owned()),
            },
            Some("volume") => {
                let level: f32 = args.get(2).and_then(|w| w.parse().ok()).ok_or("audio volume 0-100")?;
                if !(0.0..=100.0).contains(&level) {
                    return Err(format!("The volume goes from 0 to 100, not {level}."));
                }
                (AudioRequest::Volume(level / 100.0), format!("Volume {level}."))
            }
            _ => {
                // What the player has and how tracks follow on, then every track, numbered.
                let playing = state.audio_playing;
                let track = playing.track.and_then(|id| state.audio.track(id)).map(|track| track.name.clone());
                let at = |position| {
                    let length = playing.length.map_or_else(String::new, |l| format!(" of {}", crate::audio::clock(l)));
                    format!("{}{length}", crate::audio::clock(position))
                };
                let now = match (track, playing.paused) {
                    (Some(name), false) => format!("Playing: {name}, at {}", at(playing.position)),
                    (Some(name), true) => format!("Paused: {name}, at {}", at(playing.position)),
                    (None, _) => "Nothing playing".to_owned(),
                };
                let on = |on: bool| if on { "on" } else { "off" };
                let fade = state.audio.crossfade;
                let switches = format!(
                    "Play through: {}   Shuffle: {}   Loop: {}   Crossfade: {}   Volume: {}",
                    on(state.audio.through),
                    on(state.audio.shuffle),
                    on(state.audio.looping),
                    if fade > 0.0 { format!("{fade} s") } else { "off".to_owned() },
                    (state.audio_volume * 100.0).round() as u32,
                );
                let names: Vec<String> = tracks.into_iter().map(|(_, name)| name).collect();
                let text = format!("{now}\n{switches}\n{}", numbered(&names));
                return Ok(Reply::with(text, serde_json::json!(names)));
            }
        };
        self.with_state(|state| state.audio_request = Some(request));
        self.handle_audio();
        // A track that can't be played says so, rather than "Playing".
        if let AudioRequest::Play(id) = request {
            self.audio_outcome(id)?;
        }
        Ok(Reply::done(done))
    }

    /// A chapter or verse the book doesn't have, said rather than read as the last one (as
    /// typing in Scripture Lookup reads it): counted in the first translation, which numbers the
    /// passage, and named in the language typed.
    pub(crate) fn bible_reference_problem(&mut self, reference: &str) -> Option<String> {
        let running = self.running.as_mut()?;
        let page = &mut running.home_state.bible;
        let language = page.typed_language(reference);
        // Counted in the Bible of the numbering it is read in (the language typed's).
        let lead = page.lead().and_then(|lead| page.shelf.get(lead).ok());
        if let (Some(bible), Some(lead)) = (page.counted_in(reference), lead)
            && let Ok(range) = sp_bible::parse_reference_in(reference, &lead.info.language)
        {
            let book = sp_bible::book_name(range.start.book, &language);
            let chapters = bible.chapter_count(range.start.book);
            if chapters > 0 && (range.start.chapter > chapters || range.end.chapter > chapters) {
                return Some(format!("{book} has {chapters} chapters."));
            }
            let verses = bible.last_verse(range.start.book, range.start.chapter);
            if verses > 0 && range.start.verse != sp_bible::LAST && range.start.verse > verses {
                return Some(format!("{book} {} has {verses} verses.", range.start.chapter));
            }
        }
        None
    }

    fn ctl_bible(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_bible_more(args) {
            return result;
        }
        let show_it = args.iter().any(|w| w == "--show");
        // `--add [to PLAYLIST]`: the passage goes into a playlist, as Add to puts it there.
        let (args, add_to) = match args.iter().position(|w| w == "--add") {
            Some(at) => {
                let tail = &args[at + 1..];
                let into = match tail.first() {
                    Some(word) if word.eq_ignore_ascii_case("to") => Some(Some(tail[1..].join(" "))),
                    _ => Some(None),
                };
                let mut kept = args[..at].to_vec();
                if !matches!(into, Some(Some(_))) {
                    kept.extend(tail.iter().cloned());
                }
                (kept, into)
            }
            None => (args.to_vec(), None),
        };
        let args = args.as_slice();
        let reference: String =
            args.get(1..).unwrap_or_default().iter().filter(|w| *w != "--show").cloned().collect::<Vec<_>>().join(" ");
        if reference.trim().is_empty() {
            return Err("Which passage? For example: bible John 3:16-18".to_owned());
        }
        let live = self.show.live().map(|at| at.presentation);
        if let Some(problem) = self.bible_reference_problem(&reference) {
            return Err(problem);
        }
        let (found, message, title, first) = {
            let running = self.running.as_mut().ok_or("The app isn't ready.")?;
            let state = &mut running.home_state;
            state.bible_open = true;
            state.editing = None;
            state.bible.lookup = reference.clone();
            state.bible.look_up(live);
            // Said back as the Bible page names it to the operator, in the language typed; the
            // slides are made from the translations chosen there, as before. The passage's first
            // verse is what goes on screen, among its chapter's slides when they were made.
            let first = state.bible.passage_slides().start;
            (state.bible.pending.take(), state.bible.message.clone(), state.bible.reading.clone(), first)
        };
        let Some((slot, passage)) = found else {
            return Err(message.unwrap_or_else(|| format!("Could not look up {reference}.")));
        };
        self.show.replace_presentation(slot, std::sync::Arc::new(passage));
        if let Some(into) = add_to {
            let list = self.playlist_at_for_cli(into.as_deref())?;
            let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
            let saved = state.bible.saved().ok_or("The passage could not be kept for a playlist.")?;
            let name = state.services.list[list].name.clone();
            // The same passage in the same translations goes in once.
            let added = state.services.add_to(list, crate::service::Item::Passage(saved));
            if show_it {
                self.apply(vec![Command::Select(slot), Command::GoTo(SlideRef { presentation: slot, slide: first })]);
            }
            let said =
                if added { format!("Added {title} to {name}.") } else { format!("{title} is already in {name}.") };
            return Ok(Reply::done(said));
        }
        if show_it {
            self.apply(vec![Command::Select(slot), Command::GoTo(SlideRef { presentation: slot, slide: first })]);
            return Ok(Reply::done(format!("{title}: {}", self.live_words())));
        }
        Ok(Reply::done(format!("Looked up {title}; it is on the Bible page.")))
    }

    fn ctl_song(&mut self, args: &[String]) -> Result<Reply, String> {
        if !args.iter().any(|w| w == "--library")
            && let Some(result) = self.ctl_song_change(args)
        {
            return result;
        }
        if let Some(result) = self.ctl_song_arrangement(args) {
            return result;
        }
        // `--library NAME` (or `--library none`): the library new and imported songs go into,
        // instead of the one shown in the window.
        if let Some(at) = args.iter().position(|w| w == "--library") {
            let name = args.get(at + 1).cloned().ok_or("--library NAME, or --library none")?;
            let mut rest: Vec<String> = args.to_vec();
            rest.drain(at..=at + 1);
            let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
            let wanted = if name.eq_ignore_ascii_case("none") {
                None
            } else {
                let names: Vec<String> = state.libraries.iter().map(|l| l.name.clone()).collect();
                Some(state.libraries[find("library", &names, &name)?].id)
            };
            let shown = std::mem::replace(&mut state.library_shown, wanted);
            // A folder imported then goes into that library (or none), not one named after it.
            self.import_library_override = Some(wanted);
            let result = self.ctl_song(&rest);
            self.import_library_override = None;
            if let Some(running) = self.running.as_mut() {
                running.home_state.library_shown = shown;
            }
            return result;
        }
        // Where new songs go: the library shown, said in the reply.
        let into = self.running.as_ref().and_then(|r| {
            let state = &r.home_state;
            state.library_shown.and_then(|id| state.libraries.iter().find(|l| l.id == id)).map(|l| l.name.clone())
        });
        let into_words = into.map_or_else(String::new, |name| format!(" into the library {name}"));
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("new") => {
                // `song new TITLE --words TEXT`: the words given as song text, in one word (an
                // AI assistant's, a script's), every other word the title.
                let mut words = args.get(2..).unwrap_or_default().to_vec();
                let given = match words.iter().position(|w| w == "--words") {
                    Some(at) => {
                        let text = words.get(at + 1).cloned().ok_or("song new TITLE --words TEXT")?;
                        words.drain(at..=at + 1);
                        Some(text)
                    }
                    None => None,
                };
                let words = &words[..];
                // `song new TITLE [FILE.txt]`: the file is the last word when it names a file.
                // From a remote page every word is the title: the disk is never looked at.
                let files = !self.outside_call() && given.is_none();
                let (title_words, file) = match words.last() {
                    Some(last) if files && words.len() > 1 && Path::new(last).is_file() => {
                        (&words[..words.len() - 1], Some(last))
                    }
                    // A last word that names a file that isn't there is a mistake, not the title.
                    Some(last)
                        if files
                            && words.len() > 1
                            && (last.contains('/') || crate::load::is_importable(Path::new(last))) =>
                    {
                        return Err(format!("There is no file {last}."));
                    }
                    _ => (words, None),
                };
                let title = title_words.join(" ");
                if title.trim().is_empty() {
                    return Err("song new TITLE [FILE.txt]".to_owned());
                }
                let words = match (file, given) {
                    (Some(file), _) => {
                        sp_model::decode_text(&std::fs::read(file).map_err(|e| format!("Could not read {file}: {e}"))?)
                    }
                    (None, Some(given)) => given,
                    (None, None) => String::new(),
                };
                let request = crate::home::NewSong { title: title.trim().to_owned(), theme: None, words, focus: false };
                self.with_state(|state| state.new_song_request = Some(request));
                let same =
                    self.listed_songs().iter().filter(|(_, have)| have.eq_ignore_ascii_case(title.trim())).count();
                self.handle_songs();
                let mut text = format!("Made the song {}{into_words}.", title.trim());
                if same > 0 {
                    text.push_str(&format!(
                        " There was already a song called {}; give its number (sp songs) to tell them apart.",
                        title.trim()
                    ));
                }
                Ok(Reply::done(text))
            }
            Some("delete") => {
                let yes = args.iter().any(|w| w == "--yes");
                let words: Vec<String> = args.get(2..).unwrap_or_default().iter().filter(|w| *w != "--yes").cloned().collect();
                let at = self.find_song(&words.join(" "))?;
                let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
                if !yes {
                    return Err(format!(
                        "Deleting {title} can't be undone (a song made in SoftPresenter goes to the Trash). Add --yes to do it."
                    ));
                }
                self.with_state(|state| state.songs_to_delete = vec![at]);
                self.handle_song_request();
                Ok(Reply::done(format!("Deleted {title}.")))
            }
            Some("text") => self.ctl_song_text(args.get(2..).unwrap_or_default()),
            Some("reload") => self.ctl_song_reload(args.get(2..).unwrap_or_default()),
            Some("keep") => {
                let at = self.find_song_as_is(&rest(args, 2))?;
                let waiting = self.held_songs.contains(&at)
                    || self.running.as_ref().is_some_and(|r| {
                        r.home_state.notices.asks_about(crate::outside::Waiting::Song(at))
                    });
                let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
                if !waiting {
                    return Err(format!("{title} isn't waiting on a choice: its file hasn't changed outside SoftPresenter."));
                }
                let title = self.keep_song(at)?;
                crate::applog::event("outside", &format!("kept SoftPresenter's version of {title} (sp song keep)"));
                Ok(Reply::done(format!("Saved SoftPresenter's version of {title} over its file.")))
            }
            Some("export") => self.ctl_song_export(args.get(2..).unwrap_or_default()),
            Some("import") => {
                self.only_at_the_computer(crate::remote::policy::AtComputer::ImportSongs)?;
                // Song packs: what to do with songs the library has already (skip, unless told).
                let same = if args.iter().any(|w| w == "--replace") {
                    crate::pack::Same::Replace
                } else if args.iter().any(|w| w == "--keep-both") {
                    crate::pack::Same::KeepBoth
                } else {
                    crate::pack::Same::Skip
                };
                // `--as slides` or `--as pictures`: how presentation files come in (otherwise as
                // each kind was last brought in).
                let mut words: Vec<String> = args.get(2..).unwrap_or_default().to_vec();
                let mut deck_as = None;
                if let Some(at) = words.iter().position(|w| w == "--as") {
                    let how = words.get(at + 1).and_then(|w| crate::deck::ImportAs::from_name(w));
                    deck_as = Some(how.ok_or("--as slides, or --as pictures")?);
                    words.drain(at..=at + 1);
                }
                let mut paths: Vec<std::path::PathBuf> = words
                    .iter()
                    .filter(|w| !matches!(w.as_str(), "--skip" | "--replace" | "--keep-both"))
                    .map(std::path::PathBuf::from)
                    .collect();
                if paths.is_empty() {
                    return Err("song import FILE…".to_owned());
                }
                // Packs are opened here, and what came in is said; the rest import as songs.
                let library = self.running.as_ref().and_then(|r| r.home_state.library_shown);
                let packs: Vec<std::path::PathBuf> = paths.iter().filter(|path| crate::pack::is_pack(path)).cloned().collect();
                paths.retain(|path| !crate::pack::is_pack(path));
                let mut said_packs = Vec::new();
                let mut any_in = false;
                for path in packs {
                    if !path.is_file() {
                        return Err(format!("There is no file {}.", path.display()));
                    }
                    match self.open_pack(path, Some(same), library, false) {
                        Ok(said) => {
                            any_in = true;
                            said_packs.push(said);
                        }
                        Err(problem) => said_packs.push(problem),
                    }
                }
                if paths.is_empty() {
                    let text = said_packs.join("\n");
                    return if any_in { Ok(Reply::done(text)) } else { Err(text) };
                }
                let done = self.import_songs_to(&paths, &crate::SongsTo { deck_as, ..crate::SongsTo::default() });
                let (imported, problems, decks) = (&done.titles, &done.problems, &done.decks);
                let mut busy: Vec<String> = Vec::new();
                if !decks.is_empty() {
                    busy.push(format!(
                        "Making slides from {}; each appears in the song list when it is done.",
                        decks.join(", ")
                    ));
                }
                // A large song bundle is read in the background; `sp job` says when it is in.
                if !done.opening.is_empty() {
                    busy.push(format!(
                        "Opening {}; its songs come in when it has been read.",
                        done.opening.join(", ")
                    ));
                }
                let data = serde_json::json!({
                    "imported": imported,
                    "problems": problems,
                    "making": decks,
                    "opening": done.opening,
                    "notes": done.notes,
                    "pictures": done.pictures,
                    "videos": done.videos,
                });
                if imported.is_empty() {
                    if !busy.is_empty() {
                        let mut text = busy.join("\n");
                        for problem in problems.iter().take(10) {
                            text.push_str(&format!("\n{problem}"));
                        }
                        return Ok(Reply::with(text, data));
                    }
                    return Err(if problems.is_empty() {
                        "Nothing was imported.".to_owned()
                    } else {
                        // Songs here already, and what a bundle left out.
                        problems.iter().chain(&done.notes).cloned().collect::<Vec<_>>().join("\n")
                    });
                }
                // The text names the first few; the data lists them all.
                const NAMED: usize = 10;
                let mut text = match imported.as_slice() {
                    [one] => format!("Imported {one}{into_words}."),
                    many if many.len() <= NAMED => {
                        format!("Imported {} songs{into_words}: {}.", many.len(), many.join(", "))
                    }
                    many => format!(
                        "Imported {} songs{into_words}: {}, and {} more.",
                        many.len(),
                        many[..NAMED].join(", "),
                        many.len() - NAMED
                    ),
                };
                // A bundle's pictures and videos, and what its songs came without.
                let things: Vec<String> = [(done.pictures, "picture"), (done.videos, "video")]
                    .into_iter()
                    .filter(|(n, _)| *n > 0)
                    .map(|(n, what)| format!("{n} {what}{}", if n == 1 { "" } else { "s" }))
                    .collect();
                if !things.is_empty() {
                    text.push_str(&format!("\nBrought {} into the media bin.", things.join(" and ")));
                }
                for note in &done.notes {
                    text.push_str(&format!("\n{note}"));
                }
                for problem in problems.iter().take(NAMED) {
                    text.push_str(&format!("\n{problem}"));
                }
                if problems.len() > NAMED {
                    text.push_str(&format!("\n…and {} more not imported (see the log).", problems.len() - NAMED));
                }
                for line in busy {
                    text.push_str(&format!("\n{line}"));
                }
                Ok(Reply::with(text, data))
            }
            _ => Err("song new TITLE [FILE.txt], song import FILE…, song text SONG [FILE], song reload [SONG], song keep SONG, or song delete SONG --yes"
                .to_owned()),
        }
    }

    /// Undoes (or redoes) the last change to a song, whatever view the window shows, and says
    /// what changed. In a song's Edit view it takes the step Cmd-Z would there, as the editor
    /// keeps its own order of steps (typing in a box, then the song's slides).
    fn ctl_undo(&mut self, el: &ActiveEventLoop, redo: bool) -> Result<Reply, String> {
        let word = if redo { "redo" } else { "undo" };
        let in_editor = self.running.as_ref().is_some_and(|r| {
            let state = &r.home_state;
            state.theme_edit.is_none()
                && state.editing.as_ref().is_some_and(|draft| {
                    draft.at.presentation != crate::props::PROP_SLOT
                        && draft.at.presentation != crate::stage::STAGE_SLOT
                })
        });
        if in_editor {
            self.menu_action(el, if redo { crate::menubar::Action::Redo } else { crate::menubar::Action::Undo });
            return Ok(Reply::done(format!("Took the Edit view's {word} step (as Cmd-Z does there).")));
        }
        // Something deleted, when it was the last change: put back, or deleted again.
        self.keep_removed();
        if self.history.removed_next(redo) {
            let done = self.undo_removed(redo).ok_or_else(|| format!("There is nothing to {word}."))?;
            if let Some(running) = self.running.as_ref() {
                running.home_window.request_redraw();
            }
            return done.map(Reply::done);
        }
        let index = self.history.peek(redo).ok_or_else(|| format!("There is nothing to {word}."))?;
        let before = self.show.presentations().get(index).cloned().ok_or("That song is gone.")?;
        self.undo_show(redo);
        let after = self.show.presentations().get(index).cloned().ok_or("That song is gone.")?;
        let what = changed_slides(&before, &after);
        let done = if redo { "Redid" } else { "Undid" };
        Ok(Reply::with(
            format!("{done} a change to {}{what}.", after.title),
            serde_json::json!({"song": after.title, "number": index + 1}),
        ))
    }

    fn ctl_menu(&mut self, el: &ActiveEventLoop, args: &[String]) -> Result<Reply, String> {
        let items = crate::menubar::labelled_items();
        if args.len() < 2 {
            let lines: Vec<String> =
                items.iter().map(|(action, label)| format!("{:<24} {label}", crate::menubar::id_of(*action))).collect();
            let data: Vec<serde_json::Value> = items
                .iter()
                .map(|(action, label)| serde_json::json!({"id": crate::menubar::id_of(*action), "label": label}))
                .collect();
            return Ok(Reply::with(lines.join("\n"), serde_json::json!(data)));
        }
        // By its id ("ClearAll") or the words on it ("Clear All").
        let typed = rest(args, 1);
        let action = crate::menubar::chosen(&typed)
            .ok_or_else(|| format!("No menu item called \u{201c}{typed}\u{201d}. sp menu lists them."))?;
        self.menu_action(el, action);
        Ok(Reply::done(format!("Chose {} from the menu.", crate::menubar::id_of(action))))
    }

    /// `front`: the operator's window asked to come to the front, restored first if it was
    /// minimised. A start of the app turned away by this copy asks for it (`one_copy`) with
    /// `--start`, which isn't offered to people, so the log says it was that. A quiet copy (a
    /// test's) leaves its window behind everything, and says so. A copy that is quitting refuses,
    /// so the start goes on trying and opens the app once this one has gone, rather than ending
    /// with nothing open. Only what the system says is claimed: in front once the window has the
    /// keyboard, asked otherwise.
    fn ctl_front(&mut self, args: &[String]) -> Result<Reply, String> {
        let start = match args.get(1) {
            None => false,
            Some(word) if word == "--start" && args.len() == 2 => true,
            _ => return Err("front takes nothing after it.".to_owned()),
        };
        if self.quitting.is_some() || self.left {
            return Err("SoftPresenter is closing.".to_owned());
        }
        let quiet = self.quiet();
        let Some(running) = self.running.as_ref() else {
            return Err("The window isn't open yet.".to_owned());
        };
        let in_front = !quiet && {
            // Windows brings a minimised window to the front only once it is restored.
            if running.home_window.is_minimized() == Some(true) {
                running.home_window.set_minimized(false);
            }
            running.home_window.focus_window();
            running.home_window.has_focus()
        };
        if start {
            crate::applog::event(
                "start",
                if quiet {
                    "another start of SoftPresenter asked for this window to come to the front (a quiet copy's stays behind)"
                } else if in_front {
                    "another start of SoftPresenter brought this window to the front"
                } else {
                    "another start of SoftPresenter asked this window to come to the front (the system hasn't said it came)"
                },
            );
        }
        Ok(Reply::done(if quiet {
            "A quiet copy's window stays behind everything."
        } else if in_front {
            "The window is in front."
        } else {
            "Asked the window to come to the front."
        }))
    }
}

/// A look setting's value as typed, checked against VoxLive's description of the setting: one of
/// its values, a colour (#RGB to #RRGGBBAA), on or off, or a number in its range.
fn setting_value(field: &serde_json::Value, key: &str, typed: &str) -> Result<serde_json::Value, String> {
    let typed = typed.trim();
    match field.get("type").and_then(serde_json::Value::as_str).unwrap_or("") {
        "enum" => {
            let values = field.get("values").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
            values
                .iter()
                .find(|value| {
                    value.as_str().map_or_else(|| value.to_string(), str::to_owned).eq_ignore_ascii_case(typed)
                })
                .cloned()
                .ok_or_else(|| {
                    let choices: Vec<String> =
                        values.iter().map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned)).collect();
                    format!("{key} is one of {}.", choices.join(", "))
                })
        }
        "color" => {
            let digits = typed.trim_start_matches('#');
            let ok = matches!(digits.len(), 3 | 4 | 6 | 8) && digits.chars().all(|c| c.is_ascii_hexdigit());
            if ok {
                Ok(serde_json::Value::String(format!("#{}", digits.to_uppercase())))
            } else {
                Err(format!("{key} is a colour such as #FFE14D."))
            }
        }
        "boolean" => match typed.to_lowercase().as_str() {
            "on" | "yes" | "true" => Ok(serde_json::Value::Bool(true)),
            "off" | "no" | "false" => Ok(serde_json::Value::Bool(false)),
            _ => Err(format!("{key} is on or off.")),
        },
        "number" => {
            let number: f64 = typed.parse().map_err(|_| format!("{key} is a number."))?;
            let min = field.get("min").and_then(serde_json::Value::as_f64).unwrap_or(f64::MIN);
            let max = field.get("max").and_then(serde_json::Value::as_f64).unwrap_or(f64::MAX);
            if !(min..=max).contains(&number) {
                return Err(format!("{key} is {min} to {max}."));
            }
            if number.fract() == 0.0 {
                Ok(serde_json::Value::from(number as i64))
            } else {
                serde_json::Number::from_f64(number)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| format!("{key} is a number."))
            }
        }
        _ => Err(format!("{key} can't be set from here.")),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_slide_that_isnt_there_is_said_in_the_windows_language_and_in_english_to_sp() {
        use crate::i18n::{Language, english_of, with_language};
        let english = with_language(Language::English, || super::no_such_slide("River Song", 6, "9"));
        assert_eq!(english, "River Song has 6 slides; there is no slide 9.");
        assert_eq!(
            with_language(Language::English, || super::no_such_slide("Amen", 1, "2")),
            "Amen has 1 slide; there is no slide 2."
        );
        let spanish = with_language(Language::Spanish, || super::no_such_slide("River Song", 6, "9"));
        assert_eq!(spanish, "River Song tiene 6 diapositivas; no hay diapositiva 9.");
        // What `sp` writes (write_reply) is in English again.
        assert_eq!(english_of(&spanish), english);
    }

    #[test]
    fn the_status_says_when_the_words_on_screen_are_hidden() {
        use crate::i18n::{Language, english_of, with_language};
        let on_screen = "On screen: River Song, slide 2 of 3 (Verse 2).";
        assert_eq!(super::on_screen_line(on_screen.to_owned(), false), on_screen);
        let hidden = "On screen: River Song, slide 2 of 3 (Verse 2) (words hidden).";
        assert_eq!(with_language(Language::English, || super::on_screen_line(on_screen.to_owned(), true)), hidden);
        // In the window's language for a page; `sp` gets it in English, as all its replies.
        let spanish = with_language(Language::Spanish, || super::on_screen_line(on_screen.to_owned(), true));
        assert_ne!(spanish, hidden);
        assert_eq!(english_of(&spanish), hidden);
    }

    /// `job N` always says the run it comes from, and `done` only for a job this run has: so a
    /// job of an earlier run, whose number came round again, is never taken for this run's.
    #[test]
    fn a_jobs_reply_says_its_run_and_done_only_for_a_job_of_this_run() {
        let going = super::job_reply(3, Some(&None), "run-a");
        assert!(going.ok);
        assert_eq!(going.data, serde_json::json!({"job": 3, "run": "run-a", "done": false}));
        let done = super::job_reply(3, Some(&Some(Ok("Installed the Bible.".to_owned()))), "run-a");
        assert_eq!((done.ok, done.text.as_str()), (true, "Installed the Bible."));
        assert_eq!(done.data["done"], true);
        let failed = super::job_reply(3, Some(&Some(Err("It stopped.".to_owned()))), "run-a");
        assert_eq!((failed.ok, failed.data["done"].clone()), (false, serde_json::json!(true)));
        let none = super::job_reply(9, None, "run-a");
        assert_eq!((none.ok, none.text.as_str()), (false, "There is no job 9."));
        assert_eq!(none.data, serde_json::json!({"job": 9, "run": "run-a"}));
    }

    /// Every first word the app carries out a command for starts a command when typed after
    /// `sp` (without `ctl`), so a new command can't be left out of COMMANDS again.
    #[test]
    fn every_command_the_app_knows_can_be_typed_after_sp() {
        let source = include_str!("control.rs");
        let start = source.find("let result = match word.as_str() {").expect("the commands' match");
        let end = start + source[start..].find("            _ =>").expect("its end");
        let mut words = Vec::new();
        for line in source[start..end].lines() {
            let line = line.trim();
            let Some((pattern, _)) = line.split_once("=>") else { continue };
            if !pattern.trim_start().starts_with('"') {
                continue;
            }
            for word in pattern.split('|') {
                let word = word.trim().trim_matches('"');
                if !word.is_empty() {
                    words.push(word.to_owned());
                }
            }
        }
        assert!(words.len() > 20, "found the commands: {words:?}");
        for word in &words {
            assert!(
                is_command(word),
                "\"{word}\" is carried out but not in COMMANDS, so `sp {word}` would open it as a file"
            );
        }
    }

    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    /// The quoted words of a match's arms, from `start` to the next `_ =>` at that indent.
    fn arms(source: &str, start: &str, end: &str) -> Vec<String> {
        let from = source.find(start).unwrap_or_else(|| panic!("found {start}"));
        let to = from + source[from..].find(end).unwrap_or_else(|| panic!("found the end of {start}"));
        let mut words = Vec::new();
        for line in source[from..to].lines() {
            let Some((pattern, _)) = line.trim().split_once("=>") else { continue };
            if !pattern.trim_start().starts_with('"') {
                continue;
            }
            words.extend(pattern.split('|').map(|w| w.trim().trim_matches('"').to_owned()).filter(|w| !w.is_empty()));
        }
        words
    }

    #[test]
    fn an_aim_names_an_item_and_the_playlists_rev_and_leaves_the_other_words() {
        let aim = |item: &str, rev: Option<&str>| Aim { item: item.to_owned(), rev: rev.map(str::to_owned) };
        assert_eq!(aim_words(&names(&[])), Ok((Vec::new(), None)));
        assert_eq!(aim_words(&names(&["--aim", "3"])), Ok((Vec::new(), Some(aim("3", None)))));
        assert_eq!(
            aim_words(&names(&["--aim", "Opening", "2", "--rev", "00ff"])),
            Ok((names(&["2"]), Some(aim("Opening", Some("00ff")))))
        );
        assert!(aim_words(&names(&["--aim"])).is_err());
        assert!(aim_words(&names(&["--rev", "00ff"])).is_err());
        // A playlist's rev changes when its items do, and only then.
        let mut list = crate::service::Service {
            name: "Sunday".to_owned(),
            items: Vec::new(),
            uids: Default::default(),
            origin: None,
        };
        let empty = list.rev();
        list.items.push(crate::service::Item::Header { name: "Worship".to_owned() });
        assert_ne!(list.rev(), empty);
        assert_eq!(list.rev(), list.clone().rev());
    }

    #[test]
    fn a_wait_for_a_change_is_read_and_bounded() {
        let words = |text: &str| text.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(wait_for(&words("status")), None);
        assert_eq!(wait_for(&words("next --wait 5")), None, "only status waits for a change");
        assert_eq!(wait_for(&words("status --wait 20")), Some(Ok((Duration::from_secs(20), None))));
        assert_eq!(
            wait_for(&words("status --wait 2.5 --since abc")),
            Some(Ok((Duration::from_millis(2500), Some("abc".to_owned()))))
        );
        assert_eq!(wait_for(&words("status --since abc --wait 0")), Some(Ok((Duration::ZERO, Some("abc".to_owned())))));
        assert_eq!(wait_for(&words("status --wait 31")), Some(Err("A wait is 0 to 30 seconds, not 31.".to_owned())));
        assert!(matches!(wait_for(&words("status --wait -1")), Some(Err(_))));
        assert!(matches!(wait_for(&words("status --wait")), Some(Err(_))));
        assert!(matches!(wait_for(&words("status --wait soon")), Some(Err(_))));
        assert!(matches!(wait_for(&words("status --wait 5 --since")), Some(Err(_))));
        assert!(matches!(wait_for(&words("status --wait 5 --since --json")), Some(Err(_))));
    }

    #[test]
    fn a_token_says_which_parts_changed_and_ignores_the_clocks() {
        assert_eq!(changed_parts("a-b-c", "a-b-c"), Vec::<&str>::new());
        assert_eq!(changed_parts("a-b-c", "x-b-c"), ["status"]);
        assert_eq!(changed_parts("a-b-c", "a-y-z"), ["playlists", "library"]);
        assert_eq!(changed_parts("old", "a-b-c"), ["status", "playlists", "library"], "not a token: all changed");

        let status = |shows: &str, seconds: u64, running: bool| {
            serde_json::json!({
                "live": null,
                "timers": [{"name": "Sermon", "kind": "counts down from 30:00", "shows": shows, "running": running}],
                "audio": {"track": "Welcome", "paused": false, "seconds": seconds, "volume": 80},
                "token": "x-y-z",
            })
        };
        let ticking = without_clocks(&status("00:29:59", 1, true));
        assert_eq!(ticking, without_clocks(&status("00:29:58", 2, true)), "a second passing is no change");
        assert_ne!(ticking, without_clocks(&status("00:29:58", 2, false)), "a timer stopping is one");
        assert!(ticking.get("token").is_none());
        assert_eq!(ticking["audio"]["track"], "Welcome");

        let reply = Reply::with("On screen.", serde_json::json!({"token": "a-b-c"}));
        assert_eq!(with_changes(reply.clone(), Some("a-q-c")).data["changed"], serde_json::json!(["playlists"]));
        assert_eq!(with_changes(reply, None).data["changed"], serde_json::json!([]));
    }

    #[test]
    fn reads_are_kept_out_of_the_log_and_changes_never() {
        let words = |text: &str| text.split(' ').map(str::to_owned).collect::<Vec<_>>();
        for read in [
            "status",
            "status --wait 20 --since abc",
            "songs",
            "slides River",
            "slides item 3 in Friday --full",
            "slide River 2",
            "slide item 3 2 in Friday",
            "playlist",
            "playlist Friday",
            "playlists",
            "media",
            "audio",
            "audio playlists",
            "timers",
            "search river",
            "job 2",
            "settings",
            "menu",
            "voxlive screens",
            "help song",
            "bible text John 3:16",
            "bible search valley",
        ] {
            assert!(only_reads(&words(read)), "{read} only reads");
        }
        for change in [
            "playlist add River to Friday",
            "playlist open Friday",
            "playlist go 2 1",
            "playlist fill Sermon with River",
            "slide text River 1 words",
            "slide delete River 2",
            "slide media River 2",
            "media show 1",
            "media-playlist play Loop",
            "audio play Welcome",
            "settings set columns=4",
            "menu Quit",
            "voxlive connect",
            "go River 2",
            "next",
            "bible John 3:16",
            "bible John 3:16 --add",
            "bible install ASV",
            "playlist passage John 3:16 in Friday",
            "song text River words.txt",
            "library add River to Hymns",
            "something new",
        ] {
            assert!(!only_reads(&words(change)), "{change} is logged");
        }
        // Every word that changes a playlist or a slide is known as one, so a new one can't be
        // taken for a playlist's or song's name and left out of the log.
        let service = include_str!("service_cli.rs");
        let playlist = arms(service, "fn ctl_playlist_change", "            _ => return None");
        let slide = arms(service, "fn ctl_slide_change", "            _ => return None");
        assert!(playlist.len() > 8 && slide.len() > 10, "{playlist:?} {slide:?}");
        // `playlist open` and `playlist add` are carried out here, in `ctl_playlist`.
        for word in playlist.iter().map(String::as_str).chain(["open", "add"]) {
            assert!(PLAYLIST_CHANGES.contains(&word), "playlist {word} changes something");
        }
        // `slide text|style|frame` are carried out in slide_cli.rs, as `Some("text") => …`.
        let slides = include_str!("slide_cli.rs");
        let from = slides.find("fn ctl_slide(&mut self").expect("ctl_slide");
        let to = from + slides[from..].find("Some(_) =>").expect("its end");
        let own: Vec<&str> = slides[from..to]
            .lines()
            .filter_map(|line| line.trim().strip_prefix("Some(\"")?.split('"').next())
            .collect();
        assert_eq!(own, ["text", "style", "frame"]);
        for word in slide.iter().map(String::as_str).chain(own) {
            assert!(SLIDE_CHANGES.contains(&word), "slide {word} changes something");
        }
    }

    #[test]
    fn a_name_is_found_whole_in_part_or_by_number_and_never_guessed() {
        let songs = names(&["Amazing Grace", "How Great Thou Art", "Great Is Thy Faithfulness"]);
        assert_eq!(find("song", &songs, "amazing grace"), Ok(0));
        assert_eq!(find("song", &songs, "amaz"), Ok(0));
        assert_eq!(find("song", &songs, "2"), Ok(1));
        let both = find("song", &songs, "great").unwrap_err();
        assert!(both.contains("How Great Thou Art") && both.contains("Great Is Thy Faithfulness"), "{both}");
        assert!(find("song", &songs, "nothing like it").is_err());
        assert!(find("song", &songs, "9").is_err(), "a number past the end is not a place");
    }

    #[test]
    fn requests_and_replies_round_trip_as_lines_of_json() {
        let request = Request { v: PROTOCOL, args: names(&["go", "Amazing Grace", "3"]), mcp: None };
        let text = serde_json::to_string(&request).unwrap();
        assert!(!text.contains('\n'));
        let back: Request = serde_json::from_str(&text).unwrap();
        assert_eq!(back.args, request.args);
        let reply = Reply::done("On screen.");
        let back: Reply = serde_json::from_str(&serde_json::to_string(&reply).unwrap()).unwrap();
        assert!(back.ok && back.text == "On screen.");
    }

    #[test]
    fn look_settings_are_checked_against_voxlives_description() {
        let size = serde_json::json!({"type": "number", "min": 20, "max": 160});
        assert_eq!(setting_value(&size, "size", "72"), Ok(serde_json::json!(72)));
        assert_eq!(setting_value(&size, "size", "1.5e1").unwrap_err(), "size is 20 to 160.");
        assert!(setting_value(&size, "size", "big").is_err());
        let shadow = serde_json::json!({"type": "enum", "values": ["none", "soft"]});
        assert_eq!(setting_value(&shadow, "shadow", "Soft"), Ok(serde_json::json!("soft")));
        assert!(setting_value(&shadow, "shadow", "glow").is_err());
        let colour = serde_json::json!({"type": "color"});
        assert_eq!(setting_value(&colour, "text", "ffe14d"), Ok(serde_json::json!("#FFE14D")));
        assert!(setting_value(&colour, "text", "#ffe14\"").is_err());
        let on = serde_json::json!({"type": "boolean"});
        assert_eq!(setting_value(&on, "transparent", "on"), Ok(serde_json::json!(true)));
        assert!(setting_value(&serde_json::json!({"type": "list"}), "x", "1").is_err());
    }

    /// A folder for one test's socket. Its path is short, as a socket's must be.
    fn socket_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("sp-ctl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    /// Listens at a socket as the app does, answering each request with the words it was sent.
    fn echo_at(path: &Path) -> Result<Listening, String> {
        listen_at(path.to_owned(), |incoming: Incoming| {
            let heard = format!("heard {}", incoming.request.args.join(" "));
            let _ = incoming.reply.send(Reply::with(heard, serde_json::json!(incoming.request.args)));
        })
    }

    /// Sends words to that socket with the code `sp` sends them with.
    fn say_to(path: &Path, words: &[&str]) -> Result<Reply, String> {
        let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
        send_to(|| Some(path.to_owned()), &words)
    }

    const NOT_RUNNING: &str = "SoftPresenter isn't running";

    /// Where the socket is on a system with these variables set, the data folder being `data`.
    fn socket_on(system: &str, set: &[(&str, &str)]) -> Option<PathBuf> {
        let var = |name: &str| set.iter().find(|(set, _)| *set == name).map(|(_, value)| PathBuf::from(value));
        socket_path_on(system, var, || Some(PathBuf::from("data")))
    }

    #[test]
    fn the_socket_is_kept_where_each_system_keeps_what_stays_on_the_computer() {
        let roaming = ("APPDATA", "roaming");
        let local = ("LOCALAPPDATA", "local");
        let home = ("HOME", "home");
        let ends = |path: Option<PathBuf>, with: &[&str]| {
            let path = path.expect("a place for the socket");
            let wanted: PathBuf = with.iter().collect();
            assert!(path.ends_with(&wanted), "{} ends with {}", path.display(), wanted.display());
        };
        // Windows: the local app data folder, never the one that travels with the account.
        ends(socket_on("windows", &[roaming, local, home]), &["local", "SoftPresenter", "control", "ctl.sock"]);
        assert_eq!(socket_on("windows", &[roaming, home]), None, "no local folder, no socket");
        // A Mac: the caches folder, outside the library.
        ends(socket_on("macos", &[home]), &["home", "Library", "Caches", "SoftPresenter", "control", "ctl.sock"]);
        // A data folder of its own (a test copy) keeps its socket with it, on every system.
        for system in ["windows", "macos", "linux"] {
            let own = ("SOFTPRESENTER_DATA", "data");
            ends(socket_on(system, &[own, roaming, local, home]), &["data", "control", "ctl.sock"]);
            // The path named outright is used as it is.
            let named = ("SOFTPRESENTER_CONTROL_SOCKET", "named.sock");
            assert_eq!(socket_on(system, &[named, own, local, home]), Some(PathBuf::from("named.sock")));
        }
        ends(socket_on("linux", &[home]), &["data", "control", "ctl.sock"]);
    }

    #[test]
    fn a_command_reaches_the_listening_app_and_its_reply_comes_back() {
        let folder = socket_folder("round");
        let path = folder.join("c.sock");
        let listening = echo_at(&path).expect("listening");
        for words in [&["go", "River Song", "2"][..], &["status"][..]] {
            let reply = say_to(&path, words).expect("a reply");
            assert!(reply.ok);
            assert_eq!(reply.text, format!("heard {}", words.join(" ")));
            assert_eq!(reply.data, serde_json::json!(words));
        }
        // When the app stops listening the socket file goes, and the command line says so.
        drop(listening);
        assert!(std::fs::symlink_metadata(&path).is_err(), "the socket file is removed");
        let said = say_to(&path, &["status"]).expect_err("nothing listens");
        assert!(said.starts_with(NOT_RUNNING), "{said}");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_command_with_nothing_listening_says_the_app_is_not_running() {
        let folder = socket_folder("nobody");
        let said = say_to(&folder.join("c.sock"), &["status"]).expect_err("nothing listens");
        assert!(said.starts_with(NOT_RUNNING), "{said}");
        assert!(!folder.exists(), "asking makes nothing");
    }

    #[test]
    fn a_request_the_app_does_not_understand_is_told_so() {
        let folder = socket_folder("odd");
        let path = folder.join("c.sock");
        let _listening = echo_at(&path).expect("listening");
        for (request, wanted) in [
            ("{\"v\": 99, \"args\": [\"status\"]}\n", "speaks version 99 of the protocol"),
            ("not a request\n", "Not a request SoftPresenter understands"),
        ] {
            let mut stream = UnixStream::connect(&path).expect("connected");
            stream.write_all(request.as_bytes()).expect("sent");
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).expect("answered");
            let reply: Reply = serde_json::from_str(line.trim()).expect("a reply");
            assert!(!reply.ok);
            assert!(reply.text.contains(wanted), "{}", reply.text);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// `sp mcp` marks its requests as an AI assistant's; the mark reaches the app, and a request
    /// of another protocol version (an app from before the mark, or a command line from
    /// before) is refused rather than carried out without it.
    #[test]
    fn an_assistants_request_carries_its_mark_and_another_version_is_refused() {
        let folder = socket_folder("mcp");
        let path = folder.join("c.sock");
        let heard: std::sync::Arc<std::sync::Mutex<Vec<Option<McpCall>>>> = Default::default();
        let keep = heard.clone();
        let _listening = listen_at(path.clone(), move |incoming: Incoming| {
            keep.lock().unwrap().push(incoming.request.mcp.clone());
            let _ = incoming.reply.send(Reply::done("heard"));
        })
        .expect("listening");
        let call = McpCall { tool: "show_song".to_owned(), asked: true, check: true };
        let reply = send_as(|| Some(path.clone()), &names(&["status"]), Some(&call)).expect("a reply");
        assert!(reply.ok);
        say_to(&path, &["status"]).expect("a reply");
        assert_eq!(*heard.lock().unwrap(), vec![Some(call.clone()), None], "sp's own requests carry no mark");
        // What the server sends is version 2 with the mark: an app that knows only version 1
        // refuses it, as this one refuses version 1.
        let sent = serde_json::to_string(&Request { v: PROTOCOL, args: names(&["status"]), mcp: Some(call) }).unwrap();
        assert!(sent.contains("\"v\":2") && sent.contains("\"mcp\""), "{sent}");
        let mut stream = UnixStream::connect(&path).expect("connected");
        let old = "{\"v\": 1, \"args\": [\"next\"], \"mcp\": {\"tool\": \"next_slide\", \"asked\": true}}\n";
        stream.write_all(old.as_bytes()).expect("sent");
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).expect("answered");
        let reply: Reply = serde_json::from_str(line.trim()).expect("a reply");
        assert!(!reply.ok && reply.text.contains("speaks version 1"), "{}", reply.text);
        assert_eq!(heard.lock().unwrap().len(), 2, "never reached the app");
        // An older command line's own request, with no mark, still works.
        let mut stream = UnixStream::connect(&path).expect("connected");
        stream.write_all(b"{\"v\": 1, \"args\": [\"status\"]}\n").expect("sent");
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).expect("answered");
        let reply: Reply = serde_json::from_str(line.trim()).expect("a reply");
        assert!(reply.ok, "{}", reply.text);
        assert_eq!(*heard.lock().unwrap().last().unwrap(), None, "taken as the computer's own");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_second_copy_leaves_the_first_ones_socket_alone() {
        let folder = socket_folder("second");
        let path = folder.join("c.sock");
        let _first = echo_at(&path).expect("listening");
        let said = echo_at(&path).err().expect("the second is refused");
        assert_eq!(said, "Another SoftPresenter is already listening for commands.");
        assert!(say_to(&path, &["status"]).expect("the first still answers").ok);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_socket_left_by_an_app_that_stopped_is_replaced() {
        let folder = socket_folder("stale");
        let path = folder.join("c.sock");
        std::fs::create_dir_all(&folder).expect("the folder");
        drop(UnixListener::bind(&path).expect("a socket nobody listens on any more"));
        assert!(std::fs::symlink_metadata(&path).is_ok(), "its file is left behind");
        let _listening = echo_at(&path).expect("listening in its place");
        assert!(say_to(&path, &["status"]).expect("a reply").ok);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_file_in_the_sockets_place_is_left_alone() {
        let folder = socket_folder("file");
        let path = folder.join("c.sock");
        std::fs::create_dir_all(&folder).expect("the folder");
        std::fs::write(&path, "kept").expect("a file");
        let said = echo_at(&path).err().expect("refused");
        assert!(said.ends_with("is in the way of the control socket."), "{said}");
        assert_eq!(std::fs::read_to_string(&path).expect("still there"), "kept");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_socket_path_too_long_is_refused_and_nothing_is_made() {
        let folder = socket_folder("long").join("a".repeat(100));
        let said = echo_at(&folder.join("c.sock")).err().expect("refused");
        assert!(said.starts_with("The control socket's path is too long"), "{said}");
        assert!(!folder.exists());
    }

    /// A folder that is really a link to another: on Unix a symbolic link, on Windows a
    /// junction (which needs no special right to make).
    fn link_to_folder(link: &Path, to: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(to, link).expect("a link");
        #[cfg(windows)]
        run(tool("cmd.exe").expect("cmd").arg("/C").arg("mklink").arg("/J").arg(link).arg(to)).expect("a junction");
    }

    #[test]
    fn a_folder_that_is_a_link_is_refused() {
        let base = socket_folder("link");
        let real = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(&real).expect("the folder");
        link_to_folder(&link, &real);
        let said = echo_at(&link.join("c.sock")).err().expect("refused");
        assert!(said.ends_with("is not a folder."), "{said}");
        assert!(std::fs::symlink_metadata(real.join("c.sock")).is_err(), "no socket was made through the link");
        // The link first, so that removing the rest never goes through it.
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn the_sockets_folder_and_the_socket_are_for_this_user_only() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::symlink_metadata(path).expect("there").permissions().mode() & 0o777;
        // A folder made for the socket, and one that was there already and open to others.
        let made = socket_folder("made");
        let open = socket_folder("open");
        std::fs::create_dir_all(&open).expect("the folder");
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).expect("opened");
        for folder in [&made, &open] {
            let path = folder.join("c.sock");
            let _listening = echo_at(&path).expect("listening");
            assert_eq!(mode(folder), 0o700);
            assert_eq!(mode(&path), 0o600);
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// One of Windows' own tools, from its own folder, run without a console window of its own.
    #[cfg(windows)]
    fn tool(name: &str) -> Result<std::process::Command, String> {
        use std::os::windows::process::CommandExt;
        const NO_WINDOW: u32 = 0x0800_0000;
        let root = std::env::var_os("SystemRoot")
            .filter(|root| !root.is_empty())
            .ok_or("could not find Windows' own folder (SystemRoot is not set)")?;
        let mut tool = std::process::Command::new(Path::new(&root).join("System32").join(name));
        tool.creation_flags(NO_WINDOW).stdin(std::process::Stdio::null());
        Ok(tool)
    }

    /// Runs a tool and gives what it printed; an error says what it printed when it failed.
    #[cfg(windows)]
    fn run(tool: &mut std::process::Command) -> Result<String, String> {
        let name = tool.get_program().to_string_lossy().into_owned();
        let output = tool.output().map_err(|e| format!("could not run {name}: {e}"))?;
        let printed = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let problem = String::from_utf8_lossy(&output.stderr);
            return Err(format!("{name} failed: {} {}", printed.trim(), problem.trim()).trim().to_owned());
        }
        Ok(printed)
    }

    /// Changes an access list with Windows' own tool, as somebody at the computer could.
    #[cfg(windows)]
    fn icacls(path: &Path, change: &[&str]) {
        run(tool("icacls.exe").expect("icacls").arg(path).args(change)).expect("the access list changed");
    }

    /// The socket kind itself works on this Windows, before anything is kept private: when the
    /// tests below fail, this says whether the socket or the privacy is at fault.
    #[cfg(windows)]
    #[test]
    fn windows_has_the_socket_kind() {
        let folder = socket_folder("kind");
        let path = folder.join("c.sock");
        std::fs::create_dir_all(&folder).expect("the folder");
        let listener = UnixListener::bind(&path).expect("bound");
        let meta = std::fs::symlink_metadata(&path).expect("the socket file can be looked at");
        assert!(private_folder::is_socket(&meta), "the socket file is known as a socket");
        let client = std::thread::spawn({
            let path = path.clone();
            move || {
                let mut stream = UnixStream::connect(&path).expect("connected");
                stream.write_all(b"hello\n").expect("sent");
            }
        });
        let (stream, _) = listener.accept().expect("accepted");
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).expect("read");
        assert_eq!(line, "hello\n");
        client.join().expect("the client");
        drop(listener);
        std::fs::remove_file(&path).expect("the socket file can be removed");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Each step of keeping a folder private, one at a time, so a failure names the step: for a
    /// folder made for the socket, and for one made the ordinary way beforehand, which takes on
    /// the list of the folder above until it is kept private.
    #[cfg(windows)]
    #[test]
    fn windows_keeps_the_sockets_folder_to_this_user() {
        let user = sp_winapi::current_user().expect("which user this is");
        assert!(user.starts_with("S-1-"), "{user}");
        let made = socket_folder("kept");
        let ordinary = socket_folder("plain");
        std::fs::create_dir_all(&ordinary).expect("a folder made the ordinary way");
        for folder in [&made, &ordinary] {
            let kept = private_folder::keep_private(folder).expect("kept private");
            let security = sp_winapi::security_of(folder).expect("its owner and access list");
            assert_eq!(security.owner.as_deref(), Some(user.as_str()), "it belongs to this user");
            let list = security.list;
            assert!(list.starts_with("D:P"), "closed to the folder above: {list}");
            assert!(!list.contains(";WD)") && !list.contains(";BU)") && !list.contains(";AU)"), "{list}");
            // While it is kept private nobody can rename it (and so put another in its place);
            // once it is let go, it can be, which shows the refusal came from holding it.
            let moved = folder.with_extension("moved");
            assert!(std::fs::rename(folder, &moved).is_err(), "a folder being kept private can't be renamed");
            drop(kept);
            std::fs::rename(folder, &moved).expect("renamed once let go");
            std::fs::rename(&moved, folder).expect("renamed back");
            // A file made in it takes the same list.
            let inside = folder.join("inside.txt");
            std::fs::write(&inside, "x").expect("a file inside");
            let list = sp_winapi::security_of(&inside).expect("the file's access list").list;
            assert!(!list.contains(";WD)") && !list.contains(";BU)") && !list.contains(";AU)"), "{list}");
            // Kept private again, as at the next start: nothing is left in the folder but the
            // file made above.
            private_folder::keep_private(folder).expect("kept private again");
            let names: Vec<String> = std::fs::read_dir(folder)
                .expect("the folder")
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(names, ["inside.txt"], "nothing else is left in the folder");
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_does_not_listen_in_a_folder_others_may_open() {
        let folder = socket_folder("shared");
        let path = folder.join("c.sock");
        std::fs::create_dir_all(&folder).expect("the folder");
        // Everyone (S-1-1-0) is let in by an entry of the folder's own, which stays when what
        // comes from the folder above is taken off.
        icacls(&folder, &["/grant", "*S-1-1-0:(OI)(CI)F"]);
        let said = echo_at(&path).err().expect("refused");
        assert!(said.contains("is not private"), "{said}");
        assert!(said.contains("Delete that folder and start SoftPresenter again"), "it says what to do: {said}");
        assert!(std::fs::symlink_metadata(&path).is_err(), "no socket was made");
        let left: Vec<_> =
            std::fs::read_dir(&folder).expect("the folder").flatten().map(|entry| entry.path()).collect();
        assert!(left.is_empty(), "nothing was left in a folder that is not private: {left:?}");
        // Doing what it says works: the folder made afresh is private.
        std::fs::remove_dir_all(&folder).expect("the folder deleted");
        let _listening = echo_at(&path).expect("listening in the folder made afresh");
        assert!(say_to(&path, &["status"]).expect("a reply").ok);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A socket that was already there keeps its own access list, whatever the folder's is. If
    /// this fails with "Another SoftPresenter is already listening", the socket's list was not
    /// read before the socket was tried, or Windows did not put Everyone on a socket file's
    /// list when asked.
    #[cfg(windows)]
    #[test]
    fn windows_does_not_take_over_a_socket_others_may_open() {
        let folder = socket_folder("openfile");
        let path = folder.join("c.sock");
        let _first = echo_at(&path).expect("listening");
        // Everyone (S-1-1-0) is let in to the socket file itself; the folder stays private.
        icacls(&path, &["/grant", "*S-1-1-0:F", "/L"]);
        let said = echo_at(&path).err().expect("refused");
        assert!(said.contains("is in the way of the control socket: it is not private"), "{said}");
        assert!(std::fs::symlink_metadata(&path).is_ok(), "the socket was left where it was");
        assert!(say_to(&path, &["status"]).expect("the first still answers").ok);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Whether this test program runs with an administrator's full token (high rights), as it
    /// does over ssh to an administrator's account. Windows lets such a program connect to any
    /// socket, whatever the socket file's access list or label say, so a test of what those keep
    /// out must not be run from one: it runs its program through `runas` without those rights.
    #[cfg(windows)]
    fn runs_elevated() -> bool {
        let groups = run(tool("whoami.exe").expect("whoami").args(["/groups", "/fo", "csv"]));
        groups.is_ok_and(|groups| groups.contains("S-1-16-12288"))
    }

    /// Runs a program of this user with the rights an ordinary program has and gives what it
    /// printed, `set` being the environment variables it is given. From a program of an
    /// ordinary user it is run as it is; from an administrator's full token, through `runas
    /// /trustlevel:0x20000` (a basic user: the administrators' group and the privileges taken
    /// away), which prints nothing itself, so the program writes to a file that is read once it
    /// says it has finished. (`runas` opens a console window for the program, which is why this
    /// is done only from a full administrator's token: over ssh, where there is no screen, and
    /// in the automatic checks on GitHub's Windows, whose tests have that token too.) Each run
    /// has a folder of its own for its script and what the program wrote, never used again:
    /// the tests run side by side, and while they shared one, each could read the other's
    /// helper (the low-rights helper's words were taken for the refused user's, and the other
    /// way round), or what the last run wrote, when a helper still running kept the folder
    /// from being emptied.
    #[cfg(windows)]
    fn run_ordinary(program: &Path, args: &[&str], set: &[(&str, &Path)]) -> Result<String, String> {
        static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if !runs_elevated() {
            let mut run = std::process::Command::new(program);
            run.args(args);
            for (name, value) in set {
                run.env(name, value);
            }
            let ran = run.output().map_err(|e| e.to_string())?;
            return Ok(format!("{}\n{}", String::from_utf8_lossy(&ran.stdout), String::from_utf8_lossy(&ran.stderr)));
        }
        let this_run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let scratch = socket_folder(&format!("ordinary{this_run}"));
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        let (script, out) = (scratch.join("run.cmd"), scratch.join("out.txt"));
        let mut text = String::from("@echo off\r\n");
        for (name, value) in set {
            text += &format!("set {name}={}\r\n", value.display());
        }
        text += &format!(
            "\"{}\" {} > \"{}\" 2>&1\r\necho finished>> \"{}\"\r\n",
            program.display(),
            args.join(" "),
            out.display(),
            out.display()
        );
        let result = std::fs::write(&script, text).map_err(|e| e.to_string()).and_then(|()| {
            run(tool("runas.exe")?.arg("/trustlevel:0x20000").arg(&script))?;
            let waiting = std::time::Instant::now();
            loop {
                let said = std::fs::read_to_string(&out).unwrap_or_default();
                if said.contains("finished") {
                    return Ok(said);
                }
                if waiting.elapsed() > Duration::from_secs(60) {
                    return Err(format!("the program run through runas did not finish: {said}"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let _ = std::fs::remove_dir_all(&scratch);
        result
    }

    /// Not a test of its own: what a test below runs a copy of this test program to do, to say
    /// whether a program of ordinary rights can connect to the socket named.
    #[cfg(windows)]
    #[test]
    fn windows_connect_helper() {
        let Some(path) = std::env::var_os("SP_TEST_CONNECT") else { return };
        match UnixStream::connect(Path::new(&path)) {
            Ok(_) => println!("helper connect: connected"),
            Err(problem) => println!("helper connect: refused ({problem})"),
        }
    }

    /// Not a test of its own: what the test below runs a copy of this test program to do, with
    /// low rights. It says how it was run and whether it could connect to each socket named.
    #[cfg(windows)]
    #[test]
    fn windows_low_rights_helper() {
        let (Some(private), Some(open)) =
            (std::env::var_os("SP_TEST_LOW_PRIVATE"), std::env::var_os("SP_TEST_LOW_OPEN"))
        else {
            return;
        };
        // S-1-16-4096 is the label Windows gives a program run with low rights.
        let groups = run(tool("whoami.exe").expect("whoami").args(["/groups", "/fo", "csv"]));
        let level = match groups {
            Ok(groups) if groups.contains("S-1-16-4096") => "low".to_owned(),
            Ok(_) => "not low".to_owned(),
            Err(problem) => format!("unknown ({problem})"),
        };
        println!("helper level: {level}");
        for (name, path) in [("private", private), ("open", open)] {
            match UnixStream::connect(Path::new(&path)) {
                Ok(_) => println!("helper {name}: connected"),
                Err(problem) => println!("helper {name}: refused ({problem})"),
            }
        }
    }

    /// Whether a program run by the same user with low rights (low integrity: a browser's
    /// sandboxed part, say) can reach the control socket. The app sets nothing against it;
    /// Windows by itself lets such a program read the user's files but not write to them, and
    /// connecting to a socket is taken here to need writing.
    ///
    /// A copy of this test program, marked to run with low rights (and without an administrator's
    /// rights, if this test program has them: see `run_ordinary`), tries two sockets: the
    /// control socket as the app makes it, and one marked as open to low-rights programs, to
    /// show that such a program can connect at all.
    #[cfg(windows)]
    #[test]
    fn windows_keeps_a_low_rights_program_of_the_same_user_out() {
        let folder = socket_folder("low");
        let private = folder.join("c.sock");
        let _listening = echo_at(&private).expect("listening");
        let open_folder = socket_folder("lowopen");
        let open = open_folder.join("c.sock");
        let _open_listening = echo_at(&open).expect("listening");
        icacls(&open_folder, &["/setintegritylevel", "(OI)(CI)Low"]);
        icacls(&open, &["/setintegritylevel", "Low", "/L"]);
        // The copy sits outside both folders, where a low-rights program may read it.
        let programs = socket_folder("lowexe");
        std::fs::create_dir_all(&programs).expect("a folder for the copy");
        let copy = programs.join("low.exe");
        std::fs::copy(std::env::current_exe().expect("this test program"), &copy).expect("a copy of this test program");
        icacls(&copy, &["/setintegritylevel", "Low"]);
        let helper = ["control::tests::windows_low_rights_helper", "--exact", "--nocapture", "--test-threads=1"];
        let ran = run_ordinary(&copy, &helper, &[("SP_TEST_LOW_PRIVATE", &private), ("SP_TEST_LOW_OPEN", &open)]);
        let _ = std::fs::remove_dir_all(&programs);
        let said = ran.expect("the copy ran");
        println!("{said}");
        assert!(
            said.contains("helper level: low"),
            "NOTHING LEARNT: the copy did not run with low rights, so this says nothing about such programs. It said:\n{said}"
        );
        assert!(
            said.contains("helper open: connected"),
            "NOTHING LEARNT: the low-rights copy could not connect even to a socket marked as open to it, so its \
             being refused at the control socket would prove nothing. It said:\n{said}"
        );
        assert!(
            said.contains("helper private: refused"),
            "LEARNT: a low-rights program of the same user CAN connect to the control socket. Windows does not keep \
             it out by itself, and nothing the app sets does either; keeping it out would need a label on the folder \
             that refuses reading as well as writing from below. It said:\n{said}"
        );
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&open_folder);
    }

    /// The whole of the privacy rests on this: Windows lets a program connect only if the
    /// socket file's access list lets its user open the file.
    #[cfg(windows)]
    #[test]
    fn windows_refuses_a_user_the_socket_file_does_not_let_in() {
        let folder = socket_folder("deny");
        let path = folder.join("c.sock");
        let _listening = echo_at(&path).expect("listening");
        // Asked by a program of ordinary rights, as `sp` is, and not by this test program, which
        // an administrator's full token would let in whatever the list says.
        let this = std::env::current_exe().expect("this test program");
        let helper = ["control::tests::windows_connect_helper", "--exact", "--nocapture", "--test-threads=1"];
        let asks = |path: &Path| {
            let said = run_ordinary(&this, &helper, &[("SP_TEST_CONNECT", path)]).expect("the helper ran");
            assert!(
                said.contains("helper connect: "),
                "NOTHING LEARNT: what was read back is not the connect helper's, so it says nothing about the \
                 socket. It said:\n{said}"
            );
            said
        };
        let first = asks(&path);
        assert!(first.contains("helper connect: connected"), "let in at first: {first}");
        let user = sp_winapi::current_user().expect("which user this is");
        icacls(&path, &["/deny", &format!("*{user}:F"), "/L"]);
        let said = asks(&path);
        assert!(said.contains("helper connect: refused"), "refused once the file's list refuses this user: {said}");
        icacls(&path, &["/remove:d", &format!("*{user}"), "/L"]);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
