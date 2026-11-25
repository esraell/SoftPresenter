//! Tidying a song's title and words: what an import does to every song it brings in, and what
//! `sp library tidy` does to songs already in the library. The same rules for both, so a song
//! tidied in the library and the same song imported again are the same song.
//!
//! The title: a garbled one is put back ([`crate::repair_garbled`]); letters written as a letter
//! and a mark are joined into one (Unicode's NFC), odd spaces made plain and runs of them one; a
//! file's ending (".docx") is taken off; so are a comma, semicolon or colon at its end (left by
//! a title taken from a line); "Untitled", or no title, becomes the song's first line; and a title
//! that is only a number (a hymnbook's, perhaps with a counter after it) becomes "619 - its first
//! line", as hymnbooks' songs are often named, so the number still finds it.
//!
//! The words of the main and translation boxes (and the reference's letters): letters joined
//! (NFC), line and paragraph separators made line breaks, non-breaking spaces and tabs made
//! spaces, zero-width characters taken out between letters, runs of spaces made one, and no
//! spaces at a line's start or end. A blank line, or a line of only "* * *", "***" or "---",
//! between words parts the slide in two there (the parting line goes); one at a box's start or end
//! just goes. A slide of more than [`MOST_TIDY_LINES`] lines is shared out into slides of at most
//! that many, as even as they can be ([`crate::share_out`], as an import shares out a long
//! stretch). A slide is only parted when its translation parts the same way (as many stanzas,
//! or as many lines); otherwise it is left whole, and said. Repeat marks ("2x"), group names and
//! boxes with no role (a presentation's) are left as they are.
//!
//! A slide parted keeps its id, label, notes, actions and hot key on its first part, so
//! arrangements, what is live and anything pointing at it stay with it; each further part is a
//! copy of it (pictures, shapes and the reference too) with ids of its own.

use std::borrow::Cow;

use crate::{Object, Presentation, Role, Slide, Uuid, share_out};

/// The most lines a slide has after tidying.
pub const MOST_TIDY_LINES: usize = 8;
/// The longest first line taken whole for a title; a longer one is cut at a word.
const LONGEST_TITLE_LINE: usize = 80;

/// What tidying may leave out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TidyOptions {
    /// Leave a title that is only a number as it is, rather than adding the first line to it.
    pub keep_numbers: bool,
}

/// What was done to a title. A title may have had several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TitleFix {
    /// Garbled letters put back.
    Garbled,
    /// Spaces made plain or fewer, or letters written in two parts joined.
    Letters,
    /// A file's ending taken off.
    Ending,
    /// A comma, semicolon or colon taken off its end.
    Punctuation,
    /// "Untitled", or no title, became the first line.
    Untitled,
    /// A number alone became the number and the first line.
    Number,
}

impl TitleFix {
    /// A short word for each, for JSON.
    pub fn word(self) -> &'static str {
        match self {
            TitleFix::Garbled => "garbled",
            TitleFix::Letters => "letters",
            TitleFix::Ending => "ending",
            TitleFix::Punctuation => "punctuation",
            TitleFix::Untitled => "untitled",
            TitleFix::Number => "number",
        }
    }
}

/// The characters put right in a song's words, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharFixes {
    /// Letters written as a letter and a mark (и and a breve) joined into one (й).
    pub joined: usize,
    /// Line or paragraph separators (and carriage returns) made line breaks.
    pub line_breaks: usize,
    /// Non-breaking and other spaces made plain spaces.
    pub odd_spaces: usize,
    pub tabs: usize,
    /// Zero-width characters taken out.
    pub invisible: usize,
    /// Runs of spaces inside a line made one.
    pub space_runs: usize,
    /// Lines with spaces at their start or end.
    pub trimmed: usize,
    /// Lines of only "* * *", "***", "---" and the like taken out.
    pub separators: usize,
    /// Blank lines taken out of a box.
    pub blank_lines: usize,
}

impl CharFixes {
    pub fn is_empty(&self) -> bool {
        *self == CharFixes::default()
    }

    /// The kinds and their counts, with a short word for each, for JSON.
    pub fn counts(&self) -> [(&'static str, usize); 9] {
        [
            ("joined_letters", self.joined),
            ("line_separators", self.line_breaks),
            ("odd_spaces", self.odd_spaces),
            ("tabs", self.tabs),
            ("zero_width", self.invisible),
            ("space_runs", self.space_runs),
            ("trimmed_lines", self.trimmed),
            ("separator_lines", self.separators),
            ("blank_lines", self.blank_lines),
        ]
    }

    /// Adds another song's counts to these.
    pub fn add(&mut self, other: CharFixes) {
        self.joined += other.joined;
        self.line_breaks += other.line_breaks;
        self.odd_spaces += other.odd_spaces;
        self.tabs += other.tabs;
        self.invisible += other.invisible;
        self.space_runs += other.space_runs;
        self.trimmed += other.trimmed;
        self.separators += other.separators;
        self.blank_lines += other.blank_lines;
    }
}

/// Why a slide was parted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitWhy {
    /// A line of only "* * *" or the like between its words.
    Separator,
    /// A blank line between its words.
    Blank,
    /// It had this many lines.
    Long(usize),
}

