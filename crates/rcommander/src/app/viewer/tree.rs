//! A document's tree (RC-100-10): nodes with a label and an optional value, unfolded by a click
//! or the arrows as in the Explorer - Right unfolds a node or steps to its first child, Left folds
//! it or steps to its parent. A node's children are asked of the source the first time it
//! unfolds and kept when it folds again.
//!
//! The nodes are foreign input: an answer for a node that did not ask is dropped, a node id seen
//! before is dropped (so no answer can make a cycle), a label or value is kept to
//! [`MAX_TEXT_BYTES`] on one line, the tree stops growing at [`MAX_NODES`] and no node below
//! [`MAX_DEPTH`] unfolds.

use std::collections::HashMap;

use super::document::TreeNode;

/// The deepest level that unfolds; the top level is 0.
pub const MAX_DEPTH: usize = 64;
/// The most nodes a tree holds; children past it are not taken.
pub const MAX_NODES: usize = 100_000;
/// The most bytes kept of a label or a value.
pub const MAX_TEXT_BYTES: usize = 1024;
/// The rows a tree shows until the window measured it.
const FIRST_HEIGHT: usize = 20;

/// Whether a node's children are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kids {
    /// Not asked yet.
    Unknown,
    Loading,
    Loaded,
}

/// A node as the tree keeps it.
#[derive(Debug)]
struct Node {
    node: TreeNode,
    parent: Option<u64>,
    depth: usize,
    open: bool,
    kids: Kids,
    children: Vec<u64>,
}

/// A row of the unfolded tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    pub depth: usize,
}

/// What a row shows of its node.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    pub label: &'a str,
    pub value: Option<&'a str>,
    /// `None` for a leaf; `Some(open)` for a node that unfolds.
    pub open: Option<bool>,
    pub loading: bool,
}

/// A step of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    /// ArrowRight: unfold, or on to the first child.
    Open,
    /// ArrowLeft: fold, or back to the parent.
    Close,
}

#[derive(Debug)]
pub struct Tree {
    nodes: HashMap<u64, Node>,
    roots: Vec<u64>,
    top_level: Kids,
    rows: Vec<Row>,
    /// The row the cursor is on.
    pub cursor: usize,
    /// The first row on screen.
    pub top: usize,
    /// The rows that fit.
    pub height: usize,
    /// Whether children were left out at [`MAX_NODES`].
    pub full: bool,
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            roots: Vec::new(),
            top_level: Kids::Unknown,
            rows: Vec::new(),
            cursor: 0,
            top: 0,
            height: FIRST_HEIGHT,
            full: false,
        }
    }
}

impl Tree {
    /// The tree is shown: the top level to ask for (`Some(None)`) the first time.
    pub fn start(&mut self) -> Option<Option<u64>> {
        if self.top_level != Kids::Unknown {
            return None;
        }
        self.top_level = Kids::Loading;
        Some(None)
    }

    /// Whether the top level is still being asked for.
    pub fn loading(&self) -> bool {
        self.top_level == Kids::Loading
    }

    /// The rows on screen.
    pub fn visible(&self) -> &[Row] {
        let end = (self.top + self.height).min(self.rows.len());
        &self.rows[self.top.min(end)..end]
    }

