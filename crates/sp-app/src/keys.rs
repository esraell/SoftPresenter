//! Keys for getting around the window, which work on every computer (the menu bar is the
//! Mac's only): Command on a Mac, Ctrl elsewhere, with a letter or a number.
//!
//! - Holding the modifier shows a small amber tag beside everything that has a key.
//! - Modifier-1 to 7 opens a tool tab and numbers what is in it; then 1 to 9 starts that timer,
//!   shows that message or prop, runs that macro, switches to that stage layout or plays that
//!   track. Shift-modifier-M numbers the pictures and videos in the media strip, and 1 to 9
//!   shows one. The numbers stay four seconds, or until Escape, which then only puts them away.
//! - `?` (and the round button in the top bar) opens the list of every key.
//!
//! The keys that run the show (Space, the arrows, Escape, the F keys, the letters) are read in
//! `main.rs` before the window draws; these are read here, in the window's frame.

use egui::{Align2, Color32, FontId, Key, Rect, Stroke, StrokeKind, pos2, vec2};

use crate::home::HomeState;
use crate::studio::{CARE, LINE, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, fixed, fixed_semibold, semibold};

/// How long the modifier is held alone before the tags show, so a Command-click that picks
/// slides doesn't flash them.
const HELD_FOR: f64 = 0.3;
/// How long the numbers after modifier-1 to 7 (or Shift-modifier-M) stay.
pub const NUMBERS_FOR: f64 = 4.0;

/// What the numbers count, while they show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumbersOf {
    /// The cards (or layouts, or tracks) of the open tool tab.
    Tab,
    /// The pictures and videos in the media strip.
    Media,
}

/// The numbers shown after modifier-1 to 7 or Shift-modifier-M: what they count, when they
/// were last asked for, and the one just picked (by its place, from 0), for the tab or the
/// strip to act on as a click on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Numbers {
    pub of: NumbersOf,
    pub since: f64,
    pub picked: Option<usize>,
}

impl Numbers {
    /// Whether the thing at `place` (from 0) was just picked by its number.
    pub fn picked(numbers: &Option<Numbers>, of: NumbersOf, place: usize) -> bool {
        numbers.is_some_and(|numbers| numbers.of == of && numbers.picked == Some(place))
    }

    /// Whether numbers are showing on things of this kind.
    pub fn showing(numbers: &Option<Numbers>, of: NumbersOf) -> bool {
        numbers.is_some_and(|numbers| numbers.of == of)
    }
}

/// The modifier in front of a key, as a tag or the list writes it.
pub fn modifier() -> &'static str {
    if cfg!(target_os = "macos") { "\u{2318}" } else { "Ctrl+" }
}

/// Shift with the modifier.
pub fn shift_modifier() -> &'static str {
    if cfg!(target_os = "macos") { "\u{21e7}\u{2318}" } else { "Ctrl+Shift+" }
}

/// A key with the modifier: `⌘S` on a Mac, `Ctrl+S` elsewhere.
pub fn with_modifier(key: &str) -> String {
    format!("{}{key}", modifier())
}

/// A key with Shift and the modifier: `⇧⌘1` on a Mac, `Ctrl+Shift+1` elsewhere.
pub fn with_shift(key: &str) -> String {
    format!("{}{key}", shift_modifier())
}

/// The key that opens tool tab `tab` (from 0) and numbers what is in it.
pub fn tab_key(tab: usize) -> String {
    with_modifier(&(tab + 1).to_string())
}

/// The keys that switch the audience and the stage screens on and off: Shift as well as the
/// modifier, so they are not pressed by chance.
pub fn audience_key() -> String {
    with_shift("1")
}

pub fn stage_key() -> String {
    with_shift("2")
}

/// The key that numbers the pictures and videos in the media strip.
pub fn media_key() -> String {
    with_shift("M")
}

