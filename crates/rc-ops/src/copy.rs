//! Copy and move: every entry streamed from the source backend into the target backend.
//!
//! A file is `open_read` → a few buffers → `open_write`, both streams at once
//! (`stream_pair.rs`), with a pause and cancel check before each buffer; nothing in here knows
//! where either backend keeps its files. A file whose copy
//! does not complete - cancelled, failed, or not verifying - is removed from the target. On a
//! server and on this machine's disks the file is written under a part name and renamed when
//! complete (`staging.rs`), also out of an archive (RC-080-37); into other backends it is
//! written in place.
//!
//! A copy to or from a server goes through `<name>.rcpart` on the target, also onto a local disk -
//! a download that broke off must leave its part to be continued - and survives a lost
//! connection (`resume.rs`, RC-060-07). A copy from disk to disk goes the same way (RC-080-04,
//! unlike Total Commander, which writes in place): a process killed in the middle must not leave
//! a half file under the target's name, and an overwritten file stays whole until its new
//! contents are.
//!
//! Moving within one backend renames (a whole directory at once where its target is free) and
//! falls back to copying when the backend reports another device. Otherwise each copied file's
//! size is checked on the target - and against the source where the stream ended early - and only
//! then is its source removed, and each source directory once it is empty. A restored move
//! removes the source of a file it finds complete at its target. Modification times and
//! attributes are kept where the target can hold them (`staging.rs`).

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{BackendId, Entry, EntryKind, TargetKind, Vfs, VfsError, VfsPath, WriteMode};

use crate::{
    Failure, OpsError,
    name::{check_target, keeps_windows_rules},
    runner::{Runner, Step, Stop, attempt},
    staging::{Resolved, is_resumable, is_staged, open_part, remove_leftover},
    temp_name::part_path,
    walk::{Item, Subtrees},
};

/// How a directory of the plan was handled.
enum DirOutcome {
    /// Made on the target.
    Created,
    /// One of the same name was there; the contents go into it.
    Merged,
    /// Moved in one rename, contents and all.
    Renamed,
}

/// A directory made or merged: source, target, whether made here, the source's entry.
type DirDone = (PathBuf, PathBuf, bool, Entry);

/// How a file reached its target.
#[derive(PartialEq, Eq)]
pub(crate) enum Moved {
    Copied,
    Renamed,
    /// The free name was taken since the conflict check; nothing moved (RC-080-56).
    Taken,
}

/// Whether a failed rename means "not this way" rather than "not at all".
fn falls_back(error: &VfsError) -> bool {
    match error {
        VfsError::Io { source, .. } => source.kind() == io::ErrorKind::CrossesDevices,
        VfsError::Unsupported { .. } => true,
        _ => false,
    }
}

/// Whether a stream can copy the entry: a file, or a link to one (its contents are copied).
fn copyable(entry: &Entry) -> bool {
    matches!(
        entry.kind,
        EntryKind::File
            | EntryKind::Symlink {
                target_kind: Some(TargetKind::File),
                ..
            }
    )
}

impl Runner {
    /// Whether `item` is a link its listing did not tell the target of (FTP lists links that way
    /// by default, RC-080-46) that leads to a file: then its contents are copied, as a resolved
    /// one's are.
    async fn link_to_a_file(&self, item: &Item) -> bool {
        let unresolved = matches!(
            item.entry.kind,
            EntryKind::Symlink {
                target_kind: None,
                ..
            }
        );
        unresolved
            && self
                .src
                .stat(&item.path)
                .await
                .is_ok_and(|entry| copyable(&entry))
    }
}

/// Gives the one source of `items` the name `name`: its own path below the target, and the
/// first part of every path below it.
fn rename_top(items: &mut [Item], name: &Path) {
    for item in items {
        let mut parts = item.rel.components();
        parts.next();
        let rest = parts.as_path();
        // `join("")` would add a trailing separator.
        item.rel = if rest.as_os_str().is_empty() {
            name.to_path_buf()
        } else {
            name.join(rest)
        };
    }
}

