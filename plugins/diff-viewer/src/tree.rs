//! The `tree` output: a node per file of the patch - its path, `+added -removed` and whether it
//! is new, deleted, renamed, copied or binary - and under it a node per hunk: its header and its
//! line count.
//!
//! `open` builds the tree as it classifies the lines, and keeps no text: a path is where its
//! bytes are in the file, a hunk where its header line is, read again when a node is shown. Both
//! lists are capped ([`MAX_FILES`], [`MAX_HUNKS`]); what lies beyond is counted and named by one
//! last node. More than [`PAGE`] files, or hunks of one file, are shown as ranges of [`PAGE`].

mod paths;

use crate::classify::{Header, Kind};
use crate::types::{Failure, FailureKind, Source, TreeNode};

/// The most files kept; a power of two, so the vector never grows past it.
pub const MAX_FILES: usize = 1 << 15;
/// The most hunks kept, over all files.
pub const MAX_HUNKS: usize = 1 << 17;
/// The most nodes one expansion lists before it lists ranges instead.
pub const PAGE: usize = 1000;
/// The longest path kept for a label, in bytes.
const MAX_PATH_BYTES: usize = 1024;
/// The bytes of a hunk header read for its label.
const MAX_HEADER_BYTES: u32 = 256;

/// Where a node's id keeps its kind.
const KIND_SHIFT: u32 = 56;
const FILE: u64 = 1;
const HUNK: u64 = 2;
const FILE_RANGE: u64 = 3;
const HUNK_RANGE: u64 = 4;
const UNLISTED: u64 = 5;
/// Where a hunk range's id keeps its file.
const FILE_SHIFT: u32 = 24;

const NEW: u8 = 1;
const DELETED: u8 = 2;
const RENAMED: u8 = 4;
const COPIED: u8 = 8;
const BINARY: u8 = 16;

/// Bytes of the file: a path to show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Span {
    offset: u64,
    len: u16,
}

#[derive(Clone, Copy, Debug, Default)]
struct File {
    path: Span,
    /// The path a rename or copy came from.
    from: Span,
    added: u64,
    removed: u64,
    flags: u8,
    first_hunk: u32,
    hunks: u32,
}

#[derive(Clone, Copy, Debug)]
struct Hunk {
    /// Where its header line starts.
    offset: u64,
    lines: u32,
}

/// Where the headers of the file being read stand.
#[derive(Clone, Copy, Debug, Default)]
struct Reading {
    /// A file is open: its lines count for it.
    in_file: bool,
    /// The file is still in its headers: a `---` or `***` belongs to it.
    headers: bool,
    /// It started with `diff --git`: `a/` and `b/` are prefixes.
    git: bool,
    /// A `***` came: this is a context diff, whose `---` names the new file.
    star: bool,
    /// A `---` came.
    minus: bool,
    /// A context diff's hunk whose label is still `***************`: its first range names it.
    label_pending: bool,
    /// The hunk being read is kept: the last of `hunks`.
    hunk: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Tree {
    files: Vec<File>,
    hunks: Vec<Hunk>,
    reading: Reading,
    /// The current file is past [`MAX_FILES`]: nothing is kept for it.
    dropped: bool,
    unlisted_files: u64,
    unlisted_hunks: u64,
}

impl Tree {
    /// Takes the line at `offset`, of kind `kind`, whose bytes start with `line`.
    pub(crate) fn line(&mut self, kind: Kind, line: &[u8], offset: u64) {
        match kind {
            Kind::DiffStart => {
                self.start_file();
                self.reading.git = line.starts_with(b"diff --git ");
                if let Some((from, path)) = paths::of_diff_line(line) {
                    self.set_path(offset, line, path);
                    self.set(|file, span| file.from = span, offset, line, from);
                }
            }
            Kind::Header(header) => self.header(header, line, offset),
            Kind::HunkHead { .. } | Kind::ContextHunk => {
                if !self.reading.in_file {
                    self.start_file();
                }
                self.reading.headers = false;
                self.reading.label_pending = kind == Kind::ContextHunk;
                self.start_hunk(offset);
            }
            Kind::ContextRange => {
                if std::mem::take(&mut self.reading.label_pending)
                    && let Some(hunk) = self.current_hunk()
                {
                    hunk.offset = offset;
                }
            }
            Kind::Added | Kind::Removed | Kind::Unchanged => self.count(kind),
            Kind::Text | Kind::NoNewline => {}
        }
    }

