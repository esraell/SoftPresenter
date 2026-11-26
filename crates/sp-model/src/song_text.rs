//! The plain-text song format: `[Group]` headers, blank lines between slides, and a `--` line
//! between the main language and its translation.

use uuid::Uuid;

use crate::types::{Group, Object, Presentation, Rgba, Role, Slide};

/// Parses a song written in the plain-text format.
///
/// - A line in square brackets starts a group with that name.
/// - Blank lines separate slides.
/// - Inside a slide, the first line containing only `--` separates the main language from
///   its translation.
/// - Lines before the first group header belong to an unnamed group.
pub fn parse_song_text(title: &str, text: &str) -> Presentation {
    let mut groups: Vec<Group> = Vec::new();
    let mut current = new_group("");
    let mut draft = SlideDraft::default();

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if let Some(name) = group_header(trimmed) {
            draft.finish_into(&mut current);
            push_group(&mut groups, current);
            current = new_group(name);
        } else if trimmed.is_empty() {
            draft.finish_into(&mut current);
        } else if trimmed == "--" && !draft.in_translation {
            draft.in_translation = true;
        } else if draft.in_translation {
            draft.translation.push(line.to_owned());
        } else {
            draft.primary.push(line.to_owned());
        }
    }
    draft.finish_into(&mut current);
    push_group(&mut groups, current);

    Presentation {
        id: Uuid::new_v4(),
        title: title.to_owned(),
        groups,
        copyright: crate::Copyright::default(),
        theme: None,
        arrangements: Vec::new(),
        arrangement: None,
        loops: false,
        transition: None,
        catalog: None,
    }
}

/// The keys that jump to a group, and the group names each one means. Pressing a key shows
/// the first slide of the first group in the song with one of its names.
pub const GROUP_KEYS: [(char, &[&str]); 17] = [
    ('a', &["verse", "verse 1"]),
    ('s', &["verse 2"]),
    ('d', &["verse 3"]),
    ('f', &["verse 4"]),
    ('g', &["verse 5"]),
    ('h', &["verse 6"]),
    ('c', &["chorus", "chorus 1"]),
    ('x', &["chorus 2"]),
    ('z', &["chorus 3"]),
    ('b', &["bridge", "bridge 1"]),
    ('n', &["bridge 2"]),
    ('m', &["bridge 3"]),
    ('p', &["prechorus", "pre-chorus", "pre chorus"]),
    ('t', &["tag"]),
    ('i', &["intro"]),
    ('e', &["ending"]),
    ('o', &["outro"]),
];

/// The words churches name a song's parts with in the app's languages (Russian, Ukrainian,
/// Spanish), and the ones SoftProjector's songs name them with (German and Czech too), each
/// with the English the group keys and colours know.
const PART_WORDS: [(&str, &str); 38] = [
    ("куплет", "verse"),
    ("estrofa", "verse"),
    ("verso", "verse"),
    ("strophe", "verse"),
    ("verš", "verse"),
    ("припев", "chorus"),
    ("приспів", "chorus"),
    ("coro", "chorus"),
    ("estribillo", "chorus"),
    ("refrain", "chorus"),
    ("refrén", "chorus"),
    ("sbor", "chorus"),
    ("бридж", "bridge"),
    ("брідж", "bridge"),
    ("мост", "bridge"),
    ("міст", "bridge"),
    ("puente", "bridge"),
    ("предприпев", "prechorus"),
    ("пре-припев", "prechorus"),
    ("передприспів", "prechorus"),
    ("precoro", "prechorus"),
    ("pre-coro", "prechorus"),
    ("тег", "tag"),
    ("вступление", "intro"),
    ("вступ", "intro"),
    ("introducción", "intro"),
    ("einleitung", "intro"),
    ("úvod", "intro"),
    ("окончание", "ending"),
    ("концовка", "ending"),
    ("закінчення", "ending"),
    ("кінцівка", "ending"),
    ("final", "ending"),
    ("ende", "ending"),
    ("závěr", "ending"),
    ("konec", "ending"),
    ("кода", "outro"),
    ("coda", "outro"),
];

