//! The app's side of the song catalog (the engine is in `catalog`): the `catalog` commands, a
//! song joining the library, "use the newer version" and "keep mine", and the monthly check that
//! the app runs on its idle loop.
//!
//! Anything slow (a download, a check, asking the server how big a pack is) runs on a thread of
//! its own, as a numbered job; the thread only fetches and files the pack, and what it found is
//! applied to the library on the main thread, and only while nothing is on screen.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use sp_model::Presentation;

use crate::catalog::{self, Catalog, Hit, Outcome, Progress, Verdict};
use crate::control::Reply;
use crate::i18n::{t, tf, tn};

/// How long after the app starts before it first looks at what is due: the start is left alone.
const FIRST_LOOK_AFTER: Duration = Duration::from_secs(90);
/// How often what is due is looked at after that (a language is only asked about when a month,
/// or after a failure a day, has passed: see `Catalog::due`).
const LOOK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How long to wait, with something on screen or open for editing, before trying again.
const BUSY_RETRY: Duration = Duration::from_secs(5 * 60);

/// What came of a download, in words: from its language, its outcome or problem, and the
/// seconds it took.
type DownloadWords = fn(&str, Result<&Outcome, &str>, f32) -> Result<String, String>;

/// What a background job finished.
struct Done {
    job: Option<u64>,
    /// The words for the job's result, or why it failed.
    result: Result<String, String>,
    /// A pack was filed, so the copies in the library may need bringing up to date.
    installed: bool,
    /// Started by the app's own monthly check, not by a person: said in a notice when it
    /// changed something, and not when it didn't.
    auto: bool,
    langs: Vec<String>,
}

/// The catalog's state in the app.
pub struct CatalogState {
    pub catalog: Option<Arc<Catalog>>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    /// Languages being downloaded or checked now, with how far each has got.
    pub busy: Vec<(String, Arc<Progress>)>,
    started: Instant,
    next_look: Instant,
    /// The copies in the library are to be looked over (a pack was filed, or the app started).
    apply_pending: bool,
    /// The first look over the copies has been made.
    looked_once: bool,
    /// The source's answers about how big songbooks are, for the windows (`songbooks`).
    sizes_tx: Sender<Vec<catalog::Size>>,
    sizes_rx: Receiver<Vec<catalog::Size>>,
    /// The songbooks on this computer are to be read again for the windows: at the start, after
    /// a download and after a removal.
    stale: bool,
}

impl Default for CatalogState {
    fn default() -> CatalogState {
        let (tx, rx) = channel();
        let (sizes_tx, sizes_rx) = channel();
        let now = Instant::now();
        CatalogState {
            catalog: Catalog::in_data_dir().map(Arc::new),
            tx,
            rx,
            busy: Vec::new(),
            started: now,
            next_look: now + FIRST_LOOK_AFTER,
            apply_pending: false,
            looked_once: false,
            sizes_tx,
            sizes_rx,
            stale: true,
        }
    }
}

impl std::fmt::Debug for CatalogState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CatalogState(busy {})", self.busy.len())
    }
}

/// A song that joined the library.
#[derive(Debug)]
pub struct Joined {
    /// Its place among the app's songs.
    pub index: usize,
    pub title: String,
    pub library: String,
    /// It was in the library already (an earlier copy of this catalog song), and nothing new was
    /// made.
    pub already: bool,
    pub pairing: catalog::Pairing,
}

/// What looking over the copies in the library did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub updated: usize,
    pub offered: usize,
    pub gone: usize,
    /// Copies left for later: one open for editing.
    pub waiting: usize,
}

/// A language's name in English, for the sentences `sp` says; the code itself for one not known.
pub fn language_name(code: &str) -> String {
    catalog::songbook(code).map_or_else(|| code.to_owned(), |book| book.english.to_owned())
}

