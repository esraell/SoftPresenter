//! The one source the audio player plays through the sound output, for as long as the app
//! runs: on the sound card's own thread it takes each track's frames, already decoded and at
//! the card's rate ([`super::stream`]), and mixes them, with every join worked out to the frame:
//! a track that follows another starts on the very frame after the last one's last (with no
//! crossfade), or fades in as the other fades out over the crossfade's seconds; a track chosen,
//! a jump in the track, a stop and a pause all fade over a hundredth of a second, so nothing
//! clicks; and the volume moves smoothly from one block to the next.
//!
//! It never waits: orders come through a channel it only looks into, the tracks' frames through
//! their rings, and the tracks it has finished with go back the same way, so it never lets go
//! of memory either.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::time::Duration;

use super::resample::Frame;
use super::stream::Shared;

/// Frames made at a time: about 5 ms at 48 kHz.
pub const BLOCK: usize = 256;

/// How long a fade that only keeps a change from clicking lasts: 10 ms.
const DECLICK: Duration = Duration::from_millis(10);

/// What the window asks of the deck.
pub enum Order {
    /// Play this track from now: what plays fades out (over `fade`, or a moment) once this one
    /// has its first frames, and this one fades in over the same time.
    Play { stream: Arc<Shared>, fade: Duration },
    /// Play this one after the one playing: fading in over the last `fade` of it, or (with no
    /// fade) from the frame after its last.
    Then { stream: Arc<Shared>, fade: Duration },
    /// Forget the one lined up to follow.
    NoThen,
    /// Stop, fading out over a moment.
    Stop,
}

/// What the window and the deck both see: the volume and pause asked for, and what the deck
/// counts.
#[derive(Debug)]
pub struct Controls {
    volume: AtomicU32,
    paused: AtomicBool,
    /// Blocks in which the track playing had fewer frames ready than it needed (its decoding
    /// fell behind), and blocks played with something playing.
    pub underruns: AtomicU64,
    pub blocks: AtomicU64,
    /// The fewest frames the playing track had ready at the start of a block, since last asked
    /// (`u64::MAX` for none).
    pub lowest: AtomicU64,
}

impl Default for Controls {
    fn default() -> Controls {
        Controls {
            volume: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
            blocks: AtomicU64::new(0),
            lowest: AtomicU64::new(u64::MAX),
        }
    }
}

