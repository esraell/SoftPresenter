//! Settings › Keys › Running the show, as the design draws it (26 November): what each key that
//! runs the show does, one row a key, from the table in `key_table`.
//!
//! - The head has Add a Key… and Reset to Defaults at its right, always there. Add a Key reads
//!   "Press a key…" in the same width while it listens: the next key pressed by itself joins the
//!   list at the top, set to Nothing; a key the app keeps says why in a sentence; Esc, or a click
//!   anywhere else, stops listening. The keys reach it from the Settings window (`key_pressed`,
//!   called in `main.rs`), by their place on the keyboard.
//! - The first line is the Key layout switch, Standard or Like SoftProjector, with a line saying
//!   what the layout is. Choosing the other layout sets every row; the keys added stay.
//! - Each row has the key's cap, a chooser of one width, and a place for a note: a changed row's
//!   default, or that the key was added, with a × to take it out.
//! - Nothing here changes size when a choice changes: where the window is too narrow for a
//!   line beside its control, the line goes under it, by the window's width and the language
//!   alone, never by what is chosen.

use egui::{Align, Align2, FontId, Layout, Rect, Sense, Stroke, pos2, vec2};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

use super::parts;
use crate::home::HomeState;
use crate::i18n::{t, tf, tk};
use crate::icons::{self, Icon};
use crate::key_table::{ActionGroup, KeyAction, KeyStyle, KeyTable, Pressed, Refusal, ShowKey, rows};
use crate::studio::{
    Flat, LINE, SELECTED, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, even_switch, fixed_semibold, flat_button, icon_key,
    paint_flat, paint_spaced, semibold, spaced, widest,
};

/// The key's column, the room between columns, and how tall a row is, as the design has them.
const KEY_COLUMN: f32 = 84.0;
const BETWEEN: f32 = 16.0;
const ROW: f32 = 44.0;
/// How wide the Key layout switch is: two halves of one width.
const SWITCH: f32 = 300.0;
/// The least a chooser is wide; it is as wide as the longest action's name needs.
const CHOOSER: f32 = 200.0;
/// The most a chooser's list is tall: room for its 22 lines.
const LIST: f32 = 640.0;
/// How long a row just added, or shown instead, stays tinted, in seconds.
const TINT: f64 = 1.2;
/// The small capitals of the headings, and the room between their letters.
const CAPS: f32 = 11.5;
const CAPS_SPACING: f32 = 0.575;
/// How tall a line of the small words is.
const LINE_HEIGHT: f32 = 18.0;

/// What the card keeps between frames.
#[derive(Default)]
pub struct Card {
    /// Add a Key is listening for the next key.
    pub listening: bool,
    /// What Add a Key said last (why a key wasn't added), in its line in the table's head.
    pub say: String,
    /// The row just added or shown instead, tinted for a moment, and when that began (set as it
    /// is first drawn).
    tinted: Option<(ShowKey, Option<f64>)>,
    /// The row whose chooser takes the keyboard, and is scrolled to, as it is next drawn.
    focus: Option<ShowKey>,
    /// The pass the card was last drawn in: not drawn in the pass before, the page has just come
    /// into view, and it listens no more.
    drawn: Option<u64>,
    /// For the tests, which are built on a Mac: the F rows as off a Mac, or not.
    #[cfg(test)]
    pub f_keys: Option<bool>,
}

impl Card {
    /// Whether the table has the F rows here: off a Mac (`keys::f_keys`).
    pub fn f_keys(&self) -> bool {
        #[cfg(test)]
        if let Some(f_keys) = self.f_keys {
            return f_keys;
        }
        // Testing aid: SOFTPRESENTER_KEYS_F_ROWS=1 (or 0), in a copy with a data folder of its
        // own, draws this card with the F rows as off a Mac (or without them, as on one), so
        // either system's page can be pictured on the other. The keys go on as this system's do.
        static ASKED: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
        let asked = ASKED.get_or_init(|| {
            std::env::var_os("SOFTPRESENTER_DATA")?;
            match std::env::var("SOFTPRESENTER_KEYS_F_ROWS").ok()?.as_str() {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            }
        });
        asked.unwrap_or_else(crate::keys::f_keys)
    }

    /// Stops listening, and forgets what was said.
    pub fn stop(&mut self) {
        self.listening = false;
        self.say.clear();
    }

    /// Shows `key`'s row: tinted for a moment, scrolled to, its chooser with the keyboard.
    fn show(&mut self, key: ShowKey) {
        self.tinted = Some((key, None));
        self.focus = Some(key);
    }
}

/// A key pressed in the Settings window while Add a Key listens. Esc stops it (taken before the
/// table hears of it, which would say Esc is in the list already); a key pressed by itself joins
/// the list at the top; one in the list already is shown instead; one the app keeps, or held
/// with a modifier, says why and it goes on listening; a modifier by itself says nothing.
pub fn key_pressed(state: &mut HomeState, pressed: &Pressed) {
    if !listening(state) {
        // Left listening on a page since gone from view: it listens no more.
        state.keys_card.stop();
        return;
    }
    let f_keys = state.keys_card.f_keys();
    if pressed.logical == Key::Named(NamedKey::Escape) || pressed.physical == PhysicalKey::Code(KeyCode::Escape) {
        state.keys_card.stop();
        return;
    }
    match state.keys.add(pressed, f_keys) {
        Ok(key) => {
            state.keys_card.stop();
            state.keys_card.show(key);
        }
        Err(Refusal::Listed(key)) => {
            state.keys_card.stop();
            state.keys_card.say = listed_words(key, f_keys);
            state.keys_card.show(key);
        }
        Err(Refusal::NoKey) => {}
        Err(refusal) => state.keys_card.say = refusal.reason(!f_keys).unwrap_or_default(),
    }
}

/// Whether Add a Key is listening: only while Settings is open on Keys. A page changed from
/// outside the window (a command, a menu) draws nothing of this card again, so the page is
/// asked, not the card: the menus' items and the Settings window's keys go by this.
pub fn listening(state: &HomeState) -> bool {
    state.keys_card.listening && state.preferences_open && state.preferences_pane == super::Pane::Keys
}

/// "{key} is in the list already.", with the key as this system's keyboards print it.
fn listed_words(key: ShowKey, f_keys: bool) -> String {
    tf("settings.keys.refused.listed", "{key} is in the list already.", &[("key", &key.said_for(!f_keys))])
}

/// What the layout chosen is, in a line: off a Mac, or on one (`mac`), where the F keys have no
/// rows.
pub fn style_line(style: KeyStyle, mac: bool) -> &'static str {
    match (style, mac) {
        (KeyStyle::SoftPresenter, false) => {
            t("settings.keys.style.line.sp.other", "Esc and F1 clear everything; F2 to F6 clear one layer each.")
        }
        (KeyStyle::SoftPresenter, true) => t(
            "settings.keys.style.line.sp.mac",
            "Esc clears everything; the layer keys under the preview clear one layer each.",
        ),
        (KeyStyle::SoftProjector, false) => t(
            "settings.keys.style.line.spj.other",
            "As in SoftProjector: Esc hides the words, F5 shows, F2 adds to the playlist; F1 clears all.",
        ),
        (KeyStyle::SoftProjector, true) => t(
            "settings.keys.style.line.spj.mac",
            "As in SoftProjector, Esc hides the words and leaves the background up.",
        ),
    }
}

/// Whether Reset to Defaults has anything to do: a row listed here does something other than
/// its layout says, or a key was added. A row this computer doesn't list (a Mac's F rows,
/// changed in a file from a PC) doesn't count.
pub fn differs(table: &KeyTable, f_keys: bool) -> bool {
    let rows = rows(f_keys);
    rows.iter().any(|key| table.is_changed(*key)) || table.listed(f_keys).len() > rows.len()
}

