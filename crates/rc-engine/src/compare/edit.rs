//! Editing two compared text files line by line (RC-100-94): Total Commander's edit mode of
//! "Compare by content", by lines rather than as a free text field (owner, 2026-11-07).
//!
//! An [`EditSession`] holds both files as text, where each came from and how it was when read,
//! and every change as a splice of lines - replace these lines of one side by those - so undo
//! and redo are the same splice backwards and forwards, up to [`UNDO_LIMIT`] steps. A side is
//! editable only when it is no archive's, at most [`EDIT_LIMIT`] bytes, and written back it
//! gives the bytes it was read from ([`text::encode`] of [`text::decode`]); else it stays
//! read-only with the reason ([`ReadOnly`]). The diff itself is not kept here: the window
//! computes it again after every change ([`EditSession::generation`]), and a block is copied
//! only along a diff of the text as it is now.

use std::{collections::VecDeque, ops::Range, time::SystemTime};

use rc_vfs::{Entry, VfsPath};
use serde::{Deserialize, Serialize};

mod rows;

use self::rows::next_line;
pub use self::rows::{blocks, rows_at};
use super::text::{self, DiffRow, Line, LineEnding, TextDiff, TextFile, Unmappable};

/// The largest file, in bytes, the edit mode changes.
pub const EDIT_LIMIT: u64 = 4 * 1024 * 1024;

/// The most changes undo goes back.
pub const UNDO_LIMIT: usize = 1000;

/// One of the two compared files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareSide {
    Left,
    Right,
}

impl CompareSide {
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// This side's line of `row`.
    #[must_use]
    pub fn line_of(self, row: &DiffRow) -> Option<usize> {
        match self {
            Self::Left => row.left,
            Self::Right => row.right,
        }
    }
}

/// Which way lines are copied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Total Commander's "Copy ->".
    #[default]
    LeftToRight,
    /// "Copy <-".
    RightToLeft,
}

impl Direction {
    /// The side the lines come from and the side they replace lines of.
    #[must_use]
    pub fn sides(self) -> (CompareSide, CompareSide) {
        match self {
            Self::LeftToRight => (CompareSide::Left, CompareSide::Right),
            Self::RightToLeft => (CompareSide::Right, CompareSide::Left),
        }
    }
}

/// Why a side is not edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadOnly {
    /// Inside an archive, which is never written.
    Archive,
    /// Larger than [`EDIT_LIMIT`].
    TooLarge,
    /// Written back it would not give the bytes it was read from (a stray byte, a broken
    /// character): editing would change more than the lines edited.
    NotLossless,
}

/// Where a side's file is and how it was when it was read or last saved: size and time, which a
/// save compares before it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub path: VfsPath,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl Origin {
    #[must_use]
    pub fn of(path: VfsPath, entry: &Entry) -> Self {
        Self {
            path,
            size: entry.size,
            modified: entry.modified,
        }
    }

    /// The file is still as it was: the same size and time.
    #[must_use]
    pub fn unchanged(&self, entry: &Entry) -> bool {
        self.size == entry.size && self.modified == entry.modified
    }
}

/// A side as read: its text, where from, and why it is read-only, if it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideFile {
    pub file: TextFile,
    pub origin: Origin,
    pub read_only: Option<ReadOnly>,
}

/// One side of a session.
#[derive(Debug, Clone)]
struct Edited {
    file: TextFile,
    origin: Origin,
    read_only: Option<ReadOnly>,
    /// The ending of new and copied lines: the one most of the original's lines have (Total
    /// Commander's rule).
    majority: LineEnding,
    /// Changes applied to this side minus those undone: where in its history it is.
    depth: usize,
    /// The depth it was read or saved at; `None` once that state was undone and overwritten.
    saved: Option<usize>,
}

/// One change: `removed` lines of `side` from `at` replaced by `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Splice {
    side: CompareSide,
    at: usize,
    removed: Vec<Line>,
    inserted: Vec<Line>,
}

/// Two text files being edited in the compare window or by `compare.apply_blocks`.
#[derive(Debug, Clone)]
pub struct EditSession {
    left: Edited,
    right: Edited,
    undo: VecDeque<Splice>,
    redo: Vec<Splice>,
    generation: u64,
}

