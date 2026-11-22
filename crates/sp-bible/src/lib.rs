//! Bibles for the Bible page: reading a Bible file, finding the verses a reference such as
//! "John 3:16-18" names, and lining up Bibles that number their verses differently.
//!
//! A Bible file (`.spbible`, or `.pcbible` from before the project was renamed) is gzip-compressed text: a first line naming the format, then
//! `key: value` lines about the Bible, a blank line, and one verse per line as
//! `GEN 1:1 In the beginning...`. The words are kept exactly as their publisher gives them.

mod books;
mod numbering;

use std::io::{Read, Write};

pub use books::{
    BOOKS, Book, book_by_code, book_name, find_book, find_book_in, find_book_typed, reading_book_name, suggest_books,
};
pub use numbering::{Numbering, convert};

const FIRST_LINE: &str = "softpresenter-bible 1";
/// The first line of Bible files made before the project was renamed (the shipped ones and the
/// downloads among them); they still open.
const OLD_FIRST_LINE: &str = "presentercraft-bible 1";

/// One verse's place: the book's position in [`BOOKS`], then chapter and verse from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerseRef {
    pub book: usize,
    pub chapter: u16,
    pub verse: u16,
}

/// What a Bible file says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    /// A short name for files and settings, such as `kjv`.
    pub id: String,
    pub name: String,
    /// The short name shown on slides, such as `KJV`.
    pub short: String,
    /// The language's code, such as `en` or `ru`. Decides how book names are written.
    pub language: String,
    /// The terms it may be used under, in a few words.
    pub terms: String,
    /// The credit its terms ask for, if any.
    pub credit: String,
    /// Where the text was taken from.
    pub source: String,
    pub numbering: Numbering,
    /// True when the Bible only mostly follows that numbering, so that beside another Bible a
    /// few verses may be one off.
    pub loose: bool,
}

/// A whole Bible held in memory: all its words in one piece, and where each verse is in it.
#[derive(Debug)]
pub struct Bible {
    pub info: Info,
    text: String,
    /// Each verse and the bytes of `text` that hold it, in order.
    index: Vec<(VerseRef, u32, u32)>,
}

/// Verses from one place to another, both included, in the same book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: VerseRef,
    pub end: VerseRef,
}

/// The last verse of a chapter, whatever its number.
pub const LAST: u16 = u16::MAX;

impl Bible {
    /// Reads a Bible file. The error is a plain sentence.
    pub fn from_bytes(bytes: &[u8]) -> Result<Bible, String> {
        let mut text = String::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_string(&mut text)
            .map_err(|_| "This is not a SoftPresenter Bible file, or it is damaged.".to_owned())?;
        Bible::from_text(&text)
    }

    /// Reads only what a Bible file says about itself, without reading its verses.
    pub fn read_info(bytes: &[u8]) -> Result<Info, String> {
        use std::io::BufRead;
        let not_ours = || "This is not a SoftPresenter Bible file, or it is damaged.".to_owned();
        let mut head = String::new();
        let mut reader = std::io::BufReader::new(flate2::read::GzDecoder::new(bytes));
        // The lines about the Bible end at the first blank line; they are short.
        for _ in 0..40 {
            let before = head.len();
            let read = reader.read_line(&mut head).map_err(|_| not_ours())?;
            if read == 0 || head[before..].trim_end_matches('\n').is_empty() {
                break;
            }
        }
        let mut lines = head.lines();
        if !matches!(lines.next(), Some(FIRST_LINE | OLD_FIRST_LINE)) {
            return Err("This is not a SoftPresenter Bible file, or it was made by a newer version.".to_owned());
        }
        Bible::info_from(&mut lines)
    }

    fn from_text(file: &str) -> Result<Bible, String> {
        let mut lines = file.lines();
        if !matches!(lines.next(), Some(FIRST_LINE | OLD_FIRST_LINE)) {
            return Err("This is not a SoftPresenter Bible file, or it was made by a newer version.".to_owned());
        }
        let info = Bible::info_from(&mut lines)?;
        let mut bible = Bible { info, text: String::new(), index: Vec::new() };
        for line in lines {
            let Some((place, words)) = parse_verse_line(line) else { continue };
            bible.push(place, words);
        }
        if bible.index.is_empty() {
            return Err("This Bible file has no verses in it.".to_owned());
        }
        bible.index.sort_by_key(|(place, _, _)| *place);
        Ok(bible)
    }

