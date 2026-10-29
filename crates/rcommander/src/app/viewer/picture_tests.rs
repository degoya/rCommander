use std::io::Cursor;

use ::image::{ImageFormat as Codec, Rgba, RgbaImage};
use rc_engine::viewer::ImageFormat;

use super::{
    ALL_CODES, DEFAULT_SHOWN_MEGAPIXELS, MAX_PIXELS, MAX_SIDE, check_size, decode, raster,
    shown_size,
};
use crate::i18n;

/// The default budget of shown pixels.
const SHOWN: u64 = DEFAULT_SHOWN_MEGAPIXELS * 1_000_000;

fn encoded(codec: Codec) -> Vec<u8> {
    let picture = RgbaImage::from_pixel(2, 3, Rgba([10, 20, 30, 255]));
    let mut bytes = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut bytes), codec)
        .expect("encodes");
    bytes
}

#[test]
fn a_png_and_a_bmp_decode_to_rgba_rows() {
    for (codec, format) in [
        (Codec::Png, ImageFormat::Png),
        (Codec::Bmp, ImageFormat::Bmp),
    ] {
        let pixels = decode(&encoded(codec), format, SHOWN).expect("decodes");
        assert_eq!((pixels.width, pixels.height), (2, 3));
        assert_eq!(pixels.rgba.len(), 2 * 3 * 4);
        assert_eq!(&pixels.rgba[..4], &[10, 20, 30, 255]);
    }
}

#[test]
fn a_damaged_file_is_a_message_not_a_crash() {
    let mut png = encoded(Codec::Png);
    png.truncate(png.len() / 2);
    assert_eq!(decode(&png, ImageFormat::Png, SHOWN), Err("image_broken"));
    let garbage = b"\x89PNG\r\n\x1a\nnot a png at all".to_vec();
    assert_eq!(
        decode(&garbage, ImageFormat::Png, SHOWN),
        Err("image_broken")
    );
    assert_eq!(
        decode(b"GIF89a", ImageFormat::Gif, SHOWN),
        Err("image_broken")
    );
    assert_eq!(decode(&[], ImageFormat::Jpeg, SHOWN), Err("image_broken"));
}

#[test]
fn a_picture_claiming_too_many_pixels_is_refused_before_decoding() {
    assert_eq!(check_size(MAX_SIDE + 1, 1), Err("image_too_large"));
    assert_eq!(check_size(1, MAX_SIDE + 1), Err("image_too_large"));
    let side = u32::try_from(MAX_PIXELS.isqrt()).expect("fits");
    assert_eq!(check_size(side, side), Ok(()));
    assert_eq!(check_size(side + 1, side + 1), Err("image_too_large"));
}

#[test]
fn every_picture_error_has_a_message() {
    for code in ALL_CODES {
        assert_ne!(i18n::error_text(code), *code, "error.{code} is missing");
    }
}

/// The owner's scan of 5360 x 32667 (2026-10-19) was refused; it is allowed now and shown
/// scaled down to the budget, its proportions kept.
#[test]
fn a_very_large_picture_is_allowed_and_shown_scaled_down() {
    assert_eq!(check_size(5360, 32667), Ok(()));
    let (width, height) = shown_size(5360, 32667, SHOWN);
    assert!(u64::from(width) * u64::from(height) <= SHOWN);
    assert_eq!((width, height), (2291, 13965));
    assert_eq!(
        shown_size(4000, 3000, SHOWN),
        (4000, 3000),
        "within the budget"
    );
    assert_eq!(
        shown_size(32_000, 10, 1000),
        (1788, 1),
        "never below one pixel"
    );
}

#[test]
fn a_picture_above_the_budget_is_decoded_at_its_shown_size() {
    let picture = ::image::RgbImage::from_pixel(2000, 1500, ::image::Rgb([200, 100, 50]));
    let mut bytes = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut bytes), Codec::Jpeg)
        .expect("encodes");
    let pixels = decode(&bytes, ImageFormat::Jpeg, 300_000).expect("decodes");
    assert_eq!((pixels.width, pixels.height), (2000, 1500));
    assert_eq!((pixels.shown_width, pixels.shown_height), (632, 474));
    assert_eq!(pixels.rgba.len(), 632 * 474 * 4);
}

/// A plugin's raw raster (RC-100-29): exactly its size, held to the limits, scaled down as a
/// file's picture is.
#[test]
fn a_raw_raster_is_read_by_its_size_and_scaled_as_any_picture() {
    let pixels = raster([1, 2, 3, 255].repeat(6), 2, 3, SHOWN).expect("reads");
    assert_eq!((pixels.width, pixels.height), (2, 3));
    assert_eq!((pixels.shown_width, pixels.shown_height), (2, 3));
    assert_eq!(&pixels.rgba[..4], &[1, 2, 3, 255]);

    assert_eq!(raster(vec![0; 23], 2, 3, SHOWN), Err("image_broken"));
    assert_eq!(raster(vec![0; 25], 2, 3, SHOWN), Err("image_broken"));
    assert_eq!(
        raster(Vec::new(), MAX_SIDE + 1, 1, SHOWN),
        Err("image_too_large")
    );

    let scaled = raster(vec![255; 400 * 100 * 4], 400, 100, 10_000).expect("reads");
    assert_eq!((scaled.width, scaled.height), (400, 100), "its own size");
    assert_eq!((scaled.shown_width, scaled.shown_height), (200, 50));
    assert_eq!(scaled.rgba.len(), 200 * 50 * 4);
}
