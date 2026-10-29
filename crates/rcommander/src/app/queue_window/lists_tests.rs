//! The queue window's lists through `apply`, without a window (RC-010-15): F2 puts a copy into the
//! chosen list and remembers it, the window shows one list at a time, a list is made, started,
//! saved, loaded and its saved form deleted after a yes.

use std::{fs, time::Duration};

use rc_engine::{Command, CursorTarget, JobId, JobState, Outcome, PanelId, VfsPath};
use tempfile::TempDir;

use super::{ListMsg, NameAction, Note};
use crate::app::{
    Message, file_ops::FileOpsMsg, harness::Harness, queue_window::QueueMsg,
    queue_window::QueueWindow,
};

/// `root/` with `c.txt` and an empty `out/`; the left panel on `root`, the right one on `out`; a
/// database attached.
async fn setup() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("out")).expect("mkdir");
    fs::write(root.path().join("c.txt"), b"c").expect("write");
    let mut harness = Harness::with_db(root.path()).await;
    for command in [
        Command::Navigate {
            panel: Some(PanelId::Right),
            path: "out".into(),
        },
        Command::SetCursor {
            panel: Some(PanelId::Left),
            target: CursorTarget::Name("c.txt".into()),
        },
    ] {
        harness.app.engine().dispatch(command).await.expect("done");
    }
    harness.settle().await;
    (root, harness)
}

fn lists(message: ListMsg) -> Message {
    Message::Queue(QueueMsg::Lists(message))
}

fn window(harness: &Harness) -> &QueueWindow {
    harness.app.queue_window.as_ref().expect("the queue window")
}

/// A new list named `name` through the window's name field; its id.
async fn new_list(harness: &mut Harness, name: &str) -> u64 {
    harness.send(lists(ListMsg::Name(NameAction::Create))).await;
    harness.send(lists(ListMsg::NameInput(name.into()))).await;
    harness.send(lists(ListMsg::NameSubmit)).await;
    window(harness)
        .lists
        .iter()
        .find(|list| list.name == name)
        .map(|list| list.id)
        .expect("the new list")
}

fn newest(harness: &Harness) -> JobId {
    let jobs = harness.app.engine().ops().jobs();
    jobs.last().map(|job| job.id).expect("a job")
}

/// The acceptance check: F2 puts the copy into the chosen list, which holds it until it starts,
/// and the next copy dialog offers that list again.
#[tokio::test]
async fn f2_puts_the_copy_into_the_chosen_list_and_remembers_it() {
    let (root, mut harness) = setup().await;
    harness.press("Ctrl+Shift+Q").await;
    let disk = new_list(&mut harness, "Disk").await;
    harness.press("Escape").await;

    harness.press("F5").await;
    let dialog = harness.app.file_ops.dialog.as_ref().expect("the dialog");
    assert_eq!(dialog.lists.len(), 2, "both lists offered");
    harness
        .send(Message::FileOps(FileOpsMsg::QueueList(disk)))
        .await;
    harness.press("F2").await;
    let job = newest(&harness);
    assert_eq!(harness.app.file_ops.queue_target, disk);
    tokio::time::sleep(Duration::from_millis(20)).await;
    let queued = harness.app.engine().ops().job(job).expect("known");
    assert_eq!(queued.state, JobState::Queued, "the stopped list holds it");

    harness.press("Ctrl+Shift+Q").await;
    assert!(
        window(&harness).entries.is_empty(),
        "the default list shows"
    );
    harness.send(lists(ListMsg::Show(disk))).await;
    assert_eq!(window(&harness).entries[0].job.id, job);
    harness.send(lists(ListMsg::Start(true))).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        harness.app.engine().ops().wait(job),
    )
    .await
    .expect("in time")
    .expect("known");
    assert!(root.path().join("out").join("c.txt").exists());
    harness.press("Escape").await;

    harness.press("F5").await;
    let dialog = harness.app.file_ops.dialog.as_ref().expect("the dialog");
    assert_eq!(dialog.queue, disk, "the last list chosen");
}

