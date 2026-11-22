//! The Screens window's rows for a screen played out of a DeckLink card: the format it plays
//! in. Kept here, apart from the rest of the window, so they can move as they are when the
//! settings are laid out another way.

use egui::Color32;

use super::{Output, Screen};
use crate::studio::TEXT_DIM;

/// Shown when the device isn't there, or can't do what is asked: orange, as a warning.
const WARNING: Color32 = Color32::from_rgb(255, 170, 120);

const MODE_HINT: &str = "The video format the card sends. Set it to what the switcher, recorder or projector it goes \
     to expects: they must match exactly. 1080i59.94 is the usual HD format where power is 60 Hz, 1080i50 where it is \
     50 Hz. A 720p format is scaled down from SoftPresenter's 1920 × 1080 picture.";

/// The DeckLink rows for the screen, under its output. True when anything was changed.
pub fn decklink_rows(ui: &mut egui::Ui, screen: &mut Screen, found: &sp_decklink::Found) -> bool {
    let Output::DeckLink(out) = &mut screen.output else { return false };
    let device = found.find(out.id, &out.device);
    let mut changed = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Format").color(TEXT_DIM)).on_hover_text(MODE_HINT);
        ui.add_space(8.0);
        egui::ComboBox::from_id_salt(("decklink mode", screen.id))
            .selected_text(out.mode.as_str())
            .width(150.0)
            .show_ui(ui, |ui| {
                for mode in device.into_iter().flat_map(|device| device.offered()) {
                    if ui.selectable_label(out.mode == mode.name, mode.name.as_str()).clicked() && out.mode != mode.name
                    {
                        out.mode = mode.name.clone();
                        changed = true;
                    }
                }
            });
    });
    let warning = match device {
        None => Some(format!("{} isn't connected now. The screen plays out of it as soon as it is.", out.device)),
        Some(device) if device.mode(&out.mode).is_none() => {
            Some(format!("{} has no format {}. Choose one above.", device.name, out.mode))
        }
        Some(_) => None,
    };
    if let Some(warning) = warning {
        ui.label(egui::RichText::new(warning).size(11.5).color(WARNING));
    }
    changed
}
