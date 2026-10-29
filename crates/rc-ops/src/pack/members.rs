//! What each planned item becomes in an archive, and which of them went in whole.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use rc_vfs::{BackendId, EntryKind, TargetKind, VfsError};

use super::{
    Group, PackSettings,
    write::{Member, MemberKind},
};
use crate::{Failure, OpsError, queue::fold, runner::Runner, walk::Item};

/// The name an item has in the archive, `/`-separated.
pub(super) fn member_name(item: &Item, keep_paths: bool) -> String {
    if keep_paths {
        let parts: Vec<String> = item
            .rel
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        parts.join("/")
    } else {
        item.entry.name.clone()
    }
}

/// Whether the file at `path` is the archive written to `dest`: an older one being replaced,
/// which never goes into itself. Where case does not count, `Archive.zip` is `archive.zip`.
pub(super) fn is_itself(path: &Path, dest: &Path, case_sensitive: bool) -> bool {
    fold(path, case_sensitive) == fold(dest, case_sensitive)
}

impl Runner {
    /// The members of a group: what each item becomes in the archive. Items that cannot go in
    /// (a link the format cannot store, a special file, a name taken twice) are listed as
    /// failures.
    pub(super) fn members(
        &mut self,
        group: &Group,
        settings: &PackSettings,
    ) -> Vec<(Member, Item)> {
        let mut names = HashSet::new();
        let mut members = Vec::with_capacity(group.items.len());
        let case_sensitive = self.src.capabilities().case_sensitive;
        for item in &group.items {
            // Folders go only with their paths; the archive itself (an older one being
            // replaced) never goes into itself.
            let itself = self.src_id == BackendId::Local
                && is_itself(&item.path, &group.dest, case_sensitive);
            if itself || (item.entry.kind == EntryKind::Dir && !settings.keep_paths) {
                continue;
            }
            let kind = match &item.entry.kind {
                EntryKind::Dir => Some(MemberKind::Dir),
                EntryKind::File => Some(MemberKind::File),
                EntryKind::Symlink { target, .. }
                    if settings.store_links && settings.format.stores_links() =>
                {
                    Some(MemberKind::Link(target.to_string_lossy().into_owned()))
                }
                EntryKind::Symlink {
                    target_kind: Some(TargetKind::File),
                    ..
                } if !settings.store_links => Some(MemberKind::File),
                _ => None,
            };
            let name = member_name(item, settings.keep_paths);
            let refused = match &kind {
                None => Some(OpsError::NotCopyable {
                    path: item.path.clone(),
                }),
                // Names are compared ignoring case: Windows unpacks them into one file.
                Some(_) if !names.insert(name.to_lowercase()) => {
                    Some(OpsError::Vfs(VfsError::AlreadyExists {
                        path: PathBuf::from(&name),
                    }))
                }
                Some(_) => None,
            };
            if let Some(error) = refused {
                let failure = Failure::new(&self.src_path(&item.path), &error);
                self.job.update(|snapshot| snapshot.failures.push(failure));
                continue;
            }
            let Some(kind) = kind else { continue };
            members.push((
                Member {
                    path: item.path.clone(),
                    name,
                    kind,
                    size: item.entry.size,
                    modified: item.entry.modified,
                    mode: item.entry.attributes.mode,
                    readonly: item.entry.attributes.readonly,
                },
                item.clone(),
            ));
        }
        members
    }

    /// The packed items without those that grew while they were packed (`write.rs`): only their
    /// counted size went in, so each is a failure and a move to the archive keeps it.
    pub(super) fn without_grown(&mut self, items: Vec<Item>, grown: &[PathBuf]) -> Vec<Item> {
        if grown.is_empty() {
            return items;
        }
        let mut whole = Vec::with_capacity(items.len());
        for item in items {
            if grown.contains(&item.path) {
                let error = OpsError::VerifyFailed {
                    path: item.path.clone(),
                };
                let failure = Failure::new(&self.src_path(&item.path), &error);
                self.job.update(|snapshot| snapshot.failures.push(failure));
            } else {
                whole.push(item);
            }
        }
        whole
    }
}
