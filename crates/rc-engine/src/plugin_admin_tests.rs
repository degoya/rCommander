//! The `plugins.*` commands over a real plugin folder and database: preview, trust on first use,
//! install, the update that waits for the next start, switch, rollback, resume, remove, the
//! protected settings and the execution log written through.

use std::{path::Path, sync::Arc};

use rc_db::Db;
use rc_plugin_host::{
    ExecutionLog, KeyStatus, PluginIdentity, installed, key_fingerprint, sha256_hex,
    test_support::{signing_key, viewer_package},
};
use serde_json::json;

use crate::{
    Command, Engine, Outcome, PluginInfo, PluginsOutcome,
    settings::{ASSOCIATIONS_KEY, DEVELOPMENT_MODE_KEY, PLUGINS_ENABLED_KEY, parse_associations},
    test_support,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";
const KEY_ID: &str = "author-v1";

struct Setup {
    root: tempfile::TempDir,
    engine: Engine,
    db: Db,
}

impl Setup {
    async fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let engine = test_support::headless(root.path()).await;
        let db = Db::open_in_memory().await.expect("db");
        assert!(engine.attach_db(db.clone()));
        assert!(engine.attach_plugins(root.path().join("data"), None));
        Self { root, engine, db }
    }

    fn data(&self) -> &Path {
        self.engine.plugins_data_dir().expect("attached")
    }

    /// A package of `version` written beside the data folder; its path.
    fn package(&self, version: &str) -> String {
        let path = self.root.path().join(format!("csv-{version}.rcplug"));
        let bytes = viewer_package(ID, version, KEY_ID, &signing_key(7), true);
        std::fs::write(&path, bytes).expect("write");
        path.display().to_string()
    }

    async fn run(&self, command: Command) -> PluginsOutcome {
        match self.engine.dispatch(command).await.expect("the command") {
            Outcome::Plugins(outcome) => *outcome,
            other => panic!("a plugins outcome, got {other:?}"),
        }
    }

    async fn install(&self, version: &str) -> PluginsOutcome {
        let path = self.package(version);
        self.run(Command::InstallPlugin {
            digest: digest(&path),
            path,
            fingerprint: Some(fingerprint()),
        })
        .await
    }

    async fn plugin(&self) -> PluginInfo {
        match self
            .run(Command::ListPlugins {
                id: Some(ID.into()),
            })
            .await
        {
            PluginsOutcome::List { mut plugins, .. } => plugins.remove(0),
            other => panic!("a list, got {other:?}"),
        }
    }

    /// The version `rc_plugin_host::installed::list` runs, and whether it is on.
    fn runs(&self) -> Option<(String, bool)> {
        installed::list(self.data())
            .expect("list")
            .first()
            .map(|plugin| (plugin.manifest.version.clone(), plugin.enabled))
    }
}

fn fingerprint() -> String {
    key_fingerprint(&signing_key(7).verifying_key())
}

/// The SHA-256 of the file at `path`, as a preview reports it.
fn digest(path: &str) -> String {
    sha256_hex(&std::fs::read(path).expect("read"))
}

fn states(plugin: &PluginInfo) -> Vec<(String, String)> {
    plugin
        .versions
        .iter()
        .map(|version| (version.version.clone(), version.state.clone()))
        .collect()
}

