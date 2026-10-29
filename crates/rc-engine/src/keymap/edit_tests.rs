use std::collections::BTreeSet;

use super::*;
use crate::PanelView;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// What `keys` runs in the file list.
fn runs<'a>(keymap: &'a Keymap, keys: &str) -> Option<&'a Command> {
    keymap.lookup(&chord(keys), PanelView::Files)
}

/// The standard's command of `keys`.
fn standard(keys: &str) -> Command {
    runs(Keymap::shared_standard(), keys)
        .cloned()
        .expect("a standard key")
}

/// The bindings as (chord, name), whatever their order.
fn pairs(keymap: &Keymap) -> BTreeSet<(String, String)> {
    keymap
        .bindings()
        .iter()
        .map(|binding| (binding.chord.to_string(), binding.name.clone()))
        .collect()
}

/// What the setting keeps, read back: the same keymap.
fn read_back(keymap: &Keymap) -> Keymap {
    let text = keymap.overrides().to_string();
    let (read, stale) = Keymap::with_overrides(&Overrides::parse(&text).expect("reads"));
    assert!(stale.is_empty(), "{text}: {stale:?}");
    read
}

#[test]
fn a_second_key_is_added() {
    let mut keymap = Keymap::standard();
    keymap.assign(chord("Ctrl+K"), "copy").expect("free");
    assert_eq!(runs(&keymap, "Ctrl+K"), Some(&standard("F5")));
    assert_eq!(runs(&keymap, "F5"), Some(&standard("F5")), "F5 stays");
    assert_eq!(keymap.overrides().to_string(), "Ctrl+K=copy");
    assert_eq!(keymap.key_of("copy"), Some(&chord("F5")), "the first key");
    assert_eq!(pairs(&read_back(&keymap)), pairs(&keymap));
}

#[test]
fn a_key_is_removed() {
    let mut keymap = Keymap::standard();
    keymap.unassign(&chord("F5")).expect("not locked");
    assert_eq!(runs(&keymap, "F5"), None);
    assert_eq!(keymap.overrides().to_string(), "F5=");
    let rows = keymap.rows();
    let copy: Vec<&KeymapRow> = rows.iter().filter(|row| row.name == "copy").collect();
    assert_eq!(copy.len(), 1, "{copy:?}");
    assert!(copy[0].keys.is_empty() && copy[0].changed, "{copy:?}");
    assert_eq!(copy[0].command.as_ref(), Some(&standard("F5")));
    assert_eq!(pairs(&read_back(&keymap)), pairs(&keymap));
}

/// The acceptance's example: F5 to Ctrl+K - the copy runs on Ctrl+K only, the menus show it.
#[test]
fn a_key_is_replaced() {
    let mut keymap = Keymap::standard();
    keymap.unassign(&chord("F5")).expect("not locked");
    keymap.assign(chord("Ctrl+K"), "copy").expect("free");
    assert_eq!(runs(&keymap, "F5"), None);
    assert_eq!(runs(&keymap, "Ctrl+K"), Some(&standard("F5")));
    assert_eq!(keymap.key_of("copy"), Some(&chord("Ctrl+K")));
    assert_eq!(keymap.moved_key(&chord("F5")), Some(&chord("Ctrl+K")));
    assert_eq!(keymap.moved_key(&chord("F6")), Some(&chord("F6")));
    let text = keymap.overrides().to_string();
    assert_eq!(text, "F5= Ctrl+K=copy");
    assert_eq!(pairs(&read_back(&keymap)), pairs(&keymap));
}

/// A key another row holds names that row - the question "replace?" - and taking it moves it.
#[test]
fn a_taken_key_names_its_holder_and_moves() {
    let mut keymap = Keymap::standard();
    assert_eq!(keymap.holder(&chord("F6")), Some("move"));
    assert_eq!(keymap.holder(&chord("Ctrl+Shift+P")), Some("palette"));
    assert_eq!(keymap.holder(&chord("Ctrl+J")), None);
    keymap.assign(chord("F6"), "copy").expect("free of locks");
    assert_eq!(runs(&keymap, "F6"), Some(&standard("F5")));
    assert_eq!(keymap.key_of("move"), None, "move lost its only key");
    assert!(
        keymap
            .without_key()
            .iter()
            .any(|binding| binding.name == "move"),
        "the palette still offers it"
    );
    let rows = keymap.rows();
    let moved: Vec<&KeymapRow> = rows.iter().filter(|row| row.name == "move").collect();
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert!(moved[0].keys.is_empty() && moved[0].changed, "{moved:?}");
    assert_eq!(pairs(&read_back(&keymap)), pairs(&keymap));
}

