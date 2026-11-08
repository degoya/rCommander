use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rc_ops::dir_size;
use rc_vfs::{Entry, LinkKind, LocalFs, MemoryFs, Vfs, VfsPath, WriteMode};
use tokio::io::AsyncWriteExt as _;

use super::{FolderRow, SizeBy, UsageTree, run};
use crate::search::SearchCancel;
use crate::test_support;
use crate::{Command, Outcome};

async fn write(fs: &dyn Vfs, path: &str, bytes: usize) {
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        fs.mkdir(parent, true).await.expect("mkdir");
    }
    let mut stream = fs
        .open_write(path, WriteMode::Truncate)
        .await
        .expect("open");
    stream.write_all(&vec![1; bytes]).await.expect("write");
    stream.shutdown().await.expect("shutdown");
}

/// ```text
/// /tree/
///   a          3
///   b          4
///   sub/       x 10, deeper/c 5, deeper/d 50
///   other/     y 20
/// ```
async fn memory_tree() -> MemoryFs {
    let fs = MemoryFs::new();
    write(&fs, "/tree/a", 3).await;
    write(&fs, "/tree/b", 4).await;
    write(&fs, "/tree/sub/x", 10).await;
    write(&fs, "/tree/sub/deeper/c", 5).await;
    write(&fs, "/tree/sub/deeper/d", 50).await;
    write(&fs, "/tree/other/y", 20).await;
    fs
}

async fn analysed(fs: &dyn Vfs, root: &Path, keep: usize, max_folders: usize) -> UsageTree {
    let tree = Arc::new(Mutex::new(UsageTree::new(
        VfsPath::local(root),
        keep,
        max_folders,
    )));
    let failed = run(fs, root, Arc::clone(&tree), SearchCancel::default()).await;
    assert!(failed.is_none());
    Arc::try_unwrap(tree)
        .expect("the walk let go")
        .into_inner()
        .expect("not poisoned")
}

fn names(rows: &[FolderRow]) -> Vec<String> {
    rows.iter().map(|row| row.name.clone()).collect()
}

#[tokio::test]
async fn every_folder_sums_what_dir_size_counts_for_it() {
    let fs = memory_tree().await;
    let tree = analysed(&fs, Path::new("/tree"), 100, 1000).await;
    assert!(tree.finished() && !tree.cancelled() && !tree.truncated());
    for id in 0..tree.len() {
        let folder = Path::new("/tree").join(tree.relative(id));
        let counted = dir_size(&fs, &folder).await.expect("count");
        let node = tree.node(id).expect("node");
        assert_eq!(node.usage, counted, "{}", folder.display());
    }
    let rows = tree.rows(UsageTree::ROOT, SizeBy::Bytes);
    assert_eq!(
        names(&rows),
        ["sub", "other", ""],
        "largest first, own files last"
    );
    assert_eq!(rows[0].usage.bytes, 65);
    assert_eq!(rows[0].share, 706, "65 of 92 bytes");
    assert_eq!(rows[2].node, None);
    assert_eq!(rows[2].usage.bytes, 7);
}

#[tokio::test]
async fn the_largest_files_sorted_and_limited() {
    let fs = memory_tree().await;
    let tree = analysed(&fs, Path::new("/tree"), 3, 1000).await;
    let largest: Vec<(PathBuf, u64)> = tree
        .largest()
        .into_iter()
        .map(|file| (PathBuf::from(file.path), file.bytes))
        .collect();
    assert_eq!(
        largest,
        [
            (Path::new("sub").join("deeper").join("d"), 50),
            (PathBuf::from("other").join("y"), 20),
            (PathBuf::from("sub").join("x"), 10),
        ]
    );
}

#[tokio::test]
async fn a_hard_link_counts_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("big.bin");
    std::fs::write(&file, vec![1; 1000]).expect("write");
    std::fs::create_dir(dir.path().join("sub")).expect("mkdir");
    fs.create_link(&dir.path().join("sub/again.bin"), &file, LinkKind::Hardlink)
        .await
        .expect("hard link");
    let tree = analysed(&fs, dir.path(), 10, 1000).await;
    let root = tree.node(UsageTree::ROOT).expect("root");
    assert_eq!((root.usage.bytes, root.usage.hard_links), (1000, 1));
    assert_eq!(tree.largest().len(), 1);
}

#[tokio::test]
async fn the_walk_stops_at_the_folder_limit_and_says_so() {
    let fs = memory_tree().await;
    let tree = analysed(&fs, Path::new("/tree"), 10, 2).await;
    assert!(tree.truncated());
    assert_eq!(tree.len(), 2);
    assert!(tree.finished());
}

/// A backend that stops the analysis once `/tree/sub` is listed.
struct StopAtSub {
    inner: MemoryFs,
    cancel: SearchCancel,
}

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for StopAtSub {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }

    async fn list(&self, dir: &Path) -> rc_vfs::Result<Vec<Entry>> {
        if dir == Path::new("/tree/sub") {
            self.cancel.cancel();
        }
        self.inner.list(dir).await
    }
}

#[tokio::test]
async fn a_stop_keeps_what_was_counted() {
    let cancel = SearchCancel::default();
    let fs = StopAtSub {
        inner: memory_tree().await,
        cancel: cancel.clone(),
    };
    let tree = Arc::new(Mutex::new(UsageTree::new(
        VfsPath::local("/tree"),
        10,
        1000,
    )));
    run(&fs, Path::new("/tree"), Arc::clone(&tree), cancel).await;
    let tree = tree.lock().expect("not poisoned");
    assert!(tree.cancelled());
    let root = tree.node(UsageTree::ROOT).expect("root");
    assert!(root.usage.bytes >= 7, "the start's own files were counted");
    assert!(root.usage.bytes < 92, "not everything");
}

#[tokio::test]
async fn files_disk_usage_reports_to_its_depth_and_limit() {
    let (deep, file, mid) = ("x".repeat(300), "x".repeat(100), "x".repeat(50));
    let dir = test_support::tree(&[
        ("big/inner/deep.bin", deep.as_str()),
        ("big/file.bin", file.as_str()),
        ("small/s.bin", "xx"),
        ("mid/m.bin", mid.as_str()),
        ("top.bin", "x"),
    ]);
    let engine = test_support::headless(dir.path()).await;
    let outcome = engine
        .dispatch_mcp(Command::DiskUsage {
            panel: None,
            path: None,
            depth: Some(2),
            limit: Some(2),
        })
        .await
        .expect("disk usage");
    let Outcome::DiskUsage(report) = outcome else {
        panic!("a disk-usage report, got {outcome:?}");
    };
    assert_eq!(report.usage.bytes, 453);
    let top: Vec<&str> = report.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(top, ["big", "mid"]);
    assert_eq!(report.more, 1, "small left out");
    assert_eq!(report.folders[0].folders[0].name, "inner");
    assert!(report.folders[0].folders[0].folders.is_empty(), "depth 2");
    assert_eq!(report.largest.len(), 2);
    assert_eq!(report.largest[0].bytes, 300);
}
