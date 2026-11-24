//! AVFoundation and ImageIO.

mod cover;
mod gpu;
pub use cover::{DisplayWatch, cover_display};
pub use gpu::{GpuImport, GpuNext};

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_av_foundation::{
    AVAssetImageGenerator, AVMediaCharacteristicContainsAlphaChannel, AVMediaTypeVideo, AVPlayer,
    AVPlayerActionAtItemEnd, AVPlayerItem, AVPlayerItemStatus, AVPlayerItemVideoOutput, AVURLAsset,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString, CFURL, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, CGPDFBox,
    CGPDFDocument, CGPDFPage, kCGColorSpaceSRGB,
};
use objc2_core_media::{CMFormatDescription, CMTime, kCMFormatDescriptionExtension_ContainsAlphaChannel, kCMTimeZero};
use objc2_core_video::{
    CVGetCurrentHostTime, CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelBufferHeightKey,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelBufferWidthKey, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_image_io::{
    CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceThumbnailMaxPixelSize,
};

use crate::{Frame, Lent, PixelOrder};

/// Video pictures wider than this are copied at half size.
const MAX_VIDEO_WIDTH: usize = 2048;

fn shown_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn file_url(path: &Path) -> Result<Retained<NSURL>, String> {
    if !path.exists() {
        return Err(format!("{} could not be found.", shown_name(path)));
    }
    let text = path.to_str().ok_or_else(|| format!("{}: the file's name can't be read.", shown_name(path)))?;
    Ok(NSURL::fileURLWithPath(&NSString::from_str(text)))
}

/// A Core Foundation string as the Foundation string it is the same object as.
fn ns_key(key: &CFString) -> &NSString {
    // SAFETY: CFString and NSString are toll-free bridged: the same object, usable as either.
    unsafe { &*std::ptr::from_ref(key).cast::<NSString>() }
}

fn seconds(time: CMTime) -> Option<f64> {
    // SAFETY: CMTimeGetSeconds only reads the value it is given.
    let seconds = unsafe { time.seconds() };
    seconds.is_finite().then_some(seconds)
}

fn at_seconds(seconds: f64) -> CMTime {
    // SAFETY: CMTimeMakeWithSeconds only builds a value.
    unsafe { CMTime::with_seconds(seconds, 600) }
}

/// The size to ask the system's decoder for when a video is wider than [`MAX_VIDEO_WIDTH`]: its
/// own divided by a whole number, so its shape stays. `None` for a video no wider than that,
/// or whose size can't be read (its pictures are then halved as they are taken).
fn smaller_size(item: &AVPlayerItem) -> Option<(u32, u32)> {
    // SAFETY: reads a static the framework defines.
    let video = unsafe { AVMediaTypeVideo }?;
    // The newer call that replaces this one answers later, on another thread; this one reads
    // the file's header now, which opening a video on the main thread can wait for.
    #[allow(deprecated)]
    // SAFETY: plain getters on live objects.
    let size = unsafe { item.asset().tracksWithMediaType(video).firstObject()?.naturalSize() };
    crate::smaller_than_wide(size.width, size.height, MAX_VIDEO_WIDTH as f64)
}

/// The four-character codes of the video formats that carry a picture's clear parts: ProRes
/// 4444 (with and without extra quality), Animation and PNG in a movie.
const ALPHA_CODECS: [u32; 4] = [
    u32::from_be_bytes(*b"ap4h"),
    u32::from_be_bytes(*b"ap4x"),
    u32::from_be_bytes(*b"rle "),
    u32::from_be_bytes(*b"png "),
];

/// Whether a video's pictures may have clear parts, from what its format says and not from its
/// pixels: the format says so itself (HEVC with alpha, ProRes 4444 and others set a flag), or is
/// one of the few that always can ([`ALPHA_CODECS`]). A video whose format can't be read is
/// taken to, so that it keeps the way such videos are drawn.
fn may_carry_alpha(formats: &[(u32, bool)]) -> bool {
    formats.is_empty() || formats.iter().any(|(codec, flagged)| *flagged || ALPHA_CODECS.contains(codec))
}

/// Whether the first video track's format says its pictures may have clear parts (see
/// [`may_carry_alpha`]).
fn track_may_carry_alpha(item: &AVPlayerItem) -> bool {
    // SAFETY: reads a static the framework defines.
    let Some(video) = (unsafe { AVMediaTypeVideo }) else { return true };
    // The same synchronous reading of the header as `smaller_size`.
    #[allow(deprecated)]
    // SAFETY: plain getters on live objects.
    let Some(track) = (unsafe { item.asset().tracksWithMediaType(video).firstObject() }) else { return true };
    // SAFETY: a plain getter on a live object, and a static the framework defines.
    if unsafe { AVMediaCharacteristicContainsAlphaChannel.is_some_and(|alpha| track.hasMediaCharacteristic(alpha)) } {
        return true;
    }
    // SAFETY: a plain getter on a live object.
    let descriptions = unsafe { track.formatDescriptions() };
    let formats: Vec<(u32, bool)> = descriptions
        .iter()
        .map(|description| {
            // SAFETY: every element of a track's format descriptions is a CMFormatDescription, a
            // Core Foundation object that is also an Objective-C one, so the pointer is to one;
            // it is only read, and `descriptions` keeps it alive.
            let description = unsafe { &*Retained::as_ptr(&description).cast::<CMFormatDescription>() };
            // SAFETY: plain getters on a live description; the flag's key is a static the
            // framework defines.
            let (codec, flag) = unsafe {
                (
                    description.media_sub_type(),
                    description
                        .extension(kCMFormatDescriptionExtension_ContainsAlphaChannel)
                        .is_some_and(|flag| flag.downcast_ref::<CFBoolean>().is_none_or(CFBoolean::as_bool)),
                )
            };
            (codec, flag)
        })
        .collect();
    may_carry_alpha(&formats)
}

/// A video playing, with its sound. It lives on the main thread.
pub struct Player {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
    halved: Arc<Mutex<Vec<u8>>>,
    /// Whether pictures may go to the GPU as they are (see [`Frames::next_texture`]): not for a
    /// video whose format may carry clear parts.
    zero_copy: Arc<AtomicBool>,
    /// A picture taken for the GPU that couldn't be made a texture, kept for the next copy.
    parked: Arc<Mutex<Option<Parked>>>,
    looping: bool,
    ended: bool,
}

/// A picture's pixel buffer set aside to be taken next (see [`Frames::next_texture`]).
struct Parked(Retained<CVPixelBuffer>);

// SAFETY: a pixel buffer is a Core Foundation object, whose reference count may be changed from
// any thread, and this one is only put in and taken out of its place, under a lock.
unsafe impl Send for Parked {}

impl Player {
    /// Opens a video, paused at its start. Must be called on the main thread. The error is a
    /// sentence for the operator.
    pub fn open(path: &Path) -> Result<Player, String> {
        let mtm = MainThreadMarker::new().ok_or("Videos can only be opened on the main thread.")?;
        let url = file_url(path)?;
        // SAFETY: the URL is a valid file URL; the item is made on the main thread, as it must be.
        let item = unsafe { AVPlayerItem::playerItemWithURL(&url, mtm) };
        // Pictures come out as rows of BGRA pixels, the order the frames are copied in.
        let format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
        // SAFETY: the key is a static string the framework defines.
        let key = ns_key(unsafe { kCVPixelBufferPixelFormatTypeKey });
        // A video wider than the screens need is asked for smaller, which the system's decoder
        // makes on the GPU as it decodes: far less work than taking pixels out of each full
        // picture afterwards, and a smoother picture.
        let smaller = smaller_size(&item).map(|(w, h)| (NSNumber::new_u32(w), NSNumber::new_u32(h)));
        let (mut keys, mut values) = (vec![key], vec![format.as_ref() as &AnyObject]);
        // Pictures in memory the GPU can read where it is (an IOSurface, which Metal can
        // use as a texture), so the screens needn't copy them; they can still be read by
        // the CPU, as before.
        let (metal, no_properties): (_, Retained<NSDictionary<NSString, AnyObject>>) =
            (NSNumber::new_bool(true), NSDictionary::new());
        // SAFETY: the keys are static strings the framework defines.
        keys.extend(unsafe {
            [ns_key(kCVPixelBufferMetalCompatibilityKey), ns_key(kCVPixelBufferIOSurfacePropertiesKey)]
        });
        values.extend([metal.as_ref() as &AnyObject, no_properties.as_ref()]);
        if let Some((width, height)) = &smaller {
            // SAFETY: the keys are static strings the framework defines.
            keys.extend(unsafe { [ns_key(kCVPixelBufferWidthKey), ns_key(kCVPixelBufferHeightKey)] });
            values.extend([width.as_ref() as &AnyObject, height.as_ref()]);
        }
        let attributes: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(&keys, &values);
        // SAFETY: the attributes are a dictionary of the keys and value types the output expects.
        let output = unsafe {
            AVPlayerItemVideoOutput::initWithPixelBufferAttributes(AVPlayerItemVideoOutput::alloc(), Some(&attributes))
        };
        // SAFETY: both objects are alive; an item takes any number of outputs.
        unsafe { item.addOutput(&output) };
        // SAFETY: made on the main thread with a live item.
        let player = unsafe { AVPlayer::playerWithPlayerItem(Some(&item), mtm) };
        // At the end the player stays where it is, so it can be looped or left on its last picture.
        // SAFETY: a plain setter on a live object.
        unsafe { player.setActionAtItemEnd(AVPlayerActionAtItemEnd::None) };
        let zero_copy = Arc::new(AtomicBool::new(!track_may_carry_alpha(&item)));
        Ok(Player {
            player,
            item,
            output,
            halved: Arc::default(),
            zero_copy,
            parked: Arc::default(),
            looping: false,
            ended: false,
        })
    }

    /// Where its pictures are taken from, on any thread.
    pub fn frames(&self) -> Frames {
        Frames {
            output: self.output.clone(),
            halved: self.halved.clone(),
            zero_copy: self.zero_copy.clone(),
            parked: self.parked.clone(),
        }
    }

    pub fn play(&mut self) {
        if self.ended {
            self.seek(0.0);
        }
        // SAFETY: a plain call on a live object, on its thread.
        unsafe { self.player.play() };
    }

    pub fn pause(&self) {
        // SAFETY: a plain call on a live object, on its thread.
        unsafe { self.player.pause() };
    }

    pub fn playing(&self) -> bool {
        // SAFETY: a plain getter on a live object.
        unsafe { self.player.rate() != 0.0 && !self.ended }
    }

    /// Goes to a time, in seconds from the start, exactly.
    pub fn seek(&mut self, to: f64) {
        self.ended = false;
        // SAFETY: plain values; zero tolerance asks for the exact time.
        unsafe {
            self.player.seekToTime_toleranceBefore_toleranceAfter(at_seconds(to.max(0.0)), kCMTimeZero, kCMTimeZero);
        }
    }

    /// How far in it is, in seconds.
    pub fn position(&self) -> f64 {
        // SAFETY: a plain getter on a live object.
        seconds(unsafe { self.player.currentTime() }).unwrap_or(0.0)
    }

    /// How long it is, once that is known.
    pub fn duration(&self) -> Option<f64> {
        // SAFETY: a plain getter on a live object.
        seconds(unsafe { self.item.duration() }).filter(|seconds| *seconds > 0.0)
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// From 0 (silent) to 1.
    pub fn set_volume(&self, volume: f32) {
        // SAFETY: a plain setter on a live object.
        unsafe { self.player.setVolume(volume.clamp(0.0, 1.0)) };
    }

    /// True once a video that doesn't loop has played to its end.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Why the video can't play, once the system has tried and failed.
    pub fn problem(&self) -> Option<String> {
        // SAFETY: plain getters on a live object.
        unsafe {
            (self.item.status() == AVPlayerItemStatus::Failed).then(|| {
                self.item.error().map_or_else(
                    || "The video could not be played.".to_owned(),
                    |error| format!("The video could not be played: {}", error.localizedDescription()),
                )
            })
        }
    }

    /// Keeps it going: a looping video starts again at its end, and one that doesn't stops
    /// there. Says how soon to call again.
    pub fn tick(&mut self) -> Duration {
        let Some(duration) = self.duration() else {
            return Duration::from_millis(100);
        };
        let left = duration - self.position();
        if left <= 0.02 && self.playing() {
            if self.looping {
                self.seek(0.0);
            } else {
                self.pause();
                self.ended = true;
            }
            return Duration::from_millis(50);
        }
        // Paused, or ended and left on screen, nothing is due.
        if !self.playing() {
            return Duration::from_millis(500);
        }
        Duration::from_secs_f64(left.clamp(0.01, 0.5))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: plain calls on live objects, on their thread.
        unsafe {
            self.player.pause();
            // The player lets go of its item, and the item of its output: left joined, the
            // system keeps some of each video's timing objects for as long as the app runs.
            self.item.removeOutput(&self.output);
            self.player.replaceCurrentItemWithPlayerItem(None);
        }
    }
}

/// Where a playing video's pictures are taken from. It can be sent to another thread.
#[derive(Clone)]
pub struct Frames {
    output: Retained<AVPlayerItemVideoOutput>,
    /// Room for the half-size copy of a picture too wide, used again for each.
    halved: Arc<Mutex<Vec<u8>>>,
    /// True while pictures may go to the GPU without a copy: from the video's format (see
    /// [`track_may_carry_alpha`]), and until one fails to.
    zero_copy: Arc<AtomicBool>,
    /// A picture taken for the GPU that couldn't be made a texture, kept for the next copy.
    parked: Arc<Mutex<Option<Parked>>>,
}

impl Frames {
    /// The newest picture as a GPU texture made of the decoder's own memory, with nothing copied
    /// ([`GpuNext::Picture`]); [`GpuNext::Nothing`] when there is no new picture since the last
    /// was taken; and [`GpuNext::Unavailable`], with no picture taken, when this video's
    /// pictures must be copied as [`Frames::next_with`] does (its format may have clear parts,
    /// or a picture could not be made a texture: that one is kept, and the copy that follows
    /// takes it). The texture holds the picture's pixels
    /// in BGRA order, and keeps the decoder's memory from being used again until the GPU is
    /// done with it.
    pub fn next_texture(&self, import: &GpuImport, device: &wgpu::Device) -> GpuNext {
        if !self.zero_copy.load(Ordering::Relaxed) {
            return GpuNext::Unavailable;
        }
        let Some(buffer) = self.newest() else { return GpuNext::Nothing };
        if let Some(texture) = import.texture(device, &buffer) {
            return GpuNext::Picture(texture);
        }
        // From now on this video's pictures are copied, this one first: kept for the next
        // `next_with`, so that a video that is paused shows its picture all the same.
        self.zero_copy.store(false, Ordering::Relaxed);
        *self.parked.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Parked(buffer));
        GpuNext::Unavailable
    }

    /// From now on this video's pictures are copied: for when a texture made of the decoder's
    /// memory was taken but the GPU side couldn't show it, so later pictures don't fail the same
    /// way.
    pub fn copy_from_now(&self) {
        self.zero_copy.store(false, Ordering::Relaxed);
    }

    /// The picture to show now, if there is a new one since the last was taken.
    pub fn next(&self) -> Option<Frame> {
        let buffer = self.newest()?;
        let locked = Locked::read(&buffer)?;
        let (bytes, width, height, bytes_per_row) = locked.rows()?;
        Some(crate::bgra_rows_to_rgba(bytes, width, height, bytes_per_row, MAX_VIDEO_WIDTH))
    }

    /// Lends the picture to show now, as [`crate::Frames::next_with`] says: the decoder's own
    /// rows of BGRA pixels while they are held, or a half-size copy of a picture too wide.
    pub fn next_with<R>(&self, show: impl FnOnce(&Lent<'_>) -> R) -> Option<R> {
        let buffer = self.newest()?;
        let locked = Locked::read(&buffer)?;
        let (bytes, width, height, bytes_per_row) = locked.rows()?;
        let lend = |bytes, width: usize, height: usize, bytes_per_row| Lent {
            width: width as u32,
            height: height as u32,
            bytes_per_row,
            bytes,
            order: PixelOrder::Bgra,
            opaque: true,
        };
        if width <= MAX_VIDEO_WIDTH {
            return Some(show(&lend(bytes, width, height, bytes_per_row)));
        }
        // The copy is made into the same room each time; a thread that died holding it left
        // nothing a new copy doesn't replace.
        let mut halved = self.halved.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (width, height) = crate::halved_rows(bytes, width, height, bytes_per_row, &mut halved);
        (width > 0 && height > 0).then(|| show(&lend(&halved, width, height, width * 4)))
    }

    /// The newest picture's pixel buffer, if there is a new one since the last was taken (or one
    /// was kept because it couldn't go to the GPU).
    fn newest(&self) -> Option<Retained<CVPixelBuffer>> {
        if let Some(Parked(buffer)) = self.parked.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
            return Some(buffer);
        }
        let host = i64::try_from(CVGetCurrentHostTime()).unwrap_or(i64::MAX);
        // SAFETY: a video output may be asked for pictures from any thread; these are plain values.
        let time = unsafe { self.output.itemTimeForMachAbsoluteTime(host) };
        // SAFETY: as above.
        if !unsafe { self.output.hasNewPixelBufferForItemTime(time) } {
            return None;
        }
        // SAFETY: as above; a null pointer says the display time isn't wanted.
        unsafe { self.output.copyPixelBufferForItemTime_itemTimeForDisplay(time, std::ptr::null_mut()) }
    }
}

/// A pixel buffer locked for reading, and unlocked when this is let go, however the code
/// reading it ends.
struct Locked<'a>(&'a CVPixelBuffer);

