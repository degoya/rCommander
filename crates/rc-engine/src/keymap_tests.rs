use super::*;
use crate::{COMMANDS, CursorStep, ListMode, PanelId, SortKey, spec};

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

#[test]
fn the_standard_keymap_reads() {
    assert!(
        Keymap::parse(DEFAULT).is_ok(),
        "{:?}",
        Keymap::parse(DEFAULT)
    );
    assert!(!Keymap::standard().bindings().is_empty());
}

/// The acceptance check of RC-010-01: every standard key names a command of the registry. The
/// raw file is read as well, so a binding cannot slip through by some serde default.
#[test]
fn every_standard_key_points_to_a_registered_command() {
    for binding in Keymap::standard().bindings() {
        assert!(
            spec(binding.command.id()).is_some(),
            "{}: {} is not registered",
            binding.chord,
            binding.command.id()
        );
    }
    let raw: Vec<serde_json::Value> = serde_json::from_str(DEFAULT).expect("a list");
    // A reservation keeps a key free and a window key is the window's: neither names a command.
    let commands: Vec<&serde_json::Value> = raw
        .iter()
        .filter(|row| row.get("command").is_some())
        .collect();
    for row in &commands {
        let id = row["command"].as_str().expect("a command id");
        assert!(
            COMMANDS.iter().any(|command| command.id == id),
            "{row}: {id} is not registered"
        );
    }
    assert_eq!(commands.len(), Keymap::standard().bindings().len());
}

/// A reserved chord - a Total Commander key of a later job - runs nothing yet and says what it
/// is kept for.
#[test]
fn later_features_keep_their_total_commander_keys() {
    let keymap = Keymap::standard();
    for row in keymap.reserved() {
        for view in [PanelView::Files, PanelView::Terminal] {
            assert_eq!(
                keymap.lookup(&row.chord, view),
                None,
                "{} runs something",
                row.chord
            );
        }
        assert!(
            row.purpose.len() >= 4,
            "{}: say what it is kept for",
            row.chord
        );
    }
}

#[test]
fn the_total_commander_keys_run_their_commands() {
    let keymap = Keymap::standard();
    let files = |keys: &str| keymap.lookup(&chord(keys), PanelView::Files).cloned();
    assert_eq!(
        files("Tab"),
        Some(Command::SwitchActivePanel { panel: None })
    );
    assert_eq!(
        files("Enter"),
        Some(Command::OpenFile {
            panel: None,
            path: None
        })
    );
    assert_eq!(
        files("Ctrl+PageDown"),
        Some(Command::OpenEntry { panel: None })
    );
    assert_eq!(
        files("Backspace"),
        Some(Command::Navigate {
            panel: None,
            path: "..".into()
        })
    );
    assert_eq!(
        files("PageDown"),
        Some(Command::MoveCursor {
            panel: None,
            step: CursorStep::PageDown
        })
    );
    assert_eq!(
        files("Insert"),
        Some(Command::ToggleCursorSelection { panel: None })
    );
    let to = |path: &str| {
        Some(Command::Navigate {
            panel: None,
            path: path.into(),
        })
    };
    assert_eq!(files("Ctrl+PageUp"), to(".."));
    assert_eq!(files("Ctrl+\\"), to("/"));
    assert_eq!(files("Ctrl+R"), Some(Command::Refresh { panel: None }));
    assert_eq!(files("Ctrl+A"), Some(Command::SelectAll { panel: None }));
    assert_eq!(
        files("Ctrl+NumAdd"),
        Some(Command::SelectAll { panel: None })
    );
    assert_eq!(
        files("Ctrl+NumSubtract"),
        Some(Command::DeselectAll { panel: None })
    );
    assert_eq!(
        files("NumMultiply"),
        Some(Command::InvertSelection { panel: None })
    );
    assert_eq!(
        files("Alt+F1"),
        Some(Command::DriveMenu {
            panel: Some(PanelId::Left)
        })
    );
    assert_eq!(
        files("Alt+F2"),
        Some(Command::DriveMenu {
            panel: Some(PanelId::Right)
        })
    );
}

/// The terminal toggle (owner 2026-10-17, as VS Code): Ctrl+O-umlaut on a German layout,
/// Ctrl+` on a US one - the same key there (`VK_OEM_3`), two characters to the keymap.
const TOGGLES: [&str; 2] = ["Ctrl+\u{f6}", "Ctrl+`"];
const NEW_TERMINAL: [&str; 2] = ["Ctrl+Shift+\u{f6}", "Ctrl+Shift+`"];
/// The terminal over the whole window (RC-040-05): Ctrl+A-umlaut on a German layout, Ctrl+' on
/// a US one - the same key there (`VK_OEM_7`), right beside the toggle on both.
const MAXIMIZE: [&str; 2] = ["Ctrl+\u{e4}", "Ctrl+'"];
/// The clipboard's image pasted into the terminal as a path (RC-040-08).
const PASTE_IMAGE: [&str; 1] = ["Ctrl+Alt+V"];

