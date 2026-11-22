// Copyright 2022 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Vello is a compute-centric 2D graphics rendering engine written in Rust,
//! using [`wgpu`].
//! It efficiently draws large 2D scenes with interactive or near-interactive
//! performance.
//!
//! ![image](https://github.com/linebender/vello/assets/8573618/cc2b742e-2135-4b70-8051-c49aeddb5d19)
//!
//!
//! ## Motivation
//!
//! Vello is meant to fill the same place in the graphics stack as other vector graphics renderers like [Skia](https://skia.org/), [Cairo](https://www.cairographics.org/), and its predecessor project [Piet](https://www.cairographics.org/).
//! On a basic level, that means it provides tools to render shapes, images, gradients, texts, etc, using a PostScript-inspired API, the same that powers SVG files and [the browser `<canvas>` element](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D).
//!
//! Vello's selling point is that it gets better performance than other renderers by better leveraging the GPU.
//! In traditional PostScript-style renderers, some steps of the render process like sorting and clipping either need to be handled in the CPU or done through the use of intermediary textures.
//! Vello avoids this by using prefix-scan algorithms to parallelize work that usually needs to happen in sequence, so that work can be offloaded to the GPU with minimal use of temporary buffers.
//!
//! This means that Vello needs a GPU with support for compute shaders to run.
//!
//!
//! ## Getting started
//!
//! Vello is meant to be integrated deep in UI render stacks.
//! While drawing in a Vello [`Scene`] is easy, actually rendering that scene to a surface setting up a wgpu context, which is a non-trivial task.
//!
//! To use Vello as the renderer for your PDF reader / GUI toolkit / etc, your code will have to look roughly like this:
//!
//! ```ignore
//! // Initialize wgpu and get handles
//! let (width, height) = ...;
//! let device: wgpu::Device = ...;
//! let queue: wgpu::Queue = ...;
//! let mut renderer = Renderer::new(
//!    &device,
//!    RendererOptions {
//!       use_cpu: false,
//!       antialiasing_support: vello::AaSupport::all(),
//!       num_init_threads: NonZeroUsize::new(1),
//!    },
//! ).expect("Failed to create renderer");
//!
//! // Create scene and draw stuff in it
//! let mut scene = vello::Scene::new();
//! scene.fill(
//!    vello::peniko::Fill::NonZero,
//!    vello::Affine::IDENTITY,
//!    vello::Color::from_rgb8(242, 140, 168),
//!    None,
//!    &vello::Circle::new((420.0, 200.0), 120.0),
//! );
//!
//! // Draw more stuff
//! scene.push_layer(...);
//! scene.fill(...);
//! scene.stroke(...);
//! scene.pop_layer(...);
//!
//! let texture = device.create_texture(&...);
//! // Render to a wgpu Texture
//! renderer
//!    .render_to_texture(
//!       &device,
//!       &queue,
//!       &scene,
//!       &texture,
//!       &vello::RenderParams {
//!          base_color: palette::css::BLACK, // Background color
//!          width,
//!          height,
//!          antialiasing_method: AaConfig::Msaa16,
//!       },
//!    )
//!    .expect("Failed to render to a texture");
//! // Do things with surface texture, such as blitting it to the Surface using
//! // wgpu::util::TextureBlitter.
//! ```
//!
//! See the [`research/examples/`](https://github.com/linebender/vello/tree/main/research/examples) folder to see how that code integrates with frameworks like winit.

// LINEBENDER LINT SET - lib.rs - v2
// See https://linebender.org/wiki/canonical-lints/
// These lints aren't included in Cargo.toml because they
// shouldn't apply to examples and tests
#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]
// Targeting e.g. 32-bit means structs containing usize can give false positives for 64-bit.
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
// END LINEBENDER LINT SET
#![cfg_attr(docsrs, feature(doc_cfg))]
// The following lints are part of the Linebender standard set,
// but resolving them has been deferred for now.
// Feel free to send a PR that solves one or more of these.
// Need to allow instead of expect until Rust 1.83 https://github.com/rust-lang/rust/pull/130025
#![allow(missing_docs, reason = "We have many as-yet undocumented items.")]
#![expect(
    missing_debug_implementations,
    clippy::cast_possible_truncation,
    clippy::missing_assert_message,
    reason = "Deferred"
)]
#![allow(
    clippy::todo,
    unreachable_pub,
    unnameable_types,
    reason = "Deferred, only apply in some feature sets so not expect"
)]

mod debug;
mod recording;
mod render;
mod scene;
// SoftPresenter: scratch buffers sized from what is drawn.
#[cfg(feature = "wgpu")]
mod scratch;
mod shaders;

#[cfg(feature = "wgpu")]
pub mod util;
#[cfg(feature = "wgpu")]
mod wgpu_engine;

pub mod low_level {
    //! Utilities which can be used to create an alternative Vello renderer to [`Renderer`][crate::Renderer].
    //!
    //! These APIs have not been carefully designed, and might not be powerful enough for this use case.

    pub use crate::debug::DebugLayers;
    pub use crate::recording::{
        BindType, BufferProxy, Command, ImageFormat, ImageProxy, Recording, ResourceId,
        ResourceProxy, ShaderId,
    };
    pub use crate::render::Render;
    pub use crate::shaders::FullShaders;

    pub use vello_encoding;
}
/// Styling and composition primitives.
pub use peniko;
/// 2D geometry, with a focus on curves.
pub use peniko::kurbo;

#[cfg(feature = "wgpu")]
use peniko::ImageData;
#[cfg(feature = "wgpu")]
pub use wgpu;

pub use scene::{DrawGlyphs, Scene};
#[cfg(feature = "wgpu")]
pub use scratch::{ScratchPlan, ScratchRoom, ScratchSizes};
pub use vello_encoding::{FontEmbolden, Glyph, NormalizedCoord};

use low_level::ShaderId;
#[cfg(feature = "wgpu")]
use low_level::{FullShaders, Recording, Render};
use thiserror::Error;

#[cfg(feature = "wgpu")]
use debug::DebugLayers;
#[cfg(feature = "wgpu")]
use vello_encoding::{BumpAllocators, Resolver};
#[cfg(feature = "wgpu")]
use wgpu_engine::{ExternalResource, WgpuEngine};

