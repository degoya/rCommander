//! The conflict protocol: what a job asks when a target exists or a file fails, and the
//! answers.
//!
//! A conflict is a question, not a dialog: the job stops at the file, publishes the
//! [`Question`] and waits for an [`Answer`] through [`crate::Queue::answer`] - from the GUI's
//! dialog or any other caller. An answer "for all" holds for every later conflict of the job;
//! [`crate::JobOptions::on_conflict`] gives one in advance, so the job never asks.
//!
//! The answers are Total Commander's overwrite dialog: overwrite, skip, rename the new file
//! (to a free name or to one the person types), rename the existing target, overwrite older,
//! larger or smaller, compare, cancel - each "for all" where that makes sense ("skip all",
//! "overwrite all older", "copy all larger"). A typed name is untrusted input and is checked
//! ([`crate::name`]) before the job sees it. "Compare" leaves the question open: the GUI shows
//! the two files and answers again.
//!
//! Two directories of the same name are never a conflict: they are merged and the files inside
//! meet their own conflicts. A file meeting a directory (or the other way round) cannot be
//! overwritten: `Overwrite` there fails the file with `kind_mismatch`.
//!
//! A copy to or from a server that finds the part file of an interrupted copy where its target
//! would be (`<name>.rcpart`) asks too, with the part as the target: "resume" continues it (as
//! Total Commander's overwrite dialog offers), "overwrite" starts it over (RC-060-07).

use std::path::{Path, PathBuf};

use rc_vfs::{Entry, Vfs, VfsError, VfsPath, WriteMode};
use serde::{Deserialize, Serialize};

use crate::{
    Failure, OpsError,
    control::Control,
    resume::Placement,
    runner::{Runner, Step, Stop},
    staging::Resolved,
    temp_name::part_path,
    walk::Item,
};

/// How many numbered names [`auto_rename`] tries before it gives up.
pub(crate) const RENAME_CANDIDATES: u32 = 10_000;
/// How often "rename the existing target" looks for a free name again when the one it found was
/// taken before the rename (RC-080-56).
const ASIDE_TRIES: u32 = 3;

/// An answer to a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    /// Replace the target.
    Overwrite,
    /// Leave the target, do not copy this file.
    Skip,
    /// Copy under a free name beside the target: `name (2).ext`, `name (3).ext`, ...
    Rename,
    /// Replace the target only when it is older than the source; skip it otherwise (also when
    /// either time is unknown).
    OverwriteOlder,
    /// Replace the target only when the source is larger ("copy all larger"); skip otherwise.
    OverwriteLarger,
    /// Replace the target only when the source is smaller ("copy all smaller"); skip otherwise.
    OverwriteSmaller,
    /// Move the existing target aside to a free name (`name (2).ext`), then copy.
    RenameExisting,
    /// Stop the job.
    Cancel,
    /// Continue the part file (`<name>.rcpart`) an interrupted copy left, from its length on
    /// (RC-060-07); where the target is no such part, or one larger than the source, skip.
    Resume,
}

/// An answer to a failing file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorChoice {
    /// Try the file again.
    Retry,
    /// Record it as failed and go on.
    Skip,
    /// Stop the job as failed.
    Cancel,
}

/// One side of a conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryInfo {
    pub path: VfsPath,
    pub is_dir: bool,
    pub size: u64,
    /// Last modification as Unix seconds, when the backend knows it.
    pub modified_unix: Option<u64>,
}

impl EntryInfo {
    pub(crate) fn new(path: &VfsPath, entry: &Entry) -> Self {
        Self {
            path: path.clone(),
            is_dir: entry.is_dir_like(),
            size: entry.size,
            modified_unix: entry.modified_unix(),
        }
    }
}

