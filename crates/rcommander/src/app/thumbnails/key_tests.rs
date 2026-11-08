//! Which entries get a tile, from where, and what a tile is kept by.

use std::time::{Duration, UNIX_EPOCH};

use rc_engine::{Entry, EntryKind, VfsPath};
use rc_vfs::{Attributes, BackendId};

use super::{REMOTE_MAX_BYTES, Source, ThumbKey, source};
use crate::app::thumbnails::settings::ThumbSettings;

fn file(name: &str, size: u64, seconds: u64) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: EntryKind::File,
        size,
        modified: Some(UNIX_EPOCH + Duration::from_secs(seconds)),
        created: None,
        accessed: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn sftp() -> VfsPath {
    VfsPath {
        backend: BackendId::Sftp { connection_id: 7 },
        path: "/srv/photos".into(),
    }
}

#[test]
fn pictures_are_decoded_and_folders_and_other_files_show_their_icon() {
    let local = VfsPath::local("/photos");
    let settings = ThumbSettings::default();
    let picture = source(&file("IMG_0001.JPG", 4_000_000, 1), &local, &settings);
    assert_eq!(picture, Some(Source::Picture));
    assert_eq!(source(&file("report.pdf", 9, 1), &local, &settings), None);
    assert_eq!(source(&file("empty.png", 0, 1), &local, &settings), None);
    let mut dir = file("holiday", 0, 1);
    dir.kind = EntryKind::Dir;
    assert_eq!(source(&dir, &local, &settings), None);
}

#[test]
fn explorer_is_off_by_default_and_never_asked_while_off() {
    let local = VfsPath::local("/docs");
    let settings = ThumbSettings::default();
    assert!(!settings.explorer, "owner, 2026-11-07");
    assert_eq!(source(&file("report.pdf", 9, 1), &local, &settings), None);
    let on = ThumbSettings {
        explorer: true,
        ..settings
    };
    let expected = cfg!(windows).then_some(Source::Explorer);
    assert_eq!(source(&file("report.pdf", 9, 1), &local, &on), expected);
    assert_eq!(
        source(&file("report.pdf", 9, 1), &sftp(), &on),
        None,
        "never on a server"
    );
}

#[test]
fn a_server_gets_tiles_only_when_switched_on_and_only_small_pictures() {
    let off = ThumbSettings::default();
    assert!(!off.remote);
    assert_eq!(source(&file("a.png", 10, 1), &sftp(), &off), None);
    let on = ThumbSettings {
        remote: true,
        ..off
    };
    assert_eq!(
        source(&file("a.png", 10, 1), &sftp(), &on),
        Some(Source::Picture)
    );
    let large = file("big.png", REMOTE_MAX_BYTES + 1, 1);
    assert_eq!(source(&large, &sftp(), &on), None);
    let key = ThumbKey::of(&file("a.png", 10, 1), &sftp(), 96);
    assert!(!key.local(), "a server's tile never goes to disk");
}

#[test]
fn a_changed_file_and_another_size_are_other_tiles_with_other_names() {
    let dir = VfsPath::local("/photos");
    let before = ThumbKey::of(&file("a.jpg", 10, 1), &dir, 96);
    let touched = ThumbKey::of(&file("a.jpg", 10, 2), &dir, 96);
    let grown = ThumbKey::of(&file("a.jpg", 11, 1), &dir, 96);
    let larger = ThumbKey::of(&file("a.jpg", 10, 1), &dir, 128);
    assert_ne!(before, touched);
    let names = [&before, &touched, &grown, &larger].map(ThumbKey::disk_name);
    for (index, name) in names.iter().enumerate() {
        assert!(names[..index].iter().all(|other| other != name));
        assert_eq!(name.len(), 64 + ".png".len());
        assert!(!name.contains("photos"), "no path in clear");
    }
    assert_eq!(before.disk_name(), before.clone().disk_name());
}
