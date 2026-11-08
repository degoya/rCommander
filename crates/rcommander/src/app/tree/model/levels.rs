//! The levels of a folder tree (RC-100-88): the way [`DirTree::reveal`] goes down to a folder,
//! and a level read - or read again - taken in.

use std::{collections::HashMap, path::Component};

use rc_engine::{VfsPath, tree::TreeLevel};

use super::{DirTree, Kids, MAX_NODES, Node, ROOT};

impl DirTree {
    /// The way to the target, as far as it is read: each folder on it unfolded, the first one
    /// not read yet asked for. Once the way ends - at the target, at a folder that is gone, a
    /// link or a failure - the cursor goes to where it ended.
    pub(super) fn advance(&mut self) -> Vec<VfsPath> {
        let Some(target) = self.target.clone() else {
            return Vec::new();
        };
        let Ok(rest) = target.path.strip_prefix(&self.root().path) else {
            self.target = None;
            return Vec::new();
        };
        let names: Vec<String> = rest
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        // Every folder on the way unfolds, the target too: its sub-folders show, as in Total
        // Commander.
        let mut at = ROOT;
        let mut names = names.iter();
        while let Some(node) = self.nodes.get_mut(&at) {
            if node.link {
                break;
            }
            match node.kids {
                Kids::Loaded => node.open = true,
                Kids::Unread => {
                    node.kids = Kids::Loading;
                    node.open = true;
                    let path = node.path.clone();
                    self.rebuild();
                    return vec![path];
                }
                Kids::Loading => {
                    node.open = true;
                    self.rebuild();
                    return Vec::new();
                }
                Kids::Failed(_) => break,
            }
            let Some(name) = names.next() else {
                break;
            };
            match self.child_named(at, name) {
                Some(child) => at = child,
                None => break,
            }
        }
        self.target = None;
        self.rebuild();
        if let Some(row) = self.row_of(at) {
            self.cursor = row;
            self.keep_in_view();
        }
        Vec::new()
    }

    /// The sub-folder `name` of `id`: by its name, or on Windows by its name in any case, as the
    /// file system finds it.
    fn child_named(&self, id: u64, name: &str) -> Option<u64> {
        let children = &self.nodes.get(&id)?.children;
        let named = |exact: bool| {
            children.iter().copied().find(|child| {
                self.nodes.get(child).is_some_and(|node| {
                    if exact {
                        node.name == name
                    } else {
                        node.name.eq_ignore_ascii_case(name)
                    }
                })
            })
        };
        named(true).or_else(|| named(false).filter(|_| cfg!(windows)))
    }

    /// `id`'s sub-folders from `level`: those still there keep what was read below them, the
    /// others go with everything below them.
    pub(super) fn fill(&mut self, id: u64, level: &TreeLevel) {
        let Some(parent) = self.nodes.get(&id) else {
            return;
        };
        let (base, depth) = (parent.path.clone(), parent.depth + 1);
        let mut old: HashMap<String, u64> = parent
            .children
            .iter()
            .filter_map(|child| Some((self.nodes.get(child)?.name.clone(), *child)))
            .collect();
        let mut taken = Vec::with_capacity(level.children.len());
        for child in level.children.iter() {
            if let Some(kept) = old.remove(&child.name) {
                taken.push(kept);
                continue;
            }
            if self.nodes.len() >= MAX_NODES {
                self.full = true;
                break;
            }
            let path = base.join(&child.name);
            let new = self.next_id;
            self.next_id += 1;
            self.by_path.insert(path.clone(), new);
            self.nodes.insert(
                new,
                Node {
                    name: child.name.clone(),
                    path,
                    parent: Some(id),
                    depth,
                    open: false,
                    link: child.link,
                    kids: Kids::Unread,
                    truncated: false,
                    children: Vec::new(),
                },
            );
            taken.push(new);
        }
        for gone in old.into_values() {
            self.remove(gone);
        }
        if let Some(node) = self.nodes.get_mut(&id) {
            node.children = taken;
            node.kids = Kids::Loaded;
            node.truncated = level.truncated;
        }
    }

    /// `id` and everything below it go.
    fn remove(&mut self, id: u64) {
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if let Some(node) = self.nodes.remove(&id) {
                self.by_path.remove(&node.path);
                stack.extend(node.children);
            }
        }
    }
}