    /// Reads the `key: value` lines up to the first blank line.
    fn info_from<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Result<Info, String> {
        let mut info = Info {
            id: String::new(),
            name: String::new(),
            short: String::new(),
            language: String::new(),
            terms: String::new(),
            credit: String::new(),
            source: String::new(),
            numbering: Numbering::English,
            loose: false,
        };
        for line in lines {
            if line.is_empty() {
                break;
            }
            let Some((key, value)) = line.split_once(": ") else { continue };
            let value = value.to_owned();
            match key {
                "id" => info.id = value,
                "name" => info.name = value,
                "short" => info.short = value,
                "language" => info.language = value,
                "terms" => info.terms = value,
                "credit" => info.credit = value,
                "source" => info.source = value,
                "numbering" => {
                    info.numbering = Numbering::from_name(&value).ok_or_else(|| {
                        format!("This Bible file uses a verse numbering this version does not know: {value}.")
                    })?;
                }
                "numbering-fit" => info.loose = value == "loose",
                _ => {}
            }
        }
        if info.id.is_empty() || info.name.is_empty() {
            return Err("This Bible file does not say which Bible it is.".to_owned());
        }
        Ok(info)
    }

    /// Makes a Bible from one-verse-per-line text (`GEN 1:1 words`), passing each verse's
    /// words through `tidy` first. Lines that are not verses of the sixty-six books are left out.
    pub fn from_verse_lines(info: Info, lines: &str, tidy: impl Fn(&str) -> String) -> Bible {
        let mut bible = Bible { info, text: String::new(), index: Vec::new() };
        for line in lines.lines() {
            if let Some((place, words)) = parse_verse_line(line) {
                let words = tidy(words);
                if !words.is_empty() {
                    bible.push(place, &words);
                }
            }
        }
        bible.index.sort_by_key(|(place, _, _)| *place);
        bible
    }

    fn push(&mut self, place: VerseRef, words: &str) {
        let start = self.text.len() as u32;
        self.text.push_str(words);
        self.index.push((place, start, self.text.len() as u32));
    }

    /// Writes the Bible as a file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let i = &self.info;
        let mut out = format!(
            "{FIRST_LINE}\nid: {}\nname: {}\nshort: {}\nlanguage: {}\nterms: {}\ncredit: {}\nsource: {}\nnumbering: {}\nnumbering-fit: {}\n\n",
            i.id,
            i.name,
            i.short,
            i.language,
            i.terms,
            i.credit,
            i.source,
            i.numbering.name(),
            if i.loose { "loose" } else { "exact" }
        );
        for (place, words) in self.all() {
            out.push_str(&format!("{} {}:{} {words}\n", BOOKS[place.book].code, place.chapter, place.verse));
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        // Writing into memory cannot fail.
        let _ = gz.write_all(out.as_bytes());
        gz.finish().unwrap_or_default()
    }

    pub fn verse_count(&self) -> usize {
        self.index.len()
    }

    fn words(&self, entry: &(VerseRef, u32, u32)) -> &str {
        &self.text[entry.1 as usize..entry.2 as usize]
    }

    /// Every verse in order.
    pub fn all(&self) -> impl Iterator<Item = (VerseRef, &str)> {
        self.index.iter().map(|entry| (entry.0, self.words(entry)))
    }

    /// One verse's words, if the Bible has that verse.
    pub fn verse(&self, place: VerseRef) -> Option<&str> {
        let at = self.index.binary_search_by_key(&place, |(p, _, _)| *p).ok()?;
        Some(self.words(&self.index[at]))
    }

