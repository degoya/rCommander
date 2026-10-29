use rc_engine::{SETTINGS, settings::SettingPage};

use super::{APPEARANCE, APPEARANCE_TABS, PLUGINS, Tab, Tabs, appearance_tab, of, shows};
use crate::app::{
    Message,
    harness::Harness,
    settings::{Page, SettingsMessage},
};

fn rows_of(page: SettingPage) -> Vec<&'static str> {
    SETTINGS
        .iter()
        .filter(|spec| spec.page == page)
        .map(|spec| spec.key)
        .collect()
}

/// Every schema row of the Appearance page is in exactly one tab, every row a tab names is one
/// of the page's, and no tab is empty.
#[test]
fn every_appearance_row_is_in_exactly_one_tab() {
    let rows = rows_of(SettingPage::Appearance);
    for key in &rows {
        let groups = APPEARANCE
            .iter()
            .filter(|(_, keys)| keys.contains(key))
            .count();
        assert_eq!(groups, 1, "{key} is in {groups} tabs");
    }
    for (tab, keys) in APPEARANCE {
        assert!(APPEARANCE_TABS.contains(tab), "{tab:?}");
        for key in *keys {
            assert!(rows.contains(key), "{key} is no row of the page");
        }
    }
    for tab in APPEARANCE_TABS {
        assert!(
            rows.iter().any(|key| appearance_tab(key) == tab),
            "{tab:?} is empty"
        );
    }
    let shown: usize = APPEARANCE_TABS
        .iter()
        .map(|tab| {
            SETTINGS
                .iter()
                .filter(|spec| spec.page == SettingPage::Appearance && shows(Some(*tab), spec))
                .count()
        })
        .sum();
    assert_eq!(shown, rows.len(), "the tabs draw every row once");
}

/// "Settings" draws exactly the page's `plugins.*` rows, "Installed" none of them.
#[test]
fn the_plugin_settings_tab_has_the_plugins_rows_and_installed_none() {
    let rows = rows_of(SettingPage::Plugins);
    assert!(!rows.is_empty());
    for spec in SETTINGS
        .iter()
        .filter(|spec| spec.page == SettingPage::Plugins)
    {
        assert!(spec.key.starts_with("plugins."), "{}", spec.key);
        assert!(shows(Some(Tab::PluginSettings), spec), "{}", spec.key);
        assert!(!shows(Some(Tab::Installed), spec), "{}", spec.key);
    }
    for spec in SETTINGS
        .iter()
        .filter(|spec| spec.key.starts_with("plugins."))
    {
        assert_eq!(spec.page, SettingPage::Plugins, "{}", spec.key);
    }
}

/// Each page keeps its own tab; a page without tabs has none and shows every row.
#[test]
fn each_page_keeps_its_own_tab() {
    let mut tabs = Tabs::default();
    assert_eq!(tabs.chosen(Page::Plugins), Some(Tab::Installed));
    assert_eq!(tabs.chosen(Page::Appearance), Some(Tab::Window));
    assert_eq!(tabs.chosen(Page::Folders), None);
    assert!(of(Page::Folders).is_empty());
    assert_eq!(of(Page::Plugins), PLUGINS.as_slice());
    tabs.choose(Tab::Viewer);
    tabs.choose(Tab::PluginSettings);
    assert_eq!(tabs.chosen(Page::Appearance), Some(Tab::Viewer));
    assert_eq!(tabs.chosen(Page::Plugins), Some(Tab::PluginSettings));
    tabs.turn(Page::Appearance, false);
    assert_eq!(tabs.chosen(Page::Appearance), Some(Tab::Window), "round");
    tabs.turn(Page::Folders, false);
    assert_eq!(tabs.chosen(Page::Plugins), Some(Tab::PluginSettings));
    tabs.first(Page::Plugins);
    assert_eq!(tabs.chosen(Page::Plugins), Some(Tab::Installed));
}

fn chosen(harness: &Harness, page: Page) -> Option<Tab> {
    harness.app.settings_tabs.chosen(page)
}

/// Ctrl+Tab and Ctrl+Shift+Tab turn the tabs of the page shown; the tab chosen outlives the
/// window for the session.
#[tokio::test]
async fn ctrl_tab_turns_the_tabs_and_the_choice_outlives_the_window() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Appearance)))
        .await;
    harness.press("Ctrl+Tab").await;
    assert_eq!(chosen(&harness, Page::Appearance), Some(Tab::Fonts));
    harness.press("Ctrl+Shift+Tab").await;
    harness.press("Ctrl+Shift+Tab").await;
    assert_eq!(
        chosen(&harness, Page::Appearance),
        Some(Tab::Viewer),
        "back round from the first to the last"
    );
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Plugins)))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::PluginSettings)))
        .await;
    harness.press("Escape").await;
    assert!(harness.app.settings.is_none());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    assert_eq!(chosen(&harness, Page::Plugins), Some(Tab::PluginSettings));
    assert_eq!(chosen(&harness, Page::Appearance), Some(Tab::Viewer));
}

/// The Keyboard page (RC-100-21): its schema rows under General, the table of keys under
/// Shortcuts - and the keymap's deviations, a schema row too, as no row on either.
#[test]
fn the_keyboard_rows_are_under_general_and_the_keys_under_shortcuts() {
    assert_eq!(of(Page::Keyboard), [Tab::Keys, Tab::Shortcuts]);
    assert_eq!(Tabs::default().chosen(Page::Keyboard), Some(Tab::Keys));
    for spec in SETTINGS
        .iter()
        .filter(|spec| spec.page == SettingPage::Keyboard)
    {
        let keymap = spec.key == rc_engine::settings::KEYMAP_KEY;
        assert_eq!(shows(Some(Tab::Keys), spec), !keymap, "{}", spec.key);
        assert!(!shows(Some(Tab::Shortcuts), spec), "{}", spec.key);
        assert_eq!(shows(None, spec), !keymap, "{}", spec.key);
    }
    let mut tabs = Tabs::default();
    tabs.choose(Tab::Shortcuts);
    assert_eq!(tabs.chosen(Page::Keyboard), Some(Tab::Shortcuts));
    assert_eq!(tabs.chosen(Page::Appearance), Some(Tab::Window), "its own");
}
