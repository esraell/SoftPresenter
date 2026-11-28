//! The Bible page's working parts: the Bibles installed, turning a typed reference into
//! slides, and the Options pop-up that says how.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, pos2, vec2};

use std::sync::Arc;

use sp_bible::{Bible, Numbering, VerseRef, convert, parse_reference_in};
use sp_model::{Group, Object, Presentation, Rgba, Role, Slide, Uuid};

use crate::home::BibleTarget;
use crate::i18n::{t, tf, tk, tn};
use crate::quick_help::Hint;

/// Where a Bible's file is.
#[derive(Clone)]
enum Kept {
    /// Inside the app.
    Shipped(&'static [u8]),
    /// In the Bibles folder.
    File(std::path::PathBuf),
}

/// A Bible the app has, and its text once it has been needed.
pub struct Shelved {
    pub id: String,
    pub name: String,
    pub short: String,
    /// Its language, as its file says it (`en`, `ru`, `uk`…).
    pub language: String,
    pub terms: String,
    pub credit: String,
    /// True when it only mostly follows that numbering.
    pub loose: bool,
    pub shipped: bool,
    kept: Kept,
    /// Read the first time it is asked for, so an unused Bible costs no memory.
    loaded: Option<Result<Arc<Bible>, String>>,
}

/// A Bible to be read away from the window's thread: see [`Shelf::reader`].
pub struct BibleReader {
    pub id: String,
    pub short: String,
    /// The Bible, when the shelf has read it already.
    pub loaded: Option<Arc<Bible>>,
    name: String,
    kept: Kept,
}

impl BibleReader {
    /// The Bible, read now if it has not been. Slow the first time: not for the window's thread.
    pub fn read(self) -> Result<Arc<Bible>, String> {
        if let Some(bible) = self.loaded {
            return Ok(bible);
        }
        match &self.kept {
            Kept::Shipped(bytes) => Bible::from_bytes(bytes).map(Arc::new),
            Kept::File(path) => std::fs::read(path)
                .map_err(|e| {
                    let error = e.to_string();
                    tf(
                        "bibles.not_read",
                        "Could not read the {bible}: {error}",
                        &[("bible", &self.name), ("error", &error)],
                    )
                })
                .and_then(|bytes| Bible::from_bytes(&bytes).map(Arc::new)),
        }
    }
}

/// The Bibles installed: the ones that come with the app, then the ones added since.
pub struct Shelf {
    pub bibles: Vec<Shelved>,
}

impl std::fmt::Debug for Shelf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.bibles.iter().map(|b| b.id.as_str())).finish()
    }
}

impl Shelf {
    /// The Bibles that come with the app. All are in the public domain; see
    /// `docs/licences/bibles.md`.
    pub fn shipped() -> Shelf {
        let files: [&'static [u8]; 4] = [
            include_bytes!("../../../assets/bibles/kjv.spbible"),
            include_bytes!("../../../assets/bibles/web.spbible"),
            include_bytes!("../../../assets/bibles/bsb.spbible"),
            include_bytes!("../../../assets/bibles/syn.spbible"),
        ];
        let mut shelf = Shelf { bibles: Vec::new() };
        for bytes in files {
            if let Ok(info) = Bible::read_info(bytes) {
                shelf.put(info, Kept::Shipped(bytes), true);
            }
        }
        shelf
    }

    /// The Bibles that come with the app and the ones in the Bibles folder.
    pub fn with_installed() -> Shelf {
        let mut shelf = Shelf::shipped();
        for (path, info) in crate::bible_store::installed() {
            shelf.add_file(path, info);
        }
        shelf
    }

    fn put(&mut self, info: sp_bible::Info, kept: Kept, shipped: bool) {
        self.bibles.push(Shelved {
            id: info.id,
            name: info.name,
            short: info.short,
            language: info.language,
            terms: info.terms,
            credit: info.credit,
            loose: info.loose,
            shipped,
            kept,
            loaded: None,
        });
    }

    /// Puts a Bible from the Bibles folder on the shelf. One already there under the same
    /// short name stays, at its place, so the translations chosen do not shift.
    pub fn add_file(&mut self, path: std::path::PathBuf, info: sp_bible::Info) {
        if self.position(&info.id).is_none() {
            self.put(info, Kept::File(path), false);
        }
    }

    /// Puts a Bible just installed from a file on the shelf: one installed before under the
    /// same short name for files (never one built in) is replaced at its place, and read again
    /// when next asked for.
    pub fn put_installed(&mut self, path: std::path::PathBuf, info: sp_bible::Info) {
        match self.bibles.iter_mut().find(|shelved| shelved.id.eq_ignore_ascii_case(&info.id) && !shelved.shipped) {
            Some(shelved) => {
                shelved.name = info.name;
                shelved.short = info.short;
                shelved.language = info.language;
                shelved.terms = info.terms;
                shelved.credit = info.credit;
                shelved.loose = info.loose;
                shelved.kept = Kept::File(path);
                shelved.loaded = None;
            }
            None => self.add_file(path, info),
        }
    }

    /// What an install from a file, on a thread of its own, needs to know of the shelf: each
    /// Bible's short name for files, name and short name, and whether it is built in.
    pub fn known(&self) -> crate::bible_store::Known {
        crate::bible_store::Known {
            bibles: self
                .bibles
                .iter()
                .map(|shelved| crate::bible_store::KnownBible {
                    id: shelved.id.clone(),
                    name: shelved.name.clone(),
                    short: shelved.short.clone(),
                    shipped: shelved.shipped,
                })
                .collect(),
        }
    }

    /// Takes a downloaded Bible off the shelf and moves its file to the Trash. The Bibles that
    /// come with the app cannot be removed.
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        let Some(shelved) = self.bibles.get(index) else {
            return Err(t("bibles.no_such", "There is no such Bible.").to_owned());
        };
        match &shelved.kept {
            Kept::Shipped(_) => {
                return Err(tf(
                    "bibles.built_in_stays",
                    "The {bible} comes with the app and stays.",
                    &[("bible", &shelved.name)],
                ));
            }
            Kept::File(path) => {
                if path.exists() {
                    crate::sweep::to_trash(path).map_err(|e| {
                        let error = e.to_string();
                        tf(
                            "bibles.not_trashed",
                            "Could not move the {bible} to the Trash: {error}",
                            &[("bible", &shelved.name), ("error", &error)],
                        )
                    })?;
                }
            }
        }
        self.bibles.remove(index);
        Ok(())
    }

    /// What it takes to read the Bible at a place on the shelf somewhere other than the
    /// window's thread (the find box searches there): the Bible itself when it has been read
    /// already, otherwise where it is kept.
    pub fn reader(&self, index: usize) -> Option<BibleReader> {
        let shelved = self.bibles.get(index)?;
        Some(BibleReader {
            id: shelved.id.clone(),
            short: shelved.short.clone(),
            loaded: shelved.loaded.as_ref().and_then(|loaded| loaded.as_ref().ok().cloned()),
            name: shelved.name.clone(),
            kept: shelved.kept.clone(),
        })
    }

    /// Keeps a Bible that was read somewhere else, so it is not read again here.
    pub fn keep(&mut self, id: &str, bible: Arc<Bible>) {
        if let Some(shelved) = self.bibles.iter_mut().find(|shelved| shelved.id == id)
            && shelved.loaded.is_none()
        {
            shelved.loaded = Some(Ok(bible));
        }
    }

    pub fn position(&self, id: &str) -> Option<usize> {
        self.bibles.iter().position(|b| b.id == id)
    }

    /// The Bible at a place on the shelf, reading it the first time.
    pub fn get(&mut self, index: usize) -> Result<Arc<Bible>, String> {
        let shelved =
            self.bibles.get_mut(index).ok_or_else(|| t("bibles.choose_first", "Choose a Bible first.").to_owned())?;
        shelved
            .loaded
            .get_or_insert_with(|| match &shelved.kept {
                Kept::Shipped(bytes) => Bible::from_bytes(bytes).map(Arc::new),
                Kept::File(path) => std::fs::read(path)
                    .map_err(|e| {
                        let error = e.to_string();
                        tf(
                            "bibles.not_read",
                            "Could not read the {bible}: {error}",
                            &[("bible", &shelved.name), ("error", &error)],
                        )
                    })
                    .and_then(|bytes| Bible::from_bytes(&bytes).map(Arc::new)),
            })
            .clone()
    }
}

/// The numbering a reference written in a language is read in, and a name in that language is
/// numbered in (the owner's rule, 2026-11-22: chapter and verse numbers follow the language):
/// that of the first Bible in use written in it; with none, the English numbering for English
/// and the Synodal for Russian; any other language (Ukrainian with no Ukrainian Bible in use)
/// the first Bible's. `bibles` are the Bibles in use, the first translation first.
pub fn numbering_of(language: &str, bibles: &[&Bible]) -> Numbering {
    let language = crate::bible_language::language_of(language);
    if let Some(bible) =
        bibles.iter().find(|bible| crate::bible_language::language_of(&bible.info.language) == language)
    {
        return bible.info.numbering;
    }
    match language.as_str() {
        "en" => Numbering::English,
        "ru" => Numbering::Synodal,
        _ => bibles.first().map_or(Numbering::English, |lead| lead.info.numbering),
    }
}

/// A run of verses read in one numbering (`from`), as another (`to`) numbers it: its real
/// first and last verses, found in a Bible of `from` among `bibles` when one is in use (else as
/// given, a chapter's end matched through its first verse), each turned through the
/// versification tables. "Psalm 23" in the English numbering is the Synodal's 22:1-6.
pub fn renumber(range: sp_bible::Range, from: Numbering, to: Numbering, bibles: &[&Bible]) -> sp_bible::Range {
    let verses = bibles.iter().find(|bible| bible.info.numbering == from).map(|bible| bible.verses(range));
    let (first, last) = match verses.as_deref() {
        Some([(first, _), .., (last, _)]) => (*first, *last),
        Some([(only, _)]) => (*only, *only),
        _ => (range.start, range.end),
    };
    if from == to {
        return sp_bible::Range { start: first, end: last };
    }
    let end = if last.verse == sp_bible::LAST {
        VerseRef { verse: sp_bible::LAST, ..convert(VerseRef { verse: 1, ..last }, from, to) }
    } else {
        convert(last, from, to)
    };
    sp_bible::Range { start: convert(first, from, to), end }
}

/// A passage named in one numbering (`name`, its book read as the first Bible reads books), as
/// the first Bible names and numbers it, ready to look up. The name as it is when the numberings
/// agree or it can't be read (looking it up then says what is wrong).
pub fn renumbered(name: &str, from: Numbering, bibles: &[&Bible]) -> String {
    let Some(lead) = bibles.first() else { return name.to_owned() };
    if from == lead.info.numbering {
        return name.to_owned();
    }
    let Ok(range) = parse_reference_in(name, &lead.info.language) else { return name.to_owned() };
    let verses = lead.verses(renumber(range, from, lead.info.numbering, bibles));
    match verses.as_slice() {
        [(first, _), .., (last, _)] => lead.label(*first, *last),
        [(only, _)] => lead.label(*only, *only),
        [] => name.to_owned(),
    }
}

/// A passage named in one numbering (`kept`, its book read as the first Bible reads books),
/// named again in a language (`en`, `ru`, `uk`) with that language's numbering (see
/// [`numbering_of`]): for people (`read`: one psalm is "Psalm 23", see
/// `sp_bible::reading_label`) or to keep (the books' own names). So the name read back in that
/// language is the same passage. The name as it was when it can't be read.
pub fn named_in(kept: &str, from: Numbering, language: &str, bibles: &[&Bible], read: bool) -> String {
    let Some(lead) = bibles.first() else { return kept.to_owned() };
    let Ok(range) = parse_reference_in(kept, &lead.info.language) else { return kept.to_owned() };
    let there = renumber(range, from, numbering_of(language, bibles), bibles);
    if read {
        sp_bible::reading_label(language, there.start, there.end)
    } else {
        sp_bible::label_in(language, there.start, there.end)
    }
}

/// A Bible slide's name, as the first Bible names its verses ("Псалом 22:1", with the Bible's
/// short name after it in brackets when Options asks: "Псалом 22:1 (SYN)"), named again for the
/// operator in `language`, so the bar under each slide reads as the Аа button says ("Psalm 23:1
/// (SYN)"). See [`name_for_operator`].
pub fn slide_name_for_operator(name: &str, bibles: &[&Bible], language: &str) -> String {
    let (verses, short) = match name.strip_suffix(')').and_then(|name| name.rsplit_once(" (")) {
        Some((verses, short)) => (verses, Some(short)),
        None => (name, None),
    };
    let renamed = name_for_operator(verses, bibles, language);
    match short {
        Some(short) => format!("{renamed} ({short})"),
        None => renamed,
    }
}

