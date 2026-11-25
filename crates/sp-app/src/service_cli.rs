//! Command-line commands for preparing a service: playlists, libraries, songs and slides, and
//! searching the library. Each does what the window's own control does, through the same
//! requests (so a song change can be undone and is saved), and says what changed.

use sp_model::{ObjectKind, Presentation, ShowAction, Transition, TransitionKind};

use crate::control::{Reply, find, numbered};
use crate::i18n::tf;
use crate::service::Item;

/// The place of an item of a playlist, named by its number, its title, or its `uid` (from
/// `playlist --json`), which names it exactly wherever it has been moved to.
pub(crate) fn item_in(playlist: &crate::service::Service, typed: &str) -> Result<usize, String> {
    if let Ok(uid) = sp_model::Uuid::parse_str(typed.trim()) {
        return playlist
            .place_of_uid(uid)
            .ok_or_else(|| format!("That item is no longer in {}; look again and try once more.", playlist.name));
    }
    let items: Vec<String> = playlist.items.iter().map(|item| item.title().to_owned()).collect();
    find("item", &items, typed).map_err(|problem| format!("{problem} {} has {} items.", playlist.name, items.len()))
}

/// Splits `words … KEYWORD more words` at the last KEYWORD: the words before it and after.
fn split_at_word<'a>(words: &'a [String], keyword: &str) -> (&'a [String], Option<String>) {
    match words.iter().rposition(|w| w.eq_ignore_ascii_case(keyword)) {
        Some(at) if at + 1 < words.len() => (&words[..at], Some(words[at + 1..].join(" "))),
        _ => (words, None),
    }
}

/// A transition as typed: a kind's name ("dissolve", "fade black", "push") and seconds, or
/// "none" for the theme's (or the show's) own.
pub fn parse_transition(words: &[String]) -> Result<Option<Transition>, String> {
    let seconds = words.last().and_then(|w| w.trim_end_matches('s').parse::<f32>().ok());
    let name_words = if seconds.is_some() { &words[..words.len() - 1] } else { words };
    let typed: String = name_words.join("").to_lowercase().replace(['_', '-', ' '], "");
    if typed == "none" || typed == "off" {
        return Ok(None);
    }
    let kinds = TransitionKind::ALL;
    let kind =
        kinds.iter().find(|kind| kind.name().to_lowercase().replace(' ', "") == typed).copied().ok_or_else(|| {
            format!(
                "{} isn't a transition. They are: {}, or none.",
                name_words.join(" "),
                kinds.iter().map(|kind| kind.name()).collect::<Vec<_>>().join(", ")
            )
        })?;
    let seconds = seconds.unwrap_or(if kind == TransitionKind::Cut { 0.0 } else { 0.4 });
    if !(0.0..=5.0).contains(&seconds) {
        return Err(format!("A transition takes 0 to 5 seconds, not {seconds}."));
    }
    Ok(Some(Transition { kind, seconds, ..Transition::default() }))
}

fn transition_words(transition: Option<&Transition>) -> String {
    transition.map_or_else(|| "none (the theme's)".to_owned(), |t| format!("{} {:.1}s", t.kind.name(), t.seconds))
}

/// What a playlist item was found to be in the library, the media bin or the media playlists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Found {
    /// A song: its id, and its number in `sp songs` (none for one deleted while in use).
    Song { id: sp_model::Uuid, number: Option<usize> },
    /// The VoxLive item, which isn't among the songs.
    VoxLive,
    /// A picture or video in the media bin: its number there.
    Media { number: usize, video: bool },
    /// A media playlist: its number in `sp media-playlist`.
    MediaPlaylist { number: usize },
    /// Nothing to find (a header, a passage, a placeholder), or the song, picture, video or
    /// media playlist has gone.
    Nothing,
}

/// A playlist item in words, in the language set: its line ("Bars.mp4 (video)"), its kind, and
/// whether what it names has gone. `sp` lists it in English ([`item_entry`]); a remote page is
/// sent it in the window's language ([`item_text`]).
fn item_line(item: &Item, found: &Found) -> (String, &'static str, bool) {
    let title = item.title();
    let with = [("title", title)];
    match (item, found) {
        (Item::Header { name }, _) => (format!("— {name} —"), "header", false),
        (Item::Song { .. }, Found::VoxLive) => (title.to_owned(), "voxlive", false),
        (Item::Song { .. }, Found::Song { .. }) => (title.to_owned(), "song", false),
        (Item::Song { .. }, _) => (tf("remote.item.missing", "{title} (missing)", &with), "song", true),
        (Item::Passage(_), _) => (title.to_owned(), "passage", false),
        (Item::Media { .. }, Found::Media { video: true, .. }) => {
            (tf("remote.item.video", "{title} (video)", &with), "media", false)
        }
        (Item::Media { .. }, Found::Media { .. }) => {
            (tf("remote.item.picture", "{title} (picture)", &with), "media", false)
        }
        (Item::Media { .. }, _) => {
            (tf("remote.item.media_missing", "{title} (missing picture or video)", &with), "media", true)
        }
        (Item::MediaPlaylist { .. }, Found::MediaPlaylist { .. }) => {
            (tf("remote.item.media_playlist", "{title} (media playlist)", &with), "media_playlist", false)
        }
        (Item::MediaPlaylist { .. }, _) => (
            tf("remote.item.media_playlist_missing", "{title} (missing media playlist)", &with),
            "media_playlist",
            true,
        ),
        (Item::Placeholder { .. }, _) => {
            (tf("remote.item.placeholder", "{title} (placeholder)", &with), "placeholder", false)
        }
    }
}

/// A playlist item's line as a remote page is sent it: in the language set.
pub(crate) fn item_text(item: &Item, found: &Found) -> String {
    item_line(item, found).0
}

/// One playlist item as `playlist [NAME]` lists it: its line, and the same as data, with its
/// number in the playlist, its kind, its title as the playlist shows it, and what it was found
/// to be. `text` is the line, as the JSON gave it before it had the rest. Always in English, as
/// the command line is ([`item_text`] is the window's language, for a page).
pub(crate) fn item_entry(number: usize, item: &Item, found: &Found) -> (String, serde_json::Value) {
    let title = item.title();
    let (line, kind, missing) = crate::i18n::with_language(crate::i18n::Language::English, || item_line(item, found));
    let mut data = serde_json::json!({
        "item": number, "kind": kind, "title": title, "missing": missing, "text": line,
    });
    match (item, found) {
        (Item::Song { .. }, Found::Song { id, number }) => {
            data["id"] = serde_json::json!(id);
            data["number"] = serde_json::json!(number);
        }
        (Item::Song { .. }, Found::VoxLive) => {
            data["id"] = serde_json::Value::Null;
            data["number"] = serde_json::Value::Null;
        }
        // A song that has gone keeps the id it was named by, where it was named by one.
        (Item::Song { key, .. }, _) => {
            data["id"] = serde_json::json!(sp_model::Uuid::parse_str(key).ok());
            data["number"] = serde_json::Value::Null;
        }
        (Item::Passage(saved), _) => {
            data["bibles"] = serde_json::json!(saved.rows.iter().map(|row| row.bible.clone()).collect::<Vec<_>>());
        }
        (Item::Media { .. }, Found::Media { number, video }) => {
            data["number"] = serde_json::json!(number);
            data["media_type"] = serde_json::json!(if *video { "video" } else { "picture" });
        }
        (Item::MediaPlaylist { .. }, Found::MediaPlaylist { number }) => data["number"] = serde_json::json!(number),
        (Item::Media { .. } | Item::MediaPlaylist { .. }, _) => data["number"] = serde_json::Value::Null,
        _ => {}
    }
    (line, data)
}

