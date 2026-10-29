//! The jobs run at once in the queue's list (RC-010-25): F5, F6 or F8 confirmed with OK runs
//! its jobs at once, outside the lists; the queue lists them as a group of their own ahead of the
//! lists' jobs, marked, and pauses and resumes them with everything else - but never moves them,
//! puts them into a list or saves them.

mod support;

use rc_ops::{
    AT_ONCE, DEFAULT_LIST, JobId, JobKind, JobOptions, JobSpec, JobState, Queue, QueueEntry,
    Reorder, per_item,
};
use support::{Harness, flaky::wait_until, mem, write};

/// `a.txt`, `b.txt` and `c.txt` in `/src` of `Memory(1)`, each name holding `old` in `/out` of
/// `Memory(2)`: every copy into `/out` asks, and nobody answers.
async fn start() -> Harness {
    let harness = Harness::start();
    let from = harness.memory(1);
    let to = harness.memory(2);
    for name in ["a.txt", "b.txt", "c.txt"] {
        write(&from, &format!("/src/{name}"), b"new").await;
        write(&to, &format!("/out/{name}"), b"old").await;
    }
    harness
}

fn copy(names: &[&str], dir: &str) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: names
                .iter()
                .map(|name| mem(1, &format!("/src/{name}")))
                .collect(),
            target: mem(2, dir),
        },
        options: JobOptions::default(),
    }
}

async fn asks(queue: &Queue, job: JobId) {
    wait_until("the job asks", || {
        queue.job(job).is_ok_and(|job| job.question.is_some())
    })
    .await;
}

/// A queued job that asks and holds the lane of `Memory(2)`, one that waits behind it, and an
/// operation of two jobs run at once: the first asks, the second waits for its turn.
struct Jobs {
    holder: JobId,
    waiter: JobId,
    now: [JobId; 2],
}

async fn jobs(queue: &Queue) -> Jobs {
    let holder = queue.submit(copy(&["c.txt"], "/out")).expect("queued").id;
    asks(queue, holder).await;
    let waiter = queue.submit(copy(&["c.txt"], "/later")).expect("queued").id;
    let now: Vec<JobId> = queue
        .submit_batch(per_item(copy(&["a.txt", "b.txt"], "/out")), AT_ONCE)
        .expect("accepted")
        .iter()
        .map(|job| job.id)
        .collect();
    asks(queue, now[0]).await;
    Jobs {
        holder,
        waiter,
        now: [now[0], now[1]],
    }
}

fn ids(entries: &[QueueEntry]) -> Vec<JobId> {
    entries.iter().map(|entry| entry.job.id).collect()
}

fn code<T>(result: Result<T, rc_ops::OpsError>) -> Option<&'static str> {
    result.err().map(|error| error.code())
}

#[tokio::test]
async fn jobs_run_at_once_are_listed_first_marked_and_without_a_place() {
    let harness = start().await;
    let queue = &harness.queue;
    let Jobs {
        holder,
        waiter,
        now,
    } = jobs(queue).await;

    let entries = queue.entries();
    assert_eq!(ids(&entries), [now[0], now[1], holder, waiter]);
    let marked: Vec<bool> = entries.iter().map(|entry| entry.at_once).collect();
    assert_eq!(marked, [true, true, false, false]);
    assert!(
        entries[..2]
            .iter()
            .all(|entry| entry.list == AT_ONCE && entry.place.is_none())
    );
    assert_eq!(entries[3].place, Some(0), "the queued job keeps its place");
    // Without them - the queue window with `queue.show_at_once` off - the lists' jobs alone, as
    // before.
    let lists: Vec<QueueEntry> = entries.into_iter().filter(|e| !e.at_once).collect();
    assert_eq!(ids(&lists), [holder, waiter]);

    // A finished one goes with the finished ones, at the end.
    queue.cancel(now[0]).expect("cancelled");
    assert_eq!(harness.finish(now[0]).await.state, JobState::Cancelled);
    asks(queue, now[1]).await;
    let entries = queue.entries();
    assert_eq!(ids(&entries), [now[1], holder, waiter, now[0]]);
    assert!(entries[3].at_once);
}

#[tokio::test]
async fn jobs_run_at_once_are_not_moved_put_into_a_list_or_saved() {
    let harness = start().await;
    let queue = &harness.queue;
    let Jobs { waiter, now, .. } = jobs(queue).await;
    let list = queue.create_list("Later").expect("created").id;

    assert_eq!(
        code(queue.reorder(now[1], Reorder::Front)),
        Some("job_at_once")
    );
    assert_eq!(code(queue.remove(now[1])), Some("job_at_once"));
    assert_eq!(code(queue.assign(now[1], list)), Some("job_at_once"));
    assert_eq!(
        code(queue.assign_many(&[waiter, now[1]], list)),
        Some("job_at_once")
    );
    assert_eq!(
        code(queue.check_movable(&[waiter, now[0]])),
        Some("job_at_once")
    );
    assert_eq!(code(queue.check_movable(&[waiter])), None);
    let unmoved = queue.entries();
    let waiting = unmoved.iter().find(|entry| entry.job.id == waiter);
    assert_eq!(
        waiting.map(|entry| entry.list),
        Some(DEFAULT_LIST),
        "nothing moved"
    );

    // Nothing goes into the jobs run at once either, and they are no list to save or clear.
    assert_eq!(
        code(queue.assign(waiter, AT_ONCE)),
        Some("queue_list_not_found")
    );
    assert_eq!(
        code(queue.assign_many(&[waiter], AT_ONCE)),
        Some("queue_list_not_found")
    );
    assert_eq!(
        code(queue.list_specs(AT_ONCE)),
        Some("queue_list_not_found")
    );
    let (specs, left_out) = queue.list_specs(DEFAULT_LIST).expect("specs");
    let saved: Vec<JobId> = specs.iter().map(|(job, _)| *job).collect();
    assert!(!saved.iter().chain(&left_out).any(|job| now.contains(job)));

    // Moved with others, they stay where they are, as running jobs do.
    let moved = queue
        .reorder_many(&[now[1], waiter], Reorder::Front)
        .expect("moved");
    assert_eq!(ids(&moved)[..2], now);
}

#[tokio::test]
async fn pause_all_and_resume_all_reach_the_jobs_run_at_once() {
    let harness = start().await;
    let queue = &harness.queue;
    let Jobs { waiter, now, .. } = jobs(queue).await;
    let state = |job: JobId| queue.job(job).expect("known").state;
    assert_eq!(state(now[1]), JobState::Queued, "waits for its turn");

    let entries = queue.pause_all();
    let paused = |job: JobId| {
        entries
            .iter()
            .find(|entry| entry.job.id == job)
            .map(|entry| entry.job.state)
    };
    assert_eq!(paused(now[1]), Some(JobState::Paused));
    assert_eq!(paused(waiter), Some(JobState::Paused));

    let entries = queue.resume_all();
    let resumed = entries.iter().find(|entry| entry.job.id == now[1]);
    assert_eq!(resumed.map(|entry| entry.job.state), Some(JobState::Queued));
    assert_eq!(state(waiter), JobState::Queued);
}
