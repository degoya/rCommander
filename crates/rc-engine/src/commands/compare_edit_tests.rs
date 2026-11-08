//! `compare.by_content` with and without the external tool, and `compare.apply_blocks`, against
//! real files (RC-100-94).

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rc_vfs::{BackendId, VfsPath, unix_seconds};
use tempfile::TempDir;

use crate::{
    Command, Engine, EngineEvent, Outcome, PanelId,
    compare::{ApplyBlocksRequest, ByContentRequest, Direction, FileComparison, FileStamp},
    settings::{COMPARE_ARGS_KEY, COMPARE_TOOL_KEY, is_protected},
    test_support,
};

/// What `start` was handed, by every test of this module: the program and its arguments.
pub(super) static STARTED: Mutex<Vec<(PathBuf, Vec<OsString>)>> = Mutex::new(Vec::new());

/// The arguments the program `tool` was last started with.
fn started_with(tool: &str) -> Option<Vec<String>> {
    STARTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .rev()
        .find(|(program, _)| program == Path::new(tool))
        .map(|(_, arguments)| {
            arguments
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect()
        })
}

/// A program path the setting takes on this platform, unique per test.
fn tool(name: &str) -> String {
    if cfg!(windows) {
        format!("C:\\Tools\\{name}.exe")
    } else {
        format!("/opt/tools/{name}")
    }
}

/// `left/` and `right/` with `a.txt` each, differing in the second and fourth line.
async fn trees() -> (TempDir, Engine) {
    let root = test_support::tree(&[
        ("left/a.txt", "one\ntwo\nthree\nfour\n"),
        ("right/a.txt", "one\nTWO\nthree\nFOUR\n"),
    ]);
    let engine = test_support::open(&root.path().join("left"), &root.path().join("right")).await;
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    (root, engine)
}

async fn set_tool(engine: &Engine, tool: &str, args: Option<&str>) {
    engine
        .set_protected_setting(COMPARE_TOOL_KEY, &serde_json::json!(tool))
        .await
        .expect("stored");
    if let Some(args) = args {
        engine
            .set_protected_setting(COMPARE_ARGS_KEY, &serde_json::json!(args))
            .await
            .expect("stored");
    }
}

fn file(root: &TempDir, side: &str) -> VfsPath {
    VfsPath::local(root.path().join(side).join("a.txt"))
}

#[test]
fn only_a_person_picks_the_compare_tool_and_its_arguments() {
    assert!(is_protected(COMPARE_TOOL_KEY));
    assert!(is_protected(COMPARE_ARGS_KEY));
}

#[tokio::test]
async fn without_a_tool_compare_by_content_opens_the_diff_window() {
    let (root, engine) = trees().await;
    let mut events = engine.subscribe();
    let request = ByContentRequest {
        left: Some(file(&root, "left")),
        right: Some(file(&root, "right")),
    };
    engine
        .dispatch(Command::CompareByContent(request))
        .await
        .expect("opens");
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::CompareFiles {
            left: file(&root, "left"),
            right: file(&root, "right"),
        }
    );
}

#[tokio::test]
async fn with_a_tool_it_starts_with_both_files_one_argument_each() {
    let (root, engine) = trees().await;
    let program = tool("winmerge");
    set_tool(&engine, &program, Some(r#"/e /u "%1" "%2""#)).await;
    let mut events = engine.subscribe();
    let request = ByContentRequest {
        left: Some(file(&root, "left")),
        right: Some(file(&root, "right")),
    };
    engine
        .dispatch(Command::CompareByContent(request))
        .await
        .expect("started");
    let left = file(&root, "left").path.to_string_lossy().into_owned();
    let right = file(&root, "right").path.to_string_lossy().into_owned();
    assert_eq!(
        started_with(&program),
        Some(vec!["/e".into(), "/u".into(), left, right])
    );
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::CompareToolStarted { copies: false },
        "no diff window"
    );
}

/// Ctrl+Shift+F3 without paths picks the files under both cursors, as compare.files does.
#[tokio::test]
async fn the_key_picks_the_files_under_the_cursors_for_the_tool() {
    let (root, engine) = trees().await;
    let program = tool("meld");
    set_tool(&engine, &program, None).await;
    for panel in PanelId::BOTH {
        engine
            .dispatch(Command::SetCursor {
                panel: Some(panel),
                target: crate::CursorTarget::Name("a.txt".into()),
            })
            .await
            .expect("cursor");
    }
    engine
        .dispatch(Command::CompareByContent(ByContentRequest::default()))
        .await
        .expect("started");
    let both: Vec<String> = ["left", "right"]
        .iter()
        .map(|side| file(&root, side).path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        started_with(&program),
        Some(both),
        "the default \"%1\" \"%2\""
    );
}

/// A file inside an archive reaches the tool as a read-only copy under the temporary folder.
#[tokio::test]
async fn a_file_in_an_archive_goes_to_the_tool_as_a_temporary_copy() {
    let (root, engine) = crate::archives::tests::engine_beside("tree.zip").await;
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    fs::write(root.path().join("mine.txt"), "mine\n").expect("write");
    for command in [
        Command::SetCursor {
            panel: None,
            target: crate::CursorTarget::Name("tree.zip".into()),
        },
        Command::OpenFile {
            panel: None,
            path: None,
        },
    ] {
        engine.dispatch(command).await.expect("opened");
    }
    let inside = engine.panel(PanelId::Left).path.join("hello.txt");
    assert!(matches!(inside.backend, BackendId::Archive { .. }));
    let program = tool("kdiff3");
    set_tool(&engine, &program, None).await;
    let mut events = engine.subscribe();
    let request = ByContentRequest {
        left: Some(inside),
        right: Some(VfsPath::local(root.path().join("mine.txt"))),
    };
    engine
        .dispatch(Command::CompareByContent(request))
        .await
        .expect("started");
    let arguments = started_with(&program).expect("started");
    let copy = PathBuf::from(&arguments[0]);
    assert!(!copy.starts_with(root.path()), "a copy, not the archive");
    assert_eq!(fs::read(&copy).expect("read"), b"hello archive\n");
    assert!(fs::metadata(&copy).expect("stat").permissions().readonly());
    assert_eq!(arguments[1], root.path().join("mine.txt").to_string_lossy());
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::CompareToolStarted { copies: true }
    );
    if let Some(dir) = copy.parent() {
        // Best effort: Windows keeps a read-only file.
        let _ = fs::remove_dir_all(dir);
    }
}

