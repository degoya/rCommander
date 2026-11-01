use super::Favourites;
use crate::test_support::engine_with_db;
use crate::{
    Command, Connection, ConnectionDraft, ConnectionFolder, ConnectionId, DriveGroup, Engine,
    EngineEvent, FolderId, Outcome,
};

async fn create(engine: &Engine, folder_id: Option<FolderId>, name: &str) -> Connection {
    let command = Command::CreateConnection {
        folder_id,
        connection: ConnectionDraft::new(name, "example.org"),
    };
    match engine.dispatch(command).await.expect("created") {
        Outcome::Connection { connection } => connection,
        other => panic!("a connection, got {other:?}"),
    }
}

/// The servers of `drives.list`, each with its folder and whether it is a favourite.
async fn all_servers(engine: &Engine) -> Vec<(String, Vec<String>, bool)> {
    let listed = engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    match listed {
        Outcome::Drives { drives } => drives
            .into_iter()
            .filter(|drive| drive.group == DriveGroup::Servers)
            .map(|drive| (drive.label, drive.folder, drive.favourite))
            .collect(),
        other => panic!("drives, got {other:?}"),
    }
}

/// The servers the drive bar shows as buttons of their own: the favourites.
async fn servers(engine: &Engine) -> Vec<String> {
    all_servers(engine)
        .await
        .into_iter()
        .filter(|(_, _, favourite)| *favourite)
        .map(|(label, _, _)| label)
        .collect()
}

#[tokio::test]
async fn the_root_level_is_listed_by_default_and_a_mark_changes_only_its_connection() {
    let (_root, engine) = engine_with_db(&[]).await;
    let folder = match engine
        .dispatch(Command::CreateFolder {
            parent_id: None,
            name: "f".into(),
        })
        .await
        .expect("folder")
    {
        Outcome::Folder { folder } => folder.id,
        other => panic!("a folder, got {other:?}"),
    };
    let root = create(&engine, None, "root").await;
    let nested = create(&engine, Some(folder), "nested").await;
    let mut marks = Favourites::default();
    assert!(marks.contains(&root));
    assert!(!marks.contains(&nested));

    marks.set(&nested, true);
    marks.set(&root, false);
    assert!(!marks.contains(&root));
    assert!(marks.contains(&nested));
    assert_eq!(marks.shown, [nested.id]);
    assert_eq!(marks.hidden, [root.id]);

    // Back to the default: no mark is kept.
    marks.set(&nested, false);
    marks.set(&root, true);
    assert_eq!(marks, Favourites::default());
}

/// The acceptance check "the group Servers shows the marked connections": a connection in a
/// folder becomes a button once marked, one of the root level stops being one once unmarked, and
/// the drive bar is told. Every server stays listed with its folder, for the folder's menu.
#[tokio::test]
async fn the_drive_bar_lists_the_marked_connections() {
    let (_root, engine) = engine_with_db(&[]).await;
    let folder = match engine
        .dispatch(Command::CreateFolder {
            parent_id: None,
            name: "work".into(),
        })
        .await
        .expect("folder")
    {
        Outcome::Folder { folder } => folder.id,
        other => panic!("a folder, got {other:?}"),
    };
    let top = create(&engine, None, "top").await;
    let inner = create(&engine, Some(folder), "inner").await;
    assert_eq!(servers(&engine).await, ["top"]);
    // The tree's order: the folder's servers before the root's own (RC-100-63).
    assert_eq!(
        all_servers(&engine).await,
        [
            ("inner".to_owned(), vec!["work".to_owned()], false),
            ("top".to_owned(), Vec::new(), true),
        ]
    );

    let mut events = engine.subscribe();
    let answer = engine
        .dispatch(Command::SetFavourite {
            id: inner.id,
            favourite: true,
        })
        .await
        .expect("marked");
    assert!(matches!(answer, Outcome::Drives { .. }), "{answer:?}");
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::Drives));
    assert_eq!(servers(&engine).await, ["inner", "top"]);

    engine
        .dispatch(Command::SetFavourite {
            id: top.id,
            favourite: false,
        })
        .await
        .expect("unmarked");
    assert_eq!(servers(&engine).await, ["inner"]);

    // A root connection made later appears without a mark.
    create(&engine, None, "later").await;
    assert_eq!(servers(&engine).await, ["inner", "later"]);
}

#[tokio::test]
async fn a_mark_of_an_unknown_connection_is_refused_and_a_deleted_one_is_dropped() {
    let (_root, engine) = engine_with_db(&[]).await;
    let refused = engine
        .dispatch(Command::SetFavourite {
            id: ConnectionId(99),
            favourite: true,
        })
        .await;
    assert_eq!(refused.expect_err("unknown").code(), "connection_not_found");

    let gone = create(&engine, None, "gone").await;
    let kept = create(&engine, None, "kept").await;
    for id in [gone.id, kept.id] {
        engine
            .dispatch(Command::SetFavourite {
                id,
                favourite: false,
            })
            .await
            .expect("unmarked");
    }
    engine
        .dispatch(Command::DeleteConnection { id: gone.id })
        .await
        .expect("deleted");
    engine
        .dispatch(Command::SetFavourite {
            id: kept.id,
            favourite: true,
        })
        .await
        .expect("marked");
    assert_eq!(engine.favourites().await, Favourites::default());
}

#[tokio::test]
async fn ctrl_f_is_announced_for_the_window() {
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ConnectionManager {})
        .await
        .expect("announced");
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::ConnectionManager));
}

