//! A case-only rename where the backend says case does not count but the folder tells it apart
//! (RC-110-22: a WSL share on Windows, an NTFS folder flagged case-sensitive): the name in the
//! other case is another entry, and no way of renaming replaces it - Shift+F6 on a file and on
//! a folder (`JobKind::MoveAs`, which the queue's placement lets through as a change of case)
//! and the multi-rename's job.

use crate::support::{Harness, exists, mem, read, write};
use rc_ops::{ErrorPolicy, JobKind, JobOptions, JobReport, JobSnapshot, JobSpec, JobState};
use rc_rename::{RenameStep, TEMP_PREFIX};
use rc_vfs::{MemoryFs, Vfs};

/// Shift+F6 `from` to `to` on the case-blind backend 1, an error ending the job.
async fn shift_f6(harness: &Harness, from: &str, to: &str) -> JobSnapshot {
    harness
        .run(JobSpec {
            kind: JobKind::MoveAs {
                source: mem(1, from),
                target: mem(1, to),
            },
            options: JobOptions {
                on_error: ErrorPolicy::Cancel,
                ..JobOptions::default()
            },
        })
        .await
}

async fn names(fs: &MemoryFs, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = fs
        .list(std::path::Path::new(dir))
        .await
        .expect("list")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_file_in_the_other_case_is_a_conflict_and_stays() {
    let harness = Harness::start();
    let (fs, _renames) = harness.case_blind(1, None);
    write(&fs, "/dir/makefile", b"mine").await;
    write(&fs, "/dir/Makefile", b"theirs").await;
    let done = shift_f6(&harness, "/dir/makefile", "/dir/Makefile").await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert_eq!(done.failures[0].code, "already_exists");
    assert_eq!(read(&fs, "/dir/Makefile").await, b"theirs");
    assert_eq!(read(&fs, "/dir/makefile").await, b"mine");
    assert_eq!(
        names(&fs, "/dir").await,
        ["Makefile", "makefile"],
        "no temporary name is left"
    );
}

#[tokio::test]
async fn a_folder_in_the_other_case_is_a_conflict_and_stays() {
    let harness = Harness::start();
    let (fs, _renames) = harness.case_blind(1, None);
    write(&fs, "/dir/docs/a.txt", b"mine").await;
    write(&fs, "/dir/Docs/b.txt", b"theirs").await;
    let done = shift_f6(&harness, "/dir/docs", "/dir/Docs").await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "already_exists");
    assert_eq!(read(&fs, "/dir/docs/a.txt").await, b"mine");
    assert_eq!(read(&fs, "/dir/Docs/b.txt").await, b"theirs");
    assert!(!exists(&fs, "/dir/Docs/a.txt").await, "nothing was merged");
    assert_eq!(names(&fs, "/dir").await, ["Docs", "docs"]);
}

/// The disk itself, which tells case apart on Linux, behind a backend that says it does not:
/// `rename(2)` replaces a file and an empty folder without a word.
#[cfg(target_os = "linux")]
mod on_disk {
    use std::{fs, path::Path, sync::Arc};

    use async_trait::async_trait;
    use rc_ops::{Backend, ErrorPolicy, JobKind, JobOptions, JobSnapshot, JobSpec, JobState};
    use rc_vfs::{BackendId, Capabilities, LocalFs, Vfs, VfsPath};

    use crate::support::Harness;

    struct Blind(LocalFs);

    #[async_trait]
    impl rc_vfs::testing::Forward for Blind {
        fn inner(&self) -> &dyn Vfs {
            &self.0
        }
        fn capabilities(&self) -> Capabilities {
            let mut capabilities = self.0.capabilities();
            capabilities.case_sensitive = false;
            capabilities
        }
    }

    async fn shift_f6(root: &Path, from: &str, to: &str) -> JobSnapshot {
        let harness = Harness::start();
        harness.queue.register(
            BackendId::Local,
            Backend::new(Arc::new(Blind(LocalFs::new()))),
        );
        harness
            .run(JobSpec {
                kind: JobKind::MoveAs {
                    source: VfsPath::local(root.join(from)),
                    target: VfsPath::local(root.join(to)),
                },
                options: JobOptions {
                    on_error: ErrorPolicy::Cancel,
                    ..JobOptions::default()
                },
            })
            .await
    }

