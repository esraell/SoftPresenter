//! A TimelyChurch song made into the app's own song.
//!
//! The words come from the arrangement's chart. Each section of the chart becomes a group named
//! as the church named it; a blank line inside a section starts another slide, and a stretch
//! longer than the most lines a slide takes is shared out evenly, as songs imported from text
//! are. Chords, chord-only lines ("Intro: G D Em") and repeat signs (`||:` `:||`) are taken out:
//! they are for the musicians.
//!
//! A chart writes every section where it is sung, so a chorus sung three times is written three
//! times, or written once and then named again with no lines, or named with a count ("Chorus -
//! 2x", "Chorus x2", a line "Chorus: 3x"). A section with the same name and the same words as
//! one before it is that group again; a section named with no words is the first group of that
//! name again (or, failing that, its first numbered one: "Chorus 1" for "Chorus"), and one that
//! names nothing sung before it is listed; a count sings it that many times. The order sung
//! becomes the song's arrangement, chosen to play, whenever it is not simply the groups as
//! written.
//!
//! A translation is lined up line by line, by the line's words (TimelyChurch gives each line of
//! the song with its translated line): each slide gets a translation box of the translated
//! lines of its own lines, in the same order. A line nobody translated is left out of the box
//! and counted; a slide with no translated line has no box; a translation with no translated
//! line at all puts nothing under the words.

use std::collections::HashMap;

use sp_model::{
    Arrangement, CatalogOrigin, Copyright, Group, MOST_LINES, MOST_TIMES, Object, Presentation, Role, Slide, Uuid,
    group_color, section_name, share_out, without_chords, year_and_publisher,
};

use super::wire::{Chart, ChartSection, Song, StyleReply, Translation};
use crate::catalog::{self, Source};

/// The most lines a slide of a TimelyChurch song gets: the lines per slide of the church's
/// saved slide style, when it saved one (TimelyChurch's own default of six isn't the church's
/// choice), else the app's usual, as for songs imported from text.
pub fn lines_per_slide(style: Option<&StyleReply>) -> usize {
    style
        .filter(|reply| reply.has_default && reply.style.lines_per_slide > 0)
        .map_or(MOST_LINES, |reply| usize::try_from(reply.style.lines_per_slide).unwrap_or(MOST_LINES).clamp(1, 12))
}

/// A TimelyChurch song made into the app's song.
#[derive(Clone, Debug)]
pub struct MadeSong {
    pub song: Presentation,
    /// The language of the translation put under the words, if any line had one.
    pub second: Option<String>,
    /// Lines of words with no translated line, when a translation was put under the words.
    pub untranslated: usize,
    /// Sections named with no words that name no group sung before them, in order: nothing
    /// is sung for them. An intro with no words is one; so is a chorus named before the chorus
    /// is written.
    pub unmatched: Vec<String>,
}

