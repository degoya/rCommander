//! The diff window and the synchronize-dirs dialog through `apply`, against a real engine:
//! their keys open them, the keyboard stays with them, a plan becomes jobs of the file
//! operations. The iced tasks that compare do not run in the harness; the tests run the same
//! engine calls and hand the answers over, as those tasks do.

use std::{fs, time::Duration};

use rc_engine::{ConflictChoice, JobState, PanelId, sync::SyncAction};
use tempfile::TempDir;

use super::{CompareMsg, Flag, Phase, Shown, load};
use crate::app::{Message, file_ops::FileOpsMsg, harness::Harness};

/// `left/` and `right/`: `diff.txt` differs in its second line, `only.txt` is only on the left,
/// `extra.txt` only on the right.
fn trees() -> TempDir {
    crate::app::harness::tree(&[
        ("left/diff.txt", "one\ntwo\n"),
        ("right/diff.txt", "one\nTWO\n"),
        ("left/only.txt", "x"),
        ("right/extra.txt", "y"),
    ])
}

async fn harness(root: &TempDir) -> Harness {
    Harness::open(&root.path().join("left"), &root.path().join("right")).await
}

async fn send(harness: &mut Harness, message: CompareMsg) {
    harness.send(Message::Compare(message)).await;
}

/// Puts both panels' cursors on `diff.txt`.
async fn cursors_on_diff(harness: &mut Harness) {
    for panel in PanelId::BOTH {
        harness
            .app
            .engine()
            .dispatch(rc_engine::Command::SetCursor {
                panel: Some(panel),
                target: rc_engine::CursorTarget::Name("diff.txt".into()),
            })
            .await
            .expect("cursor");
    }
    harness.settle().await;
}

/// Runs the open diff window's comparison and hands its answer over, as the task does.
async fn loaded(harness: &mut Harness) {
    let window = harness.app.compare.diff.as_ref().expect("open");
    let generation = window.generation;
    let result = load(
        harness.app.engine().clone(),
        window.left.clone(),
        window.right.clone(),
        window.options,
        window.cancel.clone(),
    )
    .await;
    send(harness, CompareMsg::DiffLoaded(generation, result)).await;
}

/// Runs the open dialog's comparison and hands the plan over.
async fn planned(harness: &mut Harness) {
    let dialog = harness.app.compare.sync.as_ref().expect("open");
    assert_eq!(dialog.phase, Phase::Comparing);
    let options = dialog.form.options().expect("options");
    let plan = harness
        .app
        .engine()
        .plan_sync(&dialog.left, &dialog.right, &options, &dialog.cancel)
        .await
        .map(std::sync::Arc::new)
        .map_err(|error| error.code());
    let generation = dialog.generation;
    send(harness, CompareMsg::SyncPlanned(generation, plan)).await;
}

/// The confirmation's OK, with the queue's answer handed over as the task does.
async fn run(harness: &mut Harness) {
    let (plan, run) = harness
        .app
        .compare
        .sync
        .as_ref()
        .and_then(|dialog| dialog.confirmed())
        .expect("confirmed");
    send(harness, CompareMsg::SyncRun).await;
    assert_eq!(
        harness
            .app
            .compare
            .sync
            .as_ref()
            .map(|dialog| &dialog.phase),
        Some(&Phase::Starting)
    );
    let jobs = harness
        .app
        .engine()
        .sync_jobs(&plan, &run)
        .await
        .map_err(|error| error.code());
    send(harness, CompareMsg::SyncStarted(jobs)).await;
}

/// Waits for every job of the queue and lets the window see them end.
async fn all_done(harness: &mut Harness) {
    let jobs = harness.app.engine().ops().jobs();
    assert!(!jobs.is_empty());
    for job in jobs {
        let done = tokio::time::timeout(
            Duration::from_secs(10),
            harness.app.engine().ops().wait(job.id),
        )
        .await
        .expect("in time")
        .expect("known");
        assert_eq!(done.state, JobState::Completed, "{done:?}");
    }
    harness.settle().await;
}