/// A key said in a tooltip: "Show (Command-S)" on a Mac, "Show (Ctrl+S)" elsewhere.
pub fn hint(what: &str, key: &str, shift: bool) -> String {
    let mac = cfg!(target_os = "macos");
    match (mac, shift) {
        (true, false) => format!("{what} (Command-{key})"),
        (true, true) => format!("{what} (Shift-Command-{key})"),
        (false, false) => format!("{what} (Ctrl+{key})"),
        (false, true) => format!("{what} (Ctrl+Shift+{key})"),
    }
}

/// Whether the tags beside things with keys show now (see [`frame`]).
pub fn tags_shown(state: &HomeState) -> bool {
    state.key_tags
}

/// How wide a key's tag is.
pub fn tag_width(painter: &egui::Painter, words: &str) -> f32 {
    painter.layout_no_wrap(words.to_owned(), fixed_semibold(10.5), Color32::BLACK).size().x + 10.0
}

/// A small amber tag naming a key, its right edge at `right` and its middle at `middle`.
/// Returns where it went.
pub fn tag(painter: &egui::Painter, right: f32, middle: f32, words: &str) -> Rect {
    let galley = painter.layout_no_wrap(words.to_owned(), fixed_semibold(10.5), Color32::from_rgb(0x1a, 0x13, 0x00));
    let size = vec2(galley.size().x + 10.0, 14.0);
    let rect = Rect::from_min_size(pos2(right - size.x, middle - size.y / 2.0), size);
    painter.rect_filled(rect, 4.0, CARE);
    painter.galley(pos2(rect.min.x + 5.0, middle - galley.size().y / 2.0), galley, Color32::from_rgb(0x1a, 0x13, 0x00));
    rect
}

/// A tag at the top right corner of a control, standing half outside it so nothing under it
/// moves or is hidden.
pub fn corner_tag(painter: &egui::Painter, control: Rect, words: &str) -> Rect {
    tag(painter, control.max.x + 6.0, control.min.y + 1.0, words)
}

/// The number of a card while the numbers show, at its top left corner.
pub fn number_badge(painter: &egui::Painter, card: Rect, number: usize) {
    let centre = card.left_top() + vec2(10.0, 10.0);
    let rect = Rect::from_center_size(centre, vec2(16.0, 16.0));
    painter.rect_filled(rect, 4.0, CARE);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        number.to_string(),
        fixed_semibold(11.0),
        Color32::from_rgb(0x1a, 0x13, 0x00),
    );
}

/// Whether a key was pressed this frame with exactly these modifiers held: the modifier (⌘ on
/// a Mac, Ctrl elsewhere), Shift or not, and nothing else.
fn pressed(input: &egui::InputState, key: Key, shift: bool) -> bool {
    input.events.iter().any(|event| {
        matches!(event, egui::Event::Key { key: pressed, pressed: true, modifiers, .. }
            if *pressed == key && modifiers.command && modifiers.shift == shift && !modifiers.alt)
    })
}

/// Whether a key was pressed this frame with no modifier.
fn plain(input: &egui::InputState, key: Key) -> bool {
    input.events.iter().any(|event| {
        matches!(event, egui::Event::Key { key: pressed, pressed: true, modifiers, .. }
            if *pressed == key && !modifiers.command && !modifiers.ctrl && !modifiers.alt)
    })
}

/// `?` typed (it takes Shift on most keyboards).
fn question_mark(input: &egui::InputState) -> bool {
    input.events.iter().any(|event| match event {
        egui::Event::Text(text) => text == "?",
        egui::Event::Key { key: Key::Questionmark, pressed: true, modifiers, .. } => !modifiers.command,
        _ => false,
    })
}

/// What a key asks for that the app (not the window) must do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The audience screens on or off.
    Audience,
    /// The menu of the slide on screen.
    LiveMenu,
}

