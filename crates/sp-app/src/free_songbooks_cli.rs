//! The app's side of the free songbooks shared by churches (the list, the download and the
//! windows are `free_songbooks`): `sp songbooks` and `sp songbook install|remove`, the Free
//! Songbooks window's and the setup's asks, and bringing a songbook's songs into the library.
//!
//! A download, its check and reading the file run on a thread of their own; the songs then come
//! into the library on the main thread through `App::take_songs`, as File › Import Songs brings
//! a songbook's, and only while nothing is on screen (the songs wait until then, and the window
//! says so). Nothing about the window changes when they come: no song is chosen, nothing open is
//! closed.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use sp_model::Presentation;

use crate::catalog::Progress;
use crate::control::Reply;
use crate::free_songbooks::{self, Ask, Offered};
use crate::i18n::{t, tf, tn};

/// How often songs waiting for the screens to clear are looked at again.
const HOLD_RETRY: Duration = Duration::from_secs(2);

/// What a songbook's thread finished: the songs read and marked, or why not.
struct Done {
    book: &'static Offered,
    job: Option<u64>,
    result: Result<Vec<Presentation>, String>,
}

/// The free songbooks' downloads in the app.
pub struct Installs {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    /// Downloads under way, with how far each has got.
    busy: Vec<(&'static str, Arc<Progress>)>,
    /// Songbooks downloaded and read, whose songs wait for nothing to be on screen.
    held: Vec<Done>,
}

impl Default for Installs {
    fn default() -> Installs {
        let (tx, rx) = channel();
        Installs { tx, rx, busy: Vec::new(), held: Vec::new() }
    }
}

impl std::fmt::Debug for Installs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Installs(busy {}, held {})", self.busy.len(), self.held.len())
    }
}

impl Installs {
    /// Downloading, or downloaded and waiting for the screens.
    fn under_way(&self, id: &str) -> bool {
        self.busy.iter().any(|(busy, _)| *busy == id) || self.held.iter().any(|done| done.book.id == id)
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// What `sp` and the window say of a songbook installed already.
fn already_installed(book: &Offered, songs: usize) -> String {
    tf(
        "freesongbooks.already",
        "{name} is installed already ({songs}). To install it again, remove it first: Remove in the Free Songbooks window, or sp songbook remove {id} --yes.",
        &[("name", book.name), ("songs", &free_songbooks::songs_words(songs)), ("id", book.id)],
    )
}

impl crate::App {
    /// The songs in the library that came from a free songbook: their places and songs, leaving
    /// out songs deleted and the app's own presentations.
    fn songbook_songs(&self) -> Vec<(usize, &Presentation)> {
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        self.show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(at, song)| {
                !self.app_made(*at) && !removed.contains(at) && free_songbooks::songbook_of(song).is_some()
            })
            .map(|(at, song)| (at, song.as_ref()))
            .collect()
    }

    /// The songbooks installed, with the songs in each one's library (or, with its library gone,
    /// the songs from it still in the library).
    pub(crate) fn songbooks_installed(&self) -> BTreeMap<&'static str, usize> {
        let mut marked: BTreeMap<&'static str, usize> = BTreeMap::new();
        for (_, song) in self.songbook_songs() {
            if let Some(book) = free_songbooks::songbook_of(song) {
                *marked.entry(book.id).or_default() += 1;
            }
        }
        let libraries = self.running.as_ref().map(|r| r.home_state.libraries.as_slice()).unwrap_or_default();
        for (id, songs) in &mut marked {
            let name = free_songbooks::by_id(id).map_or("", |book| book.name);
            if let Some(library) = libraries.iter().find(|library| library.name == name) {
                *songs = (*songs).max(library.songs.len());
            }
        }
        marked
    }

