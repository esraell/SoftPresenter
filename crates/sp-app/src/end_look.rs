//! The look of the end of a song's mark, which belongs to a theme (`sp_model::EndLook`): which
//! mark, its size beside the words', its colour and its place. One small window sets it, opened
//! from two doors: Settings › Slides' "Change how it looks…", for the main theme, and the Theme
//! Editor's "Song end" row, for the theme open there. It is drawn in the window its door is in.
//!
//! At its left is a small last slide with the mark as it will show; at its right four rows,
//! each one line (Mark, Size, Colour, Place), and a sentence. A change applies at once, and
//! Done closes it; there is nothing to save. Nothing in it moves as choices change: every row
//! keeps its place and height, the small slide's words stay put wherever the mark goes, and the
//! sentence keeps the room of the longer of its two wordings.
//!
//! Whether the mark shows at all stays Settings › Slides' one switch (`end_mark`).

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use sp_engine::Show;
use sp_model::{EndLook, EndMark, EndPlace, Rgba, Uuid};

use crate::home::settings::parts;
use crate::home::{HomeState, ThemeRequest};
use crate::i18n::{t, tf};
use crate::studio::{
    BUTTON, CARD, Flat, LINE, RADIUS, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, fixed_semibold, flat_button, paint_spaced,
    semibold, widest,
};

/// Where the window was opened from, which is the window it is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Door {
    /// Settings › Slides, in the Settings window: the main theme's.
    Settings,
    /// The Theme Editor's own rows, in the operator's window: the theme open there.
    Editor,
}

/// The window, while it is open: the theme it is for, by its id (themes can be moved, renamed
/// or deleted meanwhile), and its door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub theme: Uuid,
    pub door: Door,
}

/// The colours offered for the mark, beside the words' own: the design's eight.
pub const COLOURS: [Rgba; 8] = [
    Rgba::rgb(0xff, 0xff, 0xff),
    Rgba::rgb(0x00, 0x00, 0x00),
    Rgba::rgb(0xd2, 0xce, 0xc6),
    Rgba::rgb(0xff, 0x4a, 0x1a),
    Rgba::rgb(0xff, 0xc4, 0x00),
    Rgba::rgb(0x34, 0xc5, 0x6b),
    Rgba::rgb(0x42, 0x85, 0xf4),
    Rgba::rgb(0xcc, 0x4e, 0xb4),
];

/// The window is as wide as this where its own window has the room.
const WIDTH: f32 = 880.0;
/// The small slide's width where there is room for it beside the rows, and the least it is.
const SLIDE: f32 = 300.0;
const SLIDE_LEAST: f32 = 160.0;
/// The room between the slide and the rows, a row's label column, the room after it, a row's
/// height and the room between rows, as the design has them.
const BESIDE: f32 = 22.0;
const LABEL: f32 = 62.0;
const AFTER_LABEL: f32 = 10.0;
const ROW: f32 = 34.0;
const BETWEEN_ROWS: f32 = 14.0;
/// A chip: its height, a mark chip's width, and the room between chips.
const CHIP: f32 = 32.0;
const MARK_CHIP: f32 = 62.0;
const BETWEEN: f32 = 6.0;
/// A colour's swatch.
const SWATCH: f32 = 24.0;
/// The size's percentage has one width whatever it says.
const PERCENT: f32 = 48.0;

/// Opens the window for the theme at a place in the list, from a door.
pub fn open(state: &mut HomeState, theme: usize, door: Door) {
    if let Some(id) = state.theme_ids.get(theme) {
        state.end_look = Some(Open { theme: *id, door });
    }
}

/// Whether the Theme Editor, the editor door's own window, is what the operator's window shows.
fn in_the_theme_editor(state: &HomeState) -> bool {
    state.editing.is_some() && state.theme_edit.is_some()
}

/// Whether the window is open over the operator's window, where the keys that run the show
/// wait for it and Escape is its own: opened from the Theme Editor, and the editor still there
/// to draw it in. Left open when the editor was left (another view asked for by a key, the menu,
/// `sp` or the phone), it is over nothing, and holds no key back.
pub fn over_the_operators_window(state: &HomeState) -> bool {
    state.end_look.is_some_and(|open| open.door == Door::Editor) && in_the_theme_editor(state)
}

/// The Theme Editor has gone, however it was left: a window opened from it goes with it, so it
/// isn't there again the next time the editor opens. Called before the operator's window is
/// drawn, whichever view it shows.
pub fn editor_left(state: &mut HomeState) {
    if state.end_look.is_some_and(|open| open.door == Door::Editor) && !in_the_theme_editor(state) {
        state.end_look = None;
    }
}

/// The Settings window has closed: a window opened from it goes with it.
pub fn settings_closed(state: &mut HomeState) {
    if state.end_look.is_some_and(|open| open.door == Door::Settings) {
        state.end_look = None;
    }
}

