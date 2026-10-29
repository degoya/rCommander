//! Delete and mkdir.
//!
//! Into the recycle bin, each source goes in one call - the bin takes a folder whole, and it
//! can be restored whole. Permanently, the plan is walked backwards: files first, each
//! directory once it is empty; a directory still holding a skipped entry is left standing
//! without a further question. Links are removed, never followed.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{EntryKind, Vfs, VfsError, VfsPath};

use crate::{
    DeleteMode, Failure, JobState, OpsError,
    runner::{Runner, Step, Stop, attempt},
    walk::Subtrees,
};

/// Creates the directory `path`, refusing one that exists.
async fn make_dir(vfs: &dyn Vfs, path: &Path) -> Result<(), Step> {
    match vfs.stat(path).await {
        Ok(_) => Err(VfsError::AlreadyExists {
            path: path.to_path_buf(),
        }
        .into()),
        Err(VfsError::NotFound { .. }) => Ok(vfs.mkdir(path, true).await?),
        Err(error) => Err(error.into()),
    }
}

impl Runner {
    pub(crate) async fn delete(
        &mut self,
        sources: &[VfsPath],
        mode: DeleteMode,
    ) -> Result<(), Stop> {
        let items = self.plan(sources).await?;
        let src = Arc::clone(&self.src);
        match (mode, self.trash.clone()) {
            (DeleteMode::Trash, Some(trash)) => {
                let mut subtrees = Subtrees::new(&items);
                let tops = items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.rel.components().count() == 1);
                for (at, item) in tops {
                    self.checkpoint().await?;
                    let path = self.src_path(&item.path);
                    self.progress.current = Some(path.clone());
                    let _trashed = attempt!(
                        self,
                        &path,
                        trash.trash(&item.path).await.map_err(Step::from)
                    );
                    let (files, bytes) = subtrees.tally(at);
                    self.count_done(files, bytes);
                }
            }
            (DeleteMode::Trash, None) => {
                // Refused at submit; kept for a backend registered again without a bin.
                if let Some(source) = sources.first() {
                    let error = OpsError::TrashUnavailable {
                        path: source.path.clone(),
                    };
                    return Err(Stop::Failed(Failure::new(source, &error)));
                }
            }
            (DeleteMode::Permanent, _) => {
                // Entries (and their ancestors) that stay because something was skipped.
                let mut kept: Vec<PathBuf> = Vec::new();
                for item in items.iter().rev() {
                    self.checkpoint().await?;
                    let at = self.src_path(&item.path);
                    if item.entry.kind == EntryKind::Dir {
                        if kept.iter().any(|rel| rel.starts_with(&item.rel)) {
                            continue;
                        }
                        let removed = attempt!(
                            self,
                            &at,
                            src.remove_dir(&item.path, false).await.map_err(Step::from)
                        );
                        if removed.is_none() {
                            kept.push(item.rel.clone());
                        }
                        continue;
                    }
                    self.progress.current = Some(at.clone());
                    let removed = attempt!(
                        self,
                        &at,
                        src.remove_file(&item.path).await.map_err(Step::from)
                    );
                    if removed.is_none() {
                        kept.push(item.rel.clone());
                    }
                    self.count_done(1, item.entry.size);
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn mkdir(&mut self, path: &VfsPath) -> Result<(), Stop> {
        self.job
            .update(|snapshot| snapshot.state = JobState::Running);
        self.progress.files_total = 1;
        self.progress.current = Some(path.clone());
        let vfs = Arc::clone(&self.dst);
        let _made = attempt!(self, path, make_dir(vfs.as_ref(), &path.path).await);
        self.count_done(1, 0);
        Ok(())
    }
}
