//! The app's side of Settings › Song files › Find songs on this computer (the search, the dry
//! run and the card's state are `find_songs`): starting and stopping a search, the folder
//! window, bringing in what was ticked, and `sp songs find`.
//!
//! The window and `sp` share one search and one list, so `sp songs find --import 2` brings in
//! the second row the window lists. An import reads its sources again on a thread of its own,
//! through the import's own readers, and its songs come into the library on the main thread
//! through `App::take_song`, as File › Import Songs takes them, a few milliseconds' worth at a
//! time, so neither the window nor the screens ever wait: each source into a library of its
//! name, with only its new songs (`only_new`: a song the library has stays where it is).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use sp_model::{Presentation, Uuid};

use crate::control::Reply;
use crate::find_songs::{self, Ask, Brought, Found, Phase, Source};
use crate::i18n::{t, tf};

/// How long each turn of taking songs in may take on the main thread, before the window and
/// everything else have theirs.
const SLICE: Duration = Duration::from_millis(8);
/// How many songs the reading thread hands over at once.
const BATCH: usize = 50;

/// The search and the import under way, and the `sp` jobs waiting on them.
#[derive(Default)]
pub struct Finding {
    search: Option<find_songs::Search>,
    /// The last search's number.
    searches: u64,
    importing: Option<Importing>,
    /// The `sp songs find` waiting for the search to end, and the one waiting for the import.
    find_job: Option<u64>,
    import_job: Option<u64>,
    /// The rows a finished `sp songs find` gives as JSON (`sp job N --json`), by its job.
    job_data: Option<(u64, serde_json::Value)>,
    /// A capture run's Import has been pressed (`find-songs-import`).
    capture_imported: bool,
}

impl std::fmt::Debug for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Finding(search {:?}, importing {})", self.search, self.importing.is_some())
    }
}

impl Finding {
    /// The JSON a finished job gives besides its words, if it was a search's.
    pub(crate) fn job_data(&self, job: u64) -> Option<&serde_json::Value> {
        self.job_data.as_ref().filter(|(have, _)| *have == job).map(|(_, data)| data)
    }
}

/// What the import's reading thread hands over.
enum Taken {
    /// What the library has, by the import's key, to look each song up in.
    Have(HashMap<(String, String), usize>),
    /// Songs of a source (its place in the import), each with the file it came from.
    Songs(usize, Vec<(Presentation, PathBuf)>),
    /// What the operator should be told of a source (a file that couldn't be read).
    Problems(Vec<String>),
    Done,
}

/// An import under way.
struct Importing {
    rx: Receiver<Taken>,
    /// The sources' names, the libraries their songs go into.
    names: Vec<String>,
    /// What the library has, once the thread has worked it out, and every id given out.
    have: Option<HashMap<(String, String), usize>>,
    ids: HashSet<Uuid>,
    /// The library's songs looked at for `have`; songs added since are looked at as they come.
    known_upto: usize,
    queue: VecDeque<(usize, Presentation, PathBuf)>,
    done: crate::Imported,
    /// Songs that came in, by source, and those the library had already.
    came: Vec<usize>,
    left_out: usize,
    problems: Vec<String>,
    /// How many songs pass through (new and here), and how many have.
    expected: usize,
    taken: usize,
    read_all: bool,
    began: Instant,
}

