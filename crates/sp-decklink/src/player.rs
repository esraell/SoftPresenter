//! What frames to give the card, and when: the rule a DeckLink output's thread follows, apart
//! from the card and the clock so that it can be tested with a card that only pretends.
//!
//! The card plays frames out on its own clock ("scheduled playback"): each frame is handed to
//! it with the time it is to be shown, a few frames ahead, and the card says when it has
//! finished with each. So the picture keeps coming out at the card's exact rate whatever the
//! computer is doing meanwhile. The newest picture is given to every frame scheduled: when no
//! new picture has come, the last one goes again, so a slow GPU or a busy moment never leaves
//! the card without a frame (which would show black or stutter).
//!
//! A frame the card holds (scheduled, not yet given back) is never written to: each frame of
//! the card's memory is in a small pool, written only once the card has finished with it, and
//! only if it holds an older picture than the newest. With a picture that stays (a slide),
//! after one round every frame holds it and nothing more is copied.

use std::time::Duration;

/// How a frame the card has finished with went. (Only the real card, which Windows builds go
/// without, drops or flushes frames.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
pub enum Finished {
    /// Shown at its time.
    Shown,
    /// Shown, but after its time: frames were scheduled for times that had already gone.
    Late,
    /// Not shown at all.
    Dropped,
    /// Given back unshown because playback stopped.
    Flushed,
}

/// What the player needs of a card. The real one is the DeckLink driver; the tests' and the
/// pretend devices' only keep time.
pub trait Card {
    type Frame;
    /// A new frame of the card's memory, the size and format the output plays.
    fn new_frame(&mut self) -> Result<Self::Frame, String>;
    /// Writes a whole picture, in the card's format, into a frame the card doesn't hold.
    fn write(&mut self, frame: &mut Self::Frame, bytes: &[u8]) -> Result<(), String>;
    /// What the card names a frame by when it has finished with it.
    fn key(frame: &Self::Frame) -> usize;
    /// Hands the card a frame to show at `at` for `duration` (in units of `scale` a second).
    fn schedule(&mut self, frame: &Self::Frame, at: i64, duration: i64, scale: i64) -> Result<(), String>;
    /// Starts the card's clock at `at`.
    fn start(&mut self, at: i64, scale: i64) -> Result<(), String>;
    /// Where the card's clock is now, in units of `scale` a second.
    fn stream_time(&mut self, scale: i64) -> Option<i64>;
    /// Waits up to `timeout` for frames the card has finished with: each one's key and how it
    /// went, and whether playback has stopped (or is about to, its profile changing).
    fn wait(&mut self, timeout: Duration) -> (Vec<(usize, Finished)>, Halt);
}

/// What, besides frames, the card has said since it was last asked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Halt {
    /// Playback stopped without being asked to.
    pub stopped: bool,
    /// The card's profile is changing, or has changed: its outputs may have been stopped.
    pub profile: bool,
}

/// Counts of frames, kept for the log and the operator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Shown, at their time or late.
    pub played: u64,
    /// Shown after their time.
    pub late: u64,
    /// Not shown.
    pub dropped: u64,
}

/// How many frames to keep scheduled ahead: about 80 ms' worth, at least three and at most
/// five. Fewer would leave too little time to schedule the next when the computer is busy; more
/// would show a new slide later than needed (each frame ahead is that much later on screen).
pub fn frames_ahead(duration: i64, scale: i64) -> usize {
    if duration <= 0 || scale <= 0 {
        return 3;
    }
    let frame = duration as f64 / scale as f64;
    ((0.08 / frame).ceil() as usize).clamp(3, 5)
}

struct Slot<F> {
    frame: F,
    /// Which picture it holds, if any.
    holds: Option<u64>,
    /// The card holds it: scheduled, and not yet given back.
    out: bool,
}

/// The pool of frames, the newest picture, and the card's timeline.
pub struct Player<F> {
    slots: Vec<Slot<F>>,
    picture: Vec<u8>,
    version: u64,
    duration: i64,
    scale: i64,
    /// When the next frame scheduled is to be shown.
    next_at: i64,
    ahead: usize,
    started: bool,
    pub counts: Counts,
}

impl<F> Player<F> {
    /// A player that starts with this picture (in the card's format: black, until the first
    /// picture comes), for frames lasting `duration / scale` seconds.
    pub fn new(picture: Vec<u8>, duration: i64, scale: i64) -> Player<F> {
        Player {
            slots: Vec::new(),
            picture,
            version: 0,
            duration,
            scale,
            next_at: 0,
            ahead: frames_ahead(duration, scale),
            started: false,
            counts: Counts::default(),
        }
    }

