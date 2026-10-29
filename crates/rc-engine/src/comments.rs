//! File comments as Total Commander keeps them (RC-020-06): one `descript.ion` per folder, a
//! line per file - `name comment`, a name with spaces in double quotes. A comment of several
//! lines is one line with `\n` for each break and the marker U+0004 U+00C2 at its end.
//!
//! The file is read as the viewer reads it (`viewer::decode_text`: a byte-order mark, UTF-16,
//! valid UTF-8, else Windows-1252, Total Commander's ANSI); it is written back as ASCII when it
//! is, else UTF-8 with a byte-order mark, and with CRLF. Lines that are not understood stay as
//! they were.

use std::fmt::Write as _;

/// The comment file's name in each folder.
pub const DESCRIPTION_FILE: &str = "descript.ion";

/// A comment's length at most, in characters.
pub const MAX_COMMENT: usize = 4096;

/// A `descript.ion` at most, in bytes: a larger one is not read.
pub const MAX_FILE: u64 = 1024 * 1024;

/// What marks a comment of several lines.
const MULTILINE_MARKER: &str = "\u{4}\u{C2}";

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The comments of one folder, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Descriptions {
    lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    /// A file's comment, as stored (escapes and marker included).
    Entry { name: String, stored: String },
    /// Anything else, kept.
    Other(String),
}

impl Descriptions {
    /// The comments of a `descript.ion`'s text.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| match split(line) {
                Some((name, stored)) => Line::Entry {
                    name: name.to_owned(),
                    stored: stored.to_owned(),
                },
                None => Line::Other(line.to_owned()),
            })
            .collect();
        Self { lines }
    }

    /// The comment of `name` (compared ignoring case, as Windows names are), line breaks
    /// restored.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<String> {
        self.lines.iter().find_map(|line| match line {
            Line::Entry {
                name: stored_name,
                stored,
            } if same_name(stored_name, name) => Some(unescape(stored)),
            _ => None,
        })
    }

    /// Sets or, with `None` or an empty text, removes the comment of `name`.
    pub fn set(&mut self, name: &str, comment: Option<&str>) {
        let comment = comment.map(str::trim).filter(|comment| !comment.is_empty());
        let at = self.lines.iter().position(
            |line| matches!(line, Line::Entry { name: stored, .. } if same_name(stored, name)),
        );
        match (at, comment) {
            (Some(at), None) => {
                self.lines.remove(at);
            }
            (Some(at), Some(comment)) => {
                self.lines[at] = Line::Entry {
                    name: name.to_owned(),
                    stored: escape(comment),
                };
            }
            (None, Some(comment)) => self.lines.push(Line::Entry {
                name: name.to_owned(),
                stored: escape(comment),
            }),
            (None, None) => {}
        }
    }

    /// Whether no line is left.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The text of the file, CRLF after every line.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::new();
        for line in &self.lines {
            match line {
                Line::Entry { name, stored } if name.contains(' ') => {
                    let _ = write!(text, "\"{name}\" {stored}");
                }
                Line::Entry { name, stored } => {
                    let _ = write!(text, "{name} {stored}");
                }
                Line::Other(other) => text.push_str(other),
            }
            text.push_str("\r\n");
        }
        text
    }
}

/// The text of a `descript.ion`'s bytes, decoded as the viewer decodes them.
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    crate::viewer::decode_text(bytes).1
}

/// The bytes to write for `text`: ASCII as it is, anything else as UTF-8 with a byte-order mark.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    if text.is_ascii() {
        return text.as_bytes().to_vec();
    }
    let mut bytes = BOM.to_vec();
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

/// A line's name and stored comment.
fn split(line: &str) -> Option<(&str, &str)> {
    if let Some(quoted) = line.strip_prefix('"') {
        let end = quoted.find('"')?;
        let rest = &quoted[end + 1..];
        return Some((
            &quoted[..end],
            rest.strip_prefix([' ', '\t']).unwrap_or(rest),
        ));
    }
    match line.find([' ', '\t']) {
        Some(end) => Some((&line[..end], &line[end + 1..])),
        None => Some((line, "")),
    }
}

fn same_name(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

fn escape(comment: &str) -> String {
    let comment = comment.replace("\r\n", "\n").replace('\r', "\n");
    if comment.contains('\n') {
        format!(
            "{}{MULTILINE_MARKER}",
            comment.replace('\\', "\\\\").replace('\n', "\\n")
        )
    } else {
        comment
    }
}

fn unescape(stored: &str) -> String {
    let Some(body) = stored.strip_suffix(MULTILINE_MARKER) else {
        return stored.to_owned();
    };
    let mut text = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some('n')) => {
                text.push('\n');
                chars.next();
            }
            ('\\', Some('\\')) => {
                text.push('\\');
                chars.next();
            }
            _ => text.push(c),
        }
    }
    text
}

#[cfg(test)]
#[path = "comments_tests.rs"]
mod tests;
