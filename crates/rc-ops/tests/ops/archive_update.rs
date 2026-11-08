//! Changing an archive (RC-110-05) through the queue: entries added and removed, and the safety
//! frame - a cancel, a failure, a crash, a change by someone else, too little room, encrypted
//! and compressed archives - each leaving the old archive byte for byte and no temporary file.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::support::{Harness, flaky::wait_until, payload, slow::Slow, write};
use rc_ops::{
    Answer, Backend, ConflictChoice, ErrorPolicy, JobChange, JobKind, JobOptions, JobSpec,
    JobState, UpdateSettings,
};
use rc_vfs::{ArchiveFs, ArchiveSettings, BackendId, LocalFs, MemoryFs, Vfs, VfsPath};
use tokio::io::AsyncReadExt;
use zip::{ZipWriter, write::SimpleFileOptions};

/// A zip of `a.txt` and `b.txt`.
fn small_zip(path: &Path) {
    let mut zip = ZipWriter::new(fs::File::create(path).expect("create"));
    for (name, text) in [("a.txt", b"alpha"), ("b.txt", b"bravo")] {
        zip.start_file(name, SimpleFileOptions::default())
            .expect("start");
        zip.write_all(text).expect("write");
    }
    zip.finish().expect("finish");
}

fn update(archive: &Path, add: Vec<VfsPath>, under: &str, remove: &[u64]) -> JobSpec {
    JobSpec {
        kind: JobKind::ArchiveUpdate {
            archive: VfsPath::local(archive),
            add,
            under: under.to_owned(),
            remove: remove.to_vec(),
            settings: UpdateSettings::default(),
        },
        options: JobOptions {
            on_error: ErrorPolicy::Cancel,
            ..JobOptions::default()
        },
    }
}

/// The files left in `dir` besides `keep`.
fn others(dir: &Path, keep: &str) -> Vec<String> {
    fs::read_dir(dir)
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name != keep)
        .collect()
}

async fn inside(archive: &Path, inner: &str) -> Vec<u8> {
    let outer: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let fs = ArchiveFs::open(outer, archive, ArchiveSettings::default())
        .await
        .expect("open")
        .expect("an archive");
    let mut stream = fs.open_read(Path::new(inner)).await.expect(inner);
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.expect("read");
    bytes
}

/// A source of `len` bytes on a backend that reads it at 256 KiB a second, as `Memory(7)`.
async fn slow_source(harness: &Harness, len: usize) -> VfsPath {
    let server = Slow {
        fs: MemoryFs::new(),
        rate: 256 * 1024,
    };
    write(&server.fs, "/big.bin", &payload(len, 4)).await;
    harness
        .queue
        .register(BackendId::Memory(7), Backend::new(Arc::new(server)));
    VfsPath {
        backend: BackendId::Memory(7),
        path: PathBuf::from("/big.bin"),
    }
}

/// Whether a temporary file of an update lies in `dir`.
fn has_part(dir: &Path) -> bool {
    others(dir, "").iter().any(|name| name.ends_with(".rcpart"))
}

#[tokio::test]
async fn files_go_into_a_folder_and_entries_out() {
    let dir = tempfile::tempdir().expect("tempdir");
    let archive = dir.path().join("a.zip");
    small_zip(&archive);
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("deep")).expect("mkdir");
    fs::write(src.join("deep/c.txt"), b"charlie").expect("write");
    let harness = Harness::start();
    harness.local();
    let done = harness
        .run(update(&archive, vec![VfsPath::local(&src)], "/sub", &[0]))
        .await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    assert_eq!(inside(&archive, "/sub/src/deep/c.txt").await, b"charlie");
    assert_eq!(inside(&archive, "/b.txt").await, b"bravo");
    let outer: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let fs = ArchiveFs::open(outer, &archive, ArchiveSettings::default())
        .await
        .expect("open")
        .expect("a zip");
    assert!(fs.stat(Path::new("/a.txt")).await.is_err(), "a.txt is gone");
    assert!(others(dir.path(), "a.zip").iter().all(|name| name == "src"));
}

