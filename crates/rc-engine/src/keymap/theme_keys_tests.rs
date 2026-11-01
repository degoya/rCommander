use super::*;
use crate::{Keymap, Override, Overrides, PanelView, Refusal};

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

fn select(mode: ThemeMode, id: &str) -> Command {
    Command::SelectTheme {
        mode,
        id: id.to_owned(),
    }
}

#[test]
fn a_theme_key_name_reads_back() {
    assert_eq!(
        theme_key_name(ThemeMode::Dark, "nord-dark"),
        "theme:dark:nord-dark"
    );
    assert_eq!(
        theme_key("theme:light:classic-light"),
        Some((ThemeMode::Light, "classic-light"))
    );
    assert_eq!(
        theme_key("theme:dark:own-0a1b2c3d"),
        Some((ThemeMode::Dark, "own-0a1b2c3d"))
    );
    let long = format!("theme:dark:{}", "a".repeat(MAX_THEME_KEY_ID + 1));
    for name in [
        "copy",
        "theme:",
        "theme:dark",
        "theme:dark:",
        "theme:dusk:nord-dark",
        "theme:dark:Nord",
        "theme:dark:nord dark",
        "theme:dark:nord:dark",
        "theme:dark:nord=dark",
        long.as_str(),
    ] {
        assert_eq!(theme_key(name), None, "{name}");
    }
}

/// A theme key is kept in `keys.overrides` like any other deviation: it reads, prints the same,
/// and the keymap built from it runs `theme.select` on the key - also for a theme deleted since.
#[test]
fn a_theme_key_goes_through_the_overrides() {
    let text = "F5= Ctrl+Shift+1=theme:dark:nord-dark Ctrl+Shift+2=theme:light:own-0a1b2c3d";
    let overrides = Overrides::parse(text).expect("reads");
    assert_eq!(overrides.to_string(), text);
    let (keymap, stale) = Keymap::with_overrides(&overrides);
    assert!(stale.is_empty(), "{stale:?}");
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Shift+1"), PanelView::Files),
        Some(&select(ThemeMode::Dark, "nord-dark"))
    );
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Shift+2"), PanelView::Files),
        Some(&select(ThemeMode::Light, "own-0a1b2c3d"))
    );
    // A terminal keeps its keys.
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Shift+1"), PanelView::Terminal),
        None
    );
    assert_eq!(keymap.overrides(), overrides);
    for wrong in [
        "Ctrl+Shift+1=theme:dusk:nord-dark",
        "Ctrl+Shift+1=theme:dark:Nord",
        "Tab=theme:dark:nord-dark",
    ] {
        assert!(Overrides::parse(wrong).is_err(), "{wrong}");
    }
}

/// The Shortcuts tab's edits work on a theme key as on any row: given a key, another key,
/// a key moved to it from another row, taken away - and with its last key the row goes.
#[test]
fn a_theme_key_is_edited_like_any_row() {
    let name = theme_key_name(ThemeMode::Light, "classic-light");
    let mut keymap = Keymap::standard();
    keymap
        .assign(chord("Ctrl+Shift+1"), &name)
        .expect("a free key");
    keymap.assign(chord("F2"), &name).expect("a taken key");
    assert_eq!(
        keymap.lookup(&chord("F2"), PanelView::Files),
        Some(&select(ThemeMode::Light, "classic-light"))
    );
    let rows: Vec<_> = keymap
        .rows()
        .into_iter()
        .filter(|row| row.name == name)
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.changed && row.locked.is_none()));
    assert_eq!(
        rows[0].command,
        Some(select(ThemeMode::Light, "classic-light"))
    );
    assert_eq!(
        keymap.assign(chord("Tab"), &name),
        Err(Refusal::Locked(crate::Lock::Focus))
    );
    assert_eq!(
        keymap.assign(chord("Ctrl+Shift+3"), "theme:dusk:x"),
        Err(Refusal::UnknownName)
    );
    keymap.unassign(&chord("F2")).expect("unassigned");
    assert!(keymap.overrides().rows().iter().any(|row| *row
        == Override::Bind {
            chord: chord("Ctrl+Shift+1"),
            name: name.clone()
        }));
    keymap.reset(&name);
    assert!(keymap.rows().iter().all(|row| row.name != name));
    assert!(
        keymap
            .without_key()
            .iter()
            .all(|binding| binding.name != name),
        "the palette offers a theme by its list, not as a key without one"
    );
}
