//! One operation as one job per source (RC-010-14): the jobs of a batch side by side in their
//! lane and in their order, an answer "for all" and a cancel reaching the siblings, and a batch
//! kept by the store in its order and put back as siblings after a restart.

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rc_ops::{
    AT_ONCE, Answer, BatchId, ConflictChoice, DEFAULT_LIST, FileMark, JobChange, JobId, JobKind,
    JobOptions, JobSpec, JobState, SavedJob, TransferStore, per_item,
};
use rc_vfs::{BackendId, VfsPath};
use support::{Harness, flaky::wait_until, mem, read, write};

/// Copies `names` of `/src` on `Memory(1)` into `/out` of `Memory(2)`, one job per source.
fn copies(names: &[&str]) -> Vec<JobSpec> {
    per_item(JobSpec {
        kind: JobKind::Copy {
            sources: names
                .iter()
                .map(|name| mem(1, &format!("/src/{name}")))
                .collect(),
            target: mem(2, "/out"),
        },
        options: JobOptions::default(),
    })
}

/// `a.txt`, `b.txt` and `c.txt` in `/src` of `Memory(1)`; with `taken`, the same names hold
/// `old` in `/out` of `Memory(2)`.
async fn start(taken: bool) -> (Harness, rc_vfs::MemoryFs) {
    let harness = Harness::start();
    let from = harness.memory(1);
    let to = harness.memory(2);
    for name in ["a.txt", "b.txt", "c.txt"] {
        write(&from, &format!("/src/{name}"), b"new").await;
        if taken {
            write(&to, &format!("/out/{name}"), b"old").await;
        }
    }
    (harness, to)
}

fn questions(harness: &Harness, jobs: &[JobId]) -> usize {
    jobs.iter()
        .flat_map(|job| harness.changes(*job))
        .filter(|change| matches!(change, JobChange::Question { .. }))
        .count()
}

#[tokio::test]
async fn a_batch_runs_side_by_side_in_the_order_selected() {
    let (harness, to) = start(false).await;
    let batch = harness
        .queue
        .submit_batch(copies(&["a.txt", "b.txt", "c.txt"]), DEFAULT_LIST)
        .expect("accepted");
    // a.txt again: the first sibling's copy is there by then, so it overwrites without asking.
    let mut again = copies(&["a.txt"]).remove(0);
    again.options.on_conflict = Some(ConflictChoice::Overwrite);
    let after = harness.queue.submit(again).expect("accepted").id;
    assert_eq!(batch.len(), 3);
    let shared = batch[0].batch;
    assert!(shared.is_some());
    assert!(batch.iter().all(|job| job.batch == shared), "{batch:?}");
    let ids: Vec<JobId> = batch.iter().map(|job| job.id).collect();
    for (job, name) in batch.iter().zip(["a.txt", "b.txt", "c.txt"]) {
        assert!(
            matches!(&job.kind, JobKind::Copy { sources, .. } if sources == &[mem(1, &format!("/src/{name}"))]),
            "{job:?}"
        );
    }
    for id in ids.iter().chain([&after]) {
        assert_eq!(harness.finish(*id).await.state, JobState::Completed);
    }
    // One lane: the siblings started one after the other, then the job submitted after them.
    let started: Vec<JobId> = harness
        .events()
        .into_iter()
        .filter(|event| event.change == JobChange::Started)
        .map(|event| event.job)
        .collect();
    assert_eq!(started, [ids[0], ids[1], ids[2], after]);
    assert_eq!(read(&to, "/out/c.txt").await, b"new");
    assert_eq!(harness.queue.job(after).expect("known").batch, None);
}

#[tokio::test]
async fn an_answer_for_all_reaches_the_siblings() {
    let (harness, to) = start(true).await;
    harness.on_event(|queue, event| {
        if let JobChange::Question { .. } = event.change {
            let answer = Answer::Conflict {
                choice: ConflictChoice::Overwrite,
                for_all: true,
            };
            queue.answer(event.job, answer).expect("answered");
        }
    });
    let ids: Vec<JobId> = harness
        .queue
        .submit_batch(copies(&["a.txt", "b.txt", "c.txt"]), DEFAULT_LIST)
        .expect("accepted")
        .iter()
        .map(|job| job.id)
        .collect();
    for id in &ids {
        assert_eq!(harness.finish(*id).await.state, JobState::Completed);
    }
    assert_eq!(questions(&harness, &ids), 1, "asked once for the operation");
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert_eq!(read(&to, &format!("/out/{name}")).await, b"new", "{name}");
    }
}

