//! Services: the running order for a gathering. A service lists songs, saved Bible passages,
//! pictures and videos, media playlists and placeholders in the order they will be used, so
//! everything is one click away during it.

use crate::i18n::{t, tf};
use serde::{Deserialize, Serialize};
use sp_model::Uuid;

/// One translation of a saved passage: which Bible, and the theme boxes its words and
/// reference go in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRow {
    /// The Bible's short name for files, such as `kjv`.
    pub bible: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verse: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// A Bible passage kept for later: what was typed, in which translations, made how.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPassage {
    /// What to type in Scripture Lookup to get it, such as `John 3:16-18`.
    pub lookup: String,
    /// The verse numbering `lookup` is in (`english`, `synodal`, `original`), so it opens as the
    /// same verses whatever the Bibles' order; none (a file from before) for the first
    /// translation's when it is opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numbering: Option<String>,
    pub rows: Vec<SavedRow>,
    #[serde(default)]
    pub show_verse_numbers: bool,
    #[serde(default = "yes")]
    pub break_on_new_verse: bool,
    #[serde(default)]
    pub display_translation: bool,
    /// `verse`, `passage_each`, `passage_last` or `none`.
    #[serde(default)]
    pub reference_style: String,
    /// The theme version the passage is shown in, as a playlist item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_with: Option<ShowWith>,
}

fn yes() -> bool {
    true
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// What a playlist item asks of the themes when it goes on screen: to show it in one of their
/// versions (the offering, say), and whether that version stays on after it, until it is
/// switched back. It is the playlist's; the song itself doesn't change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowWith {
    pub version: Uuid,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keep: bool,
}

/// One thing in a service.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// A song from the library. `key` finds it again next time; `title` is shown if it cannot
    /// be found.
    Song {
        key: String,
        title: String,
        /// The theme version it is shown in, if not the usual look.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show_with: Option<ShowWith>,
    },
    Passage(SavedPassage),
    /// A heading that groups the items under it, such as "Worship" or "Sermon".
    Header {
        name: String,
    },
    /// A picture or video from the media bin (a countdown video, say): a click on its tile in
    /// the slide area shows it. `id` finds it in the bin; `name` is shown if it has gone.
    Media {
        id: Uuid,
        name: String,
    },
    /// A media playlist (an announcement loop, say): a click on its tile plays it, item after
    /// item, round and round. `id` finds it; `name` is shown if it has gone.
    MediaPlaylist {
        id: Uuid,
        name: String,
    },
    /// A place kept for something to come ("Sermon slides"): a song dropped on it takes its
    /// place. It shows nothing.
    Placeholder {
        name: String,
    },
}

impl Item {
    /// A song from the library, by the key that finds it and its title, in the usual look.
    pub fn song(key: impl Into<String>, title: impl Into<String>) -> Item {
        Item::Song { key: key.into(), title: title.into(), show_with: None }
    }

    /// Whether the item can be shown in a theme version: a song or a passage.
    pub fn can_show_with(&self) -> bool {
        matches!(self, Item::Song { .. } | Item::Passage(_))
    }

    /// The theme version the item is shown in, if it is set to one.
    pub fn show_with(&self) -> Option<ShowWith> {
        match self {
            Item::Song { show_with, .. } => *show_with,
            Item::Passage(saved) => saved.show_with,
            _ => None,
        }
    }

    /// Where a song or a passage keeps the version it is shown in; other kinds have none.
    pub fn show_with_mut(&mut self) -> Option<&mut Option<ShowWith>> {
        match self {
            Item::Song { show_with, .. } => Some(show_with),
            Item::Passage(saved) => Some(&mut saved.show_with),
            _ => None,
        }
    }

    /// What the item is called in the list.
    pub fn title(&self) -> &str {
        match self {
            Item::Song { title, .. } => title,
            Item::Passage(passage) => &passage.lookup,
            Item::Header { name }
            | Item::Media { name, .. }
            | Item::MediaPlaylist { name, .. }
            | Item::Placeholder { name } => name,
        }
    }

    /// Whether this is the same thing as another item, which a playlist takes only once: a song
    /// with the same key, or a passage of the same verses (the same reference, in the same
    /// numbering when both say) in the same translations, in the same order, however it is set
    /// out. The same verses in other translations are another item.
    pub fn same_as(&self, other: &Item) -> bool {
        match (self, other) {
            (Item::Song { key, .. }, Item::Song { key: other, .. }) => key == other,
            (Item::Passage(one), Item::Passage(other)) => {
                let numbering = match (&one.numbering, &other.numbering) {
                    (Some(one), Some(other)) => one == other,
                    _ => true,
                };
                one.lookup.trim().to_lowercase() == other.lookup.trim().to_lowercase()
                    && numbering
                    && one.rows.iter().map(|row| &row.bible).eq(other.rows.iter().map(|row| &row.bible))
            }
            _ => false,
        }
    }

    /// A kind of item a services file before version 3 can't hold.
    fn needs_version_3(&self) -> bool {
        matches!(self, Item::Media { .. } | Item::MediaPlaylist { .. } | Item::Placeholder { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    pub items: Vec<Item>,
    /// The stable ids of the items ([`Service::uids`]).
    #[serde(default)]
    pub uids: UidBook,
    /// Where the playlist came from, when an outside planner made it (TimelyChurch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ServiceOrigin>,
}

/// The mark of a playlist made from a service planned elsewhere (TimelyChurch): which service
/// it is, so the next check finds it again, and whether the church has changed it since. An
/// older SoftPresenter drops the mark when it saves the playlists; the playlist stays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceOrigin {
    /// The planner (`timelychurch`).
    pub source: String,
    /// The planner's key for the service (`church.example.org/plan/57`).
    pub key: String,
    /// The fingerprint of the playlist (its name and items) as it was made: it is untouched
    /// while it still has it.
    pub made: String,
    /// The service's date, `YYYY-MM-DD`, when the planner gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The planner changed a service whose playlist the church changed here: the fingerprint
    /// of the items it would make now, offered and never forced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newer: Option<String>,
    /// The planner no longer has the service. The playlist stays until someone deletes it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub removed: bool,
}

/// Remembers a stable id (`uid`) for each item of a playlist, so a page that names an item by it
/// still finds it after the items are moved about, saved, or the app is opened again. Items
/// are changed in many places, so the book doesn't watch them: whoever asks for the ids
/// ([`Service::uids`]) first has it compare the items with what it last saw. An item that is the
/// same as one it knew keeps that one's id, wherever it has gone; an item changed in place (a
/// rename, a theme version) keeps the id of the one of its kind that went; anything else is
/// new. Two identical items (two headers of one name) are told apart by their order only.
#[derive(Default)]
pub struct UidBook(std::sync::Mutex<Book>);

#[derive(Default, Clone)]
struct Book {
    /// The items as they were when the ids were last settled; `None` for a book just read.
    seen: Option<Vec<Item>>,
    uids: Vec<Uuid>,
}

impl Clone for UidBook {
    fn clone(&self) -> UidBook {
        UidBook(std::sync::Mutex::new(self.book().clone()))
    }
}

impl std::fmt::Debug for UidBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UidBook")
    }
}

/// Ids are bookkeeping: two playlists with the same items are the same playlist.
impl PartialEq for UidBook {
    fn eq(&self, _: &UidBook) -> bool {
        true
    }
}

impl Eq for UidBook {}

impl Serialize for UidBook {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.book().uids.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UidBook {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<UidBook, D::Error> {
        let uids = Vec::<Uuid>::deserialize(deserializer)?;
        Ok(UidBook(std::sync::Mutex::new(Book { seen: None, uids })))
    }
}

impl UidBook {
    fn book(&self) -> std::sync::MutexGuard<'_, Book> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The id of each of `items`, in order.
    fn settle(&self, items: &[Item]) -> Vec<Uuid> {
        let mut book = self.book();
        let Book { seen, uids } = &mut *book;
        match seen.as_deref() {
            Some(old) if old == items && uids.len() == items.len() => return uids.clone(),
            // A file just read: its ids stand if there is one for each item and none repeats.
            None => {
                let mut once = std::collections::HashSet::new();
                if !(uids.len() == items.len() && uids.iter().all(|uid| once.insert(*uid))) {
                    *uids = items.iter().map(|_| Uuid::new_v4()).collect();
                }
                *seen = Some(items.to_vec());
                return uids.clone();
            }
            Some(_) => {}
        }
        let old = seen.take().unwrap_or_default();
        let mut taken = vec![false; old.len()];
        let mut now: Vec<Option<Uuid>> = vec![None; items.len()];
        // The same item, where it was if it still is there, else the first of its kind not yet used.
        for (at, item) in items.iter().enumerate() {
            let spot = if old.get(at) == Some(item) && !taken[at] {
                Some(at)
            } else {
                (0..old.len()).find(|&place| !taken[place] && old[place] == *item)
            };
            if let Some(place) = spot.filter(|place| *place < uids.len()) {
                taken[place] = true;
                now[at] = Some(uids[place]);
            }
        }
        // What is left, changed in place: the same kind of item takes the id of the one that went.
        for (at, item) in items.iter().enumerate() {
            if now[at].is_some() {
                continue;
            }
            let kind = std::mem::discriminant(item);
            let spot = (0..old.len())
                .find(|&place| !taken[place] && place < uids.len() && std::mem::discriminant(&old[place]) == kind);
            if let Some(place) = spot {
                taken[place] = true;
                now[at] = Some(uids[place]);
            }
        }
        let settled: Vec<Uuid> = now.into_iter().map(|uid| uid.unwrap_or_else(Uuid::new_v4)).collect();
        *uids = settled.clone();
        *seen = Some(items.to_vec());
        settled
    }
}

impl Service {
    /// The stable id of each item, in order: kept when items are moved, when the playlist is
    /// saved and read again, and after a restart. Written to the services file.
    pub fn uids(&self) -> Vec<Uuid> {
        self.uids.settle(&self.items)
    }

