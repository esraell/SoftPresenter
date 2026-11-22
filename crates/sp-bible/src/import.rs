//! Bibles in the file formats other programs use, read into the app's own: Zefania XML, OSIS
//! XML, Beblia XML, the XML that marks books, chapters and verses with `<b>`, `<c>` and `<v>`,
//! USFM (a book to a file, in a folder or a zip) and USX. Any of them may come zipped.
//!
//! The files are not trusted. What is read is bounded ([`Limits`]): the file, a zip's entries
//! and what they unpack to; no XML entity a file declares is expanded and nothing a file names
//! is fetched; nothing is read by a recursion a nested file could deepen; and a zip entry's
//! name is only ever looked at, never used as a place on disk.
//!
//! What comes out is the publisher's words with the notes, cross references, Strong's numbers,
//! headings and footnote markers taken out, and words of Jesus as plain words; a verse split
//! over several elements is joined. A psalm's title, which the King James and most English
//! Bibles keep at the start of verse 1, stays there. The verses keep their own numbers, and
//! the way they are numbered is judged from the chapters' lengths ([`crate::fit`]).

mod archive;
mod usfm;
mod xml;

use std::collections::BTreeMap;
use std::path::Path;

use crate::numbering::{Fit, fit};
use crate::{BOOKS, Bible, Info, Numbering, VerseRef};

pub use archive::Limits;

/// The kinds of file read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zefania,
    Osis,
    Beblia,
    /// XML marking books, chapters and verses with `<b n="Genesis">`, `<c n="1">` and
    /// `<v n="1">`.
    Tags,
    Usfm,
    Usx,
}

impl Format {
    /// The format's name, for the operator.
    pub fn name(self) -> &'static str {
        match self {
            Format::Zefania => "Zefania XML",
            Format::Osis => "OSIS XML",
            Format::Beblia => "Beblia XML",
            Format::Tags => "Bible XML (b, c and v tags)",
            Format::Usfm => "USFM",
            Format::Usx => "USX",
        }
    }
}

/// What a Bible is called and its language, as a file says or as the operator gives them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct About {
    pub name: String,
    /// The short name shown on slides, such as `KJV`.
    pub short: String,
    /// A language code (`en`, `ru`, `uk`…); see [`language_code`].
    pub language: String,
    /// The terms it may be used under, if the file says.
    pub terms: String,
}

impl About {
    /// Each field made one tidy line of a sensible length, and the language a code.
    fn tidied(&self) -> About {
        About {
            name: one_line(&self.name, 120),
            short: one_line(&self.short, 16),
            language: language_code(&self.language),
            terms: one_line(&self.terms, 400),
        }
    }

    /// Fills each empty field from `other`.
    fn or(mut self, other: &About) -> About {
        for (mine, theirs) in [
            (&mut self.name, &other.name),
            (&mut self.short, &other.short),
            (&mut self.language, &other.language),
            (&mut self.terms, &other.terms),
        ] {
            if mine.trim().is_empty() {
                mine.clone_from(theirs);
            }
        }
        self
    }
}

/// A Bible read from another program's file, before it becomes one of the app's own.
#[derive(Debug)]
pub struct Imported {
    pub format: Format,
    /// What the file says about itself, tidied; any field may be empty.
    pub about: About,
    /// What to offer for each field the file leaves empty: from the file's name, and the
    /// language from the alphabet its words are written in.
    pub suggested: About,
    /// The file's name (never its folder), for the Bible's note of where it came from.
    pub file_name: String,
    verses: BTreeMap<VerseRef, String>,
    /// Verses the file has that hold no words once the notes and headings are out; they are
    /// left out.
    pub empty: usize,
    /// Books in the file that the app does not have (books some Bibles add), as the file
    /// names them.
    pub left_out: Vec<String>,
    /// How its verses are numbered, judged from its chapters.
    pub fit: Fit,
}

impl Imported {
    pub fn verse_count(&self) -> usize {
        self.verses.len()
    }

    pub fn book_count(&self) -> usize {
        let mut books: Vec<usize> = self.verses.keys().map(|place| place.book).collect();
        books.dedup();
        books.len()
    }

