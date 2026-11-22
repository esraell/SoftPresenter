//! The look of the app and the parts of the main window drawn in it: the top bar, the song
//! list, the Program panel and the media strip.
//!
//! The look is of a piece of equipment: near-black surfaces in steps with almost no lines, a
//! lighter grey for "on" and "chosen", and one colour, orange, kept only for what is on screen
//! right now.

use std::sync::Arc;

use egui::{
    Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Rect, Sense, Stroke, StrokeKind,
    TextureId, pos2, vec2,
};
use sp_engine::{Command, Layer, Show, SlideRef};

use crate::editor::EditDraft;
use crate::home::{HomeState, SlideView, ThumbOf, matches_filter};
use crate::icons::{self, Icon};

// A dark, quiet window built like a piece of equipment: greys in steps, each thing that sits on
// another a step lighter than it, and one colour, orange, kept for what is on screen.

/// The window behind the panels.
pub const BG: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0c);
/// The three columns, and bars.
pub const PANEL: Color32 = Color32::from_rgb(0x19, 0x19, 0x19);
/// A region set into a panel: the dock under the slides, the objects shelf in Edit.
#[allow(dead_code)] // Used as each page of the new design lands.
pub const SHELF: Color32 = Color32::from_rgb(0x15, 0x15, 0x15);
/// Things that sit on a panel: cards, rows that are chosen, fields.
pub const CARD: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
/// A row or cell under the pointer: lighter than what it lies on, never darker.
pub const HOVER: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x2b);
/// The rare line between two areas.
pub const LINE: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
pub const TEXT: Color32 = Color32::from_rgb(0xf3, 0xf3, 0xef);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0xb9, 0xb9, 0xb2);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x8f, 0x8f, 0x88);
/// Chosen, in focus: outlines, lines and marks in light grey (and, faded, a lighter row).
pub const ACCENT: Color32 = Color32::from_rgb(0xb9, 0xb9, 0xb2);
/// Switched on or chosen: the fill of a chosen tab or button. A lighter grey, never white.
pub const SELECTED: Color32 = Color32::from_rgb(0x50, 0x50, 0x50);
/// On screen right now. Used for nothing else.
pub const TALLY: Color32 = Color32::from_rgb(0xff, 0x4a, 0x1a);
/// Take care; the tags that show an action's key.
pub const CARE: Color32 = Color32::from_rgb(0xff, 0xc4, 0x00);
/// The output is on and ready.
pub const READY: Color32 = Color32::from_rgb(0x2f, 0xd3, 0x6b);
/// The corner of rows, small buttons and fields.
pub const RADIUS: f32 = 7.0;
/// The corner of keys, cards and sheets.
pub const RADIUS_CARD: f32 = 11.0;
/// The corner of the three columns.
pub const RADIUS_PANEL: f32 = 18.0;
/// The room between the three columns, and between them and the window's edges.
pub const GAP: f32 = 10.0;
/// How wide the left and the right column are; the middle takes the rest.
pub const LEFT_WIDTH: f32 = 244.0;
pub const RIGHT_WIDTH: f32 = 360.0;
/// How tall the bar along the top of the window is.
pub const TOP_BAR: f32 = 52.0;
/// A button: its fill, and lighter under the pointer. Buttons are flat, with no outline (the
/// edge is the fill's own colour, kept so that nothing changes size where one was drawn).
pub const BUTTON: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
pub const BUTTON_HOVER: Color32 = Color32::from_rgb(0x3b, 0x3b, 0x3b);
pub const BUTTON_EDGE: Color32 = BUTTON;
const BUTTON_EDGE_HOVER: Color32 = BUTTON_HOVER;
/// The edge under a raised key (the layer keys and Clear All).
pub const KEY_EDGE: Color32 = Color32::from_rgb(0x05, 0x05, 0x05);
/// A layer key while its layer shows: the orange mixed 22% into a button's grey, and under it
/// the orange mixed 30% into the edge.
pub const KEY_LIT: Color32 = Color32::from_rgb(0x5e, 0x36, 0x2b);
pub const KEY_LIT_EDGE: Color32 = Color32::from_rgb(0x50, 0x1a, 0x0b);
/// The edge under Clear All while anything shows: the orange, 40% darker.
pub const CLEAR_ALL_EDGE: Color32 = Color32::from_rgb(0x99, 0x2c, 0x10);
/// The Audience and Stage keys while their screens are on: the green mixed 12% into a
/// button's grey; and under the pointer.
pub const SCREENS_ON: Color32 = Color32::from_rgb(0x30, 0x44, 0x37);
pub const SCREENS_ON_HOVER: Color32 = Color32::from_rgb(0x3a, 0x4f, 0x41);
/// Small words on the preview block's black.
pub const SCREEN_DIM: Color32 = Color32::from_rgb(0xa3, 0xa3, 0x9c);

/// Paints a button's block, as every button is drawn: its fill, lighter under the pointer (or
/// while it is on), and its outline.
pub fn paint_button(painter: &egui::Painter, rect: Rect, lit: bool) {
    painter.rect_filled(rect, RADIUS, if lit { BUTTON_HOVER } else { BUTTON });
    let edge = if lit { BUTTON_EDGE_HOVER } else { BUTTON_EDGE };
    painter.rect_stroke(rect, RADIUS, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
}

/// One of the window's three columns.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Left,
    Middle,
    Right,
}

/// The room a column keeps clear around itself: the side columns on both sides (so the middle,
/// which takes what is left, needs none), and all three at the foot of the window.
pub fn column_margin(column: Column) -> egui::Margin {
    let gap = GAP as i8;
    match column {
        Column::Left | Column::Right => egui::Margin { left: gap, right: gap, top: 0, bottom: gap },
        Column::Middle => egui::Margin { left: 0, right: 0, top: 0, bottom: gap },
    }
}

/// The frame of one of the three columns: a panel with round corners, standing clear of the
/// others and of the window's edges. Give it the inner margin its contents want.
pub fn column_frame(column: Column) -> Frame {
    Frame::new().fill(PANEL).corner_radius(RADIUS_PANEL as u8).outer_margin(column_margin(column))
}

/// How much of the window a side column takes: itself and the room on both sides of it.
pub fn column_outer(width: f32) -> f32 {
    width + 2.0 * GAP
}

/// The panel inside what a column's frame took up (`outer`, the frame's rectangle).
pub fn column_panel(outer: Rect, column: Column) -> Rect {
    outer - column_margin(column)
}

/// Rounds a panel's corners after its contents are drawn: whatever they painted into the four
/// corners (a bar's fill, a list's row) is painted over in the window's colour. The toolkit
/// cuts drawing off only at straight edges, so this is how a panel of several parts keeps
/// round corners.
pub fn round_corners(ui: &egui::Ui, panel: Rect) {
    // An outline outside the round shape, kept to the panel's own rectangle: only the corners.
    // 8 wide covers the whole of a corner of 18 (the corner's point is 7.5 from the curve).
    ui.painter().with_clip_rect(panel).rect_stroke(panel, RADIUS_PANEL, Stroke::new(8.0, BG), StrokeKind::Outside);
}

/// One line of words in a face with room added between its letters, as the small capital
/// labels and the numbers in the fixed-width face are set.
pub fn spaced(ui: &egui::Ui, text: &str, font: FontId, ink: Color32, spacing: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat { font_id: font, color: ink, extra_letter_spacing: spacing, ..Default::default() };
    job.append(text, 0.0, format);
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// Paints one line set with [`spaced`], anchored at `at`. Returns where it was drawn.
pub fn paint_spaced(
    ui: &egui::Ui,
    at: egui::Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    ink: Color32,
    spacing: f32,
) -> Rect {
    let galley = spaced(ui, text, font, ink, spacing);
    let rect = anchor.anchor_size(at, galley.size());
    ui.painter().galley(rect.min, galley, ink);
    rect
}

/// A small round light with a halo round it while it is lit, as the layer keys and the
/// Audience and Stage keys carry. Unlit it is a dark dot.
pub fn paint_light(painter: &egui::Painter, centre: egui::Pos2, lit: Option<Color32>) {
    match lit {
        Some(colour) => {
            painter.circle_filled(centre, 6.5, colour.gamma_multiply(0.24));
            painter.circle_filled(centre, 3.5, colour);
        }
        None => {
            painter.circle_filled(centre, 3.5, KEY_EDGE);
        }
    }
}

/// The faint inner edge of every chosen control (a pill, a key, a flat or menu button that is
/// on): white at 14%, as the design has it.
pub const ON_EDGE: Color32 = Color32::from_rgba_premultiplied(36, 36, 36, 36);

/// The pill of a segmented switch or of a row of tabs: the chosen one is the lighter grey
/// with a faint inner edge, the others are only their words.
fn paint_pill(painter: &egui::Painter, cell: Rect, on: bool) {
    if on {
        let round = cell.height() / 2.0;
        painter.rect_filled(cell, round, SELECTED);
        painter.rect_stroke(cell, round, Stroke::new(1.0, ON_EDGE), StrokeKind::Inside);
    }
}

/// How a segmented switch is set: its face, the room between its letters, the room either side
/// of each word, how tall a pill is, the room round the pills, and the track's colour.
pub struct SwitchLook {
    pub font: FontId,
    pub spacing: f32,
    pub pad: f32,
    pub height: f32,
    pub inset: f32,
    pub track: Color32,
    pub idle: Color32,
}

impl SwitchLook {
    /// The switch between the views, in the bar along the top.
    pub fn views() -> SwitchLook {
        SwitchLook {
            font: FontId::proportional(12.5),
            spacing: 0.0,
            pad: 15.0,
            height: 26.9,
            inset: 3.0,
            track: CARD,
            idle: TEXT_DIM,
        }
    }

    /// The small switch in the preview block's head, in capitals.
    pub fn small_capitals() -> SwitchLook {
        SwitchLook {
            font: fixed_semibold(11.0),
            spacing: 0.7,
            pad: 9.0,
            height: 17.0,
            inset: 2.0,
            track: Color32::from_rgb(0x16, 0x16, 0x16),
            idle: SCREEN_DIM,
        }
    }
}

/// The least height of anything that can be clicked: a smaller thing drawn keeps its size and
/// takes clicks from this much room.
pub const LEAST_CLICK: f32 = 26.0;

/// A segmented switch: a dark, fully round track with a pill for each choice, each as wide as
/// its words, the chosen one the lighter grey. Its top left corner goes at `min`. `hints` are
/// the tooltips, one for each choice (or none at all). Returns the room it took and the choice
/// clicked.
pub fn pill_switch(
    ui: &egui::Ui,
    salt: &str,
    min: egui::Pos2,
    labels: &[&str],
    hints: &[&str],
    chosen: Option<usize>,
    look: &SwitchLook,
) -> (Rect, Option<usize>) {
    const BETWEEN: f32 = 2.0;
    let height = look.height + 2.0 * look.inset;
    // Laid out twice (once to measure, once to draw); the toolkit keeps each line it has set.
    let wide = |label: &str| spaced(ui, label, look.font.clone(), look.idle, look.spacing).size().x + 2.0 * look.pad;
    let total: f32 = labels.iter().map(|label| wide(label)).sum::<f32>()
        + BETWEEN * labels.len().saturating_sub(1) as f32
        + 2.0 * look.inset;
    let track = Rect::from_min_size(min, vec2(total, height));
    ui.painter().rect_filled(track, height / 2.0, look.track);
    let reach = ((LEAST_CLICK - look.height) / 2.0).max(0.0);
    let mut clicked = None;
    let mut x = track.min.x + look.inset;
    for (index, label) in labels.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(x, track.min.y + look.inset), vec2(wide(label), look.height));
        x += cell.width() + BETWEEN;
        let mut response =
            ui.interact(cell.expand2(vec2(0.0, reach)), ui.scope_id().with((salt, index)), Sense::click());
        let on = chosen == Some(index);
        paint_pill(ui.painter(), cell, on);
        let ink = if on || response.hovered() { TEXT } else { look.idle };
        paint_spaced(ui, cell.center(), Align2::CENTER_CENTER, label, look.font.clone(), ink, look.spacing);
        if let Some(hint) = hints.get(index) {
            response = response.on_hover_text(*hint);
        }
        if response.clicked() {
            clicked = Some(index);
        }
    }
    (track, clicked)
}

/// A small segmented switch of icons, as the Bible page's Keypad | Lists: a dark, fully round
/// track with a pill 26 × 18 for each icon, the chosen one the lighter grey. Its top left
/// corner goes at `min`. Returns the room it took and the choice clicked.
pub fn icon_switch(
    ui: &egui::Ui,
    salt: &str,
    min: egui::Pos2,
    icons: &[Icon],
    hints: &[&str],
    chosen: Option<usize>,
) -> (Rect, Option<usize>) {
    const CELL: egui::Vec2 = vec2(26.0, 18.0);
    const INSET: f32 = 3.0;
    const BETWEEN: f32 = 2.0;
    let count = icons.len() as f32;
    let track = Rect::from_min_size(
        min,
        vec2(2.0 * INSET + CELL.x * count + BETWEEN * (count - 1.0).max(0.0), CELL.y + 2.0 * INSET),
    );
    ui.painter().rect_filled(track, track.height() / 2.0, CARD);
    let reach = ((LEAST_CLICK - CELL.y) / 2.0).max(0.0);
    let mut clicked = None;
    for (index, icon) in icons.iter().enumerate() {
        let cell = Rect::from_min_size(
            pos2(track.min.x + INSET + (CELL.x + BETWEEN) * index as f32, track.min.y + INSET),
            CELL,
        );
        let mut response =
            ui.interact(cell.expand2(vec2(0.0, reach)), ui.scope_id().with((salt, index)), Sense::click());
        let on = chosen == Some(index);
        paint_pill(ui.painter(), cell, on);
        let ink = if on || response.hovered() { TEXT } else { TEXT_DIM };
        icons::paint(ui.painter(), *icon, Rect::from_center_size(cell.center(), egui::Vec2::splat(12.0)), ink);
        if let Some(hint) = hints.get(index) {
            response = response.on_hover_text(*hint);
        }
        if response.clicked() {
            clicked = Some(index);
        }
    }
    (track, clicked)
}

/// Where each pill of a [`pill_switch`] with its top left corner at `min` stands.
pub fn pill_cells(ui: &egui::Ui, min: egui::Pos2, labels: &[&str], look: &SwitchLook) -> Vec<Rect> {
    let mut x = min.x + look.inset;
    labels
        .iter()
        .map(|label| {
            let wide = spaced(ui, label, look.font.clone(), look.idle, look.spacing).size().x + 2.0 * look.pad;
            let cell = Rect::from_min_size(pos2(x, min.y + look.inset), vec2(wide, look.height));
            x += wide + 2.0;
            cell
        })
        .collect()
}

/// How tall a block of tabs made by [`tab_switch`] is.
pub fn tab_switch_height(tabs: &Tabs) -> f32 {
    let rows = tabs.labels.len().div_ceil(TABS_IN_A_ROW).max(1) as f32;
    6.0 + rows * tabs.tall + (rows - 1.0) * 2.0
}

/// The tabs of a [`tab_switch`]: their names, their tooltips (one each, or none at all), and
/// for each the colour of the small dot after its name while its thing is active.
#[derive(Clone, Copy)]
pub struct Tabs<'a> {
    pub labels: &'a [&'a str],
    pub hints: &'a [&'a str],
    pub dots: &'a [Option<Color32>],
    /// A key's tag for each tab, while the modifier is held (none otherwise).
    pub tags: &'a [String],
    /// The tabs that cannot be chosen now (none, when empty): drawn faint, and not clickable.
    pub off: &'a [bool],
    /// How tall a tab is drawn (it takes clicks from at least [`LEAST_CLICK`]), and the size
    /// of its words.
    pub tall: f32,
    pub size: f32,
}

impl<'a> Tabs<'a> {
    /// Tabs of the usual size with nothing but their names.
    pub fn plain(labels: &'a [&'a str]) -> Tabs<'a> {
        Tabs { labels, hints: &[], dots: &[], tags: &[], off: &[], tall: 26.9, size: 12.5 }
    }
}

/// How many tabs stand in one row; more go on to a second row and share it evenly.
const TABS_IN_A_ROW: usize = 4;

/// Tabs as one block of pills `width` wide with its top left corner at `min`: four to a row,
/// the tabs of each row sharing its width evenly, the open one the lighter grey. `dots` gives
/// a tab a small dot after its name while its thing is active (in that colour). Returns the
/// room it took and the tab clicked.
pub fn tab_switch(
    ui: &egui::Ui,
    salt: &str,
    min: egui::Pos2,
    width: f32,
    tabs: &Tabs,
    chosen: Option<usize>,
) -> (Rect, Option<usize>) {
    let Tabs { labels, hints, dots, tags, off, tall, size } = *tabs;
    const BETWEEN: f32 = 2.0;
    const INSET: f32 = 3.0;
    let block = Rect::from_min_size(min, vec2(width, tab_switch_height(tabs)));
    // One row is fully round; two rows are a block with a rounder corner than a card's.
    let round = if labels.len() > TABS_IN_A_ROW { 14.0 } else { block.height() / 2.0 };
    ui.painter().rect_filled(block, round, CARD);
    let font = FontId::proportional(size);
    let reach = ((LEAST_CLICK - tall) / 2.0).clamp(0.0, INSET);
    let mut clicked = None;
    for (row, names) in labels.chunks(TABS_IN_A_ROW).enumerate() {
        let each = (width - 2.0 * INSET - BETWEEN * (names.len() - 1) as f32) / names.len() as f32;
        for (place, label) in names.iter().enumerate() {
            let index = row * TABS_IN_A_ROW + place;
            let corner = pos2(
                block.min.x + INSET + (each + BETWEEN) * place as f32,
                block.min.y + INSET + (tall + BETWEEN) * row as f32,
            );
            let cell = Rect::from_min_size(corner, vec2(each, tall));
            let is_off = off.get(index).copied().unwrap_or(false);
            let sense = if is_off { Sense::hover() } else { Sense::click() };
            let mut response = ui.interact(cell.expand2(vec2(0.0, reach)), ui.scope_id().with((salt, index)), sense);
            let on = chosen == Some(index);
            paint_pill(ui.painter(), cell, on);
            let ink = match (is_off, on || response.hovered()) {
                (true, _) => TEXT_FAINT,
                (false, true) => TEXT,
                (false, false) => TEXT_DIM,
            };
            // With a key's tag, the name and the tag stand together in the middle.
            let tag_wide = tags.get(index).map_or(0.0, |tag| crate::keys::tag_width(ui.painter(), tag) + 5.0);
            let centre = cell.center() - vec2(tag_wide / 2.0, 0.0);
            let words = ui.painter().text(centre, Align2::CENTER_CENTER, *label, font.clone(), ink);
            if let Some(Some(colour)) = dots.get(index) {
                ui.painter().circle_filled(pos2(words.max.x + 6.5, cell.center().y), 2.5, *colour);
            }
            if let Some(tag) = tags.get(index) {
                crate::keys::tag(ui.painter(), words.max.x + tag_wide, cell.center().y, tag);
            }
            if let Some(hint) = hints.get(index) {
                response = response.on_hover_text(*hint);
            }
            if response.clicked() {
                clicked = Some(index);
            }
        }
    }
    (block, clicked)
}

/// How far a raised key stands above what it is on, and so how far it goes down when pressed.
pub const KEY_TRAVEL: f32 = 3.0;

/// A raised key (only the layer keys and Clear All are raised): a block standing on an edge
/// 3 deep, drawn under it. While it is pressed the block goes down onto the edge. `rect` is the
/// block at rest; the edge is drawn below it. Returns the response and where the block is this
/// frame, to draw its light and its words on.
pub fn raised_key(ui: &egui::Ui, rect: Rect, id: egui::Id, fill: Color32, edge: Color32) -> (egui::Response, Rect) {
    let response = ui.interact(rect, id, Sense::click());
    let down = response.is_pointer_button_down_on();
    let painter = ui.painter();
    let face = if down { rect.translate(vec2(0.0, KEY_TRAVEL)) } else { rect };
    if !down {
        painter.rect_filled(rect.translate(vec2(0.0, KEY_TRAVEL)), RADIUS_CARD, edge);
    }
    painter.rect_filled(face, RADIUS_CARD, fill);
    (response, face)
}

/// A small round button that is only its icon, as the keys of the video on screen are: `fill`
/// under it, lighter under the pointer; `on` gives it the lighter grey of a choice that is on.
pub fn round_key(ui: &egui::Ui, centre: egui::Pos2, side: f32, id: &str, fill: Color32, on: bool) -> egui::Response {
    let cell = Rect::from_center_size(centre, egui::Vec2::splat(side));
    let response = ui.interact(cell, ui.scope_id().with(id), Sense::click());
    let painter = ui.painter();
    if on {
        painter.circle_filled(centre, side / 2.0, SELECTED);
        painter.circle_stroke(centre, side / 2.0 - 0.5, Stroke::new(1.0, ON_EDGE));
    } else {
        painter.circle_filled(centre, side / 2.0, if response.hovered() { BUTTON_HOVER } else { fill });
    }
    response
}

/// A card whose thing is running or playing (a timer, a track): the green mixed 13% into a
/// card's grey. A timer is not on screen, so it is never orange.
pub const CARD_RUNNING: Color32 = Color32::from_rgb(0x25, 0x3a, 0x2c);

/// A card whose thing is on screen (a message, a prop): the on-screen orange mixed 16% into a
/// card's grey. Its button stays the grey of a choice that is on.
pub const CARD_SHOWN: Color32 = Color32::from_rgb(0x46, 0x29, 0x22);

/// How a flat button is drawn: plain (the button grey), on (the lighter grey of a choice that
/// is on, with a faint inner edge), or the one main action of its place (the ink, with dark
/// words).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Flat {
    Plain,
    On,
    Main,
}

/// Paints a flat button's block in `rect` with corners of `corner`, lighter under the pointer.
pub fn paint_flat(painter: &egui::Painter, rect: Rect, corner: f32, look: Flat, hovered: bool) {
    match look {
        Flat::Plain => {
            painter.rect_filled(rect, corner, if hovered { BUTTON_HOVER } else { BUTTON });
        }
        Flat::On => {
            painter.rect_filled(rect, corner, SELECTED);
            painter.rect_stroke(rect, corner, Stroke::new(1.0, ON_EDGE), StrokeKind::Inside);
        }
        Flat::Main => {
            painter.rect_filled(rect, corner, if hovered { Color32::WHITE } else { TEXT });
        }
    }
}

/// How wide a flat button with words is: the longest of the wordings it can show, with `pad`
/// either side, so it keeps one width whatever it says (Show and Hide, Start and Stop).
pub fn flat_button_width(ui: &egui::Ui, wordings: &[&str], font: &FontId, pad: f32) -> f32 {
    let widest = wordings
        .iter()
        .map(|words| ui.painter().layout_no_wrap((*words).to_owned(), font.clone(), TEXT).size().x)
        .fold(0.0, f32::max);
    widest + 2.0 * pad
}

/// A flat button with words in the middle of `rect`. One that is not `enabled` is drawn faint
/// and takes no clicks. One less tall than [`LEAST_CLICK`] takes clicks from that much room.
pub fn flat_button(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    words: &str,
    font: FontId,
    look: Flat,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let reach = ((LEAST_CLICK - rect.height()) / 2.0).max(0.0);
    let response = ui.interact(rect.expand2(vec2(0.0, reach)), id, sense);
    let corner = if rect.height() < 28.0 { RADIUS } else { 9.0 };
    let painter = ui.painter();
    if enabled {
        paint_flat(painter, rect, corner, look, response.hovered());
    } else {
        painter.rect_filled(rect, corner, BUTTON.gamma_multiply(0.5));
    }
    let ink = match (enabled, look) {
        (false, _) => TEXT_FAINT,
        (true, Flat::Main) => PANEL,
        (true, _) => TEXT,
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, words, font, ink);
    response
}

/// A flat key that is only an icon, as a card's start and reset keys are: the button grey,
/// lighter under the pointer, with the icon `glyph` wide in the middle.
pub fn icon_key(ui: &egui::Ui, rect: Rect, id: egui::Id, icon: Icon, glyph: f32, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);
    let corner = if rect.height() > 30.0 { 9.0 } else { RADIUS };
    paint_flat(ui.painter(), rect, corner, Flat::Plain, enabled && response.hovered());
    let ink = if enabled { TEXT } else { TEXT_FAINT };
    icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), egui::Vec2::splat(glyph)), ink);
    response
}

/// A quiet button that is only an icon, as ⋯ and + are: no block of its own, its icon faded
/// until the pointer is over it. `hover_fill` is drawn under it then, when given.
pub fn quiet_icon(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    icon: Icon,
    glyph: f32,
    faded: f32,
    hover_fill: Option<Color32>,
) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let hovered = response.hovered() || response.is_pointer_button_down_on();
    if let Some(fill) = hover_fill.filter(|_| hovered) {
        ui.painter().rect_filled(rect, RADIUS, fill);
    }
    let ink = if hovered { TEXT } else { TEXT.gamma_multiply(faded) };
    icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), egui::Vec2::splat(glyph)), ink);
    response
}

/// Sets the whole window's colours, sizes and font.
pub fn apply_style(ctx: &egui::Context) {
    load_fonts(ctx);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_corner_radius = CornerRadius::same(RADIUS_CARD as u8);
    visuals.menu_corner_radius = CornerRadius::same(RADIUS_CARD as u8);
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = CARD;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, TEXT);
    visuals.hyperlink_color = ACCENT;
    let radius = CornerRadius::same(RADIUS as u8);
    // Buttons, menus and fields are lighter than the panels and outlined, so they stand out.
    for (widget, fill, edge) in [
        (&mut visuals.widgets.noninteractive, PANEL, Color32::TRANSPARENT),
        (&mut visuals.widgets.inactive, BUTTON, BUTTON_EDGE),
        (&mut visuals.widgets.hovered, BUTTON_HOVER, BUTTON_EDGE_HOVER),
        (&mut visuals.widgets.active, BUTTON_HOVER, BUTTON_EDGE_HOVER),
        (&mut visuals.widgets.open, BUTTON_HOVER, BUTTON_EDGE_HOVER),
    ] {
        widget.corner_radius = radius;
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1.0, edge);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
        // Nothing grows or moves under the pointer or when pressed.
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        use egui::TextStyle::{Body, Button, Heading, Monospace, Small};
        style.text_styles.insert(Body, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(Button, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(Small, FontId::new(11.5, FontFamily::Proportional));
        style.text_styles.insert(Heading, FontId::new(18.0, FontFamily::Proportional));
        style.text_styles.insert(Monospace, FontId::new(12.5, FontFamily::Monospace));
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.interact_size.y = 26.0;
        // No scroll bars anywhere, egui's own lists and menus included: everything scrolls by
        // the trackpad, the wheel or the keys, and no strip at the edge takes room or clicks.
        style.spacing.scroll = egui::style::ScrollStyle {
            floating: true,
            bar_width: 0.0,
            floating_width: 0.0,
            floating_allocated_width: 0.0,
            bar_inner_margin: 0.0,
            bar_outer_margin: 0.0,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: 0.0,
            active_handle_opacity: 0.0,
            interact_handle_opacity: 0.0,
            ..egui::style::ScrollStyle::floating()
        };
    });
}

/// The name of the heavier cut of the interface font, for headings and buttons.
const SEMIBOLD: &str = "semibold";

/// The fixed-width face at a size.
pub fn fixed(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// The name of the heavier cut of the fixed-width face, for small capital labels.
const FIXED_SEMIBOLD: &str = "fixed semibold";

/// The heavier cut of the fixed-width face at a size (the plain cut where the computer's
/// fixed-width font has only one).
pub fn fixed_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FIXED_SEMIBOLD.into()))
}

/// The heavier cut of the interface font at a size.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// The name of the face a page of the Bible is read in.
const READING: &str = "reading";

/// The face a page of the Bible is read in, at a size: a book face with serifs where the
/// computer has one (Georgia, Times New Roman), otherwise the interface font.
pub fn reading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(READING.into()))
}

/// Uses the computer's own interface font, so the app looks at home on it, with a
/// wide-coverage font behind it so song titles in any script show as letters. Without either
/// the app still works with its built-in font.
fn load_fonts(ctx: &egui::Context) {
    use egui::epaint::text::VariationCoords;
    // Each is the system's interface font where it exists, then a common one, with the file
    // that holds its heavier cut.
    const INTERFACE: [(&str, &str); 4] = [
        ("/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/SFNS.ttf"),
        ("/System/Library/Fonts/HelveticaNeue.ttc", "/System/Library/Fonts/HelveticaNeue.ttc"),
        ("C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\seguisb.ttf"),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
    ];
    // The fixed-width face, for numbers, clocks, slide numbers, key caps and small capital labels.
    const FIXED: [&str; 5] = [
        "/System/Library/Fonts/SFNSMono.ttf",
        "/System/Library/Fonts/Menlo.ttc",
        "C:\\Windows\\Fonts\\consola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    ];
    const WIDE: [&str; 2] =
        ["/System/Library/Fonts/Supplemental/Arial Unicode.ttf", "C:\\Windows\\Fonts\\arialuni.ttf"];
    // The book face the Bible page is read in; each has Cyrillic as well as Latin letters.
    const SERIF: [&str; 5] = [
        "/System/Library/Fonts/Supplemental/Georgia.ttf",
        "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
        "C:\\Windows\\Fonts\\georgia.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
    ];
    // Apple's system font is one file that can be drawn at any weight, and in a cut made for
    // small sizes or one made for headlines. Left alone it uses the headline cut, which looks
    // cramped and thin at the size of a label; ask for the small-text cut, a touch heavier
    // because light text on dark looks thinner than it is. Fonts without these settings
    // ignore them.
    let cut = |weight: f32| egui::FontTweak {
        coords: VariationCoords::new([(b"opsz", 17.0), (b"wght", weight)]),
        ..egui::FontTweak::default()
    };
    let mut fonts = egui::FontDefinitions::default();
    let mut regular_first = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut heavy_first = regular_first.clone();
    let found =
        INTERFACE.iter().find_map(|(regular, heavy)| Some((std::fs::read(regular).ok()?, std::fs::read(heavy).ok())));
    if let Some((regular, heavy)) = found {
        let heavy = heavy.unwrap_or_else(|| regular.clone());
        fonts.font_data.insert("interface".to_owned(), Arc::new(egui::FontData::from_owned(regular).tweak(cut(440.0))));
        fonts
            .font_data
            .insert("interface heavy".to_owned(), Arc::new(egui::FontData::from_owned(heavy).tweak(cut(620.0))));
        regular_first.insert(0, "interface".to_owned());
        heavy_first.insert(0, "interface heavy".to_owned());
    }
    fonts.families.insert(FontFamily::Proportional, regular_first);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), heavy_first);
    // The heavier cut of the fixed-width face is the same file asked for at a heavier weight;
    // a font that cannot be drawn heavier is drawn as it is.
    let mut fixed_heavy = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    if let Some(bytes) = FIXED.iter().find_map(|path| std::fs::read(path).ok()) {
        let heavy = egui::FontTweak { coords: VariationCoords::new([(b"wght", 600.0)]), ..egui::FontTweak::default() };
        fonts.font_data.insert("fixed".to_owned(), Arc::new(egui::FontData::from_owned(bytes.clone())));
        fonts.font_data.insert("fixed heavy".to_owned(), Arc::new(egui::FontData::from_owned(bytes).tweak(heavy)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "fixed".to_owned());
        fixed_heavy.insert(0, "fixed heavy".to_owned());
    }
    fonts.families.insert(FontFamily::Name(FIXED_SEMIBOLD.into()), fixed_heavy);
    // Without a book face, the page is read in the interface font.
    let mut book = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some(bytes) = SERIF.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts.font_data.insert(READING.to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        book.insert(0, READING.to_owned());
    }
    fonts.families.insert(FontFamily::Name(READING.into()), book);
    if let Some(bytes) = WIDE.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts.font_data.insert("wide".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in fonts.families.values_mut() {
            family.push("wide".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// How a line of words is set where it is shown: its face (the heavier cut, or the fixed-width
/// face, where it uses them), its colour, the room added between its letters (as [`spaced`]
/// adds it) and whether it is shown in capitals (as [`cap`] shows it). A field that retypes the
/// words sets them the same way.
#[derive(Clone)]
pub struct Lettering {
    pub font: FontId,
    pub ink: Color32,
    pub spacing: f32,
    pub capitals: bool,
}

impl Lettering {
    /// Words in a face and a colour, nothing more.
    pub fn plain(font: FontId, ink: Color32) -> Lettering {
        Lettering { font, ink, spacing: 0.0, capitals: false }
    }

    /// Words set as [`cap`] sets them.
    pub fn cap(ink: Color32) -> Lettering {
        Lettering { font: fixed_semibold(11.5), ink, spacing: 0.6, capitals: true }
    }

    /// The words on one line, as they are shown. In capitals each letter stands for one letter
    /// typed (a letter whose capital is two, such as ß, stays as it is), so the cursor keeps
    /// its place.
    fn lay_out(&self, ui: &egui::Ui, words: &str) -> Arc<egui::Galley> {
        let shown = if self.capitals {
            words
                .chars()
                .map(|letter| {
                    let mut capital = letter.to_uppercase();
                    match (capital.next(), capital.next()) {
                        (Some(one), None) => one,
                        _ => letter,
                    }
                })
                .collect()
        } else {
            words.to_owned()
        };
        let format = egui::TextFormat {
            font_id: self.font.clone(),
            color: self.ink,
            extra_letter_spacing: self.spacing,
            ..Default::default()
        };
        let mut job = egui::text::LayoutJob::single_section(shown, format);
        job.break_on_newline = false;
        // A space typed at the end is shown, so the cursor moves when one is typed.
        job.keep_trailing_whitespace = true;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    }
}

/// Retypes a name where it is shown. The words stay as they were drawn, set as `look` says, on
/// the same pixels: no box, no other padding; only the thin inner edge a field has while it
/// takes the keyboard, drawn round them, and the lighter grey of a selection. `start` is where
/// the words begin, at the middle of their line; the field reaches `right`, and words longer
/// than that scroll inside it. It takes no room in the layout, so nothing around it moves.
pub fn rename_field_at(
    ui: &mut egui::Ui,
    text: &mut String,
    look: &Lettering,
    start: egui::Pos2,
    right: f32,
) -> egui::Response {
    // At least as tall as the words laid out (which the name drawn is centred on) and a line of
    // the face, as a field is, and centred on the name's line; the words are centred in it, so
    // they stay where they were. The toolkit puts a widget's edges on its grid, so the room
    // reaches out to the grid line round the words rather than being moved onto it.
    let line = ui.fonts_mut(|fonts| fonts.row_height(&look.font));
    let tall = look.lay_out(ui, text).size().y.max(line);
    let grid = egui::emath::GUI_ROUNDING;
    let top = ((start.y - tall / 2.0) / grid).floor() * grid;
    let words = Rect::from_min_max(pos2(start.x, top), pos2(right.max(start.x), 2.0 * start.y - top));
    // The edge goes round the words, in room of its own outside them.
    let frame = Frame::new()
        .inner_margin(egui::Margin::symmetric(3, 1))
        .stroke(Stroke::new(1.5, TEXT_FAINT))
        .corner_radius(RADIUS as u8);
    let layout = Layout::left_to_right(Align::Center).with_main_justify(true).with_cross_justify(true);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(words + frame.total_margin()).layout(layout));
    // The theme gives fields a least height taller than a line of words; not here.
    child.spacing_mut().interact_size = egui::Vec2::ZERO;
    // The cursor in the words' own colour, not the toolkit's blue; a selection the lighter grey
    // wherever the field is.
    child.visuals_mut().text_cursor.stroke.color = look.ink;
    child.visuals_mut().selection.bg_fill = SELECTED;
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| look.lay_out(ui, text.as_str());
    child.add(
        egui::TextEdit::singleline(text)
            .font(look.font.clone())
            .frame(frame)
            .layouter(&mut layouter)
            .text_color(look.ink)
            .align(Align2::LEFT_CENTER)
            .desired_width(f32::INFINITY),
    )
}

/// A small heading in capitals, as each section starts with.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(semibold(10.5)).color(TEXT_FAINT).extra_letter_spacing(0.8));
}

/// A button drawn as a rounded block with its words in the middle. `emphasis` picks its
/// colours.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Plain,
    /// Nothing to do: drawn faint, and not clickable.
    Idle,
}

pub fn block_button(ui: &mut egui::Ui, size: egui::Vec2, text: &str, key: &str, emphasis: Emphasis) -> egui::Response {
    let sense = if emphasis == Emphasis::Idle { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = response.hovered() && emphasis != Emphasis::Idle;
    let painter = ui.painter();
    let ink = match emphasis {
        Emphasis::Plain => {
            paint_button(painter, rect, hovered);
            TEXT
        }
        Emphasis::Idle => {
            painter.rect_filled(rect, RADIUS, CARD.gamma_multiply(0.7));
            TEXT_FAINT
        }
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, text, semibold(13.5), ink);
    if !key.is_empty() {
        // The key that does the same thing, small at the right.
        painter.text(
            rect.right_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            key,
            FontId::proportional(10.5),
            ink.gamma_multiply(0.7),
        );
    }
    response
}

/// A bar under the toolbar, on every view, while a song or theme changed outside SoftPresenter
/// waits on a choice: Reload takes the file's version, Keep mine writes SoftPresenter's over it.
pub fn changed_outside_bar(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    let Some(&first) = state.waiting.first() else { return };
    let name = match first {
        crate::outside::Waiting::Song(index) => show
            .presentations()
            .get(index)
            .map_or_else(|| "A song".to_owned(), |song| format!("\u{201c}{}\u{201d}", song.title)),
        crate::outside::Waiting::Theme(index) => state
            .theme_names
            .get(index)
            .map_or_else(|| "A theme".to_owned(), |theme| format!("The theme \u{201c}{theme}\u{201d}")),
    };
    let more = state.waiting.len() - 1;
    let frame = Frame::new().fill(Color32::from_rgb(74, 52, 26)).inner_margin(egui::Margin::symmetric(14, 7));
    egui::Panel::top("changed outside").resizable(false).frame(frame).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mut words = format!("{name} was changed outside SoftPresenter.");
            if more > 0 {
                words.push_str(&format!(" ({more} more after it.)"));
            }
            ui.label(egui::RichText::new(words).size(13.0).color(Color32::from_rgb(255, 214, 160)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Keep mine").on_hover_text("Save SoftPresenter's version over the file").clicked() {
                    state.waiting_choice = Some((first, false));
                }
                if ui
                    .button("Reload")
                    .on_hover_text("Take the file's version (SoftPresenter's changes to it go)")
                    .clicked()
                {
                    state.waiting_choice = Some((first, true));
                }
            });
        });
    });
}