#[tokio::test]
async fn a_preview_asks_about_the_key_and_the_install_needs_its_fingerprint() {
    let setup = Setup::new().await;
    let path = setup.package("1.0.0");
    let PluginsOutcome::Preview {
        preview,
        installable,
        installed_versions,
        ..
    } = setup
        .run(Command::PreviewPlugin { path: path.clone() })
        .await
    else {
        panic!("a preview");
    };
    assert_eq!(preview.key, KeyStatus::Untrusted);
    assert_eq!(preview.fingerprint, Some(fingerprint()));
    assert!(installable && installed_versions.is_empty());
    assert!(setup.runs().is_none(), "a preview installs nothing");

    for confirmed in [None, Some("00".repeat(32))] {
        let refused = setup
            .engine
            .dispatch(Command::InstallPlugin {
                path: path.clone(),
                fingerprint: confirmed,
                digest: digest(&path),
            })
            .await
            .expect_err("not confirmed");
        assert_eq!(refused.code(), "plugin_key_untrusted");
    }
    let PluginsOutcome::Installed {
        plugin, runs_now, ..
    } = setup.install("1.0.0").await
    else {
        panic!("installed");
    };
    assert!(runs_now && plugin.enabled && plugin.signed);
    assert_eq!(plugin.key_fingerprint, Some(fingerprint()));
    assert_eq!(
        setup.db.plugin_keys().await.expect("keys")[0].key_id,
        KEY_ID
    );
    assert_eq!(setup.runs(), Some(("1.0.0".into(), true)));
}

#[tokio::test]
async fn an_update_runs_from_the_next_start_and_a_rollback_returns_to_the_previous() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let PluginsOutcome::Installed { runs_now, .. } = setup.install("1.1.0").await else {
        panic!("installed");
    };
    assert!(!runs_now);
    assert_eq!(setup.runs(), Some(("1.0.0".into(), true)));
    let plugin = setup.plugin().await;
    assert_eq!(
        states(&plugin),
        [
            ("1.1.0".into(), "pending".into()),
            ("1.0.0".into(), "active".into())
        ]
    );

    setup.engine.start_plugins().await.expect("start");
    assert_eq!(setup.runs(), Some(("1.1.0".into(), true)));

    setup.run(Command::RollbackPlugin { id: ID.into() }).await;
    assert_eq!(setup.runs(), Some(("1.0.0".into(), true)));
    let again = setup
        .engine
        .dispatch(Command::RollbackPlugin { id: ID.into() })
        .await
        .expect_err("nothing older");
    assert_eq!(again.code(), "plugin_no_previous_version");
}

#[tokio::test]
async fn disabling_stops_it_from_running_and_the_master_switch_stops_all() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let mut events = setup.engine.subscribe();
    setup.run(Command::DisablePlugin { id: ID.into() }).await;
    // A cache of loaded plugins (the viewer adapter) hears of it.
    let mut announced = false;
    while let Ok(event) = events.try_recv() {
        announced |= matches!(event, crate::EngineEvent::PluginsChanged);
    }
    assert!(announced);
    assert_eq!(setup.runs(), Some(("1.0.0".into(), false)));
    assert!(!setup.plugin().await.enabled);
    setup.run(Command::EnablePlugin { id: ID.into() }).await;
    assert_eq!(setup.runs(), Some(("1.0.0".into(), true)));

    setup
        .engine
        .set_protected_setting(PLUGINS_ENABLED_KEY, &json!(false))
        .await
        .expect("the window may");
    assert_eq!(setup.runs(), Some(("1.0.0".into(), false)));
    let missing = setup
        .engine
        .dispatch(Command::EnablePlugin { id: "nope".into() })
        .await
        .expect_err("unknown");
    assert_eq!(missing.code(), "plugin_not_found");
}

#[tokio::test]
async fn a_version_in_use_is_not_removed_and_the_last_removal_forgets_the_plugin() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let dir = installed::list(setup.data()).expect("list")[0]
        .version_dir
        .clone();
    let held = installed::hold(&dir);
    let refused = setup
        .engine
        .dispatch(Command::RemovePlugin {
            id: ID.into(),
            version: "1.0.0".into(),
        })
        .await
        .expect_err("in use");
    assert_eq!(refused.code(), "plugin_version_in_use");
    assert!(setup.plugin().await.versions[0].in_use);
    drop(held);
    setup
        .run(Command::RemovePlugin {
            id: ID.into(),
            version: "1.0.0".into(),
        })
        .await;
    assert!(setup.runs().is_none());
    assert_eq!(setup.db.plugin(ID).await.expect("read"), None);
}

