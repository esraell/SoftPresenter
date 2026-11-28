//! The Edit view's right-hand panel: the settings of what is picked, in tabs that follow it (the
//! object's own tab, named for what it is, then Box and Animation). Every row is a caption in
//! small capitals at the left and its controls after it; a row that belongs to a switch on the
//! same tab stays while the switch is off, greyed, so nothing moves on a click.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, pos2, vec2};
use sp_model::{
    Align as TextAlign, BoxStyle, Caps, LineTransform, ObjectKind, Rgba, SLIDE_HEIGHT, SLIDE_WIDTH, Scaling, TextStyle,
    Theme, VAlign,
};
use sp_render::object_frame;

use crate::editor::MIN_SIDE;
use crate::home::HomeState;
use crate::i18n::{t, tf, tk};
use crate::quick_help::Hint;

const PANEL: Color32 = crate::studio::PANEL;
/// What is chosen: grey, as the rest of the app.
const BLUE: Color32 = crate::studio::SELECTED;
const CONTROL: Color32 = Color32::from_rgb(45, 45, 45);
/// Fields and menus in an object's tabs: a step lighter than the panel.
const WELL: Color32 = crate::studio::CARD;
const FIELD: Color32 = Color32::from_rgb(20, 20, 20);
const RULE: Color32 = Color32::from_rgb(50, 50, 50);
const INK: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(150);
/// Space above and below the controls in the scrolling body.
const TOP: f32 = 8.0;
const BOTTOM: f32 = 10.0;
const ROW: f32 = 24.0;
/// The width of the panel and of the controls inside it. Rows are sized from this, not from
/// the space they find, so a row can never push the panel wider.
const PANEL_WIDTH: f32 = crate::studio::RIGHT_WIDTH - 20.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorTab {
    /// The object's own tab: Text, Shape, Picture, Web Page, NDI Source and so on.
    #[default]
    Own,
    Box,
    Animation,
}

/// What the panel remembers between frames.
#[derive(Debug, Default)]
pub struct InspectorState {
    pub tab: InspectorTab,
    /// The look taken by Copy Style, for Paste Style.
    pub copied_style: Option<TextStyle>,
    /// Keep the shape of the box when its width or height is typed.
    pub lock_shape: bool,
    /// True while the slide's notes are open under the inspector.
    pub notes_open: bool,
    /// What has been typed to narrow the list of fonts.
    pub font_search: String,
    /// Set when a control styled the words selected on the slide, so the field being typed in
    /// takes the keyboard back with the same words selected.
    pub refocus_typing: bool,
    /// Set when the change just made is a step of its own for Undo.
    pub new_step: bool,
    /// A− or A+ was pressed beside Size: how much to step every size by (slide units).
    pub size_step: f32,
    /// The address field of a web page box just added (see [`web_address`]), for the Edit view's
    /// keys, which are read before the panel is drawn.
    pub address: Option<AddressField>,
    /// The text field in the panel that had the keyboard when it was last drawn, if one had: an
    /// Escape that takes the keyboard out of it is its alone (see `edit_view`).
    pub field_with_keys: Option<egui::Id>,
    /// The boxes picked when the Gradient and To rows were last shown for a box that shows a
    /// picture (one saved with a gradient by an older version): the rows stay while the same
    /// boxes are picked, though Gradient is then set to None, so no row under them moves.
    pub gradient_rows_for: Vec<sp_model::Uuid>,
    /// The small panel open over the rows, if one is (see [`Panel`]), and the tab and the boxes
    /// it was opened for: another tab or another pick closes it.
    pub panel: Option<Panel>,
    pub panel_for: (InspectorTab, Vec<sp_model::Uuid>),
}

/// What is set once and then left alone opens from a key on its own row, as a small panel laid
/// over the rows under it (or over those above it): no row moves or changes size when one opens.
/// One is open at a time; its key again, its ×, a click outside it or Escape closes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    /// The words' colour running to a second one: from the last swatch of the Text tab's Colour.
    TextGradient,
    /// The words' outline: its width and colour.
    TextOutline,
    /// The words' shadow: its throw, opacity and colour.
    TextShadow,
    /// The box's margins, and the room after a paragraph and under the baseline.
    Spacing,
    /// How the lines are broken, their bullets, scrolling, and the band behind them.
    Lines,
    /// The box's own colour running to a second one: from the last swatch of Behind.
    BehindGradient,
    /// The line round a box: its width and colour (the Box tab).
    BoxOutline,
    /// The shadow under a box: its throw, opacity and colour (the Box tab).
    BoxShadow,
}

/// A web page box's address field, as the Edit view's Delete and Undo need it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AddressField {
    /// The box.
    pub object: sp_model::Uuid,
    /// The field, as the toolkit knows it when it has the keyboard.
    pub field: egui::Id,
    /// The field has the keyboard only because the box was just added, ready for the address:
    /// nothing has been typed in it since, and it hasn't been clicked. Delete, Backspace and
    /// Undo are then still for the box.
    pub given: bool,
}

/// How far A− and A+ move a size, in slide units.
const SIZE_STEP: f32 = 4.0;

const WEIGHTS: [(u16, (&str, &str)); 9] = [
    (100, tk("inspector.weight.thin", "Thin")),
    (200, tk("inspector.weight.extra_light", "Extra Light")),
    (300, tk("inspector.weight.light", "Light")),
    (400, tk("inspector.weight.regular", "Regular")),
    (500, tk("inspector.weight.medium", "Medium")),
    (600, tk("inspector.weight.semibold", "Semibold")),
    (700, tk("inspector.weight.bold", "Bold")),
    (800, tk("inspector.weight.heavy", "Heavy")),
    (900, tk("inspector.weight.black", "Black")),
];

fn weight_name(weight: u16) -> &'static str {
    WEIGHTS
        .iter()
        .min_by_key(|(w, _)| w.abs_diff(weight))
        .map_or(t("inspector.weight.regular", "Regular"), |&(_, (key, english))| t(key, english))
}

/// A fill's kind in the language set (sp-model keeps the English).
fn gradient_name(kind: sp_model::GradientKind) -> &'static str {
    match kind {
        sp_model::GradientKind::None => t("inspector.gradient.none", "Color"),
        sp_model::GradientKind::Linear => t("inspector.gradient.linear", "Linear Gradient"),
        sp_model::GradientKind::Radial => t("inspector.gradient.radial", "Radial Gradient"),
    }
}

/// How a picture sits in its place, in the language set.
fn fit_name(fit: sp_model::ImageFit) -> &'static str {
    match fit {
        sp_model::ImageFit::Fit => t("inspector.fit.fit", "Fit"),
        sp_model::ImageFit::Fill => t("inspector.fit.fill", "Fill"),
        sp_model::ImageFit::Stretch => t("inspector.fit.stretch", "Stretch"),
    }
}

/// What a text box can be linked to, in the language set.
pub fn link_name(link: sp_model::TextLink) -> &'static str {
    use sp_model::TextLink;
    match link {
        TextLink::Clock => t("inspector.link.clock", "Clock"),
        TextLink::Date => t("inspector.link.date", "Date"),
        TextLink::Timer { .. } => t("inspector.link.timer_kind", "Timer"),
        TextLink::SongTitle => t("inspector.link.song_title", "Song Title"),
        TextLink::Group => t("inspector.link.group", "Group"),
        TextLink::SlideLabel => t("inspector.link.slide_label", "Slide Label"),
        TextLink::NextSlide => t("inspector.link.next_slide", "Next Slide"),
        TextLink::Notes => t("inspector.link.notes", "Slide Notes"),
        TextLink::StageMessage => t("inspector.link.stage_message", "Stage Message"),
        TextLink::Copyright => t("inspector.link.copyright", "Copyright"),
    }
}

/// A link to the timer called `name`, as the linked text choices offer it.
pub fn timer_link_name(name: &str) -> String {
    tf("inspector.link.timer", "Timer: {name}", &[("name", name)])
}

/// The marks before each line of a box, in the language set.
fn list_name(list: sp_model::ListStyle) -> &'static str {
    match list {
        sp_model::ListStyle::None => t("inspector.list.none", "None"),
        sp_model::ListStyle::Bullets => t("inspector.list.bullets", "Bullets"),
        sp_model::ListStyle::Numbers => t("inspector.list.numbers", "Numbers"),
    }
}

/// How an object comes on or goes off, in the language set.
fn effect_name(effect: sp_model::BuildEffect, coming_in: bool) -> &'static str {
    use sp_model::BuildEffect;
    match (effect, coming_in) {
        (BuildEffect::Dissolve, _) => t("inspector.build.dissolve", "Dissolve"),
        (BuildEffect::Fly, true) => t("inspector.build.fly_in", "Fly In"),
        (BuildEffect::Fly, false) => t("inspector.build.fly_out", "Fly Out"),
        (BuildEffect::Zoom, _) => t("inspector.build.zoom", "Zoom"),
    }
}

fn to_color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn to_rgba(c: Color32) -> Rgba {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Rgba { r, g, b, a }
}

/// A thin line across the whole panel, with room above and below.
fn rule(ui: &mut egui::Ui) {
    ui.add_space(9.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, RULE);
    ui.add_space(9.0);
}

/// A slider from 0 to 100% with the percentage beside it in a number box of its own (so each
/// says what it does for Quick Help), as the slider's own value showed it; a whole percentage
/// whatever is typed. True when either changed it.
fn percent_slider(ui: &mut egui::Ui, percent: &mut f32, hint: &str) -> bool {
    let slider = egui::Slider::new(percent, 0.0..=100.0).step_by(1.0).show_value(false);
    let moved = ui.add(slider).hint(hint).changed();
    let typed = ui
        .add(
            egui::DragValue::new(percent)
                .range(0.0..=100.0)
                .speed(1.0)
                .custom_formatter(|n, _| percent_text(n))
                .custom_parser(typed_percent),
        )
        .hint(hint)
        .changed();
    *percent = percent.round();
    moved || typed
}

/// A percentage as the language writes it ("75%", "75 %"), for a number box or a slider's.
fn percent_text(n: f64) -> String {
    crate::i18n::percent(n.round().clamp(0.0, 100.0) as u64)
}

/// A percentage typed in a box: its digits (with a decimal point or comma), with or without
/// the sign and its space.
fn typed_percent(typed: &str) -> Option<f64> {
    let number: String = typed
        .chars()
        .filter_map(|c| match c {
            '0'..='9' | '.' => Some(c),
            ',' => Some('.'),
            _ => None,
        })
        .collect();
    number.parse().ok()
}

/// How wide the box with a slider's percentage is: room for "100 %", so the slider stays as long
/// whatever it says.
const PERCENT_BOX: f32 = 59.0;

/// Four lines of text, drawn left, centred, right or full width.
fn text_lines(painter: &egui::Painter, cell: Rect, which: usize, ink: Color32) {
    let (w, x0) = (15.0, cell.center().x - 7.5);
    for (row, part) in [1.0_f32, 0.6, 0.85, 0.5].into_iter().enumerate() {
        let len = if which == 3 { w } else { w * part };
        let x = match which {
            0 | 3 => x0,
            1 => x0 + (w - len) / 2.0,
            _ => x0 + w - len,
        };
        let y = cell.center().y - 5.25 + 3.5 * row as f32;
        painter.line_segment([pos2(x, y), pos2(x + len, y)], Stroke::new(1.3, ink));
    }
}

/// An arrow against a line: text to the top, the middle or the bottom of its box.
fn vertical_mark(painter: &egui::Painter, cell: Rect, which: usize, ink: Color32) {
    let c = cell.center();
    let pen = Stroke::new(1.3, ink);
    let arrow = |tip: egui::Pos2, up: bool| {
        let d = if up { 1.0 } else { -1.0 };
        painter.line_segment([tip, pos2(tip.x, tip.y + 6.0 * d)], pen);
        painter.line_segment([tip, pos2(tip.x - 2.5, tip.y + 2.5 * d)], pen);
        painter.line_segment([tip, pos2(tip.x + 2.5, tip.y + 2.5 * d)], pen);
    };
    match which {
        0 => {
            painter.line_segment([pos2(c.x - 6.0, c.y - 6.0), pos2(c.x + 6.0, c.y - 6.0)], pen);
            arrow(pos2(c.x, c.y - 3.5), true);
        }
        1 => {
            painter.line_segment([pos2(c.x - 6.0, c.y), pos2(c.x + 6.0, c.y)], pen);
            arrow(pos2(c.x, c.y - 1.5), false);
            arrow(pos2(c.x, c.y + 1.5), true);
        }
        _ => {
            painter.line_segment([pos2(c.x - 6.0, c.y + 6.0), pos2(c.x + 6.0, c.y + 6.0)], pen);
            arrow(pos2(c.x, c.y + 3.5), false);
        }
    }
}

/// Two bars against a line: objects lined up on one side, or through the middle.
/// `which` 0..3 line up left, centre, right; 3..6 top, middle, bottom.
fn align_mark(painter: &egui::Painter, cell: Rect, which: usize, ink: Color32) {
    let c = cell.center();
    let across = which < 3;
    let side = which % 3;
    let pen = Stroke::new(1.2, ink);
    // Drawn as if lining up left/centre/right, then turned for top/middle/bottom.
    let at =
        |along: f32, other: f32| if across { pos2(c.x + along, c.y + other) } else { pos2(c.x + other, c.y + along) };
    let edge = [-7.0, 0.0, 7.0][side];
    painter.line_segment([at(edge, -7.0), at(edge, 7.0)], pen);
    for (other, len) in [(-3.0_f32, 9.0_f32), (3.0, 6.0)] {
        let (from, to) = match side {
            0 => (edge + 1.5, edge + 1.5 + len),
            1 => (-len / 2.0, len / 2.0),
            _ => (edge - 1.5 - len, edge - 1.5),
        };
        painter.rect_filled(Rect::from_two_pos(at(from, other - 1.5), at(to, other + 1.5)), 0.5, ink);
    }
}

/// A place `width` wide and a key tall in a row, for a menu: one whose words are cut short may
/// draw a hair wider than it is asked to be. It is drawn in a layout of its own inside the
/// place, which the row has already counted, so what follows it in the row stays where it should.
fn fixed<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (place, _) = ui.allocate_exact_size(vec2(width, KEY), Sense::hover());
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(place).layout(Layout::left_to_right(Align::Center)));
    add(&mut inside)
}

/// A tick box with a bold title, as each group of options starts with.
fn tick(ui: &mut egui::Ui, on: &mut bool, title: &str) -> egui::Response {
    ui.checkbox(on, egui::RichText::new(title).size(13.0).strong().color(INK))
}

/// A pop-up list showing its current choice.
fn choice<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    (salt, hint): (&str, &str),
    width: f32,
    value: &mut T,
    options: &[(T, &str)],
) {
    let shown = options.iter().find(|(v, _)| v == value).map_or("", |(_, name)| name);
    fixed(ui, width, |ui| {
        egui::ComboBox::from_id_salt(salt)
            .selected_text(shown)
            .width(width)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .show_ui(ui, |ui| {
                for (option, name) in options {
                    ui.selectable_value(value, *option, *name);
                }
            })
            .response
            .hint(hint);
    });
}

/// The colours in the grid at the top of every colour pop-up: greys,
/// then rows of hues from deep to light.
const SWATCHES: [[u8; 3]; 40] = [
    [255, 255, 255],
    [217, 217, 217],
    [166, 166, 166],
    [115, 115, 115],
    [64, 64, 64],
    [32, 32, 32],
    [0, 0, 0],
    [18, 24, 38],
    [255, 59, 48],
    [255, 149, 0],
    [255, 204, 0],
    [52, 199, 89],
    [0, 199, 190],
    [0, 122, 255],
    [88, 86, 214],
    [175, 82, 222],
    [176, 30, 25],
    [178, 98, 0],
    [184, 146, 0],
    [32, 128, 56],
    [0, 128, 122],
    [0, 74, 160],
    [52, 50, 140],
    [108, 44, 140],
    [255, 160, 152],
    [255, 204, 140],
    [255, 232, 140],
    [152, 230, 170],
    [140, 232, 226],
    [140, 190, 255],
    [180, 178, 240],
    [214, 170, 240],
    [255, 45, 85],
    [162, 132, 94],
    [255, 214, 165],
    [253, 246, 227],
    [200, 230, 201],
    [187, 222, 251],
    [225, 190, 231],
    [255, 255, 200],
];

/// How tall a key, a field and a menu are in an object's tabs.
const KEY: f32 = 30.0;
/// How wide a key with one letter or picture is, where its row has the room.
const KEY_WIDE: f32 = 34.0;
/// The room between the controls of one row.
const BETWEEN: f32 = 5.0;
/// The room between one row and the next.
const ROWS_APART: f32 = 12.0;
/// The room between a row's caption and its controls.
const CAPTION_GAP: f32 = 8.0;
/// The narrowest and the widest the column of captions is: it is as wide as the longest caption
/// of the tab in the language set, between these.
const CAPTION: f32 = 58.0;
const CAPTION_MOST: f32 = 104.0;
/// The side of a colour swatch where its row has the room.
const SWATCH: f32 = 21.0;
/// How tall a small number with its name over it is.
const MINI: f32 = 46.0;
/// How wide a number on a row of its own is.
const NUMBER: f32 = 62.0;

/// The face of a row's caption: the small capitals of the fixed-width face.
fn caption_font() -> FontId {
    crate::studio::fixed_semibold(11.0)
}

/// The room between a caption's letters.
const CAPTION_SPACING: f32 = 0.6;

/// How wide the column of captions is for a tab whose rows are named `captions`.
fn caption_column(ui: &egui::Ui, captions: &[&str]) -> f32 {
    captions
        .iter()
        .map(|caption| {
            crate::studio::spaced(ui, &caption.to_uppercase(), caption_font(), DIM, CAPTION_SPACING).size().x
        })
        .fold(CAPTION, f32::max)
        .ceil()
        .min(CAPTION_MOST)
}

/// A caption set in its small capitals on one line at most `room` wide: one that is wider is
/// cut short with "…" (and the galley says so, `elided`).
fn caption_within(ui: &egui::Ui, caption: &str, room: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat {
        font_id: caption_font(),
        color: crate::studio::TEXT_DIM,
        extra_letter_spacing: CAPTION_SPACING,
        ..Default::default()
    };
    job.append(&caption.to_uppercase(), 0.0, format);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room.max(0.0));
    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
}

/// The room a row's controls have after a caption column `column` wide.
fn control_room(column: f32) -> f32 {
    PANEL_WIDTH - column - CAPTION_GAP
}

/// One row of an object's tab: its caption in small capitals at the left, in a column `column`
/// wide, then its controls, [`KEY`] tall.
fn row(ui: &mut egui::Ui, column: f32, caption: &str, add: impl FnOnce(&mut egui::Ui)) -> Rect {
    row_tall(ui, column, caption, KEY, add)
}

/// A [`row`] `tall` points tall, its controls standing on its foot. Returns where it is, across
/// the whole column, for a panel to open against.
fn row_tall(ui: &mut egui::Ui, column: f32, caption: &str, tall: f32, add: impl FnOnce(&mut egui::Ui)) -> Rect {
    let width = ui.available_width();
    let drawn = ui.allocate_ui_with_layout(vec2(width, tall), Layout::left_to_right(Align::Max), |ui| {
        ui.spacing_mut().item_spacing.x = BETWEEN;
        let (cell, over) = ui.allocate_exact_size(vec2(column, tall), Sense::hover());
        // A caption longer than the widest column is cut short with "…", never drawn under the
        // controls beside it, and said whole under the pointer.
        let words = caption_within(ui, caption, column);
        let at = pos2(cell.min.x, cell.center().y - words.size().y / 2.0);
        let cut = words.elided;
        ui.painter().galley(at, words, crate::studio::TEXT_DIM);
        if cut {
            over.full_name(caption);
        }
        ui.add_space(CAPTION_GAP - BETWEEN);
        add(ui);
    });
    whole_row(ui, drawn.response.rect)
}

/// The line that says what the rows under it change: a title in small capitals, in ink, and a
/// faint note after it. One that is not the `first` thing in its tab has a thin line over it.
fn scope_line(ui: &mut egui::Ui, first: bool, title: &str, note: &str) {
    if !first {
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(line, 0.0, crate::studio::LINE);
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 2.0);
        let look = crate::studio::Lettering::cap(INK);
        let words = crate::studio::spaced(ui, &title.to_uppercase(), look.font, INK, look.spacing);
        let (cell, _) = ui.allocate_exact_size(words.size(), Sense::hover());
        ui.painter().galley(cell.min, words, INK);
        if !note.is_empty() {
            ui.label(egui::RichText::new(note).size(12.0).color(crate::studio::TEXT_FAINT));
        }
    });
}

/// A faint line of words under a row, saying what is so.
fn line_of_words(ui: &mut egui::Ui, words: &str, ink: Color32) {
    ui.label(egui::RichText::new(words).size(12.0).color(ink));
}

/// How wide each of `count` keys in a row is: [`KEY_WIDE`] where `room` has that much for them
/// all, [`BETWEEN`] apart, and otherwise an even share of it.
fn key_width(room: f32, count: usize) -> f32 {
    ((room - BETWEEN * (count as f32 - 1.0)) / count as f32).floor().min(KEY_WIDE)
}

/// A key with a letter or a small picture on it, `width` wide: the button grey, lighter under
/// the pointer, the chosen grey while it is `on`, and half lit (its lower half) while it is
/// `mixed`: some of what is picked has it and some does not. `draw` paints its face.
fn square_key(
    ui: &mut egui::Ui,
    width: f32,
    (on, mixed): (bool, bool),
    hint: &str,
    draw: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    use crate::studio::Flat;
    let (rect, response) = ui.allocate_exact_size(vec2(width, KEY), Sense::click());
    let painter = ui.painter();
    crate::studio::paint_flat(painter, rect, 9.0, if on { Flat::On } else { Flat::Plain }, response.hovered());
    if mixed {
        let lower = Rect::from_min_max(pos2(rect.min.x, rect.center().y), rect.max);
        painter.rect_filled(lower, egui::CornerRadius { nw: 0, ne: 0, sw: 9, se: 9 }, BLUE);
    }
    draw(painter, rect, INK);
    // A key that is only a picture: its name always shows (the label it doesn't have).
    response.full_name(hint)
}

/// A [`square_key`] with a letter or two on it.
fn letter_key(ui: &mut egui::Ui, width: f32, look: (bool, bool), hint: &str, letters: &str) -> egui::Response {
    square_key(ui, width, look, hint, |painter, rect, ink| {
        painter.text(rect.center(), Align2::CENTER_CENTER, letters, crate::studio::semibold(12.5), ink);
    })
}

/// The face of the words on a [`word_key`].
fn word_key_font() -> FontId {
    FontId::proportional(12.5)
}

/// A key with words on it, as wide as they need: the chosen grey while it is `on`, faint and
/// taking no clicks while it is not `enabled`.
fn word_key(ui: &mut egui::Ui, words: &str, on: bool, enabled: bool) -> egui::Response {
    use crate::studio::Flat;
    let width = crate::studio::flat_button_width(ui, &[words], &word_key_font(), 13.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, KEY), Sense::hover());
    let id = ui.scope_id().with(("word key", words));
    let look = if on { Flat::On } else { Flat::Plain };
    crate::studio::flat_button(ui, rect, id, words, word_key_font(), look, enabled && ui.is_enabled())
}

/// A number in a field `width` wide and [`KEY`] tall.
fn number(
    ui: &mut egui::Ui,
    value: &mut f32,
    (width, hint): (f32, &str),
    make: impl FnOnce(egui::DragValue) -> egui::DragValue,
) -> egui::Response {
    ui.add_sized([width, KEY], make(egui::DragValue::new(value).speed(1.0).max_decimals(0))).hint(hint)
}

/// A number `width` wide with its name over it in small capitals: the usual words where they
/// fit that width, else their short form.
fn mini(
    ui: &mut egui::Ui,
    (width, hint): (f32, &str),
    (key, english): (&'static str, &'static str),
    value: &mut f32,
    make: impl FnOnce(egui::DragValue) -> egui::DragValue,
) -> egui::Response {
    let name = crate::studio::within(ui, key, english, mini_font(), 0.0, width, true);
    mini_named(ui, (width, hint), &name, value, make)
}

/// The face of the name over a [`mini`] number.
fn mini_font() -> FontId {
    crate::studio::fixed_semibold(10.5)
}

