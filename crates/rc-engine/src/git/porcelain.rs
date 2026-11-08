//! The reader of `git status --porcelain=v2 -z --branch` (RC-100-97): the headers
//! `# branch.oid`, `# branch.head`, `# branch.upstream` and `# branch.ab`, and the records `1`
//! (changed), `2` (renamed or copied, followed by the path it came from), `u` (unmerged), `?`
//! (untracked) and `!` (ignored), each ended by a NUL. Paths are relative to the repository's
//! root, `/`-separated, unquoted; a folder git names whole ends with `/`.
//!
//! The answer is a foreign program's about a folder anyone may have prepared: a record cut short
//! or a field missing is an error, never a panic (the fuzz target `git_porcelain`). Headers and
//! record types this reader does not know are skipped, as a newer git may add some.

use super::{GitBranch, GitMark};

/// What a record says of its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A mark the file list shows.
    Mark(GitMark),
    /// Deleted: gone from the folder, only counted.
    Deleted,
}

/// One path git reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Relative to the repository's root, `/`-separated; a folder named whole ends with `/`.
    pub path: String,
    pub change: Change,
}

/// What one run of `git status` said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub branch: GitBranch,
    pub items: Vec<Item>,
}

/// Why an answer could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PorcelainError {
    /// The answer does not end with a NUL: it was cut off.
    #[error("the answer ends inside a record")]
    Truncated,
    /// Two NULs in a row, or a record without its type.
    #[error("an empty record")]
    Empty,
    /// A record lacks a field, or a field is not what its place says.
    #[error("a record of type {kind:?} is malformed")]
    Malformed { kind: char },
    /// A rename record is not followed by the path it came from.
    #[error("a rename record lacks the path it came from")]
    NoOrigin,
}

/// The answer `bytes` read; an empty answer is a clean folder on an unborn branch.
///
/// # Errors
///
/// The answer is cut off or a record is malformed ([`PorcelainError`]).
pub fn parse(bytes: &[u8]) -> Result<Status, PorcelainError> {
    let mut status = Status::default();
    if bytes.is_empty() {
        return Ok(status);
    }
    let body = bytes.strip_suffix(&[0]).ok_or(PorcelainError::Truncated)?;
    let mut records = body.split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        let mut chars = text.chars();
        let (Some(kind), Some(' ')) = (chars.next(), chars.next()) else {
            return Err(PorcelainError::Empty);
        };
        let rest = chars.as_str();
        match kind {
            '#' => header(rest, &mut status.branch)?,
            '1' => {
                let fields = fields(rest, 8, kind)?;
                status
                    .items
                    .push(item(fields[7], changed(fields[0], kind)?)?);
            }
            '2' => {
                let fields = fields(rest, 9, kind)?;
                // The path it came from follows as a record of its own; only the new path shows.
                records.next().ok_or(PorcelainError::NoOrigin)?;
                status
                    .items
                    .push(item(fields[8], changed(fields[0], kind)?)?);
            }
            'u' => {
                let fields = fields(rest, 10, kind)?;
                status
                    .items
                    .push(item(fields[9], Change::Mark(GitMark::Conflict))?);
            }
            '?' => status
                .items
                .push(item(rest, Change::Mark(GitMark::Untracked))?),
            '!' => status
                .items
                .push(item(rest, Change::Mark(GitMark::Ignored))?),
            _ => {}
        }
    }
    Ok(status)
}

/// The `count` space-separated fields of a record's `rest`, the last one taking the remainder
/// (a path may hold spaces).
fn fields(rest: &str, count: usize, kind: char) -> Result<Vec<&str>, PorcelainError> {
    let fields: Vec<&str> = rest.splitn(count, ' ').collect();
    if fields.len() == count {
        Ok(fields)
    } else {
        Err(PorcelainError::Malformed { kind })
    }
}

fn item(path: &str, change: Change) -> Result<Item, PorcelainError> {
    if path.is_empty() {
        return Err(PorcelainError::Empty);
    }
    Ok(Item {
        path: path.to_owned(),
        change,
    })
}

/// The change of a `1` or `2` record from its `XY`: the index's state, then the folder's. A file
/// deleted in either is gone from the folder; otherwise the more important of the two marks.
fn changed(xy: &str, kind: char) -> Result<Change, PorcelainError> {
    let mut chars = xy.chars();
    let (Some(index), Some(tree), None) = (chars.next(), chars.next(), chars.next()) else {
        return Err(PorcelainError::Malformed { kind });
    };
    if tree == 'D' || (index == 'D' && tree == '.') {
        return Ok(Change::Deleted);
    }
    let staged = match index {
        'M' | 'T' => Some(GitMark::Modified),
        'A' | 'C' => Some(GitMark::Added),
        'R' => Some(GitMark::Renamed),
        'U' => Some(GitMark::Conflict),
        _ => None,
    };
    let unstaged = match tree {
        'M' | 'T' => Some(GitMark::Modified),
        'A' => Some(GitMark::Added),
        'U' => Some(GitMark::Conflict),
        _ => None,
    };
    Ok(Change::Mark(
        staged.max(unstaged).unwrap_or(GitMark::Modified),
    ))
}

/// One `# branch.*` header into `branch`; any other header is skipped.
fn header(rest: &str, branch: &mut GitBranch) -> Result<(), PorcelainError> {
    let malformed = PorcelainError::Malformed { kind: '#' };
    let (name, value) = rest.split_once(' ').unwrap_or((rest, ""));
    match name {
        "branch.oid" => {
            branch.oid = (value != "(initial)" && !value.is_empty()).then(|| value.to_owned());
        }
        "branch.head" => {
            branch.head = (value != "(detached)" && !value.is_empty()).then(|| value.to_owned());
        }
        "branch.upstream" => branch.upstream = (!value.is_empty()).then(|| value.to_owned()),
        "branch.ab" => {
            let (ahead, behind) = value.split_once(' ').ok_or(malformed)?;
            branch.ahead = count(ahead, '+').ok_or(malformed)?;
            branch.behind = count(behind, '-').ok_or(malformed)?;
        }
        _ => {}
    }
    Ok(())
}

/// `+3` or `-0` as the number after its `sign`.
fn count(text: &str, sign: char) -> Option<u32> {
    text.strip_prefix(sign)?.parse().ok()
}

#[cfg(test)]
#[path = "porcelain_tests.rs"]
mod tests;
