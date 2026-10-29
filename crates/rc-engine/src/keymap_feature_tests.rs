//! The keys of the later features: the function keys, Quick View, the archives, the connection
//! manager, the context menu, the checksum dialogs.

use super::*;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// RC-010-07: every function key of the file operations runs a registry command; the dialogs
/// are `ops.dialog`, F3 and F4 the external programs. A terminal keeps these keys.
#[test]
fn the_function_keys_run_the_file_operations() {
    use crate::{ExternalProgram, FileDialog};
    let keymap = Keymap::standard();
    let dialog = |dialog| {
        Some(Command::FileDialog {
            panel: None,
            dialog,
        })
    };
    let external = |program| {
        Some(Command::OpenExternal {
            panel: None,
            path: None,
            program,
            create: false,
        })
    };
    for (keys, wanted) in [
        ("F3", external(ExternalProgram::Viewer)),
        ("F4", external(ExternalProgram::Editor)),
        ("Shift+F4", dialog(FileDialog::NewText)),
        ("F5", dialog(FileDialog::Copy)),
        ("Shift+F5", dialog(FileDialog::CopyHere)),
        ("F6", dialog(FileDialog::Move)),
        ("Shift+F6", dialog(FileDialog::Rename)),
        ("F7", dialog(FileDialog::Mkdir)),
        ("F8", dialog(FileDialog::Delete)),
        ("Delete", dialog(FileDialog::Delete)),
        ("Shift+F8", dialog(FileDialog::DeletePermanent)),
        ("Shift+Delete", dialog(FileDialog::DeletePermanent)),
    ] {
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Files).cloned(),
            wanted,
            "{keys}"
        );
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Terminal),
            None,
            "{keys}"
        );
        assert!(keymap.reserved().iter().all(|row| row.chord != chord(keys)));
    }
}

/// RC-020-07: Ctrl+Q switches Quick View - in the file list only, as in Total Commander. (F3
/// reaches the Lister through `file.open_external` while no external viewer is set.)
#[test]
fn ctrl_q_switches_quick_view() {
    let keymap = Keymap::standard();
    assert_eq!(
        keymap.lookup(&chord("Ctrl+Q"), PanelView::Files),
        Some(&Command::ToggleQuickView { panel: None })
    );
    assert_eq!(keymap.lookup(&chord("Ctrl+Q"), PanelView::Terminal), None);
}

/// Alt+F5, Alt+Shift+F5, Alt+F6 and Alt+F9 pack and unpack (RC-030-03, RC-030-04), without a
/// target: the window asks in its dialog first.
#[test]
fn the_archive_keys_pack_and_unpack() {
    let keymap = Keymap::standard();
    let files = |keys: &str| keymap.lookup(&chord(keys), PanelView::Files).cloned();
    for keys in ["Alt+F6", "Alt+F9"] {
        assert!(
            matches!(
                files(keys),
                Some(Command::ArchiveExtract { target: None, .. })
            ),
            "{keys}"
        );
    }
    let Some(Command::ArchivePack {
        target: None,
        settings,
        ..
    }) = files("Alt+Shift+F5")
    else {
        panic!("Alt+Shift+F5 packs");
    };
    assert!(settings.delete_sources);
    assert!(matches!(
        files("Alt+F5"),
        Some(Command::ArchivePack { target: None, settings, .. }) if !settings.delete_sources
    ));
}

/// RC-050-04, as Total Commander: Ctrl+F opens the connection manager; Ctrl+Shift+F closes the
/// active panel's connection, which the window does itself (`app/connections/session.rs`): a
/// window row, no binding. A terminal keeps Ctrl+F for its shell.
#[test]
fn the_connection_manager_keys_collide_with_no_standard_key() {
    let keymap = Keymap::standard();
    let open = chord("Ctrl+F");
    let bound: Vec<&Binding> = keymap
        .bindings()
        .iter()
        .filter(|binding| binding.chord == open)
        .collect();
    assert_eq!(bound.len(), 1, "{bound:?}");
    assert_eq!(bound[0].command, Command::ConnectionManager {});
    assert_eq!(keymap.lookup(&open, PanelView::Terminal), None);
    assert!(keymap.reserved().iter().all(|row| row.chord != open));

    // Ctrl+Shift+F is the window's (`connections/session.rs`): one window row, no binding.
    let disconnect = chord("Ctrl+Shift+F");
    assert_eq!(
        keymap.window_key(&disconnect, PanelView::Files),
        Some("disconnect")
    );
    assert!(keymap.bindings().iter().all(|row| row.chord != disconnect));
    assert!(keymap.reserved().iter().all(|row| row.chord != disconnect));
    assert_eq!(
        keymap
            .window()
            .iter()
            .filter(|row| row.chord == disconnect)
            .count(),
        1
    );
}

