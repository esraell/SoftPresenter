//! Making a catalog song into the app's own song, and telling later what happened to it.
//!
//! A section becomes a group named as the pack names it ("Verse 1", "Chorus 1"…), a blank line
//! inside a section starts another slide, and a section marked as capitalised has each line
//! start with a capital. A version in another language is lined up with the song **group by
//! group, by the group's name** (Verse 1 with Verse 1; a name used twice, the first with the
//! first): where both have the group, the version's words go under the song's as the
//! translation; a group the version has no partner for shows one language, and is said so
//! ([`Pairing::only_main`]); a group only the version has is left out ([`Pairing::only_second`]).
//!
//! A copy in the library carries a [`CatalogOrigin`] with two fingerprints: of the catalog's text
//! it was made from, and of the whole song as the catalog last wrote it. While the song still
//! has the second, nobody has changed it, and an update may replace it; any edit (words, title,
//! details, theme, arrangements, a slide's look) gives another, and then a newer version is only
//! offered.

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use sp_model::{Arrangement, CatalogOrigin, Copyright, Group, Object, Presentation, Role, Slide, Uuid, group_color};

use super::fold::fold;
use super::read::{self, Section, Song};
use super::store::date_of;

/// The credit a catalog song carries, for a window to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Credit {
    pub text: &'static str,
    pub site: &'static str,
}

/// "From Worship Leader App", and where it is.
pub const CREDIT: Credit = Credit { text: "From Worship Leader App", site: super::SOURCE_SITE };

/// How a song's version in another language lined up with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pairing {
    /// The language of the version put under the song's words, if there was one.
    pub second: Option<String>,
    /// Groups of the song the version has no partner for: each shows one language.
    pub only_main: Vec<String>,
    /// Groups only the version has, which are left out.
    pub only_second: Vec<String>,
}

/// A catalog song made into the app's song.
#[derive(Clone, Debug)]
pub struct Presented {
    pub song: Presentation,
    pub pairing: Pairing,
    pub credit: Credit,
}

/// The slides of a section: its lines, each starting with a capital when the section says so,
/// split at blank lines.
fn chunks(section: &Section) -> Vec<String> {
    let mut slides = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for line in section.text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !lines.is_empty() {
                slides.push(lines.join("\n"));
                lines.clear();
            }
            continue;
        }
        lines.push(if section.capitalise { capitalised(line) } else { line.to_owned() });
    }
    slides
}

/// A line with its first letter in capitals.
fn capitalised(line: &str) -> String {
    let at = line.find(char::is_alphabetic);
    match at {
        Some(at) => {
            let mut letters = line[at..].chars();
            let first: String = letters.next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            format!("{}{first}{}", &line[..at], letters.as_str())
        }
        None => line.to_owned(),
    }
}

/// Makes a catalog song into the app's own song. `second` names the language of the version to
/// put under it, if the song has one in that language (nothing happens if it has not).
pub fn present(lang: &str, key: &str, song: &Song, second: Option<&str>, now: i64) -> Presented {
    let version = second.and_then(|want| song.versions.iter().find(|v| v.lang == want));
    let mut pairing = Pairing { second: version.map(|v| v.lang.clone()), ..Pairing::default() };
    let mut used = vec![false; version.map_or(0, |v| v.sections.len())];
    let mut groups = Vec::new();
    for section in &song.sections {
        let name = if section.title.is_empty() { "Slide".to_owned() } else { section.title.clone() };
        let mine = chunks(section);
        if mine.is_empty() {
            continue;
        }
        // The version's group with this name, the first one not taken yet.
        let partner = version.and_then(|v| {
            let at = v
                .sections
                .iter()
                .enumerate()
                .position(|(at, other)| !used[at] && fold(&other.title) == fold(&section.title))?;
            used[at] = true;
            Some(&v.sections[at])
        });
        if version.is_some() && partner.is_none() {
            pairing.only_main.push(name.clone());
        }
        let theirs = partner.map(chunks).unwrap_or_default();
        let last = mine.len() - 1;
        let slides = mine
            .into_iter()
            .enumerate()
            .map(|(at, words)| {
                let mut objects = vec![Object::text(Role::primary(), words)];
                // The slides pair one to one; what the version has over goes with the last.
                let below =
                    if at == last { theirs.get(at..).map(|rest| rest.join("\n")) } else { theirs.get(at).cloned() };
                if let Some(below) = below.filter(|text| !text.is_empty()) {
                    objects.push(Object::text(Role::translation(), below));
                }
                Slide::new(objects)
            })
            .collect();
        groups.push(Group { id: Uuid::new_v4(), color: group_color(&name), name, slides });
    }
    if let Some(version) = version {
        for (at, section) in version.sections.iter().enumerate() {
            if !used[at] && !chunks(section).is_empty() {
                pairing.only_second.push(if section.title.is_empty() {
                    "Slide".to_owned()
                } else {
                    section.title.clone()
                });
            }
        }
    }
    let mut made = Presentation {
        id: Uuid::new_v4(),
        title: song.title.clone(),
        groups,
        copyright: Copyright {
            author: song.author.clone(),
            publisher: song.copyright.clone(),
            year: song.year.clone(),
            ..Copyright::default()
        },
        theme: None,
        arrangements: Vec::new(),
        arrangement: None,
        loops: false,
        transition: None,
        catalog: None,
    };
    let made_hash = fingerprint(&made);
    made.catalog = Some(Box::new(CatalogOrigin {
        source: super::SOURCE.to_owned(),
        key: key.to_owned(),
        language: lang.to_owned(),
        second: pairing.second.clone(),
        text_hash: read::text_hash(song),
        made_hash,
        added: date_of(now),
        newer: None,
        gone: false,
    }));
    Presented { song: made, pairing, credit: CREDIT }
}

