use std::path::{Path, PathBuf};

use rc_engine::{Entry, EntryKind, VfsPath};
use rc_vfs::{ArchiveFormat, Attributes, WindowsFlags};

use super::IconKey;

fn entry(name: &str, kind: EntryKind) -> Entry {
    Entry {
        name: name.to_owned(),
        kind,
        size: 0,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn local() -> VfsPath {
    VfsPath::local("/data/work")
}

fn archive() -> VfsPath {
    VfsPath::archive(VfsPath::local("/data/pack.zip"), ArchiveFormat::Zip, "/")
}

#[test]
fn ordinary_files_share_a_key_by_their_lower_case_extension() {
    let key = IconKey::of(&entry("Report.PDF", EntryKind::File), &local());
    assert_eq!(key, IconKey::Ext("pdf".into()));
    assert_eq!(
        key,
        IconKey::of(&entry("other.pdf", EntryKind::File), &local())
    );
    assert_eq!(
        IconKey::of(&entry("Makefile", EntryKind::File), &local()),
        IconKey::Ext("".into())
    );
    assert_eq!(
        IconKey::of(&entry(".bashrc", EntryKind::File), &local()),
        IconKey::Ext("".into()),
        "a leading dot is no extension"
    );
    assert!(!key.per_file());
}

#[test]
fn programs_shortcuts_and_icon_files_are_keyed_per_file() {
    for name in ["setup.exe", "Editor.LNK", "app.ico", "site.url"] {
        let key = IconKey::of(&entry(name, EntryKind::File), &local());
        assert_eq!(key, IconKey::File(Path::new("/data/work").join(name)));
        assert!(key.per_file(), "{name}");
    }
}

#[test]
fn archive_entries_get_icons_by_extension_without_file_access() {
    for name in ["setup.exe", "Editor.lnk", "notes.txt"] {
        let key = IconKey::of(&entry(name, EntryKind::File), &archive());
        assert!(!key.per_file(), "{name}: {key:?}");
    }
    assert_eq!(
        IconKey::of(&entry("setup.exe", EntryKind::File), &archive()),
        IconKey::Ext("exe".into())
    );
}

#[test]
fn plain_folders_share_one_key_and_marked_ones_have_their_own() {
    let folder = entry("src", EntryKind::Dir);
    assert_eq!(IconKey::of(&folder, &local()), IconKey::Folder);
    let mut system = entry("Fonts", EntryKind::Dir);
    system.attributes.windows = Some(WindowsFlags {
        system: true,
        ..WindowsFlags::default()
    });
    assert_eq!(
        IconKey::of(&system, &local()),
        IconKey::File(PathBuf::from("/data/work/Fonts"))
    );
    let mut readonly = entry("Pictures", EntryKind::Dir);
    readonly.attributes.readonly = true;
    assert!(IconKey::of(&readonly, &local()).per_file());
    assert_eq!(IconKey::of(&readonly, &archive()), IconKey::Folder);
}

#[test]
fn an_overlong_extension_counts_as_none() {
    let name = format!("data.{}", "x".repeat(40));
    assert_eq!(
        IconKey::of(&entry(&name, EntryKind::File), &local()),
        IconKey::Ext("".into())
    );
}