#[test]
fn the_windows_keys_are_locked_with_their_reason() {
    let mut keymap = Keymap::standard();
    for (keys, lock) in [
        ("Tab", Lock::Focus),
        ("Shift+Tab", Lock::Focus),
        ("Escape", Lock::Escape),
        ("Shift+Escape", Lock::Escape),
        ("Alt+F", Lock::AccessKey),
        ("Alt+Shift+2", Lock::AccessKey),
        ("Ctrl+C", Lock::Window),
        ("Ctrl+Shift+P", Lock::Window),
    ] {
        assert_eq!(Keymap::locked(&chord(keys)), Some(lock), "{keys}");
        assert_eq!(
            keymap.assign(chord(keys), "copy"),
            Err(Refusal::Locked(lock)),
            "{keys}"
        );
        assert_eq!(
            keymap.unassign(&chord(keys)),
            Err(Refusal::Locked(lock)),
            "{keys}"
        );
    }
    for keys in ["Ctrl+Tab", "Ctrl+Alt+F", "Alt+F7", "Alt+Enter", "F5"] {
        assert_eq!(Keymap::locked(&chord(keys)), None, "{keys}");
    }
    assert_eq!(keymap, Keymap::standard(), "nothing changed");
    let rows = keymap.rows();
    let tab = rows.iter().find(|row| row.keys == "Tab").expect("Tab");
    assert_eq!(tab.locked, Some(Lock::Focus));
    let paste = rows
        .iter()
        .find(|row| row.keys == "Ctrl+V")
        .expect("Ctrl+V");
    assert_eq!(
        (paste.locked, paste.command.is_none()),
        (Some(Lock::Window), true)
    );
    assert_eq!(
        keymap.assign(chord("Ctrl+J"), "no_such_row"),
        Err(Refusal::UnknownName)
    );
}

#[test]
fn a_reservation_may_be_taken() {
    let mut keymap = Keymap::parse(
        r#"[{ "keys": "Ctrl+J", "reserved": "a later feature (RC-0)" },
            { "keys": "F5", "command": "ops.dialog", "name": "copy", "params": { "dialog": "copy" } }]"#,
    )
    .expect("reads");
    assert_eq!(keymap.holder(&chord("Ctrl+J")), None, "no question");
    keymap.assign(chord("Ctrl+J"), "copy").expect("free");
    assert!(keymap.reserved().is_empty());
    assert_eq!(runs(&keymap, "Ctrl+J"), Some(&standard("F5")));
}

#[test]
fn a_row_and_all_rows_go_back_to_the_standard() {
    let mut keymap = Keymap::standard();
    keymap.unassign(&chord("F5")).expect("not locked");
    keymap.assign(chord("Ctrl+K"), "copy").expect("free");
    keymap.assign(chord("F6"), "copy").expect("free");
    keymap.reset("move");
    assert_eq!(
        runs(&keymap, "F6"),
        Some(&standard("F6")),
        "move has F6 back"
    );
    assert_eq!(
        keymap.key_of("copy"),
        Some(&chord("Ctrl+K")),
        "copy keeps its own"
    );
    keymap.reset("copy");
    assert_eq!(runs(&keymap, "F5"), Some(&standard("F5")));
    assert_eq!(runs(&keymap, "Ctrl+K"), None);
    assert_eq!(pairs(&keymap), pairs(&Keymap::standard()));
    assert!(keymap.overrides().is_empty());
    // All of them: the standard with no deviations.
    let (all, stale) = Keymap::with_overrides(&Overrides::default());
    assert_eq!((all, stale), (Keymap::standard(), Vec::new()));
}

/// A deviation a newer standard no longer fits - a row it renamed or dropped, a key it no longer
/// binds - is left out; the others still apply, and the next save drops it.
#[test]
fn a_deviation_for_a_row_no_longer_there_is_left_out() {
    let overrides = Overrides::parse("Ctrl+K=gone_in_this_version F5= Ctrl+J= Ctrl+Shift+J=copy")
        .expect("reads");
    let (keymap, stale) = Keymap::with_overrides(&overrides);
    assert_eq!(
        stale,
        [
            Override::Bind {
                chord: chord("Ctrl+K"),
                name: "gone_in_this_version".to_owned()
            },
            Override::Unbind {
                chord: chord("Ctrl+J")
            },
        ]
    );
    assert_eq!(runs(&keymap, "Ctrl+K"), None);
    assert_eq!(runs(&keymap, "F5"), None);
    assert_eq!(runs(&keymap, "Ctrl+Shift+J"), Some(&standard("F5")));
    assert_eq!(keymap.overrides().to_string(), "F5= Ctrl+Shift+J=copy");
}

#[test]
fn the_standard_rows_are_unchanged_and_name_every_row() {
    let keymap = Keymap::standard();
    let rows = keymap.rows();
    assert!(rows.iter().all(|row| !row.changed), "{rows:?}");
    assert_eq!(
        rows.len(),
        keymap.bindings().len() + keymap.window().len(),
        "one row per key"
    );
    let delete: Vec<&str> = rows
        .iter()
        .filter(|row| row.name == "delete")
        .map(|row| row.keys.as_str())
        .collect();
    assert_eq!(delete, ["F8", "Delete"], "a name's keys together");
}
