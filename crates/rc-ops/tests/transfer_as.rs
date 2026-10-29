//! Copy and move of one entry to a path of its own (RC-010-07): F5 and F6 with a new name,
//! Shift+F5 in the same directory, Shift+F6, and a case-only rename whose undo fails
//! (RC-080-37).

mod support;

use std::{
    fs,
    sync::{Arc, Mutex},
};

use rc_ops::{
    Answer, ConflictChoice, ErrorChoice, Failure, JobChange, JobKind, JobOptions, JobSpec,
    JobState, OpsError, Question,
};
use rc_vfs::{Vfs, VfsPath};
use support::{Harness, exists, mem, read, write};

fn spec(kind: JobKind) -> JobSpec {
    JobSpec {
        kind,
        options: JobOptions::default(),
    }
}

#[tokio::test]
async fn copy_as_copies_a_file_under_a_new_name_in_its_own_directory() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    write(&fs, "/dir/a.txt", b"alpha").await;
    let done = harness
        .run(spec(JobKind::CopyAs {
            source: mem(1, "/dir/a.txt"),
            target: mem(1, "/dir/b.txt"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&fs, "/dir/b.txt").await, b"alpha");
    assert_eq!(read(&fs, "/dir/a.txt").await, b"alpha", "the source stays");
}

#[tokio::test]
async fn copy_as_takes_a_directory_with_its_contents_and_makes_the_parent() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    write(&fs, "/src/a.txt", b"a").await;
    write(&fs, "/src/sub/b.txt", b"b").await;
    let done = harness
        .run(spec(JobKind::CopyAs {
            source: mem(1, "/src"),
            target: mem(1, "/new/copy"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&fs, "/new/copy/a.txt").await, b"a");
    assert_eq!(read(&fs, "/new/copy/sub/b.txt").await, b"b");
    assert_eq!(done.progress.files_done, 2);
}

#[tokio::test]
async fn move_as_renames_on_the_local_disk() {
    let root = tempfile::tempdir().expect("tempdir");
    fs::write(root.path().join("old.txt"), b"x").expect("write");
    let harness = Harness::start();
    harness.local();
    let done = harness
        .run(spec(JobKind::MoveAs {
            source: VfsPath::local(root.path().join("old.txt")),
            target: VfsPath::local(root.path().join("new.txt")),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(!root.path().join("old.txt").exists());
    assert_eq!(fs::read(root.path().join("new.txt")).expect("read"), b"x");
}

#[tokio::test]
async fn move_as_between_backends_copies_then_removes() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/a.txt", b"a").await;
    let done = harness
        .run(spec(JobKind::MoveAs {
            source: mem(1, "/a.txt"),
            target: mem(2, "/in/b.txt"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&two, "/in/b.txt").await, b"a");
    assert!(!exists(&one, "/a.txt").await);
}

#[tokio::test]
async fn an_existing_target_is_a_conflict() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    write(&fs, "/a.txt", b"new").await;
    write(&fs, "/b.txt", b"old").await;
    let done = harness
        .run(JobSpec {
            kind: JobKind::CopyAs {
                source: mem(1, "/a.txt"),
                target: mem(1, "/b.txt"),
            },
            options: JobOptions {
                on_conflict: Some(ConflictChoice::Skip),
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.skipped, 1);
    assert_eq!(read(&fs, "/b.txt").await, b"old");
}

#[tokio::test]
async fn onto_itself_into_itself_and_bad_names_are_refused() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    write(&fs, "/dir/a.txt", b"a").await;
    let submit = |source: &str, target: &str| {
        harness.queue.submit(spec(JobKind::CopyAs {
            source: mem(1, source),
            target: mem(1, target),
        }))
    };
    assert!(matches!(
        submit("/dir/a.txt", "/dir/a.txt"),
        Err(OpsError::SameLocation { .. })
    ));
    assert!(matches!(
        submit("/dir", "/dir/inner"),
        Err(OpsError::TargetInsideSource { .. })
    ));
    assert!(matches!(
        submit("/dir/a.txt", "/"),
        Err(OpsError::InvalidName { .. })
    ));
    assert!(matches!(
        submit("/dir/a.txt", "/dir/con.txt"),
        Err(OpsError::InvalidName { .. })
    ));
}

/// Shift+F6 `a.txt` to `A.txt` where case does not count: through a temporary name.
#[tokio::test]
async fn a_case_only_rename_goes_through_a_temporary_name() {
    let harness = Harness::start();
    let (fs, renames) = harness.case_blind(1, None);
    write(&fs, "/dir/a.txt", b"a").await;
    let done = harness
        .run(spec(JobKind::MoveAs {
            source: mem(1, "/dir/a.txt"),
            target: mem(1, "/dir/A.txt"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&fs, "/dir/A.txt").await, b"a");
    assert!(!exists(&fs, "/dir/a.txt").await);
    let renames = renames.lock().expect("renames").clone();
    assert_eq!(renames.len(), 2, "{renames:?}");
    let temp = &renames[0].1;
    assert!(
        temp.file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".rc-tmp-")),
        "{temp:?}"
    );
    assert_eq!(renames[1], (temp.clone(), "/dir/A.txt".into()));
    assert_eq!(
        fs.list(std::path::Path::new("/dir"))
            .await
            .expect("list")
            .len(),
        1
    );
}

#[tokio::test]
async fn a_failing_second_step_puts_the_old_name_back() {
    let harness = Harness::start();
    let (fs, _renames) = harness.case_blind(1, Some("/dir/A.txt"));
    write(&fs, "/dir/a.txt", b"a").await;
    let done = harness
        .run(JobSpec {
            kind: JobKind::MoveAs {
                source: mem(1, "/dir/a.txt"),
                target: mem(1, "/dir/A.txt"),
            },
            options: JobOptions {
                on_error: rc_ops::ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert_eq!(read(&fs, "/dir/a.txt").await, b"a", "the old name is back");
    assert_eq!(
        fs.list(std::path::Path::new("/dir"))
            .await
            .expect("list")
            .len(),
        1,
        "no temporary name is left"
    );
}

/// RC-080-37: the second step fails and so does the undo - the failure names the temporary
/// name the entry stays under, and a retry finishes the rename from there.
#[tokio::test]
async fn a_failed_undo_is_reported_and_a_retry_goes_on_from_the_temporary_name() {
    let harness = Harness::start();
    let (fs, _renames, failing) = harness.case_blind_failing(1, &["/dir/A.txt", "/dir/a.txt"]);
    write(&fs, "/dir/a.txt", b"a").await;
    let asked: Arc<Mutex<Vec<Failure>>> = Arc::default();
    let seen = Arc::clone(&asked);
    harness.on_event(move |queue, event| {
        if let JobChange::Question {
            question: Question::Error(failure),
        } = &event.change
        {
            seen.lock().expect("asked").push(failure.clone());
            failing.lock().expect("failing").clear();
            let retry = Answer::Error {
                choice: ErrorChoice::Retry,
            };
            let _ = queue.answer(event.job, retry);
        }
    });
    let done = harness
        .run(spec(JobKind::MoveAs {
            source: mem(1, "/dir/a.txt"),
            target: mem(1, "/dir/A.txt"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let asked = asked.lock().expect("asked").clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].code, "rename_undo_failed");
    let temp = asked[0].path.path.file_name().expect("a name");
    assert!(temp.to_string_lossy().starts_with(".rc-tmp-"), "{temp:?}");
    assert_eq!(read(&fs, "/dir/A.txt").await, b"a");
    assert_eq!(
        fs.list(std::path::Path::new("/dir"))
            .await
            .expect("list")
            .len(),
        1,
        "no temporary name is left"
    );
}

#[tokio::test]
async fn only_a_move_may_change_case_and_only_case() {
    let harness = Harness::start();
    let (fs, _renames) = harness.case_blind(1, None);
    write(&fs, "/dir/a.txt", b"a").await;
    let copy = harness.queue.submit(spec(JobKind::CopyAs {
        source: mem(1, "/dir/a.txt"),
        target: mem(1, "/dir/A.txt"),
    }));
    assert!(matches!(copy, Err(OpsError::SameLocation { .. })));
    let same = harness.queue.submit(spec(JobKind::MoveAs {
        source: mem(1, "/dir/a.txt"),
        target: mem(1, "/dir/a.txt"),
    }));
    assert!(matches!(same, Err(OpsError::SameLocation { .. })));
}
