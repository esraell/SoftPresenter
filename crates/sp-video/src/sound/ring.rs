//! The way a sound card's thread hands samples to the mix's thread without ever waiting: a
//! ring of fixed size, written by one thread and read by one other.
//!
//! The thread that plays the sound must never be held up: it asks for no memory, takes no
//! lock and calls nothing in the system here. Putting a piece in is a few atomic loads and one
//! atomic store for each sample, into room made when the ring was; if there is not room for
//! the whole piece (the mix's thread was held up), the piece is left out and counted, and the
//! speakers' sound goes on untouched. A piece is seen by the reader only once all of it is
//! in.
//!
//! A piece is its samples with what the mix needs to place them: their rate and channels, and
//! when the first of them is heard.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// What comes with a piece's samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Piece {
    pub rate: u32,
    pub channels: u16,
    /// When its first sample is heard, in microseconds from the mix's own start.
    pub heard: u64,
}

/// The words before a piece's samples: how many samples, the rate, the channels, and the
/// time in two halves.
const HEAD: usize = 5;

/// A ring for one writer and one reader. Each word is a sample's bits, or part of a piece's
/// head. Both ends may be used through a shared reference; nothing here is unsafe, so a
/// second writer or reader could only garble the sound, never memory.
#[derive(Debug)]
pub(super) struct Ring {
    words: Box<[AtomicU32]>,
    /// How many words have ever been read and written; where each end is, is that count's
    /// place in the ring. Only the reader moves the first, only the writer the second.
    read: AtomicUsize,
    written: AtomicUsize,
    /// Pieces left out for want of room.
    lost: AtomicU64,
}

impl Ring {
    /// A ring with room for at least `words` samples and heads (a power of two of them).
    pub(super) fn new(words: usize) -> Ring {
        let words = words.max(HEAD + 1).next_power_of_two();
        Ring {
            words: (0..words).map(|_| AtomicU32::new(0)).collect(),
            read: AtomicUsize::new(0),
            written: AtomicUsize::new(0),
            lost: AtomicU64::new(0),
        }
    }

    fn word(&self, at: usize) -> &AtomicU32 {
        &self.words[at & (self.words.len() - 1)]
    }

    /// The writer puts a piece in: all of it, or none if there is not room. Never waits, and
    /// asks the system for nothing. True when it went in.
    pub(super) fn put(&self, piece: Piece, samples: impl ExactSizeIterator<Item = f32>) -> bool {
        let count = samples.len();
        let written = self.written.load(Ordering::Relaxed);
        let used = written.wrapping_sub(self.read.load(Ordering::Acquire));
        let Ok(count_word) = u32::try_from(count) else { return false };
        if count == 0 || used + HEAD + count > self.words.len() {
            if count > 0 {
                self.lost.fetch_add(1, Ordering::Relaxed);
            }
            return false;
        }
        let head = [count_word, piece.rate, u32::from(piece.channels), piece.heard as u32, (piece.heard >> 32) as u32];
        for (at, word) in head.into_iter().enumerate() {
            self.word(written.wrapping_add(at)).store(word, Ordering::Relaxed);
        }
        // Exactly as many as were promised: silence for any that don't come.
        let mut samples = samples;
        for at in 0..count {
            let sample = samples.next().unwrap_or(0.0);
            self.word(written.wrapping_add(HEAD + at)).store(sample.to_bits(), Ordering::Relaxed);
        }
        // Only now can the reader see it.
        self.written.store(written.wrapping_add(HEAD + count), Ordering::Release);
        true
    }

    /// The reader takes the oldest piece, its samples into `samples` (emptied first). `None`
    /// when there is none.
    pub(super) fn take(&self, samples: &mut Vec<f32>) -> Option<Piece> {
        samples.clear();
        let read = self.read.load(Ordering::Relaxed);
        let waiting = self.written.load(Ordering::Acquire).wrapping_sub(read);
        if waiting < HEAD {
            return None;
        }
        let head: [u32; HEAD] = std::array::from_fn(|at| self.word(read.wrapping_add(at)).load(Ordering::Relaxed));
        let count = head[0] as usize;
        if count == 0 || HEAD + count > waiting {
            // Not a piece this ring's writer put in: what waits is let go.
            self.read.store(read.wrapping_add(waiting), Ordering::Release);
            return None;
        }
        samples.extend(
            (0..count).map(|at| f32::from_bits(self.word(read.wrapping_add(HEAD + at)).load(Ordering::Relaxed))),
        );
        self.read.store(read.wrapping_add(HEAD + count), Ordering::Release);
        Some(Piece {
            rate: head[1],
            channels: u16::try_from(head[2]).unwrap_or(0),
            heard: u64::from(head[3]) | u64::from(head[4]) << 32,
        })
    }

    /// The reader lets go of everything that waits.
    pub(super) fn clear(&self) {
        self.read.store(self.written.load(Ordering::Acquire), Ordering::Release);
    }

