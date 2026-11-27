//! The videos' FFmpeg, installed in the user's own folder on Windows. The Windows package has
//! `ffmpeg.exe` and `ffprobe.exe` beside `SoftPresenter.exe`, but a tester ran the program on its
//! own from the Desktop, and no video would play. So on Windows FFmpeg's home is
//! `%LOCALAPPDATA%\SoftPresenter\ffmpeg`, wherever the program runs from: at every start a thread
//! of its own checks that the folder holds the app's own FFmpeg 9.0.2 (the SHA-256 of
//! `ffmpeg.exe` and `ffprobe.exe`), and where it doesn't, copies it there from beside the program
//! (no network), or, where it isn't there either, fetches the same build from the project's
//! GitHub release, checking the file's size and SHA-256 before anything is unpacked. Once the
//! folder is checked, the lookup (`sp_video`) runs that copy first. FFmpeg's licence and the
//! note of its source go with the programs. Nothing is ever written beside the program.
//!
//! A video added or played while FFmpeg is missing starts a fetch too. All of it runs on a
//! thread of its own, never the window's or the screens', and one at a time. No endless tries:
//! after a failed fetch, another is made only at the next start, or when the operator adds
//! another video. A test copy never fetches, unless its testing switch points it at a server on
//! this computer.

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::i18n::{t, tf, tn};

/// SoftPresenter's own build of FFmpeg for Windows, the one the Windows package carries, as the
/// project's releases publish it.
pub const URL: &str =
    "https://github.com/esraell/SoftPresenter/releases/download/ffmpeg-windows-9.0.2/ffmpeg-9.0.2-windows-x86_64.zip";
/// Its size, to the byte.
pub const SIZE: u64 = 23_359_893;
/// Its SHA-256.
pub const SHA256: &str = "ee896ed83430f5b6c86d88db7b988d95d1859d21bd2c7f656c76198e56d2f11d";
/// The SHA-256 of the two programs in it (and beside the program in the package): the app's
/// own FFmpeg.
pub const FFMPEG_SHA256: &str = "4e0ef74ad23d04313c9ad4a5b51c35f81f905a51922340b059c234259192f485";
pub const FFPROBE_SHA256: &str = "afa58277c8db57dd3ba781ea818f6662c1d608fce74ef332e1bc29084dc18c18";

/// The two programs, by their names in the folder and beside the program; in the zip they are
/// in `ffmpeg/`.
const PROGRAMS: [&str; 2] = ["ffmpeg.exe", "ffprobe.exe"];

/// FFmpeg's licence and the note of its source, kept with the programs: their names in the
/// folder (in the zip they are in `ffmpeg/`), and in the package's `Licences` folder beside the
/// program.
const NOTES: [(&str, &str); 3] = [
    ("COPYING.LGPLv2.1", "Licences/COPYING.LGPLv2.1"),
    ("LICENSE.md", "Licences/FFmpeg-LICENSE.md"),
    ("SOURCE.txt", "Licences/FFmpeg-SOURCE.txt"),
];

/// The most any one file may be (`ffmpeg.exe` is 28 MB).
const LARGEST_FILE: u64 = 128 * 1024 * 1024;

/// The folder the programs are kept in, and the one a copy or a fetch works in, inside the base
/// folder.
const FOLDER: &str = "ffmpeg";
const PART: &str = "ffmpeg.part";

/// Where the app's own FFmpeg is kept, what it must be, and where it is copied or fetched from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The zip fetched when FFmpeg isn't here: its address, its size to the byte (nothing
    /// longer is read) and its SHA-256, in lower-case hex.
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// The SHA-256 of `ffmpeg.exe` and `ffprobe.exe`.
    pub programs: [String; 2],
    /// The folder `ffmpeg` (the programs) and `ffmpeg.part` (a copy or a fetch under way) go
    /// in: `%LOCALAPPDATA%\SoftPresenter`, or a test copy's data folder.
    pub base: PathBuf,
    /// The program's own folder, where the package puts FFmpeg: copied from there when it holds
    /// the app's own. Only ever read.
    pub beside: Option<PathBuf>,
    /// A test server on this computer, where plain http is allowed.
    pub local: bool,
}

impl Plan {
    /// The folder the programs are kept in.
    pub fn folder(&self) -> PathBuf {
        self.base.join(FOLDER)
    }

    /// The folder a copy or a fetch works in, gone once it is over.
    fn part(&self) -> PathBuf {
        self.base.join(PART)
    }
}

/// Whether this copy keeps FFmpeg in a folder of its own, where, and where it comes from. Only
/// on Windows, and never in a test copy: one run with `SOFTPRESENTER_QUIET`, with a data folder
/// of its own (`SOFTPRESENTER_DATA`), or only to save a picture (`quiet_run`). The one exception
/// is the testing aid below, on any system.
fn plan_from(
    quiet_run: bool,
    windows: bool,
    beside: Option<PathBuf>,
    var: impl Fn(&str) -> Option<String>,
) -> Option<Plan> {
    let var = |name: &str| var(name).filter(|value| !value.trim().is_empty());
    let own = || [FFMPEG_SHA256.to_owned(), FFPROBE_SHA256.to_owned()];
    // Testing aid: SOFTPRESENTER_FFMPEG_FETCH_URL=http://127.0.0.1:<port>/<file> keeps FFmpeg in
    // the test copy's own data folder (`ffmpeg/` there), on any system, copied from beside the
    // program or fetched from a server on this computer. Honoured only together with
    // SOFTPRESENTER_DATA, and only for http://127.0.0.1 or http://localhost; set to anything
    // else, nothing is copied or fetched at all. SOFTPRESENTER_FFMPEG_FETCH_EXPECT=
    // <bytes>:<zip sha256>:<ffmpeg.exe sha256>:<ffprobe.exe sha256> is what the zip and the
    // programs must be instead of the real ones (a test's own), honoured only with it.
    if let Some(url) = var("SOFTPRESENTER_FFMPEG_FETCH_URL") {
        let url = url.trim().to_owned();
        let data = var("SOFTPRESENTER_DATA")?;
        if !crate::free_songbooks::is_local(&url) {
            return None;
        }
        let (size, sha256, programs) = match var("SOFTPRESENTER_FFMPEG_FETCH_EXPECT") {
            Some(expect) => expected(&expect)?,
            None => (SIZE, SHA256.to_owned(), own()),
        };
        return Some(Plan { url, size, sha256, programs, base: PathBuf::from(data), beside, local: true });
    }
    if !windows || quiet_run || var("SOFTPRESENTER_QUIET").is_some() || var("SOFTPRESENTER_DATA").is_some() {
        return None;
    }
    let local = var("LOCALAPPDATA")?;
    Some(Plan {
        url: URL.to_owned(),
        size: SIZE,
        sha256: SHA256.to_owned(),
        programs: own(),
        base: PathBuf::from(local).join("SoftPresenter"),
        beside,
        local: false,
    })
}