/// Acceptance: a cancel in the middle of the writing leaves the old archive byte for byte and
/// no temporary file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_leaves_the_old_archive_whole() {
    let dir = tempfile::tempdir().expect("tempdir");
    let archive = dir.path().join("a.zip");
    small_zip(&archive);
    let before = fs::read(&archive).expect("read");
    let harness = Harness::start();
    harness.local();
    let source = slow_source(&harness, 2 << 20).await;
    let id = harness
        .queue
        .submit(update(&archive, vec![source], "", &[]))
        .expect("accepted")
        .id;
    let path = dir.path().to_path_buf();
    wait_until("the new archive is being written", || has_part(&path)).await;
    harness.queue.cancel(id).expect("cancelled");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Cancelled);
    assert!(fs::read(&archive).expect("read") == before);
    assert!(!has_part(dir.path()), "{:?}", others(dir.path(), "a.zip"));
}

/// Acceptance: a failure in the middle of the writing - a source gone by the time it is read -
/// leaves the old archive byte for byte and no temporary file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_leaves_the_old_archive_whole() {
    let dir = tempfile::tempdir().expect("tempdir");
    let archive = dir.path().join("a.zip");
    small_zip(&archive);
    let before = fs::read(&archive).expect("read");
    let harness = Harness::start();
    harness.local();
    let slow = Slow {
        fs: MemoryFs::new(),
        rate: 256 * 1024,
    };
    write(&slow.fs, "/first.bin", &payload(1 << 20, 4)).await;
    write(&slow.fs, "/second.bin", b"gone soon").await;
    harness
        .queue
        .register(BackendId::Memory(7), Backend::new(Arc::new(slow.clone())));
    let at = |name: &str| VfsPath {
        backend: BackendId::Memory(7),
        path: PathBuf::from(name),
    };
    let id = harness
        .queue
        .submit(update(
            &archive,
            vec![at("/first.bin"), at("/second.bin")],
            "",
            &[],
        ))
        .expect("accepted")
        .id;
    let path = dir.path().to_path_buf();
    wait_until("the new archive is being written", || has_part(&path)).await;
    slow.fs
        .remove_file(Path::new("/second.bin"))
        .await
        .expect("removed");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Failed, "{:?}", done.failures);
    assert!(fs::read(&archive).expect("read") == before);
    assert!(!has_part(dir.path()), "{:?}", others(dir.path(), "a.zip"));
}

/// Acceptance: the archive changed by someone else while the update ran - the new archive is
/// thrown away with `archive_changed`, the changed one stays.
#[tokio::test]
async fn an_archive_changed_meanwhile_is_left_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let archive = dir.path().join("a.zip");
    small_zip(&archive);
    let src = dir.path().join("a.txt");
    fs::write(&src, b"newer").expect("write");
    let harness = Harness::start();
    harness.local();
    let changed = archive.clone();
    harness.on_event(move |queue, event| {
        if let JobChange::Question { .. } = event.change {
            // Someone touches the archive while the question waits.
            let file = fs::File::options()
                .write(true)
                .open(&changed)
                .expect("open");
            file.set_modified(SystemTime::now() + Duration::from_secs(3600))
                .expect("touch");
            let answer = Answer::Conflict {
                choice: ConflictChoice::Overwrite,
                for_all: false,
            };
            queue.answer(event.job, answer).expect("answered");
        }
    });
    let before = fs::read(&archive).expect("read");
    let done = harness
        .run(update(&archive, vec![VfsPath::local(&src)], "", &[]))
        .await;
    assert_eq!(done.state, JobState::Failed);
    assert_eq!(done.failures[0].code, "archive_changed");
    assert!(fs::read(&archive).expect("read") == before);
    assert!(!has_part(dir.path()));
}

