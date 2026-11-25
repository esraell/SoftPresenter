//! Checks an anonymous report against its format, field by field, exactly as the server that
//! takes it does (`docs/anonymous-reports.md` says the format in words). The app runs it on
//! every report as the last thing before the report is kept for sending: one that doesn't pass
//! is thrown away, never sent. The tests run it on the fixtures the server's own test reads
//! (`testdata/anonymous-report-fixtures.json`), so the two can't drift apart.
//!
//! The format is closed: a field that is not listed here, at any level, fails the whole report.
//! The only free text is a crash's message and backtrace and the problem lines, and those may
//! hold only plain ASCII and nothing that looks like a home folder, a network or e-mail
//! address, a link or a key.

use serde_json::{Map, Value};

pub const DISPLAYS: usize = 16;
pub const MINUTES: usize = 30;
pub const EVENTS: usize = 200;
pub const FEATURES: usize = 100;
pub const PROBLEMS: usize = 20;
pub const PROBLEM_LENGTH: usize = 300;
pub const SCENE_CODES: usize = 12;
pub const CRASH_MESSAGE: usize = 500;
pub const CRASH_BACKTRACE: usize = 16_000;
/// The longest a hardware name may be.
pub const HARDWARE_LENGTH: usize = 80;

type Checked = Result<(), String>;

fn hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

/// Letters, digits and `_`: what a word is made of.
fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// 32 lowercase hex digits.
pub fn is_hex32(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(lower_hex)
}

/// A name the app made up for a kind of thing: a view, a tool, an event, what is on screen.
pub fn is_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=32).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// A notice's id, which may have dots: `media.missing`.
fn is_id(text: &str) -> bool {
    let mut parts = text.split('.');
    let small = |part: &str| part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    let Some(first) = parts.next() else { return false };
    let rest: Vec<&str> = parts.collect();
    first.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && small(first)
        && rest.len() <= 5
        && rest.iter().all(|part| !part.is_empty() && small(part))
}

pub fn is_version(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=40).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
}

pub fn is_commit(text: &str) -> bool {
    text == "unknown" || ((7..=40).contains(&text.len()) && text.bytes().all(lower_hex))
}

/// A character the name of a processor, a graphics card, a driver or a system version may hold.
pub fn hardware_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || " ._()@/+,:#-".contains(c)
}

fn is_hardware(text: &str) -> bool {
    text.len() <= HARDWARE_LENGTH && text.chars().all(hardware_char)
}

