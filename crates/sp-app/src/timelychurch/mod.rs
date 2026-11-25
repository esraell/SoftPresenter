//! TimelyChurch: the church-management app a church may plan its services in. Its worship side
//! keeps the church's songs, setlists (the songs a team sings at a service, in order) and service
//! plans (the whole running order). SoftPresenter connects to it as a device, through its
//! read-only Worship API, turns the church's upcoming services into playlists and their songs
//! into library songs, keeps them in step, and tells it which songs were shown, for the church's
//! song use report. `docs/timelychurch.md` has the whole design.
//!
//! The parts: `wire` (the API's requests and replies), `song` (a song made into the app's song),
//! `plan` (a service made into a playlist), `client` (the requests), `token` (where the device
//! token is kept), `store` (what is kept on disk), `sync` (the connection, and the checks and
//! sends on threads of their own), `app` (putting what a check found into the library and the
//! playlists, and counting songs shown), `view` (the card's state; it is drawn in
//! `integrations`), `cli` (`sp timelychurch`).
//!
//! **Hidden until TimelyChurch's API ships:** nothing is offered, and nothing is ever asked of
//! the network, unless `SOFTPRESENTER_TIMELYCHURCH_URL` is set ([`gate`]).
//!
//! A song made from TimelyChurch carries the same mark as a song from the song catalog
//! ([`sp_model::CatalogOrigin`], with the source `timelychurch`), so the catalog's rules decide
//! what an update does: a song nobody has touched is brought up to date, and a song the church
//! changed in SoftPresenter is never overwritten (a newer version is offered).

mod app;
mod cli;
mod client;
#[cfg(test)]
pub(crate) mod mock;
mod plan;
mod song;
mod store;
mod sync;
mod token;
mod view;
mod wire;

pub use plan::{PlannedService, distinct_names, from_service, to_follow, to_service};
pub use song::{fresh_source, make_song};
pub use store::Files;
pub use sync::{Connection, Status};
#[cfg(test)]
pub use view::ServiceRow;
pub use view::{Request, RowState, View};
pub use wire::{Id, Service};

use sha2::{Digest, Sha256};

/// The address TimelyChurch is offered with, while it is offered only for trying (until its
/// Worship API ships): with none, there is no card, and nothing is ever asked of the network or
/// the secure store. Read once, as the app starts.
pub fn gate() -> Option<String> {
    // Testing aid: SOFTPRESENTER_TIMELYCHURCH_URL=<base address> offers Settings › Integrations
    // › TimelyChurch and `sp timelychurch`, with that address suggested for connecting (a test
    // stand-in on this computer may be plain http, as `mock` is).
    let base = std::env::var("SOFTPRESENTER_TIMELYCHURCH_URL").ok()?;
    let base = base.trim().trim_end_matches('/');
    (!base.is_empty()).then(|| base.to_owned())
}

/// The source's name in a song's mark and in the keys.
pub const SOURCE: &str = "timelychurch";
/// What the app calls it.
pub const SOURCE_NAME: &str = "TimelyChurch";

/// The key a song made from TimelyChurch is known by in its mark: the church (see
/// [`church_part`]), the song's id and, when TimelyChurch names it, the arrangement it was made
/// in. Ids are TimelyChurch's own, unique in one church only.
pub fn song_key(church: &str, song: &Id, arrangement: Option<&Id>) -> String {
    match arrangement {
        Some(arrangement) => format!("{}/song/{song}/arrangement/{arrangement}", church_part(church)),
        None => format!("{}/song/{song}", church_part(church)),
    }
}

/// The church, song and arrangement a song's key names.
pub fn parse_song_key(key: &str) -> Option<(String, Id, Option<Id>)> {
    let (church, rest) = key.rsplit_once("/song/")?;
    let (song, arrangement) = match rest.split_once("/arrangement/") {
        Some((song, arrangement)) => (Id::new(song), Some(Id::new(arrangement))),
        None => (Id::new(rest), None),
    };
    (song.is_safe() && arrangement.as_ref().is_none_or(Id::is_safe)).then(|| (church.to_owned(), song, arrangement))
}

/// What a playlist was made from: a setlist or a service plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Made {
    Setlist(Id),
    Plan(Id),
}

impl Made {
    pub fn of(service: &Service) -> Made {
        if service.is_setlist() { Made::Setlist(service.id.clone()) } else { Made::Plan(service.id.clone()) }
    }
}

/// The key a playlist made from TimelyChurch is known by: the church and the setlist or plan.
pub fn service_key(church: &str, from: &Made) -> String {
    match from {
        Made::Setlist(id) => format!("{}/setlist/{id}", church_part(church)),
        Made::Plan(id) => format!("{}/plan/{id}", church_part(church)),
    }
}

/// The church's address as a key holds it: lower case, without `https://` or a slash at the end.
pub fn church_part(church: &str) -> String {
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
