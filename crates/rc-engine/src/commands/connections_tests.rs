use rc_db::Db;
use rc_vfs::VfsPath;
use tempfile::TempDir;
use tokio::sync::broadcast::error::TryRecvError;

use crate::{
    Command, ConnectionDraft, ConnectionPatch, Engine, EngineEvent, FolderId, Outcome, TreeChange,
    TreeItem,
};

async fn engine() -> (TempDir, Engine) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(db.clone()));
    assert!(!engine.attach_db(db), "the first database stays");
    (root, engine)
}

async fn folder(engine: &Engine, parent: Option<FolderId>, name: &str) -> FolderId {
    match engine
        .dispatch(Command::CreateFolder {
            parent_id: parent,
            name: name.into(),
        })
        .await
        .expect("folder")
    {
        Outcome::Folder { folder } => folder.id,
        other => panic!("a folder, got {other:?}"),
    }
}

fn code(result: Result<Outcome, crate::EngineError>) -> &'static str {
    result.expect_err("refused").code()
}

#[tokio::test]
async fn the_commands_build_change_and_empty_the_tree() {
    let (_root, engine) = engine().await;
    let prod = folder(&engine, None, "prod").await;
    let created = engine
        .dispatch(Command::CreateConnection {
            folder_id: Some(prod),
            connection: ConnectionDraft::new("web", "example.org"),
        })
        .await
        .expect("create");
    let Outcome::Connection { connection } = created else {
        panic!("a connection, got {created:?}");
    };
    let id = connection.id;

    let updated = engine
        .dispatch(Command::UpdateConnection {
            id,
            patch: ConnectionPatch {
                host: Some("example.net".into()),
                ..ConnectionPatch::default()
            },
        })
        .await
        .expect("update");
    assert!(
        matches!(updated, Outcome::Connection { connection } if connection.host == "example.net")
    );

    engine
        .dispatch(Command::MoveConnection(crate::MoveConnectionRequest {
            id,
            folder_id: None,
            sort_order: None,
        }))
        .await
        .expect("move");
    engine
        .dispatch(Command::RenameFolder {
            id: prod,
            name: "production".into(),
        })
        .await
        .expect("rename");

    let listed = engine
        .dispatch(Command::ListConnections)
        .await
        .expect("list");
    let Outcome::ConnectionTree { tree } = listed else {
        panic!("the tree, got {listed:?}");
    };
    assert_eq!(tree.connections[0].id, id);
    assert_eq!(tree.folders[0].folder.name, "production");

    let deleted = engine
        .dispatch(Command::DeleteConnection { id })
        .await
        .expect("delete");
    assert_eq!(
        deleted,
        Outcome::Deleted {
            item: TreeItem::Connection(id),
            folders: 0,
            connections: 1
        }
    );
    assert_eq!(
        code(engine.dispatch(Command::GetConnection { id }).await),
        "connection_not_found"
    );
}

#[tokio::test]
async fn every_change_is_announced_and_a_read_is_not() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    let prod = folder(&engine, None, "prod").await;
    let staging = folder(&engine, None, "staging").await;
    engine
        .dispatch(Command::MoveFolder(crate::MoveFolderRequest {
            id: staging,
            parent_id: Some(prod),
            sort_order: None,
        }))
        .await
        .expect("move");
    engine
        .dispatch(Command::ListConnections)
        .await
        .expect("list");
    engine
        .dispatch(Command::DeleteFolder {
            id: prod,
            recursive: true,
        })
        .await
        .expect("delete");

    let expected = [
        (TreeItem::Folder(prod), TreeChange::Created),
        (TreeItem::Folder(staging), TreeChange::Created),
        (TreeItem::Folder(staging), TreeChange::Moved),
        (TreeItem::Folder(prod), TreeChange::Deleted),
    ];
    for (item, change) in expected {
        assert_eq!(
            events.try_recv().expect("an event"),
            EngineEvent::ConnectionTree { item, change }
        );
    }
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn invalid_input_answers_the_repository_code() {
    let (_root, engine) = engine().await;
    let create = |name: &str, host: &str, port: Option<i64>| Command::CreateConnection {
        folder_id: None,
        connection: ConnectionDraft {
            port,
            ..ConnectionDraft::new(name, host)
        },
    };
    assert_eq!(
        code(engine.dispatch(create("web", "", None)).await),
        "host_empty"
    );
    assert_eq!(
        code(engine.dispatch(create("web", "example.org", Some(0))).await),
        "port_invalid"
    );
    engine
        .dispatch(create("web", "example.org", None))
        .await
        .expect("first");
    assert_eq!(
        code(engine.dispatch(create("WEB", "example.org", None)).await),
        "name_taken"
    );
}

#[tokio::test]
async fn a_folder_cannot_move_into_its_own_subfolder() {
    let (_root, engine) = engine().await;
    let top = folder(&engine, None, "top").await;
    let inner = folder(&engine, Some(top), "inner").await;
    assert_eq!(
        code(
            engine
                .dispatch(Command::MoveFolder(crate::MoveFolderRequest {
                    id: top,
                    parent_id: Some(inner),
                    sort_order: None,
                }))
                .await
        ),
        "folder_cycle"
    );
}

#[tokio::test]
async fn without_a_database_the_tree_is_unavailable() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert_eq!(
        code(engine.dispatch(Command::ListConnections).await),
        "database_unavailable"
    );
}
