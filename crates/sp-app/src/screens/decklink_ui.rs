//! The Screens window's rows for a screen played out of a DeckLink card: the format it plays
//! in, and how it uses the card's keyer. Kept here, apart from the rest of the window, so they
//! can move as they are when the settings are laid out another way.

use egui::Color32;

use super::{Keyer, Output, Screen};
use crate::i18n::{t, tf};
use crate::studio::TEXT_DIM;

/// Shown when the device isn't there, or can't do what is asked: orange, as a warning.
const WARNING: Color32 = Color32::from_rgb(255, 170, 120);

fn mode_hint() -> &'static str {
    t(
        "screens.decklink.format.hint",
        "The video format the card sends. Set it to what the switcher, recorder or projector it goes to expects: they must match exactly. A new screen starts in the format set for the card in Desktop Video Setup; where none is set (or SoftPresenter can't play it), in 1080i59.94, the usual HD format where power is 60 Hz (1080i50 where it is 50 Hz). A 720p format is scaled down from SoftPresenter's 1920 × 1080 picture.",
    )
}

fn keyer_hint() -> &'static str {
    t(
        "screens.decklink.keyer.hint",
        "Off: the picture goes solid. Fill and key: the card sends the picture on this connector and its key (white where solid, black where clear) on the connector paired with it, in step, for a switcher's keyer: better than a second screen as this one's key. Over its input: the card lays the picture over the video coming into it. Make the screen see-through (under Look) so that only the words are keyed.",
    )
}

/// The DeckLink rows for the screen, under its output. True when anything was changed.
pub fn decklink_rows(ui: &mut egui::Ui, screen: &mut Screen, found: &sp_decklink::Found) -> bool {
    let solid = screen.look.see_through.is_none();
    let Output::DeckLink(out) = &mut screen.output else { return false };
    let device = found.find(out.id, &out.device);
    let mut changed = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t("screens.decklink.format", "Format")).color(TEXT_DIM))
            .on_hover_text(mode_hint());
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
    // Where the format comes from: the card's own setting, the usual one, or set here.
    if let Some(device) = device.filter(|device| device.mode(&out.mode).is_some()) {
        let words = super::decklink_format_words(device, &out.mode);
        ui.label(egui::RichText::new(format!("{words}.")).size(11.5).color(TEXT_DIM)).on_hover_text(mode_hint());
    }
    // The keyer, where the card has one (in its present profile), or one is set.
    let (external, internal) = device.map_or((false, false), |device| (device.external_keying, device.internal_keying));
    if external || internal || out.keyer != Keyer::Off {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t("screens.decklink.keyer", "Keyer")).color(TEXT_DIM))
                .on_hover_text(keyer_hint());
            ui.add_space(8.0);
            for (keyer, label, can) in [
                (Keyer::Off, t("screens.decklink.keyer.off", "Off"), true),
                (Keyer::External, t("screens.decklink.keyer.external", "Fill and key"), external),
                (Keyer::Internal, t("screens.decklink.keyer.internal", "Over its input"), internal),
            ] {
                if !can && out.keyer != keyer {
                    continue;
                }
                let button = ui.add(egui::Button::selectable(out.keyer == keyer, label)).on_hover_text(keyer_hint());
                if button.clicked() && out.keyer != keyer {
                    out.keyer = keyer;
                    changed = true;
                }
            }
        });
    }
    let warning = match device {
        None => Some(tf(
            "screens.decklink.not_connected",
            "{device} isn't connected now. The screen plays out of it as soon as it is.",
            &[("device", &out.device)],
        )),
        Some(device) if device.mode(&out.mode).is_none() => Some(tf(
            "screens.decklink.no_format",
            "{device} has no format {mode}. Choose one above.",
            &[("device", &device.name), ("mode", &out.mode)],
        )),
        Some(device) if (out.keyer == Keyer::External && !external) || (out.keyer == Keyer::Internal && !internal) => {
            Some(tf(
                "screens.decklink.cant_key_that_way",
                "{device} can't key that way in its card's present profile.",
                &[("device", &device.name)],
            ))
        }
        Some(device) if out.keyer != Keyer::Off && !device.mode(&out.mode).is_some_and(|mode| mode.keying) => Some(tf(
            "screens.decklink.cant_key_in",
            "{device} can't key in {mode}. Choose another format.",
            &[("device", &device.name), ("mode", &out.mode)],
        )),
        Some(_) if out.keyer != Keyer::Off && solid => Some(
            t(
                "screens.decklink.solid",
                "This screen is solid, so its key is all white. Make it see-through under Look.",
            )
            .to_owned(),
        ),
        Some(_) => None,
    };
    if let Some(warning) = warning {
        ui.label(egui::RichText::new(warning).size(11.5).color(WARNING));
    }
    changed
}
