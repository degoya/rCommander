//! A picture for the image view: decoded with the `image` crate, off the UI thread, within
//! limits a crafted file cannot get past.
//!
//! A file is read whole only up to [`MAX_FILE_BYTES`]; its dimensions are read from the header
//! first and refused above [`MAX_SIDE`] or [`MAX_PIXELS`] (a decompression bomb claims billions
//! of pixels in a few bytes), and the decoder runs under the same limits. A picture with more
//! pixels than the view shows (`viewer.picture_megapixels`) is scaled down before it reaches the
//! window, as Total Commander shows it; its own size stays what zoom and status line go by
//! (owner, 2026-10-19: a 5360 x 32667 JPEG was refused). A damaged file is a message
//! (`image_broken`), never a crash. GIF shows its first frame; ICO its largest image. A viewer
//! plugin's raw RGBA raster (RC-100-29) is held to the same limits and scaled the same way.

use std::io::Cursor;

use ::image::{ImageError, ImageFormat as Codec, ImageReader, Limits, RgbaImage, imageops};
use rc_engine::viewer::ImageFormat;

/// The largest image file read for the image view.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// The most pixels a picture may have: 256 Mi, 768 MiB decoded as RGB.
pub const MAX_PIXELS: u64 = 256 * 1024 * 1024;
/// The most pixels the view shows of a picture by default, in millions: 128 MiB as RGBA.
pub const DEFAULT_SHOWN_MEGAPIXELS: u64 = 32;
/// The longest side a picture may have.
pub const MAX_SIDE: u32 = 32 * 1024;

/// The error codes of this module, which the catalogue holds as `error.<code>`.
#[cfg(test)]
pub const ALL_CODES: &[&str] = &["image_too_large", "image_broken"];

/// A decoded picture, as RGBA rows - scaled down when it has more pixels than are shown.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    /// The picture's own size.
    pub width: u32,
    pub height: u32,
    /// The size of `rgba`.
    pub shown_width: u32,
    pub shown_height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pixels")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("shown_width", &self.shown_width)
            .field("shown_height", &self.shown_height)
            .finish_non_exhaustive()
    }
}

/// Whether a picture of `width` by `height` may be decoded: `image_too_large` otherwise.
pub fn check_size(width: u32, height: u32) -> Result<(), &'static str> {
    let pixels = u64::from(width) * u64::from(height);
    if width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        return Err("image_too_large");
    }
    Ok(())
}

/// `bytes`, a whole file in `format`, decoded, and scaled down to at most `shown` pixels. Blocks:
/// run it on a blocking thread.
pub fn decode(bytes: &[u8], format: ImageFormat, shown: u64) -> Result<Pixels, &'static str> {
    let codec = codec(format);
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), codec)
        .into_dimensions()
        .map_err(code)?;
    check_size(width, height)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), codec);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);
    let mut picture = reader.decode().map_err(code)?;
    let (shown_width, shown_height) = shown_size(width, height, shown);
    if (shown_width, shown_height) != (width, height) {
        // Scaled in the decoded colour type, before the RGBA copy: the full picture is never
        // held twice.
        picture = picture.thumbnail_exact(shown_width, shown_height);
    }
    let rgba = picture.into_rgba8();
    Ok(Pixels {
        width,
        height,
        shown_width: rgba.width(),
        shown_height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// `rgba`, a raw raster of `width` by `height`, scaled down to at most `shown` pixels. Blocks: run
/// it on a blocking thread.
pub fn raster(rgba: Vec<u8>, width: u32, height: u32, shown: u64) -> Result<Pixels, &'static str> {
    check_size(width, height)?;
    let expected = u64::from(width) * u64::from(height) * 4;
    if u64::try_from(rgba.len()).ok() != Some(expected) {
        return Err("image_broken");
    }
    let mut picture = RgbaImage::from_raw(width, height, rgba).ok_or("image_broken")?;
    let (shown_width, shown_height) = shown_size(width, height, shown);
    if (shown_width, shown_height) != (width, height) {
        picture = imageops::thumbnail(&picture, shown_width, shown_height);
    }
    Ok(Pixels {
        width,
        height,
        shown_width: picture.width(),
        shown_height: picture.height(),
        rgba: picture.into_raw(),
    })
}

/// The size a picture of `width` by `height` is shown at: itself while it has at most `shown`
/// pixels, else scaled down to them, keeping its proportions; never below one pixel a side.
pub fn shown_size(width: u32, height: u32, shown: u64) -> (u32, u32) {
    let pixels = u64::from(width) * u64::from(height);
    if pixels <= shown.max(1) {
        return (width, height);
    }
    let side = |length: u32, factor: f64| ((f64::from(length) * factor).floor() as u32).max(1);
    let factor = (shown as f64 / pixels as f64).sqrt();
    (side(width, factor), side(height, factor))
}

fn codec(format: ImageFormat) -> Codec {
    match format {
        ImageFormat::Png => Codec::Png,
        ImageFormat::Jpeg => Codec::Jpeg,
        ImageFormat::Gif => Codec::Gif,
        ImageFormat::WebP => Codec::WebP,
        ImageFormat::Bmp => Codec::Bmp,
        ImageFormat::Ico => Codec::Ico,
    }
}

fn code(error: ImageError) -> &'static str {
    match error {
        ImageError::Limits(_) => "image_too_large",
        _ => "image_broken",
    }
}

#[cfg(test)]
#[path = "picture_tests.rs"]
mod tests;
