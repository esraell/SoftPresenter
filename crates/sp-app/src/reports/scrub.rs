//! What decides whether a piece of text may go in an anonymous report. Three places hold text:
//! the problem lines, and a crash's message and backtrace. The rule is *drop, don't clean*: a
//! line that could hold a song's or a file's name, a person's or a church's, is left out whole
//! rather than have the name cut out of it, because cutting needs to know where the name ends.
//! A crash's place and the paths in its backtrace are the one thing that is cut: they are the
//! computer's that built the program, and only their part inside the program's source is kept.
//!
//! Everything kept here must also pass `check::free_text`, the same check the server makes.

use crate::log_scrub::Scrubber;

use super::check;

/// The areas of the log whose problems are about the computer and the drawing, never about a
/// song, a Bible or a person: the graphics, the screens' drawing, the window's drawing, and video
/// playing (whose lines may name a file, which always has a video's ending and so is caught
/// below). A problem from any other area is not sent. An area joins this list only after every
/// place that writes a problem in it has been read for what its words can hold.
const AREAS: [&str; 4] = ["gpu", "output", "window", "video"];

/// Whether a word looks like a file's name: something, a dot, and an ending of one to five
/// letters and digits with a letter among them (`intro.mp4`, `Sunday.pro`; not `1.5` or `0.25`).
fn like_a_file(token: &str) -> bool {
    let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    match token.rsplit_once('.') {
        Some((before, ending)) => {
            !before.is_empty()
                && (1..=5).contains(&ending.len())
                && ending.bytes().all(|b| b.is_ascii_alphanumeric())
                && ending.bytes().any(|b| b.is_ascii_alphabetic())
        }
        None => false,
    }
}

/// Whether text could hold something of the person's own: a letter outside plain ASCII (a title
/// in Russian), anything the log's cleaner would change (a path, an address, the account's
/// name, a key), a quotation (names are quoted), a path's separator, or a file's name.
fn could_name_something(text: &str, scrubber: &Scrubber) -> bool {
    let bytes = text.as_bytes();
    if !bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
        return true;
    }
    if scrubber.scrub(text) != text {
        return true;
    }
    if text.contains(['"', '`', '\\', '/', '~']) {
        return true;
    }
    // An apostrophe inside a word ("couldn't") is no quotation; any other is.
    let quoting = bytes.iter().enumerate().any(|(at, byte)| {
        *byte == b'\''
            && !(at > 0
                && bytes[at - 1].is_ascii_alphabetic()
                && bytes.get(at + 1).is_some_and(u8::is_ascii_alphabetic))
    });
    quoting || text.split_whitespace().any(like_a_file)
}

/// A problem's line as a report carries it, or nothing when it must be left out: `[area] words`
/// as `stats::problem_in` kept it.
pub fn problem_line(line: &str, scrubber: &Scrubber) -> Option<String> {
    let area = line.strip_prefix('[')?.split_once(']')?.0;
    if !AREAS.contains(&area) || could_name_something(line, scrubber) {
        return None;
    }
    let cut: String = line.chars().take(check::PROBLEM_LENGTH).collect();
    check::free_text(&cut, check::PROBLEM_LENGTH, false, true).ok().map(|()| cut)
}

/// What Rust's own messages put in backticks: names from its library, the same in every crash.
/// Any other quotation makes a crash's message one that could hold a name.
const RUST_NAMES: [&str; 10] = [
    "`Option::unwrap()`",
    "`Option::expect()`",
    "`Result::unwrap()`",
    "`Result::expect()`",
    "`Result::unwrap_err()`",
    "`None`",
    "`Some`",
    "`Err`",
    "`Ok`",
    "`RefCell`",
];

