//! `plugins.remove_old` (RC-110-55) over a real plugin folder and database: every version that
//! does not run and every unverified one go, a rollback's target included (the owner,
//! 2026-11-27); the version that runs, the staged one, the pending one, a held one and a bundled
//! one the shipped packages hold stay; a second run finds nothing.

use std::path::{Path, PathBuf};

use rc_db::Db;
use rc_plugin_host::{
    PluginInstaller, PluginState, installed, key_fingerprint, public_key_base64, sha256_hex,
    test_support::{signing_key, viewer_package},
};

use crate::{
    Command, Engine, EngineEvent, OldReason, OldVersion, Outcome, PluginVersionRef, PluginsOutcome,
    test_support,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";
/// A plugin signed by a key this engine does not trust: its only version does not verify.
const STRANGER: &str = "019d0000-0000-7000-8000-0000000000b2";

struct Setup {
    root: tempfile::TempDir,
    engine: Engine,
    db: Db,
}

impl Setup {
    /// An engine with a database, a plugin folder and an empty folder of shipped packages.
    async fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let engine = test_support::headless(root.path()).await;
        let db = Db::open_in_memory().await.expect("db");
        assert!(engine.attach_db(db.clone()));
        std::fs::create_dir_all(root.path().join("shipped")).expect("shipped");
        assert!(engine.attach_plugins(root.path().join("data"), Some(root.path().join("shipped"))));
        Self { root, engine, db }
    }

    fn data(&self) -> &Path {
        self.engine.plugins_data_dir().expect("attached")
    }

    fn version_dir(&self, id: &str, version: &str) -> PathBuf {
        installed::plugins_root(self.data()).join(id).join(version)
    }

    /// Installs `version` of `ID` as the window does; from the second, it runs from the next
    /// start.
    async fn install(&self, version: &str) {
        let path = self.root.path().join(format!("csv-{version}.rcplug"));
        let bytes = viewer_package(ID, version, "author-v1", &signing_key(7), true);
        std::fs::write(&path, &bytes).expect("write");
        self.engine
            .dispatch(Command::InstallPlugin {
                path: path.display().to_string(),
                fingerprint: Some(key_fingerprint(&signing_key(7).verifying_key())),
                digest: sha256_hex(&bytes),
            })
            .await
            .expect("installed");
    }

    /// Installs `STRANGER` 2.0.0 past the engine, under a key it never trusted.
    fn install_stranger(&self) {
        let mut trusting = PluginState::default();
        trusting
            .trusted_keys
            .insert("stranger-v1".to_owned(), public_key_base64(&signing_key(9)));
        let installer = PluginInstaller::new(
            installed::plugins_root(self.data()),
            trusting.verifier().expect("verifier"),
        );
        installer
            .install_bytes(&viewer_package(
                STRANGER,
                "2.0.0",
                "stranger-v1",
                &signing_key(9),
                true,
            ))
            .expect("install");
    }

    /// Marks `version` of `ID` as installed from the shipped packages.
    fn mark_bundled(&self, version: &str) {
        std::fs::write(self.version_dir(ID, version).join("bundled"), b"").expect("marker");
    }

    /// Puts `version` of `ID` among the shipped packages.
    fn ship(&self, version: &str) {
        let bytes = viewer_package(ID, version, "author-v1", &signing_key(7), true);
        std::fs::write(self.root.path().join("shipped").join("csv.rcplug"), bytes).expect("ship");
    }

    async fn run(&self, versions: Option<Vec<PluginVersionRef>>) -> PluginsOutcome {
        match self
            .engine
            .dispatch(Command::RemoveOldPlugins { versions })
            .await
            .expect("the command")
        {
            Outcome::Plugins(outcome) => *outcome,
            other => panic!("a plugins outcome, got {other:?}"),
        }
    }

    /// What "Remove old versions..." would remove, and the held ones.
    async fn preview(&self) -> (Vec<OldVersion>, Vec<OldVersion>) {
        match self.run(None).await {
            PluginsOutcome::OldVersions { versions, held } => (versions, held),
            other => panic!("the old versions, got {other:?}"),
        }
    }

    /// The installed versions of `id`, newest first.
    fn installed(&self, id: &str) -> Vec<String> {
        let state = PluginState::read(self.data()).expect("state");
        installed::inventory(self.data(), &state)
            .expect("inventory")
            .into_iter()
            .filter(|entry| entry.id == id)
            .map(|entry| entry.version)
            .collect()
    }
}

fn named(versions: &[OldVersion]) -> Vec<(&str, &str, OldReason)> {
    versions
        .iter()
        .map(|old| (old.id.as_str(), old.version.as_str(), old.reason))
        .collect()
}

fn reference(old: &OldVersion) -> PluginVersionRef {
    PluginVersionRef {
        id: old.id.clone(),
        version: old.version.clone(),
    }
}

