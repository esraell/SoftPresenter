//! The stage layouts, edited in the Edit view under Stage: the layouts stage screens can be
//! drawn with, each a set of boxes
//! (the words on screen, the next slide, the time…) placed on a picture of the screen. A box
//! is moved by dragging it and resized by dragging its corner.

use egui::{Align2, Color32, FontId, Frame, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use sp_model::{Align, Rgba, SLIDE_HEIGHT, SLIDE_WIDTH, SlideRect, ThemeTemplate, VAlign};

use crate::home::HomeState;
use crate::stage::{ITEMS, item_box, item_name, item_sample};
use crate::studio::{ACCENT, HOVER, LINE, TEXT, TEXT_DIM, TEXT_FAINT};

/// The smallest a box can be made, in slide units.
const MIN_SIDE: f32 = 40.0;

/// Makes sure there is a layout to edit and that the layout and the box chosen exist. Called
/// before the three parts are drawn.
pub fn prepare(state: &mut HomeState) {
    if state.stage_layouts.is_empty() {
        state.stage_layouts = crate::stage::builtin_layouts();
    }
    let (layout_at, item_at) = state.stage_edit;
    let layout_at = layout_at.min(state.stage_layouts.len() - 1);
    let item_at = item_at.filter(|at| *at < state.stage_layouts[layout_at].boxes.len());
    state.stage_edit = (layout_at, item_at);
}

/// The layouts, with + (a new one), Duplicate and − under them: the Edit view's left column,
/// under its switch.
pub fn list(ui: &mut egui::Ui, state: &mut HomeState) {
    let (layout_at, _) = state.stage_edit;
    let mut changed = false;
    egui::Panel::bottom("stage layout buttons").resizable(false).frame(Frame::new()).show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button("+").on_hover_text("A new layout").clicked() {
                let taken = |name: &str| state.stage_layouts.iter().any(|layout| layout.name == name);
                let name = (1..).map(|n| format!("Layout {n}")).find(|name| !taken(name)).unwrap_or_default();
                state.stage_layouts.push(crate::stage::new_layout(name));
                state.stage_edit = (state.stage_layouts.len() - 1, None);
                changed = true;
            }
            let can_remove = state.stage_layouts.len() > 1;
            if ui
                .add_enabled(can_remove, egui::Button::new("−"))
                .on_hover_text("Delete this layout; screens using it use the first one")
                .clicked()
            {
                state.stage_layouts.remove(layout_at);
                state.stage_edit = (layout_at.saturating_sub(1), None);
                changed = true;
            }
            if ui.button("Duplicate").clicked() {
                let original = &state.stage_layouts[layout_at];
                let copy = ThemeTemplate {
                    id: sp_model::Uuid::new_v4(),
                    name: format!("{} copy", original.name),
                    boxes: original.boxes.clone(),
                    objects: original.objects.clone(),
                    songs: original.songs,
                    bible: false,
                };
                state.stage_layouts.insert(layout_at + 1, copy);
                state.stage_edit = (layout_at + 1, None);
                changed = true;
            }
        });
    });
    ui.label(egui::RichText::new("LAYOUTS").size(11.0).color(TEXT_FAINT));
    ui.add_space(6.0);
    for (index, layout) in state.stage_layouts.iter().enumerate() {
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        if index == state.stage_edit.0 {
            ui.painter().rect_filled(row, 6.0, ACCENT.gamma_multiply(0.35));
        } else if response.hovered() {
            ui.painter().rect_filled(row, 6.0, HOVER);
        }
        ui.painter().text(
            row.left_center() + vec2(10.0, 0.0),
            Align2::LEFT_CENTER,
            layout.name.as_str(),
            FontId::proportional(13.0),
            TEXT,
        );
        if response.clicked() {
            state.stage_edit = (index, None);
        }
    }
    state.stage_layouts_changed |= changed;
}