/// The find box in the bar along the top: its id, so that Find can hand it the keyboard and a
/// search that ends can take the keyboard away again.
/// How long a notice said with `HomeState::say` stays, in seconds.
const TOAST_FOR: f64 = 2.4;

/// A notice that comes and goes: a dark grey pill with a thin edge, under the top bar in the
/// middle, never over the media strip. The window is drawn once more when it is to go, and
/// for nothing else on its account.
pub fn toast(ctx: &egui::Context, state: &mut HomeState) {
    let Some((words, since)) = state.toast.as_mut() else { return };
    let now = ctx.input(|input| input.time);
    let since = *since.get_or_insert(now);
    let left = TOAST_FOR - (now - since);
    if left <= 0.0 {
        state.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("toast")));
    let screen = ctx.content_rect();
    let galley = painter.layout(words.clone(), semibold(13.0), TEXT, (screen.width() - 80.0).max(200.0));
    let size = galley.size() + vec2(32.0, 18.0);
    let rect = Rect::from_min_size(pos2(screen.center().x - size.x / 2.0, TOP_BAR + 10.0), size);
    let corner = (size.y / 2.0).min(18.0);
    painter.rect_filled(rect.expand(8.0).translate(vec2(0.0, 6.0)), corner + 8.0, Color32::from_black_alpha(70));
    painter.rect_filled(rect, corner, Color32::from_rgb(0x2e, 0x2e, 0x2e));
    painter.rect_stroke(rect, corner, Stroke::new(1.0, Color32::from_rgb(0x4a, 0x4a, 0x4a)), StrokeKind::Inside);
    painter.galley(rect.min + vec2(16.0, 9.0), galley, TEXT);
}

pub fn find_box_id() -> egui::Id {
    egui::Id::unique("find box")
}

/// One of the two keys that switch the screens on and off: a light, the name, and ON or OFF
/// (which keeps the room of the longer word, so the key is always as wide). Its right edge
/// goes at `right`. Returns the key's response and its left edge.
fn screens_key(ui: &egui::Ui, bar: Rect, right: f32, name: &str, on: bool) -> (egui::Response, f32) {
    const PAD: f32 = 12.0;
    const BETWEEN: f32 = 8.0;
    let font = FontId::proportional(13.0);
    let state_font = fixed_semibold(11.0);
    let name_wide = ui.painter().layout_no_wrap(name.to_owned(), font.clone(), TEXT).size().x;
    let state_wide = spaced(ui, "OFF", state_font.clone(), TEXT, 0.6).size().x;
    let wide = PAD + 7.0 + BETWEEN + name_wide + BETWEEN + state_wide + PAD;
    let rect = Rect::from_min_size(pos2(right - wide, bar.center().y - 16.0), vec2(wide, 32.0));
    let response = ui.interact(rect, ui.scope_id().with(("screens key", name)), Sense::click());
    let fill = match (on, response.hovered()) {
        (true, false) => SCREENS_ON,
        (true, true) => SCREENS_ON_HOVER,
        (false, false) => BUTTON,
        (false, true) => BUTTON_HOVER,
    };
    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS_CARD, fill);
    let y = rect.center().y;
    paint_light(painter, pos2(rect.min.x + PAD + 3.5, y), on.then_some(READY));
    painter.text(pos2(rect.min.x + PAD + 7.0 + BETWEEN, y), Align2::LEFT_CENTER, name, font, TEXT);
    let (word, ink) = if on { ("ON", READY) } else { ("OFF", TEXT_FAINT) };
    paint_spaced(ui, pos2(rect.max.x - PAD - state_wide, y), Align2::LEFT_CENTER, word, state_font, ink, 0.6);
    (response, rect.min.x)
}

/// The bar along the top, lying on the window itself: the mark and the name; the switch
/// between the views; the find box; and at the right the clock, the Audience and Stage keys,
/// the keys' button and Settings.
pub fn toolbar(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let panel = egui::Panel::top("toolbar").resizable(false).show_separator_line(false).exact_size(TOP_BAR);
    panel.frame(Frame::new().fill(BG)).show(ui, |ui| {
        let bar = ui.max_rect();
        ui.allocate_rect(bar, Sense::hover());
        let middle = bar.center().y;

        // The mark, a light slab with the orange light in its corner, and the name.
        let mark = Rect::from_min_size(pos2(bar.min.x + 14.0, middle - 7.0), vec2(20.0, 14.0));
        ui.painter().rect_filled(mark, 4.0, TEXT);
        ui.painter().circle_filled(pos2(mark.max.x - 5.0, mark.min.y + 5.0), 2.0, TALLY);
        let name = ui.painter().text(
            pos2(mark.max.x + 9.0, middle),
            Align2::LEFT_CENTER,
            "SoftPresenter",
            semibold(15.5),
            TEXT,
        );

        // The views. They are numbered as the View menu and the command line number them (Show,
        // Edit, Bible, and Themes, which is a kind of the Edit view), and stand in the switch
        // as Show, Bible, Edit.
        const ORDER: [usize; 3] = [0, 2, 1];
        use crate::home::EditKind;
        // A slide, a theme or a prop opened some other way takes the stage layouts' place.
        let other_draft = state.editing.as_ref().is_some_and(|draft| !crate::stage_editor::is_stage(draft));
        if other_draft || state.theme_edit.is_some() {
            state.stage_editor_open = false;
        }
        // A prop made or opened, or another kind, takes the empty props list's place.
        if state.editing.is_some() || state.theme_edit.is_some() || state.stage_editor_open {
            state.props_editor_open = false;
        }
        let kind = state.edit_kind();
        let view = if kind.is_some() {
            1
        } else if state.bible_open {
            2
        } else {
            0
        };
        let hints = [
            crate::keys::hint("Show", "1", false),
            format!(
                "{}; {} too",
                crate::keys::hint("Bible, ready for a reference", "2", false),
                crate::keys::hint_key("B")
            ),
            crate::keys::hint("Edit", "3", false),
        ];
        let hints: Vec<&str> = hints.iter().map(String::as_str).collect();
        let at = pos2(name.max.x + 14.0, middle - 16.45);
        let (switch, clicked) = pill_switch(
            ui,
            "views",
            at,
            &["Show", "Bible", "Edit"],
            &hints,
            ORDER.iter().position(|shown| *shown == view),
            &SwitchLook::views(),
        );
        // With Show Keys on Buttons: each view's key beside it (Command-1, 2 and 3; Ctrl
        // elsewhere).
        if crate::keys::tags_shown(state) {
            let cells = pill_cells(ui, at, &["Show", "Bible", "Edit"], &SwitchLook::views());
            for (cell, key) in cells.iter().zip(["1", "2", "3"]) {
                crate::keys::corner_tag(ui.painter(), *cell, &crate::keys::with_modifier(key));
            }
        }
        // A click on the view that is open does nothing. Asked for by name (the View menu, the
        // command line), Edit is the slide editor and Themes the Theme Editor, whichever kind
        // of the Edit view is open.
        let clicked = clicked.map(|place| ORDER[place]).filter(|to| *to != view);
        match clicked.or(state.view_request.take()) {
            Some(1) if kind != Some(EditKind::Slides) => state.edit_kind_request = Some(EditKind::Slides),
            Some(3) if kind != Some(EditKind::Themes) => state.edit_kind_request = Some(EditKind::Themes),
            Some(to @ (0 | 2)) if to != view || kind.is_some() => {
                state.edit_kind_request = None;
                // Leaving the Theme Editor goes through the app, which saves the theme.
                match state.theme_edit.as_mut() {
                    Some(edit) => edit.leave = true,
                    None => {
                        state.editing = None;
                        state.stage_editor_open = false;
                        state.props_editor_open = false;
                    }
                }
                state.bible_open = to == 2;
            }
            _ => {}
        }
        open_edit_kind(show, state);

        // From the right: Settings, the keys' button, Stage, Audience and the clock.
        let gear = Rect::from_min_size(pos2(bar.max.x - 14.0 - 30.0, middle - 15.0), vec2(30.0, 30.0));
        let response = ui.interact(gear, ui.scope_id().with("settings"), Sense::click());
        if state.preferences_open {
            ui.painter().rect_filled(gear, RADIUS, SELECTED);
        } else if response.hovered() {
            ui.painter().rect_filled(gear, RADIUS, CARD);
        }
        icons::paint(ui.painter(), Icon::Gear, Rect::from_center_size(gear.center(), vec2(16.0, 16.0)), TEXT_DIM);
        let hint = if cfg!(target_os = "macos") { "Settings (Command-comma)" } else { "Settings (Ctrl-comma)" };
        if response.on_hover_text(hint).clicked() {
            state.preferences_open = !state.preferences_open;
        }

        // The keys: the list of every key, as `?` opens it.
        let keys = pos2(gear.min.x - 10.0 - 14.0, middle);
        let response = ui.interact(
            Rect::from_center_size(keys, egui::Vec2::splat(28.0)),
            ui.scope_id().with("keys"),
            Sense::click(),
        );
        ui.painter().circle_filled(keys, 14.0, if response.hovered() { HOVER } else { CARD });
        ui.painter().text(keys, Align2::CENTER_CENTER, "?", fixed_semibold(13.0), TEXT_DIM);
        if response.on_hover_text("Every key (press ?)").clicked() {
            state.key_sheet = true;
        }

        let stage_on = state.stage_on;
        let (response, left) = screens_key(ui, bar, keys.x - 14.0 - 10.0, "Stage", stage_on);
        if crate::keys::tags_shown(state) {
            crate::keys::corner_tag(ui.painter(), response.rect, &crate::keys::stage_key());
        }
        if response.on_hover_text(crate::keys::hint("Stage screens on or off", "2", true)).clicked() {
            state.stage_on = !stage_on;
        }
        let on = show.output_enabled();
        let (response, left) = screens_key(ui, bar, left - 10.0, "Audience", on);
        if crate::keys::tags_shown(state) {
            crate::keys::corner_tag(ui.painter(), response.rect, &crate::keys::audience_key());
        }
        if response.on_hover_text(crate::keys::hint("Audience screens on or off", "1", true)).clicked() {
            commands.push(Command::SetOutputEnabled(!on));
        }

        // The clock: the time of day to the second, in a dark window of its own, on a 12-hour or
        // a 24-hour clock as the computer is set, the figures alone (no AM or PM, as the design
        // has it). Its box is as wide as its widest time (two figures for the hour), so nothing
        // moves as time passes. The window is drawn again when the next second starts, and for
        // nothing else on its account.
        let now = chrono::Local::now();
        let (hour, minute, second) =
            (chrono::Timelike::hour(&now), chrono::Timelike::minute(&now), chrono::Timelike::second(&now));
        let in_front = ui.input(|input| input.focused);
        let form = state.clock.form(in_front);
        let font = fixed(14.0);
        let figures = spaced(ui, &form.figures(hour, minute, second), font.clone(), TEXT, 0.6);
        let figures_wide = spaced(ui, &form.figures(0, 0, 0).replace('1', "0"), font.clone(), TEXT, 0.6)
            .size()
            .x
            .max(spaced(ui, &form.figures(10, 0, 0).replace('1', "0"), font, TEXT, 0.6).size().x);
        let wide = figures_wide + 24.0;
        let clock = Rect::from_min_size(pos2(left - 10.0 - wide, middle - 14.5), vec2(wide, 29.0));
        ui.painter().rect_filled(clock, RADIUS, Color32::BLACK);
        ui.painter().rect_stroke(clock, RADIUS, Stroke::new(1.0, LINE), StrokeKind::Inside);
        // The figures end at one place, so the seconds stay where they are when the hour gains
        // or loses a figure.
        let figures_end = clock.max.x - 12.0;
        let at = pos2(figures_end - figures.size().x, middle - figures.size().y / 2.0);
        ui.painter().galley(at, figures, TEXT);
        let into_second = u64::from(chrono::Timelike::nanosecond(&now) % 1_000_000_000);
        ui.ctx().request_repaint_after(std::time::Duration::from_nanos(1_000_000_000 - into_second));

        // The find box takes the room between the views and the clock, up to its full width.
        let room = clock.min.x - 14.0 - (switch.max.x + 14.0);
        let mut free_from = switch.max.x + 14.0;
        if room >= 140.0 {
            let find = Rect::from_min_size(pos2(switch.max.x + 14.0, middle - 17.0), vec2(room.min(300.0), 34.0));
            find_box(ui, state, find);
            free_from = find.max.x + 12.0;
        } else {
            state.find_rect = None;
        }
        // A running timer, just before the clock: it takes only room nobody else uses, so
        // nothing moves when one starts or stops, and none is kept for it while none runs.
        if let Some(timer) = state.timers.iter().find(|timer| timer.running()) {
            let words =
                format!("{} {}", timer.name, timer.text(std::time::Instant::now(), crate::timers::seconds_into_day()));
            let right = clock.min.x - 10.0;
            let wide = (right - free_from).min(150.0);
            if wide >= 48.0 {
                let room = Rect::from_min_max(pos2(right - wide, bar.min.y), pos2(right, bar.max.y));
                let galley = spaced(ui, &words.to_uppercase(), fixed(12.0), READY, 0.6);
                let at = pos2((right - galley.size().x).max(room.min.x), middle - galley.size().y / 2.0);
                let shown = ui.interact(room, ui.scope_id().with("running timer"), Sense::hover());
                ui.painter().with_clip_rect(room).galley(at, galley, READY);
                shown.on_hover_text(format!("{words} (the Timers tab: {})", crate::keys::with_modifier("1")));
            }
        } else if let Some((words, failed)) = state.canva.quiet.clone() {
            // Otherwise, in the same room, what Canva Sync is doing (an export, a download), a
            // video just put in place, or one that failed. A click opens its page.
            let right = clock.min.x - 10.0;
            let wide = (right - free_from).min(220.0);
            if wide >= 60.0 {
                let room = Rect::from_min_max(pos2(right - wide, bar.min.y), pos2(right, bar.max.y));
                let colour = if failed { TALLY } else { TEXT_DIM };
                let galley = spaced(ui, &words.to_uppercase(), fixed(11.0), colour, 0.5);
                let at = pos2((right - galley.size().x).max(room.min.x), middle - galley.size().y / 2.0);
                let shown = ui.interact(room, ui.scope_id().with("canva sync line"), Sense::click());
                ui.painter().with_clip_rect(room).galley(at, galley, colour);
                let detail = state.canva.said.as_ref().map_or_else(|| words.clone(), |(said, _)| said.clone());
                if shown.on_hover_text(format!("{detail} (Settings › Integrations › Canva Sync)")).clicked() {
                    state.preferences_open = true;
                    state.preferences_pane =
                        crate::home::PANES.iter().position(|pane| *pane == "Integrations").unwrap_or(0);
                    state.canva.open = true;
                }
            }
        }
    });
}

/// Carries out a request for one of the Edit view's kinds (from the switch at the top of its
/// left column, the View menu, the command line, the Stage tab or a testing switch). The Theme
/// Editor is left through the app, which saves the theme, so a request made while it is open
/// waits a frame for that. With no song, a word says why; with no prop, the props' list opens
/// empty, with Add.
fn open_edit_kind(show: &Show, state: &mut HomeState) {
    use crate::home::EditKind;
    let Some(kind) = state.edit_kind_request else { return };
    let now = state.edit_kind();
    if now == Some(kind) {
        state.edit_kind_request = None;
        return;
    }
    if let Some(edit) = state.theme_edit.as_mut() {
        edit.leave = true;
        return;
    }
    state.edit_kind_request = None;
    state.props_editor_open = false;
    match kind {
        // Never nothing: a song that can be edited (see `song_to_edit`), or a word on why not.
        EditKind::Slides => match song_to_edit(show, state).and_then(|at| EditDraft::of(show, at)) {
            Some(draft) => state.editing = Some(draft),
            None => state.say(format!(
                "There is no song to edit yet. Make one with New Song ({}), or import some.",
                crate::keys::hint_key("N")
            )),
        },
        EditKind::Themes => state.theme_request = Some(crate::home::ThemeRequest::Edit(state.theme_for_songs)),
        // With no props yet, their list, empty, with Add.
        EditKind::Props => match prop_draft(state, 0) {
            Some(draft) => state.editing = Some(draft),
            None => {
                state.editing = None;
                state.stage_editor_open = false;
                state.props_editor_open = true;
            }
        },
        // The open layout's draft is made by the Edit view while this says it is open.
        EditKind::Stage => {
            state.editing = None;
            state.stage_editor_open = true;
        }
    }
}

/// A prop opened in the editor, as a slide of its own.
pub fn prop_draft(state: &HomeState, index: usize) -> Option<EditDraft> {
    let mut draft = EditDraft::of_slide(state.props.get(index)?.slide.clone(), index);
    draft.at.presentation = crate::props::PROP_SLOT;
    Some(draft)
}

/// The switch at the top of the Edit view's left column: what is being edited, slides, themes,
/// props or stage layouts. It takes the column's width, less 10 either side, 12 down.
pub fn edit_kinds(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::home::EditKind;
    const KINDS: [EditKind; 4] = [EditKind::Slides, EditKind::Themes, EditKind::Props, EditKind::Stage];
    let hints: [&str; 4] = [
        "Edit a song's slides",
        "Edit the themes and their templates",
        "Make and edit the props",
        "Edit the stage screens' layouts",
    ];
    let tabs = Tabs {
        labels: &["Slides", "Themes", "Props", "Stage"],
        hints: &hints,
        dots: &[],
        tags: &[],
        off: &[false; 4],
        tall: 23.5,
        size: 11.5,
    };
    let width = ui.available_width() - 20.0;
    let (place, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), 12.0 + tab_switch_height(&tabs)), Sense::hover());
    let open = state.edit_kind().and_then(|kind| KINDS.iter().position(|each| *each == kind));
    let (_, clicked) = tab_switch(ui, "edit kinds", place.min + vec2(10.0, 12.0), width, &tabs, open);
    if let Some(clicked) = clicked.filter(|clicked| Some(*clicked) != open) {
        state.edit_kind_request = Some(KINDS[clicked]);
    }
}

// Every kind of the Edit view lists what it edits in the left column the same way: what it is
// picked from, when there is a choice (the songs), as plain rows with an icon; a heading in
// small capitals, a row of keys (Add, Duplicate, Delete, ↑, ↓) that act on the one open, then
// each one as a picture with a strip under it (its number and name), the one open on the well
// with a ring round it, dragged to another place as slides are. A picture's other choices are on
// its right-click menu. Its caller draws the picture itself, and the menu. Delete with nothing
// picked on the canvas is the list's Delete key.

/// The heading of a list in the Edit view's left column, in small capitals: 14 under what is
/// above it and 8 in, with 8 under it.
pub fn list_heading(ui: &mut egui::Ui, text: &str) {
    const ABOVE: f32 = 14.0;
    const TALL: f32 = 15.5;
    let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), ABOVE + TALL + 8.0), Sense::hover());
    cap(ui, pos2(place.min.x + 8.0, place.min.y + ABOVE + TALL / 2.0), Align2::LEFT_CENTER, text, TEXT_DIM);
}

/// A list's heading with a + at its right, as THEME has: `hint` says what the + makes.
/// Returns the +.
pub fn list_heading_with_add(ui: &mut egui::Ui, text: &str, hint: &str) -> egui::Response {
    let top = ui.cursor().min.y;
    list_heading(ui, text);
    let middle = top + 14.0 + 15.5 / 2.0;
    let right = ui.max_rect().max.x - 8.0;
    let plus = Rect::from_center_size(pos2(right - ICON_BUTTON / 2.0, middle), egui::Vec2::splat(ICON_BUTTON));
    icon_button(ui, plus, ui.scope_id().with(("list heading add", text)), Icon::Plus, 0.6, false).on_hover_text(hint)
}

/// How tall a [`plain_row`] is.
pub const PLAIN_ROW: f32 = 34.0;

/// What a plain row of the Edit view's left column shows: an icon, a name (none while a field
/// retypes it in its place), and a small word in faint capitals at the right (MAIN).
pub struct PlainRow<'a> {
    pub icon: Icon,
    pub name: Option<&'a str>,
    pub tag: Option<&'a str>,
    /// The one open: the chosen grey, and its name heavier and brighter.
    pub on: bool,
}

/// Draws a plain row, 34 high, as songs and themes are listed above their pictures. Returns its
/// response, where its name starts (at the middle of its line) and how far right the name may
/// reach, for a field that retypes it.
pub fn plain_row(ui: &mut egui::Ui, look: &PlainRow) -> (egui::Response, egui::Pos2, f32) {
    plain_row_sensing(ui, look, Sense::click())
}

/// A [`plain_row`] that senses what `sense` says: clicks and drags for a row that can be dragged
/// to another place (the themes).
pub fn plain_row_sensing(ui: &mut egui::Ui, look: &PlainRow, sense: Sense) -> (egui::Response, egui::Pos2, f32) {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), PLAIN_ROW), sense);
    if look.on {
        ui.painter().rect_filled(row, RADIUS, CARD);
    } else if response.hovered() {
        ui.painter().rect_filled(row, RADIUS, ROW_HOVER);
    }
    let middle = row.center().y;
    let icon = Rect::from_min_size(pos2(row.min.x + 10.0, middle - 6.5), egui::Vec2::splat(13.0));
    icons::paint(ui.painter(), look.icon, icon, if look.on { TEXT_DIM } else { TEXT_FAINT });
    let mut right = row.max.x - 10.0;
    if let Some(tag) = look.tag {
        let drawn = paint_spaced(
            ui,
            pos2(right, middle),
            Align2::RIGHT_CENTER,
            &tag.to_uppercase(),
            fixed_semibold(10.5),
            TEXT_FAINT,
            0.5,
        );
        right = drawn.min.x - 8.0;
    }
    let start = pos2(row.min.x + 35.0, middle);
    if let Some(name) = look.name {
        let (font, ink) = if look.on { (semibold(13.5), TEXT) } else { (FontId::proportional(13.5), TEXT_DIM) };
        let clip = Rect::from_min_max(pos2(start.x, row.min.y), pos2(right.max(start.x), row.max.y));
        ui.painter().with_clip_rect(clip).text(start, Align2::LEFT_CENTER, name, font, ink);
    }
    (response, start, right)
}

/// One of the keys over a list of pictures in the Edit view's left column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKey {
    Add,
    Copy,
    Delete,
    Up,
    Down,
}

impl ListKey {
    /// The five, in the order they stand.
    pub const ALL: [ListKey; 5] = [ListKey::Add, ListKey::Copy, ListKey::Delete, ListKey::Up, ListKey::Down];

    fn words(self) -> &'static str {
        match self {
            ListKey::Add => "Add",
            ListKey::Copy => "Duplicate",
            ListKey::Delete => "Delete",
            ListKey::Up => "\u{2191}",
            ListKey::Down => "\u{2193}",
        }
    }
}

/// How much room the row of [`list_keys`] leaves under its keys. The list of pictures under it
/// starts with [`LIST_TOP`] of its own, room for the ring of the one open, which would be cut
/// off at the top of the list's scrolling area.
const LIST_KEYS_UNDER: f32 = 10.0;
pub const LIST_TOP: f32 = 2.0;

/// The row of keys under a list's heading: Add, Duplicate and Delete, then ↑ and ↓ to move the
/// one open, 30 high and 8 in from each side, 4 apart. The arrows are 26 wide; the three with
/// words share what is left, each as wide as its words with 3 to 6 either side, and any more
/// shared 1 : 1 : 1.2 (the left column is just wide enough for Duplicate). `can` says which of
/// the five can be pressed (in that order), `hints` what each does. Returns the one pressed.
pub fn list_keys(ui: &mut egui::Ui, salt: &str, can: [bool; 5], hints: [&str; 5]) -> Option<ListKey> {
    const TALL: f32 = 30.0;
    const ARROW: f32 = 26.0;
    const BETWEEN: f32 = 4.0;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TALL + LIST_KEYS_UNDER), Sense::hover());
    let font = FontId::proportional(12.5);
    let room = row.width() - 16.0 - 2.0 * ARROW - 4.0 * BETWEEN;
    let words = [ListKey::Add, ListKey::Copy, ListKey::Delete]
        .map(|key| ui.painter().layout_no_wrap(key.words().to_owned(), font.clone(), TEXT).size().x);
    let all_words: f32 = words.iter().sum();
    let pad = ((room - all_words) / 6.0).clamp(3.0, 6.0);
    let spare = (room - all_words - 6.0 * pad).max(0.0) / 3.2;
    let widths =
        [words[0] + 2.0 * pad + spare, words[1] + 2.0 * pad + spare, words[2] + 2.0 * pad + 1.2 * spare, ARROW, ARROW];
    let mut left = row.min.x + 8.0;
    let mut pressed = None;
    for (at, key) in ListKey::ALL.into_iter().enumerate() {
        let rect = Rect::from_min_size(pos2(left, row.min.y), vec2(widths[at], TALL));
        let id = ui.scope_id().with((salt, at));
        let response = flat_button(ui, rect, id, key.words(), font.clone(), Flat::Plain, can[at]);
        if response.on_hover_text(hints[at]).clicked() {
            pressed = Some(key);
        }
        left = rect.max.x + BETWEEN;
    }
    pressed
}

/// What one picture of a list in the Edit view's left column says in the strip under it.
pub struct ListPicture<'a> {
    /// Its place in the list, from 1, shown as two figures.
    pub number: usize,
    pub name: &'a str,
    /// A dot between the number and the name, in this colour (a slide's group).
    pub dot: Option<Color32>,
    /// The one open: on the well, with a ring round it.
    pub open: bool,
    /// On the screens now: its strip is orange.
    pub live: bool,
    /// Its name is being retyped: the caller puts a field where the name was (see
    /// [`ListPlaces`]), so the name isn't drawn.
    pub renaming: bool,
}

/// Where a picture of the list was drawn: the picture's room, for the caller to paint it in
/// (black, with corners of [`LIST_PICTURE_CORNER`]); and where its name starts, at the middle
/// of its line, and how far it may reach, for [`rename_field_at`] with [`list_name_look`].
pub struct ListPlaces {
    pub picture: Rect,
    pub name: egui::Pos2,
    pub name_right: f32,
}

/// The corners of a list's pictures.
pub const LIST_PICTURE_CORNER: f32 = 7.0;
/// The room round a list's picture, at its sides and top, inside the well of the one open.
const LIST_PICTURE_INSET: f32 = 5.0;
/// How tall the strip under a list's picture is.
const LIST_STRIP: f32 = 24.0;
/// Between one picture of the list and the next.
pub const LIST_BETWEEN: f32 = 6.0;

/// How a list picture's name is set: the fixed-width face, heavier, in capitals, with half a
/// point between the letters, as a slide tile's strip is set.
pub fn list_name_look(ink: Color32) -> Lettering {
    Lettering { font: fixed_semibold(12.0), ink, spacing: 0.5, capitals: true }
}

/// One picture of a list in the Edit view's left column, as wide as the room there is, 16 to 9
/// with a strip under it: its number, its dot if it has one, its name. `list` names the list,
/// and the picture is the `index`th of it. When another picture becomes the open one, the list
/// scrolls it into sight. Returns what the pointer did on it, and where its parts are; the
/// response's rect is the whole tile.
///
/// It takes clicks and drags. A list whose pictures can be dragged to another place moves them
/// as the playlist's items are moved, with [`drag_let_go`], [`drop_slot`], [`paint_rows_moving`]
/// and [`paint_carried`] over the tiles' rects (the slides of the Edit view do this). In a list
/// that doesn't, a drag does nothing.
pub fn list_picture(
    ui: &mut egui::Ui,
    list: egui::Id,
    index: usize,
    look: &ListPicture,
) -> (egui::Response, ListPlaces) {
    let width = ui.available_width();
    let inner = width - 2.0 * LIST_PICTURE_INSET;
    let size = vec2(inner, inner * 9.0 / 16.0);
    let (tile, _) = ui.allocate_exact_size(vec2(width, LIST_PICTURE_INSET + size.y + LIST_STRIP), Sense::hover());
    let response = ui.interact(tile, list.with(("picture", index)), Sense::click_and_drag());
    if look.open {
        ui.painter().rect_filled(tile, 9.0, CARD);
        ui.painter().rect_stroke(tile, 9.0, Stroke::new(2.0, SELECTED), StrokeKind::Outside);
    } else if response.hovered() {
        ui.painter().rect_filled(tile, 9.0, ROW_HOVER);
    }
    let picture = Rect::from_min_size(tile.min + vec2(LIST_PICTURE_INSET, LIST_PICTURE_INSET), size);
    let middle = picture.max.y + LIST_STRIP / 2.0;
    let ink = if look.live { TALLY } else { TEXT };
    let number = format!("{:02}", look.number);
    let drawn = paint_spaced(
        ui,
        pos2(picture.min.x + 3.0, middle),
        Align2::LEFT_CENTER,
        &number,
        fixed_semibold(12.0),
        ink,
        0.5,
    );
    let mut start = drawn.max.x + 7.0;
    if let Some(dot) = look.dot {
        ui.painter().circle_filled(pos2(start + 4.0, middle), 4.0, dot);
        start += 8.0 + 7.0;
    }
    let right = picture.max.x - 3.0;
    if !look.renaming {
        let words = list_name_look(ink).lay_out(ui, look.name);
        let room = Rect::from_min_max(pos2(start, picture.max.y), pos2(right, tile.max.y));
        ui.painter().with_clip_rect(room).galley(pos2(start, middle - words.size().y / 2.0), words, ink);
    }
    // The open one is brought into sight when it changes (added, copied, moved, opened).
    let shown = list.with("open");
    if look.open && ui.data(|data| data.get_temp::<usize>(shown)) != Some(index) {
        ui.data_mut(|data| data.insert_temp(shown, index));
        response.scroll_to_me(None);
    }
    (response, ListPlaces { picture, name: pos2(start, middle), name_right: right })
}

/// Asks for a picture of a slide; see [`ThumbOf`].
pub type Thumb<'t> = dyn FnMut(ThumbOf) -> Option<(TextureId, bool)> + 't;

