//! The song catalog: free songs, one songbook per language, published by Worship Leader App
//! (worshipleaderapp.com) as a song pack for each language. Each church downloads the languages
//! it wants from the source itself; the texts are the source's.
//!
//! Downloaded songs are kept apart from the church's library (`store`), searched from the find
//! box (`search`), and **join the library** (a library named "Worship Leader App") only when a
//! person adds one to a playlist, drags it in or edits it (`present`; `catalog_songs` for the
//! window's side and `catalog_cli` for the app's). About once a month the app asks the source
//! whether a language has changed (a conditional request: `fetch`), never while anything is on
//! screen; songs nobody has touched are brought up to date, and a song the church changed is
//! never overwritten, a newer version is offered instead.
//!
//! This module is the engine, with no window of its own: `books` (the songbooks the source
//! publishes, and which to recommend), `read` (the song pack), `fold` (searching alike), `store`
//! (the files), `search`, `fetch` (the network), `present` (making a song and judging an
//! update). The windows that show it (the setup's Free Songs step, Settings › Integrations) are
//! in `songbooks`, and its songs in the find box and the slides area in `catalog_songs`; all are
//! offered only in development builds for now ([`offered`]).

// Part of the engine's API (some of what is re-exported below, `refreshed_song`, `is_gone`, a
// song's credit, the counts in `Installed`…) is for windows still to come (a chooser of the
// catalog's songs, the newer versions offered), and tested only until they use it.
#![allow(dead_code, unused_imports)]

mod books;
mod fetch;
#[cfg(test)]
pub(crate) mod fixtures;
mod fold;
mod present;
mod read;
mod search;
mod store;

pub use books::{SONGBOOKS, Songbook, in_name_order, recommended, songbook};
pub use fetch::{Have, Progress, Reply, base_url};
pub use fold::fold;
pub use present::{
    CREDIT, Credit, Pairing, Presented, Source, Verdict, fingerprint, judge, kept, marked, present, refreshed,
    refreshed_song,
};
pub use read::{KEY_VERSION, Section, Song, Version, read_pack, song_key, text_hash};
pub use search::{Found, Hit, search_indexes};
pub use store::{Catalog, Entry, Gone, Installed, LangIndex, date_of, lang_of_key, valid_language};

use std::sync::{Arc, OnceLock};

/// True in a development build: one made with `SOFTPRESENTER_DEV_VERSION` set when it was
/// compiled. The catalog's windows are offered only there until Worship Leader App has answered
/// about the app offering its songs (the owner, 24 November 2026); a release shows none of them.
pub const DEV_BUILD: bool = option_env!("SOFTPRESENTER_DEV_VERSION").is_some();

/// Whether the windows offer the catalog: the setup's Free Songs step and Settings ›
/// Integrations › Worship Leader App. The `sp catalog` commands are there in every build.
pub fn offered() -> bool {
    static OFFERED: OnceLock<bool> = OnceLock::new();
    // Testing aid: SOFTPRESENTER_CATALOG_OFFERED=1 offers the catalog's windows in a build that
    // isn't a development build, to try them.
    *OFFERED.get_or_init(|| DEV_BUILD || std::env::var("SOFTPRESENTER_CATALOG_OFFERED").is_ok_and(|v| v == "1"))
}

/// The catalog's name inside the data folder and in a song's mark.
pub const SOURCE: &str = "worshipleaderapp";
/// What the app calls it, and the library its songs join.
pub const SOURCE_NAME: &str = "Worship Leader App";
pub const LIBRARY_NAME: &str = "Worship Leader App";
/// Its site, which the credit links to.
pub const SOURCE_SITE: &str = "https://worshipleaderapp.com";

/// How often a downloaded language is asked about, in seconds: about a month.
pub const CHECK_EVERY: i64 = 30 * 24 * 60 * 60;
/// How long after a check that failed (no network, the source down) the next is tried.
pub const RETRY_AFTER: i64 = 24 * 60 * 60;