    /// The words of one verse, as they will be kept.
    pub fn verse(&self, place: VerseRef) -> Option<&str> {
        self.verses.get(&place).map(String::as_str)
    }

    /// True when the file says the Bible's name, short name and language, so nothing need be
    /// asked.
    pub fn complete(&self) -> bool {
        !self.about.name.is_empty() && !self.about.short.is_empty() && !self.about.language.is_empty()
    }

    /// The fields to ask for, each filled with what the file says or else what is suggested.
    pub fn prefilled(&self) -> About {
        self.about.clone().or(&self.suggested)
    }

    /// What neither the file nor `given` says, for the command line to ask for, such as
    /// `--short and --language`; empty when nothing is missing.
    pub fn missing_flags(&self, given: &About) -> String {
        let have = given.tidied().or(&self.about);
        let missing: Vec<&str> = [
            (have.name.is_empty(), "--name"),
            (have.short.is_empty(), "--short"),
            (have.language.is_empty(), "--language"),
        ]
        .into_iter()
        .filter_map(|(missing, flag)| missing.then_some(flag))
        .collect();
        match missing.as_slice() {
            [] => String::new(),
            [one] => (*one).to_owned(),
            [first @ .., last] => format!("{} and {last}", first.join(", ")),
        }
    }

    /// The details to keep: what was given, each empty field from the file. `None` while a
    /// name, short name or language is still missing.
    pub fn details(&self, given: &About) -> Option<About> {
        let about = given.tidied().or(&self.about);
        (!about.name.is_empty() && !about.short.is_empty() && !about.language.is_empty()).then_some(about)
    }

    /// The app's own Bible, under the short name for files `id` and the details given.
    pub fn bible(&self, id: &str, about: &About) -> Bible {
        let about = about.tidied();
        let info = Info {
            id: id.to_owned(),
            name: about.name,
            short: about.short,
            language: about.language,
            terms: about.terms,
            credit: String::new(),
            source: one_line(&format!("{} ({})", self.file_name, self.format.name()), 200),
            numbering: self.fit.numbering,
            loose: self.fit.loose,
        };
        Bible::from_verses(info, self.verses.iter().map(|(place, words)| (*place, words.as_str())))
    }

    /// What came in, for the operator: "Installed the KJV (66 books, 31,102 verses), numbered
    /// as English Bibles are; 3 verses were empty."
    pub fn summary(&self, name: &str) -> String {
        let books = self.book_count();
        let mut said = format!(
            "Installed {name} ({books} book{}, {} verse{}), {}",
            if books == 1 { "" } else { "s" },
            grouped(self.verse_count()),
            if self.verse_count() == 1 { "" } else { "s" },
            self.numbering_said()
        );
        if self.empty > 0 {
            said.push_str(&format!(
                "; {} {} empty",
                grouped(self.empty),
                if self.empty == 1 { "verse was" } else { "verses were" }
            ));
        }
        if !self.left_out.is_empty() {
            let mut names = self.left_out.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
            if self.left_out.len() > 6 {
                names.push_str(", …");
            }
            said.push_str(&format!(
                "; {} book{} the app doesn't have {} left out ({names})",
                self.left_out.len(),
                if self.left_out.len() == 1 { "" } else { "s" },
                if self.left_out.len() == 1 { "was" } else { "were" },
            ));
        }
        said.push('.');
        said
    }

    /// How its verses were found to be numbered, in words.
    pub fn numbering_said(&self) -> String {
        let fit = &self.fit;
        if fit.telling == 0 {
            return "numbered as English Bibles are (nothing in it tells the ways of numbering apart)".to_owned();
        }
        let how = match fit.numbering {
            Numbering::English => "as English Bibles are",
            Numbering::Synodal => "as the Russian Synodal Bible is",
            Numbering::Original => "as the Hebrew and Greek texts are",
        };
        let mostly = if fit.loose { "mostly " } else { "" };
        let judged = if fit.by_psalms { "judged by its Psalms" } else { "judged by its chapters' lengths" };
        format!("{mostly}numbered {how}, {judged}")
    }
}

