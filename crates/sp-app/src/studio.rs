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
#[allow(dead_code)] // Used as each page of the new design lands.
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
#[allow(dead_code)] // Used as each page of the new design lands.
pub const KEY_EDGE: Color32 = Color32::from_rgb(0x05, 0x05, 0x05);

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
    });
}

/// The name of the heavier cut of the interface font, for headings and buttons.
const SEMIBOLD: &str = "semibold";

/// The fixed-width face at a size.
#[allow(dead_code)] // Used as each page of the new design lands.
pub fn fixed(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// The heavier cut of the interface font at a size.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
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
    if let Some(bytes) = FIXED.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts.font_data.insert("fixed".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "fixed".to_owned());
    }
    if let Some(bytes) = WIDE.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts.font_data.insert("wide".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in fonts.families.values_mut() {
            family.push("wide".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// A field for renaming something in place. It keeps the look of the text it stands in for
/// (same font, same colour, no filled box) and adds only a thin outline, so the words stay
/// where they were and it is plain that they can now be typed over.
fn rename_field<'t>(text: &'t mut String, font: FontId, ink: Color32, outline: Color32) -> egui::TextEdit<'t> {
    egui::TextEdit::singleline(text)
        .font(font)
        .text_color(ink)
        .background_color(Color32::TRANSPARENT)
        .min_size(egui::Vec2::ZERO)
        .frame(
            Frame::new().stroke(Stroke::new(1.0, outline)).corner_radius(4).inner_margin(egui::Margin::symmetric(4, 0)),
        )
}

/// Puts a field for retyping a name exactly over the name, so nothing around it moves. `start`
/// is where the name's text begins, at the middle of its line; the field's text begins there
/// too, the field is one line of the font tall, and it reaches `right`. It takes no room in
/// the layout.
pub fn rename_field_at(
    ui: &mut egui::Ui,
    text: &mut String,
    font: FontId,
    ink: Color32,
    outline: Color32,
    start: egui::Pos2,
    right: f32,
) -> egui::Response {
    let height = ui.fonts_mut(|fonts| fonts.row_height(&font)) + 2.0;
    // The field's frame is a line of 1 and a margin of 4, so its text lines up with the name's.
    let rect = Rect::from_min_max(
        egui::pos2(start.x - 5.0, start.y - height / 2.0),
        egui::pos2(right.max(start.x + 40.0), start.y + height / 2.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(rect).layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    // The theme gives fields a least height taller than a line of text; not here.
    child.spacing_mut().interact_size = egui::Vec2::ZERO;
    child.add(rename_field(text, font, ink, outline))
}

/// A small heading in capitals, as each section starts with.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(semibold(10.5)).color(TEXT_FAINT).extra_letter_spacing(0.8));
}

/// A button drawn as a rounded block with its words in the middle. `emphasis` picks its
/// colours: plain, or live (tally red).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Plain,
    Live,
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
        Emphasis::Live => {
            painter.rect_filled(rect, RADIUS, if hovered { TALLY } else { TALLY.gamma_multiply(0.88) });
            Color32::WHITE
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

/// Several choices in one rounded bar, the chosen one raised. Returns the one clicked.
pub fn segmented(
    ui: &mut egui::Ui,
    salt: &str,
    each: egui::Vec2,
    labels: &[&str],
    chosen: Option<usize>,
) -> Option<usize> {
    let count = labels.len();
    let total = each.x * count as f32;
    let (bar, _) = ui.allocate_exact_size(vec2(total + 6.0, each.y + 6.0), Sense::hover());
    ui.painter().rect_filled(bar, RADIUS + 2.0, BG);
    // Many choices share the width, so their words are smaller.
    let mut size: f32 = match count {
        0..=4 => 13.5,
        5 => 12.5,
        _ => 11.5,
    };
    // Each word, measured at a size (in the bolder face, the wider of the two).
    const PAD: f32 = 10.0;
    let measure = |ui: &egui::Ui, size: f32| -> Vec<f32> {
        labels
            .iter()
            .map(|label| ui.painter().layout_no_wrap((*label).to_owned(), semibold(size), TEXT).size().x)
            .collect()
    };
    let mut widths = measure(ui, size);
    // In a narrow bar the words shrink a little until they fit side by side.
    while widths.iter().sum::<f32>() + PAD * count as f32 > total && size > 9.5 {
        size -= 0.5;
        widths = measure(ui, size);
    }
    // Equal cells while every word fits in one; otherwise each takes room for its word, and the
    // room left over is shared out evenly.
    let crowded = widths.iter().any(|width| width + PAD > each.x);
    let spare = ((total - widths.iter().sum::<f32>()) / count.max(1) as f32).max(0.0);
    let cells: Vec<f32> =
        if crowded { widths.iter().map(|width| width + spare).collect() } else { vec![each.x; count] };
    let mut clicked = None;
    let mut x = bar.min.x + 3.0;
    for (index, label) in labels.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(x, bar.min.y + 3.0), vec2(cells[index], each.y));
        x += cells[index];
        let response = ui.interact(cell, ui.scope_id().with((salt, index)), Sense::click());
        let on = chosen == Some(index);
        if on {
            ui.painter().rect_filled(cell, RADIUS, CARD);
        } else if response.hovered() {
            ui.painter().rect_filled(cell, RADIUS, PANEL);
        }
        let ink = if on { TEXT } else { TEXT_DIM };
        let font = if on { semibold(size) } else { FontId::proportional(size) };
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *label, font, ink);
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// The bar along the top: the three views, themes and more on the left; the audience output
/// on the right.
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

pub fn toolbar(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let frame = Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(14, 9));
    egui::Panel::top("toolbar").resizable(false).show_separator_line(false).exact_size(TOP_BAR).frame(frame).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                // The four views. Themes is one of them: it opens the Theme Editor.
                let editing_theme = state.theme_edit.is_some();
                let view = if editing_theme {
                    3
                } else if state.editing.is_some() {
                    1
                } else if state.bible_open {
                    2
                } else {
                    0
                };
                let picked = segmented(ui, "views", vec2(74.0, 28.0), &["Show", "Edit", "Bible", "Themes"], Some(view))
                    .or(state.view_request.take().filter(|asked| *asked != view));
                match picked {
                    Some(3) if !editing_theme => {
                        state.theme_request = Some(crate::home::ThemeRequest::Edit(state.theme_for_songs))
                    }
                    Some(3) => {}
                    // Leaving the Theme Editor goes through the app, which saves the theme.
                    Some(to) if editing_theme => {
                        if let Some(edit) = state.theme_edit.as_mut() {
                            edit.leave = true;
                        }
                        state.bible_open = to == 2;
                        if to == 1 {
                            state.open_editor_next = true;
                        }
                    }
                    Some(0) => {
                        state.editing = None;
                        state.bible_open = false;
                    }
                    Some(2) => {
                        state.editing = None;
                        state.bible_open = true;
                    }
                    Some(1) if state.editing.is_none() => {
                        state.editing = EditDraft::of(show, slide_to_edit(show, state));
                    }
                    _ => {}
                }
                // Asked for while the Theme Editor was open: it has closed now, so open the slide editor.
                if state.open_editor_next && state.theme_edit.is_none() {
                    state.open_editor_next = false;
                    state.editing = EditDraft::of(show, slide_to_edit(show, state));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Settings, at the far right.
                    let (gear, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                    let on = state.preferences_open;
                    paint_button(ui.painter(), gear, on || response.hovered());
                    icons::paint(
                        ui.painter(),
                        Icon::Gear,
                        Rect::from_center_size(gear.center(), vec2(15.0, 15.0)),
                        TEXT_DIM,
                    );
                    if response.on_hover_text("Settings (Command-comma)").clicked() {
                        state.preferences_open = !state.preferences_open;
                    }
                    // Laid out from the right: Stage, then Audience to its left, so Audience reads first.
                    // The stage screens: a light and whether they are on.
                    let stage_on = state.stage_on;
                    let (rect, response) = ui.allocate_exact_size(vec2(118.0, 30.0), Sense::click());
                    let painter = ui.painter();
                    paint_button(painter, rect, response.hovered());
                    painter.circle_filled(
                        rect.left_center() + vec2(15.0, 0.0),
                        4.5,
                        if stage_on { READY } else { TEXT_FAINT },
                    );
                    painter.text(
                        rect.left_center() + vec2(28.0, 0.0),
                        Align2::LEFT_CENTER,
                        "Stage",
                        FontId::proportional(13.5),
                        TEXT,
                    );
                    painter.text(
                        rect.right_center() - vec2(12.0, 0.0),
                        Align2::RIGHT_CENTER,
                        if stage_on { "ON" } else { "OFF" },
                        FontId::proportional(11.0),
                        if stage_on { READY } else { TEXT_DIM },
                    );
                    let hint = if stage_on {
                        "The stage screens are on. Click to switch them off (Command-2)."
                    } else {
                        "The stage screens are off. Click to switch them on (Command-2)."
                    };
                    if response.on_hover_text(hint).clicked() {
                        state.stage_on = !stage_on;
                    }
                    // The audience output: a tally light, its name, and whether it is on.
                    let on = show.output_enabled();
                    let (rect, response) = ui.allocate_exact_size(vec2(148.0, 30.0), Sense::click());
                    let painter = ui.painter();
                    paint_button(painter, rect, response.hovered());
                    let light = if on { READY } else { TEXT_FAINT };
                    painter.circle_filled(rect.left_center() + vec2(15.0, 0.0), 4.5, light);
                    painter.text(
                        rect.left_center() + vec2(28.0, 0.0),
                        Align2::LEFT_CENTER,
                        "Audience",
                        FontId::proportional(13.5),
                        TEXT,
                    );
                    painter.text(
                        rect.right_center() - vec2(12.0, 0.0),
                        Align2::RIGHT_CENTER,
                        if on { "ON" } else { "OFF" },
                        FontId::proportional(11.0),
                        if on { READY } else { TEXT_DIM },
                    );
                    let hint = if on {
                        "The audience output is on. Click to switch it off."
                    } else {
                        "The audience output is off. Click to switch it on."
                    };
                    if response.on_hover_text(hint).clicked() {
                        commands.push(Command::SetOutputEnabled(!on));
                    }
                });
            });
        },
    );
}

