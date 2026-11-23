//! The Edit view's right-hand panel: three tabs (Shape, Text, Build) of settings for the
//! selected object. Controls that do nothing yet are still shown where they belong, and say so
//! when the pointer rests on them.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, pos2, vec2};
use sp_model::{
    Align as TextAlign, BoxStyle, Caps, LineTransform, ObjectKind, Rgba, SLIDE_HEIGHT, SLIDE_WIDTH, Scaling, TextStyle,
    Theme, VAlign,
};
use sp_render::object_frame;

use crate::editor::MIN_SIDE;
use crate::home::HomeState;

const PANEL: Color32 = crate::studio::PANEL;
/// What is chosen: grey, as the rest of the app.
const BLUE: Color32 = crate::studio::SELECTED;
const CONTROL: Color32 = Color32::from_rgb(45, 45, 45);
const FIELD: Color32 = Color32::from_rgb(20, 20, 20);
const RULE: Color32 = Color32::from_rgb(50, 50, 50);
const INK: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(150);
/// Space between the panel's edge and its controls.
const SIDE: f32 = 12.0;
/// Space above and below the controls in the scrolling body.
const TOP: f32 = 8.0;
const BOTTOM: f32 = 10.0;
const ROW: f32 = 24.0;
/// The width of the panel and of the controls inside it. Rows are sized from this, not from
/// the space they find, so a row can never push the panel wider.
const PANEL_WIDTH: f32 = crate::studio::RIGHT_WIDTH - 20.0;
const INNER: f32 = PANEL_WIDTH - 2.0 * SIDE;
/// The width of the column a group's tick box and title sit in.
const TICK: f32 = 98.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorTab {
    Shape,
    #[default]
    Text,
    Build,
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
}

/// How far A− and A+ move a size, in slide units.
const SIZE_STEP: f32 = 4.0;

const WEIGHTS: [(u16, &str); 9] = [
    (100, "Thin"),
    (200, "Extra Light"),
    (300, "Light"),
    (400, "Regular"),
    (500, "Medium"),
    (600, "Semibold"),
    (700, "Bold"),
    (800, "Heavy"),
    (900, "Black"),
];

fn weight_name(weight: u16) -> &'static str {
    WEIGHTS.iter().min_by_key(|(w, _)| w.abs_diff(weight)).map_or("Regular", |(_, name)| name)
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
    ui.painter().rect_filled(rect.expand2(vec2(SIDE, 0.0)), 0.0, RULE);
    ui.add_space(9.0);
}

/// A number box with a small caption under it.
fn number(
    ui: &mut egui::Ui,
    value: &mut f32,
    width: f32,
    caption: &str,
    make: impl FnOnce(egui::DragValue) -> egui::DragValue,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add_sized([width, ROW - 2.0], make(egui::DragValue::new(value).speed(1.0).max_decimals(0)));
        if !caption.is_empty() {
            ui.allocate_ui_with_layout(vec2(width, 12.0), Layout::top_down(Align::Center), |ui| {
                ui.label(egui::RichText::new(caption).size(10.5).color(DIM));
            });
        }
    });
}

/// A row of equal segments, like one wide button cut into parts. `draw` paints segment `i`.
/// Returns the segment clicked, if any.
fn segments(
    ui: &mut egui::Ui,
    salt: &str,
    width: f32,
    count: usize,
    is_on: impl Fn(usize) -> bool,
    draw: impl Fn(&egui::Painter, usize, Rect, Color32),
) -> Option<usize> {
    let (bar, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
    ui.painter().rect_filled(bar, 6.0, CONTROL);
    let each = bar.width() / count as f32;
    let mut clicked = None;
    for index in 0..count {
        let cell = Rect::from_min_size(pos2(bar.min.x + each * index as f32, bar.min.y), vec2(each, bar.height()));
        let response = ui.interact(cell, ui.scope_id().with((salt, index)), Sense::click());
        if is_on(index) {
            ui.painter().rect_filled(cell, 6.0, BLUE);
        } else if response.hovered() {
            ui.painter().rect_filled(cell.shrink(2.0), 5.0, Color32::from_gray(64));
        }
        draw(ui.painter(), index, cell, INK);
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

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

/// A tick box with a bold title, as each group of options starts with.
fn tick(ui: &mut egui::Ui, on: &mut bool, title: &str) -> egui::Response {
    ui.checkbox(on, egui::RichText::new(title).size(13.0).strong().color(INK))
}

/// A dim label on the left and a control filling the rest of the row.
/// The spacing pop-up under the gear: room between letters, between lines, and inside the
/// box's edges.
fn spacing_popup(ui: &mut egui::Ui, wanted: &mut sp_model::BoxStyle) {
    ui.set_width(250.0);
    egui::Grid::new("text spacing").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Character Spacing").size(12.5).color(DIM));
        ui.add(egui::DragValue::new(&mut wanted.letter_spacing).speed(0.2).range(-20.0..=100.0).max_decimals(1))
            .on_hover_text("Extra room between letters, in points at the box's font size");
        ui.end_row();
        ui.label(egui::RichText::new("Line Height").size(12.5).color(DIM));
        ui.add(egui::DragValue::new(&mut wanted.line_height).speed(0.01).range(0.5..=3.0).fixed_decimals(2))
            .on_hover_text("The distance from one line to the next, as a multiple of the font size");
        ui.end_row();
        ui.label(egui::RichText::new("Paragraph Spacing").size(12.5).color(DIM));
        ui.add(egui::DragValue::new(&mut wanted.paragraph_spacing).speed(0.5).range(0.0..=400.0).max_decimals(0))
            .on_hover_text("Extra room after each paragraph (each line break typed), in points");
        ui.end_row();
        ui.label(egui::RichText::new("Baseline").size(12.5).color(DIM));
        ui.add(egui::DragValue::new(&mut wanted.baseline).speed(0.5).range(-400.0..=400.0).max_decimals(0))
            .on_hover_text("Raise the text by this many points (lower it with a minus)");
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Margins").size(12.5).color(DIM));
    ui.horizontal(|ui| {
        for (margin, name) in wanted.margins.iter_mut().zip(["Left", "Top", "Right", "Bottom"]) {
            ui.vertical(|ui| {
                ui.add(egui::DragValue::new(margin).speed(1.0).range(0.0..=400.0).max_decimals(0));
                ui.label(egui::RichText::new(name).size(10.5).color(DIM));
            });
        }
    });
    ui.add_space(8.0);
    if ui.button("Reset Spacing").clicked() {
        wanted.letter_spacing = 0.0;
        wanted.line_height = sp_model::LINE_HEIGHT;
        wanted.paragraph_spacing = 0.0;
        wanted.baseline = 0.0;
        wanted.margins = [0.0; 4];
    }
}

fn labelled(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui, f32)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(104.0, ROW), Layout::right_to_left(Align::Center), |ui| {
            ui.label(egui::RichText::new(label).size(12.5).color(DIM));
        });
        // What is left after the label (egui has already stepped past the gap after it), so the
        // control ends on the edge.
        let width = ui.available_width().max(40.0);
        add(ui, width);
    });
}