async fn folder(engine: &Engine, parent_id: Option<FolderId>, name: &str) -> ConnectionFolder {
    let command = Command::CreateFolder {
        parent_id,
        name: name.to_owned(),
    };
    match engine.dispatch(command).await.expect("folder") {
        Outcome::Folder { folder } => folder,
        other => panic!("a folder, got {other:?}"),
    }
}

/// Each server of `drives.list` with the folder paths the drive bar shows as buttons above it.
async fn buttons(engine: &Engine) -> Vec<(String, Vec<Vec<String>>)> {
    let listed = engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("lists");
    let Outcome::Drives { drives } = listed else {
        panic!("drives");
    };
    drives
        .into_iter()
        .filter(|drive| drive.group == DriveGroup::Servers)
        .map(|drive| {
            let paths = drive
                .folder_buttons
                .iter()
                .map(|&length| drive.folder[..length].to_vec())
                .collect();
            (drive.label, paths)
        })
        .collect()
}

async fn bar_folders(engine: &Engine) -> Vec<FolderId> {
    match engine
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    {
        Outcome::ConnectionTree { tree } => tree.bar_folders,
        other => panic!("the tree, got {other:?}"),
    }
}

#[tokio::test]
async fn a_folder_of_the_root_level_is_a_button_by_default_and_a_subfolder_is_not() {
    let (_root, engine) = engine_with_db(&[]).await;
    let work = folder(&engine, None, "work").await;
    let staging = folder(&engine, Some(work.id), "staging").await;
    let mut marks = Favourites::default();
    assert!(marks.contains_folder(&work));
    assert!(!marks.contains_folder(&staging));

    marks.set_folder(&work, false);
    marks.set_folder(&staging, true);
    assert!(!marks.contains_folder(&work));
    assert!(marks.contains_folder(&staging));
    assert_eq!(marks.folders_hidden, [work.id]);
    assert_eq!(marks.folders_shown, [staging.id]);

    marks.set_folder(&work, true);
    marks.set_folder(&staging, false);
    assert_eq!(marks, Favourites::default(), "the default is no mark");
}

#[test]
fn marks_stored_before_the_folder_marks_still_read() {
    let stored = r#"{"shown":[3],"hidden":[1]}"#;
    let marks: Favourites = serde_json::from_str(stored).expect("old JSON reads");
    assert_eq!(marks.shown, [ConnectionId(3)]);
    assert_eq!(marks.hidden, [ConnectionId(1)]);
    assert!(marks.folders_shown.is_empty());
    assert!(marks.folders_hidden.is_empty());
}

/// The acceptance check "folder star": a top-level folder taken out of the bar, a subfolder put
/// in; `drives.list` names the buttons above each server, `connections.list` the marked folders.
#[tokio::test]
async fn the_drive_bar_shows_the_marked_folders() {
    let (_root, engine) = engine_with_db(&[]).await;
    let work = folder(&engine, None, "work").await;
    let staging = folder(&engine, Some(work.id), "staging").await;
    let empty = folder(&engine, None, "empty").await;
    create(&engine, Some(staging.id), "deep").await;
    create(&engine, Some(work.id), "web").await;
    let work_path = vec!["work".to_owned()];
    let staging_path = vec!["work".to_owned(), "staging".to_owned()];
    assert_eq!(
        buttons(&engine).await,
        [
            ("deep".to_owned(), vec![work_path.clone()]),
            ("web".to_owned(), vec![work_path.clone()]),
        ]
    );
    assert_eq!(bar_folders(&engine).await, [work.id, empty.id]);

    let mut events = engine.subscribe();
    let answer = engine
        .dispatch(Command::SetFolderFavourite {
            id: staging.id,
            favourite: true,
        })
        .await
        .expect("marked");
    assert!(matches!(answer, Outcome::Drives { .. }), "{answer:?}");
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::Drives));
    assert_eq!(
        buttons(&engine).await[0],
        ("deep".to_owned(), vec![work_path, staging_path.clone()])
    );

    engine
        .dispatch(Command::SetFolderFavourite {
            id: work.id,
            favourite: false,
        })
        .await
        .expect("unmarked");
    assert_eq!(
        buttons(&engine).await,
        [
            ("deep".to_owned(), vec![staging_path]),
            ("web".to_owned(), Vec::new()),
        ]
    );
    assert_eq!(bar_folders(&engine).await, [staging.id, empty.id]);
}

#[tokio::test]
async fn a_mark_of_an_unknown_folder_is_refused_and_a_deleted_one_is_dropped() {
    let (_root, engine) = engine_with_db(&[]).await;
    let refused = engine
        .dispatch(Command::SetFolderFavourite {
            id: FolderId(99),
            favourite: true,
        })
        .await;
    assert_eq!(refused.expect_err("unknown").code(), "folder_not_found");

    let gone = folder(&engine, None, "gone").await;
    let kept = folder(&engine, None, "kept").await;
    for id in [gone.id, kept.id] {
        engine
            .dispatch(Command::SetFolderFavourite {
                id,
                favourite: false,
            })
            .await
            .expect("unmarked");
    }
    engine
        .dispatch(Command::DeleteFolder {
            id: gone.id,
            recursive: false,
        })
        .await
        .expect("deleted");
    engine
        .dispatch(Command::SetFolderFavourite {
            id: kept.id,
            favourite: true,
        })
        .await
        .expect("marked");
    assert_eq!(engine.favourites().await, Favourites::default());
}
