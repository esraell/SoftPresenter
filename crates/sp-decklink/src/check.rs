//! A check to run from a terminal on a computer with a DeckLink card: lists the devices, the
//! ways each can play out and whether each can key, and with `play` sends a test picture to one
//! device for a few seconds, through the same output the app uses. Used by the app's
//! `softpresenter decklink-check` and by `cargo run -p sp-decklink --example decklink_check`.
//!
//! Nothing is ever sent without a device named on the command line, and the check says what it
//! will send where, and waits for "yes", before it does: the card may be on a video chain in
//! front of people.

use std::io::{BufRead, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{Alpha, DeckLinkOutput, Device, Driver, Found, Keying, State, Wanted};

/// Runs the check with these words after the program's name. `program` is how to call it
/// again, for the examples it prints.
pub fn run(args: &[String], program: &str) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("list") => {
            let found = crate::list_now();
            print!("{}", listing(&found, program));
            ExitCode::SUCCESS
        }
        Some("play") => play(&args[1..], program),
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
        "Usage:
  {program}
      lists the DeckLink devices, their formats and keying
  {program} play DEVICE [--format FORMAT] [--seconds N] [--key external|internal]
                [--alpha straight|premultiplied] [--yes]
      sends colour bars with a bar moving across them (so a freeze shows) to DEVICE (its
      number in the list, or its name) for N seconds (10 unless told), in FORMAT (the format
      a new output starts in unless told), then stops. With --key, the card's keyer is on and
      the picture has a clear part and soft edges, to check the key. It says what it will send
      where and waits for yes first, unless --yes."
    )
}

/// What `play` was asked for.
#[derive(Debug, PartialEq)]
struct Asked {
    device: String,
    format: Option<String>,
    seconds: u64,
    keying: Keying,
    alpha: Alpha,
    yes: bool,
}

/// Reads `play`'s words. The device must be named: nothing is ever sent to a device that isn't.
fn asked(args: &[String]) -> Result<Asked, String> {
    let mut asked = Asked {
        device: String::new(),
        format: None,
        seconds: 10,
        keying: Keying::Off,
        alpha: Alpha::Straight,
        yes: false,
    };
    let mut words = args.iter();
    let mut device: Vec<String> = Vec::new();
    while let Some(word) = words.next() {
        let mut value = |name: &str| words.next().cloned().ok_or_else(|| format!("{name} needs a value."));
        match word.as_str() {
            "--format" | "--mode" => asked.format = Some(value("--format")?),
            "--seconds" => {
                asked.seconds = value("--seconds")?
                    .parse::<u64>()
                    .ok()
                    .filter(|seconds| (1..=3600).contains(seconds))
                    .ok_or("--seconds is a whole number from 1 to 3600.")?;
            }
            "--key" => {
                asked.keying = match Keying::parse(&value("--key")?) {
                    Some(Keying::Off) | None => return Err("--key is external or internal.".to_owned()),
                    Some(keying) => keying,
                }
            }
            "--alpha" => {
                asked.alpha = match value("--alpha")?.as_str() {
                    "straight" => Alpha::Straight,
                    "premultiplied" => Alpha::Premultiplied,
                    _ => return Err("--alpha is straight or premultiplied.".to_owned()),
                }
            }
            "--yes" => asked.yes = true,
            other if other.starts_with("--") => return Err(format!("There is no {other}.")),
            other => device.push(other.to_owned()),
        }
    }
    asked.device = device.join(" ");
    if asked.device.trim().is_empty() {
        return Err("Say which device to send to: its number in the list, or its name. Nothing was sent.".to_owned());
    }
    Ok(asked)
}

/// The device a word names: its number in the listing, or its whole name (in any case).
fn chosen<'f>(found: &'f Found, typed: &str) -> Option<&'f Device> {
    match typed.trim().parse::<usize>() {
        Ok(number) => found.devices.get(number.checked_sub(1)?),
        Err(_) => found.devices.iter().find(|device| device.name.eq_ignore_ascii_case(typed.trim())),
    }
}

