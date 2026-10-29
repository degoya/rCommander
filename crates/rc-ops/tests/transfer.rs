//! Copy, move, delete and mkdir: recursively on the local disk, and between two in-memory
//! backends - proof that the core works through `Vfs` alone.

mod support;

use std::{fs, path::Path, time::SystemTime};

use rc_ops::{DeleteMode, JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{Vfs, VfsPath};
use support::{Harness, exists, mem, payload, read, set_mtime, write};

fn spec(kind: JobKind) -> JobSpec {
    JobSpec {
        kind,
        options: JobOptions::default(),
    }
}

/// `root/src`: two files at the top, a nested tree with an empty directory.
fn local_tree(root: &Path) {
    let src = root.join("src");
    fs::create_dir_all(src.join("deep").join("deeper")).expect("mkdir");
    fs::create_dir_all(src.join("empty")).expect("mkdir");
    fs::write(src.join("a.txt"), b"alpha").expect("write");
    fs::write(src.join("deep").join("b.bin"), payload(10_000, 1)).expect("write");
    fs::write(src.join("deep").join("deeper").join("c.txt"), b"gamma").expect("write");
}

#[tokio::test]
async fn copy_moves_a_tree_on_the_local_disk() {
    let root = tempfile::tempdir().expect("tempdir");
    local_tree(root.path());
    let harness = Harness::start();
    harness.local();
    let target = root.path().join("out");

    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![VfsPath::local(root.path().join("src"))],
            target: VfsPath::local(&target),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.progress.files_total, 3);
    assert_eq!(done.progress.files_done, 3);
    assert_eq!(done.progress.bytes_total, 10_010);
    assert_eq!(done.progress.bytes_done, 10_010);
    let copy = target.join("src");
    assert_eq!(fs::read(copy.join("a.txt")).expect("read"), b"alpha");
    assert_eq!(
        fs::read(copy.join("deep").join("b.bin")).expect("read"),
        payload(10_000, 1)
    );
    assert_eq!(
        fs::read(copy.join("deep").join("deeper").join("c.txt")).expect("read"),
        b"gamma"
    );
    assert!(copy.join("empty").is_dir(), "empty directories are copied");
    assert!(
        root.path().join("src").join("a.txt").exists(),
        "the source stays"
    );
}

#[tokio::test]
async fn copy_keeps_the_modification_time() {
    let root = tempfile::tempdir().expect("tempdir");
    local_tree(root.path());
    let source = root.path().join("src").join("a.txt");
    let when = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
    filetime_set(&source, when);
    let harness = Harness::start();
    harness.local();
    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![VfsPath::local(&source)],
            target: VfsPath::local(root.path().join("out")),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let copied = fs::metadata(root.path().join("out").join("a.txt")).expect("stat");
    assert_eq!(copied.modified().expect("mtime"), when);
}

fn filetime_set(path: &Path, when: SystemTime) {
    let file = fs::File::options().write(true).open(path).expect("open");
    file.set_modified(when).expect("set mtime");
}

