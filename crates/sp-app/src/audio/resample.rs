//! Bringing a track's sound to stereo at the sound card's rate, once, on the thread that
//! decodes it: never on the sound card's own thread, and never twice.
//!
//! Channels: one goes to both sides, two stay as they are, and more are folded into two the
//! usual way (the centre and the surrounds at -3 dB into each side, the low-frequency channel
//! left out), made no louder than the loudest side could be, as FFmpeg folds them for the
//! tracks it decodes. The channels are taken in the order WAV, FLAC and the decoders give them:
//! front left, front right, centre, low frequency, back left, back right, side left, side right.
//!
//! Rate: a windowed-sinc filter (a Kaiser window), the kind good resamplers use, rather than
//! the straight line between two samples (and, going down, the samples simply left out) that
//! the sound output would otherwise use: going up it adds no images of the music above it, and
//! going down it takes out what the lower rate can't hold instead of folding it back as noise.

use std::f64::consts::PI;

/// A stereo frame.
pub type Frame = [f32; 2];

/// Zero crossings of the filter on each side of its centre, going up (more going down, as many
/// in the lower rate's time). 32 on each side keeps whatever is above it 90 dB down.
const CROSSINGS: usize = 32;

/// The places between two samples the filter is worked out for, when the two rates have more
/// places than [`EXACT`] (between two of them it is drawn as a straight line). The usual rates
/// have far fewer: 44.1 kHz to 48 kHz has 160, 96 or 192 kHz to 48 kHz only one, and each is
/// worked out exactly.
const PHASES: usize = 256;
const EXACT: u64 = 1024;

/// The Kaiser window's shape: about 90 dB between what is kept and what is taken out.
const BETA: f64 = 9.0;

/// How much of the lower rate's half the music keeps: up to 95% of it (20.9 kHz of 44.1 kHz's
/// 22.05), with the filter falling to nothing over the rest.
const PASS: f64 = 0.95;

/// Folds one frame of `channels` channels into stereo (see the module's notes).
pub fn fold(frame: &[f32]) -> Frame {
    const HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let at = |n: usize| frame.get(n).copied().unwrap_or(0.0);
    match frame.len() {
        0 => [0.0, 0.0],
        1 => [at(0), at(0)],
        2 => [at(0), at(1)],
        // Left, right, centre.
        3 => {
            let scale = 1.0 / (1.0 + HALF);
            [(at(0) + HALF * at(2)) * scale, (at(1) + HALF * at(2)) * scale]
        }
        // Left, right, back left, back right.
        4 => {
            let scale = 1.0 / (1.0 + HALF);
            [(at(0) + HALF * at(2)) * scale, (at(1) + HALF * at(3)) * scale]
        }
        // Left, right, centre, back left, back right.
        5 => {
            let scale = 1.0 / (1.0 + 2.0 * HALF);
            [(at(0) + HALF * (at(2) + at(3))) * scale, (at(1) + HALF * (at(2) + at(4))) * scale]
        }
        // 5.1: left, right, centre, low frequency, back left, back right.
        6 => {
            let scale = 1.0 / (1.0 + 2.0 * HALF);
            [(at(0) + HALF * (at(2) + at(4))) * scale, (at(1) + HALF * (at(2) + at(5))) * scale]
        }
        // 6.1: left, right, centre, low frequency, back centre, side left, side right.
        7 => {
            let scale = 1.0 / (1.0 + 3.0 * HALF);
            [(at(0) + HALF * (at(2) + at(4) + at(5))) * scale, (at(1) + HALF * (at(2) + at(4) + at(6))) * scale]
        }
        // 7.1 and more: left, right, centre, low frequency, back left, back right, side left,
        // side right; any further channels are left out.
        _ => {
            let scale = 1.0 / (1.0 + 3.0 * HALF);
            [(at(0) + HALF * (at(2) + at(4) + at(6))) * scale, (at(1) + HALF * (at(2) + at(5) + at(7))) * scale]
        }
    }
}

