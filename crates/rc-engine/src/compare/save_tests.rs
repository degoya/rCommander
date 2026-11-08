use std::{
    fs,
    path::{Path, PathBuf},
};

use rc_vfs::{ArchiveFormat, LocalFs, MemoryFs};
use tokio::io::AsyncReadExt as _;

use super::*;
use crate::compare::edit::CompareSide;

/// A file on the memory backend.
fn memory(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Memory(1),
        path: PathBuf::from(path),
    }
}

async fn put(vfs: &MemoryFs, path: &str, bytes: &[u8]) {
    let mut stream = vfs
        .open_write(Path::new(path), WriteMode::Truncate)
        .await
        .expect("opens");
    stream.write_all(bytes).await.expect("writes");
    stream.shutdown().await.expect("closes");
}

async fn session(vfs: &MemoryFs, left: &VfsPath, right: &VfsPath) -> EditSession {
    load(
        Side { vfs, path: left },
        Side { vfs, path: right },
        &CancellationToken::new(),
    )
    .await
    .expect("reads")
    .expect("text")
}

#[tokio::test]
async fn a_side_that_would_not_be_written_back_as_read_stays_read_only() {
    let vfs = MemoryFs::new();
    put(&vfs, "/clean.txt", b"a\r\nb\n").await;
    // An unpaired surrogate decodes to U+FFFD, which writes back as other bytes.
    put(&vfs, "/broken.txt", b"\xFF\xFEa\0\0\xD8b\0").await;
    let (clean, broken) = (memory("/clean.txt"), memory("/broken.txt"));
    let edits = session(&vfs, &clean, &broken).await;
    assert_eq!(edits.read_only(CompareSide::Left), None);
    assert_eq!(
        edits.read_only(CompareSide::Right),
        Some(ReadOnly::NotLossless)
    );
    assert_eq!(edits.origin(CompareSide::Left).size, 5);
}

#[tokio::test]
async fn a_side_past_the_edit_limit_stays_read_only() {
    let vfs = MemoryFs::new();
    // Long lines: the edit limit is reached long before the line limit of the diff.
    let line = format!("{}\n", "x".repeat(99));
    let big = line.repeat(usize::try_from(EDIT_LIMIT).expect("fits") / line.len() + 1);
    put(&vfs, "/big.txt", big.as_bytes()).await;
    put(&vfs, "/small.txt", b"x\n").await;
    let edits = session(&vfs, &memory("/big.txt"), &memory("/small.txt")).await;
    assert_eq!(edits.read_only(CompareSide::Left), Some(ReadOnly::TooLarge));
    assert!(edits.editable(CompareSide::Right));
}

#[tokio::test]
async fn a_binary_file_has_no_session() {
    let vfs = MemoryFs::new();
    put(&vfs, "/a.bin", b"MZ\0\x03").await;
    put(&vfs, "/b.txt", b"b\n").await;
    let loaded = load(
        Side {
            vfs: &vfs,
            path: &memory("/a.bin"),
        },
        Side {
            vfs: &vfs,
            path: &memory("/b.txt"),
        },
        &CancellationToken::new(),
    )
    .await
    .expect("reads");
    assert!(loaded.is_none());
}

/// The archive's side is judged by its backend: the memory file system stands in for the
/// archive's entries, which are read the same way.
#[tokio::test]
async fn a_file_inside_an_archive_stays_read_only_and_is_never_saved() {
    let vfs = MemoryFs::new();
    put(&vfs, "/inner.txt", b"a\n").await;
    put(&vfs, "/b.txt", b"b\n").await;
    let inner = VfsPath::archive(
        VfsPath::local("/data/site.zip"),
        ArchiveFormat::Zip,
        "/inner.txt",
    );
    let edits = session(&vfs, &inner, &memory("/b.txt")).await;
    assert_eq!(edits.read_only(CompareSide::Left), Some(ReadOnly::Archive));
    let saved = save(&vfs, edits.origin(CompareSide::Left), b"x\n", true).await;
    assert!(matches!(saved, Err(EngineError::ArchiveReadOnly)));
}

#[tokio::test]
async fn a_save_on_a_server_backend_writes_in_place_and_refuses_a_changed_file() {
    let vfs = MemoryFs::new();
    put(&vfs, "/a.txt", b"one\n").await;
    put(&vfs, "/b.txt", b"two\n").await;
    let path = memory("/a.txt");
    let mut edits = session(&vfs, &path, &memory("/b.txt")).await;
    assert!(edits.set_line(CompareSide::Left, 0, "ONE".into()));
    let bytes = edits.encoded(CompareSide::Left).expect("encodes");
    let origin = save(&vfs, edits.origin(CompareSide::Left), &bytes, false)
        .await
        .expect("saved");
    edits.saved(CompareSide::Left, origin.clone());
    assert!(!edits.any_dirty());
    assert_eq!(read(&vfs, "/a.txt").await, b"ONE\n");

    // Someone else writes the file meanwhile: the next save asks first.
    put(&vfs, "/a.txt", b"theirs, longer\n").await;
    assert!(edits.set_line(CompareSide::Left, 0, "mine".into()));
    let bytes = edits.encoded(CompareSide::Left).expect("encodes");
    let refused = save(&vfs, edits.origin(CompareSide::Left), &bytes, false).await;
    assert!(
        matches!(refused, Err(EngineError::CompareChanged { .. })),
        "{refused:?}"
    );
    assert_eq!(read(&vfs, "/a.txt").await, b"theirs, longer\n");
    save(&vfs, edits.origin(CompareSide::Left), &bytes, true)
        .await
        .expect("overwritten");
    assert_eq!(read(&vfs, "/a.txt").await, b"mine\n");
}

#[tokio::test]
async fn a_local_save_goes_through_a_temporary_file_and_keeps_the_encoding() {
    let root = tempfile::tempdir().expect("temp dir");
    let file = root.path().join("ansi.txt");
    fs::write(&file, b"gr\xFC\xDF\r\nzwei\r\n").expect("write");
    fs::write(root.path().join("other.txt"), b"x\r\n").expect("write");
    let vfs = LocalFs::new();
    let (path, other) = (
        VfsPath::local(&file),
        VfsPath::local(root.path().join("other.txt")),
    );
    let mut edits = load(
        Side {
            vfs: &vfs,
            path: &path,
        },
        Side {
            vfs: &vfs,
            path: &other,
        },
        &CancellationToken::new(),
    )
    .await
    .expect("reads")
    .expect("text");
    assert!(edits.set_line(CompareSide::Left, 1, "drei \u{e4}".into()));
    let bytes = edits.encoded(CompareSide::Left).expect("encodes");
    save(&vfs, edits.origin(CompareSide::Left), &bytes, false)
        .await
        .expect("saved");
    assert_eq!(
        fs::read(&file).expect("read"),
        b"gr\xFC\xDF\r\ndrei \xE4\r\n"
    );
    let names: Vec<String> = fs::read_dir(root.path())
        .expect("list")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "no temporary file left: {names:?}");
}

async fn read(vfs: &MemoryFs, path: &str) -> Vec<u8> {
    let mut stream = vfs.open_read(Path::new(path)).await.expect("opens");
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.expect("reads");
    bytes
}
