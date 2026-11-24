//! Songs from text written elsewhere: lyric files downloaded from a song service, ChordPro
//! files, songs typed in a word processor, words pasted in. Whatever the source, the words are
//! turned into SoftPresenter's own song text (see [`crate::parse_song_text`]) and read from
//! that, so an imported song's slides are made exactly as a hand-written one's are.
//!
//! What is understood:
//! - SoftPresenter's own song text (`[Verse 1]` headers, `--` before a translation): read as it
//!   is, unchanged, slide for slide.
//! - Plain text whose sections start with a line of their own naming them ("Verse 1", "Chorus",
//!   "Bridge", "Pre-Chorus", "Tag"…, or in Russian and Ukrainian "Куплет 1", "Припев",
//!   "Приспів"…), with or without a colon.
//! - Lyric files as song services give them: the title on the first line, the sections, and a
//!   footer with the song's catalogue number, its writers and its copyright line, which go into
//!   the song's details. A church's own licence number in a footer is never kept.
//! - ChordPro: `{title: …}`, `{artist: …}`, `{copyright: …}` and `{ccli: …}`; sections from
//!   `{start_of_chorus}`, `{start_of_verse: Verse 2}`, `{start_of_bridge}` and section-naming
//!   `{comment: …}` lines; chords in square brackets are taken out of the words.
//!
//! Slides break at blank lines, and a stretch of more than four lines without one is shared out
//! into slides of about equal length (eight lines make two slides of four).

use crate::{Copyright, Presentation, parse_song_text};

/// The most lines a slide gets when a section's lines run on without a blank line.
pub const MOST_LINES: usize = 4;

/// One line of a song being imported.
#[derive(Debug, PartialEq)]
enum Line {
    /// The start of a section, by its name.
    Section(String),
    Words(String),
    Blank,
}

/// Reads a song from text written anywhere. `title` is used when the text doesn't give one
/// (the file's name, say).
pub fn import_song_text(title: &str, text: &str) -> Presentation {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    if is_chordpro(&text) {
        return from_chordpro(title, &text);
    }
    if is_song_text(&text) {
        return parse_song_text(title, &text);
    }
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    // The footer, from the line with the song's catalogue number to the end.
    let footer_at = lines.iter().position(|line| is_footer_start(line)).unwrap_or(lines.len());
    let (body, footer) = lines.split_at(footer_at);
    // A title: the first line, when the next line with words names a section. Without sections
    // there is no telling a title from a first line of words, so nothing is taken.
    let filled: Vec<usize> = (0..body.len()).filter(|&at| !body[at].trim().is_empty()).collect();
    let given_title = match filled.as_slice() {
        [first, second, ..] if section_name(body[*first]).is_none() && section_name(body[*second]).is_some() => {
            Some((*first, body[*first].trim().to_owned()))
        }
        _ => None,
    };
    let start = given_title.as_ref().map_or(0, |(at, _)| at + 1);
    let marked = body[start..].iter().map(|line| match section_name(line) {
        Some(name) => Line::Section(name),
        None if line.trim().is_empty() => Line::Blank,
        None => Line::Words(line.trim().to_owned()),
    });
    let mut song = parse_song_text(given_title.as_ref().map_or(title, |(_, t)| t.as_str()), &to_song_text(marked));
    song.copyright = footer_details(footer);
    song
}

/// True when the text is already SoftPresenter's own song text.
fn is_song_text(text: &str) -> bool {
    text.lines().map(str::trim).any(|line| {
        line == "--" || (line.len() > 2 && line.starts_with('[') && line.ends_with(']') && !line[1..].contains('['))
    })
}

