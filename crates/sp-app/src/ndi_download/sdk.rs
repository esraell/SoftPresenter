//! Reading NDI's SDK for Linux, as NDI publishes it, for the two things SoftPresenter needs
//! from it: NDI's licence, shown before anything is installed, and NDI's runtime for this
//! computer's processor.
//!
//! The download is untrusted until it has been read through. Its layout, as NDI's
//! `Install_NDI_SDK_v6_Linux.tar.gz` had it in November 2026 (learnt from how packagers and
//! community tools unpack it, not from running NDI's script):
//!
//! - a gzip-compressed tar holding `Install_NDI_SDK_v6_Linux.sh`;
//! - that script is a shell script with a line of its own, `__NDI_ARCHIVE_BEGIN__`, after
//!   which the rest of the file is another gzip-compressed tar;
//! - that tar holds a folder (`NDI SDK for Linux/`) with `NDI SDK License Agreement.txt`,
//!   `licenses/libndi_licenses.txt` and, for each processor, `lib/<processor>/libndi.so.6.x.y`
//!   (with `libndi.so.6` and `libndi.so` as links to it).
//!
//! Anything else stops the reading with [`Problem::Layout`], which says NDI's download has
//! changed: nothing is installed, and the operator is told so. Nothing is ever written under a
//! name the archive gives: what is kept is written under fixed names in the app's own folder.
//! No link is ever made or followed (links in the archive are passed over), and an entry whose
//! name starts at the root or climbs out with `..` stops the reading as damaged. Every size is
//! checked: the download, the script before its archive, what each compressed part unpacks to,
//! the licence and the runtime.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const MB: u64 = 1024 * 1024;
/// The largest download read. NDI's SDK was 67,470,104 bytes in November 2026.
pub const LARGEST_DOWNLOAD: u64 = 512 * MB;
/// How much of the script may come before the line that starts its archive.
const LONGEST_SCRIPT: u64 = 8 * MB;
/// The most each compressed part may unpack to (NDI's SDK, documentation and examples
/// included, is far less), so a file made to unpack for ever is stopped.
const LARGEST_UNPACKED: u64 = 4096 * MB;
const LARGEST_LICENCE: u64 = MB;
const LARGEST_NOTICES: u64 = 4 * MB;
/// A licence shorter than this isn't NDI's.
const SHORTEST_LICENCE: usize = 100;
const SMALLEST_RUNTIME: u64 = 64 * 1024;
const LARGEST_RUNTIME: u64 = 256 * MB;
/// The longest name a tar may give an entry in an entry of its own.
const LONGEST_NAME: u64 = 64 * 1024;

/// The line that ends NDI's script; its archive starts on the next.
const MARKER: &[u8] = b"__NDI_ARCHIVE_BEGIN__";
/// NDI's licence, in the SDK's own folder.
pub const LICENCE_FILE: &str = "NDI SDK License Agreement.txt";
/// The licences of what NDI's runtime is built with, which go beside the runtime.
pub const NOTICES_FILE: &str = "libndi_licenses.txt";
/// The name the runtime is kept under, which sp-ndi looks for.
pub const RUNTIME_FILE: &str = "libndi.so.6";

/// Why NDI's download wasn't read or installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Laid out other than as NDI's SDK was (NDI may have changed it). The words are for the log.
    Layout(String),
    /// Damaged, cut short, or not what NDI publishes. The words are for the log.
    Damaged(String),
    /// NDI's SDK has no runtime for this processor (named).
    NoRuntime(String),
    /// Larger than NDI's SDK should be.
    TooLarge,
    /// A file couldn't be written on this computer.
    Disk(String),
    /// A file couldn't be written because the disk is full.
    DiskFull,
    /// NDI's server couldn't be reached: this computer isn't online.
    Offline,
    /// The secure connection was refused for its certificate, as on a network that looks into
    /// secure connections with a certificate of its own.
    Blocked,
    /// NDI's server answered, but not with the download (or it took too long, or broke off);
    /// the words are for the log.
    Network(String),
    /// The download isn't signed by NDI. The words are for the log.
    Unsigned(String),
    /// NDI's own installer ended before it had installed NDI.
    Closed,
    /// The operator stopped it.
    Cancelled,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::Layout(why) => write!(f, "not laid out as NDI's SDK was: {why}"),
            Problem::Damaged(why) => write!(f, "damaged: {why}"),
            Problem::NoRuntime(processor) => write!(f, "no runtime for {processor}"),
            Problem::TooLarge => write!(f, "larger than NDI's SDK should be"),
            Problem::Disk(why) => write!(f, "could not be saved: {why}"),
            Problem::DiskFull => write!(f, "could not be saved: the disk is full"),
            Problem::Offline => write!(f, "could not be downloaded: NDI's server couldn't be reached"),
            Problem::Blocked => write!(f, "could not be downloaded: the secure connection's certificate was refused"),
            Problem::Network(why) => write!(f, "could not be downloaded: {why}"),
            Problem::Unsigned(why) => write!(f, "not signed by NDI: {why}"),
            Problem::Closed => write!(f, "NDI's installer ended before it finished"),
            Problem::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl Problem {
    /// Why a file couldn't be written: a full disk is said as one.
    pub fn disk(e: io::Error) -> Problem {
        if e.kind() == io::ErrorKind::StorageFull { Problem::DiskFull } else { Problem::Disk(e.to_string()) }
    }
}