/// One row of a list in the left column. Returns its response; `live` puts a tally light at
/// the right, and `trailing` is room kept clear there for a button.
fn list_row(ui: &mut egui::Ui, icon: Icon, title: &str, chosen: bool, live: bool, missing: bool) -> egui::Response {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click_and_drag());
    let painter = ui.painter();
    if chosen {
        painter.rect_filled(row, RADIUS, ACCENT.gamma_multiply(0.22));
    } else if response.hovered() {
        painter.rect_filled(row, RADIUS, CARD);
    }
    let ink = if missing {
        TEXT_FAINT
    } else if chosen {
        Color32::WHITE
    } else {
        TEXT
    };
    icons::paint(
        painter,
        icon,
        Rect::from_center_size(row.left_center() + vec2(15.0, 0.0), vec2(13.0, 13.0)),
        TEXT_FAINT,
    );
    if live {
        painter.circle_filled(row.right_center() - vec2(12.0, 0.0), 3.5, TALLY);
    }
    let clip = Rect::from_min_max(row.min, pos2(row.max.x - 26.0, row.max.y));
    painter.with_clip_rect(clip).text(
        row.left_center() + vec2(30.0, 0.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.5),
        ink,
    );
    response
}

/// Songs being dragged from the library, by their places among the songs.
#[derive(Clone)]
struct SongDrag(Vec<usize>);

/// A row in the list of libraries and playlists: an icon and a name, renamed in place.
/// Shows when a dragged song would be dropped on it.
/// The VoxLive item's row under LIBRARY, once VoxLive is connected and has screens: its mark
/// in VoxLive's colour; a click opens its slides (one a screen), and its right-click menu puts
/// it in the playlist in front.
fn voxlive_row(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let Some(slot) = state.voxlive_slot else { return };
    let screens = show.presentations().get(slot).map_or(0, |item| item.slide_count());
    if screens == 0 {
        return;
    }
    let chosen = show.selected() == slot && !state.playlist_open;
    let response =
        list_row(ui, Icon::Live, "VoxLive", chosen, show.live().is_some_and(|at| at.presentation == slot), false);
    // The mark over the row's icon, in VoxLive's colour, so it reads as VoxLive's, not a song.
    let mark = Rect::from_center_size(response.rect.left_center() + vec2(15.0, 0.0), vec2(16.0, 16.0));
    ui.painter().rect_filled(mark, 4.0, crate::integrations::VOXLIVE);
    ui.painter().text(mark.center(), Align2::CENTER_CENTER, "V", semibold(10.0), Color32::WHITE);
    let count = if screens == 1 { "1 screen".to_owned() } else { format!("{screens} screens") };
    ui.painter().text(
        response.rect.right_center() - vec2(8.0, 0.0),
        Align2::RIGHT_CENTER,
        count,
        FontId::proportional(11.0),
        crate::integrations::VOXLIVE_LIGHT,
    );
    let front = state.services.front().name.clone();
    let response =
        response.on_hover_text("Your VoxLive screens, a slide each: click one to put its captions on the screens");
    if response.clicked() {
        state.playlist_open = false;
        state.bible_open = false;
        commands.push(Command::Select(slot));
    }
    response.context_menu(|ui| {
        if ui.button(format!("Add to Playlist \u{201c}{front}\u{201d}")).clicked() {
            state.services.add(crate::service::Item::Song {
                key: crate::voxlive::ITEM_KEY.to_owned(),
                title: "VoxLive".to_owned(),
            });
            ui.close();
        }
    });
}

fn source_row(ui: &mut egui::Ui, icon: Icon, name: &str, chosen: bool) -> egui::Response {
    let response = list_row(ui, icon, name, chosen, false, false);
    if response.dnd_hover_payload::<SongDrag>().is_some() {
        ui.painter().rect_stroke(response.rect, RADIUS, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
    }
    response
}

/// A heading over a group of rows, with a + and an arrow that folds the rows away.
/// Returns whether + was clicked.
fn source_heading(ui: &mut egui::Ui, text: &str, collapsed: &mut bool, add_hint: &str) -> bool {
    let mut add = false;
    ui.horizontal(|ui| {
        caption(ui, text);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (arrow, hint) = if *collapsed { ("⌄", "Show") } else { ("⌃", "Fold away") };
            if ui
                .add(egui::Button::new(egui::RichText::new(arrow).color(TEXT_FAINT)).frame(false))
                .on_hover_text(hint)
                .clicked()
            {
                *collapsed = !*collapsed;
            }
            let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
            let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
            icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
            add = response.on_hover_text(add_hint).clicked();
        });
    });
    add
}

/// The left column: the libraries and the playlists as rows on top, the
/// songs or items of the one chosen below, and a filter at the bottom.
pub fn library(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let frame = column_frame(Column::Left).inner_margin(egui::Margin {
        left: 10,
        right: 10,
        top: 14,
        bottom: COLUMN_BOTTOM as i8,
    });
    let panel = egui::Panel::left("library")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(column_outer(LEFT_WIDTH))
        .frame(frame);
    let shown = panel.show(ui, |ui| {
        let column_height = ui.available_height();
        let rows = state.libraries.len() + state.services.list.len() + 1;
        let fitted = (rows as f32 * 32.0 + 70.0).min(column_height * 0.45);
        // The line between the rows on top and the list under them can be dragged.
        let sources = egui::Panel::top("sources")
            .resizable(true)
            .size_range(90.0..=(column_height * 0.7).max(90.0))
            .default_size(state.service_height.unwrap_or(fitted))
            .frame(Frame::new());
        let shown = sources.show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("sources").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                source_rows(ui, show, state, commands);
            });
        });
        state.service_height = Some(shown.response.rect.height());

        // The filter, fixed to the bottom.
        egui::Panel::bottom("filter")
            .frame(Frame::new().inner_margin(egui::Margin { top: 8, ..Default::default() }))
            .show(ui, |ui| {
                let hint = if state.playlist_open { "Filter" } else { "Search songs" };
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(10, 6))
                        .background_color(CARD),
                );
            });
        ui.add_space(10.0);
        if state.playlist_open {
            playlist_items(ui, show, state, commands);
        } else {
            library_songs(ui, show, state, commands);
        }
    });
    round_corners(ui, column_panel(shown.response.rect, Column::Left));
}

/// The LIBRARY rows (All Songs and each library) and the PLAYLISTS rows. A song dragged from
/// the list below and dropped on one goes into it.
fn source_rows(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    let song_item = |state: &HomeState, index: usize| {
        let key = state.song_keys.get(index)?;
        let song = show.presentations().get(index)?;
        Some(Item::Song { key: key.clone(), title: song.title.clone() })
    };
    if source_heading(ui, "LIBRARY", &mut state.library_collapsed, "New Library") {
        let made = crate::libraries::Library::new(crate::libraries::new_name(&state.libraries));
        state.renaming_library = Some((made.id, made.name.clone()));
        state.rename_started = true;
        state.library_shown = Some(made.id);
        state.playlist_open = false;
        state.libraries.push(made);
        state.libraries_changed = true;
    }
    if !state.library_collapsed {
        let all = source_row(ui, Icon::Library, "All Songs", !state.playlist_open && state.library_shown.is_none());
        if all.clicked() {
            state.library_shown = None;
            state.playlist_open = false;
        }
        voxlive_row(ui, show, state, commands);
        let mut delete = None;
        let ids: Vec<(sp_model::Uuid, String)> =
            state.libraries.iter().map(|library| (library.id, library.name.clone())).collect();
        for (id, name) in ids {
            let renaming = state.renaming_library.as_ref().is_some_and(|(have, _)| *have == id);
            let chosen = !state.playlist_open && state.library_shown == Some(id);
            let shown = if name.trim().is_empty() { "Untitled" } else { name.as_str() };
            let response = source_row(ui, Icon::Library, if renaming { "" } else { shown }, chosen);
            if renaming && let Some((_, typed)) = state.renaming_library.as_mut() {
                let field = rename_field_at(
                    ui,
                    typed,
                    FontId::proportional(13.5),
                    TEXT,
                    ACCENT,
                    response.rect.left_center() + vec2(26.0, 0.0),
                    response.rect.max.x - 4.0,
                );
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
                continue;
            }
            if response.clicked() {
                state.library_shown = Some(id);
                state.playlist_open = false;
            }
            if response.double_clicked() {
                state.renaming_library = Some((id, name.clone()));
                state.rename_started = true;
            }
            if let Some(dropped) = response.dnd_release_payload::<SongDrag>() {
                let keys: Vec<String> = dropped.0.iter().filter_map(|at| state.song_keys.get(*at).cloned()).collect();
                if let Some(library) = state.libraries.iter_mut().find(|library| library.id == id) {
                    for key in keys {
                        if !library.has(&key) {
                            library.toggle(&key);
                            state.libraries_changed = true;
                        }
                    }
                }
            }
            response.on_hover_text("Drag songs here to add them").context_menu(|ui| {
                if ui.button("Rename").clicked() {
                    state.renaming_library = Some((id, name.clone()));
                    state.rename_started = true;
                    ui.close();
                }
                if ui.button("Delete Library").on_hover_text("The songs stay in All Songs").clicked() {
                    delete = Some(id);
                    ui.close();
                }
            });
        }
        if let Some(id) = delete {
            state.libraries.retain(|library| library.id != id);
            if state.library_shown == Some(id) {
                state.library_shown = None;
            }
            state.libraries_changed = true;
        }
    }
    ui.add_space(8.0);
    if source_heading(ui, "PLAYLISTS", &mut state.playlists_collapsed, "New Playlist") {
        state.services.add_service();
        state.playlist_open = true;
        state.renaming_service = Some(state.services.front().name.clone());
        state.rename_started = true;
    }
    if state.playlists_collapsed {
        return;
    }
    let names: Vec<String> = state.services.list.iter().map(|playlist| playlist.name.clone()).collect();
    let mut delete = false;
    for (index, name) in names.iter().enumerate() {
        let front = index == state.services.current;
        let renaming = front && state.renaming_service.is_some();
        let response = source_row(ui, Icon::Rows, if renaming { "" } else { name }, state.playlist_open && front);
        if renaming && let Some(typed) = state.renaming_service.as_mut() {
            let field = rename_field_at(
                ui,
                typed,
                FontId::proportional(13.5),
                TEXT,
                ACCENT,
                response.rect.left_center() + vec2(26.0, 0.0),
                response.rect.max.x - 4.0,
            );
            if std::mem::take(&mut state.rename_started) {
                field.request_focus();
            }
            let (enter, escape) =
                ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
            if escape {
                state.renaming_service = None;
            } else if enter || field.lost_focus() {
                let typed = typed.clone();
                state.services.rename(&typed);
                state.renaming_service = None;
            }
            continue;
        }
        if response.clicked() {
            state.services.show(index);
            state.playlist_open = true;
        }
        if response.double_clicked() {
            state.services.show(index);
            state.renaming_service = Some(name.clone());
            state.rename_started = true;
        }
        if let Some(dropped) = response.dnd_release_payload::<SongDrag>() {
            for at in dropped.0.iter() {
                if let Some(item) = song_item(state, *at) {
                    state.services.add_to(index, item);
                }
            }
        }
        if let Some(dropped) = response.dnd_release_payload::<crate::home::MediaDrag>()
            && let Some(media) = show.media().get(dropped.0)
        {
            state.services.add_to(index, Item::Media { id: media.id, name: media.name.clone() });
        }
        response.on_hover_text("Drag songs, pictures or videos here to add them").context_menu(|ui| {
            if ui.button("Rename").clicked() {
                state.services.show(index);
                state.renaming_service = Some(name.clone());
                state.rename_started = true;
                ui.close();
            }
            if ui.button("Export…").on_hover_text("Save the playlist and its songs as a song pack").clicked() {
                state.export_request = Some(crate::home::ExportRequest::Playlist(index));
                ui.close();
            }
            if ui.add_enabled(names.len() > 1, egui::Button::new("Delete Playlist")).clicked() {
                state.services.show(index);
                delete = true;
                ui.close();
            }
        });
    }
    if delete {
        state.services.remove_service();
    }
}

