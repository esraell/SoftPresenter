//! Words in a text box that look different from the rest of the box: bold, italic, underlined,
//! struck through, another size, colour or font.
//!
//! A text object keeps its words as one string and its spans beside it. A span is a range of
//! the words and the settings that differ there. The look of a word is the theme's, then the
//! box's own (`TextStyle`), then the span's; so a change to the theme still reaches every
//! setting nobody set by hand.
//!
//! **Offsets count Unicode scalar values (`char`s)**, from 0, with the end not included. They
//! are not UTF-8 bytes (a Rust detail) and not UTF-16 units (a JavaScript one): a letter of
//! any script, an emoji or an accented letter written as one code point is one step in every
//! language, and an offset can never fall inside a character. egui's text cursor counts the
//! same way. In JavaScript, `Array.from(text)` makes the matching index. (A letter written as
//! a base letter and a combining mark is two steps.)
//!
//! Spans are kept sorted and apart, never empty and never next to an equal one, and at most
//! [`MOST_SPANS`] of them. [`TextSpan::normalize`] makes any list so; everything that reads a
//! saved file runs it, and every change to the words goes through [`edit_text`].

use serde::{Deserialize, Serialize};

use crate::{Rgba, theme::BoxStyle};

/// The most spans one text object keeps. A list longer than this is cut, never trusted.
pub const MOST_SPANS: usize = 512;
/// The smallest and largest size a span may set, in slide units.
const SIZES: std::ops::RangeInclusive<f32> = 1.0..=2000.0;
/// The longest font name a span may carry.
const LONGEST_FAMILY: usize = 200;

/// What a span sets over the box's own look. Every `None` is "as the box has it".
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpanStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Font size in slide units, at the box's own size (it shrinks and grows with the box).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgba>,
}

impl SpanStyle {
    /// True when it sets nothing.
    pub fn is_empty(&self) -> bool {
        *self == SpanStyle::default()
    }

    /// Whether every setting it carries is one a file may hold.
    fn is_sound(&self) -> bool {
        self.size.is_none_or(|size| size.is_finite() && SIZES.contains(&size))
            && self.weight.is_none_or(|weight| (1..=1000).contains(&weight))
            && self.font_family.as_ref().is_none_or(|name| {
                !name.is_empty() && name.chars().count() <= LONGEST_FAMILY && !name.chars().any(char::is_control)
            })
    }

    /// This style with what `other` sets put over it.
    pub fn over(&self, other: &SpanStyle) -> SpanStyle {
        SpanStyle {
            font_family: other.font_family.clone().or_else(|| self.font_family.clone()),
            size: other.size.or(self.size),
            weight: other.weight.or(self.weight),
            italic: other.italic.or(self.italic),
            underline: other.underline.or(self.underline),
            strike: other.strike.or(self.strike),
            fill: other.fill.or(self.fill),
        }
    }

    /// A box's look with this put over it.
    pub fn over_box(&self, base: &BoxStyle) -> BoxStyle {
        BoxStyle {
            font_family: self.font_family.clone().unwrap_or_else(|| base.font_family.clone()),
            size: self.size.unwrap_or(base.size),
            weight: self.weight.unwrap_or(base.weight),
            italic: self.italic.unwrap_or(base.italic),
            underline: self.underline.unwrap_or(base.underline),
            strike: self.strike.unwrap_or(base.strike),
            fill: self.fill.unwrap_or(base.fill),
            ..base.clone()
        }
    }
}

/// A range of a text object's words and the look set there. See the module's notes on offsets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextSpan {
    /// The first character, counting from 0.
    pub start: usize,
    /// One past the last character.
    pub end: usize,
    #[serde(flatten)]
    pub style: SpanStyle,
}

impl TextSpan {
    pub fn new(start: usize, end: usize, style: SpanStyle) -> TextSpan {
        TextSpan { start, end, style }
    }