#[cfg(feature = "wgpu")]
use std::{num::NonZeroUsize, sync::atomic::AtomicBool};
#[cfg(feature = "wgpu")]
use wgpu::{Device, Queue, TextureView};
#[cfg(all(feature = "wgpu", feature = "wgpu-profiler"))]
use wgpu_profiler::{GpuProfiler, GpuProfilerSettings};

/// Represents the anti-aliasing method to use during a render pass.
///
/// Can be configured for a render operation by setting [`RenderParams::antialiasing_method`].
/// Each value of this can only be used if the corresponding field on [`AaSupport`] was used.
///
/// This can be converted into an `AaSupport` using [`Iterator::collect`],
/// as `AaSupport` implements `FromIterator`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AaConfig {
    /// Area anti-aliasing, where the alpha value for a pixel is computed from integrating
    /// the winding number over its square area.
    ///
    /// This technique produces very accurate values when the shape has winding number of 0 or 1
    /// everywhere, but can result in conflation artifacts otherwise.
    /// It generally has better performance than the multi-sampling methods.
    ///
    /// Can only be used if [enabled][AaSupport::area] for the `Renderer`.
    Area,
    /// 8x Multisampling
    ///
    /// Can only be used if [enabled][AaSupport::msaa8] for the `Renderer`.
    Msaa8,
    /// 16x Multisampling
    ///
    /// Can only be used if [enabled][AaSupport::msaa16] for the `Renderer`.
    Msaa16,
}

/// Represents the set of anti-aliasing configurations to enable during pipeline creation.
///
/// This is configured at `Renderer` creation time ([`Renderer::new`]) by setting
/// [`RendererOptions::antialiasing_support`].
///
/// This can be created from a set of `AaConfig` using [`Iterator::collect`],
/// as `AaSupport` implements `FromIterator`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct AaSupport {
    /// Support [`AaConfig::Area`].
    pub area: bool,
    /// Support [`AaConfig::Msaa8`].
    pub msaa8: bool,
    /// Support [`AaConfig::Msaa16`].
    pub msaa16: bool,
}

impl AaSupport {
    /// Support every anti-aliasing method.
    ///
    /// This might increase startup time, as more shader variations must be compiled.
    pub fn all() -> Self {
        Self {
            area: true,
            msaa8: true,
            msaa16: true,
        }
    }

    /// Support only [`AaConfig::Area`].
    ///
    /// This should be the default choice for most users.
    pub fn area_only() -> Self {
        Self {
            area: true,
            msaa8: false,
            msaa16: false,
        }
    }
}

impl FromIterator<AaConfig> for AaSupport {
    fn from_iter<T: IntoIterator<Item = AaConfig>>(iter: T) -> Self {
        let mut result = Self {
            area: false,
            msaa8: false,
            msaa16: false,
        };
        for config in iter {
            match config {
                AaConfig::Area => result.area = true,
                AaConfig::Msaa8 => result.msaa8 = true,
                AaConfig::Msaa16 => result.msaa16 = true,
            }
        }
        result
    }
}

