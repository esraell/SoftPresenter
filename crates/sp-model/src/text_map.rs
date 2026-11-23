//! The changes made to a box's words as they are drawn (lines changed, list marks, caps), done
//! again on characters that remember where they came from, so spans can follow them.
//!
//! Each function here mirrors the plain one beside the setting it is for (`Caps::apply`,
//! `LineTransform::apply`, `ListStyle::apply`); the tests below check that the two make the
//! same words for a pile of awkward text. Only text with spans comes through here.

use crate::spans::TextSpan;
use crate::theme::{Caps, LineTransform, ListStyle, SMALL_WORDS};

/// A character and the place it had in the stored words (`None` for one put in, such as a
/// bullet).
type Tagged = Vec<(char, Option<u32>)>;
type Piece<'a> = &'a [(char, Option<u32>)];
type Cut = Option<(char, Option<u32>)>;

pub(crate) fn tag(text: &str) -> Tagged {
    text.chars().enumerate().map(|(i, c)| (c, Some(i as u32))).collect()
}

/// The words, and the spans moved to the characters they sat on. A character put in takes the
/// look of the character after it.
pub(crate) fn finish(chars: Tagged, spans: &[TextSpan]) -> (String, Vec<TextSpan>) {
    let owner = |source: u32| {
        let at = spans.partition_point(|s| s.end <= source as usize);
        spans.get(at).filter(|s| s.start <= source as usize).map(|_| at)
    };
    let mut owners: Vec<Option<usize>> = vec![None; chars.len()];
    let mut carry = None;
    for (at, (c, origin)) in chars.iter().enumerate().rev() {
        carry = origin.map_or(carry, owner);
        // A line break put in between words carries no look; everything else put in does.
        owners[at] = if origin.is_none() && *c == '\n' { None } else { carry };
    }
    let mut out: Vec<TextSpan> = Vec::new();
    for (at, owner) in owners.iter().enumerate() {
        let Some(owner) = owner else { continue };
        let style = &spans[*owner].style;
        match out.last_mut() {
            Some(last) if last.end == at && last.style == *style => last.end = at + 1,
            _ => out.push(TextSpan::new(at, at + 1, style.clone())),
        }
    }
    (chars.into_iter().map(|(c, _)| c).collect(), out)
}

fn put(out: &mut Tagged, text: &str) {
    out.extend(text.chars().map(|c| (c, None)));
}

/// Cuts at each character for which `at` is true; each piece comes with the character that
/// ended it (none for the last piece).
fn split_on(chars: Piece<'_>, at: impl Fn(char) -> bool) -> Vec<(Piece<'_>, Cut)> {
    let mut pieces = Vec::new();
    let mut from = 0;
    for (i, &(c, o)) in chars.iter().enumerate() {
        if at(c) {
            pieces.push((&chars[from..i], Some((c, o))));
            from = i + 1;
        }
    }
    pieces.push((&chars[from..], None));
    pieces
}

fn trimmed(piece: Piece<'_>) -> Piece<'_> {
    let start = piece.iter().position(|(c, _)| !c.is_whitespace()).unwrap_or(piece.len());
    let end = piece.iter().rposition(|(c, _)| !c.is_whitespace()).map_or(start, |e| e + 1);
    &piece[start..end]
}

/// Words separated by white space, joined by `glue`.
fn words_joined(chars: Piece<'_>, glue: char) -> Tagged {
    let mut out: Tagged = Vec::new();
    let mut gap: Option<Option<u32>> = None;
    for &(c, o) in chars {
        if c.is_whitespace() {
            if !out.is_empty() && gap.is_none() {
                gap = Some(o);
            }
        } else {
            if let Some(o) = gap.take() {
                out.push((glue, o));
            }
            out.push((c, o));
        }
    }
    out
}

pub(crate) fn line_transform(how: LineTransform, chars: Tagged) -> Tagged {
    match how {
        LineTransform::None => chars,
        LineTransform::RemoveLineReturns => words_joined(&chars, ' '),
        LineTransform::OneWordPerLine => words_joined(&chars, '\n'),
        LineTransform::ReplaceLineReturns => joined_lines(&chars, " / "),
        LineTransform::OneCharacterPerLine => {
            let mut out: Tagged = Vec::new();
            for &(c, o) in chars.iter().filter(|(c, _)| !c.is_whitespace()) {
                if !out.is_empty() {
                    out.push(('\n', None));
                }
                out.push((c, o));
            }
            out
        }
    }
}

/// The lines with words on them, trimmed, joined by `glue`.
fn joined_lines(chars: Piece<'_>, glue: &str) -> Tagged {
    let mut out: Tagged = Vec::new();
    for (line, _) in split_on(chars, |c| c == '\n') {
        let line = trimmed(line);
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            put(&mut out, glue);
        }
        out.extend_from_slice(line);
    }
    out
}

/// Text moving sideways: its lines run into one, a few spaces apart.
pub(crate) fn join_lines(text: &str) -> String {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("     ")
}

pub(crate) fn join_tagged(chars: Tagged) -> Tagged {
    joined_lines(&chars, "     ")
}