/// Reads the keys for getting around, once a frame, before the window is drawn: shows or hides
/// the tags, ends the numbers when their time is up, and does what a key asks. Nothing is read
/// while text is typed, or while a question, a sheet or the setup is open.
pub fn frame(ctx: &egui::Context, state: &mut HomeState, voxlive: bool) -> Option<Ask> {
    let now = ctx.input(|input| input.time);
    // The tags: while the modifier is held alone, after a moment, and not while a mouse button
    // is down (a Command-click picks slides).
    let (command, shift, alt, down, keyed) = ctx.input(|input| {
        let keyed = input.events.iter().any(|event| matches!(event, egui::Event::Key { pressed: true, .. }));
        (input.modifiers.command, input.modifiers.shift, input.modifiers.alt, input.pointer.any_down(), keyed)
    });
    if command && !shift && !alt && !down && !keyed {
        let since = *state.modifier_since.get_or_insert(now);
        state.key_tags = now - since >= HELD_FOR;
        if !state.key_tags {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(HELD_FOR - (now - since) + 0.01));
        }
    } else if !command || down {
        state.modifier_since = None;
        state.key_tags = false;
    }
    // Testing aid: SOFTPRESENTER_CAPTURE_KEYS=tags shows the tags as if the modifier were held;
    // `numbers` numbers the open tab's things and `media` the media strip's, as the keys do;
    // `sheet` opens the list of every key. Several may be given, with commas between. For
    // pictures of them.
    static TEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let test = TEST.get_or_init(|| std::env::var("SOFTPRESENTER_CAPTURE_KEYS").unwrap_or_default());
    for part in test.split(',') {
        match part {
            "tags" => state.key_tags = true,
            "numbers" => state.numbers = Some(Numbers { of: NumbersOf::Tab, since: now, picked: None }),
            "media" => state.numbers = Some(Numbers { of: NumbersOf::Media, since: now, picked: None }),
            "sheet" => state.key_sheet = true,
            _ => {}
        }
    }
    // The numbers go after their time, or when the view they are in has gone.
    if let Some(numbers) = state.numbers.as_mut() {
        numbers.picked = None;
        let left = numbers.since + NUMBERS_FOR - now;
        if left <= 0.0 || state.edit_kind().is_some() {
            state.numbers = None;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
        }
    }
    // The list of keys is over everything: Escape, Enter or ? put it away.
    if state.key_sheet {
        if ctx.input(|input| plain(input, Key::Escape) || plain(input, Key::Enter) || question_mark(input)) {
            state.key_sheet = false;
        }
        return None;
    }
    if ctx.text_edit_focused()
        || state.new_song_sheet.is_some()
        || state.alert_open()
        || state.setup.is_some()
        || state.library_search.is_some()
        || state.settings_open
    {
        return None;
    }
    let menu_open = egui::Popup::is_any_open(ctx);
    // A number while the numbers show: that thing, as a click on it does. The numbers stay for
    // another four seconds, for the next.
    if let Some(numbers) = state.numbers.as_mut() {
        let digit = ctx.input(|input| {
            [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9]
                .iter()
                .position(|key| plain(input, *key))
        });
        if let Some(place) = digit {
            numbers.picked = Some(place);
            numbers.since = now;
            ctx.request_repaint();
        }
        if ctx.input(|input| plain(input, Key::Escape)) {
            state.numbers = None;
        }
    }
    if menu_open {
        return None;
    }
    let editing = state.edit_kind().is_some();
    let mut ask = None;
    ctx.input_mut(|input| {
        if question_mark(input) {
            state.key_sheet = true;
        }
        // The views.
        if pressed(input, Key::S, false) {
            state.view_request = Some(0);
        }
        if pressed(input, Key::E, false) {
            state.view_request = Some(1);
        }
        // Command-B is Bold in the editor; elsewhere it is the Bible page, ready for a reference.
        if !editing && pressed(input, Key::B, false) {
            state.view_request = Some(2);
            state.bible.focus_lookup = true;
        }
        // The screens on and off, from anywhere.
        if pressed(input, Key::Num1, true) {
            ask = Some(Ask::Audience);
        }
        if pressed(input, Key::Num2, true) {
            state.stage_on = !state.stage_on;
        }
        if editing {
            return;
        }
        // The playlists and libraries to choose from, in the Show view.
        if pressed(input, Key::P, false) {
            state.view_request = Some(0);
            state.chooser_open = true;
        }
        // A tool tab, numbered.
        let tabs = if voxlive { 7 } else { 6 };
        let numbered = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7]
            .iter()
            .position(|key| pressed(input, *key, false));
        if let Some(tab) = numbered.filter(|tab| *tab < tabs) {
            state.live_tab = tab;
            state.numbers = Some(Numbers { of: NumbersOf::Tab, since: now, picked: None });
        }
        if pressed(input, Key::M, true) {
            state.numbers = Some(Numbers { of: NumbersOf::Media, since: now, picked: None });
            state.media_hidden = false;
        }
        // The menu of the slide on screen.
        if pressed(input, Key::O, false) {
            ask = Some(Ask::LiveMenu);
        }
    });
    ask
}

