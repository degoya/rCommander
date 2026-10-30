use std::collections::{BTreeSet, HashMap};

use rc_engine::Keymap;

use super::*;
use crate::{
    app::{
        harness::Harness,
        palette::{entries, reach},
        settings::{Page, Tab},
    },
    platform::menu_bar::mnemonic,
};

/// The palette's rows where network drives are mapped (Windows).
fn standard() -> Vec<Entry> {
    entries(&Keymap::standard(), &reach::standard(), true)
}

fn catalogue() -> HashMap<String, String> {
    serde_json::from_str(include_str!("../../../locales/en.json")).expect("en.json reads")
}

/// Every id of the menus with its slot, in order.
fn ids() -> Vec<&'static str> {
    MENUS
        .iter()
        .flat_map(|menu| menu.slots)
        .filter_map(|slot| match slot {
            Slot::Entry(id) | Slot::Dialog(id, _) => Some(*id),
            Slot::Separator => None,
        })
        .collect()
}

fn text_of(menus: &[BarMenu], id: &str) -> Option<String> {
    menus
        .iter()
        .flat_map(|menu| &menu.items)
        .find_map(|item| match item {
            BarItem::Entry { id: at, text } if *at == id => Some(text.clone()),
            _ => None,
        })
}

/// An entry is a row of the palette - the menu invents no command and no binding - and every
/// one is built where network drives are mapped.
#[test]
fn every_entry_is_a_row_of_the_palette() {
    let entries = standard();
    for menu in MENUS {
        for slot in menu.slots {
            if let Slot::Entry(name) = slot {
                assert!(
                    entries.iter().any(|entry| entry.name == *name),
                    "{name} in {} is no row of the palette",
                    menu.id
                );
            }
        }
    }
    let built = build(&Keymap::standard(), &entries);
    assert_eq!(built.len(), MENUS.len(), "no menu is left empty");
    let shown = built
        .iter()
        .flat_map(|menu| &menu.items)
        .filter(|item| matches!(item, BarItem::Entry { .. }))
        .count();
    assert_eq!(shown, ids().len(), "every entry is built");
}

#[test]
fn ids_are_unique_and_found() {
    let ids = ids();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "an id is one entry");
    for id in ids {
        assert!(slot(id).is_some(), "{id}");
    }
    assert_eq!(slot("no.such.entry"), None);
}

/// Every title and entry has its words in the catalogue with exactly one mnemonic; the
/// mnemonics differ within a menu and across the bar. Every `menubar.` key is used (the repo
/// lint leaves the prefix to this test).
#[test]
fn every_label_is_in_the_catalogue_with_one_mnemonic() {
    let catalogue = catalogue();
    let mut used = BTreeSet::new();
    let mut titles = BTreeSet::new();
    for menu in MENUS {
        let key = format!("menubar.title.{}", menu.id);
        let title = catalogue.get(&key).unwrap_or_else(|| panic!("{key}"));
        assert_eq!(title.matches('&').count(), 1, "{key}: {title}");
        assert!(titles.insert(mnemonic(title)), "{key}: mnemonic taken");
        used.insert(key);
        let mut letters = BTreeSet::new();
        for slot in menu.slots {
            let (Slot::Entry(id) | Slot::Dialog(id, _)) = slot else {
                continue;
            };
            let key = format!("menubar.{id}");
            let label = catalogue.get(&key).unwrap_or_else(|| panic!("{key}"));
            assert_eq!(label.matches('&').count(), 1, "{key}: {label}");
            let letter = mnemonic(label).unwrap_or_else(|| panic!("{key}: no mnemonic"));
            assert!(letter.is_alphanumeric(), "{key}: {label}");
            assert!(
                letters.insert(letter),
                "{key}: '{letter}' taken in {}",
                menu.id
            );
            assert!(!label.contains('\t'), "{key}: the key is the keymap's");
            used.insert(key);
        }
    }
    for key in catalogue.keys().filter(|key| key.starts_with("menubar.")) {
        assert!(used.contains(key), "{key} names no menu or entry");
    }
}

/// The key after the tab is the keymap's - its first chord, as the palette prints it; an entry
/// without a key has no tab.
#[test]
fn the_keys_come_from_the_keymap() {
    let keymap = Keymap::standard();
    let menus = build(&keymap, &standard());
    assert_eq!(menus.first().map(|menu| menu.title.as_str()), Some("&File"));
    assert_eq!(text_of(&menus, "copy").as_deref(), Some("&Copy...\tF5"));
    assert_eq!(text_of(&menus, "delete").as_deref(), Some("&Delete\tF8"));
    assert_eq!(
        text_of(&menus, "panel.refresh"),
        Some(format!("&Reread\t{}", crate::keymap::label_text("Ctrl+R")))
    );
    assert_eq!(
        text_of(&menus, "disconnect"),
        Some(format!(
            "&Disconnect\t{}",
            crate::keymap::label_text("Ctrl+Shift+F")
        )),
        "a window key"
    );
    assert_eq!(
        text_of(&menus, "palette"),
        Some(format!(
            "&Command palette...\t{}",
            crate::keymap::label_text("Ctrl+Shift+P")
        )),
        "the palette's own window key"
    );
    assert_eq!(text_of(&menus, "settings").as_deref(), Some("&Settings..."));
    assert_eq!(
        key_of(&keymap, "file.properties"),
        Some(crate::keymap::label_text("Alt+Enter"))
    );
    assert_eq!(key_of(&keymap, "no_such_name"), None);
}

