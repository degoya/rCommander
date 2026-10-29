//! F5 and F6 from one server to another, end to end against real servers in this process
//! (RC-060-06): a folder between two SFTP servers byte for byte, with nothing on this machine's
//! disk on the way; a copy within one server over its one connection; FTP to SFTP, SFTP to FTP,
//! and two copies at once within one FTP server, which hold two of its connections each.

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

use rc_ops::{Backend, JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, VfsPath};
use support::{
    Harness, payload,
    servers::{Ftp, Sftp},
};

const ONE: BackendId = BackendId::Sftp { connection_id: 1 };
const TWO: BackendId = BackendId::Sftp { connection_id: 2 };
const FTP: BackendId = BackendId::Ftp { connection_id: 3 };
const BIG: usize = 1 << 20;

fn at(backend: &BackendId, path: &str) -> VfsPath {
    VfsPath {
        backend: backend.clone(),
        path: path.into(),
    }
}

fn copy(source: VfsPath, target: VfsPath) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![source],
            target,
        },
        options: JobOptions::default(),
    }
}

/// Every file below `dir`, relative, with its checksum.
fn files(dir: &Path) -> Vec<(PathBuf, blake3::Hash)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).expect("listable") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(dir).expect("below").to_path_buf();
                found.push((rel, blake3::hash(&fs::read(&path).expect("readable"))));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// A folder with a big file, a nested one, an empty folder and an old time on `a.txt`.
fn make_tree(root: &Path, marker: &str) -> SystemTime {
    fs::create_dir_all(root.join("sub/deeper")).expect("mkdir");
    fs::create_dir_all(root.join("empty")).expect("mkdir");
    fs::write(root.join("a.txt"), b"alpha").expect("write");
    fs::write(root.join(format!("sub/{marker}.bin")), payload(BIG, 3)).expect("write");
    fs::write(root.join("sub/deeper/c.bin"), payload(7000, 9)).expect("write");
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    fs::File::options()
        .write(true)
        .open(root.join("a.txt"))
        .and_then(|file| file.set_modified(when))
        .expect("set mtime");
    when
}

/// Every path in the system's temporary directory, two levels deep, whose name holds `marker`.
fn in_temp(marker: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![(std::env::temp_dir(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().contains(marker) {
                found.push(path.clone());
            }
            if depth < 2 && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                dirs.push((path, depth + 1));
            }
        }
    }
    found
}

