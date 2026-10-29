//! How many jobs run at once (RC-060-05): a server's connection takes two, the others wait in
//! its lane - a download from it as much as an upload to it - and a limit for all lanes holds
//! the next job back even on a free lane.

mod support;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_ops::{JobChange, JobId, JobKind, JobOptions, JobSpec, JobState, Queue};
use rc_vfs::{BackendId, VfsPath};
use support::{Harness, mem, payload, write};

const SIZE: usize = 64 * 1024;
const SERVER: BackendId = BackendId::Sftp { connection_id: 1 };

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

fn copy(source: VfsPath, target: VfsPath) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![source],
            target,
        },
        options: JobOptions::default(),
    }
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

/// A harness whose jobs each pause inside their first file, holding their place; `/big.bin` on
/// `Memory(2)` and on the server.
async fn start() -> Harness {
    let harness = Harness::start();
    let local = harness.memory(2);
    let server = harness.memory_as(SERVER);
    write(&local, "/big.bin", &payload(SIZE, 5)).await;
    write(&server, "/big.bin", &payload(SIZE, 6)).await;
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
    harness
}

#[tokio::test]
async fn a_server_runs_two_jobs_and_the_third_waits() {
    let harness = start().await;
    let queue = &harness.queue;
    let first = queue
        .submit(copy(mem(2, "/big.bin"), on_server("/a")))
        .expect("ok")
        .id;
    let second = queue
        .submit(copy(on_server("/big.bin"), mem(2, "/down")))
        .expect("ok")
        .id;
    wait_for_state(queue, first, JobState::Paused).await;
    wait_for_state(queue, second, JobState::Paused).await;
    let third = queue
        .submit(copy(mem(2, "/big.bin"), on_server("/c")))
        .expect("ok")
        .id;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let entries = queue.entries();
    let entry = |id: JobId| {
        entries
            .iter()
            .find(|entry| entry.job.id == id)
            .expect("listed")
    };
    assert_eq!(
        entry(second).lane,
        Some(SERVER),
        "a download waits for the server"
    );
    assert_eq!(entry(third).job.state, JobState::Queued);
    assert_eq!(entry(third).place, Some(0), "the window shows it waiting");

    queue.cancel(first).expect("cancel");
    wait_for_state(queue, third, JobState::Paused).await;
    queue.resume(second).expect("resume");
    queue.resume(third).expect("resume");
    assert_eq!(harness.finish(second).await.state, JobState::Completed);
    assert_eq!(harness.finish(third).await.state, JobState::Completed);
}

#[tokio::test]
async fn the_limit_for_all_lanes_holds_a_job_on_a_free_lane() {
    let harness = start().await;
    let queue = &harness.queue;
    queue.set_transfer_limits(4096, 2, 1);
    harness.memory(3);
    let first = queue
        .submit(copy(mem(2, "/big.bin"), on_server("/a")))
        .expect("ok")
        .id;
    wait_for_state(queue, first, JobState::Paused).await;
    let second = queue
        .submit(copy(mem(2, "/big.bin"), mem(3, "/b")))
        .expect("ok")
        .id;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(queue.job(second).expect("known").state, JobState::Queued);

    queue.set_transfer_limits(4096, 2, 2);
    wait_for_state(queue, second, JobState::Paused).await;
    queue.resume(first).expect("resume");
    queue.resume(second).expect("resume");
    assert_eq!(harness.finish(first).await.state, JobState::Completed);
    assert_eq!(harness.finish(second).await.state, JobState::Completed);
}
