// Copyright 2023 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferUsages, CommandEncoder, CommandEncoderDescriptor,
    ComputePassDescriptor, ComputePipeline, Device, PipelineCache, PipelineCompilationOptions,
    Queue, Texture, TextureAspect, TextureUsages, TextureView, TextureViewDimension,
};

use crate::{
    Error, Result,
    low_level::{BufferProxy, Command, ImageProxy, Recording, ResourceId, ResourceProxy, ShaderId},
    recording::BindType,
};
use vello_shaders::cpu::CpuBinding;

#[cfg(not(target_arch = "wasm32"))]
struct UninitialisedShader {
    wgsl: Cow<'static, str>,
    label: &'static str,
    entries: Vec<wgpu::BindGroupLayoutEntry>,
    shader_id: ShaderId,
}

#[derive(Default)]
pub(crate) struct WgpuEngine {
    shaders: Vec<Shader>,
    pool: ResourcePool,
    bind_map: BindMap,
    downloads: HashMap<ResourceId, Buffer>,
    #[cfg(not(target_arch = "wasm32"))]
    shaders_to_initialise: Option<Vec<UninitialisedShader>>,
    pub(crate) use_cpu: bool,
    /// Overrides from a specific `Image::data`'s [`id`](peniko::Blob::id) to a wgpu `Texture`.
    ///
    /// The `Texture` should have the same size as the `Image`.
    pub(crate) image_overrides: HashMap<u64, wgpu::TexelCopyTextureInfoBase<Texture>>,
    pipeline_cache: Option<PipelineCache>,
    /// SoftPresenter: where the time of running recordings has gone, in all (see
    /// [`PaintWork`](crate::PaintWork)).
    pub(crate) work: crate::PaintWork,
}

enum PipelineState {
    Compute(ComputePipeline),
    #[cfg(feature = "debug_layers")]
    Render(wgpu::RenderPipeline),
}

struct WgpuShader {
    pipeline: PipelineState,
    bind_group_layout: BindGroupLayout,
}