/// Where no network drives are mapped their entries go, and the separator before them with them.
#[test]
fn rows_the_palette_lacks_are_left_out_with_their_separators() {
    let entries = entries(&Keymap::standard(), &reach::standard(), false);
    let menus = build(&Keymap::standard(), &entries);
    assert_eq!(text_of(&menus, "map_network_drive"), None);
    assert_eq!(text_of(&menus, "unmap_network_drive"), None);
    for menu in &menus {
        assert_ne!(
            menu.items.first(),
            Some(&BarItem::Separator),
            "{}",
            menu.title
        );
        assert_ne!(
            menu.items.last(),
            Some(&BarItem::Separator),
            "{}",
            menu.title
        );
        for pair in menu.items.windows(2) {
            assert!(
                pair[0] != BarItem::Separator || pair[1] != BarItem::Separator,
                "{}: two separators",
                menu.title
            );
        }
    }
}

#[test]
fn the_books_attach_once_and_then_follow_the_look() {
    let look = Look {
        shown: true,
        dark: false,
        enabled: true,
        alt_letters: true,
    };
    let mut bar = MenuBar::default();
    let hidden = Look {
        shown: false,
        ..look
    };
    assert_eq!(
        bar.step(hidden),
        Step::Nothing,
        "a hidden bar is not attached"
    );
    assert_eq!(bar.step(look), Step::Attach);
    assert_eq!(bar.step(look), Step::Nothing);
    assert_eq!(bar.step(Look { dark: true, ..look }), Step::Apply);
    assert_eq!(bar.step(hidden), Step::Apply);
    assert_eq!(bar.step(hidden), Step::Nothing);
}

/// The popup menus are told the look the first time and at each change, also with the bar hidden.
#[test]
fn the_popup_menus_follow_each_change_of_look() {
    let mut bar = MenuBar::default();
    assert!(bar.popups(false), "the first time, light too");
    assert!(!bar.popups(false));
    assert!(bar.popups(true), "the theme or the system turned dark");
    assert!(!bar.popups(true));
    assert!(bar.popups(false));
}

fn chosen(id: &str) -> Message {
    Message::Palette(PaletteMsg::MenuBar(MenuBarMsg::Chosen(id.to_owned())))
}

/// An entry runs what its palette row runs; a dialog entry opens its dialog; while a dialog is
/// open a choice does nothing.
#[tokio::test]
async fn a_choice_runs_its_row_and_waits_for_an_open_dialog() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.send(chosen("palette")).await;
    assert!(harness.app.palette.is_some(), "Help > Command palette");
    harness.press("Escape").await;
    assert!(harness.app.palette.is_none());

    harness.send(chosen("settings")).await;
    assert!(harness.app.settings.is_some());
    harness.send(chosen("database")).await;
    assert!(
        harness.app.database.is_none(),
        "the settings are open: the menus wait"
    );
    harness.press("Escape").await;
    assert!(harness.app.settings.is_none());

    harness.send(chosen("database")).await;
    assert!(harness.app.database.is_some());
}

/// Configuration > Plugins... opens the settings on the Plugins page, on its tab Installed
/// whichever tab was chosen before (RC-100-20).
#[tokio::test]
async fn plugins_opens_the_settings_on_the_installed_plugins() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    assert_eq!(
        slot("settings.plugins"),
        Some(Slot::Dialog(
            "settings.plugins",
            Dialog::Page(Page::Plugins)
        ))
    );
    harness.app.settings_tabs.choose(Tab::PluginSettings);
    harness.send(chosen("settings.plugins")).await;
    let dialog = harness.app.settings.as_ref().expect("the settings");
    assert_eq!(dialog.page(), Page::Plugins);
    assert_eq!(
        harness.app.settings_tabs.chosen(Page::Plugins),
        Some(Tab::Installed)
    );
}

#[tokio::test]
async fn the_setting_is_kept_in_the_books() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    assert_eq!(harness.app.dialog_windows.menu_bar.shown, None);
    harness
        .send(Message::Palette(PaletteMsg::MenuBar(MenuBarMsg::Shown(
            false,
        ))))
        .await;
    assert_eq!(harness.app.dialog_windows.menu_bar.shown, Some(false));
    harness.send(chosen("no.such.entry")).await;
    assert!(!harness.app.dialog_open());
}

/// The Win32 bar leaves Alt with a letter to a terminal in the active panel: the look it is
/// given says so (RC-040-07, Alt+V is Claude Code's image paste).
#[tokio::test]
async fn the_look_leaves_alt_letters_to_a_terminal() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    let main = iced::window::Id::unique();
    harness.app.dialog_windows = crate::app::dialog_windows::DialogWindows::new(main, Vec::new());
    let shown = MenuBarMsg::Shown(true);
    harness
        .send(Message::Palette(crate::app::palette::PaletteMsg::MenuBar(
            shown,
        )))
        .await;
    let letters = |harness: &mut Harness| {
        let _ = harness.app.sync_menu_bar();
        harness
            .app
            .dialog_windows
            .menu_bar
            .applied
            .map(|look| look.alt_letters)
    };
    assert_eq!(letters(&mut harness), Some(true), "the files");
    harness.press("Ctrl+[Semicolon]").await;
    assert_eq!(letters(&mut harness), Some(false), "the terminal");
}
