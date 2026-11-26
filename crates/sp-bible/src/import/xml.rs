//! The XML formats: Zefania, OSIS, Beblia, the `<b>`/`<c>`/`<v>` tags, USX, and the Digital
//! Bible Library's `metadata.xml`.
//!
//! Read as a stream of elements and text, one at a time, keeping what each open element did on
//! a list rather than by recursion, so a deeply nested file cannot use up the stack (and past
//! [`DEEPEST`] open elements it is refused). The five entities every XML file has and
//! character references (`&#1040;`) are read; any other entity is left out, never expanded,
//! and a document type declaration is never read.

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};

use super::usfm::{is_heading_style, is_skipped_character_style, is_title_style};
use super::{About, Collector, Format, VERSE_ROOM, one_line, push_within, verse_number};
use crate::{VerseRef, book_by_number, book_by_osis, book_by_standard_code, book_by_written_name};

/// Elements open at once past this are refused: no Bible nests so deep.
const DEEPEST: usize = 200;
/// The most attributes taken from one element; any past these are passed over. No Bible
/// format gives an element more than a handful.
const MOST_ATTRIBUTES: usize = 32;
/// The most a field of what a Bible says about itself (its name, its copyright…) may grow to,
/// in bytes, while it is read.
const FIELD_ROOM: usize = 4096;
/// The most verses one OSIS `osisID` may name.
const MOST_IDS: usize = 200;

/// An element as it opens: its name without any prefix, in lower case, and its attributes the
/// same way, with their values read.
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
}

impl Element {
    fn get(&self, key: &str) -> Option<&str> {
        self.attributes.iter().find(|(name, _)| name == key).map(|(_, value)| value.trim()).filter(|v| !v.is_empty())
    }

    fn from(start: &BytesStart) -> Element {
        let lower = |bytes: &[u8]| String::from_utf8_lossy(bytes).to_lowercase();
        let attributes = start
            .attributes()
            .with_checks(false)
            .flatten()
            .take(MOST_ATTRIBUTES)
            .map(|attribute| {
                let value = attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map(|value| value.into_owned())
                    // A value naming an entity of the file's own is kept as it is, unexpanded.
                    .unwrap_or_else(|_| String::from_utf8_lossy(&attribute.value).into_owned());
                (lower(attribute.key.local_name().as_ref()), value)
            })
            .collect();
        Element { name: lower(start.local_name().as_ref()), attributes }
    }
}

/// One step through a document.
enum Step<'a> {
    Open(&'a Element),
    Close,
    Text(&'a str),
}

/// Goes through a document from start to end. `each` says to stop early by returning false.
fn walk(text: &str, mut each: impl FnMut(Step) -> bool) -> Result<(), String> {
    let mut reader = quick_xml::Reader::from_str(text);
    let mut depth = 0_usize;
    loop {
        let event = reader.read_event().map_err(|_| {
            format!("This file isn't well-formed XML (there is a mistake near byte {}).", reader.error_position())
        })?;
        let go_on = match event {
            Event::Start(start) => {
                depth += 1;
                if depth > DEEPEST {
                    return Err("This file's elements are nested too deeply to be a Bible.".to_owned());
                }
                each(Step::Open(&Element::from(&start)))
            }
            Event::Empty(start) => each(Step::Open(&Element::from(&start))) && each(Step::Close),
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                each(Step::Close)
            }
            Event::Text(text) => text.decode().map_or(true, |text| each(Step::Text(&text))),
            Event::CData(text) => text.decode().map_or(true, |text| each(Step::Text(&text))),
            Event::GeneralRef(reference) => {
                let resolved = match reference.resolve_char_ref() {
                    Ok(Some(c)) => Some(c.to_string()),
                    Ok(None) => {
                        reference.decode().ok().and_then(|name| resolve_predefined_entity(&name)).map(str::to_owned)
                    }
                    Err(_) => None,
                };
                resolved.is_none_or(|text| each(Step::Text(&text)))
            }
            // A file cut short (elements still open at its end) is refused, not taken in part.
            Event::Eof if depth > 0 => {
                return Err("This file ends before its XML does: it may be cut short.".to_owned());
            }
            Event::Eof => return Ok(()),
            // The declaration, comments, processing instructions and the document type: passed
            // over unread.
            _ => true,
        };
        if !go_on {
            return Ok(());
        }
    }
}

