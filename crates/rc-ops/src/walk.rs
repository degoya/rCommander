//! Counting before the work: every entry under the sources, with the totals for the
//! progress.
//!
//! The walk is depth first and pre-order - a directory comes before its contents, and a
//! directory's contents stay together - so a copy creates directories before their files and a
//! permanent delete, walking the list backwards, empties a directory before removing it. Links
//! are listed, never followed, and a listed name must be one plain name.

use std::{
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
};

use rc_vfs::{Entry, EntryKind, VfsPath};

use crate::{
    JobState,
    name::check_listed,
    runner::{Runner, Step, Stop, attempt},
};

/// One entry under a source.
#[derive(Debug, Clone)]
pub(crate) struct Item {
    /// Where it is, on the source backend.
    pub(crate) path: PathBuf,
    /// Its path below the source's parent: `name` for a source, `name/inner/...` below it.
    pub(crate) rel: PathBuf,
    pub(crate) entry: Entry,
}

impl Item {
    /// Whether the item counts as a file in the progress: anything but a directory.
    pub(crate) fn is_file(&self) -> bool {
        self.entry.kind != EntryKind::Dir
    }
}

/// Files and bytes of `items` under `rel` (itself included).
fn subtree(items: &[Item], rel: &Path) -> (u64, u64) {
    tally(items.iter().filter(|item| item.rel.starts_with(rel)))
}

/// Files and bytes of `items`.
fn tally<'a>(items: impl Iterator<Item = &'a Item>) -> (u64, u64) {
    items
        .filter(|item| item.is_file())
        .fold((0, 0), |(files, bytes), item| {
            (files + 1, bytes + item.entry.size)
        })
}

/// The subtrees of a plan, and those a job gave up (RC-080-45). The walk keeps a directory's
/// contents right behind it, so a subtree is its item and the run of items under it - found
/// once, never by a pass over the whole plan per subtree. Where two sources share a name (a
/// flat view's `a/x` and `b/x`), their paths are shared too, and the plan is searched by path
/// as before.
pub(crate) struct Subtrees<'a> {
    items: &'a [Item],
    /// Every path is one item's, so subtrees are index ranges; worked out when the first
    /// subtree is asked for, so a transfer that gives none up costs nothing here.
    by_range: Option<bool>,
    /// By range: the last subtree given up. A transfer walks the plan in order, so the ones
    /// before it lie behind.
    skip: Range<usize>,
    /// By path: the subtrees given up.
    gone: Vec<&'a Path>,
}

impl<'a> Subtrees<'a> {
    pub(crate) fn new(items: &'a [Item]) -> Self {
        Self {
            items,
            by_range: None,
            skip: 0..0,
            gone: Vec::new(),
        }
    }

    fn by_range(&mut self) -> bool {
        let items = self.items;
        *self.by_range.get_or_insert_with(|| {
            let mut seen = HashSet::with_capacity(items.len());
            items.iter().all(|item| seen.insert(item.rel.as_path()))
        })
    }

    /// The index after the last item of the subtree of the item at `at`.
    fn end(&self, at: usize) -> usize {
        let rel = &self.items[at].rel;
        let below = self.items[at + 1..]
            .iter()
            .take_while(|item| item.rel.starts_with(rel))
            .count();
        at + 1 + below
    }

    /// Files and bytes of the subtree of the item at `at`, itself included.
    pub(crate) fn tally(&mut self, at: usize) -> (u64, u64) {
        if self.by_range() {
            tally(self.items[at..self.end(at)].iter())
        } else {
            subtree(self.items, &self.items[at].rel)
        }
    }

    /// Whether the item at `at` lies in a subtree given up; asked in the plan's order.
    pub(crate) fn skipped(&self, at: usize) -> bool {
        match self.by_range {
            // Nothing given up yet.
            None => false,
            Some(true) => self.skip.contains(&at),
            Some(false) => {
                let rel = &self.items[at].rel;
                self.gone.iter().any(|gone| rel.starts_with(gone))
            }
        }
    }

    /// Gives up the subtree of the item at `at`; its files and bytes, which count as done.
    pub(crate) fn give_up(&mut self, at: usize) -> (u64, u64) {
        if self.by_range() {
            let end = self.end(at);
            if !self.skip.contains(&at) {
                self.skip = at..end;
            }
            tally(self.items[at..end].iter())
        } else {
            let items = self.items;
            self.gone.push(&items[at].rel);
            subtree(items, &items[at].rel)
        }
    }
}

impl Runner {
    /// Every entry under `sources` on the source backend, counted into the progress. A source
    /// or a directory that cannot be read is a failing step like any other.
    pub(crate) async fn plan(&mut self, sources: &[VfsPath]) -> Result<Vec<Item>, Stop> {
        self.job
            .update(|snapshot| snapshot.state = JobState::Counting);
        let mut items = Vec::new();
        for source in sources {
            self.checkpoint().await?;
            let src = std::sync::Arc::clone(&self.src);
            let Some(entry) = attempt!(
                self,
                source,
                src.stat(&source.path).await.map_err(Step::from)
            ) else {
                continue;
            };
            let name = source
                .path
                .file_name()
                .map_or_else(|| PathBuf::from(&entry.name), PathBuf::from);
            let mut stack = vec![Item {
                path: source.path.clone(),
                rel: name,
                entry,
            }];
            while let Some(item) = stack.pop() {
                self.checkpoint().await?;
                if item.entry.kind != EntryKind::Dir {
                    self.progress.files_total += 1;
                    self.progress.bytes_total += item.entry.size;
                    items.push(item);
                    continue;
                }
                let at = self.src_path(&item.path);
                let listed = attempt!(self, &at, src.list(&item.path).await.map_err(Step::from));
                let Some(mut children) = listed else {
                    // Unreadable: neither it nor its contents are touched.
                    continue;
                };
                // Reversed, so the stack hands them out by name.
                children.sort_by(|a, b| b.name.cmp(&a.name));
                for child in children {
                    // A name a join would turn into a way out (`../x`, `/abs`) fails here: the
                    // entry is neither read nor written anywhere.
                    if attempt!(self, &at, check_listed(&child.name).map_err(Step::from)).is_none()
                    {
                        continue;
                    }
                    stack.push(Item {
                        path: item.path.join(&child.name),
                        rel: item.rel.join(&child.name),
                        entry: child,
                    });
                }
                items.push(item);
                self.tick(false);
            }
        }
        self.job
            .update(|snapshot| snapshot.state = JobState::Running);
        self.tick(true);
        Ok(items)
    }
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
