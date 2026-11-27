//! A video's picture as the decoder makes it, in two planes (NV12, as FFmpeg gives most videos),
//! turned into red, green and blue on the GPU: every pixel's brightness, a byte each, and its
//! colour, two bytes for each square of four pixels. That is three eighths of the bytes of
//! four-byte pixels to move, and the conversion, which the processor used to do for every pixel
//! of every picture, is a small compute pass ([`Painter::show_planes`](crate::Painter::show_planes)).
//!
//! The colours come out as FFmpeg's own converter made them from the same video: the same
//! standard's shares of red and blue ([`YuvMatrix`]), the same range, and each colour sample
//! used for its square of four pixels as it is.

use vello::wgpu;

/// The standard a video's colours were written in, which says how much of its brightness is
/// red and how much blue. These are the ones FFmpeg's converter has numbers of its own for; a
/// video that names none (or another) is taken as BT.601, as FFmpeg takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvMatrix {
    /// SD video, and any video that doesn't say.
    Bt601,
    /// HD video.
    Bt709,
    /// The old American standard (FCC 73.682).
    Fcc,
    /// SMPTE 240M, early HD.
    Smpte240,
    /// UHD video.
    Bt2020,
}

impl YuvMatrix {
    /// The shares of red and of blue in the brightness (Kr and Kb).
    fn shares(self) -> (f32, f32) {
        match self {
            YuvMatrix::Bt601 => (0.299, 0.114),
            YuvMatrix::Bt709 => (0.2126, 0.0722),
            YuvMatrix::Fcc => (0.30, 0.11),
            YuvMatrix::Smpte240 => (0.212, 0.087),
            YuvMatrix::Bt2020 => (0.2627, 0.0593),
        }
    }
}

/// How a video's planes stand for colours: its standard, and whether its bytes use their whole
/// range (as a phone's or a camera's JPEG-like video may) or the limited range of broadcast
/// video (brightness 16 to 235, colour 16 to 240), as nearly every video does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YuvColours {
    pub matrix: YuvMatrix,
    pub full_range: bool,
}

impl YuvColours {
    /// The rows that turn a pixel's brightness and colour (Y, Cb, Cr, and 1), each byte read as
    /// a fraction of 255, into its red, green and blue (from 0 to 1, once held to that).
    pub fn rows(self) -> [[f32; 4]; 3] {
        let (kr, kb) = self.matrix.shares();
        let kg = 1.0 - kr - kb;
        // Brightness and colour as fractions: 0 to 1, and -0.5 to 0.5 about the middle byte.
        let (y_scale, y_offset, c_scale) =
            if self.full_range { (1.0, 0.0, 1.0) } else { (255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0) };
        let c_offset = -128.0 / 255.0 * c_scale;
        let (red_from_cr, blue_from_cb) = (2.0 * (1.0 - kr), 2.0 * (1.0 - kb));
        let (green_from_cb, green_from_cr) = (2.0 * kb * (1.0 - kb) / kg, 2.0 * kr * (1.0 - kr) / kg);
        [
            [y_scale, 0.0, red_from_cr * c_scale, y_offset + red_from_cr * c_offset],
            [
                y_scale,
                -green_from_cb * c_scale,
                -green_from_cr * c_scale,
                y_offset - (green_from_cb + green_from_cr) * c_offset,
            ],
            [y_scale, blue_from_cb * c_scale, 0.0, y_offset + blue_from_cb * c_offset],
        ]
    }

    /// The red, green and blue bytes of one pixel's brightness and colour bytes, worked out on
    /// the processor the way the GPU does: for checking it.
    pub fn rgb(self, y: u8, cb: u8, cr: u8) -> [u8; 3] {
        let read = [f32::from(y) / 255.0, f32::from(cb) / 255.0, f32::from(cr) / 255.0, 1.0];
        self.rows().map(|row| {
            let value: f32 = row.iter().zip(read).map(|(weight, part)| weight * part).sum();
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        })
    }
}

