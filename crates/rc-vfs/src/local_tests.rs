use std::{path::Path, sync::Arc};

use tokio::io::AsyncWriteExt;

use super::LocalFs;
use crate::{Entry, SetAttributes, Vfs, WriteMode, conformance};

async fn write(fs: &LocalFs, path: &Path, mode: WriteMode, bytes: &[u8]) {
    let mut stream = fs.open_write(path, mode).await.expect("open for writing");
    stream.write_all(bytes).await.expect("write");
    stream.shutdown().await.expect("shutdown");
}

fn by_name(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

#[tokio::test]
async fn local_passes_the_conformance_suite() {
    let dir = tempfile::tempdir().expect("tempdir");
    conformance::run(&LocalFs::new(), dir.path()).await;
}

/// The read-only part holds for the reference backend too, which proves the part itself; the
/// local backend also offers the seekable read.
#[tokio::test]
async fn local_passes_the_read_only_conformance_suite() {
    let dir = tempfile::tempdir().expect("tempdir");
    conformance::prepare_read_only(&LocalFs::new(), dir.path()).await;
    conformance::run_read_only(&LocalFs::new(), dir.path()).await;
    let seekable = LocalFs::new()
        .open_read_seekable(&dir.path().join("a.txt"))
        .await;
    assert!(seekable.is_ok(), "the local backend seeks");
}

/// Two instances, as two panels hold them: the bytes go from one stream into the other.
#[tokio::test]
async fn a_stream_goes_from_one_backend_into_another_without_a_temp_file() {
    let from_dir = tempfile::tempdir().expect("tempdir");
    let to_dir = tempfile::tempdir().expect("tempdir");
    let from: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let to: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    conformance::copy_between(from.as_ref(), from_dir.path(), to.as_ref(), to_dir.path()).await;
}

#[tokio::test]
async fn read_only_round_trips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("ro.txt");
    write(&fs, &file, WriteMode::CreateNew, b"r").await;

    let on = SetAttributes {
        readonly: Some(true),
        ..SetAttributes::default()
    };
    fs.set_attributes(&file, on).await.expect("read-only on");
    assert!(fs.stat(&file).await.expect("stat").attributes.readonly);

    let off = SetAttributes {
        readonly: Some(false),
        ..SetAttributes::default()
    };
    fs.set_attributes(&file, off).await.expect("read-only off");
    assert!(!fs.stat(&file).await.expect("stat").attributes.readonly);
}

/// The viewer reads a window anywhere in a file without reading what lies before it.
#[tokio::test]
async fn a_seekable_read_starts_where_it_seeks_and_refuses_a_directory() {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("digits.txt");
    write(&fs, &file, WriteMode::CreateNew, b"0123456789").await;
    let mut stream = fs.open_read_seekable(&file).await.expect("seekable");
    assert_eq!(
        stream.seek(std::io::SeekFrom::End(0)).await.expect("end"),
        10
    );
    stream
        .seek(std::io::SeekFrom::Start(7))
        .await
        .expect("seek");
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).await.expect("read");
    assert_eq!(rest, b"789");
    assert!(matches!(
        fs.open_read_seekable(dir.path()).await.err(),
        Some(crate::VfsError::IsADirectory { .. })
    ));
}

/// RC-080-56: a rename that never replaces - a file or an empty folder at the target is
/// `already_exists`, both stay as they were; onto a free name it renames. Also through the
/// trait's default, which looks first (`MemoryFs`).
#[tokio::test]
async fn rename_no_replace_refuses_an_existing_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let (from, taken, free) = (
        dir.path().join("a.txt"),
        dir.path().join("b.txt"),
        dir.path().join("c.txt"),
    );
    write(&fs, &from, WriteMode::CreateNew, b"mine").await;
    write(&fs, &taken, WriteMode::CreateNew, b"theirs").await;
    let refused = fs.rename_no_replace(&from, &taken).await;
    assert!(
        matches!(&refused, Err(crate::VfsError::AlreadyExists { path }) if *path == taken),
        "{refused:?}"
    );
    assert_eq!(std::fs::read(&from).expect("source"), b"mine");
    assert_eq!(std::fs::read(&taken).expect("target"), b"theirs");
    fs.rename_no_replace(&from, &free).await.expect("renamed");
    assert!(!from.exists());
    assert_eq!(std::fs::read(&free).expect("renamed"), b"mine");

    let (folder, empty) = (dir.path().join("folder"), dir.path().join("empty"));
    std::fs::create_dir(&folder).expect("folder");
    std::fs::write(folder.join("inside.txt"), b"x").expect("inside");
    std::fs::create_dir(&empty).expect("empty");
    let refused = fs.rename_no_replace(&folder, &empty).await;
    assert!(
        matches!(&refused, Err(crate::VfsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert!(folder.join("inside.txt").is_file(), "the source stays");
    assert!(empty.is_dir() && !empty.join("inside.txt").exists());

    let memory = crate::MemoryFs::new();
    write_memory(&memory, "/a", b"mine").await;
    write_memory(&memory, "/b", b"theirs").await;
    let refused = memory
        .rename_no_replace(Path::new("/a"), Path::new("/b"))
        .await;
    assert!(matches!(
        refused,
        Err(crate::VfsError::AlreadyExists { .. })
    ));
    memory
        .rename_no_replace(Path::new("/a"), Path::new("/c"))
        .await
        .expect("renamed");
}

async fn write_memory(fs: &crate::MemoryFs, path: &str, bytes: &[u8]) {
    conformance::write_file(fs, Path::new(path), WriteMode::CreateNew, bytes).await;
}

#[cfg(unix)]
#[path = "local_unix_tests.rs"]
mod unix;

#[cfg(windows)]
#[path = "local_windows_tests.rs"]
mod windows;

#[path = "local_props_tests.rs"]
mod props;