impl<'a> Locked<'a> {
    fn read(buffer: &'a CVPixelBuffer) -> Option<Locked<'a>> {
        // SAFETY: the buffer is alive; it is locked for reading, and unlocked when the value
        // made here is dropped.
        let locked = unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) } == 0;
        locked.then_some(Locked(buffer))
    }

    /// The picture's rows of BGRA pixels, with its width, height and bytes from one row to the
    /// next. `None` for a buffer that isn't one plane of 32-bit BGRA pixels, or is empty.
    fn rows(&self) -> Option<(&[u8], usize, usize, usize)> {
        let buffer = self.0;
        if CVPixelBufferGetPixelFormatType(buffer) != kCVPixelFormatType_32BGRA {
            return None;
        }
        let (width, height) = (CVPixelBufferGetWidth(buffer), CVPixelBufferGetHeight(buffer));
        let bytes_per_row = CVPixelBufferGetBytesPerRow(buffer);
        let base = CVPixelBufferGetBaseAddress(buffer);
        if base.is_null() || width == 0 || height == 0 || bytes_per_row < width.checked_mul(4)? {
            return None;
        }
        let length = bytes_per_row.checked_mul(height)?;
        // SAFETY: while locked, the base address of a one-plane buffer points at `height` rows
        // of `bytes_per_row` bytes. They stay valid and unchanged until the unlock, which
        // comes when `self` is dropped, after the slice borrowed from it is let go.
        Some((unsafe { std::slice::from_raw_parts(base.cast::<u8>(), length) }, width, height, bytes_per_row))
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        // SAFETY: unlocks the lock taken in `read`, with the same flags.
        unsafe { CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly) };
    }
}

