use std::path::PathBuf;

use rc_vfs::{ArchiveFormat, BackendId, VfsPath};

use super::*;

fn drive(id: &str, group: DriveGroup, label: &str, root: Option<&str>) -> Drive {
    Drive {
        id: id.to_owned(),
        group,
        kind: DriveKind::Fixed,
        label: label.to_owned(),
        root: root.map(VfsPath::local),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: None,
    }
}

fn roots() -> Vec<(String, PathBuf)> {
    vec![
        ("/".to_owned(), PathBuf::from("/")),
        ("home".to_owned(), PathBuf::from("/home/alex")),
        ("/mnt/c".to_owned(), PathBuf::from("/mnt/c")),
    ]
}

#[test]
fn a_path_lies_on_the_drive_of_the_longest_root_above_it() {
    let key = |path: &str| drive_key(&VfsPath::local(path), &roots());
    assert_eq!(key("/mnt/c/Users").as_deref(), Some("/mnt/c"));
    assert_eq!(key("/mnt/c").as_deref(), Some("/mnt/c"));
    assert_eq!(key("/home/alex/src").as_deref(), Some("home"));
    assert_eq!(key("/home/other").as_deref(), Some("/"));
    // Whole components only: `/mnt/cd` is not on `/mnt/c`.
    assert_eq!(key("/mnt/cd").as_deref(), Some("/"));
    assert_eq!(drive_key(&VfsPath::local("relative"), &roots()), None);
}

#[test]
fn a_path_inside_an_archive_lies_on_the_archive_s_drive() {
    let outer = VfsPath::local("/mnt/c/backup.zip");
    let inside = VfsPath::archive(outer, ArchiveFormat::Zip, "/docs");
    assert_eq!(drive_key(&inside, &roots()).as_deref(), Some("/mnt/c"));
}

#[test]
fn a_path_over_sftp_lies_on_its_server() {
    let remote = VfsPath {
        backend: BackendId::Sftp { connection_id: 7 },
        path: PathBuf::from("/home/tester"),
    };
    assert_eq!(drive_key(&remote, &roots()).as_deref(), Some("server:7"));
}

#[cfg(windows)]
#[test]
fn a_windows_path_lies_on_its_letter_or_share() {
    let key = |path: &str| drive_key(&VfsPath::local(path), &[]);
    assert_eq!(key(r"c:\Users\alex").as_deref(), Some("C:"));
    assert_eq!(key(r"D:\").as_deref(), Some("D:"));
    assert_eq!(key(r"\\?\C:\long").as_deref(), Some("C:"));
    assert_eq!(
        key(r"\\WSL.localhost\Ubuntu\home").as_deref(),
        Some(r"\\wsl.localhost\ubuntu")
    );
    // A root below a letter is a drive of its own; beside it the letter is the drive.
    let below = [("A:".to_owned(), PathBuf::from(r"C:\drives\a"))];
    let key = |path: &str| drive_key(&VfsPath::local(path), &below);
    assert_eq!(key(r"C:\drives\a\sub").as_deref(), Some("A:"));
    assert_eq!(key(r"C:\drives\b").as_deref(), Some("C:"));
}

#[test]
fn a_drive_is_found_by_id_by_letter_however_written_or_by_label() {
    let drives = [
        drive("C:", DriveGroup::Drives, "C:", Some("/c")),
        drive("/mnt/data", DriveGroup::Drives, "data", Some("/mnt/data")),
        drive(r"\\wsl.localhost\ubuntu", DriveGroup::Wsl, "Ubuntu", None),
        drive("server:3", DriveGroup::Servers, "web", None),
    ];
    let found = |wanted: &str| find(&drives, wanted).map(|drive| drive.id.as_str());
    for letter in ["C:", "c", "c:", r"C:\", "c:/", " C: "] {
        assert_eq!(found(letter), Some("C:"), "{letter:?}");
    }
    assert_eq!(found("/mnt/data"), Some("/mnt/data"));
    assert_eq!(found("ubuntu"), Some(r"\\wsl.localhost\ubuntu"));
    assert_eq!(found("server:3"), Some("server:3"));
    assert_eq!(found("Web"), Some("server:3"));
    assert_eq!(found("Z:"), None);
    assert_eq!(found(""), None);
}

#[test]
fn drive_of_names_the_entry_a_panel_is_on() {
    let drives = [
        drive("/", DriveGroup::Drives, "/", Some("/")),
        drive("/mnt/c", DriveGroup::Drives, "c", Some("/mnt/c")),
        drive("server:1", DriveGroup::Servers, "web", None),
    ];
    let on = drive_of(&drives, &VfsPath::local("/mnt/c/Windows")).map(|drive| &drive.id);
    assert_eq!(on.map(String::as_str), Some("/mnt/c"));
    assert_eq!(
        roots_of(&drives).len(),
        2,
        "a server has no root to lie under"
    );
}

#[test]
fn a_drive_serializes_for_mcp_with_snake_case_names() {
    let mut entry = drive("C:", DriveGroup::Drives, "C:", Some("/c"));
    entry.kind = DriveKind::Optical;
    entry.state = DriveState::NotReady;
    let json = serde_json::to_value(&entry).expect("serializes");
    assert_eq!(json["group"], "drives");
    assert_eq!(json["kind"], "optical");
    assert_eq!(json["state"], "not_ready");
}
