//! What Unix keeps and refuses: modes, owners, no creation time, no Windows flags.

use std::{os::unix::fs::MetadataExt, time::SystemTime};

use super::super::write;
use crate::{EntryKind, LinkKind, LocalFs, PropertyChange, Vfs, VfsError, WriteMode};

#[tokio::test]
async fn a_symlink_keeps_its_target_as_given() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    write(&fs, &dir.path().join("file"), WriteMode::CreateNew, b"abc").await;
    let link = dir.path().join("link");
    fs.create_link(&link, std::path::Path::new("file"), LinkKind::Symlink)
        .await
        .expect("symlink");
    let entry = fs.stat(&link).await.expect("stat");
    assert!(
        matches!(&entry.kind, EntryKind::Symlink { target, .. } if target.as_os_str() == "file"),
        "{entry:?}"
    );
    assert_eq!(entry.size, 3, "a link shows its target's size");
}

#[tokio::test]
async fn junctions_and_shortcuts_are_windows_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    fs.mkdir(&dir.path().join("d"), false).await.expect("mkdir");
    assert!(matches!(
        fs.create_link(
            &dir.path().join("j"),
            &dir.path().join("d"),
            LinkKind::Junction
        )
        .await,
        Err(VfsError::Unsupported { .. })
    ));
    assert!(matches!(
        fs.create_link(
            &dir.path().join("s.lnk"),
            &dir.path().join("d"),
            LinkKind::Shortcut
        )
        .await,
        Err(VfsError::Unsupported { .. })
    ));
}

#[tokio::test]
async fn a_mode_is_set_and_windows_things_are_refused_first() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("script");
    write(&fs, &file, WriteMode::CreateNew, b"#!").await;
    let mode = PropertyChange {
        mode: Some(0o750),
        ..PropertyChange::default()
    };
    fs.set_properties(&file, mode).await.expect("chmod");
    assert_eq!(
        fs.stat(&file).await.expect("stat").attributes.mode,
        Some(0o750)
    );

    for refused in [
        PropertyChange {
            mode: Some(0o600),
            hidden: Some(true),
            ..PropertyChange::default()
        },
        PropertyChange {
            mode: Some(0o600),
            created: Some(SystemTime::now()),
            ..PropertyChange::default()
        },
    ] {
        assert!(matches!(
            fs.set_properties(&file, refused).await,
            Err(VfsError::Unsupported { .. })
        ));
    }
    assert_eq!(
        fs.stat(&file).await.expect("stat").attributes.mode,
        Some(0o750),
        "nothing changed before the refusal"
    );
}

#[tokio::test]
async fn details_name_owner_blocks_and_links() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file");
    write(&fs, &file, WriteMode::CreateNew, &[7; 5000]).await;
    let details = fs.details(&file).await.expect("details");
    let meta = std::fs::metadata(&file).expect("metadata");
    assert_eq!(details.uid, Some(meta.uid()));
    assert_eq!(details.gid, Some(meta.gid()));
    assert_eq!(details.links, Some(1));
    assert_eq!(details.allocated, Some(meta.blocks() * 512));
    assert!(details.accessed.is_some());
}