/// Reads a Bible from a file, a zip or a folder of book files, with the limits given.
pub fn read_path(path: &Path, limits: &Limits) -> Result<Imported, String> {
    let file_name = path.file_name().map_or_else(|| "the file".to_owned(), |name| name.to_string_lossy().into_owned());
    let documents = archive::documents_at(path, limits)?;
    from_documents(documents, &file_name)
}

/// Reads a Bible from a file's bytes (a single file, or a zip), with the limits given.
pub fn read_bytes(bytes: &[u8], file_name: &str, limits: &Limits) -> Result<Imported, String> {
    let documents = archive::documents_in(bytes, file_name, limits)?;
    from_documents(documents, file_name)
}

/// True when the path looks like a Bible this module reads: a file whose first bytes are one
/// of the formats, a zip holding such files, or a folder of USFM or USX books. Cheap: reads
/// a little of the file and, for a zip, the list of what is in it.
pub fn looks_like_bible(path: &Path) -> bool {
    archive::looks_like_bible(path)
}

/// What one file of a set holds, by its first part.
fn kind_of_text(text: &str) -> Option<Format> {
    let start = text.trim_start_matches('\u{feff}').trim_start();
    if start.starts_with('\\') {
        return usfm::looks_like_usfm(start).then_some(Format::Usfm);
    }
    if start.starts_with('<') {
        return xml::kind(start);
    }
    None
}

/// One file of what was given: its name (for messages) and its text.
pub(crate) struct Document {
    pub name: String,
    pub text: String,
}

fn from_documents(documents: Vec<Document>, file_name: &str) -> Result<Imported, String> {
    let mut about = About::default();
    let mut whole: Vec<(Format, &Document)> = Vec::new();
    let mut books: Vec<(Format, &Document)> = Vec::new();
    for document in &documents {
        if let Some(found) = bundle_about(document) {
            about = about.or(&found);
            continue;
        }
        match kind_of_text(&document.text) {
            Some(format @ (Format::Usfm | Format::Usx)) => books.push((format, document)),
            Some(format) => whole.push((format, document)),
            None => {}
        }
    }
    let mut out = Collector::default();
    let format = match (whole.as_slice(), books.as_slice()) {
        ([(format, document)], []) => {
            let said = xml::read(*format, &document.text, &mut out)?;
            about = said.or(&about);
            *format
        }
        ([], [(first, _), ..]) => {
            for (format, document) in &books {
                match format {
                    Format::Usfm => usfm::read(&document.text, &mut out)?,
                    _ => {
                        xml::read(*format, &document.text, &mut out)?;
                    }
                }
            }
            *first
        }
        ([], []) => return Err(format!("{file_name} isn't a Bible file SoftPresenter can read.")),
        _ => {
            return Err(format!("{file_name} holds more than one Bible. Take out the one to install and try that."));
        }
    };
    let (verses, empty) = out.finish()?;
    let about = about.tidied();
    let suggested = suggest(file_name, &about, &verses);
    let fit = fit(&out.chapters);
    Ok(Imported {
        format,
        about,
        suggested,
        file_name: file_name.to_owned(),
        verses,
        empty,
        left_out: out.left_out,
        fit,
    })
}

/// What a file beside the books says about the Bible: eBible's `copr.htm`, a Door43
/// `manifest.yaml`, or a Digital Bible Library `metadata.xml`. `None` for any other file.
fn bundle_about(document: &Document) -> Option<About> {
    let name = document.name.rsplit(['/', '\\']).next().unwrap_or(&document.name).to_ascii_lowercase();
    match name.as_str() {
        "copr.htm" => Some(ebible_about(&document.text)),
        "manifest.yaml" => Some(door43_about(&document.text)),
        "metadata.xml" => xml::metadata_about(&document.text),
        _ => None,
    }
}

