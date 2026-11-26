//! Songs from SoftProjector, the open-source presentation program many Russian- and
//! Ukrainian-speaking churches use. Its songs are kept in an SQLite database (its own
//! `spData.sqlite`, or a songbook exported as `.sps`) or, in songbooks made before its version
//! 2 (those it offers for download among them), in a text file also ending `.sps`;
//! [`read_songbook`] reads the songs out of any of the three, and [`from_softprojector`] turns
//! one song's text into SoftPresenter's groups and slides.
//!
//! A song's text is blocks, each starting with a line naming it: a verse ("Куплет 1",
//! "Verse 2"), the chorus ("Припев", "Приспів", "Chorus"), or a slide of another kind ("Слайд",
//! "Вставка" (an insert), "Вступление" (an intro), "Окончание" (an ending)…). A block runs to
//! the next such line and is shown as one slide. A name starting with `&` ("&Куплет") is more
//! of the verse or chorus before it. SoftProjector sings the chorus after every verse; the song
//! comes with an arrangement in that order, "Chorus after each verse", by the rule in
//! `chorus_after.rs`, which songs made as text follow too.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use crate::chorus_after::{CHORUS_AFTER_EACH_VERSE, Part, sung_order};
use crate::sqlite_read::{Database, Limits, Value, is_database};
use crate::{Arrangement, Copyright, Group, Object, Presentation, Role, Slide, Uuid, group_color};

/// One song as a songbook keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct SongbookSong {
    /// Its number in its songbook.
    pub number: Option<i64>,
    pub title: String,
    /// Who wrote the words, and who the music.
    pub words: String,
    pub music: String,
    /// The song itself, as [`from_softprojector`] takes it.
    pub text: String,
    /// The name of its songbook.
    pub book: Option<String>,
}

/// Why a file gave no songs.
#[derive(Clone, Debug, PartialEq)]
pub enum SongbookProblem {
    /// Not a songbook of a kind read here.
    NotSongbook,
    /// A songbook with no songs in it.
    NoSongs,
    /// A songbook that could not be read; why, as the end of a sentence.
    Unreadable(String),
}

/// A song's values on the way in, as the database has them: number, title, words, music, text
/// and the songbook's name.
type Fields = [Value; 6];

/// Every song in a SoftProjector database, an exported songbook or a songbook kept as text,
/// told apart by how the file starts. They come in the songbooks' order by name and, within a songbook, by number; songs
/// with the same number stay in the order they were kept in.
pub fn read_songbook<R: Read + Seek>(mut source: R) -> Result<Vec<SongbookSong>, SongbookProblem> {
    let unreadable = |e: std::io::Error| SongbookProblem::Unreadable(format!("it could not be read ({e})"));
    let mut head = Vec::new();
    source.by_ref().take(16).read_to_end(&mut head).map_err(unreadable)?;
    let fields = if is_database(&head) {
        database_songs(source).map_err(|problem| problem.unwrap_or(SongbookProblem::NotSongbook))?
    } else {
        source.seek(SeekFrom::Start(0)).map_err(unreadable)?;
        let mut bytes = Vec::new();
        source.by_ref().take(LARGEST_TEXT_SONGBOOK + 1).read_to_end(&mut bytes).map_err(unreadable)?;
        let whole = String::from_utf8_lossy(&bytes);
        let whole = whole.strip_prefix('\u{feff}').unwrap_or(&whole);
        if whole.starts_with("<?xml") && whole.contains("spSongBook") {
            return Err(SongbookProblem::Unreadable(
                "it is a songbook saved as XML, which SoftPresenter doesn't read yet".to_owned(),
            ));
        }
        if !whole.starts_with("##") {
            return Err(SongbookProblem::NotSongbook);
        }
        if bytes.len() as u64 > LARGEST_TEXT_SONGBOOK {
            return Err(SongbookProblem::Unreadable("it is larger than any songbook".to_owned()));
        }
        text_songs(whole)
    };
    if fields.is_empty() {
        return Err(SongbookProblem::NoSongs);
    }
    let words = |value: &Value| match value {
        Value::Text(text) => text.clone(),
        _ => String::new(),
    };
    Ok(fields
        .iter()
        .map(|[number, title, by_words, by_music, text, book]| SongbookSong {
            number: match number {
                Value::Integer(number) => Some(*number),
                Value::Text(written) => written.trim().parse().ok(),
                _ => None,
            },
            title: words(title),
            words: words(by_words),
            music: words(by_music),
            text: words(text),
            book: Some(words(book)).filter(|book| !book.trim().is_empty()),
        })
        .collect())
}

