//! A tile's pixels: a picture decoded with the Lister's decoder and its limits
//! (`viewer/picture.rs`: a file's dimensions read from its header first, a decompression bomb
//! refused before a byte is decoded), scaled down to fit the tile, proportions kept. Blocking:
//! run it off the UI thread.

use ::image::{RgbaImage, imageops};
use rc_engine::viewer::sniff;

use crate::app::viewer::picture;

/// A tile's pixels, RGBA rows, at most the tile's side either way.
#[derive(Clone, PartialEq, Eq)]
pub struct Tile {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Tile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tile")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// `bytes`, a whole file, as a tile of at most `side` pixels a side: `None` for a file that is
/// no picture rCommander decodes, a damaged one, or one past the picture limits.
pub fn picture(bytes: &[u8], side: u32) -> Option<Tile> {
    let format = sniff::sniff_image(bytes)?;
    // Scaled while decoding to about the tile's area, then fitted into its square.
    let shown = u64::from(side) * u64::from(side);
    let pixels = picture::decode(bytes, format, shown).ok()?;
    fit(pixels.rgba, pixels.shown_width, pixels.shown_height, side)
}

/// RGBA pixels of `width` by `height` scaled down to fit a square of `side`; a smaller picture
/// stays as it is. `None` when the buffer does not match the size.
pub fn fit(rgba: Vec<u8>, width: u32, height: u32, side: u32) -> Option<Tile> {
    let image = RgbaImage::from_raw(width, height, rgba)?;
    let (fit_width, fit_height) = fitted(width, height, side);
    let image = if (fit_width, fit_height) == (width, height) {
        image
    } else {
        imageops::thumbnail(&image, fit_width, fit_height)
    };
    Some(Tile {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// The size of a picture of `width` by `height` fitted into a square of `side`: itself when it
/// fits, else scaled down by its longer side; never below one pixel.
pub fn fitted(width: u32, height: u32, side: u32) -> (u32, u32) {
    let side = side.max(1);
    if width <= side && height <= side {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(side) / f64::from(width.max(height));
    let length = |length: u32| ((f64::from(length) * scale).round() as u32).clamp(1, side);
    (length(width), length(height))
}

/// A bitmap of `width` by `height` in BGRA rows from the top - what Windows hands out - as a
/// tile: an image with no alpha at all is opaque.
#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "only Explorer's thumbnails come as BGRA; tested everywhere"
    )
)]
pub fn from_bgra(width: u32, height: u32, bgra: &[u8], side: u32) -> Option<Tile> {
    let len = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if bgra.len() != len || width == 0 || height == 0 {
        return None;
    }
    let pixels = bgra.as_chunks::<4>().0;
    let has_alpha = pixels.iter().any(|pixel| pixel[3] != 0);
    let mut rgba = Vec::with_capacity(len);
    for pixel in pixels {
        let alpha = if has_alpha { pixel[3] } else { 255 };
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], alpha]);
    }
    fit(rgba, width, height, side)
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
