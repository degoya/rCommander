//! What a PDF says about itself, for the tree: the document information dictionary (title,
//! author, subject, keywords, creator, producer, dates), the page count, the PDF version, whether
//! it is encrypted and the first page's size. Each value is cut at [`MAX_VALUE_CHARS`].

use hayro_interpret::hayro_syntax::object::DateTime;
use hayro_interpret::hayro_syntax::{Pdf, PdfVersion};

/// The longest value shown, in characters.
pub const MAX_VALUE_CHARS: usize = 512;
/// Points per millimetre.
const POINTS_PER_MM: f64 = 72.0 / 25.4;

/// The facts in the order the tree shows them, each a label and its value.
#[must_use]
pub fn facts(pdf: &Pdf, encrypted: bool) -> Vec<(&'static str, String)> {
    let meta = pdf.metadata();
    let mut facts = Vec::new();
    for (label, value) in [
        ("title", &meta.title),
        ("author", &meta.author),
        ("subject", &meta.subject),
        ("keywords", &meta.keywords),
        ("creator", &meta.creator),
        ("producer", &meta.producer),
    ] {
        if let Some(text) = value.as_deref().map(text_string).filter(|t| !t.is_empty()) {
            facts.push((label, text));
        }
    }
    for (label, date) in [
        ("created", &meta.creation_date),
        ("modified", &meta.modification_date),
    ] {
        if let Some(date) = date {
            facts.push((label, date_text(date)));
        }
    }
    let pages = pdf.pages();
    facts.push(("pages", pages.len().to_string()));
    facts.push(("version", version_text(pdf.version()).to_owned()));
    facts.push(("encrypted", if encrypted { "yes" } else { "no" }.to_owned()));
    if let Some(first) = pages.first() {
        let (width, height) = first.render_dimensions();
        facts.push(("page size", page_size(f64::from(width), f64::from(height))));
    }
    facts
}

/// A PDF text string: UTF-16BE or UTF-8 after their byte-order marks, else PDFDocEncoding read
/// as Latin-1 (they differ in a few punctuation marks only); control characters as spaces.
#[must_use]
pub fn text_string(bytes: &[u8]) -> String {
    let text = if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|&byte| char::from(byte)).collect()
    };
    let clean: String = text
        .trim()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_VALUE_CHARS)
        .collect();
    clean
}

/// `YYYY-MM-DD hh:mm:ss`.
#[must_use]
pub fn date_text(date: &DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        date.year, date.month, date.day, date.hour, date.minute, date.second
    )
}

fn version_text(version: PdfVersion) -> &'static str {
    match version {
        PdfVersion::Pdf10 => "1.0",
        PdfVersion::Pdf11 => "1.1",
        PdfVersion::Pdf12 => "1.2",
        PdfVersion::Pdf13 => "1.3",
        PdfVersion::Pdf14 => "1.4",
        PdfVersion::Pdf15 => "1.5",
        PdfVersion::Pdf16 => "1.6",
        PdfVersion::Pdf17 => "1.7",
        PdfVersion::Pdf20 => "2.0",
    }
}

/// `595 x 842 pt (210 x 297 mm)`.
#[must_use]
pub fn page_size(width: f64, height: f64) -> String {
    format!(
        "{:.0} x {:.0} pt ({:.0} x {:.0} mm)",
        width,
        height,
        width / POINTS_PER_MM,
        height / POINTS_PER_MM
    )
}

/// Whether the file names an encryption dictionary: `/Encrypt` in its trailer or its
/// cross-reference stream's dictionary, which are never compressed. hayro opens a document whose
/// user password is empty without telling; this tells the tree.
#[must_use]
pub fn names_encryption(bytes: &[u8]) -> bool {
    const KEY: &[u8] = b"/Encrypt";
    bytes.windows(KEY.len() + 1).any(|window| {
        window.starts_with(KEY)
            && matches!(
                window[KEY.len()],
                b' ' | b'\r' | b'\n' | b'\t' | b'<' | b'0'..=b'9'
            )
    })
}

#[cfg(test)]
#[path = "info_tests.rs"]
mod tests;