/// The command line's keys (Ctrl+Enter, Ctrl+Shift+Enter, Ctrl+P, Alt+F8) are the window's
/// (`app/cmdline/keys.rs`): window rows of their own, no command runs on them.
#[test]
fn the_command_line_keys_are_the_windows() {
    let keymap = Keymap::standard();
    for (keys, name) in [
        ("Ctrl+Enter", "cmdline_insert_name"),
        ("Ctrl+Shift+Enter", "cmdline_insert_path"),
        ("Ctrl+P", "cmdline_insert_folder"),
        ("Alt+F8", "cmdline_history"),
    ] {
        let at = chord(keys);
        assert_eq!(
            keymap.window_key(&at, PanelView::Files),
            Some(name),
            "{keys}"
        );
        assert_eq!(keymap.lookup(&at, PanelView::Files), None, "{keys}");
        assert!(
            keymap.reserved().iter().all(|row| row.chord != at),
            "{keys}"
        );
    }
}

/// RC-080-02: Shift+F10 and the context-menu key open the context menu, as in Explorer and
/// Total Commander; in a terminal both stay the shell's.
#[test]
fn shift_f10_and_the_menu_key_open_the_context_menu() {
    let keymap = Keymap::standard();
    let wanted = Some(&Command::ContextMenu { panel: None });
    for keys in ["Shift+F10", "ContextMenu"] {
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Files),
            wanted,
            "{keys}"
        );
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Terminal),
            None,
            "{keys}"
        );
    }
}

/// RC-080-17: F10 quits from the file list; in a terminal it stays the shell's, as every
/// F key. Closing a dialog with it is the window's (`app/quit.rs`), not a binding.
#[test]
fn f10_quits_from_the_files_and_stays_the_shells_in_a_terminal() {
    let keymap = Keymap::standard();
    assert_eq!(
        keymap.lookup(&chord("F10"), PanelView::Files),
        Some(&Command::Quit {})
    );
    assert_eq!(keymap.lookup(&chord("F10"), PanelView::Terminal), None);
}

/// RC-020-05: Total Commander has no key for its checksum, split and combine commands (only its
/// Files menu); rCommander has them in its Files menu too (RC-080-18), and Ctrl+Shift+F9 to F12 -
/// free in both - open the dialogs as well; in a terminal they stay the shell's.
#[test]
fn ctrl_shift_f9_to_f12_open_the_checksum_dialogs() {
    let keymap = Keymap::standard();
    let dialogs = [
        ("Ctrl+Shift+F9", crate::ChecksumDialog::Create),
        ("Ctrl+Shift+F10", crate::ChecksumDialog::Verify),
        ("Ctrl+Shift+F11", crate::ChecksumDialog::Split),
        ("Ctrl+Shift+F12", crate::ChecksumDialog::Combine),
    ];
    for (keys, dialog) in dialogs {
        let wanted = Command::ChecksumDialog {
            panel: None,
            dialog,
        };
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Files),
            Some(&wanted),
            "{keys}"
        );
        assert_eq!(
            keymap
                .bindings()
                .iter()
                .filter(|binding| binding.chord == chord(keys))
                .count(),
            1,
            "{keys} is bound once"
        );
        assert_eq!(
            keymap.lookup(&chord(keys), PanelView::Terminal),
            None,
            "{keys}"
        );
    }
}

/// RC-010-14: Total Commander has no default key for its background transfer manager (F2 in the
/// copy dialog queues, the manager opens from there). Ctrl+Shift+Q - Q for queue, beside Ctrl+Q's
/// Quick View - is TC's separate Quick View window, which rCommander does not plan
/// (`keymap/tc.json`); a terminal keeps it for its shell.
#[test]
fn ctrl_shift_q_opens_the_queue_window() {
    let keymap = Keymap::standard();
    let keys = chord("Ctrl+Shift+Q");
    let bound: Vec<&Binding> = keymap
        .bindings()
        .iter()
        .filter(|binding| binding.chord == keys)
        .collect();
    assert_eq!(bound.len(), 1, "{bound:?}");
    assert_eq!(bound[0].command, Command::QueueWindow {});
    assert_eq!(keymap.lookup(&keys, PanelView::Terminal), None);
}
