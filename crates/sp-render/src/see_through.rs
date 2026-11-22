//! See-through screens, for a church that streams: the words over a camera picture in a video
//! switcher. Their picture is painted on clear ([`Painter::paint_see_through`](crate::Painter)),
//! which leaves each pixel's colour as it is with how solid it is beside it (straight alpha).
//! Here: how one such picture gives way to the next, and how one is shown in a window, which a
//! display can only show solid: its colours as they are or premultiplied, or its key (how solid
//! each pixel is, as a grey) for a switcher that takes the colours and the key as two inputs.

use sp_model::TransitionKind;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{BlendMode, Compose, Fill, Mix};
use vello::{Scene, wgpu};

/// One see-through picture giving way to another, `amount` of the way through (0 is all `from`,
/// 1 is all `to`). Both must have been built for a target of this size.
///
/// On a solid screen every transition hides the old picture under the new one's solid
/// background as it comes. A see-through picture has none, so a push or a wipe would leave the
/// old words showing under the new until the end, and a dissolve would lay the new words over
/// the old at full strength. So here every transition but a cut is a true cross-fade: the old
/// picture fades out exactly as the new one fades in, so a pixel clear in both stays clear and
/// one solid in both stays solid. A fade through black or white goes through clear instead:
/// on a stream, that is the camera alone.
pub fn see_through_transition(kind: TransitionKind, from: &Scene, to: &Scene, amount: f32, w: u32, h: u32) -> Scene {
    let a = amount.clamp(0.0, 1.0);
    let whole = Rect::new(0.0, 0.0, f64::from(w), f64::from(h));
    let mut scene = Scene::new();
    let mut lay = |picture: &Scene, alpha: f32, blend: BlendMode| {
        if alpha <= 0.0 {
            return;
        }
        scene.push_layer(Fill::NonZero, blend, alpha, Affine::IDENTITY, &whole);
        scene.append(picture, None);
        scene.pop_layer();
    };
    match kind {
        TransitionKind::Cut => lay(to, 1.0, Mix::Normal.into()),
        TransitionKind::FadeBlack | TransitionKind::FadeWhite if a < 0.5 => {
            lay(from, 1.0 - a * 2.0, Mix::Normal.into())
        }
        TransitionKind::FadeBlack | TransitionKind::FadeWhite => lay(to, a * 2.0 - 1.0, Mix::Normal.into()),
        _ => {
            // The old at 1 − a, and the new added to it at a (not laid over it): every pixel
            // is the old one's share and the new one's together, its alpha too.
            lay(from, 1.0 - a, Mix::Normal.into());
            lay(to, a, BlendMode::new(Mix::Normal, Compose::Plus));
        }
    }
    scene
}

/// How a picture painted see-through is shown in a window, always solid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Solid {
    /// The colours as they are, at full strength right to the soft edges of the words; black
    /// where it is clear. The fill for a switcher that multiplies it by the key itself.
    Straight,
    /// The colours multiplied by how solid each pixel is: the picture laid over black. The fill
    /// for a switcher whose key is set to premultiplied.
    Premultiplied,
    /// How solid each pixel is, as a grey: white where solid, black where clear, greys at the
    /// soft edges. The key.
    Key,
}

const SHADER: &str = r"
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) place: vec2<f32>,
}

// One triangle that covers the whole target, the picture the right way up.
@vertex
fn whole(@builtin(vertex_index) index: u32) -> Out {
    var out: Out;
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.place = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var sampled: sampler;

@fragment
fn straight(in: Out) -> @location(0) vec4<f32> {
    let colour = textureSample(picture, sampled, in.place);
    return vec4<f32>(colour.rgb, 1.0);
}

@fragment
fn premultiplied(in: Out) -> @location(0) vec4<f32> {
    let colour = textureSample(picture, sampled, in.place);
    return vec4<f32>(colour.rgb * colour.a, 1.0);
}

@fragment
fn key(in: Out) -> @location(0) vec4<f32> {
    let colour = textureSample(picture, sampled, in.place);
    return vec4<f32>(colour.a, colour.a, colour.a, 1.0);
}
";

/// Draws a picture painted see-through into a target of another format (a window's next
/// picture), filling it, solid as [`Solid`] says: what is written is never see-through, so a
/// window can't show what is behind it. A picture of another size than the target is scaled,
/// smoothly. One for each window, made for the format of its pictures.
pub struct SolidPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    shown: Solid,
    format: wgpu::TextureFormat,
}

impl SolidPass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shown: Solid) -> SolidPass {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sp-render solid pass"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sp-render solid pass"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sp-render solid pass"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sp-render solid pass"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let fragment = match shown {
            Solid::Straight => "straight",
            Solid::Premultiplied => "premultiplied",
            Solid::Key => "key",
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sp-render solid pass"),
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
                entry_point: Some(fragment),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        SolidPass { pipeline, layout, sampler, shown, format }
    }

    /// How it shows pictures.
    pub fn shown(&self) -> Solid {
        self.shown
    }

    /// The format of the targets it draws into.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Records drawing `picture` (a view of a texture made by [`target_texture`](crate::target_texture)
    /// and painted see-through) into `target`, which it fills.
    pub fn draw(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        picture: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp-render solid pass"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(picture) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sp-render solid pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
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
}