/// A number that changes when any of `parts` does, to tell when a picture must be drawn again.
pub fn signature(parts: &[&str]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

/// A small picture of a slide drawn with a theme of its own, in `rect` with round corners: a
/// prop's, or a stage layout's. `key` keeps it apart from other pictures of the same slide;
/// it is drawn again only when `said` (a [`signature`] of what it shows) is not what it was
/// when it was last drawn, and is black until it is ready.
#[allow(clippy::too_many_arguments)]
pub fn own_picture(
    ui: &egui::Ui,
    thumb: &mut Thumb,
    rect: Rect,
    corner: f32,
    key: sp_model::Uuid,
    slide: &sp_model::Slide,
    theme: &sp_model::Theme,
    said: u64,
) {
    let drawn = egui::Id::unique(("own picture drawn", key));
    let again = ui.data(|data| data.get_temp::<u64>(drawn)) != Some(said);
    let kept = sp_model::Slide { id: key, ..slide.clone() };
    ui.painter().rect_filled(rect, corner, Color32::BLACK);
    match thumb(ThumbOf::Own(&kept, theme, again)) {
        Some((texture, _)) => {
            ui.data_mut(|data| data.insert_temp(drawn, said));
            egui::Image::from_texture((texture, rect.size())).corner_radius(corner).paint_at(ui, rect);
        }
        None => {
            // Forgotten already: the next frame draws it from fresh.
            ui.data_mut(|data| data.insert_temp(drawn, said));
            ui.ctx().request_repaint();
        }
    }
}

/// The find box: typing in it searches the song library (titles first, then the words of the
/// songs) and the Bibles in use (a reference gives its passage, words give verses), with what
/// it finds listed under it by `library_search` in `home.rs`. It holds the
/// search's own words, so Find in the menu, its shortcut and the box are one thing.
fn find_box(ui: &mut egui::Ui, state: &mut HomeState, find: Rect) {
    let id = find_box_id();
    let focused = ui.memory(|memory| memory.has_focus(id));
    ui.painter().rect_filled(find, 17.0, PANEL);
    let edge = if focused { SELECTED } else { LINE };
    ui.painter().rect_stroke(find, 17.0, Stroke::new(1.0, edge), StrokeKind::Inside);
    // While a search is open the box holds its words; otherwise it is empty.
    let mut none = String::new();
    let open = state.library_search.is_some();
    let text = state.library_search.as_mut().map_or(&mut none, |(words, _)| words);
    let inner =
        Rect::from_min_max(pos2(find.min.x + 16.0, find.min.y + 2.0), pos2(find.max.x - 44.0, find.max.y - 2.0));
    let field = egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(egui::RichText::new("Find a song, a line, or a verse").color(TEXT_FAINT))
        .font(FontId::proportional(13.0))
        .background_color(Color32::TRANSPARENT)
        .frame(Frame::new())
        .align(Align2::LEFT_CENTER)
        .desired_width(inner.width());
    let response = ui.put(inner, field);
    if open {
        // A search opened from the menu, by its shortcut or by a click types here.
        response.request_focus();
    } else if response.has_focus() {
        state.library_search = Some((none, 0));
    }
    // The key that finds, at the right end, until the box has the keyboard.
    if !focused && !open {
        let cap = Rect::from_min_size(pos2(find.max.x - 30.0, find.center().y - 10.0), vec2(20.0, 20.0));
        ui.painter().rect_filled(cap, 5.0, CARD);
        ui.painter().text(cap.center(), Align2::CENTER_CENTER, "/", fixed(11.0), TEXT_DIM);
    }
    let hint = if cfg!(target_os = "macos") {
        "Find a song by its title or a line of its words, or a Bible passage or verse (press /, or Command-F)"
    } else {
        "Find a song by its title or a line of its words, or a Bible passage or verse (press /, or Ctrl+F)"
    };
    response.on_hover_text(hint);
    state.find_rect = Some(find);
}

/// The tints of the playlist's and the library's marks in the left column; nothing else has
/// these colours.
const PLAYLIST_TINT: Color32 = Color32::from_rgb(0x8f, 0xb4, 0xff);
const LIBRARY_TINT: Color32 = Color32::from_rgb(0xc3, 0xa6, 0xff);
/// Only these two tints mark a kind, by where a thing lives: songs take the libraries' tint, as
/// songs live in libraries. The Bible and live captions take the ink, so a colour means one
/// thing and the mark's shape and the tag's word say what it is. All read at 11 points on the
/// panel and on a picked row (contrast 7:1 or more).
pub const SONG_TINT: Color32 = LIBRARY_TINT;

/// The mark and tint of a result in the find box: an open book in ink for the Bible, a note in
/// the songs' tint for a song (or a slide of one).
pub fn found_look(bible: bool) -> (Icon, Color32) {
    if bible { (Icon::Book, TEXT) } else { (Icon::Note, SONG_TINT) }
}
/// A row under the pointer: a little lighter than the panel, darker than a chosen row.
pub const ROW_HOVER: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
/// The side of a button that is only its icon, and of the "more" button at the end of a row.
pub const ICON_BUTTON: f32 = 28.0;
pub const MORE_BUTTON: f32 = 26.0;
/// How tall a row of the left column's list is, and a row of the chooser.
const LIST_ROW: f32 = 42.0;
const CHOOSER_ROW: f32 = 36.0;

/// A small capital label in the fixed-width face, as sections and strips are headed.
pub fn cap(ui: &egui::Ui, at: egui::Pos2, anchor: Align2, text: &str, ink: Color32) -> Rect {
    let look = Lettering::cap(ink);
    paint_spaced(ui, at, anchor, &text.to_uppercase(), look.font, ink, look.spacing)
}

/// A button that is only its icon: lighter under the pointer, and drawn at `strength` (1 for
/// full) until the pointer is on it. `on` gives it the lighter grey of a choice that is on.
pub fn icon_button(ui: &egui::Ui, rect: Rect, id: egui::Id, icon: Icon, strength: f32, on: bool) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let hot = response.hovered() || open;
    if on {
        ui.painter().rect_filled(rect, RADIUS, SELECTED);
    } else if hot {
        ui.painter().rect_filled(rect, RADIUS, BUTTON);
    }
    let ink = if hot || on { TEXT } else { TEXT_DIM.gamma_multiply(strength) };
    icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), egui::Vec2::splat(14.0)), ink);
    response
}

/// The "more" button at the end of a row, a strip or a bar: three dots, always there at half
/// strength and full when the pointer is `near` (over its row). It opens what a right-click
/// on the row opens.
pub fn more_button(ui: &egui::Ui, rect: Rect, id: egui::Id, near: bool) -> egui::Response {
    icon_button(ui, rect, id, Icon::More, if near { 1.0 } else { 0.5 }, false).on_hover_text("More")
}

/// The dot that says a thing is on the screens: orange, with a faint ring round it.
pub fn paint_tally(painter: &egui::Painter, centre: egui::Pos2) {
    painter.circle_filled(centre, 7.0, TALLY.gamma_multiply(0.25));
    painter.circle_filled(centre, 4.0, TALLY);
}

/// A flat button with its words in the middle and an icon before them if one is given. `on`
/// gives it the lighter grey of a choice that is on.
pub fn labelled_button(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    label: &str,
    icon: Option<Icon>,
    on: bool,
) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 9.0, SELECTED);
        painter.rect_stroke(rect, 9.0, Stroke::new(1.0, ON_EDGE), StrokeKind::Inside);
    } else {
        painter.rect_filled(rect, 9.0, if response.hovered() || open { BUTTON_HOVER } else { BUTTON });
    }
    let words = painter.layout_no_wrap(label.to_owned(), FontId::proportional(12.5), TEXT);
    let mark = if icon.is_some() { 12.0 + 7.0 } else { 0.0 };
    let left = rect.center().x - (words.size().x + mark) / 2.0;
    if let Some(icon) = icon {
        icons::paint(painter, icon, Rect::from_center_size(pos2(left + 6.0, rect.center().y), vec2(12.0, 12.0)), TEXT);
    }
    painter.galley(pos2(left + mark, rect.center().y - words.size().y / 2.0), words, TEXT);
    response
}

/// A segmented switch whose pills share `track`'s width evenly, the chosen one the lighter
/// grey. Returns the choice clicked.
pub fn even_switch(
    ui: &egui::Ui,
    salt: &str,
    track: Rect,
    labels: &[&str],
    hints: &[&str],
    chosen: Option<usize>,
    font: FontId,
) -> Option<usize> {
    const BETWEEN: f32 = 2.0;
    const INSET: f32 = 3.0;
    ui.painter().rect_filled(track, track.height() / 2.0, CARD);
    let each = (track.width() - 2.0 * INSET - BETWEEN * labels.len().saturating_sub(1) as f32) / labels.len() as f32;
    let reach = ((LEAST_CLICK - (track.height() - 2.0 * INSET)) / 2.0).max(0.0);
    let mut clicked = None;
    for (index, label) in labels.iter().enumerate() {
        let corner = pos2(track.min.x + INSET + (each + BETWEEN) * index as f32, track.min.y + INSET);
        let cell = Rect::from_min_size(corner, vec2(each, track.height() - 2.0 * INSET));
        let mut response =
            ui.interact(cell.expand2(vec2(0.0, reach)), ui.scope_id().with((salt, index)), Sense::click());
        let on = chosen == Some(index);
        paint_pill(ui.painter(), cell, on);
        let ink = if on || response.hovered() { TEXT } else { TEXT_DIM };
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *label, font.clone(), ink);
        if let Some(hint) = hints.get(index) {
            response = response.on_hover_text(*hint);
        }
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// What one row of the left column's list shows.
#[derive(Default)]
struct ListRow<'a> {
    /// The item's number in the playlist, in the fixed-width face at the left.
    number: Option<usize>,
    title: &'a str,
    /// A small word after the title, in capitals (the library a song is in).
    tag: Option<&'a str>,
    /// The open item or song: the lighter row, its title heavier.
    chosen: bool,
    /// One of several picked out: the lighter row of what is chosen (never orange), the title
    /// as it was.
    lit: bool,
    /// On the screens now: its number is orange and the tally dot shows.
    live: bool,
    /// What it names has gone, or it is a place kept for something to come.
    faint: bool,
    /// The mark of its kind, at the right (a passage, a video…), where the tally dot goes.
    icon: Option<Icon>,
    /// A small number at the right (how many slides a song has).
    count: Option<usize>,
    /// The theme version a playlist item shows in, as a small pill on a second line under its
    /// title, and whether it is lit: the item on screen in that version.
    version: Option<(&'a str, bool)>,
}

/// How tall a row with a version's pill under its title is: the title's line, 3, and the pill.
const ROW_WITH_PILL: f32 = 5.0 + 18.2 + 3.0 + 15.0 + 5.0;

/// Draws one row of the left column's list, 42 high, taller with a version's pill. Returns its
/// response and where its "more" button goes (the caller makes the button after, so that it
/// takes its own clicks).
fn list_row(ui: &mut egui::Ui, look: &ListRow) -> (egui::Response, Rect) {
    let tall = if look.version.is_some() { ROW_WITH_PILL.max(LIST_ROW) } else { LIST_ROW };
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), tall), Sense::click_and_drag());
    let more = Rect::from_center_size(
        pos2(row.max.x - 10.0 - MORE_BUTTON / 2.0, row.center().y),
        egui::Vec2::splat(MORE_BUTTON),
    );
    if !ui.is_rect_visible(row) {
        return (response, more);
    }
    let painter = ui.painter();
    if look.chosen || look.lit {
        painter.rect_filled(row, RADIUS, CARD);
    } else if response.hovered() {
        painter.rect_filled(row, RADIUS, ROW_HOVER);
    }
    let mut left = row.min.x + 10.0;
    if let Some(number) = look.number {
        let cell = Rect::from_min_size(pos2(left, row.center().y - 12.8), vec2(28.0, 25.6));
        painter.rect_filled(cell, 4.0, CARD);
        let ink = if look.live {
            TALLY
        } else if look.chosen {
            TEXT
        } else {
            TEXT_FAINT
        };
        paint_spaced(ui, cell.center(), Align2::CENTER_CENTER, &format!("{number:02}"), fixed(19.0), ink, -0.8);
        left += 28.0 + 10.0;
    }
    // From the right: the "more" button, then the tally dot, the kind's mark or the count. A
    // numbered row (a playlist's item) says it is on screen by its orange number alone, so it
    // keeps its kind's mark; a row without a number has the dot.
    let mut right = more.min.x - 10.0;
    if look.live && look.number.is_none() {
        paint_tally(ui.painter(), pos2(right - 4.0, row.center().y));
        right -= 8.0 + 8.0;
    } else if let Some(icon) = look.icon {
        let mark = Rect::from_center_size(pos2(right - 7.0, row.center().y), vec2(14.0, 14.0));
        icons::paint(ui.painter(), icon, mark, TEXT_FAINT);
        right -= 14.0 + 8.0;
    } else if let Some(count) = look.count.filter(|count| *count > 0) {
        let drawn = ui.painter().text(
            pos2(right, row.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            fixed(11.0),
            TEXT_FAINT,
        );
        right = drawn.min.x - 8.0;
    }
    let (font, ink) = if look.faint {
        (FontId::proportional(13.5), TEXT_FAINT.gamma_multiply(0.7))
    } else if look.chosen {
        (semibold(13.5), TEXT)
    } else {
        (FontId::proportional(13.5), TEXT_DIM)
    };
    let clip = Rect::from_min_max(pos2(left, row.min.y), pos2(right.max(left), row.max.y));
    let painter = ui.painter().with_clip_rect(clip);
    // With a version's pill, the title and the pill are one block in the middle of the row.
    let title_middle = if look.version.is_some() { row.center().y - (3.0 + 15.0) / 2.0 } else { row.center().y };
    let title = painter.text(pos2(left, title_middle), Align2::LEFT_CENTER, look.title, font, ink);
    if let Some((name, lit)) = look.version {
        let (ink, fill, edge) = if lit {
            (TALLY, TALLY.gamma_multiply(0.16), TALLY.gamma_multiply(0.45))
        } else {
            (TEXT_DIM, Color32::TRANSPARENT, LINE)
        };
        let galley = spaced(ui, &name.to_uppercase(), fixed_semibold(10.0), ink, 0.5);
        let pill = Rect::from_min_size(
            pos2(left, title_middle + 18.2 / 2.0 + 3.0),
            vec2((galley.size().x + 10.0).min(right - left), 15.0),
        );
        painter.rect_filled(pill, 4.0, fill);
        painter.rect_stroke(pill, 4.0, Stroke::new(1.0, edge), StrokeKind::Inside);
        painter.galley(pos2(pill.min.x + 5.0, pill.center().y - galley.size().y / 2.0), galley, ink);
    }
    if let Some(tag) = look.tag {
        let galley = spaced(ui, &tag.to_uppercase(), fixed_semibold(11.0), TEXT_FAINT, 0.6);
        painter.galley(pos2(title.max.x + 8.0, title_middle - galley.size().y / 2.0), galley, TEXT_FAINT);
    }
    (response, more)
}

/// Songs being dragged from the library, by their places among the songs.
#[derive(Clone)]
struct SongDrag(Vec<usize>);

/// Something carried in the window that the left column can take: songs from the song list, a
/// picture or video from the media strip, or a result of the find box. Each lands where files
/// of its kind dropped from the Finder would (`file_drop::DropTarget::takes`), shown the same
/// way: an ink line where it goes in a playlist, an ink outline round a row it goes onto.
#[derive(Clone)]
enum Carried {
    Songs(Vec<usize>),
    Media(usize),
    Found(crate::home::FoundDrag),
}

impl Carried {
    /// What is being carried now, if anything the left column could take.
    fn now(ctx: &egui::Context) -> Option<Carried> {
        use egui::DragAndDrop;
        if let Some(songs) = DragAndDrop::payload::<SongDrag>(ctx) {
            return Some(Carried::Songs(songs.0.clone()));
        }
        if let Some(media) = DragAndDrop::payload::<crate::home::MediaDrag>(ctx) {
            return Some(Carried::Media(media.0));
        }
        DragAndDrop::payload::<crate::home::FoundDrag>(ctx).map(|found| Carried::Found((*found).clone()))
    }

    /// What it is, for the places it could land.
    fn kind(&self) -> crate::file_drop::Kind {
        use crate::file_drop::Kind;
        match self {
            Carried::Songs(_) => Kind::Song,
            Carried::Media(_) => Kind::Media,
            Carried::Found(found) if found.song.is_some() => Kind::Song,
            Carried::Found(_) => Kind::Passage,
        }
    }

    /// Whether it can land on `target`: as a file of its kind can, but not in All Songs, which
    /// has every song already.
    fn lands_on(&self, target: &crate::file_drop::DropTarget) -> bool {
        !matches!(target, crate::file_drop::DropTarget::Library(None)) && target.takes(self.kind())
    }

    /// What a playlist keeps for it, in order.
    fn items(&self, show: &Show, state: &HomeState) -> Vec<crate::service::Item> {
        use crate::service::Item;
        match self {
            Carried::Songs(songs) => songs
                .iter()
                .filter_map(|at| {
                    let (key, song) = (state.song_keys.get(*at)?, show.presentations().get(*at)?);
                    Some(Item::song(key.clone(), song.title.clone()))
                })
                .collect(),
            Carried::Media(at) => show
                .media()
                .get(*at)
                .map(|media| Item::Media { id: media.id, name: media.name.clone() })
                .into_iter()
                .collect(),
            Carried::Found(found) => vec![found.item.clone()],
        }
    }

    /// The songs a library takes from it, by the names they are kept by.
    fn song_keys(&self, state: &HomeState) -> Vec<String> {
        let places = match self {
            Carried::Songs(songs) => songs.clone(),
            Carried::Found(found) => found.song.into_iter().collect(),
            Carried::Media(_) => Vec::new(),
        };
        places.iter().filter_map(|at| state.song_keys.get(*at).cloned()).collect()
    }

    /// A result of the find box: dropped where it lands, it ends the search.
    fn found(&self) -> Option<&crate::home::FoundDrag> {
        match self {
            Carried::Found(found) => Some(found),
            _ => None,
        }
    }
}

/// Whether what is carried is let go over `area` this frame.
fn let_go_over(ctx: &egui::Context, area: Rect) -> bool {
    ctx.input(|input| input.pointer.any_released() && input.pointer.interact_pos().is_some_and(|at| area.contains(at)))
}

/// What is carried over `response` (a row it could be dropped on): `Some(true)` when it lands
/// there, which an ink outline shows, `Some(false)` when it doesn't, which the pointer shows;
/// `None` with nothing carried, or the pointer elsewhere.
fn carried_over(response: &egui::Response, target: &crate::file_drop::DropTarget) -> Option<bool> {
    let carried = Carried::now(&response.ctx).filter(|_| response.contains_pointer())?;
    let lands = carried.lands_on(target);
    if !lands {
        response.ctx.set_cursor_icon(egui::CursorIcon::NoDrop);
    }
    Some(lands)
}

/// What is let go over `response` (a row) this frame, when it lands there. It is taken: nothing
/// else can take it after.
fn dropped_on(response: &egui::Response, target: &crate::file_drop::DropTarget) -> Option<Carried> {
    let carried = Carried::now(&response.ctx).filter(|carried| carried.lands_on(target))?;
    let released = response.contains_pointer() && response.ctx.input(|input| input.pointer.any_released());
    released.then(|| {
        egui::DragAndDrop::clear_payload(&response.ctx);
        carried
    })
}

/// Adds songs (by the names they are kept by) to a library, each once. Returns how many were
/// not in it already.
fn add_to_library(state: &mut HomeState, id: sp_model::Uuid, keys: &[String]) -> usize {
    let Some(library) = state.libraries.iter_mut().find(|library| library.id == id) else { return 0 };
    let mut added = 0;
    for key in keys {
        if !library.has(key) {
            library.toggle(key);
            added += 1;
        }
    }
    if added > 0 {
        state.libraries_changed = true;
    }
    added
}

/// A result of the find box has been dropped on a playlist (`index` among the playlists) and
/// `item` added at its end (`added`), or found there already. The search ends. In the playlist
/// listed in the left column the item is opened, as a click on its row opens it (nothing goes
/// on the screens); in another playlist, a notice says where it went.
#[allow(clippy::too_many_arguments)]
fn found_added_to_playlist(
    ctx: &egui::Context,
    show: &Show,
    state: &mut HomeState,
    commands: &mut Vec<Command>,
    index: usize,
    item: &crate::service::Item,
    added: bool,
    name: &str,
) {
    crate::home::end_search(ctx, state);
    let Some(playlist) = state.services.list.get(index) else { return };
    let playlist_name = playlist.name.clone();
    let place = if added {
        playlist.items.len().checked_sub(1)
    } else {
        // Only a song or a passage is ever refused, being there already: it is opened where it is.
        state.services.place_of(index, item)
    };
    if index == state.services.current && state.playlist_open {
        if let Some(place) = place {
            carry_out_item_ask(show, state, commands, ItemAsk::Open(place));
        }
        if !added {
            state.say(format!("{name} is already in {playlist_name}."));
        }
    } else if added {
        state.say(format!("Added {name} to {playlist_name}."));
    } else {
        state.say(format!("{name} is already in {playlist_name}."));
    }
}

/// A song found with the find box has been dropped on a library (`added` to it, or there
/// already): the search ends, and a notice says so.
fn found_added_to_library(ctx: &egui::Context, state: &mut HomeState, id: sp_model::Uuid, added: bool, name: &str) {
    crate::home::end_search(ctx, state);
    let library = state.libraries.iter().find(|library| library.id == id).map_or_else(String::new, |l| l.name.clone());
    state.say(if added { format!("Added {name} to {library}.") } else { format!("{name} is already in {library}.") });
}

/// What is carried beside the pointer while it is dragged: its name (or how many), on a small
/// grey chip, until it is dropped. Long words are cut short with …, and near the window's right
/// edge the chip goes on the pointer's left, so all of it shows.
pub fn paint_carried(ctx: &egui::Context, words: &str) {
    let Some(at) = ctx.pointer_latest_pos() else { return };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("carried")));
    let format = egui::TextFormat { font_id: semibold(12.5), color: Color32::WHITE, ..Default::default() };
    let mut job = egui::text::LayoutJob::single_section(words.to_owned(), format);
    job.wrap =
        egui::text::TextWrapping {
            max_width: 300.0, max_rows: 1, break_anywhere: true, overflow_character: Some('…')
        };
    let text = painter.layout_job(job);
    let mut chip = Rect::from_min_size(at + vec2(14.0, 10.0), text.size() + vec2(18.0, 10.0));
    if chip.max.x > ctx.content_rect().max.x - 4.0 {
        chip = chip.translate(vec2(-(chip.width() + 28.0), 0.0));
    }
    painter.rect_filled(chip, 8.0, SELECTED);
    painter.rect_stroke(chip, 8.0, Stroke::new(1.0, Color32::from_white_alpha(30)), StrokeKind::Inside);
    painter.galley(chip.min + vec2(9.0, 5.0), text, Color32::WHITE);
}

/// Where among a list's `rows` (top to bottom) a row dragged with the pointer at `y` would go:
/// how many rows are above that place, by which half of a row the pointer is over.
pub fn drop_slot(rows: &[Rect], y: f32) -> usize {
    rows.iter().filter(|row| row.center().y < y).count()
}

/// Whether a row dragged in a list was let go this frame, to go where the ink line is. A drag
/// that Escape stopped was not: egui ends it without the button being let go, and nothing moves.
pub fn drag_let_go(response: &egui::Response) -> bool {
    response.drag_stopped() && response.ctx.input(|input| input.pointer.any_released())
}

/// While rows of a list are dragged to another place in it, as the playlist's items are: those
/// being moved (`moving`, by their place among `rows`) go faint where they are, washed with the
/// colour the list lies on (`ground`), and an ink line shows where they would go (`slot`, from
/// `drop_slot`), midway between two rows.
pub fn paint_rows_moving(
    painter: &egui::Painter,
    rows: &[Rect],
    moving: impl Fn(usize) -> bool,
    slot: usize,
    ground: Color32,
) {
    paint_moving(painter, rows, moving, slot, ground, false);
}

/// Where among tiles side by side (left to right) one dragged with the pointer at `x` would go:
/// how many tiles are left of that place, by which half of a tile the pointer is over.
pub fn drop_slot_across(tiles: &[Rect], x: f32) -> usize {
    tiles.iter().filter(|tile| tile.center().x < x).count()
}

/// As [`paint_rows_moving`], for tiles side by side (the media strip's): the ink line stands
/// upright, midway between two tiles (`slot`, from [`drop_slot_across`]).
pub fn paint_tiles_moving(
    painter: &egui::Painter,
    tiles: &[Rect],
    moving: impl Fn(usize) -> bool,
    slot: usize,
    ground: Color32,
) {
    paint_moving(painter, tiles, moving, slot, ground, true);
}

/// The things being moved go faint, and the ink line goes between the two either side of `slot`:
/// across a list, or (`across`) upright between tiles side by side.
fn paint_moving(
    painter: &egui::Painter,
    rows: &[Rect],
    moving: impl Fn(usize) -> bool,
    slot: usize,
    ground: Color32,
    across: bool,
) {
    for (at, row) in rows.iter().enumerate() {
        if moving(at) {
            painter.rect_filled(*row, RADIUS, ground.gamma_multiply(0.6));
        }
    }
    let start = |row: &Rect| if across { row.min.x } else { row.min.y };
    let end = |row: &Rect| if across { row.max.x } else { row.max.y };
    let at = match (slot.checked_sub(1).and_then(|before| rows.get(before)), rows.get(slot)) {
        (Some(before), Some(after)) => (end(before) + start(after)) / 2.0,
        (None, Some(after)) => start(after),
        (Some(before), None) => end(before),
        (None, None) => return,
    };
    let stroke = Stroke::new(2.0, TEXT);
    let Some(first) = rows.first() else { return };
    if across {
        painter.line_segment([pos2(at, first.min.y + 4.0), pos2(at, first.max.y - 4.0)], stroke);
    } else {
        painter.line_segment([pos2(first.min.x + 4.0, at), pos2(first.max.x - 4.0, at)], stroke);
    }
}

/// The left column: a small label saying whether a playlist or a library is listed, the name
/// of the one that is (which opens the chooser of playlists and libraries), its items or
/// songs, and at the foot what adds to it and a filter.
pub fn library(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let frame = column_frame(Column::Left).inner_margin(egui::Margin { left: 10, right: 10, top: 16, bottom: 12 });
    let panel = egui::Panel::left("library")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(column_outer(LEFT_WIDTH))
        .frame(frame);
    let shown = panel.show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let width = ui.available_width();
        // The label, with the button that brings songs and slides in.
        let (head, _) = ui.allocate_exact_size(vec2(width, 36.0), Sense::hover());
        let label = if state.playlist_open { "Playlist" } else { "Library" };
        cap(ui, pos2(head.min.x + 8.0, head.min.y + 14.0), Align2::LEFT_CENTER, label, TEXT_DIM);
        let place =
            Rect::from_min_size(pos2(head.max.x - 8.0 - ICON_BUTTON, head.min.y), egui::Vec2::splat(ICON_BUTTON));
        let import = icon_button(ui, place, ui.scope_id().with("import"), Icon::Import, 1.0, false)
            .on_hover_text("Import songs or slides");
        egui::Popup::menu(&import).show(|ui| {
            use crate::menubar::Action;
            // Songs by kind of file, as File › Import Songs has them; then slides.
            crate::live_tabs::menu_heading(ui, "Import Songs");
            for entry in crate::menubar::IMPORT_SONGS {
                let crate::menubar::Entry::Do(action, words, _) = *entry else { continue };
                let hint = match action {
                    Action::ImportLibraryFolder => "A folder of song documents; each folder in it becomes a library",
                    Action::ImportBundle => "Song bundles and song packs: songs with the pictures and videos they show",
                    Action::ImportWords => "Text, rich-text and Word files: a song from each",
                    _ => "Song documents, ChordPro and lyric files, and songbooks",
                };
                if ui.button(words).on_hover_text(hint).clicked() {
                    state.menu_asked = Some(action);
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .button("Slides\u{2026}")
                .on_hover_text("A presentation, a PDF or a folder of pictures, as picture slides")
                .clicked()
            {
                state.menu_asked = Some(Action::ImportSlides);
                ui.close();
            }
        });
        // The name of what is listed: a click opens the chooser, or puts it away.
        let (row, name) = ui.allocate_exact_size(vec2(width, 39.6), Sense::click());
        let middle = row.min.y + 16.8;
        let (icon, tint) =
            if state.playlist_open { (Icon::Playlist, PLAYLIST_TINT) } else { (Icon::Library, LIBRARY_TINT) };
        icons::paint(
            ui.painter(),
            icon,
            Rect::from_center_size(pos2(row.min.x + 16.0, middle), vec2(16.0, 16.0)),
            tint,
        );
        let chevron = Rect::from_center_size(pos2(row.max.x - 8.0 - 6.5, middle), vec2(13.0, 13.0));
        let arrow = if state.chooser_open { Icon::ChevronUp } else { Icon::ChevronDown };
        icons::paint(ui.painter(), arrow, chevron, if name.hovered() { TEXT } else { TEXT_FAINT });
        let title = source_name(state);
        let ink = if state.chooser_open { TEXT_DIM } else { TEXT };
        let clip = Rect::from_min_max(pos2(row.min.x + 32.0, row.min.y), pos2(chevron.min.x - 8.0, row.max.y));
        let galley = spaced(ui, &title, semibold(19.0), ink, -0.38);
        ui.painter().with_clip_rect(clip).galley(pos2(clip.min.x, middle - galley.size().y / 2.0), galley, ink);
        if crate::keys::tags_shown(state) {
            let tag_at = Rect::from_min_max(pos2(chevron.min.x - 30.0, name.rect.min.y + 6.0), chevron.max);
            crate::keys::corner_tag(ui.painter(), tag_at, &crate::keys::with_modifier("P"));
        }
        let hint = crate::keys::hint("Choose a playlist or a library", "P", false);
        if name.on_hover_text(hint).clicked() {
            state.chooser_open = !state.chooser_open;
        }
        // Escape puts it away, unless it ends retyping a name in it, or is for a menu or
        // something else over the window; it clears nothing.
        let retyping = state.renaming_service.is_some() || state.renaming_library.is_some();
        if state.chooser_open
            && !retyping
            && !state.escape_taken_above(ui.ctx())
            && ui.input(|input| input.key_pressed(egui::Key::Escape))
        {
            state.chooser_open = false;
        }
        if state.chooser_open {
            // Only things to open: the list and the foot are put away.
            egui::ScrollArea::vertical().id_salt("chooser").auto_shrink([false, false]).show(ui, |ui| {
                chooser(ui, show, state, commands);
            });
            return;
        }
        left_foot(ui, show, state, commands);
        // While songs are carried (or song files dragged in from the Finder, or a result of
        // the find box) the chooser shows above the list, so they can be dropped on a playlist
        // or a library.
        let carrying = egui::DragAndDrop::has_payload_of_type::<SongDrag>(ui.ctx())
            || egui::DragAndDrop::has_payload_of_type::<crate::home::FoundDrag>(ui.ctx());
        if carrying || crate::file_drop::wants_chooser(state) {
            let most = ui.available_height() * 0.6;
            egui::ScrollArea::vertical().id_salt("chooser").max_height(most).show(ui, |ui| {
                chooser(ui, show, state, commands);
            });
            let (line, _) = ui.allocate_exact_size(vec2(width, 9.0), Sense::hover());
            ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, LINE));
        }
        if state.playlist_open {
            playlist_items(ui, show, state, commands);
        } else {
            library_songs(ui, show, state, commands);
        }
    });
    round_corners(ui, column_panel(shown.response.rect, Column::Left));
}

/// The name of what the left column lists: the playlist in front, the library shown, or All
/// Songs.
fn source_name(state: &HomeState) -> String {
    if state.playlist_open {
        return state.services.front().name.clone();
    }
    let library = state.library_shown.and_then(|id| state.libraries.iter().find(|library| library.id == id));
    match library {
        Some(library) if library.name.trim().is_empty() => "Untitled".to_owned(),
        Some(library) => library.name.clone(),
        None => "All Songs".to_owned(),
    }
}

/// The foot of the left column: for a library the order its songs are listed in and New song,
/// for a playlist Add to playlist; then the filter.
fn left_foot(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let library = !state.playlist_open;
    let height = 8.0 + if library { 26.8 + 8.0 } else { 0.0 } + 30.0 + 10.0 + 32.0;
    let foot = egui::Panel::bottom("left foot")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(height)
        .frame(Frame::new());
    foot.show(ui, |ui| {
        let area = ui.max_rect();
        let mut top = area.min.y + 8.0;
        if library {
            let row = Rect::from_min_size(pos2(area.min.x, top), vec2(area.width(), 26.8));
            let label = cap(ui, pos2(row.min.x, row.center().y), Align2::LEFT_CENTER, "Order", TEXT_DIM);
            let track = Rect::from_min_max(pos2(label.max.x + 8.0, row.min.y), row.max);
            let orders = crate::kept::SongOrder::ALL;
            let chosen = orders.iter().position(|order| *order == state.song_order);
            let hints = ["As the library has them", "By title", "The songs with the most slides first"];
            let labels = ["As listed", "A\u{2013}Z", "Longest"];
            if let Some(at) = even_switch(ui, "song order", track, &labels, &hints, chosen, FontId::proportional(11.0))
            {
                state.song_order = orders[at];
            }
            top += 26.8 + 8.0;
        }
        let add = Rect::from_min_size(pos2(area.min.x, top), vec2(area.width(), 30.0));
        if library {
            let button = labelled_button(ui, add, ui.scope_id().with("new song"), "New song", Some(Icon::Plus), false);
            if button.on_hover_text(crate::keys::hint("New song", "N", false)).clicked() {
                state.new_song = true;
            }
        } else {
            let button = labelled_button(
                ui,
                add,
                ui.scope_id().with("add to playlist"),
                "Add to playlist",
                Some(Icon::Plus),
                false,
            );
            egui::Popup::menu(&button).show(|ui| add_to_playlist_menu(ui, show, state, commands));
        }
        // The filter: a round field, a step lighter than the panel.
        let field = Rect::from_min_size(pos2(area.min.x, top + 30.0 + 10.0), vec2(area.width(), 32.0));
        ui.painter().rect_filled(field, 16.0, CARD);
        let inner = field.shrink2(vec2(12.0, 0.0));
        let typed = ui
            .scope_builder(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .hint_text("Filter")
                        .frame(Frame::NONE)
                        .background_color(Color32::TRANSPARENT)
                        .margin(egui::Margin::ZERO)
                        .min_size(vec2(inner.width(), inner.height()))
                        .align(Align2::LEFT_CENTER)
                        .desired_width(inner.width()),
                )
            })
            .inner;
        if typed.has_focus() {
            ui.painter().rect_stroke(field, 16.0, Stroke::new(1.5, TEXT_FAINT), StrokeKind::Inside);
        }
    });
}

/// What Add to playlist offers: a song, a Bible passage (looked up on the Bible page), a
/// header, a placeholder, a picture or video, a media playlist, and the VoxLive item once
/// VoxLive is connected. What is added goes after the open item, or at the end.
fn add_to_playlist_menu(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    let mut made: Option<(Item, bool)> = None;
    ui.menu_button("A Song", |ui| {
        if let Some(song) = song_picker(ui, show, state)
            && let (Some(key), Some(found)) = (state.song_keys.get(song), show.presentations().get(song))
        {
            made = Some((Item::song(key.clone(), found.title.clone()), false));
            ui.close();
        }
    });
    if ui
        .button("A Bible Passage\u{2026}")
        .on_hover_text("Opens the Bible page: look the passage up there, then Add to the playlist")
        .clicked()
    {
        state.bible_open = true;
        state.bible.focus_lookup = true;
        ui.close();
    }
    if ui.button("A Header").on_hover_text("A heading over the items under it").clicked() {
        made = Some((Item::Header { name: "Header".to_owned() }, true));
        ui.close();
    }
    if ui
        .button("A Placeholder")
        .on_hover_text("A place kept for something to come, such as the sermon slides")
        .clicked()
    {
        made = Some((Item::Placeholder { name: "Placeholder".to_owned() }, true));
        ui.close();
    }
    ui.menu_button("A Picture or Video", |ui| {
        if show.media().is_empty() {
            ui.label(egui::RichText::new("The media bin is empty.").color(TEXT_DIM));
        }
        for media in show.media() {
            if ui.button(media.name.as_str()).clicked() {
                made = Some((Item::Media { id: media.id, name: media.name.clone() }, false));
                ui.close();
            }
        }
    });
    ui.menu_button("A Media Playlist", |ui| {
        if state.media_playlists.is_empty() {
            ui.label(
                egui::RichText::new("No media playlists yet. Make one from the menu beside MEDIA.").color(TEXT_DIM),
            );
        }
        for playlist in &state.media_playlists {
            if ui.button(playlist.name.as_str()).clicked() {
                made = Some((Item::MediaPlaylist { id: playlist.id, name: playlist.name.clone() }, false));
                ui.close();
            }
        }
    });
    // The VoxLive item: the church's VoxLive screens, a slide each.
    let screens =
        state.voxlive_slot.and_then(|slot| show.presentations().get(slot)).map_or(0, |item| item.slide_count());
    if screens > 0
        && ui
            .button("VoxLive")
            .on_hover_text("Your VoxLive screens, a slide each: a click on one puts its captions on the screens")
            .clicked()
    {
        made = Some((Item::song(crate::voxlive::ITEM_KEY.to_owned(), "VoxLive".to_owned()), false));
        ui.close();
    }
    let Some((item, rename)) = made else { return };
    let name = item.title().to_owned();
    let song = matches!(item, Item::Song { .. });
    let count = state.services.front().items.len();
    let at = state.services.chosen.map_or(count, |open| (open + 1).min(count));
    state.services.insert(at, item);
    // A song already in the playlist is not added twice; nothing new is at `at` then.
    let added = state.services.front().items.len() > count;
    if !added {
        if song {
            state.notice = Some(format!("{name} is already in {}.", state.services.front().name));
        }
        return;
    }
    if rename {
        // A header or a placeholder is named as it is made.
        state.renaming_header = Some((at, name));
        state.rename_started = true;
    }
    let _ = commands;
}

