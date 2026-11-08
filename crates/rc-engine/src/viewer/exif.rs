//! The EXIF of a photo (RC-100-99): the image view's EXIF tree beside the picture and the fields
//! `file.properties` names. JPEG, PNG and WebP never reach a viewer plugin (`plugins::set`,
//! `claims`), so their EXIF is read here, with `kamadak-exif`, which `iced_graphics` already
//! brings into the process (owner, 2026-11-07).
//!
//! Only the EXIF block is read, never the picture: `locate.rs` walks the file's
//! segments or chunks header by header and reads the one block, at most [`MAX_EXIF_BYTES`]
//! ([`read_exif`]).
//! A field's value is cut at [`MAX_VALUE_CHARS`]; a block holds at most [`MAX_FIELDS`]. A
//! damaged block is no EXIF, never an error the person sees.

mod locate;

use std::{path::Path, sync::Arc};

use exif::{Context, Field, In, Reader, Tag, Value};
use rc_vfs::Vfs;
use serde::{Deserialize, Serialize};

pub use locate::{MAX_EXIF_BYTES, read_exif};

use super::{FileWindow, ImageFormat, sniff::sniff_image};

/// The most fields one block yields; the rest are left out. A camera writes about a hundred.
pub const MAX_FIELDS: usize = 256;
/// The longest value shown, in characters; a longer one ends in an ellipsis.
pub const MAX_VALUE_CHARS: usize = 256;
/// An `undefined` value longer than this is shown by its length only (a maker note, a
/// thumbnail).
const MAX_RAW_BYTES: usize = 64;
/// The extensions `file.properties` looks into for EXIF; any other file is not opened.
pub const PHOTO_EXTENSIONS: &[&str] = &["jpg", "jpeg", "jpe", "jfif", "png", "webp"];

/// The part of the EXIF block a field belongs to, the tree's top level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExifGroup {
    /// The primary image's TIFF fields: camera, orientation, software.
    Image,
    /// The Exif IFD: exposure, lens, dates.
    Photo,
    /// The GPS IFD.
    Gps,
    /// The interoperability IFD.
    Interop,
    /// The thumbnail's fields (IFD1).
    Thumbnail,
}

/// One field: its EXIF tag name (`Make`, `ExposureTime`) and its value as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExifField {
    pub group: ExifGroup,
    pub tag: String,
    pub value: String,
}

/// A photo's EXIF block, read: its fields in their groups' order, and the summary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PhotoExif {
    pub fields: Vec<ExifField>,
    pub summary: ExifSummary,
}

/// The fields `file.properties` names: camera, capture time, exposure, size and place.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExifSummary {
    /// Make and model, the make left out when the model already names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<String>,
    /// `DateTimeOriginal` as `YYYY-MM-DD hh:mm:ss`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken: Option<String>,
    /// Exposure time, f-number, ISO and focal length, as the camera states them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Latitude and longitude in decimal degrees, `48.858370, 2.294481`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gps: Option<String>,
}

impl ExifSummary {
    /// Whether nothing is known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The EXIF block `tiff` (a TIFF header and its IFDs) read; `None` when it is no readable EXIF.
#[must_use]
pub fn parse(tiff: Vec<u8>) -> Option<PhotoExif> {
    let mut reader = Reader::new();
    reader.continue_on_error(true);
    let exif = match reader.read_raw(tiff) {
        Ok(exif) => exif,
        // What could be read of a damaged block is still shown.
        Err(exif::Error::PartialResult(partial)) => partial.into_inner().0,
        Err(_) => return None,
    };
    let mut fields: Vec<ExifField> = exif
        .fields()
        .take(MAX_FIELDS)
        .map(|field| ExifField {
            group: group_of(field),
            tag: field.tag.to_string(),
            value: shown_value(field, &exif),
        })
        .collect();
    // Stable: within a group the fields keep the block's order.
    fields.sort_by_key(|field| field.group);
    if fields.is_empty() {
        return None;
    }
    Some(PhotoExif {
        fields,
        summary: summary(&exif),
    })
}

fn group_of(field: &Field) -> ExifGroup {
    if field.ifd_num == In::THUMBNAIL {
        return ExifGroup::Thumbnail;
    }
    match field.tag.context() {
        Context::Exif => ExifGroup::Photo,
        Context::Gps => ExifGroup::Gps,
        Context::Interop => ExifGroup::Interop,
        _ => ExifGroup::Image,
    }
}

/// A field's value as text, with its unit; a long raw value by its length.
fn shown_value(field: &Field, exif: &exif::Exif) -> String {
    if let Some(text) = ascii(&field.value) {
        return cut(text);
    }
    if let Value::Undefined(bytes, _) = &field.value
        && bytes.len() > MAX_RAW_BYTES
    {
        return format!("[{} bytes]", bytes.len());
    }
    cut(field.display_value().with_unit(exif).to_string())
}

/// The first string of an ASCII value, without kamadak's quotes; `None` for any other type.
fn ascii(value: &Value) -> Option<String> {
    let Value::Ascii(strings) = value else {
        return None;
    };
    let first = strings.first()?;
    Some(String::from_utf8_lossy(first).trim().to_owned())
}

/// `text` cut at [`MAX_VALUE_CHARS`], with an ellipsis; control characters as spaces.
fn cut(text: String) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if clean.chars().count() <= MAX_VALUE_CHARS {
        return clean;
    }
    let mut short: String = clean.chars().take(MAX_VALUE_CHARS - 1).collect();
    short.push('\u{2026}');
    short
}