    /// How many pieces have been left out for want of room.
    pub(super) fn lost(&self) -> u64 {
        self.lost.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(heard: u64) -> Piece {
        Piece { rate: 48_000, channels: 2, heard }
    }

    #[test]
    fn an_empty_ring_gives_nothing_and_a_piece_comes_out_as_it_went_in() {
        let ring = Ring::new(64);
        let mut samples = vec![9.0];
        assert_eq!(ring.take(&mut samples), None);
        assert!(samples.is_empty());
        // A time past what 32 bits hold, and samples of every kind.
        let about = Piece { rate: 44_100, channels: 6, heard: 5_000_000_123 };
        let sent = [0.0, -0.0, 1.0, -1.0, 0.123_456_79, f32::MIN_POSITIVE];
        assert!(ring.put(about, sent.iter().copied()));
        assert_eq!(ring.take(&mut samples), Some(about));
        assert_eq!(samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>(), sent.map(f32::to_bits));
        assert_eq!(ring.take(&mut samples), None);
        // A piece with no samples is not put in, and is not counted as lost.
        assert!(!ring.put(about, std::iter::empty()));
        assert_eq!(ring.lost(), 0);
    }

    #[test]
    fn pieces_go_round_the_end_of_the_ring_whole_and_in_order() {
        // Room for 64 words; pieces of 5 + 7, so they fall across the end again and again.
        let ring = Ring::new(64);
        let mut samples = Vec::new();
        let mut next = 0u32;
        for round in 0..1000u64 {
            for _ in 0..3 {
                assert!(ring.put(piece(u64::from(next)), (0..7).map(|at| (next * 10 + at) as f32)));
                next += 1;
            }
            for back in (0..3).rev() {
                let was = next - 1 - back;
                assert_eq!(ring.take(&mut samples), Some(piece(u64::from(was))), "round {round}");
                assert_eq!(samples, (0..7).map(|at| (was * 10 + at) as f32).collect::<Vec<_>>());
            }
        }
        assert_eq!(ring.take(&mut samples), None);
        assert_eq!(ring.lost(), 0);
    }

    #[test]
    fn a_full_ring_leaves_the_new_piece_out_and_keeps_what_it_has() {
        let ring = Ring::new(64);
        // Five pieces of 5 + 7 words fill 60 of the 64; a sixth has no room.
        for n in 0..5 {
            assert!(ring.put(piece(n), (0..7u32).map(|at| (n * 10 + u64::from(at)) as f32)));
        }
        assert!(!ring.put(piece(5), (0..7).map(|_| 99.0)));
        assert!(!ring.put(piece(6), (0..7).map(|_| 99.0)));
        assert_eq!(ring.lost(), 2);
        // A piece too big for the ring altogether never goes in.
        assert!(!ring.put(piece(7), (0..100).map(|_| 99.0)));
        let mut samples = Vec::new();
        for n in 0..5 {
            assert_eq!(ring.take(&mut samples), Some(piece(n)));
            assert_eq!(samples, (0..7).map(|at| (n * 10 + at) as f32).collect::<Vec<_>>());
        }
        assert_eq!(ring.take(&mut samples), None);
        // Room again once it has been read; and what waits can be let go at once.
        assert!(ring.put(piece(8), (0..7).map(|_| 1.0)));
        assert!(ring.put(piece(9), (0..7).map(|_| 1.0)));
        ring.clear();
        assert_eq!(ring.take(&mut samples), None);
        assert!(ring.put(piece(10), (0..3).map(|_| 2.0)));
        assert_eq!(ring.take(&mut samples), Some(piece(10)));
        assert_eq!(samples, [2.0; 3]);
    }

    #[test]
    fn one_thread_writing_and_another_reading_lose_only_whole_pieces() {
        // A small ring, so it is often full and often empty: every piece that comes out is
        // whole and as it was written, in order, and those left out were counted.
        let ring = std::sync::Arc::new(Ring::new(1024));
        const PIECES: u64 = 200_000;
        let writer = {
            let ring = ring.clone();
            std::thread::spawn(move || {
                let mut put = 0u64;
                for n in 0..PIECES {
                    // Pieces of different lengths, each sample saying which piece and where.
                    let len = 1 + (n % 97) as usize;
                    if ring.put(piece(n), (0..len).map(|at| (n % 1000) as f32 + at as f32 / 128.0)) {
                        put += 1;
                    }
                    if n % 64 == 0 {
                        std::thread::yield_now();
                    }
                }
                put
            })
        };
        let (mut samples, mut got, mut before) = (Vec::new(), 0u64, None);
        let mut idle = 0u32;
        while idle < 2000 {
            let Some(about) = ring.take(&mut samples) else {
                // Done once the writer has finished and nothing waits.
                idle = if writer.is_finished() { idle + 1 } else { 0 };
                std::thread::yield_now();
                continue;
            };
            idle = 0;
            got += 1;
            let n = about.heard;
            assert_eq!((about.rate, about.channels), (48_000, 2));
            assert!(before.is_none_or(|before| n > before), "piece {n} after {before:?}");
            before = Some(n);
            assert_eq!(samples.len(), 1 + (n % 97) as usize, "piece {n}");
            for (at, sample) in samples.iter().enumerate() {
                assert_eq!(*sample, (n % 1000) as f32 + at as f32 / 128.0, "piece {n}, sample {at}");
            }
        }
        let put = writer.join().unwrap();
        assert_eq!(got, put, "every piece that went in came out");
        assert_eq!(put + ring.lost(), PIECES, "and the rest were counted as left out");
        assert!(got > 0);
    }
}