/// Draws a picture the system decoded into RGBA rows.
fn draw(image: &CGImage) -> Option<Frame> {
    draw_at(image, CGImage::width(Some(image)), CGImage::height(Some(image)))
}

/// Draws a picture into RGBA rows of a size, scaling it to fill them.
pub(crate) fn draw_at(image: &CGImage, width: usize, height: usize) -> Option<Frame> {
    if width == 0 || height == 0 {
        return None;
    }
    let mut rgba = vec![0u8; width.checked_mul(height)?.checked_mul(4)?];
    // SAFETY: the name is a static string the framework defines.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: the buffer holds `height` rows of `width * 4` bytes and outlives the context,
    // which is dropped before the buffer is read.
    let context =
        unsafe { CGBitmapContextCreate(rgba.as_mut_ptr().cast(), width, height, 8, width * 4, Some(&space), info) }?;
    let all = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64));
    CGContext::draw_image(Some(&context), all, Some(image));
    drop(context);
    crate::unpremultiply(&mut rgba);
    Some(Frame { width: width as u32, height: height as u32, rgba })
}

/// A still from a video, `at` seconds in, no larger than `max_side` on its longer side.
pub fn still(path: &Path, max_side: u32, at: f64) -> Result<Frame, String> {
    let url = file_url(path)?;
    // SAFETY: a valid file URL and no options.
    let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) };
    // SAFETY: a live asset; the generator may be used on any thread.
    let generator = unsafe { AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset) };
    let side = f64::from(max_side);
    // SAFETY: plain setters on a live object.
    unsafe {
        generator.setAppliesPreferredTrackTransform(true);
        generator.setMaximumSize(CGSize::new(side, side));
    }
    // The newer call that replaces this one hands the picture back later, on another thread;
    // this one waits for it, which a still for the media bin can do.
    #[allow(deprecated)]
    // SAFETY: plain values; a null pointer says the actual time isn't wanted.
    let image = unsafe { generator.copyCGImageAtTime_actualTime_error(at_seconds(at), std::ptr::null_mut()) }.map_err(
        |error| format!("{}: not a video SoftPresenter can open ({}).", shown_name(path), error.localizedDescription()),
    )?;
    draw(&image).ok_or_else(|| format!("{}: the video's picture is empty.", shown_name(path)))
}

