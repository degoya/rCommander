//! What only Unix has: dot files, symbolic links anyone may create, permission bits.

use std::{os::unix::fs::symlink, path::PathBuf};

use super::{by_name, write};
use crate::{EntryKind, LocalFs, SetAttributes, TargetKind, Vfs, WriteMode};

#[tokio::test]
async fn dot_files_are_hidden() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    write(&fs, &dir.path().join(".rc"), WriteMode::CreateNew, b"").await;
    write(&fs, &dir.path().join("rc"), WriteMode::CreateNew, b"").await;

    let entries = by_name(fs.list(dir.path()).await.expect("list"));
    assert!(entries[0].hidden, "{:?}", entries[0]);
    assert!(!entries[1].hidden, "{:?}", entries[1]);
}

#[tokio::test]
async fn links_are_described_with_their_targets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    write(&fs, &dir.path().join("file"), WriteMode::CreateNew, b"abc").await;
    fs.mkdir(&dir.path().join("dir"), false)
        .await
        .expect("mkdir");
    symlink("file", dir.path().join("to-file")).expect("link");
    symlink(dir.path().join("dir"), dir.path().join("to-dir")).expect("link");
    symlink("nowhere", dir.path().join("dangling")).expect("link");

    let entries = by_name(fs.list(dir.path()).await.expect("list"));
    let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["dangling", "dir", "file", "to-dir", "to-file"]);

    assert_eq!(
        entries[0].kind,
        EntryKind::Symlink {
            target: PathBuf::from("nowhere"),
            target_kind: None
        }
    );
    assert_eq!(
        entries[3].kind,
        EntryKind::Symlink {
            target: dir.path().join("dir"),
            target_kind: Some(TargetKind::Dir)
        }
    );
    assert!(entries[3].is_dir_like());
    assert_eq!(
        entries[4].kind,
        EntryKind::Symlink {
            target: PathBuf::from("file"),
            target_kind: Some(TargetKind::File)
        }
    );
    assert_eq!(entries[4].size, 3, "a link to a file shows the file's size");

    let stat = fs.stat(&dir.path().join("to-file")).await.expect("stat");
    assert_eq!(stat.kind, entries[4].kind, "stat does not follow the link");
}

#[tokio::test]
async fn removing_a_link_keeps_its_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let target = dir.path().join("dir");
    fs.mkdir(&target, false).await.expect("mkdir");
    write(&fs, &target.join("keep"), WriteMode::CreateNew, b"k").await;
    symlink(&target, dir.path().join("link")).expect("link");

    fs.remove_file(&dir.path().join("link"))
        .await
        .expect("unlink");
    assert_eq!(fs.list(&target).await.expect("list").len(), 1);
}

#[tokio::test]
async fn modes_are_read_and_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("script.sh");
    write(&fs, &file, WriteMode::CreateNew, b"#!/bin/sh\n").await;

    let change = SetAttributes {
        mode: Some(0o750),
        ..SetAttributes::default()
    };
    fs.set_attributes(&file, change).await.expect("chmod");
    let attributes = fs.stat(&file).await.expect("stat").attributes;
    assert_eq!(attributes.mode, Some(0o750));
    assert!(!attributes.readonly);
    assert_eq!(attributes.windows, None);

    let read_only = SetAttributes {
        mode: Some(0o644),
        readonly: Some(true),
    };
    fs.set_attributes(&file, read_only).await.expect("chmod");
    assert_eq!(
        fs.stat(&file).await.expect("stat").attributes.mode,
        Some(0o444),
        "read-only clears every write bit"
    );
}

#[tokio::test]
async fn an_unreadable_directory_is_permission_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let locked = dir.path().join("locked");
    fs.mkdir(&locked, false).await.expect("mkdir");
    let none = SetAttributes {
        mode: Some(0o000),
        ..SetAttributes::default()
    };
    fs.set_attributes(&locked, none).await.expect("chmod");

    let result = fs.list(&locked).await;
    let restore = SetAttributes {
        mode: Some(0o700),
        ..SetAttributes::default()
    };
    fs.set_attributes(&locked, restore)
        .await
        .expect("chmod back");
    // Root reads everything; the mapping is only observable for an ordinary user.
    if let Err(error) = result {
        assert_eq!(error.code(), "permission_denied");
    }
}

/// RC-060-02: the owner column shows user and group by name, as `ls -l` does.
#[tokio::test]
async fn entries_name_their_owner() {
    use std::os::unix::fs::MetadataExt;

    use nix::unistd::{Gid, Group, Uid, User};

    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let path = dir.path().join("mine");
    write(&fs, &path, WriteMode::CreateNew, b"").await;
    let meta = std::fs::metadata(&path).expect("metadata");
    let user = User::from_uid(Uid::from_raw(meta.uid()))
        .ok()
        .flatten()
        .map_or_else(|| meta.uid().to_string(), |user| user.name);
    let group = Group::from_gid(Gid::from_raw(meta.gid()))
        .ok()
        .flatten()
        .map_or_else(|| meta.gid().to_string(), |group| group.name);

    let listed = fs.list(dir.path()).await.expect("list");
    let owner = listed[0].owner.clone().expect("an owner on Unix");
    assert_eq!(
        (owner.user.as_str(), owner.group.as_str()),
        (user.as_str(), group.as_str())
    );
    let stat = fs.stat(&path).await.expect("stat");
    assert_eq!(stat.owner, Some(owner));
}
