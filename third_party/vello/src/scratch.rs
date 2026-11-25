// Copyright 2026 the SoftPresenter contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SoftPresenter's addition to Vello 0.12: scratch buffers sized from what is drawn.
//!
//! The renderer's compute stages write lines, tiles, segments and per-tile command lists into
//! scratch buffers whose sizes must be fixed before the frame is sent to the GPU. Vello 0.12
//! gives each a constant size (about 165 MB in all, for every renderer) and never looks at
//! whether a frame outgrew them: a frame that did is silently not drawn.
//!
//! Here the sizes follow the drawing instead. Each frame's counters are read back before the
//! last stage runs; a frame that needed more room than it had is run again with more, and one
//! that fitted tells the next frame how much room to ask for.
//!
//! Three things keep that safe. The shaders are compiled with bounds checks (in
//! `wgpu_engine.rs`), so a stage that runs out of room cannot write outside its buffer. A
//! read-back the GPU never wrote is told apart from one that says "everything fitted"
//! ([`counters_read_back`]). And every set of sizes a frame is given goes through one place
//! ([`settled`]), which keeps them within what the GPU allows and gives the segments buffer
//! room for every segment count, since no stage checks that.

use std::time::{Duration, Instant};

use crate::wgpu_engine::{ResourcePool, SIZE_CLASS_BITS};

use vello_encoding::{
    BufferSize, BumpAllocators, Layout, LineSoup, PathSegment, RenderConfig, SegmentCount, Tile,
};

/// How long the room a heavy frame needed is still asked for after it was last drawn.
const REMEMBERED: Duration = Duration::from_secs(3);

/// How long a spare buffer no frame has used is kept before it is given back to the GPU.
pub(crate) const KEPT_SPARE: Duration = Duration::from_secs(6);

/// How long a large spare buffer no frame has used is kept: long enough for the screens of
/// different sizes that one thread draws in turn to share it, and no longer.
pub(crate) const KEPT_LARGE_SPARE: Duration = Duration::from_secs(1);

/// The most scratch memory one frame may be given, in bytes, counted as the buffers the engine
/// really makes for it. Two of the heaviest readings in a dissolve on a 4K screen take about
/// half of it; a frame that needs more is refused, so that a slide with a whole book pasted
/// into it cannot take the computer's memory with it.
const MOST_BYTES: u64 = 1 << 30;

/// The room asked for, over what the heaviest remembered frame used: some spare, so that a
/// fade or a wipe that changes the picture from frame to frame rarely has to be run again.
const SPARE: (u64, u64) = (5, 4);

/// A tile's command list grows by this many words at a time (`PTCL_INCREMENT` in the shaders).
const PTCL_INCREMENT: u32 = 256;

/// Every tile starts with this many words of command list (`PTCL_INITIAL_ALLOC` in the
/// shaders).
const PTCL_INITIAL_ALLOC: u32 = 64;

/// The stages that can run out of room, as the shaders flag them (`shared/bump.wgsl`).
const STAGE_BINNING: u32 = 0x1;
const STAGE_TILE_ALLOC: u32 = 0x2;
const STAGE_FLATTEN: u32 = 0x4;
const STAGE_PATH_COUNT: u32 = 0x8;
const STAGE_COARSE: u32 = 0x10;
const EVERY_STAGE: u32 =
    STAGE_BINNING | STAGE_TILE_ALLOC | STAGE_FLATTEN | STAGE_PATH_COUNT | STAGE_COARSE;

/// What every byte of a buffer the counters are copied into is set to when the buffer is
/// made. No stage can leave the counters like that (their first word is a set of five flags),
/// so a buffer still full of it was never written: the GPU dropped the frame.
pub(crate) const UNREAD: u8 = 0xFF;

/// The counters a frame's stages kept, from the bytes read back from the GPU; `None` when the
/// GPU never wrote them, or wrote something no stage could have. Such a frame was not worked
/// out at all, and must never be taken for one that fitted.
pub(crate) fn counters_read_back(bytes: &[u8]) -> Option<BumpAllocators> {
    let bytes = bytes.get(..size_of::<BumpAllocators>())?;
    let bump: BumpAllocators = bytemuck::pod_read_unaligned(bytes);
    (bump.failed & !EVERY_STAGE == 0).then_some(bump)
}

/// A fingerprint of a scene as it is sent to the GPU, and of the size of the target it is drawn
/// on: scenes that differ in any byte, or in size, have different fingerprints, as near to
/// surely as 64 bits allow. It is taken of every frame (all of a slide's outlines, megabytes
/// for a slide full of text), so it reads the scene 48 bytes at a time in three strands, each
/// mixed by multiplying two words into 128 bits and folding the halves together (the mixing
/// step several fast hashes are built on; the code here is our own). That is several times
/// faster than the standard library's hasher, which took 1.9 ms for the 8 MB of a full
/// reading. It is not for telling apart scenes someone made to collide.
pub(crate) fn fingerprint(scene: &[u8], width: u32, height: u32) -> u64 {
    // Numbers with no pattern to them: the fractional parts of the square roots of 2, 3, 5
    // and 7.
    const KEYS: [u64; 4] = [
        0x6a09_e667_f3bc_c908,
        0xbb67_ae85_84ca_a73b,
        0x3c6e_f372_fe94_f82b,
        0xa54f_f53a_5f1d_36f1,
    ];
    fn mix(a: u64, b: u64) -> u64 {
        let wide = u128::from(a) * u128::from(b);
        (wide as u64) ^ ((wide >> 64) as u64)
    }
    fn word(block: &[u8], at: usize) -> u64 {
        let mut word = [0; 8];
        word.copy_from_slice(&block[at..at + 8]);
        u64::from_le_bytes(word)
    }
    fn strands(block: &[u8], strands: &mut [u64; 3]) {
        strands[0] = mix(word(block, 0) ^ KEYS[1], word(block, 8) ^ strands[0]);
        strands[1] = mix(word(block, 16) ^ KEYS[2], word(block, 24) ^ strands[1]);
        strands[2] = mix(word(block, 32) ^ KEYS[3], word(block, 40) ^ strands[2]);
    }
    let size = (u64::from(width) << 32) | u64::from(height);
    let start = mix(size ^ KEYS[0], KEYS[1]);
    let mut mixed = [start, start ^ KEYS[2], start ^ KEYS[3]];
    let mut blocks = scene.chunks_exact(48);
    for block in &mut blocks {
        strands(block, &mut mixed);
    }
    // What is left over, filled up with zeros; the length at the end tells them from real ones.
    let left = blocks.remainder();
    let mut last = [0; 48];
    last[..left.len()].copy_from_slice(left);
    strands(&last, &mut mixed);
    let all = mix(mixed[0] ^ KEYS[1], mixed[1] ^ KEYS[2]) ^ mixed[2];
    mix(all ^ KEYS[0], scene.len() as u64 ^ KEYS[1])
}

/// How many elements each scratch buffer holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScratchSizes {
    /// Flattened lines.
    pub lines: u32,
    /// Draw information and binning output, in words.
    pub bin_data: u32,
    /// Tiles.
    pub tiles: u32,
    /// Segment counts.
    pub seg_counts: u32,
    /// Path segments.
    pub segments: u32,
    /// Blend stack spill, in words.
    pub blend_spill: u32,
    /// Per-tile command lists, in words.
    pub ptcl: u32,
}

/// The bytes one element of each buffer takes.
const ELEMENT_BYTES: ScratchSizes = ScratchSizes {
    lines: size_of::<LineSoup>() as u32,
    bin_data: 4,
    tiles: size_of::<Tile>() as u32,
    seg_counts: size_of::<SegmentCount>() as u32,
    segments: size_of::<PathSegment>() as u32,
    blend_spill: 4,
    ptcl: 4,
};

