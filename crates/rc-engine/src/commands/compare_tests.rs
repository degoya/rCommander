use std::fs;

use tempfile::TempDir;

use crate::test_support;
use crate::{
    Command, Engine, Outcome, PanelId,
    compare::{FileCompareRequest, FileComparison},
    sync::{DirCompareRequest, DirComparison, PairState, RowFilter, SyncAction},
};

/// `left/` and `right/`, each with `same.txt` and `diff.txt` (same size, other bytes);
/// `only.txt` on the left only.
fn trees() -> TempDir {
    test_support::tree(&[
        ("left/same.txt", "same\n"),
        ("left/diff.txt", "one\n"),
        ("right/same.txt", "same\n"),
        ("right/diff.txt", "two\n"),
        ("left/only.txt", "x"),
    ])
}

async fn engine(root: &TempDir) -> Engine {
    test_support::open(&root.path().join("left"), &root.path().join("right")).await
}

fn files(outcome: Outcome) -> FileComparison {
    match outcome {
        Outcome::FileComparison(comparison) => comparison,
        other => panic!("a file comparison, got {other:?}"),
    }
}

fn dirs(outcome: Outcome) -> DirComparison {
    match outcome {
        Outcome::DirComparison(comparison) => comparison,
        other => panic!("a directory comparison, got {other:?}"),
    }
}

async fn set_cursor(engine: &Engine, panel: PanelId, name: &str) {
    engine
        .dispatch(Command::SetCursor {
            panel: Some(panel),
            target: crate::CursorTarget::Name(name.into()),
        })
        .await
        .expect("cursor");
}

#[tokio::test]
async fn without_paths_the_files_under_both_cursors_are_compared() {
    let root = trees();
    let engine = engine(&root).await;
    set_cursor(&engine, PanelId::Left, "diff.txt").await;
    set_cursor(&engine, PanelId::Right, "diff.txt").await;
    let comparison = files(
        engine
            .dispatch(Command::CompareFiles(FileCompareRequest::default()))
            .await
            .expect("compares"),
    );
    assert_eq!(
        comparison.left.path,
        root.path().join("left").join("diff.txt")
    );
    assert_eq!(
        comparison.right.path,
        root.path().join("right").join("diff.txt")
    );
    assert!(!comparison.identical);
}

#[tokio::test]
async fn two_selected_files_of_the_active_panel_come_first() {
    let root = trees();
    let engine = engine(&root).await;
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["same.txt".into(), "diff.txt".into()],
        })
        .await
        .expect("selects");
    let comparison = files(
        engine
            .dispatch(Command::CompareFiles(FileCompareRequest::default()))
            .await
            .expect("compares"),
    );
    let left = root.path().join("left");
    assert_eq!(comparison.left.path, left.join("diff.txt"));
    assert_eq!(comparison.right.path, left.join("same.txt"));
}

#[tokio::test]
async fn given_paths_are_relative_to_their_panels() {
    let root = trees();
    let engine = engine(&root).await;
    let comparison = files(
        engine
            .dispatch(Command::CompareFiles(FileCompareRequest {
                left: Some("same.txt".into()),
                right: Some("same.txt".into()),
                ..FileCompareRequest::default()
            }))
            .await
            .expect("compares"),
    );
    assert!(comparison.identical);
    assert_eq!(
        comparison.right.path,
        root.path().join("right").join("same.txt")
    );
}

#[tokio::test]
async fn a_directory_under_the_cursor_is_not_a_file_to_compare() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("sub")).expect("mkdir");
    let engine = test_support::headless(root.path()).await;
    let error = engine
        .dispatch(Command::CompareFiles(FileCompareRequest::default()))
        .await
        .expect_err("no files");
    assert_eq!(error.code(), "compare_needs_files");
}

