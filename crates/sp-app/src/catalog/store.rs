//! Where the catalog is kept: `Catalog/worshipleaderapp/<language>/` in the data folder, apart
//! from the church's library.
//!
//! Each language is three small things:
//!
//! - `songs-<hash>.qsp`: the song pack as it was downloaded, kept whole (about 16 MB for the
//!   biggest language, packed as it came). It is never unpacked as a whole again: a song is read
//!   from it by its place in the zip when one is opened or joined.
//! - `index.json`: made once from the pack: for each song its key, title, first line, place in
//!   the zip, a fingerprint of its text and the languages it has versions in; with the
//!   validators the server gave (ETag, Last-Modified), when the pack was fetched, the counts,
//!   and the songs the source has dropped. This is the commit point: a new pack is written
//!   under a name of its own first, and the index that names it is renamed into place last, so
//!   a crash leaves either the old pack and its index or the new ones.
//! - `status.json`: when the language was last checked, so a check that finds nothing new
//!   rewrites only this.
//!
//! Keeping the pack means a newer version of this program can make the index again (a changed
//! key scheme, say) without downloading anything.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::fold::fold;
use super::read::{self, KEY_VERSION, Song};
use crate::i18n::tf;

/// What an index file says it is.
const INDEX_FORMAT: &str = "softpresenter-song-catalog-index";
const INDEX_VERSION: u32 = 1;
/// How many dropped songs are remembered for a language.
const MOST_GONE: usize = 5_000;
/// A new pack that has fewer than this share of the songs the old one had is not believed: it
/// would mark most of the catalog gone. It is kept out, and the old pack stays.
const SHRINK_LIMIT_PER_CENT: usize = 50;
/// Below this many songs in the old pack, any new pack is believed.
const SHRINK_CHECK_FROM: usize = 40;

/// One song in a language's index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub title: String,
    /// The first line of the song's first section, as written.
    pub line: String,
    /// The song's place in the zip.
    pub at: u32,
    /// A fingerprint of the song's text (`read::text_hash`).
    pub hash: String,
    /// The language codes of the song's versions in other languages, in the pack's order.
    pub langs: Vec<String>,
    pub chords: bool,
    pub sections: u16,
}

/// A song the source no longer has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gone {
    pub key: String,
    pub title: String,
    /// The date it was found to be gone.
    pub since: String,
}

/// An index file.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct IndexFile {
    format: String,
    version: u32,
    key_version: u32,
    source: String,
    lang: String,
    /// The pack's file name, its length and SHA-256.
    pack: String,
    pack_len: u64,
    pack_sha256: String,
    etag: Option<String>,
    last_modified: Option<String>,
    /// When the pack was fetched (seconds since 1970).
    fetched: i64,
    malformed: usize,
    empty: usize,
    duplicates: usize,
    entries: Vec<Entry>,
    gone: Vec<Gone>,
}

/// What `status.json` says.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    /// When the source was last asked whether there is anything new (seconds since 1970); 0 if
    /// never, which a fresh download counts as asking.
    pub checked: i64,
    /// Why the last check failed, if it did.
    pub problem: Option<String>,
}

/// A language's index in memory, with what searching needs.
#[derive(Debug)]
pub struct LangIndex {
    pub lang: String,
    file: IndexFileInfo,
    pub entries: Vec<Entry>,
    pub gone: Vec<Gone>,
    /// Each entry's title folded, and its title and first lines folded together.
    pub(super) folded: Vec<(String, String)>,
    by_key: HashMap<String, usize>,
}

/// What an index says about its pack, for the people who ask.
#[derive(Clone, Debug)]
pub struct IndexFileInfo {
    pub pack: String,
    pub pack_len: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub fetched: i64,
    pub malformed: usize,
    pub empty: usize,
    pub duplicates: usize,
}