/// What NDI's download holds that SoftPresenter takes, found by reading it through once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    /// NDI's licence, to show before anything is installed.
    pub licence: String,
    /// The licences of what the runtime is built with, if the SDK has them.
    notices: Option<String>,
    /// Where the runtime for this processor is in NDI's archive, and its size.
    runtime: String,
    runtime_size: u64,
    /// The runtime's version, from its file's name ("6.3.2").
    pub version: String,
}

impl Contents {
    /// Testing aid: contents with this licence and nothing to install, for a picture of the
    /// Licence step (`NdiDownload::demo`).
    pub fn sample(licence: String) -> Contents {
        Contents { licence, notices: None, runtime: String::new(), runtime_size: 0, version: "6.1.1".to_owned() }
    }
}

/// Reads NDI's download through and finds NDI's licence and the runtime for `processor`
/// (`std::env::consts::ARCH`). Writes nothing.
pub fn read(download: &Path, processor: &str, cancel: &AtomicBool) -> Result<Contents, Problem> {
    let machine = machine(processor).ok_or_else(|| Problem::NoRuntime(processor.to_owned()))?;
    let mut licence = None;
    let mut notices = None;
    let mut runtimes = Vec::new();
    walk(download, cancel, &mut |entry: &Entry, data: &mut dyn Read| {
        if entry.kind != Kind::File {
            return Ok(true);
        }
        let parts: Vec<&str> = entry.path.split('/').collect();
        let name = parts.last().copied().unwrap_or("");
        if name == LICENCE_FILE && parts.len() <= 2 && licence.is_none() {
            licence = Some(text(data, entry.size, LARGEST_LICENCE)?);
        } else if name == NOTICES_FILE && parts.len() <= 3 && notices.is_none() {
            notices = Some(text(data, entry.size, LARGEST_NOTICES)?);
        } else if let Some(folder) = runtime_folder(&parts)
            && let Some(version) = runtime_version(name)
        {
            let mut head = Vec::with_capacity(64);
            (&mut *data).take(64).read_to_end(&mut head).map_err(broken)?;
            runtimes.push(Runtime {
                path: entry.path.clone(),
                folder: folder.to_owned(),
                version,
                size: entry.size,
                machine: elf_machine(&head),
            });
        }
        Ok(true)
    })?;
    let licence = licence.ok_or_else(|| Problem::Layout(format!("no {LICENCE_FILE}")))?;
    if licence.trim().len() < SHORTEST_LICENCE {
        return Err(Problem::Layout(format!("{LICENCE_FILE} is nearly empty")));
    }
    if runtimes.is_empty() {
        return Err(Problem::Layout(format!("no lib/<processor>/{RUNTIME_FILE}")));
    }
    let mut here: Vec<Runtime> =
        runtimes.into_iter().filter(|runtime| for_processor(&runtime.folder, processor)).collect();
    if here.is_empty() {
        return Err(Problem::NoRuntime(processor.to_owned()));
    }
    here.retain(|runtime| runtime.machine == Some(machine));
    // The folder named for the processor first, then the file with the fullest version (the
    // library itself, not a copy under its short name).
    here.sort_by_key(|runtime| {
        (
            runtime.folder != format!("{processor}-linux-gnu"),
            std::cmp::Reverse(runtime.version.len()),
            runtime.path.clone(),
        )
    });
    let runtime = here
        .into_iter()
        .next()
        .ok_or_else(|| Problem::Layout(format!("the runtime in lib/ isn't a shared library for {processor}")))?;
    if !(SMALLEST_RUNTIME..=LARGEST_RUNTIME).contains(&runtime.size) {
        return Err(Problem::Layout(format!("{} is {} bytes", runtime.path, runtime.size)));
    }
    Ok(Contents { licence, notices, runtime: runtime.path, runtime_size: runtime.size, version: runtime.version })
}

/// Takes what [`read`] found out of NDI's download into `into`: the runtime as
/// [`RUNTIME_FILE`], last and in one step, so it is never found half-written, and before it
/// NDI's licence and notices. Only ever writes those three names, in `into`.
pub fn install(
    download: &Path,
    contents: &Contents,
    processor: &str,
    into: &Path,
    cancel: &AtomicBool,
) -> Result<(), Problem> {
    let machine = machine(processor).ok_or_else(|| Problem::NoRuntime(processor.to_owned()))?;
    let disk = Problem::disk;
    std::fs::create_dir_all(into).map_err(disk)?;
    let part = into.join(format!(".{RUNTIME_FILE}.part"));
    let _ = std::fs::remove_file(&part);
    let mut written = false;
    let taken = walk(download, cancel, &mut |entry: &Entry, data: &mut dyn Read| {
        if entry.kind != Kind::File || entry.path != contents.runtime {
            return Ok(true);
        }
        if entry.size != contents.runtime_size {
            return Err(Problem::Damaged("the download changed after it was read".to_owned()));
        }
        // A new file (never written through something already at that name).
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(disk)?;
        let mut head = [0u8; 64];
        let mut copied = 0u64;
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            let read = data.read(&mut buffer).map_err(broken)?;
            if read == 0 {
                break;
            }
            if copied < 64 {
                let at = usize::try_from(copied).unwrap_or(64);
                let more = (64 - at).min(read);
                head[at..at + more].copy_from_slice(&buffer[..more]);
            }
            file.write_all(&buffer[..read]).map_err(disk)?;
            copied += read as u64;
        }
        if copied != contents.runtime_size || elf_machine(&head) != Some(machine) {
            return Err(Problem::Damaged("the runtime didn't read the same twice".to_owned()));
        }
        file.sync_all().map_err(disk)?;
        written = true;
        Ok(false)
    });
    let finished = taken.and_then(|()| {
        if !written {
            return Err(Problem::Damaged("the runtime wasn't there the second time".to_owned()));
        }
        write_whole(into, LICENCE_FILE, contents.licence.as_bytes())?;
        if let Some(notices) = &contents.notices {
            write_whole(into, NOTICES_FILE, notices.as_bytes())?;
        }
        std::fs::rename(&part, into.join(RUNTIME_FILE)).map_err(disk)
    });
    if finished.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    finished
}