    /// The verses a range names, in order. A range that names nothing gives an empty list.
    pub fn verses(&self, range: Range) -> Vec<(VerseRef, &str)> {
        let from = self.index.partition_point(|(p, _, _)| *p < range.start);
        self.index[from..]
            .iter()
            .take_while(|(p, _, _)| *p <= range.end)
            .map(|entry| (entry.0, self.words(entry)))
            .collect()
    }

    /// How many chapters a book has in this Bible; 0 if it does not have the book.
    pub fn chapter_count(&self, book: usize) -> u16 {
        let after = VerseRef { book: book + 1, chapter: 0, verse: 0 };
        let end = self.index.partition_point(|(p, _, _)| *p < after);
        end.checked_sub(1).map(|last| self.index[last].0).filter(|p| p.book == book).map_or(0, |p| p.chapter)
    }

    /// The number of the last verse in a chapter; 0 if this Bible does not have the chapter.
    pub fn last_verse(&self, book: usize, chapter: u16) -> u16 {
        let after = VerseRef { book, chapter: chapter + 1, verse: 0 };
        let end = self.index.partition_point(|(p, _, _)| *p < after);
        end.checked_sub(1)
            .map(|last| self.index[last].0)
            .filter(|p| p.book == book && p.chapter == chapter)
            .map_or(0, |p| p.verse)
    }

    /// The run of verses just after (or just before) a range, as many as the range has, staying
    /// inside the book. `None` at the book's end (or start), or if the range names nothing.
    pub fn step(&self, range: Range, forward: bool) -> Option<Range> {
        let from = self.index.partition_point(|(p, _, _)| *p < range.start);
        let past = self.index.partition_point(|(p, _, _)| *p <= range.end);
        let count = past.checked_sub(from).filter(|count| *count > 0)?;
        let book = self.index[from].0.book;
        let in_book = |at: usize| self.index.get(at).is_some_and(|(p, _, _)| p.book == book);
        let (first, last) = if forward {
            if !in_book(past) {
                return None;
            }
            let mut last = past;
            while last + 1 < past + count && in_book(last + 1) {
                last += 1;
            }
            (past, last)
        } else {
            let last = from.checked_sub(1).filter(|at| in_book(*at))?;
            let mut first = last;
            while first > 0 && last - first + 1 < count && in_book(first - 1) {
                first -= 1;
            }
            (first, last)
        };
        Some(Range { start: self.index[first].0, end: self.index[last].0 })
    }

    /// A range made one verse longer, at its end or at its start, staying inside the book and
    /// crossing a chapter's end if it must. `None` when there is no verse to add.
    pub fn extend(&self, range: Range, forward: bool) -> Option<Range> {
        let from = self.index.partition_point(|(p, _, _)| *p < range.start);
        let past = self.index.partition_point(|(p, _, _)| *p <= range.end);
        if past <= from {
            return None;
        }
        let book = self.index[from].0.book;
        let in_book = |at: usize| self.index.get(at).is_some_and(|(p, _, _)| p.book == book);
        if forward {
            in_book(past).then(|| Range { start: self.index[from].0, end: self.index[past].0 })
        } else {
            let earlier = from.checked_sub(1).filter(|at| in_book(*at))?;
            Some(Range { start: self.index[earlier].0, end: self.index[past - 1].0 })
        }
    }

    /// The first verses that hold every one of the given words, without regard to case: those
    /// holding the words together as typed come first, then the rest, each in Bible order.
    pub fn search(&self, words: &str, limit: usize) -> Vec<(VerseRef, &str)> {
        let wanted: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        let phrase = wanted.join(" ");
        let (mut together, mut apart) = (Vec::new(), Vec::new());
        for (place, text) in self.all() {
            let lower = text.to_lowercase();
            if lower.contains(&phrase) {
                together.push((place, text));
            } else if wanted.len() > 1 && wanted.iter().all(|word| lower.contains(word.as_str())) {
                apart.push((place, text));
            }
            if together.len() >= limit {
                break;
            }
        }
        together.extend(apart);
        together.truncate(limit);
        together
    }

