//! SoftProjector's Bible files (`.spb`), as it has exported them since 2011: UTF-8 text,
//! one line for each thing and tabs between its parts. The layout was learnt from SoftProjector's
//! source (GPL-3.0), which writes and reads these files; none of its code is used.
//!
//! - The first line says the file's version: `##spDataVersion:`, a tab, `1`. A file without it
//!   is of the kind SoftProjector made before, which SoftProjector itself no longer reads; any
//!   other version is newer than this reader.
//! - Then lines starting `##` about the Bible: `##Title:`, `##Abbreviation:`, `##Information:`
//!   (its lines joined with `@%`) and `##RightToLeft:`, each with a tab before its value. The
//!   title, abbreviation and information become the Bible's name, short name and terms; the
//!   file doesn't say its language, which is offered from the letters of its words.
//! - Then the books, a line each: the book's number, its name and how many chapters it has.
//! - A line of dashes (`-----`), then the verses, a line each: an identifier, the book's
//!   number, the chapter, the verse and the words. The place is the book, chapter and verse;
//!   the identifier, which SoftProjector uses to show the same verse from several Bibles, is
//!   not needed.
//!
//! SoftProjector uses a book's number only to tie its verses to it. The numbers are taken as 1
//! (Genesis) to 66 (Revelation), the order English Bibles have them, as other formats number
//! them; the names the file gives the books are checked against that, and when they name the
//! books in another order, the names decide.

use std::collections::HashMap;

use super::{About, Collector, guess_language, one_line, shown_name, verse_number};
use crate::{VerseRef, book_by_number, book_by_written_name};

/// The most books a file may list. There are sixty-six, and some Bibles add a dozen more.
const MOST_BOOKS: usize = 300;

/// One book as the file lists it.
struct Listed<'a> {
    id: u32,
    name: &'a str,
}

/// What the file says about its Bible, and the language its books' names are written in
/// (empty when they don't tell).
pub(super) struct Said {
    pub about: About,
    pub language: String,
}

/// True when a file starts as SoftProjector's Bible files with a version do (whatever
/// version it then gives).
pub(super) fn looks_like_softprojector(start: &str) -> bool {
    start.starts_with("##spData")
}

/// The sentence for a file ending `.spb` that isn't of the kind read: one SoftProjector made
/// before, with no line saying its version.
pub(super) fn older_kind(name: &str) -> String {
    format!(
        "{name} is a SoftProjector Bible file of an older kind, which SoftProjector itself no longer reads. \
Export the Bible again from a recent SoftProjector and install that file."
    )
}