    /// The place of the item with this id.
    pub fn place_of_uid(&self, uid: Uuid) -> Option<usize> {
        self.uids().iter().position(|have| *have == uid)
    }

    /// A short word that changes whenever the playlist's items do (added, removed, moved,
    /// renamed, set to a theme version): a remote page names an item by its place, with this,
    /// so a playlist changed meanwhile is noticed rather than the wrong item shown.
    pub fn rev(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        serde_json::to_string(&self.items).unwrap_or_default().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

/// Every service, and which one is in front.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Services {
    pub list: Vec<Service>,
    pub current: usize,
    /// The item last clicked in the service in front, which is shown as chosen.
    #[serde(skip)]
    pub chosen: Option<usize>,
    /// Set by every change. The app writes the services to their file and clears this.
    #[serde(skip)]
    pub changed: bool,
    /// True while the playlists are the one empty playlist the app made because there was no
    /// file to read (a first start), unchanged and never written: its name follows the language
    /// set ([`Services::follow_language`]). Never true for playlists read from their file.
    #[serde(skip)]
    pub fresh: bool,
}

/// The first playlist's name as the app gives it: its key, and its English.
pub const FIRST_PLAYLIST: (&str, &str) = crate::i18n::tk("left.playlist.first", "Service");

/// The name the app gives the first playlist, in the language set.
pub fn first_playlist_name() -> &'static str {
    crate::i18n::tg(FIRST_PLAYLIST.0, FIRST_PLAYLIST.1)
}

impl Default for Services {
    fn default() -> Services {
        Services {
            list: vec![Service {
                name: first_playlist_name().to_owned(),
                items: Vec::new(),
                uids: Default::default(),
                origin: None,
            }],
            current: 0,
            chosen: None,
            changed: false,
            fresh: true,
        }
    }
}

/// Where an item at `place` is after the item at `from` is moved to `to` (both places in the
/// list as it is before and after the move).
pub fn moved_place(place: usize, from: usize, to: usize) -> usize {
    if place == from {
        to
    } else if from < place && place <= to {
        place - 1
    } else if to <= place && place < from {
        place + 1
    } else {
        place
    }
}

const FORMAT: &str = "softpresenter-services";
/// The newest version of the services file: 3, with pictures and videos, media playlists and
/// placeholders in playlists; 2, library songs named by id (1 named them by their file's path or
/// title, and still reads). A file is written as 3 only when a playlist holds one of the new
/// kinds, so an older SoftPresenter still opens the rest.
const VERSION: u32 = 3;
/// What a services file saved before the project was renamed says it is; it still opens.
const OLD_FORMAT: &str = "presentercraft-services";

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    #[serde(flatten)]
    services: Services,
}

impl Services {
    /// While the playlists are as the app made them at a first start ([`Services::fresh`]), the
    /// first playlist is named in the language set: the computer's language is read on a thread
    /// and may answer after the playlist was made, and the setup's first step (or Settings) may
    /// choose another. Once anything changes the playlists, the name is the church's. Called
    /// every frame; true when it renamed the playlist.
    pub fn follow_language(&mut self) -> bool {
        if !self.first_is_the_apps() {
            return false;
        }
        let name = first_playlist_name();
        match self.list.as_mut_slice() {
            [only] if only.name != name => {
                only.name = name.to_owned();
                true
            }
            _ => false,
        }
    }

    /// Whether the playlists are still the one empty playlist the app made at a first start,
    /// with nothing changed ([`Services::fresh`]): its name is then the app's, [`FIRST_PLAYLIST`],
    /// shown in the language set, not yet the church's.
    pub fn first_is_the_apps(&self) -> bool {
        self.fresh && !self.changed && matches!(self.list.as_slice(), [only] if only.items.is_empty())
    }

    /// The place of the service in front among the list.
    pub fn front_at(&self) -> usize {
        self.current.min(self.list.len() - 1)
    }

    /// The service in front. There is always one.
    pub fn front(&self) -> &Service {
        &self.list[self.current.min(self.list.len() - 1)]
    }

    fn front_mut(&mut self) -> &mut Service {
        let at = self.current.min(self.list.len() - 1);
        self.changed = true;
        &mut self.list[at]
    }

    /// Where a playlist (by its place among the playlists) already has this item, which it then
    /// takes only once: the same song, or the same passage in the same translations (see
    /// [`Item::same_as`]).
    pub fn place_of(&self, playlist: usize, item: &Item) -> Option<usize> {
        self.list.get(playlist)?.items.iter().position(|have| have.same_as(item))
    }

    /// Adds an item to the end of the service in front and chooses it. A song or passage already
    /// in the service is chosen where it is, not added again. Returns true if it was added.
    pub fn add(&mut self, item: Item) -> bool {
        if let Some(at) = self.place_of(self.front_at(), &item) {
            self.chosen = Some(at);
            return false;
        }
        let items = &mut self.front_mut().items;
        items.push(item);
        self.chosen = Some(self.front().items.len() - 1);
        true
    }

    /// Adds an item to the end of a playlist that need not be the one in front; a song or
    /// passage already in it is not added again. Returns true if it was added.
    pub fn add_to(&mut self, playlist: usize, item: Item) -> bool {
        if self.place_of(playlist, &item).is_some() {
            return false;
        }
        let Some(list) = self.list.get_mut(playlist) else { return false };
        list.items.push(item);
        self.changed = true;
        true
    }

    /// Puts an item into the service in front at a place (`slot` items above it) and chooses
    /// it. A song or passage already in the service is chosen where it is, not added again.
    pub fn insert(&mut self, slot: usize, item: Item) {
        if let Some(at) = self.place_of(self.front_at(), &item) {
            self.chosen = Some(at);
            return;
        }
        let at = slot.min(self.front().items.len());
        self.front_mut().items.insert(at, item);
        self.chosen = Some(at);
    }

    /// Puts an item into the service in front at a place (`slot` items above it) without
    /// opening it: the open item stays open where it now is. A song or passage already in the
    /// service is not added again. Returns where the item went, if it went in.
    pub fn put_at(&mut self, slot: usize, item: Item) -> Option<usize> {
        if self.place_of(self.front_at(), &item).is_some() {
            return None;
        }
        let at = slot.min(self.front().items.len());
        self.front_mut().items.insert(at, item);
        if let Some(open) = self.chosen.as_mut()
            && *open >= at
        {
            *open += 1;
        }
        Some(at)
    }

    /// Puts items into a playlist (by its place), one after another in their order: after the
    /// item `after` when it is the playlist in front (the last one put in is chosen), else at
    /// its end. A song or passage it has already isn't added again. Returns, for each, whether
    /// it went in.
    pub fn put_in(&mut self, playlist: usize, after: Option<usize>, items: Vec<Item>) -> Vec<bool> {
        let front = playlist == self.front_at();
        let mut slot = after.filter(|_| front).map(|after| after + 1);
        let mut went = Vec::new();
        for item in items {
            if self.place_of(playlist, &item).is_some() {
                went.push(false);
                continue;
            }
            match slot.as_mut() {
                Some(slot) => {
                    let at = (*slot).min(self.front().items.len());
                    self.front_mut().items.insert(at, item);
                    self.chosen = Some(at);
                    *slot = at + 1;
                    went.push(true);
                }
                None => went.push(self.add_to(playlist, item)),
            }
        }
        went
    }

    /// Puts an item in a placeholder's place in the service in front, and chooses it. Returns
    /// false if the item there isn't a placeholder, or the service has the item elsewhere
    /// already (see [`fill_in`](Self::fill_in)).
    pub fn fill(&mut self, index: usize, item: Item) -> bool {
        let front = self.front_at();
        match self.fill_in(front, index, item) {
            Some(at) => {
                self.chosen = Some(at);
                true
            }
            None => false,
        }
    }

