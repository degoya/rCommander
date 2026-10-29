//! The manager's left side: the connection tree as rows, the filter, the cursor and the folded
//! folders. The tree is the engine's (`connections.list`), read again after every change; this
//! keeps only what the window adds - which folders are closed and where the cursor stands, by
//! node rather than by row, so a reload keeps both. A folder lists its folders, its connections,
//! then its shared credentials (RC-050-12) - those only while the setting offers them.

use std::collections::HashSet;

use rc_engine::{
    Connection, ConnectionId, ConnectionTree, Credential, CredentialId, FolderId, FolderNode,
    TreeItem,
};

/// One visible row; its texts are the tree's own, not copied for every frame (RC-080-53).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<'a> {
    pub item: TreeItem,
    /// Folders above it.
    pub depth: usize,
    pub name: &'a str,
    /// A connection's host, a credential's user; empty for a folder.
    pub detail: &'a str,
    /// A folder whose rows are shown.
    pub open: bool,
    /// The folder it sits in; `None` at the root.
    pub parent: Option<FolderId>,
}

/// The tree as the window shows it.
#[derive(Debug, Clone, Default)]
pub struct TreeState {
    pub tree: ConnectionTree,
    /// Folders the person closed; every other one is open.
    closed: HashSet<FolderId>,
    /// Only connections whose name or host contains this (ignoring case), and their folders.
    pub filter: String,
    pub cursor: Option<TreeItem>,
    /// The setting `vault.shared_credentials`: the credentials are rows, and connections can
    /// point at them.
    pub offer_credentials: bool,
}

