//! Bibles do not all number their verses the same way. The Russian Synodal text counts a
//! psalm's title as a verse, joins and splits a few psalms, and starts some chapters a verse
//! or two earlier or later than English Bibles do; the Hebrew text differs from both. This
//! turns a verse's number in one tradition into its number in another, so two Bibles can be
//! shown side by side.
//!
//! The numbers come from the standard tables in `data/` (see its README): each says how its
//! tradition's verses map to the original-language numbering, so any two can be matched by
//! going through the original.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::VerseRef;
use crate::books::{BOOKS, book_by_standard_code};

/// A way of numbering verses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Numbering {
    /// As in the King James Version and most English Bibles.
    English,
    /// As in the Russian Synodal translation (its Protestant edition).
    Synodal,
    /// As in the Hebrew and Greek texts.
    Original,
}

impl Numbering {
    pub fn name(self) -> &'static str {
        match self {
            Numbering::English => "english",
            Numbering::Synodal => "synodal",
            Numbering::Original => "original",
        }
    }

    pub fn from_name(name: &str) -> Option<Numbering> {
        match name {
            "english" => Some(Numbering::English),
            "synodal" => Some(Numbering::Synodal),
            "original" => Some(Numbering::Original),
            _ => None,
        }
    }
}

/// One tradition's verses and the original-numbering verse each one is.
struct Table {
    to_original: HashMap<VerseRef, VerseRef>,
    from_original: HashMap<VerseRef, VerseRef>,
}

/// The tables name books by their standard codes; a few differ from the codes in Bible files.
fn book_of(code: &str) -> Option<usize> {
    book_by_standard_code(code)
}

/// Reads `3:1-8` into chapter 3, verses 1 to 8.
fn span(text: &str) -> Option<(u16, u16, u16)> {
    let (chapter, verses) = text.split_once(':')?;
    let (from, to) = verses.split_once('-').unwrap_or((verses, verses));
    Some((chapter.parse().ok()?, from.parse().ok()?, to.parse().ok()?))
}

/// Reads a table's mapping lines, such as `PSA 51:1-19 = PSA 51:3-21`. Other lines (the lists
/// of chapter lengths, notes, books beyond the sixty-six) are passed over.
fn read(table: &str) -> Table {
    let mut to_original = HashMap::new();
    let mut from_original = HashMap::new();
    for line in table.lines() {
        let line = line.split('#').next().unwrap_or_default();
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [book, here, "=", original_book, there] = parts.as_slice() else { continue };
        let (Some(book), Some(original_book)) = (book_of(book), book_of(original_book)) else { continue };
        let (Some((chapter, from, to)), Some((original_chapter, original_from, _))) = (span(here), span(there)) else {
            continue;
        };
        for step in 0..=to.saturating_sub(from) {
            let ours = VerseRef { book, chapter, verse: from + step };
            let original = VerseRef { book: original_book, chapter: original_chapter, verse: original_from + step };
            // Where a table gives a verse two partners, the first stands.
            to_original.entry(ours).or_insert(original);
            from_original.entry(original).or_insert(ours);
        }
    }
    Table { to_original, from_original }
}

fn table(numbering: Numbering) -> Option<&'static Table> {
    static ENGLISH: OnceLock<Table> = OnceLock::new();
    static SYNODAL: OnceLock<Table> = OnceLock::new();
    match numbering {
        Numbering::English => Some(ENGLISH.get_or_init(|| read(include_str!("../data/eng.vrs")))),
        Numbering::Synodal => Some(SYNODAL.get_or_init(|| read(include_str!("../data/rsc.vrs")))),
        Numbering::Original => None,
    }
}