/// Which XML format a document's start is, from its first element (and, for `<bible>`, the
/// one inside it). Works on the first few kilobytes alone.
pub(super) fn kind(start: &str) -> Option<Format> {
    let mut root: Option<(String, bool)> = None;
    let mut found = None;
    let _ = walk(start, |step| {
        let Step::Open(element) = step else { return true };
        match &root {
            None => {
                let translation = element.get("translation").is_some();
                found = match element.name.as_str() {
                    "xmlbible" => Some(Format::Zefania),
                    "osis" | "osistext" => Some(Format::Osis),
                    "usx" => Some(Format::Usx),
                    "bible" if translation => Some(Format::Beblia),
                    _ => None,
                };
                root = Some((element.name.clone(), translation));
                found.is_none() && element.name == "bible"
            }
            Some(_) => {
                found = match element.name.as_str() {
                    "b" => Some(Format::Tags),
                    "testament" | "book" => Some(Format::Beblia),
                    _ => None,
                };
                false
            }
        }
    });
    found
}

/// A field of what a Bible says about itself, being read from an element's text.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    Short,
    Language,
    Terms,
}

/// What an open element did, undone when it closes.
#[derive(Default)]
struct Frame {
    /// Its words are not the Bible's (a note, a heading).
    skips: bool,
    /// The verse it holds ends with it.
    ends_verse: bool,
    /// It starts a line or paragraph, so its words are kept apart from those around it.
    block: bool,
    /// Its text is a field of what the Bible says about itself.
    field: Option<Field>,
    /// Its text is a psalm's title, which goes at the start of the verse after it.
    title: bool,
    /// It is the part of the file that says what the Bible is.
    header: bool,
}

/// Reads one of the XML Bible formats through a document.
struct Reader<'a> {
    format: Format,
    out: &'a mut Collector,
    about: About,
    frames: Vec<Frame>,
    /// Open elements whose words are not the Bible's.
    skip: usize,
    header: usize,
    field: Option<(Field, String)>,
    /// A psalm's title, read and waiting for the verse it starts.
    title: Option<String>,
    book: Option<usize>,
    chapter: u16,
    verse: Option<VerseRef>,
    /// For the `<b>` tags: each book's place, in the order they come.
    tag_books: Vec<Option<usize>>,
    tags_seen: usize,
}