    /// Puts an item in a placeholder's place in a playlist (by its place among the playlists),
    /// which need not be the one in front. Returns where the item is now, or `None` if the item
    /// there isn't a placeholder, or the playlist has the song (or passage) elsewhere already: a
    /// playlist has it once, and it is never moved out of its place silently (see
    /// [`place_of`](Self::place_of), which says where).
    pub fn fill_in(&mut self, playlist: usize, index: usize, item: Item) -> Option<usize> {
        if self.place_of(playlist, &item).is_some_and(|was| was != index) {
            return None;
        }
        let items = &mut self.list.get_mut(playlist)?.items;
        if !matches!(items.get(index), Some(Item::Placeholder { .. })) {
            return None;
        }
        items[index] = item;
        self.changed = true;
        Some(index)
    }

    /// Takes items out of a playlist (by its place among the playlists), all at once. Returns
    /// each item taken out with the place it had, in order, for [`put_back`](Self::put_back). In
    /// the playlist in front, the open item stays open where it now is, unless it went.
    pub fn remove_items(&mut self, playlist: usize, places: &[usize]) -> Vec<(usize, Item)> {
        let Some(list) = self.list.get_mut(playlist) else { return Vec::new() };
        let mut places: Vec<usize> = places.iter().copied().filter(|at| *at < list.items.len()).collect();
        places.sort_unstable();
        places.dedup();
        let mut removed: Vec<(usize, Item)> = places.iter().rev().map(|at| (*at, list.items.remove(*at))).collect();
        removed.reverse();
        if removed.is_empty() {
            return removed;
        }
        self.changed = true;
        if playlist == self.current {
            self.chosen = self
                .chosen
                .filter(|open| places.binary_search(open).is_err())
                .map(|open| open - places.iter().filter(|at| **at < open).count());
        }
        removed
    }

    /// Puts items taken out by [`remove_items`](Self::remove_items) back where they were, in
    /// the same playlist. Returns false (and changes nothing) if that playlist is no longer
    /// there or has become too short for them.
    pub fn put_back(&mut self, playlist: usize, removed: &[(usize, Item)]) -> bool {
        let Some(list) = self.list.get_mut(playlist) else { return false };
        // Each goes in at its old place, the earliest first: then every place is as it was.
        let fits = removed.iter().enumerate().all(|(before, (at, _))| *at <= list.items.len() + before);
        if removed.is_empty() || !fits {
            return false;
        }
        for (at, item) in removed {
            list.items.insert(*at, item.clone());
        }
        self.changed = true;
        if playlist == self.current
            && let Some(open) = self.chosen.as_mut()
        {
            for (at, _) in removed {
                if *open >= *at {
                    *open += 1;
                }
            }
        }
        true
    }

    /// Moves an item one place earlier or later. Returns true if it moved.
    /// Moves an item to a place in the list: `slot` is how many items will be above it.
    pub fn move_item(&mut self, index: usize, slot: usize) -> bool {
        let count = self.front().items.len();
        if index >= count {
            return false;
        }
        let to = if slot > index { slot - 1 } else { slot }.min(count - 1);
        if to == index {
            return false;
        }
        let item = self.front_mut().items.remove(index);
        self.front_mut().items.insert(to, item);
        self.chosen = Some(to);
        true
    }

    /// Moves several items of the service in front together to a place in the list (`slot`,
    /// how many items are above it now), keeping the order they are in. The open item stays
    /// open, wherever it goes. Returns the place each item is at now, by its place before (so
    /// what was picked out can be found again), or `None` when nothing moved.
    pub fn move_items(&mut self, places: &[usize], slot: usize) -> Option<Vec<usize>> {
        let count = self.front().items.len();
        let mut moving: Vec<usize> = places.iter().copied().filter(|at| *at < count).collect();
        moving.sort_unstable();
        moving.dedup();
        if moving.is_empty() {
            return None;
        }
        let staying: Vec<usize> = (0..count).filter(|at| moving.binary_search(at).is_err()).collect();
        // Where they go among the items left: after those that were above the slot.
        let to = staying.iter().filter(|at| **at < slot).count();
        let order: Vec<usize> = staying[..to].iter().chain(&moving).chain(&staying[to..]).copied().collect();
        if order.iter().copied().eq(0..count) {
            return None;
        }
        let items = std::mem::take(&mut self.front_mut().items);
        self.front_mut().items = order.iter().map(|at| items[*at].clone()).collect();
        let mut now = vec![0; count];
        for (place, was) in order.iter().enumerate() {
            now[*was] = place;
        }
        self.chosen = self.chosen.and_then(|at| now.get(at).copied());
        Some(now)
    }

    /// Sets the theme version an item of the playlist in front shows in (none for the usual
    /// look). Returns false, changing nothing, for an item that can't have one.
    pub fn set_show_with(&mut self, index: usize, show_with: Option<ShowWith>) -> bool {
        let at = self.current.min(self.list.len() - 1);
        let Some(set) = self.list[at].items.get_mut(index).and_then(Item::show_with_mut) else { return false };
        if *set != show_with {
            *set = show_with;
            self.changed = true;
        }
        true
    }

    /// Renames a header or a placeholder in the service shown.
    pub fn rename_header(&mut self, index: usize, name: &str) {
        if let Some(Item::Header { name: have } | Item::Placeholder { name: have }) =
            self.front_mut().items.get_mut(index)
            && !name.trim().is_empty()
        {
            *have = name.trim().to_owned();
        }
    }

    /// Moves several items of the playlist in front one place earlier (or later) each, keeping
    /// their order: as Move Up or Move Down does for the items picked. One already at that end,
    /// or held there by another picked one, stays. The open item stays open where it goes.
    /// Returns where each of them is now, in order, or `None` when none could move.
    pub fn nudge_many(&mut self, places: &[usize], earlier: bool) -> Option<Vec<usize>> {
        let count = self.front().items.len();
        let mut now: Vec<usize> = places.iter().copied().filter(|at| *at < count).collect();
        now.sort_unstable();
        now.dedup();
        let mut moved = false;
        let order: Vec<usize> = if earlier { (0..now.len()).collect() } else { (0..now.len()).rev().collect() };
        for which in order {
            let at = now[which];
            let to = if earlier { at.checked_sub(1) } else { Some(at + 1).filter(|to| *to < count) };
            let Some(to) = to.filter(|to| !now.contains(to)) else { continue };
            self.front_mut().items.swap(at, to);
            self.chosen = self.chosen.map(|open| {
                if open == at {
                    to
                } else if open == to {
                    at
                } else {
                    open
                }
            });
            now[which] = to;
            moved = true;
        }
        moved.then_some(now)
    }

    pub fn nudge(&mut self, index: usize, earlier: bool) -> bool {
        let count = self.front().items.len();
        let to = if earlier { index.checked_sub(1) } else { Some(index + 1).filter(|to| *to < count) };
        let Some(to) = to.filter(|_| index < count) else { return false };
        self.front_mut().items.swap(index, to);
        self.chosen = Some(to);
        true
    }

    /// Brings another service to the front.
    pub fn show(&mut self, index: usize) {
        if index < self.list.len() && index != self.current {
            self.current = index;
            self.chosen = None;
            self.changed = true;
        }
    }

    /// Makes a new, empty service and brings it to the front.
    pub fn add_service(&mut self) {
        let name = (1..)
            .map(|n| tf("notices.new_playlist.name", "Playlist {n}", &[("n", &n.to_string())]))
            .find(|name| self.list.iter().all(|s| &s.name != name));
        self.list.push(Service {
            name: name.unwrap_or_default(),
            items: Vec::new(),
            uids: Default::default(),
            origin: None,
        });
        self.current = self.list.len() - 1;
        self.chosen = None;
        self.changed = true;
    }

    /// Renames a playlist, by its place among the playlists; the one in front stays in front.
    /// An empty name is refused.
    pub fn rename_at(&mut self, playlist: usize, name: &str) {
        let name = name.trim();
        if let Some(list) = self.list.get_mut(playlist)
            && !name.is_empty()
            && list.name != name
        {
            list.name = name.to_owned();
            self.changed = true;
        }
    }

    /// Removes a playlist, by its place among the playlists. The last one cannot be removed.
    /// The playlist in front stays in front with its open item, unless it is the one removed:
    /// then the one before it (or, for the first, the one after) comes to the front.
    pub fn remove_at(&mut self, playlist: usize) -> bool {
        if self.list.len() <= 1 || playlist >= self.list.len() {
            return false;
        }
        let front = self.front_at();
        self.list.remove(playlist);
        if playlist == front {
            self.current = front.saturating_sub(1).min(self.list.len() - 1);
            self.chosen = None;
        } else {
            self.current = if playlist < front { front - 1 } else { front };
        }
        self.changed = true;
        true
    }

