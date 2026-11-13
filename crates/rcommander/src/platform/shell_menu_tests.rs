//! The shell menu's limit of entries (RC-110-29): past it no shell menu is made, on any system,
//! and the window's own opens.

use std::path::PathBuf;

use super::{Anchor, MAX_NAMES, ShellRequest, show};

fn request(names: usize) -> ShellRequest {
    ShellRequest {
        folder: PathBuf::from("folder"),
        names: (0..names).map(|index| format!("{index}.txt")).collect(),
        own: Vec::new(),
        anchor: Anchor::Pointer,
    }
}

#[test]
fn more_entries_than_the_limit_get_no_shell_menu() {
    assert!(!request(0).too_many(), "the folder's own menu");
    assert!(!request(MAX_NAMES).too_many());
    assert!(request(MAX_NAMES + 1).too_many());
    assert!(show(request(MAX_NAMES + 1)).is_none());
}
