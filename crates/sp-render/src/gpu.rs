use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use vello::low_level::vello_encoding::Patch;
use vello::peniko::{Blob, Color, ImageAlphaType, ImageData};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};

use crate::picture::rows_hold;
use crate::{Picture, PixelOrder};

/// Paints scenes into GPU textures. One per thread that draws.
pub struct Painter {
    renderer: Renderer,
    /// The moving pictures made here that something may still draw (see [`MovingPicture`]).
    moving: Vec<KeptMoving>,
    /// How many pictures have been painted (see [`Painter::paints`]).
    paints: u64,
    /// Copies a BGRA texture that is already on the GPU into a moving picture's: made the first
    /// time one is shown, or by [`Painter::prepare_texture_copy`].
    swizzle: Option<Swizzle>,
    /// Textures [`Painter::show_texture`] sent to the GPU that it may not have finished with,
    /// each with what says it has.
    in_flight: VecDeque<(wgpu::Texture, Arc<AtomicBool>)>,
}

/// The most textures [`Painter::show_texture`] holds on to at once. More than this many
/// unfinished means the GPU is far behind; wgpu itself keeps each texture until its commands
/// are done, so letting go of the oldest early costs nothing but this extra hold.
const MOST_IN_FLIGHT: usize = 8;

/// A compute pass that copies a BGRA texture's pixels into a moving picture's texture byte for
/// byte: the texture is read as the colours it holds (red, green, blue, alpha) and written back
/// in the order of the bytes it had (blue first), so the picture is the very bytes
/// [`Painter::show_rows`] sends, and is drawn the same.
struct Swizzle {
    pipeline: wgpu::ComputePipeline,
}

const SWIZZLE_SHADER: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var dest: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(source);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    textureStore(dest, id.xy, textureLoad(source, id.xy, 0).bgra);
}
";

impl Swizzle {
    fn new(device: &wgpu::Device) -> Swizzle {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sp-render texture copy"),
            source: wgpu::ShaderSource::Wgsl(SWIZZLE_SHADER.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("sp-render texture copy"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        Swizzle { pipeline }
    }
}

/// Why a picture was not painted. The texture it was to go into is left as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintError {
    /// The picture needs more working memory on the GPU than one picture may take (a slide
    /// with a whole book pasted into it, say). The same picture at the same size is refused
    /// every time, at once, so asking again is no use.
    TooHeavy,
    /// The GPU did not work the picture out: it was too slow this time, or dropped it. That
    /// says nothing about the picture, so it is worth asking for again in a moment.
    NotNow(String),
    /// Anything else the renderer reports.
    Failed(String),
}

impl PaintError {
    /// True when the same picture may well be painted if asked for again in a moment.
    pub fn worth_trying_again(&self) -> bool {
        matches!(self, PaintError::NotNow(_))
    }
}

impl std::fmt::Display for PaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaintError::TooHeavy => f.write_str("there is too much on it to draw"),
            PaintError::NotNow(why) | PaintError::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for PaintError {}

impl From<PaintError> for String {
    fn from(error: PaintError) -> String {
        error.to_string()
    }
}

impl From<vello::Error> for PaintError {
    fn from(error: vello::Error) -> PaintError {
        match error {
            vello::Error::SceneTooHeavy => PaintError::TooHeavy,
            vello::Error::GpuTooSlow | vello::Error::FrameDropped => PaintError::NotNow(error.to_string()),
            other => PaintError::Failed(other.to_string()),
        }
    }
}

/// A picture that changes many times a second (a playing video), kept in a GPU texture of its
/// own: each new picture's pixels are sent straight into it, where a [`Picture`] made new each
/// time would be copied once more and take a new place in the renderer's own texture. The
/// fourth byte of each pixel sent is its straight alpha, as a [`Picture`]'s is.
///
/// Only the [`Painter`] that made it draws its [`picture`](MovingPicture::picture) moving (any
/// other draws it clear, as nothing): it is for the thread that draws the screens, and never
/// goes in the pictures every thread shares. Scenes drawn with it show whatever was sent last,
/// and go on showing that after this is let go, for as long as they are kept.
pub struct MovingPicture {
    texture: wgpu::Texture,
    picture: Picture,
    /// Says, while this is held, that more pixels may come.
    _held: Arc<()>,
}

/// A moving picture's texture kept by the painter, with what says whether it can be let go.
struct KeptMoving {
    image: ImageData,
    held: Weak<()>,
    /// True once it has been painted, from when on the renderer keeps a copy of `image` too.
    painted: bool,
}

impl MovingPicture {
    /// The picture to draw: the same one however many times new pixels are sent.
    pub fn picture(&self) -> Picture {
        self.picture.clone()
    }

