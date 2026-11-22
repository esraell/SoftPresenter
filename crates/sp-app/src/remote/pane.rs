//! Settings › Remote Control: switching it on and off, the port, the password, the address to
//! open and the certificate's fingerprint, the pages connected and the devices remembered.

use egui::{Align, Layout, RichText};

use super::{Request, View};
use crate::studio::{TEXT, TEXT_DIM, TEXT_FAINT};

/// How long ago a time in seconds since 1970 was, in words.
fn ago(seconds: u64) -> String {
    let now = super::auth::now_seconds();
    let gone = now.saturating_sub(seconds);
    match gone {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{} min ago", gone / 60),
        3600..=86_399 => format!("{} h ago", gone / 3600),
        _ => format!("{} days ago", gone / 86_400),
    }
}

/// The date of a time in seconds since 1970, in the computer's own time zone.
fn day(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(seconds as i64, 0)
        .map(|at| at.with_timezone(&chrono::Local).format("%-d %b %Y").to_string())
        .unwrap_or_default()
}

pub fn pane(ui: &mut egui::Ui, view: &mut View) {
    let width = ui.available_width().min(600.0);
    ui.set_max_width(width);
    ui.label(
        RichText::new(
            "Run the show from another computer, a tablet or a phone on the church's network: open the address below \
             in its browser and sign in with the password. Each device is remembered until you forget it here.",
        )
        .color(TEXT_DIM),
    );
    ui.add_space(12.0);

    crate::studio::caption(ui, "REMOTE CONTROL");
    let mut on = view.on;
    let can_switch = view.has_password || view.on;
    ui.add_enabled_ui(can_switch, |ui| {
        if ui.checkbox(&mut on, "On").on_disabled_hover_text("Set a password first").changed() {
            view.request = Some(Request::Switch(on));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Port");
        let field = ui.add(egui::TextEdit::singleline(&mut view.port).desired_width(70.0));
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) || ui.button("Use").clicked() {
            match view.port.trim().parse::<u16>() {
                Ok(port) if port >= 1024 => view.request = Some(Request::Port(port)),
                _ => view.said = Some(("A port is a number from 1024 to 65535.".to_owned(), true)),
            }
        }
    });
    if let Some(address) = view.address.clone() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Open").color(TEXT_DIM));
            ui.label(RichText::new(&address).monospace().color(TEXT));
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(address.clone());
            }
        });
    }
    if let Some(ip) = view.address_ip.clone() {
        ui.label(
            RichText::new(format!(
                "By address: {ip}. The browser warns again if the computer's address changes; the name above doesn't."
            ))
            .size(11.5)
            .color(TEXT_FAINT),
        );
    }
    if let Some(fingerprint) = &view.fingerprint {
        ui.add_space(4.0);
        ui.label(RichText::new("Key fingerprint (SHA-256)").size(11.5).color(TEXT_DIM));
        // In two lines of sixteen pairs, as easy to read off against the browser's.
        let pairs: Vec<&str> = fingerprint.split(':').collect();
        let half = pairs.len().div_ceil(2);
        for line in pairs.chunks(half.max(1)) {
            ui.label(RichText::new(line.join(":")).monospace().size(11.0).color(TEXT));
        }
        ui.label(
            RichText::new(
                "The browser warns the first time, as nobody but this computer vouches for its certificate. Check that \
                 the key fingerprint is this one before going on (docs/remote-api.md says how). A certificate made \
                 again keeps the key, so this stays the same.",
            )
            .size(11.5)
            .color(TEXT_FAINT),
        );
        if ui
            .button("Make a New Certificate")
            .on_hover_text("A new key too: devices see a new fingerprint and a warning again")
            .clicked()
        {
            view.request = Some(Request::NewCertificate);
        }
    }

    ui.add_space(14.0);
    crate::studio::caption(ui, "PASSWORD");
    if view.hashing {
        ui.label(RichText::new("Setting the password…").color(TEXT_DIM));
    } else {
        if view.has_password && view.password_short {
            ui.label(
                RichText::new(format!(
                    "The saved password is shorter than {} characters, which is too easy to guess. Set a new one.",
                    super::auth::SHORTEST_PASSWORD
                ))
                .color(crate::studio::TALLY),
            );
        }
        ui.label(
            RichText::new(if view.has_password {
                "A new password forgets every device: each signs in again with it."
            } else {
                "Set a password before switching Remote Control on: at least 10 letters, figures or signs."
            })
            .size(11.5)
            .color(TEXT_DIM),
        );
        egui::Grid::new("remote password").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(if view.has_password { "New password" } else { "Password" });
            ui.add(egui::TextEdit::singleline(&mut view.password).password(true).desired_width(220.0));
            ui.end_row();
            ui.label("Again");
            ui.add(egui::TextEdit::singleline(&mut view.password_again).password(true).desired_width(220.0));
            ui.end_row();
        });
        if ui.button(if view.has_password { "Change Password" } else { "Set Password" }).clicked() {
            if view.password != view.password_again {
                view.said = Some(("The two passwords differ.".to_owned(), true));
            } else if view.password.chars().count() < super::auth::SHORTEST_PASSWORD {
                view.said = Some(("A password needs at least 10 letters, figures or signs.".to_owned(), true));
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
            1 => "A wrong password was tried in the last hour.".to_owned(),
            n => format!("{n} wrong passwords were tried in the last hour."),
        };
        ui.label(RichText::new(words).color(if view.wrong >= 3 { crate::studio::TALLY } else { TEXT_DIM }));
    }

    ui.add_space(14.0);
    crate::studio::caption(ui, "CONNECTED NOW");
    if view.pages.is_empty() {
        ui.label(
            RichText::new(if view.on { "No page is connected." } else { "Remote Control is off." }).color(TEXT_DIM),
        );
    }
    for page in view.pages.clone() {
        ui.horizontal(|ui| {
            let since = page.since.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            ui.label(RichText::new(&page.name).color(TEXT));
            ui.label(RichText::new(format!("{} · since {}", page.ip, ago(since))).size(11.5).color(TEXT_DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Disconnect").on_hover_text("Close this page; the device stays remembered").clicked()
                {
                    view.request = Some(Request::Disconnect(page.id));
                }
            });
        });
    }

    ui.add_space(14.0);
    crate::studio::caption(ui, "DEVICES REMEMBERED");
    if view.devices.is_empty() {
        ui.label(RichText::new("None yet: a device is remembered when it signs in.").color(TEXT_DIM));
    }
    for device in view.devices.clone() {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&device.name).color(TEXT));
                ui.label(
                    RichText::new(format!(
                        "First seen {} · last seen {} · {}",
                        day(device.first_seen),
                        ago(device.last_seen),
                        device.ip
                    ))
                    .size(11.5)
                    .color(TEXT_DIM),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button("Forget")
                    .on_hover_text("It signs in again with the password; its pages close now")
                    .clicked()
                {
                    view.request = Some(Request::Forget(device.id.clone()));
                }
            });
        });
    }
    if view.devices.len() > 1 && ui.button("Forget All").clicked() {
        view.request = Some(Request::ForgetAll);
    }
}