pub(crate) enum CpuShaderType {
    Present(fn(u32, &[CpuBinding<'_>])),
    Missing,
    Skipped,
}

struct CpuShader {
    shader: fn(u32, &[CpuBinding<'_>]),
}

enum ShaderKind<'a> {
    Wgpu(&'a WgpuShader),
    Cpu(&'a CpuShader),
}

struct Shader {
    label: &'static str,
    wgpu: Option<WgpuShader>,
    cpu: Option<CpuShader>,
}

impl Shader {
    fn select(&self) -> ShaderKind<'_> {
        if let Some(cpu) = self.cpu.as_ref() {
            ShaderKind::Cpu(cpu)
        } else if let Some(wgpu) = self.wgpu.as_ref() {
            ShaderKind::Wgpu(wgpu)
        } else {
            panic!("no available shader for {}", self.label)
        }
    }
}

pub(crate) enum ExternalResource<'a> {
    #[expect(unused, reason = "No buffers are accepted as arguments currently")]
    Buffer(BufferProxy, &'a Buffer),
    Image(ImageProxy, &'a TextureView),
}

/// A buffer can exist either on the GPU or on CPU.
enum MaterializedBuffer {
    Gpu(Buffer),
    Cpu(RefCell<Vec<u8>>),
}

struct BindMapBuffer {
    buffer: MaterializedBuffer,
    label: &'static str,
}

#[derive(Default)]
struct BindMap {
    buf_map: HashMap<ResourceId, BindMapBuffer>,
    image_map: HashMap<ResourceId, (Texture, TextureView)>,
    pending_clears: HashSet<ResourceId>,
    /// SoftPresenter: for a frame whose early stages run on the processor, where to read how
    /// much of its large buffers they wrote (see [`WrittenBounds`]).
    written: Option<WrittenBounds>,
    /// SoftPresenter: the large buffers of frames whose early stages ran on the processor,
    /// kept for the next frame, every byte zero again (see [`BindMap::keep`]).
    kept: Vec<Vec<u8>>,
}

/// SoftPresenter: how to tell, for a frame whose early stages run on the processor, how much of
/// its five large buffers those stages wrote, from the counters they kept (the frame's bump
/// buffer, which stays in the processor's memory): the lines, tiles, segment counts, path
/// segments and per-tile command lists, 165 MB at Vello's constant sizes and nearly all of it
/// never touched. Upstream makes each of them anew, zeroed, for every frame, and this copy
/// then looked through the two the last stage reads for their last byte that is not zero;
/// together that was nearly all of a picture's time, whatever its size (on a 2015 Intel iMac,
/// 43 ms for an empty picture: 28 ms looking through the two, about 11 ms zeroing the five).
#[derive(Clone, Copy)]
pub(crate) struct WrittenBounds {
    /// The frame's counters ([`vello_encoding::BumpAllocators`]).
    pub(crate) bump: ResourceId,
    /// The lines the paths were flattened into.
    pub(crate) lines: ResourceId,
    /// The tiles given out to the paths.
    pub(crate) tiles: ResourceId,
    /// The segment counts.
    pub(crate) seg_counts: ResourceId,
    /// The path segments.
    pub(crate) segments: ResourceId,
    /// The per-tile command lists: each tile's first part, then the parts given out after.
    pub(crate) ptcl: ResourceId,
    /// The words of the command lists that every frame of this size has (the tiles' first
    /// parts), before the ones the counters count.
    pub(crate) ptcl_fixed_words: u64,
}

impl BindMap {
    /// SoftPresenter: the bytes at the start of a buffer that the stages on the processor may
    /// have written, when the frame's counters say (see [`WrittenBounds`]): everything after
    /// them is zeros still. `None` for a buffer the counters say nothing of, and once the
    /// counters have gone.
    fn known_written(&self, id: ResourceId) -> Option<usize> {
        use vello_encoding::{BumpAllocators, LineSoup, PathSegment, SegmentCount, Tile};
        let bounds = self.written.as_ref()?;
        let MaterializedBuffer::Cpu(bump) = &self.buf_map.get(&bounds.bump)?.buffer else {
            return None;
        };
        let bump = bump.borrow();
        let bump: BumpAllocators =
            bytemuck::pod_read_unaligned(bump.get(..size_of::<BumpAllocators>())?);
        let (count, each) = if id == bounds.lines {
            (u64::from(bump.lines), size_of::<LineSoup>())
        } else if id == bounds.tiles {
            (u64::from(bump.tile), size_of::<Tile>())
        } else if id == bounds.seg_counts {
            (u64::from(bump.seg_counts), size_of::<SegmentCount>())
        } else if id == bounds.segments {
            (u64::from(bump.segments), size_of::<PathSegment>())
        } else if id == bounds.ptcl {
            (
                bounds.ptcl_fixed_words + u64::from(bump.ptcl),
                size_of::<u32>(),
            )
        } else {
            return None;
        };
        usize::try_from(count * each as u64).ok()
    }

    /// SoftPresenter: keeps a buffer the stages on the processor are done with for the next
    /// frame, with its first `written` bytes, the only ones that may be other than zero (see
    /// [`BindMap::known_written`]), made zero again: a buffer always starts as zeros, as one
    /// made anew does.
    fn keep(&mut self, mut bytes: Vec<u8>, written: usize, name: &str) {
        let written = written.min(bytes.len());
        debug_assert!(
            bytes[written..].iter().all(|byte| *byte == 0),
            "{name}: the counters say {written} bytes were written, but there is more"
        );
        bytes[..written].fill(0);
        // Never more than a frame's five (a frame that came with other sizes left its own).
        if self.kept.len() >= KEPT_ON_PROCESSOR {
            self.kept.remove(0);
        }
        self.kept.push(bytes);
    }
}

/// SoftPresenter: how many buffers are kept between frames whose early stages run on the
/// processor: the five the counters bound (see [`WrittenBounds`]).
const KEPT_ON_PROCESSOR: usize = 5;

#[derive(Hash, PartialEq, Eq)]
struct BufferProperties {
    size: u64,
    usages: BufferUsages,
    name: &'static str,
}

#[derive(Default)]
pub(crate) struct ResourcePool {
    // SoftPresenter: each spare buffer with the time it was handed back.
    bufs: HashMap<BufferProperties, Vec<(Buffer, Instant)>>,
    // SoftPresenter: true when a buffer has been destroyed since this was last asked about
    // (`WgpuEngine::take_destroyed`). The GPU frees a destroyed buffer only once the frames
    // that used it have left it and the device is looked at.
    destroyed: bool,
}

/// The transient bind map contains short-lifetime resources.
///
/// In particular, it has resources scoped to a single call of
/// `run_recording()`, including external resources and also buffer
/// uploads.
#[derive(Default)]
struct TransientBindMap<'a> {
    bufs: HashMap<ResourceId, TransientBuf<'a>>,
    // TODO: create transient image type
    images: HashMap<ResourceId, &'a TextureView>,
}

enum TransientBuf<'a> {
    Cpu(&'a [u8]),
    Gpu(&'a Buffer),
}

impl WgpuEngine {
    pub fn new(use_cpu: bool, pipeline_cache: Option<PipelineCache>) -> Self {
        Self {
            use_cpu,
            pipeline_cache,
            ..Default::default()
        }
    }

    /// Enable creating any remaining shaders in parallel
    #[cfg(not(target_arch = "wasm32"))]
    pub fn use_parallel_initialisation(&mut self) {
        if self.shaders_to_initialise.is_some() {
            return;
        }
        self.shaders_to_initialise = Some(Vec::new());
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Initialise (in parallel) any shaders which are yet to be created
    pub fn build_shaders_if_needed(
        &mut self,
        device: &Device,
        num_threads: Option<std::num::NonZeroUsize>,
    ) {
        use std::num::NonZeroUsize;

        if let Some(mut new_shaders) = self.shaders_to_initialise.take() {
            let num_threads = num_threads
                .map(NonZeroUsize::get)
                .unwrap_or_else(|| {
                    // Fallback onto a heuristic. This tries to not to use all threads.
                    // We keep the main thread blocked and not doing much whilst this is running,
                    // so we broadly leave two cores unused at the point of maximum parallelism
                    // (This choice is arbitrary, and could be tuned, although a 'proper' threadpool
                    // should probably be used instead)
                    std::thread::available_parallelism().map_or(2, |it| it.get().max(4) - 2)
                })
                .min(new_shaders.len());
            log::info!("Initialising shader modules in parallel using {num_threads} threads");
            let remainder = new_shaders.split_off(num_threads);
            let (tx, rx) = std::sync::mpsc::channel::<(ShaderId, WgpuShader)>();

            // We expect each initialisation to take much longer than acquiring a lock, so we just
            // use a mutex for our work queue
            let work_queue = std::sync::Mutex::new(remainder.into_iter());
            let work_queue = &work_queue;
            std::thread::scope(|scope| {
                let pipeline_cache = self.pipeline_cache.as_ref();
                let tx = tx;
                new_shaders
                    .into_iter()
                    .map(|it| {
                        let tx = tx.clone();
                        std::thread::Builder::new()
                            .name("Vello shader initialisation worker thread".into())
                            .spawn_scoped(scope, move || {
                                let shader = Self::create_compute_pipeline(
                                    device,
                                    it.label,
                                    it.wgsl,
                                    it.entries,
                                    pipeline_cache,
                                );
                                // We know the rx can only be closed if all the tx references are dropped
                                tx.send((it.shader_id, shader)).unwrap();
                                while let Ok(mut guard) = work_queue.lock() {
                                    if let Some(value) = guard.next() {
                                        drop(guard);
                                        let shader = Self::create_compute_pipeline(
                                            device,
                                            value.label,
                                            value.wgsl,
                                            value.entries,
                                            pipeline_cache,
                                        );
                                        tx.send((value.shader_id, shader)).unwrap();
                                    } else {
                                        break;
                                    }
                                }
                                // Another thread panicked or we finished.
                                // If another thread panicked, we ignore that here and finish our processing
                                drop(tx);
                            })
                            .expect("failed to spawn thread");
                    })
                    .for_each(drop);
                // Drop the initial sender, to mean that there will be no more senders if and only if all other threads have finished
                drop(tx);

                while let Ok((id, value)) = rx.recv() {
                    self.shaders[id.0].wgpu = Some(value);
                }
            });
        }
    }

    /// Add a shader.
    ///
    /// This function is somewhat limited, it doesn't apply a label, only allows one bind group,
    /// doesn't support push constants, and entry point is hardcoded as "main".
    ///
    /// Maybe should do template instantiation here? But shader compilation pipeline feels maybe
    /// a bit separate.
    pub fn add_compute_shader(
        &mut self,
        device: &Device,
        label: &'static str,
        wgsl: Cow<'static, str>,
        layout: &[BindType],
        cpu_shader: CpuShaderType,
    ) -> ShaderId {
        let mut add = |shader| {
            let id = self.shaders.len();
            self.shaders.push(shader);
            ShaderId(id)
        };

        if self.use_cpu {
            match cpu_shader {
                CpuShaderType::Present(shader) => {
                    return add(Shader {
                        wgpu: None,
                        cpu: Some(CpuShader { shader }),
                        label,
                    });
                }
                // This shader is unused in CPU mode, create a dummy shader
                CpuShaderType::Skipped => {
                    return add(Shader {
                        wgpu: None,
                        cpu: None,
                        label,
                    });
                }
                // Create a GPU shader as we don't have a CPU shader
                CpuShaderType::Missing => {}
            }
        }

        let entries = Self::create_bind_group_layout_entries(
            layout.iter().map(|b| (*b, wgpu::ShaderStages::COMPUTE)),
        );
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(uninit) = self.shaders_to_initialise.as_mut() {
            let id = add(Shader {
                label,
                wgpu: None,
                cpu: None,
            });
            uninit.push(UninitialisedShader {
                wgsl,
                label,
                entries,
                shader_id: id,
            });
            return id;
        }
        let wgpu = Self::create_compute_pipeline(
            device,
            label,
            wgsl,
            entries,
            self.pipeline_cache.as_ref(),
        );
        add(Shader {
            wgpu: Some(wgpu),
            cpu: None,
            label,
        })
    }

    #[cfg(feature = "debug_layers")]
    pub fn add_render_shader(
        &mut self,
        device: &Device,
        label: &'static str,
        module: &wgpu::ShaderModule,
        vertex_main: &'static str,
        fragment_main: &'static str,
        topology: wgpu::PrimitiveTopology,
        color_attachment: wgpu::ColorTargetState,
        vertex_buffer: Option<wgpu::VertexBufferLayout<'_>>,
        bind_layout: &[(BindType, wgpu::ShaderStages)],
    ) -> ShaderId {
        let entries = Self::create_bind_group_layout_entries(bind_layout.iter().copied());
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(vertex_main),
                buffers: &[vertex_buffer],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fragment_main),
                targets: &[Some(color_attachment)],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: self.pipeline_cache.as_ref(),
        });
        let id = self.shaders.len();
        self.shaders.push(Shader {
            wgpu: Some(WgpuShader {
                pipeline: PipelineState::Render(pipeline),
                bind_group_layout,
            }),
            cpu: None,
            label,
        });
        ShaderId(id)
    }

    pub fn run_recording(
        &mut self,
        device: &Device,
        queue: &Queue,
        recording: &Recording,
        external_resources: &[ExternalResource<'_>],
        label: &'static str,
        #[cfg(feature = "wgpu-profiler")] profiler: &mut wgpu_profiler::GpuProfiler,
    ) -> Result<()> {
        // SoftPresenter: the recording's time is counted, and within it the parts that can be
        // slow for a picture: making the picture atlas, sending pixels, handing over to the GPU.
        let began = Instant::now();
        let before = self.work;
        let mut free_bufs: HashSet<ResourceId> = HashSet::default();
        let mut free_images: HashSet<ResourceId> = HashSet::default();
        let mut transient_map = TransientBindMap::new(external_resources);

        let mut encoder =
            device.create_command_encoder(&CommandEncoderDescriptor { label: Some(label) });
        #[cfg(feature = "wgpu-profiler")]
        let query = profiler.begin_query(label, &mut encoder);
        for command in &recording.commands {
            match command {
                Command::Upload(buf_proxy, bytes) => {
                    transient_map
                        .bufs
                        .insert(buf_proxy.id, TransientBuf::Cpu(bytes));
                    // TODO: restrict VERTEX usage to "debug_layers" feature?
                    let usage = BufferUsages::COPY_SRC
                        | BufferUsages::COPY_DST
                        | BufferUsages::STORAGE
                        | BufferUsages::VERTEX;
                    let buf = self
                        .pool
                        .get_buf(buf_proxy.size, buf_proxy.name, usage, device);
                    // TODO: if buffer is newly created, might be better to make it mapped at creation
                    // and copy. However, we expect reuse will be most common.
                    queue.write_buffer(&buf, 0, bytes);
                    self.bind_map.insert_buf(buf_proxy, buf);
                }
                Command::UploadUniform(buf_proxy, bytes) => {
                    transient_map
                        .bufs
                        .insert(buf_proxy.id, TransientBuf::Cpu(bytes));
                    let usage = BufferUsages::UNIFORM | BufferUsages::COPY_DST;
                    // Same consideration as above
                    let buf = self
                        .pool
                        .get_buf(buf_proxy.size, buf_proxy.name, usage, device);
                    queue.write_buffer(&buf, 0, bytes);
                    self.bind_map.insert_buf(buf_proxy, buf);
                }
                Command::UploadImage(image_proxy, bytes) => {
                    let format = image_proxy.format.to_wgpu();
                    let block_size = format
                        .block_copy_size(None)
                        .expect("ImageFormat must have a valid block size");
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size: wgpu::Extent3d {
                            width: image_proxy.width,
                            height: image_proxy.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                        format,
                        view_formats: &[],
                    });
                    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
                        label: None,
                        dimension: Some(TextureViewDimension::D2),
                        usage: None,
                        aspect: TextureAspect::All,
                        mip_level_count: None,
                        base_mip_level: 0,
                        base_array_layer: 0,
                        array_layer_count: None,
                        format: Some(format),
                    });
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                            aspect: TextureAspect::All,
                        },
                        bytes,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(image_proxy.width * block_size),
                            rows_per_image: None,
                        },
                        wgpu::Extent3d {
                            width: image_proxy.width,
                            height: image_proxy.height,
                            depth_or_array_layers: 1,
                        },
                    );
                    self.bind_map
                        .insert_image(image_proxy.id, texture, texture_view);
                }
                Command::WriteImage(proxy, [x, y], image) => {
                    // SoftPresenter: making the atlas is timed.
                    let making = (!self.bind_map.image_map.contains_key(&proxy.id))
                        .then(Instant::now);
                    let (texture, _) = self.bind_map.get_or_create_image(*proxy, device);
                    if let Some(making) = making {
                        self.work.atlas_making += making.elapsed();
                        self.work.atlases_made += 1;
                        self.work.atlas_bytes_made +=
                            u64::from(proxy.width) * u64::from(proxy.height) * 4;
                    }
                    let format = proxy.format.to_wgpu();
                    let block_size = format
                        .block_copy_size(None)
                        .expect("ImageFormat must have a valid block size");
                    if let Some(overrider) = self.image_overrides.get(&image.data.id()) {
                        encoder.copy_texture_to_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &overrider.texture,
                                mip_level: overrider.mip_level,
                                origin: overrider.origin,
                                aspect: overrider.aspect,
                            },
                            wgpu::TexelCopyTextureInfo {
                                texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d { x: *x, y: *y, z: 0 },
                                aspect: TextureAspect::All,
                            },
                            wgpu::Extent3d {
                                width: image.width,
                                height: image.height,
                                depth_or_array_layers: 1,
                            },
                        );
                        self.work.pictures_copied += 1;
                    } else {
                        if image.data.is_empty() && image.width != 0 && image.height != 0 {
                            panic!(
                                "Tried to draw an invalid empty image (id: {}). \
                                Maybe it was registered to a different renderer, or \
                                unregistered before this render was submitted.",
                                image.data.id()
                            );
                        }
                        // SoftPresenter: sending a picture's pixels is timed.
                        let sending = Instant::now();
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d { x: *x, y: *y, z: 0 },
                                aspect: TextureAspect::All,
                            },
                            image.data.data(),
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(image.width * block_size),
                                rows_per_image: None,
                            },
                            wgpu::Extent3d {
                                width: image.width,
                                height: image.height,
                                depth_or_array_layers: 1,
                            },
                        );
                        self.work.sending += sending.elapsed();
                        self.work.pictures_sent += 1;
                        self.work.bytes_sent += image.data.data().len() as u64;
                    }
                }
                Command::Dispatch(shader_id, wg_size, bindings) => {
                    let (x, y, z) = *wg_size;
                    // println!("dispatching {:?} with {} bindings", wg_size, bindings.len());
                    let shader = &self.shaders[shader_id.0];
                    match shader.select() {
                        ShaderKind::Cpu(cpu_shader) => {
                            // The current strategy is to run the CPU shader synchronously. This
                            // works because there is currently the added constraint that data
                            // can only flow from CPU to GPU, not the other way around. If and
                            // when we implement that, we will need to defer the execution. Of
                            // course, we will also need to wire up more async synchronization
                            // mechanisms, as the CPU dispatch can't run until the preceding
                            // command buffer submission completes (and, in WebGPU, the async
                            // mapping operations on the buffers completes).
                            let resources =
                                transient_map.create_cpu_resources(&mut self.bind_map, bindings);
                            (cpu_shader.shader)(x, &resources);
                        }
                        ShaderKind::Wgpu(wgpu_shader) => {
                            // Workaround for https://github.com/linebender/vello/issues/637
                            if x == 0 || y == 0 || z == 0 {
                                continue;
                            }
                            let bind_group = transient_map.create_bind_group(
                                &mut self.bind_map,
                                &mut self.pool,
                                device,
                                queue,
                                &mut encoder,
                                &wgpu_shader.bind_group_layout,
                                bindings,
                            );
                            let mut cpass =
                                encoder.begin_compute_pass(&ComputePassDescriptor::default());
                            #[cfg(feature = "wgpu-profiler")]
                            let query = profiler
                                .begin_query(shader.label, &mut cpass)
                                .with_parent(Some(&query));
                            #[cfg_attr(
                                not(feature = "debug_layers"),
                                expect(
                                    irrefutable_let_patterns,
                                    reason = "Render shaders are only enabled if we have the debug pipeline"
                                )
                            )]
                            let PipelineState::Compute(pipeline) = &wgpu_shader.pipeline else {
                                panic!("cannot issue a dispatch with a render pipeline");
                            };
                            cpass.set_pipeline(pipeline);
                            cpass.set_bind_group(0, &bind_group, &[]);
                            cpass.dispatch_workgroups(x, y, z);
                            #[cfg(feature = "wgpu-profiler")]
                            profiler.end_query(&mut cpass, query);
                        }
                    }
                }
                Command::DispatchIndirect(shader_id, proxy, offset, bindings) => {
                    let shader = &self.shaders[shader_id.0];
                    match shader.select() {
                        ShaderKind::Cpu(cpu_shader) => {
                            // Same consideration as above about running the CPU shader synchronously.
                            let n_wg;
                            if let CpuBinding::BufferRW(b) = self.bind_map.get_cpu_buf(proxy.id) {
                                let slice = b.borrow();
                                let indirect: &[u32] = bytemuck::cast_slice(&slice);
                                n_wg = indirect[0];
                            } else {
                                panic!("indirect buffer missing from bind map");
                            }
                            let resources =
                                transient_map.create_cpu_resources(&mut self.bind_map, bindings);
                            (cpu_shader.shader)(n_wg, &resources);
                        }
                        ShaderKind::Wgpu(wgpu_shader) => {
                            let bind_group = transient_map.create_bind_group(
                                &mut self.bind_map,
                                &mut self.pool,
                                device,
                                queue,
                                &mut encoder,
                                &wgpu_shader.bind_group_layout,
                                bindings,
                            );
                            transient_map.materialize_gpu_buf_for_indirect(
                                &mut self.bind_map,
                                &mut self.pool,
                                device,
                                queue,
                                proxy,
                            );
                            let mut cpass =
                                encoder.begin_compute_pass(&ComputePassDescriptor::default());
                            #[cfg(feature = "wgpu-profiler")]
                            let query = profiler
                                .begin_query(shader.label, &mut cpass)
                                .with_parent(Some(&query));
                            #[cfg_attr(
                                not(feature = "debug_layers"),
                                expect(
                                    irrefutable_let_patterns,
                                    reason = "Render shaders are only enabled if we have the debug pipeline"
                                )
                            )]
                            let PipelineState::Compute(pipeline) = &wgpu_shader.pipeline else {
                                panic!("cannot issue a dispatch with a render pipeline");
                            };
                            cpass.set_pipeline(pipeline);
                            cpass.set_bind_group(0, &bind_group, &[]);
                            let buf = self.bind_map.get_gpu_buf(proxy.id).ok_or(
                                Error::UnavailableBufferUsed(proxy.name, "indirect dispatch"),
                            )?;
                            cpass.dispatch_workgroups_indirect(buf, *offset);
                            #[cfg(feature = "wgpu-profiler")]
                            profiler.end_query(&mut cpass, query);
                        }
                    }
                }
                #[cfg(feature = "debug_layers")]
                Command::Draw(draw_params) => {
                    let shader = &self.shaders[draw_params.shader_id.0];
                    #[cfg(feature = "wgpu-profiler")]
                    let label = shader.label;
                    let ShaderKind::Wgpu(shader) = shader.select() else {
                        panic!("a render pass does not have a CPU equivalent");
                    };
                    let bind_group = transient_map.create_bind_group(
                        &mut self.bind_map,
                        &mut self.pool,
                        device,
                        queue,
                        &mut encoder,
                        &shader.bind_group_layout,
                        &draw_params.resources,
                    );
                    let render_target = transient_map
                        .materialize_external_image_for_render_pass(&draw_params.target);
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: render_target,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: match draw_params.clear_color {
                                    Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                                        r: c[0] as f64,
                                        g: c[1] as f64,
                                        b: c[2] as f64,
                                        a: c[3] as f64,
                                    }),
                                    None => wgpu::LoadOp::Load,
                                },
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });
                    #[cfg(feature = "wgpu-profiler")]
                    let query = profiler
                        .begin_query(label, &mut rpass)
                        .with_parent(Some(&query));
                    let PipelineState::Render(pipeline) = &shader.pipeline else {
                        panic!("cannot issue a draw with a compute pipeline");
                    };
                    rpass.set_pipeline(pipeline);
                    if let Some(proxy) = draw_params.vertex_buffer {
                        // TODO: need a way to materialize a CPU initialized buffer. For now assume
                        // buffer exists? Also, need to materialize this buffer with vertex usage
                        let buf = self
                            .bind_map
                            .get_gpu_buf(proxy.id)
                            .ok_or(Error::UnavailableBufferUsed(proxy.name, "draw"))?;
                        rpass.set_vertex_buffer(0, buf.slice(..));
                    }
                    rpass.set_bind_group(0, &bind_group, &[]);
                    rpass.draw(0..draw_params.vertex_count, 0..draw_params.instance_count);
                    #[cfg(feature = "wgpu-profiler")]
                    profiler.end_query(&mut rpass, query);
                }
                Command::Download(proxy) => {
                    let src_buf = self
                        .bind_map
                        .get_gpu_buf(proxy.id)
                        .ok_or(Error::UnavailableBufferUsed(proxy.name, "download"))?;
                    // SoftPresenter: a buffer of its own, marked as not yet written, in place of
                    // one from the pool (see `unread_download`).
                    let buf = unread_download(device, proxy.size)?;
                    encoder.copy_buffer_to_buffer(src_buf, 0, &buf, 0, proxy.size);
                    self.downloads.insert(proxy.id, buf);
                }
                Command::Clear(proxy, offset, size) => {
                    if let Some(buf) = self.bind_map.get_buf(*proxy) {
                        match &buf.buffer {
                            MaterializedBuffer::Gpu(b) => encoder.clear_buffer(b, *offset, *size),
                            MaterializedBuffer::Cpu(b) => {
                                let mut slice = &mut b.borrow_mut()[*offset as usize..];
                                if let Some(size) = size {
                                    slice = &mut slice[..*size as usize];
                                }
                                slice.fill(0);
                            }
                        }
                    } else {
                        self.bind_map.pending_clears.insert(proxy.id);
                    }
                }
                Command::FreeBuffer(proxy) => {
                    free_bufs.insert(proxy.id);
                }
                Command::FreeImage(proxy) => {
                    free_images.insert(proxy.id);
                }
            }
        }
        #[cfg(feature = "wgpu-profiler")]
        profiler.end_query(&mut encoder, query);
        // TODO: This only actually needs to happen once per frame, but run_recording happens two or three times
        #[cfg(feature = "wgpu-profiler")]
        profiler.resolve_queries(&mut encoder);
        let handing = Instant::now();
        queue.submit(Some(encoder.finish()));
        self.work.handing_over += handing.elapsed();
        // SoftPresenter: what the frame's counters say of the buffers about to go, asked for
        // while the counters are still there (they go with them).
        let written: Vec<(ResourceId, usize)> = free_bufs
            .iter()
            .filter_map(|id| Some((*id, self.bind_map.known_written(*id)?)))
            .collect();
        for (id, written) in written {
            if let Some(buf) = self.bind_map.buf_map.remove(&id) {
                match buf.buffer {
                    MaterializedBuffer::Cpu(bytes) => {
                        self.bind_map.keep(bytes.into_inner(), written, buf.label);
                    }
                    MaterializedBuffer::Gpu(_) => {
                        self.bind_map.buf_map.insert(id, buf);
                    }
                }
            }
        }
        for id in free_bufs {
            if let Some(buf) = self.bind_map.buf_map.remove(&id)
                && let MaterializedBuffer::Gpu(gpu_buf) = buf.buffer
            {
                let props = BufferProperties {
                    size: gpu_buf.size(),
                    usages: gpu_buf.usage(),
                    name: buf.label,
                };
                self.pool
                    .bufs
                    .entry(props)
                    .or_default()
                    .push((gpu_buf, Instant::now()));
            }
        }
        for id in free_images {
            if let Some((texture, _view)) = self.bind_map.image_map.remove(&id) {
                // TODO: have a pool to avoid needless re-allocation
                texture.destroy();
            }
        }
        // SoftPresenter: what is left of the recording's time once the parts are taken out.
        let parts = (self.work.atlas_making - before.atlas_making)
            + (self.work.sending - before.sending)
            + (self.work.handing_over - before.handing_over);
        self.work.recording += began.elapsed().saturating_sub(parts);
        Ok(())
    }

    // SoftPresenter: the three functions below are additions.
    /// Gives back to the GPU every spare buffer that no drawing has used for `idle`, and
    /// every one of more than `large` bytes that none has used for `large_idle`.
    pub(crate) fn release_idle(
        &mut self,
        now: Instant,
        idle: Duration,
        large: u64,
        large_idle: Duration,
    ) {
        self.pool.release(|size, since| {
            let unused = now.saturating_duration_since(since);
            unused >= idle || (size > large && unused >= large_idle)
        });
    }

    /// Gives back to the GPU every spare buffer of more than `bytes`.
    pub(crate) fn release_larger_than(&mut self, bytes: u64) {
        self.pool.release(|size, _| size > bytes);
    }

    /// Gives back to the GPU every spare buffer.
    pub(crate) fn release_spare(&mut self) {
        self.pool.release(|_, _| true);
    }

    /// True when a spare buffer was given back to the GPU since this was last called, by any
    /// of the functions above, whoever called them.
    pub(crate) fn take_destroyed(&mut self) -> bool {
        std::mem::take(&mut self.pool.destroyed)
    }

    /// SoftPresenter: lets go of everything a recording still holds: its buffers, the clears
    /// owed to buffers not made yet, and its textures. For a recording that was stopped
    /// part-way (a stage run on the processor panicked), whose own commands to free them
    /// never ran. Between frames the engine holds only the picture atlas, which the caller
    /// has let go by then (`Renderer::forget_pictures`), so nothing another frame needs is
    /// lost. The textures are dropped, not destroyed: pixels sent to one are still on their
    /// way to it until the next commands are handed over.
    pub(crate) fn forget_stopped_recording(&mut self) {
        self.bind_map.written = None;
        // A stage that stopped part-way may have written past what its counters say.
        self.bind_map.kept.clear();
        self.bind_map.buf_map.clear();
        self.bind_map.pending_clears.clear();
        self.bind_map.image_map.clear();
    }

    /// SoftPresenter: says where the frame run next keeps its counters, for a frame whose
    /// early stages run on the processor (see [`WrittenBounds`]); `None` once it has run.
    pub(crate) fn set_written_bounds(&mut self, bounds: Option<WrittenBounds>) {
        self.bind_map.written = bounds;
    }

    /// SoftPresenter: lets go of the buffers kept in the processor's memory between frames
    /// whose early stages run there, when nothing will be drawn for a while: the next frame
    /// makes its own again.
    pub(crate) fn forget_kept(&mut self) {
        self.bind_map.kept = Vec::new();
    }

    /// The bytes of GPU memory held in spare buffers.
    pub(crate) fn spare_bytes(&self) -> u64 {
        self.pool
            .bufs
            .iter()
            .map(|(props, spare)| props.size * spare.len() as u64)
            .sum()
    }

    pub fn get_download(&self, buf: BufferProxy) -> Option<&Buffer> {
        self.downloads.get(&buf.id)
    }

    pub fn free_download(&mut self, buf: BufferProxy) {
        self.downloads.remove(&buf.id);
    }

    fn create_bind_group_layout_entries(
        layout: impl Iterator<Item = (BindType, wgpu::ShaderStages)>,
    ) -> Vec<wgpu::BindGroupLayoutEntry> {
        layout
            .enumerate()
            .map(|(i, (bind_type, visibility))| match bind_type {
                BindType::Buffer | BindType::BufReadOnly => wgpu::BindGroupLayoutEntry {
                    binding: i as u32,
                    visibility,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: bind_type == BindType::BufReadOnly,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindType::Uniform => wgpu::BindGroupLayoutEntry {
                    binding: i as u32,
                    visibility,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindType::Image(format) | BindType::ImageRead(format) => {
                    wgpu::BindGroupLayoutEntry {
                        binding: i as u32,
                        visibility,
                        ty: if bind_type == BindType::ImageRead(format) {
                            wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: TextureViewDimension::D2,
                                multisampled: false,
                            }
                        } else {
                            wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: format.to_wgpu(),
                                view_dimension: TextureViewDimension::D2,
                            }
                        },
                        count: None,
                    }
                }
            })
            .collect::<Vec<_>>()
    }

    fn create_compute_pipeline(
        device: &Device,
        label: &str,
        wgsl: Cow<'_, str>,
        entries: Vec<wgpu::BindGroupLayoutEntry>,
        cache: Option<&PipelineCache>,
    ) -> WgpuShader {
        // SAFETY: We only call this with trusted shaders (written by Vello developers)
        //
        // SoftPresenter: with bounds checks, where upstream has none. The stages do not all
        // stop writing when their scratch buffer is full (`tile_alloc` clears the tiles it could
        // not allocate; `path_tiling` trusts that the segments fit), and upstream's constant
        // sizes only make that rare. With sizes that follow the drawing a frame runs out of room
        // as a matter of course, and an unchecked write past a buffer faults the GPU: macOS then
        // drops the frame, restarts the GPU under every app, and after a few such faults ignores
        // the process. Checked, such a write lands inside the buffer of a frame that is thrown
        // away and run again anyway. The other checks stay off, as upstream has them: the
        // shaders are still Vello's own.
        let shader_module = unsafe {
            device.create_shader_module_trusted(
                wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl(wgsl),
                },
                wgpu::ShaderRuntimeChecks {
                    bounds_checks: true,
                    ..wgpu::ShaderRuntimeChecks::unchecked()
                },
            )
        };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&compute_pipeline_layout),
            module: &shader_module,
            entry_point: None,
            compilation_options: PipelineCompilationOptions {
                zero_initialize_workgroup_memory: false,
                ..Default::default()
            },
            cache,
        });
        WgpuShader {
            pipeline: PipelineState::Compute(pipeline),
            bind_group_layout,
        }
    }
}

