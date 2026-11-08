//! A folder tree as the window keeps it (RC-100-88): the folders read so far below one root,
//! which are unfolded, and the rows on screen. A folder's sub-folders are asked of the engine
//! the first time it unfolds (`Engine::tree_children`), each level once; showing a folder
//! unfolds the chain from the root down to it, one level after the other ([`DirTree::reveal`]).
//! The keys are the Explorer's and Total Commander's: Right unfolds a folder or steps to its
//! first sub-folder, Left folds it or steps to its parent; typed letters jump to the next folder
//! read whose name starts with them.
//!
//! Only the rows on screen are drawn; a level read again keeps the sub-folders still there with
//! what was unfolded below them. The tree stops taking folders at [`MAX_NODES`]. The way down
//! and the levels taken in are `model/levels.rs`, the jump by typed letters `model/jump.rs`.

mod jump;
mod levels;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use rc_engine::{VfsPath, tree::TreeLevel};

/// The most folders one tree holds; a level past it is cut and the tree says it is full.
pub const MAX_NODES: usize = 100_000;
/// The rows a tree shows until the window measured it.
const FIRST_HEIGHT: usize = 20;
/// Letters typed within this time make one name to look for, as the quick search does.
const TYPING_PAUSE: Duration = Duration::from_secs(1);
/// The root's id.
const ROOT: u64 = 0;

/// Whether a folder's sub-folders are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kids {
    /// Not asked for yet.
    Unread,
    Loading,
    Loaded,
    /// The level did not read; the code is the node's.
    Failed(&'static str),
}

#[derive(Debug)]
struct Node {
    name: String,
    path: VfsPath,
    parent: Option<u64>,
    depth: usize,
    open: bool,
    /// A link to a folder: listed, never unfolded.
    link: bool,
    kids: Kids,
    /// More sub-folders than a level lists.
    truncated: bool,
    children: Vec<u64>,
}

/// A row of the unfolded tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    pub depth: usize,
}

/// The sign before a folder's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    /// Unfolded.
    Open,
    /// Folded, or not read yet.
    Closed,
    /// Read, without sub-folders, or a link.
    Leaf,
    Loading,
    /// Its level did not read.
    Failed,
}

/// What a row shows of its folder.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    pub name: &'a str,
    pub sign: Sign,
    /// Why its level did not read: an error code.
    pub error: Option<&'static str>,
    pub truncated: bool,
}

/// A step of the cursor, by its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    /// ArrowRight: unfold, or on to the first sub-folder.
    Open,
    /// ArrowLeft: fold, or back to the parent.
    Close,
}

/// What a key or a click changed, for the window to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Nothing,
    /// The cursor is on another folder.
    Cursor,
    /// A folder unfolded; `load` when its level is to be read first.
    Unfolded {
        path: VfsPath,
        load: bool,
    },
    /// A folder folded.
    Folded(VfsPath),
}

#[derive(Debug)]
pub struct DirTree {
    nodes: HashMap<u64, Node>,
    by_path: HashMap<VfsPath, u64>,
    next_id: u64,
    rows: Vec<Row>,
    /// The row the cursor is on.
    pub cursor: usize,
    /// The first row on screen.
    pub top: usize,
    /// The rows that fit.
    pub height: usize,
    /// Folders were left out at [`MAX_NODES`].
    pub full: bool,
    /// Whether hidden folders were shown when the levels were read.
    pub hidden: bool,
    /// The folder [`DirTree::reveal`] leads to while the levels on its way are read.
    target: Option<VfsPath>,
    /// The letters typed for a jump, and when the last came.
    typed: String,
    typed_at: Option<Instant>,
}

impl DirTree {
    /// A tree of the folders below `root`, shown as `label`; nothing read yet.
    pub fn new(root: VfsPath, label: String, hidden: bool) -> Self {
        let node = Node {
            name: label,
            path: root.clone(),
            parent: None,
            depth: 0,
            open: false,
            link: false,
            kids: Kids::Unread,
            truncated: false,
            children: Vec::new(),
        };
        let mut tree = Self {
            nodes: HashMap::from([(ROOT, node)]),
            by_path: HashMap::from([(root, ROOT)]),
            next_id: ROOT + 1,
            rows: Vec::new(),
            cursor: 0,
            top: 0,
            height: FIRST_HEIGHT,
            full: false,
            hidden,
            target: None,
            typed: String::new(),
            typed_at: None,
        };
        tree.rebuild();
        tree
    }

