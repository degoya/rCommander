//! Total Commander's "Split file": parts `<stem>.001`, `<stem>.002`, ... of a chosen size and a
//! `<stem>.crc` with the original name, size and CRC32, so [`crate::combine`] can restore and
//! check it.

use std::{path::Path, sync::Arc};

use rc_vfs::{EntryKind, ReadStream, TargetKind, Vfs, VfsError, WriteMode};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::{
    Algorithm, BUFFER_SIZE, ChecksumError, Control, Hasher,
    control::Pending,
    crc_file,
    create::{absent, write_new},
    names,
};

/// Parts are numbered with three digits, as Total Commander does.
pub const MAX_PARTS: u64 = 999;

/// What [`split`] wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitReport {
    /// The parts, in order.
    pub parts: Vec<String>,
    /// The `.crc` file.
    pub crc_file: String,
    /// The bytes split.
    pub size: u64,
    /// The CRC32 of the whole file, lowercase hex.
    pub crc32: String,
}

/// The name of part `index` (from 1) of a file whose name without extension is `stem`.
#[must_use]
pub fn part_name(stem: &str, index: u64) -> String {
    format!("{stem}.{index:03}")
}

/// The name without its last extension (`disk` of `disk.iso`); the whole name when that would
/// leave nothing (`.bashrc`).
pub(crate) fn stem_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// Splits the file at `source` on `source_vfs` into parts of `part_size` bytes (the last one
/// shorter) in `target_dir` on `target_vfs`, and writes the `.crc` file. Every part and the
/// `.crc` file are created new; one that exists stops the run before anything is written. A
/// failed or cancelled run removes every part it wrote.
///
/// # Errors
///
/// `split_part_size_invalid`, `split_too_many_parts`, `already_exists`, `invalid_name` for a
/// source whose name breaks the one name rule ([`crate::plain_name`]), the source is no file, a
/// read or write failed, or the run was cancelled.
pub async fn split(
    source_vfs: &Arc<dyn Vfs>,
    source: &Path,
    target_vfs: &Arc<dyn Vfs>,
    target_dir: &Path,
    part_size: u64,
    control: &Control,
) -> Result<SplitReport, ChecksumError> {
    if part_size == 0 {
        return Err(ChecksumError::InvalidPartSize);
    }
    let entry = source_vfs.stat(source).await?;
    match entry.kind {
        EntryKind::File
        | EntryKind::Symlink {
            target_kind: Some(TargetKind::File),
            ..
        } => {}
        EntryKind::Dir => {
            return Err(VfsError::IsADirectory {
                path: source.to_path_buf(),
            }
            .into());
        }
        _ => {
            return Err(ChecksumError::NotAFile {
                path: source.to_path_buf(),
            });
        }
    }
    let parts = entry.size.div_ceil(part_size).max(1);
    if parts > MAX_PARTS {
        return Err(ChecksumError::TooManyParts {
            parts,
            max: MAX_PARTS,
        });
    }
    // A listed `a:b.iso` would split into NTFS streams, `con.iso` into a device (RC-080-43).
    names::plain(&entry.name)?;
    let stem = stem_of(&entry.name).to_owned();
    let crc_name = names::plain(&format!("{stem}.crc"))?.to_owned();
    for index in 1..=parts {
        absent(target_vfs, &target_dir.join(part_name(&stem, index))).await?;
    }
    absent(target_vfs, &target_dir.join(&crc_name)).await?;
    control.plan(parts, entry.size);
    control.begin_file(source, entry.size);

    let mut pending = Pending::new(target_vfs);
    let run = Run {
        source_vfs,
        source,
        target_vfs,
        target_dir,
        part_size,
        stem: &stem,
        control,
    };
    match run.write_parts(&mut pending).await {
        Ok((written, size, crc32)) => {
            let text = crc_file::write(&entry.name, size, &crc32);
            if let Err(error) = write_new(
                target_vfs,
                &mut pending,
                &target_dir.join(&crc_name),
                text.as_bytes(),
            )
            .await
            {
                pending.discard().await;
                return Err(error);
            }
            pending.keep();
            Ok(SplitReport {
                parts: written,
                crc_file: crc_name,
                size,
                crc32,
            })
        }
        Err(error) => {
            pending.discard().await;
            Err(error)
        }
    }
}

struct Run<'a> {
    source_vfs: &'a Arc<dyn Vfs>,
    source: &'a Path,
    target_vfs: &'a Arc<dyn Vfs>,
    target_dir: &'a Path,
    part_size: u64,
    stem: &'a str,
    control: &'a Control,
}

impl Run<'_> {
    /// Streams the source into the parts through one buffer; the parts written, the bytes and
    /// the CRC32. A part is only started when bytes are left for it (or for the first, empty
    /// one of an empty file), so a size that is a multiple of the part size ends without an
    /// empty part even when the file changed since the `stat`.
    async fn write_parts(
        &self,
        pending: &mut Pending,
    ) -> Result<(Vec<String>, u64, String), ChecksumError> {
        let mut source = Source {
            reader: self.source_vfs.open_read(self.source).await?,
            buffer: vec![0_u8; BUFFER_SIZE],
            start: 0,
            end: 0,
            eof: false,
            crc: Algorithm::Crc32.hasher(),
            total: 0,
        };
        let mut written: Vec<String> = Vec::new();
        loop {
            source.refill(self.source, self.control).await?;
            if source.exhausted() && !written.is_empty() {
                return Ok((written, source.total, source.crc.finish_hex()));
            }
            let index = written.len() as u64 + 1;
            if index > MAX_PARTS {
                return Err(ChecksumError::TooManyParts {
                    parts: index,
                    max: MAX_PARTS,
                });
            }
            let name = part_name(self.stem, index);
            let path = self.target_dir.join(&name);
            let mut writer = self
                .target_vfs
                .open_write(&path, WriteMode::CreateNew)
                .await?;
            pending.push(path.clone());
            let mut left = self.part_size;
            while left > 0 {
                source.refill(self.source, self.control).await?;
                if source.exhausted() {
                    break;
                }
                let chunk = &source.buffer[source.start..source.end];
                let take = usize::try_from(left).map_or(chunk.len(), |left| left.min(chunk.len()));
                writer
                    .write_all(&chunk[..take])
                    .await
                    .map_err(|error| ChecksumError::io(&path, error))?;
                source.start += take;
                left -= take as u64;
            }
            writer
                .shutdown()
                .await
                .map_err(|error| ChecksumError::io(&path, error))?;
            self.control.file_done();
            written.push(name);
        }
    }
}

/// The source file behind its one buffer: `buffer[start..end]` is read and not yet written.
struct Source {
    reader: ReadStream,
    buffer: Vec<u8>,
    start: usize,
    end: usize,
    eof: bool,
    crc: Hasher,
    total: u64,
}

impl Source {
    /// Reads the next buffer once the current one is written; counts it into the CRC.
    async fn refill(&mut self, path: &Path, control: &Control) -> Result<(), ChecksumError> {
        if self.start < self.end || self.eof {
            return Ok(());
        }
        control.checkpoint().await?;
        let read = self
            .reader
            .read(&mut self.buffer)
            .await
            .map_err(|error| ChecksumError::io(path, error))?;
        self.start = 0;
        self.end = read;
        self.eof = read == 0;
        self.crc.update(&self.buffer[..read]);
        self.total += read as u64;
        control.advance(read as u64);
        Ok(())
    }

    /// Everything read and written.
    fn exhausted(&self) -> bool {
        self.eof && self.start == self.end
    }
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod tests;