/// A passage named as the first translation names and numbers it (as `build_passage` titles
/// its slides), named again for the operator in `language` (`en`, `ru`, `uk`) with that
/// language's numbering, as people read it, so the name reads back as the same passage. See
/// [`named_in`].
pub fn name_for_operator(kept: &str, bibles: &[&Bible], language: &str) -> String {
    let from = bibles.first().map_or(Numbering::English, |lead| lead.info.numbering);
    named_in(kept, from, language, bibles, true)
}

/// A verse found by the Bible page's Search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundVerse {
    /// The verse and its chapter as the operator reads them: named in the language of the
    /// Bible it was found in (or the language chosen), numbered as the Bible quoted numbers.
    pub reference: String,
    pub chapter: String,
    pub words: String,
    /// The Bible it was found in: its short name, and its language.
    pub bible: String,
    pub language: String,
    /// What is looked up for the verse, and for its chapter: named as the first translation
    /// names them.
    pub kept: String,
    pub kept_chapter: String,
}

/// One translation on the Bible page: its text and where its words and reference go.
pub struct RowChoice {
    pub bible: Arc<Bible>,
    pub verse: BibleTarget,
    pub reference: BibleTarget,
}

/// A digit as its small raised form.
fn raised(digit: char) -> char {
    match digit {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        other => other,
    }
}

/// Turns what was typed in Scripture Lookup into slides. The reference is read in the
/// numbering of the first translation; the others are lined up with it. Each translation's
/// words, and its reference, go into the theme box chosen for them; a box given both holds
/// the words with the reference under them. Every slide is drawn with `template`, the Bible
/// theme's template chosen for Bible slides. The error is a plain sentence.
pub fn build_passage(
    typed: &str,
    rows: &[RowChoice],
    options: &BibleOptions,
    template: Option<Uuid>,
) -> Result<Presentation, String> {
    let lead = rows.first().ok_or_else(|| t("bibles.choose_first", "Choose a Bible first.").to_owned())?;
    // A book's short form is read in the first translation's language first: "1 Цар" is
    // 1 Samuel to a Russian Bible and 1 Kings to a Ukrainian one.
    let mut range = parse_reference_in(typed, &lead.bible.info.language)?;
    // A chapter past the end of the book is read as its last chapter: "John 99" is John 21.
    let chapters = lead.bible.chapter_count(range.start.book);
    if chapters > 0 && range.start.chapter > chapters {
        range.start = VerseRef { chapter: chapters, verse: 1, ..range.start };
        range.end = VerseRef { chapter: chapters, verse: sp_bible::LAST, ..range.start };
    }
    let places: Vec<VerseRef> = lead.bible.verses(range).into_iter().map(|(place, _)| place).collect();
    let (Some(first), Some(last)) = (places.first().copied(), places.last().copied()) else {
        return Err(tf(
            "bibles.no_such_verses",
            "The {bible} has no such verses.",
            &[("bible", &lead.bible.info.name)],
        ));
    };
    let in_row = |row: &RowChoice, place: VerseRef| convert(place, lead.bible.info.numbering, row.bible.info.numbering);
    // As the Bible names it (each slide's name, read back to find the verse on screen), or as
    // people read it (the reference on the slide: "Psalm 23:1").
    let label = |row: &RowChoice, from: VerseRef, to: VerseRef, read: bool| {
        let (from, to) = (in_row(row, from), in_row(row, to));
        let text =
            if read { sp_bible::reading_label(&row.bible.info.language, from, to) } else { row.bible.label(from, to) };
        if options.display_translation { format!("{text} ({})", row.bible.info.short) } else { text }
    };

    // One slide for each verse, or one for the whole passage.
    let chunks: Vec<&[VerseRef]> =
        if options.break_on_new_verse { places.chunks(1).collect() } else { vec![places.as_slice()] };
    let count = chunks.len();
    let mut groups = Vec::with_capacity(count);
    for (index, chunk) in chunks.into_iter().enumerate() {
        let (Some(from), Some(to)) = (chunk.first().copied(), chunk.last().copied()) else { continue };
        let words_of = |row: &RowChoice| -> String {
            let mut text = String::new();
            for place in chunk {
                let there = in_row(row, *place);
                let Some(words) = row.bible.verse(there) else { continue };
                if !text.is_empty() {
                    text.push(' ');
                }
                if options.show_verse_numbers {
                    // Small raised digits, as a printed Bible has them.
                    text.extend(there.verse.to_string().chars().map(raised));
                    text.push('\u{2009}');
                }
                text.push_str(words);
            }
            text
        };
        let reference_shown = match options.reference {
            ReferenceStyle::NoReference => false,
            ReferenceStyle::PassageLast => index + 1 == count,
            ReferenceStyle::VerseReferences | ReferenceStyle::PassageEach => true,
        };
        let reference_of = |row: &RowChoice| match options.reference {
            ReferenceStyle::VerseReferences => label(row, from, to, true),
            _ => label(row, first, last, true),
        };
        // What goes in each box, in the order the boxes are first named: every translation's
        // words first, then the references.
        let mut boxes: Vec<(Role, Vec<String>, Vec<String>)> = Vec::new();
        let mut put = |role: &Role, words: Option<String>, reference: Option<String>| {
            let at = boxes.iter().position(|(have, _, _)| have == role).unwrap_or_else(|| {
                boxes.push((role.clone(), Vec::new(), Vec::new()));
                boxes.len() - 1
            });
            boxes[at].1.extend(words);
            boxes[at].2.extend(reference);
        };
        for row in rows {
            if let Some(role) = &row.verse {
                let words = words_of(row);
                if !words.is_empty() {
                    put(role, Some(words), None);
                }
            }
        }
        if reference_shown {
            for row in rows {
                if let Some(role) = &row.reference {
                    put(role, None, Some(reference_of(row)));
                }
            }
        }
        let objects: Vec<Object> = boxes
            .into_iter()
            .map(|(role, words, references)| {
                // Translations sharing a box each take a line; references sharing one sit side by side.
                let mut lines = words;
                if !references.is_empty() {
                    lines.push(references.join("  ·  "));
                }
                Object::text(role, lines.join("\n"))
            })
            .collect();
        // Each slide is a group of its own, named by the verses on it, so the bar under
        // every slide says where it is from.
        groups.push(Group {
            id: Uuid::new_v4(),
            name: label(lead, from, to, false),
            // A Bible slide's bar is white on black.
            color: Rgba::rgb(0, 0, 0),
            slides: vec![Slide { template, ..Slide::new(objects) }],
        });
    }
    Ok(Presentation {
        id: Uuid::new_v4(),
        title: lead.bible.label(first, last),
        groups,
        copyright: sp_model::Copyright::default(),
        theme: None,
        arrangements: Vec::new(),
        arrangement: None,
        loops: false,
        transition: None,
        catalog: None,
    })
}

const PANE: Color32 = Color32::from_rgb(48, 48, 48);
const DIM: Color32 = Color32::from_gray(160);
const INK: Color32 = Color32::from_gray(235);

/// Where the reference (such as "John 3:16") is shown on the slides made from a passage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReferenceStyle {
    /// Each slide names its own verses.
    #[default]
    VerseReferences,
    /// Each slide names the whole passage.
    PassageEach,
    /// Only the last slide names the passage.
    PassageLast,
    NoReference,
}

/// A Bible that can be downloaded. Every entry must be free to pass on, including in an app
/// that is sold: `docs/licences/bibles.md` says where each one's terms were read and what they
/// ask. Do not add a Bible here without adding it there.
pub struct Offered {
    pub language: &'static str,
    pub name: &'static str,
    pub short: &'static str,
    /// Its short name for files: the download is `<id>.pcbible` (the ending it was published with).
    pub id: &'static str,
    /// The terms it is offered under, in a few words.
    pub terms: &'static str,
    /// The SHA-256 of its file. A download with any other checksum is not installed.
    pub checksum: &'static str,
}

