//! The throughput of a copy between two servers (RC-060-06): two servers in this process with
//! limited bandwidth, timed on tokio's paused clock, so the measure is the same on every machine.
//! The copy from one to the other must reach the slower of the two single ways - one server to
//! this machine, this machine to the other - less [`DISCOUNT`]. A copy that read and wrote in
//! turn would reach about half of it.

mod support;

use std::sync::Arc;

use rc_ops::{Backend, JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, MemoryFs, VfsPath};
use support::{Harness, mem, payload, read, slow::Slow, write};
use tokio::time::Instant;

const SIZE: usize = 2 * 1024 * 1024;
/// How far the copy between the servers may fall behind the slower single way.
const DISCOUNT: f64 = 0.2;
const ONE: BackendId = BackendId::Sftp { connection_id: 1 };
const TWO: BackendId = BackendId::Sftp { connection_id: 2 };

fn at(backend: &BackendId, path: &str) -> VfsPath {
    VfsPath {
        backend: backend.clone(),
        path: path.into(),
    }
}

/// Bytes a second of copying `source` into `target`.
async fn rate(harness: &Harness, source: VfsPath, target: VfsPath) -> f64 {
    let started = Instant::now();
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![source],
                target,
            },
            options: JobOptions::default(),
        })
        .await;
    let took = started.elapsed();
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.progress.bytes_done, SIZE as u64);
    SIZE as f64 / took.as_secs_f64().max(f64::EPSILON)
}

#[tokio::test(start_paused = true)]
async fn a_copy_between_two_slow_servers_keeps_the_pace_of_the_slower_way() {
    let harness = Harness::start();
    let here = harness.memory(9);
    let one = Slow {
        fs: MemoryFs::new(),
        rate: 8 << 20,
    };
    let two = Slow {
        fs: MemoryFs::new(),
        rate: 6 << 20,
    };
    harness
        .queue
        .register(ONE, Backend::new(Arc::new(one.clone())));
    harness
        .queue
        .register(TWO, Backend::new(Arc::new(two.clone())));
    let data = payload(SIZE, 21);
    write(&one.fs, "/srv/big.bin", &data).await;
    write(&here, "/up/big.bin", &data).await;

    let down = rate(&harness, at(&ONE, "/srv/big.bin"), mem(9, "/down")).await;
    let up = rate(&harness, mem(9, "/up/big.bin"), at(&TWO, "/single")).await;
    let across = rate(&harness, at(&ONE, "/srv/big.bin"), at(&TWO, "/across")).await;

    let slower = down.min(up);
    assert!(
        across >= slower * (1.0 - DISCOUNT),
        "{:.2} MiB/s between the servers, {:.2} MiB/s the slower single way",
        across / f64::from(1 << 20),
        slower / f64::from(1 << 20)
    );
    assert_eq!(read(&two.fs, "/across/big.bin").await, data);
}
