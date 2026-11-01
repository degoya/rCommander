//! The toolbar, the context menu and the keys of RC-100-63: on a tree of their own (`tree_fixture`)
//! what each action can do, and through `apply` and a real engine what the keys do to the stored
//! tree.

use rc_engine::{
    Chord, Command, ConnectionDraft, ConnectionId, ConnectionTree, CredentialId, FolderId,
    MoveFolderRequest, Outcome, TreeItem,
};

use super::{TreeAction, TreeMsg};
use crate::app::{
    Effect,
    connections::{ConnJob, ConnectionManager, Pending, jobs::Then, tree_fixture::state},
    harness::Harness,
};

const WORK: TreeItem = TreeItem::Folder(FolderId(1));
const HOME: TreeItem = TreeItem::Folder(FolderId(2));
const WEB: TreeItem = TreeItem::Connection(ConnectionId(1));
const KEY: TreeItem = TreeItem::Credential(CredentialId(7));

/// A manager on the fixture's tree with the cursor on `at`.
fn manager(at: TreeItem) -> ConnectionManager {
    let mut manager = ConnectionManager {
        tree: state(),
        ..ConnectionManager::default()
    };
    manager.tree.cursor = Some(at);
    manager
}

/// The toolbar's buttons that can run, in its order.
fn enabled(manager: &ConnectionManager) -> Vec<TreeAction> {
    TreeAction::TOOLBAR
        .into_iter()
        .filter(|action| manager.can(*action))
        .collect()
}

/// The one command `effects` run for the manager.
fn command(effects: &[Effect]) -> (Then, Command) {
    match effects {
        [Effect::Connections(ConnJob::Run(then, command))] => (*then, (**command).clone()),
        other => panic!("one command, got {} effects", other.len()),
    }
}

/// The toolbar greys out what cannot run on the node at the cursor.
#[test]
fn the_toolbar_offers_what_the_node_at_the_cursor_allows() {
    use TreeAction::{Delete, Down, Indent, Outdent, Rename, Star, Up};
    assert_eq!(
        enabled(&manager(WORK)),
        [Rename, Down, Star, Delete],
        "the first top-level folder"
    );
    assert_eq!(enabled(&manager(HOME)), [Rename, Up, Indent, Star, Delete]);
    assert_eq!(
        enabled(&manager(WEB)),
        [Rename, Down, Outdent, Indent, Star, Delete]
    );
    // A credential keeps its order by name and has no star.
    assert_eq!(enabled(&manager(KEY)), [Rename, Indent, Delete]);
    assert!(manager(WEB).can(TreeAction::Duplicate));
    assert!(!manager(WORK).can(TreeAction::Duplicate));
    assert!(manager(KEY).can(TreeAction::NewFolder));
}

#[test]
fn a_button_runs_the_move_and_the_cursor_stays_on_the_node() {
    let mut manager = manager(HOME);
    let effects = manager.on_tree(TreeMsg::Action(TreeAction::Up));
    assert_eq!(
        command(&effects),
        (
            Then::Select,
            Command::MoveFolder(MoveFolderRequest {
                id: FolderId(2),
                parent_id: None,
                sort_order: Some(0),
            })
        )
    );
    assert_eq!(manager.tree.cursor, Some(HOME));
    // Greyed out: nothing runs.
    let mut first = self::manager(WORK);
    assert!(first.on_tree(TreeMsg::Action(TreeAction::Up)).is_empty());
}

/// Space or the star on a folder switches its button in the drive bar; it shows at once.
#[test]
fn the_star_of_a_folder_switches_its_drive_bar_button() {
    let mut manager = manager(HOME);
    let effects = manager.act(TreeAction::Star);
    assert_eq!(
        command(&effects),
        (
            Then::Drives,
            Command::SetFolderFavourite {
                id: FolderId(2),
                favourite: false,
            }
        )
    );
    assert!(!manager.tree.on_bar(FolderId(2)));
    let effects = manager.act(TreeAction::Star);
    assert!(matches!(
        command(&effects).1,
        Command::SetFolderFavourite {
            favourite: true,
            ..
        }
    ));
    assert!(manager.tree.on_bar(FolderId(2)));
}

fn key(manager: &mut ConnectionManager, keys: &str) -> Vec<Effect> {
    let chord: Chord = keys.parse().expect("a chord");
    manager.key(&chord, false)
}

/// The menu opens on its first entry that can run; the arrows pass the greyed-out ones, Enter
/// runs the entry, Escape closes it, and another key closes it and works the tree.
#[test]
fn the_context_menu_walks_its_entries_that_can_run() {
    let mut manager = manager(WORK);
    key(&mut manager, "Shift+F10");
    let cursor = |manager: &ConnectionManager| manager.edit.menu.as_ref().map(|menu| menu.cursor);
    assert_eq!(cursor(&manager), Some(0), "Rename");
    key(&mut manager, "ArrowDown");
    // Up cannot run on the first folder: Down is next.
    assert_eq!(
        TreeAction::MENU[cursor(&manager).expect("open")],
        TreeAction::Down
    );
    key(&mut manager, "End");
    assert_eq!(
        TreeAction::MENU[cursor(&manager).expect("open")],
        TreeAction::NewFolder,
        "Duplicate cannot run on a folder"
    );
    key(&mut manager, "Escape");
    assert_eq!(manager.edit.menu, None);
    assert!(!manager.close, "Escape closed the menu, not the manager");

    key(&mut manager, "ContextMenu");
    key(&mut manager, "Enter");
    assert_eq!(manager.edit.menu, None);
    assert!(
        matches!(manager.pending, Some(Pending::Name { .. })),
        "Rename asks for the name"
    );

    let mut other = self::manager(HOME);
    other.on_tree(TreeMsg::Context(HOME));
    assert!(other.edit.menu.is_some(), "a right click opens it");
    other.on_tree(TreeMsg::MenuHover(1));
    other.on_tree(TreeMsg::Placed(Some(iced::Point::new(10.0, 20.0))));
    let menu = other.edit.menu.clone().expect("open");
    assert_eq!(
        (menu.cursor, menu.at),
        (1, Some(iced::Point::new(10.0, 20.0)))
    );
    let effects = key(&mut other, "F2");
    assert_eq!(other.edit.menu, None, "another key closes it");
    assert_eq!(effects.len(), 2, "F2 renames: focus and select the name");
}

