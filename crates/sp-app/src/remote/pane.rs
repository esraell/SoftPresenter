//! Settings › Phone remote, in the cards and rows of the other Settings pages: Remote Control
//! (switching it on, the address to open, and under Port and certificate the port and the
//! certificate's fingerprint), the one password, the pages connected now and the devices
//! remembered. Every hint is a line on the page, never only under the pointer.

use egui::{FontId, Rect, Sense, pos2, vec2};

use super::{Request, View};
use crate::home::settings::parts;
use crate::i18n::{t, tf, tn};
use crate::studio::{CARE, Flat, TEXT_FAINT};

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

/// How wide a password field is, and the port's.
const FIELD_WIDTH: f32 = 190.0;
const PORT_WIDTH: f32 = 96.0;

pub fn pane(ui: &mut egui::Ui, view: &mut View) {
    // What the last change did, or why it couldn't, above the cards, whichever card it came from.
    if let Some((said, problem)) = &view.said {
        let tone = if *problem { crate::cli_install::Tone::Trouble } else { crate::cli_install::Tone::Good };
        parts::said_line(ui, Some((tone, said)), "");
    }
    remote_card(ui, view);
    password_card(ui, view);
    connected_card(ui, view);
    devices_card(ui, view);
}

/// Remote Control: Turn on Remote Control (greyed, saying why, until a password is set); while
/// on, the address to open with Copy; and, folded, the port and the certificate.
fn remote_card(ui: &mut egui::Ui, view: &mut View) {
    parts::card(ui, t("remote.title", "Remote Control"), "", None, |ui| {
        let can_switch = view.has_password || view.on;
        let note = if can_switch {
            t(
                "remote.about",
                "Run the show from another computer, a tablet or a phone on the church's network: open the address below in its browser and sign in with the password. Each device is remembered until you forget it here.",
            )
        } else {
            t("remote.on.disabled", "Set a password first")
        };
        let label = t("remote.turn_on", "Turn on Remote Control");
        let key = |ui: &mut egui::Ui, place: Rect| {
            let id = ui.scope_id().with("remote on");
            let words = if view.on { t("settings.on", "On") } else { t("settings.off", "Off") };
            let look = if view.on { Flat::On } else { Flat::Plain };
            // Greyed, taking no clicks, until there is a password, as before.
            crate::studio::flat_button(ui, place, id, words, FontId::proportional(12.5), look, can_switch).clicked()
        };
        let mut clicked = false;
        if can_switch {
            parts::row(ui, label, note, parts::on_off_size(ui), |ui, place| clicked = key(ui, place));
        } else {
            parts::row_greyed(ui, label, note, parts::on_off_size(ui), |ui, place| clicked = key(ui, place));
        }
        if clicked {
            view.request = Some(Request::Switch(!view.on));
        }
        if let Some(address) = view.address.clone() {
            address_row(ui, view, &address);
        }
        parts::fold(ui, "remote more", t("remote.more", "Port and certificate"), folded_open(), |ui| {
            port_row(ui, view);
            certificate(ui, view);
        });
    });
}

/// Testing aid: `SOFTPRESENTER_CAPTURE_PREFERENCES=remote-more` opens the page with Port and
/// certificate folded out.
fn folded_open() -> bool {
    std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES").is_ok_and(|page| page == "remote-more")
}