#[tokio::test]
async fn cancel_answered_to_one_job_stops_its_siblings() {
    let (harness, to) = start(true).await;
    harness.on_event(|queue, event| {
        if let JobChange::Question { .. } = event.change {
            let answer = Answer::Conflict {
                choice: ConflictChoice::Cancel,
                for_all: false,
            };
            queue.answer(event.job, answer).expect("answered");
        }
    });
    let ids: Vec<JobId> = harness
        .queue
        .submit_batch(copies(&["a.txt", "b.txt", "c.txt"]), DEFAULT_LIST)
        .expect("accepted")
        .iter()
        .map(|job| job.id)
        .collect();
    for id in &ids {
        assert_eq!(harness.finish(*id).await.state, JobState::Cancelled);
    }
    assert_eq!(questions(&harness, &ids), 1);
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert_eq!(read(&to, &format!("/out/{name}")).await, b"old", "{name}");
    }
}

#[tokio::test]
async fn cancelling_one_job_of_a_batch_stops_only_that_one() {
    let (harness, to) = start(false).await;
    // A new list is stopped: it holds the jobs until it starts.
    let held = harness.queue.create_list("held").expect("made").id;
    let ids: Vec<JobId> = harness
        .queue
        .submit_batch(copies(&["a.txt", "b.txt"]), held)
        .expect("accepted")
        .iter()
        .map(|job| job.id)
        .collect();
    harness.queue.cancel(ids[0]).expect("cancelled");
    assert_eq!(harness.finish(ids[0]).await.state, JobState::Cancelled);
    harness.queue.start_list(held).expect("started");
    assert_eq!(harness.finish(ids[1]).await.state, JobState::Completed);
    assert_eq!(read(&to, "/out/b.txt").await, b"new");
}

const SERVER: BackendId = BackendId::Sftp { connection_id: 7 };

/// A store that numbers its records and keeps each job with its batch.
#[derive(Default)]
struct Kept {
    rows: Mutex<Vec<(i64, JobSpec, Option<BatchId>)>>,
}

#[async_trait]
impl TransferStore for Kept {
    async fn save(&self, spec: &JobSpec) -> Option<i64> {
        self.save_in(spec, None).await
    }
    async fn save_in(&self, spec: &JobSpec, batch: Option<BatchId>) -> Option<i64> {
        let mut rows = self.rows.lock().expect("rows");
        let record = i64::try_from(rows.len()).expect("few") + 1;
        rows.push((record, spec.clone(), batch));
        Some(record)
    }
    async fn mark(&self, _record: i64, _mark: &FileMark) {}
    async fn forget(&self, _record: i64) {}
}

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

#[tokio::test]
async fn a_batch_is_kept_in_its_order_and_comes_back_as_siblings() {
    // The run before the restart: an upload of three files, held by a stopped list.
    let before = Harness::start();
    let store = Arc::new(Kept::default());
    before
        .queue
        .set_store(Arc::clone(&store) as Arc<dyn TransferStore>);
    let from = before.memory(1);
    before.memory_as(SERVER);
    for name in ["a.txt", "b.txt", "c.txt"] {
        write(&from, &format!("/src/{name}"), b"new").await;
    }
    let held = before.queue.create_list("held").expect("made").id;
    let upload = per_item(JobSpec {
        kind: JobKind::Copy {
            sources: ["a.txt", "b.txt", "c.txt"]
                .iter()
                .map(|name| mem(1, &format!("/src/{name}")))
                .collect(),
            target: on_server("/up"),
        },
        options: JobOptions::default(),
    });
    let batch = before
        .queue
        .submit_batch(upload.clone(), held)
        .expect("accepted");
    let rows = Arc::clone(&store);
    wait_until("all three are kept", || {
        rows.rows.lock().expect("rows").len() == 3
    })
    .await;
    let rows = store.rows.lock().expect("rows").clone();
    let specs: Vec<JobSpec> = rows.iter().map(|(_, spec, _)| spec.clone()).collect();
    assert_eq!(specs, upload, "kept in their order");
    assert!(rows.iter().all(|(_, _, kept)| *kept == batch[0].batch));

    // The next start: the kept rows put back as one batch, paused.
    let after = Harness::start();
    let from = after.memory(1);
    let server = after.memory_as(SERVER);
    for name in ["a.txt", "b.txt", "c.txt"] {
        write(&from, &format!("/src/{name}"), b"new").await;
    }
    let saved = rows
        .into_iter()
        .map(|(record, spec, _)| SavedJob {
            record,
            spec,
            mark: None,
        })
        .collect();
    let restored: Vec<_> = after
        .queue
        .restore_batch(saved)
        .into_iter()
        .map(|job| job.expect("restored"))
        .collect();
    assert_eq!(restored.len(), 3);
    assert!(restored.iter().all(|job| job.state == JobState::Paused));
    let shared = restored[0].batch;
    assert!(shared.is_some());
    assert!(restored.iter().all(|job| job.batch == shared));
    let _entries = after.queue.resume_all();
    for job in &restored {
        assert_eq!(after.finish(job.id).await.state, JobState::Completed);
    }
    assert_eq!(read(&server, "/up/c.txt").await, b"new");
}

