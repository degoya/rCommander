//! Where a photo keeps its EXIF block, found by walking its segments or chunks header by header
//! (RC-100-99): JPEG's `APP1` segment that starts `Exif\0\0`, PNG's `eXIf` chunk, WebP's `EXIF`
//! chunk. Each step reads one header of a few bytes; the block itself is read once, and only
//! when it is at most [`MAX_EXIF_BYTES`]. A walk stops after [`MAX_STEPS`] headers, at the
//! picture data (JPEG's start of scan), at the end of the file or at anything it cannot read -
//! a crafted length never makes it read more or loop.

use rc_vfs::VfsError;

use super::{PhotoExif, parse};
use crate::viewer::{FileWindow, ImageFormat};

/// The largest EXIF block read: what JPEG's `APP1` can hold, and what a camera writes into PNG
/// or WebP too. kamadak-exif copies each field's value out of the block when it is read, so the
/// block's size times [`super::MAX_FIELDS`] bounds the memory one crafted block can take.
pub const MAX_EXIF_BYTES: usize = 64 << 10;
/// The most headers one walk reads.
pub(crate) const MAX_STEPS: usize = 4096;

/// What the walk does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Read the header of `len` bytes at `offset`.
    Header { offset: u64, len: usize },
    /// The EXIF block is the `len` bytes at `offset`.
    Block { offset: u64, len: usize },
    /// No EXIF here.
    None,
}

/// The header size of each walk: a JPEG marker with its length and the `Exif\0\0` that tells
/// an EXIF `APP1` from an XMP one; a PNG chunk's length and type; a RIFF chunk's.
fn header_len(format: ImageFormat) -> usize {
    match format {
        ImageFormat::Jpeg => 10,
        _ => 8,
    }
}

/// The first step for a file of `format`; `None` for a format without EXIF.
pub(crate) fn first(format: ImageFormat) -> Step {
    let offset = match format {
        // After the start-of-image marker.
        ImageFormat::Jpeg => 2,
        // After the eight-byte signature.
        ImageFormat::Png => 8,
        // After `RIFF`, its size and `WEBP`.
        ImageFormat::WebP => 12,
        _ => return Step::None,
    };
    Step::Header {
        offset,
        len: header_len(format),
    }
}

/// The step after reading `header` at `at` in a file of `size` bytes.
pub(crate) fn next(format: ImageFormat, at: u64, header: &[u8], size: u64) -> Step {
    let found = match format {
        ImageFormat::Jpeg => jpeg(at, header),
        ImageFormat::Png => png(at, header),
        ImageFormat::WebP => webp(at, header),
        _ => None,
    };
    match found {
        Some(Step::Block { offset, len }) => {
            let fits = offset
                .checked_add(len as u64)
                .is_some_and(|end| end <= size);
            if len == 0 || len > MAX_EXIF_BYTES || !fits {
                Step::None
            } else {
                Step::Block { offset, len }
            }
        }
        Some(Step::Header { offset, len }) if offset < size => Step::Header { offset, len },
        _ => Step::None,
    }
}

fn jpeg(at: u64, header: &[u8]) -> Option<Step> {
    let (&lead, &marker) = (header.first()?, header.get(1)?);
    if lead != 0xFF {
        return None;
    }
    let step = |offset: u64| Step::Header {
        offset,
        len: header_len(ImageFormat::Jpeg),
    };
    match marker {
        // A fill byte: the marker starts one byte later.
        0xFF => return Some(step(at + 1)),
        // Markers without a length: TEM and the restart markers.
        0x01 | 0xD0..=0xD7 => return Some(step(at + 2)),
        // The picture data begins, or the image ends: EXIF comes before either.
        0xD9 | 0xDA => return None,
        _ => {}
    }
    let length = u16::from_be_bytes([*header.get(2)?, *header.get(3)?]);
    if length < 2 {
        return None;
    }
    if marker == 0xE1 && header.get(4..10) == Some(b"Exif\0\0".as_slice()) {
        return Some(Step::Block {
            offset: at + 10,
            len: usize::from(length).checked_sub(8)?,
        });
    }
    Some(step(at.checked_add(2 + u64::from(length))?))
}

fn png(at: u64, header: &[u8]) -> Option<Step> {
    let length = u32::from_be_bytes(header.get(0..4)?.try_into().ok()?);
    let kind = header.get(4..8)?;
    match kind {
        b"eXIf" => Some(Step::Block {
            offset: at + 8,
            len: usize::try_from(length).ok()?,
        }),
        // The image ends; and an `eXIf` after the data is allowed, so `IDAT` is walked past.
        b"IEND" => None,
        _ => Some(Step::Header {
            // Length, type, data and CRC.
            offset: at.checked_add(12 + u64::from(length))?,
            len: header_len(ImageFormat::Png),
        }),
    }
}

fn webp(at: u64, header: &[u8]) -> Option<Step> {
    let kind = header.get(0..4)?;
    let length = u32::from_le_bytes(header.get(4..8)?.try_into().ok()?);
    if kind == b"EXIF" {
        return Some(Step::Block {
            offset: at + 8,
            len: usize::try_from(length).ok()?,
        });
    }
    // A chunk's data is padded to an even length.
    let padded = u64::from(length) + u64::from(length & 1);
    Some(Step::Header {
        offset: at.checked_add(8 + padded)?,
        len: header_len(ImageFormat::WebP),
    })
}

/// The TIFF data of a block: PNG and WebP writers sometimes keep JPEG's `Exif\0\0` before it.
pub(crate) fn tiff_of(mut block: Vec<u8>) -> Vec<u8> {
    if block.starts_with(b"Exif\0\0") {
        block.drain(..6);
    }
    block
}

/// The EXIF of `file`, a picture of `format`: `None` when it has none or it cannot be read as
/// EXIF.
///
/// # Errors
///
/// The backend failed to read the file.
pub async fn read_exif(
    file: &FileWindow,
    format: ImageFormat,
) -> Result<Option<PhotoExif>, VfsError> {
    let size = file.size();
    let mut step = first(format);
    for _ in 0..MAX_STEPS {
        match step {
            Step::Header { offset, len } => {
                let header = file.read_uncached(offset, len).await?;
                if header.len() < len {
                    return Ok(None);
                }
                step = next(format, offset, &header, size);
            }
            Step::Block { offset, len } => {
                let block = file.read_uncached(offset, len).await?;
                if block.len() < len {
                    return Ok(None);
                }
                return Ok(parse(tiff_of(block)));
            }
            Step::None => return Ok(None),
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "locate_tests.rs"]
mod tests;
