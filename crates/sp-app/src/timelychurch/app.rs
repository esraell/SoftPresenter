//! The app's side of TimelyChurch: what the card asks, a check's findings put into the library
//! and the playlists, the songs shown in services counted for TimelyChurch, and the card kept
//! in step. Runs on the window's thread, from the idle loop; the network is `sync`'s threads'.
//!
//! **Songs** join the library (and its library named TimelyChurch) as ordinary songs with the
//! mark. An update replaces a song nobody has touched, keeping its id, theme and transition; a
//! song the church changed is kept, and the newer version offered (`sp timelychurch newer`).
//!
//! **Playlists** carry a mark too (`service::ServiceOrigin`). A playlist nobody has touched is
//! made again when its service changes; one the church changed is kept and the change offered;
//! one the church deleted is not made again; a service removed in TimelyChurch leaves its
//! playlist, noted as removed. A new song or playlist comes at once; changes wait while
//! anything is on screen, and a song open in the editor waits until it is closed.
//!
//! **Song uses:** a TimelyChurch song counts as shown when it goes on screen from a playlist
//! made from a TimelyChurch service (the playlist in front), on the service's date. Each song
//! counts once per service, whatever happens after (shown again, the app started again), so a
//! rehearsal on the day and the service itself are one use, and showing it on another day is
//! none.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use sp_model::Presentation;

use super::store::SongSeen;
use super::sync::{Said, Status};
use super::view::{Request, RowState, ServiceRow};
use super::wire::{Id, SongUse};
use crate::catalog::{self, Verdict};
use crate::i18n::{tf, tn};
use crate::service::{Service, ServiceOrigin};

/// How soon what waits to be applied (something was on screen) is looked at again.
const APPLY_AGAIN: Duration = Duration::from_secs(20);