/// A slide parted into several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    /// The slide's number in the song as written, from 1, before tidying.
    pub slide: usize,
    /// How many slides it became.
    pub into: usize,
    pub why: SplitWhy,
}

/// Why a slide that would be parted was left whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WholeWhy {
    /// Its translation doesn't part the same way (another number of stanzas and lines).
    Translation,
    /// Some of its words are styled on their own.
    Styled,
    /// It has more than one box of main words, or of translation.
    Boxes,
}

/// A slide left whole, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeftWhole {
    /// The slide's number in the song as written, from 1, before tidying.
    pub slide: usize,
    pub why: WholeWhy,
}

/// A song tidied, and what was done to it.
#[derive(Clone, Debug)]
pub struct Tidied {
    pub song: Presentation,
    pub title_fixes: Vec<TitleFix>,
    pub splits: Vec<Split>,
    pub left_whole: Vec<LeftWhole>,
    pub chars: CharFixes,
}

/// Tidies a song as the module's notes say. The song keeps its id, its groups' ids (so its
/// arrangements), its theme and everything else.
pub fn tidy_song(song: &Presentation, options: TidyOptions) -> Tidied {
    let mut out = song.clone();
    let mut chars = CharFixes::default();
    let mut splits = Vec::new();
    let mut left_whole = Vec::new();
    let mut number = 0;
    for group in &mut out.groups {
        let mut slides = Vec::with_capacity(group.slides.len());
        for slide in std::mem::take(&mut group.slides) {
            number += 1;
            let (parts, outcome) = tidy_slide(slide, &mut chars);
            match outcome {
                Outcome::Same => {}
                Outcome::Split(why) => splits.push(Split { slide: number, into: parts.len(), why }),
                Outcome::Whole(why) => left_whole.push(LeftWhole { slide: number, why }),
            }
            slides.extend(parts);
        }
        group.slides = slides;
    }
    let first = first_line(&out);
    let (title, title_fixes) = tidy_title(&song.title, first.as_deref(), options);
    out.title = title;
    Tidied { song: out, title_fixes, splits, left_whole, chars }
}

/// A title tidied as the module's notes say, with what was done to it. `first_line` is the
/// song's first line of words (see [`first_line`]), for a title that says nothing.
pub fn tidy_title(title: &str, first_line: Option<&str>, options: TidyOptions) -> (String, Vec<TitleFix>) {
    let mut fixes = Vec::new();
    let joined = nfc(title);
    let mut text = joined.clone().into_owned();
    if let Some(repaired) = crate::repair_garbled(&text) {
        fixes.push(TitleFix::Garbled);
        text = nfc(&repaired).into_owned();
    }
    let plain = one_line(&text);
    if plain != text || (joined != title && !fixes.contains(&TitleFix::Garbled)) {
        fixes.push(TitleFix::Letters);
    }
    text = plain;
    // An ending and the punctuation before or after it, whichever way round, until neither is left.
    loop {
        let before = text.clone();
        if let Some(bare) = without_ending(&text) {
            push_once(&mut fixes, TitleFix::Ending);
            text = bare.trim_end().to_owned();
        }
        let bare = text.trim_end_matches([',', ';', ':', ' ']);
        if bare.len() != text.len() && !bare.is_empty() {
            push_once(&mut fixes, TitleFix::Punctuation);
            text = bare.to_owned();
        }
        if text == before {
            break;
        }
    }
    let first = first_line.map(title_line).filter(|line| !line.is_empty());
    if let Some(first) = &first {
        if is_untitled(&text) {
            fixes.push(TitleFix::Untitled);
            text = first.clone();
        } else if !options.keep_numbers
            && let Some(number) = hymn_number(&text)
            && first.chars().any(char::is_alphabetic)
        {
            fixes.push(TitleFix::Number);
            text = format!("{number} - {first}");
        }
    }
    fixes.sort();
    (text, fixes)
}

fn push_once(fixes: &mut Vec<TitleFix>, fix: TitleFix) {
    if !fixes.contains(&fix) {
        fixes.push(fix);
    }
}

/// The endings of files songs come in, taken off a title that ends with one.
const ENDINGS: [&str; 30] = [
    "docx",
    "doc",
    "odt",
    "rtf",
    "txt",
    "text",
    "pro",
    "pro4",
    "pro5",
    "pro6",
    "probundle",
    "pptx",
    "ppt",
    "ppsx",
    "pps",
    "key",
    "odp",
    "pdf",
    "xml",
    "sbsong",
    "song",
    "sng",
    "usr",
    "cho",
    "chordpro",
    "chopro",
    "crd",
    "sps",
    "sppres",
    "pcpres",
];

/// A title without the file ending it ends with, if it ends with one after a name.
fn without_ending(title: &str) -> Option<&str> {
    let (name, ending) = title.rsplit_once('.')?;
    let known = ENDINGS.iter().any(|known| known.eq_ignore_ascii_case(ending));
    (known && !name.trim().is_empty()).then_some(name)
}

