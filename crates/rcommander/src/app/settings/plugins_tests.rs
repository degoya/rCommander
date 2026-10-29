//! The Plugins page through `apply`, against an engine over a database and a plugin folder (the
//! GUI acceptance of RC-100-09): Install... previews the package, Install from the preview installs
//! it with the key it showed, Disable switches it off - each checked where the viewer adapter
//! reads it (`rc_plugin_host::installed::list`).

use rc_db::Db;
use rc_engine::KeyStatus;
use rc_plugin_host::{
    installed,
    test_support::{signing_key, viewer_package},
};

use super::{PluginsMsg, PluginsPage, page_message};
use crate::app::{
    Message,
    harness::{Harness, headless_engine},
    settings::{self, Page, SettingsMessage},
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";

/// The settings open on the Plugins page, over a database and a plugin folder in `root`.
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

fn package(root: &tempfile::TempDir, signed: bool) -> String {
    let path = root.path().join("csv.rcplug");
    let bytes = viewer_package(ID, "1.0.0", "author-v1", &signing_key(7), signed);
    std::fs::write(&path, bytes).expect("write");
    path.display().to_string()
}

/// What the viewer adapter would run: the version, and whether it is on.
fn runs(root: &tempfile::TempDir) -> Option<(String, bool)> {
    installed::list(&root.path().join("data"))
        .expect("list")
        .first()
        .map(|plugin| (plugin.manifest.version.clone(), plugin.enabled))
}

#[tokio::test]
async fn install_previews_then_confirms_and_disable_switches_it_off() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = shown(&root).await;
    assert_eq!(page(&harness).plugins.as_deref(), Some(&[][..]));

    let path = package(&root, true);
    harness.send(page_message(PluginsMsg::Path(path))).await;
    harness.send(page_message(PluginsMsg::Preview)).await;
    let preview = page(&harness).preview.clone().expect("the preview");
    assert_eq!(preview.preview.key, KeyStatus::Untrusted);
    assert!(preview.installable);
    assert!(
        runs(&root).is_none(),
        "nothing is installed before the confirmation"
    );

    harness.send(page_message(PluginsMsg::Install)).await;
    let shown = page(&harness);
    assert!(shown.preview.is_none() && !shown.busy);
    let plugins = shown.plugins.as_deref().expect("listed");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "CSV table");
    assert_eq!(shown.selected.as_deref(), Some(ID));
    assert_eq!(runs(&root), Some(("1.0.0".to_owned(), true)));

    harness
        .send(page_message(PluginsMsg::Enable(ID.to_owned(), false)))
        .await;
    let plugins = page(&harness).plugins.as_deref().expect("listed");
    assert!(!plugins[0].enabled);
    assert_eq!(runs(&root), Some(("1.0.0".to_owned(), false)));
    let note = harness
        .app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.note.clone())
        .expect("a note");
    assert!(!note.1, "not an error: {note:?}");
}

#[tokio::test]
async fn an_unsigned_package_is_shown_but_not_installed_outside_development_mode() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = shown(&root).await;
    harness
        .send(page_message(PluginsMsg::Path(package(&root, false))))
        .await;
    harness.send(page_message(PluginsMsg::Preview)).await;
    let preview = page(&harness).preview.clone().expect("the preview");
    assert_eq!(preview.preview.key, KeyStatus::Unsigned);
    assert!(!preview.installable);
    harness.send(page_message(PluginsMsg::Install)).await;
    assert!(runs(&root).is_none());
    harness.send(page_message(PluginsMsg::CancelPreview)).await;
    assert!(page(&harness).preview.is_none());
}

#[tokio::test]
async fn a_package_that_is_none_is_named_as_an_error() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = shown(&root).await;
    let path = root.path().join("broken.rcplug");
    std::fs::write(&path, b"not a zip").expect("write");
    harness
        .send(page_message(PluginsMsg::Path(path.display().to_string())))
        .await;
    harness.send(page_message(PluginsMsg::Preview)).await;
    let dialog = harness.app.settings.as_ref().expect("open");
    assert!(dialog.plugins.preview.is_none() && !dialog.plugins.busy);
    assert!(dialog.note.as_ref().is_some_and(|(_, error)| *error));
}

/// The audit's D8: Save took the plugin's typed settings before they were stored, so a refusal
/// lost them. They stay until the plugin took them.
#[tokio::test]
async fn a_refused_plugin_setting_keeps_what_was_typed() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = shown(&root).await;
    harness
        .send(page_message(PluginsMsg::Path(package(&root, true))))
        .await;
    harness.send(page_message(PluginsMsg::Preview)).await;
    harness.send(page_message(PluginsMsg::Install)).await;
    assert_eq!(page(&harness).selected.as_deref(), Some(ID));

    harness
        .send(page_message(PluginsMsg::Setting(
            "no_such_setting".into(),
            "typed".into(),
        )))
        .await;
    harness.send(page_message(PluginsMsg::SaveSettings)).await;
    let dialog = harness.app.settings.as_ref().expect("open");
    assert!(
        dialog.note.as_ref().is_some_and(|(_, error)| *error),
        "refused"
    );
    assert!(!dialog.plugins.busy);
    assert_eq!(
        dialog
            .plugins
            .drafts
            .get("no_such_setting")
            .map(String::as_str),
        Some("typed")
    );
}

/// RC-080-53: Install takes the package the preview read - a file swapped after the preview, under
/// the same key, is refused and named as an error, and nothing is installed.
#[tokio::test]
async fn a_package_swapped_after_the_preview_is_not_installed() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = shown(&root).await;
    let path = package(&root, true);
    harness
        .send(page_message(PluginsMsg::Path(path.clone())))
        .await;
    harness.send(page_message(PluginsMsg::Preview)).await;
    assert!(page(&harness).preview.is_some());
    let swapped = viewer_package(ID, "1.0.1", "author-v1", &signing_key(7), true);
    std::fs::write(&path, swapped).expect("swap");
    harness.send(page_message(PluginsMsg::Install)).await;
    assert!(
        runs(&root).is_none(),
        "the swapped package is not installed"
    );
    let dialog = harness.app.settings.as_ref().expect("open");
    assert!(!dialog.plugins.busy);
    assert!(dialog.note.as_ref().is_some_and(|(_, error)| *error));
}
