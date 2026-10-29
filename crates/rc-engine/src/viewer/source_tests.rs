use std::{fs, sync::Arc};

use rc_vfs::{LocalFs, VfsError};

use super::*;
use crate::viewer::test_support::{Resumable, StreamOnly, local, numbered};

#[tokio::test]
async fn a_window_anywhere_reads_exactly_those_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let len = 3 * BLOCK_SIZE + 123;
    let path = numbered(dir.path(), len);
    let all = fs::read(&path).expect("read");
    let window = FileWindow::open(local(), &path).await.expect("open");
    assert_eq!(window.size(), len as u64);
    for (offset, want) in [
        (0, 10),
        (BLOCK_SIZE - 5, 10),
        (2 * BLOCK_SIZE + 7, BLOCK_SIZE + 500),
        (len - 3, 100),
    ] {
        let got = window.read_at(offset as u64, want).await.expect("read");
        let end = (offset + want).min(len);
        assert_eq!(got, &all[offset..end], "at {offset}");
        let uncached = window
            .read_uncached(offset as u64, want)
            .await
            .expect("read");
        assert_eq!(uncached, got, "uncached at {offset}");
    }
    assert!(
        window
            .read_at(len as u64 + 5, 10)
            .await
            .expect("read")
            .is_empty()
    );
}

#[tokio::test]
async fn the_cache_stays_bounded_however_much_is_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let len = (CACHE_BLOCKS + 20) * BLOCK_SIZE;
    let path = numbered(dir.path(), len);
    let window = FileWindow::open(local(), &path).await.expect("open");
    let mut offset = 0;
    while offset < len {
        window.read_at(offset as u64, 4096).await.expect("read");
        offset += BLOCK_SIZE;
    }
    assert_eq!(window.cached_bytes(), CACHE_BLOCKS * BLOCK_SIZE);
    // A read larger than the cache still answers whole.
    let big = window.read_at(0, len).await.expect("read");
    assert_eq!(big.len(), len);
    assert!(window.cached_bytes() <= CACHE_BLOCKS * BLOCK_SIZE);
}

#[tokio::test]
async fn a_backend_that_cannot_seek_is_read_by_skipping() {
    let dir = tempfile::tempdir().expect("tempdir");
    let len = 2 * BLOCK_SIZE + 9;
    let path = numbered(dir.path(), len);
    let all = fs::read(&path).expect("read");
    let window = FileWindow::open(Arc::new(StreamOnly(LocalFs::new())), &path)
        .await
        .expect("open");
    let got = window
        .read_at(BLOCK_SIZE as u64 + 3, 50)
        .await
        .expect("read");
    assert_eq!(got, &all[BLOCK_SIZE + 3..BLOCK_SIZE + 53]);
    let tail = window
        .read_uncached(len as u64 - 4, 10)
        .await
        .expect("read");
    assert_eq!(tail, &all[len - 4..]);
}

/// A backend that cannot seek but starts a read at an offset (FTP's `REST`) is read from there,
/// never from the start (RC-080-46).
#[tokio::test]
async fn a_backend_that_resumes_is_read_from_the_offset() {
    let dir = tempfile::tempdir().expect("tempdir");
    let len = 2 * BLOCK_SIZE + 9;
    let path = numbered(dir.path(), len);
    let all = fs::read(&path).expect("read");
    let window = FileWindow::open(Arc::new(Resumable(LocalFs::new())), &path)
        .await
        .expect("open");
    let got = window
        .read_uncached(BLOCK_SIZE as u64 + 3, 50)
        .await
        .expect("read");
    assert_eq!(got, &all[BLOCK_SIZE + 3..BLOCK_SIZE + 53]);
    assert!(!window.skips_to_offsets());
}

/// Only a backend that answered it can neither seek nor resume counts as one that skips.
#[tokio::test]
async fn a_stream_only_backend_is_known_to_skip_after_its_first_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = numbered(dir.path(), BLOCK_SIZE);
    let window = FileWindow::open(Arc::new(StreamOnly(LocalFs::new())), &path)
        .await
        .expect("open");
    assert!(!window.skips_to_offsets());
    window.read_uncached(10, 10).await.expect("read");
    assert!(window.skips_to_offsets());
    let local = FileWindow::open(local(), &path).await.expect("open");
    local.read_uncached(10, 10).await.expect("read");
    assert!(!local.skips_to_offsets());
}

#[tokio::test]
async fn a_directory_is_refused_and_a_missing_file_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let refused = FileWindow::open(local(), dir.path()).await;
    assert!(matches!(refused, Err(VfsError::IsADirectory { .. })));
    let missing = FileWindow::open(local(), dir.path().join("none")).await;
    assert!(matches!(missing, Err(VfsError::NotFound { .. })));
}

#[tokio::test]
async fn a_changed_file_is_noticed_and_reloads_with_its_new_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.txt");
    fs::write(&path, b"one\n").expect("write");
    let window = FileWindow::open(local(), &path).await.expect("open");
    assert!(!window.changed().await.expect("stat"));
    fs::write(&path, b"one\ntwo\n").expect("append");
    assert!(window.changed().await.expect("stat"));
    let reloaded = window.reload().await.expect("reload");
    assert_eq!(reloaded.size(), 8);
    assert_eq!(reloaded.read_at(4, 10).await.expect("read"), b"two\n");
}