fn summary(exif: &exif::Exif) -> ExifSummary {
    let text = |tag: Tag| {
        exif.get_field(tag, In::PRIMARY)
            .and_then(|field| ascii(&field.value))
            .filter(|text| !text.is_empty())
    };
    let shown = |tag: Tag| {
        exif.get_field(tag, In::PRIMARY)
            .map(|field| field.display_value().with_unit(exif).to_string())
    };
    let number = |tag: Tag| {
        exif.get_field(tag, In::PRIMARY)
            .and_then(|field| field.value.get_uint(0))
    };
    let camera = match (text(Tag::Make), text(Tag::Model)) {
        (Some(make), Some(model)) if model.to_lowercase().starts_with(&make.to_lowercase()) => {
            Some(model)
        }
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (make, model) => make.or(model),
    };
    let taken = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .map(|field| field.display_value().to_string());
    let exposure: Vec<String> = [
        shown(Tag::ExposureTime),
        shown(Tag::FNumber),
        number(Tag::PhotographicSensitivity).map(|iso| format!("ISO {iso}")),
        shown(Tag::FocalLength),
    ]
    .into_iter()
    .flatten()
    .collect();
    ExifSummary {
        camera: camera.map(cut),
        lens: text(Tag::LensModel).map(cut),
        taken: taken.map(cut),
        exposure: (!exposure.is_empty()).then(|| cut(exposure.join(", "))),
        width: number(Tag::PixelXDimension).or_else(|| number(Tag::ImageWidth)),
        height: number(Tag::PixelYDimension).or_else(|| number(Tag::ImageLength)),
        gps: gps(exif),
    }
}

/// The position in decimal degrees, south and west negative; `None` without both halves.
fn gps(exif: &exif::Exif) -> Option<String> {
    let latitude = degrees(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, b'S')?;
    let longitude = degrees(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, b'W')?;
    Some(format!("{latitude:.6}, {longitude:.6}"))
}

fn degrees(exif: &exif::Exif, tag: Tag, reference: Tag, negative: u8) -> Option<f64> {
    let Value::Rational(parts) = &exif.get_field(tag, In::PRIMARY)?.value else {
        return None;
    };
    let mut value = 0.0;
    for (part, scale) in parts.iter().zip([1.0, 60.0, 3600.0]) {
        if part.denom == 0 {
            return None;
        }
        value += part.to_f64() / scale;
    }
    if !value.is_finite() {
        return None;
    }
    let negated = exif
        .get_field(reference, In::PRIMARY)
        .and_then(|field| ascii(&field.value))
        .is_some_and(|text| text.as_bytes().first() == Some(&negative));
    Some(if negated { -value } else { value })
}

/// Whether `file.properties` looks into `path` for EXIF: a photo by its extension.
#[must_use]
pub fn is_photo_name(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PHOTO_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// The EXIF summary of the file at `path` on `vfs`, for `file.properties`: `None` when it is no
/// JPEG, PNG or WebP by its first bytes, has no EXIF, or cannot be read.
pub async fn photo_summary(vfs: Arc<dyn Vfs>, path: &Path) -> Option<ExifSummary> {
    let file = FileWindow::open(vfs, path).await.ok()?;
    let head = file.read_uncached(0, 16).await.ok()?;
    let format = sniff_image(&head)?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return None;
    }
    let exif = read_exif(&file, format).await.ok()??;
    Some(exif.summary).filter(|summary| !summary.is_empty())
}

#[cfg(test)]
#[path = "exif_tests.rs"]
mod tests;
