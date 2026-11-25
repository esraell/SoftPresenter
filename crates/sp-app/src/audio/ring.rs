//! The way a track's decoded sound crosses from the thread that decodes it to the sound card's
//! thread that plays it: a ring of fixed size, written by one thread and read by one other.
//! The sound card's side never waits, takes no lock and asks for no memory: taking frames is a
//! few atomic loads and stores. Nothing here is unsafe; a second writer or reader could only
//! garble the sound, never memory.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::resample::Frame;

#[derive(Debug)]
pub struct Ring {
    /// Each sample's bits, left and right side by side.
    words: Box<[AtomicU32]>,
    /// How many samples have ever been read and written; where each end is, is that count's
    /// place in the ring. Only the reader moves the first, only the writer the second.
    read: AtomicUsize,
    written: AtomicUsize,
}

impl Ring {
    /// A ring with room for at least `frames` stereo frames (a power of two of samples).
    pub fn new(frames: usize) -> Ring {
        let words = (frames * 2).max(2).next_power_of_two();
        Ring {
            words: (0..words).map(|_| AtomicU32::new(0)).collect(),
            read: AtomicUsize::new(0),
            written: AtomicUsize::new(0),
        }
    }

    fn word(&self, at: usize) -> &AtomicU32 {
        &self.words[at & (self.words.len() - 1)]
    }

    /// Frames waiting to be read.
    pub fn len(&self) -> usize {
        self.written.load(Ordering::Acquire).wrapping_sub(self.read.load(Ordering::Acquire)) / 2
    }

    /// The writer puts in as many of `frames` as there is room for, and says how many.
    pub fn put(&self, frames: &[Frame]) -> usize {
        let written = self.written.load(Ordering::Relaxed);
        let used = written.wrapping_sub(self.read.load(Ordering::Acquire));
        let count = frames.len().min((self.words.len() - used) / 2);
        for (at, frame) in frames[..count].iter().enumerate() {
            self.word(written.wrapping_add(2 * at)).store(frame[0].to_bits(), Ordering::Relaxed);
            self.word(written.wrapping_add(2 * at + 1)).store(frame[1].to_bits(), Ordering::Relaxed);
        }
        // Only now can the reader see them.
        self.written.store(written.wrapping_add(2 * count), Ordering::Release);
        count
    }

    /// The reader takes up to `out.len()` frames into `out`, and says how many. Never waits.
    pub fn take(&self, out: &mut [Frame]) -> usize {
        let read = self.read.load(Ordering::Relaxed);
        let waiting = self.written.load(Ordering::Acquire).wrapping_sub(read) / 2;
        let count = out.len().min(waiting);
        for (at, frame) in out[..count].iter_mut().enumerate() {
            *frame = [
                f32::from_bits(self.word(read.wrapping_add(2 * at)).load(Ordering::Relaxed)),
                f32::from_bits(self.word(read.wrapping_add(2 * at + 1)).load(Ordering::Relaxed)),
            ];
        }
        self.read.store(read.wrapping_add(2 * count), Ordering::Release);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_come_out_as_they_went_in_round_and_round_and_never_more_than_there_is_room_for() {
        let ring = Ring::new(8);
        assert_eq!(ring.len(), 0);
        let mut out = [[0.0; 2]; 8];
        assert_eq!(ring.take(&mut out), 0);
        let frames: Vec<Frame> = (0..20).map(|n| [n as f32, -(n as f32)]).collect();
        // Round the ring several times, in uneven pieces.
        let (mut sent, mut got) = (0, Vec::new());
        while got.len() < frames.len() {
            sent += ring.put(&frames[sent..(sent + 5).min(frames.len())]);
            assert!(ring.len() <= 8);
            let taken = ring.take(&mut out[..3]);
            got.extend_from_slice(&out[..taken]);
        }
        assert_eq!(got, frames);
        // Full: no more goes in.
        assert_eq!(ring.put(&[[1.0, 1.0]; 12]), 8);
        assert_eq!(ring.put(&[[1.0, 1.0]]), 0);
        assert_eq!(ring.len(), 8);
    }

    #[test]
    fn one_thread_writes_while_another_reads_and_nothing_is_lost_or_out_of_order() {
        let ring = std::sync::Arc::new(Ring::new(256));
        let writer = ring.clone();
        let total = 200_000usize;
        let handle = std::thread::spawn(move || {
            let mut n = 0usize;
            while n < total {
                let chunk: Vec<Frame> = (n..(n + 97).min(total)).map(|k| [k as f32, k as f32 + 0.5]).collect();
                let mut sent = 0;
                while sent < chunk.len() {
                    sent += writer.put(&chunk[sent..]);
                    std::thread::yield_now();
                }
                n += chunk.len();
            }
        });
        let mut next = 0usize;
        let mut out = [[0.0; 2]; 61];
        while next < total {
            let taken = ring.take(&mut out);
            for frame in &out[..taken] {
                assert_eq!(*frame, [next as f32, next as f32 + 0.5]);
                next += 1;
            }
            std::thread::yield_now();
        }
        handle.join().unwrap();
    }
}
