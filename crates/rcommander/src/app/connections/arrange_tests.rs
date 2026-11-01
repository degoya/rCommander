use rc_engine::{
    Command, ConnectionId, CredentialId, FolderId, MoveConnectionRequest, MoveFolderRequest,
    TreeItem,
};

use super::{Arrange, Move, plan};
use crate::app::connections::tree_fixture::state;

const WORK: TreeItem = TreeItem::Folder(FolderId(1));
const HOME: TreeItem = TreeItem::Folder(FolderId(2));
const DB: TreeItem = TreeItem::Folder(FolderId(3));
const OPS: TreeItem = TreeItem::Folder(FolderId(4));
const WEB: TreeItem = TreeItem::Connection(ConnectionId(1));
const PG: TreeItem = TreeItem::Connection(ConnectionId(2));
const NAS: TreeItem = TreeItem::Connection(ConnectionId(3));
const API: TreeItem = TreeItem::Connection(ConnectionId(4));
const JUMP: TreeItem = TreeItem::Connection(ConnectionId(5));
const KEY: TreeItem = TreeItem::Credential(CredentialId(7));

fn folder(id: i64, parent: Option<i64>, to: Option<i64>, open: Option<i64>) -> Option<Move> {
    Some(Move {
        command: Command::MoveFolder(MoveFolderRequest {
            id: FolderId(id),
            parent_id: parent.map(FolderId),
            sort_order: to,
        }),
        open: open.map(FolderId),
    })
}

fn connection(id: i64, parent: Option<i64>, to: Option<i64>, open: Option<i64>) -> Option<Move> {
    Some(Move {
        command: Command::MoveConnection(MoveConnectionRequest {
            id: ConnectionId(id),
            folder_id: parent.map(FolderId),
            sort_order: to,
        }),
        open: open.map(FolderId),
    })
}

#[test]
fn up_and_down_move_among_the_siblings_of_the_same_kind() {
    let tree = state();
    assert_eq!(
        plan(&tree, HOME, Arrange::Up),
        folder(2, None, Some(0), None)
    );
    assert_eq!(
        plan(&tree, WORK, Arrange::Down),
        folder(1, None, Some(1), None)
    );
    assert_eq!(
        plan(&tree, API, Arrange::Up),
        connection(4, Some(1), Some(0), None)
    );
    assert_eq!(
        plan(&tree, NAS, Arrange::Down),
        connection(3, None, Some(1), None)
    );
}

/// The first of its kind cannot go up, the last not down - a connection never passes the
/// folders above it; greyed out on the toolbar.
#[test]
fn the_ends_of_a_kind_are_greyed_out() {
    let tree = state();
    for (item, how) in [
        (WORK, Arrange::Up),
        (HOME, Arrange::Down),
        (NAS, Arrange::Up),
        (JUMP, Arrange::Down),
        (PG, Arrange::Up),
        (PG, Arrange::Down),
        (DB, Arrange::Up),
        (OPS, Arrange::Down),
    ] {
        assert_eq!(plan(&tree, item, how), None, "{item:?} {how:?}");
    }
}

#[test]
fn indent_goes_into_the_folder_above_at_its_end() {
    let tree = state();
    assert_eq!(
        plan(&tree, HOME, Arrange::Indent),
        folder(2, Some(1), None, Some(1))
    );
    assert_eq!(
        plan(&tree, OPS, Arrange::Indent),
        folder(4, Some(3), None, Some(3))
    );
    // A connection: into the last folder of its level, the one right above the connections.
    assert_eq!(
        plan(&tree, NAS, Arrange::Indent),
        connection(3, Some(2), None, Some(2))
    );
    assert_eq!(
        plan(&tree, WEB, Arrange::Indent),
        connection(1, Some(4), None, Some(4))
    );
    // No folder above: the first folder, a connection on a level without folders.
    assert_eq!(plan(&tree, WORK, Arrange::Indent), None);
    assert_eq!(plan(&tree, DB, Arrange::Indent), None);
    assert_eq!(plan(&tree, PG, Arrange::Indent), None);
}

#[test]
fn outdent_goes_into_the_folder_s_folder() {
    let tree = state();
    // A folder lands right behind the folder it left.
    assert_eq!(
        plan(&tree, DB, Arrange::Outdent),
        folder(3, None, Some(1), None)
    );
    // A connection at the end of its new level.
    assert_eq!(
        plan(&tree, PG, Arrange::Outdent),
        connection(2, Some(1), None, Some(1))
    );
    assert_eq!(
        plan(&tree, WEB, Arrange::Outdent),
        connection(1, None, None, None)
    );
    // At the root already.
    assert_eq!(plan(&tree, WORK, Arrange::Outdent), None);
    assert_eq!(plan(&tree, NAS, Arrange::Outdent), None);
}

/// Shared credentials keep their order by name: no up and down, but they move between folders.
#[test]
fn a_credential_moves_between_folders_only() {
    let tree = state();
    assert_eq!(plan(&tree, KEY, Arrange::Up), None);
    assert_eq!(plan(&tree, KEY, Arrange::Down), None);
    assert_eq!(plan(&tree, KEY, Arrange::Outdent), None);
    assert_eq!(
        plan(&tree, KEY, Arrange::Indent),
        Some(Move {
            command: Command::MoveCredential {
                id: CredentialId(7),
                folder_id: Some(FolderId(2)),
            },
            open: Some(FolderId(2)),
        })
    );
}

#[test]
fn a_node_that_is_gone_has_no_move() {
    let tree = state();
    let gone = TreeItem::Connection(ConnectionId(99));
    for how in [
        Arrange::Up,
        Arrange::Down,
        Arrange::Indent,
        Arrange::Outdent,
    ] {
        assert_eq!(plan(&tree, gone, how), None, "{how:?}");
    }
}
