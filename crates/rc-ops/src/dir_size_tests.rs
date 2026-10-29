use std::path::Path;

use rc_vfs::{LinkKind, LocalFs, MemoryFs, Vfs, VfsError, WriteMode};
use tokio::io::AsyncWriteExt;

use super::{DirUsage, dir_size, dir_sizes};

async fn write(fs: &dyn Vfs, path: &Path, bytes: &[u8]) {
    let mut stream = fs
        .open_write(path, WriteMode::CreateNew)
        .await
        .expect("open");
    stream.write_all(bytes).await.expect("write");
    stream.shutdown().await.expect("shutdown");
}

/// `/tree` with 3 + 4 bytes at the top and 5 in `/tree/sub/deeper`.
async fn tree() -> MemoryFs {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/tree/sub/deeper"), true)
        .await
        .expect("mkdir");
    write(&fs, Path::new("/tree/a"), b"abc").await;
    write(&fs, Path::new("/tree/b"), b"abcd").await;
    write(&fs, Path::new("/tree/sub/deeper/c"), b"abcde").await;
    fs
}

#[tokio::test]
async fn a_tree_is_summed_with_its_files_and_folders() {
    let fs = tree().await;
    let usage = dir_size(&fs, Path::new("/tree")).await.expect("count");
    assert_eq!(
        usage,
        DirUsage {
            bytes: 12,
            allocated: None,
            files: 3,
            dirs: 2,
            hard_links: 0,
            unreadable: 0,
        }
    );
}

#[tokio::test]
async fn a_file_counts_itself() {
    let fs = tree().await;
    let usage = dir_size(&fs, Path::new("/tree/b")).await.expect("count");
    assert_eq!((usage.bytes, usage.files, usage.dirs), (4, 1, 0));
}

#[tokio::test]
async fn several_starts_add_up() {
    let fs = tree().await;
    let starts = [Path::new("/tree/a"), Path::new("/tree/sub")];
    let (each, total) = dir_sizes(&fs, &starts).await.expect("count");
    assert_eq!(each.len(), 2);
    assert_eq!((total.bytes, total.files, total.dirs), (8, 2, 1));
}

#[tokio::test]
async fn a_missing_start_fails() {
    let fs = tree().await;
    assert!(matches!(
        dir_size(&fs, Path::new("/nothing")).await,
        Err(VfsError::NotFound { .. })
    ));
}

#[tokio::test]
async fn a_hard_link_is_counted_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("big.bin");
    write(&fs, &file, &[1; 1000]).await;
    fs.mkdir(&dir.path().join("sub"), false)
        .await
        .expect("mkdir");
    fs.create_link(&dir.path().join("sub/again.bin"), &file, LinkKind::Hardlink)
        .await
        .expect("hard link");
    let usage = dir_size(&fs, dir.path()).await.expect("count");
    assert_eq!(usage.bytes, 1000);
    assert_eq!(usage.files, 1);
    assert_eq!(usage.hard_links, 1);
    assert_eq!(usage.dirs, 1);
}

#[tokio::test]
async fn links_are_not_followed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let inner = dir.path().join("inner");
    fs.mkdir(&inner, false).await.expect("mkdir");
    write(&fs, &inner.join("f"), &[1; 10]).await;
    // A symbolic link may need a privilege on Windows; without it the count has nothing to skip.
    if fs
        .create_link(&dir.path().join("loop"), dir.path(), LinkKind::Symlink)
        .await
        .is_err()
    {
        return;
    }
    let usage = dir_size(&fs, dir.path()).await.expect("count");
    assert_eq!(usage.bytes, 10);
    assert_eq!(
        (usage.files, usage.dirs),
        (2, 1),
        "the link is an entry, not a folder"
    );
}
