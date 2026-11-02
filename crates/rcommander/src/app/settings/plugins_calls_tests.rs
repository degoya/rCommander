//! RC-100-75: the summary of a plugin's calls (number, failures with the newest code, mean
//! duration), the latest calls closed by default and opened by their toggle, and
//! `plugins.recent_calls_shown` at 0 leaving the list out - through `apply` against an engine
//! over a database and a plugin folder.

use rc_db::{Db, NewPluginExecution, PluginExecutionSummary};
use rc_engine::settings::RECENT_CALLS_SHOWN_KEY;
use rc_plugin_host::test_support::{signing_key, viewer_package};
use serde_json::json;

use super::{Shown, shown, summary_line};
use crate::{
    app::{
        Message,
        harness::{Harness, headless_engine},
        settings::{
            self, Page, SettingsMessage, Tab,
            plugins::{PluginsMsg, PluginsPage, page_message},
        },
    },
    i18n::t,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";

#[test]
fn the_summary_names_the_calls_the_failures_with_the_last_code_and_the_mean() {
    let failed = PluginExecutionSummary {
        calls: 22,
        failures: 2,
        last_failure: Some("plugin_crashed".to_owned()),
        mean_duration_ms: 14,
    };
    assert_eq!(
        summary_line(&failed),
        Some(t!(
            "settings.plugins.calls_summary_failed",
            calls = 22,
            failures = 2,
            code = "plugin_crashed",
            mean = 14
        ))
    );
    let clean = PluginExecutionSummary {
        calls: 5,
        failures: 0,
        last_failure: None,
        mean_duration_ms: 3,
    };
    assert_eq!(
        summary_line(&clean),
        Some(t!(
            "settings.plugins.calls_summary",
            calls = 5,
            failures = 0,
            mean = 3
        ))
    );
    assert_eq!(summary_line(&PluginExecutionSummary::default()), None);
}

/// The settings open on the Plugins page with the plugin `ID` installed and selected, over a
/// database and a plugin folder in `root`.
async fn installed(root: &tempfile::TempDir) -> (Harness, Db) {
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    assert!(engine.attach_plugins(root.path().join("data"), None));
    let mut harness = Harness::new(engine);
    harness.app.db = Some(db.clone());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let loaded = settings::load(harness.app.engine().clone(), db.clone()).await;
    harness
        .send(Message::Settings(SettingsMessage::Loaded(
            loaded.map(Box::new),
        )))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Plugins)))
        .await;
    let path = root.path().join("csv.rcplug");
    let bytes = viewer_package(ID, "1.0.0", "author-v1", &signing_key(7), true);
    std::fs::write(&path, bytes).expect("write");
    harness
        .send(page_message(PluginsMsg::Path(path.display().to_string())))
        .await;
    harness.send(page_message(PluginsMsg::Preview)).await;
    harness.send(page_message(PluginsMsg::Install)).await;
    assert_eq!(page(&harness).selected.as_deref(), Some(ID));
    (harness, db)
}

fn page(harness: &Harness) -> &PluginsPage {
    &harness.app.settings.as_ref().expect("open").plugins
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
            duration_ms: 5,
        })
        .await
        .expect("record");
    }
}

#[tokio::test]
async fn the_latest_calls_are_closed_open_on_the_toggle_and_zero_leaves_them_out() {
    let root = tempfile::tempdir().expect("temp dir");
    let (mut harness, db) = installed(&root).await;
    record(&db, 30, "ok", None).await;
    record(&db, 1, "timeout", Some("plugin_timed_out")).await;
    harness
        .send(page_message(PluginsMsg::Select(ID.to_owned())))
        .await;

    let listed = page(&harness);
    let summary = listed.calls.summary.as_ref().expect("a summary");
    assert_eq!((summary.calls, summary.failures), (31, 1));
    assert_eq!(summary.last_failure.as_deref(), Some("plugin_timed_out"));
    assert_eq!(listed.calls.shown, 10, "the default");
    assert_eq!(listed.calls.latest.len(), 10, "the engine's bound");
    assert_eq!(shown(listed), Shown::Closed, "closed by default");

    harness.send(page_message(PluginsMsg::ToggleCalls)).await;
    match shown(page(&harness)) {
        Shown::Open(calls) => assert_eq!(calls.len(), 10),
        other => panic!("open, got {other:?}"),
    }
    // The same plugin read again stays open; another selection would close it.
    harness
        .send(page_message(PluginsMsg::Select(ID.to_owned())))
        .await;
    assert!(page(&harness).calls_open);

    harness
        .app
        .engine()
        .set_protected_setting(RECENT_CALLS_SHOWN_KEY, &json!(0))
        .await
        .expect("the window may");
    // Back on "Installed" from the page's settings, the list follows them.
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Installed)))
        .await;
    let listed = page(&harness);
    assert_eq!(shown(listed), Shown::Hidden);
    assert!(listed.calls.latest.is_empty());
    assert_eq!(
        listed.calls.summary.as_ref().map(|summary| summary.calls),
        Some(31),
        "the summary stays"
    );
}
