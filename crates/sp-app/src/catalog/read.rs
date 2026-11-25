//! Reading a song pack: a zip of one XML file per song.
//!
//! A song is a title, who wrote it, its sections (each with a name such as "Verse 1", whether
//! each line starts with a capital, and its lines) and any number of versions in other
//! languages (each a language code and its own sections). Everything read is untrusted: the
//! zip is bounded (`sp_model::bounded_zip`), each file's size, depth and element count are
//! bounded here, and a file that is not well-formed XML, or is too big or too odd, is skipped
//! and counted, never allowed to stop the rest.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sp_model::bounded_zip::{self, ZipLimits, ZipRefusal};

use super::fold::fold;

/// What may be read from one language's song pack. The biggest pack so far is 16 MB packed and
/// about 45 MB unpacked, with some 13,600 files; the biggest file is under 100 KB.
pub const LIMITS: ZipLimits = ZipLimits { file: 96 << 20, entry: 1 << 20, unpacked: 256 << 20, entries: 40_000 };
/// The most songs one language may have.
pub const MOST_SONGS: usize = 30_000;
/// The most sections one version of a song may have, the most versions, and the most text in
/// one section.
const MOST_SECTIONS: usize = 500;
const MOST_VERSIONS: usize = 200;
const MOST_TEXT: usize = 64 * 1024;
/// The most elements one song file may have.
const MOST_NODES: u32 = 100_000;
/// The deepest nesting of elements read: a song file is five deep.
const MOST_DEPTH: usize = 16;
/// The longest title or name kept, in letters.
const LONGEST_NAME: usize = 300;

/// One section of a song: "Verse 1" and its words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    /// Each line starts with a capital letter.
    pub capitalise: bool,
    /// The lines, joined with line breaks; a blank line starts another slide.
    pub text: String,
}

/// A song's words in another language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// The language code the pack gives it (`ru`, `en`, sometimes `kk-EN`…), as written.
    pub lang: String,
    pub sections: Vec<Section>,
}

/// One song as the pack has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Song {
    pub title: String,
    pub author: String,
    pub copyright: String,
    pub year: String,
    /// The musical key, when the pack gives one.
    pub musical_key: String,
    pub sections: Vec<Section>,
    pub versions: Vec<Version>,
    /// The song's lines had chords written in them, which were taken out.
    pub chords: bool,
}

/// What reading a pack found.
#[derive(Debug, Default)]
pub struct Read {
    /// Each song, with its place in the zip and its file's name there.
    pub songs: Vec<(usize, String, Song)>,
    /// Files that are not well-formed, or too big or odd, and were left out.
    pub malformed: usize,
    /// Files with no words in them, left out.
    pub empty: usize,
    /// Songs the pack has twice, word for word, kept once.
    pub duplicates: usize,
}

/// Why a whole pack was refused (said in words by `store::refused_words`).
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    NotAZip,
    TooMany,
    TooBig,
    NoSongs,
}

/// Reads every song in a song pack (a zip). Songs are told apart by [`song_key`]; a song the
/// pack has twice, with the same words, is kept once.
pub fn read_pack(bytes: &[u8]) -> Result<Read, Refused> {
    let mut found = Read::default();
    let wanted = |name: &str| name.to_lowercase().ends_with(".xml");
    let result = bounded_zip::for_each_entry_at(bytes, &LIMITS, wanted, |at, name, xml, _unpacked| {
        match read_song(&xml) {
            Ok(Some(song)) => found.songs.push((at, name, song)),
            Ok(None) => found.empty += 1,
            Err(()) => found.malformed += 1,
        }
        if found.songs.len() > MOST_SONGS {
            return Err(ZipRefusal::TooMany);
        }
        Ok(())
    });
    match result {
        Ok(()) => {}
        Err(ZipRefusal::Damaged | ZipRefusal::Overlap) => return Err(Refused::NotAZip),
        Err(ZipRefusal::TooMany) => return Err(Refused::TooMany),
        Err(ZipRefusal::UnpacksTooMuch) => return Err(Refused::TooBig),
    }
    // The same song twice (the pack has "Title" and "Title 2" with one text): once.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let before = found.songs.len();
    found.songs.retain(|(_, _, song)| seen.insert(full_fingerprint(song), 0).is_none());
    found.duplicates = before - found.songs.len();
    if found.songs.is_empty() {
        return Err(Refused::NoSongs);
    }
    Ok(found)
}

