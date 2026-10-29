//! The queue window through `apply`, without a window (RC-010-14): F2 in the copy dialog queues
//! the job behind the one that holds the lane, the window shows it waiting, moves and removes
//! waiting jobs, cancels a running one after a yes, and brings a job's question up.

use std::{fs, time::Duration};

use rc_engine::{
    Answer, Command, ConflictChoice, JobId, JobOptions, JobState, Outcome, PanelId, TransferRequest,
};
use tempfile::TempDir;

use super::*;
use crate::app::{file_ops::FileOpsMsg, harness::Harness};

/// `root/` with `a.txt`, `b.txt` and `out/` holding older `a.txt` and `b.txt`; the left panel on
/// `root`, the right one on `out`.
async fn setup() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let out = root.path().join("out");
    fs::create_dir(&out).expect("mkdir");
    fs::write(root.path().join("a.txt"), b"new").expect("write");
    fs::write(root.path().join("b.txt"), b"b").expect("write");
    fs::write(out.join("a.txt"), b"old").expect("write");
    fs::write(out.join("b.txt"), b"old").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    run(
        &mut harness,
        Command::Navigate {
            panel: Some(PanelId::Right),
            path: "out".into(),
        },
    )
    .await;
    (root, harness)
}

async fn run(harness: &mut Harness, command: Command) -> Outcome {
    let outcome = harness
        .app
        .engine()
        .dispatch(command)
        .await
        .expect("dispatched");
    harness.settle().await;
    outcome
}

/// A copy of `name` into the right panel that the window does not follow, as an agent's would
/// be: with no preset it asks about an existing target, and holds the local lane meanwhile.
async fn copy_aside(harness: &mut Harness, name: &str) -> JobId {
    let outcome = run(
        harness,
        Command::OpsCopy(TransferRequest {
            panel: Some(PanelId::Left),
            sources: vec![name.into()],
            target: None,
            options: JobOptions::default(),
        }),
    )
    .await;
    match outcome {
        Outcome::Job(job) => job.id,
        other => panic!("a job, got {other:?}"),
    }
}

/// Waits until job `id` asks something, then lets the app see it.
async fn asked(harness: &mut Harness, id: JobId) {
    for _ in 0..500 {
        if harness
            .app
            .engine()
            .ops()
            .job(id)
            .expect("known")
            .question
            .is_some()
        {
            harness.settle().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} asked nothing");
}

async fn finish(harness: &mut Harness, id: JobId) {
    tokio::time::timeout(Duration::from_secs(10), harness.app.engine().ops().wait(id))
        .await
        .expect("in time")
        .expect("known");
    harness.settle().await;
}

async fn skip(harness: &mut Harness, job: JobId) {
    let answer = Answer::Conflict {
        choice: ConflictChoice::Skip,
        for_all: false,
    };
    run(harness, Command::OpsAnswer { job, answer }).await;
}

fn window(harness: &Harness) -> &QueueWindow {
    harness.app.queue_window.as_ref().expect("the queue window")
}

fn order(harness: &Harness) -> Vec<JobId> {
    window(harness)
        .entries
        .iter()
        .map(|entry| entry.job.id)
        .collect()
}

fn newest(harness: &Harness) -> JobId {
    harness
        .app
        .engine()
        .ops()
        .jobs()
        .last()
        .map(|job| job.id)
        .expect("a job")
}

/// The acceptance check: F2 in the F5 dialog queues the job, the window shows it waiting; its
/// question, once it runs, is answered from the window.
#[tokio::test]
async fn f2_queues_the_copy_and_its_question_is_answered_from_the_window() {
    let (root, mut harness) = setup().await;
    let holder = copy_aside(&mut harness, "a.txt").await;
    asked(&mut harness, holder).await;
    assert!(!harness.app.file_ops.modal(), "not the window's job");

    run(
        &mut harness,
        Command::SetCursor {
            panel: Some(PanelId::Left),
            target: rc_engine::CursorTarget::Name("b.txt".into()),
        },
    )
    .await;
    harness.press("F5").await;
    harness.press("F2").await;
    assert!(
        harness.app.file_ops.dialog.is_none(),
        "F2 closes the dialog"
    );
    let queued = newest(&harness);
    assert_ne!(queued, holder);
    let tracked = harness.app.file_ops.jobs.get(queued).expect("followed");
    assert!(
        !tracked.foreground,
        "into the background, no progress window"
    );

    harness.press("Ctrl+Shift+Q").await;
    assert_eq!(order(&harness), [holder, queued]);
    let entry = window(&harness).entry(queued).expect("listed");
    assert_eq!((entry.job.state, entry.place), (JobState::Queued, Some(0)));

    skip(&mut harness, holder).await;
    finish(&mut harness, holder).await;
    asked(&mut harness, queued).await;
    let entry = window(&harness).entry(queued).expect("listed");
    assert_eq!(
        entry.job.state,
        JobState::Waiting,
        "the window shows the question"
    );
    harness.send(Message::Queue(QueueMsg::Answer(queued))).await;
    let asking = harness.app.file_ops.jobs.asking().expect("the question");
    assert_eq!(asking.id(), queued);
    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job: queued,
            choice: ConflictChoice::Overwrite,
            for_all: false,
        }))
        .await;
    finish(&mut harness, queued).await;
    assert_eq!(
        fs::read(root.path().join("out").join("b.txt")).expect("read"),
        b"b"
    );
    let entry = window(&harness).entry(queued).expect("still listed");
    assert_eq!(entry.job.state, JobState::Completed);
    harness.press("Escape").await;
    assert!(harness.app.queue_window.is_none());
}

