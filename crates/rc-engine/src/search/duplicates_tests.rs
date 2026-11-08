use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rc_vfs::{BackendId, LinkKind, LocalFs, MemoryFs, ReadStream, Vfs, WriteMode};
use tokio::io::AsyncWriteExt as _;

use super::{DuplicateQuery, DuplicateReport, Finder};
use crate::read_guard::ReadGuard;
use crate::search::{SearchCancel, SearchEvent, SearchQuery, Walk};

async fn write(fs: &dyn Vfs, path: &str, bytes: &[u8]) {
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        fs.mkdir(parent, true).await.expect("mkdir");
    }
    let mut stream = fs
        .open_write(path, WriteMode::Truncate)
        .await
        .expect("open");
    stream.write_all(bytes).await.expect("write");
    stream.shutdown().await.expect("shutdown");
}

/// ```text
/// /root/
///   a.txt          "abc"
///   b.txt          "xyz"     same size as a.txt, other contents
///   c.txt          "abcd"    alone in its size
///   d/a.txt        "abc"     a copy of a.txt by name and contents
///   d/e.bin        "abc"     a copy of a.txt by contents only
///   big1, big2     70 000 bytes alike in the first 64 KiB, the last byte differs
///   big3           a copy of big1
/// ```
async fn memory_tree() -> MemoryFs {
    let fs = MemoryFs::new();
    write(&fs, "/root/a.txt", b"abc").await;
    write(&fs, "/root/b.txt", b"xyz").await;
    write(&fs, "/root/c.txt", b"abcd").await;
    write(&fs, "/root/d/a.txt", b"abc").await;
    write(&fs, "/root/d/e.bin", b"abc").await;
    let mut big = vec![7_u8; 70_000];
    write(&fs, "/root/big1", &big).await;
    write(&fs, "/root/big3", &big).await;
    big[69_999] = 8;
    write(&fs, "/root/big2", &big).await;
    fs
}

fn walk(vfs: Arc<dyn Vfs>, root: &Path, duplicates: DuplicateQuery, cancel: &SearchCancel) -> Walk {
    let query = SearchQuery {
        duplicates: Some(duplicates),
        ..SearchQuery::default()
    };
    Walk::new(
        vfs,
        BackendId::Local,
        vec![root.to_path_buf()],
        query.compile().expect("valid query"),
        cancel.clone(),
    )
}

async fn find(vfs: Arc<dyn Vfs>, root: &Path, duplicates: DuplicateQuery) -> DuplicateReport {
    let cancel = SearchCancel::default();
    let walk = walk(Arc::clone(&vfs), root, duplicates, &cancel);
    Finder::new(vfs, duplicates, cancel, None).run(walk).await
}

/// Each group as its paths below `root`, with `!` after a hard link.
fn shown(report: &DuplicateReport, root: &Path) -> Vec<Vec<String>> {
    report
        .groups
        .iter()
        .map(|group| {
            group
                .files
                .iter()
                .map(|file| {
                    let below = file.path.path.strip_prefix(root).expect("below the root");
                    let mut name = below.to_string_lossy().replace('\\', "/");
                    if file.hard_link {
                        name.push('!');
                    }
                    name
                })
                .collect()
        })
        .collect()
}

const BY_SIZE: DuplicateQuery = DuplicateQuery {
    name: false,
    size: true,
    content: false,
};

const BY_CONTENT: DuplicateQuery = DuplicateQuery {
    name: false,
    size: false,
    content: true,
};

#[tokio::test]
async fn groups_by_size_in_walk_order_sorted_by_size() {
    let fs: Arc<dyn Vfs> = Arc::new(memory_tree().await);
    let report = find(fs, Path::new("/root"), BY_SIZE).await;
    assert_eq!(
        shown(&report, Path::new("/root")),
        [
            vec!["a.txt", "b.txt", "d/a.txt", "d/e.bin"],
            vec!["big1", "big2", "big3"],
        ]
    );
    assert_eq!(report.groups[0].size, 3);
    assert_eq!(report.stats.hits, 7);
    assert!(!report.cancelled && !report.truncated);
}

#[tokio::test]
async fn the_same_size_with_other_contents_is_no_group() {
    let fs: Arc<dyn Vfs> = Arc::new(memory_tree().await);
    let report = find(fs, Path::new("/root"), BY_CONTENT).await;
    assert_eq!(
        shown(&report, Path::new("/root")),
        [vec!["a.txt", "d/a.txt", "d/e.bin"], vec!["big1", "big3"]],
        "b.txt differs from a.txt, big2 from big1 past the first 64 KiB"
    );
}

#[tokio::test]
async fn groups_by_name_and_size_sorted_by_name() {
    let fs: Arc<dyn Vfs> = Arc::new(memory_tree().await);
    let by_name = DuplicateQuery {
        name: true,
        ..BY_SIZE
    };
    let report = find(fs, Path::new("/root"), by_name).await;
    assert_eq!(
        shown(&report, Path::new("/root")),
        [vec!["a.txt", "d/a.txt"]]
    );
}

