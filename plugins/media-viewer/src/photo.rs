//! The EXIF of TIFF, camera raw (DNG, CR2, NEF, ARW) and HEIF/HEIC photos, through kamadak-exif:
//! camera, lens, time, exposure, picture size, orientation and GPS position first, then every
//! field by its IFD.
//!
//! kamadak's container reader would read a whole TIFF into memory - a raw file is 20 to 100 MiB.
//! Instead the first [`TIFF_PREFIX`] bytes are read and parsed raw, with errors collected rather
//! than fatal: the IFDs and their values sit at the head of camera files (the image data follows
//! them), so 8 MiB holds them with room to spare, a maker note included, and stays far below the
//! store's 64 MiB beside kamadak's parsed fields. A field whose value lies beyond the prefix is
//! left out; a TIFF whose first IFD lies beyond it has no EXIF within reach. HEIF goes through
//! kamadak's own box reader, which seeks past the image data and reads only the `meta` box and
//! the EXIF item (at most 128 KiB, its own limit) - over a reader that fetches at most
//! [`HEIF_READ_BUDGET`] bytes, so a `meta` claiming GiB ends there.

use exif::{Exif, Field, In, Reader, Tag, Value};

use crate::io::SourceReader;
use crate::outline::Node;
use crate::types::{Failure, Source};
use crate::{Media, codes};

/// The head of a TIFF-based file the EXIF is read from.
pub const TIFF_PREFIX: u64 = 8 << 20;
/// The most bytes kamadak may fetch from a HEIF file.
pub const HEIF_READ_BUDGET: u64 = 16 << 20;
/// The window kamadak reads a HEIF file through.
const WINDOW: u32 = 64 << 10;
/// The most fields listed.
const MAX_FIELDS: usize = 2000;
/// A binary value longer than this is listed by its size.
const MAX_BINARY: usize = 64;

/// Reads the EXIF of the TIFF-based file `src` from its first [`TIFF_PREFIX`] bytes.
pub fn read_tiff<S: Source>(src: &mut S) -> Result<Media, Failure> {
    let length = src.size().min(TIFF_PREFIX);
    let mut reader = SourceReader::new(src, WINDOW, TIFF_PREFIX);
    let prefix = reader.exact(0, length, codes::BAD_EXIF)?;
    let mut parser = Reader::new();
    parser.continue_on_error(true);
    describe(parser.read_raw(prefix))
}

/// Reads the EXIF item of the HEIF file `src`.
pub fn read_heif<S: Source>(src: &mut S) -> Result<Media, Failure> {
    let mut reader = SourceReader::new(src, WINDOW, HEIF_READ_BUDGET);
    let mut parser = Reader::new();
    parser.continue_on_error(true);
    match parser.read_from_container(&mut reader) {
        Ok(exif) => describe(Ok(exif)),
        // A cancel or the budget, if that is what ended it; else what kamadak found.
        Err(error) => match reader.take_failure() {
            Some(failure) => Err(failure),
            None => describe(Err(error)),
        },
    }
}

/// The facts of kamadak's answer; a partial result keeps the fields it has.
fn describe(result: Result<Exif, exif::Error>) -> Result<Media, Failure> {
    let exif = match result {
        Ok(exif) => exif,
        Err(exif::Error::PartialResult(partial)) => partial.into_inner().0,
        Err(exif::Error::NotFound(_)) => return Err(Failure::bad(codes::NO_EXIF)),
        Err(_) => return Err(Failure::bad(codes::BAD_EXIF)),
    };
    if exif.fields().next().is_none() {
        return Err(Failure::bad(codes::NO_EXIF));
    }
    let mut sections = Vec::new();
    let camera = camera(&exif);
    if !camera.is_empty() {
        sections.push(Node::group("Camera", None, camera));
    }
    let gps = gps(&exif);
    if !gps.is_empty() {
        sections.push(Node::group("GPS", None, gps));
    }
    sections.extend(all_fields(&exif));
    Ok(Media {
        title: None,
        sections,
        artwork: None,
    })
}

/// The field `tag` of the primary image.
fn field(exif: &Exif, tag: Tag) -> Option<&Field> {
    exif.get_field(tag, In::PRIMARY)
}