/// `en`, `ru`, `pt-BR`.
pub fn is_language(text: &str) -> bool {
    let (first, second) = match text.split_once('-') {
        Some((first, second)) => (first, Some(second)),
        None => (text, None),
    };
    (2..=3).contains(&first.len())
        && first.bytes().all(|b| b.is_ascii_lowercase())
        && second.is_none_or(|part| (2..=8).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn is_arch(text: &str) -> bool {
    (1..=16).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

pub fn is_thread(text: &str) -> bool {
    (1..=40).contains(&text.len())
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'_' | b'.' | b'-'))
}

/// A place in the program's own source: `crates/sp-app/src/main.rs:10:5`. Never from the root
/// of a disk, except Rust's own library, which every build names `/rustc/<commit>/library/…`.
pub fn is_location(text: &str) -> bool {
    let mut parts = text.rsplitn(3, ':');
    let (Some(column), Some(line), Some(path)) = (parts.next(), parts.next(), parts.next()) else { return false };
    let digits = |part: &str, most: usize| (1..=most).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(column, 5) || !digits(line, 7) {
        return false;
    }
    let path = match path.strip_prefix("/rustc/") {
        Some(rest) => {
            let bytes = rest.as_bytes();
            if bytes.len() < 41 || !bytes[..40].iter().all(|b| lower_hex(*b)) || bytes[40] != b'/' {
                return false;
            }
            &rest[41..]
        }
        None => path,
    };
    let bytes = path.as_bytes();
    (1..=160).contains(&bytes.len())
        && word(bytes[0])
        && bytes.iter().all(|b| word(*b) || matches!(b, b'.' | b'/' | b'-'))
}

// ---------------------------------------------------------------------------------------------
// What free text may never hold

/// True when `needle` stands at `at` in `bytes`.
fn stands(bytes: &[u8], at: usize, needle: &[u8]) -> bool {
    bytes.len() >= at + needle.len() && &bytes[at..at + needle.len()] == needle
}

/// A home or disk folder: `/Users/…`, `/home/…`, `C:\…`, `\\server\…`, or `Users/name/…`.
fn holds_folder(bytes: &[u8]) -> bool {
    const ROOTS: [&[u8]; 7] = [b"/Users/", b"/home/", b"/root/", b"/Volumes/", b"/mnt/", b"/media/", b"\\Users\\"];
    for at in 0..bytes.len() {
        let edge = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        if edge {
            if ROOTS.iter().any(|root| stands(bytes, at, root)) {
                return true;
            }
            if bytes[at].is_ascii_alphabetic() && stands(bytes, at + 1, b":\\") {
                return true;
            }
            if stands(bytes, at, b"\\\\") && bytes.get(at + 2).is_some_and(u8::is_ascii_alphanumeric) {
                return true;
            }
        }
        // `Users/name/` or `home/name/` with no root before it.
        let loose = at == 0 || !(word(bytes[at - 1]) || matches!(bytes[at - 1], b'.' | b'-'));
        if loose {
            for name in [&b"Users/"[..], &b"home/"[..]] {
                if stands(bytes, at, name) {
                    let rest = &bytes[at + name.len()..];
                    let run = rest.iter().take_while(|b| **b != b'/' && !b.is_ascii_whitespace()).count();
                    if run >= 1 && rest.get(run) == Some(&b'/') {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// `name@host.example`.
fn holds_email(bytes: &[u8]) -> bool {
    bytes.iter().enumerate().any(|(at, byte)| {
        if *byte != b'@' || at == 0 {
            return false;
        }
        let before = bytes[at - 1];
        if !(before.is_ascii_alphanumeric() || b"._%+-".contains(&before)) {
            return false;
        }
        let rest = &bytes[at + 1..];
        let host = rest.iter().take_while(|b| b.is_ascii_alphanumeric() || **b == b'-').count();
        (1..=63).contains(&host)
            && rest.get(host) == Some(&b'.')
            && rest.get(host + 1).is_some_and(u8::is_ascii_alphabetic)
            && rest.get(host + 2).is_some_and(u8::is_ascii_alphabetic)
    })
}

/// Four numbers of up to three digits with dots between, standing alone.
fn holds_ipv4(bytes: &[u8]) -> bool {
    let part = |b: &u8| b.is_ascii_digit() || *b == b'.';
    bytes.split(|b| !part(b)).any(|run| {
        let groups: Vec<&[u8]> = run.split(|b| *b == b'.').collect();
        groups.len() == 4 && groups.iter().all(|group| (1..=3).contains(&group.len()))
    })
}

/// A hardware address: six pairs of hex digits with `:` or `-` between.
fn holds_mac(bytes: &[u8]) -> bool {
    (0..bytes.len()).any(|at| {
        if at > 0 && hex(bytes[at - 1]) {
            return false;
        }
        let Some(run) = bytes.get(at..at + 17) else { return false };
        let shaped = run.iter().enumerate().all(|(i, b)| if i % 3 == 2 { matches!(b, b':' | b'-') } else { hex(*b) });
        shaped && bytes.get(at + 17).is_none_or(|b| !hex(*b))
    })
}

/// `scheme://`.
fn holds_link(bytes: &[u8]) -> bool {
    (1..bytes.len()).any(|at| {
        stands(bytes, at, b"://") && (bytes[at - 1].is_ascii_alphanumeric() || b"+.-".contains(&bytes[at - 1]))
    })
}

/// How many hex digits stand at `at`, going on, and going back from before `at`.
fn hex_on(bytes: &[u8], at: usize) -> usize {
    bytes[at.min(bytes.len())..].iter().take_while(|b| hex(**b)).count()
}

fn hex_back(bytes: &[u8], at: usize) -> usize {
    bytes[..at].iter().rev().take_while(|b| hex(**b)).count()
}

/// An IPv6 address: a short form (`fe80::1`), a bracketed one (`[::1]`), or six or more groups.
/// A time of day (`12:30:45`) is none of these.
fn holds_ipv6(bytes: &[u8]) -> bool {
    // Short forms: groups, `::`, a group.
    for at in 0..bytes.len() {
        if !stands(bytes, at, b"::") {
            continue;
        }
        let after = hex_on(bytes, at + 2);
        if !(1..=4).contains(&after) || bytes.get(at + 2 + after).is_some_and(|b| word(*b)) {
            continue;
        }
        let mut end = at;
        let mut groups = 0;
        let found = loop {
            let run = hex_back(bytes, end);
            if !(1..=4).contains(&run) {
                break false;
            }
            groups += 1;
            let start = end - run;
            match start.checked_sub(1).map(|before| bytes[before]) {
                None => break true,
                Some(b':') if groups < 7 => end = start - 1,
                Some(b':') => break false,
                Some(before) => break !word(before),
            }
        };
        if found {
            return true;
        }
    }
    // Bracketed.
    for at in 0..bytes.len() {
        if bytes[at] == b'[' {
            let run = bytes[at + 1..].iter().take_while(|b| hex(**b) || **b == b':').count();
            if (2..=39).contains(&run) && bytes.get(at + 1 + run) == Some(&b']') {
                return true;
            }
        }
    }
    // Six groups or more.
    for at in 0..bytes.len() {
        if at > 0 && (word(bytes[at - 1]) || bytes[at - 1] == b':') {
            continue;
        }
        let (mut here, mut groups) = (at, 0);
        loop {
            let run = hex_on(bytes, here);
            if !(1..=4).contains(&run) {
                break;
            }
            groups += 1;
            here += run;
            match bytes.get(here) {
                Some(b':') => here += 1,
                Some(next) if word(*next) => {
                    // The last run is part of a longer word: it is no group.
                    groups -= 1;
                    break;
                }
                _ => break,
            }
        }
        if groups >= 6 {
            return true;
        }
    }
    false
}

/// A long run of letters and digits mixed: a key, a token, a password's hash.
fn holds_secret(bytes: &[u8]) -> bool {
    bytes
        .split(|b| !(word(*b) || *b == b'-'))
        .any(|run| run.len() >= 40 && run.iter().any(u8::is_ascii_digit) && run.iter().any(u8::is_ascii_alphabetic))
}

/// Free text: plain ASCII within `most`, and nothing that looks like something of the person's
/// own. `secrets` adds the two checks that a backtrace's long names and `::` would set off:
/// keys and IPv6 addresses. Says what is wrong, never repeating the text.
pub fn free_text(text: &str, most: usize, lines: bool, secrets: bool) -> Checked {
    if text.chars().count() > most {
        return Err(format!("is longer than {most} characters"));
    }
    let bytes = text.as_bytes();
    if !bytes.iter().all(|b| (0x20..=0x7e).contains(b) || (lines && *b == b'\n')) {
        return Err("may hold only plain ASCII text".to_owned());
    }
    let found = if holds_folder(bytes) {
        Some("a home or disk folder")
    } else if holds_email(bytes) {
        Some("an e-mail address")
    } else if holds_ipv4(bytes) || holds_mac(bytes) || (secrets && holds_ipv6(bytes)) {
        Some("a network address")
    } else if holds_link(bytes) {
        Some("a link")
    } else if secrets && holds_secret(bytes) {
        Some("a key")
    } else {
        None
    };
    match found {
        Some(what) => Err(format!("holds what looks like {what}")),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------
// The fields

fn at(path: &str, said: Result<(), String>) -> Checked {
    said.map_err(|why| format!("{path} {why}"))
}

/// An object with no fields beyond `allowed`.
fn closed<'a>(value: &'a Value, path: &str, allowed: &[&str]) -> Result<&'a Map<String, Value>, String> {
    let object = value.as_object().ok_or_else(|| format!("{path} must be an object"))?;
    // The field's own name is not repeated: it is not one of the format's.
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{path} has a field the format does not take"));
    }
    Ok(object)
}

/// The object at `key`, closed; nothing when it is absent, unless it is required.
fn closed_in<'a>(
    parent: &'a Map<String, Value>,
    key: &str,
    path: &str,
    allowed: &[&str],
    required: bool,
) -> Result<Option<&'a Map<String, Value>>, String> {
    match parent.get(key) {
        None if required => Err(format!("{path} is required")),
        None => Ok(None),
        Some(value) => closed(value, path, allowed).map(Some),
    }
}

fn text<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
    shaped: impl Fn(&str) -> bool,
    required: bool,
) -> Result<Option<&'a str>, String> {
    match object.get(key) {
        None if required => Err(format!("{path} is required")),
        None => Ok(None),
        Some(Value::String(text)) if shaped(text) => Ok(Some(text)),
        Some(Value::String(_)) => Err(format!("{path} has characters, a length or a shape that is not allowed")),
        Some(_) => Err(format!("{path} must be a string")),
    }
}

fn one_of(object: &Map<String, Value>, key: &str, path: &str, values: &[&str], required: bool) -> Checked {
    match object.get(key) {
        None if required => Err(format!("{path} is required")),
        None => Ok(()),
        Some(Value::String(text)) if values.contains(&text.as_str()) => Ok(()),
        Some(_) => Err(format!("{path} must be one of: {}", values.join(", "))),
    }
}

fn number_value(value: &Value, path: &str, whole: bool, max: f64, min: f64) -> Checked {
    let ok = value.as_f64().is_some_and(|n| n.is_finite() && n >= min && n <= max && (!whole || n.fract() == 0.0));
    if ok && value.is_number() {
        Ok(())
    } else {
        Err(format!("{path} must be a {}number from {min} to {max}", if whole { "whole " } else { "" }))
    }
}

fn number(object: &Map<String, Value>, key: &str, path: &str, whole: bool, max: f64) -> Checked {
    object.get(key).map_or(Ok(()), |value| number_value(value, path, whole, max, 0.0))
}

fn required_number(object: &Map<String, Value>, key: &str, path: &str, whole: bool, max: f64, min: f64) -> Checked {
    match object.get(key) {
        None => Err(format!("{path} is required")),
        Some(value) => number_value(value, path, whole, max, min),
    }
}

fn boolean(object: &Map<String, Value>, key: &str, path: &str) -> Checked {
    match object.get(key) {
        None | Some(Value::Bool(_)) => Ok(()),
        Some(_) => Err(format!("{path} must be true or false")),
    }
}

fn array<'a>(parent: &'a Map<String, Value>, key: &str, path: &str, most: usize) -> Result<&'a [Value], String> {
    match parent.get(key) {
        None => Ok(&[]),
        Some(Value::Array(items)) if items.len() <= most => Ok(items),
        Some(Value::Array(_)) => Err(format!("{path} has more than {most} items")),
        Some(_) => Err(format!("{path} must be an array")),
    }
}