/// The fingerprint of a playlist as a person sees it: its name and items.
fn playlist_fingerprint(service: &Service) -> String {
    super::fingerprint_of(&(&service.name, &service.items))
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// What applying a check did, for the notices.
#[derive(Debug, Default)]
struct Done {
    songs_added: usize,
    updated: usize,
    offered: usize,
    new_playlists: Vec<String>,
    playlists_offered: Vec<String>,
    removed: Vec<String>,
}

impl crate::App {
    /// Does what the card asked, takes in what the connection's threads found, applies it, and
    /// counts songs shown. Called from the idle loop; says when it next wants to be called.
    pub(crate) fn tick_timelychurch(&mut self) -> Option<Instant> {
        if !self.timelychurch.offered() {
            return None;
        }
        self.timelychurch_requests();
        let mut wake = self.timelychurch.tick();
        let said = std::mem::take(&mut self.timelychurch.said);
        if let Some(running) = self.running.as_mut() {
            for said in said {
                match said {
                    Said::Connected(church) => running.home_state.notices.info(tf(
                        "notices.timelychurch.connected",
                        "Connected to TimelyChurch: {church}. Its upcoming services come in as playlists.",
                        &[("church", &church)],
                    )),
                    Said::Disconnected => running.home_state.notices.trouble(super::sync::disconnected_words()),
                    Said::Trouble(why) => running.home_state.notices.trouble(why),
                }
            }
        }
        if self.timelychurch.check_due() && self.running.is_some() {
            let keys = self.timelychurch_copies().into_iter().map(|(_, key)| key).collect::<HashSet<String>>();
            self.timelychurch.start_check(keys, today(), now());
            wake = soonest(wake, Instant::now() + Duration::from_millis(250));
        }
        // What waits to be applied (something was on screen) is looked at again now and then,
        // not each time the app wakes.
        let due = self.timelychurch.apply_at.is_none_or(|at| at <= Instant::now());
        if self.timelychurch.waiting.is_some() && self.running.is_some() && due {
            self.timelychurch.apply_at =
                if self.timelychurch_apply() { None } else { Some(Instant::now() + APPLY_AGAIN) };
        }
        if let Some(at) = self.timelychurch.apply_at.filter(|_| self.timelychurch.waiting.is_some()) {
            wake = soonest(wake, at);
        }
        self.timelychurch_count_uses();
        self.timelychurch_view();
        wake
    }

    /// What the card asked in the last frame.
    fn timelychurch_requests(&mut self) {
        let Some(request) = self.running.as_mut().and_then(|r| r.home_state.timelychurch.request.take()) else {
            return;
        };
        let outcome = match request {
            Request::Connect { address, code } => self.timelychurch.pair(&address, &code),
            Request::CheckNow => self.timelychurch.check_now(),
            Request::Disconnect => {
                self.timelychurch.disconnect();
                crate::applog::event("timelychurch", "the token was forgotten on this computer");
                Ok(())
            }
        };
        if let Some(running) = self.running.as_mut() {
            running.home_state.timelychurch.problem = outcome.err();
            running.home_window.request_redraw();
        }
    }

    /// The places of the TimelyChurch songs in the library, by their marks' keys. Should two
    /// share a key (a song duplicated keeps its mark), the first, the one made, is the one.
    fn timelychurch_places(&self) -> HashMap<String, usize> {
        let mut places = HashMap::new();
        for (at, key) in self.timelychurch_copies() {
            places.entry(key).or_insert(at);
        }
        places
    }

    /// The TimelyChurch songs in the library: their places and their marks' keys.
    pub(crate) fn timelychurch_copies(&self) -> Vec<(usize, String)> {
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        self.show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(at, _)| !self.app_made(*at) && !removed.contains(at))
            .filter_map(|(at, song)| {
                let mark = song.catalog.as_ref().filter(|mark| mark.source == super::SOURCE)?;
                Some((at, mark.key.clone()))
            })
            .collect()
    }

    /// Puts what the last check found into the library and the playlists. Changes wait while
    /// anything is on screen: true once all of it is in.
    fn timelychurch_apply(&mut self) -> bool {
        let Some(fetched) = self.timelychurch.waiting.clone() else { return true };
        let full = !self.show.anything_live();
        let editing =
            self.running.as_ref().and_then(|r| r.home_state.editing.as_ref().map(|draft| draft.at.presentation));
        let church = self.timelychurch.settings.address.clone();
        let when = now();
        // Cleared only where something has to wait.
        let mut complete = true;
        let mut done = Done::default();
        let mut copies = self.timelychurch_places();
        // Whether this pass changed anything: only then is it saved, said and drawn.
        let mut changed = false;
        for ((song, arrangement), sent) in &fetched.songs {
            let made = super::make_song(&church, sent, sp_model::MOST_LINES, when);
            let key = made.song.catalog.as_ref().map(|mark| mark.key.clone()).unwrap_or_default();
            match copies.get(&key).copied() {
                None => {
                    let said = joined_words(&made);
                    let (at, _) = self.timelychurch_file(made.song);
                    copies.insert(key.clone(), at);
                    done.songs_added += 1;
                    changed = true;
                    crate::applog::event("timelychurch", &said);
                }
                Some(at) => {
                    if !full || editing == Some(at) {
                        complete = false;
                        continue;
                    }
                    let Some(old) = self.show.presentations().get(at).map(|song| (**song).clone()) else { continue };
                    let verdict = catalog::judge(&old, &super::fresh_source(&made));
                    let song = match &verdict {
                        Verdict::Update => Some(catalog::refreshed_song(&old, made.song)),
                        other => catalog::marked(&old, other),
                    };
                    match verdict {
                        Verdict::Update => done.updated += 1,
                        Verdict::Offer(_) => done.offered += 1,
                        _ => {}
                    }
                    if let Some(song) = song {
                        self.replace_song_quietly(at, song);
                        changed = true;
                    }
                }
            }
            self.timelychurch.settings.saw_song(SongSeen {
                song: song.clone(),
                arrangement: arrangement.clone(),
                key,
                updated_at: sent.updated_at.clone(),
                fetched_at: when,
                gone: false,
            });
        }
        for (song, arrangement) in &fetched.gone {
            let key = self
                .timelychurch
                .settings
                .seen_song(song, arrangement.as_ref())
                .map_or_else(|| super::song_key(&church, song, arrangement.as_ref()), |seen| seen.key.clone());
            if let Some(at) = copies.get(&key).copied() {
                if !full {
                    complete = false;
                    continue;
                }
                let marked = self.show.presentations().get(at).and_then(|old| catalog::marked(old, &Verdict::Gone));
                if let Some(song) = marked.filter(|_| {
                    self.show.presentations().get(at).is_some_and(|old| old.catalog.as_ref().is_some_and(|m| !m.gone))
                }) {
                    self.replace_song_quietly(at, song);
                    changed = true;
                }
            }
            self.timelychurch.settings.saw_song(SongSeen {
                song: song.clone(),
                arrangement: arrangement.clone(),
                key,
                updated_at: None,
                fetched_at: when,
                gone: true,
            });
        }
        // The library keys of the songs made, by what each service asks for.
        let library_keys: HashMap<(Id, Option<Id>), String> = {
            let song_keys = self.running.as_ref().map(|r| r.home_state.song_keys.clone()).unwrap_or_default();
            self.timelychurch
                .settings
                .songs
                .iter()
                .filter(|seen| !seen.gone)
                .filter_map(|seen| {
                    let at = copies.get(&seen.key)?;
                    Some(((seen.song.clone(), seen.arrangement.clone()), song_keys.get(*at)?.clone()))
                })
                .collect()
        };
        let follow = super::to_follow(&fetched.list.services);
        let mut planned: Vec<super::PlannedService> =
            follow.iter().map(|(service, setlist)| super::from_service(service, *setlist)).collect();
        super::distinct_names(&mut planned);
        let keys: Vec<String> =
            follow.iter().map(|(service, _)| super::service_key(&church, &super::Made::of(service))).collect();
        let seen_before: Vec<String> = self.timelychurch.settings.services.iter().map(|s| s.key.clone()).collect();
        let Some(running) = self.running.as_mut() else { return false };
        let services = &mut running.home_state.services;
        for (planned, key) in planned.iter().zip(&keys) {
            let made = super::to_service(planned, |song, arrangement| {
                library_keys.get(&(song.clone(), arrangement.cloned())).cloned()
            });
            let made_fingerprint = playlist_fingerprint(&made);
            let place = services.list.iter().position(|playlist| {
                playlist.origin.as_ref().is_some_and(|origin| origin.source == super::SOURCE && origin.key == *key)
            });
            match place {
                Some(at) => {
                    let front = at == services.front_at();
                    let step = playlist_step(&services.list[at], &made);
                    let playlist = &mut services.list[at];
                    let Some(origin) = playlist.origin.as_mut() else { continue };
                    let mut playlist_changed = false;
                    if origin.removed {
                        origin.removed = false;
                        playlist_changed = true;
                    }
                    if origin.date != planned.date {
                        origin.date.clone_from(&planned.date);
                        playlist_changed = true;
                    }
                    match step {
                        PlaylistStep::Nothing => {}
                        // The playlist in front waits while something is on screen.
                        PlaylistStep::Remake if front && !full => complete = false,
                        PlaylistStep::Remake => {
                            origin.made = made_fingerprint;
                            origin.newer = None;
                            playlist.name = made.name;
                            playlist.items = made.items;
                            playlist_changed = true;
                            if front {
                                services.chosen = None;
                            }
                        }
                        PlaylistStep::Offer(newer) => {
                            origin.newer = Some(newer);
                            done.playlists_offered.push(playlist.name.clone());
                            playlist_changed = true;
                        }
                        PlaylistStep::Withdraw => {
                            origin.newer = None;
                            playlist_changed = true;
                        }
                    }
                    services.changed |= playlist_changed;
                    changed |= playlist_changed;
                }
                None if self.timelychurch.settings.let_go.contains(key) => {}
                // Followed before and gone from the playlists: the church deleted it.
                None if seen_before.contains(key) => {
                    self.timelychurch.settings.let_go.push(key.clone());
                    changed = true;
                }
                None => {
                    done.new_playlists.push(made.name.clone());
                    services.list.push(Service {
                        origin: Some(ServiceOrigin {
                            source: super::SOURCE.to_owned(),
                            key: key.clone(),
                            made: made_fingerprint,
                            date: planned.date.clone(),
                            newer: None,
                            removed: false,
                        }),
                        ..made
                    });
                    services.changed = true;
                    changed = true;
                }
            }
        }
        if complete {
            // A service no longer in a list answered now, while its date is still inside the
            // dates asked for, was removed in TimelyChurch. One whose date has passed has only
            // dropped out of the dates asked for. A list with a service that couldn't be read
            // says nothing of what is missing from it.
            if fetched.fresh && fetched.list.dropped == 0 {
                for key in removed_services(&self.timelychurch.settings.services, &keys, &fetched.from) {
                    let playlist = services.list.iter_mut().find(|playlist| {
                        playlist.origin.as_ref().is_some_and(|o| o.source == super::SOURCE && o.key == key)
                    });
                    if let Some(playlist) = playlist
                        && let Some(origin) = playlist.origin.as_mut().filter(|origin| !origin.removed)
                    {
                        origin.removed = true;
                        services.changed = true;
                        done.removed.push(playlist.name.clone());
                    }
                }
            }
            // A service with a song that couldn't be fetched isn't taken as seen as it is now,
            // so the next check asks for its songs again.
            let failed = |service: &super::Service| {
                service.items.iter().any(|item| {
                    let asked = (item.song_id.clone().unwrap_or_default(), item.arrangement_id.clone());
                    item.song_id.is_some() && fetched.failed.contains(&asked)
                })
            };
            self.timelychurch.settings.services = follow
                .iter()
                .zip(&keys)
                .map(|((service, setlist), key)| super::store::ServiceSeen {
                    key: key.clone(),
                    updated_at: if failed(service) || setlist.is_some_and(failed) {
                        None
                    } else {
                        service.updated_at.clone()
                    },
                    date: service.date.clone(),
                })
                .collect();
            self.timelychurch.settings.let_go.retain(|key| keys.contains(key));
            self.timelychurch.applied(&fetched);
        } else if changed {
            self.timelychurch.save_settings();
        }
        self.timelychurch_say(&done);
        if (changed || complete)
            && let Some(running) = self.running.as_mut()
        {
            running.home_window.request_redraw();
        }
        complete
    }

    /// Files a song made from TimelyChurch in the library, and in the library named
    /// TimelyChurch (made the first time). Its place and library key.
    fn timelychurch_file(&mut self, song: Presentation) -> (usize, String) {
        let (at, key) = self.store_song(song);
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            let library = match state.libraries.iter().position(|library| library.name == super::SOURCE_NAME) {
                Some(library) => library,
                None => {
                    state.libraries.push(crate::libraries::Library::new(super::SOURCE_NAME.to_owned()));
                    state.libraries.len() - 1
                }
            };
            if !state.libraries[library].has(&key) {
                state.libraries[library].songs.push(key.clone());
            }
            state.libraries_changed = true;
        }
        (at, key)
    }

    /// One notice for each kind of thing a check changed.
    fn timelychurch_say(&mut self, done: &Done) {
        let Some(running) = self.running.as_mut() else { return };
        let notices = &mut running.home_state.notices;
        let count = |n: usize| n as u64;
        if !done.new_playlists.is_empty() {
            let names = done.new_playlists.join(", ");
            notices.info(tn(
                "notices.timelychurch.new_playlists",
                count(done.new_playlists.len()),
                "{n} playlist came from TimelyChurch: {names}.",
                "{n} playlists came from TimelyChurch: {names}.",
                &[("names", &names)],
            ));
        }
        if done.updated > 0 {
            notices.info(tn(
                "notices.timelychurch.updated",
                count(done.updated),
                "{n} song from TimelyChurch brought up to date.",
                "{n} songs from TimelyChurch brought up to date.",
                &[],
            ));
        }
        if done.offered > 0 {
            notices.info(tn(
                "notices.timelychurch.offered",
                count(done.offered),
                "{n} song you changed has a newer version in TimelyChurch; yours is kept.",
                "{n} songs you changed have a newer version in TimelyChurch; yours are kept.",
                &[],
            ));
        }
        for name in &done.playlists_offered {
            notices.info(tf(
                "notices.timelychurch.playlist_offered",
                "{name} changed in TimelyChurch; your playlist, which you changed, is kept.",
                &[("name", name)],
            ));
        }
        for name in &done.removed {
            notices.info(tf(
                "notices.timelychurch.removed",
                "{name} was removed in TimelyChurch; its playlist stays.",
                &[("name", name)],
            ));
        }
    }

    /// Counts a TimelyChurch song put on screen from a TimelyChurch service's playlist, on the
    /// service's date, once per song per service (see the module's notes).
    fn timelychurch_count_uses(&mut self) {
        if !self.timelychurch.connected() && self.timelychurch.status != Status::Reading {
            return;
        }
        let Some(live) = self.show.live() else {
            self.timelychurch.looked = None;
            return;
        };
        let Some(here) = self.showing() else { return };
        let Some(running) = self.running.as_ref() else { return };
        let services = &running.home_state.services;
        let look = (live.presentation, here.item, services.front_at());
        if self.timelychurch.looked == Some(look) {
            return;
        }
        self.timelychurch.looked = Some(look);
        let Some(mark) = self.show.presentations().get(live.presentation).and_then(|song| song.catalog.clone()) else {
            return;
        };
        let front = services.front();
        let Some(origin) = front.origin.as_ref().filter(|o| o.source == super::SOURCE && !o.removed) else { return };
        // From the playlist itself: a song it holds.
        let holds = here.item.and_then(|at| front.items.get(at)).is_some_and(|item| {
            matches!(item, crate::service::Item::Song { key, .. }
                if running.home_state.song_keys.get(live.presentation) == Some(key))
        });
        let today = today().format("%Y-%m-%d").to_string();
        if mark.source != super::SOURCE || !holds || origin.date.as_deref().is_some_and(|date| date != today) {
            return;
        }
        let Some((church, song, arrangement)) = super::parse_song_key(&mark.key) else { return };
        // A song of another church is never told to this one.
        if church != super::church_part(&self.timelychurch.settings.address) {
            return;
        }
        let Some((kind, service)) = service_of(&origin.key) else { return };
        let key = self.timelychurch.key_sung(&kind, &service, &song);
        let used = SongUse {
            song_id: song.clone(),
            arrangement_id: arrangement,
            used_at: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            service_id: Some(service),
            service_type: Some(kind),
            key,
        };
        let counted_as = format!("{}|{}", origin.key, mark.key);
        if self.timelychurch.count_use(&counted_as, used) {
            let title = self.show.presentations().get(live.presentation).map(|s| s.title.clone()).unwrap_or_default();
            crate::applog::event(
                "timelychurch",
                &format!("{title} was shown in {}: one use to tell TimelyChurch", front.name),
            );
        }
    }

    /// Keeps the card in step with the connection: made again only when something changed.
    fn timelychurch_view(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let connection = &self.timelychurch;
        let mut rows: Vec<ServiceRow> = running
            .home_state
            .services
            .list
            .iter()
            .filter_map(|playlist| {
                let origin = playlist.origin.as_ref().filter(|o| o.source == super::SOURCE)?;
                let church = super::church_part(&connection.settings.address);
                if !origin.key.starts_with(&format!("{church}/")) {
                    return None;
                }
                let state = if origin.removed {
                    RowState::Removed
                } else if origin.newer.is_some() {
                    RowState::NewerWaiting
                } else if playlist_fingerprint(playlist) != origin.made {
                    RowState::ChangedHere
                } else {
                    RowState::InStep
                };
                let songs =
                    playlist.items.iter().filter(|item| matches!(item, crate::service::Item::Song { .. })).count();
                Some(ServiceRow { name: playlist.name.clone(), date: origin.date.clone(), songs, state })
            })
            .collect();
        rows.sort_by(|a, b| a.date.cmp(&b.date));
        let view = &mut running.home_state.timelychurch;
        let fresh = super::view::View {
            offered: true,
            open: view.open,
            asks_address: super::client::ONE_ADDRESS.is_none(),
            typed_address: view.typed_address.clone(),
            typed_code: view.typed_code.clone(),
            status: connection.status.clone(),
            church: connection.settings.church.clone(),
            address: connection.settings.address.clone(),
            last_checked: connection.settings.last_checked,
            checking: connection.checking(),
            trouble: connection.trouble.clone(),
            problem: view.problem.clone(),
            services: rows,
            uses_waiting: connection.uses.queued.len(),
            request: view.request.clone(),
        };
        if *view != fresh {
            let first = !view.offered;
            *view = fresh;
            if first || view.typed_address.is_empty() {
                view.typed_address = connection.suggested_address();
            }
            running.home_window.request_redraw();
            for panel in &running.panels {
                panel.window.request_redraw();
            }
        }
    }

    /// Takes TimelyChurch's newer version of a song the church changed, as a change the
    /// window makes: it can be undone. Made from the song as TimelyChurch last sent it.
    pub(crate) fn timelychurch_use_newer(&mut self, at: usize) -> Result<String, String> {
        let old = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let mark = old.catalog.clone().filter(|mark| mark.source == super::SOURCE);
        let Some(mark) = mark else { return Err(format!("{} isn't from TimelyChurch.", old.title)) };
        if mark.newer.is_none() {
            return Err(format!("{} has no newer version waiting.", old.title));
        }
        let (_, song, arrangement) = super::parse_song_key(&mark.key).ok_or("That song's mark can't be read.")?;
        let asked = self
            .timelychurch
            .settings
            .songs
            .iter()
            .find(|seen| seen.key == mark.key)
            .map_or((song.clone(), arrangement.clone()), |seen| (seen.song.clone(), seen.arrangement.clone()));
        let sent = self.timelychurch.kept_song(&asked.0, asked.1.as_ref()).ok_or_else(|| {
            format!("{}: TimelyChurch's version isn't kept on this computer; check again first.", old.title)
        })?;
        let made = super::make_song(&self.timelychurch.settings.address, &sent, sp_model::MOST_LINES, now());
        self.change_song(at, catalog::refreshed_song(&old, made.song));
        crate::applog::event("timelychurch", &format!("{} took the newer version from TimelyChurch", old.title));
        Ok(format!("{} now has the newer version from TimelyChurch. Undo brings your version back.", old.title))
    }

    /// "Keep mine": the church keeps its song, and the same newer version is not offered again.
    pub(crate) fn timelychurch_keep_mine(&mut self, at: usize) -> Result<String, String> {
        let old = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        if old.catalog.as_ref().is_none_or(|mark| mark.source != super::SOURCE) {
            return Err(format!("{} isn't from TimelyChurch.", old.title));
        }
        let song = catalog::kept(&old).ok_or_else(|| format!("{} has no newer version waiting.", old.title))?;
        self.change_song(at, song);
        Ok(format!("{} stays as it is; the newer version won't be offered again.", old.title))
    }

    /// Puts TimelyChurch's version of a playlist the church changed in its place. The error is
    /// a sentence.
    pub(crate) fn timelychurch_use_newer_playlist(&mut self, at: usize) -> Result<String, String> {
        let church = self.timelychurch.settings.address.clone();
        let list = self.timelychurch.list.clone();
        let copies = self.timelychurch_places();
        let Some(running) = self.running.as_mut() else { return Err("The window isn't open yet.".to_owned()) };
        let song_keys = running.home_state.song_keys.clone();
        let services = &mut running.home_state.services;
        let playlist = services.list.get(at).ok_or("That playlist is gone.")?;
        let origin = playlist.origin.clone().filter(|o| o.source == super::SOURCE);
        let Some(origin) = origin else { return Err(format!("{} isn't from TimelyChurch.", playlist.name)) };
        if origin.newer.is_none() {
            return Err(format!("{} has no newer version waiting.", playlist.name));
        }
        let follow = super::to_follow(&list.services);
        let mut planned: Vec<super::PlannedService> =
            follow.iter().map(|(service, setlist)| super::from_service(service, *setlist)).collect();
        super::distinct_names(&mut planned);
        let place =
            follow.iter().position(|(service, _)| super::service_key(&church, &super::Made::of(service)) == origin.key);
        let Some(place) = place else { return Err(format!("{} is no longer in TimelyChurch.", playlist.name)) };
        let seen = &self.timelychurch.settings.songs;
        let made = super::to_service(&planned[place], |song, arrangement| {
            let seen = seen.iter().find(|s| s.song == *song && s.arrangement.as_ref() == arrangement && !s.gone)?;
            song_keys.get(*copies.get(&seen.key)?).cloned()
        });
        let name = playlist.name.clone();
        let fingerprint = playlist_fingerprint(&made);
        services.list[at] =
            Service { origin: Some(ServiceOrigin { made: fingerprint, newer: None, ..origin }), ..made };
        if at == services.front_at() {
            services.chosen = None;
        }
        services.changed = true;
        Ok(format!("{name} is now as TimelyChurch has it."))
    }
}