/// The section a line names, tidied ("CHORUS:" gives "Chorus"), or `None` for a line of words.
pub fn section_name(line: &str) -> Option<String> {
    const NAMES: [&str; 33] = [
        "verse",
        "chorus",
        "pre-chorus",
        "prechorus",
        "pre chorus",
        "bridge",
        "tag",
        "intro",
        "outro",
        "ending",
        "interlude",
        "refrain",
        "coda",
        "instrumental",
        "vamp",
        "turnaround",
        "misc",
        "куплет",
        "припев",
        "приспів",
        "предприпев",
        "пре-припев",
        "бридж",
        "мост",
        "міст",
        "вступ",
        "вступление",
        "кода",
        "окончание",
        "кінцівка",
        "проигрыш",
        "програш",
        "тег",
    ];
    let trimmed = line.trim().trim_end_matches(':').trim();
    if trimmed.is_empty() || trimmed.chars().count() > 24 {
        return None;
    }
    let lower = trimmed.to_lowercase();
    // A name, then perhaps a number or a letter ("Verse 2", "Chorus 1b", "Куплет 3").
    let name = NAMES.iter().filter(|name| lower.starts_with(*name)).max_by_key(|name| name.len())?;
    let rest = lower[name.len()..].trim();
    if !(rest.chars().count() <= 3 && rest.chars().all(char::is_alphanumeric)) {
        return None;
    }
    // As people write it: each word capitalised.
    let word = |word: &str| {
        let mut chars = word.chars();
        chars
            .next()
            .map_or_else(String::new, |first| first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect())
    };
    Some(trimmed.split_whitespace().map(word).collect::<Vec<_>>().join(" "))
}

/// The line a lyric file's footer starts at: the song's catalogue number.
fn is_footer_start(line: &str) -> bool {
    let lower = line.trim().to_lowercase();
    lower.starts_with("ccli song #") || lower.starts_with("ccli song number") || lower.starts_with("ccli song no")
}

/// True for a line that holds a church's licence number or a service's terms of use, which
/// are about the church, not the song, and are never kept.
fn is_licence_line(lower: &str) -> bool {
    lower.starts_with("ccli licen") || lower.contains("terms of use") || lower.starts_with("for use solely")
}

/// The song's details from a lyric file's footer: its catalogue number, its writers and its
/// copyright line ("© 2019 A Publisher" gives the year and the publisher).
fn footer_details(footer: &[&str]) -> Copyright {
    let mut details = Copyright::default();
    for line in footer.iter().map(|line| line.trim()).filter(|line| !line.is_empty()) {
        if is_footer_start(line) {
            details.ccli_song = line.chars().filter(char::is_ascii_digit).collect();
        } else if is_licence_line(&line.to_lowercase()) {
            continue;
        } else if let Some(rest) = copyright_text(line) {
            (details.year, details.publisher) = year_and_owner(rest);
        } else if details.author.is_empty() {
            details.author = line.to_owned();
        }
    }
    details
}

/// The text after a copyright mark, for a copyright line.
fn copyright_text(line: &str) -> Option<&str> {
    line.strip_prefix('©')
        .or_else(|| line.strip_prefix("(c)"))
        .or_else(|| line.strip_prefix("(C)"))
        .or_else(|| line.strip_prefix("Copyright"))
        .map(str::trim)
}

/// A copyright line ("© 2019 A Publisher", or the same without the mark) as its year and its
/// publisher; the year is empty when the line doesn't give one first.
pub fn year_and_publisher(line: &str) -> (String, String) {
    let line = line.trim();
    year_and_owner(copyright_text(line).unwrap_or(line))
}

/// "2019 A Publisher" as its year and owner; the year is empty when the line doesn't start
/// with one.
fn year_and_owner(text: &str) -> (String, String) {
    let text = text.trim();
    let year = text.split_whitespace().next().filter(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()));
    match year {
        Some(year) => (year.to_owned(), text[year.len()..].trim().to_owned()),
        None => (String::new(), text.to_owned()),
    }
}

/// Writes marked lines as SoftPresenter's own song text: a `[Name]` line for each section,
/// slides at blank lines, and long stretches shared out into slides.
fn to_song_text(lines: impl Iterator<Item = Line>) -> String {
    let mut out = String::new();
    let mut stretch: Vec<String> = Vec::new();
    let flush = |stretch: &mut Vec<String>, out: &mut String| {
        for slide in share_out(stretch, MOST_LINES) {
            out.push_str(&slide.join("\n"));
            out.push_str("\n\n");
        }
        stretch.clear();
    };
    for line in lines {
        match line {
            Line::Section(name) => {
                flush(&mut stretch, &mut out);
                out.push_str(&format!("[{name}]\n"));
            }
            Line::Blank => flush(&mut stretch, &mut out),
            // A line of its own that is only "--" would start a translation; keep it as words.
            Line::Words(words) if words == "--" => stretch.push("—".to_owned()),
            Line::Words(words) => stretch.push(words),
        }
    }
    flush(&mut stretch, &mut out);
    out
}