impl TreeState {
    /// The rows in display order: per level the folders, then the connections.
    pub fn rows(&self) -> Vec<Row<'_>> {
        let needle = self.filter.trim().to_lowercase();
        // What the filter shows, each name lowercased once; `None` shows everything.
        let shown = (!needle.is_empty()).then(|| Matches::of(&self.tree, &needle));
        let mut rows = Vec::new();
        let level = Level {
            folders: &self.tree.folders,
            connections: &self.tree.connections,
            credentials: &self.tree.credentials,
        };
        self.push_level(level, None, 0, shown.as_ref(), &mut rows);
        rows
    }

    fn push_level<'a>(
        &self,
        level: Level<'a>,
        parent: Option<FolderId>,
        depth: usize,
        shown: Option<&Matches>,
        rows: &mut Vec<Row<'a>>,
    ) {
        let Level {
            folders,
            connections,
            credentials,
        } = level;
        for node in folders {
            let id = node.folder.id;
            if shown.is_some_and(|shown| !shown.folders.contains(&id)) {
                continue;
            }
            // A filter opens every folder it shows.
            let open = shown.is_some() || !self.closed.contains(&id);
            rows.push(Row {
                item: TreeItem::Folder(id),
                depth,
                name: &node.folder.name,
                detail: "",
                open,
                parent,
            });
            if open {
                // Inside a folder whose own name matches, everything shows.
                let inner = shown.filter(|shown| !shown.named.contains(&id));
                let level = Level {
                    folders: &node.folders,
                    connections: &node.connections,
                    credentials: &node.credentials,
                };
                self.push_level(level, Some(id), depth + 1, inner, rows);
            }
        }
        for connection in connections {
            if shown
                .is_some_and(|shown| !shown.items.contains(&TreeItem::Connection(connection.id)))
            {
                continue;
            }
            rows.push(Row {
                item: TreeItem::Connection(connection.id),
                depth,
                name: &connection.name,
                detail: &connection.host,
                open: false,
                parent,
            });
        }
        if !self.offer_credentials {
            return;
        }
        for credential in credentials {
            if shown
                .is_some_and(|shown| !shown.items.contains(&TreeItem::Credential(credential.id)))
            {
                continue;
            }
            rows.push(Row {
                item: TreeItem::Credential(credential.id),
                depth,
                name: &credential.name,
                detail: credential.username.as_deref().unwrap_or_default(),
                open: false,
                parent,
            });
        }
    }

    /// The cursor's row in `rows`; the first row when the cursor is on none of them.
    pub fn cursor_index(&self, rows: &[Row]) -> Option<usize> {
        if rows.is_empty() {
            return None;
        }
        Some(
            self.cursor
                .and_then(|item| rows.iter().position(|row| row.item == item))
                .unwrap_or(0),
        )
    }

    /// Moves the cursor `by` rows, stopping at the ends; the item now under it.
    pub fn step(&mut self, by: isize) -> Option<TreeItem> {
        let rows = self.rows();
        let at = self.cursor_index(&rows)?;
        let last = rows.len() - 1;
        let to = at.saturating_add_signed(by).min(last);
        self.cursor = Some(rows[to].item);
        self.cursor
    }

    /// The cursor to the first (`end` false) or the last row.
    pub fn jump(&mut self, end: bool) -> Option<TreeItem> {
        let rows = self.rows();
        let row = if end { rows.last() } else { rows.first() };
        self.cursor = row.map(|row| row.item);
        self.cursor
    }

    /// The item under the cursor, as the rows show it.
    pub fn current(&self) -> Option<TreeItem> {
        let rows = self.rows();
        self.cursor_index(&rows).map(|at| rows[at].item)
    }

    /// Opens or closes `folder`; `None` switches.
    pub fn fold(&mut self, folder: FolderId, open: Option<bool>) {
        let open = open.unwrap_or_else(|| self.closed.contains(&folder));
        if open {
            self.closed.remove(&folder);
        } else {
            self.closed.insert(folder);
        }
    }

    pub fn is_open(&self, folder: FolderId) -> bool {
        !self.closed.contains(&folder)
    }

    /// The folder new things go into from `item`: a folder itself, a connection's folder, the
    /// root for none.
    pub fn target_folder(&self, item: Option<TreeItem>) -> Option<FolderId> {
        match item {
            Some(TreeItem::Folder(id)) => Some(id),
            Some(TreeItem::Connection(id)) => self.connection(id).and_then(|c| c.folder_id),
            Some(TreeItem::Credential(id)) => self.credential(id).and_then(|c| c.folder_id),
            None => None,
        }
    }

    /// The folder `item` sits in.
    pub fn parent_of(&self, item: TreeItem) -> Option<FolderId> {
        match item {
            TreeItem::Folder(id) => self.folder(id).and_then(|node| node.folder.parent_id),
            TreeItem::Connection(id) => self.connection(id).and_then(|c| c.folder_id),
            TreeItem::Credential(id) => self.credential(id).and_then(|c| c.folder_id),
        }
    }

    /// The shared credential `id`, in whichever folder.
    pub fn credential(&self, id: CredentialId) -> Option<&Credential> {
        self.credentials()
            .into_iter()
            .find(|credential| credential.id == id)
    }

    /// Every shared credential, the root's first, then folder by folder.
    pub fn credentials(&self) -> Vec<&Credential> {
        fn gather<'a>(nodes: &'a [FolderNode], found: &mut Vec<&'a Credential>) {
            for node in nodes {
                found.extend(&node.credentials);
                gather(&node.folders, found);
            }
        }
        let mut found: Vec<&Credential> = self.tree.credentials.iter().collect();
        gather(&self.tree.folders, &mut found);
        found
    }

    pub fn connection(&self, id: ConnectionId) -> Option<&Connection> {
        fn find(nodes: &[FolderNode], id: ConnectionId) -> Option<&Connection> {
            nodes.iter().find_map(|node| {
                node.connections
                    .iter()
                    .find(|connection| connection.id == id)
                    .or_else(|| find(&node.folders, id))
            })
        }
        self.tree
            .connections
            .iter()
            .find(|connection| connection.id == id)
            .or_else(|| find(&self.tree.folders, id))
    }

    pub fn folder(&self, id: FolderId) -> Option<&FolderNode> {
        fn find(nodes: &[FolderNode], id: FolderId) -> Option<&FolderNode> {
            nodes.iter().find_map(|node| {
                (node.folder.id == id)
                    .then_some(node)
                    .or_else(|| find(&node.folders, id))
            })
        }
        find(&self.tree.folders, id)
    }

    /// The name of `item`; empty for one that is gone.
    pub fn name_of(&self, item: TreeItem) -> String {
        match item {
            TreeItem::Folder(id) => self.folder(id).map(|node| node.folder.name.clone()),
            TreeItem::Connection(id) => self.connection(id).map(|c| c.name.clone()),
            TreeItem::Credential(id) => self.credential(id).map(|c| c.name.clone()),
        }
        .unwrap_or_default()
    }

    /// The row after `item` once it is gone, else the one before: where the cursor goes after
    /// a delete.
    pub fn neighbour(&self, item: TreeItem) -> Option<TreeItem> {
        let rows = self.rows();
        let at = rows.iter().position(|row| row.item == item)?;
        let depth = rows[at].depth;
        // Rows inside a folder go with it.
        rows[at + 1..]
            .iter()
            .find(|row| row.depth <= depth)
            .or_else(|| at.checked_sub(1).map(|before| &rows[before]))
            .map(|row| row.item)
    }
}

