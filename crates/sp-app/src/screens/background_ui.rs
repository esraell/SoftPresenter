//! The Screens window's rows for a screen's background: solid or see-through (and how a
//! see-through screen sends its soft edges), or the key of another screen. Kept here, apart
//! from the rest of the window, so they can move as they are when the settings are laid out
//! another way.

use egui::{Color32, Rect, vec2};

use super::{Alpha, Kind, Look, Screen, key_trouble};
use crate::studio::TEXT_DIM;

/// Shown on a key that cannot follow its fill: orange, as a warning.
const WARNING: Color32 = Color32::from_rgb(255, 170, 120);

const BACKGROUND_HINT: &str = "Solid: as usual. See-through: clear wherever nothing is drawn, so that a video switcher or \
     streaming software can lay the words over a camera picture.";
const COLOURS_HINT: &str = "How a see-through screen sends the colour of a pixel that is only partly solid, such as \
     the soft edge of a letter. Straight, the usual (and NDI's own way): the colour as it is. Premultiplied: the colour \
     already darkened by how see-through it is. Choose the one your switcher or streaming software expects; if the \
     edges of the words look dark or have a fringe, try the other.";
const KEY_HINT: &str = "For a video switcher that takes the words and their shape as two inputs, a fill and a key: \
     this screen shows the chosen screen's key, white where that screen is solid, black where it is clear, and greys \
     at the soft edges. Send the two on two displays, or on two NDI names.";

/// The background rows for the screen at `index` (an audience screen), under its look. True
/// when anything was changed. `status` is what each screen is doing, and whether it is going
/// out, as the Screens window shows it.
pub fn background_rows(ui: &mut egui::Ui, screens: &mut [Screen], index: usize, status: &[(String, bool)]) -> bool {
    let Some(screen) = screens.get(index).filter(|screen| screen.kind == Kind::Audience) else { return false };
    let mut look = screen.look;
    let (id, name) = (screen.id, screen.name.clone());
    // The screens this one can be the key of: the other audience screens that are not keys.
    let fills: Vec<(sp_model::Uuid, String)> = screens
        .iter()
        .filter(|other| other.id != id && other.kind == Kind::Audience && other.look.key_of.is_none())
        .map(|other| (other.id, other.name.clone()))
        .collect();
    ui.add_space(4.0);
    if look.key_of.is_none() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Background").color(TEXT_DIM)).on_hover_text(BACKGROUND_HINT);
            ui.add_space(8.0);
            if ui.add(egui::Button::selectable(look.see_through.is_none(), "Solid")).clicked() {
                look.see_through = None;
            }
            let see_through = ui.add(egui::Button::selectable(look.see_through.is_some(), "See-through"));
            if see_through.on_hover_text(BACKGROUND_HINT).clicked() && look.see_through.is_none() {
                look.see_through = Some(Alpha::default());
            }
        });
        if let Some(alpha) = look.see_through {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Soft edges").color(TEXT_DIM)).on_hover_text(COLOURS_HINT);
                ui.add_space(8.0);
                for (kind, label) in [(Alpha::Straight, "Straight"), (Alpha::Premultiplied, "Premultiplied")] {
                    if ui.add(egui::Button::selectable(alpha == kind, label)).on_hover_text(COLOURS_HINT).clicked() {
                        look.see_through = Some(kind);
                    }
                }
            });
        }
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Key").color(TEXT_DIM)).on_hover_text(KEY_HINT);
        ui.add_space(8.0);
        let fill_name = |fill: sp_model::Uuid| {
            fills.iter().find(|(have, _)| *have == fill).map_or("a screen that has gone".to_owned(), |(_, n)| n.clone())
        };
        let shown = look.key_of.map_or("Not a key".to_owned(), |fill| format!("Key of \"{}\"", fill_name(fill)));
        egui::ComboBox::from_id_salt(("screen key", index))
            .selected_text(shown)
            .width(220.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(look.key_of.is_none(), "Not a key").clicked() {
                    look.key_of = None;
                }
                for (fill, fill_name) in &fills {
                    if ui.selectable_label(look.key_of == Some(*fill), format!("Key of \"{fill_name}\"")).clicked() {
                        look.key_of = Some(*fill);
                    }
                }
            })
            .response
            .on_hover_text(KEY_HINT);
    });
    // A line in plain words of what the choice does, or why a key is not doing it.
    let shown: Vec<bool> = status.iter().map(|(_, shown)| *shown).collect();
    let before = screens[index].look;
    screens[index].look = look;
    let (note, warn) = match (look.key_of, look.see_through) {
        (Some(fill), _) => match key_trouble(screens, index, &shown) {
            Some(trouble) => (trouble, true),
            None => {
                let fill_name = screens.iter().find(|other| other.id == fill).map_or("", |other| other.name.as_str());
                (
                    format!(
                        "Shows the key of \"{fill_name}\": white where it is solid, black where it is clear, made from the very picture \"{fill_name}\" shows, so the two always show the same moment, fades included. This screen's own look is not used."
                    ),
                    false,
                )
            }
        },
        (None, Some(_)) => (
            format!(
                "\"{name}\" is clear wherever nothing is drawn. It leaves out the media behind the words, the themes' \
                 background colours and pictures, and its colour; the words, props, messages and announcements stay. \
                 Over NDI the clear parts go with the picture. On a display, a second screen can be its key. The \
                 Program preview shows the room's picture, not this one."
            ),
            false,
        ),
        (None, None) => (String::new(), false),
    };
    if !note.is_empty() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(note).size(11.5).color(if warn { WARNING } else { TEXT_DIM }));
    }
    look != before
}

/// Marks the small picture of a screen in the Screens window with its background: a
/// chequerboard behind a see-through screen's name (where nothing is drawn it is clear), so it
/// can be told at a glance from a solid one.
pub fn paint_background(painter: &egui::Painter, frame: Rect, look: &Look) {
    if look.see_through.is_none() || look.key_of.is_some() {
        return;
    }
    let inside = frame.shrink(3.0);
    let painter = painter.with_clip_rect(inside);
    let square = 12.0;
    let (across, down) = ((inside.width() / square).ceil() as usize, (inside.height() / square).ceil() as usize);
    for row in 0..down {
        for column in (row % 2..across).step_by(2) {
            let at = inside.min + vec2(column as f32 * square, row as f32 * square);
            painter.rect_filled(Rect::from_min_size(at, vec2(square, square)), 0.0, Color32::from_gray(52));
        }
    }
}
