//! Arranging the tree by button, key or menu (RC-100-63): up and down among the siblings of the
//! same kind, indent into the folder above, outdent into the folder's folder. Each is one move
//! command with the node's new position among its siblings (`rc-db` numbers them densely), or
//! none when the move is not possible - the button is greyed out then. Shared credentials keep
//! their order by name: up and down are off for them, indent and outdent move them.

use rc_engine::{Command, FolderId, MoveConnectionRequest, MoveFolderRequest, TreeItem};

use super::tree::TreeState;

/// One way to arrange the node at the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrange {
    /// Ctrl+Up: one place up among its siblings.
    Up,
    /// Ctrl+Down: one place down.
    Down,
    /// Ctrl+Left: out of its folder, into the folder's folder.
    Outdent,
    /// Ctrl+Right: into the folder above it on its level, at the end.
    Indent,
}

/// A move of the tree: the command, and the folder that opens so the node stays in sight.
#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub command: Command,
    pub open: Option<FolderId>,
}

/// The move `how` makes of `item`; `None` when it cannot: the first or last of its kind, no
/// folder above it, already at the root, a credential moved up or down, a node that is gone.
pub fn plan(tree: &TreeState, item: TreeItem, how: Arrange) -> Option<Move> {
    let parent = tree.parent_of(item);
    let (folders, _) = tree.level(parent)?;
    let exists = match item {
        TreeItem::Credential(id) => tree.credential(id).is_some(),
        TreeItem::Folder(_) | TreeItem::Connection(_) => place(tree, item, parent).is_some(),
    };
    if !exists {
        return None;
    }
    match how {
        Arrange::Up | Arrange::Down => {
            let (at, count) = place(tree, item, parent)?;
            let to = match how {
                Arrange::Up => at.checked_sub(1)?,
                _ => (at + 1 < count).then_some(at + 1)?,
            };
            Some(Move {
                command: moved(item, parent, Some(to)),
                open: None,
            })
        }
        Arrange::Indent => {
            let into = match item {
                TreeItem::Folder(id) => {
                    let at = folders.iter().position(|node| node.folder.id == id)?;
                    folders.get(at.checked_sub(1)?)?.folder.id
                }
                // Folders come first on a level: the one above is its last folder.
                TreeItem::Connection(_) | TreeItem::Credential(_) => folders.last()?.folder.id,
            };
            Some(Move {
                command: moved(item, Some(into), None),
                open: Some(into),
            })
        }
        Arrange::Outdent => {
            let parent = parent?;
            let grand = tree.parent_of(TreeItem::Folder(parent));
            // A folder lands right behind the folder it left; the others at their level's end.
            let to = match item {
                TreeItem::Folder(_) => {
                    let (above, _) = tree.level(grand)?;
                    let at = above.iter().position(|node| node.folder.id == parent)?;
                    Some(at + 1)
                }
                TreeItem::Connection(_) | TreeItem::Credential(_) => None,
            };
            Some(Move {
                command: moved(item, grand, to),
                open: grand,
            })
        }
    }
}

/// Where `item` stands among the siblings of its kind in `parent`, and how many there are;
/// `None` for a credential, which has no place of its own.
pub fn place(tree: &TreeState, item: TreeItem, parent: Option<FolderId>) -> Option<(usize, usize)> {
    let (folders, connections) = tree.level(parent)?;
    match item {
        TreeItem::Folder(id) => folders
            .iter()
            .position(|node| node.folder.id == id)
            .map(|at| (at, folders.len())),
        TreeItem::Connection(id) => connections
            .iter()
            .position(|connection| connection.id == id)
            .map(|at| (at, connections.len())),
        TreeItem::Credential(_) => None,
    }
}

/// The command that puts `item` into `parent` at the position `to` among its siblings (`None`:
/// at the end); a credential has no position.
pub fn moved(item: TreeItem, parent: Option<FolderId>, to: Option<usize>) -> Command {
    let sort_order = to.and_then(|to| i64::try_from(to).ok());
    match item {
        TreeItem::Folder(id) => Command::MoveFolder(MoveFolderRequest {
            id,
            parent_id: parent,
            sort_order,
        }),
        TreeItem::Connection(id) => Command::MoveConnection(MoveConnectionRequest {
            id,
            folder_id: parent,
            sort_order,
        }),
        TreeItem::Credential(id) => Command::MoveCredential {
            id,
            folder_id: parent,
        },
    }
}

#[cfg(test)]
#[path = "arrange_tests.rs"]
mod tests;
