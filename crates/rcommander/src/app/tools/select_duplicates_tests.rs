//! "Select duplicate files" through `apply`, against a real engine: Num+ over a duplicate result
//! opens it, OK selects in the panel and closes, Apply selects and stays, the folder tab marks by
//! pattern; Num+ elsewhere keeps the mask prompt.

use std::{
    fs,
    time::{Duration, SystemTime},
};

use rc_engine::{Command, PanelId};
use tempfile::TempDir;

use super::{FilesRule, SelMsg, SelTab};
use crate::app::{
    Message,
    harness::{self, Harness},
    tools::ToolsMsg,
};
use rc_engine::search::DatePick;

/// Two copies of `photo.jpg` (in `a`, 100 s after the epoch, and in `b`, 200 s) and two of
/// `x.txt` (in `a` and `b/old`, both 50 s).
fn tree() -> TempDir {
    let root = harness::tree(&[
        ("a/photo.jpg", "jpg"),
        ("b/photo.jpg", "jpg"),
        ("a/x.txt", "x"),
        ("b/old/x.txt", "x"),
    ]);
    for (name, seconds) in [
        ("a/photo.jpg", 100),
        ("b/photo.jpg", 200),
        ("a/x.txt", 50),
        ("b/old/x.txt", 50),
    ] {
        let file = fs::File::options()
            .write(true)
            .open(root.path().join(name))
            .expect("open");
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .expect("time");
    }
    root
}

/// The left panel shows the two groups as "Feed to listbox" leaves them.
async fn fed(harness: &mut Harness, root: &TempDir) {
    let paths = ["a/photo.jpg", "b/photo.jpg", "a/x.txt", "b/old/x.txt"]
        .iter()
        .map(|name| root.path().join(name).display().to_string())
        .collect();
    harness
        .app
        .engine()
        .dispatch(Command::FeedPaths {
            panel: Some(PanelId::Left),
            root: None,
            paths,
            groups: vec![0, 0, 1, 1],
        })
        .await
        .expect("feed");
    for _ in 0..500 {
        let state = harness.app.engine().panel(PanelId::Left);
        if state.flat.as_ref().is_some_and(|flat| !flat.loading) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    harness.settle().await;
}

async fn send(harness: &mut Harness, message: SelMsg) {
    harness
        .send(Message::Tools(ToolsMsg::Select(message)))
        .await;
}

fn selected(harness: &Harness) -> Vec<String> {
    let state = harness.app.engine().panel(PanelId::Left);
    let mut names: Vec<String> = state.selection.iter().cloned().collect();
    names.sort();
    names
}

fn rel(parts: &[&str]) -> String {
    parts
        .iter()
        .collect::<std::path::PathBuf>()
        .display()
        .to_string()
}

#[tokio::test]
async fn num_plus_over_a_duplicate_result_opens_the_dialog_and_ok_selects() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    fed(&mut harness, &root).await;
    harness.press("NumAdd").await;
    let dialog = harness.app.tools.select.as_ref().expect("the dialog");
    assert!(dialog.open);
    assert_eq!(
        dialog.folders,
        ["a", "b", rel(&["b", "old"]).as_str()],
        "the result's folders"
    );
    send(
        &mut harness,
        SelMsg::Rule(FilesRule::Date(DatePick::AllOlder)),
    )
    .await;
    send(&mut harness, SelMsg::Ok).await;
    assert!(!harness.app.tools.select_open(), "OK closes");
    assert_eq!(selected(&harness), [rel(&["a", "photo.jpg"])]);
    // The choice stays for the session.
    harness.press("NumAdd").await;
    let dialog = harness.app.tools.select.as_ref().expect("kept");
    assert_eq!(dialog.rule, FilesRule::Date(DatePick::AllOlder));
}

#[tokio::test]
async fn apply_stays_open_and_leave_one_keeps_a_file_per_group() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    fed(&mut harness, &root).await;
    harness.press("NumAdd").await;
    send(&mut harness, SelMsg::Mask("*".to_owned())).await;
    send(&mut harness, SelMsg::LeaveOne(true)).await;
    send(&mut harness, SelMsg::Apply).await;
    assert!(harness.app.tools.select_open(), "Apply stays");
    let names = selected(&harness);
    assert_eq!(names.len(), 2, "one file left in each group: {names:?}");
    assert!(
        !names.contains(&rel(&["b", "photo.jpg"])),
        "the newest stays"
    );
    harness.press("Escape").await;
    assert!(!harness.app.tools.select_open());
}

#[tokio::test]
async fn the_folder_tab_marks_by_pattern_and_selects_with_subfolders() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    fed(&mut harness, &root).await;
    harness.press("NumAdd").await;
    send(&mut harness, SelMsg::Tab(SelTab::Folders)).await;
    send(&mut harness, SelMsg::Pattern("b".to_owned())).await;
    send(&mut harness, SelMsg::ParentOnly(true)).await;
    send(&mut harness, SelMsg::PatternMark(true)).await;
    let dialog = harness.app.tools.select.as_ref().expect("open");
    assert_eq!(
        dialog.marked.iter().cloned().collect::<Vec<_>>(),
        ["b"],
        "only the direct parent"
    );
    send(&mut harness, SelMsg::Subfolders(true)).await;
    send(&mut harness, SelMsg::LeaveOne(false)).await;
    send(&mut harness, SelMsg::Ok).await;
    assert_eq!(
        selected(&harness),
        [rel(&["b", "old", "x.txt"]), rel(&["b", "photo.jpg"])]
    );
}

#[tokio::test]
async fn num_plus_elsewhere_keeps_the_mask_prompt() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("NumAdd").await;
    assert!(harness.app.tools.select.is_none());
    assert!(
        harness.app.prompt.is_some(),
        "Total Commander's Select group"
    );
}
