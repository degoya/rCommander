//! Failing files: a full disk, no access, a copy that does not verify, a move whose source
//! stream ends early - asked, skipped, retried or fatal.

mod support;

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use rc_ops::{
    Answer, Backend, ErrorChoice, ErrorPolicy, JobChange, JobKind, JobOptions, JobSpec, JobState,
    Question,
};
use rc_vfs::{BackendId, MemoryFs, ReadStream, SetAttributes, Vfs};
use support::{Corrupting, Harness, LIES, Lying, exists, mem, payload, read, write};
use tokio::io::AsyncReadExt;

fn copy(sources: &[&str], target: &str, options: JobOptions) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: sources.iter().map(|path| mem(1, path)).collect(),
            target: mem(2, target),
        },
        options,
    }
}

/// `Memory(2)` with room for 10 000 bytes.
fn small_disk(harness: &Harness) -> MemoryFs {
    let small = MemoryFs::with_capacity(10_000);
    harness
        .queue
        .register(BackendId::Memory(2), Backend::new(Arc::new(small.clone())));
    small
}

#[tokio::test]
async fn a_full_disk_asks_and_the_job_goes_on_after_skip() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let small = small_disk(&harness);
    write(&one, "/a.bin", &payload(8_000, 1)).await;
    write(&one, "/b.bin", &payload(8_000, 2)).await;
    write(&one, "/c.txt", b"fits").await;
    harness.on_event(|queue, event| {
        if let JobChange::Question { question } = &event.change {
            let Question::Error(failure) = question else {
                panic!("an error question: {question:?}");
            };
            assert_eq!(failure.code, "disk_full");
            queue
                .answer(
                    event.job,
                    Answer::Error {
                        choice: ErrorChoice::Skip,
                    },
                )
                .expect("answered");
        }
    });
    let done = harness
        .run(copy(
            &["/a.bin", "/b.bin", "/c.txt"],
            "/",
            JobOptions::default(),
        ))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.failures.len(), 1);
    assert_eq!(done.failures[0].code, "disk_full");
    assert_eq!(read(&small, "/a.bin").await, payload(8_000, 1));
    assert!(
        !exists(&small, "/b.bin").await,
        "the partial file is removed"
    );
    assert_eq!(read(&small, "/c.txt").await, b"fits", "the job went on");
    assert_eq!(done.progress.files_done, 3);
}

#[tokio::test]
async fn retry_after_making_room_completes_the_file() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let small = small_disk(&harness);
    write(&small, "/filler", &payload(6_000, 0)).await;
    write(&one, "/a.bin", &payload(8_000, 1)).await;
    let disk = small.clone();
    harness.on_event(move |queue, event| {
        if let JobChange::Question { .. } = event.change {
            let (queue, disk, job) = (queue.clone(), disk.clone(), event.job);
            tokio::spawn(async move {
                disk.remove_file(Path::new("/filler")).await.expect("room");
                let retry = Answer::Error {
                    choice: ErrorChoice::Retry,
                };
                queue.answer(job, retry).expect("answered");
            });
        }
    });
    let done = harness
        .run(copy(&["/a.bin"], "/", JobOptions::default()))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty(), "{done:?}");
    assert_eq!(read(&small, "/a.bin").await, payload(8_000, 1));
    assert_eq!(
        done.progress.bytes_done, 8_000,
        "the failed try is not counted"
    );
}

/// `Memory(2)` with a read-only `/locked`.
async fn locked(harness: &Harness) -> MemoryFs {
    let two = harness.memory(2);
    two.mkdir(Path::new("/locked"), false).await.expect("mkdir");
    let readonly = SetAttributes {
        readonly: Some(true),
        ..SetAttributes::default()
    };
    two.set_attributes(Path::new("/locked"), readonly)
        .await
        .expect("read-only");
    two
}

#[tokio::test]
async fn no_access_is_recorded_when_errors_are_skipped() {
    let harness = Harness::start();
    let one = harness.memory(1);
    locked(&harness).await;
    write(&one, "/a.txt", b"a").await;
    let options = JobOptions {
        on_error: ErrorPolicy::Skip,
        ..JobOptions::default()
    };
    let done = harness.run(copy(&["/a.txt"], "/locked", options)).await;
    assert_eq!(done.state, JobState::Completed);
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert_eq!(done.failures[0].code, "permission_denied");
}