/// What a job waits on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Question {
    /// The target exists; answer with [`Answer::Conflict`]. The target may be the part file an
    /// interrupted copy left (`<name>.rcpart`) where the target's name is free; `resumable` then
    /// says it can be continued ([`ConflictChoice::Resume`]).
    Conflict {
        source: EntryInfo,
        target: EntryInfo,
        #[serde(default)]
        resumable: bool,
    },
    /// A file failed; answer with [`Answer::Error`].
    Error(Failure),
    /// An archive to unpack is encrypted; answered only with [`crate::Queue::answer_password`],
    /// never with an [`Answer`] (RC-030-03). `wrong`: the password given before did not fit.
    Password { archive: VfsPath, wrong: bool },
}

/// The caller's answer to a [`Question`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Conflict {
        choice: ConflictChoice,
        /// Apply the choice to every later conflict of the job.
        #[serde(default)]
        for_all: bool,
    },
    /// Copy this file under `name`, in the target's directory; refused with `invalid_name`
    /// (the question stays open) unless it is one plain file name. Never "for all".
    RenameTo {
        name: String,
    },
    /// The caller compares the two files; the question stays open for the real answer.
    Compare,
    Error {
        choice: ErrorChoice,
    },
}

impl Answer {
    /// Whether this answers `question`.
    pub(crate) fn fits(&self, question: &Question) -> bool {
        matches!(
            (self, question),
            (
                Self::Conflict { .. } | Self::RenameTo { .. } | Self::Compare,
                Question::Conflict { .. }
            ) | (Self::Error { .. }, Question::Error(_))
        )
    }
}

/// What a resolved conflict does with the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Decision {
    Overwrite,
    /// Continue the part file (RC-060-07).
    Resume,
    Skip,
    Rename,
    RenameExisting,
    /// Copy under this (checked) name in the target's directory.
    RenameTo(String),
    Cancel,
}

/// The decision of `choice` for `source` meeting `target`.
pub(crate) fn decide(choice: ConflictChoice, source: &Entry, target: &Entry) -> Decision {
    match choice {
        ConflictChoice::Overwrite => Decision::Overwrite,
        ConflictChoice::Skip => Decision::Skip,
        ConflictChoice::Rename => Decision::Rename,
        ConflictChoice::RenameExisting => Decision::RenameExisting,
        ConflictChoice::Cancel => Decision::Cancel,
        // Only a resumable part continues (`Runner::conflict`); any other target stays.
        ConflictChoice::Resume => Decision::Skip,
        ConflictChoice::OverwriteLarger if source.size > target.size => Decision::Overwrite,
        ConflictChoice::OverwriteSmaller if source.size < target.size => Decision::Overwrite,
        ConflictChoice::OverwriteLarger | ConflictChoice::OverwriteSmaller => Decision::Skip,
        ConflictChoice::OverwriteOlder => match (target.modified, source.modified) {
            (Some(target), Some(source)) if target < source => Decision::Overwrite,
            _ => Decision::Skip,
        },
    }
}

/// `name` with the number `n`: `report (2).pdf`, `archive.tar (2).gz`, `.profile (2)`.
#[must_use]
pub(crate) fn numbered(name: &str, n: u32) -> String {
    let path = Path::new(name);
    match (path.file_stem(), path.extension()) {
        (Some(stem), Some(extension)) => format!(
            "{} ({n}).{}",
            stem.to_string_lossy(),
            extension.to_string_lossy()
        ),
        _ => format!("{name} ({n})"),
    }
}

/// The first free name beside `path`, numbered from 2; at most [`RENAME_CANDIDATES`] are tried,
/// each after a look at `control`, so a pause holds and a cancel stops the search.
///
/// # Errors
///
/// A `stat` fails for another reason than a missing entry, every candidate is taken
/// (`already_exists`), or the job is cancelled.
pub(crate) async fn auto_rename(
    vfs: &dyn Vfs,
    path: &Path,
    control: &Control,
) -> Result<PathBuf, Step> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = path.parent().unwrap_or(path);
    for n in 2..=RENAME_CANDIDATES + 1 {
        control.checkpoint().await.map_err(Stop::from)?;
        let candidate = dir.join(numbered(&name, n));
        match vfs.stat(&candidate).await {
            Err(VfsError::NotFound { .. }) => return Ok(candidate),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
    }
    Err(OpsError::Vfs(VfsError::AlreadyExists {
        path: path.to_path_buf(),
    })
    .into())
}