/// The same verse under another way of numbering. Where the traditions agree the number is
/// unchanged.
pub fn convert(verse: VerseRef, from: Numbering, to: Numbering) -> VerseRef {
    if from == to {
        return verse;
    }
    let original = table(from).and_then(|t| t.to_original.get(&verse)).copied().unwrap_or(verse);
    let there = table(to).and_then(|t| t.from_original.get(&original)).copied().unwrap_or(original);
    // The tables count a psalm's title as verse 0 where a tradition leaves it unnumbered.
    // Bible files keep such a title in verse 1.
    VerseRef { verse: there.verse.max(1), ..there }
}

/// How many verses each chapter has under one way of numbering, from the lists at the top of
/// its table (`PSA 1:6 2:12 3:8 …`, a chapter and its last verse).
fn chapter_lengths(table: &str) -> HashMap<(usize, u16), u16> {
    let mut lengths = HashMap::new();
    for line in table.lines() {
        let line = line.split('#').next().unwrap_or_default();
        if line.contains('=') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(book) = parts.next().and_then(book_of) else { continue };
        for part in parts {
            if let Some((chapter, last)) = part.split_once(':')
                && let (Ok(chapter), Ok(last)) = (chapter.parse(), last.parse())
            {
                lengths.insert((book, chapter), last);
            }
        }
    }
    lengths
}

type Lengths = HashMap<(usize, u16), u16>;

fn lengths(numbering: Numbering) -> &'static Lengths {
    static ENGLISH: OnceLock<Lengths> = OnceLock::new();
    static SYNODAL: OnceLock<Lengths> = OnceLock::new();
    static ORIGINAL: OnceLock<Lengths> = OnceLock::new();
    match numbering {
        Numbering::English => ENGLISH.get_or_init(|| chapter_lengths(include_str!("../data/eng.vrs"))),
        Numbering::Synodal => SYNODAL.get_or_init(|| chapter_lengths(include_str!("../data/rsc.vrs"))),
        Numbering::Original => ORIGINAL.get_or_init(|| chapter_lengths(include_str!("../data/org.vrs"))),
    }
}

/// The way a Bible numbers its verses, as judged from how long its chapters are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub numbering: Numbering,
    /// True when more than one chapter in fifty is longer or shorter than that numbering has
    /// it, so that beside another Bible a verse here and there may be one off. A few differ in
    /// nearly every Bible (the King James ends 3 John at verse 14, where the table has 15), and
    /// those only add or leave out a verse at a chapter's end.
    pub loose: bool,
    /// How many of its chapters differ in length from that numbering.
    pub differ: usize,
    /// How many of its chapters tell the ways of numbering apart (their length differs between
    /// them). With none there was nothing to judge by, and English is taken.
    pub telling: usize,
    /// True when its Psalms alone would have decided the same: they tell the Synodal numbering
    /// from the others.
    pub by_psalms: bool,
}

