use super::*;
use crate::fixtures::tiff;
use crate::test_support::{fact, named};
use crate::types::FailureKind;

#[test]
fn a_tiff_names_its_camera_time_and_position_in_either_byte_order() {
    for little in [true, false] {
        let media = read_tiff(&mut named(&tiff(little), "photo.dng")).expect("the EXIF reads");
        let sections = &media.sections;
        assert_eq!(fact(sections, "Camera", "Make"), Some("Canon"));
        assert_eq!(fact(sections, "Camera", "Model"), Some("EOS R5"));
        let taken = fact(sections, "Camera", "Taken").expect("the time taken");
        assert!(taken.contains("2024"), "{taken}");
        assert!(taken.contains("12:34:56"), "{taken}");
        let f_number = fact(sections, "Camera", "F-number").expect("the f-number");
        assert!(f_number.contains("2.8"), "{f_number}");
        assert_eq!(fact(sections, "GPS", "Latitude"), Some("48.140000"));
        assert_eq!(fact(sections, "GPS", "Longitude"), Some("11.571667"));
        let primary = sections
            .iter()
            .find(|node| node.label == "Fields (primary image)")
            .expect("every field is listed");
        assert!(primary.find("Make").is_some());
        assert!(primary.find("GPSLatitude").is_some());
    }
}

#[test]
fn south_and_west_are_negative() {
    let bytes = tiff(true);
    // The references are inline ASCII: swap N for S and E for W.
    let mut swapped = bytes.clone();
    for byte in &mut swapped {
        *byte = match *byte {
            b'N' => b'S',
            b'E' => b'W',
            other => other,
        };
    }
    // "EOS R5" became "WOS R5"; only the position matters here.
    let media = read_tiff(&mut named(&swapped, "photo.tif")).expect("the EXIF reads");
    assert_eq!(fact(&media.sections, "GPS", "Latitude"), Some("-48.140000"));
    assert_eq!(
        fact(&media.sections, "GPS", "Longitude"),
        Some("-11.571667")
    );
}

#[test]
fn a_tiff_without_ifds_in_reach_has_no_exif() {
    // The first IFD lies far beyond the end of the file.
    let mut bytes = b"II*\0".to_vec();
    bytes.extend_from_slice(&0x7FFF_0000u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 64]);
    let failure = read_tiff(&mut named(&bytes, "photo.tif")).expect_err("no EXIF");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(codes::NO_EXIF));
}

#[test]
fn a_file_that_is_no_tiff_is_bad_exif() {
    let failure = read_tiff(&mut named(b"not a tiff at all", "photo.tif")).expect_err("no TIFF");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(codes::BAD_EXIF));
}

#[test]
fn a_heif_without_exif_is_named() {
    let bytes = crate::fixtures_video::ftyp(b"heic");
    let failure = read_heif(&mut named(&bytes, "photo.heic")).expect_err("no EXIF");
    assert_eq!(failure.kind, FailureKind::BadData);
}
