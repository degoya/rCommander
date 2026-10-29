use std::fs;

use rc_engine::CommandClass;
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::{
    DirsCompareParams, DirsSyncApplyParams, FilesCompareParams, ModeArg,
    apply::{ActionArg, RowActionArg},
};
use crate::RcMcpServer;
use crate::test_support::Fixture;
use crate::tools::class_of as escalate;

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

/// Both panels on `root`, which holds `a.txt`, `b.txt`, `server.pem` and `left/`, `right/`.
async fn server() -> (tempfile::TempDir, RcMcpServer) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::write(root.join("a.txt"), "one\ntwo\n").expect("write");
    fs::write(root.join("server.pem"), "one\ntwo\n").expect("write");
    fs::write(root.join("b.txt"), "one\nTWO\n").expect("write");
    for side in ["left", "right"] {
        fs::create_dir(root.join(side)).expect("mkdir");
        fs::write(root.join(side).join("both.txt"), "x").expect("write");
    }
    fs::write(root.join("right").join("extra.txt"), "e").expect("write");
    let built = fixture.build().await;
    let server = built.server();
    (built.root, server)
}

fn files(left: &str, right: &str) -> FilesCompareParams {
    FilesCompareParams {
        left: Some(left.into()),
        right: Some(right.into()),
        ignore_whitespace: false,
        ignore_case: false,
        ignore_line_endings: false,
        unified_limit: None,
    }
}

fn dirs(mode: Option<ModeArg>) -> DirsCompareParams {
    DirsCompareParams {
        left: Some("left".into()),
        right: Some("right".into()),
        mode,
        subdirs: None,
        by_content: false,
        ignore_date: false,
        time_tolerance_secs: None,
        ignore_dst_hour: false,
        masks: None,
        show: None,
        limit: None,
    }
}

#[tokio::test]
async fn files_compare_reports_the_first_difference_and_a_unified_diff() {
    let (_root, server) = server().await;
    let answer = server
        .files_compare(Parameters(FilesCompareParams {
            unified_limit: Some(1000),
            ..files("a.txt", "b.txt")
        }))
        .await
        .expect("answer");
    assert_ne!(answer.is_error, Some(true));
    let answer = body(&answer);
    assert_eq!(answer["kind"], "file_comparison");
    assert_eq!(answer["identical"], false);
    assert_eq!(answer["first_difference"]["offset"], 4);
    assert_eq!(answer["text"]["changes"], 1);
    assert_eq!(
        answer["text"]["unified"]["text"],
        "@@ -1,2 +1,2 @@\n one\n-two\n+TWO\n"
    );

    let ignoring = server
        .files_compare(Parameters(FilesCompareParams {
            ignore_case: true,
            ..files("a.txt", "b.txt")
        }))
        .await
        .expect("answer");
    let ignoring = body(&ignoring);
    assert_eq!(ignoring["text"]["equal"], true);
    assert_eq!(ignoring["text"]["unified"], Value::Null);
}

#[tokio::test]
async fn files_compare_refuses_a_directory_with_its_code() {
    let (_root, server) = server().await;
    let answer = server
        .files_compare(Parameters(files("left", "a.txt")))
        .await
        .expect("answer");
    assert_eq!(answer.is_error, Some(true));
    assert_eq!(body(&answer)["code"], "is_a_directory");
}

