//! Opens each file given: a picture is decoded; a video gives a still, then plays for two
//! seconds while another thread counts its pictures as the screens take them (lent, not
//! copied) and looks at every pixel's fourth byte, which says whether the player's pictures
//! come opaque. For checking by hand:
//!
//!     cargo run -p sp-video --example check -- clip.mp4 photo.heic
//!
//! With `--handover FIRST NEXT`, plays FIRST looping, lines NEXT up to follow it after 0.3 s, and
//! says what the screens' pictures did around the hand-over: each picture's middle colour, the
//! time between the last of FIRST and the first of NEXT, and whether any was black. With MP4
//! files on a Mac this checks the system's player (the unit test checks FFmpeg's). Videos whose
//! pictures are red (FIRST) and blue (NEXT), brighter picture by picture, show the most:
//!
//!     ffmpeg -f lavfi -i "color=c=black:s=320x180:r=30:d=2,format=rgb24,geq=r='40+N*3':g=0:b=0" red.mp4
//!     ffmpeg -f lavfi -i "color=c=black:s=320x180:r=30:d=2,format=rgb24,geq=r=0:g=0:b='40+N*3'" blue.mp4
//!     cargo run -p sp-video --example check -- --handover red.mp4 blue.mp4

use std::path::Path;
use std::time::{Duration, Instant};

/// How long each video plays: CHECK_SECONDS, or 2.5 s.
static RUN_MS: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| {
    std::env::var("CHECK_SECONDS").ok().and_then(|s| s.parse::<f64>().ok()).map_or(2500, |s| (s * 1000.0) as u64)
});

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag, first, next] = args.as_slice()
        && flag == "--handover"
    {
        return handover(Path::new(first), Path::new(next));
    }
    for arg in std::env::args().skip(1) {
        let path = Path::new(&arg);
        let video = ["mp4", "mov", "m4v", "mkv", "webm", "avi"].iter().any(|ext| arg.to_lowercase().ends_with(ext));
        if !video {
            let started = Instant::now();
            match sp_video::picture(path, 2560) {
                Ok(frame) => {
                    println!("{arg}: picture {frame:?} in {:?}, first pixel {:?}", started.elapsed(), &frame.rgba[..4])
                }
                Err(e) => println!("{arg}: {e}"),
            }
            continue;
        }
        match sp_video::still(path, 640) {
            Ok(frame) => println!("{arg}: still {frame:?}"),
            Err(e) => println!("{arg}: {e}"),
        }
        let mut player = match sp_video::Player::open(path) {
            Ok(player) => player,
            Err(e) => {
                println!("{arg}: {e}");
                continue;
            }
        };
        let frames = player.frames();
        let counter = std::thread::spawn(move || {
            let (mut count, mut seen, started) = (0, None, Instant::now());
            // The lowest and highest fourth byte of any pixel of any picture.
            let mut fourth = (u8::MAX, u8::MIN);
            while started.elapsed() < Duration::from_millis(*RUN_MS) {
                let lent = frames.next_with(|lent| {
                    let row = lent.width as usize * 4;
                    for line in lent.bytes.chunks(lent.bytes_per_row).take(lent.height as usize) {
                        for pixel in line[..row].as_chunks::<4>().0 {
                            fourth = (fourth.0.min(pixel[3]), fourth.1.max(pixel[3]));
                        }
                    }
                    (lent.width, lent.height, lent.bytes_per_row, lent.order, lent.opaque)
                });
                if let Some(lent) = lent {
                    count += 1;
                    seen = Some(lent);
                }
                std::thread::sleep(Duration::from_millis(4));
            }
            (count, seen, fourth)
        });
        player.set_looping(true);
        player.set_volume(0.0);
        player.play();
        let started = Instant::now();
        let mut said = 0;
        while started.elapsed() < Duration::from_millis(*RUN_MS) {
            let wait = player.tick();
            if started.elapsed().as_secs() > said {
                said = started.elapsed().as_secs();
                println!("  {said} s: at {:.2}, playing {}", player.position(), player.playing());
            }
            // The system finishes opening and playing the video through the main thread's
            // run loop, which the app's event loop keeps turning; here it is turned by hand.
            spin(wait.min(Duration::from_millis(50)));
        }
        let (count, seen, fourth) = counter.join().unwrap_or((0, None, (0, 0)));
        let lent = seen.map_or_else(
            || "nothing".to_owned(),
            |(w, h, bytes_per_row, order, opaque)| {
                let kind = if opaque { "shown opaque" } else { "shown with its alpha" };
                format!("{w}x{h}, {bytes_per_row} bytes a row, {order:?}, {kind}")
            },
        );
        println!(
            "{arg}: {} {count} pictures of {lent}, fourth bytes {} to {}, at {:.2} s of {:?} s, playing {}, problem {:?}",
            if player.uses_ffmpeg() { "FFmpeg" } else { "system" },
            fourth.0,
            fourth.1,
            player.position(),
            player.duration(),
            player.playing(),
            player.problem()
        );
    }
}