    /// Spans as a file may hold them, made fit to keep for `length` characters: spans outside
    /// the words, empty, setting nothing or setting something a file may not hold are dropped;
    /// one running past the end is cut there; they are sorted, one that overlaps an earlier
    /// one is dropped, equal neighbours are joined, and only [`MOST_SPANS`] are kept.
    pub fn normalize(spans: Vec<TextSpan>, length: usize) -> Vec<TextSpan> {
        let mut spans: Vec<TextSpan> = spans
            .into_iter()
            .map(|span| TextSpan { end: span.end.min(length), ..span })
            .filter(|span| span.start < span.end && !span.style.is_empty() && span.style.is_sound())
            .collect();
        // Stable, so of two spans starting together the one written first is the one kept.
        spans.sort_by_key(|span| span.start);
        let mut kept: Vec<TextSpan> = Vec::with_capacity(spans.len().min(MOST_SPANS));
        for span in spans {
            match kept.last_mut() {
                Some(last) if span.start < last.end => {}
                Some(last) if span.start == last.end && span.style == last.style => last.end = span.end,
                _ => kept.push(span),
            }
        }
        kept.truncate(MOST_SPANS);
        kept
    }

    /// Reads spans from a file, one by one: a span that is not shaped right is dropped and the
    /// rest read, so one bad entry does not lose the box. The list is cut to a few times the
    /// most kept before it is looked at; [`TextSpan::normalize`] then makes it sound.
    pub(crate) fn lenient<'de, D: serde::Deserializer<'de>>(from: D) -> Result<Vec<TextSpan>, D::Error> {
        let raw: Vec<serde_json::Value> = Vec::deserialize(from)?;
        Ok(raw.into_iter().take(MOST_SPANS * 4).filter_map(|v| serde_json::from_value(v).ok()).collect())
    }
}

/// The style set at a character, if any span covers it.
pub fn style_at(spans: &[TextSpan], at: usize) -> Option<&SpanStyle> {
    spans.iter().find(|span| span.start <= at && at < span.end).map(|span| &span.style)
}

/// Cuts the spans so that none runs across `at`.
fn split_at(spans: &mut Vec<TextSpan>, at: usize) {
    if let Some(i) = spans.iter().position(|s| s.start < at && at < s.end) {
        let mut second = spans[i].clone();
        second.start = at;
        spans[i].end = at;
        spans.insert(i + 1, second);
    }
}

/// Joins neighbours that touch and look alike, and drops spans that set nothing or hold nothing.
fn tidy(spans: &mut Vec<TextSpan>) {
    spans.retain(|s| s.start < s.end && !s.style.is_empty());
    spans.sort_by_key(|s| s.start);
    let mut at = 0;
    while at + 1 < spans.len() {
        if spans[at].end == spans[at + 1].start && spans[at].style == spans[at + 1].style {
            spans[at].end = spans[at + 1].end;
            spans.remove(at + 1);
        } else {
            at += 1;
        }
    }
}

/// Makes `change` to the style of every character from `start` to `end`, whether or not a span
/// is there yet. A span the change leaves setting nothing goes. `length` is the text's length.
pub fn restyle(spans: &mut Vec<TextSpan>, start: usize, end: usize, length: usize, change: impl Fn(&mut SpanStyle)) {
    let end = end.min(length);
    if start >= end {
        return;
    }
    split_at(spans, start);
    split_at(spans, end);
    // The gaps between spans inside the range become spans that set nothing, to be changed too.
    let mut gaps = Vec::new();
    let mut at = start;
    for span in spans.iter().filter(|s| s.end > start && s.start < end) {
        if span.start > at {
            gaps.push(TextSpan::new(at, span.start, SpanStyle::default()));
        }
        at = span.end;
    }
    if at < end {
        gaps.push(TextSpan::new(at, end, SpanStyle::default()));
    }
    spans.extend(gaps);
    spans.sort_by_key(|s| s.start);
    for span in spans.iter_mut().filter(|s| s.start >= start && s.end <= end) {
        change(&mut span.style);
    }
    tidy(spans);
    spans.truncate(MOST_SPANS);
}

/// Sets the exact style of the characters from `start` to `end`: `None` clears them.
fn assign(spans: &mut Vec<TextSpan>, start: usize, end: usize, style: Option<SpanStyle>) {
    if start >= end {
        return;
    }
    split_at(spans, start);
    split_at(spans, end);
    spans.retain(|s| s.end <= start || s.start >= end);
    if let Some(style) = style {
        spans.push(TextSpan::new(start, end, style));
    }
    tidy(spans);
}

