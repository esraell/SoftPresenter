//! Turning a screen's picture (RGBA rows, as the screens' renderer leaves them) into a frame
//! of the card's: its size, and 8-bit BGRA or 8-bit YUV 4:2:2. Done on the output's own
//! thread, once for each new picture.

use crate::Alpha;

/// The card's pixel formats used, as `BMDPixelFormat` codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// `bmdFormat8BitBGRA`: blue, green, red, alpha, a byte each. Any card that plays RGB takes
    /// it, and it carries the alpha a keyer needs.
    Bgra,
    /// `bmdFormat8BitYUV` ('2vuy'): for every two pixels, Cb, Y, Cr, Y, in BT.709's video
    /// range, for a mode that can't take BGRA.
    Yuv,
}

impl PixelFormat {
    pub fn code(self) -> u32 {
        match self {
            PixelFormat::Bgra => u32::from_be_bytes(*b"BGRA"),
            PixelFormat::Yuv => u32::from_be_bytes(*b"2vuy"),
        }
    }

    pub fn in_words(self) -> &'static str {
        match self {
            PixelFormat::Bgra => "8-bit BGRA",
            PixelFormat::Yuv => "8-bit YUV 4:2:2",
        }
    }

    /// The bytes in a row of `width` pixels, packed tight.
    pub fn row_bytes(self, width: u32) -> usize {
        match self {
            PixelFormat::Bgra => width as usize * 4,
            PixelFormat::Yuv => width.div_ceil(2) as usize * 4,
        }
    }
}

/// How a frame is laid out on the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    /// The bytes in each row, which the card may make more than the pixels need.
    pub row_bytes: usize,
}

impl Layout {
    pub fn frame_bytes(&self) -> usize {
        self.row_bytes * self.height as usize
    }
}

/// A black frame, solid, in this layout.
pub fn black(layout: Layout) -> Vec<u8> {
    let mut out = vec![0; layout.frame_bytes()];
    for row in out.chunks_mut(layout.row_bytes) {
        match layout.format {
            PixelFormat::Bgra => row[..layout.width as usize * 4].chunks_mut(4).for_each(|pixel| pixel[3] = 255),
            PixelFormat::Yuv => row[..PixelFormat::Yuv.row_bytes(layout.width)]
                .chunks_mut(4)
                .for_each(|pair| pair.copy_from_slice(&[128, 16, 128, 16])),
        }
    }
    out
}