/// `<bytes>:<zip sha256>:<ffmpeg.exe sha256>:<ffprobe.exe sha256>`, as the testing aid gives it.
fn expected(text: &str) -> Option<(u64, String, [String; 2])> {
    let words: Vec<String> = text.trim().split(':').map(|word| word.trim().to_ascii_lowercase()).collect();
    let [size, zip, ffmpeg, ffprobe] = <[String; 4]>::try_from(words).ok()?;
    let sums = [zip, ffmpeg, ffprobe];
    sums.iter().all(|sum| sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit())).then_some(())?;
    let [zip, ffmpeg, ffprobe] = sums;
    Some((size.parse().ok()?, zip, [ffmpeg, ffprobe]))
}

static PLAN: OnceLock<Option<Plan>> = OnceLock::new();

/// Works out, once, as the app starts and before anything looks for FFmpeg, whether this copy
/// keeps FFmpeg in a folder of its own, and tells the lookup where.
pub fn prepare(quiet_run: bool) {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    let plan = PLAN.get_or_init(|| plan_from(quiet_run, cfg!(windows), beside, |name| std::env::var(name).ok()));
    if let Some(plan) = plan {
        sp_video::set_fetched_ffmpeg_folder(plan.folder());
    }
}

/// This copy's plan: none until [`prepare`] (so a test never fetches), or where it keeps none.
fn plan() -> Option<&'static Plan> {
    PLAN.get().and_then(Option::as_ref)
}

/// The size of the file as the operator is told it, in whole megabytes: "23 MB".
pub fn size_words(bytes: u64) -> String {
    let megabytes = (bytes.saturating_add(500_000) / 1_000_000).max(1);
    tf("freesongbooks.size", "{size} MB", &[("size", &crate::i18n::count(megabytes))])
}

/// This copy's file's size as the operator is told it.
pub fn planned_size() -> String {
    size_words(plan().map_or(SIZE, |plan| plan.size))
}

// ---------------------------------------------------------------------------------------------
// One at a time

/// Where fetching stands in this run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// None has been made.
    Idle,
    Fetching,
    /// The last one put FFmpeg in place.
    Ready,
    /// The last one failed, and why, in the operator's words.
    Failed(String),
}

/// What started a look, a copy or a fetch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The app started.
    Start,
    /// The operator added a video, and FFmpeg is missing.
    Added,
    /// A video was to play, and FFmpeg is missing.
    Played,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    /// How many fetches have begun in this run, and how many have ended.
    pub begun: u64,
    pub ended: u64,
    /// FFmpeg was missing as the last fetch began, so videos couldn't play: only then is the
    /// operator told of it (one made while another FFmpeg plays videos is said in the log only).
    pub needed: bool,
}

impl Status {
    /// Whether a fetch may begin now, for this reason: never while one is under way, and after
    /// one has failed in this run only for a video the operator adds (or at the next start).
    fn may_begin(&self, why: Why) -> bool {
        match self.phase {
            Phase::Fetching => false,
            Phase::Failed(_) => why == Why::Added,
            Phase::Idle | Phase::Ready => true,
        }
    }
}

static STATUS: Mutex<Status> = Mutex::new(Status { phase: Phase::Idle, begun: 0, ended: 0, needed: false });

/// Held for the whole of a look, a copy or a fetch, so only one works on the folder at a time.
static WORK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, Status> {
    STATUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where fetching stands now. Never waits on a fetch.
pub fn status() -> Status {
    lock().clone()
}

/// True while FFmpeg is being fetched.
pub fn under_way() -> bool {
    lock().phase == Phase::Fetching
}

/// Marks a fetch begun, if one may begin for this reason: whether it was.
fn mark_begun(why: Why, needed: bool) -> bool {
    let mut status = lock();
    if !status.may_begin(why) {
        return false;
    }
    status.phase = Phase::Fetching;
    status.begun += 1;
    status.needed = needed;
    true
}

/// Keeps how a fetch went, and marks it over.
fn mark_ended(result: &Result<(), String>) {
    let mut status = lock();
    status.phase = match result {
        Ok(()) => Phase::Ready,
        Err(why) => Phase::Failed(why.clone()),
    };
    status.ended += 1;
}

/// Called on the work's thread when a fetch begins and when it ends.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Starts the work on a thread of its own: at start ([`Why::Start`]), the folder is checked and,
/// where it must be, FFmpeg copied or fetched; for a video added or played while FFmpeg is
/// missing, the same, marked as a fetch at once (if one may begin), so the notices can say it is
/// being fetched. `wake` is called on that thread as a fetch begins and once it is over
/// ([`status`] then has it). Whether the work began.
fn begin(plan: &Plan, why: Why, wake: Wake) -> bool {
    let marked = why != Why::Start;
    if marked && !mark_begun(why, true) {
        return false;
    }
    let plan = plan.clone();
    let woken = wake.clone();
    let spawned = std::thread::Builder::new().name("install FFmpeg".to_owned()).spawn(move || {
        run(&plan, why, marked, &woken);
    });
    if let Err(error) = spawned {
        let why =
            tf("media.ffmpeg.fetch.no_thread", "it couldn't be started ({error})", &[("error", &error.to_string())]);
        crate::applog::event("video", &format!("FFmpeg was not installed: {}", crate::i18n::english_of(&why)));
        if marked {
            mark_ended(&Err(why));
            wake();
        }
        return false;
    }
    true
}

/// The work itself, on its own thread: see [`begin`]. The log says what became of the folder,
/// and which FFmpeg the app now runs.
fn run(plan: &Plan, why: Why, marked: bool, wake: &Wake) {
    let _work = WORK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if marked {
        wake();
    }
    let say = |line: &str| crate::applog::event("video", line);
    let needed = !sp_video::ffmpeg_found();
    let mut began = marked;
    let mut may_fetch = || {
        if !began && mark_begun(why, needed) {
            began = true;
            wake();
        }
        began
    };
    // Work that panics still ends, so a fetch never stays "under way" for the rest of the run.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| install(plan, &say, &mut may_fetch)))
        .unwrap_or_else(|_| {
            Err(tf("media.ffmpeg.fetch.stopped", "the download stopped ({error})", &[("error", "panic")]))
        });
    if let Ok(Some(how)) = &result {
        sp_video::set_fetched_ffmpeg_checked(true);
        say(&format!("FFmpeg installed at {} ({})", plan.folder().display(), how.words()));
    }
    match sp_video::ffmpeg_program() {
        Some(used) => say(&format!("FFmpeg used: {}", used.display())),
        None => say("FFmpeg used: none (it isn't on this computer)"),
    }
    if began {
        mark_ended(&result.map(|_| ()));
        wake();
    }
}

/// How the folder came to hold the app's own FFmpeg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// It held it already.
    Already,
    /// Copied from beside the program.
    Copied,
    Fetched,
}

