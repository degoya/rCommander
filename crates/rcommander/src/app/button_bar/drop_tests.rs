use std::path::PathBuf;

use rc_engine::{ButtonAction, CommandKind};

use super::{button_for, local_paths};

#[test]
fn only_absolute_paths_go_to_a_button() {
    let absolute = std::env::temp_dir().join("a file.txt");
    let paths = vec![absolute.clone(), PathBuf::from("relative.txt")];
    assert_eq!(
        local_paths(&paths),
        vec![absolute.to_string_lossy().into_owned()]
    );
}

#[test]
fn a_dropped_file_makes_a_program_button_named_after_it() {
    let path = std::env::temp_dir().join("notepad.exe");
    let made = button_for(&path.to_string_lossy());
    let ButtonAction::Command(action) = &made.action else {
        panic!("a command");
    };
    assert_eq!(action.kind, CommandKind::Program);
    assert_eq!(action.command, path.to_string_lossy());
    assert_eq!(made.tooltip, "notepad");
    assert!(made.icon.is_none(), "the program's own symbol shows");
}