/// The one way to change the words of a text object that has spans: replaces the characters
/// from `start` to `end` with `with`, and moves, cuts and joins the spans to follow. Offsets
/// past the end are taken as the end.
///
/// Spans after the change move along; a span the change cuts into keeps what is left of it;
/// one that is wholly replaced goes. The new words are given the look of the first character
/// they replace, or when they replace none (typing) of the character before them, as a word
/// processor does, so typing at the end of a bold word continues it and typing at the start of
/// one does not.
pub fn edit_text(text: &mut String, spans: &mut Vec<TextSpan>, start: usize, end: usize, with: &str) {
    let length = text.chars().count();
    let (start, end) = (start.min(length), end.min(length).max(start.min(length)));
    let byte = |at: usize| text.char_indices().nth(at).map_or(text.len(), |(i, _)| i);
    let (from, to) = (byte(start), byte(end));
    text.replace_range(from..to, with);
    if spans.is_empty() {
        return;
    }
    let added = with.chars().count();
    let gone = end - start;
    let inherits = if gone > 0 {
        style_at(spans, start).cloned()
    } else if start > 0 {
        style_at(spans, start - 1).cloned()
    } else {
        // At the very start, the look of the words after.
        style_at(spans, 0).cloned()
    };
    let mut moved = Vec::with_capacity(spans.len() + 1);
    for span in spans.drain(..) {
        let (mut s, mut e) = (span.start, span.end);
        // Take the removed characters out, then make room for the added ones.
        let cut = |p: usize| {
            if p <= start {
                p
            } else if p >= end {
                p - gone
            } else {
                start
            }
        };
        (s, e) = (cut(s), cut(e));
        if e <= s {
            continue;
        }
        // Room at `start`: what begins at or after it moves right; what holds it grows.
        let push = |p: usize, is_start: bool| if p > start || (p == start && is_start) { p + added } else { p };
        (s, e) = (push(s, true), push(e, false));
        moved.push(TextSpan { start: s, end: e, ..span });
    }
    // A span that held the place the words went in has grown across them; the new words are
    // now given their own look below.
    *spans = moved;
    let new_length = length - gone + added;
    assign(spans, start, start + added, inherits);
    *spans = TextSpan::normalize(std::mem::take(spans), new_length);
}

/// The smallest single edit that makes `old` into `new`: the characters of `old` from the first
/// result to the second are replaced by the third. `None` when they are the same.
pub fn smallest_edit<'a>(old: &str, new: &'a str) -> Option<(usize, usize, &'a str)> {
    if old == new {
        return None;
    }
    let (mut before, mut after) = (0, 0);
    let (old_len, new_len) = (old.chars().count(), new.chars().count());
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b {
            break;
        }
        before += 1;
    }
    let room = old_len.min(new_len) - before;
    for (a, b) in old.chars().rev().zip(new.chars().rev()) {
        if a != b || after >= room {
            break;
        }
        after += 1;
    }
    let from = new.char_indices().nth(before).map_or(new.len(), |(i, _)| i);
    let to = new.char_indices().nth(new_len - after).map_or(new.len(), |(i, _)| i);
    Some((before, old_len - after, &new[from..to]))
}

/// The byte range of each span in `text`, for a layout that counts bytes. Spans past the end
/// of the text are left out.
pub fn byte_ranges<'a>(text: &str, spans: &'a [TextSpan]) -> Vec<(std::ops::Range<usize>, &'a SpanStyle)> {
    if spans.is_empty() {
        return Vec::new();
    }
    let mut starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    starts.push(text.len());
    let at = |c: usize| starts.get(c).copied();
    spans.iter().filter_map(|s| Some((at(s.start)?..at(s.end.min(starts.len() - 1))?, &s.style))).collect()
}

/// The words of a box as they are drawn, with the spans moved to match.
pub struct Drawn<'a> {
    pub text: std::borrow::Cow<'a, str>,
    pub spans: std::borrow::Cow<'a, [TextSpan]>,
}