/// What is known of a downloaded language.
#[derive(Clone, Debug)]
pub struct LanguageInfo {
    pub lang: String,
    pub songs: usize,
    /// Songs the source has dropped since they were downloaded.
    pub gone: usize,
    /// Files in the pack left out for not being well-formed, and songs left out as duplicates.
    pub malformed: usize,
    pub duplicates: usize,
    /// The pack as downloaded, and everything the language takes on disk, in bytes.
    pub pack_bytes: u64,
    pub disk_bytes: u64,
    /// When the pack was fetched, and when the source was last asked (seconds since 1970).
    pub fetched: i64,
    pub checked: i64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Why the last check failed.
    pub problem: Option<String>,
}

/// What a download or a check of one language did.
#[derive(Debug)]
pub enum Outcome {
    /// Nothing new at the source.
    Unchanged { songs: usize },
    /// A pack was fetched and taken in.
    Installed(Installed),
}

/// What setup shows for a language: how many songs, how big the download, how much room it
/// takes.
#[derive(Clone, Debug)]
pub struct Size {
    pub lang: String,
    /// The songs, known once the language has been downloaded.
    pub songs: Option<usize>,
    /// The download, from the last fetch or from the server's answer to a request for its size.
    pub download_bytes: Option<u64>,
    pub disk_bytes: u64,
    pub installed: bool,
    /// Why the server's answer couldn't be had.
    pub problem: Option<String>,
}

impl Catalog {
    /// What is known of each downloaded language.
    pub fn languages(&self) -> Vec<LanguageInfo> {
        self.installed()
            .into_iter()
            .map(|lang| {
                let status = self.status(&lang);
                let (index, problem) = match self.index(&lang) {
                    Ok(index) => (index, status.problem.clone()),
                    Err(problem) => (None, Some(problem)),
                };
                let info = index.as_ref().map(|index| index.info().clone());
                LanguageInfo {
                    songs: index.as_ref().map_or(0, |index| index.entries.len()),
                    gone: index.as_ref().map_or(0, |index| index.gone.len()),
                    malformed: info.as_ref().map_or(0, |info| info.malformed),
                    duplicates: info.as_ref().map_or(0, |info| info.duplicates),
                    pack_bytes: info.as_ref().map_or(0, |info| info.pack_len),
                    disk_bytes: self.disk_bytes(&lang),
                    fetched: info.as_ref().map_or(0, |info| info.fetched),
                    checked: status.checked,
                    etag: info.as_ref().and_then(|info| info.etag.clone()),
                    last_modified: info.as_ref().and_then(|info| info.last_modified.clone()),
                    problem,
                    lang,
                }
            })
            .collect()
    }

    /// For setup: for each of these languages, the songs it has, the download's size and the room
    /// it takes. A language not downloaded is asked about with a request that downloads nothing
    /// (when `ask` is true); one that was is answered from its last fetch.
    pub fn sizes(&self, langs: &[String], ask: bool) -> Vec<Size> {
        self.sizes_from(&fetch::base_url(), langs, ask)
    }

    /// [`Catalog::sizes`] asking the source at `base`.
    pub fn sizes_from(&self, base: &str, langs: &[String], ask: bool) -> Vec<Size> {
        langs
            .iter()
            .map(|lang| {
                if let Ok(Some(index)) = self.index(lang) {
                    return Size {
                        lang: lang.clone(),
                        songs: Some(index.entries.len()),
                        download_bytes: Some(index.info().pack_len),
                        disk_bytes: self.disk_bytes(lang),
                        installed: true,
                        problem: None,
                    };
                }
                let (download_bytes, problem) = if ask {
                    match fetch::size_of(base, lang) {
                        Ok(size) => (size, None),
                        Err(problem) => (None, Some(problem)),
                    }
                } else {
                    (None, None)
                };
                Size { lang: lang.clone(), songs: None, download_bytes, disk_bytes: 0, installed: false, problem }
            })
            .collect()
    }

    /// Downloads a language, or asks the source whether the one here has changed and downloads
    /// it only if it has (with the ETag and Last-Modified of the last fetch). Slow: it is for a
    /// thread of its own, never the window's. `progress` is filled in as the pack comes.
    pub fn download(&self, lang: &str, now: i64, progress: Option<&Progress>) -> Result<Outcome, String> {
        self.download_from(&fetch::base_url(), lang, now, progress)
    }