impl ScratchSizes {
    /// The constant sizes Vello 0.12 gives every frame: about 165 MB.
    pub const UPSTREAM: Self = Self {
        lines: 1 << 21,
        bin_data: 1 << 18,
        tiles: 1 << 21,
        seg_counts: 1 << 21,
        segments: 1 << 21,
        blend_spill: 1 << 20,
        ptcl: 1 << 23,
    };

    /// The room kept ready when nothing heavy has been drawn lately: about 33 MB, on top of
    /// the command list every tile of the target starts with (8 MB for a 4K screen). A song
    /// slide of four lines in each of two languages, outlined and shadowed, makes about 95 000
    /// lines and 125 000 segments on a 1080p screen; this is room for four times that.
    pub const RESTING: Self = Self {
        lines: 1 << 19,
        bin_data: 1 << 14,
        tiles: 1 << 18,
        seg_counts: 1 << 19,
        segments: 1 << 19,
        blend_spill: 1 << 12,
        ptcl: 1 << 19,
    };

    /// A guess at the room a scene needs before any of it has been drawn, from how many path
    /// segments it has and how wide the target is, so that a heavy slide is rarely run more
    /// than once the first time it is shown. Measured on slides of outlined, shadowed text:
    /// each path segment makes about 3.5 lines and 3.5 tile segments on a thumbnail, and 5.5
    /// lines, 9 tile segments and 6.5 words of command list on a 4K screen. A wrong guess costs
    /// a run or some memory for one frame, never a wrong picture; and it stops at eight times
    /// the resting room, past which a scene is left to say what it needs.
    fn guessed(layout: &Layout, width_in_tiles: u32) -> Self {
        let tags = u64::from(layout.path_tags_size());
        // 240 tiles are 3840 pixels.
        let width = u64::from(width_in_tiles.min(240));
        let count = |tenths: u64| u32::try_from(tags * tenths / 10).unwrap_or(u32::MAX);
        let segments = count(35 + 60 * width / 240);
        let guess = Self {
            lines: count(40 + 22 * width / 240),
            bin_data: 0,
            tiles: count(10),
            seg_counts: segments,
            segments,
            blend_spill: 0,
            ptcl: count(10 + 60 * width / 240),
        };
        guess.at_most(Self::RESTING.zip(Self::RESTING, |n, _| n.saturating_mul(8)))
    }

    /// The size in bytes of the buffer the engine makes for each of these sizes, in the order
    /// of the fields.
    fn buffers(&self) -> [u64; 7] {
        let each = ELEMENT_BYTES;
        [
            (self.lines, each.lines),
            (self.bin_data, each.bin_data),
            (self.tiles, each.tiles),
            (self.seg_counts, each.seg_counts),
            (self.segments, each.segments),
            (self.blend_spill, each.blend_spill),
            (self.ptcl, each.ptcl),
        ]
        .map(|(count, each)| pool_class(u64::from(count.max(1)) * u64::from(each)))
    }

    /// The bytes of GPU memory buffers of these sizes take, as the engine makes them: each
    /// rounded up to a size buffers are pooled at.
    pub fn bytes(&self) -> u64 {
        self.buffers().into_iter().sum()
    }

    fn zip(self, other: Self, f: impl Fn(u32, u32) -> u32) -> Self {
        Self {
            lines: f(self.lines, other.lines),
            bin_data: f(self.bin_data, other.bin_data),
            tiles: f(self.tiles, other.tiles),
            seg_counts: f(self.seg_counts, other.seg_counts),
            segments: f(self.segments, other.segments),
            blend_spill: f(self.blend_spill, other.blend_spill),
            ptcl: f(self.ptcl, other.ptcl),
        }
    }

    fn at_least(self, other: Self) -> Self {
        self.zip(other, u32::max)
    }

    fn at_most(self, other: Self) -> Self {
        self.zip(other, u32::min)
    }

    /// True when every buffer of these sizes holds what `needed` asks for.
    fn hold(&self, needed: &Self) -> bool {
        *self == self.at_least(*needed)
    }

    /// Each size with the spare added, rounded up to a size buffers are pooled at.
    fn with_spare(self) -> Self {
        self.zip(ELEMENT_BYTES, |n, each| {
            let spare = u64::from(n) * SPARE.0 / SPARE.1;
            pooled(u32::try_from(spare).unwrap_or(u32::MAX), each)
        })
    }

    /// The most elements each buffer can have when no buffer may be larger than `bytes`.
    fn within(bytes: u64) -> Self {
        // The engine rounds a buffer's size up to one it pools at, so the largest allowed is
        // the largest pooled size that fits.
        let mut largest = pool_class(bytes);
        while largest > bytes {
            // The next pooled size down is always more than three quarters of this one.
            largest = pool_class(largest / 4 * 3 + 1);
        }
        let count = |each: u32| u32::try_from(largest / u64::from(each)).unwrap_or(u32::MAX);
        Self::default().zip(ELEMENT_BYTES, |_, each| count(each))
    }
}

/// Rounds a size in bytes up to one the engine makes buffers at.
fn pool_class(bytes: u64) -> u64 {
    ResourcePool::size_class(bytes, SIZE_CLASS_BITS)
}

/// Rounds a count of elements up to all that fits in the buffer the engine would make for it:
/// asking for less wastes the difference.
fn pooled(count: u32, element_bytes: u32) -> u32 {
    let each = u64::from(element_bytes);
    u32::try_from(pool_class(u64::from(count.max(1)) * each) / each).unwrap_or(u32::MAX)
}

/// The sizes a frame can really be given, from sizes someone would like it to have: no buffer
/// larger than the GPU allows or smaller than the scene and target need anyway, and the
/// segments buffer never shorter than the segment counts buffer.
///
/// Every segment count becomes at most one segment, and the stage that writes the segments
/// checks only that the counts fitted. So with fewer segments than counts, a frame could pass
/// that check and still have more segments than their buffer holds. Every set of sizes a
/// frame runs with comes through here, so that cannot happen.
fn settled(sizes: ScratchSizes, floor: ScratchSizes, limit: ScratchSizes) -> ScratchSizes {
    let mut sizes = sizes.at_most(limit).at_least(floor);
    sizes.segments = sizes
        .segments
        .max(sizes.seg_counts)
        .min(limit.segments.max(floor.segments));
    sizes.seg_counts = sizes.seg_counts.min(sizes.segments);
    sizes
}

/// The scratch sizes one frame is given, and what the scene and target take of them anyway.
#[derive(Clone, Copy, Debug)]
pub struct ScratchPlan {
    /// The sizes this frame's buffers have.
    pub sizes: ScratchSizes,
    /// The sizes they would have with the resting room.
    resting: ScratchSizes,
    /// The least each buffer can be for this scene on this target, whatever is drawn.
    floor: ScratchSizes,
    /// Words of `bin_data` taken by draw information, before binning's own output.
    bin_data_start: u32,
    /// Words of command list the tiles start with, before any is added.
    ptcl_start: u32,
}

