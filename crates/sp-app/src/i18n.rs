//! The app's own words in other languages (English, Spanish, Russian, Ukrainian). Songs, Bible
//! text and Bible book names are content and keep their own languages; this is only what the
//! app itself says: buttons, menus, tooltips, notices. `docs/translating.md` is the guide for
//! whoever wraps the next area.
//!
//! # Wrapping a string
//!
//! English stays in the code, as the source text and the fallback, and the key says *where* the
//! words are, not what they are, so one English word that needs two translations ("Back" for
//! going back and for sending an object behind others) gets two keys:
//!
//! ```ignore
//! ui.label(t("right.clear_all", "Clear All"));
//! let said = tf("left.added", "Added {item} to {list}.", &[("item", &song), ("list", &name)]);
//! let songs = tn("left.songs", count, "{n} song", "{n} songs", &[]);
//! ```
//!
//! A key starts with the name of its area's table (`right.` is in `locales/<language>/right.json`)
//! so that agents working on different areas never meet. A language's table is a flat JSON
//! object: a key and its words, or, for a plural, its forms in an array (two for English and
//! Spanish: one and other; three for Russian and Ukrainian: one, few and many). Anything a table
//! lacks is read in English, so a half-translated language still works.
//!
//! # The current language
//!
//! The choice is saved with the settings (`auto`, `en`, `es`, `ru` or `uk`); `auto` follows the
//! computer's language. Every call looks the language up as it runs, so a change shows at once
//! in the next drawing, with no restart.
//!
//! The tables are `locales/<language>/<area>.json`, listed by `build.rs` and included in the
//! program; `format.json` of each language holds its number and quotation rules.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/locales.rs"));
}

/// A language the app's words exist in (or will).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    English,
    Spanish,
    Russian,
    Ukrainian,
}

impl Language {
    pub const ALL: [Language; 4] = [Language::English, Language::Spanish, Language::Russian, Language::Ukrainian];

    /// The short code settings and tables use.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
            Language::Russian => "ru",
            Language::Ukrainian => "uk",
        }
    }

    /// The language a code (or a system's language tag such as `es-ES`, `ru_RU.UTF-8`) names.
    pub fn from_tag(tag: &str) -> Option<Language> {
        let first = tag.trim().split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
        match first.as_str() {
            "en" | "eng" => Some(Language::English),
            "es" | "spa" => Some(Language::Spanish),
            "ru" | "rus" => Some(Language::Russian),
            "uk" | "ukr" => Some(Language::Ukrainian),
            _ => None,
        }
    }

    /// The language's own name, always written in itself, never translated.
    pub fn own_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Spanish => "Español",
            Language::Russian => "Русский",
            Language::Ukrainian => "Українська",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    /// Which form of a counted word `n` takes: 0 is the first of the table's forms. English and
    /// Spanish have two (1, then the rest); Russian and Ukrainian three (1, 21, 31...; 2 to 4,
    /// 22 to 24...; the rest, and 11 to 14).
    pub fn plural_form(self, n: u64) -> usize {
        match self {
            Language::English | Language::Spanish => usize::from(n != 1),
            Language::Russian | Language::Ukrainian => {
                let (units, tens) = (n % 10, n % 100);
                if units == 1 && tens != 11 {
                    0
                } else if (2..=4).contains(&units) && !(12..=14).contains(&tens) {
                    1
                } else {
                    2
                }
            }
        }
    }

    /// How many forms a plural has in this language.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn plural_forms(self) -> usize {
        match self {
            Language::English | Language::Spanish => 2,
            Language::Russian | Language::Ukrainian => 3,
        }
    }
}

/// What the Language setting is: follow the computer, or one language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Computer,
    Is(Language),
}

impl Choice {
    /// The choices in the order the Language card lists them: the computer's first.
    pub const ALL: [Choice; 5] = [
        Choice::Computer,
        Choice::Is(Language::English),
        Choice::Is(Language::Spanish),
        Choice::Is(Language::Russian),
        Choice::Is(Language::Ukrainian),
    ];

    /// What is saved: `auto`, or the language's code.
    pub fn code(self) -> &'static str {
        match self {
            Choice::Computer => "auto",
            Choice::Is(language) => language.code(),
        }
    }

    pub fn from_code(code: &str) -> Choice {
        Language::ALL.into_iter().find(|language| language.code() == code).map_or(Choice::Computer, Choice::Is)
    }
}