/// The Bibles offered under "Free", grouped by language, in the order of the languages' own
/// names as the Bibles window shows them ([`language_name`]): those in Latin letters first (an
/// accented letter read as the plain one), then the other alphabets. The window and `sp bibles`
/// list the person's languages before the rest ([`offered_in_order`]). Each language is named in
/// English, as [`LANGUAGES`] names it. The four that come with the app are not here.
pub const OFFERED: &[Offered] = &[
    Offered {
        language: "Czech",
        name: "Bible kralická (1613)",
        short: "BKR",
        id: "bkr",
        terms: "Public domain",
        checksum: "afe50769955a809d4d02c97c72bee56887dfdae0177bd535be1ebfd33dfa8dbc",
    },
    Offered {
        language: "Danish",
        name: "Bibelen (GT 1871, NT 1907)",
        short: "DA1871",
        id: "da1871",
        terms: "Public domain",
        checksum: "33b548be8e7ea566898eb61b3c693bef945db2ccd50ebfb7ede0a524989fd1d1",
    },
    Offered {
        language: "German",
        name: "Luther 1912",
        short: "LUT1912",
        id: "lut1912",
        terms: "Public domain",
        checksum: "d9e65a85d606686f06bf2131ebd6f00a1113c783a16b3753edf59d81183ed3b5",
    },
    Offered {
        language: "German",
        name: "Elberfelder 1905",
        short: "ELB1905",
        id: "elb1905",
        terms: "Public domain",
        checksum: "b208bc964c3889f2dc7e08fb9395e8bfdd2d276a89fe49dfed0045909752f8b2",
    },
    Offered {
        language: "German",
        name: "Albrecht, Neues Testament und Psalmen (1926)",
        short: "ALBRECHT",
        id: "albrecht",
        terms: "Public domain",
        checksum: "3f6719e660bd142cf4fe7e32d0e0dfe34b79f59a590a4dcd142deb9aeca90177",
    },
    Offered {
        language: "German",
        name: "Tafelbibel (1911, ohne die Hagiographen)",
        short: "TAFEL",
        id: "tafel",
        terms: "Public domain",
        checksum: "cbfb59880d6d904244302ef22c60554fd14b5a6db7020dc930d1da657336ab93",
    },
    Offered {
        language: "German",
        name: "Textbibel (1906)",
        short: "TEXTBIBEL",
        id: "textbibel",
        terms: "Public domain",
        checksum: "d5db4510d6e877f9bc4e3bf8defda0b52b12b417b1a6d3a7dbc10f239c05f011",
    },
    Offered {
        language: "English",
        name: "American Standard Version",
        short: "ASV",
        id: "asv",
        terms: "Public domain",
        checksum: "2c22ff4c11a61c54290c6ed2707b0db42bfd47d45cafbb53dc99c86fb09d1145",
    },
    Offered {
        language: "English",
        name: "Young's Literal Translation",
        short: "YLT",
        id: "ylt",
        terms: "Public domain",
        checksum: "e4fa412454a02a4d9e8b869db7f930f2bfef77fab00c06bc155fc5935cd817d5",
    },
    Offered {
        language: "English",
        name: "Darby Translation",
        short: "DBY",
        id: "dby",
        terms: "Public domain",
        checksum: "ef27866208f7b101c3aea860ed0fea9d083e982bca8c54fd7f228a61026af1f3",
    },
    Offered {
        language: "English",
        name: "Geneva Bible 1599",
        short: "GNV",
        id: "gnv",
        terms: "Public domain",
        checksum: "f87c06d1934e1a075c09246fd604ef22d31be162567c215bdfc28043ac02eb1a",
    },
    Offered {
        language: "English",
        name: "Literal Standard Version",
        short: "LSV",
        id: "lsv",
        terms: "CC BY-SA 4.0",
        checksum: "71ae53243fa24330c33080cd402d0f7c4ee0e89bf747ff7f5bc9d2299c23a7d5",
    },
    Offered {
        language: "English",
        name: "Majority Standard Bible",
        short: "MSB",
        id: "msb",
        terms: "Public domain",
        checksum: "239725b82c9aaf335c7cadb92e9a0d264cdeb2205fa5ea262e65e19ac3055bf0",
    },
    Offered {
        language: "English",
        name: "World English Bible British Edition",
        short: "WEBBE",
        id: "webbe",
        terms: "Public domain",
        checksum: "ee24c5c156c17e16a97f78f724fecd76f74eaa3bed352247061c67dc05c50ce7",
    },
    Offered {
        language: "English",
        name: "Free Bible Version",
        short: "FBV",
        id: "fbv",
        terms: "CC BY-SA 4.0",
        checksum: "1fae7a9a0b673e829cf8ab4c1f6c9058d800916ea858ff1cd1f5e40cbcb185a8",
    },
    Offered {
        language: "English",
        name: "Unlocked Literal Bible",
        short: "ULB",
        id: "ulb",
        terms: "CC BY-SA 4.0",
        checksum: "b9f04939988ccc1b633b855e83cdf51529032debf0e49f3ca0f574888867532e",
    },
    Offered {
        language: "English",
        name: "Revised Version (1885)",
        short: "RV",
        id: "rv",
        terms: "Public domain",
        checksum: "78c2be1e133ab03f194109768f380ce38eebf3457a818da1ca4f12ab129223f6",
    },
    Offered {
        language: "English",
        name: "Noah Webster Bible (1833)",
        short: "WBT",
        id: "wbt",
        terms: "Public domain",
        checksum: "b85ec8e694efadb40acf85980badbdc162bf9c1338351fb48293604fa614450c",
    },
    Offered {
        language: "English",
        name: "JPS Tanakh (1917)",
        short: "JPS",
        id: "jps1917",
        terms: "Public domain",
        checksum: "c443fe676fb5dcb08f6f2ef2b467c9b9321b784c23cc9a642e514e8b1bdd0f57",
    },
    Offered {
        language: "English",
        name: "Isaac Leeser Tanakh (1853)",
        short: "LEESER",
        id: "leeser",
        terms: "Public domain",
        checksum: "357612b15eec8d699db4bfb1b469cb6ae8881f7f05f09e77f7e06b918bd4a870",
    },
    Offered {
        language: "English",
        name: "Noyes Translation (1869, New Testament and Job to Malachi)",
        short: "NOYES",
        id: "noyes",
        terms: "Public domain",
        checksum: "9bc0e719e919191a31a71502b8d40f3bf52ac61893ed583da470460dab6745b3",
    },
    Offered {
        language: "English",
        name: "Open English Bible (New Testament and 17 Old Testament books)",
        short: "OEB",
        id: "oeb",
        terms: "Public domain",
        checksum: "4872236e2fe1f47906368fa1964936aa509afad05c67f1faeaf9c0acffd2b605",
    },
    Offered {
        language: "English",
        name: "Tyndale New Testament (1534)",
        short: "TYNDALE",
        id: "tyndale",
        terms: "Public domain",
        checksum: "096176439c4b532c365857e5019fda212579cb1c39ca37d217981fc1da59d291",
    },
    Offered {
        language: "English",
        name: "World Messianic Bible",
        short: "WMB",
        id: "wmb",
        terms: "Public domain",
        checksum: "235a4c924dd066f143603fbb69edbeb842e61595bdbc5afff4abf029a64e126a",
    },
    Offered {
        language: "English",
        name: "King James Version, Pure Cambridge Edition",
        short: "PCE",
        id: "kjvpce",
        terms: "Public domain",
        checksum: "8691b32eb291d1d57741ed5de0eeb53379221ea36766bd4be1bfed6951eccbda",
    },
    Offered {
        language: "English",
        name: "Updated King James Version",
        short: "UKJV",
        id: "ukjv",
        terms: "Public domain",
        checksum: "876549ee3c8394c89f97be4ead3048131758970862550c9a88d5c7cedfae8211",
    },
    Offered {
        language: "English",
        name: "New Heart English Bible",
        short: "NHEB",
        id: "nheb",
        terms: "Public domain",
        checksum: "d7baec27dcac9c7fc7a8d01d0f431d763b3e1f9f44c49b37bf1dcb15139e2db3",
    },
    Offered {
        language: "English",
        name: "A Conservative Version",
        short: "ACV",
        id: "acv",
        terms: "Public domain",
        checksum: "8904613c7e4ede4a4aef00453a1db1b27df71cda99e43b44af69291ff692a31d",
    },
    Offered {
        language: "English",
        name: "Rotherham's Emphasised Bible (1902)",
        short: "EBR",
        id: "rotherham",
        terms: "Public domain",
        checksum: "f0f12114c043c724b3dac956266796083d52b382fa8bc224fd5e66d9dade3a1f",
    },
    Offered {
        language: "English",
        name: "Weymouth New Testament (1912)",
        short: "WNT",
        id: "weymouth",
        terms: "Public domain",
        checksum: "8335bdf2048ac99832ba6b144494834efedfe1e8e3e5464d09631daf656addeb",
    },
    Offered {
        language: "English",
        name: "Montgomery New Testament (1924)",
        short: "MONT",
        id: "montgomery",
        terms: "Public domain",
        checksum: "65d527af4dd25648a539a5619b30bc12b9486c38e09d180315ea1b224678fd4a",
    },
    Offered {
        language: "English",
        name: "Twentieth Century New Testament (1904)",
        short: "TCNT",
        id: "tcnt",
        terms: "Public domain",
        checksum: "7069711bd8ca39688142c2ae064d5e015a4c87c33ce1c98690f3e64046ef06fb",
    },
    Offered {
        language: "English",
        name: "Godbey New Testament (1902)",
        short: "GODBEY",
        id: "godbey",
        terms: "Public domain",
        checksum: "47810088151c1262cc51bcda7060692b887b94acd7f48d26c4b7a813a0bf6791",
    },
    Offered {
        language: "English",
        name: "The Living Oracles (1826)",
        short: "LO",
        id: "lo",
        terms: "Public domain",
        checksum: "9d47cfcddcc011340addeff40c2333222c3cbb747b6f21f2a692615409e06e43",
    },
    Offered {
        language: "English",
        name: "Etheridge Peshitta New Testament (1849)",
        short: "ETH",
        id: "etheridge",
        terms: "Public domain",
        checksum: "319ff0a9e62813197e85f7c7c9823bcbad971b082637381050888a10b762b213",
    },
    Offered {
        language: "Spanish",
        name: "Reina-Valera 1909",
        short: "RV1909",
        id: "rv1909",
        terms: "Public domain",
        checksum: "2e48a2e412be0471cd1dacbd4bc9c4de213c7cb00ad9d5b9080c06ed2f389fd7",
    },
    Offered {
        language: "Spanish",
        name: "Versión Biblia Libre",
        short: "VBL",
        id: "vbl",
        terms: "CC BY-SA 4.0",
        checksum: "07c50f1e213747cac53b32a515a79fe9f3f774dfbebb0cd20b172811b032119d",
    },
    Offered {
        language: "Spanish",
        name: "Nuevo Testamento Reina-Valera (1858)",
        short: "RV1858",
        id: "rv1858",
        terms: "Public domain",
        checksum: "2d0892f82fde0238faa1d95873bfb7a0c9f80098989c4c4fa0b9e8552a10ba3e",
    },
    Offered {
        language: "Esperanto",
        name: "Londona Biblio (1926)",
        short: "LB",
        id: "londona",
        terms: "Public domain",
        checksum: "352135040a70f0ba139e9c3f26edb76dd8baae1ab978acd6801217b7d63118cd",
    },
    Offered {
        language: "Basque",
        name: "Testamentu Berria (Leiçarraga, 1571)",
        short: "LEIZ",
        id: "leizarraga",
        terms: "Public domain",
        checksum: "5406435de056862a0e70df33c7dbf85825b463b30ab3d22b49b8c2361519e806",
    },
    Offered {
        language: "French",
        name: "Louis Segond 1910",
        short: "LSG",
        id: "lsg",
        terms: "Public domain",
        checksum: "7f044422526909a0611618fe55663db4079964f1db0dc0cdc304b244db4682ee",
    },
    Offered {
        language: "French",
        name: "Bible Darby",
        short: "FRDBY",
        id: "frdby",
        terms: "Public domain",
        checksum: "5d179898daa5e6ed02009eb7909d62fe7273eb55cb13d519fd1478c7a4d40eb2",
    },
    Offered {
        language: "French",
        name: "Bible annotée (1900)",
        short: "BA",
        id: "ba1900",
        terms: "Public domain",
        checksum: "e91f2a6f4aa984eed13420c8fbdc40eaf348313e0966724d63570b598b7e214d",
    },
    Offered {
        language: "French",
        name: "Bible David Martin (1744)",
        short: "MAR",
        id: "martin",
        terms: "Public domain",
        checksum: "70add562b6136e114f783b066f6822131744f79866511f862009f9fab559b8f5",
    },
    Offered {
        language: "French",
        name: "Bible Perret-Gentil et Rilliet",
        short: "PGR",
        id: "pgr",
        terms: "Public domain",
        checksum: "9a500192eab289665376fad060772a0f7eaac19150b4b7b41902de2c662b389c",
    },
    Offered {
        language: "Italian",
        name: "Diodati (1649)",
        short: "DIO",
        id: "diodati",
        terms: "Public domain",
        checksum: "27f2fb32f4ef6cc011b21ec5098c634954efe3f914e8f6ccdbe891f60b421e15",
    },
    Offered {
        language: "Italian",
        name: "Riveduta (1927)",
        short: "RIV",
        id: "riveduta",
        terms: "Public domain",
        checksum: "3b8eee14565c34d6184766e831d497511fd379434eafae5a02445277148d5c60",
    },
    Offered {
        language: "Hungarian",
        name: "Károli Biblia (1908)",
        short: "KAR",
        id: "karoli",
        terms: "Public domain",
        checksum: "e09304f2badd1b688cb2a61493683b92df36526640b420d9b3aac5ece2702e53",
    },
    Offered {
        language: "Malagasy",
        name: "Baiboly Malagasy (1865)",
        short: "MG1865",
        id: "mg1865",
        terms: "Public domain",
        checksum: "001ef39bfd2e0421a2c0cbf198bf363338a98685dcb18d60576cd839440c33ca",
    },
    Offered {
        language: "Dutch",
        name: "Statenvertaling",
        short: "SV",
        id: "sv",
        terms: "Public domain",
        checksum: "c408fecbb797cb578cc913218e8fefde18a8b47cc1e31a0cfa7e472be5c907b3",
    },
    Offered {
        language: "Norwegian",
        name: "Studentmållagsbibelen (1921)",
        short: "SMB",
        id: "smb",
        terms: "Public domain",
        checksum: "2e9ac913c1799725bf6443b344019bbbf36518860049dbb334485194f74cb888",
    },
    Offered {
        language: "Polish",
        name: "Biblia Gdańska (1881)",
        short: "BG",
        id: "gdanska",
        terms: "Public domain",
        checksum: "e2844a862da6b642047d2767dffacbaf94e3467f1ccfcdb0fa1d712f386ca9cc",
    },
    Offered {
        language: "Portuguese",
        name: "Bíblia Livre",
        short: "BLIVRE",
        id: "blivre",
        terms: "CC BY 4.0",
        checksum: "25d1b645b9a82bd7c8cd21f3999987b35072744056a8bdb76d627889bee4f1f4",
    },
    Offered {
        language: "Serbian",
        name: "Sveto pismo (Daničić-Karadžić)",
        short: "DK",
        id: "dk",
        terms: "Public domain",
        checksum: "0e1e76838ed9999806432ea1344e3f4898cccdcf50669500538d6c34ea460efe",
    },
    Offered {
        language: "Serbian",
        name: "Свето писмо (Даничић-Караџић)",
        short: "ДК",
        id: "dkc",
        terms: "Public domain",
        checksum: "adf075e2daf43d240f898737524b1b6d00e1e5ebcb3bf5c446fa266a3df9138f",
    },
    Offered {
        language: "Finnish",
        name: "Biblia (1776)",
        short: "FI1776",
        id: "fi1776",
        terms: "Public domain",
        checksum: "a1ef7bdb79fb27ff5a0b0f34c3be9fbfc3f7c4ebaded13029547f3a4b6ce880d",
    },
    Offered {
        language: "Swedish",
        name: "Bibeln (1917)",
        short: "SV1917",
        id: "sv1917",
        terms: "Public domain",
        checksum: "380d457f4afb125dccac3515f84a879586af057fdb3d67d44c2cf94ba6b8ac7d",
    },
    Offered {
        language: "Swedish",
        name: "Karl XII:s bibel (1873)",
        short: "KXII",
        id: "karl12",
        terms: "Public domain",
        checksum: "17bf60e385a4ac8903f45084fed8bdff6320ab5c33d1d20af8fdff449c287788",
    },
    Offered {
        language: "Tagalog",
        name: "Ang Dating Biblia (1905)",
        short: "ADB",
        id: "adb1905",
        terms: "Public domain",
        checksum: "6288b8e02ace59d16ebad5fc3cc7e16ea3c61d3bb64de2e14b9e9ae85584e10e",
    },
    Offered {
        language: "Klingon",
        name: "Klingon Language Version",
        short: "KLV",
        id: "klv",
        terms: "Public domain",
        checksum: "dd66a494e6781bea9223d016bfe33a03273a692681317c7eddb38cb85f137c2b",
    },
    Offered {
        language: "Greek",
        name: "SBL Greek New Testament",
        short: "SBLGNT",
        id: "sblgnt",
        terms: "CC BY 4.0",
        checksum: "5874411a89d59706a1bf7d8d34616ddd91d86aadc7dffbd911da1d8769da6c7f",
    },
    Offered {
        language: "Greek",
        name: "Byzantine Text, Robinson-Pierpont",
        short: "RP",
        id: "rp",
        terms: "Public domain",
        checksum: "57876e9b5662d7095cc6cdf60412c87047e39e6943c92bfaeeb2a0f72b2979df",
    },
    Offered {
        language: "Greek",
        name: "Αγία Γραφή (Βάμβας, 1850)",
        short: "VAMVAS",
        id: "vamvas",
        terms: "Public domain",
        checksum: "30c477295f2831acdc30fb19e5ab120a71b56523f3e61a0314cf5ee81b624780",
    },
    Offered {
        language: "Greek",
        name: "Textus Receptus",
        short: "TR",
        id: "tr",
        terms: "Public domain",
        checksum: "6b5d80e15912436e4e0e9399c7f0994c7874c5a56587d38ce012d4477015377a",
    },
    Offered {
        language: "Greek",
        name: "Tischendorf 8th Edition",
        short: "TISCH",
        id: "tisch",
        terms: "Public domain",
        checksum: "596353d579cc1d4212951447da8cd9aaf8634a55b07c3a42e5d031d603b7c66a",
    },
    Offered {
        language: "Russian",
        name: "Russian Open Bible",
        short: "ROB",
        id: "rob",
        terms: "CC BY-SA 4.0",
        checksum: "b80b4a6a0ff066e3e5b2ba26a1efb7c1da18f3b128a84b4f274739d5c88d23b4",
    },
    Offered {
        language: "Ukrainian",
        name: "Переклад Куліша та Пулюя",
        short: "UKR1905",
        id: "ukr1905",
        terms: "Public domain",
        checksum: "2f7f15cdc4bcdd1bde456487d202258846c6259aafa0feeb83f5038bef3d63a3",
    },
    Offered {
        language: "Ukrainian",
        name: "Новий переклад українською (Новий Завіт і Псалми)",
        short: "НПУ",
        id: "ukronpu",
        terms: "CC BY-SA 4.0",
        checksum: "5c34d4bfce41efa33e85bf003160d1c4b439a275083a0c5de5a8f14040dc3265",
    },
    Offered {
        language: "Ukrainian",
        name: "Новий Завіт, переклад П. Куліша (1871)",
        short: "КУЛІШ1871",
        id: "kulish1871",
        terms: "Public domain",
        checksum: "264ffd8e70a83985d3fddf7c4d08f8a233fcfad671ee46f8efb4fda49ac4764b",
    },
    Offered {
        language: "Armenian",
        name: "Նոր Կտակարան (1853)",
        short: "ARMW",
        id: "armw",
        terms: "Public domain",
        checksum: "5cc5d9c55d205506c4f1d85e94e5f3ecfee913db9809d566ce1e4893df9ff9fe",
    },
    Offered {
        language: "Hebrew",
        name: "Westminster Leningrad Codex",
        short: "WLC",
        id: "wlc",
        terms: "Public domain",
        checksum: "376b60ec5e9db4c4afeb5fd5b542c53cea2973e38e879f368a6b09011cbb43d7",
    },
    Offered {
        language: "Hebrew",
        name: "כתבי הקודש עם הברית החדשה של דליטש",
        short: "DELITZSCH",
        id: "delitzsch",
        terms: "Public domain",
        checksum: "2ebcb0c0dd746d3c1f270cbbe9dfdfb2d6730e7f6f133a6a11340e7c2e8db325",
    },
    Offered {
        language: "Arabic",
        name: "الكتاب المقدس، ترجمة فاندايك",
        short: "AVD",
        id: "avd",
        terms: "Public domain",
        checksum: "c3d421da185e9145ec7877382f092051cb991f33f7c09d05b31f9750614151df",
    },
    Offered {
        language: "Persian",
        name: "ترجمه قدیم",
        short: "OPV",
        id: "opv",
        terms: "Public domain",
        checksum: "b25e64340ad8470e1ff2d97ef230ba8c6f54297e1ee78b43a7e59bd8a9a89fc9",
    },
    Offered {
        language: "Chinese",
        name: "和合本（繁體）",
        short: "CUVT",
        id: "cuvt",
        terms: "Public domain",
        checksum: "bb4f15cce31d511cdf461033c1c6acc0dad4925bc5842b0585f2431fc84d6792",
    },
    Offered {
        language: "Chinese",
        name: "和合本（简体）",
        short: "CUVS",
        id: "cuvs",
        terms: "Public domain",
        checksum: "cca56953914bc415a4be03099e1b1826299e53253c54a75ac47e21290ae9c308",
    },
    Offered {
        language: "Korean",
        name: "개역한글 (1961)",
        short: "개역한글",
        id: "krv",
        terms: "Public domain",
        checksum: "ae035814149433b83429d0130d9c2cf08db2b92ee9864f76d7763ffe4e922316",
    },
];

