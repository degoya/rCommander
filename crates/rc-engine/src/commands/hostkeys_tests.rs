//! The trust store in the settings: list, search, remove, import.

use rc_db::KnownHostSource;
use tempfile::TempDir;

use crate::test_support::engine_with_db;
use crate::{Command, Engine, EngineEvent, Outcome};

const PLAIN: &str = include_str!("../../../rc-remote/tests/fixtures/known_hosts/known_hosts");
const HASHED: &str =
    include_str!("../../../rc-remote/tests/fixtures/known_hosts/hashed_known_hosts");

async fn import(engine: &Engine, root: &TempDir, name: &str, text: &str) -> super::ImportReport {
    let path = root.path().join(name);
    std::fs::write(&path, text).expect("write");
    match engine
        .dispatch(Command::ImportKnownHosts {
            path: Some(path.display().to_string()),
        })
        .await
        .expect("import")
    {
        Outcome::KnownHostsImported(report) => report,
        other => panic!("an import report, got {other:?}"),
    }
}

async fn list(engine: &Engine, filter: Option<&str>) -> Vec<rc_db::KnownHost> {
    match engine
        .dispatch(Command::ListHostKeys {
            filter: filter.map(str::to_owned),
        })
        .await
        .expect("list")
    {
        Outcome::KnownHosts { keys } => keys,
        other => panic!("keys, got {other:?}"),
    }
}

#[tokio::test]
async fn an_import_stores_reports_and_announces() {
    let (root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let report = import(&engine, &root, "known_hosts", PLAIN).await;
    // `neg.example`'s key is the one the file revokes (RC-080-12).
    assert_eq!(report.added, 4);
    assert_eq!(report.unchanged, 0);
    assert!(report.conflicts.is_empty());
    assert_eq!(report.skipped.len(), 7);
    assert!(report.path.ends_with("known_hosts"));
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::KnownHostsChanged
    );
    let keys = list(&engine, None).await;
    assert_eq!(keys.len(), 4);
    assert!(keys.iter().all(|key| key.source == KnownHostSource::Import));

    let again = import(&engine, &root, "known_hosts", PLAIN).await;
    assert_eq!((again.added, again.unchanged), (0, 4));
}

#[tokio::test]
async fn a_conflicting_import_keeps_the_stored_key() {
    let (root, engine) = engine_with_db(&[]).await;
    import(&engine, &root, "known_hosts", PLAIN).await;
    let other = include_str!("../../../rc-remote/tests/fixtures/known_hosts/other.pub");
    let mut fields = other.split_whitespace();
    let line = format!(
        "box.example {} {}\n",
        fields.next().expect("type"),
        fields.next().expect("key")
    );
    let report = import(&engine, &root, "changed", &line).await;
    assert_eq!(report.added, 0);
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].host, "box.example");
}

#[tokio::test]
async fn a_missing_file_is_a_coded_error() {
    let (root, engine) = engine_with_db(&[]).await;
    let error = engine
        .dispatch(Command::ImportKnownHosts {
            path: Some(root.path().join("nope").display().to_string()),
        })
        .await
        .expect_err("missing");
    assert_eq!(error.code(), "known_hosts_unreadable");
}

#[tokio::test]
async fn the_list_searches_names_types_fingerprints_and_hashed_hosts() {
    let (root, engine) = engine_with_db(&[]).await;
    import(&engine, &root, "known_hosts", PLAIN).await;
    import(&engine, &root, "hashed", HASHED).await;
    assert_eq!(list(&engine, Some("MIXED")).await.len(), 1);
    assert_eq!(list(&engine, Some("ecdsa")).await.len(), 2);
    let by_fingerprint = list(&engine, Some("5RiEvVR9s2eAmq1p")).await;
    assert_eq!(
        by_fingerprint.len(),
        3,
        "the ed25519 key: two plain names and one hashed"
    );
    // `box.example` names two plain rows and the hashed row of port 22.
    let box_rows = list(&engine, Some("box.example")).await;
    assert_eq!(box_rows.len(), 3);
    assert_eq!(box_rows.iter().filter(|key| key.is_hashed()).count(), 1);
    assert_eq!(list(&engine, Some("  ")).await.len(), 6);
}

#[tokio::test]
async fn remove_forgets_a_key_and_names_a_missing_one() {
    let (root, engine) = engine_with_db(&[]).await;
    import(&engine, &root, "known_hosts", PLAIN).await;
    let keys = list(&engine, None).await;
    let Outcome::KnownHostRemoved { key } = engine
        .dispatch(Command::RemoveHostKey { id: keys[0].id })
        .await
        .expect("removed")
    else {
        panic!("the removed key");
    };
    assert_eq!(key, keys[0]);
    assert_eq!(list(&engine, None).await.len(), 3);
    let error = engine
        .dispatch(Command::RemoveHostKey { id: keys[0].id })
        .await
        .expect_err("gone");
    assert_eq!(error.code(), "known_host_not_found");
}