/// A stretch of lines as slides of at most `most` lines (at least one), as equal as they can be:
/// with four, nine lines make slides of three, three and three.
pub fn share_out(lines: &[String], most: usize) -> Vec<&[String]> {
    if lines.is_empty() {
        return Vec::new();
    }
    let slides = lines.len().div_ceil(most.max(1));
    let (base, extra) = (lines.len() / slides, lines.len() % slides);
    let mut out = Vec::new();
    let mut at = 0;
    for n in 0..slides {
        let take = base + usize::from(n < extra);
        out.push(&lines[at..at + take]);
        at += take;
    }
    out
}

/// True for ChordPro text: a line that is a ChordPro directive SoftPresenter knows.
fn is_chordpro(text: &str) -> bool {
    text.lines().map(str::trim).any(|line| {
        line.starts_with('{') && line.ends_with('}') && {
            let name = directive(line).0;
            matches!(
                name.as_str(),
                "title" | "t" | "soc" | "start_of_chorus" | "sov" | "start_of_verse" | "sob" | "start_of_bridge"
            )
        }
    })
}

/// A ChordPro directive line as its name, in lower case, and its value.
fn directive(line: &str) -> (String, &str) {
    let inner = line.trim().trim_start_matches('{').trim_end_matches('}').trim();
    match inner.split_once([':', ' ']) {
        Some((name, value)) => (name.trim().to_lowercase(), value.trim()),
        None => (inner.to_lowercase(), ""),
    }
}

/// A ChordPro song.
fn from_chordpro(fallback: &str, text: &str) -> Presentation {
    let mut title = None;
    let mut details = Copyright::default();
    let mut lines = Vec::new();
    let mut skipping = false;
    let mut verses = 0;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with('{') && line.ends_with('}') {
            let (name, value) = directive(line);
            match name.as_str() {
                "title" | "t" => title = Some(value.to_owned()).filter(|v| !v.is_empty()),
                "artist" | "composer" | "lyricist" | "subtitle" | "st" if details.author.is_empty() => {
                    details.author = value.to_owned();
                }
                "copyright" => (details.year, details.publisher) = year_and_publisher(value),
                "ccli" => details.ccli_song = value.chars().filter(char::is_ascii_digit).collect(),
                "soc" | "start_of_chorus" => lines.push(Line::Section(label(value, "Chorus"))),
                "sob" | "start_of_bridge" => lines.push(Line::Section(label(value, "Bridge"))),
                "sov" | "start_of_verse" => {
                    verses += 1;
                    lines.push(Line::Section(label(value, &format!("Verse {verses}"))));
                }
                "c" | "comment" | "ci" | "comment_italic" | "cb" | "comment_box" => {
                    if let Some(section) = section_name(value) {
                        lines.push(Line::Section(section));
                    }
                }
                // Chord diagrams and tablature have no words to show.
                "sot" | "start_of_tab" | "start_of_grid" | "sog" => skipping = true,
                "eot" | "end_of_tab" | "end_of_grid" | "eog" => skipping = false,
                "eoc" | "end_of_chorus" | "eov" | "end_of_verse" | "eob" | "end_of_bridge" => lines.push(Line::Blank),
                _ => {}
            }
            continue;
        }
        if skipping {
            continue;
        }
        let words = without_chords(line);
        if words.is_empty() {
            lines.push(Line::Blank);
        } else if let Some(section) = section_name(&words) {
            lines.push(Line::Section(section));
        } else {
            lines.push(Line::Words(words));
        }
    }
    let mut song = parse_song_text(title.as_deref().unwrap_or(fallback), &to_song_text(lines.into_iter()));
    song.copyright = details;
    song
}

/// A section's own label from a ChordPro directive, or the usual name when it has none.
fn label(value: &str, usual: &str) -> String {
    let value = value.trim().trim_start_matches("label=").trim_matches('"').trim();
    if value.is_empty() { usual.to_owned() } else { value.to_owned() }
}