impl How {
    /// As the log says it.
    fn words(self) -> &'static str {
        match self {
            How::Already => "there already, checked",
            How::Copied => "copied from the package",
            How::Fetched => "fetched",
        }
    }
}

/// Makes sure the folder holds the app's own FFmpeg (both programs' SHA-256): as it is, or
/// copied from beside the program when the copy there is the app's own, or else fetched, if
/// `may_fetch` lets a fetch begin (none: it didn't). Each step is said in `say` (the log); the
/// error is in the operator's words. Blocks for as long as the copy or the download takes:
/// never on the window's or the screens' thread.
pub fn install(plan: &Plan, say: &dyn Fn(&str), may_fetch: &mut dyn FnMut() -> bool) -> Result<Option<How>, String> {
    if holds_own(&plan.folder(), plan) {
        return Ok(Some(How::Already));
    }
    if let Some(beside) = plan.beside.as_deref()
        && PROGRAMS.iter().all(|name| beside.join(name).is_file())
    {
        match copy_in(beside, plan) {
            Ok(()) => return Ok(Some(How::Copied)),
            Err(why) => say(&format!("FFmpeg beside the program ({}) wasn't copied: {why}", beside.display())),
        }
    }
    if !may_fetch() {
        return Ok(None);
    }
    fetch(plan, say).map(|_| Some(How::Fetched))
}

/// True when `folder` holds both programs, each with the SHA-256 expected.
fn holds_own(folder: &Path, plan: &Plan) -> bool {
    PROGRAMS.iter().zip(&plan.programs).all(|(name, sum)| sha256_of(&folder.join(name)).is_ok_and(|have| have == *sum))
}

/// A file's SHA-256, in lower-case hex.
fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut chunk = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            return Ok(hex(hash));
        }
        hash.update(&chunk[..read]);
    }
}

fn hex(hash: Sha256) -> String {
    hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Copies at most `LARGEST_FILE` bytes and one more from `from` into a new file `to`, and says
/// how many bytes, and their SHA-256.
fn copy_hashing(from: &mut dyn Read, to: &Path) -> std::io::Result<(u64, String)> {
    let mut out = BufWriter::new(std::fs::File::create(to)?);
    let mut from = from.take(LARGEST_FILE + 1);
    let mut hash = Sha256::new();
    let mut total = 0_u64;
    let mut chunk = vec![0_u8; 256 * 1024];
    loop {
        let read = from.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
        out.write_all(&chunk[..read])?;
        total += read as u64;
    }
    out.flush()?;
    Ok((total, hex(hash)))
}

/// A temporary folder of its own: what a copy or a fetch a quit or a crash cut short left there
/// goes first.
fn fresh(part: &Path) -> std::io::Result<()> {
    if part.symlink_metadata().is_ok() {
        std::fs::remove_dir_all(part)?;
    }
    std::fs::create_dir_all(part)
}

/// Copies the app's own FFmpeg from beside the program, with its licence and the note of its
/// source from the package's `Licences` where they are, into the temporary folder, checks each
/// program's SHA-256 as it is copied, and renames the copy into place. Nothing beside the program
/// is changed. The error is for the log.
fn copy_in(beside: &Path, plan: &Plan) -> Result<(), String> {
    let part = plan.part();
    let copied = || -> Result<(), String> {
        fresh(&part).map_err(|e| e.to_string())?;
        let copy = part.join(FOLDER);
        std::fs::create_dir_all(&copy).map_err(|e| e.to_string())?;
        for (name, sum) in PROGRAMS.iter().zip(&plan.programs) {
            let mut from = std::fs::File::open(beside.join(name)).map_err(|e| format!("{name}: {e}"))?;
            let (_, have) = copy_hashing(&mut from, &copy.join(name)).map_err(|e| format!("{name}: {e}"))?;
            if have != *sum {
                return Err(format!("{name} isn't the app's own FFmpeg (its SHA-256 is {have})"));
            }
        }
        for (name, packaged) in NOTES {
            let from = beside.join(packaged);
            if from.is_file() {
                std::fs::copy(&from, copy.join(name)).map_err(|e| format!("{packaged}: {e}"))?;
            }
        }
        put_in_place(&copy, &plan.folder(), &part).map_err(|e| e.to_string())
    };
    let result = copied();
    let _ = std::fs::remove_dir_all(&part);
    result
}

// ---------------------------------------------------------------------------------------------
// The fetch

/// Fetches FFmpeg as `plan` says and puts it in place: the file downloaded into a temporary
/// folder beside the programs' (`ffmpeg.part`), its size and SHA-256 checked before anything is
/// unpacked, the five files expected taken from it into a folder of their own there (the
/// programs checked by their own SHA-256 too), and that folder renamed into place, in place of a
/// broken or outdated one. The temporary folder goes whatever happens. Each step is said in
/// `say` (the log); the error is in the operator's words.
pub fn fetch(plan: &Plan, say: &dyn Fn(&str)) -> Result<PathBuf, String> {
    say(&format!("fetching FFmpeg from {}\u{2026}", plan.url));
    let part = plan.part();
    let result = fetch_into(plan, &part, say);
    let _ = std::fs::remove_dir_all(&part);
    match &result {
        Ok(folder) => say(&format!("FFmpeg put in place at {}", folder.display())),
        Err(why) => say(&format!("FFmpeg was not fetched: {}", crate::i18n::english_of(why))),
    }
    result
}

fn not_saved(error: std::io::Error) -> String {
    tf(
        "media.ffmpeg.fetch.not_saved",
        "it couldn't be saved on this computer ({error})",
        &[("error", &error.to_string())],
    )
}

fn fetch_into(plan: &Plan, part: &Path, say: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let lower = plan.url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || plan.local && crate::free_songbooks::is_local(&plan.url)) {
        return Err(t("media.ffmpeg.fetch.not_https", "its address isn't a secure (https) one").to_owned());
    }
    fresh(part).map_err(not_saved)?;
    let download = part.join("download.zip");
    let sum = download_to(plan, &download)?;
    if sum != plan.sha256 {
        return Err(changed());
    }
    say(&format!("FFmpeg fetched and checked: {} bytes, SHA-256 {sum} as expected", plan.size));
    let unpacked = part.join(FOLDER);
    unpack(&download, &unpacked, plan)?;
    let folder = plan.folder();
    put_in_place(&unpacked, &folder, part).map_err(not_saved)?;
    Ok(folder)
}