/// Errors that can occur in Vello.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// There is no available device with the features required by Vello.
    #[cfg(feature = "wgpu")]
    #[error("Couldn't find suitable device")]
    NoCompatibleDevice,
    /// Failed to create surface.
    /// See [`wgpu::CreateSurfaceError`] for more information.
    #[cfg(feature = "wgpu")]
    #[error("Couldn't create wgpu surface")]
    WgpuCreateSurfaceError(#[from] wgpu::CreateSurfaceError),
    /// Surface doesn't support the required texture formats.
    /// Make sure that you have a surface which provides one of
    /// [`TextureFormat::Rgba8Unorm`][wgpu::TextureFormat::Rgba8Unorm]
    /// or [`TextureFormat::Bgra8Unorm`][wgpu::TextureFormat::Bgra8Unorm] as texture formats.
    // TODO: Why does this restriction exist?
    #[cfg(feature = "wgpu")]
    #[error("Couldn't find `Rgba8Unorm` or `Bgra8Unorm` texture formats for surface")]
    UnsupportedSurfaceFormat,

    /// Used a buffer inside a recording while it was not available.
    /// Check if you have created it and not freed before its last usage.
    #[cfg(feature = "wgpu")]
    #[error("Buffer '{0}' is not available but used for {1}")]
    UnavailableBufferUsed(&'static str, &'static str),
    /// Failed to async map a buffer.
    /// See [`wgpu::BufferAsyncError`] for more information.
    #[cfg(feature = "wgpu")]
    #[error("Failed to async map a buffer")]
    BufferAsyncError(#[from] wgpu::BufferAsyncError),
    /// Failed to download an internal buffer for debug visualization.
    #[cfg(feature = "wgpu")]
    #[cfg(feature = "debug_layers")]
    #[error("Failed to download internal buffer '{0}' for visualization")]
    DownloadError(&'static str),

    #[cfg(feature = "wgpu")]
    #[error("wgpu Error from scope")]
    WgpuErrorFromScope(#[from] wgpu::Error),

    /// Failed to create [`GpuProfiler`].
    /// See [`wgpu_profiler::CreationError`] for more information.
    #[cfg(feature = "wgpu-profiler")]
    #[error("Couldn't create wgpu profiler")]
    #[doc(hidden)] // End-users of Vello should not have `wgpu-profiler` enabled.
    ProfilerCreationError(#[from] wgpu_profiler::CreationError),

    // SoftPresenter: an addition.
    /// The scene needs more scratch room than the renderer may use: more than one GPU buffer
    /// can hold, more than one frame may have in all (1 GiB), or more than the fixed sizes the
    /// renderer was told to keep to. Nothing was drawn, and the same scene on the same target
    /// will be refused again.
    #[cfg(feature = "wgpu")]
    #[error("The scene is too heavy to draw: it needs more scratch memory than is allowed")]
    SceneTooHeavy,
    /// The GPU did not finish working out the frame in the time one call may take (see
    /// `GPU_WAIT`): it is busy with something else, or stuck. Also given for a frame that was
    /// still asking for more room after every run one call makes (`RUNS`). Nothing was drawn.
    /// Unlike [`SceneTooHeavy`](Self::SceneTooHeavy) this says nothing about the scene:
    /// drawing it again later may well work, and goes on from the room found so far.
    #[cfg(feature = "wgpu")]
    #[error("The GPU took too long to work out the picture")]
    GpuTooSlow,
    /// The GPU never ran the frame's commands: the counters read back were not written. macOS
    /// does that to the frames of a process whose GPU work faulted. Nothing was drawn, and the
    /// frame is not taken for one that fitted.
    #[cfg(feature = "wgpu")]
    #[error("The GPU dropped the picture without working it out")]
    FrameDropped,
    /// Asking the device how far the GPU has got failed. The renderer only ever asks without
    /// waiting ([`wgpu::PollType::Poll`]), for which wgpu 30 has no error to give (its two
    /// errors, a timeout and a wrong submission, belong to waits), so this is not expected to
    /// be seen; it exists because the call can fail by its type.
    #[cfg(feature = "wgpu")]
    #[error("Asking the GPU how far it has got failed: {0}")]
    GpuWait(#[from] wgpu::PollError),

    /// Failed to compile the shaders.
    #[cfg(feature = "hot_reload")]
    #[error("Failed to compile shaders:\n{0}")]
    #[doc(hidden)] // End-users of Vello should not have `hot_reload` enabled.
    ShaderCompilation(#[from] vello_shaders::compile::ErrorVec),
}

#[cfg_attr(
    not(feature = "wgpu"),
    expect(dead_code, reason = "this can be unused when wgpu feature is not used")
)]
pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// Renders a scene into a texture or surface.
///
/// Currently, each renderer only supports a single surface format, if it
/// supports drawing to surfaces at all.
/// This is an assumption which is known to be limiting, and is planned to change.
#[cfg(feature = "wgpu")]
pub struct Renderer {
    #[cfg_attr(
        not(feature = "hot_reload"),
        expect(
            dead_code,
            reason = "Options are only used to reinitialise on a hot reload"
        )
    )]
    options: RendererOptions,
    engine: WgpuEngine,
    resolver: Resolver,
    image_atlas: Option<recording::ImageProxy>,
    shaders: FullShaders,
    // SoftPresenter: how much scratch room recent frames used; how many times the coarse
    // stages were sent to the GPU; and, while spare buffers given back are still held by
    // frames on the GPU, the flag the device sets when those frames have left it.
    scratch: scratch::Scratch,
    coarse_runs: u64,
    giving_back: Option<std::sync::Arc<AtomicBool>>,
    // SoftPresenter: the longest one call waits for the GPU (`GPU_WAIT`, unless a test set
    // another); the most times one call runs a frame (`RUNS`, unless a test set another); and
    // how many of the next frames a test has asked to be treated as dropped by the GPU.
    longest_wait: std::time::Duration,
    most_runs: usize,
    frames_to_drop: u32,
    // SoftPresenter: how long this renderer has waited for the GPU, in all, reading back the
    // counters of frames it had to check (see `gpu_waited`).
    gpu_waited: std::time::Duration,
    #[cfg(feature = "debug_layers")]
    debug: debug::DebugRenderer,
    #[cfg(feature = "wgpu-profiler")]
    #[doc(hidden)] // End-users of Vello should not have `wgpu-profiler` enabled.
    /// The profiler used with events for this renderer. This is *not* treated as public API.
    pub profiler: GpuProfiler,
    #[cfg(feature = "wgpu-profiler")]
    #[doc(hidden)] // End-users of Vello should not have `wgpu-profiler` enabled.
    /// The results from profiling. This is *not* treated as public API.
    pub profile_result: Option<Vec<wgpu_profiler::GpuTimerQueryResult>>,
}
// This is not `Send` (or `Sync`) on WebAssembly as the
// underlying wgpu types are not. This can be enabled with the
// `fragile-send-sync-non-atomic-wasm` feature in wgpu.
// See https://github.com/gfx-rs/wgpu/discussions/4127 for
// further discussion of this topic.
#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
static_assertions::assert_impl_all!(Renderer: Send);

/// Parameters used in a single render that are configurable by the client.
///
/// These are used in [`Renderer::render_to_texture`].
pub struct RenderParams {
    /// The background color applied to the target. This value is only applicable to the full
    /// pipeline.
    pub base_color: peniko::Color,

    /// Dimensions of the rasterization target
    pub width: u32,
    pub height: u32,

    /// The anti-aliasing algorithm. The selected algorithm must have been initialized while
    /// constructing the `Renderer`.
    pub antialiasing_method: AaConfig,
}

#[cfg(feature = "wgpu")]
/// Options which are set at renderer creation time, used in [`Renderer::new`].
pub struct RendererOptions {
    /// If true, run all stages up to fine rasterization on the CPU.
    ///
    /// This is not a recommended configuration as it is expected to have poor performance,
    /// but it can be useful for debugging.
    // TODO: Consider evolving this so that the CPU stages can be configured dynamically via
    // `RenderParams`.
    pub use_cpu: bool,

    /// Represents the enabled set of AA configurations. This will be used to determine which
    /// pipeline permutations should be compiled at startup.
    ///
    /// By default this will be all modes, to support the widest range of.
    /// It is recommended that most users configure this.
    pub antialiasing_support: AaSupport,

    /// How many threads to use for initialisation of shaders.
    ///
    /// Use `Some(1)` to use a single thread. This is recommended when on macOS
    /// (see <https://github.com/bevyengine/bevy/pull/10812#discussion_r1496138004>)
    ///
    /// Set to `None` to use a heuristic which will use many but not all threads
    ///
    /// Has no effect on WebAssembly
    ///
    /// Will default to `None` on most platforms, `Some(1)` on macOS.
    pub num_init_threads: Option<NonZeroUsize>,

    /// The pipeline cache to use when creating the shaders.
    ///
    /// For much more discussion of expected usage patterns, see the documentation on that type.
    pub pipeline_cache: Option<wgpu::PipelineCache>,
}

#[cfg(feature = "wgpu")]
impl Default for RendererOptions {
    fn default() -> Self {
        Self {
            use_cpu: false,
            antialiasing_support: AaSupport::all(),
            #[cfg(target_os = "macos")]
            num_init_threads: NonZeroUsize::new(1),
            #[cfg(not(target_os = "macos"))]
            num_init_threads: None,
            pipeline_cache: None,
        }
    }
}

/// SoftPresenter: the longest one call to `render_to_texture` waits for the GPU, over all the
/// runs of its frame, before it gives the frame up with [`Error::GpuTooSlow`]; and how often
/// the device is looked at meanwhile.
///
/// The wait is for the coarse stages only. A thread that draws for an audience must not stand
/// still for long behind a GPU that is stuck, so this is a few times the longest wait seen
/// with a GPU that is merely busy, and no more (`PROVENANCE.md` has the measurements).
#[cfg(feature = "wgpu")]
const GPU_WAIT: std::time::Duration = std::time::Duration::from_millis(400);
#[cfg(feature = "wgpu")]
const GPU_LOOK: std::time::Duration = std::time::Duration::from_micros(200);

/// SoftPresenter: the most times one call to `render_to_texture` runs a frame. A stage that
/// runs out of room still counts all it needed, so each run settles at least the first stage
/// that was short: five stages, and the segments, which have no flag of their own, are settled
/// in seven runs at most. This is the end for a frame that somehow is not.
#[cfg(feature = "wgpu")]
const RUNS: usize = 16;

#[cfg(feature = "wgpu")]
struct RenderResult {
    bump: Option<BumpAllocators>,
    #[cfg(feature = "debug_layers")]
    captured: Option<render::CapturedBuffers>,
}

#[cfg(feature = "wgpu")]
impl Renderer {
    /// Creates a new renderer for the specified device.
    pub fn new(device: &Device, options: RendererOptions) -> Result<Self> {
        let mut engine = WgpuEngine::new(options.use_cpu, options.pipeline_cache.clone());
        // If we are running in parallel (i.e. the number of threads is not 1)
        if options.num_init_threads != NonZeroUsize::new(1) {
            #[cfg(not(target_arch = "wasm32"))]
            engine.use_parallel_initialisation();
        }
        let shaders = shaders::full_shaders(device, &mut engine, &options)?;
        #[cfg(not(target_arch = "wasm32"))]
        engine.build_shaders_if_needed(device, options.num_init_threads);
        #[cfg(feature = "debug_layers")]
        let debug = debug::DebugRenderer::new(device, wgpu::TextureFormat::Rgba8Unorm, &mut engine);

        let limits = device.limits();
        let largest_buffer = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size);

        Ok(Self {
            options,
            engine,
            resolver: Resolver::new(),
            image_atlas: None,
            shaders,
            scratch: scratch::Scratch::new(largest_buffer),
            coarse_runs: 0,
            giving_back: None,
            longest_wait: GPU_WAIT,
            most_runs: RUNS,
            frames_to_drop: 0,
            gpu_waited: std::time::Duration::ZERO,
            #[cfg(feature = "debug_layers")]
            debug,
            #[cfg(feature = "wgpu-profiler")]
            profiler: GpuProfiler::new(device, GpuProfilerSettings::default())?,
            #[cfg(feature = "wgpu-profiler")]
            profile_result: None,
        })
    }

    /// Renders a scene to the target texture.
    ///
    /// The texture is assumed to be of the specified dimensions and have been created with
    /// the [`wgpu::TextureFormat::Rgba8Unorm`] format and the [`wgpu::TextureUsages::STORAGE_BINDING`]
    /// flag set.
    ///
    /// If you want to render Vello content to a surface (such as in a UI toolkit), you have two options:
    /// 1) Render to an intermediate texture, which is the same size as the surface.
    ///    You would then use [`TextureBlitter`][wgpu::util::TextureBlitter] to blit the rendered result from
    ///    that texture to the surface.
    ///    This pattern is supported by the [`util`] module.
    /// 2) Call `render_to_texture` directly on the [`SurfaceTexture`][wgpu::SurfaceTexture]'s texture, if
    ///    it has the right usages. This should generally be avoided, as some GPUs assume that you will not
    ///    be rendering to the surface using a compute pipeline, and optimise accordingly.
    ///
    /// SoftPresenter: this is no longer Vello's single pass with constant scratch sizes, which
    /// is kept as [`render_to_texture_unchecked`](Self::render_to_texture_unchecked). The
    /// scratch buffers are sized from what recent frames used; the frame's counters are read
    /// back before its last stage, and a frame that ran out of room is run again with more.
    ///
    /// `Ok` means the scene was sent to be drawn in full, with one exception that is taken on
    /// trust: a scene that was drawn at this size before and fitted (a playing video, frame
    /// after frame) is not checked again, since the same scene always counts the same.
    /// Otherwise the call waits for the GPU to finish the frame's earlier stages, by asking
    /// the device again and again and never waiting inside it, and for no longer than
    /// `GPU_WAIT` in all. The errors SoftPresenter added say why nothing was drawn:
    /// [`Error::SceneTooHeavy`], [`Error::GpuTooSlow`] and [`Error::FrameDropped`].
    pub fn render_to_texture(
        &mut self,
        device: &Device,
        queue: &Queue,
        scene: &Scene,
        texture: &TextureView,
        params: &RenderParams,
    ) -> Result<()> {
        // Shaders run on the CPU cannot have their counters read back this way.
        if self.engine.use_cpu {
            return self.render_to_texture_unchecked(device, queue, scene, texture, params);
        }
        self.render_checked(device, queue, scene, texture, params)?;
        #[cfg(feature = "wgpu-profiler")]
        {
            self.profiler.end_frame().unwrap();
            if let Some(result) = self
                .profiler
                .process_finished_frame(queue.get_timestamp_period())
            {
                self.profile_result = Some(result);
            }
        }
        Ok(())
    }

    /// SoftPresenter: draws a scene with scratch buffers sized for it, running it again with
    /// more room for as long as its counters say it ran out.
    fn render_checked(
        &mut self,
        device: &Device,
        queue: &Queue,
        scene: &Scene,
        texture: &TextureView,
        params: &RenderParams,
    ) -> Result<()> {
        let now = std::time::Instant::now();
        let give_up_at = now + self.longest_wait;
        if !self.scratch.allows(params.width, params.height) {
            return Err(Error::SceneTooHeavy);
        }
        let mut room = self.scratch.room(now);
        // The room an earlier run of this frame found it needs.
        let mut found = None;
        for run in 0..self.most_runs {
            let mut render = Render::new();
            let coarse = render.render_encoding_coarse(
                scene.encoding(),
                &mut self.resolver,
                &self.shaders,
                &mut self.image_atlas,
                params,
                true,
                Some(room),
            );
            let plan = *render
                .scratch_plan()
                .expect("a render given scratch sizes has a plan");
            let print = render.scene_print();
            self.scratch.drawing(&plan, now);
            // A frame that would take more than one frame may have even in the least room it
            // could be given, or a scene already found too heavy, is not sent to the GPU at
            // all. Only its pictures are: the resolver has counted them as sent.
            if !plan.within_the_most() || self.scratch.refused_before(print) {
                let pictures = render::pictures_only(coarse);
                if !pictures.commands.is_empty() {
                    self.engine.run_recording(
                        device,
                        queue,
                        &pictures,
                        &[],
                        "render_to_texture refused",
                        #[cfg(feature = "wgpu-profiler")]
                        &mut self.profiler,
                    )?;
                }
                // Spare buffers still age while a scene is refused frame after frame.
                self.engine.release_idle(
                    now,
                    scratch::KEPT_SPARE,
                    self.scratch.largest_resting_buffer(),
                    scratch::KEPT_LARGE_SPARE,
                );
                return Err(Error::SceneTooHeavy);
            }
            let bump_buf = render.bump_buf();
            // A test may ask for the frame to be dropped: its commands are then not sent, and
            // what follows is what follows a read-back that says the GPU dropped the frame.
            let dropped = self.frames_to_drop > 0;
            self.frames_to_drop -= u32::from(dropped);
            if !dropped {
                self.engine.run_recording(
                    device,
                    queue,
                    &coarse,
                    &[],
                    "render_to_texture coarse",
                    #[cfg(feature = "wgpu-profiler")]
                    &mut self.profiler,
                )?;
                self.coarse_runs += 1;
            }
            // A scene that was drawn before and fitted, given no less room now, fits again: its
            // counters need not be waited for. A playing video is this case, frame after frame.
            // Any other frame is judged by its own counters, and only by counters the GPU
            // really wrote: a frame it was too slow with or dropped is an error, never a fit,
            // and is not remembered as either fitting or too heavy.
            let verdict = if dropped {
                Err(Error::FrameDropped)
            } else if self.scratch.fitted_before(print, &plan, now) {
                self.engine.free_download(bump_buf);
                Ok(scratch::Verdict::Fits)
            } else {
                let waiting = std::time::Instant::now();
                let bump = self.read_bump(device, bump_buf, give_up_at);
                self.gpu_waited += waiting.elapsed();
                bump.map(|bump| self.scratch.judge(&plan, &bump, print, now))
            };
            let mut rest = Recording::default();
            if matches!(verdict, Ok(scratch::Verdict::Fits)) {
                let target = render.out_image();
                render.record_fine(&self.shaders, &mut rest);
                self.engine.run_recording(
                    device,
                    queue,
                    &rest,
                    &[ExternalResource::Image(target, texture)],
                    "render_to_texture fine",
                    #[cfg(feature = "wgpu-profiler")]
                    &mut self.profiler,
                )?;
                if run > 0 {
                    // The stages that could not say what they needed were given room by guess,
                    // often far too much. The next frame asks for what this one used, so these
                    // buffers are not kept for it.
                    self.engine.release_spare();
                } else {
                    self.engine.release_idle(
                        now,
                        scratch::KEPT_SPARE,
                        self.scratch.largest_resting_buffer(),
                        scratch::KEPT_LARGE_SPARE,
                    );
                }
                return Ok(());
            }
            render.abandon_fine(&mut rest);
            self.engine.run_recording(
                device,
                queue,
                &rest,
                &[],
                "render_to_texture abandoned",
                #[cfg(feature = "wgpu-profiler")]
                &mut self.profiler,
            )?;
            // The buffers that were too small are of no more use.
            self.engine.release_spare();
            match verdict {
                Ok(scratch::Verdict::Again(more)) if std::time::Instant::now() < give_up_at => {
                    found = Some(more);
                    room = self.scratch.room_to_run_again(more);
                }
                // Out of time between two runs: the next frame starts with the room found.
                Ok(scratch::Verdict::Again(more)) => {
                    self.scratch.keep(more, now);
                    return Err(Error::GpuTooSlow);
                }
                Ok(_) => return Err(Error::SceneTooHeavy),
                Err(e) => {
                    if let Some(more) = found {
                        self.scratch.keep(more, now);
                    }
                    if matches!(e, Error::FrameDropped) {
                        self.forget_pictures(device, queue);
                    }
                    return Err(e);
                }
            }
        }
        // Still asking for more after every run allowed. That says the frame did not settle,
        // not that the scene is too heavy, so it is not refused: the next call goes on from
        // the room found.
        if let Some(more) = found {
            self.scratch.keep(more, now);
        }
        Err(Error::GpuTooSlow)
    }

    /// SoftPresenter: after a frame the GPU dropped. The resolver counts a picture as being in
    /// the atlas from the moment a frame's recording is made, but the commands that put it
    /// there never ran, and nothing says which did. So the atlas is let go and the resolver
    /// starts again: the next frame sends every picture it draws.
    fn forget_pictures(&mut self, device: &Device, queue: &Queue) {
        self.resolver = Resolver::new();
        if let Some(atlas) = self.image_atlas.take() {
            let mut recording = Recording::default();
            recording.free_image(atlas);
            let _ = self.engine.run_recording(
                device,
                queue,
                &recording,
                &[],
                "render_to_texture pictures forgotten",
                #[cfg(feature = "wgpu-profiler")]
                &mut self.profiler,
            );
        }
    }

    /// SoftPresenter: reads the counters the coarse stages of this frame kept, once the GPU
    /// has finished them, or gives up at `give_up_at`.
    ///
    /// The device is only ever asked how far it has got ([`wgpu::PollType::Poll`]), again and
    /// again; it is never waited in. A wait holds a lock that presenting a window and
    /// destroying a texture need, so another thread drawing with the same device would stand
    /// still meanwhile; and a wait for one submission with a time limit, while another thread
    /// submits and polls, can trip an assertion inside wgpu 30 (in `Device::maintain`: the
    /// queue is found empty with the awaited submission not yet counted as finished), which
    /// would take the calling thread down. The mapping asked for here finishes when the
    /// frame's submission has, so looking for it is all the waiting that is needed.
    fn read_bump(
        &mut self,
        device: &Device,
        bump_buf: recording::BufferProxy,
        give_up_at: std::time::Instant,
    ) -> Result<BumpAllocators> {
        use std::sync::mpsc::TryRecvError;
        let read = |engine: &WgpuEngine| -> Result<BumpAllocators> {
            let buffer = engine
                .get_download(bump_buf)
                .ok_or(Error::UnavailableBufferUsed("vello.bump_buf", "download"))?;
            let slice = buffer.slice(..size_of::<BumpAllocators>() as u64);
            let (sender, receiver) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |mapped| {
                let _ = sender.send(mapped);
            });
            loop {
                device.poll(wgpu::PollType::Poll)?;
                match receiver.try_recv() {
                    Ok(mapped) => break mapped?,
                    Err(TryRecvError::Empty) if std::time::Instant::now() < give_up_at => {
                        std::thread::sleep(GPU_LOOK);
                    }
                    Err(TryRecvError::Empty) => return Err(Error::GpuTooSlow),
                    Err(TryRecvError::Disconnected) => return Err(wgpu::BufferAsyncError.into()),
                }
            }
            let bytes = slice
                .get_mapped_range()
                .map_err(|_| wgpu::BufferAsyncError)?;
            // A buffer still as it was made was never written: the GPU dropped the frame.
            scratch::counters_read_back(&bytes).ok_or(Error::FrameDropped)
        };
        let bump = read(&self.engine);
        self.engine.free_download(bump_buf);
        bump
    }

    /// SoftPresenter: sets the longest one call to `render_to_texture` waits for the GPU before
    /// it gives the frame up with [`Error::GpuTooSlow`]; `None` is the usual `GPU_WAIT`. For
    /// tests of what happens when the GPU is too slow: with no time at all, every frame that
    /// has to be waited for is given up.
    pub fn set_longest_wait(&mut self, wait: Option<std::time::Duration>) {
        self.longest_wait = wait.unwrap_or(GPU_WAIT);
    }

    /// SoftPresenter: sets the most times one call to `render_to_texture` runs a frame before
    /// it gives it up; `None` is the usual `RUNS`. For tests of a frame that does not settle.
    pub fn set_most_runs(&mut self, runs: Option<usize>) {
        self.most_runs = runs.unwrap_or(RUNS).max(1);
    }

    /// SoftPresenter: treats the next `frames` frames as macOS treats the frames of a process
    /// whose GPU work faulted: their commands never run, and `render_to_texture` returns
    /// [`Error::FrameDropped`]. For tests of what is left behind by such a frame.
    pub fn drop_frames(&mut self, frames: u32) {
        self.frames_to_drop = frames;
    }

    /// SoftPresenter: draws every frame with these scratch sizes as they are, never reading
    /// more into them: a scene that outgrows them is refused with [`Error::SceneTooHeavy`].
    /// `None` goes back to sizes that follow the drawing.
    pub fn set_fixed_scratch(&mut self, sizes: Option<ScratchSizes>) {
        self.scratch.set_fixed(sizes);
    }

    /// SoftPresenter: call when nothing more will be drawn for a while. What a heavy frame made
    /// the scratch buffers grow to is given back to the GPU: every spare buffer if a heavy
    /// frame was drawn lately, and any large one left over from an earlier one. The sizes stay
    /// remembered for a few seconds, so drawing the same thing again soon does not have to find
    /// them out again. After light frames this does nothing.
    ///
    /// This never waits. A buffer given back is freed once the last frame that used it has
    /// left the GPU, and only when the device is next looked at, which may be long after if
    /// nothing is being drawn. That goes for buffers given back here and for those a frame
    /// gave back as it was drawn (one that was run again lets its outgrown buffers go at
    /// once). So the call returns `false` while any of that is still to come: call it again
    /// in a moment, until it returns `true`. The call that finds something was given back
    /// always returns `false`; each call after it looks at the device once.
    pub fn rest(&mut self, device: &Device, queue: &Queue) -> bool {
        use std::sync::atomic::Ordering;
        if self.scratch.is_heavy(std::time::Instant::now()) {
            self.engine.release_spare();
        } else {
            self.engine
                .release_larger_than(self.scratch.largest_resting_buffer());
        }
        // Buffers given back just now, or by any frame drawn since the last call: a frame that
        // was run again has given its outgrown buffers back already, and those are held by the
        // GPU until the device is looked at just the same.
        if self.engine.take_destroyed() {
            // The device sets this when everything sent to the GPU so far has left it.
            let left = std::sync::Arc::new(AtomicBool::new(false));
            let flag = left.clone();
            queue.on_submitted_work_done(move || flag.store(true, Ordering::Release));
            self.giving_back = Some(left);
            // The call that asks is never the one that hears the answer: the device is looked
            // at on the next call, so what this one returns does not turn on how fast the GPU
            // happens to be.
            return false;
        }
        let Some(left) = &self.giving_back else {
            return true;
        };
        let _ = device.poll(wgpu::PollType::Poll);
        if !left.load(Ordering::Acquire) {
            return false;
        }
        self.giving_back = None;
        true
    }

    /// SoftPresenter: the bytes of GPU memory this renderer keeps in spare buffers between
    /// frames.
    pub fn spare_bytes(&self) -> u64 {
        self.engine.spare_bytes()
    }

    /// SoftPresenter: the scratch room the next frame will be given, before what its own
    /// target size and scene add.
    pub fn scratch_wanted(&self) -> ScratchSizes {
        self.scratch.wanted(std::time::Instant::now())
    }

    /// SoftPresenter: how many frames were drawn without waiting for their counters, because
    /// the same scene had fitted before.
    pub fn scratch_unwaited(&self) -> u64 {
        self.scratch.unwaited()
    }

    /// SoftPresenter: how many times a frame had to be run again for want of scratch room.
    pub fn scratch_reruns(&self) -> u64 {
        self.scratch.reruns()
    }

    /// SoftPresenter: how many times the coarse stages of a frame were sent to the GPU: once
    /// for a frame that fitted, once more for each time it was run again, and not at all for
    /// a scene already known to be too heavy.
    pub fn scratch_runs(&self) -> u64 {
        self.coarse_runs
    }

    /// SoftPresenter: how long this renderer has waited for the GPU, in all, reading back the
    /// counters of the frames it checked (a frame known to fit is not waited for). A caller
    /// timing a frame subtracts this before and after to tell the GPU's time from its own.
    pub fn gpu_waited(&self) -> std::time::Duration {
        self.gpu_waited
    }

    /// Vello 0.12's own `render_to_texture`: one pass with constant scratch sizes, which does
    /// not notice a scene that outgrows them (such a scene is not drawn).
    pub fn render_to_texture_unchecked(
        &mut self,
        device: &Device,
        queue: &Queue,
        scene: &Scene,
        texture: &TextureView,
        params: &RenderParams,
    ) -> Result<()> {
        let (recording, target) = render::render_full(
            scene,
            &mut self.resolver,
            &self.shaders,
            &mut self.image_atlas,
            params,
        );
        let external_resources = [ExternalResource::Image(
            *target.as_image().unwrap(),
            texture,
        )];
        self.engine.run_recording(
            device,
            queue,
            &recording,
            &external_resources,
            "render_to_texture",
            #[cfg(feature = "wgpu-profiler")]
            &mut self.profiler,
        )?;
        // N.B. This is horrible; this integration of wgpu-profiler really needs some work...
        #[cfg(feature = "wgpu-profiler")]
        {
            self.profiler.end_frame().unwrap();
            if let Some(result) = self
                .profiler
                .process_finished_frame(queue.get_timestamp_period())
            {
                self.profile_result = Some(result);
            }
        }

        Ok(())
    }

    /// Overwrite `image` with `texture`.
    ///
    /// Most users should prefer [`register_texture`](Self::register_texture), which
    /// ergonomically encapulates these requirements.
    ///
    /// `texture` must have the [`wgpu::TextureFormat::Rgba8Unorm`] format and
    /// the [`wgpu::TextureUsages::COPY_SRC`] flag set. The `Rgba8UnormSrgb` format
    /// might also be supported, but this is not tested.
    ///
    /// The given `Texture`'s data will be copied into the slot in Vello's image
    /// atlas where the image would be placed each frame.
    /// This has the effect that wherever you request the image to be drawn (e.g.
    /// through [`Scene::draw_image`]), the data from the texture will be used instead.
    ///
    /// Correct behaviour is not guaranteed if the texture does not have the same
    /// dimensions as the image, nor if an image which uses the same [blob] but different
    /// dimensions would be rendered.
    ///
    /// [blob]: peniko::ImageData::data
    pub fn override_image(
        &mut self,
        image: &ImageData,
        texture: Option<wgpu::TexelCopyTextureInfoBase<wgpu::Texture>>,
    ) -> Option<wgpu::TexelCopyTextureInfoBase<wgpu::Texture>> {
        self.resolver.mark_image_dirty(image);
        match texture {
            Some(texture) => self.engine.image_overrides.insert(image.data.id(), texture),
            None => self.engine.image_overrides.remove(&image.data.id()),
        }
    }

    /// Marks `image` as dirty in the atlas cache, so its override texture will be recopied on
    /// the next render that uses it.
    ///
    /// Call this for any `image` whose existing override texture has changed since the last render.
    /// Otherwise, stale image data from the atlas may be used.
    pub fn mark_override_image_dirty(&mut self, image: &ImageData) {
        self.resolver.mark_image_dirty(image);
    }

    /// Register a [`wgpu::Texture`] with Vello, to allow drawing GPU-resident data.
    ///
    /// The returned `Image` can be used in [`Scene`]s (only those rendered with this `Renderer`)
    /// Rendering Scenes which use this `Image` with other `Renderer`s will panic.
    ///  
    /// `texture` must have the [`wgpu::TextureFormat::Rgba8Unorm`] format and
    /// the [`wgpu::TextureUsages::COPY_SRC`] flag set. This is because the data will
    /// be copied into Vello's image atlas at the start of each frame.
    /// The `Rgba8UnormSrgb` format might also be supported, but this is not tested.
    /// The texture is assumed to have unpremultiplied alpha.
    ///
    /// This is a utility wrapper around [`override_image`](Self::override_image).
    /// For greater control, use that method.
    ///
    /// If the texture is no longer active then it should be unregistered using [`unregister_texture`](Self::unregister_texture)
    pub fn register_texture(&mut self, texture: wgpu::Texture) -> ImageData {
        // Create a fake, empty blob which will be used to back the returned image
        // This image data will never be read by Vello, due to being added to
        // image_overrides, below.
        let fake_blob = peniko::Blob::new(std::sync::Arc::new(&[]));

        let image = ImageData {
            data: fake_blob,
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: texture.width(),
            height: texture.height(),
        };

        // For this utility API, we take the full texture and use the base layer and mip level
        let texture_base = wgpu::TexelCopyTextureInfoBase {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        };

        // We overwrite any attempt to use the fake blob, instead reading from the texture.
        self.override_image(&image, Some(texture_base));

        image
    }

    /// Unregister a [`wgpu::Texture`] that was registered with [`register_texture`](Self::register_texture).
    pub fn unregister_texture(&mut self, handle: ImageData) {
        self.override_image(&handle, None);
    }

    /// Reload the shaders. This should only be used during `vello` development
    #[cfg(feature = "hot_reload")]
    #[doc(hidden)] // End-users of Vello should not have `hot_reload` enabled.
    pub async fn reload_shaders(&mut self, device: &Device) -> Result<(), Error> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut engine = WgpuEngine::new(self.options.use_cpu, self.options.pipeline_cache.clone());
        // We choose not to initialise these shaders in parallel, to ensure the error scope works correctly
        let shaders = shaders::full_shaders(device, &mut engine, &self.options)?;
        #[cfg(feature = "debug_layers")]
        let debug = debug::DebugRenderer::new(device, wgpu::TextureFormat::Rgba8Unorm, &mut engine);
        let error = scope.pop().await;
        if let Some(error) = error {
            return Err(error.into());
        }
        self.engine = engine;
        self.image_atlas = None;
        self.shaders = shaders;
        #[cfg(feature = "debug_layers")]
        {
            self.debug = debug;
        }
        Ok(())
    }

    /// Renders a scene to the target texture using an async pipeline.
    ///
    /// Almost all consumers should prefer [`Self::render_to_texture`].
    ///
    /// The return value is the value of the `BumpAllocators` in this rendering, which is currently used
    /// for debug output.
    ///
    /// This return type is not stable, and will likely be changed when a more principled way to access
    /// relevant statistics is implemented
    #[cfg_attr(docsrs, doc(hidden))]
    #[deprecated(
        note = "render_to_texture should be preferred, as the _async version has no stability guarantees"
    )]
    pub async fn render_to_texture_async(
        &mut self,
        device: &Device,
        queue: &Queue,
        scene: &Scene,
        texture: &TextureView,
        params: &RenderParams,
        debug_layers: DebugLayers,
    ) -> Result<Option<BumpAllocators>> {
        if cfg!(not(feature = "debug_layers")) && !debug_layers.is_empty() {
            static HAS_WARNED: AtomicBool = AtomicBool::new(false);
            if !HAS_WARNED.swap(true, std::sync::atomic::Ordering::Release) {
                log::warn!(
                    "Requested debug layers {debug_layers:?} but `debug_layers` feature is not enabled"
                );
            }
        }

        let result = self
            .render_to_texture_async_internal(device, queue, scene, texture, params)
            .await?;

        #[cfg(feature = "debug_layers")]
        {
            let mut recording = Recording::default();
            let target_proxy = recording::ImageProxy::new(
                params.width,
                params.height,
                recording::ImageFormat::Rgba8,
            );
            if let Some(captured) = result.captured {
                let bump = result.bump.as_ref().unwrap();
                // TODO: We could avoid this download if `DebugLayers::VALIDATION` is unset.
                let downloads = DebugDownloads::map(&self.engine, &captured, bump).await?;
                self.debug.render(
                    &mut recording,
                    target_proxy,
                    &captured,
                    bump,
                    params,
                    &downloads,
                    debug_layers,
                );

                // TODO: this sucks. better to release everything in a helper
                // TODO: it would be much better to have a way to safely destroy a buffer.
                self.engine.free_download(captured.lines);
                captured.release_buffers(&mut recording);
            }
            let external_resources = [ExternalResource::Image(target_proxy, texture)];
            self.engine.run_recording(
                device,
                queue,
                &recording,
                &external_resources,
                "render_to_texture_async debug layers",
                #[cfg(feature = "wgpu-profiler")]
                &mut self.profiler,
            )?;
        }

        #[cfg(feature = "wgpu-profiler")]
        {
            self.profiler.end_frame().unwrap();
            if let Some(result) = self
                .profiler
                .process_finished_frame(queue.get_timestamp_period())
            {
                self.profile_result = Some(result);
            }
        }

        Ok(result.bump)
    }

    async fn render_to_texture_async_internal(
        &mut self,
        device: &Device,
        queue: &Queue,
        scene: &Scene,
        texture: &TextureView,
        params: &RenderParams,
    ) -> Result<RenderResult> {
        let mut render = Render::new();
        let encoding = scene.encoding();
        // TODO: turn this on; the download feature interacts with CPU dispatch.
        // Currently this is always enabled when the `debug_layers` setting is enabled as the bump
        // counts are used for debug visualiation.
        let robust = cfg!(feature = "debug_layers");
        let recording = render.render_encoding_coarse(
            encoding,
            &mut self.resolver,
            &self.shaders,
            &mut self.image_atlas,
            params,
            robust,
            None,
        );
        let target = render.out_image();
        let bump_buf = render.bump_buf();
        #[cfg(feature = "debug_layers")]
        let captured = render.take_captured_buffers();
        self.engine.run_recording(
            device,
            queue,
            &recording,
            &[],
            "t_async_coarse",
            #[cfg(feature = "wgpu-profiler")]
            &mut self.profiler,
        )?;

        let mut bump: Option<BumpAllocators> = None;
        if let Some(bump_buf) = self.engine.get_download(bump_buf) {
            let buf_slice = bump_buf.slice(..);
            let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
            buf_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
            receiver.receive().await.expect("channel was closed")?;
            let mapped = buf_slice.get_mapped_range().unwrap();
            bump = Some(bytemuck::pod_read_unaligned(&mapped));
        }
        // TODO: apply logic to determine whether we need to rerun coarse, and also
        // allocate the blend stack as needed.
        self.engine.free_download(bump_buf);
        // Maybe clear to reuse allocation?
        let mut recording = Recording::default();
        render.record_fine(&self.shaders, &mut recording);
        let external_resources = [ExternalResource::Image(target, texture)];
        self.engine.run_recording(
            device,
            queue,
            &recording,
            &external_resources,
            "t_async_fine",
            #[cfg(feature = "wgpu-profiler")]
            &mut self.profiler,
        )?;
        Ok(RenderResult {
            bump,
            #[cfg(feature = "debug_layers")]
            captured,
        })
    }
}
#[cfg(all(feature = "debug_layers", feature = "wgpu"))]
pub(crate) struct DebugDownloads<'a> {
    pub lines: wgpu::BufferSlice<'a>,
}

#[cfg(all(feature = "debug_layers", feature = "wgpu"))]
impl<'a> DebugDownloads<'a> {
    pub async fn map(
        engine: &'a WgpuEngine,
        captured: &render::CapturedBuffers,
        bump: &BumpAllocators,
    ) -> Result<DebugDownloads<'a>> {
        use vello_encoding::LineSoup;

        let Some(lines_buf) = engine.get_download(captured.lines) else {
            return Err(Error::DownloadError("linesoup"));
        };

        let lines = lines_buf.slice(..bump.lines as u64 * size_of::<LineSoup>() as u64);
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
        lines.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
        receiver.receive().await.expect("channel was closed")?;
        Ok(Self { lines })
    }
}
