//! A TimelyChurch song made into the app's own song.
//!
//! TimelyChurch sends each section of a song once, with its name and lines, and the order the
//! arrangement sings them in as a list of names, a repeat being the name again. Each section
//! becomes a group named as the church named it; a blank line inside a section starts another
//! slide, and a stretch longer than the most lines a slide takes is shared out evenly, as songs
//! imported from text are. Chords in square brackets and repeat signs (`||:` `:||`), if a line
//! carries any, are taken out: they are for the musicians.
//!
//! The order sung becomes the song's arrangement, chosen to play, whenever it is not simply the
//! groups as written. A name in the order that names no section (an instrumental part with no
//! words) is no group, and is listed so the window can say so.
//!
//! A translation comes lined up with the song: its sections in the same places as the song's,
//! line for line, with `null` for a line nobody translated. Each slide gets a translation box
//! holding the translated lines of its own lines, in order. An untranslated line is left out of
//! the box and counted; a slide with none translated has no box; a translation with no line
//! translated at all puts nothing under the words.

use sp_model::{
    Arrangement, CatalogOrigin, Copyright, Group, Object, Presentation, Role, Slide, Uuid, group_color, share_out,
    without_chords, year_and_publisher,
};

use super::wire::{Section, Song, Translation, TranslationSection};
use crate::catalog::{self, Source};

/// A TimelyChurch song made into the app's song.
#[derive(Clone, Debug)]
pub struct MadeSong {
    pub song: Presentation,
    /// The language of the translation put under the words, if any line had one.
    pub second: Option<String>,
    /// Lines of words with no translated line, when a translation was put under the words.
    pub untranslated: usize,
    /// Names in the order sung that name no section with words, in order: nothing is sung for
    /// them.
    pub unmatched: Vec<String>,
}

/// The translation that goes under the words, when the church published any: the first, in
/// the order TimelyChurch sends them, that has a line translated. One rule, in one place, so
/// that a choice of language can take its place here later.
pub fn translation_for(song: &Song) -> Option<&Translation> {
    song.translations
        .iter()
        .find(|translation| translation.sections.iter().flat_map(|s| &s.lines).flatten().any(|l| !l.trim().is_empty()))
}

/// Makes a TimelyChurch song into the app's song. `church` is the church's part of the keys
/// (see [`super::church_part`]); `most_lines` is the most lines a slide takes; `now` (seconds
/// since 1970) is when it joins the library.
pub fn make_song(church: &str, song: &Song, most_lines: usize, now: i64) -> MadeSong {
    let translation = translation_for(song);
    let mut used = false;
    let mut missing = 0;
    let mut groups: Vec<Group> = Vec::new();
    for (place, section) in song.sections.iter().enumerate() {
        let below = translation.and_then(|translation| lined_up(section, place, &translation.sections));
        let mut slides = Vec::new();
        for stanza in stanzas(section, below) {
            let words: Vec<String> = stanza.iter().map(|(line, _)| line.clone()).collect();
            let mut at = 0;
            for piece in share_out(&words, most_lines) {
                let lines = &stanza[at..at + piece.len()];
                at += piece.len();
                let mut objects = vec![Object::text(Role::primary(), piece.join("\n"))];
                let translated: Vec<&str> = lines.iter().filter_map(|(_, below)| below.as_deref()).collect();
                missing += lines.len() - translated.len();
                if !translated.is_empty() {
                    used = true;
                    objects.push(Object::text(Role::translation(), translated.join("\n")));
                }
                slides.push(Slide::new(objects));
            }
        }
        if slides.is_empty() {
            continue;
        }
        let name = super::plain(&section.name);
        groups.push(Group { id: Uuid::new_v4(), color: group_color(&name), name, slides });
    }
    let (sung, unmatched) = order_sung(&groups, &song.order);
    let as_written: Vec<Uuid> = groups.iter().map(|group| group.id).collect();
    let arrangements = if sung.is_empty() || sung == as_written {
        Vec::new()
    } else {
        let name = song
            .arrangement
            .as_ref()
            .map(|a| super::plain(&a.name))
            .filter(|name| !name.is_empty() && !is_default_name(name))
            .unwrap_or_else(|| super::SOURCE_NAME.to_owned());
        vec![Arrangement { id: Uuid::new_v4(), name, groups: sung }]
    };
    let mut title = super::plain(&song.title);
    // A song sung in an arrangement that isn't its default is a song of the library of its own.
    if let Some(arrangement) = song.arrangement.as_ref().filter(|a| a.is_default == Some(false)) {
        let name = super::plain(&arrangement.name);
        if !name.is_empty() && !is_default_name(&name) {
            title = format!("{title} ({name})");
        }
    }
    let (year, publisher) = song.copyright.as_deref().map(year_and_publisher).unwrap_or_default();
    let authors: Vec<String> = song.authors.iter().map(|name| super::plain(name)).filter(|n| !n.is_empty()).collect();
    let second = translation.filter(|_| used).map(|t| t.language.trim().to_lowercase()).filter(|l| !l.is_empty());
    let mut made = Presentation {
        id: Uuid::new_v4(),
        title,
        groups,
        copyright: Copyright {
            artist: String::new(),
            author: authors.join(", "),
            publisher,
            year,
            ccli_song: song.ccli(),
        },
        theme: None,
        arrangement: arrangements.first().map(|a| a.id),
        arrangements,
        loops: false,
        transition: None,
        catalog: None,
    };
    let made_hash = catalog::fingerprint(&made);
    let arrangement = song.arrangement.as_ref().map(|a| &a.id).filter(|id| id.is_safe());
    made.catalog = Some(Box::new(CatalogOrigin {
        source: super::SOURCE.to_owned(),
        key: super::song_key(church, &song.id, arrangement),
        language: String::new(),
        second: second.clone(),
        // What TimelyChurch has is the song as SoftPresenter makes it from what it sent: a
        // change there that changes nothing shown (a new tempo) is no change here.
        text_hash: made_hash.clone(),
        made_hash,
        added: catalog::date_of(now),
        newer: None,
        gone: false,
    }));
    MadeSong { song: made, second, untranslated: if used { missing } else { 0 }, unmatched }
}