/// One row of the chooser, for a playlist or a library (`target`, where what is carried in the
/// window can land: an ink outline shows it will). Returns the row's response, its "more"
/// button's place when it has one, and where its name starts and ends (for a field to retype
/// it).
#[allow(clippy::too_many_arguments)]
fn chooser_row(
    ui: &mut egui::Ui,
    icon: Icon,
    tint: Color32,
    name: &str,
    count: usize,
    counted: &str,
    on: bool,
    menu: bool,
    target: &crate::file_drop::DropTarget,
) -> (egui::Response, Option<Rect>, egui::Pos2, f32) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), CHOOSER_ROW), Sense::hover());
    let button = if menu { Rect::from_min_max(row.min, pos2(row.max.x - ICON_BUTTON, row.max.y)) } else { row };
    let response = ui.interact(button, ui.scope_id().with("chooser row"), Sense::click());
    let carried = carried_over(&response, target) == Some(true);
    let painter = ui.painter();
    if on {
        painter.rect_filled(button, RADIUS, CARD);
    } else if response.hovered() || carried {
        painter.rect_filled(button, RADIUS, ROW_HOVER);
    }
    if carried {
        painter.rect_stroke(button, RADIUS, Stroke::new(2.0, TEXT), StrokeKind::Inside);
    }
    let middle = row.center().y;
    icons::paint(painter, icon, Rect::from_center_size(pos2(row.min.x + 16.0, middle), vec2(16.0, 16.0)), tint);
    // From the right: the tick on the one that is open, then how much is in it.
    let mut right = button.max.x - 8.0;
    if on {
        let tick = Rect::from_center_size(pos2(right - 6.5, middle), vec2(13.0, 13.0));
        icons::paint(painter, Icon::Check, tick, TEXT);
        right -= 13.0 + 10.0;
    }
    let number = painter.text(pos2(right, middle), Align2::RIGHT_CENTER, count.to_string(), fixed(12.0), TEXT_FAINT);
    right = number.min.x - 8.0;
    let start = pos2(row.min.x + 34.0, middle);
    let font = if on { semibold(14.0) } else { FontId::proportional(14.0) };
    let clip = Rect::from_min_max(pos2(start.x, row.min.y), pos2(right, row.max.y));
    painter.with_clip_rect(clip).text(start, Align2::LEFT_CENTER, name, font, TEXT);
    let noun = if count == 1 { counted.to_owned() } else { format!("{counted}s") };
    let more = menu
        .then(|| Rect::from_center_size(pos2(row.max.x - ICON_BUTTON / 2.0, middle), egui::Vec2::splat(ICON_BUTTON)));
    (response.on_hover_text(format!("{count} {noun}")), more, start, right)
}

/// A heading of the chooser with a + at its right; `second` sets it apart from the rows above
/// with a line. Returns whether + was clicked.
fn chooser_head(ui: &mut egui::Ui, text: &str, add_hint: &str, second: bool) -> bool {
    let height = if second { 43.0 + 10.0 } else { 36.0 };
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let top = if second {
        ui.painter().hline(head.x_range(), head.min.y + 10.0, Stroke::new(1.0, LINE));
        head.min.y + 10.0 + 12.0
    } else {
        head.min.y + 4.0
    };
    let place = Rect::from_min_size(pos2(head.max.x - 8.0 - ICON_BUTTON, top), egui::Vec2::splat(ICON_BUTTON));
    cap(ui, pos2(head.min.x + 8.0, place.center().y), Align2::LEFT_CENTER, text, TEXT_DIM);
    icon_button(ui, place, ui.scope_id().with(("chooser add", second)), Icon::Plus, 1.0, false)
        .on_hover_text(add_hint)
        .clicked()
}

/// The chooser: only things to open. The playlists first, then the song libraries under a
/// line (All Songs, then each library); the one that is open has the lighter row, a heavier
/// name and a tick. Songs carried from the list, a picture or video from the media bin, and a
/// result of the find box can be dropped on a row that takes them (see `Carried`).
fn chooser(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
    if chooser_head(ui, "Playlists", "New playlist", false) {
        state.services.add_service();
        state.playlist_open = true;
        state.renaming_service = Some((state.services.front_at(), state.services.front().name.clone()));
        state.rename_started = true;
    }
    let playlists: Vec<(String, usize)> = state
        .services
        .list
        .iter()
        .map(|playlist| {
            (playlist.name.clone(), playlist.items.iter().filter(|item| !matches!(item, Item::Header { .. })).count())
        })
        .collect();
    let mut delete = None;
    for (index, (name, count)) in playlists.iter().enumerate() {
        let front = index == state.services.current;
        let renaming = state.renaming_service.as_ref().is_some_and(|(at, _)| *at == index);
        let on = state.playlist_open && front;
        let shown = if renaming { "" } else { name.as_str() };
        let target = crate::file_drop::DropTarget::Playlist { index, after: None };
        let (response, more, start, right) = ui
            .push_id(("playlist", index), |ui| {
                chooser_row(ui, Icon::Playlist, PLAYLIST_TINT, shown, *count, "item", on, true, &target)
            })
            .inner;
        crate::file_drop::offer(ui, state, crate::file_drop::Place::onto(response.rect, target.clone()));
        if renaming && let Some((_, typed)) = state.renaming_service.as_mut() {
            // Set as the row sets the name.
            let look = Lettering::plain(if on { semibold(14.0) } else { FontId::proportional(14.0) }, TEXT);
            let field = rename_field_at(ui, typed, &look, start, right);
            if std::mem::take(&mut state.rename_started) {
                field.request_focus();
            }
            let (enter, escape) =
                ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
            if escape {
                state.renaming_service = None;
            } else if enter || field.lost_focus() {
                let typed = typed.clone();
                state.services.rename_at(index, &typed);
                state.renaming_service = None;
            }
            // The row's ⋯ stays where it was.
            if let Some(place) = more {
                more_button(ui, place, ui.scope_id().with(("playlist more", index)), false);
            }
            continue;
        }
        if response.clicked() {
            state.services.show(index);
            state.playlist_open = true;
            state.chooser_open = false;
            state.filter.clear();
        }
        if response.double_clicked() {
            state.services.show(index);
            state.chooser_open = true;
            state.renaming_service = Some((index, name.clone()));
            state.rename_started = true;
        }
        // Songs, a picture or video, or a result of the find box go at the playlist's end.
        if let Some(carried) = dropped_on(&response, &target) {
            let items = carried.items(show, state);
            let added: Vec<bool> = items.iter().map(|item| state.services.add_to(index, item.clone())).collect();
            if let (Some(found), Some(item)) = (carried.found(), items.first()) {
                found_added_to_playlist(ui.ctx(), show, state, commands, index, item, added == [true], &found.name);
            }
        }
        let several = playlists.len() > 1;
        let mut menu = |ui: &mut egui::Ui| {
            // Renamed where it is: the playlist in front stays in front.
            if ui.button("Rename").clicked() {
                state.chooser_open = true;
                state.renaming_service = Some((index, name.clone()));
                state.rename_started = true;
                ui.close();
            }
            if ui.button("Export\u{2026}").on_hover_text("Save the playlist and its songs as a song pack").clicked() {
                state.export_request = Some(crate::home::ExportRequest::Playlist(index));
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(several, egui::Button::new("Delete Playlist")).clicked() {
                delete = Some(index);
                ui.close();
            }
        };
        if let Some(place) = more {
            let button = more_button(ui, place, ui.scope_id().with(("playlist more", index)), response.hovered());
            egui::Popup::menu(&button).show(&mut menu);
        }
        response.context_menu(&mut menu);
    }
    // Deleted where it is, at once (Undo puts it back): the playlist in front stays in front,
    // unless it is the one deleted.
    if let Some(index) = delete {
        state.delete_playlist(index);
    }
    if chooser_head(ui, "Song libraries", "New library", true) {
        let made = crate::libraries::Library::new(crate::libraries::new_name(&state.libraries));
        state.renaming_library = Some((made.id, made.name.clone()));
        state.rename_started = true;
        state.library_shown = Some(made.id);
        state.playlist_open = false;
        state.libraries.push(made);
        state.libraries_changed = true;
    }
    let hidden = state.hidden_slots();
    let songs =
        (0..show.presentations().len()).filter(|at| !hidden.contains(at) && !state.removed_songs.contains(at)).count();
    let on = !state.playlist_open && state.library_shown.is_none();
    let everything = crate::file_drop::DropTarget::Library(None);
    let (all, ..) = ui
        .push_id("all songs", |ui| {
            chooser_row(ui, Icon::Library, LIBRARY_TINT, "All Songs", songs, "song", on, false, &everything)
        })
        .inner;
    crate::file_drop::offer_library_row(ui, state, all.rect, None);
    if all.clicked() {
        state.library_shown = None;
        state.playlist_open = false;
        state.chooser_open = false;
        state.filter.clear();
    }
    let mut delete = None;
    let libraries: Vec<(sp_model::Uuid, String, usize)> =
        state.libraries.iter().map(|library| (library.id, library.name.clone(), library.songs.len())).collect();
    for (id, name, count) in libraries {
        let renaming = state.renaming_library.as_ref().is_some_and(|(have, _)| *have == id);
        let on = !state.playlist_open && state.library_shown == Some(id);
        let shown = if renaming {
            ""
        } else if name.trim().is_empty() {
            "Untitled"
        } else {
            name.as_str()
        };
        let target = crate::file_drop::DropTarget::Library(Some(id));
        let (response, more, start, right) = ui
            .push_id(("library", id), |ui| {
                chooser_row(ui, Icon::Library, LIBRARY_TINT, shown, count, "song", on, true, &target)
            })
            .inner;
        crate::file_drop::offer_library_row(ui, state, response.rect, Some(id));
        if renaming && let Some((_, typed)) = state.renaming_library.as_mut() {
            // Set as the row sets the name.
            let look = Lettering::plain(if on { semibold(14.0) } else { FontId::proportional(14.0) }, TEXT);
            let field = rename_field_at(ui, typed, &look, start, right);
            if std::mem::take(&mut state.rename_started) {
                field.request_focus();
            }
            let (enter, escape) =
                ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
            if escape {
                state.renaming_library = None;
            } else if enter || field.lost_focus() {
                let typed = typed.trim().to_owned();
                if let Some(library) = state.libraries.iter_mut().find(|library| library.id == id)
                    && !typed.is_empty()
                {
                    library.name = typed;
                    state.libraries_changed = true;
                }
                state.renaming_library = None;
            }
            // The row's ⋯ stays where it was.
            if let Some(place) = more {
                more_button(ui, place, ui.scope_id().with(("library more", id)), false);
            }
            continue;
        }
        if response.clicked() {
            state.library_shown = Some(id);
            state.playlist_open = false;
            state.chooser_open = false;
            state.filter.clear();
        }
        if response.double_clicked() {
            state.chooser_open = true;
            state.renaming_library = Some((id, name.clone()));
            state.rename_started = true;
        }
        // Songs carried from the list, or a song found with the find box (never a passage).
        if let Some(carried) = dropped_on(&response, &target) {
            let added = add_to_library(state, id, &carried.song_keys(state));
            if let Some(found) = carried.found() {
                found_added_to_library(ui.ctx(), state, id, added > 0, &found.name);
            }
        }
        let mut menu = |ui: &mut egui::Ui| {
            if ui.button("Rename").clicked() {
                state.chooser_open = true;
                state.renaming_library = Some((id, name.clone()));
                state.rename_started = true;
                ui.close();
            }
            ui.separator();
            if ui.button("Delete Library").on_hover_text("The songs stay in All Songs").clicked() {
                delete = Some(id);
                ui.close();
            }
        };
        if let Some(place) = more {
            let button = more_button(ui, place, ui.scope_id().with(("library more", id)), response.hovered());
            egui::Popup::menu(&button).show(&mut menu);
        }
        response.context_menu(&mut menu);
    }
    // At once: Undo puts it back.
    if let Some(id) = delete {
        state.delete_library(id);
    }
    ui.add_space(8.0);
}
/// The name of the first library a song is in, shown at the right of its row in a playlist.
fn library_of<'a>(state: &'a HomeState, key: &str) -> Option<&'a str> {
    state.libraries.iter().find(|library| library.has(key)).map(|library| library.name.as_str())
}

/// The songs the left column lists: how many the library (or All Songs) has, and those whose
/// titles have what is typed in the filter, by their place among the songs, in the order
/// chosen.
fn song_list(show: &Show, state: &HomeState, library: Option<&crate::libraries::Library>) -> crate::kept::SongList {
    let hidden = state.hidden_slots();
    let listed = |index: usize| !hidden.contains(&index) && !state.removed_songs.contains(&index);
    // The songs in the library shown, found once with a set rather than row by row.
    let in_set: Option<std::collections::HashSet<usize>> = library.map(|library| {
        let wanted: std::collections::HashSet<&str> = library.songs.iter().map(String::as_str).collect();
        state.song_keys.iter().enumerate().filter(|(_, key)| wanted.contains(key.as_str())).map(|(at, _)| at).collect()
    });
    let in_library = |index: usize| in_set.as_ref().is_none_or(|set| set.contains(&index));
    let mut total = 0;
    let mut shown = Vec::new();
    for (index, song) in show.presentations().iter().enumerate() {
        if listed(index) && in_library(index) {
            total += 1;
            if matches_filter(&song.title, &state.filter) {
                shown.push(index);
            }
        }
    }
    state.song_order.sort(show, &mut shown);
    crate::kept::SongList { total, shown }
}

/// What a song's menu asked for that changes a playlist: the song added to the playlist in
/// front, or to another playlist (`None` for a new one).
#[derive(Default)]
pub struct SongAsks {
    pub add: Option<usize>,
    pub add_to: Option<(usize, Option<usize>)>,
}

impl SongAsks {
    /// Carries out what was asked.
    pub fn carry_out(self, show: &Show, state: &mut HomeState) {
        use crate::service::Item;
        if let Some(index) = self.add
            && let (Some(key), Some(song)) = (state.song_keys.get(index), show.presentations().get(index))
        {
            state.services.add(Item::song(key.clone(), song.title.clone()));
        }
        // To the playlist chosen, or to a new one, which is then shown with its name ready to type.
        if let Some((index, playlist)) = self.add_to
            && let (Some(key), Some(song)) = (state.song_keys.get(index).cloned(), show.presentations().get(index))
        {
            let item = Item::song(key, song.title.clone());
            match playlist {
                Some(at) => {
                    let name = state.services.list.get(at).map_or_else(String::new, |p| p.name.clone());
                    state.notice = Some(if state.services.add_to(at, item) {
                        format!("Added {} to {name}.", song.title)
                    } else {
                        format!("{} is already in {name}.", song.title)
                    });
                }
                None => {
                    state.services.add_service();
                    state.services.add(item);
                    state.playlist_open = true;
                    state.chooser_open = true;
                    state.renaming_service = Some((state.services.front_at(), state.services.front().name.clone()));
                    state.rename_started = true;
                }
            }
        }
    }
}

/// The songs the left column lists for the library chosen (or every song, with none chosen),
/// in the order chosen and through the filter. A library chosen that has gone is let go. The
/// list is made again only when the songs, the library shown, the filter or the order change,
/// so a library of thousands of songs doesn't slow every frame.
pub fn listed_songs(show: &Show, state: &mut HomeState) -> std::sync::Arc<crate::kept::SongList> {
    let library = state.library_shown.and_then(|id| state.libraries.iter().position(|library| library.id == id));
    if library.is_none() {
        state.library_shown = None;
    }
    let epoch = state.lists.songs.epoch();
    let songs_of =
        |at: Option<usize>| at.and_then(|at| state.libraries.get(at)).map(|library| library.songs.as_slice());
    let order = state.song_order;
    let kept = state.lists.song_list.get(|(made_in, made_of, filter, in_order)| {
        *made_in == epoch && made_of.as_deref() == songs_of(library) && *filter == state.filter && *in_order == order
    });
    match kept {
        Some(list) => list,
        None => {
            let made = song_list(show, state, library.and_then(|at| state.libraries.get(at)));
            let made_of = songs_of(library).map(<[String]>::to_vec);
            state.lists.song_list.keep((epoch, made_of, state.filter.clone(), order), made)
        }
    }
}

/// The songs of the library chosen (or every song): a click opens a song, its "more" button
/// (or a right-click) opens its menu, and it can be dragged onto a playlist or a library in
/// the chooser, which shows while songs are carried.
fn library_songs(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let list = listed_songs(show, state);
    let (total, shown) = (list.total, &list.shown);
    let in_library = state.library_shown.is_some();
    let live_in = show.live().map(|at| at.presentation);
    let mut asks = SongAsks::default();
    let list = ui.available_rect_before_wrap();
    let target = crate::file_drop::DropTarget::Library(state.library_shown);
    crate::file_drop::offer(ui, state, crate::file_drop::Place::onto(list, target.clone()).where_songs_land());
    let (pointer, mods, down) =
        ui.input(|input| (input.pointer.interact_pos(), input.modifiers, input.pointer.primary_down()));
    // Songs picked out lose the ones no longer listed.
    if !state.picked_songs.is_empty() {
        state.picked_songs.retain(|at| shown.contains(at));
    }
    // ↑ and ↓ open the song above or below in the list, as a click on it does; with Shift they
    // pick out the songs on the way, as the Finder does. Not while typing, with the song search
    // open, editing, or on the Bible page.
    let mut scroll_to = None;
    if list_keys_work(ui, state) && !shown.is_empty() {
        let (up, down, shift) = ui.input(|input| {
            (input.key_pressed(egui::Key::ArrowUp), input.key_pressed(egui::Key::ArrowDown), input.modifiers.shift)
        });
        let last = shown.len() - 1;
        let current = shown.iter().position(|at| *at == show.selected());
        let next = match current {
            Some(at) if up && at > 0 => Some(at - 1),
            Some(at) if down && at < last => Some(at + 1),
            None if up => Some(last),
            None if down => Some(0),
            _ => None,
        };
        if next.is_none() && (up || down) {
            state.say(if down { "That is the last song in the list." } else { "That is the first song in the list." });
        }
        if let Some(next) = next {
            let song = shown[next];
            if shift {
                let mods = egui::Modifiers::SHIFT;
                if state.song_anchor.is_none() {
                    state.song_anchor = Some(show.selected());
                }
                crate::home::pick(&mut state.picked_songs, &mut state.song_anchor, shown, song, mods);
            } else {
                state.picked_songs = vec![song];
                state.song_anchor = Some(song);
            }
            state.picking = crate::home::Picking::Songs;
            commands.push(Command::Select(song));
            state.services.chosen = None;
            scroll_to = Some(next);
        }
    }
    // Command-A (Ctrl elsewhere) picks every song listed, with nothing being typed.
    if list_keys_work(ui, state) && ui.input(|input| crate::home::command_key(input, egui::Key::A, false)) {
        state.picked_songs = shown.clone();
        state.picking = crate::home::Picking::Songs;
    }
    // Only the rows in view are laid out, so thousands of songs cost no more to draw than a few.
    let mut scroll = egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]);
    // The song gone to by ↑ or ↓ is kept in sight.
    if let Some(row) = scroll_to {
        let id = ui.make_persistent_id("song list");
        let offset = egui::scroll_area::State::load(ui.ctx(), id).map_or(0.0, |state| state.clamped_offset().y);
        let (top, height) = (row as f32 * LIST_ROW, list.height());
        if top < offset {
            scroll = scroll.vertical_scroll_offset(top);
        } else if top + LIST_ROW > offset + height {
            scroll = scroll.vertical_scroll_offset(top + LIST_ROW - height);
        }
    }
    scroll.show_rows(ui, LIST_ROW, shown.len(), |ui, in_view| {
        let several = state.picked_songs.len() > 1;
        let mut rows: Vec<(usize, Rect)> = Vec::new();
        for index in shown[in_view].iter().copied() {
            let Some(song) = show.presentations().get(index) else { continue };
            // Several picked out are lit together; otherwise the song shown is.
            let chosen = if several {
                state.picked_songs.contains(&index)
            } else {
                index == show.selected() && !state.bible_open
            };
            // In All Songs, the first library a song is in is named after its title.
            let tag = if in_library { None } else { state.song_keys.get(index).and_then(|key| library_of(state, key)) };
            let look = ListRow {
                title: song.title.as_str(),
                tag,
                chosen,
                live: live_in == Some(index),
                count: Some(song.slide_count()),
                ..ListRow::default()
            };
            let (response, more) = list_row(ui, &look);
            rows.push((index, response.rect));
            // A drag starting on a song picks out the songs it passes over, and carries them
            // once it leaves the list; on one of several already picked, it carries them all.
            if response.drag_started()
                && let Some(start) = pointer
            {
                let keeps = several && state.picked_songs.contains(&index);
                if !keeps && !mods.command && !mods.shift {
                    state.picked_songs = vec![index];
                    state.song_anchor = Some(index);
                }
                state.picking = crate::home::Picking::Songs;
                state.sweep = Some(crate::home::Sweep {
                    list: crate::home::Picking::Songs,
                    from: Some(index),
                    start,
                    keeps,
                    carried: false,
                });
            }
            if response.clicked() {
                state.picking = crate::home::Picking::Songs;
                crate::home::pick(&mut state.picked_songs, &mut state.song_anchor, shown, index, mods);
                // A plain click opens the song; with Command or Shift only the picking changes,
                // unless one song is left picked.
                let open = if mods.command || mods.shift {
                    (state.picked_songs.len() == 1).then(|| state.picked_songs[0])
                } else {
                    Some(index)
                };
                if let Some(open) = open {
                    commands.push(Command::Select(open));
                    state.bible_open = false;
                    state.services.chosen = None;
                }
            }
            // The "more" button opens what a right-click on the row does.
            let near = response.hovered() || ui.rect_contains_pointer(more);
            let button = more_button(ui, more, ui.scope_id().with(("song more", index)), near);
            egui::Popup::menu(&button).show(|ui| song_menu(ui, show, state, commands, index, &mut asks, false));
            response.context_menu(|ui| song_menu(ui, show, state, commands, index, &mut asks, false));
        }
        let ctx = ui.ctx().clone();
        sweep_rows(ui, state, crate::home::Picking::Songs, &rows, shown, list, pointer, down, |picked| {
            egui::DragAndDrop::set_payload(&ctx, SongDrag(picked));
        });
        if total == 0 {
            let empty = if in_library {
                "No songs here yet. Make one, import some, or drag songs onto this library\u{2019}s name."
            } else {
                "No songs yet. Make one, or import some."
            };
            ui.add_space(12.0);
            ui.label(egui::RichText::new(empty).size(12.5).color(TEXT_FAINT));
        } else if shown.is_empty() {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("No songs match.").size(12.5).color(TEXT_FAINT));
        }
    });
    asks.carry_out(show, state);
    // A song found with the find box goes into the library listed (not All Songs, which has
    // it already, and never a passage or verse): the list is outlined in ink while it would.
    if let Some(carried) = Carried::now(ui.ctx()).filter(|carried| carried.found().is_some())
        && pointer.is_some_and(|at| list.contains(at))
    {
        if carried.lands_on(&target) {
            ui.painter().rect_stroke(list, RADIUS, Stroke::new(2.0, TEXT), StrokeKind::Inside);
        } else {
            ui.ctx().set_cursor_icon(egui::CursorIcon::NoDrop);
        }
        if let (true, Some(id), Some(found)) = (let_go_over(ui.ctx(), list), state.library_shown, carried.found())
            && carried.lands_on(&target)
        {
            egui::DragAndDrop::clear_payload(ui.ctx());
            let added = add_to_library(state, id, &carried.song_keys(state));
            // Picked out and opened in the list, as a click on its row does; nothing goes on screen.
            if let Some(song) = found.song {
                state.picking = crate::home::Picking::Songs;
                state.picked_songs = vec![song];
                state.song_anchor = Some(song);
                commands.push(Command::Select(song));
                state.bible_open = false;
                state.services.chosen = None;
            }
            found_added_to_library(ui.ctx(), state, id, added > 0, &found.name);
        }
    }
    // Songs being carried say how many beside the pointer, until they are dropped.
    if let Some(carried) = egui::DragAndDrop::payload::<SongDrag>(ui.ctx()) {
        let words = match carried.0.as_slice() {
            [one] => show.presentations().get(*one).map_or_else(String::new, |song| song.title.clone()),
            many => format!("{} songs", many.len()),
        };
        paint_carried(ui.ctx(), &words);
    }
}
/// The song a playlist names, among the open songs: found by the name its file gave it, or by
/// its title for one saved before it had a file. `None` when it isn't open (or is hidden).
pub fn playlist_song(show: &Show, state: &HomeState, key: &str) -> Option<usize> {
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    let listed = |index: usize| !hidden.contains(&index) && !state.removed_songs.contains(&index);
    let by_title = key.strip_prefix("title:");
    // An id finds its song even where the song is known by another key (the built-in sample,
    // which has no file, is known by its title).
    let by_id = sp_model::Uuid::parse_str(key).ok();
    state
        .song_keys
        .iter()
        .position(|have| have == key)
        .filter(|index| listed(*index))
        .or_else(|| {
            by_id.and_then(|id| {
                show.presentations().iter().enumerate().position(|(at, song)| song.id == id && listed(at))
            })
        })
        // A title only when one song has it: with several, which one is meant can't be told.
        .or_else(|| {
            let title = by_title?;
            let mut with_it =
                (0..show.presentations().len()).filter(|at| listed(*at) && show.presentations()[*at].title == title);
            let first = with_it.next()?;
            with_it.next().is_none().then_some(first)
        })
        .filter(|index| listed(*index))
}

/// The slide Edit opens: in the song chosen in the Show view (never another song that happens
/// to be on screen, or on the Announcements layer), the slide picked out with a right-click,
/// or else the slide on screen if it is in that song, or else the song's first slide.
pub fn slide_to_edit(show: &Show, state: &HomeState) -> SlideRef {
    let song = show.selected();
    state
        .selected_slide
        .filter(|at| at.presentation == song)
        .or_else(|| show.cursor().filter(|at| at.presentation == song))
        .unwrap_or(SlideRef { presentation: song, slide: 0 })
}

/// The slide Edit opens on, so that Edit (Command-3, its key in the top bar, View › Edit)
/// always opens something when there is a song: [`slide_to_edit`] when the song chosen can be
/// edited; when it can't (it has no slides, or it is a Bible passage or the VoxLive item, which
/// the app makes), the slide on screen or last on screen when that is in a song that can, then
/// the first song of the playlist in front, then the first song there is. `None` with no song
/// that has a slide.
pub fn song_to_edit(show: &Show, state: &HomeState) -> Option<SlideRef> {
    let editable = |at: SlideRef| {
        !state.app_made(at.presentation)
            && !state.removed_songs.contains(&at.presentation)
            && show.presentations().get(at.presentation).is_some_and(|song| song.slide(at.slide).is_some())
    };
    let first = |song: usize| Some(SlideRef { presentation: song, slide: 0 }).filter(|at| editable(*at));
    let playlist = state.services.front().items.iter().filter_map(|item| match item {
        crate::service::Item::Song { key, .. } => playlist_song(show, state, key),
        _ => None,
    });
    Some(slide_to_edit(show, state))
        .filter(|at| editable(*at))
        .or_else(|| show.live().or(show.cursor()).filter(|at| editable(*at)))
        .or_else(|| playlist.into_iter().find_map(first))
        .or_else(|| (0..show.presentations().len()).find_map(first))
}

/// A question asked before something is deleted, drawn as a macOS alert: a small rounded panel
/// in the middle of the window over a dimmed app, an icon at the top, the question in bold and
/// what it means under it, centred, and two buttons side by side. Return answers with
/// `confirm` (drawn in red, as macOS draws a destructive button); Escape cancels. `None` until
/// it is answered.
pub fn alert(ctx: &egui::Context, id: &str, icon: Icon, title: &str, message: &str, confirm: &str) -> Option<bool> {
    const WIDTH: f32 = 268.0;
    let red = Color32::from_rgb(255, 99, 88);
    // It fades and settles in quickly, as a macOS alert does.
    let shown = ctx.animate_bool_with_time(egui::Id::unique(("alert shown", id)), true, 0.14);
    let frame = Frame::new()
        .fill(Color32::from_rgb(44, 44, 46))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(14.0)
        .inner_margin(egui::Margin { left: 18, right: 18, top: 20, bottom: 16 })
        .shadow(egui::Shadow { offset: [0, 14], blur: 44, spread: 0, color: Color32::from_black_alpha(150) });
    let area = egui::Modal::default_area(egui::Id::unique(("alert", id)))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -30.0 + 8.0 * (1.0 - shown)));
    let response = egui::Modal::new(egui::Id::unique(("alert", id)))
        .area(area)
        .backdrop_color(Color32::from_black_alpha((80.0 * shown) as u8))
        .frame(frame)
        .show(ctx, |ui| {
            ui.multiply_opacity(shown);
            ui.set_width(WIDTH - 36.0);
            let mut answer = None;
            ui.vertical_centered(|ui| {
                // The icon, in a rounded tile as an app's icon is.
                let (tile, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                ui.painter().rect_filled(tile, 13.0, Color32::from_rgb(68, 68, 72));
                ui.painter().rect_stroke(
                    tile,
                    13.0,
                    Stroke::new(1.0, Color32::from_white_alpha(18)),
                    StrokeKind::Inside,
                );
                icons::paint(ui.painter(), icon, Rect::from_center_size(tile.center(), vec2(30.0, 30.0)), TEXT);
                ui.add_space(12.0);
                ui.add(
                    egui::Label::new(egui::RichText::new(title).font(semibold(13.5)).color(Color32::WHITE))
                        .halign(Align::Center)
                        .wrap(),
                );
                if !message.is_empty() {
                    ui.add_space(5.0);
                    ui.add(
                        egui::Label::new(egui::RichText::new(message).size(11.5).color(TEXT_DIM))
                            .halign(Align::Center)
                            .wrap(),
                    );
                }
                ui.add_space(16.0);
            });
            // Two buttons side by side, each half the width: Cancel, then the answer.
            let gap = 8.0;
            let size = vec2((ui.available_width() - gap) / 2.0, 28.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, yes) in [("Cancel", false), (confirm, true)] {
                    let (place, button) = ui.allocate_exact_size(size, Sense::click());
                    let fill = match (yes, button.is_pointer_button_down_on(), button.hovered()) {
                        (_, true, _) => Color32::from_white_alpha(52),
                        (true, _, _) | (_, _, true) => Color32::from_white_alpha(34),
                        _ => Color32::from_white_alpha(22),
                    };
                    ui.painter().rect_filled(place, 8.0, fill);
                    // The answer Return gives has a faint ring, as the default button does.
                    if yes {
                        ui.painter().rect_stroke(
                            place,
                            8.0,
                            Stroke::new(1.0, Color32::from_white_alpha(30)),
                            StrokeKind::Inside,
                        );
                    }
                    let ink = if yes { red } else { Color32::WHITE };
                    let font = if yes { semibold(13.0) } else { FontId::proportional(13.0) };
                    ui.painter().text(place.center(), Align2::CENTER_CENTER, label, font, ink);
                    if button.clicked() {
                        answer = Some(yes);
                    }
                }
            });
            answer
        });
    let (enter, escape) =
        ctx.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
    if shown < 1.0 {
        ctx.request_repaint();
    }
    let answer = response.inner.or(if escape {
        Some(false)
    } else if enter {
        Some(true)
    } else {
        None
    });
    if answer.is_some() {
        // The next alert fades in again.
        ctx.animate_bool_with_time(egui::Id::unique(("alert shown", id)), false, 0.0);
    }
    answer
}

/// Carries on a drag across a list's rows (see `Sweep`): while the pointer is in the list,
/// everything from the row it started on to the row under the pointer is picked out, and the
/// list scrolls when the pointer nears its top or bottom; once the pointer leaves the list,
/// what is picked is handed to `carry`. The drag ends when the button is let go.
#[allow(clippy::too_many_arguments)]
fn sweep_rows(
    ui: &mut egui::Ui,
    state: &mut HomeState,
    which: crate::home::Picking,
    rows: &[(usize, Rect)],
    order: &[usize],
    list: Rect,
    pointer: Option<egui::Pos2>,
    down: bool,
    carry: impl FnOnce(Vec<usize>),
) {
    let Some(mut sweep) = state.sweep.filter(|sweep| sweep.list == which) else { return };
    if !down {
        state.sweep = None;
        return;
    }
    let Some(at) = pointer else { return };
    if sweep.carried {
        return;
    }
    if !list.contains(at) {
        let picked = if state.picked_songs.is_empty() || which != crate::home::Picking::Songs {
            sweep.from.into_iter().collect()
        } else {
            // In the list's order, whatever order they were picked in.
            let picked: std::collections::HashSet<usize> = state.picked_songs.iter().copied().collect();
            order.iter().copied().filter(|at| picked.contains(at)).collect()
        };
        carry(picked);
        sweep.carried = true;
        state.sweep = Some(sweep);
        return;
    }
    if sweep.keeps || at.distance(sweep.start) < 3.0 {
        return;
    }
    // The row under the pointer, or the first or last in view when it is above or below them
    // all. `rows` are the rows in view; `order` is every row of the list, in order, so a sweep
    // goes on picking when the row it started on has scrolled out of view.
    let under = rows.iter().find(|(_, row)| at.y < row.max.y + 1.0).or(rows.last()).map(|(index, _)| *index);
    if let (Some(from), Some(to)) = (sweep.from.and_then(|f| order.iter().position(|i| *i == f)), under)
        && let Some(to) = order.iter().position(|i| *i == to)
    {
        let (a, b) = (from.min(to), from.max(to));
        state.picked_songs = order[a..=b].to_vec();
    }
    // Near the top or bottom, the list scrolls on.
    let edge = 24.0;
    if at.y < list.min.y + edge {
        ui.scroll_with_delta(vec2(0.0, 8.0));
        ui.ctx().request_repaint();
    } else if at.y > list.max.y - edge {
        ui.scroll_with_delta(vec2(0.0, -8.0));
        ui.ctx().request_repaint();
    }
}