/// Reads the verses of a SoftProjector Bible file into `out`. `name` is the file's, for the
/// sentence that says what is wrong with it.
pub(super) fn read(text: &str, name: &str, out: &mut Collector) -> Result<Said, String> {
    let name = shown_name(name);
    // Lines counted from 1, as a person reading the file counts them, without the ends Windows
    // gives them.
    let mut lines = text
        .trim_start_matches('\u{feff}')
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .enumerate()
        .map(|(at, line)| (at + 1, line))
        .skip_while(|(_, line)| line.trim().is_empty())
        .peekable();

    // The version.
    let first = lines.next().map(|(_, line)| line.trim_start()).unwrap_or_default();
    if !looks_like_softprojector(first) {
        return Err(older_kind(&name));
    }
    let Some((_, version)) = first.split_once('\t') else {
        return Err(format!(
            "{name} doesn't say which version of SoftProjector's Bible files it is, so it wasn't read."
        ));
    };
    let version = version.trim();
    if version != "1" {
        let which = if !version.is_empty() && version.len() <= 6 && version.chars().all(|c| c.is_ascii_digit()) {
            format!(" (version {version})")
        } else {
            String::new()
        };
        return Err(format!("{name} is in a newer SoftProjector Bible format{which} than SoftPresenter can read."));
    }

    // What it says about the Bible, by each line's name (SoftProjector writes them in one order).
    let mut about = About::default();
    while let Some((_, line)) = lines.next_if(|(_, line)| line.trim().is_empty() || line.starts_with("##")) {
        let Some((key, value)) = line.split_once('\t') else { continue };
        let key = key.trim_start_matches('#').trim().trim_end_matches(':').trim();
        match key.to_ascii_lowercase().as_str() {
            "title" => about.name = value.to_owned(),
            "abbreviation" => about.short = value.to_owned(),
            // Its lines, which SoftProjector joins with `@%`, become one when tidied.
            "information" => about.terms = value.replace("@%", "\n"),
            _ => {}
        }
    }

    // The books, up to the line of dashes.
    let mut listed: Vec<Listed> = Vec::new();
    let mut dashes = false;
    for (at, line) in lines.by_ref() {
        if line.starts_with("---") {
            dashes = true;
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        // A verse (five parts) where a book should be: the line of dashes is missing.
        if line.split('\t').nth(4).is_some() {
            return Err(no_dashes(&name));
        }
        let mut parts = line.split('\t');
        let id = parts.next().and_then(|id| id.trim().parse::<u32>().ok());
        let (Some(id), Some(book_name)) = (id, parts.next()) else {
            return Err(format!(
                "Line {at} of {name} isn't a book as SoftProjector lists one (its number, name and chapters, with tabs between)."
            ));
        };
        if listed.iter().any(|book| book.id == id) {
            return Err(format!("{name} lists book {id} twice, so it wasn't read."));
        }
        if listed.len() >= MOST_BOOKS {
            return Err(format!("{name} lists more books than any Bible has, so it wasn't read."));
        }
        listed.push(Listed { id, name: book_name.trim() });
    }
    if !dashes {
        return Err(no_dashes(&name));
    }
    let books = place_books(&listed, out);
    let language = guess_language(listed.iter().map(|book| book.name));

    // The verses.
    for (at, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(5, '\t').collect();
        let [_id, book, chapter, verse, words] = parts.as_slice() else {
            return Err(not_a_verse(at, &name));
        };
        let (Ok(book), Ok(chapter)) = (book.trim().parse::<u32>(), chapter.trim().parse::<u32>()) else {
            return Err(not_a_verse(at, &name));
        };
        let Some((verse, last)) = verse_number(verse) else {
            // A verse numbered 0 is no verse the app has, and is left out like one past the
            // last; a verse that isn't numbered at all is a broken line.
            if verse.trim().starts_with(|c: char| c.is_ascii_digit()) {
                continue;
            }
            return Err(not_a_verse(at, &name));
        };
        let Some(found) = books.get(&book) else {
            return Err(format!(
                "Line {at} of {name} is a verse of book {book}, which isn't in the file's list of books."
            ));
        };
        // A book the app doesn't have is left out (and named); so is a chapter past any there is
        // (`Collector::begin` says so).
        let Some(book) = *found else { continue };
        let place = VerseRef { book, chapter: u16::try_from(chapter).unwrap_or(u16::MAX), verse };
        if out.begin(place, last) {
            out.add(place, words);
        }
    }
    Ok(Said { about, language })
}

fn no_dashes(name: &str) -> String {
    format!("{name} has no line of dashes after its list of books, so its verses can't be found.")
}

fn not_a_verse(line: usize, name: &str) -> String {
    format!(
        "Line {line} of {name} isn't a verse as SoftProjector writes one (its number, book, chapter, verse and words, with tabs between)."
    )
}

/// Each listed book's place in [`crate::BOOKS`], by its number: the usual order. When the
/// books' names, read in English, Russian or Ukrainian, name them in another order, the names
/// decide instead: at least half of the names must be known, each a different book, and they must
/// disagree with the numbers about two books at least (as any other order does; a single name
/// taken for the wrong book doesn't move it). It is one way or the other for the whole file,
/// never some books each way, so two books never land on one. Books the app doesn't have are
/// left out, named.
fn place_books(listed: &[Listed], out: &mut Collector) -> HashMap<u32, Option<usize>> {
    let by_number: Vec<Option<usize>> = listed.iter().map(|book| book_by_number(book.id)).collect();
    let by_name: Vec<Option<usize>> = listed.iter().map(|book| book_by_written_name(book.name)).collect();
    let known: Vec<usize> = by_name.iter().flatten().copied().collect();
    let mut distinct = known.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let disagree = by_name.iter().zip(&by_number).filter(|(name, number)| name.is_some() && name != number).count();
    let names_decide = known.len() * 2 >= listed.len() && distinct.len() == known.len() && disagree >= 2;
    let chosen = if names_decide { by_name } else { by_number };
    let mut books = HashMap::new();
    for (book, found) in listed.iter().zip(chosen) {
        if found.is_none() {
            let label = if book.name.is_empty() { format!("book {}", book.id) } else { one_line(book.name, 60) };
            out.leave_out(&label);
        }
        books.insert(book.id, found);
    }
    books
}