/// Writes a file whole under its name in `folder`: into a hidden new file first, then renamed.
pub fn write_whole(folder: &Path, name: &str, bytes: &[u8]) -> Result<(), Problem> {
    let disk = Problem::disk;
    let part = folder.join(format!(".{name}.part"));
    let _ = std::fs::remove_file(&part);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| std::fs::rename(&part, folder.join(name)));
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written.map_err(disk)
}

/// A runtime found in NDI's archive.
struct Runtime {
    path: String,
    folder: String,
    version: String,
    size: u64,
    machine: Option<u16>,
}

/// The ELF machine number of a processor: the runtimes NDI's SDK has for 64-bit Linux.
fn machine(processor: &str) -> Option<u16> {
    match processor {
        "x86_64" => Some(62),
        "aarch64" => Some(183),
        _ => None,
    }
}

/// True for a folder of `lib/` that holds the runtime for `processor`: `x86_64-linux-gnu`, or
/// for 64-bit ARM any folder named for it (NDI's name for it isn't known for certain; the
/// library's own header is checked as well).
fn for_processor(folder: &str, processor: &str) -> bool {
    match processor {
        "x86_64" => folder == "x86_64-linux-gnu",
        "aarch64" => folder.starts_with("aarch64"),
        _ => false,
    }
}

/// The processor's folder, for an entry `lib/<processor>/<file>` at the top of the archive or
/// in its one folder.
fn runtime_folder<'a>(parts: &[&'a str]) -> Option<&'a str> {
    let n = parts.len();
    ((3..=4).contains(&n) && parts[n - 3] == "lib").then(|| parts[n - 2])
}

/// The version a runtime's file name says: "6" for `libndi.so.6`, "6.3.2" for
/// `libndi.so.6.3.2`; none for any other name.
fn runtime_version(name: &str) -> Option<String> {
    if name == RUNTIME_FILE {
        return Some("6".to_owned());
    }
    let rest = name.strip_prefix("libndi.so.6.")?;
    let numbers = rest.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    numbers.then(|| format!("6.{rest}"))
}

/// The machine an ELF shared library is for, if the bytes start one (64-bit, little-endian).
fn elf_machine(head: &[u8]) -> Option<u16> {
    let shared = head.len() >= 20
        && head.starts_with(b"\x7fELF")
        && head[4] == 2
        && head[5] == 1
        && u16::from_le_bytes([head[16], head[17]]) == 3;
    shared.then(|| u16::from_le_bytes([head[18], head[19]]))
}

/// An entry's data as text, if it is no longer than `largest`, with its lines ending in `\n`.
fn text(data: &mut dyn Read, size: u64, largest: u64) -> Result<String, Problem> {
    if size > largest {
        return Err(Problem::Layout(format!("a text of {size} bytes")));
    }
    let mut bytes = Vec::new();
    (&mut *data).take(largest).read_to_end(&mut bytes).map_err(broken)?;
    let text =
        decode(&bytes).ok_or_else(|| Problem::Layout("a text isn't in an encoding SoftPresenter reads".to_owned()))?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// A text file's words, whatever NDI saved it in: UTF-8 (with or without its byte-order
/// mark), UTF-16 with its mark, or else Windows' Western code page, as an older text from
/// Windows is. None for anything that isn't text (a zero byte, a byte that code page lacks), so
/// the operator is never asked to accept garbage.
fn decode(bytes: &[u8]) -> Option<String> {
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest.chunks(2).map(|pair| Some(unit([pair[0], *pair.get(1)?]))).collect::<Option<_>>()?;
        String::from_utf16(&units).ok()
    };
    let text = if let Some(rest) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        String::from_utf8(rest.to_vec()).ok()?
    } else if let Some(rest) = bytes.strip_prefix(b"\xff\xfe") {
        utf16(rest, u16::from_le_bytes)?
    } else if let Some(rest) = bytes.strip_prefix(b"\xfe\xff") {
        utf16(rest, u16::from_be_bytes)?
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        text.to_owned()
    } else {
        bytes.iter().map(|byte| windows_1252(*byte)).collect::<Option<String>>()?
    };
    (!text.contains('\0')).then_some(text)
}

/// A byte of Windows' Western code page (1252) as a character; none for the five it leaves out.
fn windows_1252(byte: u8) -> Option<char> {
    const HIGH: [u16; 32] = [
        0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039, 0x0152, 0, 0x017d,
        0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0,
        0x017e, 0x0178,
    ];
    match byte {
        0x80..=0x9f => {
            Some(HIGH[usize::from(byte - 0x80)]).filter(|code| *code != 0).and_then(|code| char::from_u32(code.into()))
        }
        _ => Some(char::from(byte)),
    }
}

