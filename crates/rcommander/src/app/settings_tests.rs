use std::collections::BTreeMap;

use iced::theme::Mode;
use rc_engine::{
    PanelId, SETTINGS,
    settings::{SettingKind, SettingSpec},
};

use super::{Page, SettingsMessage, view::dialog_page};
use crate::{
    app::{Message, harness::Harness},
    mcp::McpStatus,
    theme::ThemeChoice,
};

fn catalogue() -> BTreeMap<String, String> {
    serde_json::from_str(include_str!("../../locales/en.json")).expect("en.json parses")
}

/// The i18n keys a schema row names: its label, its help, its choices.
fn keys_of(spec: &SettingSpec) -> Vec<&'static str> {
    let mut keys = vec![spec.label, spec.help];
    if let SettingKind::Choice { choices, .. } = spec.kind {
        keys.extend(choices.iter().map(|choice| choice.label));
    }
    keys
}

/// Every setting of the schema has its label, help and choices in `en.json`, and every
/// `setting.` key of the catalogue belongs to a row (the lint `i18n_keys` leaves them to this
/// test, as the dialog looks them up through the schema).
#[test]
fn every_schema_row_has_its_words_and_no_word_is_left_over() {
    let catalogue = catalogue();
    let named: Vec<&str> = SETTINGS.iter().flat_map(keys_of).collect();
    for key in &named {
        assert!(catalogue.contains_key(*key), "missing in en.json: {key}");
    }
    let unused: Vec<&String> = catalogue
        .keys()
        .filter(|key| key.starts_with("setting.") && !named.contains(&key.as_str()))
        .collect();
    assert!(unused.is_empty(), "no schema row names {unused:?}");
}

/// Every schema row is drawn on a page of the dialog.
#[test]
fn every_schema_row_has_a_page() {
    for spec in SETTINGS {
        assert!(Page::ALL.contains(&dialog_page(spec.page)), "{}", spec.key);
    }
}

#[tokio::test]
async fn the_theme_changes_at_once_and_system_follows_the_system() {
    let (_root, mut harness) = Harness::temp().await;
    harness
        .send(Message::Settings(SettingsMessage::Theme(ThemeChoice::Dark)))
        .await;
    assert!(harness.app.dark);
    harness.send(Message::SystemTheme(Mode::Light)).await;
    assert!(harness.app.dark, "a fixed look ignores the system");

    harness
        .send(Message::Settings(SettingsMessage::Theme(
            ThemeChoice::System,
        )))
        .await;
    assert!(!harness.app.dark, "the system is light");
    harness.send(Message::SystemTheme(Mode::Dark)).await;
    assert!(harness.app.dark, "and follows it when it turns dark");

    harness
        .send(Message::Settings(SettingsMessage::Theme(
            ThemeChoice::Light,
        )))
        .await;
    assert!(!harness.app.dark);
}

#[tokio::test]
async fn the_open_dialog_keeps_keys_from_the_panels_and_escape_closes_it() {
    let (_root, mut harness) = Harness::temp().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    assert!(harness.app.settings.is_some());
    harness.press("Tab").await;
    assert_eq!(
        harness.app.active,
        PanelId::Left,
        "Tab did not reach the panels"
    );
    // Tab and Shift+Tab walk the page's fields instead (Windows test 2026-10-18).
    harness.press("Shift+Tab").await;
    assert_eq!(harness.fields_walked, [false, true]);
    harness.press("Escape").await;
    assert!(harness.app.settings.is_none());
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Right);
}

/// The X of the settings' window closes them as Escape does (RC-080-15).
#[tokio::test]
async fn the_close_x_closes_the_dialog_as_escape_does() {
    let (_root, mut harness) = Harness::temp().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    assert!(harness.app.settings.is_some());
    harness.send(Message::close_dialog()).await;
    assert!(harness.app.settings.is_none());
}

#[tokio::test]
async fn without_a_database_the_dialog_says_so() {
    let (_root, mut harness) = Harness::temp().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let dialog = harness.app.settings.as_ref().expect("open");
    assert!(dialog.note.as_ref().is_some_and(|(_, error)| *error));
    harness
        .send(Message::Settings(SettingsMessage::Cancel))
        .await;
    assert!(harness.app.settings.is_none());
}

/// The audit's D8: a change that failed after the MCP switch skipped the server's restart, so
/// the server ran on as before while the stored switch said otherwise.
#[tokio::test]
async fn a_failed_change_after_the_mcp_switch_still_restarts_the_server() {
    let root = tempfile::tempdir().expect("temp dir");
    let db = rc_db::Db::open_in_memory().await.expect("db");
    let harness = Harness::on_db(root.path(), db.clone()).await;
    let changes = super::Changes {
        settings: vec![
            ("mcp.http.enabled", serde_json::Value::Bool(false)),
            ("no.such.setting", serde_json::Value::Bool(true)),
        ],
        ..super::Changes::default()
    };
    let server = crate::mcp::McpServer::default();
    let stored = super::save::store(harness.app.engine().clone(), db, server, changes).await;
    assert!(stored.error.is_some(), "the unknown key is refused");
    assert_eq!(stored.mcp, Some(McpStatus::Off), "restarted, switched off");
}

/// The settings open over `db`, loaded.
async fn open_on(harness: &mut Harness, db: rc_db::Db) {
    harness.app.db = Some(db.clone());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let loaded = super::save::load(harness.app.engine().clone(), db).await;
    harness
        .send(Message::Settings(SettingsMessage::Loaded(
            loaded.map(Box::new),
        )))
        .await;
}

fn saved(error: Option<&'static str>, close: bool) -> Message {
    Message::Settings(SettingsMessage::Saved {
        result: super::Stored {
            mcp: Some(McpStatus::Failed("restarted".into())),
            error,
        },
        changes: Box::default(),
        close,
    })
}

/// The audit's D8: Escape while Apply stored the changes dropped the answer - the MCP server's
/// new state and a failure. Both reach the window; a dialog opened again is not the one that
/// saved.
#[tokio::test]
async fn escape_while_saving_keeps_the_answer_for_the_window() {
    let root = tempfile::tempdir().expect("temp dir");
    let db = rc_db::Db::open_in_memory().await.expect("db");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    open_on(&mut harness, db.clone()).await;
    harness
        .send(Message::Settings(SettingsMessage::Toggle(
            rc_engine::settings::ALTERNATE_ROWS_KEY,
            false,
        )))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Apply))
        .await;
    assert!(
        harness
            .app
            .settings
            .as_ref()
            .is_some_and(|dialog| dialog.saving)
    );
    harness.press("Escape").await;
    assert!(harness.app.settings.is_none());

    harness.send(saved(Some("database_error"), false)).await;
    assert_eq!(harness.app.mcp, McpStatus::Failed("restarted".into()));
    assert!(harness.app.info.is_some(), "the failure in the status bar");

    open_on(&mut harness, db).await;
    harness.send(saved(None, true)).await;
    assert!(harness.app.settings.is_some(), "the new dialog stays open");
}