    /// [`Catalog::download`] from the source at `base`.
    pub fn download_from(
        &self,
        base: &str,
        lang: &str,
        now: i64,
        progress: Option<&Progress>,
    ) -> Result<Outcome, String> {
        let index = self.index(lang).ok().flatten();
        let have = index
            .as_ref()
            .map(|index| Have { etag: index.info().etag.clone(), last_modified: index.info().last_modified.clone() })
            .unwrap_or_default();
        let result = fetch::get(base, lang, &have, progress).and_then(|reply| match reply {
            Reply::Unchanged => {
                let songs = index.as_ref().map_or(0, |index| index.entries.len());
                if index.is_some() {
                    self.note_checked(lang, now, None)?;
                }
                Ok(Outcome::Unchanged { songs })
            }
            Reply::Pack { bytes, etag, last_modified } => {
                self.install(lang, &bytes, etag, last_modified, now).map(Outcome::Installed)
            }
        });
        if let Err(problem) = &result
            && index.is_some()
        {
            // Kept in English whatever the window speaks: `sp catalog` reads it back later.
            let _ = self.note_checked(lang, now, Some(crate::i18n::english_of(problem)));
        }
        result
    }

    /// The downloaded languages that are due to be asked about: a month after the last check, or
    /// a day after one that failed.
    pub fn due(&self, now: i64) -> Vec<String> {
        self.installed()
            .into_iter()
            .filter(|lang| {
                let status = self.status(lang);
                let wait = if status.problem.is_some() { RETRY_AFTER } else { CHECK_EVERY };
                now.saturating_sub(status.checked) >= wait
            })
            .collect()
    }

    /// Asks about each of `langs` in turn (see [`Catalog::download`]).
    pub fn check(&self, langs: &[String], now: i64) -> Vec<(String, Result<Outcome, String>)> {
        langs.iter().map(|lang| (lang.clone(), self.download(lang, now, None))).collect()
    }

    /// A song of the catalog made into the app's own, for opening or joining the library. `second`
    /// is the language of the version to put under it.
    pub fn present(&self, key: &str, second: Option<&str>, now: i64) -> Result<Presented, String> {
        let (lang, _, song) = self.song(key)?;
        Ok(present::present(&lang, key, &song, second, now))
    }

    /// What the catalog has now for a song a copy was made from.
    pub fn source_of(&self, lang: &str, key: &str) -> Source {
        match self.index(lang) {
            Ok(Some(index)) => match index.entry(key) {
                Some(entry) => Source::Has(entry.hash.clone()),
                None => Source::Gone,
            },
            _ => Source::Unknown,
        }
    }

    /// What an update does about each copy in the library: the places of the songs that are
    /// copies, and the verdict for each that is not [`Verdict::Current`].
    pub fn plan<'a>(
        &self,
        songs: impl IntoIterator<Item = (usize, &'a sp_model::Presentation)>,
    ) -> Vec<(usize, Verdict)> {
        songs
            .into_iter()
            .filter_map(|(at, song)| {
                let origin = song.catalog.as_ref().filter(|origin| origin.source == SOURCE)?;
                let verdict = judge(song, &self.source_of(&origin.language, &origin.key));
                (verdict != Verdict::Current).then_some((at, verdict))
            })
            .collect()
    }

    /// The catalog's index of a language, for searching without the catalog's lock.
    pub fn indexes(&self) -> Vec<Arc<LangIndex>> {
        self.installed().iter().filter_map(|lang| self.index(lang).ok().flatten()).collect()
    }

    /// Songs of a language in title order, `count` from `from`: for listing what the catalog
    /// has. Hits carry what the find box's do.
    pub fn list(&self, lang: &str, from: usize, count: usize) -> Vec<Hit> {
        let Ok(Some(index)) = self.index(lang) else { return Vec::new() };
        index
            .entries
            .iter()
            .skip(from)
            .take(count)
            .map(|entry| Hit {
                key: entry.key.clone(),
                lang: lang.to_owned(),
                title: entry.title.clone(),
                line: entry.line.clone(),
                in_title: true,
                versions: entry.langs.clone(),
                chords: entry.chords,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
