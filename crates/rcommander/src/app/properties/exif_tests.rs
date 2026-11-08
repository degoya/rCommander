//! The EXIF rows of the properties dialog.

use rc_engine::viewer::exif::ExifSummary;

use super::facts;
use crate::i18n::t;

#[test]
fn the_stated_fields_become_rows_in_order() {
    let exif = ExifSummary {
        camera: Some("Canon EOS R5".to_owned()),
        taken: Some("2024-05-01 12:34:56".to_owned()),
        width: Some(8192),
        height: Some(5464),
        gps: Some("48.856667, 2.294444".to_owned()),
        ..ExifSummary::default()
    };
    let rows = facts(&exif);
    let labels: Vec<&str> = rows.iter().map(|(label, _)| *label).collect();
    assert_eq!(
        labels,
        [
            t!("props.exif_camera"),
            t!("props.exif_taken"),
            t!("props.exif_pixels"),
            t!("props.exif_gps")
        ]
    );
    assert_eq!(rows[0].1, "Canon EOS R5");
    assert!(rows[2].1.contains("8192") && rows[2].1.contains("5464"));
}

#[test]
fn a_photo_without_a_size_pair_shows_no_size() {
    let exif = ExifSummary {
        width: Some(10),
        ..ExifSummary::default()
    };
    assert!(facts(&exif).is_empty());
}