/// The name of the first library a song is in, shown at the right of its row in a playlist.
fn library_of<'a>(state: &'a HomeState, key: &str) -> Option<&'a str> {
    state.libraries.iter().find(|library| library.has(key)).map(|library| library.name.as_str())
}

/// The songs the left column lists: how many the library (or All Songs) has, and those whose
/// titles have what is typed in the filter, by their place among the songs, in order.
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
    crate::kept::SongList { total, shown }
}

/// The songs of the library chosen (or every song), each with a + to put it in the playlist in
/// front; a song can also be dragged onto a playlist or a library above.
fn library_songs(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    let library = state.library_shown.and_then(|id| state.libraries.iter().position(|library| library.id == id));
    if library.is_none() {
        state.library_shown = None;
    }
    // The list is made again only when the songs, the library shown or the filter change, so
    // a library of thousands of songs doesn't slow every frame.
    let epoch = state.lists.songs.epoch();
    let songs_of =
        |at: Option<usize>| at.and_then(|at| state.libraries.get(at)).map(|library| library.songs.as_slice());
    let kept = state.lists.song_list.get(|(made_in, made_of, filter)| {
        *made_in == epoch && made_of.as_deref() == songs_of(library) && *filter == state.filter
    });
    let list = match kept {
        Some(list) => list,
        None => {
            let made = song_list(show, state, library.and_then(|at| state.libraries.get(at)));
            let made_of = songs_of(library).map(<[String]>::to_vec);
            state.lists.song_list.keep((epoch, made_of, state.filter.clone()), made)
        }
    };
    let (total, shown) = (list.total, &list.shown);
    let library = library.and_then(|at| state.libraries.get(at)).map(|library| library.name.to_uppercase());
    let heading = library.clone().unwrap_or_else(|| "ALL SONGS".to_owned());
    let live_in = show.live().map(|at| at.presentation);
    let mut add = None;
    let mut add_to: Option<(usize, Option<usize>)> = None;
    ui.horizontal(|ui| {
        caption(ui, &heading);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
            let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
            icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
            if response.on_hover_text("New song (Command-N)").clicked() {
                state.new_song = true;
            }
            let count = if shown.len() == total { total.to_string() } else { format!("{} of {total}", shown.len()) };
            ui.label(egui::RichText::new(count).size(11.0).color(TEXT_FAINT));
        });
    });
    ui.add_space(4.0);
    let list = ui.available_rect_before_wrap();
    let (pointer, mods, down) =
        ui.input(|input| (input.pointer.interact_pos(), input.modifiers, input.pointer.primary_down()));
    // Songs picked out lose the ones no longer listed (`shown` is in order).
    state.picked_songs.retain(|at| shown.binary_search(at).is_ok());
    let spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 2.0;
    // ↑ and ↓ open the song above or below in the list, as a click on it does; with Shift they
    // pick out the songs on the way, as the Finder does. Not while typing, with the song search
    // open, editing, or on the Bible page.
    const ROW: f32 = 30.0;
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
    // Only the rows in view are laid out, so thousands of songs cost no more to draw than a few.
    let mut scroll = egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]);
    // The song gone to by ↑ or ↓ is kept in sight.
    if let Some(row) = scroll_to {
        let id = ui.make_persistent_id("song list");
        let offset = egui::scroll_area::State::load(ui.ctx(), id).map_or(0.0, |state| state.clamped_offset().y);
        let (top, height) = (row as f32 * (ROW + 2.0), list.height());
        if top < offset {
            scroll = scroll.vertical_scroll_offset(top);
        } else if top + ROW > offset + height {
            scroll = scroll.vertical_scroll_offset(top + ROW - height);
        }
    }
    scroll.show_rows(ui, ROW, shown.len(), |ui, in_view| {
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
            let response = list_row(ui, Icon::Document, song.title.as_str(), chosen, live_in == Some(index), false);
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
            // A "+" at the right of the row under the pointer opens a menu of the playlists to
            // put the song in, the one in front first, so it is always said which one.
            let plus = Rect::from_center_size(response.rect.right_center() - vec2(13.0, 0.0), vec2(20.0, 20.0));
            let plus_id = ui.scope_id().with(("add to playlist", index));
            let menu_open = egui::Popup::is_id_open(ui.ctx(), plus_id.with("popup"));
            if response.hovered() || ui.rect_contains_pointer(plus) || menu_open {
                let button = ui.interact(plus, plus_id, Sense::click());
                let lit = button.hovered() || menu_open;
                let ink = if lit { Color32::WHITE } else { TEXT_DIM };
                ui.painter().rect_filled(plus, 5.0, if lit { SELECTED } else { HOVER });
                icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
                let button = button.on_hover_text("Add to a playlist");
                egui::Popup::menu(&button).show(|ui| {
                    ui.label(egui::RichText::new("Add to playlist").size(11.0).color(TEXT_FAINT));
                    let order = std::iter::once(state.services.current)
                        .chain((0..state.services.list.len()).filter(|at| *at != state.services.current));
                    for at in order {
                        let Some(name) = state.services.list.get(at).map(|p| p.name.clone()) else { continue };
                        if ui.button(name).clicked() {
                            add_to = Some((index, Some(at)));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("New Playlist…").clicked() {
                        add_to = Some((index, None));
                        ui.close();
                    }
                });
                if menu_open {
                    continue;
                }
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
            response.context_menu(|ui| song_menu(ui, show, state, commands, index, &mut add));
        }
        let ctx = ui.ctx().clone();
        sweep_rows(ui, state, crate::home::Picking::Songs, &rows, shown, list, pointer, down, |picked| {
            egui::DragAndDrop::set_payload(&ctx, SongDrag(picked));
        });
        if total == 0 {
            let empty = if library.is_some() {
                "No songs in this library yet. Drag songs onto it, or use a song's right-click menu."
            } else {
                "No songs loaded."
            };
            ui.label(egui::RichText::new(empty).size(12.0).color(TEXT_DIM));
        } else if shown.is_empty() {
            ui.label(egui::RichText::new("No song matches.").color(TEXT_DIM));
        }
    });
    ui.spacing_mut().item_spacing.y = spacing;
    if let Some(index) = add
        && let (Some(key), Some(song)) = (state.song_keys.get(index), show.presentations().get(index))
    {
        state.services.add(Item::Song { key: key.clone(), title: song.title.clone() });
    }
    // A song added from its "+" menu: to the playlist chosen, or to a new one, which is then
    // shown with its name ready to type.
    if let Some((index, playlist)) = add_to
        && let (Some(key), Some(song)) = (state.song_keys.get(index).cloned(), show.presentations().get(index))
    {
        let item = Item::Song { key, title: song.title.clone() };
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
                state.renaming_service = Some(state.services.front().name.clone());
                state.rename_started = true;
            }
        }
    }
    // Songs being carried say how many beside the pointer, until they are dropped.
    if let Some(carried) = egui::DragAndDrop::payload::<SongDrag>(ui.ctx())
        && let Some(at) = ui.ctx().pointer_latest_pos()
    {
        let words = match carried.0.as_slice() {
            [one] => show.presentations().get(*one).map_or_else(String::new, |song| song.title.clone()),
            many => format!("{} songs", many.len()),
        };
        let painter =
            ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("carried songs")));
        let text = painter.layout_no_wrap(words, semibold(12.5), Color32::WHITE);
        let chip = Rect::from_min_size(at + vec2(14.0, 10.0), text.size() + vec2(18.0, 10.0));
        painter.rect_filled(chip, 8.0, SELECTED);
        painter.rect_stroke(chip, 8.0, Stroke::new(1.0, Color32::from_white_alpha(30)), StrokeKind::Inside);
        painter.galley(chip.min + vec2(9.0, 5.0), text, Color32::WHITE);
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
            state.picked_songs.clone()
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