/// Makes sure the directory `path` exists, with its parents.
pub(crate) async fn ensure_dir(vfs: &dyn Vfs, path: &Path) -> Result<(), Step> {
    match vfs.stat(path).await {
        Ok(entry) if entry.is_dir_like() => Ok(()),
        Ok(_) => Err(VfsError::NotADirectory {
            path: path.to_path_buf(),
        }
        .into()),
        Err(VfsError::NotFound { .. }) => Ok(vfs.mkdir(path, true).await?),
        Err(error) => Err(error.into()),
    }
}

impl Runner {
    /// Copies (or moves) every source into the directory `target`.
    pub(crate) async fn transfer(
        &mut self,
        sources: &[VfsPath],
        target: &VfsPath,
        moving: bool,
    ) -> Result<(), Stop> {
        self.transfer_named(sources, target, moving, None).await
    }

    /// Copies (or moves) the one entry `source` to `target`, the path it gets: a copy under
    /// another name, a rename.
    pub(crate) async fn transfer_as(
        &mut self,
        source: &VfsPath,
        target: &VfsPath,
        moving: bool,
    ) -> Result<(), Stop> {
        // The queue refused a target without a parent or a name.
        let (Some(dir), Some(name)) = (target.path.parent(), target.path.file_name()) else {
            return Ok(());
        };
        let dir = VfsPath {
            backend: target.backend.clone(),
            path: dir.to_path_buf(),
        };
        let name = PathBuf::from(name);
        self.transfer_named(std::slice::from_ref(source), &dir, moving, Some(&name))
            .await
    }

    /// Copies (or moves) every source into the directory `target`; with `name`, the one source
    /// arrives under that name.
    async fn transfer_named(
        &mut self,
        sources: &[VfsPath],
        target: &VfsPath,
        moving: bool,
        name: Option<&Path>,
    ) -> Result<(), Stop> {
        self.rename_ok =
            moving && self.src_id == target.backend && self.src.capabilities().rename_across_dirs;
        self.moving = moving;
        self.staged = is_staged(&target.backend);
        self.resumable = is_resumable(&self.src_id, &target.backend);
        self.dst_id = Some(target.backend.clone());
        let dst = Arc::clone(&self.dst);
        if attempt!(self, target, ensure_dir(dst.as_ref(), &target.path).await).is_none() {
            return Ok(());
        }
        let mut items = self.plan(sources).await?;
        if let Some(name) = name {
            rename_top(&mut items, name);
        }
        let on_target = |path: &Path| VfsPath {
            backend: target.backend.clone(),
            path: path.to_path_buf(),
        };
        // Subtrees done in one step (renamed) or given up (skipped): their items are counted.
        let mut subtrees = Subtrees::new(&items);
        // Directories made or merged, in plan order.
        let mut dirs: Vec<DirDone> = Vec::new();
        // Names out of an archive are judged as unpacking judges them, everywhere (RC-080-12).
        let strict = keeps_windows_rules(&target.backend)
            || matches!(self.src_id, BackendId::Archive { .. });
        for (at, item) in items.iter().enumerate() {
            if subtrees.skipped(at) {
                continue;
            }
            self.checkpoint().await?;
            let to = target.path.join(&item.rel);
            // On a local Windows folder every name keeps Windows' rules, the sources' own too: a
            // listed `a:b` is no stream there, `con` no device (RC-080-12).
            let named = || check_target(&item.rel, strict).map_err(Step::from);
            if attempt!(self, &on_target(&to), named()).is_none() {
                let (files, bytes) = subtrees.give_up(at);
                self.count_done(files, bytes);
                continue;
            }
            if item.entry.kind == EntryKind::Dir {
                match attempt!(self, &on_target(&to), self.transfer_dir(item, &to).await) {
                    Some(DirOutcome::Created) => {
                        dirs.push((item.path.clone(), to, true, item.entry.clone()));
                    }
                    Some(DirOutcome::Merged) => {
                        dirs.push((item.path.clone(), to, false, item.entry.clone()));
                    }
                    Some(DirOutcome::Renamed) | None => {
                        let (files, bytes) = subtrees.give_up(at);
                        self.count_done(files, bytes);
                    }
                }
            } else if copyable(&item.entry) || self.link_to_a_file(item).await {
                self.transfer_file(item, &on_target(&to), moving).await?;
            } else {
                let failure = Failure::new(
                    &self.src_path(&item.path),
                    &OpsError::NotCopyable {
                        path: item.path.clone(),
                    },
                );
                self.job.update(|snapshot| snapshot.failures.push(failure));
                self.count_done(1, item.entry.size);
            }
        }
        self.finish_dirs(&dirs, moving).await;
        Ok(())
    }

