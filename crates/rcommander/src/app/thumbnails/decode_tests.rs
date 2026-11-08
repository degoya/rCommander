//! A tile from a picture: fitted into its square, and never from a bomb or a broken file.

use std::io::Cursor;

use ::image::{ImageFormat, Rgba, RgbaImage};

use super::{fit, fitted, from_bgra, picture};

/// A PNG of `width` by `height` in one colour.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba([200, 30, 30, 255]));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode");
    bytes
}

/// The CRC-32 of a PNG chunk.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = u32::try_from(data.len())
        .expect("small")
        .to_be_bytes()
        .to_vec();
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
}

/// A PNG whose header claims 100 000 by 100 000 pixels, in a few bytes.
fn bomb() -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&100_000u32.to_be_bytes());
    header.extend_from_slice(&100_000u32.to_be_bytes());
    // 8-bit RGBA, deflate, adaptive filter, no interlace.
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &header));
    // A few compressed bytes that would inflate to rows of zeros.
    png.extend(chunk(
        b"IDAT",
        &[0x78, 0x9c, 0x63, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01],
    ));
    png.extend(chunk(b"IEND", &[]));
    png
}

#[test]
fn a_decompression_bomb_gives_no_tile_and_no_crash() {
    assert_eq!(picture(&bomb(), 96), None);
}

#[test]
fn a_broken_or_foreign_file_gives_no_tile() {
    let mut cut = png(40, 40);
    cut.truncate(cut.len() / 2);
    assert_eq!(picture(&cut, 96), None);
    assert_eq!(picture(b"%PDF-1.7 not a picture", 96), None);
}

#[test]
fn a_picture_is_fitted_into_its_tile_keeping_its_proportions() {
    let tile = picture(&png(300, 150), 96).expect("a tile");
    assert_eq!((tile.width, tile.height), (96, 48));
    assert_eq!(tile.rgba.len(), 96 * 48 * 4);
    let small = picture(&png(20, 10), 96).expect("a tile");
    assert_eq!((small.width, small.height), (20, 10), "never enlarged");
}

#[test]
fn fitting_keeps_one_pixel_and_the_square() {
    assert_eq!(fitted(4000, 3000, 96), (96, 72));
    assert_eq!(fitted(3000, 4000, 96), (72, 96));
    assert_eq!(fitted(10_000, 1, 96), (96, 1));
    assert_eq!(fitted(50, 50, 96), (50, 50));
    assert_eq!(
        fit(vec![0; 3], 2, 2, 96),
        None,
        "a buffer of the wrong size"
    );
}

#[test]
fn a_bgra_bitmap_without_alpha_is_opaque() {
    let bgra = [10, 20, 30, 0, 40, 50, 60, 0];
    let tile = from_bgra(2, 1, &bgra, 96).expect("a tile");
    assert_eq!(tile.rgba, [30, 20, 10, 255, 60, 50, 40, 255]);
    let with_alpha = from_bgra(1, 1, &[1, 2, 3, 128], 96).expect("a tile");
    assert_eq!(with_alpha.rgba, [3, 2, 1, 128]);
    assert_eq!(from_bgra(2, 2, &bgra, 96), None, "too short");
}
