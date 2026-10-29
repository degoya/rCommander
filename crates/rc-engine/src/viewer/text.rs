//! Lines of text from a byte offset on: what the text view shows.
//!
//! The view keeps its place as a byte offset, never as a line number, so it can show the end
//! of a 5 GB log before any line is counted: [`read_lines`] reads a page of lines from an
//! offset, [`lines_before`] walks back for scrolling up, and the scrollbar maps to bytes until
//! the [`crate::viewer::LineIndex`] has counted the lines. A line break is LF, CRLF or a lone
//! CR, in the file's code units; a line longer than [`MAX_LINE_BYTES`] is cut into pieces.

use rc_vfs::VfsError;
use serde::Serialize;

use super::{
    BLOCK_SIZE, Detected, FileWindow,
    encoding::{TextEncoding, char_bounds, decode_window},
};

/// The longest piece of a line the view shows as one line.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

const LF: u16 = 0x0A;
const CR: u16 = 0x0D;

/// One line as the view shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextLine {
    /// Where the line's first byte is.
    pub offset: u64,
    /// The line without its break.
    pub text: String,
}

/// A page of lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextPage {
    pub lines: Vec<TextLine>,
    /// Where the line after the page starts.
    pub end: u64,
    /// Whether the page reaches the end of the file.
    pub eof: bool,
}

/// How a file ends its lines, for the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

/// A line break found in a buffer: the line's length and the break's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Break {
    pub content: usize,
    pub len: usize,
}

/// The first line break in `bytes` (whole code units from their start). `None` when there is
/// none, or when the buffer ends on a CR whose LF may follow (`at_eof` says it cannot).
pub(crate) fn find_break(bytes: &[u8], encoding: TextEncoding, at_eof: bool) -> Option<Break> {
    let unit = encoding.unit();
    let mut at = 0;
    while at + unit <= bytes.len() {
        match encoding.unit_value(&bytes[at..]) {
            LF => {
                return Some(Break {
                    content: at,
                    len: unit,
                });
            }
            CR => {
                let next = at + unit;
                if next + unit <= bytes.len() {
                    let len = if encoding.unit_value(&bytes[next..]) == LF {
                        2 * unit
                    } else {
                        unit
                    };
                    return Some(Break { content: at, len });
                }
                return at_eof.then_some(Break {
                    content: at,
                    len: unit,
                });
            }
            _ => at += unit,
        }
    }
    None
}

/// The first line ending in `head`, after its byte-order mark.
#[must_use]
pub fn detect_line_ending(head: &[u8], detected: Detected) -> Option<LineEnding> {
    let skip = usize::try_from(detected.bom_len)
        .unwrap_or(usize::MAX)
        .min(head.len());
    let found = find_break(&head[skip..], detected.encoding, false)?;
    let unit = detected.encoding.unit();
    Some(match (found.len / unit, found.content) {
        (2, _) => LineEnding::CrLf,
        (_, at) if detected.encoding.unit_value(&head[skip + at..]) == CR => LineEnding::Cr,
        _ => LineEnding::Lf,
    })
}

/// Where the text of a file starts at or after `offset`: past the byte-order mark and, in
/// UTF-16, on a code unit.
#[must_use]
pub fn align(offset: u64, detected: Detected) -> u64 {
    let offset = offset.max(detected.bom_len);
    let unit = detected.encoding.unit() as u64;
    detected.bom_len + (offset - detected.bom_len).next_multiple_of(unit)
}

