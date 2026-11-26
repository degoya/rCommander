//! RC-110-65: a copy from disk to disk through the operating system's own call lands as the
//! stream lands - the contents, the modification time, no temporary file - and a cancel in its
//! middle leaves no half file, a pause holds it.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use rc_ops::{Backend, JobChange, JobId, JobKind, JobOptions, JobSpec, JobState, Queue};
use rc_vfs::{BackendId, CopyWatch, LocalFs, Vfs, VfsError, VfsPath};

use crate::support::{Harness, payload};

/// Above the size from which a file takes the system's copy (1 MiB).
const SIZE: usize = 3 << 20;

/// `root/src/big.bin` of [`SIZE`] bytes with an old modification time.
fn source(root: &Path) -> PathBuf {
    let src = root.join("src");
    fs::create_dir_all(&src).expect("mkdir");
    let file = src.join("big.bin");
    fs::write(&file, payload(SIZE, 4)).expect("write");
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    fs::File::options()
        .write(true)
        .open(&file)
        .expect("open")
        .set_modified(when)
        .expect("set mtime");
    file
}

fn copy(source: &Path, target: &Path, verify: bool) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![VfsPath::local(source)],
            target: VfsPath::local(target),
        },
        options: JobOptions {
            verify,
            ..JobOptions::default()
        },
    }
}

/// The names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The bytes done of the last progress event.
fn last_bytes(harness: &Harness, job: JobId) -> Option<u64> {
    harness
        .changes(job)
        .iter()
        .rev()
        .find_map(|change| match change {
            JobChange::Progress { progress } => Some(progress.bytes_done),
            _ => None,
        })
}

#[tokio::test]
async fn both_ways_land_the_same_file() {
    let root = tempfile::tempdir().expect("tempdir");
    let file = source(root.path());
    let harness = Harness::start();
    harness.local();
    // A job that verifies streams; one that does not takes the system's copy.
    for (dir, verify) in [("streamed", true), ("system", false)] {
        let id = harness
            .queue
            .submit(copy(&file, &root.path().join(dir), verify))
            .expect("accepted")
            .id;
        let done = harness.finish(id).await;
        assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
        assert_eq!(
            names(&root.path().join(dir)),
            ["big.bin"],
            "no temporary file"
        );
        assert_eq!(
            last_bytes(&harness, id),
            Some(SIZE as u64),
            "the progress ends whole"
        );
    }
    let landed = |dir: &str| {
        let path = root.path().join(dir).join("big.bin");
        let meta = fs::metadata(&path).expect("stat");
        (
            fs::read(&path).expect("read"),
            meta.modified().expect("mtime"),
        )
    };
    let (streamed, system) = (landed("streamed"), landed("system"));
    assert_eq!(system.0, payload(SIZE, 4));
    assert_eq!(system, streamed, "the same contents and modification time");
}

/// The local disk whose system copy stops halfway: half the file written and reported, then it
/// waits until the job pauses and goes on (the rest) or cancels (nothing left).
#[derive(Default)]
struct Halfway(LocalFs);