#[tokio::test]
async fn a_suspended_plugin_runs_again_after_resume() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let health = setup.engine.plugin_health(ID).await;
    for _ in 0..rc_plugin_host::DEFAULT_CRASH_LIMIT {
        health.record_failure();
    }
    assert!(setup.plugin().await.suspended);
    let PluginsOutcome::Changed { plugin } =
        setup.run(Command::ResumePlugin { id: ID.into() }).await
    else {
        panic!("changed");
    };
    assert!(!plugin.suspended && !health.is_suspended());
}

/// The acceptance test of RC-100-09: `settings.set` refuses every `plugins.*` and `plugin.*` key
/// (`setting_protected`), from MCP or not; the window's way writes the state at once.
#[tokio::test]
async fn settings_set_refuses_every_plugin_key_and_the_window_reaches_the_state() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let keys = [
        PLUGINS_ENABLED_KEY,
        "plugins.crash_limit",
        "plugins.viewer_output_limit_mib",
        "plugins.viewer_token_colors",
        ASSOCIATIONS_KEY,
        DEVELOPMENT_MODE_KEY,
        "plugin.019d0000-0000-7000-8000-0000000000a1.separator",
    ];
    for key in keys {
        for via_mcp in [false, true] {
            let command = Command::SetSetting {
                key: key.into(),
                value: json!(true),
            };
            let refused = if via_mcp {
                setup.engine.dispatch_mcp(command).await
            } else {
                setup.engine.dispatch(command).await
            };
            assert_eq!(refused.expect_err(key).code(), "setting_protected", "{key}");
        }
    }
    setup
        .engine
        .set_protected_setting(DEVELOPMENT_MODE_KEY, &json!(true))
        .await
        .expect("the window may");
    let state = rc_plugin_host::PluginState::read(setup.data()).expect("state");
    assert!(state.development_mode);

    let association = format!("CSV={ID} .tsv={ID}");
    let stored = setup
        .engine
        .set_protected_setting(ASSOCIATIONS_KEY, &json!(association))
        .await
        .expect("an association");
    assert_eq!(stored.value, json!(format!("csv={ID} tsv={ID}")));
    assert_eq!(
        parse_associations(&format!("csv={ID}")).expect("pairs"),
        [("csv".to_owned(), ID.to_owned())]
    );
    assert!(parse_associations("csv=not-an-id").is_none());
}

#[tokio::test]
async fn a_plugin_setting_takes_only_what_its_manifest_declares() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let plugin = setup
        .engine
        .set_plugin_setting(ID, "separator", "semicolon")
        .await
        .expect("a declared value");
    assert_eq!(plugin.settings[0].value.as_deref(), Some("semicolon"));
    assert_eq!(plugin.settings[0].setting, format!("plugin.{ID}.separator"));
    for (name, value) in [("separator", "pipe"), ("colour", "red")] {
        let refused = setup
            .engine
            .set_plugin_setting(ID, name, value)
            .await
            .expect_err("refused");
        assert_eq!(refused.code(), "plugin_setting_refused");
    }
}

#[tokio::test]
async fn every_call_is_written_into_the_execution_table() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let log: Arc<ExecutionLog> = setup.engine.plugin_execution_log().expect("a log");
    let identity = PluginIdentity {
        id: ID.into(),
        name: "CSV table".into(),
        version: "1.0.0".into(),
        plugin_type: "viewer",
    };
    let failed: Result<(), rc_plugin_types::Failure> = Err(rc_plugin_types::Failure::coded(
        rc_plugin_types::FailureKind::Other,
        "plugin_crashed",
    ));
    log.finish(&identity, log.begin("open"), &failed);
    let mut written = Vec::new();
    for _ in 0..100 {
        written = setup.db.plugin_executions(ID, 10).await.expect("read");
        if !written.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(written.len(), 1);
    assert_eq!(
        (written[0].operation.as_str(), written[0].outcome.as_str()),
        ("open", "crash")
    );
    assert_eq!(written[0].code.as_deref(), Some("plugin_crashed"));
    match setup
        .run(Command::ListPlugins {
            id: Some(ID.into()),
        })
        .await
    {
        PluginsOutcome::List { executions, .. } => assert_eq!(executions.len(), 1),
        other => panic!("a list, got {other:?}"),
    }
}

