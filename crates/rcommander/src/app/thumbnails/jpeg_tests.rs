//! A JPEG's tile: from the embedded thumbnail when it fits, else decoded at a fraction of the
//! photo's size, so a large photo never takes its full size in memory.

use std::io::Cursor;

use ::image::{ImageFormat as Codec, Rgb, RgbImage};

use super::{fits, header, scaled, thumbnail_bytes, tile};
use crate::app::thumbnails::decode::{self, Tile};

const RED: [u8; 3] = [220, 20, 20];
const GREEN: [u8; 3] = [20, 200, 20];

/// A JPEG of `width` by `height` in one colour.
fn jpeg(width: u32, height: u32, colour: [u8; 3]) -> Vec<u8> {
    let image = RgbImage::from_pixel(width, height, Rgb(colour));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), Codec::Jpeg)
        .expect("encode");
    bytes
}

/// One big-endian IFD entry of one value.
fn entry(tag: u16, kind: u16, value: [u8; 4]) -> Vec<u8> {
    let mut out = tag.to_be_bytes().to_vec();
    out.extend(kind.to_be_bytes());
    out.extend(1u32.to_be_bytes());
    out.extend(value);
    out
}

/// An EXIF block (a TIFF header and its IFDs) whose IFD1 says the thumbnail is `length` bytes,
/// followed by `thumbnail`.
fn tiff(thumbnail: &[u8], length: u32) -> Vec<u8> {
    let mut tiff = b"MM\0\x2a\0\0\0\x08".to_vec();
    // IFD0 at 8: one field (Orientation, a SHORT), then IFD1 at 8 + 2 + 12 + 4 = 26.
    tiff.extend(1u16.to_be_bytes());
    tiff.extend(entry(0x0112, 3, [0, 1, 0, 0]));
    tiff.extend(26u32.to_be_bytes());
    // IFD1 at 26: two fields, the thumbnail after them at 26 + 2 + 24 + 4 = 56.
    tiff.extend(2u16.to_be_bytes());
    tiff.extend(entry(0x0201, 4, 56u32.to_be_bytes()));
    tiff.extend(entry(0x0202, 4, length.to_be_bytes()));
    tiff.extend(0u32.to_be_bytes());
    tiff.extend_from_slice(thumbnail);
    tiff
}

/// `photo` with an APP1 EXIF segment holding `thumbnail` right after its SOI marker.
fn with_thumbnail(photo: &[u8], thumbnail: &[u8]) -> Vec<u8> {
    let length = u32::try_from(thumbnail.len()).expect("small");
    let block = tiff(thumbnail, length);
    let segment = u16::try_from(2 + 6 + block.len()).expect("one segment");
    let mut out = photo[..2].to_vec();
    out.extend([0xFF, 0xE1]);
    out.extend(segment.to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(block);
    out.extend_from_slice(&photo[2..]);
    out
}

/// The colour in the middle of `tile`.
fn middle(tile: &Tile) -> [u8; 3] {
    let at = ((tile.height / 2 * tile.width + tile.width / 2) * 4) as usize;
    [tile.rgba[at], tile.rgba[at + 1], tile.rgba[at + 2]]
}

fn is_red(colour: [u8; 3]) -> bool {
    colour[0] > 150 && colour[1] < 100
}

fn is_green(colour: [u8; 3]) -> bool {
    colour[1] > 150 && colour[0] < 100
}

#[test]
fn a_large_photo_is_decoded_at_an_eighth_of_its_sides() {
    let (width, height) = (1600, 1200);
    let bytes = jpeg(width, height, GREEN);
    let (decoder, size) = header(&bytes).expect("a JPEG");
    assert_eq!(size, (width, height));
    let wanted = decode::fitted(width, height, 96);
    let pixels = scaled(decoder, wanted).expect("decoded");
    assert_eq!((pixels.width, pixels.height), (200, 150));
    // The memory bound: the pixels decoded are a 64th of the whole photo's, 117 KiB, not 7.3 MiB
    // - for a 24 MP photo 1.4 MiB instead of 92.
    let whole = (width * height * 4) as usize;
    assert_eq!(pixels.rgba.len() * 64, whole);
    assert!(pixels.rgba.len() <= 128 * 1024);
}

#[test]
fn a_large_photo_without_a_thumbnail_becomes_its_tile() {
    let bytes = jpeg(1600, 1200, GREEN);
    let made = tile(&bytes, 96).expect("a tile");
    assert_eq!((made.width, made.height), (96, 72));
    assert_eq!(made.rgba.len(), 96 * 72 * 4);
    assert!(is_green(middle(&made)));
    let through = decode::picture(&bytes, 96).expect("the tile path takes JPEGs here");
    assert_eq!((through.width, through.height), (96, 72));
}

#[test]
fn the_embedded_thumbnail_stands_in_when_it_is_large_enough() {
    let bytes = with_thumbnail(&jpeg(1200, 900, GREEN), &jpeg(160, 120, RED));
    let made = tile(&bytes, 96).expect("a tile");
    assert_eq!((made.width, made.height), (96, 72));
    assert!(is_red(middle(&made)), "from the thumbnail");
}

#[test]
fn a_thumbnail_smaller_than_the_tile_is_passed_over() {
    let bytes = with_thumbnail(&jpeg(1200, 900, GREEN), &jpeg(160, 120, RED));
    let made = tile(&bytes, 256).expect("a tile");
    assert_eq!((made.width, made.height), (256, 192));
    assert!(is_green(middle(&made)), "decoded from the photo");
}

#[test]
fn a_letterboxed_thumbnail_is_passed_over() {
    // A 3:2 photo with a 4:3 thumbnail, as many cameras write it: black bars in the tile.
    let bytes = with_thumbnail(&jpeg(1200, 800, GREEN), &jpeg(160, 120, RED));
    let made = tile(&bytes, 96).expect("a tile");
    assert_eq!((made.width, made.height), (96, 64));
    assert!(is_green(middle(&made)));
}

#[test]
fn a_thumbnail_pointing_outside_its_block_is_none() {
    let thumbnail = jpeg(16, 16, RED);
    let whole = tiff(&thumbnail, u32::try_from(thumbnail.len()).expect("small"));
    assert_eq!(thumbnail_bytes(&whole), Some(thumbnail.as_slice()));
    let past = tiff(&thumbnail, u32::MAX);
    assert_eq!(thumbnail_bytes(&past), None);
    assert_eq!(thumbnail_bytes(b"no EXIF at all"), None);
}

#[test]
fn a_thumbnail_fits_by_its_longer_side_and_the_proportions() {
    assert!(fits((160, 120), (4000, 3000), (96, 72)));
    assert!(fits((120, 160), (3000, 4000), (72, 96)), "portrait");
    assert!(!fits((160, 120), (4000, 3000), (256, 192)), "too small");
    assert!(!fits((160, 120), (6000, 4000), (96, 64)), "letterboxed");
    assert!(!fits((0, 120), (4000, 3000), (96, 72)));
}

#[test]
fn what_is_no_jpeg_this_path_takes_gives_no_tile_here() {
    assert!(tile(b"\xFF\xD8 not much more", 96).is_none());
    assert!(tile(b"no JPEG at all", 96).is_none());
}
