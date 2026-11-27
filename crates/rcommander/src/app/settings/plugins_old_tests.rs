//! "Remove old versions..." through `apply` (RC-110-55), against an engine over a database and a
//! plugin folder: the button waits while nothing is old, the question lists what goes and why,
//! Cancel and Escape leave everything, Remove removes it and the status line counts it.

use rc_db::Db;
use rc_engine::OldReason;
use rc_plugin_host::{
    PluginInstaller, PluginState, installed, public_key_base64,
    test_support::{signing_key, viewer_package},
};

use super::{OldMsg, reason};
use crate::{
    app::{
        Message,
        harness::{Harness, headless_engine},
        settings::{
            self, Page, SettingsMessage,
            plugins::{PluginsMsg, PluginsPage, page_message},
        },
    },
    i18n::t,
};

/// A plugin signed by a key this engine never trusted: its only version does not verify.
const STRANGER: &str = "019d0000-0000-7000-8000-0000000000b2";

/// Installs `STRANGER` 2.0.0 into the plugin folder of `root`, past the engine.
fn install_stranger(root: &tempfile::TempDir) {
    let mut trusting = PluginState::default();
    trusting
        .trusted_keys
        .insert("stranger-v1".to_owned(), public_key_base64(&signing_key(9)));
    let installer = PluginInstaller::new(
        installed::plugins_root(&root.path().join("data")),
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

fn installed_versions(root: &tempfile::TempDir) -> usize {
    let data = root.path().join("data");
    installed::inventory(&data, &PluginState::default())
        .expect("inventory")
        .len()
}

/// The settings open on the Plugins page, over a database and the plugin folder in `root`.
async fn shown(root: &tempfile::TempDir) -> Harness {
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    assert!(engine.attach_plugins(root.path().join("data"), None));
    let mut harness = Harness::new(engine);
    harness.app.db = Some(db.clone());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let loaded = settings::load(harness.app.engine().clone(), db).await;
    harness
        .send(Message::Settings(SettingsMessage::Loaded(
            loaded.map(Box::new),
        )))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Plugins)))
        .await;
    harness
}

fn page(harness: &Harness) -> &PluginsPage {
    &harness.app.settings.as_ref().expect("open").plugins
}

fn old(message: OldMsg) -> Message {
    page_message(PluginsMsg::Old(message))
}

#[tokio::test]
async fn without_old_versions_the_button_waits_and_asks_nothing() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = shown(&root).await;
    assert_eq!(page(&harness).plugins.as_deref(), Some(&[][..]));
    assert!(page(&harness).old.versions.is_empty());
    assert!(!page(&harness).old.offered(false));
    harness.send(old(OldMsg::Ask)).await;
    assert!(!page(&harness).old.asking);
}

#[tokio::test]
async fn the_question_lists_them_cancel_keeps_them_and_remove_counts_them() {
    let root = tempfile::tempdir().expect("temp dir");
    install_stranger(&root);
    let mut harness = shown(&root).await;
    let listed = &page(&harness).old;
    assert_eq!(listed.versions.len(), 1);
    let shown = &listed.versions[0];
    assert_eq!(
        (shown.id.as_str(), shown.version.as_str(), shown.reason),
        (STRANGER, "2.0.0", OldReason::Refused)
    );
    assert_eq!(reason(shown), t!("settings.plugins.old_key_untrusted"));
    assert!(listed.offered(false) && !listed.asking);

    // Cancel and Escape close the question; nothing is removed.
    harness.send(old(OldMsg::Ask)).await;
    assert!(page(&harness).old.asking);
    harness.send(old(OldMsg::Cancel)).await;
    assert!(!page(&harness).old.asking);
    harness.send(old(OldMsg::Ask)).await;
    harness.press("Escape").await;
    assert!(!page(&harness).old.asking);
    assert!(
        harness.app.settings.is_some(),
        "Escape took the question only"
    );
    assert_eq!(installed_versions(&root), 1);
    assert_eq!(page(&harness).old.versions.len(), 1);

    // Remove: gone, the button waits again and the status line counts it.
    harness.send(old(OldMsg::Ask)).await;
    harness.send(old(OldMsg::Confirm)).await;
    assert_eq!(installed_versions(&root), 0);
    let dialog = harness.app.settings.as_ref().expect("open");
    assert!(!dialog.plugins.busy && !dialog.plugins.old.asking);
    assert!(dialog.plugins.old.versions.is_empty());
    assert_eq!(
        dialog.note.clone(),
        Some((t!("settings.plugins.old_removed", count = 1), false))
    );
}