    /// The location the tree starts at.
    pub fn root(&self) -> &VfsPath {
        &self.nodes[&ROOT].path
    }

    /// The rows on screen.
    pub fn visible(&self) -> &[Row] {
        let end = (self.top + self.height).min(self.rows.len());
        &self.rows[self.top.min(end)..end]
    }

    /// How many folders the tree holds.
    #[cfg(test)]
    pub fn folders(&self) -> usize {
        self.nodes.len()
    }

    /// What row `row` shows.
    pub fn shown(&self, row: Row) -> Option<Shown<'_>> {
        let node = self.nodes.get(&row.id)?;
        let sign = match node.kids {
            Kids::Loading => Sign::Loading,
            Kids::Failed(_) => Sign::Failed,
            _ if node.link => Sign::Leaf,
            Kids::Loaded if node.children.is_empty() => Sign::Leaf,
            Kids::Loaded if node.open => Sign::Open,
            Kids::Loaded | Kids::Unread => Sign::Closed,
        };
        Some(Shown {
            name: &node.name,
            sign,
            error: match node.kids {
                Kids::Failed(code) => Some(code),
                _ => None,
            },
            truncated: node.truncated,
        })
    }

    /// The folder under the cursor.
    pub fn cursor_path(&self) -> Option<&VfsPath> {
        let row = self.rows.get(self.cursor)?;
        self.nodes.get(&row.id).map(|node| &node.path)
    }

    /// Unfolds the folders from the root down to `path` and puts the cursor there once each
    /// level on the way is read: the folders to read now. A folder of another location, or one
    /// that is gone, stops the way where it can go no further.
    pub fn reveal(&mut self, path: &VfsPath) -> Vec<VfsPath> {
        if path.backend != self.root().backend || !path.path.starts_with(&self.root().path) {
            return Vec::new();
        }
        self.target = Some(path.clone());
        self.advance()
    }

    /// Whether the tree waits for levels to show a folder.
    #[cfg(test)]
    pub fn revealing(&self) -> bool {
        self.target.is_some()
    }

    /// The engine's answer for `path`'s level: kept, or its failure kept at the folder. An
    /// answer for a folder the tree does not wait for, nor holds read, is dropped. Answers the
    /// folders to read next on the way [`DirTree::reveal`] goes.
    pub fn arrived(
        &mut self,
        path: &VfsPath,
        answer: Result<TreeLevel, &'static str>,
    ) -> Vec<VfsPath> {
        let Some(&id) = self.by_path.get(path) else {
            return Vec::new();
        };
        let waited = self
            .nodes
            .get(&id)
            .is_some_and(|node| matches!(node.kids, Kids::Loading | Kids::Loaded));
        if !waited {
            return Vec::new();
        }
        match answer {
            Ok(level) => self.fill(id, &level),
            Err(code) => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.kids = Kids::Failed(code);
                    node.open = false;
                }
            }
        }
        let next = self.advance();
        self.rebuild();
        next
    }

    /// The level of `path`, while the tree holds it read, to be asked for again: a panel showing
    /// it was read again. `None` for a folder the tree has not read.
    pub fn reload(&self, path: &VfsPath) -> Option<VfsPath> {
        let id = self.by_path.get(path)?;
        let node = self.nodes.get(id)?;
        (node.kids == Kids::Loaded).then(|| node.path.clone())
    }

    /// F2: the folder under the cursor is read again; its path, or `None` for a link.
    pub fn reread(&mut self) -> Option<VfsPath> {
        let id = self.rows.get(self.cursor)?.id;
        let node = self.nodes.get_mut(&id)?;
        if node.link || node.kids == Kids::Loading {
            return None;
        }
        node.kids = Kids::Loading;
        node.open = true;
        let path = node.path.clone();
        self.rebuild();
        Some(path)
    }

    /// A key: the cursor moves, or the folder under it unfolds or folds.
    pub fn step(&mut self, step: Step) -> Change {
        self.target = None;
        let Some(last) = self.rows.len().checked_sub(1) else {
            return Change::Nothing;
        };
        let Some(id) = self.rows.get(self.cursor).map(|row| row.id) else {
            return Change::Nothing;
        };
        let page = self.height.saturating_sub(1).max(1);
        let to = match step {
            Step::Up => self.cursor.saturating_sub(1),
            Step::Down => (self.cursor + 1).min(last),
            Step::PageUp => self.cursor.saturating_sub(page),
            Step::PageDown => (self.cursor + page).min(last),
            Step::Home => 0,
            Step::End => last,
            Step::Open => {
                let unfolded = self.nodes.get(&id).is_some_and(|node| {
                    node.open && node.kids == Kids::Loaded && !node.children.is_empty()
                });
                if !unfolded {
                    return self.unfold(id);
                }
                (self.cursor + 1).min(last)
            }
            Step::Close => {
                let Some(node) = self.nodes.get(&id) else {
                    return Change::Nothing;
                };
                if node.open {
                    return self.fold(id);
                }
                match node.parent.and_then(|parent| self.row_of(parent)) {
                    Some(row) => row,
                    None => return Change::Nothing,
                }
            }
        };
        self.move_to(to)
    }

    /// A click on row `row`: the cursor goes there.
    pub fn click(&mut self, row: usize) -> Change {
        self.target = None;
        if row >= self.rows.len() {
            return Change::Nothing;
        }
        self.move_to(row)
    }

    /// A click on the sign of row `row`: its folder unfolds or folds, the cursor stays.
    pub fn toggle(&mut self, row: usize) -> Change {
        self.target = None;
        let Some(id) = self.rows.get(row).map(|row| row.id) else {
            return Change::Nothing;
        };
        if self.nodes.get(&id).is_some_and(|node| node.open) {
            self.fold(id)
        } else {
            self.unfold(id)
        }
    }

    /// The wheel: `rows` down (up below 0); the cursor stays where it is.
    pub fn scroll(&mut self, rows: i32) {
        let last_top = self.rows.len().saturating_sub(self.height);
        let top = i64::try_from(self.top).unwrap_or(i64::MAX) + i64::from(rows);
        self.top = usize::try_from(top.max(0)).unwrap_or(0).min(last_top);
    }

    /// The window measured `rows` rows that fit.
    pub fn set_height(&mut self, rows: usize) {
        self.height = rows.max(1);
        self.keep_in_view();
    }

    fn move_to(&mut self, row: usize) -> Change {
        let before = self.cursor;
        self.cursor = row;
        self.keep_in_view();
        if self.cursor == before {
            Change::Nothing
        } else {
            Change::Cursor
        }
    }

    /// Unfolds `id`: its level is read the first time, or again after it failed.
    fn unfold(&mut self, id: u64) -> Change {
        let Some(node) = self.nodes.get_mut(&id) else {
            return Change::Nothing;
        };
        if node.link || node.kids == Kids::Loading {
            return Change::Nothing;
        }
        node.open = true;
        let load = matches!(node.kids, Kids::Unread | Kids::Failed(_));
        if load {
            node.kids = Kids::Loading;
        }
        let path = node.path.clone();
        self.rebuild();
        Change::Unfolded { path, load }
    }

    fn fold(&mut self, id: u64) -> Change {
        let Some(node) = self.nodes.get_mut(&id) else {
            return Change::Nothing;
        };
        node.open = false;
        let path = node.path.clone();
        self.rebuild();
        Change::Folded(path)
    }

    fn row_of(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|row| row.id == id)
    }

    /// The rows again after a folder folded, unfolded or got its sub-folders; the cursor stays on
    /// its folder, or on the row it was on when that folder is gone from view.
    fn rebuild(&mut self) {
        let at = self.rows.get(self.cursor).map(|row| row.id);
        self.rows.clear();
        let mut stack = vec![ROOT];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            self.rows.push(Row {
                id,
                depth: node.depth,
            });
            // A level read again shows its old sub-folders until the new ones are there.
            if node.open && !matches!(node.kids, Kids::Unread | Kids::Failed(_)) {
                stack.extend(node.children.iter().rev());
            }
        }
        let last = self.rows.len().saturating_sub(1);
        self.cursor = at
            .and_then(|id| self.row_of(id))
            .unwrap_or(self.cursor)
            .min(last);
        self.keep_in_view();
    }

    /// The top moved so that the cursor's row is on screen.
    fn keep_in_view(&mut self) {
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + self.height {
            self.top = self.cursor + 1 - self.height;
        }
        self.top = self.top.min(self.rows.len().saturating_sub(self.height));
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