/// `play`: sends the test picture to one device, as asked, then stops.
fn play(args: &[String], program: &str) -> ExitCode {
    let asked = match asked(args) {
        Ok(asked) => asked,
        Err(why) => {
            eprintln!("decklink-check: {why}\n{}", usage(program));
            return ExitCode::from(2);
        }
    };
    let found = crate::list_now();
    if !matches!(found.driver, Driver::Ready(_)) {
        eprintln!("decklink-check: {} Nothing was sent.", found.driver.in_words());
        return ExitCode::from(1);
    }
    let Some(device) = chosen(&found, &asked.device) else {
        eprintln!("decklink-check: there is no device \"{}\". Nothing was sent.\n", asked.device);
        eprint!("{}", listing(&found, program));
        return ExitCode::from(1);
    };
    let format = match &asked.format {
        Some(name) => device.mode(name),
        None => device.start(asked.keying != Keying::Off).map(|start| start.mode),
    };
    let Some(format) = format else {
        let names: Vec<&str> = device.offered().map(|mode| mode.name.as_str()).collect();
        eprintln!(
            "decklink-check: {} has no format {}. Its formats: {}. Nothing was sent.",
            device.name,
            asked.format.as_deref().unwrap_or("to start in"),
            names.join(", ")
        );
        return ExitCode::from(1);
    };
    let wanted = Wanted { device: device.name.clone(), id: device.id, mode: format.name.clone(), keying: asked.keying };
    let pixel_format = match crate::play::choose(device, &wanted) {
        Ok((_, pixel_format)) => pixel_format,
        Err(why) => {
            eprintln!("decklink-check: {why} Nothing was sent.");
            return ExitCode::from(1);
        }
    };
    println!("{}", plan(device, format, pixel_format, &asked));
    if !asked.yes && !confirmed() {
        println!("Nothing was sent.");
        return ExitCode::SUCCESS;
    }
    let log: crate::Log = Arc::new(|line: &str, problem| {
        if problem {
            eprintln!("  ! {line}");
        } else {
            println!("  {line}");
        }
    });
    let output = match DeckLinkOutput::start(wanted, log) {
        Ok(output) => output,
        Err(why) => {
            eprintln!("decklink-check: {why}");
            return ExitCode::from(1);
        }
    };
    let (width, height) = (1920u32, 1080u32);
    let keyed = asked.keying != Keying::Off;
    let started = Instant::now();
    let length = Duration::from_secs(asked.seconds);
    let frame = format.frame_time();
    let mut said_state = None;
    while started.elapsed() < length {
        let picture = test_picture(width, height, started.elapsed().as_secs_f32(), keyed);
        output.show_shared(width, height, Arc::new(picture), if keyed { asked.alpha } else { Alpha::Opaque });
        let state = output.state();
        if said_state.as_ref() != Some(&state) {
            match &state {
                State::Starting => println!("  Opening {}…", device.name),
                State::Playing(what) => println!("  Playing at {what}."),
                State::Waiting(why) => eprintln!("  ! {why}"),
            }
            said_state = Some(state);
        }
        std::thread::sleep(frame);
    }
    let counts = output.counts();
    let played = matches!(output.state(), State::Playing(_));
    output.stop();
    println!(
        "Stopped after {:.1} s: {} frames played, {} late, {} dropped. {} is no longer sent anything.",
        started.elapsed().as_secs_f32(),
        counts.played,
        counts.late,
        counts.dropped,
        device.name
    );
    if played && counts.played > 0 { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

/// What `play` will send where, in plain words, said before anything is sent.
fn plan(device: &Device, format: &crate::Mode, pixel_format: crate::PixelFormat, asked: &Asked) -> String {
    let mut about = vec![device.model.clone()];
    if let (Some(sub), Some(of)) = (device.sub_device, device.sub_devices) {
        about.push(format!("sub-device {} of {of}", sub + 1));
    }
    about.push(device.connections_in_words());
    let picture = match asked.keying {
        Keying::Off => "colour bars with a white bar moving across them".to_owned(),
        _ => format!(
            "colour bars across the top half, and below them a clear part with a white ramp from clear to solid and a soft-edged white bar moving across ({} alpha)",
            if asked.alpha == Alpha::Premultiplied { "premultiplied" } else { "straight" }
        ),
    };
    let keyer = match asked.keying {
        Keying::Off => "the keyer off".to_owned(),
        Keying::External => "the keyer sending fill and key (the key on the connector paired with this one)".to_owned(),
        Keying::Internal => "the keyer laying the picture over the card's input".to_owned(),
    };
    format!(
        "This sends {picture}\n  to {} ({}),\n  as {} in {}, with {keyer},\n  for {} seconds, then stops.",
        device.name,
        about.join(", "),
        format.name,
        pixel_format.in_words(),
        asked.seconds
    )
}

/// Asks for "yes" on the terminal; anything else (or nothing to read) is no.
fn confirmed() -> bool {
    print!("Whatever that connector feeds will show it. Type yes to send it: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("yes")
}

/// The test picture at `seconds` in: 75% colour bars with a white bar moving across them, a
/// little over a second from side to side, so a freeze or a stutter shows. `keyed`, the bottom
/// half is clear but for a white ramp from clear to solid and a soft-edged white bar moving
/// across, to check a keyer's soft edges and its clear parts. Tightly packed RGBA rows, colours
/// straight (not multiplied by the alpha), as the app's renderer leaves them.
pub fn test_picture(width: u32, height: u32, seconds: f32, keyed: bool) -> Vec<u8> {
    const BARS: [[u8; 3]; 7] =
        [[191, 191, 191], [191, 191, 0], [0, 191, 191], [0, 191, 0], [191, 0, 191], [191, 0, 0], [0, 0, 191]];
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; w * h * 4];
    // The moving bar's left edge, and its width: a twentieth of the picture.
    let bar = (w / 20).max(1);
    let travel = (w + bar) as f32;
    let left = ((seconds * 0.8).fract() * travel) as isize - bar as isize;
    for y in 0..h {
        let bottom = keyed && y >= h / 2;
        for x in 0..w {
            let at = (y * w + x) * 4;
            let in_bar = (x as isize) >= left && (x as isize) < left + bar as isize;
            let pixel = if bottom {
                if in_bar {
                    // Soft edges: solid in the middle, fading to clear over a quarter of its
                    // width each side.
                    let from_edge = ((x as isize - left).min(left + bar as isize - 1 - x as isize)).max(0) as f32;
                    let alpha = (from_edge / (bar as f32 / 4.0)).min(1.0);
                    [255, 255, 255, (alpha * 255.0).round() as u8]
                } else if y >= h * 3 / 4 {
                    // The ramp: clear at the left, solid at the right.
                    [255, 255, 255, (x * 255 / (w - 1).max(1)) as u8]
                } else {
                    [0, 0, 0, 0]
                }
            } else if in_bar {
                [255, 255, 255, 255]
            } else {
                let [r, g, b] = BARS[(x * BARS.len() / w).min(BARS.len() - 1)];
                [r, g, b, 255]
            };
            out[at..at + 4].copy_from_slice(&pixel);
        }
    }
    out
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
    if let Some(start) = device.start(false) {
        out.push_str(&format!("   A new output starts in {}.\n", start.in_words()));
    }
    out.push_str(&format!(
        "   Desktop Video Setup's default format, as the driver gives it: {}.\n",
        card_mode_in_words(device)
    ));
    out
}

/// The device's own default format exactly as the driver gave it, so that what Desktop Video
/// Setup is set to can be checked against it: "'Hp30'", "'iunk'", "nothing".
fn card_mode_in_words(device: &Device) -> String {
    let raw = match device.card_mode {
        None => return "nothing (it doesn't say)".to_owned(),
        Some(raw) => u32::try_from(raw).map_or_else(|_| raw.to_string(), crate::code_in_words),
    };
    if device.card_mode_3d { format!("{raw}, in 3D") } else { raw }
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
        assert!(text.contains("A new output starts in 1080p30, as Desktop Video Setup has it."), "{text}");
        assert!(text.contains("Desktop Video Setup's default format, as the driver gives it: 'Hp30'."), "{text}");
        assert!(text.contains(
            "A new output starts in 1080i59.94, the usual HD format: Desktop Video Setup has NTSC, which SoftPresenter doesn't offer."
        ));
        assert!(
            text.contains("A new output starts in 1080i59.94, the usual HD format: Desktop Video Setup sets none.")
        );
        assert!(text.contains("as the driver gives it: 'iunk'.") && text.contains("as the driver gives it: nothing"));
        assert!(!text.contains("486i"), "standard definition is not offered");
    }

    #[test]
    fn nothing_is_sent_without_a_device_named() {
        let words = |text: &str| text.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        assert!(asked(&words("--seconds 5")).unwrap_err().contains("Say which device"));
        assert!(asked(&[]).is_err());
        let play = asked(&words("DeckLink Duo (1) --format 720p50 --seconds 5 --key external --yes")).unwrap();
        assert_eq!(play.device, "DeckLink Duo (1)");
        assert_eq!(
            (play.format.as_deref(), play.seconds, play.keying, play.yes),
            (Some("720p50"), 5, Keying::External, true)
        );
        assert!(asked(&words("1 --seconds 0")).is_err());
        assert!(asked(&words("1 --key off")).is_err());
        assert!(asked(&words("1 --colour red")).unwrap_err().contains("There is no --colour"));
    }

    #[test]
    fn a_device_is_named_by_its_number_in_the_list_or_its_whole_name() {
        let found = Found { driver: Driver::Ready("pretend".to_owned()), devices: crate::pretend::devices() };
        assert_eq!(chosen(&found, "2").map(|device| device.name.as_str()), Some("Pretend DeckLink (2)"));
        assert_eq!(
            chosen(&found, "pretend decklink keyer").map(|device| device.name.as_str()),
            Some("Pretend DeckLink Keyer")
        );
        assert!(chosen(&found, "0").is_none() && chosen(&found, "9").is_none() && chosen(&found, "Pretend").is_none());
    }

    #[test]
    fn the_plan_says_what_goes_where_before_anything_is_sent() {
        let devices = crate::pretend::devices();
        let device = &devices[4];
        let format = device.mode("1080i50").unwrap();
        let mut asked = Asked {
            device: "5".to_owned(),
            format: None,
            seconds: 10,
            keying: Keying::Off,
            alpha: Alpha::Straight,
            yes: false,
        };
        let plain = plan(device, format, crate::PixelFormat::Bgra, &asked);
        assert!(plain.contains("colour bars with a white bar moving"), "{plain}");
        assert!(plain.contains("to Pretend DeckLink Keyer (Pretend DeckLink, sub-device 1 of 1, SDI)"), "{plain}");
        assert!(plain.contains("as 1080i50 in 8-bit BGRA, with the keyer off"), "{plain}");
        assert!(plain.contains("for 10 seconds, then stops"), "{plain}");
        asked.keying = Keying::External;
        let keyed = plan(device, format, crate::PixelFormat::Bgra, &asked);
        assert!(keyed.contains("fill and key") && keyed.contains("clear part"), "{keyed}");
    }

    #[test]
    fn the_test_picture_has_bars_a_moving_bar_and_when_keyed_a_clear_part_and_soft_edges() {
        let (w, h) = (200, 100);
        let at = |picture: &[u8], x: usize, y: usize| picture[(y * w + x) * 4..(y * w + x) * 4 + 4].to_vec();
        let still = test_picture(w as u32, h as u32, 0.6, false);
        assert_eq!(at(&still, 15, 50), vec![191, 191, 191, 255], "the first bar");
        assert_eq!(at(&still, 199, 50), vec![0, 0, 191, 255], "the last bar");
        let later = test_picture(w as u32, h as u32, 0.9, false);
        assert_ne!(still, later, "the bar moves");
        let white = |picture: &[u8]| (0..w).filter(|x| at(picture, *x, 10) == vec![255, 255, 255, 255]).count();
        assert_eq!(white(&still), 10, "the moving bar is a twentieth of the width");
        let keyed = test_picture(w as u32, h as u32, 0.0, true);
        assert_eq!(at(&keyed, 100, 60)[3], 0, "clear below the bars");
        assert_eq!(at(&keyed, 0, 90)[3], 0, "the ramp starts clear");
        assert_eq!(at(&keyed, 199, 90)[3], 255, "and ends solid");
        let moving = test_picture(w as u32, h as u32, 0.6, true);
        let alphas: Vec<u8> = (0..w).map(|x| at(&moving, x, 60)[3]).filter(|alpha| *alpha > 0).collect();
        assert!(alphas.iter().any(|alpha| *alpha < 255) && alphas.contains(&255), "soft edges: {alphas:?}");
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
