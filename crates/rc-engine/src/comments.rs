//! File comments as Total Commander keeps them (RC-020-06): one `descript.ion` per folder, a
//! line per file - `name comment`, a name with spaces in double quotes. A comment of several
//! lines is one line with `\n` for each break and the marker U+0004 U+00C2 at its end.
//!
//! The file is read as the viewer reads it (`viewer::detect_whole`: a byte-order mark, UTF-16,
//! valid UTF-8, else Windows-1252, Total Commander's ANSI) and kept line by line as the bytes it
//! had (RC-110-24): a changed comment writes its own line anew, in the file's encoding and with
//! CRLF, and every other line goes back byte for byte - also in a file of a code page that was
//! only read as Windows-1252 (OEM 850, cp1250). A comment the file's encoding cannot hold is
//! refused ([`NotEncodable`]) instead of the file being written in another one. Only a file of
//! plain ASCII takes UTF-8 with a byte-order mark for such a comment, which changes none of its
//! bytes.

use crate::viewer::{
    TextEncoding, detect_whole,
    encoding::{decode as decode_as, encode_char},
};

/// The comment file's name in each folder.
pub const DESCRIPTION_FILE: &str = "descript.ion";

/// A comment's length at most, in characters.
pub const MAX_COMMENT: usize = 4096;

/// A `descript.ion` at most, in bytes: a larger one is not read.
pub const MAX_FILE: u64 = 1024 * 1024;

/// What marks a comment of several lines.
const MULTILINE_MARKER: &str = "\u{4}\u{C2}";

/// A comment or a name with a character the `descript.ion`'s encoding lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotEncodable {
    /// The encoding's usual name.
    pub encoding: &'static str,
}

/// The comments of one folder, in file order, each line with the bytes it has in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptions {
    encoding: TextEncoding,
    /// Whether the file starts with its encoding's byte-order mark.
    mark: bool,
    lines: Vec<Line>,
}

/// A line of the file: its bytes, line end included, and what they say.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    raw: Vec<u8>,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// A file's comment, as stored (escapes and marker included).
    Entry { name: String, stored: String },
    /// A line that is not understood, kept.
    Other,
    /// An empty line, kept while the file has comments.
    Blank,
}

impl Default for Descriptions {
    /// No comments, as a folder without a `descript.ion` has them.
    fn default() -> Self {
        Self {
            encoding: TextEncoding::Utf8,
            mark: false,
            lines: Vec::new(),
        }
    }
}

