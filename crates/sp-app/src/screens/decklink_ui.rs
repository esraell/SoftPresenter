//! The Screens window's rows for a screen played out of a DeckLink card: how it uses the card's
//! keyer (one choice, which sets its background too), and the format it plays in. Kept here,
//! apart from the rest of the window, so they can move as they are when the settings are laid
//! out another way.

use egui::Color32;

use super::{Alpha, DeckLinkOut, Keyer, Kind, Output, Screen};
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

/// What each way of using the keyer does, in plain words: shown under the choice, and over
/// each of its buttons.
fn keyer_about(keyer: Keyer) -> &'static str {
    match keyer {
        Keyer::Off => {
            t("screens.decklink.keyer.off.about", "No keying: the picture goes out whole, as a display shows it.")
        }
        Keyer::External => t(
            "screens.decklink.keyer.external.about",
            "Two cables to a switcher's keyer: the picture on this connector, its key on the one paired with it. Only the words are keyed: the media and the themes' backgrounds are left out.",
        ),
        Keyer::Internal => t(
            "screens.decklink.keyer.internal.about",
            "The card lays the picture over the video coming into it. Only the words are keyed: the media and the themes' backgrounds are left out.",
        ),
    }
}

fn edges_hint() -> &'static str {
    t(
        "screens.decklink.edges.hint",
        "How the card sends a pixel that is only partly solid, such as the soft edge of a letter. Premultiplied, the usual for a switcher's keyer (with its premultiplied key setting on): the colour already darkened by how see-through it is. Straight: the colour as it is. If the edges of the words look dark or have a fringe, try the other.",
    )
}

/// The DeckLink rows for the screen, under its output. True when anything was changed.
pub fn decklink_rows(ui: &mut egui::Ui, screen: &mut Screen, found: &sp_decklink::Found) -> bool {
    let Output::DeckLink(out) = &screen.output else { return false };
    let out = out.clone();
    let audience = screen.kind == Kind::Audience;
    let device = found.find(out.id, &out.device);
    let mut changed = false;
    // The keyer first, where the card has one (in its present profile) for an audience screen,
    // or one is set: one choice, which makes the screen see-through or solid with it.
    let (external, internal) = device.map_or((false, false), |device| (device.external_keying, device.internal_keying));
    if (audience && (external || internal)) || out.keyer != Keyer::Off {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(t("screens.decklink.keyer", "Keyer")).color(TEXT_DIM));
            ui.add_space(8.0);
            for (keyer, label, can) in [
                (Keyer::Off, t("screens.decklink.keyer.off", "Off"), true),
                (Keyer::External, t("screens.decklink.keyer.external", "Fill and key"), audience && external),
                (Keyer::Internal, t("screens.decklink.keyer.internal", "Over the card's input"), audience && internal),
            ] {
                if !can && out.keyer != keyer {
                    continue;
                }
                let button =
                    ui.add(egui::Button::selectable(out.keyer == keyer, label)).on_hover_text(keyer_about(keyer));
                if button.clicked() && out.keyer != keyer {
                    screen.set_keyer(keyer, None);
                    changed = true;
                }
            }
        });
        ui.label(egui::RichText::new(keyer_about(out.keyer)).size(11.5).color(TEXT_DIM));
        // How the soft edges go: premultiplied as usual, straight for the rare switcher that
        // wants it.
        if let Some(alpha) = screen.look.see_through.filter(|_| screen.keyed()) {
            ui.horizontal_wrapped(|ui| {
                let small = |words: &str| egui::RichText::new(words).size(11.5);
                ui.label(small(t("screens.background.edges", "Soft edges")).color(TEXT_DIM))
                    .on_hover_text(edges_hint());
                for (kind, label) in [
                    (Alpha::Premultiplied, t("screens.decklink.edges.premultiplied", "Premultiplied (usual)")),
                    (Alpha::Straight, t("screens.background.straight", "Straight")),
                ] {
                    let button = egui::Button::selectable(alpha == kind, small(label)).small();
                    if ui.add(button).on_hover_text(edges_hint()).clicked() && alpha != kind {
                        screen.look.see_through = Some(kind);
                        changed = true;
                    }
                }
            });
        }
    }
    ui.add_space(4.0);
    let mut mode = out.mode.clone();
    // Desktop Video Setup's format, where it is another: offered beside the format, and said
    // plainly under it.
    let card = out.card_format_to_offer(found);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(t("screens.decklink.format", "Format")).color(TEXT_DIM))
            .on_hover_text(mode_hint());
        ui.add_space(8.0);
        egui::ComboBox::from_id_salt(("decklink mode", screen.id)).selected_text(mode.as_str()).width(150.0).show_ui(
            ui,
            |ui| {
                for offered in device.into_iter().flat_map(|device| device.offered()) {
                    if ui.selectable_label(mode == offered.name, offered.name.as_str()).clicked() {
                        mode.clone_from(&offered.name);
                    }
                }
            },
        );
        if let Some(card) = &card
            && ui.button(tf("screens.decklink.format.use", "Use {card}", &[("card", card)])).clicked()
        {
            mode.clone_from(card);
        }
    });
    // Else where the format comes from: the card's own setting, the usual one, or set here.
    if let Some(card) = &card {
        let words = tf(
            "screens.decklink.format.differs",
            "Desktop Video Setup has {card}. A switcher usually needs every input in its own format.",
            &[("card", card)],
        );
        ui.label(egui::RichText::new(words).size(11.5).color(WARNING)).on_hover_text(mode_hint());
    } else if let Some(device) = device.filter(|device| device.mode(&out.mode).is_some()) {
        let words = super::decklink_format_words(device, &out.mode);
        ui.label(egui::RichText::new(format!("{words}.")).size(11.5).color(TEXT_DIM)).on_hover_text(mode_hint());
    }
    if mode != out.mode
        && let Output::DeckLink(out) = &mut screen.output
    {
        out.mode = mode;
        changed = true;
    }
    if let Some(trouble) = trouble(&out, found) {
        ui.label(egui::RichText::new(trouble).size(11.5).color(WARNING));
    }
    changed
}