/// A picture of `width` x `height` tightly packed RGBA rows, as a frame in `layout`.
///
/// `alpha` says how its fourth byte is to be read. With `keyed` (the card's keyer is on, and
/// the format is BGRA), the alpha goes with the picture: the colours as they are for straight
/// alpha, multiplied by the alpha for premultiplied, and a solid picture all solid. Without the
/// keyer the card drops the alpha, so a see-through picture is laid over black first (each
/// colour multiplied by its alpha), as a screen that can't be see-through shows it.
///
/// A picture of another size is scaled to the layout's, averaging the pixels each one covers
/// (on colours multiplied by their alpha, so that clear pixels never bleed colour into the
/// edges of solid ones).
pub fn to_frame(rgba: &[u8], width: u32, height: u32, layout: Layout, alpha: Alpha, keyed: bool) -> Vec<u8> {
    let keyed = keyed && layout.format == PixelFormat::Bgra;
    let pixels = width as usize * height as usize;
    if rgba.len() < pixels * 4 || width == 0 || height == 0 {
        return black(layout);
    }
    // Each pixel as it is to go, before packing: (r, g, b, a).
    let (w, h) = (layout.width, layout.height);
    let scaled;
    let (source, straight_out): (&[u8], bool) = if (width, height) == (w, h) {
        (rgba, false)
    } else {
        // Scaled with each colour multiplied by its alpha; a straight picture keyed straight
        // is divided again afterwards.
        let premultiplied = match alpha {
            Alpha::Opaque => solid(rgba),
            Alpha::Straight | Alpha::Premultiplied | Alpha::Key => premultiply(rgba),
        };
        scaled = scale(&premultiplied, width, height, w, h);
        (&scaled, true)
    };
    let mut out = vec![0; layout.frame_bytes()];
    for (y, row) in out.chunks_mut(layout.row_bytes).enumerate().take(h as usize) {
        let line = &source[y * w as usize * 4..(y + 1) * w as usize * 4];
        match layout.format {
            // A solid picture, the usual one: no alpha to work out for each pixel, so the rows
            // go straight through (the same bytes `pass` gives, a pixel at a time, for this).
            PixelFormat::Bgra if alpha == Alpha::Opaque => {
                for (into, pixel) in row.as_chunks_mut::<4>().0.iter_mut().zip(line.as_chunks::<4>().0) {
                    *into = [pixel[2], pixel[1], pixel[0], 255];
                }
            }
            PixelFormat::Yuv if alpha == Alpha::Opaque => {
                let (pairs, last) = line.as_chunks::<8>();
                for (into, pair) in row.as_chunks_mut::<4>().0.iter_mut().zip(pairs) {
                    let [r0, g0, b0, _, r1, g1, b1, _] = pair.map(i32::from);
                    *into = yuv_rgb([r0, g0, b0], [r1, g1, b1]);
                }
                // An odd width ends with one pixel, which is its own pair.
                if let (Some(into), Some(end)) = (row.as_chunks_mut::<4>().0.get_mut(pairs.len()), last.get(..4)) {
                    let one = [i32::from(end[0]), i32::from(end[1]), i32::from(end[2])];
                    *into = yuv_rgb(one, one);
                }
            }
            PixelFormat::Bgra => {
                for (into, pixel) in row.as_chunks_mut::<4>().0.iter_mut().zip(line.as_chunks::<4>().0) {
                    let [r, g, b, a] = pass(pixel, alpha, keyed, straight_out);
                    *into = [b, g, r, a];
                }
            }
            PixelFormat::Yuv => {
                for (into, pair) in row.as_chunks_mut::<4>().0.iter_mut().zip(line.as_chunks::<4>().0.chunks(2)) {
                    let first = pass(&pair[0], alpha, false, straight_out);
                    let second = pair.get(1).map_or(first, |second| pass(second, alpha, false, straight_out));
                    *into = yuv_pair(first, second);
                }
            }
        }
    }
    out
}

/// One pixel as it is to go. `scaled` says it has been scaled already, and so is a colour
/// multiplied by its alpha (or solid).
fn pass(&[r, g, b, a]: &[u8; 4], alpha: Alpha, keyed: bool, scaled: bool) -> [u8; 4] {
    if alpha == Alpha::Key {
        return [a, a, a, 255];
    }
    if scaled {
        return match (alpha, keyed) {
            (Alpha::Opaque, _) | (_, false) => [r, g, b, if keyed { a } else { 255 }],
            (Alpha::Premultiplied, true) => [r, g, b, a],
            (Alpha::Straight, true) => [unmultiply(r, a), unmultiply(g, a), unmultiply(b, a), a],
            (Alpha::Key, true) => [a, a, a, 255],
        };
    }
    match (alpha, keyed) {
        (Alpha::Opaque, _) => [r, g, b, 255],
        (Alpha::Straight, true) => [r, g, b, a],
        (Alpha::Premultiplied, true) => [times(r, a), times(g, a), times(b, a), a],
        (Alpha::Straight | Alpha::Premultiplied, false) => [times(r, a), times(g, a), times(b, a), 255],
        (Alpha::Key, _) => [a, a, a, 255],
    }
}

/// `colour × alpha / 255`, rounded to the nearest (with 255 odd it is never half-way).
fn times(colour: u8, alpha: u8) -> u8 {
    ((u16::from(colour) * u16::from(alpha) + 127) / 255) as u8
}

