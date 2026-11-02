//! `plugins.list` with one plugin (RC-100-75): the summary of every retained call, and at most
//! `plugins.recent_calls_shown` of the latest calls - 0 none.

use rc_db::{Db, NewPluginExecution};
use rc_plugin_host::{
    key_fingerprint, sha256_hex,
    test_support::{signing_key, viewer_package},
};
use serde_json::json;

use crate::{
    Command, Engine, Outcome, PluginsOutcome,
    settings::{DEFAULT_RECENT_CALLS_SHOWN, RECENT_CALLS_SHOWN_KEY},
    test_support,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";

/// An engine with a database, a plugin folder and the plugin `ID` installed; the temporary
/// folder lives as long as the engine is used.
async fn installed() -> (tempfile::TempDir, Engine, Db) {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    assert!(engine.attach_plugins(root.path().join("data"), None));
    let path = root.path().join("csv-1.0.0.rcplug");
    let bytes = viewer_package(ID, "1.0.0", "author-v1", &signing_key(7), true);
    std::fs::write(&path, &bytes).expect("write");
    engine
        .dispatch(Command::InstallPlugin {
            path: path.display().to_string(),
            fingerprint: Some(key_fingerprint(&signing_key(7).verifying_key())),
            digest: sha256_hex(&bytes),
        })
        .await
        .expect("installed");
    (root, engine, db)
}

async fn record(db: &Db, count: usize, outcome: &str, code: Option<&str>) {
    for _ in 0..count {
        db.record_plugin_execution(&NewPluginExecution {
            plugin_id: ID.to_owned(),
            plugin_version: "1.0.0".to_owned(),
            plugin_type: "viewer".to_owned(),
            operation: "open".to_owned(),
            outcome: outcome.to_owned(),
            code: code.map(str::to_owned),
            duration_ms: 6,
        })
        .await
        .expect("record");
    }
}

async fn list(engine: &Engine, id: Option<&str>) -> PluginsOutcome {
    match engine
        .dispatch(Command::ListPlugins {
            id: id.map(str::to_owned),
        })
        .await
        .expect("listed")
    {
        Outcome::Plugins(outcome) => *outcome,
        other => panic!("a plugins outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn the_engine_answers_at_most_the_calls_the_setting_allows_and_the_summary_of_all() {
    let (_root, engine, db) = installed().await;
    record(&db, 20, "ok", None).await;
    record(&db, 2, "crash", Some("plugin_crashed")).await;

    let PluginsOutcome::List {
        executions,
        summary,
        calls_shown,
        ..
    } = list(&engine, Some(ID)).await
    else {
        panic!("a list");
    };
    assert_eq!(calls_shown, DEFAULT_RECENT_CALLS_SHOWN);
    assert_eq!(executions.len(), 10);
    assert_eq!(executions[0].outcome, "crash", "newest first");
    let summary = summary.expect("a summary with an id");
    assert_eq!((summary.calls, summary.failures), (22, 2));
    assert_eq!(summary.last_failure.as_deref(), Some("plugin_crashed"));
    assert_eq!(summary.mean_duration_ms, 6);

    engine
        .set_protected_setting(RECENT_CALLS_SHOWN_KEY, &json!(3))
        .await
        .expect("the window may");
    let PluginsOutcome::List { executions, .. } = list(&engine, Some(ID)).await else {
        panic!("a list");
    };
    assert_eq!(executions.len(), 3);

    // Without an id: no calls, no summary.
    let PluginsOutcome::List {
        executions,
        summary,
        ..
    } = list(&engine, None).await
    else {
        panic!("a list");
    };
    assert!(executions.is_empty() && summary.is_none());
}

#[tokio::test]
async fn zero_hides_the_calls_but_keeps_the_summary() {
    let (_root, engine, db) = installed().await;
    record(&db, 4, "ok", None).await;
    engine
        .set_protected_setting(RECENT_CALLS_SHOWN_KEY, &json!(0))
        .await
        .expect("the window may");
    let PluginsOutcome::List {
        executions,
        summary,
        calls_shown,
        ..
    } = list(&engine, Some(ID)).await
    else {
        panic!("a list");
    };
    assert_eq!(calls_shown, 0);
    assert!(executions.is_empty());
    assert_eq!(summary.map(|summary| summary.calls), Some(4));
}

#[tokio::test]
async fn the_setting_refuses_more_than_its_maximum() {
    let (_root, engine, _db) = installed().await;
    let refused = engine
        .set_protected_setting(RECENT_CALLS_SHOWN_KEY, &json!(101))
        .await
        .expect_err("above the maximum");
    assert_eq!(refused.code(), "setting_invalid");
}