/// A video's picture in two planes, lent to be sent to the GPU (see
/// [`Painter::show_planes`](crate::Painter::show_planes)).
#[derive(Clone, Copy, Debug)]
pub struct YuvPlanes<'a> {
    pub width: u32,
    pub height: u32,
    /// Each pixel's brightness, a byte each, each row starting `luma_row` bytes after the last.
    pub luma: &'a [u8],
    pub luma_row: usize,
    /// The colour of each square of four pixels, its Cb and Cr bytes side by side: half the
    /// width and half the height, rounded up (see [`YuvPlanes::chroma_size`]), each row
    /// starting `chroma_row` bytes after the last.
    pub chroma: &'a [u8],
    pub chroma_row: usize,
    pub colours: YuvColours,
}

impl YuvPlanes<'_> {
    /// How many colour samples a picture of this size has across and down: one for each
    /// square of four pixels, and one for the half square at an odd side's edge.
    pub fn chroma_size(width: u32, height: u32) -> (u32, u32) {
        (width.div_ceil(2), height.div_ceil(2))
    }

    /// True when both planes have the bytes their sizes need.
    fn hold(&self) -> bool {
        let (cw, ch) = Self::chroma_size(self.width, self.height);
        plane_holds(self.width, self.height, 1, self.luma.len(), self.luma_row)
            && plane_holds(cw, ch, 2, self.chroma.len(), self.chroma_row)
    }
}

/// True when `len` bytes hold `height` rows of `width` samples of `size` bytes, each row
/// starting `row` bytes after the last (the last needs only its samples).
fn plane_holds(width: u32, height: u32, size: usize, len: usize, row: usize) -> bool {
    let wide = width as usize * size;
    width > 0
        && height > 0
        && row >= wide
        && u32::try_from(row).is_ok()
        && (height as usize - 1)
            .checked_mul(row)
            .and_then(|rows| rows.checked_add(wide))
            .is_some_and(|need| len >= need)
}

const SHADER: &str = "
struct Colours {
    red: vec4<f32>,
    green: vec4<f32>,
    blue: vec4<f32>,
}

@group(0) @binding(0) var luma: texture_2d<f32>;
@group(0) @binding(1) var chroma: texture_2d<f32>;
@group(0) @binding(2) var dest: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> colours: Colours;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(luma);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    // Each colour sample is used for its square of four pixels as it is, as FFmpeg's own
    // converter uses it.
    let colour = textureLoad(chroma, id.xy / 2u, 0).rg;
    let pixel = vec4<f32>(textureLoad(luma, id.xy, 0).r, colour, 1.0);
    let rgb = vec3<f32>(dot(colours.red, pixel), dot(colours.green, pixel), dot(colours.blue, pixel));
    textureStore(dest, id.xy, vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0));
}
";

/// The compute pass that turns a video's planes into a moving picture's pixels, with the
/// textures the planes are sent into (kept from picture to picture, made again for another
/// size) and the colours' rows.
pub(crate) struct ToColours {
    pipeline: wgpu::ComputePipeline,
    uniform: wgpu::Buffer,
    /// The colours whose rows the uniform holds.
    written: Option<YuvColours>,
    planes: Option<PlaneTextures>,
    /// Where the time of the last picture turned went (see [`PlanesWork`]).
    pub(crate) work: PlanesWork,
}

/// Where the time went sending a video's planes to the GPU and turning them into colours, for
/// one picture: making the textures the planes go into (only for the first picture of a size),
/// handing the GPU's queue their bytes, and handing it the pass that turns them into colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanesWork {
    pub textures: std::time::Duration,
    pub bytes: std::time::Duration,
    pub pass: std::time::Duration,
}

struct PlaneTextures {
    size: (u32, u32),
    luma: wgpu::Texture,
    chroma: wgpu::Texture,
}

