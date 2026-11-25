//! The Bible's part of the find box in the bar along the top: a passage when what is typed is a
//! reference, and verses holding the words when it is not; and the Bible view's Search, which
//! lists verses holding the words as they are typed.
//!
//! Reading a Bible for the first time and searching all its verses take too long for the thread
//! that draws the window, so both are done on a thread of the find box's own. The window asks
//! with what is typed and takes the answer when it comes; meanwhile it shows the songs found.
//! The reference is read, and the verses searched, by `sp-bible`, as the Bible view does.

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use sp_bible::Bible;

use crate::bible::BibleReader;
use crate::bible_language::Reading;
use crate::i18n::{t, tf};

/// How many verses a search for words lists.
pub const VERSES_LISTED: usize = 3;
/// The fewest letters typed before verses are searched for them.
const LEAST_LETTERS: usize = 3;

/// One Bible result: a passage, or a verse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibleRow {
    /// The passage's or the verse's name as the operator reads it: the book in the language
    /// typed (for a verse, of the Bible it was found in), numbered as that language numbers.
    pub title: String,
    /// What it is, at the right of its row, in capitals: `BIBLE`, or `BIBLE · KJV`.
    pub tag: String,
    /// The words of its first verse, from the Bible the operator reads it in.
    pub line: String,
    /// What the Bible view looks up for it: its name as the first Bible writes and numbers it.
    pub lookup: String,
    /// What a playlist keeps for it, as the Bible view's own Add keeps a passage: named in the
    /// language it was typed in (a verse found, in its Bible's language) with that numbering.
    pub kept: String,
    pub numbering: sp_bible::Numbering,
    /// The language the operator reads it in (`en`, `ru`, `uk`…), for the Bible view to go on
    /// in when it is opened; `None` when the words said nothing of one.
    pub language: Option<String>,
    /// A passage named by a reference, not a verse found by its words.
    pub passage: bool,
}

/// True when what is typed could name a passage: a book, then a number. (`1 John` alone is a
/// book, and a word alone is searched for; neither is offered as a whole book.)
fn names_a_passage(typed: &str) -> bool {
    let letters = typed.find(char::is_alphabetic);
    letters.is_some_and(|at| typed[at..].contains(|c: char| c.is_ascii_digit()))
}

