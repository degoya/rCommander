//! Where a header line names a path: byte positions `(start, end)` within the line, so the tree
//! keeps an offset and a length instead of the path.

use crate::classify::Header;

/// The bytes after `--- `, `+++ ` and `*** `.
const MARKER: usize = 4;

/// The old and the new path a `diff` line names; the old one empty when it names one path only.
/// `diff --git a/x b/y` names both; `diff --cc x` and `diff --combined x` the one; any other
/// `diff` command line its last word.
pub(super) fn of_diff_line(line: &[u8]) -> Option<((usize, usize), (usize, usize))> {
    let len = line.len();
    if let Some(rest) = line.strip_prefix(b"diff --git ") {
        let base = len - rest.len();
        let Some(split) = rest.windows(3).rposition(|window| window == b" b/") else {
            // Quoted paths (`"a/x y" "b/x y"`): the whole as the label.
            return Some(((0, 0), (base, len)));
        };
        let from = if rest.starts_with(b"a/") {
            (base + 2, base + split)
        } else {
            (0, 0)
        };
        return Some((from, (base + split + 3, len)));
    }
    for option in [&b"diff --cc "[..], b"diff --combined "] {
        if line.starts_with(option) {
            return Some(((0, 0), (option.len(), len)));
        }
    }
    let start = line.iter().rposition(|byte| *byte == b' ')? + 1;
    Some(((0, 0), (start, len)))
}

/// What follows the header's marker: the path of `---`, `+++`, `***`, `rename from` and the
/// like; empty for a header that names none.
pub(super) fn after_prefix(header: Header, line: &[u8]) -> (usize, usize) {
    let len = line.len();
    match header {
        Header::Minus | Header::Plus | Header::Star => (MARKER.min(len), len),
        Header::RenameFrom | Header::RenameTo | Header::CopyFrom | Header::CopyTo => {
            // `rename from x`, `copy to x`: after the second blank.
            let start = line
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b' ')
                .nth(1)
                .map_or(len, |(at, _)| at + 1);
            (start, len)
        }
        _ => (len, len),
    }
}

/// The path of a `---`, `+++` or `***` line: up to the tab before a timestamp, without git's
/// `a/` or `b/`.
pub(super) fn header_path(line: &[u8], rest: (usize, usize), git: bool) -> (usize, usize) {
    let (mut start, end) = rest;
    let Some(text) = line.get(start..end) else {
        return (end, end);
    };
    let end = start
        + text
            .iter()
            .position(|byte| *byte == b'\t')
            .unwrap_or(text.len());
    if git && (text.starts_with(b"a/") || text.starts_with(b"b/")) {
        start = (start + 2).min(end);
    }
    (start, end)
}

/// Whether the path at `path` is `/dev/null`: no file on that side.
pub(super) fn is_dev_null(line: &[u8], path: (usize, usize)) -> bool {
    line.get(path.0..path.1) == Some(b"/dev/null".as_slice())
}
