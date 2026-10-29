//! Line comparison of two text files: decoding, the line diff (the `similar` crate, Myers with
//! a deadline), the rows of a side-by-side view, jumping between differences, and a unified
//! diff for agents.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffOp};

use crate::viewer::{TextEncoding, detect_whole, encoding::decode as decode_as};

/// How long the line diff may search for the shortest edit; past it the diff is still correct,
/// only not minimal (a pathological pair of files cannot hold the comparison up).
const DIFF_DEADLINE: Duration = Duration::from_secs(5);

/// Leading bytes searched for a NUL, which marks a file as binary (as git does).
const BINARY_PROBE: usize = 8000;

/// Line breaks a file may hold to be compared line by line. Every line costs a `String` and a
/// key however short it is, so 16 MiB of nothing but line breaks would take gigabytes.
pub const MAX_LINES: usize = 1_000_000;

/// What a line comparison ignores.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextOptions {
    /// Spaces, tabs and every other white space inside a line.
    pub ignore_whitespace: bool,
    /// Upper and lower case.
    pub ignore_case: bool,
    /// `\n`, `\r\n`, `\r`, and whether the last line has an ending at all.
    pub ignore_line_endings: bool,
}

/// How a file's bytes were read as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Utf8,
    /// UTF-8 behind a byte order mark.
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// Anything that is neither UTF-16 nor valid UTF-8: Total Commander's ANSI code page.
    Windows1252,
}

/// How a line ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEnding {
    /// The last line, without an ending.
    None,
    Lf,
    CrLf,
    Cr,
}

/// One line, without its ending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    pub ending: LineEnding,
}

/// A file read as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFile {
    pub encoding: Encoding,
    pub lines: Vec<Line>,
}

/// `bytes` as text, or `None` for a binary file or one of [`MAX_LINES`] line breaks or more.
/// The encoding is the viewer's ([`detect_whole`]): a byte order mark, UTF-16 without one, valid
/// UTF-8, else Windows-1252; a NUL byte in the first 8000 bytes of a file in none of the UTF-16
/// forms and without a mark makes it binary.
#[must_use]
pub fn decode(bytes: &[u8]) -> Option<TextFile> {
    let detected = detect_whole(bytes);
    let encoding = match detected.encoding {
        TextEncoding::Utf8 if detected.bom_len > 0 => Encoding::Utf8Bom,
        TextEncoding::Utf8 => Encoding::Utf8,
        TextEncoding::Utf16Le => Encoding::Utf16Le,
        TextEncoding::Utf16Be => Encoding::Utf16Be,
        // Detection never answers Latin-1 or CP437; Windows-1252 is Latin-1's superset.
        TextEncoding::Latin1 | TextEncoding::Windows1252 | TextEncoding::Cp437 => {
            Encoding::Windows1252
        }
    };
    let utf16 = matches!(encoding, Encoding::Utf16Le | Encoding::Utf16Be);
    if !utf16 && detected.bom_len == 0 && bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        return None;
    }
    let mark = usize::try_from(detected.bom_len).unwrap_or(0);
    let text = decode_as(bytes.get(mark..).unwrap_or_default(), detected.encoding);
    if text
        .bytes()
        .filter(|byte| matches!(byte, b'\n' | b'\r'))
        .count()
        >= MAX_LINES
    {
        return None;
    }
    Some(TextFile {
        encoding,
        lines: split_lines(&text),
    })
}

fn split_lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let Some(at) = rest.find(['\n', '\r']) else {
            lines.push(Line {
                text: rest.to_owned(),
                ending: LineEnding::None,
            });
            break;
        };
        let (ending, len) = if rest[at..].starts_with("\r\n") {
            (LineEnding::CrLf, 2)
        } else if rest[at..].starts_with('\r') {
            (LineEnding::Cr, 1)
        } else {
            (LineEnding::Lf, 1)
        };
        lines.push(Line {
            text: rest[..at].to_owned(),
            ending,
        });
        rest = &rest[at + len..];
    }
    lines
}

/// What a line is compared by under `options`.
fn key(line: &Line, options: &TextOptions) -> (String, Option<LineEnding>) {
    let mut text = if options.ignore_whitespace {
        line.text.chars().filter(|c| !c.is_whitespace()).collect()
    } else {
        line.text.clone()
    };
    if options.ignore_case {
        text = text.to_lowercase();
    }
    (text, (!options.ignore_line_endings).then_some(line.ending))
}

/// What a row of the side-by-side view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    /// The same line on both sides (under the options).
    Equal,
    /// A line on each side, different.
    Changed,
    /// A line only on the left.
    Deleted,
    /// A line only on the right.
    Inserted,
}

/// One row of the side-by-side view: indices into the two files' lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRow {
    pub kind: RowKind,
    pub left: Option<usize>,
    pub right: Option<usize>,
}

/// Line numbers (from 1) on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinePosition {
    pub left: usize,
    pub right: usize,
}

/// A unified diff, cut at a size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unified {
    pub text: String,
    /// The diff went on past the size limit.
    pub truncated: bool,
}

/// Two text files and the rows that line them up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDiff {
    pub left: TextFile,
    pub right: TextFile,
    pub rows: Vec<DiffRow>,
}

impl TextDiff {
    /// Lines up `left` and `right`, comparing lines under `options`.
    #[must_use]
    pub fn compute(left: TextFile, right: TextFile, options: &TextOptions) -> Self {
        let rows = {
            let left_keys: Vec<_> = left.lines.iter().map(|line| key(line, options)).collect();
            let right_keys: Vec<_> = right.lines.iter().map(|line| key(line, options)).collect();
            let deadline = Instant::now() + DIFF_DEADLINE;
            let ops = similar::capture_diff_slices_deadline(
                Algorithm::Myers,
                &left_keys,
                &right_keys,
                Some(deadline),
            );
            rows_of(&ops)
        };
        Self { left, right, rows }
    }