impl ScratchPlan {
    /// The plan for a scene of this layout on a target of this many tiles. The room it is
    /// given is counted from where the scene's and target's own share of `bin_data` and `ptcl`
    /// ends; no buffer is made larger than the room's limit unless that share alone is.
    ///
    /// Of the room wanted with a guess for the scene, the room wanted, the least the room may
    /// be and, when the sizes follow the drawing, the resting room, the frame gets the first
    /// that keeps it within the most one frame may have. The room wanted and the least are
    /// what other scenes used lately, each buffer's most taken together, which can be more
    /// than any one frame may have though each scene fitted; this frame is not refused for
    /// that. If even the last does not fit, the plan says so
    /// ([`ScratchPlan::within_the_most`]) and must not be run.
    pub(crate) fn new(
        layout: &Layout,
        width_in_tiles: u32,
        height_in_tiles: u32,
        room: ScratchRoom,
    ) -> Self {
        let ScratchRoom {
            wanted,
            least,
            limit,
            follows_drawing,
        } = room;
        let bin_data_start = layout.bin_data_start;
        let ptcl_start = width_in_tiles
            .saturating_mul(height_in_tiles)
            .saturating_mul(PTCL_INITIAL_ALLOC);
        // The shaders take a command list shorter than the tiles' own starts, or draw
        // information longer than its buffer, without flagging either: neither is ever allowed.
        let floor = ScratchSizes {
            lines: 1,
            bin_data: bin_data_start.saturating_add(1),
            tiles: 1,
            seg_counts: 1,
            segments: 1,
            blend_spill: 1,
            ptcl: ptcl_start.saturating_add(PTCL_INCREMENT),
        };
        let starts = ScratchSizes {
            bin_data: bin_data_start,
            ptcl: ptcl_start,
            ..ScratchSizes::default()
        };
        let from =
            |beyond: ScratchSizes| settled(beyond.zip(starts, u32::saturating_add), floor, limit);
        let resting = from(ScratchSizes::RESTING);
        let (guessed, last) = if follows_drawing {
            let guess = ScratchSizes::guessed(layout, width_in_tiles);
            (wanted.at_least(guess), resting)
        } else {
            (wanted, from(least))
        };
        let sizes = [guessed, wanted, least]
            .into_iter()
            .map(from)
            .find(|sizes| sizes.bytes() <= MOST_BYTES)
            .unwrap_or(last);
        Self {
            sizes,
            resting,
            floor,
            bin_data_start,
            ptcl_start,
        }
    }

    /// False when the frame would take more scratch memory than one frame may have, even with
    /// the least room it could be given. Such a plan must not be run.
    pub(crate) fn within_the_most(&self) -> bool {
        self.sizes.bytes() <= MOST_BYTES
    }

    /// Writes the plan's sizes into a frame's configuration, in place of Vello's constants.
    pub(crate) fn apply(&self, config: &mut RenderConfig) {
        let sizes = &self.sizes;
        config.buffer_sizes.lines = BufferSize::new(sizes.lines);
        config.buffer_sizes.bin_data = BufferSize::new(sizes.bin_data);
        config.buffer_sizes.tiles = BufferSize::new(sizes.tiles);
        config.buffer_sizes.seg_counts = BufferSize::new(sizes.seg_counts);
        config.buffer_sizes.segments = BufferSize::new(sizes.segments);
        config.buffer_sizes.blend_spill = BufferSize::new(sizes.blend_spill);
        config.buffer_sizes.ptcl = BufferSize::new(sizes.ptcl);
        config.gpu.lines_size = sizes.lines;
        config.gpu.binning_size = sizes.bin_data - self.bin_data_start;
        config.gpu.tiles_size = sizes.tiles;
        config.gpu.seg_counts_size = sizes.seg_counts;
        config.gpu.segments_size = sizes.segments;
        config.gpu.blend_size = sizes.blend_spill;
        config.gpu.ptcl_size = sizes.ptcl;
    }

    /// What the frame used of each buffer, by the counters the GPU kept. Counters of stages
    /// after one that ran out of room are too low: those stages did not run.
    fn used(&self, bump: &BumpAllocators) -> ScratchSizes {
        ScratchSizes {
            lines: bump.lines,
            bin_data: self.bin_data_start.saturating_add(bump.binning),
            tiles: bump.tile,
            seg_counts: bump.seg_counts,
            segments: bump.segments,
            blend_spill: bump.blend,
            ptcl: self.ptcl_start.saturating_add(bump.ptcl),
        }
    }

    /// Sizes without the scene's and target's own share of the two shared buffers, so that
    /// they can be carried from one frame to another of a different size.
    fn beyond_starts(&self, sizes: ScratchSizes) -> ScratchSizes {
        ScratchSizes {
            bin_data: sizes.bin_data.saturating_sub(self.bin_data_start),
            ptcl: sizes.ptcl.saturating_sub(self.ptcl_start),
            ..sizes
        }
    }

    /// The largest buffer a frame within the resting sizes has on this target with this
    /// scene, in bytes: a larger one left spare was made for something heavier.
    fn largest_resting_buffer(&self) -> u64 {
        self.resting.buffers().into_iter().max().unwrap_or(0)
    }

    /// The sizes to run the frame again with, after it ran out of room; `None` when there are
    /// none that the GPU allows, that stay within the most one frame may have, and that give
    /// the frame more than it had.
    ///
    /// The stage that ran out first gets what it asked for and some spare; the stages after
    /// it, which did not run and so could not say, get twice what they had. If that would
    /// pass the most one frame may have, the later stages are left to ask in their turn; and
    /// if that still would, the spare goes too.
    ///
    /// The room the frame had may be more than it needs: what another scene used lately. If
    /// keeping it is all that takes the frame past the most it may have, it is let go: every
    /// buffer gets what the counters say this frame used (they are right up to the stage that
    /// ran out, and too low after it) or the resting room, with some spare if that fits. So
    /// `None` means that the scene itself is too heavy, whatever was drawn before it.
    fn grown(&self, bump: &BumpAllocators, limit: ScratchSizes) -> Option<ScratchSizes> {
        let used = self.used(bump);
        let had = self.sizes;
        // The stages in the order they run, each with the buffers it fills.
        let over = [
            used.lines > had.lines,
            used.bin_data > had.bin_data,
            used.tiles > had.tiles,
            used.seg_counts > had.seg_counts,
            used.ptcl > had.ptcl
                || used.blend_spill > had.blend_spill
                || used.segments > had.segments,
        ];
        let flagged = [
            STAGE_FLATTEN,
            STAGE_BINNING,
            STAGE_TILE_ALLOC,
            STAGE_PATH_COUNT,
            STAGE_COARSE,
        ]
        .map(|stage| bump.failed & stage != 0);
        let first = (0..over.len()).find(|stage| over[*stage] || flagged[*stage])?;
        // A stage counts all it needed even when it ran out, so its counter says what to give
        // it. Should one ever be flagged without its counter saying so, it is doubled as well.
        let doubled_from = if over[first] { first + 1 } else { first };
        let stage = ScratchSizes {
            lines: 0,
            bin_data: 1,
            tiles: 2,
            seg_counts: 3,
            segments: 4,
            blend_spill: 4,
            ptcl: 4,
        };
        let doubled = had.zip(ELEMENT_BYTES, |n, each| pooled(n.saturating_mul(2), each));
        let later = stage.zip(doubled, |stage, doubled| {
            if stage as usize >= doubled_from {
                doubled
            } else {
                0
            }
        });
        let asked = used.with_spare().at_least(had);
        let own = used.at_least(self.resting);
        let own_with_spare = used.with_spare().at_least(self.resting);
        [
            asked.at_least(later),
            asked,
            used.at_least(had),
            own_with_spare,
            own,
        ]
        .into_iter()
        .map(|sizes| settled(sizes, self.floor, limit))
        .find(|sizes| sizes.bytes() <= MOST_BYTES && sizes.hold(&used) && *sizes != had)
    }
}

/// The scratch room to give a frame, and the most the GPU lets any frame have.
#[derive(Clone, Copy, Debug)]
pub struct ScratchRoom {
    /// The room to give the frame, counted from where the scene's and target's own share of
    /// the two shared buffers ends.
    pub wanted: ScratchSizes,
    /// The room to give it instead, counted the same way, if `wanted` would take the frame
    /// past the most scratch memory one frame may have.
    pub least: ScratchSizes,
    /// The most elements each buffer may have.
    pub limit: ScratchSizes,
    /// True when the sizes follow the drawing (they are not fixed): the frame is given more
    /// than `wanted` when the scene looks as if it needs it, and the resting room when
    /// `wanted` and `least` would both take it past the most one frame may have.
    pub follows_drawing: bool,
}