/// A picture file in any format the system reads, no larger than `max_side` on its longer
/// side, turned the way the camera held it.
pub fn picture(path: &Path, max_side: u32) -> Result<Frame, String> {
    let url = file_url(path)?;
    // SAFETY: NSURL and CFURL are toll-free bridged: the same object, usable as either.
    let url: &CFURL = unsafe { &*Retained::as_ptr(&url).cast::<CFURL>() };
    // SAFETY: a valid file URL and no options.
    let source = unsafe { CGImageSource::with_url(url, None) }
        .ok_or_else(|| format!("{}: not a picture SoftPresenter can open.", shown_name(path)))?;
    let yes = NSNumber::new_bool(true);
    let size = NSNumber::new_u32(max_side);
    // SAFETY: the keys are static strings the framework defines.
    let keys = unsafe {
        [
            ns_key(kCGImageSourceCreateThumbnailFromImageAlways),
            ns_key(kCGImageSourceCreateThumbnailWithTransform),
            ns_key(kCGImageSourceThumbnailMaxPixelSize),
        ]
    };
    let options: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&keys, &[yes.as_ref() as &AnyObject, yes.as_ref(), size.as_ref()]);
    // SAFETY: NSDictionary and CFDictionary are toll-free bridged; the keys and values are the
    // types these options take.
    let options: &CFDictionary = unsafe { &*Retained::as_ptr(&options).cast::<CFDictionary>() };
    // SAFETY: a live source and a dictionary of options it understands.
    let image = unsafe { source.thumbnail_at_index(0, Some(options)) }
        .ok_or_else(|| format!("{}: the picture could not be read.", shown_name(path)))?;
    draw(&image).ok_or_else(|| format!("{}: the picture is empty.", shown_name(path)))
}