/// Gives the theme of this id its look, in the list and in the Theme Editor's copy of it when
/// that theme is open there (so a later change in the editor doesn't put the old look back).
/// The theme's place in the list when its look changed; `None` when it has it already, or no
/// theme has that id any more.
pub fn give(
    themes: &mut [sp_model::Theme],
    edit: Option<&mut crate::home::ThemeEdit>,
    id: Uuid,
    look: EndLook,
) -> Option<usize> {
    let index = themes.iter().position(|theme| theme.id == id)?;
    if themes[index].end_look == look {
        return None;
    }
    themes[index].end_look = look;
    if let Some(edit) = edit.filter(|edit| edit.index == index) {
        edit.theme.end_look = look;
    }
    Some(index)
}

/// Testing aid: `SOFTPRESENTER_END_LOOK=MARK,SIZE,PLACE,COLOUR` (as `•,120,bottom,ffc400`; any
/// part may be left empty), in a copy with a data folder of its own, gives the main theme that
/// look for the run, to picture a last slide and the window with it. It is read as a theme's
/// file is, so what can't be read is the usual look's; nothing is saved unless the look is then
/// changed in the window.
pub fn asked_for_the_run() -> Option<EndLook> {
    std::env::var_os("SOFTPRESENTER_DATA")?;
    let asked = std::env::var("SOFTPRESENTER_END_LOOK").ok()?;
    let mut parts = asked.split(',').map(str::trim);
    let (mark, size, place, colour) = (parts.next(), parts.next(), parts.next(), parts.next());
    let mut saved = serde_json::Map::new();
    if let Some(mark) = mark {
        saved.insert("mark".to_owned(), mark.into());
    }
    if let Some(size) = size.and_then(|size| size.parse::<u16>().ok()) {
        saved.insert("size".to_owned(), size.into());
    }
    if let Some(place) = place {
        saved.insert("place".to_owned(), place.into());
    }
    if let Some(rgb) = colour.filter(|hex| hex.len() == 6).and_then(|hex| u32::from_str_radix(hex, 16).ok()) {
        let part = |shift: u32| (rgb >> shift) & 0xff;
        saved.insert("colour".to_owned(), serde_json::json!({ "r": part(16), "g": part(8), "b": part(0), "a": 255 }));
    }
    serde_json::from_value(serde_json::Value::Object(saved)).ok()
}

/// The look of the theme at a place in the list (the usual one for a place with no theme).
pub fn look_of(state: &HomeState, theme: usize) -> EndLook {
    state.theme_end_looks.get(theme).copied().unwrap_or_default()
}

/// A mark as a chip or a row shows it: three of it, two spaces apart.
pub fn chip_words(mark: EndMark) -> String {
    let glyph = mark.glyph();
    format!("{glyph}\u{a0}\u{a0}{glyph}\u{a0}\u{a0}{glyph}")
}

/// The face a mark is written in at a size: the reading face the small slide's words are in
/// where it has the mark (a book face may lack the squares), else the interface's.
pub fn mark_font(ctx: &egui::Context, mark: EndMark, size: f32) -> FontId {
    let reading = crate::studio::reading(size);
    if ctx.fonts_mut(|fonts| fonts.has_glyph(&reading, mark.glyph())) { reading } else { FontId::proportional(size) }
}

/// The sentence under the rows: the no-room rule while the mark is on, or that it is switched
/// off.
fn sentence(on: bool) -> &'static str {
    if on {
        t(
            "settings.endlook.note",
            "Where there is no room for the mark under the words, it isn\u{2019}t drawn. The words are never moved or made smaller for it.",
        )
    } else {
        t(
            "settings.endlook.note.off",
            "The mark is switched off in Settings \u{203a} Slides. Its look is kept for when it is on.",
        )
    }
}