#[tokio::test]
async fn a_folder_goes_from_one_server_to_another_byte_for_byte_past_this_disk() {
    let (one, two) = (Sftp::start().await, Sftp::start().await);
    let harness = Harness::start();
    // No local backend at all: the copy has nowhere on this machine to write.
    harness.queue.register(ONE, Backend::new(one.vfs.clone()));
    harness.queue.register(TWO, Backend::new(two.vfs.clone()));
    let marker = format!("rc-remote-remote-{}", std::process::id());
    let when = make_tree(&one.on_disk("/src"), &marker);
    assert!(in_temp(&marker).is_empty());

    let id = harness
        .queue
        .submit(copy(at(&ONE, "/src"), at(&TWO, "/in")))
        .expect("accepted")
        .id;
    // Watch the temporary directory while the job runs.
    let running = Arc::new(AtomicBool::new(true));
    let watcher = {
        let (running, marker) = (Arc::clone(&running), marker.clone());
        tokio::spawn(async move {
            let mut seen = Vec::new();
            while running.load(Ordering::SeqCst) {
                seen.extend(in_temp(&marker));
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            seen
        })
    };
    let done = harness.finish(id).await;
    running.store(false, Ordering::SeqCst);
    let seen = watcher.await.expect("the watcher ends");

    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(files(&two.on_disk("/in/src")), files(&one.on_disk("/src")));
    assert!(two.on_disk("/in/src/empty").is_dir());
    let kept = fs::metadata(two.on_disk("/in/src/a.txt")).and_then(|meta| meta.modified());
    assert_eq!(kept.expect("a time"), when, "the time went along");
    assert!(
        !two.on_disk(&format!("/in/src/sub/{marker}.bin.rcpart"))
            .exists()
    );
    assert!(
        seen.is_empty(),
        "nothing in the temporary directory: {seen:?}"
    );
    assert!(in_temp(&marker).is_empty());
}

#[tokio::test]
async fn a_move_between_two_servers_removes_the_source_once_it_is_there() {
    let (one, two) = (Sftp::start().await, Sftp::start().await);
    let harness = Harness::start();
    harness.queue.register(ONE, Backend::new(one.vfs.clone()));
    harness.queue.register(TWO, Backend::new(two.vfs.clone()));
    fs::create_dir_all(one.on_disk("/out")).expect("mkdir");
    fs::write(one.on_disk("/out/b.bin"), payload(BIG, 7)).expect("write");
    let done = harness
        .run(JobSpec {
            kind: JobKind::Move {
                sources: vec![at(&ONE, "/out/b.bin")],
                target: at(&TWO, "/in"),
            },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        fs::read(two.on_disk("/in/b.bin")).expect("read"),
        payload(BIG, 7)
    );
    assert!(!one.on_disk("/out/b.bin").exists(), "the source went");
}

#[tokio::test]
async fn a_copy_within_one_server_needs_no_second_connection() {
    let one = Sftp::start().await;
    let harness = Harness::start();
    // Both panels on one server: one backend, one session.
    harness.queue.register(ONE, Backend::new(one.vfs.clone()));
    fs::create_dir_all(one.on_disk("/a/sub")).expect("mkdir");
    fs::write(one.on_disk("/a/sub/big.bin"), payload(BIG, 5)).expect("write");
    assert_eq!(one.server.accepts(), 1);

    let done = harness.run(copy(at(&ONE, "/a"), at(&ONE, "/b"))).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(files(&one.on_disk("/b/a")), files(&one.on_disk("/a")));
    assert_eq!(one.server.accepts(), 1, "no second TCP connection");
}

#[tokio::test]
async fn ftp_and_sftp_copy_into_each_other() {
    let (sftp, ftp) = (Sftp::start().await, Ftp::start().await);
    let harness = Harness::start();
    harness.queue.register(ONE, Backend::new(sftp.vfs.clone()));
    harness
        .queue
        .register(FTP, Backend::new(Arc::new(ftp.vfs.clone())));
    fs::create_dir_all(ftp.on_disk("/pub")).expect("mkdir");
    fs::write(ftp.on_disk("/pub/f.bin"), payload(BIG, 11)).expect("write");

    let down = harness
        .run(copy(at(&FTP, "/pub"), at(&ONE, "/from-ftp")))
        .await;
    assert_eq!(down.state, JobState::Completed, "{down:?}");
    assert_eq!(
        fs::read(sftp.on_disk("/from-ftp/pub/f.bin")).expect("read"),
        payload(BIG, 11)
    );

    let up = harness
        .run(copy(at(&ONE, "/from-ftp/pub/f.bin"), at(&FTP, "/back")))
        .await;
    assert_eq!(up.state, JobState::Completed, "{up:?}");
    assert_eq!(
        fs::read(ftp.on_disk("/back/f.bin")).expect("read"),
        payload(BIG, 11)
    );
}

#[tokio::test]
async fn two_copies_within_one_ftp_server_at_once_both_finish() {
    let ftp = Ftp::start().await;
    let harness = Harness::start();
    harness
        .queue
        .register(FTP, Backend::new(Arc::new(ftp.vfs.clone())));
    for dir in ["a", "b"] {
        fs::create_dir_all(ftp.on_disk(&format!("/{dir}"))).expect("mkdir");
        for n in 0..3_u8 {
            let file = ftp.on_disk(&format!("/{dir}/{n}.bin"));
            fs::write(file, payload(256 * 1024, n)).expect("write");
        }
    }
    // Two jobs in the server's lane at once, each needing two of its two transfer connections.
    let first = harness
        .queue
        .submit(copy(at(&FTP, "/a"), at(&FTP, "/x")))
        .expect("accepted")
        .id;
    let second = harness
        .queue
        .submit(copy(at(&FTP, "/b"), at(&FTP, "/y")))
        .expect("accepted")
        .id;
    let (first, second) = (harness.finish(first).await, harness.finish(second).await);
    assert_eq!(first.state, JobState::Completed, "{first:?}");
    assert_eq!(second.state, JobState::Completed, "{second:?}");
    for n in 0..3_u8 {
        let x = fs::read(ftp.on_disk(&format!("/x/a/{n}.bin"))).expect("read");
        assert_eq!(x, payload(256 * 1024, n));
        let y = fs::read(ftp.on_disk(&format!("/y/b/{n}.bin"))).expect("read");
        assert_eq!(y, payload(256 * 1024, n));
    }
}