/// A pop-up list showing its current choice.
fn choice<T: PartialEq + Copy>(ui: &mut egui::Ui, salt: &str, width: f32, value: &mut T, options: &[(T, &str)]) {
    let shown = options.iter().find(|(v, _)| v == value).map_or("", |(_, name)| name);
    egui::ComboBox::from_id_salt(salt)
        .selected_text(shown)
        .width(width)
        .wrap_mode(egui::TextWrapMode::Truncate)
        .show_ui(ui, |ui| {
            for (option, name) in options {
                ui.selectable_value(value, *option, *name);
            }
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

/// A colour well that opens a pop-up: a grid of swatches to pick from, and the full colour
/// picker under it for any other colour.
fn colour(ui: &mut egui::Ui, value: &mut Rgba, hint: &str) {
    colour_split(ui, value, hint, None);
}

/// A colour well; with `split` (the words picked have two colours) it shows the two, cut
/// corner to corner, and no swatch is marked as the one in use.
fn colour_split(ui: &mut egui::Ui, value: &mut Rgba, hint: &str, split: Option<Rgba>) {
    let (well, response) = ui.allocate_exact_size(vec2(54.0, ROW - 2.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(well, 4.0, to_color32(*value));
    if let Some(other) = split {
        // The lower right half, from the top right corner to the bottom left.
        let lower = vec![well.right_top(), well.right_bottom(), well.left_bottom()];
        painter.add(egui::Shape::convex_polygon(lower, to_color32(other), egui::Stroke::NONE));
    }
    painter.rect_stroke(well, 4.0, egui::Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
    let response = response.on_hover_text(hint);
    egui::Popup::from_toggle_button_response(&response)
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

/// A colour, or a gradient from it to a second colour: a choice of which, and the gradient's
/// colour and angle when there is one.
fn fill_kind(ui: &mut egui::Ui, salt: &str, gradient: &mut sp_model::Gradient) {
    let options: Vec<(sp_model::GradientKind, &str)> =
        sp_model::GradientKind::ALL.iter().map(|kind| (*kind, kind.name())).collect();
    choice(ui, salt, 140.0, &mut gradient.kind, &options);
    if gradient.on() {
        ui.add_space(6.0);
        colour(ui, &mut gradient.to, "The colour it runs to");
        if gradient.kind == sp_model::GradientKind::Linear {
            ui.add_space(6.0);
            ui.add(egui::DragValue::new(&mut gradient.angle).speed(1.0).range(0.0..=359.0).suffix("°").max_decimals(0))
                .on_hover_text("The way it runs: 0° left to right, 90° top to bottom");
        }
    }
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
        if theme_slide || layout {
            // One section has a plain heading, not a lone tab.
            let (heading, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
            crate::studio::paint_spaced(
                ui,
                heading.left_center(),
                Align2::LEFT_CENTER,
                if layout { "LAYOUT" } else { "THEME SLIDE" },
                crate::studio::fixed_semibold(11.5),
                crate::studio::TEXT_DIM,
                0.6,
            );
        } else {
            let tab_names = [InspectorTab::Shape, InspectorTab::Text, InspectorTab::Build];
            // A screen picture on a stage layout has no words: its settings are on the Shape tab.
            let picture = state
                .editing
                .as_ref()
                .and_then(|draft| draft.selected_object())
                .is_some_and(|object| matches!(object.kind, ObjectKind::Screen { .. }));
            if stage && picture && state.inspector.tab == InspectorTab::Text {
                state.inspector.tab = InspectorTab::Shape;
            }
            let tabs = crate::studio::Tabs::plain(&["Shape", "Text", "Build"]);
            let width = ui.available_width();
            let (place, _) =
                ui.allocate_exact_size(vec2(width, crate::studio::tab_switch_height(&tabs)), Sense::hover());
            let open = tab_names.iter().position(|tab| *tab == state.inspector.tab);
            let (_, clicked) = crate::studio::tab_switch(ui, "inspector tab", place.min, width, &tabs, open);
            if let Some(clicked) = clicked {
                state.inspector.tab = tab_names[clicked];
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
                            .add(egui::Button::selectable(open, "Slide Notes"))
                            .on_hover_text("Notes for whoever runs the slides; the audience never sees them")
                            .clicked()
                        {
                            state.inspector.notes_open = !open;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Done").on_hover_text("Go back to the Show view").clicked() {
                            leave = true;
                        }
                        if !for_theme {
                            ui.checkbox(&mut state.live_edit, "Live Edit").on_hover_text(
                                "When on, changes to a slide that is on screen appear on the output as they are made",
                            );
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
                    ui.label(egui::RichText::new("SLIDE NOTES").size(10.5).strong().color(DIM));
                    let mut notes = draft.slide.notes.clone().unwrap_or_default();
                    let field = egui::TextEdit::multiline(&mut notes)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text("Notes for whoever runs the slides");
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
            // The controls get a fixed width, SIDE in from each edge, and the column is given
            // that and the height they used, never what they measured: a control wider than
            // its room cannot widen the column and take the right margin with it.
            let room = ui.available_rect_before_wrap();
            let inside = Rect::from_min_max(
                pos2(room.min.x + SIDE, room.min.y + TOP),
                pos2(room.max.x - SIDE, room.min.y + TOP + 100_000.0),
            );
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
                    } else {
                        // With several objects selected, a change made here is made to them all.
                        let before = state.editing.as_ref().and_then(|draft| draft.selected_object().cloned());
                        match state.inspector.tab {
                            InspectorTab::Shape => shape_tab(ui, state, theme),
                            InspectorTab::Text => text_tab(ui, state, theme),
                            InspectorTab::Build => build_tab(ui, state),
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
    leave
}

/// The panel for a theme as a whole, shown in the Theme Editor when no box is selected.
fn theme_slide_panel(ui: &mut egui::Ui, state: &mut HomeState) {
    let picture_request = &mut state.theme_picture_request;
    let Some(edit) = state.theme_edit.as_mut() else { return };
    let before = (edit.theme.name.clone(), edit.theme.background, edit.theme.background_picture.clone());
    ui.add_space(4.0);
    labelled_left(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut edit.theme.name).desired_width(f32::INFINITY));
    });
    labelled_left(ui, "Size", |ui| {
        ui.label(egui::RichText::new("1920 × 1080").size(12.5).color(INK))
            .on_hover_text("Every slide is 1920 by 1080; other sizes are not made yet.");
    });
    rule(ui);
    ui.horizontal(|ui| {
        let mut filled = edit.theme.background.is_some();
        if tick(ui, &mut filled, "Background").on_hover_text("A colour behind everything on the slide").changed() {
            edit.theme.background = filled.then_some(Rgba { r: 20, g: 30, b: 60, a: 255 });
        }
        if let Some(background) = edit.theme.background.as_mut() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| colour(ui, background, "Background colour"));
        }
    });
    rule(ui);
    // A picture behind every slide drawn with the theme, while no media is showing.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Picture").size(13.0).strong().color(INK))
            .on_hover_text("A picture behind every slide drawn with this theme, while no media is showing");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if edit.theme.background_picture.is_some() {
                if ui.button("Remove").clicked() {
                    edit.theme.background_picture = None;
                }
                if ui.button("Change…").clicked() {
                    *picture_request = true;
                }
            } else if ui.button("Choose…").on_hover_text("Choose a picture from a file").clicked() {
                *picture_request = true;
            }
        });
    });
    rule(ui);
    let about = "A song's slide uses, of the templates marked Songs, the one that suits what is on it: with the main language and a translation, the first with a box for both; with one language, the first with no translation box. Bible slides use the template marked Bible. Right-click a template to change what it is for. Songs and Bible slides are drawn with the main theme; right-click a theme to make it the main one.";
    ui.label(egui::RichText::new(about).size(11.5).color(DIM));
    if before != (edit.theme.name.clone(), edit.theme.background, edit.theme.background_picture.clone()) {
        // A theme always has a name.
        if edit.theme.name.trim().is_empty() {
            edit.theme.name = before.0;
        }
        edit.changed = true;
    }
}

/// A bold label on the left and a control filling the rest of the row.
fn labelled_left(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(44.0, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(44.0);
            ui.label(egui::RichText::new(label).size(12.5).strong().color(INK));
        });
        add(ui);
    });
}