    /// How many nodes the tree holds.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// What row `row` shows.
    pub fn shown(&self, row: Row) -> Option<Shown<'_>> {
        let node = self.nodes.get(&row.id)?;
        Some(Shown {
            label: &node.node.label,
            value: node.node.value.as_deref(),
            open: node.node.has_children.then_some(node.open),
            loading: node.kids == Kids::Loading,
        })
    }

    /// The source's answer for `parent`'s children (the top level for `None`). An answer nobody
    /// waits for is dropped; a failed one is the error to show, and the node may be asked again.
    pub fn arrived(
        &mut self,
        parent: Option<u64>,
        answer: Result<Vec<TreeNode>, &'static str>,
    ) -> Result<(), &'static str> {
        let (waiting, depth) = match parent {
            None => (self.top_level == Kids::Loading, 0),
            Some(id) => match self.nodes.get(&id) {
                Some(node) => (node.kids == Kids::Loading, node.depth + 1),
                None => (false, 0),
            },
        };
        if !waiting {
            return Ok(());
        }
        let children = match answer {
            Ok(children) => children,
            Err(code) => {
                self.set_kids(parent, Kids::Unknown);
                if let Some(node) = parent.and_then(|id| self.nodes.get_mut(&id)) {
                    node.open = false;
                }
                self.rebuild();
                return Err(code);
            }
        };
        let mut taken = Vec::with_capacity(children.len());
        for mut child in children {
            if self.nodes.contains_key(&child.id) || Some(child.id) == parent {
                continue;
            }
            if self.nodes.len() >= MAX_NODES {
                self.full = true;
                break;
            }
            child.label = one_line(&child.label);
            child.value = child.value.as_deref().map(one_line);
            child.has_children &= depth < MAX_DEPTH;
            taken.push(child.id);
            self.nodes.insert(
                child.id,
                Node {
                    node: child,
                    parent,
                    depth,
                    open: false,
                    kids: Kids::Unknown,
                    children: Vec::new(),
                },
            );
        }
        match parent.and_then(|id| self.nodes.get_mut(&id)) {
            Some(node) => node.children = taken,
            None => self.roots = taken,
        }
        self.set_kids(parent, Kids::Loaded);
        self.rebuild();
        Ok(())
    }

    /// A key: the cursor moves, or the node under it unfolds or folds. Answers the node whose
    /// children to ask for.
    pub fn step(&mut self, step: Step) -> Option<u64> {
        let last = self.rows.len().checked_sub(1)?;
        let page = self.height.saturating_sub(1).max(1);
        let id = self.rows.get(self.cursor)?.id;
        let mut asked = None;
        self.cursor = match step {
            Step::Up => self.cursor.saturating_sub(1),
            Step::Down => (self.cursor + 1).min(last),
            Step::PageUp => self.cursor.saturating_sub(page),
            Step::PageDown => (self.cursor + page).min(last),
            Step::Home => 0,
            Step::End => last,
            Step::Open => {
                let node = self.nodes.get(&id)?;
                let unfolded = node.open && node.kids == Kids::Loaded && !node.children.is_empty();
                if unfolded {
                    (self.cursor + 1).min(last)
                } else {
                    asked = self.unfold(id);
                    self.cursor
                }
            }
            Step::Close => {
                let node = self.nodes.get(&id)?;
                let (open, parent) = (node.open, node.parent);
                if open {
                    self.fold(id);
                    self.cursor
                } else {
                    parent
                        .and_then(|parent| self.row_of(parent))
                        .unwrap_or(self.cursor)
                }
            }
        };
        self.keep_in_view();
        asked
    }

    /// A click on row `row`: the cursor goes there, and the node unfolds or folds.
    pub fn click(&mut self, row: usize) -> Option<u64> {
        let id = self.rows.get(row)?.id;
        self.cursor = row;
        let open = self.nodes.get(&id)?.open;
        let asked = if open {
            self.fold(id);
            None
        } else {
            self.unfold(id)
        };
        self.keep_in_view();
        asked
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

    /// Unfolds `id`: its children asked for the first time, shown once there.
    fn unfold(&mut self, id: u64) -> Option<u64> {
        let node = self.nodes.get_mut(&id)?;
        if !node.node.has_children {
            return None;
        }
        node.open = true;
        let asked = (node.kids == Kids::Unknown).then(|| {
            node.kids = Kids::Loading;
            id
        });
        self.rebuild();
        asked
    }

    fn fold(&mut self, id: u64) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.open = false;
        }
        self.rebuild();
    }

    fn set_kids(&mut self, parent: Option<u64>, kids: Kids) {
        match parent.and_then(|id| self.nodes.get_mut(&id)) {
            Some(node) => node.kids = kids,
            None if parent.is_none() => self.top_level = kids,
            None => {}
        }
    }

    fn row_of(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|row| row.id == id)
    }

    /// The rows again after a node folded, unfolded or got its children; the cursor stays on its
    /// node, or on the row it was on when that node is gone from view.
    fn rebuild(&mut self) {
        let at = self.rows.get(self.cursor).map(|row| row.id);
        self.rows.clear();
        let mut stack: Vec<u64> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            self.rows.push(Row {
                id,
                depth: node.depth,
            });
            if node.open && node.kids == Kids::Loaded {
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

/// `text` kept to [`MAX_TEXT_BYTES`] and on one line: a line break or other control character
/// becomes a space.
fn one_line(text: &str) -> String {
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end]
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