impl LangIndex {
    fn from_file(file: IndexFile) -> LangIndex {
        let folded = file
            .entries
            .iter()
            .map(|entry| {
                let title = fold(&entry.title);
                let words = format!("{title} {}", fold(&entry.line));
                (title, words)
            })
            .collect();
        let by_key = file.entries.iter().enumerate().map(|(at, entry)| (entry.key.clone(), at)).collect();
        LangIndex {
            lang: file.lang,
            file: IndexFileInfo {
                pack: file.pack,
                pack_len: file.pack_len,
                etag: file.etag,
                last_modified: file.last_modified,
                fetched: file.fetched,
                malformed: file.malformed,
                empty: file.empty,
                duplicates: file.duplicates,
            },
            entries: file.entries,
            gone: file.gone,
            folded,
            by_key,
        }
    }

    pub fn info(&self) -> &IndexFileInfo {
        &self.file
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.by_key.get(key).map(|at| &self.entries[*at])
    }

    pub fn is_gone(&self, key: &str) -> Option<&Gone> {
        self.gone.iter().find(|gone| gone.key == key)
    }
}

/// True for a language code the catalog will use as a folder name and in an address: two or
/// three lower-case letters, perhaps a dash and up to eight more letters or digits.
pub fn valid_language(lang: &str) -> bool {
    let (base, region) = match lang.split_once('-') {
        Some((base, region)) => (base, Some(region)),
        None => (lang, None),
    };
    (2..=3).contains(&base.len())
        && base.bytes().all(|b| b.is_ascii_lowercase())
        && region.is_none_or(|region| {
            (1..=8).contains(&region.len())
                && region.bytes().all(|b| b.is_ascii_alphanumeric() && !b.is_ascii_uppercase())
        })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A date as year-month-day, from seconds since 1970.
pub fn date_of(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0).map_or_else(String::new, |time| time.format("%Y-%m-%d").to_string())
}

/// What installing a pack did.
#[derive(Debug)]
pub struct Installed {
    pub songs: usize,
    pub malformed: usize,
    pub empty: usize,
    pub duplicates: usize,
    /// Songs the old pack had and this one does not.
    pub gone: usize,
    /// Songs new to the catalog, and songs whose text changed.
    pub added: usize,
    pub changed: usize,
}

/// The catalog on disk: one folder, a language to a subfolder.
pub struct Catalog {
    root: PathBuf,
    loaded: Mutex<HashMap<String, Arc<LangIndex>>>,
    zips: Mutex<HashMap<String, zip::ZipArchive<File>>>,
}

impl Catalog {
    /// A catalog kept in `root`, which need not exist yet.
    pub fn at(root: PathBuf) -> Catalog {
        Catalog { root, loaded: Mutex::new(HashMap::new()), zips: Mutex::new(HashMap::new()) }
    }

