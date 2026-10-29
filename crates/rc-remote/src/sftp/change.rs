//! Changing the tree: uploads, `MKDIR`, `RENAME`, `REMOVE`, `RMDIR` and `SETSTAT`.
//!
//! Version 3 answers "already exists", "is a directory" and "not empty" with a bare failure, so
//! each refusal is explained by looking at the path and its parent, as FTP does.

use std::{io::SeekFrom, path::Path, time::SystemTime};

use rc_vfs::{EntryKind, Result, SetAttributes, VfsError, WriteMode, WriteStream};
use russh_sftp::{
    client::error::Error,
    protocol::{FileAttributes, OpenFlags},
};
use tokio::io::AsyncSeekExt;

use super::{SftpVfs, error::is_vague, path::wire_path};
use crate::tree::{self, TreeOps};

fn not_found(path: &Path) -> VfsError {
    VfsError::NotFound {
        path: path.to_path_buf(),
    }
}

impl SftpVfs {
    pub(super) async fn open_upload(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        let wire = wire_path(path)?;
        let flags = OpenFlags::WRITE
            | OpenFlags::CREATE
            | match mode {
                WriteMode::CreateNew => OpenFlags::EXCLUDE,
                WriteMode::Truncate => OpenFlags::TRUNCATE,
                WriteMode::Append => OpenFlags::APPEND,
            };
        let mut file = match self.open_for_writing(wire, flags).await {
            Ok(file) => file,
            Err(error) => {
                let exclusive = mode == WriteMode::CreateNew;
                return Err(self.explain_create(path, error, exclusive).await);
            }
        };
        if mode == WriteMode::Append {
            // Writes go to explicit offsets; a server that ignores APPEND still continues here.
            file.seek(SeekFrom::End(0))
                .await
                .map_err(|error| VfsError::from_io(error, path))?;
        }
        Ok(Box::new(self.transfer_file(file, path)))
    }

    pub(super) async fn mkdir_one(&self, path: &Path) -> Result<()> {
        let wire = wire_path(path)?;
        match self.raw.mkdir(wire, FileAttributes::empty()).await {
            Ok(_) => Ok(()),
            Err(error) => Err(self.explain_create(path, error, true).await),
        }
    }

    /// Creates the missing directories from the top down ([`tree::mkdir_all`]).
    pub(super) async fn mkdir_all(&self, path: &Path) -> Result<()> {
        tree::mkdir_all(self, path).await
    }