impl crate::App {
    /// A presentation's number in `sp songs`, from 1: none for a Bible passage, the VoxLive
    /// item, or a song deleted while it was in use.
    pub(crate) fn song_number(&self, at: usize) -> Option<usize> {
        self.is_listed(at).then(|| (0..at).filter(|index| self.is_listed(*index)).count() + 1)
    }

    /// Whether `sp songs` lists a presentation: not a Bible passage, the VoxLive item, or a
    /// song deleted while it was in use.
    pub(crate) fn is_listed(&self, at: usize) -> bool {
        let removed = self.running.as_ref().is_some_and(|r| r.home_state.removed_songs.contains(&at));
        at < self.show.presentations().len() && !self.app_made(at) && !removed
    }

    /// What a playlist item was found to be: its song, picture, video or media playlist.
    pub(crate) fn found(&self, item: &Item) -> Found {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Found::Nothing };
        match item {
            Item::Song { key, .. } => match crate::studio::playlist_song(&self.show, state, key) {
                Some(at) if at == self.voxlive_slot => Found::VoxLive,
                Some(at) => Found::Song { id: self.show.presentations()[at].id, number: self.song_number(at) },
                None => Found::Nothing,
            },
            Item::Media { id, .. } => match self.show.media().iter().position(|media| media.id == *id) {
                Some(at) => Found::Media { number: at + 1, video: crate::media::is_video(&self.show.media()[at].path) },
                None => Found::Nothing,
            },
            Item::MediaPlaylist { id, .. } => match state.media_playlists.iter().position(|p| p.id == *id) {
                Some(at) => Found::MediaPlaylist { number: at + 1 },
                None => Found::Nothing,
            },
            Item::Passage(_) | Item::Header { .. } | Item::Placeholder { .. } => Found::Nothing,
        }
    }

    /// `playlist [NAME]`: a playlist's items, as lines and as data. `for_page`: each item's
    /// `text` in the window's language, as a remote page is sent it (`sp` has it in English).
    pub(crate) fn playlist_listing(&self, list: usize, for_page: bool) -> (Vec<String>, Vec<serde_json::Value>) {
        let Some(playlist) = self.running.as_ref().and_then(|r| r.home_state.services.list.get(list)) else {
            return (Vec::new(), Vec::new());
        };
        let uids = playlist.uids();
        playlist
            .items
            .iter()
            .enumerate()
            .map(|(at, item)| {
                let found = self.found(item);
                let (line, mut data) = item_entry(at + 1, item, &found);
                data["uid"] = serde_json::json!(uids[at]);
                if for_page {
                    data["text"] = serde_json::json!(item_text(item, &found));
                }
                (line, data)
            })
            .unzip()
    }

    /// The playlist a name stands for, or the one in front (for other command modules).
    pub(crate) fn playlist_at_for_cli(&self, typed: Option<&str>) -> Result<usize, String> {
        self.playlist_at(typed)
    }

    /// The playlist a name stands for, or the one in front.
    fn playlist_at(&self, typed: Option<&str>) -> Result<usize, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        match typed {
            Some(typed) => {
                let names: Vec<String> = state.services.list.iter().map(|p| p.name.clone()).collect();
                find("playlist", &names, typed)
            }
            None => Ok(state.services.list.iter().position(|p| p.name == state.services.front().name).unwrap_or(0)),
        }
    }

    /// An item of a playlist, by its number or its title.
    fn playlist_item(&self, list: usize, typed: &str) -> Result<usize, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        item_in(&state.services.list[list], typed)
    }

    /// `playlist new|rename|delete|remove|move|header …`; `None` for a word that isn't one of them.
    pub(crate) fn ctl_playlist_change(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "new" => self.playlist_new(&words.join(" ")),
            "go" => self.playlist_go(words),
            "rename" => {
                let (old, new) = split_at_word(words, "to");
                match new {
                    Some(new) => self.playlist_rename(&old.join(" "), &new),
                    None => Err("playlist rename OLD to NEW".to_owned()),
                }
            }
            "delete" => self.playlist_delete(words),
            "remove" => {
                let (item, list) = split_at_word(words, "from");
                self.playlist_remove(&item.join(" "), list.as_deref())
            }
            "move" => {
                let (head, list) = split_at_word(words, "in");
                let (item, to) = split_at_word(head, "to");
                match to.and_then(|to| to.parse::<usize>().ok()) {
                    Some(to) => self.playlist_move(&item.join(" "), to, list.as_deref()),
                    None => Err("playlist move ITEM to N [in PLAYLIST]".to_owned()),
                }
            }
            // `playlist header|media|media-playlist|placeholder|passage NAME [at N] [in PLAYLIST]`
            // "in" names a playlist only when one is called that, and "at" a place only when a
            // number follows, so either can be part of the name ("Live at Church.mp4").
            "header" | "media" | "media-playlist" | "placeholder" | "passage" => {
                let (mut head, mut list) = split_at_word(words, "in");
                if list.as_deref().is_some_and(|typed| self.playlist_at(Some(typed)).is_err()) {
                    (head, list) = (words, None);
                }
                let (text, at) = match split_at_word(head, "at") {
                    (text, Some(at)) if at.parse::<usize>().is_ok() => (text, at.parse::<usize>().ok()),
                    _ => (head, None),
                };
                if word == "passage" {
                    self.playlist_passage(&text.join(" "), at, list.as_deref())
                } else {
                    self.playlist_put(&word, &text.join(" "), at, list.as_deref())
                }
            }
            // `playlist fill PLACEHOLDER with SONG | with media ITEM [in PLAYLIST]`
            "fill" => {
                let (head, list) = split_at_word(words, "in");
                match split_at_word(head, "with") {
                    (place, Some(with)) if !place.is_empty() => {
                        self.playlist_fill(&place.join(" "), &with, list.as_deref())
                    }
                    _ => Err("playlist fill PLACEHOLDER with SONG | with media ITEM [in PLAYLIST]".to_owned()),
                }
            }
            // `playlist item N show-with NAME|usual [--keep] [in PLAYLIST]`
            "item" => self.playlist_show_with(words),
            _ => return None,
        };
        Some(result)
    }

    fn playlist_new(&mut self, name: &str) -> Result<Reply, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("playlist new NAME".to_owned());
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if state.services.list.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
            return Err(format!("There is a playlist called {name} already."));
        }
        state.services.list.push(crate::service::Service {
            name: name.to_owned(),
            items: Vec::new(),
            uids: Default::default(),
            origin: None,
        });
        state.services.changed = true;
        Ok(Reply::done(format!("Made the playlist {name}.")))
    }

    fn playlist_rename(&mut self, old: &str, new: &str) -> Result<Reply, String> {
        let at = self.playlist_at(Some(old))?;
        let new = new.trim();
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if new.is_empty() {
            return Err("playlist rename OLD to NEW".to_owned());
        }
        if state.services.list.iter().enumerate().any(|(i, p)| i != at && p.name.eq_ignore_ascii_case(new)) {
            return Err(format!("There is a playlist called {new} already."));
        }
        let was = std::mem::replace(&mut state.services.list[at].name, new.to_owned());
        state.services.changed = true;
        Ok(Reply::done(format!("Renamed the playlist {was} to {new}.")))
    }

    /// At once, as in the window: `sp undo` puts it back (an old --yes is let by).
    fn playlist_delete(&mut self, words: &[String]) -> Result<Reply, String> {
        let name: Vec<String> = words.iter().filter(|w| *w != "--yes").cloned().collect();
        let at = self.playlist_at(Some(&name.join(" ")))?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let title = state.services.list[at].name.clone();
        // Deleted where it is: the playlist in front stays in front, unless it is this one.
        if !state.delete_playlist(at) {
            return Err(format!("{title} is the only playlist; there is always one."));
        }
        Ok(Reply::done(format!("Deleted the playlist {title}; its songs stay in the library.")))
    }

    fn playlist_remove(&mut self, item: &str, list: Option<&str>) -> Result<Reply, String> {
        let at = self.playlist_at(list)?;
        let which = self.playlist_item(at, item)?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        // The open item of the playlist in front stays open, unless it is the one taken out;
        // `sp undo` puts it back.
        let gone = state.take_out_items(at, &[which]);
        let name = state.services.list[at].name.clone();
        let title = gone.first().map(|(_, item)| item.title().to_owned()).unwrap_or_default();
        Ok(Reply::done(format!("Took {title} out of {name}.")))
    }

    fn playlist_move(&mut self, item: &str, to: usize, list: Option<&str>) -> Result<Reply, String> {
        let at = self.playlist_at(list)?;
        let which = self.playlist_item(at, item)?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let playlist = &mut state.services.list[at];
        if !(1..=playlist.items.len()).contains(&to) {
            return Err(format!("{} has {} items; there is no place {to}.", playlist.name, playlist.items.len()));
        }
        let moving = playlist.items.remove(which);
        let title = moving.title().to_owned();
        playlist.items.insert(to - 1, moving);
        let name = playlist.name.clone();
        // In the playlist in front, the open item stays open wherever it now is.
        if at == state.services.current {
            state.services.chosen = state.services.chosen.map(|open| crate::service::moved_place(open, which, to - 1));
        }
        state.services.changed = true;
        Ok(Reply::done(format!("Moved {title} to place {to} in {name}.")))
    }

    /// Puts a header, a picture or video from the media bin, a media playlist or a placeholder
    /// into a playlist, at a place (or at the end).
    fn playlist_put(&mut self, kind: &str, text: &str, at: Option<usize>, list: Option<&str>) -> Result<Reply, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err(format!("playlist {kind} NAME [at N] [in PLAYLIST]"));
        }
        let (item, what) = match kind {
            "media" => {
                let names: Vec<String> = self.show.media().iter().map(|media| media.name.clone()).collect();
                let media = &self.show.media()[find("media item", &names, text)?];
                let what = if crate::media::is_video(&media.path) { "video" } else { "picture" };
                (Item::Media { id: media.id, name: media.name.clone() }, what)
            }
            "media-playlist" => {
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let names: Vec<String> = state.media_playlists.iter().map(|playlist| playlist.name.clone()).collect();
                let playlist = &state.media_playlists[find("media playlist", &names, text)?];
                (Item::MediaPlaylist { id: playlist.id, name: playlist.name.clone() }, "media playlist")
            }
            "placeholder" => (Item::Placeholder { name: text.to_owned() }, "placeholder"),
            _ => (Item::Header { name: text.to_owned() }, "header"),
        };
        let list = self.playlist_at(list)?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let playlist = &mut state.services.list[list];
        let place = at.unwrap_or(playlist.items.len() + 1);
        if !(1..=playlist.items.len() + 1).contains(&place) {
            return Err(format!(
                "{} has {} items; a {what} can go at 1 to {}.",
                playlist.name,
                playlist.items.len(),
                playlist.items.len() + 1
            ));
        }
        let title = item.title().to_owned();
        playlist.items.insert(place - 1, item);
        let name = playlist.name.clone();
        state.services.changed = true;
        Ok(Reply::done(format!("Put the {what} {title} at place {place} in {name}.")))
    }

    /// Puts a Bible passage into a playlist, at a place (or at the end), as `bible REF --add`
    /// keeps one (the Bible page's translations and options, named in the language typed), but
    /// leaving the Bible page as it is. The same passage in the same translations goes in once.
    fn playlist_passage(&mut self, typed: &str, at: Option<usize>, list: Option<&str>) -> Result<Reply, String> {
        let typed = typed.trim();
        if typed.is_empty() {
            return Err("playlist passage REFERENCE [at N] [in PLAYLIST]".to_owned());
        }
        if let Some(problem) = self.bible_reference_problem(typed) {
            return Err(problem);
        }
        let list = self.playlist_at(list)?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let (_, saved) = state.bible.passage_typed(typed)?;
        let item = Item::Passage(saved);
        let title = item.title().to_owned();
        let name = state.services.list[list].name.clone();
        if let Some(was) = state.services.place_of(list, &item) {
            return Ok(Reply::done(format!("{title} is already in {name} (item {}).", was + 1)));
        }
        let playlist = &mut state.services.list[list];
        let place = at.unwrap_or(playlist.items.len() + 1);
        if !(1..=playlist.items.len() + 1).contains(&place) {
            return Err(format!(
                "{name} has {} items; a passage can go at 1 to {}.",
                playlist.items.len(),
                playlist.items.len() + 1
            ));
        }
        playlist.items.insert(place - 1, item);
        state.services.changed = true;
        Ok(Reply::done(format!("Put {title} at place {place} in {name}.")))
    }

    /// Fills a placeholder with a song, or with a picture or video from the media bin (`media
    /// ITEM`), as dropping one on it does.
    fn playlist_fill(&mut self, place: &str, with: &str, list: Option<&str>) -> Result<Reply, String> {
        let list = self.playlist_at(list)?;
        let at = {
            let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
            let items: Vec<String> =
                state.services.list[list].items.iter().map(|item| item.title().to_owned()).collect();
            find("placeholder", &items, place)?
        };
        let item = match with.trim().strip_prefix("media ") {
            Some(media) => {
                let names: Vec<String> = self.show.media().iter().map(|media| media.name.clone()).collect();
                let media = &self.show.media()[find("media item", &names, media)?];
                Item::Media { id: media.id, name: media.name.clone() }
            }
            None => {
                let song = self.find_song(with)?;
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let key = state.song_keys.get(song).cloned().ok_or("That song is gone.")?;
                let title = self.show.presentations().get(song).map(|found| found.title.clone()).unwrap_or_default();
                Item::song(key, title)
            }
        };
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let shown = state.services.current;
        let playlist = &state.services.list[list];
        let Some(Item::Placeholder { name: placeholder }) = playlist.items.get(at).cloned() else {
            let title = playlist.items.get(at).map(|item| item.title().to_owned()).unwrap_or_default();
            return Err(format!("{title} isn't a placeholder; only a placeholder can be filled."));
        };
        let name = playlist.name.clone();
        let title = item.title().to_owned();
        // A song it has elsewhere already isn't moved into the placeholder.
        if let Some(was) = state.services.place_of(list, &item).filter(|was| *was != at) {
            return Err(format!("{title} is already in {name} (item {}); a playlist has a song once.", was + 1));
        }
        // In the playlist in front it becomes the open item, as a drop on it does; in another,
        // the open item of the one in front stays.
        if list == shown {
            state.services.fill(at, item);
        } else {
            state.services.fill_in(list, at, item);
        }
        Ok(Reply::done(format!("Filled the placeholder {placeholder} in {name} with {title}.")))
    }

    /// `library new|rename|delete|add|remove …`; `None` for a word that isn't one of them.
    pub(crate) fn ctl_library_change(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "new" => self.library_new(&words.join(" ")),
            "rename" => match split_at_word(words, "to") {
                (old, Some(new)) => self.library_rename(&old.join(" "), &new),
                _ => Err("library rename OLD to NEW".to_owned()),
            },
            "delete" => self.library_delete(words),
            "add" => match split_at_word(words, "to") {
                (songs, Some(library)) if !songs.is_empty() => self.library_add(songs, &library),
                _ => Err("library add SONG… to LIBRARY   (put a name with spaces in quotes)".to_owned()),
            },
            "remove" => match split_at_word(words, "from") {
                (songs, Some(library)) if !songs.is_empty() => self.library_remove(songs, &library),
                _ => Err("library remove SONG… from LIBRARY".to_owned()),
            },
            _ => return None,
        };
        Some(result)
    }

    fn library_index(&self, typed: &str) -> Result<usize, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.libraries.iter().map(|l| l.name.clone()).collect();
        find("library", &names, typed)
    }

    fn library_new(&mut self, name: &str) -> Result<Reply, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("library new NAME".to_owned());
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if state.libraries.iter().any(|l| l.name.eq_ignore_ascii_case(name)) {
            return Err(format!("There is a library called {name} already."));
        }
        state.libraries.push(crate::libraries::Library::new(name.to_owned()));
        state.libraries_changed = true;
        Ok(Reply::done(format!("Made the library {name}.")))
    }

    fn library_rename(&mut self, old: &str, new: &str) -> Result<Reply, String> {
        let at = self.library_index(old)?;
        let new = new.trim();
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if new.is_empty()
            || state.libraries.iter().enumerate().any(|(i, l)| i != at && l.name.eq_ignore_ascii_case(new))
        {
            return Err(format!("There is a library called {new} already (or no new name was given)."));
        }
        let was = std::mem::replace(&mut state.libraries[at].name, new.to_owned());
        state.libraries_changed = true;
        Ok(Reply::done(format!("Renamed the library {was} to {new}.")))
    }

    /// At once, as in the window: `sp undo` puts it back (an old --yes is let by).
    fn library_delete(&mut self, words: &[String]) -> Result<Reply, String> {
        let name: Vec<String> = words.iter().filter(|w| *w != "--yes").cloned().collect();
        let at = self.library_index(&name.join(" "))?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let (id, title) = (state.libraries[at].id, state.libraries[at].name.clone());
        state.delete_library(id);
        Ok(Reply::done(format!("Deleted the library {title}; its songs stay in All Songs.")))
    }

    fn library_add(&mut self, songs: &[String], library: &str) -> Result<Reply, String> {
        let at = self.library_index(library)?;
        let mut keys = Vec::new();
        for typed in songs {
            let index = self.find_song_as_is(typed)?;
            let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
            let key = state.song_keys.get(index).cloned().ok_or("That song can't go in a library.")?;
            keys.push((key, self.show.presentations()[index].title.clone()));
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let into = &mut state.libraries[at];
        let (mut added, mut there) = (Vec::new(), Vec::new());
        for (key, title) in keys {
            if into.has(&key) {
                there.push(title);
            } else {
                into.songs.push(key);
                added.push(title);
            }
        }
        let name = into.name.clone();
        state.libraries_changed |= !added.is_empty();
        let mut text = match added.as_slice() {
            [] => format!("Nothing added to {name}."),
            many => format!("Added to {name}: {}.", many.join(", ")),
        };
        if !there.is_empty() {
            text.push_str(&format!(" In it already: {}.", there.join(", ")));
        }
        if added.is_empty() { Err(text) } else { Ok(Reply::done(text)) }
    }

    fn library_remove(&mut self, songs: &[String], library: &str) -> Result<Reply, String> {
        let at = self.library_index(library)?;
        let mut removed = Vec::new();
        for typed in songs {
            let index = self.find_song_as_is(typed)?;
            let title = self.show.presentations()[index].title.clone();
            let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
            let key = state.song_keys.get(index).cloned().unwrap_or_default();
            let from = &mut state.libraries[at];
            if from.has(&key) {
                from.songs.retain(|have| *have != key);
                removed.push(title);
            } else {
                return Err(format!("{title} isn't in {}.", from.name));
            }
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        state.libraries_changed = true;
        Ok(Reply::done(format!("Took out of {}: {}.", state.libraries[at].name, removed.join(", "))))
    }

    /// `song rename|duplicate|theme|transition|info …`; `None` for a word that isn't one of them.
    pub(crate) fn ctl_song_change(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "rename" => match split_at_word(words, "to") {
                (song, Some(title)) => self.song_rename(&song.join(" "), &title),
                _ => Err("song rename SONG to TITLE".to_owned()),
            },
            "duplicate" => {
                let (song, title) = split_at_word(words, "as");
                self.song_duplicate(&song.join(" "), title.as_deref())
            }
            "theme" => match words {
                [song, theme @ ..] if !theme.is_empty() => self.song_theme(song, &theme.join(" ")),
                _ => Err("song theme SONG THEME|main".to_owned()),
            },
            "transition" => match words {
                [song, kind @ ..] if !kind.is_empty() => self.song_transition(song, kind),
                _ => Err("song transition SONG KIND [SECONDS] | none".to_owned()),
            },
            "info" => match words {
                [song, settings @ ..] => self.song_info(song, settings),
                [] => Err("song info SONG [artist=… author=… publisher=… year=… ccli=…]".to_owned()),
            },
            _ => return None,
        };
        Some(result)
    }

    fn song_rename(&mut self, song: &str, title: &str) -> Result<Reply, String> {
        let at = self.find_song(song)?;
        let title = title.trim().to_owned();
        if title.is_empty() {
            return Err("song rename SONG to TITLE".to_owned());
        }
        let was = self.show.presentations()[at].title.clone();
        let same = self.listed_songs().iter().any(|(other, have)| *other != at && have.eq_ignore_ascii_case(&title));
        self.with_state(|state| state.pending_title = Some((at, title.clone())));
        self.handle_songs();
        self.save_unsaved();
        let mut text = format!("Renamed {was} to {title}.");
        if same {
            text.push_str(&format!(
                " Another song is called {title} too; give its number (sp songs) to tell them apart."
            ));
        }
        Ok(Reply::done(text))
    }

    fn song_duplicate(&mut self, song: &str, title: Option<&str>) -> Result<Reply, String> {
        let at = self.find_song(song)?;
        let was = self.show.presentations()[at].title.clone();
        let before = self.show.presentations().len();
        self.with_state(|state| state.song_request = Some((at, crate::home::SongRequest::Duplicate { item: None })));
        self.handle_song_request();
        let copy = (before < self.show.presentations().len()).then_some(self.show.presentations().len() - 1);
        let copy = copy.ok_or_else(|| format!("{was} could not be copied."))?;
        if let Some(title) = title.map(str::trim).filter(|t| !t.is_empty()) {
            self.with_state(|state| state.pending_title = Some((copy, title.to_owned())));
            self.handle_songs();
            self.save_unsaved();
        }
        let named = self.show.presentations()[copy].title.clone();
        Ok(Reply::done(format!("Copied {was} as {named}.")))
    }

    fn song_theme(&mut self, song: &str, theme: &str) -> Result<Reply, String> {
        let at = self.find_song(song)?;
        let (index, said) = if theme.eq_ignore_ascii_case("main") || theme.eq_ignore_ascii_case("none") {
            (None, "the main theme".to_owned())
        } else {
            let names: Vec<String> = self.themes.iter().map(|t| t.name.clone()).collect();
            let index = find("theme", &names, theme)?;
            (Some(index), format!("the theme {}", names[index]))
        };
        let title = self.show.presentations()[at].title.clone();
        self.with_state(|state| state.song_theme_request = Some((at, index)));
        self.handle_song_theme();
        Ok(Reply::done(format!("{title} now uses {said}.")))
    }

    fn song_transition(&mut self, song: &str, kind: &[String]) -> Result<Reply, String> {
        let at = self.find_song(song)?;
        let transition = parse_transition(kind)?;
        let title = self.show.presentations()[at].title.clone();
        self.with_state(|state| state.song_transition_request = Some((at, transition)));
        self.handle_song_transition();
        self.save_unsaved();
        Ok(Reply::done(format!("{title}'s transition: {}.", transition_words(transition.as_ref()))))
    }

    fn song_info(&mut self, song: &str, settings: &[String]) -> Result<Reply, String> {
        let at = self.find_song(song)?;
        let presentation = self.show.presentations()[at].clone();
        if settings.is_empty() {
            let theme = presentation
                .theme
                .and_then(|id| self.themes.iter().find(|t| t.id == id))
                .map_or_else(|| "the main theme".to_owned(), |t| t.name.clone());
            let c = &presentation.copyright;
            let lines = [
                presentation.title.clone(),
                format!("  slides: {}   groups: {}", presentation.slide_count(), presentation.groups.len()),
                format!("  theme: {theme}   transition: {}", transition_words(presentation.transition.as_ref())),
                format!("  artist: {}", c.artist),
                format!("  author: {}", c.author),
                format!("  publisher: {}", c.publisher),
                format!("  year: {}", c.year),
                format!("  ccli: {}", c.ccli_song),
                format!("  loops to the first slide: {}", if presentation.loops { "yes" } else { "no" }),
            ];
            let data = serde_json::json!({
                "title": presentation.title, "id": presentation.id, "slides": presentation.slide_count(),
                "theme": theme, "transition": presentation.transition, "loops": presentation.loops,
                "artist": c.artist, "author": c.author, "publisher": c.publisher, "year": c.year, "ccli": c.ccli_song,
            });
            return Ok(Reply::with(lines.join("\n"), data));
        }
        let mut details = presentation.copyright.clone();
        let mut said = Vec::new();
        for setting in settings {
            let (key, value) = setting.split_once('=').ok_or_else(|| format!("{setting} isn't key=value."))?;
            let field = match key.to_lowercase().as_str() {
                "artist" => &mut details.artist,
                "author" | "writer" | "writers" => &mut details.author,
                "publisher" | "copyright" => &mut details.publisher,
                "year" => &mut details.year,
                "ccli" | "ccli_song" => &mut details.ccli_song,
                other => return Err(format!("{other} isn't a song detail: artist, author, publisher, year, ccli.")),
            };
            let value = value.trim();
            match key.to_lowercase().as_str() {
                "year" if !value.is_empty() && !(value.len() == 4 && value.chars().all(|c| c.is_ascii_digit())) => {
                    return Err(format!("A year is four digits, not {value}."));
                }
                "ccli" | "ccli_song" if !value.chars().all(|c| c.is_ascii_digit()) => {
                    return Err(format!("A CCLI song number is digits only, not {value}."));
                }
                _ => {}
            }
            *field = value.to_owned();
            said.push(format!("{key} \u{201c}{value}\u{201d}"));
        }
        self.with_state(|state| state.copyright_request = Some((at, details)));
        self.handle_copyright();
        self.save_unsaved();
        Ok(Reply::done(format!("{}: {}.", presentation.title, said.join(", "))))
    }

    /// `slide add|delete|move|duplicate|group|label|enable|disable|notes|hotkey|transition|
    /// template|media …`; `None` for a word that isn't one of them.
    pub(crate) fn ctl_slide_change(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "add" => self.slide_add(words),
            "delete" => self.slide_edit(words, |song, index, _| {
                song.without_slide(index).map(|song| (song, format!("deleted slide {}", index + 1)))
            }),
            "duplicate" => self.slide_edit(words, |song, index, _| {
                let (_, slide) = song.slide(index)?;
                let copy = slide.clone();
                song.with_slides_after(index, &[copy])
                    .map(|song| (song, format!("copied slide {} after it", index + 1)))
            }),
            "move" => match words {
                [song, number, to, place] if to.eq_ignore_ascii_case("to") => {
                    let count = self.find_song_as_is(song).map(|at| self.show.presentations()[at].slide_count());
                    match (count, place.parse::<usize>()) {
                        (Ok(count), Ok(target)) if !(1..=count).contains(&target) => {
                            Err(format!("{song} has {count} slides; there is no place {target}."))
                        }
                        (_, Err(_)) => Err("slide move SONG N to M".to_owned()),
                        (_, Ok(target)) => {
                            self.slide_edit(&[song.clone(), number.clone()], move |song, index, _| {
                                let to = target - 1;
                                if to == index {
                                    return Some((song.clone(), format!("slide {} is there already", index + 1)));
                                }
                                let (moved, _) = song.with_slides_moved(&[index], to, to > index)?;
                                // It takes the group of the place it lands in.
                                let group = moved.slide(to).map(|(group, _)| group.name.clone()).unwrap_or_default();
                                let joined =
                                    if group.is_empty() { String::new() } else { format!("; it is now in {group}") };
                                Some((moved, format!("moved slide {} to {}{joined}", index + 1, to + 1)))
                            })
                        }
                    }
                }
                _ => Err("slide move SONG N to M".to_owned()),
            },
            "group" => self.slide_set(words, "slide group SONG N NAME", |song, index, value| {
                song.with_group_starting(index, value)
                    .map(|song| (song, format!("slide {} starts the group {value}", index + 1)))
            }),
            "label" => self.slide_change_one(words, "slide label SONG N LABEL|none", |slide, value| {
                slide.label = (!value.eq_ignore_ascii_case("none")).then(|| value.to_owned());
                Ok(format!("label {}", slide.label.as_deref().unwrap_or("none")))
            }),
            "notes" => self.slide_change_one(words, "slide notes SONG N TEXT|none", |slide, value| {
                slide.notes = (!value.eq_ignore_ascii_case("none")).then(|| value.replace("\\n", "\n"));
                Ok(if slide.notes.is_some() { "notes set".to_owned() } else { "no notes".to_owned() })
            }),
            "enable" | "disable" => {
                let on = word == "enable";
                self.slide_edit(words, move |song, index, _| {
                    let (_, slide) = song.slide(index)?;
                    let mut changed = slide.clone();
                    changed.enabled = on;
                    song.with_slide(index, changed).map(|song| {
                        (song, format!("slide {} {}", index + 1, if on { "on (shown)" } else { "off (passed over)" }))
                    })
                })
            }
            "checked" => self.slide_edit(words, |song, index, _| {
                let (_, slide) = song.slide(index)?;
                let said = match slide.check {
                    Some(_) => format!("slide {} marked as checked", index + 1),
                    None => format!("slide {} had nothing to check", index + 1),
                };
                let changed = sp_model::Slide { check: None, ..slide.clone() };
                song.with_slide(index, changed).map(|song| (song, said))
            }),
            "hotkey" => self.slide_change_one(words, "slide hotkey SONG N KEY|none", |slide, value| {
                slide.hot_key = match value {
                    none if none.eq_ignore_ascii_case("none") => None,
                    key if key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) => {
                        key.chars().next().map(|c| c.to_ascii_uppercase())
                    }
                    other => return Err(format!("A hot key is one letter or digit, not {other}.")),
                };
                Ok(format!("hot key {}", slide.hot_key.map_or("none".to_owned(), |c| c.to_string())))
            }),
            "transition" => match words {
                [song, number, kind @ ..] if !kind.is_empty() => match parse_transition(kind) {
                    Ok(transition) => self.slide_edit(&[song.clone(), number.clone()], move |song, index, _| {
                        let (_, slide) = song.slide(index)?;
                        let mut changed = slide.clone();
                        changed.transition = transition;
                        song.with_slide(index, changed).map(|song| {
                            (
                                song,
                                format!("slide {}'s transition: {}", index + 1, transition_words(transition.as_ref())),
                            )
                        })
                    }),
                    Err(problem) => Err(problem),
                },
                _ => Err("slide transition SONG N KIND [SECONDS] | none".to_owned()),
            },
            "template" => self.slide_template(words),
            "media" => self.slide_media(words),
            "web" => self.slide_web(words),
            _ => return None,
        };
        Some(result)
    }

    /// Finds the song and slide named by `words[0]` and `words[1]`, makes a changed copy of the
    /// song with `change` (given the song, the slide's index and the rest of the words), and puts
    /// it in place, undoably (so nothing needs --yes; an old one is let by).
    fn slide_edit(
        &mut self,
        words: &[String],
        change: impl FnOnce(&Presentation, usize, &str) -> Option<(Presentation, String)>,
    ) -> Result<Reply, String> {
        let words: Vec<String> = words.iter().filter(|w| *w != "--yes").cloned().collect();
        let [song, number, rest @ ..] = words.as_slice() else {
            return Err("Which song and slide? (SONG N)".to_owned());
        };
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let count = presentation.slide_count();
        let index = number
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=count).contains(n))
            .ok_or_else(|| crate::control::no_such_slide(&presentation.title, count, number))?
            - 1;
        let (song, said) = change(&presentation, index, &rest.join(" ")).ok_or("That can't be done to that slide.")?;
        self.change_song(at, song);
        Ok(Reply::done(format!("{}: {said}.", presentation.title)))
    }

    /// The same, for a change that needs the rest of the words.
    fn slide_set(
        &mut self,
        words: &[String],
        usage: &str,
        change: impl FnOnce(&Presentation, usize, &str) -> Option<(Presentation, String)>,
    ) -> Result<Reply, String> {
        if words.len() < 3 {
            return Err(usage.to_owned());
        }
        self.slide_edit(words, change)
    }

    /// The same, for a change made to the slide itself, which may refuse the value.
    fn slide_change_one(
        &mut self,
        words: &[String],
        usage: &str,
        change: impl FnOnce(&mut sp_model::Slide, &str) -> Result<String, String>,
    ) -> Result<Reply, String> {
        if words.len() < 3 {
            return Err(usage.to_owned());
        }
        let mut refused = None;
        let reply = self.slide_edit(words, |song, index, value| {
            let (_, slide) = song.slide(index)?;
            let mut changed = slide.clone();
            match change(&mut changed, value) {
                Ok(said) => song.with_slide(index, changed).map(|song| (song, format!("slide {}: {said}", index + 1))),
                Err(problem) => {
                    refused = Some(problem);
                    None
                }
            }
        });
        match refused {
            Some(problem) => Err(problem),
            None => reply,
        }
    }

    /// `slide add SONG [after N] [group NAME] WORDS`.
    fn slide_add(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "slide add SONG [after N] [group NAME] WORDS   (\\n starts a new line)";
        let [song, rest @ ..] = words else { return Err(usage.to_owned()) };
        let mut rest: Vec<String> = rest.to_vec();
        let mut after = None;
        let mut group = None;
        loop {
            match rest.first().map(|w| w.to_lowercase()).as_deref() {
                Some("after") if rest.len() > 1 => {
                    after = Some(rest[1].parse::<usize>().map_err(|_| usage.to_owned())?);
                    rest.drain(..2);
                }
                Some("group") if rest.len() > 1 => {
                    group = Some(rest[1].clone());
                    rest.drain(..2);
                }
                _ => break,
            }
        }
        let text = rest.join(" ").replace("\\n", "\n");
        if text.trim().is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let count = presentation.slide_count();
        if let Some(after) = after
            && !(1..=count).contains(&after)
        {
            return Err(crate::control::no_such_slide(&presentation.title, count, &after.to_string()));
        }
        // A new slide at the end, laid out like the one before it, with the words in it; then
        // moved after the slide named, and starting a group if one was named.
        let (mut song, new) = presentation.with_slide_added();
        song = song.with_slide_text(new, &text, "").ok_or("The slide could not be made.")?;
        let mut place = new;
        if let Some(after) = after
            && after < count
        {
            let (moved, _) = song.with_slides_moved(&[new], after - 1, true).ok_or("The slide could not be moved.")?;
            song = moved;
            place = after;
        }
        if let Some(group) = &group {
            // Only the new slide goes into the group: a slide after it that was in the same group
            // stays in that group (which then goes on after the new one).
            let was = song.slide(place).map(|(g, _)| g.name.clone()).unwrap_or_default();
            let next_was_same = song.slide(place + 1).is_some_and(|(g, _)| g.name == was)
                && song
                    .slide(place)
                    .is_some_and(|(g, _)| g.slides.first().map(|s| s.id) != song.slide(place + 1).map(|(_, s)| s.id));
            song = song.with_group_starting(place, group).ok_or("The group could not be made.")?;
            if next_was_same && !was.is_empty() {
                song = song.with_group_starting(place + 1, &was).ok_or("The group could not be made.")?;
            }
        }
        self.change_song(at, song);
        Ok(Reply::done(format!("{}: added slide {}.", presentation.title, place + 1)))
    }

    /// `slide template SONG N NAME|auto`: the song theme's template the slide is drawn with.
    fn slide_template(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "slide template SONG N TEMPLATE|auto";
        let [song, number, name @ ..] = words else { return Err(usage.to_owned()) };
        if name.is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.find_song(song)?;
        let theme = self.theme_of(at).clone();
        let wanted = name.join(" ");
        let template = if wanted.eq_ignore_ascii_case("auto") {
            None
        } else {
            let names: Vec<String> = theme.templates.iter().map(|t| t.name.clone()).collect();
            let at = find("template", &names, &wanted)
                .map_err(|problem| format!("{problem} The theme {} has: {}, or auto.", theme.name, names.join(", ")))?;
            Some(theme.templates[at].clone())
        };
        let id = template.as_ref().map(|t| t.id);
        let said =
            template.map_or_else(|| "the template that suits it".to_owned(), |t| format!("the template {}", t.name));
        self.slide_edit(&[song.clone(), number.clone()], move |song, index, _| {
            let (_, slide) = song.slide(index)?;
            let mut changed = slide.clone();
            changed.template = id;
            song.with_slide(index, changed).map(|song| (song, format!("slide {} uses {said}", index + 1)))
        })
    }

    /// `slide media SONG N ITEM|none`: the picture or video from the media bin put behind the
    /// words when the slide goes on screen.
    fn slide_media(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "slide media SONG N MEDIA-ITEM|none";
        let [song, number, name @ ..] = words else { return Err(usage.to_owned()) };
        if name.is_empty() {
            return Err(usage.to_owned());
        }
        let wanted = name.join(" ");
        let item = if wanted.eq_ignore_ascii_case("none") {
            None
        } else {
            let names: Vec<String> = self.show.media().iter().map(|m| m.name.clone()).collect();
            let at = find("media item", &names, &wanted)?;
            Some((self.show.media()[at].id, names[at].clone()))
        };
        self.slide_edit(&[song.clone(), number.clone()], move |song, index, _| {
            let (_, slide) = song.slide(index)?;
            let mut changed = slide.clone();
            changed.actions.retain(|action| !matches!(action, ShowAction::Media { .. }));
            if let Some((id, _)) = &item {
                changed.actions.push(ShowAction::Media { id: *id });
            }
            let said =
                item.as_ref().map_or_else(|| "no media".to_owned(), |(_, name)| format!("shows {name} behind it"));
            song.with_slide(index, changed).map(|song| (song, format!("slide {} {said}", index + 1)))
        })
    }

    /// `slide web SONG N URL|off`: the address of the slide's web page box (one filling the slide,
    /// behind its words, is made if it has none), or the box taken off.
    fn slide_web(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "slide web SONG N URL|off   (an https:// address)";
        let [song, number, address] = words else { return Err(usage.to_owned()) };
        let off = address.eq_ignore_ascii_case("off") || address.eq_ignore_ascii_case("none");
        let address = address.trim().to_owned();
        let address = if !off && !address.contains("://") { format!("https://{address}") } else { address };
        if !off && !sp_video::web::allowed(&address) {
            return Err(format!("{address} isn't an address SoftPresenter shows: it must start with https://."));
        }
        self.slide_edit(&[song.clone(), number.clone()], move |song, index, _| {
            let (_, slide) = song.slide(index)?;
            let mut changed = slide.clone();
            let said = if off {
                changed.objects.retain(|object| !matches!(object.kind, ObjectKind::Web { .. }));
                format!("slide {} has no web page", index + 1)
            } else {
                match changed.objects.iter_mut().find_map(|object| match &mut object.kind {
                    ObjectKind::Web { url } => Some(url),
                    _ => None,
                }) {
                    Some(url) => *url = address.clone(),
                    None => {
                        let mut web = sp_model::Object::web(address.clone());
                        web.frame = Some(sp_model::SlideRect {
                            x: 0.0,
                            y: 0.0,
                            w: sp_model::SLIDE_WIDTH,
                            h: sp_model::SLIDE_HEIGHT,
                        });
                        changed.objects.insert(0, web);
                    }
                }
                format!("slide {} shows the web page {address}", index + 1)
            };
            song.with_slide(index, changed).map(|song| (song, said))
        })
    }

    /// `search WORDS`: songs whose title or words have them, as Command-F finds them.
    pub(crate) fn ctl_search(&self, typed: &str) -> Result<Reply, String> {
        let wanted = typed.trim().to_lowercase();
        if wanted.is_empty() {
            return Err("search WORDS".to_owned());
        }
        let mut lines = Vec::new();
        let mut data = Vec::new();
        for (number, (at, title)) in self.listed_songs().into_iter().enumerate() {
            let Some(song) = self.show.presentations().get(at) else { continue };
            let in_title = title.to_lowercase().contains(&wanted);
            let slides: Vec<usize> = song
                .slides()
                .enumerate()
                .filter(|(_, (_, slide))| {
                    slide.objects.iter().any(|object| match &object.kind {
                        ObjectKind::Text { text, .. } => text.to_lowercase().contains(&wanted),
                        _ => false,
                    })
                })
                .map(|(index, _)| index + 1)
                .collect();
            if !in_title && slides.is_empty() {
                continue;
            }
            let where_ = match slides.as_slice() {
                [] => "the title".to_owned(),
                [one] => format!("slide {one}"),
                many => format!("slides {}", many.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")),
            };
            lines.push(format!("{} ({where_})", title));
            data.push(serde_json::json!({
                "song": title, "number": number + 1, "id": song.id, "title": in_title, "slides": slides,
            }));
        }
        if lines.is_empty() {
            return Err(format!("No song has \u{201c}{}\u{201d} in its title or words.", typed.trim()));
        }
        Ok(Reply::with(numbered(&lines), serde_json::json!(data)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_are_read_by_name_and_seconds() {
        let words = |text: &str| text.split(' ').map(str::to_owned).collect::<Vec<_>>();
        let fade = parse_transition(&words("fade black 1.5")).unwrap().unwrap();
        assert_eq!((fade.kind, fade.seconds), (TransitionKind::FadeBlack, 1.5));
        assert_eq!(parse_transition(&words("Dissolve")).unwrap().unwrap().kind, TransitionKind::Dissolve);
        assert_eq!(parse_transition(&words("cut")).unwrap().unwrap().seconds, 0.0);
        assert_eq!(parse_transition(&words("none")).unwrap(), None);
        assert!(parse_transition(&words("sparkle")).unwrap_err().contains("Dissolve"));
        assert!(parse_transition(&words("push 9")).is_err());
    }

    #[test]
    fn playlist_items_say_their_kind_and_keep_their_old_line() {
        let id = sp_model::Uuid::new_v4();
        let song = Item::song(id.to_string(), "River Song".to_owned());
        let (line, data) = item_entry(2, &song, &Found::Song { id, number: Some(7) });
        assert_eq!(line, "River Song");
        assert_eq!(data["item"], 2);
        assert_eq!(data["kind"], "song");
        assert_eq!(data["text"], "River Song");
        assert_eq!(data["id"], serde_json::json!(id));
        assert_eq!(data["number"], 7);
        assert_eq!(data["missing"], false);

        // A song that has gone keeps the id it was named by.
        let (line, data) = item_entry(3, &song, &Found::Nothing);
        assert_eq!(line, "River Song (missing)");
        assert_eq!((data["missing"].clone(), data["id"].clone()), (serde_json::json!(true), serde_json::json!(id)));
        assert!(data["number"].is_null());
        let by_title = Item::song("title:Old Hymn".to_owned(), "Old Hymn".to_owned());
        assert!(item_entry(1, &by_title, &Found::Nothing).1["id"].is_null(), "a title is not an id");

        let voxlive = Item::song("title:VoxLive".to_owned(), "VoxLive".to_owned());
        let (line, data) = item_entry(1, &voxlive, &Found::VoxLive);
        assert_eq!((line.as_str(), data["kind"].as_str()), ("VoxLive", Some("voxlive")));

        let video = Item::Media { id, name: "Bars.mp4".to_owned() };
        let (line, data) = item_entry(4, &video, &Found::Media { number: 2, video: true });
        assert_eq!(line, "Bars.mp4 (video)");
        assert_eq!((data["kind"].as_str(), data["media_type"].as_str()), (Some("media"), Some("video")));
        assert_eq!(data["number"], 2);
        assert_eq!(item_entry(4, &video, &Found::Nothing).0, "Bars.mp4 (missing picture or video)");
        let picture = Item::Media { id, name: "Blue.png".to_owned() };
        assert_eq!(item_entry(4, &picture, &Found::Media { number: 1, video: false }).0, "Blue.png (picture)");

        let loop_ = Item::MediaPlaylist { id, name: "Announcements".to_owned() };
        assert_eq!(item_entry(5, &loop_, &Found::MediaPlaylist { number: 1 }).0, "Announcements (media playlist)");
        let (line, data) = item_entry(5, &loop_, &Found::Nothing);
        assert_eq!(
            (line.as_str(), data["kind"].as_str()),
            ("Announcements (missing media playlist)", Some("media_playlist"))
        );

        let header = Item::Header { name: "Worship".to_owned() };
        assert_eq!(item_entry(6, &header, &Found::Nothing).0, "— Worship —");
        let place = Item::Placeholder { name: "Sermon slides".to_owned() };
        let (line, data) = item_entry(7, &place, &Found::Nothing);
        assert_eq!((line.as_str(), data["title"].as_str()), ("Sermon slides (placeholder)", Some("Sermon slides")));

        let passage = Item::Passage(crate::service::SavedPassage {
            lookup: "John 3:16-18".to_owned(),
            numbering: None,
            rows: vec![crate::service::SavedRow { bible: "kjv".to_owned(), verse: None, reference: None }],
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference_style: String::new(),
            show_with: None,
        });
        let (line, data) = item_entry(8, &passage, &Found::Nothing);
        assert_eq!((line.as_str(), data["kind"].as_str()), ("John 3:16-18", Some("passage")));
        assert_eq!(data["bibles"], serde_json::json!(["kjv"]));
    }

    #[test]
    fn a_page_has_an_items_words_in_the_windows_language_and_sp_in_english() {
        use crate::i18n::{Language, english_of, with_language};
        let id = sp_model::Uuid::new_v4();
        let picture = Item::Media { id, name: "pic001.png".to_owned() };
        let found = Found::Media { number: 1, video: false };
        let (line, data) = with_language(Language::Spanish, || item_entry(4, &picture, &found));
        assert_eq!((line.as_str(), data["text"].as_str()), ("pic001.png (picture)", Some("pic001.png (picture)")));
        let for_page = with_language(Language::Spanish, || item_text(&picture, &found));
        assert_eq!(for_page, "pic001.png (imagen)");
        assert_eq!(english_of(&for_page), "pic001.png (picture)");
        let place = Item::Placeholder { name: "Sermón".to_owned() };
        assert_eq!(with_language(Language::Spanish, || item_text(&place, &Found::Nothing)), "Sermón (hueco)");
        let song = Item::song("title:Gone".to_owned(), "Gone".to_owned());
        assert_eq!(with_language(Language::Spanish, || item_text(&song, &Found::Nothing)), "Gone (falta)");
        // Names, headers, passages and songs found are the same in every language.
        let header = Item::Header { name: "Adoración".to_owned() };
        assert_eq!(with_language(Language::Spanish, || item_text(&header, &Found::Nothing)), "— Adoración —");
        assert_eq!(with_language(Language::English, || item_text(&picture, &found)), "pic001.png (picture)");
    }

    #[test]
    fn words_split_at_the_last_keyword() {
        let words: Vec<String> = ["River", "Song", "to", "Sunday", "Morning"].iter().map(|w| w.to_string()).collect();
        let (head, tail) = split_at_word(&words, "to");
        assert_eq!((head.join(" "), tail.as_deref()), ("River Song".to_owned(), Some("Sunday Morning")));
        assert_eq!(split_at_word(&words, "from").1, None);
    }
}
