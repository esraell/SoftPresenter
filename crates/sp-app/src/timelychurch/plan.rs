//! A TimelyChurch setlist or service plan made into a playlist.
//!
//! First into what TimelyChurch asks for ([`PlannedService`]): its songs by TimelyChurch's ids,
//! headings, places kept for what isn't a song. Then, once the songs are in the library, into
//! the playlist itself ([`to_service`]), with each song found by the key the library knows it
//! by. A song that isn't in the library (it couldn't be fetched) keeps its place as a
//! placeholder with its title, so nothing moves when it arrives.
//!
//! A setlist and a plan are for the same service when they are for the same calendar event.
//! Then the plan gives the running order and the setlist its songs: they go where the plan keeps
//! a place for worship (a `worship` item, by its label when the setlist's songs carry labels),
//! else into the plan's empty song places in turn; any left over follow the last of them.

use serde::Serialize;

use super::wire::{PlanItem, ServicePlan, Setlist, SetlistSong};
use crate::service::{Item, Service};

/// One thing a TimelyChurch service asks for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Planned {
    /// A song, by TimelyChurch's id, in the arrangement the setlist names (none for the song's
    /// default), with its title for while it isn't in the library.
    Song { song: u64, arrangement: Option<u64>, title: String },
    /// A heading over the items under it: a part of the service ("Worship", "Communion").
    Header { name: String },
    /// A place kept for something that isn't a song (the sermon, the offering, a song not
    /// chosen yet), by its name.
    Placeholder { name: String },
    /// A reading, with the passage when the plan says which.
    Scripture { title: String, reference: Option<String> },
}

/// A TimelyChurch service, as it asks for its items.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlannedService {
    pub name: String,
    /// The service's date, `YYYY-MM-DD`.
    pub date: Option<String>,
    pub items: Vec<Planned>,
}

impl PlannedService {
    /// A fingerprint of what TimelyChurch asks for: it changes when TimelyChurch's setlist or
    /// plan changes in a way the playlist would show.
    pub fn fingerprint(&self) -> String {
        super::fingerprint_of(self)
    }
}

/// A setlist's songs in order, under a heading for each part of the service they are labelled
/// with.
pub fn from_setlist(setlist: &Setlist) -> PlannedService {
    let mut items = Vec::new();
    let mut part: Option<String> = None;
    for song in in_order(&setlist.songs) {
        if let Some(label) = label(song.worship_spot.as_deref())
            && part.as_deref() != Some(label.as_str())
        {
            items.push(Planned::Header { name: label.clone() });
            part = Some(label);
        }
        items.push(planned_song(song));
    }
    PlannedService {
        name: name_or_date(&setlist.name, setlist.event_date.as_deref()),
        date: setlist.event_date.clone(),
        items,
    }
}

/// The setlist for the same service as a plan: for the same calendar event. (Two for one
/// service on the same day but for different events are not taken to be the same.)
pub fn linked_setlist<'a>(plan: &ServicePlan, setlists: &'a [Setlist]) -> Option<&'a Setlist> {
    let event = plan.event_id?;
    setlists.iter().find(|setlist| setlist.linked_event_id == Some(event))
}

/// A plan's running order, with its setlist's songs in the places the plan keeps for them.
pub fn from_plan(plan: &ServicePlan, setlist: Option<&Setlist>) -> PlannedService {
    let mut left: Vec<&SetlistSong> = setlist.map(|setlist| in_order(&setlist.songs)).unwrap_or_default();
    let labelled = left.iter().any(|song| label(song.worship_spot.as_deref()).is_some());
    let mut ordered: Vec<&PlanItem> = plan.items.iter().collect();
    ordered.sort_by_key(|item| item.order);
    let has_worship = ordered.iter().any(|item| item.kind == "worship");
    let mut first_worship = true;
    let mut items = Vec::new();
    // Where songs went last, so the setlist's songs left over can follow them.
    let mut after_songs: Option<usize> = None;
    for item in ordered {
        let title = super::plain(&item.title);
        let title = if title.is_empty() { word_for(&item.kind) } else { title };
        match item.kind.as_str() {
            "worship" => {
                let name = label(item.worship_spot.as_deref()).unwrap_or_else(|| title.clone());
                items.push(Planned::Header { name: name.clone() });
                let here: Vec<&SetlistSong> = if labelled {
                    let (here, rest): (Vec<&SetlistSong>, Vec<&SetlistSong>) = left
                        .into_iter()
                        .partition(|song| label(song.worship_spot.as_deref()).is_some_and(|l| same(&l, &name)));
                    left = rest;
                    here
                } else if first_worship {
                    std::mem::take(&mut left)
                } else {
                    Vec::new()
                };
                first_worship = false;
                items.extend(here.into_iter().map(planned_song));
                after_songs = Some(items.len());
            }
            "song" => {
                let planned = match item.song_id {
                    Some(song) => Planned::Song {
                        song,
                        arrangement: None,
                        title: item.song_title.as_deref().map(super::plain).filter(|t| !t.is_empty()).unwrap_or(title),
                    },
                    None if !has_worship && !left.is_empty() => planned_song(left.remove(0)),
                    None => Planned::Placeholder { name: title },
                };
                items.push(planned);
                after_songs = Some(items.len());
            }
            "section" => items.push(Planned::Header { name: title }),
            "scripture" => items.push(Planned::Scripture {
                title,
                reference: item.scripture_reference.as_deref().map(super::plain).filter(|r| !r.is_empty()),
            }),
            _ => items.push(Planned::Placeholder { name: title }),
        }
    }
    let at = after_songs.unwrap_or(items.len());
    for (n, song) in left.into_iter().enumerate() {
        items.insert(at + n, planned_song(song));
    }
    PlannedService { name: name_or_date(&plan.title, plan.date.as_deref()), date: plan.date.clone(), items }
}

