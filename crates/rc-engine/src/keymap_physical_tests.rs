//! Keys named by where they sit (RC-100-48): `[Semicolon]` is the key right of L on every layout.

use super::*;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// A key press as the window reports it: the chord of its character (or, for a dead key, of its
/// place) and the code of the key it sat on.
fn pressed(keys: &str, code: &'static str) -> Chord {
    Chord {
        physical: Some(code),
        ..chord(keys)
    }
}

fn toggle() -> Command {
    Command::ToggleTerminal { panel: None }
}

#[test]
fn both_notations_read_and_print_alike() {
    for text in [
        "Ctrl+[Semicolon]",
        "Ctrl+Shift+[Quote]",
        "[Slash]",
        "Ctrl+\u{d6}",
    ] {
        assert_eq!(chord(text).to_string(), text);
    }
    assert_eq!(chord("Ctrl+[Semicolon]").code(), Some("Semicolon"));
    assert_eq!(chord("Ctrl+\u{f6}").code(), None, "a character is no place");
    for (code, _) in PHYSICAL_KEYS {
        assert_eq!(chord(&format!("Alt+[{code}]")).code(), Some(*code));
    }
    for wrong in [
        "Ctrl+[semicolon]",
        "Ctrl+[KeyA]",
        "Ctrl+[Backslash]",
        "Ctrl+[]",
    ] {
        assert!(wrong.parse::<Chord>().is_err(), "{wrong}");
    }
}

#[test]
fn the_code_a_press_carries_is_no_part_of_the_chord() {
    assert_eq!(pressed("Ctrl+\u{f6}", "Semicolon"), chord("Ctrl+\u{f6}"));
    assert_ne!(
        pressed("Ctrl+\u{f6}", "Semicolon"),
        chord("Ctrl+[Semicolon]")
    );
    assert!(chord("Ctrl+[Semicolon]").matches(&pressed("Ctrl+\u{f6}", "Semicolon")));
    assert!(!chord("Ctrl+[Semicolon]").matches(&pressed("Ctrl+Shift+\u{f6}", "Semicolon")));
    assert!(!chord("Ctrl+[Semicolon]").matches(&chord("Ctrl+\u{f6}")));
    assert!(!chord("Ctrl+[Quote]").matches(&pressed("Ctrl+\u{f6}", "Semicolon")));
}

/// The owner's decision (2026-10-29): the toggle, the new terminal and the maximize sit on the
/// keys right of L - Ctrl+O-umlaut on German, Ctrl+; on US, Ctrl+n-tilde on Spanish.
#[test]
fn the_terminal_keys_follow_the_key_on_every_layout() {
    let keymap = Keymap::standard();
    for character in ["\u{f6}", ";", "\u{f1}"] {
        let press = pressed(&format!("Ctrl+{character}"), "Semicolon");
        for view in [PanelView::Files, PanelView::Terminal] {
            assert_eq!(keymap.lookup(&press, view), Some(&toggle()), "{character}");
        }
        let open = pressed(&format!("Ctrl+Shift+{character}"), "Semicolon");
        assert!(
            matches!(
                keymap.lookup(&open, PanelView::Files),
                Some(Command::OpenTerminal { .. })
            ),
            "{character}"
        );
    }
    let maximize = Some(&Command::MaximizeTerminal { panel: None });
    for character in ["\u{e4}", "'"] {
        let press = pressed(&format!("Ctrl+{character}"), "Quote");
        assert_eq!(keymap.lookup(&press, PanelView::Terminal), maximize);
    }
    // The key left of 1 toggles too, by place (owner 2026-10-29): VS Code's Ctrl+` on US.
    let backquote = pressed("Ctrl+`", "Backquote");
    assert_eq!(
        keymap.lookup(&backquote, PanelView::Terminal),
        Some(&toggle())
    );
}

/// A dead key gives no character - the Spanish key right of n-tilde is the acute accent -, so
/// the window names it by place; the maximize finds it, and it reads as its accent.
#[test]
fn a_dead_key_is_found_by_its_place_and_shown_as_its_accent() {
    let keymap = Keymap::standard();
    let spanish_acute = pressed("Ctrl+[Quote]", "Quote");
    assert_eq!(
        keymap.lookup(&spanish_acute, PanelView::Files),
        Some(&Command::MaximizeTerminal { panel: None })
    );
    let shown = chord("Ctrl+[Quote]").label(|_| Some('\u{b4}'));
    assert_eq!(shown, "Ctrl+\u{b4}");
    assert_eq!(spanish_acute.stored(), chord("Ctrl+[Quote]"));
}

/// A binding on the character goes before one on the place: an older keymap in character form
/// keeps its keys.
#[test]
fn a_binding_on_the_character_goes_before_one_on_the_place() {
    let keymap = Keymap::parse(
        r#"[
            { "keys": "Ctrl+\u00f6", "command": "terminal.open" },
            { "keys": "Ctrl+[Semicolon]", "command": "panel.toggle_terminal" }
        ]"#,
    )
    .expect("reads");
    let german = pressed("Ctrl+\u{f6}", "Semicolon");
    assert!(matches!(
        keymap.lookup(&german, PanelView::Files),
        Some(Command::OpenTerminal { .. })
    ));
    let us = pressed("Ctrl+;", "Semicolon");
    assert_eq!(keymap.lookup(&us, PanelView::Files), Some(&toggle()));
}

