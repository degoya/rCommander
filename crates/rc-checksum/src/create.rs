//! Writing checksum files for a selection, as Total Commander's "Create CRC/MD5/SHA files":
//! one file per file (`foo.iso.sha256` beside `foo.iso`) or one file for all.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{EntryKind, TargetKind, Vfs, VfsError, WriteMode};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt as _;

use crate::{
    Algorithm, ChecksumError, Control,
    control::Pending,
    format::{sfv_header, sum_line},
    hash_file, names,
};

/// Where the lines go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "layout", rename_all = "snake_case")]
pub enum Layout {
    /// One checksum file beside each file, named after it (`foo.iso.sha256`), holding its line.
    PerFile,
    /// One checksum file in the folder for every file; its name `None` means `<file>.<ext>` for
    /// a single file of the folder itself and `checksums.<ext>` otherwise - also for a folder
    /// holding one file, whose line names it by its path below the folder.
    Combined {
        #[serde(default)]
        file_name: Option<String>,
    },
}

/// One file's digest; `name` relative to the folder, `/`-separated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChecksum {
    pub name: String,
    pub hex: String,
}

/// What [`create`] hashed and wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateReport {
    pub algorithm: Algorithm,
    /// Every file hashed, in the order of the lines.
    pub files: Vec<FileChecksum>,
    /// The checksum files written, relative to the folder.
    pub written: Vec<String>,
}

/// Hashes the entries `names` of `dir` - files, and the files inside folders, without following
/// links to folders - and writes their checksum files. Nothing is written until every file is
/// hashed, and no existing file is overwritten: a checksum file that exists already stops the
/// run before it reads anything.
///
/// # Errors
///
/// A name given or listed is not one entry of its folder, a checksum file's name breaks the one
/// name rule ([`crate::plain_name`]), an entry cannot be read, nothing to hash, a checksum file
/// exists already (`already_exists`), the run was cancelled.
pub async fn create(
    vfs: &Arc<dyn Vfs>,
    dir: &Path,
    names: &[String],
    algorithm: Algorithm,
    layout: &Layout,
    control: &Control,
) -> Result<CreateReport, ChecksumError> {
    let (files, sizes) = collect(vfs, dir, names).await?;
    let targets = targets(&files, algorithm, layout)?;
    for (target, _) in &targets {
        // The folders on the way exist; the file's own name is new (RC-080-43).
        names::plain(target.rsplit('/').next().unwrap_or(target))?;
        absent(vfs, &names::join(dir, target)).await?;
    }
    let bytes = sizes
        .iter()
        .fold(0_u64, |sum, size| sum.saturating_add(*size));
    control.plan(files.len() as u64, bytes);
    let mut hashed = Vec::with_capacity(files.len());
    for (name, size) in files.iter().zip(&sizes) {
        let path = names::join(dir, name);
        control.begin_file(&path, *size);
        let hex = hash_file(vfs, &path, algorithm, control).await?;
        hashed.push(FileChecksum {
            name: name.clone(),
            hex,
        });
        control.file_done();
    }
    control.check()?;
    let mut pending = Pending::new(vfs);
    let mut written = Vec::with_capacity(targets.len());
    for (target, lines) in &targets {
        let mut text = sfv_header(algorithm).to_owned();
        for (index, line_name) in lines {
            text.push_str(&sum_line(algorithm, &hashed[*index].hex, line_name));
        }
        if let Err(error) = write_new(
            vfs,
            &mut pending,
            &names::join(dir, target),
            text.as_bytes(),
        )
        .await
        {
            pending.discard().await;
            return Err(error);
        }
        written.push(target.clone());
    }
    pending.keep();
    Ok(CreateReport {
        algorithm,
        files: hashed,
        written,
    })
}

