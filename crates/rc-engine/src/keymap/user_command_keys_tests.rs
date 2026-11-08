use super::*;
use crate::{Keymap, Overrides, PanelView, Refusal};

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

#[test]
fn a_start_key_name_reads_back() {
    assert_eq!(start_key_name(7), "start:7");
    assert_eq!(start_key("start:7"), Some(7));
    for name in [
        "copy",
        "start:",
        "start:0",
        "start:07",
        "start:-1",
        "start:1a",
        "start: 1",
        "start:1234567890123456789",
        "workspace:7",
    ] {
        assert_eq!(start_key(name), None, "{name}");
    }
}

#[test]
fn an_own_command_key_is_its_name() {
    assert_eq!(own_command_key("em_putty"), Some("em_putty"));
    assert_eq!(own_command_key("em_a_1"), Some("em_a_1"));
    for name in ["em_", "em_Putty", "em-putty", "cm_copy", "copy", "em_a.b"] {
        assert_eq!(own_command_key(name), None, "{name}");
    }
    assert_eq!(own_command_key(&format!("em_{}", "x".repeat(41))), None);
}

/// Total Commander's `Ctrl+Alt+E=em_putty` and a Start menu entry's key are kept in
/// `keys.overrides` like any other deviation and run their command on the key - also for a
/// command deleted since.
#[test]
fn own_command_and_start_keys_go_through_the_overrides() {
    let text = "Ctrl+Alt+E=em_putty Ctrl+Alt+2=start:12";
    let overrides = Overrides::parse(text).expect("reads");
    assert_eq!(overrides.to_string(), text);
    let (keymap, stale) = Keymap::with_overrides(&overrides);
    assert!(stale.is_empty(), "{stale:?}");
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Alt+E"), PanelView::Files),
        Some(&Command::RunUserCommand {
            name: "em_putty".into(),
            panel: None,
            params: None,
        })
    );
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Alt+2"), PanelView::Files),
        Some(&Command::RunStartItem {
            id: 12,
            panel: None,
            params: None,
        })
    );
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Alt+E"), PanelView::Terminal),
        None,
        "a terminal keeps its keys"
    );
    assert_eq!(keymap.overrides(), overrides);
    for wrong in [
        "Ctrl+Alt+1=start:x",
        "Ctrl+Alt+1=start:0",
        "Ctrl+Alt+1=em_P",
    ] {
        assert!(Overrides::parse(wrong).is_err(), "{wrong}");
    }
}

#[test]
fn own_command_and_start_keys_are_edited_like_any_row() {
    let mut keymap = Keymap::standard();
    keymap
        .assign(chord("Ctrl+Alt+N"), "em_np")
        .expect("a free key");
    keymap
        .assign(chord("Ctrl+Alt+5"), &start_key_name(5))
        .expect("a free key");
    for name in ["em_np", "start:5"] {
        assert!(
            keymap
                .rows()
                .iter()
                .any(|row| row.name == name && row.changed),
            "{name}"
        );
    }
    assert_eq!(
        keymap.assign(chord("Ctrl+Alt+6"), "start:none"),
        Err(Refusal::UnknownName)
    );
    assert_eq!(
        keymap.assign(chord("Ctrl+Alt+6"), "em_"),
        Err(Refusal::UnknownName)
    );
    keymap.reset("em_np");
    assert!(keymap.rows().iter().all(|row| row.name != "em_np"));
}
