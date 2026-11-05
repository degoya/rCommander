//! Links to folders in a copy or a move (RC-080-63), as Total Commander's `CopyLinks` and
//! `IgnoreLinks` have them: a move on one disk renames the link, else `ops.copy_links` copies
//! the folder's contents, makes the link anew, or skips it. Real links on the local disk, so
//! Unix only here; Windows' junctions go through the CI.
#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::support::Harness;
use rc_ops::{
    Backend, CopyLinks, ErrorPolicy, JobKind, JobOptions, JobSnapshot, JobSpec, JobState,
};
use rc_vfs::{BackendId, LocalFs, MemoryFs, VfsPath};

/// The local disk once more under another id: a second file system to the queue, so a move
/// there copies instead of renaming.
const OTHER: BackendId = BackendId::Memory(7);

fn on(backend: &BackendId, path: &Path) -> VfsPath {
    VfsPath {
        backend: backend.clone(),
        path: path.to_path_buf(),
    }
}

/// `root/real/inner.txt`, `root/src/link` leading to `../real`, and an empty `root/dst`.
fn tree() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("temp dir");
    let real = root.path().join("real");
    fs::create_dir(&real).expect("mkdir");
    fs::write(real.join("inner.txt"), b"inner").expect("write");
    let src = root.path().join("src");
    fs::create_dir(&src).expect("mkdir");
    symlink("../real", src.join("link")).expect("link");
    let dst = root.path().join("dst");
    fs::create_dir(&dst).expect("mkdir");
    (root, real, src, dst)
}

fn harness() -> Harness {
    let harness = Harness::start();
    harness.local();
    harness
        .queue
        .register(OTHER, Backend::new(Arc::new(LocalFs::new())));
    harness
}

async fn run(
    harness: &Harness,
    moving: bool,
    source: VfsPath,
    target: VfsPath,
    links: CopyLinks,
) -> JobSnapshot {
    let sources = vec![source];
    let kind = if moving {
        JobKind::Move { sources, target }
    } else {
        JobKind::Copy { sources, target }
    };
    harness
        .run(JobSpec {
            kind,
            options: JobOptions {
                on_error: ErrorPolicy::Skip,
                copy_links: links,
                ..JobOptions::default()
            },
        })
        .await
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

fn the_folder_is_untouched(real: &Path) {
    assert_eq!(fs::read(real.join("inner.txt")).expect("read"), b"inner");
}

#[tokio::test]
async fn a_move_on_one_disk_renames_the_link_whatever_the_setting() {
    for links in [CopyLinks::Contents, CopyLinks::Link, CopyLinks::Skip] {
        let harness = harness();
        let (_root, real, src, dst) = tree();
        let local = BackendId::Local;
        let done = run(
            &harness,
            true,
            on(&local, &src.join("link")),
            on(&local, &dst),
            links,
        )
        .await;
        assert_eq!(done.state, JobState::Completed, "{links:?}: {done:?}");
        assert!(done.failures.is_empty(), "{links:?}: {done:?}");
        assert!(is_link(&dst.join("link")), "{links:?}: the link moved");
        assert!(!src.join("link").exists(), "{links:?}");
        the_folder_is_untouched(&real);
    }
}

#[tokio::test]
async fn contents_copies_the_folder_as_a_plain_one() {
    let harness = harness();
    let (_root, real, src, dst) = tree();
    let local = BackendId::Local;
    let done = run(
        &harness,
        false,
        on(&local, &src.join("link")),
        on(&local, &dst),
        CopyLinks::Contents,
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty(), "{done:?}");
    assert!(!is_link(&dst.join("link")), "a plain folder");
    assert_eq!(
        fs::read(dst.join("link/inner.txt")).expect("copied"),
        b"inner"
    );
    assert!(is_link(&src.join("link")), "a copy keeps its source");
    the_folder_is_untouched(&real);
}

#[tokio::test]
async fn contents_moved_to_another_disk_removes_only_the_link() {
    let harness = harness();
    let (_root, real, src, dst) = tree();
    let done = run(
        &harness,
        true,
        on(&BackendId::Local, &src.join("link")),
        on(&OTHER, &dst),
        CopyLinks::Contents,
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty(), "{done:?}");
    assert_eq!(
        fs::read(dst.join("link/inner.txt")).expect("copied"),
        b"inner"
    );
    assert!(
        !is_link(&src.join("link")),
        "the link at the source is gone"
    );
    the_folder_is_untouched(&real);
}

#[tokio::test]
async fn link_makes_the_link_anew_with_its_target_unchanged() {
    for moving in [false, true] {
        let harness = harness();
        let (_root, real, src, dst) = tree();
        let done = run(
            &harness,
            moving,
            on(&BackendId::Local, &src.join("link")),
            on(&OTHER, &dst),
            CopyLinks::Link,
        )
        .await;
        assert_eq!(done.state, JobState::Completed, "{done:?}");
        assert!(done.failures.is_empty(), "{done:?}");
        assert_eq!(
            fs::read_link(dst.join("link")).expect("a link"),
            PathBuf::from("../real"),
            "a relative target stays relative"
        );
        assert_eq!(is_link(&src.join("link")), !moving, "moving: {moving}");
        the_folder_is_untouched(&real);
    }
}

#[tokio::test]
async fn link_onto_a_target_without_links_has_its_own_code() {
    let harness = harness();
    let memory = MemoryFs::new();
    let none = BackendId::Memory(8);
    harness
        .queue
        .register(none.clone(), Backend::new(Arc::new(memory)));
    let (_root, _real, src, _dst) = tree();
    let done = run(
        &harness,
        false,
        on(&BackendId::Local, &src.join("link")),
        on(&none, Path::new("/")),
        CopyLinks::Link,
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert_eq!(done.failures[0].code, "links_unsupported");
}

#[tokio::test]
async fn skip_passes_the_link_over_and_counts_it() {
    let harness = harness();
    let (_root, real, src, dst) = tree();
    let done = run(
        &harness,
        false,
        on(&BackendId::Local, &src.join("link")),
        on(&OTHER, &dst),
        CopyLinks::Skip,
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.skipped, 1, "{done:?}");
    assert!(
        fs::symlink_metadata(dst.join("link")).is_err(),
        "nothing made"
    );
    the_folder_is_untouched(&real);
}

#[tokio::test]
async fn contents_does_not_follow_a_link_into_its_own_way() {
    let harness = harness();
    let (_root, _real, src, dst) = tree();
    let folder = src.join("folder");
    fs::create_dir(&folder).expect("mkdir");
    fs::write(folder.join("a.txt"), b"a").expect("write");
    symlink("..", folder.join("up")).expect("a link to a folder above");
    let done = run(
        &harness,
        false,
        on(&BackendId::Local, &folder),
        on(&BackendId::Local, &dst),
        CopyLinks::Contents,
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert_eq!(done.failures[0].code, "link_loop");
    assert_eq!(fs::read(dst.join("folder/a.txt")).expect("copied"), b"a");
    assert!(fs::symlink_metadata(dst.join("folder/up")).is_err());
}
