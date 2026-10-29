use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use rc_vfs::{EntryKind, TargetKind};
use russh_sftp::protocol::{FileAttributes, FileMode};

use super::*;

fn attrs(mode: FileMode, permissions: u32, size: u64) -> FileAttributes {
    FileAttributes {
        size: Some(size),
        permissions: Some(mode.bits() | permissions),
        mtime: Some(1_700_000_000),
        atime: Some(1_700_000_000),
        ..FileAttributes::default()
    }
}

#[test]
fn hostile_names_are_not_entries_of_the_directory() {
    for name in [
        "",
        ".",
        "..",
        "a/b",
        "../../etc",
        "a\\b",
        "nul\0",
        "cr\r",
        "lf\n",
        "bell\u{7}",
        "C:x",
        "c:",
    ] {
        assert!(!safe_name(name), "{name:?}");
    }
    for name in [".hidden", "a b", "Gr\u{fc}\u{df}e", "...", "x..", "ab:c"] {
        assert!(safe_name(name), "{name:?}");
    }
}

#[test]
fn a_file_carries_size_time_mode_and_the_dot_file_is_hidden() {
    let file = entry(".profile".to_owned(), &attrs(FileMode::REG, 0o644, 42));
    assert_eq!(file.kind, EntryKind::File);
    assert_eq!(file.size, 42);
    assert_eq!(
        file.modified,
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
    assert_eq!(file.attributes.mode, Some(0o644));
    assert!(!file.attributes.readonly);
    assert!(file.hidden);

    let dir = entry("srv".to_owned(), &attrs(FileMode::DIR, 0o555, 4096));
    assert_eq!(dir.kind, EntryKind::Dir);
    assert_eq!(dir.size, 0, "a directory has size 0");
    assert!(dir.attributes.readonly);
    assert!(!dir.hidden);
}

#[test]
fn attributes_without_type_bits_list_a_file() {
    let bare = FileAttributes {
        size: Some(7),
        ..FileAttributes::default()
    };
    let file = entry("x".to_owned(), &bare);
    assert_eq!(file.kind, EntryKind::File);
    assert_eq!(file.size, 7);
    assert_eq!(file.attributes.mode, None);
}

#[test]
fn a_link_shows_its_target_kind_size_and_time() {
    let link = entry("current".to_owned(), &attrs(FileMode::LNK, 0o777, 7));
    assert!(matches!(link.kind, EntryKind::Symlink { .. }));
    assert_eq!(link.size, 0);

    let to_dir = link_entry(
        link.clone(),
        PathBuf::from("releases/42"),
        Some(&attrs(FileMode::DIR, 0o755, 4096)),
    );
    assert_eq!(
        to_dir.kind,
        EntryKind::Symlink {
            target: PathBuf::from("releases/42"),
            target_kind: Some(TargetKind::Dir),
        }
    );
    assert!(to_dir.is_dir_like(), "a link to a directory can be entered");

    let to_file = link_entry(
        link.clone(),
        PathBuf::from("/etc/motd"),
        Some(&attrs(FileMode::REG, 0o644, 99)),
    );
    assert_eq!(to_file.size, 99);

    let dangling = link_entry(link, PathBuf::from("gone"), None);
    assert_eq!(
        dangling.kind,
        EntryKind::Symlink {
            target: PathBuf::from("gone"),
            target_kind: None,
        }
    );
    assert!(!dangling.is_dir_like());
}