#[tokio::test]
async fn move_renames_on_the_local_disk_and_leaves_no_source() {
    let root = tempfile::tempdir().expect("tempdir");
    local_tree(root.path());
    fs::create_dir(root.path().join("out")).expect("mkdir");
    let harness = Harness::start();
    harness.local();
    let done = harness
        .run(spec(JobKind::Move {
            sources: vec![VfsPath::local(root.path().join("src"))],
            target: VfsPath::local(root.path().join("out")),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(!root.path().join("src").exists(), "the source is gone");
    let moved = root.path().join("out").join("src");
    assert_eq!(fs::read(moved.join("a.txt")).expect("read"), b"alpha");
    assert_eq!(
        fs::read(moved.join("deep").join("deeper").join("c.txt")).expect("read"),
        b"gamma"
    );
    assert_eq!(
        done.progress.files_done, 3,
        "a renamed tree counts its files"
    );
}

#[tokio::test]
async fn delete_permanently_removes_a_tree_on_the_local_disk() {
    let root = tempfile::tempdir().expect("tempdir");
    local_tree(root.path());
    let harness = Harness::start();
    harness.local();
    let done = harness
        .run(spec(JobKind::Delete {
            sources: vec![VfsPath::local(root.path().join("src"))],
            mode: DeleteMode::Permanent,
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(!root.path().join("src").exists());
    assert_eq!(done.progress.files_done, 3);
}

#[tokio::test]
async fn mkdir_creates_parents_and_refuses_an_existing_directory() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let deep = root.path().join("x").join("y");
    let made = harness
        .run(spec(JobKind::Mkdir {
            path: VfsPath::local(&deep),
        }))
        .await;
    assert_eq!(made.state, JobState::Completed, "{made:?}");
    assert!(deep.is_dir());

    let again = harness
        .run(JobSpec {
            kind: JobKind::Mkdir {
                path: VfsPath::local(&deep),
            },
            options: JobOptions {
                on_error: rc_ops::ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(again.failures.len(), 1, "{again:?}");
    assert_eq!(again.failures[0].code, "already_exists");
}

#[tokio::test]
async fn trash_is_refused_where_the_backend_has_no_bin() {
    let harness = Harness::start();
    harness.memory(1);
    let refused = harness
        .queue
        .submit(spec(JobKind::Delete {
            sources: vec![mem(1, "/a")],
            mode: DeleteMode::Trash,
        }))
        .expect_err("no recycle bin");
    assert_eq!(refused.code(), "trash_unavailable");
}

/// The acceptance test for "the core only uses the trait": two in-memory backends, neither of
/// them local, and a tree streamed from one into the other.
#[tokio::test]
async fn copy_between_two_in_memory_backends() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/data/big.bin", &payload(100_000, 7)).await;
    write(&one, "/data/sub/small.txt", b"small").await;
    set_mtime(&one, "/data/sub/small.txt", 1_600_000_000).await;

    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![mem(1, "/data")],
            target: mem(2, "/backup"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        read(&two, "/backup/data/big.bin").await,
        payload(100_000, 7)
    );
    assert_eq!(read(&two, "/backup/data/sub/small.txt").await, b"small");
    let kept = two
        .stat(Path::new("/backup/data/sub/small.txt"))
        .await
        .expect("stat");
    assert_eq!(
        kept.modified,
        Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000))
    );
    assert!(
        exists(&one, "/data/big.bin").await,
        "a copy keeps the source"
    );
}

#[tokio::test]
async fn move_between_two_backends_copies_then_removes() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/data/a.txt", b"a").await;
    write(&one, "/data/sub/b.txt", b"b").await;
    let done = harness
        .run(spec(JobKind::Move {
            sources: vec![mem(1, "/data")],
            target: mem(2, "/"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&two, "/data/sub/b.txt").await, b"b");
    assert!(
        !exists(&one, "/data").await,
        "the emptied source tree is gone"
    );
}

#[tokio::test]
async fn a_directory_is_not_copied_into_itself() {
    let harness = Harness::start();
    let one = harness.memory(1);
    write(&one, "/data/a.txt", b"a").await;
    let inside = harness
        .queue
        .submit(spec(JobKind::Copy {
            sources: vec![mem(1, "/data")],
            target: mem(1, "/data/sub"),
        }))
        .expect_err("refused");
    assert_eq!(inside.code(), "target_inside_source");
    let onto = harness
        .queue
        .submit(spec(JobKind::Copy {
            sources: vec![mem(1, "/data/a.txt")],
            target: mem(1, "/data"),
        }))
        .expect_err("refused");
    assert_eq!(onto.code(), "same_location");
    let unknown = harness
        .queue
        .submit(spec(JobKind::Copy {
            sources: vec![mem(1, "/data/a.txt")],
            target: mem(9, "/"),
        }))
        .expect_err("refused");
    assert_eq!(unknown.code(), "backend_unavailable");
}

#[tokio::test]
async fn progress_events_end_at_the_totals() {
    let harness = Harness::start();
    let one = harness.memory(1);
    harness.memory(2);
    write(&one, "/f.bin", &payload(20_000, 3)).await;
    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![mem(1, "/f.bin")],
            target: mem(2, "/"),
        }))
        .await;
    let changes = harness.changes(done.id);
    let progress: Vec<u64> = changes
        .iter()
        .filter_map(|change| match change {
            rc_ops::JobChange::Progress { progress } => Some(progress.bytes_done),
            _ => None,
        })
        .collect();
    assert!(
        progress.len() > 3,
        "a 4 KiB buffer reports per buffer: {progress:?}"
    );
    assert!(
        progress.windows(2).all(|pair| pair[0] <= pair[1]),
        "{progress:?}"
    );
    assert_eq!(progress.last(), Some(&20_000));
    assert_eq!(changes.first(), Some(&rc_ops::JobChange::Queued));
    assert_eq!(
        changes.last(),
        Some(&rc_ops::JobChange::Finished {
            state: JobState::Completed
        })
    );
}

/// Total Commander's second bar (RC-010-07): the bytes of the file at work, from 0 at each file
/// up to its size.
#[tokio::test]
async fn each_file_reports_its_own_bytes_up_to_its_size() {
    let harness = Harness::start();
    let one = harness.memory(1);
    harness.memory(2);
    write(&one, "/d/a.bin", &payload(12_000, 1)).await;
    write(&one, "/d/b.bin", &payload(9_000, 2)).await;
    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![mem(1, "/d")],
            target: mem(2, "/"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let files: Vec<(String, u64, u64)> = harness
        .changes(done.id)
        .iter()
        .filter_map(|change| match change {
            rc_ops::JobChange::Progress { progress } => progress.current.as_ref().map(|at| {
                let name = at.path.to_string_lossy().into_owned();
                (name, progress.file_bytes_done, progress.file_bytes_total)
            }),
            _ => None,
        })
        .collect();
    for (name, size) in [("/d/a.bin", 12_000), ("/d/b.bin", 9_000)] {
        let own: Vec<u64> = files
            .iter()
            .filter(|(at, ..)| at == name)
            .map(|&(_, done, total)| {
                assert_eq!(total, size, "{name}: {files:?}");
                done
            })
            .collect();
        assert!(own.len() > 1, "{name}: {files:?}");
        assert!(own[0] < size, "{name} starts below its size: {own:?}");
        assert!(own.windows(2).all(|pair| pair[0] <= pair[1]), "{own:?}");
        assert_eq!(own.last(), Some(&size), "{name}: {own:?}");
    }
    let second = files
        .iter()
        .position(|(at, ..)| at == "/d/b.bin")
        .expect("the second file");
    assert!(
        files[..second].iter().all(|(at, ..)| at == "/d/a.bin"),
        "one file after the other: {files:?}"
    );
    assert_eq!(
        done.progress.file_bytes_total, 0,
        "nothing at work once over"
    );
}

#[tokio::test]
async fn delete_to_the_bin_moves_each_source_whole() {
    let harness = Harness::start();
    let one = rc_vfs::MemoryFs::new();
    harness.queue.register(
        rc_vfs::BackendId::Memory(1),
        rc_ops::Backend::new(std::sync::Arc::new(one.clone()))
            .with_trash(Some(std::sync::Arc::new(support::BinTrash(one.clone())))),
    );
    write(&one, "/tree/a.txt", b"a").await;
    write(&one, "/tree/sub/b.txt", b"b").await;
    write(&one, "/single.txt", b"s").await;
    let done = harness
        .run(spec(JobKind::Delete {
            sources: vec![mem(1, "/tree"), mem(1, "/single.txt")],
            mode: DeleteMode::Trash,
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(!exists(&one, "/tree").await);
    assert_eq!(
        read(&one, "/.bin/tree/sub/b.txt").await,
        b"b",
        "restorable whole"
    );
    assert_eq!(read(&one, "/.bin/single.txt").await, b"s");
    assert_eq!(done.progress.files_done, 3);
}