    /// No row differs.
    #[must_use]
    pub fn is_equal(&self) -> bool {
        self.rows.iter().all(|row| row.kind == RowKind::Equal)
    }

    /// The rows that start a block of differing rows, in order.
    pub fn change_starts(&self) -> impl Iterator<Item = usize> + '_ {
        self.rows.iter().enumerate().filter_map(|(index, row)| {
            let starts = row.kind != RowKind::Equal
                && (index == 0 || self.rows[index - 1].kind == RowKind::Equal);
            starts.then_some(index)
        })
    }

    /// How many blocks of differing rows there are.
    #[must_use]
    pub fn changes(&self) -> usize {
        self.change_starts().count()
    }

    /// The start of the next block after row `from`; the first block when `from` is `None`.
    #[must_use]
    pub fn next_change(&self, from: Option<usize>) -> Option<usize> {
        self.change_starts()
            .find(|&start| from.is_none_or(|from| start > from))
    }

    /// The start of the last block before row `from`.
    #[must_use]
    pub fn previous_change(&self, from: usize) -> Option<usize> {
        self.change_starts()
            .take_while(|&start| start < from)
            .last()
    }

    /// The line numbers at `row`: on a side without a line there, the next line it has.
    #[must_use]
    pub fn position(&self, row: usize) -> LinePosition {
        let before = &self.rows[..row.min(self.rows.len())];
        LinePosition {
            left: before.iter().filter(|r| r.left.is_some()).count() + 1,
            right: before.iter().filter(|r| r.right.is_some()).count() + 1,
        }
    }

    /// A unified diff with `context` equal lines around each change, at most `limit` bytes.
    #[must_use]
    pub fn unified(&self, context: usize, limit: usize) -> Unified {
        let mut out = String::new();
        let mut truncated = false;
        for (start, end) in self.hunks(context) {
            let lines = self.hunk_lines(start, end);
            if lines.iter().map(String::len).sum::<usize>() + out.len() > limit {
                truncated = true;
                break;
            }
            for line in lines {
                out.push_str(&line);
            }
        }
        Unified {
            text: out,
            truncated,
        }
    }

    /// Row ranges of the hunks: each block of changes with `context` rows around it, blocks
    /// that touch merged.
    fn hunks(&self, context: usize) -> Vec<(usize, usize)> {
        let mut hunks: Vec<(usize, usize)> = Vec::new();
        for start in self.change_starts() {
            let end = self.rows[start..]
                .iter()
                .position(|row| row.kind == RowKind::Equal)
                .map_or(self.rows.len(), |len| start + len);
            let (from, to) = (
                start.saturating_sub(context),
                (end + context).min(self.rows.len()),
            );
            match hunks.last_mut() {
                Some(last) if last.1 >= from => last.1 = to,
                _ => hunks.push((from, to)),
            }
        }
        hunks
    }

    fn hunk_lines(&self, start: usize, end: usize) -> Vec<String> {
        let at = self.position(start);
        let rows = &self.rows[start..end];
        let left_len = rows.iter().filter(|r| r.left.is_some()).count();
        let right_len = rows.iter().filter(|r| r.right.is_some()).count();
        // An empty side names the line before the hunk, as `diff -u` does.
        let from = |line: usize, len: usize| if len == 0 { line - 1 } else { line };
        let mut lines = vec![format!(
            "@@ -{},{left_len} +{},{right_len} @@\n",
            from(at.left, left_len),
            from(at.right, right_len)
        )];
        let mut index = 0;
        while index < rows.len() {
            if rows[index].kind == RowKind::Equal {
                if let Some(line) = rows[index].left {
                    lines.push(format!(" {}\n", self.left.lines[line].text));
                }
                index += 1;
                continue;
            }
            let block_end = rows[index..]
                .iter()
                .position(|row| row.kind == RowKind::Equal)
                .map_or(rows.len(), |len| index + len);
            let block = &rows[index..block_end];
            for line in block.iter().filter_map(|row| row.left) {
                lines.push(format!("-{}\n", self.left.lines[line].text));
            }
            for line in block.iter().filter_map(|row| row.right) {
                lines.push(format!("+{}\n", self.right.lines[line].text));
            }
            index = block_end;
        }
        lines
    }
}

/// The side-by-side rows of a diff: a replaced block pairs its lines as changed, the rest of
/// the longer side is deleted or inserted.
fn rows_of(ops: &[DiffOp]) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let row = |kind, left, right| DiffRow { kind, left, right };
    for op in ops {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => rows.extend(
                (0..len).map(|i| row(RowKind::Equal, Some(old_index + i), Some(new_index + i))),
            ),
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                rows.extend((0..old_len).map(|i| row(RowKind::Deleted, Some(old_index + i), None)))
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                rows.extend((0..new_len).map(|i| row(RowKind::Inserted, None, Some(new_index + i))))
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for i in 0..old_len.max(new_len) {
                    let left = (i < old_len).then_some(old_index + i);
                    let right = (i < new_len).then_some(new_index + i);
                    let kind = match (left, right) {
                        (Some(_), Some(_)) => RowKind::Changed,
                        (Some(_), None) => RowKind::Deleted,
                        _ => RowKind::Inserted,
                    };
                    rows.push(row(kind, left, right));
                }
            }
        }
    }
    rows
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