/// What keeps a DeckLink screen from playing as it is set, in words: its device not connected,
/// a format the device hasn't, or keying the card can't do (that way, or in that format).
/// `None` when all is well. A keyed screen is see-through by its keyer, so its key is never all
/// white for being solid.
fn trouble(out: &DeckLinkOut, found: &sp_decklink::Found) -> Option<String> {
    let Some(device) = found.find(out.id, &out.device) else {
        return Some(tf(
            "screens.decklink.not_connected",
            "{device} isn't connected now. The screen plays out of it as soon as it is.",
            &[("device", &out.device)],
        ));
    };
    let mode = device.mode(&out.mode);
    if mode.is_none() {
        return Some(tf(
            "screens.decklink.no_format",
            "{device} has no format {mode}. Choose one above.",
            &[("device", &device.name), ("mode", &out.mode)],
        ));
    }
    let can = match out.keyer {
        Keyer::Off => true,
        Keyer::External => device.external_keying,
        Keyer::Internal => device.internal_keying,
    };
    if !can {
        return Some(tf(
            "screens.decklink.cant_key_that_way",
            "{device} can't key that way in its card's present profile.",
            &[("device", &device.name)],
        ));
    }
    if out.keyer != Keyer::Off && !mode.is_some_and(|mode| mode.keying) {
        return Some(tf(
            "screens.decklink.cant_key_in",
            "{device} can't key in {mode}. Choose another format.",
            &[("device", &device.name), ("mode", &out.mode)],
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card as the driver might list it: 1080i59.94, 1080p29.97 and 1080p30, keying in every
    /// one but 1080i59.94, set in Desktop Video Setup to `card` (four letters).
    fn found(card: Option<&[u8; 4]>, keying: bool) -> sp_decklink::Found {
        let mode = |code: &[u8; 4], dominance: &[u8; 4], rate: (i64, i64), keys: bool| {
            sp_decklink::Mode::new(
                u32::from_be_bytes(*code),
                String::new(),
                (1920, 1080),
                rate,
                u32::from_be_bytes(*dominance),
                (true, true, keys),
            )
        };
        let device = sp_decklink::Device {
            name: "DeckLink Duo (1)".to_owned(),
            model: "DeckLink Duo 2".to_owned(),
            id: Some(1),
            sub_device: Some(0),
            sub_devices: Some(2),
            profile: u32::from_be_bytes(*b"2dfd"),
            duplex: 0,
            connections: 1,
            can_play: true,
            internal_keying: false,
            external_keying: keying,
            modes: vec![
                mode(b"Hi59", b"uppr", (1001, 30000), false),
                mode(b"Hp29", b"prog", (1001, 30000), true),
                mode(b"Hp30", b"prog", (1000, 30000), true),
            ],
            card_mode: card.map(|code| i64::from(u32::from_be_bytes(*code))),
            card_mode_3d: false,
        };
        sp_decklink::Found { driver: sp_decklink::Driver::Ready("16.3".to_owned()), devices: vec![device] }
    }

    fn out(mode: &str, keyer: Keyer) -> DeckLinkOut {
        DeckLinkOut { device: "DeckLink Duo (1)".to_owned(), id: Some(1), mode: mode.to_owned(), keyer }
    }

    #[test]
    fn a_format_other_than_desktop_video_setups_is_offered_its_own() {
        // The church's screen: 1080p30 where Desktop Video Setup (and the switcher) has
        // 1080p29.97.
        let church = found(Some(b"Hp29"), true);
        assert_eq!(out("1080p30", Keyer::External).card_format_to_offer(&church).as_deref(), Some("1080p29.97"));
        assert_eq!(out("1080p30", Keyer::Off).card_format_to_offer(&church).as_deref(), Some("1080p29.97"));
        // The same: nothing to say.
        assert_eq!(out("1080p29.97", Keyer::External).card_format_to_offer(&church), None);
        // Desktop Video Setup sets none, or one this output can't key in while it keys.
        assert_eq!(out("1080p30", Keyer::Off).card_format_to_offer(&found(None, true)), None);
        let interlaced = found(Some(b"Hi59"), true);
        assert_eq!(out("1080p30", Keyer::External).card_format_to_offer(&interlaced), None, "it can't key in that");
        assert_eq!(out("1080p30", Keyer::Off).card_format_to_offer(&interlaced).as_deref(), Some("1080i59.94"));
        // The card away: nothing.
        let away = sp_decklink::Found { driver: sp_decklink::Driver::Ready("16.3".to_owned()), devices: Vec::new() };
        assert_eq!(out("1080p30", Keyer::Off).card_format_to_offer(&away), None);
    }

    #[test]
    fn a_keyed_screen_is_never_warned_that_it_is_solid() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let card = found(Some(b"Hp29"), true);
            // Keyed and playing as set: nothing is wrong, whatever its background says.
            assert_eq!(trouble(&out("1080p29.97", Keyer::External), &card), None);
            assert_eq!(trouble(&out("1080p29.97", Keyer::Off), &card), None);
            // What can still be wrong is said.
            let cant = trouble(&out("1080p29.97", Keyer::Internal), &card).unwrap();
            assert!(cant.contains("can't key that way"), "{cant}");
            let not_in = trouble(&out("1080i59.94", Keyer::External), &card).unwrap();
            assert!(not_in.contains("can't key in 1080i59.94"), "{not_in}");
            let no_format = trouble(&out("720p50", Keyer::Off), &card).unwrap();
            assert!(no_format.contains("has no format 720p50"), "{no_format}");
            let away = sp_decklink::Found { driver: sp_decklink::Driver::Ready("16.3".to_owned()), devices: vec![] };
            assert!(trouble(&out("1080p30", Keyer::Off), &away).unwrap().contains("isn't connected"));
        });
    }

    #[test]
    fn the_screens_window_draws_a_keyed_decklink_screen_and_changes_nothing_by_itself() {
        let ctx = egui::Context::default();
        let card = found(Some(b"Hp29"), true);
        let mut screen = Screen::new("Lyrics".to_owned(), Output::DeckLink(out("1080p30", Keyer::Off)));
        screen.set_keyer(Keyer::External, None);
        let before = screen.clone();
        for _ in 0..2 {
            let mut drawn = ctx.run_ui(egui::RawInput::default(), |ui| {
                assert!(!decklink_rows(ui, &mut screen, &card), "nothing was clicked");
            });
            drawn.textures_delta.clear();
        }
        assert_eq!(screen, before);
    }
}
