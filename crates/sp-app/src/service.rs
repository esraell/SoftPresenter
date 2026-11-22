//! Services: the running order for a gathering. A service lists songs, saved Bible passages,
//! pictures and videos, media playlists and placeholders in the order they will be used, so
//! everything is one click away during it.

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
}

fn yes() -> bool {
    true
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

    /// A kind of item a services file before version 3 can't hold.
    fn needs_version_3(&self) -> bool {
        matches!(self, Item::Media { .. } | Item::MediaPlaylist { .. } | Item::Placeholder { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    pub items: Vec<Item>,
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
}

impl Default for Services {
    fn default() -> Services {
        Services {
            list: vec![Service { name: "Service".to_owned(), items: Vec::new() }],
            current: 0,
            chosen: None,
            changed: false,
        }
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
    /// The service in front. There is always one.
    pub fn front(&self) -> &Service {
        &self.list[self.current.min(self.list.len() - 1)]
    }

    fn front_mut(&mut self) -> &mut Service {
        let at = self.current.min(self.list.len() - 1);
        self.changed = true;
        &mut self.list[at]
    }

    /// Adds an item to the end of the service in front and chooses it. A song already in the
    /// service is chosen where it is, not added again.
    pub fn add(&mut self, item: Item) {
        if let Item::Song { key, .. } = &item
            && let Some(at) =
                self.front().items.iter().position(|have| matches!(have, Item::Song { key: k, .. } if k == key))
        {
            self.chosen = Some(at);
            return;
        }
        let items = &mut self.front_mut().items;
        items.push(item);
        self.chosen = Some(self.front().items.len() - 1);
    }

    /// Adds an item to the end of a playlist that need not be the one in front; a song already
    /// in it is not added again. Returns true if it was added.
    pub fn add_to(&mut self, playlist: usize, item: Item) -> bool {
        let Some(list) = self.list.get_mut(playlist) else { return false };
        if let Item::Song { key, .. } = &item
            && list.items.iter().any(|have| matches!(have, Item::Song { key: k, .. } if k == key))
        {
            return false;
        }
        list.items.push(item);
        self.changed = true;
        true
    }

    /// Puts an item into the service in front at a place (`slot` items above it) and chooses
    /// it. A song already in the service is chosen where it is, not added again.
    pub fn insert(&mut self, slot: usize, item: Item) {
        if let Item::Song { key, .. } = &item
            && let Some(at) =
                self.front().items.iter().position(|have| matches!(have, Item::Song { key: k, .. } if k == key))
        {
            self.chosen = Some(at);
            return;
        }
        let at = slot.min(self.front().items.len());
        self.front_mut().items.insert(at, item);
        self.chosen = Some(at);
    }

    /// Puts an item in a placeholder's place in the service in front, and chooses it. Returns
    /// false if the item there isn't a placeholder. A song already elsewhere in the service is
    /// moved here.
    pub fn fill(&mut self, index: usize, item: Item) -> bool {
        if !matches!(self.front().items.get(index), Some(Item::Placeholder { .. })) {
            return false;
        }
        let mut at = index;
        if let Item::Song { key, .. } = &item
            && let Some(was) =
                self.front().items.iter().position(|have| matches!(have, Item::Song { key: k, .. } if k == key))
        {
            self.front_mut().items.remove(was);
            if was < at {
                at -= 1;
            }
        }
        self.front_mut().items[at] = item;
        self.chosen = Some(at);
        true
    }

    /// Takes an item out of the service in front. Returns true if there was one.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.front().items.len() {
            return false;
        }
        self.front_mut().items.remove(index);
        self.chosen = None;
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

    /// Renames a header or a placeholder in the service shown.
    pub fn rename_header(&mut self, index: usize, name: &str) {
        if let Some(Item::Header { name: have } | Item::Placeholder { name: have }) =
            self.front_mut().items.get_mut(index)
            && !name.trim().is_empty()
        {
            *have = name.trim().to_owned();
        }
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
        let name = (1..).map(|n| format!("Playlist {n}")).find(|name| self.list.iter().all(|s| &s.name != name));
        self.list.push(Service { name: name.unwrap_or_default(), items: Vec::new() });
        self.current = self.list.len() - 1;
        self.chosen = None;
        self.changed = true;
    }

    /// Renames the service in front. An empty name is refused.
    pub fn rename(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && self.front().name != name {
            self.front_mut().name = name.to_owned();
        }
    }

    /// Removes the service in front. The last one cannot be removed.
    pub fn remove_service(&mut self) -> bool {
        if self.list.len() <= 1 {
            return false;
        }
        self.list.remove(self.current.min(self.list.len() - 1));
        self.current = self.current.saturating_sub(1).min(self.list.len() - 1);
        self.chosen = None;
        self.changed = true;
        true
    }

    pub fn to_json(&self) -> String {
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
        Ok(services)
    }
}

/// The small choices that are not part of any song, theme or service, kept between runs.
/// What starts playing by itself when the app opens: nothing, a media playlist (on the media
/// layer, item after item), or a song on the Announcements layer (by its key).
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
    /// True to start with the audience screens off; false to turn them on at launch.
    pub start_audience_off: bool,
    /// Next at the end of a song goes on to the next song in the service.
    pub continuous: bool,
    /// What starts playing by itself when the app opens.
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
    /// True when the left column was left listing a playlist, not a library's songs.
    #[serde(default)]
    pub playlist_in_front: bool,
    /// The order a library's songs are listed in: `listed`, `az` or `longest`.
    #[serde(default)]
    pub song_order: String,
    /// True when each button's key is written beside it (Help › Show Keys on Buttons).
    #[serde(default)]
    pub keys_on_buttons: bool,
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
    pub const ALL: [CopyrightShow; 4] =
        [CopyrightShow::Never, CopyrightShow::FirstSlide, CopyrightShow::LastSlide, CopyrightShow::EverySlide];

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
            start_with: StartWith::Nothing,
            setup_done: false,
            bible_lists_hidden: false,
            bible_rows_open: false,
            playlist_in_front: false,
            song_order: "listed".to_owned(),
            keys_on_buttons: false,
        }
    }
}