impl Runner {
    /// Moves the existing target `met` aside to a free numbered name. The rename never replaces:
    /// a name taken between the look and the rename is left alone and the next free one tried,
    /// [`ASIDE_TRIES`] times, then `already_exists`.
    async fn move_aside(&self, met: &Path) -> Result<(), Step> {
        for _ in 0..ASIDE_TRIES {
            let aside = auto_rename(self.dst.as_ref(), met, &self.job.control).await?;
            match self.dst.rename_no_replace(met, &aside).await {
                Ok(()) => return Ok(()),
                Err(VfsError::AlreadyExists { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(OpsError::Vfs(VfsError::AlreadyExists {
            path: met.to_path_buf(),
        })
        .into())
    }

    /// Where the file goes and how it is opened, or whether a conflict answer skips it or a
    /// restored job finds it complete. A typed name can meet a conflict of its own, which is
    /// asked again. Where a resumable transfer's target is free but its part file is there, the
    /// part is the conflict (`staging.rs`); a restored job continues the part of the file it was
    /// writing at once.
    pub(crate) async fn resolve(
        &mut self,
        item: &Item,
        source: &VfsPath,
        target: &VfsPath,
    ) -> Result<Resolved, Step> {
        let mut target = target.clone();
        if let Some(placement) = self.restored_placement(item, &target.path).await? {
            return Ok(Resolved::Write(placement));
        }
        loop {
            let (met, existing, is_part) = match self.dst.stat(&target.path).await {
                Err(VfsError::NotFound { .. }) => match self.leftover_part(&target.path).await? {
                    Some(part) => (part_path(&target.path), part, true),
                    None => {
                        let placement = Placement::new(target.path, WriteMode::CreateNew);
                        return Ok(Resolved::Write(placement));
                    }
                },
                Err(error) => return Err(error.into()),
                Ok(existing) if self.restored && self.already_complete(item, &existing) => {
                    return Ok(Resolved::Complete);
                }
                Ok(existing) => (target.path.clone(), existing, false),
            };
            let shown = VfsPath {
                backend: target.backend.clone(),
                path: met.clone(),
            };
            let resumable = is_part && existing.size <= item.entry.size;
            let decision = self
                .conflict((source, &item.entry), (&shown, &existing), resumable)
                .await?;
            // A part stands in for a free target: replacing it creates the target.
            let replace = if is_part {
                WriteMode::CreateNew
            } else {
                WriteMode::Truncate
            };
            return match decision {
                Decision::Resume => Ok(Resolved::Write(Placement {
                    dest: target.path,
                    mode: WriteMode::CreateNew,
                    resume: true,
                })),
                Decision::Overwrite if existing.is_dir_like() => {
                    Err(OpsError::KindMismatch { path: met }.into())
                }
                Decision::Overwrite => Ok(Resolved::Write(Placement::new(target.path, replace))),
                Decision::Skip => Ok(Resolved::Skip),
                Decision::Rename => {
                    let free =
                        auto_rename(self.dst.as_ref(), &target.path, &self.job.control).await?;
                    Ok(Resolved::Write(Placement::new(free, WriteMode::CreateNew)))
                }
                Decision::RenameExisting => {
                    self.move_aside(&met).await?;
                    let placement = Placement::new(target.path, WriteMode::CreateNew);
                    Ok(Resolved::Write(placement))
                }
                Decision::RenameTo(name) => {
                    target.path.set_file_name(name);
                    continue;
                }
                Decision::Cancel => Err(Step::Stop(Stop::Cancelled)),
            };
        }
    }
}

#[cfg(test)]
#[path = "conflict_tests.rs"]
mod tests;