/// What a frame's counters say about the room it was given.
#[derive(Debug)]
pub(crate) enum Verdict {
    /// Everything fitted: the last stage can run.
    Fits,
    /// Something ran out of room: run the frame again with this much.
    Again(ScratchSizes),
    /// The frame needs more room than the renderer may use.
    TooHeavy,
}

/// A renderer's memory of how much scratch room its recent frames used.
#[derive(Debug)]
pub(crate) struct Scratch {
    /// Sizes used for every frame as they are, never grown: Vello's own behaviour.
    fixed: Option<ScratchSizes>,
    /// The most any frame used since `since`, and in the stretch of time before that; both
    /// without the scene's and target's own share.
    lately: ScratchSizes,
    before: ScratchSizes,
    since: Instant,
    /// The most elements the GPU lets each buffer have.
    limit: ScratchSizes,
    /// How many times a frame had to be run again for want of room.
    reruns: u64,
    /// The scenes drawn lately that fitted, newest last: each one's fingerprint and what it
    /// used of every buffer.
    fitted: Vec<(u64, ScratchSizes)>,
    /// How many frames were drawn without waiting for their counters.
    unwaited: u64,
    /// The scenes found too heavy to draw, newest last, by their fingerprints.
    refused: Vec<u64>,
    /// The largest buffer a frame within the resting sizes has, on the largest target drawn
    /// lately, and when a target that large was last drawn.
    resting_buffer: (u64, Instant),
}

/// How many scenes that fitted are kept in mind: a few screens of different sizes, each
/// showing one scene or fading between two.
const FITTED_KEPT: usize = 8;

/// How many scenes found too heavy are kept in mind.
const REFUSED_KEPT: usize = 8;

impl Scratch {
    /// A new memory, for a GPU whose largest buffer a stage can use is this many bytes.
    pub(crate) fn new(largest_buffer: u64) -> Self {
        Self {
            fixed: None,
            lately: ScratchSizes::default(),
            before: ScratchSizes::default(),
            since: Instant::now(),
            limit: ScratchSizes::within(largest_buffer),
            reruns: 0,
            fitted: Vec::new(),
            unwaited: 0,
            refused: Vec::new(),
            resting_buffer: (0, Instant::now()),
        }
    }

    pub(crate) fn set_fixed(&mut self, sizes: Option<ScratchSizes>) {
        self.fixed = sizes;
        // What is too heavy for one set of sizes may not be for another.
        self.refused.clear();
    }

    pub(crate) fn reruns(&self) -> u64 {
        self.reruns
    }

    pub(crate) fn unwaited(&self) -> u64 {
        self.unwaited
    }

    /// True when a scene with this fingerprint was drawn before and fitted in no more room
    /// than this plan gives. The GPU's stages count the same for the same scene and target
    /// size every time, so it fits again and its counters need not be read.
    pub(crate) fn fitted_before(&mut self, print: u64, plan: &ScratchPlan, now: Instant) -> bool {
        let Some(at) = self.fitted.iter().position(|(known, _)| *known == print) else {
            return false;
        };
        let (_, used) = self.fitted[at];
        if !plan.sizes.hold(&used) {
            return false;
        }
        // Newest last, and the room it uses stays remembered while it keeps being drawn.
        let known = self.fitted.remove(at);
        self.fitted.push(known);
        self.remember(plan.beyond_starts(used), now);
        self.unwaited += 1;
        true
    }

    /// Remembers a scene found too heavy to draw, by its fingerprint: the newest few are kept.
    pub(crate) fn refuse(&mut self, print: u64) {
        self.refused.retain(|known| *known != print);
        if self.refused.len() >= REFUSED_KEPT {
            self.refused.remove(0);
        }
        self.refused.push(print);
    }

    /// True when a scene with this fingerprint was found too heavy to draw. The same scene on
    /// the same target is too heavy every time, so it need not be sent to the GPU to find out
    /// again. Only what the counters of a frame really said is remembered here (or a stage run
    /// on the processor that ran out of room): never a frame the GPU dropped or was too slow
    /// with.
    pub(crate) fn refused_before(&self, print: u64) -> bool {
        self.refused.contains(&print)
    }

    #[cfg(test)]
    fn limit(&self) -> ScratchSizes {
        self.limit
    }

    /// The room for a frame drawn now.
    pub(crate) fn room(&self, now: Instant) -> ScratchRoom {
        let least = match self.fixed {
            Some(sizes) => sizes,
            None => self
                .remembered(now)
                .at_least(ScratchSizes::RESTING)
                .at_most(self.limit),
        };
        ScratchRoom {
            wanted: self.wanted(now),
            least,
            limit: self.limit,
            follows_drawing: self.fixed.is_none(),
        }
    }

    /// The room for a frame that is run again with this much, as [`Scratch::judge`] said.
    pub(crate) fn room_to_run_again(&self, more: ScratchSizes) -> ScratchRoom {
        ScratchRoom {
            wanted: more,
            least: more,
            limit: self.limit,
            follows_drawing: self.fixed.is_none(),
        }
    }

    /// The room to give a frame drawn now, counted beyond the shared buffers' starts.
    pub(crate) fn wanted(&self, now: Instant) -> ScratchSizes {
        match self.fixed {
            Some(sizes) => sizes,
            None => self
                .remembered(now)
                .with_spare()
                .at_least(ScratchSizes::RESTING)
                .at_most(self.limit),
        }
    }

    /// The most a frame used in the time still remembered now.
    fn remembered(&self, now: Instant) -> ScratchSizes {
        let age = now.saturating_duration_since(self.since);
        if age >= REMEMBERED * 2 {
            ScratchSizes::default()
        } else if age >= REMEMBERED {
            self.lately
        } else {
            self.lately.at_least(self.before)
        }
    }

    /// True when a target of this size can be drawn at all: its tiles' own command lists fit
    /// in one buffer.
    pub(crate) fn allows(&self, width: u32, height: u32) -> bool {
        let nothing = ScratchSizes::default();
        let room = ScratchRoom {
            wanted: nothing,
            least: nothing,
            limit: self.limit,
            follows_drawing: false,
        };
        let (width, height) = (width.div_ceil(16), height.div_ceil(16));
        let plan = ScratchPlan::new(&Layout::default(), width, height, room);
        self.limit.hold(&plan.sizes)
    }

    /// True while a frame heavier than the resting sizes allow for is remembered.
    pub(crate) fn is_heavy(&self, now: Instant) -> bool {
        self.fixed.is_none() && self.wanted(now) != ScratchSizes::RESTING.at_most(self.limit)
    }

    /// Notes that a frame with this plan is being drawn now, for
    /// [`Scratch::largest_resting_buffer`].
    pub(crate) fn drawing(&mut self, plan: &ScratchPlan, now: Instant) {
        let largest = plan.largest_resting_buffer();
        let (kept, seen) = self.resting_buffer;
        // A larger target counts at once; a smaller one only when the larger is no longer
        // being drawn.
        if largest >= kept || now.saturating_duration_since(seen) >= REMEMBERED * 2 {
            self.resting_buffer = (largest, now);
        }
    }

    /// The largest buffer a frame within the resting sizes has on the largest target drawn
    /// lately, in bytes. A larger buffer left spare was made for something heavier, and is
    /// not kept as long.
    pub(crate) fn largest_resting_buffer(&self) -> u64 {
        self.resting_buffer.0
    }