impl BindMap {
    fn insert_buf(&mut self, proxy: &BufferProxy, buffer: Buffer) {
        self.buf_map.insert(
            proxy.id,
            BindMapBuffer {
                buffer: MaterializedBuffer::Gpu(buffer),
                label: proxy.name,
            },
        );
    }

    /// Get a buffer, only if it's on GPU.
    fn get_gpu_buf(&self, id: ResourceId) -> Option<&Buffer> {
        self.buf_map.get(&id).and_then(|b| match &b.buffer {
            MaterializedBuffer::Gpu(b) => Some(b),
            _ => None,
        })
    }

    /// Get a CPU buffer.
    ///
    /// Panics if buffer is not present or is on GPU.
    fn get_cpu_buf(&self, id: ResourceId) -> CpuBinding<'_> {
        match &self.buf_map[&id].buffer {
            MaterializedBuffer::Cpu(b) => CpuBinding::BufferRW(b),
            _ => panic!("getting cpu buffer, but it's on gpu"),
        }
    }

    fn materialize_cpu_buf(&mut self, buf: &BufferProxy) {
        self.buf_map.entry(buf.id).or_insert_with(|| {
            // SoftPresenter: one kept from the frame before, all zeros, when there is one
            // of this size (see `BindMap::keep`): making 165 MB of zeros anew took longer
            // than drawing an ordinary slide.
            let size = buf.size as usize;
            let bytes = match self.kept.iter().position(|kept| kept.len() == size) {
                Some(at) => self.kept.swap_remove(at),
                None => vec![0; size],
            };
            let buffer = MaterializedBuffer::Cpu(RefCell::new(bytes));
            BindMapBuffer {
                buffer,
                // TODO: do we need to cfg this?
                label: buf.name,
            }
        });
    }

    fn insert_image(&mut self, id: ResourceId, image: Texture, image_view: TextureView) {
        self.image_map.insert(id, (image, image_view));
    }

    fn get_buf(&mut self, proxy: BufferProxy) -> Option<&BindMapBuffer> {
        self.buf_map.get(&proxy.id)
    }

    fn get_or_create_image(
        &mut self,
        proxy: ImageProxy,
        device: &Device,
    ) -> &(Texture, TextureView) {
        match self.image_map.entry(proxy.id) {
            Entry::Occupied(occupied) => occupied.into_mut(),
            Entry::Vacant(vacant) => {
                let format = proxy.format.to_wgpu();
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: proxy.width,
                        height: proxy.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                    format,
                    view_formats: &[],
                });
                let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
                    label: None,
                    usage: None,
                    dimension: Some(TextureViewDimension::D2),
                    aspect: TextureAspect::All,
                    mip_level_count: None,
                    base_mip_level: 0,
                    base_array_layer: 0,
                    array_layer_count: None,
                    format: Some(proxy.format.to_wgpu()),
                });
                vacant.insert((texture, texture_view))
            }
        }
    }
}