async fn compared(engine: &Engine) -> FileComparison {
    let request = crate::compare::FileCompareRequest {
        left: Some("a.txt".into()),
        right: Some("a.txt".into()),
        ..crate::compare::FileCompareRequest::default()
    };
    match engine
        .dispatch(Command::CompareFiles(request))
        .await
        .expect("compared")
    {
        Outcome::FileComparison(comparison) => comparison,
        other => panic!("a comparison, got {other:?}"),
    }
}

fn stamps(comparison: &FileComparison) -> (FileStamp, FileStamp) {
    (
        FileStamp {
            size: comparison.left_size,
            modified_unix: comparison.left_modified_unix,
        },
        FileStamp {
            size: comparison.right_size,
            modified_unix: comparison.right_modified_unix,
        },
    )
}

#[tokio::test]
async fn apply_blocks_copies_the_blocks_named_and_saves_the_other_side() {
    let (root, engine) = trees().await;
    let before = compared(&engine).await;
    assert_eq!(before.text.as_ref().map(|text| text.changes), Some(2));
    assert!(before.left_modified_unix.is_some());
    let (expect_left, expect_right) = stamps(&before);
    let outcome = engine
        .dispatch(Command::ApplyBlocks(ApplyBlocksRequest {
            left: "a.txt".into(),
            right: "a.txt".into(),
            blocks: vec![1],
            direction: Direction::LeftToRight,
            expect_left,
            expect_right,
            ..ApplyBlocksRequest::default()
        }))
        .await
        .expect("applied");
    assert_eq!(
        fs::read_to_string(root.path().join("right").join("a.txt")).expect("read"),
        "one\nTWO\nthree\nfour\n",
        "only the second block came over"
    );
    let Outcome::FileComparison(after) = outcome else {
        panic!("a comparison");
    };
    assert_eq!(after.text.map(|text| text.changes), Some(1));
    assert_eq!(
        fs::read_to_string(root.path().join("left").join("a.txt")).expect("read"),
        "one\ntwo\nthree\nfour\n",
        "the source is untouched"
    );
}

#[tokio::test]
async fn apply_blocks_refuses_a_file_changed_since_and_an_unknown_block() {
    let (root, engine) = trees().await;
    let before = compared(&engine).await;
    let (expect_left, mut expect_right) = stamps(&before);
    let request = |expect_right, blocks| ApplyBlocksRequest {
        left: "a.txt".into(),
        right: "a.txt".into(),
        blocks,
        direction: Direction::RightToLeft,
        expect_left,
        expect_right,
        ..ApplyBlocksRequest::default()
    };
    let unknown = engine
        .dispatch(Command::ApplyBlocks(request(expect_right, vec![7])))
        .await
        .expect_err("no block 7");
    assert_eq!(unknown.code(), "compare_block_not_found");
    expect_right.size += 1;
    let changed = engine
        .dispatch(Command::ApplyBlocks(request(expect_right, vec![0])))
        .await
        .expect_err("changed");
    assert_eq!(changed.code(), "compare_changed");
    assert_eq!(
        fs::read_to_string(root.path().join("left").join("a.txt")).expect("read"),
        "one\ntwo\nthree\nfour\n",
        "nothing was written"
    );
    let modified = fs::metadata(root.path().join("right").join("a.txt"))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(unix_seconds);
    assert_eq!(modified, before.right_modified_unix);
}