/// Reads NDI's download through: the script in it, and the archive after its marker, each of
/// whose entries is handed to `visit` (with its data) until it says to stop.
fn walk(download: &Path, cancel: &AtomicBool, visit: &mut Visit<'_>) -> Result<(), Problem> {
    let file = std::fs::File::open(download).map_err(|e| Problem::Damaged(e.to_string()))?;
    let size = file.metadata().map_err(|e| Problem::Damaged(e.to_string()))?.len();
    if size > LARGEST_DOWNLOAD {
        return Err(Problem::TooLarge);
    }
    let mut file = BufReader::new(file);
    if !file.fill_buf().map_err(broken)?.starts_with(&[0x1f, 0x8b]) {
        return Err(Problem::Damaged("the download isn't compressed as NDI's is".to_owned()));
    }
    let mut outer = Budget { inner: flate2::bufread::GzDecoder::new(file), left: LARGEST_UNPACKED, cancel };
    let mut found = false;
    each_entry(&mut outer, &mut |entry: &Entry, script: &mut dyn Read| {
        let top = !entry.path.contains('/');
        let name = &entry.path;
        if entry.kind != Kind::File || !top || !name.starts_with("Install_NDI_SDK_v") || !name.ends_with("_Linux.sh") {
            return Ok(true);
        }
        found = true;
        let mut archive = after_marker(script, cancel)?;
        each_entry(&mut archive, visit)?;
        Ok(false)
    })?;
    if !found {
        return Err(Problem::Layout("no Install_NDI_SDK_v…_Linux.sh".to_owned()));
    }
    Ok(())
}

type Visit<'a> = dyn FnMut(&Entry, &mut dyn Read) -> Result<bool, Problem> + 'a;

/// Reads NDI's script up to the line that marks the start of its archive, and gives the
/// archive after it, uncompressed.
fn after_marker<'a>(script: &'a mut dyn Read, cancel: &'a AtomicBool) -> Result<impl Read + 'a, Problem> {
    let mut reader = BufReader::with_capacity(64 * 1024, script);
    let mut left = LONGEST_SCRIPT;
    loop {
        let mut line = Vec::new();
        let read = (&mut reader).take(left).read_until(b'\n', &mut line).map_err(broken)?;
        if read == 0 || (left == read as u64 && !line.ends_with(b"\n")) {
            return Err(Problem::Layout("no line __NDI_ARCHIVE_BEGIN__ in the script".to_owned()));
        }
        left -= read as u64;
        let bare = line.strip_suffix(b"\n").unwrap_or(&line);
        let bare = bare.strip_suffix(b"\r").unwrap_or(bare);
        if bare == MARKER {
            break;
        }
    }
    let head = reader.fill_buf().map_err(broken)?;
    if !head.starts_with(&[0x1f, 0x8b]) {
        let what = if head.starts_with(b"BZh") {
            "bzip2"
        } else if head.starts_with(b"\xfd7zXZ\0") {
            "xz"
        } else if head.starts_with(b"\x28\xb5\x2f\xfd") {
            "zstd"
        } else {
            "not gzip"
        };
        return Err(Problem::Layout(format!("the script's archive is {what}")));
    }
    Ok(Budget { inner: flate2::bufread::GzDecoder::new(reader), left: LARGEST_UNPACKED, cancel })
}

/// A reader that stops when the operator cancels, or after `left` bytes.
struct Budget<'a, R> {
    inner: R,
    left: u64,
    cancel: &'a AtomicBool,
}

impl<R: Read> Read for Budget<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other(Stop::Cancelled));
        }
        let read = self.inner.read(buf)?;
        self.left = self.left.checked_sub(read as u64).ok_or_else(|| io::Error::other(Stop::TooLarge))?;
        Ok(read)
    }
}

/// Why a [`Budget`] stopped.
#[derive(Debug)]
enum Stop {
    Cancelled,
    TooLarge,
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Stop::Cancelled => "cancelled",
            Stop::TooLarge => "too large",
        })
    }
}

impl std::error::Error for Stop {}

/// What a failed read means: cancelled, too large, or a damaged download.
fn broken(e: io::Error) -> Problem {
    match e.get_ref().and_then(|inner| inner.downcast_ref::<Stop>()) {
        Some(Stop::Cancelled) => Problem::Cancelled,
        Some(Stop::TooLarge) => Problem::TooLarge,
        None => Problem::Damaged(e.to_string()),
    }
}

/// What an entry of a tar is. Only files are ever read; links and folders are passed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    File,
    Link,
    Folder,
    Other,
}

/// One entry of a tar: its name (cleaned: no `.` or empty parts) and what it is.
#[derive(Debug)]
struct Entry {
    path: String,
    kind: Kind,
    size: u64,
}

/// Reads a tar through, handing each entry and its data (no more than its own) to `visit`,
/// until the tar ends or `visit` says to stop. Reads the POSIX and GNU forms, with the names
/// and sizes their extra entries give (GNU long names, pax headers).
fn each_entry(tar: &mut dyn Read, visit: &mut Visit<'_>) -> Result<(), Problem> {
    let mut long_name: Option<String> = None;
    let mut pax_path: Option<String> = None;
    let mut pax_size: Option<u64> = None;
    loop {
        let mut header = [0u8; 512];
        if !block(tar, &mut header)? {
            return Err(Problem::Damaged("a tar ends without its end".to_owned()));
        }
        if header.iter().all(|b| *b == 0) {
            return Ok(());
        }
        if !sum_is_right(&header) {
            return Err(Problem::Damaged("a tar header's checksum is wrong".to_owned()));
        }
        let size = match pax_size.take() {
            Some(size) => size,
            None => number(&header[124..136])?,
        };
        // (A size so large its padding can't be worked out is no entry of NDI's.)
        let padding = size
            .checked_next_multiple_of(512)
            .map(|padded| padded - size)
            .ok_or_else(|| Problem::Damaged("a tar entry's size is beyond any file".to_owned()))?;
        let flag = header[156];
        match flag {
            b'L' => {
                long_name = Some(small(tar, size, padding)?);
                continue;
            }
            b'x' => {
                let records = small(tar, size, padding)?;
                (pax_path, pax_size) = pax(&records)?;
                continue;
            }
            b'g' | b'K' => {
                skip(tar, size + padding)?;
                continue;
            }
            _ => {}
        }
        let raw = pax_path.take().or_else(|| long_name.take()).unwrap_or_else(|| header_name(&header));
        long_name = None;
        let path = clean(&raw)?;
        let kind = match flag {
            b'0' | 0 | b'7' => Kind::File,
            b'1' | b'2' => Kind::Link,
            b'5' => Kind::Folder,
            _ => Kind::Other,
        };
        let mut data = (&mut *tar).take(size);
        // Nothing more is read once `visit` has what it wanted.
        if !visit(&Entry { path, kind, size }, &mut data)? {
            return Ok(());
        }
        let rest = io::copy(&mut data, &mut io::sink()).map_err(broken)?;
        if data.limit() > 0 {
            return Err(Problem::Damaged(format!("a tar is cut short ({rest} bytes read of the rest)")));
        }
        skip(tar, padding)?;
    }
}