/// Open: the address in the fixed-width face with Copy; the address by figures as its line.
/// Where the address and Copy don't fit beside the row's words, the address goes on a line of its
/// own under them, whole.
fn address_row(ui: &mut egui::Ui, view: &View, address: &str) {
    let copy = t("remote.copy", "Copy");
    let key = parts::key_size(ui, copy);
    let font = crate::studio::fixed_semibold(12.5);
    let wide = ui.painter().layout_no_wrap(address.to_owned(), font.clone(), crate::studio::TEXT).size().x;
    let whole = vec2(wide.ceil() + 14.0, 24.0);
    let beside = whole.x + 8.0 + key.x + parts::LABEL_ROOM <= ui.available_width();
    let note = view
        .address_ip
        .as_ref()
        .map(|ip| {
            tf(
                "remote.by_ip",
                "By address: {ip}. The browser warns again if the computer's address changes; the name above doesn't.",
                &[("ip", ip)],
            )
        })
        .unwrap_or_default();
    let tag = |ui: &egui::Ui, at: Rect, font: FontId| {
        ui.painter().rect_filled(at, 6.0, crate::studio::SELECTED);
        let galley = parts::one_line(ui, address, font, crate::studio::TEXT, at.width() - 14.0);
        ui.painter().galley(pos2(at.min.x + 7.0, at.center().y - galley.size().y / 2.0), galley, crate::studio::TEXT);
    };
    let mut copied = false;
    let control = if beside { vec2(whole.x + 8.0 + key.x, 30.0) } else { key };
    parts::row(ui, t("remote.open", "Open"), &note, control, |ui, place| {
        if beside {
            tag(ui, Rect::from_min_size(place.min + vec2(0.0, 3.0), whole), font.clone());
        }
        let key_place = Rect::from_min_size(pos2(place.max.x - key.x, place.min.y), key);
        copied = parts::key(ui, key_place, ui.scope_id().with("remote copy"), copy, false).clicked();
    });
    if !beside {
        let width = ui.available_width();
        let (line, _) = ui.allocate_exact_size(vec2(width, whole.y), Sense::hover());
        // Smaller, rather than cut short, where even a line of its own is too narrow for it.
        let size = (12.5 * (width - 15.0) / wide).clamp(10.0, 12.5);
        let wide = wide * size / 12.5;
        let at = Rect::from_min_size(line.min, vec2((wide.ceil() + 14.0).min(width), whole.y));
        tag(ui, at, crate::studio::fixed_semibold(size));
        ui.add_space(12.0);
    }
    if copied {
        ui.ctx().copy_text(address.to_owned());
    }
}

/// The port, typed and used with Use or Return.
fn port_row(ui: &mut egui::Ui, view: &mut View) {
    let use_words = t("remote.port.use", "Use");
    let key = parts::key_size(ui, use_words);
    let range = t("remote.port.range", "A port is a number from 1024 to 65535.");
    parts::row(ui, t("remote.port", "Port"), range, vec2(PORT_WIDTH + 8.0 + key.x, 32.0), |ui, place| {
        let field = parts::field(ui, Rect::from_min_size(place.min, vec2(PORT_WIDTH, 32.0)), &mut view.port, false, "");
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let key_place = Rect::from_min_size(place.min + vec2(PORT_WIDTH + 8.0, 1.0), key);
        if parts::key(ui, key_place, ui.scope_id().with("remote port use"), use_words, false).clicked() || entered {
            match view.port.trim().parse::<u16>() {
                Ok(port) if port >= 1024 => view.request = Some(Request::Port(port)),
                _ => view.said = Some((range.to_owned(), true)),
            }
        }
    });
}

/// The certificate's key fingerprint, to check against what a browser shows, in two lines of
/// sixteen pairs, then Make a New Certificate.
fn certificate(ui: &mut egui::Ui, view: &mut View) {
    let Some(fingerprint) = view.fingerprint.clone() else { return };
    let about = t(
        "remote.fingerprint.about",
        "The browser warns the first time, as nobody but this computer vouches for its certificate. Check that the key fingerprint is this one before going on (docs/remote-api.md says how). A certificate made again keeps the key, so this stays the same.",
    );
    parts::row(ui, t("remote.fingerprint", "Key fingerprint (SHA-256)"), about, vec2(0.0, 0.0), |_, _| {});
    let pairs: Vec<&str> = fingerprint.split(':').collect();
    let half = pairs.len().div_ceil(2);
    let lines: Vec<String> = pairs.chunks(half.max(1)).map(|line| line.join(":")).collect();
    parts::code(ui, &lines.join("\n"));
    ui.add_space(12.0);
    let make = t("remote.new_certificate", "Make a New Certificate");
    let hint = t("remote.new_certificate.hint", "A new key too: devices see a new fingerprint and a warning again");
    parts::row(ui, make, hint, parts::key_size(ui, make), |ui, place| {
        if parts::key(ui, place, ui.scope_id().with("remote new certificate"), make, false).clicked() {
            view.request = Some(Request::NewCertificate);
        }
    });
}