#[async_trait]
impl rc_vfs::testing::Forward for Halfway {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        watch: Arc<CopyWatch>,
    ) -> rc_vfs::Result<u64> {
        // Its own task, as the system's copy runs on its own thread: it goes on while the job
        // waits in its pause and does not poll it.
        let (from, to) = (from.to_path_buf(), to.to_path_buf());
        tokio::spawn(async move {
            let bytes = fs::read(&from).map_err(|error| VfsError::from_io(error, &from))?;
            let half = bytes.len() / 2;
            fs::write(&to, &bytes[..half]).map_err(|error| VfsError::from_io(error, &to))?;
            watch.report(half as u64);
            let mut held = false;
            loop {
                if watch.is_cancelled() {
                    let _ = fs::remove_file(&to);
                    return Err(VfsError::Cancelled { path: from });
                }
                match (held, watch.is_paused()) {
                    (false, true) => held = true,
                    (true, false) => break,
                    _ => {}
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            fs::write(&to, &bytes).map_err(|error| VfsError::from_io(error, &to))?;
            watch.report(bytes.len() as u64);
            Ok(bytes.len() as u64)
        })
        .await
        .expect("the copy task")
    }
}

/// A harness whose local disk is [`Halfway`]; `act` runs once, at the first progress event in
/// the middle of the file, and the bytes done then are kept.
fn halfway(act: impl Fn(&Queue, JobId) + Send + 'static) -> (Harness, Arc<Mutex<Option<u64>>>) {
    let harness = Harness::start();
    harness
        .queue
        .register(BackendId::Local, Backend::new(Arc::new(Halfway::default())));
    let seen: Arc<Mutex<Option<u64>>> = Arc::default();
    let record = Arc::clone(&seen);
    harness.on_event(move |queue, event| {
        let mut seen = record.lock().expect("seen");
        if let JobChange::Progress { progress } = &event.change
            && seen.is_none()
            && progress.bytes_done > 0
            && progress.bytes_done < SIZE as u64
        {
            *seen = Some(progress.bytes_done);
            act(queue, event.job);
        }
    });
    (harness, seen)
}

#[tokio::test]
async fn a_cancel_in_the_middle_leaves_no_half_file() {
    let root = tempfile::tempdir().expect("tempdir");
    let file = source(root.path());
    let (harness, seen) = halfway(|queue, job| {
        queue.cancel(job).expect("cancel");
    });
    let target = root.path().join("out");
    let done = harness.run(copy(&file, &target, false)).await;
    assert_eq!(*seen.lock().expect("seen"), Some(SIZE as u64 / 2));
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert!(names(&target).is_empty(), "no half file, no temporary file");
    assert!(file.exists(), "the source stays");
}

#[tokio::test]
async fn a_pause_holds_the_system_copy_and_resume_finishes_it() {
    let root = tempfile::tempdir().expect("tempdir");
    let file = source(root.path());
    let (harness, seen) = halfway(|queue, job| {
        queue.pause(job).expect("pause");
    });
    let target = root.path().join("out");
    let id = harness
        .queue
        .submit(copy(&file, &target, false))
        .expect("accepted")
        .id;
    for _ in 0..400 {
        if harness.queue.job(id).expect("known").state == JobState::Paused {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let snapshot = harness.queue.job(id).expect("known");
    assert_eq!(snapshot.state, JobState::Paused);
    assert_eq!(*seen.lock().expect("seen"), Some(SIZE as u64 / 2));
    assert_eq!(
        snapshot.progress.bytes_done,
        SIZE as u64 / 2,
        "nothing moves while paused"
    );
    assert!(!target.join("big.bin").exists(), "not under its name yet");
    harness.queue.resume(id).expect("resume");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    assert_eq!(names(&target), ["big.bin"]);
    assert_eq!(
        fs::read(target.join("big.bin")).expect("read"),
        payload(SIZE, 4)
    );
}

/// RC-110-56: the Mark-of-the-Web of a file copied through the system is its source's, byte for
/// byte, also of a read-only file - whose copy is read-only again.
#[cfg(windows)]
#[tokio::test]
async fn the_zone_of_a_system_copy_is_its_sources() {
    let root = tempfile::tempdir().expect("tempdir");
    let file = source(root.path());
    let stream = |file: &Path| {
        let mut name = file.as_os_str().to_os_string();
        name.push(":Zone.Identifier");
        PathBuf::from(name)
    };
    let zone = b"[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/big.bin\r\n";
    fs::write(stream(&file), zone).expect("zone");
    let mut permissions = fs::metadata(&file).expect("stat").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&file, permissions).expect("read-only");
    let harness = Harness::start();
    harness.local();
    let target = root.path().join("out");
    let done = harness.run(copy(&file, &target, false)).await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    let copied = target.join("big.bin");
    assert_eq!(fs::read(stream(&copied)).expect("the copy's zone"), zone);
    assert!(
        fs::metadata(&copied)
            .expect("stat")
            .permissions()
            .readonly()
    );
}