// The choice, as `Choice::code` numbered: 0 the computer's, then 1 to 4 the languages.
static CHOICE: AtomicU8 = AtomicU8::new(0);
// The computer's language as a number (1 to 4 the languages; 0 when not read yet, or not one of
// ours).
static COMPUTER: AtomicU8 = AtomicU8::new(0);

thread_local! {
    // A test's own language, for the thread it runs on, so tests never meet through the global.
    static OVERRIDE: Cell<Option<Language>> = const { Cell::new(None) };
}

fn number_of(language: Language) -> u8 {
    language.index() as u8 + 1
}

fn language_numbered(number: u8) -> Option<Language> {
    (number as usize).checked_sub(1).and_then(|at| Language::ALL.get(at).copied())
}

/// The setting as it is now.
pub fn choice() -> Choice {
    language_numbered(CHOICE.load(Ordering::Relaxed)).map_or(Choice::Computer, Choice::Is)
}

/// Changes the setting. The next drawing is in the new language.
pub fn set_choice(choice: Choice) {
    let number = match choice {
        Choice::Computer => 0,
        Choice::Is(language) => number_of(language),
    };
    CHOICE.store(number, Ordering::Relaxed);
}

/// The computer's language, as the system writes it (`es-ES`), once it has answered. A language
/// the app has no words in leaves English.
pub fn set_computer(tag: &str) {
    COMPUTER.store(Language::from_tag(tag).map_or(0, number_of), Ordering::Relaxed);
}

/// The language the computer is set to, when the app has words in it.
pub fn computer() -> Option<Language> {
    language_numbered(COMPUTER.load(Ordering::Relaxed))
}

/// The language the app is in now: the one chosen, or with the choice left to the computer, the
/// computer's, or English.
pub fn current() -> Language {
    if let Some(language) = OVERRIDE.with(Cell::get) {
        return language;
    }
    match choice() {
        Choice::Is(language) => language,
        Choice::Computer => computer().unwrap_or(Language::English),
    }
}

/// Runs `body` with the app in `language` on this thread only (for tests and for measuring
/// every language's wording).
#[cfg_attr(not(test), allow(dead_code))]
pub fn with_language<R>(language: Language, body: impl FnOnce() -> R) -> R {
    let before = OVERRIDE.with(|cell| cell.replace(Some(language)));
    let result = body();
    OVERRIDE.with(|cell| cell.set(before));
    result
}

/// What a table holds for a key.
enum Words {
    One(&'static str),
    Forms(Vec<&'static str>),
}

/// How a language writes numbers and quotations (`format.json`).
#[derive(Clone, Debug)]
struct Format {
    decimal: &'static str,
    group: &'static str,
    /// A whole number is grouped from this many digits (Spanish leaves four alone: 1234).
    group_from: usize,
    open: &'static str,
    close: &'static str,
}

impl Format {
    const ENGLISH: Format = Format { decimal: ".", group: ",", group_from: 4, open: "\u{201c}", close: "\u{201d}" };
}

struct Table {
    words: HashMap<&'static str, Words>,
    format: Format,
}

struct Tables {
    languages: Vec<Table>,
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

/// Reads every table once. A table that does not parse is a mistake in the program: a test
/// reads them all and says which.
fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut languages: Vec<Table> =
            Language::ALL.iter().map(|_| Table { words: HashMap::new(), format: Format::ENGLISH }).collect();
        for (code, area, text) in generated::FILES {
            let Some(language) = Language::from_tag(code) else { continue };
            let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) else {
                continue;
            };
            let table = &mut languages[language.index()];
            if *area == "format" {
                let read = |name: &str, fallback: &'static str| {
                    map.get(name).and_then(|value| value.as_str()).map_or(fallback, leak)
                };
                table.format = Format {
                    decimal: read("decimal", Format::ENGLISH.decimal),
                    group: read("group", Format::ENGLISH.group),
                    group_from: map
                        .get("group_from")
                        .and_then(serde_json::Value::as_u64)
                        .map_or(Format::ENGLISH.group_from, |digits| digits as usize),
                    open: read("quote_open", Format::ENGLISH.open),
                    close: read("quote_close", Format::ENGLISH.close),
                };
                continue;
            }
            for (key, value) in map {
                if key.starts_with('_') {
                    continue;
                }
                let words = match value {
                    serde_json::Value::String(text) => Words::One(leak(&text)),
                    serde_json::Value::Array(forms) => {
                        Words::Forms(forms.iter().filter_map(|form| form.as_str()).map(leak).collect())
                    }
                    _ => continue,
                };
                table.words.insert(leak(&key), words);
            }
        }
        Tables { languages }
    })
}