/// The playlists a song's menu offers to add it to, by their places: all of them, except the
/// playlist in front when the menu is for one of its items (the song is in it already).
fn playlists_to_add_to(state: &HomeState, in_playlist: bool) -> Vec<usize> {
    let front = state.services.front_at();
    (0..state.services.list.len()).filter(|at| !(in_playlist && *at == front)).collect()
}

/// The songs a song's menu acts on (Export…, Delete…): in a library, the songs picked out with
/// it when it is one of them, else this song alone; from a playlist (an item or its title bar),
/// this song alone, whatever was picked out in a library before.
fn songs_menu_acts_on(state: &HomeState, index: usize, in_playlist: bool) -> Vec<usize> {
    if !in_playlist && state.picked_songs.contains(&index) { state.picked_songs.clone() } else { vec![index] }
}

/// A song's right-click menu, in the library or (`in_playlist`) for an item of the playlist in
/// front, which then isn't offered as a playlist to add it to, and acts on this song alone. From
/// a playlist its Delete is "Delete Song from Library…", at the foot of the item's menu (see
/// [`delete_song_from_library`]), so it is never taken for Remove from Playlist.
pub fn song_menu(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    commands: &mut Vec<Command>,
    index: usize,
    asks: &mut SongAsks,
    in_playlist: bool,
) {
    let front = state.services.front().name.clone();
    if !in_playlist && ui.button(format!("Add to {front}")).clicked() {
        asks.add = Some(index);
        ui.close();
    }
    // Another playlist, or a new one.
    ui.menu_button("Add to Playlist", |ui| {
        for at in playlists_to_add_to(state, in_playlist) {
            let Some(playlist) = state.services.list.get(at) else { continue };
            if ui.button(playlist.name.as_str()).clicked() {
                asks.add_to = Some((index, Some(at)));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("New Playlist\u{2026}").clicked() {
            asks.add_to = Some((index, None));
            ui.close();
        }
    });
    // The libraries the song is in.
    if let Some(key) = state.song_keys.get(index).cloned() {
        ui.menu_button("Libraries", |ui| {
            if state.libraries.is_empty() {
                ui.label(
                    egui::RichText::new("No libraries yet. Make one where playlists and libraries are chosen.")
                        .color(TEXT_DIM),
                );
            }
            for library in &mut state.libraries {
                if ui.add(egui::Button::selectable(library.has(&key), library.name.as_str())).clicked() {
                    library.toggle(&key);
                    state.libraries_changed = true;
                }
            }
        });
    }
    ui.separator();
    if ui.button("Rename").clicked() {
        commands.push(Command::Select(index));
        state.bible_open = false;
        state.editing = None;
        state.renaming_song = show.presentations().get(index).map(|song| song.title.clone());
        state.rename_started = true;
        ui.close();
    }
    if ui.button("Duplicate").clicked() {
        state.song_request = Some((index, crate::home::SongRequest::Duplicate));
        ui.close();
    }
    if ui.button("Export…").on_hover_text("Save as a song pack, to open on another computer").clicked() {
        state.export_request = Some(crate::home::ExportRequest::Songs(songs_menu_acts_on(state, index, in_playlist)));
        ui.close();
    }
    // Next on the last slide goes back to the first.
    let loops = show.presentations().get(index).is_some_and(|song| song.loops);
    if ui
        .add(egui::Button::selectable(loops, "Loop to First Slide"))
        .on_hover_text("Next on the last slide goes back to the first, for a loop of announcements")
        .clicked()
    {
        state.song_loop_request = Some(index);
        ui.close();
    }
    // The Announcements layer: the song's slides go round by themselves over everything else.
    announce_button(
        ui,
        state,
        crate::announcements::Announce::Song(index),
        "Its slides go round by themselves over the slides (each for its Go to Next time, or 8 seconds)",
    );
    // How its slides give way to each other, unless a slide has its own.
    let own_transition = show.presentations().get(index).and_then(|song| song.transition);
    ui.menu_button("Transition", |ui| {
        if ui
            .add(egui::Button::selectable(own_transition.is_none(), usual_transition(state.transition)))
            .on_hover_text("The transition chosen under the slides")
            .clicked()
        {
            state.song_transition_request = Some((index, None));
            ui.close();
        }
        ui.separator();
        let mut look = own_transition.unwrap_or(state.transition);
        let before = look;
        transition_picker(ui, &mut look, "song");
        if look != before || (own_transition.is_none() && ui.button("Use This for the Song").clicked()) {
            state.song_transition_request = Some((index, Some(look)));
        }
    });
    // The theme this song is drawn with.
    let own = show.presentations().get(index).and_then(|song| song.theme);
    ui.menu_button("Theme", |ui| {
        if ui
            .add(egui::Button::selectable(own.is_none(), "Main Theme"))
            .on_hover_text("The theme songs use unless they set their own")
            .clicked()
        {
            state.song_theme_request = Some((index, None));
            ui.close();
        }
        ui.separator();
        for (at, name) in state.theme_names.iter().enumerate() {
            let on = own.is_some() && state.theme_ids.get(at).copied() == own;
            if ui.add(egui::Button::selectable(on, name.as_str())).clicked() {
                state.song_theme_request = Some((index, Some(at)));
                ui.close();
            }
        }
    });
    if ui.button("Edit Words").clicked()
        && let Some(song) = show.presentations().get(index)
    {
        commands.push(Command::Select(index));
        state.bible_open = false;
        state.editing = None;
        state.reflow = Some((index, sp_model::to_song_text(song)));
        ui.close();
    }
    // In a library, Delete deletes every song picked out, when this is one of them.
    if !in_playlist && ui.button("Delete…").clicked() {
        state.confirm_delete_songs = songs_menu_acts_on(state, index, in_playlist);
        ui.close();
    }
}

/// The last choice of a playlist item's menu when the item is a song: deletes the song itself
/// (from the library, every playlist and every library), after asking. After Remove from
/// Playlist, which only takes the item out.
pub fn delete_song_from_library(ui: &mut egui::Ui, state: &mut HomeState, song: usize) {
    let hint = "Deletes the song itself, not just this item, after asking";
    if ui.button("Delete Song from Library\u{2026}").on_hover_text(hint).clicked() {
        state.confirm_delete_songs = songs_menu_acts_on(state, song, true);
        ui.close();
    }
}

/// A field over a row, for retyping its name. Returns `Some(Some(name))` when Return is pressed
/// or the field is left, and `Some(None)` on Escape.
fn rename_in_row(
    ui: &mut egui::Ui,
    typed: &mut String,
    look: &Lettering,
    at: egui::Pos2,
    right: f32,
    focus: bool,
) -> Option<Option<String>> {
    let field = rename_field_at(ui, typed, look, at, right);
    if focus {
        field.request_focus();
    }
    let (enter, escape) = ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
    if escape {
        Some(None)
    } else if enter || field.lost_focus() {
        Some(Some(typed.clone()))
    } else {
        None
    }
}

/// A search field and the songs whose titles have what is typed in them, to choose one (to
/// fill a placeholder). Returns the song clicked, by its place among the songs.
pub fn song_picker(ui: &mut egui::Ui, show: &Show, state: &HomeState) -> Option<usize> {
    let id = ui.make_persistent_id("song picker");
    let mut typed = ui.data_mut(|data| data.get_temp::<String>(id)).unwrap_or_default();
    let field = ui.add(egui::TextEdit::singleline(&mut typed).hint_text("Search songs").desired_width(220.0));
    if typed.is_empty() && !field.has_focus() {
        field.request_focus();
    }
    let wanted = typed.trim().to_lowercase();
    ui.data_mut(|data| data.insert_temp(id, typed));
    let hidden = state.hidden_slots();
    let mut picked = None;
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        let mut shown = 0;
        for (index, song) in show.presentations().iter().enumerate() {
            if hidden.contains(&index) || state.removed_songs.contains(&index) || state.song_keys.get(index).is_none() {
                continue;
            }
            if !wanted.is_empty() && !song.title.to_lowercase().contains(&wanted) {
                continue;
            }
            if ui.add(egui::Button::new(song.title.as_str()).frame(false)).clicked() {
                picked = Some(index);
            }
            shown += 1;
            if shown == 40 {
                break;
            }
        }
        if shown == 0 {
            ui.label(egui::RichText::new("No song has that in its title.").color(TEXT_DIM));
        }
    });
    if picked.is_some() {
        ui.data_mut(|data| data.remove::<String>(id));
    }
    picked
}

/// What a playlist item's row, its bar over the slides or its menu asks for, carried out
/// once the frame is drawn.
pub enum ItemAsk {
    /// Open the item: it becomes the open one and the slides scroll to it. Nothing changes on
    /// the screens.
    Open(usize),
    Earlier(usize),
    Later(usize),
    Remove(usize),
    Rename(usize),
    /// A placeholder filled with a song, by its place among the songs.
    Fill(usize, usize),
    /// A Bible passage opened on the Bible page, with the translations and options it was
    /// kept with, to change it or to read round it.
    Bible(usize),
}

/// A playlist item's menu, from its "more" button or a right-click: for a song, the song's
/// own menu under Song; then what [`item_moves`] offers.
#[allow(clippy::too_many_arguments)]
pub fn item_menu(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    commands: &mut Vec<Command>,
    index: usize,
    ask: &mut Option<ItemAsk>,
    asks: &mut SongAsks,
) {
    use crate::service::Item;
    let song = match state.services.front().items.get(index) {
        Some(Item::Song { key, .. }) => playlist_song(show, state, key).filter(|at| !state.app_made(*at)),
        _ => None,
    };
    if let Some(song) = song {
        ui.menu_button("Song", |ui| song_menu(ui, show, state, commands, song, asks, true));
        ui.separator();
    }
    let announced = match state.services.front().items.get(index) {
        Some(Item::Media { id, .. }) => Some(crate::announcements::Announce::Media(*id)),
        Some(Item::MediaPlaylist { id, .. }) => Some(crate::announcements::Announce::MediaPlaylist(*id)),
        _ => None,
    };
    if let Some(what) = announced {
        announce_button(ui, state, what, "It goes round by itself over the slides until it is cleared");
        ui.separator();
    }
    version_section(ui, state, index);
    item_moves(ui, show, state, index, ask);
    if let Some(song) = song {
        delete_song_from_library(ui, state, song);
    }
}

/// A Version section in a playlist item's menu, once some theme has a version: every version,
/// the one it shows in marked, picked with one click, then a separator. Nothing otherwise.
pub fn version_section(ui: &mut egui::Ui, state: &mut HomeState, index: usize) {
    if can_show_with(state, index) {
        crate::live_tabs::menu_heading(ui, "Version");
        show_with_menu(ui, state, index, false);
        ui.separator();
    }
}

/// Whether a playlist item (by its place in the playlist in front) can be shown in a theme
/// version: a song or a passage, once some theme has a version.
pub fn can_show_with(state: &HomeState, index: usize) -> bool {
    !state.versions.is_empty()
        && state.services.front().items.get(index).is_some_and(crate::service::Item::can_show_with)
}

/// What a playlist item shows in when it goes on screen: the usual look or one of the themes'
/// versions, and whether that version stays on after it. It is the playlist's own; the song
/// itself doesn't change. The menu of the item's version button (`heading`, with a line saying
/// it applies in this playlist only), and the Version section of its menus.
pub fn show_with_menu(ui: &mut egui::Ui, state: &mut HomeState, index: usize, heading: bool) {
    use crate::service::ShowWith;
    let Some(item) = state.services.front().items.get(index) else { return };
    let now = item.show_with().filter(|wanted| state.versions.iter().any(|version| version.id == wanted.version));
    if heading {
        crate::live_tabs::menu_heading(ui, "When this item goes on screen, show it in");
    }
    let mut chosen = None;
    if ui.add(egui::Button::selectable(now.is_none(), "The usual look")).clicked() {
        chosen = Some(None);
    }
    for version in &state.versions {
        let on = now.is_some_and(|wanted| wanted.version == version.id);
        if ui.add(egui::Button::selectable(on, format!("The {} version", version.name))).clicked() {
            chosen = Some(Some(ShowWith { version: version.id, keep: now.is_some_and(|wanted| wanted.keep) }));
        }
    }
    ui.separator();
    let keep = now.is_some_and(|wanted| wanted.keep);
    let kept = ui
        .add_enabled(now.is_some(), egui::Button::selectable(keep, "Keep It After This Item"))
        .on_hover_text("The version stays on when the next item goes on screen, until it is switched back");
    if kept.clicked() {
        chosen = Some(now.map(|wanted| ShowWith { keep: !keep, ..wanted }));
    }
    if heading {
        ui.add_enabled(
            false,
            egui::Button::new("It applies in this playlist only; the song itself doesn\u{2019}t change"),
        );
    }
    if let Some(chosen) = chosen {
        state.services.set_show_with(index, chosen);
        ui.close();
    }
}

/// Play on Announcements Layer, for a song, a picture, a video or a media playlist; Stop
/// Announcements while it plays there.
pub fn announce_button(ui: &mut egui::Ui, state: &mut HomeState, what: crate::announcements::Announce, hint: &str) {
    let announcing = state.announcement_live.as_ref().is_some_and(|(live, _)| *live == what);
    let (label, hint) = if announcing {
        ("Stop Announcements", "Take it off the Announcements layer")
    } else {
        ("Play on Announcements Layer", hint)
    };
    if ui.button(label).on_hover_text(hint).clicked() {
        state.announcement_request = Some((!announcing).then_some(what));
        ui.close();
    }
}

/// The part of a playlist item's menu that is about its place in the playlist: filling a
/// placeholder, renaming a header or a placeholder, moving it up or down, taking it out.
pub fn item_moves(ui: &mut egui::Ui, show: &Show, state: &HomeState, index: usize, ask: &mut Option<ItemAsk>) {
    use crate::service::Item;
    let items = &state.services.front().items;
    let item = items.get(index);
    if matches!(item, Some(Item::Placeholder { .. })) {
        ui.menu_button("Fill with Song", |ui| {
            if let Some(song) = song_picker(ui, show, state) {
                *ask = Some(ItemAsk::Fill(index, song));
                ui.close();
            }
        });
    }
    if matches!(item, Some(Item::Passage(_))) {
        if ui
            .button("Open on the Bible Page")
            .on_hover_text("With the translations and options it was kept with")
            .clicked()
        {
            *ask = Some(ItemAsk::Bible(index));
            ui.close();
        }
        ui.separator();
    }
    if matches!(item, Some(Item::Placeholder { .. } | Item::Header { .. })) {
        if ui.button("Rename").clicked() {
            *ask = Some(ItemAsk::Rename(index));
            ui.close();
        }
        ui.separator();
    }
    if ui.add_enabled(index > 0, egui::Button::new("Move Up")).clicked() {
        *ask = Some(ItemAsk::Earlier(index));
        ui.close();
    }
    if ui.add_enabled(index + 1 < items.len(), egui::Button::new("Move Down")).clicked() {
        *ask = Some(ItemAsk::Later(index));
        ui.close();
    }
    ui.separator();
    // With several picked out, this one among them, they all go.
    let picks = &state.item_picks;
    let label = if picks.several() && picks.places.contains(&index) {
        format!("Remove {} Items from Playlist", picks.places.len())
    } else {
        "Remove from Playlist".to_owned()
    };
    if ui.button(label).on_hover_text("Takes it out of this playlist only").clicked() {
        *ask = Some(ItemAsk::Remove(index));
        ui.close();
    }
}

/// The playlist items Delete takes out: those picked out, or else the one open.
pub fn items_to_take_out(state: &mut HomeState) -> Vec<usize> {
    state.item_picks.check(&state.services);
    if state.item_picks.places.is_empty() {
        state.services.chosen.into_iter().collect()
    } else {
        state.item_picks.places.clone()
    }
}

/// Carries out what a playlist item's row, bar or menu asked for.
pub fn carry_out_item_ask(show: &Show, state: &mut HomeState, commands: &mut Vec<Command>, ask: ItemAsk) {
    use crate::service::Item;
    match ask {
        ItemAsk::Rename(index) => {
            let name = state.services.front().items.get(index).map(|item| item.title().to_owned());
            state.renaming_header = Some((index, name.unwrap_or_default()));
            state.rename_started = true;
        }
        ItemAsk::Open(index) => {
            let Some(item) = state.services.front().items.get(index).cloned() else { return };
            state.services.chosen = Some(index);
            // Delete then takes it (or the items picked with it) out of the playlist.
            state.picking = crate::home::Picking::Items;
            // The slides scroll to it, even if it was the one last scrolled to.
            state.slides_jump_to = Some(index);
            match item {
                Item::Song { key, .. } => {
                    if let Some(at) = playlist_song(show, state, &key) {
                        commands.push(Command::Select(at));
                        state.bible_open = false;
                    }
                }
                // Shown with the rest of the playlist: a passage as its slides, a picture, a
                // video or a media playlist as a tile to click to put it on the screens (a
                // placeholder shows nothing). Opening changes nothing on the screens, and
                // nothing on the Bible page.
                Item::Passage(_) | Item::Media { .. } | Item::MediaPlaylist { .. } | Item::Placeholder { .. } => {
                    state.bible_open = false;
                }
                Item::Header { .. } => {}
            }
        }
        ItemAsk::Bible(index) => {
            if let Some(Item::Passage(saved)) = state.services.front().items.get(index).cloned() {
                state.services.chosen = Some(index);
                state.bible_open = true;
                state.bible.open_saved(&saved, show.live().map(|at| at.presentation), crate::home::SlidesOf::Page);
            }
        }
        ItemAsk::Fill(index, song) => {
            if let (Some(key), Some(found)) = (state.song_keys.get(song), show.presentations().get(song)) {
                let item = Item::song(key.clone(), found.title.clone());
                if state.services.fill(index, item) {
                    commands.push(Command::Select(song));
                    state.bible_open = false;
                }
            }
        }
        ItemAsk::Earlier(index) => {
            state.services.nudge(index, true);
        }
        ItemAsk::Later(index) => {
            state.services.nudge(index, false);
        }
        // With several picked out, this one among them, they all go.
        ItemAsk::Remove(index) => {
            let picks = &state.item_picks;
            let places =
                if picks.several() && picks.places.contains(&index) { picks.places.clone() } else { vec![index] };
            state.take_out_items(state.services.current, &places);
        }
    }
}

/// A drag starting on a playlist row: on one of several picked out it moves them all; on any
/// other row it picks that row alone, as the Finder does.
fn picked_to_move(state: &mut HomeState, index: usize) {
    if !state.item_picks.places.contains(&index) {
        state.item_picks.places = vec![index];
        state.item_picks.anchor = Some(index);
        state.item_picks.made_in(&state.services);
    }
}

/// Whether ↑ and ↓ go through the song list or the playlist: in the Show view, with nothing
/// being typed and the song search closed.
pub fn list_keys_work(ui: &egui::Ui, state: &HomeState) -> bool {
    // Not under an open menu, where the arrows move between its choices, nor under the list of
    // keys.
    !ui.ctx().text_edit_focused()
        && state.library_search.is_none()
        && state.edit_kind().is_none()
        && !state.bible_open
        && !state.key_sheet
        && !egui::Popup::is_any_open(ui.ctx())
}

/// The items of the playlist in front, in running order: each with its number, its title, a
/// small mark for a Bible passage, a video or picture, a media playlist or a placeholder, the
/// tally dot on the one on screen, and a "more" button; headers are small capital lines. Rows
/// are dragged up and down to change the order.
fn playlist_items(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    let removed = state.removed_songs.clone();
    let listed = |index: usize| !hidden.contains(&index) && !removed.contains(&index);
    // A song in a playlist is found by the name its file gave it; one saved before it had a
    // file is found by its title.
    let song_at = |keys: &[String], key: &str| {
        let by_title = key.strip_prefix("title:");
        keys.iter()
            .position(|have| have == key)
            .or_else(|| by_title.and_then(|title| show.presentations().iter().position(|song| song.title == title)))
            .filter(|index| listed(*index))
    };
    let items = state.services.front().items.clone();
    // Each song item's song and library, looked up again only when the songs, the items or
    // the libraries change, not for every row on every frame.
    let epoch = state.lists.songs.epoch();
    let kept = state
        .lists
        .playlist_rows
        .get(|(made_in, made_of, libraries)| *made_in == epoch && *made_of == items && *libraries == state.libraries);
    let looked_up = match kept {
        Some(looked_up) => looked_up,
        None => {
            let looked_up = items
                .iter()
                .map(|item| match item {
                    Item::Song { key, .. } => {
                        (song_at(&state.song_keys, key), library_of(state, key).map(str::to_owned))
                    }
                    _ => (None, None),
                })
                .collect();
            state.lists.playlist_rows.keep((epoch, items.clone(), state.libraries.clone()), looked_up)
        }
    };
    let live_in = show.live().map(|at| at.presentation);
    // The passage on screen, by its slides' title and as its place keeps it (see `holds_kept`).
    let live_passage = live_in.filter(|at| hidden.contains(at)).and_then(|at| {
        let title = show.presentations().get(at)?.title.clone();
        let kept = state.bible.passage_held(at, &title).0;
        Some((title, kept))
    });
    let shows_passage =
        |lookup: &str| live_passage.as_ref().is_some_and(|(title, kept)| lookup == title || lookup == kept);
    let filtering = !state.filter.trim().is_empty();
    // Items picked out let go once the playlist has changed under them.
    state.item_picks.check(&state.services);
    let mods = ui.input(|input| input.modifiers);

    // What a click on a row asks for, carried out after the list is drawn.
    let mut ask: Option<ItemAsk> = None;
    let mut asks = SongAsks::default();
    // Rows can be dragged up and down to change the order (not while the list is filtered).
    let mut rows: Vec<Rect> = Vec::new();
    // The item each row is, in step with `rows`.
    let mut row_items: Vec<usize> = Vec::new();
    let mut dragging = None;
    let mut dropped = None;
    // The row clicked, picked out once every row is known (a Shift-click picks a run of them).
    let mut clicked = None;
    let mut renamed: Option<Option<(usize, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    // The row of the item on screen is brought into view when another item goes on screen.
    let on_screen = items.iter().enumerate().position(|(index, item)| match item {
        Item::Song { .. } => live_in.is_some() && looked_up.get(index).and_then(|(found, _)| *found) == live_in,
        Item::Passage(saved) => shows_passage(&saved.lookup),
        Item::Media { id, .. } => show.live_media().and_then(|at| show.media().get(at)).is_some_and(|m| m.id == *id),
        Item::MediaPlaylist { id, .. } => state.media_playing == Some(*id),
        Item::Header { .. } | Item::Placeholder { .. } => false,
    });
    let follow = if on_screen != state.live_row_seen {
        state.live_row_seen = on_screen;
        on_screen
    } else {
        None
    };
    let list = egui::ScrollArea::vertical().id_salt("playlist items").auto_shrink([false, false]).show(ui, |ui| {
        if items.is_empty() {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(
                    "Nothing yet. Use Add to playlist below, drag songs here from a library, or drag a \
                     picture or video here from the media strip.",
                )
                .size(12.5)
                .color(TEXT_FAINT),
            );
        }
        let mut number = 0;
        for (index, item) in items.iter().enumerate() {
            let is_header = matches!(item, Item::Header { .. });
            if !is_header {
                number += 1;
            }
            if filtering && !matches_filter(item.title(), &state.filter) {
                continue;
            }
            // What the row's menu offers, from its "more" button or a right-click.
            let song = looked_up.get(index).and_then(|(found, _)| *found);
            let mut menu =
                |ui: &mut egui::Ui, state: &mut HomeState, ask: &mut Option<ItemAsk>, asks: &mut SongAsks| {
                    item_menu(ui, show, state, commands, index, ask, asks);
                };
            // A header: a small capital line, renamed in place.
            if let Item::Header { name } = item {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.5), Sense::click_and_drag());
                rows.push(row);
                row_items.push(index);
                if state.item_picks.several() && state.item_picks.places.contains(&index) {
                    ui.painter().rect_filled(row, RADIUS, CARD);
                }
                let at = pos2(row.min.x + 10.0, row.min.y + 21.75);
                match state.renaming_header.as_mut().filter(|(at, _)| *at == index) {
                    Some((_, typed)) => {
                        let focus = std::mem::take(&mut rename_started);
                        let look = Lettering::cap(TEXT_DIM);
                        if let Some(done) = rename_in_row(ui, typed, &look, at, row.max.x - 4.0, focus) {
                            renamed = Some(done.map(|name| (index, name)));
                        }
                    }
                    None => {
                        cap(ui, at, Align2::LEFT_CENTER, name, TEXT_DIM);
                    }
                }
                if response.drag_started() && !filtering {
                    picked_to_move(state, index);
                }
                if response.dragged() && !filtering {
                    dragging = Some(index);
                }
                if drag_let_go(&response) && !filtering {
                    dropped = Some(index);
                }
                if response.double_clicked() {
                    ask = Some(ItemAsk::Rename(index));
                } else if response.clicked() {
                    clicked = Some(index);
                }
                response.context_menu(|ui| menu(ui, state, &mut ask, &mut asks));
                continue;
            }
            // Each kind's mark; whether what it names is still here, and on screen now.
            let (icon, missing, live, gone) = match item {
                Item::Song { .. } => {
                    let live = song.is_some() && song == live_in;
                    (None, song.is_none(), live, "This song is not open in the library")
                }
                Item::Passage(saved) => {
                    let live = shows_passage(&saved.lookup);
                    (Some(Icon::Book), false, live, "")
                }
                Item::Media { id, .. } => {
                    let found = show.media().iter().position(|media| media.id == *id);
                    let video = found.is_some_and(|at| crate::media::is_video(&show.media()[at].path));
                    let icon = if video { Icon::Film } else { Icon::Picture };
                    let live = found.is_some() && found == show.live_media();
                    (Some(icon), found.is_none(), live, "This picture or video is no longer in the media bin")
                }
                Item::MediaPlaylist { id, .. } => {
                    let found = state.media_playlists.iter().any(|playlist| playlist.id == *id);
                    let live = state.media_playing == Some(*id);
                    (Some(Icon::Loop), !found, live, "This media playlist has been deleted")
                }
                // Not missing, but faint: a place kept for something to come.
                Item::Placeholder { .. } => (Some(Icon::Target), true, false, ""),
                Item::Header { .. } => continue,
            };
            let chosen = state.services.chosen == Some(index);
            let renaming = state
                .renaming_header
                .as_ref()
                .is_some_and(|(at, _)| *at == index && matches!(item, Item::Placeholder { .. }));
            // The version it shows in, lit while it is on screen in it.
            let version = item.show_with().and_then(|wanted| {
                let found = state.versions.iter().find(|version| version.id == wanted.version)?;
                Some((found.name.as_str(), live && state.version_switch.lights(index, found.id)))
            });
            let look = ListRow {
                number: Some(number),
                title: if renaming { "" } else { item.title() },
                chosen,
                lit: state.item_picks.several() && state.item_picks.places.contains(&index),
                live,
                faint: missing,
                icon,
                version,
                ..ListRow::default()
            };
            let (mut response, more) = list_row(ui, &look);
            // A placeholder is renamed in place, as a header is.
            if renaming && let Some((_, typed)) = state.renaming_header.as_mut() {
                let row = response.rect;
                let focus = std::mem::take(&mut rename_started);
                let at = pos2(row.min.x + 48.0, row.center().y);
                // Faint, as a placeholder's name is, and short of its mark at the right.
                let look = Lettering::plain(FontId::proportional(13.5), TEXT_FAINT.gamma_multiply(0.7));
                let right = more.min.x - 10.0 - 14.0 - 8.0;
                if let Some(done) = rename_in_row(ui, typed, &look, at, right, focus) {
                    renamed = Some(done.map(|name| (index, name)));
                }
            }
            rows.push(response.rect);
            row_items.push(index);
            if response.drag_started() && !filtering {
                picked_to_move(state, index);
            }
            if response.dragged() && !filtering {
                dragging = Some(index);
            }
            if drag_let_go(&response) && !filtering {
                dropped = Some(index);
            }
            // The library a song is in is said when the pointer rests on its row.
            if let Some(library) = looked_up.get(index).and_then(|(_, library)| library.as_deref()) {
                response = response.on_hover_text(format!("In the library {library}"));
            }
            if missing && !gone.is_empty() {
                response = response.on_hover_text(gone);
            }
            if matches!(item, Item::Placeholder { .. }) {
                response = response.on_hover_text(
                    "A place kept for something to come: fill it with a song from its menu, or drag a \
                     picture or video onto it",
                );
            }
            if response.double_clicked() && matches!(item, Item::Placeholder { .. }) {
                ask = Some(ItemAsk::Rename(index));
            }
            if response.clicked() {
                clicked = Some(index);
            }
            // Gone to with ↑ or ↓, or just gone on screen: kept in sight, at once.
            if state.playlist_scroll_to == Some(index) || follow == Some(index) {
                response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
                if state.playlist_scroll_to == Some(index) {
                    state.playlist_scroll_to = None;
                }
            }
            let near = response.hovered() || ui.rect_contains_pointer(more);
            let button = more_button(ui, more, ui.scope_id().with(("item more", index)), near);
            egui::Popup::menu(&button).show(|ui| menu(ui, state, &mut ask, &mut asks));
            response.context_menu(|ui| menu(ui, state, &mut ask, &mut asks));
        }
    });
    asks.carry_out(show, state);
    // Picked out as in the Finder: a click picks the item alone, and opens it (a header opens
    // nothing; the slides go to it); Command-click adds or takes away one, Shift-click picks
    // every row from the one last clicked to it. With Command or Shift, only the picking
    // changes, unless one item is left picked. (A click comes when the button is let go, so a
    // drag of several picked items keeps them all.)
    if let Some(index) = clicked {
        let picks = &mut state.item_picks;
        if picks.anchor.is_none() {
            picks.anchor = state.services.chosen;
        }
        crate::home::pick(&mut picks.places, &mut picks.anchor, &row_items, index, mods);
        picks.made_in(&state.services);
        let several = mods.command || mods.shift;
        let open = if several { (picks.places.len() == 1).then(|| picks.places[0]) } else { Some(index) };
        match open {
            Some(open) if matches!(items.get(open), Some(Item::Header { .. })) => state.slides_jump_to = Some(open),
            Some(open) => ask = Some(ItemAsk::Open(open)),
            None => {}
        }
        if several {
            state.picking = crate::home::Picking::Items;
        }
    }
    // Command-A (Ctrl elsewhere) picks every item listed, with nothing being typed.
    if list_keys_work(ui, state) && ui.input(|input| crate::home::command_key(input, egui::Key::A, false)) {
        state.item_picks.places = row_items.clone();
        state.item_picks.made_in(&state.services);
        state.picking = crate::home::Picking::Items;
    }
    // Files dragged from the Finder: into the playlist, after the item under the pointer.
    let (area, current) = (list.inner_rect, state.services.current);
    let target = |after| crate::file_drop::DropTarget::Playlist { index: current, after };
    crate::file_drop::offer(ui, state, crate::file_drop::Place::onto(area, target(None)));
    for (row, item) in rows.iter().zip(&row_items) {
        let place = crate::file_drop::Place::onto(row.intersect(area), target(Some(*item)));
        crate::file_drop::offer(ui, state, place.outlining(area).line_at(row.max.y.min(area.max.y)));
    }
    // Where a dragged row would land: a line between rows, and on release the move.
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let slot = pointer.map(|at| drop_slot(&rows, at.y));
    // A picture or video carried from the media bin, or a result of the find box (a song, or a
    // Bible passage or verse): dropped on a placeholder it takes its place; dropped between
    // rows it goes there, above or below the row by which half of it the pointer is over.
    if let Some(carried) = Carried::now(ui.ctx()).filter(|carried| carried.lands_on(&target(None)))
        && let Some(at) = pointer.filter(|at| list.inner_rect.contains(*at) && !filtering)
        && let made = carried.items(show, state)
        && let [item] = made.as_slice()
    {
        let onto = rows
            .iter()
            .zip(&row_items)
            .find(|(row, item)| row.contains(at) && matches!(items.get(**item), Some(Item::Placeholder { .. })));
        let slot = drop_slot(&rows, at.y);
        let line = |rows: &[Rect]| {
            rows.get(slot)
                .map_or_else(|| rows.last().map_or(list.inner_rect.min.y + 4.0, |row| row.max.y), |row| row.min.y)
        };
        match onto {
            Some((row, _)) => {
                ui.painter().rect_stroke(*row, RADIUS, Stroke::new(2.0, TEXT), StrokeKind::Inside);
            }
            None => {
                let (left, right) = (list.inner_rect.min.x, list.inner_rect.max.x);
                let y = line(&rows);
                ui.painter().line_segment([pos2(left + 4.0, y), pos2(right - 4.0, y)], Stroke::new(2.0, TEXT));
            }
        }
        if ui.input(|input| input.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            let count = state.services.front().items.len();
            match onto {
                Some((_, place)) => {
                    state.services.fill(*place, item.clone());
                }
                None => {
                    // `slot` counts rows shown, which are every item while nothing filters.
                    let place = row_items.get(slot).copied().unwrap_or(items.len());
                    state.services.insert(place, item.clone());
                }
            }
            // A result of the find box ends the search, and what it put in the playlist (or
            // the song already there, which isn't added twice) is opened, as a click on its
            // row opens it: nothing goes on the screens.
            if let Some(found) = carried.found() {
                crate::home::end_search(ui.ctx(), state);
                let refused = onto.is_none() && state.services.front().items.len() == count;
                if refused {
                    state.say(format!("{} is already in {}.", found.name, state.services.front().name));
                }
                ask = state.services.chosen.map(ItemAsk::Open);
            }
        }
    }
    // A row dragged by itself moves alone; one of several picked out moves them all, in their
    // order, to the one line.
    let moving: Vec<usize> = match dragging.or(dropped) {
        Some(held) if state.item_picks.several() && state.item_picks.places.contains(&held) => {
            let mut picked = state.item_picks.places.clone();
            picked.sort_unstable();
            picked
        }
        Some(held) => vec![held],
        None => Vec::new(),
    };
    if let (Some(_), Some(slot)) = (dragging, slot) {
        paint_rows_moving(ui.painter(), &rows, |at| moving.contains(&row_items[at]), slot, PANEL);
    }
    if let (Some(index), Some(slot)) = (dropped, slot) {
        if let [_, _, ..] = moving.as_slice() {
            // Still picked out where they have gone.
            if let Some(now) = state.services.move_items(&moving, slot) {
                let picks = &mut state.item_picks;
                picks.places = picks.places.iter().filter_map(|at| now.get(*at).copied()).collect();
                picks.anchor = picks.anchor.and_then(|at| now.get(at).copied());
                picks.made_in(&state.services);
            }
        } else if state.services.move_item(index, slot) {
            state.item_picks.places = state.services.chosen.into_iter().collect();
            state.item_picks.anchor = state.services.chosen;
            state.item_picks.made_in(&state.services);
        }
    }
    state.rename_started = rename_started;
    match renamed {
        Some(Some((index, name))) => {
            state.services.rename_header(index, &name);
            state.renaming_header = None;
        }
        Some(None) => state.renaming_header = None,
        None => {}
    }
    // ↑ and ↓ open the item above or below, passing over headers (and items the filter hides).
    if ask.is_none() && list_keys_work(ui, state) {
        let (up, down, shift) = ui.input(|input| {
            (input.key_pressed(egui::Key::ArrowUp), input.key_pressed(egui::Key::ArrowDown), input.modifiers.shift)
        });
        let openable: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| !matches!(item, Item::Header { .. }))
            .filter(|(_, item)| !filtering || matches_filter(item.title(), &state.filter))
            .map(|(index, _)| index)
            .collect();
        if (up || down) && !openable.is_empty() {
            let last = openable.len() - 1;
            let current = state.services.chosen.and_then(|chosen| openable.iter().position(|at| *at == chosen));
            let next = match current {
                Some(at) if up && at > 0 => Some(at - 1),
                Some(at) if down && at < last => Some(at + 1),
                None if up => Some(last),
                None if down => Some(0),
                _ => None,
            };
            match next {
                Some(next) => {
                    ask = Some(ItemAsk::Open(openable[next]));
                    state.playlist_scroll_to = Some(openable[next]);
                    // With Shift, the items on the way are picked out too, as in the Finder.
                    let picks = &mut state.item_picks;
                    if shift {
                        if picks.anchor.is_none() {
                            picks.anchor = state.services.chosen;
                        }
                        let mods = egui::Modifiers::SHIFT;
                        crate::home::pick(&mut picks.places, &mut picks.anchor, &row_items, openable[next], mods);
                    } else {
                        picks.places = vec![openable[next]];
                        picks.anchor = Some(openable[next]);
                    }
                    picks.made_in(&state.services);
                }
                None => state.say(crate::running::no_item_beyond(down)),
            }
        }
    }
    if let Some(ask) = ask {
        carry_out_item_ask(show, state, commands, ask);
    }
}
/// The least room kept for what is under Clear All (the tool tabs and the open one's
/// contents, or the inspector). In a window too short for it the preview's picture gives up
/// height, down to [`LEAST_PICTURE`], so the keys under it move up; this depends only on the
/// window's height, never on what is on screen.
const LEAST_BODY: f32 = 212.0;
/// The least height of the preview's picture in a short window (it is 180 in a tall one).
const LEAST_PICTURE: f32 = 112.0;