impl BoxStyle {
    /// The text as it is drawn under this look (lines changed, list marks put in, caps), with
    /// the spans moved to follow the characters they were on. With no spans this is exactly the
    /// three plain changes and costs nothing more. `joined` also runs the lines into one, as
    /// text moving sideways is drawn.
    pub fn drawn<'a>(&self, text: &'a str, spans: &'a [TextSpan], joined: bool) -> Drawn<'a> {
        use std::borrow::Cow;
        if spans.is_empty() {
            let mut text = self.line_transform.apply(text);
            if self.list != crate::ListStyle::None {
                text = Cow::Owned(self.list.apply(&text).into_owned());
            }
            if self.caps != crate::Caps::None {
                text = Cow::Owned(self.caps.apply(&text).into_owned());
            }
            let text = if joined { Cow::Owned(crate::text_map::join_lines(&text)) } else { text };
            return Drawn { text, spans: Cow::Borrowed(&[]) };
        }
        let mut chars = crate::text_map::tag(text);
        chars = crate::text_map::line_transform(self.line_transform, chars);
        chars = crate::text_map::list(self.list, chars);
        chars = crate::text_map::caps(self.caps, chars);
        if joined {
            chars = crate::text_map::join_tagged(chars);
        }
        let (text, spans) = crate::text_map::finish(chars, spans);
        Drawn { text: Cow::Owned(text), spans: Cow::Owned(spans) }
    }
}

impl SpanStyle {
    /// What a span must set to make `look` out of `base`: only the settings that differ.
    pub fn between(base: &BoxStyle, look: &BoxStyle) -> SpanStyle {
        SpanStyle {
            font_family: (look.font_family != base.font_family).then(|| look.font_family.clone()),
            size: (look.size != base.size).then_some(look.size),
            weight: (look.weight != base.weight).then_some(look.weight),
            italic: (look.italic != base.italic).then_some(look.italic),
            underline: (look.underline != base.underline).then_some(look.underline),
            strike: (look.strike != base.strike).then_some(look.strike),
            fill: (look.fill != base.fill).then_some(look.fill),
        }
    }
}

/// Which settings differ between the characters of a range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpanMix {
    pub font_family: bool,
    pub size: bool,
    pub weight: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub fill: bool,
}

/// The look of the characters from `start` to `end` over the box's look `base`: that of the
/// first character, and which settings are not the same all through the range.
pub fn look_of_range(spans: &[TextSpan], start: usize, end: usize, base: &BoxStyle) -> (BoxStyle, SpanMix) {
    let mut pieces: Vec<BoxStyle> = Vec::new();
    let mut at = start;
    for span in spans.iter().filter(|s| s.end > start && s.start < end) {
        if span.start > at {
            pieces.push(base.clone());
        }
        pieces.push(span.style.over_box(base));
        at = span.end;
    }
    if at < end {
        pieces.push(base.clone());
    }
    let first = pieces.first().cloned().unwrap_or_else(|| base.clone());
    let mut mix = SpanMix::default();
    for p in &pieces {
        mix.font_family |= p.font_family != first.font_family;
        mix.size |= p.size != first.size;
        mix.weight |= p.weight != first.weight;
        mix.italic |= p.italic != first.italic;
        mix.underline |= p.underline != first.underline;
        mix.strike |= p.strike != first.strike;
        mix.fill |= p.fill != first.fill;
    }
    (first, mix)
}

/// Makes `change` to the look of the characters from `start` to `end` of words `length`
/// characters long, over the box's look `base`: a setting that comes out as the box's own is
/// not kept as a span's.
pub fn restyle_range(
    spans: &mut Vec<TextSpan>,
    start: usize,
    end: usize,
    length: usize,
    base: &BoxStyle,
    change: impl Fn(&mut BoxStyle),
) {
    restyle(spans, start, end, length, |own| {
        let mut look = own.over_box(base);
        change(&mut look);
        *own = SpanStyle::between(base, &look);
    });
}