fn prefs_file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("preferences.json"))
}

/// The choices as last saved, or the usual ones.
pub fn load_prefs() -> Prefs {
    prefs_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remembers the choices for the next start.
pub fn save_prefs(prefs: &Prefs) -> Result<(), String> {
    let path = prefs_file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(prefs).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the settings: {e}"))
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
}

fn bible_history_file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("bible-history.json"))
}

/// The passages that were on screen, newest first. Nothing if there is no file yet.
pub fn load_bible_history() -> Vec<HistoryEntry> {
    bible_history_file().map(|path| load_history_from(&path)).unwrap_or_default()
}

fn load_history_from(path: &std::path::Path) -> Vec<HistoryEntry> {
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Keeps the passages that were on screen, for the next start.
pub fn save_bible_history(history: &[HistoryEntry]) -> Result<(), String> {
    let path =
        bible_history_file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    save_history_in(&path, history)
}

fn save_history_in(path: &std::path::Path, history: &[HistoryEntry]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let text = serde_json::to_string_pretty(history).unwrap_or_default();
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, text)
        .and_then(|()| std::fs::rename(&partial, path))
        .map_err(|e| format!("Could not save the Bible history: {e}"))
}

/// How the Bible page was last set up: its translations, boxes and slide options.
pub fn load_bible_page() -> Option<SavedPassage> {
    let text = std::fs::read_to_string(bible_page_file()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Remembers how the Bible page is set up, for the next start.
pub fn save_bible_page(page: &SavedPassage) -> Result<(), String> {
    let path = bible_page_file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let text = serde_json::to_string_pretty(page).unwrap_or_default();
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, text)
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the Bible page's settings: {e}"))
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("services.json"))
}

/// The services as last saved, or one empty service. A file that can't be read (or comes from
/// a newer build) is moved aside, so the empty service used instead is never saved over it.
pub fn load() -> Services {
    let Some(path) = file() else { return Services::default() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Services::default() };
    Services::from_json(&text).unwrap_or_else(|problem| {
        set_aside(&path, &problem);
        Services::default()
    })
}