#[tokio::test]
async fn a_query_without_size_or_contents_is_refused() {
    let query = SearchQuery {
        duplicates: Some(DuplicateQuery {
            name: true,
            ..DuplicateQuery::default()
        }),
        ..SearchQuery::default()
    };
    assert!(query.compile().is_err());
}

#[tokio::test]
async fn a_hard_link_is_no_duplicate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let file = dir.path().join("one.bin");
    std::fs::write(&file, b"same").expect("write");
    fs.create_link(&dir.path().join("two.bin"), &file, LinkKind::Hardlink)
        .await
        .expect("hard link");
    let vfs: Arc<dyn Vfs> = Arc::new(fs);
    for query in [BY_SIZE, BY_CONTENT] {
        let report = find(Arc::clone(&vfs), dir.path(), query).await;
        assert!(
            report.groups.is_empty(),
            "two names of one file: {:?}",
            shown(&report, dir.path())
        );
    }
    std::fs::write(dir.path().join("three.bin"), b"same").expect("write");
    for query in [BY_SIZE, BY_CONTENT] {
        let report = find(Arc::clone(&vfs), dir.path(), query).await;
        assert_eq!(
            shown(&report, dir.path()),
            [vec!["one.bin", "three.bin", "two.bin!"]]
        );
    }
}

/// A backend that notes every file opened for reading and, when asked, cancels the search at the
/// first one.
struct Watched {
    inner: MemoryFs,
    opened: Mutex<Vec<PathBuf>>,
    cancel: Option<SearchCancel>,
}

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Watched {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }

    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        self.opened
            .lock()
            .expect("not poisoned")
            .push(path.to_path_buf());
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        self.inner.open_read(path).await
    }
}

#[tokio::test]
async fn only_files_of_a_size_shared_are_read_and_guarded_ones_never() {
    let watched = Arc::new(Watched {
        inner: memory_tree().await,
        opened: Mutex::new(Vec::new()),
        cancel: None,
    });
    let vfs: Arc<dyn Vfs> = watched.clone();
    let cancel = SearchCancel::default();
    let walk = walk(Arc::clone(&vfs), Path::new("/root"), BY_CONTENT, &cancel);
    let guard = ReadGuard::new(
        |path| (path.file_name()? == "e.bin").then_some("a test secret"),
        &BackendId::Local,
    );
    let (sender, mut events) = tokio::sync::mpsc::channel(64);
    let report = Finder::new(vfs, BY_CONTENT, cancel, Some(guard))
        .streaming(sender)
        .run(walk)
        .await;
    let opened = watched.opened.lock().expect("not poisoned").clone();
    assert!(
        !opened.contains(&PathBuf::from("/root/c.txt")),
        "alone in its size"
    );
    assert!(!opened.contains(&PathBuf::from("/root/d/e.bin")), "guarded");
    assert!(opened.contains(&PathBuf::from("/root/b.txt")));
    assert_eq!(
        shown(&report, Path::new("/root")),
        [vec!["a.txt", "d/a.txt"], vec!["big1", "big3"]]
    );
    assert_eq!(report.stats.skipped, 1);
    let mut skipped = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let SearchEvent::Skipped { path, code } = event {
            skipped.push((path.path, code));
        }
    }
    assert_eq!(
        skipped,
        [(PathBuf::from("/root/d/e.bin"), "sensitive_path")]
    );
}

#[tokio::test]
async fn a_stop_while_reading_ends_the_run_as_cancelled() {
    let cancel = SearchCancel::default();
    let vfs: Arc<dyn Vfs> = Arc::new(Watched {
        inner: memory_tree().await,
        opened: Mutex::new(Vec::new()),
        cancel: Some(cancel.clone()),
    });
    let walk = walk(Arc::clone(&vfs), Path::new("/root"), BY_CONTENT, &cancel);
    let report = Finder::new(vfs, BY_CONTENT, cancel, None).run(walk).await;
    assert!(report.cancelled);
    assert!(report.groups.is_empty());
}

#[tokio::test]
async fn past_the_limit_the_result_is_cut() {
    let vfs: Arc<dyn Vfs> = Arc::new(memory_tree().await);
    let cancel = SearchCancel::default();
    let walk = walk(Arc::clone(&vfs), Path::new("/root"), BY_SIZE, &cancel);
    let report = Finder::new(vfs, BY_SIZE, cancel, None)
        .with_limit(3)
        .run(walk)
        .await;
    assert!(report.truncated && report.stats.truncated);
    assert_eq!(
        shown(&report, Path::new("/root")),
        [vec!["a.txt", "b.txt"]],
        "the first three files: a.txt, b.txt, big1"
    );
}
