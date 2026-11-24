//! A decoded picture's memory handed to the GPU as a texture, with nothing copied.
//!
//! The system's player decodes into pixel buffers that live in an IOSurface (memory the GPU
//! reads where it is). A texture cache from Core Video makes a Metal texture over such a
//! buffer, and wgpu's Metal interface wraps that texture as a wgpu one.
//!
//! The risk is memory being used again while the GPU still reads it. The decoder takes a
//! buffer back for its next picture once the last hold on it is let go, and the texture cache
//! does the same with the texture it made. So the wgpu texture carries both holds, and lets
//! them go in the callback wgpu runs when it destroys the texture: which wgpu does only after
//! the GPU has finished with every command that used it. (The painter that draws the texture
//! keeps its own hold on the texture until the GPU says it is done as well: see
//! `Painter::show_texture` in sp-render.)

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer, CVPixelBufferGetHeight,
    CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth, kCVPixelFormatType_32BGRA,
};
use objc2_metal::{MTLPixelFormat, MTLTexture, MTLTextureType};

use super::MAX_VIDEO_WIDTH;

/// What [`super::Frames::next_texture`] found.
pub enum GpuNext {
    /// A new picture, as a texture (BGRA).
    Picture(wgpu::Texture),
    /// No new picture since the last was taken.
    Nothing,
    /// This video's pictures can't go to the GPU as they are: they are to be copied (see
    /// [`super::Frames::next_with`]), which gives the picture just taken, if one was.
    Unavailable,
}

/// Makes textures from decoded pictures for one wgpu device: Core Video's texture cache for
/// its Metal device. Made and used on the thread that draws.
pub struct GpuImport {
    cache: CFRetained<CVMetalTextureCache>,
}

/// What a texture over a decoded picture holds until the GPU is done with it: the picture's
/// buffer (so the decoder doesn't use its memory again) and the texture the cache made.
struct Held {
    _buffer: Retained<CVPixelBuffer>,
    _texture: CFRetained<CVMetalTexture>,
}

// SAFETY: both are Core Foundation objects, whose reference counts may be changed from any
// thread, and `Held` does nothing with them but let them go.
unsafe impl Send for Held {}
// SAFETY: as above; nothing is reachable through a shared `Held`.
unsafe impl Sync for Held {}

impl GpuImport {
    /// For a wgpu device, or `None` when it is not a Metal one, or Core Video can't make a
    /// cache for it, or SOFTPRESENTER_VIDEO_COPY is set.
    pub fn new(device: &wgpu::Device) -> Option<GpuImport> {
        // Testing aid: SOFTPRESENTER_VIDEO_COPY=1 has videos copied to the GPU as before, to
        // compare the two ways.
        if std::env::var_os("SOFTPRESENTER_VIDEO_COPY").is_some() {
            return None;
        }
        // SAFETY: `as_hal` is only unsafe because what it hands out must not be used to break
        // wgpu's own rules about the device; it is only read here, for the Metal device.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
        let mut made: *mut CVMetalTextureCache = std::ptr::null_mut();
        // SAFETY: the device is a live Metal device; the allocator and all the attributes are
        // left at their defaults; `made` is a valid place for one pointer.
        let status =
            unsafe { CVMetalTextureCache::create(None, None, hal.raw_device(), None, NonNull::from(&mut made)) };
        drop(hal);
        if status != 0 {
            return None;
        }
        // SAFETY: on success the function stored a cache it created, which the caller owns (a
        // reference counted object with one count), or null.
        let cache = unsafe { CFRetained::from_raw(NonNull::new(made)?) };
        Some(GpuImport { cache })
    }

    /// A texture over a decoded picture's memory. `None` for a picture that isn't one plane of
    /// 32-bit BGRA pixels, or isn't in an IOSurface (the texture cache refuses it), or is empty or wider than the screens need
    /// ([`MAX_VIDEO_WIDTH`]), or that Core Video or Metal won't make a texture of.
    pub(super) fn texture(&self, device: &wgpu::Device, buffer: &Retained<CVPixelBuffer>) -> Option<wgpu::Texture> {
        if CVPixelBufferGetPixelFormatType(buffer) != kCVPixelFormatType_32BGRA {
            return None;
        }
        let (width, height) = (CVPixelBufferGetWidth(buffer), CVPixelBufferGetHeight(buffer));
        if width == 0 || height == 0 || width > MAX_VIDEO_WIDTH {
            return None;
        }
        let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
        // Lets the cache give back the memory of textures nothing holds any more.
        self.cache.flush(0);
        let mut made: *mut CVMetalTexture = std::ptr::null_mut();
        // SAFETY: the cache and buffer are live; the buffer is one plane of BGRA pixels, which
        // `BGRA8Unorm` reads as they are; `made` is a valid place for one pointer.
        let status = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &self.cache,
                buffer,
                None,
                MTLPixelFormat::BGRA8Unorm,
                width,
                height,
                0,
                NonNull::from(&mut made),
            )
        };
        if status != 0 {
            return None;
        }
        // SAFETY: on success the function stored a texture it created, which the caller owns
        // (one count), or null.
        let texture = unsafe { CFRetained::from_raw(NonNull::new(made)?) };
        let raw = CVMetalTextureGetTexture(&texture)?;
        if raw.width() != width || raw.height() != height || raw.pixelFormat() != MTLPixelFormat::BGRA8Unorm {
            return None;
        }
        let held = Held { _buffer: buffer.clone(), _texture: texture };
        // SAFETY: `raw` is a 2D, one-level Metal texture of this size and format, made on the
        // Metal device of the wgpu device it is wrapped for (the cache was made from that
        // device's own), and its memory is not touched by anything else while it is held
        // (the decoder has finished writing a picture it hands over). `held` lets go of the
        // decoder's memory only when wgpu destroys the texture, after the GPU has finished with
        // every command that used it.
        let hal = unsafe {
            wgpu::hal::metal::Device::texture_from_raw(
                raw,
                wgpu::TextureFormat::Bgra8Unorm,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent { width: w, height: h, depth: 1 },
                Some(Box::new(move || drop(held))),
            )
        };
        let descriptor = wgpu::TextureDescriptor {
            label: Some("sp-video decoded picture"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            // Read by shaders (Core Video makes it readable), and copied from for a check.
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        // SAFETY: the texture was made from this device's Metal device, as the descriptor says
        // (2D, one level, one sample, this size and format, usages Metal allows of it), it is
        // fully written (a decoded picture), and Metal keeps no layouts to match.
        Some(unsafe {
            device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal, &descriptor, wgpu::TextureUses::RESOURCE)
        })
    }
}
