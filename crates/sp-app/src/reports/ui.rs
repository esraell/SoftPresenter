//! What a person sees of the anonymous reports: the setup's step, the card in Settings ›
//! About, and the one-time notice. All drawn with the window's own parts (the On/Off key, rows,
//! flat keys, the fold); every one of them says the same thing in the same words.

use winit::event::{ElementState, WindowEvent};

use super::stats::{self, Count};
use crate::home::settings::parts;
use crate::home::{HomeState, Page, open_page};
use crate::i18n::t;

/// What is sent and what isn't, in one plain line: the setup's step and Settings say it alike.
/// While reports are off it says what they would be.
pub fn about_words(on: bool) -> &'static str {
    if on {
        t(
            "reports.about",
            "SoftPresenter sends anonymous reports of how it runs and what goes wrong: the kind of computer, speeds, crashes. Never your songs, Bible text, file names or anything about you.",
        )
    } else {
        t(
            "reports.about.off",
            "SoftPresenter can send anonymous reports of how it runs and what goes wrong: the kind of computer, speeds, crashes. Never your songs, Bible text, file names or anything about you.",
        )
    }
}

/// The one-time notice of an install that never passed the setup's step.
pub fn notice_words() -> &'static str {
    t(
        "reports.notice",
        "Anonymous reports are on. SoftPresenter sends how it runs and what goes wrong, so problems get fixed; never your songs, Bible text, file names or anything about you. You can see what's sent, and turn it off, in Settings \u{203a} About.",
    )
}

/// What a test copy says where the switch is: it never sends, whatever the switch says.
fn test_copy_words() -> &'static str {
    t("reports.test_copy", "This is a test copy: it never sends reports, whatever is chosen here.")
}

/// Counts what the person does: keys pressed and clicks made, as numbers only. Called with
/// every event of a window; never waits.
pub fn window_event(event: &WindowEvent) {
    if !stats::is_on() {
        return;
    }
    match event {
        WindowEvent::KeyboardInput { event, is_synthetic: false, .. }
            if event.state == ElementState::Pressed && !event.repeat =>
        {
            stats::bump(Count::Keys);
        }
        WindowEvent::MouseInput { state: ElementState::Pressed, .. } => stats::bump(Count::Clicks),
        _ => {}
    }
}

/// The row with the On/Off key. Returns what was chosen, if the key was pressed.
fn switch_row(ui: &mut egui::Ui, on: bool, note: &str) -> Option<bool> {
    let mut chosen = None;
    let size = parts::on_off_size(ui);
    parts::row(ui, t("reports.switch", "Send anonymous reports"), note, size, |ui, place| {
        if parts::on_off(ui, place, ui.scope_id().with("anonymous reports"), on).clicked() {
            chosen = Some(!on);
        }
    });
    chosen
}

fn asked_id() -> egui::Id {
    egui::Id::unique("anonymous reports preview asked")
}