/// A song's right-click menu in the library.
fn song_menu(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    commands: &mut Vec<Command>,
    index: usize,
    add: &mut Option<usize>,
) {
    let front = state.services.front().name.clone();
    if ui.button(format!("Add to {front}")).clicked() {
        *add = Some(index);
        ui.close();
    }
    // The libraries the song is in.
    if let Some(key) = state.song_keys.get(index).cloned() {
        ui.menu_button("Libraries", |ui| {
            if state.libraries.is_empty() {
                ui.label(egui::RichText::new("No libraries yet. Make one with + beside LIBRARY.").color(TEXT_DIM));
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
    // The songs picked out with it, when it is one of them; else this song alone.
    if ui.button("Export…").on_hover_text("Save as a song pack, to open on another computer").clicked() {
        let songs = if state.picked_songs.contains(&index) { state.picked_songs.clone() } else { vec![index] };
        state.export_request = Some(crate::home::ExportRequest::Songs(songs));
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
    let announcing = state.announcement_live == Some(index);
    let (label, hint) = if announcing {
        ("Stop Announcements", "Take this song off the Announcements layer")
    } else {
        (
            "Play on Announcements Layer",
            "Its slides go round by themselves over the slides (each for its Go to Next time, or 8 seconds)",
        )
    };
    if ui.button(label).on_hover_text(hint).clicked() {
        state.announcement_request = Some((!announcing).then_some(index));
        ui.close();
    }
    // How its slides give way to each other, unless a slide has its own.
    let own_transition = show.presentations().get(index).and_then(|song| song.transition);
    ui.menu_button("Transition", |ui| {
        if ui
            .add(egui::Button::selectable(own_transition.is_none(), "As All Slides"))
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
    if ui.button("Reflow").clicked()
        && let Some(song) = show.presentations().get(index)
    {
        commands.push(Command::Select(index));
        state.bible_open = false;
        state.editing = None;
        state.reflow = Some((index, sp_model::to_song_text(song)));
        ui.close();
    }
    // Delete deletes every song picked out, when this is one of them.
    if ui.button("Delete…").clicked() {
        state.confirm_delete_songs =
            if state.picked_songs.contains(&index) { state.picked_songs.clone() } else { vec![index] };
        ui.close();
    }
}

/// A field over a row, for retyping its name. Returns `Some(Some(name))` when Return is pressed
/// or the field is left, and `Some(None)` on Escape.
fn rename_in_row(
    ui: &mut egui::Ui,
    typed: &mut String,
    font: FontId,
    at: egui::Pos2,
    right: f32,
    focus: bool,
) -> Option<Option<String>> {
    let field = rename_field_at(ui, typed, font, TEXT, ACCENT, at, right);
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

/// The items of the playlist in front: headers, songs (with the library each is in), Bible
/// passages, pictures and videos, media playlists and placeholders, dragged up and down to
/// change the order.
/// Whether ↑ and ↓ go through the song list or the playlist: in the Show view, with nothing
/// being typed and the song search closed.
fn list_keys_work(ui: &egui::Ui, state: &HomeState) -> bool {
    !ui.ctx().text_edit_focused()
        && state.library_search.is_none()
        && state.editing.is_none()
        && state.theme_edit.is_none()
        && !state.bible_open
}

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
    let filtering = !state.filter.trim().is_empty();
    ui.horizontal(|ui| {
        let count = items.iter().filter(|item| !matches!(item, Item::Header { .. })).count();
        let noun = if count == 1 { "ITEM" } else { "ITEMS" };
        // The playlist's name, so it is plain which one the items are of.
        caption(ui, &format!("{} \u{b7} {count} {noun}", state.services.front().name.to_uppercase()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // + adds a header (a heading that groups the items under it) or a placeholder (a
            // place kept for something to come), named as it is made.
            let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
            let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
            icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
            let response = response.on_hover_text("New Header or Placeholder");
            egui::Popup::menu(&response).show(|ui| {
                let made = if ui.button("New Header").on_hover_text("A heading over the items under it").clicked() {
                    Some(Item::Header { name: "Header".to_owned() })
                } else if ui
                    .button("New Placeholder")
                    .on_hover_text("A place kept for something to come, such as the sermon slides")
                    .clicked()
                {
                    Some(Item::Placeholder { name: "Placeholder".to_owned() })
                } else {
                    None
                };
                if let Some(item) = made {
                    let name = item.title().to_owned();
                    state.services.add(item);
                    let at = state.services.front().items.len() - 1;
                    state.renaming_header = Some((at, name));
                    state.rename_started = true;
                    ui.close();
                }
            });
        });
    });
    ui.add_space(4.0);

    // What a click on a row asks for, carried out after the list is drawn.
    enum Ask {
        Open(usize),
        Earlier(usize),
        Later(usize),
        Remove(usize),
        Rename(usize),
        /// A placeholder filled with a song, by its place among the songs.
        Fill(usize, usize),
    }
    let mut ask = None;
    // Rows can be dragged up and down to change the order (not while the list is filtered).
    let mut rows: Vec<Rect> = Vec::new();
    // The item each row is, in step with `rows`.
    let mut row_items: Vec<usize> = Vec::new();
    let mut dragging = None;
    let mut dropped = None;
    let mut renamed: Option<Option<(usize, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let list = egui::ScrollArea::vertical().id_salt("playlist items").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        if items.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Nothing yet. Drag songs onto this playlist, use + on a song in a library, Add to on \
                     the Bible page, or drag a picture or video here from the media bin.",
                )
                .size(12.0)
                .color(TEXT_FAINT),
            );
        }
        for (index, item) in items.iter().enumerate() {
            if filtering && !matches_filter(item.title(), &state.filter) {
                continue;
            }
            // A header: bold, full width, and renamed in place.
            if let Item::Header { name } = item {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click_and_drag());
                rows.push(row);
                row_items.push(index);
                match state.renaming_header.as_mut().filter(|(at, _)| *at == index) {
                    Some((_, typed)) => {
                        let at = row.left_center() + vec2(6.0, 0.0);
                        let focus = std::mem::take(&mut rename_started);
                        if let Some(done) = rename_in_row(ui, typed, semibold(13.0), at, row.max.x - 4.0, focus) {
                            renamed = Some(done.map(|name| (index, name)));
                        }
                    }
                    None => {
                        ui.painter().text(
                            row.left_center() + vec2(6.0, 0.0),
                            Align2::LEFT_CENTER,
                            name.as_str(),
                            semibold(13.0),
                            TEXT,
                        );
                        ui.painter().hline(row.x_range(), row.max.y - 1.0, Stroke::new(1.0, LINE));
                    }
                }
                if response.dragged() && !filtering {
                    dragging = Some(index);
                }
                if response.drag_stopped() && !filtering {
                    dropped = Some(index);
                }
                if response.double_clicked() {
                    ask = Some(Ask::Rename(index));
                }
                response.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        ask = Some(Ask::Rename(index));
                        ui.close();
                    }
                    if ui.button("Remove from Playlist").clicked() {
                        ask = Some(Ask::Remove(index));
                        ui.close();
                    }
                });
                continue;
            }
            // Each kind's mark; whether what it names is still here, and on screen now; and a
            // word at the right (the library a song is in, or what kind of item it is).
            let (icon, missing, live, beside, gone) = match item {
                Item::Song { .. } => {
                    let (found, library) = looked_up.get(index).cloned().unwrap_or_default();
                    let live = found.is_some() && found == live_in;
                    (Icon::Document, found.is_none(), live, library, "This song is not open in the library")
                }
                Item::Passage(_) => (Icon::Book, false, false, None, ""),
                Item::Media { id, .. } => {
                    let found = show.media().iter().position(|media| media.id == *id);
                    let video = found.is_some_and(|at| crate::media::is_video(&show.media()[at].path));
                    let (icon, word) = if video { (Icon::Camera, "video") } else { (Icon::Picture, "picture") };
                    let live = found.is_some() && found == show.live_media();
                    let gone = "This picture or video is no longer in the media bin";
                    (icon, found.is_none(), live, Some(word.to_owned()), gone)
                }
                Item::MediaPlaylist { id, .. } => {
                    let found = state.media_playlists.iter().any(|playlist| playlist.id == *id);
                    let live = state.media_playing == Some(*id);
                    let gone = "This media playlist has been deleted";
                    (Icon::Loop, !found, live, Some("media playlist".to_owned()), gone)
                }
                // Not missing, but faint: a place kept for something to come.
                Item::Placeholder { .. } => (Icon::Target, true, false, Some("placeholder".to_owned()), ""),
                Item::Header { .. } => continue,
            };
            let chosen = state.services.chosen == Some(index);
            let mut response = list_row(ui, icon, item.title(), chosen, live, missing);
            // A placeholder is renamed in place, as a header is.
            if let Some((_, typed)) = state
                .renaming_header
                .as_mut()
                .filter(|(at, _)| *at == index && matches!(item, Item::Placeholder { .. }))
            {
                let row = response.rect;
                ui.painter().rect_filled(Rect::from_min_max(row.min + vec2(26.0, 0.0), row.max), RADIUS, PANEL);
                let focus = std::mem::take(&mut rename_started);
                let at = row.left_center() + vec2(30.0, 0.0);
                if let Some(done) = rename_in_row(ui, typed, FontId::proportional(13.5), at, row.max.x - 4.0, focus) {
                    renamed = Some(done.map(|name| (index, name)));
                }
            }
            // The library the song is in, or the kind of item, at the right.
            if let Some(library) = beside.filter(|_| !live) {
                ui.painter().text(
                    response.rect.right_center() - vec2(8.0, 0.0),
                    Align2::RIGHT_CENTER,
                    library,
                    FontId::proportional(11.0),
                    TEXT_FAINT,
                );
            }
            rows.push(response.rect);
            row_items.push(index);
            if response.dragged() && !filtering {
                dragging = Some(index);
            }
            if response.drag_stopped() && !filtering {
                dropped = Some(index);
            }
            if missing && !gone.is_empty() {
                response = response.on_hover_text(gone);
            }
            if matches!(item, Item::Placeholder { .. }) {
                response = response.on_hover_text(
                    "A place kept for something to come: right-click to fill it with a song, or drag a \
                     picture or video onto it",
                );
            }
            if response.double_clicked() && matches!(item, Item::Placeholder { .. }) {
                ask = Some(Ask::Rename(index));
            }
            if response.clicked() {
                ask = Some(Ask::Open(index));
            }
            // Gone to with ↑ or ↓: kept in sight.
            if state.playlist_scroll_to == Some(index) {
                response.scroll_to_me(None);
                state.playlist_scroll_to = None;
            }
            response.context_menu(|ui| {
                if matches!(item, Item::Placeholder { .. }) {
                    ui.menu_button("Fill with Song", |ui| {
                        if let Some(song) = song_picker(ui, show, state) {
                            ask = Some(Ask::Fill(index, song));
                            ui.close();
                        }
                    });
                    if ui.button("Rename").clicked() {
                        ask = Some(Ask::Rename(index));
                        ui.close();
                    }
                    ui.separator();
                }
                if ui.add_enabled(index > 0, egui::Button::new("Move Up")).clicked() {
                    ask = Some(Ask::Earlier(index));
                    ui.close();
                }
                if ui.add_enabled(index + 1 < items.len(), egui::Button::new("Move Down")).clicked() {
                    ask = Some(Ask::Later(index));
                    ui.close();
                }
                ui.separator();
                if ui.button("Remove from Playlist").clicked() {
                    ask = Some(Ask::Remove(index));
                    ui.close();
                }
            });
        }
    });
    // Where a dragged row would land: a line between rows, and on release the move.
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let pointer_y = pointer.map(|at| at.y);
    let slot = pointer_y.map(|y| rows.iter().filter(|row| row.center().y < y).count());
    // A picture or video carried from the media bin: dropped on a placeholder it takes its
    // place; dropped between rows it goes there.
    if let Some(carried) = egui::DragAndDrop::payload::<crate::home::MediaDrag>(ui.ctx())
        && let Some(at) = pointer.filter(|at| list.inner_rect.contains(*at) && !filtering)
        && let Some(media) = show.media().get(carried.0)
    {
        let onto = rows
            .iter()
            .zip(&row_items)
            .find(|(row, item)| row.contains(at) && matches!(items.get(**item), Some(Item::Placeholder { .. })));
        let slot = rows.iter().filter(|row| row.center().y < at.y).count();
        let line = |rows: &[Rect]| {
            rows.get(slot)
                .map_or_else(|| rows.last().map_or(list.inner_rect.min.y + 4.0, |row| row.max.y), |row| row.min.y)
        };
        match onto {
            Some((row, _)) => {
                ui.painter().rect_stroke(*row, RADIUS, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
            }
            None => {
                let (left, right) = (list.inner_rect.min.x, list.inner_rect.max.x);
                let y = line(&rows);
                ui.painter().line_segment([pos2(left + 4.0, y), pos2(right - 4.0, y)], Stroke::new(2.0, ACCENT));
            }
        }
        if ui.input(|input| input.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            let item = Item::Media { id: media.id, name: media.name.clone() };
            match onto {
                Some((_, place)) => {
                    state.services.fill(*place, item);
                }
                None => {
                    // `slot` counts rows shown, which are every item while nothing filters.
                    let place = row_items.get(slot).copied().unwrap_or(items.len());
                    state.services.insert(place, item);
                }
            }
        }
    }
    if let (Some(_), Some(slot)) = (dragging, slot) {
        let y = rows.get(slot).map_or_else(|| rows.last().map_or(0.0, |row| row.max.y), |row| row.min.y);
        let (left, right) = rows.first().map_or((0.0, 0.0), |row| (row.min.x, row.max.x));
        ui.painter().line_segment([pos2(left + 4.0, y), pos2(right - 4.0, y)], Stroke::new(2.0, ACCENT));
    }
    if let (Some(index), Some(slot)) = (dropped, slot) {
        state.services.move_item(index, slot);
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
        let (up, down) =
            ui.input(|input| (input.key_pressed(egui::Key::ArrowUp), input.key_pressed(egui::Key::ArrowDown)));
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
            if let Some(next) = next {
                ask = Some(Ask::Open(openable[next]));
                state.playlist_scroll_to = Some(openable[next]);
            }
        }
    }
    match ask {
        Some(Ask::Rename(index)) => {
            state.renaming_header =
                Some((index, items.get(index).map(|item| item.title().to_owned()).unwrap_or_default()));
            state.rename_started = true;
        }
        Some(Ask::Open(index)) => {
            state.services.chosen = Some(index);
            state.picking = crate::home::Picking::Nothing;
            // The slide area scrolls to it, even if it was the one last scrolled to.
            state.slides_scrolled_to = None;
            state.slides_picked_here = false;
            match items.get(index) {
                Some(Item::Song { key, .. }) => {
                    if let Some(at) = song_at(&state.song_keys, key) {
                        commands.push(Command::Select(at));
                        state.bible_open = false;
                    }
                }
                Some(Item::Passage(saved)) => {
                    state.bible_open = true;
                    state.bible.open_saved(saved, live_in);
                }
                // Shown in the slide area with the rest of the playlist, as a tile to click to
                // put it on the screens (a placeholder shows nothing). Opening changes nothing
                // on the screens.
                Some(Item::Media { .. } | Item::MediaPlaylist { .. } | Item::Placeholder { .. }) => {
                    state.bible_open = false;
                }
                Some(Item::Header { .. }) | None => {}
            }
        }
        Some(Ask::Fill(index, song)) => {
            if let (Some(key), Some(found)) = (state.song_keys.get(song), show.presentations().get(song)) {
                let item = Item::Song { key: key.clone(), title: found.title.clone() };
                if state.services.fill(index, item) {
                    commands.push(Command::Select(song));
                    state.bible_open = false;
                }
            }
        }
        Some(Ask::Earlier(index)) => {
            state.services.nudge(index, true);
        }
        Some(Ask::Later(index)) => {
            state.services.nudge(index, false);
        }
        Some(Ask::Remove(index)) => {
            state.services.remove(index);
        }
        None => {}
    }
}