/// The sources as `sp` lists them, in English: a line for each, numbered, as the window shows
/// them.
fn listing(card: &find_songs::Card, stopped: bool) -> (String, serde_json::Value) {
    crate::i18n::with_language(crate::i18n::Language::English, || {
        let places: Vec<String> = card.looked.iter().map(|place| place.words()).collect();
        let found = card.sources.len();
        let mut text = if found == 0 {
            format!("No songbooks, libraries or song files were found in {}.", places.join(", "))
        } else {
            format!(
                "{} in {}{}:",
                if found == 1 {
                    "1 songbook or library found".to_owned()
                } else {
                    format!("{found} songbooks and libraries found")
                },
                places.join(", "),
                if stopped { " (stopped before the end)" } else { "" }
            )
        };
        let mut rows = Vec::new();
        for (at, source) in card.sources.iter().enumerate() {
            let (name, line, count) = find_songs::row_words(source);
            text.push_str(&format!("\n  {:>2}  {name}  \u{2014}  {line}  \u{2014}  {}", at + 1, count.to_lowercase()));
            rows.push(serde_json::json!({
                "number": at + 1,
                "name": name,
                "kind": source.kind.english(),
                "softprojector": source.kind == find_songs::Kind::Songbook,
                "where": source.place.words(),
                "path": source.path.display().to_string(),
                "songs": source.songs,
                "new": source.fresh(),
                "here": source.here,
                "why": source.why,
                "ticked": card.ticked.get(at).copied().unwrap_or(false),
            }));
        }
        let new = find_songs::new_in(card.sources.iter().filter(|source| source.can_import()));
        if new > 0 {
            text.push_str(&format!(
                "\nsp songs find --import N\u{2026} brings in the new songs of those numbered (or one by its name): {new} new in all. Nothing comes in until then."
            ));
        } else if found > 0 {
            text.push_str("\nNothing here is new to your library.");
        }
        let data = serde_json::json!({ "places": places, "sources": rows, "new": new, "stopped": stopped });
        (text, data)
    })
}

impl crate::App {
    /// Marks a job done with what it said.
    fn finish_find_job(&mut self, job: Option<u64>, result: Result<String, String>) {
        if let Some(job) = job
            && let Some((_, kept)) = self.jobs.iter_mut().find(|(have, _)| *have == job)
        {
            *kept = Some(result);
        }
    }

    /// The Settings window drawn again, where it is open.
    fn redraw_settings(&self) {
        if let Some(running) = self.running.as_ref() {
            for panel in running.panels.iter().filter(|panel| panel.kind == crate::Panel::Settings) {
                panel.window.request_redraw();
            }
        }
    }

    /// Starts a search of the places where it looks (the usual ones and the folders added),
    /// against the library as it is now. A search under way stops first. `job`: an `sp songs
    /// find` waiting for it.
    fn start_find(&mut self, job: Option<u64>) -> Result<(), String> {
        if self.finding.importing.is_some() {
            return Err(t("settings.find.busy", "Songs are being brought in; find again once they are in.").to_owned());
        }
        self.end_search(true);
        let Some(running) = self.running.as_ref() else { return Err("The window isn't open yet.".to_owned()) };
        let card = &running.home_state.find_songs;
        let places = find_songs::places(&card.added);
        let library = find_songs::Library {
            songs: self.import_compares_with().into_iter().map(|(_, song)| song).collect(),
            ids: self.import_ids(),
        };
        self.finding.searches += 1;
        let id = self.finding.searches;
        let waker = self.waker.clone();
        let wake = move || {
            if let Some(waker) = &waker {
                let _ = waker.send_event(crate::AppEvent::FindSongs);
            }
        };
        let looked: Vec<find_songs::PlaceName> = places.iter().map(|place| place.name.clone()).collect();
        let search = find_songs::start(id, places, library, find_songs::never_listed(), wake)
            .map_err(|e| tf("settings.find.not_started", "Couldn\u{2019}t start looking: {error}", &[("error", &e)]))?;
        crate::applog::event("find", &format!("looking for songs in {} places", looked.len()));
        self.finding.search = Some(search);
        self.finding.find_job = job;
        if let Some(running) = self.running.as_mut() {
            let card = &mut running.home_state.find_songs;
            card.phase = Phase::Searching;
            card.looking = looked.first().cloned();
            card.looked = looked;
            card.sources.clear();
            card.ticked.clear();
            card.brought = None;
        }
        self.redraw_settings();
        Ok(())
    }