/// See what's sent: the next report as it stands now, exactly as it would go, with Copy. The
/// reporter's thread makes it when the fold is opened (and again after a change).
fn see_whats_sent(ui: &mut egui::Ui, salt: &str, open: bool) {
    parts::fold(ui, salt, t("reports.see", "See what\u{2019}s sent"), open, |ui| {
        let asked = ui.data(|data| data.get_temp::<bool>(asked_id()).unwrap_or(false));
        if !asked {
            super::ask_preview();
            ui.data_mut(|data| data.insert_temp(asked_id(), true));
        }
        match super::preview() {
            Some(report) => {
                parts::note(
                    ui,
                    t(
                        "reports.see.about",
                        "The next report, as it stands now. This is all of it: nothing else is sent with it.",
                    ),
                );
                egui::ScrollArea::vertical().id_salt((salt, "report")).max_height(220.0).show(ui, |ui| {
                    parts::code(ui, &report);
                });
                ui.add_space(8.0);
                let words = t("reports.copy", "Copy");
                let size = parts::key_size(ui, words);
                let (place, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                if parts::key(ui, place, ui.scope_id().with((salt, "copy")), words, false).clicked() {
                    ui.ctx().copy_text(report);
                }
            }
            None => {
                parts::note(ui, t("reports.see.making", "Making it\u{2026}"));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    });
}

/// The report shown is out of date (reports were switched, or the id was reset): made again.
fn preview_stale(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(asked_id(), false));
}

/// The setup's step: reports shown on, with the On/Off key, the one line on what is and isn't
/// sent, See what's sent, and the way to the privacy page. Passing the step (Continue) is what
/// counts as having been told; the setup does that (`passed_setup_step`).
pub fn setup_step(ui: &mut egui::Ui, state: &mut HomeState) {
    crate::setup::title(ui, t("reports.setup.title", "Anonymous reports"));
    let standing = super::standing().unwrap_or_default();
    crate::setup::say(ui, about_words(standing.enabled));
    ui.add_space(12.0);
    // Where reports start off (the EU, the EEA and the UK), the line invites turning them on.
    let note = if standing.starts_on {
        t("reports.setup.note", "On unless you turn it off. You can change it later in Settings \u{203a} About.")
    } else {
        t(
            "reports.setup.note_off",
            "Off unless you turn it on. Turning it on helps us find and fix problems. You can change it later in Settings \u{203a} About.",
        )
    };
    if let Some(on) = switch_row(ui, standing.enabled, note) {
        super::set_enabled(on);
        preview_stale(ui.ctx());
    }
    ui.add_space(12.0);
    // Testing aid: SOFTPRESENTER_SETUP=reports-open shows the step with See what's sent opened.
    let open = std::env::var("SOFTPRESENTER_SETUP").is_ok_and(|step| step == "reports-open");
    see_whats_sent(ui, "setup anonymous reports", open);
    if crate::setup::link(ui, t("reports.privacy.read", "Read the privacy page")).clicked() {
        open_page(state, Page::Privacy);
    }
    if !standing.may_send {
        ui.add_space(8.0);
        crate::setup::say(ui, test_copy_words());
    }
}

/// The setup's step was passed with Continue: the person has seen how reports stand, and left
/// them so.
pub fn passed_setup_step() {
    super::confirm_shown();
}

/// The card in Settings › About: the On/Off key, See what's sent, Reset my anonymous id and the
/// privacy page.
pub fn settings_card(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(standing) = super::standing() else { return };
    let mut privacy = false;
    parts::card(
        ui,
        t("reports.card", "Anonymous reports"),
        t("reports.card.about", "How the app runs, never what you show"),
        None,
        |ui| {
            if let Some(on) = switch_row(ui, standing.enabled, about_words(standing.enabled)) {
                super::set_enabled(on);
                preview_stale(ui.ctx());
            }
            let reset = t("reports.reset", "Reset my anonymous id");
            let size = parts::key_size(ui, reset);
            parts::row(
                ui,
                t("reports.id", "Anonymous id"),
                t(
                    "reports.id.about",
                    "A random number that says which reports came from the same install, never whose it is",
                ),
                size,
                |ui, place| {
                    if parts::key(ui, place, ui.scope_id().with("reset anonymous id"), reset, false).clicked() {
                        super::new_install_id();
                        preview_stale(ui.ctx());
                    }
                },
            );
            let read = t("reports.privacy", "Privacy\u{2026}");
            let size = parts::key_size(ui, read);
            parts::row(
                ui,
                t("reports.privacy.label", "Privacy"),
                t("reports.privacy.about", "What is sent, what never is, where it goes and how long it is kept"),
                size,
                |ui, place| {
                    if parts::key(ui, place, ui.scope_id().with("reports privacy"), read, false).clicked() {
                        privacy = true;
                    }
                },
            );
            if !standing.may_send {
                parts::note(ui, test_copy_words());
            }
            // Testing aid: SOFTPRESENTER_CAPTURE_PREFERENCES=reports-sent shows the card with See
            // what's sent opened.
            let open = std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES").is_ok_and(|page| page == "reports-sent");
            see_whats_sent(ui, "settings anonymous reports", open);
        },
    );
    if privacy {
        open_page(state, Page::Privacy);
    }
}
