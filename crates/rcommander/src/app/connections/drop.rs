//! Dropping a dragged node (RC-100-63): where the pointer stands on a row decides - its upper
//! third before the row, its lower third after it, the middle of a folder into the folder (of
//! any other row the nearer half); below the last row is the end of the root. A node dropped
//! beside one of another kind goes to its own kind's edge of that level (folders come first,
//! then connections, then shared credentials), as the tree cannot show it elsewhere. The view
//! draws the insertion line or the target folder's highlight from the same [`DropAt`].

use rc_engine::{FolderId, TreeItem};

use super::{
    arrange::{Move, moved, place},
    tree::TreeState,
};

/// Where on a row the pointer stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Before,
    Into,
    After,
}

/// Where a drag would drop now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAt {
    /// On this row, in this zone.
    Row(TreeItem, Zone),
    /// Below the last row: the end of the root.
    End,
}

/// The zone of a row `height` high the pointer stands in at `y` from its top; only a `folder`
/// has a middle.
pub fn zone(y: f32, height: f32, folder: bool) -> Zone {
    let third = height / 3.0;
    if y < third {
        Zone::Before
    } else if y >= height - third {
        Zone::After
    } else if folder {
        Zone::Into
    } else if y < height / 2.0 {
        Zone::Before
    } else {
        Zone::After
    }
}

/// The move that drops `item` at `at`; `None` when nothing would change or it cannot go there
/// (onto itself, into itself or a folder inside it).
pub fn plan(tree: &TreeState, item: TreeItem, at: DropAt) -> Option<Move> {
    let (parent, to) = match at {
        DropAt::End => (None, None),
        DropAt::Row(target, _) if target == item => return None,
        DropAt::Row(TreeItem::Folder(folder), Zone::Into) => (Some(folder), None),
        DropAt::Row(target, zone) => {
            let parent = tree.parent_of(target);
            (
                parent,
                beside(tree, item, target, parent, zone == Zone::After),
            )
        }
    };
    if let TreeItem::Folder(id) = item
        && inside(tree, parent, id)
    {
        return None;
    }
    if tree.parent_of(item) == parent {
        match place(tree, item, parent) {
            // Its own place (counted without itself), or the end where it is the last.
            Some((now, count)) if to.unwrap_or(count.saturating_sub(1)) == now => return None,
            // A credential in its own folder already: no place to change.
            None => return None,
            Some(_) => {}
        }
    }
    Some(Move {
        command: moved(item, parent, to),
        open: parent,
    })
}

/// The position for `item` next to `target` in `parent`, `after` it or before: among its own
/// kind's siblings without itself; at its kind's edge when `target` is of another kind.
fn beside(
    tree: &TreeState,
    item: TreeItem,
    target: TreeItem,
    parent: Option<FolderId>,
    after: bool,
) -> Option<usize> {
    let same = matches!(
        (item, target),
        (TreeItem::Folder(_), TreeItem::Folder(_))
            | (TreeItem::Connection(_), TreeItem::Connection(_))
    );
    if !same {
        // A connection beside a folder goes first among the connections; everything else to
        // the end of its kind.
        return match (item, target) {
            (TreeItem::Connection(_), TreeItem::Folder(_)) => Some(0),
            _ => None,
        };
    }
    let (at, _) = place(tree, target, parent)?;
    // Counted without the dragged node when it stands before the target on this level.
    let before_target = tree.parent_of(item) == parent
        && place(tree, item, parent).is_some_and(|(own, _)| own < at);
    let at = if before_target { at - 1 } else { at };
    Some(if after { at + 1 } else { at })
}

/// Whether the folder `parent` is the folder `id` or inside it.
fn inside(tree: &TreeState, parent: Option<FolderId>, id: FolderId) -> bool {
    let mut current = parent;
    // A tree is never deeper than this; the bound only stops a damaged one from looping.
    for _ in 0..=rc_engine::MAX_DEPTH {
        match current {
            Some(folder) if folder == id => return true,
            Some(folder) => current = tree.parent_of(TreeItem::Folder(folder)),
            None => return false,
        }
    }
    false
}

#[cfg(test)]
#[path = "drop_tests.rs"]
mod tests;
