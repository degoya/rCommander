//! The window's way into comparing (`compare.open_diff`, `sync.open_dialog`, the engine methods
//! it calls) and `sync.apply`, against two real trees.

use std::{fs, path::PathBuf, time::Duration};

use rc_ops::{DeleteMode, JobSnapshot, JobState};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::test_support;
use crate::{
    Command, Engine, EngineEvent, Outcome, PanelId, VfsPath,
    compare::{RowKind, TextOptions},
    sync::{RowAction, SyncAction, SyncApplyRequest, SyncMode, SyncOptions, SyncRun},
};

/// `left/` and `right/`, each with `same.txt` and `diff.txt` (same size, other bytes);
/// `only.txt` on the left only, `extra.txt` on the right only.
fn trees() -> TempDir {
    test_support::tree(&[
        ("left/same.txt", "same\n"),
        ("left/diff.txt", "one\n"),
        ("right/same.txt", "same\n"),
        ("right/diff.txt", "two\n"),
        ("left/only.txt", "x"),
        ("right/extra.txt", "y"),
    ])
}

async fn engine(root: &TempDir) -> Engine {
    test_support::open(&root.path().join("left"), &root.path().join("right")).await
}

fn side(root: &TempDir, side: &str, name: &str) -> PathBuf {
    root.path().join(side).join(name)
}

/// Every job of `outcome`, waited for.
async fn finished(engine: &Engine, outcome: Outcome) -> Vec<JobSnapshot> {
    let Outcome::Jobs { jobs } = outcome else {
        panic!("jobs, got {outcome:?}");
    };
    let mut done = Vec::new();
    for job in jobs {
        let waited = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
            .await
            .expect("in time")
            .expect("known");
        assert_eq!(waited.state, JobState::Completed, "{waited:?}");
        done.push(waited);
    }
    done
}

#[tokio::test]
async fn the_open_commands_announce_what_the_window_opens() {
    let root = trees();
    let engine = engine(&root).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::OpenSyncDialog {})
        .await
        .expect("opens");
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::SyncDirs {
            left: VfsPath::local(root.path().join("left")),
            right: VfsPath::local(root.path().join("right")),
        }
    );
    for (panel, name) in [(PanelId::Left, "diff.txt"), (PanelId::Right, "diff.txt")] {
        engine
            .dispatch(Command::SetCursor {
                panel: Some(panel),
                target: crate::CursorTarget::Name(name.into()),
            })
            .await
            .expect("cursor");
    }
    while events.try_recv().is_ok() {}
    engine.dispatch(Command::OpenDiff {}).await.expect("opens");
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::CompareFiles {
            left: VfsPath::local(side(&root, "left", "diff.txt")),
            right: VfsPath::local(side(&root, "right", "diff.txt")),
        }
    );
}

#[tokio::test]
async fn the_window_diffs_text_and_compares_bytes() {
    let root = trees();
    let engine = engine(&root).await;
    let (left, right) = (
        VfsPath::local(side(&root, "left", "diff.txt")),
        VfsPath::local(side(&root, "right", "diff.txt")),
    );
    let cancel = CancellationToken::new();
    let diff = engine
        .diff_paths(&left, &right, &TextOptions::default(), &cancel)
        .await
        .expect("reads")
        .expect("text");
    assert_eq!(diff.rows.len(), 1);
    assert_eq!(diff.rows[0].kind, RowKind::Changed);
    let bytes = engine
        .compare_paths(&left, &right, &cancel)
        .await
        .expect("reads");
    assert!(!bytes.identical);
    assert_eq!(bytes.first_difference.map(|at| at.offset), Some(0));
}

#[tokio::test]
async fn sync_apply_copies_both_ways_from_the_proposal() {
    let root = trees();
    let engine = engine(&root).await;
    let outcome = engine
        .dispatch(Command::SyncApply(SyncApplyRequest::default()))
        .await
        .expect("runs");
    finished(&engine, outcome).await;
    assert_eq!(
        fs::read(side(&root, "right", "only.txt")).expect("copied"),
        b"x"
    );
    assert_eq!(
        fs::read(side(&root, "left", "extra.txt")).expect("copied"),
        b"y"
    );
    // Same size and time: equal, not copied.
    assert_eq!(
        fs::read(side(&root, "right", "diff.txt")).expect("kept"),
        b"two\n"
    );
}

#[tokio::test]
async fn sync_apply_deletes_only_when_confirmed() {
    let root = trees();
    let engine = engine(&root).await;
    let mut request = SyncApplyRequest {
        options: SyncOptions {
            mode: SyncMode::MirrorToRight,
            ..SyncOptions::default()
        },
        ..SyncApplyRequest::default()
    };
    let error = engine
        .dispatch(Command::SyncApply(request.clone()))
        .await
        .expect_err("deletes need a yes");
    assert_eq!(error.code(), "sync_deletes_unconfirmed");
    assert!(side(&root, "right", "extra.txt").exists());
    assert!(
        !side(&root, "right", "only.txt").exists(),
        "nothing ran at all"
    );
    request.run = SyncRun {
        deletes_confirmed: true,
        delete_mode: DeleteMode::Permanent,
        ..SyncRun::default()
    };
    let outcome = engine
        .dispatch(Command::SyncApply(request))
        .await
        .expect("runs");
    finished(&engine, outcome).await;
    assert!(!side(&root, "right", "extra.txt").exists());
    assert!(side(&root, "right", "only.txt").exists());
}