/// Changes the rate of stereo sound from `from` to `to` samples a second.
pub struct Resampler {
    from: u32,
    to: u32,
    /// Taps on each side of the centre, in samples of the rate coming in.
    half: usize,
    /// The filter at each place between two samples: `2 * half` taps each. With `exact`, one
    /// for each place a frame out can fall (`to / gcd` of them); otherwise [`PHASES`] + 1.
    table: Vec<f32>,
    exact: bool,
    /// The greatest common divisor of the two rates: a frame out's place, in `to`ths, is always
    /// a whole number of these.
    common: u64,
    /// The frames coming in that are still needed: the `half - 1` before the next frame out's
    /// place, and those after it.
    input: Vec<Frame>,
    /// Where the next frame out lies: a whole frame of `input`, and the part of the way to the
    /// next one, in `to`ths (counted exactly, so the place never drifts).
    whole: usize,
    part: u64,
    /// The taps for one frame out, worked out once for both channels.
    taps: Vec<f32>,
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// The zeroth modified Bessel function of the first kind, which the Kaiser window is made of.
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let quarter = x * x / 4.0;
    for k in 1..64 {
        term *= quarter / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Resampler {
        let (from, to) = (from.max(1), to.max(1));
        // The filter's width is set by the lower of the two rates: going down, it spans as many
        // of the higher rate's samples as the crossings take at the lower one.
        let ratio = (f64::from(to) / f64::from(from)).min(1.0);
        let half = ((CROSSINGS as f64 / ratio).ceil() as usize).max(1);
        let cutoff = 0.5 * ratio * PASS; // in cycles a sample coming in
        let span = half as f64;
        let window_scale = bessel_i0(BETA);
        let taps = 2 * half;
        let common = gcd(u64::from(from), u64::from(to));
        let places = u64::from(to) / common;
        let exact = places <= EXACT;
        let rows = if exact { places as usize } else { PHASES + 1 };
        let mut table = vec![0.0f32; rows * taps];
        for phase in 0..rows {
            let part = if exact { phase as f64 / places as f64 } else { phase as f64 / PHASES as f64 };
            for k in 0..taps {
                // The distance from the frame out to tap k's frame coming in.
                let t = part + (half as f64 - 1.0) - k as f64;
                let x = 2.0 * cutoff * t;
                let sinc = if x.abs() < 1e-12 { 1.0 } else { (PI * x).sin() / (PI * x) };
                let along = t / span;
                let window = if along.abs() >= 1.0 {
                    0.0
                } else {
                    bessel_i0(BETA * (1.0 - along * along).sqrt()) / window_scale
                };
                table[phase * taps + k] = (2.0 * cutoff * sinc * window) as f32;
            }
        }
        // Each place's taps add up to exactly one, so a steady level stays as it is.
        for row in table.chunks_mut(taps) {
            let sum: f32 = row.iter().sum();
            if sum.abs() > 1e-6 {
                row.iter_mut().for_each(|tap| *tap /= sum);
            }
        }
        Resampler {
            from,
            to,
            half,
            table,
            exact,
            common,
            // The frames before the first are silence, so the first frame out is the first in.
            input: vec![[0.0; 2]; half - 1],
            whole: half - 1,
            part: 0,
            taps: vec![0.0; taps],
        }
    }

    /// True when the rates are the same, and the frames go through as they are.
    pub fn passes(&self) -> bool {
        self.from == self.to
    }

    /// Takes frames coming in and puts the frames they make at the new rate on the end of `out`.
    pub fn push(&mut self, frames: &[Frame], out: &mut Vec<Frame>) {
        if self.passes() {
            out.extend_from_slice(frames);
            return;
        }
        self.input.extend_from_slice(frames);
        let taps = 2 * self.half;
        while self.whole + self.half < self.input.len() {
            // The taps for this place: worked out exactly, or between the two nearest worked out,
            // on a straight line.
            let filter: &[f32] = if self.exact {
                let row = (self.part / self.common) as usize;
                &self.table[row * taps..][..taps]
            } else {
                let place = self.part as f64 / f64::from(self.to) * PHASES as f64;
                let row = (place as usize).min(PHASES - 1);
                let along = (place - row as f64) as f32;
                let (low, high) = (&self.table[row * taps..][..taps], &self.table[(row + 1) * taps..][..taps]);
                for ((tap, low), high) in self.taps.iter_mut().zip(low).zip(high) {
                    *tap = low + (high - low) * along;
                }
                &self.taps
            };
            let first = self.whole + 1 - self.half;
            let (mut left, mut right) = (0.0f32, 0.0f32);
            for (tap, frame) in filter.iter().zip(&self.input[first..first + taps]) {
                left += tap * frame[0];
                right += tap * frame[1];
            }
            out.push([left, right]);
            self.part += u64::from(self.from);
            while self.part >= u64::from(self.to) {
                self.part -= u64::from(self.to);
                self.whole += 1;
            }
        }
        // Let go of the frames no longer needed.
        let done = self.whole + 1 - self.half;
        if done > 0 {
            self.input.drain(..done);
            self.whole -= done;
        }
    }

    /// The end of the sound: the last frames still held are let out.
    pub fn finish(&mut self, out: &mut Vec<Frame>) {
        if !self.passes() {
            let tail = vec![[0.0; 2]; self.half];
            self.push(&tail, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f64, seconds: f64, level: f32) -> Vec<Frame> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count)
            .map(|n| {
                let value = (2.0 * PI * hz * n as f64 / f64::from(rate)).sin() as f32 * level;
                [value, -value]
            })
            .collect()
    }

    /// Resamples in uneven pieces, as a decoder hands them over.
    fn resample(from: u32, to: u32, frames: &[Frame]) -> Vec<Frame> {
        let mut resampler = Resampler::new(from, to);
        let mut out = Vec::new();
        let mut at = 0;
        for size in [1, 7, 300, 1024, 4096].iter().cycle() {
            if at >= frames.len() {
                break;
            }
            let end = (at + size).min(frames.len());
            resampler.push(&frames[at..end], &mut out);
            at = end;
        }
        resampler.finish(&mut out);
        out
    }

    /// The loudest difference from a perfect tone, away from the ends.
    fn worst_error(out: &[Frame], rate: u32, hz: f64, level: f32) -> f32 {
        let skip = rate as usize / 10;
        out[skip..out.len() - skip]
            .iter()
            .enumerate()
            .map(|(n, frame)| {
                let exact = (2.0 * PI * hz * (n + skip) as f64 / f64::from(rate)).sin() as f32 * level;
                (frame[0] - exact).abs().max((frame[1] + exact).abs())
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn a_tone_comes_out_at_the_new_rate_in_time_at_the_same_pitch_and_level() {
        for (from, to) in [(44_100, 48_000), (22_050, 48_000), (96_000, 48_000), (192_000, 48_000), (48_000, 44_100)] {
            let input = tone(from, 1000.0, 1.0, 0.5);
            let out = resample(from, to, &input);
            // A second is a second: as many frames as the new rate has, give or take one.
            let wanted = to as usize;
            assert!(out.len().abs_diff(wanted) <= 1, "{from} to {to}: {} frames", out.len());
            // Every frame within -80 dB of the perfect tone at its place: no delay, no change
            // of pitch or level, and nothing added.
            let error = worst_error(&out, to, 1000.0, 0.5);
            assert!(error < 1e-4, "{from} to {to}: off by {error}");
        }
        // Two rates with more places than are worked out exactly (drawn between them): -70 dB.
        let out = resample(44_101, 48_000, &tone(44_101, 1000.0, 1.0, 0.5));
        let error = worst_error(&out, 48_000, 1000.0, 0.5);
        assert!(error < 2e-4, "44,101 to 48,000: off by {error}");
        // High in the band, near the top of what 44.1 kHz holds: still within -60 dB.
        let out = resample(44_100, 48_000, &tone(44_100, 18_000.0, 1.0, 0.5));
        let error = worst_error(&out, 48_000, 18_000.0, 0.5);
        assert!(error < 1e-3, "18 kHz off by {error}");
    }

    #[test]
    fn going_down_takes_out_what_the_lower_rate_cant_hold() {
        // 30 kHz at 96 kHz would fold back to 18 kHz at 48 kHz: it is taken out instead, to
        // below -80 dB.
        let out = resample(96_000, 48_000, &tone(96_000, 30_000.0, 0.5, 0.5));
        let loudest = out[1000..out.len() - 1000].iter().map(|frame| frame[0].abs()).fold(0.0, f32::max);
        assert!(loudest < 5e-5, "{loudest}");
        // And what it can hold goes through as it was.
        let out = resample(192_000, 48_000, &tone(192_000, 15_000.0, 0.5, 0.5));
        assert!(worst_error(&out, 48_000, 15_000.0, 0.5) < 1e-3);
    }

    #[test]
    fn the_same_rate_goes_through_untouched_and_a_steady_level_stays_steady() {
        let input = tone(48_000, 440.0, 0.1, 0.3);
        assert_eq!(resample(48_000, 48_000, &input), input);
        let out = resample(44_100, 48_000, &vec![[0.25, -0.25]; 44_100]);
        for frame in &out[200..out.len() - 200] {
            assert!((frame[0] - 0.25).abs() < 1e-4 && (frame[1] + 0.25).abs() < 1e-4, "{frame:?}");
        }
    }

    #[test]
    fn channels_are_folded_into_stereo() {
        assert_eq!(fold(&[0.5]), [0.5, 0.5]);
        assert_eq!(fold(&[0.1, 0.2]), [0.1, 0.2]);
        // 5.1 with only the centre sounding: equally in both sides, no louder than the centre.
        let [left, right] = fold(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        assert!((left - right).abs() < 1e-6 && left > 0.2 && left < 0.5, "{left}");
        // The low-frequency channel is left out, and no side ever goes past full scale.
        assert_eq!(fold(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]), [0.0, 0.0]);
        for channels in 3..=8 {
            let [left, right] = fold(&vec![1.0; channels]);
            assert!(left <= 1.0 + 1e-6 && right <= 1.0 + 1e-6, "{channels}: {left} {right}");
        }
        // The left front goes to the left only.
        let [left, right] = fold(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(left > 0.0 && right == 0.0);
    }
}