    /// Judges a frame by the counters read back from the GPU.
    pub(crate) fn judge(
        &mut self,
        plan: &ScratchPlan,
        bump: &BumpAllocators,
        print: u64,
        now: Instant,
    ) -> Verdict {
        let used = plan.used(bump);
        if bump.failed == 0 && plan.sizes.hold(&used) {
            self.remember(plan.beyond_starts(used), now);
            self.fitted.retain(|(known, _)| *known != print);
            if self.fitted.len() >= FITTED_KEPT {
                self.fitted.remove(0);
            }
            self.fitted.push((print, used));
            return Verdict::Fits;
        }
        // Refused when told to keep to fixed sizes, when what is known to be needed is more
        // than the GPU allows or than one frame may have, or when nothing is left to grow.
        let grown = if self.fixed.is_some() {
            None
        } else {
            plan.grown(bump, self.limit)
        };
        let Some(grown) = grown else {
            self.refuse(print);
            return Verdict::TooHeavy;
        };
        self.reruns += 1;
        Verdict::Again(plan.beyond_starts(grown))
    }

    /// Keeps the room a frame was about to be run again with, when the frame is given up on
    /// before that (the GPU was too slow): the next frame starts from there, not from the
    /// beginning.
    pub(crate) fn keep(&mut self, more: ScratchSizes, now: Instant) {
        self.remember(more, now);
    }