/// The right column: what the audience sees now, the buttons that take it away, stepping
/// through the slides, and what comes next.
pub fn program_panel(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    preview: crate::home::Preview,
    commands: &mut Vec<Command>,
) {
    let output_stopped = state.output_stopped;
    let frame = column_frame(Column::Right).inner_margin(egui::Margin { left: 16, right: 16, top: 14, bottom: 12 });
    let panel = egui::Panel::right("live")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(column_outer(RIGHT_WIDTH))
        .frame(frame);
    let shown = panel.show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        let slide_live = show.live().is_some();
        let media_live = show.live_media().is_some();
        let any_live = show.anything_live();
        let on = show.output_enabled();
        let width = ui.available_width();

        // The heading carries the tally light: red while anything is on screen.
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.5, if any_live && on { TALLY } else { TEXT_FAINT });
            caption(ui, "PROGRAM");
            // Which screens the preview shows: the audience's, or the stage's.
            let shown = if state.preview_stage { "STAGE ▾" } else { "AUDIENCE ▾" };
            let chooser = ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(shown).font(semibold(10.5)).color(TEXT_DIM).extra_letter_spacing(0.8),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_text("Choose which screens the preview shows");
            egui::Popup::menu(&chooser).show(|ui| {
                for (stage, name) in [(false, "Audience"), (true, "Stage")] {
                    if ui.add(egui::Button::selectable(state.preview_stage == stage, name)).clicked() {
                        state.preview_stage = stage;
                        ui.close();
                    }
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (text, ink) = match (on, any_live) {
                    (false, _) => ("OUTPUT OFF", TALLY),
                    (true, true) => ("ON SCREEN", TALLY),
                    (true, false) => ("CLEAR", TEXT_FAINT),
                };
                ui.label(egui::RichText::new(text).font(semibold(10.5)).color(ink).extra_letter_spacing(0.8));
            });
        });

        let (picture, _) = ui.allocate_exact_size(vec2(width, (width * 9.0 / 16.0).round()), Sense::hover());
        ui.painter().rect_filled(picture, RADIUS, Color32::BLACK);
        // While the output fades from one picture to the next, so does this.
        let fading = preview.amount < 1.0;
        if let Some(before) = preview.before.filter(|_| fading) {
            egui::Image::from_texture((before, picture.size())).corner_radius(RADIUS).paint_at(ui, picture);
        }
        if let Some(texture) = preview.now {
            let veil =
                if fading { Color32::WHITE.gamma_multiply(preview.amount.clamp(0.0, 1.0)) } else { Color32::WHITE };
            egui::Image::from_texture((texture, picture.size())).corner_radius(RADIUS).tint(veil).paint_at(ui, picture);
        }
        if fading {
            ui.ctx().request_repaint();
        }
        if !on && !state.preview_stage {
            ui.painter().rect_filled(picture, RADIUS, Color32::from_black_alpha(190));
            ui.painter().text(
                picture.center(),
                Align2::CENTER_CENTER,
                "Audience output is off",
                FontId::proportional(13.5),
                TEXT_DIM,
            );
        }
        let border = if any_live && on { Stroke::new(2.0, TALLY) } else { Stroke::new(1.0, LINE) };
        ui.painter().rect_stroke(picture, RADIUS, border, StrokeKind::Outside);
        // A screen that could not draw what is live shows the picture before it, whatever the
        // preview above shows: the operator must know.
        if let Some(undrawn) = preview.undrawn.filter(|_| on) {
            Frame::new().fill(TALLY.gamma_multiply(0.22)).corner_radius(RADIUS).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(undrawn.in_words()).size(12.5).color(Color32::WHITE));
            });
        }
        if output_stopped {
            ui.label(
                egui::RichText::new("The output has stopped. Restart SoftPresenter to bring it back.").color(TALLY),
            );
        }
        if let Some(video) = state.video_live.clone() {
            let compact = ui.available_height() < ROOMY_COLUMN;
            video_controls(ui, state, &video, width, compact);
        }
        ui.add_space(2.0);

        // Clear: the big one takes everything off; the two under it take one layer each.
        let clear_all = if any_live { Emphasis::Live } else { Emphasis::Idle };
        if block_button(ui, vec2(width, 40.0), "Clear All", "esc", clear_all)
            .on_hover_text("Clear All (Escape)")
            .clicked()
        {
            commands.push(Command::ClearAll);
        }
        ui.horizontal(|ui| {
            let half = (width - 8.0) / 2.0;
            let emphasis = |live: bool| if live { Emphasis::Plain } else { Emphasis::Idle };
            if block_button(ui, vec2(half, 30.0), "Clear Slide", "", emphasis(slide_live))
                .on_hover_text("Take the words off the screen")
                .clicked()
            {
                commands.push(Command::Clear(Layer::Slides));
            }
            if block_button(ui, vec2(half, 30.0), "Clear Media", "", emphasis(media_live))
                .on_hover_text("Take the picture or video off the screen")
                .clicked()
            {
                commands.push(Command::Clear(Layer::Media));
            }
        });
        // The other layers, each on its own.
        ui.horizontal(|ui| {
            let third = (width - 16.0) / 3.0;
            let emphasis = |live: bool| if live { Emphasis::Plain } else { Emphasis::Idle };
            let layers = [
                ("Props", "Clear Props: take the props off (F4)", !state.props_live.is_empty(), Layer::Props),
                (
                    "Messages",
                    "Clear Messages: take the message off (F6)",
                    state.message_live.is_some(),
                    Layer::Messages,
                ),
                ("Audio", "Clear Audio: stop the audio (F5)", state.audio_playing.track.is_some(), Layer::Audio),
            ];
            for (name, hint, live, layer) in layers {
                if block_button(ui, vec2(third, 26.0), name, "", emphasis(live)).on_hover_text(hint).clicked() {
                    commands.push(Command::Clear(layer));
                }
            }
        });
        // While a song plays on the Announcements layer, a button to take it off.
        if state.announcement_live.is_some()
            && block_button(ui, vec2(width, 26.0), "Clear Announcements", "", Emphasis::Plain)
                .on_hover_text("Take the announcements off the screens")
                .clicked()
        {
            commands.push(Command::Clear(Layer::Announcements));
        }
        ui.add_space(6.0);

        // Stepping through the song, with where we are between the buttons.
        let at = show.live().or(show.cursor());
        let song = at.and_then(|at| show.presentations().get(at.presentation));
        ui.horizontal(|ui| {
            let side = ((width - 16.0 - 64.0) / 2.0).floor();
            if block_button(ui, vec2(side, 36.0), "Previous", "", Emphasis::Plain)
                .on_hover_text("Show the slide before (Left arrow)")
                .clicked()
            {
                commands.push(Command::Previous);
            }
            let (middle, _) = ui.allocate_exact_size(vec2(64.0, 36.0), Sense::hover());
            // The place in the song's play order, which an arrangement can make longer.
            let place = match (show.position(), at, song) {
                (Some((step, length)), _, _) => format!("{} / {length}", step + 1),
                (None, Some(at), Some(song)) => format!("{} / {}", at.slide + 1, song.slide_count()),
                _ => "–".to_owned(),
            };
            ui.painter().text(middle.center(), Align2::CENTER_CENTER, place, FontId::monospace(12.5), TEXT_DIM);
            if block_button(ui, vec2(side, 36.0), "Next", "", Emphasis::Plain)
                .on_hover_text("Show the next slide (Right arrow or Space)")
                .clicked()
            {
                commands.push(Command::Next);
            }
        });
        ui.add_space(6.0);

        // Timers, messages and the stage, in tabs under the rest.
        crate::live_tabs::live_tabs(ui, show, state);
    });
    round_corners(ui, column_panel(shown.response.rect, Column::Right));
}