/// What the log says of a song that joined the library: its key, the translation under its
/// words and the lines it lacks, and names in its order sung that name no section.
fn joined_words(made: &super::song::MadeSong) -> String {
    let key = made.song.catalog.as_ref().map(|mark| mark.key.as_str()).unwrap_or("");
    let mut said = format!("{} joined the library ({key})", made.song.title);
    if let Some(second) = &made.second {
        said.push_str(&format!(", with the {second} translation under the words"));
        if made.untranslated > 0 {
            let lines = if made.untranslated == 1 { "line" } else { "lines" };
            said.push_str(&format!(" ({} {lines} untranslated)", made.untranslated));
        }
    }
    if !made.unmatched.is_empty() {
        said.push_str(&format!("; sung but with no words: {}", made.unmatched.join(", ")));
    }
    said
}

/// What a check does about a playlist made from a TimelyChurch service.
#[derive(Debug, PartialEq, Eq)]
enum PlaylistStep {
    /// As TimelyChurch's service makes it, with nothing offered: nothing to do.
    Nothing,
    /// Nobody has touched it, and the service makes it otherwise now: made again.
    Remake,
    /// The church changed it, and TimelyChurch changed the service too: its version (by this
    /// fingerprint) is offered, never forced, and said once.
    Offer(String),
    /// TimelyChurch's service makes it as it was made again: the offer goes.
    Withdraw,
}

