use std::collections::BTreeSet;

use rc_engine::Keymap;

use super::*;
use crate::app::harness::Harness;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// The rows where network drives are mapped (Windows).
fn standard() -> Vec<Entry> {
    entries(&Keymap::standard(), &reach::standard(), true)
}

fn named<'a>(entries: &'a [Entry], name: &str) -> &'a Entry {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("no entry {name}"))
}

/// One row per name with all its chords, the window keys but the palette's own, the commands
/// the palette runs, and the two dialogs without a key.
#[test]
fn the_palette_lists_every_way_in() {
    let entries = standard();
    let refresh = named(&entries, "panel.refresh");
    assert_eq!(
        refresh.keys,
        format!("{}, F2", crate::keymap::label_text("Ctrl+R"))
    );
    assert_eq!(refresh.action, Action::Press(chord("Ctrl+R")));
    assert_eq!(
        named(&entries, "sort_name").keys,
        crate::keymap::label_text("Ctrl+F3")
    );
    assert_eq!(
        named(&entries, "cmdline_insert_folder").action,
        Action::Press(chord("Ctrl+P"))
    );
    assert!(entries.iter().all(|entry| entry.name != OPEN_KEY));
    assert_eq!(
        named(&entries, "vault.lock").action,
        Action::Run(Box::new(Command::LockVault))
    );
    assert_eq!(
        named(&entries, "settings").action,
        Action::Open(Surface::Settings)
    );
    assert_eq!(
        named(&entries, "database").action,
        Action::Open(Surface::Database)
    );
    // RC-010-16: the network drive dialogs, which have no key.
    assert_eq!(
        named(&entries, "map_network_drive").action,
        Action::Open(Surface::Network(DriveAction::MapNetwork))
    );
    assert_eq!(
        named(&entries, "unmap_network_drive").action,
        Action::Open(Surface::Network(DriveAction::UnmapNetwork))
    );
    let elsewhere = super::entries(&Keymap::standard(), &reach::standard(), false);
    assert!(
        elsewhere
            .iter()
            .all(|entry| !entry.name.ends_with("network_drive")),
        "no network drive rows where none are mapped"
    );
    let names: BTreeSet<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names.len(), entries.len(), "a name is one row");
    let keymap = Keymap::standard();
    for binding in keymap.bindings() {
        assert!(names.contains(binding.name.as_str()), "{}", binding.name);
    }
}

/// The names are the catalogue's `keys.` keys, both ways: every row has its words, and every
/// `keys.` key names a row (the repo lint leaves the prefix to this test).
#[test]
fn every_name_is_in_the_catalogue_and_back() {
    let catalogue: std::collections::HashMap<String, String> =
        serde_json::from_str(include_str!("../../locales/en.json")).expect("en.json reads");
    let entries = standard();
    for entry in &entries {
        let key = format!("keys.{}", entry.name);
        assert!(catalogue.contains_key(&key), "{key} is missing");
        assert_eq!(entry.label, catalogue[&key]);
    }
    let mut names: BTreeSet<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    names.insert(OPEN_KEY.to_owned());
    for key in catalogue.keys() {
        if let Some(name) = key.strip_prefix("keys.") {
            assert!(
                names.contains(name),
                "{key} names no row of the keymap or palette"
            );
        }
    }
}

#[test]
fn typing_narrows_and_the_arrows_choose() {
    let mut palette = Palette::new(standard());
    let all = palette.shown.len();
    palette.set_query("SORT size".into());
    assert_eq!(palette.shown.len(), 1);
    assert_eq!(
        key(&mut palette, &chord("Enter")),
        PaletteKey::Run(Action::Press(chord("Ctrl+F6")))
    );
    palette.set_query("ctrl+f3".into());
    let first = palette.entry(palette.shown[0]).expect("an entry");
    assert_eq!(first.name, "sort_name");
    palette.set_query(String::new());
    assert_eq!(palette.shown.len(), all);
    assert_eq!(key(&mut palette, &chord("ArrowDown")), PaletteKey::Stay);
    assert_eq!(palette.selected, 1);
    key(&mut palette, &chord("ArrowUp"));
    key(&mut palette, &chord("ArrowUp"));
    assert_eq!(palette.selected, 0, "stays on the first row");
    key(&mut palette, &chord("PageDown"));
    assert_eq!(palette.selected, PAGE);
    for _ in 0..all {
        key(&mut palette, &chord("PageDown"));
    }
    assert_eq!(palette.selected, all - 1, "stays on the last row");
    palette.set_query("no such command anywhere".into());
    assert!(palette.shown.is_empty());
    assert_eq!(key(&mut palette, &chord("Enter")), PaletteKey::Stay);
    assert_eq!(key(&mut palette, &chord("Escape")), PaletteKey::Close);
}

/// Ctrl+Shift+P opens the palette, which then has the keys; a row runs as its key would.
#[tokio::test]
async fn ctrl_shift_p_opens_the_palette_and_a_row_runs_its_key() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.press("Ctrl+Shift+P").await;
    assert!(harness.app.palette.is_some());
    // Ctrl+T goes to the palette, not to the tabs.
    harness.press("Ctrl+T").await;
    assert_eq!(harness.app.tabs.strips[0].len(), 1);
    harness
        .send(Message::Palette(PaletteMsg::Query("ctrl+t".into())))
        .await;
    harness.press("Enter").await;
    assert!(harness.app.palette.is_none());
    assert_eq!(harness.app.tabs.strips[0].len(), 2, "the row ran Ctrl+T");

    harness.press("Ctrl+Shift+P").await;
    harness.press("Escape").await;
    assert!(harness.app.palette.is_none());
    assert_eq!(harness.app.tabs.strips[0].len(), 2);
}

#[tokio::test]
async fn the_palette_opens_the_settings() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.press("Ctrl+Shift+P").await;
    let palette = harness.app.palette.as_ref().expect("open");
    let at = palette
        .shown
        .iter()
        .position(|index| {
            palette
                .entry(*index)
                .is_some_and(|entry| entry.name == "settings")
        })
        .expect("a settings row");
    harness.send(Message::Palette(PaletteMsg::Pick(at))).await;
    assert!(harness.app.palette.is_none());
    assert!(harness.app.settings.is_some());
}
