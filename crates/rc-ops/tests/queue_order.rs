//! The order of the waiting jobs of one lane (RC-010-14): reordered, removed, paused, while the
//! job that holds the lane is paused inside its file.

mod support;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_ops::{
    JobChange, JobEvent, JobId, JobKind, JobOptions, JobSpec, JobState, Queue, QueueEntry, Reorder,
};
use rc_vfs::{BackendId, Vfs};
use support::{Harness, exists, mem, payload, write};

const SIZE: usize = 64 * 1024;

/// Copies `/big.bin` of `Memory(1)` into `dir` of `Memory(to)`.
fn copy_big(to: u32, dir: &str) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![mem(1, "/big.bin")],
            target: mem(to, dir),
        },
        options: JobOptions::default(),
    }
}

/// The harness with `/big.bin` on `Memory(1)` and empty `Memory(2)` and `Memory(3)`; the first
/// job to reach the middle of its file is paused there.
async fn start() -> (Harness, Vec<rc_vfs::MemoryFs>) {
    let harness = Harness::start();
    let one = harness.memory(1);
    let targets = vec![harness.memory(2), harness.memory(3)];
    write(&one, "/big.bin", &payload(SIZE, 5)).await;
    let paused: Arc<Mutex<bool>> = Arc::default();
    harness.on_event(move |queue, event| {
        let mut paused = paused.lock().expect("paused");
        if !*paused && mid_file(event) {
            *paused = true;
            queue.pause(event.job).expect("pause");
        }
    });
    (harness, targets)
}

fn mid_file(event: &JobEvent) -> bool {
    matches!(
        &event.change,
        JobChange::Progress { progress }
            if progress.current.is_some()
                && progress.bytes_done > 0
                && progress.bytes_done < SIZE as u64
    )
}