/// The small last slide: the first song's last slide as sung (the one the screens mark,
/// `end_mark::last_sung`), the last two lines of its words and the last line of its translation
/// when it has one, and the mark as `look` has it while `on`: under them, half a line down, or
/// at the bottom of the slide. The words keep their place whichever, and with the mark off.
pub fn sample(ui: &egui::Ui, place: Rect, show: &Show, look: &EndLook, on: bool) {
    // Drawn for a slide 192 wide, and larger with the slide.
    let scale = place.width() / 192.0;
    let (words_size, translation_size) = (15.0 * scale, 11.0 * scale);
    let painter = ui.painter().with_clip_rect(place);
    painter.rect_filled(place, RADIUS, Color32::BLACK);
    let slide = show.presentations().iter().find_map(|song| {
        let (_, slide) = crate::end_mark::last_sung(song)?;
        song.slide(slide).map(|(_, slide)| slide)
    });
    // The last `most` lines of a box of the slide's words.
    let last_lines = |role: sp_model::Role, most: usize| -> Vec<String> {
        let words = slide.and_then(|slide| slide.objects.iter().find(|o| o.role.as_ref() == Some(&role)));
        let words = words.filter(|object| object.holds_song_words()).and_then(|object| object.as_text());
        let lines: Vec<&str> = words.unwrap_or_default().lines().filter(|line| !line.trim().is_empty()).collect();
        lines[lines.len().saturating_sub(most)..].iter().map(|line| (*line).to_owned()).collect()
    };
    let room = place.width() - 28.0 * scale;
    let lines: Vec<_> = last_lines(sp_model::Role::primary(), 2)
        .iter()
        .map(|line| (parts::one_line(ui, line, crate::studio::reading(words_size), TEXT, room), TEXT))
        .chain(last_lines(sp_model::Role::translation(), 1).iter().map(|line| {
            (parts::one_line(ui, line, crate::studio::reading(translation_size), TEXT_DIM, room), TEXT_DIM)
        }))
        .collect();
    let gap = 2.0 * scale;
    let height: f32 =
        lines.iter().map(|(line, _)| line.size().y).sum::<f32>() + gap * lines.len().saturating_sub(1) as f32;
    // The words sit a little above the middle, where they stay whatever the mark does.
    let mut y = place.center().y - 8.0 * scale - height / 2.0;
    for (line, ink) in lines {
        let line_height = line.size().y;
        painter.galley(pos2(place.center().x - line.size().x / 2.0, y), line, ink);
        y += line_height + gap;
    }
    if !on {
        return;
    }
    let size = words_size * f32::from(look.size) / 100.0;
    let ink = look.colour.map_or(TEXT, crate::home::color32);
    let mark = parts::one_line(ui, &look.mark.text(), mark_font(ui.ctx(), look.mark, size), ink, f32::INFINITY);
    let top = match look.place {
        EndPlace::Under => y - gap + words_size * 0.5,
        // As low as it may go: 3% above the slide's bottom.
        EndPlace::Bottom => place.max.y - place.height() * 0.03 - mark.size().y,
    };
    painter.galley(pos2(place.center().x - mark.size().x / 2.0, top), mark, ink);
}

/// A chip: the well's grey with its words, outlined in the one accent when it is the choice.
fn chip(ui: &egui::Ui, rect: Rect, id: egui::Id, words: &str, font: FontId, chosen: bool) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    ui.painter().rect_filled(rect, 8.0, BUTTON);
    if chosen {
        ui.painter().rect_stroke(rect, 8.0, Stroke::new(1.0, TALLY), StrokeKind::Inside);
    }
    let ink = if chosen || response.hovered() { TEXT } else { TEXT_DIM };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, words, font, ink);
    response
}

/// How wide a chip with words is.
fn chip_width(ui: &egui::Ui, words: &str) -> f32 {
    widest(ui, &[words], &FontId::proportional(13.0)) + 24.0
}