/// `[average, highest]`.
fn pair(object: &Map<String, Value>, key: &str, path: &str, max: f64) -> Checked {
    match object.get(key) {
        None => Ok(()),
        Some(Value::Array(both)) if both.len() == 2 => {
            number_value(&both[0], path, false, max, 0.0)?;
            number_value(&both[1], path, false, max, 0.0)
        }
        Some(_) => Err(format!("{path} must be [average, highest]")),
    }
}

fn free(value: &Value, path: &str, most: usize, lines: bool, secrets: bool) -> Checked {
    match value {
        Value::String(text) => at(path, free_text(text, most, lines, secrets)),
        _ => Err(format!("{path} must be a string")),
    }
}

/// A time in milliseconds: under three hours.
const MS: f64 = 1e7;
const COUNT: f64 = 1e9;

fn minute(value: &Value, path: &str) -> Checked {
    let row = closed(
        value,
        path,
        &["at", "up_s", "ticks", "cpu", "mem_mb", "ui", "screens", "scene", "actions", "outputs_on"],
    )?;
    required_number(row, "at", &format!("{path}.at"), true, 1e11, 0.0)?;
    number(row, "up_s", &format!("{path}.up_s"), true, 1e8)?;
    required_number(row, "ticks", &format!("{path}.ticks"), true, 60.0, 1.0)?;
    if let Some(cpu) = closed_in(row, "cpu", &format!("{path}.cpu"), &["app", "video", "system"], false)? {
        for key in ["app", "video", "system"] {
            pair(cpu, key, &format!("{path}.cpu.{key}"), 1e5)?;
        }
    }
    if let Some(mem) = closed_in(row, "mem_mb", &format!("{path}.mem_mb"), &["app", "video", "free_least"], false)? {
        pair(mem, "app", &format!("{path}.mem_mb.app"), 1e8)?;
        pair(mem, "video", &format!("{path}.mem_mb.video"), 1e8)?;
        if mem.get("free_least").is_some_and(|free| !free.is_null()) {
            number(mem, "free_least", &format!("{path}.mem_mb.free_least"), false, 1e8)?;
        }
    }
    let ui_keys = ["view", "frames", "p50", "p95", "max", "over50", "over100"];
    if let Some(ui) = closed_in(row, "ui", &format!("{path}.ui"), &ui_keys, false)? {
        text(ui, "view", &format!("{path}.ui.view"), is_code, false)?;
        for key in ["frames", "over50", "over100"] {
            number(ui, key, &format!("{path}.ui.{key}"), true, COUNT)?;
        }
        for key in ["p50", "p95", "max"] {
            number(ui, key, &format!("{path}.ui.{key}"), false, MS)?;
        }
    }
    let screens_keys = ["passes", "p50", "p95", "max", "over50", "over100", "shown", "not_shown"];
    if let Some(screens) = closed_in(row, "screens", &format!("{path}.screens"), &screens_keys, false)? {
        for key in ["passes", "over50", "over100", "shown", "not_shown"] {
            number(screens, key, &format!("{path}.screens.{key}"), true, COUNT)?;
        }
        for key in ["p50", "p95", "max"] {
            number(screens, key, &format!("{path}.screens.{key}"), false, MS)?;
        }
    }
    for (i, code) in array(row, "scene", &format!("{path}.scene"), SCENE_CODES)?.iter().enumerate() {
        if !code.as_str().is_some_and(is_code) {
            return Err(format!("{path}.scene[{i}] must be a code"));
        }
    }
    if let Some(actions) = closed_in(row, "actions", &format!("{path}.actions"), &["keys", "clicks", "remote"], false)?
    {
        for key in ["keys", "clicks", "remote"] {
            number(actions, key, &format!("{path}.actions.{key}"), true, COUNT)?;
        }
    }
    number(row, "outputs_on", &format!("{path}.outputs_on"), true, 64.0)
}

