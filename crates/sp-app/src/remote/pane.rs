//! Settings › Remote Control: switching it on and off, the port, the password, the address to
//! open and the certificate's fingerprint, the pages connected and the devices remembered.

use egui::{Align, Layout, RichText};

use super::{Request, View};
use crate::i18n::{t, tf, tn};
use crate::studio::{TEXT, TEXT_DIM, TEXT_FAINT};

/// How long ago a time in seconds since 1970 was, in words.
fn ago(seconds: u64) -> String {
    let now = super::auth::now_seconds();
    let gone = now.saturating_sub(seconds);
    match gone {
        0..=59 => t("remote.ago.now", "just now").to_owned(),
        60..=3599 => tf("remote.ago.minutes", "{n} min ago", &[("n", &crate::i18n::count(gone / 60))]),
        3600..=86_399 => tf("remote.ago.hours", "{n} h ago", &[("n", &crate::i18n::count(gone / 3600))]),
        _ => tn("remote.ago.days", gone / 86_400, "{n} days ago", "{n} days ago", &[]),
    }
}

/// The date of a time in seconds since 1970, in the computer's own time zone, in the language's
/// way of writing it (a format from its table).
fn day(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(seconds as i64, 0)
        .map(|at| written(at.with_timezone(&chrono::Local).format(t("remote.day.format", "%-d %b %Y"))))
        .unwrap_or_default()
}

/// A time as a table's format writes it; nothing, rather than a stop, if chrono can't read the
/// format.
fn written(time: impl std::fmt::Display) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = write!(out, "{time}");
    out
}