/// Whether a title says the song has none.
fn is_untitled(title: &str) -> bool {
    let lower = title.trim().to_lowercase();
    if lower.is_empty() {
        return true;
    }
    // "Untitled 2" too, but never a number alone.
    let bare = lower.trim_end_matches(|c: char| c.is_ascii_digit() || matches!(c, ' ' | '-' | '_'));
    ["untitled", "без названия", "без назви", "sin título", "sin titulo"].contains(&bare)
}

/// The number of a title that is only a number: a hymnbook's, perhaps with a counter after it
/// ("100 4", "146-translated 1", "2854 -").
fn hymn_number(title: &str) -> Option<&str> {
    let digits = title.find(|c: char| !c.is_ascii_digit()).unwrap_or(title.len());
    if digits == 0 || digits > 4 {
        return None;
    }
    let (number, rest) = title.split_at(digits);
    let rest = rest.to_lowercase().replacen("translated", "", 1);
    let only_marks = rest.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == '-');
    (only_marks && rest.chars().filter(char::is_ascii_digit).count() <= 3).then_some(number)
}

/// A song's first line of words: the first line of main words of its first slide that has
/// any, as written, leaving out parting lines.
pub fn first_line(song: &Presentation) -> Option<String> {
    song.groups.iter().flat_map(|group| &group.slides).find_map(|slide| {
        let text = slide.text_for(&Role::primary())?;
        text.lines().map(str::trim).find(|line| !line.is_empty() && !is_separator(line)).map(str::to_owned)
    })
}

/// A line of words made fit to be a title: tidied, without a comma, semicolon or colon at its
/// end, and cut at a word when it is long.
fn title_line(line: &str) -> String {
    let line = one_line(&nfc(line));
    let line = line.trim_end_matches([',', ';', ':', ' ']);
    if line.chars().count() <= LONGEST_TITLE_LINE {
        return line.to_owned();
    }
    let cut: String = line.chars().take(LONGEST_TITLE_LINE).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head).trim_end_matches([',', ';', ':', ' ']);
    format!("{cut}\u{2026}")
}

/// Text in Unicode's NFC: a letter and a combining mark that make one letter written as that
/// letter.
fn nfc(text: &str) -> Cow<'_, str> {
    icu_normalizer::ComposingNormalizerBorrowed::new_nfc().normalize(text)
}

/// Spaces other than the plain one: non-breaking, narrow, figure, ideographic and the rest.
fn is_odd_space(c: char) -> bool {
    matches!(c, '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// Zero-width characters always taken out: the zero-width space, the word joiner and a byte
/// order mark inside text.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200b}' | '\u{2060}' | '\u{feff}')
}

/// The zero-width joiner and non-joiner, which matter beside an emoji or in some scripts and
/// are only taken out between letters of the alphabets songs here are written in.
fn is_joiner(c: char) -> bool {
    matches!(c, '\u{200c}' | '\u{200d}')
}

/// A letter of the Latin, Greek or Cyrillic alphabets, or a space: where a joiner does nothing.
fn joiner_does_nothing_beside(c: Option<char>) -> bool {
    c.is_none_or(|c| c == ' ' || c == '\n' || (c.is_alphabetic() && u32::from(c) < 0x0530))
}

/// Odd spaces and tabs made plain spaces, and zero-width characters taken out where they do
/// nothing; line breaks are kept.
fn plain_characters(text: &str, fixes: &mut CharFixes) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let near = |from: usize, step: isize| {
        let mut at = from as isize + step;
        while at >= 0 && (at as usize) < chars.len() {
            let c = chars[at as usize];
            if !(is_invisible(c) || is_joiner(c)) {
                return Some(c);
            }
            at += step;
        }
        None
    };
    for (at, &c) in chars.iter().enumerate() {
        if is_odd_space(c) {
            fixes.odd_spaces += 1;
            out.push(' ');
        } else if c == '\t' {
            fixes.tabs += 1;
            out.push(' ');
        } else if is_invisible(c)
            || (is_joiner(c) && joiner_does_nothing_beside(near(at, -1)) && joiner_does_nothing_beside(near(at, 1)))
        {
            fixes.invisible += 1;
        } else {
            out.push(c);
        }
    }
    out
}

