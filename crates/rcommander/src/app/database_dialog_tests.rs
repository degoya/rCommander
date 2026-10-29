//! The database dialog through `apply`, against an engine over a database file: the keys belong
//! to the dialog, a backup lands where the path says, a restore is staged.

use std::path::Path;

use rc_db::{DB_FILE, Db};
use rc_engine::PanelId;
use tempfile::TempDir;

use super::{DatabaseMsg, DialogMode, Phase};
use crate::app::{Message, harness::Harness};

async fn harness(data: &Path) -> Harness {
    let db = Db::open(data.join(DB_FILE)).await.expect("db");
    Harness::on_db(data, db).await
}

async fn send(harness: &mut Harness, message: DatabaseMsg) {
    harness.send(Message::Database(message)).await;
}

fn phase(harness: &Harness) -> Phase {
    harness
        .app
        .database
        .as_ref()
        .map(|dialog| dialog.phase.clone())
        .expect("the dialog is open")
}

async fn open_with_path(harness: &mut Harness, mode: DialogMode, path: &Path) {
    send(harness, DatabaseMsg::Open).await;
    send(harness, DatabaseMsg::Mode(mode)).await;
    send(harness, DatabaseMsg::Path(path.display().to_string())).await;
}

#[tokio::test]
async fn the_dialog_backs_up_to_the_typed_path_and_reports_path_and_size() {
    let data = TempDir::new().expect("temp dir");
    let mut harness = harness(data.path()).await;
    let target = data.path().join("mine.db");
    open_with_path(&mut harness, DialogMode::Backup, &target).await;

    harness.press("Enter").await;

    let Phase::Done(result) = phase(&harness) else {
        panic!("expected a finished backup, got {:?}", phase(&harness));
    };
    assert!(result.contains("mine.db"), "{result}");
    assert!(target.is_file());
    assert_eq!(harness.app.info.as_deref(), Some(result.as_str()));

    // Enter on a finished dialog closes it.
    harness.press("Enter").await;
    assert!(harness.app.database.is_none());
}

#[tokio::test]
async fn an_existing_file_is_refused_until_overwriting_is_ticked() {
    let data = TempDir::new().expect("temp dir");
    let mut harness = harness(data.path()).await;
    let target = data.path().join("taken.db");
    std::fs::write(&target, b"keep").expect("write");
    open_with_path(&mut harness, DialogMode::Backup, &target).await;

    send(&mut harness, DatabaseMsg::Confirm).await;
    assert!(
        matches!(phase(&harness), Phase::Failed(_)),
        "{:?}",
        phase(&harness)
    );
    assert_eq!(std::fs::read(&target).expect("read"), b"keep");

    send(&mut harness, DatabaseMsg::Overwrite(true)).await;
    send(&mut harness, DatabaseMsg::Confirm).await;
    assert!(
        matches!(phase(&harness), Phase::Done(_)),
        "{:?}",
        phase(&harness)
    );
    assert_ne!(std::fs::read(&target).expect("read"), b"keep");
}

#[tokio::test]
async fn a_restore_is_checked_and_staged_for_the_next_start() {
    let data = TempDir::new().expect("temp dir");
    let mut harness = harness(data.path()).await;
    let backup = data.path().join("full.db");
    open_with_path(&mut harness, DialogMode::Backup, &backup).await;
    send(&mut harness, DatabaseMsg::Confirm).await;
    send(&mut harness, DatabaseMsg::Close).await;

    open_with_path(&mut harness, DialogMode::Restore, &backup).await;
    send(&mut harness, DatabaseMsg::Confirm).await;

    assert!(
        matches!(phase(&harness), Phase::Done(_)),
        "{:?}",
        phase(&harness)
    );
    assert!(rc_db::staged_restore_path(&data.path().join(DB_FILE)).is_file());
}

/// While the dialog is open the panels do not move under it; Escape closes it.
#[tokio::test]
async fn the_keys_belong_to_the_open_dialog() {
    let data = TempDir::new().expect("temp dir");
    std::fs::create_dir(data.path().join("sub")).expect("mkdir");
    let mut harness = harness(data.path()).await;
    send(&mut harness, DatabaseMsg::Open).await;
    let before = harness.app.pane(PanelId::Left).cursor();

    harness.press("ArrowDown").await;
    harness.press("Tab").await;
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), before);
    assert_eq!(harness.app.active, PanelId::Left);

    harness.press("Escape").await;
    assert!(harness.app.database.is_none());
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Right);
}