/// [`mini`] with its name as given.
fn mini_named(
    ui: &mut egui::Ui,
    (width, hint): (f32, &str),
    name: &str,
    value: &mut f32,
    make: impl FnOnce(egui::DragValue) -> egui::DragValue,
) -> egui::Response {
    ui.allocate_ui_with_layout(vec2(width, MINI), Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        let (cell, _) = ui.allocate_exact_size(vec2(width, MINI - KEY - 3.0), Sense::hover());
        let ink = crate::studio::TEXT_FAINT;
        crate::studio::paint_spaced(ui, cell.left_center(), Align2::LEFT_CENTER, name, mini_font(), ink, 0.0);
        number(ui, value, (width, hint), make)
    })
    .inner
}

/// How wide each of `count` [`mini`] numbers is across `room`.
fn mini_width(room: f32, count: usize) -> f32 {
    ((room - BETWEEN * (count as f32 - 1.0)) / count as f32).floor()
}

/// The colours every row of swatches offers, as the design has them: white, black, a warm grey
/// and five hues.
const ROW_SWATCHES: [[u8; 3]; 8] = [
    [0xff, 0xff, 0xff],
    [0x00, 0x00, 0x00],
    [0xd2, 0xce, 0xc6],
    [0xff, 0x4a, 0x1a],
    [0xff, 0xc4, 0x00],
    [0x34, 0xc5, 0x6b],
    [0x42, 0x85, 0xf4],
    [0xcc, 0x4e, 0xb4],
];

/// The ring round the swatch in use: a gap of the panel's colour, then ink.
fn paint_chosen(painter: &egui::Painter, cell: Rect) {
    painter.rect_stroke(cell.expand(2.0), 7.0, Stroke::new(2.0, PANEL), egui::StrokeKind::Inside);
    painter.rect_stroke(cell.expand(4.0), 9.0, Stroke::new(2.0, INK), egui::StrokeKind::Inside);
}

/// What a row of swatches is for.
struct Swatches<'a> {
    /// Tells this row's swatches from another row's.
    salt: &'a str,
    /// The room the row has; its swatches are as big as fit in it on one line, [`SWATCH`] at
    /// the most.
    room: f32,
    /// How many of [`ROW_SWATCHES`] it offers.
    count: usize,
    /// It starts with a swatch for no colour at all.
    none_too: bool,
    /// What is picked has several colours: no swatch is ringed, and the last well shows this
    /// one beside the first.
    mixed: Option<Rgba>,
}

/// How big each swatch of a row is, and how many it has with the well for any colour.
fn swatch_side(row: &Swatches) -> (f32, usize) {
    let cells = row.count + 1 + usize::from(row.none_too);
    let side = ((row.room - BETWEEN * (cells as f32 - 1.0)) / cells as f32).floor().min(SWATCH);
    (side, cells)
}

/// A row of colour swatches, always on one line: none (when the row has it), the colours, then
/// a well for any other colour, which opens the grid of swatches and the full picker. The one
/// in use is ringed. Returns true when the colour was changed.
fn swatch_row(ui: &mut egui::Ui, row: &Swatches, colour: &mut Option<Rgba>) -> bool {
    let (side, cells) = swatch_side(row);
    let (strip, _) =
        ui.allocate_exact_size(vec2(side * cells as f32 + BETWEEN * (cells as f32 - 1.0), side), Sense::hover());
    let id = ui.scope_id().with(("swatches", row.salt));
    let cell_at = |place: usize| {
        Rect::from_min_size(pos2(strip.min.x + place as f32 * (side + BETWEEN), strip.min.y), vec2(side, side))
    };
    let edge = Stroke::new(1.0, crate::studio::LINE);
    let before = *colour;
    let now = before.filter(|_| row.mixed.is_none());
    let alpha = before.map_or(255, |c| c.a);
    let mut place = 0;
    if row.none_too {
        let cell = cell_at(place);
        place += 1;
        let response = ui.interact(cell, id.with("none"), Sense::click()).hint(t("inspector.swatch.none", "None"));
        let painter = ui.painter();
        painter.rect_filled(cell, 5.0, crate::studio::CARD);
        let inside = cell.shrink(3.0);
        painter.line_segment([inside.left_bottom(), inside.right_top()], Stroke::new(1.5, crate::studio::TALLY));
        painter.rect_stroke(cell, 5.0, edge, egui::StrokeKind::Inside);
        if before.is_none() && row.mixed.is_none() {
            paint_chosen(painter, cell);
        }
        if response.clicked() {
            *colour = None;
        }
    }
    let offered = &ROW_SWATCHES[..row.count.min(ROW_SWATCHES.len())];
    for [r, g, b] in offered {
        let cell = cell_at(place);
        let response =
            ui.interact(cell, id.with(place), Sense::click()).hint(t("quickhelp.inspector.swatch", "Use this colour."));
        place += 1;
        let painter = ui.painter();
        painter.rect_filled(cell, 5.0, Color32::from_rgb(*r, *g, *b));
        painter.rect_stroke(cell, 5.0, edge, egui::StrokeKind::Inside);
        if now.is_some_and(|c| [c.r, c.g, c.b] == [*r, *g, *b]) {
            paint_chosen(painter, cell);
        }
        if response.clicked() {
            *colour = Some(Rgba { r: *r, g: *g, b: *b, a: alpha });
        }
    }
    // Any other colour: the well shows it once one is chosen, and the two when they are mixed.
    let cell = cell_at(place);
    let own = now.filter(|c| !offered.contains(&[c.r, c.g, c.b]));
    let hint = if row.mixed.is_some() {
        t("inspector.swatch.mixed", "Mixed colours: choose one for all")
    } else {
        t("inspector.swatch.any", "Any colour")
    };
    let response = ui.interact(cell, id.with("any"), Sense::click()).hint(hint);
    let painter = ui.painter();
    match (row.mixed, before, own) {
        (Some(other), Some(first), _) => {
            painter.rect_filled(cell, 5.0, to_color32(first));
            let lower = vec![cell.right_top(), cell.right_bottom(), cell.left_bottom()];
            painter.add(egui::Shape::convex_polygon(lower, to_color32(other), Stroke::NONE));
        }
        (_, _, Some(own)) => {
            painter.rect_filled(cell, 5.0, to_color32(own));
        }
        _ => paint_any_colour(painter, cell),
    }
    painter.rect_stroke(cell, 5.0, edge, egui::StrokeKind::Inside);
    if own.is_some() {
        paint_chosen(painter, cell);
    }
    let mut picked = before.unwrap_or(Rgba { r: 128, g: 128, b: 128, a: 255 });
    let start = picked;
    colour_popup(&response, &mut picked);
    if picked != start {
        *colour = Some(picked);
    }
    *colour != before
}

/// The face of the well for any colour while none of its own is chosen: the hues side by side.
fn paint_any_colour(painter: &egui::Painter, cell: Rect) {
    let hues = &ROW_SWATCHES[3..];
    let each = cell.width() / hues.len() as f32;
    for (place, [r, g, b]) in hues.iter().enumerate() {
        let band = Rect::from_min_size(pos2(cell.min.x + each * place as f32, cell.min.y), vec2(each, cell.height()));
        let corner = |at: usize| if place == at { 5 } else { 0 };
        let last = hues.len() - 1;
        let corners = egui::CornerRadius { nw: corner(0), sw: corner(0), ne: corner(last), se: corner(last) };
        painter.rect_filled(band, corners, Color32::from_rgb(*r, *g, *b));
    }
}

/// [`swatch_row`] for something that always has a colour.
fn swatches(ui: &mut egui::Ui, row: &Swatches, colour: &mut Rgba) {
    let mut chosen = Some(*colour);
    if swatch_row(ui, row, &mut chosen)
        && let Some(chosen) = chosen
    {
        *colour = chosen;
    }
}

/// What a colour well or a row's last swatch opens: the grid of swatches to pick from, and the
/// full colour picker under it for any other colour.
fn colour_popup(response: &egui::Response, value: &mut Rgba) {
    egui::Popup::from_toggle_button_response(response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for row in SWATCHES.chunks(8) {
                ui.horizontal(|ui| {
                    for [r, g, b] in row {
                        let colour = Color32::from_rgb(*r, *g, *b);
                        let (cell, chosen) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                        ui.painter().rect_filled(cell, 4.0, colour);
                        if chosen.hovered() {
                            ui.painter().rect_stroke(
                                cell,
                                4.0,
                                egui::Stroke::new(2.0, Color32::WHITE),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if chosen.clicked() {
                            *value = Rgba { r: *r, g: *g, b: *b, a: value.a };
                        }
                    }
                });
            }
            ui.add_space(6.0);
            let mut c = to_color32(*value);
            if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::OnlyBlend) {
                *value = to_rgba(c);
            }
        });
}

/// A colour well `width` wide that opens [`colour_popup`].
fn colour_well(ui: &mut egui::Ui, width: f32, tall: f32, value: &mut Rgba, hint: &str) {
    let (well, response) = ui.allocate_exact_size(vec2(width, tall), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(well, 5.0, to_color32(*value));
    painter.rect_stroke(well, 5.0, egui::Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
    let response = response.hint(hint);
    colour_popup(&response, value);
}

/// The colour well of the theme's and the stage layout's panels.
fn colour(ui: &mut egui::Ui, value: &mut Rgba, hint: &str) {
    colour_well(ui, 54.0, ROW - 2.0, value, hint);
}

/// The rows of a fill that can run to a second colour: Gradient (which kind, and for a linear
/// one its angle) and To (the colour it runs to). They stay while there is no gradient, or no
/// fill at all (`filled`), greyed, so no row under them moves.
fn gradient_rows(ui: &mut egui::Ui, column: f32, salt: &str, filled: bool, gradient: &mut sp_model::Gradient) {
    let room = control_room(column);
    row(ui, column, t("inspector.row.gradient", "Gradient"), |ui| {
        ui.add_enabled_ui(filled, |ui| {
            let options: Vec<(sp_model::GradientKind, &str)> =
                sp_model::GradientKind::ALL.iter().map(|kind| (*kind, gradient_name(*kind))).collect();
            choice(
                ui,
                (salt, t("quickhelp.inspector.fill_kind", "A plain colour, or one that fades to another.")),
                room - NUMBER - BETWEEN,
                &mut gradient.kind,
                &options,
            );
            ui.add_enabled_ui(gradient.kind == sp_model::GradientKind::Linear, |ui| {
                number(ui, &mut gradient.angle, (NUMBER, ""), |d| d.range(0.0..=359.0).suffix("°"))
                    .hint(t("inspector.gradient.angle", "The way it runs: 0° left to right, 90° top to bottom"));
            });
        });
    });
    let swatches_tall = swatch_side(&Swatches { salt, room, count: 8, none_too: false, mixed: None }).0;
    row_tall(ui, column, t("inspector.row.to", "To"), swatches_tall, |ui| {
        ui.add_enabled_ui(filled && gradient.on(), |ui| {
            swatches(ui, &Swatches { salt, room, count: 8, none_too: false, mixed: None }, &mut gradient.to);
        });
    });
}

/// A row of swatches under `caption`, as tall as its swatches.
fn swatches_row(
    ui: &mut egui::Ui,
    column: f32,
    caption: &str,
    look: &Swatches,
    add: impl FnOnce(&mut egui::Ui, &Swatches),
) -> Rect {
    row_tall(ui, column, caption, swatch_side(look).0, |ui| add(ui, look))
}

/// The room between a panel's edge and its rows.
const PANEL_PAD: f32 = 10.0;

/// The room a panel's row has for its controls after a caption column `column` wide.
fn panel_room(column: f32) -> f32 {
    PANEL_WIDTH - 2.0 * PANEL_PAD - column - CAPTION_GAP
}

/// The whole width of the row just drawn, for a panel to open against.
fn whole_row(ui: &egui::Ui, row: Rect) -> Rect {
    let across = ui.max_rect().x_range();
    Rect::from_x_y_ranges(across, row.y_range())
}

/// The key that opens a panel: three dots, lit while its panel is open, with a small dot in the
/// accent colour while something inside is not as it comes (`changed`), so a setting out of
/// sight is never a surprise.
fn more_key(ui: &mut egui::Ui, open: &mut Option<Panel>, which: Panel, hint: &str, changed: bool) -> egui::Response {
    let on = *open == Some(which);
    let key = square_key(ui, KEY, (on, false), hint, |painter, cell, ink| {
        let c = cell.center();
        for step in [-5.0, 0.0, 5.0] {
            painter.circle_filled(pos2(c.x + step, c.y), 1.5, ink);
        }
        if changed {
            painter.circle_filled(pos2(cell.max.x - 6.0, cell.min.y + 6.0), 2.5, crate::studio::TALLY);
        }
    });
    key.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hint));
    if key.clicked() {
        *open = if on { None } else { Some(which) };
    }
    key
}

/// A switch with the key to its look joined to it: `words` switches it on or off, and the dots
/// beside it open `which`. Returns true when the switch was clicked.
fn switch_with_more(
    ui: &mut egui::Ui,
    open: &mut Option<Panel>,
    which: Panel,
    (words, on): (&str, bool),
    hint: &str,
) -> bool {
    let between = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 1.0;
    let switch =
        t("quickhelp.inspector.effect_switch", "Switch this effect on or off; the key beside it sets its look.");
    let clicked = word_key(ui, words, on, true).hint(switch).clicked();
    ui.spacing_mut().item_spacing.x = between;
    more_key(ui, open, which, hint, false);
    clicked
}

/// The last swatch of a row that can run to a second colour: a ramp (the two colours, when
/// there is a gradient; else grey to dark), ringed while a gradient is on. It opens the
/// gradient's panel.
fn ramp_swatch(
    ui: &mut egui::Ui,
    side: f32,
    open: &mut Option<Panel>,
    which: Panel,
    ends: Option<(Rgba, Rgba)>,
) -> egui::Response {
    let (cell, key) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    let (from, to) = ends.map_or((Color32::from_gray(210), Color32::from_gray(40)), |(a, b)| {
        (to_color32(Rgba { a: 255, ..a }), to_color32(Rgba { a: 255, ..b }))
    });
    let painter = ui.painter();
    // The ramp as bands from one colour to the other, top to bottom.
    const BANDS: usize = 6;
    for band in 0..BANDS {
        let part = band as f32 / (BANDS - 1) as f32;
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * part).round() as u8;
        let colour = Color32::from_rgb(mix(from.r(), to.r()), mix(from.g(), to.g()), mix(from.b(), to.b()));
        let top = cell.min.y + cell.height() * band as f32 / BANDS as f32;
        let foot = cell.min.y + cell.height() * (band + 1) as f32 / BANDS as f32;
        let corner = |at: usize| if band == at { 5 } else { 0 };
        let corners = egui::CornerRadius { nw: corner(0), ne: corner(0), sw: corner(BANDS - 1), se: corner(BANDS - 1) };
        painter.rect_filled(Rect::from_min_max(pos2(cell.min.x, top), pos2(cell.max.x, foot)), corners, colour);
    }
    painter.rect_stroke(cell, 5.0, Stroke::new(1.0, crate::studio::LINE), egui::StrokeKind::Inside);
    if ends.is_some() || *open == Some(which) {
        paint_chosen(painter, cell);
    }
    let hint = t("inspector.gradient.more", "Gradient\u{2026}");
    key.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hint));
    if key.clicked() {
        *open = if *open == Some(which) { None } else { Some(which) };
    }
    key.hint(hint)
}

/// The room a row of `cells` swatches has, and each one's side, when one more swatch (the ramp)
/// follows them in `room`.
fn room_before_ramp(room: f32, cells: usize, most: f32) -> (f32, f32) {
    let with_ramp = cells as f32 + 1.0;
    let side = ((room - BETWEEN * (with_ramp - 1.0)) / with_ramp).floor().min(most);
    (side * cells as f32 + BETWEEN * (cells as f32 - 1.0), side)
}

/// The panel `which`, when it is the one open: a card as wide as the column, laid over the rows
/// under `against` (the row its key is on), or over those above it with `up`. Its title in
/// small capitals, a × to close it, then its rows from `add`. A click outside it or Escape
/// closes it, unless that click or Escape was for a menu or a colour picker opened from it.
fn panel_over(
    ui: &mut egui::Ui,
    open: &mut Option<Panel>,
    (which, up): (Panel, bool),
    against: Rect,
    title: &str,
    add: impl FnOnce(&mut egui::Ui),
) {
    if *open != Some(which) {
        return;
    }
    let id = egui::Id::unique(("inspector panel", which));
    let was_open = ui.ctx().read_response(id).is_some();
    // A menu or a picker opened from the panel takes the click or the Escape that closes it.
    let above = egui::Popup::is_any_open(ui.ctx());
    let frame = Frame::popup(ui.style()).inner_margin(PANEL_PAD - 1.0).corner_radius(10);
    let mut close = false;
    let shown = egui::Popup::new(id, ui.ctx().clone(), against, ui.layer_id())
        .open(true)
        .align(if up { egui::RectAlign::TOP_START } else { egui::RectAlign::BOTTOM_START })
        .gap(4.0)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .frame(frame)
        .show(|ui| {
            ui.set_width(PANEL_WIDTH - 2.0 * PANEL_PAD);
            tab_style(ui);
            ui.horizontal(|ui| {
                let look = crate::studio::Lettering::cap(INK);
                let words = crate::studio::spaced(ui, &title.to_uppercase(), look.font, INK, look.spacing);
                let (cell, _) = ui.allocate_exact_size(vec2(words.size().x, 22.0), Sense::hover());
                ui.painter().galley(pos2(cell.min.x, cell.center().y - words.size().y / 2.0), words, INK);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (cell, key) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                    let ink = if key.hovered() { INK } else { DIM };
                    crate::icons::paint(
                        ui.painter(),
                        crate::icons::Icon::Close,
                        Rect::from_center_size(cell.center(), vec2(11.0, 11.0)),
                        ink,
                    );
                    let hint = t("inspector.panel.close", "Close");
                    key.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hint));
                    close = key.hint(hint).clicked();
                });
            });
            add(ui);
        });
    let Some(shown) = shown else { return };
    let (clicked, at, escape) = ui
        .input(|input| (input.pointer.any_click(), input.pointer.interact_pos(), input.key_pressed(egui::Key::Escape)));
    let outside = clicked && was_open && at.is_some_and(|at| !shown.response.rect.contains(at));
    if close || (!above && (outside || escape)) {
        *open = None;
    }
}

/// A line of faint words in a panel, saying what is so.
fn panel_note(ui: &mut egui::Ui, words: &str) {
    ui.add(egui::Label::new(egui::RichText::new(words).size(11.5).color(crate::studio::TEXT_FAINT)).wrap());
}

/// A gradient's rows in its panel: Kind, Angle (for a linear one; greyed otherwise) and To (the
/// colour it runs to; greyed while there is none). `filled` is whether there is a colour to
/// run from.
fn gradient_panel(ui: &mut egui::Ui, salt: &str, filled: bool, gradient: &mut sp_model::Gradient) {
    let captions = [t("inspector.row.kind", "Kind"), t("inspector.shadow.angle", "Angle"), t("inspector.row.to", "To")];
    let column = caption_column(ui, &captions);
    let room = panel_room(column);
    row(ui, column, captions[0], |ui| {
        ui.add_enabled_ui(filled, |ui| {
            let options: Vec<(sp_model::GradientKind, &str)> =
                sp_model::GradientKind::ALL.iter().map(|kind| (*kind, gradient_name(*kind))).collect();
            choice(
                ui,
                (salt, t("quickhelp.inspector.fill_kind", "A plain colour, or one that fades to another.")),
                room,
                &mut gradient.kind,
                &options,
            );
        });
    });
    row(ui, column, captions[1], |ui| {
        ui.add_enabled_ui(filled && gradient.kind == sp_model::GradientKind::Linear, |ui| {
            number(ui, &mut gradient.angle, (NUMBER, ""), |d| d.range(0.0..=359.0).suffix("°"))
                .hint(t("inspector.gradient.angle", "The way it runs: 0° left to right, 90° top to bottom"));
        });
    });
    let look = Swatches { salt, room, count: 8, none_too: false, mixed: None };
    swatches_row(ui, column, captions[2], &look, |ui, look| {
        ui.add_enabled_ui(filled && gradient.on(), |ui| swatches(ui, look, &mut gradient.to));
    });
}

/// A shadow's rows in its panel: Throw (Angle, Offset and Blur), Opacity as a slider with its
/// percentage, and Colour. `can` is whether they take a change.
fn shadow_panel(ui: &mut egui::Ui, salt: &str, can: bool, look: &mut sp_model::Shadow) {
    let captions = [
        t("inspector.row.throw", "Throw"),
        t("inspector.shape.opacity", "Opacity"),
        t("inspector.row.colour", "Colour"),
    ];
    let column = caption_column(ui, &captions);
    let room = panel_room(column);
    let each = mini_width(room, 3);
    row_tall(ui, column, captions[0], MINI, |ui| {
        ui.add_enabled_ui(can, |ui| {
            mini(
                ui,
                (each, t("quickhelp.inspector.shadow_angle", "Which way the shadow falls, in degrees.")),
                tk("inspector.shadow.angle", "Angle"),
                &mut look.angle,
                |d| d.range(0.0..=360.0).suffix("°"),
            );
            mini(
                ui,
                (each, t("quickhelp.inspector.shadow_offset", "How far the shadow falls from the box.")),
                tk("inspector.shadow.offset", "Offset"),
                &mut look.offset,
                |d| d.speed(0.5).range(0.0..=200.0),
            );
            mini(
                ui,
                (each, t("quickhelp.inspector.shadow_blur", "How soft the shadow\u{2019}s edge is.")),
                tk("inspector.shadow.blur", "Blur"),
                &mut look.blur,
                |d| d.speed(0.5).range(0.0..=200.0),
            );
        });
    });
    row(ui, column, captions[1], |ui| {
        ui.add_enabled_ui(can, |ui| {
            ui.spacing_mut().slider_width = room - BETWEEN - PERCENT_BOX;
            ui.spacing_mut().interact_size.x = PERCENT_BOX;
            let shown = (look.opacity * 100.0).round();
            let mut percent = shown;
            let hint =
                t("quickhelp.inspector.shadow_opacity", "How dark the shadow is. At 100% nothing shows through it.");
            if percent_slider(ui, &mut percent, hint) && percent != shown {
                look.opacity = (percent / 100.0).clamp(0.0, 1.0);
            }
        });
    });
    let colours = Swatches { salt, room, count: 8, none_too: false, mixed: None };
    swatches_row(ui, column, captions[2], &colours, |ui, colours| {
        ui.add_enabled_ui(can, |ui| swatches(ui, colours, &mut look.color));
    });
}