pub(crate) fn list(how: ListStyle, chars: Tagged) -> Tagged {
    if how == ListStyle::None {
        return chars;
    }
    let mut out: Tagged = Vec::new();
    let mut number = 0;
    let pieces = split_on(&chars, |c| c == '\n');
    let last = pieces.len() - 1;
    for (i, (line, cut)) in pieces.into_iter().enumerate() {
        // The text after the last line break is a line only if it has anything in it.
        if i == last && line.is_empty() {
            break;
        }
        // A line that ended in a break loses the carriage return before it.
        let line = match (cut.is_some(), line.last()) {
            (true, Some(('\r', _))) => &line[..line.len() - 1],
            _ => line,
        };
        if i > 0 {
            out.push(('\n', None));
        }
        if line.iter().all(|(c, _)| c.is_whitespace()) {
            out.extend_from_slice(line);
            continue;
        }
        number += 1;
        match how {
            ListStyle::Bullets => put(&mut out, "\u{2022}  "),
            _ => put(&mut out, &format!("{number}.  ")),
        }
        let start = line.iter().position(|(c, _)| !c.is_whitespace()).unwrap_or(0);
        out.extend_from_slice(&line[start..]);
    }
    out
}

/// The characters changed to upper or lower case, which can be more characters than there were.
fn recase(chars: Piece<'_>, upper: bool) -> Tagged {
    let whole: String = chars.iter().map(|(c, _)| *c).collect();
    let whole = if upper { whole.to_uppercase() } else { whole.to_lowercase() };
    let mut made = whole.chars();
    let mut out: Tagged = Vec::with_capacity(chars.len());
    for &(c, o) in chars {
        let count = if upper { c.to_uppercase().count() } else { c.to_lowercase().count() };
        out.extend(made.by_ref().take(count).map(|m| (m, o)));
    }
    out.extend(made.map(|m| (m, None)));
    out
}

/// Every word with a capital first letter and the rest in lower case.
fn start_case(chars: Piece<'_>) -> Tagged {
    let mut out: Tagged = Vec::with_capacity(chars.len());
    let mut start = true;
    for &(c, o) in chars {
        if start && c.is_alphabetic() {
            out.extend(c.to_uppercase().map(|m| (m, o)));
        } else {
            out.extend(c.to_lowercase().map(|m| (m, o)));
        }
        start = !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}');
    }
    out
}

pub(crate) fn caps(how: Caps, chars: Tagged) -> Tagged {
    match how {
        Caps::None => chars,
        Caps::AllCaps => recase(&chars, true),
        Caps::Lowercase => recase(&chars, false),
        Caps::StartCase => start_case(&chars),
        Caps::TitleCase => {
            let mut out: Tagged = Vec::with_capacity(chars.len());
            for (line, cut) in split_on(&chars, |c| c == '\n') {
                let started = start_case(line);
                let mut first = true;
                for (word, space) in split_on(&started, |c| c == ' ') {
                    let text: String = word.iter().map(|(c, _)| *c).collect();
                    let small = !first && SMALL_WORDS.contains(&text.to_lowercase().as_str());
                    first = first && word.is_empty();
                    if small {
                        out.extend(recase(word, false));
                    } else {
                        out.extend_from_slice(word);
                    }
                    out.extend(space);
                }
                out.extend(cut);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWKWARD: [&str; 9] = [
        "",
        "plain words",
        "  Spaces  between \n\n blank lines \r\n and \r\n",
        "the Lord of the rings and THE END\nA tale in a day",
        "straße ΟΔΟΣ İstanbul don't DON\u{2019}T",
        "Привет, мир\nвторая строка ",
        "emoji \u{1F600} line\n\u{1F600}",
        "one\n\n\n two",
        "tail with newline\n",
    ];

    fn plain(text: &str, line: LineTransform, list_style: ListStyle, case: Caps) -> String {
        let text = line.apply(text);
        let text = list_style.apply(&text);
        case.apply(&text).into_owned()
    }

    #[test]
    fn tagged_changes_make_the_same_words_as_the_plain_ones() {
        let lines = [
            LineTransform::None,
            LineTransform::RemoveLineReturns,
            LineTransform::ReplaceLineReturns,
            LineTransform::OneWordPerLine,
            LineTransform::OneCharacterPerLine,
        ];
        let lists = [ListStyle::None, ListStyle::Bullets, ListStyle::Numbers];
        let cases = [Caps::None, Caps::AllCaps, Caps::Lowercase, Caps::TitleCase, Caps::StartCase];
        for text in AWKWARD {
            for line in lines {
                for list_style in lists {
                    for case in cases {
                        let mut chars = tag(text);
                        chars = line_transform(line, chars);
                        chars = list(list_style, chars);
                        chars = caps(case, chars);
                        let (made, _) = finish(chars, &[]);
                        assert_eq!(
                            made,
                            plain(text, line, list_style, case),
                            "{text:?} {line:?} {list_style:?} {case:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sideways_joining_matches() {
        for text in AWKWARD {
            let (made, _) = finish(join_tagged(tag(text)), &[]);
            assert_eq!(made, join_lines(text), "{text:?}");
        }
    }
}
