//! A video's sound, as the system's player is about to play it, handed to the mix for the
//! screens that send sound (see [`crate::sound`]).
//!
//! The system's player puts a video's sound out itself, so there is no output of the app's to
//! tap. The player has its own way to listen in: an audio processing tap on the item's audio
//! mix, which the player calls with each piece of sound it has decoded, a moment before it is
//! played. The tap changes nothing (the samples go on as they came) and copies them for the
//! mix while anybody wants them. Each piece comes with its time in the video; set beside the
//! item's own clock, that says how far ahead of the speakers it is, and so when it is heard.
//!
//! The tap hears the sound before the player's volume, so the player's volume is given to the
//! mix as well ([`Tapped::set_gain`]); and what was handed over ahead of a pause or a seek is
//! taken back ([`Tapped::flush`]).

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2_av_foundation::{
    AVAudioMixInputParameters, AVMediaTypeAudio, AVMutableAudioMix, AVMutableAudioMixInputParameters, AVPlayerItem,
};
use objc2_core_audio_types::{
    AudioBufferList, AudioStreamBasicDescription, kAudioFormatFlagIsFloat, kAudioFormatFlagIsNonInterleaved,
    kAudioFormatLinearPCM,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{CMItemCount, CMTimeRange, CMTimebase, kCMTimeZero};
use objc2_foundation::NSArray;
use objc2_media_toolbox::{
    MTAudioProcessingTap, MTAudioProcessingTapCallbacks, MTAudioProcessingTapFlags,
    kMTAudioProcessingTapCallbacksVersion_0, kMTAudioProcessingTapCreationFlag_PostEffects,
};

use crate::sound::Feed;

/// How long after the player plays a sample it counts as heard, for the mix: the pictures
/// reach NDI a little after they are drawn, and a block can't go out before it is full.
/// Measured with a clip that beeps on a flash and sp-ndi's `sync` example: at this, the beep
/// reaches a receiver within a picture and a half of its flash, after it more often than
/// before (how far differed each time the video started, was paused or moved, and then
/// stayed; most likely by where the player's pictures fall against the screens' drawing).
const PLAYER_DELAY: Duration = Duration::from_millis(20);

/// The player is never taken to be further ahead of the speakers than this.
const MOST_AHEAD: f64 = 2.0;

/// The samples' layout the tap was told to expect: none it can read, 32-bit floats with each
/// channel in a buffer of its own, or 32-bit floats with the channels side by side in one.
const UNREADABLE: u32 = 0;
const APART: u32 = 1;
const TOGETHER: u32 = 2;

/// The item's clock, read on the thread the tap is called on.
struct Clock(Retained<CMTimebase>);

// SAFETY: a timebase is a Core Media object made to be read from any thread (the system's own
// rendering threads read the very same one), and its reference count may be changed from any
// thread. It is only read here: its time and its rate.
unsafe impl Send for Clock {}
// SAFETY: as above.
unsafe impl Sync for Clock {}

/// What one tap keeps: made when the tap is, freed when the system has finished with the tap.
struct Storage {
    feed: Arc<Feed>,
    clock: Option<Clock>,
    /// True once the system has taken this in (its `init` call): it is then freed by `finalize`.
    adopted: AtomicBool,
    layout: AtomicU32,
    rate: AtomicU32,
    channels: AtomicU32,
    /// Samples handed over, and the loudest of the second being counted (an `f32`'s bits,
    /// which for sizes without a sign are in the order of the sizes), for the sync trace.
    traced: AtomicU64,
    loudest: AtomicU32,
}

/// The sound of a video the system's player plays, tapped for the mix: one tap for each of its
/// audio tracks (the player plays one of them, or mixes them).
pub(super) struct Tapped {
    feeds: Vec<Arc<Feed>>,
}

impl Tapped {
    /// Taps the item's sound, before it is given to a player. `None` for a video with no
    /// sound, or if the taps could not be made (the video then plays as ever, and the screens
    /// that send sound are sent none of it).
    pub(super) fn attach(item: &AVPlayerItem) -> Option<Tapped> {
        // SAFETY: reads a static the framework defines.
        let audio = unsafe { AVMediaTypeAudio }?;
        // The same synchronous reading of the file's header as for its pictures' size.
        #[allow(deprecated)]
        // SAFETY: plain getters on live objects.
        let tracks = unsafe { item.asset().tracksWithMediaType(audio) };
        let mut feeds = Vec::new();
        let mut inputs: Vec<Retained<AVAudioMixInputParameters>> = Vec::new();
        for track in tracks.iter() {
            let feed = Arc::new(Feed::new());
            // SAFETY: a plain getter on a live object.
            let clock = unsafe { item.timebase() }.map(Clock);
            let storage = Box::into_raw(Box::new(Storage {
                feed: feed.clone(),
                clock,
                adopted: AtomicBool::new(false),
                layout: AtomicU32::new(UNREADABLE),
                rate: AtomicU32::new(0),
                channels: AtomicU32::new(0),
                traced: AtomicU64::new(0),
                loudest: AtomicU32::new(0),
            }));
            let mut callbacks = MTAudioProcessingTapCallbacks {
                version: kMTAudioProcessingTapCallbacksVersion_0,
                clientInfo: storage.cast(),
                init: Some(init),
                finalize: Some(finalize),
                prepare: Some(prepare),
                unprepare: Some(unprepare),
                process: Some(process),
            };
            let mut made: *const MTAudioProcessingTap = std::ptr::null();
            // SAFETY: the callbacks are a valid structure for the length of the call (the
            // system copies it), each a function of the type its place asks for; `made` is
            // written with the new tap. `clientInfo` is the storage, which `init` hands back
            // to the system to keep until `finalize`.
            let status = unsafe {
                MTAudioProcessingTap::create(
                    None,
                    NonNull::from(&mut callbacks),
                    kMTAudioProcessingTapCreationFlag_PostEffects,
                    NonNull::from(&mut made),
                )
            };
            let Some(made) = NonNull::new(made.cast_mut()).filter(|_| status == 0) else {
                // SAFETY: the storage came from Box::into_raw just above. The system frees it
                // (in `finalize`) only once it has taken it in (`init`); it has not, so it is
                // still this function's to free, once.
                if !unsafe { &*storage }.adopted.load(Ordering::Relaxed) {
                    // SAFETY: as above.
                    drop(unsafe { Box::from_raw(storage) });
                }
                continue;
            };
            // SAFETY: MTAudioProcessingTapCreate follows the create rule: the tap it wrote is
            // owned by the caller, who lets it go once (when this is dropped; the input
            // parameters keep their own hold on it).
            let tap = unsafe { CFRetained::from_raw(made) };
            // SAFETY: plain calls on live objects.
            let input = unsafe {
                let input = AVMutableAudioMixInputParameters::audioMixInputParametersWithTrack(Some(&track));
                input.setAudioTapProcessor(Some(&tap));
                input
            };
            inputs.push(Retained::into_super(input));
            feeds.push(feed);
        }
        if feeds.is_empty() {
            return None;
        }
        // SAFETY: plain calls on live objects; the mix is copied by the item.
        unsafe {
            let mix = AVMutableAudioMix::audioMix();
            mix.setInputParameters(&NSArray::from_retained_slice(&inputs));
            item.setAudioMix(Some(&mix));
        }
        Some(Tapped { feeds })
    }

    /// The player's volume, which the tap doesn't hear: 0 (silent) to 1.
    pub(super) fn set_gain(&self, gain: f32) {
        for feed in &self.feeds {
            feed.set_gain(gain);
        }
    }

    /// The player was paused or moved: what it had decoded ahead will not be played.
    pub(super) fn flush(&self) {
        for feed in &self.feeds {
            feed.flush();
        }
    }
}

/// A tap's storage, from the tap.
///
/// # Safety
/// The tap must be one made by [`Tapped::attach`], between its `init` and its `finalize`.
unsafe fn storage_of<'a>(tap: NonNull<MTAudioProcessingTap>) -> &'a Storage {
    // SAFETY: `init` gave the system the pointer to the `Storage` made with the tap, which
    // lives until `finalize`; nothing changes it but through its atomics.
    unsafe { &*tap.as_ref().storage().as_ptr().cast::<Storage>() }
}

/// The system takes the tap in: it is given the storage to keep.
unsafe extern "C-unwind" fn init(
    _tap: NonNull<MTAudioProcessingTap>,
    client: *mut c_void,
    storage_out: NonNull<*mut c_void>,
) {
    // SAFETY: `client` is the `Storage` handed to MTAudioProcessingTapCreate, still alive (it
    // is freed only by `finalize`, or by `attach` if this was never called); `storage_out` is
    // the system's place for the tap's storage, valid to write.
    unsafe {
        (*client.cast::<Storage>()).adopted.store(true, Ordering::Relaxed);
        storage_out.write(client);
    }
}

/// The system has finished with the tap: its storage is freed.
unsafe extern "C-unwind" fn finalize(tap: NonNull<MTAudioProcessingTap>) {
    // SAFETY: the storage is the box `attach` made and `init` handed over; the system calls
    // this once, after the last of the tap's other calls, so nothing else has it.
    drop(unsafe { Box::from_raw(tap.as_ref().storage().as_ptr().cast::<Storage>()) });
}

/// The system says what the samples will look like.
unsafe extern "C-unwind" fn prepare(
    tap: NonNull<MTAudioProcessingTap>,
    _most_frames: CMItemCount,
    format: NonNull<AudioStreamBasicDescription>,
) {
    // SAFETY: the system calls this for a tap it has taken in and not yet finished with.
    let storage = unsafe { storage_of(tap) };
    // SAFETY: the format is a valid description for the length of the call; it is copied.
    let format = unsafe { format.read() };
    let floats = format.mFormatID == kAudioFormatLinearPCM
        && format.mFormatFlags & kAudioFormatFlagIsFloat != 0
        && format.mBitsPerChannel == 32;
    let layout = match (floats, format.mFormatFlags & kAudioFormatFlagIsNonInterleaved != 0) {
        (false, _) => UNREADABLE,
        (true, true) => APART,
        (true, false) => TOGETHER,
    };
    storage.rate.store(format.mSampleRate.round().clamp(0.0, f64::from(u32::MAX)) as u32, Ordering::Relaxed);
    storage.channels.store(format.mChannelsPerFrame, Ordering::Relaxed);
    storage.layout.store(layout, Ordering::Relaxed);
}

unsafe extern "C-unwind" fn unprepare(_tap: NonNull<MTAudioProcessingTap>) {}

/// The system hands over a piece of sound on its way to being played: it is passed on as it
/// is, and copied for the mix while anybody wants it. Called on the system's sound thread:
/// nothing here waits.
unsafe extern "C-unwind" fn process(
    tap: NonNull<MTAudioProcessingTap>,
    frames: CMItemCount,
    _flags: MTAudioProcessingTapFlags,
    buffers: NonNull<AudioBufferList>,
    frames_out: NonNull<CMItemCount>,
    flags_out: NonNull<MTAudioProcessingTapFlags>,
) {
    let mut range = CMTimeRange {
        // SAFETY: reads a static the framework defines.
        start: unsafe { kCMTimeZero },
        // SAFETY: as above.
        duration: unsafe { kCMTimeZero },
    };
    // SAFETY: the tap is the one being called; the buffers, the flags and the count are the
    // system's own, valid for the length of this call, and `range` is written. This is the
    // one way a tap gets its samples: they are put in `buffers`, which then also goes on to
    // be played.
    let status =
        unsafe { tap.as_ref().source_audio(frames, buffers, flags_out.as_ptr(), &mut range, frames_out.as_ptr()) };
    if status != 0 {
        // SAFETY: the system's place for how many samples there are, valid to write.
        unsafe { frames_out.write(0) };
        return;
    }
    if !Feed::wanted() {
        return;
    }
    // SAFETY: the system calls this for a tap it has taken in and not yet finished with.
    let storage = unsafe { storage_of(tap) };
    let (layout, rate) = (storage.layout.load(Ordering::Relaxed), storage.rate.load(Ordering::Relaxed));
    let Ok(channels) = u16::try_from(storage.channels.load(Ordering::Relaxed)) else { return };
    // SAFETY: written by the call above.
    let got = usize::try_from(unsafe { frames_out.read() }).unwrap_or(0);
    if layout == UNREADABLE || rate == 0 || channels == 0 || got == 0 {
        return;
    }
    // SAFETY: an AudioBufferList is its count followed by that many buffers, laid out one
    // after another from `mBuffers`; the system filled this one for this call.
    let list = unsafe {
        let list = buffers.as_ptr();
        std::slice::from_raw_parts((&raw const (*list).mBuffers).cast::<objc2_core_audio_types::AudioBuffer>(), {
            (*list).mNumberBuffers as usize
        })
    };
    // Each buffer's samples: as many 32-bit floats as its size says, where the system put them.
    let floats = |at: usize| -> Option<(*const f32, usize)> {
        let buffer = list.get(at)?;
        (!buffer.mData.is_null())
            .then_some((buffer.mData.cast::<f32>().cast_const(), buffer.mDataByteSize as usize / 4))
    };
    let mut samples = Vec::with_capacity(got * usize::from(channels));
    if layout == APART {
        let Some(each): Option<Vec<(*const f32, usize)>> = (0..usize::from(channels)).map(floats).collect() else {
            return;
        };
        if each.iter().any(|(_, len)| *len < got) {
            return;
        }
        for frame in 0..got {
            for (data, _) in &each {
                // SAFETY: `frame` is below `got`, which is no more than the floats this buffer
                // holds (checked above); the system keeps the buffer for this call. Read
                // unaligned, since nothing says where the system put it.
                samples.push(unsafe { data.add(frame).read_unaligned() });
            }
        }
    } else {
        let Some((data, len)) = floats(0) else { return };
        let count = got * usize::from(channels);
        if len < count {
            return;
        }
        for at in 0..count {
            // SAFETY: `at` is below `count`, which is no more than the floats the buffer
            // holds (checked above); as above.
            samples.push(unsafe { data.add(at).read_unaligned() });
        }
    }
    // How far ahead of the speakers this piece is: its time in the video, less the time the
    // item's clock says is playing now, at the speed it is playing.
    let ahead = storage.clock.as_ref().map_or(0.0, |clock| {
        // SAFETY: plain readings of live objects' values (see `Clock`).
        let (now, speed, start) = unsafe { (clock.0.time().seconds(), clock.0.rate(), range.start.seconds()) };
        let ahead = (start - now) / speed;
        if speed > 0.0 && ahead.is_finite() { ahead.clamp(0.0, MOST_AHEAD) } else { 0.0 }
    });
    if crate::ffmpeg::tracing() {
        // Once for each second of sound: how far ahead it comes, and the loudest sample of
        // that second as the tap hears it (before the player's volume).
        let loudest = samples.iter().fold(0.0f32, |loudest, sample| loudest.max(sample.abs()));
        let loudest = f32::from_bits(storage.loudest.fetch_max(loudest.to_bits(), Ordering::Relaxed)).max(loudest);
        let before = storage.traced.fetch_add(got as u64, Ordering::Relaxed);
        if before / u64::from(rate) != (before + got as u64) / u64::from(rate) {
            storage.loudest.store(0, Ordering::Relaxed);
            eprintln!(
                "sync tap {got} samples at {rate} Hz, {channels} channels, {ahead:.3}s ahead of the speakers, loudest {loudest:.3}"
            );
        }
    }
    storage.feed.push(rate, channels, samples, Instant::now() + Duration::from_secs_f64(ahead) + PLAYER_DELAY);
}