impl EditSession {
    #[must_use]
    pub fn new(left: SideFile, right: SideFile) -> Self {
        let edited = |side: SideFile| Edited {
            majority: majority(&side.file.lines),
            file: side.file,
            origin: side.origin,
            read_only: side.read_only,
            depth: 0,
            saved: Some(0),
        };
        Self {
            left: edited(left),
            right: edited(right),
            undo: VecDeque::new(),
            redo: Vec::new(),
            generation: 0,
        }
    }

    fn side(&self, side: CompareSide) -> &Edited {
        match side {
            CompareSide::Left => &self.left,
            CompareSide::Right => &self.right,
        }
    }

    fn side_mut(&mut self, side: CompareSide) -> &mut Edited {
        match side {
            CompareSide::Left => &mut self.left,
            CompareSide::Right => &mut self.right,
        }
    }

    #[must_use]
    pub fn file(&self, side: CompareSide) -> &TextFile {
        &self.side(side).file
    }

    #[must_use]
    pub fn origin(&self, side: CompareSide) -> &Origin {
        &self.side(side).origin
    }

    #[must_use]
    pub fn read_only(&self, side: CompareSide) -> Option<ReadOnly> {
        self.side(side).read_only
    }

    #[must_use]
    pub fn editable(&self, side: CompareSide) -> bool {
        self.read_only(side).is_none()
    }

    /// The side differs from the file as read or last saved.
    #[must_use]
    pub fn dirty(&self, side: CompareSide) -> bool {
        let edited = self.side(side);
        edited.saved != Some(edited.depth)
    }

    #[must_use]
    pub fn any_dirty(&self) -> bool {
        self.dirty(CompareSide::Left) || self.dirty(CompareSide::Right)
    }

    /// Counts the changes: a diff computed at another generation is of other text.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Both files as they are now, for the diff.
    #[must_use]
    pub fn files(&self) -> (TextFile, TextFile) {
        (self.left.file.clone(), self.right.file.clone())
    }

    /// `diff` is of the text as it is now.
    #[must_use]
    pub fn current(&self, diff: &TextDiff) -> bool {
        diff.left == self.left.file && diff.right == self.right.file
    }

    /// Line `index` of `side` now reads `text`; its ending stays.
    pub fn set_line(&mut self, side: CompareSide, index: usize, text: String) -> bool {
        let Some(old) = self.file(side).lines.get(index) else {
            return false;
        };
        if old.text == text {
            return false;
        }
        let line = Line {
            text,
            ending: old.ending,
        };
        self.change(side, index, 1, vec![line])
    }

    /// A new line `text` before line `at` of `side` (at the end when `at` is the line count).
    pub fn insert_line(&mut self, side: CompareSide, at: usize, text: String) -> bool {
        let ending = self.side(side).majority;
        self.change(side, at, 0, vec![Line { text, ending }])
    }

    /// Line `index` of `side` removed.
    pub fn delete_line(&mut self, side: CompareSide, index: usize) -> bool {
        index < self.file(side).lines.len() && self.change(side, index, 1, Vec::new())
    }

    /// The lines of `rows` of `diff` copied in `direction`, replacing the other side's lines of
    /// those rows (Total Commander's "Copy ->" and "Copy <-"); `false` when `diff` is not of
    /// the text as it is now, the target is read-only or nothing would change.
    pub fn copy_rows(&mut self, diff: &TextDiff, rows: Range<usize>, direction: Direction) -> bool {
        self.copy_blocks(diff, &[rows], direction) > 0
    }

    /// [`EditSession::copy_rows`] for several ranges of rows of one diff, each one change of its
    /// own; answers how many changed something. The ranges are applied from the last, so the
    /// line numbers of the earlier ones still hold.
    pub fn copy_blocks(
        &mut self,
        diff: &TextDiff,
        blocks: &[Range<usize>],
        direction: Direction,
    ) -> usize {
        let (from, to) = direction.sides();
        if !self.editable(to) || !self.current(diff) {
            return 0;
        }
        let mut sorted: Vec<Range<usize>> = blocks
            .iter()
            .filter(|rows| rows.start < rows.end && rows.end <= diff.rows.len())
            .cloned()
            .collect();
        sorted.sort_by_key(|rows| std::cmp::Reverse(rows.start));
        sorted.dedup();
        let majority = self.side(to).majority;
        let mut changed = 0;
        for rows in sorted {
            let span = &diff.rows[rows.clone()];
            let lines: Vec<Line> = span
                .iter()
                .filter_map(|row| from.line_of(row))
                .filter_map(|index| self.file(from).lines.get(index))
                .map(|line| Line {
                    text: line.text.clone(),
                    ending: majority,
                })
                .collect();
            let targets: Vec<usize> = span.iter().filter_map(|row| to.line_of(row)).collect();
            let at = match targets.first() {
                Some(&first) => first,
                None => next_line(diff, rows.start, to),
            };
            if self.change(to, at, targets.len(), lines) {
                changed += 1;
            }
        }
        changed
    }