/// Acceptance: too little room for the new archive beside the old one - `not_enough_space`, and
/// not a byte written.
#[tokio::test]
async fn too_little_room_writes_nothing() {
    let harness = Harness::start();
    let bytes = {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a.zip");
        small_zip(&path);
        fs::read(&path).expect("read")
    };
    let disk = MemoryFs::with_capacity(bytes.len() as u64 + 64 * 1024);
    write(&disk, "/data/a.zip", &bytes).await;
    harness
        .queue
        .register(BackendId::Local, Backend::new(Arc::new(disk.clone())));
    let source = harness.memory(1);
    write(&source, "/big.bin", &payload(256 * 1024, 1)).await;
    let add = vec![VfsPath {
        backend: BackendId::Memory(1),
        path: PathBuf::from("/big.bin"),
    }];
    let done = harness
        .run(update(Path::new("/data/a.zip"), add, "", &[]))
        .await;
    assert_eq!(done.state, JobState::Failed);
    assert_eq!(done.failures[0].code, "not_enough_space");
    let left: Vec<String> = disk
        .list(Path::new("/data"))
        .await
        .expect("list")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(left, ["a.zip"]);
    assert!(crate::support::read(&disk, "/data/a.zip").await == bytes);
}

/// Acceptance: encrypted zip and 7z archives are refused, and compressed tars as Total Commander
/// refuses them; each stays as it was.
#[tokio::test]
async fn encrypted_and_compressed_archives_are_refused() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../rc-vfs/tests/fixtures/archives");
    let harness = Harness::start();
    harness.local();
    for (name, code) in [
        ("encrypted-zipcrypto.zip", "archive_encrypted"),
        ("encrypted.7z", "archive_encrypted"),
        ("encrypted-header.7z", "archive_encrypted"),
        ("tree.tar.gz", "archive_not_modifiable"),
        ("single.txt.gz", "archive_not_modifiable"),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let archive = dir.path().join(name);
        fs::copy(fixtures.join(name), &archive).expect("copy");
        let before = fs::read(&archive).expect("read");
        let done = harness.run(update(&archive, Vec::new(), "", &[0])).await;
        assert_eq!(done.state, JobState::Failed, "{name}");
        assert_eq!(done.failures[0].code, code, "{name}");
        assert!(fs::read(&archive).expect("read") == before, "{name}");
        assert!(!has_part(dir.path()), "{name}");
    }
}

/// Only an archive on a local drive is changed: one inside an archive or on a server is refused
/// when the job is submitted.
#[tokio::test]
async fn an_archive_elsewhere_is_refused_at_once() {
    let harness = Harness::start();
    harness.memory(1);
    let spec = JobSpec {
        kind: JobKind::ArchiveUpdate {
            archive: VfsPath {
                backend: BackendId::Memory(1),
                path: PathBuf::from("/a.zip"),
            },
            add: Vec::new(),
            under: String::new(),
            remove: vec![0],
            settings: UpdateSettings::default(),
        },
        options: JobOptions::default(),
    };
    let error = harness.queue.submit(spec).expect_err("refused");
    assert_eq!(error.code(), "archive_not_local");
}

/// A read-only archive file is not replaced.
#[cfg(unix)]
#[tokio::test]
async fn a_read_only_archive_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let archive = dir.path().join("a.zip");
    small_zip(&archive);
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o444)).expect("chmod");
    let harness = Harness::start();
    harness.local();
    let done = harness.run(update(&archive, Vec::new(), "", &[0])).await;
    assert_eq!(done.failures[0].code, "archive_file_read_only");
}

/// Acceptance: the process ends in the middle of the writing (its runtime shut down, nothing
/// finishes or cleans up) - the old archive stays byte for byte, beside it at most the
/// temporary file.
#[test]
fn a_crash_leaves_the_old_archive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let archive = dir.path().join("a.zip");
    small_zip(&archive);
    let before = fs::read(&archive).expect("read");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime");
    let path = dir.path().to_path_buf();
    let harness = runtime.block_on(async {
        let harness = Harness::start();
        harness.local();
        let source = slow_source(&harness, 4 << 20).await;
        harness
            .queue
            .submit(update(&archive, vec![source], "", &[]))
            .expect("accepted");
        wait_until("the new archive is being written", || has_part(&path)).await;
        harness
    });
    runtime.shutdown_background();
    assert!(fs::read(&archive).expect("read") == before);
    let left = others(dir.path(), "a.zip");
    assert!(
        left.len() <= 1 && left.iter().all(|name| name.ends_with(".rcpart")),
        "{left:?}"
    );
    drop(harness);
}