/// A language's name as the window's notices say it, in the language set (in English as
/// [`language_name`] says it); the code itself for one not known.
fn language_shown(code: &str) -> String {
    let base = code.split('-').next().unwrap_or(code);
    match base {
        "en" => t("notices.catalog.lang.en", "English"),
        "ru" => t("notices.catalog.lang.ru", "Russian"),
        "uk" => t("notices.catalog.lang.uk", "Ukrainian"),
        "es" => t("notices.catalog.lang.es", "Spanish"),
        "de" => t("notices.catalog.lang.de", "German"),
        "fr" => t("notices.catalog.lang.fr", "French"),
        "pt" => t("notices.catalog.lang.pt", "Portuguese"),
        "ro" => t("notices.catalog.lang.ro", "Romanian"),
        "pl" => t("notices.catalog.lang.pl", "Polish"),
        "cs" => t("notices.catalog.lang.cs", "Czech"),
        "sk" => t("notices.catalog.lang.sk", "Slovak"),
        "bg" => t("notices.catalog.lang.bg", "Bulgarian"),
        "hu" => t("notices.catalog.lang.hu", "Hungarian"),
        "tr" => t("notices.catalog.lang.tr", "Turkish"),
        "kk" => t("notices.catalog.lang.kk", "Kazakh"),
        "ky" => t("notices.catalog.lang.ky", "Kyrgyz"),
        "uz" => t("notices.catalog.lang.uz", "Uzbek"),
        "az" => t("notices.catalog.lang.az", "Azerbaijani"),
        "mn" => t("notices.catalog.lang.mn", "Mongolian"),
        "ar" => t("notices.catalog.lang.ar", "Arabic"),
        "ko" => t("notices.catalog.lang.ko", "Korean"),
        "sv" => t("notices.catalog.lang.sv", "Swedish"),
        "be" => t("notices.catalog.lang.be", "Belarusian"),
        _ => return code.to_owned(),
    }
    .to_owned()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn number(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (at, c) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn size(bytes: u64) -> String {
    if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / f64::from(1 << 20))
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}

/// What a download or a check of a language did, in a sentence.
fn outcome_words(lang: &str, outcome: &Outcome) -> String {
    let name = language_shown(lang);
    let count = |n: usize| crate::i18n::count(n as u64);
    match outcome {
        Outcome::Unchanged { songs } => tf(
            "notices.catalog.up_to_date",
            "The {language} songs are up to date ({songs} songs); nothing was downloaded.",
            &[("language", &name), ("songs", &count(*songs))],
        ),
        Outcome::Installed(done) => {
            let songs = count(done.songs);
            let mut said = if done.added + done.changed + done.gone > 0 {
                tf(
                    "notices.catalog.downloaded_changes",
                    "Downloaded the {language} songs: {songs} songs ({added} new, {changed} changed, {gone} gone from the source).",
                    &[
                        ("language", &name),
                        ("songs", &songs),
                        ("added", &count(done.added)),
                        ("changed", &count(done.changed)),
                        ("gone", &count(done.gone)),
                    ],
                )
            } else {
                tf(
                    "notices.catalog.downloaded",
                    "Downloaded the {language} songs: {songs} songs.",
                    &[("language", &name), ("songs", &songs)],
                )
            };
            if done.malformed > 0 {
                said.push(' ');
                said.push_str(&tf(
                    "notices.catalog.malformed",
                    "{n} files in the pack could not be read and were left out.",
                    &[("n", &count(done.malformed))],
                ));
            }
            said
        }
    }
}

fn hit_json(hit: &Hit) -> serde_json::Value {
    serde_json::json!({
        "key": hit.key, "language": hit.lang, "title": hit.title, "line": hit.line,
        "in_title": hit.in_title, "versions": hit.versions, "chords": hit.chords,
        "credit": catalog::CREDIT.text, "site": catalog::CREDIT.site,
    })
}

impl crate::App {
    fn catalog_engine(&self) -> Result<Arc<Catalog>, String> {
        self.catalog.catalog.clone().ok_or_else(|| "Could not find where to keep the song catalog.".to_owned())
    }

    /// The copies of catalog songs in the library: their places and their marks.
    fn catalog_copies(&self) -> Vec<(usize, &Presentation)> {
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        crate::catalog_songs::copies(self.show.presentations(), |at| self.app_made(at) || removed.contains(&at))
    }

    /// The keys of the catalog songs already in the library, which the find box shows as the
    /// library's.
    pub(crate) fn catalog_joined_keys(&self) -> HashSet<String> {
        self.catalog_copies().into_iter().filter_map(|(_, song)| song.catalog.as_ref().map(|o| o.key.clone())).collect()
    }

    /// The copy the library has of a catalog song, as a song that has joined already.
    fn catalog_joined(&self, key: &str) -> Option<Joined> {
        let (index, song) =
            self.catalog_copies().into_iter().find(|(_, song)| song.catalog.as_ref().is_some_and(|o| o.key == key))?;
        Some(Joined {
            index,
            title: song.title.clone(),
            library: catalog::LIBRARY_NAME.to_owned(),
            already: true,
            pairing: catalog::Pairing::default(),
        })
    }

    /// Puts a catalog song in the library as an ordinary song, in a library named after the
    /// catalog (made the first time), marked with where it came from. `second` is the language
    /// of the version to put under its words. A song already joined is not made again.
    pub(crate) fn join_catalog_song(&mut self, key: &str, second: Option<&str>) -> Result<Joined, String> {
        let engine = self.catalog_engine()?;
        if let Some(joined) = self.catalog_joined(key) {
            return Ok(joined);
        }
        let made = engine.present(key, second, now())?;
        self.catalog_file(made.song, made.pairing)
    }

    /// Files a song made from the catalog (and marked so) in the library, and in the library
    /// named after the catalog, made the first time: how every catalog song joins, from the
    /// command line or the window.
    fn catalog_file(&mut self, song: Presentation, pairing: catalog::Pairing) -> Result<Joined, String> {
        let key = song.catalog.as_ref().map(|origin| origin.key.clone()).unwrap_or_default();
        let title = song.title.clone();
        let (index, library_key) = self.store_song(song);
        let Some(running) = self.running.as_mut() else { return Err("The window isn't open yet.".to_owned()) };
        let state = &mut running.home_state;
        let at = match state.libraries.iter().position(|library| library.name == catalog::LIBRARY_NAME) {
            Some(at) => at,
            None => {
                state.libraries.push(crate::libraries::Library::new(catalog::LIBRARY_NAME.to_owned()));
                state.libraries.len() - 1
            }
        };
        if !state.libraries[at].has(&library_key) {
            state.libraries[at].songs.push(library_key);
        }
        state.libraries_changed = true;
        crate::applog::event("catalog", &format!("{title} joined the library from the song catalog ({key})"));
        Ok(Joined { index, title, library: catalog::LIBRARY_NAME.to_owned(), already: false, pairing })
    }

    /// Carries out what the window asked of the catalog's songs in the last frame (see
    /// `catalog_songs`): a song to look at goes into its place, and songs put to use join the
    /// library. Called after each drawing of the window.
    pub(crate) fn handle_catalog_songs(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let catalog = &mut running.home_state.catalog;
        let (look, joins) = (catalog.look.take(), std::mem::take(&mut catalog.joins));
        if let Some((place, song)) = look {
            self.look_at_catalog_song(place, song);
        }
        for join in joins {
            self.join_window_song(&join.song, join.then);
        }
    }

    /// Shows a catalog song in the slides area from one of the two places kept for it (see
    /// `catalog_songs::put_in_place`), and chooses it. It is not in the library.
    fn look_at_catalog_song(&mut self, place: usize, song: Arc<Presentation>) {
        let Some(running) = self.running.as_mut() else { return };
        let Some((slot, was)) =
            crate::catalog_songs::put_in_place(&mut self.show, &mut running.home_state, place, song)
        else {
            return;
        };
        for id in was {
            running.thumbs.forget(id, &mut running.egui_renderer);
        }
        running.home_window.request_redraw();
        self.apply(vec![sp_engine::Command::Select(slot)]);
    }

    /// Files a catalog song the window put to use, under the key the window already put in a
    /// playlist or a library for it; if a copy of it joined the library since it was made, that
    /// copy is the one used. What the window does then is `catalog_songs::joined`.
    fn join_window_song(&mut self, song: &Presentation, then: crate::catalog_songs::Then) {
        let Some(origin) = song.catalog.as_ref() else { return };
        let joined = match self.catalog_joined(&origin.key) {
            Some(joined) => joined,
            None => match self.catalog_file(song.clone(), catalog::Pairing::default()) {
                Ok(joined) => joined,
                Err(problem) => {
                    crate::applog::problem("catalog", &format!("{} could not join the library: {problem}", song.title));
                    return;
                }
            },
        };
        let Some(running) = self.running.as_mut() else { return };
        let chosen =
            crate::catalog_songs::joined(&self.show, &mut running.home_state, song, joined.index, joined.already, then);
        running.home_window.request_redraw();
        if let Some(index) = chosen {
            self.apply(vec![sp_engine::Command::Select(index)]);
        }
    }

    /// Takes a catalog song's newer version in place of the church's changed copy, as a change
    /// the window makes: it can be undone.
    pub(crate) fn catalog_use_newer_version(&mut self, at: usize) -> Result<String, String> {
        let engine = self.catalog_engine()?;
        let old = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let origin = old.catalog.clone().filter(|origin| origin.source == catalog::SOURCE);
        let Some(origin) = origin else { return Err(format!("{} isn't from {}.", old.title, catalog::SOURCE_NAME)) };
        let Some(offered) = origin.newer.clone() else {
            return Err(format!("{} has no newer version waiting.", old.title));
        };
        match engine.source_of(&origin.language, &origin.key) {
            catalog::Source::Has(hash) if hash == offered => {}
            catalog::Source::Has(_) => {
                return Err(format!("{}: the catalog has changed again since; check for updates first.", old.title));
            }
            _ => return Err(format!("{} is no longer in {}.", old.title, catalog::SOURCE_NAME)),
        }
        let fresh = engine.present(&origin.key, origin.second.as_deref(), now())?;
        let song = catalog::refreshed(&old, fresh);
        self.change_song(at, song);
        crate::applog::event("catalog", &format!("{} took the newer version from the song catalog", old.title));
        Ok(format!(
            "{} now has the newer version from {}. Undo brings your version back.",
            old.title,
            catalog::SOURCE_NAME
        ))
    }

    /// "Keep mine": the church keeps its song, and the same newer version is not offered again.
    pub(crate) fn catalog_keep_mine(&mut self, at: usize) -> Result<String, String> {
        let old = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let song = catalog::kept(&old).ok_or_else(|| format!("{} has no newer version waiting.", old.title))?;
        self.change_song(at, song);
        Ok(format!("{} stays as it is; the newer version won't be offered again.", old.title))
    }

    /// Looks over the copies of catalog songs in the library against the catalog: songs nobody
    /// has touched are brought up to date, a song the church changed only gets a note that a
    /// newer version exists, and a song the source dropped gets a note and stays. Nothing is done
    /// while anything is on screen (`None`), and a copy open for editing is left for later.
    pub(crate) fn catalog_apply(&mut self) -> Option<Applied> {
        if self.show.anything_live() {
            return None;
        }
        let engine = self.catalog.catalog.clone()?;
        let plan = engine.plan(self.catalog_copies());
        let editing =
            self.running.as_ref().and_then(|r| r.home_state.editing.as_ref().map(|draft| draft.at.presentation));
        let mut done = Applied::default();
        for (at, verdict) in plan {
            if editing == Some(at) {
                done.waiting += 1;
                continue;
            }
            let Some(old) = self.show.presentations().get(at).cloned() else { continue };
            let song = match &verdict {
                Verdict::Update => {
                    let Some(origin) = old.catalog.clone() else { continue };
                    match engine.present(&origin.key, origin.second.as_deref(), now()) {
                        Ok(fresh) => catalog::refreshed(&old, fresh),
                        Err(problem) => {
                            crate::applog::problem(
                                "catalog",
                                &format!("{} could not be updated: {problem}", old.title),
                            );
                            continue;
                        }
                    }
                }
                other => match catalog::marked(&old, other) {
                    Some(song) => song,
                    None => continue,
                },
            };
            match verdict {
                Verdict::Update => done.updated += 1,
                Verdict::Offer(_) => done.offered += 1,
                Verdict::Gone => done.gone += 1,
                Verdict::Tidy | Verdict::Current => {}
            }
            self.replace_song_quietly(at, song);
        }
        Some(done)
    }

    /// Puts a song changed by an outside source's update (the catalog, TimelyChurch) in its
    /// place: saved, and not a step to undo (nobody changed it; the steps kept for it are let
    /// go, as they would put back what the update just replaced).
    pub(crate) fn replace_song_quietly(&mut self, at: usize, song: Presentation) {
        let ids: Vec<sp_model::Uuid> = song.slides().map(|(_, slide)| slide.id).collect();
        self.history.forget(at);
        self.show.replace_presentation(at, Arc::new(song));
        if !self.unsaved.contains(&at) {
            self.unsaved.push(at);
        }
        self.unsaved_since.get_or_insert_with(Instant::now);
        if let Some(running) = self.running.as_mut() {
            for id in ids {
                running.thumbs.forget(id, &mut running.egui_renderer);
            }
            running.home_window.request_redraw();
        }
    }

    /// Starts a job on a thread of its own: what it makes is its result, and `installed` says a
    /// pack may have been filed.
    fn catalog_spawn(
        &mut self,
        langs: Vec<String>,
        job: Option<u64>,
        auto: bool,
        work: impl FnOnce(&Catalog, &[(String, Arc<Progress>)]) -> (Result<String, String>, bool) + Send + 'static,
    ) -> Result<(), String> {
        let engine = self.catalog_engine()?;
        let progress: Vec<(String, Arc<Progress>)> = langs.iter().map(|lang| (lang.clone(), Progress::new())).collect();
        self.catalog.busy.extend(progress.iter().cloned());
        let (tx, waker, named) = (self.catalog.tx.clone(), self.waker.clone(), langs);
        let spawned = std::thread::Builder::new().name("song catalog".to_owned()).spawn(move || {
            let (result, installed) = work(&engine, &progress);
            let _ = tx.send(Done { job, result, installed, auto, langs: named });
            if let Some(waker) = waker {
                let _ = waker.send_event(crate::AppEvent::CatalogDone);
            }
        });
        if spawned.is_err() {
            self.catalog.busy.clear();
            return Err("Could not start the download.".to_owned());
        }
        Ok(())
    }

    /// Starts downloading a language on a thread of its own (see [`Catalog::download`]): for
    /// `sp catalog download` and the songbook windows alike. `words` says what came of it (the
    /// outcome or the problem, and the seconds it took), for the job and the notices.
    fn start_catalog_download(&mut self, lang: String, job: Option<u64>, words: DownloadWords) -> Result<(), String> {
        let named = lang.clone();
        self.catalog_spawn(vec![lang], job, false, move |engine, progress| {
            let started = Instant::now();
            let outcome = engine.download(&named, now(), progress.first().map(|(_, p)| p.as_ref()));
            let installed = matches!(outcome, Ok(Outcome::Installed(_)));
            let seconds = started.elapsed().as_secs_f32();
            (words(&named, outcome.as_ref().map_err(String::as_str), seconds), installed)
        })
    }

    /// Carries out what the songbook windows (the setup's Free Songs step, Settings ›
    /// Integrations) asked, and tells them what is known: the songbooks on this computer, the
    /// downloads under way and the sizes the source gave. Called after each drawing of a window;
    /// does nothing where the catalog isn't offered.
    pub(crate) fn handle_songbooks(&mut self) {
        if !catalog::offered() {
            return;
        }
        let Some(running) = self.running.as_mut() else { return };
        let asks = std::mem::take(&mut running.home_state.songbooks.asks);
        for ask in asks {
            match ask {
                crate::songbooks::Ask::Download(langs) => self.download_songbooks(langs),
                crate::songbooks::Ask::Remove(lang) => self.remove_songbook(&lang),
                crate::songbooks::Ask::Look(langs) => self.look_at_songbooks(langs),
            }
        }
        let answers: Vec<Vec<catalog::Size>> = self.catalog.sizes_rx.try_iter().collect();
        let installed = std::mem::take(&mut self.catalog.stale).then(|| {
            let Some(engine) = self.catalog.catalog.as_ref() else { return Default::default() };
            engine
                .installed()
                .into_iter()
                .filter_map(|lang| {
                    let index = engine.index(&lang).ok().flatten()?;
                    Some((lang, index.entries.len()))
                })
                .collect()
        });
        let busy = self.catalog.busy.iter().map(|(lang, progress)| (lang.clone(), progress.per_cent())).collect();
        let Some(running) = self.running.as_mut() else { return };
        let known = &mut running.home_state.songbooks.known;
        known.busy = busy;
        if let Some(installed) = installed {
            known.installed = installed;
        }
        for sizes in answers {
            known.take_sizes(sizes);
        }
    }

    /// Downloads songbooks the windows chose, each on a thread of its own, saying so in one
    /// notice; those here already or under way are left alone.
    fn download_songbooks(&mut self, langs: Vec<String>) {
        let here = self.catalog.catalog.as_ref().map(|engine| engine.installed()).unwrap_or_default();
        let mut started: Vec<String> = Vec::new();
        for lang in langs {
            let wanted = catalog::valid_language(&lang)
                && !here.contains(&lang)
                && !started.contains(&lang)
                && !self.catalog.busy.iter().any(|(busy, _)| *busy == lang);
            if !wanted {
                continue;
            }
            match self.start_catalog_download(lang.clone(), None, window_download_words) {
                Ok(()) => started.push(lang),
                Err(problem) => {
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.notices.trouble(problem);
                    }
                }
            }
        }
        let name = |lang: &String| catalog::songbook(lang).map_or_else(|| lang.clone(), |book| book.name().to_owned());
        let said = match started.as_slice() {
            [] => return,
            [one] => tf(
                "notices.songbooks.downloading",
                "Downloading the {name} songbook from Worship Leader App in the background.",
                &[("name", &name(one))],
            ),
            many => tn(
                "notices.songbooks.downloading_several",
                many.len() as u64,
                "Downloading {n} songbook from Worship Leader App in the background: {names}.",
                "Downloading {n} songbooks from Worship Leader App in the background: {names}.",
                &[("names", &many.iter().map(name).collect::<Vec<_>>().join(", "))],
            ),
        };
        crate::applog::event("catalog", &format!("downloading {} for the window", started.join(", ")));
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.info(said);
        }
    }

    /// Takes a songbook off this computer, as Settings asked (it asked first). Songs already in
    /// the library stay. Done is seen in the list, so only a failure is said.
    fn remove_songbook(&mut self, lang: &str) {
        if self.catalog.busy.iter().any(|(busy, _)| busy == lang) {
            return;
        }
        let Ok(engine) = self.catalog_engine() else { return };
        self.catalog.stale = true;
        match engine.remove(lang) {
            Ok(_) => crate::applog::event("catalog", &format!("removed the {lang} songs from Settings")),
            Err(problem) => {
                let name = catalog::songbook(lang).map_or(lang, |book| book.name());
                let said = tf(
                    "notices.songbooks.remove_failed",
                    "The {name} songbook couldn\u{2019}t be removed: {problem}",
                    &[("name", name), ("problem", &problem)],
                );
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notices.trouble(said);
                }
            }
        }
    }

    /// Asks the source how big these songbooks are, on a thread of its own (a request that
    /// downloads nothing); the answer comes back to `handle_songbooks`.
    fn look_at_songbooks(&mut self, langs: Vec<String>) {
        let Ok(engine) = self.catalog_engine() else { return };
        let langs: Vec<String> = langs.into_iter().filter(|lang| catalog::valid_language(lang)).collect();
        if langs.is_empty() {
            return;
        }
        if let Some(running) = self.running.as_mut() {
            running.home_state.songbooks.known.looking.extend(langs.iter().cloned());
        }
        let tx = self.catalog.sizes_tx.clone();
        let spawned = std::thread::Builder::new().name("song catalog sizes".to_owned()).spawn(move || {
            let _ = tx.send(engine.sizes(&langs, true));
        });
        if let Err(problem) = spawned {
            crate::applog::problem("catalog", &format!("could not ask the source about sizes: {problem}"));
        }
    }

    /// Takes in what the catalog's threads have finished, starts the monthly check when it is due
    /// and nothing is on screen, and looks over the library's copies once a pack has been filed.
    /// Called from the idle loop; returns when it next wants to be called.
    pub(crate) fn tick_catalog(&mut self) -> Option<Instant> {
        while let Ok(done) = self.catalog.rx.try_recv() {
            self.catalog.busy.retain(|(lang, _)| !done.langs.contains(lang));
            self.catalog.stale = true;
            if done.installed {
                self.catalog.apply_pending = true;
            }
            crate::applog::event("catalog", done.result.as_ref().unwrap_or_else(|problem| problem));
            if let Some(job) = done.job
                && let Some((_, kept)) = self.jobs.iter_mut().find(|(have, _)| *have == job)
            {
                *kept = Some(done.result.clone());
            }
            if let Some(running) = self.running.as_mut() {
                // The monthly check says nothing of a check that found nothing new, or couldn't
                // be made (it tries again tomorrow).
                if !done.auto || (done.installed && done.result.is_ok()) {
                    running.home_state.notices.outcome(done.result);
                }
                running.home_window.request_redraw();
            }
        }
        let busy_at = Instant::now() + BUSY_RETRY;
        let mut wake: Option<Instant> = None;
        let mut soon = |at: Instant| wake = Some(wake.map_or(at, |have: Instant| have.min(at)));
        // The first look over the copies, a little after the app starts.
        if !self.catalog.looked_once && self.catalog.started.elapsed() >= FIRST_LOOK_AFTER {
            self.catalog.looked_once = true;
            self.catalog.apply_pending = true;
        }
        if self.catalog.apply_pending {
            match self.catalog_apply() {
                None => soon(busy_at),
                Some(done) => {
                    self.catalog.apply_pending = done.waiting > 0;
                    if done.waiting > 0 {
                        soon(busy_at);
                    }
                    self.say_catalog_applied(&done);
                }
            }
        }
        // The monthly check.
        if self.catalog.next_look <= Instant::now() {
            if self.catalog.busy.is_empty() && self.running.is_some() {
                if self.show.anything_live() {
                    self.catalog.next_look = busy_at;
                } else {
                    self.catalog.next_look = Instant::now() + LOOK_EVERY;
                    self.start_due_check();
                }
            } else {
                self.catalog.next_look = busy_at;
            }
        }
        soon(self.catalog.next_look);
        wake
    }

    /// Starts the check of every downloaded language that is due.
    fn start_due_check(&mut self) {
        let Some(engine) = self.catalog.catalog.clone() else { return };
        let due = engine.due(now());
        if due.is_empty() {
            return;
        }
        let langs = due.clone();
        let started = self.catalog_spawn(due, None, true, move |engine, _| check_langs(engine, &langs));
        if let Err(problem) = started {
            crate::applog::problem("catalog", &problem);
        }
    }

    /// One notice for a look over the copies: how many were brought up to date and how many have
    /// a newer version waiting. A song gone from the source is noted on the song, quietly.
    fn say_catalog_applied(&mut self, done: &Applied) {
        let mut parts = Vec::new();
        let source: &[(&str, &str)] = &[("source", catalog::SOURCE_NAME)];
        if done.updated > 0 {
            parts.push(tn(
                "notices.catalog.updated",
                done.updated as u64,
                "{n} song from {source} brought up to date",
                "{n} songs from {source} brought up to date",
                source,
            ));
        }
        if done.offered > 0 {
            parts.push(tn(
                "notices.catalog.offered",
                done.offered as u64,
                "{n} song you changed has a newer version in {source}; yours is kept",
                "{n} songs you changed have a newer version in {source}; yours are kept",
                source,
            ));
        }
        if parts.is_empty() {
            return;
        }
        let words = format!("{}.", parts.join(". "));
        crate::applog::event("catalog", &words);
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.info(words);
        }
    }

    /// `catalog …`: the song catalog from the command line.
    pub(crate) fn ctl_catalog(&mut self, args: &[String]) -> Result<Reply, String> {
        let word = args.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
        let rest: Vec<String> = args.iter().skip(2).cloned().collect();
        match word.as_str() {
            "" | "status" | "languages" => self.catalog_status(),
            "search" => self.catalog_search(&rest),
            "list" => self.catalog_list(&rest),
            "add" => self.catalog_add(&rest),
            "download" => self.catalog_download(&rest),
            "check" => self.catalog_check_now(),
            "remove" => self.catalog_remove(&rest),
            "newer" => Ok(self.catalog_newer_list()),
            "use-newer" => self.catalog_use_newer_cmd(&rest),
            "keep-mine" => self.catalog_keep_mine_cmd(&rest),
            "sizes" => self.catalog_sizes(&rest),
            other => Err(format!(
                "catalog doesn't know \u{201c}{other}\u{201d}. It takes: search WORDS, list LANG, add KEY, newer, use-newer SONG, keep-mine SONG, download LANG, check, remove LANG --yes, sizes [LANG…]."
            )),
        }
    }

    fn catalog_status(&self) -> Result<Reply, String> {
        let engine = self.catalog_engine()?;
        let languages = engine.languages();
        let joined = self.catalog_copies().len();
        let newer =
            self.catalog_copies().iter().filter(|(_, s)| s.catalog.as_ref().is_some_and(|o| o.newer.is_some())).count();
        let mut text = format!("Song catalog: songs from {} ({}).", catalog::SOURCE_NAME, catalog::SOURCE_SITE);
        if languages.is_empty() {
            text.push_str("\nNo language downloaded yet: sp catalog download LANG");
        }
        for info in &languages {
            let when = |seconds: i64| if seconds > 0 { catalog::date_of(seconds) } else { "never".to_owned() };
            text.push_str(&format!(
                "\n  {}  {}  {} songs, download {}, {} on disk, fetched {}, checked {}",
                info.lang,
                language_name(&info.lang),
                number(info.songs),
                size(info.pack_bytes),
                size(info.disk_bytes),
                when(info.fetched),
                when(info.checked)
            ));
            if info.gone > 0 {
                text.push_str(&format!(", {} gone from the source", info.gone));
            }
            if info.malformed > 0 {
                text.push_str(&format!(", {} files left out", info.malformed));
            }
            if let Some(problem) = &info.problem {
                text.push_str(&format!("\n    last check failed: {problem}"));
            }
        }
        for (lang, progress) in &self.catalog.busy {
            text.push_str(&format!(
                "\n  {lang}: {}",
                progress.per_cent().map_or_else(|| "working…".to_owned(), |per| format!("{per}%"))
            ));
        }
        if joined > 0 {
            text.push_str(&format!("\n{joined} songs from the catalog are in the library"));
            if newer > 0 {
                text.push_str(&format!(", {newer} with a newer version waiting"));
            }
            text.push('.');
        }
        let data = serde_json::json!({
            "source": {"name": catalog::SOURCE_NAME, "site": catalog::SOURCE_SITE, "library": catalog::LIBRARY_NAME},
            "languages": languages.iter().map(|i| serde_json::json!({
                "language": i.lang, "name": language_name(&i.lang), "songs": i.songs, "gone": i.gone,
                "malformed": i.malformed, "duplicates": i.duplicates,
                "download_bytes": i.pack_bytes, "disk_bytes": i.disk_bytes,
                "fetched": i.fetched, "checked": i.checked, "etag": i.etag, "last_modified": i.last_modified,
                "problem": i.problem,
            })).collect::<Vec<_>>(),
            "busy": self.catalog.busy.iter().map(|(lang, p)| serde_json::json!({"language": lang, "per_cent": p.per_cent()})).collect::<Vec<_>>(),
            "joined": joined, "newer": newer,
        });
        Ok(Reply::with(text, data))
    }

    fn catalog_search(&self, words: &[String]) -> Result<Reply, String> {
        let engine = self.catalog_engine()?;
        let mut typed = words.to_vec();
        let mut limit = 10_usize;
        if let Some(at) = typed.iter().position(|w| w == "--limit") {
            limit = typed
                .get(at + 1)
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| (1..=200).contains(n))
                .ok_or("--limit takes a number from 1 to 200.")?;
            typed.drain(at..at + 2);
        }
        let query = typed.join(" ");
        if query.trim().is_empty() {
            return Err("catalog search WORDS [--limit N]".to_owned());
        }
        let skip = self.catalog_joined_keys();
        let found = engine.search(&query, limit, &[], &skip);
        let mut text = format!("{} found in the catalog for \u{201c}{}\u{201d}", found.total, query.trim());
        if found.total > found.hits.len() {
            text.push_str(&format!(" (showing {})", found.hits.len()));
        }
        text.push(':');
        for hit in &found.hits {
            let also =
                if hit.versions.is_empty() { String::new() } else { format!("  also in {}", hit.versions.join(", ")) };
            text.push_str(&format!("\n  {}  {}  \u{2014}  {}  [{}]{also}", hit.lang, hit.title, hit.line, hit.key));
        }
        if found.hits.is_empty() {
            text = format!("Nothing in the catalog for \u{201c}{}\u{201d}.", query.trim());
        }
        let data =
            serde_json::json!({"found": found.total, "hits": found.hits.iter().map(hit_json).collect::<Vec<_>>()});
        Ok(Reply::with(text, data))
    }

    fn catalog_list(&self, args: &[String]) -> Result<Reply, String> {
        let engine = self.catalog_engine()?;
        let [lang, tail @ ..] = args else { return Err("catalog list LANG [FROM [COUNT]]".to_owned()) };
        let number_at =
            |at: usize, default: usize| tail.get(at).and_then(|n| n.parse::<usize>().ok()).unwrap_or(default);
        let (from, count) = (number_at(0, 0), number_at(1, 20).min(200));
        let hits = engine.list(lang, from, count);
        if hits.is_empty() {
            return Err(format!("No {lang} songs there: download them first (catalog download {lang})."));
        }
        let text = hits.iter().map(|h| format!("{}  {}  [{}]", h.lang, h.title, h.key)).collect::<Vec<_>>().join("\n");
        Ok(Reply::with(text, serde_json::json!({"from": from, "hits": hits.iter().map(hit_json).collect::<Vec<_>>()})))
    }

    /// A song key typed in full, or by its start when only one song has it.
    fn catalog_key(&self, typed: &str) -> Result<String, String> {
        let engine = self.catalog_engine()?;
        let typed = typed.trim();
        if let Some(lang) = catalog::lang_of_key(typed)
            && let Ok(Some(index)) = engine.index(lang)
            && index.entry(typed).is_some()
        {
            return Ok(typed.to_owned());
        }
        if typed.chars().count() < 4 {
            return Err("Give the song's key (catalog search shows it), or the start of it.".to_owned());
        }
        let mut found: Vec<String> = Vec::new();
        for index in engine.indexes() {
            found.extend(index.entries.iter().filter(|e| e.key.starts_with(typed)).map(|e| e.key.clone()));
        }
        match found.as_slice() {
            [one] => Ok(one.clone()),
            [] => Err(format!("The catalog has no song with the key {typed}.")),
            many => Err(format!(
                "{} songs start with that: {}…",
                many.len(),
                many.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            )),
        }
    }

    fn catalog_add(&mut self, args: &[String]) -> Result<Reply, String> {
        let mut words = args.to_vec();
        let mut second = None;
        if let Some(at) = words.iter().position(|w| w == "--second") {
            second = Some(words.get(at + 1).cloned().ok_or("--second takes a language: --second en")?);
            words.drain(at..(at + 2).min(words.len()));
        }
        let [typed] = words.as_slice() else { return Err("catalog add KEY [--second LANGUAGE]".to_owned()) };
        let key = self.catalog_key(typed)?;
        let joined = self.join_catalog_song(&key, second.as_deref())?;
        let mut said = if joined.already {
            format!("\u{201c}{}\u{201d} is in your library already.", joined.title)
        } else {
            format!("\u{201c}{}\u{201d} is in your library now, in the library {}.", joined.title, joined.library)
        };
        if let Some(lang) = &joined.pairing.second {
            said.push_str(&format!(" With its {} version below.", language_name(lang)));
            for group in &joined.pairing.only_main {
                said.push_str(&format!(
                    " {group} shows one language only: the {} version has no such group.",
                    language_name(lang)
                ));
            }
        }
        let data = serde_json::json!({
            "song": joined.title, "id": self.show.presentations().get(joined.index).map(|s| s.id.to_string()),
            "library": joined.library, "already": joined.already, "key": key,
            "second": joined.pairing.second, "only_main": joined.pairing.only_main, "only_second": joined.pairing.only_second,
        });
        Ok(Reply::with(said, data))
    }

    fn catalog_download(&mut self, args: &[String]) -> Result<Reply, String> {
        self.only_at_the_computer("Downloading songs")?;
        let [lang] = args else { return Err("catalog download LANGUAGE   (such as ru, uk, en)".to_owned()) };
        let lang = lang.to_lowercase();
        if !catalog::valid_language(&lang) {
            return Err(format!(
                "\u{201c}{lang}\u{201d} isn't a language code (two or three small letters, such as ru)."
            ));
        }
        if self.catalog.busy.iter().any(|(busy, _)| *busy == lang) {
            return Err(format!("The {lang} songs are being downloaded already."));
        }
        let job = self.start_job();
        self.start_catalog_download(lang.clone(), Some(job), |lang, outcome, seconds| {
            outcome
                .map(|outcome| {
                    let took = crate::i18n::number(f64::from(seconds), 1);
                    format!("{} ({took} {})", outcome_words(lang, outcome), t("common.seconds_unit", "s"))
                })
                .map_err(str::to_owned)
        })?;
        Ok(Reply::done(format!(
            "Downloading the {} songs in the background (job {job}); sp catalog shows how far it has got.",
            language_name(&lang)
        )))
    }

    fn catalog_check_now(&mut self) -> Result<Reply, String> {
        self.only_at_the_computer("Checking for newer songs")?;
        let engine = self.catalog_engine()?;
        let langs = engine.installed();
        if langs.is_empty() {
            return Err("No language is downloaded yet: sp catalog download LANG".to_owned());
        }
        if !self.catalog.busy.is_empty() {
            return Err("A download or a check is under way already.".to_owned());
        }
        let job = self.start_job();
        let named = langs.clone();
        self.catalog_spawn(langs, Some(job), false, move |engine, _| check_langs(engine, &named))?;
        Ok(Reply::done(format!(
            "Asking the source about {} language(s) in the background (job {job}).",
            engine.installed().len()
        )))
    }

    fn catalog_remove(&mut self, args: &[String]) -> Result<Reply, String> {
        self.only_at_the_computer("Removing songs")?;
        let yes = args.iter().any(|w| w == "--yes");
        let langs: Vec<&String> = args.iter().filter(|w| *w != "--yes").collect();
        let [lang] = langs.as_slice() else { return Err("catalog remove LANGUAGE --yes".to_owned()) };
        let lang = lang.to_lowercase();
        let engine = self.catalog_engine()?;
        if !engine.installed().contains(&lang) {
            return Err(format!("The {lang} songs aren't downloaded."));
        }
        if self.catalog.busy.iter().any(|(busy, _)| *busy == lang) {
            return Err(format!("The {lang} songs are being downloaded; wait for that to end."));
        }
        let name = language_name(&lang);
        let copies = self
            .catalog_copies()
            .iter()
            .filter(|(_, song)| song.catalog.as_ref().is_some_and(|o| o.language == lang))
            .count();
        if !yes {
            return Err(format!(
                "That takes the {name} songs off this computer ({copies} already in your library stay). Add --yes to do it."
            ));
        }
        engine.remove(&lang)?;
        crate::applog::event("catalog", &format!("removed the {lang} songs"));
        Ok(Reply::done(format!(
            "Removed the {name} songs from the catalog. The {copies} already in your library stay."
        )))
    }

    fn catalog_newer_list(&self) -> Reply {
        let offers: Vec<(usize, &Presentation)> = self
            .catalog_copies()
            .into_iter()
            .filter(|(_, song)| song.catalog.as_ref().is_some_and(|o| o.newer.is_some() || o.gone))
            .collect();
        let line = |song: &Presentation| {
            let origin = song.catalog.as_ref();
            match origin {
                Some(o) if o.gone => {
                    format!("  {}  is no longer in {} (yours stays)", song.title, catalog::SOURCE_NAME)
                }
                _ => format!("  {}  has a newer version in {}; yours is kept", song.title, catalog::SOURCE_NAME),
            }
        };
        let text = if offers.is_empty() {
            "No song has a newer version waiting.".to_owned()
        } else {
            offers.iter().map(|(_, song)| line(song)).collect::<Vec<_>>().join("\n")
        };
        let data = serde_json::json!({"songs": offers.iter().map(|(_, song)| {
            let o = song.catalog.as_ref();
            serde_json::json!({
                "song": song.title, "id": song.id.to_string(), "key": o.map(|o| o.key.clone()),
                "newer": o.is_some_and(|o| o.newer.is_some()), "gone": o.is_some_and(|o| o.gone),
            })
        }).collect::<Vec<_>>()});
        Reply::with(text, data)
    }

    fn catalog_use_newer_cmd(&mut self, args: &[String]) -> Result<Reply, String> {
        let typed = args.join(" ");
        if typed.trim().is_empty() {
            return Err("catalog use-newer SONG".to_owned());
        }
        let at = self.find_song_as_is(&typed)?;
        self.catalog_use_newer_version(at).map(Reply::done)
    }

    fn catalog_keep_mine_cmd(&mut self, args: &[String]) -> Result<Reply, String> {
        let typed = args.join(" ");
        if typed.trim().is_empty() {
            return Err("catalog keep-mine SONG".to_owned());
        }
        let at = self.find_song_as_is(&typed)?;
        self.catalog_keep_mine(at).map(Reply::done)
    }

    /// `catalog sizes [LANG…] [--ask]`: what setup shows for each language: the songs, the
    /// download and the room it takes. Languages already downloaded answer from the last fetch;
    /// the others are asked about at the source (a request that downloads nothing) with `--ask`,
    /// as a job.
    fn catalog_sizes(&mut self, args: &[String]) -> Result<Reply, String> {
        let engine = self.catalog_engine()?;
        let ask = args.iter().any(|w| w == "--ask");
        let mut langs: Vec<String> = args.iter().filter(|w| *w != "--ask").map(|w| w.to_lowercase()).collect();
        if langs.is_empty() {
            langs = engine.installed();
        }
        if let Some(bad) = langs.iter().find(|lang| !catalog::valid_language(lang)) {
            return Err(format!(
                "\u{201c}{bad}\u{201d} isn't a language code (two or three small letters, such as ru)."
            ));
        }
        let describe = move |engine: &Catalog, langs: &[String], ask: bool| {
            let sizes = engine.sizes(langs, ask);
            let text = sizes
                .iter()
                .map(|s| {
                    format!(
                        "  {}  {}  {}, download {}, {} on disk{}",
                        s.lang,
                        language_name(&s.lang),
                        s.songs.map_or_else(|| "songs not counted yet".to_owned(), |n| format!("{} songs", number(n))),
                        s.download_bytes.map_or_else(|| "size not known".to_owned(), size),
                        size(s.disk_bytes),
                        s.problem.as_ref().map(|p| format!(" ({p})")).unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            (text, sizes)
        };
        if !ask {
            let (text, sizes) = describe(&engine, &langs, false);
            return Ok(Reply::with(
                if text.is_empty() { "No language is downloaded yet.".to_owned() } else { text },
                sizes_json(&sizes),
            ));
        }
        let job = self.start_job();
        self.catalog_spawn(Vec::new(), Some(job), false, move |engine, _| {
            (Ok(describe(engine, &langs, true).0), false)
        })?;
        Ok(Reply::done(format!("Asking the source how big each is, in the background (job {job}).")))
    }
}

/// What a download the songbook windows started came to, for the notices, in the app's language.
fn window_download_words(lang: &str, outcome: Result<&Outcome, &str>, _seconds: f32) -> Result<String, String> {
    let name = catalog::songbook(lang).map_or(lang, |book| book.name());
    match outcome {
        Ok(Outcome::Installed(done)) => Ok(tf(
            "notices.songbooks.done",
            "The {name} songbook is on this computer: {songs} from Worship Leader App.",
            &[("name", name), ("songs", &crate::songbooks::songs_words(done.songs))],
        )),
        Ok(Outcome::Unchanged { .. }) => {
            Ok(tf("notices.songbooks.unchanged", "The {name} songbook is up to date.", &[("name", name)]))
        }
        Err(problem) => Err(tf(
            "notices.songbooks.failed",
            "The {name} songbook didn\u{2019}t download: {problem}",
            &[("name", name), ("problem", problem)],
        )),
    }
}

fn sizes_json(sizes: &[catalog::Size]) -> serde_json::Value {
    serde_json::json!({"languages": sizes.iter().map(|s| serde_json::json!({
        "language": s.lang, "songs": s.songs, "download_bytes": s.download_bytes,
        "disk_bytes": s.disk_bytes, "installed": s.installed, "problem": s.problem,
    })).collect::<Vec<_>>()})
}

/// Asks the source about each of these languages in turn: the words for the job, and whether a
/// pack was filed (so the copies in the library are looked over).
fn check_langs(engine: &Catalog, langs: &[String]) -> (Result<String, String>, bool) {
    let mut said = Vec::new();
    let (mut installed, mut failed) = (false, 0);
    for (lang, result) in engine.check(langs, now()) {
        match result {
            Ok(outcome) => {
                installed |= matches!(outcome, Outcome::Installed(_));
                said.push(outcome_words(&lang, &outcome));
            }
            Err(problem) => {
                failed += 1;
                said.push(tf(
                    "notices.catalog.not_checked",
                    "The {language} songs could not be checked: {error}",
                    &[("language", &language_shown(&lang)), ("error", &problem)],
                ));
            }
        }
    }
    let words = said.join(" ");
    if failed == langs.len() && failed > 0 { (Err(words), false) } else { (Ok(words), installed) }
}