/// The inspector, drawn in the room it is given: under Clear All in the right column, where the
/// tool tabs are in the other views. Returns true when the operator asks to go back to the Show
/// view.
pub fn inspector(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) -> bool {
    let mut leave = false;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let theme_slide =
            state.theme_edit.is_some() && state.editing.as_ref().is_some_and(|draft| draft.selected.is_none());
        let stage = state.theme_edit.is_none() && state.editing.as_ref().is_some_and(crate::stage_editor::is_stage);
        let layout = stage && state.editing.as_ref().is_some_and(|draft| draft.selected.is_none());
        let tabs = tabs_for(state);
        let nothing_picked = tabs.is_empty();
        // A panel belongs to the tab and the boxes it was opened for.
        let panel_for =
            (state.inspector.tab, state.editing.as_ref().map(|draft| draft.selection()).unwrap_or_default());
        if state.inspector.panel_for != panel_for {
            state.inspector.panel = None;
            state.inspector.panel_for = panel_for;
        }
        if nothing_picked {
            // One section has a plain heading, not a lone tab.
            let (heading, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
            let words = if layout {
                t("inspector.heading.layout", "LAYOUT")
            } else if theme_slide {
                t("inspector.heading.theme_slide", "THEME SLIDE")
            } else if state.edit_kind() == Some(crate::home::EditKind::Props) {
                t("inspector.heading.prop", "PROP")
            } else {
                t("inspector.heading.slide", "SLIDE")
            };
            crate::studio::paint_spaced(
                ui,
                heading.left_center(),
                Align2::LEFT_CENTER,
                words,
                crate::studio::fixed_semibold(11.5),
                crate::studio::TEXT_DIM,
                0.6,
            );
        } else {
            // Box and Animation stay open when another object is picked; a tab that what is
            // picked now doesn't have gives way to its first.
            if !tabs.iter().any(|(tab, _)| *tab == state.inspector.tab) {
                state.inspector.tab = tabs[0].0;
            }
            let names: Vec<&str> = tabs.iter().map(|(_, name)| *name).collect();
            let hints: Vec<&str> = tabs
                .iter()
                .map(|(tab, _)| match tab {
                    InspectorTab::Own => t(
                        "quickhelp.inspector.own",
                        "This box\u{2019}s own settings: its words, shape, picture or page.",
                    ),
                    InspectorTab::Box => {
                        t("quickhelp.inspector.box", "The box itself: its place, size, turn, look and order.")
                    }
                    InspectorTab::Animation => {
                        t("quickhelp.inspector.build", "How the box comes onto the slide and leaves it.")
                    }
                })
                .collect();
            let switch = crate::studio::Tabs { hints: &hints, ..crate::studio::Tabs::plain(&names) };
            let width = ui.available_width();
            let (place, _) =
                ui.allocate_exact_size(vec2(width, crate::studio::tab_switch_height(&switch)), Sense::hover());
            let open = tabs.iter().position(|(tab, _)| *tab == state.inspector.tab);
            let (_, clicked) = crate::studio::tab_switch(ui, "inspector tab", place.min, width, &switch, open);
            if let Some(clicked) = clicked {
                state.inspector.tab = tabs[clicked].0;
            }
        }
        ui.add_space(4.0);

        egui::Panel::bottom("edit footer").resizable(false).frame(Frame::new().fill(PANEL).inner_margin(8)).show(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let for_theme = state.theme_edit.is_some() || stage;
                    if !for_theme {
                        let open = state.inspector.notes_open;
                        if ui
                            .add(egui::Button::selectable(open, t("inspector.notes", "Slide Notes")))
                            .hint(t(
                                "inspector.notes.hint",
                                "Notes for whoever runs the slides; the audience never sees them",
                            ))
                            .clicked()
                        {
                            state.inspector.notes_open = !open;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let done = ui
                            .button(t("inspector.done", "Done"))
                            .hint(t("inspector.done.hint", "Go back to the Service view"));
                        if done.clicked() {
                            leave = true;
                        }
                        if !for_theme {
                            ui.checkbox(&mut state.live_edit, t("inspector.live_edit", "Live Edit")).hint(t(
                                "inspector.live_edit.hint",
                                "When on, changes to a slide that is on screen appear on the output as they are made",
                            ));
                        }
                    });
                });
            },
        );
        // The slide's notes, above the footer while they are open.
        if state.inspector.notes_open
            && state.theme_edit.is_none()
            && !stage
            && let Some(draft) = state.editing.as_mut()
        {
            egui::Panel::bottom("slide notes").resizable(false).frame(Frame::new().fill(PANEL).inner_margin(8)).show(
                ui,
                |ui| {
                    ui.label(
                        egui::RichText::new(t("inspector.notes.heading", "SLIDE NOTES")).size(10.5).strong().color(DIM),
                    );
                    let mut notes = draft.slide.notes.clone().unwrap_or_default();
                    let field = egui::TextEdit::multiline(&mut notes)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text(t("inspector.notes.placeholder", "Notes for whoever runs the slides"));
                    if ui.add(field).changed() {
                        draft.slide.notes = (!notes.trim().is_empty()).then_some(notes);
                    }
                },
            );
        }

        // Testing aid: SOFTPRESENTER_INSPECTOR_SCROLL=<points> opens the inspector scrolled down
        // that far, to take a picture of rows below the fold.
        let mut body = egui::ScrollArea::vertical().id_salt("inspector body").auto_shrink([false, false]);
        if let Some(down) = std::env::var("SOFTPRESENTER_INSPECTOR_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
            body = body.vertical_scroll_offset(down);
        }
        body.show(ui, |ui| {
            // The controls get a fixed width, the column's own, and the column is given
            // that and the height they used, never what they measured: a control wider than
            // its room cannot widen the column and take the right margin with it.
            let room = ui.available_rect_before_wrap();
            let inside =
                Rect::from_min_max(pos2(room.min.x, room.min.y + TOP), pos2(room.max.x, room.min.y + TOP + 100_000.0));
            let mut content = ui.new_child(egui::UiBuilder::new().max_rect(inside));
            {
                let ui = &mut content;
                {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    let visuals = ui.visuals_mut();
                    visuals.extreme_bg_color = FIELD;
                    visuals.widgets.inactive.weak_bg_fill = CONTROL;
                    visuals.widgets.inactive.bg_fill = CONTROL;
                    visuals.selection.bg_fill = BLUE;
                    if theme_slide {
                        theme_slide_panel(ui, state);
                    } else if layout {
                        crate::stage_editor::layout_panel(ui, state);
                    } else if nothing_picked {
                        slide_panel(ui, state);
                    } else {
                        tab_style(ui);
                        // With several objects selected, a change made here is made to them all.
                        let before = state.editing.as_ref().and_then(|draft| draft.selected_object().cloned());
                        let text = before.as_ref().is_some_and(|object| object.as_text().is_some());
                        match state.inspector.tab {
                            InspectorTab::Own if text => text_tab(ui, state, theme),
                            InspectorTab::Own => kind_tab(ui, state),
                            InspectorTab::Box => box_tab(ui, state, theme),
                            InspectorTab::Animation => build_tab(ui, state),
                        }
                        if let (Some(before), Some(draft)) = (before, state.editing.as_mut()) {
                            draft.share_change(theme, &before);
                        }
                    }
                }
            }
            let used = content.min_rect().height();
            ui.allocate_rect(Rect::from_min_size(room.min, vec2(room.width(), TOP + used + BOTTOM)), Sense::hover());
        });
    });
    // Which of the panel's text fields has the keyboard now, if one has; not one elsewhere in the
    // window, such as the words being typed on the slide, which are drawn after the panel.
    let column = ui.max_rect();
    let focused = ui.memory(|memory| memory.focused()).filter(|_| ui.ctx().text_edit_focused());
    state.inspector.field_with_keys =
        focused.filter(|id| ui.ctx().read_response(*id).is_some_and(|field| column.contains(field.rect.center())));
    leave
}

/// The panel for a theme as a whole, shown in the Theme Editor when no box is selected.
fn theme_slide_panel(ui: &mut egui::Ui, state: &mut HomeState) {
    let mut end_look_asked = None;
    let end_looks = state.theme_end_looks.clone();
    let picture_request = &mut state.theme_picture_request;
    let Some(edit) = state.theme_edit.as_mut() else { return };
    let before = (edit.theme.name.clone(), edit.theme.background, edit.theme.background_picture.clone());
    ui.add_space(4.0);
    // The two names share a column as wide as the wider of them.
    let (name, size) = (t("inspector.theme.name", "Name"), t("inspector.theme.size", "Size"));
    let column = crate::studio::widest(ui, &[name, size], &FontId::proportional(12.5)).max(44.0);
    labelled_left(ui, column, name, |ui| {
        // As the window calls it (the built-in theme in the language set); typed back as shown,
        // the built-in theme keeps the app's name for it, so no translation is saved.
        let mut shown = crate::theme_store::shown_theme_name(&edit.theme).into_owned();
        if ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY)).changed() {
            edit.theme.name = crate::theme_store::typed_theme_name(edit.theme.id, &shown);
        }
    });
    labelled_left(ui, column, size, |ui| {
        ui.label(egui::RichText::new("1920 × 1080").size(12.5).color(INK))
            .hint(t("inspector.theme.size.hint", "Every slide is 1920 by 1080; other sizes are not made yet."));
    });
    rule(ui);
    ui.horizontal(|ui| {
        let mut filled = edit.theme.background.is_some();
        let words = t("inspector.theme.background", "Background");
        let hint = t("inspector.theme.background.hint", "A colour behind everything on the slide");
        if tick(ui, &mut filled, words).hint(hint).changed() {
            edit.theme.background = filled.then_some(Rgba { r: 20, g: 30, b: 60, a: 255 });
        }
        if let Some(background) = edit.theme.background.as_mut() {
            let hint = t("inspector.theme.background.colour", "Background colour");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| colour(ui, background, hint));
        }
    });
    rule(ui);
    // A picture behind every slide drawn with the theme, while no media is showing.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t("inspector.theme.picture", "Picture")).size(13.0).strong().color(INK)).hint(t(
            "inspector.theme.picture.hint",
            "A picture behind every slide drawn with this theme, while no media is showing",
        ));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if edit.theme.background_picture.is_some() {
                if ui.button(t("inspector.theme.picture.remove", "Remove")).clicked() {
                    edit.theme.background_picture = None;
                }
                if ui.button(t("inspector.theme.picture.change", "Change…")).clicked() {
                    *picture_request = true;
                }
            } else if ui
                .button(t("inspector.theme.picture.choose", "Choose…"))
                .hint(t("inspector.theme.picture.choose.hint", "Choose a picture from a file"))
                .clicked()
            {
                *picture_request = true;
            }
        });
    });
    rule(ui);
    // Song end: the theme's mark for the end of a song, in its colour, and the window that
    // changes it (`end_look`). Not on a Bible template: a passage has no such mark.
    let bible = edit.templates().get(edit.template).is_some_and(|template| template.bible);
    if !bible {
        let look = end_looks.get(edit.index).copied().unwrap_or_default();
        let mut change = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t("inspector.theme.song_end", "Song end")).size(13.0).strong().color(INK))
                .hint(t(
                    "inspector.theme.song_end.hint",
                    "The mark under the last slide of a song drawn with this theme, when Settings \u{203a} Slides has it on",
                ));
            let ink = look.colour.map_or(INK, crate::home::color32);
            let face = crate::studio::marks(ui.ctx(), 13.0);
            ui.label(egui::RichText::new(crate::end_look::chip_words(look.mark)).font(face).color(ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                change = ui.button(t("inspector.theme.song_end.change", "Change\u{2026}")).clicked();
            });
        });
        if change {
            end_look_asked = Some(edit.index);
        }
        rule(ui);
    }
    let about = t(
        "inspector.theme.about",
        "A song's slide uses, of the templates marked Songs, the one that suits what is on it: with the main language and a translation, the first with a box for both; with one language, the first with no translation box. Bible slides use the template marked Bible. Right-click a template to change what it is for. Songs and Bible slides are drawn with the main theme; right-click a theme to make it the main one.",
    );
    ui.label(egui::RichText::new(about).size(11.5).color(DIM));
    if before != (edit.theme.name.clone(), edit.theme.background, edit.theme.background_picture.clone()) {
        // A theme always has a name.
        if edit.theme.name.trim().is_empty() {
            edit.theme.name = before.0;
        }
        edit.changed = true;
    }
    if let Some(theme) = end_look_asked {
        crate::end_look::open(state, theme, crate::end_look::Door::Editor);
    }
}

/// A bold label on the left, in a column `column` wide, and a control filling the rest of the row.
fn labelled_left(ui: &mut egui::Ui, column: f32, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(column, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(column);
            ui.label(egui::RichText::new(label).size(12.5).strong().color(INK));
        });
        add(ui);
    });
}

fn nothing_selected(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(t("inspector.nothing", "Click something on the slide to select it."))
            .size(12.5)
            .color(Color32::from_gray(190)),
    );
    ui.label(
        egui::RichText::new(t(
            "inspector.nothing.how",
            "Drag to move it, drag a handle to resize it, press Delete to remove it. Arrow keys nudge.",
        ))
        .size(11.5)
        .color(DIM),
    );
}

/// The look of an object's tabs and the slide's panel: rows a key tall, their fields and menus
/// a step lighter than the panel, numbers in the fixed-width face.
fn tab_style(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(BETWEEN, ROWS_APART);
    ui.spacing_mut().interact_size.y = KEY;
    ui.style_mut().drag_value_text_style = egui::TextStyle::Monospace;
    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = WELL;
    visuals.widgets.inactive.weak_bg_fill = WELL;
    visuals.widgets.inactive.bg_fill = WELL;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(9);
    }
}

/// A song's slide with nothing picked: what the slide itself has. Its label, the template it is
/// drawn with, and going on to the next slide by itself (its notes are the footer's Slide
/// Notes). A prop has none of these.
fn slide_panel(ui: &mut egui::Ui, state: &mut HomeState) {
    let of_song = state.edit_kind() == Some(crate::home::EditKind::Slides);
    let mut state_request = None;
    let templates = state.song_templates.clone();
    let group = state.edited_slide_group.clone();
    let Some(draft) = state.editing.as_mut() else { return };
    if !of_song {
        return nothing_selected(ui);
    }
    tab_style(ui);
    let captions = [
        t("slides.menu.label", "Label"),
        t("slides.menu.template", "Template"),
        t("inspector.row.go_next", "Go to Next"),
        t("slides.menu.group", "Group"),
    ];
    let column = caption_column(ui, &captions);
    let room = control_room(column);
    // Group: the group the slide is in, with its colour, and Change…, which opens the groups
    // to choose from. The groups are the song's, not the slide's, so the change is asked of
    // the app as the slide's own Group menu asks it, and is a step of the Edit view's Undo.
    if let Some((name, colour)) = group {
        let mut chosen = None;
        row(ui, column, captions[3], |ui| {
            let change = t("inspector.slide.group.change", "Change\u{2026}");
            let key_wide = crate::studio::flat_button_width(ui, &[change], &word_key_font(), 13.0);
            let (dot, _) = ui.allocate_exact_size(vec2(10.0, KEY), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.5, crate::home::group_dot_colour(colour));
            let said = if name.is_empty() { t("slides.menu.label.none", "None") } else { name.as_str() };
            // The name has all the room there is, so Change… stands at the row's right end; a
            // name too long for it is cut short, and said whole under the pointer.
            let name_room = vec2(room - 10.0 - key_wide - 2.0 * BETWEEN, KEY);
            let (place, over) = ui.allocate_exact_size(name_room, Sense::hover());
            let mut job = egui::text::LayoutJob::simple_singleline(said.to_owned(), FontId::proportional(13.0), INK);
            job.wrap = egui::text::TextWrapping::truncate_at_width(place.width());
            let words = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
            let cut = words.elided;
            ui.painter().galley(pos2(place.min.x, place.center().y - words.size().y / 2.0), words, INK);
            if cut {
                over.full_name(said);
            }
            let key = word_key(ui, change, false, true).hint(t(
                "inspector.slide.group.hint",
                "This slide and the slides after it in its group become the group you choose",
            ));
            egui::Popup::menu(&key).show(|ui| {
                let title = t("inspector.slide.group.title", "This slide\u{2019}s group");
                ui.label(egui::RichText::new(title).size(11.5).color(DIM));
                ui.separator();
                // The names every Group menu offers, in the language set, and the slide's own when
                // it is another; the group it is in is ticked, named in whichever language.
                let names: Vec<&str> = crate::home::group_names().collect();
                let own = (!name.is_empty() && !names.iter().any(|have| sp_model::same_part(&name, have)))
                    .then_some(name.as_str());
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for offered in own.into_iter().chain(names.iter().copied()) {
                        let now = sp_model::same_part(&name, offered);
                        if ui.add(egui::Button::selectable(now, offered)).clicked() {
                            if !now {
                                chosen = Some(offered.to_owned());
                            }
                            ui.close();
                        }
                    }
                });
            });
        });
        if let Some(name) = chosen {
            state_request = Some((draft.at, crate::home::SlideRequest::Group(vec![draft.at.slide], name)));
        }
    }
    let before = (draft.slide.label.clone(), draft.slide.template, draft.slide.advance);
    let (mut label, mut template, mut advance) = before.clone();
    row(ui, column, captions[0], |ui| {
        let none = t("slides.menu.label.none", "None");
        let shown = label.as_deref().map_or(none, crate::home::slide_label_words).to_owned();
        egui::ComboBox::from_id_salt("slide label")
            .selected_text(shown)
            .width(room)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .show_ui(ui, |ui| {
                if ui.add(egui::Button::selectable(label.is_none(), none)).clicked() {
                    label = None;
                }
                // In the language set, and ticked whatever language the slide's label was given in.
                for offered in crate::home::SLIDE_LABELS {
                    let name = crate::i18n::tg(offered.0, offered.1);
                    let ticked = crate::i18n::every_wording(offered).any(|said| label.as_deref() == Some(said));
                    if ui.add(egui::Button::selectable(ticked, name)).clicked() {
                        label = Some(name.to_owned());
                    }
                }
            })
            .response
            .hint(t("quickhelp.edit.slide_label", "A label for the slide, such as Blank Slide or Instrumental."));
    });
    // Which of the theme's templates the slide is drawn with. Usual lets the theme pick the one
    // that suits what is on the slide.
    if !templates.is_empty() {
        row(ui, column, captions[1], |ui| {
            let usual = t("slides.menu.template.usual", "Usual (the theme picks)");
            let current = template.filter(|id| templates.iter().any(|(have, _)| have == id));
            let shown = templates
                .iter()
                .find(|(id, _)| Some(*id) == current)
                .map_or(usual.to_owned(), |(id, name)| crate::theme_store::template_name(*id, name).into_owned());
            egui::ComboBox::from_id_salt("slide template")
                .selected_text(shown)
                .width(room)
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    if ui.add(egui::Button::selectable(current.is_none(), usual)).clicked() {
                        template = None;
                    }
                    for (id, name) in &templates {
                        let shown = crate::theme_store::template_name(*id, name);
                        if ui.add(egui::Button::selectable(current == Some(*id), shown.as_ref())).clicked() {
                            template = Some(*id);
                        }
                    }
                })
                .response
                .hint(t("quickhelp.edit.slide_template", "Which of the theme\u{2019}s layouts this slide uses."));
        });
    }
    // Going on to the next slide by itself, after a time.
    row(ui, column, captions[2], |ui| {
        let after =
            |seconds: f32| tf("slides.menu.go_next.after", "After {n} Seconds", &[("n", &format!("{seconds:.0}"))]);
        let none = t("slides.menu.go_next.none", "None");
        let shown = advance.map_or(none.to_owned(), after);
        egui::ComboBox::from_id_salt("slide go to next")
            .selected_text(shown)
            .width(room)
            .show_ui(ui, |ui| {
                if ui.add(egui::Button::selectable(advance.is_none(), none)).clicked() {
                    advance = None;
                }
                for seconds in [3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0] {
                    if ui.add(egui::Button::selectable(advance == Some(seconds), after(seconds))).clicked() {
                        advance = Some(seconds);
                    }
                }
            })
            .response
            .hint(t("quickhelp.edit.slide_advance", "Go on to the next slide by itself after a time."));
    });
    if (label.clone(), template, advance) != before {
        draft.record();
        (draft.slide.label, draft.slide.template, draft.slide.advance) = (label, template, advance);
    }
    nothing_selected(ui);
    if state_request.is_some() {
        state.slide_request = state_request;
    }
}

/// True for a picture object whose file is a video, which plays in its box.
fn is_video_file(file: &str) -> bool {
    crate::media::is_video(std::path::Path::new(file))
}

/// The name of an object's own tab: what the object is.
fn own_tab_name(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Text { .. } => t("inspector.tab.text", "Text"),
        ObjectKind::Shape { .. } => t("inspector.tab.shape", "Shape"),
        ObjectKind::Image { file, .. } if is_video_file(file) => t("inspector.tab.video", "Video"),
        ObjectKind::Image { .. } => t("inspector.tab.picture", "Picture"),
        ObjectKind::Web { .. } => t("inspector.web", "Web Page"),
        ObjectKind::Ndi { .. } => t("inspector.ndi", "NDI Source"),
        ObjectKind::Mirror { .. } => t("inspector.tab.mirror", "AirPlay"),
        ObjectKind::Screen { .. } => t("inspector.tab.screen", "Screen"),
    }
}

/// What an object is, in the words of the line that says what several picked objects are.
fn kind_words(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Text { .. } => t("inspector.kind.text", "text box"),
        ObjectKind::Shape { .. } => t("inspector.kind.shape", "shape"),
        ObjectKind::Image { file, .. } if is_video_file(file) => t("inspector.kind.video", "video"),
        ObjectKind::Image { .. } => t("inspector.kind.picture", "picture"),
        ObjectKind::Web { .. } => t("inspector.kind.web", "web page"),
        ObjectKind::Ndi { .. } => t("inspector.kind.ndi", "NDI source"),
        ObjectKind::Mirror { .. } => t("inspector.kind.mirror", "AirPlay"),
        ObjectKind::Screen { .. } => t("inspector.kind.screen", "screen picture"),
    }
}

/// The tabs for what is picked, with their names: the object's own tab first (several objects
/// of one kind keep their kind's tab; of different kinds they have none), then Box, then
/// Animation where objects come on and go off: on slides and props, not on a theme's template
/// or a stage layout. Empty with nothing picked.
fn tabs_for(state: &HomeState) -> Vec<(InspectorTab, &'static str)> {
    let Some(draft) = state.editing.as_ref() else { return Vec::new() };
    let picked = draft.selection();
    let mut kinds = draft.slide.objects.iter().filter(|object| picked.contains(&object.id)).map(|object| &object.kind);
    let Some(first) = kinds.next() else { return Vec::new() };
    // A picture and a video are different kinds here, though one kind of object holds both.
    let one_kind = kinds.all(|kind| own_tab_name(kind) == own_tab_name(first));
    let mut tabs = Vec::with_capacity(3);
    if one_kind {
        tabs.push((InspectorTab::Own, own_tab_name(first)));
    }
    tabs.push((InspectorTab::Box, t("inspector.tab.box", "Box")));
    if state.theme_edit.is_none() && !crate::stage_editor::is_stage(draft) {
        tabs.push((InspectorTab::Animation, t("inspector.tab.animation", "Animation")));
    }
    tabs
}

/// A web page box's address: what is typed is kept while the field has the keyboard, and taken
/// when it is left (with https:// put in front of a bare name). An address that isn't https is
/// refused, and says so. A box just added hands the field the keyboard, so the address can be
/// typed at once; `address` says so, and for how long (see [`AddressField`]).
fn web_address(
    ui: &mut egui::Ui,
    object: sp_model::Uuid,
    url: &mut String,
    address: &mut Option<AddressField>,
    also: &[&str],
) -> f32 {
    let caption = t("inspector.row.address", "Address");
    let column = caption_column(ui, &[&[caption][..], also].concat());
    let id = ui.make_persistent_id(("web address", object));
    let bad_id = id.with("refused");
    let mut typed = ui.data_mut(|data| data.get_temp::<String>(id)).unwrap_or_else(|| url.clone());
    let mut field = None;
    row(ui, column, caption, |ui| {
        let edit = egui::TextEdit::singleline(&mut typed)
            .id(id.with("field"))
            .hint_text("https://…")
            .align(Align2::LEFT_CENTER);
        field = Some(ui.add_sized([control_room(column), KEY], edit));
    });
    let Some(field) = field else { return column };
    let has_keys = |ui: &egui::Ui| ui.memory(|memory| memory.has_focus(field.id));
    if ui.data_mut(|data| data.remove_temp::<bool>(egui::Id::unique("web address focus"))).is_some() {
        field.request_focus();
        *address = Some(AddressField { object, field: field.id, given: true });
    }
    // Typing in it, a click in it, or leaving it makes the field the person's: Delete,
    // Backspace and Undo work on its words from then on.
    if let Some(known) = address.as_mut().filter(|known| known.field == field.id)
        && (field.changed() || field.is_pointer_button_down_on() || !has_keys(ui))
    {
        known.given = false;
    }
    if field.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, typed));
    } else if field.lost_focus() {
        ui.data_mut(|data| data.remove::<String>(id));
        let typed = typed.trim();
        let typed =
            if !typed.is_empty() && !typed.contains("://") { format!("https://{typed}") } else { typed.to_owned() };
        let refused = !typed.is_empty() && !sp_video::web::allowed(&typed);
        if !refused {
            *url = typed;
        }
        ui.data_mut(|data| data.insert_temp(bad_id, refused));
    }
    if ui.data_mut(|data| data.get_temp::<bool>(bad_id)).unwrap_or(false) {
        let refused = t("inspector.web.refused", "Only web pages whose address starts with https:// can be shown.");
        line_of_words(ui, refused, crate::studio::TALLY);
    } else {
        let live = t("inspector.web.live", "The page shows, live, while the slide is on screen.");
        line_of_words(ui, live, crate::studio::TEXT_DIM);
    }
    column
}

/// How a picture sits in its box: a row with the three ways.
fn fits_row(ui: &mut egui::Ui, column: f32, salt: &str, fit: &mut sp_model::ImageFit) {
    row(ui, column, t("inspector.row.fits", "Fits"), |ui| {
        let fits: Vec<(sp_model::ImageFit, &str)> =
            sp_model::ImageFit::ALL.iter().map(|fit| (*fit, fit_name(*fit))).collect();
        choice(
            ui,
            (salt, t("quickhelp.inspector.fits", "How the picture fits its box.")),
            control_room(column),
            fit,
            &fits,
        );
    });
}