/// The Bible offered under that short name (ASV, НПУ) or short name for files (asv, ukronpu),
/// in any case, in any alphabet: what `bible install NAME` and the policies of a remote page and
/// an assistant all take as naming one.
pub fn offered_named(typed: &str) -> Option<&'static Offered> {
    let typed = typed.to_lowercase();
    OFFERED.iter().find(|bible| bible.short.to_lowercase() == typed || bible.id.to_lowercase() == typed)
}

#[derive(Debug)]
pub struct BibleOptions {
    pub open: bool,
    /// The Bibles window, where Bibles are installed and removed, and what is typed in its
    /// search box.
    pub manage_open: bool,
    pub manage_search: String,
    pub show_verse_numbers: bool,
    pub break_on_new_verse: bool,
    pub display_translation: bool,
    pub reference: ReferenceStyle,
    /// What went wrong with the last download, for the operator.
    pub problem: Option<String>,
    /// True when one of the pop-up's own menus was open on the last frame, so the click that
    /// closed that menu does not also close the pop-up.
    pub menu_was_open: bool,
    /// A downloaded Bible asked to be removed, by its place on the shelf.
    pub remove: Option<usize>,
    /// Install from File… was clicked: the window for choosing a file is to open.
    pub file_request: bool,
    /// Bible files read that don't say what they are, as each is read: the sheet asks
    /// about the first, then the next (several dropped together are each asked about).
    pub details: std::collections::VecDeque<DetailsSheet>,
    /// Bibles the sheet's Install asked for, with the details given, to be installed on a
    /// thread of their own.
    pub installs: Vec<(sp_bible::import::Imported, sp_bible::import::About)>,
}

/// The sheet that asks for a Bible file's name, short name and language when the file doesn't
/// say them, each filled in with what the file says or else a guess, before it is installed.
#[derive(Debug)]
pub struct DetailsSheet {
    pub imported: sp_bible::import::Imported,
    pub about: sp_bible::import::About,
    /// True until the first empty field has been given the keyboard.
    pub focus: bool,
}

impl DetailsSheet {
    pub fn new(imported: sp_bible::import::Imported) -> DetailsSheet {
        let about = imported.prefilled();
        DetailsSheet { imported, about, focus: true }
    }
}

/// The sheet asking what a Bible file is (see [`DetailsSheet`]): its name, the short name
/// shown on slides and its language, then Install. `Some(true)` when Install is clicked (or
/// Return pressed) with all three given, `Some(false)` for Cancel or Escape. `waiting` is how
/// many more files wait to be asked about after this one.
pub fn details_sheet(ctx: &egui::Context, sheet: &mut DetailsSheet, waiting: usize) -> Option<bool> {
    let mut decided = None;
    // A modal, as the Bibles window is, so it comes over that window when opened from it.
    let frame = Frame::new()
        .fill(Color32::from_rgb(38, 38, 40))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(14.0)
        .inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 16 })
        .shadow(egui::Shadow { offset: [0, 16], blur: 44, spread: 0, color: Color32::from_black_alpha(150) });
    let id = egui::Id::unique("bible details");
    let area = egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, vec2(0.0, -30.0));
    egui::Modal::new(id).area(area).backdrop_color(Color32::from_black_alpha(90)).frame(frame).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.add_space(4.0);
        let title = match waiting {
            0 => t("bibles.details.title", "Install a Bible").to_owned(),
            1 => t("bibles.details.title.one_more", "Install a Bible (one more after this)").to_owned(),
            more => tf(
                "bibles.details.title.more",
                "Install a Bible ({more} more after this)",
                &[("more", &crate::i18n::count(more as u64))],
            ),
        };
        ui.label(egui::RichText::new(title).font(crate::studio::semibold(16.0)));
        ui.add_space(4.0);
        let imported = &sheet.imported;
        let books = tn("bibles.details.books", imported.book_count() as u64, "{n} book", "{n} books", &[]);
        let verses = tn("bibles.details.verses", imported.verse_count() as u64, "{n} verse", "{n} verses", &[]);
        ui.add(
            egui::Label::new(
                egui::RichText::new(tf(
                    "bibles.details.explain",
                    "{file} doesn't say all of this. Check the name, the short name shown on slides and the language, then Install.\n{format}: {books}, {verses}.",
                    &[
                        ("file", &imported.file_name),
                        ("format", imported.format.name()),
                        ("books", &books),
                        ("verses", &verses),
                    ],
                ))
                .size(11.5)
                .color(DIM),
            )
            .wrap(),
        );
        ui.add_space(8.0);
        let first_empty = [&sheet.about.name, &sheet.about.short, &sheet.about.language]
            .iter()
            .position(|field| field.trim().is_empty())
            .unwrap_or(0);
        let mut enter = false;
        egui::Grid::new("bible details").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
            let fields: [(&str, &str, &mut String); 3] = [
                (t("bibles.details.name", "Name"), t("bibles.details.name.hint", "The Bible's full name"), &mut sheet.about.name),
                (
                    t("bibles.details.short_name", "Short name"),
                    t("bibles.details.short_name.hint", "Shown on slides, such as KJV"),
                    &mut sheet.about.short,
                ),
                (
                    t("bibles.details.language", "Language"),
                    t("bibles.details.language.hint", "en, ru, uk…"),
                    &mut sheet.about.language,
                ),
            ];
            for (at, (label, hint, value)) in fields.into_iter().enumerate() {
                ui.label(egui::RichText::new(label).color(DIM));
                let width = if at == 0 { 320.0 } else { 140.0 };
                let field = ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(width));
                if sheet.focus && at == first_empty {
                    field.request_focus();
                }
                if field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    enter = true;
                }
                ui.end_row();
            }
        });
        sheet.focus = false;
        let language = sp_bible::import::language_code(&sheet.about.language);
        if !language.is_empty() {
            ui.label(egui::RichText::new(language_name(&language)).size(11.0).color(DIM));
        }
        ui.add_space(10.0);
        let ready = sheet.imported.details(&sheet.about).is_some();
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let size = vec2(96.0, 30.0);
                let emphasis = if ready { crate::studio::Emphasis::Plain } else { crate::studio::Emphasis::Idle };
                if crate::studio::block_button(ui, size, t("bibles.install", "Install"), "", emphasis).clicked() && ready {
                    decided = Some(true);
                }
                if crate::studio::block_button(ui, size, t("bibles.details.cancel", "Cancel"), "", crate::studio::Emphasis::Plain).clicked() {
                    decided = Some(false);
                }
            });
        });
        if enter && ready {
            decided = Some(true);
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            decided = Some(false);
        }
    });
    decided
}

impl Default for BibleOptions {
    fn default() -> BibleOptions {
        BibleOptions {
            open: false,
            manage_open: false,
            manage_search: String::new(),
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference: ReferenceStyle::VerseReferences,
            problem: None,
            menu_was_open: false,
            remove: None,
            file_request: false,
            details: std::collections::VecDeque::new(),
            installs: Vec::new(),
        }
    }
}

/// The themes to choose from for Bible slides, and the choice now made.
pub struct ThemeChoice<'a> {
    /// Each theme as the window calls it ([`crate::home::HomeState::themes_shown`]).
    pub names: &'a [String],
    /// Each theme's templates, by id and the name it gives them.
    pub template_names: &'a [Vec<(sp_model::Uuid, String)>],
    /// Each theme's two layouts, as boxes, for the pictures.
    pub layouts: &'a [Vec<Vec<(Role, sp_model::SlideRect)>>],
    pub theme: usize,
    pub layout: usize,
}

impl ThemeChoice<'_> {
    /// What one of a theme's templates is called in the window.
    fn template_name(&self, theme: usize, template: usize) -> std::borrow::Cow<'_, str> {
        self.template_names
            .get(theme)
            .and_then(|names| names.get(template))
            .map_or(std::borrow::Cow::Borrowed(""), |(id, name)| crate::theme_store::template_name(*id, name))
    }
}

/// What the Options panel previews: the first slide of the passage showing (or of a sample of
/// two verses when none is), its last slide when it has more than one, and how many it has.
/// A picture not drawn yet is `None`.
pub struct SlidesPreview {
    pub first: Option<egui::TextureId>,
    pub last: Option<egui::TextureId>,
    pub count: usize,
    /// The passage's name, and whether it is the sample.
    pub name: String,
    pub sample: bool,
    /// The short names of the translations, for "Translation name (KJV, SYN)".
    pub translations: String,
}