/// SoftPresenter: an addition. A buffer for the GPU to copy something into and the CPU to read,
/// every byte of it set to [`UNREAD`](crate::scratch::UNREAD) first.
///
/// A copy the GPU never makes (macOS drops a frame's commands when the GPU faults, and all of a
/// process's commands after a few faults) leaves the buffer as it was made. A new buffer is all
/// zeros, which for the counters of a frame reads as "nothing ran out of room", and one from
/// the pool would still hold the counters of an earlier frame. The bytes are written while the
/// buffer is being made, on the CPU, so they are there whether or not the GPU ever runs
/// anything again.
pub(crate) fn unread_download(device: &Device, size: u64) -> Result<Buffer> {
    // A buffer mapped as it is made must be a whole number of words.
    let size = size.max(1).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("download"),
        size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    let unread = vec![crate::scratch::UNREAD; size as usize];
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .map_err(|_| Error::UnavailableBufferUsed("download", "marking as unread"))?
        .copy_from_slice(&unread);
    buffer.unmap();
    Ok(buffer)
}

// SoftPresenter: 2, not 1: buffers are made at 1, 1.25, 1.5 and 1.75 times a power of two, so
// a large scratch buffer is at most a quarter larger than was asked for, not a half.
pub(crate) const SIZE_CLASS_BITS: u32 = 2;

