//! Several jobs in the queue window through `apply`, without a window (RC-010-15, owner
//! 2026-10-18): Ctrl+click, Shift+arrows, Space, Insert and Ctrl+A select, the Reorder keys move
//! the selection keeping its order, Delete takes it off - asking once when a job of it has
//! started -, the context menu puts it into a new list and clears every list.

use std::{fs, time::Duration};

use iced::keyboard::Modifiers;
use rc_engine::{Command, JobId, JobOptions, Outcome, PanelId, TransferRequest};
use tempfile::TempDir;

use super::Confirm;
use crate::app::{
    Message,
    file_ops::FileOpsMsg,
    harness::Harness,
    queue_window::{ListMsg, MenuMsg, QueueMsg, QueueWindow},
};

/// `root/` with `a.txt` and `out/` holding an older `a.txt`; the left panel on `root`, the right
/// one on `out`. A copy of `a.txt` asks about the existing target and holds the local lane, three
/// copies of new files wait behind it; the queue window is open.
async fn setup() -> (TempDir, Harness, [JobId; 4]) {
    let root = tempfile::tempdir().expect("temp dir");
    let out = root.path().join("out");
    fs::create_dir(&out).expect("mkdir");
    fs::write(root.path().join("a.txt"), b"new").expect("write");
    fs::write(out.join("a.txt"), b"old").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    run(
        &mut harness,
        Command::Navigate {
            panel: Some(PanelId::Right),
            path: "out".into(),
        },
    )
    .await;
    let holder = job(run(
        &mut harness,
        Command::OpsCopy(TransferRequest {
            panel: Some(PanelId::Left),
            sources: vec!["a.txt".into()],
            target: None,
            options: JobOptions::default(),
        }),
    )
    .await);
    for _ in 0..500 {
        let ops = harness.app.engine().ops();
        if ops.job(holder).expect("known").question.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut jobs = [holder; 4];
    for (at, name) in ["n1", "n2", "n3"].into_iter().enumerate() {
        jobs[at + 1] = waiting(&mut harness, root.path(), name).await;
    }
    run(&mut harness, Command::QueueWindow {}).await;
    assert_eq!(order(&harness), jobs);
    (root, harness, jobs)
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

fn job(outcome: Outcome) -> JobId {
    match outcome {
        Outcome::Job(job) => job.id,
        other => panic!("a job, got {other:?}"),
    }
}

/// A copy of a new file `name` into `out`, waiting in the local lane (a new folder is made at
/// once, outside the lanes).
async fn waiting(harness: &mut Harness, root: &std::path::Path, name: &str) -> JobId {
    fs::write(root.join(name), name).expect("write");
    job(run(
        harness,
        Command::OpsCopy(TransferRequest {
            panel: Some(PanelId::Left),
            sources: vec![name.into()],
            target: None,
            options: JobOptions::default(),
        }),
    )
    .await)
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

/// A click on the row of `job`, with Ctrl held when `ctrl`.
async fn click(harness: &mut Harness, job: JobId, ctrl: bool) {
    let held = if ctrl {
        Modifiers::CTRL
    } else {
        Modifiers::empty()
    };
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(held)))
        .await;
    harness.send(Message::Queue(QueueMsg::Select(job))).await;
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(Modifiers::empty())))
        .await;
}

#[tokio::test]
async fn ctrl_click_and_delete_take_waiting_jobs_off_without_asking() {
    let (root, mut harness, [holder, one, two, three]) = setup().await;
    click(&mut harness, one, false).await;
    click(&mut harness, three, true).await;
    assert_eq!(window(&harness).chosen(), [one, three]);
    harness.press("Delete").await;
    assert_eq!(window(&harness).confirm, None, "none of them has started");
    assert_eq!(order(&harness), [holder, two]);
    assert!(window(&harness).marked.is_empty());
    assert!(!root.path().join("out").join("n1").exists());
    assert!(!root.path().join("out").join("n3").exists());
}

