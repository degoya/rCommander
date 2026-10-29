//! The archive test job (RC-030-02): a sound archive passes, a damaged entry and an encrypted
//! one are listed with their codes, and the archive itself is never changed.

mod support;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_ops::{Backend, ErrorPolicy, JobKind, JobOptions, JobSnapshot, JobSpec, JobState};
use rc_vfs::{ArchiveFormat, ArchiveFs, ArchiveSettings, LocalFs, VfsPath};
use support::{Harness, damaged_zip};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives")
        .join(name)
}

/// Tests the archive file `file` of `format` through a queue; the finished job.
async fn test(file: &Path, format: ArchiveFormat) -> JobSnapshot {
    let harness = Harness::start();
    let root = VfsPath::archive(VfsPath::local(file), format, "/");
    let fs = ArchiveFs::new(
        Arc::new(LocalFs::new()),
        file,
        format,
        ArchiveSettings::default(),
    );
    harness
        .queue
        .register(root.backend.clone(), Backend::new(Arc::new(fs)));
    let job = harness
        .queue
        .submit(JobSpec {
            kind: JobKind::TestArchive { archive: root },
            options: JobOptions {
                on_error: ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .expect("accepted");
    harness.queue.wait(job.id).await.expect("finishes")
}

/// The failures' file names and codes.
fn failed(job: &JobSnapshot) -> Vec<(String, String)> {
    job.failures
        .iter()
        .map(|failure| {
            let name = failure
                .path
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, failure.code.clone())
        })
        .collect()
}

#[tokio::test]
async fn archive_test_passes_a_sound_archive_of_every_format() {
    for (name, format) in [
        ("tree.zip", ArchiveFormat::Zip),
        ("tree.7z", ArchiveFormat::SevenZ),
        (
            "tree.tar.xz",
            ArchiveFormat::Tar {
                compression: Some(rc_vfs::Compression::Xz),
            },
        ),
    ] {
        let job = test(&fixture(name), format).await;
        assert_eq!(job.state, JobState::Completed, "{name}: {job:?}");
        assert!(job.failures.is_empty(), "{name}: {:?}", job.failures);
        assert_eq!(job.progress.files_done, job.progress.files_total, "{name}");
        assert_eq!(job.progress.bytes_done, job.progress.bytes_total, "{name}");
    }
}

#[tokio::test]
async fn archive_test_reports_the_damaged_entry_of_a_zip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = damaged_zip(dir.path());
    let before = std::fs::read(&file).expect("read");
    let job = test(&file, ArchiveFormat::Zip).await;
    assert_eq!(job.state, JobState::Completed, "{job:?}");
    assert_eq!(
        failed(&job),
        vec![("hello.txt".to_owned(), "archive_damaged".to_owned())]
    );
    assert_eq!(
        std::fs::read(&file).expect("read"),
        before,
        "a test changes nothing"
    );
}

#[tokio::test]
async fn archive_test_lists_an_encrypted_entry_as_password_required() {
    let job = test(&fixture("encrypted-aes.zip"), ArchiveFormat::Zip).await;
    assert_eq!(job.state, JobState::Completed, "{job:?}");
    assert!(!job.failures.is_empty());
    assert!(
        job.failures
            .iter()
            .all(|failure| failure.code == "password_required"),
        "{:?}",
        job.failures
    );
}