    fn header(&mut self, header: Header, line: &[u8], offset: u64) {
        let rest = paths::after_prefix(header, line);
        match header {
            Header::Star => {
                // A `diff` line may have opened the file; a second `***` starts the next one.
                if !self.reading.headers || self.reading.star || self.reading.minus {
                    self.start_file();
                }
                self.reading.star = true;
                self.set_path(offset, line, paths::header_path(line, rest, false));
            }
            Header::Minus => {
                if self.reading.star && self.reading.headers {
                    self.new_path(offset, line, rest);
                    return;
                }
                if !self.reading.headers || self.reading.minus {
                    self.start_file();
                }
                self.reading.minus = true;
                let path = paths::header_path(line, rest, self.reading.git);
                if paths::is_dev_null(line, path) {
                    self.flag(NEW);
                } else {
                    self.set_path(offset, line, path);
                }
            }
            Header::Plus => self.new_path(offset, line, rest),
            Header::NewFile => self.flag(NEW),
            Header::Deleted => self.flag(DELETED),
            Header::Binary => self.flag(BINARY),
            Header::RenameFrom | Header::CopyFrom => {
                self.flag(if header == Header::RenameFrom {
                    RENAMED
                } else {
                    COPIED
                });
                self.set(|file, span| file.from = span, offset, line, rest);
            }
            Header::RenameTo | Header::CopyTo => self.set_path(offset, line, rest),
            Header::Other => {}
        }
    }

    /// The path a `+++` (or a context diff's `---`) names: `/dev/null` marks the file deleted.
    fn new_path(&mut self, offset: u64, line: &[u8], rest: (usize, usize)) {
        let path = paths::header_path(line, rest, self.reading.git);
        if paths::is_dev_null(line, path) {
            self.flag(DELETED);
        } else {
            self.set_path(offset, line, path);
        }
    }

    fn start_file(&mut self) {
        self.reading = Reading {
            in_file: true,
            headers: true,
            ..Reading::default()
        };
        self.dropped = self.files.len() >= MAX_FILES;
        if self.dropped {
            self.unlisted_files += 1;
            return;
        }
        let first_hunk = u32::try_from(self.hunks.len()).unwrap_or(u32::MAX);
        self.files.push(File {
            first_hunk,
            ..File::default()
        });
    }

    fn current_file(&mut self) -> Option<&mut File> {
        if self.dropped {
            return None;
        }
        self.files.last_mut()
    }

    fn current_hunk(&mut self) -> Option<&mut Hunk> {
        if self.dropped || !self.reading.hunk {
            return None;
        }
        self.hunks.last_mut()
    }

    fn start_hunk(&mut self, offset: u64) {
        self.reading.hunk = false;
        if self.dropped {
            return;
        }
        if self.hunks.len() >= MAX_HUNKS {
            self.unlisted_hunks += 1;
            return;
        }
        let Some(file) = self.files.last_mut() else {
            return;
        };
        // A file's hunks are one run: a hunk after the cap is never kept, so no other file's
        // hunk comes between.
        file.hunks += 1;
        self.hunks.push(Hunk { offset, lines: 0 });
        self.reading.hunk = true;
    }

    fn count(&mut self, kind: Kind) {
        let Some(file) = self.current_file() else {
            return;
        };
        match kind {
            Kind::Added => file.added += 1,
            Kind::Removed => file.removed += 1,
            _ => {}
        }
        if let Some(hunk) = self.current_hunk() {
            hunk.lines = hunk.lines.saturating_add(1);
        }
    }

    fn flag(&mut self, flag: u8) {
        if let Some(file) = self.current_file() {
            file.flags |= flag;
        }
    }

    fn set_path(&mut self, offset: u64, line: &[u8], at: (usize, usize)) {
        self.set(|file, span| file.path = span, offset, line, at);
    }

    /// Sets a span of the current file to bytes `at` (start, end) of `line`, which starts at
    /// `offset`.
    fn set(
        &mut self,
        field: impl FnOnce(&mut File, Span),
        offset: u64,
        line: &[u8],
        at: (usize, usize),
    ) {
        let (start, end) = at;
        if start >= end || end > line.len() {
            return;
        }
        let len = u16::try_from((end - start).min(MAX_PATH_BYTES)).unwrap_or(0);
        if let Some(file) = self.current_file() {
            field(
                file,
                Span {
                    offset: offset + start as u64,
                    len,
                },
            );
        }
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.files.capacity() * size_of::<File>() + self.hunks.capacity() * size_of::<Hunk>()
    }

    /// The files kept, for the tests.
    #[cfg(test)]
    pub(crate) fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The children of `node`, or the top level for `None`.
    pub(crate) fn children<S: Source>(
        &self,
        src: &mut S,
        node: Option<u64>,
    ) -> Result<Vec<TreeNode>, Failure> {
        let Some(id) = node else {
            let mut nodes = self.files_from(src, 0, self.files.len(), true)?;
            if self.unlisted_files > 0 || self.unlisted_hunks > 0 {
                nodes.push(TreeNode {
                    id: UNLISTED << KIND_SHIFT,
                    label: "not listed".to_owned(),
                    value: Some(format!(
                        "{} more files, {} more hunks",
                        self.unlisted_files, self.unlisted_hunks
                    )),
                    has_children: false,
                });
            }
            return Ok(nodes);
        };
        let payload = id & ((1 << KIND_SHIFT) - 1);
        let not_found = || Failure::of(FailureKind::NotFound);
        match id >> KIND_SHIFT {
            FILE => {
                let file = self.file(payload).ok_or_else(not_found)?;
                let first = file.first_hunk as usize;
                self.hunks_of(src, payload, first, first + file.hunks as usize, true)
            }
            FILE_RANGE => {
                let lo = usize::try_from(payload).map_err(|_| not_found())?;
                self.files_from(
                    src,
                    lo,
                    lo.saturating_add(PAGE).min(self.files.len()),
                    false,
                )
            }
            HUNK_RANGE => {
                let index = payload >> FILE_SHIFT;
                let lo =
                    usize::try_from(payload & ((1 << FILE_SHIFT) - 1)).map_err(|_| not_found())?;
                let file = self.file(index).ok_or_else(not_found)?;
                let first = file.first_hunk as usize;
                let end = first + file.hunks as usize;
                let start = first.saturating_add(lo);
                self.hunks_of(
                    src,
                    index,
                    start,
                    start.saturating_add(PAGE).min(end),
                    false,
                )
            }
            HUNK | UNLISTED => Ok(Vec::new()),
            _ => Err(not_found()),
        }
    }

