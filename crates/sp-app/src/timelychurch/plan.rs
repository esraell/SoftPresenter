//! A TimelyChurch service (a setlist or a service plan) made into a playlist.
//!
//! First into what TimelyChurch asks for ([`PlannedService`]): its songs by TimelyChurch's ids,
//! headings, places kept for what isn't a song. Then, once the songs are in the library, into
//! the playlist itself ([`to_service`]), with each song found by the key the library knows it
//! by. A song that isn't in the library (it couldn't be fetched, or TimelyChurch no longer has
//! it) keeps its place as a placeholder with its title, so nothing moves when it arrives.
//!
//! A setlist and a plan are for the same service when they are for the same calendar event (if
//! TimelyChurch says which event each is for). Then the plan gives the running order and the
//! setlist its songs: they go where the plan keeps a place for worship (a `worship` item, by its
//! label when the setlist's songs carry labels), else into the plan's empty song places in
//! turn; any left over follow the last of them. The setlist is then not a playlist of its own.

use serde::Serialize;

use super::wire::{Id, Item as WireItem, Service as WireService};
use crate::i18n::{tg, tgf};
use crate::service::{Item, Service};

/// One thing a TimelyChurch service asks for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Planned {
    /// A song, by TimelyChurch's id, in the arrangement the service names (none for the song's
    /// default), with its title for while it isn't in the library, and the key it is sung in.
    Song { song: Id, arrangement: Option<Id>, title: String, key: Option<String> },
    /// A heading over the items under it: a part of the service ("Worship", "Communion").
    Header { name: String },
    /// A place kept for something that isn't a song (the sermon, the offering, a song not
    /// chosen yet), by its name.
    Placeholder { name: String },
    /// A reading, with the passage when the service says which.
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

/// The services that become playlists, each with the setlist joined to it: every plan (with
/// the setlist for the same event, if there is one), and every setlist no plan took.
pub fn to_follow(services: &[WireService]) -> Vec<(&WireService, Option<&WireService>)> {
    let joined: Vec<(&WireService, Option<&WireService>)> = services
        .iter()
        .filter(|service| !service.is_setlist())
        .map(|plan| (plan, linked_setlist(plan, services)))
        .collect();
    let taken: Vec<&Id> = joined.iter().filter_map(|(_, setlist)| setlist.map(|s| &s.id)).collect();
    let mut follow: Vec<(&WireService, Option<&WireService>)> = Vec::new();
    for service in services {
        if service.is_setlist() {
            if !taken.contains(&&service.id) {
                follow.push((service, None));
            }
        } else if let Some(pair) = joined.iter().find(|(plan, _)| std::ptr::eq(*plan, service)) {
            follow.push(*pair);
        }
    }
    follow
}

/// What a service asks for: a setlist's songs, or a plan's running order with its setlist's
/// songs in it.
pub fn from_service(service: &WireService, setlist: Option<&WireService>) -> PlannedService {
    if service.is_setlist() { from_setlist(service) } else { from_plan(service, setlist) }
}

/// The setlist for the same service as a plan: for the same calendar event. (Two for one
/// service on the same day but for different events are not taken to be the same.)
pub fn linked_setlist<'a>(plan: &WireService, services: &'a [WireService]) -> Option<&'a WireService> {
    let event = plan.event_id.as_ref().filter(|event| !event.is_empty())?;
    services.iter().find(|service| service.is_setlist() && service.event_id.as_ref() == Some(event))
}

/// A setlist's songs in order, under a heading for each part of the service they are labelled
/// with; anything else it holds in its place.
pub fn from_setlist(setlist: &WireService) -> PlannedService {
    let mut items = Vec::new();
    let mut part: Option<String> = None;
    for item in in_order(&setlist.items) {
        if !item.is_song() {
            items.push(other(item));
            continue;
        }
        if let Some(label) = label(item.worship_spot.as_deref())
            && part.as_deref() != Some(label.as_str())
        {
            items.push(Planned::Header { name: label.clone() });
            part = Some(label);
        }
        items.push(planned_song(item));
    }
    PlannedService { name: name_or_date(&setlist.name, setlist.date.as_deref()), date: setlist.date.clone(), items }
}