    /// True when pixels of this size and order of bytes can be sent into it.
    pub fn fits(&self, width: u32, height: u32, order: PixelOrder) -> bool {
        (self.picture.width(), self.picture.height(), self.picture.image.format) == (width, height, order.format())
    }
}

impl Painter {
    /// Fails if the GPU cannot run the renderer's shaders.
    pub fn new(device: &wgpu::Device) -> Result<Painter, String> {
        let options = RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            // One thread is the renderer's own advice for macOS.
            num_init_threads: std::num::NonZeroUsize::new(1),
            pipeline_cache: None,
        };
        Renderer::new(device, options)
            .map(|renderer| Painter {
                renderer,
                moving: Vec::new(),
                paints: 0,
                swizzle: None,
                in_flight: VecDeque::new(),
            })
            .map_err(|e| e.to_string())
    }

    /// Makes a moving picture of a size, for pixels in an order of bytes. It shows nothing
    /// until pixels are sent with [`Painter::show_rows`]. `None` for a size of zero, or one
    /// larger than the GPU's textures can be.
    pub fn moving_picture(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        order: PixelOrder,
    ) -> Option<MovingPicture> {
        let most = device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > most || height > most {
            return None;
        }
        // The renderer copies this texture into its own, which holds RGBA bytes, so it is made
        // the same kind whatever the order of the bytes sent: the order is said by the picture
        // drawn, and put right by the renderer as it draws.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sp-render moving picture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // Pixels are written by the queue (`show_rows`) or by a compute pass (`show_texture`).
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        // The renderer takes the pixels from the texture, by the picture's name. The picture's
        // own are clear and never looked at while the texture is kept; they are there because
        // the renderer stops the thread if it is ever asked to draw a picture with neither
        // (and memory nothing reads or writes costs next to nothing).
        let clear = vec![0u8; (width as usize).checked_mul(height as usize)?.checked_mul(4)?];
        let image = ImageData {
            data: Blob::new(Arc::new(clear)),
            format: order.format(),
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        };
        let whole = wgpu::TexelCopyTextureInfoBase {
            texture: texture.clone(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        };
        self.renderer.override_image(&image, Some(whole));
        let held = Arc::new(());
        self.moving.push(KeptMoving { image: image.clone(), held: Arc::downgrade(&held), painted: false });
        Some(MovingPicture { texture, picture: Picture { image }, _held: held })
    }

    /// Sends a moving picture its new pixels: rows of four-byte pixels in the order of bytes it
    /// was made for, each row starting `bytes_per_row` after the last (which may be more than
    /// its pixels fill). Everything drawn with its picture from now on shows them. False, with
    /// nothing changed, when there are too few bytes for its size.
    pub fn show_rows(
        &mut self,
        queue: &wgpu::Queue,
        moving: &MovingPicture,
        rows: &[u8],
        bytes_per_row: usize,
    ) -> bool {
        let (width, height) = (moving.picture.width(), moving.picture.height());
        let Ok(stride) = u32::try_from(bytes_per_row) else { return false };
        if !rows_hold(width, height, rows.len(), bytes_per_row) {
            return false;
        }
        queue.write_texture(
            moving.texture.as_image_copy(),
            rows,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.renderer.mark_override_image_dirty(&moving.picture.image);
        true
    }

    /// Sends a moving picture its new pixels from a texture already on the GPU (a decoded video
    /// picture's own memory), with no copy through the CPU: the texture is 32-bit BGRA with
    /// the picture's size, and the moving picture must have been made for [`PixelOrder::Rgba`]
    /// (the GPU turns the order of the bytes as it copies). Everything drawn with its picture
    /// from now on shows them. False, with nothing changed, when the texture isn't of that
    /// kind.
    ///
    /// The painter keeps `source` until the GPU says it has finished with it, however soon the
    /// caller would have let it go, so whatever memory the texture stands for isn't used
    /// again early.
    pub fn show_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        moving: &MovingPicture,
        source: wgpu::Texture,
    ) -> bool {
        let (width, height) = (moving.picture.width(), moving.picture.height());
        if source.format() != wgpu::TextureFormat::Bgra8Unorm
            || (source.width(), source.height()) != (width, height)
            || !source.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
            || moving.picture.image.format != PixelOrder::Bgra.format()
        {
            return false;
        }
        self.retire_finished();
        let swizzle = self.swizzle.get_or_insert_with(|| Swizzle::new(device));
        let from = source.create_view(&wgpu::TextureViewDescriptor::default());
        let to = moving.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp-render texture copy"),
            layout: &swizzle.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&from) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&to) },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&swizzle.pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        queue.submit([encoder.finish()]);
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        queue.on_submitted_work_done(move || flag.store(true, Ordering::Release));
        self.in_flight.push_back((source, done));
        if self.in_flight.len() > MOST_IN_FLIGHT {
            self.in_flight.pop_front();
        }
        self.renderer.mark_override_image_dirty(&moving.picture.image);
        true
    }

    /// Gets the compute pass [`Painter::show_texture`] uses ready, which takes a moment (the
    /// GPU's compiler works on it), so that the first picture of a video doesn't. Does nothing
    /// the second time.
    pub fn prepare_texture_copy(&mut self, device: &wgpu::Device) {
        self.swizzle.get_or_insert_with(|| Swizzle::new(device));
    }

    /// Lets go of the textures [`Painter::show_texture`] held that the GPU has finished with.
    fn retire_finished(&mut self) {
        while self.in_flight.front().is_some_and(|(_, done)| done.load(Ordering::Acquire)) {
            self.in_flight.pop_front();
        }
    }

    /// How many textures sent with [`Painter::show_texture`] are still held, the GPU not
    /// having said it has finished with them as of the last [`Painter::show_texture`] or
    /// [`Painter::paint`].
    pub fn textures_in_flight(&self) -> usize {
        self.in_flight.len()
    }

    /// How many moving pictures' textures are kept: those something may still show, as of the
    /// last painting.
    pub fn moving_pictures(&self) -> usize {
        self.moving.len()
    }

    /// After a scene is painted, lets go of the textures of moving pictures nothing shows any
    /// more. One is kept while more pixels may come for it, and then while any scene drawn
    /// with it is kept (the picture a fade starts from, say), so that scene goes on showing
    /// the pixels sent last.
    fn forget_unused_moving_pictures(&mut self, painted: &Scene) {
        if self.moving.is_empty() {
            return;
        }
        let in_scene =
            |image: &ImageData| {
                painted.encoding().resources.patches.iter().any(
                    |patch| matches!(patch, Patch::Image { image: drawn, .. } if drawn.data.id() == image.data.id()),
                )
            };
        let renderer = &mut self.renderer;
        self.moving.retain_mut(|kept| {
            if in_scene(&kept.image) {
                kept.painted = true;
                return true;
            }
            // Who holds the picture, now that the scene just painted isn't drawn with it: the
            // list here, the renderer once it has painted it (it remembers every picture it
            // has a place for), and each scene kept that is drawn with it. Should the
            // renderer have given the place away, a last scene loses its picture early and
            // shows it clear; nothing worse.
            let unused =
                kept.held.strong_count() == 0 && kept.image.data.strong_count() <= 1 + usize::from(kept.painted);
            if unused {
                renderer.override_image(&kept.image, None);
            }
            !unused
        });
    }

    /// Paints into a view of a texture made by [`target_texture`] with the same size.
    ///
    /// The painter's working memory on the GPU is sized from what it draws. The call waits for
    /// the GPU to work the picture out (not to fill in its pixels), looks at whether it had the
    /// room, and works it out again with more if not. So `Ok` means the whole picture was sent
    /// to be drawn, and an error that nothing was: the texture keeps what it had.
    ///
    /// Two things are taken on trust. A picture that was painted at this size before and had
    /// room (a playing video, frame after frame) is not looked at again. And the wait is short:
    /// four tenths of a second for each run of the picture on a Mac's or a discrete GPU (four
    /// times that on an integrated one of a PC, sixteen on one that runs on the processor),
    /// and four of those at most, after which the picture is given up with
    /// [`PaintError::NotNow`], so that a thread drawing for an audience is never held for long
    /// by a GPU that is stuck. A GPU seen to finish what was given up, only slowly, is given
    /// longer the next time, up to four seconds a run, so a heavy slide appears, later.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> Result<(), PaintError> {
        self.paint_on(device, queue, scene, target, w, h, Color::BLACK)
    }

    /// Paints as [`Painter::paint`] does, on clear rather than black: where the scene draws
    /// nothing the texture is left clear, for a see-through screen. The texture then holds each
    /// pixel's colour as it is with how solid it is beside it (straight alpha: the renderer
    /// works premultiplied, and divides the colours by the alpha as it writes them).
    pub fn paint_see_through(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> Result<(), PaintError> {
        self.paint_on(device, queue, scene, target, w, h, Color::TRANSPARENT)
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_on(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        w: u32,
        h: u32,
        base_color: Color,
    ) -> Result<(), PaintError> {
        self.paints += 1;
        self.retire_finished();
        let params = RenderParams { base_color, width: w, height: h, antialiasing_method: AaConfig::Area };
        // A scene of plain colour fills only (no text, gradient or picture) makes vello 0.12 take
        // a shortcut that throws its picture atlas away while still counting the pictures in it
        // as uploaded, so the next picture drawn from it comes out black. Such a scene is given a
        // gradient no one can see, so it takes the usual way.
        if scene.encoding().resources.patches.is_empty() {
            let mut padded = scene.clone();
            let unseen = vello::peniko::Gradient::new_linear((0.0, 0.0), (1.0, 0.0))
                .with_stops([Color::TRANSPARENT, Color::TRANSPARENT]);
            padded.fill(
                vello::peniko::Fill::NonZero,
                vello::kurbo::Affine::IDENTITY,
                &unseen,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, 1.0, 1.0),
            );
            let done = self.renderer.render_to_texture(device, queue, &padded, target, &params);
            self.forget_unused_moving_pictures(scene);
            return done.map_err(PaintError::from);
        }
        let done = self.renderer.render_to_texture(device, queue, scene, target, &params);
        self.forget_unused_moving_pictures(scene);
        done.map_err(PaintError::from)
    }

    /// Call when nothing more will be drawn for a while: GPU memory that a heavy picture made
    /// the painter take is given back. Costs nothing after ordinary slides, and never waits.
    ///
    /// The GPU lets the memory go once the last picture that used it has left it, and only
    /// when the device is next looked at. So this returns `false` while that is still to come:
    /// call it again in a moment (each call looks at the device), until it returns `true`.
    pub fn rest(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        self.renderer.rest(device, queue)
    }

    /// How many pictures the painter has been asked for.
    pub fn paints(&self) -> u64 {
        self.paints
    }

    /// The bytes of GPU memory the painter keeps ready between pictures.
    pub fn spare_bytes(&self) -> u64 {
        self.renderer.spare_bytes()
    }

    /// How many times a picture had to be worked out again because it needed more room than the
    /// pictures before it.
    pub fn reruns(&self) -> u64 {
        self.renderer.scratch_reruns()
    }

    /// How many times a picture was sent to the GPU to be worked out: once for one that had
    /// room, once more for each time it was worked out again, and not at all for one already
    /// known to be too heavy.
    pub fn runs(&self) -> u64 {
        self.renderer.scratch_runs()
    }

    /// How long [`Painter::paint`] has waited for the GPU, in all, checking whether pictures
    /// fitted: the rest of a paint's time is the painter's own work.
    pub fn gpu_waited(&self) -> std::time::Duration {
        self.renderer.gpu_waited()
    }

    /// Sets how long one [`Painter::paint`] waits for the GPU before it gives the picture up
    /// with [`PaintError::NotNow`]; `None` is the usual wait. For tests of what happens when
    /// the GPU is too slow.
    pub fn wait_at_most(&mut self, wait: Option<std::time::Duration>) {
        self.renderer.set_longest_wait(wait);
    }

    /// Sets how many times one [`Painter::paint`] works a picture out before it gives it up;
    /// `None` is the usual number. For tests of a picture that goes on needing more room.
    pub fn work_out_at_most(&mut self, times: Option<usize>) {
        self.renderer.set_most_runs(times);
    }

    /// Has the next `pictures` pictures dropped as macOS drops the pictures of an app whose
    /// GPU work faulted: nothing of them reaches the GPU. For tests of what such a picture
    /// leaves behind.
    pub fn drop_pictures(&mut self, pictures: u32) {
        self.renderer.drop_frames(pictures);
    }

    /// Keeps the painter's working memory at these sizes whatever is drawn, as the renderer did
    /// before it sized it from the picture; a picture that needs more is then an error. For
    /// tests that compare the two ways.
    pub fn keep_scratch_at(&mut self, sizes: Option<vello::ScratchSizes>) {
        self.renderer.set_fixed_scratch(sizes);
    }
}