    /// The catalog in the app's data folder.
    pub fn in_data_dir() -> Option<Catalog> {
        crate::bible_store::data_dir().map(|dir| Catalog::at(dir.join("Catalog").join(super::SOURCE)))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn dir(&self, lang: &str) -> Result<PathBuf, String> {
        if valid_language(lang) {
            Ok(self.root.join(lang))
        } else {
            Err(format!("\u{201c}{lang}\u{201d} isn't a language code (two or three small letters, such as ru)."))
        }
    }

    fn forget_loaded(&self, lang: &str) {
        if let Ok(mut loaded) = self.loaded.lock() {
            loaded.remove(lang);
        }
        if let Ok(mut zips) = self.zips.lock() {
            zips.remove(lang);
        }
    }

    /// The languages that have songs here, in order of their codes.
    pub fn installed(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.root) else { return Vec::new() };
        let mut langs: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| valid_language(name) && self.root.join(name).join("index.json").is_file())
            .collect();
        langs.sort();
        langs
    }

    /// A language's index, read from disk the first time and then kept. `Ok(None)` when the
    /// language isn't here. An index made under another key scheme is made again from the pack
    /// kept beside it.
    pub fn index(&self, lang: &str) -> Result<Option<Arc<LangIndex>>, String> {
        let dir = self.dir(lang)?;
        if let Ok(loaded) = self.loaded.lock()
            && let Some(have) = loaded.get(lang)
        {
            return Ok(Some(have.clone()));
        }
        let Ok(text) = std::fs::read_to_string(dir.join("index.json")) else { return Ok(None) };
        let mut file: IndexFile = serde_json::from_str(&text)
            .map_err(|e| format!("The {lang} song catalog's index can't be read ({e}). Download it again."))?;
        if file.format != INDEX_FORMAT || file.version > INDEX_VERSION || file.lang != lang {
            return Err(format!("The {lang} song catalog's index isn't one this version reads. Download it again."));
        }
        let pack = dir.join(&file.pack);
        if !valid_pack_name(&file.pack) || std::fs::metadata(&pack).map(|meta| meta.len()).ok() != Some(file.pack_len) {
            return Err(format!("The {lang} song catalog's songs are missing or damaged. Download it again."));
        }
        if file.key_version != KEY_VERSION {
            let bytes = std::fs::read(&pack).map_err(|e| format!("The {lang} songs can't be read ({e})."))?;
            file = build_index(lang, &bytes, &file.pack, &file_meta(&file), &file.gone)?;
            write_atomically(
                &dir.join("index.json"),
                serde_json::to_string(&file).map_err(|e| e.to_string())?.as_bytes(),
            )?;
        }
        let index = Arc::new(LangIndex::from_file(file));
        if let Ok(mut loaded) = self.loaded.lock() {
            loaded.insert(lang.to_owned(), index.clone());
        }
        Ok(Some(index))
    }

    /// When a language was last checked, and why that failed if it did.
    pub fn status(&self, lang: &str) -> Status {
        self.dir(lang)
            .ok()
            .and_then(|dir| std::fs::read_to_string(dir.join("status.json")).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Notes that the source was asked at `now`, and how it went.
    pub fn note_checked(&self, lang: &str, now: i64, problem: Option<String>) -> Result<(), String> {
        let dir = self.dir(lang)?;
        let text = serde_json::to_string(&Status { checked: now, problem }).map_err(|e| e.to_string())?;
        write_atomically(&dir.join("status.json"), text.as_bytes())
    }

    /// The bytes a language takes on disk.
    pub fn disk_bytes(&self, lang: &str) -> u64 {
        let Ok(dir) = self.dir(lang) else { return 0 };
        std::fs::read_dir(dir)
            .map(|entries| entries.flatten().filter_map(|entry| entry.metadata().ok()).map(|meta| meta.len()).sum())
            .unwrap_or(0)
    }

    /// Takes a downloaded pack in as a language's songs: it is read whole first (so a pack that
    /// can't be read changes nothing), the songs the old pack had and this one lacks are noted
    /// as gone, and the new pack and index are put in place, the index last.
    pub fn install(
        &self,
        lang: &str,
        bytes: &[u8],
        etag: Option<String>,
        last_modified: Option<String>,
        now: i64,
    ) -> Result<Installed, String> {
        let dir = self.dir(lang)?;
        let old = self.index(lang).ok().flatten();
        let pack_sha = hex(&Sha256::digest(bytes));
        let pack = format!("songs-{}.qsp", &pack_sha[..16]);
        let meta =
            Meta { etag, last_modified, fetched: now, pack_sha256: pack_sha.clone(), pack_len: bytes.len() as u64 };
        let carried: Vec<Gone> = old.as_ref().map(|old| old.gone.clone()).unwrap_or_default();
        let mut file = build_index(lang, bytes, &pack, &meta, &carried)?;
        // What changed against the old index.
        let (mut gone, mut added, mut changed) = (0, 0, 0);
        if let Some(old) = &old {
            let now_has: HashMap<&str, &Entry> = file.entries.iter().map(|e| (e.key.as_str(), e)).collect();
            if old.entries.len() >= SHRINK_CHECK_FROM
                && file.entries.len() * 100 < old.entries.len() * SHRINK_LIMIT_PER_CENT
            {
                return Err(format!(
                    "The new {lang} songs have {} songs where there were {}; that is not believed, so the songs here are kept.",
                    file.entries.len(),
                    old.entries.len()
                ));
            }
            let since = date_of(now);
            for entry in &old.entries {
                match now_has.get(entry.key.as_str()) {
                    None => {
                        gone += 1;
                        file.gone.push(Gone {
                            key: entry.key.clone(),
                            title: entry.title.clone(),
                            since: since.clone(),
                        });
                    }
                    Some(new) if new.hash != entry.hash => changed += 1,
                    Some(_) => {}
                }
            }
            let had: std::collections::HashSet<&str> = old.entries.iter().map(|e| e.key.as_str()).collect();
            added = file.entries.iter().filter(|e| !had.contains(e.key.as_str())).count();
        }
        // A song that is back is no longer gone; the list is bounded.
        let present: std::collections::HashSet<String> = file.entries.iter().map(|e| e.key.clone()).collect();
        file.gone.retain(|gone| !present.contains(&gone.key));
        if file.gone.len() > MOST_GONE {
            let extra = file.gone.len() - MOST_GONE;
            file.gone.drain(..extra);
        }
        std::fs::create_dir_all(&dir).map_err(|e| {
            tf(
                "notices.catalog.no_folder",
                "Could not make the catalog's folder: {error}",
                &[("error", &crate::notices::write_problem(&e))],
            )
        })?;
        write_atomically(&dir.join(&pack), bytes)?;
        let text = serde_json::to_string(&file).map_err(|e| e.to_string())?;
        write_atomically(&dir.join("index.json"), text.as_bytes())?;
        self.forget_loaded(lang);
        // The check is the fetch just made; the old pack and any half-written file go.
        let _ = self.note_checked(lang, now, None);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if (name.starts_with("songs-") && name != pack) || name.ends_with(".part") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(Installed {
            songs: file.entries.len(),
            malformed: file.malformed,
            empty: file.empty,
            duplicates: file.duplicates,
            gone,
            added,
            changed,
        })
    }

    /// Takes a language's songs away: its folder goes. Songs already copied into the library
    /// stay there.
    pub fn remove(&self, lang: &str) -> Result<bool, String> {
        let dir = self.dir(lang)?;
        self.forget_loaded(lang);
        if !dir.exists() {
            return Ok(false);
        }
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not remove the {lang} songs: {e}"))?;
        Ok(true)
    }

    /// Reads a song from its language's pack. The key must be one the index has; the song read
    /// must still be the one the index fingerprinted.
    pub fn song(&self, key: &str) -> Result<(String, Entry, Song), String> {
        let lang = lang_of_key(key).ok_or_else(|| format!("\u{201c}{key}\u{201d} isn't a song key."))?;
        let index = self.index(lang)?.ok_or_else(|| format!("The {lang} songs aren't downloaded."))?;
        let entry = index.entry(key).ok_or_else(|| format!("The catalog has no song with the key {key}."))?.clone();
        let dir = self.dir(lang)?;
        let song = {
            let mut zips = self.zips.lock().map_err(|_| "The catalog is busy.".to_owned())?;
            if !zips.contains_key(lang) {
                let file = File::open(dir.join(&index.info().pack))
                    .map_err(|e| format!("The {lang} songs can't be opened ({e})."))?;
                let archive = zip::ZipArchive::new(file)
                    .map_err(|_| format!("The {lang} songs are damaged. Download them again."))?;
                zips.insert(lang.to_owned(), archive);
            }
            let archive = zips.get_mut(lang).ok_or("The catalog is busy.")?;
            let xml = sp_model::bounded_zip::read_entry(archive, entry.at as usize, read::LIMITS.entry)
                .map_err(|_| format!("The song {key} can't be read from the {lang} songs. Download them again."))?;
            read::read_song(&xml).ok().flatten().ok_or_else(|| format!("The song {key} can't be read."))?
        };
        if read::text_hash(&song) != entry.hash {
            return Err(format!(
                "The song {key} isn't what the catalog's index says; download the {lang} songs again."
            ));
        }
        Ok((lang.to_owned(), entry, song))
    }
}

/// What a fetch brings with the pack.
struct Meta {
    etag: Option<String>,
    last_modified: Option<String>,
    fetched: i64,
    pack_sha256: String,
    pack_len: u64,
}

fn file_meta(file: &IndexFile) -> Meta {
    Meta {
        etag: file.etag.clone(),
        last_modified: file.last_modified.clone(),
        fetched: file.fetched,
        pack_sha256: file.pack_sha256.clone(),
        pack_len: file.pack_len,
    }
}

/// A pack's file name: what `install` makes and nothing that could be a path.
fn valid_pack_name(name: &str) -> bool {
    name.strip_prefix("songs-")
        .and_then(|rest| rest.strip_suffix(".qsp"))
        .is_some_and(|hash| !hash.is_empty() && hash.len() <= 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The language a key is for: the letters before its first colon.
pub fn lang_of_key(key: &str) -> Option<&str> {
    key.split_once(':').map(|(lang, _)| lang).filter(|lang| valid_language(lang))
}

/// Why a pack was refused, in a sentence in the language set (a language's status keeps the
/// problem in English: see `Catalog::download_from`).
fn refused_words(lang: &str, why: &read::Refused) -> String {
    let lang = [("lang", lang)];
    match why {
        read::Refused::NotAZip => {
            tf("notices.catalog.refused.not_a_pack", "The {lang} songs isn't a song pack that could be read.", &lang)
        }
        read::Refused::TooMany => {
            tf("notices.catalog.refused.too_many", "The {lang} songs holds more files than a song pack should.", &lang)
        }
        read::Refused::TooBig => {
            tf("notices.catalog.refused.too_big", "The {lang} songs unpacks to more than a song pack should.", &lang)
        }
        read::Refused::NoSongs => tf("notices.catalog.refused.no_songs", "The {lang} songs has no songs in it.", &lang),
    }
}

/// Reads a pack whole and makes its index: songs sorted by title, each with its key.
fn build_index(lang: &str, bytes: &[u8], pack: &str, meta: &Meta, carried: &[Gone]) -> Result<IndexFile, String> {
    let found = read::read_pack(bytes).map_err(|why| refused_words(lang, &why))?;
    let mut entries: Vec<Entry> = read::keyed(lang, found.songs)
        .into_iter()
        .map(|(key, at, song)| Entry {
            line: song.sections.first().and_then(|s| s.text.lines().next()).unwrap_or("").to_owned(),
            hash: read::text_hash(&song),
            langs: {
                let mut langs: Vec<String> = Vec::new();
                for version in &song.versions {
                    if !langs.contains(&version.lang) {
                        langs.push(version.lang.clone());
                    }
                }
                langs
            },
            chords: song.chords,
            sections: u16::try_from(song.sections.len()).unwrap_or(u16::MAX),
            title: song.title,
            at: u32::try_from(at).unwrap_or(u32::MAX),
            key,
        })
        .collect();
    entries.sort_by_cached_key(|entry| (fold(&entry.title), entry.key.clone()));
    Ok(IndexFile {
        format: INDEX_FORMAT.to_owned(),
        version: INDEX_VERSION,
        key_version: KEY_VERSION,
        source: super::SOURCE.to_owned(),
        lang: lang.to_owned(),
        pack: pack.to_owned(),
        pack_len: meta.pack_len,
        pack_sha256: meta.pack_sha256.clone(),
        etag: meta.etag.clone(),
        last_modified: meta.last_modified.clone(),
        fetched: meta.fetched,
        malformed: found.malformed,
        empty: found.empty,
        duplicates: found.duplicates,
        entries,
        gone: carried.to_vec(),
    })
}

/// Writes a file under a name of its own and renames it into place, so a reader sees the old
/// file or the new, never half of one.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let partial = path.with_extension(match path.extension() {
        Some(ext) => format!("{}.part", ext.to_string_lossy()),
        None => "part".to_owned(),
    });
    std::fs::write(&partial, bytes).and_then(|()| std::fs::rename(&partial, path)).map_err(|e| {
        tf(
            "notices.catalog.not_saved",
            "Could not save the catalog's files: {error}",
            &[("error", &crate::notices::write_problem(&e))],
        )
    })
}