/// The one password: Set or Not set yet. Once set, one row with Change Password…; with none, or
/// changing, the two fields and Set Password (or Cancel and Change Password); "Setting the
/// password…" while it is kept; a saved one too short to be safe, and wrong ones tried lately.
fn password_card(ui: &mut egui::Ui, view: &mut View) {
    let state = if view.has_password {
        t("remote.password.is_set", "Set")
    } else {
        t("remote.password.not_set", "Not set yet")
    };
    parts::card(ui, t("remote.password", "Password"), state, None, |ui| {
        if view.has_password && view.password_short {
            short_warning(ui);
        }
        let forgets = t("remote.password.forgets", "A new password forgets every device: each signs in again with it.");
        if view.hashing {
            let setting = t("remote.password.setting", "Setting the password\u{2026}");
            parts::row(ui, t("remote.password", "Password"), setting, vec2(0.0, 32.0), |_, _| {});
        } else if view.has_password && !view.changing {
            let change = t("remote.password.change_open", "Change Password\u{2026}");
            parts::row(ui, t("remote.password", "Password"), forgets, parts::key_size(ui, change), |ui, place| {
                if parts::key(ui, place, ui.scope_id().with("remote password change"), change, false).clicked() {
                    view.changing = true;
                }
            });
        } else {
            password_fields(ui, view, forgets);
        }
        if view.wrong > 0 {
            let words = match view.wrong {
                1 => t("remote.wrong.one", "A wrong password was tried in the last hour.").to_owned(),
                n => tf(
                    "remote.wrong.many",
                    "{n} wrong passwords were tried in the last hour.",
                    &[("n", &crate::i18n::count(n as u64))],
                ),
            };
            parts::coloured_note(ui, &words, if view.wrong >= 3 { CARE } else { TEXT_FAINT });
        }
    });
}

/// "The saved password is shorter than…", in amber, above the password's rows.
fn short_warning(ui: &mut egui::Ui) {
    let words = tf(
        "remote.password.saved_short",
        "The saved password is shorter than {n} characters, which is too easy to guess. Set a new one.",
        &[("n", &super::auth::SHORTEST_PASSWORD.to_string())],
    );
    parts::coloured_note(ui, &words, CARE);
}

/// The password typed twice, then, on a line of their own at the right, Cancel (while changing
/// one) and Set Password or Change Password, the one main key.
fn password_fields(ui: &mut egui::Ui, view: &mut View, forgets: &str) {
    let (label, note) = if view.has_password {
        (t("remote.password.new", "New password"), forgets)
    } else {
        (
            t("remote.password", "Password"),
            t(
                "remote.password.before",
                "Set a password before switching Remote Control on: at least 10 letters, figures or signs.",
            ),
        )
    };
    let size = vec2(FIELD_WIDTH, 32.0);
    parts::row(ui, label, note, size, |ui, place| {
        parts::field(ui, place, &mut view.password, true, "");
    });
    parts::row(ui, t("remote.password.again", "Again"), "", size, |ui, place| {
        parts::field(ui, place, &mut view.password_again, true, "");
    });
    let set = if view.has_password {
        t("remote.password.change", "Change Password")
    } else {
        t("remote.password.set", "Set Password")
    };
    let cancel = t("remote.password.cancel", "Cancel");
    let set_size = parts::key_size(ui, set);
    let cancel_size = parts::key_size(ui, cancel);
    let width = ui.available_width();
    let (line, _) = ui.allocate_exact_size(vec2(width, set_size.y), Sense::hover());
    let set_place = Rect::from_min_size(pos2(line.max.x - set_size.x, line.min.y), set_size);
    let font = FontId::proportional(12.5);
    let pressed = crate::studio::flat_button(
        ui,
        set_place,
        ui.scope_id().with("remote password set"),
        set,
        font,
        Flat::Main,
        true,
    )
    .clicked();
    if view.has_password {
        let cancel_place = Rect::from_min_size(pos2(set_place.min.x - 8.0 - cancel_size.x, line.min.y), cancel_size);
        if parts::key(ui, cancel_place, ui.scope_id().with("remote password cancel"), cancel, false).clicked() {
            view.changing = false;
            view.password.clear();
            view.password_again.clear();
        }
    }
    ui.add_space(12.0);
    if pressed {
        set_password(view);
    }
}

/// Asks for the password typed, when the two agree and it is long enough; otherwise says why not.
fn set_password(view: &mut View) {
    if view.password != view.password_again {
        view.said = Some((t("remote.password.differ", "The two passwords differ.").to_owned(), true));
    } else if view.password.chars().count() < super::auth::SHORTEST_PASSWORD {
        view.said = Some((
            t("remote.password.too_short", "A password needs at least 10 letters, figures or signs.").to_owned(),
            true,
        ));
    } else {
        view.request = Some(Request::Password(std::mem::take(&mut view.password)));
        view.password_again.clear();
        view.changing = false;
    }
}

