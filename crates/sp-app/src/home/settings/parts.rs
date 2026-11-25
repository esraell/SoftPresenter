//! The pieces every Settings page is built from, as the design draws them: a card with its title
//! and a line in small capitals, rows with a control at their end, a row of keys among which one is
//! chosen (each with its light and a small picture), and the On/Off key.

use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, FontId, Frame, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::i18n::t;
use crate::studio::{
    BUTTON, BUTTON_HOVER, CARD, Flat, LINE, ON_EDGE, RADIUS_CARD, RADIUS_PANEL, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT,
    fixed, flat_button, paint_light, semibold, widest,
};

/// The small capitals of a card's line: their size and the room between their letters.
const CAPS: f32 = 11.5;
const CAPS_SPACING: f32 = 0.575;
/// How tall each line of a row's name, of the words under it and of a note is, as the design
/// sets them: 18, whatever the size of the letters.
const LINE_HEIGHT: f32 = 18.0;
/// How tall a card's title line is (15 in the design's 1.4).
const TITLE_LINE: f32 = 21.0;
/// The room between the parts of a card's heading.
const HEAD_BETWEEN: f32 = 10.0;

/// Lays out words in one look, wrapped at `wrap` (never, when it is infinite), with `spacing`
/// between the letters.
pub fn words(ui: &egui::Ui, text: &str, font: FontId, ink: Color32, spacing: f32, wrap: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat { font_id: font, color: ink, extra_letter_spacing: spacing, ..Default::default() };
    job.append(text, 0.0, format);
    job.wrap.max_width = wrap;
    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
}

/// Words in one look wrapped at `wrap`, every line `line` tall, the room the letters don't take
/// shared above and below them, as the design's lines are.
pub fn lines(ui: &egui::Ui, text: &str, font: FontId, ink: Color32, wrap: f32, line: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat { font_id: font, color: ink, line_height: Some(line), ..Default::default() };
    job.append(text, 0.0, format);
    job.wrap.max_width = wrap;
    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
}

/// Words on one line, cut short with "…" where they would pass `wrap`.
pub fn one_line(ui: &egui::Ui, text: &str, font: FontId, ink: Color32, wrap: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat { font_id: font, color: ink, ..Default::default() });
    job.wrap = egui::text::TextWrapping::truncate_at_width(wrap);
    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
}

/// A page's card: the well with its title, a line in small capitals beside it saying what it is
/// for, and `aside` in the same capitals at its right end; `add` draws what it holds.
pub fn card(ui: &mut egui::Ui, title: &str, about: &str, aside: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    Frame::new()
        .fill(CARD)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(egui::Margin { left: 18, right: 18, top: 14, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(width - 36.0);
            // Every room in a card is the design's own, with nothing added between.
            ui.spacing_mut().item_spacing.y = 0.0;
            card_head(ui, title, about, aside);
            add(ui);
        });
    // 14 between cards, as the design has, whatever the page puts between what it holds.
    ui.add_space((14.0 - ui.spacing().item_spacing.y).max(0.0));
}

