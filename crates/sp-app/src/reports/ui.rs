//! What a person sees of the anonymous reports: the line of the setup's Ready step and its
//! fold, the card in Settings ›
//! About, and the one-time notice. All drawn with the window's own parts (the On/Off key, rows,
//! flat keys, the fold); every one of them says the same thing in the same words.

use winit::event::{ElementState, WindowEvent};

use super::stats::{self, Count};
use crate::home::settings::parts;
use crate::home::{HomeState, Page, open_page};
use crate::i18n::t;
use crate::quick_help::Hint;

/// What is sent and what isn't, in one plain line, as Settings says it.
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

/// The one-time notice of an install that never finished the setup on its Ready step.
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
        if parts::on_off(ui, place, ui.scope_id().with("anonymous reports"), on).hint(note).clicked() {
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
                if parts::key(ui, place, ui.scope_id().with((salt, "copy")), words, false)
                    .hint(t("quickhelp.reports.copy", "Copy the report."))
                    .clicked()
                {
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

/// The Reports line of the setup's Ready step: what reports are, by how they start where the
/// computer is (on; or, in the EU, the EEA and the UK, off until turned on). It follows the
/// region, never the switch, so switching never changes the line or its height.
pub fn setup_line(starts_on: bool) -> &'static str {
    if starts_on {
        t(
            "reports.setup.line",
            "Anonymous: speeds and crashes. Never your songs, Bible text, file names or anything about you.",
        )
    } else {
        t(
            "reports.setup.line_off",
            "Off until turned on; they help us fix problems. Never your songs, Bible text, file names or anything about you.",
        )
    }
}

/// What the Ready step's switch is called for a screen reader and the tests.
pub fn setup_switch_id() -> egui::Id {
    egui::Id::unique("setup anonymous reports switch")
}

/// The Ready step's switch was pressed: reports go on or off at once, as Settings' switch does.
pub fn setup_switched(ctx: &egui::Context, on: bool) {
    super::set_enabled(on);
    preview_stale(ctx);
}

/// The title of the Ready step's fold that shows what is sent.
pub fn setup_fold_title() -> &'static str {
    t("reports.setup.fold", "What the reports send")
}

/// What the Ready step's fold says first.
pub fn setup_fold_words() -> &'static str {
    t(
        "reports.setup.fold.about",
        "Anonymous reports: the kind of computer, speeds and crashes. The next report, as it stands now: this is all of it, nothing else is sent with it. Change it later in Settings \u{203a} About.",
    )
}

/// The Ready step's fold, opened: what the reports are, the next report whole in a box that
/// scrolls with Copy beside it, and the way to the privacy page. A test copy says here, not on
/// the row, that it never sends.
pub fn setup_fold(ui: &mut egui::Ui, state: &mut HomeState, may_send: bool) {
    parts::note(ui, setup_fold_words());
    let asked = ui.data(|data| data.get_temp::<bool>(asked_id()).unwrap_or(false));
    if !asked {
        super::ask_preview();
        ui.data_mut(|data| data.insert_temp(asked_id(), true));
    }
    match super::preview() {
        Some(report) => {
            let words = t("reports.copy", "Copy");
            let key = parts::key_size(ui, words);
            ui.horizontal_top(|ui| {
                let width = ui.available_width() - key.x - 10.0;
                ui.allocate_ui(egui::vec2(width, 150.0), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("setup anonymous report")
                        .max_height(150.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| parts::code(ui, &report));
                });
                let (place, _) = ui.allocate_exact_size(key, egui::Sense::hover());
                if parts::key(ui, place, ui.scope_id().with("setup report copy"), words, false).clicked() {
                    ui.ctx().copy_text(report);
                }
            });
        }
        None => {
            parts::note(ui, t("reports.see.making", "Making it\u{2026}"));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    ui.add_space(8.0);
    let hint = t("quickhelp.reports.privacy", "Open the privacy page in your browser.");
    if crate::setup::link(ui, t("reports.privacy.read", "Read the privacy page"), hint).clicked() {
        open_page(state, Page::Privacy);
    }
    if !may_send {
        ui.add_space(6.0);
        parts::note(ui, test_copy_words());
    }
}

/// The setup was finished on its Ready step, where the Reports line stands: the person has seen
/// how reports stand, and left them so.
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
                    let hint = t(
                        "quickhelp.reports.reset",
                        "Start a new anonymous id, so later reports can\u{2019}t be matched with earlier ones.",
                    );
                    if parts::key(ui, place, ui.scope_id().with("reset anonymous id"), reset, false)
                        .hint(hint)
                        .clicked()
                    {
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
                    let hint = t("quickhelp.reports.privacy", "Open the privacy page in your browser.");
                    if parts::key(ui, place, ui.scope_id().with("reports privacy"), read, false).hint(hint).clicked() {
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
