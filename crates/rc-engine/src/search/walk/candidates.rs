//! The walk over Everything's candidates (RC-100-93): instead of listing the start folders, the
//! walk takes the paths Everything found for the masks and judges each one as it would have met
//! it - below a start folder, within the depth, through folders it would have entered (the
//! directory exclusions, hidden folders, links), then by name, the filters and the content. So
//! the meaning of a search stays rCommander's; Everything only saves the listing. A candidate
//! that is gone meanwhile (Everything's index runs behind) is no hit and no error.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rc_vfs::{EntryKind, VfsError};

use super::Walk;
use crate::search::SearchEvent;

/// The candidates still to judge.
#[derive(Debug)]
pub(crate) struct Candidates {
    /// The start folders, as the walk was given them.
    roots: Vec<PathBuf>,
    paths: std::vec::IntoIter<PathBuf>,
    /// Folders between a start folder and a candidate, judged once each: whether the walk would
    /// have entered them.
    entered: HashMap<PathBuf, bool>,
}

impl Candidates {
    /// The start folder `path` lies below, and the path below it; `None` outside all of them.
    fn place<'a>(&self, path: &'a Path) -> Option<(PathBuf, &'a Path)> {
        self.roots.iter().find_map(|root| {
            let rest = path.strip_prefix(root).ok()?;
            rest.components()
                .next()
                .is_some()
                .then(|| (root.clone(), rest))
        })
    }
}

impl Walk {
    /// The walk judging `paths` (Everything's candidates) instead of listing its start folders.
    pub(crate) fn over_candidates(mut self, paths: Vec<PathBuf>) -> Self {
        let roots = self.stack.drain(..).rev().map(|dir| dir.path).collect();
        self.candidates = Some(Candidates {
            roots,
            paths: paths.into_iter(),
            entered: HashMap::new(),
        });
        self
    }

    /// The next candidate judged: `None` when there are no more, `Some(None)` for one that is no
    /// hit.
    pub(super) async fn next_candidate(&mut self) -> Option<Option<SearchEvent>> {
        let candidates = self.candidates.as_mut()?;
        let path = candidates.paths.next()?;
        let Some((root, rest)) = candidates.place(&path) else {
            return Some(None);
        };
        let levels = rest.components().count();
        // A candidate in a start folder is at depth 0, as the walk counts.
        let depth = u32::try_from(levels.saturating_sub(1)).unwrap_or(u32::MAX);
        if self.query.max_depth.is_some_and(|max| depth > max) {
            return Some(None);
        }
        let mut folder = root;
        for name in rest.components().take(levels.saturating_sub(1)) {
            folder.push(name);
            if !self.entered(&folder).await {
                return Some(None);
            }
        }
        self.stats.entries += 1;
        let entry = match self.vfs.stat(&path).await {
            Ok(entry) => entry,
            Err(VfsError::NotFound { .. }) => return Some(None),
            Err(error) => {
                self.stats.skipped += 1;
                return Some(Some(SearchEvent::Skipped {
                    path: self.located(None, &path),
                    code: error.code(),
                }));
            }
        };
        if self.query.skip_hidden && entry.hidden {
            return Some(None);
        }
        // What the ignore list hides is no hit (RC-100-92).
        if let Some(dir) = path.parent()
            && crate::ignore_list::ignored(&self.located(None, dir), &entry)
        {
            return Some(None);
        }
        Some(self.judge(path, None, entry).await)
    }

    /// Whether the walk would have entered `folder` on its way down: not excluded by name, not
    /// hidden when hidden ones are left out, no link unless links are followed. Judged once.
    async fn entered(&mut self, folder: &Path) -> bool {
        if let Some(known) = self
            .candidates
            .as_ref()
            .and_then(|candidates| candidates.entered.get(folder))
        {
            return *known;
        }
        let name = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let entered = self.query.names.enters(&name)
            && match self.vfs.stat(folder).await {
                Ok(entry) => {
                    !(self.query.skip_hidden && entry.hidden)
                        && match entry.kind {
                            EntryKind::Dir => true,
                            EntryKind::Symlink { .. } => self.query.follow_symlinks,
                            _ => false,
                        }
                }
                Err(_) => false,
            };
        if let Some(candidates) = self.candidates.as_mut() {
            candidates.entered.insert(folder.to_path_buf(), entered);
        }
        entered
    }
}

#[cfg(test)]
#[path = "candidates_tests.rs"]
mod tests;