/// What the Bibles in use have for what is typed, in the order they are listed (`bibles` in
/// the translations' order, the first being the one the slides are numbered by). `reading` is
/// how the operator's language is decided: the session's when the words say nothing of one, or
/// the language chosen on the Bible page.
///
/// A reference gives its passage, read as the Bible view reads it (so it is the passage its
/// slides will be: its numbers in the numbering of the language typed), named in the language
/// it was typed in, numbered as it numbers, and quoted from the first Bible in use of that
/// language (the first Bible when that language has none). Anything else of three
/// letters or more is searched for as words in the Bibles written in their alphabet (all of
/// them, in order, when none is): the first verses holding them, from the first Bible that has
/// any, then the next, up to [`VERSES_LISTED`] in all. With a language chosen, each row is
/// named and quoted in it (and tagged with the Bible quoted), whatever Bible found it.
pub fn search_bibles(typed: &str, bibles: &[(String, Arc<Bible>)], reading: &Reading) -> Vec<BibleRow> {
    use crate::bible_language::{language_of, reading_typed, search_order};
    let session = reading.session.as_str();
    let typed = typed.trim();
    let Some((_, lead)) = bibles.first() else { return Vec::new() };
    let languages: Vec<String> = bibles.iter().map(|(_, bible)| language_of(&bible.info.language)).collect();
    let in_use: Vec<&Bible> = bibles.iter().map(|(_, bible)| bible.as_ref()).collect();
    if names_a_passage(typed)
        && let Ok(reference) = sp_bible::read_reference(typed, &lead.info.language)
    {
        // Read in the numbering of the language it was typed in, whatever the language chosen
        // (see `bible::numbering_of`), as the first Bible numbers it.
        let typed_in = crate::bible_language::typed_bible(&reference.languages, session, &languages).1;
        let numbering = typed_in.as_deref().map_or(lead.info.numbering, |in_| crate::bible::numbering_of(in_, &in_use));
        let range = crate::bible::renumber(reference.range, numbering, lead.info.numbering, &in_use);
        let verses = lead.verses(range);
        let (Some((first, words)), Some((last, _))) = (verses.first(), verses.last()) else { return Vec::new() };
        let (reader, language) = reading_typed(&reference.languages, reading, &languages);
        let reader_bible = &bibles[reader].1;
        // The operator's words: the same verse in their Bible, as the slides line it up.
        let there = sp_bible::convert(*first, lead.info.numbering, reader_bible.info.numbering);
        let line = reader_bible.verse(there).unwrap_or(words);
        let lookup = lead.label(*first, *last);
        let named =
            |language: &str, read: bool| crate::bible::named_in(&lookup, lead.info.numbering, language, &in_use, read);
        let kept = typed_in.as_deref().map_or_else(|| lookup.clone(), |in_| named(in_, false));
        return vec![BibleRow {
            title: named(&languages[reader], true),
            tag: t("notices.find.bible_tag", "BIBLE").to_owned(),
            line: line.to_owned(),
            kept,
            numbering,
            lookup,
            language,
            passage: true,
        }];
    }
    let mut rows = Vec::new();
    if typed.chars().count() < LEAST_LETTERS {
        return rows;
    }
    let order = search_order(typed, session, &languages).unwrap_or_else(|| (0..bibles.len()).collect());
    for at in order {
        let (short, bible) = &bibles[at];
        for (place, words) in bible.search(typed, VERSES_LISTED - rows.len()) {
            // Opened as the first Bible numbers it, which the slides follow.
            let there = sp_bible::convert(place, bible.info.numbering, lead.info.numbering);
            let (quoted, (short, line)) = in_chosen(reading, &languages, bibles, there, at, (short, words));
            // Named as the Bible quoted numbers it; kept as the Bible it was found in has it.
            let named = sp_bible::convert(there, lead.info.numbering, bibles[quoted].1.info.numbering);
            rows.push(BibleRow {
                title: sp_bible::reading_label(&languages[quoted], named, named),
                tag: tf("notices.find.bible_tag.from", "BIBLE \u{b7} {bible}", &[("bible", &short.to_uppercase())]),
                line: line.to_owned(),
                lookup: lead.label(there, there),
                kept: sp_bible::label_in(&languages[at], place, place),
                numbering: bible.info.numbering,
                language: Some(languages[at].clone()).filter(|language| !language.is_empty()),
                passage: false,
            });
        }
        if rows.len() >= VERSES_LISTED {
            break;
        }
    }
    rows
}

/// How many verses the Bible view's Search lists.
pub const PAGE_VERSES: usize = 60;

/// What the Bible view's Search finds for some words: the verses, and what to say when there
/// are none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageFound {
    pub verses: Vec<crate::bible::FoundVerse>,
    pub message: Option<String>,
}

/// The verse found in the Bible at `found`, as the operator reads it: from the Bible in use of the
/// language chosen, when one is (its place, its short name and its words for the verse, `there`
/// in the first Bible's numbers), else as found.
fn in_chosen<'a>(
    reading: &Reading,
    languages: &[String],
    bibles: &'a [(String, Arc<Bible>)],
    there: sp_bible::VerseRef,
    found: usize,
    as_found: (&'a str, &'a str),
) -> (usize, (&'a str, &'a str)) {
    let Some(chosen) = reading.chosen_in(languages).filter(|chosen| *chosen != found) else { return (found, as_found) };
    let ((short, bible), lead) = (&bibles[chosen], &bibles[0].1);
    let place = sp_bible::convert(there, lead.info.numbering, bible.info.numbering);
    match bible.verse(place) {
        Some(words) => (chosen, (short.as_str(), words)),
        None => (found, as_found),
    }
}

