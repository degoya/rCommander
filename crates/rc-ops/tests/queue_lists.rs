//! Several named lists in one queue (RC-010-15): lists on different targets run side by side,
//! lists on one target share its lane, a stopped list holds its jobs, each list keeps its own
//! order, and a list's jobs are taken out to save and loaded again.

mod support;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_ops::{
    DEFAULT_LIST, JobChange, JobId, JobKind, JobOptions, JobSpec, JobState, Queue, QueueEntry,
    QueueListId, Reorder,
};
use support::{Harness, mem, payload, write};

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

/// `/big.bin` on `Memory(1)`, empty `Memory(2)` and `Memory(3)`; with `hold_inside`, every job
/// pauses once inside its file, holding its lane.
async fn start(hold_inside: bool) -> Harness {
    let harness = Harness::start();
    write(&harness.memory(1), "/big.bin", &payload(SIZE, 5)).await;
    harness.memory(2);
    harness.memory(3);
    if hold_inside {
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
    }
    harness
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

fn submit(queue: &Queue, spec: JobSpec, list: QueueListId) -> JobId {
    queue.submit_into(spec, None, list).expect("accepted").id
}

fn entry(entries: &[QueueEntry], job: JobId) -> &QueueEntry {
    entries
        .iter()
        .find(|entry| entry.job.id == job)
        .unwrap_or_else(|| panic!("job {job} is listed"))
}

fn started_list(queue: &Queue, name: &str) -> QueueListId {
    let list = queue.create_list(name).expect("created");
    assert!(!list.started, "a new list starts stopped");
    queue.start_list(list.id).expect("started").id
}

/// The acceptance check: two lists on different targets run at once; two lists on one target
/// share its lane - the second list's job waits for the first one's.
#[tokio::test]
async fn lists_share_the_lane_of_their_target_and_nothing_else() {
    let harness = start(true).await;
    let queue = &harness.queue;
    let disk = started_list(queue, "Disk");
    let server = started_list(queue, "Server");
    let first = submit(queue, copy_big(2, "/a"), disk);
    let second = submit(queue, copy_big(3, "/b"), server);
    // Both reach the middle of their file: they ran side by side.
    wait_for_state(queue, first, JobState::Paused).await;
    wait_for_state(queue, second, JobState::Paused).await;

    let third = submit(queue, copy_big(2, "/c"), server);
    tokio::time::sleep(Duration::from_millis(30)).await;
    let entries = queue.entries();
    let waiting = entry(&entries, third);
    assert_eq!(waiting.list, server);
    assert_eq!(waiting.place, Some(0), "it waits for the other list's job");
    assert_eq!(waiting.job.state, JobState::Queued);

    queue.resume(first).expect("resume");
    assert_eq!(harness.finish(first).await.state, JobState::Completed);
    wait_for_state(queue, third, JobState::Paused).await;
    for id in [second, third] {
        queue.resume(id).expect("resume");
        assert_eq!(harness.finish(id).await.state, JobState::Completed);
    }
}

#[tokio::test]
async fn a_stopped_list_holds_its_jobs_and_lets_the_others_pass() {
    let harness = start(false).await;
    let queue = &harness.queue;
    let later = queue.create_list("Later").expect("created").id;
    let held = submit(queue, copy_big(2, "/held"), later);
    let other = submit(queue, copy_big(2, "/other"), DEFAULT_LIST);
    assert_eq!(harness.finish(other).await.state, JobState::Completed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(queue.job(held).expect("known").state, JobState::Queued);
    let lists = queue.lists();
    assert_eq!(lists.len(), 2);
    assert_eq!((lists[1].name.as_str(), lists[1].jobs), ("Later", 1));

    queue.start_list(later).expect("started");
    assert_eq!(harness.finish(held).await.state, JobState::Completed);
}

#[tokio::test]
async fn starting_a_list_resumes_its_paused_job() {
    let harness = start(true).await;
    let queue = &harness.queue;
    let list = started_list(queue, "Disk");
    let job = submit(queue, copy_big(2, "/a"), list);
    wait_for_state(queue, job, JobState::Paused).await;
    queue.pause_list(list).expect("stopped");
    assert_eq!(queue.job(job).expect("known").state, JobState::Paused);
    assert!(!queue.list(list).expect("listed").started);
    queue.start_list(list).expect("started");
    assert_eq!(harness.finish(job).await.state, JobState::Completed);
}

#[tokio::test]
async fn each_list_keeps_its_own_order_in_a_shared_lane() {
    let harness = start(true).await;
    let queue = &harness.queue;
    let (one, two) = (started_list(queue, "One"), started_list(queue, "Two"));
    let holder = submit(queue, copy_big(2, "/h"), one);
    wait_for_state(queue, holder, JobState::Paused).await;
    let x1 = submit(queue, copy_big(2, "/x1"), one);
    let y1 = submit(queue, copy_big(2, "/y1"), two);
    let x2 = submit(queue, copy_big(2, "/x2"), one);
    // Up passes over the other list's job: x2 takes x1's place.
    let entries = queue.reorder(x2, Reorder::Up).expect("waiting");
    let places: Vec<Option<usize>> = [x2, y1, x1]
        .iter()
        .map(|job| entry(&entries, *job).place)
        .collect();
    assert_eq!(places, [Some(0), Some(1), Some(2)]);
    let entries = queue.reorder(x2, Reorder::Up).expect("first of its list");
    assert_eq!(entry(&entries, x2).place, Some(0));
    for id in [x1, y1, x2, holder] {
        queue.cancel(id).expect("cancel");
    }
    for id in [x1, y1, x2, holder] {
        harness.finish(id).await;
    }
}

#[tokio::test]
async fn a_job_moves_between_lists_and_a_deleted_list_hands_its_jobs_on() {
    let harness = start(false).await;
    let queue = &harness.queue;
    let stopped = queue.create_list("Stopped").expect("created").id;
    let job = submit(queue, copy_big(2, "/a"), stopped);
    let other = queue.create_list("Other").expect("created").id;
    let entries = queue.assign(job, other).expect("moved");
    assert_eq!(entry(&entries, job).list, other);
    assert_eq!(queue.list(stopped).expect("listed").jobs, 0);

    let refused = queue.delete_list(DEFAULT_LIST).map(|_| ());
    assert_eq!(refused.map_err(|e| e.code()), Err("queue_list_fixed"));
    let renamed = queue.rename_list(other, " NAS ").expect("renamed");
    assert_eq!(renamed.name, "NAS");
    let missing = queue.assign(job, 99).map(|_| ());
    assert_eq!(missing.map_err(|e| e.code()), Err("queue_list_not_found"));
    // The default list is started: the job goes on.
    let lists = queue.delete_list(other).expect("deleted");
    assert!(lists.iter().all(|list| list.id != other));
    assert_eq!(harness.finish(job).await.state, JobState::Completed);
}

/// A list's copies are taken out in their order and loaded into a new, stopped list: waiting
/// until it starts, then run.
#[tokio::test]
async fn a_list_is_taken_out_and_loaded_again_waiting() {
    let harness = start(false).await;
    let queue = &harness.queue;
    let source = queue.create_list("Backup").expect("created").id;
    let first = submit(queue, copy_big(2, "/a"), source);
    let second = submit(queue, copy_big(3, "/b"), source);
    let delete = JobSpec {
        kind: JobKind::Delete {
            sources: vec![mem(3, "/gone")],
            mode: rc_ops::DeleteMode::Permanent,
        },
        options: JobOptions::default(),
    };
    let unsaved = submit(queue, delete.clone(), source);
    let (specs, left_out) = queue.list_specs(source).expect("listed");
    assert_eq!(
        specs,
        [(first, copy_big(2, "/a")), (second, copy_big(3, "/b"))]
    );
    assert_eq!(left_out, [unsaved]);
    let refused = queue.load(delete, source).map(|_| ());
    assert_eq!(refused.map_err(|e| e.code()), Err("unsupported"));

    let loaded = queue.create_list("Backup again").expect("created").id;
    let jobs: Vec<JobId> = specs
        .into_iter()
        .map(|(_, spec)| queue.load(spec, loaded).expect("loaded").id)
        .collect();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let entries = queue.entries();
    let states: Vec<JobState> = jobs
        .iter()
        .map(|job| entry(&entries, *job).job.state)
        .collect();
    assert_eq!(states, [JobState::Queued, JobState::Queued], "waiting");
    queue.start_list(loaded).expect("started");
    for id in jobs {
        assert_eq!(harness.finish(id).await.state, JobState::Completed);
    }
    for id in [first, second, unsaved] {
        queue.cancel(id).expect("cancel");
        harness.finish(id).await;
    }
}
