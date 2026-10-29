//! Icon pixels: the RGBA image the list draws, and the conversion from the 32-bit BGRA rows a
//! Windows icon bitmap yields. Portable, so the conversion is tested on every platform.

/// An icon as straight (not premultiplied) RGBA, row by row from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// The RGBA image of an icon's colour bitmap `color` (BGRA, top-down, `width * height * 4`
/// bytes) and its AND mask `mask` read the same way (black opaque, white transparent).
///
/// An icon whose colour bitmap carries alpha keeps it, so its soft edges stay soft on light and
/// dark rows alike; an older icon without alpha takes its transparency from the mask; with
/// neither, it is opaque. `None` when a buffer does not match the size.
// Only the Windows shell module reads icon bitmaps; the tests hold the conversion everywhere.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows shell module reads icon bitmaps")
)]
pub fn from_bgra(width: u32, height: u32, color: &[u8], mask: Option<&[u8]>) -> Option<Rgba> {
    let len = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if color.len() != len || mask.is_some_and(|mask| mask.len() != len) {
        return None;
    }
    let has_alpha = color.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0);
    let mut pixels = Vec::with_capacity(len);
    for (index, pixel) in color.as_chunks::<4>().0.iter().enumerate() {
        let alpha = match mask {
            _ if has_alpha => pixel[3],
            // A set mask bit reads back white: the pixel is transparent.
            Some(mask) if mask[index * 4] != 0 => 0,
            _ => 255,
        };
        pixels.extend_from_slice(&[pixel[2], pixel[1], pixel[0], alpha]);
    }
    Some(Rgba {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
#[path = "pixels_tests.rs"]
mod tests;
