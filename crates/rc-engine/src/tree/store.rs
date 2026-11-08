//! The levels of the folder trees the engine keeps (RC-100-88): only in memory, one per folder
//! read, at most [`MAX_NODES`] sub-folders over all of them. Past the limit the subtrees the
//! window folded go first, the one folded longest ago first, each with every level below it;
//! then the levels asked for longest ago. A level marked to be read again is read at its next
//! use.

use std::{collections::HashMap, sync::Mutex};

use rc_vfs::VfsPath;

use super::TreeLevel;
use crate::recover::LockOrRecover as _;

/// The most sub-folders the engine keeps over every level.
pub const MAX_NODES: usize = 50_000;
/// The most sub-folders of one folder a tree lists; a level with more says so.
pub const MAX_LEVEL: usize = 100_000;

#[derive(Debug)]
struct Kept {
    level: TreeLevel,
    /// Read again at the next use: a panel showing the folder was read again.
    stale: bool,
    /// When the window folded it, by the store's clock; `None` while it is unfolded.
    folded: Option<u64>,
    /// When it was last read or asked for.
    used: u64,
}

#[derive(Debug)]
struct Levels {
    kept: HashMap<VfsPath, Kept>,
    /// The sub-folders over every kept level.
    nodes: usize,
    clock: u64,
    limit: usize,
}

impl Levels {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// `path`'s level and every kept level below it go.
    fn drop_subtree(&mut self, path: &VfsPath) {
        let below: Vec<VfsPath> = self
            .kept
            .keys()
            .filter(|kept| kept.backend == path.backend && kept.path.starts_with(&path.path))
            .cloned()
            .collect();
        for gone in below {
            if let Some(kept) = self.kept.remove(&gone) {
                self.nodes -= kept.level.children.len();
            }
        }
    }

    /// The level to give up next, never `keep` nor a folder above it (its subtree holds `keep`):
    /// the one folded longest ago, else the one asked for longest ago.
    fn victim(&self, keep: &VfsPath) -> Option<VfsPath> {
        let others = || {
            self.kept.iter().filter(move |(path, _)| {
                path.backend != keep.backend || !keep.path.starts_with(&path.path)
            })
        };
        others()
            .filter_map(|(path, kept)| kept.folded.map(|folded| (folded, path)))
            .min_by_key(|(folded, _)| *folded)
            .or_else(|| {
                others()
                    .map(|(path, kept)| (kept.used, path))
                    .min_by_key(|(used, _)| *used)
            })
            .map(|(_, path)| path.clone())
    }

    /// Levels given up until the limit holds; `keep`, the level just read, stays.
    fn trim(&mut self, keep: &VfsPath) {
        while self.nodes > self.limit {
            let Some(victim) = self.victim(keep) else {
                return;
            };
            self.drop_subtree(&victim);
        }
    }
}

/// The kept levels, shared by every tree of the window.
#[derive(Debug)]
pub(crate) struct TreeStore {
    levels: Mutex<Levels>,
}

impl Default for TreeStore {
    fn default() -> Self {
        Self::with_limit(MAX_NODES)
    }
}

impl TreeStore {
    /// A store that keeps at most `limit` sub-folders.
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            levels: Mutex::new(Levels {
                kept: HashMap::new(),
                nodes: 0,
                clock: 0,
                limit,
            }),
        }
    }

    /// The level of `path` while it is kept and need not be read again.
    pub(crate) fn get(&self, path: &VfsPath) -> Option<TreeLevel> {
        let mut levels = self.levels.lock_or_recover("engine.tree");
        let now = levels.tick();
        let kept = levels.kept.get_mut(path).filter(|kept| !kept.stale)?;
        kept.used = now;
        Some(kept.level.clone())
    }

    /// Keeps `level`, read just now, in place of what was kept for its folder; other levels go
    /// while the limit is passed.
    pub(crate) fn put(&self, level: &TreeLevel) {
        let mut levels = self.levels.lock_or_recover("engine.tree");
        let now = levels.tick();
        let added = level.children.len();
        let before = levels.kept.insert(
            level.path.clone(),
            Kept {
                level: level.clone(),
                stale: false,
                folded: None,
                used: now,
            },
        );
        let removed = before.map_or(0, |kept| kept.level.children.len());
        levels.nodes = levels.nodes + added - removed;
        levels.trim(&level.path);
    }

    /// The level of `path`, if kept, is read again at its next use.
    pub(crate) fn stale(&self, path: &VfsPath) {
        let mut levels = self.levels.lock_or_recover("engine.tree");
        if let Some(kept) = levels.kept.get_mut(path) {
            kept.stale = true;
        }
    }

    /// The window folded `path`: its subtree may go first.
    pub(crate) fn folded(&self, path: &VfsPath) {
        let mut levels = self.levels.lock_or_recover("engine.tree");
        let now = levels.tick();
        if let Some(kept) = levels.kept.get_mut(path) {
            kept.folded = Some(now);
        }
    }

    /// The window unfolded `path` again: it and the folders above it stay.
    pub(crate) fn unfolded(&self, path: &VfsPath) {
        let mut levels = self.levels.lock_or_recover("engine.tree");
        for (kept_path, kept) in &mut levels.kept {
            if kept_path.backend == path.backend && path.path.starts_with(&kept_path.path) {
                kept.folded = None;
            }
        }
    }

    /// Every level goes: the names sort another way now.
    pub(crate) fn clear(&self) {
        let mut levels = self.levels.lock_or_recover("engine.tree");
        levels.kept.clear();
        levels.nodes = 0;
    }

    /// How many sub-folders are kept.
    #[cfg(test)]
    pub(crate) fn nodes(&self) -> usize {
        self.levels.lock_or_recover("engine.tree").nodes
    }

    /// Whether the level of `path` is kept, stale or not.
    #[cfg(test)]
    pub(crate) fn holds(&self, path: &VfsPath) -> bool {
        self.levels
            .lock_or_recover("engine.tree")
            .kept
            .contains_key(path)
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
