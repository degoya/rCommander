//! Several jobs at once (RC-010-15, owner 2026-10-18): the selected waiting jobs moved keeping
//! their order, a running job among the jobs taken off cancelled as a single cancel does it,
//! part of a batch taken while its siblings keep theirs, and a list or every list cleared.

mod support;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_ops::{
    AT_ONCE, DEFAULT_LIST, JobChange, JobId, JobKind, JobOptions, JobSpec, JobState, Queue,
    QueueEntry, QueueListId, Reorder, per_item,
};
use rc_vfs::MemoryFs;
use support::{Harness, exists, flaky::wait_until, mem, payload, write};

const SIZE: usize = 64 * 1024;

/// Copies `/big.bin` of `Memory(1)` into `dir` of `Memory(2)`.
fn copy_big(dir: &str) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![mem(1, "/big.bin")],
            target: mem(2, dir),
        },
        options: JobOptions::default(),
    }
}

/// `/big.bin` on `Memory(1)`, an empty `Memory(2)` (returned); every job pauses once inside its
/// file, holding the lane.
async fn start() -> (Harness, MemoryFs) {
    let harness = Harness::start();
    write(&harness.memory(1), "/big.bin", &payload(SIZE, 5)).await;
    let to = harness.memory(2);
    let paused: Arc<Mutex<HashSet<JobId>>> = Arc::default();
    harness.on_event(move |queue, event| {
        let inside = matches!(
            &event.change,
            JobChange::Progress { progress }
                if progress.current.is_some() && progress.bytes_done > 0
        );
        if inside && paused.lock().expect("paused").insert(event.job) {
            queue.pause(event.job).expect("pause");
        }
    });
    (harness, to)
}

