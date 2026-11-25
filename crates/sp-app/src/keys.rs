//! Keys for getting around the window, which work on every computer (the menu bar is the
//! Mac's only): Command on a Mac, Ctrl elsewhere, with a letter or a number.
//!
//! - With Show Keys on Buttons on (in the list of keys, and in Settings › Key Mappings), a small amber tag
//!   beside everything that has a key names it, all the time; off (as it starts), never.
//! - Command-1, 2 and 3 are Show, Bible and Edit. Option-1 to 7 (Alt elsewhere) opens a tool
//!   tab and numbers what is in it; then 1 to 9 starts that timer, shows that message or prop,
//!   runs that macro, switches to that stage layout or plays that track. Shift-modifier-M
//!   numbers the pictures and videos in the media strip, and 1 to 9 shows one. The numbers stay
//!   four seconds, or until Escape, which then only puts them away.
//! - `?` (and the round button in the top bar) opens the list of every key.
//!
//! The keys that run the show (Space, the arrows, Escape, the F keys off a Mac, the letters) are
//! read in `main.rs` before the window draws; these are read here, in the window's frame.

use egui::{Align2, Color32, FontId, Key, Rect, Stroke, StrokeKind, pos2, vec2};

use crate::home::HomeState;
use crate::i18n::t;
use crate::studio::{CARE, LINE, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, fixed, fixed_semibold, semibold};

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
pub fn shift_modifier() -> String {
    if cfg!(target_os = "macos") { "\u{21e7}\u{2318}".to_owned() } else { format!("Ctrl+{}+", shift_word()) }
}

/// The word for the Shift key off a Mac, as the language's keyboards print it (Mayús in Spanish).
fn shift_word() -> &'static str {
    t("keys.cap.shift", "Shift")
}

/// A key with the modifier: `⌘S` on a Mac, `Ctrl+S` elsewhere.
pub fn with_modifier(key: &str) -> String {
    format!("{}{key}", modifier())
}

/// A key with Shift and the modifier: `⇧⌘1` on a Mac, `Ctrl+Shift+1` elsewhere.
pub fn with_shift(key: &str) -> String {
    format!("{}{key}", shift_modifier())
}

/// Whether the F keys clear the screens (F1 everything, F2 to F6 one layer each): off a Mac
/// only. A Mac's F keys are the system's (brightness, volume, Mission Control), so there Escape
/// clears everything and the layer keys in the right column clear one layer each.
pub fn f_keys() -> bool {
    !cfg!(target_os = "macos")
}

/// A layer key's tooltip, with its F key after it off a Mac: "… (F2)".
pub fn clear_hint(what: &str, f_key: &str) -> String {
    clear_hint_on(what, f_key, f_keys())
}

fn clear_hint_on(what: &str, f_key: &str, f_keys: bool) -> String {
    if f_keys { format!("{what} ({f_key})") } else { what.to_owned() }
}

/// What the list of keys says in place of a key for something only a button does.
#[cfg(test)]
pub const BUTTON_ONLY: &str = "Button only";

/// The modifier the tool tabs' keys use: Option on a Mac, Alt elsewhere.
pub fn tab_modifier() -> &'static str {
    if cfg!(target_os = "macos") { "\u{2325}" } else { "Alt+" }
}