/// The name and language on eBible's copyright page (its `<title>` and `<html lang>`), and its
/// licence where it names one.
fn ebible_about(page: &str) -> About {
    let between = |from: &str, to: &str| {
        let start = page.find(from)? + from.len();
        let end = page[start..].find(to)? + start;
        Some(html_text(&page[start..end]))
    };
    // Invisible direction marks sometimes sit inside the licence's address.
    let lower: String = page
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(*c as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069))
        .collect();
    let terms = if let Some(at) = lower.find("creativecommons.org/licenses/") {
        let rest = &lower[at + "creativecommons.org/licenses/".len()..];
        let kind: String =
            rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '/' || *c == '.').collect();
        let mut parts = kind.trim_matches('/').split('/');
        match (parts.next(), parts.next()) {
            (Some(kind), Some(version)) if !kind.is_empty() => format!("CC {} {version}", kind.to_ascii_uppercase()),
            _ => String::new(),
        }
    } else if lower.contains("public domain") {
        "Public domain".to_owned()
    } else {
        String::new()
    };
    About {
        name: between("<title>", "</title>").unwrap_or_default(),
        short: String::new(),
        language: between("<html lang=\"", "\"").unwrap_or_default(),
        terms,
    }
}

/// The few entities eBible's pages use, and no tags.
fn html_text(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">")
}

/// The title, identifier, language and rights in a Door43 resource's `manifest.yaml`
/// (`dublin_core:` with `title:`, `identifier:`, `rights:`, and `language:` with its own
/// `identifier:`). Read line by line: only those keys, only plain values.
fn door43_about(manifest: &str) -> About {
    let mut about = About::default();
    let mut section: Vec<(usize, String)> = Vec::new();
    for line in manifest.lines() {
        let indent = line.len() - line.trim_start().len();
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        section.retain(|(at, _)| *at < indent);
        let value = value.trim().trim_matches(['\'', '"']).trim();
        let path: Vec<&str> = section.iter().map(|(_, key)| key.as_str()).collect();
        match (path.as_slice(), key.trim()) {
            (["dublin_core"], "title") => about.name = value.to_owned(),
            (["dublin_core"], "identifier") => about.short = value.to_uppercase(),
            (["dublin_core"], "rights") => about.terms = value.to_owned(),
            (["dublin_core", "language"], "identifier") => about.language = value.to_owned(),
            _ => {}
        }
        if value.is_empty() {
            section.push((indent, key.trim().to_owned()));
        }
    }
    about
}

/// What to offer for the fields a file leaves empty: the name and short name from the file's
/// name ("RST.xml" gives RST), the language from the alphabet the words are written in.
fn suggest(file_name: &str, about: &About, verses: &BTreeMap<VerseRef, String>) -> About {
    let stem = file_name.split('.').next().unwrap_or(file_name);
    let mut stem = stem.to_owned();
    for ending in ["_usfm", "-usfm", "_usx", "-usx", "_osis", "-osis", "_xml", "_zefania", "-zefania", "_bible"] {
        if stem.to_ascii_lowercase().ends_with(ending) {
            stem.truncate(stem.len() - ending.len());
        }
    }
    let name = one_line(&stem.replace(['_', '-'], " "), 120);
    // The first letters of its name's words, as "Russian Synodal Translation" gives RST.
    let initials = |name: &str| -> String {
        name.split_whitespace()
            .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
            .flat_map(char::to_uppercase)
            .take(6)
            .collect()
    };
    let short = if about.name.chars().filter(|c| c.is_alphanumeric()).count() >= 2 && about.name.contains(' ') {
        initials(&about.name)
    } else if (!stem.contains([' ', '_']) && stem.chars().count() <= 10) || initials(&name).chars().count() < 2 {
        // A short file name, or one of a single word: its first letters.
        stem.chars().filter(|c| c.is_alphanumeric()).take(8).flat_map(char::to_uppercase).collect()
    } else {
        initials(&name)
    };
    About { name, short, language: guess_language(verses), terms: String::new() }
}

