//! A clipboard bitmap becomes a PNG (RC-040-08).

use super::dib_to_png;

/// A 40-byte info header for a `width` by one bitmap of `bits` bits, `compression` as given.
fn header(width: i32, bits: u16, compression: u32) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&40u32.to_le_bytes());
    header.extend_from_slice(&width.to_le_bytes());
    header.extend_from_slice(&1i32.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&bits.to_le_bytes());
    header.extend_from_slice(&compression.to_le_bytes());
    header.extend_from_slice(&[0; 20]);
    header
}

/// The pixels of `png`, as RGBA.
fn pixels(png: &[u8]) -> Vec<[u8; 4]> {
    let image = image::load_from_memory(png).expect("a PNG").to_rgba8();
    image.pixels().map(|pixel| pixel.0).collect()
}

#[test]
fn a_24_bit_bitmap_becomes_a_png() {
    let mut dib = header(2, 24, 0);
    // A red and a green pixel, blue-green-red each, the row padded to four bytes.
    dib.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
    let png = dib_to_png(&dib).expect("converted");
    assert!(png.starts_with(b"\x89PNG"));
    assert_eq!(pixels(&png), [[255, 0, 0, 255], [0, 255, 0, 255]]);
}

/// A screenshot is often 32 bits with colour masks and an empty fourth byte: it stays opaque.
#[test]
fn a_32_bit_bitmap_with_masks_stays_opaque() {
    let mut dib = header(2, 32, 3);
    for mask in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff] {
        dib.extend_from_slice(&mask.to_le_bytes());
    }
    dib.extend_from_slice(&[255, 0, 0, 0, 0, 0, 255, 0]);
    let png = dib_to_png(&dib).expect("converted");
    assert_eq!(pixels(&png), [[0, 0, 255, 255], [255, 0, 0, 255]]);
}

#[test]
fn what_is_no_bitmap_is_refused() {
    assert_eq!(dib_to_png(b""), None);
    assert_eq!(dib_to_png(b"garbage that is no bitmap"), None);
    let mut short = header(64, 24, 0);
    short.extend_from_slice(&[0; 3]);
    assert_eq!(dib_to_png(&short), None, "too few pixels");
}