/// The person's overrides written in character form before RC-100-48 still apply.
#[test]
fn an_override_in_character_form_keeps_working() {
    let overrides =
        Overrides::parse("Ctrl+\u{f6}=terminal.open Ctrl+[Comma]=panel.toggle_terminal")
            .expect("reads");
    let (keymap, stale) = Keymap::with_overrides(&overrides);
    assert!(stale.is_empty(), "{stale:?}");
    let german = pressed("Ctrl+\u{f6}", "Semicolon");
    assert!(matches!(
        keymap.lookup(&german, PanelView::Files),
        Some(Command::OpenTerminal { .. })
    ));
    let comma = pressed("Ctrl+,", "Comma");
    assert_eq!(keymap.lookup(&comma, PanelView::Files), Some(&toggle()));
    assert_eq!(
        overrides.to_string(),
        "Ctrl+\u{d6}=terminal.open Ctrl+[Comma]=panel.toggle_terminal"
    );
}

/// The keymap editor keeps an umlaut, a punctuation mark and a dead key by place, a letter or a
/// digit by its character - the French key right of L is M, and Ctrl+M stays Ctrl+M.
#[test]
fn the_editor_stores_a_layout_key_by_place_and_a_letter_as_it_is() {
    assert_eq!(
        pressed("Ctrl+\u{f6}", "Semicolon").stored(),
        chord("Ctrl+[Semicolon]")
    );
    assert_eq!(pressed("Alt+-", "Slash").stored(), chord("Alt+[Slash]"));
    assert_eq!(pressed("Ctrl+M", "Semicolon").stored(), chord("Ctrl+M"));
    assert_eq!(chord("Ctrl+K").stored(), chord("Ctrl+K"));
    assert_eq!(chord("F5").stored(), chord("F5"));
}

#[test]
fn a_key_named_by_place_reads_as_the_layout_prints_it() {
    let semicolon = chord("Ctrl+Shift+[Semicolon]");
    assert_eq!(
        semicolon.label(|_| None),
        "Ctrl+Shift+;",
        "US without a layout"
    );
    assert_eq!(semicolon.label(|_| Some('\u{f6}')), "Ctrl+Shift+\u{d6}");
    assert_eq!(semicolon.label(|_| Some('\u{d6}')), "Ctrl+Shift+\u{d6}");
    assert_eq!(
        chord("Ctrl+[BracketRight]").label(|_| Some('+')),
        "Ctrl+Plus"
    );
    assert_eq!(chord("Ctrl+[Quote]").label(|_| Some('\0')), "Ctrl+'");
    assert_eq!(chord("Ctrl+F5").label(|_| Some('x')), "Ctrl+F5");
    for (code, us) in PHYSICAL_KEYS {
        let shown = chord(&format!("[{code}]")).label(|_| None);
        assert_eq!(shown, us.to_uppercase(), "{code}");
    }
}

/// French AZERTY (owner 2026-10-29): the key right of L is M, and Ctrl+M stays Multi-Rename; the
/// toggle is the key left of 1, which prints the superscript two there.
#[test]
fn on_azerty_ctrl_m_stays_and_the_key_left_of_1_toggles() {
    let keymap = Keymap::standard();
    let m = pressed("Ctrl+M", "Semicolon");
    assert_eq!(
        keymap.lookup(&m, PanelView::Files),
        keymap.lookup(&chord("Ctrl+M"), PanelView::Files),
        "Ctrl+M keeps its own binding"
    );
    assert_ne!(keymap.lookup(&m, PanelView::Files), Some(&toggle()));
    let two = pressed("Ctrl+\u{b2}", "Backquote");
    assert_eq!(keymap.lookup(&two, PanelView::Files), Some(&toggle()));
    assert_eq!(keymap.lookup(&two, PanelView::Terminal), Some(&toggle()));
    let shown = chord("Ctrl+[Backquote]").label(|_| Some('\u{b2}'));
    assert_eq!(shown, "Ctrl+\u{b2}");
}

/// German: the key left of 1 is the dead circumflex, which gives no character; with Ctrl the
/// window names it by place, and it toggles (Ctrl+Shift opens a terminal).
#[test]
fn the_german_dead_circumflex_toggles_by_its_place() {
    let keymap = Keymap::standard();
    let circumflex = pressed("Ctrl+[Backquote]", "Backquote");
    assert_eq!(
        keymap.lookup(&circumflex, PanelView::Terminal),
        Some(&toggle())
    );
    let shifted = pressed("Ctrl+Shift+[Backquote]", "Backquote");
    assert!(matches!(
        keymap.lookup(&shifted, PanelView::Files),
        Some(Command::OpenTerminal { .. })
    ));
    assert_eq!(chord("Ctrl+[Backquote]").label(|_| Some('^')), "Ctrl+^");
}
