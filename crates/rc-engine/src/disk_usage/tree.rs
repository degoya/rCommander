//! The tree of a disk-usage walk: one node per folder with the sums below it and those of its
//! own files, the largest files in a bounded heap, and how the walk ended.

use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use rc_ops::{Counted, DirUsage, UsageVisitor};
use rc_vfs::{Entry, VfsPath};
use serde::{Deserialize, Serialize};

use super::{DiskUsageReport, FolderUsage};
use crate::recover::LockOrRecover as _;
use crate::search::SearchCancel;

/// What the sizes are shown and sorted by (`disk_usage.size`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SizeBy {
    /// The files' lengths.
    #[default]
    Bytes,
    /// What they occupy on the volume; unknown where the backend does not tell.
    Allocated,
}

impl SizeBy {
    /// The setting's value as a choice; `bytes` for anything else.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        if value == "allocated" {
            Self::Allocated
        } else {
            Self::Bytes
        }
    }

    /// The size of `usage` by this choice; `None` when the backend did not tell it.
    #[must_use]
    pub fn of(self, usage: &DirUsage) -> Option<u64> {
        match self {
            Self::Bytes => Some(usage.bytes),
            Self::Allocated => usage.allocated,
        }
    }
}

/// A folder of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageNode {
    /// Its name; empty for the start.
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Everything below it.
    pub usage: DirUsage,
    /// Its own files only.
    pub own: DirUsage,
    /// It could not be listed.
    pub unreadable: bool,
}

impl UsageNode {
    fn new(name: String, parent: Option<usize>) -> Self {
        let empty = DirUsage {
            allocated: Some(0),
            ..DirUsage::default()
        };
        Self {
            name,
            parent,
            children: Vec::new(),
            usage: empty,
            own: empty,
            unreadable: false,
        }
    }
}

/// One of the largest files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeFile {
    /// Relative to the folder analysed.
    pub path: String,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocated: Option<u64>,
}

/// A row of a folder's list: one of its subfolders, or its own files (`node: None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub node: Option<usize>,
    pub name: String,
    pub usage: DirUsage,
    /// Its share of the folder by the size asked, in thousandths.
    pub share: u32,
    pub unreadable: bool,
}

/// A file in the heap of the largest: the smallest and, among equals, the latest found leaves
/// first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Big {
    bytes: u64,
    order: Reverse<u64>,
    path: String,
    allocated: Option<u64>,
}

/// How the walk stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stand {
    Walking,
    Done,
    Stopped,
    Failed(&'static str),
}

/// The tree of one walk.
#[derive(Debug)]
pub struct UsageTree {
    root: VfsPath,
    nodes: Vec<UsageNode>,
    largest: BinaryHeap<Reverse<Big>>,
    keep: usize,
    max_folders: usize,
    found: u64,
    truncated: bool,
    stand: Stand,
}

impl UsageTree {
    /// The start's node.
    pub const ROOT: usize = 0;

    /// An empty tree of `root`, keeping the `keep` largest files and at most `max_folders`
    /// folders.
    #[must_use]
    pub fn new(root: VfsPath, keep: usize, max_folders: usize) -> Self {
        Self {
            root,
            nodes: vec![UsageNode::new(String::new(), None)],
            largest: BinaryHeap::new(),
            keep,
            max_folders: max_folders.max(1),
            found: 0,
            truncated: false,
            stand: Stand::Walking,
        }
    }

    /// The folder analysed.
    #[must_use]
    pub fn root(&self) -> &VfsPath {
        &self.root
    }

    #[must_use]
    pub fn node(&self, id: usize) -> Option<&UsageNode> {
        self.nodes.get(id)
    }