/// A group's name as the group keys and colours know it: lower case, and in English when a
/// church named the part in its own language ("Куплет 2" is "verse 2", "Приспів:" is
/// "chorus"). Any other name comes back in lower case, as it was.
pub fn group_name_in_english(name: &str) -> String {
    let lower = name.trim().trim_end_matches([':', '.']).trim().to_lowercase();
    let at = lower.find(|c: char| c.is_whitespace() || c.is_ascii_digit()).unwrap_or(lower.len());
    let (word, rest) = lower.split_at(at);
    match PART_WORDS.iter().find(|(local, _)| *local == word) {
        Some((_, english)) if rest.trim().is_empty() => (*english).to_owned(),
        Some((_, english)) => format!("{english} {}", rest.trim()),
        None => lower,
    }
}

/// The group names a key jumps to, in lower case; empty for a key that jumps nowhere.
pub fn groups_for_key(key: char) -> &'static [&'static str] {
    let key = key.to_ascii_lowercase();
    GROUP_KEYS.iter().find(|(have, _)| *have == key).map_or(&[], |(_, names)| names)
}

/// The key that jumps to a group with this name, if there is one.
pub fn key_for_group(name: &str) -> Option<char> {
    let name = group_name_in_english(name);
    GROUP_KEYS.iter().find(|(_, names)| names.contains(&name.as_str())).map(|(key, _)| key.to_ascii_uppercase())
}

/// The colour bar shown for a group, chosen from its name so that every "Chorus" matches.
pub fn group_color(name: &str) -> Rgba {
    const PALETTE: [Rgba; 8] = [
        Rgba::rgb(66, 133, 244),
        // Not red: in the app red means "on screen now" and nothing else.
        Rgba::rgb(204, 78, 180),
        Rgba::rgb(244, 160, 0),
        Rgba::rgb(52, 168, 110),
        Rgba::rgb(155, 89, 182),
        Rgba::rgb(0, 172, 193),
        Rgba::rgb(120, 144, 156),
        Rgba::rgb(141, 110, 99),
    ];
    const KNOWN: [(&str, usize); 8] =
        [("verse", 0), ("chorus", 1), ("bridge", 2), ("pre", 3), ("intro", 4), ("ending", 5), ("outro", 5), ("tag", 6)];
    let lower = group_name_in_english(name);
    if lower.is_empty() {
        return Rgba::rgb(128, 128, 128);
    }
    if let Some((_, index)) = KNOWN.iter().find(|(prefix, _)| lower.starts_with(prefix)) {
        return PALETTE[*index];
    }
    // FNV-1a: stable across runs and platforms, unlike the standard hasher.
    let hash = lower.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    PALETTE[(hash % PALETTE.len() as u64) as usize]
}

/// Writes a song in the plain-text format that [`parse_song_text`] reads.
pub fn to_song_text(presentation: &Presentation) -> String {
    let mut out = String::new();
    for group in &presentation.groups {
        if !out.is_empty() {
            out.push('\n');
        }
        if !group.name.is_empty() {
            out.push_str(&format!("[{}]\n", group.name));
        }
        for (index, slide) in group.slides.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            let translation = slide.text_for(&Role::translation());
            if let Some(primary) = slide.text_for(&Role::primary()) {
                out.push_str(primary);
                out.push('\n');
            }
            if let Some(translation) = translation {
                out.push_str("--\n");
                out.push_str(translation);
                out.push('\n');
            }
        }
    }
    out
}

