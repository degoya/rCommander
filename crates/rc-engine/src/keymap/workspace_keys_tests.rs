use super::*;
use crate::{Keymap, Overrides, PanelView, Refusal};

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

#[test]
fn a_workspace_key_name_reads_back() {
    assert_eq!(workspace_key_name(7), "workspace:7");
    assert_eq!(workspace_key("workspace:7"), Some(7));
    assert_eq!(
        workspace_key("workspace:123456789012345678"),
        Some(123_456_789_012_345_678)
    );
    for name in [
        "copy",
        "workspace:",
        "workspace:0",
        "workspace:07",
        "workspace:-1",
        "workspace:+1",
        "workspace:1a",
        "workspace: 1",
        "workspace:1234567890123456789",
        "theme:dark:nord-dark",
    ] {
        assert_eq!(workspace_key(name), None, "{name}");
    }
}

/// A workspace key is kept in `keys.overrides` like any other deviation and runs
/// `workspaces.apply` on the key - also for a workspace deleted since.
#[test]
fn a_workspace_key_goes_through_the_overrides() {
    let text = "Ctrl+Alt+1=workspace:3 Ctrl+Shift+1=theme:dark:nord-dark";
    let overrides = Overrides::parse(text).expect("reads");
    assert_eq!(overrides.to_string(), text);
    let (keymap, stale) = Keymap::with_overrides(&overrides);
    assert!(stale.is_empty(), "{stale:?}");
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Alt+1"), PanelView::Files),
        Some(&Command::ApplyWorkspace { id: 3 })
    );
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Alt+1"), PanelView::Terminal),
        None,
        "a terminal keeps its keys"
    );
    assert_eq!(keymap.overrides(), overrides);
    for wrong in ["Ctrl+Alt+1=workspace:x", "Ctrl+Alt+1=workspace:0"] {
        assert!(Overrides::parse(wrong).is_err(), "{wrong}");
    }
}

#[test]
fn a_workspace_key_is_edited_like_any_row() {
    let name = workspace_key_name(5);
    let mut keymap = Keymap::standard();
    keymap
        .assign(chord("Ctrl+Alt+5"), &name)
        .expect("a free key");
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Alt+5"), PanelView::Files),
        Some(&Command::ApplyWorkspace { id: 5 })
    );
    assert!(
        keymap
            .rows()
            .iter()
            .any(|row| row.name == name && row.changed)
    );
    assert_eq!(
        keymap.assign(chord("Ctrl+Alt+6"), "workspace:none"),
        Err(Refusal::UnknownName)
    );
    keymap.reset(&name);
    assert!(keymap.rows().iter().all(|row| row.name != name));
}

/// Ctrl+Shift+W opens the workspaces' window (RC-100-68); Total Commander closes all tabs there.
#[test]
fn ctrl_shift_w_opens_the_workspaces_window() {
    assert_eq!(
        Keymap::standard().lookup(&chord("Ctrl+Shift+W"), PanelView::Files),
        Some(&Command::WorkspaceManager {})
    );
}