impl Controls {
    pub fn set_volume(&self, volume: f32) {
        self.volume.store(volume.max(0.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

/// How a track's loudness changes: steady, coming in, or going out.
#[derive(Clone, Copy, Debug)]
enum Ramp {
    Steady,
    /// Coming in from silence over `length` frames, `done` of them so far (a quarter sine:
    /// with the one going out, as loud all through).
    In {
        length: u32,
        done: u32,
    },
    /// Going out from `from` over `length` frames, `done` so far.
    Out {
        from: f32,
        length: u32,
        done: u32,
    },
}

impl Ramp {
    /// The loudness for the next frame, and moves on by one.
    fn step(&mut self) -> f32 {
        use std::f32::consts::FRAC_PI_2;
        match self {
            Ramp::Steady => 1.0,
            Ramp::In { length, done } => {
                let gain = (FRAC_PI_2 * *done as f32 / *length as f32).sin();
                *done += 1;
                if *done >= *length {
                    *self = Ramp::Steady;
                }
                gain
            }
            Ramp::Out { from, length, done } => {
                let gain =
                    if *done >= *length { 0.0 } else { *from * (FRAC_PI_2 * *done as f32 / *length as f32).cos() };
                *done = (*done + 1).min(*length);
                gain
            }
        }
    }

    /// The loudness now, without moving on.
    fn now(&self) -> f32 {
        self.clone().step()
    }

    fn finished(&self) -> bool {
        matches!(self, Ramp::Out { length, done, .. } if done >= length)
    }
}

struct Voice {
    stream: Arc<Shared>,
    ramp: Ramp,
}

/// Up to this many tracks fade out at once.
const FADING: usize = 4;

pub struct Deck {
    orders: Receiver<Order>,
    returned: SyncSender<Arc<Shared>>,
    controls: Arc<Controls>,
    rate: u32,
    current: Option<Voice>,
    /// A track asked to play, waiting for its first frames.
    waiting: Option<(Arc<Shared>, Duration)>,
    /// The track lined up to follow the one playing.
    then: Option<(Arc<Shared>, Duration)>,
    fading: [Option<Voice>; FADING],
    /// The volume as it was at the end of the last block, and how far in from a pause (0 to 1).
    level: f32,
    unpaused: f32,
    block: [Frame; BLOCK],
    at: usize,
    scratch: [Frame; BLOCK],
}

impl Deck {
    pub fn new(rate: u32, orders: Receiver<Order>, returned: SyncSender<Arc<Shared>>, controls: Arc<Controls>) -> Deck {
        let level = controls.volume();
        Deck {
            orders,
            returned,
            controls,
            rate: rate.max(1),
            current: None,
            waiting: None,
            then: None,
            fading: std::array::from_fn(|_| None),
            level,
            unpaused: 1.0,
            block: [[0.0; 2]; BLOCK],
            at: BLOCK * 2,
            scratch: [[0.0; 2]; BLOCK],
        }
    }

    fn frames(&self, time: Duration) -> u32 {
        (time.as_secs_f64() * f64::from(self.rate)).round().clamp(1.0, f64::from(u32::MAX)) as u32
    }

    /// Hands a track back to the window, which lets go of it there.
    fn give_back(&self, stream: Arc<Shared>) {
        stream.ended.store(true, Ordering::Release);
        stream.released.store(true, Ordering::Release);
        match self.returned.try_send(stream) {
            Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
        }
    }

    /// Sends what plays out over a moment (or `fade`), to make room for another.
    fn fade_out(&mut self, voice: Voice, fade: Duration) {
        let length = self.frames(fade.max(DECLICK));
        let from = voice.ramp.now();
        let voice = Voice { ramp: Ramp::Out { from, length, done: 0 }, ..voice };
        match self.fading.iter_mut().find(|slot| slot.is_none()) {
            Some(slot) => *slot = Some(voice),
            None => self.give_back(voice.stream),
        }
    }

    fn obey(&mut self) {
        while let Ok(order) = self.orders.try_recv() {
            match order {
                Order::Play { stream, fade } => {
                    if let Some((old, _)) = self.waiting.replace((stream, fade)) {
                        self.give_back(old);
                    }
                }
                Order::Then { stream, fade } => {
                    if let Some((old, _)) = self.then.replace((stream, fade)) {
                        self.give_back(old);
                    }
                }
                Order::NoThen => {
                    if let Some((old, _)) = self.then.take() {
                        self.give_back(old);
                    }
                }
                Order::Stop => {
                    for (old, _) in [self.waiting.take(), self.then.take()].into_iter().flatten() {
                        self.give_back(old);
                    }
                    if let Some(voice) = self.current.take() {
                        self.fade_out(voice, DECLICK);
                    }
                }
            }
        }
        // A track asked for begins once it has frames to play (one that can't be played is
        // handed back, and what plays goes on).
        if self.waiting.as_ref().is_some_and(|(stream, _)| stream.ready()) {
            let Some((stream, fade)) = self.waiting.take() else { return };
            if stream.has_failed() {
                self.give_back(stream);
                return;
            }
            if let Some(old) = self.current.take() {
                self.fade_out(old, fade);
            }
            let length = self.frames(fade.max(DECLICK));
            self.current = Some(Voice { stream, ramp: Ramp::In { length, done: 0 } });
        }
    }

    /// Lines the next track up in place of the one playing, as it comes to its end: fading in
    /// over the end of it, or (`fade` none) at once.
    fn follow(&mut self, fade: Duration) {
        let Some((stream, _)) = self.then.take() else { return };
        if stream.has_failed() {
            self.give_back(stream);
            return;
        }
        let ramp = if fade.is_zero() {
            Ramp::Steady
        } else {
            if let Some(old) = self.current.take() {
                self.fade_out(old, fade);
            }
            Ramp::In { length: self.frames(fade), done: 0 }
        };
        if let Some(old) = self.current.take() {
            self.give_back(old.stream);
        }
        self.current = Some(Voice { stream, ramp });
    }

    /// Makes the next block.
    fn fill(&mut self) {
        self.obey();
        self.block = [[0.0; 2]; BLOCK];
        let paused = self.controls.paused();
        if paused && self.unpaused <= 0.0 {
            // Paused: silence, and the tracks wait where they are.
            return;
        }
        // The one lined up starts fading in once the one playing has no more than the
        // crossfade left.
        if let (Some(current), Some((_, fade))) = (&self.current, &self.then)
            && !fade.is_zero()
            && current.stream.left().is_some_and(|left| left <= *fade)
        {
            let left = current.stream.left().unwrap_or(*fade).max(DECLICK);
            self.follow(left);
        }
        let something = self.current.is_some() || self.fading.iter().any(Option::is_some);
        if something {
            self.controls.blocks.fetch_add(1, Ordering::Relaxed);
        }
        // The track playing, to the frame its sound ends, and on into the one lined up.
        let mut filled = 0;
        while filled < BLOCK {
            let (got, over) = {
                let Some(current) = self.current.as_mut() else { break };
                let stream = &current.stream;
                if filled == 0 {
                    self.controls.lowest.fetch_min(stream.ring.len() as u64, Ordering::Relaxed);
                }
                let wanted = BLOCK - filled;
                // Asked before taking: once all is decoded, what the ring holds is all there is.
                let mut all_there = stream.decoded() || stream.has_failed();
                let mut got = stream.ring.take(&mut self.scratch[..wanted]);
                if got < wanted && !all_there && stream.decoded() {
                    // Its last frames came in just now.
                    all_there = true;
                    got += stream.ring.take(&mut self.scratch[got..wanted]);
                }
                if got > 0 {
                    stream.begin();
                }
                for (frame, sound) in self.block[filled..filled + got].iter_mut().zip(&self.scratch[..got]) {
                    let gain = current.ramp.step();
                    frame[0] += sound[0] * gain;
                    frame[1] += sound[1] * gain;
                }
                stream.played.fetch_add(got as u64, Ordering::Relaxed);
                (got, got < wanted && all_there)
            };
            filled += got;
            if filled == BLOCK {
                break;
            }
            if over {
                // Its last frame has played: the one lined up comes in on the next frame.
                let ended = self.current.take();
                if let Some(old) = ended {
                    self.give_back(old.stream);
                }
                if self.then.is_some() {
                    self.follow(Duration::ZERO);
                    continue;
                }
                break;
            }
            // Its decoding fell behind: the rest of the block is silent.
            self.controls.underruns.fetch_add(1, Ordering::Relaxed);
            break;
        }
        // Tracks fading out.
        for at in 0..FADING {
            let Some(voice) = &mut self.fading[at] else { continue };
            let got = voice.stream.ring.take(&mut self.scratch);
            for (frame, sound) in self.block.iter_mut().zip(&self.scratch[..got]) {
                let gain = voice.ramp.step();
                frame[0] += sound[0] * gain;
                frame[1] += sound[1] * gain;
            }
            voice.stream.played.fetch_add(got as u64, Ordering::Relaxed);
            let done = voice.ramp.finished() || (got < BLOCK && (voice.stream.decoded() || voice.stream.has_failed()));
            if done && let Some(voice) = self.fading[at].take() {
                self.give_back(voice.stream);
            }
        }
        // The volume, moving from the last block's to this one's across the block, and the
        // pause fading in or out.
        let volume = self.controls.volume();
        let step = (volume - self.level) / BLOCK as f32;
        let pause_step = 1.0 / self.frames(DECLICK) as f32;
        for frame in &mut self.block {
            self.level += step;
            self.unpaused =
                if paused { (self.unpaused - pause_step).max(0.0) } else { (self.unpaused + pause_step).min(1.0) };
            let gain = self.level * self.unpaused;
            frame[0] *= gain;
            frame[1] *= gain;
        }
        self.level = volume;
    }
}

impl Iterator for Deck {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= BLOCK * 2 {
            self.fill();
            self.at = 0;
        }
        let sample = self.block[self.at / 2][self.at % 2];
        self.at += 1;
        Some(sample)
    }
}

impl rodio::Source for Deck {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        2
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Sender, sync_channel};

    const RATE: u32 = 48_000;

    /// A track already decoded: `frames` of it, each frame's left side its number (from 1),
    /// its right side the number negated, at full scale.
    fn track(frames: usize, level: f32) -> Arc<Shared> {
        let shared = Arc::new(Shared::new(Duration::ZERO, RATE));
        let sound: Vec<Frame> = (0..frames).map(|_| [level, -level]).collect();
        assert_eq!(shared.ring.put(&sound), frames);
        shared.mark_decoded();
        shared
    }

    struct Rig {
        deck: Deck,
        orders: SyncSender<Order>,
        controls: Arc<Controls>,
        _returned: Receiver<Arc<Shared>>,
        _keep: Option<Sender<()>>,
    }

    fn rig() -> Rig {
        let (orders, orders_in) = sync_channel(16);
        let (returned, returned_in) = sync_channel(32);
        let controls = Arc::new(Controls::default());
        Rig {
            deck: Deck::new(RATE, orders_in, returned, controls.clone()),
            orders,
            controls,
            _returned: returned_in,
            _keep: None,
        }
    }

    fn take(deck: &mut Deck, frames: usize) -> Vec<Frame> {
        (0..frames).map(|_| [deck.next().unwrap(), deck.next().unwrap()]).collect()
    }

    #[test]
    fn a_track_that_follows_with_no_crossfade_starts_on_the_frame_after_the_last_ones_last() {
        let mut rig = rig();
        // 1,000 frames at 0.5, then 2,000 at 0.25: lengths that end inside a block.
        let (first, second) = (track(1000, 0.5), track(2000, 0.25));
        rig.orders.send(Order::Play { stream: first.clone(), fade: Duration::ZERO }).unwrap();
        rig.orders.send(Order::Then { stream: second.clone(), fade: Duration::ZERO }).unwrap();
        let out = take(&mut rig.deck, 4000);
        // The first comes in over 10 ms so it doesn't click, and then plays at its level.
        let declick = (RATE / 100) as usize;
        assert!(out[0][0].abs() < 0.01);
        assert!(out[declick..1000].iter().all(|frame| (frame[0] - 0.5).abs() < 1e-6 && (frame[1] + 0.5).abs() < 1e-6));
        // The join: frame 999 is the first's last, frame 1000 the second's first. No gap, no
        // overlap, nothing in between.
        assert!((out[999][0] - 0.5).abs() < 1e-6);
        assert!(out[1000..3000].iter().all(|frame| (frame[0] - 0.25).abs() < 1e-6), "{:?}", &out[995..1005]);
        // Then silence, the second having ended.
        assert!(out[3000..].iter().all(|frame| frame[0] == 0.0));
        assert!(first.ended.load(Ordering::Relaxed) && second.ended.load(Ordering::Relaxed));
        assert_eq!(rig.controls.underruns.load(Ordering::Relaxed), 0);
        assert_eq!(second.played.load(Ordering::Relaxed), 2000);
    }

    #[test]
    fn a_crossfade_keeps_the_sound_as_loud_all_through_with_no_step() {
        let mut rig = rig();
        // Two tracks of the same steady level, crossfading over the first's last 100 ms.
        let fade = Duration::from_millis(100);
        let (first, second) = (track(24_000, 0.5), track(24_000, 0.5));
        rig.orders.send(Order::Play { stream: first.clone(), fade: Duration::ZERO }).unwrap();
        rig.orders.send(Order::Then { stream: second.clone(), fade }).unwrap();
        let out = take(&mut rig.deck, 40_000);
        let declick = (RATE / 100) as usize;
        // From the first's start (after its 10 ms) to well into the second, the sound never
        // steps: no frame differs from the one before by more than a smooth curve would.
        let mut worst_step = 0.0f32;
        for pair in out[declick..30_000].windows(2) {
            worst_step = worst_step.max((pair[1][0] - pair[0][0]).abs());
        }
        assert!(worst_step < 0.001, "a step of {worst_step}");
        // Equal-power: the two at -3 dB each in the middle, so the level dips at most to
        // 0.5 times the square root of two over two... and never above 0.5 times root two.
        let lowest = out[declick..30_000].iter().map(|frame| frame[0]).fold(f32::MAX, f32::min);
        let highest = out[declick..30_000].iter().map(|frame| frame[0]).fold(0.0, f32::max);
        assert!(lowest >= 0.5 - 1e-4, "dips to {lowest}");
        assert!(highest <= 0.5 * std::f32::consts::SQRT_2 + 1e-4, "rises to {highest}");
        // The second began as the first had 100 ms left: 4,800 frames before the first's end.
        let played = second.played.load(Ordering::Relaxed) as usize;
        assert!(played.abs_diff(40_000 - 24_000 + 4_800) <= BLOCK, "{played}");
        assert_eq!(rig.controls.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_same_track_going_round_joins_seamlessly() {
        // A loop of one track: the second copy of a sine follows the first with no gap, so a
        // sine that ends at the end of a whole cycle carries straight on.
        let mut rig = rig();
        let cycle = 480; // 100 Hz
        let make = || {
            let shared = Arc::new(Shared::new(Duration::ZERO, RATE));
            let sound: Vec<Frame> = (0..cycle * 20)
                .map(|n| {
                    let value = (std::f32::consts::TAU * n as f32 / cycle as f32).sin() * 0.5;
                    [value, value]
                })
                .collect();
            shared.ring.put(&sound);
            shared.mark_decoded();
            shared
        };
        let (first, again) = (make(), make());
        rig.orders.send(Order::Play { stream: first, fade: Duration::ZERO }).unwrap();
        rig.orders.send(Order::Then { stream: again, fade: Duration::ZERO }).unwrap();
        let out = take(&mut rig.deck, cycle * 40);
        // Across the join (frame 9,600) the sine goes on exactly.
        for (n, frame) in out.iter().enumerate().skip(cycle * 2) {
            let exact = (std::f32::consts::TAU * (n % (cycle * 20)) as f32 / cycle as f32).sin() * 0.5;
            assert!((frame[0] - exact).abs() < 1e-5, "frame {n}: {} for {exact}", frame[0]);
        }
    }

    #[test]
    fn a_track_whose_decoding_falls_behind_is_counted_and_plays_on_when_it_catches_up() {
        let mut rig = rig();
        let slow = Arc::new(Shared::new(Duration::ZERO, RATE));
        slow.ring.put(&[[0.5, 0.5]; 600]);
        slow.mark_decoding();
        rig.orders.send(Order::Play { stream: slow.clone(), fade: Duration::ZERO }).unwrap();
        let out = take(&mut rig.deck, BLOCK * 4);
        assert!(out[600..].iter().all(|frame| frame[0] == 0.0));
        let underruns = rig.controls.underruns.load(Ordering::Relaxed);
        assert!(underruns >= 1, "{underruns}");
        slow.ring.put(&[[0.5, 0.5]; 600]);
        let out = take(&mut rig.deck, BLOCK);
        assert!(out.iter().all(|frame| (frame[0] - 0.5).abs() < 1e-6));
    }

    #[test]
    fn a_pause_fades_out_holds_the_place_and_fades_back_in() {
        let mut rig = rig();
        let long = track(48_000, 0.5);
        rig.orders.send(Order::Play { stream: long.clone(), fade: Duration::ZERO }).unwrap();
        let _ = take(&mut rig.deck, 4800);
        rig.controls.set_paused(true);
        let out = take(&mut rig.deck, 4800);
        let declick = (RATE / 100) as usize;
        // Down to silence within 10 ms (and a block), smoothly.
        assert!(out[declick + BLOCK..].iter().all(|frame| frame[0] == 0.0));
        let worst = out.windows(2).map(|pair| (pair[1][0] - pair[0][0]).abs()).fold(0.0, f32::max);
        assert!(worst < 0.01, "{worst}");
        let at = long.played.load(Ordering::Relaxed);
        let _ = take(&mut rig.deck, 4800);
        assert_eq!(long.played.load(Ordering::Relaxed), at, "the place is held");
        rig.controls.set_paused(false);
        let out = take(&mut rig.deck, 4800);
        assert!(out[declick + BLOCK..].iter().all(|frame| (frame[0] - 0.5).abs() < 1e-6));
    }

    #[test]
    fn a_track_that_cant_be_played_leaves_the_one_playing_as_it_was() {
        let mut rig = rig();
        let playing = track(48_000, 0.5);
        rig.orders.send(Order::Play { stream: playing.clone(), fade: Duration::ZERO }).unwrap();
        let _ = take(&mut rig.deck, 1000);
        let broken = Arc::new(Shared::new(Duration::ZERO, RATE));
        broken.mark_failed();
        rig.orders.send(Order::Play { stream: broken.clone(), fade: Duration::ZERO }).unwrap();
        let out = take(&mut rig.deck, 1000);
        assert!(out.iter().all(|frame| (frame[0] - 0.5).abs() < 1e-6));
        assert!(broken.released.load(Ordering::Relaxed));
        assert!(!playing.ended.load(Ordering::Relaxed));
    }

    #[test]
    fn the_volume_moves_smoothly() {
        let mut rig = rig();
        let long = track(48_000, 0.5);
        rig.orders.send(Order::Play { stream: long, fade: Duration::ZERO }).unwrap();
        let _ = take(&mut rig.deck, 1000);
        rig.controls.set_volume(0.0);
        let out = take(&mut rig.deck, BLOCK * 3);
        let worst = out.windows(2).map(|pair| (pair[1][0] - pair[0][0]).abs()).fold(0.0, f32::max);
        assert!(worst <= 0.5 / BLOCK as f32 + 1e-5, "{worst}");
        assert!(out[BLOCK * 2..].iter().all(|frame| frame[0] == 0.0));
    }
}