impl ResourcePool {
    /// Get a buffer from the pool or create one.
    fn get_buf(
        &mut self,
        size: u64,
        name: &'static str,
        usage: BufferUsages,
        device: &Device,
    ) -> Buffer {
        let rounded_size = Self::size_class(size, SIZE_CLASS_BITS);
        let props = BufferProperties {
            size: rounded_size,
            usages: usage,
            name,
        };
        if let Some(buf_vec) = self.bufs.get_mut(&props)
            && let Some((buf, _)) = buf_vec.pop()
        {
            return buf;
        }
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(name),
            size: rounded_size,
            usage,
            mapped_at_creation: false,
        })
    }

    // SoftPresenter: an addition. Upstream keeps every buffer it ever made.
    /// Destroys the spare buffers `gone` picks, by their size in bytes and when they were
    /// handed back.
    fn release(&mut self, gone: impl Fn(u64, Instant) -> bool) {
        let mut destroyed = false;
        for (props, spare) in &mut self.bufs {
            spare.retain(|(buffer, since)| {
                let gone = gone(props.size, *since);
                if gone {
                    buffer.destroy();
                    destroyed = true;
                }
                !gone
            });
        }
        self.destroyed |= destroyed;
        self.bufs.retain(|_, spare| !spare.is_empty());
    }

    /// Quantize a size up to the nearest size class.
    pub(crate) fn size_class(x: u64, bits: u32) -> u64 {
        if x > 1 << bits {
            let a = (x - 1).leading_zeros();
            let b = (x - 1) | (((u64::MAX / 2) >> bits) >> a);
            b + 1
        } else {
            1 << bits
        }
    }
}