/// Plays `first` looping, lines `next` up after 0.3 s, and says what the screens' pictures did
/// around the hand-over.
fn handover(first: &Path, next: &Path) {
    let mut player = match sp_video::Player::open(first) {
        Ok(player) => player,
        Err(e) => return println!("{}: {e}", first.display()),
    };
    let engine = if player.uses_ffmpeg() { "FFmpeg" } else { "the system's player" };
    player.set_looping(true);
    player.set_volume(0.0);
    let frames = player.frames();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = {
        let (frames, stop) = (frames.clone(), stop.clone());
        std::thread::spawn(move || {
            let (started, mut seen) = (Instant::now(), Vec::new());
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let middle = frames.next_with(|lent| {
                    let at = (lent.height as usize / 2) * lent.bytes_per_row + (lent.width as usize / 2) * 4;
                    let pixel = &lent.bytes[at..at + 4];
                    match lent.order {
                        sp_video::PixelOrder::Rgba => [pixel[0], pixel[1], pixel[2]],
                        sp_video::PixelOrder::Bgra => [pixel[2], pixel[1], pixel[0]],
                    }
                });
                if let Some(middle) = middle {
                    seen.push((started.elapsed(), middle));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            seen
        })
    };
    player.play();
    let started = Instant::now();
    let length = player.duration();
    let (mut lined_up, mut took_over) = (false, None);
    let until = Duration::from_secs_f64(length.unwrap_or(2.0) * 1.0 + 2.5);
    while started.elapsed() < until {
        if !lined_up && started.elapsed() > Duration::from_millis(300) {
            if let Err(e) = player.play_next(next) {
                return println!("{}: {e}", next.display());
            }
            lined_up = true;
        }
        let wait = player.tick();
        if let Some(handed) = player.take_handed_over() {
            took_over = Some((started.elapsed(), handed));
        }
        spin(wait.min(Duration::from_millis(5)));
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let seen = reader.join().unwrap_or_default();
    let black = seen.iter().filter(|(_, [r, g, b])| (*r).max(*g).max(*b) < 30).count();
    let first_blue = seen.iter().position(|(_, [r, _, b])| b > r);
    println!("{} with {engine}: {} pictures, {black} black; hand-over {took_over:?}", first.display(), seen.len());
    if let Some(at) = first_blue.filter(|at| *at > 0) {
        let from = at.saturating_sub(4);
        for (when, colour) in &seen[from..(at + 4).min(seen.len())] {
            println!("  {when:?}  {colour:?}");
        }
        let gap = seen[at].0 - seen[at - 1].0;
        let red_after = seen[at..].iter().any(|(_, [r, _, b])| r > b);
        println!(
            "last picture of the first {:?}, first of the next {:?}, {gap:?} between them; first's pictures after: {red_after}",
            seen[at - 1].1,
            seen[at].1
        );
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn spin(time: Duration) {
    use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
    // SAFETY: reads a static the framework defines.
    let mode = unsafe { kCFRunLoopDefaultMode };
    CFRunLoop::run_in_mode(mode, time.as_secs_f64(), false);
}

#[cfg(not(target_os = "macos"))]
fn spin(time: Duration) {
    std::thread::sleep(time);
}