/// Opens a PDF and says how many pages (at most `most_pages`) it has, or why it can't be read.
fn open_pdf(
    path: &Path,
    most_pages: usize,
) -> Result<(objc2_core_foundation::CFRetained<CGPDFDocument>, usize), String> {
    let name = shown_name(path);
    let url = file_url(path)?;
    // SAFETY: NSURL and CFURL are toll-free bridged: the same object, usable as either.
    let url: &CFURL = unsafe { &*Retained::as_ptr(&url).cast::<CFURL>() };
    let document =
        CGPDFDocument::with_url(Some(url)).ok_or_else(|| format!("{name} is not a PDF SoftPresenter can read."))?;
    if !CGPDFDocument::is_unlocked(Some(&document)) {
        return Err(format!("{name} is locked with a password. Save a copy without one, then try again."));
    }
    let pages = CGPDFDocument::number_of_pages(Some(&document)).min(most_pages);
    if pages == 0 {
        return Err(format!("{name} has no pages."));
    }
    Ok((document, pages))
}

/// The size each page of a PDF is drawn at by [`pdf_draw`] (as large as fits in `max_width` x
/// `max_height`, turned as the page says and cut to its crop box), without drawing any: one for
/// each of the first `most_pages` pages, so a deck can be laid out at once.
pub fn pdf_sizes(path: &Path, max_width: u32, max_height: u32, most_pages: usize) -> Result<Vec<(u32, u32)>, String> {
    let (document, pages) = open_pdf(path, most_pages)?;
    Ok((1..=pages)
        .map(|number| {
            CGPDFDocument::page(Some(&document), number)
                .and_then(|page| fitted(&page, max_width, max_height))
                .map_or((max_width, max_height), |(_, _, width, height, _)| (width as u32, height as u32))
        })
        .collect())
}