/// What Reset to Defaults says under the pointer: what it does, naming the layout, or that
/// there is nothing to reset.
pub fn reset_hint(table: &KeyTable, f_keys: bool) -> String {
    match (differs(table, f_keys), table.style) {
        (true, KeyStyle::SoftPresenter) => {
            t("settings.keys.reset.hint.sp", "Every key back to the Standard layout; added keys go")
        }
        (true, KeyStyle::SoftProjector) => {
            t("settings.keys.reset.hint.spj", "Every key back to the SoftProjector layout; added keys go")
        }
        (false, KeyStyle::SoftPresenter) => {
            t("settings.keys.reset.none.sp", "Every key is as the Standard layout has it")
        }
        (false, KeyStyle::SoftProjector) => {
            t("settings.keys.reset.none.spj", "Every key is as the SoftProjector layout has it")
        }
    }
    .to_owned()
}

/// Reset to Defaults: every row back to what the layout chosen says, and the keys added out.
pub fn reset(state: &mut HomeState) {
    state.keys.reset();
    state.keys_card.stop();
    state.keys_card.tinted = None;
    state.keys_card.focus = None;
}

/// A changed row's note: "Default: Clear All".
pub fn default_words(action: KeyAction) -> String {
    tf("settings.keys.default", "Default: {action}", &[("action", &action.words())])
}

/// The line under the table when a slides' action has no key left: "No key does Next Slide
/// now." It names only Next, Previous, First and Last: the clears have their buttons, and the
/// newer actions have no key as they come, so naming them would never let the line go.
pub fn none_line(table: &KeyTable, f_keys: bool) -> String {
    let none: Vec<String> = [KeyAction::Next, KeyAction::Previous, KeyAction::First, KeyAction::Last]
        .into_iter()
        .filter(|action| table.keys_for(*action, f_keys).is_empty())
        .map(KeyAction::words)
        .collect();
    if none.is_empty() {
        return String::new();
    }
    tf("settings.keys.none", "No key does {actions} now.", &[("actions", &crate::keys::any_of(&none))])
}

/// What an added row says of itself: a letter takes over from the group it jumped to; other
/// letters, and digits, still type a reference in Bible.
fn added_note(key: ShowKey) -> String {
    if !key.is_letter_or_digit() {
        return t("settings.keys.added", "Added").to_owned();
    }
    let group = sp_model::GROUP_KEYS.iter().find(|(letter, _)| letter.to_string() == key.name());
    match group {
        Some((_, names)) => tf(
            "settings.keys.added.group",
            "Added \u{b7} no longer jumps to {group}",
            &[("group", &super::group_shown(names))],
        ),
        None => t("settings.keys.added.types", "Added \u{b7} in Bible it still types").to_owned(),
    }
}

/// The note under the table: what Esc does first, and on a Mac where its F keys are.
fn foot_note(mac: bool) -> String {
    let esc =
        t("settings.keys.esc_first", "Whatever Esc does here, it closes an open menu, window or the find box first.");
    if !mac {
        return esc.to_owned();
    }
    let f_keys = t(
        "settings.keys.mac_f_keys",
        "The F keys are the Mac\u{2019}s own (brightness, sound): Add a Key brings one in.",
    );
    format!("{esc} {f_keys}")
}

/// Every sentence Add a Key can say, for the room its line keeps.
fn every_sentence(f_keys: bool) -> Vec<String> {
    use crate::key_table::Kept;
    let kept =
        [Kept::Enter, Kept::Tab, Kept::Up, Kept::Down, Kept::Slash, Kept::QuestionMark, Kept::Delete, Kept::Backspace];
    let mut all: Vec<String> = kept.into_iter().filter_map(|kept| Refusal::Kept(kept).reason(!f_keys)).collect();
    all.extend(Refusal::Held.reason(!f_keys));
    all.push(listed_words(ShowKey::PageDown, f_keys));
    all
}

/// The card.
pub fn keys_card(ui: &mut egui::Ui, state: &mut HomeState) {
    let f_keys = state.keys_card.f_keys();
    // Not drawn in the pass before: the page has just come into view, so it listens no more.
    let pass = ui.ctx().cumulative_pass_nr();
    let card = &mut state.keys_card;
    if card.drawn.is_none_or(|last| pass != last && pass != last + 1) {
        card.stop();
        card.tinted = None;
        card.focus = None;
    }
    card.drawn = Some(pass);
    let mut asked = Asked::default();
    parts::framed(
        ui,
        |_| {},
        |ui| {
            head(ui, state, f_keys, &mut asked);
            layout_line(ui, state, f_keys, &mut asked);
            let chooser = chooser_width(ui);
            table_head(ui, state, f_keys);
            // A chooser's list is as tall as its 22 lines where the window has the room. The
            // toolkit gives a new list's area 400 points whatever its chooser asks for
            // (`default_area_size`), so that is raised while the choosers are made, and put back.
            let taller = TallerLists::raise(ui.ctx());
            for key in state.keys.listed(f_keys) {
                key_row(ui, state, f_keys, key, chooser, &mut asked);
            }
            drop(taller);
            notes(ui, state, f_keys);
        },
    );
    // A click anywhere but on Add a Key stops it listening.
    if state.keys_card.listening && !asked.add && ui.input(|input| input.pointer.any_pressed()) {
        let on_key = ui
            .input(|input| input.pointer.interact_pos())
            .is_some_and(|at| asked.add_key.is_some_and(|key| key.contains(at)));
        if !on_key {
            state.keys_card.stop();
        }
    }
    if asked.add {
        let listening = !state.keys_card.listening;
        state.keys_card.stop();
        state.keys_card.listening = listening;
        // Nothing else has the keyboard while it listens.
        ui.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
    }
    if asked.reset {
        reset(state);
    }
    if let Some(style) = asked.style
        && style != state.keys.style
    {
        state.keys.set_style(style);
    }
    if let Some((key, action)) = asked.set {
        state.keys.set(key, action);
    }
    if let Some(key) = asked.remove {
        state.keys.remove(key);
        state.keys_card.say.clear();
    }
}

/// The room the toolkit gives a new list's area, raised while this lives and put back as it is
/// dropped, however the code between ends.
struct TallerLists {
    ctx: egui::Context,
    usual: egui::Vec2,
}

impl TallerLists {
    fn raise(ctx: &egui::Context) -> TallerLists {
        let usual = ctx.global_style().spacing.default_area_size;
        ctx.global_style_mut(|style| style.spacing.default_area_size.y = usual.y.max(LIST + 60.0));
        TallerLists { ctx: ctx.clone(), usual }
    }
}

impl Drop for TallerLists {
    fn drop(&mut self) {
        let usual = self.usual;
        self.ctx.global_style_mut(|style| style.spacing.default_area_size = usual);
    }
}

/// What was asked of the card in a frame, carried out once it is drawn.
#[derive(Default)]
struct Asked {
    add: bool,
    /// Where Add a Key is, for telling a click elsewhere.
    add_key: Option<Rect>,
    reset: bool,
    style: Option<KeyStyle>,
    set: Option<(ShowKey, KeyAction)>,
    remove: Option<ShowKey>,
}