/// The window, if it is open from `door`: over its window, which it dims. It closes with Done,
/// its ×, Escape or a click outside it, and when its theme is no longer there.
pub fn window(ctx: &egui::Context, state: &mut HomeState, show: &Show, door: Door) {
    let Some(open) = state.end_look.filter(|open| open.door == door) else { return };
    // Its theme was deleted, or the editor it was opened in has gone: it goes too.
    let Some(theme) = state.theme_ids.iter().position(|id| *id == open.theme) else {
        state.end_look = None;
        return;
    };
    if door == Door::Editor && state.theme_edit.is_none() {
        state.end_look = None;
        return;
    }
    let look = look_of(state, theme);
    let on = state.end_mark;
    let name = state.theme_names.get(theme).map(String::as_str).unwrap_or_default();
    let name = crate::theme_store::theme_name(open.theme, name).into_owned();
    let screen = ctx.content_rect();
    let width = WIDTH.min(screen.width() - 32.0);
    let inside = width - 36.0;
    // The rows need their label and the seven marks in one line; the slide takes what is left
    // beside them, up to its own width, and goes over them where even its least doesn't fit.
    let rows_least = LABEL + AFTER_LABEL + 7.0 * MARK_CHIP + 6.0 * BETWEEN;
    let beside = inside - BESIDE - rows_least >= SLIDE_LEAST;
    let slide_wide = if beside { (inside - BESIDE - rows_least).min(SLIDE) } else { inside.min(SLIDE) };
    let slide_size = vec2(slide_wide, (slide_wide * 9.0 / 16.0).round());
    let rows_wide = if beside { inside - slide_wide - BESIDE } else { inside };
    let mut closed = false;
    let mut chosen: Option<EndLook> = None;
    let area = egui::Area::new(egui::Id::unique(if door == Door::Editor { "end look, editor" } else { "end look" }))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .constrain(false);
    area.show(ctx, |ui| {
        ui.set_clip_rect(screen);
        let note_font = FontId::proportional(12.5);
        // The sentence keeps the room of the longer of its two wordings.
        let note_room = [true, false]
            .into_iter()
            .map(|on| parts::lines(ui, sentence(on), note_font.clone(), TEXT_FAINT, rows_wide, 18.0).size().y)
            .fold(18.0, f32::max);
        let rows_tall = 4.0 * ROW + 3.0 * BETWEEN_ROWS + BETWEEN_ROWS + note_room;
        let body = if beside { slide_size.y.max(rows_tall) } else { slide_size.y + 16.0 + rows_tall };
        let height = 14.0 + 30.0 + 14.0 + body + 18.0 + 30.0 + 16.0;
        let sheet = Rect::from_center_size(screen.center(), vec2(width, height));
        // The window under it takes no clicks; one outside the window puts it away.
        let outside = ui.allocate_rect(screen, Sense::click());
        if outside.clicked() && !outside.interact_pointer_pos().is_some_and(|at| sheet.contains(at)) {
            closed = true;
        }
        let painter = ui.painter().clone();
        painter.with_clip_rect(screen).rect_filled(screen, 0.0, Color32::from_black_alpha(153));
        painter.rect_filled(sheet, 18.0, Color32::from_rgb(0x1f, 0x1f, 0x1f));
        painter.rect_stroke(sheet, 18.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        let left = sheet.min.x + 18.0;
        let title = tf("settings.endlook.title", "The end of a song \u{b7} {theme}", &[("theme", &name)]);
        let title = parts::one_line(ui, &title, semibold(15.0), TEXT, inside - 40.0);
        painter.galley(pos2(left, sheet.min.y + 29.0 - title.size().y / 2.0), title, TEXT);
        let close = Rect::from_min_size(pos2(sheet.max.x - 12.0 - 30.0, sheet.min.y + 14.0), vec2(30.0, 30.0));
        let close_response = ui.interact(close, ui.scope_id().with("close"), Sense::click());
        if close_response.hovered() {
            painter.rect_filled(close, 7.0, CARD);
        }
        crate::icons::paint(
            &painter,
            crate::icons::Icon::Close,
            Rect::from_center_size(close.center(), vec2(14.0, 14.0)),
            TEXT_DIM,
        );
        if close_response.on_hover_text(t("keys.close", "Close (Esc)")).clicked() {
            closed = true;
        }
        // The small last slide, and the rows beside it or under it.
        let top = sheet.min.y + 14.0 + 30.0 + 14.0;
        let slide = Rect::from_min_size(pos2(left, top), slide_size);
        sample(ui, slide, show, &look, on);
        let rows = if beside { pos2(slide.max.x + BESIDE, top) } else { pos2(left, slide.max.y + 16.0) };
        let row_top = |n: usize| rows.y + n as f32 * (ROW + BETWEEN_ROWS);
        let label = |n: usize, words: &str| {
            paint_spaced(
                ui,
                pos2(rows.x, row_top(n) + ROW / 2.0),
                Align2::LEFT_CENTER,
                &words.to_uppercase(),
                fixed_semibold(11.5),
                TEXT_DIM,
                0.575,
            );
        };
        let controls = rows.x + LABEL + AFTER_LABEL;
        let chip_top = |n: usize| row_top(n) + (ROW - CHIP) / 2.0;
        // Mark: the seven, each showing its three marks.
        label(0, t("settings.endlook.mark", "Mark"));
        for (at, mark) in EndMark::ALL.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(controls + at as f32 * (MARK_CHIP + BETWEEN), chip_top(0)),
                vec2(MARK_CHIP, CHIP),
            );
            let id = ui.scope_id().with(("mark", at));
            let font = FontId::proportional(14.0);
            if chip(ui, rect, id, &chip_words(mark), font, look.mark == mark).clicked() {
                chosen = Some(EndLook { mark, ..look });
            }
        }
        // Size: A−, the percentage in one width, A+, greyed at their ends.
        label(1, t("settings.endlook.size", "Size"));
        let square = |x: f32| Rect::from_min_size(pos2(x, chip_top(1)), vec2(CHIP, CHIP));
        let font = FontId::proportional(12.5);
        let (smaller, bigger) = (square(controls), square(controls + CHIP + BETWEEN + PERCENT + BETWEEN));
        let can_shrink = look.size > EndLook::SMALLEST;
        let can_grow = look.size < EndLook::LARGEST;
        if flat_button(ui, smaller, ui.scope_id().with("smaller"), "A\u{2212}", font.clone(), Flat::Plain, can_shrink)
            .on_hover_text(t("settings.endlook.smaller", "Smaller"))
            .clicked()
        {
            chosen = Some(look.stepped(false));
        }
        let percent = tf("settings.size.percent", "{n}%", &[("n", &crate::i18n::count(u64::from(look.size)))]);
        painter.text(
            pos2(smaller.max.x + BETWEEN + PERCENT / 2.0, smaller.center().y),
            Align2::CENTER_CENTER,
            percent,
            fixed_semibold(13.0),
            TEXT,
        );
        if flat_button(ui, bigger, ui.scope_id().with("bigger"), "A+", font, Flat::Plain, can_grow)
            .on_hover_text(t("settings.endlook.bigger", "Bigger"))
            .clicked()
        {
            chosen = Some(look.stepped(true));
        }
        painter.text(
            pos2(bigger.max.x + 10.0, bigger.center().y),
            Align2::LEFT_CENTER,
            t("settings.endlook.size.of", "of the words\u{2019} size"),
            FontId::proportional(12.5),
            TEXT_DIM,
        );
        // Colour: the words' own, or one of the eight.
        label(2, t("settings.endlook.colour", "Colour"));
        let same = t("settings.endlook.colour.words", "Same as the words");
        let same_at = Rect::from_min_size(pos2(controls, chip_top(2)), vec2(chip_width(ui, same), CHIP));
        if chip(
            ui,
            same_at,
            ui.scope_id().with("colour of the words"),
            same,
            FontId::proportional(13.0),
            look.colour.is_none(),
        )
        .clicked()
        {
            chosen = Some(EndLook { colour: None, ..look });
        }
        for (at, colour) in COLOURS.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(same_at.max.x + BETWEEN + at as f32 * (SWATCH + BETWEEN), row_top(2) + (ROW - SWATCH) / 2.0),
                vec2(SWATCH, SWATCH),
            );
            let response = ui.interact(rect, ui.scope_id().with(("colour", at)), Sense::click());
            painter.rect_filled(rect, 6.0, crate::home::color32(colour));
            let ring = if look.colour == Some(colour) { Stroke::new(2.0, TALLY) } else { Stroke::new(1.0, LINE) };
            painter.rect_stroke(rect, 6.0, ring, StrokeKind::Inside);
            if response.clicked() {
                chosen = Some(EndLook { colour: Some(colour), ..look });
            }
        }
        // Place: under the last line, or the bottom of the screen.
        label(3, t("settings.endlook.place", "Place"));
        let mut x = controls;
        for (place, words) in [
            (EndPlace::Under, t("settings.endlook.place.under", "Under the last line")),
            (EndPlace::Bottom, t("settings.endlook.place.bottom", "Bottom of the screen")),
        ] {
            let rect = Rect::from_min_size(pos2(x, chip_top(3)), vec2(chip_width(ui, words), CHIP));
            x = rect.max.x + BETWEEN;
            let id = ui.scope_id().with(("place", place == EndPlace::Bottom));
            if chip(ui, rect, id, words, FontId::proportional(13.0), look.place == place).clicked() {
                chosen = Some(EndLook { place, ..look });
            }
        }
        // The sentence, in the room of the longer one.
        let note = parts::lines(ui, sentence(on), note_font, TEXT_FAINT, rows_wide, 18.0);
        painter.galley(pos2(rows.x, row_top(3) + ROW + BETWEEN_ROWS), note, TEXT_FAINT);
        // Done.
        let done_words = t("keys.done", "Done");
        let done_wide = widest(ui, &[done_words], &FontId::proportional(12.5)) + 30.0;
        let done =
            Rect::from_min_size(pos2(sheet.max.x - 18.0 - done_wide, sheet.max.y - 16.0 - 30.0), vec2(done_wide, 30.0));
        if flat_button(ui, done, ui.scope_id().with("done"), done_words, FontId::proportional(12.5), Flat::Main, true)
            .clicked()
        {
            closed = true;
        }
    });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        closed = true;
    }
    if let Some(look) = chosen.filter(|chosen| *chosen != look) {
        // The window shows it at once; the app gives it to the theme, its file and the screens.
        if let Some(have) = state.theme_end_looks.get_mut(theme) {
            *have = look;
        }
        state.theme_request = Some(ThemeRequest::EndLook(open.theme, look));
    }
    if closed {
        state.end_look = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::settings::{Pane, settings_ui};
    use crate::i18n::Language;
    use sp_model::Theme;

    /// Settings on Slides with two themes, the built-in one the main one, and the window open
    /// for it.
    fn opened() -> HomeState {
        let builtin = Theme::builtin();
        let mut state = HomeState {
            preferences_open: true,
            preferences_pane: Pane::Slides,
            theme_ids: vec![builtin.id, Uuid::from_u128(7)],
            theme_names: vec![builtin.name.clone(), "Evening".to_owned()],
            theme_end_looks: vec![EndLook::default(); 2],
            theme_for_songs: 0,
            end_mark: true,
            ..HomeState::default()
        };
        open(&mut state, 0, Door::Settings);
        state
    }

    /// A song in two languages, whose last slide the small slide shows.
    fn song() -> Show {
        let song = sp_model::parse_song_text("Song", "One\n\nMorning\nlight\n--\nRanok\nsvitlo\n");
        Show::new(vec![std::sync::Arc::new(song)])
    }

    /// One frame of Settings in a window of this size, with these events.
    fn frame(
        ctx: &egui::Context,
        state: &mut HomeState,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let show = song();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
            events,
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| settings_ui(ui, state, &show))
    }

    const USUAL: egui::Vec2 = vec2(1280.0, 800.0);

    /// What two frames drew: every piece of words with the room it was laid out in, and every
    /// block.
    fn drawn(ctx: &egui::Context, state: &mut HomeState, size: egui::Vec2) -> (Vec<(String, Rect)>, Vec<[i32; 4]>) {
        fn walk(shape: &egui::Shape, words: &mut Vec<(String, Rect)>, blocks: &mut Vec<[i32; 4]>) {
            match shape {
                egui::Shape::Text(text) => {
                    words.push((text.galley.text().to_owned(), text.galley.rect.translate(text.pos.to_vec2())));
                }
                egui::Shape::Rect(block) => {
                    let tenths = |x: f32| (x * 10.0).round() as i32;
                    let rect = block.rect;
                    blocks.push([tenths(rect.min.y), tenths(rect.min.x), tenths(rect.width()), tenths(rect.height())]);
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, words, blocks)),
                _ => {}
            }
        }
        let (mut words, mut blocks) = (Vec::new(), Vec::new());
        for _ in 0..2 {
            let output = frame(ctx, state, size, vec![]);
            words.clear();
            blocks.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words, &mut blocks));
            output.drop_without_applying_deltas();
        }
        blocks.sort_unstable();
        blocks.dedup();
        (words, blocks)
    }

    fn click_at(ctx: &egui::Context, state: &mut HomeState, at: egui::Pos2) {
        use egui::{Event, Modifiers};
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [vec![Event::PointerMoved(at), press(true)], vec![press(false)]] {
            frame(ctx, state, USUAL, events).drop_without_applying_deltas();
        }
    }

    /// Clicks the words where the window drew them last (it is drawn over the page).
    fn click(ctx: &egui::Context, state: &mut HomeState, words: &str) {
        let (drawn, _) = drawn(ctx, state, USUAL);
        let at =
            drawn.iter().rfind(|(have, _)| have == words).unwrap_or_else(|| panic!("{words} isn\u{2019}t drawn")).1;
        click_at(ctx, state, at.center());
    }

    /// What the window asked of the app, taken.
    fn asked(state: &mut HomeState) -> Option<EndLook> {
        match state.theme_request.take() {
            Some(ThemeRequest::EndLook(_, look)) => Some(look),
            _ => None,
        }
    }

    #[test]
    fn end_look_window_sets_the_themes_mark_size_colour_and_place() {
        let ctx = crate::studio::tests::styled();
        let mut state = opened();
        let usual = EndLook::default();
        let id = state.theme_ids[0];
        // It says whose look it is, and has the seven marks, each a chip of its three.
        let (words, _) = drawn(&ctx, &mut state, USUAL);
        assert!(words.iter().any(|(have, _)| have == "The end of a song \u{b7} Built-in"), "{words:?}");
        for mark in EndMark::ALL {
            assert!(words.iter().any(|(have, _)| *have == chip_words(mark)), "{mark:?}");
        }
        // A mark chosen is asked of the app for that theme, and shows at once.
        click(&ctx, &mut state, &chip_words(EndMark::Bullet));
        assert_eq!(state.theme_request, Some(ThemeRequest::EndLook(id, EndLook { mark: EndMark::Bullet, ..usual })));
        assert_eq!(asked(&mut state).map(|look| look.mark), Some(EndMark::Bullet));
        assert_eq!(state.theme_end_looks[0].mark, EndMark::Bullet);
        assert_eq!(state.theme_end_looks[1], usual, "the other theme keeps its own");
        let (words, _) = drawn(&ctx, &mut state, USUAL);
        assert!(words.iter().any(|(have, _)| *have == EndMark::Bullet.text()), "the small slide draws it");
        // The mark chosen already asks nothing.
        click(&ctx, &mut state, &chip_words(EndMark::Bullet));
        assert_eq!(asked(&mut state), None);
        // Size: a step at a time, and nothing past its ends.
        click(&ctx, &mut state, "A+");
        assert_eq!(asked(&mut state).map(|look| look.size), Some(90));
        click(&ctx, &mut state, "A\u{2212}");
        click(&ctx, &mut state, "A\u{2212}");
        assert_eq!(state.theme_end_looks[0].size, 70);
        let (words, _) = drawn(&ctx, &mut state, USUAL);
        assert!(words.iter().any(|(have, _)| have == "70%"));
        state.theme_end_looks[0].size = EndLook::LARGEST;
        state.theme_request = None;
        click(&ctx, &mut state, "A+");
        assert_eq!((asked(&mut state), state.theme_end_looks[0].size), (None, 160), "greyed at its end");
        state.theme_end_looks[0].size = EndLook::SMALLEST;
        click(&ctx, &mut state, "A\u{2212}");
        assert_eq!((asked(&mut state), state.theme_end_looks[0].size), (None, 40));
        // Colour: one of the eight swatches, or the words' own again.
        let (_, blocks) = drawn(&ctx, &mut state, USUAL);
        let swatch = (SWATCH * 10.0) as i32;
        let swatches: Vec<[i32; 4]> =
            blocks.into_iter().filter(|block| block[2] == swatch && block[3] == swatch).collect();
        assert_eq!(swatches.len(), 8, "{swatches:?}");
        let mut across: Vec<i32> = swatches.iter().map(|block| block[1]).collect();
        across.sort_unstable();
        let fifth = pos2(across[4] as f32 / 10.0 + SWATCH / 2.0, swatches[0][0] as f32 / 10.0 + SWATCH / 2.0);
        click_at(&ctx, &mut state, fifth);
        assert_eq!(asked(&mut state).map(|look| look.colour), Some(Some(COLOURS[4])));
        click(&ctx, &mut state, "Same as the words");
        assert_eq!(asked(&mut state).map(|look| look.colour), Some(None));
        // Place.
        click(&ctx, &mut state, "Bottom of the screen");
        assert_eq!(asked(&mut state).map(|look| look.place), Some(EndPlace::Bottom));
        click(&ctx, &mut state, "Under the last line");
        assert_eq!(state.theme_end_looks[0].place, EndPlace::Under);
        // The sentence says the rule, or that the mark is off.
        let rule = "Where there is no room for the mark under the words, it isn\u{2019}t drawn. The words are never moved or made smaller for it.";
        let off = "The mark is switched off in Settings \u{203a} Slides. Its look is kept for when it is on.";
        let (words, _) = drawn(&ctx, &mut state, USUAL);
        assert!(words.iter().any(|(have, _)| have == rule) && !words.iter().any(|(have, _)| have == off));
        state.end_mark = false;
        let (words, _) = drawn(&ctx, &mut state, USUAL);
        assert!(words.iter().any(|(have, _)| have == off) && !words.iter().any(|(have, _)| have == rule));
        // The other theme's window changes that theme.
        open(&mut state, 1, Door::Settings);
        state.theme_request = None;
        click(&ctx, &mut state, &chip_words(EndMark::Square));
        let other = state.theme_ids[1];
        assert!(matches!(state.theme_request.take(), Some(ThemeRequest::EndLook(id, _)) if id == other));
    }

    #[test]
    fn end_look_window_closes_with_done_escape_a_click_outside_or_when_its_theme_or_page_goes() {
        let ctx = crate::studio::tests::styled();
        let mut state = opened();
        click(&ctx, &mut state, "Done");
        assert_eq!(state.end_look, None);
        open(&mut state, 0, Door::Settings);
        frame(&ctx, &mut state, USUAL, vec![]).drop_without_applying_deltas();
        frame(&ctx, &mut state, USUAL, vec![crate::studio::tests::key(egui::Key::Escape)])
            .drop_without_applying_deltas();
        assert_eq!(state.end_look, None, "Escape");
        open(&mut state, 0, Door::Settings);
        frame(&ctx, &mut state, USUAL, vec![]).drop_without_applying_deltas();
        click_at(&ctx, &mut state, pos2(4.0, 4.0));
        assert_eq!(state.end_look, None, "a click outside it");
        // A click inside it, on nothing, leaves it open.
        open(&mut state, 0, Door::Settings);
        let (words, _) = drawn(&ctx, &mut state, USUAL);
        let title = words.iter().rfind(|(have, _)| have.starts_with("The end of a song")).expect("its title").1;
        click_at(&ctx, &mut state, title.center());
        assert!(state.end_look.is_some());
        // Its theme deleted, another page chosen, or Settings closed: it goes.
        state.theme_ids.remove(0);
        frame(&ctx, &mut state, USUAL, vec![]).drop_without_applying_deltas();
        assert_eq!(state.end_look, None, "its theme is gone");
        let mut state = opened();
        state.preferences_pane = Pane::Keys;
        frame(&ctx, &mut state, USUAL, vec![]).drop_without_applying_deltas();
        assert_eq!(state.end_look, None, "another page");
        let mut state = opened();
        settings_closed(&mut state);
        assert_eq!(state.end_look, None);
        // Opened in the Theme Editor it is the operator's window's: the keys that run the show
        // wait for it, Escape is its own, and it goes with the editor.
        let mut state = opened();
        assert!(!over_the_operators_window(&state));
        let theme = Theme::builtin();
        state.editing =
            Some(crate::editor::EditDraft::of_slide(theme.templates[0].as_slide(crate::editor::sample_words()), 0));
        state.theme_edit = Some(crate::home::ThemeEdit::of(0, theme));
        open(&mut state, 0, Door::Editor);
        assert!(over_the_operators_window(&state) && state.escape_taken_above(&ctx));
        settings_closed(&mut state);
        assert!(state.end_look.is_some(), "Settings closing leaves the editor\u{2019}s alone");
        editor_left(&mut state);
        assert!(state.end_look.is_some(), "the editor is still there");
        // The editor left with the window open, by either of the two things that say it is
        // open: the window is over nothing at once, before anything is drawn, and then it goes.
        for leave in [
            (|state: &mut HomeState| state.editing = None) as fn(&mut HomeState),
            |state: &mut HomeState| state.theme_edit = None,
            |state: &mut HomeState| (state.editing, state.theme_edit) = (None, None),
        ] {
            let mut left = HomeState { end_look: state.end_look, ..opened() };
            left.editing.clone_from(&state.editing);
            left.theme_edit.clone_from(&state.theme_edit);
            leave(&mut left);
            assert!(left.end_look.is_some());
            assert!(!over_the_operators_window(&left) && !left.escape_taken_above(&ctx), "over nothing");
            editor_left(&mut left);
            assert_eq!(left.end_look, None, "no Theme Editor is open");
        }
        // One opened from Settings is none of the editor's business.
        let mut settings = opened();
        editor_left(&mut settings);
        assert!(settings.end_look.is_some());
    }

    #[test]
    fn end_look_nothing_in_the_window_moves_as_choices_change() {
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for size in [vec2(1440.0, 900.0), vec2(1100.0, 700.0), vec2(820.0, 700.0)] {
                    let said = format!("{language:?} at {size:?}");
                    let ctx = crate::studio::tests::styled();
                    let mut state = opened();
                    // What may change: the mark on the small slide, the percentage and the
                    // sentence. Everything else, and every block, stays where it is.
                    let mut changing: Vec<String> = EndMark::ALL.iter().map(|mark| mark.text()).collect();
                    changing.extend([sentence(true).to_owned(), sentence(false).to_owned()]);
                    // (And, on the page under it, the switch's own word.)
                    changing.extend([t("settings.on", "On").to_owned(), t("settings.off", "Off").to_owned()]);
                    let still = |words: &[(String, Rect)]| -> Vec<(String, Rect)> {
                        words
                            .iter()
                            .filter(|(have, _)| !changing.contains(have) && !have.ends_with('%'))
                            .cloned()
                            .collect()
                    };
                    let first = drawn(&ctx, &mut state, size);
                    // Everything of the window is inside its own window.
                    for (have, rect) in first.0.iter().filter(|(have, _)| !have.is_empty()) {
                        assert!(
                            rect.max.x <= size.x - 16.0 + 0.5,
                            "{said}: \u{201c}{have}\u{201d} ends at {}",
                            rect.max.x
                        );
                    }
                    let usual = EndLook::default();
                    let mut looks: Vec<(EndLook, bool)> =
                        EndMark::ALL.into_iter().map(|mark| (EndLook { mark, ..usual }, true)).collect();
                    looks.extend([
                        (EndLook { size: 40, ..usual }, true),
                        (EndLook { size: 160, ..usual }, true),
                        (EndLook { colour: Some(COLOURS[4]), ..usual }, true),
                        (EndLook { place: EndPlace::Bottom, ..usual }, true),
                        (
                            EndLook {
                                mark: EndMark::Square,
                                size: 160,
                                colour: Some(COLOURS[7]),
                                place: EndPlace::Bottom,
                            },
                            true,
                        ),
                        (usual, false),
                    ]);
                    for (look, on) in looks {
                        state.theme_end_looks[0] = look;
                        state.end_mark = on;
                        let now = drawn(&ctx, &mut state, size);
                        assert_eq!(still(&now.0), still(&first.0), "{said}: {look:?} (on: {on}) moved words");
                        assert_eq!(now.1, first.1, "{said}: {look:?} (on: {on}) moved a block");
                        // The mark is on the small slide while it is on, and only then.
                        assert_eq!(now.0.iter().any(|(have, _)| *have == look.mark.text()), on, "{said}: {look:?}");
                    }
                }
            });
        }
    }

    #[test]
    fn end_look_every_mark_has_its_glyph_in_the_faces_the_window_writes_it_in() {
        let ctx = crate::studio::tests::styled();
        for mark in EndMark::ALL {
            let chip = FontId::proportional(14.0);
            assert!(ctx.fonts_mut(|fonts| fonts.has_glyph(&chip, mark.glyph())), "{mark:?} on its chip");
            let slide = mark_font(&ctx, mark, 12.0);
            assert!(ctx.fonts_mut(|fonts| fonts.has_glyph(&slide, mark.glyph())), "{mark:?} on the small slide");
        }
    }

    #[test]
    fn end_look_given_to_a_theme_reaches_the_editors_copy_too() {
        let builtin = Theme::builtin();
        let evening = Theme { id: Uuid::from_u128(7), name: "Evening".to_owned(), ..builtin.clone() };
        let mut themes = vec![builtin.clone(), evening.clone()];
        let dots = EndLook { mark: EndMark::Dot, size: 120, colour: Some(COLOURS[4]), place: EndPlace::Bottom };
        // The theme open in the Theme Editor: its copy there takes the look as well.
        let mut edit = crate::home::ThemeEdit::of(1, evening.clone());
        assert_eq!(give(&mut themes, Some(&mut edit), evening.id, dots), Some(1));
        assert_eq!((themes[1].end_look, edit.theme.end_look), (dots, dots));
        assert_eq!(themes[0].end_look, EndLook::default(), "the other theme keeps its own");
        assert!(!edit.changed, "nothing else of the theme is touched");
        // Another theme's look leaves the editor's copy alone.
        assert_eq!(give(&mut themes, Some(&mut edit), builtin.id, dots), Some(0));
        assert_eq!(edit.theme.end_look, dots);
        // The look it has already, or a theme that is gone: nothing to do.
        assert_eq!(give(&mut themes, None, builtin.id, dots), None);
        assert_eq!(give(&mut themes, None, Uuid::from_u128(99), dots), None);
        // What was given is what the theme's file keeps.
        assert_eq!(Theme::from_json(&themes[1].to_json()).expect("read").end_look, dots);
    }
}