impl Reader<'_> {
    fn begin(&mut self, book: usize, chapter: u16, number: &str) {
        self.verse = None;
        let Some((verse, last)) = verse_number(number) else { return };
        let at = VerseRef { book, chapter, verse };
        if self.out.begin(at, last) {
            self.verse = Some(at);
            if let Some(title) = self.title.take() {
                self.out.put_first(at, &title);
            }
        }
    }

    fn words(&mut self, text: &str) {
        if let Some((_, field)) = self.field.as_mut() {
            push_within(field, text, FIELD_ROOM);
        } else if self.skip > 0 {
        } else if let Some(title) = self.title.as_mut().filter(|_| self.frames.iter().any(|f| f.title)) {
            push_within(title, text, VERSE_ROOM);
        } else if let Some(at) = self.verse {
            self.out.add(at, text);
        }
    }

    fn book_called(&mut self, found: Option<usize>, name: &str) {
        if found.is_none() {
            self.out.leave_out(name);
        }
        self.book = found;
        self.chapter = 0;
        self.verse = None;
    }

    fn open(&mut self, element: &Element) {
        let mut frame = Frame::default();
        let name = element.name.as_str();
        match self.format {
            Format::Zefania => self.zefania(name, element, &mut frame),
            Format::Osis => self.osis(name, element, &mut frame),
            Format::Beblia => self.beblia(name, element, &mut frame),
            Format::Tags => self.tags(name, element, &mut frame),
            Format::Usx => self.usx(name, element, &mut frame),
            Format::Usfm | Format::SoftProjector => {}
        }
        if frame.block {
            self.words(" ");
        }
        if frame.skips {
            self.skip += 1;
        }
        if frame.header {
            self.header += 1;
        }
        if let Some(field) = frame.field {
            self.field = Some((field, String::new()));
        }
        if frame.title {
            // A title in two paragraphs is one title.
            push_within(self.title.get_or_insert_with(String::new), " ", VERSE_ROOM);
        }
        self.frames.push(frame);
    }

    fn close(&mut self) {
        let Some(frame) = self.frames.pop() else { return };
        if frame.skips {
            self.skip = self.skip.saturating_sub(1);
        }
        if frame.header {
            self.header = self.header.saturating_sub(1);
        }
        if frame.block {
            self.words(" ");
        }
        if frame.ends_verse {
            self.verse = None;
        }
        if frame.field.is_some()
            && let Some((field, text)) = self.field.take()
        {
            let text = one_line(&text, 400);
            let place = match field {
                Field::Name => &mut self.about.name,
                Field::Short => &mut self.about.short,
                Field::Language => &mut self.about.language,
                Field::Terms => &mut self.about.terms,
            };
            if place.is_empty() {
                *place = text;
            }
        }
        if frame.title && self.title.as_deref().is_some_and(|title| title.trim().is_empty()) {
            self.title = None;
        }
    }

    /// Zefania XML: `<XMLBIBLE biblename>`, its `<INFORMATION>`, then `<BIBLEBOOK bnumber>`,
    /// `<CHAPTER cnumber>` and `<VERS vnumber>`; `<NOTE>`, `<XREF>`, `<CAPTION>` (headings) and
    /// the rest that are not the text are left out, `<gr>` (Strong's numbers) and `<STYLE>`
    /// keep their words.
    fn zefania(&mut self, name: &str, element: &Element, frame: &mut Frame) {
        match name {
            "xmlbible" => self.about.name = element.get("biblename").unwrap_or_default().to_owned(),
            "information" => frame.header = true,
            "title" if self.header > 0 => frame.field = Some(Field::Name),
            "identifier" if self.header > 0 => frame.field = Some(Field::Short),
            "language" if self.header > 0 => frame.field = Some(Field::Language),
            "rights" if self.header > 0 => frame.field = Some(Field::Terms),
            "biblebook" => {
                let number = element.get("bnumber").and_then(|n| n.parse::<u32>().ok());
                let written = element.get("bname").or(element.get("bsname")).unwrap_or_default();
                let found = match number {
                    Some(number) => book_by_number(number),
                    None => book_by_written_name(written),
                };
                let label =
                    if written.is_empty() { format!("book {}", number.unwrap_or(0)) } else { written.to_owned() };
                self.book_called(found, &label);
            }
            "chapter" => {
                self.chapter = element.get("cnumber").and_then(|n| n.parse().ok()).unwrap_or(0);
                self.verse = None;
            }
            "vers" => {
                if let (Some(book), Some(number)) = (self.book, element.get("vnumber")) {
                    self.begin(book, self.chapter, number);
                }
                frame.ends_verse = true;
            }
            "note" | "xref" | "caption" | "remark" | "prolog" | "sup" | "media" => frame.skips = true,
            "br" => frame.block = true,
            _ => {}
        }
    }

    /// OSIS XML: `<div type="book" osisID="Gen">`, `<chapter osisID="Gen.1">` and `<verse
    /// osisID="Gen.1.1">`, either holding their words or as markers at their start and end
    /// (`sID`, `eID`). `<note>` and `<title>` (headings) are left out; `<w>` keeps its words
    /// (not its Strong's numbers), and `<q who="Jesus">` its words.
    fn osis(&mut self, name: &str, element: &Element, frame: &mut Frame) {
        match name {
            "osistext" => {
                if let Some(work) = element.get("osisidwork").filter(|w| w.chars().count() <= 16 && !w.contains(' ')) {
                    self.about.short = work.to_owned();
                }
                if let Some(language) = element.get("lang") {
                    self.about.language = language.to_owned();
                }
            }
            "header" => frame.header = true,
            "title" if self.header > 0 => frame.field = Some(Field::Name),
            "language" if self.header > 0 => frame.field = Some(Field::Language),
            "rights" if self.header > 0 => frame.field = Some(Field::Terms),
            "div" => {
                frame.block = true;
                if element.get("type").is_some_and(|kind| kind.eq_ignore_ascii_case("book"))
                    && let Some(id) = element.get("osisid")
                {
                    let id = id.to_owned();
                    self.book_called(book_by_osis(&id), &id);
                }
            }
            "chapter" => {
                if element.get("eid").is_none()
                    && let Some(id) = element.get("osisid")
                {
                    let mut parts = id.split('.');
                    if let (Some(book), Some(chapter)) = (parts.next(), parts.next()) {
                        if let Some(found) = book_by_osis(book) {
                            self.book = Some(found);
                        }
                        self.chapter = chapter.parse().unwrap_or(0);
                    }
                    self.verse = None;
                }
            }
            "verse" => {
                if element.get("eid").is_some() {
                    self.verse = None;
                    return;
                }
                let Some(ids) = element.get("osisid") else { return };
                let ids: Vec<(Option<usize>, u16, u16)> = ids
                    .split_whitespace()
                    .take(MOST_IDS)
                    .filter_map(|id| {
                        let mut parts = id.split('.');
                        let (book, chapter, verse) = (parts.next()?, parts.next()?, parts.next()?);
                        Some((book_by_osis(book), chapter.parse().ok()?, verse.parse().ok()?))
                    })
                    .collect();
                if let Some(&(Some(book), chapter, verse)) = ids.first() {
                    let last = ids.iter().filter(|id| id.1 == chapter).map(|id| id.2).max().unwrap_or(verse);
                    self.chapter = chapter;
                    self.begin(book, chapter, &format!("{verse}-{last}"));
                } else {
                    self.verse = None;
                }
                // Holding its words, it ends where it closes; a marker ends at its `eID`.
                frame.ends_verse = element.get("sid").is_none();
            }
            "q" => {
                if let Some(marker) = element.get("marker") {
                    let marker = marker.to_owned();
                    self.words(&marker);
                }
            }
            "note" | "title" | "figure" | "index" | "milestone" | "rdg" | "catchword" | "speaker" => frame.skips = true,
            "lb" | "l" | "lg" | "p" | "item" | "list" | "row" | "cell" | "table" => frame.block = true,
            _ => {}
        }
    }

    /// Beblia XML: `<bible translation status>`, `<testament>`, `<book number>`, `<chapter
    /// number>` and `<verse number>`.
    fn beblia(&mut self, name: &str, element: &Element, frame: &mut Frame) {
        match name {
            "bible" => {
                self.about.name = element.get("translation").unwrap_or_default().to_owned();
                self.about.terms = element.get("status").unwrap_or_default().to_owned();
            }
            "book" => {
                let number = element.get("number").and_then(|n| n.parse::<u32>().ok());
                let written = element.get("name").unwrap_or_default().to_owned();
                let found = number.and_then(book_by_number).or_else(|| book_by_written_name(&written));
                let label = if written.is_empty() { format!("book {}", number.unwrap_or(0)) } else { written };
                self.book_called(found, &label);
            }
            "chapter" => {
                self.chapter = element.get("number").and_then(|n| n.parse().ok()).unwrap_or(0);
                self.verse = None;
            }
            "verse" => {
                if let (Some(book), Some(number)) = (self.book, element.get("number")) {
                    self.begin(book, self.chapter, number);
                }
                frame.ends_verse = true;
            }
            _ => {}
        }
    }

    /// `<bible>`, `<b n="Genesis">`, `<c n="1">`, `<v n="1">`. Books are named, in any of the
    /// languages the app knows, or else taken in order when there are sixty-six.
    fn tags(&mut self, name: &str, element: &Element, frame: &mut Frame) {
        match name {
            "b" => {
                let found = self.tag_books.get(self.tags_seen).copied().flatten();
                self.tags_seen += 1;
                let written = element.get("n").unwrap_or_default().to_owned();
                self.book_called(found, &written);
            }
            "c" => {
                self.chapter = element.get("n").and_then(|n| n.parse().ok()).unwrap_or(0);
                self.verse = None;
            }
            "v" => {
                if let (Some(book), Some(number)) = (self.book, element.get("n")) {
                    self.begin(book, self.chapter, number);
                }
                frame.ends_verse = true;
            }
            _ => {}
        }
    }

    /// USX: `<book code="GEN">`, `<chapter number>` and `<verse number>` markers (with `eid`
    /// ends in USX 3), `<para style>` paragraphs (headings left out by their style, a psalm's
    /// title kept for its first verse), `<note>` left out, `<char>` keeping its words unless
    /// its style is one that isn't the text.
    fn usx(&mut self, name: &str, element: &Element, frame: &mut Frame) {
        match name {
            "book" => {
                let code = element.get("code").unwrap_or_default().to_owned();
                let found = book_by_standard_code(&code);
                if found.is_some() || super::usfm::is_added_book(&code) {
                    self.book_called(found, &code);
                } else {
                    self.book = None;
                }
                frame.skips = true;
            }
            "chapter" => {
                if element.get("eid").is_none() {
                    self.chapter = element.get("number").and_then(|n| n.parse().ok()).unwrap_or(0);
                    self.verse = None;
                    self.title = None;
                }
            }
            "verse" => {
                if element.get("eid").is_some() {
                    self.verse = None;
                } else if let (Some(book), Some(number)) = (self.book, element.get("number")) {
                    self.begin(book, self.chapter, number);
                }
            }
            "para" => {
                let style = element.get("style").unwrap_or_default();
                if is_title_style(style) && self.verse.is_none() {
                    frame.title = true;
                } else if is_heading_style(style) {
                    frame.skips = true;
                } else {
                    frame.block = true;
                }
            }
            "char" => frame.skips = is_skipped_character_style(element.get("style").unwrap_or_default()),
            "note" | "figure" | "sidebar" | "periph" => frame.skips = true,
            "table" | "row" | "cell" => frame.block = true,
            _ => {}
        }
    }
}