/// The Options panel under the Options button, its right edge in line with the button's:
/// plain-worded choices on the left, and on the right a preview of the slides they make,
/// which changes as they are clicked. Returns a theme and layout picked for Bible slides.
pub fn options_popup(
    ctx: &egui::Context,
    options: &mut BibleOptions,
    button: Rect,
    themes: &ThemeChoice,
    preview: &SlidesPreview,
) -> Option<(usize, usize)> {
    let mut picked_theme = None;
    let size = vec2(640.0, 330.0);
    // Kept inside the window: in a narrow one it moves left rather than off the edge.
    let left = (button.max.x - size.x + 8.0).max(8.0);
    let corner = pos2(left, button.max.y + 8.0);
    let area = egui::Area::new(egui::Id::unique("bible options"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .constrain(true)
        .fixed_pos(corner)
        .show(ctx, |ui| {
            Frame::new()
                .fill(PANE)
                .corner_radius(12)
                .stroke(Stroke::new(1.0, Color32::from_gray(78)))
                .inner_margin(egui::Margin::symmetric(18, 14))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_black_alpha(120),
                })
                .show(ui, |ui| {
                    ui.set_width(size.x - 36.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(vec2(320.0, size.y), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(320.0);
                            picked_theme = choices(ui, options, themes, &preview.translations);
                        });
                        ui.add_space(18.0);
                        ui.vertical(|ui| slides_preview(ui, preview));
                    });
                });
        });
    // A click anywhere outside the pop-up and its button closes it, as any pop-over does. A
    // click in one of its own menus, which open outside it, does not.
    let outside = ctx.input(|input| {
        input.pointer.any_click()
            && input.pointer.interact_pos().is_some_and(|at| !area.response.rect.contains(at) && !button.contains(at))
    });
    if outside && !egui::Popup::is_any_open(ctx) && !std::mem::take(&mut options.menu_was_open) {
        options.open = false;
    }
    options.menu_was_open = egui::Popup::is_any_open(ctx);
    picked_theme
}

/// What a reference style is called in the Options panel, and what it does.
fn reference_words(style: ReferenceStyle) -> (&'static str, &'static str) {
    match style {
        ReferenceStyle::VerseReferences => (
            t("bibles.reference.every", "On every slide"),
            t("bibles.reference.every.hint", "Each slide names its own verses: John 3:16, then John 3:17"),
        ),
        ReferenceStyle::PassageEach => (
            t("bibles.reference.passage_each", "Whole passage, every slide"),
            t("bibles.reference.passage_each.hint", "Each slide names the whole passage: John 3:16-18"),
        ),
        ReferenceStyle::PassageLast => (
            t("bibles.reference.passage_last", "Whole passage, last slide"),
            t("bibles.reference.passage_last.hint", "Only the last slide names the passage"),
        ),
        ReferenceStyle::NoReference => {
            (t("bibles.reference.none", "None"), t("bibles.reference.none.hint", "No reference on the slides"))
        }
    }
}

/// The Options panel's choices.
fn choices(
    ui: &mut egui::Ui,
    options: &mut BibleOptions,
    themes: &ThemeChoice,
    translations: &str,
) -> Option<(usize, usize)> {
    let mut picked_theme = None;
    let heading = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).size(11.0).strong().color(Color32::from_gray(140)));
    };
    ui.spacing_mut().item_spacing.y = 6.0;
    // Boxes and circles that can be seen when empty: the usual ones all but vanish on the
    // panel's grey.
    let widgets = &mut ui.style_mut().visuals.widgets;
    widgets.inactive.bg_fill = Color32::from_gray(64);
    widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(128));
    widgets.hovered.bg_fill = Color32::from_gray(84);
    heading(ui, t("bibles.options.each_slide", "EACH SLIDE SHOWS"));
    ui.radio_value(&mut options.break_on_new_verse, true, t("bibles.options.one_verse", "One verse"));
    ui.radio_value(&mut options.break_on_new_verse, false, t("bibles.options.whole", "The whole passage"))
        .hint(t("bibles.options.whole.hint", "All its verses on one slide"));
    ui.add_space(6.0);
    ui.checkbox(&mut options.show_verse_numbers, t("bibles.options.numbers", "Verse numbers"))
        .hint(t("bibles.options.numbers.hint", "Put each verse's number before its words"));
    let named = if translations.is_empty() {
        t("bibles.options.translation", "Translation name").to_owned()
    } else {
        tf("bibles.options.translation.named", "Translation name ({translations})", &[("translations", translations)])
    };
    ui.checkbox(&mut options.display_translation, named)
        .hint(t("bibles.options.translation.hint", "Put the Bible's short name after the reference"));
    ui.add_space(6.0);
    egui::Grid::new("bible slide options").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
        ui.label(t("bibles.options.reference", "Reference"));
        let (chosen, _) = reference_words(options.reference);
        egui::ComboBox::from_id_salt("bible reference").selected_text(chosen).width(200.0).show_ui(ui, |ui| {
            for style in [
                ReferenceStyle::VerseReferences,
                ReferenceStyle::PassageEach,
                ReferenceStyle::PassageLast,
                ReferenceStyle::NoReference,
            ] {
                let (name, hint) = reference_words(style);
                ui.selectable_value(&mut options.reference, style, name).hint(hint);
            }
        });
        ui.end_row();
        ui.label(t("bibles.options.template", "Template"));
        // Which template of the main theme Bible slides use, each shown as a small picture of
        // where its boxes sit.
        let chosen = themes.template_name(themes.theme, themes.layout).into_owned();
        egui::ComboBox::from_id_salt("bible theme")
            .selected_text(chosen)
            .width(200.0)
            .height(320.0)
            .show_ui(ui, |ui| {
                for (index, name) in themes.names.iter().enumerate().filter(|(index, _)| *index == themes.theme) {
                    ui.label(
                        egui::RichText::new(tf("bibles.options.main_theme", "{name} (the main theme)", &[("name", name)]))
                            .font(crate::studio::semibold(12.0))
                            .color(DIM),
                    );
                    for layout in 0..themes.layouts.get(index).map_or(0, Vec::len) {
                        let (row, response) = ui.allocate_exact_size(vec2(210.0, 40.0), Sense::click());
                        let on = index == themes.theme && layout == themes.layout;
                        if on {
                            ui.painter().rect_filled(row, 6.0, crate::studio::ACCENT.gamma_multiply(0.3));
                        } else if response.hovered() {
                            ui.painter().rect_filled(row, 6.0, crate::studio::HOVER);
                        }
                        let picture = Rect::from_min_size(row.min + vec2(6.0, 4.0), vec2(57.0, 32.0));
                        let boxes = themes.layouts.get(index).map_or(&[][..], |layouts| layouts[layout].as_slice());
                        crate::home::box_diagram(ui.painter(), picture, boxes, None);
                        ui.painter().text(
                            pos2(picture.max.x + 10.0, row.center().y),
                            Align2::LEFT_CENTER,
                            themes.template_name(index, layout),
                            FontId::proportional(13.0),
                            INK,
                        );
                        if response.clicked() {
                            picked_theme = Some((index, layout));
                            ui.close();
                        }
                    }
                    ui.add_space(4.0);
                }
            })
            .response
            .hint(t(
                "bibles.options.template.hint",
                "The template of the main theme Bible slides use. Its boxes are what the Verse and Reference menus offer. In the Themes view, right-click a template for this too.",
            ));
        ui.end_row();
    });
    ui.add_space(10.0);
    if ui
        .button(t("bibles.options.manage", "Manage Bibles…"))
        .hint(t("bibles.options.manage.hint", "Install and remove Bibles"))
        .clicked()
    {
        options.manage_open = true;
        options.open = false;
    }
    picked_theme
}

/// The right of the Options panel: the first slide, and the last when there are more.
fn slides_preview(ui: &mut egui::Ui, preview: &SlidesPreview) {
    let width = ui.available_width();
    let caption = if preview.sample {
        tf("bibles.preview.sample", "PREVIEW · {name} (a sample)", &[("name", &preview.name)])
    } else {
        tf("bibles.preview.named", "PREVIEW · {name}", &[("name", &preview.name)])
    };
    ui.add(
        egui::Label::new(egui::RichText::new(caption).size(11.0).strong().color(Color32::from_gray(140))).truncate(),
    );
    ui.add_space(4.0);
    let picture = |ui: &mut egui::Ui, texture: Option<egui::TextureId>, width: f32| {
        let (rect, _) = ui.allocate_exact_size(vec2(width, (width * 9.0 / 16.0).round()), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
        match texture {
            Some(texture) => {
                egui::Image::from_texture((texture, rect.size())).corner_radius(4).paint_at(ui, rect);
            }
            // Drawn in a moment.
            None => ui.ctx().request_repaint(),
        }
        ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(70)), egui::StrokeKind::Outside);
    };
    picture(ui, preview.first, width);
    ui.add_space(8.0);
    match preview.count {
        0 => {}
        1 => {
            let one = tn("bibles.preview.slides", 1, "{n} slide", "{n} slides", &[]);
            ui.label(egui::RichText::new(one).size(12.0).color(DIM));
        }
        count => {
            ui.horizontal_top(|ui| {
                picture(ui, preview.last, (width * 0.45).round());
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(t("bibles.preview.last", "The last")).size(12.0).color(INK));
                    let slides = tn("bibles.preview.slides", count as u64, "{n} slide", "{n} slides", &[]);
                    ui.label(egui::RichText::new(slides).size(12.0).color(DIM));
                });
            });
        }
    }
}

/// The slides the Bible page makes for a passage, from the passage's own (`passage`, made by
/// [`build_passage`] from what was asked and titled as the first translation names it): with a
/// slide a verse and the passage all in one chapter, the whole chapter, so any verse of it can
/// be put on screen and Space goes on through the chapter after the passage; otherwise the
/// passage's own. Returns the slides and the place of the passage's among them.
pub fn page_deck(
    passage: Presentation,
    rows: &[RowChoice],
    options: &BibleOptions,
    template: Option<Uuid>,
) -> (Presentation, std::ops::Range<usize>) {
    let count = passage.slide_count();
    let Some(lead) = rows.first() else { return (passage, 0..count) };
    let Ok(range) = parse_reference_in(&passage.title, &lead.bible.info.language) else { return (passage, 0..count) };
    let one_chapter = (range.start.book, range.start.chapter) == (range.end.book, range.end.chapter);
    let whole = sp_bible::Range {
        start: VerseRef { verse: 1, ..range.start },
        end: VerseRef { verse: sp_bible::LAST, ..range.start },
    };
    // Already the whole chapter, or made into one slide (a chapter would overfill it).
    if !options.break_on_new_verse || !one_chapter || lead.bible.verses(whole).len() == count {
        return (passage, 0..count);
    }
    let (chapter, first) = in_its_chapter(&lead.bible, range.start);
    match build_passage(&chapter, rows, options, template) {
        Ok(slides) if first + count <= slides.slide_count() => (slides, first..first + count),
        _ => (passage, 0..count),
    }
}

/// A verse's whole chapter, as this Bible names it for a lookup (`Genesis 1`), and the place of
/// the verse's slide among the chapter's when they are made one a verse (the verses before it
/// that the Bible has). The Bible is the first translation, which numbers the slides.
pub fn in_its_chapter(bible: &Bible, verse: VerseRef) -> (String, usize) {
    let first = VerseRef { verse: 1, ..verse };
    let before = bible.verses(sp_bible::Range { start: first, end: verse }).len();
    (format!("{} {}", bible.book_name(verse.book), verse.chapter), before.saturating_sub(1))
}

/// The id of a row in the lists: ("book", 42), ("chapter", 3u16), ("verse", 16u16) or
/// ("all", 3u16).
pub fn list_row_id(id: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::unique(("bible lists", id))
}

/// How the Bibles window names a language.
enum OwnName {
    /// As people who speak it write it.
    Is(&'static str),
    /// Its own name and what it is called in the language set (a key and its English, as
    /// [`tk`](crate::i18n::tk) pairs them), for an alphabet the operator may not read.
    Words((&'static str, &'static str)),
}

/// A language Bibles are offered in.
struct KnownLanguage {
    /// Its ISO 639 code: what a Bible's language and the languages the person uses are matched
    /// by.
    code: &'static str,
    /// The other codes systems and Bible files give it: three letters, older codes, a written
    /// form of it (`nb`, `nn` for Norwegian) or an older form (`grc`, `hbo`).
    also: &'static [&'static str],
    /// Its name in English, as [`OFFERED`] names it.
    english: &'static str,
    own: OwnName,
}

