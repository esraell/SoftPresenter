//! The Bible's part of the find box in the bar along the top: a passage when what is typed is a
//! reference, and verses holding the words when it is not.
//!
//! Reading a Bible for the first time and searching all its verses take too long for the thread
//! that draws the window, so both are done on a thread of the find box's own. The window asks
//! with what is typed and takes the answer when it comes; meanwhile it shows the songs found.
//! The reference is read, and the verses searched, by `sp-bible`, as the Bible view does.

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use sp_bible::Bible;

use crate::bible::BibleReader;

/// How many verses a search for words lists.
pub const VERSES_LISTED: usize = 3;
/// The fewest letters typed before verses are searched for them.
const LEAST_LETTERS: usize = 3;

/// One Bible result: a passage, or a verse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibleRow {
    /// The passage's or the verse's name, as its Bible writes it: `John 3:16-18`.
    pub title: String,
    /// What it is, at the right of its row, in capitals: `BIBLE`, or `BIBLE · KJV`.
    pub tag: String,
    /// The words of its first verse.
    pub line: String,
    /// What to type in Scripture Lookup to get it.
    pub lookup: String,
    /// A passage named by a reference, not a verse found by its words.
    pub passage: bool,
}

/// True when what is typed could name a passage: a book, then a number. (`1 John` alone is a
/// book, and a word alone is searched for; neither is offered as a whole book.)
fn names_a_passage(typed: &str) -> bool {
    let letters = typed.find(char::is_alphabetic);
    letters.is_some_and(|at| typed[at..].contains(|c: char| c.is_ascii_digit()))
}

/// What the Bibles in use have for what is typed, in the order they are listed. A reference
/// gives its passage, named and quoted from the first Bible. Anything else of three letters or
/// more is searched for as words: the first verses holding them, from the first Bible that has
/// any, then the next, up to [`VERSES_LISTED`] in all.
pub fn search_bibles(typed: &str, bibles: &[(String, Arc<Bible>)]) -> Vec<BibleRow> {
    let typed = typed.trim();
    let Some((_, lead)) = bibles.first() else { return Vec::new() };
    if names_a_passage(typed)
        && let Ok(range) = sp_bible::parse_reference_in(typed, &lead.info.language)
    {
        let verses = lead.verses(range);
        return match (verses.first(), verses.last()) {
            (Some((first, words)), Some((last, _))) => {
                let title = lead.label(*first, *last);
                vec![BibleRow {
                    lookup: title.clone(),
                    title,
                    tag: "BIBLE".to_owned(),
                    line: (*words).to_owned(),
                    passage: true,
                }]
            }
            _ => Vec::new(),
        };
    }
    let mut rows = Vec::new();
    if typed.chars().count() < LEAST_LETTERS {
        return rows;
    }
    for (short, bible) in bibles {
        for (place, words) in bible.search(typed, VERSES_LISTED - rows.len()) {
            let title = bible.label(place, place);
            rows.push(BibleRow {
                lookup: title.clone(),
                title,
                tag: format!("BIBLE \u{b7} {}", short.to_uppercase()),
                line: words.to_owned(),
                passage: false,
            });
        }
        if rows.len() >= VERSES_LISTED {
            break;
        }
    }
    rows
}

/// What was asked: the words typed, and which Bibles (by their ids, in order).
type Asked = (String, Vec<String>);

/// An answer from the find box's thread.
struct Answer {
    asked: Asked,
    rows: Vec<BibleRow>,
    /// Bibles read for the first time on the way, for the shelf to keep.
    read: Vec<(String, Arc<Bible>)>,
}

struct Job {
    asked: Asked,
    bibles: Vec<BibleReader>,
    wake: egui::Context,
}

/// Reads the Bibles a job names and searches them.
fn answer(job: Job) -> Answer {
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
    Answer { rows: search_bibles(&job.asked.0, &bibles), asked: job.asked, read }
}

