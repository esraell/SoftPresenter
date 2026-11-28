//! The setup's Screens step: which display the congregation sees, a stage display for the band
//! if there is another, Identify Screens, and NDI beside it (the key that opens the Install NDI
//! window, or that NDI is here). Each choice is the Screens window's own, and goes there at once.

use egui::{Align2, RichText, Sense, vec2};

use crate::displays::{DisplayInfo, kept_for_operator};
use crate::home::HomeState;
use crate::i18n::{t, tf};
use crate::screens::{Kind, Output, Screen};
use crate::studio::{CARD, HOVER, READY, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// Words under the title, wrapped.
fn say(ui: &mut egui::Ui, text: &str) {
    super::words(ui, text, egui::FontId::proportional(13.0), TEXT_DIM, 18.0, 1);
}

/// A choice in a list, lit when chosen.
fn choice(ui: &mut egui::Ui, chosen: bool, label: &str, detail: &str) -> egui::Response {
    let width = ui.available_width();
    let (place, response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let fill = if chosen {
        SELECTED
    } else if response.hovered() {
        HOVER
    } else {
        CARD
    };
    ui.painter().rect_filled(place, 9.0, fill);
    let mark = if chosen { "\u{25c9}" } else { "\u{25cb}" };
    let ink = if chosen { READY } else { TEXT_FAINT };
    ui.painter().text(
        place.left_center() + vec2(16.0, 0.0),
        Align2::LEFT_CENTER,
        mark,
        egui::FontId::proportional(14.0),
        ink,
    );
    ui.painter().text(place.left_center() + vec2(40.0, -7.0), Align2::LEFT_CENTER, label, semibold(13.0), TEXT);
    ui.painter().text(
        place.left_center() + vec2(40.0, 9.0),
        Align2::LEFT_CENTER,
        detail,
        egui::FontId::proportional(11.0),
        TEXT_DIM,
    );
    response
}

/// The displays the audience may be given, with their sizes: each one the operator doesn't work
/// on (`displays::kept_for_operator`). None with one display.
pub(super) fn audience_displays(displays: &[DisplayInfo], operator: &[usize]) -> Vec<(String, String)> {
    displays
        .iter()
        .filter(|display| kept_for_operator(displays, operator, display.index).is_none())
        .map(|display| (display.name.clone(), format!("{} × {}", display.w, display.h)))
        .collect()
}

/// What the Screens step says when only one display is connected (`name`, when it is known):
/// that is where the operator works, and the congregation sees SoftPresenter on a TV or
/// projector, or over NDI (`ndi`, when it is installed). What happens until one is plugged in
/// depends on the choice, so each choice says it.
pub(super) fn one_display_words(name: Option<&str>, ndi: bool) -> String {
    let mut words = match name {
        Some(name) => tf(
            "setup.screens.one.named",
            "Only one display is connected ({name}). This is where you work, so the audience screen never covers it: the congregation sees it on a TV or projector.",
            &[("name", name)],
        ),
        None => t(
            "setup.screens.one",
            "Only one display is connected. This is where you work, so the audience screen never covers it: the congregation sees it on a TV or projector.",
        )
        .to_owned(),
    };
    if ndi {
        words.push(' ');
        words.push_str(t("setup.screens.one.ndi", "It can also be sent over NDI, to a stream or a video switcher."));
    }
    words
}

/// The Screens step's choices with one display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OneDisplayChoice {
    /// In a window for now, and on a TV or projector as soon as one is plugged in (Automatic).
    Automatic,
    /// Only in a window, even with a TV or projector plugged in: shown only while it is the
    /// screen's output (an earlier setup gave it, and the Screens window can).
    Window,
    Ndi,
    /// Shown nowhere for now: off, or anything the Screens window gave that the step doesn't offer.
    NotNow,
}

/// Which one-display choice is lit for the audience's `output`: exactly one, whatever it is.
/// NDI counts only while it is installed (`ndi`): without it the screen shows nowhere.
pub(super) fn one_display_choice(output: &Output, ndi: bool) -> OneDisplayChoice {
    match output {
        Output::Automatic => OneDisplayChoice::Automatic,
        Output::Window => OneDisplayChoice::Window,
        Output::Ndi(_) if ndi => OneDisplayChoice::Ndi,
        _ => OneDisplayChoice::NotNow,
    }
}

/// What the Screens step says with several displays: which one is kept for SoftPresenter's own
/// window. Where the window system doesn't say which display is the main one (Wayland), it is
/// the one the window is on, and those displays aren't offered.
pub(super) fn several_displays_words(main: Option<&str>) -> String {
    match main {
        Some(name) => tf(
            "setup.screens.several.main",
            "The main display ({name}) is kept for SoftPresenter's own window. Choose the projector or TV; the screen goes there straight away so you can check.",
            &[("name", name)],
        ),
        None => t(
            "setup.screens.several.window",
            "The display SoftPresenter's own window is on is kept for it. Choose the projector or TV; the screen goes there straight away so you can check.",
        )
        .to_owned(),
    }
}

/// Which display the congregation sees, and a stage display for the band if there is another.
/// Choosing one puts the screen there at once, so the person can look at it. With one display
/// the audience screen is left to choose for itself (Automatic) unless told otherwise: in a
/// window until a TV or projector is plugged in, which it then fills at once (`screens::settle`).
pub fn step(ui: &mut egui::Ui, state: &mut HomeState) {
    super::head(ui, t("setup.screens.title", "Which screen does the congregation see?"), "");
    ui.spacing_mut().item_spacing.y = 0.0;
    let others = audience_displays(&state.displays, &state.operator_displays);
    let audience = match state.screens.iter().position(|screen| screen.kind == Kind::Audience) {
        Some(at) => at,
        None => {
            state.screens.insert(0, Screen::new("Screen 1".to_owned(), Output::Off));
            0
        }
    };
    if others.is_empty() {
        // The audience on the one display (given by a version that offered it, as on Wayland,
        // where no display is the main one) shows nowhere: it goes back to choosing for itself,
        // which shows it in a window and covers nothing.
        if let Output::Display(name) = &state.screens[audience].output
            && let Some(display) = state.displays.iter().find(|d| d.is_named(name))
            && kept_for_operator(&state.displays, &state.operator_displays, display.index).is_some()
        {
            state.screens[audience].output = Output::Automatic;
            state.screens_changed = true;
        }
        say(ui, &one_display_words(state.displays.first().map(|d| d.name.as_str()), state.ndi_available));
        ui.add_space(10.0);
        let lit = one_display_choice(&state.screens[audience].output, state.ndi_available);
        let set = |output: Output, state: &mut HomeState| {
            state.screens[audience].go_to(output);
            state.screens_changed = true;
        };
        if choice(
            ui,
            lit == OneDisplayChoice::Automatic,
            t("setup.screens.automatic", "In a window for now"),
            t("setup.screens.automatic.detail", "On a TV or projector as soon as one is plugged in"),
        )
        .clicked()
        {
            set(Output::Automatic, state);
        }
        ui.add_space(6.0);
        if lit == OneDisplayChoice::Window {
            choice(
                ui,
                true,
                t("setup.screens.window", "Only in a window"),
                t("setup.screens.window.detail", "Stays in a window, even with a TV or projector plugged in"),
            );
            ui.add_space(6.0);
        }
        if state.ndi_available
            && choice(
                ui,
                lit == OneDisplayChoice::Ndi,
                t("setup.screens.ndi", "Send over NDI"),
                t("setup.screens.ndi.detail", "For a stream or a video switcher"),
            )
            .clicked()
        {
            set(Output::Ndi("SoftPresenter Audience".to_owned()), state);
        }
        if state.ndi_available {
            ui.add_space(6.0);
        }
        if choice(
            ui,
            lit == OneDisplayChoice::NotNow,
            t("setup.screens.not_now", "Not now"),
            t("setup.screens.not_now.detail", "Choose in the Screens window later"),
        )
        .clicked()
        {
            set(Output::Off, state);
        }
        return;
    }
    say(ui, &several_displays_words(state.displays.iter().find(|d| d.main).map(|d| d.name.as_str())));
    ui.add_space(10.0);
    // The display the audience has, among those offered: on one kept for the operator it shows
    // nowhere, so "Not now" is lit, and it is left for the person to choose (nothing moves to
    // another display without a click).
    let current = match &state.screens[audience].output {
        Output::Display(name) => state
            .displays
            .iter()
            .find(|d| {
                d.is_named(name) && kept_for_operator(&state.displays, &state.operator_displays, d.index).is_none()
            })
            .map(|d| d.name.clone()),
        _ => None,
    };
    for (name, size) in &others {
        if choice(ui, current.as_deref() == Some(name), name, size).clicked() {
            state.screens[audience].go_to(Output::Display(name.clone()));
            state.screens_changed = true;
        }
        ui.add_space(6.0);
    }
    if choice(
        ui,
        current.is_none(),
        t("setup.screens.not_now", "Not now"),
        t("setup.screens.not_now.display", "Choose a display later in the Screens window"),
    )
    .clicked()
    {
        state.screens[audience].output = Output::Off;
        state.screens_changed = true;
    }
    // A stage display for the band, on a display the audience isn't using.
    let free: Vec<&(String, String)> = others.iter().filter(|(name, _)| current.as_deref() != Some(name)).collect();
    if !free.is_empty() {
        ui.add_space(14.0);
        ui.label(
            RichText::new(t("setup.screens.stage", "A screen for the band (stage display)?"))
                .font(semibold(13.5))
                .color(TEXT),
        );
        ui.add_space(6.0);
        let stage = state.screens.iter().position(|screen| screen.kind == Kind::Stage);
        let stage_on = stage.and_then(|at| match &state.screens[at].output {
            Output::Display(name) => Some(name.clone()),
            _ => None,
        });
        for (name, size) in free {
            if choice(ui, stage_on.as_deref() == Some(name), name, size).clicked() {
                match stage {
                    Some(at) => state.screens[at].go_to(Output::Display(name.clone())),
                    None => state.screens.push(Screen::stage(
                        t("setup.screens.stage.name", "Stage").to_owned(),
                        Output::Display(name.clone()),
                    )),
                }
                state.screens_changed = true;
            }
            ui.add_space(6.0);
        }
        if choice(ui, stage_on.is_none(), t("setup.screens.stage.none", "No stage display"), "").clicked()
            && let Some(at) = stage
        {
            state.screens[at].output = Output::Off;
            state.screens_changed = true;
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let identify = if state.identify {
            t("setup.screens.identify.stop", "Stop Identifying")
        } else {
            t("setup.screens.identify", "Identify Screens")
        };
        if super::key(ui, "identify", identify, 90.0, true).clicked() {
            state.identify = !state.identify;
        }
        // NDI beside it: the key that opens the Install NDI window (the setup is back at this
        // step when it is left), or, once NDI is here, that it is, with a green light.
        if state.ndi_available {
            let wake = {
                let ctx = ui.ctx().clone();
                move || ctx.request_repaint()
            };
            state.ndi_download.ask_version(true, wake);
            let words = ndi_installed_words(state.ndi_download.version().as_deref());
            let (light, _) = ui.allocate_exact_size(vec2(13.0, 30.0), Sense::hover());
            crate::studio::paint_light(ui.painter(), light.center(), Some(READY));
            ui.label(RichText::new(words).size(12.5).color(TEXT_DIM));
        } else if super::key(ui, "install ndi", crate::ndi_download::install_key(), 90.0, true)
            .on_hover_text(t(
                "setup.screens.ndi.install.hover",
                "NDI sends the slides over your network, to a live stream or another computer. It\u{2019}s free.",
            ))
            .clicked()
        {
            state.ndi_download.open(crate::ndi_download::Place::Home);
        }
    });
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(RichText::new(identify_and_ndi_words(state.ndi_available)).size(11.5).color(TEXT_FAINT))
            .wrap(),
    );
}

/// "NDI 6.1.1 is installed", in the place of the Install NDI… key once NDI is here.
fn ndi_installed_words(version: Option<&str>) -> String {
    match version {
        Some(version) => tf("setup.screens.ndi.installed", "NDI {version} is installed", &[("version", version)]),
        None => t("setup.screens.ndi.installed.unversioned", "NDI is installed").to_owned(),
    }
}

/// The one line under Identify Screens and NDI: what each is for.
fn identify_and_ndi_words(ndi: bool) -> &'static str {
    if ndi {
        t(
            "setup.screens.identify.with_ndi",
            "Identify shows each screen\u{2019}s name on it. A screen can be sent over your network: choose NDI for it in Settings \u{203a} Screens.",
        )
    } else {
        t(
            "setup.screens.identify.without_ndi",
            "Identify shows each screen\u{2019}s name on it. NDI sends the slides over your network, for a live stream; it can be added later too.",
        )
    }
}