/// Up to `max_lines` lines from `offset` (a line start, or any byte - the page then starts
/// with the rest of that line).
///
/// # Errors
///
/// The backend failed to read.
pub async fn read_lines(
    window: &FileWindow,
    detected: Detected,
    offset: u64,
    max_lines: usize,
) -> Result<TextPage, VfsError> {
    let size = window.size();
    let mut pos = align(offset, detected);
    let mut buf: Vec<u8> = Vec::new();
    let mut buf_start = pos;
    let mut lines = Vec::new();
    while lines.len() < max_lines && pos < size {
        let skip = usize::try_from(pos - buf_start).unwrap_or(usize::MAX);
        if skip > 0 {
            buf.drain(..skip.min(buf.len()));
            buf_start = pos;
        }
        let at_eof = buf_start + buf.len() as u64 >= size;
        let piece = find_break(&buf, detected.encoding, at_eof)
            .filter(|found| found.content <= MAX_LINE_BYTES)
            .map(|found| (found.content, found.len))
            .or_else(|| {
                if buf.len() > MAX_LINE_BYTES {
                    let (_, cut) = char_bounds(&buf[..MAX_LINE_BYTES], pos, detected, false);
                    Some((cut.max(detected.encoding.unit()), 0))
                } else {
                    at_eof.then_some((buf.len(), 0))
                }
            });
        let Some((content, break_len)) = piece else {
            let more = window
                .read_at(buf_start + buf.len() as u64, BLOCK_SIZE)
                .await?;
            if more.is_empty() {
                break;
            }
            buf.extend_from_slice(&more);
            continue;
        };
        let decoded = decode_window(&buf[..content], pos, detected, true);
        lines.push(TextLine {
            offset: decoded.start.min(pos + content as u64),
            text: decoded.text,
        });
        pos += (content + break_len) as u64;
    }
    Ok(TextPage {
        lines,
        end: pos.min(size),
        eof: pos >= size,
    })
}

/// Where the line holding the byte at `offset` starts: after the last line break before it,
/// looked for at most [`MAX_LINE_BYTES`] back.
///
/// # Errors
///
/// The backend failed to read.
pub async fn line_start(
    window: &FileWindow,
    detected: Detected,
    offset: u64,
) -> Result<u64, VfsError> {
    let offset = align(offset.min(window.size()), detected);
    let floor = align(offset.saturating_sub(MAX_LINE_BYTES as u64), detected);
    if offset <= floor {
        return Ok(offset);
    }
    let unit = detected.encoding.unit();
    // One unit past `offset`, to tell a CR of CRLF from a lone one.
    let len = usize::try_from(offset - floor).unwrap_or(usize::MAX) + unit;
    let bytes = window.read_at(floor, len).await?;
    let before = usize::try_from(offset - floor)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let mut at = before - before % unit;
    while at >= unit {
        at -= unit;
        let value = detected.encoding.unit_value(&bytes[at..]);
        let is_break = value == LF
            || (value == CR
                && bytes
                    .get(at + unit..at + 2 * unit)
                    .is_none_or(|next| detected.encoding.unit_value(next) != LF));
        if is_break {
            return Ok(floor + (at + unit) as u64);
        }
    }
    Ok(floor)
}

/// The start of the line `count` lines above the one starting at `offset`; the first line's
/// start when there are fewer.
///
/// # Errors
///
/// The backend failed to read.
pub async fn lines_before(
    window: &FileWindow,
    detected: Detected,
    offset: u64,
    count: usize,
) -> Result<u64, VfsError> {
    let unit = detected.encoding.unit() as u64;
    let first = align(0, detected);
    let mut at = line_start(window, detected, offset).await?;
    for _ in 0..count {
        if at <= first {
            break;
        }
        at = line_start(window, detected, at - unit).await?;
    }
    Ok(at)
}

/// Where the view starts to show the last `rows` lines of the file ("scroll to the end").
///
/// # Errors
///
/// The backend failed to read.
pub async fn last_page(
    window: &FileWindow,
    detected: Detected,
    rows: usize,
) -> Result<u64, VfsError> {
    // The last byte belongs to the last line, also when it is that line's break.
    let last = window
        .size()
        .saturating_sub(detected.encoding.unit() as u64);
    lines_before(window, detected, last, rows.saturating_sub(1)).await
}

/// `text` with every tab replaced by the spaces up to the next multiple of `width`.
#[must_use]
pub fn expand_tabs(text: &str, width: usize) -> String {
    if width == 0 || !text.contains('\t') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + width);
    let mut column = 0;
    for c in text.chars() {
        if c == '\t' {
            let spaces = width - column % width;
            out.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            out.push(c);
            column += 1;
        }
    }
    out
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
