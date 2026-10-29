use std::{fs, time::Duration};

use rc_ops::{Answer, ConflictChoice, JobId, JobOptions, JobState, QueueEntry, Reorder};

use crate::test_support;
use crate::{Command, Engine, EngineEvent, Outcome};

async fn engine() -> (tempfile::TempDir, Engine) {
    test_support::engine(&[("a.txt", "a"), ("b.txt", "b"), ("docs/a.txt", "old")]).await
}

async fn copy(engine: &Engine, name: &str) -> JobId {
    let outcome = engine
        .dispatch(Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec![name.into()],
            target: Some("docs".into()),
            options: JobOptions::default(),
        }))
        .await
        .expect("queued");
    match outcome {
        Outcome::Job(job) => job.id,
        other => panic!("a job, got {other:?}"),
    }
}

/// The first job holds the local lane: it asks about `docs/a.txt`.
async fn holding(engine: &Engine) -> JobId {
    let id = copy(engine, "a.txt").await;
    for _ in 0..500 {
        if engine.ops().job(id).expect("known").question.is_some() {
            return id;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} asked nothing");
}

fn entries(outcome: Outcome) -> Vec<QueueEntry> {
    match outcome {
        Outcome::Queue { entries } => entries,
        other => panic!("the queue, got {other:?}"),
    }
}

fn order(entries: &[QueueEntry]) -> Vec<JobId> {
    entries.iter().map(|entry| entry.job.id).collect()
}

#[tokio::test]
async fn queue_move_reorders_the_waiting_jobs_and_announces_it() {
    let (root, engine) = engine().await;
    let first = holding(&engine).await;
    let second = copy(&engine, "b.txt").await;
    let third = waiting(&engine, root.path(), "new").await;
    let listed = entries(engine.dispatch(Command::QueueList {}).await.expect("list"));
    assert_eq!(order(&listed), [first, second, third]);
    assert_eq!(listed[1].place, Some(0));

    let mut events = engine.subscribe();
    let moved = engine
        .dispatch(Command::QueueMove {
            job: third,
            step: Reorder::Front,
        })
        .await
        .expect("moved");
    assert_eq!(order(&entries(moved)), [first, third, second]);
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::QueueChanged));

    let refused = engine
        .dispatch(Command::QueueRemove { job: first })
        .await
        .map(|_| ())
        .map_err(|error| error.code());
    assert_eq!(refused, Err("job_not_waiting"));
    let removed = entries(
        engine
            .dispatch(Command::QueueRemove { job: second })
            .await
            .expect("removed"),
    );
    assert_eq!(order(&removed), [first, third]);

    engine
        .dispatch(Command::OpsAnswer {
            job: first,
            answer: Answer::Conflict {
                choice: ConflictChoice::Skip,
                for_all: false,
            },
        })
        .await
        .expect("answered");
    let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(third))
        .await
        .expect("in time")
        .expect("known");
    assert_eq!(done.state, JobState::Completed);
    let cleared = entries(
        engine
            .dispatch(Command::QueueClearFinished {})
            .await
            .expect("cleared"),
    );
    assert!(
        cleared.iter().all(|entry| !entry.job.state.is_finished()),
        "{cleared:?}"
    );
}

#[tokio::test]
async fn queue_window_is_announced_for_the_window() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    let listed = entries(
        engine
            .dispatch(Command::QueueWindow {})
            .await
            .expect("dispatched"),
    );
    assert!(listed.is_empty());
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::QueueWindow));
}

/// One more job waiting in the local lane: a copy of a new file `name` into `docs` (a new
/// folder is made at once, outside the lanes).
async fn waiting(engine: &Engine, root: &std::path::Path, name: &str) -> JobId {
    fs::write(root.join(name), name).expect("source");
    copy(engine, name).await
}

/// RC-010-15, the owner's selection in the queue window: several jobs moved keeping their order,
/// put into a new list, taken off - the asking one cancelled -, then a list and every list cleared.
#[tokio::test]
async fn several_jobs_move_go_into_a_new_list_and_are_taken_off() {
    let (root, engine) = engine().await;
    let first = holding(&engine).await;
    let second = copy(&engine, "b.txt").await;
    let one = waiting(&engine, root.path(), "new1").await;
    let two = waiting(&engine, root.path(), "new2").await;

    let mut events = engine.subscribe();
    let moved = engine
        .dispatch(Command::QueueMoveMany {
            jobs: vec![first, two, one],
            step: Reorder::Front,
        })
        .await
        .expect("moved");
    assert_eq!(order(&entries(moved)), [first, one, two, second]);
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::QueueChanged));
    let unknown = engine
        .dispatch(Command::QueueMoveMany {
            jobs: vec![one, 999],
            step: Reorder::Back,
        })
        .await
        .map(|_| ())
        .map_err(|error| error.code());
    assert_eq!(unknown, Err("job_not_found"));

    let Outcome::QueueLists { lists, .. } = engine
        .dispatch(Command::QueueCreate {
            name: "Later".into(),
            jobs: vec![second],
        })
        .await
        .expect("made")
    else {
        panic!("the lists");
    };
    let later = lists
        .iter()
        .find(|list| list.name == "Later")
        .expect("listed");
    assert_eq!(
        (later.jobs, later.started),
        (1, false),
        "a new list is stopped"
    );
    let later = later.id;

    let left = entries(
        engine
            .dispatch(Command::QueueDiscard {
                jobs: vec![first, one],
            })
            .await
            .expect("taken off"),
    );
    assert!(
        !order(&left).contains(&first) && !order(&left).contains(&one),
        "{left:?}"
    );
    // The asking job is cancelled: the lane passes on, past the job taken off.
    for _ in 0..500 {
        if root.path().join("docs").join("new2").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        root.path().join("docs").join("new2").is_file(),
        "the next job ran"
    );
    assert!(
        !root.path().join("docs").join("new1").exists(),
        "the one taken off did not"
    );
    assert_eq!(
        fs::read(root.path().join("docs").join("a.txt")).expect("read"),
        b"old"
    );

    let cleared = entries(
        engine
            .dispatch(Command::QueueClear {
                list: later,
                all_lists: false,
            })
            .await
            .expect("cleared"),
    );
    assert!(!order(&cleared).contains(&second), "{cleared:?}");
    assert!(!root.path().join("docs").join("b.txt").exists());
    let all = entries(
        engine
            .dispatch(Command::QueueClear {
                list: rc_ops::DEFAULT_LIST,
                all_lists: true,
            })
            .await
            .expect("cleared"),
    );
    assert!(all.is_empty(), "{all:?}");
}