/// Reads a whole XML Bible (or one USX book) into `out`, and returns what it says about
/// itself.
pub(super) fn read(format: Format, text: &str, out: &mut Collector) -> Result<About, String> {
    let tag_books = if format == Format::Tags { tag_books(text)? } else { Vec::new() };
    let mut reader = Reader {
        format,
        out,
        about: About::default(),
        frames: Vec::new(),
        skip: 0,
        header: 0,
        field: None,
        title: None,
        book: None,
        chapter: 0,
        verse: None,
        tag_books,
        tags_seen: 0,
    };
    walk(text, |step| {
        match step {
            Step::Open(element) => reader.open(element),
            Step::Close => reader.close(),
            Step::Text(text) => reader.words(text),
        }
        true
    })?;
    Ok(reader.about)
}

/// The books of a `<b>`-tag Bible in order, each found by its name; when some are not found
/// and there are sixty-six (or a Testament's worth), by their order instead.
fn tag_books(text: &str) -> Result<Vec<Option<usize>>, String> {
    let mut names: Vec<String> = Vec::new();
    walk(text, |step| {
        if let Step::Open(element) = step
            && element.name == "b"
        {
            names.push(element.get("n").unwrap_or_default().to_owned());
        }
        true
    })?;
    let by_name: Vec<Option<usize>> = names.iter().map(|name| book_by_written_name(name)).collect();
    let first = match names.len() {
        66 | 39 => 0,
        27 => 39,
        _ => return Ok(by_name),
    };
    if by_name.iter().all(Option::is_some) {
        return Ok(by_name);
    }
    Ok((0..names.len()).map(|at| Some(first + at)).collect())
}