#[tokio::test]
async fn the_keys_mark_and_move_the_selection_keeping_its_order() {
    let (_root, mut harness, [holder, one, two, three]) = setup().await;
    harness.press("ArrowDown").await;
    harness.press("Shift+ArrowDown").await;
    assert_eq!(window(&harness).chosen(), [one, two]);
    harness.press("Ctrl+End").await;
    assert_eq!(order(&harness), [holder, three, one, two], "to the end");
    harness.press("Ctrl+ArrowUp").await;
    assert_eq!(order(&harness), [holder, one, two, three], "up as a block");

    // The cursor is on `two`: Space unmarks it, Insert marks `three`.
    harness.press("Space").await;
    harness.press("Insert").await;
    assert_eq!(window(&harness).chosen(), [one, three]);
    harness.press("Ctrl+ArrowDown").await;
    assert_eq!(order(&harness), [holder, two, one, three], "the last holds");
    harness.press("Ctrl+A").await;
    assert_eq!(window(&harness).chosen(), [holder, two, one, three]);
}

/// The owner: a running or partly done job among the selection is cancelled, after one question.
#[tokio::test]
async fn a_started_job_in_the_selection_asks_once_and_is_cancelled() {
    let (root, mut harness, [holder, one, two, three]) = setup().await;
    harness.press("Ctrl+A").await;
    harness.press("Delete").await;
    assert_eq!(
        window(&harness).confirm,
        Some(Confirm::Remove {
            jobs: vec![holder, one, two, three],
            started: 1,
        })
    );
    harness.press("Escape").await;
    assert_eq!(window(&harness).confirm, None, "no: everything stays");
    assert!(harness.app.queue_window.is_some(), "Escape only said no");
    assert_eq!(order(&harness).len(), 4);

    harness.press("Delete").await;
    harness.press("Enter").await;
    assert!(order(&harness).is_empty(), "{:?}", order(&harness));
    // The asking copy ends and lets the lane go: the next copy runs.
    let four = waiting(&mut harness, root.path(), "n4").await;
    tokio::time::timeout(
        Duration::from_secs(10),
        harness.app.engine().ops().wait(four),
    )
    .await
    .expect("in time")
    .expect("known");
    assert!(root.path().join("out").join("n4").is_file());
    assert!(!root.path().join("out").join("n1").exists());
    assert_eq!(
        fs::read(root.path().join("out").join("a.txt")).expect("read"),
        b"old",
        "nothing written"
    );
}

#[tokio::test]
async fn the_menu_moves_the_selection_into_a_new_list_and_clears_every_list() {
    let (_root, mut harness, [holder, one, two, three]) = setup().await;
    click(&mut harness, one, false).await;
    click(&mut harness, two, true).await;
    harness.send(Message::Queue(QueueMsg::RowMenu(two))).await;
    let menu = window(&harness).menu.as_ref().expect("the menu");
    let gather = menu
        .items
        .iter()
        .position(|item| item.label == "Move to a new list...")
        .expect("offered");
    assert_eq!(window(&harness).chosen(), [one, two], "the selection stays");
    harness
        .send(Message::Queue(QueueMsg::Menu(MenuMsg::Pick(gather))))
        .await;
    assert!(window(&harness).menu.is_none());
    let lists = |message| Message::Queue(QueueMsg::Lists(message));
    harness
        .send(lists(ListMsg::NameInput("Later".into())))
        .await;
    harness.send(lists(ListMsg::NameSubmit)).await;
    assert_eq!(order(&harness), [holder, three], "they left the list shown");
    let later = window(&harness)
        .lists
        .iter()
        .find(|list| list.name == "Later")
        .cloned()
        .expect("the new list");
    assert_eq!((later.jobs, later.started), (2, false));

    // A right click on a job outside the selection selects it alone.
    harness.send(Message::Queue(QueueMsg::RowMenu(three))).await;
    assert_eq!(window(&harness).chosen(), [three]);
    harness.press("Escape").await;
    assert!(window(&harness).menu.is_none(), "Escape closes the menu");
    assert!(harness.app.queue_window.is_some(), "and only the menu");

    harness.press("Shift+F10").await;
    harness.press("End").await;
    harness.press("Enter").await;
    assert_eq!(
        window(&harness).confirm,
        Some(Confirm::Clear {
            all_lists: true,
            unfinished: 4,
        })
    );
    harness.press("Enter").await;
    assert!(order(&harness).is_empty());
    let lists = &window(&harness).lists;
    assert!(lists.iter().all(|list| list.jobs == 0), "{lists:?}");
}
