//! RC-080-05: the window's own keys, the rows' names and the conflict check.

use std::collections::{BTreeMap, HashSet};

use super::*;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// The keys the window catches itself are window rows, not stale reservations: the command
/// line's (RC-010-08), the disconnect key (RC-050-03) and the command palette. No command runs
/// for them, in either view.
#[test]
fn the_window_keys_are_window_rows() {
    let keymap = Keymap::standard();
    for (keys, name) in [
        ("Ctrl+Enter", "cmdline_insert_name"),
        ("Ctrl+Shift+Enter", "cmdline_insert_path"),
        ("Ctrl+P", "cmdline_insert_folder"),
        ("Alt+F8", "cmdline_history"),
        ("Ctrl+Shift+F", "disconnect"),
        ("Ctrl+Shift+P", "palette"),
    ] {
        assert_eq!(
            keymap.window_key(&chord(keys), PanelView::Files),
            Some(name),
            "{keys}"
        );
        assert_eq!(
            keymap.window_key(&chord(keys), PanelView::Terminal),
            None,
            "{keys} leaves a terminal"
        );
        for view in [PanelView::Files, PanelView::Terminal] {
            assert_eq!(keymap.lookup(&chord(keys), view), None, "{keys}");
        }
        assert!(
            keymap.reserved().iter().all(|row| row.chord != chord(keys)),
            "{keys} is still reserved"
        );
    }
}

/// Every name is a valid part of an i18n key, a binding without `"name"` is named by its
/// command, and a name stands for one command with one set of parameters - so Ctrl+F3 and
/// Ctrl+F4 cannot both read "Sort".
#[test]
fn every_row_has_a_name_of_its_own() {
    let keymap = Keymap::standard();
    let mut meanings: BTreeMap<&str, HashSet<String>> = BTreeMap::new();
    for binding in keymap.bindings() {
        assert!(
            is_name(&binding.name),
            "{}: {:?}",
            binding.chord,
            binding.name
        );
        let meaning = serde_json::to_string(&binding.command).expect("a command serialises");
        meanings.entry(&binding.name).or_default().insert(meaning);
    }
    for (name, meaning) in &meanings {
        assert_eq!(meaning.len(), 1, "{name} names {meaning:?}");
    }
    let raw: Vec<serde_json::Value> = serde_json::from_str(DEFAULT).expect("a list");
    for row in raw.iter().filter(|row| row.get("params").is_some()) {
        assert!(
            row.get("name").is_some(),
            "{row}: a row with params names itself"
        );
    }
    let refresh = keymap
        .bindings()
        .iter()
        .find(|binding| binding.chord == chord("Ctrl+R"))
        .expect("Ctrl+R is bound");
    assert_eq!(refresh.name, "panel.refresh");
    for key in keymap.window() {
        assert!(is_name(&key.name), "{}: {:?}", key.chord, key.name);
        assert!(
            !meanings.contains_key(key.name.as_str()),
            "{} names a binding too",
            key.name
        );
    }
}

/// The conflict check of RC-080-05: no chord is taken twice in the same context - not by two
/// bindings, nor by a binding and a window key or a reservation. The parser refuses it, and the
/// standard keymap holds every chord once.
#[test]
fn no_chord_is_taken_twice() {
    let keymap = Keymap::standard();
    let chords: Vec<&Chord> = keymap
        .bindings()
        .iter()
        .map(|binding| &binding.chord)
        .chain(keymap.reserved().iter().map(|row| &row.chord))
        .chain(keymap.window().iter().map(|key| &key.chord))
        .collect();
    let unique: HashSet<&&Chord> = chords.iter().collect();
    assert_eq!(unique.len(), chords.len());
    let raw: Vec<serde_json::Value> = serde_json::from_str(DEFAULT).expect("a list");
    assert_eq!(raw.len(), chords.len(), "every row is read");
    for twice in [
        r#"[{ "keys": "Ctrl+P", "window": "a" }, { "keys": "Ctrl+P", "command": "panel.refresh" }]"#,
        r#"[{ "keys": "Ctrl+P", "command": "panel.refresh" }, { "keys": "Ctrl+P", "window": "a" }]"#,
        r#"[{ "keys": "Ctrl+P", "window": "a" }, { "keys": "Ctrl+P", "reserved": "later" }]"#,
    ] {
        assert!(
            matches!(Keymap::parse(twice), Err(KeymapError::Duplicate { .. })),
            "{twice}"
        );
    }
}

#[test]
fn a_wrong_window_row_or_name_does_not_read() {
    for row in [
        r#"[{ "keys": "F2", "window": "a", "command": "panel.refresh" }]"#,
        r#"[{ "keys": "F2", "window": "a", "reserved": "later" }]"#,
        r#"[{ "keys": "F2", "window": "a", "params": {} }]"#,
        r#"[{ "keys": "F2", "window": "a", "name": "b" }]"#,
        r#"[{ "keys": "F2", "window": "Not A Name" }]"#,
        r#"[{ "keys": "F2", "window": "" }]"#,
        r#"[{ "keys": "F2", "reserved": "later", "name": "b" }]"#,
        r#"[{ "keys": "F2", "command": "panel.refresh", "name": "has space" }]"#,
    ] {
        assert!(
            matches!(Keymap::parse(row), Err(KeymapError::Row { .. })),
            "{row}"
        );
    }
    let named = r#"[
        { "keys": "F2", "command": "panel.refresh", "name": "reread" },
        { "keys": "F9", "window": "menu", "when": "always" }
    ]"#;
    let keymap = Keymap::parse(named).expect("reads");
    assert_eq!(keymap.bindings()[0].name, "reread");
    assert_eq!(
        keymap.window_key(&chord("F9"), PanelView::Terminal),
        Some("menu")
    );
}

/// RC-080-16: Ctrl+Shift+A, the list of the tabs (TC's key), is a window key in both views - from
/// a terminal it lists the terminal's sessions - and runs no command.
#[test]
fn the_tab_list_is_a_window_key_in_both_views() {
    let keymap = Keymap::standard();
    for view in [PanelView::Files, PanelView::Terminal] {
        assert_eq!(
            keymap.window_key(&chord("Ctrl+Shift+A"), view),
            Some("tab_list"),
            "{view:?}"
        );
        assert_eq!(keymap.lookup(&chord("Ctrl+Shift+A"), view), None);
    }
}