/// What to do about `playlist` (with its mark), now that its service makes `made`.
fn playlist_step(playlist: &Service, made: &Service) -> PlaylistStep {
    let Some(origin) = &playlist.origin else { return PlaylistStep::Nothing };
    let fresh = playlist_fingerprint(made);
    if fresh == origin.made {
        return if origin.newer.is_some() { PlaylistStep::Withdraw } else { PlaylistStep::Nothing };
    }
    if playlist_fingerprint(playlist) == origin.made {
        PlaylistStep::Remake
    } else if origin.newer.as_deref() == Some(fresh.as_str()) {
        PlaylistStep::Nothing
    } else {
        PlaylistStep::Offer(fresh)
    }
}

/// The services a list answered now shows were removed in TimelyChurch: followed before, not in
/// the list, and dated on or after `from`, the first date asked for. One whose date has passed
/// has only dropped out of the dates asked for.
fn removed_services<'a>(seen: &'a [super::store::ServiceSeen], listed: &[String], from: &str) -> Vec<&'a str> {
    seen.iter()
        .filter(|seen| !listed.contains(&seen.key) && seen.date.as_deref().is_none_or(|date| date >= from))
        .map(|seen| seen.key.as_str())
        .collect()
}

/// The kind and id of the service a playlist's key names (`…/setlist/501`).
fn service_of(key: &str) -> Option<(String, Id)> {
    let (rest, id) = key.rsplit_once('/')?;
    let kind = rest.rsplit('/').next()?;
    let id = Id::new(id);
    (matches!(kind, "setlist" | "plan") && id.is_safe()).then(|| (kind.to_owned(), id))
}