/// The language a Bible is most likely in, from the letters of its first verses: Ukrainian or
/// Russian for Cyrillic (by the letters only one of them has), Greek, Hebrew, and English for
/// Latin letters with English's commonest words. Empty when it can't be told.
fn guess_language(verses: &BTreeMap<VerseRef, String>) -> String {
    let (mut cyrillic, mut latin, mut greek, mut hebrew, mut ukrainian, mut russian) = (0, 0, 0, 0, 0, 0);
    let mut english_words = 0;
    for words in verses.values().take(400) {
        for c in words.chars() {
            match c {
                'і' | 'ї' | 'є' | 'ґ' | 'І' | 'Ї' | 'Є' | 'Ґ' => ukrainian += 1,
                'ы' | 'э' | 'ъ' | 'ё' | 'Ы' | 'Э' | 'Ъ' | 'Ё' => russian += 1,
                _ => {}
            }
            match c as u32 {
                0x0400..=0x04FF => cyrillic += 1,
                0x0370..=0x03FF | 0x1F00..=0x1FFF => greek += 1,
                0x0590..=0x05FF => hebrew += 1,
                _ if c.is_ascii_alphabetic() => latin += 1,
                _ => {}
            }
        }
        english_words +=
            words.split_whitespace().filter(|w| matches!(*w, "the" | "and" | "of" | "The" | "And")).count();
    }
    let most = cyrillic.max(latin).max(greek).max(hebrew);
    if most == 0 {
        String::new()
    } else if most == cyrillic {
        if ukrainian > russian { "uk" } else { "ru" }.to_owned()
    } else if most == greek {
        "el".to_owned()
    } else if most == hebrew {
        "he".to_owned()
    } else if english_words * 40 > latin / 5 {
        // About one word in eight is one of these in English text.
        "en".to_owned()
    } else {
        String::new()
    }
}

/// A language as a code the app knows it by: `en`, `ru`, `uk` and the other languages it
/// names, from a two- or three-letter code ("RUS", "ukr", "en-GB") or an English name;
/// another code lower case as given.
pub fn language_code(language: &str) -> String {
    let plain = language.trim().to_lowercase();
    let plain = plain.split(['-', '_']).next().unwrap_or_default();
    let code = match plain {
        "en" | "eng" | "english" => "en",
        "ru" | "rus" | "russian" | "русский" => "ru",
        "uk" | "ukr" | "ukrainian" | "українська" => "uk",
        "de" | "deu" | "ger" | "german" | "deutsch" => "de",
        "fr" | "fra" | "fre" | "french" | "français" => "fr",
        "es" | "spa" | "spanish" | "español" => "es",
        "pt" | "por" | "portuguese" | "português" => "pt",
        "el" | "ell" | "gre" | "greek" => "el",
        "he" | "heb" | "hebrew" => "he",
        other => other,
    };
    one_line(code, 16).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// A short name for files from a Bible's short name (or its name): ASCII letters, digits and
/// dashes only, Cyrillic written in Latin letters ("НПУ" gives `npu`). `bible` when nothing
/// is left.
pub fn id_from(short: &str, name: &str) -> String {
    let make = |text: &str| -> String {
        let mut out = String::new();
        for c in text.to_lowercase().chars() {
            match c {
                'a'..='z' | '0'..='9' => out.push(c),
                ' ' | '-' | '_' | '.' => out.push('-'),
                _ => out.push_str(latin_for(c)),
            }
        }
        let mut id = out.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
        id.truncate(32);
        id.trim_end_matches('-').to_owned()
    };
    [make(short), make(name)].into_iter().find(|id| !id.is_empty()).unwrap_or_else(|| "bible".to_owned())
}

/// A Cyrillic letter in Latin letters; nothing for anything else.
fn latin_for(c: char) -> &'static str {
    match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' | 'ґ' => "g",
        'д' => "d",
        'е' | 'э' => "e",
        'є' => "ye",
        'ё' => "yo",
        'ж' => "zh",
        'з' => "z",
        'и' | 'і' => "i",
        'ї' => "yi",
        'й' | 'ы' => "y",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "kh",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "shch",
        'ю' => "yu",
        'я' => "ya",
        _ => "",
    }
}

/// A number with its thousands marked: 31,102.
pub fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Text made one line: no control characters, single spaces, at most `most` characters.
pub(crate) fn one_line(text: &str, most: usize) -> String {
    let words: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut line = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((at, _)) = line.char_indices().nth(most) {
        line.truncate(at);
        line = line.trim_end().to_owned();
    }
    line
}

/// The most a verse may hold, in characters; past that the rest is left out. The longest verse
/// in any Bible is a few hundred.
const LONGEST_VERSE: usize = 4_000;
/// The most verses a Bible may have. There are 31,102 in the sixty-six books.
const MOST_VERSES: usize = 60_000;
/// Chapter and verse numbers past this are taken for mistakes and left out (the Psalms have
/// 150 chapters, Psalm 119 has 176 verses).
const HIGHEST_NUMBER: u16 = 250;