impl PlaneTextures {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> PlaneTextures {
        let plane = |label, width, height, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let (cw, ch) = YuvPlanes::chroma_size(width, height);
        PlaneTextures {
            size: (width, height),
            luma: plane("sp-render video brightness", width, height, wgpu::TextureFormat::R8Unorm),
            chroma: plane("sp-render video colour", cw, ch, wgpu::TextureFormat::Rg8Unorm),
        }
    }
}

impl ToColours {
    pub(crate) fn new(device: &wgpu::Device) -> ToColours {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sp-render video colours"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("sp-render video colours"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sp-render video colours"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ToColours { pipeline, uniform, written: None, planes: None, work: PlanesWork::default() }
    }

    /// Sends the planes to the GPU and the pass that writes their colours into `dest` (an
    /// RGBA texture of their size that a compute pass can write), red first, opaque. False,
    /// with nothing sent, when the planes lack bytes or `dest` is another size.
    pub(crate) fn convert(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        planes: &YuvPlanes<'_>,
        dest: &wgpu::Texture,
    ) -> bool {
        let (width, height) = (planes.width, planes.height);
        if !planes.hold() || (dest.width(), dest.height()) != (width, height) {
            return false;
        }
        let began = std::time::Instant::now();
        if self.planes.as_ref().is_none_or(|kept| kept.size != (width, height)) {
            self.planes = Some(PlaneTextures::new(device, width, height));
        }
        let made = began.elapsed();
        let Some(textures) = self.planes.as_ref() else { return false };
        let send = |texture: &wgpu::Texture, bytes: &[u8], row: usize| {
            queue.write_texture(
                texture.as_image_copy(),
                bytes,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: u32::try_from(row).ok(), rows_per_image: None },
                texture.size(),
            );
        };
        send(&textures.luma, planes.luma, planes.luma_row);
        send(&textures.chroma, planes.chroma, planes.chroma_row);
        let sent = began.elapsed();
        if self.written != Some(planes.colours) {
            let bytes: Vec<u8> =
                planes.colours.rows().as_flattened().iter().flat_map(|word| word.to_le_bytes()).collect();
            queue.write_buffer(&self.uniform, 0, &bytes);
            self.written = Some(planes.colours);
        }
        let view = |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (luma, chroma, to) = (view(&textures.luma), view(&textures.chroma), view(dest));
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp-render video colours"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&luma) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&chroma) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&to) },
                wgpu::BindGroupEntry { binding: 3, resource: self.uniform.as_entire_binding() },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        queue.submit([encoder.finish()]);
        self.work =
            PlanesWork { textures: made, bytes: sent.saturating_sub(made), pass: began.elapsed().saturating_sub(sent) };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The standard colour bars at 75%, as the standards give their bytes (limited range):
    /// yellow, cyan, green, magenta, red and blue.
    const BARS: [[u8; 3]; 6] = [[191, 191, 0], [0, 191, 191], [0, 191, 0], [191, 0, 191], [191, 0, 0], [0, 0, 191]];
    const BARS_709: [[u8; 3]; 6] =
        [[168, 44, 136], [145, 147, 44], [134, 63, 52], [63, 193, 204], [51, 109, 212], [28, 212, 120]];
    const BARS_601: [[u8; 3]; 6] =
        [[162, 44, 142], [131, 156, 44], [112, 72, 58], [84, 184, 198], [65, 100, 212], [35, 212, 114]];

