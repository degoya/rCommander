//! What a file says about itself, as a small tree built once in `open`: sections (`File`,
//! `Audio`, `Tracks`, `Tags`, `Camera`, ...) of `label: value` facts. The `tree` output hands it
//! out a level at a time; the `text` output writes the same facts as indented `label: value`
//! lines, for copying and searching.
//!
//! Everything a file can make large is capped here: a value at [`MAX_VALUE_CHARS`] characters,
//! the tree at [`MAX_NODES`] nodes, and control characters (a line break in a tag) become spaces,
//! so one line of text is one fact.

use crate::types::{Failure, FailureKind, Sink, TreeNode};

/// The most characters of one value; a longer one ends in `...`.
pub const MAX_VALUE_CHARS: usize = 512;
/// The most nodes of one document's tree; the facts after it are left out.
pub const MAX_NODES: usize = 20_000;
/// The most bytes one `write-at` moves.
const WRITE_CHUNK: usize = 1 << 20;

/// One fact, or a section of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub label: String,
    pub value: Option<String>,
    pub children: Vec<Node>,
}

impl Node {
    /// A fact: `label: value`.
    #[must_use]
    pub fn fact(label: impl Into<String>, value: impl Into<String>) -> Self {
        let (label, value): (String, String) = (label.into(), value.into());
        Self {
            label: clean(&label),
            value: Some(clean(&value)),
            children: Vec::new(),
        }
    }

    /// A section of `children`, with an optional value of its own.
    #[must_use]
    pub fn group(label: impl Into<String>, value: Option<String>, children: Vec<Node>) -> Self {
        let label: String = label.into();
        Self {
            label: clean(&label),
            value: value.map(|value| clean(&value)),
            children,
        }
    }

    /// The child labelled `label`, for the tests and the title.
    #[must_use]
    pub fn find(&self, label: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.label == label)
    }
}

/// `text` without control characters and at most [`MAX_VALUE_CHARS`] characters.
#[must_use]
pub fn clean(text: &str) -> String {
    let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let mut out = String::with_capacity(trimmed.len().min(MAX_VALUE_CHARS * 4));
    for (count, c) in trimmed.chars().enumerate() {
        if count == MAX_VALUE_CHARS {
            out.push_str("...");
            break;
        }
        out.push(if c.is_control() { ' ' } else { c });
    }
    out
}

struct Flat {
    label: String,
    value: Option<String>,
    children: Vec<u64>,
}

/// The tree of one document, flattened: a node's id is its index.
pub struct Outline {
    nodes: Vec<Flat>,
    roots: Vec<u64>,
}

impl Outline {
    /// Flattens `roots`, keeping at most [`MAX_NODES`] nodes.
    #[must_use]
    pub fn new(roots: Vec<Node>) -> Self {
        let mut outline = Self {
            nodes: Vec::new(),
            roots: Vec::new(),
        };
        for root in roots {
            if let Some(id) = outline.push(root) {
                outline.roots.push(id);
            }
        }
        outline
    }

    /// Adds `node` and its children; `None` once the tree is full.
    fn push(&mut self, node: Node) -> Option<u64> {
        if self.nodes.len() >= MAX_NODES {
            return None;
        }
        let id = self.nodes.len() as u64;
        self.nodes.push(Flat {
            label: node.label,
            value: node.value,
            children: Vec::new(),
        });
        let mut children = Vec::new();
        for child in node.children {
            match self.push(child) {
                Some(child_id) => children.push(child_id),
                None => break,
            }
        }
        if let Some(flat) = usize::try_from(id)
            .ok()
            .and_then(|at| self.nodes.get_mut(at))
        {
            flat.children = children;
        }
        Some(id)
    }

    /// The number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no node.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// `children`: the sections for `None`, else the children of the node with that id.
    pub fn children(&self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        let ids = match node {
            None => &self.roots,
            Some(id) => {
                &self
                    .get(id)
                    .ok_or_else(|| Failure::of(FailureKind::NotFound))?
                    .children
            }
        };
        Ok(ids
            .iter()
            .filter_map(|id| {
                self.get(*id).map(|flat| TreeNode {
                    id: *id,
                    label: flat.label.clone(),
                    value: flat.value.clone(),
                    has_children: !flat.children.is_empty(),
                })
            })
            .collect())
    }

    fn get(&self, id: u64) -> Option<&Flat> {
        usize::try_from(id).ok().and_then(|at| self.nodes.get(at))
    }

    /// The facts as text: a line per node, `label: value`, two spaces deeper per level, a blank
    /// line between sections.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (index, root) in self.roots.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            self.render_node(*root, 0, &mut out);
        }
        out
    }

    fn render_node(&self, id: u64, depth: usize, out: &mut String) {
        let Some(flat) = self.get(id) else {
            return;
        };
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str(&flat.label);
        if let Some(value) = &flat.value {
            out.push_str(": ");
            out.push_str(value);
        }
        out.push('\n');
        for child in &flat.children {
            self.render_node(*child, depth + 1, out);
        }
    }
}

/// Writes `bytes` into `out` in windows of at most 1 MiB.
pub fn write_all(out: &mut impl Sink, bytes: &[u8]) -> Result<(), Failure> {
    let mut offset = 0u64;
    for chunk in bytes.chunks(WRITE_CHUNK) {
        out.write_at(offset, chunk)?;
        offset += chunk.len() as u64;
    }
    Ok(())
}

#[cfg(test)]
#[path = "outline_tests.rs"]
mod tests;