/// Makes a TimelyChurch song into the app's song, from `chart` (one of its arrangements'),
/// with `translation` under its words if it has translated lines. `church` is the address the
/// church was connected with, for the song's key; `most_lines` comes from
/// [`lines_per_slide`]; `now` (seconds since 1970) is when it joins the library.
pub fn make_song(
    church: &str,
    song: &Song,
    chart: &Chart,
    translation: Option<&Translation>,
    most_lines: usize,
    now: i64,
) -> MadeSong {
    let arrangement = song.arrangements.iter().find(|arrangement| arrangement.id == chart.arrangement_id);
    let mut title = super::plain(&song.title);
    // A song sung in more than one arrangement is a song of the library for each.
    if let Some(arrangement) = arrangement.filter(|a| !a.is_default && song.arrangements.len() > 1) {
        let name = super::plain(&arrangement.name);
        if !name.is_empty() {
            title = format!("{title} ({name})");
        }
    }
    let mut lined = LinedUp::new(translation);
    let mut groups: Vec<Group> = Vec::new();
    // Each group's slides as lines of words, to tell a group sung again from a new one.
    let mut written: Vec<Vec<Vec<String>>> = Vec::new();
    let mut sung: Vec<Uuid> = Vec::new();
    let mut unmatched = Vec::new();
    for part in parts(&chart.sections) {
        let slides: Vec<Vec<String>> = part
            .stanzas
            .iter()
            .flat_map(|stanza| share_out(stanza, most_lines).into_iter().map(<[String]>::to_vec))
            .collect();
        let again = if slides.is_empty() {
            // Named with no words: the first group of that name sung again; failing that, the
            // first numbered one of it ("Chorus" for "Chorus 1").
            if part.name.is_empty() {
                None
            } else {
                groups
                    .iter()
                    .position(|group| same_name(&group.name, &part.name))
                    .or_else(|| groups.iter().position(|group| numbered(&group.name, &part.name)))
            }
        } else {
            (0..groups.len()).find(|&at| same_name(&groups[at].name, &part.name) && written[at] == slides)
        };
        let id = match again {
            Some(at) => groups[at].id,
            // Nothing of that name sung before: no words to show.
            None if slides.is_empty() => {
                if !part.name.is_empty() {
                    unmatched.push(part.name);
                }
                continue;
            }
            None => {
                let made = slides.iter().map(|lines| lined.slide(lines)).collect();
                let group =
                    Group { id: Uuid::new_v4(), name: part.name.clone(), color: group_color(&part.name), slides: made };
                let id = group.id;
                groups.push(group);
                written.push(slides);
                id
            }
        };
        sung.extend(std::iter::repeat_n(id, part.times));
    }
    let as_written: Vec<Uuid> = groups.iter().map(|group| group.id).collect();
    let arrangements = if sung == as_written || sung.is_empty() {
        Vec::new()
    } else {
        let name = arrangement
            .map(|a| super::plain(&a.name))
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("default arrangement"))
            .unwrap_or_else(|| super::SOURCE_NAME.to_owned());
        vec![Arrangement { id: Uuid::new_v4(), name, groups: sung }]
    };
    if let Some(notes) = song.notes.as_deref().map(str::trim).filter(|notes| !notes.is_empty())
        && let Some(first) = groups.first_mut().and_then(|group| group.slides.first_mut())
    {
        first.notes = Some(notes.to_owned());
    }
    let (year, publisher) = song.copyright_info.as_deref().map(year_and_publisher).unwrap_or_default();
    let words = |text: Option<&str>| text.map(super::plain).unwrap_or_default();
    let second = translation.filter(|_| lined.used).map(|t| t.language.trim().to_lowercase());
    let mut made = Presentation {
        id: Uuid::new_v4(),
        title,
        groups,
        copyright: Copyright {
            artist: words(song.artist.as_deref()),
            author: words(song.composer.as_deref()),
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
    made.catalog = Some(Box::new(CatalogOrigin {
        source: super::SOURCE.to_owned(),
        key: super::song_key(church, song.id, chart.arrangement_id),
        language: String::new(),
        second: second.clone(),
        // TimelyChurch's songs carry no version, so its text's fingerprint is the song as it
        // makes now: a change at TimelyChurch that changes nothing shown is no change.
        text_hash: made_hash.clone(),
        made_hash,
        added: catalog::date_of(now),
        newer: None,
        gone: false,
    }));
    MadeSong { song: made, second, untranslated: if lined.used { lined.missing } else { 0 }, unmatched }
}

/// What TimelyChurch has for a song now, for [`catalog::judge`] to weigh against the copy in
/// the library: the song made again from what TimelyChurch sent today.
pub fn fresh_source(fresh: &MadeSong) -> Source {
    fresh.song.catalog.as_ref().map_or(Source::Unknown, |mark| Source::Has(mark.text_hash.clone()))
}

/// A chart's sections from its text as TimelyChurch keeps it: a `[Name]` line of its own starts
/// a section (one that isn't a chord), and ChordPro directives (`{title: …}`) and `#` comments
/// are left out.
pub fn sections_from_text(text: &str) -> Vec<ChartSection> {
    let mut sections = vec![ChartSection::default()];
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') || (line.starts_with('{') && line.ends_with('}')) {
            continue;
        }
        let header = line
            .strip_prefix('[')
            .and_then(|inner| inner.strip_suffix(']'))
            .filter(|inner| !inner.contains(['[', ']']) && !inner.trim().is_empty() && !is_chord(inner.trim()));
        if let Some(name) = header {
            sections.push(ChartSection { name: Some(name.trim().to_owned()), lines: Vec::new() });
        } else if let Some(section) = sections.last_mut() {
            section.lines.push(raw.trim_end().to_owned());
        }
    }
    sections.retain(|section| section.name.is_some() || section.lines.iter().any(|line| !line.trim().is_empty()));
    sections
}