/// Confirmed with OK, not queued with F2 (owner, 2026-10-18): the jobs run at once - also while
/// another job holds their lane -, one after the other, and are in none of the queue's lists.
#[tokio::test]
async fn jobs_run_at_once_pass_the_lane_and_stay_out_of_the_lists() {
    let (harness, to) = start(true).await;
    // a.txt meets its old copy and asks; nobody answers, so it holds the lane of Memory(2).
    let holder = harness
        .queue
        .submit(copies(&["a.txt"]).remove(0))
        .expect("accepted")
        .id;
    let queue = harness.queue.clone();
    wait_until("the holder asks", || {
        queue.job(holder).is_ok_and(|job| job.question.is_some())
    })
    .await;
    let now: Vec<JobId> = harness
        .queue
        .submit_batch(
            per_item(JobSpec {
                kind: JobKind::Copy {
                    sources: vec![mem(1, "/src/b.txt"), mem(1, "/src/c.txt")],
                    target: mem(2, "/elsewhere"),
                },
                options: JobOptions::default(),
            }),
            AT_ONCE,
        )
        .expect("accepted")
        .iter()
        .map(|job| job.id)
        .collect();
    for id in &now {
        assert_eq!(harness.finish(*id).await.state, JobState::Completed);
    }
    assert_eq!(read(&to, "/elsewhere/c.txt").await, b"new");
    // Listed all the same (RC-010-25): finished, after the queued one, and marked.
    let entries = harness.queue.entries();
    let listed: Vec<(JobId, bool)> = entries
        .iter()
        .map(|entry| (entry.job.id, entry.at_once))
        .collect();
    assert_eq!(listed, [(holder, false), (now[0], true), (now[1], true)]);
    assert!(entries[1..].iter().all(|entry| entry.list == AT_ONCE));
    // One after the other: the second started once the first was over.
    let events = harness.events();
    let at = |job: JobId, change: &JobChange| {
        events
            .iter()
            .position(|event| event.job == job && event.change == *change)
            .expect("seen")
    };
    let first_done = events
        .iter()
        .position(|event| event.job == now[0] && matches!(event.change, JobChange::Finished { .. }))
        .expect("finished");
    assert!(first_done < at(now[1], &JobChange::Started));
    assert!(harness.queue.job(holder).expect("known").question.is_some());
}

/// A job of a batch run at once that waits for its turn and is cancelled ends at once, while the
/// sibling ahead of it still runs; a paused one starts only once resumed.
#[tokio::test]
async fn a_waiting_job_run_at_once_ends_when_cancelled_and_a_paused_one_waits_for_its_resume() {
    let (harness, _to) = start(true).await;
    let ids: Vec<JobId> = harness
        .queue
        .submit_batch(copies(&["a.txt", "b.txt", "c.txt"]), AT_ONCE)
        .expect("accepted")
        .iter()
        .map(|job| job.id)
        .collect();
    // a.txt meets its old copy and asks; nobody answers, so it keeps the batch's place.
    let queue = harness.queue.clone();
    wait_until("the first asks", || {
        queue.job(ids[0]).is_ok_and(|job| job.question.is_some())
    })
    .await;
    harness.queue.cancel(ids[2]).expect("cancel");
    let ended = tokio::time::timeout(std::time::Duration::from_secs(2), harness.finish(ids[2]));
    assert_eq!(
        ended.await.expect("ends at once").state,
        JobState::Cancelled
    );
    assert!(!harness.changes(ids[1]).contains(&JobChange::Started));

    harness.queue.pause(ids[1]).expect("pause");
    harness.queue.cancel(ids[0]).expect("cancel");
    harness.finish(ids[0]).await;
    assert!(!harness.changes(ids[1]).contains(&JobChange::Started));
    harness.queue.resume(ids[1]).expect("resume");
    wait_until("the resumed one asks", || {
        queue.job(ids[1]).is_ok_and(|job| job.question.is_some())
    })
    .await;
    harness.queue.cancel(ids[1]).expect("cancel");
    harness.finish(ids[1]).await;
}