/// The largest songbook kept as text that is read (the largest there is has four megabytes).
const LARGEST_TEXT_SONGBOOK: u64 = 64 * 1024 * 1024;

/// The songs of a songbook kept as text, the way SoftProjector kept them before its version 2
/// (the songbooks it offers for download are still kept so): three lines starting `##` (a
/// code, the songbook's title, a note about it), then a song a line, its parts separated by
/// `#$#`: number, title, category, tune, who wrote the words, who wrote the music, and the
/// text, in which `@$` ends a verse and `@%` ends a line. A line with fewer parts is no song.
/// They come in the order of their numbers, as they would out of the database SoftProjector
/// puts them in.
fn text_songs(whole: &str) -> Vec<Fields> {
    let mut lines = whole.split('\n');
    let _code = lines.next();
    let book = lines.next().unwrap_or("").replace('#', "").trim().to_owned();
    let _note = lines.next();
    let mut fields: Vec<Fields> = lines
        .filter_map(|line| {
            let parts: Vec<&str> = line.trim_end_matches('\r').split("#$#").collect();
            let [number, title, _category, _tune, words, music, text, ..] = parts.as_slice() else { return None };
            // Each verse's lines, then an empty line before the next: the same text, to the
            // letter, as SoftProjector's own databases hold for these songbooks.
            let verses: Vec<String> = text
                .split("@$")
                .map(|verse| verse.split("@%").collect::<Vec<_>>().join("\n").trim().to_owned())
                .collect();
            // A number is kept as a number where it is one, as a column of whole numbers keeps it.
            let number = number.trim();
            let number = match (number.parse::<i64>(), number.parse::<f64>()) {
                (Ok(whole), _) => Value::Integer(whole),
                (_, Ok(fraction)) if fraction.is_finite() => Value::Real(fraction),
                _ => Value::Text(number.to_owned()),
            };
            let written = |part: &str| Value::Text(part.to_owned());
            let text = verses.join("\n\n");
            Some([number, written(title), written(words), written(music), written(text.trim()), written(&book)])
        })
        .take(MOST_SONGS)
        .collect();
    fields.sort_by(|[a, ..], [b, ..]| sort_order(a, b, &|a: &str, b: &str| a.cmp(b)));
    fields
}

/// The most songs one file may give: many times the largest songbook, and few enough that a
/// file made to exhaust the computer can't.
const MOST_SONGS: usize = 200_000;

/// A whole number against one with a fraction, exactly (not by rounding the whole number to
/// the nearest the other kind can hold), so that sorting by it is always one order.
fn whole_and_fraction(whole: i64, fraction: f64) -> Ordering {
    // Beyond every whole number, either way (2 to the 63rd).
    const EDGE: f64 = 9_223_372_036_854_775_808.0;
    if fraction >= EDGE {
        return Ordering::Less;
    }
    if fraction < -EDGE || fraction.is_nan() {
        return Ordering::Greater;
    }
    // Inside that, the whole part is a whole number exactly.
    let part = fraction.trunc();
    whole.cmp(&(part as i64)).then_with(|| 0.0_f64.total_cmp(&(fraction - part)))
}

/// A songbook's id, or a song's `songbook_id`, as something to look up by: a number with no
/// fraction is the whole number it equals.
#[derive(PartialEq, Eq, Hash)]
enum Id {
    Whole(i64),
    Fraction(u64),
    Text(String),
    Bytes(Vec<u8>),
}

impl Id {
    /// `None` for nothing, which equals nothing.
    fn of(value: &Value) -> Option<Id> {
        Some(match value {
            Value::Null => return None,
            Value::Integer(whole) => Id::Whole(*whole),
            Value::Real(x) if whole_and_fraction(*x as i64, *x) == Ordering::Equal => Id::Whole(*x as i64),
            Value::Real(x) => Id::Fraction(x.to_bits()),
            Value::Text(text) => Id::Text(text.clone()),
            Value::Blob(bytes) => Id::Bytes(bytes.clone()),
        })
    }
}