pub fn pane(ui: &mut egui::Ui, view: &mut View) {
    let width = ui.available_width().min(600.0);
    ui.set_max_width(width);
    ui.label(
        RichText::new(
            t("remote.about", "Run the show from another computer, a tablet or a phone on the church's network: open the address below in its browser and sign in with the password. Each device is remembered until you forget it here."),
        )
        .color(TEXT_DIM),
    );
    ui.add_space(12.0);

    crate::studio::caption(ui, t("remote.caption", "REMOTE CONTROL"));
    let mut on = view.on;
    let can_switch = view.has_password || view.on;
    ui.add_enabled_ui(can_switch, |ui| {
        if ui
            .checkbox(&mut on, t("remote.turn_on", "Turn on Remote Control"))
            .on_disabled_hover_text(t("remote.on.disabled", "Set a password first"))
            .changed()
        {
            view.request = Some(Request::Switch(on));
        }
    });
    ui.horizontal(|ui| {
        ui.label(t("remote.port", "Port"));
        let field = ui.add(egui::TextEdit::singleline(&mut view.port).desired_width(70.0));
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
            || ui.button(t("remote.port.use", "Use")).clicked()
        {
            match view.port.trim().parse::<u16>() {
                Ok(port) if port >= 1024 => view.request = Some(Request::Port(port)),
                _ => {
                    view.said =
                        Some((t("remote.port.range", "A port is a number from 1024 to 65535.").to_owned(), true))
                }
            }
        }
    });
    if let Some(address) = view.address.clone() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("remote.open", "Open")).color(TEXT_DIM));
            ui.label(RichText::new(&address).monospace().color(TEXT));
            if ui.small_button(t("remote.copy", "Copy")).clicked() {
                ui.ctx().copy_text(address.clone());
            }
        });
    }
    if let Some(ip) = view.address_ip.clone() {
        ui.label(
            RichText::new(tf(
                "remote.by_ip",
                "By address: {ip}. The browser warns again if the computer's address changes; the name above doesn't.",
                &[("ip", &ip)],
            ))
            .size(11.5)
            .color(TEXT_FAINT),
        );
    }
    if let Some(fingerprint) = &view.fingerprint {
        ui.add_space(4.0);
        ui.label(RichText::new(t("remote.fingerprint", "Key fingerprint (SHA-256)")).size(11.5).color(TEXT_DIM));
        // In two lines of sixteen pairs, as easy to read off against the browser's.
        let pairs: Vec<&str> = fingerprint.split(':').collect();
        let half = pairs.len().div_ceil(2);
        for line in pairs.chunks(half.max(1)) {
            ui.label(RichText::new(line.join(":")).monospace().size(11.0).color(TEXT));
        }
        ui.label(
            RichText::new(
                t("remote.fingerprint.about", "The browser warns the first time, as nobody but this computer vouches for its certificate. Check that the key fingerprint is this one before going on (docs/remote-api.md says how). A certificate made again keeps the key, so this stays the same."),
            )
            .size(11.5)
            .color(TEXT_FAINT),
        );
        if ui
            .button(t("remote.new_certificate", "Make a New Certificate"))
            .on_hover_text(t(
                "remote.new_certificate.hint",
                "A new key too: devices see a new fingerprint and a warning again",
            ))
            .clicked()
        {
            view.request = Some(Request::NewCertificate);
        }
    }

    ui.add_space(14.0);
    crate::studio::caption(ui, t("remote.password.caption", "PASSWORD"));
    if view.hashing {
        ui.label(RichText::new(t("remote.password.setting", "Setting the password…")).color(TEXT_DIM));
    } else {
        if view.has_password && view.password_short {
            ui.label(
                RichText::new(tf(
                    "remote.password.saved_short",
                    "The saved password is shorter than {n} characters, which is too easy to guess. Set a new one.",
                    &[("n", &super::auth::SHORTEST_PASSWORD.to_string())],
                ))
                .color(crate::studio::TALLY),
            );
        }
        ui.label(
            RichText::new(if view.has_password {
                t("remote.password.forgets", "A new password forgets every device: each signs in again with it.")
            } else {
                t(
                    "remote.password.before",
                    "Set a password before switching Remote Control on: at least 10 letters, figures or signs.",
                )
            })
            .size(11.5)
            .color(TEXT_DIM),
        );
        egui::Grid::new("remote password").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(if view.has_password {
                t("remote.password.new", "New password")
            } else {
                t("remote.password", "Password")
            });
            ui.add(egui::TextEdit::singleline(&mut view.password).password(true).desired_width(220.0));
            ui.end_row();
            ui.label(t("remote.password.again", "Again"));
            ui.add(egui::TextEdit::singleline(&mut view.password_again).password(true).desired_width(220.0));
            ui.end_row();
        });
        if ui
            .button(if view.has_password {
                t("remote.password.change", "Change Password")
            } else {
                t("remote.password.set", "Set Password")
            })
            .clicked()
        {
            if view.password != view.password_again {
                view.said = Some((t("remote.password.differ", "The two passwords differ.").to_owned(), true));
            } else if view.password.chars().count() < super::auth::SHORTEST_PASSWORD {
                view.said = Some((
                    t("remote.password.too_short", "A password needs at least 10 letters, figures or signs.")
                        .to_owned(),
                    true,
                ));
            } else {
                view.request = Some(Request::Password(std::mem::take(&mut view.password)));
                view.password_again.clear();
            }
        }
    }
    if let Some((said, problem)) = &view.said {
        ui.add_space(6.0);
        ui.label(RichText::new(said).color(if *problem { crate::studio::TALLY } else { TEXT_DIM }));
    }

    if view.wrong > 0 {
        ui.add_space(6.0);
        let words = match view.wrong {
            1 => t("remote.wrong.one", "A wrong password was tried in the last hour.").to_owned(),
            n => tf(
                "remote.wrong.many",
                "{n} wrong passwords were tried in the last hour.",
                &[("n", &crate::i18n::count(n as u64))],
            ),
        };
        ui.label(RichText::new(words).color(if view.wrong >= 3 { crate::studio::TALLY } else { TEXT_DIM }));
    }

    ui.add_space(14.0);
    crate::studio::caption(ui, t("remote.connected.caption", "CONNECTED NOW"));
    if view.pages.is_empty() {
        ui.label(
            RichText::new(if view.on {
                t("remote.connected.none", "No page is connected.")
            } else {
                t("remote.off", "Remote Control is off.")
            })
            .color(TEXT_DIM),
        );
    }
    for page in view.pages.clone() {
        ui.horizontal(|ui| {
            let since = page.since.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            ui.label(RichText::new(&page.name).color(TEXT));
            ui.label(
                RichText::new(tf(
                    "remote.page.since",
                    "{ip} · since {ago}",
                    &[("ip", &page.ip.to_string()), ("ago", &ago(since))],
                ))
                .size(11.5)
                .color(TEXT_DIM),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button(t("remote.page.disconnect", "Disconnect"))
                    .on_hover_text(t("remote.page.disconnect.hint", "Close this page; the device stays remembered"))
                    .clicked()
                {
                    view.request = Some(Request::Disconnect(page.id));
                }
            });
        });
    }

    ui.add_space(14.0);
    crate::studio::caption(ui, t("remote.devices.caption", "DEVICES REMEMBERED"));
    if view.devices.is_empty() {
        ui.label(
            RichText::new(t("remote.devices.none", "None yet: a device is remembered when it signs in."))
                .color(TEXT_DIM),
        );
    }
    for device in view.devices.clone() {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&device.name).color(TEXT));
                ui.label(
                    RichText::new(tf(
                        "remote.device.seen",
                        "First seen {day} · last seen {ago} · {ip}",
                        &[("day", &day(device.first_seen)), ("ago", &ago(device.last_seen)), ("ip", &device.ip)],
                    ))
                    .size(11.5)
                    .color(TEXT_DIM),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button(t("remote.device.forget", "Forget"))
                    .on_hover_text(t(
                        "remote.device.forget.hint",
                        "It signs in again with the password; its pages close now",
                    ))
                    .clicked()
                {
                    view.request = Some(Request::Forget(device.id.clone()));
                }
            });
        });
    }
    if view.devices.len() > 1 && ui.button(t("remote.devices.forget_all", "Forget All")).clicked() {
        view.request = Some(Request::ForgetAll);
    }
}