/// Every language a Bible is offered in, and the app's own: the one table that turns a code or
/// an English name into the language's code, and names it in the Bibles window.
const LANGUAGES: &[KnownLanguage] = &[
    KnownLanguage { code: "en", also: &["eng"], english: "English", own: OwnName::Is("English") },
    KnownLanguage { code: "ru", also: &["rus"], english: "Russian", own: OwnName::Is("Русский") },
    KnownLanguage { code: "uk", also: &["ukr"], english: "Ukrainian", own: OwnName::Is("Українська") },
    KnownLanguage { code: "de", also: &["deu", "ger"], english: "German", own: OwnName::Is("Deutsch") },
    KnownLanguage { code: "fr", also: &["fra", "fre"], english: "French", own: OwnName::Is("Français") },
    KnownLanguage { code: "es", also: &["spa"], english: "Spanish", own: OwnName::Is("Español") },
    KnownLanguage { code: "pt", also: &["por"], english: "Portuguese", own: OwnName::Is("Português") },
    KnownLanguage { code: "cs", also: &["ces", "cze"], english: "Czech", own: OwnName::Is("Čeština") },
    KnownLanguage { code: "da", also: &["dan"], english: "Danish", own: OwnName::Is("Dansk") },
    KnownLanguage { code: "nl", also: &["nld", "dut"], english: "Dutch", own: OwnName::Is("Nederlands") },
    KnownLanguage { code: "eo", also: &["epo"], english: "Esperanto", own: OwnName::Is("Esperanto") },
    KnownLanguage { code: "eu", also: &["eus", "baq"], english: "Basque", own: OwnName::Is("Euskara") },
    KnownLanguage { code: "fi", also: &["fin"], english: "Finnish", own: OwnName::Is("Suomi") },
    KnownLanguage { code: "hu", also: &["hun"], english: "Hungarian", own: OwnName::Is("Magyar") },
    KnownLanguage { code: "it", also: &["ita"], english: "Italian", own: OwnName::Is("Italiano") },
    KnownLanguage { code: "mg", also: &["mlg"], english: "Malagasy", own: OwnName::Is("Malagasy") },
    KnownLanguage {
        code: "no",
        also: &["nb", "nn", "nor", "nob", "nno"],
        english: "Norwegian",
        own: OwnName::Is("Norsk"),
    },
    KnownLanguage { code: "pl", also: &["pol"], english: "Polish", own: OwnName::Is("Polski") },
    KnownLanguage { code: "sr", also: &["srp"], english: "Serbian", own: OwnName::Is("Srpski · Српски") },
    KnownLanguage { code: "sv", also: &["swe"], english: "Swedish", own: OwnName::Is("Svenska") },
    // Filipino is the standard form of Tagalog: a computer set to it reads Tagalog Bibles.
    KnownLanguage { code: "tl", also: &["tgl", "fil"], english: "Tagalog", own: OwnName::Is("Tagalog") },
    KnownLanguage { code: "tlh", also: &[], english: "Klingon", own: OwnName::Is("tlhIngan Hol") },
    KnownLanguage {
        code: "el",
        also: &["ell", "gre", "grc"],
        english: "Greek",
        own: OwnName::Words(tk("bibles.language.greek", "Ελληνικά · Greek")),
    },
    KnownLanguage {
        code: "he",
        also: &["heb", "hbo", "iw"],
        english: "Hebrew",
        own: OwnName::Words(tk("bibles.language.hebrew", "עברית · Hebrew")),
    },
    KnownLanguage {
        code: "ar",
        also: &["ara"],
        english: "Arabic",
        own: OwnName::Words(tk("bibles.language.arabic", "العربية · Arabic")),
    },
    KnownLanguage {
        code: "hy",
        also: &["hye", "arm"],
        english: "Armenian",
        own: OwnName::Words(tk("bibles.language.armenian", "Հայերեն · Armenian")),
    },
    KnownLanguage {
        code: "zh",
        also: &["zho", "chi"],
        english: "Chinese",
        own: OwnName::Words(tk("bibles.language.chinese", "中文 · Chinese")),
    },
    KnownLanguage {
        code: "ko",
        also: &["kor"],
        english: "Korean",
        own: OwnName::Words(tk("bibles.language.korean", "한국어 · Korean")),
    },
    KnownLanguage {
        code: "fa",
        also: &["fas", "per"],
        english: "Persian",
        own: OwnName::Words(tk("bibles.language.persian", "فارسی · Persian")),
    },
];

/// The language a code or an English name stands for: `uk`, `ukr`, "Ukrainian", and a code
/// with a script or country after it as systems and files write them (`zh-Hant`, `nb-NO`,
/// `uk_UA.UTF-8`), in any case.
fn known_language(language: &str) -> Option<&'static KnownLanguage> {
    let lower = language.trim().to_lowercase();
    let first = lower.split(['-', '_', '.', '@']).next().unwrap_or_default();
    if first.is_empty() {
        return None;
    }
    LANGUAGES
        .iter()
        .find(|known| known.code == first || known.also.contains(&first) || known.english.eq_ignore_ascii_case(first))
}

/// A language's ISO code, from a code or an English name as [`known_language`] reads them; `None`
/// for one no Bible is offered in.
pub fn iso_code(language: &str) -> Option<&'static str> {
    known_language(language).map(|known| known.code)
}

/// A language as people who speak it write it, for the Bibles window: from a language code
/// (`uk`) or an English name ("Ukrainian"). One not known is shown as it is.
pub fn language_name(language: &str) -> String {
    match known_language(language).map(|known| &known.own) {
        Some(OwnName::Is(name)) => (*name).to_owned(),
        Some(OwnName::Words((key, english))) => t(key, english).to_owned(),
        None => language.to_owned(),
    }
}

/// The languages the person uses, as codes, each once, in the order they count: the languages
/// the church shows (`church`, once the setup asks), the app's language as chosen in Settings ›
/// Language (`chosen`; `None` for This computer's, which is the computer's first language, or
/// English until the computer has said), the computer's other preferred languages (`computer`,
/// as the system writes them, its first language first), then the languages of the Bibles
/// installed (`installed`: not those built in, which every copy has). A code no Bible is offered
/// in is left out.
pub fn your_languages(
    church: &[String],
    chosen: Option<&str>,
    computer: &[String],
    installed: &[&str],
) -> Vec<&'static str> {
    let app = chosen.or(computer.first().map(String::as_str)).unwrap_or("en");
    let mut out: Vec<&'static str> = Vec::new();
    let every = church
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(app))
        .chain(computer.iter().map(String::as_str))
        .chain(installed.iter().copied());
    for code in every.filter_map(iso_code) {
        if !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

/// [`your_languages`] now: the church's languages, the Language setting, the computer's
/// preferred languages and the Bibles on the shelf that didn't come with the app.
pub fn your_languages_now(shelf: &Shelf) -> Vec<&'static str> {
    let chosen = match crate::i18n::choice() {
        crate::i18n::Choice::Is(language) => Some(language.code()),
        crate::i18n::Choice::Computer => None,
    };
    let installed: Vec<&str> =
        shelf.bibles.iter().filter(|bible| !bible.shipped).map(|bible| bible.language.as_str()).collect();
    your_languages(&crate::songbooks::church_languages(), chosen, &crate::i18n::computer_tags(), &installed)
}

/// The Bibles offered, in the order the Bibles window and `sp bibles` list them: those in the
/// person's languages first (`yours`, from [`your_languages`]), a language at a time in that
/// order, then the rest as [`OFFERED`] has them. Each is marked true when it is in one of the
/// person's languages.
pub fn offered_in_order(yours: &[&str]) -> Vec<(bool, &'static Offered)> {
    let code_of = |bible: &Offered| iso_code(bible.language);
    let mut out: Vec<(bool, &'static Offered)> = Vec::with_capacity(OFFERED.len());
    for code in yours {
        out.extend(OFFERED.iter().filter(|bible| code_of(bible) == Some(*code)).map(|bible| (true, bible)));
    }
    out.extend(
        OFFERED
            .iter()
            .filter(|bible| code_of(bible).is_none_or(|code| !yours.contains(&code)))
            .map(|bible| (false, bible)),
    );
    out
}

/// Puts downloads that have just ended on the shelf, or says what went wrong. Done every
/// frame, so a Bible finished while its window is closed is ready all the same.
pub fn collect_downloads(options: &mut BibleOptions, shelf: &mut Shelf, downloads: &mut crate::bible_store::Downloads) {
    for (_, result) in downloads.finished() {
        match result {
            Ok((path, info)) => {
                options.problem = None;
                shelf.add_file(path, info);
            }
            Err(problem) => options.problem = Some(problem),
        }
    }
}

/// The Bibles window: a search box, the Bibles installed (each with a Remove button, but for
/// the ones built in), and every Bible that can be downloaded, grouped by language, the
/// person's languages first ([`offered_in_order`]), each with its terms and an Install button
/// that shows the download going on. Removing asks nothing
/// more: the file goes to the Trash and can be put back. Escape or Done closes it.
pub fn manage_bibles(
    ctx: &egui::Context,
    options: &mut BibleOptions,
    shelf: &mut Shelf,
    downloads: &mut crate::bible_store::Downloads,
) {
    let can_download = crate::bible_store::download_base().is_some();
    let ordered = offered_in_order(&your_languages_now(shelf));
    let frame = Frame::new()
        .fill(Color32::from_rgb(38, 38, 40))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(14.0)
        .inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 16 })
        .shadow(egui::Shadow { offset: [0, 16], blur: 44, spread: 0, color: Color32::from_black_alpha(150) });
    let area =
        egui::Modal::default_area(egui::Id::unique("manage bibles")).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0));
    let mut close = false;
    egui::Modal::new(egui::Id::unique("manage bibles")).area(area).backdrop_color(Color32::from_black_alpha(90)).frame(frame).show(
        ctx,
        |ui| {
            let width = 600.0_f32.min(ctx.content_rect().width() - 80.0);
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("bibles.window.title", "Bibles"))
                        .font(crate::studio::semibold(18.0))
                        .color(Color32::WHITE),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t("bibles.window.done", "Done")).clicked() {
                        close = true;
                    }
                    if ui
                        .button(t("bibles.window.from_file", "Install from File…"))
                        .hint(t(
                            "bibles.window.from_file.hint",
                            "A Bible file you have: Zefania, OSIS, Beblia or other XML, USFM or USX (zipped or not), SoftProjector's .spb, or .spbible",
                        ))
                        .clicked()
                    {
                        options.file_request = true;
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut options.manage_search)
                            .hint_text(t("bibles.window.search", "Search by name or language"))
                            .desired_width(200.0),
                    );
                });
            });
            ui.add_space(8.0);
            if let Some(problem) = &options.problem {
                ui.label(egui::RichText::new(problem.as_str()).size(12.0).color(Color32::from_rgb(255, 150, 120)));
                ui.add_space(6.0);
            }
            let typed = options.manage_search.trim().to_lowercase();
            let matches = |name: &str, short: &str, language: &str| {
                typed.is_empty()
                    || [name, short, language, &language_name(language)].iter().any(|t| t.to_lowercase().contains(&typed))
            };
            let height = (ctx.content_rect().height() - 220.0).clamp(240.0, 560.0);
            egui::ScrollArea::vertical().id_salt("bibles window").max_height(height).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_width(width);
                heading(ui, t("bibles.window.installed", "INSTALLED"));
                let mut any = false;
                for (index, bible) in shelf.bibles.iter().enumerate() {
                    if !matches(&bible.name, &bible.short, &bible.language) {
                        continue;
                    }
                    any = true;
                    let detail = format!("{} · {}", bible.short, language_name(&bible.language));
                    let right = if bible.shipped {
                        Right::Note(t("bibles.window.built_in", "Built in"))
                    } else {
                        Right::Button(t("bibles.window.remove", "Remove"))
                    };
                    if bible_row(ui, ("installed", index), &bible.name, &detail, None, right) {
                        options.remove = Some(index);
                    }
                }
                if !any {
                    nothing(ui, t("bibles.window.none_installed", "No installed Bible matches."));
                }
                ui.add_space(12.0);
                heading(ui, t("bibles.window.available", "AVAILABLE"));
                if !can_download {
                    nothing(
                        ui,
                        t(
                            "bibles.window.no_downloads",
                            "Downloads aren't available in this copy. Drop a Bible file on the window, or use Install from File…, to install it.",
                        ),
                    );
                }
                // The person's languages first, under a heading of their own, then the others
                // under theirs; with none of the person's to show, no headings, as before.
                let shown: Vec<(bool, &Offered)> = ordered
                    .iter()
                    .copied()
                    .filter(|(_, bible)| {
                        shelf.position(bible.id).is_none() && matches(bible.name, bible.short, bible.language)
                    })
                    .collect();
                let parted = shown.iter().any(|(yours, _)| *yours);
                let mut part = None;
                let mut language = "";
                for &(yours, bible) in &shown {
                    if parted && part != Some(yours) {
                        part = Some(yours);
                        part_heading(
                            ui,
                            if yours {
                                t("bibles.window.your_languages", "Your languages")
                            } else {
                                t("bibles.window.other_languages", "Other languages")
                            },
                        );
                    }
                    if bible.language != language {
                        language = bible.language;
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(language_name(language)).font(crate::studio::semibold(12.5)).color(DIM));
                        ui.add_space(2.0);
                    }
                    let right = if downloads.is_busy(bible.id) {
                        Right::Note(t("bibles.window.downloading", "Downloading…"))
                    } else if can_download {
                        Right::Button(t("bibles.install", "Install"))
                    } else {
                        Right::Note("")
                    };
                    if bible_row(ui, ("offered", bible.id), bible.name, bible.short, Some(terms_shown(bible.terms)), right) {
                        let ctx = ui.ctx().clone();
                        downloads.start(bible.id, bible.checksum, move || ctx.request_repaint());
                    }
                }
                if shown.is_empty() && can_download {
                    nothing(
                        ui,
                        if typed.is_empty() {
                            t("bibles.window.all_installed", "Every Bible offered is installed.")
                        } else {
                            t("bibles.window.none_offered", "No Bible to install matches.")
                        },
                    );
                }
            });
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(t(
                    "bibles.window.footer",
                    "Every Bible here may be shared freely: public domain, or an open licence. A Bible file you have (Zefania, OSIS, USFM, SoftProjector's .spb and others, or .spbible) installs when dropped on the window or chosen with Install from File…",
                ))
                .size(11.0)
                .color(DIM),
            );
            if downloads_busy(downloads) {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            }
        },
    );
    // Escape with the sheet asking what a Bible file is open closes only the sheet.
    if close || (options.details.is_empty() && ctx.input(|input| input.key_pressed(egui::Key::Escape))) {
        options.manage_open = false;
    }
}