/// Which of two values SQLite puts first when it sorts: nothing, then numbers, then text by
/// its bytes, then bytes.
fn sort_order(a: &Value, b: &Value, text: &impl Fn(&str, &str) -> Ordering) -> Ordering {
    let rank = |value: &Value| match value {
        Value::Null => 0,
        Value::Integer(_) | Value::Real(_) => 1,
        Value::Text(_) => 2,
        Value::Blob(_) => 3,
    };
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
        (Value::Integer(a), Value::Real(b)) => whole_and_fraction(*a, *b),
        (Value::Real(a), Value::Integer(b)) => whole_and_fraction(*b, *a).reverse(),
        // Nothing and less-than-nothing are the same number, as they are to a whole 0. (A
        // number that isn't one never gets here: the reader gives nothing for it.)
        (Value::Real(a), Value::Real(b)) => a.partial_cmp(b).unwrap_or_else(|| a.total_cmp(b)),
        (Value::Text(a), Value::Text(b)) => text(a, b),
        (Value::Blob(a), Value::Blob(b)) => a.cmp(b),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// The songs of an SQLite file, as SQLite itself gives them for these two questions. Of a
/// whole database (it has a `Songbooks` table):
///
/// ```sql
/// SELECT s.number, s.title, s.words, s.music, s.song_text, b.name
/// FROM Songs s LEFT JOIN Songbooks b ON b.id = s.songbook_id ORDER BY b.name, s.number
/// ```
///
/// and of an exported songbook (it has a `SongBook` table, of one row):
///
/// ```sql
/// SELECT s.number, s.title, s.words, s.music, s.song_text, (SELECT title FROM SongBook LIMIT 1)
/// FROM Songs s ORDER BY s.number
/// ```
///
/// `Err(None)` is a database that is neither.
fn database_songs<R: Read + Seek>(source: R) -> Result<Vec<Fields>, Option<SongbookProblem>> {
    let unreadable = |why: String| Some(SongbookProblem::Unreadable(why));
    let limits = Limits { rows: MOST_SONGS, ..Limits::NORMAL };
    let mut db = Database::open(source, limits).map_err(unreadable)?;
    let (whole, one) = (db.has_table("Songbooks"), db.has_table("SongBook"));
    if !whole && !one {
        return Err(None);
    }
    let mut columns = vec!["number", "title", "words", "music", "song_text"];
    if whole {
        columns.push("songbook_id");
    }
    let songs = db.rows("Songs", &columns).map_err(unreadable)?;
    let books = if whole { db.rows("Songbooks", &["id", "name"]) } else { db.rows("SongBook", &["title"]) };
    let books = books.map_err(unreadable)?;
    // The songbooks' names by id, to find a song's by its songbook_id as `=` has it for two
    // columns of whole numbers: nothing equals nothing, and a number never equals text.
    let mut names_by_id: HashMap<Id, Vec<&Value>> = HashMap::new();
    for book in &books {
        if let ([id, name], true) = (book.values.as_slice(), whole)
            && let Some(id) = Id::of(id)
        {
            names_by_id.entry(id).or_default().push(name);
        }
    }
    let first_title = books.first().and_then(|book| book.values.first()).cloned().unwrap_or(Value::Null);
    let mut fields: Vec<Fields> = Vec::new();
    for song in songs {
        let mut values = song.values.into_iter();
        let mut next = || values.next().unwrap_or(Value::Null);
        let (number, title, words, music, text) = (next(), next(), next(), next(), next());
        if !whole {
            fields.push([number, title, words, music, text, first_title.clone()]);
            continue;
        }
        // Once with each songbook of that id (there is one, where ids are the rows' numbers),
        // or once with no songbook.
        let names = Id::of(&next()).and_then(|id| names_by_id.get(&id)).map_or(&[][..], Vec::as_slice);
        // Songbooks sharing an id (never, where ids are the rows' numbers) can't multiply the
        // songs past the limit.
        if fields.len() + names.len() > MOST_SONGS {
            return Err(unreadable("it has more songs than any songbook".to_owned()));
        }
        if names.is_empty() {
            fields.push([number, title, words, music, text, Value::Null]);
            continue;
        }
        for name in names {
            fields.push([number.clone(), title.clone(), words.clone(), music.clone(), text.clone(), (*name).clone()]);
        }
    }
    let text = |a: &str, b: &str| db.text_order(a, b);
    fields.sort_by(|[a_number, .., a_book], [b_number, .., b_book]| {
        let by_book = if whole { sort_order(a_book, b_book, &text) } else { Ordering::Equal };
        by_book.then_with(|| sort_order(a_number, b_number, &text))
    });
    Ok(fields)
}

/// What kind of block a title line starts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Verse,
    Chorus,
    Other,
}

const VERSE: [&str; 4] = ["Verse", "Куплет", "Strophe", "Verš"];
const CHORUS: [&str; 6] = ["Chorus", "Sbor", "Refrain", "Припев", "Приспів", "Refrén"];
const OTHER: [&str; 23] = [
    "Slide",
    "Слайд",
    "Dia",
    "Snímek",
    "Insert",
    "Вставка",
    "Einfügung",
    "Vložka",
    "Intro",
    "Вступление",
    "Вступ",
    "Einleitung",
    "Úvod",
    "Ending",
    "Окончание",
    "Закінчення",
    "Ende",
    "Závěr",
    "Konec",
    "Bridge",
    "Мост",
    "Бридж",
    "Міст",
];