fn soonest(have: Option<Instant>, at: Instant) -> Option<Instant> {
    Some(have.map_or(at, |have| have.min(at)))
}

impl super::sync::Connection {
    /// The key a song was to be sung in at a service, as the services list gave it.
    pub fn key_sung(&self, kind: &str, service: &Id, song: &Id) -> Option<String> {
        let found = self.list.services.iter().find(|s| s.id == *service && s.is_setlist() == (kind == "setlist"))?;
        let item = found.items.iter().find(|item| item.song_id.as_ref() == Some(song))?;
        item.key.as_deref().map(str::trim).filter(|key| !key.is_empty()).map(str::to_owned)
    }
}

#[cfg(test)]
mod checks {
    use super::*;

    #[test]
    fn a_playlists_key_names_its_service() {
        assert_eq!(service_of("church.example.org/setlist/501"), Some(("setlist".to_owned(), Id::new("501"))));
        assert_eq!(service_of("church.example.org/plan/a-9"), Some(("plan".to_owned(), Id::new("a-9"))));
        assert_eq!(service_of("church.example.org/song/1"), None);
        assert_eq!(service_of("nothing"), None);
    }

    #[test]
    fn a_playlists_fingerprint_follows_its_name_and_items() {
        let service = |name: &str| Service {
            name: name.to_owned(),
            items: vec![crate::service::Item::song("k", "Song")],
            uids: Default::default(),
            origin: None,
        };
        assert_eq!(playlist_fingerprint(&service("A")), playlist_fingerprint(&service("A")));
        assert_ne!(playlist_fingerprint(&service("A")), playlist_fingerprint(&service("B")));
        let marked = Service {
            origin: Some(ServiceOrigin {
                source: "timelychurch".to_owned(),
                key: "k".to_owned(),
                made: String::new(),
                date: None,
                newer: None,
                removed: true,
            }),
            ..service("A")
        };
        assert_eq!(playlist_fingerprint(&marked), playlist_fingerprint(&service("A")), "the mark isn't the playlist");
    }