/// The keys of the toolbar's actions in the tree.
#[test]
fn ctrl_and_the_arrows_arrange_the_tree() {
    let mut manager = manager(WEB);
    let (_, down) = command(&key(&mut manager, "Ctrl+ArrowDown"));
    assert!(matches!(down, Command::MoveConnection(ref r) if r.sort_order == Some(1)));
    let (_, out) = command(&key(&mut manager, "Ctrl+ArrowLeft"));
    assert!(matches!(out, Command::MoveConnection(ref r) if r.folder_id.is_none()));
    let (_, into) = command(&key(&mut manager, "Ctrl+ArrowRight"));
    assert!(matches!(into, Command::MoveConnection(ref r) if r.folder_id == Some(FolderId(4))));
    assert!(
        key(&mut manager, "Ctrl+ArrowUp").is_empty(),
        "the first connection"
    );
}

/// A drag: pressed on a row, moved over another, released there - the zone it last stood in
/// decides; leaving the list ends it without a drop.
#[test]
fn a_drag_drops_where_the_pointer_last_stood() {
    let nas = TreeItem::Connection(ConnectionId(3));
    let jump = TreeItem::Connection(ConnectionId(5));
    let mut manager = manager(WORK);
    manager.press(nas);
    // Moves while no drag runs are ignored; this one runs: the lower third of `jump`.
    manager.on_tree(TreeMsg::Hover(jump, 20.0));
    assert_eq!(
        manager.edit.hover,
        Some(super::DropAt::Row(jump, super::super::drop::Zone::After))
    );
    let (then, moved) = command(&manager.release(jump));
    assert_eq!(then, Then::Select);
    assert!(matches!(
        moved,
        Command::MoveConnection(ref request)
            if request.id == ConnectionId(3) && request.sort_order == Some(1)
    ));
    assert_eq!((manager.drag, manager.edit.hover), (None, None));

    // Dropped on a folder without a move seen: into it.
    manager.press(nas);
    let (_, into) = command(&manager.release(HOME));
    assert!(matches!(
        into,
        Command::MoveConnection(ref request) if request.folder_id == Some(FolderId(2))
    ));

    manager.press(nas);
    manager.on_tree(TreeMsg::HoverEnd);
    manager.on_tree(TreeMsg::Leave);
    assert_eq!((manager.drag, manager.edit.hover), (None, None));
    assert!(manager.on_tree(TreeMsg::DropEnd).is_empty(), "no drag left");
}

async fn tree(harness: &Harness) -> ConnectionTree {
    match harness
        .app
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("the tree")
    {
        Outcome::ConnectionTree { tree } => tree,
        other => panic!("the tree, got {other:?}"),
    }
}

fn names(tree: &ConnectionTree) -> Vec<&str> {
    tree.folders
        .iter()
        .map(|node| node.folder.name.as_str())
        .chain(tree.connections.iter().map(|c| c.name.as_str()))
        .collect()
}

/// The acceptance path by the keyboard alone against a real engine: Ctrl+Down, Ctrl+Right and
/// Ctrl+Left store the new order and nesting, Space takes a folder off the drive bar, the menu
/// opens and closes.
#[tokio::test]
async fn the_keyboard_alone_arranges_the_stored_tree() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    for name in ["a", "b"] {
        let command = Command::CreateFolder {
            parent_id: None,
            name: name.into(),
        };
        harness
            .app
            .engine()
            .dispatch(command)
            .await
            .expect("folder");
    }
    let command = Command::CreateConnection {
        folder_id: None,
        connection: ConnectionDraft::new("x", "example.org"),
    };
    harness
        .app
        .engine()
        .dispatch(command)
        .await
        .expect("connection");
    harness.press("Ctrl+F").await;
    harness.settle().await;
    harness.press("Home").await;

    harness.press("Ctrl+ArrowDown").await;
    harness.settle().await;
    assert_eq!(names(&tree(&harness).await), ["b", "a", "x"]);

    harness.press("Ctrl+ArrowRight").await;
    harness.settle().await;
    let stored = tree(&harness).await;
    assert_eq!(names(&stored), ["b", "x"]);
    assert_eq!(stored.folders[0].folders[0].folder.name, "a");

    harness.press("Ctrl+ArrowLeft").await;
    harness.settle().await;
    let stored = tree(&harness).await;
    assert_eq!(
        names(&stored),
        ["b", "a", "x"],
        "right behind the folder it left"
    );
    let a = stored.folders[1].folder.id;

    harness.press("Space").await;
    harness.settle().await;
    assert_eq!(
        tree(&harness).await.bar_folders,
        [stored.folders[0].folder.id]
    );
    assert!(
        !harness
            .app
            .connections
            .as_ref()
            .expect("open")
            .tree
            .on_bar(a)
    );

    harness.press("Shift+F10").await;
    let open = |harness: &Harness| {
        harness
            .app
            .connections
            .as_ref()
            .is_some_and(|manager| manager.edit.menu.is_some())
    };
    assert!(open(&harness));
    harness.press("Escape").await;
    assert!(!open(&harness));
    assert!(harness.app.connections.is_some(), "the manager stays");
}