/// One block of a song's text: what its title line says (its kind, and whether it is more of
/// the block before), its name, and its lines.
type Block<'a> = (Option<(Kind, bool)>, String, Vec<&'a str>);

/// The kind of block a line starts, and whether it is more of the one before (`&`).
fn title_line(line: &str) -> Option<(Kind, bool)> {
    let line = line.trim();
    let (more, name) = match line.strip_prefix('&') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, line),
    };
    // A title line is short: the name and perhaps a number.
    if name.chars().count() > 24 {
        return None;
    }
    let starts = |names: &[&str]| names.iter().any(|n| name.starts_with(n));
    let kind = if starts(&VERSE) {
        Kind::Verse
    } else if starts(&CHORUS) {
        Kind::Chorus
    } else if starts(&OTHER) {
        Kind::Other
    } else {
        return None;
    };
    Some((kind, more))
}

/// One SoftProjector song as a presentation. `number` is its number in its songbook, put
/// before the title so a song can still be found by it.
pub fn from_softprojector(number: Option<i64>, title: &str, words: &str, music: &str, text: &str) -> Presentation {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.lines().collect();
    // The blocks: each title line's kind, its name, and its words.
    let mut blocks: Vec<Block> = Vec::new();
    for line in &lines {
        match title_line(line) {
            Some(kind) => blocks.push((Some(kind), line.trim().trim_start_matches('&').trim().to_owned(), Vec::new())),
            None => match blocks.last_mut() {
                Some(block) => block.2.push(line),
                // Words before any title line are a slide of their own.
                None => blocks.push((None, String::new(), vec![line])),
            },
        }
    }
    let mut groups: Vec<Group> = Vec::new();
    // Each group's kind of part, for the order it is sung in.
    let mut parts: Vec<Part> = Vec::new();
    let mut chorus: Option<usize> = None;
    let mut last_verse: Option<usize> = None;
    let mut choruses = 0;
    for (kind, name, words) in blocks {
        let words = words.join("\n").trim().to_owned();
        if words.is_empty() {
            continue;
        }
        let slide = Slide::new(vec![Object::text(Role::primary(), words)]);
        match kind {
            // More of the verse or chorus before: another slide in its group.
            Some((Kind::Verse, true)) if last_verse.is_some() => {
                groups[last_verse.unwrap_or(0)].slides.push(slide);
            }
            Some((Kind::Chorus, true)) if chorus.is_some() => {
                groups[chorus.unwrap_or(0)].slides.push(slide);
            }
            Some((Kind::Chorus, _)) => {
                choruses += 1;
                let name = if name.is_empty() || (choruses > 1 && groups.iter().any(|g| g.name == name)) {
                    format!("{name} {choruses}").trim().to_owned()
                } else {
                    name
                };
                groups.push(new_group(&name, slide));
                parts.push(Part::Chorus);
                chorus = Some(groups.len() - 1);
            }
            Some((Kind::Verse, _)) => {
                groups.push(new_group(&name, slide));
                parts.push(Part::Verse);
                last_verse = Some(groups.len() - 1);
            }
            _ => {
                groups.push(new_group(&name, slide));
                parts.push(Part::Other);
            }
        }
    }
    // A chorus is sung where it is written, and then after each verse, as SoftProjector sings
    // it (`chorus_after.rs`).
    let order: Vec<Uuid> = sung_order(&parts).into_iter().map(|at| groups[at].id).collect();
    let plain_order = groups.iter().map(|g| g.id).collect::<Vec<_>>();
    let (arrangements, arrangement) = if order != plain_order {
        let name = CHORUS_AFTER_EACH_VERSE.to_owned();
        let arrangement = Arrangement { id: Uuid::new_v4(), name, groups: order, auto: true };
        let id = arrangement.id;
        (vec![arrangement], Some(id))
    } else {
        (Vec::new(), None)
    };
    let title = title.trim();
    let title = match number.filter(|n| *n > 0) {
        Some(n) if !title.is_empty() => format!("{n}. {title}"),
        Some(n) => n.to_string(),
        None if title.is_empty() => "Untitled".to_owned(),
        None => title.to_owned(),
    };
    let (words, music) = (words.trim(), music.trim());
    let author = match (words.is_empty(), music.is_empty()) {
        (false, true) => words.to_owned(),
        (true, false) => music.to_owned(),
        (false, false) if words == music => words.to_owned(),
        (false, false) => format!("{words} / {music}"),
        (true, true) => String::new(),
    };
    Presentation {
        id: Uuid::new_v4(),
        title,
        groups,
        copyright: Copyright { author, ..Copyright::default() },
        theme: None,
        arrangements,
        arrangement,
        loops: false,
        transition: None,
        catalog: None,
    }
}

