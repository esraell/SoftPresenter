//! TimelyChurch: the church-management app a church may plan its services in. Its worship side
//! keeps the church's songs (words and chords as ChordPro, in named sections), setlists (the
//! songs a team sings at a service, in order) and service plans (the whole running order), and
//! pairs devices with a PIN ("Song Sync"). SoftPresenter is to connect as such a device, turn
//! the church's upcoming setlists and plans into playlists and their songs into library songs,
//! and keep them in step. `docs/timelychurch.md` has the whole design, and what waits on
//! TimelyChurch.
//!
//! This module is the part that doesn't depend on the device interface, which isn't published
//! yet: `wire` (TimelyChurch's data, shaped as its tools show it), `song` (a song, its chart and
//! its translation made into the app's song), `plan` (a setlist or a plan made into a playlist).
//! No network and no window here.
//!
//! A song made from TimelyChurch carries the same mark as a song from the song catalog
//! ([`sp_model::CatalogOrigin`], with the source `timelychurch`), so the catalog's rules decide
//! what an update does: a song nobody has touched is brought up to date, and a song the church
//! changed in SoftPresenter is never overwritten (a newer version is offered).

// Wired into the app (Settings › Integrations, the fetching, the library and playlists) once
// TimelyChurch's device interface is known; until then it is tested only.
#![allow(dead_code, unused_imports)]

mod plan;
mod song;
mod wire;

pub use plan::{Planned, PlannedService, distinct_names, from_plan, from_setlist, linked_setlist, to_service};
pub use song::{MadeSong, fresh_source, lines_per_slide, make_song, sections_from_text};
pub use wire::{
    Arrangement, Chart, ChartSection, PlanItem, ServicePlan, Setlist, SetlistSong, SlideStyle, Song, StyleReply,
    Translation, TranslationRow, TranslationSection,
};

use sha2::{Digest, Sha256};

/// The source's name in a song's mark and in the keys.
pub const SOURCE: &str = "timelychurch";
/// What the app calls it.
pub const SOURCE_NAME: &str = "TimelyChurch";

/// The key a song made from TimelyChurch is known by in its mark: the church (the address it
/// was connected with), the song's id and the arrangement whose chart it was made from. Ids
/// are TimelyChurch's own, unique in one church only.
pub fn song_key(church: &str, song: u64, arrangement: u64) -> String {
    format!("{}/song/{song}/arrangement/{arrangement}", church_part(church))
}

/// The church, song and arrangement a song's key names.
pub fn parse_song_key(key: &str) -> Option<(String, u64, u64)> {
    let (church, rest) = key.rsplit_once("/song/")?;
    let (song, arrangement) = rest.split_once("/arrangement/")?;
    Some((church.to_owned(), song.parse().ok()?, arrangement.parse().ok()?))
}

/// What a playlist was made from: a setlist or a service plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
    Setlist(u64),
    Plan(u64),
}

/// The key a playlist made from TimelyChurch is known by: the church and the setlist or plan.
pub fn service_key(church: &str, from: Made) -> String {
    match from {
        Made::Setlist(id) => format!("{}/setlist/{id}", church_part(church)),
        Made::Plan(id) => format!("{}/plan/{id}", church_part(church)),
    }
}

/// The church's address as a key holds it: lower case, without `https://` or a slash at the end.
fn church_part(church: &str) -> String {
    let church = church.trim();
    let church = church.strip_prefix("https://").or_else(|| church.strip_prefix("http://")).unwrap_or(church);
    church.trim_end_matches('/').to_lowercase()
}

/// 16 hex letters of the SHA-256 of a value's JSON: a fingerprint that changes when the value
/// does, the same on every computer.
fn fingerprint_of(value: &impl serde::Serialize) -> String {
    let json = serde_json::to_string(value).unwrap_or_default();
    Sha256::digest(json.as_bytes())[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// A name or title as TimelyChurch sends it, with the few HTML escapes some of its titles carry
/// (`&amp;` for `&`) turned back into their letters, and the spaces at the ends taken off.
fn plain(text: &str) -> String {
    if !text.contains('&') {
        return text.trim().to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
            let letter = match &rest[1..end] {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                code => code
                    .strip_prefix("#x")
                    .or_else(|| code.strip_prefix("#X"))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| code.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((letter, end))
        });
        match decoded {
            Some((letter, end)) => {
                out.push(letter);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

#[cfg(test)]
mod tests;