    pub fn to_json(&self) -> String {
        for service in &self.list {
            service.uids();
        }
        let newest = self.list.iter().flat_map(|service| &service.items).any(Item::needs_version_3);
        let file =
            File { format: FORMAT.to_owned(), version: if newest { VERSION } else { 2 }, services: self.clone() };
        let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Reads the services file. The error is a plain sentence.
    pub fn from_json(text: &str) -> Result<Services, String> {
        let file: File =
            serde_json::from_str(text).map_err(|e| format!("The services file could not be read ({e})."))?;
        if file.format != FORMAT && file.format != OLD_FORMAT {
            return Err("That is not a SoftPresenter services file.".to_owned());
        }
        if file.version > VERSION {
            return Err("The services file was made by a newer SoftPresenter.".to_owned());
        }
        let mut services = file.services;
        if services.list.is_empty() {
            services.list = Services::default().list;
        }
        services.current = services.current.min(services.list.len() - 1);
        // A file from before items had ids (or with ids that don't fit) is saved again at once, so
        // the ids made now are the ones the next opening reads.
        for service in &services.list {
            let before = service.uids.book().uids.clone();
            if service.uids() != before {
                services.changed = true;
            }
        }
        Ok(services)
    }
}

/// The small choices that are not part of any song, theme or service, kept between runs.
/// What an earlier version's Settings › Screens › When SoftPresenter Opens said to play: nothing,
/// a media playlist (on the media layer, item after item), or a song on the Announcements layer
/// (by its key). Start-up is a macro now; this is read once, to make that macro.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "play", rename_all = "snake_case")]
pub enum StartWith {
    #[default]
    Nothing,
    MediaPlaylist {
        id: sp_model::Uuid,
    },
    Announcements {
        song: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Whether one slide fades into the next, and over how many seconds.
    pub dissolve: bool,
    pub dissolve_time: f32,
    /// How one slide gives way to the next. `None` in a file from before transitions could be
    /// chosen, which said only whether slides dissolved, and for how long.
    pub transition: Option<sp_model::Transition>,
    /// How one picture or video in the media bin gives way to the next.
    pub media_transition: Option<sp_model::Transition>,
    /// How many slides fit across the grid, and the view: `grid`, `easy` or `table`.
    pub columns: usize,
    pub slide_view: String,
    /// From before screens were listed: the name the screen was sent under over NDI, and
    /// whether it was on. Read once, to make that screen; no longer written.
    #[serde(skip_serializing)]
    pub ndi_name: String,
    #[serde(skip_serializing)]
    pub ndi_on: bool,
    /// From before the left column listed one thing at a time: how tall its upper part was
    /// dragged to be. Still read, so an older file opens; no longer used.
    pub service_height: Option<f32>,
    /// True when the stage screens were switched off.
    pub stage_off: bool,
    /// When a song's copyright line is shown on the audience screens.
    pub copyright_show: CopyrightShow,
    /// The church's CCLI licence number, for the copyright line.
    pub ccli_licence: String,
    /// From before start-up was a macro: true to start with the audience screens off. Read once,
    /// to put the step in the start-up macro (`macros::from_audience_off`); no longer written.
    #[serde(skip_serializing)]
    pub start_audience_off: bool,
    /// Next at the end of a song goes on to the next song in the service.
    pub continuous: bool,
    /// The end of a song is marked on the audience screens: a mark (three stars, unless its
    /// theme chose another) under the words of its last slide as sung (`end_mark`).
    pub end_mark: bool,
    /// From before start-up was a macro: what played by itself when the app opened. Read once,
    /// to make a macro that does it (`macros::from_opening_choice`); no longer written.
    #[serde(skip_serializing)]
    pub start_with: StartWith,
    /// True once the setup a fresh install starts with has been seen.
    #[serde(default)]
    pub setup_done: bool,
    /// True when the Bible page's book, chapter and verse lists were put away.
    #[serde(default)]
    pub bible_lists_hidden: bool,
    /// True when the Bible page's table of translations was left open.
    #[serde(default)]
    pub bible_rows_open: bool,
    /// How the Bible page shows a passage: `page` (its chapter to read, the default) or
    /// `slides`.
    #[serde(default)]
    pub bible_view: String,
    /// How the Bible view's left column offers the books: `keypad` (a key for every book, the
    /// default) or `lists`.
    #[serde(default)]
    pub bible_books: String,
    /// The language the Bible page is read in: `auto` (the default: the language typed, else
    /// the last one typed, else the computer's), or `en`, `ru` or `uk`.
    #[serde(default)]
    pub bible_language: String,
    /// True when the left column was left listing a playlist, not a library's songs.
    #[serde(default)]
    pub playlist_in_front: bool,
    /// The order a library's songs are listed in: `listed`, `az` or `longest`.
    #[serde(default)]
    pub song_order: String,
    /// True when each button's key is written beside it (Help › Show Keys on Buttons).
    #[serde(default)]
    pub keys_on_buttons: bool,
    /// What each key that runs the show does (Settings › Keys): the style, and the keys changed
    /// from it. A file from before the table: SoftPresenter's style, nothing changed.
    #[serde(default)]
    pub keys: crate::key_table::KeyTable,
    /// True when Settings › Screens adds a technical line to every screen's card (Details).
    #[serde(default)]
    pub screens_details: bool,
    /// The language of the app's own words: `auto` (the computer's, the default), or `en`, `es`,
    /// `ru` or `uk`.
    #[serde(default)]
    pub language: String,
    /// How each kind of presentation file was last brought in (`pptx`: `slides` or `pictures`).
    #[serde(default)]
    pub import_as: std::collections::BTreeMap<String, String>,
    /// How large the operator's windows draw everything: `auto` (larger in a big window, the
    /// default), or a percentage: `100`, `125`, `150` or `175`.
    #[serde(default)]
    pub interface_size: String,
}

/// When a song's copyright line is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyrightShow {
    #[default]
    Never,
    FirstSlide,
    LastSlide,
    EverySlide,
}

impl CopyrightShow {
    pub fn name(self) -> &'static str {
        match self {
            CopyrightShow::Never => "Never",
            CopyrightShow::FirstSlide => "On a song's first slide",
            CopyrightShow::LastSlide => "On a song's last slide",
            CopyrightShow::EverySlide => "On every slide of a song",
        }
    }

    /// True when the line is shown on the slide at `index` of a song with `count` slides.
    pub fn shows_on(self, index: usize, count: usize) -> bool {
        match self {
            CopyrightShow::Never => false,
            CopyrightShow::FirstSlide => index == 0,
            CopyrightShow::LastSlide => index + 1 == count,
            CopyrightShow::EverySlide => true,
        }
    }
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            dissolve: true,
            dissolve_time: 0.4,
            transition: None,
            media_transition: None,
            columns: 4,
            slide_view: "grid".to_owned(),
            ndi_name: "SoftPresenter Audience".to_owned(),
            ndi_on: false,
            service_height: None,
            stage_off: false,
            copyright_show: CopyrightShow::Never,
            ccli_licence: String::new(),
            start_audience_off: false,
            continuous: false,
            end_mark: false,
            start_with: StartWith::Nothing,
            setup_done: false,
            bible_lists_hidden: false,
            bible_rows_open: false,
            bible_view: String::new(),
            bible_books: String::new(),
            bible_language: String::new(),
            playlist_in_front: false,
            song_order: "listed".to_owned(),
            keys_on_buttons: false,
            keys: crate::key_table::KeyTable::default(),
            screens_details: false,
            language: String::new(),
            import_as: std::collections::BTreeMap::new(),
            interface_size: String::new(),
        }
    }
}

fn prefs_file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("preferences.json"))
}

/// The choices as last saved, or the usual ones; and a file there that couldn't be read, kept
/// aside (`whole_file`).
pub fn load_prefs() -> (Prefs, Option<crate::whole_file::Unread>) {
    prefs_file().map_or_else(|| (Prefs::default(), None), |path| load_prefs_from(&path))
}

fn load_prefs_from(path: &std::path::Path) -> (Prefs, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::Settings, |text| {
        serde_json::from_str(text).map_err(|e| e.to_string())
    })
}

/// The choices as last saved, or the usual ones, only looked at: for a start that doesn't open
/// the app (another copy has the data folder), which leaves a file it can't read to that copy.
pub fn peek_prefs() -> Prefs {
    prefs_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remembers the choices for the next start.
/// The choices' file and what goes in it, for the library's writer to write (`writer`), paced
/// (`writer::Paced`), so they are as they were at the next start. The error is a plain sentence.
pub fn prefs_to_save(prefs: &Prefs) -> Result<crate::writer::Save, String> {
    let path = prefs_file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(prefs_to_save_at(path, prefs))
}

fn prefs_to_save_at(path: std::path::PathBuf, prefs: &Prefs) -> crate::writer::Save {
    let text = serde_json::to_string_pretty(prefs).unwrap_or_default();
    crate::whole_file::settings_save(path, crate::whole_file::What::Settings, text.into_bytes())
}

fn bible_page_file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("bible-page.json"))
}

/// A Bible passage that was on screen, and when it went on (local time,
/// `2026-11-19T10:42:05`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub passage: String,
    pub at: String,
    /// The verse numbering `passage` is in, as a playlist's passage keeps it; none (from before)
    /// for the first translation's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numbering: Option<String>,
}

fn bible_history_file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("bible-history.json"))
}