/// Tidies text typed into a box so it survives being saved: line endings are normalised,
/// trailing spaces go, and blank lines (which would start a new slide) are removed.
pub fn clean_box_text(text: &str) -> String {
    text.lines().map(str::trim_end).filter(|line| !line.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

#[derive(Default)]
struct SlideDraft {
    primary: Vec<String>,
    translation: Vec<String>,
    in_translation: bool,
}

impl SlideDraft {
    /// Ends the slide being collected and adds it to the group if it has any text.
    fn finish_into(&mut self, group: &mut Group) {
        let draft = std::mem::take(self);
        let mut boxes = Vec::new();
        if !draft.primary.is_empty() {
            boxes.push(Object::text(Role::primary(), draft.primary.join("\n")));
        }
        if !draft.translation.is_empty() {
            boxes.push(Object::text(Role::translation(), draft.translation.join("\n")));
        }
        if !boxes.is_empty() {
            group.slides.push(Slide::new(boxes));
        }
    }
}

fn group_header(trimmed: &str) -> Option<&str> {
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.trim())
}

fn new_group(name: &str) -> Group {
    Group { id: Uuid::new_v4(), name: name.to_owned(), color: group_color(name), slides: Vec::new() }
}

fn push_group(groups: &mut Vec<Group>, group: Group) {
    if !group.slides.is_empty() {
        groups.push(group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_named_in_russian_ukrainian_or_spanish_get_the_keys_and_colours_of_english_ones() {
        assert_eq!(group_name_in_english("Куплет 2"), "verse 2");
        assert_eq!(group_name_in_english("  Приспів: "), "chorus");
        assert_eq!(group_name_in_english("Estribillo"), "chorus");
        assert_eq!(group_name_in_english("Куплет2"), "verse 2");
        assert_eq!(group_name_in_english("Песня"), "песня", "any other name stays as it was");
        assert_eq!(key_for_group("Куплет 1"), Some('A'));
        assert_eq!(key_for_group("Куплет 3"), Some('D'));
        assert_eq!(key_for_group("Припев"), Some('C'));
        assert_eq!(key_for_group("Передприспів"), Some('P'));
        assert_eq!(key_for_group("Estrofa 2"), Some('S'));
        assert_eq!(group_color("Припев 2"), group_color("Chorus"));
        assert_eq!(group_color("Куплет"), group_color("Verse 4"));
        assert_eq!(group_color("Бридж"), group_color("Bridge"));
        // And as SoftProjector's songs name them.
        assert_eq!(group_color("Strophe 1"), group_color("Verse"));
        assert_eq!(group_color("Verš 2"), group_color("Verse"));
        assert_eq!(group_color("Refrén"), group_color("Chorus"));
        assert_eq!(key_for_group("Sbor"), Some('C'));
        assert_eq!(key_for_group("Refrain"), Some('C'));
        assert_eq!(key_for_group("Úvod"), Some('I'));
        assert_eq!(key_for_group("Závěr"), Some('E'));
    }

    #[test]
    fn group_keys_name_their_groups_both_ways() {
        assert_eq!(groups_for_key('A'), ["verse", "verse 1"]);
        assert_eq!(groups_for_key('c'), ["chorus", "chorus 1"]);
        assert!(groups_for_key('q').is_empty());
        assert_eq!(key_for_group(" Verse 2 "), Some('S'));
        assert_eq!(key_for_group("Chorus"), Some('C'));
        assert_eq!(key_for_group("Sermon"), None);
    }

    const SONG: &str =
        "[Verse 1]\nLine a\nLine b\n--\nСтрока а\nСтрока б\n\nSecond slide\n\n\n[Chorus]\nSing\n--\nПой\n";

    #[test]
    fn groups_and_slides() {
        let p = parse_song_text("T", SONG);
        assert_eq!(p.title, "T");
        assert_eq!(
            p.groups.iter().map(|g| (g.name.as_str(), g.slides.len())).collect::<Vec<_>>(),
            vec![("Verse 1", 2), ("Chorus", 1)]
        );
        assert_eq!(p.slide_count(), 3);
    }

    #[test]
    fn box_split() {
        let p = parse_song_text("T", SONG);
        let (_, s) = p.slide(0).unwrap();
        assert_eq!(s.text_for(&Role::primary()), Some("Line a\nLine b"));
        assert_eq!(s.text_for(&Role::translation()), Some("Строка а\nСтрока б"));
        let (_, s) = p.slide(1).unwrap();
        assert_eq!(s.objects.len(), 1);
    }

    #[test]
    fn text_before_any_header_goes_to_unnamed_group() {
        let p = parse_song_text("T", "Alone\n\n[Verse]\nX\n");
        assert_eq!(p.groups[0].name, "");
        assert_eq!(p.groups[0].slides.len(), 1);
    }

    #[test]
    fn windows_line_endings_and_trailing_blank_lines() {
        let p = parse_song_text("T", "[V]\r\nA\r\n--\r\nБ\r\n\r\n\r\n");
        assert_eq!(p.slide_count(), 1);
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("Б"));
    }

    #[test]
    fn empty_file_has_no_slides() {
        assert_eq!(parse_song_text("T", "").slide_count(), 0);
    }

    #[test]
    fn empty_side_of_split_makes_no_empty_box() {
        let p = parse_song_text("T", "A\n--\n\nB\n--\n");
        assert_eq!(p.slide(0).unwrap().1.objects.len(), 1);
        assert_eq!(p.slide(1).unwrap().1.objects.len(), 1);
        let p = parse_song_text("T", "--\nТолько перевод\n");
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("Только перевод"));
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::primary()), None);
    }

    #[test]
    fn empty_groups_are_dropped() {
        assert_eq!(parse_song_text("T", "[A]\n[B]\nX\n").groups.len(), 1);
    }

    #[test]
    fn same_group_name_gets_same_color() {
        assert_eq!(group_color("Chorus"), group_color("chorus 2"));
    }

    #[test]
    fn a_second_separator_is_text() {
        let p = parse_song_text("T", "A\n--\nB\n--\nC\n");
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("B\n--\nC"));
    }

    #[test]
    fn writing_then_reading_gives_the_same_song() {
        let p = parse_song_text(
            "T",
            "Loose\n--\nБез группы\n\n[Verse 1]\nA\nB\n--\nА\nБ\n\nOnly primary\n\n[Chorus]\n--\nТолько перевод\n",
        );
        let again = parse_song_text("T", &to_song_text(&p));
        let shape = |p: &Presentation| {
            p.groups
                .iter()
                .map(|g| (g.name.clone(), g.slides.iter().map(Slide::texts).collect::<Vec<_>>()))
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(&p), shape(&again));
        assert_eq!(to_song_text(&p), to_song_text(&again));
    }

    #[test]
    fn cleaning_box_text_removes_what_would_break_the_file() {
        assert_eq!(clean_box_text("  one  \r\n\n\n two\n   \n"), "  one\n two");
        assert_eq!(clean_box_text(" \n\n"), "");
    }

    #[test]
    fn replacing_a_slides_text_keeps_ids_and_everything_else() {
        let p = parse_song_text("T", "[V]\nA\n--\nБ\n\nC\n\n[Ch]\nD\n");
        let edited = p.with_slide_text(1, "New words\n\nsecond line", "Перевод").unwrap();
        assert_eq!(edited.id, p.id);
        assert_eq!(edited.slide(1).unwrap().1.id, p.slide(1).unwrap().1.id);
        // The object that was there keeps its id, so anything pointing at it still does.
        assert_eq!(edited.slide(1).unwrap().1.objects[0].id, p.slide(1).unwrap().1.objects[0].id);
        assert_eq!(edited.slide(1).unwrap().1.text_for(&Role::primary()), Some("New words\nsecond line"));
        assert_eq!(edited.slide(1).unwrap().1.text_for(&Role::translation()), Some("Перевод"));
        assert_eq!(edited.slide(0).unwrap().1, p.slide(0).unwrap().1);
        assert_eq!(edited.slide(2).unwrap().1, p.slide(2).unwrap().1);
        // Emptying the translation removes that box; emptying both is refused.
        let one_box = p.with_slide_text(0, "A", "  ").unwrap();
        assert_eq!(one_box.slide(0).unwrap().1.objects.len(), 1);
        assert!(p.with_slide_text(0, "", "\n").is_none());
        assert!(p.with_slide_text(9, "A", "").is_none());
    }

    #[test]
    fn a_whole_slide_can_be_replaced() {
        let p = parse_song_text("T", "[V]\nA\n\nB\n\n[C]\nD\n");
        let mut edited = p.slide(2).unwrap().1.clone();
        edited.objects.clear();
        let q = p.with_slide(2, edited.clone()).unwrap();
        assert_eq!(q.slide(2).unwrap().1, &edited);
        assert_eq!(q.slide(0).unwrap().1, p.slide(0).unwrap().1);
        assert_eq!(q.slide_count(), 3, "an empty slide is still a slide");
        assert!(p.with_slide(3, edited).is_none());
    }

    #[test]
    fn flat_slide_index_crosses_groups() {
        let p = parse_song_text("T", SONG);
        assert_eq!(p.slide(2).unwrap().0.name, "Chorus");
        assert!(p.slide(3).is_none());
    }
}
