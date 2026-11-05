//! The directories of a copy or a move (`copy.rs`): each made on the target, merged into one of
//! the same name, or - moving within one backend - renamed with its contents; at the end, deepest
//! first, their times, and when moving the emptied sources.
//!
//! A link to a folder the plan walked into (`ops.copy_links = contents`, RC-080-63) is such a
//! directory too, with two differences: a move renames the link itself, or - copied instead -
//! removes only the link at the source, and only when nothing below it failed or was skipped;
//! what lies below it is copied, never renamed or removed.

use std::path::{Path, PathBuf};

use rc_vfs::{Entry, VfsError};

use crate::{
    Failure, OpsError,
    copy::falls_back,
    links::Mark,
    runner::{Runner, Step},
    walk::Item,
};

/// How a directory of the plan was handled.
pub(crate) enum DirOutcome {
    /// Made on the target.
    Created,
    /// One of the same name was there; the contents go into it.
    Merged,
    /// Moved in one rename, contents and all.
    Renamed,
}

/// A directory made or merged.
pub(crate) struct DirDone {
    pub(crate) from: PathBuf,
    pub(crate) to: PathBuf,
    /// Made here, not merged into.
    pub(crate) made: bool,
    /// The source's entry.
    pub(crate) entry: Entry,
    pub(crate) mark: Mark,
    /// Of a link walked into: everything below it arrived, so a move may remove the link.
    pub(crate) whole: bool,
}

/// A link walked into whose contents are still being copied: where its run of the plan ends,
/// its place among the directories done, and the job's skips and failures when it began.
pub(crate) struct OpenLink {
    pub(crate) end: usize,
    pub(crate) dir: usize,
    pub(crate) counts: (u64, usize),
}

/// The index after the last item below the item at `at` of the pre-order plan `items`.
pub(crate) fn subtree_end(items: &[Item], at: usize) -> usize {
    let rel = &items[at].rel;
    at + 1
        + items[at + 1..]
            .iter()
            .take_while(|item| item.rel.starts_with(rel))
            .count()
}

impl Runner {
    /// The job's skips and failures so far.
    pub(crate) fn counts(&self) -> (u64, usize) {
        self.job
            .update(|snapshot| (snapshot.skipped, snapshot.failures.len()))
    }

    /// Closes the links of `open` whose run of the plan ends at or before `at`: each one is
    /// whole when nothing was skipped or failed meanwhile.
    pub(crate) fn close_links(&self, open: &mut Vec<OpenLink>, dirs: &mut [DirDone], at: usize) {
        while let Some(link) = open.pop_if(|link| link.end <= at) {
            let whole = self.counts() == link.counts;
            if let Some(dir) = dirs.get_mut(link.dir) {
                dir.whole = whole;
            }
        }
    }

    /// Deepest first: the times of the directories made, and when moving the emptied sources.
    /// A source directory that still holds something skipped stays; of a link walked into only
    /// the link goes, once all below it arrived; what lies below one stays.
    pub(crate) async fn finish_dirs(&mut self, dirs: &[DirDone], moving: bool) {
        let set_mtime = self.dst.capabilities().set_mtime;
        for dir in dirs.iter().rev() {
            if dir.made
                && set_mtime
                && self.options.keep_attributes
                && let Some(modified) = dir.entry.modified
            {
                let _ = self.dst.set_mtime(&dir.to, modified).await;
            }
            if !moving || dir.mark.below {
                continue;
            }
            let removed = if dir.mark.link {
                if !dir.whole {
                    continue;
                }
                self.src.remove_file(&dir.from).await
            } else {
                self.src.remove_dir(&dir.from, false).await
            };
            match removed {
                Ok(()) | Err(VfsError::DirectoryNotEmpty { .. } | VfsError::NotFound { .. }) => {}
                Err(error) => {
                    let failure = Failure::new(&self.src_path(&dir.from), &error.into());
                    self.job.update(|snapshot| snapshot.failures.push(failure));
                }
            }
        }
    }

    /// Makes or merges the directory `item` at `to`; moving within one backend with `may_rename`
    /// it is renamed there, contents and all.
    pub(crate) async fn transfer_dir(
        &mut self,
        item: &Item,
        to: &Path,
        may_rename: bool,
    ) -> Result<DirOutcome, Step> {
        match self.dst.stat(to).await {
            Ok(existing) if existing.is_dir_like() => Ok(DirOutcome::Merged),
            Ok(_) => Err(OpsError::KindMismatch {
                path: to.to_path_buf(),
            }
            .into()),
            Err(VfsError::NotFound { .. }) => {
                if self.rename_ok && may_rename {
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
}
