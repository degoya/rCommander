//! A photo's EXIF read from hand-built TIFF blocks: the fields in their groups, the summary
//! `file.properties` names, the position in decimal degrees, and damaged or oversized values.

use std::path::Path;

use super::{ExifGroup, MAX_VALUE_CHARS, is_photo_name, parse};
use crate::viewer::exif_fixtures::{ascii, camera_tiff, rational, tiff};

#[test]
fn a_cameras_block_gives_its_fields_in_groups_and_the_summary() {
    let exif = parse(camera_tiff()).expect("EXIF");
    let groups: Vec<ExifGroup> = exif.fields.iter().map(|field| field.group).collect();
    let mut sorted = groups.clone();
    sorted.sort();
    assert_eq!(groups, sorted, "grouped: image, photo, GPS");
    let make = exif
        .fields
        .iter()
        .find(|field| field.tag == "Make")
        .expect("Make");
    assert_eq!(
        (make.group, make.value.as_str()),
        (ExifGroup::Image, "Canon")
    );
    assert!(
        exif.fields
            .iter()
            .any(|field| field.group == ExifGroup::Gps && field.tag == "GPSLatitude")
    );
    let summary = exif.summary;
    assert_eq!(summary.camera.as_deref(), Some("Canon EOS R5"));
    assert_eq!(summary.taken.as_deref(), Some("2024-05-01 12:34:56"));
    let exposure = summary.exposure.expect("exposure");
    for part in ["1/125 s", "f/2.8", "ISO 100", "50 mm"] {
        assert!(exposure.contains(part), "{part} in {exposure}");
    }
    assert_eq!(summary.gps.as_deref(), Some("48.856667, 2.294444"));
}

#[test]
fn a_model_that_names_its_make_is_the_camera_alone() {
    let block = tiff(
        vec![ascii(0x010F, "NIKON"), ascii(0x0110, "NIKON Z 6")],
        Vec::new(),
        Vec::new(),
    );
    let summary = parse(block).expect("EXIF").summary;
    assert_eq!(summary.camera.as_deref(), Some("NIKON Z 6"));
    assert_eq!(summary.gps, None);
    assert_eq!(summary.exposure, None);
}

#[test]
fn south_and_west_are_negative() {
    let block = tiff(
        vec![ascii(0x010F, "Apple")],
        Vec::new(),
        vec![
            ascii(0x0001, "S"),
            rational(0x0002, &[(33, 1), (52, 1), (0, 1)]),
            ascii(0x0003, "W"),
            rational(0x0004, &[(70, 1), (30, 1), (0, 1)]),
        ],
    );
    let summary = parse(block).expect("EXIF").summary;
    assert_eq!(summary.gps.as_deref(), Some("-33.866667, -70.500000"));
}

#[test]
fn a_zero_denominator_gives_no_position() {
    let block = tiff(
        vec![ascii(0x010F, "Apple")],
        Vec::new(),
        vec![
            ascii(0x0001, "N"),
            rational(0x0002, &[(33, 0), (52, 1), (0, 1)]),
            ascii(0x0003, "E"),
            rational(0x0004, &[(70, 1), (30, 1), (0, 1)]),
        ],
    );
    assert_eq!(parse(block).expect("EXIF").summary.gps, None);
}

#[test]
fn a_long_value_is_cut_with_an_ellipsis() {
    let long_text = "x".repeat(600);
    let block = tiff(vec![ascii(0x010E, &long_text)], Vec::new(), Vec::new());
    let exif = parse(block).expect("EXIF");
    let value = &exif.fields[0].value;
    assert_eq!(value.chars().count(), MAX_VALUE_CHARS);
    assert!(value.ends_with('\u{2026}'));
}

#[test]
fn damaged_blocks_are_no_exif_and_never_panic() {
    assert_eq!(parse(b"not a tiff".to_vec()), None);
    assert_eq!(parse(Vec::new()), None);
    let block = camera_tiff();
    for end in [0, 7, 8, 12, 30, block.len() / 2, block.len() - 1] {
        // Whatever a truncated block yields, it yields without a panic.
        let _ = parse(block[..end].to_vec());
    }
}

#[test]
fn only_photos_by_their_extension_are_looked_into() {
    for name in ["a.jpg", "b.JPEG", "c.png", "d.webp", "e.jfif"] {
        assert!(is_photo_name(Path::new(name)), "{name}");
    }
    for name in ["a.txt", "b.gif", "c", "d.tiff"] {
        assert!(!is_photo_name(Path::new(name)), "{name}");
    }
}