/// An NDI source box's settings, shaped like [`web_address`]: the Source menu (the sources the
/// finder sees, less this app's own outputs) with Look again beside it, one line saying what
/// happens, then how the picture fits, which stream is received and whether sound is wanted.
fn ndi_source(
    ui: &mut egui::Ui,
    object: sp_model::Uuid,
    (source, scaling, quality, sound): (&mut String, &mut sp_model::ImageFit, &mut sp_model::NdiQuality, &mut bool),
    view: &mut crate::ndi_in::NdiView,
    installed: bool,
    download: &mut crate::ndi_download::NdiDownload,
    also: &[&str],
) -> f32 {
    let captions = [
        t("inspector.row.source", "Source"),
        t("inspector.row.fits", "Fits"),
        t("inspector.ndi.quality", "Quality"),
        t("inspector.ndi.sound", "Sound"),
    ];
    let column = caption_column(ui, &[&captions[..], also].concat());
    let room = control_room(column);
    let stopped = !source.is_empty() && view.stopped.contains(source);
    let runtime = installed && !view.unavailable;
    row(ui, column, captions[0], |ui| {
        let shown = if source.is_empty() {
            t("inspector.ndi.choose", "Choose a source\u{2026}").to_owned()
        } else if stopped {
            tf(
                "inspector.ndi.menu_not_sending",
                "{name} \u{b7} not sending",
                &[("name", sp_model::ndi_short_name(source))],
            )
        } else {
            crate::ndi_in::menu_name(source)
        };
        // Look again is as wide as its words, a shorter word where the usual is too long; the
        // menu has the rest, a long name cut short.
        let (key, english) = tk("inspector.ndi.look_again", "Look again");
        let again = crate::studio::within(ui, key, english, word_key_font(), 0.0, 74.0, false);
        let again_wide = crate::studio::flat_button_width(ui, &[&again], &word_key_font(), 13.0);
        let chooser = egui::ComboBox::from_id_salt(("ndi source", object))
            .selected_text(shown)
            .width(room - again_wide - BETWEEN)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .show_ui(ui, |ui| {
                // The one chosen stays in the list while it is away.
                if !source.is_empty() && !view.sources.contains(source) {
                    let name = crate::ndi_in::menu_name(source);
                    ui.selectable_value(source, source.clone(), name);
                }
                for found in &view.sources {
                    ui.selectable_value(source, found.clone(), crate::ndi_in::menu_name(found));
                }
                if view.sources.is_empty() {
                    let none = t("inspector.ndi.none_found", "None found yet");
                    ui.label(egui::RichText::new(none).size(11.5).color(crate::studio::TEXT_DIM));
                }
            });
        chooser.response.clone().hint(t("quickhelp.inspector.ndi_source", "The NDI source this box shows."));
        if ui.data_mut(|data| data.remove_temp::<bool>(egui::Id::unique("ndi source focus"))).is_some() {
            chooser.response.request_focus();
        }
        let hint = t("inspector.ndi.look_again.hint", "Look for NDI sources on the network again");
        if word_key(ui, &again, false, true).hint(hint).clicked() {
            view.look_again = true;
        }
    });
    let (words, amber) = crate::ndi_in::line(runtime, view.sources.len(), source, stopped);
    line_of_words(ui, &words, if amber { crate::studio::CARE } else { crate::studio::TEXT_DIM });
    // Without NDI, the key that opens the Install NDI window.
    if !installed && ui.button(crate::ndi_download::install_key()).clicked() {
        download.open(crate::ndi_download::Place::Home);
    }
    fits_row(ui, column, "ndi fits", scaling);
    let quality_name = |q: sp_model::NdiQuality| match q {
        sp_model::NdiQuality::Full => t("inspector.ndi.quality.full", "Full"),
        sp_model::NdiQuality::Light => t("inspector.ndi.quality.light", "Light"),
    };
    let qualities: Vec<(sp_model::NdiQuality, &str)> =
        sp_model::NdiQuality::ALL.iter().map(|q| (*q, quality_name(*q))).collect();
    row(ui, column, captions[2], |ui| {
        choice(
            ui,
            ("ndi quality", t("quickhelp.inspector.ndi_quality", "The quality the NDI source is taken at.")),
            room,
            quality,
            &qualities,
        )
    });
    let light = t("inspector.ndi.quality.light.about", "Light is the sender\u{2019}s smaller picture");
    line_of_words(ui, light, crate::studio::TEXT_FAINT);
    let sounds = [(false, t("inspector.ndi.sound.off", "Off")), (true, t("inspector.ndi.sound.on", "On"))];
    row(ui, column, captions[3], |ui| {
        choice(
            ui,
            ("ndi sound", t("quickhelp.inspector.ndi_sound", "Whether the source\u{2019}s sound is wanted.")),
            room,
            sound,
            &sounds,
        )
    });
    let later = t("inspector.ndi.sound.later", "Sound comes later");
    ui.label(egui::RichText::new(later).size(12.0).color(crate::studio::TEXT_FAINT)).hint(t(
        "inspector.ndi.sound.later.hint",
        "The setting is kept. SoftPresenter has no way yet to play a source\u{2019}s sound.",
    ));
    column
}

/// Which of the fill's rows a tab has, for what is picked.
#[derive(Clone, Copy)]
struct FillRows {
    /// The colour is captioned Behind, not Fill: everything picked shows a picture.
    behind: bool,
    /// Gradient and To are there: nothing picked shows a picture, or one of them has a gradient
    /// already (from an older file), so nothing saved is out of reach.
    gradient: bool,
}

/// The rows for the colour inside a box or a shape: Fill (none, the swatches, any colour), then
/// its gradient's where it has them (see [`FillRows`]).
fn fill_rows(ui: &mut egui::Ui, column: f32, object: &mut sp_model::Object, rows: FillRows) {
    let room = control_room(column);
    let look = Swatches { salt: "fill", room, count: 8, none_too: true, mixed: None };
    // Behind a picture, the colour shows in the bars beside it and while it hasn't come:
    // "Fill" there would read as how the picture fits.
    let caption = if rows.behind { t("inspector.row.behind", "Behind") } else { t("inspector.shape.fill", "Fill") };
    swatches_row(ui, column, caption, &look, |ui, look| {
        // A colour chosen for a box that had none starts part see-through over what is behind
        // it, as a new fill always has.
        let had = object.fill.is_some();
        if swatch_row(ui, look, &mut object.fill)
            && !had
            && let Some(fill) = object.fill.as_mut()
        {
            fill.a = 160;
        }
    });
    if !rows.gradient {
        return;
    }
    // Drawing changes nothing: the gradient is written back only when a control changed it.
    let mut gradient = object.gradient.unwrap_or_default();
    let before = gradient;
    gradient_rows(ui, column, "fill gradient", object.fill.is_some(), &mut gradient);
    if gradient != before {
        object.gradient = gradient.on().then_some(gradient);
    }
}

/// The row for the line round a box or a shape: its width (none at 0), then its colour.
fn outline_row(ui: &mut egui::Ui, column: f32, object: &mut sp_model::Object) {
    let room = control_room(column);
    let look = Swatches { salt: "outline", room: room - NUMBER - BETWEEN, count: 5, none_too: false, mixed: None };
    row(ui, column, t("inspector.row.outline", "Outline"), |ui| {
        let mut line = object.stroke.unwrap_or(sp_model::StrokeLook { width: 0.0, color: Rgba::WHITE });
        let before = line;
        number(ui, &mut line.width, (NUMBER, ""), |d| d.speed(0.2).range(0.0..=100.0).max_decimals(1))
            .hint(t("inspector.shape.stroke.hint", "Draw a line round the box or shape"));
        swatches(ui, &look, &mut line.color);
        if line != before {
            // A colour chosen while there is no line draws one: 3 points to start with.
            if before.width == 0.0 && line.width == 0.0 {
                line.width = 3.0;
            }
            object.stroke = (line.width > 0.0).then_some(line);
        }
    });
}

/// How wide the line round a box is when Outline is switched on, in slide units.
const BOX_OUTLINE: f32 = 4.0;

/// The shadow a box or a shape starts with: 75 % black, falling down and to the right.
const BOX_SHADOW: sp_model::Shadow =
    sp_model::Shadow { color: Rgba::BLACK, opacity: 0.75, angle: 315.0, offset: 5.0, blur: 5.0 };

/// The room between the six keys that line an object up, [`KEY_WIDE`] each, across `room`:
/// [`BETWEEN`] where they fit so, and less where they don't.
fn line_up_gap(room: f32) -> f32 {
    ((room - 6.0 * KEY_WIDE) / 5.0).floor().clamp(1.0, BETWEEN)
}

/// Three bars with the same room between them: spread out left to right (`across`), or top to
/// bottom.
fn spread_mark(painter: &egui::Painter, cell: Rect, across: bool, ink: Color32) {
    let c = cell.center();
    for step in [-6.0, 0.0, 6.0] {
        let bar = if across {
            Rect::from_center_size(pos2(c.x + step, c.y), vec2(2.5, 12.0))
        } else {
            Rect::from_center_size(pos2(c.x, c.y + step), vec2(12.0, 2.5))
        };
        painter.rect_filled(bar, 1.0, ink);
    }
}

/// A triangle standing on a line: mirror left to right (`across`), or top to bottom.
fn flip_mark(painter: &egui::Painter, cell: Rect, across: bool, ink: Color32) {
    let c = cell.center();
    let (a, b) = if across { (vec2(-6.0, 4.0), vec2(6.0, 4.0)) } else { (vec2(-4.0, -6.0), vec2(-4.0, 6.0)) };
    let tip = if across { vec2(0.0, -5.0) } else { vec2(5.0, 0.0) };
    painter.add(egui::Shape::convex_polygon(vec![c + a, c + b, c + tip], ink, Stroke::NONE));
}

/// The captions of the rows for the colour behind a box that shows a picture.
fn behind_captions(rows: FillRows) -> Vec<&'static str> {
    let mut captions = vec![t("inspector.row.behind", "Behind")];
    if rows.gradient {
        captions.extend([t("inspector.row.gradient", "Gradient"), t("inspector.row.to", "To")]);
    }
    captions
}

/// The object's own tab for everything but text: what makes it what it is. For a shape that
/// is which shape, its fill and its outline. A box that shows a picture (a picture, a video, a
/// web page, an NDI source, AirPlay, a screen picture) ends with Behind: the colour of the box
/// itself, which shows in the bars beside the picture and while it hasn't come.
fn kind_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    let stage = state.editing.as_ref().is_some_and(crate::stage_editor::is_stage) && state.theme_edit.is_none();
    let screens = if stage { state.screens.clone() } else { Vec::new() };
    let address = &mut state.inspector.address;
    let gradient_rows_for = &mut state.inspector.gradient_rows_for;
    let (ndi_view, ndi_installed, ndi_download) = (&mut state.ndi, state.ndi_available, &mut state.ndi_download);
    let mirror_view = &state.mirror;
    let Some(draft) = state.editing.as_mut() else { return nothing_selected(ui) };
    // Gradient and To are there only for a box saved with a gradient (by an older version), so
    // nothing saved is out of reach. Whether they show is settled when the boxes are picked,
    // not as they are changed: a gradient set to None here leaves its rows where they are.
    let picked = draft.selection();
    if draft.slide.objects.iter().any(|object| picked.contains(&object.id) && object.gradient.is_some()) {
        gradient_rows_for.clone_from(&picked);
    }
    let behind = FillRows { behind: true, gradient: *gradient_rows_for == picked };
    let also = behind_captions(behind);
    let Some(object) = draft.selected_object_mut() else {
        return nothing_selected(ui);
    };
    let object_id = object.id;
    // A screen picture: which screen it shows, its corners and its label.
    if matches!(object.kind, ObjectKind::Screen { .. }) {
        crate::stage_editor::screen_settings(ui, object, &screens);
        let column = caption_column(ui, &also);
        return fill_rows(ui, column, object, behind);
    }
    if let ObjectKind::Shape { shape } = &mut object.kind {
        let captions = [
            t("inspector.tab.shape", "Shape"),
            t("inspector.shape.fill", "Fill"),
            t("inspector.row.gradient", "Gradient"),
            t("inspector.row.to", "To"),
            t("inspector.row.outline", "Outline"),
        ];
        let column = caption_column(ui, &captions);
        // Which shape it is. One still called what it was takes the new shape's name.
        let was = *shape;
        row(ui, column, captions[0], |ui| {
            let shapes: Vec<(sp_model::Shape, &str)> =
                sp_model::Shape::ALL.into_iter().map(|shape| (shape, crate::editor::shape_name(shape))).collect();
            choice(
                ui,
                (
                    "which shape",
                    t(
                        "quickhelp.inspector.shape_kind",
                        "Make this shape another, keeping its place, size and colours.",
                    ),
                ),
                control_room(column),
                shape,
                &shapes,
            );
        });
        let now = *shape;
        if now != was && object.name == crate::editor::shape_name(was) {
            crate::editor::shape_name(now).clone_into(&mut object.name);
        }
        fill_rows(ui, column, object, FillRows { behind: false, gradient: true });
        outline_row(ui, column, object);
        return;
    }
    let mut video = false;
    let column = match &mut object.kind {
        ObjectKind::Text { .. } | ObjectKind::Shape { .. } | ObjectKind::Screen { .. } => return,
        // A picture: how it sits in its place.
        ObjectKind::Image { file, fit } => {
            let captions =
                [&[t("inspector.row.file", "File"), t("inspector.row.fits", "Fits")][..], &also[..]].concat();
            let column = caption_column(ui, &captions);
            // The file by its name alone, cut short where it is long and whole under the pointer.
            let name = std::path::Path::new(file.as_str()).file_name().map(|name| name.to_string_lossy().into_owned());
            let name = name.unwrap_or_else(|| file.clone());
            row(ui, column, captions[0], |ui| {
                let words = egui::RichText::new(name.as_str()).size(12.5).color(INK);
                let room = vec2(control_room(column), KEY);
                ui.allocate_ui_with_layout(room, Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(words).truncate()).hint(name.as_str());
                });
            });
            fits_row(ui, column, "picture fit", fit);
            video = is_video_file(file);
            column
        }
        // A web page: its address. The page is laid out at the box's size.
        ObjectKind::Web { url } => web_address(ui, object_id, url, address, &also),
        // An NDI source: which one, and how its picture is received and shown.
        ObjectKind::Ndi { source, scaling, quality, sound } => {
            let ndi = (source, scaling, quality, sound);
            ndi_source(ui, object_id, ndi, ndi_view, ndi_installed, ndi_download, &also)
        }
        // AirPlay: who is showing a screen, and how its picture sits in the box.
        ObjectKind::Mirror { scaling } => {
            let ink = if mirror_view.on { crate::studio::TEXT_DIM } else { crate::studio::CARE };
            line_of_words(ui, &crate::mirror::line(mirror_view), ink);
            let column = caption_column(ui, &[&[t("inspector.row.fits", "Fits")][..], &also[..]].concat());
            fits_row(ui, column, "mirror fits", scaling);
            column
        }
    };
    fill_rows(ui, column, object, behind);
    if video {
        let plays = t("inspector.video.plays", "It plays from the start while the slide is on screen.");
        line_of_words(ui, plays, crate::studio::TEXT_DIM);
    }
}

/// The Box tab, the same seven rows for every kind of object: Place, Align, Rotate with the two
/// flips beside it, Opacity with its number, Effects (Outline and Shadow, each a switch with
/// the key to its look; a shape's outline is on its own tab), Order and Object. The colour in a
/// box is not here: a text box's is the Text tab's Behind, a shape's its own tab's Fill, and
/// one that shows a picture has Behind on its own tab.
fn box_tab(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) {
    let stage = state.editing.as_ref().is_some_and(crate::stage_editor::is_stage) && state.theme_edit.is_none();
    let for_theme = state.theme_edit.is_some() || stage;
    // Undo and Redo here take the Edit view's steps in turn, as Command-Z does.
    let history = (state.can_undo(false), state.can_undo(true));
    let menu_asked = &mut state.menu_asked;
    let lock_shape = &mut state.inspector.lock_shape;
    let panel = &mut state.inspector.panel;
    let rename_started = &mut state.rename_started;
    let Some(draft) = state.editing.as_mut() else { return };
    let picked = draft.selection();
    // With several picked: how many, and what they are.
    let mut kinds: Vec<String> = Vec::new();
    // A shape's outline is on its own tab, so with a shape among them Effects has Shadow alone.
    let any_shape = draft
        .slide
        .objects
        .iter()
        .any(|object| picked.contains(&object.id) && matches!(object.kind, ObjectKind::Shape { .. }));
    for object in draft.slide.objects.iter().filter(|object| picked.contains(&object.id)) {
        let kind = kind_words(&object.kind).to_owned();
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let themed = draft.selected_object().map(|object| object_frame(&draft.slide, object, theme));
    let (Some(frame), Some(object)) = (themed, draft.selected_object_mut()) else {
        return nothing_selected(ui);
    };
    let captions = [
        t("inspector.row.place", "Place"),
        t("inspector.row.align", "Align"),
        t("inspector.row.rotate", "Rotate"),
        t("inspector.shape.opacity", "Opacity"),
        t("inspector.row.effects", "Effects"),
        t("inspector.row.order", "Order"),
        t("inspector.row.object", "Object"),
    ];
    let column = caption_column(ui, &captions);
    let room = control_room(column);
    let [place, arrange, rotate, opacity, effects, order, named] = captions;
    if picked.len() > 1 {
        let count = crate::i18n::tn("inspector.several", picked.len() as u64, "{n} object", "{n} objects", &[]);
        scope_line(ui, true, &count, &kinds.join(", "));
    }
    let mut edited = frame;

    row_tall(ui, column, place, MINI, |ui| {
        // The sign that keeps the box's shape stands between its width and its height.
        let lock_wide = 12.0;
        let each = mini_width(room - lock_wide - BETWEEN, 4);
        let shape = frame.w / frame.h.max(1.0);
        mini_named(
            ui,
            (
                each,
                t(
                    "quickhelp.inspector.x",
                    "How far the box is from the slide\u{2019}s left edge, in the slide\u{2019}s pixels.",
                ),
            ),
            "X",
            &mut edited.x,
            |d| d,
        );
        mini_named(
            ui,
            (
                each,
                t(
                    "quickhelp.inspector.y",
                    "How far the box is from the slide\u{2019}s top edge, in the slide\u{2019}s pixels.",
                ),
            ),
            "Y",
            &mut edited.y,
            |d| d,
        );
        let wide = t("edit.status.w", "W").to_uppercase();
        mini_named(
            ui,
            (each, t("quickhelp.inspector.width", "How wide the box is, in the slide\u{2019}s pixels.")),
            &wide,
            &mut edited.w,
            |d| d.range(MIN_SIDE..=10_000.0),
        );
        let lock = ui
            .add_sized(
                [lock_wide, KEY],
                egui::Button::selectable(*lock_shape, egui::RichText::new("=").size(11.0)).frame(false),
            )
            .hint(t("inspector.shape.lock.hint", "Keep the box's shape when its width or height changes"));
        if lock.clicked() {
            *lock_shape = !*lock_shape;
        }
        let high = t("edit.status.h", "H").to_uppercase();
        mini_named(
            ui,
            (each, t("quickhelp.inspector.height", "How tall the box is, in the slide\u{2019}s pixels.")),
            &high,
            &mut edited.h,
            |d| d.range(MIN_SIDE..=10_000.0),
        );
        if *lock_shape {
            if edited.w != frame.w {
                edited.h = (edited.w / shape).max(MIN_SIDE);
            } else if edited.h != frame.h {
                edited.w = (edited.h * shape).max(MIN_SIDE);
            }
        }
    });

    // Line the object up with the slide's sides or middle; with three or more picked, spread
    // them out evenly too, which is done once this object is let go of.
    let mut spread: Option<bool> = None;
    row(ui, column, arrange, |ui| {
        // Six keys at the design's width in every language: where a long caption leaves them
        // less room, they stand closer together, never narrower.
        ui.spacing_mut().item_spacing.x = line_up_gap(room);
        let hints = [
            t("inspector.arrange.left", "Line up with the left of the slide"),
            t("inspector.arrange.centre", "Centre across the slide"),
            t("inspector.arrange.right", "Line up with the right of the slide"),
            t("inspector.arrange.top", "Line up with the top of the slide"),
            t("inspector.arrange.middle", "Centre down the slide"),
            t("inspector.arrange.bottom", "Line up with the bottom of the slide"),
        ];
        for (which, hint) in hints.into_iter().enumerate() {
            let key = square_key(ui, KEY_WIDE, (false, false), hint, |p, cell, ink| align_mark(p, cell, which, ink));
            if key.clicked() {
                match which {
                    0 => edited.x = 0.0,
                    1 => edited.x = (SLIDE_WIDTH - edited.w) / 2.0,
                    2 => edited.x = SLIDE_WIDTH - edited.w,
                    3 => edited.y = 0.0,
                    4 => edited.y = (SLIDE_HEIGHT - edited.h) / 2.0,
                    _ => edited.y = SLIDE_HEIGHT - edited.h,
                }
            }
        }
    });
    // With three or more picked, the two keys that spread them out evenly, on a line of their
    // own under the six.
    if picked.len() > 2 {
        row(ui, column, "", |ui| {
            ui.spacing_mut().item_spacing.x = line_up_gap(room);
            for (across, hint) in [
                (true, t("inspector.arrange.spread_across", "Spread them out evenly, left to right")),
                (false, t("inspector.arrange.spread_down", "Spread them out evenly, top to bottom")),
            ] {
                let key =
                    square_key(ui, KEY_WIDE, (false, false), hint, |p, cell, ink| spread_mark(p, cell, across, ink));
                if key.clicked() {
                    spread = Some(across);
                }
            }
        });
    }
    if edited != frame {
        object.frame = Some(edited);
    }

    row(ui, column, rotate, |ui| {
        number(
            ui,
            &mut object.rotation,
            (NUMBER, t("quickhelp.inspector.rotation", "Turn the box, in degrees.")),
            |d| d.speed(0.5).range(-360.0..=360.0).suffix("°").max_decimals(1),
        );
        // After the angle: mirror it left to right, or top to bottom.
        for (across, hint) in [
            (true, t("inspector.flip.across", "Mirror it left to right")),
            (false, t("inspector.flip.down", "Mirror it top to bottom")),
        ] {
            let on = if across { object.flip_h } else { object.flip_v };
            if square_key(ui, KEY_WIDE, (on, false), hint, |p, cell, ink| flip_mark(p, cell, across, ink)).clicked() {
                if across {
                    object.flip_h = !on;
                } else {
                    object.flip_v = !on;
                }
            }
        }
    });

    row(ui, column, opacity, |ui| {
        // How solid the object is: the slider, and beside it the percentage, which can be typed.
        ui.spacing_mut().slider_width = room - BETWEEN - PERCENT_BOX;
        ui.spacing_mut().interact_size.x = PERCENT_BOX;
        // It shows a whole percentage, and an opacity in between is kept until it is changed.
        let shown = (object.opacity * 100.0).round();
        let mut percent = shown;
        let hint = t("quickhelp.inspector.opacity", "How solid the box is. At 100% nothing shows through it.");
        if percent_slider(ui, &mut percent, hint) && percent != shown {
            object.opacity = (percent / 100.0).clamp(0.0, 1.0);
        }
    });

    // Effects: Outline and Shadow, each a switch with the key to its look beside it.
    let (outlined, shadowed) = (object.stroke.is_some(), object.shadow.is_some());
    let effects = row(ui, column, effects, |ui| {
        if !any_shape {
            let outline = (t("inspector.row.outline", "Outline"), outlined);
            let hint = t("inspector.outline.more", "Outline: width and colour");
            if switch_with_more(ui, panel, Panel::BoxOutline, outline, hint) {
                object.stroke = (!outlined).then_some(sp_model::StrokeLook { width: BOX_OUTLINE, color: Rgba::WHITE });
            }
        }
        let shadow = (t("inspector.shadow", "Shadow"), shadowed);
        let hint = t("inspector.shadow.more", "Shadow: angle, offset, blur, opacity and colour");
        if switch_with_more(ui, panel, Panel::BoxShadow, shadow, hint) {
            object.shadow = (!shadowed).then_some(BOX_SHADOW);
        }
    });
    let title = t("inspector.row.outline", "Outline");
    panel_over(ui, panel, (Panel::BoxOutline, false), effects, title, |ui| {
        let captions = [t("inspector.stroke.width", "Width"), t("inspector.row.colour", "Colour")];
        let column = caption_column(ui, &captions);
        let room = panel_room(column);
        let mut line = object.stroke.unwrap_or(sp_model::StrokeLook { width: 0.0, color: Rgba::WHITE });
        let before = line;
        row(ui, column, captions[0], |ui| {
            number(ui, &mut line.width, (NUMBER, ""), |d| d.speed(0.2).range(0.0..=100.0).max_decimals(1))
                .hint(t("inspector.shape.stroke.hint", "Draw a line round the box or shape"));
        });
        let colours = Swatches { salt: "box outline", room, count: 8, none_too: false, mixed: None };
        swatches_row(ui, column, captions[1], &colours, |ui, look| swatches(ui, look, &mut line.color));
        if line != before {
            // A colour chosen while there is no line draws one, as wide as the switch makes it.
            if before.width == 0.0 && line.width == 0.0 {
                line.width = BOX_OUTLINE;
            }
            object.stroke = (line.width > 0.0).then_some(line);
        }
    });
    let title = t("inspector.shadow", "Shadow");
    panel_over(ui, panel, (Panel::BoxShadow, false), effects, title, |ui| {
        // Without a shadow its rows are there, greyed: a box keeps no look for one it hasn't got.
        let on = object.shadow.is_some();
        let mut look = object.shadow.unwrap_or(BOX_SHADOW);
        shadow_panel(ui, "box shadow", on, &mut look);
        if on {
            object.shadow = Some(look);
        } else {
            panel_note(ui, t("inspector.box_shadow.off", "Shadow is off. Switch it on to set its look."));
        }
    });

    let mut restack: Option<bool> = None;
    row(ui, column, order, |ui| {
        if word_key(ui, t("inspector.order.forward", "Forward"), false, true)
            .hint(t("inspector.forward.hint", "Move in front of the next object"))
            .clicked()
        {
            restack = Some(true);
        }
        if word_key(ui, t("inspector.order.back", "Back"), false, true)
            .hint(t("inspector.backward.hint", "Move behind the previous object"))
            .clicked()
        {
            restack = Some(false);
        }
    });

    // Rename starts retyping the name where it is listed, under Objects, as a double click on
    // it there does.
    let (mut delete, mut duplicate, mut rename) = (false, false, false);
    row(ui, column, named, |ui| {
        // Their names where the three fit the row at the usual size; where they don't, in a
        // language with longer words, all three are pictures that say their names under the
        // pointer. Never two lines, and never some of each.
        let rename_hint = t("inspector.rename.hint", "Retype its name in the Objects list");
        let duplicate_hint =
            crate::keys::hint(t("inspector.duplicate.hint", "Make a copy of it beside it"), "D", false);
        let delete_hint = t("inspector.delete.hint", "Remove this object from the slide (Delete)");
        let keys: [(&str, &str, crate::icons::Icon); 3] = [
            (t("inspector.rename", "Rename"), rename_hint, crate::icons::Icon::Pencil),
            (t("inspector.duplicate", "Duplicate"), duplicate_hint.as_str(), crate::icons::Icon::Stack),
            (t("inspector.delete", "Delete"), delete_hint, crate::icons::Icon::Trash),
        ];
        // A name is one object's: Rename is greyed while several are picked.
        let one = picked.len() == 1;
        let several = t("inspector.rename.several", "Pick one object to rename it");
        let mut pressed = [false; 3];
        if object_keys_named(ui, room) {
            for (at, (words, hint, _)) in keys.into_iter().enumerate() {
                let can = at != 0 || one;
                pressed[at] = word_key(ui, words, false, can).hint_or_greyed(hint, (!can).then_some(several)).clicked();
            }
        } else {
            for (at, (words, hint, icon)) in keys.into_iter().enumerate() {
                let can = at != 0 || one;
                ui.add_enabled_ui(can, |ui| {
                    let key = square_key(ui, KEY_WIDE, (false, false), "", |painter, cell, ink| {
                        let glyph = Rect::from_center_size(cell.center(), vec2(14.0, 14.0));
                        crate::icons::paint(painter, icon, glyph, ink);
                    });
                    // Its name always; what it does in its place while Quick Help is on.
                    let key = key.name_or_hint(words, hint);
                    key.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can, words));
                    pressed[at] = key.reason(several).clicked();
                });
            }
        }
        [rename, duplicate, delete] = pressed;
    });
    let renamed = (object.id, crate::editor::object_name(object));
    let follows_theme =
        object.frame.is_none() && object.rotation == 0.0 && object.opacity == 1.0 && object.fill.is_none();

    let mut reset = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(FOOT_APART, FOOT_APART);
        for (words, redo, hint) in [
            (
                t("inspector.undo", "Undo"),
                false,
                crate::keys::hint(t("inspector.undo.hint", "Undo the last change"), "Z", false),
            ),
            (t("inspector.redo", "Redo"), true, crate::keys::hint(t("inspector.redo.hint", "Redo it"), "Z", true)),
        ] {
            let can = if redo { history.1 } else { history.0 };
            if word_key(ui, words, false, can)
                .hint_or_greyed(hint, (!can).then(|| crate::home::nothing_to(redo)))
                .clicked()
            {
                use crate::menubar::Action;
                *menu_asked = Some(if redo { Action::Redo } else { Action::Undo });
            }
        }
        if !for_theme {
            let words = t("inspector.reset_to_theme", "Reset to theme");
            let hint = t("inspector.reset_to_theme.hint", "Put this object back where the theme has it");
            reset = word_key(ui, words, false, !follows_theme).hint(hint).clicked();
        }
    });
    if let Some(across) = spread {
        draft.distribute(theme, across);
    }
    if let Some(forward) = restack {
        draft.restack(forward);
    }
    if reset {
        draft.reset_to_theme();
    }
    if rename {
        draft.renaming = Some(renamed);
        *rename_started = true;
    }
    if duplicate {
        let copies = draft.copied(theme);
        draft.paste(&copies);
    }
    if delete {
        draft.delete_selected();
    }
}