/// The pages open now, each with where it is and since when, and Disconnect.
fn connected_card(ui: &mut egui::Ui, view: &mut View) {
    let listed = !view.pages.is_empty();
    let about =
        if listed { t("remote.connected.about", "Disconnect closes a page; the device stays remembered") } else { "" };
    parts::card(ui, t("remote.connected.title", "Connected now"), about, None, |ui| {
        if !listed {
            let none = if view.on {
                t("remote.connected.none", "No page is connected.")
            } else {
                t("remote.off", "Remote Control is off.")
            };
            parts::note(ui, none);
        }
        let disconnect = t("remote.page.disconnect", "Disconnect");
        for page in view.pages.clone() {
            let since = page.since.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let note =
                tf("remote.page.since", "{ip} · since {ago}", &[("ip", &page.ip.to_string()), ("ago", &ago(since))]);
            parts::row(ui, &page.name, &note, parts::key_size(ui, disconnect), |ui, place| {
                if parts::key(ui, place, ui.scope_id().with(("remote disconnect", page.id)), disconnect, false)
                    .clicked()
                {
                    view.request = Some(Request::Disconnect(page.id));
                }
            });
        }
    });
}

/// The devices that signed in, each with when it was first and last seen and Forget; Forget All
/// at the heading's right when there are two or more.
fn devices_card(ui: &mut egui::Ui, view: &mut View) {
    let listed = !view.devices.is_empty();
    let about =
        if listed { t("remote.devices.about", "A device forgotten signs in again with the password") } else { "" };
    let all = (view.devices.len() > 1).then(|| t("remote.devices.forget_all", "Forget All"));
    let forget_all = parts::card_with_key(ui, t("remote.devices.title", "Devices remembered"), about, all, |ui| {
        if !listed {
            parts::note(ui, t("remote.devices.none", "None yet: a device is remembered when it signs in."));
        }
        let forget = t("remote.device.forget", "Forget");
        for device in view.devices.clone() {
            let note = tf(
                "remote.device.seen",
                "First seen {day} · last seen {ago} · {ip}",
                &[("day", &day(device.first_seen)), ("ago", &ago(device.last_seen)), ("ip", &device.ip)],
            );
            parts::row(ui, &device.name, &note, parts::key_size(ui, forget), |ui, place| {
                if parts::key(ui, place, ui.scope_id().with(("remote forget", &device.id)), forget, false).clicked() {
                    view.request = Some(Request::Forget(device.id.clone()));
                }
            });
        }
    });
    if forget_all {
        view.request = Some(Request::ForgetAll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws the page twice (the second with every size known) in a page `width` wide, with what
    /// the pointer did in `events` on the second, and gives the words drawn and where.
    fn drawn(ctx: &egui::Context, view: &mut View, width: f32, events: Vec<egui::Event>) -> Vec<(String, Rect)> {
        let mut words = Vec::new();
        for round in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 3000.0))),
                events: if round == 1 { events.clone() } else { Vec::new() },
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| pane(ui, view));
            });
            words = words_of(&output);
            output.drop_without_applying_deltas();
        }
        words
    }

    /// Every piece of words a frame drew, and where.
    fn words_of(output: &egui::FullOutput) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut into = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut into));
        into
    }

    /// Clicks the words `name` where the page draws them (the last place, when there are several).
    fn click(ctx: &egui::Context, view: &mut View, name: &str) {
        let words = drawn(ctx, view, 740.0, vec![]);
        let at = words.iter().rev().find(|(said, _)| said == name).map(|(_, rect)| rect.center());
        let at = at.unwrap_or_else(|| panic!("{name} isn't drawn"));
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        drawn(ctx, view, 740.0, vec![egui::Event::PointerMoved(at), press(true)]);
        drawn(ctx, view, 740.0, vec![press(false)]);
    }

    fn device(id: &str, name: &str, ip: &str) -> super::super::auth::Device {
        let now = super::super::auth::now_seconds();
        super::super::auth::Device {
            id: id.to_owned(),
            name: name.to_owned(),
            token: String::new(),
            first_seen: now,
            last_seen: now,
            ip: ip.to_owned(),
        }
    }

    fn view() -> View {
        View { port: "4316".to_owned(), ..View::default() }
    }

    /// On, with a password, a page connected and two devices remembered.
    fn busy() -> View {
        View {
            on: true,
            has_password: true,
            address: Some("https://church-mac.local:4316".to_owned()),
            address_ip: Some("https://192.168.1.20:4316".to_owned()),
            fingerprint: Some(vec!["3F"; 32].join(":")),
            pages: vec![super::super::server::Page {
                id: 7,
                device: "one".to_owned(),
                name: "Safari on an iPhone".to_owned(),
                ip: std::net::IpAddr::from([192, 168, 1, 31]),
                since: std::time::SystemTime::now(),
            }],
            devices: vec![
                device("one", "Safari on an iPhone", "192.168.1.31"),
                device("two", "Chrome on a laptop", "192.168.1.40"),
            ],
            ..view()
        }
    }

    fn has(words: &[(String, Rect)], wanted: &str) -> bool {
        words.iter().any(|(said, _)| said == wanted)
    }

    #[test]
    fn phone_remote_is_four_cards_and_says_a_password_comes_first() {
        let ctx = crate::studio::tests::styled();
        let mut view = view();
        let words = drawn(&ctx, &mut view, 740.0, vec![]);
        for title in ["Remote Control", "Password", "Connected now", "Devices remembered"] {
            assert!(has(&words, title), "{title}");
        }
        assert!(has(&words, "Turn on Remote Control"));
        assert!(has(&words, "Set a password first"), "said on the page, not only under the pointer");
        assert!(has(&words, "NOT SET YET"));
        assert!(has(&words, "Set Password") && !has(&words, "Cancel"), "nothing to cancel before the first");
        assert!(has(&words, "Remote Control is off."));
        assert!(has(&words, "None yet: a device is remembered when it signs in."));
        assert!(!has(&words, "Open"), "no address while off");
        // The port and the certificate are folded away.
        assert!(has(&words, "Port and certificate") && !has(&words, "Port"));
        // The key takes no clicks until there is a password, as before.
        click(&ctx, &mut view, "Off");
        assert_eq!(view.request, None);
    }

    #[test]
    fn phone_remote_shows_the_address_pages_and_devices_while_on() {
        let ctx = crate::studio::tests::styled();
        let mut view = View { password_short: true, wrong: 3, ..busy() };
        let words = drawn(&ctx, &mut view, 740.0, vec![]);
        assert!(has(&words, "Open") && has(&words, "https://church-mac.local:4316") && has(&words, "Copy"));
        assert!(words.iter().any(|(said, _)| said.starts_with("By address: https://192.168.1.20:4316.")));
        assert!(has(&words, "SET") && has(&words, "Change Password\u{2026}"));
        assert!(words.iter().any(|(said, _)| said.starts_with("The saved password is shorter than 10 characters")));
        assert!(has(&words, "3 wrong passwords were tried in the last hour."));
        assert!(has(&words, "DISCONNECT CLOSES A PAGE; THE DEVICE STAYS REMEMBERED"));
        assert!(has(&words, "A DEVICE FORGOTTEN SIGNS IN AGAIN WITH THE PASSWORD"));
        assert!(has(&words, "Disconnect") && has(&words, "Forget") && has(&words, "Forget All"));
        assert!(!has(&words, "Again"), "the fields show only once Change Password… is chosen");
    }

    #[test]
    fn phone_remote_keys_ask_the_app_what_they_asked_before() {
        let ctx = crate::studio::tests::styled();
        let mut view = busy();
        click(&ctx, &mut view, "On");
        assert_eq!(view.request.take(), Some(Request::Switch(false)), "On switches it off");
        click(&ctx, &mut view, "Disconnect");
        assert_eq!(view.request.take(), Some(Request::Disconnect(7)));
        click(&ctx, &mut view, "Forget All");
        assert_eq!(view.request.take(), Some(Request::ForgetAll));
        click(&ctx, &mut view, "Forget");
        assert_eq!(view.request.take(), Some(Request::Forget("two".to_owned())));
        // Change Password… opens the two fields, and Cancel closes them.
        click(&ctx, &mut view, "Change Password\u{2026}");
        assert!(view.changing);
        click(&ctx, &mut view, "Cancel");
        assert!(!view.changing);
        click(&ctx, &mut view, "Change Password\u{2026}");
        view.password = "short".to_owned();
        view.password_again = "short".to_owned();
        click(&ctx, &mut view, "Change Password");
        assert_eq!(view.request, None);
        assert_eq!(view.said.take().map(|said| said.1), Some(true), "too short is said, as a problem");
        view.password = "lanterns-on-the-harbour".to_owned();
        view.password_again = "lanterns-on-the-harbour".to_owned();
        click(&ctx, &mut view, "Change Password");
        assert_eq!(view.request.take(), Some(Request::Password("lanterns-on-the-harbour".to_owned())));
        assert!(view.password.is_empty() && view.password_again.is_empty(), "neither is kept once asked");
        assert!(!view.changing);
        // Under Port and certificate: the port, and a new certificate.
        click(&ctx, &mut view, "Port and certificate");
        view.port = "80".to_owned();
        click(&ctx, &mut view, "Use");
        assert_eq!((view.request.take(), view.said.take().map(|said| said.1)), (None, Some(true)));
        view.port = "4320".to_owned();
        click(&ctx, &mut view, "Use");
        assert_eq!(view.request.take(), Some(Request::Port(4320)));
        click(&ctx, &mut view, "Make a New Certificate");
        assert_eq!(view.request.take(), Some(Request::NewCertificate));
    }

    #[test]
    fn phone_remote_password_fields_take_what_is_typed() {
        let ctx = crate::studio::tests::styled();
        let mut view = view();
        let words = drawn(&ctx, &mut view, 740.0, vec![]);
        let again = words.iter().find(|(said, _)| said == "Again").map(|(_, rect)| rect.center().y).expect("Again");
        // The field is at the row's right end, inside the card's margin.
        let at = egui::pos2(740.0 - 18.0 - FIELD_WIDTH / 2.0, again);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        drawn(&ctx, &mut view, 740.0, vec![egui::Event::PointerMoved(at), press(true)]);
        drawn(&ctx, &mut view, 740.0, vec![press(false)]);
        drawn(&ctx, &mut view, 740.0, vec![egui::Event::Text("harbour".to_owned())]);
        assert_eq!(view.password_again, "harbour");
        assert!(view.password.is_empty());
    }

    #[test]
    fn phone_remote_fits_the_narrowest_settings_page() {
        use crate::i18n::Language;
        for language in [Language::English, Language::Spanish, Language::Russian, Language::Ukrainian] {
            crate::i18n::with_language(language, || {
                let ctx = crate::studio::tests::styled();
                let mut view = View {
                    address: Some("https://a-rather-long-computer-name-for-a-church.local:4316".to_owned()),
                    password_short: true,
                    wrong: 4,
                    changing: true,
                    ..busy()
                };
                // The least Settings window leaves its page 490 wide.
                let drawn = drawn(&ctx, &mut view, 490.0, vec![]);
                for (words, rect) in drawn.iter().filter(|(w, _)| !w.is_empty()) {
                    assert!(rect.max.x <= 490.5, "{language:?}: \u{201c}{words}\u{201d} ends at {}", rect.max.x);
                }
                let place = |wanted: &str| drawn.iter().find(|(said, _)| said == wanted).map(|(_, rect)| *rect);
                // The address is never cut short: on a line of its own where it must be.
                let address =
                    place("https://a-rather-long-computer-name-for-a-church.local:4316").expect("the address");
                // Whole, at least: as wide as all of it at the least size it may take.
                let whole = ctx.fonts_mut(|fonts| {
                    let text = "https://a-rather-long-computer-name-for-a-church.local:4316".to_owned();
                    fonts.layout_no_wrap(text, crate::studio::fixed_semibold(10.0), egui::Color32::WHITE).size().x
                });
                assert!(address.width() >= whole - 1.0, "{language:?}: the address is cut at {}", address.width());
                assert!(address.max.x <= 490.5);
                // A row with a field keeps its height: the next row starts under its words.
                let forgets =
                    t("remote.password.forgets", "A new password forgets every device: each signs in again with it.");
                let (note, again) =
                    (place(forgets).expect("the note"), place(t("remote.password.again", "Again")).expect("Again"));
                assert!(
                    again.min.y > note.max.y + 12.0,
                    "{language:?}: Again at {} under a note ending at {}",
                    again.min.y,
                    note.max.y
                );
            });
        }
    }
}
