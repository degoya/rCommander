//! `connections_list` in pages (RC-110-61): the tree's items in display order - a folder, then
//! what it holds: its folders, its connections, its credentials; the root's after its folders -
//! at most `limit` of them per page. A page is still a tree: the folders above an item on it
//! stand around it as its context, holding only what is on the page.

use rc_engine::{ConnectionTree, FolderNode};
use rmcp::schemars;
use serde::Deserialize;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListParams {
    /// Items (folders, connections, credentials) per page: 1000 when omitted, at most 1000.
    #[serde(default)]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: Option<usize>,
    /// The next_cursor of the previous page.
    #[serde(default)]
    pub cursor: Option<String>,
}

/// The items `from..to` of the walk through the tree, counting as it goes.
struct Window {
    from: usize,
    to: usize,
    at: usize,
}

impl Window {
    /// Whether the next item is on the page.
    fn takes(&mut self) -> bool {
        let at = self.at;
        self.at += 1;
        (self.from..self.to).contains(&at)
    }
}

/// How many items `tree` has.
pub(crate) fn items(tree: &ConnectionTree) -> usize {
    fn count(node: &FolderNode) -> usize {
        1 + node.folders.iter().map(count).sum::<usize>()
            + node.connections.len()
            + node.credentials.len()
    }
    tree.folders.iter().map(count).sum::<usize>() + tree.connections.len() + tree.credentials.len()
}

/// The items `from..to` of `tree`, with the folders above them.
pub(crate) fn page(tree: &ConnectionTree, from: usize, to: usize) -> ConnectionTree {
    let mut window = Window { from, to, at: 0 };
    let mut page = tree.clone();
    page.folders = tree
        .folders
        .iter()
        .filter_map(|node| folder(node, &mut window))
        .collect();
    page.connections = keep(&tree.connections, &mut window);
    page.credentials = keep(&tree.credentials, &mut window);
    page
}

/// `node` with what of it is on the page; `None` when nothing is.
fn folder(node: &FolderNode, window: &mut Window) -> Option<FolderNode> {
    let own = window.takes();
    let folders: Vec<FolderNode> = node
        .folders
        .iter()
        .filter_map(|inner| folder(inner, window))
        .collect();
    let connections = keep(&node.connections, window);
    let credentials = keep(&node.credentials, window);
    let shown = own || !folders.is_empty() || !connections.is_empty() || !credentials.is_empty();
    shown.then(|| FolderNode {
        folder: node.folder.clone(),
        folders,
        connections,
        credentials,
    })
}

fn keep<T: Clone>(items: &[T], window: &mut Window) -> Vec<T> {
    items.iter().filter(|_| window.takes()).cloned().collect()
}

#[cfg(test)]
#[path = "connections_page_tests.rs"]
mod tests;