#[tokio::test]
async fn ctrl_shift_f3_shows_the_two_files_side_by_side() {
    let root = trees();
    let mut harness = harness(&root).await;
    cursors_on_diff(&mut harness).await;
    harness.press("Ctrl+Shift+F3").await;
    let window = harness.app.compare.diff.as_ref().expect("the window");
    assert_eq!(window.left.path, root.path().join("left").join("diff.txt"));
    assert!(matches!(window.shown, Shown::Loading));
    loaded(&mut harness).await;
    let window = harness.app.compare.diff.as_ref().expect("open");
    assert_eq!(window.changes, 1);
    assert_eq!(window.current, Some(1), "on the first difference");

    // The keys stay with the window: Tab switches no panel.
    let active = harness.app.active;
    harness.press("Tab").await;
    assert_eq!(harness.app.active, active);
    send(
        &mut harness,
        CompareMsg::DiffOption(super::DiffOption::Case, true),
    )
    .await;
    loaded(&mut harness).await;
    let window = harness.app.compare.diff.as_ref().expect("open");
    assert!(window.options.ignore_case);
    assert_eq!(window.changes, 0, "TWO and two are equal ignoring case");
    harness.press("Escape").await;
    assert!(harness.app.compare.diff.is_none());
}

#[tokio::test]
async fn without_two_files_the_window_stays_shut_and_says_why() {
    let root = trees();
    // Folders sort first: the left cursor starts on `sub`, no file.
    fs::create_dir(root.path().join("left").join("sub")).expect("mkdir");
    let mut harness = harness(&root).await;
    harness.press("Ctrl+Shift+F3").await;
    assert!(harness.app.compare.diff.is_none());
    let active = harness.app.active;
    assert!(harness.app.pane(active).error.is_some());
}

#[tokio::test]
async fn alt_shift_f2_compares_the_panels_and_copies_both_ways() {
    let root = trees();
    let mut harness = harness(&root).await;
    harness.press("Alt+Shift+F2").await;
    let dialog = harness.app.compare.sync.as_ref().expect("the dialog");
    assert_eq!(dialog.left.path, root.path().join("left"));
    assert_eq!(dialog.right.path, root.path().join("right"));
    harness.press("Enter").await;
    planned(&mut harness).await;
    let dialog = harness.app.compare.sync.as_ref().expect("open");
    assert_eq!(dialog.phase, Phase::Ready);
    let actions: Vec<_> = (0..dialog.shown.len())
        .filter_map(|at| dialog.row(at))
        .map(|(_, row)| (row.path.display().to_string(), row.action))
        .collect();
    assert!(actions.contains(&("only.txt".into(), SyncAction::CopyToRight)));
    assert!(actions.contains(&("extra.txt".into(), SyncAction::CopyToLeft)));

    send(&mut harness, CompareMsg::SyncAsk).await;
    run(&mut harness).await;
    assert!(harness.app.compare.sync.is_none(), "the jobs took over");
    all_done(&mut harness).await;
    assert!(root.path().join("right").join("only.txt").exists());
    assert!(root.path().join("left").join("extra.txt").exists());
}

#[tokio::test]
async fn a_mirror_deletes_only_what_the_confirmation_lets_through() {
    let root = trees();
    let mut harness = harness(&root).await;
    harness.press("Alt+Shift+F2").await;
    send(
        &mut harness,
        CompareMsg::SyncMode(rc_engine::sync::SyncMode::MirrorToRight),
    )
    .await;
    send(&mut harness, CompareMsg::SyncCompare).await;
    planned(&mut harness).await;
    send(&mut harness, CompareMsg::SyncAsk).await;
    let confirm = harness
        .app
        .compare
        .sync
        .as_ref()
        .and_then(|dialog| dialog.confirm.as_ref())
        .expect("asked");
    assert!(confirm.has_deletes());
    send(&mut harness, CompareMsg::SyncFlag(Flag::Deletes, false)).await;
    run(&mut harness).await;
    all_done(&mut harness).await;
    assert!(
        root.path().join("right").join("extra.txt").exists(),
        "the delete was unticked"
    );
    assert!(root.path().join("right").join("only.txt").exists());
}