/// The right column, the same in every view: the preview block (what the screens show, the
/// slide's number and name, and one row for whatever is playing), the six layer keys, Clear
/// All, and under them `body`: the tool tabs in the Show and Bible views, the inspector in the
/// Edit view. Nothing from the top of the column down to Clear All moves, whatever is on
/// screen.
pub fn right_column(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    preview: crate::home::Preview,
    commands: &mut Vec<Command>,
    body: impl FnOnce(&mut egui::Ui, &mut HomeState),
) {
    let panel = egui::Panel::right("live")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(column_outer(RIGHT_WIDTH))
        .frame(column_frame(Column::Right));
    let shown = panel.show(ui, |ui| {
        let column = ui.max_rect();
        ui.allocate_rect(column, Sense::hover());
        const SIDE: f32 = 10.0;
        let left = column.min.x + SIDE;
        let width = column.width() - 2.0 * SIDE;
        // Everything down to Clear All has its own fixed place. In a short window the picture
        // is less tall, which is the only thing that ever changes these places.
        let fixed_height = 10.0 + 316.0 + 12.0 + 44.0 + 10.0 + 44.0 + 13.0 + 50.0 + 14.0;
        let short_by = (LEAST_BODY - (column.height() - SIDE - fixed_height)).clamp(0.0, 180.0 - LEAST_PICTURE);
        let block = Rect::from_min_size(pos2(left, column.min.y + 10.0), vec2(width, 316.0 - short_by));
        // What shows, layer by layer. The screens show five of the layers; the sixth is heard.
        let slide = show.live().is_some();
        let media = show.live_media().is_some();
        let props = !state.props_live.is_empty();
        let message = state.message_live.is_some();
        let audio = state.audio_playing.track.is_some();
        let announcing = state.announcement_live.is_some();
        let on_screen = slide || media || props || message || announcing;
        preview_block(ui, show, state, preview, commands, block, on_screen);

        // The six layer keys: each is lit while its layer shows, and clears that layer alone.
        let keys_top = block.max.y + 12.0;
        let each = (width - 2.0 * 7.0) / 3.0;
        let hint = crate::keys::clear_hint;
        let layers = [
            ("SLIDE", Layer::Slides, slide, hint("Clear the slide: take the words off the screens", "F2")),
            ("MEDIA", Layer::Media, media, hint("Clear the media: take the picture or video off the screens", "F3")),
            ("PROPS", Layer::Props, props, hint("Clear the props: take them off the screens", "F4")),
            ("MESSAGES", Layer::Messages, message, hint("Clear the message: take it off the screens", "F6")),
            ("AUDIO", Layer::Audio, audio, hint("Clear the audio: stop it playing", "F5")),
            (
                "ANNOUNCE",
                Layer::Announcements,
                announcing,
                "Clear the announcements: take them off the screens".to_owned(),
            ),
        ];
        for (place, (name, layer, lit, hint)) in layers.into_iter().enumerate() {
            let corner = pos2(left + (each + 7.0) * (place % 3) as f32, keys_top + 54.0 * (place / 3) as f32);
            let rect = Rect::from_min_size(corner, vec2(each, 44.0));
            let (fill, edge) = if lit { (KEY_LIT, KEY_LIT_EDGE) } else { (BUTTON, KEY_EDGE) };
            let (response, face) = raised_key(ui, rect, ui.scope_id().with(("layer key", place)), fill, edge);
            paint_light(ui.painter(), pos2(face.min.x + 12.5, face.center().y), lit.then_some(TALLY));
            let ink = if lit { TEXT } else { TEXT_FAINT };
            let at = pos2(face.min.x + 23.0, face.center().y);
            paint_spaced(ui, at, Align2::LEFT_CENTER, name, fixed_semibold(11.0), ink, 0.6);
            if response.on_hover_text(if lit { hint.as_str() } else { "Nothing showing" }).clicked() {
                commands.push(Command::Clear(layer));
            }
        }

        // Clear All: orange while any layer shows or plays, grey when none does, always in its
        // place.
        let any_live = on_screen || audio;
        let rect = Rect::from_min_size(pos2(left, keys_top + 98.0 + 13.0), vec2(width, 50.0));
        let (fill, edge, ink) =
            if any_live { (TALLY, CLEAR_ALL_EDGE, Color32::WHITE) } else { (BUTTON, KEY_EDGE, TEXT_FAINT) };
        let (response, face) = raised_key(ui, rect, ui.scope_id().with("clear all"), fill, edge);
        ui.painter().text(face.center(), Align2::CENTER_CENTER, "Clear All", semibold(15.0), ink);
        if response.on_hover_text("Clear All: take everything off the screens (Escape)").clicked() {
            commands.push(Command::ClearAll);
        }

        // Under Clear All: the tabs and the open one's contents, or the inspector.
        let under = Rect::from_min_max(pos2(left, rect.max.y + 14.0), pos2(left + width, column.max.y - SIDE));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(under).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(under.intersect(ui.clip_rect()));
        body(&mut child, state);
    });
    round_corners(ui, column_panel(shown.response.rect, Column::Right));
}

/// The preview block, in `block` (`anything_shows` says whether any layer the screens show has
/// something on it): black, with a head (the on-screen light, the switch between
/// the audience's picture and the stage's, and ON SCREEN or CLEAR), the picture, the readout
/// (the slide's number large, then its group and song), and one row, always there, for whatever
/// is playing.
fn preview_block(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    preview: crate::home::Preview,
    commands: &mut Vec<Command>,
    block: Rect,
    anything_shows: bool,
) {
    let on = show.output_enabled();
    let on_screen = anything_shows && on;
    ui.painter().rect_filled(block, 13.0, Color32::BLACK);
    let inner = block.shrink(10.0);

    // The head.
    let head = block.min.y + 24.0;
    let light = pos2(inner.min.x + 6.5, head);
    if on_screen {
        ui.painter().circle_filled(light, 6.5, TALLY.gamma_multiply(0.3));
        ui.painter().circle_filled(light, 3.5, TALLY);
    } else {
        ui.painter().circle_filled(light, 3.5, Color32::from_rgb(0x3a, 0x3a, 0x3a));
    }
    let (switch, picked) = pill_switch(
        ui,
        "preview of",
        pos2(inner.min.x + 18.0, head - 10.5),
        &["AUDIENCE", "STAGE"],
        &["Show what the audience screens show", "Show what the stage screens show"],
        Some(usize::from(state.preview_stage)),
        &SwitchLook::small_capitals(),
    );
    // Audience or Stage goes back to what all of them see.
    if let Some(picked) = picked {
        state.preview_stage = picked == 1;
        state.preview_screen = None;
    }
    // A screen looked at that has gone from the list: back to what all see.
    if state.preview_screen.is_some_and(|id| !state.screens.iter().any(|screen| screen.id == id)) {
        state.preview_screen = None;
    }
    // With more than two screens, a small arrow opens a menu to look at one of them.
    if state.screens.len() > 2 {
        let arrow = Rect::from_center_size(pos2(switch.max.x + 6.0 + 14.0, head), vec2(28.0, 28.0));
        let response = ui.interact(arrow, ui.scope_id().with("look at one screen"), Sense::click());
        let lit = state.preview_screen.is_some();
        let back = Rect::from_center_size(arrow.center(), vec2(22.0, 22.0));
        if lit || response.hovered() {
            ui.painter().rect_filled(back, 6.0, if lit { SELECTED } else { Color32::from_rgb(0x26, 0x26, 0x26) });
        }
        let ink = if lit || response.hovered() { TEXT } else { SCREEN_DIM };
        icons::paint(ui.painter(), Icon::ChevronDown, Rect::from_center_size(arrow.center(), vec2(12.0, 12.0)), ink);
        let response = response.on_hover_text("Look at one screen, as it draws the show");
        egui::Popup::menu(&response).show(|ui| look_at_menu(ui, state));
    }
    // Set from the right, so the longer wording takes its room to the left and nothing moves.
    let (words, ink) = match (on, on_screen) {
        (false, _) => ("OUTPUT OFF", CARE),
        (true, true) => ("ON SCREEN", TALLY),
        (true, false) => ("CLEAR", SCREEN_DIM),
    };
    paint_spaced(ui, pos2(inner.max.x - 3.0, head), Align2::RIGHT_CENTER, words, fixed_semibold(11.0), ink, 0.9);

    // The picture: as wide as the block allows, 16 to 9, in the middle of its room.
    let room = Rect::from_min_max(pos2(inner.min.x, block.min.y + 46.0), pos2(inner.max.x, block.max.y - 90.0));
    let tall = room.height().min(room.width() * 9.0 / 16.0);
    let picture = Rect::from_center_size(room.center(), vec2((tall * 16.0 / 9.0).round(), tall));
    const CORNER: f32 = 6.0;
    ui.painter().rect_filled(picture, CORNER, Color32::BLACK);
    // While the output fades from one picture to the next, so does this.
    let fading = preview.amount < 1.0;
    if let Some(before) = preview.before.filter(|_| fading) {
        egui::Image::from_texture((before, picture.size())).corner_radius(CORNER).paint_at(ui, picture);
    }
    if let Some(texture) = preview.now {
        let veil = if fading { Color32::WHITE.gamma_multiply(preview.amount.clamp(0.0, 1.0)) } else { Color32::WHITE };
        egui::Image::from_texture((texture, picture.size())).corner_radius(CORNER).tint(veil).paint_at(ui, picture);
    }
    if fading {
        ui.ctx().request_repaint();
    }
    // What the operator must know is said over the picture, so nothing under it moves.
    let over_picture = |ui: &egui::Ui, words: &str, ink: Color32| {
        ui.painter().rect_filled(picture, CORNER, Color32::from_black_alpha(200));
        let galley = ui.painter().layout(words.to_owned(), FontId::proportional(12.5), ink, picture.width() - 28.0);
        ui.painter().galley(picture.center() - galley.size() / 2.0, galley, ink);
    };
    if state.output_stopped {
        over_picture(ui, "The output has stopped. Restart SoftPresenter to bring it back.", CARE);
    } else if let Some(undrawn) = preview.undrawn.filter(|_| on) {
        // A screen that could not draw what is live shows the picture before it, whatever the
        // preview shows: the operator must know.
        over_picture(ui, undrawn.in_words(), CARE);
    } else if let Some(said) = looked_at(state).and_then(|screen| screen_not_showing(state, screen)) {
        over_picture(ui, &said, TEXT_DIM);
    } else if !on && !state.preview_stage {
        over_picture(ui, "The audience screens are off", TEXT_DIM);
    }
    let ring = if on_screen { Stroke::new(2.0, TALLY) } else { Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2a, 0x2a)) };
    ui.painter().rect_stroke(picture, CORNER, ring, StrokeKind::Outside);

    // The readout: where the song is, large, then the slide's group and the song.
    let readout =
        Rect::from_min_max(pos2(inner.min.x + 3.0, room.max.y + 10.0), pos2(inner.max.x - 3.0, room.max.y + 44.0));
    let at = show.live();
    let song = at.and_then(|at| show.presentations().get(at.presentation));
    // The place in the song's play order, which an arrangement can make longer.
    let place = match (show.position(), at, song) {
        (Some((step, length)), Some(_), _) => Some((step + 1, length)),
        (_, Some(at), Some(song)) => Some((at.slide + 1, song.slide_count())),
        _ => None,
    };
    let (number, of) = match place {
        Some((number, of)) => (format!("{number:02}"), format!("/{of:02}")),
        None => ("--".to_owned(), "/--".to_owned()),
    };
    let big = fixed(26.0);
    let small = fixed(12.0);
    // Two dashes stand a little apart, so they do not run into one line.
    let number = spaced(ui, &number, big.clone(), TEXT, if place.is_some() { -1.0 } else { 2.0 });
    let of = spaced(ui, &of, small.clone(), SCREEN_DIM, 0.0);
    // Room is kept for two figures each, so the words beside them stay where they are.
    let kept = spaced(ui, "00", big, TEXT, -1.0).size().x + spaced(ui, "/00", small, SCREEN_DIM, 0.0).size().x;
    let figures = number.size().x + of.size().x;
    let base = readout.center().y + number.size().y * 0.36;
    let (number_wide, of_tall) = (number.size().x, of.size().y);
    ui.painter().galley(pos2(readout.min.x, readout.center().y - number.size().y / 2.0), number, TEXT);
    ui.painter().galley(pos2(readout.min.x + number_wide, base - of_tall + 1.0), of, SCREEN_DIM);
    let words = Rect::from_min_max(pos2(readout.min.x + figures.max(kept) + 10.0, readout.min.y), readout.max);
    // The second line is there only when there is something to say: the one screen looked at,
    // or how far the slide's click builds have got.
    let builds = show.live_slide().map_or(0, |(_, slide)| slide.click_steps().len());
    let second = match looked_at(state) {
        Some(screen) => Some(format!("LOOKING AT {}", screen.name.to_uppercase())),
        None if builds > 0 => Some(format!("BUILD {}/{builds}", show.build_step())),
        None => None,
    };
    let first_y = if second.is_some() { words.center().y - 8.0 } else { words.center().y };
    let painter = ui.painter().with_clip_rect(words);
    let label = fixed(12.0);
    match (at, song) {
        (Some(at), Some(song)) => {
            let group = song.slides().nth(at.slide).map(|(group, _)| group.name.to_uppercase()).unwrap_or_default();
            let group = spaced(ui, &group, fixed_semibold(12.0), TEXT, 0.7);
            let after = pos2(words.min.x + group.size().x, first_y);
            painter.galley(pos2(words.min.x, first_y - group.size().y / 2.0), group, TEXT);
            let title = format!(" \u{b7} {}", song.title.to_uppercase());
            let title = spaced(ui, &title, label.clone(), SCREEN_DIM, 0.7);
            painter.galley(pos2(after.x, first_y - title.size().y / 2.0), title, SCREEN_DIM);
        }
        _ => {
            let words_now = if anything_shows { "NO SLIDE ON SCREEN" } else { "NOTHING ON SCREEN" };
            let line = spaced(ui, words_now, label.clone(), SCREEN_DIM, 0.7);
            painter.galley(pos2(words.min.x, first_y - line.size().y / 2.0), line, SCREEN_DIM);
        }
    }
    if let Some(second) = second {
        let line = spaced(ui, &second, label, SCREEN_DIM, 0.7);
        painter.galley(pos2(words.min.x, words.center().y + 9.0 - line.size().y / 2.0), line, TEXT_FAINT);
    }

    // The row for whatever is playing: always there, so Clear All never moves.
    let row =
        Rect::from_min_max(pos2(inner.min.x + 2.0, readout.max.y + 6.0), pos2(inner.max.x - 2.0, readout.max.y + 36.0));
    // Identify first: it covers every screen the room sees, so it can be stopped from here
    // whatever else plays, however it was started.
    if state.identify {
        if playing_line(
            ui,
            row,
            "Identifying the screens:",
            "the room sees it",
            "Stop",
            "Take each screen's name and colour off it",
        ) {
            state.identify = false;
        }
    } else if let Some(video) = state.video_live.clone() {
        video_row(ui, state, &video, row);
    } else if let Some((_, name)) = state.announcement_live.clone() {
        if playing_line(ui, row, "Announcements:", &name, "Clear", "Take the announcements off the screens") {
            commands.push(Command::Clear(Layer::Announcements));
        }
    } else if let Some(pattern) = state.test_pattern {
        let name = match pattern {
            crate::props::TestPattern::ColorBars => "Colour bars",
            crate::props::TestPattern::Grid => "Grid",
            crate::props::TestPattern::White => "White",
        };
        if playing_line(ui, row, "Test pattern:", name, "Off", "Take the test pattern off the audience screens") {
            state.test_pattern = None;
        }
    }
}

/// The one screen the preview looks at, if it looks at one.
fn looked_at(state: &HomeState) -> Option<&crate::screens::Screen> {
    let id = state.preview_screen?;
    state.screens.iter().find(|screen| screen.id == id)
}

/// Why a screen shows nothing in the room, if it doesn't: switched off, or its display not
/// connected.
fn screen_not_showing(state: &HomeState, screen: &crate::screens::Screen) -> Option<String> {
    use crate::screens::Output;
    match &screen.output {
        Output::Off => Some(format!("{} is switched off", screen.name)),
        Output::Display(name) if !state.displays.iter().any(|display| &display.name == name) => {
            Some(format!("{} is not connected", screen.name))
        }
        _ => None,
    }
}

/// The menu of screens to look at in the preview: what the room sees, each audience screen, then
/// the stage screens. A screen that shows nothing says so beside its name.
fn look_at_menu(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::screens::Kind;
    ui.set_min_width(232.0);
    let mut chosen: Option<(Option<sp_model::Uuid>, bool)> = None;
    let heading = |ui: &mut egui::Ui, words: &str| {
        ui.label(egui::RichText::new(words).size(11.5).color(TEXT_FAINT));
    };
    heading(ui, "Audience");
    let everyone = state.preview_screen.is_none() && !state.preview_stage;
    if ui.add(egui::Button::selectable(everyone, "What the room sees")).clicked() {
        chosen = Some((None, false));
    }
    for stage in [false, true] {
        if stage {
            ui.separator();
            heading(ui, "Stage");
        }
        for screen in state.screens.iter().filter(|screen| (screen.kind == Kind::Stage) == stage) {
            let words = match screen_not_showing(state, screen) {
                Some(_) if screen.output == crate::screens::Output::Off => format!("{} \u{b7} off", screen.name),
                Some(_) => format!("{} \u{b7} not connected", screen.name),
                None => screen.name.clone(),
            };
            let on = state.preview_screen == Some(screen.id);
            if ui.add(egui::Button::selectable(on, words)).clicked() {
                chosen = Some((Some(screen.id), stage));
            }
        }
    }
    if let Some((screen, stage)) = chosen {
        state.preview_screen = screen;
        state.preview_stage = stage;
        ui.close();
    }
}

/// One line in the preview block's row for what is playing: what it is, its name, and a small
/// key at the right that stops it. Returns true when the key was clicked.
fn playing_line(ui: &egui::Ui, row: Rect, what: &str, name: &str, key: &str, hint: &str) -> bool {
    let font = FontId::proportional(12.0);
    let key_wide = ui.painter().layout_no_wrap(key.to_owned(), font.clone(), TEXT).size().x + 18.0;
    let button = Rect::from_min_size(pos2(row.max.x - key_wide, row.center().y - 13.0), vec2(key_wide, 26.0));
    let response = ui.interact(button, ui.scope_id().with(("playing line", what)), Sense::click());
    let fill = if response.hovered() { BUTTON_HOVER } else { Color32::from_rgb(0x1e, 0x1e, 0x1e) };
    ui.painter().rect_filled(button, 13.0, fill);
    ui.painter().text(button.center(), Align2::CENTER_CENTER, key, font.clone(), TEXT);
    let words = Rect::from_min_max(row.min, pos2(button.min.x - 8.0, row.max.y));
    let painter = ui.painter().with_clip_rect(words);
    let said = painter.text(words.left_center(), Align2::LEFT_CENTER, what, font, SCREEN_DIM);
    painter.text(pos2(said.max.x + 5.0, words.center().y), Align2::LEFT_CENTER, name, semibold(12.0), TEXT);
    response.on_hover_text(hint).clicked()
}

/// How far Forward goes in the video on screen, in seconds.
const VIDEO_STEP: f64 = 10.0;

/// The keys of the video on screen, in the preview block's one row: Play or Pause, Start Over,
/// Forward ten seconds, a slim bar to click or drag through, the time left, Loop and Sound.
fn video_row(ui: &mut egui::Ui, state: &mut HomeState, video: &crate::home::VideoLive, row: Rect) {
    use crate::home::VideoRequest;
    let clock = |seconds: f64| crate::audio::clock(std::time::Duration::from_secs_f64(seconds.max(0.0)));
    let length = video.duration;
    let y = row.center().y;
    let dark = Color32::from_rgb(0x1e, 0x1e, 0x1e);
    // Play or Pause: the one light key.
    let play_at = pos2(row.min.x + 14.0, y);
    let big = ui.interact(
        Rect::from_center_size(play_at, egui::Vec2::splat(28.0)),
        ui.scope_id().with("video play"),
        Sense::click(),
    );
    ui.painter().circle_filled(play_at, 14.0, if big.hovered() { Color32::WHITE } else { TEXT });
    let (play, words) = if video.playing { (Icon::Pause, "Pause") } else { (Icon::Play, "Play") };
    let nudge = if video.playing { 0.0 } else { 1.0 };
    let glyph = Rect::from_center_size(play_at + vec2(nudge, 0.0), egui::Vec2::splat(12.0));
    icons::paint(ui.painter(), play, glyph, Color32::from_gray(17));
    if big.on_hover_text(words).clicked() {
        state.video_request = Some(VideoRequest::TogglePause);
    }
    // Start Over, and Forward ten seconds.
    let start_at = pos2(row.min.x + 45.0, y);
    let start = round_key(ui, start_at, 26.0, "video start", dark, false);
    icons::paint(ui.painter(), Icon::ToStart, Rect::from_center_size(start_at, egui::Vec2::splat(12.0)), TEXT);
    if start.on_hover_text("Start over, from the beginning").clicked() {
        state.video_request = Some(VideoRequest::Restart);
    }
    let forward_at = pos2(row.min.x + 75.0, y);
    let forward = round_key(ui, forward_at, 26.0, "video forward", dark, false);
    // The arrow that turns clockwise (the icon named Back is drawn that way).
    icons::paint(ui.painter(), Icon::Back, Rect::from_center_size(forward_at, egui::Vec2::splat(15.0)), TEXT);
    if forward.on_hover_text("Forward 10 seconds").clicked()
        && let Some(length) = length
    {
        state.video_request = Some(VideoRequest::Seek((video.position + VIDEO_STEP).min(length)));
    }
    // Sound and Loop, from the right: each the lighter grey while it is on.
    let sound_at = pos2(row.max.x - 13.0, y);
    let (sound, sound_hint) = if video.muted {
        (Icon::Muted, "Muted. Click to play its sound")
    } else {
        (Icon::Sound, "Its sound plays. Click to mute")
    };
    let response = round_key(ui, sound_at, 26.0, "video sound", dark, !video.muted);
    let ink = if video.muted { TEXT_FAINT } else { TEXT };
    icons::paint(ui.painter(), sound, Rect::from_center_size(sound_at, egui::Vec2::splat(14.0)), ink);
    if response.on_hover_text(sound_hint).clicked() {
        state.video_request = Some(VideoRequest::Muted(!video.muted));
    }
    let loop_at = pos2(row.max.x - 43.0, y);
    let loop_hint = if video.looping {
        "Looping: it starts again at the end. Click to stop looping"
    } else {
        "Not looping: it stops at the end. Click to loop"
    };
    let response = round_key(ui, loop_at, 26.0, "video loop", dark, video.looping);
    let ink = if video.looping { TEXT } else { TEXT_FAINT };
    icons::paint(ui.painter(), Icon::Loop, Rect::from_center_size(loop_at, egui::Vec2::splat(14.0)), ink);
    if response.on_hover_text(loop_hint).clicked() {
        state.video_request = Some(VideoRequest::Looping(!video.looping));
    }
    // The bar between them, and the time left at its right.
    let bar = Rect::from_x_y_ranges(row.min.x + 98.0..=row.max.x - 112.0, y - 8.0..=y + 8.0);
    let shown = video_bar(ui, state, video, bar);
    let left = length.map_or_else(|| "\u{2013}".to_owned(), |length| format!("\u{2212}{}", clock(length - shown)));
    ui.painter().text(pos2(row.max.x - 64.0, y), Align2::RIGHT_CENTER, left, fixed(12.0), SCREEN_DIM);
}

/// How tall the open media strip is: its top margin, its head, the room under that, a picture
/// with the room round it for its ring, its name, and the room under them.
const MEDIA_STRIP_HEIGHT: f32 = 4.0 + 30.0 + 6.0 + 3.0 + 72.0 + 22.2 + 3.0 + 14.0;

/// The ring round media items picked out to change several at once.
const PICKED: Color32 = Color32::from_rgb(0x6a, 0xa5, 0xff);

/// A thin upright line between two groups of controls in the bar under the slides.
fn dock_divider(ui: &egui::Ui, x: f32, middle: f32) {
    ui.painter().vline(x, egui::Rangef::new(middle - 10.0, middle + 10.0), Stroke::new(1.0, LINE));
}

/// The bar under the slides, the top of the dock: the transition for all slides (Cut,
/// Dissolve, or another from Advanced), Continuous and, once a theme has a version, the button
/// that switches the screens to one, Undo and Redo, the three ways to lay the slides out and
/// their size. It is a shelf, a step darker than the panel, with a line along
/// its top, and stays put while the slides scroll. In a narrow window Undo and Redo, then
/// Continuous, then the size, are left out rather than squeezed (each is in the menu bar too).
pub fn dock_bar(ui: &mut egui::Ui, state: &mut HomeState) {
    use sp_model::TransitionKind as K;
    let panel = egui::Panel::bottom("slide bar")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(49.0)
        .frame(Frame::new().fill(SHELF));
    panel.show(ui, |ui| {
        let bar = ui.max_rect();
        ui.painter().hline(bar.x_range(), bar.min.y + 0.5, Stroke::new(1.0, LINE));
        let middle = bar.min.y + 10.0 + 15.1;
        let (left, right) = (bar.min.x + 20.0, bar.max.x - 20.0);
        // What there is room for, the least needed first to go.
        const SWITCH: f32 = 210.7;
        const VIEWS: f32 = 3.0 * 30.0 + 2.0 * 2.0 + 6.0;
        const UNDO: f32 = 58.0;
        const SLIDER: f32 = 86.0;
        // Continuous, with the version button beside it once a theme has a version.
        let versions = !state.versions.is_empty();
        let continuous_wide: f32 = 94.0 + if versions { 8.0 + VERSION_BUTTON.x } else { 0.0 };
        let room = right - left;
        let with_continuous = room >= SWITCH + 13.0 + continuous_wide + 12.0 + VIEWS;
        let with_undo = room >= SWITCH + 13.0 + continuous_wide + 13.0 + 2.0 * UNDO + 8.0 + 12.0 + VIEWS;
        let with_slider = room
            >= SWITCH
                + 12.0
                + VIEWS
                + 12.0
                + 60.0
                + if with_continuous { 13.0 + continuous_wide } else { 0.0 }
                + if with_undo { 13.0 + 2.0 * UNDO + 8.0 } else { 0.0 };
        // The transition: one switch of a fixed width, so nothing beside it moves when the
        // name in its third part changes.
        let kind = state.transition.kind;
        let track = Rect::from_min_size(pos2(left, middle - 15.1), vec2(SWITCH.min(room), 30.2));
        ui.painter().rect_filled(track, 15.1, CARD);
        let other = !matches!(kind, K::Cut | K::Dissolve);
        let third = if other { kind.name() } else { "Advanced" };
        let mut x = track.min.x + 3.0;
        let mut more = None;
        for (index, label, width, on, hint) in [
            (0, "Cut", 40.4, kind == K::Cut, "Slides switch at once"),
            (1, "Dissolve", 68.5, kind == K::Dissolve, "Each slide fades into the next"),
            (
                2,
                third,
                track.max.x - 3.0 - (track.min.x + 3.0 + 40.4 + 2.0 + 68.5 + 2.0),
                other,
                "Every other transition, and how long it takes",
            ),
        ] {
            let cell = Rect::from_min_size(pos2(x, track.min.y + 3.0), vec2(width, 24.2));
            x += width + 2.0;
            // A little taller than it is drawn, so nothing to click is under 26 high.
            let reach = cell.expand2(vec2(0.0, (LEAST_CLICK - cell.height()) / 2.0));
            let response = ui.interact(reach, ui.scope_id().with(("transition", index)), Sense::click());
            paint_pill(ui.painter(), cell, on);
            let ink = if on || response.hovered() { TEXT } else { TEXT_DIM };
            if index == 2 {
                // The name, cut short if it is long, and the arrow that says it opens a list.
                let words =
                    Rect::from_min_max(pos2(cell.min.x + 10.0, cell.min.y), pos2(cell.max.x - 20.0, cell.max.y));
                ui.painter().with_clip_rect(words).text(
                    words.left_center(),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(12.0),
                    ink,
                );
                ui.painter().text(
                    pos2(cell.max.x - 12.0, cell.center().y),
                    Align2::CENTER_CENTER,
                    "\u{25be}",
                    FontId::proportional(12.0),
                    ink,
                );
            } else {
                ui.painter().text(cell.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.0), ink);
            }
            let response = response.on_hover_text(hint);
            match index {
                0 if response.clicked() => state.transition = sp_model::Transition::of(K::Cut),
                1 if response.clicked() && kind != K::Dissolve => {
                    state.transition = sp_model::Transition::of(K::Dissolve);
                }
                2 => more = Some(response),
                _ => {}
            }
        }
        if let Some(more) = more {
            egui::Popup::from_toggle_button_response(&more)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| transition_picker(ui, &mut state.transition, "slides"));
        }
        let mut x = track.max.x;
        if with_continuous {
            dock_divider(ui, x + 6.5, middle);
            x += 13.0;
            let place = Rect::from_min_size(pos2(x, middle - 15.0), vec2(94.0, 30.0));
            let button =
                labelled_button(ui, place, ui.scope_id().with("continuous"), "Continuous", None, state.continuous);
            if button.on_hover_text("Next at the end of a song goes on to the next song").clicked() {
                state.continuous = !state.continuous;
            }
            x += 94.0;
            if versions {
                version_switch_button(ui, state, Rect::from_min_size(pos2(x + 8.0, middle - 15.0), VERSION_BUTTON));
                x += 8.0 + VERSION_BUTTON.x;
            }
        }
        if with_undo {
            use crate::menubar::Action;
            dock_divider(ui, x + 6.5, middle);
            x += 13.0;
            let hints = [crate::keys::hint("Undo", "Z", false), crate::keys::hint("Redo", "Z", true)];
            for ((label, action), hint) in [("Undo", Action::Undo), ("Redo", Action::Redo)].into_iter().zip(&hints) {
                let place = Rect::from_min_size(pos2(x, middle - 15.0), vec2(UNDO, 30.0));
                if labelled_button(ui, place, ui.scope_id().with(label), label, None, false)
                    .on_hover_text(hint)
                    .clicked()
                {
                    state.menu_asked = Some(action);
                }
                x += UNDO + 8.0;
            }
        }
        // From the right: the slide size, then the three views.
        let mut edge = right;
        if with_slider {
            let wide = SLIDER.min(edge - x - 12.0 - VIEWS - 12.0).max(60.0);
            let place = Rect::from_min_size(pos2(edge - wide, middle - 10.0), vec2(wide, 20.0));
            // Further right is bigger, so fewer slides fit across.
            let mut size = 7 - state.columns.clamp(2, 7);
            let slider = ui
                .scope_builder(
                    egui::UiBuilder::new().max_rect(place).layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.spacing_mut().slider_width = wide;
                        ui.add(egui::Slider::new(&mut size, 0..=5).show_value(false))
                    },
                )
                .inner;
            if slider.on_hover_text("Slide size").changed() {
                state.columns = 7 - size;
            }
            edge -= wide + 12.0;
        }
        let views = Rect::from_min_size(pos2(edge - VIEWS, middle - 15.1), vec2(VIEWS, 30.2));
        ui.painter().rect_filled(views, 15.1, CARD);
        for (index, (view, icon, hint)) in [
            (SlideView::Grid, Icon::Grid, "Grid"),
            (SlideView::Easy, Icon::Rows, "Words Only"),
            (SlideView::Table, Icon::Table, "Table"),
        ]
        .into_iter()
        .enumerate()
        {
            let cell =
                Rect::from_min_size(pos2(views.min.x + 3.0 + 32.0 * index as f32, views.min.y + 3.0), vec2(30.0, 24.2));
            let reach = cell.expand2(vec2(0.0, (LEAST_CLICK - cell.height()) / 2.0));
            let response = ui.interact(reach, ui.scope_id().with(("slide view", index)), Sense::click());
            let on = state.slide_view == view;
            paint_pill(ui.painter(), cell, on);
            let ink = if on || response.hovered() { TEXT } else { TEXT_DIM };
            icons::paint(ui.painter(), icon, Rect::from_center_size(cell.center(), vec2(13.0, 13.0)), ink);
            if response.on_hover_text(hint).clicked() {
                state.slide_view = view;
            }
        }
    });
}

/// The size of the button beside Continuous that switches the screens to a theme version.
const VERSION_BUTTON: egui::Vec2 = vec2(40.0, 30.0);