    /// Ends the search under way, keeping what it found: the card lists it, and an `sp`
    /// waiting for it is told. `stopped`: before it had looked everywhere.
    fn end_search(&mut self, stopped: bool) {
        let Some(search) = self.finding.search.take() else { return };
        search.stop();
        let Some(running) = self.running.as_mut() else { return };
        let card = &mut running.home_state.find_songs;
        card.phase = Phase::Found;
        card.looking = None;
        let (text, data) = listing(card, stopped);
        let chosen = card.chosen();
        if let Some(job) = self.finding.find_job.take() {
            self.finding.job_data = Some((job, data));
            self.finish_find_job(Some(job), Ok(text));
        }
        self.redraw_settings();
        // Testing aid: SOFTPRESENTER_CAPTURE_PREFERENCES=find-songs-import presses Import, with
        // what is ticked, once the search Find started has ended, so a picture can catch the
        // card bringing songs in, and done.
        if !stopped
            && !std::mem::replace(&mut self.finding.capture_imported, true)
            && std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES").as_deref() == Ok("find-songs-import")
            && let Err(problem) = self.start_import(chosen, None)
        {
            eprintln!("find-songs-import: {problem}");
        }
    }

    /// Starts bringing in the sources at these places of the card's list: read again on a
    /// thread of their own, their songs taken in a slice at a time (see `tick_find_songs`).
    fn start_import(&mut self, chosen: Vec<usize>, job: Option<u64>) -> Result<usize, String> {
        if self.finding.importing.is_some() {
            return Err(t("settings.find.importing_already", "Songs are being brought in already.").to_owned());
        }
        let Some(running) = self.running.as_ref() else { return Err("The window isn't open yet.".to_owned()) };
        let card = &running.home_state.find_songs;
        let sources: Vec<Source> = chosen.iter().filter_map(|at| card.sources.get(*at).cloned()).collect();
        if sources.is_empty() {
            return Err(t("settings.find.none_ticked", "Nothing ticked has songs new to your library.").to_owned());
        }
        let bringing = find_songs::new_in(sources.iter());
        let compare = self.import_compares_with();
        let (tx, rx) = channel();
        let waker = self.waker.clone();
        let wake = move || {
            if let Some(waker) = &waker {
                let _ = waker.send_event(crate::AppEvent::FindSongs);
            }
        };
        let reading = sources.clone();
        let slow = find_songs::slow();
        std::thread::Builder::new()
            .name("find songs import".to_owned())
            .spawn(move || {
                let have = compare.iter().map(|(at, song)| (crate::load::import_key(song), *at)).collect();
                let _ = tx.send(Taken::Have(have));
                wake();
                for (at, source) in reading.iter().enumerate() {
                    let (songs, problems) = find_songs::read_again(source);
                    if !problems.is_empty() {
                        let _ = tx.send(Taken::Problems(problems));
                    }
                    let mut songs = songs.into_iter().peekable();
                    while songs.peek().is_some() {
                        let batch: Vec<_> = songs.by_ref().take(if slow.is_zero() { BATCH } else { 1 }).collect();
                        if !slow.is_zero() {
                            std::thread::sleep(slow);
                        }
                        if tx.send(Taken::Songs(at, batch)).is_err() {
                            return;
                        }
                        wake();
                    }
                }
                let _ = tx.send(Taken::Done);
                wake();
            })
            .map_err(|e| {
                tf("settings.find.not_started", "Couldn\u{2019}t start looking: {error}", &[("error", &e.to_string())])
            })?;
        crate::applog::event("find", &format!("bringing in {bringing} new songs from {} sources", sources.len()));
        self.finding.importing = Some(Importing {
            rx,
            names: sources.iter().map(|source| source.name.clone()).collect(),
            have: None,
            ids: self.import_ids(),
            known_upto: self.show.presentations().len(),
            queue: VecDeque::new(),
            done: crate::Imported::default(),
            came: vec![0; sources.len()],
            left_out: 0,
            problems: Vec::new(),
            expected: sources.iter().map(|source| source.songs).sum(),
            taken: 0,
            read_all: false,
            began: Instant::now(),
        });
        self.finding.import_job = job;
        if let Some(running) = self.running.as_mut() {
            let card = &mut running.home_state.find_songs;
            for (at, ticked) in card.ticked.iter_mut().enumerate() {
                *ticked = chosen.contains(&at);
            }
            card.phase = Phase::Importing;
            card.bringing = bringing;
            card.progress = 0.0;
        }
        self.redraw_settings();
        Ok(bringing)
    }