/// Clears one setting from every span (as a change to the whole box does), by `clear`.
pub fn clear_in_spans(spans: &mut Vec<TextSpan>, length: usize, clear: impl Fn(&mut SpanStyle)) {
    restyle(spans, 0, length, length, |own| clear(own));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Object, ObjectKind, Rgba, Role};

    fn bold() -> SpanStyle {
        SpanStyle { weight: Some(700), ..SpanStyle::default() }
    }

    fn italic() -> SpanStyle {
        SpanStyle { italic: Some(true), ..SpanStyle::default() }
    }

    fn span(start: usize, end: usize, style: SpanStyle) -> TextSpan {
        TextSpan::new(start, end, style)
    }

    /// "one two three" with "two" (4..7) bold.
    fn words() -> (String, Vec<TextSpan>) {
        ("one two three".to_owned(), vec![span(4, 7, bold())])
    }

    fn spans_of(object: &Object) -> Vec<(usize, usize)> {
        object.spans().iter().map(|s| (s.start, s.end)).collect()
    }

    #[test]
    fn typing_before_a_span_moves_it_and_does_not_join_it() {
        let (mut text, mut spans) = words();
        edit_text(&mut text, &mut spans, 0, 0, "ab ");
        assert_eq!(text, "ab one two three");
        assert_eq!(spans, vec![span(7, 10, bold())]);
    }

    #[test]
    fn typing_at_the_start_of_a_span_stays_outside_it() {
        let (mut text, mut spans) = words();
        edit_text(&mut text, &mut spans, 4, 4, "X");
        assert_eq!(text, "one Xtwo three");
        assert_eq!(spans, vec![span(5, 8, bold())]);
    }

    #[test]
    fn typing_inside_a_span_grows_it() {
        let (mut text, mut spans) = words();
        edit_text(&mut text, &mut spans, 5, 5, "--");
        assert_eq!(text, "one t--wo three");
        assert_eq!(spans, vec![span(4, 9, bold())]);
    }

    #[test]
    fn typing_at_the_end_of_a_span_continues_it() {
        let (mut text, mut spans) = words();
        edit_text(&mut text, &mut spans, 7, 7, "s");
        assert_eq!(text, "one twos three");
        assert_eq!(spans, vec![span(4, 8, bold())]);
    }

    #[test]
    fn typing_after_a_span_and_a_space_is_plain() {
        let (mut text, mut spans) = words();
        edit_text(&mut text, &mut spans, 8, 8, "very ");
        assert_eq!(text, "one two very three");
        assert_eq!(spans, vec![span(4, 7, bold())]);
    }

    #[test]
    fn deleting_across_spans_cuts_and_closes_them() {
        let mut text = "aaa bbb ccc ddd".to_owned();
        let mut spans = vec![span(0, 3, bold()), span(8, 11, italic())];
        // From the middle of the first span to the middle of the second.
        edit_text(&mut text, &mut spans, 1, 9, "");
        assert_eq!(text, "acc ddd");
        assert_eq!(spans, vec![span(0, 1, bold()), span(1, 3, italic())]);
    }

    #[test]
    fn deleting_a_whole_span_removes_it() {
        let (mut text, mut spans) = words();
        edit_text(&mut text, &mut spans, 3, 8, "");
        assert_eq!(text, "onethree");
        assert!(spans.is_empty());
    }

    #[test]
    fn replacing_everything_leaves_the_look_of_the_first_replaced_letter() {
        let (mut text, mut spans) = words();
        edit_text(&mut text, &mut spans, 0, 13, "fresh");
        assert_eq!(text, "fresh");
        assert!(spans.is_empty(), "the first letter had no span");
        let mut text = "bold start".to_owned();
        let mut spans = vec![span(0, 4, bold())];
        edit_text(&mut text, &mut spans, 0, 10, "new words");
        assert_eq!(spans, vec![span(0, 9, bold())]);
    }

    #[test]
    fn offsets_count_characters_not_bytes() {
        // A Cyrillic word (2 bytes a letter), an emoji (4 bytes, 2 UTF-16 units) and a plain word.
        let mut text = "мир \u{1F600} end".to_owned();
        let mut spans = vec![span(4, 5, bold()), span(6, 9, italic())];
        edit_text(&mut text, &mut spans, 3, 3, "!");
        assert_eq!(text, "мир! \u{1F600} end");
        assert_eq!(spans, vec![span(5, 6, bold()), span(7, 10, italic())]);
        // Taking the emoji out takes its span with it, and the next one moves by one.
        edit_text(&mut text, &mut spans, 5, 6, "");
        assert_eq!(text, "мир!  end");
        assert_eq!(spans, vec![span(6, 9, italic())]);
        // Offsets past the end are the end: typing there continues the last word's look.
        edit_text(&mut text, &mut spans, 99, 200, "?");
        assert!(text.ends_with("end?"));
        assert_eq!(spans, vec![span(6, 10, italic())]);
    }

    #[test]
    fn the_smallest_edit_is_found_by_character() {
        assert_eq!(smallest_edit("abc", "abc"), None);
        assert_eq!(smallest_edit("мир \u{1F600} x", "мир \u{1F600}\u{1F600} x"), Some((5, 5, "\u{1F600}")));
        assert_eq!(smallest_edit("hello world", "hello brave world"), Some((6, 6, "brave ")));
        assert_eq!(smallest_edit("aaa", "aa"), Some((2, 3, "")));
        assert_eq!(smallest_edit("", "x"), Some((0, 0, "x")));
    }

    #[test]
    fn setting_the_whole_text_keeps_the_spans_of_words_that_stay() {
        let mut object = Object::text(Role::primary(), "one two three");
        if let ObjectKind::Text { spans, .. } = &mut object.kind {
            *spans = vec![span(4, 7, bold())];
        }
        object.set_text("one two three four");
        assert_eq!(spans_of(&object), vec![(4, 7)]);
        object.set_text("zero one two three four");
        assert_eq!(spans_of(&object), vec![(9, 12)]);
        // Letters for letters: the capitals keep the spans where they were.
        object.set_text("ZERO ONE TWO THREE FOUR");
        assert_eq!(spans_of(&object), vec![(9, 12)]);
        assert_eq!(object.as_text(), Some("ZERO ONE TWO THREE FOUR"));
    }

    #[test]
    fn restyle_splits_fills_gaps_and_joins() {
        let mut spans = vec![span(2, 4, bold())];
        restyle(&mut spans, 0, 10, 10, |s| s.italic = Some(true));
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].style, SpanStyle { weight: Some(700), italic: Some(true), ..SpanStyle::default() });
        // Taking it off again leaves just the bold.
        restyle(&mut spans, 0, 10, 10, |s| s.italic = None);
        assert_eq!(spans, vec![span(2, 4, bold())]);
        // Taking the bold off the end of it shortens it.
        restyle(&mut spans, 3, 4, 10, |s| s.weight = None);
        assert_eq!(spans, vec![span(2, 3, bold())]);
    }

    #[test]
    fn hostile_spans_are_made_sound() {
        let ok = bold();
        let huge = SpanStyle { size: Some(1e9), ..SpanStyle::default() };
        let nan = SpanStyle { size: Some(f32::NAN), ..SpanStyle::default() };
        let zero_weight = SpanStyle { weight: Some(0), ..SpanStyle::default() };
        let long_name = SpanStyle { font_family: Some("x".repeat(500)), ..SpanStyle::default() };
        let control = SpanStyle { font_family: Some("a\u{0}b".to_owned()), ..SpanStyle::default() };
        let spans = vec![
            span(5, 3, ok.clone()),           // backwards
            span(4, 4, ok.clone()),           // empty
            span(0, 2, SpanStyle::default()), // sets nothing
            span(0, 2, huge),                 // size out of range
            span(0, 2, nan),                  // not a number
            span(0, 2, zero_weight),          // weight out of range
            span(0, 2, long_name),            // name too long
            span(0, 2, control),              // control character
            span(50, 60, ok.clone()),         // beyond the words
            span(6, usize::MAX, italic()),    // runs on past the end: cut there
            span(2, 5, ok.clone()),           // kept
            span(3, 4, italic()),             // inside the one before it: dropped
            span(5, 6, ok.clone()),           // touches 2..5 and looks the same: joined
        ];
        let kept = TextSpan::normalize(spans, 8);
        assert_eq!(kept, vec![span(2, 6, ok), span(6, 8, italic())]);
    }

    #[test]
    fn too_many_spans_are_cut() {
        let spans: Vec<TextSpan> = (0..2000).map(|i| span(i * 2, i * 2 + 1, bold())).collect();
        assert_eq!(TextSpan::normalize(spans, 10_000).len(), MOST_SPANS);
    }

    #[test]
    fn a_file_with_bad_spans_loads_with_the_good_ones() {
        let json = r#"{"kind":"text","text":"hello big world","spans":[
            {"start":-1,"end":3,"weight":700},
            {"start":"x","end":3,"weight":700},
            {"start":6,"end":9,"weight":700},
            {"start":6,"end":9,"italic":true},
            {"start":9,"end":99,"italic":true,"size":0},
            {"start":12,"end":999,"underline":true}]}"#;
        let kind: ObjectKind = serde_json::from_str(json).unwrap();
        let ObjectKind::Text { spans, .. } = kind else { panic!() };
        assert_eq!(
            spans,
            vec![span(6, 9, bold()), span(12, 15, SpanStyle { underline: Some(true), ..SpanStyle::default() })]
        );
        // Not a list at all is a bad file, as any other wrong shape is.
        assert!(serde_json::from_str::<ObjectKind>(r#"{"kind":"text","text":"x","spans":5}"#).is_err());
    }

    #[test]
    fn spans_save_only_when_there_are_some_and_come_back() {
        let mut object = Object::text(Role::primary(), "plain");
        let plain = serde_json::to_string(&object).unwrap();
        assert!(!plain.contains("spans"), "{plain}");
        let before: Object = serde_json::from_str(&plain).unwrap();
        assert_eq!(serde_json::to_string(&before).unwrap(), plain, "an old file is saved as it was");
        if let ObjectKind::Text { spans, .. } = &mut object.kind {
            *spans = vec![span(1, 3, SpanStyle { fill: Some(Rgba::rgb(255, 0, 0)), ..bold() })];
        }
        let saved = serde_json::to_string(&object).unwrap();
        assert!(saved.contains(r#""spans":[{"start":1,"end":3,"weight":700,"fill""#), "{saved}");
        assert_eq!(serde_json::from_str::<Object>(&saved).unwrap(), object);
    }

    #[test]
    fn a_span_over_the_box_keeps_what_the_theme_gives_the_rest() {
        let theme = crate::Theme::builtin();
        let base = theme.text_style(None, &crate::TextStyle::default()).unwrap();
        let sized = SpanStyle { size: Some(99.0), ..SpanStyle::default() };
        let styled = sized.over_box(&base);
        assert_eq!(styled.size, 99.0);
        assert_eq!(styled.fill, base.fill);
        assert_eq!(styled.weight, base.weight);
        // The theme changes its colour: the span, which never set one, follows.
        let mut other = base.clone();
        other.fill = Rgba::rgb(1, 2, 3);
        assert_eq!(sized.over_box(&other).fill, other.fill);
    }

    #[test]
    fn spans_follow_their_words_through_caps_lists_and_line_changes() {
        let mut look = crate::Theme::builtin().text_style(None, &crate::TextStyle::default()).unwrap();
        look.caps = crate::Caps::AllCaps;
        look.list = crate::ListStyle::Bullets;
        let text = "first line\nsecond one";
        let spans = vec![span(6, 10, bold()), span(11, 17, italic())];
        let drawn = look.drawn(text, &spans, false);
        assert_eq!(drawn.text, "\u{2022}  FIRST LINE\n\u{2022}  SECOND ONE");
        // "line" and "second" in the drawn words; each bullet takes the look of what follows it.
        let chars: Vec<char> = drawn.text.chars().collect();
        let said = |s: &TextSpan| chars[s.start..s.end].iter().collect::<String>();
        let found: Vec<String> = drawn.spans.iter().map(said).collect();
        assert_eq!(found, vec!["LINE", "\u{2022}  SECOND"]);
        // A box with no spans costs nothing: the words come back borrowed when nothing changes.
        look.caps = crate::Caps::None;
        look.list = crate::ListStyle::None;
        assert!(matches!(look.drawn(text, &[], false).text, std::borrow::Cow::Borrowed(_)));
    }
}