#[tokio::test]
async fn escape_stops_a_comparison_then_closes_the_dialog() {
    let root = trees();
    let mut harness = harness(&root).await;
    harness.press("Alt+Shift+F2").await;
    harness.press("Enter").await;
    let cancel = harness
        .app
        .compare
        .sync
        .as_ref()
        .map(|dialog| dialog.cancel.clone())
        .expect("open");
    harness.press("Escape").await;
    assert!(cancel.is_cancelled());
    assert!(harness.app.compare.sync.is_some(), "still open");
    planned(&mut harness).await;
    let dialog = harness.app.compare.sync.as_ref().expect("open");
    assert_eq!(dialog.phase, Phase::Failed("cancelled"));
    harness.press("Escape").await;
    assert!(harness.app.compare.sync.is_none());
}

/// The audit's D8: a second "Synchronize dirs" replaced the dialog and its comparison ran on
/// unseen. It is stopped first.
#[tokio::test]
async fn a_new_sync_dialog_stops_the_comparison_of_the_one_it_replaces() {
    let root = trees();
    let mut harness = harness(&root).await;
    harness.press("Alt+Shift+F2").await;
    harness.press("Enter").await;
    let cancel = harness
        .app
        .compare
        .sync
        .as_ref()
        .map(|dialog| dialog.cancel.clone())
        .expect("open");
    // Opened again from elsewhere - the menu, an MCP call - while the dialog has the keys.
    harness
        .app
        .engine()
        .dispatch(rc_engine::Command::OpenSyncDialog {})
        .await
        .expect("opens");
    harness.settle().await;
    assert!(cancel.is_cancelled(), "the old comparison stopped");
    let dialog = harness.app.compare.sync.as_ref().expect("the new dialog");
    assert!(!dialog.cancel.is_cancelled());
}

#[tokio::test]
async fn shift_f2_selects_the_single_files_in_both_panels() {
    let root = trees();
    let mut harness = harness(&root).await;
    harness.press("Shift+F2").await;
    let selection = |panel: PanelId| -> Vec<String> {
        harness
            .app
            .pane(panel)
            .state
            .selection
            .iter()
            .cloned()
            .collect()
    };
    // `diff.txt`: same size, times within the tolerance - not a difference by name, size, time.
    assert_eq!(selection(PanelId::Left), ["only.txt"]);
    assert_eq!(selection(PanelId::Right), ["extra.txt"]);
}

/// "Compare" in the conflict dialog of F5: source and target in the diff window, the question
/// still open underneath; Escape closes only the window, and the question is answered afterwards
/// (RC-010-07).
#[tokio::test]
async fn compare_in_the_conflict_dialog_opens_the_pair_over_the_question() {
    let root = trees();
    let mut harness = harness(&root).await;
    cursors_on_diff(&mut harness).await;
    harness.press("F5").await;
    harness
        .send(Message::FileOps(FileOpsMsg::Start { background: false }))
        .await;
    let job = harness
        .app
        .engine()
        .ops()
        .jobs()
        .last()
        .map(|job| job.id)
        .expect("a job");
    for _ in 0..500 {
        if harness
            .app
            .engine()
            .ops()
            .job(job)
            .expect("known")
            .question
            .is_some()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    harness.settle().await;
    assert!(harness.app.file_ops.jobs.asking().is_some(), "the conflict");
    harness
        .send(Message::FileOps(FileOpsMsg::Compare(job)))
        .await;
    let window = harness.app.compare.diff.as_ref().expect("the window");
    assert_eq!(window.left.path, root.path().join("left").join("diff.txt"));
    assert_eq!(
        window.right.path,
        root.path().join("right").join("diff.txt")
    );
    harness.press("Escape").await;
    assert!(harness.app.compare.diff.is_none());
    assert!(
        harness.app.file_ops.jobs.asking().is_some(),
        "the question stays"
    );
    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job,
            choice: ConflictChoice::Overwrite,
            for_all: false,
        }))
        .await;
    tokio::time::timeout(
        Duration::from_secs(10),
        harness.app.engine().ops().wait(job),
    )
    .await
    .expect("in time")
    .expect("known");
    harness.settle().await;
    assert!(harness.app.file_ops.jobs.asking().is_none());
    assert_eq!(
        fs::read_to_string(root.path().join("right").join("diff.txt")).expect("read"),
        "one\ntwo\n"
    );
}