impl Descriptions {
    /// The comments of a `descript.ion`'s bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let detected = detect_whole(bytes);
        let encoding = detected.encoding;
        let skip = usize::try_from(detected.bom_len)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let lines = raw_lines(&bytes[skip..], encoding)
            .into_iter()
            .map(|raw| Line::read(raw, encoding))
            .collect();
        Self {
            encoding,
            mark: skip > 0,
            lines,
        }
    }

    /// The comments of a `descript.ion`'s text, as UTF-8.
    #[cfg(test)]
    pub fn parse(text: &str) -> Self {
        Self::from_bytes(text.as_bytes())
    }

    /// The comment of `name` (compared ignoring case, as Windows names are), line breaks
    /// restored.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<String> {
        self.lines.iter().find_map(|line| match &line.kind {
            Kind::Entry {
                name: stored_name,
                stored,
            } if same_name(stored_name, name) => Some(unescape(stored)),
            _ => None,
        })
    }

    /// Sets or, with `None` or an empty text, removes the comment of `name`. Only that line
    /// changes; nothing does when the file's encoding cannot hold the name or the comment.
    pub fn set(&mut self, name: &str, comment: Option<&str>) -> Result<(), NotEncodable> {
        let comment = comment.map(str::trim).filter(|comment| !comment.is_empty());
        let at = self.lines.iter().position(|line| {
            matches!(&line.kind, Kind::Entry { name: stored, .. } if same_name(stored, name))
        });
        let Some(comment) = comment else {
            if let Some(at) = at {
                self.lines.remove(at);
            }
            return Ok(());
        };
        let stored = escape(comment);
        let text = if name.contains(' ') {
            format!("\"{name}\" {stored}\r\n")
        } else {
            format!("{name} {stored}\r\n")
        };
        let line = Line {
            raw: self.encode(&text)?,
            kind: Kind::Entry {
                name: name.to_owned(),
                stored,
            },
        };
        match at {
            Some(at) => self.lines[at] = line,
            None => {
                self.end_last_line();
                self.lines.push(line);
            }
        }
        Ok(())
    }

    /// Every file's comment, line breaks restored, in file order (RC-100-90).
    pub fn entries(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.lines.iter().filter_map(|line| match &line.kind {
            Kind::Entry { name, stored } => Some((name.clone(), unescape(stored))),
            Kind::Other | Kind::Blank => None,
        })
    }

    /// Whether no line but empty ones is left.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.kind == Kind::Blank)
    }

    /// The bytes of the file: its byte-order mark when it has one, then every line's.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mark: &[u8] = if self.mark { self.encoding.bom() } else { &[] };
        let lines = self.lines.iter().map(|line| line.raw.len()).sum::<usize>();
        let mut bytes = Vec::with_capacity(mark.len() + lines);
        bytes.extend_from_slice(mark);
        for line in &self.lines {
            bytes.extend_from_slice(&line.raw);
        }
        bytes
    }

    /// `text` in the file's encoding. A file of plain ASCII without a mark - a new one too -
    /// becomes UTF-8 with a mark for a text beyond ASCII: its lines are the same bytes there.
    fn encode(&mut self, text: &str) -> Result<Vec<u8>, NotEncodable> {
        let mut bytes = Vec::with_capacity(text.len() * self.encoding.unit());
        for c in text.chars() {
            let Some(encoded) = encode_char(c, self.encoding) else {
                return Err(NotEncodable {
                    encoding: self.encoding.label(),
                });
            };
            bytes.extend_from_slice(&encoded);
        }
        if self.encoding == TextEncoding::Utf8
            && !self.mark
            && !text.is_ascii()
            && self.lines.iter().all(|line| line.raw.is_ascii())
        {
            self.mark = true;
        }
        Ok(bytes)
    }

    /// Ends the last line with CRLF when the file stops without a line break, before a line
    /// follows it; a stray half of a UTF-16 unit there goes.
    fn end_last_line(&mut self) {
        let encoding = self.encoding;
        let Some(last) = self.lines.last_mut() else {
            return;
        };
        let unit = encoding.unit();
        let whole = last.raw.len() - last.raw.len() % unit;
        let ended = whole == last.raw.len()
            && whole >= unit
            && encoding.unit_value(&last.raw[whole - unit..]) == LINE_FEED;
        if ended {
            return;
        }
        last.raw.truncate(whole);
        for c in ['\r', '\n'] {
            if let Some(encoded) = encode_char(c, encoding) {
                last.raw.extend_from_slice(&encoded);
            }
        }
    }
}

impl Line {
    /// The line of the bytes `raw`, its line end among them.
    fn read(raw: &[u8], encoding: TextEncoding) -> Self {
        let text = decode_as(raw, encoding);
        let text = text.trim_end_matches(['\r', '\n']);
        let kind = if text.trim().is_empty() {
            Kind::Blank
        } else {
            match split(text) {
                Some((name, stored)) => Kind::Entry {
                    name: name.to_owned(),
                    stored: stored.to_owned(),
                },
                None => Kind::Other,
            }
        };
        Self {
            raw: raw.to_vec(),
            kind,
        }
    }
}

const LINE_FEED: u16 = 0x0A;

/// `bytes` cut after every line feed of `encoding`; what follows the last one is a line too.
fn raw_lines(bytes: &[u8], encoding: TextEncoding) -> Vec<&[u8]> {
    let unit = encoding.unit();
    let mut lines = Vec::new();
    let (mut start, mut at) = (0, 0);
    while at + unit <= bytes.len() {
        let end = at + unit;
        if encoding.unit_value(&bytes[at..end]) == LINE_FEED {
            lines.push(&bytes[start..end]);
            start = end;
        }
        at = end;
    }
    if start < bytes.len() {
        lines.push(&bytes[start..]);
    }
    lines
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