/// Draws the pages of a PDF on `threads` threads at once, each page as [`pdf_sizes`] says, on
/// white, and hands each to `each` with its number (from 1), from the thread that drew it, in
/// whatever order they finish; `each` returns `false` to stop everything. Every thread opens
/// the document for itself. Returns how many pages were drawn.
pub fn pdf_draw(
    path: &Path,
    max_width: u32,
    max_height: u32,
    most_pages: usize,
    threads: usize,
    each: impl Fn(usize, Frame) -> bool + Sync,
) -> Result<usize, String> {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
    let (_, pages) = open_pdf(path, most_pages)?;
    let next = AtomicUsize::new(1);
    let stopped = AtomicBool::new(false);
    let drawn = AtomicUsize::new(0);
    let work = || {
        let Ok((document, _)) = open_pdf(path, most_pages) else { return };
        // SAFETY: the name is a static string the framework defines.
        let Some(space) = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })) else { return };
        loop {
            let number = next.fetch_add(1, Relaxed);
            if number > pages || stopped.load(Relaxed) {
                return;
            }
            let Some(page) = CGPDFDocument::page(Some(&document), number) else { continue };
            let Some(frame) = draw_page(&page, max_width, max_height, &space) else { continue };
            drawn.fetch_add(1, Relaxed);
            if !each(number, frame) {
                stopped.store(true, Relaxed);
                return;
            }
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..threads.clamp(1, pages) {
            scope.spawn(work);
        }
        work();
    });
    Ok(drawn.load(Relaxed))
}

