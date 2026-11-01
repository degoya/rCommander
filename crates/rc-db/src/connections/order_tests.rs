use super::slot;
use crate::{
    Db,
    connections::{ConnectionDraft, ConnectionTree, FolderId},
};

fn draft(name: &str) -> ConnectionDraft {
    ConnectionDraft::new(name, "example.org")
}

/// The names of the root's connections, in the tree's order.
fn root_connections(tree: &ConnectionTree) -> Vec<&str> {
    tree.connections.iter().map(|c| c.name.as_str()).collect()
}

/// The names of the folders under `parent` (the root for `None`), in the tree's order.
fn folder_names(tree: &ConnectionTree, parent: Option<FolderId>) -> Vec<String> {
    let level = match parent {
        None => &tree.folders,
        Some(id) => {
            &tree
                .folders
                .iter()
                .find(|node| node.folder.id == id)
                .expect("a top-level folder")
                .folders
        }
    };
    level.iter().map(|node| node.folder.name.clone()).collect()
}

#[test]
fn a_position_is_clamped_to_the_level() {
    assert_eq!(slot(None, 3), 3, "None is the end");
    assert_eq!(slot(Some(0), 3), 0);
    assert_eq!(slot(Some(2), 3), 2);
    assert_eq!(slot(Some(3), 3), 3);
    assert_eq!(slot(Some(99), 3), 3, "past the end is the end");
    assert_eq!(slot(Some(-1), 3), 0, "before the first is the first");
    assert_eq!(slot(Some(i64::MIN), 0), 0);
    assert_eq!(slot(Some(i64::MAX), 2), 2);
}

#[tokio::test]
async fn a_connection_moves_to_a_position_and_its_level_is_numbered_densely() {
    let db = Db::open_in_memory().await.expect("open");
    let mut ids = Vec::new();
    for name in ["a", "b", "c", "d"] {
        ids.push(
            db.create_connection(None, &draft(name))
                .await
                .expect(name)
                .id,
        );
    }
    // Up: "d" to the second place.
    let moved = db.move_connection(ids[3], None, Some(1)).await.expect("up");
    assert_eq!(moved.sort_order, 1);
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(root_connections(&tree), ["a", "d", "b", "c"]);
    let orders: Vec<i64> = tree.connections.iter().map(|c| c.sort_order).collect();
    assert_eq!(orders, [0, 1, 2, 3], "dense, no gap, no twin");

    // Down: "a" behind "b" - its position counted without itself.
    db.move_connection(ids[0], None, Some(2))
        .await
        .expect("down");
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(root_connections(&tree), ["d", "b", "a", "c"]);

    // First and past the end.
    db.move_connection(ids[2], None, Some(0))
        .await
        .expect("first");
    db.move_connection(ids[3], None, Some(40))
        .await
        .expect("last");
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(root_connections(&tree), ["c", "b", "a", "d"]);
    assert_eq!(
        db.connection_position(ids[0]).await.expect("position"),
        2,
        "where the tree lists it"
    );
}

#[tokio::test]
async fn a_move_into_another_folder_numbers_both_levels() {
    let db = Db::open_in_memory().await.expect("open");
    let prod = db.create_folder(None, "prod").await.expect("prod");
    let mut root = Vec::new();
    for name in ["a", "b", "c"] {
        root.push(
            db.create_connection(None, &draft(name))
                .await
                .expect(name)
                .id,
        );
    }
    for name in ["x", "y"] {
        db.create_connection(Some(prod.id), &draft(name))
            .await
            .expect(name);
    }
    let moved = db
        .move_connection(root[1], Some(prod.id), Some(1))
        .await
        .expect("into prod, between x and y");
    assert_eq!((moved.folder_id, moved.sort_order), (Some(prod.id), 1));
    let tree = db.connection_tree().await.expect("tree");
    let inside: Vec<&str> = tree.folders[0]
        .connections
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(inside, ["x", "b", "y"]);
    let left: Vec<i64> = tree.connections.iter().map(|c| c.sort_order).collect();
    assert_eq!(left, [0, 1], "the level it left closes its gap");
    assert_eq!(root_connections(&tree), ["a", "c"]);
}

#[tokio::test]
async fn folders_move_among_folders_and_connections_stay_where_they_are() {
    let db = Db::open_in_memory().await.expect("open");
    let mut folders = Vec::new();
    for name in ["one", "two", "three"] {
        folders.push(db.create_folder(None, name).await.expect(name).id);
    }
    let jump = db
        .create_connection(None, &draft("jump"))
        .await
        .expect("jump");
    db.move_folder(folders[2], None, Some(0))
        .await
        .expect("three first");
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(folder_names(&tree, None), ["three", "one", "two"]);
    let orders: Vec<i64> = tree.folders.iter().map(|n| n.folder.sort_order).collect();
    assert_eq!(orders, [0, 1, 2]);
    // The other kind is not touched: the connection keeps its number.
    assert_eq!(tree.connections[0].id, jump.id);
    assert_eq!(tree.connections[0].sort_order, 0);

    // Indent: "two" into "one", at the end; outdent: back behind "one".
    db.move_folder(folders[1], Some(folders[0]), None)
        .await
        .expect("indent");
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(folder_names(&tree, None), ["three", "one"]);
    assert_eq!(folder_names(&tree, Some(folders[0])), ["two"]);
    db.move_folder(folders[1], None, Some(2))
        .await
        .expect("outdent");
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(folder_names(&tree, None), ["three", "one", "two"]);
}

#[tokio::test]
async fn a_level_with_equal_numbers_is_ordered_as_it_was_shown() {
    let db = Db::open_in_memory().await.expect("open");
    let mut ids = Vec::new();
    for name in ["b", "a", "c"] {
        ids.push(
            db.create_connection(None, &draft(name))
                .await
                .expect(name)
                .id,
        );
    }
    // An older import may leave two at one number: the tree breaks the tie by name.
    let mut writer = db.writer().await;
    sqlx::query("UPDATE connections SET sort_order = 5")
        .execute(&mut *writer)
        .await
        .expect("equal numbers");
    drop(writer);
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(root_connections(&tree), ["a", "b", "c"]);
    db.move_connection(ids[2], None, Some(1))
        .await
        .expect("c second");
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(root_connections(&tree), ["a", "c", "b"]);
    let orders: Vec<i64> = tree.connections.iter().map(|c| c.sort_order).collect();
    assert_eq!(orders, [0, 1, 2]);
}