    /// The folders kept so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Whether the walk ended.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.stand != Stand::Walking
    }

    /// Whether a stop ended it before its end.
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.stand == Stand::Stopped
    }

    /// The error code of a start that could not be read.
    #[must_use]
    pub fn failed(&self) -> Option<&'static str> {
        match self.stand {
            Stand::Failed(code) => Some(code),
            _ => None,
        }
    }

    /// Whether it stopped at its folder limit.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The path of folder `id` relative to the start.
    #[must_use]
    pub fn relative(&self, id: usize) -> PathBuf {
        let mut names = Vec::new();
        let mut at = Some(id);
        while let Some(node) = at.and_then(|id| self.nodes.get(id)) {
            if node.parent.is_some() {
                names.push(node.name.as_str());
            }
            at = node.parent;
        }
        names.iter().rev().collect()
    }

    /// Where folder `id` is.
    #[must_use]
    pub fn path_of(&self, id: usize) -> VfsPath {
        self.root.join(self.relative(id))
    }

    /// The rows of folder `id`: its subfolders and its own files, largest first by `by` (an
    /// unknown size last), each with its share of the folder.
    #[must_use]
    pub fn rows(&self, id: usize, by: SizeBy) -> Vec<FolderRow> {
        let Some(node) = self.nodes.get(id) else {
            return Vec::new();
        };
        let whole = by.of(&node.usage).unwrap_or(0);
        let share = |usage: &DirUsage| -> u32 {
            match (by.of(usage), whole) {
                (Some(part), whole) if whole > 0 => {
                    u32::try_from(u128::from(part) * 1000 / u128::from(whole)).unwrap_or(1000)
                }
                _ => 0,
            }
        };
        let mut rows: Vec<FolderRow> = node
            .children
            .iter()
            .filter_map(|&child| self.nodes.get(child).map(|folder| (child, folder)))
            .map(|(child, folder)| FolderRow {
                node: Some(child),
                name: folder.name.clone(),
                usage: folder.usage,
                share: share(&folder.usage),
                unreadable: folder.unreadable,
            })
            .collect();
        if node.own.files > 0 || node.own.hard_links > 0 {
            rows.push(FolderRow {
                node: None,
                name: String::new(),
                usage: node.own,
                share: share(&node.own),
                unreadable: false,
            });
        }
        rows.sort_by_key(|row| Reverse(by.of(&row.usage)));
        rows
    }

    /// The largest files kept, largest first.
    #[must_use]
    pub fn largest(&self) -> Vec<LargeFile> {
        let mut files: Vec<&Big> = self.largest.iter().map(|Reverse(big)| big).collect();
        files.sort_by(|a, b| b.cmp(a));
        files
            .into_iter()
            .map(|big| LargeFile {
                path: big.path.clone(),
                bytes: big.bytes,
                allocated: big.allocated,
            })
            .collect()
    }

    /// The report of `files.disk_usage`: subfolders to `depth` levels, `limit` per level and
    /// `limit` largest files.
    pub(crate) fn report(&self, depth: u32, limit: usize) -> DiskUsageReport {
        let (folders, more) = self.folders(Self::ROOT, depth, limit);
        DiskUsageReport {
            path: self.root.clone(),
            usage: self.nodes[Self::ROOT].usage,
            folders,
            more,
            largest: self.largest().into_iter().take(limit).collect(),
            truncated: self.truncated,
        }
    }

    /// The subfolders of `id` largest first, `limit` of them with theirs to `depth` levels, and
    /// how many were left out.
    fn folders(&self, id: usize, depth: u32, limit: usize) -> (Vec<FolderUsage>, usize) {
        if depth == 0 {
            return (Vec::new(), 0);
        }
        let rows: Vec<FolderRow> = self
            .rows(id, SizeBy::Bytes)
            .into_iter()
            .filter(|row| row.node.is_some())
            .collect();
        let more = rows.len().saturating_sub(limit);
        let folders = rows
            .into_iter()
            .take(limit)
            .filter_map(|row| {
                let child = row.node?;
                let (folders, more) = self.folders(child, depth - 1, limit);
                Some(FolderUsage {
                    name: row.name,
                    usage: row.usage,
                    folders,
                    more,
                })
            })
            .collect();
        (folders, more)
    }

    /// A folder `name` met in `parent`; `None` past the folder limit, which cuts the tree.
    fn add_folder(&mut self, parent: usize, name: &str) -> Option<usize> {
        if self.nodes.len() >= self.max_folders {
            self.truncated = true;
            return None;
        }
        let id = self.nodes.len();
        self.nodes
            .push(UsageNode::new(name.to_owned(), Some(parent)));
        if let Some(node) = self.nodes.get_mut(parent) {
            node.children.push(id);
        }
        self.up(parent, |usage| usage.dirs += 1);
        Some(id)
    }

    /// An entry of `parent` that is no folder: counted, or another name of a counted file.
    fn add_file(&mut self, parent: usize, name: &str, counted: Option<Counted>) {
        let Some(counted) = counted else {
            if let Some(node) = self.nodes.get_mut(parent) {
                node.own.hard_links += 1;
            }
            self.up(parent, |usage| usage.hard_links += 1);
            return;
        };
        let add = |usage: &mut DirUsage| {
            usage.files += 1;
            usage.bytes += counted.bytes;
            usage.allocated = usage
                .allocated
                .zip(counted.allocated)
                .map(|(sum, more)| sum + more);
        };
        if let Some(node) = self.nodes.get_mut(parent) {
            add(&mut node.own);
        }
        self.up(parent, add);
        self.offer(parent, name, counted);
    }

    fn mark_unreadable(&mut self, folder: usize) {
        if let Some(node) = self.nodes.get_mut(folder) {
            node.unreadable = true;
        }
        self.up(folder, |usage| usage.unreadable += 1);
    }

    /// `change` on folder `id` and every folder above it.
    fn up(&mut self, id: usize, change: impl Fn(&mut DirUsage)) {
        let mut at = Some(id);
        while let Some(node) = at.and_then(|id| self.nodes.get_mut(id)) {
            change(&mut node.usage);
            at = node.parent;
        }
    }

    /// A file to the heap of the largest, when it is among them.
    fn offer(&mut self, parent: usize, name: &str, counted: Counted) {
        if self.keep == 0 {
            return;
        }
        let smallest = self.largest.peek().map(|Reverse(big)| big.bytes);
        if self.largest.len() >= self.keep && smallest.is_some_and(|bytes| counted.bytes <= bytes) {
            return;
        }
        self.found += 1;
        let path = self.relative(parent).join(name);
        self.largest.push(Reverse(Big {
            bytes: counted.bytes,
            order: Reverse(self.found),
            path: path_text(&path),
            allocated: counted.allocated,
        }));
        if self.largest.len() > self.keep {
            self.largest.pop();
        }
    }

    pub(super) fn finish(&mut self, stopped: bool) {
        self.stand = if stopped { Stand::Stopped } else { Stand::Done };
    }

    pub(super) fn fail(&mut self, code: &'static str) {
        self.stand = Stand::Failed(code);
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The walk's visitor: every folder and file into the shared tree, until the stop.
pub(super) struct Tally {
    tree: Arc<Mutex<UsageTree>>,
    cancel: SearchCancel,
}

impl Tally {
    pub(super) fn new(tree: Arc<Mutex<UsageTree>>, cancel: SearchCancel) -> Self {
        Self { tree, cancel }
    }

    fn tree(&self) -> MutexGuard<'_, UsageTree> {
        self.tree.lock_or_recover("disk_usage.tree")
    }
}

impl UsageVisitor for Tally {
    type Folder = usize;

    fn folder(&mut self, parent: &usize, entry: &Entry, _: &Path) -> ControlFlow<(), usize> {
        if self.cancel.is_cancelled() {
            return ControlFlow::Break(());
        }
        match self.tree().add_folder(*parent, &entry.name) {
            Some(id) => ControlFlow::Continue(id),
            None => ControlFlow::Break(()),
        }
    }

    fn file(&mut self, parent: &usize, entry: &Entry, counted: Option<Counted>) -> ControlFlow<()> {
        if self.cancel.is_cancelled() {
            return ControlFlow::Break(());
        }
        self.tree().add_file(*parent, &entry.name, counted);
        ControlFlow::Continue(())
    }

    fn unreadable(&mut self, folder: &usize) {
        self.tree().mark_unreadable(*folder);
    }
}