fn words(language: Language, key: &str) -> Option<&'static Words> {
    if language == Language::English {
        return None;
    }
    tables().languages[language.index()].words.get(key)
}

/// The words for `key` in the current language, or `english` where the table has none yet.
pub fn t(key: &'static str, english: &'static str) -> &'static str {
    let said = match words(current(), key) {
        Some(Words::One(text)) => text,
        // A plural asked for as plain words: its first form.
        Some(Words::Forms(forms)) => forms.first().copied().unwrap_or(english),
        None => return english,
    };
    remember_fixed(said, english);
    said
}

/// The words for a name the app gives something that is then saved (the first playlist, the
/// quick message, "Arrangement 2"), in the language set: as [`t`], but not remembered for
/// [`english_of`]. Once saved it is the church's own name, which the log and the command line
/// show as it is (turned back into English, `sp` would print a name it then can't find).
pub fn tg(key: &'static str, english: &'static str) -> &'static str {
    match words(current(), key) {
        Some(Words::One(text)) => text,
        Some(Words::Forms(forms)) => forms.first().copied().unwrap_or(english),
        None => english,
    }
}

/// [`tf`] for a name the app gives and saves (see [`tg`]): not remembered for [`english_of`].
pub fn tgf(key: &'static str, english: &'static str, args: &[(&str, &str)]) -> String {
    fill(tg(key, english), args)
}

/// What a key says in each language the app has words in, English first, for telling words the
/// app gave and saved (a new box's sample words, its numbered name) in whichever language it was
/// in when it gave them: a box made in Spanish still says "Texto" with the window in English.
/// The pair is as [`tk`] makes it.
pub fn every_wording((key, english): (&'static str, &'static str)) -> impl Iterator<Item = &'static str> {
    Language::ALL.into_iter().map(move |language| match words(language, key) {
        Some(Words::One(text)) => *text,
        Some(Words::Forms(forms)) => forms.first().copied().unwrap_or(english),
        None => english,
    })
}

/// A key and its English together, for a place that keeps them in a table before it draws
/// them (the layer keys' names): the same as [`t`]'s two arguments, but the scan for wrapped
/// keys finds them. Hand the pair on to [`ts`] or [`t`].
pub const fn tk(key: &'static str, english: &'static str) -> (&'static str, &'static str) {
    (key, english)
}

/// Like [`t`], for the places that have a budget (a layer key's ten letters, a bar's one line):
/// the table's short form (the key with `.short` after it) when it has one, else the usual
/// words.
pub fn ts(key: &'static str, english: &'static str) -> &'static str {
    short_key(key)
        .and_then(|short| match words(current(), &short) {
            Some(Words::One(text)) => {
                remember_fixed(text, english);
                Some(*text)
            }
            _ => None,
        })
        .unwrap_or_else(|| t(key, english))
}

fn short_key(key: &str) -> Option<String> {
    (current() != Language::English).then(|| format!("{key}.short"))
}

/// Puts each `{name}` of `template` in its place. A name with no value stays as it is, which
/// shows in the window as a plain mistake rather than hiding.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if args.iter().any(|(name, _)| *name == &after[..close]) => {
                let name = &after[..close];
                out.push_str(args.iter().find(|(each, _)| *each == name).map_or("", |(_, value)| value));
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A whole sentence with `{placeholders}`, so word order can change between languages:
/// `tf("left.already_in", "{item} is already in {list}.", &[("item", a), ("list", b)])`.
pub fn tf(key: &'static str, english: &'static str, args: &[(&str, &str)]) -> String {
    let template = t(key, english);
    let said = fill(template, args);
    if !std::ptr::eq(template, english) {
        remember_made(&said, || fill(english, args));
    }
    said
}

/// A counted thing in the right form: English `one` and `other` stand for the table's forms
/// where it has none. `{n}` in the words is the number, written as the language writes numbers;
/// `args` fills any others.
/// `tn("left.songs", 5, "{n} song", "{n} songs", &[])` is "5 songs", "5 canciones", "5 песен".
pub fn tn(key: &'static str, n: u64, one: &'static str, other: &'static str, args: &[(&str, &str)]) -> String {
    let language = current();
    let template = match words(language, key) {
        Some(Words::Forms(forms)) if !forms.is_empty() => forms[language.plural_form(n).min(forms.len() - 1)],
        Some(Words::One(text)) => text,
        _ if n == 1 => one,
        _ => other,
    };
    let number = count(n);
    let mut all: Vec<(&str, &str)> = vec![("n", number.as_str())];
    all.extend_from_slice(args);
    let said = fill(template, &all);
    if language != Language::English {
        remember_made(&said, || {
            let number = group_digits(&n.to_string(), &Format::ENGLISH);
            let mut all: Vec<(&str, &str)> = vec![("n", number.as_str())];
            all.extend_from_slice(args);
            fill(if n == 1 { one } else { other }, &all)
        });
    }
    said
}