/// An offered Bible's terms as the window shows them: "Public domain" in the language set, a
/// licence by its own name (CC BY 4.0) as it is.
pub(crate) fn terms_shown(terms: &'static str) -> &'static str {
    if terms == "Public domain" { t("bibles.terms.public_domain", "Public domain") } else { terms }
}

/// True while any Bible is downloading.
fn downloads_busy(downloads: &crate::bible_store::Downloads) -> bool {
    OFFERED.iter().any(|bible| downloads.is_busy(bible.id))
}

/// The name of a part of the Bibles to download (the person's languages, then the others): a
/// little larger and brighter than the languages' names under it, with a faint line to the edge.
fn part_heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text).font(crate::studio::semibold(13.5)).color(INK));
        let rest = ui.available_rect_before_wrap();
        let y = ui.min_rect().center().y;
        ui.painter().hline((rest.left() + 4.0)..=rest.right(), y, Stroke::new(1.0, Color32::from_white_alpha(40)));
    });
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(11.0).strong().color(Color32::from_gray(130)));
    ui.add_space(4.0);
}

fn nothing(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(DIM)).wrap());
}

/// What a Bible's row shows at its right.
enum Right {
    Note(&'static str),
    Button(&'static str),
}

/// One Bible in the window: its name, a line under it, its terms, and a note or a button at
/// the right, which keeps its room however long the name is. True when the button is clicked.
fn bible_row(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    detail: &str,
    terms: Option<&str>,
    right: Right,
) -> bool {
    let width = ui.available_width();
    let (row, hover) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
    if hover.hovered() {
        ui.painter().rect_filled(row, 8.0, Color32::from_white_alpha(8));
    }
    // The right side: the button (or note), and the terms before it.
    let button = Rect::from_min_size(pos2(row.max.x - 92.0, row.center().y - 13.0), vec2(84.0, 26.0));
    let mut clicked = false;
    match right {
        Right::Button(label) => {
            let response = ui.interact(button, ui.scope_id().with(id), Sense::click());
            let fill = if response.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(24) };
            ui.painter().rect_filled(button, 7.0, fill);
            ui.painter().text(button.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.5), INK);
            clicked = response.clicked();
        }
        Right::Note(note) => {
            ui.painter().text(button.center(), Align2::CENTER_CENTER, note, FontId::proportional(11.5), DIM);
        }
    }
    let mut text_right = button.min.x - 10.0;
    if let Some(terms) = terms.filter(|t| !t.is_empty()) {
        let galley = ui.painter().layout_no_wrap(terms.to_owned(), FontId::proportional(10.5), DIM);
        let pill = Rect::from_min_size(
            pos2(text_right - galley.size().x - 12.0, row.center().y - 9.0),
            vec2(galley.size().x + 12.0, 18.0),
        );
        ui.painter().rect_filled(pill, 9.0, Color32::from_white_alpha(14));
        ui.painter().galley(pos2(pill.min.x + 6.0, pill.center().y - galley.size().y / 2.0), galley, DIM);
        text_right = pill.min.x - 10.0;
    }
    // The name and the line under it, cut off before what is at the right.
    let clip = Rect::from_min_max(pos2(row.min.x + 10.0, row.min.y), pos2(text_right, row.max.y));
    let painter = ui.painter().with_clip_rect(clip);
    painter.text(pos2(row.min.x + 10.0, row.min.y + 14.0), Align2::LEFT_CENTER, name, FontId::proportional(13.5), INK);
    painter.text(
        pos2(row.min.x + 10.0, row.min.y + 31.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::proportional(11.0),
        DIM,
    );
    clicked
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_ukrainian_bible_reads_ukrainian_book_names_and_shows_them() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../dist/bibles/ukr1905.pcbible");
        let Ok(bytes) = std::fs::read(path) else { return };
        let mut shelf = Shelf::shipped();
        let info = sp_bible::Bible::read_info(&bytes).unwrap();
        shelf.add_file(path.into(), info);
        let rows = rows(&mut shelf, &[("ukr1905", main(), reference())]);
        let passage = build_passage("Ів 3:16", &rows, &BibleOptions::default(), None).unwrap();
        assert_eq!(passage.title, "Від Івана 3:16");
    }

    use super::*;

    fn main() -> BibleTarget {
        Some(Role::primary())
    }

    fn translation() -> BibleTarget {
        Some(Role::translation())
    }

    fn reference() -> BibleTarget {
        Some(Role::reference())
    }

    fn rows(shelf: &mut Shelf, ids: &[(&str, BibleTarget, BibleTarget)]) -> Vec<RowChoice> {
        ids.iter()
            .map(|(id, verse, reference)| RowChoice {
                bible: shelf.get(shelf.position(id).unwrap()).unwrap(),
                verse: verse.clone(),
                reference: reference.clone(),
            })
            .collect()
    }

    fn texts(slide: &Slide) -> Vec<&str> {
        slide.objects.iter().filter_map(Object::as_text).collect()
    }

    #[test]
    fn a_language_reads_in_its_bibles_numbering_and_a_passage_turns_through_the_tables() {
        use sp_bible::Numbering::{English, Synodal};
        let mut shelf = Shelf::shipped();
        let kjv = shelf.get(shelf.position("kjv").unwrap()).unwrap();
        let syn = shelf.get(shelf.position("syn").unwrap()).unwrap();
        let both = [syn.as_ref(), kjv.as_ref()];
        assert_eq!((numbering_of("en", &both), numbering_of("ru", &both)), (English, Synodal));
        // No Bible in use in the language: English and Russian have their own; Ukrainian the
        // first Bible's.
        assert_eq!(numbering_of("ru", &[kjv.as_ref()]), Synodal);
        assert_eq!((numbering_of("uk", &both), numbering_of("uk", &[kjv.as_ref()])), (Synodal, English));
        // A Ukrainian Bible in use: its numbering, whatever Bible is first.
        let info = sp_bible::Info {
            id: "test-uk".to_owned(),
            name: "Test Ukrainian".to_owned(),
            short: "TUK".to_owned(),
            language: "uk".to_owned(),
            terms: "Written for a test".to_owned(),
            credit: String::new(),
            source: "a test".to_owned(),
            numbering: English,
            loose: false,
        };
        let ukrainian = Bible::from_verse_lines(info, "PSA 23:1 Words written for a test.", str::to_owned);
        assert_eq!(numbering_of("uk", &[syn.as_ref(), &ukrainian]), English);
        // "Psalm 23" in the English numbering is the Synodal's 22:1-6: its end found in the King
        // James Version; with no Bible of the English numbering in use, its chapter matched.
        let psalm = parse_reference_in("Psalm 23", "en").unwrap();
        let at = |chapter, verse| VerseRef { book: psalm.start.book, chapter, verse };
        let turned = renumber(psalm, English, Synodal, &both);
        assert_eq!((turned.start, turned.end), (at(22, 1), at(22, 6)));
        let turned = renumber(psalm, English, Synodal, &[syn.as_ref()]);
        assert_eq!((turned.start, turned.end), (at(22, 1), at(22, sp_bible::LAST)));
        // Named for the first Bible to look up, and back for people in each language.
        assert_eq!(renumbered("Psalms 51:1-19", English, &both), "Псалтирь 50:3-21");
        assert_eq!(renumbered("Psalm 23", English, &both), "Псалтирь 22:1-6");
        assert_eq!(renumbered("Psalm 23", English, &[kjv.as_ref(), syn.as_ref()]), "Psalm 23", "as it is");
        assert_eq!(name_for_operator("Псалтирь 22:1-6", &both, "en"), "Psalm 23:1-6");
        assert_eq!(name_for_operator("Псалтирь 22:1-6", &both, "ru"), "Псалом 22:1-6");
        assert_eq!(named_in("Псалтирь 50:3-21", Synodal, "en", &both, false), "Psalms 51:1-19");
        // A slide's bar reads in the language the operator reads in, its Bible's short name kept.
        assert_eq!(slide_name_for_operator("Псалтирь 22:1", &both, "en"), "Psalm 23:1");
        assert_eq!(slide_name_for_operator("Псалтирь 22:1 (SYN)", &both, "en"), "Psalm 23:1 (SYN)");
        assert_eq!(slide_name_for_operator("Псалтирь 22:1", &both, "ru"), "Псалом 22:1");
    }

