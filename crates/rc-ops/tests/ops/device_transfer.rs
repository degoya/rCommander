//! Copy and move onto and off a portable device (RC-100-61): the queue treats
//! `BackendId::Device` like any backend that is no disk and no server - the file is written in
//! place, never as a part file (MTP renames nothing reliably and resumes nothing), and a move off
//! the device removes the source once the copy is complete. An in-memory backend stands in for
//! the device; the device backend's own streaming is tested in `rc-vfs` (`device/fs_tests.rs`).

use std::{fs, path::PathBuf};

use crate::support::{Harness, exists, payload, read, write};
use rc_ops::{JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, VfsPath};

fn device(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Device {
            device: "usb#phone".to_owned(),
        },
        path: PathBuf::from(path),
    }
}

fn spec(kind: JobKind) -> JobSpec {
    JobSpec {
        kind,
        options: JobOptions::default(),
    }
}

#[tokio::test]
async fn a_file_is_copied_onto_the_device_in_place() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("photo.jpg");
    fs::write(&source, payload(300_000, 7)).expect("write");
    let harness = Harness::start();
    harness.local();
    let phone = harness.memory_as(BackendId::Device {
        device: "usb#phone".to_owned(),
    });
    write(&phone, "/Internal storage/keep.txt", b"k").await;

    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![VfsPath::local(&source)],
            target: device("/Internal storage"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.progress.bytes_done, 300_000);
    assert_eq!(
        read(&phone, "/Internal storage/photo.jpg").await,
        payload(300_000, 7)
    );
    assert!(
        !exists(&phone, "/Internal storage/photo.jpg.rcpart").await,
        "no part file on a device"
    );
    assert!(source.exists(), "the source stays");
}

#[tokio::test]
async fn a_move_off_the_device_removes_the_source_after_the_copy() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let phone = harness.memory_as(BackendId::Device {
        device: "usb#phone".to_owned(),
    });
    write(&phone, "/Internal storage/DCIM/a.jpg", b"jpeg").await;

    let done = harness
        .run(spec(JobKind::Move {
            sources: vec![device("/Internal storage/DCIM/a.jpg")],
            target: VfsPath::local(root.path()),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(fs::read(root.path().join("a.jpg")).expect("read"), b"jpeg");
    assert!(
        !exists(&phone, "/Internal storage/DCIM/a.jpg").await,
        "the source is gone"
    );
}