/// Moves a settings file the app can't read out of the way, beside itself, so what the app uses
/// instead is never saved over it, and logs where it went.
pub fn set_aside(path: &std::path::Path, problem: &str) {
    let stem = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let aside = (1..)
        .map(|n| {
            let more = if n == 1 { String::new() } else { format!(" {n}") };
            path.with_file_name(format!("{stem} (could not be read{more}).json"))
        })
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.with_extension("unread"));
    match std::fs::rename(path, &aside) {
        Ok(()) => crate::applog::problem("settings", &format!("{problem} It was kept as {}.", aside.display())),
        Err(e) => crate::applog::problem("settings", &format!("{problem} It could not be moved aside: {e}")),
    }
}

/// Writes the services to their file. The error is a plain sentence.
pub fn save(services: &Services) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep services on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, services.to_json())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the services: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        Item::Song { key: key.to_owned(), title: title.to_owned() }
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
        // A song already in the playlist moves into the placeholder's place.
        services.insert(0, Item::Placeholder { name: "Opening".to_owned() });
        assert!(services.fill(0, song("two")));
        assert_eq!(services.front().items, vec![song("two"), song("one"), song("three")]);
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
        assert!(load_history_from(&path).is_empty(), "no file yet is no history");
        let history = vec![
            HistoryEntry { passage: "John 3:16".to_owned(), at: "2026-11-19T10:42:05".to_owned() },
            HistoryEntry { passage: "Psalm 23".to_owned(), at: "2026-11-17T09:30:00".to_owned() },
        ];
        save_history_in(&path, &history).unwrap();
        assert_eq!(load_history_from(&path), history);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_song_dropped_on_another_playlist_goes_there_once_and_the_front_stays() {
        let mut playlists = Services::default();
        playlists.add_service();
        playlists.show(0);
        let song = Item::Song { key: "songs/joy".to_owned(), title: "Joy".to_owned() };
        assert!(playlists.add_to(1, song.clone()));
        assert!(!playlists.add_to(1, song), "a song already there is not added again");
        assert!(!playlists.add_to(9, Item::Header { name: "x".to_owned() }));
        assert_eq!((playlists.current, playlists.list[1].items.len(), playlists.list[0].items.len()), (0, 1, 0));
        assert_eq!(playlists.list[1].name, "Playlist 1");
    }

    fn song(key: &str) -> Item {
        Item::Song { key: key.to_owned(), title: key.to_uppercase() }
    }

    fn passage(lookup: &str) -> Item {
        Item::Passage(SavedPassage {
            lookup: lookup.to_owned(),
            rows: vec![SavedRow { bible: "kjv".to_owned(), verse: Some("primary".to_owned()), reference: None }],
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference_style: "verse".to_owned(),
        })
    }

    #[test]
    fn items_are_added_in_order_and_a_song_is_not_added_twice() {
        let mut services = Services::default();
        services.add(song("a"));
        services.add(passage("John 3:16"));
        services.add(song("a"));
        assert_eq!(services.front().items.len(), 2);
        assert_eq!(services.chosen, Some(0));
        // The same passage may be wanted twice.
        services.add(passage("John 3:16"));
        assert_eq!(services.front().items.len(), 3);
        assert!(services.changed);
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
        assert!(services.remove(1));
        assert!(!services.remove(9));
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
    fn services_are_made_shown_renamed_and_removed_but_never_all() {
        let mut services = Services::default();
        services.add(song("a"));
        services.add_service();
        assert_eq!((services.list.len(), services.current), (2, 1));
        assert!(services.front().items.is_empty());
        services.rename("  Sunday ");
        services.rename("   ");
        assert_eq!(services.front().name, "Sunday");
        services.show(0);
        assert_eq!(services.front().items.len(), 1);
        assert!(services.remove_service());
        assert_eq!(services.front().name, "Sunday");
        assert!(!services.remove_service());
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
    fn services_survive_their_file_and_other_files_are_refused() {
        let mut services = Services::default();
        services.add(song("a"));
        services.add(passage("Пс 22:1-3"));
        services.add_service();
        let mut again = Services::from_json(&services.to_json()).unwrap();
        // What is chosen and whether anything changed are not kept.
        again.changed = services.changed;
        again.chosen = services.chosen;
        assert_eq!(again, services);
        assert!(Services::from_json("{}").is_err());
        assert!(
            Services::from_json("{\"format\":\"something else\",\"version\":1,\"list\":[],\"current\":0}").is_err()
        );
    }
}