#[tokio::test]
async fn the_inactive_and_unverified_versions_go_and_every_kept_one_stays() {
    let setup = Setup::new().await;
    for version in ["0.9.0", "1.0.0", "1.1.0", "1.2.0", "1.3.0", "1.4.0"] {
        setup.install(version).await;
    }
    setup.engine.start_plugins().await.expect("start");
    // 1.3.0 runs, 1.4.0 is under test; 1.2.0, the rollback's target, goes as well.
    setup
        .db
        .set_plugin_versions(ID, Some("1.3.0"), Some("1.4.0"), None)
        .await
        .expect("versions");
    setup.engine.sync_plugin_state().await.expect("state");
    // 1.0.0 came with rCommander and still does; 1.1.0 came with an older build only.
    setup.mark_bundled("1.0.0");
    setup.mark_bundled("1.1.0");
    setup.ship("1.0.0");
    setup.install_stranger();
    let state = PluginState::read(setup.data()).expect("state");
    let held_dir = installed::inventory(setup.data(), &state)
        .expect("inventory")
        .into_iter()
        .find(|entry| entry.id == ID && entry.version == "0.9.0")
        .expect("0.9.0")
        .version_dir;
    let held = installed::hold(&held_dir);

    let (versions, shown_held) = setup.preview().await;
    assert_eq!(
        named(&versions),
        [
            (ID, "1.2.0", OldReason::Inactive),
            (ID, "1.1.0", OldReason::Inactive),
            (STRANGER, "2.0.0", OldReason::Refused)
        ]
    );
    assert_eq!(versions[2].refused.as_deref(), Some("plugin_key_untrusted"));
    assert_eq!(versions[0].name, "CSV table");
    assert_eq!(named(&shown_held), [(ID, "0.9.0", OldReason::Inactive)]);
    assert_eq!(setup.installed(ID).len(), 6, "the preview removes nothing");

    let confirmed = versions.iter().chain(&shown_held).map(reference).collect();
    let mut events = setup.engine.subscribe();
    let PluginsOutcome::OldRemoved { removed, skipped } = setup.run(Some(confirmed)).await else {
        panic!("removed");
    };
    assert_eq!(named(&removed), named(&versions));
    assert_eq!(named(&skipped), named(&shown_held));
    let mut announced = 0;
    while let Ok(event) = events.try_recv() {
        announced += usize::from(matches!(event, EngineEvent::PluginsChanged));
    }
    assert_eq!(announced, 1, "one PluginsChanged at the end");
    assert_eq!(setup.installed(ID), ["1.4.0", "1.3.0", "1.0.0", "0.9.0"]);
    assert!(setup.installed(STRANGER).is_empty());
    let row = setup.db.plugin(ID).await.expect("read").expect("a row");
    assert_eq!(row.active_version.as_deref(), Some("1.3.0"));
    assert_eq!(row.staged_version.as_deref(), Some("1.4.0"));

    // A second run finds nothing; the held version stays named until its viewer closes.
    let (versions, shown_held) = setup.preview().await;
    assert!(versions.is_empty(), "{versions:?}");
    assert_eq!(named(&shown_held), [(ID, "0.9.0", OldReason::Inactive)]);
    drop(held);
    let (versions, _) = setup.preview().await;
    assert_eq!(named(&versions), [(ID, "0.9.0", OldReason::Inactive)]);
}

#[tokio::test]
async fn only_confirmed_versions_still_old_go_and_one_held_since_is_named() {
    let setup = Setup::new().await;
    for version in ["1.0.0", "1.1.0", "1.2.0"] {
        setup.install(version).await;
    }
    setup.engine.start_plugins().await.expect("start");
    let (versions, held) = setup.preview().await;
    assert_eq!(
        named(&versions),
        [
            (ID, "1.1.0", OldReason::Inactive),
            (ID, "1.0.0", OldReason::Inactive)
        ]
    );
    assert!(held.is_empty());

    // Held after the list was read; the version that runs is named too, but is not old.
    let guard = installed::hold(&setup.version_dir(ID, "1.0.0"));
    let mut confirmed: Vec<PluginVersionRef> = versions.iter().map(reference).collect();
    confirmed.push(PluginVersionRef {
        id: ID.to_owned(),
        version: "1.2.0".to_owned(),
    });
    let PluginsOutcome::OldRemoved { removed, skipped } = setup.run(Some(confirmed)).await else {
        panic!("removed");
    };
    assert_eq!(named(&removed), [(ID, "1.1.0", OldReason::Inactive)]);
    assert_eq!(named(&skipped), [(ID, "1.0.0", OldReason::Inactive)]);
    assert_eq!(setup.installed(ID), ["1.2.0", "1.0.0"]);
    drop(guard);
}

#[tokio::test]
async fn a_pending_version_stays_and_one_rolled_back_from_goes() {
    let setup = Setup::new().await;
    setup.install("1.0.0").await;
    setup.install("1.1.0").await;
    // 1.1.0 runs from the next start.
    assert_eq!(setup.preview().await, (Vec::new(), Vec::new()));
    setup.engine.start_plugins().await.expect("start");
    setup
        .engine
        .dispatch(Command::RollbackPlugin { id: ID.into() })
        .await
        .expect("rolled back");
    // 1.0.0 runs again; 1.1.0, newer, does not run and is old as well.
    let (versions, held) = setup.preview().await;
    assert_eq!(named(&versions), [(ID, "1.1.0", OldReason::Inactive)]);
    assert!(held.is_empty());
}

#[tokio::test]
async fn without_a_plugin_folder_the_command_is_refused() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(root.path()).await;
    let refused = engine
        .dispatch(Command::RemoveOldPlugins { versions: None })
        .await
        .expect_err("no plugin folder");
    assert_eq!(refused.code(), "plugins_unavailable");
}

#[tokio::test]
async fn mcp_cannot_reach_it() {
    let setup = Setup::new().await;
    let refused = setup
        .engine
        .dispatch_mcp(Command::RemoveOldPlugins { versions: None })
        .await
        .expect_err("GUI only");
    assert_eq!(refused.code(), "gui_only");
}