#[tokio::test]
async fn the_window_s_keys_move_and_remove_waiting_jobs_and_cancel_after_a_yes() {
    let (root, mut harness) = setup().await;
    let holder = copy_aside(&mut harness, "a.txt").await;
    asked(&mut harness, holder).await;
    let second = copy_aside(&mut harness, "b.txt").await;
    // A copy of a new file, not a new folder: that is made at once, outside the lanes.
    fs::write(root.path().join("new"), b"new").expect("write");
    let third = copy_aside(&mut harness, "new").await;
    run(&mut harness, Command::QueueWindow {}).await;
    assert_eq!(order(&harness), [holder, second, third]);
    assert_eq!(window(&harness).selected, Some(holder));

    harness.press("ArrowDown").await;
    harness.press("ArrowDown").await;
    harness.press("Ctrl+ArrowUp").await;
    assert_eq!(order(&harness), [holder, third, second], "moved up");
    assert_eq!(
        window(&harness).selected,
        Some(third),
        "the selection follows"
    );
    harness.press("F5").await;
    assert!(
        harness.app.file_ops.dialog.is_none(),
        "the window has the keys"
    );

    harness.press("Delete").await;
    assert_eq!(order(&harness), [holder, second], "removed");
    assert!(!root.path().join("out").join("new").exists());

    harness.send(Message::Queue(QueueMsg::Select(holder))).await;
    harness.press("Delete").await;
    let asks = Confirm::Remove {
        jobs: vec![holder],
        started: 1,
    };
    assert_eq!(window(&harness).confirm, Some(asks), "asks first");
    harness.press("Escape").await;
    assert_eq!(window(&harness).confirm, None);
    assert!(harness.app.queue_window.is_some(), "Escape only said no");
    harness.press("Delete").await;
    harness.press("Enter").await;
    // Cancelled and taken off the queue (RC-010-15): the next job gets the lane.
    assert_eq!(order(&harness), [second]);
    assert!(harness.app.engine().ops().job(holder).is_err());
    assert_eq!(
        fs::read(root.path().join("out").join("a.txt")).expect("read"),
        b"old",
        "nothing written"
    );
    asked(&mut harness, second).await;
    skip(&mut harness, second).await;
    finish(&mut harness, second).await;
    harness.send(Message::Queue(QueueMsg::ClearFinished)).await;
    assert!(window(&harness).entries.is_empty(), "{:?}", order(&harness));
}

#[tokio::test]
async fn a_click_on_the_background_bar_opens_the_window() {
    let (_root, mut harness) = setup().await;
    let open = Command::QueueWindow {};
    harness
        .send(Message::FileOps(FileOpsMsg::Key(Box::new(open))))
        .await;
    assert!(window(&harness).entries.is_empty());
}