/// The English of words said in another language, kept for the places that must stay in
/// English whatever the window speaks: the log (developers read it) and the command line's
/// replies (scripts read them). Every call of [`t`], [`ts`], [`tf`] and [`tn`] in another
/// language remembers its English here; [`english_of`] gives a text back with each piece
/// it recognises put back into English. Remembering never waits: a call that finds the table
/// busy (the screens' thread, say) leaves it alone.
#[derive(Default)]
struct Shadow {
    /// Fixed words, by what they say in the language: their English.
    fixed: HashMap<&'static str, &'static str>,
    /// Sentences made with placeholders: their English, made the same way. Emptied when it
    /// grows past [`MADE_KEPT`] (a readout that changes every moment would fill it).
    made: HashMap<String, String>,
    /// Both, longest first, for [`english_of`]; made again after a change.
    sorted: Option<Vec<(String, String)>>,
}

const MADE_KEPT: usize = 4096;

fn shadow() -> &'static std::sync::Mutex<Shadow> {
    static SHADOW: OnceLock<std::sync::Mutex<Shadow>> = OnceLock::new();
    SHADOW.get_or_init(Default::default)
}

fn remember_fixed(said: &'static str, english: &'static str) {
    if said == english {
        return;
    }
    let mut shadow = shadow().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !shadow.fixed.contains_key(said) {
        shadow.fixed.insert(said, english);
        shadow.sorted = None;
    }
}

fn remember_made(said: &str, english: impl FnOnce() -> String) {
    let mut shadow = shadow().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !shadow.made.contains_key(said) {
        let english = english();
        if english == said {
            return;
        }
        if shadow.made.len() >= MADE_KEPT {
            shadow.made.clear();
        }
        shadow.made.insert(said.to_owned(), english);
        shadow.sorted = None;
    }
}

/// `text` with the app's own words in it put back into English: a whole sentence said in
/// another language, or each piece of one joined from several (a whole phrase, never part of
/// a word). Only sentences the app made with its words (`tf`, `tn`, filled with their own
/// names and numbers) and phrases of two words or more are put back; a single word never is,
/// on its own or inside a line, because a name the church chose ("Culto", "Más de ti") can be
/// the same word, and a reply must never change the church's names. Names the app gives and
/// saves (`tg`) aren't remembered at all. For the log and the command line's replies.
pub fn english_of(text: &str) -> String {
    // Waited for, not tried: the lock is only held while a translation is noted, and a
    // try that failed left a line in the language set now and then (and a test flaky).
    let mut shadow = shadow().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if shadow.fixed.is_empty() && shadow.made.is_empty() {
        return text.to_owned();
    }
    // The whole sentence when it was said as one; then any piece still in the language (a
    // placeholder filled with the app's own words, or sentences joined).
    let whole = shadow
        .made
        .get(text)
        .cloned()
        .or_else(|| shadow.fixed.get(text).filter(|_| several_words(text)).map(|english| (*english).to_owned()));
    if shadow.sorted.is_none() {
        let mut all: Vec<(String, String)> = shadow
            .fixed
            .iter()
            .filter(|(said, _)| several_words(said))
            .map(|(said, english)| ((*said).to_owned(), (*english).to_owned()))
            .chain(shadow.made.iter().map(|(said, english)| (said.clone(), english.clone())))
            .filter(|(said, _)| said.chars().count() >= 3)
            .collect();
        all.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        shadow.sorted = Some(all);
    }
    let mut out = whole.unwrap_or_else(|| text.to_owned());
    for (said, english) in shadow.sorted.iter().flatten() {
        if out.contains(said.as_str()) {
            out = replace_whole(&out, said, english);
        }
    }
    out
}

/// Whether the app's words are a phrase of two words or more, which a church's name is unlikely
/// to be the same as by chance (see [`english_of`]).
fn several_words(said: &str) -> bool {
    said.split_whitespace().nth(1).is_some()
}

/// Every place `said` stands in `text` as a whole word or phrase (not inside a longer word),
/// replaced by `with`.
fn replace_whole(text: &str, said: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(said) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + said.len()..].chars().next();
        let bounded = !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric);
        out.push_str(&rest[..at]);
        out.push_str(if bounded { with } else { said });
        rest = &rest[at + said.len()..];
    }
    out.push_str(rest);
    out
}