/// A texture the painter can draw into, that can be shown, copied to a window or read back.
/// Sizes of zero are raised to one pixel.
pub fn target_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sp-render target"),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Reads a texture back from the GPU as tightly packed RGBA rows. Blocks until done.
pub fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Result<Vec<u8>, String> {
    read_back(device, queue, texture, None)
}

/// Reads a texture back as `read_pixels` does, for a thread that shares the device with others
/// drawing on it (Remote Control's): it never waits inside the device, which would hold up the
/// other threads' work meanwhile, but looks every millisecond whether the copy has come, for at
/// most `within`.
pub fn read_pixels_gently(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    within: std::time::Duration,
) -> Result<Vec<u8>, String> {
    read_back(device, queue, texture, Some(within))
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    gently: Option<std::time::Duration>,
) -> Result<Vec<u8>, String> {
    let (w, h) = (texture.width(), texture.height());
    let row = w as usize * 4;
    // The GPU requires each row of a copy to start on a 256-byte boundary.
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded_row = row.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sp-render readback"),
        size: (padded_row * h as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    match gently {
        None => {
            device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
            rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        }
        Some(within) => {
            let until = std::time::Instant::now() + within;
            loop {
                device.poll(wgpu::PollType::Poll).map_err(|e| e.to_string())?;
                match rx.try_recv() {
                    Ok(mapped) => break mapped.map_err(|e| e.to_string())?,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err("The copy was lost.".to_owned()),
                    Err(std::sync::mpsc::TryRecvError::Empty) if std::time::Instant::now() >= until => {
                        return Err("The GPU took too long to give the picture back.".to_owned());
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        std::thread::sleep(std::time::Duration::from_millis(1))
                    }
                }
            }
        }
    }
    let mapped = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let mut pixels = Vec::with_capacity(row * h as usize);
    for padded in mapped.chunks(padded_row) {
        pixels.extend_from_slice(&padded[..row]);
    }
    Ok(pixels)
}
