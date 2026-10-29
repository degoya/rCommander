//! Every conflict answer, asked and preset, and "for all".

mod support;

use rc_ops::{
    Answer, ConflictChoice, JobChange, JobKind, JobOptions, JobSnapshot, JobSpec, JobState,
    Question,
};
use rc_vfs::MemoryFs;
use support::{Harness, exists, mem, read, set_mtime, write};

/// `Memory(1)` holds `/src/{a,b,c}.txt` (new), `Memory(2)` holds `/dst/{a,b}.txt` (old) and
/// `/dst/c.txt` (newer than the source).
async fn setup(harness: &Harness) -> (MemoryFs, MemoryFs) {
    let one = harness.memory(1);
    let two = harness.memory(2);
    for name in ["a", "b", "c"] {
        write(
            &one,
            &format!("/src/{name}.txt"),
            format!("new {name}").as_bytes(),
        )
        .await;
        write(
            &two,
            &format!("/dst/{name}.txt"),
            format!("old {name}").as_bytes(),
        )
        .await;
        set_mtime(&one, &format!("/src/{name}.txt"), 2_000).await;
        set_mtime(&two, &format!("/dst/{name}.txt"), 1_000).await;
    }
    set_mtime(&two, "/dst/c.txt", 3_000).await;
    (one, two)
}

fn copy(on_conflict: Option<ConflictChoice>) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: ["a", "b", "c"]
                .iter()
                .map(|name| mem(1, &format!("/src/{name}.txt")))
                .collect(),
            target: mem(2, "/dst"),
        },
        options: JobOptions {
            on_conflict,
            ..JobOptions::default()
        },
    }
}

/// Runs the copy, answering each question with the next of `answers`; the questions asked.
async fn asked(harness: &Harness, answers: Vec<Answer>) -> (JobSnapshot, usize) {
    let mut queue = answers.into_iter();
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = std::sync::Arc::clone(&count);
    harness.on_event(move |jobs, event| {
        if let JobChange::Question { question } = &event.change {
            assert!(
                matches!(question, Question::Conflict { .. }),
                "{question:?}"
            );
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let answer = queue.next().expect("an answer for every question");
            jobs.answer(event.job, answer).expect("the answer fits");
        }
    });
    let done = harness.run(copy(None)).await;
    (done, count.load(std::sync::atomic::Ordering::SeqCst))
}

fn conflict(choice: ConflictChoice, for_all: bool) -> Answer {
    Answer::Conflict { choice, for_all }
}

#[tokio::test]
async fn overwrite_replaces_the_target() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(
        &harness,
        vec![
            conflict(ConflictChoice::Overwrite, false),
            conflict(ConflictChoice::Skip, false),
            conflict(ConflictChoice::Skip, false),
        ],
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(questions, 3);
    assert_eq!(read(&two, "/dst/a.txt").await, b"new a");
    assert_eq!(read(&two, "/dst/b.txt").await, b"old b");
    assert_eq!(done.skipped, 2);
}

#[tokio::test]
async fn skip_for_all_asks_once() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(&harness, vec![conflict(ConflictChoice::Skip, true)]).await;
    assert_eq!(done.state, JobState::Completed);
    assert_eq!(questions, 1, "for all: no further question");
    assert_eq!(done.skipped, 3);
    assert_eq!(done.progress.files_done, 3);
    assert_eq!(read(&two, "/dst/c.txt").await, b"old c");
}

#[tokio::test]
async fn rename_writes_beside_the_target() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(&harness, vec![conflict(ConflictChoice::Rename, true)]).await;
    assert_eq!(done.state, JobState::Completed);
    assert_eq!(questions, 1);
    assert_eq!(read(&two, "/dst/a (2).txt").await, b"new a");
    assert_eq!(read(&two, "/dst/a.txt").await, b"old a");
    assert_eq!(read(&two, "/dst/c (2).txt").await, b"new c");
}