    /// Starts downloading a songbook on a thread of its own: fetched, checked against the size
    /// and checksum this version expects, and read into songs there. Its songs come into the
    /// library when it is done (see [`App::tick_free_songbooks`]). Refused for one installed
    /// already, or on its way.
    pub(crate) fn install_free_songbook(&mut self, book: &'static Offered, job: Option<u64>) -> Result<(), String> {
        if let Some(songs) = self.songbooks_installed().get(book.id) {
            return Err(already_installed(book, *songs));
        }
        if self.free_songbooks.under_way(book.id) {
            return Err(tf("freesongbooks.busy", "{name} is being downloaded already.", &[("name", book.name)]));
        }
        let folder = crate::bible_store::data_dir()
            .ok_or_else(|| t("notices.no_library_folder", "Could not find the library folder.").to_owned())?;
        let progress = Progress::new();
        let (tx, waker, watched, base) =
            (self.free_songbooks.tx.clone(), self.waker.clone(), progress.clone(), free_songbooks::base());
        let spawned = std::thread::Builder::new().name("songbook download".to_owned()).spawn(move || {
            let result = free_songbooks::download(&base, book, Some(&watched))
                .and_then(|bytes| free_songbooks::read(book, &bytes, &folder, now()));
            let _ = tx.send(Done { book, job, result });
            if let Some(waker) = waker {
                let _ = waker.send_event(crate::AppEvent::SongbookDone);
            }
        });
        if spawned.is_err() {
            return Err(t("freesongbooks.not_started", "Could not start the download.").to_owned());
        }
        self.free_songbooks.busy.push((book.id, progress));
        crate::applog::event("songbooks", &format!("downloading {} ({})", book.id, book.name));
        Ok(())
    }