/// What fits a page in the size given: its crop box's width and height, then the size drawn
/// and the scale.
fn fitted(page: &CGPDFPage, max_width: u32, max_height: u32) -> Option<(f64, f64, usize, usize, f64)> {
    let crop = CGPDFPage::box_rect(Some(page), CGPDFBox::CropBox);
    let (w, h) = (crop.size.width, crop.size.height);
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let turn = CGPDFPage::rotation_angle(Some(page)).rem_euclid(360);
    let (shown_w, shown_h) = if turn % 180 == 0 { (w, h) } else { (h, w) };
    let scale = (f64::from(max_width) / shown_w).min(f64::from(max_height) / shown_h);
    let width = ((shown_w * scale).round() as usize).max(1);
    let height = ((shown_h * scale).round() as usize).max(1);
    Some((w, h, width, height, scale))
}

/// One PDF page drawn as large as fits in the size given.
fn draw_page(page: &CGPDFPage, max_width: u32, max_height: u32, space: &CGColorSpace) -> Option<Frame> {
    let (w, h, width, height, scale) = fitted(page, max_width, max_height)?;
    let crop = CGPDFPage::box_rect(Some(page), CGPDFBox::CropBox);
    let turn = CGPDFPage::rotation_angle(Some(page)).rem_euclid(360);
    let mut rgba = vec![0u8; width.checked_mul(height)?.checked_mul(4)?];
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: the buffer holds `height` rows of `width * 4` bytes and outlives the context,
    // which is dropped before the buffer is read.
    let context =
        unsafe { CGBitmapContextCreate(rgba.as_mut_ptr().cast(), width, height, 8, width * 4, Some(space), info) }?;
    let context = Some(&*context);
    // White first: a page draws only what is on it.
    CGContext::set_rgb_fill_color(context, 1.0, 1.0, 1.0, 1.0);
    CGContext::fill_rect(context, CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64)));
    // The page's crop box to the whole picture, turned clockwise as the page says. The last
    // change named applies first to what is drawn.
    CGContext::scale_ctm(context, scale, scale);
    match turn {
        90 => {
            CGContext::translate_ctm(context, 0.0, w);
            CGContext::rotate_ctm(context, -std::f64::consts::FRAC_PI_2);
        }
        180 => {
            CGContext::translate_ctm(context, w, h);
            CGContext::rotate_ctm(context, std::f64::consts::PI);
        }
        270 => {
            CGContext::translate_ctm(context, h, 0.0);
            CGContext::rotate_ctm(context, std::f64::consts::FRAC_PI_2);
        }
        _ => {}
    }
    CGContext::translate_ctm(context, -crop.origin.x, -crop.origin.y);
    CGContext::clip_to_rect(context, crop);
    CGContext::draw_pdf_page(context, Some(page));
    Some(Frame { width: width as u32, height: height as u32, rgba })
}