/// The strip under the slides that holds pictures to show behind the words. It is slim until
/// it has something in it.
/// The room the columns leave under what they hold, at the bottom of the window.
const COLUMN_BOTTOM: f32 = 8.0;
/// How tall the open media bin is: its top margin, its bar, a picture and its name (whose
/// letters reach 2 points below their row), and the room under them.
const MEDIA_STRIP_HEIGHT: f32 = 10.0 + 24.0 + 6.0 + 4.0 + 3.0 + 81.0 + 18.0 + 2.0 + COLUMN_BOTTOM;

pub fn media_strip(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    // Folded down (with its chevron, or from the View menu) to a slim bar that opens it again;
    // opened by itself while there is something to say or a file is being dropped.
    let folded = state.media_hidden && state.notice.is_none() && !state.drop_hover;
    let empty = show.media().is_empty();
    // Open, it is as tall as its contents (the bar, the pictures and their names) with the same
    // room under them as the left column leaves under its filter, so the three columns end on
    // one line.
    let height = if folded { 30.0 } else { MEDIA_STRIP_HEIGHT } + GAP;
    let margin = if folded {
        egui::Margin::symmetric(16, 4)
    } else {
        egui::Margin { left: 16, right: 16, top: 10, bottom: COLUMN_BOTTOM as i8 }
    };
    // The foot of the middle column: its lower corners are the column's.
    let corner = RADIUS_PANEL as u8;
    let frame = Frame::new()
        .fill(PANEL)
        .corner_radius(CornerRadius { nw: 0, ne: 0, sw: corner, se: corner })
        .inner_margin(margin)
        .outer_margin(column_margin(Column::Middle));
    egui::Panel::bottom("media bin").resizable(false).exact_size(height).frame(frame).show(ui, |ui| {
        let area = ui.max_rect().expand2(vec2(16.0, 10.0));
        if folded {
            ui.horizontal(|ui| {
                let (place, response) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
                let ink = if response.hovered() { TEXT } else { TEXT_DIM };
                icons::paint(
                    ui.painter(),
                    Icon::ChevronUp,
                    Rect::from_center_size(place.left_center() + vec2(7.0, 0.0), vec2(11.0, 11.0)),
                    ink,
                );
                let count = show.media().len();
                let words = match count {
                    0 => "MEDIA".to_owned(),
                    1 => "MEDIA  ·  1 item".to_owned(),
                    n => format!("MEDIA  ·  {n} items"),
                };
                ui.painter().text(
                    place.left_center() + vec2(20.0, 0.0),
                    Align2::LEFT_CENTER,
                    words,
                    semibold(11.5),
                    ink,
                );
                if response.on_hover_text("Show the media bin").clicked() {
                    state.media_hidden = false;
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            // Folds the bin down to a slim bar.
            let (fold, response) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::click());
            let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
            icons::paint(ui.painter(), Icon::ChevronDown, Rect::from_center_size(fold.center(), vec2(11.0, 11.0)), ink);
            if response.on_hover_text("Hide the media bin").clicked() {
                state.media_hidden = true;
            }
            caption(ui, "MEDIA");
            media_playlist_picker(ui, state);
            let note = match &state.notice {
                Some(notice) => egui::RichText::new(notice.as_str()).size(12.0).color(Color32::from_rgb(255, 170, 120)),
                // The empty bin says it itself, below.
                None if empty => egui::RichText::new("").size(12.0).color(TEXT_FAINT),
                None => egui::RichText::new("Click a picture or video to put it behind the words")
                    .size(12.0)
                    .color(TEXT_FAINT),
            };
            ui.add_space(6.0);
            // Cut off before the controls at the right, rather than run under them; the whole of
            // it shows on hover.
            let full = note.text().to_owned();
            ui.scope(|ui| {
                ui.set_max_width((ui.available_width() - 150.0).max(0.0));
                ui.add(egui::Label::new(note).truncate()).on_hover_text(full);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("+").on_hover_text("Add pictures and videos").clicked() {
                    state.media_request = Some(crate::home::MediaRequest::Choose);
                }
                // The media bin's own transition: how one picture or video gives way to the next.
                let label = ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!("{} ▾", state.media_transition.label()))
                                .size(12.0)
                                .color(TEXT_DIM),
                        )
                        .frame(false),
                    )
                    .on_hover_text("How one picture or video gives way to the next");
                egui::Popup::from_toggle_button_response(&label)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| transition_picker(ui, &mut state.media_transition, "media"));
            });
        });
        // Nothing in it yet: a big place to click, or to drop files on.
        if empty && !state.drop_hover {
            ui.add_space(6.0);
            // A little short of the bottom, so the dashed edge is never cut off.
            let room = ui.available_size() - vec2(0.0, 4.0);
            let (place, response) = ui.allocate_exact_size(room, Sense::click());
            let hot = response.hovered();
            let painter = ui.painter();
            painter.rect_filled(place, 10.0, if hot { CARD } else { PANEL });
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
            ui.add_space(4.0);
            let image_size = vec2(144.0, 81.0);
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
            // A media playlist shows only its own files.
            let shown = state
                .media_playlist_shown
                .and_then(|id| state.media_playlists.iter().find(|playlist| playlist.id == id))
                .cloned();
            let order: Vec<usize> = show
                .media()
                .iter()
                .enumerate()
                .filter(|(_, item)| shown.as_ref().is_none_or(|playlist| playlist.has(&item.path)))
                .map(|(index, _)| index)
                .collect();
            state.picked_media.retain(|at| order.contains(at));
            let scroll = egui::scroll_area::ScrollSource::MOUSE_WHEEL | egui::scroll_area::ScrollSource::SCROLL_BAR;
            egui::ScrollArea::horizontal().id_salt("media row").scroll_source(scroll).auto_shrink([false, false]).show(
                ui,
                |ui| {
                    let mut placed: Vec<(usize, Rect)> = Vec::new();
                    // Room round the row, so the outline of an item on screen or picked out is
                    // not cut off at the edges.
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.add_space(3.0);
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let several = state.picked_media.len() > 1;
                        for (index, item) in show.media().iter().enumerate() {
                            if !order.contains(&index) {
                                continue;
                            }
                            let (rect, response) =
                                ui.allocate_exact_size(image_size + vec2(0.0, 18.0), Sense::click_and_drag());
                            placed.push((index, rect));
                            // A drag starting on an item picks out the items it passes over while it
                            // stays in the bin; dragged out onto a slide it becomes what the slide
                            // shows, onto the space under the slides a slide of its own.
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
                            ui.painter().rect_filled(image, 6.0, Color32::BLACK);
                            match thumb(ThumbOf::Media(index)) {
                                Some((texture, _)) => {
                                    egui::Image::from_texture((texture, image_size))
                                        .corner_radius(6.0)
                                        .paint_at(ui, image);
                                }
                                None => ui.ctx().request_repaint(),
                            }
                            // A video says so in its corner.
                            if crate::media::is_video(&item.path) {
                                let badge =
                                    Rect::from_min_size(image.left_bottom() + vec2(5.0, -19.0), vec2(22.0, 14.0));
                                ui.painter().rect_filled(badge, 3.0, Color32::from_black_alpha(170));
                                ui.painter().text(
                                    badge.center(),
                                    Align2::CENTER_CENTER,
                                    "▶",
                                    FontId::proportional(9.0),
                                    Color32::WHITE,
                                );
                            }
                            let live = show.live_media() == Some(index);
                            let picked = state.picked_media.contains(&index);
                            if live {
                                ui.painter().rect_stroke(image, 6.0, Stroke::new(2.0, TALLY), StrokeKind::Outside);
                            } else if picked {
                                ui.painter().rect_stroke(image, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
                            } else if response.hovered() {
                                ui.painter().rect_stroke(image, 6.0, Stroke::new(1.0, TEXT_FAINT), StrokeKind::Outside);
                            }
                            // Picked out and on screen: a light ring inside the red one.
                            if live && picked {
                                ui.painter().rect_stroke(image, 6.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
                            }
                            let label = Rect::from_min_max(pos2(rect.min.x, image.max.y + 3.0), rect.max);
                            ui.painter().with_clip_rect(label).text(
                                label.left_center(),
                                Align2::LEFT_CENTER,
                                item.name.as_str(),
                                FontId::proportional(11.5),
                                TEXT_DIM,
                            );
                            response.on_hover_text(item.name.as_str()).context_menu(|ui| {
                                media_item_menu(ui, state, index, item);
                            });
                        }
                    });
                    sweep_media(ui, state, &placed, bin, pointer, down);
                },
            );
        }
        if state.drop_hover {
            let painter = ui.painter();
            painter.rect_filled(area, 0.0, ACCENT.gamma_multiply(0.18));
            painter.rect_stroke(area.shrink(4.0), RADIUS, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
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
/// dropped on a slide.
fn sweep_media(
    ui: &mut egui::Ui,
    state: &mut HomeState,
    placed: &[(usize, Rect)],
    bin: Rect,
    pointer: Option<egui::Pos2>,
    down: bool,
) {
    let Some(mut sweep) = state.sweep.filter(|sweep| sweep.list == crate::home::Picking::Media) else { return };
    if !down {
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
    if sweep.keeps || at.distance(sweep.start) < 3.0 {
        return;
    }
    let band = Rect::from_two_pos(sweep.start, at);
    state.picked_media = placed.iter().filter(|(_, rect)| rect.intersects(band)).map(|(index, _)| *index).collect();
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("media band")));
    painter.rect_filled(band, 2.0, ACCENT.gamma_multiply(0.12));
    painter.rect_stroke(band, 2.0, Stroke::new(1.0, ACCENT.gamma_multiply(0.8)), StrokeKind::Inside);
    let edge = 30.0;
    if at.x < bin.min.x + edge {
        ui.scroll_with_delta(vec2(10.0, 0.0));
        ui.ctx().request_repaint();
    } else if at.x > bin.max.x - edge {
        ui.scroll_with_delta(vec2(-10.0, 0.0));
        ui.ctx().request_repaint();
    }
}

/// The right end of a slide area's heading: the three ways to lay slides out, and their size.
pub fn view_controls(ui: &mut egui::Ui, state: &mut HomeState) {
    ui.spacing_mut().item_spacing.x = 8.0;
    // Further right is bigger, so fewer slides fit across.
    let mut size = 7 - state.columns.clamp(2, 7);
    ui.spacing_mut().slider_width = 96.0;
    if ui.add(egui::Slider::new(&mut size, 0..=5).show_value(false)).on_hover_text("Slide size").changed() {
        state.columns = 7 - size;
    }
    ui.add_space(4.0);
    let views = [
        (SlideView::Table, Icon::Rows, "Table View"),
        (SlideView::Easy, Icon::Document, "Easy View: words only"),
        (SlideView::Grid, Icon::Grid, "Grid View"),
    ];
    let (bar, _) = ui.allocate_exact_size(vec2(30.0 * 3.0 + 6.0, 30.0), Sense::hover());
    ui.painter().rect_filled(bar, RADIUS + 1.0, PANEL);
    // Laid out right to left, so the first here is the rightmost.
    for (index, (view, icon, hint)) in views.into_iter().enumerate() {
        let cell =
            Rect::from_min_size(pos2(bar.max.x - 3.0 - 30.0 * (index as f32 + 1.0), bar.min.y + 3.0), vec2(30.0, 24.0));
        let response = ui.interact(cell, ui.scope_id().with(("slide view", index)), Sense::click());
        let on = state.slide_view == view;
        if on {
            ui.painter().rect_filled(cell, RADIUS - 2.0, CARD);
        }
        let ink = if on || response.hovered() { TEXT } else { TEXT_FAINT };
        icons::paint(ui.painter(), icon, Rect::from_center_size(cell.center(), vec2(14.0, 14.0)), ink);
        if response.on_hover_text(hint).clicked() {
            state.slide_view = view;
        }
    }
    // How one slide gives way to the next on the screens: Cut, Dissolve, or another chosen
    // from the picker, which the label beside the buttons names ("Push: 0.6s").
    use sp_model::TransitionKind as K;
    let kind = state.transition.kind;
    // Without room beside what is to their left (a narrow window), the transition buttons are
    // left out rather than drawn over it. They need 216 points, and the label more.
    let needed = if kind == K::Cut { 216.0 } else { 260.0 };
    if ui.available_width() < needed {
        // A narrow window: one button naming the transition, which opens the picker, rather
        // than the transition being left out.
        if ui.available_width() >= 110.0 {
            ui.add_space(6.0);
            let words = if kind == K::Cut {
                "Cut ▾".to_owned()
            } else {
                format!("{} {:.1}s ▾", kind.name(), state.transition.seconds)
            };
            let button = ui
                .add(egui::Button::new(egui::RichText::new(words).size(12.0).color(TEXT_DIM)))
                .on_hover_text("How one slide gives way to the next");
            egui::Popup::from_toggle_button_response(&button)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| transition_picker(ui, &mut state.transition, "slides"));
        }
        return;
    }
    ui.add_space(10.0);
    if kind != K::Cut {
        // The time only: the transition's name is the button chosen beside it.
        let label = ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("{:.1}s ▾", state.transition.seconds)).size(12.0).color(TEXT_DIM),
                )
                .frame(false),
            )
            .on_hover_text("How long it takes, and its direction");
        egui::Popup::from_toggle_button_response(&label)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| transition_settings(ui, &mut state.transition, "slides"));
    }
    // Laid out right to left: the last cell here is the leftmost.
    // The third button is Advanced; once one of its transitions is chosen, it shows that name.
    let others =
        if matches!(kind, K::Cut | K::Dissolve) { "Advanced ▾".to_owned() } else { format!("{} ▾", kind.name()) };
    let (bar, _) = ui.allocate_exact_size(vec2(44.0 + 64.0 + 84.0 + 6.0, 30.0), Sense::hover());
    ui.painter().rect_filled(bar, RADIUS + 1.0, PANEL);
    let mut more = None;
    for (index, label, width, x, hint) in [
        (0, "Cut".to_owned(), 44.0, 3.0, "Slides switch at once"),
        (1, "Dissolve".to_owned(), 64.0, 47.0, "Each slide fades into the next"),
        (2, others, 84.0, 111.0, "Choose another transition"),
    ] {
        let cell = Rect::from_min_size(bar.min + vec2(x, 3.0), vec2(width, 24.0));
        let response = ui.interact(cell, ui.scope_id().with(("transition", index)), Sense::click());
        let on = match index {
            0 => kind == K::Cut,
            1 => kind == K::Dissolve,
            _ => !matches!(kind, K::Cut | K::Dissolve),
        };
        if on {
            ui.painter().rect_filled(cell, RADIUS - 2.0, CARD);
        }
        let ink = if on || response.hovered() { TEXT } else { TEXT_FAINT };
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, label.as_str(), FontId::proportional(12.0), ink);
        let response = response.on_hover_text(hint);
        match index {
            0 if response.clicked() => state.transition = sp_model::Transition::of(K::Cut),
            1 if response.clicked() && kind != K::Dissolve => state.transition = sp_model::Transition::of(K::Dissolve),
            2 => more = Some(response),
            _ => {}
        }
    }
    if let Some(more) = more {
        egui::Popup::from_toggle_button_response(&more)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| transition_picker(ui, &mut state.transition, "slides"));
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

