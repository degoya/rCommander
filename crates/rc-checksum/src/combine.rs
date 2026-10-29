//! Total Commander's "Combine files": the parts `<stem>.001`, `<stem>.002`, ... back into one
//! file, named and checked by `<stem>.crc` when there is one.

use std::{collections::BTreeMap, io, path::Path, sync::Arc};

use rc_vfs::{EntryKind, Vfs, VfsError, WriteMode};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::{
    Algorithm, BUFFER_SIZE, ChecksumError, Control, MAX_PARTS,
    control::Pending,
    crc_file::{self, CrcInfo, MAX_CRC_FILE},
    create::absent,
    names, part_name,
    verify::read_capped,
};

/// What [`combine`] wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombineReport {
    /// The combined file's name in the target folder.
    pub target: String,
    /// The parts read, in order.
    pub parts: Vec<String>,
    pub size: u64,
    /// The combined file's CRC32, lowercase hex.
    pub crc32: String,
    /// Whether a `.crc` file was there and its CRC32 matched; `false` when there was none.
    pub crc_checked: bool,
}

/// Combines the split file that `first` belongs to - its `.001` part (any part will do) or its
/// `.crc` file - into `target_dir` on `target_vfs`. The name is `target_name`, else the one
/// the `.crc` file holds, else the parts' stem. Every part from `.001` on must be there, and
/// with a `.crc` file their sizes must add up to its size, before anything is written. The
/// target is created new; a failed or cancelled run, or a CRC that does not match, removes it.
///
/// # Errors
///
/// `combine_not_a_split_file`, `combine_part_missing`, `combine_size_mismatch`,
/// `combine_crc_mismatch`, `split_crc_file_invalid`, `invalid_name`, `already_exists`, `io` when
/// the listed sizes overflow, a read or write failed, or the run was cancelled.
pub async fn combine(
    source_vfs: &Arc<dyn Vfs>,
    first: &Path,
    target_vfs: &Arc<dyn Vfs>,
    target_dir: &Path,
    target_name: Option<&str>,
    control: &Control,
) -> Result<CombineReport, ChecksumError> {
    let not_split = || ChecksumError::NotASplitFile {
        path: first.to_path_buf(),
    };
    let name = first
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(not_split)?;
    let stem = split_stem(&name).ok_or_else(not_split)?.to_owned();
    let dir = first.parent().ok_or_else(not_split)?;
    let info = read_crc(source_vfs, &dir.join(format!("{stem}.crc"))).await?;
    let parts = find_parts(source_vfs, dir, &stem).await?;
    // A server names the sizes: ones that add up past `u64` are refused, not wrapped.
    let total = parts
        .iter()
        .try_fold(0_u64, |total, (_, size)| total.checked_add(*size))
        .ok_or_else(|| {
            ChecksumError::Vfs(VfsError::Io {
                path: dir.to_path_buf(),
                source: io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the parts' sizes add up to more than 2^64 bytes",
                ),
            })
        })?;
    if let Some(expected) = info.as_ref().and_then(|info| info.size) {
        if total < expected {
            return Err(ChecksumError::PartMissing {
                name: part_name(&stem, parts.len() as u64 + 1),
            });
        }
        if total > expected {
            return Err(ChecksumError::SizeMismatch {
                expected,
                actual: total,
            });
        }
    }
    let target = match (target_name, &info) {
        (Some(name), _) => names::plain(name)?.to_owned(),
        (None, Some(info)) => info.file_name.clone(),
        (None, None) => names::plain(&stem)?.to_owned(),
    };
    let target_path = target_dir.join(&target);
    absent(target_vfs, &target_path).await?;
    control.plan(parts.len() as u64, total);

    let mut pending = Pending::new(target_vfs);
    let result = write_target(
        source_vfs,
        dir,
        &parts,
        target_vfs,
        &target_path,
        &mut pending,
        control,
    )
    .await;
    let (size, crc32) = match result {
        Ok(done) => done,
        Err(error) => {
            pending.discard().await;
            return Err(error);
        }
    };
    if let Some(info) = &info
        && info.crc32 != crc32
    {
        pending.discard().await;
        return Err(ChecksumError::CrcMismatch {
            expected: info.crc32.clone(),
            actual: crc32,
        });
    }
    pending.keep();
    Ok(CombineReport {
        target,
        parts: parts.into_iter().map(|(name, _)| name).collect(),
        size,
        crc32,
        crc_checked: info.is_some(),
    })
}