/// Reads one song file: `Ok(None)` when it has no words, `Err` when it is not a song file.
pub fn read_song(xml: &[u8]) -> Result<Option<Song>, ()> {
    let text = std::str::from_utf8(xml).map_err(|_| ())?;
    let options =
        roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: MOST_NODES, ..roxmltree::ParsingOptions::default() };
    let document = roxmltree::Document::parse_with_options(text, options).map_err(|_| ())?;
    let root = document.root_element();
    if root.tag_name().name() != "song" || !shallow_enough(root) {
        return Err(());
    }
    let words = |node: Option<roxmltree::Node<'_, '_>>| -> String {
        node.map(|node| node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect::<String>())
            .map(|text| one_line(&text))
            .unwrap_or_default()
    };
    let mut chords = false;
    let sections = |lyrics: roxmltree::Node<'_, '_>, chords: &mut bool| -> Result<Vec<Section>, ()> {
        let mut sections = Vec::new();
        for section in lyrics.children().filter(|n| n.is_element() && n.tag_name().name() == "section") {
            let words = child(section, "lyrics")
                .map(|node| node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect::<String>())
                .unwrap_or_default();
            if words.len() > MOST_TEXT {
                return Err(());
            }
            let (text, had_chords) = clean_lines(&words);
            *chords |= had_chords;
            if text.is_empty() {
                continue;
            }
            let title = one_line(section.attribute("title").unwrap_or(""));
            let capitalise = section.attribute("capitalise").is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
            sections.push(Section { title: clip(&title), capitalise, text });
            if sections.len() > MOST_SECTIONS {
                return Err(());
            }
        }
        Ok(sections)
    };
    let main = match child(root, "lyrics") {
        Some(lyrics) => sections(lyrics, &mut chords)?,
        None => Vec::new(),
    };
    let mut versions = Vec::new();
    for options in root.children().filter(|n| n.is_element() && n.tag_name().name() == "translationoptions") {
        let lang = words(child(options, "lang"));
        let Some(lyrics) = child(options, "lyrics") else { continue };
        let sections = sections(lyrics, &mut chords)?;
        if lang.is_empty() || sections.is_empty() {
            continue;
        }
        versions.push(Version { lang: clip(&lang), sections });
        if versions.len() > MOST_VERSIONS {
            return Err(());
        }
    }
    if main.is_empty() {
        return Ok(None);
    }
    let title = clip(&words(child(root, "title")));
    let title = if title.is_empty() { first_words(&main[0].text) } else { title };
    Ok(Some(Song {
        title,
        author: clip(&words(child(root, "author"))),
        copyright: clip(&words(child(root, "copyright"))),
        year: clip(&words(child(root, "year"))),
        musical_key: clip(&words(child(root, "key"))),
        sections: main,
        versions,
        chords,
    }))
}

/// True when no element is more than [`MOST_DEPTH`] deep: walked without recursion, one visit each.
fn shallow_enough(root: roxmltree::Node<'_, '_>) -> bool {
    let (mut node, mut depth) = (root, 1_usize);
    loop {
        if let Some(child) = node.first_child() {
            node = child;
            depth += 1;
            if depth > MOST_DEPTH {
                return false;
            }
            continue;
        }
        loop {
            if node == root {
                return true;
            }
            if let Some(next) = node.next_sibling() {
                node = next;
                break;
            }
            match node.parent() {
                Some(parent) => {
                    node = parent;
                    depth -= 1;
                }
                None => return true,
            }
        }
    }
}

/// The first element inside `parent` with this name.
fn child<'a, 'i>(parent: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    parent.children().find(|node| node.is_element() && node.tag_name().name() == name)
}

/// A short name or title: one line, trimmed.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A name no longer than the limit, cut at a letter.
fn clip(text: &str) -> String {
    text.chars().take(LONGEST_NAME).collect()
}

/// The first few words of a text, for a song that has no title.
fn first_words(text: &str) -> String {
    clip(&text.lines().next().unwrap_or("").split_whitespace().take(6).collect::<Vec<_>>().join(" "))
}

/// A section's lines: line endings made plain, trailing spaces gone, chord lines taken out (and
/// said), a first line that only names the section ("1 куплет:") left off, and no blank lines
/// at either end. A blank line in the middle stays: it starts another slide.
fn clean_lines(text: &str) -> (String, bool) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut chords = false;
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        if is_chord_line(line) {
            chords = true;
            continue;
        }
        lines.push(line.trim_end().to_owned());
    }
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if lines.len() > 1 && is_label(&lines[0]) {
        lines.remove(0);
        while lines.first().is_some_and(|line| line.trim().is_empty()) {
            lines.remove(0);
        }
    }
    (lines.join("\n"), chords)
}

/// A line that only names the section, as translations often start one: at most three short
/// words, ending in a colon, with nothing but letters, digits, dots and hyphens before it.
fn is_label(line: &str) -> bool {
    let line = line.trim();
    let Some(name) = line.strip_suffix(':').map(|rest| rest.trim_end_matches(':').trim()) else { return false };
    !name.is_empty()
        && name.chars().count() <= 20
        && name.split_whitespace().count() <= 3
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '-'))
}

/// A line of chords: a dot, then only chord names (`.G  Am  C/E`).
fn is_chord_line(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix('.') else { return false };
    let mut tokens = rest.split_whitespace().peekable();
    tokens.peek().is_some() && tokens.all(is_chord)
}