/// One line with no spaces at its ends and none doubled.
fn tidy_line(line: &str, fixes: &mut CharFixes) -> String {
    let trimmed = line.trim_matches(' ');
    if trimmed.len() != line.len() {
        fixes.trimmed += 1;
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut spaces = 0;
    for c in trimmed.chars() {
        if c == ' ' {
            spaces += 1;
            continue;
        }
        if spaces > 0 {
            if spaces > 1 {
                fixes.space_runs += 1;
            }
            out.push(' ');
            spaces = 0;
        }
        out.push(c);
    }
    out
}

/// A title's letters and spaces tidied, on one line.
fn one_line(text: &str) -> String {
    let mut ignored = CharFixes::default();
    let plain = plain_characters(&text.replace(['\n', '\r', '\u{2028}', '\u{2029}'], " "), &mut ignored);
    tidy_line(&plain, &mut ignored)
}

/// A box's words with their characters put right (see the module's notes), counted in `fixes`.
/// Blank lines and parting lines stay: the slide is parted at them, or they go, afterwards.
pub fn tidy_text(text: &str, fixes: &mut CharFixes) -> String {
    let mut breaks = 0;
    let mut lined = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    breaks += 1;
                    lined.push('\n');
                }
            }
            '\u{2028}' | '\u{2029}' => {
                breaks += 1;
                lined.push('\n');
            }
            c => lined.push(c),
        }
    }
    fixes.line_breaks += breaks;
    let joined = nfc(&lined);
    if joined != lined {
        fixes.joined += lined.chars().count().saturating_sub(joined.chars().count()).max(1);
    }
    let plain = plain_characters(&joined, fixes);
    plain.split('\n').map(|line| tidy_line(line, fixes)).collect::<Vec<_>>().join("\n")
}

/// A line of only marks that part stanzas: three or more of `*`, `-`, `–`, `—`, `_` or `=`,
/// with spaces between or not ("* * *", "***", "---").
fn is_separator(line: &str) -> bool {
    let marks = line.chars().filter(|c| !c.is_whitespace()).count();
    marks >= 3 && line.chars().all(|c| c.is_whitespace() || matches!(c, '*' | '-' | '–' | '—' | '_' | '='))
}

/// A box's lines as stanzas (runs of lines with words), with what stood between and around
/// them.
#[derive(Default)]
struct Stanzas {
    list: Vec<Vec<String>>,
    /// Parting lines between stanzas, and blank lines there.
    inner_separators: usize,
    inner_blanks: usize,
    /// Parting lines and blank lines at the box's start or end.
    edge_separators: usize,
    edge_blanks: usize,
}

impl Stanzas {
    fn of(text: &str) -> Stanzas {
        let lines: Vec<&str> = text.split('\n').collect();
        let words = |line: &&str| !line.trim().is_empty() && !is_separator(line);
        let (Some(first), Some(last)) = (lines.iter().position(words), lines.iter().rposition(words)) else {
            return Stanzas::default();
        };
        let mut found = Stanzas::default();
        for (at, line) in lines.iter().enumerate() {
            let inner = (first..=last).contains(&at);
            if words(line) {
                if found.list.is_empty() || lines[at - 1].trim().is_empty() || is_separator(lines[at - 1]) {
                    found.list.push(Vec::new());
                }
                if let Some(stanza) = found.list.last_mut() {
                    stanza.push((*line).to_owned());
                }
            } else if is_separator(line) {
                *if inner { &mut found.inner_separators } else { &mut found.edge_separators } += 1;
            } else {
                *if inner { &mut found.inner_blanks } else { &mut found.edge_blanks } += 1;
            }
        }
        found
    }

    fn lines(&self) -> usize {
        self.list.iter().map(Vec::len).sum()
    }

    /// The box's words without the parting and blank lines at its start and end.
    fn trimmed(text: &str) -> String {
        let lines: Vec<&str> = text.split('\n').collect();
        let words = |line: &&str| !line.trim().is_empty() && !is_separator(line);
        match (lines.iter().position(words), lines.iter().rposition(words)) {
            (Some(first), Some(last)) => lines[first..=last].join("\n"),
            _ => text.to_owned(),
        }
    }
}

enum Outcome {
    Same,
    Split(SplitWhy),
    Whole(WholeWhy),
}

