//! A move that only changes the case of a name (`a.txt` to `A.txt`) where case does not count:
//! the backend sees source and target as one entry, so a rename between them may do nothing and
//! a copy would meet the source as its own conflict. It goes through a free temporary name in
//! the same directory instead - `a.txt` to `.rc-tmp-...` to `A.txt` - and when the second step
//! fails the first is undone, so the entry keeps its old name. Where the undo fails too, the
//! failure names the temporary name the entry stays under (`rename_undo_failed`), and a retry
//! goes on from there (RC-080-37).

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use rc_vfs::{Vfs, VfsError, VfsPath};

use crate::{
    ErrorChoice, OpsError,
    queue::fold,
    runner::{Runner, Step, Stop, attempt},
    temp_name::case_path,
};

/// How many temporary names are tried before giving up.
const TRIES: u32 = 16;

/// Tells the temporary names of one process apart.
static NEXT: AtomicU32 = AtomicU32::new(0);

impl Runner {
    /// Whether moving `source` to `target` only changes the case of its path on a backend
    /// where case does not count.
    pub(crate) fn changes_case_only(&self, source: &VfsPath, target: &VfsPath) -> bool {
        source.backend == target.backend
            && source.backend == self.src_id
            && !self.src.capabilities().case_sensitive
            && source.path != target.path
            && fold(&source.path, false) == fold(&target.path, false)
    }

    /// Renames `source` to `target`, which differs only in case, through a temporary name.
    pub(crate) async fn rename_case(
        &mut self,
        source: &VfsPath,
        target: &VfsPath,
    ) -> Result<(), Stop> {
        let src = Arc::clone(&self.src);
        let Some(entry) = attempt!(
            self,
            source,
            src.stat(&source.path).await.map_err(Step::from)
        ) else {
            return Ok(());
        };
        self.progress.files_total = 1;
        self.progress.bytes_total = entry.size;
        self.tick(true);
        // The temporary name the entry stays under after a failed undo: a retry starts there,
        // and the failure names it.
        let mut parked: Option<PathBuf> = None;
        loop {
            let error = match two_steps(src.as_ref(), &source.path, &target.path, &mut parked).await
            {
                Ok(()) => {
                    self.count_done(1, entry.size);
                    self.tick(true);
                    return Ok(());
                }
                Err(Step::Stop(stop)) => return Err(stop),
                Err(Step::Fail(error)) => error,
            };
            let at = VfsPath {
                backend: source.backend.clone(),
                path: parked.clone().unwrap_or_else(|| source.path.clone()),
            };
            if self.failed(&at, error).await? == ErrorChoice::Skip {
                return Ok(());
            }
        }
    }
}

/// `from` to a temporary name, then to `to`; back to `from` when the second step fails. Where the
/// undo fails too, the entry stays at the temporary name, kept in `parked`, and the next call
/// goes on from there.
async fn two_steps(
    vfs: &dyn Vfs,
    from: &Path,
    to: &Path,
    parked: &mut Option<PathBuf>,
) -> Result<(), Step> {
    let temp = match parked.take() {
        Some(temp) => temp,
        None => {
            let temp = free_temp_name(vfs, from).await?;
            vfs.rename(from, &temp).await?;
            temp
        }
    };
    let Err(error) = vfs.rename(&temp, to).await else {
        return Ok(());
    };
    // The entry goes back to its old name; the error to report is the second step's.
    if vfs.rename(&temp, from).await.is_ok() {
        return Err(error.into());
    }
    *parked = Some(temp.clone());
    Err(OpsError::RenameUndoFailed { path: temp }.into())
}

/// A name beside `path` that nothing has ([`case_path`]).
async fn free_temp_name(vfs: &dyn Vfs, path: &Path) -> Result<PathBuf, Step> {
    let dir = path.parent().unwrap_or(path);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    for _ in 0..TRIES {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let candidate = case_path(dir, nanos, n);
        match vfs.stat(&candidate).await {
            Err(VfsError::NotFound { .. }) => return Ok(candidate),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
    }
    Err(OpsError::Vfs(VfsError::AlreadyExists {
        path: dir.join(".rc-tmp"),
    })
    .into())
}

#[cfg(test)]
#[path = "case_rename_tests.rs"]
mod tests;