fn is_chord(token: &str) -> bool {
    /// A note's name, `A` to `H` with a sharp or flat: how many bytes of the text it takes.
    fn note(text: &str) -> Option<usize> {
        let mut letters = text.chars();
        let first = letters.next().filter(|c| ('A'..='H').contains(c))?;
        let sign = letters.next().filter(|c| matches!(c, '#' | 'b' | '♯' | '♭')).map_or(0, char::len_utf8);
        Some(first.len_utf8() + sign)
    }
    let (chord, bass) = token.split_once('/').map_or((token, None), |(chord, bass)| (chord, Some(bass)));
    let Some(taken) = note(chord) else { return false };
    let rest = &chord[taken..];
    let plain = rest.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '(' | ')' | '+' | '-' | '#'));
    let known = rest.is_empty()
        || rest.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '('))
        || ["m", "maj", "min", "M", "dim", "aug", "sus", "add"].iter().any(|word| rest.starts_with(word));
    plain && known && bass.is_none_or(|bass| note(bass) == Some(bass.len()))
}

/// What a song's words say, in one folded string: its sections and its versions, for telling
/// songs apart and for telling when the text changed.
fn canonical(song: &Song, with_versions: bool) -> String {
    let mut out = String::new();
    let add = |out: &mut String, label: &str, sections: &[Section]| {
        out.push_str(label);
        out.push('\u{1}');
        for section in sections {
            out.push_str(&section.title);
            out.push(if section.capitalise { '\u{2}' } else { '\u{3}' });
            out.push_str(&section.text);
            out.push('\u{1}');
        }
    };
    out.push_str(&song.title);
    out.push('\u{1}');
    out.push_str(&song.author);
    out.push('\u{1}');
    out.push_str(&song.copyright);
    out.push('\u{1}');
    out.push_str(&song.year);
    out.push('\u{1}');
    add(&mut out, "", &song.sections);
    if with_versions {
        for version in &song.versions {
            add(&mut out, &version.lang, &version.sections);
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fingerprint of everything a song says, to tell when the catalog's text changed: 16 hex
/// letters of a SHA-256.
pub fn text_hash(song: &Song) -> String {
    hex(&Sha256::digest(canonical(song, true).as_bytes())[..8])
}

/// The words of a song, folded, to tell a duplicate (the same song twice in one pack).
fn full_fingerprint(song: &Song) -> String {
    hex(&Sha256::digest(fold(&canonical(song, false)).as_bytes())[..12])
}

/// How the catalog names a song. There is no number or id in the song pack, so the key is made
/// from the song itself: the language, the title as a search would fold it (lower case, ё as е,
/// no punctuation, words joined by hyphens, at most 48 letters), `~`, and eight hex letters of a
/// SHA-256 of the folded first section's words. Two songs of one language that would have one key
/// (a title and a first section shared, a text differing after) are told apart by a `.` and four
/// more letters, of the name of the file each has in the pack (see [`keyed`]). A song's key changes when its title or its first section
/// does: the catalog then sees one song gone and a new one come, and a copy in the library says
/// so quietly. Everything else about keys lives here, so a real id from the source can take
/// its place by changing this function and bumping [`KEY_VERSION`].
pub fn song_key(lang: &str, song: &Song) -> String {
    let slug: Vec<String> = fold(&song.title).split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect();
    let mut slug = slug.join("-");
    if slug.chars().count() > 48 {
        slug = slug.chars().take(48).collect::<String>().trim_end_matches('-').to_owned();
    }
    if slug.is_empty() {
        slug = "song".to_owned();
    }
    let first = song.sections.first().map_or_else(String::new, |s| fold(&s.text));
    let digest = hex(&Sha256::digest(first.as_bytes())[..4]);
    format!("{lang}:{slug}~{digest}")
}

/// What the keys say they are made by. Written in each language's index; an index made by
/// another version is made again from its song pack.
pub const KEY_VERSION: u32 = 1;

/// Gives each song of a pack its key, telling apart songs that would share one.
pub fn keyed(lang: &str, songs: Vec<(usize, String, Song)>) -> Vec<(String, usize, Song)> {
    let mut base: Vec<String> = songs.iter().map(|(_, _, song)| song_key(lang, song)).collect();
    let mut shared: HashMap<String, usize> = HashMap::new();
    for key in &base {
        *shared.entry(key.clone()).or_insert(0) += 1;
    }
    // Songs that would share a key are told apart by their file's name in the pack, which stays
    // the same when the song's words are corrected.
    for (at, (_, name, _)) in songs.iter().enumerate() {
        if shared.get(&base[at]).copied().unwrap_or(0) > 1 {
            let more = hex(&Sha256::digest(name.as_bytes())[..2]);
            base[at] = format!("{}.{more}", base[at]);
        }
    }
    // Even then, two with one name (a pack may do that) are not both kept under one key.
    let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
    songs
        .into_iter()
        .zip(base)
        .filter(|(_, key)| taken.insert(key.clone()))
        .map(|((at, _, song), key)| (key, at, song))
        .collect()
}