fn nothing_selected(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Click something on the slide to select it.").size(12.5).color(Color32::from_gray(190)),
    );
    ui.label(
        egui::RichText::new(
            "Drag to move it, drag a handle to resize it, press Delete to remove it. Arrow keys nudge.",
        )
        .size(11.5)
        .color(DIM),
    );
}

/// A web page box's address: what is typed is kept while the field has the keyboard, and taken
/// when it is left (with https:// put in front of a bare name). An address that isn't https is
/// refused, and says so.
fn web_address(ui: &mut egui::Ui, object: sp_model::Uuid, url: &mut String) {
    ui.label(egui::RichText::new("Web Page").size(12.5).strong().color(INK));
    ui.add_space(2.0);
    let id = ui.make_persistent_id(("web address", object));
    let bad_id = id.with("refused");
    let mut typed = ui.data_mut(|data| data.get_temp::<String>(id)).unwrap_or_else(|| url.clone());
    let field = ui.add(egui::TextEdit::singleline(&mut typed).hint_text("https://…").desired_width(INNER));
    if ui.data_mut(|data| data.remove_temp::<bool>(egui::Id::unique("web address focus"))).is_some() {
        field.request_focus();
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
    let words = if ui.data_mut(|data| data.get_temp::<bool>(bad_id)).unwrap_or(false) {
        egui::RichText::new("Only web pages whose address starts with https:// can be shown.")
            .color(crate::studio::TALLY)
    } else {
        egui::RichText::new("The page shows, live, while the slide is on screen.").color(crate::studio::TEXT_DIM)
    };
    ui.label(words.size(11.5));
}

fn shape_tab(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) {
    let stage = state.editing.as_ref().is_some_and(crate::stage_editor::is_stage) && state.theme_edit.is_none();
    let for_theme = state.theme_edit.is_some() || stage;
    let screens = if stage { state.screens.clone() } else { Vec::new() };
    // Undo and Redo here take the Edit view's steps in turn, as Command-Z does.
    let history = (state.can_undo(false), state.can_undo(true));
    let menu_asked = &mut state.menu_asked;
    let lock_shape = &mut state.inspector.lock_shape;
    let Some(draft) = state.editing.as_mut() else { return };
    let themed = draft.selected_object().map(|object| object_frame(&draft.slide, object, theme));
    let (Some(frame), Some(object)) = (themed, draft.selected_object_mut()) else {
        return nothing_selected(ui);
    };
    // A screen picture: which screen it shows, its corners and its label.
    if stage && matches!(object.kind, ObjectKind::Screen { .. }) {
        crate::stage_editor::screen_settings(ui, object, &screens);
        rule(ui);
    }
    let mut edited = frame;
    // Spreading the selected objects out evenly is done once this object is let go of.
    let mut spread: Option<bool> = None;

    // Line the object up with the slide's sides or middle.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let each = ((INNER - 24.0) / 3.0).floor();
        let across = segments(ui, "align across", each, 3, |_| false, |p, i, cell, ink| align_mark(p, cell, i, ink));
        ui.add_space(12.0);
        let down = segments(ui, "align down", each, 3, |_| false, |p, i, cell, ink| align_mark(p, cell, i + 3, ink));
        ui.add_space(12.0);
        // Spread three or more selected objects out evenly: across, or down.
        let picked = segments(
            ui,
            "distribute",
            each,
            2,
            |_| false,
            |p, i, cell, ink| {
                align_mark(p, cell, [1, 4][i], ink);
            },
        );
        if let Some(index) = picked {
            spread = Some(index == 0);
        }
        match across {
            Some(0) => edited.x = 0.0,
            Some(1) => edited.x = (SLIDE_WIDTH - edited.w) / 2.0,
            Some(_) => edited.x = SLIDE_WIDTH - edited.w,
            None => {}
        }
        match down {
            Some(0) => edited.y = 0.0,
            Some(1) => edited.y = (SLIDE_HEIGHT - edited.h) / 2.0,
            Some(_) => edited.y = SLIDE_HEIGHT - edited.h,
            None => {}
        }
    });
    rule(ui);

    let title = |ui: &mut egui::Ui, text: &str| {
        ui.allocate_ui_with_layout(vec2(72.0, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(72.0);
            ui.label(egui::RichText::new(text).size(12.5).strong().color(INK));
        });
    };
    ui.horizontal_top(|ui| {
        title(ui, "Position");
        number(ui, &mut edited.x, 84.0, "X", |d| d);
        ui.add_space(16.0);
        number(ui, &mut edited.y, 84.0, "Y", |d| d);
    });
    ui.horizontal_top(|ui| {
        title(ui, "Size");
        let shape = frame.w / frame.h.max(1.0);
        number(ui, &mut edited.w, 84.0, "Width", |d| d.range(MIN_SIDE..=10_000.0));
        let lock = ui
            .add_sized(
                [10.0, ROW - 2.0],
                egui::Button::selectable(*lock_shape, egui::RichText::new("=").size(11.0)).frame(false),
            )
            .on_hover_text("Keep the box's shape when its width or height changes");
        if lock.clicked() {
            *lock_shape = !*lock_shape;
        }
        number(ui, &mut edited.h, 84.0, "Height", |d| d.range(MIN_SIDE..=10_000.0));
        if *lock_shape {
            if edited.w != frame.w {
                edited.h = (edited.w / shape).max(MIN_SIDE);
            } else if edited.h != frame.h {
                edited.w = (edited.h * shape).max(MIN_SIDE);
            }
        }
    });
    ui.horizontal_top(|ui| {
        title(ui, "Transform");
        // Mirror the object left to right, or top to bottom.
        let flips = [object.flip_h, object.flip_v];
        let flipped = segments(
            ui,
            "flip",
            84.0,
            2,
            |i| flips[i],
            |p, i, cell, ink| {
                let c = cell.center();
                let (a, b) =
                    if i == 0 { (vec2(-6.0, 4.0), vec2(6.0, 4.0)) } else { (vec2(-4.0, -6.0), vec2(-4.0, 6.0)) };
                let tip = if i == 0 { vec2(0.0, -5.0) } else { vec2(5.0, 0.0) };
                p.add(egui::Shape::convex_polygon(vec![c + a, c + b, c + tip], ink, Stroke::NONE));
            },
        );
        match flipped {
            Some(0) => object.flip_h = !object.flip_h,
            Some(_) => object.flip_v = !object.flip_v,
            None => {}
        }
        ui.add_space(16.0);
        number(ui, &mut object.rotation, 84.0, "Rotation", |d| {
            d.speed(0.5).range(-360.0..=360.0).suffix("°").max_decimals(1)
        });
    });
    if edited != frame {
        object.frame = Some(edited);
    }
    rule(ui);

    // A web page: its address. The page is laid out at the box's size.
    let object_id = object.id;
    if let sp_model::ObjectKind::Web { url } = &mut object.kind {
        web_address(ui, object_id, url);
        rule(ui);
    }

    // A picture: how it sits in its place.
    if let sp_model::ObjectKind::Image { fit, .. } = &mut object.kind {
        ui.horizontal(|ui| {
            title(ui, "Picture");
            let options: Vec<(sp_model::ImageFit, &str)> =
                sp_model::ImageFit::ALL.iter().map(|fit| (*fit, fit.name())).collect();
            choice(ui, "picture fit", 120.0, fit, &options);
        });
        rule(ui);
    }

    ui.horizontal(|ui| {
        title(ui, "Opacity");
        ui.spacing_mut().slider_width = INNER - 72.0 - 14.0;
        ui.add(egui::Slider::new(&mut object.opacity, 0.0..=1.0).show_value(false));
    });
    rule(ui);

    ui.horizontal(|ui| {
        let mut filled = object.fill.is_some();
        if tick(ui, &mut filled, "Fill").on_hover_text("Fill the box with a colour behind its text").changed() {
            object.fill = filled.then_some(Rgba { r: 0, g: 0, b: 0, a: 160 });
        }
        if let Some(fill) = object.fill.as_mut() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| colour(ui, fill, "Fill colour"));
        }
    });
    if object.fill.is_some() {
        ui.add_space(4.0);
        let mut gradient = object.gradient.unwrap_or_default();
        ui.horizontal(|ui| fill_kind(ui, "shape fill kind", &mut gradient));
        object.gradient = gradient.on().then_some(gradient);
    }
    rule(ui);
    // A line round the box or shape: 3 points of white to start with.
    ui.horizontal(|ui| {
        let mut stroked = object.stroke.is_some();
        if tick(ui, &mut stroked, "Stroke").on_hover_text("Draw a line round the box or shape").changed() {
            object.stroke = stroked.then_some(sp_model::StrokeLook { width: 3.0, color: Rgba::WHITE });
        }
        if let Some(line) = object.stroke.as_mut() {
            number(ui, &mut line.width, 84.0, "Width", |d| d.speed(0.2).range(0.0..=100.0).max_decimals(1));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| colour(ui, &mut line.color, "Stroke colour"));
        }
    });
    rule(ui);
    // A soft shadow under the box or shape: 75 % black, falling down and to the right.
    ui.horizontal_top(|ui| {
        let mut shadowed = object.shadow.is_some();
        if tick(ui, &mut shadowed, "Shadow").on_hover_text("Put a soft shadow under the box or shape").changed() {
            object.shadow = shadowed.then_some(sp_model::Shadow {
                color: Rgba::BLACK,
                opacity: 0.75,
                angle: 315.0,
                offset: 5.0,
                blur: 5.0,
            });
        }
        if let Some(look) = object.shadow.as_mut() {
            let mut percent = (look.opacity * 100.0).round();
            number(ui, &mut percent, 78.0, "Opacity", |d| d.range(0.0..=100.0).suffix("%"));
            look.opacity = percent / 100.0;
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| colour(ui, &mut look.color, "Shadow colour"));
        }
    });
    if let Some(look) = object.shadow.as_mut() {
        ui.horizontal_top(|ui| {
            let each = (INNER - 2.0 * 12.0) / 3.0;
            number(ui, &mut look.angle, each, "Angle", |d| d.range(0.0..=360.0).suffix("°"));
            ui.add_space(6.0);
            number(ui, &mut look.offset, each, "Offset", |d| d.speed(0.5).range(0.0..=200.0));
            ui.add_space(6.0);
            number(ui, &mut look.blur, each, "Blur", |d| d.speed(0.5).range(0.0..=200.0));
        });
    }
    rule(ui);

    // Not in the reference layout, but needed until objects can be managed from the slide itself.
    ui.label(egui::RichText::new("OBJECT").size(10.5).strong().color(DIM));
    ui.add(egui::TextEdit::singleline(&mut object.name).desired_width(f32::INFINITY).hint_text("Name"));
    let follows_theme =
        object.frame.is_none() && object.rotation == 0.0 && object.opacity == 1.0 && object.fill.is_none();
    if let Some(across) = spread {
        draft.distribute(theme, across);
    }
    ui.horizontal_wrapped(|ui| {
        let small = |label: &str| egui::Button::new(egui::RichText::new(label).size(12.0));
        for (words, redo, hint) in [
            ("Undo", false, crate::keys::hint("Undo the last change", "Z", false)),
            ("Redo", true, crate::keys::hint("Redo it", "Z", true)),
        ] {
            let can = if redo { history.1 } else { history.0 };
            let button = ui
                .add_enabled(can, small(words))
                .on_hover_text(hint)
                .on_disabled_hover_text(crate::home::nothing_to(redo));
            if button.clicked() {
                use crate::menubar::Action;
                *menu_asked = Some(if redo { Action::Redo } else { Action::Undo });
            }
        }
        if ui.add(small("Bring Forward")).on_hover_text("Move in front of the next object").clicked() {
            draft.restack(true);
        }
        if ui.add(small("Send Backward")).on_hover_text("Move behind the previous object").clicked() {
            draft.restack(false);
        }
        if ui.add(small("Delete")).on_hover_text("Remove this object from the slide (Delete)").clicked() {
            draft.delete_selected();
        }
        if !for_theme
            && ui
                .add_enabled(!follows_theme, small("Reset to theme"))
                .on_hover_text("Put this object back where the theme has it")
                .clicked()
        {
            draft.reset_to_theme();
        }
    });
}

