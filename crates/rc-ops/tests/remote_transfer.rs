//! Copies and moves to and from a server (RC-060-05), against in-memory backends registered under
//! a server's id: the part file a copy is written under, a cancel and a failed check that leave
//! nothing under the target's name, a move that removes its source only after the size checks,
//! and the time stamps the option keeps.

mod support;

use std::{
    io,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use rc_ops::{
    Backend, ConflictChoice, ErrorPolicy, JobChange, JobEvent, JobId, JobKind, JobOptions, JobSpec,
    JobState, Queue,
};
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsPath, WriteMode, WriteStream};
use support::{Harness, exists, payload, read, set_mtime, write};
use tokio::io::AsyncWrite;

const SIZE: usize = 64 * 1024;
const SERVER: BackendId = BackendId::Sftp { connection_id: 1 };

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

fn here(path: &str) -> VfsPath {
    support::mem(2, path)
}

fn spec(kind: JobKind, options: JobOptions) -> JobSpec {
    JobSpec { kind, options }
}

fn upload(sources: &[&str], options: JobOptions) -> JobSpec {
    spec(
        JobKind::Copy {
            sources: sources.iter().map(|path| here(path)).collect(),
            target: on_server("/up"),
        },
        options,
    )
}

fn mid_file(event: &JobEvent) -> bool {
    matches!(
        &event.change,
        JobChange::Progress { progress }
            if progress.current.is_some()
                && progress.bytes_done > 0
                && progress.bytes_done < SIZE as u64
    )
}

/// Acts once, at the first progress event inside a file.
fn at_mid_file(harness: &Harness, act: impl Fn(&Queue, JobId) + Send + 'static) {
    let done = Arc::new(Mutex::new(false));
    harness.on_event(move |queue, event| {
        let mut done = done.lock().expect("done");
        if !*done && mid_file(event) {
            *done = true;
            act(queue, event.job);
        }
    });
}

async fn wait_for_state(queue: &Queue, id: JobId, state: JobState) {
    for _ in 0..400 {
        if queue.job(id).expect("known").state == state {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job {id} never reached {state:?}");
}

/// `Memory(2)` with `/big.bin` and `/tree/{a.txt, sub/b.bin}`, and an empty server.
async fn start() -> (Harness, MemoryFs, MemoryFs) {
    let harness = Harness::start();
    let local = harness.memory(2);
    let server = harness.memory_as(SERVER);
    write(&local, "/big.bin", &payload(SIZE, 5)).await;
    write(&local, "/tree/a.txt", b"alpha").await;
    write(&local, "/tree/sub/b.bin", &payload(9000, 3)).await;
    (harness, local, server)
}

#[tokio::test]
async fn an_upload_appears_under_its_name_only_when_complete() {
    let (harness, local, server) = start().await;
    at_mid_file(&harness, |queue, job| {
        queue.pause(job).expect("pause");
    });
    let id = harness
        .queue
        .submit(upload(&["/big.bin", "/tree"], JobOptions::default()))
        .expect("accepted")
        .id;
    wait_for_state(&harness.queue, id, JobState::Paused).await;
    assert!(exists(&server, "/up/big.bin.rcpart").await, "written aside");
    assert!(
        !exists(&server, "/up/big.bin").await,
        "nothing half-written"
    );

    harness.queue.resume(id).expect("resume");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&server, "/up/big.bin").await, payload(SIZE, 5));
    assert_eq!(read(&server, "/up/tree/a.txt").await, b"alpha");
    assert_eq!(read(&server, "/up/tree/sub/b.bin").await, payload(9000, 3));
    assert!(!exists(&server, "/up/big.bin.rcpart").await);
    assert!(exists(&local, "/big.bin").await, "a copy keeps its source");
}

#[tokio::test]
async fn a_download_brings_the_tree_back_unchanged() {
    let (harness, _local, server) = start().await;
    write(&server, "/srv/tree/a.txt", b"alpha").await;
    write(&server, "/srv/tree/sub/b.bin", &payload(SIZE, 7)).await;
    let back = harness.memory(3);
    let done = harness
        .run(spec(
            JobKind::Copy {
                sources: vec![on_server("/srv/tree")],
                target: support::mem(3, "/back"),
            },
            JobOptions::default(),
        ))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&back, "/back/tree/a.txt").await, b"alpha");
    assert_eq!(read(&back, "/back/tree/sub/b.bin").await, payload(SIZE, 7));
    assert!(
        !exists(&back, "/back/tree/sub/b.bin.rcpart").await,
        "no part file on a local target"
    );
}

#[tokio::test]
async fn a_cancel_inside_a_file_leaves_nothing_under_the_target_name() {
    let (harness, local, server) = start().await;
    at_mid_file(&harness, |queue, job| {
        queue.cancel(job).expect("cancel");
    });
    let done = harness
        .run(upload(&["/big.bin"], JobOptions::default()))
        .await;
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert!(!exists(&server, "/up/big.bin").await);
    assert!(!exists(&server, "/up/big.bin.rcpart").await, "removed");
    assert!(exists(&local, "/big.bin").await);
}