/// What a Digital Bible Library bundle's `metadata.xml` says: the name and abbreviation (the
/// ones in the Bible's own language first), the language's code, and the copyright statement.
/// `None` when the file is something else.
pub(super) fn metadata_about(text: &str) -> Option<About> {
    let mut path: Vec<String> = Vec::new();
    let mut about = About::default();
    let (mut local_name, mut local_short) = (String::new(), String::new());
    let mut is_metadata = false;
    let mut terms = String::new();
    let result = walk(text, |step| {
        match step {
            Step::Open(element) => {
                if path.is_empty() {
                    is_metadata = element.name == "dblmetadata";
                    if !is_metadata {
                        return false;
                    }
                }
                path.push(element.name.clone());
            }
            Step::Close => {
                path.pop();
            }
            Step::Text(text) => {
                let names: Vec<&str> = path.iter().map(String::as_str).collect();
                match names.as_slice() {
                    [_, "identification", "name"] => push_within(&mut about.name, text, FIELD_ROOM),
                    [_, "identification", "namelocal"] => push_within(&mut local_name, text, FIELD_ROOM),
                    [_, "identification", "abbreviation"] => push_within(&mut about.short, text, FIELD_ROOM),
                    [_, "identification", "abbreviationlocal"] => push_within(&mut local_short, text, FIELD_ROOM),
                    [_, "language", "iso"] => push_within(&mut about.language, text, FIELD_ROOM),
                    [_, "copyright", ..] => {
                        push_within(&mut terms, text, FIELD_ROOM);
                        push_within(&mut terms, " ", FIELD_ROOM);
                    }
                    _ => {}
                }
            }
        }
        true
    });
    if result.is_err() || !is_metadata {
        return None;
    }
    if !local_name.trim().is_empty() {
        about.name = local_name;
    }
    if !local_short.trim().is_empty() {
        about.short = local_short;
    }
    about.terms = terms;
    Some(about)
}