/// The passages that were on screen, newest first, nothing if there is no file yet; and a file
/// there that couldn't be read, kept aside (`whole_file`).
pub fn load_bible_history() -> (Vec<HistoryEntry>, Option<crate::whole_file::Unread>) {
    bible_history_file().map_or((Vec::new(), None), |path| load_history_from(&path))
}

fn load_history_from(path: &std::path::Path) -> (Vec<HistoryEntry>, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::BibleHistory, |text| {
        serde_json::from_str(text).map_err(|e| e.to_string())
    })
}

/// The passages that were on screen, for the library's writer to keep for the next start
/// (`writer`), paced (`writer::Paced`). The error is a plain sentence.
pub fn bible_history_to_save(history: &[HistoryEntry]) -> Result<crate::writer::Save, String> {
    let path = bible_history_file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(history_to_save_at(path, history))
}

fn history_to_save_at(path: std::path::PathBuf, history: &[HistoryEntry]) -> crate::writer::Save {
    let text = serde_json::to_string_pretty(history).unwrap_or_default();
    crate::whole_file::settings_save(path, crate::whole_file::What::BibleHistory, text.into_bytes())
}

/// How the Bible page was last set up (its translations, boxes and slide options), if it was;
/// and a file there that couldn't be read, kept aside (`whole_file`).
pub fn load_bible_page() -> (Option<SavedPassage>, Option<crate::whole_file::Unread>) {
    bible_page_file().map_or((None, None), |path| load_bible_page_from(&path))
}

fn load_bible_page_from(path: &std::path::Path) -> (Option<SavedPassage>, Option<crate::whole_file::Unread>) {
    match crate::whole_file::read(path, crate::whole_file::What::BiblePage, |text| {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }) {
        Ok(page) => (page, None),
        Err(unread) => (None, Some(unread)),
    }
}

/// How the Bible page is set up, for the library's writer to keep for the next start (`writer`),
/// paced (`writer::Paced`). The error is a plain sentence.
pub fn bible_page_to_save(page: &SavedPassage) -> Result<crate::writer::Save, String> {
    let path = bible_page_file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(bible_page_to_save_at(path, page))
}

fn bible_page_to_save_at(path: std::path::PathBuf, page: &SavedPassage) -> crate::writer::Save {
    let text = serde_json::to_string_pretty(page).unwrap_or_default();
    crate::whole_file::settings_save(path, crate::whole_file::What::BiblePage, text.into_bytes())
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("services.json"))
}

/// The services as last saved, or one empty service. A file that can't be read (or comes from
/// a newer build) is kept aside (`whole_file`), so the empty service used instead is never saved
/// over it, and comes back to tell the operator.
pub fn load() -> (Services, Option<crate::whole_file::Unread>) {
    file().map_or_else(|| (Services::default(), None), |path| load_from(&path))
}

fn load_from(path: &std::path::Path) -> (Services, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::Playlists, Services::from_json)
}

/// What the window says when the playlists couldn't be written: a plain sentence with the
/// system's reason.
pub fn not_saved(error: &std::io::Error) -> String {
    crate::whole_file::What::Playlists.not_saved(error)
}

/// The services' file and what goes in it, for the library's writer to write (`writer`). The
/// error is a plain sentence.
pub fn to_save(services: &Services) -> Result<crate::writer::Save, String> {
    let path = file().ok_or_else(|| {
        t("notices.save.no_place_services", "Could not find where to keep playlists on this computer.").to_owned()
    })?;
    Ok(to_save_at(path, services))
}

