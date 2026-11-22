//! USFM: a book to a file, its text marked with backslash markers (`\id GEN`, `\c 1`, `\v 1`,
//! `\p`, `\s1 A heading`, `\f + \ft a note\f*`, `\w word|strong="H1"\w*`). Read in one pass,
//! marker by marker; nothing nests deeper than one note or span at a time.
//!
//! Also the lists of styles USX shares with it: which paragraphs are headings, which are a
//! psalm's title, which character spans are not the text.

use super::{Collector, VERSE_ROOM, push_within, verse_number};
use crate::{VerseRef, book_by_standard_code};

/// True when a file's start is USFM: its first marker names the book (`\id GEN`).
pub(super) fn looks_like_usfm(start: &str) -> bool {
    start.strip_prefix("\\id").is_some_and(|rest| {
        rest.starts_with([' ', '\t'])
            && rest.trim_start().chars().take(3).filter(char::is_ascii_alphanumeric).count() == 3
    })
}

/// A style's name without the number some styles carry (`s1`, `toc2`, `q1`).
fn base(style: &str) -> &str {
    style.trim_start_matches('+').trim_end_matches(|c: char| c.is_ascii_digit())
}

/// Paragraph styles whose words are not the Bible's: titles of the book, headings, section
/// references, the book's introduction, remarks.
pub(super) fn is_heading_style(style: &str) -> bool {
    matches!(
        base(style),
        "id" | "ide"
            | "usfm"
            | "h"
            | "toc"
            | "toca"
            | "mt"
            | "mte"
            | "ms"
            | "mr"
            | "s"
            | "sr"
            | "r"
            | "sp"
            | "sd"
            | "rem"
            | "sts"
            | "cl"
            | "cp"
            | "cd"
            | "iex"
            | "ip"
            | "ipi"
            | "im"
            | "imi"
            | "ipq"
            | "imq"
            | "ipr"
            | "iq"
            | "ib"
            | "ili"
            | "iot"
            | "io"
            | "is"
            | "imt"
            | "imte"
            | "ie"
            | "lit"
            | "qa"
            | "restore"
            | "periph"
    )
}

/// A psalm's title (`\d`), which belongs at the start of the verse after it, as the King James
/// and most English Bibles keep it.
pub(super) fn is_title_style(style: &str) -> bool {
    base(style) == "d"
}

/// Character spans whose words are not the text: alternative and published verse and chapter
/// numbers, pictures, references after a quotation, categories, cross references' targets.
pub(super) fn is_skipped_character_style(style: &str) -> bool {
    matches!(base(style), "va" | "vp" | "ca" | "fig" | "rq" | "cat" | "xt" | "ior" | "iqt" | "str" | "fv")
}

/// Paragraph styles of the text itself: a new paragraph, poetry, lists, tables.
fn is_paragraph_style(style: &str) -> bool {
    matches!(
        base(style),
        "p" | "m"
            | "po"
            | "pr"
            | "cls"
            | "pmo"
            | "pm"
            | "pmc"
            | "pmr"
            | "pi"
            | "mi"
            | "nb"
            | "pc"
            | "ph"
            | "b"
            | "q"
            | "qr"
            | "qc"
            | "qm"
            | "qd"
            | "lh"
            | "li"
            | "lf"
            | "lim"
            | "tr"
            | "th"
            | "thr"
            | "tc"
            | "tcr"
            | "pb"
    )
}

/// The codes of books some Bibles add to the sixty-six (USFM's list), which are left out and
/// said; other codes the app doesn't know (front matter, glossaries) are passed over quietly.
pub(super) fn is_added_book(code: &str) -> bool {
    matches!(
        code.trim().to_ascii_uppercase().as_str(),
        "TOB"
            | "JDT"
            | "ESG"
            | "WIS"
            | "SIR"
            | "BAR"
            | "LJE"
            | "S3Y"
            | "SUS"
            | "BEL"
            | "1MA"
            | "2MA"
            | "3MA"
            | "4MA"
            | "1ES"
            | "2ES"
            | "MAN"
            | "PS2"
            | "ODA"
            | "PSS"
            | "EZA"
            | "5EZ"
            | "6EZ"
            | "DAG"
            | "PS3"
            | "2BA"
            | "LBA"
            | "JUB"
            | "ENO"
            | "1MQ"
            | "2MQ"
            | "3MQ"
            | "REP"
            | "4BA"
            | "LAO"
    )
}

/// What the next word is wanted for.
#[derive(Clone, Copy)]
enum Wanted {
    Book,
    Chapter,
    Verse,
}

struct Reader<'a> {
    out: &'a mut Collector,
    book: Option<usize>,
    chapter: u16,
    verse: Option<VerseRef>,
    wanted: Option<Wanted>,
    /// Inside a note (`\f`, `\x`…): the marker that ends it.
    note: Option<String>,
    /// Inside a span whose words are not the text (`\va`, `\fig`…): the marker that ends it.
    hidden: Option<String>,
    /// Inside a milestone's attributes (`\qt-s |who="Jesus"\*`), up to `\*`.
    milestone: bool,
    /// After `|` in a span: its attributes, up to the next marker.
    attributes: bool,
    /// In a paragraph whose words are not the text.
    heading: bool,
    /// A psalm's title, read and waiting for the verse it starts; `reading_title` while its
    /// words are still coming.
    title: Option<String>,
    reading_title: bool,
}