/// The verses holding the words typed in the Bible view (`bibles` in the translations' order,
/// the first numbering the slides; `reading` how the operator's language is decided): in the Bibles
/// in use written in the words' alphabet (the session's language first), or, with none, in the
/// first translation, up to [`PAGE_VERSES`]. Each verse is named in the language of the Bible
/// it was found in (or in the language chosen, quoted from its Bible), with the numbers of the
/// Bible quoted; it is opened as the first translation numbers it.
pub fn search_page(typed: &str, bibles: &[(String, Arc<Bible>)], reading: &Reading) -> PageFound {
    use crate::bible_language::{language_of, search_order};
    let session = reading.session.as_str();
    let mut found = PageFound::default();
    let typed = typed.trim();
    let Some((_, lead)) = bibles.first() else { return found };
    if typed.is_empty() {
        return found;
    }
    let languages: Vec<String> = bibles.iter().map(|(_, bible)| language_of(&bible.info.language)).collect();
    let order = search_order(typed, session, &languages).unwrap_or_else(|| vec![0]);
    let mut searched = Vec::new();
    for at in order {
        if found.verses.len() >= PAGE_VERSES {
            break;
        }
        let (_, bible) = &bibles[at];
        searched.push(bible.info.name.clone());
        let language = &languages[at];
        for (place, words) in bible.search(typed, PAGE_VERSES - found.verses.len()) {
            // The verse as the first translation numbers it, which the slides follow.
            let there = sp_bible::convert(place, bible.info.numbering, lead.info.numbering);
            let as_found = (bible.info.short.as_str(), words);
            let (quoted, (short, words)) = in_chosen(reading, &languages, bibles, there, at, as_found);
            // Named as the Bible quoted numbers it.
            let (named, at_quoted) =
                (&languages[quoted], sp_bible::convert(there, lead.info.numbering, bibles[quoted].1.info.numbering));
            found.verses.push(crate::bible::FoundVerse {
                reference: sp_bible::reading_label(named, at_quoted, at_quoted),
                chapter: format!("{} {}", sp_bible::reading_book_name(at_quoted.book, named, true), at_quoted.chapter),
                words: words.to_owned(),
                bible: short.to_owned(),
                language: language.clone(),
                kept: lead.label(there, there),
                kept_chapter: format!("{} {}", lead.book_name(there.book), there.chapter),
            });
        }
    }
    if found.verses.is_empty() {
        found.message = Some(match searched.as_slice() {
            [one] => tf("notices.find.nothing_in", "Nothing in the {bible} has those words.", &[("bible", one)]),
            _ => t("notices.find.nothing", "None of the Bibles in use has those words.").to_owned(),
        });
    }
    found
}

/// What was asked: the words typed, which Bibles (by their ids, in order), and how the
/// operator's language is decided (the session's, and a language chosen).
type Asked = (String, Vec<String>, Reading);

/// How a finder answers: what is typed, in the Bibles read, in the operator's language.
type Search<T> = fn(&str, &[(String, Arc<Bible>)], &Reading) -> T;

/// An answer from a finder's thread.
struct Answer<T> {
    asked: Asked,
    rows: T,
    /// Bibles read for the first time on the way, for the shelf to keep.
    read: Vec<(String, Arc<Bible>)>,
}

struct Job<T> {
    asked: Asked,
    bibles: Vec<BibleReader>,
    wake: egui::Context,
    search: Search<T>,
}

/// Reads the Bibles a job names and searches them.
fn answer<T>(job: Job<T>) -> Answer<T> {
    let mut read = Vec::new();
    let mut bibles = Vec::new();
    for reader in job.bibles {
        let (id, short, fresh) = (reader.id.clone(), reader.short.clone(), reader.loaded.is_none());
        if let Ok(bible) = reader.read() {
            if fresh {
                read.push((id, bible.clone()));
            }
            bibles.push((short, bible));
        }
    }
    Answer { rows: (job.search)(&job.asked.0, &bibles, &job.asked.2), asked: job.asked, read }
}

/// Bible results found on a thread of their own, asked for as the words change and taken when
/// they come: the find box's (by default), or the Bible view's Search's (`Finder::new` with
/// [`search_page`]).
pub struct Finder<T = Vec<BibleRow>> {
    /// What was last asked for.
    asked: Option<Asked>,
    /// The latest answer, once the window has taken it.
    have: Option<Answer<T>>,
    arrived: Arc<Mutex<Option<Answer<T>>>>,
    jobs: Option<Sender<Job<T>>>,
    search: Search<T>,
}