#[tokio::test]
async fn a_cancelled_overwrite_keeps_the_old_file_on_the_server() {
    let (harness, _local, server) = start().await;
    write(&server, "/up/big.bin", b"the old one").await;
    at_mid_file(&harness, |queue, job| {
        queue.cancel(job).expect("cancel");
    });
    let options = JobOptions {
        on_conflict: Some(ConflictChoice::Overwrite),
        ..JobOptions::default()
    };
    let done = harness.run(upload(&["/big.bin"], options)).await;
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert_eq!(read(&server, "/up/big.bin").await, b"the old one");

    let done = harness.run(upload(&["/big.bin"], options)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&server, "/up/big.bin").await, payload(SIZE, 5));
}

#[tokio::test]
async fn a_move_removes_each_source_after_its_copy() {
    let (harness, local, server) = start().await;
    let done = harness
        .run(spec(
            JobKind::Move {
                sources: vec![here("/tree")],
                target: on_server("/up"),
            },
            JobOptions::default(),
        ))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&server, "/up/tree/sub/b.bin").await, payload(9000, 3));
    assert!(!exists(&local, "/tree").await, "the emptied source is gone");
}

#[tokio::test]
async fn a_move_whose_copy_comes_up_short_keeps_its_source() {
    let harness = Harness::start();
    let local = harness.memory(2);
    let target = MemoryFs::new();
    harness
        .queue
        .register(SERVER, Backend::new(Arc::new(Short(target.clone()))));
    write(&local, "/big.bin", &payload(SIZE, 5)).await;
    let done = harness
        .run(spec(
            JobKind::Move {
                sources: vec![here("/big.bin")],
                target: on_server("/up"),
            },
            JobOptions {
                on_error: ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        ))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let codes: Vec<&str> = done.failures.iter().map(|f| f.code.as_str()).collect();
    assert_eq!(codes, ["verify_failed"]);
    assert_eq!(read(&local, "/big.bin").await, payload(SIZE, 5), "kept");
    assert!(!exists(&target, "/up/big.bin").await);
    assert!(!exists(&target, "/up/big.bin.rcpart").await);
}

#[tokio::test]
async fn times_are_kept_only_with_the_option() {
    let (harness, local, server) = start().await;
    set_mtime(&local, "/tree/a.txt", 1_600_000_000).await;
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    for (keep, dir) in [(true, "/kept"), (false, "/fresh")] {
        let done = harness
            .run(spec(
                JobKind::Copy {
                    sources: vec![here("/tree/a.txt")],
                    target: on_server(dir),
                },
                JobOptions {
                    keep_attributes: keep,
                    ..JobOptions::default()
                },
            ))
            .await;
        assert_eq!(done.state, JobState::Completed, "{done:?}");
        let copied = server
            .stat(&Path::new(dir).join("a.txt"))
            .await
            .expect("copied");
        assert_eq!(copied.modified == Some(when), keep, "{dir}");
    }
}

/// A server that stores only the first half of each write while it reports all of it written.
struct Short(MemoryFs);

struct ShortWriter(WriteStream);

impl AsyncWrite for ShortWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let half = bytes.len().div_ceil(2);
        match Pin::new(&mut self.0).poll_write(cx, &bytes[..half]) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(bytes.len())),
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Short {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        Ok(Box::new(ShortWriter(self.0.open_write(path, mode).await?)))
    }
}

/// RC-080-12 (X7): a part file is created anew, never truncated - a link a server put under the
/// part's name is removed, and the file it leads to stays as it was.
#[cfg(unix)]
#[tokio::test]
async fn a_link_under_the_part_name_is_removed_not_written_through() {
    let harness = Harness::start();
    let local = harness.memory(2);
    write(&local, "/big.bin", &payload(SIZE, 5)).await;
    harness
        .queue
        .register(SERVER, Backend::new(Arc::new(rc_vfs::LocalFs::new())));
    let dir = tempfile::tempdir().expect("temp dir");
    let up = dir.path().join("up");
    std::fs::create_dir(&up).expect("mkdir");
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, b"keep me").expect("write");
    std::os::unix::fs::symlink(&victim, up.join("big.bin.rcpart")).expect("link");
    let done = harness
        .run(spec(
            JobKind::Copy {
                sources: vec![here("/big.bin")],
                target: VfsPath {
                    backend: SERVER,
                    path: up.clone(),
                },
            },
            JobOptions {
                on_conflict: Some(ConflictChoice::Overwrite),
                ..JobOptions::default()
            },
        ))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(std::fs::read(&victim).expect("read"), b"keep me");
    assert_eq!(
        std::fs::read(up.join("big.bin")).expect("read"),
        payload(SIZE, 5)
    );
    assert!(!up.join("big.bin.rcpart").exists());
}