/// Tidies one slide: its boxes' characters, and its parting or long stretches. Returns the
/// slide, or the slides it was parted into.
fn tidy_slide(mut slide: Slide, chars: &mut CharFixes) -> (Vec<Slide>, Outcome) {
    let words_role = |object: &Object| {
        object.role.as_ref().is_some_and(|role| *role == Role::primary() || *role == Role::translation())
    };
    let reference_role = |object: &Object| {
        object.role.as_ref().is_some_and(|role| *role == Role::reference() || *role == Role::translation_reference())
    };
    for object in &mut slide.objects {
        if !(words_role(object) || reference_role(object)) {
            continue;
        }
        let Some(text) = object.as_text() else { continue };
        let tidied = tidy_text(text, chars);
        if tidied != text {
            object.set_text(&tidied);
        }
    }
    let boxes = |role: Role| -> Vec<usize> {
        (0..slide.objects.len())
            .filter(|&at| slide.objects[at].role.as_ref() == Some(&role) && slide.objects[at].as_text().is_some())
            .collect()
    };
    let (primaries, translations) = (boxes(Role::primary()), boxes(Role::translation()));
    let found: Vec<(usize, Stanzas)> = primaries
        .iter()
        .chain(&translations)
        .map(|&at| (at, Stanzas::of(slide.objects[at].as_text().unwrap_or_default())))
        .collect();
    // Each words box loses the parting and blank lines at its start and end.
    for (at, stanzas) in &found {
        if stanzas.edge_separators + stanzas.edge_blanks == 0 {
            continue;
        }
        let Some(text) = slide.objects[*at].as_text() else { continue };
        let trimmed = Stanzas::trimmed(text);
        chars.separators += stanzas.edge_separators;
        chars.blank_lines += stanzas.edge_blanks;
        slide.objects[*at].set_text(&trimmed);
    }
    let [primary] = primaries.as_slice() else {
        let parts_wanted = found.iter().any(|(_, s)| s.list.len() > 1 || s.lines() > MOST_TIDY_LINES);
        let outcome = if primaries.len() > 1 && parts_wanted { Outcome::Whole(WholeWhy::Boxes) } else { Outcome::Same };
        return (vec![slide], outcome);
    };
    let main = &found[0].1;
    // The main words in the slides wanted: a slide for each stanza, a long one shared out.
    let pieces: Vec<Vec<String>> = main
        .list
        .iter()
        .flat_map(|stanza| share_out(stanza, MOST_TIDY_LINES).into_iter().map(<[String]>::to_vec))
        .collect();
    if pieces.len() <= 1 {
        return (vec![slide], Outcome::Same);
    }
    if translations.len() > 1 {
        return (vec![slide], Outcome::Whole(WholeWhy::Boxes));
    }
    let styled = primaries.iter().chain(&translations).any(|&at| !slide.objects[at].spans().is_empty());
    if styled {
        return (vec![slide], Outcome::Whole(WholeWhy::Styled));
    }
    // The translation in as many pieces, stanza for stanza or line for line.
    let translation = translations.first().map(|&at| (at, &found[1].1)).filter(|(_, s)| !s.list.is_empty());
    let translated: Option<Vec<Vec<String>>> = match translation {
        None => None,
        Some((_, theirs)) => match paired(main, theirs, &pieces) {
            Some(paired) => Some(paired),
            None => return (vec![slide], Outcome::Whole(WholeWhy::Translation)),
        },
    };
    let why = if main.inner_separators > 0 {
        SplitWhy::Separator
    } else if main.inner_blanks > 0 {
        SplitWhy::Blank
    } else {
        SplitWhy::Long(main.lines())
    };
    chars.separators += main.inner_separators;
    chars.blank_lines += main.inner_blanks;
    if let Some((_, theirs)) = translation {
        chars.separators += theirs.inner_separators;
        chars.blank_lines += theirs.inner_blanks;
    }
    let parts = pieces
        .iter()
        .enumerate()
        .map(|(n, piece)| {
            let mut part = slide.clone();
            if n > 0 {
                part.id = Uuid::new_v4();
                for object in &mut part.objects {
                    object.id = Uuid::new_v4();
                }
                part.label = None;
                part.notes = None;
                part.actions.clear();
                part.hot_key = None;
                part.check = None;
            }
            part.objects[*primary].set_text(&piece.join("\n"));
            if let (Some((at, _)), Some(theirs)) = (translation, &translated) {
                part.objects[at].set_text(&theirs[n].join("\n"));
            }
            part
        })
        .collect();
    (parts, Outcome::Split(why))
}

/// The translation's lines in the same pieces as the main words': stanza for stanza (a stanza
/// shared out only if it has as many lines as the main one), else line for line when the two
/// have as many lines. `None` when they don't part the same way.
fn paired(main: &Stanzas, theirs: &Stanzas, pieces: &[Vec<String>]) -> Option<Vec<Vec<String>>> {
    if main.list.len() == theirs.list.len() {
        let mut out = Vec::new();
        for (ours, others) in main.list.iter().zip(&theirs.list) {
            let ours_parts = share_out(ours, MOST_TIDY_LINES).len();
            if ours_parts == 1 {
                out.push(others.clone());
            } else if others.len() == ours.len() {
                out.extend(share_out(others, MOST_TIDY_LINES).into_iter().map(<[String]>::to_vec));
            } else {
                return None;
            }
        }
        return Some(out);
    }
    if main.lines() != theirs.lines() {
        return None;
    }
    let mut lines = theirs.list.iter().flatten().cloned();
    Some(pieces.iter().map(|piece| lines.by_ref().take(piece.len()).collect()).collect())
}