#[tokio::test]
async fn overwrite_older_for_all_keeps_a_newer_target() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(
        &harness,
        vec![conflict(ConflictChoice::OverwriteOlder, true)],
    )
    .await;
    assert_eq!(done.state, JobState::Completed);
    assert_eq!(questions, 1);
    assert_eq!(read(&two, "/dst/a.txt").await, b"new a");
    assert_eq!(read(&two, "/dst/b.txt").await, b"new b");
    assert_eq!(
        read(&two, "/dst/c.txt").await,
        b"old c",
        "newer than the source"
    );
    assert_eq!(done.skipped, 1);
}

#[tokio::test]
async fn overwrite_for_all_replaces_everything() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(&harness, vec![conflict(ConflictChoice::Overwrite, true)]).await;
    assert_eq!(done.state, JobState::Completed);
    assert_eq!(questions, 1);
    assert_eq!(read(&two, "/dst/c.txt").await, b"new c");
}

#[tokio::test]
async fn cancel_stops_the_job() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(
        &harness,
        vec![
            conflict(ConflictChoice::Overwrite, false),
            conflict(ConflictChoice::Cancel, false),
        ],
    )
    .await;
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert_eq!(questions, 2);
    assert_eq!(
        read(&two, "/dst/a.txt").await,
        b"new a",
        "done before the cancel"
    );
    assert_eq!(read(&two, "/dst/c.txt").await, b"old c", "never reached");
    assert!(done.question.is_none());
}

#[tokio::test]
async fn a_preset_never_asks() {
    for (choice, a_after, skipped) in [
        (ConflictChoice::Overwrite, "new a", 0),
        (ConflictChoice::Skip, "old a", 3),
        (ConflictChoice::OverwriteOlder, "new a", 1),
        (ConflictChoice::Rename, "old a", 0),
    ] {
        let harness = Harness::start();
        let (_, two) = setup(&harness).await;
        harness.on_event(|_, event| {
            assert!(
                !matches!(event.change, JobChange::Question { .. }),
                "a preset job asked"
            );
        });
        let done = harness.run(copy(Some(choice))).await;
        assert_eq!(done.state, JobState::Completed, "{choice:?}: {done:?}");
        assert_eq!(
            read(&two, "/dst/a.txt").await,
            a_after.as_bytes(),
            "{choice:?}"
        );
        assert_eq!(done.skipped, skipped, "{choice:?}");
    }
    let harness = Harness::start();
    setup(&harness).await;
    let cancelled = harness.run(copy(Some(ConflictChoice::Cancel))).await;
    assert_eq!(cancelled.state, JobState::Cancelled);
}

#[tokio::test]
async fn directories_merge_without_a_question() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/tree/new.txt", b"n").await;
    write(&two, "/dst/tree/old.txt", b"o").await;
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![mem(1, "/tree")],
                target: mem(2, "/dst"),
            },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(exists(&two, "/dst/tree/old.txt").await);
    assert!(exists(&two, "/dst/tree/new.txt").await);
}

#[tokio::test]
async fn a_wrong_answer_is_refused() {
    let harness = Harness::start();
    setup(&harness).await;
    harness.on_event(|jobs, event| {
        if let JobChange::Question { .. } = event.change {
            let wrong = jobs.answer(
                event.job,
                Answer::Error {
                    choice: rc_ops::ErrorChoice::Skip,
                },
            );
            assert_eq!(wrong.map(|_| ()).map_err(|e| e.code()), Err("no_question"));
            jobs.cancel(event.job).expect("cancel while waiting");
        }
    });
    let done = harness.run(copy(None)).await;
    assert_eq!(done.state, JobState::Cancelled);
    let late = harness.queue.answer(
        done.id,
        Answer::Conflict {
            choice: ConflictChoice::Skip,
            for_all: false,
        },
    );
    assert_eq!(late.map(|_| ()).map_err(|e| e.code()), Err("job_finished"));
}

fn rename_to(name: &str) -> Answer {
    Answer::RenameTo {
        name: name.to_owned(),
    }
}

#[tokio::test]
async fn rename_to_a_typed_name() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(
        &harness,
        vec![rename_to("typed.txt"), conflict(ConflictChoice::Skip, true)],
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(questions, 2);
    assert_eq!(read(&two, "/dst/typed.txt").await, b"new a");
    assert_eq!(read(&two, "/dst/a.txt").await, b"old a");
}

