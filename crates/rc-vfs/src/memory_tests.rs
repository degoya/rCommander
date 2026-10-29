use std::path::Path;

use tokio::io::AsyncWriteExt;

use super::MemoryFs;
use crate::{SetAttributes, Vfs, WriteMode, conformance};

#[tokio::test]
async fn memory_passes_the_conformance_suite() {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/root"), false).await.expect("mkdir");
    conformance::run(&fs, Path::new("/root")).await;
}

/// The read-only part of the suite, which the archive backend runs too.
#[tokio::test]
async fn memory_passes_the_read_only_conformance_suite() {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/root"), false).await.expect("mkdir");
    conformance::prepare_read_only(&fs, Path::new("/root")).await;
    conformance::run_read_only(&fs, Path::new("/root")).await;
}

#[tokio::test]
async fn two_instances_stream_into_each_other() {
    let from = MemoryFs::new();
    let to = MemoryFs::new();
    from.mkdir(Path::new("/a"), false).await.expect("mkdir");
    to.mkdir(Path::new("/b"), false).await.expect("mkdir");
    conformance::copy_between(&from, Path::new("/a"), &to, Path::new("/b")).await;
}

#[tokio::test]
async fn a_full_disk_fails_the_write_with_storage_full() {
    let fs = MemoryFs::with_capacity(10);
    assert!(fs.capabilities().free_space);
    let mut stream = fs
        .open_write(Path::new("/big"), WriteMode::CreateNew)
        .await
        .expect("open");
    let error = stream
        .write_all(&[7; 32])
        .await
        .expect_err("more than the capacity");
    assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
    assert_eq!(fs.stat(Path::new("/big")).await.expect("stat").size, 10);
    assert_eq!(fs.free_space(Path::new("/")).await.expect("free"), Some(0));
    fs.remove_file(Path::new("/big")).await.expect("remove");
    assert_eq!(fs.free_space(Path::new("/")).await.expect("free"), Some(10));
}

#[tokio::test]
async fn a_read_only_directory_refuses_new_entries() {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/locked"), false).await.expect("mkdir");
    conformance::write_file(&fs, Path::new("/locked/old"), WriteMode::CreateNew, b"x").await;
    fs.set_attributes(
        Path::new("/locked"),
        SetAttributes {
            readonly: Some(true),
            ..SetAttributes::default()
        },
    )
    .await
    .expect("read-only");
    let denied = fs
        .open_write(Path::new("/locked/new"), WriteMode::CreateNew)
        .await
        .map(|_| ())
        .expect_err("refused");
    assert_eq!(denied.code(), "permission_denied");
    let kept = fs
        .remove_file(Path::new("/locked/old"))
        .await
        .expect_err("refused");
    assert_eq!(kept.code(), "permission_denied");
}

#[tokio::test]
async fn renaming_a_directory_moves_its_contents() {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/a/b"), true).await.expect("mkdir");
    conformance::write_file(&fs, Path::new("/a/b/f"), WriteMode::CreateNew, b"f").await;
    fs.mkdir(Path::new("/z"), false).await.expect("mkdir");
    fs.rename(Path::new("/a"), Path::new("/z/a"))
        .await
        .expect("rename");
    assert_eq!(
        conformance::read_file(&fs, Path::new("/z/a/b/f")).await,
        b"f"
    );
    assert_eq!(
        fs.stat(Path::new("/a")).await.expect_err("gone").code(),
        "not_found"
    );
    let inside = fs
        .rename(Path::new("/z"), Path::new("/z/a/z"))
        .await
        .expect_err("into itself");
    assert_eq!(inside.code(), "invalid_path");
}

#[tokio::test]
async fn relative_paths_are_invalid() {
    let fs = MemoryFs::new();
    let error = fs.list(Path::new("rel")).await.expect_err("relative");
    assert_eq!(error.code(), "invalid_path");
}