/// The button beside Continuous that switches every screen whose theme has a version to it, by
/// hand: the theme icon and an arrow, lit while a version is on (its tooltip names it). Its
/// menu: the usual look or each version, Keep It Until Switched Back, and Make or Change
/// Versions… (Edit › Themes).
fn version_switch_button(ui: &mut egui::Ui, state: &mut HomeState, place: Rect) {
    use crate::versions::Ask;
    let on = state.version_switch.on;
    let name = on.and_then(|on| state.versions.iter().find(|version| version.id == on.version)).map(|v| v.name.clone());
    let response = ui.interact(place, ui.scope_id().with("theme version"), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let fill = match (name.is_some(), response.hovered() || open) {
        (true, _) => KEY_LIT,
        (false, true) => BUTTON_HOVER,
        (false, false) => BUTTON,
    };
    ui.painter().rect_filled(place, 9.0, fill);
    let start = place.center().x - (13.0 + 4.0 + 7.0) / 2.0;
    let icon = Rect::from_min_size(pos2(start, place.center().y - 6.5), egui::Vec2::splat(13.0));
    icons::paint(ui.painter(), Icon::Theme, icon, if name.is_some() { TALLY } else { TEXT });
    let arrow = pos2(icon.max.x + 4.0 + 3.5, place.center().y);
    ui.painter().text(arrow, Align2::CENTER_CENTER, "\u{25be}", FontId::proportional(12.5), TEXT_FAINT);
    let example = state.versions.first().map(|version| version.name.clone()).unwrap_or_default();
    let now = match &name {
        Some(name) => format!("the {name} version is on"),
        None => "the usual look".to_owned(),
    };
    let hint =
        format!("Theme version: {now}. Choose a version, such as {example}, for every screen whose theme has it");
    let response = response.on_hover_text(hint);
    egui::Popup::menu(&response).show(|ui| {
        crate::live_tabs::menu_heading(ui, "Show the screens in");
        let keep = on.is_some_and(|on| on.keep);
        let mut asked = None;
        if ui.add(egui::Button::selectable(on.is_none(), "The usual look")).clicked() {
            asked = Some(Ask::Use { version: None, keep: false });
        }
        for version in &state.versions {
            let chosen = on.is_some_and(|on| on.version == version.id);
            if ui.add(egui::Button::selectable(chosen, format!("The {} version", version.name))).clicked() {
                asked = Some(Ask::Use { version: Some(version.id), keep });
            }
        }
        ui.separator();
        let kept = ui
            .add_enabled(on.is_some(), egui::Button::selectable(keep, "Keep It Until Switched Back"))
            .on_hover_text("The version stays on when the next item goes on screen");
        if kept.clicked() {
            asked = Some(Ask::Keep(!keep));
        }
        if ui.button("Make or Change Versions\u{2026}").on_hover_text("Edit \u{203a} Themes").clicked() {
            state.menu_asked = Some(crate::menubar::Action::ThemesView);
            ui.close();
        }
        if let Some(asked) = asked {
            state.version_asks.push(asked);
            ui.close();
        }
    });
}

/// A small round label that opens a menu: its words and an arrow, on a pill a step lighter
/// than the shelf.
fn menu_pill(ui: &egui::Ui, id: &str, left: f32, middle: f32, words: &str, right_aligned: bool) -> egui::Response {
    let galley = spaced(ui, &format!("{words} \u{25be}"), fixed_semibold(11.0), TEXT_DIM, 0.7);
    let size = vec2(galley.size().x + 18.0, 21.0);
    let min = pos2(if right_aligned { left - size.x } else { left }, middle - size.y / 2.0);
    let pill = Rect::from_min_size(min, size);
    let response = ui.interact(pill.expand2(vec2(0.0, 2.5)), ui.scope_id().with(id), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    ui.painter().rect_filled(pill, 10.5, if response.hovered() || open { BUTTON } else { CARD });
    ui.painter().galley(pos2(pill.min.x + 9.0, middle - galley.size().y / 2.0), galley, TEXT_DIM);
    response
}

/// The media strip, the foot of the dock: "Media", the menu that shows all media or one media
/// playlist, how one picture or video gives way to the next, the button that adds some, then
/// the pictures and videos with their names. The one showing is ringed in orange.
pub fn media_strip(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    // Folded down (with its chevron, or from the View menu) to a slim bar that opens it again;
    // opened by itself while there is something to say or a file is being dropped.
    let folded = state.media_hidden && state.notice.is_none() && !crate::file_drop::media_coming(state);
    let empty = show.media().is_empty();
    let height = if folded { 30.0 } else { MEDIA_STRIP_HEIGHT } + GAP;
    let margin =
        if folded { egui::Margin::symmetric(20, 0) } else { egui::Margin { left: 20, right: 20, top: 4, bottom: 14 } };
    // The foot of the middle column: its lower corners are the column's.
    let corner = RADIUS_PANEL as u8;
    let frame = Frame::new()
        .fill(SHELF)
        .corner_radius(CornerRadius { nw: 0, ne: 0, sw: corner, se: corner })
        .inner_margin(margin)
        .outer_margin(column_margin(Column::Middle));
    let panel =
        egui::Panel::bottom("media bin").resizable(false).show_separator_line(false).exact_size(height).frame(frame);
    panel.show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let area = ui.max_rect().expand2(vec2(20.0, 4.0));
        if crate::file_drop::media_coming(state) {
            crate::file_drop::offer(
                ui,
                state,
                crate::file_drop::Place::onto(area, crate::file_drop::DropTarget::MediaStrip),
            );
        }
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        let middle = head.center().y;
        let label = cap(ui, pos2(head.min.x, middle), Align2::LEFT_CENTER, "Media", TEXT_DIM);
        // Holding the modifier: the key that numbers the pictures and videos, after the label.
        let mut label = label;
        if crate::keys::tags_shown(state) {
            let words = format!("{} then 1\u{2013}9", crate::keys::media_key());
            let wide = crate::keys::tag_width(ui.painter(), &words);
            let tag = crate::keys::tag(ui.painter(), label.max.x + 8.0 + wide, middle, &words);
            label.max.x = tag.max.x;
        }
        if folded {
            let count = show.media().len();
            let words = match count {
                0 => String::new(),
                1 => "1 item".to_owned(),
                n => format!("{n} items"),
            };
            ui.painter().text(pos2(label.max.x + 12.0, middle), Align2::LEFT_CENTER, words, fixed(11.0), TEXT_FAINT);
            let place = Rect::from_center_size(pos2(head.max.x - 15.0, middle), egui::Vec2::splat(ICON_BUTTON));
            let open = icon_button(ui, place, ui.scope_id().with("unfold media"), Icon::ChevronUp, 1.0, false);
            if open.on_hover_text("Show the media strip").clicked() {
                state.media_hidden = false;
            }
            return;
        }
        // From the right: the button that adds pictures and videos, the one that folds the
        // strip away, and how one picture or video gives way to the next.
        let place = Rect::from_center_size(pos2(head.max.x - 15.0, middle), egui::Vec2::splat(30.0));
        let add = icon_button(ui, place, ui.scope_id().with("add media"), Icon::Plus, 1.0, false);
        if add.on_hover_text("Add pictures or videos").clicked() {
            state.media_request = Some(crate::home::MediaRequest::Choose);
        }
        let place = place.translate(vec2(-32.0, 0.0));
        let fold = icon_button(ui, place, ui.scope_id().with("fold media"), Icon::ChevronDown, 0.6, false);
        if fold.on_hover_text("Hide the media strip").clicked() {
            state.media_hidden = true;
        }
        let transition =
            menu_pill(ui, "media transition", place.min.x - 8.0, middle, &state.media_transition.label(), true)
                .on_hover_text("How one picture or video gives way to the next");
        egui::Popup::from_toggle_button_response(&transition)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| transition_picker(ui, &mut state.media_transition, "media"));
        // From the left: the menu of media playlists, then anything there is to say (a file
        // that could not be added, a song that could not be saved).
        let source = media_playlist_picker(ui, state, label.max.x + 12.0, middle);
        if let Some(notice) = &state.notice {
            let room = Rect::from_min_max(
                pos2(source.max.x + 12.0, head.min.y),
                pos2(transition.rect.min.x - 12.0, head.max.y),
            );
            let said = ui.interact(room, ui.scope_id().with("notice"), Sense::hover());
            ui.painter().with_clip_rect(room).text(
                room.left_center(),
                Align2::LEFT_CENTER,
                notice,
                FontId::proportional(12.0),
                CARE,
            );
            said.on_hover_text(notice.as_str());
        }
        ui.add_space(6.0);
        // Nothing in it yet: a big place to click, or to drop files on.
        if empty && !crate::file_drop::media_coming(state) {
            let room = ui.available_size() - vec2(0.0, 2.0);
            let (place, response) = ui.allocate_exact_size(room, Sense::click());
            let hot = response.hovered();
            let painter = ui.painter();
            painter.rect_filled(place, RADIUS_CARD, if hot { CARD } else { PANEL });
            dashed_rect(painter, place.shrink(1.0), Stroke::new(1.5, if hot { TEXT_DIM } else { LINE }));
            // The icon, and beside it what to do and what comes after, centred together.
            let ink = if hot { TEXT } else { TEXT_DIM };
            let title = painter.layout_no_wrap(
                "Click or drag pictures and videos here to add them".to_owned(),
                semibold(13.0),
                ink,
            );
            let hint = painter.layout_no_wrap(
                "Then click one to put it behind the words, or drag it onto a slide".to_owned(),
                FontId::proportional(11.5),
                TEXT_FAINT,
            );
            let (icon_side, gap) = (28.0, 14.0);
            let text_width = title.size().x.max(hint.size().x);
            let text_height = title.size().y + 3.0 + hint.size().y;
            let left = place.center().x - (icon_side + gap + text_width) / 2.0;
            let middle = place.center().y;
            icons::paint(
                painter,
                Icon::Picture,
                Rect::from_center_size(pos2(left + icon_side / 2.0, middle), vec2(icon_side, icon_side)),
                ink,
            );
            let text_left = left + icon_side + gap;
            let top = middle - text_height / 2.0;
            let title_height = title.size().y;
            painter.galley(pos2(text_left, top), title, ink);
            painter.galley(pos2(text_left, top + title_height + 3.0), hint, TEXT_FAINT);
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                state.media_request = Some(crate::home::MediaRequest::Choose);
            }
        }
        if !empty {
            let image_size = vec2(128.0, 72.0);
            // The strip the items sit in: a drag that starts between them draws a box, and
            // picks out every item it touches, as in the Finder; a click there picks nothing.
            let bin = ui.available_rect_before_wrap();
            let (pointer, mods, down) =
                ui.input(|input| (input.pointer.interact_pos(), input.modifiers, input.pointer.primary_down()));
            let background = ui.interact(bin, ui.scope_id().with("media background"), Sense::click_and_drag());
            if background.clicked() {
                state.picked_media.clear();
                state.picking = crate::home::Picking::Media;
            }
            if background.drag_started()
                && let Some(start) = pointer
            {
                if !mods.command && !mods.shift {
                    state.picked_media.clear();
                }
                state.picking = crate::home::Picking::Media;
                state.sweep = Some(crate::home::Sweep {
                    list: crate::home::Picking::Media,
                    from: None,
                    start,
                    keeps: false,
                    carried: false,
                });
            }
            // A media playlist shows only its own files, in the order it plays them; one put
            // together by hand (not following a folder) is reordered by dragging them.
            let shown = state
                .media_playlist_shown
                .and_then(|id| state.media_playlists.iter().find(|playlist| playlist.id == id))
                .cloned();
            let order: Vec<usize> = match &shown {
                Some(playlist) => crate::media::playlist_places(&playlist.files, show.media()),
                None => (0..show.media().len()).collect(),
            };
            let reorders = shown.as_ref().is_some_and(crate::media::Playlist::arranged_by_hand);
            state.picked_media.retain(|at| order.contains(at));
            let scroll = egui::scroll_area::ScrollSource::MOUSE_WHEEL | egui::scroll_area::ScrollSource::SCROLL_BAR;
            egui::ScrollArea::horizontal().id_salt("media row").scroll_source(scroll).auto_shrink([false, false]).show(
                ui,
                |ui| {
                    let mut placed: Vec<(usize, Rect)> = Vec::new();
                    // Room round the row, so the ring of an item on screen or picked out is
                    // not cut off at the edges.
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.add_space(3.0);
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let several = state.picked_media.len() > 1;
                        for (index, item) in order.iter().map(|at| (*at, &show.media()[*at])) {
                            let (rect, response) =
                                ui.allocate_exact_size(image_size + vec2(0.0, 22.2), Sense::click_and_drag());
                            placed.push((index, rect));
                            // Its number, after Shift-modifier-M: the number shows it.
                            let numbers = &state.numbers;
                            if crate::keys::Numbers::picked(numbers, crate::keys::NumbersOf::Media, placed.len() - 1) {
                                commands.push(Command::ShowMedia(index));
                            }
                            // A drag starting on an item picks out the items it passes over while it
                            // stays in the bin (in a media playlist's own view, moves it there);
                            // dragged out onto a slide it becomes what the slide shows, onto the
                            // space under the slides a slide of its own.
                            if response.drag_started()
                                && let Some(start) = pointer
                            {
                                let keeps = several && state.picked_media.contains(&index);
                                if !keeps && !mods.command && !mods.shift {
                                    state.picked_media = vec![index];
                                    state.media_anchor = Some(index);
                                }
                                state.picking = crate::home::Picking::Media;
                                state.sweep = Some(crate::home::Sweep {
                                    list: crate::home::Picking::Media,
                                    from: Some(index),
                                    start,
                                    keeps,
                                    carried: false,
                                });
                            }
                            // A click shows it behind the words; with Command or Shift it is only
                            // picked out (to delete several at once).
                            if response.clicked() {
                                state.picking = crate::home::Picking::Media;
                                crate::home::pick(
                                    &mut state.picked_media,
                                    &mut state.media_anchor,
                                    &order,
                                    index,
                                    mods,
                                );
                                if !mods.command && !mods.shift {
                                    commands.push(Command::ShowMedia(index));
                                }
                            }
                            if !ui.is_rect_visible(rect) {
                                continue;
                            }
                            let image = Rect::from_min_size(rect.min, image_size);
                            ui.painter().rect_filled(image, RADIUS, Color32::BLACK);
                            match thumb(ThumbOf::Media(index)) {
                                Some((texture, _)) => {
                                    egui::Image::from_texture((texture, image_size))
                                        .corner_radius(RADIUS)
                                        .paint_at(ui, image);
                                }
                                None => ui.ctx().request_repaint(),
                            }
                            // A video says so in its corner.
                            if crate::media::is_video(&item.path) {
                                let galley = spaced(ui, "\u{25b6} VIDEO", fixed(11.0), Color32::WHITE, 0.3);
                                let tag = Rect::from_min_size(
                                    pos2(image.min.x + 6.0, image.max.y - 6.0 - galley.size().y - 4.0),
                                    galley.size() + vec2(10.0, 4.0),
                                );
                                ui.painter().rect_filled(tag, 3.0, Color32::from_black_alpha(178));
                                ui.painter().galley(tag.min + vec2(5.0, 2.0), galley, Color32::WHITE);
                            }
                            let live = show.live_media() == Some(index);
                            let picked = state.picked_media.contains(&index);
                            if live {
                                ui.painter().rect_stroke(image, RADIUS, Stroke::new(3.0, TALLY), StrokeKind::Outside);
                            } else if picked {
                                ui.painter().rect_stroke(image, RADIUS, Stroke::new(3.0, PICKED), StrokeKind::Outside);
                            }
                            // Picked out and on screen: a blue ring inside the orange one.
                            if live && picked {
                                ui.painter().rect_stroke(image, RADIUS, Stroke::new(2.0, PICKED), StrokeKind::Inside);
                            }
                            let label = Rect::from_min_max(pos2(rect.min.x, image.max.y + 6.0), rect.max);
                            let (font, ink) =
                                if live { (fixed_semibold(12.0), TALLY) } else { (fixed(12.0), TEXT_DIM) };
                            let name = spaced(ui, &item.name.to_uppercase(), font, ink, 0.6);
                            ui.painter().with_clip_rect(label).galley(
                                pos2(label.min.x, label.center().y - name.size().y / 2.0),
                                name,
                                ink,
                            );
                            // The "more" button, on the picture: what a right-click opens.
                            let near = ui.rect_contains_pointer(rect);
                            let place = Rect::from_min_size(
                                pos2(image.max.x - 5.0 - MORE_BUTTON, image.min.y + 5.0),
                                egui::Vec2::splat(MORE_BUTTON),
                            );
                            ui.painter().rect_filled(place, 6.0, Color32::from_black_alpha(178));
                            let more = more_button(ui, place, ui.scope_id().with(("media more", index)), near);
                            egui::Popup::menu(&more).show(|ui| media_item_menu(ui, show, state, index, item));
                            response.on_hover_text(item.name.as_str()).context_menu(|ui| {
                                media_item_menu(ui, show, state, index, item);
                            });
                        }
                    });
                    if crate::keys::Numbers::showing(&state.numbers, crate::keys::NumbersOf::Media) {
                        for (place, (_, rect)) in placed.iter().take(9).enumerate() {
                            crate::keys::number_badge(ui.painter(), *rect, place + 1);
                        }
                    }
                    sweep_media(ui, state, (&placed, show), bin, pointer, down, reorders);
                },
            );
        }
        if crate::file_drop::strip_lit(state) {
            let painter = ui.painter();
            painter.rect_filled(area, 0.0, TEXT.gamma_multiply(0.08));
            painter.rect_stroke(area.shrink(4.0), RADIUS, Stroke::new(2.0, TEXT), StrokeKind::Inside);
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                "Drop to add",
                FontId::proportional(15.0),
                Color32::WHITE,
            );
        }
    });
}
/// A rounded-looking dashed outline: short dashes along each side of `rect`.
fn dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let (dash, gap) = (7.0, 5.0);
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    for side in 0..4 {
        let (from, to) = (corners[side], corners[(side + 1) % 4]);
        let length = from.distance(to);
        let along = (to - from) / length.max(1.0);
        let mut at = 8.0;
        while at < length - 8.0 {
            let end = (at + dash).min(length - 8.0);
            painter.line_segment([from + along * at, from + along * end], stroke);
            at += dash + gap;
        }
    }
}

/// Carries on a drag across the media bin (see `Sweep`): while the pointer is in the bin, a box
/// from where it started to the pointer picks out every item it touches, and the row scrolls
/// at its ends; once a drag that started on an item leaves the bin, that item is carried to be
/// dropped on a slide. With `reorders` (a media playlist put together by hand is shown), a drag
/// that started on an item moves it instead, with the others picked out with it: they go faint,
/// an ink line shows where they would go, and letting go in the strip puts them there.
fn sweep_media(
    ui: &mut egui::Ui,
    state: &mut HomeState,
    (placed, show): (&[(usize, Rect)], &Show),
    bin: Rect,
    pointer: Option<egui::Pos2>,
    down: bool,
    reorders: bool,
) {
    let Some(mut sweep) = state.sweep.filter(|sweep| sweep.list == crate::home::Picking::Media) else { return };
    let moving = sweep.from.filter(|_| reorders).map(|from| {
        let several = state.picked_media.len() > 1 && state.picked_media.contains(&from);
        let picked = |index: &usize| if several { state.picked_media.contains(index) } else { *index == from };
        placed.iter().map(|(index, _)| *index).filter(picked).collect::<Vec<usize>>()
    });
    let tiles: Vec<Rect> = placed.iter().map(|(_, rect)| *rect).collect();
    let moved = |at: egui::Pos2| !sweep.carried && bin.contains(at) && at.distance(sweep.start) >= 3.0;
    if !down {
        if let (Some(moving), Some(at)) = (moving, pointer)
            && moved(at)
        {
            let slot = drop_slot_across(&tiles, at.x);
            move_in_media_playlist(state, show, placed, &moving, slot);
        }
        state.sweep = None;
        return;
    }
    let Some(at) = pointer else { return };
    if sweep.carried {
        return;
    }
    if !bin.contains(at) {
        if let Some(from) = sweep.from {
            egui::DragAndDrop::set_payload(ui.ctx(), crate::home::MediaDrag(from));
            sweep.carried = true;
            state.sweep = Some(sweep);
        }
        return;
    }
    if let Some(moving) = moving {
        if moved(at) {
            let slot = drop_slot_across(&tiles, at.x);
            paint_tiles_moving(ui.painter(), &tiles, |place| moving.contains(&placed[place].0), slot, SHELF);
            let words = match moving.as_slice() {
                [one] => show.media().get(*one).map(|item| item.name.clone()).unwrap_or_default(),
                many => format!("{} items", many.len()),
            };
            paint_carried(ui.ctx(), &words);
            scroll_strip_at_ends(ui, bin, at);
        }
        return;
    }
    if sweep.keeps || at.distance(sweep.start) < 3.0 {
        return;
    }
    let band = Rect::from_two_pos(sweep.start, at);
    state.picked_media = placed.iter().filter(|(_, rect)| rect.intersects(band)).map(|(index, _)| *index).collect();
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("media band")));
    painter.rect_filled(band, 2.0, ACCENT.gamma_multiply(0.12));
    painter.rect_stroke(band, 2.0, Stroke::new(1.0, ACCENT.gamma_multiply(0.8)), StrokeKind::Inside);
    scroll_strip_at_ends(ui, bin, at);
}

/// Puts `moving` (bin places, in the order shown) at `slot` among the items the media playlist
/// shown has in the strip (`placed`, in its order), and keeps the change.
fn move_in_media_playlist(state: &mut HomeState, show: &Show, placed: &[(usize, Rect)], moving: &[usize], slot: usize) {
    let path = |index: &usize| show.media().get(*index).map(|item| item.path.clone());
    let shown: Vec<std::path::PathBuf> = placed.iter().filter_map(|(index, _)| path(index)).collect();
    let carried: Vec<std::path::PathBuf> = moving.iter().filter_map(path).collect();
    let Some(playlist) =
        state.media_playlist_shown.and_then(|id| state.media_playlists.iter_mut().find(|playlist| playlist.id == id))
    else {
        return;
    };
    let files = crate::media::moved_files(&playlist.files, &shown, &carried, slot);
    if files != playlist.files {
        playlist.files = files;
        state.media_playlists_changed = true;
    }
}

/// While something is dragged in the media strip, it scrolls when the pointer is near its ends.
fn scroll_strip_at_ends(ui: &mut egui::Ui, bin: Rect, at: egui::Pos2) {
    let edge = 30.0;
    if at.x < bin.min.x + edge {
        ui.scroll_with_delta(vec2(10.0, 0.0));
        ui.ctx().request_repaint();
    } else if at.x > bin.max.x - edge {
        ui.scroll_with_delta(vec2(-10.0, 0.0));
        ui.ctx().request_repaint();
    }
}

/// The choice in a Transition menu that follows what is set above it, naming what that is:
/// "Usual (Dissolve, 0.4 s)", or "Usual (Cut)".
pub fn usual_transition(look: sp_model::Transition) -> String {
    if look.kind == sp_model::TransitionKind::Cut {
        format!("Usual ({})", look.kind.name())
    } else {
        format!("Usual ({}, {:.1} s)", look.kind.name(), look.seconds)
    }
}

/// The transition picker: every transition, in its group, with the
/// chosen one's settings under them.
pub fn transition_picker(ui: &mut egui::Ui, look: &mut sp_model::Transition, id: &str) {
    use sp_model::TransitionKind as K;
    ui.set_width(330.0);
    for category in ["Dissolves", "Movements", "Objects", "Wipes", "None"] {
        caption(ui, &category.to_uppercase());
        ui.horizontal_wrapped(|ui| {
            for kind in K::ALL.into_iter().filter(|kind| kind.category() == category) {
                let on = look.kind == kind;
                if ui.add(egui::Button::selectable(on, kind.name()).min_size(vec2(100.0, 28.0))).clicked() && !on {
                    // A newly chosen kind keeps the direction, and takes its usual time.
                    *look = sp_model::Transition { direction: look.direction, ..sp_model::Transition::of(kind) };
                }
            }
        });
        ui.add_space(4.0);
    }
    ui.separator();
    transition_settings(ui, look, id);
}

/// The chosen transition's settings: its direction, if it moves, and how long it takes.
fn transition_settings(ui: &mut egui::Ui, look: &mut sp_model::Transition, id: &str) {
    if look.kind == sp_model::TransitionKind::Cut {
        ui.label(egui::RichText::new("Cut switches at once.").color(TEXT_DIM));
        return;
    }
    egui::Grid::new(("transition settings", id)).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        if look.kind.has_direction() {
            ui.label(egui::RichText::new("Direction").color(TEXT_DIM));
            egui::ComboBox::from_id_salt(("transition direction", id)).selected_text(look.direction.name()).show_ui(
                ui,
                |ui| {
                    for direction in sp_model::Direction::ALL {
                        ui.selectable_value(&mut look.direction, direction, direction.name());
                    }
                },
            );
            ui.end_row();
        }
        ui.label(egui::RichText::new("Duration").color(TEXT_DIM));
        ui.add(egui::Slider::new(&mut look.seconds, 0.0..=5.0).suffix(" s").max_decimals(1));
        ui.end_row();
    });
    if ui.button("Reset to Default Parameters").clicked() {
        *look = sp_model::Transition::of(look.kind);
    }
}

/// The bar of the video on screen, in `bar`: a slim rounded track, the part played in white, the
/// playhead under the pointer (and while dragged), and the time a click would go to over it.
/// Clicking or dragging asks for the video to go there. Returns the time it shows.
fn video_bar(ui: &mut egui::Ui, state: &mut HomeState, video: &crate::home::VideoLive, bar: Rect) -> f64 {
    use crate::home::VideoRequest;
    let clock = |seconds: f64| crate::audio::clock(std::time::Duration::from_secs_f64(seconds.max(0.0)));
    let length = video.duration;
    let response = ui.interact(bar, ui.scope_id().with("video bar"), Sense::click_and_drag());
    let active = response.hovered() || response.dragged();
    let track = Rect::from_center_size(bar.center(), vec2(bar.width(), if active { 6.0 } else { 4.0 }));
    let seconds_at = |x: f32, length: f64| f64::from(((x - track.min.x) / track.width()).clamp(0.0, 1.0)) * length;
    // While the playhead is dragged it follows the pointer, not the player, which catches up a
    // moment later (or, playing through FFmpeg, at the end of the drag).
    let dragged = length
        .zip(response.interact_pointer_pos().filter(|_| response.dragged() || response.drag_stopped()))
        .map(|(length, at)| seconds_at(at.x, length));
    let shown = dragged.unwrap_or(video.position);
    let fraction = length.map_or(0.0, |length| (shown / length.max(0.001)).clamp(0.0, 1.0));
    let x = track.min.x + track.width() * fraction as f32;
    ui.painter().rect_filled(track, 3.0, Color32::from_white_alpha(38));
    ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), 3.0, Color32::WHITE);
    if active && let Some(length) = length {
        ui.painter().circle_filled(pos2(x, track.center().y), 7.0, Color32::WHITE);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        // Where a click would go, over the bar.
        if let Some(hover) = response.hover_pos().filter(|_| !response.dragged()) {
            let tip = clock(seconds_at(hover.x, length));
            let at = pos2(hover.x.clamp(track.min.x + 16.0, track.max.x - 16.0), bar.min.y - 9.0);
            let galley = ui.painter().layout_no_wrap(tip, FontId::proportional(11.0), TEXT);
            let pill = Rect::from_center_size(at, galley.size() + vec2(10.0, 4.0));
            ui.painter().rect_filled(pill, 4.0, Color32::from_black_alpha(200));
            ui.painter().galley(pill.min + vec2(5.0, 2.0), galley, TEXT);
        }
    }
    match dragged {
        Some(to) if response.drag_stopped() => state.video_request = Some(VideoRequest::Seek(to)),
        Some(to) => state.video_request = Some(VideoRequest::Scrub(to)),
        None => {
            if let (Some(length), Some(at)) = (length, response.interact_pointer_pos().filter(|_| response.clicked())) {
                state.video_request = Some(VideoRequest::Seek(seconds_at(at.x, length)));
            }
        }
    }
    shown
}

/// The menu beside MEDIA: all media or one media playlist, a new playlist, and renaming or
/// deleting the one shown.
fn media_playlist_picker(ui: &egui::Ui, state: &mut HomeState, left: f32, middle: f32) -> Rect {
    let shown = state.media_playlist_shown.and_then(|id| state.media_playlists.iter().find(|p| p.id == id));
    if shown.is_none() {
        state.media_playlist_shown = None;
    }
    let label = shown.map_or_else(|| "All Media".to_owned(), |playlist| playlist.name.clone());
    let picker = menu_pill(ui, "media source", left, middle, &label, false).on_hover_text("Choose a media playlist");
    let place = picker.rect;
    media_playlist_menu(state, &picker);
    place
}

/// What the menu beside MEDIA offers.
fn media_playlist_menu(state: &mut HomeState, picker: &egui::Response) {
    egui::Popup::from_toggle_button_response(picker)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(200.0);
            if ui.add(egui::Button::selectable(state.media_playlist_shown.is_none(), "All Media")).clicked() {
                state.media_playlist_shown = None;
                ui.close();
            }
            for playlist in &state.media_playlists {
                let on = state.media_playlist_shown == Some(playlist.id);
                let name = if playlist.name.trim().is_empty() { "Untitled" } else { playlist.name.as_str() };
                if ui.add(egui::Button::selectable(on, name)).clicked() {
                    state.media_playlist_shown = Some(playlist.id);
                    ui.close();
                }
            }
            ui.separator();
            // A new one is shown, with its name ready to type in the Name field under.
            let name_field = egui::Id::unique("media playlist name");
            let ask_name = egui::Id::unique("media playlist name asked");
            if ui
                .button("New Media Playlist\u{2026}")
                .on_hover_text("Add media to it from an item's right-click menu")
                .clicked()
            {
                let made = crate::media::Playlist::new(crate::media::new_playlist_name(&state.media_playlists));
                state.media_playlist_shown = Some(made.id);
                state.media_playlists.push(made);
                state.media_playlists_changed = true;
                ui.data_mut(|data| data.insert_temp(ask_name, true));
            }
            if let Some(at) =
                state.media_playlist_shown.and_then(|id| state.media_playlists.iter().position(|p| p.id == id))
            {
                let id = state.media_playlists[at].id;
                ui.separator();
                let front = state.services.front().name.clone();
                if ui
                    .button(format!("Add to Playlist \u{201c}{front}\u{201d}"))
                    .on_hover_text("Put it in the running order of the playlist in front, to play it from there")
                    .clicked()
                {
                    let name = state.media_playlists[at].name.clone();
                    state.services.add(crate::service::Item::MediaPlaylist { id, name });
                    ui.close();
                }
                // Played item after item on the media layer, round and round.
                if state.media_playing == Some(id) {
                    if ui.button("Stop Playing").clicked() {
                        state.media_request = Some(crate::home::MediaRequest::PlayPlaylist(None));
                        ui.close();
                    }
                } else if ui
                    .button("Play This Playlist")
                    .on_hover_text("Its pictures (8 seconds each) and videos (to their end) in turn, round and round")
                    .clicked()
                {
                    state.media_request = Some(crate::home::MediaRequest::PlayPlaylist(Some(id)));
                    ui.close();
                }
                announce_button(
                    ui,
                    state,
                    crate::announcements::Announce::MediaPlaylist(id),
                    "Its pictures and videos in turn over the slides, round and round, until it is cleared",
                );
                // A folder it follows: what is in the folder is in the playlist.
                match state.media_playlists[at].folder.clone() {
                    Some(folder) => {
                        ui.label(
                            egui::RichText::new(format!("Follows {}", folder.display())).size(11.5).color(TEXT_DIM),
                        );
                        if ui.button("Stop Following the Folder").clicked() {
                            state.media_request = Some(crate::home::MediaRequest::WatchFolder(id, false));
                            ui.close();
                        }
                    }
                    None => {
                        if ui
                            .button("Follow a Folder…")
                            .on_hover_text(
                                "The pictures and videos in a folder join this playlist as they come and change",
                            )
                            .clicked()
                        {
                            state.media_request = Some(crate::home::MediaRequest::WatchFolder(id, true));
                            ui.close();
                        }
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Name").color(TEXT_DIM));
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut state.media_playlists[at].name)
                            .id(name_field)
                            .desired_width(140.0),
                    );
                    state.media_playlists_changed |= field.changed();
                    // Just made: the name takes the keyboard, all of it picked so typing replaces it.
                    if ui.data_mut(|data| data.remove_temp::<bool>(ask_name)).unwrap_or(false) {
                        field.request_focus();
                        if let Some(mut edit) = egui::text_edit::TextEditState::load(ui.ctx(), name_field) {
                            let all = state.media_playlists[at].name.chars().count();
                            edit.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(all),
                            )));
                            edit.store(ui.ctx(), name_field);
                        }
                    }
                });
                if ui.button("Delete Playlist").on_hover_text("The media stays in All Media").clicked() {
                    state.media_playlists.remove(at);
                    state.media_playlist_shown = None;
                    state.media_playlists_changed = true;
                    ui.close();
                }
            }
        });
}

/// A media item's right-click menu: how it fills the screen, whether a video loops, the media
/// playlists it is in, and last Delete… (from the media bin, after asking); in a media
/// playlist's own view, Remove from that playlist instead, at once.
fn media_item_menu(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, index: usize, item: &sp_model::MediaItem) {
    use crate::home::MediaRequest;
    ui.menu_button("Scaling", |ui| {
        for (fit, name, hint) in [
            (sp_model::ImageFit::Fill, "Scale to Fill", "Fill the screen, cutting off what hangs over"),
            (sp_model::ImageFit::Fit, "Scale to Fit", "Show all of it, with bars where its shape differs"),
            (sp_model::ImageFit::Stretch, "Stretch to Fill", "Stretch it to the screen's shape"),
        ] {
            if ui.add(egui::Button::selectable(item.fit == fit, name)).on_hover_text(hint).clicked() {
                state.media_request = Some(MediaRequest::Look(index, fit, item.looping));
                ui.close();
            }
        }
    });
    if crate::media::is_video(&item.path)
        && ui
            .add(egui::Button::selectable(item.looping, "Loop"))
            .on_hover_text("Start again at the end; otherwise stay on the last picture")
            .clicked()
    {
        state.media_request = Some(MediaRequest::Look(index, item.fit, !item.looping));
        ui.close();
    }
    if ui
        .button("Add to Song as Slide")
        .on_hover_text("A slide that shows this when it goes on screen, after the song's chosen slide")
        .clicked()
    {
        state.media_request = Some(MediaRequest::AddAsSlide(index));
        ui.close();
    }
    let front = state.services.front().name.clone();
    if ui
        .button(format!("Add to Playlist \u{201c}{front}\u{201d}"))
        .on_hover_text("Put it in the running order of the playlist in front, to show it from there")
        .clicked()
    {
        state.services.add(crate::service::Item::Media { id: item.id, name: item.name.clone() });
        ui.close();
    }
    let hint = if crate::media::is_video(&item.path) {
        "Over the slides, looping, until it is cleared"
    } else {
        "Over the slides until it is cleared"
    };
    announce_button(ui, state, crate::announcements::Announce::Media(item.id), hint);
    ui.menu_button("Media Playlists", |ui| {
        if state.media_playlists.is_empty() {
            ui.label(
                egui::RichText::new("No media playlists yet. Make one from the menu beside MEDIA.").color(TEXT_DIM),
            );
        }
        for playlist in &mut state.media_playlists {
            if ui.add(egui::Button::selectable(playlist.has(&item.path), playlist.name.as_str())).clicked() {
                playlist.toggle(&item.path);
                state.media_playlists_changed = true;
            }
        }
    });
    ui.separator();
    // Every item picked out, when this is one of them.
    let these = if state.picked_media.contains(&index) { state.picked_media.clone() } else { vec![index] };
    let shown = state.media_playlist_shown.and_then(|id| state.media_playlists.iter().find(|p| p.id == id));
    match shown {
        Some(playlist) => {
            let words = remove_words(&these, &playlist.name);
            if !playlist.arranged_by_hand() {
                ui.add_enabled(false, egui::Button::new(words)).on_disabled_hover_text(FOLLOWS_A_FOLDER);
            } else if ui.button(words).on_hover_text("Taken out of this media playlist only").clicked() {
                remove_from_media_playlist(show, state, &these);
                ui.close();
            }
        }
        None => {
            if ui.button("Delete…").clicked() {
                state.confirm_delete_media = these;
                ui.close();
            }
        }
    }
}

