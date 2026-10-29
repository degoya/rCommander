//! The clipboard's image as a PNG (RC-040-08): Windows keeps a screenshot as a
//! device-independent bitmap (`CF_DIB`, read by `windows/clipboard.rs`), which is a BMP file
//! without its 14-byte file header; [`dib_to_png`] puts the header in front, reads it with the
//! `image` crate and writes a PNG. Elsewhere there is no reader yet and [`read_png`] has none.

use std::io::Cursor;

use iced::Task;
use image::ImageFormat;

/// The PNG of the image on the clipboard, `None` inside when it holds none. `None` where the
/// system has no reader.
#[cfg(windows)]
pub fn read_png() -> Option<Task<Option<Vec<u8>>>> {
    Some(super::windows::clipboard::read_dib().map(|dib| dib.as_deref().and_then(dib_to_png)))
}

/// The PNG of the image on the clipboard, `None` inside when it holds none. `None` where the
/// system has no reader.
#[cfg(not(windows))]
pub fn read_png() -> Option<Task<Option<Vec<u8>>>> {
    None
}

/// The PNG of the device-independent bitmap `dib`; `None` for one that cannot be read.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Windows reader and the tests")
)]
pub fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    let offset = pixel_offset(dib)?;
    let total = u32::try_from(dib.len().checked_add(14)?).ok()?;
    let mut bmp = Vec::with_capacity(dib.len() + 14);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&total.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&offset.to_le_bytes());
    bmp.extend_from_slice(dib);
    let image = image::load_from_memory_with_format(&bmp, ImageFormat::Bmp).ok()?;
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .ok()?;
    Some(png)
}

/// Where the pixels start in the BMP file `dib` becomes: the file header, the info header, the
/// colour masks a 40-byte header keeps after itself, and the colour table.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Windows reader and the tests")
)]
fn pixel_offset(dib: &[u8]) -> Option<u32> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(dib.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(dib.get(at..at + 4)?.try_into().ok()?));
    let header = u32_at(0)?;
    if header < 12 || usize::try_from(header).ok()? > dib.len() {
        return None;
    }
    if header == 12 {
        // BITMAPCOREHEADER: three bytes per colour.
        let bits = u16_at(10)?;
        let colours = if bits <= 8 { 3u32 << bits } else { 0 };
        return Some(14 + 12 + colours);
    }
    let bits = u16_at(14)?;
    let compression = u32_at(16)?;
    let used = if header >= 36 { u32_at(32)? } else { 0 };
    let masks = match (header, compression) {
        (40, 3) => 12,
        (40, 6) => 16,
        _ => 0,
    };
    let colours = match used {
        0 if bits <= 8 => 1u32 << bits,
        0 => 0,
        used => used,
    };
    14u32
        .checked_add(header)?
        .checked_add(masks)?
        .checked_add(colours.checked_mul(4)?)
}

#[cfg(test)]
#[path = "clipboard_image_tests.rs"]
mod tests;