/// A field's value as a person reads it, with its unit; binary data by its size. Text kamadak
/// would quote and escape (a name, a model) is decoded as UTF-8 instead; text it reads (a time)
/// keeps its reading.
fn shown(exif: &Exif, field: &Field) -> String {
    match &field.value {
        Value::Ascii(strings) => {
            let display = field.display_value().with_unit(exif).to_string();
            if !display.starts_with('"') {
                return display;
            }
            strings
                .iter()
                .map(|bytes| {
                    String::from_utf8_lossy(bytes)
                        .trim_end_matches('\0')
                        .to_owned()
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
        Value::Undefined(bytes, _) | Value::Byte(bytes) if bytes.len() > MAX_BINARY => {
            format!("{} bytes", bytes.len())
        }
        _ => field.display_value().with_unit(exif).to_string(),
    }
}

/// The value of `tag` of the primary image, as shown.
fn text(exif: &Exif, tag: Tag) -> Option<String> {
    field(exif, tag)
        .map(|found| shown(exif, found))
        .filter(|value| !value.trim().is_empty())
}

/// An unsigned number of `tag` of the primary image.
fn number(exif: &Exif, tag: Tag) -> Option<u32> {
    field(exif, tag).and_then(|found| found.value.get_uint(0))
}

fn camera(exif: &Exif) -> Vec<Node> {
    let mut facts = Vec::new();
    let mut add = |label: &str, value: Option<String>| {
        if let Some(value) = value {
            facts.push(Node::fact(label, value));
        }
    };
    add("Make", text(exif, Tag::Make));
    add("Model", text(exif, Tag::Model));
    add(
        "Lens",
        text(exif, Tag::LensModel).or_else(|| text(exif, Tag::LensMake)),
    );
    add(
        "Taken",
        text(exif, Tag::DateTimeOriginal).or_else(|| text(exif, Tag::DateTime)),
    );
    add("Exposure", text(exif, Tag::ExposureTime));
    add("F-number", text(exif, Tag::FNumber));
    add("ISO", text(exif, Tag::PhotographicSensitivity));
    add("Focal length", text(exif, Tag::FocalLength));
    let size = match (
        number(exif, Tag::PixelXDimension),
        number(exif, Tag::PixelYDimension),
    ) {
        (Some(width), Some(height)) => Some((width, height)),
        _ => number(exif, Tag::ImageWidth).zip(number(exif, Tag::ImageLength)),
    };
    add(
        "Dimensions",
        size.map(|(width, height)| format!("{width} x {height}")),
    );
    add("Orientation", text(exif, Tag::Orientation));
    facts
}

/// The position as decimal degrees, south and west negative, and the altitude.
fn gps(exif: &Exif) -> Vec<Node> {
    let mut facts = Vec::new();
    if let Some(latitude) = degrees(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, b'S') {
        facts.push(Node::fact("Latitude", format!("{latitude:.6}")));
    }
    if let Some(longitude) = degrees(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, b'W') {
        facts.push(Node::fact("Longitude", format!("{longitude:.6}")));
    }
    if let Some(altitude) = text(exif, Tag::GPSAltitude) {
        facts.push(Node::fact("Altitude", altitude));
    }
    facts
}

/// Degrees, minutes and seconds as decimal degrees; negative when the reference is `negative`.
fn degrees(exif: &Exif, tag: Tag, reference: Tag, negative: u8) -> Option<f64> {
    let Value::Rational(parts) = &field(exif, tag)?.value else {
        return None;
    };
    let mut value = 0.0;
    for (part, divisor) in parts.iter().zip([1.0, 60.0, 3600.0]) {
        value += part.to_f64() / divisor;
    }
    if !value.is_finite() {
        return None;
    }
    let sign = match field(exif, reference).map(|found| &found.value) {
        Some(Value::Ascii(strings))
            if strings.first().and_then(|s| s.first()) == Some(&negative) =>
        {
            -1.0
        }
        _ => 1.0,
    };
    Some(sign * value)
}

/// Every field, a section per IFD.
fn all_fields(exif: &Exif) -> Vec<Node> {
    let mut sections: Vec<(In, Vec<Node>)> = Vec::new();
    for found in exif.fields().take(MAX_FIELDS) {
        let fact = Node::fact(found.tag.to_string(), shown(exif, found));
        match sections.iter_mut().find(|(ifd, _)| *ifd == found.ifd_num) {
            Some((_, facts)) => facts.push(fact),
            None => sections.push((found.ifd_num, vec![fact])),
        }
    }
    sections
        .into_iter()
        .map(|(ifd, facts)| {
            let label = match ifd {
                In::PRIMARY => "Fields (primary image)".to_owned(),
                In::THUMBNAIL => "Fields (thumbnail)".to_owned(),
                In(other) => format!("Fields (image {other})"),
            };
            Node::group(label, None, facts)
        })
        .collect()
}

#[cfg(test)]
#[path = "photo_tests.rs"]
mod tests;
