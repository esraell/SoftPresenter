//! Receives pictures from the NDI source whose name contains the given words and says what
//! they hold. Used to check that SoftPresenter's NDI picture really arrives:
//! `cargo run -p sp-ndi --features sdk-examples --example receive -- "SoftPresenter"`.
//!
//! With a number of seconds after the name it stays connected that long, and says when each
//! picture that differs from the one before it came, counted from when it connected and by the
//! clock (seconds into the UTC day, to set beside when something was done), with a fingerprint
//! that tells one picture from another:
//! `cargo run -p sp-ndi --features sdk-examples --example receive -- "SoftPresenter" 10`.
//!
//! Each picture is also described by its alpha, which a see-through screen sends (a solid one
//! arrives as RGBX, all solid): how many pixels are clear (0), partly see-through and solid
//! (255), and the least and most. With `--alpha-at X,Y` (in the first picture's pixels, which
//! may be given more than once), the colour and alpha found at each place are printed too.

#[cfg(feature = "sdk-examples")]
fn main() {
    use std::time::{Duration, Instant};

    use grafton_ndi::{Finder, FinderOptions, NDI, Receiver, ReceiverColorFormat, ReceiverOptions};

    // Places to print the colour and alpha of, from --alpha-at X,Y (taken out of the arguments).
    let mut args: Vec<String> = std::env::args().collect();
    let mut places: Vec<(usize, usize)> = Vec::new();
    while let Some(at) = args.iter().position(|arg| arg == "--alpha-at") {
        let place = args
            .get(at + 1)
            .and_then(|xy| xy.split_once(','))
            .and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)));
        places.extend(place);
        args.drain(at..(at + 2).min(args.len()));
    }
    let wanted = args.get(1).cloned().unwrap_or_default();
    let ndi = match NDI::new() {
        Ok(ndi) => ndi,
        Err(e) => return eprintln!("The NDI runtime could not be started: {e}"),
    };
    let options = FinderOptions::builder().show_local_sources(true).build();
    let Ok(finder) = Finder::new(&ndi, &options) else { return eprintln!("Could not look for sources.") };
    let mut found = None;
    for _ in 0..10 {
        let _ = finder.wait_for_sources(Duration::from_millis(500));
        if let Ok(sources) = finder.current_sources() {
            found = sources.into_iter().find(|source| source.name.contains(&wanted));
        }
        if found.is_some() {
            break;
        }
    }
    let Some(source) = found else { return println!("no source with \"{wanted}\" in its name") };
    println!("source: {}", source.name);
    let options = ReceiverOptions::builder(source).color(ReceiverColorFormat::RGBX_RGBA).build();
    let receiver = match Receiver::new(&ndi, &options) {
        Ok(receiver) => receiver,
        Err(e) => return eprintln!("Could not connect: {e}"),
    };
    let connected = Instant::now();
    // What a picture holds, and a fingerprint of its pixels (FNV-1a) to tell pictures apart.
    let describe = |data: &[u8]| {
        let pixels = data.as_chunks::<4>().0;
        let bright = pixels.iter().filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count();
        let lit = pixels.iter().filter(|p| p[0] > 40 || p[1] > 40 || p[2] > 40).count();
        let print = data
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3));
        // How see-through the pixels are.
        let clear = pixels.iter().filter(|p| p[3] == 0).count();
        let solid = pixels.iter().filter(|p| p[3] == 255).count();
        let least = pixels.iter().map(|p| p[3]).min().unwrap_or(0);
        let most = pixels.iter().map(|p| p[3]).max().unwrap_or(0);
        let between = pixels.len() - clear - solid;
        // The pixels near half solid (soft edges), their colour on average: straight colours
        // keep their strength there, premultiplied ones are darkened by about half.
        let soft: Vec<&[u8; 4]> = pixels.iter().filter(|p| (96..=160).contains(&p[3])).collect();
        let mean = |at: usize| soft.iter().map(|p| f64::from(p[at])).sum::<f64>() / soft.len().max(1) as f64;
        let edge = if soft.is_empty() {
            String::new()
        } else {
            format!(
                "; soft edges ({} pixels): on average red {:.0} green {:.0} blue {:.0} at alpha {:.0}",
                soft.len(),
                mean(0),
                mean(1),
                mean(2),
                mean(3)
            )
        };
        // The red channel the same way: for a key (solid greys), how many are black, grey, white.
        let (no_red, full_red) =
            (pixels.iter().filter(|p| p[0] == 0).count(), pixels.iter().filter(|p| p[0] == 255).count());
        (
            format!(
                "pixels: {} total, {lit} not black, {bright} white; alpha: {clear} clear, {between} between, \
                 {solid} solid, from {least} to {most}; red: {no_red} at 0, {} between, {full_red} at 255{edge}",
                pixels.len(),
                pixels.len() - no_red - full_red
            ),
            print,
        )
    };
    // The colour and alpha at each place asked for, in a picture `width` pixels across.
    let at_places = |data: &[u8], width: usize| {
        for (x, y) in &places {
            let at = (y * width + x) * 4;
            if let Some(p) = data.get(at..at + 4) {
                println!("  at {x},{y}: red {} green {} blue {} alpha {}", p[0], p[1], p[2], p[3]);
            }
        }
    };
    let Some(stay) = args.get(2).and_then(|seconds| seconds.parse::<f32>().ok()) else {
        return match receiver.video().capture(Duration::from_secs(8)) {
            Ok(frame) => {
                println!("picture: {}x{} {:?}", frame.width(), frame.height(), frame.pixel_format());
                println!("{}", describe(frame.data()).0);
                at_places(frame.data(), frame.width() as usize);
            }
            Err(e) => println!("no picture arrived: {e}"),
        };
    };
    let (mut pictures, mut last) = (0_u32, None);
    while connected.elapsed().as_secs_f32() < stay {
        let Ok(frame) = receiver.video().capture(Duration::from_millis(200)) else { continue };
        pictures += 1;
        let (holds, print) = describe(frame.data());
        if last != Some(print) {
            last = Some(print);
            let after = connected.elapsed().as_secs_f32();
            let clock = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |since| since.as_secs_f64() % 86_400.0);
            println!(
                "{after:.2} s (clock {clock:.2}): picture {pictures}, {}x{} {:?}, fingerprint {print:016x}; {holds}",
                frame.width(),
                frame.height(),
                frame.pixel_format()
            );
            at_places(frame.data(), frame.width() as usize);
        }
    }
    println!("{pictures} pictures in {stay} s");
}

#[cfg(not(feature = "sdk-examples"))]
fn main() {
    eprintln!("Build with --features sdk-examples to receive NDI.");
}
