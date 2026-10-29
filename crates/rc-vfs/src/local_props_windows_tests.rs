//! What only Windows has: the attribute flags, the creation time, junctions and shortcuts.

use std::time::{Duration, SystemTime};

use super::super::write;
use crate::{EntryKind, LinkKind, LocalFs, PropertyChange, TargetKind, Vfs, VfsError, WriteMode};

#[tokio::test]
async fn the_flags_and_the_creation_time_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file.txt");
    write(&fs, &file, WriteMode::CreateNew, b"a").await;
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    let change = PropertyChange {
        hidden: Some(true),
        system: Some(true),
        archive: Some(false),
        created: Some(then),
        ..PropertyChange::default()
    };
    fs.set_properties(&file, change).await.expect("set");
    let entry = fs.stat(&file).await.expect("stat");
    let flags = entry.attributes.windows.expect("flags");
    assert!(flags.hidden && flags.system && !flags.archive, "{flags:?}");
    assert_eq!(entry.created, Some(then));
    let clear = PropertyChange {
        hidden: Some(false),
        system: Some(false),
        ..PropertyChange::default()
    };
    fs.set_properties(&file, clear).await.expect("clear");
    let flags = fs.stat(&file).await.expect("stat").attributes.windows;
    assert!(flags.is_some_and(|flags| !flags.hidden && !flags.system));
    let mode = PropertyChange {
        mode: Some(0o644),
        ..PropertyChange::default()
    };
    assert!(matches!(
        fs.set_properties(&file, mode).await,
        Err(VfsError::Unsupported { .. })
    ));
}

#[tokio::test]
async fn a_junction_leads_to_its_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let target = dir.path().join("target");
    fs.mkdir(&target, false).await.expect("mkdir");
    write(&fs, &target.join("inside.txt"), WriteMode::CreateNew, b"a").await;
    let junction = dir.path().join("junction");
    fs.create_link(&junction, &target, LinkKind::Junction)
        .await
        .expect("junction");
    let entry = fs.stat(&junction).await.expect("stat");
    assert!(
        matches!(
            entry.kind,
            EntryKind::Symlink {
                target_kind: Some(TargetKind::Dir),
                ..
            }
        ),
        "{entry:?}"
    );
    let inside = fs.list(&junction).await.expect("list through");
    assert_eq!(inside.len(), 1);
    let file = dir.path().join("file");
    write(&fs, &file, WriteMode::CreateNew, b"a").await;
    assert!(matches!(
        fs.create_link(&dir.path().join("j2"), &file, LinkKind::Junction)
            .await,
        Err(VfsError::NotADirectory { .. })
    ));
}

#[tokio::test]
async fn a_shortcut_is_a_shell_link_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file.txt");
    write(&fs, &file, WriteMode::CreateNew, b"a").await;
    let shortcut = dir.path().join("file.txt.lnk");
    fs.create_link(&shortcut, &file, LinkKind::Shortcut)
        .await
        .expect("shortcut");
    let bytes = std::fs::read(&shortcut).expect("read");
    // ShellLinkHeader: HeaderSize 0x4C, then the CLSID 00021401-0000-0000-C000-000000000046.
    assert_eq!(&bytes[..4], &[0x4C, 0, 0, 0]);
    assert_eq!(&bytes[4..8], &[0x01, 0x14, 0x02, 0x00]);
}

#[tokio::test]
async fn a_symlink_is_made_or_explained() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file");
    write(&fs, &file, WriteMode::CreateNew, b"a").await;
    // Without Developer Mode or administrator rights Windows refuses, and the error says why.
    match fs
        .create_link(&dir.path().join("link"), &file, LinkKind::Symlink)
        .await
    {
        Ok(()) => assert!(matches!(
            fs.stat(&dir.path().join("link")).await.expect("stat").kind,
            EntryKind::Symlink { .. }
        )),
        Err(error) => assert_eq!(error.code(), "symlink_privilege", "{error}"),
    }
}
