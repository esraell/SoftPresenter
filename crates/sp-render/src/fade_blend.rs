//! The cheapest way to show a fade between two pictures that are already in textures
//! ([`CachedSide`]): one pass that reads a texel of each and adds them up, with no scene to
//! encode or sort and none of the renderer's passes. Only the transitions that are a plain sum of
//! the two pictures (and perhaps a flat colour) are done this way ([`blend_weights`]); the rest
//! are drawn by the renderer from the same textures.
//!
//! The sums are the renderer's own for those transitions, taken in the same (non-linear) colour
//! values it blends in, and with the same ends: nothing but the old picture at 0 and the new
//! one at 1.

use sp_model::TransitionKind;
use vello::wgpu;

use crate::CachedSide;

/// What a fade step is made of: `from * from + to * to + plus`, each colour value of a texel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weights {
    pub from: f32,
    pub to: f32,
    /// A flat colour added (a fade through black adds none, one through white adds its share).
    pub plus: [f32; 3],
}

/// The weights of a transition `amount` of the way through, for the kinds that are a sum of the
/// two pictures; `None` for the others (they move or cut one of the pictures).
pub fn blend_weights(kind: TransitionKind, amount: f32) -> Option<Weights> {
    let a = amount.clamp(0.0, 1.0);
    // The renderer lays one picture over the other with a share of its strength, so the old
    // picture keeps what the new one does not cover.
    Some(match kind {
        TransitionKind::Dissolve => Weights { from: 1.0 - a, to: a, plus: [0.0; 3] },
        // Each fades on its own over black: the old at 1 - a, then the new at a over that.
        TransitionKind::Fade => Weights { from: (1.0 - a) * (1.0 - a), to: a, plus: [0.0; 3] },
        TransitionKind::FadeBlack | TransitionKind::FadeWhite => {
            let through = if kind == TransitionKind::FadeBlack { 0.0 } else { 1.0 };
            if a < 0.5 {
                let t = a * 2.0;
                Weights { from: 1.0 - t, to: 0.0, plus: [through * t; 3] }
            } else {
                let t = a * 2.0 - 1.0;
                Weights { from: 0.0, to: t, plus: [through * (1.0 - t); 3] }
            }
        }
        _ => return None,
    })
}

const SHADER: &str = r"
struct Mixing {
    weights: vec4<f32>,
    plus: vec4<f32>,
}

@group(0) @binding(0) var from_picture: texture_2d<f32>;
@group(0) @binding(1) var to_picture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> mixing: Mixing;

@vertex
fn whole(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn mix_pictures(@builtin(position) place: vec4<f32>) -> @location(0) vec4<f32> {
    let at = vec2<i32>(place.xy);
    let before = textureLoad(from_picture, at, 0).rgb;
    let after = textureLoad(to_picture, at, 0).rgb;
    return vec4<f32>(before * mixing.weights.x + after * mixing.weights.y + mixing.plus.rgb, 1.0);
}
";

/// Draws a fade step from two cached sides into a texture of the same size and of the format
/// [`target_texture`](crate::target_texture) makes. One for each thread that draws.
pub struct FadeBlend {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
}

impl FadeBlend {
    pub fn new(device: &wgpu::Device) -> FadeBlend {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let texture = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sp-render fade blend"),
            entries: &[
                entry(0, texture),
                entry(1, texture),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sp-render fade blend"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sp-render fade blend"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sp-render fade blend"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("whole"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("mix_pictures"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sp-render fade blend"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        FadeBlend { pipeline, layout, uniform }
    }

    /// Draws `weights` of the two sides into `target` (a view of a texture made by
    /// [`target_texture`](crate::target_texture), the sides' size), filling it, and submits it.
    /// False, with nothing drawn, when the sides are not that size.
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        from: &CachedSide,
        to: &CachedSide,
        weights: Weights,
        target: &wgpu::Texture,
    ) -> bool {
        let size = (target.width(), target.height());
        if from.is_split() || to.is_split() || from.size() != size || to.size() != size {
            return false;
        }
        let words = [weights.from, weights.to, 0.0, 0.0, weights.plus[0], weights.plus[1], weights.plus[2], 0.0];
        let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        queue.write_buffer(&self.uniform, 0, &bytes);
        let (from, to) = (from.view(), to.view());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp-render fade blend"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&from) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&to) },
                wgpu::BindGroupEntry { binding: 2, resource: self.uniform.as_entire_binding() },
            ],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sp-render fade blend"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        // The renderer looks at the device after each picture, which lets go of what the GPU has
        // finished with (the textures of fades that are over, and each step's own bindings);
        // a step that never calls it would leave all that held until something else did.
        let _ = device.poll(wgpu::PollType::Poll);
        true
    }
}