fn format_of(language: Language) -> &'static Format {
    if language == Language::English {
        return &Format::ENGLISH;
    }
    &tables().languages[language.index()].format
}

/// A whole number as the current language writes it: 13,613 in English, 13.613 in Spanish,
/// 13 613 (a no-break space) in Russian and Ukrainian.
pub fn count(n: u64) -> String {
    group_digits(&n.to_string(), format_of(current()))
}

fn group_digits(digits: &str, format: &Format) -> String {
    if digits.len() < format.group_from {
        return digits.to_owned();
    }
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push_str(format.group);
        }
        out.push(digit);
    }
    out
}

/// A number with `places` decimals as the current language writes it: 0.4 or 0,4.
pub fn number(value: f64, places: usize) -> String {
    let format = format_of(current());
    let printed = format!("{:.*}", places, value.abs());
    let (whole, fraction) =
        printed.split_once('.').map_or((printed.as_str(), None), |(whole, rest)| (whole, Some(rest)));
    let mut out = String::new();
    if value < 0.0 && printed.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('\u{2212}');
    }
    out.push_str(&group_digits(whole, format));
    if let Some(fraction) = fraction {
        out.push_str(format.decimal);
        out.push_str(fraction);
    }
    out
}

/// A length of time in seconds with its unit, as the language writes it: "0.4 s", "0,4 s",
/// "0,4 с". The unit is the table's `common.seconds_unit`.
pub fn seconds(value: f64) -> String {
    let places = if (value - value.round()).abs() < 0.005 { 0 } else { 1 };
    format!("{} {}", number(value, places), t("common.seconds_unit", "s"))
}

/// Puts a name or phrase in the language's quotation marks (“…” in English and Spanish, «…» in
/// Russian and Ukrainian).
pub fn quoted(text: &str) -> String {
    let format = format_of(current());
    format!("{}{text}{}", format.open, format.close)
}

/// Every key the program asks for in `source` (the text of a Rust file), with its English:
/// the calls of [`t`], [`ts`], [`tf`] and [`tn`] whose key is a string. For the tests that list
/// what a language still lacks.
#[cfg(test)]
pub fn wrapped_in(source: &str) -> Vec<(String, String)> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    for name in ["t", "ts", "tf", "tn", "tk", "tg", "tgf"] {
        let pattern = format!("{name}(");
        let mut from = 0;
        while let Some(hit) = source[from..].find(&pattern) {
            let start = from + hit;
            from = start + pattern.len();
            let before = start.checked_sub(1).map(|at| bytes[at]);
            // `t(`, not the end of `format(` or `split(`; `i18n::t(` and `crate::i18n::t(` count.
            if before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
                continue;
            }
            let mut rest = &source[from..];
            let Some((key, after)) = string_literal(rest) else { continue };
            if !key.contains('.') || key.contains(' ') {
                continue;
            }
            rest = after;
            // English is the second literal for `t`, `ts` and `tf`; `tn` gives the number first.
            let english = if name == "tn" {
                rest.find(',')
                    .and_then(|at| rest[at + 1..].find(',').map(|more| &rest[at + 1 + more + 1..]))
                    .and_then(|tail| string_literal(tail).map(|(text, _)| text))
            } else {
                rest.trim_start().strip_prefix(',').and_then(|tail| string_literal(tail).map(|(text, _)| text))
            };
            found.push((key, english.unwrap_or_default()));
        }
    }
    found
}

