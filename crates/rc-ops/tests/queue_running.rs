//! The owner's queue test list against jobs that really run (RC-080-14), not jobs paused in their
//! file as `queue_order.rs` and `remote_lanes.rs` hold them: a server with limited bandwidth
//! keeps three downloads busy - with the default limits two run and the third waits; one
//! cancelled while it runs leaves nothing behind and the waiting one starts in its place.

mod support;

use std::{path::Path, sync::Arc, time::Duration};

use rc_ops::{
    Backend, DEFAULT_MAX_RUNNING, DEFAULT_PER_CONNECTION, JobId, JobKind, JobOptions, JobSpec,
    JobState, Queue,
};
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsPath};
use support::{Harness, mem, payload, read, slow::Slow, write};

const SERVER: BackendId = BackendId::Sftp { connection_id: 1 };
const SIZE: usize = 48 * 1024;
/// 16 KiB a second: a file of [`SIZE`] takes three seconds to read, long enough to watch it run.
const RATE: u64 = 16 * 1024;

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

fn download(name: &str) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![on_server(&format!("/srv/{name}"))],
            target: mem(2, "/down"),
        },
        options: JobOptions::default(),
    }
}

async fn wait_for(queue: &Queue, id: JobId, done: impl Fn(&rc_ops::JobSnapshot) -> bool) {
    for _ in 0..1000 {
        if done(&queue.job(id).expect("known")) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job {id}: {:?}", queue.job(id));
}

/// A job that moves bytes: running and past its first read.
fn moving(job: &rc_ops::JobSnapshot) -> bool {
    job.state == JobState::Running && job.progress.bytes_done > 0
}

/// The names in `dir` of `fs`, sorted; none when it does not exist.
async fn names(fs: &MemoryFs, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = fs
        .list(Path::new(dir))
        .await
        .map(|entries| entries.into_iter().map(|entry| entry.name).collect())
        .unwrap_or_default();
    names.sort();
    names
}

#[tokio::test]
async fn three_downloads_from_one_server_two_run_and_the_third_waits_for_a_cancel() {
    let harness = Harness::start();
    let config = harness.queue.config();
    assert_eq!(
        (config.per_connection, config.max_running),
        (DEFAULT_PER_CONNECTION, DEFAULT_MAX_RUNNING)
    );
    assert_eq!((DEFAULT_PER_CONNECTION, DEFAULT_MAX_RUNNING), (2, 4));
    let here = harness.memory(2);
    let server = Slow {
        fs: MemoryFs::new(),
        rate: RATE,
    };
    harness
        .queue
        .register(SERVER, Backend::new(Arc::new(server.clone())));
    for (seed, name) in [(1, "a.bin"), (2, "b.bin"), (3, "c.bin")] {
        write(&server.fs, &format!("/srv/{name}"), &payload(SIZE, seed)).await;
    }

    let queue = &harness.queue;
    let first = queue.submit(download("a.bin")).expect("ok").id;
    let second = queue.submit(download("b.bin")).expect("ok").id;
    let third = queue.submit(download("c.bin")).expect("ok").id;
    wait_for(queue, first, moving).await;
    wait_for(queue, second, moving).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let entries = queue.entries();
    let entry = |id: JobId| {
        entries
            .iter()
            .find(|entry| entry.job.id == id)
            .expect("listed")
    };
    for id in [first, second] {
        assert_eq!(entry(id).job.state, JobState::Running, "{entries:?}");
        assert_eq!(entry(id).place, None, "a running job waits for nothing");
    }
    assert_eq!(entry(third).job.state, JobState::Queued, "{entries:?}");
    assert_eq!(
        entry(third).lane,
        Some(SERVER),
        "a download waits for its server"
    );
    assert_eq!(entry(third).place, Some(0));
    assert_eq!(entry(third).job.progress.bytes_done, 0);

    // Cancelled while it runs - not paused first: no file and no part file stays behind.
    let running = queue.job(first).expect("known");
    assert!(moving(&running), "{running:?}");
    assert!(
        running.progress.bytes_done < SIZE as u64,
        "still inside its file"
    );
    queue.cancel(first).expect("cancel");
    assert_eq!(harness.finish(first).await.state, JobState::Cancelled);
    assert!(
        names(&here, "/down")
            .await
            .iter()
            .all(|name| !name.starts_with("a.bin")),
        "{:?}",
        names(&here, "/down").await
    );

    wait_for(queue, third, moving).await;
    for (id, name, seed) in [(second, "b.bin", 2), (third, "c.bin", 3)] {
        assert_eq!(harness.finish(id).await.state, JobState::Completed);
        assert_eq!(
            read(&here, &format!("/down/{name}")).await,
            payload(SIZE, seed)
        );
    }
    assert_eq!(names(&here, "/down").await, ["b.bin", "c.bin"]);
}