/// RC-010-14, the owner's Windows test: F5 of two selected files is two jobs, in the selection's
/// order, run one after the other; confirmed with OK they are in no queue list (only F2 queues,
/// as in Total Commander) yet listed in the window as run at once; the progress window follows the
/// operation, and one answer "for all" serves both.
#[tokio::test]
async fn f5_of_two_files_is_two_jobs_and_one_answer_for_all_serves_both() {
    let (root, mut harness) = setup().await;
    run(
        &mut harness,
        Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["a.txt".into(), "b.txt".into()],
        },
    )
    .await;
    harness.press("F5").await;
    harness.press("Enter").await;
    let jobs: Vec<JobId> = harness
        .app
        .engine()
        .ops()
        .jobs()
        .iter()
        .map(|job| job.id)
        .collect();
    let [first, second] = jobs[..] else {
        panic!("one job per file, got {jobs:?}");
    };
    let tracked = |id| harness.app.file_ops.jobs.get(id).expect("followed").clone();
    assert!(
        tracked(first).foreground,
        "the first in the progress window"
    );
    assert!(!tracked(second).foreground);
    assert!(tracked(first).snapshot.batch.is_some());
    assert_eq!(
        tracked(first).snapshot.batch,
        tracked(second).snapshot.batch
    );

    asked(&mut harness, first).await;
    run(&mut harness, Command::QueueWindow {}).await;
    // OK queues nothing, but the window lists both as run at once (RC-010-25).
    assert_eq!(order(&harness), [first, second]);
    assert!(window(&harness).entries.iter().all(|entry| entry.at_once));
    let waiting = harness.app.engine().ops().job(second).expect("known");
    assert_eq!(waiting.state, JobState::Queued, "after the first");

    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job: first,
            choice: ConflictChoice::Overwrite,
            for_all: true,
        }))
        .await;
    // The second job asks nothing: it would wait for an answer past `finish`'s time.
    finish(&mut harness, first).await;
    finish(&mut harness, second).await;
    let out = root.path().join("out");
    assert_eq!(fs::read(out.join("a.txt")).expect("read"), b"new");
    assert_eq!(fs::read(out.join("b.txt")).expect("read"), b"b");
    assert!(harness.app.file_ops.jobs.foreground().is_none());
}

/// RC-010-14: one button pauses or resumes the whole queue - "Pause all" while a job waits
/// unpaused, "Resume all" once all are paused - and none applies to an empty queue.
#[tokio::test]
async fn the_queue_s_one_pause_button_follows_the_jobs() {
    let (_root, mut harness) = setup().await;
    run(&mut harness, Command::QueueWindow {}).await;
    assert_eq!(window(&harness).all, AllToggle::Pause, "nothing to resume");

    // A stopped list holds its job, so nothing finishes meanwhile.
    let held = harness
        .app
        .engine()
        .ops()
        .create_list("held")
        .expect("made")
        .id;
    run(
        &mut harness,
        Command::OpsCopyTo {
            panel: Some(PanelId::Left),
            sources: vec!["a.txt".into()],
            target: "out".into(),
            options: JobOptions::default(),
            queue: Some(held),
        },
    )
    .await;
    assert_eq!(window(&harness).all, AllToggle::Pause);
    harness.send(Message::Queue(QueueMsg::PauseAll)).await;
    harness.settle().await;
    assert_eq!(window(&harness).all, AllToggle::Resume);
    harness.send(Message::Queue(QueueMsg::ResumeAll)).await;
    harness.settle().await;
    assert_eq!(window(&harness).all, AllToggle::Pause);
}

/// RC-080-45: a progress event patches the job's row in the window and in the bar from the
/// queue - its state and progress - and reads nothing else again.
#[tokio::test]
async fn progress_patches_the_job_s_row_alone() {
    let (_root, mut harness) = setup().await;
    run(&mut harness, Command::QueueWindow {}).await;
    let held = harness
        .app
        .engine()
        .ops()
        .create_list("held")
        .expect("made")
        .id;
    for name in ["a.txt", "b.txt"] {
        let copy = Command::OpsCopyTo {
            panel: Some(PanelId::Left),
            sources: vec![name.into()],
            target: "out".into(),
            options: JobOptions::default(),
            queue: Some(held),
        };
        run(&mut harness, copy).await;
    }
    // The window on the stopped list, which holds both jobs.
    harness.app.queue_window.as_mut().expect("open").list = held;
    harness.app.refresh_queue_window();
    let (first, second) = (order(&harness)[0], order(&harness)[1]);
    let stale = |harness: &mut Harness, files_total| {
        let app = &mut harness.app;
        let window = app.queue_window.as_mut().expect("the queue window");
        let rows = window.entries.iter_mut().chain(app.file_ops.bar.iter_mut());
        for entry in rows {
            entry.job.progress.files_total = files_total;
            entry.job.state = JobState::Running;
        }
    };
    stale(&mut harness, 999);
    let progress = rc_engine::JobChange::Progress {
        progress: rc_engine::Progress::default(),
    };
    let _ = harness.app.on_job(first, progress);
    let (state, queued) = harness
        .app
        .engine()
        .ops()
        .job_progress(first)
        .expect("known");
    let row = |job: JobId| window(&harness).entry(job).expect("listed").job.clone();
    assert_eq!(
        (row(first).state, row(first).progress),
        (state, queued.clone())
    );
    let in_bar = harness.app.file_ops.bar.iter().find(|e| e.job.id == first);
    assert_eq!(in_bar.map(|entry| entry.job.progress.clone()), Some(queued));
    assert_eq!(
        row(second).progress.files_total,
        999,
        "the other row is not read"
    );
    assert_eq!(order(&harness), [first, second]);
}