#[tokio::test]
async fn on_error_cancel_fails_the_job() {
    let harness = Harness::start();
    let one = harness.memory(1);
    locked(&harness).await;
    write(&one, "/a.txt", b"a").await;
    let options = JobOptions {
        on_error: ErrorPolicy::Cancel,
        ..JobOptions::default()
    };
    let done = harness.run(copy(&["/a.txt"], "/locked", options)).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "permission_denied");
}

#[tokio::test]
async fn verify_detects_a_corrupted_target() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let disk = MemoryFs::new();
    harness.queue.register(
        BackendId::Memory(2),
        Backend::new(Arc::new(Corrupting(disk.clone()))),
    );
    write(&one, "/a.bin", &payload(10_000, 3)).await;

    let unchecked = harness
        .run(copy(&["/a.bin"], "/plain", JobOptions::default()))
        .await;
    assert_eq!(unchecked.state, JobState::Completed);
    assert_ne!(
        read(&disk, "/plain/a.bin").await,
        payload(10_000, 3),
        "without verify the damage goes unnoticed"
    );

    let options = JobOptions {
        verify: true,
        on_error: ErrorPolicy::Skip,
        ..JobOptions::default()
    };
    let checked = harness.run(copy(&["/a.bin"], "/checked", options)).await;
    assert_eq!(checked.failures.len(), 1, "{checked:?}");
    assert_eq!(checked.failures[0].code, "verify_failed");
    assert!(
        !exists(&disk, "/checked/a.bin").await,
        "a copy that does not verify is removed"
    );
}

/// A backend whose file streams end after half the file, without an error.
struct Short(MemoryFs);

#[async_trait]
impl rc_vfs::testing::Forward for Short {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        let half = self.0.stat(path).await?.size / 2;
        Ok(Box::new(self.0.open_read(path).await?.take(half)))
    }
}