#[tokio::test]
async fn a_typed_name_that_exists_is_asked_again() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(
        &harness,
        vec![
            rename_to("b.txt"),
            conflict(ConflictChoice::Overwrite, false),
            conflict(ConflictChoice::Skip, true),
        ],
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(questions, 3);
    assert_eq!(read(&two, "/dst/b.txt").await, b"new a");
    assert_eq!(read(&two, "/dst/a.txt").await, b"old a");
}

#[tokio::test]
async fn a_bad_typed_name_and_compare_keep_the_question_open() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    harness.on_event(|jobs, event| {
        if let JobChange::Question { .. } = event.change {
            for bad in ["../escape", "CON", "a/b", ""] {
                let refused = jobs.answer(event.job, rename_to(bad));
                assert_eq!(
                    refused.map(|_| ()).map_err(|e| e.code()),
                    Err("invalid_name"),
                    "{bad:?}"
                );
            }
            let compared = jobs.answer(event.job, Answer::Compare).expect("compare");
            assert_eq!(compared.state, JobState::Waiting, "still asking");
            assert!(compared.question.is_some());
            jobs.answer(event.job, conflict(ConflictChoice::Skip, true))
                .expect("the real answer");
        }
    });
    let done = harness.run(copy(None)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.skipped, 3);
    assert!(!exists(&two, "/escape").await);
    assert_eq!(read(&two, "/dst/a.txt").await, b"old a");
}

#[tokio::test]
async fn rename_existing_moves_the_target_aside() {
    let harness = Harness::start();
    let (_, two) = setup(&harness).await;
    let (done, questions) = asked(
        &harness,
        vec![conflict(ConflictChoice::RenameExisting, true)],
    )
    .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(questions, 1);
    for name in ["a", "b", "c"] {
        assert_eq!(
            read(&two, &format!("/dst/{name}.txt")).await,
            format!("new {name}").as_bytes()
        );
        assert_eq!(
            read(&two, &format!("/dst/{name} (2).txt")).await,
            format!("old {name}").as_bytes()
        );
    }
}

/// `/src/big.txt` (10 bytes) and `/src/small.txt` (1 byte) meet 5-byte targets.
async fn setup_sizes(harness: &Harness) -> MemoryFs {
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/src/big.txt", b"0123456789").await;
    write(&one, "/src/small.txt", b"s").await;
    write(&two, "/dst/big.txt", b"old-b").await;
    write(&two, "/dst/small.txt", b"old-s").await;
    two
}

fn copy_sizes(on_conflict: Option<ConflictChoice>) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![mem(1, "/src/big.txt"), mem(1, "/src/small.txt")],
            target: mem(2, "/dst"),
        },
        options: JobOptions {
            on_conflict,
            ..JobOptions::default()
        },
    }
}

#[tokio::test]
async fn copy_all_larger_replaces_only_smaller_targets() {
    let harness = Harness::start();
    let two = setup_sizes(&harness).await;
    let answers = vec![conflict(ConflictChoice::OverwriteLarger, true)];
    let mut queue = answers.into_iter();
    harness.on_event(move |jobs, event| {
        if let JobChange::Question { .. } = event.change {
            let answer = queue.next().expect("one question only");
            jobs.answer(event.job, answer).expect("fits");
        }
    });
    let done = harness.run(copy_sizes(None)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&two, "/dst/big.txt").await, b"0123456789");
    assert_eq!(read(&two, "/dst/small.txt").await, b"old-s");
    assert_eq!(done.skipped, 1);
}

#[tokio::test]
async fn copy_all_smaller_replaces_only_larger_targets() {
    let harness = Harness::start();
    let two = setup_sizes(&harness).await;
    let done = harness
        .run(copy_sizes(Some(ConflictChoice::OverwriteSmaller)))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&two, "/dst/big.txt").await, b"old-b");
    assert_eq!(read(&two, "/dst/small.txt").await, b"s");
    assert_eq!(done.skipped, 1);
}