/// Upcoming services with the same name (every week's "Sunday Service") told apart by their
/// dates: "Sunday Service (2026-12-06)". A name used once stays as the church wrote it.
pub fn distinct_names(services: &mut [PlannedService]) {
    let names: Vec<String> = services.iter().map(|service| service.name.to_lowercase()).collect();
    for (at, service) in services.iter_mut().enumerate() {
        let repeated = names.iter().enumerate().any(|(other, name)| other != at && *name == names[at]);
        if let (true, Some(date)) = (repeated, service.date.as_deref()) {
            service.name = format!("{} ({date})", service.name);
        }
    }
}

/// The playlist itself. `find` gives the library key of the song made from a TimelyChurch song
/// in an arrangement (none: its default), or `None` when it isn't in the library yet. A
/// reading is a placeholder named with its passage until TimelyChurch's way of writing a
/// passage is known. A song comes once, as in any playlist.
pub fn to_service(planned: &PlannedService, mut find: impl FnMut(u64, Option<u64>) -> Option<String>) -> Service {
    let mut items: Vec<Item> = Vec::new();
    for wanted in &planned.items {
        let item = match wanted {
            Planned::Song { song, arrangement, title } => match find(*song, *arrangement) {
                Some(key) => Item::song(key, title.clone()),
                None => Item::Placeholder { name: title.clone() },
            },
            Planned::Header { name } => Item::Header { name: name.clone() },
            Planned::Placeholder { name } => Item::Placeholder { name: name.clone() },
            Planned::Scripture { title, reference: Some(reference) } if !title.contains(reference.as_str()) => {
                Item::Placeholder { name: format!("{title}: {reference}") }
            }
            Planned::Scripture { title, .. } => Item::Placeholder { name: title.clone() },
        };
        if !items.iter().any(|have| have.same_as(&item)) {
            items.push(item);
        }
    }
    Service { name: planned.name.clone(), items, uids: Default::default() }
}

/// A setlist's songs in their order.
fn in_order(songs: &[SetlistSong]) -> Vec<&SetlistSong> {
    let mut songs: Vec<&SetlistSong> = songs.iter().collect();
    songs.sort_by_key(|song| song.order);
    songs
}

fn planned_song(song: &SetlistSong) -> Planned {
    Planned::Song { song: song.song_id, arrangement: song.arrangement_id(), title: super::plain(&song.title) }
}

/// A part-of-the-service label, tidied; `None` when there is none.
fn label(text: Option<&str>) -> Option<String> {
    text.map(super::plain).filter(|label| !label.is_empty())
}

fn same(one: &str, other: &str) -> bool {
    one.trim().to_lowercase() == other.trim().to_lowercase()
}

/// The name a service is shown by: its own, or its date when it has none.
fn name_or_date(name: &str, date: Option<&str>) -> String {
    let name = super::plain(name);
    if name.is_empty() { date.map_or_else(|| super::SOURCE_NAME.to_owned(), str::to_owned) } else { name }
}

/// What to call an item with no title, by its kind.
fn word_for(kind: &str) -> String {
    let word = match kind {
        "song" | "worship" => "Song",
        "scripture" => "Reading",
        "prayer" => "Prayer",
        "sermon" => "Sermon",
        "offering" => "Offering",
        "announcement" => "Announcements",
        _ => "Item",
    };
    word.to_owned()
}