    fn near(a: [u8; 3], b: [u8; 3], within: u8) -> bool {
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= within)
    }

    #[test]
    fn colour_bars_come_out_as_the_standards_give_them() {
        for (matrix, bytes) in [(YuvMatrix::Bt709, BARS_709), (YuvMatrix::Bt601, BARS_601)] {
            let colours = YuvColours { matrix, full_range: false };
            for (rgb, [y, cb, cr]) in BARS.iter().zip(bytes) {
                let got = colours.rgb(y, cb, cr);
                assert!(near(got, *rgb, 2), "{matrix:?} {y},{cb},{cr}: {got:?}, not {rgb:?}");
            }
            assert_eq!(colours.rgb(235, 128, 128), [255, 255, 255], "{matrix:?} white");
            assert_eq!(colours.rgb(16, 128, 128), [0, 0, 0], "{matrix:?} black");
            assert_eq!(colours.rgb(255, 128, 128), [255, 255, 255], "{matrix:?} above white is held to white");
        }
    }

    /// Full range: colours written with the standards' own formulas over every byte come back.
    #[test]
    fn full_range_colours_come_back_as_they_were_written() {
        for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709] {
            let (kr, kb) = matrix.shares();
            let colours = YuvColours { matrix, full_range: true };
            for rgb in BARS.into_iter().chain([[255, 255, 255], [0, 0, 0], [255, 0, 0], [30, 140, 250]]) {
                let [r, g, b] = rgb.map(|value| f32::from(value) / 255.0);
                let y = kr * r + (1.0 - kr - kb) * g + kb * b;
                let byte = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
                let (cb, cr) = (
                    byte((b - y) / (2.0 * (1.0 - kb)) + 128.0 / 255.0),
                    byte((r - y) / (2.0 * (1.0 - kr)) + 128.0 / 255.0),
                );
                let got = colours.rgb(byte(y), cb, cr);
                assert!(near(got, rgb, 2), "{matrix:?} {rgb:?} came back as {got:?}");
            }
            assert_eq!(colours.rgb(255, 128, 128), [255, 255, 255]);
            assert_eq!(colours.rgb(0, 128, 128), [0, 0, 0]);
        }
    }

    /// Each standard's numbers are FFmpeg's (its table of red and blue in the colour, in 16.16
    /// fixed point, for limited range).
    #[test]
    fn the_standards_numbers_are_ffmpegs() {
        for (matrix, ffmpeg) in [
            (YuvMatrix::Bt601, [104_597, 132_201, 25_675, 53_279]),
            (YuvMatrix::Bt709, [117_489, 138_438, 13_975, 34_925]),
            (YuvMatrix::Fcc, [104_448, 132_798, 24_759, 53_109]),
            (YuvMatrix::Smpte240, [117_579, 136_230, 16_907, 35_559]),
            (YuvMatrix::Bt2020, [110_013, 140_363, 12_277, 42_626]),
        ] {
            let rows = YuvColours { matrix, full_range: false }.rows();
            let ours = [rows[0][2], rows[2][1], -rows[1][1], -rows[1][2]];
            for (ours, theirs) in ours.into_iter().zip(ffmpeg) {
                let theirs = theirs as f32 / 65536.0;
                assert!((ours - theirs).abs() < 0.001, "{matrix:?}: {ours} against FFmpeg's {theirs}");
            }
        }
    }

    #[test]
    fn planes_must_hold_their_sizes_bytes() {
        let colours = YuvColours { matrix: YuvMatrix::Bt709, full_range: false };
        let (luma, chroma) = (vec![0u8; 5 * 3], vec![0u8; 6 * 2]);
        let planes =
            YuvPlanes { width: 5, height: 3, luma: &luma, luma_row: 5, chroma: &chroma, chroma_row: 6, colours };
        assert_eq!(YuvPlanes::chroma_size(5, 3), (3, 2), "an odd side's edge has a sample of its own");
        assert!(planes.hold());
        assert!(!YuvPlanes { chroma: &chroma[..11], ..planes }.hold(), "a colour byte short");
        assert!(!YuvPlanes { chroma_row: 4, ..planes }.hold(), "colour rows too short for three samples");
        assert!(!YuvPlanes { luma: &luma[..14], ..planes }.hold(), "a brightness byte short");
        assert!(
            YuvPlanes { luma_row: 8, luma: &[0u8; 8 * 2 + 5], ..planes }.hold(),
            "longer rows, the last only its pixels"
        );
        assert!(!YuvPlanes { width: 0, ..planes }.hold());
    }
}