    fn file(&self, index: u64) -> Option<&File> {
        self.files.get(usize::try_from(index).ok()?)
    }

    /// Files `lo..hi`, or ranges of them when `paged` and there are more than [`PAGE`].
    fn files_from<S: Source>(
        &self,
        src: &mut S,
        lo: usize,
        hi: usize,
        paged: bool,
    ) -> Result<Vec<TreeNode>, Failure> {
        if paged && hi - lo > PAGE {
            return Ok(ranges(
                lo,
                hi,
                |start| (FILE_RANGE << KIND_SHIFT) | start as u64,
                "files",
            ));
        }
        let mut nodes = Vec::with_capacity(hi.saturating_sub(lo));
        for index in lo..hi {
            let Some(file) = self.files.get(index) else {
                break;
            };
            nodes.push(TreeNode {
                id: (FILE << KIND_SHIFT) | index as u64,
                label: read_span(src, file.path)?.unwrap_or_else(|| "(unnamed)".to_owned()),
                value: Some(file_value(src, file)?),
                has_children: file.hunks > 0,
            });
        }
        Ok(nodes)
    }

    /// Hunks `lo..hi` of file `file`, or ranges of them when `paged` and there are more than
    /// [`PAGE`].
    fn hunks_of<S: Source>(
        &self,
        src: &mut S,
        file: u64,
        lo: usize,
        hi: usize,
        paged: bool,
    ) -> Result<Vec<TreeNode>, Failure> {
        if paged && hi - lo > PAGE {
            let first = lo;
            return Ok(ranges(
                lo,
                hi,
                |start| (HUNK_RANGE << KIND_SHIFT) | (file << FILE_SHIFT) | (start - first) as u64,
                "hunks",
            ));
        }
        let mut nodes = Vec::with_capacity(hi.saturating_sub(lo));
        for index in lo..hi {
            let Some(hunk) = self.hunks.get(index) else {
                break;
            };
            nodes.push(TreeNode {
                id: (HUNK << KIND_SHIFT) | index as u64,
                label: read_line(src, hunk.offset)?,
                value: Some(match hunk.lines {
                    1 => "1 line".to_owned(),
                    lines => format!("{lines} lines"),
                }),
                has_children: false,
            });
        }
        Ok(nodes)
    }
}

/// Range nodes over `lo..hi`, [`PAGE`] each, numbered from 1 within the list.
fn ranges(lo: usize, hi: usize, id: impl Fn(usize) -> u64, what: &str) -> Vec<TreeNode> {
    (lo..hi)
        .step_by(PAGE)
        .map(|start| {
            let end = (start + PAGE).min(hi);
            TreeNode {
                id: id(start),
                label: format!("{what} {}..{}", start - lo + 1, end - lo),
                value: None,
                has_children: true,
            }
        })
        .collect()
}

/// `+added -removed` (the minus sign U+2212), then what else is known of the file.
fn file_value<S: Source>(src: &mut S, file: &File) -> Result<String, Failure> {
    let mut value = format!("+{} \u{2212}{}", file.added, file.removed);
    for (flag, word) in [(NEW, "new"), (DELETED, "deleted"), (BINARY, "binary")] {
        if file.flags & flag != 0 {
            value.push_str(", ");
            value.push_str(word);
        }
    }
    for (flag, word) in [(RENAMED, "renamed"), (COPIED, "copied")] {
        if file.flags & flag != 0 {
            value.push_str(", ");
            value.push_str(word);
            if let Some(from) = read_span(src, file.from)? {
                value.push_str(" from ");
                value.push_str(&from);
            }
        }
    }
    Ok(value)
}

/// The text of `span`; `None` for an empty one.
fn read_span<S: Source>(src: &mut S, span: Span) -> Result<Option<String>, Failure> {
    if span.len == 0 {
        return Ok(None);
    }
    let bytes = src.read_at(span.offset, u32::from(span.len))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// The line starting at `offset`, up to [`MAX_HEADER_BYTES`] of it.
fn read_line<S: Source>(src: &mut S, offset: u64) -> Result<String, Failure> {
    let length = src
        .size()
        .saturating_sub(offset)
        .min(u64::from(MAX_HEADER_BYTES));
    let bytes = src.read_at(offset, u32::try_from(length).unwrap_or(MAX_HEADER_BYTES))?;
    let line = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    Ok(String::from_utf8_lossy(line).into_owned())
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
