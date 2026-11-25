//! What the app keeps of TimelyChurch, in the data folder's `TimelyChurch` folder; nothing in
//! it is secret (the token is in the system's secure store, see `token`):
//!
//! - `settings.json`: the church connected to, when it was last checked, and what was seen of
//!   its services and songs (to tell what changed);
//! - `services.json`: the last services list, as sent, with its ETag, so a "nothing changed"
//!   answer after a restart still has the list it means;
//! - `songs/`: each song as last sent, so a newer version offered can be taken later with no
//!   network;
//! - `song-uses.json`: the songs shown in services that TimelyChurch hasn't been told of yet,
//!   each with its own idempotency key, and which have been recorded already.
//!
//! The songs are kept for the church's own use only: they are never shared or sent anywhere.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::wire::{Id, SongUse};

/// The folder's name in the data folder.
pub const FOLDER: &str = "TimelyChurch";

/// What is kept about the connection and what was seen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The church's TimelyChurch address the computer was paired with (`https://…`).
    pub address: String,
    /// The church's name, as TimelyChurch gave it.
    pub church: String,
    pub church_id: Option<Id>,
    /// A token is kept for the church: connected until someone disconnects.
    pub connected: bool,
    /// TimelyChurch refused the token (the computer was disconnected there, or the church
    /// turned the connection off).
    pub disconnected_there: bool,
    /// When TimelyChurch last answered a check (seconds since 1970).
    pub last_checked: Option<i64>,
    /// The songs fetched, and when.
    pub songs: Vec<SongSeen>,
    /// The services followed, as last seen.
    pub services: Vec<ServiceSeen>,
    /// Services whose playlist the church deleted: not made again.
    pub let_go: Vec<String>,
}

/// A song fetched: what was asked for (the song, in an arrangement or its default), the key of
/// the library song made from it, and when TimelyChurch last changed it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongSeen {
    pub song: Id,
    pub arrangement: Option<Id>,
    pub key: String,
    pub updated_at: Option<String>,
    /// When it was fetched (seconds since 1970).
    pub fetched_at: i64,
    /// TimelyChurch no longer has it.
    pub gone: bool,
}

/// A service followed: its key, when TimelyChurch last changed it and its date.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServiceSeen {
    pub key: String,
    pub updated_at: Option<String>,
    pub date: Option<String>,
}

impl Settings {
    pub fn seen_song(&self, song: &Id, arrangement: Option<&Id>) -> Option<&SongSeen> {
        self.songs.iter().find(|seen| seen.song == *song && seen.arrangement.as_ref() == arrangement)
    }

    /// Records a song fetched, in place of what was seen of it before.
    pub fn saw_song(&mut self, seen: SongSeen) {
        self.songs.retain(|have| !(have.song == seen.song && have.arrangement == seen.arrangement));
        self.songs.push(seen);
    }
}

/// The last services list, as sent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cached {
    pub etag: Option<String>,
    /// The dates it was asked for.
    pub from: String,
    pub to: String,
    pub body: String,
}

/// A song shown in a service, waiting to be sent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Queued {
    /// Its idempotency key: made once, when it is queued, and sent with every try.
    pub key: String,
    #[serde(rename = "use")]
    pub used: SongUse,
}

/// The songs shown in services: those waiting to be sent, and which (service and song) have
/// been counted already, so a song is counted once per service whatever happens after.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UseBook {
    pub queued: Vec<Queued>,
    pub counted: Vec<String>,
}

/// How many counted songs are remembered: plenty for every service in the window.
const MOST_COUNTED: usize = 2000;

impl UseBook {
    /// Counts a song shown in a service once: queues its use with an idempotency key made from
    /// what it is (the same if it were ever queued again), unless it has been counted already.
    /// True when it was queued.
    pub fn count(&mut self, counted_as: &str, used: SongUse) -> bool {
        if self.counted.iter().any(|have| have == counted_as) {
            return false;
        }
        self.counted.push(counted_as.to_owned());
        if self.counted.len() > MOST_COUNTED {
            let extra = self.counted.len() - MOST_COUNTED;
            self.counted.drain(..extra);
        }
        self.queued.push(Queued { key: idempotency_key(counted_as), used });
        true
    }

    /// Takes away the uses TimelyChurch has (or will never take), by their keys.
    pub fn sent(&mut self, keys: &[String]) {
        self.queued.retain(|queued| !keys.contains(&queued.key));
    }
}

/// An idempotency key for a use: a UUID-shaped SHA-256 of what was counted (the service and
/// the song), so the same use always has the same key.
pub fn idempotency_key(counted_as: &str) -> String {
    use sha2::{Digest, Sha256};
    let hex: String = Sha256::digest(counted_as.as_bytes())[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// The folder and its files.
#[derive(Clone, Debug)]
pub struct Files {
    root: PathBuf,
}

impl Files {
    /// In the app's data folder.
    pub fn in_data_dir() -> Option<Files> {
        crate::bible_store::data_dir().map(|dir| Files::at(dir.join(FOLDER)))
    }

    pub fn at(root: PathBuf) -> Files {
        Files { root }
    }

    pub fn settings(&self) -> Settings {
        read(&self.root.join("settings.json")).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        write(&self.root.join("settings.json"), settings)
    }

    pub fn cached(&self) -> Option<Cached> {
        read(&self.root.join("services.json"))
    }

    pub fn save_cached(&self, cached: &Cached) -> Result<(), String> {
        write(&self.root.join("services.json"), cached)
    }

    /// Forgets the list kept, and its ETag: the next check asks for the whole list.
    pub fn forget_cached(&self) {
        let _ = std::fs::remove_file(self.root.join("services.json"));
    }

    fn song_file(&self, song: &Id, arrangement: Option<&Id>) -> Option<PathBuf> {
        if !song.is_safe() || arrangement.is_some_and(|a| !a.is_safe()) {
            return None;
        }
        let name = match arrangement {
            Some(arrangement) => format!("{song}@{arrangement}.json"),
            None => format!("{song}.json"),
        };
        Some(self.root.join("songs").join(name))
    }

    /// The song as last sent, if it was kept.
    pub fn song(&self, song: &Id, arrangement: Option<&Id>) -> Option<super::wire::Song> {
        read(&self.song_file(song, arrangement)?)
    }

    /// Keeps a song as sent (its body, as it came).
    pub fn save_song(&self, song: &Id, arrangement: Option<&Id>, body: &str) {
        if let Some(file) = self.song_file(song, arrangement) {
            let _ = write_text(&file, body);
        }
    }

    pub fn uses(&self) -> UseBook {
        read(&self.root.join("song-uses.json")).unwrap_or_default()
    }

    pub fn save_uses(&self, book: &UseBook) -> Result<(), String> {
        write(&self.root.join("song-uses.json"), book)
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    write_text(path, &serde_json::to_string_pretty(value).unwrap_or_default())
}

/// Writes a file whole or not at all: to a part file first, then put in its place.
fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, text).and_then(|()| std::fs::rename(&partial, path)).map_err(|e| {
        crate::i18n::tf(
            "integrations.timelychurch.not_saved",
            "What SoftPresenter keeps of TimelyChurch could not be saved: {error}",
            &[("error", &e.to_string())],
        )
    })
}
