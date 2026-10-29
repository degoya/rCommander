use super::build_tree;
use crate::{
    Db,
    connections::{ConnectionDraft, ConnectionError, ConnectionFolder, FolderId, MAX_DEPTH},
};

fn draft(name: &str) -> ConnectionDraft {
    ConnectionDraft::new(name, "example.org")
}

#[tokio::test]
async fn folders_nest_and_the_tree_keeps_display_order() {
    let db = Db::open_in_memory().await.expect("open");
    let prod = db.create_folder(None, "prod").await.expect("prod");
    let dev = db.create_folder(None, "dev").await.expect("dev");
    let web = db.create_folder(Some(prod.id), "web").await.expect("web");
    db.create_connection(Some(web.id), &draft("web-1"))
        .await
        .expect("web-1");
    db.create_connection(None, &draft("jump"))
        .await
        .expect("jump");

    let tree = db.connection_tree().await.expect("tree");
    let names: Vec<&str> = tree
        .folders
        .iter()
        .map(|node| node.folder.name.as_str())
        .collect();
    assert_eq!(names, ["prod", "dev"], "creation order, not the alphabet");
    assert_eq!(tree.folders[0].folders[0].folder.id, web.id);
    assert_eq!(tree.folders[0].folders[0].connections[0].name, "web-1");
    assert!(tree.folders[1].folders.is_empty());
    assert_eq!(tree.folders[1].folder.id, dev.id);
    assert_eq!(tree.connections[0].name, "jump");
}

#[tokio::test]
async fn a_folder_name_is_taken_once_per_parent() {
    let db = Db::open_in_memory().await.expect("open");
    let prod = db.create_folder(None, "prod").await.expect("prod");
    let error = db.create_folder(None, "PROD").await.expect_err("taken");
    assert_eq!(error.code(), "name_taken");
    db.create_folder(Some(prod.id), "prod")
        .await
        .expect("the same name one level down");
    let error = db.create_folder(None, " ").await.expect_err("blank");
    assert_eq!(error.code(), "name_empty");
}

#[tokio::test]
async fn a_rename_checks_the_siblings() {
    let db = Db::open_in_memory().await.expect("open");
    db.create_folder(None, "prod").await.expect("prod");
    let dev = db.create_folder(None, "dev").await.expect("dev");
    let error = db.rename_folder(dev.id, "Prod").await.expect_err("taken");
    assert_eq!(error.code(), "name_taken");
    let renamed = db.rename_folder(dev.id, "staging").await.expect("rename");
    assert_eq!(renamed.name, "staging");
    let error = db
        .rename_folder(FolderId(99), "x")
        .await
        .expect_err("missing");
    assert_eq!(error.code(), "folder_not_found");
}

#[tokio::test]
async fn a_folder_never_moves_into_itself_or_its_subfolders() {
    let db = Db::open_in_memory().await.expect("open");
    let top = db.create_folder(None, "top").await.expect("top");
    let middle = db.create_folder(Some(top.id), "middle").await.expect("mid");
    let bottom = db
        .create_folder(Some(middle.id), "bottom")
        .await
        .expect("bottom");
    for target in [top.id, middle.id, bottom.id] {
        let error = db
            .move_folder(top.id, Some(target), None)
            .await
            .expect_err("cycle");
        assert!(matches!(error, ConnectionError::FolderCycle), "{error}");
        assert_eq!(error.code(), "folder_cycle");
    }
    let unchanged = db.connection_folder(top.id).await.expect("top");
    assert_eq!(unchanged.parent_id, None);

    let moved = db
        .move_folder(bottom.id, None, None)
        .await
        .expect("up to the root");
    assert_eq!(moved.parent_id, None);
    let moved = db
        .move_folder(top.id, Some(bottom.id), None)
        .await
        .expect("into a former subfolder that is no longer inside");
    assert_eq!(moved.parent_id, Some(bottom.id));
}

#[tokio::test]
async fn folders_nest_at_most_max_depth() {
    let db = Db::open_in_memory().await.expect("open");
    let mut parent = None;
    let mut chain = Vec::new();
    for level in 1..=MAX_DEPTH {
        let folder = db
            .create_folder(parent, &format!("level {level}"))
            .await
            .expect("within the limit");
        parent = Some(folder.id);
        chain.push(folder.id);
    }
    let error = db
        .create_folder(parent, "one too deep")
        .await
        .expect_err("too deep");
    assert_eq!(error.code(), "folder_too_deep");

    let side = db.create_folder(None, "side").await.expect("side");
    db.create_folder(Some(side.id), "child")
        .await
        .expect("child");
    let error = db
        .move_folder(side.id, Some(chain[MAX_DEPTH - 2]), None)
        .await
        .expect_err("its child would be too deep");
    assert_eq!(error.code(), "folder_too_deep");
}

#[tokio::test]
async fn deleting_a_folder_with_content_wants_recursive() {
    let db = Db::open_in_memory().await.expect("open");
    let prod = db.create_folder(None, "prod").await.expect("prod");
    let web = db.create_folder(Some(prod.id), "web").await.expect("web");
    db.create_connection(Some(web.id), &draft("web-1"))
        .await
        .expect("web-1");
    db.create_connection(Some(prod.id), &draft("db-1"))
        .await
        .expect("db-1");
    db.create_connection(None, &draft("jump"))
        .await
        .expect("jump");

    let error = db
        .delete_folder(prod.id, false)
        .await
        .expect_err("not empty");
    assert_eq!(error.code(), "folder_not_empty");
    assert_eq!(db.connection_tree().await.expect("tree").folders.len(), 1);

    let removed = db.delete_folder(prod.id, true).await.expect("recursive");
    assert_eq!((removed.folders, removed.connections), (2, 2));
    let tree = db.connection_tree().await.expect("tree");
    assert!(tree.folders.is_empty());
    assert_eq!(tree.connections.len(), 1, "the root's connection stays");

    let empty = db.create_folder(None, "empty").await.expect("empty");
    let removed = db.delete_folder(empty.id, false).await.expect("empty");
    assert_eq!((removed.folders, removed.connections), (1, 0));
    let error = db.delete_folder(empty.id, false).await.expect_err("gone");
    assert_eq!(error.code(), "folder_not_found");
}

fn folder(id: i64, parent: Option<i64>) -> ConnectionFolder {
    ConnectionFolder {
        id: FolderId(id),
        parent_id: parent.map(FolderId),
        name: format!("f{id}"),
        sort_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[test]
fn a_damaged_table_with_a_cycle_or_an_orphan_still_builds() {
    let tree = build_tree(
        vec![
            folder(1, None),
            folder(2, Some(3)),
            folder(3, Some(2)),
            folder(4, Some(77)),
        ],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(tree.folders.len(), 1);
    assert_eq!(tree.folders[0].folder.id, FolderId(1));
    assert!(tree.folders[0].folders.is_empty());
}
