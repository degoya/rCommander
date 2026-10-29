//! The generic icons: a page for a file, a folder for a folder. They stand in where no native
//! icon is (Linux, macOS, the setting "generic") and until a native one has loaded.
//!
//! They are drawn here, in code - rCommander's own work under its licence, with no image file
//! and no third-party icon set to account for. Each is drawn at 32 px from shapes on a 16-unit
//! grid with 4x4 samples per pixel, so the edges are smooth at every scaling.

use super::pixels::Rgba;

/// The side of a drawn icon in pixels: sharp at 200 %, scaled down below.
pub const SIDE: u32 = 32;
/// Samples per pixel and axis.
const SAMPLES: u32 = 4;

/// What a generic icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    File,
    Folder,
}

type Rgb = [u8; 3];

/// The colours of one look.
struct Palette {
    page: Rgb,
    page_edge: Rgb,
    folder_back: Rgb,
    folder_front: Rgb,
}

const LIGHT: Palette = Palette {
    page: [0xFF, 0xFF, 0xFF],
    page_edge: [0x8A, 0x8A, 0x8A],
    folder_back: [0xD9, 0xA4, 0x2E],
    folder_front: [0xF2, 0xC5, 0x5C],
};

const DARK: Palette = Palette {
    page: [0x4A, 0x4A, 0x4A],
    page_edge: [0xB0, 0xB0, 0xB0],
    folder_back: [0xC4, 0x8F, 0x22],
    folder_front: [0xE8, 0xB6, 0x4C],
};

/// The generic icon `glyph` for the light or the dark look.
pub fn draw(glyph: Glyph, dark: bool) -> Rgba {
    let palette = if dark { &DARK } else { &LIGHT };
    let shape = |x: f32, y: f32| match glyph {
        Glyph::File => page(palette, x, y),
        Glyph::Folder => folder(palette, x, y),
    };
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    let per_pixel = (SAMPLES * SAMPLES) as f32;
    let unit = 16.0 / SIDE as f32;
    for py in 0..SIDE {
        for px in 0..SIDE {
            let mut sum = [0.0f32; 3];
            let mut covered = 0.0f32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * unit;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * unit;
                    if let Some(color) = shape(x, y) {
                        for (total, channel) in sum.iter_mut().zip(color) {
                            *total += f32::from(channel);
                        }
                        covered += 1.0;
                    }
                }
            }
            if covered == 0.0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // Straight alpha: the colour of the covered samples, the coverage as alpha.
            let channel = |total: f32| (total / covered).round() as u8;
            let alpha = (covered / per_pixel * 255.0).round() as u8;
            pixels.extend_from_slice(&[channel(sum[0]), channel(sum[1]), channel(sum[2]), alpha]);
        }
    }
    Rgba {
        width: SIDE,
        height: SIDE,
        pixels,
    }
}

/// A sheet with its top right corner folded, an edge one unit wide.
fn page(palette: &Palette, x: f32, y: f32) -> Option<Rgb> {
    let outer = (3.0..13.0).contains(&x) && (1.0..15.0).contains(&y) && x - y < 8.0;
    if !outer {
        return None;
    }
    let inner = (4.0..12.0).contains(&x) && (2.0..14.0).contains(&y) && x - y < 6.6;
    // The fold: the triangle between the cut corner and the page.
    let fold = x >= 9.0 && y < 5.0;
    Some(if inner && !fold {
        palette.page
    } else {
        palette.page_edge
    })
}

/// A folder: the back with its tab, the front lower and lighter.
fn folder(palette: &Palette, x: f32, y: f32) -> Option<Rgb> {
    let front = (1.0..15.0).contains(&x) && (5.5..13.5).contains(&y);
    if front {
        return Some(palette.folder_front);
    }
    let tab = (1.0..7.0).contains(&x) && (2.5..4.0).contains(&y) && x - y < 4.5;
    let back = (1.0..15.0).contains(&x) && (4.0..13.5).contains(&y);
    (tab || back).then_some(palette.folder_back)
}

#[cfg(test)]
#[path = "fallback_tests.rs"]
mod tests;
