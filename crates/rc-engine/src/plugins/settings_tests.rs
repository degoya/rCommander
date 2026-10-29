//! The `plugins.*` settings as the viewer adapter reads them.

use rc_db::Db;
use rc_plugin_host::{SettingKind, SettingManifest};

use super::{PluginSettings, stored_plugin_settings};
use crate::settings::{ASSOCIATIONS_KEY, OUTPUT_LIMIT_KEY, PLUGINS_ENABLED_KEY};

#[tokio::test]
async fn without_a_database_the_defaults_apply() {
    let settings = PluginSettings::read(None).await;
    assert_eq!(settings, PluginSettings::default());
    assert!(settings.enabled, "plugins run unless switched off");
    assert_eq!(settings.output_limit, 256 * 1024 * 1024);
}

#[tokio::test]
async fn the_stored_switch_limit_and_associations_are_read() {
    let db = Db::open_in_memory().await.expect("db");
    let id = "019d0000-0000-7000-8000-0000000000a1";
    db.set(PLUGINS_ENABLED_KEY, &false).await.expect("stored");
    db.set(OUTPUT_LIMIT_KEY, &2_u64).await.expect("stored");
    db.set(ASSOCIATIONS_KEY, &format!("json={id} csv={id}"))
        .await
        .expect("stored");
    let settings = PluginSettings::read(Some(&db)).await;
    assert!(!settings.enabled);
    assert_eq!(settings.output_limit, 2 * 1024 * 1024);
    let pairs: Vec<(&str, &str)> = settings
        .associations
        .iter()
        .map(|(extension, id)| (extension.as_str(), id.as_str()))
        .collect();
    assert_eq!(pairs, [("csv", id), ("json", id)]);
}

/// A plugin's own declared settings as stored, nothing of another plugin's and no undeclared
/// key (RC-100-14); none without a database.
#[tokio::test]
async fn a_plugins_stored_settings_are_its_declared_ones() {
    let id = "019d0000-0000-7000-8000-0000000000a1";
    let other = "019d0000-0000-7000-8000-0000000000b2";
    let declared = [setting("format"), setting("label")];
    assert!(stored_plugin_settings(None, id, &declared).await.is_empty());

    let db = Db::open_in_memory().await.expect("db");
    db.set(&format!("plugin.{id}.format"), "pretty")
        .await
        .expect("stored");
    db.set(&format!("plugin.{id}.undeclared"), "x")
        .await
        .expect("stored");
    db.set(&format!("plugin.{other}.label"), "theirs")
        .await
        .expect("stored");
    assert_eq!(
        stored_plugin_settings(Some(&db), id, &declared).await,
        [("format".to_owned(), "pretty".to_owned())]
    );
}

fn setting(key: &str) -> SettingManifest {
    SettingManifest {
        key: key.to_owned(),
        kind: SettingKind::Text,
        values: Vec::new(),
        multiline: false,
        default: None,
        masked: false,
    }
}