#[tokio::test]
async fn compare_dirs_defaults_to_the_panels_and_lists_differences() {
    let root = trees();
    let engine = engine(&root).await;
    let comparison = dirs(
        engine
            .dispatch(Command::CompareDirs(DirCompareRequest::default()))
            .await
            .expect("compares"),
    );
    assert_eq!(comparison.left.path, root.path().join("left"));
    let shown: Vec<(&str, PairState, SyncAction)> = comparison
        .rows
        .iter()
        .map(|shown| {
            (
                shown.row.path.to_str().expect("utf-8"),
                shown.row.state,
                shown.row.action,
            )
        })
        .collect();
    // diff.txt: same size, times within the tolerance; the content differs but is not compared.
    assert_eq!(
        shown,
        [("only.txt", PairState::LeftOnly, SyncAction::CopyToRight)]
    );
    assert_eq!(comparison.summary.equal, 2);
    assert_eq!(comparison.summary.copy_to_right.files, 1);
    assert!(!comparison.truncated);

    let mut request = DirCompareRequest {
        show: RowFilter::All,
        limit: 1,
        ..DirCompareRequest::default()
    };
    request.options.by_content = true;
    let limited = dirs(
        engine
            .dispatch(Command::CompareDirs(request))
            .await
            .expect("compares"),
    );
    assert_eq!(limited.rows.len(), 1);
    assert!(limited.truncated);
    assert_eq!(limited.summary.different, 1, "by content diff.txt differs");
}

/// A test read judge: `diff.txt` is secret.
fn diff_is_secret(path: &std::path::Path) -> Option<&'static str> {
    (path.file_name()? == "diff.txt").then_some("a test secret")
}

/// Under MCP a comparison of a guarded file is refused, given or picked from the cursors; the
/// window compares it.
#[tokio::test]
async fn an_mcp_comparison_of_a_guarded_file_is_refused() {
    let root = trees();
    let engine = engine(&root).await;
    assert!(engine.attach_read_judge(diff_is_secret));
    let given = FileCompareRequest {
        left: Some("same.txt".into()),
        right: Some("diff.txt".into()),
        ..FileCompareRequest::default()
    };
    let error = engine
        .dispatch_mcp(Command::CompareFiles(given.clone()))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "sensitive_path");
    set_cursor(&engine, PanelId::Left, "diff.txt").await;
    set_cursor(&engine, PanelId::Right, "same.txt").await;
    let error = engine
        .dispatch_mcp(Command::CompareFiles(FileCompareRequest::default()))
        .await
        .expect_err("refused by the cursor's file too");
    assert_eq!(error.code(), "sensitive_path");
    engine
        .dispatch(Command::CompareFiles(given))
        .await
        .expect("the window compares it");
}

/// Under MCP a directory comparison by content never reads a guarded file: its pair is judged
/// by size and time, so a differing content does not show.
#[tokio::test]
async fn an_mcp_comparison_by_content_skips_guarded_files() {
    let root = trees();
    let engine = engine(&root).await;
    assert!(engine.attach_read_judge(diff_is_secret));
    let mut request = DirCompareRequest {
        show: RowFilter::All,
        ..DirCompareRequest::default()
    };
    request.options.by_content = true;
    let guarded = dirs(
        engine
            .dispatch_mcp(Command::CompareDirs(request.clone()))
            .await
            .expect("compares"),
    );
    assert_eq!(guarded.summary.different, 0, "diff.txt is not read");
    let read = dirs(
        engine
            .dispatch(Command::CompareDirs(request))
            .await
            .expect("compares"),
    );
    assert_eq!(read.summary.different, 1, "the window reads it");
}

/// A comparison stops with `cancelled` once the token of its call is cancelled, from the window
/// and through MCP alike; the same call with a live token runs to its end (RC-080-50).
#[tokio::test]
async fn a_comparison_stops_with_the_token_of_its_call() {
    let root = trees();
    let engine = engine(&root).await;
    let cancelled = tokio_util::sync::CancellationToken::new();
    cancelled.cancel();
    let commands = [
        Command::CompareFiles(FileCompareRequest {
            left: Some("diff.txt".into()),
            right: Some("diff.txt".into()),
            ..FileCompareRequest::default()
        }),
        Command::CompareDirs(DirCompareRequest::default()),
        Command::SelectDifferences { by_content: true },
    ];
    for command in commands {
        let id = command.id();
        let stopped = engine
            .dispatch_cancellable(command.clone(), cancelled.clone())
            .await;
        assert_eq!(stopped.expect_err(id).code(), "cancelled", "{id}");
        let through_mcp = engine
            .dispatch_mcp_cancellable(command.clone(), cancelled.clone())
            .await;
        assert_eq!(through_mcp.expect_err(id).code(), "cancelled", "{id}");
        engine
            .dispatch_cancellable(command, tokio_util::sync::CancellationToken::new())
            .await
            .expect(id);
    }
}