#[test]
fn every_refusal_code_of_the_host_is_a_code_of_the_engine() {
    for code in rc_plugin_host::REFUSAL_CODES {
        assert!(crate::PluginAdminError::ALL_CODES.contains(code), "{code}");
    }
}

/// RC-080-47: a package path is read only as a regular file within the largest package's size;
/// a folder (as a pipe would be) is refused before it is opened.
#[tokio::test]
async fn a_package_that_is_no_regular_file_is_refused() {
    let setup = Setup::new().await;
    let folder = setup.root.path().display().to_string();
    for command in [
        Command::PreviewPlugin {
            path: folder.clone(),
        },
        Command::InstallPlugin {
            path: folder.clone(),
            fingerprint: None,
            digest: String::new(),
        },
    ] {
        let refused = setup.engine.dispatch(command).await.expect_err("a folder");
        assert_eq!(refused.code(), "plugin_invalid");
        assert!(
            refused.to_string().contains("not a regular file"),
            "{refused}"
        );
    }
}

/// RC-080-43 (S7): a package for an installed plugin signed by another key - under a key id of
/// its own - is shown as changed and refused, before its key is trusted: the stranger's key
/// inherits neither the plugin nor a place in the trust store.
#[tokio::test]
async fn an_update_under_another_key_is_refused_before_its_key_is_trusted() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    let path = setup.root.path().join("stranger.rcplug");
    let stranger = signing_key(9);
    std::fs::write(
        &path,
        viewer_package(ID, "1.1.0", "stranger-v1", &stranger, true),
    )
    .expect("write");
    let path = path.display().to_string();
    let PluginsOutcome::Preview {
        preview,
        installable,
        ..
    } = setup
        .run(Command::PreviewPlugin { path: path.clone() })
        .await
    else {
        panic!("a preview");
    };
    assert_eq!(preview.key, KeyStatus::Changed);
    assert!(!installable);
    let refused = setup
        .engine
        .dispatch(Command::InstallPlugin {
            digest: digest(&path),
            path,
            fingerprint: Some(key_fingerprint(&stranger.verifying_key())),
        })
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "plugin_key_changed");
    let keys = setup.db.plugin_keys().await.expect("keys");
    assert!(keys.iter().all(|key| key.key_id != "stranger-v1"));
    assert_eq!(setup.runs(), Some(("1.0.0".to_owned(), true)));
}

/// RC-080-53: the install names the digest of the file its preview read; a package swapped since -
/// a valid one, signed by the same key - is refused, and nothing is installed.
#[tokio::test]
async fn a_package_swapped_after_its_preview_is_refused() {
    let setup = Setup::new().await;
    let path = setup.package("1.0.0");
    let PluginsOutcome::Preview { digest: shown, .. } = setup
        .run(Command::PreviewPlugin { path: path.clone() })
        .await
    else {
        panic!("a preview");
    };
    assert_eq!(shown, digest(&path));
    let swapped = viewer_package(ID, "1.0.1", KEY_ID, &signing_key(7), true);
    std::fs::write(&path, swapped).expect("swap");
    let refused = setup
        .engine
        .dispatch(Command::InstallPlugin {
            path,
            fingerprint: Some(fingerprint()),
            digest: shown,
        })
        .await
        .expect_err("swapped");
    assert_eq!(refused.code(), "plugin_package_changed");
    assert!(setup.runs().is_none(), "nothing installed");
}