/// One row of the list of keys: the keys, and what they do.
type Row = (Vec<String>, &'static str);

/// The list of every key, in groups, as the window shows it.
pub fn every_key() -> Vec<(&'static str, Vec<Row>)> {
    let m = with_modifier;
    let one = |key: &str| vec![key.to_owned()];
    vec![
        (
            "Running the slides",
            vec![
                (vec!["Space".to_owned(), "\u{2192}".to_owned()], "Next slide"),
                (one("\u{2190}"), "Previous slide"),
                (
                    vec!["\u{2191}".to_owned(), "\u{2193}".to_owned()],
                    "The item above or below; in Bible, the verse before or after",
                ),
                (one("Enter"), "The open item goes on screen, from its first slide"),
                (vec!["Home".to_owned(), "End".to_owned()], "First or last slide of the song"),
                (one("A letter"), "Jump to a group (C for the chorus), or a slide\u{2019}s own hot key"),
            ],
        ),
        (
            "Clearing",
            vec![
                (vec!["Esc".to_owned(), "F1".to_owned()], "Clear everything"),
                (
                    vec!["F2".to_owned(), "F3".to_owned(), "F4".to_owned(), "F5".to_owned(), "F6".to_owned()],
                    "Clear the slide, media, props, audio, message",
                ),
            ],
        ),
        (
            "Going places",
            vec![
                (vec![m("S"), m("B"), m("E")], "Show, Bible (ready for a reference), Edit"),
                (one(&m(",")), "Settings"),
                (one(&m("P")), "Playlists and libraries"),
                (vec!["/".to_owned(), m("F")], "Find a song, a line or a verse"),
                (one("?"), "This list"),
            ],
        ),
        (
            "The live tools",
            vec![
                (
                    vec![format!("{} \u{2026} {}", m("1"), m("7"))],
                    "Open Timers, Messages, Props, Macros, Stage, Audio, VoxLive",
                ),
                (
                    one("then 1 \u{2026} 9"),
                    "Start that timer, show that message or prop, run that macro, switch the stage to that layout, play that track",
                ),
                (vec![media_key(), "then 1 \u{2026} 9".to_owned()], "Show that picture or video"),
                (vec![audience_key(), stage_key()], "Audience screens, stage screens, on or off"),
                (one(&m("O")), "The menu for the slide on screen"),
            ],
        ),
        (
            "Menus and windows",
            vec![
                (vec!["\u{2191}".to_owned(), "\u{2193}".to_owned()], "Move"),
                (vec!["\u{2192}".to_owned(), "\u{2190}".to_owned()], "Open a group of choices; go back"),
                (one("Enter"), "Choose"),
                (one("Esc"), "Close"),
            ],
        ),
        (
            "Changing things",
            vec![
                (vec![m("Z"), with_shift("Z")], "Undo, redo"),
                (vec![m("C"), m("X"), m("V")], "Copy, cut, paste the picked slides (objects in Edit)"),
                (one("Delete"), "Delete what is picked, after asking"),
                (one(&m("N")), "A new song"),
                (vec!["T".to_owned(), m("D"), m("A")], "In Edit: a new text box, duplicate, pick everything"),
                (one("Arrows"), "In Edit: nudge the picked object (Shift for one pixel)"),
            ],
        ),
    ]
}

/// The list of every key, as a sheet over the dimmed window: groups in two columns, a note at
/// the foot, and Done. `?`, Escape, Enter, Done or the × put it away.
pub fn key_sheet(ctx: &egui::Context, state: &mut HomeState) {
    if !state.key_sheet {
        return;
    }
    let screen = ctx.content_rect();
    let width = 720.0_f32.min(screen.width() - 32.0);
    let height = 868.0_f32.min(screen.height() - 32.0);
    let sheet = Rect::from_center_size(screen.center(), vec2(width, height));
    let area = egui::Area::new(egui::Id::unique("key sheet"))
        .order(egui::Order::Foreground)
        .fixed_pos(sheet.min)
        .constrain(false);
    area.show(ctx, |ui| {
        // The window under it is dimmed, in the sheet's own layer so the sheet stays bright.
        ui.painter().with_clip_rect(screen).rect_filled(screen, 0.0, Color32::from_black_alpha(153));
        let painter = ui.painter().clone();
        painter.rect_filled(sheet.translate(vec2(0.0, 30.0)).expand(30.0), 48.0, Color32::from_black_alpha(40));
        painter.rect_filled(sheet, 18.0, Color32::from_rgb(0x1f, 0x1f, 0x1f));
        painter.rect_stroke(sheet, 18.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        // The whole sheet takes the pointer, so nothing under it is clicked.
        ui.allocate_rect(sheet, egui::Sense::click());
        painter.text(pos2(sheet.min.x + 18.0, sheet.min.y + 29.0), Align2::LEFT_CENTER, "Keys", semibold(15.0), TEXT);
        let close = Rect::from_min_size(pos2(sheet.max.x - 12.0 - 30.0, sheet.min.y + 14.0), vec2(30.0, 30.0));
        let close_response = ui.interact(close, ui.scope_id().with("close keys"), egui::Sense::click());
        if close_response.hovered() {
            painter.rect_filled(close, 7.0, crate::studio::CARD);
        }
        crate::icons::paint(
            &painter,
            crate::icons::Icon::Close,
            Rect::from_center_size(close.center(), vec2(14.0, 14.0)),
            TEXT_DIM,
        );
        if close_response.on_hover_text("Close (Esc)").clicked() {
            state.key_sheet = false;
        }
        // Two columns of groups, in a body that scrolls when the window is too short for all.
        let column = (width - 36.0 - 28.0) / 2.0;
        let keys_width = (column * 150.0 / 323.0).round();
        let body = Rect::from_min_max(pos2(sheet.min.x, sheet.min.y + 68.0), pos2(sheet.max.x, sheet.max.y - 59.0));
        let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical().id_salt("key sheet body").auto_shrink([false, false]).show(&mut inside, |ui| {
            let origin = ui.max_rect().min;
            let painter = ui.painter().clone();
            let mut tops = [origin.y + 4.0; 2];
            for (place, (group, rows)) in every_key().into_iter().enumerate() {
                let side = place % 2;
                // The groups stand in rows of two, each row starting under the taller of the last.
                if side == 0 {
                    let row = tops[0].max(tops[1]);
                    tops = [row; 2];
                }
                let left = origin.x + 18.0 + side as f32 * (column + 28.0);
                let mut y = tops[side];
                let heading = &group.to_uppercase();
                let font = fixed_semibold(11.5);
                crate::studio::paint_spaced(
                    ui,
                    pos2(left, y + 7.75),
                    Align2::LEFT_CENTER,
                    heading,
                    font,
                    TEXT_DIM,
                    0.6,
                );
                y += 21.5;
                for (keys, what) in rows {
                    // The keys, as caps, going on to another line when they don't fit.
                    let (mut x, mut line) = (left, y);
                    for key in &keys {
                        let galley = painter.layout_no_wrap(key.clone(), fixed_semibold(11.0), TEXT);
                        let wide = galley.size().x + 14.0;
                        if x > left && x + wide > left + keys_width {
                            x = left;
                            line += 28.0;
                        }
                        let cap = Rect::from_min_size(pos2(x, line), vec2(wide, 24.0));
                        painter.rect_filled(cap, 6.0, SELECTED);
                        painter.galley(cap.min + vec2(7.0, 12.0 - galley.size().y / 2.0), galley, TEXT);
                        x = cap.max.x + 4.0;
                    }
                    let key_bottom = line + 24.0;
                    let room = column - keys_width - 10.0;
                    let words = painter.layout(what.to_owned(), FontId::proportional(13.0), TEXT, room);
                    let words_height = words.size().y;
                    painter.galley(pos2(left + keys_width + 10.0, y + 3.0), words, TEXT);
                    y = key_bottom.max(y + 3.0 + words_height) + 6.0;
                }
                tops[side] = y + 10.0;
            }
            let bottom = tops[0].max(tops[1]);
            ui.allocate_rect(Rect::from_min_max(origin, pos2(origin.x + width, bottom)), egui::Sense::hover());
        });
        // The foot: what holding the modifier does, and Done.
        let foot = sheet.max.y - 59.0;
        let note = format!(
            "Hold {} to see the keys beside the things they do. Settings \u{203a} Key Mappings has the menu bar's too.",
            if cfg!(target_os = "macos") { "Command" } else { "Ctrl" }
        );
        let note = painter.layout(note.to_uppercase(), fixed(11.5), TEXT_FAINT, width - 36.0 - 90.0);
        painter.galley(pos2(sheet.min.x + 18.0, foot + 12.0), note, TEXT_FAINT);
        let done = Rect::from_min_size(pos2(sheet.max.x - 18.0 - 57.3, foot + 12.5), vec2(57.3, 30.0));
        let done_response = ui.interact(done, ui.scope_id().with("done keys"), egui::Sense::click());
        painter.rect_filled(done, 9.0, if done_response.hovered() { Color32::WHITE } else { TEXT });
        painter.text(
            done.center(),
            Align2::CENTER_CENTER,
            "Done",
            FontId::proportional(12.5),
            Color32::from_rgb(0x19, 0x19, 0x19),
        );
        if done_response.clicked() {
            state.key_sheet = false;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(
        ctx: &egui::Context,
        state: &mut HomeState,
        time: f64,
        key: Key,
        modifiers: egui::Modifiers,
    ) -> Option<Ask> {
        let input = egui::RawInput {
            time: Some(time),
            events: vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers },
            ],
            ..egui::RawInput::default()
        };
        let mut asked = None;
        ctx.run_ui(input, |ui| asked = frame(ui.ctx(), state, false)).drop_without_applying_deltas();
        asked
    }

    fn idle(ctx: &egui::Context, state: &mut HomeState, time: f64) {
        let input = egui::RawInput { time: Some(time), ..egui::RawInput::default() };
        ctx.run_ui(input, |ui| {
            frame(ui.ctx(), state, false);
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn modifier_and_a_number_opens_that_tab_and_numbers_what_is_in_it() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        press(&ctx, &mut state, 1.0, Key::Num4, egui::Modifiers::COMMAND);
        assert_eq!(state.live_tab, 3, "Macros");
        assert!(Numbers::showing(&state.numbers, NumbersOf::Tab));
        // Then 2 picks the second macro, for that frame only, and the numbers stay.
        press(&ctx, &mut state, 2.0, Key::Num2, egui::Modifiers::NONE);
        assert!(Numbers::picked(&state.numbers, NumbersOf::Tab, 1));
        idle(&ctx, &mut state, 2.1);
        assert!(!Numbers::picked(&state.numbers, NumbersOf::Tab, 1), "a pick is for one frame");
        assert!(Numbers::showing(&state.numbers, NumbersOf::Tab));
    }

    #[test]
    fn the_numbers_go_after_four_seconds_and_a_pick_keeps_them_four_more() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        press(&ctx, &mut state, 10.0, Key::Num1, egui::Modifiers::COMMAND);
        idle(&ctx, &mut state, 13.9);
        assert!(state.numbers.is_some(), "still there just before four seconds");
        press(&ctx, &mut state, 13.95, Key::Num1, egui::Modifiers::NONE);
        idle(&ctx, &mut state, 17.0);
        assert!(state.numbers.is_some(), "a pick starts the four seconds again");
        idle(&ctx, &mut state, 18.0);
        assert!(state.numbers.is_none(), "gone four seconds after the last pick");
    }

    #[test]
    fn escape_only_puts_the_numbers_away() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        press(&ctx, &mut state, 1.0, Key::M, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert!(Numbers::showing(&state.numbers, NumbersOf::Media));
        let asked = press(&ctx, &mut state, 1.5, Key::Escape, egui::Modifiers::NONE);
        assert!(state.numbers.is_none());
        assert_eq!(asked, None);
    }

    #[test]
    fn getting_around_keys_go_to_the_views_the_chooser_and_the_screens() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        press(&ctx, &mut state, 1.0, Key::B, egui::Modifiers::COMMAND);
        assert_eq!(state.view_request, Some(2));
        assert!(state.bible.focus_lookup, "ready for a reference");
        press(&ctx, &mut state, 2.0, Key::P, egui::Modifiers::COMMAND);
        assert!(state.chooser_open && state.view_request == Some(0));
        let stage = state.stage_on;
        press(&ctx, &mut state, 3.0, Key::Num2, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert_eq!(state.stage_on, !stage);
        let asked = press(&ctx, &mut state, 4.0, Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert_eq!(asked, Some(Ask::Audience));
        assert_eq!(state.live_tab, 0, "Shift-modifier-1 is the audience screens, not a tab");
    }

    #[test]
    fn question_mark_opens_the_list_and_escape_closes_it() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        let input =
            egui::RawInput { time: Some(1.0), events: vec![egui::Event::Text("?".to_owned())], ..Default::default() };
        ctx.run_ui(input, |ui| {
            frame(ui.ctx(), &mut state, false);
        })
        .drop_without_applying_deltas();
        assert!(state.key_sheet);
        press(&ctx, &mut state, 2.0, Key::Escape, egui::Modifiers::NONE);
        assert!(!state.key_sheet);
    }

    #[test]
    fn the_tags_show_once_the_modifier_has_been_held_a_moment_and_not_during_a_click() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        let run = |state: &mut HomeState, time: f64, events: Vec<egui::Event>| {
            let input = egui::RawInput { time: Some(time), events, ..Default::default() };
            ctx.run_ui(input, |ui| {
                frame(ui.ctx(), state, false);
            })
            .drop_without_applying_deltas();
        };
        run(&mut state, 1.0, vec![egui::Event::ModifiersChanged(egui::Modifiers::COMMAND)]);
        assert!(!state.key_tags, "not at once");
        run(&mut state, 1.4, Vec::new());
        assert!(state.key_tags, "after a moment");
        let click = egui::Event::PointerButton {
            pos: pos2(10.0, 10.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::COMMAND,
        };
        run(&mut state, 1.5, vec![click]);
        assert!(!state.key_tags, "a Command-click hides them");
    }
}