/// A run of the chart under one name: its verses' parts as lines of words, and how many times
/// running it is sung.
struct Part {
    name: String,
    times: usize,
    stanzas: Vec<Vec<String>>,
}

impl Part {
    fn new(name: String, times: usize) -> Part {
        Part { name, times, stanzas: Vec::new() }
    }

    fn has_words(&self) -> bool {
        self.stanzas.iter().any(|stanza| !stanza.is_empty())
    }

    fn words(&mut self, line: String) {
        match self.stanzas.last_mut() {
            Some(stanza) => stanza.push(line),
            None => self.stanzas.push(vec![line]),
        }
    }

    fn stanza_break(&mut self) {
        if self.stanzas.last().is_some_and(|stanza| !stanza.is_empty()) {
            self.stanzas.push(Vec::new());
        }
    }

    fn done(mut self) -> Part {
        self.stanzas.retain(|stanza| !stanza.is_empty());
        self
    }
}

/// The chart as parts, in the order sung.
fn parts(sections: &[ChartSection]) -> Vec<Part> {
    let mut parts = Vec::new();
    for section in sections {
        let (name, times) =
            section.name.as_deref().map_or((String::new(), 1), |name| split_count(name.trim().trim_end_matches(':')));
        let mut part = Part::new(name, times);
        for raw in &section.lines {
            if raw.trim().is_empty() {
                part.stanza_break();
                continue;
            }
            let line = words_of(raw);
            if line.is_empty() || is_chord_line(&line) || is_direction(&line) {
                continue;
            }
            if let Some((name, times)) = heading(&line) {
                // A heading where nothing has been sung yet names the part that starts here.
                if !part.has_words() && (part.name.is_empty() || same_name(&part.name, &name)) {
                    part = Part::new(name, times);
                } else {
                    parts.push(std::mem::replace(&mut part, Part::new(name, times)).done());
                }
                continue;
            }
            part.words(line);
        }
        parts.push(part.done());
    }
    parts
}

/// A line's words: no chords, no repeat signs, single spaces. A line that is only `--` (which
/// would start a translation in the song text) becomes a dash.
fn words_of(line: &str) -> String {
    let words = without_chords(line);
    let words = if words.contains(['|']) {
        let unmarked = words.replace("||:", " ").replace(":||", " ").replace("|:", " ").replace(":|", " ");
        unmarked.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        words
    };
    if words == "--" { "\u{2014}".to_owned() } else { words }
}

/// The section a line of words names, tidied, and how many times it is sung: "CHORUS 1" is
/// "Chorus 1" once, "Chorus: 3x" is "Chorus" three times. `None` for a line of words.
fn heading(line: &str) -> Option<(String, usize)> {
    let (name, times) = split_count(line.trim().trim_end_matches(':'));
    Some((section_name(&name)?, times))
}

/// The marks of a count in a section's name: "x" in Latin and Cyrillic, and the times sign.
const TIMES_MARKS: [char; 5] = ['x', 'X', 'х', 'Х', '×'];
/// What may stand between a name and its count.
const BETWEEN: &[char] = &[' ', '-', '–', '—', ':', ',', '(', '.'];

/// A section's name without a count at its end, and the count: "Chorus - 2x", "Chorus x2",
/// "Припев 2х" and "Chorus (3x)" are "Chorus" or "Припев" two or three times. A name with no
/// count is sung once.
fn split_count(name: &str) -> (String, usize) {
    let trimmed = name.trim().trim_end_matches(')').trim_end();
    let found = count_last(trimmed).or_else(|| mark_last(trimmed));
    match found {
        Some((base, times)) if !base.is_empty() => (base.to_owned(), times.clamp(1, MOST_TIMES)),
        _ => (name.trim().to_owned(), 1),
    }
}

/// "… 2x": the number, then the mark, last.
fn count_last(name: &str) -> Option<(&str, usize)> {
    let rest = name.strip_suffix(TIMES_MARKS)?.trim_end();
    let digits_at = rest.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let times = rest[digits_at..].parse().ok()?;
    Some((rest[..digits_at].trim_end_matches(BETWEEN), times))
}

