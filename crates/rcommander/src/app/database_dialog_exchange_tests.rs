//! The export and import of the database dialog (RC-080-19) through `apply`, against engines over
//! database files: an encrypted export from one, read and imported into another; a wrong export
//! password refused without a preview; Enter reads the file before it imports.

use std::path::Path;

use rc_db::{DB_FILE, Db};
use rc_engine::{Command, ConnectionDraft, Outcome};
use tempfile::TempDir;

use super::{DatabaseMsg, DialogMode, ExchangeMsg, Phase};
use crate::app::{Message, harness::Harness};

async fn harness(data: &Path) -> Harness {
    let db = Db::open(data.join(DB_FILE)).await.expect("db");
    Harness::on_db(data, db).await
}

async fn send(harness: &mut Harness, message: DatabaseMsg) {
    harness.send(Message::Database(message)).await;
}

async fn exchange(harness: &mut Harness, message: ExchangeMsg) {
    send(harness, DatabaseMsg::Exchange(message)).await;
}

fn phase(harness: &Harness) -> Phase {
    harness
        .app
        .database
        .as_ref()
        .map(|dialog| dialog.phase.clone())
        .expect("the dialog is open")
}

async fn names(harness: &Harness) -> Vec<String> {
    match harness
        .app
        .engine()
        .dispatch(Command::ListConnections)
        .await
    {
        Ok(Outcome::ConnectionTree { tree }) => {
            tree.connections.into_iter().map(|c| c.name).collect()
        }
        other => panic!("the tree, got {other:?}"),
    }
}

/// An encrypted export of a database with one connection, written through the dialog.
async fn exported(source: &Path, file: &Path) {
    let mut harness = harness(source).await;
    harness
        .app
        .engine()
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("web", "web.example.org"),
        })
        .await
        .expect("connection");
    send(&mut harness, DatabaseMsg::OpenMode(DialogMode::Export)).await;
    send(&mut harness, DatabaseMsg::Path(file.display().to_string())).await;
    exchange(&mut harness, ExchangeMsg::Password("export pw".into())).await;
    exchange(&mut harness, ExchangeMsg::Repeat("export pw".into())).await;
    harness.press("Enter").await;
    let Phase::Done(result) = phase(&harness) else {
        panic!("a finished export, got {:?}", phase(&harness));
    };
    assert!(result.contains("all.rcexport"), "{result}");
    let text = std::fs::read_to_string(file).expect("written");
    assert!(!text.contains("web.example.org"), "sealed");
}

#[tokio::test]
async fn an_encrypted_export_is_read_and_imported_into_another_database() {
    let source = TempDir::new().expect("temp dir");
    let file = source.path().join("all.rcexport");
    exported(source.path(), &file).await;

    let target = TempDir::new().expect("temp dir");
    let mut harness = harness(target.path()).await;
    send(&mut harness, DatabaseMsg::OpenMode(DialogMode::Import)).await;
    send(&mut harness, DatabaseMsg::Path(file.display().to_string())).await;
    exchange(&mut harness, ExchangeMsg::Password("export pw".into())).await;
    // Enter reads first: a preview, nothing written.
    harness.press("Enter").await;
    assert_eq!(phase(&harness), Phase::Editing);
    let dialog = harness.app.database.as_ref().expect("open");
    let report = dialog.exchange.report.as_ref().expect("a preview");
    assert!(report.dry_run && report.encrypted);
    assert!(
        dialog
            .exchange
            .chosen
            .iter()
            .any(|key| key.starts_with("connection:"))
    );
    assert!(names(&harness).await.is_empty());

    harness.press("Enter").await;
    assert!(
        matches!(phase(&harness), Phase::Done(_)),
        "{:?}",
        phase(&harness)
    );
    assert_eq!(names(&harness).await, ["web"]);
}

#[tokio::test]
async fn a_wrong_export_password_shows_no_preview() {
    let source = TempDir::new().expect("temp dir");
    let file = source.path().join("all.rcexport");
    exported(source.path(), &file).await;

    let target = TempDir::new().expect("temp dir");
    let mut harness = harness(target.path()).await;
    send(&mut harness, DatabaseMsg::OpenMode(DialogMode::Import)).await;
    send(&mut harness, DatabaseMsg::Path(file.display().to_string())).await;
    exchange(&mut harness, ExchangeMsg::Password("wrong".into())).await;
    exchange(&mut harness, ExchangeMsg::Read).await;
    assert!(
        matches!(phase(&harness), Phase::Failed(_)),
        "{:?}",
        phase(&harness)
    );
    let dialog = harness.app.database.as_ref().expect("open");
    assert!(dialog.exchange.report.is_none());
    // Confirm without a preview runs nothing.
    send(&mut harness, DatabaseMsg::Confirm).await;
    assert!(names(&harness).await.is_empty());
}

#[tokio::test]
async fn an_export_without_a_part_says_why_and_runs_nothing() {
    let data = TempDir::new().expect("temp dir");
    let mut harness = harness(data.path()).await;
    let file = data.path().join("none.rcexport");
    send(&mut harness, DatabaseMsg::OpenMode(DialogMode::Export)).await;
    send(&mut harness, DatabaseMsg::Path(file.display().to_string())).await;
    for part in super::Part::ALL {
        exchange(&mut harness, ExchangeMsg::Part(part, false)).await;
    }
    send(&mut harness, DatabaseMsg::Confirm).await;
    let dialog = harness.app.database.as_ref().expect("open");
    assert!(dialog.exchange.problem.is_some());
    assert_eq!(dialog.phase, Phase::Editing);
    assert!(!file.exists());
    harness.press("Escape").await;
    assert!(harness.app.database.is_none());
}