/// The string literal that starts `text` after any spaces or line breaks, and what follows it.
#[cfg(test)]
fn string_literal(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let body = text.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Some((out, &body[at + 1..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                'u' => {
                    // `\u{2019}`
                    let mut digits = String::new();
                    for (_, d) in chars.by_ref() {
                        if d == '{' {
                            continue;
                        }
                        if d == '}' {
                            break;
                        }
                        digits.push(d);
                    }
                    out.push(u32::from_str_radix(&digits, 16).ok().and_then(char::from_u32)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Every key the program wraps, with its English, from the source files of this crate.
#[cfg(test)]
pub fn wrapped_keys() -> std::collections::BTreeMap<String, String> {
    fn visit(dir: &std::path::Path, found: &mut std::collections::BTreeMap<String, String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                for (key, english) in wrapped_in(&text).into_iter().filter(|(key, _)| !key.starts_with("no.such")) {
                    found.entry(key).or_insert(english);
                }
            }
        }
    }
    let mut found = std::collections::BTreeMap::new();
    visit(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
    found
}

/// The keys the program wraps that `language` has no words for yet, with their English.
#[cfg(test)]
pub fn missing(language: Language) -> Vec<(String, String)> {
    wrapped_keys()
        .into_iter()
        .filter(|(key, _)| {
            language == Language::English || !tables().languages[language.index()].words.contains_key(key.as_str())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_word_is_read_in_english() {
        for language in Language::ALL {
            with_language(language, || {
                assert_eq!(t("no.such.key", "Nothing here"), "Nothing here");
                assert_eq!(tf("no.such.key", "Added {n}", &[("n", "3")]), "Added 3");
            });
        }
        // And English itself reads the code, whatever a table says.
        with_language(Language::English, || assert_eq!(t("right.clear_all", "Clear All"), "Clear All"));
    }

    #[test]
    fn a_table_word_is_used_and_a_short_form_where_there_is_one() {
        with_language(Language::Spanish, || {
            assert_eq!(t("right.clear_all", "Clear All"), "Borrar todo");
            assert_eq!(ts("right.clear_all", "Clear All"), "Borrar todo", "no short form: the usual words");
            assert_eq!(ts("right.layer.slide", "Slide"), "Diapo.", "the short form");
            assert_eq!(t("right.layer.slide", "Slide"), "Diapositiva");
        });
        with_language(Language::English, || assert_eq!(ts("right.layer.slide", "Slide"), "Slide"));
    }

    #[test]
    fn placeholders_are_filled_where_the_language_puts_them() {
        assert_eq!(fill("{song} is in {list}.", &[("list", "Sunday"), ("song", "Grace")]), "Grace is in Sunday.");
        assert_eq!(fill("{a} and {a}", &[("a", "x")]), "x and x");
        assert_eq!(
            fill("{unknown} {", &[("a", "x")]),
            "{unknown} {",
            "what has no value stays, and so does a lone brace"
        );
        assert_eq!(fill("{n}{n}", &[("n", "1")]), "11");
        with_language(Language::Spanish, || {
            assert_eq!(
                tf("left.added", "Added {item} to {list}.", &[("item", "Gracia"), ("list", "Domingo")]),
                "Se añadió Gracia a Domingo."
            );
            assert_eq!(
                tn(
                    "left.added_songs",
                    3,
                    "Added {n} song to {list}.",
                    "Added {n} songs to {list}.",
                    &[("list", "Domingo")]
                ),
                "Se añadieron 3 canciones a Domingo."
            );
        });
    }

    #[test]
    fn the_log_and_the_command_line_get_the_english_back() {
        with_language(Language::Spanish, || {
            let said = tf("left.added", "Added {item} to {list}.", &[("item", "Gracia"), ("list", "Domingo")]);
            assert_eq!(said, "Se añadió Gracia a Domingo.");
            assert_eq!(english_of(&said), "Added Gracia to Domingo.", "a whole sentence");
            let songs = tn("left.songs", 1234, "{n} song", "{n} songs", &[]);
            let joined = format!("{}: {songs}", t("right.clear_all", "Clear All"));
            assert_eq!(english_of(&joined), "Clear All: 1,234 songs", "pieces, numbers written the English way");
            let untitled = t("left.untitled", "Untitled");
            let inside = tf("left.added", "Added {item} to {list}.", &[("item", untitled), ("list", "Domingo")]);
            assert_eq!(english_of(&inside), "Added Untitled to Domingo.", "a placeholder filled with the app's words");
        });
        assert_eq!(english_of("Borrar todos los días"), "Borrar todos los días", "never part of a word");
        with_language(Language::Spanish, || {
            // A single word the app says ("Culto", the Service tab) is never put back inside a
            // line or alone: a playlist the church calls Culto, or a song "Más de ti", stays.
            let _ = t("topbar.view.service", "Service");
            let _ = t("dock.advanced", "Advanced");
            assert_eq!(english_of("1. Culto (3 items)"), "1. Culto (3 items)");
            assert_eq!(english_of("Culto"), "Culto");
            assert_eq!(english_of("Más de ti"), "Más de ti");
            // A name the app gives and saves isn't remembered, so it prints as saved.
            let quick = tg("tools.message.quick.name", "Quick message");
            assert_eq!(english_of(&format!("Showing {quick}.")), format!("Showing {quick}."));
        });
        assert_eq!(replace_whole("todo, todos y todo", "todo", "all"), "all, todos y all");
    }

    #[test]
    fn plural_forms_follow_the_language() {
        let forms = |language: Language, counts: &[u64]| -> Vec<usize> {
            counts.iter().map(|n| language.plural_form(*n)).collect()
        };
        assert_eq!(forms(Language::English, &[0, 1, 2, 21]), [1, 0, 1, 1]);
        assert_eq!(forms(Language::Spanish, &[0, 1, 2, 21]), [1, 0, 1, 1]);
        for language in [Language::Russian, Language::Ukrainian] {
            assert_eq!(
                forms(language, &[1, 2, 4, 5, 11, 12, 14, 20, 21, 22, 25, 101, 111, 112]),
                [0, 1, 1, 2, 2, 2, 2, 2, 0, 1, 2, 0, 2, 2],
                "{language:?}"
            );
        }
    }

    #[test]
    fn counted_words_take_their_form() {
        let songs = |n| tn("left.songs", n, "{n} song", "{n} songs", &[]);
        with_language(Language::English, || assert_eq!((songs(1), songs(2)), ("1 song".into(), "2 songs".into())));
        with_language(Language::Spanish, || {
            assert_eq!(
                (songs(1), songs(0), songs(13613)),
                ("1 canción".into(), "0 canciones".into(), "13.613 canciones".into())
            );
        });
        // The tables of the Cyrillic languages come later; their rules already work.
        with_language(Language::Russian, || assert_eq!(songs(5), "5 songs", "no table yet: the English"));
        let template = |n| {
            let forms = ["{n} песня", "{n} песни", "{n} песен"];
            fill(forms[Language::Russian.plural_form(n)], &[("n", &n.to_string())])
        };
        assert_eq!(
            (template(1), template(2), template(5), template(21)),
            ("1 песня".into(), "2 песни".into(), "5 песен".into(), "21 песня".into())
        );
    }

    #[test]
    fn numbers_are_written_as_the_language_writes_them() {
        with_language(Language::English, || {
            assert_eq!(
                (count(13613), count(999), count(1234), number(0.4, 1)),
                ("13,613".into(), "999".into(), "1,234".into(), "0.4".into())
            );
        });
        with_language(Language::Spanish, || {
            assert_eq!(count(13613), "13.613");
            assert_eq!(count(1234), "1234", "Spanish leaves four figures alone");
            assert_eq!(number(0.4, 1), "0,4");
            assert_eq!(seconds(0.4), "0,4 s");
            assert_eq!(seconds(3.0), "3 s");
            assert_eq!(quoted("Amén"), "“Amén”");
        });
        // The Cyrillic languages' rules, from their format tables once those are written.
        let russian = Format { decimal: ",", group: "\u{a0}", group_from: 4, open: "«", close: "»" };
        assert_eq!(group_digits("13613", &russian), "13\u{a0}613");
    }

    #[test]
    fn the_choice_follows_the_computer_until_one_is_made() {
        // Without touching the real setting (other tests run beside this one), the rule itself.
        assert_eq!(Language::from_tag("es-ES"), Some(Language::Spanish));
        assert_eq!(Language::from_tag("es_MX.UTF-8"), Some(Language::Spanish));
        assert_eq!(Language::from_tag("ru"), Some(Language::Russian));
        assert_eq!(Language::from_tag("uk-UA"), Some(Language::Ukrainian));
        assert_eq!(Language::from_tag("de-DE"), None, "a language we have no words in: English");
        assert_eq!(Choice::from_code("auto"), Choice::Computer);
        assert_eq!(Choice::from_code("es"), Choice::Is(Language::Spanish));
        assert_eq!(Choice::from_code("klingon"), Choice::Computer);
        for choice in Choice::ALL {
            assert_eq!(Choice::from_code(choice.code()), choice);
        }
    }

    // The one test that sets the global setting; nothing else reads it without an override.
    #[test]
    fn a_change_of_language_shows_at_once() {
        set_choice(Choice::Computer);
        set_computer("es-ES");
        assert_eq!(current(), Language::Spanish, "the computer's language");
        assert_eq!(t("right.clear_all", "Clear All"), "Borrar todo");
        set_choice(Choice::Is(Language::English));
        assert_eq!(current(), Language::English);
        assert_eq!(t("right.clear_all", "Clear All"), "Clear All", "the very next call");
        set_choice(Choice::Computer);
        set_computer("de-DE");
        assert_eq!(current(), Language::English, "a computer in another language: English");
        set_computer("");
        set_choice(Choice::Computer);
    }

    #[test]
    fn the_tables_agree_with_the_program() {
        let wrapped = wrapped_keys();
        assert!(wrapped.len() > 50, "the source scan found only {}", wrapped.len());
        for language in Language::ALL.into_iter().filter(|language| *language != Language::English) {
            let table = &tables().languages[language.index()];
            for (key, words) in &table.words {
                // A short form is asked for through its usual key.
                let asked = key.strip_suffix(".short").unwrap_or(key);
                let Some(english) = wrapped.get(asked) else {
                    panic!("{language:?} has `{key}`, which no code asks for")
                };
                let area = key.split('.').next().unwrap_or("");
                assert!(
                    generated::FILES.iter().any(|(code, file, _)| *code == language.code() && file == &area),
                    "`{key}` is in no table named {area}"
                );
                // The placeholders of the English must all be in the translation.
                let names = |text: &str| {
                    let mut names: Vec<String> =
                        text.split('{').skip(1).filter_map(|rest| rest.split('}').next()).map(str::to_owned).collect();
                    names.sort();
                    names.dedup();
                    names
                };
                let forms: Vec<&str> = match words {
                    Words::One(text) => vec![*text],
                    Words::Forms(forms) => {
                        assert_eq!(
                            forms.len(),
                            language.plural_forms(),
                            "{language:?} `{key}` needs {} forms",
                            language.plural_forms()
                        );
                        forms.clone()
                    }
                };
                for form in forms {
                    if key.ends_with(".short") {
                        continue;
                    }
                    assert_eq!(names(form), names(english), "{language:?} `{key}`: {form}");
                }
            }
        }
    }

    // The areas that are translated in full. An area joins the list when its agent has wrapped
    // it and written its Spanish; from then on a string wrapped without Spanish fails here.
    const SPANISH_DONE: &[&str] = &[
        "topbar",
        "left",
        "right",
        "slides",
        "dock",
        "keys",
        "common",
        "media",
        "biblepage",
        "bibles",
        "setup",
        "integrations",
        "remote",
        "inspector",
        "editor",
        "import",
        "tools",
        "settings",
        "edit",
        "themes",
        "bible",
        "dialogs",
        "find",
        "notices",
        "menubar",
        "screens",
    ];

    #[test]
    fn the_finished_areas_have_all_their_spanish() {
        let lacking: Vec<String> = missing(Language::Spanish)
            .into_iter()
            .filter(|(key, _)| SPANISH_DONE.contains(&key.split('.').next().unwrap_or("")))
            .map(|(key, english)| format!("{key} ({english})"))
            .collect();
        assert!(lacking.is_empty(), "no Spanish yet for: {lacking:#?}");
    }

    #[test]
    fn every_table_parses() {
        for (code, area, text) in generated::FILES {
            let value: serde_json::Value =
                serde_json::from_str(text).unwrap_or_else(|e| panic!("{code}/{area}.json: {e}"));
            let map = value.as_object().unwrap_or_else(|| panic!("{code}/{area}.json is not an object"));
            if *area == "format" {
                continue;
            }
            for (key, value) in map.iter().filter(|(key, _)| !key.starts_with('_')) {
                assert!(key.starts_with(&format!("{area}.")), "{code}/{area}.json: `{key}` must start with `{area}.`");
                assert!(value.is_string() || value.is_array(), "{code}/{area}.json: `{key}`");
            }
        }
    }

    /// Not a check: lists what Spanish still lacks of what the program wraps.
    /// `cargo test -p sp-app i18n::tests::spanish_lacks -- --ignored --nocapture`
    /// (`SOFTPRESENTER_LANGUAGE_LACKS=ru` for another language).
    #[test]
    #[ignore = "a listing, not a check"]
    fn spanish_lacks() {
        let language = std::env::var("SOFTPRESENTER_LANGUAGE_LACKS")
            .ok()
            .and_then(|code| Language::from_tag(&code))
            .unwrap_or(Language::Spanish);
        let lacking = missing(language);
        eprintln!("{} of {} wrapped keys have no {language:?} yet:", lacking.len(), wrapped_keys().len());
        for (key, english) in lacking {
            eprintln!("  {key}  ({english})");
        }
    }

    #[test]
    fn the_scan_reads_calls_across_lines() {
        let found = wrapped_in(
            "let a = t(\"x.one\", \"One\");\nlet b = crate::i18n::tf(\n    \"x.two\",\n    \"Two {n}\",\n    &[],\n);\nlet c = tn(\"x.three\", n, \"{n} a\", \"{n} b\", &[]); format(\"x.no\", \"No\")",
        );
        assert_eq!(
            found,
            [
                ("x.one".to_owned(), "One".to_owned()),
                ("x.two".to_owned(), "Two {n}".to_owned()),
                ("x.three".to_owned(), "{n} a".to_owned())
            ]
        );
    }
}