async fn paused(queue: &Queue, id: JobId) {
    for _ in 0..400 {
        if queue.job(id).expect("known").state == JobState::Paused {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job {id} never paused");
}

fn submit(queue: &Queue, dir: &str, list: QueueListId) -> JobId {
    queue
        .submit_into(copy_big(dir), None, list)
        .expect("accepted")
        .id
}

/// The waiting jobs of `entries` in the order they start.
fn waiting(entries: &[QueueEntry]) -> Vec<JobId> {
    let mut places: Vec<(usize, JobId)> = entries
        .iter()
        .filter_map(|entry| entry.place.map(|place| (place, entry.job.id)))
        .collect();
    places.sort_unstable();
    places.into_iter().map(|(_, job)| job).collect()
}

fn listed(entries: &[QueueEntry]) -> Vec<JobId> {
    entries.iter().map(|entry| entry.job.id).collect()
}

/// Waits for the `finished` event of `job`, which the queue no longer lists; its state.
async fn ended(harness: &Harness, job: JobId) -> JobState {
    let end = || {
        harness
            .changes(job)
            .into_iter()
            .find_map(|change| match change {
                JobChange::Finished { state } => Some(state),
                _ => None,
            })
    };
    wait_until("the job ends", || end().is_some()).await;
    end().expect("ended")
}

/// The acceptance check of the order: the selected waiting jobs move up, down, to the front and
/// to the back keeping their order among themselves; the running job among them stays.
#[tokio::test]
async fn the_selected_jobs_move_keeping_their_order() {
    let (harness, _to) = start().await;
    let queue = &harness.queue;
    let holder = submit(queue, "/h", DEFAULT_LIST);
    paused(queue, holder).await;
    let [a, b, c, d] = ["/a", "/b", "/c", "/d"].map(|dir| submit(queue, dir, DEFAULT_LIST));
    assert_eq!(waiting(&queue.entries()), [a, b, c, d]);

    let entries = queue
        .reorder_many(&[holder, b, d], Reorder::Up)
        .expect("moved");
    assert_eq!(waiting(&entries), [b, a, d, c], "each one place up");
    let entries = queue.reorder_many(&[b, d], Reorder::Up).expect("moved");
    assert_eq!(
        waiting(&entries),
        [b, d, a, c],
        "the first holds, the next closes up"
    );
    let entries = queue.reorder_many(&[c, a], Reorder::Front).expect("moved");
    assert_eq!(
        waiting(&entries),
        [a, c, b, d],
        "as a block, in their order"
    );
    let entries = queue.reorder_many(&[a, b], Reorder::Back).expect("moved");
    assert_eq!(waiting(&entries), [c, d, a, b]);
    let entries = queue.reorder_many(&[c, d], Reorder::Down).expect("moved");
    assert_eq!(waiting(&entries), [a, c, d, b]);
    assert_eq!(
        queue.job(holder).expect("known").state,
        JobState::Paused,
        "the running one stays"
    );

    let unknown = queue.reorder_many(&[a, 999], Reorder::Front).map(|_| ());
    assert_eq!(unknown.map_err(|e| e.code()), Err("job_not_found"));
    assert_eq!(waiting(&queue.entries()), [a, c, d, b], "nothing moved");
    queue.clear(None).expect("cleared");
    ended(&harness, holder).await;
}

/// Taken off: the waiting job ends without having started, the paused one inside its file is
/// cancelled and leaves no partial file, the finished one is forgotten; none is listed.
#[tokio::test]
async fn taking_jobs_off_cancels_the_started_one_and_forgets_the_finished_one() {
    let (harness, to) = start().await;
    let queue = &harness.queue;
    let running = submit(queue, "/r", DEFAULT_LIST);
    paused(queue, running).await;
    let waiting_job = submit(queue, "/w", DEFAULT_LIST);
    let mkdir = queue
        .submit_into(
            JobSpec {
                kind: JobKind::Mkdir {
                    path: mem(1, "/made"),
                },
                options: JobOptions::default(),
            },
            None,
            DEFAULT_LIST,
        )
        .expect("accepted")
        .id;
    assert_eq!(harness.finish(mkdir).await.state, JobState::Completed);
    let kept = submit(queue, "/k", DEFAULT_LIST);

    let entries = queue
        .discard(&[running, waiting_job, mkdir])
        .expect("taken off");
    assert_eq!(listed(&entries), [kept]);
    assert_eq!(ended(&harness, running).await, JobState::Cancelled);
    assert_eq!(ended(&harness, waiting_job).await, JobState::Cancelled);
    assert!(
        !harness.changes(waiting_job).contains(&JobChange::Started),
        "the waiting one never started"
    );
    assert!(!exists(&to, "/r/big.bin").await, "no partial file");
    // The lane is free: the job left goes on.
    paused(queue, kept).await;
    let missing = queue.discard(&[kept, 999]).map(|_| ());
    assert_eq!(missing.map_err(|e| e.code()), Err("job_not_found"));
    assert_eq!(listed(&queue.entries()), [kept], "nothing taken off");
    queue.cancel(kept).expect("cancel");
    assert_eq!(harness.finish(kept).await.state, JobState::Cancelled);
}

/// Part of a batch moved to another list and part taken off: the siblings left keep their batch
/// and run.
#[tokio::test]
async fn a_selection_splits_a_batch_and_the_siblings_keep_theirs() {
    let harness = Harness::start();
    let from = harness.memory(1);
    let to = harness.memory(2);
    for name in ["a.txt", "b.txt", "c.txt"] {
        write(&from, &format!("/src/{name}"), b"new").await;
    }
    let held = queue_list(&harness.queue, "Held");
    let specs = per_item(JobSpec {
        kind: JobKind::Copy {
            sources: ["a.txt", "b.txt", "c.txt"]
                .map(|name| mem(1, &format!("/src/{name}")))
                .to_vec(),
            target: mem(2, "/out"),
        },
        options: JobOptions::default(),
    });
    let jobs: Vec<JobId> = harness
        .queue
        .submit_batch(specs, held)
        .expect("accepted")
        .iter()
        .map(|job| job.id)
        .collect();
    let [first, second, third] = jobs[..] else {
        panic!("three jobs, got {jobs:?}");
    };
    let batch = harness.queue.job(first).expect("known").batch;
    assert!(batch.is_some());

    let other = queue_list(&harness.queue, "Other");
    let entries = harness.queue.assign_many(&[third], other).expect("moved");
    let entry = |id: JobId| {
        entries
            .iter()
            .find(|entry| entry.job.id == id)
            .cloned()
            .expect("listed")
    };
    assert_eq!((entry(first).list, entry(third).list), (held, other));
    harness.queue.discard(&[second]).expect("taken off");
    assert_eq!(ended(&harness, second).await, JobState::Cancelled);
    for id in [first, third] {
        let job = harness.queue.job(id).expect("known");
        assert_eq!((job.batch, job.state), (batch, JobState::Queued));
    }
    harness.queue.start_list(held).expect("started");
    harness.queue.start_list(other).expect("started");
    for id in [first, third] {
        assert_eq!(harness.finish(id).await.state, JobState::Completed);
    }
    assert!(exists(&to, "/out/a.txt").await);
    assert!(!exists(&to, "/out/b.txt").await, "the one taken off");
    assert!(exists(&to, "/out/c.txt").await);
}

/// A list cleared takes its jobs off, finished or not, and leaves the other lists' and the ones
/// run at once; every list cleared takes them all.
#[tokio::test]
async fn a_list_is_cleared_and_then_every_list() {
    let (harness, _to) = start().await;
    let queue = &harness.queue;
    let stopped = queue_list(queue, "Stopped");
    let mine = [submit(queue, "/m1", stopped), submit(queue, "/m2", stopped)];
    let other = submit(queue, "/o", DEFAULT_LIST);
    paused(queue, other).await;
    let at_once = submit(queue, "/now", AT_ONCE);

    let entries = queue.clear(Some(stopped)).expect("cleared");
    assert_eq!(
        listed(&entries),
        [at_once, other],
        "run at once, it is listed first"
    );
    for id in mine {
        assert_eq!(ended(&harness, id).await, JobState::Cancelled);
    }
    let missing = queue.clear(Some(99)).map(|_| ());
    assert_eq!(missing.map_err(|e| e.code()), Err("queue_list_not_found"));

    let entries = queue.clear(None).expect("cleared");
    assert_eq!(listed(&entries), [at_once], "the job run at once stays");
    let at_once_list = queue.clear(Some(AT_ONCE)).map(|_| ());
    assert_eq!(
        at_once_list.map_err(|e| e.code()),
        Err("queue_list_not_found")
    );
    assert_eq!(ended(&harness, other).await, JobState::Cancelled);
    let alone = queue.job(at_once).expect("run at once, it stays");
    assert_ne!(alone.state, JobState::Cancelled);
    queue.cancel(at_once).expect("cancel");
    assert_eq!(harness.finish(at_once).await.state, JobState::Cancelled);
}

fn queue_list(queue: &Queue, name: &str) -> QueueListId {
    queue.create_list(name).expect("created").id
}
