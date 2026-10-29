//! The background bar as the queue window's preview (RC-010-25), through `apply` on a real
//! engine: its jobs are the window's in the window's order, "N more" counts the rest, and with
//! `queue.show_at_once` off the window lists the queue alone while the bar keeps every job.

use std::{fs, time::Duration};

use rc_engine::{
    Command, JobKind, JobOptions, JobSnapshot, Outcome, PanelId, Progress, TransferRequest, VfsPath,
};
use tempfile::TempDir;

use super::*;
use crate::app::harness::Harness;

/// A waiting copy, job `id`, of the default list.
fn entry(id: JobId) -> QueueEntry {
    QueueEntry {
        job: JobSnapshot {
            id,
            kind: JobKind::Copy {
                sources: vec![VfsPath::local(format!("/src/{id}.txt"))],
                target: VfsPath::local("/dst"),
            },
            options: JobOptions::default(),
            state: JobState::Queued,
            progress: Progress::default(),
            question: None,
            skipped: 0,
            failures: Vec::new(),
            report: None,
            resumable: false,
            batch: None,
        },
        lane: None,
        place: Some(id as usize),
        list: rc_engine::DEFAULT_LIST,
        at_once: false,
    }
}

#[test]
fn the_opened_bar_lists_its_first_rows_and_counts_the_rest() {
    let entries: Vec<QueueEntry> = [1, 2, 3].map(entry).to_vec();
    let jobs: Vec<&QueueEntry> = entries.iter().collect();
    let (listed, more) = preview(&jobs, 2);
    let ids: Vec<JobId> = listed.iter().map(|entry| entry.job.id).collect();
    assert_eq!((ids, more), (vec![1, 2], 1));
    assert_eq!(preview(&jobs, 5).1, 0);
}

#[test]
fn the_more_line_names_the_queue_unless_the_window_leaves_out_the_jobs_run_at_once() {
    assert_eq!(more_line(0, false), None);
    assert_eq!(more_line(3, false).as_deref(), Some("3 more in the queue"));
    assert_eq!(
        more_line(3, true).as_deref(),
        Some("3 more in the background")
    );
}

/// `root/` with `a.txt` to `d.txt` and `out/` holding older copies of them; the left panel
/// on `root`, the right one on `out`, so every copy asks.
async fn setup() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let out = root.path().join("out");
    fs::create_dir(&out).expect("mkdir");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        fs::write(root.path().join(name), b"new").expect("write");
        fs::write(out.join(name), b"old").expect("write");
    }
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

async fn run(harness: &mut Harness, command: Command) -> JobId {
    let outcome = harness
        .app
        .engine()
        .dispatch(command)
        .await
        .expect("dispatched");
    harness.settle().await;
    match outcome {
        Outcome::Job(job) => job.id,
        Outcome::Jobs { jobs } => jobs.first().map_or(0, |job| job.id),
        _ => 0,
    }
}

/// Waits until job `id` asks something, then lets the app see it.
async fn asked(harness: &mut Harness, id: JobId) {
    for _ in 0..500 {
        let job = harness.app.engine().ops().job(id).expect("known");
        if job.question.is_some() {
            harness.settle().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} asked nothing");
}

/// A queued copy that asks and holds the local lane, two queued jobs behind it, and a copy run
/// at once (an agent's `ops_copy_to` without a queue) that asks too; none in the progress window.
async fn four_jobs(harness: &mut Harness) -> [JobId; 4] {
    let copy = |name: &str| {
        Command::OpsCopy(TransferRequest {
            panel: Some(PanelId::Left),
            sources: vec![name.into()],
            target: None,
            options: JobOptions::default(),
        })
    };
    let holder = run(harness, copy("a.txt")).await;
    asked(harness, holder).await;
    let waiter = run(harness, copy("b.txt")).await;
    // A copy, not a new folder: that is made at once, outside the lanes.
    let third = run(harness, copy("d.txt")).await;
    let at_once = Command::OpsCopyTo {
        panel: Some(PanelId::Left),
        sources: vec!["c.txt".into()],
        target: "out".into(),
        options: JobOptions::default(),
        queue: None,
    };
    let now = run(harness, at_once).await;
    asked(harness, now).await;
    [now, holder, waiter, third]
}

fn settings(rows: usize, at_once: bool) -> Message {
    Message::FileOps(FileOpsMsg::Settings(Stored {
        rows,
        at_once,
        ..Stored::default()
    }))
}

fn window_order(harness: &Harness) -> Vec<JobId> {
    let window = harness.app.queue_window.as_ref().expect("the window");
    window.entries.iter().map(|entry| entry.job.id).collect()
}

fn bar_order(harness: &Harness) -> Vec<JobId> {
    harness
        .app
        .background()
        .iter()
        .map(|entry| entry.job.id)
        .collect()
}

/// The owner's report: the bar listed five and "3 more in the queue", the queue window nothing.
/// Now the bar's rows are the window's first ones, and the rest is what the window lists beyond.
#[tokio::test]
async fn the_bar_previews_the_window_s_first_rows_and_counts_what_it_lists_beyond() {
    let (_root, mut harness) = setup().await;
    let jobs = four_jobs(&mut harness).await;
    harness.send(settings(2, true)).await;
    run(&mut harness, Command::QueueWindow {}).await;

    assert_eq!(window_order(&harness), jobs, "the job run at once first");
    assert_eq!(bar_order(&harness), window_order(&harness));
    let background = harness.app.background();
    let (listed, more) = preview(&background, 2);
    let listed: Vec<JobId> = listed.iter().map(|entry| entry.job.id).collect();
    assert_eq!(listed, jobs[..2]);
    assert_eq!(more, 2, "what the window lists beyond the bar's rows");

    // Pause all in the bar reaches the job run at once too, the last job first.
    let effects = harness.app.steer_background(All::Pause);
    let [Effect::Dispatch(_, commands)] = &effects[..] else {
        panic!("one dispatch");
    };
    let paused: Vec<JobId> = commands
        .iter()
        .filter_map(|command| match command {
            Command::OpsPause { job } => Some(*job),
            _ => None,
        })
        .collect();
    assert_eq!(paused, [jobs[3], jobs[2], jobs[1], jobs[0]]);
}

/// `queue.show_at_once` off: the window lists the queue alone, the bar keeps the job run at once,
/// and its "more" line names the background; on again, the window lists it again.
#[tokio::test]
async fn with_the_setting_off_the_window_lists_the_queue_alone_and_the_bar_every_job() {
    let (_root, mut harness) = setup().await;
    let [now, holder, waiter, third] = four_jobs(&mut harness).await;
    run(&mut harness, Command::QueueWindow {}).await;

    harness.send(settings(2, false)).await;
    assert!(harness.app.file_ops.hide_at_once);
    assert_eq!(window_order(&harness), [holder, waiter, third]);
    assert_eq!(bar_order(&harness), [now, holder, waiter, third]);

    harness.send(settings(2, true)).await;
    assert!(!harness.app.file_ops.hide_at_once);
    assert_eq!(window_order(&harness), [now, holder, waiter, third]);
}
