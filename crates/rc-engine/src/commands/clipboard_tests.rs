use std::{fs, path::Path};

use rc_vfs::BackendId;

use crate::test_support;
use crate::{
    Command, ConnectionDraft, CursorTarget, Engine, NameFormat, Outcome, VfsPath,
    archives::tests::engine_beside,
};

async fn text(engine: &Engine, format: NameFormat) -> String {
    match engine
        .dispatch(Command::CopyNames {
            panel: None,
            format,
        })
        .await
        .expect("names")
    {
        Outcome::ClipboardText { text } => text,
        other => panic!("text expected, got {other:?}"),
    }
}

#[tokio::test]
async fn names_paths_and_details_one_line_each() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::write(root.path().join("a.txt"), b"abc").expect("a");
    fs::write(root.path().join("b.txt"), b"abcd").expect("b");
    let engine = test_support::headless(root.path()).await;
    assert_eq!(text(&engine, NameFormat::Names).await, "a.txt");
    engine
        .dispatch(Command::SelectAll { panel: None })
        .await
        .expect("select");
    let lines: Vec<String> = text(&engine, NameFormat::FullPaths)
        .await
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        lines,
        vec![
            root.path().join("a.txt").display().to_string(),
            root.path().join("b.txt").display().to_string(),
        ]
    );
    let details = text(&engine, NameFormat::Details).await;
    let first = details.lines().next().expect("a line");
    let columns: Vec<&str> = first.split('\t').collect();
    assert_eq!(&columns[..2], ["a.txt", "3"]);
    assert_eq!(columns[2].len(), "2026-10-17 12:00:00".len());
}

#[tokio::test]
async fn an_empty_folder_has_nothing_to_copy() {
    let (_root, engine) = test_support::engine(&[]).await;
    let error = engine
        .dispatch(Command::CopyNames {
            panel: None,
            format: NameFormat::Names,
        })
        .await
        .expect_err("nothing");
    assert_eq!(error.code(), "entry_not_found");
}

#[tokio::test]
async fn a_full_path_inside_an_archive_is_the_one_the_panel_shows() {
    let (root, engine) = engine_beside("tree.zip").await;
    for command in [
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("tree.zip".into()),
        },
        Command::OpenFile {
            panel: None,
            path: None,
        },
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("hello.txt".into()),
        },
    ] {
        engine.dispatch(command).await.expect("into the archive");
    }
    assert_eq!(
        text(&engine, NameFormat::FullPaths).await,
        root.path()
            .join("tree.zip")
            .join("hello.txt")
            .display()
            .to_string()
    );
    assert_eq!(text(&engine, NameFormat::Names).await, "hello.txt");
}

#[tokio::test]
async fn a_full_path_on_a_connection_is_written_with_its_name() {
    let (root, engine) = test_support::engine_with_db(&[]).await;
    let id = match engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("nas", "nas.example.org"),
        })
        .await
        .expect("created")
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    };
    let folder = VfsPath {
        backend: BackendId::Ftp {
            connection_id: id.0,
        },
        path: "/pub".into(),
    };
    let (name, written) = engine.connection_writer(&folder).await.expect("a writer");
    assert_eq!(
        written(&name, Path::new("/pub/a.txt")),
        "ftp://nas/pub/a.txt"
    );
    let on_sftp = VfsPath {
        backend: BackendId::Sftp {
            connection_id: id.0,
        },
        path: "/srv".into(),
    };
    let (name, written) = engine.connection_writer(&on_sftp).await.expect("a writer");
    assert_eq!(
        written(&name, Path::new("/srv/my site/a.txt")),
        "sftp://nas/srv/my site/a.txt"
    );
    assert!(
        engine
            .connection_writer(&VfsPath::local(root.path()))
            .await
            .is_none()
    );
}
