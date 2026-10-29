//! Where a copied file lands (RC-060-05): on a server it is written under a part name beside its
//! target and renamed once it is complete, so a cancel, a failure or a lost connection never
//! leaves a half file under the target's name - only `<name>.rcpart`. An overwritten file on a
//! server stays as it was until the new one is complete. A download from a server is written
//! the same way onto a local disk, and so is a copy from disk to disk (RC-080-04): a process
//! killed in the middle leaves its part, never a half file under the target's name, and the job
//! continues it after the restart. A file out of an archive, copied or unpacked, goes through a
//! part on a disk or a server too (RC-080-37): a damaged entry never costs the file it would
//! have replaced. Such a copy is staged but not resumable - it starts over and is not kept across
//! a restart. Into other backends (the tests' memory) a file is written in place and removed when
//! it does not complete.
//!
//! A lost connection keeps the part and the copy continues it (`resume.rs`, RC-060-07); a cancel
//! or another failure removes it, and a part that cannot be removed is logged. A part found where
//! a new copy's target is free is a conflict of its own - "resume" or "overwrite" - and a job put
//! back after a restart continues the part of the file it was writing without asking.
//!
//! What the file keeps of its source: the modification time, and the Unix mode where the source
//! has one and the target can hold one (never on Windows); else only the read-only flag. The
//! job's `keep_attributes` turns all of it off.

use std::{path::Path, time::Duration};

use rc_vfs::{BackendId, Entry, SetAttributes, Vfs, VfsError, WriteMode, WriteStream};

use crate::{
    resume::Placement,
    runner::{Runner, Step},
    temp_name::part_path,
    walk::Item,
};

/// Whether `backend` is a server: a transfer there goes through a part file and waits in the
/// lane of the connection.
pub(crate) fn is_remote(backend: &BackendId) -> bool {
    matches!(backend, BackendId::Sftp { .. } | BackendId::Ftp { .. })
}

/// Whether a file written onto `dst` goes through a part file: onto a disk of this machine or a
/// server, from any source (RC-080-37).
pub(crate) fn is_staged(dst: &BackendId) -> bool {
    *dst == BackendId::Local || is_remote(dst)
}

/// Whether a copy from `src` to `dst` continues its part file and is kept across a restart: to
/// or from a server, and from disk to disk on this machine (RC-080-04). Out of an archive it
/// starts over.
pub(crate) fn is_resumable(src: &BackendId, dst: &BackendId) -> bool {
    is_remote(src) || is_remote(dst) || (*src == BackendId::Local && *dst == BackendId::Local)
}

/// Removes what a file that did not complete left at `path`: its part, or the file written in
/// place. Nothing there is fine; a refusal is logged with the path, as the file's outcome is
/// decided already.
pub(crate) async fn remove_leftover(vfs: &dyn Vfs, path: &Path) {
    match vfs.remove_file(path).await {
        Ok(()) | Err(VfsError::NotFound { .. }) => {}
        Err(error) => tracing::warn!(
            path = %path.display(),
            code = error.code(),
            "a file left by an incomplete copy could not be removed: {error}"
        ),
    }
}

/// What a file's conflict check comes to: where it is written, skipped by an answer, or - a
/// restored job - found complete at its target already.
pub(crate) enum Resolved {
    Write(Placement),
    Skip,
    Complete,
}

/// Opens the part file `part` to be written from its start: created anew, never truncated - a
/// part left over is removed first, so a link a server put under its name is removed, not
/// written through (RC-080-12).
pub(crate) async fn open_part(vfs: &dyn Vfs, part: &Path) -> Result<WriteStream, VfsError> {
    match vfs.open_write(part, WriteMode::CreateNew).await {
        Err(VfsError::AlreadyExists { .. }) => {
            vfs.remove_file(part).await?;
            vfs.open_write(part, WriteMode::CreateNew).await
        }
        opened => opened,
    }
}

/// How far apart two modification times may be and still count as one: a server keeps whole
/// seconds, FAT two.
const SAME_TIME: Duration = Duration::from_secs(2);