/// Whether the Box tab's Rename, Duplicate and Delete have their names: where the three fit one
/// line across `room` at the usual size, in the language set. Otherwise all three are pictures.
fn object_keys_named(ui: &egui::Ui, room: f32) -> bool {
    let names =
        [t("inspector.rename", "Rename"), t("inspector.duplicate", "Duplicate"), t("inspector.delete", "Delete")];
    let wide: f32 =
        names.iter().map(|name| crate::studio::flat_button_width(ui, &[name], &word_key_font(), 13.0)).sum();
    wide + 2.0 * BETWEEN <= room
}

/// How far apart the keys at the foot of a tab are, side by side and where they wrap.
const FOOT_APART: f32 = 6.0;

fn text_tab(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) {
    let stage = state.editing.as_ref().is_some_and(crate::stage_editor::is_stage) && state.theme_edit.is_none();
    let for_theme = state.theme_edit.is_some();
    let fonts = state.fonts.clone();
    let timers: Vec<(sp_model::Uuid, String)> =
        state.timers.iter().map(|timer| (timer.id, timer.name.clone())).collect();
    let inspector = &mut state.inspector;
    let Some(draft) = state.editing.as_mut() else { return };
    ui.spacing_mut().item_spacing.y = TEXT_ROWS_APART;
    let words_selected = draft.selected_words();
    let boxes = draft.selection().len();
    let taken_roles: Vec<sp_model::Role> =
        draft.slide.objects.iter().filter_map(|object| object.role.clone()).collect();
    let Some(object) = draft.selected_object_mut() else {
        return nothing_selected(ui);
    };
    // A stage item: what it shows, and that item's own settings (which timer, the clock's form).
    let stage_item = stage && crate::stage::item_of(object).is_some();
    if stage_item {
        crate::stage_editor::item_settings(ui, object, &timers);
    }
    let role = object.role.clone();
    let object_id = object.id;
    let linked = object.link.is_some();
    let ObjectKind::Text { text, style, spans } = &mut object.kind else { return };
    let Some(base) = theme.text_style(role.as_ref(), &TextStyle::default()) else { return };
    let picked = words_selected.filter(|(id, ..)| *id == object_id);
    // What the rows below change: the words picked, or the whole box.
    match picked {
        Some((_, from, to)) => {
            let words: String = text.chars().skip(from).take(to - from).collect::<String>().replace('\n', " ");
            let words = if words.chars().count() > 26 {
                crate::i18n::quoted(&format!("{}\u{2026}", words.chars().take(25).collect::<String>().trim_end()))
            } else {
                crate::i18n::quoted(&words)
            };
            scope_line(ui, !stage_item, t("inspector.text.selected", "SELECTED WORDS"), &words);
        }
        None if boxes > 1 => {
            let count = crate::i18n::tn("inspector.text.boxes", boxes as u64, "{n} box", "{n} boxes", &[]);
            scope_line(ui, !stage_item, &count, "");
        }
        None => {
            let note =
                if linked { "" } else { t("inspector.text.whole_box.note", "Select words in it to change only them") };
            scope_line(ui, !stage_item, t("inspector.text.whole_box", "THE WHOLE BOX"), note);
        }
    }
    // The words are typed on the slide itself (double-click the box), not here.

    let shows = !for_theme && !stage_item;
    let mut captions = vec![
        t("inspector.row.font", "Font"),
        t("inspector.row.size", "Size"),
        t("inspector.row.style", "Style"),
        t("inspector.row.colour", "Colour"),
        t("inspector.row.align", "Align"),
        t("inspector.row.effects", "Effects"),
        t("inspector.row.spacing", "Spacing"),
        t("inspector.row.fitting", "Fitting"),
        t("inspector.row.words", "Words"),
        t("inspector.row.behind", "Behind"),
    ];
    if shows {
        captions.push(t("inspector.row.shows", "Shows"));
    }
    if for_theme && boxes == 1 {
        captions.push(t("inspector.row.holds", "Holds"));
    }
    let column = caption_column(ui, &captions);

    // With words selected on the slide the controls show, and change, those words; with none,
    // the whole box.
    let box_look = style.over(&base);
    let (shown, mix) = match picked {
        Some((_, from, to)) => sp_model::look_of_range(spans, from, to, &box_look),
        None => sp_model::look_of_range(spans, 0, text.chars().count(), &box_look),
    };
    let mut wanted = shown.clone();
    text_look(ui, column, &mut wanted, &base, &fonts, inspector, (mix, picked.is_some()));
    if wanted != shown {
        let length = text.chars().count();
        match picked {
            Some((_, from, to)) => {
                sp_model::restyle_range(spans, from, to, length, &box_look, |look| look.follow_change(&shown, &wanted));
                // What a word cannot have (alignment, shadow) goes to the box.
                let mut whole = wanted.clone();
                for_each_span_setting(&mut whole, &box_look);
                *style = TextStyle::between(&base, &whole);
                // The words stay selected: the field takes the keyboard back.
                inspector.refocus_typing = true;
            }
            None => {
                // Every word takes what was set for the box; what the controls did not change
                // stays as it was.
                let mut whole = wanted.clone();
                for_each_span_setting(&mut whole, &box_look);
                crate::editor::apply_to_whole_box(spans, length, &shown, &wanted, &mut whole);
                *style = TextStyle::between(&base, &whole);
            }
        }
        // A switch or a choice is a step of its own; dragging a size or a colour is one run.
        inspector.new_step = wanted.weight != shown.weight
            || wanted.italic != shown.italic
            || wanted.underline != shown.underline
            || wanted.strike != shown.strike
            || wanted.font_family != shown.font_family;
    }

    // A− and A+: every size moves by the same step, so the differences stay. With words
    // picked only those words move; with none the box's size and every word's together.
    let step = std::mem::take(&mut inspector.size_step);
    if step != 0.0 {
        let length = text.chars().count();
        let box_look = style.over(&base);
        let stepped = |size: f32| (size + step).clamp(8.0, 600.0);
        match picked {
            Some((_, from, to)) => {
                sp_model::restyle_range(spans, from, to, length, &box_look, |look| look.size = stepped(look.size));
                inspector.refocus_typing = true;
            }
            None => {
                let mut whole = box_look.clone();
                whole.size = stepped(box_look.size);
                *style = TextStyle::between(&base, &whole);
                for span in spans.iter_mut() {
                    if let Some(size) = span.style.size {
                        span.style.size = Some(stepped(size));
                    }
                }
            }
        }
        inspector.new_step = true;
    }

    // Behind: the colour of the box itself, behind its words.
    behind_row(ui, column, &mut inspector.panel, &mut object.fill, &mut object.gradient);

    // Shows: the box's own words, or something that changes, such as the time, in their place.
    if shows {
        let timer_name =
            |id: sp_model::Uuid| timers.iter().find(|(have, _)| *have == id).map(|(_, name)| name.as_str());
        let mut link = object.link;
        let room = control_room(column);
        row(ui, column, t("inspector.row.shows", "Shows"), |ui| {
            let own = t("inspector.shows.own", "Its own words");
            let shown = match link {
                None => own.to_owned(),
                Some(sp_model::TextLink::Timer { id }) => {
                    let name = timer_name(id).unwrap_or(t("inspector.link.gone", "(gone)"));
                    let full = tf("inspector.link.timer", "Timer: {name}", &[("name", name)]);
                    // Where those words would run past the menu (less its padding and arrow),
                    // the short form: the timer's name alone.
                    if crate::studio::widest(ui, &[full.as_str()], &FontId::proportional(13.0)) <= room - 40.0 {
                        full
                    } else {
                        crate::i18n::fill(crate::i18n::ts("inspector.link.timer", "Timer: {name}"), &[("name", name)])
                    }
                }
                Some(other) => link_name(other).to_owned(),
            };
            egui::ComboBox::from_id_salt("linked text")
                .selected_text(shown)
                .width(room)
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    if ui.add(egui::Button::selectable(link.is_none(), own)).clicked() {
                        link = None;
                    }
                    for option in sp_model::TextLink::SIMPLE {
                        if ui.add(egui::Button::selectable(link == Some(option), link_name(option))).clicked() {
                            link = Some(option);
                        }
                    }
                    for (id, name) in &timers {
                        let option = sp_model::TextLink::Timer { id: *id };
                        let words = timer_link_name(name);
                        if ui.add(egui::Button::selectable(link == Some(option), words)).clicked() {
                            link = Some(option);
                        }
                    }
                })
                .response
                .hint(t(
                    "inspector.text.linked.hint",
                    "Show something that changes, such as the time or a timer, instead of these words",
                ));
        });
        if link != object.link {
            // The box's own words say what it is linked to, so its picture in the editor does too.
            let placeholder = text.is_empty()
                || crate::editor::is_sample_words(text)
                || (text.starts_with('[') && text.ends_with(']'));
            match link {
                Some(chosen) if placeholder => {
                    let name = match chosen {
                        sp_model::TextLink::Timer { id } => timer_name(id),
                        _ => None,
                    };
                    sp_model::edit_text(text, spans, 0, usize::MAX, &crate::links::placeholder(chosen, name));
                }
                // Unlinked: the words the link put there go too, and the box says the sample
                // words again ("Text", in the language set), as a new one does. Words of its own
                // were never replaced, so they stay.
                None if placeholder => sp_model::edit_text(text, spans, 0, usize::MAX, crate::editor::sample_words()),
                _ => {}
            }
            if link.is_some() {
                spans.clear();
            }
            object.link = link;
        }
    }

    // In a template: which of a slide's words the box holds. No two boxes hold the same, and
    // every text box holds some (one with no role yet takes one from its name when the
    // template is kept), so there is no choice of none.
    if for_theme && boxes == 1 {
        let mut holds = object.role.clone();
        row(ui, column, t("inspector.row.holds", "Holds"), |ui| {
            let shown = holds.as_ref().map_or_else(|| object.name.clone(), crate::editor::role_label);
            egui::ComboBox::from_id_salt("holds")
                .selected_text(shown)
                .width(control_room(column))
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    for role in crate::editor::word_roles() {
                        let on = holds.as_ref() == Some(&role);
                        let taken = !on && taken_roles.contains(&role);
                        let key =
                            ui.add_enabled(!taken, egui::Button::selectable(on, crate::editor::role_label(&role)));
                        let why = t("editor.add.box.taken", "This template has one already");
                        if key.reason(why).clicked() {
                            holds = Some(role);
                        }
                    }
                })
                .response
                .hint(t(
                    "quickhelp.inspector.holds",
                    "Which of a slide\u{2019}s words this box of the template holds.",
                ));
        });
        if holds != object.role {
            // A box still named after what it held is named after what it holds now.
            let named_for_role = object.role.as_ref().is_some_and(|role| object.name == role.label());
            if let (true, Some(role)) = (named_for_role, holds.as_ref()) {
                object.name = role.label();
            }
            object.role = holds;
            inspector.new_step = true;
        }
    }

    // The keys at the foot, as wide as their words: the look as a whole.
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(FOOT_APART, FOOT_APART);
        if word_key(ui, t("inspector.text.copy_style", "Copy style"), false, true)
            .hint(t("inspector.text.copy_style.hint", "Remember how this text looks"))
            .clicked()
        {
            inspector.copied_style = Some(style.clone());
        }
        let can_paste = inspector.copied_style.is_some();
        let hint = t("inspector.text.paste_style.hint", "Give this text the look that was copied");
        if word_key(ui, t("inspector.text.paste_style", "Paste style"), false, can_paste)
            .hint_or_greyed(hint, (!can_paste).then(crate::home::copy_a_style))
            .clicked()
            && let Some(copied) = &inspector.copied_style
        {
            *style = copied.clone();
        }
        // More: let the theme decide the look again (on a slide), and give every word the
        // box's look again. Each is greyed where it has nothing to do.
        let hint = t("inspector.text.more", "More: back to the theme, clear word styles");
        let more = square_key(ui, KEY, (false, false), hint, |painter, cell, ink| {
            let c = cell.center();
            for step in [-5.0, 0.0, 5.0] {
                painter.circle_filled(pos2(c.x + step, c.y), 1.5, ink);
            }
        });
        more.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hint));
        egui::Popup::menu(&more).show(|ui| {
            if !for_theme && !stage {
                let plain = style.is_default();
                let hint = if plain {
                    t("inspector.text.plain", "Looks as the theme has it")
                } else {
                    t(
                        "inspector.text.reset.hint",
                        "Give this text the theme's look again; words you styled one by one keep theirs",
                    )
                };
                let back = egui::Button::new(t("inspector.text.reset", "Back to theme"));
                let back = ui.add_enabled(!plain, back).hint(hint).reason(hint);
                if back.clicked() {
                    *style = TextStyle::default();
                    ui.close();
                }
            }
            let clear = egui::Button::new(t("inspector.text.clear_words", "Clear word styles"));
            let hint = t("inspector.text.clear_words.hint", "Every word takes the box's look again");
            if ui.add_enabled(!spans.is_empty(), clear).hint(hint).clicked() {
                spans.clear();
                inspector.new_step = true;
                ui.close();
            }
        });
    });
}

/// Puts the box's own value back in each setting a word can have.
fn for_each_span_setting(look: &mut BoxStyle, box_look: &BoxStyle) {
    look.font_family.clone_from(&box_look.font_family);
    (look.size, look.weight, look.italic) = (box_look.size, box_look.weight, box_look.italic);
    (look.underline, look.strike, look.fill) = (box_look.underline, box_look.strike, box_look.fill);
}

/// How wide the Weight menu beside the Font menu is.
const WEIGHT: f32 = 96.0;

/// The room between one row of the Text tab and the next: closer than the other tabs' rows
/// ([`ROWS_APART`]), so the whole tab fits a window of 1100 by 700 with words selected.
const TEXT_ROWS_APART: f32 = 8.0;