/// A line of ChordPro words without the chords in square brackets, and without the spaces
/// left where a chord stood alone.
pub fn without_chords(line: &str) -> String {
    let mut out = String::new();
    let mut in_chord = false;
    for c in line.chars() {
        match c {
            '[' => in_chord = true,
            ']' if in_chord => in_chord = false,
            _ if !in_chord => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Text from a file's bytes, whatever encoding it was saved in: UTF-8 (with or without a byte
/// order mark), UTF-16 with a byte order mark, or else one of the two older Windows encodings:
/// Windows-1251 for Russian and Ukrainian, Windows-1252 for Western European languages.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |little: bool, rest: &[u8]| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| if little { u16::from_le_bytes(pair) } else { u16::from_be_bytes(pair) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(true, rest);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(false, rest);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    // In Cyrillic text nearly every letter is a byte above 0xBF (or Ё, ё and the Ukrainian
    // letters below it); in Western European text those are the occasional accented letter
    // among plain ones.
    let cyrillic =
        |b: u8| b >= 0xC0 || matches!(b, 0xA5 | 0xA8 | 0xAA | 0xAF | 0xB2 | 0xB3 | 0xB4 | 0xB8 | 0xBA | 0xBF);
    let high = bytes.iter().filter(|&&b| cyrillic(b)).count();
    let plain = bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    if high > plain {
        bytes.iter().map(|&b| windows_1251(b)).collect()
    } else {
        bytes.iter().map(|&b| windows_1252(b)).collect()
    }
}

/// One byte of Windows-1251 as the character it stands for.
pub(crate) fn windows_1251(byte: u8) -> char {
    const HIGH: [char; 64] = [
        'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', 'ђ', '‘', '’', '“', '”', '•',
        '–', '—', '\u{fffd}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', '\u{a0}', 'Ў', 'ў', 'Ј', '¤', 'Ґ', '¦', '§', 'Ё', '©',
        'Є', '«', '¬', '\u{ad}', '®', 'Ї', '°', '±', 'І', 'і', 'ґ', 'µ', '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ',
        'ї',
    ];
    match byte {
        0x00..=0x7F => char::from(byte),
        0x80..=0xBF => HIGH[usize::from(byte - 0x80)],
        // А (U+0410) to я (U+044F), in order.
        _ => char::from_u32(0x0410 + u32::from(byte - 0xC0)).unwrap_or('\u{fffd}'),
    }
}

/// One byte of Windows-1252 as the character it stands for.
pub(crate) fn windows_1252(byte: u8) -> char {
    const MIDDLE: [char; 32] = [
        '€', '\u{fffd}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{fffd}', 'Ž', '\u{fffd}',
        '\u{fffd}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{fffd}', 'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => MIDDLE[usize::from(byte - 0x80)],
        // Everything else is the same as Unicode's first 256 characters.
        _ => char::from(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slides(song: &Presentation) -> Vec<(String, Vec<String>)> {
        song.groups
            .iter()
            .flat_map(|group| {
                group.slides.iter().map(move |slide| {
                    (group.name.clone(), slide.texts().into_iter().map(|(_, text)| text).collect::<Vec<_>>())
                })
            })
            .collect()
    }

    #[test]
    fn songs_in_softpresenters_own_text_read_exactly_as_before() {
        for text in [
            crate::to_song_text(&parse_song_text(
                "x",
                "[Verse 1]\nOne line\nTwo line\n--\nОдна строка\n\n[Chorus]\nSing\n",
            )),
            "[Verse 1]\nA\nB\nC\nD\nE\nF\n".to_owned(),
            "Only words\n--\nТолько слова\n".to_owned(),
            include_str!("../../../samples/sample-song.txt").to_owned(),
        ] {
            assert_eq!(slides(&import_song_text("T", &text)), slides(&parse_song_text("T", &text)), "{text}");
            assert_eq!(import_song_text("T", &text).title, "T");
        }
    }

    #[test]
    fn a_lyric_file_gives_its_title_sections_slides_and_details_but_never_the_church_licence() {
        // Written for this test, in the shape such files have.
        let text = "Lantern Song\r\n\r\nVerse 1\r\nA light along the shore\r\nA road we walked before\r\nA song upon the wind\r\nA home to come back in\r\nThe night is nearly done\r\nThe morning has begun\r\n\r\nChorus\r\nShine on, shine on\r\nUntil the dark is gone\r\n\r\nCCLI Song # 1234567\r\nAnna Writer | Ben Writer\r\n© 2021 Harbour Music\r\nFor use solely with the SongSelect® Terms of Use. All rights reserved. www.ccli.com\r\nCCLI License # 7654321\r\n";
        let song = import_song_text("file name", text);
        assert_eq!(song.title, "Lantern Song");
        let got = slides(&song);
        assert_eq!(got.len(), 3, "six lines share out as three and three; the chorus is one slide");
        assert_eq!(got[0].0, "Verse 1");
        assert_eq!(got[0].1[0].lines().count(), 3);
        assert_eq!(got[2].0, "Chorus");
        assert_eq!(song.copyright.ccli_song, "1234567");
        assert_eq!(song.copyright.author, "Anna Writer | Ben Writer");
        assert_eq!((song.copyright.year.as_str(), song.copyright.publisher.as_str()), ("2021", "Harbour Music"));
        assert!(!format!("{song:?}").contains("7654321"), "the church's licence number is not kept");
    }

    #[test]
    fn plain_text_with_section_lines_in_russian_and_without_a_title() {
        let text = "Куплет 1:\nСвет над рекой\nПесня над тобой\n\nПРИПЕВ\nДомой, домой\n";
        let song = import_song_text("Река", text);
        assert_eq!(song.title, "Река", "no title line: the given one");
        let got = slides(&song);
        assert_eq!((got[0].0.as_str(), got[1].0.as_str()), ("Куплет 1", "Припев"));
    }

    #[test]
    fn words_with_no_sections_keep_their_first_line_and_break_at_blank_lines() {
        let song = import_song_text("Words", "Hold on\n\nthree\nfour\n");
        assert_eq!(song.title, "Words", "a first line on its own isn't taken for a title without sections");
        assert_eq!(slides(&song).len(), 2);
        assert_eq!(slides(&import_song_text("W", "a\nb\nc\nd\ne\nf\ng\nh\n")).len(), 2, "eight lines make two slides");
    }

    #[test]
    fn chordpro_gives_title_sections_and_words_without_chords() {
        let text = "# made for this test\n{title: River Song}\n{artist: Anna Writer}\n{key: G}\n\n{comment: Verse 1}\n[G]Down by the [C]river\nWe [D]sing  [G]\n\n{start_of_chorus}\nFlow on, [Em]flow on\n{end_of_chorus}\n\n{sot}\ne|---0---|\n{eot}\n{start_of_verse: Verse 2}\nUp on the hill\n{end_of_verse}\n";
        let song = import_song_text("file", text);
        assert_eq!(song.title, "River Song");
        assert_eq!(song.copyright.author, "Anna Writer");
        let got = slides(&song);
        assert_eq!(got[0], ("Verse 1".to_owned(), vec!["Down by the river\nWe sing".to_owned()]));
        assert_eq!(got[1], ("Chorus".to_owned(), vec!["Flow on, flow on".to_owned()]));
        assert_eq!(got[2].0, "Verse 2");
        assert_eq!(got.len(), 3, "tablature has no words to show");
    }

    #[test]
    fn long_stretches_share_out_evenly() {
        let lines: Vec<String> = (1..=9).map(|n| n.to_string()).collect();
        let sizes = |n: usize| share_out(&lines[..n], MOST_LINES).iter().map(|s| s.len()).collect::<Vec<_>>();
        assert_eq!(sizes(9), vec![3, 3, 3]);
        assert_eq!(sizes(5), vec![3, 2]);
        assert_eq!(sizes(4), vec![4]);
        // Another most: six lines a slide keep eight lines as two of four.
        assert_eq!(share_out(&lines[..8], 6).iter().map(|s| s.len()).collect::<Vec<_>>(), vec![4, 4]);
        assert_eq!(share_out(&lines[..3], 0).len(), 3, "a slide has at least one line");
    }

    #[test]
    fn text_is_read_in_utf8_utf16_or_the_older_windows_encodings() {
        assert_eq!(decode_text("Свет".as_bytes()), "Свет");
        assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, b'h', b'i']), "hi");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
        // "Свет над" and "Ёё" in Windows-1251.
        assert_eq!(decode_text(&[0xD1, 0xE2, 0xE5, 0xF2, b' ', 0xED, 0xE0, 0xE4]), "Свет над");
        assert_eq!(decode_text(&[0xA8, 0xB8]), "Ёё");
        // "Café crème “hi”" in Windows-1252.
        assert_eq!(decode_text(b"Caf\xe9 cr\xe8me \x93hi\x94"), "Café crème “hi”");
    }
}
