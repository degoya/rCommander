//! The walk to a photo's EXIF block: JPEG past other `APP` segments and an XMP `APP1`, PNG past
//! its data, WebP to a block at its end; crafted lengths end the walk without reading more.

use std::fs;

use super::{MAX_EXIF_BYTES, Step, first, next, read_exif, tiff_of};
use crate::viewer::{
    FileWindow, ImageFormat,
    exif_fixtures::{camera_tiff, jpeg_with},
    test_support::local,
};

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend(kind);
    out.extend(data);
    out.extend([0, 0, 0, 0]);
    out
}

/// A PNG whose `eXIf` comes after its `IDAT`, as the format allows.
fn png_with(tiff: &[u8]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend(chunk(b"IHDR", &[0; 13]));
    out.extend(chunk(b"IDAT", &[7; 100]));
    out.extend(chunk(b"eXIf", tiff));
    out.extend(chunk(b"IEND", &[]));
    out
}

/// A WebP with an odd-sized chunk (padded) before an `EXIF` chunk that keeps `Exif\0\0`.
fn webp_with(tiff: &[u8]) -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    body.extend(b"VP8 ");
    body.extend(5u32.to_le_bytes());
    body.extend([1, 2, 3, 4, 5, 0]);
    let mut block = b"Exif\0\0".to_vec();
    block.extend(tiff);
    body.extend(b"EXIF");
    body.extend((block.len() as u32).to_le_bytes());
    body.extend(&block);
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

async fn read(bytes: &[u8], format: ImageFormat) -> Option<super::PhotoExif> {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("photo");
    fs::write(&path, bytes).expect("write");
    let file = FileWindow::open(local(), &path).await.expect("open");
    read_exif(&file, format).await.expect("read")
}

#[tokio::test]
async fn jpeg_png_and_webp_give_their_blocks() {
    let tiff = camera_tiff();
    for (bytes, format) in [
        (jpeg_with(&tiff), ImageFormat::Jpeg),
        (png_with(&tiff), ImageFormat::Png),
        (webp_with(&tiff), ImageFormat::WebP),
    ] {
        let exif = read(&bytes, format).await.expect("EXIF");
        assert_eq!(
            exif.summary.camera.as_deref(),
            Some("Canon EOS R5"),
            "{format:?}"
        );
    }
}

#[tokio::test]
async fn a_photo_without_exif_or_another_format_has_none() {
    let mut plain = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02];
    plain.extend([0; 32]);
    assert!(read(&plain, ImageFormat::Jpeg).await.is_none());
    assert!(read(b"GIF89a......", ImageFormat::Gif).await.is_none());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &[0; 13]));
    png.extend(chunk(b"IEND", &[]));
    assert!(read(&png, ImageFormat::Png).await.is_none());
}

#[test]
fn a_crafted_length_ends_the_walk() {
    // A block larger than the limit is not read.
    let mut big = u32::try_from(MAX_EXIF_BYTES + 1)
        .expect("small")
        .to_be_bytes()
        .to_vec();
    big.extend(b"eXIf");
    assert_eq!(next(ImageFormat::Png, 8, &big, u64::MAX), Step::None);
    // A block past the end of the file is not read.
    let mut short = 100u32.to_be_bytes().to_vec();
    short.extend(b"eXIf");
    assert_eq!(next(ImageFormat::Png, 8, &short, 50), Step::None);
    // A chunk claiming 4 GiB leads past the end.
    let mut huge = u32::MAX.to_be_bytes().to_vec();
    huge.extend(b"IDAT");
    assert_eq!(next(ImageFormat::Png, 8, &huge, 1 << 20), Step::None);
    // A JPEG segment shorter than its own length field, and no marker at all.
    assert_eq!(
        next(
            ImageFormat::Jpeg,
            2,
            &[0xFF, 0xE0, 0, 1, 0, 0, 0, 0, 0, 0],
            1 << 20
        ),
        Step::None
    );
    assert_eq!(next(ImageFormat::Jpeg, 2, &[0x12; 10], 1 << 20), Step::None);
    // An EXIF APP1 whose length leaves no block.
    let mut empty = vec![0xFF, 0xE1, 0x00, 0x08];
    empty.extend(b"Exif\0\0");
    assert_eq!(next(ImageFormat::Jpeg, 2, &empty, 1 << 20), Step::None);
}

#[test]
fn the_walk_starts_after_each_signature() {
    assert_eq!(
        first(ImageFormat::Jpeg),
        Step::Header { offset: 2, len: 10 }
    );
    assert_eq!(first(ImageFormat::Png), Step::Header { offset: 8, len: 8 });
    assert_eq!(
        first(ImageFormat::WebP),
        Step::Header { offset: 12, len: 8 }
    );
    assert_eq!(first(ImageFormat::Bmp), Step::None);
}

#[test]
fn jpeg_fill_bytes_and_restart_markers_are_stepped_over() {
    assert_eq!(
        next(
            ImageFormat::Jpeg,
            2,
            &[0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0],
            100
        ),
        Step::Header { offset: 3, len: 10 }
    );
    assert_eq!(
        next(
            ImageFormat::Jpeg,
            2,
            &[0xFF, 0xD3, 0, 0, 0, 0, 0, 0, 0, 0],
            100
        ),
        Step::Header { offset: 4, len: 10 }
    );
}

#[test]
fn a_leading_exif_marker_is_taken_off() {
    assert_eq!(tiff_of(b"Exif\0\0II*\0".to_vec()), b"II*\0");
    assert_eq!(tiff_of(b"MM\0*".to_vec()), b"MM\0*");
}