/// The card's head: its title and its line in small capitals, and at the right Add a Key… and
/// Reset to Defaults, which never move. Where the window is too narrow for all of them on one
/// line, the line in capitals goes under the title, and then the keys under that.
fn head(ui: &mut egui::Ui, state: &HomeState, f_keys: bool, asked: &mut Asked) {
    let width = ui.available_width();
    let font = FontId::proportional(12.5);
    let add_words = t("settings.keys.add", "Add a Key\u{2026}");
    let listening_words = t("settings.keys.add.listening", "Press a key\u{2026}");
    let reset_words = t("settings.keys.reset", "Reset to Defaults");
    // Add a Key keeps one width whichever it says: its words with the + before them, or
    // "Press a key…".
    let add_wide = (widest(ui, &[add_words], &font) + 18.0).max(widest(ui, &[listening_words], &font)) + 26.0;
    let reset_wide = parts::key_size(ui, reset_words).x;
    let keys_wide = add_wide + 8.0 + reset_wide;
    let title =
        parts::lines(ui, t("settings.keys.show.title", "Running the show"), semibold(15.0), TEXT, f32::INFINITY, 21.0);
    let about = t("settings.keys.show.about", "What each key does").to_uppercase();
    let about_font = crate::studio::fixed(CAPS);
    let about_wide = spaced(ui, &about, about_font.clone(), TEXT_FAINT, CAPS_SPACING).size().x;
    let title_wide = title.size().x;
    let one_line = title_wide + 10.0 + about_wide + 10.0 + keys_wide <= width;
    let keys_beside = title_wide + 10.0 + keys_wide <= width;
    let (place, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    ui.painter().galley(pos2(place.min.x, place.center().y - title.size().y / 2.0), title, TEXT);
    let about_at = |ui: &egui::Ui, at: egui::Pos2| {
        paint_spaced(ui, at, Align2::LEFT_CENTER, &about, about_font.clone(), TEXT_FAINT, CAPS_SPACING);
    };
    let keys_place = if one_line {
        about_at(ui, pos2(place.min.x + title_wide + 10.0, place.center().y + 1.0));
        place
    } else {
        let (line, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
        about_at(ui, pos2(line.min.x, line.center().y));
        if keys_beside {
            place
        } else {
            ui.add_space(4.0);
            ui.allocate_exact_size(vec2(width, 30.0), Sense::hover()).0
        }
    };
    let reset_at = Rect::from_min_size(pos2(keys_place.max.x - reset_wide, keys_place.min.y), vec2(reset_wide, 30.0));
    let add_at = Rect::from_min_size(pos2(reset_at.min.x - 8.0 - add_wide, keys_place.min.y), vec2(add_wide, 30.0));
    asked.add_key = Some(add_at);
    // Add a Key…, lit and reading "Press a key…" while it listens.
    let listening = state.keys_card.listening;
    let response = ui.interact(add_at, ui.scope_id().with("add a key"), Sense::click());
    paint_flat(ui.painter(), add_at, 9.0, if listening { Flat::On } else { Flat::Plain }, response.hovered());
    if listening {
        ui.painter().text(add_at.center(), Align2::CENTER_CENTER, listening_words, font.clone(), TEXT);
    } else {
        let words = ui.painter().layout_no_wrap(add_words.to_owned(), font.clone(), TEXT);
        let left = add_at.center().x - (words.size().x + 18.0) / 2.0;
        icons::paint(
            ui.painter(),
            Icon::Plus,
            Rect::from_center_size(pos2(left + 6.0, add_at.center().y), vec2(12.0, 12.0)),
            TEXT,
        );
        ui.painter().galley(pos2(left + 18.0, add_at.center().y - words.size().y / 2.0), words, TEXT);
    }
    let hint = t(
        "settings.keys.add.hint",
        "Any key on the keyboard, or a clicker\u{2019}s button, can be given something to do",
    );
    asked.add = response.on_hover_text(hint).clicked();
    // Reset to Defaults: always there, greyed while there is nothing to reset.
    let lit = differs(&state.keys, f_keys);
    let reset = flat_button(ui, reset_at, ui.scope_id().with("reset keys"), reset_words, font, Flat::Plain, lit);
    asked.reset = reset.on_hover_text(reset_hint(&state.keys, f_keys)).clicked();
    ui.add_space(10.0);
}

/// The Key layout line: its caption in the key's column, the switch of two halves, and a line
/// saying what the layout chosen is, which keeps the room of the longer of the two; beside the
/// switch, or under it where the window leaves it too little room there.
fn layout_line(ui: &mut egui::Ui, state: &HomeState, f_keys: bool, asked: &mut Asked) {
    let width = ui.available_width();
    let font = FontId::proportional(12.0);
    let line_left = KEY_COLUMN + BETWEEN + SWITCH + BETWEEN;
    let beside = width - line_left >= 220.0;
    let room = if beside { width - line_left } else { width };
    let line_of = |ui: &egui::Ui, style| {
        parts::lines(ui, style_line(style, !f_keys), font.clone(), TEXT_FAINT, room, LINE_HEIGHT)
    };
    // Two lines' room at least, and as much as the longer layout's line takes.
    let line_room = [KeyStyle::SoftPresenter, KeyStyle::SoftProjector]
        .into_iter()
        .map(|style| line_of(ui, style).size().y)
        .fold(2.0 * LINE_HEIGHT, f32::max);
    let first = if beside { line_room.max(30.0) } else { 30.0 };
    let (place, _) = ui.allocate_exact_size(vec2(width, first), Sense::hover());
    let middle = place.center().y;
    // The language's usual words, or its short form where they are too long for the column.
    let (key, english) = tk("settings.keys.style.caption", "Key layout");
    let caption = crate::studio::within(ui, key, english, fixed_semibold(CAPS), CAPS_SPACING, KEY_COLUMN, true);
    paint_spaced(
        ui,
        pos2(place.min.x, middle),
        Align2::LEFT_CENTER,
        &caption,
        fixed_semibold(CAPS),
        TEXT_DIM,
        CAPS_SPACING,
    );
    let track = Rect::from_min_size(pos2(place.min.x + KEY_COLUMN + BETWEEN, middle - 15.0), vec2(SWITCH, 30.0));
    let halves = [
        t("settings.keys.style.softpresenter", "Standard"),
        t("settings.keys.style.softprojector", "Like SoftProjector"),
    ];
    let styles = [KeyStyle::SoftPresenter, KeyStyle::SoftProjector];
    let chosen = styles.iter().position(|style| *style == state.keys.style);
    let clicked = even_switch(ui, "key layout", track, &halves, &[], chosen, FontId::proportional(12.5));
    if let Some(at) = clicked {
        asked.style = Some(styles[at]);
    }
    let line = line_of(ui, state.keys.style);
    if beside {
        ui.painter().galley(pos2(place.min.x + line_left, middle - line.size().y / 2.0), line, TEXT_FAINT);
    } else {
        ui.add_space(6.0);
        let (under, _) = ui.allocate_exact_size(vec2(width, line_room), Sense::hover());
        ui.painter().galley(pos2(under.min.x, under.center().y - line.size().y / 2.0), line, TEXT_FAINT);
    }
    ui.add_space(12.0);
}

/// How wide every chooser is: what the longest action's name needs in the language set, with
/// the arrow, and 200 at least, so that no choice ever changes its size.
fn chooser_width(ui: &egui::Ui) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let names: Vec<String> = KeyAction::ALL.iter().map(|action| action.words()).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let spacing = ui.spacing();
    let around = 2.0 * spacing.button_padding.x + spacing.icon_spacing + spacing.icon_width;
    (widest(ui, &names, &font) + around + 2.0).ceil().max(CHOOSER)
}

/// The table's head: KEY and DOES over their columns, and Add a Key's line, always kept: what
/// it said last, or, with nothing said, faintly what Add a Key is for, so the line never reads
/// as a gap. It starts a little after DOES where every sentence fits there on one line, else it
/// is under the headings, with the room the longest takes.
fn table_head(ui: &mut egui::Ui, state: &HomeState, f_keys: bool) {
    let width = ui.available_width();
    let font = FontId::proportional(12.0);
    let does = t("settings.keys.col.does", "Does").to_uppercase();
    let does_left = KEY_COLUMN + BETWEEN;
    let say_left = does_left + spaced(ui, &does, fixed_semibold(CAPS), TEXT_DIM, CAPS_SPACING).size().x + BETWEEN;
    let hint = t(
        "settings.keys.add.hint",
        "Any key on the keyboard, or a clicker\u{2019}s button, can be given something to do",
    );
    let mut sentences = every_sentence(f_keys);
    sentences.push(hint.to_owned());
    let wide: Vec<&str> = sentences.iter().map(String::as_str).collect();
    let beside = widest(ui, &wide, &font) <= width - say_left;
    let (place, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let middle = place.min.y + 12.0;
    let heading = |left: f32, words: &str| {
        paint_spaced(
            ui,
            pos2(place.min.x + left, middle),
            Align2::LEFT_CENTER,
            words,
            fixed_semibold(CAPS),
            TEXT_DIM,
            CAPS_SPACING,
        );
    };
    heading(0.0, &t("settings.keys.col.key", "Key").to_uppercase());
    heading(does_left, &does);
    let (say, ink) = match state.keys_card.say.as_str() {
        "" => (hint, TEXT_FAINT),
        said => (said, TEXT),
    };
    if beside {
        let galley = parts::one_line(ui, say, font, ink, width - say_left);
        ui.painter().galley(pos2(place.min.x + say_left, middle - galley.size().y / 2.0), galley, ink);
        return;
    }
    let room = sentences
        .iter()
        .map(|sentence| parts::lines(ui, sentence, font.clone(), TEXT, width, LINE_HEIGHT).size().y)
        .fold(LINE_HEIGHT, f32::max);
    let (under, _) = ui.allocate_exact_size(vec2(width, room + 6.0), Sense::hover());
    let galley = parts::lines(ui, say, font, ink, width, LINE_HEIGHT);
    ui.painter().galley(under.min, galley, ink);
}

/// A line of a chooser's list.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Line {
    Heading(&'static str),
    Action(KeyAction),
    Separator,
}

/// What every chooser lists: the actions in their three groups, each under its heading, then a
/// separator and Nothing.
fn chooser_list() -> Vec<Line> {
    let mut list = Vec::new();
    for (group, heading) in [
        (ActionGroup::Clear, t("settings.keys.group.clear", "Clear")),
        (ActionGroup::Slides, t("settings.keys.group.slides", "Slides")),
        (ActionGroup::PlaylistViews, t("settings.keys.group.playlist_views", "Playlist and views")),
    ] {
        list.push(Line::Heading(heading));
        list.extend(KeyAction::ALL.into_iter().filter(|action| action.group() == group).map(Line::Action));
    }
    list.push(Line::Separator);
    list.push(Line::Action(KeyAction::Nothing));
    list
}

/// One key's row: a line over it, the key's cap, its chooser, and at the right a changed row's
/// default or an added key's note, and an added key's ×.
fn key_row(ui: &mut egui::Ui, state: &mut HomeState, f_keys: bool, key: ShowKey, chooser: f32, asked: &mut Asked) {
    let width = ui.available_width();
    let (place, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
    // A row just added, or shown instead of being added twice, is tinted for a moment.
    if let Some((tinted, since)) = state.keys_card.tinted
        && tinted == key
    {
        let now = ui.input(|input| input.time);
        let since = since.unwrap_or(now);
        let left = 1.0 - (now - since) / TINT;
        if left > 0.0 {
            ui.painter().rect_filled(place, 4.0, TALLY.gamma_multiply(0.16 * left as f32));
            ui.ctx().request_repaint();
            state.keys_card.tinted = Some((key, Some(since)));
        } else {
            state.keys_card.tinted = None;
        }
    }
    ui.painter().hline(place.x_range(), place.min.y + 0.5, Stroke::new(1.0, LINE));
    let middle = place.center().y;
    // The cap, as the list of keys draws one; a name too long for its column is cut short, and
    // said whole under the pointer.
    let cap = key.cap_for(!f_keys);
    let cap_font = fixed_semibold(11.0);
    let galley = parts::one_line(ui, &cap, cap_font, TEXT, KEY_COLUMN - 14.0);
    let cap_at = Rect::from_min_size(pos2(place.min.x, middle - 12.0), vec2((galley.size().x + 14.0).max(44.0), 24.0));
    ui.painter().rect_filled(cap_at, 6.0, SELECTED);
    if galley.elided {
        ui.interact(cap_at, ui.scope_id().with(("key cap", key.name())), Sense::hover()).on_hover_text(cap.as_str());
    }
    ui.painter().galley(cap_at.center() - galley.size() / 2.0, galley, TEXT);
    // The chooser, one width on every row.
    let now = state.keys.action(key);
    let cell =
        Rect::from_min_size(pos2(place.min.x + KEY_COLUMN + BETWEEN, place.min.y + 1.0), vec2(chooser, ROW - 1.0));
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(cell).layout(Layout::left_to_right(Align::Center)));
    // As tall as the whole list where the window has that much under the row or over it.
    let window = ui.ctx().content_rect();
    let list_room = ((window.max.y - cell.max.y).max(cell.min.y - window.min.y) - 24.0).clamp(160.0, LIST);
    let shown = egui::ComboBox::from_id_salt(("key does", key.name()))
        .selected_text(now.words())
        .width(chooser)
        .height(list_room)
        .show_ui(&mut inside, |ui| {
            let mut chosen = None;
            for line in chooser_list() {
                match line {
                    Line::Heading(heading) => {
                        ui.add_space(4.0);
                        let heading =
                            egui::RichText::new(heading.to_uppercase()).font(fixed_semibold(10.5)).color(TEXT_FAINT);
                        ui.add(egui::Label::new(heading).selectable(false));
                    }
                    Line::Separator => {
                        ui.separator();
                    }
                    Line::Action(action) => {
                        if ui.add(egui::Button::selectable(now == action, action.words())).clicked() {
                            chosen = Some(action);
                        }
                    }
                }
            }
            chosen
        });
    if let Some(action) = shown.inner.flatten() {
        asked.set = Some((key, action));
    }
    if state.keys_card.focus == Some(key) {
        state.keys_card.focus = None;
        shown.response.request_focus();
        ui.scroll_to_rect(place, None);
    }
    // The note, in a column of one width on every row, and an added key's ×.
    let added = state.keys.is_added(key) && !rows(f_keys).contains(&key);
    let note = if added {
        added_note(key)
    } else if state.keys.is_changed(key) {
        default_words(KeyTable::default_action(state.keys.style, key))
    } else {
        String::new()
    };
    let note_left = place.min.x + KEY_COLUMN + BETWEEN + chooser + BETWEEN;
    let note_right = if added { place.max.x - 28.0 - 10.0 } else { place.max.x };
    if !note.is_empty() && note_right - note_left > 20.0 {
        // Two lines at most inside the row, and only then cut short, said whole under the pointer.
        let mut job = egui::text::LayoutJob::default();
        let look = egui::TextFormat {
            font_id: FontId::proportional(12.0),
            color: TEXT_DIM,
            line_height: Some(16.0),
            ..Default::default()
        };
        job.append(&note, 0.0, look);
        job.wrap = egui::text::TextWrapping { max_width: note_right - note_left, max_rows: 2, ..Default::default() };
        let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
        let cut = galley.elided;
        let at = Rect::from_min_size(pos2(note_left, middle - galley.size().y / 2.0), galley.size());
        ui.painter().galley(at.min, galley, TEXT_DIM);
        if cut {
            ui.interact(at, ui.scope_id().with(("key note", key.name())), Sense::hover()).on_hover_text(note.as_str());
        }
    }
    if added {
        let at = Rect::from_min_size(pos2(place.max.x - 28.0, middle - 14.0), vec2(28.0, 28.0));
        let hint = tf("settings.keys.remove", "Take {key} out of the list", &[("key", &key.said_for(!f_keys))]);
        if icon_key(ui, at, ui.scope_id().with(("remove key", key.name())), Icon::Close, 10.0, true)
            .on_hover_text(hint)
            .clicked()
        {
            asked.remove = Some(key);
        }
    }
}

/// Under the table: what Esc does first (and, on a Mac, where its F keys are), and a line of
/// its own, always kept, for a slides' action no key does.
fn notes(ui: &mut egui::Ui, state: &HomeState, f_keys: bool) {
    let width = ui.available_width();
    let top = ui.cursor().min;
    ui.painter().hline(top.x..=top.x + width, top.y + 0.5, Stroke::new(1.0, LINE));
    ui.add_space(10.0);
    let font = FontId::proportional(12.0);
    let note = parts::lines(ui, &foot_note(!f_keys), font.clone(), TEXT_FAINT, width, LINE_HEIGHT);
    let (place, _) = ui.allocate_exact_size(vec2(width, note.size().y), Sense::hover());
    ui.painter().galley(place.min, note, TEXT_FAINT);
    let (place, _) = ui.allocate_exact_size(vec2(width, LINE_HEIGHT), Sense::hover());
    let none = none_line(&state.keys, f_keys);
    if !none.is_empty() {
        let galley = parts::one_line(ui, &none, font, TEXT_DIM, width);
        let cut = galley.elided;
        ui.painter().galley(pos2(place.min.x, place.center().y - galley.size().y / 2.0), galley, TEXT_DIM);
        if cut {
            ui.interact(place, ui.scope_id().with("no key does"), Sense::hover()).on_hover_text(none.as_str());
        }
    }
    ui.add_space(8.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::settings::tests::words_drawn;
    use crate::home::settings::{Pane, settings_ui};
    use crate::i18n::Language;
    use crate::key_table::Refusal;
    use egui::{pos2, vec2};
    use sp_engine::Show;
    use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey};

    /// Settings at Keys, as on a computer with the F rows (`f_keys`) or a Mac.
    fn state_on(f_keys: bool) -> HomeState {
        let mut state = HomeState { preferences_open: true, preferences_pane: Pane::Keys, ..HomeState::default() };
        state.keys_card.f_keys = Some(f_keys);
        state
    }

    /// What two frames of the page drew in a window `width` wide and tall enough for all of it:
    /// every piece of words with its place, and every block (a card, a key, a chooser).
    fn page(ctx: &egui::Context, state: &mut HomeState, width: f32) -> (Vec<(String, Rect)>, Vec<[i32; 4]>) {
        fn blocks(shape: &egui::Shape, into: &mut Vec<[i32; 4]>) {
            match shape {
                egui::Shape::Rect(block) => {
                    let tenths = |x: f32| (x * 10.0).round() as i32;
                    let rect = block.rect;
                    into.push([tenths(rect.min.y), tenths(rect.min.x), tenths(rect.width()), tenths(rect.height())]);
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| blocks(shape, into)),
                _ => {}
            }
        }
        let show = Show::new(Vec::new());
        let (mut words, mut drawn_blocks) = (Vec::new(), Vec::new());
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 4000.0))),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| settings_ui(ui, state, &show));
            words = words_placed(&output);
            drawn_blocks.clear();
            output.shapes.iter().for_each(|clipped| blocks(&clipped.shape, &mut drawn_blocks));
            output.drop_without_applying_deltas();
        }
        drawn_blocks.sort_unstable();
        drawn_blocks.dedup();
        (words, drawn_blocks)
    }

    /// Every piece of words a frame drew, with the room it was laid out in (which, unlike the
    /// ink of its letters, is the same whatever the words are).
    fn words_placed(output: &egui::FullOutput) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => {
                    into.push((text.galley.text().to_owned(), text.galley.rect.translate(text.pos.to_vec2())));
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut into = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut into));
        into
    }

    fn shown(words: &[(String, Rect)], wanted: &str) -> bool {
        words.iter().any(|(have, _)| have == wanted)
    }

    fn place(words: &[(String, Rect)], wanted: &str) -> Rect {
        words.iter().find(|(have, _)| have == wanted).unwrap_or_else(|| panic!("{wanted} isn\u{2019}t drawn")).1
    }

    /// One frame of Settings in a window 1280 by 1400, tall enough for a chooser's whole list.
    fn frame(ctx: &egui::Context, state: &mut HomeState, events: Vec<egui::Event>) -> egui::FullOutput {
        let show = Show::new(Vec::new());
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 1400.0))),
            events,
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| settings_ui(ui, state, &show))
    }

    /// Clicks at `at` in that window.
    fn click_at(ctx: &egui::Context, state: &mut HomeState, at: egui::Pos2) {
        use egui::{Event, Modifiers};
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [vec![Event::PointerMoved(at), press(true)], vec![press(false)]] {
            frame(ctx, state, events).drop_without_applying_deltas();
        }
    }

    /// Clicks the words `name` where that window drew them first (`last`: last, in an open
    /// list). False when they weren't drawn.
    fn click_where(ctx: &egui::Context, state: &mut HomeState, name: &str, last: bool) -> bool {
        let mut at = None;
        for _ in 0..2 {
            let output = frame(ctx, state, vec![]);
            at = if last {
                crate::studio::tests::last_place_of(&output, name)
            } else {
                crate::studio::tests::place_of(&output, name)
            };
            output.drop_without_applying_deltas();
        }
        let Some(at) = at else { return false };
        click_at(ctx, state, at);
        true
    }

    fn click(ctx: &egui::Context, state: &mut HomeState, _show: &Show, name: &str) -> bool {
        click_where(ctx, state, name, false)
    }

    fn click_last(ctx: &egui::Context, state: &mut HomeState, _show: &Show, name: &str) -> bool {
        click_where(ctx, state, name, true)
    }

    fn key(named: NamedKey) -> Pressed {
        Pressed::from(Key::Named(named))
    }

    fn typed(letter: &str) -> Pressed {
        Pressed::from(Key::Character(letter.into()))
    }

    #[test]
    fn choosing_an_action_in_a_row_changes_the_table_and_shows_its_default() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = state_on(false);
        // Esc's chooser, the first on the page, opens its list: the three groups under their
        // headings, in order, then Nothing.
        assert!(click(&ctx, &mut state, &show, "Clear All"), "Esc\u{2019}s chooser");
        frame(&ctx, &mut state, vec![]).drop_without_applying_deltas();
        let open = frame(&ctx, &mut state, vec![]);
        let mut above = f32::MIN;
        // In a window with the room, the whole list shows without scrolling.
        let listed = [
            "CLEAR",
            "Hide the Words",
            "Clear Slide",
            "Clear Media",
            "Clear Props",
            "Clear Audio",
            "Clear Messages",
            "SLIDES",
            "Show Selected",
            "Show Again",
            "PLAYLIST AND VIEWS",
            "Add to Playlist",
            "Remove from Playlist",
            "Go to Bible",
            "Go to Service",
            "Nothing",
        ];
        for words in listed {
            let at = crate::studio::tests::last_place_of(&open, words).unwrap_or_else(|| panic!("{words} is listed"));
            assert!(at.y > above, "{words} comes in its place");
            above = at.y;
        }
        let usual = ctx.global_style().spacing.default_area_size;
        assert_eq!(usual, egui::Style::default().spacing.default_area_size, "the toolkit\u{2019}s own is put back");
        open.drop_without_applying_deltas();
        use KeyAction as A;
        let actions = |actions: &[KeyAction]| actions.iter().map(|action| Line::Action(*action)).collect::<Vec<_>>();
        let mut whole = vec![Line::Heading("Clear")];
        whole.extend(actions(&[
            A::ClearAll,
            A::HideWords,
            A::ClearSlide,
            A::ClearMedia,
            A::ClearProps,
            A::ClearAudio,
            A::ClearMessages,
        ]));
        whole.push(Line::Heading("Slides"));
        whole.extend(actions(&[A::ShowSelected, A::ShowAgain, A::Next, A::Previous, A::First, A::Last]));
        whole.push(Line::Heading("Playlist and views"));
        whole.extend(actions(&[A::AddToPlaylist, A::RemoveFromPlaylist, A::GoToBible, A::GoToService]));
        whole.extend([Line::Separator, Line::Action(A::Nothing)]);
        assert_eq!(chooser_list(), whole);
        assert!(click_last(&ctx, &mut state, &show, "Clear Slide"));
        assert_eq!(state.keys.action(ShowKey::Escape), KeyAction::ClearSlide);
        let (words, _) = page(&ctx, &mut state, 1280.0);
        assert!(shown(&words, "Default: Clear All"), "the row says what it was");
        // The row's default chosen again puts it back, and its note goes.
        assert!(click(&ctx, &mut state, &show, "Clear Slide"), "Esc\u{2019}s chooser again");
        assert!(click_last(&ctx, &mut state, &show, "Clear All"));
        assert_eq!(state.keys.action(ShowKey::Escape), KeyAction::ClearAll);
        let (words, _) = page(&ctx, &mut state, 1280.0);
        assert!(!words.iter().any(|(have, _)| have.starts_with("Default:")));
    }

    #[test]
    fn the_style_switch_sets_every_row_and_reset_returns_to_the_style() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = state_on(true);
        state.keys.set(ShowKey::Space, KeyAction::Nothing);
        assert!(click(&ctx, &mut state, &show, "Like SoftProjector"));
        assert_eq!(state.keys.style, KeyStyle::SoftProjector);
        assert_eq!(state.keys.action(ShowKey::F5), KeyAction::ShowSelected);
        assert_eq!(state.keys.action(ShowKey::F1), KeyAction::ClearAll);
        assert_eq!(state.keys.action(ShowKey::Space), KeyAction::Next, "a changed row is the layout\u{2019}s again");
        assert_eq!(reset_hint(&state.keys, true), "Every key is as the SoftProjector layout has it");
        // A row changed and a key added: Reset to Defaults goes back to the layout chosen, and
        // the key added goes.
        state.keys.set(ShowKey::F2, KeyAction::ClearSlide);
        state.keys.add(&typed("b"), true).expect("added");
        assert_eq!(reset_hint(&state.keys, true), "Every key back to the SoftProjector layout; added keys go");
        assert!(click(&ctx, &mut state, &show, "Reset to Defaults"));
        assert_eq!(state.keys.action(ShowKey::F2), KeyAction::AddToPlaylist);
        assert_eq!(state.keys.listed(true), crate::key_table::rows(true));
        assert_eq!(state.keys.style, KeyStyle::SoftProjector, "the layout stays");
        assert!(click(&ctx, &mut state, &show, "Standard"));
        assert_eq!(state.keys.style, KeyStyle::SoftPresenter);
        assert_eq!(state.keys.action(ShowKey::F5), KeyAction::ClearAudio);
        assert_eq!(reset_hint(&state.keys, true), "Every key is as the Standard layout has it");
        // Choosing a layout leaves the keys added as they are.
        state.keys.add(&typed("b"), true).expect("added");
        state.keys.set(ShowKey::Other(KeyCode::KeyB), KeyAction::Next);
        assert!(click(&ctx, &mut state, &show, "Like SoftProjector"));
        assert_eq!(state.keys.action(ShowKey::Other(KeyCode::KeyB)), KeyAction::Next);
        assert_eq!(state.keys.listed(true)[0], ShowKey::Other(KeyCode::KeyB));
    }

    #[test]
    fn choosing_the_style_already_chosen_keeps_a_changed_row() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = state_on(false);
        state.keys.set(ShowKey::Escape, KeyAction::ClearSlide);
        assert!(click(&ctx, &mut state, &show, "Standard"), "the lit half");
        assert_eq!(state.keys.style, KeyStyle::SoftPresenter);
        assert_eq!(state.keys.action(ShowKey::Escape), KeyAction::ClearSlide, "nothing changed");
        // The other half still switches.
        assert!(click(&ctx, &mut state, &show, "Like SoftProjector"));
        assert_eq!(state.keys.action(ShowKey::Escape), KeyAction::HideWords);
    }

    #[test]
    fn the_style_line_says_what_each_style_does_on_and_off_a_mac() {
        let lines = [
            (KeyStyle::SoftPresenter, false, "Esc and F1 clear everything; F2 to F6 clear one layer each."),
            (
                KeyStyle::SoftPresenter,
                true,
                "Esc clears everything; the layer keys under the preview clear one layer each.",
            ),
            (
                KeyStyle::SoftProjector,
                false,
                "As in SoftProjector: Esc hides the words, F5 shows, F2 adds to the playlist; F1 clears all.",
            ),
            (KeyStyle::SoftProjector, true, "As in SoftProjector, Esc hides the words and leaves the background up."),
        ];
        for (style, mac, line) in lines {
            assert_eq!(style_line(style, mac), line);
            // On the page, under its caption and beside the switch's two halves.
            let ctx = crate::studio::tests::styled();
            let mut state = state_on(!mac);
            state.keys.set_style(style);
            let (words, _) = page(&ctx, &mut state, 1280.0);
            for wanted in [line, "KEY LAYOUT", "Standard", "Like SoftProjector", "KEY", "DOES"] {
                assert!(shown(&words, wanted), "{style:?}, mac {mac}: {wanted}");
            }
            // The Mac's note under the table, and only there.
            let mac_note = "Whatever Esc does here, it closes an open menu, window or the find box first. The F keys are the Mac\u{2019}s own (brightness, sound): Add a Key brings one in.";
            assert_eq!(shown(&words, mac_note), mac);
            let note = "Whatever Esc does here, it closes an open menu, window or the find box first.";
            assert_eq!(shown(&words, note), !mac);
            // A Mac has nine rows, a PC twenty-one: F12 only off a Mac.
            assert_eq!(shown(&words, "F12"), !mac);
            for cap in ["Esc", "Space", "\u{2192}", "\u{2190}", "Page Down", "Page Up", "Home", "End", "."] {
                assert!(shown(&words, cap), "mac {mac}: the row of {cap}");
            }
        }
    }

    #[test]
    fn no_key_doing_next_is_said_under_the_table() {
        let ctx = crate::studio::tests::styled();
        let mut state = state_on(false);
        // As it comes nothing is said, though no key does Show Selected or Add to Playlist.
        assert_eq!(none_line(&state.keys, false), "");
        for key in [ShowKey::Space, ShowKey::Right, ShowKey::PageDown] {
            state.keys.set(key, KeyAction::Nothing);
        }
        assert_eq!(none_line(&state.keys, false), "No key does Next Slide now.");
        let (words, _) = page(&ctx, &mut state, 1280.0);
        assert!(shown(&words, "No key does Next Slide now."));
        for key in [ShowKey::Left, ShowKey::PageUp, ShowKey::End] {
            state.keys.set(key, KeyAction::ShowAgain);
        }
        assert_eq!(none_line(&state.keys, false), "No key does Next Slide, Previous Slide or Last Slide now.");
        // In SoftProjector's layout on a Mac no key does Clear All: its button does, and the
        // line says nothing of it.
        let mut state = state_on(false);
        state.keys.set_style(KeyStyle::SoftProjector);
        assert_eq!(none_line(&state.keys, false), "");
        // A key added that does Next counts.
        for key in [ShowKey::Space, ShowKey::Right, ShowKey::PageDown] {
            state.keys.set(key, KeyAction::Nothing);
        }
        state.keys.add(&typed("n"), false).expect("added");
        state.keys.set(ShowKey::Other(KeyCode::KeyN), KeyAction::Next);
        assert_eq!(none_line(&state.keys, false), "");
    }

    #[test]
    fn add_a_key_listens_adds_at_the_top_and_esc_or_a_click_elsewhere_cancels() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = state_on(false);
        let rows = crate::key_table::rows(false);
        assert!(click(&ctx, &mut state, &show, "Add a Key\u{2026}"));
        assert!(state.keys_card.listening);
        let (words, _) = page(&ctx, &mut state, 1280.0);
        assert!(shown(&words, "Press a key\u{2026}") && !shown(&words, "Add a Key\u{2026}"));
        // A key the app keeps is refused in a sentence, and it goes on listening.
        key_pressed(&mut state, &key(NamedKey::Enter));
        let enter = "Enter always puts the picked slide on screen. Press another.";
        assert_eq!((state.keys_card.say.as_str(), state.keys_card.listening), (enter, true));
        let (words, _) = page(&ctx, &mut state, 1280.0);
        assert!(shown(&words, enter));
        key_pressed(&mut state, &typed("?"));
        assert_eq!(state.keys_card.say, "? always opens the list of keys. Press another.");
        // A modifier by itself says nothing new; a key held with one is the menus'.
        key_pressed(&mut state, &key(NamedKey::Shift));
        assert_eq!(state.keys_card.say, "? always opens the list of keys. Press another.");
        let held = Pressed { held: ModifiersState::SUPER, ..typed("b") };
        key_pressed(&mut state, &held);
        assert_eq!(
            state.keys_card.say,
            "Keys held with \u{2318}, \u{2303} or \u{2325} are the menus\u{2019}. Press a key by itself."
        );
        assert!(state.keys_card.listening);
        // Esc stops listening and is never said to be in the list.
        key_pressed(&mut state, &key(NamedKey::Escape));
        assert!(!state.keys_card.listening && state.keys_card.say.is_empty());
        assert_eq!(state.keys.listed(false), rows, "nothing was added");
        // Not listening, a key is none of the card's.
        key_pressed(&mut state, &typed("b"));
        assert_eq!(state.keys.listed(false), rows);
        // A key pressed by itself joins at the top, set to Nothing, with its note and its ×.
        assert!(click(&ctx, &mut state, &show, "Add a Key\u{2026}"));
        key_pressed(&mut state, &typed("b"));
        let b = ShowKey::Other(KeyCode::KeyB);
        assert_eq!(state.keys.listed(false)[0], b);
        assert_eq!(state.keys.action(b), KeyAction::Nothing);
        assert!(!state.keys_card.listening && state.keys_card.say.is_empty());
        let (words, _) = page(&ctx, &mut state, 1280.0);
        assert!(shown(&words, "Added \u{b7} no longer jumps to Bridge 1"));
        assert!(place(&words, "B").center().y < place(&words, "Esc").center().y, "above the layout\u{2019}s rows");
        assert!(shown(&words, "Add a Key\u{2026}"));
        // The same key again is shown instead, with a sentence.
        assert!(click(&ctx, &mut state, &show, "Add a Key\u{2026}"));
        key_pressed(&mut state, &typed("b"));
        assert_eq!((state.keys_card.say.as_str(), state.keys_card.listening), ("B is in the list already.", false));
        assert_eq!(state.keys.listed(false).len(), rows.len() + 1);
        // A click anywhere else stops listening and adds nothing.
        assert!(click(&ctx, &mut state, &show, "Add a Key\u{2026}"));
        assert!(state.keys_card.listening && state.keys_card.say.is_empty());
        assert!(click(&ctx, &mut state, &show, "KEY LAYOUT"));
        assert!(!state.keys_card.listening);
        // The other notes: a digit still types in Bible, a mark is only added, and an F key on a
        // Mac comes in doing what the layout gives it.
        state.keys.set_style(KeyStyle::SoftProjector);
        for pressed in [typed("1"), typed(","), key(NamedKey::F1)] {
            assert!(click(&ctx, &mut state, &show, "Add a Key\u{2026}"));
            key_pressed(&mut state, &pressed);
        }
        assert_eq!(state.keys.action(ShowKey::F1), KeyAction::ClearAll);
        let (words, _) = page(&ctx, &mut state, 1280.0);
        assert!(shown(&words, "Added \u{b7} in Bible it still types"));
        assert_eq!(words.iter().filter(|(have, _)| have == "Added").count(), 2, "the comma and F1");
        // Its × takes a key out again: at the right end of its row, under Reset to Defaults.
        let drawn = frame(&ctx, &mut state, vec![]);
        let words = words_drawn(&drawn);
        drawn.drop_without_applying_deltas();
        let at = pos2(place(&words, "Reset to Defaults").max.x - 1.0, place(&words, "F1").center().y);
        click_at(&ctx, &mut state, at);
        assert!(!state.keys.is_added(ShowKey::F1), "F1 is out of the list");
        assert_eq!(state.keys.listed(false).len(), rows.len() + 3);
    }

    #[test]
    fn a_key_added_to_the_table_has_its_note_and_a_cross_that_takes_it_out() {
        let ctx = crate::studio::tests::styled();
        let mut state = state_on(false);
        state.keys.set_style(KeyStyle::SoftProjector);
        for pressed in [typed("b"), typed("1"), typed(","), key(NamedKey::F1)] {
            state.keys.add(&pressed, false).expect("added");
        }
        // Newest first, over the layout's rows; an F key on a Mac does what the layout gives it.
        assert_eq!(state.keys.listed(false)[0], ShowKey::F1);
        assert_eq!(state.keys.action(ShowKey::F1), KeyAction::ClearAll);
        let drawn = frame(&ctx, &mut state, vec![]);
        drawn.drop_without_applying_deltas();
        let drawn = frame(&ctx, &mut state, vec![]);
        let words = words_placed(&drawn);
        drawn.drop_without_applying_deltas();
        assert!(shown(&words, "Added \u{b7} no longer jumps to Bridge 1"), "a group\u{2019}s letter");
        assert!(shown(&words, "Added \u{b7} in Bible it still types"), "a digit");
        assert_eq!(words.iter().filter(|(have, _)| have == "Added").count(), 2, "the comma and F1");
        assert!(place(&words, "B").center().y < place(&words, "Esc").center().y, "above the layout\u{2019}s rows");
        // Its × is at the right end of its row, under Reset to Defaults.
        let at = pos2(place(&words, "Reset to Defaults").max.x - 1.0, place(&words, "F1").center().y);
        click_at(&ctx, &mut state, at);
        assert!(!state.keys.is_added(ShowKey::F1), "F1 is out of the list");
        assert_eq!(state.keys.listed(false).len(), crate::key_table::rows(false).len() + 3);
    }

    #[test]
    fn the_sentences_line_is_beside_the_headings_at_1100_and_says_what_add_a_key_does() {
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for (width, f_keys) in [(1100.0, true), (1100.0, false), (1040.0, true), (1280.0, false), (820.0, true)]
                {
                    let said = format!("{language:?} at {width}, F keys {f_keys}");
                    let ctx = crate::studio::tests::styled();
                    let mut state = state_on(f_keys);
                    let (words, _) = page(&ctx, &mut state, width);
                    // With nothing said, the line says what Add a Key is for, so it never reads
                    // as a gap.
                    let hint = t(
                        "settings.keys.add.hint",
                        "Any key on the keyboard, or a clicker\u{2019}s button, can be given something to do",
                    );
                    assert!(shown(&words, hint), "{said}: the line says what Add a Key does");
                    // From 1040 up the first row follows the headings at once: no band between.
                    let heading = place(&words, &t("settings.keys.col.key", "Key").to_uppercase());
                    let names: Vec<String> = KeyAction::ALL.iter().map(|action| action.words()).collect();
                    let first = words
                        .iter()
                        .filter(|(have, _)| names.contains(have))
                        .map(|(_, rect)| rect.center().y)
                        .fold(f32::MAX, f32::min);
                    if width >= 1040.0 {
                        assert!(first - heading.center().y < 45.0, "{said}: a band of {}", first - heading.center().y);
                        assert!((place(&words, hint).center().y - heading.center().y).abs() < 2.0, "{said}");
                    }
                    // A sentence takes the hint's place.
                    state.keys_card.listening = true;
                    state.keys_card.say = Refusal::Held.reason(!f_keys).unwrap_or_default();
                    let (words, _) = page(&ctx, &mut state, width);
                    assert!(shown(&words, &state.keys_card.say) && !shown(&words, hint), "{said}");
                }
            });
        }
    }

    #[test]
    fn leaving_the_keys_page_without_a_click_ends_listening() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = state_on(false);
        let rows = crate::key_table::rows(false);
        assert!(click(&ctx, &mut state, &show, "Add a Key\u{2026}"));
        assert!(listening(&state));
        // Another page is chosen from outside the window (a command), and nothing is drawn yet:
        // it listens no more, so the menus come back and a key pressed is not added.
        state.preferences_pane = Pane::About;
        assert!(!listening(&state), "not on the Keys page");
        key_pressed(&mut state, &typed("b"));
        assert_eq!(state.keys.listed(false), rows, "nothing was added");
        assert!(!state.keys_card.listening);
        // The same when Settings is closed.
        state.preferences_pane = Pane::Keys;
        assert!(click(&ctx, &mut state, &show, "Add a Key\u{2026}"));
        state.preferences_open = false;
        assert!(!listening(&state), "Settings is closed");
        key_pressed(&mut state, &typed("b"));
        assert_eq!(state.keys.listed(false), rows);
    }

    #[test]
    fn an_added_keys_note_takes_two_lines_in_the_narrowest_window() {
        let ctx = crate::studio::tests::styled();
        let mut state = state_on(true);
        state.keys.add(&typed("b"), true).expect("added");
        let note = "Added \u{b7} no longer jumps to Bridge 1";
        let (wide, _) = page(&ctx, &mut state, 1280.0);
        assert!(place(&wide, note).height() < 20.0, "one line where there is room");
        let (narrow, _) = page(&ctx, &mut state, 820.0);
        let at = place(&narrow, note);
        assert!(at.height() > 20.0 && at.height() <= 40.0, "two lines inside its row: {}", at.height());
        // In the middle of its row, beside its key's cap.
        assert!((at.center().y - place(&narrow, "B").center().y).abs() < 3.0);
    }

    #[test]
    fn reset_is_lit_only_while_a_row_listed_here_differs_or_a_key_was_added() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        // A PC's file with F2 changed, read on a Mac: no row here differs.
        let mut state = state_on(false);
        state.keys.set(ShowKey::F2, KeyAction::HideWords);
        assert!(differs(&state.keys, true) && !differs(&state.keys, false));
        assert!(click(&ctx, &mut state, &show, "Reset to Defaults"));
        assert_eq!(state.keys.action(ShowKey::F2), KeyAction::HideWords, "greyed: it takes no click");
        state.keys.add(&typed("b"), false).expect("added");
        assert!(differs(&state.keys, false), "a key added lights it");
        state.keys.remove(ShowKey::Other(KeyCode::KeyB));
        state.keys.set(ShowKey::Space, KeyAction::Nothing);
        assert!(differs(&state.keys, false), "and so does a row changed");
    }

    #[test]
    fn nothing_on_the_keys_page_moves_when_a_choice_or_the_style_changes() {
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for (width, f_keys) in [(820.0, true), (820.0, false), (1280.0, true), (1280.0, false)] {
                    let said = format!("{language:?} at {width}, F keys {f_keys}");
                    let ctx = crate::studio::tests::styled();
                    let mut state = state_on(f_keys);
                    // What may change: what the choosers say, the style's line, a changed row's
                    // note, Add a Key's words and what it says. Everything else stays as it is.
                    let held = Refusal::Held.reason(!f_keys).unwrap_or_default();
                    let mut changing: Vec<String> = KeyAction::ALL.iter().map(|action| action.words()).collect();
                    for style in [KeyStyle::SoftPresenter, KeyStyle::SoftProjector] {
                        changing.push(style_line(style, !f_keys).to_owned());
                    }
                    changing.push(default_words(KeyAction::ClearAll));
                    changing.push(default_words(KeyAction::HideWords));
                    changing.push(t("settings.keys.add", "Add a Key\u{2026}").to_owned());
                    changing.push(t("settings.keys.add.listening", "Press a key\u{2026}").to_owned());
                    changing.push(held.clone());
                    changing.push(
                        t(
                            "settings.keys.add.hint",
                            "Any key on the keyboard, or a clicker\u{2019}s button, can be given something to do",
                        )
                        .to_owned(),
                    );
                    let still = |words: &[(String, Rect)]| -> Vec<(String, Rect)> {
                        words.iter().filter(|(have, _)| !changing.contains(have)).cloned().collect()
                    };
                    // Where each chooser's words start: one for every row.
                    let choosers = |words: &[(String, Rect)]| -> Vec<(i32, i32)> {
                        let names: Vec<String> = KeyAction::ALL.iter().map(|action| action.words()).collect();
                        words
                            .iter()
                            .filter(|(have, _)| names.contains(have))
                            .map(|(_, rect)| ((rect.min.x * 10.0).round() as i32, (rect.min.y * 10.0).round() as i32))
                            .collect()
                    };
                    let standard = page(&ctx, &mut state, width);
                    assert_eq!(choosers(&standard.0).len(), crate::key_table::rows(f_keys).len(), "{said}");
                    // The other layout: only the switch's lit half moves, to a half as large.
                    state.keys.set_style(KeyStyle::SoftProjector);
                    let softprojector = page(&ctx, &mut state, width);
                    assert_eq!(still(&softprojector.0), still(&standard.0), "{said}: the layout moved words");
                    assert_eq!(choosers(&softprojector.0), choosers(&standard.0), "{said}: a chooser moved");
                    let only = |a: &[[i32; 4]], b: &[[i32; 4]]| -> Vec<[i32; 4]> {
                        a.iter().filter(|block| !b.contains(block)).copied().collect()
                    };
                    let (was, now) = (only(&standard.1, &softprojector.1), only(&softprojector.1, &standard.1));
                    assert_eq!((was.len(), now.len()), (1, 1), "{said}: only the lit half moves: {was:?} {now:?}");
                    assert_eq!((was[0][0], was[0][2], was[0][3]), (now[0][0], now[0][2], now[0][3]), "{said}");
                    let line = |words: &[(String, Rect)], style| place(words, style_line(style, !f_keys));
                    let (one, other) =
                        (line(&standard.0, KeyStyle::SoftPresenter), line(&softprojector.0, KeyStyle::SoftProjector));
                    assert!((one.center().y - other.center().y).abs() <= 1.5, "{said}: the style\u{2019}s line");
                    assert!((one.min.x - other.min.x).abs() <= 0.5, "{said}: the style\u{2019}s line");
                    // A row changed: its note comes, and nothing else moves.
                    state.keys.set(ShowKey::Escape, KeyAction::ClearSlide);
                    let changed = page(&ctx, &mut state, width);
                    let note = default_words(KeyAction::HideWords);
                    let is_note = |have: &str| {
                        let start = have.trim_end_matches('\u{2026}');
                        start.chars().count() > 3 && note.starts_with(start)
                    };
                    assert!(changed.0.iter().any(|(have, _)| is_note(have)), "{said}: the changed row\u{2019}s note");
                    let no_cut = |words: &[(String, Rect)]| -> Vec<(String, Rect)> {
                        still(words).into_iter().filter(|(have, _)| !have.ends_with('\u{2026}')).collect()
                    };
                    assert_eq!(no_cut(&changed.0), no_cut(&softprojector.0), "{said}: a choice moved words");
                    assert_eq!(choosers(&changed.0), choosers(&standard.0), "{said}: a choice moved a chooser");
                    assert_eq!(changed.1, softprojector.1, "{said}: a choice moved a block");
                    // Reset to Defaults, then the first layout again: as they were.
                    reset(&mut state);
                    let after = page(&ctx, &mut state, width);
                    assert_eq!(after, softprojector, "{said}: Reset to Defaults");
                    state.keys.set_style(KeyStyle::SoftPresenter);
                    let back = page(&ctx, &mut state, width);
                    assert_eq!(back, standard, "{said}: back to the first layout");
                    // Listening, with a refusal said: the same blocks, the same words elsewhere.
                    state.keys_card.listening = true;
                    state.keys_card.say = held.clone();
                    let asking = page(&ctx, &mut state, width);
                    assert!(state.keys_card.listening, "{said}");
                    assert!(shown(&asking.0, &held), "{said}: the sentence is shown whole");
                    assert_eq!(still(&asking.0), still(&standard.0), "{said}: listening moved words");
                    assert_eq!(asking.1, standard.1, "{said}: listening moved a block");
                }
            });
        }
    }

    #[test]
    fn the_keys_page_stays_inside_the_narrowest_window_in_every_language() {
        let right = 820.0 - crate::studio::GAP - 28.0;
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for f_keys in [true, false] {
                    let ctx = crate::studio::tests::styled();
                    let mut state = state_on(f_keys);
                    state.keys.set_style(KeyStyle::SoftProjector);
                    state.keys.set(ShowKey::Escape, KeyAction::RemoveFromPlaylist);
                    for pressed in [typed("b"), typed("1"), key(NamedKey::Insert)] {
                        state.keys.add(&pressed, f_keys).expect("added");
                    }
                    page(&ctx, &mut state, 820.0);
                    state.keys_card.listening = true;
                    state.keys_card.say = Refusal::Held.reason(!f_keys).unwrap_or_default();
                    let (words, blocks) = page(&ctx, &mut state, 820.0);
                    for (have, rect) in words.iter().filter(|(have, _)| !have.is_empty()) {
                        assert!(
                            rect.max.x <= right + 0.5,
                            "{language:?}, F keys {f_keys}: \u{201c}{have}\u{201d} ends at {}",
                            rect.max.x
                        );
                    }
                    // The page's blocks (those that start in it) end inside its panel.
                    let page_left = ((crate::studio::GAP + crate::studio::LEFT_WIDTH) * 10.0) as i32;
                    for block in blocks.iter().filter(|block| block[1] > page_left) {
                        let ends = (block[1] + block[2]) as f32 / 10.0;
                        assert!(ends <= 820.0 - crate::studio::GAP + 0.5, "{language:?}: a block ends at {ends}");
                    }
                }
            });
        }
    }
}
