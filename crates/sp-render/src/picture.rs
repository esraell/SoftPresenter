use std::sync::Arc;

use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

/// A decoded picture, ready to draw. Cheap to clone: the pixels are shared.
#[derive(Clone)]
pub struct Picture {
    pub(crate) image: ImageData,
}

/// The order of the four bytes of a pixel: red first, or blue first (as the system's video
/// player decodes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelOrder {
    Rgba,
    Bgra,
}

impl PixelOrder {
    pub(crate) fn format(self) -> ImageFormat {
        match self {
            PixelOrder::Rgba => ImageFormat::Rgba8,
            PixelOrder::Bgra => ImageFormat::Bgra8,
        }
    }
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Picture({}x{})", self.image.width, self.image.height)
    }
}

impl Picture {
    /// Makes a picture from rows of RGBA pixels with straight (not premultiplied) alpha.
    /// Returns `None` if either side is zero or the pixel count does not match the size.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Option<Picture> {
        Picture::from_pixels(width, height, rgba, ImageFormat::Rgba8)
    }

    fn from_pixels(width: u32, height: u32, pixels: Vec<u8>, format: ImageFormat) -> Option<Picture> {
        let expected = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return None;
        }
        let data = Blob::new(Arc::new(pixels));
        Some(Picture { image: ImageData { data, format, alpha_type: ImageAlphaType::Alpha, width, height } })
    }

    /// Makes a picture by copying rows of four-byte pixels, each row starting `bytes_per_row`
    /// after the last (which may be more than its pixels fill), in either order of bytes. The
    /// colours are copied as they are, not put in another order: the GPU reads either. With
    /// `opaque`, the copy is opaque whatever the fourth bytes say; otherwise they are its
    /// straight alpha. Returns `None` if either side is zero or there are too few bytes.
    pub fn from_rows(
        width: u32,
        height: u32,
        rows: &[u8],
        bytes_per_row: usize,
        order: PixelOrder,
        opaque: bool,
    ) -> Option<Picture> {
        let row = (width as usize).checked_mul(4)?;
        if !rows_hold(width, height, rows.len(), bytes_per_row) {
            return None;
        }
        let mut pixels = Vec::with_capacity(row.checked_mul(height as usize)?);
        if bytes_per_row == row && !opaque {
            pixels.extend_from_slice(&rows[..row * height as usize]);
        } else {
            for line in rows.chunks(bytes_per_row).take(height as usize) {
                pixels.extend_from_slice(&line[..row]);
            }
        }
        if opaque {
            let alpha = u32::from_ne_bytes([0, 0, 0, 255]);
            for pixel in pixels.as_chunks_mut::<4>().0 {
                *pixel = (u32::from_ne_bytes(*pixel) | alpha).to_ne_bytes();
            }
        }
        Picture::from_pixels(width, height, pixels, order.format())
    }

    /// True when both are the same picture (not merely alike).
    pub fn same(&self, other: &Picture) -> bool {
        self.image.data.id() == other.image.data.id()
    }

    /// A copy no larger than `max_side` on its longer side, each pixel the average of those it
    /// covers. A picture already small enough is handed back as it is (its pixels shared).
    pub fn shrunk(&self, max_side: u32) -> Picture {
        let (w, h) = (self.image.width, self.image.height);
        if w.max(h) <= max_side || max_side == 0 {
            return self.clone();
        }
        let scale = f64::from(max_side) / f64::from(w.max(h));
        let (nw, nh) = (((f64::from(w) * scale).round() as u32).max(1), ((f64::from(h) * scale).round() as u32).max(1));
        let source = self.image.data.data();
        let mut out = Vec::with_capacity((nw * nh * 4) as usize);
        for y in 0..nh {
            let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
            for x in 0..nw {
                let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
                let mut sum = [0u32; 4];
                for sy in y0..y1 {
                    let row = (sy * w) as usize * 4;
                    for sx in x0..x1 {
                        let at = row + sx as usize * 4;
                        for (total, value) in sum.iter_mut().zip(&source[at..at + 4]) {
                            *total += u32::from(*value);
                        }
                    }
                }
                let count = (y1 - y0) * (x1 - x0);
                out.extend(sum.map(|total| ((total + count / 2) / count) as u8));
            }
        }
        // Each channel is averaged where it stands, so the copy's bytes are in the same order.
        Picture::from_pixels(nw, nh, out, self.image.format).unwrap_or_else(|| self.clone())
    }

    /// The pixels, four bytes each, row after row, in the picture's own order of bytes.
    pub fn pixels(&self) -> &[u8] {
        self.image.data.data()
    }

    pub fn width(&self) -> u32 {
        self.image.width
    }

    pub fn height(&self) -> u32 {
        self.image.height
    }
}

