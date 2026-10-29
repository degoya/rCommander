use rc_engine::{Entry, EntryKind, PanelId, VfsPath};
use rc_vfs::Attributes;

use super::{DirSummary, Quick, QuickShown, wanted};

fn entry(name: &str, kind: EntryKind, size: u64) -> Entry {
    Entry {
        name: name.to_owned(),
        kind,
        size,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

#[test]
fn a_directory_counts_its_files_and_folders_and_adds_the_files_bytes() {
    let entries = [
        entry("a.txt", EntryKind::File, 10),
        entry("b.bin", EntryKind::File, 32),
        entry("sub", EntryKind::Dir, 0),
    ];
    assert_eq!(
        DirSummary::of(&entries),
        DirSummary {
            files: 2,
            dirs: 1,
            bytes: 42
        }
    );
}

#[test]
fn the_other_panel_shows_what_the_cursor_is_on() {
    let quick = Quick::new(PanelId::Left);
    assert_eq!(quick.target(), PanelId::Right);
    assert!(matches!(quick.shown, QuickShown::Nothing));
    assert_eq!(quick.path(), None);
    let dir = VfsPath::local("/data");
    let file = entry("photo.jpg", EntryKind::File, 1);
    assert_eq!(
        wanted(&dir, Some(&file)),
        Some((VfsPath::local("/data/photo.jpg"), false))
    );
    let sub = entry("sub", EntryKind::Dir, 0);
    assert_eq!(
        wanted(&dir, Some(&sub)),
        Some((VfsPath::local("/data/sub"), true))
    );
    assert_eq!(wanted(&dir, None), None);
}