    /// Why creating `path` failed; `exclusive` when an existing entry is the conflict (a new
    /// directory, `CreateNew`). Only a vague refusal is looked into.
    async fn explain_create(&self, path: &Path, error: Error, exclusive: bool) -> VfsError {
        if !is_vague(&error) {
            return self.fail(error, path);
        }
        match self.probe_link(path).await {
            Ok(Some(_)) if exclusive => {
                return VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                };
            }
            Ok(Some(attrs)) if attrs.is_dir() => {
                return VfsError::IsADirectory {
                    path: path.to_path_buf(),
                };
            }
            _ => {}
        }
        if let Some(parent) = path.parent() {
            match self.probe(parent).await {
                Ok(None) => return not_found(parent),
                Ok(Some(attrs)) if !attrs.is_dir() => {
                    return VfsError::NotADirectory {
                        path: parent.to_path_buf(),
                    };
                }
                _ => {}
            }
        }
        self.fail(error, path)
    }

    pub(super) async fn rename_entry(&self, from: &Path, to: &Path) -> Result<()> {
        let (wire_from, wire_to) = (wire_path(from)?, wire_path(to)?);
        let Err(error) = self.raw.rename(wire_from, wire_to).await else {
            return Ok(());
        };
        if let Ok(None) = self.probe_link(from).await {
            return Err(not_found(from));
        }
        if let Some(parent) = to.parent()
            && let Ok(None) = self.probe(parent).await
        {
            return Err(not_found(parent));
        }
        if is_vague(&error)
            && let Ok(Some(_)) = self.probe_link(to).await
        {
            return Err(VfsError::AlreadyExists {
                path: to.to_path_buf(),
            });
        }
        Err(self.fail(error, from))
    }

    pub(super) async fn remove_one_file(&self, path: &Path) -> Result<()> {
        let wire = wire_path(path)?;
        let Err(error) = self.raw.remove(wire).await else {
            return Ok(());
        };
        Err(match self.probe_link(path).await {
            Ok(None) => not_found(path),
            Ok(Some(attrs)) if attrs.is_dir() => VfsError::IsADirectory {
                path: path.to_path_buf(),
            },
            Ok(Some(_)) | Err(_) => self.fail(error, path),
        })
    }

    pub(super) async fn remove_empty_dir(&self, path: &Path) -> Result<()> {
        let wire = wire_path(path)?;
        let Err(error) = self.raw.rmdir(wire).await else {
            return Ok(());
        };
        Err(match self.probe_link(path).await {
            Ok(None) => not_found(path),
            Ok(Some(attrs)) if !attrs.is_dir() => VfsError::NotADirectory {
                path: path.to_path_buf(),
            },
            Ok(Some(_)) => match self.list_dir(path, false).await {
                Ok(entries) if !entries.is_empty() => VfsError::DirectoryNotEmpty {
                    path: path.to_path_buf(),
                },
                _ => self.fail(error, path),
            },
            Err(_) => self.fail(error, path),
        })
    }

    /// Removes a directory and everything below it ([`tree::remove_tree`]).
    pub(super) async fn remove_tree(&self, dir: &Path) -> Result<()> {
        tree::remove_tree(self, dir).await
    }

    /// Sets the modification time; the access time stays (version 3 sets both at once).
    pub(super) async fn set_modified(&self, path: &Path, modified: SystemTime) -> Result<()> {
        let mtime = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .and_then(|since| u32::try_from(since.as_secs()).ok())
            .ok_or_else(|| VfsError::Unsupported {
                path: path.to_path_buf(),
                operation: "a time before 1970 or after 2106 over SFTP",
            })?;
        let current = self.probe(path).await?.ok_or_else(|| not_found(path))?;
        let change = FileAttributes {
            atime: Some(current.atime.unwrap_or(mtime)),
            mtime: Some(mtime),
            ..FileAttributes::empty()
        };
        self.set_stat(path, change).await
    }

    /// A new mode, or the owner's write bit on or off for read-only.
    pub(super) async fn change_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        let mut mode = change.mode.map(|bits| bits & 0o7777);
        if let Some(readonly) = change.readonly {
            let base = match mode {
                Some(bits) => bits,
                None => {
                    let current = self.probe(path).await?.ok_or_else(|| not_found(path))?;
                    current.permissions.unwrap_or(0o644) & 0o7777
                }
            };
            mode = Some(if readonly {
                base & !0o200
            } else {
                base | 0o200
            });
        }
        let Some(mode) = mode else {
            return Ok(());
        };
        let change = FileAttributes {
            permissions: Some(mode),
            ..FileAttributes::empty()
        };
        self.set_stat(path, change).await
    }

    async fn set_stat(&self, path: &Path, change: FileAttributes) -> Result<()> {
        let wire = wire_path(path)?;
        let Err(error) = self.raw.setstat(wire, change).await else {
            return Ok(());
        };
        Err(match self.probe_link(path).await {
            Ok(None) => not_found(path),
            Ok(Some(_)) | Err(_) => self.fail(error, path),
        })
    }
}

impl TreeOps for SftpVfs {
    async fn kind_of(&self, path: &Path) -> Result<Option<bool>> {
        Ok(self.probe(path).await?.map(|attrs| attrs.is_dir()))
    }

    async fn make_one_dir(&self, path: &Path) -> Result<()> {
        self.mkdir_one(path).await
    }

    async fn entries_of(&self, dir: &Path) -> Result<Vec<(String, bool)>> {
        let entries = self.list_dir(dir, false).await?;
        Ok(entries
            .into_iter()
            .map(|entry| {
                let is_dir = entry.kind == EntryKind::Dir;
                (entry.name, is_dir)
            })
            .collect())
    }

    async fn remove_one(&self, path: &Path) -> Result<()> {
        self.remove_one_file(path).await
    }

    async fn remove_empty(&self, path: &Path) -> Result<()> {
        self.remove_empty_dir(path).await
    }
}