/// Reads one 512-byte block: false at the very end.
fn block(tar: &mut dyn Read, header: &mut [u8; 512]) -> Result<bool, Problem> {
    let mut at = 0;
    while at < header.len() {
        let read = tar.read(&mut header[at..]).map_err(broken)?;
        if read == 0 {
            return if at == 0 { Ok(false) } else { Err(Problem::Damaged("a tar is cut short".to_owned())) };
        }
        at += read;
    }
    Ok(true)
}

fn skip(tar: &mut dyn Read, count: u64) -> Result<(), Problem> {
    let skipped = io::copy(&mut (&mut *tar).take(count), &mut io::sink()).map_err(broken)?;
    if skipped != count {
        return Err(Problem::Damaged("a tar is cut short".to_owned()));
    }
    Ok(())
}

/// A small entry's data (a long name, a pax header) as text.
fn small(tar: &mut dyn Read, size: u64, padding: u64) -> Result<String, Problem> {
    if size > LONGEST_NAME {
        return Err(Problem::Damaged("a tar names an entry at great length".to_owned()));
    }
    let mut bytes = Vec::new();
    (&mut *tar).take(size).read_to_end(&mut bytes).map_err(broken)?;
    if bytes.len() as u64 != size {
        return Err(Problem::Damaged("a tar is cut short".to_owned()));
    }
    skip(tar, padding)?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.trim_end_matches('\0').to_owned())
}

/// The path and size a pax header gives the next entry, if it gives them.
fn pax(records: &str) -> Result<(Option<String>, Option<u64>), Problem> {
    let (mut path, mut size) = (None, None);
    let mut rest = records;
    while !rest.is_empty() {
        let bad = || Problem::Damaged("a pax header is damaged".to_owned());
        let (length, _) = rest.split_once(' ').ok_or_else(bad)?;
        let length: usize = length.parse().map_err(|_| bad())?;
        let record = rest.get(..length).ok_or_else(bad)?;
        rest = &rest[length..];
        let body = record.split_once(' ').map(|(_, body)| body).ok_or_else(bad)?;
        let body = body.strip_suffix('\n').ok_or_else(bad)?;
        let (key, value) = body.split_once('=').ok_or_else(bad)?;
        match key {
            "path" => path = Some(value.to_owned()),
            "size" => size = Some(value.parse().map_err(|_| bad())?),
            _ => {}
        }
    }
    Ok((path, size))
}

/// The name a header gives (with the POSIX prefix before it).
fn header_name(header: &[u8; 512]) -> String {
    let field = |bytes: &[u8]| {
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    };
    let name = field(&header[..100]);
    if &header[257..263] == b"ustar\0" {
        let prefix = field(&header[345..500]);
        if !prefix.is_empty() {
            return format!("{prefix}/{name}");
        }
    }
    name
}

/// A name with its `.` and empty parts left out. One that starts at the root, or climbs out
/// of the archive with `..`, is refused: NDI's never does.
fn clean(raw: &str) -> Result<String, Problem> {
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(Problem::Damaged(format!("an entry is named from the root ({raw})")));
    }
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(Problem::Damaged(format!("an entry climbs out of the archive ({raw})"))),
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

/// A header's number: octal digits, or the big-endian binary form GNU tar uses for large ones.
fn number(field: &[u8]) -> Result<u64, Problem> {
    let bad = || Problem::Damaged("a tar header's number is damaged".to_owned());
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        let mut value = u64::from(field[0] & 0x7f);
        for byte in &field[1..] {
            value = value.checked_mul(256).and_then(|v| v.checked_add(u64::from(*byte))).ok_or_else(bad)?;
        }
        return Ok(value);
    }
    let text = std::str::from_utf8(field).map_err(|_| bad())?.trim_matches(|c| c == '\0' || c == ' ');
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| bad())
}

/// True when a header's checksum (its bytes added up, the checksum's own as spaces) is right.
fn sum_is_right(header: &[u8; 512]) -> bool {
    let Ok(stored) = number(&header[148..156]) else { return false };
    let mut unsigned: u64 = 0;
    let mut signed: i64 = 0;
    for (at, byte) in header.iter().enumerate() {
        let byte = if (148..156).contains(&at) { b' ' } else { *byte };
        unsigned += u64::from(byte);
        signed += i64::from(byte as i8);
    }
    stored == unsigned || i64::try_from(stored).is_ok_and(|stored| stored == signed)
}

