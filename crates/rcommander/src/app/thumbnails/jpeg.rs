//! A JPEG's tile without decoding the whole photo (RC-110-67): the thumbnail the camera embedded
//! in the EXIF block when it is large enough for the tile and shows the same picture (its
//! proportions), else the photo decoded at 1/2, 1/4 or 1/8 of its size in the inverse DCT
//! (`jpeg-decoder`), the smallest scale still at least the tile. A 6000 x 4000 photo for a
//! 96-pixel tile is decoded at 750 x 500: 1.1 MiB of RGB samples instead of 69.
//!
//! `None` for what this path does not take - a lossless or CMYK JPEG, 16-bit samples, a damaged
//! file - and the caller decodes it the general way. The picture limits hold as they do there:
//! the size from the header is checked before a byte is decoded. Blocking: run it off the UI
//! thread.

use exif::{In, Reader, Tag};
use jpeg_decoder::{CodingProcess, Decoder, PixelFormat};
use rc_engine::viewer::ImageFormat;

use super::decode::{self, Tile};
use crate::app::viewer::picture;

/// How far an embedded thumbnail's proportions may differ from the photo's, as a fraction: past
/// it the camera letterboxed it (160 x 120 for a 3:2 photo), or it is no longer this picture.
const ASPECT_TOLERANCE: f64 = 0.02;

/// A JPEG's pixels at the size they were decoded at, RGBA rows.
#[derive(Debug, PartialEq, Eq)]
struct Scaled {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// `bytes`, a whole JPEG file, as a tile of at most `side` pixels a side: from its EXIF
/// thumbnail or decoded scaled down; `None` when neither way takes it.
pub fn tile(bytes: &[u8], side: u32) -> Option<Tile> {
    let (decoder, size) = header(bytes)?;
    let wanted = decode::fitted(size.0, size.1, side);
    if let Some(tile) = decoder
        .exif_data()
        .and_then(|tiff| embedded(tiff, size, wanted, side))
    {
        return Some(tile);
    }
    let pixels = scaled(decoder, wanted)?;
    decode::fit(pixels.rgba, pixels.width, pixels.height, side)
}

/// A decoder with its header read, and the photo's width and height.
type Header<'a> = (Decoder<&'a [u8]>, (u32, u32));

/// A decoder over `bytes` with the header read - the EXIF block comes before the frame - and the
/// photo's size, within the picture limits.
fn header(bytes: &[u8]) -> Option<Header<'_>> {
    let mut decoder = Decoder::new(bytes);
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    let size = (u32::from(info.width), u32::from(info.height));
    picture::check_size(size.0, size.1).ok()?;
    Some((decoder, size))
}

/// The photo decoded at the smallest DCT scale that still gives the `wanted` size on one side at
/// least; `None` when it is no JPEG this path takes.
fn scaled(mut decoder: Decoder<&[u8]>, (width, height): (u32, u32)) -> Option<Scaled> {
    let info = decoder.info()?;
    if !matches!(
        info.coding_process,
        CodingProcess::DctSequential | CodingProcess::DctProgressive
    ) {
        return None;
    }
    let requested = (u16::try_from(width).ok()?, u16::try_from(height).ok()?);
    let (out_width, out_height) = decoder.scale(requested.0, requested.1).ok()?;
    let (out_width, out_height) = (u32::from(out_width), u32::from(out_height));
    // The decoder refuses an output larger than the scaled picture.
    let most = usize::try_from(u64::from(out_width) * u64::from(out_height) * 4).ok()?;
    decoder.set_max_decoding_buffer_size(most);
    let samples = decoder.decode().ok()?;
    let rgba = rgba(
        &samples,
        decoder.info()?.pixel_format,
        out_width,
        out_height,
    )?;
    Some(Scaled {
        width: out_width,
        height: out_height,
        rgba,
    })
}

/// Grey or RGB samples as RGBA; `None` for another format or a buffer of the wrong size.
fn rgba(samples: &[u8], format: PixelFormat, width: u32, height: u32) -> Option<Vec<u8>> {
    let pixels = usize::try_from(u64::from(width) * u64::from(height)).ok()?;
    if samples.len() != pixels.checked_mul(format.pixel_bytes())? {
        return None;
    }
    let mut rgba = Vec::with_capacity(pixels.checked_mul(4)?);
    match format {
        PixelFormat::L8 => {
            for &grey in samples {
                rgba.extend_from_slice(&[grey, grey, grey, 255]);
            }
        }
        PixelFormat::RGB24 => {
            for pixel in samples.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        PixelFormat::L16 | PixelFormat::CMYK32 => return None,
    }
    Some(rgba)
}

/// The thumbnail in the EXIF block `tiff` as the tile of a photo of `size`: only when it has at
/// least the `wanted` tile's longer side and the photo's proportions.
fn embedded(tiff: &[u8], size: (u32, u32), wanted: (u32, u32), side: u32) -> Option<Tile> {
    let thumbnail = thumbnail_bytes(tiff)?;
    let mut header = Decoder::new(thumbnail);
    header.read_info().ok()?;
    let info = header.info()?;
    let found = (u32::from(info.width), u32::from(info.height));
    if !fits(found, size, wanted) {
        return None;
    }
    let shown = u64::from(found.0) * u64::from(found.1);
    let pixels = picture::decode(thumbnail, ImageFormat::Jpeg, shown).ok()?;
    decode::fit(pixels.rgba, pixels.shown_width, pixels.shown_height, side)
}

/// Whether a thumbnail of `found` pixels stands in for a photo of `size` in a tile of `wanted`.
fn fits(found: (u32, u32), size: (u32, u32), wanted: (u32, u32)) -> bool {
    let (found_width, found_height) = found;
    let (width, height) = size;
    if found_width == 0 || found_height == 0 || width == 0 || height == 0 {
        return false;
    }
    if found_width.max(found_height) < wanted.0.max(wanted.1) {
        return false;
    }
    let aspect = f64::from(width) / f64::from(height);
    let found_aspect = f64::from(found_width) / f64::from(found_height);
    (found_aspect - aspect).abs() <= aspect * ASPECT_TOLERANCE
}

/// The JPEG the EXIF block `tiff` holds as its thumbnail (IFD1's `JPEGInterchangeFormat` and its
/// length), a slice of the block; `None` without one or when it points outside the block.
fn thumbnail_bytes(tiff: &[u8]) -> Option<&[u8]> {
    let exif = Reader::new().read_raw(tiff.to_vec()).ok()?;
    let at = |tag| {
        exif.get_field(tag, In::THUMBNAIL)
            .and_then(|field| field.value.get_uint(0))
            .and_then(|value| usize::try_from(value).ok())
    };
    let start = at(Tag::JPEGInterchangeFormat)?;
    let length = at(Tag::JPEGInterchangeFormatLength)?;
    tiff.get(start..start.checked_add(length)?)
}

#[cfg(test)]
#[path = "jpeg_tests.rs"]
mod tests;