#[tokio::test]
async fn dirs_compare_lists_differences_with_the_proposed_actions() {
    let (_root, server) = server().await;
    let both = body(
        &server
            .dirs_compare(Parameters(dirs(None)))
            .await
            .expect("answer"),
    );
    assert_eq!(both["summary"]["equal"], 1);
    let rows = both["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "{both}");
    assert_eq!(rows[0]["path"], "extra.txt");
    assert_eq!(rows[0]["state"], "right_only");
    assert_eq!(rows[0]["action"], "copy_to_left");
    assert!(rows[0].get("left").is_none());
    assert_eq!(rows[0]["right"]["kind"], "file");

    let mirror = body(
        &server
            .dirs_compare(Parameters(dirs(Some(ModeArg::MirrorToRight))))
            .await
            .expect("answer"),
    );
    assert_eq!(mirror["rows"][0]["action"], "delete_right");
    assert_eq!(mirror["summary"]["delete_right"], 1);
}

/// Whether two files are equal tells about each one's contents: a key file is refused, whether
/// named or picked from the panel's selection.
#[tokio::test]
async fn files_compare_refuses_a_sensitive_file_given_or_selected() {
    let (_root, server) = server().await;
    let given = server
        .files_compare(Parameters(files("a.txt", "server.pem")))
        .await
        .expect("answer");
    assert_eq!(given.is_error, Some(true));
    assert_eq!(body(&given)["code"], "sensitive_path");
    server
        .context
        .engine
        .dispatch(rc_engine::Command::ToggleSelection {
            panel: None,
            names: vec!["server.pem".into(), "a.txt".into()],
        })
        .await
        .expect("selects");
    let selected = server
        .files_compare(Parameters(FilesCompareParams {
            left: None,
            right: None,
            ..files("", "")
        }))
        .await
        .expect("answer");
    assert_eq!(
        body(&selected)["code"],
        "sensitive_path",
        "{:?}",
        body(&selected)
    );
}

fn apply(mode: Option<ModeArg>) -> DirsSyncApplyParams {
    DirsSyncApplyParams {
        left: Some("left".into()),
        right: Some("right".into()),
        mode,
        subdirs: None,
        by_content: false,
        ignore_date: false,
        time_tolerance_secs: None,
        ignore_dst_hour: false,
        masks: None,
        proposed: None,
        actions: Vec::new(),
        allow_deletes: false,
        permanent: false,
        overwrite: false,
        on_error: Default::default(),
        verify: false,
    }
}

#[tokio::test]
async fn dirs_sync_apply_queues_the_proposed_copies() {
    let (root, server) = server().await;
    let answer = body(
        &server
            .dirs_sync_apply(Parameters(apply(None)))
            .await
            .expect("answer"),
    );
    let jobs = answer["jobs"].as_array().expect("jobs");
    assert_eq!(jobs.len(), 1, "{answer}");
    let id = jobs[0]["id"].as_u64().expect("an id");
    let done = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        server.context.engine.ops().wait(id),
    )
    .await
    .expect("in time")
    .expect("known");
    assert_eq!(done.state, rc_engine::JobState::Completed);
    assert!(root.path().join("left").join("extra.txt").exists());
}

#[tokio::test]
async fn dirs_sync_apply_deletes_nothing_unless_allowed() {
    let (root, server) = server().await;
    let refused = server
        .dirs_sync_apply(Parameters(apply(Some(ModeArg::MirrorToRight))))
        .await
        .expect("answer");
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(body(&refused)["code"], "sync_deletes_unconfirmed");
    let unknown = server
        .dirs_sync_apply(Parameters(DirsSyncApplyParams {
            actions: vec![RowActionArg {
                path: "nowhere.txt".into(),
                action: ActionArg::CopyToLeft,
            }],
            ..apply(None)
        }))
        .await
        .expect("answer");
    assert_eq!(body(&unknown)["code"], "sync_row_not_found");
    assert!(root.path().join("right").join("extra.txt").exists());
    assert!(server.context.engine.ops().jobs().is_empty());
}

#[test]
fn a_sync_that_may_delete_or_overwrite_is_escalated() {
    let args = |value: Value| value.as_object().cloned().expect("object");
    let write = CommandClass::Write;
    for destructive in [
        serde_json::json!({ "allow_deletes": true }),
        serde_json::json!({ "overwrite": true }),
    ] {
        assert_eq!(
            escalate("dirs_sync_apply", Some(&args(destructive)), write),
            CommandClass::Destructive
        );
    }
    let plain = args(serde_json::json!({ "allow_deletes": false, "mode": "mirror_to_right" }));
    assert_eq!(escalate("dirs_sync_apply", Some(&plain), write), write);
    assert_eq!(escalate("dirs_sync_apply", None, write), write);
    let other = args(serde_json::json!({ "overwrite": true }));
    assert_eq!(escalate("dirs_compare", Some(&other), write), write);
}

/// A client's cancel reaches the engine (RC-080-50): a call whose token is cancelled ends with
/// `cancelled`, and the same call without one compares.
#[tokio::test]
async fn a_cancelled_call_ends_with_cancelled() {
    let (_root, server) = server().await;
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let compared = crate::tools::with_call_cancel(
        cancel,
        server.files_compare(Parameters(files("a.txt", "b.txt"))),
    )
    .await
    .expect("answers");
    assert_eq!(
        body(&compared)["code"],
        "cancelled",
        "{:?}",
        body(&compared)
    );

    let compared = server
        .files_compare(Parameters(files("a.txt", "b.txt")))
        .await
        .expect("answers");
    assert!(
        body(&compared).get("code").is_none(),
        "{:?}",
        body(&compared)
    );
}