/// The Text tab's rows for one look, their captions in a column `column` wide: Font with its
/// Weight, Size, Style, Colour, Align, Effects, Spacing, Fitting and Words. What is set once
/// opens from a key on its row as a panel over the rows ([`Panel`]), so the tab fits one screen
/// and no row moves. `wanted` starts as the look the text has now. With `words_picked`, a
/// second line over Align says the rows from there on are the whole box's.
fn text_look(
    ui: &mut egui::Ui,
    column: f32,
    wanted: &mut BoxStyle,
    base: &BoxStyle,
    fonts: &[String],
    inspector: &mut InspectorState,
    (mix, words_picked): (sp_model::SpanMix, bool),
) {
    let room = control_room(column);
    row(ui, column, t("inspector.row.font", "Font"), |ui| {
        // The menu in a place of its own: its words cut short can make it a hair wider than it
        // is asked to be, and Weight beside it stays where it is.
        let font_menu = fixed(ui, room - WEIGHT - BETWEEN, |ui| {
            egui::ComboBox::from_id_salt("font family")
                .selected_text(if mix.font_family {
                    egui::RichText::new(t("inspector.font.mixed", "Mixed")).italics().color(DIM)
                } else {
                    egui::RichText::new(wanted.font_family.as_str())
                })
                .width(room - WEIGHT - BETWEEN)
                .height(360.0)
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    // Type to narrow the list: the field has the keyboard as soon as the menu opens,
                    // and Return takes the first font left.
                    let search = ui.add(
                        egui::TextEdit::singleline(&mut inspector.font_search)
                            .hint_text(t("inspector.font.search", "Search fonts"))
                            .desired_width(f32::INFINITY),
                    );
                    search.request_focus();
                    let typed = inspector.font_search.trim().to_lowercase();
                    // Fonts that start with what was typed come before those that only contain it.
                    let mut found: Vec<&String> =
                        fonts.iter().filter(|name| name.to_lowercase().starts_with(&typed)).collect();
                    found.extend(fonts.iter().filter(|name| {
                        let name = name.to_lowercase();
                        !name.starts_with(&typed) && name.contains(&typed)
                    }));
                    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if enter
                        && !typed.is_empty()
                        && let Some(first) = found.first()
                    {
                        wanted.font_family = (*first).clone();
                        inspector.font_search.clear();
                        ui.close();
                    }
                    ui.separator();
                    if found.is_empty() {
                        ui.label(
                            egui::RichText::new(t("inspector.font.none", "No font has that in its name.")).color(DIM),
                        );
                    }
                    for family in found {
                        if ui.selectable_label(wanted.font_family == *family, family.as_str()).clicked() {
                            wanted.font_family = family.clone();
                            inspector.font_search.clear();
                            ui.close();
                        }
                    }
                })
        });
        font_menu
            .response
            .clone()
            .hint(t("quickhelp.inspector.font", "Choose the font. Type in its list to find one."));
        // Testing aid: SOFTPRESENTER_CAPTURE_FONT_MENU=1 opens the font list once, for a capture.
        static MENU_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if std::env::var_os("SOFTPRESENTER_CAPTURE_FONT_MENU").is_some()
            && !MENU_OPENED.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            egui::Popup::open_id(ui.ctx(), font_menu.response.id.with("popup"));
        }
        // Weight, beside the font it belongs to.
        egui::ComboBox::from_id_salt("font weight")
            .selected_text(weight_name(wanted.weight))
            .width(WEIGHT)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .show_ui(ui, |ui| {
                for (weight, (key, english)) in WEIGHTS {
                    ui.selectable_value(&mut wanted.weight, weight, t(key, english));
                }
            })
            .response
            .hint(t("inspector.row.weight", "Weight"));
    });
    row(ui, column, t("inspector.row.size", "Size"), |ui| {
        // Room for "Mixed" whatever the sizes are, so the field keeps one width.
        number(ui, &mut wanted.size, (NUMBER + 14.0, ""), |d| {
            d.range(8.0..=600.0).custom_formatter(move |value, _| {
                if mix.size { t("inspector.size.mixed", "Mixed").to_owned() } else { format!("{value:.0}") }
            })
        })
        .hint(t("inspector.size.hint", "Size, in slide units"));
        for (label, by, hint) in [
            ("A\u{2212}", -SIZE_STEP, t("inspector.size.smaller", "Every size one step smaller")),
            ("A+", SIZE_STEP, t("inspector.size.bigger", "Every size one step bigger")),
        ] {
            if letter_key(ui, KEY_WIDE, (false, false), hint, label).clicked() {
                inspector.size_step = by;
            }
        }
    });
    row(ui, column, t("inspector.row.style", "Style"), |ui| {
        let bold = wanted.weight >= 600;
        let styles = [
            (bold, mix.weight, crate::keys::hint(t("inspector.style.bold", "Bold"), "B", false)),
            (wanted.italic, mix.italic, crate::keys::hint(t("inspector.style.italic", "Italic"), "I", false)),
            (
                wanted.underline,
                mix.underline,
                crate::keys::hint(t("inspector.style.underline", "Underlined"), "U", false),
            ),
            (wanted.strike, mix.strike, t("inspector.style.strike", "Struck through").to_owned()),
        ];
        for (which, (on, mixed, hint)) in styles.into_iter().enumerate() {
            let key = square_key(ui, KEY_WIDE, (on && !mixed, mixed), &hint, |p, cell, ink| {
                let c = cell.center();
                let font = if which == 0 { crate::studio::semibold(13.5) } else { FontId::proportional(13.5) };
                p.text(c, Align2::CENTER_CENTER, ["B", "I", "U", "S"][which], font, ink);
                match which {
                    2 => {
                        p.line_segment([pos2(c.x - 5.0, c.y + 8.0), pos2(c.x + 5.0, c.y + 8.0)], Stroke::new(1.0, ink));
                    }
                    3 => {
                        p.line_segment([pos2(c.x - 6.0, c.y), pos2(c.x + 6.0, c.y)], Stroke::new(1.0, ink));
                    }
                    _ => {}
                }
            });
            if key.clicked() {
                match which {
                    0 => wanted.weight = if bold && !mixed { 400 } else { 700 },
                    1 => wanted.italic = !wanted.italic,
                    2 => wanted.underline = !wanted.underline,
                    _ => wanted.strike = !wanted.strike,
                }
            }
        }
    });
    // Colour: the swatches, any colour, and last a ramp that opens the gradient's panel.
    let (swatch_room, side) = room_before_ramp(room, 9, SWATCH);
    let colours = Swatches {
        salt: "text colour",
        room: swatch_room,
        count: 8,
        none_too: false,
        mixed: mix.fill.then_some(mix.other_fill).flatten(),
    };
    let ends = wanted.gradient.on().then_some((wanted.fill, wanted.gradient.to));
    let colour = swatches_row(ui, column, t("inspector.row.colour", "Colour"), &colours, |ui, look| {
        swatches(ui, look, &mut wanted.fill);
        ramp_swatch(ui, side, &mut inspector.panel, Panel::TextGradient, ends);
    });
    let title = t("inspector.row.gradient", "Gradient");
    panel_over(ui, &mut inspector.panel, (Panel::TextGradient, false), colour, title, |ui| {
        gradient_panel(ui, "text gradient", true, &mut wanted.gradient);
    });

    if words_picked {
        scope_line(ui, false, t("inspector.text.whole_box", "THE WHOLE BOX"), "");
    }
    row(ui, column, t("inspector.row.align", "Align"), |ui| {
        // Seven keys and the break between the two groups, in the room there is.
        let rule = BETWEEN + 1.0;
        let wide = key_width(room - rule, 7);
        let across = [
            (TextAlign::Left, t("inspector.align.left", "Left")),
            (TextAlign::Center, t("inspector.align.centre", "Centre")),
            (TextAlign::Right, t("inspector.align.right", "Right")),
            (TextAlign::Justify, t("inspector.align.justify", "Both edges")),
        ];
        for (which, (align, hint)) in across.into_iter().enumerate() {
            let on = wanted.align == align;
            if square_key(ui, wide, (on, false), hint, |p, cell, ink| text_lines(p, cell, which, ink)).clicked() {
                wanted.align = align;
            }
        }
        let (line, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
        ui.painter().rect_filled(line.translate(vec2(0.0, -6.0)), 0.0, crate::studio::LINE);
        let down = [
            (VAlign::Top, t("inspector.align.top", "Top")),
            (VAlign::Middle, t("inspector.align.middle", "Middle")),
            (VAlign::Bottom, t("inspector.align.bottom", "Bottom")),
        ];
        for (which, (align, hint)) in down.into_iter().enumerate() {
            let on = wanted.valign == align;
            if square_key(ui, wide, (on, false), hint, |p, cell, ink| vertical_mark(p, cell, which, ink)).clicked() {
                wanted.valign = align;
            }
        }
    });
    // Effects: Outline and Shadow, each a switch with the key to its look beside it.
    let outlined = wanted.outline_width > 0.0;
    let effects = row(ui, column, t("inspector.row.effects", "Effects"), |ui| {
        let outline = (t("inspector.row.outline", "Outline"), outlined);
        let hint = t("inspector.outline.more", "Outline: width and colour");
        if switch_with_more(ui, &mut inspector.panel, Panel::TextOutline, outline, hint) {
            wanted.outline_width = if outlined { 0.0 } else { base.outline_width.max(1.0) };
        }
        let shadow = (t("inspector.shadow", "Shadow"), wanted.shadow);
        let hint = t("inspector.shadow.more", "Shadow: angle, offset, blur, opacity and colour");
        if switch_with_more(ui, &mut inspector.panel, Panel::TextShadow, shadow, hint) {
            wanted.shadow = !wanted.shadow;
        }
    });
    let outlined = wanted.outline_width > 0.0;
    let title = t("inspector.row.outline", "Outline");
    panel_over(ui, &mut inspector.panel, (Panel::TextOutline, false), effects, title, |ui| {
        let captions = [t("inspector.stroke.width", "Width"), t("inspector.row.colour", "Colour")];
        let column = caption_column(ui, &captions);
        let room = panel_room(column);
        row(ui, column, captions[0], |ui| {
            // Typing a width draws the outline: there is none at 0.
            let mut width = wanted.outline_width;
            number(
                ui,
                &mut width,
                (NUMBER, t("quickhelp.inspector.stroke_width", "The outline\u{2019}s width, in points.")),
                |d| d.speed(0.25).range(0.0..=60.0).max_decimals(1),
            );
            wanted.outline_width = width;
        });
        let colours = Swatches { salt: "text outline", room, count: 8, none_too: false, mixed: None };
        swatches_row(ui, column, captions[1], &colours, |ui, look| {
            swatches(ui, look, &mut wanted.outline_color);
        });
        if !outlined {
            panel_note(ui, t("inspector.outline.off", "Outline is off. Its colour is kept for when it is on."));
        }
    });
    let title = t("inspector.shadow", "Shadow");
    panel_over(ui, &mut inspector.panel, (Panel::TextShadow, false), effects, title, |ui| {
        shadow_panel(ui, "text shadow", true, &mut wanted.shadow_look);
        if !wanted.shadow {
            panel_note(ui, t("inspector.shadow.off", "Shadow is off. Its look is kept for when it is on."));
        }
    });

    // Spacing: between letters and between lines on the row; the margins and the rest behind
    // its key.
    let more_spacing = wanted.margins != [0.0; 4] || wanted.paragraph_spacing != 0.0 || wanted.baseline != 0.0;
    let spacing = row_tall(ui, column, t("inspector.row.spacing", "Spacing"), MINI, |ui| {
        let each = mini_width(room - KEY - BETWEEN, 2).min(76.0);
        mini(ui, (each, ""), tk("inspector.mini.letters", "Letters"), &mut wanted.letter_spacing, |d| {
            d.speed(0.2).range(-20.0..=100.0).max_decimals(1)
        })
        .hint(t("inspector.spacing.letters.hint", "Extra room between letters, in points at the box's font size"));
        mini(ui, (each, ""), tk("inspector.mini.lines", "Lines"), &mut wanted.line_height, |d| {
            d.speed(0.01).range(0.5..=3.0).fixed_decimals(2)
        })
        .hint(t(
            "inspector.spacing.line_height.hint",
            "The distance from one line to the next, as a multiple of the font size",
        ));
        ui.add_space(room - 2.0 * (each + BETWEEN) - KEY);
        let hint = t("inspector.spacing.more", "Margins and more spacing");
        more_key(ui, &mut inspector.panel, Panel::Spacing, hint, more_spacing);
    });
    let title = t("inspector.spacing.title", "Margins and spacing");
    panel_over(ui, &mut inspector.panel, (Panel::Spacing, true), spacing, title, |ui| {
        let captions = [t("inspector.row.margins", "Margins"), t("inspector.row.spacing", "Spacing")];
        let column = caption_column(ui, &captions);
        let room = panel_room(column);
        row_tall(ui, column, captions[0], MINI, |ui| {
            let each = mini_width(room, 4);
            let sides = [
                tk("inspector.margin.left", "Left"),
                tk("inspector.margin.top", "Top"),
                tk("inspector.margin.right", "Right"),
                tk("inspector.margin.bottom", "Bottom"),
            ];
            for (margin, name) in wanted.margins.iter_mut().zip(sides) {
                mini(
                    ui,
                    (
                        each,
                        t(
                            "quickhelp.inspector.margin",
                            "Room between the box\u{2019}s edge and its words, on this side.",
                        ),
                    ),
                    name,
                    margin,
                    |d| d.range(0.0..=400.0),
                );
            }
        });
        row_tall(ui, column, captions[1], MINI, |ui| {
            let each = mini_width(room, 2).min(96.0);
            mini(ui, (each, ""), tk("inspector.mini.paragraph", "Paragraph"), &mut wanted.paragraph_spacing, |d| {
                d.speed(0.5).range(0.0..=400.0)
            })
            .hint(t(
                "inspector.spacing.paragraph.hint",
                "Extra room after each paragraph (each line break typed), in points",
            ));
            mini(ui, (each, ""), tk("inspector.mini.baseline", "Baseline"), &mut wanted.baseline, |d| {
                d.speed(0.5).range(-400.0..=400.0)
            })
            .hint(t("inspector.spacing.baseline.hint", "Raise the text by this many points (lower it with a minus)"));
        });
        // Spacing and margins as a new box has them.
        let changed = more_spacing || wanted.letter_spacing != 0.0 || wanted.line_height != sp_model::LINE_HEIGHT;
        ui.horizontal(|ui| {
            let reset = t("inspector.spacing.reset", "Reset spacing");
            let hint = t("inspector.spacing.reset.hint", "Spacing and margins as a new box has them");
            if word_key(ui, reset, false, changed).hint(hint).clicked() {
                wanted.letter_spacing = 0.0;
                wanted.line_height = sp_model::LINE_HEIGHT;
                wanted.paragraph_spacing = 0.0;
                wanted.baseline = 0.0;
                wanted.margins = [0.0; 4];
            }
        });
    });

    row(ui, column, t("inspector.row.fitting", "Fitting"), |ui| {
        let options = [
            (Scaling::None, t("inspector.scaling.none", "None")),
            (Scaling::TextDown, t("inspector.scaling.down", "Text down")),
            (Scaling::TextUp, t("inspector.scaling.up", "Text up")),
            (Scaling::TextUpOrDown, t("inspector.scaling.up_or_down", "Text up or down")),
            (Scaling::ContainerUpOrDown, t("inspector.scaling.container", "Box up or down")),
        ];
        choice(
            ui,
            ("scaling", t("quickhelp.inspector.scaling", "Whether the words grow or shrink to fill the box.")),
            room,
            &mut wanted.scaling,
            &options,
        );
    });

    // Words: their capitals on the row; how the lines are broken, their bullets, scrolling and
    // the band behind them behind its key.
    let more_lines = wanted.line_transform != LineTransform::None
        || wanted.list != sp_model::ListStyle::None
        || wanted.scroll != sp_model::Scroll::None
        || wanted.line_fill.on;
    let words = row(ui, column, t("inspector.row.words", "Words"), |ui| {
        let options = [
            (Caps::None, t("inspector.caps.none", "As typed")),
            (Caps::AllCaps, t("inspector.caps.all", "ALL CAPITALS")),
            (Caps::Lowercase, t("inspector.caps.lower", "all small")),
            (Caps::StartCase, t("inspector.caps.start", "Each Word")),
            (Caps::TitleCase, t("inspector.caps.title", "Like a Title")),
        ];
        choice(
            ui,
            ("caps", t("quickhelp.inspector.caps", "Capital letters for the words, whatever was typed.")),
            room - KEY - BETWEEN,
            &mut wanted.caps,
            &options,
        );
        let hint = t("inspector.lines.more", "Lines: breaks, bullets, scrolling and the band behind the lines");
        more_key(ui, &mut inspector.panel, Panel::Lines, hint, more_lines);
    });
    let title = t("inspector.row.lines", "Lines");
    panel_over(ui, &mut inspector.panel, (Panel::Lines, true), words, title, |ui| {
        let captions = [
            t("inspector.row.breaks", "Breaks"),
            t("inspector.row.bullets", "Bullets"),
            t("inspector.row.scroll", "Scroll"),
            t("inspector.row.band", "Band"),
            t("inspector.row.covers", "Covers"),
        ];
        let column = caption_column(ui, &captions);
        let room = panel_room(column);
        row(ui, column, captions[0], |ui| {
            let options = [
                (LineTransform::None, t("inspector.lines.none", "As typed")),
                (LineTransform::RemoveLineReturns, t("inspector.lines.remove", "Run together")),
                (LineTransform::ReplaceLineReturns, t("inspector.lines.replace", "Run together, with / between lines")),
                (LineTransform::OneWordPerLine, t("inspector.lines.word", "A word on each line")),
                (LineTransform::OneCharacterPerLine, t("inspector.lines.character", "A letter on each line")),
            ];
            choice(
                ui,
                (
                    "line transform",
                    t("quickhelp.inspector.line_transform", "How the lines of the words are broken on the slide."),
                ),
                room,
                &mut wanted.line_transform,
                &options,
            );
        });
        // Bullets or numbers before each line.
        row(ui, column, captions[1], |ui| {
            let lists: Vec<(sp_model::ListStyle, &str)> =
                sp_model::ListStyle::ALL.iter().map(|list| (*list, list_name(*list))).collect();
            choice(
                ui,
                ("list", t("quickhelp.inspector.list", "Bullets or numbers before each line.")),
                room,
                &mut wanted.list,
                &lists,
            );
        });
        // Text that keeps moving through its box, as a ticker does.
        row(ui, column, captions[2], |ui| {
            let ways = [
                (sp_model::Scroll::None, t("inspector.scroll.none", "Still")),
                (sp_model::Scroll::Left, t("inspector.scroll.left", "Scrolls left")),
                (sp_model::Scroll::Right, t("inspector.scroll.right", "Scrolls right")),
                (sp_model::Scroll::Up, t("inspector.scroll.up", "Scrolls up")),
                (sp_model::Scroll::Down, t("inspector.scroll.down", "Scrolls down")),
            ];
            choice(
                ui,
                ("scroll", t("quickhelp.inspector.scroll_way", "The way the words scroll across the box.")),
                room,
                &mut wanted.scroll,
                &ways,
            );
        });
        // A band of colour behind each line: under its words, or across the box. (The colour of
        // the box itself is the tab's Behind row.)
        let band = Swatches { salt: "band", room, count: 8, none_too: true, mixed: None };
        swatches_row(ui, column, captions[3], &band, |ui, look| {
            let mut colour = wanted.line_fill.on.then_some(wanted.line_fill.color);
            let was_on = wanted.line_fill.on;
            if swatch_row(ui, look, &mut colour) {
                wanted.line_fill.on = colour.is_some();
                if let Some(colour) = colour {
                    // A colour chosen where there was none keeps how solid the last one was.
                    let solid = if was_on { colour.a } else { wanted.line_fill.color.a };
                    wanted.line_fill.color = Rgba { a: solid, ..colour };
                }
            }
        });
        row(ui, column, captions[4], |ui| {
            ui.add_enabled_ui(wanted.line_fill.on, |ui| {
                let options = [
                    (false, t("inspector.text.background.lines", "Lines Only")),
                    (true, t("inspector.text.background.full", "Full Width")),
                ];
                choice(
                    ui,
                    (
                        "line fill",
                        t(
                            "quickhelp.inspector.line_fill",
                            "The colour behind each line only, or across the whole box.",
                        ),
                    ),
                    room,
                    &mut wanted.line_fill.full_width,
                    &options,
                );
            });
        });
    });
}

/// The Text tab's Behind row: the colour the box is filled with behind its words (none, the
/// swatches, any colour), and last a ramp that opens the fill's gradient. It sets the box's
/// own fill and gradient, the one colour a box has behind what it holds; the panel opens
/// upward, over the rows above it.
fn behind_row(
    ui: &mut egui::Ui,
    column: f32,
    panel: &mut Option<Panel>,
    fill: &mut Option<Rgba>,
    gradient: &mut Option<sp_model::Gradient>,
) {
    let room = control_room(column);
    let (swatch_room, side) = room_before_ramp(room, 10, 18.0);
    let look = Swatches { salt: "behind", room: swatch_room, count: 8, none_too: true, mixed: None };
    let ends = fill.zip(gradient.filter(sp_model::Gradient::on)).map(|(fill, gradient)| (fill, gradient.to));
    let behind = swatches_row(ui, column, t("inspector.row.behind", "Behind"), &look, |ui, look| {
        // A colour chosen for a box that had none starts part see-through over what is behind
        // it, as a new fill always has.
        let had = fill.is_some();
        if swatch_row(ui, look, fill)
            && !had
            && let Some(fill) = fill.as_mut()
        {
            fill.a = 160;
        }
        ramp_swatch(ui, side, panel, Panel::BehindGradient, ends);
    });
    let title = t("inspector.row.gradient", "Gradient");
    panel_over(ui, panel, (Panel::BehindGradient, true), behind, title, |ui| {
        // Drawing changes nothing: the gradient is written back only when a control changed it.
        let mut look = gradient.unwrap_or_default();
        let before = look;
        gradient_panel(ui, "behind gradient", fill.is_some(), &mut look);
        if look != before {
            *gradient = look.on().then_some(look);
        }
    });
}