/// A card's head: its title in 15, then its line in small capitals, and its aside at the right
/// end, as the design's heading has them. The line and the aside share what the title leaves, in
/// proportion to their lengths, each wrapping in its share, while neither takes more than two
/// lines; otherwise the aside goes under the line.
fn card_head(ui: &mut egui::Ui, title: &str, about: &str, aside: Option<&str>) {
    let width = ui.available_width();
    let title = lines(ui, title, semibold(15.0), TEXT, f32::INFINITY, TITLE_LINE);
    let caps = |text: &str, wrap: f32| words(ui, &text.to_uppercase(), fixed(CAPS), TEXT_FAINT, CAPS_SPACING, wrap);
    let after_title = (width - title.size().x - HEAD_BETWEEN).max(60.0);
    let (about, aside, beside) = match aside {
        None => (caps(about, after_title), None, true),
        Some(aside) => {
            let (about_whole, aside_whole) = (caps(about, f32::INFINITY), caps(aside, f32::INFINITY));
            // The design's spring between them takes nothing when they are short of room, and
            // leaves two gaps.
            let share = after_title - 2.0 * HEAD_BETWEEN;
            let whole = about_whole.size().x + aside_whole.size().x;
            if whole <= share {
                (about_whole, Some(aside_whole), true)
            } else {
                let about_room = share * about_whole.size().x / whole;
                let aside_room = share - about_room;
                let (about_shared, aside_shared) = (caps(about, about_room), caps(aside, aside_room));
                let fits = |galley: &egui::Galley, room: f32| galley.rows.len() <= 2 && galley.size().x <= room + 0.5;
                if fits(&about_shared, about_room) && fits(&aside_shared, aside_room) {
                    (about_shared, Some(aside_shared), true)
                } else {
                    (caps(about, after_title), Some(caps(aside, after_title)), false)
                }
            }
        }
    };
    let aside_height = aside.as_ref().map_or(0.0, |aside| aside.size().y);
    let text = if beside { about.size().y.max(aside_height) } else { about.size().y + 4.0 + aside_height };
    let height = title.size().y.max(text + 2.0);
    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let about_at = pos2(place.min.x + title.size().x + HEAD_BETWEEN, place.min.y + 2.0);
    let about_height = about.size().y;
    ui.painter().galley(place.min, title, TEXT);
    ui.painter().galley(about_at, about, TEXT_FAINT);
    if let Some(aside) = aside {
        let at = if beside {
            pos2(place.max.x - aside.size().x, place.min.y + 2.0)
        } else {
            about_at + vec2(0.0, about_height + 4.0)
        };
        ui.painter().galley(at, aside, TEXT_FAINT);
    }
    ui.add_space(8.0);
}

/// A row of a card: a line across its top, its name with a line of words under it at the left,
/// and a control `control` large at the right, both in the middle of the row's height. `draw`
/// draws the control in the place given.
pub fn row(ui: &mut egui::Ui, label: &str, note: &str, control: egui::Vec2, draw: impl FnOnce(&mut egui::Ui, Rect)) {
    let width = ui.available_width();
    let room = (width - control.x - 16.0).max(80.0);
    let label = lines(ui, label, semibold(13.0), TEXT, room, LINE_HEIGHT);
    let note = (!note.is_empty()).then(|| lines(ui, note, FontId::proportional(12.5), TEXT_FAINT, room, LINE_HEIGHT));
    let text = label.size().y + note.as_ref().map_or(0.0, |note| note.size().y);
    let inner = text.max(control.y);
    let (place, _) = ui.allocate_exact_size(vec2(width, inner + 24.0), Sense::hover());
    ui.painter().hline(place.x_range(), place.min.y + 0.5, Stroke::new(1.0, LINE));
    let top = place.min.y + 12.0 + (inner - text) / 2.0;
    let label_height = label.size().y;
    ui.painter().galley(pos2(place.min.x, top), label, TEXT);
    if let Some(note) = note {
        ui.painter().galley(pos2(place.min.x, top + label_height), note, TEXT_FAINT);
    }
    let at = pos2(place.max.x - control.x, place.min.y + 12.0 + (inner - control.y) / 2.0);
    draw(ui, Rect::from_min_size(at, control));
}

/// A line of small words in a card, wrapped (the design's note), 18 to a line.
pub fn note(ui: &mut egui::Ui, text: &str) {
    let width = ui.available_width().min(600.0);
    let galley = lines(ui, text, FontId::proportional(12.0), TEXT_FAINT, width, LINE_HEIGHT);
    let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y), Sense::hover());
    ui.painter().galley(place.min, galley, TEXT_FAINT);
    ui.add_space(12.0);
}

/// How large an On/Off key is: as wide as the longer of its two words, in the language set.
pub fn on_off_size(ui: &egui::Ui) -> egui::Vec2 {
    let words = [t("settings.on", "On"), t("settings.off", "Off")];
    vec2(widest(ui, &words, &FontId::proportional(12.5)) + 26.0, 30.0)
}

/// The On/Off key: the lighter grey with "On" while on, the button grey with "Off" while off. It
/// keeps one width whichever it says.
pub fn on_off(ui: &egui::Ui, rect: Rect, id: egui::Id, on: bool) -> egui::Response {
    let words = if on { t("settings.on", "On") } else { t("settings.off", "Off") };
    flat_button(ui, rect, id, words, FontId::proportional(12.5), if on { Flat::On } else { Flat::Plain }, true)
}