/// True when `len` bytes hold `height` rows of `width` four-byte pixels, each row starting
/// `bytes_per_row` after the last, and neither side is zero.
pub(crate) fn rows_hold(width: u32, height: u32, len: usize, bytes_per_row: usize) -> bool {
    let needed = (|| {
        let row = (width as usize).checked_mul(4)?;
        if width == 0 || height == 0 || bytes_per_row < row {
            return None;
        }
        bytes_per_row.checked_mul(height as usize - 1)?.checked_add(row)
    })();
    needed.is_some_and(|needed| len >= needed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_pixel_count_must_agree() {
        assert!(Picture::from_rgba(2, 2, vec![0; 16]).is_some());
        assert!(Picture::from_rgba(2, 2, vec![0; 15]).is_none());
        assert!(Picture::from_rgba(0, 2, vec![]).is_none());
        assert!(Picture::from_rgba(2, 0, vec![]).is_none());
        let p = Picture::from_rgba(3, 1, vec![0; 12]).unwrap();
        assert_eq!((p.width(), p.height()), (3, 1));
    }

    #[test]
    fn rows_are_copied_without_their_padding_in_the_order_they_came() {
        // Two pixels a row, two rows, three bytes of padding after the first row.
        let rows = [1, 2, 3, 0, 4, 5, 6, 77, 99, 99, 99, 7, 8, 9, 0, 10, 11, 12, 200];
        let opaque = Picture::from_rows(2, 2, &rows, 11, PixelOrder::Bgra, true).unwrap();
        assert_eq!(opaque.image.format, ImageFormat::Bgra8);
        assert_eq!(opaque.image.data.data(), [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]);
        let clear = Picture::from_rows(2, 2, &rows, 11, PixelOrder::Rgba, false).unwrap();
        assert_eq!(clear.image.format, ImageFormat::Rgba8);
        assert_eq!(clear.image.data.data(), [1, 2, 3, 0, 4, 5, 6, 77, 7, 8, 9, 0, 10, 11, 12, 200]);
        // Rows with no padding, taken whole.
        let tight = Picture::from_rows(1, 2, &[1, 2, 3, 4, 5, 6, 7, 8], 4, PixelOrder::Rgba, false).unwrap();
        assert_eq!(tight.image.data.data(), [1, 2, 3, 4, 5, 6, 7, 8]);
        // Too few bytes, rows closer together than their pixels, or no size: no picture.
        assert!(Picture::from_rows(2, 2, &rows[..18], 11, PixelOrder::Bgra, true).is_none());
        assert!(Picture::from_rows(2, 2, &rows, 7, PixelOrder::Bgra, true).is_none());
        assert!(Picture::from_rows(0, 2, &rows, 11, PixelOrder::Bgra, true).is_none());
        assert!(Picture::from_rows(2, 0, &rows, 11, PixelOrder::Bgra, true).is_none());
    }

    #[test]
    fn a_shrunk_copy_keeps_the_order_of_its_bytes() {
        let rows: Vec<u8> = [[10, 20, 30, 255], [30, 40, 50, 255]].concat().repeat(2);
        let picture = Picture::from_rows(4, 1, &rows, 16, PixelOrder::Bgra, true).unwrap();
        let small = picture.shrunk(2);
        assert_eq!(small.image.format, ImageFormat::Bgra8);
        assert_eq!(small.image.data.data(), [20, 30, 40, 255, 20, 30, 40, 255]);
    }

    #[test]
    fn a_shrunk_copy_keeps_the_shape_and_averages_the_pixels() {
        // Four pixels across, two down: black and white columns, which average to grey.
        let rgba: Vec<u8> =
            (0..8).flat_map(|i| if i % 2 == 0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        let picture = Picture::from_rgba(4, 2, rgba).unwrap();
        let small = picture.shrunk(2);
        assert_eq!((small.width(), small.height()), (2, 1));
        assert_eq!(&small.image.data.data()[..4], &[128, 128, 128, 255]);
        // Small enough already: the same pixels, not a copy.
        assert!(picture.shrunk(10).same(&picture));
    }
}
