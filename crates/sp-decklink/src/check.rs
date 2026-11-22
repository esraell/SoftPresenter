//! A check to run from a terminal on a computer with a DeckLink card: lists the devices, the
//! ways each can play out and whether each can key. Used by the app's
//! `softpresenter decklink-check` and by `cargo run -p sp-decklink --example decklink_check`.

use std::process::ExitCode;

use crate::{Device, Driver, Found};

/// Runs the check with these words after the program's name. `program` is how to call it
/// again, for the examples it prints.
pub fn run(args: &[String], program: &str) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("list") => {
            let found = crate::list_now();
            print!("{}", listing(&found, program));
            ExitCode::SUCCESS
        }
        Some("help" | "--help" | "-h") => {
            println!("{}", usage(program));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("decklink-check: no such check '{other}'.\n{}", usage(program));
            ExitCode::from(2)
        }
    }
}

fn usage(program: &str) -> String {
    format!(
        "Usage:\n  {program}            lists the DeckLink devices, their modes and keying\n  {program} list       the same"
    )
}

/// What the listing says, as printed.
pub fn listing(found: &Found, program: &str) -> String {
    let mut out = String::new();
    out.push_str("DeckLink check, SoftPresenter for Blackmagic Design DeckLink\n");
    out.push_str(&format!("Driver: {}\n", found.driver.in_words()));
    if !matches!(found.driver, Driver::Ready(_)) {
        if matches!(found.driver, Driver::NotInstalled | Driver::TooOld(_)) {
            out.push_str(&format!("Desktop Video comes from {}\n", crate::DOWNLOAD_URL));
        }
        return out;
    }
    if found.devices.is_empty() {
        out.push_str("No DeckLink devices: the driver is installed, but sees no card. Is it plugged in and powered?\n");
        return out;
    }
    out.push_str(&format!("Devices: {}\n", found.devices.len()));
    for (at, device) in found.devices.iter().enumerate() {
        out.push('\n');
        out.push_str(&describe(at + 1, device));
    }
    out.push_str(&format!("\nThe numbers above are how to name a device to this check ({program} play NUMBER).\n"));
    out
}

/// One device, as the listing shows it.
fn describe(number: usize, device: &Device) -> String {
    let mut out = format!("{number}. {}\n", device.name);
    let mut about = vec![device.model.clone()];
    if let (Some(sub), Some(of)) = (device.sub_device, device.sub_devices) {
        about.push(format!("sub-device {} of {of}", sub + 1));
    }
    if let Some(profile) = device.profile_in_words() {
        about.push(format!("profile: {profile}"));
    }
    if let Some(id) = device.id {
        about.push(format!("id {id:#x}"));
    }
    out.push_str(&format!("   {}\n", about.join(", ")));
    if !device.can_play {
        out.push_str("   Can't play out (an input only, or not offered by this driver).\n");
        return out;
    }
    if !device.usable() {
        out.push_str("   Not used in the card's present profile, or has no mode SoftPresenter offers.\n");
    }
    out.push_str(&format!(
        "   Plays out on: {}. Keying: {}.\n",
        device.connections_in_words(),
        device.keying_in_words()
    ));
    let modes: Vec<String> = device
        .offered()
        .map(|mode| {
            let mut marks = String::new();
            for (has, mark) in [(mode.bgra, 'B'), (mode.yuv, 'Y'), (mode.keying, 'K')] {
                if has {
                    marks.push(mark);
                }
            }
            format!("{} {marks}", mode.name)
        })
        .collect();
    if modes.is_empty() {
        out.push_str("   No 720p or 1080 mode.\n");
    } else {
        out.push_str("   Modes (B: 8-bit BGRA, Y: 8-bit YUV, K: can key):\n");
        for row in modes.chunks(5) {
            out.push_str(&format!("     {}\n", row.join("   ")));
        }
    }
    if let Some(mode) = device.default_mode() {
        out.push_str(&format!("   A new output starts in {}.\n", mode.name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_shows_each_device_its_modes_and_keying() {
        let found = Found { driver: Driver::Ready("pretend".to_owned()), devices: crate::pretend::devices() };
        let text = listing(&found, "decklink-check");
        assert!(text.contains("1. Pretend DeckLink (1)"), "{text}");
        assert!(text.contains("sub-device 1 of 4"));
        assert!(text.contains("Keying: external and internal keying."));
        assert!(text.contains("1080i59.94 BYK"));
        assert!(text.contains("A new output starts in 1080i59.94."));
        assert!(!text.contains("NTSC"), "standard definition is not offered");
    }

    #[test]
    fn without_the_driver_or_a_card_the_listing_says_so_plainly() {
        let none = Found { driver: Driver::NotInstalled, devices: Vec::new() };
        let text = listing(&none, "decklink-check");
        assert!(text.contains("isn't installed") && text.contains(crate::DOWNLOAD_URL), "{text}");
        let old = Found { driver: Driver::TooOld("15.3.1".to_owned()), devices: Vec::new() };
        assert!(listing(&old, "decklink-check").contains("15.3.1 is installed; SoftPresenter needs 16.0"));
        let empty = Found { driver: Driver::Ready("16.3".to_owned()), devices: Vec::new() };
        assert!(listing(&empty, "decklink-check").contains("sees no card"));
    }
}
