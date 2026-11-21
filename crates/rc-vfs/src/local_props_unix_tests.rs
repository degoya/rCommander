//! What Unix keeps and refuses: modes, owners, no creation time, no Windows flags.

use std::{
    os::unix::fs::MetadataExt,
    time::{Duration, SystemTime},
};

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

/// A link's own modification time, not its target's; the link's access time stays (RC-110-45).
#[tokio::test]
async fn a_symlinks_own_time_is_set_without_following_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file");
    write(&fs, &file, WriteMode::CreateNew, b"abc").await;
    let link = dir.path().join("link");
    fs.create_link(&link, std::path::Path::new("file"), LinkKind::Symlink)
        .await
        .expect("symlink");
    let accessed = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    std::fs::set_times_nofollow(&link, std::fs::FileTimes::new().set_accessed(accessed))
        .expect("set the link's access time");
    let target_before = std::fs::metadata(&file)
        .expect("metadata")
        .modified()
        .expect("mtime");
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let change = PropertyChange {
        modified: Some(then),
        ..PropertyChange::default()
    };
    fs.set_properties(&link, change)
        .await
        .expect("set_properties");
    let own = std::fs::symlink_metadata(&link).expect("symlink_metadata");
    assert_eq!(own.modified().expect("mtime"), then);
    assert_eq!(own.accessed().expect("atime"), accessed);
    let target = std::fs::metadata(&file).expect("metadata");
    assert_eq!(
        target.modified().expect("mtime"),
        target_before,
        "the target is untouched"
    );
}