    fn playlist(items: &[&str]) -> Service {
        Service {
            name: "Sunday".to_owned(),
            items: items.iter().map(|key| crate::service::Item::song(*key, *key)).collect(),
            uids: Default::default(),
            origin: None,
        }
    }

    /// A playlist as made from `items`, with its mark.
    fn made(items: &[&str]) -> Service {
        let made = playlist(items);
        let origin = ServiceOrigin {
            source: "timelychurch".to_owned(),
            key: "church/setlist/1".to_owned(),
            made: playlist_fingerprint(&made),
            date: None,
            newer: None,
            removed: false,
        };
        Service { origin: Some(origin), ..made }
    }

    #[test]
    fn an_untouched_playlist_is_made_again_and_a_changed_one_only_offered() {
        let ours = made(&["a", "b"]);
        assert_eq!(playlist_step(&ours, &playlist(&["a", "b"])), PlaylistStep::Nothing);
        assert_eq!(playlist_step(&ours, &playlist(&["a", "c"])), PlaylistStep::Remake, "nobody touched it");
        // The church moved a song: TimelyChurch's change is offered, once.
        let changed = Service { items: vec![ours.items[1].clone(), ours.items[0].clone()], ..ours.clone() };
        let newer = playlist(&["a", "c"]);
        let PlaylistStep::Offer(fingerprint) = playlist_step(&changed, &newer) else { panic!("offered") };
        assert_eq!(fingerprint, playlist_fingerprint(&newer));
        let mut offered = changed.clone();
        offered.origin.as_mut().expect("marked").newer = Some(fingerprint);
        assert_eq!(playlist_step(&offered, &newer), PlaylistStep::Nothing, "said once");
        assert_eq!(playlist_step(&offered, &playlist(&["a", "b"])), PlaylistStep::Withdraw, "taken back");
        assert!(matches!(playlist_step(&offered, &playlist(&["d"])), PlaylistStep::Offer(_)), "a newer one again");
        assert_eq!(playlist_step(&playlist(&["a"]), &playlist(&["b"])), PlaylistStep::Nothing, "not TimelyChurch's");
    }

    #[test]
    fn a_service_missing_inside_the_dates_was_removed_and_one_past_only_dropped_out() {
        let seen = |key: &str, date: Option<&str>| super::super::store::ServiceSeen {
            key: key.to_owned(),
            updated_at: None,
            date: date.map(str::to_owned),
        };
        let followed = [
            seen("c/setlist/1", Some("2026-12-06")),
            seen("c/plan/2", Some("2026-12-13")),
            seen("c/setlist/3", Some("2026-11-29")),
            seen("c/setlist/4", None),
        ];
        let listed = ["c/setlist/1".to_owned()];
        assert_eq!(removed_services(&followed, &listed, "2026-12-01"), ["c/plan/2", "c/setlist/4"]);
        assert!(removed_services(&followed, &[], "2027-01-01").iter().all(|key| *key == "c/setlist/4"));
    }
}