/// The colour a premultiplied one was, `colour × 255 / alpha`, rounded, at most 255.
fn unmultiply(colour: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    ((u32::from(colour) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8
}

fn solid(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    out.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 255);
    out
}

fn premultiply(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for pixel in out.as_chunks_mut::<4>().0 {
        let a = pixel[3];
        for colour in &mut pixel[..3] {
            *colour = times(*colour, a);
        }
    }
    out
}

/// Fixed-point weights are out of this many.
const ONE: u32 = 1 << 14;

/// For each pixel of a row (or column) of `to`, the source pixels it takes from and how much
/// of each, out of `ONE`: the source pixels it covers, by how much of each it covers, when
/// making smaller; the two nearest, by nearness, when making larger.
fn weights(from: u32, to: u32) -> Vec<Vec<(usize, u32)>> {
    let ratio = f64::from(from) / f64::from(to);
    (0..to)
        .map(|i| {
            let mut taps: Vec<(usize, f64)> = Vec::new();
            if ratio >= 1.0 {
                let (start, end) = (f64::from(i) * ratio, f64::from(i + 1) * ratio);
                let mut at = start.floor() as usize;
                while (at as f64) < end && at < from as usize {
                    let covered = (end.min(at as f64 + 1.0) - start.max(at as f64)).max(0.0);
                    if covered > 0.0 {
                        taps.push((at, covered));
                    }
                    at += 1;
                }
            } else {
                let centre = (f64::from(i) + 0.5) * ratio - 0.5;
                let left = centre.floor().max(0.0);
                let right = (left + 1.0).min(f64::from(from) - 1.0);
                let towards = (centre - left).clamp(0.0, 1.0);
                taps.push((left as usize, 1.0 - towards));
                taps.push((right as usize, towards));
            }
            let total: f64 = taps.iter().map(|(_, weight)| weight).sum();
            let mut fixed: Vec<(usize, u32)> =
                taps.iter().map(|(at, weight)| (*at, (weight / total * f64::from(ONE)).round() as u32)).collect();
            // Make them add up to exactly ONE, so a flat colour stays the same.
            let sum: u32 = fixed.iter().map(|(_, weight)| weight).sum();
            if let Some(biggest) = fixed.iter_mut().max_by_key(|(_, weight)| *weight) {
                biggest.1 = (biggest.1 + ONE).saturating_sub(sum);
            }
            fixed
        })
        .collect()
}

/// Scales RGBA rows from one size to another, across then down. Each pixel's four bytes are
/// worked out together, and the pass down adds whole rows at a time, which is what lets the
/// compiler do several bytes at once: about a fifth of the time the byte-by-byte way took.
fn scale(rgba: &[u8], width: u32, height: u32, to_width: u32, to_height: u32) -> Vec<u8> {
    let across = weights(width, to_width);
    let down = weights(height, to_height);
    let mut wide = vec![0u8; to_width as usize * height as usize * 4];
    for (line, into) in rgba.chunks_exact(width as usize * 4).zip(wide.chunks_exact_mut(to_width as usize * 4)) {
        for (into, taps) in into.as_chunks_mut::<4>().0.iter_mut().zip(&across) {
            let mut sum = [0u32; 4];
            for (at, weight) in taps {
                let pixel = &line[at * 4..at * 4 + 4];
                for (total, value) in sum.iter_mut().zip(pixel) {
                    *total += u32::from(*value) * weight;
                }
            }
            *into = sum.map(|total| ((total + ONE / 2) / ONE).min(255) as u8);
        }
    }
    let row = to_width as usize * 4;
    let mut out = vec![0u8; row * to_height as usize];
    let mut sums = vec![0u32; row];
    for (into, taps) in out.chunks_exact_mut(row).zip(&down) {
        sums.fill(0);
        for (line, weight) in taps {
            for (total, value) in sums.iter_mut().zip(&wide[line * row..(line + 1) * row]) {
                *total += u32::from(*value) * weight;
            }
        }
        for (value, total) in into.iter_mut().zip(&sums) {
            *value = ((total + ONE / 2) / ONE).min(255) as u8;
        }
    }
    out
}

/// BT.709's luma and colour differences, scaled to the 8-bit video range (Y 16 to 235, Cb and
/// Cr 16 to 240), as fixed-point weights out of 65536 for red, green and blue.
const Y: [i32; 3] = [11966, 40254, 4064];
const CB: [i32; 3] = [-6596, -22189, 28785];
const CR: [i32; 3] = [28785, -26145, -2640];

/// Two pixels side by side as the card's YUV 4:2:2 takes them: Cb, Y, Cr, Y, the colour
/// differences shared by the two (their average).
fn yuv_pair(first: [u8; 4], second: [u8; 4]) -> [u8; 4] {
    let rgb = |pixel: [u8; 4]| [i32::from(pixel[0]), i32::from(pixel[1]), i32::from(pixel[2])];
    yuv_rgb(rgb(first), rgb(second))
}

/// [`yuv_pair`] for two pixels' red, green and blue. Written flat, with no table of weights
/// and no alpha to look at, so the compiler can work on several pairs at once: a solid frame's
/// rows go through here, and for 1080p it takes under a third of the time.
fn yuv_rgb([r0, g0, b0]: [i32; 3], [r1, g1, b1]: [i32; 3]) -> [u8; 4] {
    let luma =
        |r: i32, g: i32, b: i32| ((Y[0] * r + Y[1] * g + Y[2] * b + (16 << 16) + (1 << 15)) >> 16).clamp(1, 254) as u8;
    let chroma = |w: [i32; 3]| {
        ((w[0] * (r0 + r1) + w[1] * (g0 + g1) + w[2] * (b0 + b1) + (256 << 16) + (1 << 16)) >> 17).clamp(1, 254) as u8
    };
    [chroma(CB), luma(r0, g0, b0), chroma(CR), luma(r1, g1, b1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(width: u32, height: u32, format: PixelFormat) -> Layout {
        Layout { width, height, format, row_bytes: format.row_bytes(width) }
    }

    fn picture(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        pixel.repeat(width as usize * height as usize)
    }

    /// The YUV of one colour, as two pixels of it give it.
    fn yuv(r: u8, g: u8, b: u8) -> [u8; 4] {
        yuv_pair([r, g, b, 255], [r, g, b, 255])
    }

    #[test]
    fn black_and_white_are_the_ends_of_the_video_range() {
        assert_eq!(yuv(0, 0, 0), [128, 16, 128, 16]);
        assert_eq!(yuv(255, 255, 255), [128, 235, 128, 235]);
        assert_eq!(black(layout(4, 2, PixelFormat::Yuv)), [128, 16, 128, 16].repeat(4));
        assert_eq!(black(layout(2, 1, PixelFormat::Bgra)), vec![0, 0, 0, 255, 0, 0, 0, 255]);
    }

    #[test]
    fn colour_bars_come_out_at_the_standard_75_per_cent_values() {
        // BT.709 75% colour bars in 8-bit video range: Y, Cb, Cr.
        let bars = [
            ((191, 191, 191), (180, 128, 128)),
            ((191, 191, 0), (168, 44, 136)),
            ((0, 191, 191), (145, 147, 44)),
            ((0, 191, 0), (133, 63, 52)),
            ((191, 0, 191), (63, 193, 204)),
            ((191, 0, 0), (51, 109, 212)),
            ((0, 0, 191), (28, 212, 120)),
        ];
        for ((r, g, b), (y, cb, cr)) in bars {
            assert_eq!(yuv(r, g, b), [cb, y, cr, y], "bar {r} {g} {b}");
        }
    }

    #[test]
    fn two_pixels_share_the_average_of_their_colour_and_keep_their_own_brightness() {
        assert_eq!(yuv_pair([0, 0, 0, 255], [255, 255, 255, 255]), [128, 16, 128, 235]);
        let [cb, _, cr, _] = yuv_pair([191, 0, 0, 255], [0, 0, 191, 255]);
        let ([red_cb, _, red_cr, _], [blue_cb, _, blue_cr, _]) = (yuv(191, 0, 0), yuv(0, 0, 191));
        assert!((i32::from(cb) - (i32::from(red_cb) + i32::from(blue_cb)) / 2).abs() <= 1);
        assert!((i32::from(cr) - (i32::from(red_cr) + i32::from(blue_cr)) / 2).abs() <= 1);
    }

    #[test]
    fn a_solid_picture_goes_to_bgra_with_its_bytes_swapped_and_all_solid() {
        let frame = to_frame(&[10, 20, 30, 7], 1, 1, layout(1, 1, PixelFormat::Bgra), Alpha::Opaque, false);
        assert_eq!(frame, vec![30, 20, 10, 255]);
        let keyed = to_frame(&[10, 20, 30, 7], 1, 1, layout(1, 1, PixelFormat::Bgra), Alpha::Opaque, true);
        assert_eq!(keyed, vec![30, 20, 10, 255], "a solid picture keys all solid");
    }

    #[test]
    fn a_see_through_picture_keeps_its_alpha_only_for_the_keyer() {
        let half_white = [255, 255, 255, 128];
        let bgra = layout(1, 1, PixelFormat::Bgra);
        assert_eq!(to_frame(&half_white, 1, 1, bgra, Alpha::Straight, true), vec![255, 255, 255, 128]);
        assert_eq!(to_frame(&half_white, 1, 1, bgra, Alpha::Premultiplied, true), vec![128, 128, 128, 128]);
        // Without the keyer, laid over black: the card drops the alpha.
        assert_eq!(to_frame(&half_white, 1, 1, bgra, Alpha::Straight, false), vec![128, 128, 128, 255]);
        let yuv = to_frame(&half_white.repeat(2), 2, 1, layout(2, 1, PixelFormat::Yuv), Alpha::Straight, true);
        assert_eq!(yuv, yuv_pair([128, 128, 128, 255], [128, 128, 128, 255]).to_vec(), "YUV never keys");
    }

    #[test]
    fn a_key_is_the_alpha_as_solid_greys_in_either_format() {
        let half_white = [255, 255, 255, 128];
        let bgra = layout(1, 1, PixelFormat::Bgra);
        assert_eq!(to_frame(&half_white, 1, 1, bgra, Alpha::Key, false), vec![128, 128, 128, 255]);
        assert_eq!(to_frame(&half_white, 1, 1, bgra, Alpha::Key, true), vec![128, 128, 128, 255], "solid even keyed");
        let solid_and_clear = [9, 9, 9, 255, 9, 9, 9, 0];
        let yuv = to_frame(&solid_and_clear, 2, 1, layout(2, 1, PixelFormat::Yuv), Alpha::Key, false);
        assert_eq!(yuv, yuv_pair([255, 255, 255, 255], [0, 0, 0, 255]).to_vec());
        assert_eq!([yuv[1], yuv[3]], [235, 16], "white where solid, black where clear");
        // Scaled to one pixel: half covered, half grey.
        let scaled = to_frame(&solid_and_clear, 2, 1, layout(1, 1, PixelFormat::Bgra), Alpha::Key, false);
        assert_eq!(scaled, vec![128, 128, 128, 255]);
    }

    #[test]
    fn a_1080_picture_scaled_to_720_keeps_flat_colours_and_averages_edges() {
        let grey = picture(1920, 1080, [100, 150, 200, 255]);
        let frame = to_frame(&grey, 1920, 1080, layout(1280, 720, PixelFormat::Bgra), Alpha::Opaque, false);
        assert_eq!(frame.len(), 1280 * 720 * 4);
        assert!(frame.chunks(4).all(|pixel| pixel == [200, 150, 100, 255]));
        // Left half white, right half black: the middle pixel of 720p covers both.
        let mut split = picture(1920, 1080, [0, 0, 0, 255]);
        for row in split.chunks_mut(1920 * 4) {
            row[..960 * 4].fill(255);
        }
        let frame = to_frame(&split, 1920, 1080, layout(1280, 720, PixelFormat::Bgra), Alpha::Opaque, false);
        assert_eq!(&frame[..4], &[255, 255, 255, 255]);
        assert_eq!(&frame[1279 * 4..1280 * 4], &[0, 0, 0, 255]);
        let middle: Vec<u8> =
            frame[..1280 * 4].chunks(4).map(|pixel| pixel[0]).filter(|v| *v != 0 && *v != 255).collect();
        assert!(middle.len() <= 1, "one pixel at most straddles the edge: {middle:?}");
    }

    #[test]
    fn scaling_a_see_through_picture_never_bleeds_colour_from_clear_pixels() {
        // A solid red pixel beside a clear one that holds green (clear pixels' colour is
        // anything): scaled to one pixel, keyed straight, it is red at half alpha.
        let rgba = [255, 0, 0, 255, 0, 255, 0, 0];
        let frame = to_frame(&rgba, 2, 1, layout(1, 1, PixelFormat::Bgra), Alpha::Straight, true);
        assert_eq!(frame, vec![0, 0, 255, 128]);
    }

    #[test]
    fn weights_always_add_up_to_one() {
        for (from, to) in [(1920, 1280), (1080, 720), (1280, 1920), (7, 3), (3, 7)] {
            for taps in weights(from, to) {
                assert_eq!(taps.iter().map(|(_, weight)| weight).sum::<u32>(), ONE, "{from} to {to}");
            }
        }
    }

    #[test]
    fn rows_the_card_makes_longer_are_padded_and_left_alone() {
        let padded = Layout { width: 2, height: 2, format: PixelFormat::Bgra, row_bytes: 12 };
        let frame = to_frame(&picture(2, 2, [1, 2, 3, 255]), 2, 2, padded, Alpha::Opaque, false);
        assert_eq!(frame.len(), 24);
        assert_eq!(&frame[8..12], &[0, 0, 0, 0]);
        assert_eq!(&frame[12..16], &[3, 2, 1, 255]);
    }

    #[test]
    fn a_picture_of_the_wrong_size_for_its_pixels_comes_out_black() {
        let frame = to_frame(&[1, 2, 3], 1, 1, layout(1, 1, PixelFormat::Bgra), Alpha::Opaque, false);
        assert_eq!(frame, black(layout(1, 1, PixelFormat::Bgra)));
    }

    #[test]
    fn a_solid_picture_comes_out_as_each_pixel_worked_out_alone_gives_it() {
        // The rows of a solid picture go through a shorter way than the rest; it must give the
        // same bytes, in both formats, for even and odd widths.
        for width in [1u32, 2, 5, 8, 33] {
            let height = 3;
            let rgba: Vec<u8> = (0..(width * height * 4) as usize).map(|i| (i * 37 % 256) as u8).collect();
            let bgra = to_frame(&rgba, width, height, layout(width, height, PixelFormat::Bgra), Alpha::Opaque, true);
            let expected: Vec<u8> = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| {
                    let [r, g, b, a] = pass(pixel, Alpha::Opaque, true, false);
                    [b, g, r, a]
                })
                .collect();
            assert_eq!(bgra, expected, "BGRA, width {width}");
            let yuv = to_frame(&rgba, width, height, layout(width, height, PixelFormat::Yuv), Alpha::Opaque, false);
            let mut expected = Vec::new();
            for line in rgba.chunks(width as usize * 4) {
                for pair in line.as_chunks::<4>().0.chunks(2) {
                    let first = pass(&pair[0], Alpha::Opaque, false, false);
                    let second = pair.get(1).map_or(first, |second| pass(second, Alpha::Opaque, false, false));
                    expected.extend(yuv_pair(first, second));
                }
            }
            assert_eq!(yuv, expected, "YUV, width {width}");
        }
    }

    /// Scaling as it was first written, a byte at a time, which `scale` must agree with.
    fn scale_a_byte_at_a_time(rgba: &[u8], width: u32, height: u32, to_width: u32, to_height: u32) -> Vec<u8> {
        let across = weights(width, to_width);
        let down = weights(height, to_height);
        let mut wide = vec![0u8; to_width as usize * height as usize * 4];
        for y in 0..height as usize {
            let line = &rgba[y * width as usize * 4..(y + 1) * width as usize * 4];
            let into = &mut wide[y * to_width as usize * 4..(y + 1) * to_width as usize * 4];
            for (x, taps) in across.iter().enumerate() {
                for channel in 0..4 {
                    let sum: u32 = taps.iter().map(|(at, weight)| u32::from(line[at * 4 + channel]) * weight).sum();
                    into[x * 4 + channel] = ((sum + ONE / 2) / ONE).min(255) as u8;
                }
            }
        }
        let row = to_width as usize * 4;
        let mut out = vec![0u8; row * to_height as usize];
        for (y, taps) in down.iter().enumerate() {
            let into = &mut out[y * row..(y + 1) * row];
            for (at, value) in into.iter_mut().enumerate() {
                let sum: u32 = taps.iter().map(|(line, weight)| u32::from(wide[line * row + at]) * weight).sum();
                *value = ((sum + ONE / 2) / ONE).min(255) as u8;
            }
        }
        out
    }

    #[test]
    fn scaling_gives_the_bytes_the_byte_at_a_time_way_gave() {
        for (from, to) in
            [((64, 36), (40, 23)), ((64, 36), (128, 72)), ((33, 17), (7, 5)), ((9, 9), (30, 20)), ((16, 9), (16, 9))]
        {
            let rgba: Vec<u8> = (0..from.0 * from.1 * 4).map(|i| (i * 37 % 256) as u8).collect();
            assert_eq!(
                scale(&rgba, from.0, from.1, to.0, to.1),
                scale_a_byte_at_a_time(&rgba, from.0, from.1, to.0, to.1),
                "{from:?} to {to:?}"
            );
        }
    }
}