    #[test]
    fn the_page_makes_a_passage_in_one_chapter_into_the_chapters_slides_with_its_own_among_them() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", main(), reference())]);
        let deck = |asked: &str, options: &BibleOptions| {
            let passage = build_passage(asked, &rows, options, None).unwrap();
            let (slides, among) = page_deck(passage, &rows, options, None);
            (slides.title.clone(), slides.slide_count(), among)
        };
        let one_a_verse = BibleOptions::default();
        assert!(one_a_verse.break_on_new_verse);
        // A verse, or verses, of one chapter: the chapter, the passage's slides picked out.
        assert_eq!(deck("John 3:16", &one_a_verse), ("John 3:1-36".to_owned(), 36, 15..16));
        assert_eq!(deck("John 3:16-18", &one_a_verse), ("John 3:1-36".to_owned(), 36, 15..18));
        // The whole chapter is its own; a passage across two chapters keeps its own.
        assert_eq!(deck("John 3", &one_a_verse), ("John 3:1-36".to_owned(), 36, 0..36));
        assert_eq!(deck("John 3:35-4:2", &one_a_verse), ("John 3:35-4:2".to_owned(), 4, 0..4));
        // Made into one slide: the passage's one slide.
        let one_slide = BibleOptions { break_on_new_verse: false, ..BibleOptions::default() };
        assert_eq!(deck("John 3:16", &one_slide), ("John 3:16".to_owned(), 1, 0..1));
        assert_eq!(deck("John 3:16-18", &one_slide), ("John 3:16-18".to_owned(), 1, 0..1));
    }

    #[test]
    fn a_reference_becomes_one_slide_for_each_verse_in_both_languages() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", main(), reference()), ("syn", translation(), None)]);
        let passage = build_passage("John 3:16-17", &rows, &BibleOptions::default(), None).unwrap();
        assert_eq!(passage.title, "John 3:16-17");
        assert_eq!(passage.slide_count(), 2);
        // Each slide's bar names its own verse.
        assert_eq!(passage.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["John 3:16", "John 3:17"]);
        let (_, first) = passage.slide(0).unwrap();
        let shown = texts(first);
        assert!(shown[0].starts_with("For God so loved the world"));
        assert!(shown[1].starts_with("Ибо так возлюбил Бог мир"));
        assert_eq!(shown[2], "John 3:16");
    }

    #[test]
    fn the_russian_psalm_is_the_one_that_matches_the_english() {
        let mut shelf = Shelf::shipped();
        let english_first = rows(&mut shelf, &[("kjv", main(), reference()), ("syn", translation(), reference())]);
        let passage = build_passage("Psalm 23:1", &english_first, &BibleOptions::default(), None).unwrap();
        let shown = texts(passage.slide(0).unwrap().1).join(" | ");
        assert!(shown.contains("The LORD is my shepherd") && shown.contains("Пастырь мой"), "{shown}");
        // Each reference reads as one psalm, as people write it ("Psalm 23:1", "Псалом 22:1"),
        // while the slide's name stays as the Bible names it.
        assert!(shown.contains("Psalm 23:1  ·  Псалом 22:1"), "{shown}");
        assert_eq!(passage.groups[0].name, "Psalms 23:1");
        // Across two psalms, psalms.
        let across = build_passage("Psalm 23:6-24:1", &english_first, &BibleOptions::default(), None).unwrap();
        let options = BibleOptions { reference: ReferenceStyle::PassageEach, ..BibleOptions::default() };
        let whole = build_passage("Psalm 23:6-24:1", &english_first, &options, None).unwrap();
        assert!(texts(across.slide(1).unwrap().1).join(" | ").contains("Psalm 24:1  ·  Псалом 23:1"));
        let shown = texts(whole.slide(0).unwrap().1).join(" | ");
        assert!(shown.contains("Psalms 23:6-24:1  ·  Псалтирь 22:6-23:1"), "{shown}");
        // Typed the Russian way with the Russian Bible first, it is the same psalm.
        let russian_first = rows(&mut shelf, &[("syn", main(), None), ("kjv", translation(), None)]);
        let passage = build_passage("Пс 22:1", &russian_first, &BibleOptions::default(), None).unwrap();
        let shown = texts(passage.slide(0).unwrap().1).join(" | ");
        assert!(shown.contains("Пастырь мой") && shown.contains("The LORD is my shepherd"), "{shown}");
    }

    #[test]
    fn the_options_change_the_slides() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("web", main(), reference())]);
        let options = BibleOptions {
            break_on_new_verse: false,
            show_verse_numbers: true,
            display_translation: true,
            ..BibleOptions::default()
        };
        let passage = build_passage("jn 3:16-17", &rows, &options, None).unwrap();
        assert_eq!(passage.slide_count(), 1);
        let shown = texts(passage.slide(0).unwrap().1);
        assert!(
            shown[0].starts_with("¹⁶\u{2009}For God so loved") && shown[0].contains(" ¹⁷\u{2009}For God"),
            "{}",
            shown[0]
        );
        assert_eq!(shown[1], "John 3:16-17 (WEB)");

        let last_only = BibleOptions { reference: ReferenceStyle::PassageLast, ..BibleOptions::default() };
        let passage = build_passage("jn 3:16-17", &rows, &last_only, None).unwrap();
        assert_eq!(texts(passage.slide(0).unwrap().1).len(), 1);
        assert_eq!(texts(passage.slide(1).unwrap().1)[1], "John 3:16-17");

        let none = BibleOptions { reference: ReferenceStyle::NoReference, ..BibleOptions::default() };
        let passage = build_passage("jn 3:16", &rows, &none, None).unwrap();
        assert_eq!(texts(passage.slide(0).unwrap().1).len(), 1);
    }

    #[test]
    fn words_and_reference_sharing_a_box_are_one_object_with_the_reference_last() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", main(), main()), ("syn", translation(), None)]);
        let passage = build_passage("John 3:16", &rows, &BibleOptions::default(), None).unwrap();
        let (_, slide) = passage.slide(0).unwrap();
        assert_eq!(slide.objects.len(), 2);
        let main_box = slide.text_for(&Role::primary()).unwrap();
        assert!(main_box.starts_with("For God so loved") && main_box.ends_with("\nJohn 3:16"), "{main_box}");
        // A translation sent nowhere is left off the slide.
        let rows = self::rows(&mut shelf, &[("kjv", main(), None), ("syn", None, None)]);
        let passage = build_passage("John 3:16", &rows, &BibleOptions::default(), None).unwrap();
        assert_eq!(passage.slide(0).unwrap().1.objects.len(), 1);
    }

    #[test]
    fn bible_slides_name_the_template_chosen_for_them() {
        let mut shelf = Shelf::shipped();
        let theme = sp_model::Theme::builtin();
        let chosen = theme.templates[0].id;
        // One language, on the template for two: the slide still uses the one it names.
        let one = rows(&mut shelf, &[("kjv", main(), reference())]);
        let passage = build_passage("John 3:16-17", &one, &BibleOptions::default(), Some(chosen)).unwrap();
        for (_, slide) in passage.slides() {
            assert_eq!(slide.template, Some(chosen));
            assert_eq!(theme.template_for(slide).map(|template| template.id), Some(chosen));
            assert_eq!(slide.objects[0].frame, None);
        }
    }

    #[test]
    fn a_reference_that_names_nothing_says_so() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", main(), reference())]);
        // A chapter past the end of the book is read as its last chapter.
        let last = build_passage("John 99", &rows, &BibleOptions::default(), None).unwrap();
        assert!(last.title.starts_with("John 21"), "{}", last.title);
        assert!(
            build_passage("John 3:99", &rows, &BibleOptions::default(), None).unwrap_err().contains("no such verses")
        );
        assert!(build_passage("Nobody 1:1", &rows, &BibleOptions::default(), None).unwrap_err().contains("No book"));
        assert!(build_passage("John 3:16", &[], &BibleOptions::default(), None).is_err());
    }

    #[test]
    fn a_bible_installed_again_replaces_its_earlier_copy_but_never_a_built_in_one() {
        let info = |id: &str, name: &str| sp_bible::Info {
            id: id.to_owned(),
            name: name.to_owned(),
            short: "TB".to_owned(),
            language: "en".to_owned(),
            terms: String::new(),
            credit: String::new(),
            source: String::new(),
            numbering: Numbering::English,
            loose: false,
        };
        let mut shelf = Shelf::shipped();
        let shipped = shelf.bibles.len();
        shelf.put_installed("a.spbible".into(), info("tb", "Test Bible"));
        let known = shelf.known();
        assert!(known.bibles.iter().any(|bible| bible.id == "tb" && !bible.shipped));
        assert!(known.bibles.iter().any(|bible| bible.id == "kjv" && bible.shipped));
        shelf.put_installed("b.spbible".into(), info("TB", "Test Bible, corrected"));
        assert_eq!(shelf.bibles.len(), shipped + 1);
        assert_eq!(shelf.bibles[shipped].name, "Test Bible, corrected");
        shelf.put_installed("kjv.spbible".into(), info("kjv", "Not the King James"));
        assert_eq!(shelf.bibles[0].name, "King James Version");
    }

    #[test]
    fn a_language_is_named_as_its_speakers_write_it_from_its_name_or_code() {
        assert_eq!(language_name("Dutch"), "Nederlands");
        assert_eq!(language_name("nl"), "Nederlands");
        // A file's language code, with or without its script or country, is named the same.
        assert_eq!(language_name("zh-Hant"), language_name("Chinese"));
        assert_eq!(language_name("pt-BR"), "Português");
        // One it doesn't know is shown as it is.
        assert_eq!(language_name("xx"), "xx");
    }

    #[test]
    fn offered_bibles_are_grouped_by_language_and_named_once() {
        let mut seen: Vec<&str> = Vec::new();
        let mut languages: Vec<&str> = Vec::new();
        let shipped = Shelf::shipped();
        for bible in OFFERED {
            assert!(!seen.contains(&bible.id), "{} is listed twice", bible.id);
            assert!(shipped.position(bible.id).is_none(), "{} already comes with the app", bible.id);
            assert_eq!(bible.checksum.len(), 64);
            seen.push(bible.id);
            if languages.last() != Some(&bible.language) {
                assert!(!languages.contains(&bible.language), "{} is split into two groups", bible.language);
                languages.push(bible.language);
            }
        }
    }

    #[test]
    fn every_offered_language_has_one_code_of_its_own() {
        let mut codes: Vec<(&str, &str)> = Vec::new();
        for bible in OFFERED {
            let code = iso_code(bible.language).unwrap_or_else(|| panic!("{} has no code", bible.language));
            match codes.iter().find(|(seen, _)| *seen == code) {
                Some((_, named)) => assert_eq!(*named, bible.language, "{code} is two languages"),
                None => codes.push((code, bible.language)),
            }
            // Its code names it as its English name does.
            assert_eq!(language_name(code), language_name(bible.language));
        }
        // Codes and aliases are each one language's.
        let mut every: Vec<&str> = Vec::new();
        for known in LANGUAGES {
            for code in std::iter::once(known.code).chain(known.also.iter().copied()) {
                assert!(!every.contains(&code), "{code} is given twice");
                assert_eq!(code, code.to_ascii_lowercase());
                every.push(code);
            }
        }
        // The app's own languages are there.
        for language in crate::i18n::Language::ALL {
            assert_eq!(iso_code(language.code()), Some(language.code()));
        }
    }

    #[test]
    fn a_language_is_found_by_any_code_systems_and_files_give_it() {
        for (given, code) in [
            ("nb-NO", "no"),
            ("nn", "no"),
            ("Norwegian", "no"),
            ("zh-Hant", "zh"),
            ("zh_CN.UTF-8", "zh"),
            ("grc", "el"),
            ("hbo", "he"),
            ("rus", "ru"),
            ("UKR", "uk"),
            ("uk_UA.UTF-8@euro", "uk"),
            ("pt-BR", "pt"),
            ("fil", "tl"),
            ("english", "en"),
            ("RUSSIAN", "ru"),
        ] {
            assert_eq!(iso_code(given), Some(code), "{given}");
        }
        for unknown in ["", " ", "xx", "C", "POSIX", "be-BY", "Englishish"] {
            assert_eq!(iso_code(unknown), None, "{unknown:?}");
        }
    }

    fn tags(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    #[test]
    fn your_languages_are_the_apps_then_the_computers_then_the_installed_bibles() {
        // The app in Russian, a computer preferring Ukrainian then English, a Spanish Bible
        // installed.
        assert_eq!(your_languages(&[], Some("ru"), &tags(&["uk-UA", "en-US"]), &["es"]), ["ru", "uk", "en", "es"]);
        // This computer's: the computer's own language comes first, and English isn't added for
        // the app's words being in English (a German computer, with no German words).
        assert_eq!(your_languages(&[], None, &tags(&["uk-UA", "en-US"]), &[]), ["uk", "en"]);
        assert_eq!(your_languages(&[], None, &tags(&["de-DE"]), &[]), ["de"]);
        // Before the computer has said, the app is in English.
        assert_eq!(your_languages(&[], None, &[], &[]), ["en"]);
        // Each once, however it is written.
        assert_eq!(
            your_languages(&[], Some("uk"), &tags(&["uk-UA", "ru", "ru_RU.UTF-8", "en"]), &["ukr", "rus", "en"]),
            ["uk", "ru", "en"]
        );
        // Codes no Bible is offered in, or none at all, are left out.
        assert_eq!(your_languages(&[], Some("es"), &tags(&["be-BY", "xx", "", "C"]), &["", "xx"]), ["es"]);
        // The languages the church shows, once the setup asks, come first.
        assert_eq!(
            your_languages(&tags(&["ru", "en"]), Some("uk"), &tags(&["uk", "de"]), &["es"]),
            ["ru", "en", "uk", "de", "es"]
        );
    }

    #[test]
    fn the_persons_languages_are_offered_first_and_the_rest_as_before() {
        let ordered = offered_in_order(&["ru", "uk", "en", "es"]);
        assert_eq!(ordered.len(), OFFERED.len());
        let languages: Vec<&str> = ordered.iter().map(|(_, bible)| bible.language).collect();
        let mut groups: Vec<&str> = languages.clone();
        groups.dedup();
        assert_eq!(groups[..4], ["Russian", "Ukrainian", "English", "Spanish"]);
        // Marked as the person's exactly while in those four.
        let yours = ordered.iter().take_while(|(yours, _)| *yours).count();
        assert!(ordered[yours..].iter().all(|(yours, _)| !yours));
        assert_eq!(
            yours,
            OFFERED.iter().filter(|b| ["Russian", "Ukrainian", "English", "Spanish"].contains(&b.language)).count()
        );
        // Within a language, and among the others, the list's own order.
        let rest: Vec<&str> = ordered[yours..].iter().map(|(_, bible)| bible.id).collect();
        let before: Vec<&str> = OFFERED
            .iter()
            .filter(|b| !["Russian", "Ukrainian", "English", "Spanish"].contains(&b.language))
            .map(|b| b.id)
            .collect();
        assert_eq!(rest, before);
        let english: Vec<&str> = ordered.iter().filter(|(_, b)| b.language == "English").map(|(_, b)| b.id).collect();
        let english_before: Vec<&str> = OFFERED.iter().filter(|b| b.language == "English").map(|b| b.id).collect();
        assert_eq!(english, english_before);
        // With none of the person's, the list is as it was.
        let plain: Vec<&str> = offered_in_order(&[]).iter().map(|(_, b)| b.id).collect();
        assert_eq!(plain, OFFERED.iter().map(|b| b.id).collect::<Vec<_>>());
    }

    #[test]
    fn an_offered_bible_is_named_by_its_short_name_in_any_case_and_alphabet() {
        assert_eq!(offered_named("ASV").map(|b| b.id), Some("asv"));
        assert_eq!(offered_named("asv").map(|b| b.id), Some("asv"));
        assert_eq!(offered_named("Lut1912").map(|b| b.id), Some("lut1912"));
        assert_eq!(offered_named("НПУ").map(|b| b.id), Some("ukronpu"));
        assert_eq!(offered_named("нпу").map(|b| b.id), Some("ukronpu"));
        assert_eq!(offered_named("UKRONPU").map(|b| b.id), Some("ukronpu"));
        // Only a whole short name: never in part, and never a Bible that comes with the app.
        for typed in ["NIV", "AS", "", "KJV", "American Standard Version"] {
            assert!(offered_named(typed).is_none(), "{typed}");
        }
    }
}