impl Reader<'_> {
    fn marker(&mut self, name: &str, closing: bool) {
        let name = name.trim_start_matches('+');
        let starts_text = !closing && (name == "v" || name == "c" || name == "id" || is_paragraph_style(name));
        if let Some(note) = &self.note {
            let ends = (closing && name == note) || (note == "esb" && name == "esbe");
            // A note left open ends at the next verse or paragraph.
            if ends || starts_text {
                self.note = None;
            }
            if !starts_text {
                return;
            }
        }
        if self.milestone {
            if closing && name.is_empty() {
                self.milestone = false;
            }
            return;
        }
        if let Some(hidden) = &self.hidden {
            if closing && name == hidden {
                self.hidden = None;
            }
            if !starts_text {
                return;
            }
            self.hidden = None;
        }
        self.attributes = false;
        if closing {
            // The end of a span that keeps its words (`\w*`, `\add*`, `\wj*`).
            return;
        }
        if name.ends_with("-s") || name.ends_with("-e") {
            self.milestone = true;
            return;
        }
        let ends_title = name == "v" || name == "c" || is_paragraph_style(name) || is_heading_style(name);
        if ends_title {
            self.reading_title = false;
        }
        match name {
            "id" => {
                self.wanted = Some(Wanted::Book);
                self.heading = true;
                self.verse = None;
                self.title = None;
            }
            "c" => {
                self.wanted = Some(Wanted::Chapter);
                self.verse = None;
                self.title = None;
            }
            "v" => {
                self.wanted = Some(Wanted::Verse);
                self.heading = false;
            }
            "f" | "fe" | "ef" | "x" | "ex" | "esb" => self.note = Some(name.to_owned()),
            _ if is_skipped_character_style(name) => self.hidden = Some(name.to_owned()),
            _ if is_title_style(name) => {
                self.heading = false;
                if self.verse.is_none() {
                    // A title in two paragraphs is one title.
                    push_within(self.title.get_or_insert_with(String::new), " ", VERSE_ROOM);
                    self.reading_title = true;
                } else {
                    self.words(" ");
                }
            }
            _ if is_heading_style(name) => self.heading = true,
            _ if is_paragraph_style(name) => {
                self.heading = false;
                self.words(" ");
            }
            // Any other marker starts or ends a span that keeps its words.
            _ => {}
        }
    }

    fn text(&mut self, mut text: &str) {
        if let Some(wanted) = self.wanted {
            let trimmed = text.trim_start();
            if trimmed.is_empty() {
                return;
            }
            self.wanted = None;
            let (word, rest) = trimmed.split_once(char::is_whitespace).unwrap_or((trimmed, ""));
            match wanted {
                Wanted::Book => {
                    let found = book_by_standard_code(word);
                    if found.is_none() && is_added_book(word) {
                        self.out.leave_out(word);
                    }
                    self.book = found;
                    self.chapter = 0;
                }
                Wanted::Chapter => self.chapter = word.parse().unwrap_or(0),
                Wanted::Verse => self.begin(word),
            }
            text = rest;
        }
        if self.note.is_some() || self.hidden.is_some() || self.milestone || self.attributes {
            return;
        }
        // `|` starts a span's attributes (`\w grace|strong="H2580"\w*`): not words.
        if let Some(bar) = text.find('|') {
            self.attributes = true;
            text = &text[..bar];
        }
        self.words(text);
    }

    fn words(&mut self, text: &str) {
        if self.reading_title {
            if let Some(title) = self.title.as_mut() {
                push_within(title, text, VERSE_ROOM);
            }
        } else if !self.heading
            && let Some(at) = self.verse
        {
            self.out.add(at, text);
        }
    }

    fn begin(&mut self, number: &str) {
        self.verse = None;
        let (Some(book), Some((verse, last))) = (self.book, verse_number(number)) else { return };
        let at = VerseRef { book, chapter: self.chapter, verse };
        if self.out.begin(at, last) {
            self.verse = Some(at);
            if let Some(title) = self.title.take().filter(|title| !title.trim().is_empty()) {
                self.out.put_first(at, &title);
            }
        }
    }
}

/// Reads one USFM book into `out`.
pub(super) fn read(text: &str, out: &mut Collector) -> Result<(), String> {
    let mut reader = Reader {
        out,
        book: None,
        chapter: 0,
        verse: None,
        wanted: None,
        note: None,
        hidden: None,
        milestone: false,
        attributes: false,
        heading: false,
        title: None,
        reading_title: false,
    };
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            // A marker: letters, digits, `+` and `-`, then `*` if it closes a span; one space
            // or line end after an opening marker belongs to it.
            let start = at + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'+' | b'-')) {
                end += 1;
            }
            let name = &text[start..end];
            let closing = bytes.get(end) == Some(&b'*');
            if closing || bytes.get(end).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')) {
                end += 1;
            }
            reader.marker(name, closing);
            at = end;
        } else {
            // Text up to the next marker or line end; a line end is a space between words.
            let end = bytes[at..].iter().position(|b| matches!(b, b'\\' | b'\n')).map_or(bytes.len(), |n| at + n);
            reader.text(&text[at..end]);
            if bytes.get(end) == Some(&b'\n') {
                reader.text(" ");
                at = end + 1;
            } else {
                at = end;
            }
        }
    }
    Ok(())
}