/// The stem of a part (`disk` of `disk.001`) or of a `.crc` file (`disk` of `disk.crc`).
fn split_stem(name: &str) -> Option<&str> {
    let (stem, extension) = name.rsplit_once('.')?;
    let is_part = extension.len() == 3 && extension.bytes().all(|byte| byte.is_ascii_digit());
    (!stem.is_empty() && (is_part || extension.eq_ignore_ascii_case("crc"))).then_some(stem)
}

/// The `.crc` file at `path`, `None` when there is none.
async fn read_crc(vfs: &Arc<dyn Vfs>, path: &Path) -> Result<Option<CrcInfo>, ChecksumError> {
    let text = match read_capped(vfs, path, MAX_CRC_FILE).await {
        Ok(text) => text,
        Err(ChecksumError::Vfs(VfsError::NotFound { .. })) => return Ok(None),
        Err(ChecksumError::ChecksumFileTooLarge { .. }) => {
            return Err(ChecksumError::InvalidCrcFile {
                path: path.to_path_buf(),
                reason: "too large",
            });
        }
        Err(error) => return Err(error),
    };
    crc_file::parse(path, &text).map(Some)
}

/// The parts `<stem>.001` ... in `dir` with their sizes, in order; fails on the first gap.
async fn find_parts(
    vfs: &Arc<dyn Vfs>,
    dir: &Path,
    stem: &str,
) -> Result<Vec<(String, u64)>, ChecksumError> {
    let case_sensitive = vfs.capabilities().case_sensitive;
    let mut found = BTreeMap::new();
    for entry in vfs.list(dir).await? {
        let Some((entry_stem, number)) = entry.name.rsplit_once('.') else {
            continue;
        };
        let same = if case_sensitive {
            entry_stem == stem
        } else {
            entry_stem.eq_ignore_ascii_case(stem)
        };
        if !same || number.len() != 3 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(index) = number.parse::<u64>() else {
            continue;
        };
        if index >= 1 && matches!(entry.kind, EntryKind::File | EntryKind::Symlink { .. }) {
            found.insert(index, (entry.name, entry.size));
        }
    }
    let mut parts = Vec::with_capacity(found.len());
    for (expected, (index, part)) in (1..=MAX_PARTS).zip(found) {
        if index != expected {
            return Err(ChecksumError::PartMissing {
                name: part_name(stem, expected),
            });
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(ChecksumError::PartMissing {
            name: part_name(stem, 1),
        });
    }
    Ok(parts)
}

/// Streams every part into the new target through one buffer; its size and CRC32.
async fn write_target(
    source_vfs: &Arc<dyn Vfs>,
    dir: &Path,
    parts: &[(String, u64)],
    target_vfs: &Arc<dyn Vfs>,
    target: &Path,
    pending: &mut Pending,
    control: &Control,
) -> Result<(u64, String), ChecksumError> {
    let mut writer = target_vfs.open_write(target, WriteMode::CreateNew).await?;
    pending.push(target.to_path_buf());
    let mut crc = Algorithm::Crc32.hasher();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    let mut total = 0_u64;
    for (name, size) in parts {
        let path = dir.join(name);
        control.begin_file(&path, *size);
        let mut reader = source_vfs.open_read(&path).await?;
        loop {
            control.checkpoint().await?;
            let read = reader
                .read(&mut buffer)
                .await
                .map_err(|error| ChecksumError::io(&path, error))?;
            if read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..read])
                .await
                .map_err(|error| ChecksumError::io(target, error))?;
            crc.update(&buffer[..read]);
            total += read as u64;
            control.advance(read as u64);
        }
        control.file_done();
    }
    writer
        .shutdown()
        .await
        .map_err(|error| ChecksumError::io(target, error))?;
    Ok((total, crc.finish_hex()))
}

#[cfg(test)]
#[path = "combine_tests.rs"]
mod tests;