    /// From now on, frames scheduled show this picture.
    pub fn set_picture(&mut self, bytes: Vec<u8>) {
        self.picture = bytes;
        self.version += 1;
    }

    /// How many frames the card holds.
    pub fn out(&self) -> usize {
        self.slots.iter().filter(|slot| slot.out).count()
    }

    /// How long one frame lasts.
    pub fn frame_time(&self) -> Duration {
        Duration::from_secs_f64(self.duration as f64 / self.scale.max(1) as f64)
    }

    /// Schedules frames until the card holds as many as it should, each with the newest
    /// picture, and starts the card's clock the first time. The error is the card's.
    pub fn top_up<C: Card<Frame = F>>(&mut self, card: &mut C) -> Result<(), String> {
        while self.out() < self.ahead {
            let index = match self.free_slot() {
                Some(index) => index,
                None => {
                    // Room for one more than are kept ahead, and one more again: a frame given
                    // back is free at once, but the card may hand them back in a bunch.
                    if self.slots.len() >= self.ahead + 2 {
                        break;
                    }
                    let frame = card.new_frame()?;
                    self.slots.push(Slot { frame, holds: None, out: false });
                    self.slots.len() - 1
                }
            };
            let slot = &mut self.slots[index];
            if slot.holds != Some(self.version) {
                card.write(&mut slot.frame, &self.picture)?;
                slot.holds = Some(self.version);
            }
            card.schedule(&slot.frame, self.next_at, self.duration, self.scale)?;
            slot.out = true;
            self.next_at += self.duration;
        }
        if !self.started {
            card.start(0, self.scale)?;
            self.started = true;
        }
        Ok(())
    }

    /// A free frame: one that already holds the newest picture if there is one (so nothing
    /// is copied), else any.
    fn free_slot(&self) -> Option<usize> {
        let free = || self.slots.iter().enumerate().filter(|(_, slot)| !slot.out);
        free().find(|(_, slot)| slot.holds == Some(self.version)).or_else(|| free().next()).map(|(index, _)| index)
    }