fn event(value: &Value, path: &str) -> Checked {
    let event = closed(value, path, &["t", "kind", "id", "n", "ms"])?;
    required_number(event, "t", &format!("{path}.t"), false, 1e8, 0.0)?;
    text(event, "kind", &format!("{path}.kind"), is_code, true)?;
    text(event, "id", &format!("{path}.id"), |id| id.len() <= 64 && is_id(id), false)?;
    number(event, "n", &format!("{path}.n"), false, COUNT)?;
    number(event, "ms", &format!("{path}.ms"), false, MS)
}

const TOP: [&str; 21] = [
    "v",
    "kind",
    "install_id",
    "session",
    "sent_at",
    "app",
    "os",
    "cpu",
    "ram_gb",
    "gpu",
    "displays",
    "outputs",
    "integrations",
    "language",
    "uptime_s",
    "minutes",
    "events",
    "features",
    "problems",
    "crash",
    "dropped",
];

/// Checks a whole report. An error says which field is wrong and how, never what it held.
pub fn report(report: &Value) -> Checked {
    let top = closed(report, "the report", &TOP)?;
    if top.get("v").and_then(Value::as_f64) != Some(1.0) {
        return Err("unsupported v (the format's version is 1)".to_owned());
    }
    one_of(top, "kind", "kind", &["report", "crash"], true)?;
    let kind = top.get("kind").and_then(Value::as_str).unwrap_or_default();
    text(top, "install_id", "install_id", is_hex32, true)?;
    text(top, "session", "session", is_hex32, false)?;
    required_number(top, "sent_at", "sent_at", true, 1e11, 0.0)?;

    let app = closed_in(top, "app", "app", &["version", "commit", "channel"], true)?.unwrap_or(top);
    text(app, "version", "app.version", is_version, true)?;
    text(app, "commit", "app.commit", is_commit, false)?;
    one_of(app, "channel", "app.channel", &["release", "dev"], true)?;

    let os = closed_in(top, "os", "os", &["name", "version", "arch"], true)?.unwrap_or(top);
    one_of(os, "name", "os.name", &["macos", "windows", "linux"], true)?;
    text(os, "version", "os.version", is_hardware, false)?;
    text(os, "arch", "os.arch", is_arch, false)?;

    if let Some(cpu) = closed_in(top, "cpu", "cpu", &["model", "cores"], false)? {
        text(cpu, "model", "cpu.model", is_hardware, false)?;
        number(cpu, "cores", "cpu.cores", true, 4096.0)?;
    }
    number(top, "ram_gb", "ram_gb", true, 65536.0)?;

    if let Some(gpu) = closed_in(top, "gpu", "gpu", &["name", "driver", "backend", "type"], false)? {
        text(gpu, "name", "gpu.name", is_hardware, false)?;
        text(gpu, "driver", "gpu.driver", is_hardware, false)?;
        one_of(gpu, "backend", "gpu.backend", &["metal", "vulkan", "dx12", "gl", "other"], false)?;
        one_of(gpu, "type", "gpu.type", &["integrated", "discrete", "virtual", "cpu", "other"], false)?;
    }

    for (i, value) in array(top, "displays", "displays", DISPLAYS)?.iter().enumerate() {
        let path = format!("displays[{i}]");
        let display = closed(value, &path, &["w", "h", "scale", "hz", "main"])?;
        required_number(display, "w", &format!("{path}.w"), true, 65536.0, 1.0)?;
        required_number(display, "h", &format!("{path}.h"), true, 65536.0, 1.0)?;
        number(display, "scale", &format!("{path}.scale"), false, 16.0)?;
        number(display, "hz", &format!("{path}.hz"), false, 1000.0)?;
        boolean(display, "main", &format!("{path}.main"))?;
    }

    let output_kinds = ["audience", "stage", "on", "display", "window", "ndi", "decklink"];
    if let Some(outputs) = closed_in(top, "outputs", "outputs", &output_kinds, false)? {
        for key in output_kinds {
            number(outputs, key, &format!("outputs.{key}"), true, 64.0)?;
        }
    }
    if let Some(integrations) = closed_in(top, "integrations", "integrations", &["ndi", "decklink", "ffmpeg"], false)? {
        for key in ["ndi", "decklink", "ffmpeg"] {
            boolean(integrations, key, &format!("integrations.{key}"))?;
        }
    }
    text(top, "language", "language", |language| language.len() <= 12 && is_language(language), false)?;
    number(top, "uptime_s", "uptime_s", true, 1e8)?;

    for (i, row) in array(top, "minutes", "minutes", MINUTES)?.iter().enumerate() {
        minute(row, &format!("minutes[{i}]"))?;
    }
    for (i, row) in array(top, "events", "events", EVENTS)?.iter().enumerate() {
        event(row, &format!("events[{i}]"))?;
    }
    if let Some(features) = top.get("features") {
        let features = features.as_object().ok_or("features must be an object")?;
        if features.len() > FEATURES {
            return Err(format!("features has more than {FEATURES} entries"));
        }
        for (i, (name, count)) in features.iter().enumerate() {
            if !is_code(name) {
                return Err(format!("features: entry {} is not named with a code", i + 1));
            }
            number_value(count, &format!("features: entry {}", i + 1), true, 1e7, 0.0)?;
        }
    }
    for (i, line) in array(top, "problems", "problems", PROBLEMS)?.iter().enumerate() {
        free(line, &format!("problems[{i}]"), PROBLEM_LENGTH, false, true)?;
    }

    let crash_keys = ["message", "location", "thread", "backtrace", "uptime_s"];
    if let Some(crash) = closed_in(top, "crash", "crash", &crash_keys, kind == "crash")? {
        if kind != "crash" {
            return Err("crash goes only with kind \"crash\"".to_owned());
        }
        if let Some(message) = crash.get("message") {
            free(message, "crash.message", CRASH_MESSAGE, false, true)?;
        }
        if let Some(location) = text(crash, "location", "crash.location", |l| l.len() <= 180 && is_location(l), false)?
        {
            at("crash.location", free_text(location, 180, false, false))?;
        }
        text(crash, "thread", "crash.thread", is_thread, false)?;
        // A backtrace's long names may run past forty characters, so the key check stays off.
        if let Some(backtrace) = crash.get("backtrace") {
            free(backtrace, "crash.backtrace", CRASH_BACKTRACE, true, false)?;
        }
        number(crash, "uptime_s", "crash.uptime_s", true, 1e8)?;
    }
    if let Some(dropped) = closed_in(top, "dropped", "dropped", &["minutes", "events", "problems"], false)? {
        for key in ["minutes", "events", "problems"] {
            number(dropped, key, &format!("dropped.{key}"), true, COUNT)?;
        }
    }
    Ok(())
}