/// What is said of a file that isn't the one this version expects.
fn changed() -> String {
    t("media.ffmpeg.fetch.changed", "the file isn't the one this version of SoftPresenter expects").to_owned()
}

/// Downloads the file into `to`, reading nothing past its expected size, and returns its
/// SHA-256 (hex). A file of another size is refused.
fn download_to(plan: &Plan, to: &Path) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        // 23 MB over a slow church connection may take a while.
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        // GitHub sends the file from elsewhere: every step must be https too (but for a test
        // server on this computer).
        .https_only(!plan.local)
        .max_redirects(5)
        .user_agent(format!("SoftPresenter/{}", crate::updates::VERSION))
        .build()
        .into();
    let mut response = agent.get(&plan.url).header("Accept-Encoding", "identity").call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => {
            tf("media.ffmpeg.fetch.answered", "the server answered {code}", &[("code", &code.to_string())])
        }
        other => tf(
            "media.ffmpeg.fetch.unreachable",
            "the server couldn't be reached ({error})",
            &[("error", &other.to_string())],
        ),
    })?;
    let said = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    // A file of another size is another file: nothing of it is read.
    if said.is_some_and(|said| said != plan.size) {
        return Err(changed());
    }
    let mut file = BufWriter::new(std::fs::File::create(to).map_err(not_saved)?);
    // One byte past the size is let through, so a longer file is caught here, in plain words.
    let mut reader = response.body_mut().with_config().limit(plan.size.saturating_add(1)).reader();
    let mut hash = Sha256::new();
    let mut total: u64 = 0;
    let mut chunk = vec![0_u8; 256 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|e| {
            tf("media.ffmpeg.fetch.stopped", "the download stopped ({error})", &[("error", &e.to_string())])
        })?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > plan.size {
            return Err(changed());
        }
        hash.update(&chunk[..read]);
        file.write_all(&chunk[..read]).map_err(not_saved)?;
    }
    file.flush().map_err(not_saved)?;
    if total != plan.size {
        return Err(t("media.ffmpeg.fetch.cut_short", "the download was cut short").to_owned());
    }
    Ok(hex(hash))
}

/// Takes the five files expected from the zip, by their exact names, into `into` (made here):
/// any other entry, a path that climbs out (`../`) or a folder, is never read. Each is checked
/// as it is read (the zip's own CRC, and the programs' SHA-256), and none may be larger than
/// [`LARGEST_FILE`].
fn unpack(zip_path: &Path, into: &Path, plan: &Plan) -> Result<(), String> {
    let unreadable = |error: &dyn std::fmt::Display| {
        tf("media.ffmpeg.fetch.unpacked", "the file couldn't be unpacked ({error})", &[("error", &error.to_string())])
    };
    let file = std::fs::File::open(zip_path).map_err(|e| unreadable(&e))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| unreadable(&e))?;
    std::fs::create_dir_all(into).map_err(|e| unreadable(&e))?;
    let sums = plan.programs.iter().map(Some);
    let names = PROGRAMS.iter().zip(sums).chain(NOTES.iter().map(|(name, _)| name).zip(std::iter::repeat(None)));
    for (name, sum) in names {
        let inside = format!("{FOLDER}/{name}");
        let mut entry = zip.by_name(&inside).map_err(|e| unreadable(&format!("{inside}: {e}")))?;
        if entry.is_dir() || entry.size() > LARGEST_FILE {
            return Err(unreadable(&inside));
        }
        // The name it is written under is this list's own, never the zip's.
        let (copied, have) = copy_hashing(&mut entry, &into.join(name)).map_err(|e| unreadable(&e))?;
        if copied > LARGEST_FILE {
            return Err(unreadable(&inside));
        }
        if sum.is_some_and(|sum| *sum != have) {
            return Err(changed());
        }
    }
    Ok(())
}

/// Renames the new folder into place as `folder`. One already there (broken, or not the app's
/// own FFmpeg) is moved into the temporary folder first, which goes after, or else deleted; if
/// the new one then can't be put in place, the old one is put back.
fn put_in_place(new: &Path, folder: &Path, part: &Path) -> std::io::Result<()> {
    let aside = part.join("old");
    let mut moved_aside = false;
    if folder.symlink_metadata().is_ok() {
        if retried(|| std::fs::rename(folder, &aside)).is_ok() {
            moved_aside = true;
        } else if folder.is_dir() {
            std::fs::remove_dir_all(folder)?;
        } else {
            std::fs::remove_file(folder)?;
        }
    }
    retried(|| std::fs::rename(new, folder)).inspect_err(|_| {
        if moved_aside {
            let _ = std::fs::rename(&aside, folder);
        }
    })
}