/// What deleting a folder takes with it: its folders (itself included) and connections.
pub fn contents(node: &FolderNode) -> (u64, u64) {
    node.folders.iter().fold(
        (1, node.connections.len() as u64),
        |(folders, connections), inner| {
            let (more_folders, more_connections) = contents(inner);
            (folders + more_folders, connections + more_connections)
        },
    )
}

fn contains(text: &str, needle: &str) -> bool {
    needle.is_empty() || text.to_lowercase().contains(needle)
}

fn connection_matches(connection: &Connection, needle: &str) -> bool {
    contains(&connection.name, needle) || contains(&connection.host, needle)
}

fn credential_matches(credential: &Credential, needle: &str) -> bool {
    contains(&credential.name, needle)
        || credential
            .username
            .as_deref()
            .is_some_and(|user| contains(user, needle))
}

/// What one level of the tree holds.
#[derive(Clone, Copy)]
struct Level<'a> {
    folders: &'a [FolderNode],
    connections: &'a [Connection],
    credentials: &'a [Credential],
}

/// What a filter shows, worked out in one pass over the tree with each name lowercased once
/// (RC-080-53): the folders it shows - their own name matches, or something inside does - those
/// whose own name matches, and the connections and credentials that match.
#[derive(Debug, Default)]
struct Matches {
    folders: HashSet<FolderId>,
    named: HashSet<FolderId>,
    items: HashSet<TreeItem>,
}

impl Matches {
    fn of(tree: &ConnectionTree, needle: &str) -> Self {
        let mut matches = Self::default();
        matches.level(&tree.folders, &tree.connections, &tree.credentials, needle);
        matches
    }

    /// Marks what matches on one level and below; whether anything did.
    fn level(
        &mut self,
        folders: &[FolderNode],
        connections: &[Connection],
        credentials: &[Credential],
        needle: &str,
    ) -> bool {
        let mut any = false;
        for connection in connections {
            if connection_matches(connection, needle) {
                self.items.insert(TreeItem::Connection(connection.id));
                any = true;
            }
        }
        for credential in credentials {
            if credential_matches(credential, needle) {
                self.items.insert(TreeItem::Credential(credential.id));
                any = true;
            }
        }
        for node in folders {
            let id = node.folder.id;
            let named = contains(&node.folder.name, needle);
            let inside = self.level(&node.folders, &node.connections, &node.credentials, needle);
            if named {
                self.named.insert(id);
            }
            if named || inside {
                self.folders.insert(id);
                any = true;
            }
        }
        any
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