    /// A book's name as this Bible's language writes it.
    pub fn book_name(&self, book: usize) -> &'static str {
        book_name(book, &self.info.language)
    }

    /// How a run of verses is named, such as "John 3:16-18" or "Psalms 23". `first` and `last`
    /// are the first and last verses actually shown.
    pub fn label(&self, first: VerseRef, last: VerseRef) -> String {
        label_in(&self.info.language, first, last)
    }
}

/// How a run of verses is named in a language (`en`, `ru`, `uk`; English for any other), as a
/// Bible of that language names it: "John 3:16-18", "От Иоанна 3:16-18". The numbers are used
/// as they are given; nothing is converted.
pub fn label_in(language: &str, first: VerseRef, last: VerseRef) -> String {
    label_with(book_name(first.book, language), first, last)
}

/// How a run of verses reads to people in a language: as [`label_in`] names it, with the book
/// named as [`reading_book_name`] has it ("Psalm 23:1-6", "Псалом 22:1", "Psalms 23:1-24:6").
/// It reads back as the same verses. For what people read (a title, the reference field, the
/// reference on a slide); what is kept is named by [`label_in`].
pub fn reading_label(language: &str, first: VerseRef, last: VerseRef) -> String {
    let one_chapter = (first.book, first.chapter) == (last.book, last.chapter);
    label_with(reading_book_name(first.book, language, one_chapter), first, last)
}

/// A run of verses named with the book's name given.
fn label_with(book: &str, first: VerseRef, last: VerseRef) -> String {
    if first == last {
        format!("{book} {}:{}", first.chapter, first.verse)
    } else if first.chapter == last.chapter {
        format!("{book} {}:{}-{}", first.chapter, first.verse, last.verse)
    } else {
        format!("{book} {}:{}-{}:{}", first.chapter, first.verse, last.chapter, last.verse)
    }
}

/// Reads `GEN 1:1 words` into the verse's place and its words.
fn parse_verse_line(line: &str) -> Option<(VerseRef, &str)> {
    let (code, rest) = line.split_once(' ')?;
    let (place, words) = rest.split_once(' ')?;
    let (chapter, verse) = place.split_once(':')?;
    let place = VerseRef { book: book_by_code(code)?, chapter: chapter.parse().ok()?, verse: verse.parse().ok()? };
    Some((place, words.trim()))
}

/// Reads what someone typed, such as "John 3:16", "jn 3:16-18", "1 Cor 13", "Пс 22:1-3" or
/// "John 3:16-4:2", into the verses it names. The error is a plain sentence.
pub fn parse_reference(typed: &str) -> Result<Range, String> {
    parse_reference_in(typed, "")
}

/// The same, reading the book's name in a language first (`en`, `ru` or `uk`), as
/// [`books::find_book_in`] does.
pub fn parse_reference_in(typed: &str, language: &str) -> Result<Range, String> {
    read_reference(typed, language).map(|reference| reference.range)
}

/// A reference as it was typed: the verses it names, and the languages its book's name was
/// typed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub range: Range,
    /// `en`, `ru` or `uk`, in that order; two when the name is spelt alike in two languages
    /// ("Пс 23" and "1 Кор 13" are Russian and Ukrainian alike). See [`books::find_book_typed`].
    pub languages: Vec<&'static str>,
}