    /// The card has finished with a frame. A frame shown late or dropped means the frames
    /// scheduled are behind the card's clock (the computer stalled): the next ones are
    /// scheduled from just ahead of where the clock is now, or every one after would be late
    /// too.
    pub fn finished<C: Card<Frame = F>>(&mut self, key: usize, how: Finished, card: &mut C) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| C::key(&slot.frame) == key) {
            slot.out = false;
        }
        match how {
            Finished::Shown => self.counts.played += 1,
            Finished::Late => {
                self.counts.played += 1;
                self.counts.late += 1;
                self.catch_up(card);
            }
            Finished::Dropped => {
                self.counts.dropped += 1;
                self.catch_up(card);
            }
            Finished::Flushed => {}
        }
    }

    fn catch_up<C: Card<Frame = F>>(&mut self, card: &mut C) {
        let Some(now) = card.stream_time(self.scale) else { return };
        // Two frames on from the frame the clock is in: time enough to reach the card.
        let earliest = (now.div_euclid(self.duration) + 2) * self.duration;
        if self.next_at < earliest {
            self.next_at = earliest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card that only keeps a list of what it was asked to do. Its clock is set by the test.
    #[derive(Default)]
    struct Pretend {
        frames: usize,
        /// Each frame's contents, by key.
        written: Vec<Vec<u8>>,
        writes: usize,
        /// (key, at) of each frame scheduled.
        scheduled: Vec<(usize, i64)>,
        started: Option<i64>,
        clock: i64,
    }

    impl Card for Pretend {
        type Frame = usize;
        fn new_frame(&mut self) -> Result<usize, String> {
            self.frames += 1;
            self.written.push(Vec::new());
            Ok(self.frames - 1)
        }
        fn write(&mut self, frame: &mut usize, bytes: &[u8]) -> Result<(), String> {
            self.writes += 1;
            self.written[*frame] = bytes.to_vec();
            Ok(())
        }
        fn key(frame: &usize) -> usize {
            *frame
        }
        fn schedule(&mut self, frame: &usize, at: i64, _duration: i64, _scale: i64) -> Result<(), String> {
            self.scheduled.push((*frame, at));
            Ok(())
        }
        fn start(&mut self, at: i64, _scale: i64) -> Result<(), String> {
            self.started = Some(at);
            Ok(())
        }
        fn stream_time(&mut self, _scale: i64) -> Option<i64> {
            Some(self.clock)
        }
        fn wait(&mut self, _timeout: Duration) -> (Vec<(usize, Finished)>, Halt) {
            (Vec::new(), Halt::default())
        }
    }

    /// 1080i59.94: a frame lasts 1001 of 30000.
    const D: i64 = 1001;
    const S: i64 = 30000;

    #[test]
    fn about_80_ms_of_frames_are_kept_ahead() {
        assert_eq!(frames_ahead(1001, 30000), 3, "29.97 frames a second");
        assert_eq!(frames_ahead(1001, 60000), 5, "59.94");
        assert_eq!(frames_ahead(1000, 50000), 4, "50");
        assert_eq!(frames_ahead(1000, 25000), 3, "25");
        assert_eq!(frames_ahead(0, 0), 3);
    }

    #[test]
    fn frames_are_scheduled_ahead_one_after_another_and_the_clock_started() {
        let mut card = Pretend::default();
        let mut player: Player<usize> = Player::new(vec![0; 4], D, S);
        player.top_up(&mut card).unwrap();
        assert_eq!(card.scheduled.iter().map(|(_, at)| *at).collect::<Vec<_>>(), vec![0, D, 2 * D]);
        assert_eq!(card.started, Some(0));
        assert_eq!(player.out(), 3);
        // Topping up again with all three still held schedules nothing.
        player.top_up(&mut card).unwrap();
        assert_eq!(card.scheduled.len(), 3);
    }

    #[test]
    fn the_last_picture_goes_again_until_a_new_one_comes_and_a_held_frame_is_never_written() {
        let mut card = Pretend::default();
        let mut player: Player<usize> = Player::new(vec![0; 4], D, S);
        player.top_up(&mut card).unwrap();
        // The card gives the first frame back: it goes again, with the same picture, and is
        // not written again.
        let writes = card.writes;
        player.finished(0, Finished::Shown, &mut card);
        player.top_up(&mut card).unwrap();
        assert_eq!(card.scheduled.last(), Some(&(0, 3 * D)));
        assert_eq!(card.writes, writes, "it already holds that picture");
        // A new picture: only frames given back are written with it, the held ones are not.
        player.set_picture(vec![9; 4]);
        player.finished(1, Finished::Shown, &mut card);
        player.top_up(&mut card).unwrap();
        assert_eq!(card.written[1], vec![9; 4]);
        assert_eq!(card.written[2], vec![0; 4], "frame 2 is still held by the card, untouched");
        assert_eq!(card.written[0], vec![0; 4]);
        assert_eq!(player.counts, Counts { played: 2, late: 0, dropped: 0 });
    }

    #[test]
    fn with_a_picture_that_stays_nothing_more_is_copied_after_one_round() {
        let mut card = Pretend::default();
        let mut player: Player<usize> = Player::new(vec![0; 4], D, S);
        player.top_up(&mut card).unwrap();
        player.set_picture(vec![5; 4]);
        // Three hundred frames given back in turn, ten seconds at 29.97.
        for turn in 0..300 {
            let key = card.scheduled[turn].0;
            player.finished(key, Finished::Shown, &mut card);
            player.top_up(&mut card).unwrap();
        }
        assert!(card.writes <= 3 + 3, "one write for each frame of the pool, then none: {}", card.writes);
        assert!(card.frames <= 5);
        assert_eq!(player.counts.played, 300);
    }

    #[test]
    fn after_a_stall_the_frames_are_scheduled_from_where_the_cards_clock_is() {
        let mut card = Pretend::default();
        let mut player: Player<usize> = Player::new(vec![0; 4], D, S);
        player.top_up(&mut card).unwrap();
        // The computer stalls for a second: the clock is 30 frames on, and what was scheduled
        // comes back late or dropped.
        card.clock = 30 * D + 10;
        player.finished(0, Finished::Late, &mut card);
        player.finished(1, Finished::Dropped, &mut card);
        player.top_up(&mut card).unwrap();
        let after: Vec<i64> = card.scheduled[3..].iter().map(|(_, at)| *at).collect();
        assert_eq!(after, vec![32 * D, 33 * D], "two frames on from the clock, on frame boundaries");
        assert!(after.iter().all(|at| *at > card.clock));
        // A later late frame from before the catch-up does not push the timeline further.
        player.finished(2, Finished::Late, &mut card);
        player.top_up(&mut card).unwrap();
        assert_eq!(card.scheduled.last().map(|(_, at)| *at), Some(34 * D));
        assert_eq!(player.counts, Counts { played: 2, late: 2, dropped: 1 });
    }

    #[test]
    fn frames_given_back_unshown_when_playback_stops_are_not_counted() {
        let mut card = Pretend::default();
        let mut player: Player<usize> = Player::new(vec![0; 4], D, S);
        player.top_up(&mut card).unwrap();
        player.finished(0, Finished::Flushed, &mut card);
        assert_eq!(player.counts, Counts::default());
        assert_eq!(player.out(), 2);
    }
}