fn text_tab(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) {
    let stage = state.editing.as_ref().is_some_and(crate::stage_editor::is_stage) && state.theme_edit.is_none();
    let for_theme = state.theme_edit.is_some();
    let fonts = state.fonts.clone();
    let timers: Vec<(sp_model::Uuid, String)> =
        state.timers.iter().map(|timer| (timer.id, timer.name.clone())).collect();
    let inspector = &mut state.inspector;
    let Some(draft) = state.editing.as_mut() else { return };
    let words_selected = draft.selected_words();
    let Some(object) = draft.selected_object_mut() else {
        return nothing_selected(ui);
    };
    // A stage item: what it shows, and that item's own settings (which timer, the clock's form).
    let stage_item = stage && crate::stage::item_of(object).is_some();
    if stage_item {
        crate::stage_editor::item_settings(ui, object, &timers);
        rule(ui);
    }
    let role = object.role.clone();
    let object_id = object.id;
    let ObjectKind::Text { text, style, spans } = &mut object.kind else {
        ui.label(
            egui::RichText::new("A shape has no text. Its colour and outline are on the Shape tab.")
                .size(12.0)
                .color(DIM),
        );
        return;
    };
    let Some(base) = theme.text_style(role.as_ref(), &TextStyle::default()) else { return };
    let picked = words_selected.filter(|(id, ..)| *id == object_id);
    // What the rows below change: the words picked, or the whole box.
    let heading = |ui: &mut egui::Ui, title: &str, note: &str| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).size(10.5).color(INK).strong());
            if !note.is_empty() {
                ui.label(egui::RichText::new(note).size(11.0).color(DIM));
            }
        });
    };
    match picked {
        Some((_, from, to)) => {
            let words: String = text.chars().skip(from).take(to - from).collect::<String>().replace('\n', " ");
            let words = if words.chars().count() > 26 {
                format!("\u{201c}{}\u{2026}\u{201d}", words.chars().take(25).collect::<String>().trim_end())
            } else {
                format!("\u{201c}{words}\u{201d}")
            };
            heading(ui, "SELECTED WORDS", &words);
        }
        None => heading(ui, "THE WHOLE BOX", "Select words in it to change only them"),
    }

    ui.horizontal(|ui| {
        let half = (ui.available_width() - 6.0) / 2.0;
        if ui
            .add_sized([half, ROW], egui::Button::new("Copy Style"))
            .on_hover_text("Remember how this text looks")
            .clicked()
        {
            inspector.copied_style = Some(style.clone());
        }
        let paste = ui
            .add_enabled_ui(inspector.copied_style.is_some(), |ui| {
                ui.add_sized([half, ROW], egui::Button::new("Paste Style"))
            })
            .inner
            .on_hover_text("Give this text the look that was copied")
            .on_disabled_hover_text(crate::home::COPY_A_STYLE);
        if paste.clicked()
            && let Some(copied) = &inspector.copied_style
        {
            *style = copied.clone();
        }
    });
    if !spans.is_empty()
        && ui
            .add_sized([ui.available_width(), ROW], egui::Button::new("Clear word styles"))
            .on_hover_text("Every word takes the box's look again")
            .clicked()
    {
        spans.clear();
        inspector.new_step = true;
    }
    ui.add_space(2.0);
    // The words are typed on the slide itself (double-click the box), not here.

    // With words selected on the slide the controls show, and change, those words; with none,
    // the whole box.
    let box_look = style.over(&base);
    let (shown, mix) = match words_selected.filter(|(id, ..)| *id == object_id) {
        Some((_, from, to)) => sp_model::look_of_range(spans, from, to, &box_look),
        None => sp_model::look_of_range(spans, 0, text.chars().count(), &box_look),
    };
    let mut wanted = shown.clone();
    text_look(ui, &mut wanted, &base, &fonts, inspector, mix, picked.is_some());
    if wanted != shown {
        let length = text.chars().count();
        match words_selected.filter(|(id, ..)| *id == object_id) {
            Some((_, from, to)) => {
                sp_model::restyle_range(spans, from, to, length, &box_look, |look| look.follow_change(&shown, &wanted));
                // What a word cannot have (alignment, shadow) goes to the box.
                let mut whole = wanted.clone();
                whole.font_family.clone_from(&box_look.font_family);
                (whole.size, whole.weight, whole.italic) = (box_look.size, box_look.weight, box_look.italic);
                (whole.underline, whole.strike, whole.fill) = (box_look.underline, box_look.strike, box_look.fill);
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

    if for_theme || stage_item {
        return;
    }
    // Linked text: the box shows something that changes, such as the time, in place of its words.
    rule(ui);
    let timer_name = |id: sp_model::Uuid| timers.iter().find(|(have, _)| *have == id).map(|(_, name)| name.as_str());
    let mut link = object.link;
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            let mut linked = link.is_some();
            if tick(ui, &mut linked, "Linked Text")
                .on_hover_text("Show something that changes, such as the time or a timer, instead of these words")
                .changed()
            {
                link = linked.then_some(sp_model::TextLink::Clock);
            }
        });
        if let Some(chosen) = link {
            let shown = match chosen {
                sp_model::TextLink::Timer { id } => format!("Timer: {}", timer_name(id).unwrap_or("(gone)")),
                other => other.name().to_owned(),
            };
            egui::ComboBox::from_id_salt("linked text").selected_text(shown).width(ui.available_width()).show_ui(
                ui,
                |ui| {
                    for option in sp_model::TextLink::SIMPLE {
                        if ui.add(egui::Button::selectable(chosen == option, option.name())).clicked() {
                            link = Some(option);
                        }
                    }
                    for (id, name) in &timers {
                        let option = sp_model::TextLink::Timer { id: *id };
                        if ui.add(egui::Button::selectable(chosen == option, format!("Timer: {name}"))).clicked() {
                            link = Some(option);
                        }
                    }
                },
            );
        }
    });
    if link != object.link {
        // The box's own words say what it is linked to, so its picture in the editor does too.
        let placeholder = text.is_empty() || text == "Text" || (text.starts_with('[') && text.ends_with(']'));
        match link {
            Some(chosen) if placeholder => {
                let name = match chosen {
                    sp_model::TextLink::Timer { id } => timer_name(id),
                    _ => None,
                };
                sp_model::edit_text(text, spans, 0, usize::MAX, &crate::links::placeholder(chosen, name));
            }
            // Unlinked: the words the link put there go too, and the box says "Text" again, as a
            // new one does. Words of its own were never replaced, so they stay.
            None if placeholder => sp_model::edit_text(text, spans, 0, usize::MAX, "Text"),
            _ => {}
        }
        if link.is_some() {
            spans.clear();
        }
        object.link = link;
    }
    if stage {
        return;
    }
    rule(ui);
    ui.horizontal(|ui| {
        let plain = style.is_default();
        let note = if plain { "Looks as the theme has it" } else { "Styled by hand on this slide" };
        ui.label(egui::RichText::new(note).size(11.5).color(DIM));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let reset = egui::Button::new(egui::RichText::new("Reset style").size(12.0));
            if ui
                .add_enabled(!plain, reset)
                .on_hover_text("Give this text the theme's look again; words you styled one by one keep theirs")
                .clicked()
            {
                *style = TextStyle::default();
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

/// The small heading over rows that act on the whole box.
fn heading_line(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).size(10.5).color(INK).strong());
}

/// The Text tab's controls for one look. `wanted` starts as the look the text has now.
fn text_look(
    ui: &mut egui::Ui,
    wanted: &mut BoxStyle,
    base: &BoxStyle,
    fonts: &[String],
    inspector: &mut InspectorState,
    mix: sp_model::SpanMix,
    words_picked: bool,
) {
    let full = ui.available_width();
    let font_menu = egui::ComboBox::from_id_salt("font family")
        .selected_text(if mix.font_family {
            egui::RichText::new("Mixed").italics().color(DIM)
        } else {
            egui::RichText::new(wanted.font_family.as_str())
        })
        .width(full)
        .height(360.0)
        .show_ui(ui, |ui| {
            // Type to narrow the list: the field has the keyboard as soon as the menu opens,
            // and Return takes the first font left.
            let search = ui.add(
                egui::TextEdit::singleline(&mut inspector.font_search)
                    .hint_text("Search fonts")
                    .desired_width(f32::INFINITY),
            );
            search.request_focus();
            let typed = inspector.font_search.trim().to_lowercase();
            // Fonts that start with what was typed come before those that only contain it.
            let mut found: Vec<&String> = fonts.iter().filter(|name| name.to_lowercase().starts_with(&typed)).collect();
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
                ui.label(egui::RichText::new("No font has that in its name.").color(DIM));
            }
            for family in found {
                if ui.selectable_label(wanted.font_family == *family, family.as_str()).clicked() {
                    wanted.font_family = family.clone();
                    inspector.font_search.clear();
                    ui.close();
                }
            }
        });
    // Testing aid: SOFTPRESENTER_CAPTURE_FONT_MENU=1 opens the font list once, for a capture.
    static MENU_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if std::env::var_os("SOFTPRESENTER_CAPTURE_FONT_MENU").is_some()
        && !MENU_OPENED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        egui::Popup::open_id(ui.ctx(), font_menu.response.id.with("popup"));
    }
    egui::ComboBox::from_id_salt("font weight").selected_text(weight_name(wanted.weight)).width(full).show_ui(
        ui,
        |ui| {
            for (weight, name) in WEIGHTS {
                ui.selectable_value(&mut wanted.weight, weight, name);
            }
        },
    );
    // Size on a row of its own, with the two steps beside it: every width is worked out from
    // the room there is, so the row is never wider than the column.
    ui.horizontal(|ui| {
        let gap = ui.spacing().item_spacing.x;
        let button = 28.0;
        // A button grows to its words and padding, so the padding is small enough that the
        // button's own size is the 28 it is given.
        ui.spacing_mut().button_padding.x = 2.0;
        let field = (full - 2.0 * (button + gap)).max(40.0);
        ui.add_sized(
            [field, ROW - 2.0],
            egui::DragValue::new(&mut wanted.size)
                .speed(1.0)
                .range(8.0..=600.0)
                .max_decimals(0)
                .custom_formatter(move |value, _| if mix.size { "Mixed".to_owned() } else { format!("{value:.0}") }),
        )
        .on_hover_text("Size, in slide units");
        for (label, by, hint) in
            [("A\u{2212}", -SIZE_STEP, "Every size one step smaller"), ("A+", SIZE_STEP, "Every size one step bigger")]
        {
            if ui
                .add_sized([button, ROW - 2.0], egui::Button::new(egui::RichText::new(label).size(11.0)))
                .on_hover_text(hint)
                .clicked()
            {
                inspector.size_step = by;
            }
        }
    });
    ui.horizontal(|ui| {
        let on = [
            wanted.weight >= 600 && !mix.weight,
            wanted.italic && !mix.italic,
            wanted.underline && !mix.underline,
            wanted.strike && !mix.strike,
        ];
        let halves = [mix.weight, mix.italic, mix.underline, mix.strike];
        let bar_start = ui.cursor().min;
        let pressed = segments(
            ui,
            "bius",
            112.0,
            4,
            |i| on[i],
            |p, i, cell, ink| {
                let font = FontId::proportional(13.5);
                let c = cell.center();
                p.text(c, Align2::CENTER_CENTER, ["B", "I", "U", "S"][i], font, ink);
                match i {
                    2 => {
                        p.line_segment([pos2(c.x - 5.0, c.y + 8.0), pos2(c.x + 5.0, c.y + 8.0)], Stroke::new(1.0, ink))
                    }
                    3 => p.line_segment([pos2(c.x - 6.0, c.y), pos2(c.x + 6.0, c.y)], Stroke::new(1.0, ink)),
                    _ => p.line_segment([c, c], Stroke::NONE),
                };
            },
        );
        // A style that some of the words have and some do not is half lit, its lower half in grey.
        for (index, half) in halves.into_iter().enumerate() {
            if half {
                let cell = Rect::from_min_size(
                    pos2(bar_start.x + 28.0 * index as f32, bar_start.y + ROW / 2.0),
                    vec2(28.0, ROW / 2.0),
                );
                ui.painter().rect_filled(cell, 4.0, Color32::from_rgba_unmultiplied(150, 150, 150, 120));
            }
        }
        match pressed {
            Some(0) => wanted.weight = if on[0] { 400 } else { 700 },
            Some(1) => wanted.italic = !wanted.italic,
            Some(2) => wanted.underline = !wanted.underline,
            Some(_) => wanted.strike = !wanted.strike,
            None => {}
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // Spacing: between letters, between lines, and inside the box's edges.
            let gear = ui.add(egui::Button::new("").min_size(vec2(80.0, ROW))).on_hover_text("Spacing");
            let mark = Rect::from_center_size(gear.rect.center(), vec2(14.0, 14.0));
            crate::icons::paint(ui.painter(), crate::icons::Icon::Gear, mark, INK);
            egui::Popup::from_toggle_button_response(&gear)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| spacing_popup(ui, wanted));
        });
    });
    labelled(ui, "Capitalisation", |ui, width| {
        let options = [
            (Caps::None, "None"),
            (Caps::AllCaps, "All Caps"),
            (Caps::Lowercase, "Lowercase"),
            (Caps::TitleCase, "Title Case"),
            (Caps::StartCase, "Start Case"),
        ];
        choice(ui, "caps", width, &mut wanted.caps, &options);
    });
    rule(ui);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Text Colour").size(13.0).color(INK));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            colour_split(ui, &mut wanted.fill, "Text colour", mix.fill.then_some(mix.other_fill).flatten())
        });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| fill_kind(ui, "text fill kind", &mut wanted.gradient));
    rule(ui);

    if words_picked {
        heading_line(ui, "THE WHOLE BOX");
    }
    labelled(ui, "Scaling", |ui, width| {
        let options = [
            (Scaling::None, "None"),
            (Scaling::TextUp, "Text up"),
            (Scaling::TextDown, "Text down"),
            (Scaling::TextUpOrDown, "Text up or down"),
            (Scaling::ContainerUpOrDown, "Container up or down"),
        ];
        choice(ui, "scaling", width, &mut wanted.scaling, &options);
    });
    labelled(ui, "Line Transform", |ui, width| {
        let options = [
            (LineTransform::None, "None"),
            (LineTransform::RemoveLineReturns, "Remove line returns"),
            (LineTransform::ReplaceLineReturns, "Replace line returns"),
            (LineTransform::OneWordPerLine, "One word per line"),
            (LineTransform::OneCharacterPerLine, "One character per line"),
        ];
        choice(ui, "line transform", width, &mut wanted.line_transform, &options);
    });
    rule(ui);

    let across = [TextAlign::Left, TextAlign::Center, TextAlign::Right, TextAlign::Justify];
    let now = wanted.align;
    if let Some(index) = segments(
        ui,
        "align",
        full,
        4,
        |i| across.get(i) == Some(&now),
        |p, i, cell, ink| {
            text_lines(p, cell, i, ink);
        },
    ) && let Some(align) = across.get(index)
    {
        wanted.align = *align;
    }
    let down = [VAlign::Top, VAlign::Middle, VAlign::Bottom];
    let now = wanted.valign;
    if let Some(index) =
        segments(ui, "valign", full, 3, |i| down[i] == now, |p, i, cell, ink| vertical_mark(p, cell, i, ink))
    {
        wanted.valign = down[index];
    }
    rule(ui);

    ui.horizontal_top(|ui| {
        let mut stroked = wanted.outline_width > 0.0;
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            if tick(ui, &mut stroked, "Stroke").changed() {
                wanted.outline_width = if stroked { base.outline_width.max(1.0) } else { 0.0 };
            }
        });
        if stroked {
            number(ui, &mut wanted.outline_width, 78.0, "Width", |d| d.speed(0.25).range(0.25..=60.0).max_decimals(1));
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                colour(ui, &mut wanted.outline_color, "Stroke colour")
            });
        }
    });
    rule(ui);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            tick(ui, &mut wanted.shadow, "Shadow");
        });
        if wanted.shadow {
            let mut percent = (wanted.shadow_look.opacity * 100.0).round();
            number(ui, &mut percent, 78.0, "Opacity", |d| d.range(0.0..=100.0).suffix("%"));
            wanted.shadow_look.opacity = percent / 100.0;
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                colour(ui, &mut wanted.shadow_look.color, "Shadow colour")
            });
        }
    });
    if wanted.shadow {
        ui.horizontal_top(|ui| {
            let each = (full - 2.0 * 12.0) / 3.0;
            let look = &mut wanted.shadow_look;
            number(ui, &mut look.angle, each, "Angle", |d| d.range(0.0..=360.0).suffix("°"));
            ui.add_space(6.0);
            number(ui, &mut look.offset, each, "Offset", |d| d.speed(0.5).range(0.0..=200.0));
            ui.add_space(6.0);
            number(ui, &mut look.blur, each, "Blur", |d| d.speed(0.5).range(0.0..=200.0));
        });
    }
    rule(ui);

    // A colour behind the text: a band under each line's words, or across the box.
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            tick(ui, &mut wanted.line_fill.on, "Background").on_hover_text("A colour behind the lines of text");
        });
        if wanted.line_fill.on {
            let width = (ui.available_width() - 44.0).max(60.0);
            choice(
                ui,
                "line fill",
                width,
                &mut wanted.line_fill.full_width,
                &[(false, "Lines Only"), (true, "Full Width")],
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                colour(ui, &mut wanted.line_fill.color, "The colour behind the text");
            });
        }
    });
    rule(ui);
    // Text that keeps moving through its box, as a ticker does.
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            let mut scrolling = wanted.scroll != sp_model::Scroll::None;
            if tick(ui, &mut scrolling, "Scrolling")
                .on_hover_text("The text keeps moving through its box, as a ticker does")
                .changed()
            {
                wanted.scroll = if scrolling { sp_model::Scroll::Left } else { sp_model::Scroll::None };
            }
        });
        if wanted.scroll != sp_model::Scroll::None {
            let picked = sp_model::Scroll::WAYS.iter().position(|way| *way == wanted.scroll).unwrap_or(0);
            let width = ui.available_width();
            if let Some(index) = segments(
                ui,
                "scroll direction",
                width,
                4,
                |i| i == picked,
                |p, i, cell, ink| {
                    p.text(
                        cell.center(),
                        Align2::CENTER_CENTER,
                        ["←", "→", "↑", "↓"][i],
                        FontId::proportional(14.0),
                        ink,
                    );
                },
            ) {
                wanted.scroll = sp_model::Scroll::WAYS[index];
            }
        }
    });
    rule(ui);
    // Bullets or numbers before each line.
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            ui.label(egui::RichText::new("List").size(13.0).strong().color(INK));
        });
        let options: Vec<(sp_model::ListStyle, &str)> =
            sp_model::ListStyle::ALL.iter().map(|list| (*list, list.name())).collect();
        choice(ui, "list", ui.available_width(), &mut wanted.list, &options);
    });
}