/// What TimelyChurch has for a song now, for [`catalog::judge`] to weigh against the copy in
/// the library: the song made again from what TimelyChurch sent today.
pub fn fresh_source(fresh: &MadeSong) -> Source {
    fresh.song.catalog.as_ref().map_or(Source::Unknown, |mark| Source::Has(mark.text_hash.clone()))
}

/// "Default" or "Default Arrangement": the name of no arrangement in particular.
fn is_default_name(name: &str) -> bool {
    matches!(name.trim().to_lowercase().as_str(), "default" | "default arrangement")
}

/// The translation's section for the song's section at `place`: the one at the same place,
/// unless that one names another section and one further on names this one.
fn lined_up<'a>(section: &Section, place: usize, theirs: &'a [TranslationSection]) -> Option<&'a TranslationSection> {
    let named = |other: &TranslationSection| other.name.as_deref().is_some_and(|name| same_name(name, &section.name));
    match theirs.get(place) {
        Some(at) if at.name.as_deref().is_none_or(|name| name.trim().is_empty()) || named(at) => Some(at),
        at => theirs.iter().find(|other| named(other)).or(at),
    }
}

/// A section's parts (split at its blank lines) as lines of words, each with its translated
/// line. Lines are paired by their place in the section as sent, before anything is taken out.
/// A line left with no words once its chords are out is no line.
fn stanzas(section: &Section, below: Option<&TranslationSection>) -> Vec<Vec<(String, Option<String>)>> {
    let mut parts: Vec<Vec<(String, Option<String>)>> = vec![Vec::new()];
    for (at, raw) in section.lines.iter().enumerate() {
        let raw = raw.as_deref().unwrap_or("");
        if raw.trim().is_empty() {
            if parts.last().is_some_and(|part| !part.is_empty()) {
                parts.push(Vec::new());
            }
            continue;
        }
        let words = words_of(raw);
        if words.is_empty() {
            continue;
        }
        let translated = below
            .and_then(|below| below.lines.get(at))
            .and_then(Option::as_deref)
            // A translated line is one line: a line break in it would end the box's line early.
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|line| !line.is_empty())
            .map(|line| if line == "--" { "\u{2014}".to_owned() } else { line });
        if let Some(part) = parts.last_mut() {
            part.push((words, translated));
        }
    }
    parts.retain(|part| !part.is_empty());
    parts
}

/// The groups in the order sung, and the names in the order that name no group. A name that
/// several groups have (two sections sent with one name) takes them in turn. No order sung:
/// the groups as written.
fn order_sung(groups: &[Group], order: &[String]) -> (Vec<Uuid>, Vec<String>) {
    if order.is_empty() {
        return (groups.iter().map(|group| group.id).collect(), Vec::new());
    }
    let mut turns: Vec<(String, usize)> = Vec::new();
    let mut sung = Vec::new();
    let mut unmatched = Vec::new();
    for name in order {
        let named: Vec<&Group> = groups.iter().filter(|group| same_name(&group.name, name)).collect();
        if named.is_empty() {
            unmatched.push(super::plain(name));
            continue;
        }
        let folded = fold(name);
        let turn = match turns.iter_mut().find(|(have, _)| *have == folded) {
            Some((_, turn)) => {
                *turn += 1;
                *turn
            }
            None => {
                turns.push((folded, 0));
                0
            }
        };
        sung.push(named[turn % named.len()].id);
    }
    (sung, unmatched)
}

/// A name as compared: lower case, single spaces, no escapes.
fn fold(name: &str) -> String {
    super::plain(name).split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Two section names that are the same name, whatever the case and spacing.
fn same_name(one: &str, other: &str) -> bool {
    fold(one) == fold(other)
}

/// A line's words: no chords, no repeat signs, single spaces. A line that is only `--` (which
/// would start a translation in the song text) becomes a dash.
fn words_of(line: &str) -> String {
    let words = without_chords(line);
    let words = if words.contains('|') {
        words.replace("||:", " ").replace(":||", " ").replace("|:", " ").replace(":|", " ")
    } else {
        words
    };
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if words == "--" { "\u{2014}".to_owned() } else { words }
}

#[cfg(test)]
mod checks {
    use super::*;

    #[test]
    fn a_lines_words_lose_chords_and_repeat_signs() {
        assert_eq!(words_of("||: [G]Sing it [D]twice :||"), "Sing it twice");
        assert_eq!(words_of("[C] [G]"), "");
        assert_eq!(words_of("  plain   words "), "plain words");
        assert_eq!(words_of("--"), "\u{2014}");
    }

    #[test]
    fn names_compare_whatever_the_case_and_spacing() {
        assert!(same_name("Chorus  1", "chorus 1"));
        assert!(same_name("Faith &amp; Hope", "faith & hope"));
        assert!(!same_name("Chorus", "Chorus 1"));
    }
}