async fn wait_for_state(queue: &Queue, id: JobId, state: JobState) {
    for _ in 0..400 {
        if queue.job(id).expect("known").state == state {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job {id} never reached {state:?}");
}

/// The first job, paused inside its file on `Memory(2)`.
async fn holding(harness: &Harness) -> JobId {
    let id = harness
        .queue
        .submit(copy_big(2, "/a"))
        .expect("accepted")
        .id;
    wait_for_state(&harness.queue, id, JobState::Paused).await;
    id
}

/// The index of the first event of `job` with `change`.
fn position(events: &[JobEvent], job: JobId, change: &JobChange) -> usize {
    events
        .iter()
        .position(|event| event.job == job && &event.change == change)
        .unwrap_or_else(|| panic!("job {job} never sent {change:?}"))
}

fn place(entries: &[QueueEntry], job: JobId) -> Option<usize> {
    entries
        .iter()
        .find(|entry| entry.job.id == job)
        .and_then(|entry| entry.place)
}

/// The acceptance check: three jobs onto one target, the third pushed to the front starts next.
#[tokio::test]
async fn the_job_moved_to_the_front_starts_next() {
    let (harness, _targets) = start().await;
    let first = holding(&harness).await;
    let second = harness.queue.submit(copy_big(2, "/b")).expect("ok").id;
    let third = harness.queue.submit(copy_big(2, "/c")).expect("ok").id;

    let entries = harness.queue.entries();
    let order: Vec<JobId> = entries.iter().map(|entry| entry.job.id).collect();
    assert_eq!(order, [first, second, third]);
    assert_eq!(entries[0].lane, Some(BackendId::Memory(2)));
    assert_eq!(
        (place(&entries, second), place(&entries, third)),
        (Some(0), Some(1))
    );

    let entries = harness
        .queue
        .reorder(third, Reorder::Front)
        .expect("waiting");
    let order: Vec<JobId> = entries.iter().map(|entry| entry.job.id).collect();
    assert_eq!(order, [first, third, second], "the window's order follows");
    assert_eq!(place(&entries, third), Some(0));
    let refused = harness.queue.reorder(first, Reorder::Up).map(|_| ());
    assert_eq!(refused.map_err(|e| e.code()), Err("job_not_waiting"));

    harness.queue.resume(first).expect("resume");
    for id in [first, second, third] {
        assert_eq!(harness.finish(id).await.state, JobState::Completed);
    }
    let events = harness.events();
    let finished = JobChange::Finished {
        state: JobState::Completed,
    };
    assert!(position(&events, first, &finished) < position(&events, third, &JobChange::Started));
    assert!(
        position(&events, third, &finished) < position(&events, second, &JobChange::Started),
        "the second waits for the third"
    );
}

#[tokio::test]
async fn up_and_down_swap_neighbours_and_stop_at_the_ends() {
    let (harness, _targets) = start().await;
    let first = holding(&harness).await;
    let second = harness.queue.submit(copy_big(2, "/b")).expect("ok").id;
    let third = harness.queue.submit(copy_big(2, "/c")).expect("ok").id;
    let entries = harness
        .queue
        .reorder(second, Reorder::Up)
        .expect("first already");
    assert_eq!(place(&entries, second), Some(0));
    let entries = harness.queue.reorder(second, Reorder::Down).expect("ok");
    assert_eq!(
        (place(&entries, third), place(&entries, second)),
        (Some(0), Some(1))
    );
    let entries = harness
        .queue
        .reorder(second, Reorder::Down)
        .expect("last already");
    assert_eq!(place(&entries, second), Some(1));
    // The waiting ones first: cancelled after the first, they could have finished already.
    for id in [second, third, first] {
        harness.queue.cancel(id).expect("cancel");
    }
    for id in [first, second, third] {
        harness.finish(id).await;
    }
}

/// The acceptance check: a removed waiting job never starts, a cancelled running one leaves no
/// partial file.
#[tokio::test]
async fn a_removed_job_never_starts_and_a_cancelled_one_leaves_no_partial_file() {
    let (harness, targets) = start().await;
    let first = holding(&harness).await;
    let second = harness.queue.submit(copy_big(2, "/b")).expect("ok").id;
    let refused = harness.queue.remove(first).map(|_| ());
    assert_eq!(refused.map_err(|e| e.code()), Err("job_not_waiting"));

    let entries = harness.queue.remove(second).expect("waiting");
    assert!(entries.iter().all(|entry| entry.job.id != second));
    let gone = harness.queue.job(second).map(|_| ()).map_err(|e| e.code());
    assert_eq!(gone, Err("job_not_found"));
    harness.queue.cancel(first).expect("cancel");
    assert_eq!(harness.finish(first).await.state, JobState::Cancelled);

    let ended = JobChange::Finished {
        state: JobState::Cancelled,
    };
    for _ in 0..400 {
        if harness.changes(second).contains(&ended) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let changes = harness.changes(second);
    assert!(changes.contains(&ended), "{changes:?}");
    assert!(!changes.contains(&JobChange::Started), "{changes:?}");
    let two = &targets[0];
    assert!(!exists(two, "/a/big.bin").await, "no partial file");
    assert!(!exists(two, "/b/big.bin").await, "never copied");
}

/// The acceptance check: a paused job holds only its own lane.
#[tokio::test]
async fn a_paused_job_holds_only_its_own_lane() {
    let (harness, targets) = start().await;
    let first = holding(&harness).await;
    let before = harness.queue.job(first).expect("known").progress.bytes_done;
    let other = harness.run(copy_big(3, "/")).await;
    assert_eq!(other.state, JobState::Completed, "{other:?}");
    assert!(exists(&targets[1], "/big.bin").await);
    let held = harness.queue.job(first).expect("known");
    assert_eq!(held.state, JobState::Paused);
    assert_eq!(
        held.progress.bytes_done, before,
        "nothing moved while paused"
    );
    let size = targets[0]
        .stat(std::path::Path::new("/a/big.bin"))
        .await
        .expect("the file being written")
        .size;
    assert_eq!(size, before);
    harness.queue.cancel(first).expect("cancel");
    harness.finish(first).await;
}

#[tokio::test]
async fn a_paused_waiting_job_lets_the_next_go_first() {
    let (harness, _targets) = start().await;
    let first = holding(&harness).await;
    let second = harness.queue.submit(copy_big(2, "/b")).expect("ok").id;
    let third = harness.queue.submit(copy_big(2, "/c")).expect("ok").id;
    harness.queue.pause(second).expect("pause");
    harness.queue.resume(first).expect("resume");
    harness.finish(first).await;
    assert_eq!(harness.finish(third).await.state, JobState::Completed);
    assert_eq!(
        harness.queue.job(second).expect("known").state,
        JobState::Paused
    );
    assert!(!harness.changes(second).contains(&JobChange::Started));
    harness.queue.resume(second).expect("resume");
    assert_eq!(harness.finish(second).await.state, JobState::Completed);
}

#[tokio::test]
async fn pause_all_and_resume_all_reach_every_job_and_clear_finished_forgets() {
    let (harness, _targets) = start().await;
    let first = holding(&harness).await;
    let second = harness.queue.submit(copy_big(2, "/b")).expect("ok").id;
    let entries = harness.queue.pause_all();
    assert!(
        entries
            .iter()
            .all(|entry| entry.job.state == JobState::Paused),
        "{entries:?}"
    );
    let entries = harness.queue.resume_all();
    assert!(
        entries
            .iter()
            .all(|entry| entry.job.state != JobState::Paused)
    );
    harness.finish(first).await;
    harness.finish(second).await;
    assert_eq!(harness.queue.entries().len(), 2);
    assert!(harness.queue.clear_finished().is_empty());
}

/// A waiting job that is cancelled ends at once, while the job ahead of it still holds the lane.
#[tokio::test]
async fn a_cancelled_waiting_job_ends_without_waiting_for_the_lane() {
    let (harness, _targets) = start().await;
    let first = holding(&harness).await;
    let second = harness.queue.submit(copy_big(2, "/b")).expect("ok").id;
    harness.queue.cancel(second).expect("cancel");
    let ended = tokio::time::timeout(Duration::from_secs(2), harness.finish(second)).await;
    assert_eq!(ended.expect("ends at once").state, JobState::Cancelled);
    assert_eq!(
        harness.queue.job(first).expect("known").state,
        JobState::Paused
    );
    harness.queue.cancel(first).expect("cancel");
    harness.finish(first).await;
}
