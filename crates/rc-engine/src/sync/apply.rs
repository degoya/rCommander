//! A plan's actions as file operations: the shape `rc-ops` jobs take (a copy of sources into a
//! target directory, a directory to create, entries to delete). Running them is `rc-ops`'s work
//! and arrives with it; this decides what to run.

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use super::{ItemKind, SyncAction, SyncPlan, SyncRow};
use crate::EngineError;

/// One operation of a synchronization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SyncOperation {
    /// Creates a directory (and missing parents): a directory copied without everything in it.
    Mkdir { path: VfsPath },
    /// Copies every source into the directory `target`, a directory with its contents.
    Copy {
        sources: Vec<VfsPath>,
        target: VfsPath,
        /// The sources replace files of the same name in `target`.
        overwrite: bool,
    },
    /// Deletes every source, a directory with its contents.
    Delete { sources: Vec<VfsPath> },
}

impl SyncPlan {
    /// The operations that carry out the rows' actions, in the order to run them: directories,
    /// copies, then deletes. A directory whose rows below all share its action, with nothing
    /// left out below it, is copied or deleted as a whole.
    ///
    /// # Errors
    ///
    /// `sync_deletes_unconfirmed` when a row deletes and `deletes_confirmed` is false: then no
    /// operation at all is answered, so nothing can be deleted by mistake. `invalid_name` when a
    /// row copied into a local Windows folder has a name Windows would turn into a stream, a
    /// device or a spoofed name (`rc_ops::check_target`, RC-080-12).
    pub fn operations(&self, deletes_confirmed: bool) -> Result<Vec<SyncOperation>, EngineError> {
        let mut mkdirs = Vec::new();
        let mut copies: Vec<(VfsPath, bool, Vec<VfsPath>)> = Vec::new();
        let mut deletes = Vec::new();
        let mut index = 0;
        while index < self.rows.len() {
            let row = &self.rows[index];
            let below = self.below(index);
            let whole = row.kind() == Some(ItemKind::Dir) && self.as_a_whole(index, below);
            let (from, to) = match row.action {
                SyncAction::None => {
                    index += 1;
                    continue;
                }
                SyncAction::CopyToRight => (&self.left, &self.right),
                SyncAction::CopyToLeft => (&self.right, &self.left),
                SyncAction::DeleteRight => {
                    if row.kind() != Some(ItemKind::Dir) || whole {
                        deletes.push(self.right.join(&row.path));
                    }
                    index += if whole { 1 + below } else { 1 };
                    continue;
                }
                SyncAction::DeleteLeft => {
                    if row.kind() != Some(ItemKind::Dir) || whole {
                        deletes.push(self.left.join(&row.path));
                    }
                    index += if whole { 1 + below } else { 1 };
                    continue;
                }
            };
            rc_ops::check_target(&row.path, rc_ops::keeps_windows_rules(&to.backend))?;
            if row.kind() == Some(ItemKind::Dir) && !whole {
                mkdirs.push(SyncOperation::Mkdir {
                    path: to.join(&row.path),
                });
            } else {
                let target = match row.path.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => to.join(parent),
                    _ => to.clone(),
                };
                let overwrite = match row.action {
                    SyncAction::CopyToRight => row.right.is_some(),
                    _ => row.left.is_some(),
                };
                let source = from.join(&row.path);
                match copies
                    .iter_mut()
                    .find(|(t, o, _)| *t == target && *o == overwrite)
                {
                    Some((_, _, sources)) => sources.push(source),
                    None => copies.push((target, overwrite, vec![source])),
                }
            }
            index += if whole { 1 + below } else { 1 };
        }
        if !deletes.is_empty() && !deletes_confirmed {
            return Err(EngineError::SyncDeletesUnconfirmed {
                count: deletes.len(),
            });
        }
        let copies = copies
            .into_iter()
            .map(|(target, overwrite, sources)| SyncOperation::Copy {
                sources,
                target,
                overwrite,
            });
        let deletes = (!deletes.is_empty()).then_some(SyncOperation::Delete { sources: deletes });
        Ok(mkdirs.into_iter().chain(copies).chain(deletes).collect())
    }

    /// How many rows below row `index` belong to it (they follow it in tree order).
    fn below(&self, index: usize) -> usize {
        let parent = &self.rows[index].path;
        self.rows[index + 1..]
            .iter()
            .take_while(|row| row.path.starts_with(parent))
            .count()
    }

    /// Whether the directory row `index` is copied or deleted in one piece: only its side has
    /// it (nothing to merge into or keep on the other side), nothing below it was left out,
    /// and every row below it has its action.
    fn as_a_whole(&self, index: usize, below: usize) -> bool {
        let row = &self.rows[index];
        let one_sided = match row.action {
            SyncAction::CopyToRight => row.right.is_none(),
            SyncAction::CopyToLeft => row.left.is_none(),
            SyncAction::DeleteRight => row.left.is_none(),
            SyncAction::DeleteLeft => row.right.is_none(),
            SyncAction::None => false,
        };
        let same = |other: &SyncRow| other.action == row.action && !other.partial;
        one_sided && !row.partial && self.rows[index + 1..=index + below].iter().all(same)
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