    /// Deepest first: the times of the directories made, and when moving the emptied sources.
    /// A source directory that still holds something skipped stays.
    async fn finish_dirs(&mut self, dirs: &[DirDone], moving: bool) {
        let set_mtime = self.dst.capabilities().set_mtime;
        for (from, to, made, entry) in dirs.iter().rev() {
            if *made
                && set_mtime
                && self.options.keep_attributes
                && let Some(modified) = entry.modified
            {
                let _ = self.dst.set_mtime(to, modified).await;
            }
            if moving {
                match self.src.remove_dir(from, false).await {
                    Ok(())
                    | Err(VfsError::DirectoryNotEmpty { .. } | VfsError::NotFound { .. }) => {}
                    Err(error) => {
                        let failure = Failure::new(&self.src_path(from), &error.into());
                        self.job.update(|snapshot| snapshot.failures.push(failure));
                    }
                }
            }
        }
    }

    async fn transfer_dir(&mut self, item: &Item, to: &Path) -> Result<DirOutcome, Step> {
        match self.dst.stat(to).await {
            Ok(existing) if existing.is_dir_like() => Ok(DirOutcome::Merged),
            Ok(_) => Err(OpsError::KindMismatch {
                path: to.to_path_buf(),
            }
            .into()),
            Err(VfsError::NotFound { .. }) => {
                if self.rename_ok {
                    match self.src.rename_no_replace(&item.path, to).await {
                        Ok(()) => return Ok(DirOutcome::Renamed),
                        // A folder that took the name meanwhile is merged into, as one found
                        // first; never replaced (RC-080-56).
                        Err(VfsError::AlreadyExists { .. }) => {
                            return match self.dst.stat(to).await {
                                Ok(existing) if existing.is_dir_like() => Ok(DirOutcome::Merged),
                                Ok(_) => Err(OpsError::KindMismatch {
                                    path: to.to_path_buf(),
                                }
                                .into()),
                                Err(error) => Err(error.into()),
                            };
                        }
                        Err(error) if falls_back(&error) => self.rename_ok = false,
                        Err(error) => return Err(error.into()),
                    }
                }
                self.dst.mkdir(to, false).await?;
                Ok(DirOutcome::Created)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// One file: the conflict, the copy, and when moving the source's removal.
    async fn transfer_file(
        &mut self,
        item: &Item,
        target: &VfsPath,
        moving: bool,
    ) -> Result<(), Stop> {
        let source = self.src_path(&item.path);
        let size = item.entry.size;
        let mut asked_again = false;
        let moved = loop {
            let resolved = attempt!(self, target, self.resolve(item, &source, target).await);
            let placement = match resolved {
                Some(Resolved::Write(placement)) => placement,
                Some(Resolved::Skip) => {
                    self.count_skipped(size);
                    return Ok(());
                }
                // A restored job's file is there already: a move still owes its source's removal.
                Some(Resolved::Complete) if moving => {
                    self.remove_source(item, &source).await?;
                    self.count_done(1, size);
                    return Ok(());
                }
                Some(Resolved::Complete) => {
                    self.count_skipped(size);
                    return Ok(());
                }
                None => {
                    self.count_done(1, size);
                    return Ok(());
                }
            };
            self.progress.begin_file(source.clone(), size);
            let start = self.progress.bytes_done;
            let moved = attempt!(self, &source, {
                self.progress.restart_file(start);
                // Judged again right before it is opened, a restored job's file too (RC-080-42).
                match self.admit(&item.path).await {
                    Err(refused) => Err(refused.into()),
                    Ok(()) if self.resumable => self.copy_resuming(item, &placement).await,
                    Ok(()) => self.copy_one(item, &placement.dest, placement.mode).await,
                }
            });
            self.progress.bytes_done = start;
            match moved {
                // A file took the free name since the conflict check: asked about again, once.
                Some(Moved::Taken) if !asked_again => asked_again = true,
                Some(Moved::Taken) => {
                    let taken = OpsError::Vfs(VfsError::AlreadyExists {
                        path: placement.dest.clone(),
                    });
                    let failure = Failure::new(&source, &taken);
                    self.job.update(|snapshot| snapshot.failures.push(failure));
                    break None;
                }
                moved => break moved,
            }
        };
        if moved == Some(Moved::Renamed) {
            // Renamed in place: nothing streamed, and the file is whole at its target.
            self.progress.file_bytes_done = size;
        }
        if moving && moved == Some(Moved::Copied) {
            self.remove_source(item, &source).await?;
        }
        self.count_done(1, size);
        Ok(())
    }

    /// Removes a moved file's source; a refusal is a failure of the file.
    async fn remove_source(&mut self, item: &Item, source: &VfsPath) -> Result<(), Stop> {
        let src = Arc::clone(&self.src);
        let _removed = attempt!(
            self,
            source,
            src.remove_file(&item.path).await.map_err(Step::from)
        );
        Ok(())
    }

    /// Renames (moving within one backend) or streams the file to `dest`; whatever is left of
    /// a copy that did not complete is removed.
    pub(crate) async fn copy_one(
        &mut self,
        item: &Item,
        dest: &Path,
        mode: WriteMode,
    ) -> Result<Moved, Step> {
        if self.rename_ok {
            let renamed = if mode == WriteMode::CreateNew {
                self.src.rename_no_replace(&item.path, dest).await
            } else {
                self.src.rename(&item.path, dest).await
            };
            match renamed {
                Ok(()) => return Ok(Moved::Renamed),
                Err(VfsError::AlreadyExists { .. }) if mode == WriteMode::CreateNew => {
                    return Ok(Moved::Taken);
                }
                // A backend whose rename does not replace: the target goes first.
                Err(VfsError::AlreadyExists { .. }) if mode == WriteMode::Truncate => {
                    self.dst.remove_file(dest).await?;
                    self.src.rename(&item.path, dest).await?;
                    return Ok(Moved::Renamed);
                }
                Err(error) if falls_back(&error) => self.rename_ok = false,
                Err(error) => return Err(error.into()),
            }
        }
        self.failed_on = None;
        // Staged, the copy is written under a part name, replaced when it is left over.
        let part = self.staged.then(|| part_path(dest));
        let written = part.as_deref().unwrap_or(dest);
        let pair = self.one_ftp_pair().await?;
        let reader = self.src.open_read(&item.path).await?;
        let opened = match &part {
            Some(part) => open_part(self.dst.as_ref(), part).await,
            None => self.dst.open_write(written, mode).await,
        };
        let writer = opened.map_err(|error| self.target_refused(error, written))?;
        let hasher = blake3::Hasher::new();
        let landed = self
            .land(item, (reader, writer), pair, written, hasher, 0)
            .await;
        let landed = match (landed, &part) {
            (Ok(()), Some(part)) => self.settle(part, dest, mode).await,
            (other, _) => other,
        };
        if let Err(step) = landed {
            remove_leftover(self.dst.as_ref(), written).await;
            return Err(step);
        }
        self.preserve(&item.entry, dest).await;
        Ok(Moved::Copied)
    }
}