    #[tokio::test]
    async fn a_file_in_the_other_case_stays() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(root.path().join("makefile"), b"mine").expect("write");
        fs::write(root.path().join("Makefile"), b"theirs").expect("write");
        let done = shift_f6(root.path(), "makefile", "Makefile").await;
        assert_eq!(done.state, JobState::Failed, "{done:?}");
        assert_eq!(done.failures[0].code, "already_exists");
        assert_eq!(
            fs::read(root.path().join("Makefile")).expect("read"),
            b"theirs"
        );
        assert_eq!(
            fs::read(root.path().join("makefile")).expect("read"),
            b"mine"
        );
        assert_eq!(fs::read_dir(root.path()).expect("list").count(), 2);
    }

    #[tokio::test]
    async fn an_empty_folder_in_the_other_case_stays() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir(root.path().join("docs")).expect("mkdir");
        fs::write(root.path().join("docs").join("a.txt"), b"mine").expect("write");
        fs::create_dir(root.path().join("Docs")).expect("mkdir");
        let done = shift_f6(root.path(), "docs", "Docs").await;
        assert_eq!(done.state, JobState::Failed, "{done:?}");
        assert_eq!(done.failures[0].code, "already_exists");
        assert_eq!(
            fs::read(root.path().join("docs").join("a.txt")).expect("read"),
            b"mine"
        );
        let other = fs::read_dir(root.path().join("Docs")).expect("the other folder");
        assert_eq!(other.count(), 0, "still its own, empty folder");
        assert_eq!(fs::read_dir(root.path()).expect("list").count(), 2);
    }
}

/// The multi-rename's steps for `makefile` to `Makefile` where case is ignored: through a
/// temporary name (`rc_rename::schedule`).
fn case_steps() -> Vec<RenameStep> {
    let temp = format!("{TEMP_PREFIX}t-0");
    vec![
        RenameStep {
            from: "makefile".to_owned(),
            to: temp.clone(),
        },
        RenameStep {
            from: temp,
            to: "Makefile".to_owned(),
        },
    ]
}

async fn multi_rename(harness: &Harness, rollback: bool) -> JobSnapshot {
    harness
        .run(JobSpec {
            kind: JobKind::rename(mem(1, "/d"), case_steps()).rolling_back(rollback),
            options: JobOptions {
                on_error: ErrorPolicy::Cancel,
                ..JobOptions::default()
            },
        })
        .await
}

#[tokio::test]
async fn a_multi_rename_never_replaces_the_other_case_and_rolls_back() {
    let harness = Harness::start();
    let (fs, _renames) = harness.case_blind(1, None);
    write(&fs, "/d/makefile", b"mine").await;
    write(&fs, "/d/Makefile", b"theirs").await;
    let done = multi_rename(&harness, true).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "already_exists");
    assert_eq!(
        done.report,
        Some(JobReport::RenameStopped {
            rolled_back: true,
            parked: Vec::new(),
        })
    );
    assert_eq!(read(&fs, "/d/Makefile").await, b"theirs");
    assert_eq!(read(&fs, "/d/makefile").await, b"mine");
    assert_eq!(names(&fs, "/d").await, ["Makefile", "makefile"]);
}

/// Without the roll-back the entry waits under its temporary name, named in the report; the
/// other file is untouched all the same.
#[tokio::test]
async fn a_multi_rename_without_roll_back_names_the_parked_entry() {
    let harness = Harness::start();
    let (fs, _renames) = harness.case_blind(1, None);
    write(&fs, "/d/makefile", b"mine").await;
    write(&fs, "/d/Makefile", b"theirs").await;
    let done = multi_rename(&harness, false).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "already_exists");
    let Some(JobReport::RenameStopped {
        rolled_back: false,
        parked,
    }) = &done.report
    else {
        panic!("a report of the parked entry, got {done:?}");
    };
    assert_eq!(parked.len(), 1, "{parked:?}");
    assert_eq!(read(&fs, "/d/Makefile").await, b"theirs");
    assert_eq!(read(&fs, &format!("/d/{}", parked[0].temp)).await, b"mine");
}