/// The find box's Bible results, asked for as the words change and taken when they come.
#[derive(Default)]
pub struct Finder {
    /// What was last asked for.
    asked: Option<Asked>,
    /// The latest answer, once the window has taken it.
    have: Option<Answer>,
    arrived: Arc<Mutex<Option<Answer>>>,
    jobs: Option<Sender<Job>>,
}

impl Finder {
    /// The Bible results for what is typed, or `None` while they are still being found. Asks
    /// for them when the words or the Bibles in use have changed (`bibles` is called only
    /// then). Never waits.
    pub fn rows(
        &mut self,
        typed: &str,
        in_use: &[String],
        bibles: impl FnOnce() -> Vec<BibleReader>,
        wake: &egui::Context,
    ) -> Option<&[BibleRow]> {
        let current = |asked: &Asked| asked.0 == typed && asked.1 == in_use;
        if !self.asked.as_ref().is_some_and(current) {
            let asked: Asked = (typed.to_owned(), in_use.to_vec());
            self.asked = Some(asked.clone());
            let job = Job { asked, bibles: bibles(), wake: wake.clone() };
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
        self.have.as_ref().filter(|answer| current(&answer.asked)).map(|answer| answer.rows.as_slice())
    }

    /// The Bibles read for the first time by the last answer, for the shelf to keep so they are
    /// not read again.
    pub fn take_read(&mut self) -> Vec<(String, Arc<Bible>)> {
        self.have.as_mut().map(|answer| std::mem::take(&mut answer.read)).unwrap_or_default()
    }

    /// Hands a job to the thread, starting it the first time. The thread sleeps between jobs,
    /// answers only the newest of those waiting, and ends when the window does.
    fn send(&mut self, job: Job) {
        if self.jobs.is_none() {
            let (jobs, waiting) = channel::<Job>();
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

    #[test]
    fn a_reference_gives_its_passage_from_the_first_bible() {
        let bibles = shipped();
        for typed in ["John 3:16-18", "jn 3:16-18", "Ин 3:16-18", "Ів 3:16-18"] {
            let rows = search_bibles(typed, &bibles);
            assert_eq!(rows.len(), 1, "{typed}");
            assert_eq!((rows[0].title.as_str(), rows[0].tag.as_str()), ("John 3:16-18", "BIBLE"), "{typed}");
            assert_eq!(rows[0].lookup, "John 3:16-18");
            assert!(rows[0].line.starts_with("For God so loved the world"), "{}", rows[0].line);
        }
        // A chapter alone is the whole chapter.
        let psalm = search_bibles("ps 23", &bibles);
        assert_eq!(psalm.len(), 1);
        assert!(psalm[0].title.ends_with("23:1-6"), "{}", psalm[0].title);
        // A book alone, or a book that is a number and a name, is not offered whole.
        assert!(search_bibles("john", &bibles).iter().all(|row| !row.passage));
        assert!(search_bibles("1 john", &bibles).iter().all(|row| !row.passage));
    }

    #[test]
    fn words_give_the_first_verses_that_hold_them() {
        let bibles = shipped();
        let rows = search_bibles("valley of the shadow", &bibles);
        assert!(!rows.is_empty() && rows.len() <= VERSES_LISTED);
        assert!(rows.iter().all(|row| row.tag.starts_with("BIBLE \u{b7} ") && !row.passage), "{rows:?}");
        assert!(rows.iter().any(|row| row.title.ends_with("23:4")), "{rows:?}");
        // Verses holding the words together come before those holding them apart.
        assert!(rows[0].line.to_lowercase().contains("valley of the shadow"), "{rows:?}");
        // Words of another language are found in the Bible of that language.
        let russian = search_bibles("долиною смертной тени", &bibles);
        assert!(!russian.is_empty(), "the Russian Bible is searched after the English ones");
        // Fewer than three letters, or nothing any Bible has, finds nothing.
        assert!(search_bibles("of", &bibles).is_empty());
        assert!(search_bibles("zzzqqqxxx", &bibles).is_empty());
        assert!(search_bibles("valley", &[]).is_empty());
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
            let rows = search_bibles(typed, &bibles);
            eprintln!("{typed:?}: {} rows in {:?}", rows.len(), started.elapsed());
        }
    }
}