/// The files to hash, relative to `dir`, in the order given; folders expanded by name. With
/// each one's size beside it, for the progress.
async fn collect(
    vfs: &Arc<dyn Vfs>,
    dir: &Path,
    names: &[String],
) -> Result<(Vec<String>, Vec<u64>), ChecksumError> {
    let mut files = Vec::new();
    let mut sizes = Vec::new();
    for name in names {
        let name = names::listed(name)?;
        let path = dir.join(name);
        let entry = vfs.stat(&path).await?;
        match entry.kind {
            EntryKind::File
            | EntryKind::Symlink {
                target_kind: Some(TargetKind::File),
                ..
            } => {
                files.push(name.to_owned());
                sizes.push(entry.size);
            }
            EntryKind::Dir => walk(vfs, &path, name, &mut files, &mut sizes).await?,
            _ => return Err(ChecksumError::NotAFile { path }),
        }
    }
    if files.is_empty() {
        return Err(ChecksumError::NoFiles);
    }
    Ok((files, sizes))
}

/// The files below `path` (whose relative name is `prefix`), sorted by name, depth first, each
/// with its size. Links to folders and special files are skipped.
async fn walk(
    vfs: &Arc<dyn Vfs>,
    path: &Path,
    prefix: &str,
    files: &mut Vec<String>,
    sizes: &mut Vec<u64>,
) -> Result<(), ChecksumError> {
    let mut stack = vec![(path.to_path_buf(), prefix.to_owned())];
    while let Some((dir, prefix)) = stack.pop() {
        let mut entries = vfs.list(&dir).await?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut subdirs = Vec::new();
        for entry in entries {
            // A listed `..` or `a/b` would lead the lines, and the join, out of the folder.
            names::listed(&entry.name)?;
            let relative = format!("{prefix}/{}", entry.name);
            match entry.kind {
                EntryKind::File
                | EntryKind::Symlink {
                    target_kind: Some(TargetKind::File),
                    ..
                } => {
                    files.push(relative);
                    sizes.push(entry.size);
                }
                EntryKind::Dir => subdirs.push((dir.join(&entry.name), relative)),
                _ => {}
            }
        }
        // Reversed onto the stack, so the first folder by name is walked first.
        stack.extend(subdirs.into_iter().rev());
    }
    Ok(())
}

/// Each checksum file to write (relative to the folder) with its lines: the index into the
/// hashed files and the name the line carries.
type Targets = Vec<(String, Vec<(usize, String)>)>;

fn targets(
    files: &[String],
    algorithm: Algorithm,
    layout: &Layout,
) -> Result<Targets, ChecksumError> {
    let extension = algorithm.extension();
    match layout {
        Layout::PerFile => Ok(files
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let base = name.rsplit('/').next().unwrap_or(name).to_owned();
                (format!("{name}.{extension}"), vec![(index, base)])
            })
            .collect()),
        Layout::Combined { file_name } => {
            let target = match file_name {
                Some(file_name) => names::plain(file_name)?.to_owned(),
                // Beside the one file only when it lies in the folder itself: the target and
                // the lines share the folder as their base.
                None if files.len() == 1 && !files[0].contains('/') => {
                    format!("{}.{extension}", files[0])
                }
                None => format!("checksums.{extension}"),
            };
            let lines = files.iter().cloned().enumerate().collect();
            Ok(vec![(target, lines)])
        }
    }
}

/// Fails with `already_exists` when `path` exists.
pub(crate) async fn absent(vfs: &Arc<dyn Vfs>, path: &Path) -> Result<(), ChecksumError> {
    match vfs.stat(path).await {
        Ok(_) => Err(VfsError::AlreadyExists {
            path: path.to_path_buf(),
        }
        .into()),
        Err(VfsError::NotFound { .. }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Creates `path` (never over an existing file), records it in `pending` and writes `bytes`.
pub(crate) async fn write_new(
    vfs: &Arc<dyn Vfs>,
    pending: &mut Pending,
    path: &Path,
    bytes: &[u8],
) -> Result<(), ChecksumError> {
    let mut writer = vfs.open_write(path, WriteMode::CreateNew).await?;
    pending.push(PathBuf::from(path));
    writer
        .write_all(bytes)
        .await
        .map_err(|source| ChecksumError::io(path, source))?;
    writer
        .shutdown()
        .await
        .map_err(|source| ChecksumError::io(path, source))
}

#[cfg(test)]
#[path = "create_tests.rs"]
mod tests;
