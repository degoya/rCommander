//! A process killed with three copies in its queue (RC-080-04), over the database file: the first
//! engine's runtime is shut down while one copy writes its part and two wait - nothing ends,
//! nothing is cleaned up, the database handles are never closed -, then a second engine opens the
//! same file, puts the three jobs back and resumes them to byte-identical files.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use rc_db::Db;
use rc_ops::{Backend, JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, VfsPath, testing::Trickle};

use crate::test_support;
use crate::{Command, Engine};

const FILES: [(&str, usize); 3] = [
    ("one.bin", 2 * 1024 * 1024 + 7),
    ("two.bin", 2 * 1024 * 1024 + 1031),
    ("three.bin", 1024 * 1024 + 5),
];

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime")
}

fn bytes(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|at| (at % 253) as u8 ^ seed).collect()
}

fn part_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".rcpart");
    path.with_file_name(name)
}

async fn engine_on(db: &Db, root: &Path) -> Engine {
    let engine = test_support::open(root, root).await;
    assert!(engine.attach_db(db.clone()));
    engine
}

#[test]
fn a_killed_process_leaves_its_copies_to_resume_after_the_restart() {
    let root = tempfile::tempdir().expect("temp dir");
    let file = root.path().join("data").join("rcommander.db");
    let (src, out) = (root.path().join("src"), root.path().join("out"));
    std::fs::create_dir(&src).expect("mkdir");
    let mut copies = Vec::new();
    for (seed, (name, len)) in (1_u8..).zip(FILES) {
        let data = bytes(len, seed);
        std::fs::write(src.join(name), &data).expect("write");
        copies.push((out.join(name), data));
    }

    let first = runtime();
    let leftovers = first.block_on(async {
        let db = Db::open(&file).await.expect("db");
        let engine = engine_on(&db, root.path()).await;
        let local = Backend::new(Arc::new(Trickle::default()));
        engine.ops().register(BackendId::Local, local);
        for (name, _) in FILES {
            let spec = JobSpec {
                kind: JobKind::Copy {
                    sources: vec![VfsPath::local(src.join(name))],
                    target: VfsPath::local(&out),
                },
                options: JobOptions::default(),
            };
            engine.ops().submit(spec).expect("accepted");
        }
        // One copy at a time on one disk: the first well into its file, two waiting, all kept.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let kept = db.transfers().await.expect("rows").len();
            let started = std::fs::metadata(part_of(&copies[0].0)).map_or(0, |meta| meta.len());
            if kept == FILES.len() && started >= 256 * 1024 {
                break;
            }
            assert!(Instant::now() < deadline, "the first copy never got going");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        (engine, db)
    });
    // The kill: neither the engine nor the database is told; nothing more runs in that runtime.
    std::mem::forget(leftovers);
    first.shutdown_background();

    for (target, data) in &copies {
        assert!(
            !target.exists() || std::fs::read(target).expect("read") == *data,
            "{} is incomplete under its own name",
            target.display()
        );
    }
    assert!(
        part_of(&copies[0].0).exists(),
        "the running copy left its part"
    );

    let second = runtime();
    second.block_on(async {
        let db = Db::open(&file).await.expect("the same database again");
        let engine = engine_on(&db, root.path()).await;
        let restored = engine.restore_transfers().await.expect("restore");
        assert_eq!(restored.len(), FILES.len(), "all three jobs are back");
        for job in &restored {
            assert_eq!(job.state, JobState::Paused);
            assert!(job.resumable);
            engine
                .dispatch(Command::OpsResume { job: job.id })
                .await
                .expect("resume");
        }
        for job in &restored {
            let done = tokio::time::timeout(Duration::from_secs(30), engine.ops().wait(job.id))
                .await
                .expect("in time")
                .expect("known");
            assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
        }
        assert!(db.transfers().await.expect("rows").is_empty());
    });
    for (target, data) in &copies {
        assert_eq!(&std::fs::read(target).expect("read"), data);
        assert!(!part_of(target).exists());
    }
}