#[tokio::test]
async fn a_list_is_saved_loaded_and_its_saved_form_deleted_after_a_yes() {
    let (root, mut harness) = setup().await;
    harness.press("Ctrl+Shift+Q").await;
    let backup = new_list(&mut harness, "Backup").await;
    harness.send(lists(ListMsg::Show(backup))).await;
    harness.send(lists(ListMsg::Name(NameAction::Save))).await;
    harness.send(lists(ListMsg::NameSubmit)).await;
    assert_eq!(
        window(&harness).note,
        Some(Note::Refused("queue_list_empty")),
        "nothing to save yet"
    );

    let spec = rc_ops::JobSpec {
        kind: rc_engine::JobKind::Copy {
            sources: vec![VfsPath::local(root.path().join("c.txt"))],
            target: VfsPath::local(root.path().join("out")),
        },
        options: rc_engine::JobOptions::default(),
    };
    let ops = harness.app.engine().ops();
    ops.submit_into(spec, None, backup).expect("held");
    harness.send(lists(ListMsg::Name(NameAction::Save))).await;
    let naming = window(&harness).naming.as_ref().expect("the name field");
    assert_eq!(naming.text, "Backup", "the list's own name");
    harness.send(lists(ListMsg::NameSubmit)).await;
    let saved = window(&harness).saved.clone();
    assert_eq!(saved.len(), 1, "{:?}", window(&harness).note);
    assert_eq!(window(&harness).saved_choice, Some(saved[0].id));

    harness.send(lists(ListMsg::Load)).await;
    let shown = window(&harness).list;
    assert_ne!(shown, backup, "the loaded list shows");
    assert_eq!(
        window(&harness).note,
        Some(Note::Loaded {
            jobs: 1,
            unloadable: 0
        })
    );
    assert_eq!(window(&harness).entries.len(), 1);

    harness.send(lists(ListMsg::DeleteSaved)).await;
    assert_eq!(window(&harness).confirm_delete_saved, Some(saved[0].id));
    harness.press("Escape").await;
    assert!(window(&harness).confirm_delete_saved.is_none(), "kept");
    harness.send(lists(ListMsg::DeleteSaved)).await;
    harness.press("Enter").await;
    assert!(window(&harness).saved.is_empty());
    let Ok(Outcome::QueueLists { saved, .. }) =
        harness.app.engine().dispatch(Command::QueueLists {}).await
    else {
        panic!("the lists");
    };
    assert!(saved.is_empty());
    for job in harness.app.engine().ops().jobs() {
        let _ = harness.app.engine().ops().cancel(job.id);
    }
}

/// Puts the left panel's cursor on `name`.
async fn cursor_on(harness: &mut Harness, name: &str) {
    let command = Command::SetCursor {
        panel: Some(PanelId::Left),
        target: CursorTarget::Name(name.into()),
    };
    harness.app.engine().dispatch(command).await.expect("done");
    harness.settle().await;
}

/// Waits for job `id`; its last state.
async fn over(harness: &mut Harness, id: JobId) -> JobState {
    let done = tokio::time::timeout(Duration::from_secs(10), harness.app.engine().ops().wait(id))
        .await
        .expect("in time")
        .expect("known");
    harness.settle().await;
    done.state
}

/// The owner's report of 2026-10-18 (a delete went into the queue without F2), as Total Commander
/// does it for copy, move and delete alike: OK (Enter) runs the job at once in the progress
/// window, in none of the queue's lists; only F2 puts it into the chosen list, which holds it.
#[tokio::test]
async fn ok_runs_copy_move_and_delete_at_once_and_only_f2_queues_them() {
    let (root, mut harness) = setup().await;
    for name in ["d.txt", "e.txt", "f.txt"] {
        fs::write(root.path().join(name), name).expect("write");
    }
    harness
        .app
        .engine()
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("read again");
    harness.press("Ctrl+Shift+Q").await;
    let held = new_list(&mut harness, "Held").await;
    harness.press("Escape").await;
    // Listed as run at once, if at all (RC-010-25), never as a job of a list.
    let in_a_list = |harness: &Harness, id: JobId| {
        harness
            .app
            .engine()
            .ops()
            .entries()
            .iter()
            .any(|entry| entry.job.id == id && !entry.at_once)
    };

    // OK: at once, in the progress window, in no list.
    for (key, name) in [("F5", "c.txt"), ("F6", "d.txt"), ("Shift+F8", "e.txt")] {
        cursor_on(&mut harness, name).await;
        harness.press(key).await;
        harness.press("Enter").await;
        let job = newest(&harness);
        let tracked = harness.app.file_ops.jobs.get(job).map(|job| job.foreground);
        assert!(
            tracked.is_none_or(|foreground| foreground),
            "{key}: the progress window"
        );
        assert!(!in_a_list(&harness, job), "{key}: in no queue list");
        assert_eq!(over(&mut harness, job).await, JobState::Completed, "{key}");
    }
    assert!(root.path().join("out").join("c.txt").exists());
    assert!(root.path().join("out").join("d.txt").exists());
    assert!(!root.path().join("e.txt").exists(), "deleted at once");

    // F2: into the chosen list, which holds it - the delete dialog has it as F5's has.
    cursor_on(&mut harness, "f.txt").await;
    harness.press("Shift+F8").await;
    let dialog = harness.app.file_ops.dialog.as_ref().expect("the dialog");
    assert!(dialog.queues(), "F2 in the delete dialog");
    harness
        .send(Message::FileOps(FileOpsMsg::QueueList(held)))
        .await;
    harness.press("F2").await;
    let job = newest(&harness);
    let entry = harness
        .app
        .engine()
        .ops()
        .entries()
        .into_iter()
        .find(|entry| entry.job.id == job)
        .expect("listed");
    assert_eq!((entry.list, entry.job.state), (held, JobState::Queued));
    assert!(root.path().join("f.txt").exists(), "held, not deleted");
    let started = Command::QueueStart { list: held };
    harness
        .app
        .engine()
        .dispatch(started)
        .await
        .expect("started");
    assert_eq!(over(&mut harness, job).await, JobState::Completed);
    assert!(!root.path().join("f.txt").exists());
}