impl BindMapBuffer {
    // Upload a buffer from CPU to GPU if needed.
    //
    // Note data flow is one way only, from CPU to GPU. Once this method is
    // called, the buffer is no longer materialized on CPU, and cannot be
    // accessed from a CPU shader.
    //
    // SoftPresenter: `known_written` is how many bytes at its start may be other than zero,
    // when the frame's counters say (`BindMap::known_written`); the bytes that were sent are
    // handed back, for the caller to keep for the next frame.
    fn upload_if_needed(
        &mut self,
        proxy: &BufferProxy,
        device: &Device,
        queue: &Queue,
        pool: &mut ResourcePool,
        known_written: Option<usize>,
    ) -> Option<Vec<u8>> {
        if let MaterializedBuffer::Cpu(cpu_buf) = &self.buffer {
            let usage = BufferUsages::COPY_SRC
                | BufferUsages::COPY_DST
                | BufferUsages::STORAGE
                | BufferUsages::INDIRECT;
            // SoftPresenter: the GPU's buffer is only as long as what the stages on the
            // processor wrote (see `written_len`), where upstream makes it as long as the
            // whole buffer and sends it all: Vello's constant sizes are about 80 MB for the
            // three buffers the last stage reads, nearly all of it zeros, and Windows'
            // software graphics took 16 seconds over one small picture with them. Every byte
            // of the shorter buffer is sent, so one used again holds nothing of an earlier
            // frame. Where the frame's counters say how much was written (`known_written`),
            // the buffer is not looked through for it.
            let buf = {
                let bytes = cpu_buf.borrow();
                let written = match known_written {
                    Some(known) => {
                        let needed = needed_len(known, bytes.len());
                        debug_assert!(
                            written_len(&bytes) <= needed,
                            "{}: the counters say {known} bytes were written, but there is more",
                            proxy.name
                        );
                        needed
                    }
                    None => written_len(&bytes),
                };
                let buf = pool.get_buf(written as u64, proxy.name, usage, device);
                let sent = bytes.len().min(buf.size() as usize);
                queue.write_buffer(&buf, 0, &bytes[..sent]);
                buf
            };
            let sent = std::mem::replace(&mut self.buffer, MaterializedBuffer::Gpu(buf));
            if let MaterializedBuffer::Cpu(bytes) = sent {
                return Some(bytes.into_inner());
            }
        }
        None
    }
}