/// The verses as they are read, and what is noticed along the way.
#[derive(Default)]
pub(crate) struct Collector {
    verses: BTreeMap<VerseRef, String>,
    /// Each chapter's last verse number, a verse that covers several ("1-2") counting as its
    /// last.
    chapters: BTreeMap<(usize, u16), u16>,
    left_out: Vec<String>,
    too_many: bool,
}

impl Collector {
    /// A verse begins: `last` is the last verse number it covers (its own unless it is several
    /// joined). A verse begun again (its words split over several elements) carries on after a
    /// space. False when the verse is not kept (numbers out of range, or too many verses).
    pub fn begin(&mut self, at: VerseRef, last: u16) -> bool {
        if at.book >= BOOKS.len()
            || !(1..=HIGHEST_NUMBER).contains(&at.chapter)
            || !(1..=HIGHEST_NUMBER).contains(&at.verse)
        {
            return false;
        }
        if let Some(words) = self.verses.get_mut(&at) {
            words.push(' ');
        } else {
            if self.verses.len() >= MOST_VERSES {
                self.too_many = true;
                return false;
            }
            self.verses.insert(at, String::new());
        }
        let longest = self.chapters.entry((at.book, at.chapter)).or_insert(0);
        *longest = (*longest).max(at.verse).max(last.min(HIGHEST_NUMBER));
        true
    }

    /// Words of a verse begun.
    pub fn add(&mut self, at: VerseRef, words: &str) {
        if let Some(verse) = self.verses.get_mut(&at)
            && verse.len() < LONGEST_VERSE * 4
        {
            verse.push_str(words);
        }
    }

    /// Words put at the start of a verse begun (a psalm's title before its first verse).
    pub fn put_first(&mut self, at: VerseRef, words: &str) {
        if let Some(verse) = self.verses.get_mut(&at) {
            verse.insert_str(0, &format!("{words} "));
        }
    }

    /// A book in the file that the app does not have.
    pub fn leave_out(&mut self, name: &str) {
        let name = one_line(name, 60);
        if !name.is_empty() && !self.left_out.contains(&name) && self.left_out.len() < 100 {
            self.left_out.push(name);
        }
    }

    /// The verses with their words tidied, and how many were left out for holding none.
    fn finish(&mut self) -> Result<(BTreeMap<VerseRef, String>, usize), String> {
        if self.too_many {
            return Err("This file has more verses than any Bible, so it wasn't read.".to_owned());
        }
        let mut empty = 0;
        let mut kept = BTreeMap::new();
        for (place, words) in std::mem::take(&mut self.verses) {
            let words = tidy(&words);
            if words.is_empty() {
                empty += 1;
            } else {
                kept.insert(place, words);
            }
        }
        if kept.is_empty() {
            return Err("No verses of the sixty-six books were found in this file.".to_owned());
        }
        Ok((kept, empty))
    }
}

/// A verse's words as they are kept: one line with single spaces, without pilcrows (a
/// paragraph mark, never a word) or invisible marks, at most [`LONGEST_VERSE`] characters.
fn tidy(words: &str) -> String {
    let cleaned: String = words.chars().filter(|c| !matches!(c, '¶' | '\u{feff}' | '\u{200b}')).collect();
    one_line(&cleaned, LONGEST_VERSE)
}

/// A verse number as files write it: `16`, `16-18` (several joined), `16a` (part of one).
/// The verse, and the last it covers.
pub(crate) fn verse_number(text: &str) -> Option<(u16, u16)> {
    let text = text.trim();
    let digits = |text: &str| -> Option<u16> {
        let number: String = text.trim().chars().take_while(char::is_ascii_digit).collect();
        number.parse().ok().filter(|n| *n > 0)
    };
    let first = digits(text)?;
    let last = text.split_once(['-', '–', ',']).and_then(|(_, rest)| digits(rest)).filter(|last| *last >= first);
    Some((first, last.unwrap_or(first)))
}

#[cfg(test)]
mod tests;