/// The home folder of the account the app runs as, from the system's own records of the
/// account (not the `HOME` variable, which whatever started the app may have set).
pub fn home_folder() -> Option<std::path::PathBuf> {
    let user = objc2_foundation::NSUserName();
    let home = objc2_foundation::NSHomeDirectoryForUser(Some(&user))?.to_string();
    let home = std::path::PathBuf::from(home);
    (home.is_absolute() && home.is_dir()).then_some(home)
}

/// The names macOS gives the connected displays ("HP 27f", "3DTV"), as System Settings shows
/// them, each with its display id. Only the main thread may ask; elsewhere there are none.
pub fn display_names() -> Vec<(u32, String)> {
    let Some(main_thread) = MainThreadMarker::new() else { return Vec::new() };
    let key = NSString::from_str("NSScreenNumber");
    objc2_app_kit::NSScreen::screens(main_thread)
        .iter()
        .filter_map(|screen| {
            let description = screen.deviceDescription();
            let number = description.objectForKey(&key)?.downcast::<NSNumber>().ok()?;
            Some((number.unsignedIntValue(), screen.localizedName().to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(four: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*four)
    }

    /// Whether pictures go to the GPU as they are is decided from the video's format, never from
    /// its pixels: the ordinary codecs do, whatever the extra formats say; the ones that carry
    /// clear parts, or say they do, don't; and a video whose format can't be read doesn't.
    #[test]
    fn a_videos_format_says_whether_its_pictures_may_have_clear_parts() {
        // H.264, HEVC, ProRes 422, JPEG and MPEG-4: none.
        for plain in [b"avc1", b"hvc1", b"hev1", b"apcn", b"mp4v", b"jpeg"] {
            assert!(!may_carry_alpha(&[(code(plain), false)]), "{}", String::from_utf8_lossy(plain));
        }
        // ProRes 4444 (both), Animation and PNG always can.
        for alpha in [b"ap4h", b"ap4x", b"rle ", b"png "] {
            assert!(may_carry_alpha(&[(code(alpha), false)]), "{}", String::from_utf8_lossy(alpha));
        }
        // HEVC with alpha says so itself.
        assert!(may_carry_alpha(&[(code(b"hvc1"), true)]));
        // One format of several that can is enough; a video with no format to read is taken to.
        assert!(may_carry_alpha(&[(code(b"avc1"), false), (code(b"ap4h"), false)]));
        assert!(may_carry_alpha(&[]));
    }
}
