use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use rc_ops::{JobOptions, JobState};

use super::common_folder;
use crate::test_support::{self, expect_jobs};
use crate::{Command, Engine, PanelId, PanelState, ignore_list::IgnoreList};

/// The panel once its flat view has read everything.
async fn settled(engine: &Engine) -> PanelState {
    for _ in 0..500 {
        let state = engine.panel(PanelId::Left);
        if state.flat.as_ref().is_some_and(|flat| !flat.loading) {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the flat view did not finish");
}

fn names(state: &PanelState) -> Vec<String> {
    let mut names: Vec<String> = state
        .entries
        .iter()
        .map(|entry| entry.name.replace('\\', "/"))
        .collect();
    names.sort();
    names
}

#[test]
fn the_common_folder_holds_every_path() {
    let root = if cfg!(windows) { "C:\\data" } else { "/data" };
    let path = |rest: &str| PathBuf::from(root).join(rest);
    assert_eq!(
        common_folder(&[path("a/x.txt"), path("a/b/y.txt")]),
        path("a")
    );
    assert_eq!(common_folder(&[path("a/x.txt"), path("c/y.txt")]), path(""));
    assert_eq!(common_folder(&[path("a/x.txt")]), path("a"));
}

#[tokio::test]
async fn a_list_file_feeds_the_panel() {
    let (root, engine) = test_support::engine(&[
        ("src/a.rs", "a"),
        ("src/deep/b.rs", "b"),
        ("other.txt", "o"),
    ])
    .await;
    let list = format!(
        "{}\n{}\n",
        root.path().join("src/a.rs").display(),
        root.path().join("src/deep/b.rs").display()
    );
    fs::write(root.path().join("list.txt"), list).expect("list");
    engine
        .dispatch(Command::LoadList {
            panel: None,
            path: "list.txt".into(),
        })
        .await
        .expect("loads");
    let state = settled(&engine).await;
    assert_eq!(state.path.path, root.path().join("src"));
    assert_eq!(names(&state), ["a.rs", "deep/b.rs"]);
}

#[tokio::test]
async fn quoted_names_and_missing_files() {
    let (root, engine) = test_support::engine(&[("a b.txt", "a"), ("c.txt", "c")]).await;
    let list = format!(
        "\"{}\"\n{}\nrelative.txt\n{}\n",
        root.path().join("a b.txt").display(),
        root.path().join("gone.txt").display(),
        root.path().join("c.txt").display(),
    );
    fs::write(root.path().join("list.txt"), list).expect("list");
    engine
        .dispatch(Command::LoadList {
            panel: None,
            path: "list.txt".into(),
        })
        .await
        .expect("loads");
    let state = settled(&engine).await;
    assert_eq!(names(&state), ["a b.txt", "c.txt"]);
    assert_eq!(
        state.flat.as_ref().map(|flat| flat.skipped),
        Some(1),
        "the missing one is counted, the relative one is no path"
    );
    fs::write(root.path().join("empty.txt"), "relative\n").expect("list");
    let error = engine
        .dispatch(Command::LoadList {
            panel: None,
            path: "empty.txt".into(),
        })
        .await
        .expect_err("nothing to show");
    assert_eq!(error.code(), "list_file_empty");
}

fn bak_list() -> Option<Arc<IgnoreList>> {
    Some(Arc::new(IgnoreList::parse("*.bak")))
}

#[tokio::test]
async fn copy_and_move_skip_ignored_entries() {
    let root = test_support::tree(&[
        ("docs/a.txt", "a"),
        ("docs/a.bak", "old"),
        ("docs/sub/b.bak", "old"),
        ("out/", ""),
        ("moved/", ""),
    ]);
    let engine = test_support::open(root.path(), &root.path().join("out")).await;
    engine.apply_ignore_list(bak_list());
    for (command, from) in [
        (
            Command::OpsCopy(crate::TransferRequest {
                panel: None,
                sources: vec!["docs".into()],
                target: None,
                options: JobOptions::default(),
                rename_mask: None,
            }),
            "copy",
        ),
        (
            Command::OpsMove(crate::TransferRequest {
                panel: None,
                sources: vec!["docs".into()],
                target: Some(root.path().join("moved").display().to_string()),
                options: JobOptions::default(),
                rename_mask: None,
            }),
            "move",
        ),
    ] {
        let jobs = expect_jobs(engine.dispatch(command).await.expect("queued"));
        for job in jobs {
            let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
                .await
                .expect("in time")
                .expect("known");
            assert_eq!(done.state, JobState::Completed, "{from}: {done:?}");
            if from == "copy" {
                assert_eq!(done.progress.files_total, 1, "ignored ones are not counted");
            }
        }
    }
    let out = root.path().join("out/docs");
    assert!(out.join("a.txt").is_file());
    assert!(!out.join("a.bak").exists(), "not copied");
    assert!(!out.join("sub/b.bak").exists(), "not copied below either");
    let moved = root.path().join("moved/docs");
    // The move renames `docs` whole on the same disk, as Total Commander's does - or, copied,
    // leaves the ignored ones behind in their folder.
    if moved.join("a.bak").exists() {
        assert!(!root.path().join("docs").exists());
    } else {
        assert!(moved.join("a.txt").is_file());
        assert!(
            root.path().join("docs/a.bak").is_file(),
            "left where it was"
        );
    }
    engine.apply_ignore_list(None);
}

#[tokio::test]
async fn the_delete_question_counts_ignored_entries() {
    let (_root, engine) = test_support::engine(&[
        ("dir/a.bak", "x"),
        ("dir/b.txt", "x"),
        ("dir/sub/c.bak", "x"),
        ("dir/sub/d.txt", "x"),
        ("free.txt", "x"),
    ])
    .await;
    let names = vec!["dir".to_owned(), "free.txt".to_owned()];
    assert_eq!(
        engine.ignored_below(PanelId::Left, &names).await,
        0,
        "no list"
    );
    engine.apply_ignore_list(bak_list());
    assert_eq!(engine.ignored_below(PanelId::Left, &names).await, 2);
    assert!(
        !engine
            .panel(PanelId::Left)
            .entries
            .iter()
            .any(|entry| entry.name.ends_with(".bak"))
    );
    engine.apply_ignore_list(None);
}

#[tokio::test]
async fn the_list_hides_entries_in_the_panel_after_a_refresh() {
    let (_root, engine) = test_support::engine(&[("keep.txt", "x"), ("old.bak", "x")]).await;
    engine.apply_ignore_list(bak_list());
    engine
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("refresh");
    let shown: Vec<String> = engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(shown, ["keep.txt"]);
    engine.apply_ignore_list(None);
}