impl Runner {
    /// The part file an interrupted copy left where the free target `dest` would be: a file, on a
    /// transfer that resumes; `None` otherwise.
    pub(crate) async fn leftover_part(&self, dest: &Path) -> Result<Option<Entry>, Step> {
        if !self.resumable {
            return Ok(None);
        }
        match self.dst.stat(&part_path(dest)).await {
            Ok(part) if !part.is_dir_like() => Ok(Some(part)),
            Ok(_) | Err(VfsError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// A restored job meeting the part of a file it was writing before the restart: continued
    /// while the source is unchanged ([`crate::FileMark`]), else started over - in both cases
    /// without asking, as the job had decided. `None` for every other file.
    pub(crate) async fn restored_placement(
        &self,
        item: &Item,
        dest: &Path,
    ) -> Result<Option<Placement>, Step> {
        if !self.restored || self.leftover_part(dest).await?.is_none() {
            return Ok(None);
        }
        let mark = self
            .mark
            .as_ref()
            .filter(|mark| mark.is_of(&item.path, dest));
        if mark.is_none() {
            // An unmarked file (a small one): continued where its target is still free.
            return match self.dst.stat(dest).await {
                Err(VfsError::NotFound { .. }) => Ok(Some(Placement {
                    dest: dest.to_path_buf(),
                    mode: WriteMode::CreateNew,
                    resume: true,
                })),
                Ok(_) => Ok(None),
                Err(error) => Err(error.into()),
            };
        }
        let replace = mark.is_some_and(|mark| mark.replace);
        Ok(Some(Placement {
            dest: dest.to_path_buf(),
            mode: if replace {
                WriteMode::Truncate
            } else {
                WriteMode::CreateNew
            },
            resume: mark.is_some_and(|mark| mark.unchanged(&item.entry)),
        }))
    }

    /// Whether a restored job finds `item` already copied: a file of the source's size, and of
    /// its modification time where the job keeps times. Counted as done, never asked about.
    pub(crate) fn already_complete(&self, item: &Item, existing: &Entry) -> bool {
        if existing.is_dir_like() || existing.size != item.entry.size {
            return false;
        }
        if !self.options.keep_attributes {
            return true;
        }
        match (existing.modified, item.entry.modified) {
            (Some(a), Some(b)) => {
                let apart = a.duration_since(b).or_else(|_| b.duration_since(a));
                apart.is_ok_and(|apart| apart < SAME_TIME)
            }
            _ => true,
        }
    }

    /// Gives the complete part file its target's name. A server's rename does not replace an
    /// entry (SFTP version 3), so an overwrite removes the old target first; a file that took the
    /// free name meanwhile fails the copy with `already_exists`. The local rename replaces what it
    /// finds, so onto a free name there it is the rename that refuses (`rename_no_replace`,
    /// RC-080-56). A refusal names the target as the side that failed.
    pub(crate) async fn settle(
        &mut self,
        part: &Path,
        dest: &Path,
        mode: WriteMode,
    ) -> Result<(), Step> {
        let local = self.dst_id == Some(BackendId::Local);
        let renamed = match mode {
            WriteMode::CreateNew if local => self.dst.rename_no_replace(part, dest).await,
            _ => match self.dst.rename(part, dest).await {
                Err(VfsError::AlreadyExists { .. }) if mode == WriteMode::Truncate => {
                    match self.dst.remove_file(dest).await {
                        Ok(()) => self.dst.rename(part, dest).await,
                        Err(error) => Err(error),
                    }
                }
                renamed => renamed,
            },
        };
        renamed.map_err(|error| self.target_refused(error, dest))
    }

    /// Carries the modification time and the attributes over, where the target can hold them
    /// and the job keeps them. A refusal costs nothing but the attribute.
    pub(crate) async fn preserve(&self, entry: &Entry, dest: &Path) {
        if !self.options.keep_attributes {
            return;
        }
        let capabilities = self.dst.capabilities();
        if capabilities.set_mtime
            && let Some(modified) = entry.modified
        {
            let _ = self.dst.set_mtime(dest, modified).await;
        }
        let mode = entry.attributes.mode.filter(|_| capabilities.unix_mode);
        let readonly = (mode.is_none() && entry.attributes.readonly).then_some(true);
        if mode.is_some() || readonly.is_some() {
            let change = SetAttributes { mode, readonly };
            let _ = self.dst.set_attributes(dest, change).await;
        }
    }
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
