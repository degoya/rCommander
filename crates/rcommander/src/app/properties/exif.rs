//! A photo's EXIF in the properties dialog (RC-100-99): the fields `file.properties` names -
//! camera, lens, capture time, exposure, size in pixels and position - each with its label, in
//! that order; a field the photo does not state is left out.

use rc_engine::viewer::exif::ExifSummary;

use crate::i18n::t;

/// The rows for `exif`: a label and its value.
pub(super) fn facts(exif: &ExifSummary) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();
    for (label, value) in [
        (t!("props.exif_camera"), &exif.camera),
        (t!("props.exif_lens"), &exif.lens),
        (t!("props.exif_taken"), &exif.taken),
        (t!("props.exif_exposure"), &exif.exposure),
    ] {
        if let Some(value) = value {
            rows.push((label, value.clone()));
        }
    }
    if let (Some(width), Some(height)) = (exif.width, exif.height) {
        rows.push((
            t!("props.exif_pixels"),
            t!("props.exif_pixels_value", width = width, height = height),
        ));
    }
    if let Some(gps) = &exif.gps {
        rows.push((t!("props.exif_gps"), gps.clone()));
    }
    rows
}

#[cfg(test)]
#[path = "exif_tests.rs"]
mod tests;
