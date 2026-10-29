//! The sections and tables of a document, found by the open pass: the nodes of the `tree` beside
//! its entries. A node knows the run of entries (key lines) under its header by their numbers,
//! so the tree reads them from the line index when it is opened and never keeps them.
//!
//! INI and registry sections are one level, a node each, in the order of the file. TOML tables
//! nest by the dots of their names (`[a.b]` under `a`, made when only `[a.b]` names it), an array
//! of tables is a node with an element `[0]`, `[1]`, ... per `[[a]]`, and a table named after one
//! (`[a.b]` after `[[a]]`) belongs to its last element, as TOML has it. The other formats have no
//! headers: their entries are the top level.
//!
//! At most [`MAX_NODES`] nodes are kept, so memory stays bounded whatever the file; the headers
//! after that are counted ([`Outline::hidden`]) and their entries are not in the tree.

use std::collections::HashMap;

use crate::tree::preview;

/// The most nodes of one document.
pub const MAX_NODES: usize = 1 << 14;
/// The most bytes of a TOML name's part kept for finding its table again.
const MAX_SEGMENT_BYTES: usize = 256;
/// The most levels a TOML name nests; the rest of a deeper name is its last part.
const MAX_SEGMENTS: usize = 32;

#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) label: String,
    /// The number of the first entry under the node's header.
    pub(crate) first_entry: u64,
    /// The entries under its header, before the next one.
    pub(crate) entries: u64,
    pub(crate) children: Vec<u32>,
    /// An array of tables: its children are its elements.
    array: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Outline {
    pub(crate) nodes: Vec<Node>,
    /// The nodes of the top level, after its entries.
    pub(crate) top: Vec<u32>,
    /// The entries before the first header: the top level's.
    pub(crate) root_entries: u64,
    /// Headers past [`MAX_NODES`], not in the tree.
    pub(crate) hidden: u64,
}

impl Outline {
    pub(crate) fn node(&self, index: u32) -> Option<&Node> {
        self.nodes.get(index as usize)
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.nodes.capacity() * size_of::<Node>()
            + self.top.capacity() * size_of::<u32>()
            + self
                .nodes
                .iter()
                .map(|node| node.label.capacity() + node.children.capacity() * size_of::<u32>())
                .sum::<usize>()
    }
}

/// Builds the [`Outline`] as the open pass meets the headers.
#[derive(Debug)]
pub(crate) struct OutlineBuilder {
    outline: Outline,
    /// TOML: names nest by their dots.
    nested: bool,
    /// The node whose entries are being counted.
    current: Option<u32>,
    seen_header: bool,
    /// TOML: the node of a name's part under its parent (0 the top level, else the node + 1).
    tables: HashMap<(u32, String), u32>,
}

impl OutlineBuilder {
    pub(crate) fn new(nested: bool) -> Self {
        Self {
            outline: Outline::default(),
            nested,
            current: None,
            seen_header: false,
            tables: HashMap::new(),
        }
    }

    /// A header line, `[name]` (`array`: `[[name]]`), after `entries` entries.
    pub(crate) fn header(&mut self, array: bool, name: &str, entries: u64) {
        self.close(entries);
        if !self.seen_header {
            self.outline.root_entries = entries;
            self.seen_header = true;
        }
        let node = if self.nested {
            self.table(array, name)
        } else {
            let label = if name.is_empty() { "[]" } else { name };
            self.add(None, preview(label, false))
        };
        match node {
            Some(index) => {
                if let Some(node) = self.outline.nodes.get_mut(index as usize) {
                    node.first_entry = entries;
                }
                self.current = Some(index);
            }
            None => self.outline.hidden += 1,
        }
    }

    /// The outline, `entries` entries in all.
    pub(crate) fn finish(mut self, entries: u64) -> Outline {
        self.close(entries);
        if !self.seen_header {
            self.outline.root_entries = entries;
        }
        self.outline.nodes.shrink_to_fit();
        self.outline.top.shrink_to_fit();
        self.outline
    }

    /// The entries counted so far belong to the current node.
    fn close(&mut self, entries: u64) {
        if let Some(node) = self
            .current
            .take()
            .and_then(|index| self.outline.nodes.get_mut(index as usize))
        {
            node.entries = entries.saturating_sub(node.first_entry);
        }
    }

    fn add(&mut self, parent: Option<u32>, label: String) -> Option<u32> {
        if self.outline.nodes.len() >= MAX_NODES {
            return None;
        }
        let index = u32::try_from(self.outline.nodes.len()).ok()?;
        self.outline.nodes.push(Node {
            label,
            first_entry: 0,
            entries: 0,
            children: Vec::new(),
            array: false,
        });
        match parent.and_then(|parent| self.outline.nodes.get_mut(parent as usize)) {
            Some(parent) => parent.children.push(index),
            None => self.outline.top.push(index),
        }
        Some(index)
    }

    /// The node of TOML header `name`: its table, or a new element of its array of tables.
    fn table(&mut self, array: bool, name: &str) -> Option<u32> {
        let segments = key_path(name);
        let (last, parents) = segments.split_last()?;
        let mut parent = None;
        for segment in parents {
            parent = Some(self.descend(parent, segment)?);
        }
        if !array {
            return self.descend(parent, last);
        }
        let list = self.find_or_add(parent, last)?;
        let index = self.outline.nodes.get_mut(list as usize).map(|node| {
            node.array = true;
            node.children.len()
        })?;
        self.add(Some(list), format!("[{index}]"))
    }

    /// The table `segment` under `parent`; of an array of tables, its last element.
    fn descend(&mut self, parent: Option<u32>, segment: &str) -> Option<u32> {
        let index = self.find_or_add(parent, segment)?;
        let node = self.outline.nodes.get(index as usize)?;
        Some(match node.children.last() {
            Some(element) if node.array => *element,
            _ => index,
        })
    }

    fn find_or_add(&mut self, parent: Option<u32>, segment: &str) -> Option<u32> {
        let key = (parent.map_or(0, |parent| parent + 1), segment.to_owned());
        if let Some(index) = self.tables.get(&key) {
            return Some(*index);
        }
        let index = self.add(parent, preview(segment, false))?;
        self.tables.insert(key, index);
        Some(index)
    }
}

/// The parts of a TOML key: `a.b."c.d".'e'` is `a`, `b`, `c.d`, `e`; quotes are taken off, an
/// escape is kept as written. Each part at most [`MAX_SEGMENT_BYTES`], at most
/// [`MAX_SEGMENTS`] parts.
pub(crate) fn key_path(name: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut part = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in name.chars() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                    part.push(ch);
                } else if ch == '\\' && open == '"' {
                    escaped = true;
                    part.push(ch);
                } else if ch == open {
                    quote = None;
                } else {
                    part.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '.' => parts.push(std::mem::take(&mut part)),
                _ if ch.is_whitespace() => {}
                _ => part.push(ch),
            },
        }
    }
    parts.push(part);
    if parts.len() > MAX_SEGMENTS {
        let rest = parts.split_off(MAX_SEGMENTS - 1).join(".");
        parts.push(rest);
    }
    for part in &mut parts {
        if part.len() > MAX_SEGMENT_BYTES {
            let mut end = MAX_SEGMENT_BYTES;
            while !part.is_char_boundary(end) {
                end -= 1;
            }
            part.truncate(end);
        }
    }
    parts
}

#[cfg(test)]
#[path = "outline_tests.rs"]
mod tests;