    /// What the search said since the last look, and a slice of the import's songs taken in.
    /// Called from the idle loop and when their threads wake the app; returns when it next
    /// wants to be called (soon, while songs are waiting to come in).
    pub(crate) fn tick_find_songs(&mut self) -> Option<Instant> {
        if let Some(search) = self.finding.search.as_ref() {
            let said = search.said();
            let mut ended = false;
            if let Some(running) = self.running.as_mut()
                && !said.is_empty()
            {
                let card = &mut running.home_state.find_songs;
                for found in said {
                    match found {
                        Found::Looking(place) => card.looking = Some(place),
                        Found::Source(source) => card.add(*source),
                        Found::Done => ended = true,
                    }
                }
                self.redraw_settings();
            }
            if ended {
                self.end_search(false);
            }
        }
        let mut importing = self.finding.importing.take()?;
        let slice = Instant::now() + SLICE;
        loop {
            match importing.rx.try_recv() {
                Ok(Taken::Have(have)) => importing.have = Some(have),
                Ok(Taken::Songs(at, songs)) => {
                    importing.queue.extend(songs.into_iter().map(|(song, path)| (at, song, path)));
                }
                Ok(Taken::Problems(problems)) => importing.problems.extend(problems),
                Ok(Taken::Done) => importing.read_all = true,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                // A thread that ended without saying so has nothing more to give.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    importing.read_all = true;
                    break;
                }
            }
        }
        if let Some(mut have) = importing.have.take() {
            // Songs added to the library since (by hand, or in an earlier slice) are looked for
            // too.
            let now = self.show.presentations().len();
            if importing.known_upto < now {
                let added: Vec<(usize, std::sync::Arc<Presentation>)> =
                    self.import_compares_with().into_iter().filter(|(at, _)| *at >= importing.known_upto).collect();
                for (at, song) in added {
                    have.entry(crate::load::import_key(&song)).or_insert(at);
                }
            }
            while Instant::now() < slice
                && let Some((at, song, path)) = importing.queue.pop_front()
            {
                let library = importing.names.get(at).cloned();
                let before = importing.done.titles.len();
                self.take_song(
                    song,
                    &path,
                    &library,
                    None,
                    None,
                    (&mut have, &mut importing.ids),
                    true,
                    &mut importing.done,
                );
                if importing.done.titles.len() > before {
                    importing.came[at] += 1;
                } else {
                    importing.left_out += 1;
                }
                importing.taken += 1;
            }
            importing.known_upto = self.show.presentations().len();
            importing.have = Some(have);
        }
        if importing.read_all && importing.queue.is_empty() {
            self.finish_import(importing);
            return None;
        }
        let progress = importing.taken as f32 / importing.expected.max(1) as f32;
        if let Some(running) = self.running.as_mut() {
            running.home_state.find_songs.progress = progress.min(1.0);
            running.home_window.request_redraw();
        }
        self.redraw_settings();
        let busy = !importing.queue.is_empty();
        self.finding.importing = Some(importing);
        busy.then(|| Instant::now() + Duration::from_millis(4))
    }

    /// The import has taken in every song: the card and a notice say what came in, from where,
    /// and how many were left out, and an `sp` waiting for it is told.
    fn finish_import(&mut self, importing: Importing) {
        let brought = Brought {
            came: importing.done.titles.len(),
            from: importing.names.iter().cloned().zip(importing.came.iter().copied()).collect(),
            left_out: importing.left_out,
        };
        crate::applog::event(
            "find",
            &format!(
                "brought in {} songs from {} sources ({} in the library already) in {} ms",
                brought.came,
                brought.from.len(),
                brought.left_out,
                importing.began.elapsed().as_millis()
            ),
        );
        for problem in &importing.problems {
            crate::applog::event("find", &format!("not imported: {problem}"));
        }
        let said = find_songs::brought_words(&brought).concat();
        let english =
            crate::i18n::with_language(crate::i18n::Language::English, || find_songs::brought_words(&brought).concat());
        let job = self.finding.import_job.take();
        self.finish_find_job(job, Ok(english));
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            state.notices.info(said);
            if !importing.problems.is_empty() {
                const NAMED: usize = 3;
                let more = importing.problems.len().saturating_sub(NAMED);
                let mut named: Vec<String> = importing.problems.iter().take(NAMED).cloned().collect();
                if more > 0 {
                    named.push(tf(
                        "notices.bundle.more_in_log",
                        " and {n} more (see the log)",
                        &[("n", &crate::i18n::count(more as u64))],
                    ));
                }
                state.notices.trouble(named.join(" "));
            }
            let card = &mut state.find_songs;
            card.phase = Phase::Done;
            card.progress = 1.0;
            card.brought = Some(brought);
            running.home_window.request_redraw();
        }
        self.redraw_settings();
    }

    /// Carries out what the card asked: Find, Stop, Search Another Folder… and Import. Called
    /// after each drawing of the Settings window.
    pub(crate) fn handle_find_songs(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let asks = std::mem::take(&mut running.home_state.find_songs.asks);
        for ask in asks {
            let result = match ask {
                Ask::Find => self.start_find(None),
                Ask::Stop => {
                    self.end_search(true);
                    Ok(())
                }
                Ask::AnotherFolder => {
                    // The system's own window for choosing a folder, as the app's others are.
                    let chosen = rfd::FileDialog::new()
                        .set_title(t("notices.dialog.find_folder", "Search Another Folder"))
                        .pick_folder();
                    match chosen {
                        Some(folder) => self.add_find_folder(folder),
                        None => Ok(()),
                    }
                }
                Ask::Import => {
                    let chosen = self.running.as_ref().map(|r| r.home_state.find_songs.chosen()).unwrap_or_default();
                    self.start_import(chosen, None).map(|_| ())
                }
            };
            if let Err(problem) = result
                && let Some(running) = self.running.as_mut()
            {
                running.home_state.notices.trouble(problem);
            }
        }
    }

    /// Adds a folder to where it looks. Before a search, it joins the places shown; after one,
    /// a search starts again at once, so what the card says it looked in stays true.
    fn add_find_folder(&mut self, folder: PathBuf) -> Result<(), String> {
        let Some(running) = self.running.as_mut() else { return Ok(()) };
        let card = &mut running.home_state.find_songs;
        if !card.added.contains(&folder) {
            card.added.push(folder);
        }
        let again = matches!(card.phase, Phase::Found | Phase::Done);
        self.redraw_settings();
        if again { self.start_find(None) } else { Ok(()) }
    }

    /// `songs find [--in FOLDER]…`: looks for songbooks, libraries and song files, as the
    /// window's Find does (a job: `sp` waits for it, and lists what was found, numbered).
    /// `songs find --import N…|NAME`: brings in the new songs of those numbered in the last
    /// search's list (or the one named), as the window's Import does (a job too).
    pub(crate) fn ctl_songs_find(&mut self, args: &[String]) -> Result<Reply, String> {
        self.only_at_the_computer(crate::remote::policy::AtComputer::FindSongs)?;
        const USAGE: &str = "songs find [--in FOLDER]…   |   songs find --import N…|NAME";
        let mut folders = Vec::new();
        let mut picked: Option<Vec<String>> = None;
        let mut words = args.iter().skip(2).peekable();
        while let Some(word) = words.next() {
            match word.as_str() {
                "--in" => folders.push(PathBuf::from(words.next().ok_or(USAGE)?)),
                "--import" => {
                    let mut named = Vec::new();
                    while let Some(next) = words.next_if(|next| !next.starts_with("--")) {
                        named.push(next.clone());
                    }
                    if named.is_empty() {
                        return Err(USAGE.to_owned());
                    }
                    picked = Some(named);
                }
                _ => return Err(USAGE.to_owned()),
            }
        }
        if self.running.is_none() {
            return Err("The window isn't open yet.".to_owned());
        }
        if let Some(picked) = picked {
            if !folders.is_empty() {
                return Err("--import picks from the last search's list: run sp songs find --in FOLDER first, then sp songs find --import N.".to_owned());
            }
            let chosen = self.pick_found(&picked)?;
            let job = self.start_job();
            match self.start_import(chosen, Some(job)) {
                Ok(bringing) => Ok(Reply::with(
                    format!("Bringing in {bringing} new songs\u{2026}"),
                    serde_json::json!({ "wait": true, "new": bringing }),
                )),
                Err(problem) => {
                    self.jobs.retain(|(have, _)| *have != job);
                    self.last_job = None;
                    Err(problem)
                }
            }
        } else {
            for folder in &folders {
                if !folder.is_dir() {
                    return Err(format!("There is no folder {}.", folder.display()));
                }
            }
            if let Some(running) = self.running.as_mut() {
                let card = &mut running.home_state.find_songs;
                for folder in folders {
                    if !card.added.contains(&folder) {
                        card.added.push(folder);
                    }
                }
            }
            let job = self.start_job();
            if let Err(problem) = self.start_find(Some(job)) {
                self.jobs.retain(|(have, _)| *have != job);
                self.last_job = None;
                return Err(problem);
            }
            Ok(Reply::with("Looking for songs\u{2026}", serde_json::json!({ "wait": true })))
        }
    }

    /// The places in the last search's list of the sources picked, by their numbers or one by
    /// its name; each must have something new.
    fn pick_found(&self, picked: &[String]) -> Result<Vec<usize>, String> {
        let card = &self.running.as_ref().ok_or("The window isn't open yet.")?.home_state.find_songs;
        match card.phase {
            Phase::Searching => {
                return Err("Still looking: sp songs find --import N once the search has ended.".to_owned());
            }
            Phase::Importing => return Err("Songs are being brought in already.".to_owned()),
            Phase::Before | Phase::Done => {
                return Err("Find first: sp songs find lists what there is, numbered.".to_owned());
            }
            Phase::Found => {}
        }
        let names: Vec<String> = card.sources.iter().map(|source| find_songs::row_words(source).0).collect();
        let places: Vec<usize> = if picked.iter().all(|word| word.parse::<usize>().is_ok()) {
            picked
                .iter()
                .map(|word| {
                    let n: usize = word.parse().unwrap_or(0);
                    (1..=card.sources.len()).contains(&n).then(|| n - 1).ok_or_else(|| {
                        format!("There is no {n} in the list; sp songs find lists {} of them.", card.sources.len())
                    })
                })
                .collect::<Result<_, _>>()?
        } else {
            vec![crate::control::find("songbook or library", &names, &picked.join(" "))?]
        };
        for at in &places {
            let source = &card.sources[*at];
            if let Some(why) = &source.why {
                return Err(format!("{} couldn't be read: {why}", names[*at]));
            }
            if source.fresh() == 0 {
                return Err(format!("{}: everything here is already in your library.", names[*at]));
            }
        }
        let mut places = places;
        places.dedup();
        Ok(places)
    }
}