/// SoftPresenter: how much of a buffer that stages on the processor filled the GPU needs: all
/// of it up to the last byte that is not zero, and [`ZERO_TAIL`] bytes after (the whole buffer
/// if that is shorter). A buffer starts as zeros, so what is left out is zeros, and a stage on
/// the GPU reads the same from the shorter buffer as from the whole one: the shaders are
/// compiled with bounds checks, which read the buffer's last element for anything past its
/// end, and that element is zeros, as what was there in the whole buffer was.
fn written_len(bytes: &[u8]) -> usize {
    const BLOCK: usize = 4096;
    let blocks_of_zeros = bytes
        .rchunks(BLOCK)
        .take_while(|block| block.iter().all(|byte| *byte == 0))
        .count();
    let within = bytes.len().saturating_sub(blocks_of_zeros * BLOCK);
    let written = bytes[..within]
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |last| last + 1);
    needed_len(written, bytes.len())
}

/// SoftPresenter: the length the GPU's buffer needs for a buffer of `len` bytes of which only
/// the first `written` may be other than zeros (see [`written_len`]).
fn needed_len(written: usize, len: usize) -> usize {
    // Bounded first: counters may say more than the buffer holds.
    (written.min(len) + ZERO_TAIL).next_multiple_of(4).min(len)
}

/// SoftPresenter: the zeros kept after the last byte written (see [`written_len`]): more than
/// two of the largest element a stage on the GPU reads from such a buffer (a path segment, 24
/// bytes), so that the buffer's last whole element is zeros wherever the written part ends.
const ZERO_TAIL: usize = 256;

impl<'a> TransientBindMap<'a> {
    /// Create new transient bind map, seeded from external resources
    fn new(external_resources: &'a [ExternalResource<'_>]) -> Self {
        let mut bufs = HashMap::default();
        let mut images = HashMap::default();
        for resource in external_resources {
            match resource {
                ExternalResource::Buffer(proxy, gpu_buf) => {
                    bufs.insert(proxy.id, TransientBuf::Gpu(gpu_buf));
                }
                ExternalResource::Image(proxy, gpu_image) => {
                    images.insert(proxy.id, *gpu_image);
                }
            }
        }
        TransientBindMap { bufs, images }
    }

    fn materialize_gpu_buf_for_indirect(
        &mut self,
        bind_map: &mut BindMap,
        pool: &mut ResourcePool,
        device: &Device,
        queue: &Queue,
        buf: &BufferProxy,
    ) {
        if !self.bufs.contains_key(&buf.id)
            && let Some(b) = bind_map.buf_map.get_mut(&buf.id)
        {
            b.upload_if_needed(buf, device, queue, pool, None);
        }
    }

    #[cfg(feature = "debug_layers")]
    fn materialize_external_image_for_render_pass(&mut self, proxy: &ImageProxy) -> &TextureView {
        // TODO: Maybe this should support instantiating a transient texture. Right now all render
        // passes target a `SurfaceTexture`, so supporting external textures is sufficient.
        self.images
            .get(&proxy.id)
            .expect("texture not materialized")
    }

    fn create_bind_group(
        &mut self,
        bind_map: &mut BindMap,
        pool: &mut ResourcePool,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        layout: &BindGroupLayout,
        bindings: &[ResourceProxy],
    ) -> BindGroup {
        for proxy in bindings {
            match proxy {
                ResourceProxy::Buffer(proxy)
                | ResourceProxy::BufferRange {
                    proxy,
                    offset: _,
                    size: _,
                } => {
                    if self.bufs.contains_key(&proxy.id) {
                        continue;
                    }
                    // SoftPresenter: what the frame's counters say was written of it.
                    let known = bind_map.known_written(proxy.id);
                    match bind_map.buf_map.entry(proxy.id) {
                        Entry::Vacant(v) => {
                            // TODO: only some buffers will need indirect & vertex, but does it hurt?
                            let usage = BufferUsages::COPY_SRC
                                | BufferUsages::COPY_DST
                                | BufferUsages::STORAGE
                                | BufferUsages::INDIRECT
                                | BufferUsages::VERTEX;
                            let buf = pool.get_buf(proxy.size, proxy.name, usage, device);
                            if bind_map.pending_clears.remove(&proxy.id) {
                                encoder.clear_buffer(&buf, 0, None);
                            }
                            v.insert(BindMapBuffer {
                                buffer: MaterializedBuffer::Gpu(buf),
                                label: proxy.name,
                            });
                        }
                        Entry::Occupied(mut o) => {
                            let sent = o
                                .get_mut()
                                .upload_if_needed(proxy, device, queue, pool, known);
                            // SoftPresenter: kept for the next frame, when the counters
                            // say how much of it to make zero again.
                            if let (Some(bytes), Some(known)) = (sent, known) {
                                bind_map.keep(bytes, known, proxy.name);
                            }
                        }
                    }
                }
                ResourceProxy::Image(proxy) => {
                    if self.images.contains_key(&proxy.id) {
                        continue;
                    }
                    if let Entry::Vacant(v) = bind_map.image_map.entry(proxy.id) {
                        let format = proxy.format.to_wgpu();
                        let texture = device.create_texture(&wgpu::TextureDescriptor {
                            label: None,
                            size: wgpu::Extent3d {
                                width: proxy.width,
                                height: proxy.height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                            format,
                            view_formats: &[],
                        });
                        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
                            label: None,
                            usage: None,
                            dimension: Some(TextureViewDimension::D2),
                            aspect: TextureAspect::All,
                            mip_level_count: None,
                            base_mip_level: 0,
                            base_array_layer: 0,
                            array_layer_count: None,
                            format: Some(proxy.format.to_wgpu()),
                        });
                        v.insert((texture, texture_view));
                    }
                }
            }
        }
        let entries = bindings
            .iter()
            .enumerate()
            .map(|(i, proxy)| match proxy {
                ResourceProxy::Buffer(proxy) => {
                    let buf = match self.bufs.get(&proxy.id) {
                        Some(TransientBuf::Gpu(b)) => b,
                        _ => bind_map.get_gpu_buf(proxy.id).unwrap(),
                    };
                    wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: buf.as_entire_binding(),
                    }
                }
                ResourceProxy::BufferRange {
                    proxy,
                    offset,
                    size,
                } => {
                    let buf = match self.bufs.get(&proxy.id) {
                        Some(TransientBuf::Gpu(b)) => b,
                        _ => bind_map.get_gpu_buf(proxy.id).unwrap(),
                    };
                    wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: buf,
                            offset: *offset,
                            size: core::num::NonZeroU64::new(*size),
                        }),
                    }
                }
                ResourceProxy::Image(proxy) => {
                    let view = self
                        .images
                        .get(&proxy.id)
                        .copied()
                        .or_else(|| bind_map.image_map.get(&proxy.id).map(|v| &v.1))
                        .unwrap();
                    wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: wgpu::BindingResource::TextureView(view),
                    }
                }
            })
            .collect::<Vec<_>>();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &entries,
        })
    }

    fn create_cpu_resources(
        &self,
        bind_map: &'a mut BindMap,
        bindings: &[ResourceProxy],
    ) -> Vec<CpuBinding<'_>> {
        // First pass is mutable; create buffers as needed
        for resource in bindings {
            match resource {
                ResourceProxy::Buffer(proxy)
                | ResourceProxy::BufferRange {
                    proxy,
                    offset: _,
                    size: _,
                } => match self.bufs.get(&proxy.id) {
                    Some(TransientBuf::Cpu(_)) => (),
                    Some(TransientBuf::Gpu(_)) => panic!("buffer was already materialized on GPU"),
                    _ => bind_map.materialize_cpu_buf(proxy),
                },
                ResourceProxy::Image(_) => todo!(),
            }
        }
        // Second pass takes immutable references
        bindings
            .iter()
            .map(|resource| match resource {
                ResourceProxy::Buffer(buf) => match self.bufs.get(&buf.id) {
                    Some(TransientBuf::Cpu(b)) => CpuBinding::Buffer(b),
                    _ => bind_map.get_cpu_buf(buf.id),
                },
                ResourceProxy::BufferRange { .. } => todo!(),
                ResourceProxy::Image(_) => todo!(),
            })
            .collect()
    }
}

