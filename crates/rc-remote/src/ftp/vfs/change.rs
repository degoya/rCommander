//! Changing the tree: uploads, `MKD`, `RNFR`/`RNTO`, `DELE`, `RMD`, `MFMT`, `SITE CHMOD`.
//!
//! Each refusal is explained by looking at the path and its parent, so `550` becomes
//! `already_exists`, `not_found`, `not_a_directory`, `is_a_directory` or `directory_not_empty`
//! where that is what happened.

use std::{path::Path, time::SystemTime};

use rc_vfs::{EntryKind, Result, SetAttributes, VfsError, WriteMode, WriteStream};

use super::{FtpVfs, not_found};
use crate::{
    ftp::{error::Failure, stream::Upload, time::format_stamp},
    tree::{self, TreeOps},
};

impl FtpVfs {
    pub(super) async fn open_upload(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        let wire = self.wire_path(path)?;
        // FTP has no exclusive create; checking first is the closest there is.
        if mode == WriteMode::CreateNew && self.probe(path).await?.is_some() {
            return Err(VfsError::AlreadyExists {
                path: path.to_path_buf(),
            });
        }
        let mut lease = self.transfer_lease(path).await?;
        let opened = match mode {
            WriteMode::Append => lease.appe(&wire).await,
            WriteMode::CreateNew | WriteMode::Truncate => lease.stor(&wire).await,
        };
        match opened {
            Ok(transfer) => Ok(Box::new(Upload::new(transfer, lease))),
            Err(failure) => {
                drop(lease);
                Err(self.explain_create(path, failure, false).await)
            }
        }
    }

    pub(super) async fn mkdir_one(&self, path: &Path) -> Result<()> {
        let wire = self.wire_path(path)?;
        let mut lease = self.lease(path).await?;
        let Err(failure) = lease.mkdir(&wire).await else {
            return Ok(());
        };
        drop(lease);
        Err(self.explain_create(path, failure, true).await)
    }

    /// Creates the missing directories from the top down ([`tree::mkdir_all`]).
    pub(super) async fn mkdir_all(&self, path: &Path) -> Result<()> {
        tree::mkdir_all(self, path).await
    }

    /// Why creating `path` (a file, or a directory with `dir`) failed.
    async fn explain_create(&self, path: &Path, failure: Failure, dir: bool) -> VfsError {
        match self.probe(path).await {
            Ok(Some(_)) if dir => {
                return VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                };
            }
            Ok(Some(entry)) if entry.is_dir_like() => {
                return VfsError::IsADirectory {
                    path: path.to_path_buf(),
                };
            }
            _ => {}
        }
        if let Some(parent) = path.parent() {
            match self.probe(parent).await {
                Ok(None) => return not_found(parent),
                Ok(Some(entry)) if !entry.is_dir_like() => {
                    return VfsError::NotADirectory {
                        path: parent.to_path_buf(),
                    };
                }
                _ => {}
            }
        }
        failure.into_vfs(path)
    }

    pub(super) async fn rename_entry(&self, from: &Path, to: &Path) -> Result<()> {
        let (wire_from, wire_to) = (self.wire_path(from)?, self.wire_path(to)?);
        let mut lease = self.lease(from).await?;
        let Err(failure) = lease.rename(&wire_from, &wire_to).await else {
            return Ok(());
        };
        drop(lease);
        if let Ok(None) = self.probe(from).await {
            return Err(not_found(from));
        }
        if let Some(parent) = to.parent()
            && let Ok(None) = self.probe(parent).await
        {
            return Err(not_found(parent));
        }
        Err(failure.into_vfs(from))
    }

    pub(super) async fn remove_one_file(&self, path: &Path) -> Result<()> {
        let wire = self.wire_path(path)?;
        let mut lease = self.lease(path).await?;
        let Err(failure) = lease.remove(&wire).await else {
            return Ok(());
        };
        drop(lease);
        Err(match self.probe(path).await {
            Ok(None) => not_found(path),
            Ok(Some(entry)) if entry.kind == EntryKind::Dir => VfsError::IsADirectory {
                path: path.to_path_buf(),
            },
            Ok(Some(_)) | Err(_) => failure.into_vfs(path),
        })
    }

    pub(super) async fn remove_empty_dir(&self, path: &Path) -> Result<()> {
        let wire = self.wire_path(path)?;
        let mut lease = self.lease(path).await?;
        let Err(failure) = lease.rmdir(&wire).await else {
            return Ok(());
        };
        drop(lease);
        Err(match self.probe(path).await {
            Ok(None) => not_found(path),
            Ok(Some(entry)) if !entry.is_dir_like() => VfsError::NotADirectory {
                path: path.to_path_buf(),
            },
            Ok(Some(_)) => match self.list_dir(path).await {
                Ok(entries) if !entries.is_empty() => VfsError::DirectoryNotEmpty {
                    path: path.to_path_buf(),
                },
                _ => failure.into_vfs(path),
            },
            Err(_) => failure.into_vfs(path),
        })
    }

    /// Removes a directory and everything below it ([`tree::remove_tree`]).
    pub(super) async fn remove_tree(&self, dir: &Path) -> Result<()> {
        tree::remove_tree(self, dir).await
    }

    pub(super) async fn set_modified(&self, path: &Path, modified: SystemTime) -> Result<()> {
        let unsupported = |operation| VfsError::Unsupported {
            path: path.to_path_buf(),
            operation,
        };
        if !self.pool.info.mfmt {
            return Err(unsupported("setting times on a server without MFMT"));
        }
        let stamp = format_stamp(modified).ok_or_else(|| unsupported("a time before 1970"))?;
        let wire = self.wire_path(path)?;
        let mut lease = self.lease(path).await?;
        let Err(failure) = lease.mfmt(&stamp, &wire).await else {
            return Ok(());
        };
        drop(lease);
        Err(self.explain_missing(path, failure).await)
    }

    pub(super) async fn change_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        if change.readonly.is_some() {
            return Err(VfsError::Unsupported {
                path: path.to_path_buf(),
                operation: "the read-only flag over FTP",
            });
        }
        let Some(mode) = change.mode else {
            return Ok(());
        };
        let wire = self.wire_path(path)?;
        let mut lease = self.lease(path).await?;
        let Err(failure) = lease.chmod(mode & 0o7777, &wire).await else {
            return Ok(());
        };
        drop(lease);
        Err(self.explain_missing(path, failure).await)
    }

    /// `not_found` when `path` is missing, what the server said otherwise.
    async fn explain_missing(&self, path: &Path, failure: Failure) -> VfsError {
        match self.probe(path).await {
            Ok(None) => not_found(path),
            Ok(Some(_)) | Err(_) => failure.into_vfs(path),
        }
    }
}

impl TreeOps for FtpVfs {
    async fn kind_of(&self, path: &Path) -> Result<Option<bool>> {
        Ok(self.probe(path).await?.map(|entry| entry.is_dir_like()))
    }

    async fn make_one_dir(&self, path: &Path) -> Result<()> {
        self.mkdir_one(path).await
    }

    async fn entries_of(&self, dir: &Path) -> Result<Vec<(String, bool)>> {
        let entries = self.list_dir(dir).await?;
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
