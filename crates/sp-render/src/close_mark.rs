//! A round plate with an X on it, laid over a picture already in a target: the button a screen's
//! window shows when the pointer moves over it, to switch that screen off (the app decides when
//! and where; see its `close_button` module). Drawn by one small pass in the fragment shader, with
//! no scene to encode and none of the renderer's passes, so showing it, or fading it, costs a
//! window one copy of its picture and this.

use vello::wgpu;

/// Where the mark goes: its square's left and top edges and its side, in pixels of the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Square {
    pub x: f32,
    pub y: f32,
    pub side: f32,
}

impl Square {
    /// True when the square lies wholly inside a target this size (the only place it is drawn).
    pub fn inside(&self, w: u32, h: u32) -> bool {
        self.side >= 1.0
            && self.x >= 0.0
            && self.y >= 0.0
            && self.x + self.side <= w as f32
            && self.y + self.side <= h as f32
    }
}

/// How the mark looks: the plate, the thin ring round its edge and the X, each as red, green,
/// blue and how solid (0 to 1, not premultiplied) in the target's own colour values, and how
/// strongly the whole is laid over the picture (0 not at all, 1 fully).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkLook {
    pub plate: [f32; 4],
    pub ring: [f32; 4],
    pub cross: [f32; 4],
    pub strength: f32,
}

const SHADER: &str = r"
struct Look {
    plate: vec4<f32>,
    ring: vec4<f32>,
    cross: vec4<f32>,
    // x: how strongly it is laid over the picture; y: one pixel, in the square's own units
    // (-1 to 1 across it).
    sizes: vec4<f32>,
}

@group(0) @binding(0) var<uniform> look: Look;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) place: vec2<f32>,
}

// One triangle over the whole viewport, which is the mark's square; `place` runs from -1 to 1
// across it.
@vertex
fn whole(@builtin(vertex_index) index: u32) -> Out {
    var out: Out;
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.place = corner * 2.0 - 1.0;
    return out;
}

// How much of a pixel a shape covers, from how far its centre is outside the shape (negative
// inside): soft over one pixel, so the edges are smooth at any size.
fn covered(outside: f32) -> f32 {
    return clamp(0.5 - outside / look.sizes.y, 0.0, 1.0);
}

// How far a point is from the line from a to b.
fn from_line(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let along = b - a;
    let t = clamp(dot(p - a, along) / dot(along, along), 0.0, 1.0);
    return length(p - a - along * t);
}

fn premultiplied(colour: vec4<f32>, coverage: f32) -> vec4<f32> {
    let a = colour.a * coverage;
    return vec4<f32>(colour.rgb * a, a);
}

fn over(top: vec4<f32>, under: vec4<f32>) -> vec4<f32> {
    return top + under * (1.0 - top.a);
}

@fragment
fn mark(in: Out) -> @location(0) vec4<f32> {
    let p = in.place;
    let d = length(p);
    let plate = covered(d - 1.0);
    let ring = covered(abs(d - 0.975) - 0.025);
    let arm = 0.34;
    let line = min(from_line(p, vec2<f32>(-arm, -arm), vec2<f32>(arm, arm)),
                   from_line(p, vec2<f32>(-arm, arm), vec2<f32>(arm, -arm)));
    let cross = covered(line - 0.06);
    var colour = premultiplied(look.plate, plate);
    colour = over(premultiplied(look.ring, ring), colour);
    colour = over(premultiplied(look.cross, cross), colour);
    return colour * look.sizes.x;
}
";

/// Lays the mark over a target of one format (a window's next picture, once its picture has
/// been copied in). One is enough for every target of its format, so long as each submission
/// draws it once at most: its look is kept in one buffer, which a submission reads as it was
/// last written before it.
pub struct CloseMark {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    format: wgpu::TextureFormat,
}

impl CloseMark {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> CloseMark {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sp-render close mark"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sp-render close mark"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sp-render close mark"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sp-render close mark"),
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
                entry_point: Some("mark"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                // Laid over what is there; how solid the target is stays as it was, so a window
                // never becomes see-through where the mark is.
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sp-render close mark"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp-render close mark"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        CloseMark { pipeline, bind_group, uniform, format }
    }

    /// The format of the targets it draws into.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Records laying the mark over `target`, a picture `size` pixels wide and high, in `square`,
    /// keeping the rest of the picture as it is; for the encoder that has just put the picture
    /// there. Its look is written to the queue now, so it is the one this encoder's work uses
    /// when submitted next. Nothing is recorded at strength 0, or for a square not wholly inside
    /// the target.
    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        square: Square,
        look: &MarkLook,
    ) {
        if look.strength <= 0.0 || !square.inside(size.0, size.1) {
            return;
        }
        let sizes = [look.strength.min(1.0), 2.0 / square.side, 0.0, 0.0];
        let words: Vec<f32> = [look.plate, look.ring, look.cross, sizes].concat();
        let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        queue.write_buffer(&self.uniform, 0, &bytes);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sp-render close mark"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(square.x, square.y, square.side, square.side, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_is_drawn_only_in_a_square_wholly_inside_its_target() {
        let square = Square { x: 1824.0, y: 24.0, side: 72.0 };
        assert!(square.inside(1920, 1080));
        assert!(!square.inside(1880, 1080), "past the right edge");
        assert!(!square.inside(1920, 90), "past the bottom");
        assert!(!Square { x: -1.0, ..square }.inside(1920, 1080), "past the left edge");
        assert!(!Square { side: 0.5, ..square }.inside(1920, 1080), "too small to see");
    }
}
