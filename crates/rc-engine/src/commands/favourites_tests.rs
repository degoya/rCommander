use super::Favourites;
use crate::test_support::engine_with_db;
use crate::{
    Command, Connection, ConnectionDraft, ConnectionId, DriveGroup, Engine, EngineEvent, FolderId,
    Outcome,
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

/// The labels of the drive bar's servers, in order.
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
    assert_eq!(
        all_servers(&engine).await,
        [
            ("top".to_owned(), Vec::new(), true),
            ("inner".to_owned(), vec!["work".to_owned()], false),
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
    assert_eq!(servers(&engine).await, ["top", "inner"]);

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
    assert_eq!(servers(&engine).await, ["later", "inner"]);
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