// SoftPresenter: an addition.
#[cfg(test)]
mod tests {
    use super::{
        BindMap, KEPT_ON_PROCESSOR, MaterializedBuffer, ZERO_TAIL, needed_len, written_len,
    };
    use crate::recording::BufferProxy;

    /// A buffer kept for the next frame is all zeros again, is the one the next frame is given
    /// when it asks for that size, and no more are kept than one frame has.
    #[test]
    fn a_buffer_kept_for_the_next_frame_starts_as_zeros_again() {
        let mut map = BindMap::default();
        let mut bytes = vec![0_u8; 4096];
        bytes[..100].fill(9);
        map.keep(bytes, 100, "test");
        assert!(map.kept[0].iter().all(|byte| *byte == 0));
        // Counters that say more than the buffer holds mean all of it.
        map.keep(vec![9_u8; 64], 1 << 20, "test");
        assert!(map.kept[1].iter().all(|byte| *byte == 0));

        let same = BufferProxy::new(4096, "test");
        map.materialize_cpu_buf(&same);
        assert_eq!(map.kept.len(), 1, "the kept one of that size is used");
        let other = BufferProxy::new(128, "test");
        map.materialize_cpu_buf(&other);
        assert_eq!(map.kept.len(), 1, "another size is made anew");
        for proxy in [same, other] {
            let MaterializedBuffer::Cpu(bytes) = &map.buf_map[&proxy.id].buffer else {
                panic!("in the processor's memory");
            };
            assert_eq!(bytes.borrow().len() as u64, proxy.size);
            assert!(bytes.borrow().iter().all(|byte| *byte == 0));
        }

        for _ in 0..KEPT_ON_PROCESSOR + 3 {
            map.keep(vec![0; 16], 0, "test");
        }
        assert_eq!(map.kept.len(), KEPT_ON_PROCESSOR);
    }

    /// What the counters say was written is sent with the same zeros after it as what is
    /// found by looking through the buffer.
    #[test]
    fn what_the_counters_bound_is_sent_as_what_was_found_is() {
        assert_eq!(needed_len(0, 1 << 20), ZERO_TAIL);
        assert_eq!(
            needed_len(1001, 1 << 20),
            (1001 + ZERO_TAIL).next_multiple_of(4)
        );
        assert_eq!(needed_len(4000, 4096), 4096);
        assert_eq!(needed_len(usize::MAX, 4096), 4096);
        let mut bytes = vec![0_u8; 8192];
        bytes[999] = 1;
        assert_eq!(written_len(&bytes), needed_len(1000, bytes.len()));
    }

    /// What the GPU is sent of a buffer filled on the processor ends a little after the last
    /// byte that is not zero, wherever that byte is, and is never longer than the buffer.
    #[test]
    fn a_buffer_filled_on_the_processor_is_sent_up_to_what_was_written() {
        for len in [0, 4, 252, 256, 4096, 4100, 12_288, 1 << 20] {
            let mut bytes = vec![0_u8; len];
            assert_eq!(
                written_len(&bytes),
                ZERO_TAIL.min(len),
                "all zeros, {len} long"
            );
            for last in [
                0,
                1,
                3,
                255,
                4095,
                4096,
                8191,
                len / 2,
                len.saturating_sub(1),
            ] {
                if last >= len {
                    continue;
                }
                bytes.fill(0);
                bytes[last] = 7;
                let sent = written_len(&bytes);
                assert!(sent <= len && sent % 4 == 0, "{sent} of {len}");
                assert!(
                    sent == len || sent >= last + 1 + ZERO_TAIL,
                    "{sent} for a last byte at {last} of {len}"
                );
                assert!(
                    sent <= last + 1 + ZERO_TAIL + 3,
                    "no more than is needed: {sent} for {last} of {len}"
                );
                assert!(
                    bytes[sent..].iter().all(|byte| *byte == 0),
                    "only zeros are left out"
                );
            }
        }
    }
}