#[tokio::test]
async fn sync_apply_without_the_proposal_runs_only_the_named_rows() {
    let root = trees();
    let engine = engine(&root).await;
    let request = SyncApplyRequest {
        proposed: false,
        actions: vec![RowAction {
            path: PathBuf::from("extra.txt"),
            action: SyncAction::CopyToLeft,
        }],
        ..SyncApplyRequest::default()
    };
    let outcome = engine
        .dispatch(Command::SyncApply(request))
        .await
        .expect("runs");
    let jobs = finished(&engine, outcome).await;
    assert_eq!(jobs.len(), 1);
    assert!(side(&root, "left", "extra.txt").exists());
    assert!(!side(&root, "right", "only.txt").exists());
}

/// A test read judge: `only.txt` is secret.
fn only_is_secret(path: &std::path::Path) -> Option<&'static str> {
    (path.file_name()? == "only.txt").then_some("a test secret")
}

/// Under MCP a synchronization that would copy a guarded file runs nothing; the window runs it.
#[tokio::test]
async fn an_mcp_sync_of_a_guarded_file_is_refused() {
    let root = trees();
    let engine = engine(&root).await;
    assert!(engine.attach_read_judge(only_is_secret));
    let error = engine
        .dispatch_mcp(Command::SyncApply(SyncApplyRequest::default()))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "sensitive_path");
    assert!(engine.ops().jobs().is_empty(), "no job was submitted");
    let outcome = engine
        .dispatch(Command::SyncApply(SyncApplyRequest::default()))
        .await
        .expect("the window runs it");
    finished(&engine, outcome).await;
    assert!(side(&root, "right", "only.txt").exists());
}

/// Writes `content` to `path` with the modification time `secs` after the epoch.
fn file_at(path: PathBuf, content: &str, secs: u64) {
    fs::write(&path, content).expect("write");
    let file = fs::File::options().write(true).open(&path).expect("open");
    file.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(secs))
        .expect("time");
}

fn selected(engine: &Engine, panel: PanelId) -> Vec<String> {
    engine.panel(panel).selection.into_iter().collect()
}

/// Shift+F2 as Total Commander: each panel selects its files the other lacks or has older, both
/// a pair of one time and two sizes; folders and equal files stay unselected.
#[tokio::test]
async fn select_differences_marks_newer_single_and_resized_files() {
    let root = tempfile::tempdir().expect("temp dir");
    let (left, right) = (root.path().join("left"), root.path().join("right"));
    fs::create_dir_all(left.join("sub")).expect("mkdir");
    fs::create_dir(&right).expect("mkdir");
    let day = 1_700_000_000;
    file_at(left.join("new.txt"), "a", day + 100);
    file_at(right.join("new.txt"), "a", day);
    file_at(left.join("big.txt"), "aa", day);
    file_at(right.join("big.txt"), "aaa", day);
    file_at(left.join("same.txt"), "abcd", day);
    file_at(right.join("same.txt"), "wxyz", day);
    file_at(left.join("only.txt"), "x", day);
    file_at(right.join("extra.txt"), "y", day);
    let engine = test_support::open(&left, &right).await;
    engine
        .dispatch(Command::SelectDifferences { by_content: false })
        .await
        .expect("compares");
    assert_eq!(
        selected(&engine, PanelId::Left),
        ["big.txt", "new.txt", "only.txt"]
    );
    assert_eq!(selected(&engine, PanelId::Right), ["big.txt", "extra.txt"]);
    // By content the pair of one size and other bytes differs too, and the pair of equal bytes
    // is equal whatever its times (as the synchronize-dirs plan compares by content).
    engine
        .dispatch(Command::SelectDifferences { by_content: true })
        .await
        .expect("compares");
    assert_eq!(
        selected(&engine, PanelId::Left),
        ["big.txt", "only.txt", "same.txt"]
    );
    assert_eq!(
        selected(&engine, PanelId::Right),
        ["big.txt", "extra.txt", "same.txt"]
    );
}

/// RC-020-08: with `selection.include_folders` on (set as MCP's `settings_set` does), Shift+F2
/// selects the folders only one side has too; a folder on both sides is not compared. Off, as
/// before.
#[tokio::test]
async fn select_differences_marks_lone_folders_when_folders_take_part() {
    let root = tempfile::tempdir().expect("temp dir");
    let (left, right) = (root.path().join("left"), root.path().join("right"));
    for dir in [
        left.join("only_left"),
        left.join("both"),
        right.join("only_right"),
        right.join("both"),
    ] {
        fs::create_dir_all(dir).expect("mkdir");
    }
    fs::write(left.join("both").join("inside.txt"), "x").expect("write");
    let day = 1_700_000_000;
    file_at(left.join("only.txt"), "x", day);
    file_at(left.join("same.txt"), "s", day);
    file_at(right.join("same.txt"), "s", day);
    let engine = test_support::open(&left, &right).await;
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    let compare = || Command::SelectDifferences { by_content: false };

    engine.dispatch(compare()).await.expect("compares");
    assert_eq!(selected(&engine, PanelId::Left), ["only.txt"]);
    assert!(selected(&engine, PanelId::Right).is_empty());

    engine
        .dispatch_mcp(Command::SetSetting {
            key: crate::settings::SELECT_FOLDERS_KEY.into(),
            value: serde_json::json!(true),
        })
        .await
        .expect("an MCP caller may set it");
    engine.dispatch(compare()).await.expect("compares");
    assert_eq!(selected(&engine, PanelId::Left), ["only.txt", "only_left"]);
    assert_eq!(selected(&engine, PanelId::Right), ["only_right"]);
}