/// A rename tried a few times, a fifth of a second apart: on Windows a program just written may
/// be held open for a moment (Defender looks at every new file), and its folder can't be moved
/// meanwhile.
fn retried(mut rename: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut tries = 1;
    loop {
        match rename() {
            Err(_) if tries < 5 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(200));
            }
            done => return done,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// In the app

/// What the window has said of the fetches, and the videos the operator added while FFmpeg was
/// missing, added once it is ready.
#[derive(Debug, Default)]
pub struct Told {
    begun: u64,
    ended: u64,
    waiting: Vec<PathBuf>,
}

impl crate::App {
    /// Wakes the app with what a fetch has to tell (taken in by [`Self::tick_ffmpeg_fetch`]).
    fn ffmpeg_wake(&self) -> Wake {
        let waker = Mutex::new(self.waker.clone());
        Arc::new(move || {
            if let Ok(waker) = waker.lock()
                && let Some(waker) = waker.as_ref()
            {
                let _ = waker.send_event(crate::AppEvent::FfmpegFetch);
            }
        })
    }

    /// As the app starts: on a thread of its own, the folder FFmpeg is kept in is checked and,
    /// where it doesn't hold the app's own, FFmpeg is copied there from beside the program or
    /// fetched, wherever the program runs from (Windows; never a test copy).
    pub(crate) fn install_ffmpeg_at_start(&self) {
        if let Some(plan) = plan() {
            begin(plan, Why::Start, self.ffmpeg_wake());
        }
    }

    /// A video can't be added or played because FFmpeg is missing: a fetch begins, where this
    /// copy keeps FFmpeg and one may begin (see [`Status::may_begin`]).
    fn fetch_missing_ffmpeg(&self, why: Why) {
        if let Some(plan) = plan() {
            begin(plan, why, self.ffmpeg_wake());
        }
    }

    /// The operator added a video that needs FFmpeg, which is missing: a fetch begins if it may,
    /// and while one is under way the video is kept, to be added once FFmpeg is ready. Whether
    /// it was kept.
    pub(crate) fn ffmpeg_for_added(&mut self, path: &Path) -> bool {
        self.fetch_missing_ffmpeg(Why::Added);
        if !under_way() {
            return false;
        }
        if !self.ffmpeg_fetch.waiting.iter().any(|kept| kept == path) {
            self.ffmpeg_fetch.waiting.push(path.to_owned());
        }
        true
    }

    /// A video couldn't be opened to play. When that is because FFmpeg is missing, a fetch
    /// begins if it may, and the words are the app's own, saying so; otherwise the player's.
    pub(crate) fn video_problem(&self, path: &Path, problem: String) -> String {
        if !crate::media::needs_missing_ffmpeg(path) {
            return problem;
        }
        self.fetch_missing_ffmpeg(Why::Played);
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let advice = crate::media::ffmpeg_advice();
        format!(
            "{name}: {}",
            tf(
                "notices.media.needs_ffmpeg",
                "This video needs FFmpeg to play, and FFmpeg isn't on this computer. {advice}",
                &[("advice", &advice)],
            )
        )
    }

    /// A fetch began or ended: said in the notices, when FFmpeg was missing. Once FFmpeg is
    /// ready, the videos the operator added meanwhile go in the media bin, and a video on screen
    /// that couldn't play starts.
    pub(crate) fn tick_ffmpeg_fetch(&mut self) {
        let now = status();
        let Some(running) = self.running.as_mut() else { return };
        let told = &mut self.ffmpeg_fetch;
        if now.begun > told.begun {
            told.begun = now.begun;
            if now.needed && now.ended < now.begun {
                running.home_state.notices.info(tf(
                    "media.ffmpeg.getting",
                    "Getting FFmpeg for videos ({size})\u{2026}",
                    &[("size", &planned_size())],
                ));
            }
        }
        running.home_window.request_redraw();
        if now.ended <= told.ended {
            return;
        }
        told.ended = now.ended;
        let waiting = std::mem::take(&mut told.waiting);
        if !now.needed && waiting.is_empty() {
            // Fetched while another FFmpeg played the videos: the log says how it went.
            return;
        }
        match now.phase {
            Phase::Ready if sp_video::ffmpeg_found() => {
                let mut added = Vec::new();
                for path in waiting {
                    if self.add_media_file(&path) == Ok(true) {
                        added.push(path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()));
                    }
                }
                let said = if added.is_empty() {
                    t("media.ffmpeg.ready", "FFmpeg is ready: videos play again.").to_owned()
                } else {
                    tn(
                        "media.ffmpeg.ready_added",
                        added.len() as u64,
                        "FFmpeg is ready: videos play again. {names} is in the media bin now.",
                        "FFmpeg is ready: videos play again. {names} are in the media bin now.",
                        &[("names", &added.join(", "))],
                    )
                };
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.info(said);
                }
                // The media on screen is a video that couldn't play: it plays now.
                let live_video = self
                    .show
                    .live_media()
                    .and_then(|index| self.show.media().get(index))
                    .is_some_and(|item| crate::media::is_video(&item.path));
                if live_video && self.video.is_none() {
                    self.video_refused = None;
                    self.refresh_output();
                }
            }
            Phase::Ready => {
                let folder = plan().map(Plan::folder).unwrap_or_default();
                crate::applog::event("video", "FFmpeg was put in place, but the lookup doesn't find it there");
                running.home_state.notices.trouble(tf(
                    "media.ffmpeg.not_found_after",
                    "FFmpeg was fetched into {folder}, but SoftPresenter still can't find it there.",
                    &[("folder", &folder.display().to_string())],
                ));
            }
            Phase::Failed(why) => running.home_state.notices.trouble(tf(
                "media.ffmpeg.failed",
                "Couldn't get FFmpeg for videos: {problem}. Adding a video tries again, and so does starting SoftPresenter again.",
                &[("problem", &why)],
            )),
            Phase::Idle | Phase::Fetching => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A folder of its own for one test.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sp-ffmpeg-fetch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Stand-ins for the two programs, written for these tests.
    const FFMPEG: &[u8] = b"a program standing in for ffmpeg";
    const FFPROBE: &[u8] = b"a program standing in for ffprobe";

    /// A zip with these entries, deflated as the real one is, after the folder's own entry, as
    /// in the real one.
    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.add_directory("ffmpeg/", options).unwrap();
            for (name, bytes) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        out
    }

    /// A zip with the five files, made up for the test.
    fn good_zip() -> Vec<u8> {
        zip_of(&[
            ("ffmpeg/ffmpeg.exe", FFMPEG),
            ("ffmpeg/LICENSE.md", b"licence words written for this test"),
            ("ffmpeg/ffprobe.exe", FFPROBE),
            ("ffmpeg/COPYING.LGPLv2.1", b"the licence, as a stand-in"),
            ("ffmpeg/SOURCE.txt", b"where the source is, as a stand-in"),
        ])
    }

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// A plan fetching `zip` from `url` into `base`, whose programs are the stand-ins.
    fn plan_for(url: &str, zip: &[u8], base: &Path) -> Plan {
        Plan {
            url: url.to_owned(),
            size: zip.len() as u64,
            sha256: sha256(zip),
            programs: [sha256(FFMPEG), sha256(FFPROBE)],
            base: base.to_owned(),
            beside: None,
            local: true,
        }
    }

    /// A whole answer: 200 with this body, saying this length (none: the body ends when the
    /// connection closes).
    fn ok(body: &[u8], length: Option<usize>) -> Vec<u8> {
        let mut head = "HTTP/1.1 200 OK\r\nConnection: close\r\n".to_owned();
        if let Some(length) = length {
            head.push_str(&format!("Content-Length: {length}\r\n"));
        }
        head.push_str("\r\n");
        [head.into_bytes(), body.to_vec()].concat()
    }

    /// Serves on this computer the answers `make` gives for the server's address, one connection
    /// each, in turn; returns the address.
    fn serve(make: impl FnOnce(&str) -> Vec<Vec<u8>>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let answers = make(&address);
        std::thread::spawn(move || {
            for answer in answers {
                let Ok((mut stream, _)) = listener.accept() else { return };
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(&answer);
                let _ = stream.flush();
            }
        });
        address
    }

    /// An address on this computer nobody answers at.
    fn nobody() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/ffmpeg.zip", listener.local_addr().unwrap())
    }

    /// The log's lines a fetch said, kept for the test.
    fn fetch_saying(plan: &Plan) -> (Result<PathBuf, String>, Vec<String>) {
        let lines = Mutex::new(Vec::new());
        let result = fetch(plan, &|line| lines.lock().unwrap().push(line.to_owned()));
        (result, lines.into_inner().unwrap())
    }

    /// The log's lines an install said, a fetch allowed.
    fn install_saying(plan: &Plan) -> (Result<Option<How>, String>, Vec<String>) {
        let lines = Mutex::new(Vec::new());
        let result = install(plan, &|line| lines.lock().unwrap().push(line.to_owned()), &mut || true);
        (result, lines.into_inner().unwrap())
    }

    fn names_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    const FIVE: [&str; 5] = ["COPYING.LGPLv2.1", "LICENSE.md", "SOURCE.txt", "ffmpeg.exe", "ffprobe.exe"];

    /// A package's folder: the program with FFmpeg beside it, and the licences.
    fn package(folder: &Path, ffmpeg: &[u8]) {
        std::fs::create_dir_all(folder.join("Licences")).unwrap();
        std::fs::write(folder.join("SoftPresenter.exe"), b"the app, as a stand-in").unwrap();
        std::fs::write(folder.join("ffmpeg.exe"), ffmpeg).unwrap();
        std::fs::write(folder.join("ffprobe.exe"), FFPROBE).unwrap();
        std::fs::write(folder.join("Licences/COPYING.LGPLv2.1"), b"the licence").unwrap();
        std::fs::write(folder.join("Licences/FFmpeg-LICENSE.md"), b"FFmpeg's licence page").unwrap();
        std::fs::write(folder.join("Licences/FFmpeg-SOURCE.txt"), b"where the source is").unwrap();
    }

    #[test]
    fn the_programs_beside_the_app_are_copied_in_with_no_network_and_an_outdated_copy_is_replaced() {
        let root = temp("copy");
        let (base, beside) = (root.join("Local/SoftPresenter"), root.join("Desktop folder"));
        package(&beside, FFMPEG);
        let before = names_in(&beside);
        // An outdated copy from before (another build of ffmpeg.exe), and something left over.
        std::fs::create_dir_all(base.join("ffmpeg/leftover")).unwrap();
        std::fs::write(base.join("ffmpeg/ffmpeg.exe"), b"an older build").unwrap();
        std::fs::write(base.join("ffmpeg/ffprobe.exe"), FFPROBE).unwrap();
        // Nobody answers at the zip's address: nothing is fetched.
        let plan = Plan { beside: Some(beside.clone()), ..plan_for(&nobody(), b"unused", &base) };
        let (result, lines) = install_saying(&plan);
        assert_eq!(result, Ok(Some(How::Copied)), "{lines:?}");
        assert!(lines.is_empty(), "nothing fetched: {lines:?}");
        assert_eq!(names_in(&base.join("ffmpeg")), FIVE);
        assert_eq!(std::fs::read(base.join("ffmpeg/ffmpeg.exe")).unwrap(), FFMPEG);
        assert_eq!(std::fs::read(base.join("ffmpeg/LICENSE.md")).unwrap(), b"FFmpeg's licence page");
        assert!(!base.join("ffmpeg.part").exists(), "the temporary folder goes");
        assert_eq!(names_in(&beside), before, "nothing is written beside the program");
        // The next start finds it there, as it is, beside the program or not.
        let plan = Plan { beside: None, ..plan };
        assert_eq!(install_saying(&plan).0, Ok(Some(How::Already)));
        // Checked, the folder is where FFmpeg is looked for first; until then, after the
        // program's own folder.
        let folder = base.join("ffmpeg");
        assert_eq!(sp_video::ffmpeg_places_with_fetched(&folder, true)[0], folder.join("ffmpeg.exe"));
        assert_ne!(sp_video::ffmpeg_places_with_fetched(&folder, false)[0], folder.join("ffmpeg.exe"));
        assert!(sp_video::ffmpeg_places_with_fetched(&folder, false).contains(&folder.join("ffmpeg.exe")));
        assert_eq!(sp_video::ffmpeg_program_with_fetched(&folder), Some(folder.join("ffmpeg.exe")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn programs_beside_the_app_that_arent_its_own_are_not_copied_and_the_zip_is_fetched() {
        let root = temp("not-own");
        let (base, beside) = (root.join("Local/SoftPresenter"), root.join("Desktop folder"));
        package(&beside, b"some other build of ffmpeg");
        let zip = good_zip();
        let address = serve(|_| vec![ok(&zip, Some(zip.len()))]);
        let plan = Plan { beside: Some(beside), ..plan_for(&format!("{address}/ffmpeg.zip"), &zip, &base) };
        let (result, lines) = install_saying(&plan);
        assert_eq!(result, Ok(Some(How::Fetched)), "{lines:?}");
        assert!(lines[0].contains("wasn't copied: ffmpeg.exe isn't the app's own FFmpeg"), "{lines:?}");
        assert_eq!(std::fs::read(base.join("ffmpeg/ffmpeg.exe")).unwrap(), FFMPEG);
        assert_eq!(names_in(&base.join("ffmpeg")), FIVE);
        // With no fetch allowed (one under way, or one failed), nothing more is done.
        std::fs::remove_dir_all(base.join("ffmpeg")).unwrap();
        assert_eq!(install(&plan, &|_| {}, &mut || false), Ok(None));
        assert!(!base.join("ffmpeg").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_good_zip_is_put_in_place_and_the_lookup_finds_it_there() {
        let base = temp("good");
        // A broken folder from before, with something in it: it is replaced.
        std::fs::create_dir_all(base.join("ffmpeg/leftover")).unwrap();
        std::fs::write(base.join("ffmpeg/LICENSE.md"), b"old").unwrap();
        // And what a cut-short fetch left behind.
        std::fs::create_dir_all(base.join("ffmpeg.part")).unwrap();
        std::fs::write(base.join("ffmpeg.part/download.zip"), b"half").unwrap();
        let zip = good_zip();
        // GitHub answers the release's address by sending the file from elsewhere.
        let address = serve(|address| {
            let moved = format!(
                "HTTP/1.1 302 Found\r\nLocation: {address}/elsewhere/ffmpeg.zip\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            vec![moved.into_bytes(), ok(&zip, Some(zip.len()))]
        });
        let plan = plan_for(&format!("{address}/release/ffmpeg.zip"), &zip, &base);
        let (result, lines) = fetch_saying(&plan);
        assert_eq!(result.unwrap(), base.join("ffmpeg"));
        assert_eq!(names_in(&base.join("ffmpeg")), FIVE);
        assert_eq!(std::fs::read(base.join("ffmpeg/ffmpeg.exe")).unwrap(), FFMPEG);
        assert!(!base.join("ffmpeg.part").exists(), "the temporary folder goes");
        assert_eq!(
            sp_video::ffmpeg_program_with_fetched(&base.join("ffmpeg")),
            Some(base.join("ffmpeg").join("ffmpeg.exe"))
        );
        assert!(lines[0].starts_with("fetching FFmpeg from http://127.0.0.1:"), "{lines:?}");
        assert!(lines.iter().any(|line| line.starts_with("FFmpeg fetched and checked")), "{lines:?}");
        assert!(lines.iter().any(|line| line.starts_with("FFmpeg put in place at")), "{lines:?}");
        // It holds the app's own now.
        assert_eq!(install_saying(&plan).0, Ok(Some(How::Already)));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_zip_with_another_hash_or_programs_or_without_a_file_is_refused_and_nothing_is_put_in_place() {
        let base = temp("wrong");
        let zip = good_zip();
        let mut other = zip.clone();
        let last = other.len() - 1;
        other[last] ^= 1;
        // The same size, other bytes.
        let address = serve(|_| vec![ok(&other, Some(other.len()))]);
        let (result, lines) = fetch_saying(&plan_for(&format!("{address}/ffmpeg.zip"), &zip, &base));
        assert!(result.unwrap_err().contains("isn't the one this version"));
        assert!(lines.last().unwrap().starts_with("FFmpeg was not fetched: the file isn't the one"), "{lines:?}");
        assert!(!base.join("ffmpeg").exists() && !base.join("ffmpeg.part").exists());
        // The zip expected, but with a program in it other than the app's own: refused too.
        let swapped = zip_of(&[
            ("ffmpeg/ffmpeg.exe", b"another build"),
            ("ffmpeg/ffprobe.exe", FFPROBE),
            ("ffmpeg/COPYING.LGPLv2.1", b"c"),
            ("ffmpeg/LICENSE.md", b"l"),
            ("ffmpeg/SOURCE.txt", b"s"),
        ]);
        let address = serve(|_| vec![ok(&swapped, Some(swapped.len()))]);
        let (result, _) = fetch_saying(&plan_for(&format!("{address}/ffmpeg.zip"), &swapped, &base));
        assert!(result.unwrap_err().contains("isn't the one this version"));
        assert!(!base.join("ffmpeg").exists() && !base.join("ffmpeg.part").exists());
        // Its own hash, but a file missing: refused.
        let short = zip_of(&[("ffmpeg/ffmpeg.exe", FFMPEG), ("ffmpeg/ffprobe.exe", FFPROBE)]);
        let address = serve(|_| vec![ok(&short, Some(short.len()))]);
        let (result, _) = fetch_saying(&plan_for(&format!("{address}/ffmpeg.zip"), &short, &base));
        assert!(result.unwrap_err().contains("couldn't be unpacked"));
        assert!(!base.join("ffmpeg").exists() && !base.join("ffmpeg.part").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_old_folder_is_put_back_when_the_new_one_cant_be_put_in_place() {
        let base = temp("put-back");
        let (folder, part) = (base.join("ffmpeg"), base.join("ffmpeg.part"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("ffmpeg.exe"), b"the old one").unwrap();
        std::fs::create_dir_all(&part).unwrap();
        // The new folder isn't there: the rename fails, each try.
        assert!(put_in_place(&part.join("missing"), &folder, &part).is_err());
        assert_eq!(std::fs::read(folder.join("ffmpeg.exe")).unwrap(), b"the old one");
        assert!(!part.join("old").exists());
        // And a new one goes in its place.
        std::fs::create_dir_all(part.join("new")).unwrap();
        std::fs::write(part.join("new/ffmpeg.exe"), b"the new one").unwrap();
        put_in_place(&part.join("new"), &folder, &part).unwrap();
        assert_eq!(std::fs::read(folder.join("ffmpeg.exe")).unwrap(), b"the new one");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn entries_that_climb_out_of_the_folder_are_never_written() {
        let root = temp("evil");
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let zip = zip_of(&[
            ("ffmpeg/ffmpeg.exe", FFMPEG),
            ("../evil", b"outside"),
            ("ffmpeg/../../evil2", b"outside"),
            ("ffmpeg/ffprobe.exe", FFPROBE),
            ("ffmpeg/COPYING.LGPLv2.1", b"c"),
            ("ffmpeg/LICENSE.md", b"l"),
            ("ffmpeg/SOURCE.txt", b"s"),
            ("ffmpeg/extra.dll", b"not asked for"),
        ]);
        let address = serve(|_| vec![ok(&zip, Some(zip.len()))]);
        let result = fetch(&plan_for(&format!("{address}/ffmpeg.zip"), &zip, &base), &|_| {});
        // The five are taken; nothing else is written anywhere.
        assert_eq!(result.unwrap(), base.join("ffmpeg"));
        assert_eq!(names_in(&base.join("ffmpeg")), FIVE);
        assert_eq!(names_in(&base), ["ffmpeg"]);
        assert_eq!(names_in(&root), ["base"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_body_too_long_or_cut_short_is_refused() {
        let base = temp("size");
        let zip = good_zip();
        let plan = |address: &str| plan_for(&format!("{address}/ffmpeg.zip"), &zip, &base);
        let refused = |address: String| {
            let error = fetch(&plan(&address), &|_| {}).unwrap_err();
            assert!(!base.join("ffmpeg").exists() && !base.join("ffmpeg.part").exists(), "{error}");
            error
        };
        // Said to be longer: refused before anything is read.
        let longer = [zip.clone(), b"and more".to_vec()].concat();
        let error = refused(serve(|_| vec![ok(&longer, Some(longer.len()))]));
        assert!(error.contains("isn't the one this version"), "{error}");
        // Longer, its length not said: reading stops past the size expected.
        let huge = vec![b'x'; zip.len() * 4];
        let error = refused(serve(|_| vec![ok(&huge, None)]));
        assert!(error.contains("isn't the one this version"), "{error}");
        // Cut short: the length said, half the bytes sent.
        let error = refused(serve(|_| vec![ok(&zip[..zip.len() / 2], Some(zip.len()))]));
        assert!(error.contains("cut short") || error.contains("download stopped"), "{error}");
        // Cut short, no length said.
        let error = refused(serve(|_| vec![ok(&zip[..zip.len() / 2], None)]));
        assert!(error.contains("cut short") || error.contains("download stopped"), "{error}");
        // Refused by the server.
        let error = refused(serve(|_| {
            vec![b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()]
        }));
        assert!(error.contains("answered 404"), "{error}");
        // Plain http to anywhere but this computer: nothing is asked of the network.
        let error = fetch(&Plan { url: "http://example.org/ffmpeg.zip".to_owned(), ..plan("x") }, &|_| {}).unwrap_err();
        assert!(error.contains("isn't a secure (https) one"), "{error}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn with_no_server_the_failure_is_logged_and_only_an_added_video_tries_again() {
        let base = temp("nobody");
        let plan = plan_for(&nobody(), b"whatever", &base);
        let (result, lines) = fetch_saying(&plan);
        assert!(result.unwrap_err().contains("couldn't be reached"));
        assert!(
            lines.last().unwrap().starts_with("FFmpeg was not fetched: the server couldn't be reached"),
            "{lines:?}"
        );
        assert!(!base.join("ffmpeg").exists() && !base.join("ffmpeg.part").exists());
        // The same on its own thread, as the app makes it for a video that couldn't play: the
        // app carries on and hears when it is over. (The one test that uses the run's own
        // status.)
        let (woke, wakes) = std::sync::mpsc::channel();
        let woke = Mutex::new(woke);
        let wake: Wake = Arc::new(move || {
            let _ = woke.lock().unwrap().send(());
        });
        assert!(begin(&plan, Why::Played, wake.clone()));
        assert!(!begin(&plan, Why::Added, wake.clone()), "one fetch at a time");
        for _ in 0..2 {
            wakes.recv_timeout(Duration::from_secs(30)).expect("woken as it begins and ends");
        }
        let now = status();
        assert!(matches!(&now.phase, Phase::Failed(why) if why.contains("couldn't be reached")), "{now:?}");
        assert_eq!((now.begun, now.ended, now.needed), (1, 1, true));
        // A video played again doesn't try again; one the operator adds does.
        assert!(!now.may_begin(Why::Played) && !now.may_begin(Why::Start));
        assert!(now.may_begin(Why::Added));
        assert!(!begin(&plan, Why::Played, wake));
        assert_eq!(status().begun, 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_fetch_waits_for_none_under_way_and_after_a_failure_only_for_an_added_video() {
        let at = |phase: Phase| Status { phase, begun: 1, ended: 1, needed: true };
        for why in [Why::Start, Why::Added, Why::Played] {
            assert!(at(Phase::Idle).may_begin(why));
            assert!(at(Phase::Ready).may_begin(why));
            assert!(!at(Phase::Fetching).may_begin(why));
        }
        let failed = at(Phase::Failed("no".to_owned()));
        assert!(failed.may_begin(Why::Added) && !failed.may_begin(Why::Played) && !failed.may_begin(Why::Start));
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn only_a_persons_own_copy_on_windows_installs_and_a_test_copy_only_from_this_computer() {
        let local = ("LOCALAPPDATA", "C:\\Users\\someone\\AppData\\Local");
        let beside = || Some(PathBuf::from("C:\\Users\\someone\\Desktop"));
        let plan_in = |quiet_run, windows, pairs: &[(&str, &str)]| plan_from(quiet_run, windows, beside(), env(pairs));
        // A person's own copy on Windows: the real file, into their own folder, wherever the
        // program runs from.
        let plan = plan_in(false, true, &[local]).unwrap();
        assert_eq!((plan.url.as_str(), plan.size, plan.sha256.as_str(), plan.local), (URL, SIZE, SHA256, false));
        assert_eq!(plan.programs, [FFMPEG_SHA256, FFPROBE_SHA256]);
        assert_eq!(plan.folder(), Path::new(local.1).join("SoftPresenter").join("ffmpeg"));
        assert_eq!(plan.beside, beside());
        // Elsewhere, never.
        assert_eq!(plan_in(false, false, &[local]), None);
        // A quiet copy, one that only saves a picture, one with its own data folder: never.
        assert_eq!(plan_in(false, true, &[local, ("SOFTPRESENTER_QUIET", "1")]), None);
        assert_eq!(plan_in(true, true, &[local]), None);
        assert_eq!(plan_in(false, true, &[local, ("SOFTPRESENTER_DATA", "/tmp/x")]), None);
        // The testing aid: from this computer, into the test copy's own data folder, any system.
        let switched = [
            ("SOFTPRESENTER_QUIET", "1"),
            ("SOFTPRESENTER_DATA", "/tmp/test-copy"),
            ("SOFTPRESENTER_FFMPEG_FETCH_URL", "http://127.0.0.1:4330/ffmpeg.zip"),
        ];
        let plan = plan_in(true, false, &switched).unwrap();
        assert_eq!(plan.base, Path::new("/tmp/test-copy"));
        assert_eq!((plan.size, plan.sha256.as_str(), plan.local), (SIZE, SHA256, true));
        let (zip, ffmpeg, ffprobe) = ("ab".repeat(32), "cd".repeat(32), "EF".repeat(32));
        let expect = format!("1234:{zip}:{ffmpeg}:{ffprobe}");
        let plan =
            plan_in(true, false, &[switched[1], switched[2], ("SOFTPRESENTER_FFMPEG_FETCH_EXPECT", &expect)]).unwrap();
        assert_eq!((plan.size, plan.sha256, plan.programs), (1234, zip.clone(), [ffmpeg, ffprobe.to_lowercase()]));
        // ... and nothing at all when it names anywhere else, or the copy has no data folder.
        let elsewhere = ("SOFTPRESENTER_FFMPEG_FETCH_URL", "https://example.org/ffmpeg.zip");
        assert_eq!(plan_in(false, true, &[local, switched[1], elsewhere]), None);
        assert_eq!(plan_in(false, true, &[local, switched[2]]), None);
        let lookalike = ("SOFTPRESENTER_FFMPEG_FETCH_URL", "http://127.0.0.1.example.org/");
        assert_eq!(plan_in(false, true, &[local, switched[1], lookalike]), None);
        // A wrong expectation does nothing.
        for wrong in ["12:nothex", &format!("12:{zip}")] {
            let wrong = ("SOFTPRESENTER_FFMPEG_FETCH_EXPECT", wrong);
            assert_eq!(plan_in(true, false, &[switched[1], switched[2], wrong]), None);
        }
    }

    #[test]
    fn a_test_run_never_installs_or_fetches() {
        // Tests never call `prepare`, so there is no plan, and nothing begins.
        assert_eq!(plan(), None);
    }

    #[test]
    fn the_file_is_said_in_whole_megabytes() {
        use crate::i18n::{Language, with_language};
        assert_eq!(with_language(Language::English, || size_words(SIZE)), "23 MB");
        assert_eq!(with_language(Language::English, || size_words(400)), "1 MB");
    }
}