    fn remember(&mut self, used: ScratchSizes, now: Instant) {
        if now.saturating_duration_since(self.since) >= REMEMBERED {
            // What was remembered moves back one stretch of time, or out of memory if no frame
            // was drawn for two.
            self.before = self.remembered(now);
            self.lately = ScratchSizes::default();
            self.since = now;
        }
        self.lately = self.lately.at_least(used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_LIMIT: u64 = 1 << 40;
    const PRINT: u64 = 7;

    fn room_of(wanted: ScratchSizes, scratch: &Scratch) -> ScratchRoom {
        ScratchRoom {
            wanted,
            least: wanted,
            limit: scratch.limit(),
            follows_drawing: false,
        }
    }

    fn plan_within(wanted: ScratchSizes, scratch: &Scratch) -> ScratchPlan {
        // A 3840 by 2160 target is 240 by 135 tiles.
        let layout = Layout {
            bin_data_start: 1000,
            ..Layout::default()
        };
        ScratchPlan::new(&layout, 240, 135, room_of(wanted, scratch))
    }

    fn run_again(verdict: Verdict) -> ScratchSizes {
        match verdict {
            Verdict::Again(more) => more,
            other => panic!("should run again, not {other:?}"),
        }
    }

    /// Numbers that look random and are the same every run.
    struct Numbers(u64);

    impl Numbers {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 20
        }

        /// A count that is sometimes tiny, sometimes huge.
        fn count(&mut self) -> u32 {
            let bits = self.next() % 33;
            (self.next() & ((1u64 << bits) - 1)) as u32
        }

        fn sizes(&mut self) -> ScratchSizes {
            ScratchSizes {
                lines: self.count(),
                bin_data: self.count(),
                tiles: self.count(),
                seg_counts: self.count(),
                segments: self.count(),
                blend_spill: self.count(),
                ptcl: self.count(),
            }
        }
    }

    #[test]
    fn sizes_are_rounded_up_to_what_a_pooled_buffer_holds() {
        assert_eq!(pool_class(9), 10);
        assert_eq!(pool_class(125_829_120), 134_217_728);
        assert_eq!(pool_class(100_663_296), 100_663_296);
        assert_eq!(pooled(1 << 20, 4), 1 << 20);
        assert_eq!(pooled((1 << 20) + 1, 4), 5 << 18);
        // 24-byte elements: a million of them go in a 24 MiB buffer, which holds 1 048 576.
        assert_eq!(pooled(1_000_000, 24), 1 << 20);
        assert_eq!(pooled(u32::MAX, 24), u32::MAX);
    }

    #[test]
    fn no_buffer_is_asked_to_be_larger_than_the_gpu_allows() {
        // 128 MiB, the usual limit: the engine would round 120 MiB up to 128, and 130 up to 160.
        let limit = ScratchSizes::within(128 << 20);
        assert_eq!(limit.lines, (128 << 20) / 24);
        assert_eq!(u64::from(limit.ptcl) * 4, 128 << 20);
        assert_eq!(
            u64::from(ScratchSizes::within(100 << 20).ptcl) * 4,
            96 << 20
        );
        assert_eq!(
            u64::from(ScratchSizes::within(127 << 20).ptcl) * 4,
            112 << 20
        );
        let scratch = Scratch::new(128 << 20);
        assert!(scratch.allows(3840, 2160));
        let small = Scratch::new(4 << 20);
        assert!(
            !small.allows(3840, 2160),
            "a 4K target's command lists alone are 8 MB"
        );
    }

    #[test]
    fn a_scene_of_many_path_segments_is_given_more_room_before_it_is_drawn() {
        let scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        // A song slide: about twenty thousand path segments. The resting room is enough.
        let song = Layout {
            path_tag_base: 0,
            path_data_base: 20_480 / 4,
            ..Layout::default()
        };
        assert_eq!(song.path_tags_size(), 20_480);
        let plan = ScratchPlan::new(&song, 240, 135, scratch.room(start));
        assert_eq!(plan.sizes.lines, ScratchSizes::RESTING.lines);
        // A full reading: 480 000. On a 4K screen it was measured to make 2.6 million lines and
        // 3.7 million tile segments; as a thumbnail, 1.7 million of each.
        let reading = Layout {
            path_tag_base: 0,
            path_data_base: 479_232 / 4,
            ..Layout::default()
        };
        let plan = ScratchPlan::new(&reading, 240, 135, scratch.room(start));
        assert!(
            plan.sizes.lines > 2_600_000 && plan.sizes.segments > 3_700_000,
            "{:?}",
            plan.sizes
        );
        assert!(
            plan.sizes.segments <= ScratchSizes::RESTING.segments * 8,
            "the guess has an end"
        );
        let thumb = ScratchPlan::new(&reading, 30, 17, scratch.room(start));
        assert!(thumb.sizes.lines > 1_750_000, "{:?}", thumb.sizes);
        assert!(thumb.sizes.lines < plan.sizes.lines, "{:?}", thumb.sizes);
        // Sizes told to stay fixed are not guessed at.
        let mut fixed = Scratch::new(NO_LIMIT);
        fixed.set_fixed(Some(ScratchSizes::RESTING));
        let plan = ScratchPlan::new(&reading, 240, 135, fixed.room(start));
        assert_eq!(plan.sizes.lines, ScratchSizes::RESTING.lines);
    }

    #[test]
    fn a_plan_never_goes_under_what_the_target_and_scene_take_anyway() {
        let scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(ScratchSizes::default(), &scratch);
        assert_eq!(plan.sizes.ptcl, 240 * 135 * 64 + 256);
        assert_eq!(plan.sizes.bin_data, 1001);
        assert!(plan.sizes.lines >= 1 && plan.sizes.tiles >= 1 && plan.sizes.blend_spill >= 1);
    }

    #[test]
    fn a_frame_that_fits_is_remembered_and_then_forgotten() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let plan = plan_within(scratch.wanted(start), &scratch);
        assert!(!scratch.is_heavy(start));
        let heavy = ScratchSizes::RESTING.lines - 1;
        let bump = BumpAllocators {
            lines: heavy,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT, start),
            Verdict::Fits
        ));
        let next = scratch.wanted(start).lines;
        assert!(
            next >= heavy / 4 * 5,
            "the next frame gets the room and some spare"
        );
        assert!(scratch.is_heavy(start));
        // Light frames a while later: the heavy one is kept for one stretch, then dropped.
        let light = BumpAllocators::default();
        let later = start + REMEMBERED;
        assert!(matches!(
            scratch.judge(&plan, &light, PRINT, later),
            Verdict::Fits
        ));
        assert!(scratch.wanted(later).lines > ScratchSizes::RESTING.lines);
        let later = start + REMEMBERED * 2;
        assert!(matches!(
            scratch.judge(&plan, &light, PRINT, later),
            Verdict::Fits
        ));
        assert_eq!(scratch.wanted(later), ScratchSizes::RESTING);
    }

    #[test]
    fn a_heavy_frame_is_forgotten_by_the_clock_alone_when_nothing_more_is_drawn() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let plan = plan_within(scratch.wanted(start), &scratch);
        let lines = ScratchSizes::RESTING.lines - 1;
        let bump = BumpAllocators {
            lines,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT, start),
            Verdict::Fits
        ));
        assert!(scratch.is_heavy(start + REMEMBERED));
        assert!(!scratch.is_heavy(start + REMEMBERED * 2));
        assert_eq!(
            scratch.wanted(start + REMEMBERED * 2),
            ScratchSizes::RESTING
        );
    }

    #[test]
    fn a_frame_that_ran_out_is_run_again_with_room_for_what_it_asked() {
        let start = Instant::now();
        let mut scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(scratch.wanted(start), &scratch);
        // Flattening made five times the lines there was room for; nothing after it ran.
        let asked = plan.sizes.lines * 5;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines: asked,
            ..BumpAllocators::default()
        };
        let more = run_again(scratch.judge(&plan, &bump, PRINT, start));
        let again = ScratchPlan::new(
            &Layout::default(),
            240,
            135,
            scratch.room_to_run_again(more),
        );
        assert!(again.sizes.lines >= asked);
        assert!(
            again.sizes.tiles >= plan.sizes.tiles * 2,
            "later stages could not say: doubled"
        );
        assert!(again.sizes.ptcl >= plan.sizes.ptcl * 2);
        assert_eq!(scratch.reruns(), 1);
    }

    #[test]
    fn a_stage_flagged_without_a_count_past_its_room_is_given_more_all_the_same() {
        let start = Instant::now();
        let mut scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(scratch.wanted(start), &scratch);
        let bump = BumpAllocators {
            failed: STAGE_TILE_ALLOC,
            ..BumpAllocators::default()
        };
        let more = run_again(scratch.judge(&plan, &bump, PRINT, start));
        assert!(more.tiles >= plan.sizes.tiles * 2, "the flagged stage");
        assert!(
            more.segments >= plan.sizes.segments * 2,
            "and those after it"
        );
        assert_eq!(
            more.lines, plan.sizes.lines,
            "earlier stages fitted and keep their sizes"
        );
    }

    #[test]
    fn segments_past_their_buffer_are_caught_though_no_stage_flags_them() {
        let start = Instant::now();
        let mut scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(scratch.wanted(start), &scratch);
        let segments = plan.sizes.segments + 1;
        let bump = BumpAllocators {
            segments,
            ..BumpAllocators::default()
        };
        let more = run_again(scratch.judge(&plan, &bump, PRINT, start));
        assert!(more.segments > plan.sizes.segments);
        assert_eq!(
            more.lines, plan.sizes.lines,
            "earlier stages fitted and keep their sizes"
        );
    }

    #[test]
    fn a_frame_too_heavy_for_the_gpu_or_for_fixed_sizes_is_refused() {
        let start = Instant::now();
        let mut scratch = Scratch::new(64 << 20);
        let plan = plan_within(scratch.wanted(start), &scratch);
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines: 1 << 24,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT, start),
            Verdict::TooHeavy
        ));
        // Needing a little more than the spare allows, but no more than the GPU does, is fine.
        let most = scratch.limit().lines;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines: most - 1,
            ..BumpAllocators::default()
        };
        let more = run_again(scratch.judge(&plan, &bump, PRINT + 1, start));
        assert_eq!(more.lines, most);

        let mut fixed = Scratch::new(NO_LIMIT);
        fixed.set_fixed(Some(ScratchSizes::RESTING));
        let plan = plan_within(fixed.wanted(start), &fixed);
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines: 1 << 20,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            fixed.judge(&plan, &bump, PRINT, start),
            Verdict::TooHeavy
        ));
    }

    #[test]
    fn a_scene_that_fitted_before_is_not_waited_for_again() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let plan = plan_within(scratch.wanted(start), &scratch);
        assert!(!scratch.fitted_before(PRINT, &plan, start), "never seen");
        let bump = BumpAllocators {
            lines: 5_000,
            tile: 700,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT, start),
            Verdict::Fits
        ));
        assert!(scratch.fitted_before(PRINT, &plan, start));
        assert!(
            !scratch.fitted_before(PRINT + 1, &plan, start),
            "another scene is still checked"
        );
        assert_eq!(scratch.unwaited(), 1);
        // Given less room than it used, it is checked again.
        let tight = ScratchSizes {
            lines: 4_000,
            ..ScratchSizes::RESTING
        };
        assert!(!scratch.fitted_before(PRINT, &plan_within(tight, &scratch), start));
    }

    #[test]
    fn only_the_newest_few_scenes_that_fitted_are_kept_in_mind() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let plan = plan_within(scratch.wanted(start), &scratch);
        let bump = BumpAllocators {
            lines: 5_000,
            tile: 700,
            ..BumpAllocators::default()
        };
        let scenes = 3 * FITTED_KEPT as u64;
        for print in 0..scenes {
            assert!(matches!(
                scratch.judge(&plan, &bump, print, start),
                Verdict::Fits
            ));
        }
        assert_eq!(
            scratch.fitted.len(),
            FITTED_KEPT,
            "the memory does not grow"
        );
        let forgotten = scenes - FITTED_KEPT as u64;
        for print in 0..scenes {
            let known = scratch.fitted_before(print, &plan, start);
            assert_eq!(known, print >= forgotten, "scene {print}");
        }
        // A forgotten scene is judged afresh, and fits as it did.
        assert!(matches!(
            scratch.judge(&plan, &bump, 0, start),
            Verdict::Fits
        ));
        assert!(scratch.fitted_before(0, &plan, start));
        assert!(
            !scratch.fitted_before(forgotten, &plan, start),
            "the oldest made way for it"
        );
        // One scene drawn again and again takes one place, not all of them.
        for _ in 0..3 * FITTED_KEPT {
            assert!(matches!(
                scratch.judge(&plan, &bump, 0, start),
                Verdict::Fits
            ));
        }
        assert!(scratch.fitted_before(scenes - 1, &plan, start));
    }

    #[test]
    fn a_frame_is_never_given_more_than_the_most_one_frame_may_have() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        // Room close to the most a frame may have, and a few more lines than fit.
        let had = ScratchSizes {
            lines: 8_000_000,
            seg_counts: 16_000_000,
            segments: 16_000_000,
            ..ScratchSizes::RESTING
        };
        let plan = plan_within(had, &scratch);
        assert!(plan.sizes.bytes() < MOST_BYTES, "{}", plan.sizes.bytes());
        let lines = 9_000_000;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines,
            ..BumpAllocators::default()
        };
        // Doubling the stages after flattening would pass the most: only the lines grow.
        let more = run_again(scratch.judge(&plan, &bump, PRINT, start));
        assert!(more.lines >= 9_000_000);
        assert_eq!(
            more.segments, had.segments,
            "the later stages are not doubled past the most"
        );
        let again = ScratchPlan::new(
            &Layout::default(),
            240,
            135,
            scratch.room_to_run_again(more),
        );
        assert!(again.within_the_most(), "{}", again.sizes.bytes());
        assert!(
            again.sizes.lines >= 9_000_000,
            "and the plan made from it has the room"
        );
        // Lines alone past the most: refused.
        let lines = 50_000_000;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT, start),
            Verdict::TooHeavy
        ));
    }

    #[test]
    fn the_first_run_of_a_frame_keeps_within_the_most_one_frame_may_have_too() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        // A frame that took nine tenths of the most one frame may have, and fitted.
        let heavy = ScratchSizes {
            lines: 36_000_000,
            ..ScratchSizes::RESTING
        };
        let plan = plan_within(heavy, &scratch);
        assert!(plan.within_the_most());
        let bump = BumpAllocators {
            lines: heavy.lines,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT, start),
            Verdict::Fits
        ));
        // With the usual spare the next frame would pass the most...
        let wanted = scratch.wanted(start);
        assert!(plan_within(wanted, &scratch).sizes.bytes() > MOST_BYTES);
        // ...so it is given what the last one used, which fits, and no more.
        let next = ScratchPlan::new(&Layout::default(), 240, 135, scratch.room(start));
        assert!(next.within_the_most(), "{}", next.sizes.bytes());
        assert!(next.sizes.lines >= heavy.lines);
        // The guess for a scene of many path segments is dropped the same way.
        let reading = Layout {
            path_tag_base: 0,
            path_data_base: 479_232 / 4,
            ..Layout::default()
        };
        let guessed = ScratchPlan::new(&reading, 240, 135, scratch.room(start));
        assert!(guessed.within_the_most(), "{}", guessed.sizes.bytes());
        assert!(guessed.sizes.lines >= heavy.lines);
        // Fixed sizes past the most are not run at all.
        let mut fixed = Scratch::new(NO_LIMIT);
        fixed.set_fixed(Some(ScratchSizes {
            lines: 60_000_000,
            ..ScratchSizes::RESTING
        }));
        let plan = ScratchPlan::new(&Layout::default(), 240, 135, fixed.room(start));
        assert!(!plan.within_the_most());
    }

    #[test]
    fn room_left_by_another_scene_never_gets_a_scene_refused() {
        let start = Instant::now();
        // Run again: the frame has a great deal of command list room, left by a scene that
        // needed it, and itself needs a great many lines. Both together would pass the most
        // one frame may have; the lines alone do not.
        let mut scratch = Scratch::new(NO_LIMIT);
        let had = ScratchSizes {
            ptcl: 200_000_000,
            ..ScratchSizes::RESTING
        };
        let plan = plan_within(had, &scratch);
        assert!(plan.within_the_most(), "{}", plan.sizes.bytes());
        let lines = 16_000_000;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines,
            ..BumpAllocators::default()
        };
        let more = run_again(scratch.judge(&plan, &bump, PRINT, start));
        assert!(
            !scratch.refused_before(PRINT),
            "it is not too heavy: the room it had was"
        );
        let again = ScratchPlan::new(
            &Layout::default(),
            240,
            135,
            scratch.room_to_run_again(more),
        );
        assert!(again.within_the_most(), "{}", again.sizes.bytes());
        assert!(again.sizes.lines >= lines);
        assert!(
            again.sizes.ptcl < plan.sizes.ptcl,
            "the room the other scene left is let go"
        );
        // Truly too heavy by itself: refused, and remembered as that.
        let lines = 50_000_000;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT + 1, start),
            Verdict::TooHeavy
        ));
        assert!(scratch.refused_before(PRINT + 1));

        // First run: two heavy scenes of different shapes drawn within a few seconds. What
        // is remembered is the most of each buffer, which together would pass the most one
        // frame may have; a frame drawn next is given the resting room, not refused.
        let mut scratch = Scratch::new(NO_LIMIT);
        let many_lines = ScratchSizes {
            lines: 30_000_000,
            ..ScratchSizes::RESTING
        };
        let plan = plan_within(many_lines, &scratch);
        let bump = BumpAllocators {
            lines: 30_000_000,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, 1, start),
            Verdict::Fits
        ));
        let long_lists = ScratchSizes {
            ptcl: 180_000_000,
            ..ScratchSizes::RESTING
        };
        let plan = plan_within(long_lists, &scratch);
        let bump = BumpAllocators {
            ptcl: 180_000_000,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, 2, start),
            Verdict::Fits
        ));
        let room = scratch.room(start);
        assert!(
            plan_within(room.least, &scratch).sizes.bytes() > MOST_BYTES,
            "both together"
        );
        let next = ScratchPlan::new(&Layout::default(), 240, 135, room);
        assert!(next.within_the_most(), "{}", next.sizes.bytes());
        assert!(next.sizes.hold(&ScratchSizes::RESTING), "{:?}", next.sizes);
        // Sizes told to stay fixed are never changed, so they have no such way out.
        let mut fixed = Scratch::new(NO_LIMIT);
        fixed.set_fixed(Some(ScratchSizes {
            lines: 60_000_000,
            ..ScratchSizes::RESTING
        }));
        let plan = ScratchPlan::new(&Layout::default(), 240, 135, fixed.room(start));
        assert!(!plan.within_the_most());
    }

    #[test]
    fn segments_always_have_room_for_every_segment_count() {
        let mut numbers = Numbers(11);
        let start = Instant::now();
        let mut run_again = 0;
        for round in 0..20_000 {
            // GPUs that allow buffers from a megabyte to more than anyone has.
            let mut scratch = Scratch::new(1 << (20 + numbers.next() % 21));
            let limit = scratch.limit();
            let layout = Layout {
                bin_data_start: numbers.count() % (1 << 16),
                path_tag_base: 0,
                path_data_base: numbers.count() % (1 << 20),
                ..Layout::default()
            };
            let (width, height) = (1 + numbers.count() % 512, 1 + numbers.count() % 512);
            let room = ScratchRoom {
                wanted: numbers.sizes(),
                least: numbers.sizes(),
                limit,
                follows_drawing: round % 2 == 0,
            };
            let plan = ScratchPlan::new(&layout, width, height, room);
            let sizes = plan.sizes;
            assert!(
                sizes.segments >= sizes.seg_counts,
                "round {round}: {sizes:?}"
            );
            assert!(
                sizes.segments >= 1 && sizes.seg_counts >= 1,
                "round {round}: {sizes:?}"
            );
            assert!(sizes.segments <= limit.segments, "round {round}: {sizes:?}");
            // And for whatever the counters of that frame say, the sizes to run it again with.
            let bump = BumpAllocators {
                failed: (numbers.next() % 32) as u32,
                binning: numbers.count(),
                ptcl: numbers.count(),
                tile: numbers.count(),
                seg_counts: numbers.count(),
                segments: numbers.count(),
                blend: numbers.count(),
                lines: numbers.count(),
            };
            let Verdict::Again(more) = scratch.judge(&plan, &bump, PRINT, start) else {
                continue;
            };
            run_again += 1;
            let again = ScratchPlan::new(&layout, width, height, scratch.room_to_run_again(more));
            let sizes = again.sizes;
            assert!(
                sizes.segments >= sizes.seg_counts,
                "round {round}, again: {sizes:?}"
            );
            assert!(again.within_the_most(), "round {round}, again: {sizes:?}");
            assert_ne!(
                sizes, plan.sizes,
                "round {round}: a frame run again has other room"
            );
            assert!(
                sizes.hold(&plan.used(&bump)),
                "round {round}: all that the counters asked"
            );
        }
        assert!(
            run_again > 500,
            "enough of the frames were run again to say so: {run_again}"
        );
    }

    #[test]
    fn counters_the_gpu_never_wrote_are_not_taken_for_a_frame_that_fitted() {
        // A buffer as it is made, before the GPU copies the counters into it.
        let unwritten = [UNREAD; size_of::<BumpAllocators>()];
        assert!(counters_read_back(&unwritten).is_none());
        // Too few bytes, or flags no stage has.
        assert!(counters_read_back(&unwritten[..8]).is_none());
        let odd = BumpAllocators {
            failed: 0x20,
            ..BumpAllocators::default()
        };
        assert!(counters_read_back(bytemuck::bytes_of(&odd)).is_none());
        // Counters the stages wrote come back as they are, whether the frame fitted or not.
        let fitted = BumpAllocators {
            lines: 12,
            tile: 5,
            ..BumpAllocators::default()
        };
        let read = counters_read_back(bytemuck::bytes_of(&fitted)).expect("written");
        assert_eq!((read.failed, read.lines, read.tile), (0, 12, 5));
        let short = BumpAllocators {
            failed: EVERY_STAGE,
            lines: u32::MAX,
            ..fitted
        };
        let read = counters_read_back(bytemuck::bytes_of(&short)).expect("written");
        assert_eq!((read.failed, read.lines), (EVERY_STAGE, u32::MAX));
    }

    /// The same on a real GPU: the buffer the counters are copied into, read back without the
    /// copy ever being sent, says that it was never written.
    #[test]
    fn a_read_back_buffer_the_gpu_never_filled_says_so() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        fn now<F: Future>(future: F) -> F::Output {
            let mut future = std::pin::pin!(future);
            match future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(output) => output,
                Poll::Pending => panic!("the GPU library answers at once on a computer"),
            }
        }
        let instance = wgpu::Instance::default();
        let Ok(adapter) = now(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            return eprintln!("no GPU here: not checked");
        };
        let (device, _queue) =
            now(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("a device");
        let size = size_of::<BumpAllocators>() as u64;
        let buffer = crate::wgpu_engine::unread_download(&device, size).expect("a buffer");
        let slice = buffer.slice(..size);
        slice.map_async(wgpu::MapMode::Read, |mapped| mapped.expect("mapped"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let bytes = slice.get_mapped_range().expect("mapped");
        assert!(bytes.iter().all(|byte| *byte == UNREAD));
        assert!(counters_read_back(&bytes).is_none());
    }

    #[test]
    fn scenes_that_differ_anywhere_have_different_fingerprints() {
        use std::collections::HashSet;
        let mut numbers = Numbers(5);
        let scene: Vec<u8> = (0..1000).map(|_| numbers.next() as u8).collect();
        let mut seen = HashSet::new();
        // Every length up to a thousand bytes, so every way a scene can end part-way through
        // the 48 bytes read at a time; and the same bytes with zeros after them.
        for length in 0..=scene.len() {
            assert!(
                seen.insert(fingerprint(&scene[..length], 1920, 1080)),
                "length {length}"
            );
            let mut padded = scene[..length].to_vec();
            padded.push(0);
            assert!(
                seen.insert(fingerprint(&padded, 0, 0)),
                "length {length} and a zero"
            );
        }
        // Any one bit changed, anywhere.
        for at in 0..scene.len() {
            let mut changed = scene.clone();
            changed[at] ^= 1 << (at % 8);
            assert!(
                seen.insert(fingerprint(&changed, 1920, 1080)),
                "byte {at} changed"
            );
        }
        // The same bytes in another order, and scenes of nothing but zeros.
        let mut swapped = scene.clone();
        swapped.swap(100, 148);
        assert!(seen.insert(fingerprint(&swapped, 1920, 1080)));
        for length in 1..200 {
            assert!(
                seen.insert(fingerprint(&vec![0; length], 1920, 1080)),
                "{length} zeros"
            );
        }
        // The same scene on a target of another size.
        assert!(seen.insert(fingerprint(&scene, 1080, 1920)));
        assert!(seen.insert(fingerprint(&scene, 1920, 1081)));
        // And the same scene at the same size is the same every time.
        assert_eq!(
            fingerprint(&scene, 1920, 1080),
            fingerprint(&scene.clone(), 1920, 1080)
        );
    }

    /// The renderer must only ever ask the device how far the GPU has got, never wait in it:
    /// a wait with a time limit, while another thread uses the device, trips an assertion in
    /// wgpu 30 and takes the thread down (seen within five hundred waits when tried), and any
    /// wait holds up that other thread. No test of drawing meets that timing reliably, so
    /// this looks at the source.
    #[test]
    fn the_renderer_never_waits_inside_the_device() {
        let sources = [
            ("lib.rs", include_str!("lib.rs")),
            ("render.rs", include_str!("render.rs")),
            ("wgpu_engine.rs", include_str!("wgpu_engine.rs")),
            ("recording.rs", include_str!("recording.rs")),
        ];
        for (file, source) in sources {
            for waiting in ["PollType::Wait", "wait_indefinitely", "PollType::wait"] {
                assert!(!source.contains(waiting), "{file} has `{waiting}`");
            }
        }
    }

    #[test]
    fn a_scene_found_too_heavy_is_remembered_as_that() {
        let start = Instant::now();
        let mut scratch = Scratch::new(64 << 20);
        let plan = plan_within(scratch.wanted(start), &scratch);
        assert!(!scratch.refused_before(PRINT));
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines: 1 << 24,
            ..BumpAllocators::default()
        };
        assert!(matches!(
            scratch.judge(&plan, &bump, PRINT, start),
            Verdict::TooHeavy
        ));
        assert!(scratch.refused_before(PRINT));
        assert!(
            !scratch.refused_before(PRINT + 1),
            "another scene is still tried"
        );
        assert!(
            !scratch.fitted_before(PRINT, &plan, start),
            "and it never counts as fitting"
        );
        // A frame run again is not refused, and one that fits is not either.
        let lines = plan.sizes.lines * 2;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines,
            ..BumpAllocators::default()
        };
        run_again(scratch.judge(&plan, &bump, 50, start));
        assert!(matches!(
            scratch.judge(&plan, &BumpAllocators::default(), 51, start),
            Verdict::Fits
        ));
        assert!(!scratch.refused_before(50) && !scratch.refused_before(51));
        // Only the newest few are kept.
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines: 1 << 24,
            ..BumpAllocators::default()
        };
        for print in 100..100 + 3 * REFUSED_KEPT as u64 {
            assert!(matches!(
                scratch.judge(&plan, &bump, print, start),
                Verdict::TooHeavy
            ));
        }
        assert_eq!(scratch.refused.len(), REFUSED_KEPT);
        assert!(!scratch.refused_before(PRINT));
        // Other sizes may hold what these did not: nothing stays refused.
        scratch.set_fixed(Some(ScratchSizes::UPSTREAM));
        assert!(!scratch.refused_before(100 + 3 * REFUSED_KEPT as u64 - 1));
    }

    #[test]
    fn room_found_for_a_frame_given_up_on_is_where_the_next_one_starts() {
        let start = Instant::now();
        let mut scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(scratch.wanted(start), &scratch);
        let lines = plan.sizes.lines * 5;
        let bump = BumpAllocators {
            failed: STAGE_FLATTEN,
            lines,
            ..BumpAllocators::default()
        };
        let more = run_again(scratch.judge(&plan, &bump, PRINT, start));
        scratch.keep(more, start);
        let next = ScratchPlan::new(&Layout::default(), 240, 135, scratch.room(start));
        assert!(next.sizes.lines >= lines, "{:?}", next.sizes);
    }

    #[test]
    fn a_resting_buffer_is_as_large_as_the_largest_target_drawn_lately_makes_it() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let room = scratch.room(start);
        let on = |width: u32, height: u32| {
            ScratchPlan::new(
                &Layout::default(),
                width.div_ceil(16),
                height.div_ceil(16),
                room,
            )
        };
        // Up to 4K the lines and segments are the largest: 12 MiB each.
        scratch.drawing(&on(480, 270), start);
        assert_eq!(scratch.largest_resting_buffer(), 12 << 20);
        scratch.drawing(&on(3840, 2160), start);
        assert_eq!(scratch.largest_resting_buffer(), 12 << 20);
        // On an 8K target the tiles' own command lists are 33 MB: that buffer is not "large".
        scratch.drawing(&on(7680, 4320), start);
        let on_8k = scratch.largest_resting_buffer();
        assert!(on_8k >= (7680 / 16) * (4320 / 16) * 64 * 4, "{on_8k}");
        // A smaller target drawn in turn with it changes nothing...
        scratch.drawing(&on(480, 270), start + REMEMBERED);
        assert_eq!(scratch.largest_resting_buffer(), on_8k);
        // ...until the large one has not been drawn for a while.
        scratch.drawing(&on(480, 270), start + REMEMBERED * 2);
        assert_eq!(scratch.largest_resting_buffer(), 12 << 20);
    }
}