/// "… x2": the mark, standing apart from the name, then the number, last.
fn mark_last(name: &str) -> Option<(&str, usize)> {
    let digits_at = name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let times = name[digits_at..].parse().ok()?;
    let rest = name[..digits_at].trim_end().strip_suffix(TIMES_MARKS)?;
    (rest.is_empty() || rest.ends_with(BETWEEN)).then(|| (rest.trim_end_matches(BETWEEN), times))
}

/// Two section names that are the same name, whatever the case and spacing.
fn same_name(one: &str, other: &str) -> bool {
    let fold = |name: &str| name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    fold(one) == fold(other)
}

/// A group's name that is a name with a number or letter after it ("Chorus 1", "Chorus A"
/// for "Chorus").
fn numbered(group: &str, name: &str) -> bool {
    let (group, name) = (group.trim().to_lowercase(), name.trim().to_lowercase());
    group.strip_prefix(&name).and_then(|rest| rest.strip_prefix(' ')).is_some_and(|rest| {
        let rest = rest.trim();
        !rest.is_empty() && rest.chars().count() <= 2 && rest.chars().all(char::is_alphanumeric)
    })
}

/// A line that only says something to the musicians: "End", "Fine", "Play Verse 1", "Repeat
/// Chorus". Kept to these few: a line that only might be one is kept as words.
fn is_direction(line: &str) -> bool {
    let lower = line.trim().trim_end_matches(['.', '!', ':']).to_lowercase();
    if matches!(lower.as_str(), "end" | "the end" | "fine" | "repeat" | "конец" | "кінець" | "повтор")
    {
        return true;
    }
    ["play ", "repeat ", "повтор ", "повторить "]
        .iter()
        .any(|start| lower.strip_prefix(start).is_some_and(|rest| heading(rest).is_some()))
}

/// A line of chords with no words: "G D/F# Em C", "Intro: Fm – Bbm – C", "F------".
fn is_chord_line(line: &str) -> bool {
    let mut tokens = line.split_whitespace().peekable();
    // A label first ("Intro:"), as charts put before the chords of a part with no words.
    if tokens.peek().is_some_and(|token| token.len() > 1 && token.ends_with(':')) {
        tokens.next();
    }
    let mut chords = 0;
    for token in tokens {
        let token = token.trim_start_matches('(').trim_end_matches([')', '-', '–', '—', '.', ',']);
        if token.chars().all(|c| "-–—|/:.,".contains(c)) || is_count(token) {
            continue;
        }
        if !is_chord(token) {
            return false;
        }
        chords += 1;
    }
    chords > 0
}