/// The colour of a choice that is on (the video loops).
const ON: Color32 = Color32::from_rgb(52, 199, 89);

/// How far Forward goes in the video on screen, in seconds.
const VIDEO_STEP: f64 = 10.0;

/// How tall the right column's room under the preview must be for the video's controls to take
/// their full size; in less, they take two rows, leaving the tabs under them room.
const ROOMY_COLUMN: f32 = 650.0;

/// The video on screen, as a media player shows it: its name; a slim bar to click or drag
/// through, the playhead showing under the pointer; the time gone and the time left; and the
/// controls in one group about Play or Pause in the middle: Loop, Start Over, Play or Pause,
/// Forward ten seconds, and Sound. `compact` (a short window) puts the same in two rows: the
/// name with the times, then Play or Pause and Start Over, the bar, and Forward, Loop and Sound.
fn video_controls(ui: &mut egui::Ui, state: &mut HomeState, video: &crate::home::VideoLive, width: f32, compact: bool) {
    use crate::home::VideoRequest;
    let clock = |seconds: f64| crate::audio::clock(std::time::Duration::from_secs_f64(seconds.max(0.0)));
    let length = video.duration;
    let left = |shown: f64| length.map_or_else(|| "–".to_owned(), |length| format!("−{}", clock(length - shown)));
    // How big the buttons and their icons are.
    let scale = if compact { 0.75 } else { 1.0 };
    // A round button that is only its icon: a patch under the pointer.
    let round = |ui: &egui::Ui, at: egui::Pos2, id: &str, hint: &str| {
        let radius = 17.0 * scale;
        let cell = Rect::from_center_size(at, egui::Vec2::splat(radius * 2.0));
        let response = ui.interact(cell, ui.scope_id().with(id), Sense::click());
        if response.hovered() {
            ui.painter().circle_filled(at, radius, Color32::from_white_alpha(18));
        }
        response.on_hover_text(hint)
    };
    let margin = if compact {
        egui::Margin { left: 10, right: 10, top: 8, bottom: 6 }
    } else {
        egui::Margin { left: 14, right: 14, top: 10, bottom: 8 }
    };
    egui::Frame::new().fill(CARD).corner_radius(10).inner_margin(margin).show(ui, |ui| {
        ui.set_width(width - f32::from(margin.left + margin.right));
        ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
        // The name, with a small mark that it is a video; short, the times at its right.
        ui.horizontal(|ui| {
            let (mark, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            icons::paint(ui.painter(), Icon::Camera, mark, TEXT_DIM);
            let name =
                egui::Label::new(egui::RichText::new(video.name.as_str()).font(semibold(12.5)).color(TEXT)).truncate();
            if compact {
                let times = format!("{} · {}", clock(video.position), left(video.position));
                let font = FontId::proportional(11.0);
                let wide = ui.painter().layout_no_wrap(times.clone(), font.clone(), TEXT_DIM).size().x;
                let room = (ui.available_width() - wide - 8.0).max(0.0);
                ui.allocate_ui(vec2(room, 16.0), |ui| ui.add(name));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(egui::RichText::new(times).font(font).color(TEXT_DIM));
                });
            } else {
                ui.add(name);
            }
        });
        // Where the bar, the times and the buttons go.
        let (bar, times, row) = if compact {
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            let bar =
                Rect::from_x_y_ranges(row.min.x + 64.0..=row.max.x - 88.0, row.center().y - 8.0..=row.center().y + 8.0);
            (bar, None, row)
        } else {
            ui.add_space(6.0);
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
            let (times, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
            (bar, Some(times), row)
        };
        let shown = video_bar(ui, state, video, bar);
        // The time gone, and the time left.
        if let Some(times) = times {
            let font = FontId::proportional(11.0);
            ui.painter().text(times.left_center(), Align2::LEFT_CENTER, clock(shown), font.clone(), TEXT_DIM);
            ui.painter().text(times.right_center(), Align2::RIGHT_CENTER, left(shown), font, TEXT_DIM);
        }
        // Play or Pause, Start Over, Forward, Loop and Sound.
        let [play_at, start_at, forward_at, loop_at, sound_at] = if compact {
            let y = row.center().y;
            [15.0, 45.0, -69.0, -41.0, -13.0].map(|x: f32| pos2(if x > 0.0 { row.min.x + x } else { row.max.x + x }, y))
        } else {
            let middle = row.center();
            [0.0, -1.0, 1.0, -2.0, 2.0].map(|n: f32| middle + vec2(52.0 * n, 0.0))
        };
        // Play or Pause: the biggest, a white circle.
        let radius = 20.0 * scale;
        let big = ui.interact(
            Rect::from_center_size(play_at, egui::Vec2::splat(radius * 2.0 + 2.0)),
            ui.scope_id().with("video play"),
            Sense::click(),
        );
        ui.painter().circle_filled(play_at, if big.hovered() { radius + 1.0 } else { radius }, Color32::WHITE);
        let (play, words) = if video.playing { (Icon::Pause, "Pause") } else { (Icon::Play, "Play") };
        let nudge = if video.playing { 0.0 } else { 1.5 * scale };
        icons::paint(
            ui.painter(),
            play,
            Rect::from_center_size(play_at + vec2(nudge, 0.0), egui::Vec2::splat(16.0 * scale)),
            Color32::from_gray(28),
        );
        if big.on_hover_text(words).clicked() {
            state.video_request = Some(VideoRequest::TogglePause);
        }
        // Start Over, and Forward ten seconds.
        if round(ui, start_at, "video start", "Start over, from the beginning").clicked() {
            state.video_request = Some(VideoRequest::Restart);
        }
        icons::paint(
            ui.painter(),
            Icon::ToStart,
            Rect::from_center_size(start_at, egui::Vec2::splat(15.0 * scale)),
            TEXT,
        );
        let forward = round(ui, forward_at, "video forward", "Forward 10 seconds");
        // The arrow that turns clockwise (the icon named Back is drawn that way), with 10 in it.
        icons::paint(
            ui.painter(),
            Icon::Back,
            Rect::from_center_size(forward_at, egui::Vec2::splat(20.0 * scale)),
            TEXT,
        );
        let ten = FontId::proportional(if compact { 7.5 } else { 8.5 });
        ui.painter().text(forward_at + vec2(0.0, 1.5 * scale), Align2::CENTER_CENTER, "10", ten, TEXT);
        if forward.clicked()
            && let Some(length) = length
        {
            state.video_request = Some(VideoRequest::Seek((video.position + VIDEO_STEP).min(length)));
        }
        // Loop and Sound: green while it loops; a speaker, struck through while muted.
        let (loop_ink, loop_hint) = if video.looping {
            (ON, "Looping: it starts again at the end. Click to stop looping")
        } else {
            (TEXT_FAINT, "Not looping: it stops at the end. Click to loop")
        };
        if round(ui, loop_at, "video loop", loop_hint).clicked() {
            state.video_request = Some(VideoRequest::Looping(!video.looping));
        }
        icons::paint(
            ui.painter(),
            Icon::Loop,
            Rect::from_center_size(loop_at, egui::Vec2::splat(17.0 * scale)),
            loop_ink,
        );
        if video.looping {
            ui.painter().circle_filled(loop_at + vec2(0.0, 13.0 * scale), 1.8 * scale, ON);
        }
        let (sound, sound_ink, sound_hint) = if video.muted {
            (Icon::Muted, TEXT_FAINT, "Muted. Click to play its sound")
        } else {
            (Icon::Sound, TEXT, "Its sound plays. Click to mute")
        };
        if round(ui, sound_at, "video sound", sound_hint).clicked() {
            state.video_request = Some(VideoRequest::Muted(!video.muted));
        }
        icons::paint(ui.painter(), sound, Rect::from_center_size(sound_at, egui::Vec2::splat(17.0 * scale)), sound_ink);
    });
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
fn media_playlist_picker(ui: &mut egui::Ui, state: &mut HomeState) {
    let shown = state.media_playlist_shown.and_then(|id| state.media_playlists.iter().find(|p| p.id == id));
    if shown.is_none() {
        state.media_playlist_shown = None;
    }
    let label = shown.map_or_else(|| "All Media ▾".to_owned(), |playlist| format!("{} ▾", playlist.name));
    let picker = ui
        .add(egui::Label::new(egui::RichText::new(label).font(semibold(11.5)).color(TEXT_DIM)).sense(Sense::click()))
        .on_hover_text("Choose a media playlist");
    egui::Popup::from_toggle_button_response(&picker)
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
            if ui.button("New Playlist").on_hover_text("Add media to it from an item's right-click menu").clicked() {
                let made = crate::media::Playlist::new(crate::media::new_playlist_name(&state.media_playlists));
                state.media_playlist_shown = Some(made.id);
                state.media_playlists.push(made);
                state.media_playlists_changed = true;
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
                    let field =
                        ui.add(egui::TextEdit::singleline(&mut state.media_playlists[at].name).desired_width(140.0));
                    state.media_playlists_changed |= field.changed();
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
/// playlists it is in, and Remove.
fn media_item_menu(ui: &mut egui::Ui, state: &mut HomeState, index: usize, item: &sp_model::MediaItem) {
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
    // Delete…: every item picked out, when this is one of them.
    if ui.button("Delete…").clicked() {
        state.confirm_delete_media =
            if state.picked_media.contains(&index) { state.picked_media.clone() } else { vec![index] };
        ui.close();
    }
}

#[cfg(test)]
mod tests {
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
            picked_songs: vec![5, 7],
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
            assert_eq!(state.picked_songs, vec![5, 7]);
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
        assert_eq!(carried, Some(vec![5, 7]));
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

    fn press(at: egui::Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    /// Runs the video controls (full size, or in two rows) through frames of input, and returns
    /// what each frame asked of the video.
    fn video_frames(
        video: &crate::home::VideoLive,
        compact: bool,
        frames: &[Vec<Event>],
    ) -> Vec<Option<crate::home::VideoRequest>> {
        let ctx = egui::Context::default();
        let mut state = HomeState::default();
        let mut asked = Vec::new();
        for (n, events) in frames.iter().enumerate() {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))),
                time: Some(n as f64 * 0.1),
                events: events.clone(),
                ..RawInput::default()
            };
            let output = ctx.run_ui(input, |ui| video_controls(ui, &mut state, video, 300.0, compact));
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
        // Full size the bar is a row of its own; in two rows it is between the buttons. Find it
        // by where a drag along it scrubs (from x 100 to 180, inside the bar either way).
        for compact in [false, true] {
            let found = (10..100).map(|y| y as f32).find_map(|y| {
                let frames = vec![
                    vec![Event::PointerMoved(pos2(100.0, y))],
                    vec![press(pos2(100.0, y), true)],
                    vec![Event::PointerMoved(pos2(140.0, y))],
                    vec![Event::PointerMoved(pos2(180.0, y))],
                    vec![press(pos2(180.0, y), false)],
                    vec![],
                ];
                let asked = video_frames(&video, compact, &frames);
                asked.iter().any(|a| matches!(a, Some(VideoRequest::Scrub(_)))).then_some(asked)
            });
            let asked = found.unwrap_or_else(|| panic!("a drag along the bar scrubs (compact: {compact})"));
            let scrubs: Vec<f64> = asked
                .iter()
                .filter_map(|a| if let Some(VideoRequest::Scrub(at)) = a { Some(*at) } else { None })
                .collect();
            let landed = asked.iter().find_map(|a| if let Some(VideoRequest::Seek(at)) = a { Some(*at) } else { None });
            let landed = landed.expect("letting go goes there");
            assert!(scrubs.windows(2).all(|w| w[0] <= w[1]), "dragging right moves on: {scrubs:?}");
            assert!(landed >= scrubs[0] && landed <= 100.0 && landed > 30.0, "{landed} after {scrubs:?}");
        }
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
}