#[test]
fn a_terminal_gets_every_key_but_the_toggle_and_the_maximize() {
    let keymap = Keymap::standard();
    let toggle = Some(&Command::ToggleTerminal { panel: None });
    for keys in TOGGLES {
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Files),
            toggle,
            "{keys}"
        );
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Terminal),
            toggle,
            "{keys}"
        );
    }
    let always: Vec<String> = keymap
        .bindings()
        .iter()
        .filter(|binding| binding.when == KeyContext::Always)
        .map(|binding| binding.chord.to_string())
        .collect();
    let toggles: Vec<String> = TOGGLES
        .iter()
        .chain(&MAXIMIZE)
        .chain(&PASTE_IMAGE)
        .map(|keys| chord(keys).to_string())
        .collect();
    assert_eq!(
        always, toggles,
        "only the toggle, the maximize and the image paste leave a terminal"
    );
    for binding in keymap.bindings() {
        if binding.when == KeyContext::Files {
            assert_eq!(
                keymap.lookup(&binding.chord, PanelView::Terminal),
                None,
                "{} reaches the keymap from a terminal",
                binding.chord
            );
        }
    }
}

/// Ctrl+Shift+O-umlaut (Ctrl+Shift+` on a US layout) opens another terminal from the file list
/// (the window asks for the shell); in a terminal the shell gets it, as every key but the
/// toggle.
#[test]
fn ctrl_shift_oe_opens_a_terminal_from_the_files() {
    let keymap = Keymap::standard();
    let open = Command::OpenTerminal {
        panel: None,
        shell: None,
        distro: None,
        cwd: None,
        size: None,
    };
    for keys in NEW_TERMINAL {
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Files),
            Some(&open),
            "{keys}"
        );
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Terminal),
            None,
            "{keys}"
        );
    }
}

/// Ctrl+A-umlaut (Ctrl+' on a US layout) reaches `terminal.maximize` from the files and from a
/// terminal alike: the shell never gets it.
#[test]
fn ctrl_ae_maximizes_from_the_files_and_from_a_terminal() {
    let keymap = Keymap::standard();
    let maximize = Some(&Command::MaximizeTerminal { panel: None });
    for keys in MAXIMIZE {
        for view in [PanelView::Files, PanelView::Terminal] {
            assert_eq!(keymap.lookup(&chord(keys), view), maximize, "{keys}");
        }
        let maximized = chord(keys);
        let bound = keymap
            .bindings()
            .iter()
            .filter(|binding| binding.chord == maximized)
            .count();
        assert_eq!(bound, 1, "{keys} is bound once");
        assert!(
            keymap.reserved().iter().all(|row| row.chord != maximized),
            "{keys} is reserved for something else"
        );
    }
}

/// The toggle collides with no other standard key: no other binding and no reservation uses
/// either of its chords.
#[test]
fn the_toggle_chord_collides_with_no_standard_key() {
    let keymap = Keymap::standard();
    for keys in TOGGLES {
        let toggle = chord(keys);
        let bound: Vec<&Binding> = keymap
            .bindings()
            .iter()
            .filter(|binding| binding.chord == toggle)
            .collect();
        assert_eq!(bound.len(), 1, "{keys}: {bound:?}");
        assert_eq!(bound[0].command, Command::ToggleTerminal { panel: None });
        assert!(
            keymap.reserved().iter().all(|row| row.chord != toggle),
            "{keys} is reserved for something else"
        );
    }
    let parsed = Keymap::parse(DEFAULT).expect("reads");
    assert_eq!(parsed, keymap, "the parser refuses a chord bound twice");
}

#[test]
fn chords_read_and_print_alike() {
    let ctrl_shift_f = chord("ctrl+shift+F");
    assert_eq!(
        ctrl_shift_f,
        Chord {
            key: "f".into(),
            ctrl: true,
            shift: true,
            alt: false
        }
    );
    assert_eq!(ctrl_shift_f.to_string(), "Ctrl+Shift+F");
    for text in [
        "Tab",
        "Ctrl+`",
        "Ctrl+\u{d6}",
        "Alt+F7",
        "Ctrl+Shift+PageUp",
        "Shift+A",
    ] {
        assert_eq!(chord(text).to_string(), text);
    }
    assert_eq!(Chord::plain("Enter").map(|c| c.key), Some("Enter".into()));
    assert_eq!(Chord::plain("Q").map(|c| c.key), Some("q".into()));
}