/// How large a flat key with words is: as wide as they are, and 30 high.
pub fn key_size(ui: &egui::Ui, words: &str) -> egui::Vec2 {
    vec2(widest(ui, &[words], &FontId::proportional(12.5)) + 26.0, 30.0)
}

/// A flat key with words in `rect`: the button grey, or, `quiet`, no block at all and faint
/// words that grow lighter under the pointer (for taking something away again).
pub fn key(ui: &egui::Ui, rect: Rect, id: egui::Id, words: &str, quiet: bool) -> egui::Response {
    if !quiet {
        return flat_button(ui, rect, id, words, FontId::proportional(12.5), Flat::Plain, true);
    }
    let response = ui.interact(rect, id, Sense::click());
    let ink = if response.hovered() { TEXT_DIM } else { TEXT_FAINT };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, words, FontId::proportional(12.5), ink);
    response
}

/// A flat key whose words follow a green light: one that has just done what it says ("Copied").
pub fn lit_key(ui: &egui::Ui, rect: Rect, id: egui::Id, words: &str) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    crate::studio::paint_flat(ui.painter(), rect, 9.0, Flat::Plain, response.hovered());
    let galley = ui.painter().layout_no_wrap(words.to_owned(), FontId::proportional(12.5), TEXT);
    let left = rect.center().x - (galley.size().x + 14.0) / 2.0;
    paint_light(ui.painter(), pos2(left + 3.5, rect.center().y), Some(crate::studio::READY));
    ui.painter().galley(pos2(left + 14.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    response
}

/// Two lists side by side under a line (one above the other where there isn't room for 240
/// each), each with its heading in small capitals: what is allowed, each line ticked in green,
/// and what never is, each crossed in faint.
pub fn two_lists(ui: &mut egui::Ui, (yes_title, yes): (&str, &[&str]), (no_title, no): (&str, &[&str])) {
    let width = ui.available_width();
    let beside = width >= 240.0 * 2.0 + 24.0;
    let column = if beside { (width - 24.0) / 2.0 } else { width };
    let list = |ui: &egui::Ui, title: &str, lines: &[&str], allowed: bool, at: Pos2| -> f32 {
        let heading =
            words(ui, &title.to_uppercase(), crate::studio::fixed_semibold(CAPS), TEXT_DIM, CAPS_SPACING, column);
        let mut y = at.y + heading.size().y + 8.0;
        ui.painter().galley(at, heading, TEXT_DIM);
        for line in lines {
            let ink = if allowed { TEXT } else { TEXT_DIM };
            let galley = words(ui, line, FontId::proportional(13.0), ink, 0.0, column - 20.0);
            let (mark, colour) = if allowed { ("\u{2713}", crate::studio::READY) } else { ("\u{00d7}", TEXT_FAINT) };
            ui.painter().text(pos2(at.x, y), Align2::LEFT_TOP, mark, crate::studio::semibold(13.0), colour);
            let height = galley.size().y;
            ui.painter().galley(pos2(at.x + 20.0, y), galley, ink);
            y += height + 6.0;
        }
        y - 6.0 - at.y
    };
    let top = ui.cursor().min;
    ui.painter().hline(top.x..=top.x + width, top.y + 0.5, Stroke::new(1.0, LINE));
    let first = list(ui, yes_title, yes, true, top + vec2(0.0, 12.0));
    let second_at = if beside { top + vec2(column + 24.0, 12.0) } else { top + vec2(0.0, 12.0 + first + 12.0) };
    let second = list(ui, no_title, no, false, second_at);
    let height = if beside { first.max(second) } else { first + 12.0 + second };
    ui.allocate_exact_size(vec2(width, 12.0 + height + 12.0), Sense::hover());
}

/// The line under a card's tiles: what the last click did, with a light of its tone, at the left,
/// and a standing note at the right (under it when both don't fit on one line).
pub fn said_line(ui: &mut egui::Ui, said: Option<(crate::cli_install::Tone, &str)>, note: &str) {
    use crate::cli_install::Tone;
    let width = ui.available_width();
    let font = FontId::proportional(12.0);
    let note_whole = words(ui, note, font.clone(), TEXT_FAINT, 0.0, f32::INFINITY);
    let said_whole = said.map(|(_, text)| words(ui, text, font.clone(), TEXT_FAINT, 0.0, f32::INFINITY));
    let said_width = said_whole.as_ref().map_or(0.0, |said| said.size().x + 15.0);
    let one_line = said_width + 16.0 + note_whole.size().x <= width;
    let said_galley = said.map(|(_, text)| words(ui, text, font.clone(), TEXT_FAINT, 0.0, width - 15.0));
    let note_galley = if one_line { note_whole } else { words(ui, note, font, TEXT_FAINT, 0.0, width) };
    let said_height = said_galley.as_ref().map_or(0.0, |said| said.size().y);
    let height =
        if one_line { said_height.max(note_galley.size().y) } else { said_height + 4.0 + note_galley.size().y };
    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if let (Some((tone, _)), Some(galley)) = (said, said_galley) {
        let colour = match tone {
            Tone::Trouble => crate::studio::TALLY,
            Tone::Care => crate::studio::CARE,
            Tone::Good => crate::studio::READY,
            Tone::Plain => TEXT_FAINT,
        };
        ui.painter().circle_filled(pos2(place.min.x + 3.5, place.min.y + 8.0), 3.5, colour);
        ui.painter().galley(pos2(place.min.x + 15.0, place.min.y), galley, TEXT_FAINT);
    }
    let note_at = if one_line {
        pos2(place.max.x - note_galley.size().x, place.min.y)
    } else {
        pos2(place.min.x, place.min.y + if said.is_some() { said_height + 4.0 } else { 0.0 })
    };
    ui.painter().galley(note_at, note_galley, TEXT_FAINT);
    ui.add_space(12.0);
}

/// A part of a card folded away under a line until its title is clicked, as the design's More:
/// `open` opens it (a capture run's, or after a copy).
pub fn fold(ui: &mut egui::Ui, salt: &str, title: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    let top = ui.cursor().min;
    ui.painter().hline(top.x..=top.x + width, top.y + 0.5, Stroke::new(1.0, LINE));
    ui.add_space(12.0);
    let id = ui.make_persistent_id(salt);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    if open {
        state.set_open(true);
    }
    let (place, response) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click());
    if response.clicked() {
        state.toggle(ui);
    }
    let ink = if response.hovered() { TEXT } else { TEXT_DIM };
    // The design's marker: a filled triangle pointing at the title while closed, down while open.
    let middle = pos2(place.min.x + 4.0, place.center().y);
    let corners = if state.is_open() {
        [middle + vec2(-4.0, -2.5), middle + vec2(4.0, -2.5), middle + vec2(0.0, 3.5)]
    } else {
        [middle + vec2(-2.5, -4.0), middle + vec2(3.5, 0.0), middle + vec2(-2.5, 4.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(corners.to_vec(), ink, Stroke::NONE));
    ui.painter().text(
        pos2(place.min.x + 14.0, place.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.0),
        ink,
    );
    state.show_body_unindented(ui, |ui| {
        ui.add_space(10.0);
        add(ui);
    });
    state.store(ui.ctx());
    ui.add_space(12.0);
}

/// Words to copy, as they will be copied: in the fixed-width face on the panel's dark, wrapped.
pub fn code(ui: &mut egui::Ui, text: &str) {
    let width = ui.available_width();
    // 12 on lines 1.6 apart, as the design's.
    let galley = lines(ui, text, crate::studio::fixed(12.0), TEXT_DIM, width - 28.0, 19.2);
    let (place, _) = ui.allocate_exact_size(vec2(width, galley.size().y + 24.0), Sense::hover());
    ui.painter().rect_filled(place, 9.0, crate::studio::PANEL);
    ui.painter().galley(place.min + vec2(14.0, 12.0), galley, TEXT_DIM);
}

/// What a key among a few shows above its name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Picture {
    /// Three small slides, the ones the copyright line falls on marked along their foot.
    Strip([bool; 3]),
    /// How one slide gives way to the next (`None`: the dots of More…).
    Transition(Option<sp_model::TransitionKind>),
    /// A language's flag.
    Flag(crate::i18n::Language),
    /// A small screen: this computer's.
    Computer,
    /// How much an assistant may do: a screen alone (0), lines beside it (1), or lines going to
    /// a lit one (2).
    Level(u8),
}

impl Picture {
    /// The picture's height, and the room under it before the name.
    fn height(self) -> (f32, f32) {
        match self {
            Picture::Strip(_) => (13.0, 6.0),
            Picture::Transition(_) => (30.0, 6.0),
            Picture::Flag(_) | Picture::Computer => (20.0, 6.0),
            Picture::Level(_) => (21.0, 0.0),
        }
    }

    /// The picture's width.
    fn width(self) -> f32 {
        match self {
            Picture::Strip(_) => 66.0,
            Picture::Transition(_) => 56.0,
            Picture::Flag(_) | Picture::Computer => 30.0,
            Picture::Level(0) => 34.0,
            Picture::Level(1) => 68.0,
            Picture::Level(_) => 86.0,
        }
    }

    /// Whether it stays at the top of its key while the words sit at the foot.
    fn on_top(self) -> bool {
        matches!(self, Picture::Level(_))
    }
}

/// One key among a few: its picture, its name and a line under it.
pub struct Pick {
    pub picture: Picture,
    pub name: String,
    pub line: String,
}

/// A row of keys among which one is chosen, sharing the row's width: each a flat key with its
/// light at the top right (lit when chosen), a small picture, its name and a line, all as tall as
/// the tallest and at least `least` tall. Returns the key clicked, and every key's response (for
/// a menu one of them opens).
pub fn picks(
    ui: &mut egui::Ui,
    salt: &str,
    keys: &[Pick],
    chosen: Option<usize>,
    least: f32,
) -> (Option<usize>, Vec<egui::Response>) {
    if keys.is_empty() {
        return (None, Vec::new());
    }
    let width = ui.available_width();
    // As many across as fit with no word broken, the rest on rows under them.
    let least_wide = least_width(ui, keys);
    let across = (1..=keys.len())
        .rev()
        .find(|across| (width - BETWEEN * (*across - 1) as f32) / *across as f32 >= least_wide)
        .unwrap_or(1);
    // Rows as even as they can be: five keys that don't fit across go three and two.
    let across = keys.len().div_ceil(keys.len().div_ceil(across));
    let each = (width - BETWEEN * (across - 1) as f32) / across as f32;
    let inner = (each - 24.0).max(10.0);
    let laid: Vec<_> = keys
        .iter()
        .map(|key| {
            let name = words(ui, &key.name, semibold(13.0), TEXT, 0.0, inner);
            let line =
                (!key.line.is_empty()).then(|| words(ui, &key.line, FontId::proportional(12.0), TEXT_DIM, 0.0, inner));
            (name, line)
        })
        .collect();
    let content = |key: &Pick, (name, line): &(Arc<egui::Galley>, Option<Arc<egui::Galley>>)| {
        let (picture, under) = key.picture.height();
        // A picture wide enough to reach under the light starts below it.
        let clear = if key.picture.width() > inner - 14.0 && !key.picture.on_top() { 12.0 } else { 0.0 };
        clear + picture + under + 3.0 + name.size().y + line.as_ref().map_or(0.0, |line| 3.0 + line.size().y)
    };
    let tallest = keys.iter().zip(&laid).map(|(key, laid)| content(key, laid)).fold(0.0, f32::max);
    let height = (tallest + 23.0).max(least);
    let rows = keys.len().div_ceil(across);
    ui.add_space(4.0);
    let (place, _) =
        ui.allocate_exact_size(vec2(width, rows as f32 * height + (rows - 1) as f32 * BETWEEN), Sense::hover());
    let mut clicked = None;
    let mut responses = Vec::new();
    for (index, (key, (name, line))) in keys.iter().zip(laid).enumerate() {
        let (column, row) = ((index % across) as f32, (index / across) as f32);
        let corner = place.min + vec2(column * (each + BETWEEN), row * (height + BETWEEN));
        let rect = Rect::from_min_size(corner, vec2(each, height));
        let response = ui.interact(rect, ui.scope_id().with((salt, index)), Sense::click());
        let on = chosen == Some(index);
        let painter = ui.painter();
        if on {
            painter.rect_filled(rect, RADIUS_CARD, SELECTED);
            painter.rect_stroke(rect, RADIUS_CARD, Stroke::new(1.0, ON_EDGE), StrokeKind::Inside);
        } else {
            painter.rect_filled(rect, RADIUS_CARD, if response.hovered() { BUTTON_HOVER } else { BUTTON });
        }
        // The light: lit in ink when chosen, the dark of a key's edge when not.
        let light = pos2(rect.max.x - 13.5, rect.min.y + 13.5);
        if on {
            painter.circle_filled(light, 6.5, TEXT.gamma_multiply(0.18));
            painter.circle_filled(light, 3.5, TEXT);
        } else {
            paint_light(painter, light, None);
        }
        // From the foot up: the line, the name, the picture.
        let left = rect.min.x + 12.0;
        let mut y = rect.max.y - 11.0;
        if let Some(line) = line {
            y -= line.size().y;
            painter.galley(pos2(left, y), line, TEXT_DIM);
            y -= 3.0;
        }
        y -= name.size().y;
        painter.galley(pos2(left, y), name, TEXT);
        y -= 3.0;
        let (picture_height, under) = key.picture.height();
        let top = if key.picture.on_top() { rect.min.y + 12.0 } else { y - under - picture_height };
        paint_picture(painter, key.picture, pos2(left, top), if on { TEXT } else { TEXT_DIM });
        if response.clicked() {
            clicked = Some(index);
        }
        responses.push(response);
    }
    ui.add_space(12.0);
    (clicked, responses)
}

/// The room between two keys among a few.
const BETWEEN: f32 = 8.0;

/// The least width a key among a few takes so that none of its words is broken: its longest
/// word, and its padding.
pub fn least_width(ui: &egui::Ui, keys: &[Pick]) -> f32 {
    let longest = |text: &str, font: FontId| {
        text.split_whitespace()
            .map(|word| ui.painter().layout_no_wrap(word.to_owned(), font.clone(), TEXT).size().x)
            .fold(0.0, f32::max)
    };
    keys.iter()
        .map(|key| {
            let words = longest(&key.name, semibold(13.0)).max(longest(&key.line, FontId::proportional(12.0)));
            words.max(key.picture.width())
        })
        .fold(0.0, f32::max)
        + 24.0
}

/// How wide a row of these keys must be to hold them all side by side.
pub fn row_width(ui: &egui::Ui, keys: &[Pick]) -> f32 {
    least_width(ui, keys) * keys.len() as f32 + BETWEEN * keys.len().saturating_sub(1) as f32
}

/// A key's picture, its top left corner at `at`. `ink` is the key's own colour (lighter when it
/// is chosen).
fn paint_picture(painter: &egui::Painter, picture: Picture, at: Pos2, ink: Color32) {
    const DARK: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
    match picture {
        Picture::Strip(marked) => {
            for (place, marked) in marked.into_iter().enumerate() {
                let slide = Rect::from_min_size(at + vec2(place as f32 * 23.0, 0.0), vec2(20.0, 13.0));
                painter.rect_filled(slide, 2.0, DARK);
                let foot = Rect::from_min_max(pos2(slide.min.x, slide.max.y - 3.0), slide.max);
                if marked {
                    painter.rect_filled(foot, CornerRadius { nw: 0, ne: 0, sw: 2, se: 2 }, ink);
                }
            }
        }
        Picture::Transition(kind) => {
            use sp_model::TransitionKind as K;
            let rect = Rect::from_min_size(at, vec2(56.0, 30.0));
            match kind {
                Some(K::Cut) => {
                    let (left, right) = rect.split_left_right_at_fraction(0.5);
                    painter.rect_filled(left, CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }, DARK);
                    painter.rect_filled(right, CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, TEXT);
                }
                Some(K::Dissolve) => gradient(painter, rect, 4.0, &[(0.0, DARK), (1.0, TEXT)]),
                Some(_) => gradient(
                    painter,
                    rect,
                    4.0,
                    &[(0.0, DARK), (0.3, DARK), (0.5, Color32::from_gray(0x5a)), (0.7, TEXT), (1.0, TEXT)],
                ),
                None => {
                    painter.rect_filled(rect, 4.0, DARK);
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "\u{22ef}",
                        FontId::proportional(13.0),
                        TEXT_DIM,
                    );
                }
            }
        }
        Picture::Flag(language) => flag(painter, Rect::from_min_size(at, vec2(30.0, 20.0)), language),
        Picture::Computer => {
            // The design's small screen: 16 × 10 on a stand, in 24 × 16, drawn 30 × 20.
            let unit = 1.25;
            let screen = Rect::from_min_size(at + vec2(4.0 * unit, 1.5 * unit), vec2(16.0 * unit, 10.0 * unit));
            let stroke = Stroke::new(1.4 * unit, TEXT_DIM);
            painter.rect_stroke(screen, 1.5 * unit, stroke, StrokeKind::Middle);
            painter.line_segment([at + vec2(9.0 * unit, 14.5 * unit), at + vec2(15.0 * unit, 14.5 * unit)], stroke);
        }
        Picture::Level(level) => {
            // A screen, dark or lit, with a faint edge; three lines of words before it; and for
            // the last, an arrow from the words to a lit screen, 6 apart, as the design draws them.
            let screen = |x: f32, lit: bool| {
                let rect = Rect::from_min_size(pos2(x, at.y), vec2(34.0, 21.0));
                painter.rect_filled(rect, 3.0, if lit { TEXT_DIM } else { DARK });
                painter.rect_stroke(rect, 3.0, Stroke::new(1.0, TEXT_FAINT), StrokeKind::Outside);
            };
            let lines = |x: f32| {
                for row in 0..3 {
                    let line = Rect::from_min_size(pos2(x, at.y + 3.0 + row as f32 * 6.0), vec2(28.0, 3.0));
                    painter.rect_filled(line, 1.0, ink);
                }
            };
            match level {
                0 => screen(at.x, false),
                1 => {
                    lines(at.x);
                    screen(at.x + 34.0, false);
                }
                _ => {
                    lines(at.x);
                    let arrow = pos2(at.x + 40.0, at.y + 10.5);
                    painter.text(arrow, Align2::LEFT_CENTER, "\u{2192}", FontId::proportional(13.0), TEXT_DIM);
                    screen(at.x + 52.0, true);
                }
            }
        }
    }
}