/// The chosen box: what it is, its size, colour and alignment; and boxes to add. It goes in the
/// right column, where the inspector is for a slide, and scrolls when the room is short.
pub fn item(ui: &mut egui::Ui, state: &mut HomeState) {
    let (layout_at, _) = state.stage_edit;
    let mut changed = false;
    egui::ScrollArea::vertical().id_salt("stage item").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(6.0);
        let layout = &mut state.stage_layouts[layout_at];
        ui.menu_button("Add Item", |ui| {
            for (role, name, _) in ITEMS {
                let there = layout.boxes.iter().any(|item| item.role.0 == role);
                if ui.add_enabled(!there, egui::Button::new(name)).clicked() {
                    let rect = SlideRect { x: 560.0, y: 440.0, w: 800.0, h: 200.0 };
                    layout.boxes.push(item_box(role, rect, 72.0, Rgba::WHITE, Align::Center, VAlign::Middle));
                    state.stage_edit.1 = Some(layout.boxes.len() - 1);
                    changed = true;
                    ui.close();
                }
            }
        });
        ui.add_space(10.0);
        let Some(item) = state.stage_edit.1.and_then(|at| layout.boxes.get_mut(at)) else {
            ui.label(egui::RichText::new("Click a box to change it.").size(12.0).color(TEXT_DIM));
            return;
        };
        ui.label(egui::RichText::new(item_name(&item.role)).size(15.0).color(TEXT).strong());
        ui.add_space(8.0);
        egui::Grid::new("stage item settings").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label(egui::RichText::new("Size").color(TEXT_DIM));
            changed |= ui.add(egui::DragValue::new(&mut item.size).range(12.0..=400.0).suffix(" pt")).changed();
            ui.end_row();
            ui.label(egui::RichText::new("Colour").color(TEXT_DIM));
            let mut color = Color32::from_rgba_unmultiplied(item.fill.r, item.fill.g, item.fill.b, item.fill.a);
            if ui.color_edit_button_srgba(&mut color).changed() {
                let [r, g, b, a] = color.to_srgba_unmultiplied();
                item.fill = Rgba { r, g, b, a };
                changed = true;
            }
            ui.end_row();
            ui.label(egui::RichText::new("Across").color(TEXT_DIM));
            ui.horizontal(|ui| {
                for (align, name) in [(Align::Left, "Left"), (Align::Center, "Centre"), (Align::Right, "Right")] {
                    if ui.add(egui::Button::selectable(item.align == align, name)).clicked() {
                        item.align = align;
                        changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label(egui::RichText::new("Down").color(TEXT_DIM));
            ui.horizontal(|ui| {
                for (valign, name) in [(VAlign::Top, "Top"), (VAlign::Middle, "Middle"), (VAlign::Bottom, "Bottom")] {
                    if ui.add(egui::Button::selectable(item.valign == valign, name)).clicked() {
                        item.valign = valign;
                        changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label(egui::RichText::new("Place").color(TEXT_DIM));
            ui.label(
                egui::RichText::new(format!(
                    "{:.0}, {:.0}  ·  {:.0} × {:.0}",
                    item.rect.x, item.rect.y, item.rect.w, item.rect.h
                ))
                .monospace()
                .size(11.5)
                .color(TEXT_DIM),
            );
            ui.end_row();
        });
        item.min_size = (item.size * 0.4).max(18.0);
        ui.add_space(10.0);
        if ui.button("Remove Item").clicked()
            && let Some(at) = state.stage_edit.1
        {
            layout.boxes.remove(at);
            state.stage_edit.1 = None;
            changed = true;
        }
    });
    state.stage_layouts_changed |= changed;
}

/// The layout's name, and a picture of the screen with its boxes on it: the middle column.
pub fn canvas(ui: &mut egui::Ui, state: &mut HomeState) {
    let (layout_at, item_at) = state.stage_edit;
    let mut changed = false;
    let layout = &mut state.stage_layouts[layout_at];
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Name").color(TEXT_DIM));
        changed |= ui.add(egui::TextEdit::singleline(&mut layout.name).desired_width(260.0)).changed();
    });
    ui.add_space(10.0);
    let width = ui.available_width().min((ui.available_height() - 30.0) * SLIDE_WIDTH / SLIDE_HEIGHT);
    let (screen, _) = ui.allocate_exact_size(vec2(width, width * SLIDE_HEIGHT / SLIDE_WIDTH), Sense::hover());
    let scale = width / SLIDE_WIDTH;
    let painter = ui.painter_at(screen);
    painter.rect_filled(screen, 4.0, Color32::BLACK);
    painter.rect_stroke(screen, 4.0, Stroke::new(1.0, LINE), StrokeKind::Outside);
    let to_screen =
        |rect: SlideRect| Rect::from_min_size(screen.min + vec2(rect.x, rect.y) * scale, vec2(rect.w, rect.h) * scale);
    let mut picked = None;
    for (index, item) in layout.boxes.iter_mut().enumerate() {
        let place = to_screen(item.rect);
        let selected = item_at == Some(index);
        // The box's stand-in words, at its size and colour, where its alignment puts them.
        let ink = Color32::from_rgba_unmultiplied(item.fill.r, item.fill.g, item.fill.b, item.fill.a);
        let (x, anchor_x) = match item.align {
            Align::Left => (place.min.x + 4.0, egui::Align::Min),
            Align::Right => (place.max.x - 4.0, egui::Align::Max),
            Align::Center | Align::Justify => (place.center().x, egui::Align::Center),
        };
        let (y, anchor_y) = match item.valign {
            VAlign::Top => (place.min.y + 2.0, egui::Align::Min),
            VAlign::Bottom => (place.max.y - 2.0, egui::Align::Max),
            VAlign::Middle => (place.center().y, egui::Align::Center),
        };
        let size = (item.size * scale).clamp(6.0, 120.0);
        painter.with_clip_rect(place).text(
            pos2(x, y),
            Align2([anchor_x, anchor_y]),
            item_sample(&item.role),
            FontId::proportional(size),
            ink,
        );
        let edge = if selected { Stroke::new(2.0, ACCENT) } else { Stroke::new(1.0, Color32::from_gray(90)) };
        painter.rect_stroke(place, 2.0, edge, StrokeKind::Inside);
        painter.text(
            place.left_top() + vec2(4.0, 3.0),
            Align2::LEFT_TOP,
            item_name(&item.role),
            FontId::proportional(10.0),
            Color32::from_gray(150),
        );
        // Dragging the box moves it, kept on the screen.
        let body = ui.interact(place, ui.scope_id().with(("stage box", layout_at, index)), Sense::click_and_drag());
        if body.clicked() || body.drag_started() {
            picked = Some(index);
        }
        if body.dragged() {
            let moved = body.drag_delta() / scale;
            item.rect.x = (item.rect.x + moved.x).clamp(0.0, SLIDE_WIDTH - item.rect.w).round();
            item.rect.y = (item.rect.y + moved.y).clamp(0.0, SLIDE_HEIGHT - item.rect.h).round();
            changed = true;
        }
    }
    // The chosen box's corner resizes it.
    if let Some(item) = item_at.and_then(|at| layout.boxes.get_mut(at)) {
        let corner = to_screen(item.rect).right_bottom();
        let handle = Rect::from_center_size(corner, vec2(12.0, 12.0));
        painter.rect_filled(handle, 2.0, ACCENT);
        let grip = ui.interact(handle, ui.scope_id().with(("stage resize", layout_at)), Sense::drag());
        if grip.dragged() {
            let grown = grip.drag_delta() / scale;
            item.rect.w = (item.rect.w + grown.x).clamp(MIN_SIDE, SLIDE_WIDTH - item.rect.x).round();
            item.rect.h = (item.rect.h + grown.y).clamp(MIN_SIDE, SLIDE_HEIGHT - item.rect.y).round();
            changed = true;
        }
        if grip.hovered() || grip.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        }
    }
    if picked.is_some() {
        state.stage_edit.1 = picked;
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "Drag a box to move it; drag its corner to resize it. Each stage screen picks its layout in the Stage tab.",
        )
        .size(11.5)
        .color(TEXT_FAINT),
    );
    state.stage_layouts_changed |= changed;
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_item_has_a_name_and_stand_in_words() {
        for (role, name, sample) in super::ITEMS {
            let role = sp_model::Role(role.to_owned());
            assert_eq!(super::item_name(&role), name);
            assert_eq!(super::item_sample(&role), sample);
            assert!(!sample.is_empty());
        }
    }
}