/// "x2", "2x", "х3": a count of times on its own.
fn is_count(token: &str) -> bool {
    let digits = token.trim_start_matches(TIMES_MARKS).trim_end_matches(TIMES_MARKS);
    digits.len() < token.len() && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// A chord's name: a note (A to G, with a sharp or flat), what kind of chord, and a bass note
/// after a slash: "Am", "C#m7", "Bbsus4", "F/A", "Ebmaj7(add9)".
fn is_chord(token: &str) -> bool {
    const KINDS: [&str; 16] =
        ["maj", "min", "dim", "aug", "sus", "add", "m", "M", "°", "ø", "Δ", "+", "-", "(", ")", "#"];
    let note = |text: &str| -> Option<usize> {
        let first = text.chars().next().filter(|c| ('A'..='G').contains(c))?;
        let after = &text[first.len_utf8()..];
        let accidental = after.chars().next().filter(|c| matches!(*c, '#' | 'b' | '♯' | '♭'));
        Some(first.len_utf8() + accidental.map_or(0, char::len_utf8))
    };
    let Some(root) = note(token) else { return false };
    let mut rest = &token[root..];
    while let Some(c) = rest.chars().next() {
        if c.is_ascii_digit() {
            rest = &rest[1..];
        } else if let Some(bass) = rest.strip_prefix('/') {
            let Some(length) = note(bass) else { return false };
            rest = &bass[length..];
        } else if let Some(kind) = KINDS.iter().find(|kind| rest.starts_with(*kind)) {
            rest = &rest[kind.len()..];
        } else {
            return false;
        }
    }
    true
}

/// A translation's lines, ready to go under the song's: each line of the song (as its words)
/// with its translated lines, in the order the song sings them.
struct LinedUp {
    by_line: HashMap<String, (Vec<String>, usize)>,
    /// Whether any line was given a translation.
    used: bool,
    /// Lines of words with no translated line.
    missing: usize,
}

impl LinedUp {
    fn new(translation: Option<&Translation>) -> LinedUp {
        let mut by_line: HashMap<String, (Vec<String>, usize)> = HashMap::new();
        for row in translation.iter().flat_map(|t| &t.sections).flat_map(|section| &section.rows) {
            // A translated line is one line: a line break in it would end the box's line early.
            let translated = row.translated.split_whitespace().collect::<Vec<_>>().join(" ");
            let translated = if translated == "--" { "\u{2014}".to_owned() } else { translated };
            by_line.entry(words_of(&row.source)).or_default().0.push(translated);
        }
        LinedUp { by_line, used: false, missing: 0 }
    }

    /// The translation of this line, the next time it comes: the translation given for that
    /// time, else any given for the same line.
    fn next(&mut self, line: &str) -> Option<String> {
        let (translations, taken) = self.by_line.get_mut(line)?;
        let at = (*taken).min(translations.len().saturating_sub(1));
        *taken += 1;
        let given = translations.get(at).filter(|t| !t.is_empty());
        given.or_else(|| translations.iter().find(|t| !t.is_empty())).cloned()
    }

    /// A slide of these lines, with their translation under them.
    fn slide(&mut self, lines: &[String]) -> Slide {
        let mut objects = vec![Object::text(Role::primary(), lines.join("\n"))];
        let below: Vec<String> = lines.iter().filter_map(|line| self.next(line)).collect();
        self.missing += lines.len() - below.len();
        if !below.is_empty() {
            self.used = true;
            objects.push(Object::text(Role::translation(), below.join("\n")));
        }
        Slide::new(objects)
    }
}

#[cfg(test)]
mod checks {
    use super::*;

    #[test]
    fn chords_are_told_from_words() {
        for chord in ["A", "Am", "C#m7", "Bbsus4", "F/A", "Ab7/C", "Ebmaj7(add9)", "Dm2", "F3/Bb", "Bbm/G", "E°"] {
            assert!(is_chord(chord), "{chord}");
        }
        for word in ["Amen", "Dad", "Bad", "Add", "Be", "Abba", "Emmanuel", "Gods", "C/x", "\u{410}m", "am"] {
            assert!(!is_chord(word), "{word}");
        }
        assert!(is_chord_line("Intro: Fm – Fm2/Db – Bbm/G – C"));
        assert!(is_chord_line("G    D/F#   Em   C   x2"));
        assert!(!is_chord_line("A Baby born for all"));
        assert!(!is_chord_line("Am I the one"));
        assert!(!is_chord_line("Intro:"), "a name with no chords is a heading, not a chord line");
    }

    #[test]
    fn counts_are_read_only_where_they_stand_apart() {
        assert_eq!(split_count("Chorus - 2x"), ("Chorus".to_owned(), 2));
        assert_eq!(split_count("Chorus x2"), ("Chorus".to_owned(), 2));
        assert_eq!(split_count("Припев 3х"), ("Припев".to_owned(), 3));
        assert_eq!(split_count("Ending (2x)"), ("Ending".to_owned(), 2));
        assert_eq!(split_count("Chorus x99"), ("Chorus".to_owned(), MOST_TIMES));
        assert_eq!(split_count("Verse 2"), ("Verse 2".to_owned(), 1));
        assert_eq!(split_count("Box2"), ("Box2".to_owned(), 1));
        assert_eq!(split_count("Remix"), ("Remix".to_owned(), 1));
        assert_eq!(split_count("2x"), ("2x".to_owned(), 1));
        assert_eq!(heading("Chorus: 3x"), Some(("Chorus".to_owned(), 3)));
        assert_eq!(heading("CHORUS 1"), Some(("Chorus 1".to_owned(), 1)));
        assert_eq!(heading("Sing to the Lord"), None);
    }

    #[test]
    fn only_a_few_notes_to_the_band_are_taken_for_them() {
        for note in ["End", "Fine.", "Play Verse 1", "Repeat chorus", "Повтор припева", "Конец"] {
            assert!(is_direction(note), "{note}");
        }
        for line in ["Repeat the sounding joy", "Play on, all you people", "The end of all our searching"] {
            assert!(!is_direction(line), "{line}");
        }
    }
}