/// The key that opens tool tab `tab` (from 0) and numbers what is in it: `⌥1` on a Mac,
/// `Alt+1` elsewhere. (Command-1, 2 and 3 are Show, Bible and Edit.)
pub fn tab_key(tab: usize) -> String {
    format!("{}{}", tab_modifier(), tab + 1)
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

/// A key with the modifier, written into a sentence: "⌘" and the key on a Mac, "Ctrl+" and
/// the key elsewhere.
pub fn hint_key(key: &str) -> String {
    with_modifier(key)
}

/// Every key the window writes in words goes through here: the Mac's symbols in the Mac's order
/// (⌃ ⌥ ⇧ ⌘, then the key), or the words Windows and Linux use ("Ctrl+Alt+Shift+").
fn chord_on(mac: bool, control: bool, option: bool, shift: bool, command: bool, key: &str) -> String {
    let mut chord = String::new();
    if mac {
        for (on, mark) in [(control, "\u{2303}"), (option, "\u{2325}"), (shift, "\u{21e7}"), (command, "\u{2318}")] {
            if on {
                chord.push_str(mark);
            }
        }
    } else {
        // Off a Mac the command key is Ctrl, so Control and Command are one key there.
        for (on, word) in [(control || command, "Ctrl"), (option, "Alt"), (shift, shift_word())] {
            if on {
                chord.push_str(word);
                chord.push('+');
            }
        }
    }
    chord.push_str(key);
    chord
}

/// A key said in a tooltip: "Show (⌘1)" on a Mac, "Show (Ctrl+1)" elsewhere.
pub fn hint(what: &str, key: &str, shift: bool) -> String {
    format!("{what} ({})", chord_on(cfg!(target_os = "macos"), false, false, shift, true, key))
}

/// A tool tab's key said in a tooltip: "Timers (℥1)" on a Mac, "Timers (Alt+1)" elsewhere.
pub fn tab_hint(what: &str, tab: usize) -> String {
    format!("{what} ({})", chord_on(cfg!(target_os = "macos"), false, true, false, false, &(tab + 1).to_string()))
}

/// The Mac menu bar's keys for the Edit Words and Configure Screens buttons, which have none
/// elsewhere: ⌃R and ⌥⌘1.
pub fn words_key() -> Option<String> {
    cfg!(target_os = "macos").then(|| chord_on(true, true, false, false, false, "R"))
}

pub fn screens_key() -> Option<String> {
    cfg!(target_os = "macos").then(|| chord_on(true, false, true, false, true, "1"))
}

/// Whether the tags beside things with keys show: Show Keys on Buttons is on (or a testing aid
/// shows them for a picture).
pub fn tags_shown(state: &HomeState) -> bool {
    state.keys_on_buttons || state.key_tags_for_capture
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

/// Whether a key was pressed this frame with Option (Alt elsewhere) alone. A Mac's Option and a
/// number types a sign (¡, ™, £…), so the key is matched by where it is on the keyboard too.
fn pressed_tab(input: &egui::InputState, key: Key) -> bool {
    input.events.iter().any(|event| {
        matches!(event, egui::Event::Key { key: pressed, physical_key, pressed: true, modifiers, .. }
            if (*pressed == key || *physical_key == Some(key))
                && modifiers.alt && !modifiers.command && !modifiers.ctrl && !modifiers.shift)
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

/// Reads the keys for getting around, once a frame, before the window is drawn: ends the
/// numbers when their time is up, and does what a key asks. Nothing is read while text is
/// typed, or while a question, a sheet or the setup is open. (Holding a modifier no longer shows
/// the keys' tags: Show Keys on Buttons does, all the time.)
pub fn frame(ctx: &egui::Context, state: &mut HomeState, voxlive: bool) -> Option<Ask> {
    let now = ctx.input(|input| input.time);
    // Testing aid: SOFTPRESENTER_CAPTURE_KEYS=tags shows the tags as Show Keys on Buttons does,
    // without changing the setting; `numbers` numbers the open tab's things and `media` the
    // media strip's, as the keys do; `sheet` opens the list of every key. Several may be given,
    // with commas between. For pictures of them.
    static TEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let test = TEST.get_or_init(|| std::env::var("SOFTPRESENTER_CAPTURE_KEYS").unwrap_or_default());
    for part in test.split(',') {
        match part {
            "tags" => state.key_tags_for_capture = true,
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
        || !state.bible.options.details.is_empty()
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
        // The views: Command-1, 2 and 3 are Show, Bible (ready for a reference) and Edit, from
        // anywhere (on a Mac the menu bar's View menu takes them first, to the same effect).
        if pressed(input, Key::Num1, false) {
            state.view_request = Some(0);
        }
        if pressed(input, Key::Num2, false) {
            state.view_request = Some(2);
            state.bible.focus_lookup = true;
        }
        if pressed(input, Key::Num3, false) {
            state.view_request = Some(1);
        }
        // Command-B is the Bible page too, for those used to it; in the editor it is Bold.
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
        // The playlists and libraries to choose from, in the Service view.
        if pressed(input, Key::P, false) {
            state.view_request = Some(0);
            state.chooser_open = true;
        }
        // A tool tab, numbered: Option-1 to 7 (Alt elsewhere).
        let tabs = if voxlive { 7 } else { 6 };
        let numbered = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7]
            .iter()
            .position(|key| pressed_tab(input, *key));
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
type Row = (Vec<String>, String);

/// The list of every key, in groups, as the window shows it: the list of keys (`?`) and
/// Settings › Key Mappings are both made from it.
pub fn every_key() -> Vec<(&'static str, Vec<Row>)> {
    every_key_on(f_keys())
}

/// The list of every key, with the F keys (off a Mac) or without them (on a Mac, where clearing
/// one layer is the layer keys' alone).
fn every_key_on(f_keys: bool) -> Vec<(&'static str, Vec<Row>)> {
    let m = with_modifier;
    let one = |key: &str| vec![key.to_owned()];
    let keys = |keys: &[&str]| keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>();
    let row = |keys: Vec<String>, what: &str| (keys, what.to_owned());
    vec![
        (
            "Running the slides",
            vec![
                row(keys(&[t("keys.cap.space", "Space"), "\u{2192}"]), t("keys.running.1", "Next slide")),
                row(one("\u{2190}"), t("keys.running.2", "Previous slide")),
                row(
                    keys(&["\u{2191}", "\u{2193}"]),
                    t("keys.running.3", "The item above or below; in Bible, pick the verse before or after"),
                ),
                row(
                    one(t("keys.cap.enter", "Enter")),
                    t(
                        "keys.running.4",
                        "The slide picked goes on screen (found with / or looked up, or picked in Bible); else the open item, from its first slide",
                    ),
                ),
                row(
                    keys(&[t("keys.cap.home", "Home"), t("keys.cap.end", "End")]),
                    t("keys.running.5", "First or last slide of the song"),
                ),
                row(
                    one(t("keys.cap.letter", "A letter")),
                    t(
                        "keys.running.6",
                        "Jump to a group (C for the chorus), or a slide\u{2019}s own hot key; in Bible, start the next reference",
                    ),
                ),
            ],
        ),
        (
            "Clearing",
            if f_keys {
                vec![
                    row(keys(&["Esc", "F1"]), t("keys.clearing.1", "Clear everything")),
                    row(
                        keys(&["F2", "F3", "F4", "F5", "F6"]),
                        t("keys.clearing.2", "Clear the slide, media, props, audio, message"),
                    ),
                ]
            } else {
                vec![
                    row(one("Esc"), t("keys.clearing.3", "Clear everything")),
                    row(
                        one(t("keys.cap.button_only", "Button only")),
                        t(
                            "keys.clearing.4",
                            "Clear the slide, media, props, messages, audio or announcements: the layer keys under the preview",
                        ),
                    ),
                ]
            },
        ),
        (
            "Going places",
            vec![
                row(vec![m("1"), m("2"), m("3")], t("keys.places.1", "Show, Bible (ready for a reference), Edit")),
                row(one(&m("B")), t("keys.places.2", "Bible, too (Bold while editing a slide)")),
                row(one(&m(",")), t("keys.places.3", "Settings")),
                row(one(&m("P")), t("keys.places.4", "Playlists and libraries")),
                row(vec!["/".to_owned(), m("F")], t("keys.places.5", "Find a song, a line or a verse")),
                row(one("?"), t("keys.places.6", "This list (its foot has Show keys on buttons)")),
            ],
        ),
        (
            "The live tools",
            vec![
                row(
                    vec![format!("{} \u{2026} {}", tab_key(0), tab_key(6))],
                    t("keys.live.1", "Open Timers, Messages, Props, Macros, Stage, Audio, VoxLive"),
                ),
                row(
                    one(t("keys.cap.then", "then 1 \u{2026} 9")),
                    t(
                        "keys.live.2",
                        "Start that timer, show that message or prop, run that macro, switch the stage to that layout, play that track",
                    ),
                ),
                row(
                    vec![media_key(), t("keys.cap.then", "then 1 \u{2026} 9").to_owned()],
                    t("keys.live.3", "Show that picture or video"),
                ),
                row(vec![audience_key(), stage_key()], t("keys.live.4", "Audience screens, stage screens, on or off")),
                row(one(&m("O")), t("keys.live.5", "The menu for the slide on screen")),
            ],
        ),
        (
            "Menus and windows",
            vec![
                row(keys(&["\u{2191}", "\u{2193}"]), t("keys.menus.1", "Move")),
                row(keys(&["\u{2192}", "\u{2190}"]), t("keys.menus.2", "Open a group of choices; go back")),
                row(one(t("keys.cap.enter", "Enter")), t("keys.menus.3", "Choose")),
                row(one("Esc"), t("keys.menus.4", "Close")),
            ],
        ),
        (
            "Changing things",
            vec![
                row(vec![m("Z"), with_shift("Z")], t("keys.changing.1", "Undo, redo")),
                row(
                    vec![m("C"), m("X"), m("V")],
                    t("keys.changing.2", "Copy, cut, paste the picked slides (objects in Edit)"),
                ),
                row(
                    one(t("keys.cap.delete", "Delete")),
                    t(
                        "keys.changing.3",
                        "Delete what is picked: what Undo brings back goes at once (playlist items, slides, a media playlist's items); songs in a library, pictures in the bin and screens ask first",
                    ),
                ),
                row(one(&m("N")), t("keys.changing.4", "A new song")),
                row(
                    vec!["T".to_owned(), m("D"), m("A")],
                    t("keys.changing.5", "In Edit: a new text box, duplicate, pick everything"),
                ),
                row(
                    one(t("keys.cap.arrows", "Arrows")),
                    t("keys.changing.6", "In Edit: nudge the picked object (Shift for one pixel)"),
                ),
            ],
        ),
    ]
}

/// A group of the list of keys, in the language that is set (the list itself keeps English
/// names for them, which the tests and the code look groups up by).
pub fn group_title(group: &str) -> &'static str {
    match group {
        "Running the slides" => t("keys.group.running", "Running the slides"),
        "Clearing" => t("keys.group.clearing", "Clearing"),
        "Going places" => t("keys.group.places", "Going places"),
        "The live tools" => t("keys.group.live", "The live tools"),
        "Menus and windows" => t("keys.group.menus", "Menus and windows"),
        "Changing things" => t("keys.group.changing", "Changing things"),
        _ => "",
    }
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
        .fixed_pos(screen.min)
        .constrain(false);
    area.show(ctx, |ui| {
        ui.set_clip_rect(screen);
        // The whole window under it takes the pointer, so nothing there is clicked; a click
        // outside the sheet puts it away.
        let outside = ui.allocate_rect(screen, egui::Sense::click());
        if outside.clicked() && !outside.interact_pointer_pos().is_some_and(|at| sheet.contains(at)) {
            state.key_sheet = false;
        }
        // The window under it is dimmed, in the sheet's own layer so the sheet stays bright.
        ui.painter().with_clip_rect(screen).rect_filled(screen, 0.0, Color32::from_black_alpha(153));
        let painter = ui.painter().clone();
        painter.rect_filled(sheet.translate(vec2(0.0, 30.0)).expand(30.0), 48.0, Color32::from_black_alpha(40));
        painter.rect_filled(sheet, 18.0, Color32::from_rgb(0x1f, 0x1f, 0x1f));
        painter.rect_stroke(sheet, 18.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        painter.text(
            pos2(sheet.min.x + 18.0, sheet.min.y + 29.0),
            Align2::LEFT_CENTER,
            t("keys.title", "Keys"),
            semibold(15.0),
            TEXT,
        );
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
        if close_response.on_hover_text(t("keys.close", "Close (Esc)")).clicked() {
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
                let heading = &group_title(group).to_uppercase();
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
                    let words = painter.layout(what, FontId::proportional(13.0), TEXT, room);
                    let words_height = words.size().y;
                    painter.galley(pos2(left + keys_width + 10.0, y + 3.0), words, TEXT);
                    y = key_bottom.max(y + 3.0 + words_height) + 6.0;
                }
                tops[side] = y + 10.0;
            }
            let bottom = tops[0].max(tops[1]);
            ui.allocate_rect(Rect::from_min_max(origin, pos2(origin.x + width, bottom)), egui::Sense::hover());
        });
        // The foot: the switch that writes each button's key beside it (also in Settings ›
        // Key Mappings), a note, and Done.
        let foot = sheet.max.y - 59.0;
        // Done is as wide as its word in the language that is set.
        let done_wide =
            painter.layout_no_wrap(t("keys.done", "Done").to_owned(), FontId::proportional(12.5), TEXT).size().x + 30.0;
        let label = painter.layout_no_wrap(
            t("keys.show_on_buttons", "Show keys on buttons").to_owned(),
            FontId::proportional(13.0),
            TEXT,
        );
        let track = Rect::from_min_size(pos2(sheet.min.x + 18.0, foot + 19.5), vec2(30.0, 17.0));
        let switch = Rect::from_min_max(
            track.min - vec2(4.0, 6.0),
            pos2(track.max.x + 10.0 + label.size().x + 4.0, track.max.y + 6.0),
        );
        let switch_response = ui.interact(switch, ui.scope_id().with("keys on buttons"), egui::Sense::click());
        if switch_response.clicked() {
            state.keys_on_buttons = !state.keys_on_buttons;
        }
        let on = state.keys_on_buttons;
        painter.rect_filled(track, 8.5, if on { SELECTED } else { crate::studio::BUTTON });
        if on {
            painter.rect_stroke(track, 8.5, Stroke::new(1.0, Color32::from_white_alpha(36)), StrokeKind::Inside);
        }
        let knob = if on { track.right_center() - vec2(8.5, 0.0) } else { track.left_center() + vec2(8.5, 0.0) };
        painter.circle_filled(knob, 6.0, if on { TEXT } else { TEXT_DIM });
        painter.galley(pos2(track.max.x + 10.0, track.center().y - label.size().y / 2.0), label, TEXT);
        let where_else = t("keys.also_in", "Also in Settings \u{203a} Keys.");
        let left = switch.max.x + 14.0;
        let note = painter.layout(
            where_else.to_uppercase(),
            fixed(11.0),
            TEXT_FAINT,
            (sheet.max.x - 18.0 - done_wide - 12.0 - left).max(60.0),
        );
        painter.galley(pos2(left, track.center().y - note.size().y / 2.0), note, TEXT_FAINT);
        switch_response
            .on_hover_text(t("keys.show_on_buttons.hint", "Write each button's key beside it, all the time"));
        let done = Rect::from_min_size(pos2(sheet.max.x - 18.0 - done_wide, foot + 12.5), vec2(done_wide, 30.0));
        let done_response = ui.interact(done, ui.scope_id().with("done keys"), egui::Sense::click());
        painter.rect_filled(done, 9.0, if done_response.hovered() { Color32::WHITE } else { TEXT });
        painter.text(
            done.center(),
            Align2::CENTER_CENTER,
            t("keys.done", "Done"),
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

    #[test]
    fn keys_in_words_use_the_macs_symbols_in_its_order_and_other_computers_words() {
        // ⌃ ⌥ ⇧ ⌘ then the key, as the Mac's own menus write them.
        assert_eq!(chord_on(true, false, false, false, true, "1"), "\u{2318}1");
        assert_eq!(chord_on(true, false, false, true, true, "E"), "\u{21e7}\u{2318}E");
        assert_eq!(chord_on(true, true, true, true, true, "K"), "\u{2303}\u{2325}\u{21e7}\u{2318}K");
        assert_eq!(chord_on(true, false, true, false, false, "3"), "\u{2325}3");
        assert_eq!(chord_on(false, false, false, false, true, "1"), "Ctrl+1");
        assert_eq!(chord_on(false, true, true, true, true, "K"), "Ctrl+Alt+Shift+K");
        // What the window says in a hover hint or a notice goes through it.
        let (service, shifted, tab) = (hint("Service", "1", false), hint("Screens", "1", true), tab_hint("Timers", 0));
        if cfg!(target_os = "macos") {
            assert_eq!(
                (service.as_str(), shifted.as_str(), tab.as_str()),
                ("Service (\u{2318}1)", "Screens (\u{21e7}\u{2318}1)", "Timers (\u{2325}1)")
            );
            assert_eq!(hint_key("N"), "\u{2318}N");
        } else {
            assert_eq!(
                (service.as_str(), shifted.as_str(), tab.as_str()),
                ("Service (Ctrl+1)", "Screens (Ctrl+Shift+1)", "Timers (Alt+1)")
            );
            assert_eq!(hint_key("N"), "Ctrl+N");
        }
    }

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
    fn the_f_keys_clear_off_a_mac_and_on_a_mac_one_layer_is_cleared_by_its_button_only() {
        let clearing =
            |f_keys: bool| every_key_on(f_keys).into_iter().find(|(group, _)| *group == "Clearing").expect("listed").1;
        let keys = |rows: &[Row]| rows.iter().flat_map(|(keys, _)| keys.clone()).collect::<Vec<_>>();
        // Windows and Linux: Escape and F1 clear everything; F2 to F6 a layer each.
        assert_eq!(keys(&clearing(true)), ["Esc", "F1", "F2", "F3", "F4", "F5", "F6"]);
        // A Mac: Escape alone, and the layers by their keys in the right column.
        assert_eq!(keys(&clearing(false)), ["Esc", BUTTON_ONLY]);
        let no_f_key = |key: &String| !(key.len() >= 2 && key.starts_with('F') && key[1..].parse::<u8>().is_ok());
        for (group, rows) in every_key_on(false) {
            for (keys, _) in rows {
                assert!(keys.iter().all(no_f_key), "{group}: {keys:?}");
            }
        }
        // The layer keys' tooltips name their F key off a Mac only.
        assert_eq!(clear_hint_on("Clear the props", "F4", true), "Clear the props (F4)");
        assert_eq!(clear_hint_on("Clear the props", "F4", false), "Clear the props");
        assert_eq!(f_keys(), !cfg!(target_os = "macos"));
    }

    #[test]
    fn modifier_and_a_number_opens_that_tab_and_numbers_what_is_in_it() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        press(&ctx, &mut state, 1.0, Key::Num4, egui::Modifiers::ALT);
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
        press(&ctx, &mut state, 10.0, Key::Num1, egui::Modifiers::ALT);
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
    fn command_one_two_three_are_show_bible_and_edit_and_option_numbers_the_tabs() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        press(&ctx, &mut state, 1.0, Key::Num3, egui::Modifiers::COMMAND);
        assert_eq!(state.view_request, Some(1), "Edit");
        press(&ctx, &mut state, 2.0, Key::Num2, egui::Modifiers::COMMAND);
        assert_eq!(state.view_request, Some(2), "Bible");
        assert!(state.bible.focus_lookup, "ready for a reference");
        press(&ctx, &mut state, 3.0, Key::Num1, egui::Modifiers::COMMAND);
        assert_eq!(state.view_request, Some(0), "Show");
        assert!(state.numbers.is_none(), "Command and a number opens no tab");
        // Option-5 (Alt-5 elsewhere) opens the fifth tab and numbers it, even when the key
        // reports the sign a Mac types with Option, as long as it is that place on the keyboard.
        let input = egui::RawInput {
            time: Some(4.0),
            events: vec![
                egui::Event::ModifiersChanged(egui::Modifiers::ALT),
                egui::Event::Key {
                    key: Key::Num5,
                    physical_key: Some(Key::Num5),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                },
            ],
            ..egui::RawInput::default()
        };
        ctx.run_ui(input, |ui| _ = frame(ui.ctx(), &mut state, false)).drop_without_applying_deltas();
        assert_eq!(state.live_tab, 4);
        assert!(state.numbers.is_some());
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
    fn in_an_open_menu_down_then_enter_chooses_its_first_choice() {
        let ctx = egui::Context::default();
        let mut chosen = Vec::new();
        let mut frame_with = |time: f64, events: Vec<egui::Event>, open: bool| {
            let input = egui::RawInput {
                time: Some(time),
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                let button = ui.button("More");
                if open {
                    egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&button));
                }
                egui::Popup::menu(&button).show(|ui| {
                    for name in ["First", "Second"] {
                        if ui.button(name).clicked() {
                            chosen.push(name);
                        }
                    }
                });
            })
            .drop_without_applying_deltas();
        };
        let key = |key: Key| {
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame_with(0.0, Vec::new(), true);
        frame_with(0.1, Vec::new(), false);
        frame_with(0.2, key(Key::ArrowDown), false);
        frame_with(0.3, Vec::new(), false);
        frame_with(0.4, key(Key::Enter), false);
        frame_with(0.5, Vec::new(), false);
        assert_eq!(chosen, ["First"]);
    }

    #[test]
    fn holding_a_modifier_shows_no_tags_and_show_keys_on_buttons_shows_them_all_the_time() {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        let run = |state: &mut HomeState, time: f64, events: Vec<egui::Event>| {
            let input = egui::RawInput { time: Some(time), events, ..Default::default() };
            ctx.run_ui(input, |ui| {
                frame(ui.ctx(), state, false);
            })
            .drop_without_applying_deltas();
        };
        assert!(!tags_shown(&state), "off as it starts");
        for modifiers in [egui::Modifiers::COMMAND, egui::Modifiers::ALT] {
            run(&mut state, 1.0, vec![egui::Event::ModifiersChanged(modifiers)]);
            run(&mut state, 2.0, Vec::new());
            assert!(!tags_shown(&state), "holding {modifiers:?} shows nothing");
        }
        state.keys_on_buttons = true;
        run(&mut state, 3.0, vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
        assert!(tags_shown(&state), "on: shown with nothing held");
        // Kept with the preferences.
        let prefs = state.prefs();
        assert!(prefs.keys_on_buttons);
        let mut fresh = HomeState::default();
        fresh.apply_prefs(&prefs);
        assert!(fresh.keys_on_buttons);
    }

    #[test]
    fn the_switch_at_the_foot_of_the_key_list_turns_the_keys_on_buttons_on_and_off() {
        let ctx = egui::Context::default();
        let mut state = HomeState { key_sheet: true, ..HomeState::default() };
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 900.0));
        let sheet = Rect::from_center_size(screen.center(), vec2(720.0, 868.0));
        let at = pos2(sheet.min.x + 18.0 + 15.0, sheet.max.y - 59.0 + 28.0);
        let frame_with = |state: &mut HomeState, time: f64, events: Vec<egui::Event>| {
            let input = egui::RawInput { time: Some(time), screen_rect: Some(screen), events, ..Default::default() };
            ctx.run_ui(input, |ui| key_sheet(ui.ctx(), state)).drop_without_applying_deltas();
        };
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // The sheet is laid out in its first frame and takes clicks from the next.
        frame_with(&mut state, 0.0, vec![]);
        frame_with(&mut state, 0.05, vec![egui::Event::PointerMoved(at)]);
        frame_with(&mut state, 0.1, vec![press(true)]);
        frame_with(&mut state, 0.2, vec![press(false)]);
        assert!(state.keys_on_buttons && state.key_sheet, "on, and the list stays open");
        frame_with(&mut state, 0.3, vec![press(true)]);
        frame_with(&mut state, 0.4, vec![press(false)]);
        assert!(!state.keys_on_buttons, "off again");
    }
}