/// [`parse_reference_in`], also saying which languages the book's name was typed in. The verses
/// are exactly the ones `parse_reference_in` reads: the language only says how it was written.
pub fn read_reference(typed: &str, language: &str) -> Result<Reference, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Type a reference, such as John 3:16.".to_owned());
    }
    // The book is everything up to the first number that is not the "1" of "1 John".
    let mut split = None;
    let chars: Vec<(usize, char)> = typed.char_indices().collect();
    for (n, (at, c)) in chars.iter().enumerate() {
        if c.is_ascii_digit() && n > 0 && chars[..n].iter().any(|(_, c)| c.is_alphabetic()) {
            split = Some(*at);
            break;
        }
    }
    let (book_text, numbers) = match split {
        Some(at) => (&typed[..at], &typed[at..]),
        None => (typed, ""),
    };
    let (book, languages) = find_book_typed(book_text, language)
        .ok_or_else(|| format!("No book of the Bible is called \"{}\".", book_text.trim()))?;
    let unclear = || format!("\"{}\" is not a chapter and verse. Try something like 3:16 or 3:16-18.", numbers.trim());
    let number = |text: &str| text.trim().parse::<u16>().ok().filter(|n| *n > 0);
    // "3:16" or "3,16" or "3.16"; a space also separates chapter from verse.
    let place = |text: &str| -> Option<(u16, Option<u16>)> {
        let text = text.trim();
        match text.split_once([':', ',', '.', ' ']) {
            Some((chapter, verse)) => Some((number(chapter)?, Some(number(verse)?))),
            None => Some((number(text)?, None)),
        }
    };
    let numbers = numbers.trim();
    // "3:16,18" lists verses; it is read as 16 to 18.
    let listed;
    let numbers = match numbers.split_once(':') {
        Some((chapter, verses)) if verses.contains(',') && !verses.contains(['-', '\u{2013}', '\u{2014}']) => {
            let parts: Vec<&str> = verses.split(',').map(str::trim).filter(|part| !part.is_empty()).collect();
            match (parts.first(), parts.last()) {
                (Some(first), Some(last)) => {
                    listed = format!("{chapter}:{first}-{last}");
                    listed.as_str()
                }
                _ => numbers,
            }
        }
        _ => numbers,
    };
    if numbers.is_empty() {
        // A whole book with one chapter is fine; otherwise start at chapter 1.
        return Ok(Reference {
            range: Range {
                start: VerseRef { book, chapter: 1, verse: 1 },
                end: VerseRef { book, chapter: 1, verse: LAST },
            },
            languages,
        });
    }
    let (first, second) = match numbers.split_once(['-', '\u{2013}', '\u{2014}']) {
        Some((first, second)) => (first, Some(second)),
        None => (numbers, None),
    };
    let (chapter, verse) = place(first).ok_or_else(unclear)?;
    let start = VerseRef { book, chapter, verse: verse.unwrap_or(1) };
    let end = match second {
        None => VerseRef { book, chapter, verse: verse.unwrap_or(LAST) },
        Some(second) => match (place(second).ok_or_else(unclear)?, verse) {
            // "3:16-18": the second number is a verse of the same chapter.
            ((to, None), Some(_)) => VerseRef { book, chapter, verse: to },
            // "3-4": whole chapters.
            ((to, None), None) => VerseRef { book, chapter: to, verse: LAST },
            ((to_chapter, Some(to_verse)), _) => VerseRef { book, chapter: to_chapter, verse: to_verse },
        },
    };
    if end < start {
        return Err("The reference ends before it starts.".to_owned());
    }
    Ok(Reference { range: Range { start, end }, languages })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(numbering: Numbering, language: &str) -> Info {
        Info {
            id: "test".to_owned(),
            name: "Test Bible".to_owned(),
            short: "TB".to_owned(),
            language: language.to_owned(),
            terms: "Public domain".to_owned(),
            credit: String::new(),
            source: "made for a test".to_owned(),
            numbering,
            loose: false,
        }
    }

    fn small() -> Bible {
        let lines = "JOH 3:16 For God so loved the world\nJOH 3:17 For God sent not his Son\nJOH 3:18 He that believeth\n\
                     JOH 4:1 When therefore the Lord knew\nGEN 1:1 In the beginning\nXXX 1:1 not a book\nrubbish";
        Bible::from_verse_lines(info(Numbering::English, "en"), lines, str::to_owned)
    }

    fn range(book: &str, start: (u16, u16), end: (u16, u16)) -> Range {
        let book = book_by_code(book).unwrap();
        Range {
            start: VerseRef { book, chapter: start.0, verse: start.1 },
            end: VerseRef { book, chapter: end.0, verse: end.1 },
        }
    }

    #[test]
    fn a_list_of_verses_is_read_from_the_first_to_the_last() {
        let range = parse_reference("John 3:16,18").unwrap();
        assert_eq!((range.start.chapter, range.start.verse, range.end.verse), (3, 16, 18));
        // A comma between chapter and verse still works.
        let range = parse_reference("John 3,16").unwrap();
        assert_eq!((range.start.chapter, range.start.verse, range.end.verse), (3, 16, 16));
    }

    #[test]
    fn references_are_read_the_ways_people_type_them() {
        assert_eq!(parse_reference("John 3:16"), Ok(range("JOH", (3, 16), (3, 16))));
        assert_eq!(parse_reference("  jn 3:16-18 "), Ok(range("JOH", (3, 16), (3, 18))));
        assert_eq!(parse_reference("Jn3.16"), Ok(range("JOH", (3, 16), (3, 16))));
        assert_eq!(parse_reference("John 3 16"), Ok(range("JOH", (3, 16), (3, 16))));
        assert_eq!(parse_reference("John 3"), Ok(range("JOH", (3, 1), (3, LAST))));
        assert_eq!(parse_reference("John 3-4"), Ok(range("JOH", (3, 1), (4, LAST))));
        assert_eq!(parse_reference("John 3:16-4:2"), Ok(range("JOH", (3, 16), (4, 2))));
        assert_eq!(parse_reference("1 John 4:8"), Ok(range("1JO", (4, 8), (4, 8))));
        assert_eq!(parse_reference("1cor 13:4–7"), Ok(range("1CO", (13, 4), (13, 7))));
        assert_eq!(parse_reference("Psalm 23"), Ok(range("PSA", (23, 1), (23, LAST))));
        assert_eq!(parse_reference("Jude"), Ok(range("JUD", (1, 1), (1, LAST))));
        assert_eq!(parse_reference("Gene 1:1"), Ok(range("GEN", (1, 1), (1, 1))));
    }

    #[test]
    fn russian_references_are_read() {
        assert_eq!(parse_reference("Ин 3:16"), Ok(range("JOH", (3, 16), (3, 16))));
        assert_eq!(parse_reference("Иоанна 3:16"), Ok(range("JOH", (3, 16), (3, 16))));
        assert_eq!(parse_reference("От Иоанна 3:16"), Ok(range("JOH", (3, 16), (3, 16))));
        assert_eq!(parse_reference("Пс 22:1-3"), Ok(range("PSA", (22, 1), (22, 3))));
        assert_eq!(parse_reference("1 Кор 13"), Ok(range("1CO", (13, 1), (13, LAST))));
        assert_eq!(parse_reference("3 Царств 8:1"), Ok(range("1KI", (8, 1), (8, 1))));
    }

    #[test]
    fn a_reference_says_which_language_it_was_typed_in() {
        let languages = |typed: &str, preferred: &str| read_reference(typed, preferred).map(|r| r.languages);
        // English, Russian and Ukrainian, whole names and short forms.
        for typed in ["gen 1 1", "Genesis 1:1", "John 3:16", "jn 3:16-18", "1 Cor 13", "ps 23"] {
            assert_eq!(languages(typed, "ru"), Ok(vec!["en"]), "{typed}");
        }
        for typed in ["Быт 1 1", "Бытие 1:1", "Ин 3:16", "От Иоанна 3:16", "Иоанна 3:16"] {
            assert_eq!(languages(typed, "en"), Ok(vec!["ru"]), "{typed}");
        }
        for typed in ["Бут 1 1", "Буття 1:1", "Ів 3:16", "Від Івана 3:16", "1м 1:1"] {
            assert_eq!(languages(typed, "ru"), Ok(vec!["uk"]), "{typed}");
        }
        // Spelt alike in Russian and Ukrainian.
        assert_eq!(languages("Пс 23", "ru"), Ok(vec!["ru", "uk"]));
        assert_eq!(languages("1 Кор 13:4", "en"), Ok(vec!["ru", "uk"]));
        // Digits alone name no book, so no language.
        assert!(languages("23", "en").is_err());
        assert!(languages("3:16", "ru").is_err());
        // The verses are the ones parse_reference_in reads, whatever the language.
        for (typed, preferred) in [("Пс 22:1-3", "ru"), ("1 Цар 1", "uk"), ("gen 1 1", "ru"), ("jn 3", "")] {
            assert_eq!(read_reference(typed, preferred).map(|r| r.range), parse_reference_in(typed, preferred));
        }
    }

    #[test]
    fn a_passage_is_named_in_any_language_with_its_numbers_as_given() {
        let john = book_by_code("JOH").unwrap();
        let at = |chapter, verse| VerseRef { book: john, chapter, verse };
        assert_eq!(label_in("en", at(3, 16), at(3, 18)), "John 3:16-18");
        assert_eq!(label_in("ru", at(3, 16), at(3, 16)), "От Иоанна 3:16");
        assert_eq!(label_in("uk", at(3, 16), at(4, 2)), "Від Івана 3:16-4:2");
        assert_eq!(label_in("de", at(3, 16), at(3, 16)), "John 3:16");
        assert_eq!(small().label(at(3, 16), at(3, 17)), label_in("en", at(3, 16), at(3, 17)));
    }

    #[test]
    fn a_psalm_reads_as_one_and_psalms_as_several_and_each_reads_back_as_the_same_verses() {
        let psalms = book_by_code("PSA").unwrap();
        let at = |chapter, verse| VerseRef { book: psalms, chapter, verse };
        let john = |chapter, verse| VerseRef { book: book_by_code("JOH").unwrap(), chapter, verse };
        for (language, first, last, read) in [
            ("en", at(23, 1), at(23, 6), "Psalm 23:1-6"),
            ("en", at(23, 4), at(23, 4), "Psalm 23:4"),
            ("en", at(23, 1), at(24, 10), "Psalms 23:1-24:10"),
            ("ru", at(22, 1), at(22, 6), "Псалом 22:1-6"),
            ("ru", at(22, 1), at(23, 6), "Псалтирь 22:1-23:6"),
            ("uk", at(22, 1), at(22, 6), "Псалом 22:1-6"),
            ("uk", at(22, 1), at(23, 6), "Псалми 22:1-23:6"),
            ("en", john(3, 16), john(3, 16), "John 3:16"),
            ("ru", john(3, 16), john(3, 18), "От Иоанна 3:16-18"),
        ] {
            assert_eq!(reading_label(language, first, last), read, "{language}");
            // Read back in any of the languages, it is the same verses.
            for reader in ["en", "ru", "uk", ""] {
                let range = parse_reference_in(read, reader).map(|range| (range.start, range.end));
                assert_eq!(range, Ok((first, last)), "{read} read in {reader:?}");
            }
        }
        // What is kept keeps the books' own names.
        assert_eq!(label_in("en", at(23, 1), at(23, 6)), "Psalms 23:1-6");
        assert_eq!(label_in("ru", at(22, 1), at(22, 6)), "Псалтирь 22:1-6");
    }

    #[test]
    fn unclear_references_are_refused_in_plain_words() {
        assert!(parse_reference("").unwrap_err().contains("Type a reference"));
        assert!(parse_reference("Hezekiah 3:1").unwrap_err().contains("No book"));
        assert!(parse_reference("John 3:x").unwrap_err().contains("not a chapter and verse"));
        assert!(parse_reference("John 3:18-16").unwrap_err().contains("ends before"));
        assert!(parse_reference("John 0:1").is_err());
    }

    #[test]
    fn verses_are_found_in_order_and_other_lines_are_left_out() {
        let bible = small();
        assert_eq!(bible.verse_count(), 5);
        let found = bible.verses(parse_reference("John 3:16-17").unwrap());
        assert_eq!(
            found.iter().map(|(_, words)| *words).collect::<Vec<_>>(),
            ["For God so loved the world", "For God sent not his Son"]
        );
        assert_eq!(bible.verses(parse_reference("John 3").unwrap()).len(), 3);
        assert_eq!(bible.verses(parse_reference("John 3:18-4:1").unwrap()).len(), 2);
        assert!(bible.verses(parse_reference("John 9").unwrap()).is_empty());
        assert_eq!(bible.verse(VerseRef { book: 0, chapter: 1, verse: 1 }), Some("In the beginning"));
    }

    #[test]
    fn a_bible_survives_being_written_and_read() {
        let bible = small();
        let again = Bible::from_bytes(&bible.to_bytes()).unwrap();
        assert_eq!(again.info, bible.info);
        assert_eq!(Bible::read_info(&bible.to_bytes()), Ok(bible.info.clone()));
        assert!(Bible::read_info(b"hello").is_err());
        assert_eq!(again.all().collect::<Vec<_>>(), bible.all().collect::<Vec<_>>());
    }

    #[test]
    fn files_that_are_not_bibles_are_refused() {
        assert!(Bible::from_bytes(b"hello").is_err());
        assert!(Bible::from_text("something else\n").is_err());
        assert!(Bible::from_text("softpresenter-bible 1\nid: x\nname: X\n\n").unwrap_err().contains("no verses"));
        assert!(Bible::from_text("softpresenter-bible 1\n\nGEN 1:1 words\n").unwrap_err().contains("which Bible"));
    }

    #[test]
    fn chapters_and_verses_are_counted_and_passages_are_stepped_through() {
        let bible = small();
        let john = book_by_code("JOH").unwrap();
        assert_eq!(bible.chapter_count(john), 4);
        assert_eq!(bible.last_verse(john, 3), 18);
        assert_eq!((bible.chapter_count(5), bible.last_verse(john, 9)), (0, 0));
        // From 3:16 one verse at a time, across the chapter's end, to the book's end.
        let one = parse_reference("John 3:16").unwrap();
        let next = bible.step(one, true).unwrap();
        assert_eq!((next.start.verse, next.end.verse), (17, 17));
        let two = bible.step(parse_reference("John 3:17-18").unwrap(), true).unwrap();
        assert_eq!((two.start.chapter, two.start.verse, two.end), (4, 1, two.start));
        assert!(bible.step(two, true).is_none());
        // Backwards, as many verses as the range has, stopping at the book's start.
        let back = bible.step(parse_reference("John 3:18-4:1").unwrap(), false).unwrap();
        assert_eq!((back.start.verse, back.end.verse), (16, 17));
        assert!(bible.step(one, false).is_none());
        assert!(bible.step(parse_reference("John 9").unwrap(), true).is_none());
        // A passage grows by one verse at either end, across a chapter's end.
        let grown = bible.extend(parse_reference("John 3:17").unwrap(), true).unwrap();
        assert_eq!((grown.start.verse, grown.end.verse), (17, 18));
        let grown = bible.extend(grown, true).unwrap();
        assert_eq!((grown.end.chapter, grown.end.verse), (4, 1));
        assert!(bible.extend(grown, true).is_none());
        let grown = bible.extend(parse_reference("John 3:17").unwrap(), false).unwrap();
        assert_eq!((grown.start.verse, grown.end.verse), (16, 17));
        assert!(bible.extend(grown, false).is_none());
    }

    #[test]
    fn search_wants_every_word_and_ignores_case() {
        let bible = small();
        assert_eq!(bible.search("god SON", 10).len(), 1);
        // Words together as typed come before words that are only both present.
        let ranked = bible.search("god sent", 10);
        assert_eq!(ranked.first().map(|(place, _)| place.verse), Some(17));
        assert_eq!(bible.search("god", 1).len(), 1);
        assert!(bible.search("  ", 10).is_empty());
    }

    #[test]
    fn labels_name_the_verses_shown_in_the_bibles_language() {
        let english = small();
        let john = book_by_code("JOH").unwrap();
        let at = |chapter, verse| VerseRef { book: john, chapter, verse };
        assert_eq!(english.label(at(3, 16), at(3, 16)), "John 3:16");
        assert_eq!(english.label(at(3, 16), at(3, 18)), "John 3:16-18");
        assert_eq!(english.label(at(3, 16), at(4, 2)), "John 3:16-4:2");
        let russian =
            Bible::from_verse_lines(info(Numbering::Synodal, "ru"), "JOH 3:16 Ибо так возлюбил", str::to_owned);
        assert_eq!(russian.label(at(3, 16), at(3, 16)), "От Иоанна 3:16");
    }
}