    /// Takes in the downloads that have ended: a failure is said at once; a songbook read waits
    /// until nothing is on screen, and its songs then come into the library. Called from the idle
    /// loop and when a download's thread wakes the app; returns when it next wants to be called.
    pub(crate) fn tick_free_songbooks(&mut self) -> Option<Instant> {
        while let Ok(done) = self.free_songbooks.rx.try_recv() {
            self.free_songbooks.busy.retain(|(id, _)| *id != done.book.id);
            match &done.result {
                Ok(_) => self.free_songbooks.held.push(done),
                Err(problem) => {
                    let said = tf(
                        "freesongbooks.failed",
                        "{name} wasn\u{2019}t installed: {problem}",
                        &[("name", done.book.name), ("problem", problem)],
                    );
                    crate::applog::problem("songbooks", &said);
                    self.finish_songbook_job(done.job, Err(said.clone()));
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.free_songbooks.problem = Some(said.clone());
                        running.home_state.notices.trouble(said);
                        running.home_window.request_redraw();
                    }
                }
            }
        }
        if self.free_songbooks.held.is_empty() {
            return None;
        }
        if self.show.anything_live() || self.running.is_none() {
            return Some(Instant::now() + HOLD_RETRY);
        }
        for done in std::mem::take(&mut self.free_songbooks.held) {
            self.take_in_songbook(done);
        }
        None
    }

    fn finish_songbook_job(&mut self, job: Option<u64>, result: Result<String, String>) {
        if let Some(job) = job
            && let Some((_, kept)) = self.jobs.iter_mut().find(|(have, _)| *have == job)
        {
            *kept = Some(result);
        }
    }

    /// Brings a songbook's songs into the library as File › Import Songs brings a songbook's
    /// (`take_songs`): each into the Songs folder and into a library named after the songbook
    /// (made if there isn't one); one the library has already, word for word under the same
    /// title, isn't added again, and goes into that library too. The window is left as it is.
    fn take_in_songbook(&mut self, done: Done) {
        let Done { book, job, result } = done;
        let Ok(songs) = result else { return };
        let started = Instant::now();
        let path = PathBuf::from(format!("{}.sps", book.id));
        let read: Vec<crate::SongRead> = songs
            .into_iter()
            .map(|song| crate::SongRead::Song {
                song: Ok(song),
                path: path.clone(),
                library: Some(book.name.to_owned()),
            })
            .collect();
        let to = crate::SongsTo { quiet: true, ..crate::SongsTo::default() };
        let mut imported = crate::Imported::default();
        self.take_songs(read, &to, &mut imported);
        let new = imported.titles.len();
        let landed = imported.landed.len();
        crate::applog::event(
            "songbooks",
            &format!(
                "installed {}: {new} songs new, {} in the library already, in {} ms",
                book.id,
                landed.saturating_sub(new),
                started.elapsed().as_millis()
            ),
        );
        let songs = free_songbooks::songs_words(landed);
        let said = if landed > new {
            tn(
                "freesongbooks.installed_some_here",
                (landed - new) as u64,
                "Installed {name}: {songs}, in the library {name}; {n} of them was in your library already.",
                "Installed {name}: {songs}, in the library {name}; {n} of them were in your library already.",
                &[("name", book.name), ("songs", &songs)],
            )
        } else {
            tf(
                "freesongbooks.installed",
                "Installed {name}: {songs}, in the library {name}.",
                &[("name", book.name), ("songs", &songs)],
            )
        };
        self.finish_songbook_job(job, Ok(said.clone()));
        if let Some(running) = self.running.as_mut() {
            running.home_state.notices.info(said);
            running.home_window.request_redraw();
        }
    }

    /// Takes a free songbook back out of the library: its songs nobody changed and nothing else
    /// uses go to the Trash (all together, in a folder named after the songbook), and its
    /// library goes once empty; the songs the church changed, and those a playlist or another
    /// library names, stay as the church's own (no longer marked as the songbook's), so it can be
    /// installed again. Refused while anything is on screen.
    pub(crate) fn remove_free_songbook(&mut self, book: &'static Offered) -> Result<String, String> {
        if self.show.anything_live() {
            return Err(t(
                "freesongbooks.remove_live",
                "Something is on screen: remove the songbook once the screens are clear.",
            )
            .to_owned());
        }
        if self.free_songbooks.under_way(book.id) {
            return Err(tf(
                "freesongbooks.remove_busy",
                "{name} is being downloaded; wait for that to end.",
                &[("name", book.name)],
            ));
        }
        let Some(running) = self.running.as_ref() else { return Err("The window isn't open yet.".to_owned()) };
        let state = &running.home_state;
        let songs: Vec<free_songbooks::InLibrary> = self
            .songbook_songs()
            .into_iter()
            .filter(|(_, song)| free_songbooks::songbook_of(song) == Some(book))
            .map(|(at, song)| free_songbooks::InLibrary {
                at,
                key: state.song_keys.get(at).cloned().unwrap_or_default(),
                untouched: free_songbooks::untouched(song),
            })
            .collect();
        if songs.is_empty() {
            return Err(tf("freesongbooks.not_installed", "{name} isn\u{2019}t installed.", &[("name", book.name)]));
        }
        // What else names a song: a playlist, or a library other than the songbook's own.
        let mut used: HashSet<String> = HashSet::new();
        for list in &state.services.list {
            for item in &list.items {
                if let crate::service::Item::Song { key, .. } = item {
                    used.insert(key.clone());
                }
            }
        }
        for library in state.libraries.iter().filter(|library| library.name != book.name) {
            used.extend(library.songs.iter().cloned());
        }
        let editing = state.editing.as_ref().map(|draft| draft.at.presentation);
        let (gone, mut kept) = free_songbooks::removal(&songs, &used, editing);
        // The songs' files go into a folder of their own (a move on the same disk, quick), and
        // that folder to the Trash on a thread of its own.
        let own = crate::bible_store::data_dir().map(|dir| dir.join("Songs"));
        let staging = crate::bible_store::data_dir()
            .map(|dir| dir.join(format!(".removing-songbook-{}", sp_model::Uuid::new_v4())));
        let folder = staging.as_ref().map(|staging| {
            staging.join(tf("freesongbooks.trash_folder", "{name} (removed songbook)", &[("name", &trash_name(book))]))
        });
        if let Some(folder) = &folder {
            let _ = std::fs::create_dir_all(folder);
        }
        let mut taken = Vec::new();
        for at in gone {
            let moved = match (self.sources.get(at), &own, &folder) {
                (Some(Some(path)), Some(own), Some(folder)) if path.starts_with(own) && path.exists() => {
                    let name = path.file_name().map(PathBuf::from).unwrap_or_default();
                    std::fs::rename(path, folder.join(name)).is_ok()
                }
                // Not written yet (just installed), or no file of its own: nothing to move.
                (Some(Some(path)), _, _) => !path.exists(),
                _ => true,
            };
            if moved {
                taken.push(at);
            } else {
                kept.push(at);
            }
        }
        for &at in &taken {
            self.unsaved.retain(|have| *have != at);
            self.history.forget(at);
        }
        for &at in &kept {
            if let Some(song) = self.show.presentations().get(at) {
                let mut own_song = song.as_ref().clone();
                own_song.catalog = None;
                self.replace_song_quietly(at, own_song);
            }
        }
        let Some(running) = self.running.as_mut() else { return Err("The window isn't open yet.".to_owned()) };
        let state = &mut running.home_state;
        let keys: HashSet<String> = taken.iter().filter_map(|at| state.song_keys.get(*at).cloned()).collect();
        state.removed_songs.extend(taken.iter().copied());
        for library in &mut state.libraries {
            let before = library.songs.len();
            library.songs.retain(|key| !keys.contains(key));
            state.libraries_changed |= library.songs.len() != before;
        }
        // Its library goes once nothing is left in it.
        if let Some(at) =
            state.libraries.iter().position(|library| library.name == book.name && library.songs.is_empty())
        {
            let id = state.libraries.remove(at).id;
            if state.library_shown == Some(id) {
                state.library_shown = None;
            }
            state.libraries_changed = true;
        }
        running.home_window.request_redraw();
        // Another song is shown if the one shown went.
        if taken.contains(&self.show.selected()) {
            let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
            if let Some(next) =
                (0..self.show.presentations().len()).find(|at| !removed.contains(at) && !self.app_made(*at))
            {
                self.apply(vec![sp_engine::Command::Select(next)]);
            }
        }
        if let (Some(staging), Some(folder)) = (staging, folder) {
            let spawned = std::thread::Builder::new().name("songbook to the Trash".to_owned()).spawn(move || {
                if let Err(problem) = crate::sweep::to_trash(&folder) {
                    crate::applog::problem(
                        "songbooks",
                        &format!("could not move removed songs to the Trash: {problem}"),
                    );
                    return;
                }
                let _ = std::fs::remove_dir(&staging);
            });
            if let Err(problem) = spawned {
                crate::applog::problem("songbooks", &format!("could not start moving songs to the Trash: {problem}"));
            }
        }
        crate::applog::event(
            "songbooks",
            &format!("removed {}: {} songs gone, {} kept", book.id, taken.len(), kept.len()),
        );
        let gone_words = free_songbooks::songs_words(taken.len());
        Ok(if kept.is_empty() {
            tf(
                "freesongbooks.removed",
                "Removed {name}: {songs} went to the Trash.",
                &[("name", book.name), ("songs", &gone_words)],
            )
        } else {
            tf(
                "freesongbooks.removed_some_kept",
                "Removed {name}: {songs} went to the Trash; {kept} stay in your library as yours (you changed them, or a playlist or another library uses them).",
                &[("name", book.name), ("songs", &gone_words), ("kept", &crate::i18n::count(kept.len() as u64))],
            )
        })
    }

    /// Carries out what the Free Songbooks window and the setup asked, and tells them how the
    /// songbooks stand. Called after each drawing of the window.
    pub(crate) fn handle_free_songbooks(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let asks = std::mem::take(&mut running.home_state.free_songbooks.asks);
        let showing = running.home_state.free_songbooks.open || running.home_state.setup.is_some();
        for ask in asks {
            let (said, failed) = match ask {
                Ask::Install(id) => {
                    let Some(book) = free_songbooks::by_id(id) else { continue };
                    match self.install_free_songbook(book, None) {
                        Ok(()) => (
                            tf(
                                "freesongbooks.downloading_notice",
                                "Downloading {name} in the background; its songs come into your library when it is done.",
                                &[("name", book.name)],
                            ),
                            false,
                        ),
                        Err(problem) => (problem, true),
                    }
                }
                Ask::Remove(id) => {
                    let Some(book) = free_songbooks::by_id(id) else { continue };
                    match self.remove_free_songbook(book) {
                        Ok(said) => (said, false),
                        Err(problem) => (problem, true),
                    }
                }
            };
            if let Some(running) = self.running.as_mut() {
                let state = &mut running.home_state;
                if failed {
                    state.free_songbooks.problem = Some(said.clone());
                    state.notices.trouble(said);
                } else {
                    state.notices.info(said);
                }
            }
        }
        let installed = showing.then(|| self.songbooks_installed());
        let busy = self.free_songbooks.busy.iter().map(|(id, progress)| (*id, progress.per_cent())).collect();
        let held = self.free_songbooks.held.iter().map(|done| done.book.id).collect();
        let Some(running) = self.running.as_mut() else { return };
        let known = &mut running.home_state.free_songbooks.known;
        known.busy = busy;
        known.held = held;
        if let Some(installed) = installed {
            known.installed = installed;
        }
    }

    /// `songbooks`: the free songbooks shared by churches, each with whether it is installed.
    pub(crate) fn ctl_songbooks(&self) -> Result<Reply, String> {
        let installed = self.songbooks_installed();
        let mut text = String::from("Free songbooks shared by churches:");
        let mut rows = Vec::new();
        for book in free_songbooks::OFFERED {
            let busy = self.free_songbooks.busy.iter().find(|(id, _)| *id == book.id);
            let held = self.free_songbooks.held.iter().any(|done| done.book.id == book.id);
            let state = if let Some((_, progress)) = busy {
                progress.per_cent().map_or_else(|| "downloading".to_owned(), |per| format!("downloading {per}%"))
            } else if held {
                "downloaded; its songs come in once nothing is on screen".to_owned()
            } else if let Some(songs) = installed.get(book.id) {
                format!("installed ({songs} songs in its library)")
            } else {
                "not installed".to_owned()
            };
            text.push_str(&format!(
                "\n  {:<17} {:<10} {}  {} songs, {}  {state}",
                book.id,
                free_songbooks::language_english(book.language),
                book.name,
                book.songs,
                size_said(book.bytes)
            ));
            rows.push(serde_json::json!({
                "id": book.id, "name": book.name, "language": book.language, "songs": book.songs,
                "bytes": book.bytes, "sha256": book.sha256, "installed": installed.contains_key(book.id),
                "library_songs": installed.get(book.id), "downloading": busy.is_some(), "waiting": held,
                "per_cent": busy.and_then(|(_, progress)| progress.per_cent()),
            }));
        }
        text.push_str("\nsp songbook install NAME installs one; sp songbook remove NAME --yes takes it out again.");
        text.push_str("\nThe songs belong to their writers; a church's licence (such as CCLI) covers showing them in its services.");
        Ok(Reply::with(text, serde_json::json!({ "songbooks": rows })))
    }

    /// `songbook install NAME` | `songbook remove NAME --yes`.
    pub(crate) fn ctl_songbook(&mut self, args: &[String]) -> Result<Reply, String> {
        let word = args.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
        let yes = args.iter().any(|w| w == "--yes");
        let typed: Vec<&str> = args.iter().skip(2).filter(|w| *w != "--yes").map(String::as_str).collect();
        let usage = "songbook install NAME | songbook remove NAME --yes   (sp songbooks lists them)";
        let find = || {
            let typed = typed.join(" ");
            if typed.trim().is_empty() {
                return Err(usage.to_owned());
            }
            free_songbooks::find(&typed).ok_or_else(|| {
                format!("No free songbook is called \u{201c}{}\u{201d}. sp songbooks lists them.", typed.trim())
            })
        };
        match word.as_str() {
            "install" => {
                self.only_at_the_computer(crate::remote::policy::AtComputer::DownloadSongs)?;
                let book = find()?;
                let job = self.start_job();
                if let Err(problem) = self.install_free_songbook(book, Some(job)) {
                    // Nothing started: the job is forgotten.
                    self.jobs.retain(|(have, _)| *have != job);
                    self.last_job = None;
                    return Err(problem);
                }
                Ok(Reply::done(format!(
                    "Downloading {} ({}) in the background; its {} songs come into the library {} when it is done, once nothing is on screen.",
                    book.name,
                    size_said(book.bytes),
                    book.songs,
                    book.name
                )))
            }
            "remove" => {
                self.only_at_the_computer(crate::remote::policy::AtComputer::RemoveSongs)?;
                let book = find()?;
                let songs = self.songbooks_installed().get(book.id).copied();
                let Some(songs) = songs else { return Err(format!("{} isn't installed.", book.name)) };
                if !yes {
                    return Err(format!(
                        "That takes {}'s songs ({songs} in its library) out of your library, to the Trash, except songs you have changed and songs a playlist or another library uses. Add --yes to do it.",
                        book.name
                    ));
                }
                self.remove_free_songbook(book).map(Reply::done)
            }
            _ => Err(usage.to_owned()),
        }
    }
}

/// A download's size as `sp` says it: "22 KB", "4.7 MB".
fn size_said(bytes: u64) -> String {
    if bytes < 1_000_000 {
        format!("{} KB", bytes.div_ceil(1000))
    } else {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    }
}

/// A songbook's name as a folder in the Trash: its letters, digits, spaces and hyphens, nothing a
/// file system keeps for itself.
fn trash_name(book: &Offered) -> String {
    book.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect()
}
