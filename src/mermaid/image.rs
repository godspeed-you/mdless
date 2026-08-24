//! PNG handling for `mmdc`-rendered diagrams.
//!
//! This module deliberately emits **no terminal escape sequences**: the Kitty /
//! iTerm2 / Sixel encoders live in `terminal::protocols` (Workstream A). Here we
//! only decode the PNG dimensions and compute the cell box the image should
//! occupy.

use std::fmt;

use crate::util::DEFAULT_CELL_PIXELS;

/// A decoded diagram image: the original PNG bytes plus its pixel size.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageData {
    /// Original PNG bytes, ready to be handed to a terminal image protocol.
    pub png: Vec<u8>,
    /// Image width in pixels.
    pub width_px: u32,
    /// Image height in pixels.
    pub height_px: u32,
}

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("png", &format_args!("{} bytes", self.png.len()))
            .field("width_px", &self.width_px)
            .field("height_px", &self.height_px)
            .finish()
    }
}

/// A PNG that could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot decode diagram image: {0}")]
pub struct ImageDecodeError(pub String);

impl ImageData {
    /// Decodes `png` far enough to learn its pixel dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`ImageDecodeError`] when the bytes are not a readable PNG.
    pub fn from_png(png: Vec<u8>) -> Result<Self, ImageDecodeError> {
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(&png), image::ImageFormat::Png);
        let (width_px, height_px) = reader
            .into_dimensions()
            .map_err(|e| ImageDecodeError(e.to_string()))?;
        if width_px == 0 || height_px == 0 {
            return Err(ImageDecodeError("image has zero extent".to_string()));
        }
        Ok(Self {
            png,
            width_px,
            height_px,
        })
    }

    /// Computes the `(cols, rows)` cell box for this image.
    ///
    /// `cell_pixels` is the terminal's `(width, height)` of one character cell.
    /// The aspect ratio is preserved and the result is clamped to `max_cols`
    /// columns. Both components are at least `1`. A `cell_pixels` component of
    /// `0` (unknown cell size) falls back to a conventional `8 x 16` cell.
    #[must_use]
    pub fn cell_size(&self, cell_pixels: (u16, u16), max_cols: usize) -> (u16, u16) {
        let cw = u32::from(if cell_pixels.0 == 0 {
            DEFAULT_CELL_PIXELS.0
        } else {
            cell_pixels.0
        });
        let ch = u32::from(if cell_pixels.1 == 0 {
            DEFAULT_CELL_PIXELS.1
        } else {
            cell_pixels.1
        });

        let cols = self.width_px.div_ceil(cw).max(1);
        let rows = self.height_px.div_ceil(ch).max(1);

        let max_cols = u32::from(u16::try_from(max_cols.max(1)).unwrap_or(u16::MAX));
        if cols <= max_cols {
            return (clamp_u16(cols), clamp_u16(rows));
        }
        // Scale down preserving the pixel aspect ratio:
        //   rows = height_px * (max_cols * cw / width_px) / ch
        let scaled_rows = (u64::from(self.height_px) * u64::from(max_cols) * u64::from(cw))
            .div_ceil(u64::from(self.width_px) * u64::from(ch))
            .max(1);
        (
            clamp_u16(max_cols),
            clamp_u16(u32::try_from(scaled_rows).unwrap_or(u32::MAX)),
        )
    }
}

fn clamp_u16(v: u32) -> u16 {
    u16::try_from(v).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a real PNG of the given size (the `image` crate encodes it).
    fn png(w: u32, h: u32) -> Vec<u8> {
        let buf = image::RgbaImage::new(w, h);
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn decodes_dimensions() {
        let img = ImageData::from_png(png(120, 60)).unwrap();
        assert_eq!((img.width_px, img.height_px), (120, 60));
    }

    #[test]
    fn rejects_garbage() {
        assert!(ImageData::from_png(vec![]).is_err());
        assert!(ImageData::from_png(b"not a png".to_vec()).is_err());
        assert!(ImageData::from_png(vec![0xff; 4096]).is_err());
    }

    #[test]
    fn cell_size_without_clamping() {
        let img = ImageData::from_png(png(80, 32)).unwrap();
        // 80/8 = 10 cols, 32/16 = 2 rows.
        assert_eq!(img.cell_size((8, 16), 80), (10, 2));
    }

    #[test]
    fn cell_size_rounds_up() {
        let img = ImageData::from_png(png(81, 33)).unwrap();
        assert_eq!(img.cell_size((8, 16), 80), (11, 3));
    }

    #[test]
    fn cell_size_clamps_and_preserves_aspect() {
        let img = ImageData::from_png(png(800, 400)).unwrap();
        // Unclamped: 100 x 25. Clamped to 40 cols → 40/100 of the height.
        assert_eq!(img.cell_size((8, 16), 40), (40, 10));
    }

    #[test]
    fn unknown_cell_size_uses_defaults() {
        let img = ImageData::from_png(png(80, 32)).unwrap();
        assert_eq!(img.cell_size((0, 0), 80), (10, 2));
    }

    /// Regression: `terminal::protocols::{fit_cells, cells_for_pixels}` used to
    /// duplicate this computation and disagreed with it at exactly these
    /// inputs (they scaled the rounded cell counts, not the raw pixels, and
    /// treated `max_cols == 0` as unlimited). They are gone; this pins the one
    /// surviving answer.
    #[test]
    fn cell_size_scales_pixels_not_rounded_cells() {
        let img = ImageData::from_png(png(81, 33)).unwrap();
        // 81x33 px at 8x16 rounds up to 11x3 cells; scaling the *pixels* to
        // 4 columns gives 1 row, scaling the rounded cells would give 2.
        assert_eq!(img.cell_size((8, 16), 4), (4, 1));
        // `max_cols == 0` means "one column", never "unlimited".
        assert_eq!(img.cell_size((8, 16), 0), (1, 1));
    }

    #[test]
    fn unknown_cell_size_matches_the_shared_default() {
        let img = ImageData::from_png(png(80, 32)).unwrap();
        assert_eq!(
            img.cell_size((0, 0), 80),
            img.cell_size(DEFAULT_CELL_PIXELS, 80)
        );
        assert_eq!(
            crate::terminal::capabilities::Capabilities::default().cell_size(),
            DEFAULT_CELL_PIXELS
        );
    }

    #[test]
    fn never_returns_zero() {
        let img = ImageData::from_png(png(1, 1)).unwrap();
        let (c, r) = img.cell_size((100, 100), 1);
        assert!(c >= 1 && r >= 1);
        let (c, r) = img.cell_size((8, 16), 0);
        assert!(c >= 1 && r >= 1);
    }
}