fn to_save_at(path: std::path::PathBuf, services: &Services) -> crate::writer::Save {
    let text = services.to_json();
    let mut save =
        crate::writer::Save::new(path, crate::writer::Kind::Playlists, crate::writer::Guard::Any, move || {
            text.into_bytes()
        });
    save.make_folder = true;
    save
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let mut prefs = Prefs { ccli_licence: "12345".to_owned(), ..Prefs::default() };
        prefs.columns = 6;
        let good = serde_json::to_string_pretty(&prefs).unwrap();
        crate::whole_file::tests::check_loader(
            "preferences.json",
            crate::whole_file::What::Settings,
            good.as_bytes(),
            |path| {
                let (read, unread) = load_prefs_from(path);
                if unread.is_some() {
                    assert_eq!(read, Prefs::default(), "the usual settings instead");
                } else {
                    assert!(read == prefs || read == Prefs::default(), "the file's settings");
                }
                unread
            },
            |path| crate::whole_file::tests::written(prefs_to_save_at(path.to_owned(), &Prefs::default())),
        );
    }

    #[test]
    fn a_bible_page_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let page = SavedPassage {
            lookup: String::new(),
            numbering: None,
            rows: Vec::new(),
            show_verse_numbers: true,
            break_on_new_verse: false,
            display_translation: true,
            reference_style: "passage_last".to_owned(),
            show_with: None,
        };
        let good = serde_json::to_string_pretty(&page).unwrap();
        crate::whole_file::tests::check_loader(
            "bible-page.json",
            crate::whole_file::What::BiblePage,
            good.as_bytes(),
            |path| {
                let (read, unread) = load_bible_page_from(path);
                assert_eq!(read.is_some(), unread.is_none(), "the page as it was, or as usual");
                unread
            },
            |path| crate::whole_file::tests::written(bible_page_to_save_at(path.to_owned(), &page)),
        );
    }

    #[test]
    fn a_bible_history_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let history = vec![HistoryEntry {
            passage: "John 3:16".to_owned(),
            at: "2026-11-19T10:42:05".to_owned(),
            numbering: None,
        }];
        let good = serde_json::to_string_pretty(&history).unwrap();
        crate::whole_file::tests::check_loader(
            "bible-history.json",
            crate::whole_file::What::BibleHistory,
            good.as_bytes(),
            |path| load_history_from(path).1,
            |path| crate::whole_file::tests::written(history_to_save_at(path.to_owned(), &[])),
        );
    }

    #[test]
    fn a_playlists_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let mut services = Services::default();
        services.add(Item::song("a", "Alpha"));
        let good = services.to_json();
        crate::whole_file::tests::check_loader(
            "services.json",
            crate::whole_file::What::Playlists,
            good.as_bytes(),
            |path| load_from(path).1,
            |path| crate::whole_file::tests::written(to_save_at(path.to_owned(), &Services::default())),
        );
    }

    #[test]
    fn the_first_playlist_of_a_first_start_follows_the_language_until_it_is_changed() {
        use crate::i18n::{Language, with_language};
        let mut services = with_language(Language::English, Services::default);
        assert_eq!((services.front().name.as_str(), services.fresh), ("Service", true));
        // The computer's language answers, or the setup chooses Spanish: it follows.
        assert!(with_language(Language::Spanish, || services.follow_language()));
        assert_eq!(services.front().name, "Culto");
        assert!(!with_language(Language::Spanish, || services.follow_language()), "once");
        // Renamed by the church: changed, written, and from then on theirs in any language.
        services.rename_at(0, "Sunday");
        assert!(!with_language(Language::English, || services.follow_language()), "a change waits to be written");
        services.changed = false;
        services.fresh = false;
        assert!(!with_language(Language::English, || services.follow_language()));
        assert_eq!(services.front().name, "Sunday");
        // A song added is a change too.
        let mut services = with_language(Language::English, Services::default);
        services.add(Item::song("a", "Alpha"));
        assert!(!with_language(Language::Spanish, || services.follow_language()));
        assert_eq!(services.front().name, "Service");
        // Playlists read from their file are never renamed, whatever they are called.
        let read = Services::from_json(&with_language(Language::English, Services::default).to_json()).unwrap();
        let mut read = read;
        assert!(!read.fresh);
        assert!(!with_language(Language::Spanish, || read.follow_language()));
        assert_eq!(read.front().name, "Service");
    }

    #[test]
    fn a_playlist_with_media_or_a_placeholder_is_written_as_version_3_and_others_as_2() {
        let mut services = Services::default();
        services.add(song("one"));
        let text = services.to_json();
        assert!(text.contains("\"version\": 2"), "only songs: an older SoftPresenter can open it\n{text}");
        let id = Uuid::new_v4();
        services.add(Item::Media { id, name: "Countdown.mp4".to_owned() });
        services.add(Item::MediaPlaylist { id, name: "Announcements".to_owned() });
        services.add(Item::Placeholder { name: "Sermon slides".to_owned() });
        let text = services.to_json();
        assert!(text.contains("\"version\": 3"), "{text}");
        assert!(text.contains("\"kind\": \"media_playlist\"") && text.contains("\"kind\": \"placeholder\""), "{text}");
        let again = Services::from_json(&text).unwrap();
        assert_eq!(again.front().items, services.front().items, "every kind comes back as it was");
    }

    #[test]
    fn a_version_2_services_file_still_opens() {
        let text = r#"{"format": "softpresenter-services", "version": 2, "list": [{"name": "Sunday",
            "items": [{"kind": "song", "key": "abc", "title": "Abc"}, {"kind": "header", "name": "Word"}]}],
            "current": 0}"#;
        let services = Services::from_json(text).unwrap();
        assert_eq!(services.front().items, vec![song_titled("abc", "Abc"), Item::Header { name: "Word".to_owned() }]);
    }

    fn song_titled(key: &str, title: &str) -> Item {
        Item::song(key.to_owned(), title.to_owned())
    }

    #[test]
    fn a_song_dropped_on_a_placeholder_takes_its_place() {
        let mut services = Services::default();
        services.add(song("one"));
        services.add(Item::Placeholder { name: "Sermon slides".to_owned() });
        services.add(song("two"));
        assert!(!services.fill(0, song("three")), "only a placeholder is filled");
        assert!(services.fill(1, song("three")));
        assert_eq!(services.front().items, vec![song("one"), song("three"), song("two")]);
        assert_eq!(services.chosen, Some(1));
        // A song already in the playlist isn't moved into the placeholder's place: nothing
        // changes, and the caller says it is there already.
        services.insert(0, Item::Placeholder { name: "Opening".to_owned() });
        let before = services.front().items.clone();
        assert!(!services.fill(0, song("two")));
        assert_eq!(services.front().items, before);
        assert_eq!(services.place_of(0, &song("two")), Some(3));
    }

    #[test]
    fn an_item_dropped_between_two_goes_there_but_a_song_already_there_is_only_chosen() {
        let mut services = Services::default();
        services.add(song("one"));
        services.add(song("two"));
        let id = Uuid::new_v4();
        services.insert(1, Item::Media { id, name: "Walk-in.mp4".to_owned() });
        assert_eq!(services.front().items[1], Item::Media { id, name: "Walk-in.mp4".to_owned() });
        services.insert(0, song("two"));
        assert_eq!(services.front().items.len(), 3, "not added twice");
        assert_eq!(services.chosen, Some(2));
    }

    #[test]
    fn the_bible_history_comes_back_as_it_was_saved() {
        let path =
            std::env::temp_dir().join(format!("softpresenter-history-{}/bible-history.json", std::process::id()));
        assert!(load_history_from(&path).0.is_empty(), "no file yet is no history");
        let history = vec![
            HistoryEntry { passage: "John 3:16".to_owned(), at: "2026-11-19T10:42:05".to_owned(), numbering: None },
            HistoryEntry {
                passage: "Psalms 23:1-6".to_owned(),
                at: "2026-11-17T09:30:00".to_owned(),
                numbering: Some("english".to_owned()),
            },
        ];
        crate::whole_file::tests::written(history_to_save_at(path.clone(), &history));
        assert_eq!(load_history_from(&path).0, history);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_song_dropped_on_another_playlist_goes_there_once_and_the_front_stays() {
        let mut playlists = Services::default();
        playlists.add_service();
        playlists.show(0);
        let song = Item::song("songs/joy".to_owned(), "Joy".to_owned());
        assert!(playlists.add_to(1, song.clone()));
        assert!(!playlists.add_to(1, song), "a song already there is not added again");
        assert!(!playlists.add_to(9, Item::Header { name: "x".to_owned() }));
        assert_eq!((playlists.current, playlists.list[1].items.len(), playlists.list[0].items.len()), (0, 1, 0));
        assert_eq!(playlists.list[1].name, "Playlist 1");
    }

    #[test]
    fn a_passage_goes_in_a_playlist_once_in_the_same_translations() {
        let in_bibles = |lookup: &str, bibles: &[&str]| {
            let Item::Passage(mut saved) = passage(lookup) else { unreachable!() };
            saved.rows = bibles
                .iter()
                .map(|bible| SavedRow { bible: (*bible).to_owned(), verse: None, reference: None })
                .collect();
            Item::Passage(saved)
        };
        let mut services = Services::default();
        services.add_service();
        services.show(0);
        let john = in_bibles("John 3:16", &["kjv", "rst"]);
        assert!(services.add(john.clone()));
        assert!(!services.add(john.clone()), "the same passage, the same translations: once");
        assert_eq!((services.front().items.len(), services.chosen), (1, Some(0)), "the one there is opened");
        // Set out otherwise (verse numbers, boxes, a version), it is still the same passage.
        let Item::Passage(mut set_out) = john.clone() else { unreachable!() };
        set_out.show_verse_numbers = true;
        set_out.rows[0].verse = Some("primary".to_owned());
        set_out.lookup = " john 3:16".to_owned();
        assert!(!services.add_to(0, Item::Passage(set_out)));
        services.insert(0, john.clone());
        assert_eq!(services.front().items.len(), 1, "nor put in at a place");
        // The same verses in other translations, or in another order, are another item.
        assert!(services.add(in_bibles("John 3:16", &["kjv"])));
        assert!(services.add(in_bibles("John 3:16", &["rst", "kjv"])));
        assert!(services.add(in_bibles("John 3:17", &["kjv", "rst"])));
        assert_eq!(services.front().items.len(), 4);
        // Another playlist takes it once too, and the front stays.
        assert!(services.add_to(1, john.clone()));
        assert!(!services.add_to(1, john.clone()));
        assert_eq!((services.current, services.list[1].items.len()), (0, 1));
        assert_eq!(services.place_of(1, &john), Some(0));
    }

    fn song(key: &str) -> Item {
        Item::song(key.to_owned(), key.to_uppercase())
    }

    fn passage(lookup: &str) -> Item {
        Item::Passage(SavedPassage {
            lookup: lookup.to_owned(),
            numbering: None,
            rows: vec![SavedRow { bible: "kjv".to_owned(), verse: Some("primary".to_owned()), reference: None }],
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference_style: "verse".to_owned(),
            show_with: None,
        })
    }

    #[test]
    fn items_are_added_in_order_and_a_song_or_passage_is_not_added_twice() {
        let mut services = Services::default();
        services.add(song("a"));
        services.add(passage("John 3:16"));
        services.add(song("a"));
        assert_eq!(services.front().items.len(), 2);
        assert_eq!(services.chosen, Some(0));
        // The same passage in the same translations is opened where it is, too.
        assert!(!services.add(passage("John 3:16")));
        assert_eq!((services.front().items.len(), services.chosen), (2, Some(1)));
        assert!(services.changed);
    }

    #[test]
    fn the_version_an_item_shows_in_survives_the_file_and_older_files_read_as_before() {
        let mut services = Services::default();
        services.add(song("a"));
        services.add(passage("John 3:16"));
        services.add(song("b"));
        let offering = ShowWith { version: Uuid::from_u128(9), keep: true };
        *services.list[0].items[0].show_with_mut().unwrap() = Some(offering);
        *services.list[0].items[1].show_with_mut().unwrap() = Some(ShowWith { keep: false, ..offering });
        let text = services.to_json();
        assert!(text.contains("\"show_with\""));
        let again = Services::from_json(&text).unwrap();
        assert_eq!(again.list, services.list);
        assert_eq!(again.front().items[1].show_with(), Some(ShowWith { keep: false, ..offering }));
        // An item in the usual look writes nothing new, and other kinds have no such choice.
        assert!(!serde_json::to_string(&song("c")).unwrap().contains("show_with"));
        assert!(Item::Header { name: "Word".to_owned() }.show_with_mut().is_none());
        // A file from before reads with every item in the usual look.
        let old = r#"{"format":"softpresenter-services","version":2,"current":0,
            "list":[{"name":"Sunday","items":[{"kind":"song","key":"a","title":"A"}]}]}"#;
        let read = Services::from_json(old).unwrap();
        assert_eq!(read.front().items[0], Item::song("a", "A"));
        assert_eq!(read.front().items[0].show_with(), None);
        assert!(read.to_json().contains("\"version\": 2"));
    }

    #[test]
    fn items_move_and_leave() {
        let mut services = Services::default();
        for key in ["a", "b", "c"] {
            services.add(song(key));
        }
        assert!(services.nudge(2, true));
        assert_eq!(services.front().items.iter().map(Item::title).collect::<Vec<_>>(), ["A", "C", "B"]);
        assert!(!services.nudge(0, true));
        assert!(!services.nudge(2, false));
        assert_eq!(services.remove_items(0, &[1]).len(), 1);
        assert!(services.remove_items(0, &[9]).is_empty());
        assert_eq!(services.front().items.len(), 2);
        // Headers are kept, renamed, and items are dragged to a new place.
        services.add(Item::Header { name: "Worship".to_owned() });
        services.rename_header(2, "Sermon");
        assert!(services.move_item(2, 0));
        assert_eq!(services.front().items.iter().map(Item::title).collect::<Vec<_>>(), ["Sermon", "A", "B"]);
        assert!(!services.move_item(0, 0));
        let again = Services::from_json(&services.to_json()).unwrap();
        assert_eq!(again.front().items[0], Item::Header { name: "Sermon".to_owned() });
    }

    #[test]
    fn items_taken_out_go_back_where_they_were() {
        let mut services = Services::default();
        for key in ["a", "b", "c", "d", "e"] {
            services.add(song(key));
        }
        let titles = |services: &Services| -> Vec<String> {
            services.front().items.iter().map(|item| item.title().to_owned()).collect()
        };
        services.chosen = Some(4);
        let removed = services.remove_items(0, &[3, 1, 9]);
        assert_eq!(removed.iter().map(|(at, item)| (*at, item.title())).collect::<Vec<_>>(), [(1, "B"), (3, "D")]);
        assert_eq!(titles(&services), ["A", "C", "E"]);
        assert_eq!(services.chosen, Some(2), "E, still open where it now is");
        assert!(services.put_back(0, &removed));
        assert_eq!(titles(&services), ["A", "B", "C", "D", "E"]);
        assert_eq!(services.chosen, Some(4));
        // The open item taken out is no longer open; a playlist too short, or gone, takes nothing.
        services.chosen = Some(1);
        let removed = services.remove_items(0, &[1, 4]);
        assert_eq!(services.chosen, None);
        services.remove_items(0, &[0, 1, 2]);
        assert!(!services.put_back(0, &removed), "nowhere to put E back");
        assert!(!services.put_back(5, &removed));
    }

    #[test]
    fn several_items_move_together_in_their_order() {
        let mut services = Services::default();
        for key in ["a", "b", "c", "d", "e"] {
            services.add(song(key));
        }
        let titles = |services: &Services| -> Vec<String> {
            services.front().items.iter().map(|item| item.title().to_owned()).collect()
        };
        services.chosen = Some(3);
        // B and D (picked in either order) to the top: in their order, D still open.
        let now = services.move_items(&[3, 1], 0).expect("moved");
        assert_eq!(titles(&services), ["B", "D", "A", "C", "E"]);
        assert_eq!(now, [2, 0, 3, 1, 4]);
        assert_eq!(services.chosen, Some(1));
        // A and E to between C and the end... after C: the slot counts the items above it.
        services.move_items(&[2, 4], 4).expect("moved");
        assert_eq!(titles(&services), ["B", "D", "C", "A", "E"]);
        // Down past themselves: B and D to the end.
        services.move_items(&[0, 1], 5).expect("moved");
        assert_eq!(titles(&services), ["C", "A", "E", "B", "D"]);
        // Dropped where they are, or nothing to move: nothing changes.
        services.changed = false;
        assert_eq!(services.move_items(&[3, 4], 4), None);
        assert_eq!(services.move_items(&[3, 4], 5), None);
        assert_eq!(services.move_items(&[9], 0), None);
        assert!(!services.changed);
    }

    #[test]
    fn move_up_or_down_with_several_picked_moves_them_all_one_place() {
        let mut services = Services::default();
        for key in ["a", "b", "c", "d", "e"] {
            services.add(song(key));
        }
        let titles = |services: &Services| -> Vec<String> {
            services.front().items.iter().map(|item| item.title().to_owned()).collect()
        };
        services.chosen = Some(3);
        // B and D up one place each, D still open.
        assert_eq!(services.nudge_many(&[3, 1], true), Some(vec![0, 2]));
        assert_eq!(titles(&services), ["B", "A", "D", "C", "E"]);
        assert_eq!(services.chosen, Some(2));
        // At the top, B stays, and D goes up to it, no further.
        assert_eq!(services.nudge_many(&[0, 2], true), Some(vec![0, 1]));
        assert_eq!(titles(&services), ["B", "D", "A", "C", "E"]);
        // Both at the top: nothing moves.
        assert_eq!(services.nudge_many(&[0, 1], true), None);
        // Down, together, keeping their order.
        assert_eq!(services.nudge_many(&[0, 1], false), Some(vec![1, 2]));
        assert_eq!(titles(&services), ["A", "B", "D", "C", "E"]);
    }

    #[test]
    fn items_put_in_go_after_the_open_one_in_order_and_once() {
        let mut services = Services::default();
        for key in ["a", "b", "c"] {
            services.add(song(key));
        }
        let titles = |services: &Services, at: usize| -> Vec<String> {
            services.list[at].items.iter().map(|item| item.title().to_owned()).collect()
        };
        let went = services.put_in(0, Some(0), vec![song("x"), song("c"), song("y")]);
        assert_eq!(went, [true, false, true], "C is in it already");
        assert_eq!(titles(&services, 0), ["A", "X", "Y", "B", "C"]);
        assert_eq!(services.chosen, Some(2), "the last put in is the open one");
        // Another playlist: at its end, the open item of the one in front left alone.
        services.list.push(Service {
            name: "Evening".to_owned(),
            items: vec![song("a")],
            uids: Default::default(),
            origin: None,
        });
        assert_eq!(services.put_in(1, Some(0), vec![song("b")]), [true]);
        assert_eq!((titles(&services, 1), services.chosen), (vec!["A".to_owned(), "B".to_owned()], Some(2)));
    }

    #[test]
    fn services_are_made_shown_renamed_and_removed_but_never_all() {
        let mut services = Services::default();
        services.add(song("a"));
        services.add_service();
        assert_eq!((services.list.len(), services.current), (2, 1));
        assert!(services.front().items.is_empty());
        services.rename_at(1, "  Sunday ");
        services.rename_at(1, "   ");
        assert_eq!(services.front().name, "Sunday");
        services.show(0);
        assert_eq!(services.front().items.len(), 1);
        assert!(services.remove_at(0));
        assert_eq!(services.front().name, "Sunday");
        assert!(!services.remove_at(0));
    }

    #[test]
    fn a_playlist_renamed_or_removed_by_its_place_leaves_the_one_in_front_alone() {
        let named = |names: &[&str]| Services {
            list: names
                .iter()
                .map(|name| Service {
                    name: (*name).to_owned(),
                    items: vec![song("a"), song("b")],
                    uids: Default::default(),
                    origin: None,
                })
                .collect(),
            current: 2,
            chosen: Some(1),
            changed: false,
            fresh: false,
        };
        let names = |services: &Services| services.list.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        // Renaming another playlist renames it there; the one in front and its open item stay.
        let mut services = named(&["A", "B", "C", "D"]);
        services.rename_at(0, "Morning");
        assert_eq!(names(&services), ["Morning", "B", "C", "D"]);
        assert_eq!((services.front().name.as_str(), services.chosen), ("C", Some(1)));
        assert!(services.changed);
        // Removing one before it, or after it: the same playlist stays in front, still open where it was.
        let mut services = named(&["A", "B", "C", "D"]);
        assert!(services.remove_at(0));
        assert_eq!((services.front().name.as_str(), services.current, services.chosen), ("C", 1, Some(1)));
        assert!(services.remove_at(2));
        assert_eq!((services.front().name.as_str(), services.current, services.chosen), ("C", 1, Some(1)));
        // Removing the one in front: the one before it comes to the front, with nothing open.
        assert!(services.remove_at(1));
        assert_eq!((services.front().name.as_str(), services.chosen), ("B", None));
        // The first in front: the next one comes.
        let mut services = named(&["A", "B"]);
        services.current = 0;
        assert!(services.remove_at(0));
        assert_eq!(services.front().name, "B");
        assert!(!services.remove_at(0), "there is always one");
        assert!(!services.remove_at(5));
    }

    #[test]
    fn an_item_moved_takes_the_others_with_it() {
        // A B C D E, with B moved to the fourth place: A C D B E.
        let after: Vec<usize> = (0..5).map(|at| moved_place(at, 1, 3)).collect();
        assert_eq!(after, [0, 3, 1, 2, 4]);
        // And D back to the second: A D B C E.
        let after: Vec<usize> = (0..5).map(|at| moved_place(at, 3, 1)).collect();
        assert_eq!(after, [0, 2, 3, 1, 4]);
        assert_eq!(moved_place(2, 2, 2), 2);
    }

    #[test]
    fn a_placeholder_filled_in_another_playlist_leaves_the_open_item_alone() {
        let mut services = Services::default();
        services.list[0].items = vec![song("a"), song("b"), song("c")];
        services.list.push(Service {
            name: "Evening".to_owned(),
            items: vec![song("x"), Item::Placeholder { name: "Sermon".to_owned() }],
            uids: Default::default(),
            origin: None,
        });
        services.chosen = Some(2);
        assert_eq!(services.fill_in(1, 1, song("y")), Some(1));
        assert_eq!(services.list[1].items[1], song("y"));
        assert_eq!((services.current, services.chosen), (0, Some(2)), "the open item stays");
        assert_eq!(services.fill_in(1, 0, song("z")), None, "only a placeholder is filled");
        // In the playlist in front, a song it has above the placeholder stays where it is.
        services.list[0].items.insert(3, Item::Placeholder { name: "Here".to_owned() });
        assert_eq!(services.fill_in(0, 3, song("a")), None);
        assert_eq!(services.list[0].items[..3], [song("a"), song("b"), song("c")]);
        assert_eq!(services.chosen, Some(2), "c is still open");
    }

    #[test]
    fn an_item_put_in_without_opening_it_leaves_the_open_item_open_where_it_now_is() {
        let mut services = Services::default();
        services.list[0].items = vec![song("a"), song("b"), song("c")];
        services.chosen = Some(1);
        // Above the open item: it moves down one, and is still the open one.
        assert_eq!(services.put_at(1, song("x")), Some(1));
        assert_eq!(services.list[0].items, [song("a"), song("x"), song("b"), song("c")]);
        assert_eq!(services.chosen, Some(2), "b is still open");
        // Below it, or past the end: it stays.
        assert_eq!(services.put_at(9, song("y")), Some(4));
        assert_eq!(services.chosen, Some(2));
        // A song it has already isn't added again, and nothing is opened.
        assert_eq!(services.put_at(0, song("c")), None);
        assert_eq!((services.list[0].items.len(), services.chosen), (5, Some(2)));
        // With nothing open, nothing is opened.
        services.chosen = None;
        assert_eq!(services.put_at(0, song("z")), Some(0));
        assert_eq!(services.chosen, None);
    }

    #[test]
    fn preferences_read_back_and_a_missing_one_takes_its_usual_value() {
        let prefs = Prefs { dissolve: false, columns: 6, service_height: Some(300.0), ..Prefs::default() };
        let again: Prefs = serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(again, prefs);
        // A file from an older version, with fewer choices in it, still reads.
        let old: Prefs = serde_json::from_str("{\"columns\": 3}").unwrap();
        assert_eq!((old.columns, old.dissolve, old.dissolve_time), (3, true, 0.4));
    }

    #[test]
    fn the_key_table_reads_back_and_an_old_file_is_the_softpresenter_style() {
        use crate::key_table::{KeyAction, KeyStyle, KeyTable, ShowKey};
        let old: Prefs = serde_json::from_str("{\"columns\": 3}").unwrap();
        assert_eq!(old.keys, KeyTable::default());
        assert_eq!(old.keys.style, KeyStyle::SoftPresenter);
        let saved: Prefs =
            serde_json::from_str(r#"{"keys": {"style": "softprojector", "changed": {"esc": "clear_slide"}}}"#).unwrap();
        assert_eq!(saved.keys.style, KeyStyle::SoftProjector);
        assert_eq!(saved.keys.action(ShowKey::Escape), KeyAction::ClearSlide);
        assert_eq!(saved.keys.action(ShowKey::F2), KeyAction::AddToPlaylist, "the rest are the style's");
        // Written and read again, it is the same table.
        let again: Prefs = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(again.keys, saved.keys);
        assert_eq!(again.keys.action(ShowKey::Escape), KeyAction::ClearSlide);
    }

    #[test]
    fn an_unknown_key_or_action_in_the_table_keeps_every_other_preference() {
        use crate::key_table::{KeyAction, KeyStyle, ShowKey};
        let prefs: Prefs =
            serde_json::from_str(r#"{"columns": 3, "keys": {"changed": {"f99": "clear_all", "esc": "fly_away"}}}"#)
                .unwrap();
        assert_eq!(prefs.columns, 3);
        assert_eq!(prefs.keys.action(ShowKey::Escape), KeyAction::ClearAll);
        assert!(!prefs.keys.is_changed(ShowKey::Escape) && !prefs.keys.any_changed(), "nothing to reset");
        // A newer copy's style, or a row saved in another shape, is passed over too, and the rows
        // that can be read are kept.
        let prefs: Prefs = serde_json::from_str(
            r#"{"columns": 3, "keys": {"style": "something_newer", "changed": {"esc": 7, "f2": "nothing"}}}"#,
        )
        .unwrap();
        assert_eq!(prefs.columns, 3);
        assert_eq!(prefs.keys.style, KeyStyle::SoftPresenter);
        assert_eq!(prefs.keys.action(ShowKey::Escape), KeyAction::ClearAll);
        assert_eq!(prefs.keys.action(ShowKey::F2), KeyAction::Nothing);
        assert!(prefs.keys.is_changed(ShowKey::F2) && !prefs.keys.is_changed(ShowKey::Escape));
        // A row saved as its style's default is no change.
        let prefs: Prefs = serde_json::from_str(r#"{"keys": {"changed": {"esc": "clear_all"}}}"#).unwrap();
        assert!(!prefs.keys.any_changed());
        // And a table that is not a table at all.
        let prefs: Prefs = serde_json::from_str(r#"{"columns": 3, "keys": 5}"#).unwrap();
        assert_eq!((prefs.columns, prefs.keys.style), (3, KeyStyle::SoftPresenter));
    }

    #[test]
    fn services_survive_their_file_and_other_files_are_refused() {
        let mut services = Services::default();
        services.add(song("a"));
        services.add(passage("Пс 22:1-3"));
        services.add_service();
        let mut again = Services::from_json(&services.to_json()).unwrap();
        // What is chosen, whether anything changed and whether they are a first start's are not
        // kept.
        again.changed = services.changed;
        again.chosen = services.chosen;
        again.fresh = services.fresh;
        assert_eq!(again, services);
        assert!(Services::from_json("{}").is_err());
        assert!(
            Services::from_json("{\"format\":\"something else\",\"version\":1,\"list\":[],\"current\":0}").is_err()
        );
    }

    #[test]
    fn an_item_keeps_its_uid_when_the_items_are_moved_renamed_or_saved() {
        let mut services = Services::default();
        services.list[0].items = vec![song("a"), Item::Header { name: "Worship".to_owned() }, song("b"), song("c")];
        let first = services.list[0].uids();
        assert_eq!(first.len(), 4);
        assert_eq!(first.iter().collect::<std::collections::HashSet<_>>().len(), 4, "all different");
        assert_eq!(services.list[0].uids(), first, "asking again changes nothing");
        // Moved: each keeps its own.
        services.list[0].items.swap(0, 3);
        assert_eq!(services.list[0].uids(), vec![first[3], first[1], first[2], first[0]]);
        // One taken out, one put in: the new one has a new uid, the others their own.
        services.list[0].items.remove(1);
        services.list[0].items.insert(0, song("d"));
        let now = services.list[0].uids();
        assert_eq!(&now[1..], &[first[3], first[2], first[0]]);
        assert!(!first.contains(&now[0]));
        // Renamed in place: the same item.
        services.list[0].items[0] = song("d2");
        assert_eq!(services.list[0].uids(), now);
        // Several changes at once before anyone asks: still told apart by what they are.
        let before = services.list[0].uids();
        services.list[0].items.reverse();
        services.list[0].items.push(song("e"));
        let after = services.list[0].uids();
        assert_eq!(after[..4], [before[3], before[2], before[1], before[0]]);
        assert!(!before.contains(&after[4]));
        // Saved and read again, and a clone: the same uids.
        let again = Services::from_json(&services.to_json()).unwrap();
        assert_eq!(again.list[0].uids(), after);
        assert_eq!(services.list[0].clone().uids(), after);
        // Read again and moved: still each its own.
        let mut again = again;
        again.list[0].items.rotate_left(1);
        assert_eq!(again.list[0].uids(), [&after[1..], &after[..1]].concat());
    }

    #[test]
    fn a_file_from_before_uids_gets_them_without_losing_anything() {
        let text = r#"{"format": "softpresenter-services", "version": 2, "list": [{"name": "Sunday",
            "items": [{"kind": "song", "key": "abc", "title": "Abc"}, {"kind": "header", "name": "Word"}]}],
            "current": 0}"#;
        let services = Services::from_json(text).unwrap();
        assert!(services.changed, "saved again at once, so the ids made now are the ones read next time");
        let uids = services.front().uids();
        assert_eq!(uids.len(), 2);
        assert_eq!(services.front().items, vec![song_titled("abc", "Abc"), Item::Header { name: "Word".to_owned() }]);
        // Saving writes them, and the next reading keeps them.
        let saved = services.to_json();
        assert!(saved.contains(&uids[0].to_string()) && saved.contains(&uids[1].to_string()));
        let read = Services::from_json(&saved).unwrap();
        assert!(!read.changed, "nothing to save again");
        assert_eq!(read.front().uids(), uids);
    }

    #[test]
    fn uids_that_do_not_fit_the_items_are_made_again() {
        let text = |uids: &str| {
            format!(
                r#"{{"format": "softpresenter-services", "version": 2, "list": [{{"name": "Sunday",
                "items": [{{"kind": "header", "name": "A"}}, {{"kind": "header", "name": "B"}}], "uids": {uids}}}],
                "current": 0}}"#
            )
        };
        let one = "00000000-0000-4000-8000-000000000001";
        let two = "00000000-0000-4000-8000-000000000002";
        // Right: kept.
        let kept = Services::from_json(&text(&format!("[\"{one}\", \"{two}\"]"))).unwrap().front().uids();
        assert_eq!((kept[0].to_string(), kept[1].to_string()), (one.to_owned(), two.to_owned()));
        // The same one twice, or too few: new ones, one each, all different.
        for bad in [format!("[\"{one}\", \"{one}\"]"), format!("[\"{one}\"]"), "[]".to_owned()] {
            let made = Services::from_json(&text(&bad)).unwrap().front().uids();
            assert_eq!(made.len(), 2);
            assert_ne!(made[0], made[1]);
        }
    }
}