/// The Animation tab: how the object comes on and goes off, and the order clicks set the
/// slide's builds going in. Each build's own rows stay while it has none, greyed.
fn build_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    use sp_model::{Build, BuildEffect, Direction};
    let Some(draft) = state.editing.as_mut() else { return };
    let order: Vec<String> = draft
        .slide
        .click_steps()
        .into_iter()
        .filter_map(|(at, coming_in)| {
            let object = draft.slide.objects.get(at)?;
            let build = if coming_in { object.build_in } else { object.build_out }?;
            let name = object
                .as_text()
                .map_or_else(|| object.name.clone(), |text| text.lines().next().unwrap_or("").to_owned());
            Some(tf(
                "inspector.build.step",
                "{name}: {effect}",
                &[("name", &name), ("effect", effect_name(build.effect, coming_in))],
            ))
        })
        .collect();
    let Some(object) = draft.selected_object_mut() else {
        return nothing_selected(ui);
    };
    let is_text = object.as_text().is_some();
    let captions = [
        t("inspector.build.in", "Comes on"),
        t("inspector.build.starts", "When"),
        t("inspector.build.timing", "Timing"),
        t("inspector.build.delivery", "Text"),
        t("inspector.build.out", "Goes off"),
    ];
    let column = caption_column(ui, &captions);
    let room = control_room(column);
    let [comes_on, when, timing, delivery, goes_off] = captions;
    for coming_in in [true, false] {
        let slot = if coming_in { &mut object.build_in } else { &mut object.build_out };
        let on = slot.is_some();
        // The rows show a build's usual settings while there is none.
        let mut build = slot.unwrap_or(Build { on_click: !coming_in, ..Build::of(BuildEffect::Dissolve) });
        let before = build;
        let mut effect = slot.map(|build| build.effect);
        if !coming_in {
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0.0, crate::studio::LINE);
        }
        row(ui, column, if coming_in { comes_on } else { goes_off }, |ui| {
            let half = ((room - BETWEEN) / 2.0).floor();
            let mut options: Vec<(Option<BuildEffect>, &str)> = vec![(None, t("inspector.build.none", "None"))];
            options.extend(BuildEffect::ALL.iter().map(|effect| (Some(*effect), effect_name(*effect, coming_in))));
            choice(
                ui,
                (
                    if coming_in { "build in" } else { "build out" },
                    t("quickhelp.inspector.build_effect", "How the box comes onto the slide, or leaves it."),
                ),
                half,
                &mut effect,
                &options,
            );
            // The side it flies from, or to: only a build that flies has one.
            ui.add_enabled_ui(effect == Some(BuildEffect::Fly), |ui| {
                let sides = [
                    (Direction::FromLeft, t("inspector.build.side.left", "Left")),
                    (Direction::FromRight, t("inspector.build.side.right", "Right")),
                    (Direction::FromTop, t("inspector.build.side.top", "Top")),
                    (Direction::FromBottom, t("inspector.build.side.bottom", "Bottom")),
                ];
                choice(
                    ui,
                    (
                        if coming_in { "build in side" } else { "build out side" },
                        t("quickhelp.inspector.build_side", "The side it flies in from, or out to."),
                    ),
                    half,
                    &mut build.direction,
                    &sides,
                );
            });
        });
        if coming_in {
            row(ui, column, when, |ui| {
                ui.add_enabled_ui(on, |ui| {
                    let starts = [
                        (false, t("inspector.build.with_slide", "With the slide")),
                        (true, t("inspector.build.on_click", "On a click of Next")),
                    ];
                    choice(
                        ui,
                        (
                            "build in start",
                            t(
                                "quickhelp.inspector.build_start",
                                "Whether it comes with the slide or waits for the next click.",
                            ),
                        ),
                        room,
                        &mut build.on_click,
                        &starts,
                    );
                });
            });
        }
        row_tall(ui, column, timing, MINI, |ui| {
            ui.add_enabled_ui(on, |ui| {
                let each = mini_width(room, 4);
                mini(
                    ui,
                    (each, t("quickhelp.inspector.build_seconds", "How long the build takes, in seconds.")),
                    tk("inspector.build.seconds", "Seconds"),
                    &mut build.seconds,
                    |d| d.speed(0.05).range(0.0..=10.0).max_decimals(2),
                );
                mini(
                    ui,
                    (each, t("quickhelp.inspector.build_delay", "How long it waits before it starts, in seconds.")),
                    tk("inspector.build.delay", "Delay"),
                    &mut build.delay,
                    |d| d.speed(0.05).range(0.0..=30.0).max_decimals(2),
                );
            });
        });
        if coming_in && is_text {
            row(ui, column, delivery, |ui| {
                ui.add_enabled_ui(on, |ui| {
                    let key = word_key(ui, t("inspector.build.by_paragraph", "By paragraph"), build.by_paragraph, true);
                    let hint = if build.on_click {
                        t("inspector.build.by_click", "Each click shows the next paragraph.")
                    } else {
                        t("inspector.build.by_time", "Each paragraph follows the last after its time in seconds.")
                    };
                    if key.hint(hint).clicked() {
                        build.by_paragraph = !build.by_paragraph;
                    }
                });
            });
        }
        if !coming_in {
            ui.add_enabled_ui(on, |ui| {
                let leaves = t("inspector.build.leaves", "Leaves when Next is clicked.");
                line_of_words(ui, leaves, crate::studio::TEXT_FAINT);
            });
        }
        if effect != slot.map(|build| build.effect) {
            *slot = effect.map(|effect| Build { effect, on_click: !coming_in, ..slot.unwrap_or(Build::of(effect)) });
        } else if on && build != before {
            *slot = Some(build);
        }
    }
    // The order clicks set the slide's builds going in: builds in first, then builds out.
    scope_line(ui, false, t("inspector.build.order", "Click order"), "");
    if order.is_empty() {
        let none = t("inspector.build.order.none", "No builds wait for a click. Next goes straight to the next slide.");
        line_of_words(ui, none, crate::studio::TEXT_FAINT);
    }
    for (index, step) in order.iter().enumerate() {
        ui.label(egui::RichText::new(format!("{}.  {step}", index + 1)).size(12.0).color(INK));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::RawInput;
    use sp_engine::Show;
    use sp_engine::SlideRef;
    use sp_model::parse_song_text;

    use super::*;
    use crate::editor::EditDraft;

    /// Lays the inspector out in a column as wide as the window's and gives the right edge of
    /// the widest control the frame holds.
    fn widest(state: &mut HomeState, theme: &Theme) -> f32 {
        let ctx = crate::studio::tests::styled();
        let mut widest: f32 = 0.0;
        let mut worst = String::new();
        for _ in 0..3 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_WIDTH, 900.0))),
                ..RawInput::default()
            };
            ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
                    inspector(ui, state, theme);
                    widest = 0.0;
                    ui.ctx().viewport(|viewport| {
                        for (_, rects) in viewport.this_pass.widgets.layers() {
                            for widget in rects {
                                // Below the tab bar and above the footer, and not the scroll area.
                                if widget.rect.min.y > 45.0
                                    && widget.rect.max.y < 850.0
                                    && widget.rect.height() < 100.0
                                    && widget.rect.max.x > widest
                                {
                                    widest = widget.rect.max.x;
                                    worst = format!("{:?} {:?}", widget.rect, widget.sense);
                                }
                            }
                        }
                    });
                });
            })
            .drop_without_applying_deltas();
        }
        eprintln!("widest control ends at {widest}: {worst}");
        widest
    }

    fn editing_text(tab: InspectorTab, with_spans: bool, select: bool) -> HomeState {
        let show = Show::new(vec![Arc::new(parse_song_text("S", "We lift our voices\nWe sing together\n"))]);
        let mut state = HomeState::default();
        let mut draft = EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 }).unwrap();
        draft.selected = draft.slide.objects.first().map(|object| object.id);
        if with_spans
            && let Some(sp_model::ObjectKind::Text { spans, .. }) = draft.selected_object_mut().map(|o| &mut o.kind)
        {
            spans.push(sp_model::TextSpan::new(
                3,
                7,
                sp_model::SpanStyle {
                    italic: Some(true),
                    fill: Some(Rgba { r: 255, g: 200, b: 0, a: 255 }),
                    ..Default::default()
                },
            ));
        }
        if select {
            draft.typing = draft.selected;
            draft.typing_range = Some((0, 9));
        }
        state.editing = Some(draft);
        state.inspector.tab = tab;
        state
    }

    /// The words the inspector draws for `state`, in a column as wide as the window's.
    fn words_drawn(state: &mut HomeState, theme: &Theme) -> Vec<String> {
        fn walk(shape: &egui::Shape, into: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let ctx = crate::studio::tests::styled();
        let mut words = Vec::new();
        for _ in 0..2 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_WIDTH, 900.0))),
                ..RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| inspector(ui, state, theme));
            });
            words.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
            output.drop_without_applying_deltas();
        }
        words
    }

    #[test]
    fn opacity_says_its_percentage_as_each_language_writes_it_and_takes_one_typed() {
        let theme = Theme::builtin();
        for (language, solid, shadow) in [
            (crate::i18n::Language::English, "75%", "40%"),
            (crate::i18n::Language::Spanish, "75\u{a0}%", "40\u{a0}%"),
            (crate::i18n::Language::Russian, "75\u{a0}%", "40\u{a0}%"),
            (crate::i18n::Language::Ukrainian, "75\u{a0}%", "40\u{a0}%"),
        ] {
            crate::i18n::with_language(language, || {
                // The Box tab: the object's opacity beside its slider, its shadow's in its panel.
                let mut state = editing_text(InspectorTab::Box, false, false);
                if let Some(object) = state.editing.as_mut().and_then(|draft| draft.selected_object_mut()) {
                    object.opacity = 0.75;
                    let look =
                        sp_model::Shadow { color: Rgba::BLACK, opacity: 0.4, angle: 315.0, offset: 5.0, blur: 5.0 };
                    object.shadow = Some(look);
                }
                let mut state = with_panel(state, Panel::BoxShadow);
                let words = words_drawn(&mut state, &theme);
                assert!(words.iter().any(|w| w == solid), "{language:?}: {solid} in {words:?}");
                assert!(words.iter().any(|w| w == shadow), "{language:?}: {shadow} in {words:?}");
                // An opacity between two whole percentages is shown rounded, and left as it is.
                if let Some(object) = state.editing.as_mut().and_then(|draft| draft.selected_object_mut()) {
                    object.opacity = 0.455;
                    object.shadow = object.shadow.map(|look| sp_model::Shadow { opacity: 0.333, ..look });
                }
                let words = words_drawn(&mut state, &theme);
                assert!(words.contains(&crate::i18n::percent(46)), "{language:?}: {words:?}");
                let object = state.editing.as_ref().and_then(|draft| draft.selected_object()).expect("picked");
                assert_eq!(object.opacity, 0.455, "{language:?}: drawing changes nothing");
                assert_eq!(object.shadow.map(|look| look.opacity), Some(0.333), "{language:?}");
                // The Text tab: its shadow's opacity, the same way, in the shadow's panel.
                let mut state = with_panel(editing_text(InspectorTab::Own, false, false), Panel::TextShadow);
                let percent = (theme.templates[0].boxes[0].shadow_look.opacity * 100.0).round() as u64;
                let words = words_drawn(&mut state, &theme);
                let wanted = crate::i18n::percent(percent);
                assert!(words.contains(&wanted), "{language:?}: {wanted} in {words:?}");
            });
        }
        // Typed with the sign or without, its space, or a decimal comma.
        for typed in ["75", "75%", "75 %", "75\u{a0}%", " 75 "] {
            assert_eq!(typed_percent(typed), Some(75.0), "{typed:?}");
        }
        assert_eq!(typed_percent("12,5 %"), Some(12.5));
        assert_eq!(typed_percent("%"), None);
    }

    /// Every piece of words the inspector draws for `state`, with where it starts (to the
    /// nearest point), in a column as wide as the window's.
    fn words_placed(state: &mut HomeState, theme: &Theme) -> Vec<(String, i32, i32)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, i32, i32)>) {
            match shape {
                egui::Shape::Text(text) => {
                    into.push((text.galley.text().to_owned(), text.pos.x.round() as i32, text.pos.y.round() as i32))
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let ctx = crate::studio::tests::styled();
        let mut words = Vec::new();
        for _ in 0..3 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_WIDTH, 2400.0))),
                ..RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| inspector(ui, state, theme));
            });
            words.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
            output.drop_without_applying_deltas();
        }
        words
    }

    /// The text box being edited with its shadow switched on or off.
    fn with_shadow(on: bool) -> HomeState {
        let mut state = editing_text(InspectorTab::Own, false, false);
        if let Some(ObjectKind::Text { style, .. }) =
            state.editing.as_mut().and_then(|draft| draft.selected_object_mut()).map(|object| &mut object.kind)
        {
            style.shadow = Some(on);
        }
        state
    }

    #[test]
    fn shadow_switched_off_keeps_its_row_and_moves_nothing() {
        let theme = Theme::builtin();
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                let on = words_placed(&mut with_shadow(true), &theme);
                let off = words_placed(&mut with_shadow(false), &theme);
                assert_eq!(on, off, "{language:?}: the same words in the same places, Shadow on or off");
                // The shadow's own numbers are in its panel, whether it is on or off.
                let angle = t("inspector.shadow.angle", "Angle");
                for shadow in [true, false] {
                    let mut state = with_panel(with_shadow(shadow), Panel::TextShadow);
                    let open = words_placed(&mut state, &theme);
                    assert!(
                        open.iter().any(|(words, ..)| words.to_uppercase() == angle.to_uppercase()),
                        "{language:?}: {open:?}"
                    );
                }
            });
        }
    }

    const LANGUAGES: [crate::i18n::Language; 4] = [
        crate::i18n::Language::English,
        crate::i18n::Language::Spanish,
        crate::i18n::Language::Russian,
        crate::i18n::Language::Ukrainian,
    ];

    /// A slide being edited with a star added to it, picked alone or with the first text box.
    fn editing_shape(tab: InspectorTab, with_text: bool) -> HomeState {
        let mut state = editing_text(tab, false, false);
        if let Some(draft) = state.editing.as_mut() {
            let text = draft.selected;
            draft.add_shape(sp_model::Shape::Star);
            let star = draft.slide.objects.last().map(|object| object.id);
            draft.select_only(star);
            if let Some(text) = text.filter(|_| with_text) {
                draft.toggle_selected(text);
            }
        }
        state
    }

    #[test]
    fn the_tabs_follow_what_is_picked() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let names = |state: &HomeState| tabs_for(state).into_iter().map(|(_, name)| name).collect::<Vec<_>>();
            assert_eq!(names(&editing_text(InspectorTab::Own, false, false)), ["Text", "Box", "Animation"]);
            assert_eq!(names(&editing_shape(InspectorTab::Own, false)), ["Shape", "Box", "Animation"]);
            // Different kinds together have no tab of their own, and a line says what they are.
            let mut both = editing_shape(InspectorTab::Own, true);
            assert_eq!(names(&both), ["Box", "Animation"]);
            let words = words_drawn(&mut both, &Theme::builtin());
            assert!(words.iter().any(|said| said == "2 OBJECTS"), "{words:?}");
            assert!(words.iter().any(|said| said == "text box, shape"), "{words:?}");
            // A stage layout's objects are on the stage screens all the time: no Animation.
            let mut stage = editing_text(InspectorTab::Own, false, false);
            if let Some(draft) = stage.editing.as_mut() {
                draft.at.presentation = crate::stage::STAGE_SLOT;
            }
            assert_eq!(names(&stage), ["Text", "Box"]);
            // Nothing picked: no tabs, a plain heading.
            let mut none = editing_text(InspectorTab::Own, false, false);
            if let Some(draft) = none.editing.as_mut() {
                draft.select_only(None);
            }
            assert!(names(&none).is_empty());
        });
    }

    #[test]
    fn a_tab_the_new_pick_lacks_gives_way_and_box_stays_open() {
        let theme = Theme::builtin();
        // Box stays open from a text box to a shape.
        let mut state = editing_shape(InspectorTab::Box, false);
        words_drawn(&mut state, &theme);
        assert_eq!(state.inspector.tab, InspectorTab::Box);
        // A text box and a shape together have no tab of their own: Box opens.
        let mut state = editing_shape(InspectorTab::Own, true);
        words_drawn(&mut state, &theme);
        assert_eq!(state.inspector.tab, InspectorTab::Box);
    }

    /// The squares the inspector takes clicks in that are no bigger than a swatch: its colour
    /// swatches, for `state`.
    fn swatches_drawn(state: &mut HomeState, theme: &Theme) -> Vec<Rect> {
        let ctx = crate::studio::tests::styled();
        let mut found = Vec::new();
        for _ in 0..3 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_WIDTH, 2400.0))),
                ..RawInput::default()
            };
            ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
                    inspector(ui, state, theme);
                    found.clear();
                    ui.ctx().viewport(|viewport| {
                        for (_, rects) in viewport.this_pass.widgets.layers() {
                            for widget in rects {
                                let size = widget.rect.size();
                                if widget.sense.senses_click() && size.x == size.y && size.x <= SWATCH {
                                    found.push(widget.rect);
                                }
                            }
                        }
                    });
                });
            })
            .drop_without_applying_deltas();
        }
        found
    }

    #[test]
    fn every_row_of_swatches_is_on_one_line_inside_the_column() {
        let theme = Theme::builtin();
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                for (what, mut state, rows) in [
                    // Colour and Behind, each ending in its ramp.
                    ("text", editing_text(InspectorTab::Own, false, false), vec![10, 11]),
                    // The Box tab has no colours on its rows.
                    ("text box", editing_text(InspectorTab::Box, false, false), vec![]),
                    // Fill, To and Outline.
                    ("shape", editing_shape(InspectorTab::Own, false), vec![10, 9, 6]),
                    // Behind.
                    ("picture", editing_picture("cross.png", None), vec![10]),
                ] {
                    let found = swatches_drawn(&mut state, &theme);
                    let mut lines: Vec<(i32, Vec<Rect>)> = Vec::new();
                    for cell in found {
                        match lines.iter_mut().find(|(y, _)| *y == cell.min.y.round() as i32) {
                            Some((_, line)) => line.push(cell),
                            None => lines.push((cell.min.y.round() as i32, vec![cell])),
                        }
                    }
                    lines.sort_by_key(|(y, _)| *y);
                    let counts: Vec<usize> = lines.iter().map(|(_, line)| line.len()).collect();
                    assert_eq!(counts, rows, "{language:?} {what}: swatches to a line");
                    for cell in lines.iter().flat_map(|(_, line)| line) {
                        assert!(cell.max.x <= PANEL_WIDTH + 0.1, "{language:?} {what}: {cell:?} past the column");
                        assert!(cell.width() >= 14.0, "{language:?} {what}: {cell:?} too small to press");
                    }
                }
            });
        }
    }

    /// Every piece of words the inspector draws for `state`, with the room it takes.
    fn word_rects(state: &mut HomeState, theme: &Theme) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => {
                    into.push((text.galley.text().to_owned(), Rect::from_min_size(text.pos, text.galley.size())))
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let ctx = crate::studio::tests::styled();
        let mut words = Vec::new();
        for _ in 0..3 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_WIDTH, 2400.0))),
                ..RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| inspector(ui, state, theme));
            });
            words.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
            output.drop_without_applying_deltas();
        }
        words
    }

    #[test]
    fn no_words_run_into_the_words_beside_them_in_any_language() {
        let theme = Theme::builtin();
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                for (what, mut state) in [
                    ("text", editing_text(InspectorTab::Own, true, false)),
                    ("text box", editing_text(InspectorTab::Box, false, false)),
                    ("animation", editing_text(InspectorTab::Animation, false, false)),
                    ("shape", editing_shape(InspectorTab::Own, false)),
                    ("slide", {
                        let mut state = editing_text(InspectorTab::Own, false, false);
                        state.song_templates = vec![(sp_model::Uuid::new_v4(), "Two languages".to_owned())];
                        if let Some(draft) = state.editing.as_mut() {
                            draft.select_only(None);
                        }
                        state
                    }),
                    ("video", editing_picture("clip.mp4", None)),
                    ("three picked", {
                        let mut state = editing_shape(InspectorTab::Box, true);
                        if let Some(draft) = state.editing.as_mut() {
                            draft.select_all();
                        }
                        state
                    }),
                ] {
                    let words = word_rects(&mut state, &theme);
                    for (said, place) in &words {
                        assert!(place.max.x <= PANEL_WIDTH + 0.5, "{language:?} {what}: {said:?} ends at {place:?}");
                        for (other, beside) in &words {
                            let same_line = (place.center().y - beside.center().y).abs() < 6.0;
                            if same_line && beside.min.x > place.min.x && !said.is_empty() && !other.is_empty() {
                                assert!(
                                    place.max.x <= beside.min.x + 0.5,
                                    "{language:?} {what}: {said:?} ({place:?}) runs into {other:?} ({beside:?})"
                                );
                            }
                        }
                    }
                }
            });
        }
    }

    #[test]
    fn a_box_without_its_shadow_or_fill_keeps_their_rows() {
        let theme = Theme::builtin();
        let places = |shadow: bool, fill: bool| {
            let mut state = editing_text(InspectorTab::Box, false, false);
            if let Some(object) = state.editing.as_mut().and_then(|draft| draft.selected_object_mut()) {
                object.shadow = shadow.then_some(BOX_SHADOW);
                object.fill = fill.then_some(Rgba { r: 0, g: 0, b: 0, a: 160 });
                object.gradient = fill.then_some(sp_model::Gradient::default());
            }
            words_placed(&mut state, &theme)
        };
        let plain = places(false, false);
        assert_eq!(plain, places(true, false), "Shadow on or off");
        assert_eq!(plain, places(false, true), "Fill on or off");
        // Drawing changes nothing: a gradient kept by a box with no fill is still there after.
        let mut state = editing_text(InspectorTab::Box, false, false);
        let kept = sp_model::Gradient { kind: sp_model::GradientKind::Linear, ..Default::default() };
        if let Some(object) = state.editing.as_mut().and_then(|draft| draft.selected_object_mut()) {
            object.gradient = Some(kept);
        }
        let before = state.editing.as_ref().and_then(|draft| draft.selected_object().cloned());
        words_placed(&mut state, &theme);
        assert_eq!(state.editing.as_ref().and_then(|draft| draft.selected_object().cloned()), before);
    }

    #[test]
    fn a_build_switched_on_moves_no_row() {
        let theme = Theme::builtin();
        let places = |built: bool| {
            let mut state = editing_text(InspectorTab::Animation, false, false);
            if let Some(object) = state.editing.as_mut().and_then(|draft| draft.selected_object_mut()) {
                let dissolve = sp_model::Build::of(sp_model::BuildEffect::Dissolve);
                object.build_in = built.then_some(dissolve);
            }
            // The menu says which effect, and the click order lists it: every other word stays.
            let mut words = words_placed(&mut state, &theme);
            let order = words.iter().position(|(said, ..)| said.eq_ignore_ascii_case("Click order")).expect("order");
            words.truncate(order + 1);
            words.retain(|(said, ..)| said != "None" && said != "Dissolve");
            words
        };
        crate::i18n::with_language(crate::i18n::Language::English, || assert_eq!(places(false), places(true)));
    }

    /// One frame of the inspector for `state` in `ctx` with `events`, and where it drew `words`
    /// last (on top: in an open menu).
    fn frame_with(
        (ctx, time): (&egui::Context, &mut f64),
        state: &mut HomeState,
        theme: &Theme,
        events: Vec<egui::Event>,
        words: &str,
    ) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, words: &str, found: &mut Option<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == words => {
                    *found = Some(text.pos + text.galley.size() / 2.0);
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, words, found)),
                _ => {}
            }
        }
        *time += 0.1;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_WIDTH, 2400.0))),
            time: Some(*time),
            events,
            ..RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| inspector(ui, state, theme));
        });
        let mut found = None;
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, words, &mut found));
        output.drop_without_applying_deltas();
        found
    }

    /// Clicks the middle of `words` where the inspector draws them for `state` in `ctx` (kept
    /// from one click to the next, so a menu a click opened is still open), and draws a frame
    /// after.
    fn click_in(ctx: &egui::Context, time: &mut f64, state: &mut HomeState, theme: &Theme, words: &str) {
        frame_with((ctx, time), state, theme, vec![], words);
        let at = frame_with((ctx, time), state, theme, vec![], words).unwrap_or_else(|| panic!("{words} drawn"));
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for events in [vec![egui::Event::PointerMoved(at)], vec![press(true)], vec![press(false)], vec![]] {
            frame_with((ctx, time), state, theme, events, words);
        }
    }

    /// [`click_in`] with a context of its own.
    fn click_words(state: &mut HomeState, theme: &Theme, words: &str) {
        click_in(&crate::studio::tests::styled(), &mut 0.0, state, theme, words);
    }

    #[test]
    fn the_box_tabs_object_row_renames_duplicates_and_deletes() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let count = |state: &HomeState| state.editing.as_ref().map_or(0, |draft| draft.slide.objects.len());
            let mut state = editing_shape(InspectorTab::Box, false);
            let before = count(&state);
            let star = state.editing.as_ref().and_then(|draft| draft.selected);
            click_words(&mut state, &theme, "Rename");
            let renaming = state.editing.as_ref().and_then(|draft| draft.renaming.clone());
            assert_eq!(renaming.map(|(id, name)| (Some(id), name)), Some((star, "Star".to_owned())));
            assert!(state.rename_started, "the Objects list takes the keyboard for it");
            click_words(&mut state, &theme, "Duplicate");
            assert_eq!(count(&state), before + 1, "a copy beside it");
            click_words(&mut state, &theme, "Delete");
            assert_eq!(count(&state), before, "the copy, which is picked, goes");
        });
    }

    #[test]
    fn a_shapes_own_tab_says_which_shape_and_a_slide_with_nothing_picked_has_its_panel() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let words = words_drawn(&mut editing_shape(InspectorTab::Own, false), &theme);
            for wanted in ["SHAPE", "Star", "FILL", "OUTLINE"] {
                assert!(words.iter().any(|said| said == wanted), "{wanted} in {words:?}");
            }
            // A song's slide with nothing picked: its own settings, under the heading SLIDE.
            let mut state = editing_text(InspectorTab::Own, false, false);
            if let Some(draft) = state.editing.as_mut() {
                draft.select_only(None);
            }
            let words = words_drawn(&mut state, &theme);
            for wanted in ["SLIDE", "LABEL", "GO TO NEXT", "None"] {
                assert!(words.iter().any(|said| said == wanted), "{wanted} in {words:?}");
            }
            // Its notes are the footer's Slide Notes, in one place only.
            assert!(!words.iter().any(|said| said == "SLIDE NOTES"), "{words:?}");
            assert_eq!(words.iter().filter(|said| *said == "Slide Notes").count(), 1, "{words:?}");
            // A time chosen from Go to Next's menu (the last None drawn) is the slide's, and one
            // Undo step.
            let depth = |state: &HomeState| state.editing.as_ref().map_or(0, EditDraft::undo_depth);
            let advance = |state: &HomeState| state.editing.as_ref().and_then(|draft| draft.slide.advance);
            let before = depth(&state);
            let (ctx, mut time) = (crate::studio::tests::styled(), 0.0);
            click_in(&ctx, &mut time, &mut state, &theme, "None");
            click_in(&ctx, &mut time, &mut state, &theme, "After 5 Seconds");
            assert_eq!(advance(&state), Some(5.0));
            assert_eq!(depth(&state), before + 1, "one step for Undo");
            assert!(state.editing.as_mut().is_some_and(EditDraft::undo), "Undo");
            assert_eq!(advance(&state), None);
        });
    }

    #[test]
    fn a_templates_text_box_says_what_it_holds_and_has_no_animation() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut draft = EditDraft::of_template(&theme.templates[0], 0);
            let main = draft.slide.objects.iter().find(|object| object.role == Some(sp_model::Role::primary()));
            draft.selected = main.map(|object| object.id);
            assert!(draft.selected.is_some(), "the template has a main box");
            let theme_edit = Some(crate::home::ThemeEdit::of(0, theme.clone()));
            let mut state = HomeState { editing: Some(draft), theme_edit, ..HomeState::default() };
            let tabs: Vec<&str> = tabs_for(&state).into_iter().map(|(_, name)| name).collect();
            assert_eq!(tabs, ["Text", "Box"]);
            let words = words_drawn(&mut state, &theme);
            for wanted in ["HOLDS", "Main"] {
                assert!(words.iter().any(|said| said == wanted), "{wanted} in {words:?}");
            }
            // From its menu the box takes what no other box holds; what another holds is greyed.
            let (ctx, mut time) = (crate::studio::tests::styled(), 0.0);
            let role = |state: &HomeState| {
                state.editing.as_ref().and_then(|draft| draft.selected_object()).and_then(|object| object.role.clone())
            };
            click_in(&ctx, &mut time, &mut state, &theme, "Main");
            click_in(&ctx, &mut time, &mut state, &theme, "Translation");
            assert_eq!(role(&state), Some(sp_model::Role::primary()), "the template has a translation box");
            // The click closed the menu: open it again.
            if frame_with((&ctx, &mut time), &mut state, &theme, vec![], "Translation reference").is_none() {
                click_in(&ctx, &mut time, &mut state, &theme, "Main");
            }
            click_in(&ctx, &mut time, &mut state, &theme, "Translation reference");
            assert_eq!(role(&state), Some(sp_model::Role::translation_reference()));
            // On a song's slide the row isn't there.
            let words = words_drawn(&mut editing_text(InspectorTab::Own, false, false), &theme);
            assert!(!words.iter().any(|said| said == "HOLDS"), "{words:?}");
        });
    }

    /// The Box tab's Rename, Duplicate and Delete, in every language: their names all on one
    /// line, or none drawn and three picture keys in their place. Never two lines, never some
    /// of each. The inspector is as wide at 1100 x 700 as at 1440 x 900, so one width is both.
    #[test]
    fn the_object_rows_keys_have_their_names_on_one_line_or_are_all_pictures() {
        let theme = Theme::builtin();
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                let names = [
                    t("inspector.rename", "Rename"),
                    t("inspector.duplicate", "Duplicate"),
                    t("inspector.delete", "Delete"),
                ];
                let words = word_rects(&mut editing_text(InspectorTab::Box, false, false), &theme);
                let caption = t("inspector.row.object", "Object").to_uppercase();
                let (_, row) = words.iter().find(|(said, _)| *said == caption).expect("the Object row");
                let on_row = |place: &Rect| (place.center().y - row.center().y).abs() < KEY / 2.0;
                let drawn: Vec<Option<Rect>> = names
                    .iter()
                    .map(|name| {
                        words.iter().find(|(said, place)| said == name && on_row(place)).map(|(_, place)| *place)
                    })
                    .collect();
                let named = drawn.iter().flatten().count();
                assert!(named == 0 || named == 3, "{language:?}: all names or none on the row: {drawn:?}");
                // No key's words under the row: it is one line.
                for name in names {
                    let below = words.iter().any(|(said, place)| said == name && place.min.y > row.max.y + 4.0);
                    assert!(!below, "{language:?}: {name} on a second line");
                }
                if named == 3 {
                    let ends = drawn[2].expect("Delete").max.x;
                    assert!(ends <= PANEL_WIDTH, "{language:?}: the names end at {ends}");
                }
                // Names only where the three fit the row at the usual size, measured with this
                // computer's own letters. The row's controls start where the row above's do:
                // 13 before the words on its first key.
                let forward = t("inspector.order.forward", "Forward");
                let (_, above) = words.iter().find(|(said, _)| said == forward).expect("the Order row");
                let room = (PANEL_WIDTH - (above.min.x - 13.0)).round();
                let mut fits = false;
                let measuring = crate::studio::tests::styled();
                let measured = measuring.run_ui(RawInput::default(), |ui| fits = object_keys_named(ui, room));
                measured.drop_without_applying_deltas();
                assert_eq!(named == 3, fits, "{language:?}: names where they fit {room}");
            });
        }
    }

    /// A slide being edited with a picture object for `file` added and picked, on its own tab or
    /// (with a gradient to give it, or `Some(None)` for none) on the Box tab.
    fn editing_picture(file: &str, gradient: Option<Option<sp_model::Gradient>>) -> HomeState {
        let mut state = editing_text(InspectorTab::Own, false, false);
        if let Some(draft) = state.editing.as_mut() {
            let mut object = sp_model::Object::shape(sp_model::Shape::Rectangle);
            object.kind = ObjectKind::Image { file: file.to_owned(), fit: sp_model::ImageFit::Fit };
            object.gradient = gradient.flatten();
            let id = object.id;
            draft.add(object);
            draft.select_only(Some(id));
        }
        state
    }

    #[test]
    fn a_video_has_its_own_tab_with_its_file_and_a_picture_has_no_line_about_playing() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let plays = "It plays from the start while the slide is on screen.";
            let mut video = editing_picture("/media/welcome loop.mp4", None);
            let tabs: Vec<&str> = tabs_for(&video).into_iter().map(|(_, name)| name).collect();
            assert_eq!(tabs, ["Video", "Box", "Animation"]);
            let words = words_drawn(&mut video, &theme);
            for wanted in ["FILE", "welcome loop.mp4", "FITS", plays] {
                assert!(words.iter().any(|said| said == wanted), "{wanted} in {words:?}");
            }
            let mut picture = editing_picture("/media/cross.png", None);
            let tabs: Vec<&str> = tabs_for(&picture).into_iter().map(|(_, name)| name).collect();
            assert_eq!(tabs, ["Picture", "Box", "Animation"]);
            let words = words_drawn(&mut picture, &theme);
            assert!(
                words.iter().any(|said| said == "cross.png") && !words.iter().any(|said| said == plays),
                "{words:?}"
            );
        });
    }

    #[test]
    fn a_picture_boxs_colour_is_behind_it_with_gradient_rows_only_for_a_saved_gradient() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let has = |words: &[String], wanted: &str| words.iter().any(|said| said == wanted);
            let words = words_drawn(&mut editing_picture("cross.png", Some(None)), &theme);
            assert!(has(&words, "BEHIND") && !has(&words, "FILL"), "{words:?}");
            assert!(!has(&words, "GRADIENT") && !has(&words, "TO"), "{words:?}");
            // One saved with a gradient, by an older build: its rows are there, so it can be
            // changed or taken off.
            let saved = sp_model::Gradient { kind: sp_model::GradientKind::Linear, ..Default::default() };
            let words = words_drawn(&mut editing_picture("cross.png", Some(Some(saved))), &theme);
            assert!(has(&words, "BEHIND") && has(&words, "GRADIENT") && has(&words, "TO"), "{words:?}");
            // Its Box tab has no row for the colour, nor has a text box's: a text box's is the
            // Text tab's Behind.
            for mut state in
                [editing_picture("cross.png", Some(Some(saved))), editing_text(InspectorTab::Own, false, false)]
            {
                state.inspector.tab = InspectorTab::Box;
                let words = words_drawn(&mut state, &theme);
                for gone in ["FILL", "BEHIND", "GRADIENT", "TO", "OUTLINE"] {
                    assert!(!has(&words, gone), "{gone} in {words:?}");
                }
            }
            let words = words_drawn(&mut editing_text(InspectorTab::Own, false, false), &theme);
            assert!(has(&words, "BEHIND") && !has(&words, "FILL"), "{words:?}");
        });
    }

    #[test]
    fn a_change_made_with_several_picked_goes_to_every_one() {
        let theme = Theme::builtin();
        let mut state = editing_shape(InspectorTab::Own, false);
        let draft = state.editing.as_mut().expect("editing");
        let first = draft.selected.expect("the star");
        draft.add_shape(sp_model::Shape::Ellipse);
        let second = draft.selected.expect("the ellipse");
        draft.select_only(Some(first));
        draft.toggle_selected(second);
        let before = draft.selected_object().cloned().expect("picked");
        // The Shape menu's change, and a build chosen on Animation.
        if let Some(object) = draft.selected_object_mut() {
            object.kind = ObjectKind::Shape { shape: sp_model::Shape::Triangle };
            object.build_in = Some(sp_model::Build::of(sp_model::BuildEffect::Zoom));
        }
        draft.share_change(&theme, &before);
        for object in draft.slide.objects.iter().filter(|object| [first, second].contains(&object.id)) {
            assert!(matches!(object.kind, ObjectKind::Shape { shape: sp_model::Shape::Triangle }), "{:?}", object.kind);
            assert_eq!(object.build_in.map(|build| build.effect), Some(sp_model::BuildEffect::Zoom));
        }
        // With a shape among them, the Box tab has no Fill or Outline: they are the shape's own.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let words = words_drawn(&mut editing_shape(InspectorTab::Box, true), &theme);
            assert!(!words.iter().any(|said| said == "FILL" || said == "OUTLINE"), "{words:?}");
        });
    }

    /// Every control the inspector draws for `state` that takes a click, in a column as wide as
    /// the window's.
    fn keys_drawn(state: &mut HomeState, theme: &Theme) -> Vec<Rect> {
        let ctx = crate::studio::tests::styled();
        let mut keys = Vec::new();
        for _ in 0..3 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_WIDTH, 2400.0))),
                ..RawInput::default()
            };
            ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
                    inspector(ui, state, theme);
                    keys.clear();
                    ui.ctx().viewport(|viewport| {
                        for (_, rects) in viewport.this_pass.widgets.layers() {
                            keys.extend(rects.iter().filter(|widget| widget.sense.senses_click()).map(|w| w.rect));
                        }
                    });
                });
            })
            .drop_without_applying_deltas();
        }
        keys
    }

    /// The six keys that line an object up, as the Box tab drew them: the one line that holds
    /// six keys of the design's width.
    fn line_up_keys(state: &mut HomeState, theme: &Theme) -> Vec<Rect> {
        let keys = keys_drawn(state, theme);
        let wide = |key: &&Rect| (key.width() - KEY_WIDE).abs() < 0.01 && (key.height() - KEY).abs() < 0.01;
        let mut lines: Vec<f32> = keys.iter().filter(wide).map(|key| key.min.y).collect();
        lines.sort_by(f32::total_cmp);
        lines.dedup();
        let six = lines.into_iter().find(|line| keys.iter().filter(wide).filter(|key| key.min.y == *line).count() == 6);
        let line = six.expect("a line of six keys");
        let mut row: Vec<Rect> = keys.iter().filter(wide).filter(|key| key.min.y == line).copied().collect();
        row.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        row
    }

    #[test]
    fn the_keys_that_line_an_object_up_are_the_designs_width_in_every_language() {
        let theme = Theme::builtin();
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                let row = line_up_keys(&mut editing_text(InspectorTab::Box, false, false), &theme);
                assert_eq!(row.len(), 6, "{language:?}");
                // Inside the column, none over the next, and all as wide as the design has them.
                assert!(row.last().is_some_and(|last| last.max.x <= PANEL_WIDTH + 0.1), "{language:?}: {row:?}");
                for pair in row.windows(2) {
                    assert!(pair[1].min.x >= pair[0].max.x + 1.0 - 0.01, "{language:?}: {pair:?}");
                }
            });
        }
    }

    #[test]
    fn a_picture_boxs_gradient_set_to_none_moves_no_row() {
        let theme = Theme::builtin();
        let saved = sp_model::Gradient { kind: sp_model::GradientKind::Linear, ..Default::default() };
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                let mut state = editing_picture("cross.png", Some(Some(saved)));
                let with = words_placed(&mut state, &theme);
                // Gradient set to None, as its menu does, with the box still picked.
                if let Some(object) = state.editing.as_mut().and_then(|draft| draft.selected_object_mut()) {
                    object.gradient = None;
                }
                let without = words_placed(&mut state, &theme);
                let gradient = t("inspector.row.gradient", "Gradient").to_uppercase();
                assert!(without.iter().any(|(words, ..)| *words == gradient), "{language:?}: its row stays");
                // Every caption is where it was (the menu itself now says another kind).
                let captions = |all: &[(String, i32, i32)]| -> Vec<(String, i32, i32)> {
                    let caption = |words: &str| words.chars().any(char::is_alphabetic) && words == words.to_uppercase();
                    all.iter().filter(|(words, ..)| caption(words)).cloned().collect()
                };
                assert_eq!(captions(&with), captions(&without), "{language:?}");
                // Picked again later, a box with no gradient has no rows for one.
                let mut fresh = editing_picture("cross.png", Some(None));
                let words = words_placed(&mut fresh, &theme);
                assert!(!words.iter().any(|(words, ..)| *words == gradient), "{language:?}: {words:?}");
            });
        }
    }

    /// `state` with one of the inspector's panels open, as its key opens it.
    fn with_panel(mut state: HomeState, panel: Panel) -> HomeState {
        let picked = state.editing.as_ref().map(|draft| draft.selection()).unwrap_or_default();
        state.inspector.panel_for = (state.inspector.tab, picked);
        state.inspector.panel = Some(panel);
        state
    }

    /// The controls that take a click on the row captioned `caption` (as drawn, in capitals),
    /// left to right.
    fn row_keys(state: &mut HomeState, theme: &Theme, caption: &str) -> Vec<Rect> {
        let words = words_placed(state, theme);
        let (_, across, down) = words.iter().find(|(said, ..)| said == caption).unwrap_or_else(|| panic!("{caption}"));
        let (edge, middle) = (*across as f32 + 20.0, *down as f32 + 6.0);
        let on_row = |key: &Rect| key.min.x > edge && key.min.y - 1.0 <= middle && middle <= key.max.y + 1.0;
        let mut keys: Vec<Rect> = keys_drawn(state, theme).into_iter().filter(on_row).collect();
        keys.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        keys.dedup();
        keys
    }

    /// Clicks at `at` in the inspector drawn for `state` in `ctx`, and draws a frame after.
    fn click_point(ctx: &egui::Context, time: &mut f64, state: &mut HomeState, theme: &Theme, at: egui::Pos2) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with((ctx, time), state, theme, vec![], "");
        for events in [vec![egui::Event::PointerMoved(at)], vec![press(true)], vec![press(false)], vec![]] {
            frame_with((ctx, time), state, theme, events, "");
        }
    }

    #[test]
    fn a_panel_opens_over_the_rows_and_moves_none_in_any_language() {
        let theme = Theme::builtin();
        let panels = [
            Panel::TextGradient,
            Panel::TextOutline,
            Panel::TextShadow,
            Panel::Spacing,
            Panel::Lines,
            Panel::BehindGradient,
        ];
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                for select in [false, true] {
                    let closed = words_placed(&mut editing_text(InspectorTab::Own, false, select), &theme);
                    for panel in panels {
                        let mut state = with_panel(editing_text(InspectorTab::Own, false, select), panel);
                        let open = words_placed(&mut state, &theme);
                        // Every word of the tab is where it was, under the panel or beside it.
                        for placed in &closed {
                            assert!(open.contains(placed), "{language:?} {panel:?}: {placed:?} moved");
                        }
                        assert!(open.len() > closed.len(), "{language:?} {panel:?}: the panel has rows of its own");
                        assert_eq!(state.inspector.panel, Some(panel), "{language:?}: it stays open while it is used");
                        // And the panel, with all in it, is inside the column.
                        for key in keys_drawn(&mut state, &theme) {
                            assert!(
                                key.max.x <= PANEL_WIDTH + 0.5,
                                "{language:?} {panel:?}: {key:?} is past the column"
                            );
                        }
                    }
                }
            });
        }
    }

    #[test]
    fn a_panel_closes_with_its_key_a_click_outside_or_another_pick() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let (ctx, mut time) = (crate::studio::tests::styled(), 0.0);
            let mut state = editing_text(InspectorTab::Own, false, false);
            // The dots at the end of the Words row open Lines; again, they close it.
            let dots = *row_keys(&mut state, &theme, "WORDS").last().expect("the row's keys");
            click_point(&ctx, &mut time, &mut state, &theme, dots.center());
            assert_eq!(state.inspector.panel, Some(Panel::Lines));
            click_point(&ctx, &mut time, &mut state, &theme, dots.center());
            assert_eq!(state.inspector.panel, None, "its key again closes it");
            // Open again: a menu in it is used without closing it; a click outside it closes it.
            click_point(&ctx, &mut time, &mut state, &theme, dots.center());
            click_in(&ctx, &mut time, &mut state, &theme, "None");
            click_in(&ctx, &mut time, &mut state, &theme, "Bullets");
            assert_eq!(state.inspector.panel, Some(Panel::Lines), "choosing from its menu leaves it open");
            click_in(&ctx, &mut time, &mut state, &theme, "FONT");
            assert_eq!(state.inspector.panel, None, "a click outside closes it");
            // Another tab, or another box picked, closes one too.
            let mut state = with_panel(editing_text(InspectorTab::Own, false, false), Panel::Spacing);
            state.inspector.tab = InspectorTab::Box;
            words_placed(&mut state, &theme);
            assert_eq!(state.inspector.panel, None, "another tab closes it");
        });
    }

    #[test]
    fn the_text_tabs_behind_row_sets_the_boxs_own_fill_and_its_ramp_the_gradient() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let (ctx, mut time) = (crate::studio::tests::styled(), 0.0);
            let mut state = editing_text(InspectorTab::Own, false, false);
            let picked = |state: &HomeState| {
                state.editing.as_ref().and_then(|draft| draft.selected_object().cloned()).expect("a box picked")
            };
            let before = picked(&state);
            assert_eq!((before.fill, before.gradient), (None, None));
            // None, the eight colours, any colour, and last the ramp.
            let keys = row_keys(&mut state, &theme, "BEHIND");
            assert_eq!(keys.len(), 11, "{keys:?}");
            // White: the box's own fill, part see-through as a new fill is. The words' look,
            // with the band behind their lines, is not touched.
            click_point(&ctx, &mut time, &mut state, &theme, keys[1].center());
            let filled = picked(&state);
            assert_eq!(filled.fill, Some(Rgba { r: 255, g: 255, b: 255, a: 160 }));
            assert_eq!(filled.gradient, None);
            assert_eq!(filled.kind, before.kind, "the text's own look is as it was");
            // The ramp opens the gradient's panel; a kind chosen there is the box's gradient.
            click_point(&ctx, &mut time, &mut state, &theme, keys[10].center());
            assert_eq!(state.inspector.panel, Some(Panel::BehindGradient));
            click_in(&ctx, &mut time, &mut state, &theme, "Color");
            click_in(&ctx, &mut time, &mut state, &theme, "Linear Gradient");
            let ramped = picked(&state);
            assert_eq!(ramped.gradient.map(|gradient| gradient.kind), Some(sp_model::GradientKind::Linear));
            assert_eq!(ramped.fill, filled.fill);
            assert_eq!(state.inspector.panel, Some(Panel::BehindGradient), "the panel stays open");
        });
    }

    /// The captions of the rows the inspector draws for `state`, top to bottom.
    fn captions_drawn(state: &mut HomeState, theme: &Theme) -> Vec<String> {
        let caption = |words: &str| words.chars().any(char::is_alphabetic) && words == words.to_uppercase();
        let mut rows: Vec<(i32, String)> = words_placed(state, theme)
            .into_iter()
            .filter(|(words, across, _)| *across == 0 && caption(words))
            .map(|(words, _, down)| (down, words))
            .collect();
        rows.sort();
        rows.into_iter().map(|(_, words)| words).collect()
    }

    #[test]
    fn the_box_tab_has_the_same_seven_rows_for_every_kind() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let seven = ["PLACE", "ALIGN", "ROTATE", "OPACITY", "EFFECTS", "ORDER", "OBJECT"];
            let boxed = |mut state: HomeState| {
                state.inspector.tab = InspectorTab::Box;
                state
            };
            for (what, mut state, outline) in [
                ("text", boxed(editing_text(InspectorTab::Own, false, false)), true),
                ("picture", boxed(editing_picture("cross.png", None)), true),
                ("video", boxed(editing_picture("clip.mp4", None)), true),
                // A shape's outline is on its own tab: Effects has Shadow alone.
                ("shape", boxed(editing_shape(InspectorTab::Own, false)), false),
                ("a shape and a text box", boxed(editing_shape(InspectorTab::Own, true)), false),
            ] {
                let rows = captions_drawn(&mut state, &theme);
                let rows: Vec<&str> = rows.iter().map(String::as_str).filter(|row| !row.contains("OBJECTS")).collect();
                assert_eq!(rows, seven, "{what}");
                let words = words_drawn(&mut state, &theme);
                assert!(words.iter().any(|said| said == "Shadow"), "{what}: {words:?}");
                assert_eq!(words.iter().any(|said| said == "Outline"), outline, "{what}: {words:?}");
            }
        });
        // Opening Outline's or Shadow's panel moves none of them, in any language.
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                let closed = words_placed(&mut editing_text(InspectorTab::Box, false, false), &theme);
                for panel in [Panel::BoxOutline, Panel::BoxShadow] {
                    let mut state = with_panel(editing_text(InspectorTab::Box, false, false), panel);
                    let open = words_placed(&mut state, &theme);
                    for placed in &closed {
                        assert!(open.contains(placed), "{language:?} {panel:?}: {placed:?} moved");
                    }
                    assert!(open.len() > closed.len(), "{language:?} {panel:?}: the panel has rows of its own");
                    for key in keys_drawn(&mut state, &theme) {
                        assert!(key.max.x <= PANEL_WIDTH + 0.5, "{language:?} {panel:?}: {key:?} is past the column");
                    }
                }
            });
        }
    }

    #[test]
    fn the_box_tabs_outline_switches_a_line_on_at_four_points_and_off() {
        let theme = Theme::builtin();
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let (ctx, mut time) = (crate::studio::tests::styled(), 0.0);
            let mut state = editing_text(InspectorTab::Box, false, false);
            let stroke = |state: &HomeState| {
                state.editing.as_ref().and_then(|draft| draft.selected_object()).and_then(|object| object.stroke)
            };
            assert_eq!(stroke(&state), None);
            click_in(&ctx, &mut time, &mut state, &theme, "Outline");
            assert_eq!(stroke(&state), Some(sp_model::StrokeLook { width: 4.0, color: Rgba::WHITE }));
            click_in(&ctx, &mut time, &mut state, &theme, "Outline");
            assert_eq!(stroke(&state), None, "Outline again takes the line away");
            // The dots beside it open its panel: Width and Colour.
            let keys = row_keys(&mut state, &theme, "EFFECTS");
            click_point(&ctx, &mut time, &mut state, &theme, keys[1].center());
            assert_eq!(state.inspector.panel, Some(Panel::BoxOutline));
            let words = words_drawn(&mut state, &theme);
            assert!(words.iter().any(|said| said == "WIDTH") && words.iter().any(|said| said == "COLOUR"), "{words:?}");
        });
    }

    #[test]
    fn a_fill_saved_with_any_kind_of_box_is_kept_and_has_a_row_on_its_own_tab() {
        let theme = Theme::builtin();
        let fill = Rgba { r: 20, g: 40, b: 60, a: 200 };
        let gradient = sp_model::Gradient {
            kind: sp_model::GradientKind::Linear,
            to: Rgba { r: 1, g: 2, b: 3, a: 255 },
            angle: 30.0,
        };
        crate::i18n::with_language(crate::i18n::Language::English, || {
            for (what, state, rows) in [
                ("text", editing_text(InspectorTab::Own, false, false), vec!["BEHIND"]),
                ("shape", editing_shape(InspectorTab::Own, false), vec!["FILL", "GRADIENT", "TO"]),
                ("picture", editing_picture("cross.png", None), vec!["BEHIND", "GRADIENT", "TO"]),
                ("video", editing_picture("clip.mp4", None), vec!["BEHIND", "GRADIENT", "TO"]),
            ] {
                // As a file written by an earlier version has it: the fill and its gradient on
                // the object, read back from the song's own form.
                let mut state = state;
                let draft = state.editing.as_mut().expect("editing");
                if let Some(object) = draft.selected_object_mut() {
                    object.fill = Some(fill);
                    object.gradient = Some(gradient);
                }
                let written = serde_json::to_string(&draft.slide).expect("written");
                draft.slide = serde_json::from_str(&written).expect("read");
                let before = draft.selected_object().cloned().expect("picked");
                assert_eq!((before.fill, before.gradient), (Some(fill), Some(gradient)), "{what}");
                // Every tab drawn: nothing is changed by drawing, and its own tab has the rows.
                for tab in [InspectorTab::Own, InspectorTab::Box, InspectorTab::Animation] {
                    state.inspector.tab = tab;
                    let captions = captions_drawn(&mut state, &theme);
                    if tab == InspectorTab::Own {
                        for row in &rows {
                            assert!(captions.iter().any(|caption| caption == row), "{what}: {row} in {captions:?}");
                        }
                    }
                    let after = state.editing.as_ref().and_then(|draft| draft.selected_object().cloned());
                    assert_eq!(after.as_ref(), Some(&before), "{what} {tab:?}: drawing changes nothing");
                }
            }
        });
    }

    #[test]
    fn the_slide_panels_group_row_keeps_its_name_and_change_apart_inside_the_column_in_every_language() {
        let theme = Theme::builtin();
        for language in LANGUAGES {
            crate::i18n::with_language(language, || {
                for name in ["Chorus", "A group with a name far too long for the row it is on"] {
                    let mut state = editing_text(InspectorTab::Own, false, false);
                    if let Some(draft) = state.editing.as_mut() {
                        draft.select_only(None);
                    }
                    state.edited_slide_group = Some((name.to_owned(), Rgba { r: 200, g: 80, b: 180, a: 255 }));
                    let words = word_rects(&mut state, &theme);
                    let change = t("inspector.slide.group.change", "Change\u{2026}");
                    let key = words.iter().find(|(said, _)| said == change).map(|(_, place)| *place);
                    let key = key.unwrap_or_else(|| panic!("{language:?}: {change} in {words:?}"));
                    // Change… ends inside the column, at its right; the name, cut short where
                    // it is long, ends before it.
                    assert!(key.max.x <= PANEL_WIDTH + 0.5, "{language:?}: {key:?}");
                    assert!(key.min.x > PANEL_WIDTH / 2.0, "{language:?}: Change… is at the row's right: {key:?}");
                    let said = words.iter().find(|(said, place)| {
                        said.starts_with(&name[..5]) && (place.center().y - key.center().y).abs() < 12.0
                    });
                    let (_, said) = said.unwrap_or_else(|| panic!("{language:?}: {name} in {words:?}"));
                    assert!(said.max.x <= key.min.x, "{language:?} {name}: {said:?} runs into {key:?}");
                    for control in keys_drawn(&mut state, &theme) {
                        assert!(control.max.x <= PANEL_WIDTH + 0.5, "{language:?}: {control:?} is past the column");
                    }
                }
            });
        }
    }

    #[test]
    fn no_control_reaches_past_the_inspectors_right_edge() {
        let theme = Theme::builtin();
        for (tab, spans, select) in [
            (InspectorTab::Own, false, false),
            (InspectorTab::Own, true, false),
            (InspectorTab::Own, true, true),
            (InspectorTab::Box, false, false),
            (InspectorTab::Animation, false, false),
        ] {
            let mut state = editing_text(tab, spans, select);
            let edge = PANEL_WIDTH;
            let widest = widest(&mut state, &theme);
            assert!(
                widest <= edge + 0.1,
                "{tab:?} spans {spans} selected {select}: ends at {widest}, the edge is {edge}"
            );
        }
    }

    #[test]
    fn an_ndi_source_box_without_ndi_has_the_key_that_opens_the_install_window() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = egui::Context::default();
            let data = std::env::temp_dir().join(format!("softpresenter-inspector-ndi-{}", std::process::id()));
            let mut download = crate::ndi_download::NdiDownload::in_folder(data);
            let mut view = crate::ndi_in::NdiView::default();
            let (mut source, mut fit, mut quality, mut sound) =
                (String::new(), sp_model::ImageFit::ALL[0], sp_model::NdiQuality::Full, false);
            let object = sp_model::Uuid::new_v4();
            let mut draw = |ui: &mut egui::Ui, installed: bool, download: &mut crate::ndi_download::NdiDownload| {
                ui.set_width(PANEL_WIDTH);
                ndi_source(
                    ui,
                    object,
                    (&mut source, &mut fit, &mut quality, &mut sound),
                    &mut view,
                    installed,
                    download,
                    &[],
                );
            };
            // With NDI here there is no such key.
            let words = crate::ndi_download::testing::drawn(&ctx, Vec::new(), &mut |ui| draw(ui, true, &mut download));
            assert!(!words.iter().any(|(text, _)| text.contains("Install NDI")), "{words:?}");
            // Without it: the line that says so, and the key.
            let words = crate::ndi_download::testing::drawn(&ctx, Vec::new(), &mut |ui| draw(ui, false, &mut download));
            assert!(words.iter().any(|(text, _)| text.ends_with("so this box has nothing to show yet.")), "{words:?}");
            let clicked = crate::ndi_download::testing::click(&ctx, "Install NDI\u{2026}", &mut |ui| {
                draw(ui, false, &mut download)
            });
            assert!(clicked, "the key is drawn");
            assert_eq!(download.shown_in(), Some(crate::ndi_download::Place::Home));
        });
    }
}
