//! A copy or move target with Total Commander's wildcards (RC-110-04): `target\*.bak` typed in
//! the F5 and F6 dialogs (`ops.copy_to`, `ops.move_to`), `rename_mask` of `ops.copy` and
//! `ops.move`.
//!
//! Only the last part of a typed target may hold `*` or `?`; it is the mask, the rest the folder
//! ([`split_mask`]). `*.*` and `*` alone stay "into the folder, names unchanged"
//! (`typed_target`). The engine works out every entry's new name (`rc_rename::mask_targets`)
//! and submits one `CopyAs` or `MoveAs` per entry as one batch, whatever
//! `queue.one_job_per_item` says: one operation, so an answer "for all" and a cancel hold for
//! every entry. Before anything is submitted it refuses two entries with one new name
//! (`mask_collision`), a new name no file system can hold (`invalid_name`, on every backend: it
//! was typed), a copy beside itself that keeps its name (`mask_unchanged`), a rename onto the old
//! name of another entry of the rename (`mask_chain`) and a mask into an archive
//! (`mask_into_archive`). A target that exists already is a conflict of the job, with the
//! overwrite dialog, as in Total Commander; a rename that keeps a name passes that entry over.

use std::path::is_separator;

use rc_ops::{JobKind, JobOptions, JobSpec, OpsError, QueueListId, check_name};
use rc_rename::{MaskSource, RenameError, has_wildcards, mask_targets};
use rc_vfs::{BackendId, VfsPath};

use crate::{Engine, EngineError, Outcome};

/// `typed`, a target as `typed_target` left it, split into its folder and the mask its last part
/// is; the whole of it and no mask when that part has no wildcard. The folder of `*.bak` alone
/// is empty: the panel's own.
///
/// # Errors
///
/// `invalid_name` for a wildcard in the folder (`c:\x*\*.bak`).
pub(crate) fn split_mask(typed: &str) -> Result<(&str, Option<&str>), OpsError> {
    let start = typed.rfind(is_separator).map_or(0, |at| at + 1);
    let (folder, last) = typed.split_at(start);
    // A long path's `\\?\` is no wildcard.
    let checked = folder
        .strip_prefix(r"\\?\")
        .or_else(|| folder.strip_prefix("//?/"))
        .unwrap_or(folder);
    if has_wildcards(checked) {
        return Err(OpsError::InvalidName {
            name: typed.to_owned(),
            reason: "a wildcard outside the last part",
        });
    }
    Ok(if has_wildcards(last) {
        (folder, Some(last))
    } else {
        (typed, None)
    })
}

/// Refuses a `rename_mask` (`ops.copy`, `ops.move`) that is more than a name: a separator in it
/// would reach another folder.
///
/// # Errors
///
/// `invalid_name`.
pub(crate) fn check_rename_mask(mask: &str) -> Result<(), OpsError> {
    if mask.contains(['/', '\\']) || mask.trim().is_empty() {
        return Err(OpsError::InvalidName {
            name: mask.to_owned(),
            reason: "a mask is one name, without a separator",
        });
    }
    Ok(())
}

/// Refuses a mask into an archive: its add takes no new names.
pub(crate) fn refuse_mask_into_archive(target: &VfsPath) -> Result<(), EngineError> {
    if matches!(target.backend, BackendId::Archive { .. }) {
        return Err(RenameError::MaskIntoArchive.into());
    }
    Ok(())
}

/// The name of `source`, the last part of its path.
fn name_of(source: &VfsPath) -> Result<String, OpsError> {
    source.file_name().ok_or_else(|| OpsError::InvalidName {
        name: source.path.display().to_string(),
        reason: "empty",
    })
}

impl Engine {
    /// `sources` copied or moved into the folder `target` under the names `mask` gives them, as
    /// one batch into the queue list `list`; the backends are open and the transfer settings
    /// applied.
    pub(crate) async fn submit_masked(
        &self,
        sources: Vec<VfsPath>,
        target: &VfsPath,
        mask: &str,
        (options, list): (JobOptions, QueueListId),
        moving: bool,
    ) -> Result<Outcome, EngineError> {
        let case_insensitive = !self.vfs(target)?.capabilities().case_sensitive;
        let names = sources.iter().map(name_of).collect::<Result<Vec<_>, _>>()?;
        let same_folder: Vec<bool> = sources
            .iter()
            .map(|source| in_folder(source, target, case_insensitive))
            .collect();
        let entries: Vec<MaskSource<'_>> = names
            .iter()
            .zip(&same_folder)
            .map(|(name, &same_folder)| MaskSource { name, same_folder })
            .collect();
        let targets = mask_targets(mask, &entries, moving, case_insensitive)?;
        targets
            .iter()
            .flatten()
            .try_for_each(|name| check_name(name))?;
        let mut options = options;
        options.copy_links = self.copy_links().await;
        let specs: Vec<JobSpec> = sources
            .into_iter()
            .zip(targets)
            .filter_map(|(source, name)| Some((source, target.join(&name?))))
            .map(|(source, target)| JobSpec {
                kind: if moving {
                    JobKind::MoveAs { source, target }
                } else {
                    JobKind::CopyAs { source, target }
                },
                options,
            })
            .collect();
        self.refuse_in_transfer(specs.iter().map(|spec| &spec.kind))
            .await?;
        let mut jobs = self.inner.ops.submit_batch(specs, list)?;
        Ok(if jobs.len() == 1 {
            Outcome::job(jobs.remove(0))
        } else {
            Outcome::Jobs { jobs }
        })
    }
}

/// Whether `source` lies in the folder `folder` (Shift+F5, a rename in place).
fn in_folder(source: &VfsPath, folder: &VfsPath, case_insensitive: bool) -> bool {
    let Some(parent) = source.path.parent() else {
        return false;
    };
    if source.backend != folder.backend {
        return false;
    }
    let text = |path: &std::path::Path| {
        let text = path.to_string_lossy();
        let text = text.trim_end_matches(is_separator);
        if case_insensitive {
            text.to_lowercase()
        } else {
            text.to_owned()
        }
    };
    text(parent) == text(&folder.path)
}

#[cfg(test)]
#[path = "file_ops_mask_tests.rs"]
mod tests;