#[cfg(test)]
pub mod fixture {
    //! A download laid out as NDI's SDK for Linux is, made for the tests: the words and the
    //! "libraries" are made up here (a library is an ELF header and filler), nothing is NDI's.

    use std::io::Write;

    /// One entry of a tar to make: its name, its type flag and its data (a link's target for a
    /// link).
    pub struct Item<'a> {
        pub name: &'a str,
        pub flag: u8,
        pub data: Vec<u8>,
    }

    pub fn file<'a>(name: &'a str, data: &[u8]) -> Item<'a> {
        Item { name, flag: b'0', data: data.to_vec() }
    }

    pub fn link<'a>(name: &'a str, target: &str) -> Item<'a> {
        Item { name, flag: b'2', data: target.as_bytes().to_vec() }
    }

    fn header(name: &str, flag: u8, size: u64, link: &str) -> [u8; 512] {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0001750\0");
        header[116..124].copy_from_slice(b"0001750\0");
        header[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
        header[136..148].copy_from_slice(b"15042000000\0");
        header[156] = flag;
        header[157..157 + link.len()].copy_from_slice(link.as_bytes());
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        header
    }

    /// A tar of the items, names over 100 letters given in a pax header.
    pub fn tar(items: &[Item<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        for item in items {
            let pad = |out: &mut Vec<u8>| out.resize(out.len().next_multiple_of(512), 0);
            let name = if item.name.len() > 100 {
                let record = |key: &str, value: &str| {
                    let body = format!(" {key}={value}\n");
                    let mut length = body.len() + 1;
                    while format!("{length}{body}").len() != length {
                        length += 1;
                    }
                    format!("{length}{body}")
                };
                let records = record("path", item.name);
                out.extend_from_slice(&header("PaxHeaders/entry", b'x', records.len() as u64, ""));
                out.extend_from_slice(records.as_bytes());
                pad(&mut out);
                "long-name-in-pax"
            } else {
                item.name
            };
            if item.flag == b'2' || item.flag == b'1' {
                out.extend_from_slice(&header(name, item.flag, 0, &String::from_utf8_lossy(&item.data)));
            } else {
                out.extend_from_slice(&header(name, item.flag, item.data.len() as u64, ""));
                out.extend_from_slice(&item.data);
                pad(&mut out);
            }
        }
        out.extend_from_slice(&[0u8; 1024]);
        out
    }

    pub fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    /// A made-up shared library for the ELF machine number given, `size` bytes long.
    pub fn library(machine: u16, size: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; size];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[6] = 1;
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        for (at, byte) in bytes.iter_mut().enumerate().skip(64) {
            *byte = (at % 251) as u8;
        }
        bytes
    }

    pub const LICENCE: &str = "TEST LICENCE, written for SoftPresenter's tests.\r\n\r\nThis stands where the SDK's licence would be. It is long enough to be taken for one, and says nothing else.\r\n";
    pub const SCRIPT_HEAD: &str = "#!/bin/sh\n# A stand-in for the installer's script, written for the tests.\nARCHIVE=`awk '/^__NDI_ARCHIVE_BEGIN__/ { print NR + 1; exit 0; }' \"$0\"`\necho \"__NDI_ARCHIVE_BEGIN__ is not alone on this line\"\nexit 0\n";

    /// The items of the SDK's own folder, as NDI's SDK for Linux lays them out.
    pub fn sdk_items() -> Vec<Item<'static>> {
        vec![
            Item { name: "NDI SDK for Linux/", flag: b'5', data: Vec::new() },
            file("NDI SDK for Linux/NDI SDK License Agreement.txt", LICENCE.as_bytes()),
            file(
                "NDI SDK for Linux/licenses/libndi_licenses.txt",
                b"Test notices: the libraries the runtime is built with.\n",
            ),
            file("NDI SDK for Linux/include/Processing.NDI.Lib.h", b"/* a header, made up */\n"),
            file("NDI SDK for Linux/lib/x86_64-linux-gnu/libndi.so.6.3.2", &library(62, 80_000)),
            link("NDI SDK for Linux/lib/x86_64-linux-gnu/libndi.so.6", "libndi.so.6.3.2"),
            link("NDI SDK for Linux/lib/x86_64-linux-gnu/libndi.so", "libndi.so.6"),
            file("NDI SDK for Linux/lib/aarch64-rpi4-linux-gnueabi/libndi.so.6.3.2", &library(183, 70_000)),
            link("NDI SDK for Linux/lib/aarch64-rpi4-linux-gnueabi/libndi.so.6", "libndi.so.6.3.2"),
            file("NDI SDK for Linux/lib/i686-linux-gnu/libndi.so.6.3.2", &library(3, 70_000)),
            file("NDI SDK for Linux/bin/x86_64-linux-gnu/ndi-record", b"not used"),
        ]
    }

    /// The installer's script: its head, the marker, then the payload as given.
    pub fn script(payload: &[u8]) -> Vec<u8> {
        let mut script = SCRIPT_HEAD.as_bytes().to_vec();
        script.extend_from_slice(b"__NDI_ARCHIVE_BEGIN__\n");
        script.extend_from_slice(payload);
        script
    }

    /// A whole download: a gzipped tar holding the script, whose payload is these items.
    pub fn download(items: &[Item<'_>]) -> Vec<u8> {
        download_of_script(&script(&gzip(&tar(items))))
    }

    pub fn download_of_script(script: &[u8]) -> Vec<u8> {
        gzip(&tar(&[file("Install_NDI_SDK_v6_Linux.sh", script)]))
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::*;
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-ndi-sdk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn saved(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = folder(name).join("Install_NDI_SDK_v6_Linux.tar.gz");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn read_bytes(name: &str, bytes: &[u8], processor: &str) -> Result<Contents, Problem> {
        read(&saved(name, bytes), processor, &AtomicBool::new(false))
    }

    #[test]
    fn an_entry_whose_size_is_near_the_largest_number_is_refused_not_added_up() {
        // A header in GNU's binary form saying the entry is 2^64 - 1 bytes long.
        let mut header = [0u8; 512];
        header[..4].copy_from_slice(b"huge");
        header[100..108].copy_from_slice(b"0000644\0");
        header[124] = 0x80;
        header[128..136].copy_from_slice(&u64::MAX.to_be_bytes());
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        let mut tar = header.to_vec();
        tar.extend_from_slice(&[0u8; 1024]);
        let visited = each_entry(&mut tar.as_slice(), &mut |_: &Entry, _: &mut dyn Read| Ok(true));
        assert!(matches!(visited, Err(Problem::Damaged(_))), "{visited:?}");
    }

    #[test]
    fn the_licence_and_the_runtime_for_each_processor_are_found_and_links_passed_over() {
        let download = download(&sdk_items());
        let contents = read_bytes("found", &download, "x86_64").unwrap();
        assert!(contents.licence.starts_with("TEST LICENCE"), "{}", contents.licence);
        assert!(!contents.licence.contains('\r'), "lines end in \\n");
        assert_eq!(contents.runtime, "NDI SDK for Linux/lib/x86_64-linux-gnu/libndi.so.6.3.2");
        assert_eq!(contents.version, "6.3.2");
        assert_eq!(contents.runtime_size, 80_000);
        assert!(contents.notices.as_deref().is_some_and(|notices| notices.starts_with("Test notices")));
        let arm = read_bytes("found-arm", &download, "aarch64").unwrap();
        assert_eq!(arm.runtime, "NDI SDK for Linux/lib/aarch64-rpi4-linux-gnueabi/libndi.so.6.3.2");
    }

    #[test]
    fn installing_writes_only_the_runtime_the_licence_and_the_notices_under_their_own_names() {
        let download = saved("install", &download(&sdk_items()));
        let contents = read(&download, "x86_64", &AtomicBool::new(false)).unwrap();
        let into = folder("install-into").join("NDI");
        install(&download, &contents, "x86_64", &into, &AtomicBool::new(false)).unwrap();
        let mut names: Vec<String> =
            std::fs::read_dir(&into).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, [LICENCE_FILE, RUNTIME_FILE, NOTICES_FILE], "in name order");
        assert_eq!(std::fs::read(into.join(RUNTIME_FILE)).unwrap(), library(62, 80_000));
        assert!(std::fs::symlink_metadata(into.join(RUNTIME_FILE)).unwrap().file_type().is_file());
        assert_eq!(std::fs::read_to_string(into.join(LICENCE_FILE)).unwrap(), contents.licence);
    }

    #[test]
    fn a_script_without_its_marker_line_is_a_changed_layout() {
        let mut script = SCRIPT_HEAD.as_bytes().to_vec();
        script.extend_from_slice(&gzip(&tar(&sdk_items())));
        let problem = read_bytes("no-marker", &download_of_script(&script), "x86_64").unwrap_err();
        assert!(matches!(problem, Problem::Layout(_)), "{problem:?}");
        // The marker with more on its line doesn't count.
        let mut script = SCRIPT_HEAD.as_bytes().to_vec();
        script.extend_from_slice(b"__NDI_ARCHIVE_BEGIN__ # and more\n");
        script.extend_from_slice(&gzip(&tar(&sdk_items())));
        assert!(matches!(
            read_bytes("marker-not-alone", &download_of_script(&script), "x86_64"),
            Err(Problem::Layout(_))
        ));
    }

    #[test]
    fn a_payload_compressed_another_way_is_a_changed_layout() {
        let mut payload = b"BZh91AY&SY".to_vec();
        payload.extend_from_slice(&[0u8; 200]);
        let problem = read_bytes("bzip2", &download_of_script(&script(&payload)), "x86_64").unwrap_err();
        assert_eq!(problem, Problem::Layout("the script's archive is bzip2".to_owned()));
    }

    #[test]
    fn a_download_without_the_script_or_the_licence_or_a_runtime_installs_nothing() {
        let other = gzip(&tar(&[file("README.txt", b"something else")]));
        assert!(matches!(read_bytes("no-script", &other, "x86_64"), Err(Problem::Layout(_))));
        let without_licence: Vec<Item<'_>> =
            sdk_items().into_iter().filter(|item| !item.name.ends_with(LICENCE_FILE)).collect();
        assert_eq!(
            read_bytes("no-licence", &download(&without_licence), "x86_64").unwrap_err(),
            Problem::Layout(format!("no {LICENCE_FILE}"))
        );
        let without_runtimes: Vec<Item<'_>> =
            sdk_items().into_iter().filter(|item| !item.name.contains("/lib/")).collect();
        assert!(matches!(read_bytes("no-runtime", &download(&without_runtimes), "x86_64"), Err(Problem::Layout(_))));
    }

    #[test]
    fn a_processor_the_sdk_has_no_runtime_for_is_said() {
        let download = download(&sdk_items());
        assert_eq!(read_bytes("riscv", &download, "riscv64").unwrap_err(), Problem::NoRuntime("riscv64".to_owned()));
        let only_intel: Vec<Item<'_>> = sdk_items().into_iter().filter(|item| !item.name.contains("aarch64")).collect();
        assert_eq!(
            read_bytes("no-arm", &download_of_script(&script(&gzip(&tar(&only_intel)))), "aarch64").unwrap_err(),
            Problem::NoRuntime("aarch64".to_owned())
        );
    }

    #[test]
    fn a_runtime_that_isnt_a_library_for_this_processor_isnt_taken() {
        let mut items = sdk_items();
        for item in &mut items {
            if item.name.ends_with("x86_64-linux-gnu/libndi.so.6.3.2") {
                item.data = library(183, 80_000);
            }
        }
        assert!(matches!(read_bytes("wrong-machine", &download(&items), "x86_64"), Err(Problem::Layout(_))));
    }

    #[test]
    fn names_from_the_root_or_climbing_out_stop_the_reading() {
        for name in ["/etc/libndi.so.6.3.2", "NDI SDK for Linux/../../libndi.so.6.3.2", "../outside.txt"] {
            let mut items = sdk_items();
            items.insert(1, file(name, b"x"));
            let problem = read_bytes("climbing", &download(&items), "x86_64").unwrap_err();
            assert!(matches!(problem, Problem::Damaged(_)), "{name}: {problem:?}");
        }
        // And in the outer archive.
        let outer = gzip(&tar(&[file("../Install_NDI_SDK_v6_Linux.sh", &script(&gzip(&tar(&sdk_items()))))]));
        assert!(matches!(read_bytes("climbing-outer", &outer, "x86_64"), Err(Problem::Damaged(_))));
    }

    #[test]
    fn a_long_name_given_in_a_pax_header_is_read() {
        let deep = "NDI SDK for Linux/lib/x86_64-linux-gnu/libndi.so.6.3.2";
        let long = format!("{}/lib/x86_64-linux-gnu/libndi.so.6.3.2", "N".repeat(90));
        let mut items: Vec<Item<'_>> =
            sdk_items().into_iter().filter(|item| !item.name.starts_with(&deep[..38])).collect();
        items.push(file(&long, &library(62, 80_000)));
        let contents = read_bytes("pax", &download(&items), "x86_64").unwrap();
        assert_eq!(contents.runtime, long);
    }

    #[test]
    fn a_cut_short_or_damaged_download_is_said_to_be() {
        let whole = download(&sdk_items());
        let problem = read_bytes("cut", &whole[..whole.len() / 2], "x86_64").unwrap_err();
        assert!(matches!(problem, Problem::Damaged(_)), "{problem:?}");
        assert!(matches!(read_bytes("not-gzip", b"<html>Not found</html>", "x86_64"), Err(Problem::Damaged(_))));
        // A header whose checksum is wrong.
        let mut tar_bytes = tar(&[file("Install_NDI_SDK_v6_Linux.sh", &script(&gzip(&tar(&sdk_items()))))]);
        tar_bytes[0] = b'J';
        assert!(matches!(read_bytes("checksum", &gzip(&tar_bytes), "x86_64"), Err(Problem::Damaged(_))));
    }

    #[test]
    fn cancelling_stops_the_reading() {
        let download = saved("cancel", &download(&sdk_items()));
        assert_eq!(read(&download, "x86_64", &AtomicBool::new(true)).unwrap_err(), Problem::Cancelled);
    }

    #[test]
    fn the_licence_reads_in_any_encoding_a_text_file_comes_in_and_garbage_is_refused() {
        let words = "Licence \u{201c}quoted\u{201d} \u{2013} caf\u{e9}";
        assert_eq!(decode(words.as_bytes()).as_deref(), Some(words));
        let mut with_mark = b"\xef\xbb\xbf".to_vec();
        with_mark.extend_from_slice(words.as_bytes());
        assert_eq!(decode(&with_mark).as_deref(), Some(words));
        let mut little = b"\xff\xfe".to_vec();
        little.extend(words.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode(&little).as_deref(), Some(words));
        let mut big = b"\xfe\xff".to_vec();
        big.extend(words.encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(decode(&big).as_deref(), Some(words));
        // Windows' Western code page: curly quotes, a dash and an accent.
        assert_eq!(decode(b"Licence \x93quoted\x94 \x96 caf\xe9").as_deref(), Some(words));
        // Not text: a zero byte, a byte that code page leaves out, half a UTF-16 unit.
        assert_eq!(decode(b"Licence\0"), None);
        assert_eq!(decode(b"Licence \x81\xff"), None);
        assert_eq!(decode(b"\xff\xfeL\0i"), None);
        // In a download: a licence that isn't text installs nothing.
        let mut items = sdk_items();
        for item in &mut items {
            if item.name.ends_with(LICENCE_FILE) {
                item.data = [b"\xff\xfe".as_slice(), &[0x00, 0xd8], &[b'x'; 200]].concat();
            }
        }
        assert!(matches!(read_bytes("garbled-licence", &download(&items), "x86_64"), Err(Problem::Layout(_))));
    }

    #[test]
    fn the_runtimes_name_says_its_version() {
        assert_eq!(runtime_version("libndi.so.6").as_deref(), Some("6"));
        assert_eq!(runtime_version("libndi.so.6.3.2").as_deref(), Some("6.3.2"));
        assert_eq!(runtime_version("libndi.so.6.3.2.0").as_deref(), Some("6.3.2.0"));
        for other in ["libndi.so", "libndi.so.5.6", "libndi.so.6.", "libndi.so.6.x", "libndi.so.6..1"] {
            assert_eq!(runtime_version(other), None, "{other}");
        }
    }
}