/// A plan's running order, with its setlist's songs in the places the plan keeps for them.
pub fn from_plan(plan: &WireService, setlist: Option<&WireService>) -> PlannedService {
    let mut left: Vec<&WireItem> = setlist
        .map(|setlist| in_order(&setlist.items).into_iter().filter(|item| item.is_song()).collect())
        .unwrap_or_default();
    let labelled = left.iter().any(|song| label(song.worship_spot.as_deref()).is_some());
    let ordered = in_order(&plan.items);
    let has_worship = ordered.iter().any(|item| item.kind.eq_ignore_ascii_case("worship"));
    let mut first_worship = true;
    let mut items = Vec::new();
    // Where songs went last, so the setlist's songs left over can follow them.
    let mut after_songs: Option<usize> = None;
    for item in ordered {
        if item.kind.eq_ignore_ascii_case("worship") {
            let title = title_of(item);
            let name = label(item.worship_spot.as_deref()).unwrap_or(title);
            items.push(Planned::Header { name: name.clone() });
            let here: Vec<&WireItem> = if labelled {
                let (here, rest): (Vec<&WireItem>, Vec<&WireItem>) = left
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
        } else if item.is_song() {
            let planned = match &item.song_id {
                Some(song) if !song.is_empty() => planned_song(item),
                _ if !has_worship && !left.is_empty() => planned_song(left.remove(0)),
                _ => Planned::Placeholder { name: title_of(item) },
            };
            items.push(planned);
            after_songs = Some(items.len());
        } else {
            items.push(other(item));
        }
    }
    let at = after_songs.unwrap_or(items.len());
    for (n, song) in left.into_iter().enumerate() {
        items.insert(at + n, planned_song(song));
    }
    PlannedService { name: name_or_date(&plan.name, plan.date.as_deref()), date: plan.date.clone(), items }
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
/// in an arrangement (none: its default), or `None` when it isn't in the library. A reading is
/// a placeholder named with its passage until TimelyChurch's way of writing a passage is
/// known. A song comes once, as in any playlist.
pub fn to_service(planned: &PlannedService, mut find: impl FnMut(&Id, Option<&Id>) -> Option<String>) -> Service {
    let mut items: Vec<Item> = Vec::new();
    for wanted in &planned.items {
        let item = match wanted {
            Planned::Song { song, arrangement, title, .. } => match find(song, arrangement.as_ref()) {
                Some(key) => Item::song(key, title.clone()),
                None => Item::Placeholder { name: title.clone() },
            },
            Planned::Header { name } => Item::Header { name: name.clone() },
            Planned::Placeholder { name } => Item::Placeholder { name: name.clone() },
            Planned::Scripture { title, reference: Some(reference) } if !title.contains(reference.as_str()) => {
                Item::Placeholder {
                    name: tgf(
                        "left.timelychurch.reading_of",
                        "{title}: {passage}",
                        &[("title", title), ("passage", reference)],
                    ),
                }
            }
            Planned::Scripture { title, .. } => Item::Placeholder { name: title.clone() },
        };
        if !items.iter().any(|have| have.same_as(&item)) {
            items.push(item);
        }
    }
    Service { name: planned.name.clone(), items, uids: Default::default(), origin: None }
}

/// Items in their order (items sent without places stay in the order sent).
fn in_order(items: &[WireItem]) -> Vec<&WireItem> {
    let mut items: Vec<&WireItem> = items.iter().collect();
    items.sort_by_key(|item| item.position);
    items
}

fn planned_song(item: &WireItem) -> Planned {
    let title = title_of(item);
    match &item.song_id {
        Some(song) if song.is_safe() => Planned::Song {
            song: song.clone(),
            arrangement: item.arrangement_id.clone().filter(Id::is_safe),
            title,
            key: item.key.as_deref().map(super::plain).filter(|key| !key.is_empty()),
        },
        _ => Planned::Placeholder { name: title },
    }
}

/// An item that isn't a song: a heading, a reading, or a placeholder named for it.
fn other(item: &WireItem) -> Planned {
    let title = title_of(item);
    match item.kind.to_lowercase().as_str() {
        "heading" | "header" | "section" => Planned::Header { name: title },
        "reading" | "scripture" => Planned::Scripture {
            title,
            reference: item.reference.as_deref().map(super::plain).filter(|r| !r.is_empty()),
        },
        _ => Planned::Placeholder { name: title },
    }
}

/// An item's title, or a word for its kind when it has none.
fn title_of(item: &WireItem) -> String {
    let title = super::plain(&item.title);
    if !title.is_empty() {
        return title;
    }
    // Names the app gives and the playlist then keeps: in the language set.
    let word = match item.kind.to_lowercase().as_str() {
        "song" | "worship" => tg("left.timelychurch.song", "Song"),
        "reading" | "scripture" => tg("left.timelychurch.reading", "Reading"),
        "prayer" => tg("left.timelychurch.prayer", "Prayer"),
        "sermon" => tg("left.timelychurch.sermon", "Sermon"),
        "offering" => tg("left.timelychurch.offering", "Offering"),
        "announcement" | "announcements" => tg("left.timelychurch.announcements", "Announcements"),
        _ => tg("left.timelychurch.item", "Item"),
    };
    word.to_owned()
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