/// A rectangle with rounded corners filled with colours that change across it, from left to
/// right through `stops` (each a place from 0 to 1 and its colour).
fn gradient(painter: &egui::Painter, rect: Rect, corner: f32, stops: &[(f32, Color32)]) {
    // The rounded outline, then each band between two stops cut from it: each band is convex,
    // and its colour changes evenly across it, so a fan of triangles from one corner draws it.
    let outline = rounded_outline(rect, corner);
    let colour_at = |x: f32| {
        let along = ((x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
        let at = stops.iter().position(|(place, _)| *place >= along).unwrap_or(stops.len() - 1);
        if at == 0 {
            return stops[0].1;
        }
        let (from, to) = (stops[at - 1], stops[at]);
        let share = if to.0 > from.0 { (along - from.0) / (to.0 - from.0) } else { 1.0 };
        from.1.lerp_to_gamma(to.1, share)
    };
    let mut mesh = egui::Mesh::default();
    for pair in stops.windows(2) {
        let (from, to) = (rect.min.x + pair[0].0 * rect.width(), rect.min.x + pair[1].0 * rect.width());
        if to - from < 0.01 {
            continue;
        }
        let band = clip_x(&clip_x(&outline, from, true), to, false);
        if band.len() < 3 {
            continue;
        }
        let first = mesh.vertices.len() as u32;
        for point in &band {
            mesh.colored_vertex(*point, colour_at(point.x));
        }
        for at in 1..band.len() as u32 - 1 {
            mesh.add_triangle(first, first + at, first + at + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The outline of a rectangle with rounded corners, clockwise from the top left.
pub fn rounded_outline(rect: Rect, corner: f32) -> Vec<Pos2> {
    let corner = corner.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut points = Vec::new();
    let centres = [
        (pos2(rect.max.x - corner, rect.min.y + corner), -0.25),
        (pos2(rect.max.x - corner, rect.max.y - corner), 0.0),
        (pos2(rect.min.x + corner, rect.max.y - corner), 0.25),
        (pos2(rect.min.x + corner, rect.min.y + corner), 0.5),
    ];
    for (centre, start) in centres {
        for step in 0..=6 {
            let turn = (start + 0.25 * step as f32 / 6.0) * std::f32::consts::TAU;
            points.push(centre + corner * vec2(turn.cos(), turn.sin()));
        }
    }
    points
}

/// The part of a convex outline to the right of `x` (`keep_right`), or to its left.
fn clip_x(points: &[Pos2], x: f32, keep_right: bool) -> Vec<Pos2> {
    let inside = |point: &Pos2| if keep_right { point.x >= x } else { point.x <= x };
    let mut out = Vec::new();
    for (at, point) in points.iter().enumerate() {
        let next = points[(at + 1) % points.len()];
        if inside(point) {
            out.push(*point);
        }
        if inside(point) != inside(&next) {
            let share = (x - point.x) / (next.x - point.x);
            out.push(pos2(x, point.y + (next.y - point.y) * share));
        }
    }
    out
}
/// A language's flag, 30 × 20 with small rounded corners and a faint ring: the flag most people
/// know that language by (the United States for English, Spain, Russia, Ukraine).
pub fn flag(painter: &egui::Painter, rect: Rect, language: crate::i18n::Language) {
    use crate::i18n::Language;
    const R: u8 = 3;
    let unit = rect.height() / 16.0;
    let band = |from: f32, to: f32| {
        Rect::from_min_max(pos2(rect.min.x, rect.min.y + from * unit), pos2(rect.max.x, rect.min.y + to * unit))
    };
    let top = CornerRadius { nw: R, ne: R, sw: 0, se: 0 };
    let foot = CornerRadius { nw: 0, ne: 0, sw: R, se: R };
    match language {
        Language::English => {
            let (red, white) = (Color32::from_rgb(0xb2, 0x22, 0x34), Color32::WHITE);
            let stripe = 16.0 / 7.0;
            for at in 0..7 {
                let corners = match at {
                    0 => top,
                    6 => foot,
                    _ => CornerRadius::ZERO,
                };
                let colour = if at % 2 == 0 { red } else { white };
                painter.rect_filled(band(at as f32 * stripe, (at + 1) as f32 * stripe), corners, colour);
            }
            let canton = Rect::from_min_size(rect.min, vec2(10.5 * unit, 4.0 * stripe * unit));
            painter.rect_filled(
                canton,
                CornerRadius { nw: R, ne: 0, sw: 0, se: 0 },
                Color32::from_rgb(0x3c, 0x3b, 0x6e),
            );
        }
        Language::Spanish => {
            painter.rect_filled(rect, R, Color32::from_rgb(0xaa, 0x15, 0x1b));
            painter.rect_filled(band(4.0, 12.0), 0.0, Color32::from_rgb(0xf1, 0xbf, 0x00));
        }
        Language::Russian => {
            painter.rect_filled(band(0.0, 5.33), top, Color32::WHITE);
            painter.rect_filled(band(5.33, 10.67), 0.0, Color32::from_rgb(0x00, 0x39, 0xa6));
            painter.rect_filled(band(10.67, 16.0), foot, Color32::from_rgb(0xd5, 0x2b, 0x1e));
        }
        Language::Ukrainian => {
            painter.rect_filled(band(0.0, 8.0), top, Color32::from_rgb(0x00, 0x57, 0xb7));
            painter.rect_filled(band(8.0, 16.0), foot, Color32::from_rgb(0xff, 0xd7, 0x00));
        }
    }
    painter.rect_stroke(rect, R, Stroke::new(1.0, Color32::from_white_alpha(31)), StrokeKind::Outside);
}
