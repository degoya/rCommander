//! RC-110-27: what the window keeps about jobs is read again from the queue after missed events.

use std::{fs, path::Path, time::Duration};

use rc_engine::{
    Command, EngineEvent, JobChange, JobOptions, JobSnapshot, JobState, Outcome, PanelId,
    TransferRequest,
};
use tempfile::TempDir;

use super::super::FollowUp;
use crate::app::{Message, harness::Harness};

/// `root` with `abc.txt` and the folder `out`, the panels on the two.
async fn harness(root: &Path) -> Harness {
    fs::write(root.join("abc.txt"), b"abc").expect("write");
    fs::create_dir(root.join("out")).expect("mkdir");
    let mut harness = Harness::open_with_db(root, &root.join("out")).await;
    harness.settle().await;
    harness
}

/// A copy of `abc.txt` into `out`, started on the engine and followed by the window in its
/// progress window as it was when the queue took it.
async fn followed_copy(harness: &mut Harness, root: &Path) -> JobSnapshot {
    let copy = Command::OpsCopy(TransferRequest {
        panel: Some(PanelId::Left),
        sources: vec!["abc.txt".into()],
        target: Some(root.join("out").to_string_lossy().into_owned()),
        options: JobOptions::default(),
        rename_mask: None,
    });
    let engine = harness.app.engine().clone();
    let Ok(Outcome::Job(job)) = engine.dispatch(copy).await else {
        panic!("a job");
    };
    let mut heard = (*job).clone();
    heard.state = JobState::Running;
    heard.question = None;
    let jobs = &mut harness.app.file_ops.jobs;
    assert!(
        jobs.adopt(heard, PanelId::Left, FollowUp::Refresh, true)
            .is_none(),
        "the window follows it"
    );
    *job
}

/// Waits until the queue's own snapshot of `job` meets `met`.
async fn until(harness: &Harness, job: &JobSnapshot, met: impl Fn(&JobSnapshot) -> bool) {
    for _ in 0..500 {
        if met(&harness.app.engine().ops().job(job.id).expect("known")) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {} did not get there", job.id);
}

/// A `Finished` the window never heard: after `EngineLagged` the job is over in the window
/// too, and the event that may still arrive behind the gap ends nothing twice.
#[tokio::test]
async fn a_lost_finished_is_caught_up_after_missed_events() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    let job = followed_copy(&mut harness, root.path()).await;
    until(&harness, &job, |job| job.state.is_finished()).await;
    harness.miss_events();
    assert!(
        harness.app.file_ops.jobs.get(job.id).is_some(),
        "unheard, the job runs on in the window"
    );
    assert!(harness.app.file_ops.modal(), "its progress window stays");

    harness.send(Message::EngineLagged).await;
    assert!(harness.app.file_ops.jobs.get(job.id).is_none(), "over");
    assert!(!harness.app.file_ops.modal(), "the progress window closed");
    assert!(root.path().join("out").join("abc.txt").exists());

    let late = EngineEvent::Job {
        job: job.id,
        change: JobChange::Finished {
            state: JobState::Completed,
        },
    };
    let effects = harness.app.apply(Message::Engine(late));
    assert!(effects.is_empty(), "the late event reports nothing again");
    assert!(harness.app.file_ops.ended_unheard.is_empty());
}

/// A conflict asked while the window missed events is asked after `EngineLagged`.
#[tokio::test]
async fn a_lost_conflict_question_is_asked_after_missed_events() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    fs::write(root.path().join("out").join("abc.txt"), b"old").expect("write");
    let job = followed_copy(&mut harness, root.path()).await;
    until(&harness, &job, |job| job.question.is_some()).await;
    harness.miss_events();
    assert!(harness.app.file_ops.jobs.asking().is_none(), "unheard");

    harness.send(Message::EngineLagged).await;
    let asking = harness.app.file_ops.jobs.asking().expect("the question");
    assert_eq!(asking.id(), job.id);
    assert_eq!(asking.snapshot.state, JobState::Waiting);
    let _ = harness.app.engine().ops().cancel(job.id);
}

/// Without a job the window follows, missed events change nothing about jobs.
#[tokio::test]
async fn a_resync_without_followed_jobs_changes_nothing() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.send(Message::EngineLagged).await;
    assert!(harness.app.file_ops.jobs.is_empty());
    assert!(harness.app.file_ops.ended_unheard.is_empty());
    assert!(!harness.app.dialog_open());
}