/// A fingerprint of a song as a person sees and has changed it: the whole song with every id
/// numbered afresh in order and its catalog mark left out, so two copies of one text have one
/// fingerprint and any edit gives another. 16 hex letters of a SHA-256.
pub fn fingerprint(song: &Presentation) -> String {
    let mut plain = song.clone();
    plain.catalog = None;
    plain.id = Uuid::nil();
    let mut next = 0_u128;
    let mut fresh = || {
        next += 1;
        Uuid::from_u128(next)
    };
    let mut groups: HashMap<Uuid, Uuid> = HashMap::new();
    for group in &mut plain.groups {
        let new = fresh();
        groups.insert(group.id, new);
        group.id = new;
        for slide in &mut group.slides {
            slide.id = fresh();
            for object in &mut slide.objects {
                object.id = fresh();
            }
        }
    }
    let mut arrangements: HashMap<Uuid, Uuid> = HashMap::new();
    for arrangement in &mut plain.arrangements {
        let new = fresh();
        arrangements.insert(arrangement.id, new);
        let Arrangement { id, groups: order, .. } = arrangement;
        *id = new;
        for group in order {
            *group = groups.get(group).copied().unwrap_or(*group);
        }
    }
    plain.arrangement = plain.arrangement.map(|id| arrangements.get(&id).copied().unwrap_or(id));
    let json = sp_model::to_json(&plain);
    Sha256::digest(json.as_bytes())[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// What the catalog has for a copy's song now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The song is there, with this text fingerprint.
    Has(String),
    /// The language is here and the song is not in it.
    Gone,
    /// The catalog can't say (the language isn't downloaded).
    Unknown,
}

/// What an update does about one copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing to do.
    Current,
    /// The song is unchanged and the catalog has newer text: replace it.
    Update,
    /// The church changed the song and the catalog has newer text: offer it, keep the song.
    Offer(String),
    /// The catalog dropped the song: note it on the copy, which stays.
    Gone,
    /// The marks on the copy are out of date (an offer the catalog took back, a song that came
    /// back): put right.
    Tidy,
}

/// What an update should do about a copy, from what the catalog has now.
pub fn judge(song: &Presentation, source: &Source) -> Verdict {
    let Some(origin) = &song.catalog else { return Verdict::Current };
    match source {
        Source::Unknown => Verdict::Current,
        Source::Gone if origin.gone => Verdict::Current,
        Source::Gone => Verdict::Gone,
        Source::Has(hash) if *hash == origin.text_hash => {
            if origin.gone || origin.newer.is_some() {
                Verdict::Tidy
            } else {
                Verdict::Current
            }
        }
        Source::Has(hash) => {
            if fingerprint(song) == origin.made_hash {
                Verdict::Update
            } else if origin.newer.as_deref() == Some(hash.as_str()) {
                Verdict::Current
            } else {
                Verdict::Offer(hash.clone())
            }
        }
    }
}

/// The song after a verdict that only changes its marks (an offer, a song gone, tidying).
pub fn marked(song: &Presentation, verdict: &Verdict) -> Option<Presentation> {
    let mut out = song.clone();
    let origin = out.catalog.as_mut()?;
    match verdict {
        Verdict::Offer(hash) => origin.newer = Some(hash.clone()),
        Verdict::Gone => origin.gone = true,
        Verdict::Tidy => {
            origin.gone = false;
            origin.newer = None;
        }
        Verdict::Current | Verdict::Update => return None,
    }
    Some(out)
}

/// The song after taking the catalog's newer version: its words and groups made again from
/// `fresh` (the same song, made now, with the same second language), keeping the copy's id, its
/// theme, its transition and whether it loops. The result is the catalog's to the letter, so
/// it is unchanged again.
pub fn refreshed(old: &Presentation, fresh: Presented) -> Presentation {
    refreshed_song(old, fresh.song)
}

/// [`refreshed`] for any song made from an outside source and marked with where it came from
/// (the catalog, TimelyChurch): `fresh` is the same song made again now.
pub fn refreshed_song(old: &Presentation, fresh: Presentation) -> Presentation {
    let mut song = fresh;
    song.id = old.id;
    song.theme = old.theme;
    song.transition = old.transition;
    song.loops = old.loops;
    let made_hash = fingerprint(&song);
    if let (Some(mark), Some(was)) = (song.catalog.as_mut(), &old.catalog) {
        mark.added.clone_from(&was.added);
        mark.made_hash = made_hash;
    }
    song
}

/// "Keep mine": the church has seen the newer version and keeps its own. The song stays
/// changed, so it is never replaced by itself, and the same newer version is not offered again.
pub fn kept(song: &Presentation) -> Option<Presentation> {
    let mut out = song.clone();
    let origin = out.catalog.as_mut()?;
    let newer = origin.newer.take()?;
    origin.text_hash = newer;
    Some(out)
}

#[cfg(test)]
mod tests;
