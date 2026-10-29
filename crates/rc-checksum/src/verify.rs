//! Checking a checksum file: every line's file hashed again and compared, with one result per
//! line. The file is untrusted: its size is capped and a name that would leave its folder is
//! reported as invalid, never opened.

use std::{path::Path, sync::Arc};

use rc_vfs::{EntryKind, TargetKind, Vfs, VfsError};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt as _;

use crate::{
    Algorithm, ChecksumError, Control, MAX_CHECKSUM_FILE, hash_file, names, parse_checksums,
};

/// How one line came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyStatus {
    /// The file's digest is the one written.
    Ok,
    /// The file's digest differs: changed or damaged.
    Mismatch,
    /// No file of that name.
    Missing,
    /// The file is there but cannot be read (`code` says why).
    Unreadable,
    /// The line is in no known format, or its name leaves the folder (`code` says which).
    Invalid,
}

/// One line's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyEntry {
    /// The line's number in the checksum file, from 1.
    pub line: usize,
    /// The name as the line writes it; empty for a line in no known format.
    pub name: String,
    pub status: VerifyStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<Algorithm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// The digest the file has now, for `ok` and `mismatch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// Why a line is `unreadable` (a file-system code) or `invalid` (`unknown_format`,
    /// `absolute_name`, `parent_reference`, `forbidden_character`, `empty_name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// How many lines came out how.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifySummary {
    pub ok: usize,
    pub mismatch: usize,
    pub missing: usize,
    pub unreadable: usize,
    pub invalid: usize,
}

impl VerifySummary {
    /// Whether every line checked out.
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.mismatch + self.missing + self.unreadable + self.invalid == 0
    }
}

/// What [`verify`] found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub entries: Vec<VerifyEntry>,
    pub summary: VerifySummary,
}

/// Checks every line of the checksum file at `path` against the files it names, relative to
/// the file's folder. The format is read from the content (`sha256sum`, BSD tags, SFV); the
/// extension only tells SHA-256 from BLAKE3.
///
/// # Errors
///
/// The checksum file cannot be read, is larger than [`MAX_CHECKSUM_FILE`], has no line in a
/// known format (`checksum_format_unknown`), or the run was cancelled.
pub async fn verify(
    vfs: &Arc<dyn Vfs>,
    path: &Path,
    control: &Control,
) -> Result<VerifyReport, ChecksumError> {
    let text = read_capped(vfs, path, MAX_CHECKSUM_FILE).await?;
    let hint = path
        .extension()
        .and_then(|extension| Algorithm::from_extension(&extension.to_string_lossy()));
    let lines = parse_checksums(&text, hint);
    if !lines.iter().any(|(_, line)| line.is_some()) {
        return Err(ChecksumError::UnknownFormat {
            path: path.to_path_buf(),
        });
    }
    control.plan(lines.len() as u64, 0);
    let base = path.parent().unwrap_or(path);
    let mut entries = Vec::with_capacity(lines.len());
    let mut summary = VerifySummary::default();
    for (number, line) in lines {
        let mut entry = VerifyEntry {
            line: number,
            name: String::new(),
            status: VerifyStatus::Invalid,
            algorithm: None,
            expected: None,
            actual: None,
            code: Some("unknown_format".to_owned()),
        };
        if let Some(line) = line {
            entry.name = line.name;
            entry.algorithm = Some(line.algorithm);
            entry.expected = Some(line.hex);
            entry.code = None;
            check(vfs, base, &mut entry, control).await?;
        }
        match entry.status {
            VerifyStatus::Ok => summary.ok += 1,
            VerifyStatus::Mismatch => summary.mismatch += 1,
            VerifyStatus::Missing => summary.missing += 1,
            VerifyStatus::Unreadable => summary.unreadable += 1,
            VerifyStatus::Invalid => summary.invalid += 1,
        }
        entries.push(entry);
        control.file_done();
    }
    Ok(VerifyReport { entries, summary })
}

/// Hashes the file of one parsed line and sets its status; only a cancel is an error.
async fn check(
    vfs: &Arc<dyn Vfs>,
    base: &Path,
    entry: &mut VerifyEntry,
    control: &Control,
) -> Result<(), ChecksumError> {
    let (Some(algorithm), Some(expected)) = (entry.algorithm, entry.expected.clone()) else {
        return Ok(());
    };
    let path = match names::inside(base, &entry.name) {
        Ok(path) => path,
        Err(reason) => {
            entry.status = VerifyStatus::Invalid;
            entry.code = Some(reason.to_owned());
            return Ok(());
        }
    };
    let unreadable = |entry: &mut VerifyEntry, code: &str| {
        entry.status = VerifyStatus::Unreadable;
        entry.code = Some(code.to_owned());
    };
    let size = match vfs.stat(&path).await {
        Err(VfsError::NotFound { .. }) => {
            entry.status = VerifyStatus::Missing;
            return Ok(());
        }
        Err(error) => {
            unreadable(entry, error.code());
            return Ok(());
        }
        Ok(found) => match found.kind {
            EntryKind::File
            | EntryKind::Symlink {
                target_kind: Some(TargetKind::File),
                ..
            } => found.size,
            EntryKind::Dir => {
                unreadable(entry, "is_a_directory");
                return Ok(());
            }
            _ => {
                unreadable(entry, "checksum_not_a_file");
                return Ok(());
            }
        },
    };
    control.begin_file(&path, size);
    match hash_file(vfs, &path, algorithm, control).await {
        Ok(actual) => {
            entry.status = if actual == expected {
                VerifyStatus::Ok
            } else {
                VerifyStatus::Mismatch
            };
            entry.actual = Some(actual);
            Ok(())
        }
        Err(ChecksumError::Cancelled) => Err(ChecksumError::Cancelled),
        Err(error) => {
            unreadable(entry, error.code());
            Ok(())
        }
    }
}

/// The text of the small file at `path`, refused above `limit` bytes; read through `take`, so a
/// file that grows after the `stat` cannot exceed the limit either. Invalid UTF-8 is replaced.
pub(crate) async fn read_capped(
    vfs: &Arc<dyn Vfs>,
    path: &Path,
    limit: u64,
) -> Result<String, ChecksumError> {
    let size = vfs.stat(path).await?.size;
    let too_large = |size| ChecksumError::ChecksumFileTooLarge {
        path: path.to_path_buf(),
        size,
    };
    if size > limit {
        return Err(too_large(size));
    }
    let mut bytes = Vec::new();
    vfs.open_read(path)
        .await?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|source| ChecksumError::io(path, source))?;
    if bytes.len() as u64 > limit {
        return Err(too_large(bytes.len() as u64));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
#[path = "verify_tests.rs"]
mod tests;