#[test]
fn a_chord_is_plain_only_without_a_modifier() {
    assert!(chord("Enter").is_plain());
    for text in ["Ctrl+Enter", "Shift+Enter", "Alt+Enter", "Ctrl+Shift+Enter"] {
        assert!(!chord(text).is_plain(), "{text} plain");
    }
}

#[test]
fn a_wrong_chord_or_command_does_not_read() {
    for text in ["", "Ctrl+", "Hyper+A", "Enterr", "Ctrl+NoSuchKey"] {
        assert!(text.parse::<Chord>().is_err(), "{text:?} read");
    }
    let unknown = r#"[{ "keys": "F2", "command": "panel.no_such_command" }]"#;
    assert!(matches!(
        Keymap::parse(unknown),
        Err(KeymapError::Command { .. })
    ));
    let bad_params =
        r#"[{ "keys": "F2", "command": "panel.move_cursor", "params": { "step": "sideways" } }]"#;
    assert!(matches!(
        Keymap::parse(bad_params),
        Err(KeymapError::Command { .. })
    ));
    let twice = r#"[
        { "keys": "F2", "command": "panel.open_entry" },
        { "keys": "F2", "command": "panel.switch_active", "when": "always" }
    ]"#;
    assert!(matches!(
        Keymap::parse(twice),
        Err(KeymapError::Duplicate { .. })
    ));
    assert!(matches!(
        Keymap::parse(r#"{ "keys": "F2" }"#),
        Err(KeymapError::Json(_))
    ));
    for row in [
        r#"[{ "keys": "F2" }]"#,
        r#"[{ "keys": "F2", "command": "panel.open_entry", "reserved": "both" }]"#,
        r#"[{ "keys": "F2", "reserved": "with params", "params": {} }]"#,
    ] {
        assert!(
            matches!(Keymap::parse(row), Err(KeymapError::Row { .. })),
            "{row}"
        );
    }
    let taken = r#"[
        { "keys": "F2", "reserved": "later" },
        { "keys": "F2", "command": "panel.open_entry" }
    ]"#;
    assert!(matches!(
        Keymap::parse(taken),
        Err(KeymapError::Duplicate { .. })
    ));
}

/// RC-010-02: the file list's Total Commander keys; Ctrl+H stays free, as in Total Commander.
#[test]
fn the_file_list_keys_run_their_commands() {
    let keymap = Keymap::standard();
    let files = |keys: &str| keymap.lookup(&chord(keys), PanelView::Files).cloned();
    assert_eq!(
        files("NumAdd"),
        Some(Command::SelectByMask(crate::MaskRequest {
            panel: None,
            mask: None
        }))
    );
    assert_eq!(
        files("NumSubtract"),
        Some(Command::DeselectByMask(crate::MaskRequest {
            panel: None,
            mask: None
        }))
    );
    assert_eq!(
        files("Alt+NumAdd"),
        Some(Command::SelectExtension {
            panel: None,
            select: true
        })
    );
    assert_eq!(
        files("NumDivide"),
        Some(Command::RestoreSelection { panel: None })
    );
    assert_eq!(
        files("Shift+End"),
        Some(Command::SelectRange {
            panel: None,
            step: CursorStep::Last
        })
    );
    assert_eq!(
        files("Ctrl+F1"),
        Some(Command::SetListMode {
            panel: None,
            mode: ListMode::Brief
        })
    );
    let sort = |key| {
        Some(Command::Sort {
            panel: None,
            key,
            descending: None,
        })
    };
    assert_eq!(files("Ctrl+F3"), sort(SortKey::Name));
    assert_eq!(files("Ctrl+F4"), sort(SortKey::Ext));
    assert_eq!(files("Ctrl+F5"), sort(SortKey::Date));
    assert_eq!(files("Ctrl+F6"), sort(SortKey::Size));
    assert_eq!(files("F2"), Some(Command::Refresh { panel: None }));
    assert_eq!(
        files("ArrowRight"),
        Some(Command::MoveCursor {
            panel: None,
            step: CursorStep::Right
        })
    );
    assert_eq!(files("Ctrl+H"), None);
    assert!(
        !keymap
            .reserved()
            .iter()
            .any(|reserved| reserved.chord == chord("Ctrl+H"))
    );
}
