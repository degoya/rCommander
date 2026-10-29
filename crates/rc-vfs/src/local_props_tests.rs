//! Links, details and property changes on every platform (RC-020-06).

use std::time::{Duration, SystemTime};

use super::write;
use crate::{LinkKind, LocalFs, MemoryFs, PropertyChange, Vfs, VfsError, WriteMode};

#[cfg(unix)]
#[path = "local_props_unix_tests.rs"]
mod unix;

#[cfg(windows)]
#[path = "local_props_windows_tests.rs"]
mod windows;

#[tokio::test]
async fn a_hard_link_is_a_second_name_counted_as_one_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("data.bin");
    write(&fs, &file, WriteMode::CreateNew, b"twelve bytes").await;
    let link = dir.path().join("again.bin");
    fs.create_link(&link, &file, LinkKind::Hardlink)
        .await
        .expect("hard link");

    let listed = fs.list_details(dir.path()).await.expect("list");
    assert_eq!(listed.len(), 2);
    let ids: Vec<_> = listed.iter().map(|(_, details)| details.id).collect();
    assert!(ids[0].is_some(), "{listed:?}");
    assert_eq!(ids[0], ids[1], "one file under two names");
    assert!(listed.iter().all(|(_, details)| details.links == Some(2)));
}

#[tokio::test]
async fn a_lone_file_has_no_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("alone.txt");
    write(&fs, &file, WriteMode::CreateNew, b"x").await;
    let details = fs.details(&file).await.expect("details");
    assert_eq!(details.id, None);
}

#[tokio::test]
async fn links_never_replace_and_hard_links_take_no_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file");
    let taken = dir.path().join("taken");
    write(&fs, &file, WriteMode::CreateNew, b"a").await;
    write(&fs, &taken, WriteMode::CreateNew, b"b").await;
    for kind in [LinkKind::Hardlink, LinkKind::Symlink] {
        assert!(matches!(
            fs.create_link(&taken, &file, kind).await,
            Err(VfsError::AlreadyExists { .. })
        ));
    }
    fs.mkdir(&dir.path().join("sub"), false)
        .await
        .expect("mkdir");
    assert!(matches!(
        fs.create_link(
            &dir.path().join("h"),
            &dir.path().join("sub"),
            LinkKind::Hardlink
        )
        .await,
        Err(VfsError::IsADirectory { .. })
    ));
    assert!(matches!(
        fs.create_link(
            &dir.path().join("h"),
            &dir.path().join("none"),
            LinkKind::Hardlink
        )
        .await,
        Err(VfsError::NotFound { .. })
    ));
}

#[tokio::test]
async fn a_shortcut_is_named_lnk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file");
    write(&fs, &file, WriteMode::CreateNew, b"a").await;
    assert!(matches!(
        fs.create_link(
            &dir.path().join("file - shortcut"),
            &file,
            LinkKind::Shortcut
        )
        .await,
        Err(VfsError::InvalidPath { .. })
    ));
}

#[tokio::test]
async fn the_modification_time_is_set_with_the_attributes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("file");
    write(&fs, &file, WriteMode::CreateNew, b"a").await;
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let change = PropertyChange {
        readonly: Some(true),
        modified: Some(then),
        ..PropertyChange::default()
    };
    fs.set_properties(&file, change).await.expect("set");
    let entry = fs.stat(&file).await.expect("stat");
    assert_eq!(entry.modified, Some(then));
    assert!(entry.attributes.readonly);
    let writable = PropertyChange {
        readonly: Some(false),
        ..PropertyChange::default()
    };
    fs.set_properties(&file, writable).await.expect("writable");
    assert!(!fs.stat(&file).await.expect("stat").attributes.readonly);
}

#[tokio::test]
async fn a_backend_without_links_says_so_and_refuses() {
    let fs = MemoryFs::new();
    let capabilities = fs.property_capabilities();
    assert!(!capabilities.links.any());
    assert!(!capabilities.windows_flags && !capabilities.set_created);
    fs.mkdir(std::path::Path::new("/d"), false)
        .await
        .expect("mkdir");
    for kind in LinkKind::ALL {
        assert!(matches!(
            fs.create_link(std::path::Path::new("/l"), std::path::Path::new("/d"), kind)
                .await,
            Err(VfsError::Unsupported { .. })
        ));
    }
    let hidden = PropertyChange {
        hidden: Some(true),
        ..PropertyChange::default()
    };
    assert!(matches!(
        fs.set_properties(std::path::Path::new("/d"), hidden).await,
        Err(VfsError::Unsupported { .. })
    ));
}

#[tokio::test]
async fn local_names_its_link_kinds() {
    let links = LocalFs::new().property_capabilities().links;
    assert!(links.symlink && links.hardlink);
    assert_eq!(links.junction, cfg!(windows));
    assert_eq!(links.shortcut, cfg!(windows));
}