/// A song's words as one string to tell duplicates by: the main and translation words of all
/// its slides, letters and digits only, in lower case. Empty for a song with no words.
pub fn words_key(song: &Presentation) -> String {
    let mut key = String::new();
    for (_, slide) in song.slides() {
        for object in &slide.objects {
            let words = object.role.as_ref().is_some_and(|r| *r == Role::primary() || *r == Role::translation());
            if let (true, Some(text)) = (words, object.as_text()) {
                key.extend(nfc(text).chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase));
                key.push(' ');
            }
        }
    }
    key.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arrangement, Group, ShowAction, group_color, parse_song_text};

    /// A name garbled as a zip tool garbles it: its UTF-8 bytes read as code page 437.
    fn garbled(name: &str) -> String {
        name.bytes().map(crate::names::cp437).collect()
    }

    /// The same written as a Mac writes a file's name: letters and their marks apart (NFD).
    fn apart(text: &str) -> String {
        icu_normalizer::DecomposingNormalizerBorrowed::new_nfd().normalize(text).into_owned()
    }

    fn title(title: &str, first: Option<&str>) -> (String, Vec<TitleFix>) {
        tidy_title(title, first, TidyOptions::default())
    }

    fn words(slide: &Slide, role: Role) -> String {
        slide.text_for(&role).unwrap_or_default().to_owned()
    }

    fn song_of(slides: Vec<Slide>) -> Presentation {
        let mut song = parse_song_text("Test Song", "");
        song.groups =
            vec![Group { id: Uuid::new_v4(), name: "Verse 1".to_owned(), color: group_color("Verse 1"), slides }];
        song
    }

    fn slide(primary: &str, translation: Option<&str>) -> Slide {
        let mut objects = vec![Object::text(Role::primary(), primary)];
        if let Some(translation) = translation {
            objects.push(Object::text(Role::translation(), translation));
        }
        Slide::new(objects)
    }

    // Titles and words below are made up for these tests.

    #[test]
    fn garbled_titles_are_put_back_however_they_were_stored() {
        for name in ["Свет над тихой рекой", "Світло над річкою", "Ёлка и йод", "Canción del río", "The river’s song"]
        {
            assert_eq!(title(&garbled(name), None), (name.to_owned(), vec![TitleFix::Garbled]), "{name}");
            // Garbled, then stored with its letters and marks apart, as a Mac stores file names.
            assert_eq!(title(&apart(&garbled(name)), None).0, name, "{name}");
            // Garbled from a name already written apart: the й comes back as one letter.
            assert_eq!(title(&garbled(&apart(name)), None).0, name, "{name}");
        }
    }

    #[test]
    fn titles_that_are_already_right_are_left_alone() {
        for name in [
            "Свет над тихой рекой",
            "Ёлка, йод и щётка",
            "Світло над річкою: їжак, ґанок, є!",
            "¿Qué canción, señor?",
            "Größe, café, naïve",
            "The river’s song — “again”",
            "1 - Свет над рекой",
            "Psalm 23:1-6",
            "О, Боже мой…",
        ] {
            assert_eq!(title(name, Some("First line")), (name.to_owned(), vec![]), "{name}");
        }
    }

    #[test]
    fn endings_punctuation_and_spaces_come_off_titles() {
        assert_eq!(
            title("River Song (modified).docx", None),
            ("River Song (modified)".to_owned(), vec![TitleFix::Ending])
        );
        assert_eq!(title("Свет над рекой Modified.DOCX", None).0, "Свет над рекой Modified");
        assert_eq!(title("Свет над рекой,", None), ("Свет над рекой".to_owned(), vec![TitleFix::Punctuation]));
        assert_eq!(title("Свет над рекой;  ", None).0, "Свет над рекой");
        assert_eq!(title("River Song.txt,", None).0, "River Song", "punctuation after an ending");
        assert_eq!(title("River Song,.pro", None).0, "River Song", "punctuation before an ending");
        assert_eq!(title("Song.txt.docx", None).0, "Song", "two endings");
        assert_eq!(title(" Свет  над\u{a0}рекой ", None), ("Свет над рекой".to_owned(), vec![TitleFix::Letters]));
        assert_eq!(title(&apart("Ёлка и йод"), None), ("Ёлка и йод".to_owned(), vec![TitleFix::Letters]));
        assert_eq!(title("Свет над рекой 2.0", None).0, "Свет над рекой 2.0", "not a file's ending");
        assert_eq!(title(",", None).0, ",", "nothing left to be a title");
    }

    #[test]
    fn untitled_songs_take_their_first_line_and_number_titles_add_it() {
        let first = Some("Свет над тихой рекой,");
        assert_eq!(title("Untitled", first), ("Свет над тихой рекой".to_owned(), vec![TitleFix::Untitled]));
        assert_eq!(title("untitled 2", first).0, "Свет над тихой рекой");
        assert_eq!(title("", first).0, "Свет над тихой рекой");
        assert_eq!(title("Untitled", None).0, "Untitled", "no words to take a title from");
        assert_eq!(title("619", first), ("619 - Свет над тихой рекой".to_owned(), vec![TitleFix::Number]));
        assert_eq!(title("100 4", first).0, "100 - Свет над тихой рекой", "the counter goes");
        assert_eq!(title("146-translated 1", first).0, "146 - Свет над тихой рекой");
        assert_eq!(title("2854 - ", first).0, "2854 - Свет над тихой рекой");
        assert_eq!(title("2", first).0, "2 - Свет над тихой рекой", "a number is never taken for no title");
        assert_eq!(title("619", Some("123")).0, "619", "a first line with no words isn't added");
        let kept = tidy_title("619", first, TidyOptions { keep_numbers: true });
        assert_eq!(kept, ("619".to_owned(), vec![]));
        let long = "Свет над тихой рекой и над полем широким и над лесом густым и над домом родным моим всегда";
        let (made, _) = title("Untitled", Some(long));
        assert!(made.ends_with('…') && made.chars().count() <= 81 && long.starts_with(made.trim_end_matches('…')));
    }

    #[test]
    fn titles_are_tidied_once_and_for_all() {
        for name in ["Untitled", "619", "100 4", "Song.txt,", &garbled("Свет над рекой,"), " Свет  ", "Свет."]
        {
            let (once, _) = title(name, Some("Свет над тихой рекой,"));
            assert_eq!(title(&once, Some("Свет над тихой рекой,")), (once.clone(), vec![]), "{name}");
        }
    }

    #[test]
    fn words_get_their_letters_and_spaces_put_right() {
        let mut fixes = CharFixes::default();
        let text = "  Свет\u{a0}над\tрекой  \u{2028}Ёлка и\u{a0}\u{a0}йод\r\nпоём  вместе ";
        assert_eq!(tidy_text(&apart(text), &mut fixes), "Свет над рекой\nЁлка и йод\nпоём вместе");
        assert_eq!(fixes.joined, 4, "й, Ё, й and ё");
        assert_eq!((fixes.line_breaks, fixes.odd_spaces, fixes.tabs), (1, 3, 1));
        assert_eq!((fixes.space_runs, fixes.trimmed), (2, 2));
        // Ukrainian ї and a Spanish accent written apart.
        assert_eq!(tidy_text("i\u{308}жак, cancio\u{301}n, \u{456}\u{308}", &mut fixes), "ïжак, canción, ї");
        // Zero-width characters go between letters, and stay beside an emoji.
        assert_eq!(tidy_text("д\u{200d}\u{200d}орог \u{200b}x", &mut fixes), "дорог x");
        assert_eq!(tidy_text("🙏\u{200d}🔥", &mut fixes), "🙏\u{200d}🔥");
    }

    #[test]
    fn words_that_are_already_right_are_left_alone() {
        for text in [
            "Свет над тихой рекой\nЁлка, йод и щётка",
            "Світло над річкою: їжак, ґанок, є",
            "¿Qué canción, señor?\nGröße, café",
            "The river’s song — “again”\nSing it: 2x",
            // A stress mark over a Cyrillic letter has no letter of its own to join into.
            "Мо\u{301}й свет",
            "",
        ] {
            let mut fixes = CharFixes::default();
            assert_eq!(tidy_text(text, &mut fixes), text);
            assert!(fixes.is_empty(), "{text}");
        }
    }

    #[test]
    fn a_blank_line_or_a_parting_line_parts_a_slide_and_one_at_the_end_just_goes() {
        let mut first = slide("One\nTwo\n\nThree\nFour", None);
        first.label = Some("Start".to_owned());
        first.notes = Some("Breathe".to_owned());
        let media = ShowAction::Media { id: Uuid::new_v4() };
        first.actions.push(media.clone());
        let id = first.id;
        let song = song_of(vec![
            first,
            slide("Five\nSix\n* * *\nSeven", None),
            slide("Eight\nNine\n***\n", None),
            slide("Ten\n---", None),
        ]);
        let tidied = tidy_song(&song, TidyOptions::default());
        let slides: Vec<&Slide> = tidied.song.slides().map(|(_, s)| s).collect();
        let texts: Vec<String> = slides.iter().map(|s| words(s, Role::primary())).collect();
        assert_eq!(texts, ["One\nTwo", "Three\nFour", "Five\nSix", "Seven", "Eight\nNine", "Ten"]);
        assert_eq!(
            tidied.splits,
            [Split { slide: 1, into: 2, why: SplitWhy::Blank }, Split { slide: 2, into: 2, why: SplitWhy::Separator }]
        );
        // The first part is the slide itself; the next a copy of its own.
        assert_eq!(
            (slides[0].id, slides[0].label.as_deref(), slides[0].actions.clone()),
            (id, Some("Start"), vec![media])
        );
        assert_ne!(slides[1].id, id);
        assert_ne!(slides[1].objects[0].id, slides[0].objects[0].id);
        assert!(slides[1].label.is_none() && slides[1].notes.is_none() && slides[1].actions.is_empty());
        assert_eq!((tidied.chars.separators, tidied.chars.blank_lines), (3, 2));
        assert_eq!(tidied.song.groups.len(), 1, "the parts stay in their group");
        assert_eq!(tidied.song.groups[0].id, song.groups[0].id);
    }

    #[test]
    fn a_long_slide_is_shared_out_and_its_translation_with_it_when_it_lines_up() {
        let twelve: Vec<String> = (1..=12).map(|n| format!("Line {n}")).collect();
        let lines = twelve.join("\n");
        let other: Vec<String> = (1..=12).map(|n| format!("Строка {n}")).collect();
        let song = song_of(vec![slide(&lines, Some(&other.join("\n")))]);
        let tidied = tidy_song(&song, TidyOptions::default());
        let slides: Vec<&Slide> = tidied.song.slides().map(|(_, s)| s).collect();
        assert_eq!(slides.len(), 2);
        assert_eq!(words(slides[0], Role::primary()), twelve[..6].join("\n"));
        assert_eq!(words(slides[1], Role::translation()), other[6..].join("\n"));
        assert_eq!(tidied.splits, [Split { slide: 1, into: 2, why: SplitWhy::Long(12) }]);
        // Eight lines stay as they are.
        let eight = song_of(vec![slide(&twelve[..8].join("\n"), None)]);
        assert_eq!(tidy_song(&eight, TidyOptions::default()).song, eight);
    }

    #[test]
    fn a_translation_parts_stanza_for_stanza_or_line_for_line_or_the_slide_stays_whole() {
        let stanzas = song_of(vec![slide("A\nB\n\nC", Some("А\n\nБ\nВ"))]);
        let tidied = tidy_song(&stanzas, TidyOptions::default());
        let parts: Vec<(String, String)> =
            tidied.song.slides().map(|(_, s)| (words(s, Role::primary()), words(s, Role::translation()))).collect();
        assert_eq!(parts, [("A\nB".to_owned(), "А".to_owned()), ("C".to_owned(), "Б\nВ".to_owned())]);
        let lines = song_of(vec![slide("A\nB\n\nC", Some("А\nБ\nВ"))]);
        let parts: Vec<String> = tidy_song(&lines, TidyOptions::default())
            .song
            .slides()
            .map(|(_, s)| words(s, Role::translation()))
            .collect();
        assert_eq!(parts, ["А\nБ", "В"]);
        // Two stanzas over one line of translation: left whole, and said.
        let whole = song_of(vec![slide("A\nB\n\nC\nD\n", Some("А Б В Г"))]);
        let tidied = tidy_song(&whole, TidyOptions::default());
        assert_eq!(tidied.song.slide_count(), 1);
        assert_eq!(words(tidied.song.slide(0).unwrap().1, Role::primary()), "A\nB\n\nC\nD", "only its end is trimmed");
        assert_eq!(tidied.left_whole, [LeftWhole { slide: 1, why: WholeWhy::Translation }]);
        assert!(tidied.splits.is_empty());
    }

    #[test]
    fn the_reference_and_other_things_on_a_slide_come_with_each_part() {
        let mut parted = slide("A\n\nB", None);
        parted.objects.push(Object::text(Role::reference(), "Psalm 1:1\u{a0}"));
        parted.objects.push(Object::shape(crate::Shape::Rectangle));
        let tidied = tidy_song(&song_of(vec![parted]), TidyOptions::default());
        assert_eq!(tidied.song.slide_count(), 2);
        for (_, part) in tidied.song.slides() {
            assert_eq!(words(part, Role::reference()), "Psalm 1:1");
            assert_eq!(part.objects.len(), 3);
        }
    }

    #[test]
    fn boxes_with_no_role_repeat_marks_and_arrangements_are_left_as_they_are() {
        let mut deck =
            Slide::new(vec![Object { role: None, ..Object::text(Role::primary(), "  Point one\n\n  Point two") }]);
        deck.objects[0].name = "Title 1".to_owned();
        let mut song = song_of(vec![deck, slide("Sing it: 2x\nAgain x4", None)]);
        let group = song.groups[0].id;
        song.arrangements =
            vec![Arrangement { id: Uuid::new_v4(), name: "Short".to_owned(), groups: vec![group, group] }];
        song.arrangement = Some(song.arrangements[0].id);
        let tidied = tidy_song(&song, TidyOptions::default());
        assert_eq!(tidied.song, song);
        assert!(tidied.chars.is_empty() && tidied.splits.is_empty() && tidied.title_fixes.is_empty());
    }

    #[test]
    fn a_song_is_tidied_once_and_for_all() {
        let twelve: Vec<String> = (1..=12).map(|n| format!("Line  {n} ")).collect();
        let mut song = song_of(vec![
            slide(&apart("Ёлка\u{2028}и йод\n\n* * *\nСвет"), Some("Fir\nand iodine\n\nLight")),
            slide(&twelve.join("\n"), None),
            slide("A\nB\n\nC", Some("one line")),
        ]);
        song.title = garbled("Ёлка и йод,");
        let once = tidy_song(&song, TidyOptions::default());
        assert_eq!(once.song.title, "Ёлка и йод");
        assert_eq!(once.song.slide_count(), 5);
        let twice = tidy_song(&once.song, TidyOptions::default());
        assert_eq!(twice.song, once.song);
        assert!(twice.title_fixes.is_empty() && twice.splits.is_empty() && twice.chars.is_empty());
        assert_eq!(twice.left_whole.len(), 1, "a slide left whole is said again, and still left");
    }

    #[test]
    fn the_same_words_give_the_same_key_however_they_are_written() {
        let a = song_of(vec![slide("Ёлка, и йод!", Some("Fir"))]);
        let b = song_of(vec![slide(&apart("ёлка и  йод"), Some("fir."))]);
        assert_eq!(words_key(&a), words_key(&b));
        assert_ne!(words_key(&a), words_key(&song_of(vec![slide("Ёлка", None)])));
        let first = first_line(&song_of(vec![slide("\n* * *\nСвет над рекой\nи", None)]));
        assert_eq!(first.as_deref(), Some("Свет над рекой"));
    }
}
