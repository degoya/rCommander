//! The page of the import from Remote Desktop Manager through `apply` and a real engine on
//! `rc-engine`'s fixture: the button under the tree opens it, the preview chooses what is new,
//! the import fills the folder of RDM's entries, and Escape returns to the tree.

use std::fs;

use rc_db::Db;
use rc_engine::{
    Command, Engine, ImportStatus, Outcome, VfsPath,
    import::{rdm::RdmSkip, rdm_report::DEFAULT_FOLDER},
};

use super::RdmImportMsg;
use crate::{
    app::{
        Effect, Message,
        connections::{ConnMsg, ConnectionManager, import::Phase},
        harness::Harness,
    },
    i18n::error_text,
};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-engine/tests/fixtures/rdm/sample.rdm"
);

fn manager(harness: &Harness) -> &ConnectionManager {
    harness
        .app
        .connections
        .as_ref()
        .expect("the manager is open")
}

async fn send(harness: &mut Harness, message: RdmImportMsg) {
    harness
        .send(Message::Connections(ConnMsg::RdmImport(message)))
        .await;
}

#[test]
fn every_reason_an_entry_is_refused_has_a_text() {
    for skip in RdmSkip::ALL {
        assert_ne!(
            error_text(skip.code()),
            skip.code(),
            "no text for error.{}",
            skip.code()
        );
    }
}

#[tokio::test]
async fn the_page_previews_imports_and_goes_back_to_the_tree() {
    let root = tempfile::tempdir().expect("temp dir");
    let export = root.path().join("export.rdm");
    fs::copy(FIXTURE, &export).expect("fixture");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let mut harness = Harness::new(engine);
    harness.press("Ctrl+F").await;
    send(&mut harness, RdmImportMsg::Open).await;
    let dialog = manager(&harness).rdm_import.as_ref().expect("the page");
    assert!(dialog.report.is_none());
    // Nothing to read without a file.
    send(&mut harness, RdmImportMsg::Read).await;
    assert_eq!(
        manager(&harness)
            .rdm_import
            .as_ref()
            .expect("page")
            .page
            .phase,
        Phase::Done
    );

    send(
        &mut harness,
        RdmImportMsg::Path(export.display().to_string()),
    )
    .await;
    send(&mut harness, RdmImportMsg::Read).await;
    let dialog = manager(&harness).rdm_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Ready, "{:?}", dialog.page.error);
    assert_eq!(dialog.chosen.len(), 3);
    assert!(
        dialog
            .chosen
            .contains("0f3c1a52-7d4e-4b8a-9c61-2e5b8d7a4f10")
    );
    assert!(
        !dialog.chosen.contains("entry5"),
        "a refused entry is not offered"
    );

    // One left out.
    send(
        &mut harness,
        RdmImportMsg::Choose("7b9d1f3a-5c6e-4f8b-9d0e-2f4a6b8c0d2e".to_owned(), false),
    )
    .await;
    send(&mut harness, RdmImportMsg::Run).await;
    let dialog = manager(&harness).rdm_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Done, "{:?}", dialog.page.error);
    let created = dialog
        .report
        .as_ref()
        .expect("report")
        .entries
        .iter()
        .filter(|row| matches!(row.status, ImportStatus::Created { .. }))
        .count();
    assert_eq!(created, 2);
    let Outcome::ConnectionTree { tree } = harness
        .app
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    else {
        panic!("the tree");
    };
    assert_eq!(tree.folders[0].folder.name, DEFAULT_FOLDER);

    harness.press("Escape").await;
    assert!(
        manager(&harness).rdm_import.is_none(),
        "Escape left the page"
    );
}

#[tokio::test]
async fn a_protected_export_shows_why_and_offers_nothing() {
    let root = tempfile::tempdir().expect("temp dir");
    let export = root.path().join("protected.rdm");
    fs::write(&export, "q83vEjRWeJAbCd+/AAECAwQFBgcICQoLDA0ODxA=").expect("file");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let mut harness = Harness::new(engine);
    harness.press("Ctrl+F").await;
    send(&mut harness, RdmImportMsg::Open).await;
    send(
        &mut harness,
        RdmImportMsg::Path(export.display().to_string()),
    )
    .await;
    send(&mut harness, RdmImportMsg::Read).await;
    let dialog = manager(&harness).rdm_import.as_ref().expect("the page");
    assert_eq!(
        dialog.page.error.as_deref(),
        Some(error_text("rdm_encrypted").as_str())
    );
    assert!(dialog.chosen.is_empty() && dialog.report.is_none());
}

/// The answers of `effects` held back, arriving now: a read that comes late.
async fn arrive(harness: &mut Harness, effects: Vec<Effect>) {
    for effect in effects {
        if let Effect::Connections(job) = effect {
            let answer = job.run(harness.app.engine().clone()).await;
            harness.send(Message::Connections(answer)).await;
        }
    }
}

/// A changed path drops the preview (RC-080-39): Import takes nothing from the new file, and a
/// read of the old one that answers late is left out too.
#[tokio::test]
async fn a_changed_path_drops_the_preview_and_imports_nothing() {
    let root = tempfile::tempdir().expect("temp dir");
    let (first, second) = (
        root.path().join("first.rdm"),
        root.path().join("second.rdm"),
    );
    fs::copy(FIXTURE, &first).expect("fixture");
    fs::copy(FIXTURE, &second).expect("fixture");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let mut harness = Harness::new(engine);
    harness.press("Ctrl+F").await;
    send(&mut harness, RdmImportMsg::Open).await;
    send(
        &mut harness,
        RdmImportMsg::Path(first.display().to_string()),
    )
    .await;
    send(&mut harness, RdmImportMsg::Read).await;
    let dialog = manager(&harness).rdm_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Ready, "{:?}", dialog.page.error);
    // The first file read once more, its answer held back.
    let late = harness
        .app
        .apply(Message::Connections(ConnMsg::RdmImport(RdmImportMsg::Read)));

    send(
        &mut harness,
        RdmImportMsg::Path(second.display().to_string()),
    )
    .await;
    let dialog = manager(&harness).rdm_import.as_ref().expect("the page");
    assert_eq!(dialog.page.phase, Phase::Done);
    assert!(dialog.report.is_none() && dialog.chosen.is_empty());
    send(&mut harness, RdmImportMsg::Run).await;
    arrive(&mut harness, late).await;
    let dialog = manager(&harness).rdm_import.as_ref().expect("the page");
    assert!(dialog.report.is_none(), "the late preview is left out");
    send(&mut harness, RdmImportMsg::Run).await;

    let Outcome::ConnectionTree { tree } = harness
        .app
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    else {
        panic!("the tree");
    };
    assert!(
        tree.connections.is_empty() && tree.folders.is_empty(),
        "{tree:?}"
    );
}
