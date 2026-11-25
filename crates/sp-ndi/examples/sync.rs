//! Measures whether an NDI source's sound is in step with its pictures. Play a clip that beeps
//! on a white flash (black otherwise, silent otherwise) on the source, and this says, for each
//! flash, how long after it the beep came: by the timecodes NDI gave the two (which a receiver
//! that lines them up goes by), and by when each arrived (which one that plays them as they
//! come goes by). Negative: the sound is early. It also says what the sound is (rate,
//! channels, block size), how steadily it comes and whether its timecodes follow on.
//!
//!     cargo run -p sp-ndi --features sdk-examples --example sync -- "SoftPresenter" 60
//!
//! The name is words the source's name contains; the number is how many seconds to listen.
//! A clip to play, a flash and a beep every two seconds:
//!
//!     ffmpeg -f lavfi -i "color=black:s=640x360:r=30,drawbox=c=white:t=fill:enable='lt(mod(t,2),0.1)'" \
//!       -f lavfi -i "aevalsrc='if(lt(mod(t,2),0.1),0.05*sin(2*PI*1000*t),0)':s=48000" -t 600 flash.mp4

#[cfg(feature = "sdk-examples")]
fn main() {
    use std::time::{Duration, Instant};

    use grafton_ndi::{Finder, FinderOptions, NDI, Receiver, ReceiverColorFormat, ReceiverOptions};

    let args: Vec<String> = std::env::args().collect();
    let wanted = args.get(1).cloned().unwrap_or_else(|| "SoftPresenter".to_owned());
    let stay = args.get(2).and_then(|seconds| seconds.parse::<f32>().ok()).unwrap_or(60.0);
    let ndi = match NDI::new() {
        Ok(ndi) => ndi,
        Err(e) => return eprintln!("The NDI runtime could not be started: {e}"),
    };
    let options = FinderOptions::builder().show_local_sources(true).build();
    let finder = match Finder::new(&ndi, &options) {
        Ok(finder) => finder,
        Err(e) => return eprintln!("Could not look for sources: {e}"),
    };
    let mut found = None;
    for _ in 0..16 {
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
    let began = Instant::now();
    let stay = Duration::from_secs_f32(stay);
    // Seconds, from a timecode in NDI's 100 ns steps.
    let seconds = |timecode: i64| timecode as f64 / 1e7;
    // Each flash and each beep: its timecode and when it arrived, both in seconds.
    let (flashes, beeps) = std::thread::scope(|scope| {
        let pictures = scope.spawn(|| {
            let (mut flashes, mut lit, mut count) = (Vec::new(), false, 0u32);
            while began.elapsed() < stay {
                let Ok(frame) = receiver.video().capture(Duration::from_millis(100)) else { continue };
                let arrived = began.elapsed().as_secs_f64();
                count += 1;
                let pixels = frame.data().as_chunks::<4>().0;
                let bright = pixels.iter().step_by(97).filter(|p| p[0] > 180 && p[1] > 180 && p[2] > 180).count();
                let now_lit = bright * 97 * 4 > pixels.len();
                if now_lit && !lit {
                    flashes.push((seconds(frame.timecode()), arrived));
                }
                lit = now_lit;
            }
            println!("{count} pictures");
            flashes
        });
        let sound = scope.spawn(|| {
            let mut beeps = Vec::new();
            // How long it has been silent, in samples; the last block's timecode and length;
            // the largest step between blocks' timecodes away from their length; arrival gaps.
            let (mut silent, mut blocks, mut samples) = (48_000usize, 0u32, 0u64);
            let mut before: Option<(i64, f64, Instant)> = None;
            let (mut worst_step, mut longest_gap, mut breaks) = (0.0f64, Duration::ZERO, 0u32);
            let mut said = false;
            while began.elapsed() < stay {
                let Ok(frame) = receiver.audio().capture(Duration::from_millis(100)) else { continue };
                let (arrived, now) = (began.elapsed().as_secs_f64(), Instant::now());
                let (rate, channels, each) =
                    (frame.sample_rate() as f64, frame.num_channels() as usize, frame.num_samples() as usize);
                if !said {
                    said = true;
                    println!("sound: {rate} Hz, {channels} channels, {each} samples a block");
                }
                blocks += 1;
                samples += each as u64;
                if let Some((timecode, length, at)) = before {
                    let step = seconds(frame.timecode() - timecode) - length;
                    if step.abs() > 0.05 {
                        breaks += 1;
                    } else {
                        worst_step = worst_step.max(step.abs());
                    }
                    longest_gap = longest_gap.max(now - at);
                }
                before = Some((frame.timecode(), each as f64 / rate, now));
                let data = frame.data();
                for at in 0..each {
                    let loud = (0..channels).any(|channel| data[channel * each + at].abs() > 0.004);
                    if loud {
                        // A beep begins after a twentieth of a second's silence.
                        if silent > rate as usize / 20 {
                            beeps.push((seconds(frame.timecode()) + at as f64 / rate, arrived));
                        }
                        silent = 0;
                    } else {
                        silent += 1;
                    }
                }
            }
            println!(
                "{blocks} blocks of sound, {:.1} s of it; timecodes follow on to within {:.2} ms ({breaks} breaks); \
                 longest wait between blocks {} ms",
                samples as f64 / 48_000.0,
                worst_step * 1000.0,
                longest_gap.as_millis()
            );
            beeps
        });
        (pictures.join().unwrap_or_default(), sound.join().unwrap_or_default())
    });
    println!("{} flashes, {} beeps", flashes.len(), beeps.len());
    let mut by_timecode = Vec::new();
    for (flash, flash_arrived) in &flashes {
        let nearest = beeps.iter().min_by(|a, b| (a.0 - flash).abs().total_cmp(&(b.0 - flash).abs()));
        let Some((beep, beep_arrived)) = nearest.filter(|(beep, _)| (beep - flash).abs() < 0.5) else {
            println!("flash at {flash_arrived:7.2} s: no beep near it");
            continue;
        };
        let (timecode, arrival) = ((beep - flash) * 1000.0, (beep_arrived - flash_arrived) * 1000.0);
        // Where each falls in its own two seconds, by timecode: for a clip that flashes and
        // beeps every two seconds, whichever of the two moves is the one that wandered.
        let (flash_in, beep_in) = (flash.rem_euclid(2.0) * 1000.0, beep.rem_euclid(2.0) * 1000.0);
        println!(
            "flash at {flash_arrived:7.2} s: beep {timecode:+7.1} ms after it by timecode, {arrival:+7.1} ms by arrival \
             (flash at {flash_in:6.1} ms of its two seconds, beep at {beep_in:6.1})"
        );
        by_timecode.push(timecode);
    }
    // Beeps with no flash: sound where there was no picture for it (said twice, say).
    for (beep, beep_arrived) in &beeps {
        if !flashes.iter().any(|(flash, _)| (beep - flash).abs() < 0.5) {
            let after =
                flashes.iter().map(|(flash, _)| beep - flash).filter(|after| *after > 0.0).fold(f64::MAX, f64::min);
            println!("beep at {beep_arrived:7.2} s: no flash near it ({:.0} ms after the last flash)", after * 1000.0);
        }
    }
    if !by_timecode.is_empty() {
        let mean = by_timecode.iter().sum::<f64>() / by_timecode.len() as f64;
        let (least, most) =
            by_timecode.iter().fold((f64::MAX, f64::MIN), |(least, most), ms| (least.min(*ms), most.max(*ms)));
        let third = by_timecode.len().div_ceil(3);
        let mean_of = |part: &[f64]| part.iter().sum::<f64>() / part.len().max(1) as f64;
        println!(
            "beep after flash, by timecode: {mean:+.1} ms on average (from {least:+.1} to {most:+.1}); \
             the first third {:+.1} ms, the last third {:+.1} ms",
            mean_of(&by_timecode[..third]),
            mean_of(&by_timecode[by_timecode.len() - third..])
        );
    }
}

#[cfg(not(feature = "sdk-examples"))]
fn main() {
    eprintln!("Build with --features sdk-examples to receive NDI.");
}
