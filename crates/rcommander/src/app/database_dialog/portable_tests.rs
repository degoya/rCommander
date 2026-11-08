//! The portable step of the database dialog (RC-100-95), short of the copy itself: the engine's
//! `app.make_portable` would write beside the test binary (`rc-engine`'s tests run it on a
//! folder of their own).

use rc_db::{BackupReport, DB_FILE, Db};

use super::{PortableMsg, PortableStep, finished};
use crate::app::{Message, database_dialog::DatabaseMsg, harness::Harness};

async fn harness() -> (tempfile::TempDir, Harness) {
    let data = tempfile::tempdir().expect("temp dir");
    let db = Db::open(data.path().join(DB_FILE)).await.expect("db");
    let harness = Harness::on_db(data.path(), db).await;
    (data, harness)
}

fn step(harness: &Harness) -> PortableStep {
    harness
        .app
        .database
        .as_ref()
        .map(|dialog| dialog.portable)
        .expect("the dialog is open")
}

#[tokio::test]
async fn the_question_comes_before_anything_is_copied() {
    let (_data, mut harness) = harness().await;
    harness.send(Message::Database(DatabaseMsg::Open)).await;
    harness
        .send(Message::Database(DatabaseMsg::Portable(PortableMsg::Ask)))
        .await;
    if !crate::portable::offered() {
        // macOS: no question, no copy.
        assert_eq!(step(&harness), PortableStep::Idle);
        return;
    }
    assert_eq!(step(&harness), PortableStep::Asking);
    // Escape takes the question back and leaves the dialog open.
    harness.press("Escape").await;
    assert_eq!(step(&harness), PortableStep::Idle);
    assert!(harness.app.database.is_some());
    harness
        .send(Message::Database(DatabaseMsg::Portable(PortableMsg::Ask)))
        .await;
    harness
        .send(Message::Database(DatabaseMsg::Portable(
            PortableMsg::Cancel,
        )))
        .await;
    assert_eq!(step(&harness), PortableStep::Idle);
    // A restart is offered only after a copy.
    harness
        .send(Message::Database(DatabaseMsg::Portable(
            PortableMsg::Restart,
        )))
        .await;
    assert!(!harness.app.quit.leaving);
}

#[tokio::test]
async fn a_finished_copy_offers_the_restart() {
    let (data, mut harness) = harness().await;
    harness.send(Message::Database(DatabaseMsg::Open)).await;
    let dialog = harness.app.database.as_mut().expect("the dialog");
    let report = BackupReport {
        path: data.path().join("stick").join("data").join(DB_FILE),
        size: 4096,
    };
    assert_eq!(
        finished(dialog, Some(&report)),
        None,
        "not the copy's answer"
    );
    dialog.portable = PortableStep::Copying;
    let text = finished(dialog, Some(&report)).expect("the copy's text");
    assert!(text.contains("data"), "{text}");
    assert_eq!(dialog.portable, PortableStep::Made);
    // A failed copy goes back to the start.
    dialog.portable = PortableStep::Copying;
    assert_eq!(finished(dialog, None), None);
    assert_eq!(dialog.portable, PortableStep::Idle);
}