fn new_group(name: &str, slide: Slide) -> Group {
    Group { id: Uuid::new_v4(), name: name.to_owned(), color: group_color(name), slides: vec![slide] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(song: &Presentation) -> Vec<(String, String)> {
        song.groups
            .iter()
            .flat_map(|g| {
                g.slides.iter().map(move |s| (g.name.clone(), s.text_for(&Role::primary()).unwrap_or("").to_owned()))
            })
            .collect()
    }

    /// Words written for this test, in the shape SoftProjector keeps them.
    const SONG: &str = "Куплет 1\nСвет над рекой\nПесня над тобой\n\nПрипев\nДомой, домой\n\nКуплет 2\nВечер настал\n\n&Куплет 2\nИ день прошёл\n\nОкончание\nАминь\n";

    #[test]
    fn blocks_become_groups_and_the_chorus_follows_every_verse() {
        let song = from_softprojector(Some(123), "Река", "Анна", "Борис", SONG);
        assert_eq!(song.title, "123. Река");
        assert_eq!(song.copyright.author, "Анна / Борис");
        let got = words(&song);
        assert_eq!(got[0], ("Куплет 1".to_owned(), "Свет над рекой\nПесня над тобой".to_owned()));
        assert_eq!(got[1], ("Припев".to_owned(), "Домой, домой".to_owned()));
        assert_eq!(song.groups[2].name, "Куплет 2");
        assert_eq!(song.groups[2].slides.len(), 2, "the & block is more of verse 2");
        let names: Vec<&str> = song.arrangements[0]
            .groups
            .iter()
            .map(|id| song.groups.iter().find(|g| g.id == *id).unwrap().name.as_str())
            .collect();
        assert_eq!(names, ["Куплет 1", "Припев", "Куплет 2", "Припев", "Окончание"]);
        assert_eq!(song.arrangement, Some(song.arrangements[0].id));
    }

    #[test]
    fn a_later_chorus_takes_over_as_softprojector_sings_it() {
        // Written for this test: a chorus at the top, two verses, a second chorus, a verse, an
        // insert and a third chorus (named as the first, so numbered), then a last verse.
        let text = "Припев\nДомой\n\nКуплет 1\nСвет\n\nКуплет 2\nВечер\n\nПрипев 1.2\nДомой опять\n\n\
            Куплет 3\nНочь\n\nВставка\nТихо\n\nПрипев\nВ конце\n\nКуплет 4\nУтро\n";
        let song = from_softprojector(None, "Дом", "", "", text);
        let arrangement = &song.arrangements[0];
        assert!(arrangement.auto, "it follows the song's parts");
        assert_eq!(arrangement.name, "Chorus after each verse");
        assert_eq!(song.arrangement, Some(arrangement.id));
        let names: Vec<&str> = arrangement
            .groups
            .iter()
            .map(|id| song.groups.iter().find(|g| g.id == *id).unwrap().name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "Припев",
                "Куплет 1",
                "Припев",
                "Куплет 2",
                "Припев 1.2",
                "Куплет 3",
                "Припев 1.2",
                "Вставка",
                "Припев 3",
                "Куплет 4",
                "Припев 3"
            ]
        );
    }

    #[test]
    fn a_song_without_titles_or_chorus_is_one_slide_and_no_arrangement() {
        let song = from_softprojector(None, "Words", "", "", "One line\nTwo line\n");
        assert_eq!(words(&song).len(), 1);
        assert!(song.arrangements.is_empty());
        assert_eq!(song.title, "Words");
    }

    /// What the importer made of SQLite's own answer, as JSON, when it asked the system's
    /// SQLite tool: a number as a number or as text that is one, the rest as text or nothing.
    fn from_sqlite_json(json: &str) -> Vec<SongbookSong> {
        let rows: Vec<serde_json::Value> = serde_json::from_str(json).unwrap();
        let field = |row: &serde_json::Value, key: &str| row.get(key).and_then(|v| v.as_str()).unwrap_or("").to_owned();
        rows.iter()
            .map(|row| SongbookSong {
                number: row.get("number").and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok())),
                title: field(row, "title"),
                words: field(row, "words"),
                music: field(row, "music"),
                text: field(row, "text"),
                book: Some(field(row, "book")).filter(|b| !b.trim().is_empty()),
            })
            .collect()
    }

    fn read(bytes: &[u8]) -> Result<Vec<SongbookSong>, SongbookProblem> {
        read_songbook(std::io::Cursor::new(bytes))
    }

    #[test]
    fn a_database_and_a_songbook_give_what_sqlite_gives_for_them() {
        // Each file with SQLite's own answer for it (`tests/songbooks/make.sh` made both).
        let made: [(&str, &[u8], &str); 5] = [
            (
                "database",
                include_bytes!("../tests/songbooks/database.sqlite"),
                include_str!("../tests/songbooks/database.json"),
            ),
            (
                "songbook",
                include_bytes!("../tests/songbooks/songbook.sps"),
                include_str!("../tests/songbooks/songbook.json"),
            ),
            (
                "utf16le",
                include_bytes!("../tests/songbooks/utf16le.sps"),
                include_str!("../tests/songbooks/utf16le.json"),
            ),
            (
                "utf16be",
                include_bytes!("../tests/songbooks/utf16be.sps"),
                include_str!("../tests/songbooks/utf16be.json"),
            ),
            // Songbooks sharing an id (a song comes once with each), ids that aren't whole
            // numbers, and numbers too large to be told apart as fractions.
            (
                "shared",
                include_bytes!("../tests/songbooks/shared.sqlite"),
                include_str!("../tests/songbooks/shared.json"),
            ),
        ];
        for (name, bytes, json) in made {
            let (ours, theirs) = (read(bytes).unwrap(), from_sqlite_json(json));
            assert_eq!(ours.len(), theirs.len(), "{name}");
            for (at, (our, their)) in ours.iter().zip(&theirs).enumerate() {
                assert_eq!(our, their, "{name}, song {at}");
            }
        }
        // A database: songs under their songbooks' names, a songbook without a name and a song
        // without a songbook under none, songbooks in the order of their names.
        let songs = read(include_bytes!("../tests/songbooks/database.sqlite")).unwrap();
        assert_eq!(songs.len(), 226);
        let mut books: Vec<Option<&str>> = songs.iter().map(|song| song.book.as_deref()).collect();
        books.dedup();
        assert_eq!(books, [None, Some("Evening Hymns"), Some("Вечерние песни"), Some("Ранкові пісні")]);
        // Songs with the same number stay in the order they were kept in.
        let sevens: Vec<&str> =
            songs.iter().filter(|song| song.number == Some(7)).map(|song| song.title.as_str()).collect();
        assert_eq!(sevens, ["Песня 7", "Ранок", "Ранок удруге"]);
        // An exported songbook: every song under the songbook's one title, by number.
        let songs = read(include_bytes!("../tests/songbooks/songbook.sps")).unwrap();
        assert!(songs.iter().all(|song| song.book.as_deref() == Some("Zpěvník pro zkoušku")));
        let numbers: Vec<Option<i64>> = songs.iter().map(|song| song.number).collect();
        assert_eq!(numbers, [None, Some(1), Some(2), Some(3), Some(3), Some(10), Some(20)]);
        let song =
            from_softprojector(songs[2].number, &songs[2].title, &songs[2].words, &songs[2].music, &songs[2].text);
        assert_eq!(song.title, "2. Druhá píseň");
        assert_eq!(song.copyright.author, "Jana / Petr");
        assert_eq!(words(&song)[1], ("Refrén".to_owned(), "Zpívej, zpívej".to_owned()));
    }

    #[test]
    fn a_file_that_is_no_songbook_says_so() {
        assert_eq!(read(b""), Err(SongbookProblem::NotSongbook));
        assert_eq!(read(b"Verse 1\nJust a song\n"), Err(SongbookProblem::NotSongbook));
        // An SQLite file of something else, one with no songs, and one that is damaged.
        let other = include_bytes!("../tests/songbooks/other.sqlite");
        assert_eq!(read(other), Err(SongbookProblem::NotSongbook));
        let empty = include_bytes!("../tests/songbooks/empty.sps");
        assert_eq!(read(empty), Err(SongbookProblem::NoSongs));
        let whole = include_bytes!("../tests/songbooks/songbook.sps");
        let cut = &whole[..5 * 1024];
        let why = "the file is cut short or damaged (a row is longer than the file)".to_owned();
        assert_eq!(read(cut), Err(SongbookProblem::Unreadable(why)));
        // A songbook whose songs have lost a column the import needs.
        let narrow = include_bytes!("../tests/songbooks/narrow.sps");
        assert!(matches!(read(narrow), Err(SongbookProblem::Unreadable(why)) if why.contains("no column song_text")));
    }

    /// A songbook kept as text, written for this test: a song a line, out of order, one with
    /// the extra parts later versions added, one whose number has a letter, and a line that is
    /// no song.
    const TEXT_SONGBOOK: &str = "##7\n##Пісні для перевірки\n##Написано для тестів@%другий рядок\n\
        2#$#Друга#$#0#$#D#$#Олена#$#Іван#$#Куплет 1@%Сонце встає@%над полем@$Приспів@%Радій, радій@$Куплет 2@%Вечір настав\n\
        12a#$#A letter in its number#$#0#$##$##$##$#Verse 1@%Twelve and a little more\n\
        1#$#Перша#$##$##$##$##$#Куплет 1 @%Ранок над полем \n\
        \n\
        not a song\n\
        10#$#The tenth#$#0#$#G#$#Ann#$#Ann#$#Verse 1.@%A lamp in the window@$Chorus@%Come home#$#Arial#$#0#$#sky.png#$#a note\r\n";

    #[test]
    fn a_songbook_kept_as_text_gives_its_songs_in_the_order_of_their_numbers() {
        let songs = read(TEXT_SONGBOOK.as_bytes()).unwrap();
        assert_eq!(songs.iter().map(|song| song.number).collect::<Vec<_>>(), [Some(1), Some(2), Some(10), None]);
        assert!(songs.iter().all(|song| song.book.as_deref() == Some("Пісні для перевірки")));
        let second = &songs[1];
        assert_eq!((second.title.as_str(), second.words.as_str(), second.music.as_str()), ("Друга", "Олена", "Іван"));
        assert_eq!(second.text, "Куплет 1\nСонце встає\nнад полем\n\nПриспів\nРадій, радій\n\nКуплет 2\nВечір настав");
        // Spaces at the ends of lines stay, as they do in the database of the same songbook.
        assert_eq!(songs[0].text, "Куплет 1 \nРанок над полем");
        // The parts after the text (a font, a picture, a note) are not the song's.
        assert_eq!(songs[2].text, "Verse 1.\nA lamp in the window\n\nChorus\nCome home");
        assert_eq!(songs[3].title, "A letter in its number");
        let song = from_softprojector(second.number, &second.title, &second.words, &second.music, &second.text);
        assert_eq!(song.title, "2. Друга");
        assert_eq!(
            song.groups.iter().map(|group| group.name.as_str()).collect::<Vec<_>>(),
            ["Куплет 1", "Приспів", "Куплет 2"]
        );
        assert_eq!(words(&song)[0].1, "Сонце встає\nнад полем");
        // With a byte-order mark and Windows line ends it reads the same.
        let windows = format!("\u{feff}{}", TEXT_SONGBOOK.replace('\n', "\r\n"));
        assert_eq!(read(windows.as_bytes()).unwrap(), songs);
        // A songbook with no songs, one saved as XML, and text that is no songbook.
        assert_eq!(read("##1\n##Empty\n##\n".as_bytes()), Err(SongbookProblem::NoSongs));
        assert_eq!(read(b"##"), Err(SongbookProblem::NoSongs));
        let xml = b"<?xml version=\"1.0\"?><spSongBook version=\"2.0\"></spSongBook>";
        assert!(matches!(read(xml), Err(SongbookProblem::Unreadable(why)) if why.contains("XML")));
        assert_eq!(read(b"#$#@$@%"), Err(SongbookProblem::NotSongbook));
        // Bytes that aren't text, and separators with nothing between, are read without harm.
        let odd = b"##\xff\xfe\n##\n##\n#$##$##$##$##$##$#\n1#$#\xff#$##$##$##$##$#@$@$@%@%\n";
        let songs = read(odd).unwrap();
        assert_eq!(songs.len(), 2);
        assert!(songs.iter().all(|song| song.text.is_empty() && song.book.is_none()));
    }

    #[test]
    fn numbers_of_every_kind_sort_in_one_order() {
        // Whole numbers, fractions, nothing written two ways, and numbers too large to tell
        // apart as fractions, many times over and mixed up: sorting needs one order of them.
        let numbers = [
            "0",
            "-0.0",
            "0.0",
            "9007199254740993",
            "9007199254740992.0",
            "9007199254740992",
            "-1",
            "1e30",
            "-1e30",
            "x",
            "",
            "2.5",
            "9223372036854775807",
            "9223372036854775808.0",
        ];
        let mut text = String::from("##1\n##Numbers\n##\n");
        let mut dice = 0x2545_f491_4f6c_dd1d_u64;
        for at in 0..600 {
            dice ^= dice << 13;
            dice ^= dice >> 7;
            dice ^= dice << 17;
            let number = numbers[(dice % numbers.len() as u64) as usize];
            text.push_str(&format!("{number}#$#Song {at}#$##$##$##$##$#Verse 1@%Line\n"));
        }
        let songs = read(text.as_bytes()).unwrap();
        assert_eq!(songs.len(), 600);
        let whole: Vec<i64> = songs.iter().filter_map(|song| song.number).collect();
        assert!(whole.windows(2).all(|pair| pair[0] <= pair[1]));
        let order = |a: f64, b: f64| sort_order(&Value::Real(a), &Value::Real(b), &|a: &str, b: &str| a.cmp(b));
        assert_eq!(order(-0.0, 0.0), Ordering::Equal);
        assert_eq!(whole_and_fraction(0, -0.0), Ordering::Equal);
        assert_eq!(whole_and_fraction(9_007_199_254_740_993, 9_007_199_254_740_992.0), Ordering::Greater);
        assert_eq!(whole_and_fraction(i64::MAX, 9_223_372_036_854_775_808.0), Ordering::Less);
        assert_eq!(whole_and_fraction(i64::MIN, -9_223_372_036_854_775_808.0), Ordering::Equal);
        assert_eq!(whole_and_fraction(2, 2.5), Ordering::Less);
        assert_eq!(whole_and_fraction(-2, -2.5), Ordering::Greater);
    }

    /// Writes the songs real songbooks give as JSON, to compare with what another program
    /// makes of the same files. Testing aid: `SOFTPRESENTER_SONGBOOK_FILES` names the songbooks
    /// (separated as in PATH) and `SOFTPRESENTER_SONGBOOK_DUMP` the folder the JSON goes in,
    /// one file a songbook; run with
    /// `cargo test -p sp-model the_songs_of_real_songbooks -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs songbooks to read"]
    fn the_songs_of_real_songbooks_written_out() {
        let files = std::env::var_os("SOFTPRESENTER_SONGBOOK_FILES").expect("SOFTPRESENTER_SONGBOOK_FILES names them");
        let folder = std::path::PathBuf::from(std::env::var_os("SOFTPRESENTER_SONGBOOK_DUMP").expect("a folder"));
        for (at, path) in std::env::split_paths(&files).enumerate() {
            let songs = read_songbook(std::fs::File::open(&path).unwrap()).unwrap();
            let rows: Vec<serde_json::Value> = songs
                .iter()
                .map(|song| serde_json::json!([song.number, song.title, song.words, song.music, song.text, song.book]))
                .collect();
            std::fs::write(folder.join(format!("{at}.json")), serde_json::to_vec(&rows).unwrap()).unwrap();
            println!("{at}: {} songs from {}", songs.len(), path.display());
        }
    }

    /// Compares what real files give with what the system's SQLite gives for the importer's
    /// two questions. Testing aid: `SOFTPRESENTER_SQLITE_FILES` names the files (separated as
    /// in PATH); run on a Mac with
    /// `cargo test -p sp-model the_systems_sqlite -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs files to compare, and /usr/bin/sqlite3"]
    fn every_song_is_what_the_systems_sqlite_gives() {
        let files = std::env::var_os("SOFTPRESENTER_SQLITE_FILES").expect("SOFTPRESENTER_SQLITE_FILES names the files");
        for path in std::env::split_paths(&files) {
            let ask = |query: &str| {
                let output = std::process::Command::new("/usr/bin/sqlite3")
                    .args(["-readonly", "-json"])
                    .arg(&path)
                    .arg(query)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
                String::from_utf8_lossy(&output.stdout).into_owned()
            };
            let tables = ask("SELECT name FROM sqlite_master WHERE type = 'table'");
            let query = if tables.contains("\"Songbooks\"") {
                "SELECT s.number AS number, s.title AS title, s.words AS words, s.music AS music, s.song_text AS text, \
                 b.name AS book FROM Songs s LEFT JOIN Songbooks b ON b.id = s.songbook_id ORDER BY b.name, s.number"
            } else {
                "SELECT s.number AS number, s.title AS title, s.words AS words, s.music AS music, s.song_text AS text, \
                 (SELECT title FROM SongBook LIMIT 1) AS book FROM Songs s ORDER BY s.number"
            };
            let theirs = from_sqlite_json(&ask(query));
            let ours = read_songbook(std::fs::File::open(&path).unwrap()).unwrap();
            assert_eq!(ours.len(), theirs.len(), "{}", path.display());
            for (at, (our, their)) in ours.iter().zip(&theirs).enumerate() {
                assert_eq!(our, their, "{}, song {at}", path.display());
            }
            println!(
                "{}: {} songs the same, in the same order",
                path.file_name().unwrap().to_string_lossy(),
                ours.len()
            );
        }
    }
}
