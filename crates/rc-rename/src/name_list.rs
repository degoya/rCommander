//! Name lists (Total Commander's "Load names from file" and "Edit names"): a text with one new
//! name per line, read from a file or the clipboard, and written for an editor.

use rc_plugin_text::{Decoder, Encoding, detect};

use crate::error::RenameError;
use crate::rule::{MAX_NAMES, MAX_NAMES_BYTES};

/// The byte-order mark an editor file starts with, so Notepad reads it as UTF-8.
const BOM_UTF8: &[u8] = b"\xEF\xBB\xBF";

/// The names of a file's bytes: its encoding from a byte-order mark (UTF-8, UTF-16), else UTF-8
/// when the bytes are valid UTF-8, else Windows-1252 (an ANSI file); then
/// [`names_from_text`].
///
/// # Errors
///
/// [`RenameError::NameList`] for a file of more than [`MAX_NAMES_BYTES`] bytes or more than
/// [`MAX_NAMES`] lines.
pub fn names_from_bytes(bytes: &[u8]) -> Result<Vec<String>, RenameError> {
    if bytes.len() > MAX_NAMES_BYTES {
        return Err(too_large());
    }
    let (encoding, bom) = detect(bytes, Encoding::Windows1252);
    let mut text = String::with_capacity(bytes.len());
    let mut decoder = Decoder::new(encoding);
    decoder.push(&bytes[bom..], &mut text);
    decoder.finish(&mut text);
    names_from_text(&text)
}

/// One name per line, LF or CRLF; a line break at the end ends the last name and adds none.
/// Each line is a name as it is, spaces included: the plan checks it as any new name.
///
/// # Errors
///
/// [`RenameError::NameList`] for more than [`MAX_NAMES`] lines or [`MAX_NAMES_BYTES`] bytes.
pub fn names_from_text(text: &str) -> Result<Vec<String>, RenameError> {
    if text.len() > MAX_NAMES_BYTES {
        return Err(too_large());
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    let body = body.strip_suffix('\r').unwrap_or(body);
    if body.is_empty() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for line in body.split('\n') {
        if names.len() == MAX_NAMES {
            return Err(too_large());
        }
        names.push(line.strip_suffix('\r').unwrap_or(line).to_owned());
    }
    Ok(names)
}

/// The text an editor gets: UTF-8 with a byte-order mark, one name per line, CRLF with `crlf`
/// (Windows) else LF, a line break after the last.
#[must_use]
pub fn names_to_bytes(names: &[String], crlf: bool) -> Vec<u8> {
    let newline: &[u8] = if crlf { b"\r\n" } else { b"\n" };
    let mut out = BOM_UTF8.to_vec();
    for name in names {
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(newline);
    }
    out
}

fn too_large() -> RenameError {
    RenameError::NameList {
        max_names: MAX_NAMES,
        max_bytes: MAX_NAMES_BYTES,
    }
}

#[cfg(test)]
#[path = "name_list_tests.rs"]
mod tests;
