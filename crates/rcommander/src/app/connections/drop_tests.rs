use rc_engine::{
    Command, ConnectionId, CredentialId, FolderId, MoveConnectionRequest, MoveFolderRequest,
    TreeItem,
};

use super::{DropAt, Zone, plan, zone};
use crate::app::connections::{arrange::Move, tree_fixture::state};

const WORK: TreeItem = TreeItem::Folder(FolderId(1));
const HOME: TreeItem = TreeItem::Folder(FolderId(2));
const DB: TreeItem = TreeItem::Folder(FolderId(3));
const WEB: TreeItem = TreeItem::Connection(ConnectionId(1));
const PG: TreeItem = TreeItem::Connection(ConnectionId(2));
const NAS: TreeItem = TreeItem::Connection(ConnectionId(3));
const JUMP: TreeItem = TreeItem::Connection(ConnectionId(5));
const KEY: TreeItem = TreeItem::Credential(CredentialId(7));

fn folder(id: i64, parent: Option<i64>, to: Option<i64>) -> Option<Move> {
    Some(Move {
        command: Command::MoveFolder(MoveFolderRequest {
            id: FolderId(id),
            parent_id: parent.map(FolderId),
            sort_order: to,
        }),
        open: parent.map(FolderId),
    })
}

fn connection(id: i64, parent: Option<i64>, to: Option<i64>) -> Option<Move> {
    Some(Move {
        command: Command::MoveConnection(MoveConnectionRequest {
            id: ConnectionId(id),
            folder_id: parent.map(FolderId),
            sort_order: to,
        }),
        open: parent.map(FolderId),
    })
}

/// The upper third before, the lower third after, the middle of a folder into it; another row
/// has no middle and splits in halves.
#[test]
fn the_thirds_of_a_row_decide() {
    let height = 24.0;
    assert_eq!(zone(0.0, height, true), Zone::Before);
    assert_eq!(zone(7.9, height, true), Zone::Before);
    assert_eq!(zone(8.0, height, true), Zone::Into);
    assert_eq!(zone(15.9, height, true), Zone::Into);
    assert_eq!(zone(16.0, height, true), Zone::After);
    assert_eq!(zone(23.9, height, true), Zone::After);
    assert_eq!(zone(10.0, height, false), Zone::Before);
    assert_eq!(zone(12.0, height, false), Zone::After);
}

#[test]
fn before_and_after_a_row_of_the_same_kind() {
    let tree = state();
    assert_eq!(
        plan(&tree, JUMP, DropAt::Row(NAS, Zone::Before)),
        connection(5, None, Some(0))
    );
    // Counted without itself: `nas` after `jump` is the second place.
    assert_eq!(
        plan(&tree, NAS, DropAt::Row(JUMP, Zone::After)),
        connection(3, None, Some(1))
    );
    // Into another level, between its rows.
    assert_eq!(
        plan(&tree, PG, DropAt::Row(WEB, Zone::After)),
        connection(2, Some(1), Some(1))
    );
    assert_eq!(
        plan(&tree, HOME, DropAt::Row(WORK, Zone::Before)),
        folder(2, None, Some(0))
    );
}

#[test]
fn the_middle_of_a_folder_drops_into_it_at_the_end() {
    let tree = state();
    assert_eq!(
        plan(&tree, WEB, DropAt::Row(HOME, Zone::Into)),
        connection(1, Some(2), None)
    );
    assert_eq!(
        plan(&tree, HOME, DropAt::Row(DB, Zone::Into)),
        folder(2, Some(3), None)
    );
}

/// Beside a row of another kind a node goes to its own kind's edge of that level.
#[test]
fn a_wrong_kind_goes_to_its_group_s_edge() {
    let tree = state();
    // A connection before or after a folder: the first of the connections.
    assert_eq!(
        plan(&tree, JUMP, DropAt::Row(WORK, Zone::Before)),
        connection(5, None, Some(0))
    );
    assert_eq!(
        plan(&tree, PG, DropAt::Row(HOME, Zone::After)),
        connection(2, None, Some(0))
    );
    // A folder beside a connection: the end of the folders.
    assert_eq!(
        plan(&tree, DB, DropAt::Row(NAS, Zone::After)),
        folder(3, None, None)
    );
    // A connection beside a credential: the end of the connections.
    assert_eq!(
        plan(&tree, WEB, DropAt::Row(KEY, Zone::Before)),
        connection(1, None, None)
    );
}

#[test]
fn below_the_last_row_is_the_end_of_the_root() {
    let tree = state();
    assert_eq!(plan(&tree, PG, DropAt::End), connection(2, None, None));
    assert_eq!(plan(&tree, DB, DropAt::End), folder(3, None, None));
    assert_eq!(
        plan(&tree, KEY, DropAt::Row(HOME, Zone::Into)),
        Some(Move {
            command: Command::MoveCredential {
                id: CredentialId(7),
                folder_id: Some(FolderId(2)),
            },
            open: Some(FolderId(2)),
        })
    );
}

/// A drop that changes nothing, or would put a folder into itself, is no move.
#[test]
fn no_move_onto_itself_into_itself_or_to_where_it_is() {
    let tree = state();
    assert_eq!(plan(&tree, WORK, DropAt::Row(WORK, Zone::Into)), None);
    assert_eq!(plan(&tree, WORK, DropAt::Row(DB, Zone::Into)), None);
    assert_eq!(plan(&tree, WORK, DropAt::Row(DB, Zone::Before)), None);
    assert_eq!(plan(&tree, NAS, DropAt::Row(JUMP, Zone::Before)), None);
    assert_eq!(
        plan(&tree, JUMP, DropAt::End),
        None,
        "the root's last already"
    );
    assert_eq!(plan(&tree, HOME, DropAt::Row(NAS, Zone::After)), None);
    assert_eq!(
        plan(&tree, KEY, DropAt::End),
        None,
        "a credential at the root"
    );
}