impl Default for Finder {
    fn default() -> Finder {
        Finder::new(search_bibles)
    }
}

impl Finder {
    /// The find box's Bible results for what is typed (see [`Finder::answer`]).
    pub fn rows(
        &mut self,
        typed: &str,
        in_use: &[String],
        reading: &Reading,
        bibles: impl FnOnce() -> Vec<BibleReader>,
        wake: &egui::Context,
    ) -> Option<&[BibleRow]> {
        self.answer(typed, in_use, reading, bibles, wake).map(Vec::as_slice)
    }
}

impl<T: Send + 'static> Finder<T> {
    /// A finder that answers with `search`.
    pub fn new(search: Search<T>) -> Finder<T> {
        Finder { asked: None, have: None, arrived: Arc::new(Mutex::new(None)), jobs: None, search }
    }

    /// The results for what is typed, or `None` while they are still being found. Asks for
    /// them when the words, the Bibles in use or the operator's language have changed (`bibles`
    /// is called only then). Never waits.
    pub fn answer(
        &mut self,
        typed: &str,
        in_use: &[String],
        reading: &Reading,
        bibles: impl FnOnce() -> Vec<BibleReader>,
        wake: &egui::Context,
    ) -> Option<&T> {
        let current = |asked: &Asked| asked.0 == typed && asked.1 == in_use && asked.2 == *reading;
        if !self.asked.as_ref().is_some_and(current) {
            let asked: Asked = (typed.to_owned(), in_use.to_vec(), reading.clone());
            self.asked = Some(asked.clone());
            let job = Job { asked, bibles: bibles(), wake: wake.clone(), search: self.search };
            if cfg!(test) {
                // Tests have their answer at once, on their own thread.
                self.have = Some(answer(job));
            } else {
                self.send(job);
            }
        }
        if let Some(answer) = self.arrived.lock().ok().and_then(|mut arrived| arrived.take()) {
            self.have = Some(answer);
        }
        self.have.as_ref().filter(|answer| current(&answer.asked)).map(|answer| &answer.rows)
    }

    /// The Bibles read for the first time by the last answer, for the shelf to keep so they are
    /// not read again.
    pub fn take_read(&mut self) -> Vec<(String, Arc<Bible>)> {
        self.have.as_mut().map(|answer| std::mem::take(&mut answer.read)).unwrap_or_default()
    }

    /// Hands a job to the thread, starting it the first time. The thread sleeps between jobs,
    /// answers only the newest of those waiting, and ends when the window does.
    fn send(&mut self, job: Job<T>) {
        if self.jobs.is_none() {
            let (jobs, waiting) = channel::<Job<T>>();
            let arrived = self.arrived.clone();
            let spawned = std::thread::Builder::new().name("find".to_owned()).spawn(move || {
                while let Ok(mut job) = waiting.recv() {
                    // What was typed since is all that matters.
                    while let Ok(newer) = waiting.try_recv() {
                        job = newer;
                    }
                    let wake = job.wake.clone();
                    let found = answer(job);
                    if let Ok(mut arrived) = arrived.lock() {
                        *arrived = Some(found);
                    }
                    wake.request_repaint();
                }
            });
            if spawned.is_ok() {
                self.jobs = Some(jobs);
            }
        }
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Bibles that come with the app, read.
    fn shipped() -> Vec<(String, Arc<Bible>)> {
        let mut shelf = crate::bible::Shelf::shipped();
        (0..shelf.bibles.len())
            .map(|at| (shelf.bibles[at].short.clone(), shelf.get(at).expect("a shipped Bible reads")))
            .collect()
    }

    /// The shipped Bibles named, in that order.
    fn these(names: &[&str]) -> Vec<(String, Arc<Bible>)> {
        let all = shipped();
        names.iter().map(|name| all.iter().find(|(short, _)| short == name).expect("shipped").clone()).collect()
    }

    /// A Bible made for a test, with words written for it.
    fn made(short: &str, language: &str, lines: &str) -> (String, Arc<Bible>) {
        let info = sp_bible::Info {
            id: short.to_lowercase(),
            name: format!("Test {short}"),
            short: short.to_owned(),
            language: language.to_owned(),
            terms: "Written for a test".to_owned(),
            credit: String::new(),
            source: "a test".to_owned(),
            numbering: sp_bible::Numbering::English,
            loose: false,
        };
        (short.to_owned(), Arc::new(Bible::from_verse_lines(info, lines, str::to_owned)))
    }

    #[test]
    fn a_reference_gives_its_passage_named_in_the_language_typed() {
        // The King James Version first, as the shipped Bibles are listed.
        let bibles = shipped();
        for typed in ["John 3:16-18", "jn 3:16-18"] {
            let rows = search_bibles(typed, &bibles, &Reading::auto("en"));
            assert_eq!(rows.len(), 1, "{typed}");
            assert_eq!((rows[0].title.as_str(), rows[0].tag.as_str()), ("John 3:16-18", "BIBLE"), "{typed}");
            assert_eq!((rows[0].lookup.as_str(), rows[0].language.as_deref()), ("John 3:16-18", Some("en")));
            assert!(rows[0].line.starts_with("For God so loved the world"), "{}", rows[0].line);
        }
        // Typed in Russian: named and quoted in Russian; looked up as the first Bible names it.
        let russian = search_bibles("Ин 3:16-18", &bibles, &Reading::auto("en"));
        assert_eq!((russian[0].title.as_str(), russian[0].lookup.as_str()), ("От Иоанна 3:16-18", "John 3:16-18"));
        assert!(russian[0].line.starts_with("Ибо так возлюбил"), "{}", russian[0].line);
        assert_eq!(russian[0].language.as_deref(), Some("ru"));
        // Typed in Ukrainian with no Ukrainian Bible in use: the first Bible.
        let ukrainian = search_bibles("Ів 3:16-18", &bibles, &Reading::auto("en"));
        assert_eq!(ukrainian[0].title, "John 3:16-18");
        assert!(ukrainian[0].line.starts_with("For God so loved"));
        // A chapter alone is the whole chapter.
        let psalm = search_bibles("ps 23", &bibles, &Reading::auto("en"));
        assert_eq!(psalm.len(), 1);
        assert!(psalm[0].title.ends_with("23:1-6"), "{}", psalm[0].title);
        // A book alone, or a book that is a number and a name, is not offered whole.
        assert!(search_bibles("john", &bibles, &Reading::auto("en")).iter().all(|row| !row.passage));
        assert!(search_bibles("1 john", &bibles, &Reading::auto("en")).iter().all(|row| !row.passage));
    }

    #[test]
    fn a_single_verse_is_looked_up_alone_found_by_a_reference_or_by_its_words() {
        // The Bible page then makes its slides its chapter's (see `bible::page_deck`); what is
        // looked up, and what a playlist keeps, is the verse.
        let bibles = these(&["KJV", "SYN"]);
        let verse = &search_bibles("gen 1 4", &bibles, &Reading::auto("en"))[0];
        assert_eq!(verse.lookup, "Genesis 1:4");
        assert_eq!(search_bibles("jn 3:16-18", &bibles, &Reading::auto("en"))[0].lookup, "John 3:16-18");
        // A verse found by its words: as the first Bible numbers it.
        let found = search_bibles("valley of the shadow of death", &bibles, &Reading::auto("en"));
        assert!(found.iter().any(|row| row.lookup == "Psalms 23:4" && !row.passage), "{found:?}");
        // With the Synodal first, its numbers and names.
        let russian_first = these(&["SYN", "KJV"]);
        assert_eq!(search_bibles("Быт 1 4", &russian_first, &Reading::auto("ru"))[0].lookup, "Бытие 1:4");
    }

    #[test]
    fn with_russian_first_an_english_reference_is_read_in_english() {
        let bibles = these(&["SYN", "KJV"]);
        let genesis = search_bibles("gen 1 1", &bibles, &Reading::auto("ru"));
        assert_eq!((genesis[0].title.as_str(), genesis[0].lookup.as_str()), ("Genesis 1:1", "Бытие 1:1"));
        assert!(genesis[0].line.starts_with("In the beginning"), "{}", genesis[0].line);
        let russian = search_bibles("Быт 1 1", &bibles, &Reading::auto("en"));
        assert_eq!((russian[0].title.as_str(), russian[0].lookup.as_str()), ("Бытие 1:1", "Бытие 1:1"));
        assert!(russian[0].line.starts_with("В начале"));
        // Numbers follow the language typed: "Psalm 23" in English is the English 23rd psalm,
        // the Synodal 22nd, kept as it was typed with the English numbering beside it.
        let psalm = search_bibles("Psalm 23:1", &bibles, &Reading::auto("en"));
        assert_eq!((psalm[0].title.as_str(), psalm[0].lookup.as_str()), ("Psalm 23:1", "Псалтирь 22:1"));
        assert!(psalm[0].line.contains("The LORD is my shepherd"), "{}", psalm[0].line);
        assert_eq!((psalm[0].kept.as_str(), psalm[0].numbering), ("Psalms 23:1", sp_bible::Numbering::English));
        // Typed in Russian: the Synodal's numbers, the same verse.
        let russian = search_bibles("Пс 22:1", &bibles, &Reading::auto("ru"));
        assert_eq!((russian[0].title.as_str(), russian[0].lookup.as_str()), ("Псалом 22:1", "Псалтирь 22:1"));
    }

    #[test]
    fn a_name_spelt_alike_in_russian_and_ukrainian_follows_the_session() {
        let russian = made("RT", "ru", "PSA 23:1 Слова русского теста\nJOH 3:16 Русский тест");
        let ukrainian = made("UT", "uk", "PSA 23:1 Слова українського тесту\nJOH 3:16 Український тест");
        let bibles = vec![russian, ukrainian];
        assert!(search_bibles("Пс 23:1", &bibles, &Reading::auto("uk"))[0].line.contains("українського"));
        assert!(search_bibles("Пс 23:1", &bibles, &Reading::auto("ru"))[0].line.contains("русского"));
        // Neither: the first in the translations; named in its language.
        let row = &search_bibles("Пс 23:1", &bibles, &Reading::auto("en"))[0];
        assert_eq!((row.title.as_str(), row.language.as_deref()), ("Псалом 23:1", Some("ru")));
        // Clearly Ukrainian: the Ukrainian Bible, whatever the session.
        let row = &search_bibles("Ів 3:16", &bibles, &Reading::auto("ru"))[0];
        assert_eq!((row.title.as_str(), row.line.as_str()), ("Від Івана 3:16", "Український тест"));
    }

    #[test]
    fn words_are_searched_for_in_the_bibles_of_their_alphabet() {
        // The Russian Synodal first: English words are looked for in the English Bible.
        let bibles = these(&["SYN", "KJV"]);
        let rows = search_bibles("valley of the shadow", &bibles, &Reading::auto("ru"));
        assert!(!rows.is_empty() && rows.len() <= VERSES_LISTED);
        assert!(rows.iter().all(|row| row.tag == "BIBLE \u{b7} KJV" && !row.passage), "{rows:?}");
        // Verses holding the words together come before those holding them apart.
        assert!(rows[0].line.to_lowercase().contains("valley of the shadow"), "{rows:?}");
        // Named in English with the numbers of the Bible it was found in, and opened as the first
        // Bible names it: the English 23rd psalm is the Synodal 22nd.
        let psalm = rows.iter().find(|row| row.line.contains("valley of the shadow of death")).expect("Psalm 23:4");
        assert_eq!((psalm.title.as_str(), psalm.lookup.as_str()), ("Psalm 23:4", "Псалтирь 22:4"));
        assert_eq!(psalm.language.as_deref(), Some("en"));
        // Russian words, in the Russian Bible.
        let russian = search_bibles("долиною смертной тени", &bibles, &Reading::auto("en"));
        assert!(!russian.is_empty() && russian.iter().all(|row| row.tag == "BIBLE \u{b7} SYN"), "{russian:?}");
        assert_eq!(russian[0].language.as_deref(), Some("ru"));
        // Fewer than three letters, or nothing any Bible has, finds nothing.
        assert!(search_bibles("of", &bibles, &Reading::auto("en")).is_empty());
        assert!(search_bibles("zzzqqqxxx", &bibles, &Reading::auto("en")).is_empty());
        assert!(search_bibles("valley", &[], &Reading::auto("en")).is_empty());
    }

    #[test]
    fn cyrillic_words_are_looked_for_in_the_sessions_bible_first_and_then_the_other() {
        let russian = made("RT", "ru", "GEN 1:1 любов в начале\nGEN 1:2 любов вторая");
        let ukrainian = made("UT", "uk", "GEN 1:1 любов на початку\nGEN 1:2 любов друга");
        let english = made("ET", "en", "GEN 1:1 love at first");
        let bibles = vec![russian, english, ukrainian];
        let tags = |session: &str| -> Vec<String> {
            search_bibles("любов", &bibles, &Reading::auto(session)).into_iter().map(|row| row.tag).collect()
        };
        assert_eq!(tags("ru"), ["BIBLE \u{b7} RT", "BIBLE \u{b7} RT", "BIBLE \u{b7} UT"]);
        assert_eq!(tags("uk"), ["BIBLE \u{b7} UT", "BIBLE \u{b7} UT", "BIBLE \u{b7} RT"]);
        // No Bible in the words' alphabet: all of them, in the translations' order, as before.
        let only_russian = vec![made("RT", "ru", "GEN 1:1 Аминь, amen")];
        assert_eq!(search_bibles("amen", &only_russian, &Reading::auto("en")).len(), 1);
    }

    #[test]
    fn a_language_chosen_names_and_quotes_every_row_in_it() {
        let bibles = these(&["SYN", "KJV"]);
        let english = Reading { session: "ru".to_owned(), chosen: Some("en".to_owned()) };
        // A reference typed in Russian (the Synodal's numbers), read in English with the English
        // numbers: never a Russian number under an English name.
        let psalm = &search_bibles("Пс 22:1", &bibles, &english)[0];
        assert_eq!((psalm.title.as_str(), psalm.lookup.as_str()), ("Psalm 23:1", "Псалтирь 22:1"));
        assert!(psalm.line.contains("The LORD is my shepherd"), "{}", psalm.line);
        assert_eq!(psalm.language.as_deref(), Some("ru"), "the session goes on in what was typed");
        // Verses found by Russian words: named and quoted in English, tagged with the KJV.
        let rows = search_bibles("долиною смертной тени", &bibles, &english);
        assert!(!rows.is_empty() && rows.iter().all(|row| row.tag == "BIBLE \u{b7} KJV"), "{rows:?}");
        let found = rows.iter().find(|row| row.lookup == "Псалтирь 22:4").expect("the 22nd psalm's fourth verse");
        assert_eq!(found.title, "Psalm 23:4");
        assert!(found.line.contains("valley of the shadow of death"), "{}", found.line);
        // The Bible page's own list, the same way.
        let page = search_page("долиною смертной тени", &bibles, &english);
        let verse = page.verses.iter().find(|verse| verse.kept == "Псалтирь 22:4").expect("found");
        assert_eq!((verse.reference.as_str(), verse.bible.as_str()), ("Psalm 23:4", "KJV"));
        assert_eq!(verse.chapter, "Psalm 23");
        assert!(verse.words.contains("valley of the shadow of death"));
        // Auto as before.
        assert_eq!(search_page("долиною смертной тени", &bibles, &Reading::auto("en")).verses[0].bible, "SYN");
    }

    /// Not a check: says how long a one-word search of the Bibles that come with the app takes.
    /// `cargo test --release -p sp-app find::tests::how_long -- --ignored --nocapture`
    #[test]
    #[ignore = "a measurement, not a check"]
    fn how_long_a_search_takes() {
        let started = std::time::Instant::now();
        let bibles = shipped();
        eprintln!("reading {} Bibles: {:?}", bibles.len(), started.elapsed());
        for typed in ["shepherd", "zzzqqqxxx", "the", "John 3:16"] {
            let started = std::time::Instant::now();
            let rows = search_bibles(typed, &bibles, &Reading::auto("en"));
            eprintln!("{typed:?}: {} rows in {:?}", rows.len(), started.elapsed());
        }
    }
}