fn build_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    use sp_model::{Build, BuildEffect, Direction};
    let Some(draft) = state.editing.as_mut() else { return };
    // A stage screen shows its layout's objects at once, whatever comes on the slides.
    if crate::stage_editor::is_stage(draft) && state.theme_edit.is_none() {
        let said = "A stage layout's objects are on the stage screens all the time; builds are for slides and props.";
        ui.label(egui::RichText::new(said).size(12.0).color(DIM));
        return;
    }
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
            Some(format!("{name}: {}", build.effect.name(coming_in)))
        })
        .collect();
    let Some(object) = draft.selected_object_mut() else {
        return nothing_selected(ui);
    };
    let is_text = object.as_text().is_some();
    let label = |ui: &mut egui::Ui, text: &str| {
        ui.allocate_ui_with_layout(vec2(72.0, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(72.0);
            ui.label(egui::RichText::new(text).size(12.5).strong().color(INK));
        });
    };
    // Each menu fills what is left of its row after its label, as every inspector menu does.
    for coming_in in [true, false] {
        let build = if coming_in { &mut object.build_in } else { &mut object.build_out };
        ui.horizontal(|ui| {
            label(ui, if coming_in { "Build In" } else { "Build Out" });
            let mut effect = build.map(|build| build.effect);
            let mut options: Vec<(Option<BuildEffect>, &str)> = vec![(None, "None")];
            options.extend(BuildEffect::ALL.iter().map(|effect| (Some(*effect), effect.name(coming_in))));
            let room = ui.available_width();
            choice(ui, if coming_in { "build in" } else { "build out" }, room, &mut effect, &options);
            if effect != build.map(|build| build.effect) {
                *build =
                    effect.map(|effect| Build { effect, on_click: !coming_in, ..build.unwrap_or(Build::of(effect)) });
            }
        });
        if let Some(build) = build.as_mut() {
            if build.effect == BuildEffect::Fly {
                ui.horizontal(|ui| {
                    label(ui, if coming_in { "From" } else { "To" });
                    let sides = [
                        (Direction::FromLeft, "Left"),
                        (Direction::FromRight, "Right"),
                        (Direction::FromTop, "Top"),
                        (Direction::FromBottom, "Bottom"),
                    ];
                    let room = ui.available_width();
                    choice(
                        ui,
                        if coming_in { "build in side" } else { "build out side" },
                        room,
                        &mut build.direction,
                        &sides,
                    );
                });
            }
            ui.horizontal_top(|ui| {
                label(ui, "Timing");
                number(ui, &mut build.seconds, 72.0, "Seconds", |d| d.speed(0.05).range(0.0..=10.0).max_decimals(2));
                ui.add_space(8.0);
                number(ui, &mut build.delay, 72.0, "Delay", |d| d.speed(0.05).range(0.0..=30.0).max_decimals(2));
            });
            if coming_in {
                ui.horizontal(|ui| {
                    label(ui, "Starts");
                    let starts = [(false, "With the Slide"), (true, "On a Click")];
                    let room = ui.available_width();
                    choice(ui, "build in start", room, &mut build.on_click, &starts);
                });
                if is_text {
                    ui.horizontal(|ui| {
                        label(ui, "Delivery");
                        let ways = [(false, "All at Once"), (true, "By Paragraph")];
                        let room = ui.available_width();
                        choice(ui, "build in delivery", room, &mut build.by_paragraph, &ways);
                    });
                    if build.by_paragraph {
                        let hint = if build.on_click {
                            "Each click shows the next paragraph."
                        } else {
                            "Each paragraph follows the last after its time in seconds."
                        };
                        ui.label(egui::RichText::new(hint).size(11.5).color(DIM));
                    }
                }
            } else {
                ui.label(egui::RichText::new("Leaves when Next is clicked.").size(11.5).color(DIM));
            }
        }
        rule(ui);
    }
    // The order clicks set the slide's builds going in: builds in first, then builds out.
    ui.label(egui::RichText::new("Click Order").size(12.5).strong().color(INK));
    ui.add_space(4.0);
    if order.is_empty() {
        ui.label(
            egui::RichText::new("No builds wait for a click. Next goes straight to the next slide.")
                .size(11.5)
                .color(DIM),
        );
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

    #[test]
    fn no_control_reaches_past_the_inspectors_right_edge() {
        let theme = Theme::builtin();
        for (tab, spans, select) in [
            (InspectorTab::Text, false, false),
            (InspectorTab::Text, true, false),
            (InspectorTab::Text, true, true),
            (InspectorTab::Shape, false, false),
            (InspectorTab::Build, false, false),
        ] {
            let mut state = editing_text(tab, spans, select);
            let edge = PANEL_WIDTH - SIDE;
            let widest = widest(&mut state, &theme);
            assert!(
                widest <= edge + 0.1,
                "{tab:?} spans {spans} selected {select}: ends at {widest}, the edge is {edge}"
            );
        }
    }
}
