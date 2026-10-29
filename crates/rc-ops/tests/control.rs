//! Pause, resume and cancel inside a file, and the order of jobs on one target backend.

mod support;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_ops::{
    JobChange, JobEvent, JobId, JobKind, JobOptions, JobSpec, JobState, Queue, Scheduling,
};
use rc_vfs::{Vfs, VfsPath};
use support::{Harness, exists, mem, payload, read, write};

const SIZE: usize = 64 * 1024;

fn copy_big(to: u32) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![mem(1, "/big.bin")],
            target: mem(to, "/"),
        },
        options: JobOptions::default(),
    }
}

/// The bytes done of a progress event in the middle of a file.
fn mid_file(event: &JobEvent) -> Option<u64> {
    match &event.change {
        JobChange::Progress { progress }
            if progress.current.is_some()
                && progress.bytes_done > 0
                && progress.bytes_done < SIZE as u64 =>
        {
            Some(progress.bytes_done)
        }
        _ => None,
    }
}

/// Acts once, at the first progress event inside the file; the bytes done then.
fn at_mid_file(
    harness: &Harness,
    act: impl Fn(&Queue, JobId) + Send + 'static,
) -> Arc<Mutex<Option<u64>>> {
    let seen: Arc<Mutex<Option<u64>>> = Arc::default();
    let record = Arc::clone(&seen);
    harness.on_event(move |queue, event| {
        let mut seen = record.lock().expect("seen");
        if seen.is_none()
            && let Some(done) = mid_file(event)
        {
            *seen = Some(done);
            act(queue, event.job);
        }
    });
    seen
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

#[tokio::test]
async fn pause_holds_inside_a_file_and_resume_finishes_it() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/big.bin", &payload(SIZE, 5)).await;
    let paused_at = at_mid_file(&harness, |queue, job| {
        queue.pause(job).expect("pause");
    });
    let id = harness.queue.submit(copy_big(2)).expect("accepted").id;
    wait_for_state(&harness.queue, id, JobState::Paused).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let at = paused_at.lock().expect("seen").expect("paused mid-file");
    let snapshot = harness.queue.job(id).expect("known");
    assert_eq!(snapshot.state, JobState::Paused);
    assert_eq!(
        snapshot.progress.bytes_done, at,
        "nothing moves while paused"
    );
    assert_eq!(
        harness.queue.job_progress(id).expect("known"),
        (snapshot.state, snapshot.progress.clone()),
        "a progress row reads what the snapshot has (RC-080-45)"
    );
    let partial = two
        .stat(std::path::Path::new("/big.bin"))
        .await
        .expect("the file being written");
    assert_eq!(partial.size, at);
    assert!(harness.changes(id).contains(&JobChange::Paused));

    harness.queue.resume(id).expect("resume");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&two, "/big.bin").await, payload(SIZE, 5));
    assert!(harness.changes(id).contains(&JobChange::Resumed));
}

#[tokio::test]
async fn cancel_inside_a_file_leaves_no_partial_file() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/big.bin", &payload(SIZE, 5)).await;
    let cancelled_at = at_mid_file(&harness, |queue, job| {
        queue.cancel(job).expect("cancel");
    });
    let done = harness.run(copy_big(2)).await;
    assert!(cancelled_at.lock().expect("seen").is_some());
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert!(
        !exists(&two, "/big.bin").await,
        "the partial file is removed"
    );
    assert!(exists(&one, "/big.bin").await, "the source stays");
}

#[tokio::test]
async fn cancel_inside_a_local_file_leaves_no_partial_file() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("big.bin");
    std::fs::write(&source, payload(SIZE, 9)).expect("write");
    let harness = Harness::start();
    harness.local();
    at_mid_file(&harness, |queue, job| {
        queue.cancel(job).expect("cancel");
    });
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![VfsPath::local(&source)],
                target: VfsPath::local(root.path().join("out")),
            },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert!(!root.path().join("out").join("big.bin").exists());
}

#[tokio::test]
async fn cancel_ends_a_paused_job_without_a_partial_file() {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    write(&one, "/big.bin", &payload(SIZE, 5)).await;
    at_mid_file(&harness, |queue, job| {
        queue.pause(job).expect("pause");
    });
    let id = harness.queue.submit(copy_big(2)).expect("accepted").id;
    wait_for_state(&harness.queue, id, JobState::Paused).await;
    harness.queue.cancel(id).expect("cancel");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Cancelled);
    assert!(!exists(&two, "/big.bin").await);
    let again = harness.queue.pause(id).map(|_| ()).map_err(|e| e.code());
    assert_eq!(again, Err("job_finished"));
}

/// The index of the first event of `job` with `change`.
fn position(events: &[JobEvent], job: JobId, change: &JobChange) -> usize {
    events
        .iter()
        .position(|event| event.job == job && &event.change == change)
        .unwrap_or_else(|| panic!("job {job} never sent {change:?}"))
}

#[tokio::test]
async fn jobs_on_one_target_run_one_after_another() {
    let harness = Harness::start();
    let one = harness.memory(1);
    harness.memory(2);
    write(&one, "/big.bin", &payload(SIZE, 5)).await;
    at_mid_file(&harness, |queue, job| {
        queue.pause(job).expect("pause");
    });
    let first = harness.queue.submit(copy_big(2)).expect("accepted").id;
    wait_for_state(&harness.queue, first, JobState::Paused).await;
    let mut second_spec = copy_big(2);
    second_spec.kind = JobKind::Copy {
        sources: vec![mem(1, "/big.bin")],
        target: mem(2, "/other"),
    };
    let second = harness.queue.submit(second_spec).expect("accepted").id;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        harness.queue.job(second).expect("known").state,
        JobState::Queued,
        "waits for the paused job on the same target"
    );
    harness.queue.resume(first).expect("resume");
    harness.finish(first).await;
    harness.finish(second).await;
    let events = harness.events();
    let finished = JobChange::Finished {
        state: JobState::Completed,
    };
    assert!(
        position(&events, first, &finished) < position(&events, second, &JobChange::Started),
        "the second starts after the first has finished"
    );
}

#[tokio::test]
async fn parallel_scheduling_starts_at_once() {
    let harness = Harness::start();
    harness.queue.set_scheduling(Scheduling::Parallel);
    let one = harness.memory(1);
    harness.memory(2);
    write(&one, "/big.bin", &payload(SIZE, 5)).await;
    at_mid_file(&harness, |queue, job| {
        queue.pause(job).expect("pause");
    });
    let first = harness.queue.submit(copy_big(2)).expect("accepted").id;
    wait_for_state(&harness.queue, first, JobState::Paused).await;
    let second = harness
        .run(JobSpec {
            kind: JobKind::Mkdir { path: mem(2, "/x") },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(
        second.state,
        JobState::Completed,
        "ran beside the paused job"
    );
    harness.queue.cancel(first).expect("cancel");
    harness.finish(first).await;
}