/// A crash's message, or nothing when it could quote something of the person's own.
pub fn crash_message(message: &str, scrubber: &Scrubber) -> Option<String> {
    let message = message.trim();
    // Looked at without Rust's own names, which are the only quotations let through.
    let mut plain = message.to_owned();
    for name in RUST_NAMES {
        plain = plain.replace(name, name.trim_matches('`'));
    }
    if message.is_empty() || could_name_something(&plain, scrubber) {
        return None;
    }
    let cut: String = message.chars().take(check::CRASH_MESSAGE).collect();
    check::free_text(&cut, check::CRASH_MESSAGE, false, true).ok().map(|()| cut)
}

/// The part of a source file's path that is inside the program's source, with `/` between its
/// parts: `crates/sp-app/src/deck.rs`, `wgpu-27.0.1/src/api/device.rs`, or Rust's own library as
/// every build names it (`/rustc/<commit>/library/…`). Nothing for a path of any other shape.
fn source_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    // Rust's own library.
    if let Some(at) = path.find("/rustc/") {
        let rest = &path[at + 7..];
        if rest.len() > 41 && rest.as_bytes()[..40].iter().all(u8::is_ascii_hexdigit) && rest.as_bytes()[40] == b'/' {
            return Some(path[at..].to_owned());
        }
    }
    // A crate from the registry: what follows the index's folder.
    if let Some(at) = path.find("/registry/src/") {
        return path[at + 14..].split_once('/').map(|(_, rest)| rest.to_owned());
    }
    // A crate fetched from a repository: what follows its name and its commit.
    if let Some(at) = path.find("/git/checkouts/") {
        return path[at + 15..].splitn(3, '/').nth(2).map(str::to_owned);
    }
    // The program's own crates and the copies of others it carries.
    for folder in ["crates/", "third_party/"] {
        if path.starts_with(folder) {
            return Some(path);
        }
        if let Some(at) = path.rfind(&format!("/{folder}")) {
            return Some(path[at + 1..].to_owned());
        }
    }
    None
}

/// A crash's place, `path:line:column`, cut down to the part inside the program's source; or
/// nothing when it has another shape.
pub fn location(place: &str) -> Option<String> {
    let mut parts = place.trim().rsplitn(3, ':');
    let (column, line, path) = (parts.next()?, parts.next()?, parts.next()?);
    let cut = format!("{}:{line}:{column}", source_path(path)?);
    (check::is_location(&cut) && check::free_text(&cut, 180, false, false).is_ok()).then_some(cut)
}

/// A backtrace with every path cut down as a crash's place is; a line whose path has another
/// shape, or that holds anything but plain ASCII, is left out. Kept from its top (where the
/// crash was) down to the format's limit. Nothing when what is left still fails the check.
pub fn backtrace(text: &str) -> Option<String> {
    let mut out = String::new();
    for line in text.lines() {
        let kept = match line.trim_start().strip_prefix("at ") {
            Some(place) => match location(place) {
                Some(place) => format!("             at {place}"),
                None => continue,
            },
            None => line.trim_end().to_owned(),
        };
        if kept.is_empty() || check::free_text(&kept, 2000, false, false).is_err() {
            continue;
        }
        if out.len() + kept.len() + 1 > check::CRASH_BACKTRACE {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&kept);
    }
    (!out.is_empty() && check::free_text(&out, check::CRASH_BACKTRACE, true, false).is_ok()).then_some(out)
}

/// A thread's name, or nothing when it has characters the format doesn't take.
pub fn thread(name: &str) -> Option<String> {
    check::is_thread(name).then(|| name.to_owned())
}

/// The name of a processor, a graphics card, a driver or a system version: any character the
/// format doesn't take becomes a space, and it is cut to the format's length. Nothing when
/// nothing is left.
pub fn hardware_name(name: &str) -> Option<String> {
    let plain: String = name.chars().map(|c| if check::hardware_char(c) { c } else { ' ' }).collect();
    let joined = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = joined.chars().take(check::HARDWARE_LENGTH).collect();
    let cut = cut.trim_end().to_owned();
    (!cut.is_empty()).then_some(cut)
}