/// Judges the way a Bible numbers its verses from each chapter's last verse number (`chapters`:
/// book and chapter, and the last verse): the way of numbering fewest of its chapters differ
/// from. With nothing to tell them apart, or a tie, English is taken (then the Synodal, then the
/// original's).
pub fn fit(chapters: &std::collections::BTreeMap<(usize, u16), u16>) -> Fit {
    const ALL: [Numbering; 3] = [Numbering::English, Numbering::Synodal, Numbering::Original];
    let telling = |place: &(usize, u16)| {
        let [english, synodal, original] = ALL.map(|numbering| lengths(numbering).get(place));
        english != synodal || english != original
    };
    let psalm = |place: &(usize, u16)| BOOKS[place.0].code == "PSA";
    let differ = |numbering: Numbering, psalms_only: bool| {
        let table = lengths(numbering);
        chapters
            .iter()
            .filter(|(place, _)| !psalms_only || psalm(place))
            .filter(|(place, last)| table.get(*place) != Some(*last))
            .count()
    };
    // `min_by_key` keeps the first of equals, so a tie goes to the earlier in `ALL`.
    let best = |psalms_only: bool| {
        ALL.into_iter().min_by_key(|numbering| differ(*numbering, psalms_only)).unwrap_or(Numbering::English)
    };
    let telling_chapters = chapters.keys().filter(|place| telling(place)).count();
    let numbering = if telling_chapters == 0 { Numbering::English } else { best(false) };
    let telling_psalms = chapters.keys().filter(|place| psalm(place) && telling(place)).count();
    let differ_count = differ(numbering, false);
    Fit {
        numbering,
        loose: differ_count * 50 > chapters.len(),
        differ: differ_count,
        telling: telling_chapters,
        by_psalms: telling_psalms > 0 && best(true) == numbering,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(code: &str, chapter: u16, verse: u16) -> VerseRef {
        VerseRef { book: crate::books::book_by_code(code).unwrap(), chapter, verse }
    }

    fn both_ways(code: &str, english: (u16, u16), synodal: (u16, u16)) {
        let (e, s) = (at(code, english.0, english.1), at(code, synodal.0, synodal.1));
        assert_eq!(convert(e, Numbering::English, Numbering::Synodal), s, "{code} {english:?} to Synodal");
        assert_eq!(convert(s, Numbering::Synodal, Numbering::English), e, "{code} {synodal:?} to English");
    }

    #[test]
    fn the_tables_are_read() {
        assert!(table(Numbering::English).unwrap().to_original.len() > 2000);
        assert!(table(Numbering::Synodal).unwrap().to_original.len() > 2000);
    }

    #[test]
    fn psalms_line_up() {
        both_ways("PSA", (1, 1), (1, 1));
        both_ways("PSA", (3, 1), (3, 2));
        both_ways("PSA", (9, 1), (9, 2));
        both_ways("PSA", (10, 1), (9, 22));
        both_ways("PSA", (23, 1), (22, 1));
        both_ways("PSA", (51, 1), (50, 3));
        both_ways("PSA", (115, 1), (113, 9));
        both_ways("PSA", (116, 10), (115, 1));
        both_ways("PSA", (119, 105), (118, 105));
        both_ways("PSA", (147, 12), (147, 1));
        both_ways("PSA", (150, 6), (150, 6));
    }

    #[test]
    fn a_title_verse_belongs_to_the_first_english_verse() {
        assert_eq!(convert(at("PSA", 50, 1), Numbering::Synodal, Numbering::English), at("PSA", 51, 1));
        assert_eq!(convert(at("PSA", 50, 2), Numbering::Synodal, Numbering::English), at("PSA", 51, 1));
        assert_eq!(convert(at("PSA", 3, 1), Numbering::Synodal, Numbering::English), at("PSA", 3, 1));
    }

    #[test]
    fn chapters_that_start_differently_line_up() {
        both_ways("JON", (1, 17), (2, 1));
        both_ways("JON", (2, 1), (2, 2));
        both_ways("DAN", (4, 1), (3, 31));
        both_ways("DAN", (4, 37), (4, 34));
        both_ways("JOB", (41, 1), (40, 20));
        both_ways("ROM", (16, 25), (14, 24));
        both_ways("JOH", (3, 16), (3, 16));
    }

    #[test]
    fn hebrew_numbering_is_reached_from_both() {
        // "Have mercy upon me, O God": English 51:1, Synodal 50:3, Hebrew 51:3.
        assert_eq!(convert(at("PSA", 51, 1), Numbering::English, Numbering::Original), at("PSA", 51, 3));
        assert_eq!(convert(at("PSA", 50, 3), Numbering::Synodal, Numbering::Original), at("PSA", 51, 3));
        assert_eq!(convert(at("PSA", 51, 3), Numbering::Original, Numbering::Synodal), at("PSA", 50, 3));
        // Malachi 4 in English is the end of chapter 3 in Hebrew.
        assert_eq!(convert(at("MAL", 4, 1), Numbering::English, Numbering::Original), at("MAL", 3, 19));
    }

    #[test]
    fn the_same_numbering_changes_nothing() {
        let verse = at("PSA", 23, 1);
        assert_eq!(convert(verse, Numbering::English, Numbering::English), verse);
    }
}