/// Why nothing is taken out of, or moved in, a media playlist that follows a folder.
const FOLLOWS_A_FOLDER: &str = "It follows a folder: what is in the folder is in it, by name";

/// What taking items out of a media playlist is called: "Remove from “Loop”", or with several
/// "Remove 2 Items from “Loop”".
fn remove_words(these: &[usize], playlist: &str) -> String {
    match these.len() {
        1 => format!("Remove from \u{201c}{playlist}\u{201d}"),
        many => format!("Remove {many} Items from \u{201c}{playlist}\u{201d}"),
    }
}

/// Takes media bin items (by place) out of the media playlist shown, at once: they stay in the
/// bin and in any other playlist. Nothing is taken out of one that follows a folder.
pub fn remove_from_media_playlist(show: &Show, state: &mut HomeState, these: &[usize]) {
    let paths: Vec<&std::path::Path> =
        these.iter().filter_map(|at| show.media().get(*at)).map(|item| item.path.as_path()).collect();
    let Some(playlist) = state
        .media_playlist_shown
        .and_then(|id| state.media_playlists.iter_mut().find(|p| p.id == id))
        .filter(|playlist| playlist.arranged_by_hand())
    else {
        return;
    };
    let before = playlist.files.len();
    playlist.files.retain(|file| !paths.contains(&file.as_path()));
    if playlist.files.len() != before {
        state.media_playlists_changed = true;
        state.picked_media.retain(|at| !these.contains(at));
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use egui::{Event, Modifiers, PointerButton, RawInput, pos2};

    use super::*;
    use crate::home::{Picking, Sweep};

    fn rows() -> Vec<(usize, Rect)> {
        (0..3).map(|n| (5 + n, Rect::from_min_size(pos2(0.0, n as f32 * 32.0), vec2(200.0, 30.0)))).collect()
    }

    /// Every row of the list the tests' rows are in, in order.
    fn order() -> Vec<usize> {
        rows().iter().map(|(index, _)| *index).collect()
    }

    #[test]
    fn usual_in_a_transition_menu_names_what_it_follows() {
        let dissolve =
            sp_model::Transition { seconds: 0.4, ..sp_model::Transition::of(sp_model::TransitionKind::Dissolve) };
        assert_eq!(usual_transition(dissolve), "Usual (Dissolve, 0.4 s)");
        let cut = sp_model::Transition::of(sp_model::TransitionKind::Cut);
        assert_eq!(usual_transition(cut), "Usual (Cut)");
    }

    #[test]
    fn the_song_list_is_in_the_order_chosen_and_the_filter_still_applies() {
        let song = |title: &str, slides: usize| {
            let words = (0..slides).map(|n| format!("Line {n}\n")).collect::<Vec<_>>().join("\n");
            Arc::new(sp_model::parse_song_text(title, &words))
        };
        let show = Show::new(vec![song("Morning", 2), song("Evening", 5), song("Noon", 3)]);
        let mut state = HomeState::default();
        assert_eq!(song_list(&show, &state, None).shown, [0, 1, 2]);
        state.song_order = crate::kept::SongOrder::AZ;
        assert_eq!(song_list(&show, &state, None).shown, [1, 0, 2]);
        state.song_order = crate::kept::SongOrder::Longest;
        assert_eq!(song_list(&show, &state, None).shown, [1, 2, 0]);
        state.filter = "n".to_owned();
        let list = song_list(&show, &state, None);
        assert_eq!((list.total, list.shown.as_slice()), (3, &[1, 2, 0][..]), "every title has an n");
        state.filter = "noon".to_owned();
        assert_eq!(song_list(&show, &state, None).shown, [2]);
    }

    #[test]
    fn a_sweep_goes_on_picking_when_the_row_it_started_on_has_scrolled_out_of_view() {
        let list = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 300.0));
        let mut state = HomeState {
            sweep: Some(Sweep {
                list: Picking::Songs,
                from: Some(1),
                start: pos2(10.0, 15.0),
                keeps: false,
                carried: false,
            }),
            ..HomeState::default()
        };
        // Rows 5 to 7 are in view; the sweep started on row 1, now scrolled away above them.
        let all: Vec<usize> = (0..10).collect();
        in_frame(|ui| {
            sweep_rows(ui, &mut state, Picking::Songs, &rows(), &all, list, Some(pos2(10.0, 40.0)), true, |_| {})
        });
        assert_eq!(state.picked_songs, vec![1, 2, 3, 4, 5, 6]);
    }

    /// Runs `body` once in a frame, with nothing pressed.
    fn in_frame(body: impl FnOnce(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        let mut body = Some(body);
        let output = ctx.run_ui(RawInput::default(), |ui| {
            if let Some(body) = body.take() {
                body(ui);
            }
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn edit_opens_the_song_chosen_not_the_one_last_on_screen() {
        let song = |title: &str| Arc::new(sp_model::parse_song_text(title, "One\n\nTwo\n\nThree\n"));
        let mut show = Show::new(vec![song("New Song"), song("Sample song")]);
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
        show.apply(Command::Select(1));
        let mut state = HomeState::default();
        assert_eq!(slide_to_edit(&show, &state), SlideRef { presentation: 1, slide: 0 });
        // A slide of the chosen song picked out with a right-click is the one opened.
        state.selected_slide = Some(SlideRef { presentation: 1, slide: 1 });
        assert_eq!(slide_to_edit(&show, &state), SlideRef { presentation: 1, slide: 1 });
        // In the song on screen, Edit opens the slide on screen.
        state.selected_slide = None;
        show.apply(Command::Select(0));
        assert_eq!(slide_to_edit(&show, &state), SlideRef { presentation: 0, slide: 2 });
    }

    #[test]
    fn a_drag_down_the_song_list_picks_the_songs_it_passes_and_carries_them_out_of_it() {
        let list = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 300.0));
        let mut state = HomeState {
            sweep: Some(Sweep {
                list: Picking::Songs,
                from: Some(5),
                start: pos2(10.0, 15.0),
                keeps: false,
                carried: false,
            }),
            ..HomeState::default()
        };
        let mut carried = None;
        in_frame(|ui| {
            sweep_rows(ui, &mut state, Picking::Songs, &rows(), &order(), list, Some(pos2(10.0, 75.0)), true, |_| {});
            assert_eq!(state.picked_songs, vec![5, 6, 7], "from the first row to the one under the pointer");
            sweep_rows(ui, &mut state, Picking::Songs, &rows(), &order(), list, Some(pos2(10.0, 40.0)), true, |_| {});
            assert_eq!(state.picked_songs, vec![5, 6], "moving back up picks fewer");
            // Out of the list: the songs picked are carried, once.
            sweep_rows(
                ui,
                &mut state,
                Picking::Songs,
                &rows(),
                &order(),
                list,
                Some(pos2(10.0, 420.0)),
                true,
                |picked| {
                    carried = Some(picked);
                },
            );
            sweep_rows(ui, &mut state, Picking::Songs, &rows(), &order(), list, Some(pos2(10.0, 430.0)), true, |_| {
                panic!("carried twice")
            });
            sweep_rows(ui, &mut state, Picking::Songs, &rows(), &order(), list, Some(pos2(10.0, 430.0)), false, |_| {});
        });
        assert_eq!(carried, Some(vec![5, 6]));
        assert!(state.sweep.is_none(), "letting go ends the drag");
    }

    #[test]
    fn a_drag_on_one_of_several_picked_songs_carries_them_all_without_repicking() {
        let list = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 300.0));
        let mut state = HomeState {
            picked_songs: vec![7, 5],
            sweep: Some(Sweep {
                list: Picking::Songs,
                from: Some(7),
                start: pos2(10.0, 80.0),
                keeps: true,
                carried: false,
            }),
            ..HomeState::default()
        };
        let mut carried = None;
        in_frame(|ui| {
            sweep_rows(ui, &mut state, Picking::Songs, &rows(), &order(), list, Some(pos2(10.0, 20.0)), true, |_| {});
            assert_eq!(state.picked_songs, vec![7, 5]);
            sweep_rows(
                ui,
                &mut state,
                Picking::Songs,
                &rows(),
                &order(),
                list,
                Some(pos2(-30.0, 20.0)),
                true,
                |picked| {
                    carried = Some(picked);
                },
            );
        });
        assert_eq!(carried, Some(vec![5, 7]), "in the list's order, not the order picked");
    }

    /// Draws the media bin alone in a 1280 × 800 window for each frame of pointer events, as the
    /// app does, keeping `state` between them.
    fn media_frames(show: &Show, state: &mut HomeState, frames: &[Vec<Event>]) {
        let ctx = egui::Context::default();
        for (n, events) in frames.iter().enumerate() {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
                time: Some(n as f64 * 0.1),
                events: events.clone(),
                ..RawInput::default()
            };
            let output = ctx.run_ui(input, |ui| {
                let mut commands = Vec::new();
                media_strip(ui, show, state, &mut |_| None, &mut commands);
            });
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn a_list_too_long_for_its_room_shows_no_scroll_bar_and_its_right_edge_takes_clicks() {
        let ctx = styled();
        let style = ctx.global_style();
        assert_eq!(style.spacing.scroll.allocated_width(), 0.0, "no room is kept for a bar");
        assert_eq!(style.spacing.scroll.bar_width, 0.0, "no strip at the edge senses the pointer");
        // Forty rows in a list 200 high and 300 wide: a click 1 point in from its right edge,
        // where a bar would be, still reaches the first row.
        let (mut clicked, mut width) = (false, 0.0);
        let first_row = pos2(299.0, 12.0);
        let frames =
            [vec![Event::PointerMoved(first_row)], vec![press(first_row, true)], vec![press(first_row, false)], vec![]];
        for (n, events) in frames.into_iter().enumerate() {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 400.0))),
                time: Some(n as f64 * 0.1),
                events,
                ..RawInput::default()
            };
            let output = ctx.run_ui(input, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                egui::Frame::NONE.inner_margin(0.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        width = ui.available_width();
                        for row in 0..40 {
                            let response =
                                ui.add_sized([ui.available_width(), 24.0], egui::Button::new(format!("Row {row}")));
                            clicked |= row == 0 && response.clicked();
                        }
                    });
                });
            });
            output.drop_without_applying_deltas();
        }
        assert!(clicked, "the row under the right edge took the click");
        assert!(width >= 299.0, "the rows have the list's whole width, {width}");
    }

    #[test]
    fn a_songs_menu_for_a_playlists_item_doesnt_offer_that_playlist() {
        let mut state = HomeState::default();
        state.services.add_service();
        state.services.add_service();
        let front = state.services.front_at();
        let all: Vec<usize> = (0..state.services.list.len()).collect();
        assert_eq!(playlists_to_add_to(&state, false), all, "from the library, every playlist");
        let offered = playlists_to_add_to(&state, true);
        assert!(!offered.contains(&front), "not the playlist the song is in");
        assert_eq!(offered.len(), all.len() - 1, "the others");
    }

    #[test]
    fn a_songs_menu_from_a_playlist_acts_on_that_song_alone_and_deletes_it_last() {
        use crate::service::Item;
        // Songs Command-clicked in a library earlier are still picked out.
        let mut state = HomeState { picked_songs: vec![0, 1, 2], ..HomeState::default() };
        assert_eq!(songs_menu_acts_on(&state, 1, false), [0, 1, 2], "in the library, the songs picked with it");
        assert_eq!(songs_menu_acts_on(&state, 4, false), [4], "or this one, when it isn't among them");
        assert_eq!(songs_menu_acts_on(&state, 1, true), [1], "from a playlist, this song alone");
        // The item's menu: Delete is the last choice, named for what it does.
        fn words(shape: &egui::Shape, into: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| words(shape, into)),
                _ => {}
            }
        }
        let show = Show::new(vec![Arc::new(sp_model::parse_song_text("One", "Words\n"))]);
        state.song_keys = vec!["one".to_owned()];
        state.services.list[0].items = vec![Item::song("one", "One"), Item::song("two", "Two")];
        let ctx = styled();
        let mut drawn_words = Vec::new();
        let output = drawn(&ctx, vec![], |ui| {
            item_menu(ui, &show, &mut state, &mut Vec::new(), 0, &mut None, &mut SongAsks::default());
        });
        output.shapes.iter().for_each(|clipped| words(&clipped.shape, &mut drawn_words));
        output.drop_without_applying_deltas();
        let last: Vec<&str> = drawn_words.iter().rev().take(2).rev().map(String::as_str).collect();
        assert_eq!(last, ["Remove from Playlist", "Delete Song from Library\u{2026}"]);
        // The song's own part, from a playlist, has no Delete of its own.
        let mut drawn_words = Vec::new();
        let output = drawn(&ctx, vec![], |ui| {
            song_menu(ui, &show, &mut state, &mut Vec::new(), 0, &mut SongAsks::default(), true);
        });
        output.shapes.iter().for_each(|clipped| words(&clipped.shape, &mut drawn_words));
        output.drop_without_applying_deltas();
        assert!(drawn_words.iter().all(|words| !words.starts_with("Delete")), "{drawn_words:?}");
    }

    fn press(at: egui::Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    /// Runs the video's row of keys through frames of input, and returns what each frame asked
    /// of the video. The row is 316 wide from 20 in, so its bar runs from 118 to 224.
    fn video_frames(video: &crate::home::VideoLive, frames: &[Vec<Event>]) -> Vec<Option<crate::home::VideoRequest>> {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        let mut asked = Vec::new();
        let row = Rect::from_min_size(pos2(20.0, 20.0), vec2(316.0, 30.0));
        for (n, events) in frames.iter().enumerate() {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))),
                time: Some(n as f64 * 0.1),
                events: events.clone(),
                ..RawInput::default()
            };
            let output = ctx.run_ui(input, |ui| video_row(ui, &mut state, video, row));
            output.drop_without_applying_deltas();
            asked.push(state.video_request.take());
        }
        asked
    }

    #[test]
    fn the_playhead_follows_a_drag_and_the_video_goes_where_it_is_let_go() {
        use crate::home::VideoRequest;
        let video = crate::home::VideoLive {
            name: "Walk-in".to_owned(),
            position: 0.0,
            duration: Some(100.0),
            playing: true,
            looping: false,
            muted: false,
        };
        // A drag along the bar, in the middle of the row's height.
        let frames = vec![
            vec![Event::PointerMoved(pos2(130.0, 35.0))],
            vec![press(pos2(130.0, 35.0), true)],
            vec![Event::PointerMoved(pos2(170.0, 35.0))],
            vec![Event::PointerMoved(pos2(210.0, 35.0))],
            vec![press(pos2(210.0, 35.0), false)],
            vec![],
        ];
        let asked = video_frames(&video, &frames);
        let scrubs: Vec<f64> =
            asked.iter().filter_map(|a| if let Some(VideoRequest::Scrub(at)) = a { Some(*at) } else { None }).collect();
        assert!(!scrubs.is_empty(), "a drag along the bar scrubs: {asked:?}");
        let landed = asked.iter().find_map(|a| if let Some(VideoRequest::Seek(at)) = a { Some(*at) } else { None });
        let landed = landed.expect("letting go goes there");
        assert!(scrubs.windows(2).all(|w| w[0] <= w[1]), "dragging right moves on: {scrubs:?}");
        assert!(landed >= scrubs[0] && landed <= 100.0 && landed > 30.0, "{landed} after {scrubs:?}");
    }

    #[test]
    fn the_videos_keys_ask_for_what_they_say() {
        use crate::home::VideoRequest;
        let video = crate::home::VideoLive {
            name: "Walk-in".to_owned(),
            position: 5.0,
            duration: Some(100.0),
            playing: true,
            looping: false,
            muted: false,
        };
        // From the left: Play or Pause, Start Over, Forward; from the right: Sound, then Loop.
        let click = |x: f32| {
            let at = pos2(x, 35.0);
            let frames = vec![vec![Event::PointerMoved(at)], vec![press(at, true)], vec![press(at, false)], vec![]];
            video_frames(&video, &frames).into_iter().flatten().next()
        };
        assert_eq!(click(34.0), Some(VideoRequest::TogglePause));
        assert_eq!(click(65.0), Some(VideoRequest::Restart));
        assert_eq!(click(95.0), Some(VideoRequest::Seek(15.0)));
        assert_eq!(click(293.0), Some(VideoRequest::Looping(true)));
        assert_eq!(click(323.0), Some(VideoRequest::Muted(true)));
    }

    /// The media strip, with three pictures in the bin, and a media playlist of them in its own
    /// order, shown.
    fn strip_with_a_playlist(files: &[&str]) -> (Show, HomeState) {
        let mut show = Show::new(vec![Arc::new(sp_model::parse_song_text("Song", "Words\n"))]);
        for name in ["a.png", "b.png", "c.png"] {
            show.add_media(sp_model::MediaItem::new(name.into(), name.into()));
        }
        let mut playlist = crate::media::Playlist::new("Loop".to_owned());
        playlist.files = files.iter().map(std::path::PathBuf::from).collect();
        let state = HomeState {
            media_playlist_shown: Some(playlist.id),
            media_playlists: vec![playlist],
            ..HomeState::default()
        };
        (show, state)
    }

    /// Where the strip drew each item's name, left to right.
    fn names_in_strip(show: &Show, state: &mut HomeState) -> Vec<(String, egui::Pos2)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.pos)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        let mut names = Vec::new();
        for n in 0..2 {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
                time: Some(f64::from(n) * 0.1),
                ..RawInput::default()
            };
            let output = ctx.run_ui(input, |ui| media_strip(ui, show, state, &mut |_| None, &mut Vec::new()));
            names.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut names));
            output.drop_without_applying_deltas();
        }
        let mut names: Vec<(String, egui::Pos2)> =
            names.into_iter().filter(|(text, _)| text.ends_with(".PNG")).collect();
        names.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
        names
    }

    #[test]
    fn a_media_playlists_view_shows_its_items_in_the_order_they_play() {
        let (show, mut state) = strip_with_a_playlist(&["c.png", "a.png"]);
        let names: Vec<String> = names_in_strip(&show, &mut state).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["C.PNG", "A.PNG"], "the playlist's order, not the bin's");
        state.media_playlist_shown = None;
        let names: Vec<String> = names_in_strip(&show, &mut state).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["A.PNG", "B.PNG", "C.PNG"], "All Media is the bin's order");
        // What the menu offers there in place of Delete…, for one or several picked.
        assert_eq!(remove_words(&[2], "Loop"), "Remove from \u{201c}Loop\u{201d}");
        assert_eq!(remove_words(&[2, 0], "Loop"), "Remove 2 Items from \u{201c}Loop\u{201d}");
    }

    #[test]
    fn an_item_dragged_in_a_media_playlists_view_moves_there() {
        let (show, mut state) = strip_with_a_playlist(&["a.png", "b.png", "c.png"]);
        let names = names_in_strip(&show, &mut state);
        // From the middle of the first item's picture to the right half of the last.
        let from = names[0].1 + vec2(40.0, -40.0);
        let to = names[2].1 + vec2(110.0, -40.0);
        let frames = vec![
            vec![Event::PointerMoved(from)],
            vec![press(from, true)],
            vec![Event::PointerMoved(from + vec2(20.0, 0.0))],
            vec![Event::PointerMoved(to)],
            vec![Event::PointerMoved(to)],
            vec![press(to, false)],
            vec![],
        ];
        media_frames(&show, &mut state, &frames);
        let files: Vec<&str> = state.media_playlists[0].files.iter().filter_map(|file| file.to_str()).collect();
        assert_eq!(files, ["b.png", "c.png", "a.png"]);
        assert!(state.media_playlists_changed);
        // In All Media a drag from an item picks; nothing moves.
        let (show, mut state) = strip_with_a_playlist(&["a.png", "b.png", "c.png"]);
        state.media_playlist_shown = None;
        media_frames(&show, &mut state, &frames);
        assert!(!state.media_playlists_changed);
        // A playlist that follows a folder keeps the folder's order.
        let (show, mut state) = strip_with_a_playlist(&["a.png", "b.png", "c.png"]);
        state.media_playlists[0].folder = Some("/a".into());
        media_frames(&show, &mut state, &frames);
        assert!(!state.media_playlists_changed);
    }

    #[test]
    fn a_drag_between_media_items_draws_a_box_that_picks_every_item_it_touches() {
        let mut show = Show::new(vec![Arc::new(sp_model::parse_song_text("Song", "Words\n"))]);
        for name in ["a.png", "b.png", "c.png"] {
            show.add_media(sp_model::MediaItem::new(name.into(), name.into()));
        }
        let mut state = HomeState::default();
        // Pressed in the empty part of the strip, right of the items, then dragged left over
        // the first two (each is 144 wide, 12 apart, from 16 in).
        let start = pos2(1000.0, 740.0);
        let frames = vec![
            vec![Event::PointerMoved(start)],
            vec![press(start, true)],
            vec![Event::PointerMoved(pos2(900.0, 738.0))],
            vec![Event::PointerMoved(pos2(100.0, 730.0))],
            vec![Event::PointerMoved(pos2(100.0, 730.0))],
        ];
        media_frames(&show, &mut state, &frames);
        assert_eq!(state.picked_media, vec![0, 1, 2], "the box from x 1000 to x 100 touches all three");
        assert_eq!(state.picking, Picking::Media, "Delete now deletes from the media bin");
        let frames = vec![vec![press(pos2(100.0, 730.0), false)], vec![]];
        media_frames(&show, &mut state, &frames);
        assert!(state.sweep.is_none(), "letting go ends the box");
    }

    /// A context set up as the window is, with its fonts and look.
    pub fn styled() -> egui::Context {
        let ctx = egui::Context::default();
        apply_style(&ctx);
        // The fonts are taken up at the start of the next frame.
        ctx.run_ui(RawInput::default(), |_| {}).drop_without_applying_deltas();
        ctx
    }

    /// Runs one frame in a window 1280 by 800, with these events, and gives what it drew.
    pub fn drawn(ctx: &egui::Context, events: Vec<Event>, body: impl FnOnce(&mut egui::Ui)) -> egui::FullOutput {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            events,
            ..RawInput::default()
        };
        let mut body = Some(body);
        ctx.run_ui(raw, |ui| {
            if let Some(body) = body.take() {
                body(ui);
            }
        })
    }

    /// How a line of words was set, and where each of its letters was drawn: the face, the room
    /// between letters and the colour, then each letter's place in the window.
    pub type Letters = ((FontId, f32, Color32), Vec<(char, egui::Pos2)>);

    /// The letters of the line `words` a frame drew.
    pub fn letters(output: egui::FullOutput, words: &str) -> Letters {
        fn walk(shape: &egui::Shape, into: &mut Vec<egui::epaint::TextShape>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut texts));
        output.drop_without_applying_deltas();
        let text = texts.into_iter().find(|text| text.galley.job.text == words).expect(words);
        let format = &text.galley.job.sections[0].format;
        let row = &text.galley.rows[0];
        let places = row.row.glyphs.iter().map(|glyph| (glyph.chr, text.pos + row.pos.to_vec2() + glyph.pos.to_vec2()));
        ((format.font_id.clone(), format.extra_letter_spacing, format.color), places.collect())
    }

    /// Where a frame drew the line `words` (in a menu too), at its middle; `None` if it didn't.
    pub fn place_of(output: &egui::FullOutput, words: &str) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, words: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == words => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| walk(shape, words)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|clipped| walk(&clipped.shape, words))
    }

    /// Where the last of `words` drawn was: in an open menu, which is drawn over what is under it,
    /// when the same words are also in the window (Duplicate is a list key and a menu's choice).
    pub fn last_place_of(output: &egui::FullOutput, words: &str) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, words: &str, into: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == words => {
                    into.push(text.pos + text.galley.rect.center().to_vec2());
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, words, into)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, words, &mut found));
        found.pop()
    }

    /// The frames of a drag from `from` to `to`: the pointer presses there, goes in four steps,
    /// is held a frame, lets go, and one frame more.
    pub fn drag_frames(from: egui::Pos2, to: egui::Pos2) -> Vec<Vec<Event>> {
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut frames = vec![vec![Event::PointerMoved(from)], vec![button(from, true)]];
        for step in 1..=4 {
            frames.push(vec![Event::PointerMoved(from + (to - from) * (step as f32 / 4.0))]);
        }
        frames.extend([vec![], vec![button(to, false)], vec![]]);
        frames
    }

    /// The frames of a click at `at` with a button: the pointer goes there, presses, lets go,
    /// and one frame more for what the click opens.
    pub fn click_frames(at: egui::Pos2, button: PointerButton) -> [Vec<Event>; 4] {
        let press = |pressed| Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE };
        [vec![Event::PointerMoved(at)], vec![press(true)], vec![press(false)], Vec::new()]
    }

    /// Two lines' letters are set the same way, each on the same spot.
    pub fn same_letters(shown: &Letters, typed: &Letters, what: &str) {
        assert_eq!(shown.0, typed.0, "{what}: set differently");
        assert_eq!(shown.1.len(), typed.1.len(), "{what}: other letters");
        for ((was, at), (now, to)) in shown.1.iter().zip(&typed.1) {
            assert_eq!(was, now, "{what}");
            assert!(
                at.distance(*to) < 0.001,
                "{what}: {was} moved from {:.5} {:.5} to {:.5} {:.5}",
                at.x,
                at.y,
                to.x,
                to.y
            );
        }
    }

    /// A key pressed, with nothing held.
    pub fn key(key: egui::Key) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    /// A context with only the toolkit's own fonts under the window's family names, as on a
    /// computer that has none of the fonts the window looks for.
    fn styled_without_the_computers_fonts() -> egui::Context {
        let ctx = styled();
        let mut fonts = egui::FontDefinitions::default();
        let plain = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
        let fixed = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
        fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), plain);
        fonts.families.insert(FontFamily::Name(FIXED_SEMIBOLD.into()), fixed);
        ctx.set_fonts(fonts);
        ctx.run_ui(RawInput::default(), |_| {}).drop_without_applying_deltas();
        ctx
    }

    #[test]
    fn a_name_retyped_in_place_keeps_its_letters_where_and_as_they_were_and_the_room_it_was_given() {
        // The faces and sizes the window's names are set in, the capitals of a heading, and a
        // name with letters only the wide face has (where the computer has one).
        let mut looks = vec![(Lettering::cap(TEXT_DIM), "Worship \u{793c}\u{62dd}")];
        for size in [11.5, 12.0, 12.5, 13.0, 13.5, 14.0] {
            looks.push((Lettering::plain(FontId::proportional(size), TEXT_DIM), "Evening"));
            looks.push((Lettering::plain(semibold(size), TEXT), "Sunday Morning"));
            looks.push((Lettering::plain(fixed(size), TEXT_FAINT), "Kids 10:30"));
        }
        // Where names start: the toolkit lays the window out on a grid of 1/32 of a point, and
        // so does the field; a name starts on it wherever it is drawn.
        let starts = [pos2(40.0, 30.0), pos2(40.25, 30.5), pos2(46.3125, 21.75), pos2(52.59375, 47.09375)];
        let cases: Vec<_> = looks.iter().flat_map(|look| starts.map(|start| (start, look))).collect();
        for ctx in [styled(), styled_without_the_computers_fonts()] {
            for &(start, (look, words)) in &cases {
                let what = format!("{words} in {:?} at {start:?}", look.font);
                let shown = if look.capitals { words.to_uppercase() } else { (*words).to_owned() };
                let label = drawn(&ctx, vec![], |ui| {
                    if look.capitals {
                        cap(ui, start, Align2::LEFT_CENTER, words, look.ink);
                    } else {
                        ui.painter().text(start, Align2::LEFT_CENTER, words, look.font.clone(), look.ink);
                    }
                });
                let shown_letters = letters(label, &shown);
                let mut typed = (*words).to_owned();
                let mut place = Rect::NOTHING;
                let mut field = |ctx: &egui::Context| {
                    drawn(ctx, vec![], |ui| {
                        let response = rename_field_at(ui, &mut typed, look, start, 300.0);
                        response.request_focus();
                        place = response.rect;
                    })
                };
                field(&ctx).drop_without_applying_deltas();
                let typed_letters = letters(field(&ctx), &shown);
                same_letters(&shown_letters, &typed_letters, &what);
                assert!(ctx.memory(|memory| memory.focused().is_some()), "{what}: it has the keyboard");
                let height = ctx.fonts_mut(|fonts| {
                    let laid = fonts.layout_no_wrap(shown.clone(), look.font.clone(), look.ink).size().y;
                    laid.max(fonts.row_height(&look.font))
                });
                // The edge goes round the room given, 4.5 out at the sides and 2.5 above and
                // below the line, centred on it (taller by at most the toolkit's grid).
                let across = (place.min.x - (start.x - 4.5)).abs().max((place.max.x - 304.5).abs());
                assert!(across < 0.001, "{what}: {place:?} is not the room given");
                assert!((place.center().y - start.y).abs() < 0.001, "{what}: {place:?} is not centred on the line");
                let extra = place.height() - 5.0 - height;
                let grid = egui::emath::GUI_ROUNDING;
                assert!((-0.001..=2.0 * grid + 0.001).contains(&extra), "{what}: {place:?} is {extra} taller");
            }
        }
    }

    #[test]
    fn another_playlist_renamed_in_the_chooser_leaves_the_one_in_front_open() {
        use crate::service::{Item, Service};
        let ctx = styled();
        let show = Show::new(Vec::new());
        let mut state = HomeState { playlist_open: true, chooser_open: true, ..HomeState::default() };
        state.services.list[0].name = "Morning".to_owned();
        state.services.list.push(Service { name: "Evening".to_owned(), items: vec![Item::song("a", "A")] });
        state.services.current = 1;
        state.services.chosen = Some(0);
        let frame = |state: &mut HomeState, events: Vec<Event>| {
            drawn(&ctx, events, |ui| chooser(ui, &show, state, &mut Vec::new())).drop_without_applying_deltas();
        };
        state.renaming_service = Some((0, "Morning".to_owned()));
        state.rename_started = true;
        frame(&mut state, vec![]);
        frame(&mut state, vec![Event::Text(" Service".to_owned())]);
        frame(&mut state, vec![key(egui::Key::Enter)]);
        assert!(state.renaming_service.is_none());
        assert_eq!(state.services.list[0].name, "Morning Service", "renamed where it is");
        assert_eq!(state.services.list[1].name, "Evening");
        assert_eq!((state.services.current, state.services.chosen), (1, Some(0)), "Evening stays open, its item too");
    }

    #[test]
    fn a_playlist_or_library_renamed_in_the_chooser_stays_put_return_keeps_it_and_escape_puts_it_back() {
        let ctx = styled();
        let show = Show::new(Vec::new());
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.services.list[0].name = "Sunday Morning".to_owned();
        state.libraries.push(crate::libraries::Library::new("Hymns".to_owned()));
        let library = state.libraries[0].id;
        let height = std::cell::Cell::new(0.0);
        let chooser_frame = |state: &mut HomeState, events: Vec<Event>| {
            drawn(&ctx, events, |ui| {
                chooser(ui, &show, state, &mut Vec::new());
                height.set(ui.min_rect().height());
            })
        };
        let playlist = letters(chooser_frame(&mut state, vec![]), "Sunday Morning");
        let hymns = letters(chooser_frame(&mut state, vec![]), "Hymns");
        let tall = height.get();
        // The open playlist's name is set heavier; a library not open, plainly.
        assert_eq!(playlist.0.0, semibold(14.0));
        assert_eq!(hymns.0.0, FontId::proportional(14.0));
        for which in ["playlist", "library"] {
            let (words, before) = if which == "playlist" { ("Sunday Morning", &playlist) } else { ("Hymns", &hymns) };
            for (press, kept) in [(egui::Key::Enter, true), (egui::Key::Escape, false)] {
                if which == "playlist" {
                    state.renaming_service = Some((0, words.to_owned()));
                } else {
                    state.renaming_library = Some((library, words.to_owned()));
                }
                state.rename_started = true;
                chooser_frame(&mut state, vec![]).drop_without_applying_deltas();
                same_letters(before, &letters(chooser_frame(&mut state, vec![]), words), which);
                assert_eq!(height.get(), tall, "{which}: the chooser is as tall as it was");
                chooser_frame(&mut state, vec![Event::Text(" Two".to_owned())]).drop_without_applying_deltas();
                chooser_frame(&mut state, vec![key(press)]).drop_without_applying_deltas();
                let name = if which == "playlist" {
                    state.services.front().name.clone()
                } else {
                    state.libraries[0].name.clone()
                };
                assert!(state.renaming_service.is_none() && state.renaming_library.is_none(), "{which}: done");
                assert_eq!(name, if kept { format!("{words} Two") } else { words.to_owned() }, "{which}");
                // Back as it was, for the next round.
                state.services.list[0].name = "Sunday Morning".to_owned();
                state.libraries[0].name = "Hymns".to_owned();
            }
        }
    }

    #[test]
    fn a_header_or_placeholder_renamed_in_the_playlist_keeps_its_capitals_spacing_and_place() {
        use crate::service::Item;
        let ctx = styled();
        let show = Show::new(Vec::new());
        let mut state = HomeState { playlist_open: true, ..HomeState::default() };
        state.services.list[0].items =
            vec![Item::Header { name: "Worship".to_owned() }, Item::Placeholder { name: "Offering".to_owned() }];
        let mut commands = Vec::new();
        let mut items_frame = |state: &mut HomeState, events: Vec<Event>| {
            drawn(&ctx, events, |ui| playlist_items(ui, &show, state, &mut commands))
        };
        let header = letters(items_frame(&mut state, vec![]), "WORSHIP");
        let placeholder = letters(items_frame(&mut state, vec![]), "Offering");
        assert_eq!(header.0, (fixed_semibold(11.5), 0.6, TEXT_DIM), "as a small capital label");
        for (at, words, before) in [(0, "WORSHIP", &header), (1, "Offering", &placeholder)] {
            let typed = state.services.front().items[at].title().to_owned();
            state.renaming_header = Some((at, typed));
            state.rename_started = true;
            items_frame(&mut state, vec![]).drop_without_applying_deltas();
            same_letters(before, &letters(items_frame(&mut state, vec![]), words), words);
            items_frame(&mut state, vec![key(egui::Key::Escape)]).drop_without_applying_deltas();
            assert!(state.renaming_header.is_none(), "{words}: Escape ends it");
        }
        assert_eq!(state.services.front().items[0].title(), "Worship", "Escape keeps the name it had");
    }
}