/// RC-080-37: a move whose source stream ends early fails the file and keeps the source - the
/// target holds what was written, which is not the source.
#[tokio::test]
async fn a_move_with_a_short_source_stream_keeps_the_source() {
    let harness = Harness::start();
    let one = MemoryFs::new();
    harness.queue.register(
        BackendId::Memory(1),
        Backend::new(Arc::new(Short(one.clone()))),
    );
    let two = harness.memory(2);
    write(&one, "/a.bin", &payload(10_000, 3)).await;
    let done = harness
        .run(JobSpec {
            kind: JobKind::Move {
                sources: vec![mem(1, "/a.bin")],
                target: mem(2, "/in"),
            },
            options: JobOptions {
                on_error: ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert_eq!(done.failures[0].code, "verify_failed");
    assert_eq!(
        read(&one, "/a.bin").await,
        payload(10_000, 3),
        "the source stays"
    );
    assert!(
        !exists(&two, "/in/a.bin").await,
        "the short copy is removed"
    );
}

#[tokio::test]
async fn verify_passes_a_good_copy() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/a.bin", &payload(10_000, 3)).await;
    let options = JobOptions {
        verify: true,
        ..JobOptions::default()
    };
    let done = harness.run(copy(&["/a.bin"], "/", options)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty());
    assert_eq!(read(&two, "/a.bin").await, payload(10_000, 3));
}

#[tokio::test]
async fn a_missing_source_is_a_failure_not_a_crash() {
    let harness = Harness::start();
    let one = harness.memory(1);
    harness.memory(2);
    write(&one, "/there.txt", b"t").await;
    let options = JobOptions {
        on_error: ErrorPolicy::Skip,
        ..JobOptions::default()
    };
    let done = harness
        .run(copy(&["/gone.txt", "/there.txt"], "/", options))
        .await;
    assert_eq!(done.state, JobState::Completed);
    assert_eq!(done.failures[0].code, "not_found");
    assert_eq!(done.progress.files_total, 1);
}

#[tokio::test]
async fn a_permanent_delete_keeps_what_holds_a_skipped_entry() {
    let harness = Harness::start();
    let one = harness.memory(1);
    write(&one, "/tree/free.txt", b"f").await;
    write(&one, "/tree/locked/kept.txt", b"k").await;
    let readonly = SetAttributes {
        readonly: Some(true),
        ..SetAttributes::default()
    };
    one.set_attributes(Path::new("/tree/locked"), readonly)
        .await
        .expect("read-only");
    let done = harness
        .run(JobSpec {
            kind: JobKind::Delete {
                sources: vec![mem(1, "/tree")],
                mode: rc_ops::DeleteMode::Permanent,
            },
            options: JobOptions {
                on_error: ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.failures.len(), 1, "only the file itself: {done:?}");
    assert_eq!(done.failures[0].code, "permission_denied");
    assert!(!exists(&one, "/tree/free.txt").await);
    assert!(exists(&one, "/tree/locked/kept.txt").await);
}

/// A server that lists `../escape` and `/abs` in a folder: neither is read nor written anywhere,
/// each fails with `invalid_name`, and the real file is copied.
#[tokio::test]
async fn listed_names_that_lead_out_of_their_folder_fail_and_write_nothing() {
    let harness = Harness::start();
    let one = MemoryFs::new();
    write(&one, "/src/ok.txt", b"ok").await;
    harness.queue.register(
        BackendId::Memory(1),
        Backend::new(Arc::new(Lying(one.clone()))),
    );
    let two = harness.memory(2);
    two.mkdir(Path::new("/dst"), true).await.expect("mkdir");
    let options = JobOptions {
        on_error: ErrorPolicy::Skip,
        ..JobOptions::default()
    };
    let done = harness.run(copy(&["/src"], "/dst", options)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let codes: Vec<&str> = done
        .failures
        .iter()
        .map(|failure| failure.code.as_str())
        .collect();
    assert_eq!(codes, vec!["invalid_name"; LIES.len()], "{done:?}");
    assert_eq!(read(&two, "/dst/src/ok.txt").await, b"ok");
    for outside in ["/abs", "/escape", "/dst/escape", "/dst/abs"] {
        assert!(!exists(&two, outside).await, "{outside}");
    }
}

/// RC-080-12 (X2): on Windows a copy into a local folder refuses a source whose own name Windows
/// would turn into a stream, a device or a spoofed name; the other file is copied.
#[cfg(windows)]
#[tokio::test]
async fn a_top_level_name_windows_cannot_hold_fails_on_a_local_target() {
    let harness = Harness::start();
    harness.local();
    let one = harness.memory(1);
    let names = [
        "/notes: draft.txt",
        "/con",
        "/invoice\u{202e}fdp.exe",
        "/ok.txt",
    ];
    for name in names {
        write(&one, name, b"x").await;
    }
    let target = tempfile::tempdir().expect("temp dir");
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: names.iter().map(|name| mem(1, name)).collect(),
                target: rc_vfs::VfsPath::local(target.path()),
            },
            options: JobOptions {
                on_error: ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let codes: Vec<&str> = done
        .failures
        .iter()
        .map(|failure| failure.code.as_str())
        .collect();
    assert_eq!(codes, vec!["invalid_name"; 3], "{done:?}");
    assert!(target.path().join("ok.txt").exists());
    assert!(!target.path().join("notes").exists());
}

/// RC-080-12 (X6): F5 out of an archive judges every name as unpacking does, on every system: a
/// stored `con`, `report.` or `notes:x` fails with `invalid_name`, everything else is copied.
#[tokio::test]
async fn names_out_of_an_archive_are_judged_as_unpacking_judges_them() {
    let harness = Harness::start();
    let archive = BackendId::Archive {
        outer: Box::new(mem(9, "/a.zip")),
        format: rc_vfs::ArchiveFormat::Zip,
    };
    let inside = harness.memory_as(archive.clone());
    for name in [
        "/con",
        "/report.",
        "/dir/notes:x",
        "/dir/kept.txt",
        "/ok.txt",
    ] {
        write(&inside, name, b"x").await;
    }
    let two = harness.memory(2);
    two.mkdir(Path::new("/dst"), true).await.expect("mkdir");
    let sources = ["/con", "/report.", "/dir", "/ok.txt"].map(|path| rc_vfs::VfsPath {
        backend: archive.clone(),
        path: std::path::PathBuf::from(path),
    });
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: sources.to_vec(),
                target: mem(2, "/dst"),
            },
            options: JobOptions {
                on_error: ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let codes: Vec<&str> = done
        .failures
        .iter()
        .map(|failure| failure.code.as_str())
        .collect();
    assert_eq!(codes, vec!["invalid_name"; 3], "{done:?}");
    assert!(exists(&two, "/dst/ok.txt").await);
    assert!(exists(&two, "/dst/dir/kept.txt").await);
    for refused in ["/dst/con", "/dst/report.", "/dst/dir/notes:x"] {
        assert!(!exists(&two, refused).await, "{refused}");
    }
}