    /// Takes back the last change; `false` when there is none.
    pub fn undo(&mut self) -> bool {
        let Some(splice) = self.undo.pop_back() else {
            return false;
        };
        let inverse = Splice {
            side: splice.side,
            at: splice.at,
            removed: splice.inserted.clone(),
            inserted: splice.removed.clone(),
        };
        self.apply(&inverse);
        let edited = self.side_mut(splice.side);
        edited.depth = edited.depth.saturating_sub(1);
        self.redo.push(splice);
        self.generation += 1;
        true
    }

    /// Makes the last change taken back again.
    pub fn redo(&mut self) -> bool {
        let Some(splice) = self.redo.pop() else {
            return false;
        };
        self.apply(&splice);
        self.side_mut(splice.side).depth += 1;
        self.undo.push_back(splice);
        self.generation += 1;
        true
    }

    /// `side` as bytes in its encoding, its endings and its mark.
    ///
    /// # Errors
    ///
    /// A line holds a character the encoding cannot write.
    pub fn encoded(&self, side: CompareSide) -> Result<Vec<u8>, Unmappable> {
        text::encode(self.file(side))
    }

    /// `side` was written: it is as on disk now, at `origin`.
    pub fn saved(&mut self, side: CompareSide, origin: Origin) {
        let edited = self.side_mut(side);
        edited.origin = origin;
        edited.saved = Some(edited.depth);
    }

    /// Replaces `remove` lines of `side` from `at` by `inserted`, as one change that undo takes
    /// back. The file keeps how it ends: lines inside it always end, and its last line ends as
    /// the last one did.
    fn change(
        &mut self,
        side: CompareSide,
        mut at: usize,
        mut remove: usize,
        mut inserted: Vec<Line>,
    ) -> bool {
        let edited = self.side(side);
        let lines = &edited.file.lines;
        let len = lines.len();
        if !self.editable(side) || at > len || remove > len - at {
            return false;
        }
        let last_ending = lines.last().map_or(edited.majority, |line| line.ending);
        let majority = edited.majority;
        let reaches_end = at + remove == len;
        if reaches_end && at > 0 {
            let before = &lines[at - 1];
            // The line before becomes a line inside the file, or the last one.
            let ending = if inserted.is_empty() {
                last_ending
            } else if before.ending == LineEnding::None {
                majority
            } else {
                before.ending
            };
            if ending != before.ending {
                inserted.insert(
                    0,
                    Line {
                        text: before.text.clone(),
                        ending,
                    },
                );
                at -= 1;
                remove += 1;
            }
        }
        if reaches_end && let Some(last) = inserted.last_mut() {
            last.ending = last_ending;
        }
        let removed = lines[at..at + remove].to_vec();
        if removed == inserted {
            return false;
        }
        let splice = Splice {
            side,
            at,
            removed,
            inserted,
        };
        self.apply(&splice);
        // A saved state the redo list held is gone with it.
        self.redo.clear();
        for edited in [&mut self.left, &mut self.right] {
            if edited.saved.is_some_and(|saved| saved > edited.depth) {
                edited.saved = None;
            }
        }
        self.side_mut(side).depth += 1;
        self.undo.push_back(splice);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.generation += 1;
        true
    }

    fn apply(&mut self, splice: &Splice) {
        let lines = &mut self.side_mut(splice.side).file.lines;
        let end = (splice.at + splice.removed.len()).min(lines.len());
        lines.splice(splice.at..end, splice.inserted.iter().cloned());
    }
}

/// The ending most lines have; a file without any gets the platform's.
fn majority(lines: &[Line]) -> LineEnding {
    let count = |ending: LineEnding| lines.iter().filter(|line| line.ending == ending).count();
    let platform = if cfg!(windows) {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    };
    [LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr]
        .into_iter()
        .map(|ending| (count(ending), ending))
        .filter(|(count, _)| *count > 0)
        .max_by_key(|(count, _)| *count)
        .map_or(platform, |(_, ending)| ending)
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
