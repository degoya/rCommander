//! The `queue` tools end to end: an rmcp client over an in-process pipe (RC-010-14).

use std::{fs, time::Duration};

use rc_engine::{Answer, Command, ConflictChoice, Engine, JobId, JobOptions, Outcome};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

/// A folder name the mask hides: `token=<value>` is a credential's shape.
const PLANTED: &str = "canary4711";

/// `root/` with `a.txt`, `docs/a.txt` and `token=canary4711/c.txt`; both panels on `root`.
async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("docs")).expect("mkdir");
    fs::write(root.join("a.txt"), b"new").expect("write");
    fs::write(root.join("docs").join("a.txt"), b"old").expect("write");
    let planted = root.join(format!("token={PLANTED}"));
    fs::create_dir(&planted).expect("mkdir");
    fs::write(planted.join("c.txt"), b"c").expect("write");
    let built = fixture.build().await;
    (built.root, built.context)
}

/// A copy into `docs` as the window starts it: a conflict asks.
async fn copy(engine: &Engine, source: &str) -> JobId {
    let outcome = engine
        .dispatch(Command::OpsCopy(rc_engine::TransferRequest {
            panel: None,
            sources: vec![source.into()],
            target: Some("docs".into()),
            options: JobOptions::default(),
        }))
        .await
        .expect("queued");
    match outcome {
        Outcome::Job(job) => job.id,
        other => panic!("a job, got {other:?}"),
    }
}

fn ids(answer: &Value) -> Vec<u64> {
    answer["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("entries: {answer}"))
        .iter()
        .filter_map(|entry| entry["job"]["id"].as_u64())
        .collect()
}

/// The acceptance check: the tools list and move jobs, and no answer carries a secret.
#[tokio::test]
async fn queue_tools_list_and_move_jobs_with_paths_masked() {
    let (_root, context) = setup().await;
    let engine = context.engine().clone();
    let first = copy(&engine, "a.txt").await;
    for _ in 0..500 {
        if engine.ops().job(first).expect("known").question.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let second = copy(&engine, &format!("token={PLANTED}/c.txt")).await;
    let third = copy(&engine, &format!("token={PLANTED}")).await;
    let client = connect(&context).await;

    let listed = call(&client, "queue_list", json!({})).await;
    assert_eq!(ids(&listed), [first, second, third], "{listed}");
    assert_eq!(listed["entries"][1]["place"], 0, "{listed}");
    assert_eq!(listed["entries"][1]["lane"], "local", "{listed}");
    let rendered = listed.to_string();
    assert!(!rendered.contains(PLANTED), "{rendered}");
    assert!(rendered.contains("[redacted]"), "{rendered}");

    let moved = call(
        &client,
        "queue_move",
        json!({ "job": third, "step": "front" }),
    )
    .await;
    assert_eq!(ids(&moved), [first, third, second], "{moved}");
    assert!(!moved.to_string().contains(PLANTED));

    let refused = call(&client, "queue_move", json!({ "job": first, "step": "up" })).await;
    assert_eq!(refused["code"], "job_not_waiting", "{refused}");

    let removed = call(&client, "queue_remove", json!({ "job": second })).await;
    assert_eq!(ids(&removed), [first, third], "{removed}");
    let paused = call(&client, "queue_pause_all", json!({})).await;
    // The first waits on its question, which a pause does not hide.
    assert_eq!(paused["entries"][1]["job"]["state"], "paused", "{paused}");
    call(&client, "queue_resume_all", json!({})).await;
    engine
        .dispatch(Command::OpsAnswer {
            job: first,
            answer: Answer::Conflict {
                choice: ConflictChoice::Skip,
                for_all: false,
            },
        })
        .await
        .expect("answered");
    let wait = engine.ops().wait(third);
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("in time")
        .expect("known");
    let cleared = call(&client, "queue_clear_finished", json!({})).await;
    assert_eq!(ids(&cleared), Vec::<u64>::new(), "{cleared}");
}

/// The list tools (RC-010-15): a list made, a job put into it, saved and loaded again - the
/// loaded list waits until it is started; deleting a saved list is destructive, loading a write.
#[tokio::test]
async fn queue_list_tools_make_fill_save_and_load_a_list() {
    let (_root, context) = setup().await;
    let engine = context.engine().clone();
    let client = connect(&context).await;
    let created = call(&client, "queue_create", json!({ "name": "Backup" })).await;
    let list = created["lists"][1]["id"].as_u64().expect("the new list");
    assert_eq!(created["lists"][1]["started"], false, "{created}");

    let job = copy(&engine, "a.txt").await;
    let assigned = call(&client, "queue_assign", json!({ "job": job, "list": list })).await;
    let entry = assigned["entries"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["job"]["id"] == job))
        .cloned()
        .unwrap_or_default();
    assert_eq!(entry["list"], list, "{assigned}");

    let saved = call(&client, "queue_save", json!({ "list": list })).await;
    assert_eq!(saved["saved"]["name"], "Backup", "{saved}");
    assert_eq!(saved["saved"]["entries"], 1, "{saved}");
    let id = saved["saved"]["id"].as_i64().expect("the saved id");
    let loaded = call(&client, "queue_load", json!({ "saved": id })).await;
    assert_eq!(loaded["list"]["started"], false, "{loaded}");
    assert_eq!(loaded["jobs"][0]["state"], "queued", "{loaded}");
    assert_eq!(loaded["unloadable"], json!([]), "{loaded}");

    let refused = call(&client, "queue_delete", json!({ "list": 0 })).await;
    assert_eq!(refused["code"], "queue_list_fixed", "{refused}");
    let lists = call(&client, "queue_lists", json!({})).await;
    assert_eq!(lists["saved"][0]["id"], id, "{lists}");
    for job in engine.ops().jobs() {
        let _ = engine.ops().cancel(job.id);
    }

    let class = |tool: &str| {
        crate::coverage::command_of(tool)
            .and_then(rc_engine::spec)
            .map(|spec| spec.class)
    };
    assert_eq!(
        class("queue_delete_saved"),
        Some(rc_engine::CommandClass::Destructive)
    );
    assert_eq!(class("queue_load"), Some(rc_engine::CommandClass::Write));
}

/// Several jobs at once (RC-010-15): moved, put into a new list and back, taken off - the asking
/// one cancelled -, every list cleared; taking jobs off is a write, as ops_cancel is.
#[tokio::test]
async fn queue_tools_steer_several_jobs_at_once() {
    let (_root, context) = setup().await;
    let engine = context.engine().clone();
    let first = copy(&engine, "a.txt").await;
    for _ in 0..500 {
        if engine.ops().job(first).expect("known").question.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let second = copy(&engine, &format!("token={PLANTED}/c.txt")).await;
    let third = copy(&engine, "a.txt").await;
    let client = connect(&context).await;

    let moved = call(
        &client,
        "queue_move_many",
        json!({ "jobs": [first, third], "step": "front" }),
    )
    .await;
    assert_eq!(ids(&moved), [first, third, second], "{moved}");
    let created = call(
        &client,
        "queue_create",
        json!({ "name": "Later", "jobs": [second] }),
    )
    .await;
    assert_eq!(created["lists"][1]["jobs"], 1, "{created}");
    let back = call(
        &client,
        "queue_assign_many",
        json!({ "jobs": [second], "list": 0 }),
    )
    .await;
    assert_eq!(back["entries"][2]["list"], 0, "{back}");
    let missing = call(&client, "queue_discard", json!({ "jobs": [second, 999] })).await;
    assert_eq!(missing["code"], "job_not_found", "{missing}");

    let left = call(&client, "queue_discard", json!({ "jobs": [first, second] })).await;
    assert_eq!(ids(&left), [third], "{left}");
    let cleared = call(&client, "queue_clear", json!({ "all_lists": true })).await;
    assert_eq!(ids(&cleared), Vec::<u64>::new(), "{cleared}");

    let class = |tool: &str| {
        crate::coverage::command_of(tool)
            .and_then(rc_engine::spec)
            .map(|spec| spec.class)
    };
    for tool in [
        "queue_move_many",
        "queue_assign_many",
        "queue_discard",
        "queue_clear",
    ] {
        assert_eq!(class(tool), class("ops_cancel"), "{tool}");
    }
}

/// A copy confirmed with OK runs at once, outside the lists (RC-010-25): queue_list names it,
/// marked at_once, ahead of the queued jobs, and the tools that move a job refuse it by its code.
#[tokio::test]
async fn a_job_run_at_once_is_listed_marked_and_never_moved() {
    let (_root, context) = setup().await;
    let engine = context.engine().clone();
    let queued = copy(&engine, "a.txt").await;
    let outcome = engine
        .dispatch(Command::OpsCopyTo {
            panel: None,
            sources: vec!["a.txt".into()],
            target: "docs".into(),
            options: JobOptions::default(),
            queue: None,
        })
        .await
        .expect("run at once");
    let now = match outcome {
        Outcome::Job(job) => job.id,
        Outcome::Jobs { jobs } => jobs.first().expect("a job").id,
        other => panic!("a job, got {other:?}"),
    };
    let client = connect(&context).await;

    let listed = call(&client, "queue_list", json!({})).await;
    assert_eq!(ids(&listed), [now, queued], "{listed}");
    assert_eq!(listed["entries"][0]["at_once"], true, "{listed}");
    assert_eq!(listed["entries"][1]["at_once"], false, "{listed}");

    let moved = call(&client, "queue_move", json!({ "job": now, "step": "up" })).await;
    let assigned = call(&client, "queue_assign", json!({ "job": now, "list": 0 })).await;
    for refused in [moved, assigned] {
        let code = refused["code"].as_str().unwrap_or_default();
        // It may have finished meanwhile; either way it is not moved.
        assert!(matches!(code, "job_at_once" | "job_finished"), "{refused}");
    }
    let saved = call(&client, "queue_save", json!({ "list": rc_engine::AT_ONCE })).await;
    assert_eq!(saved["code"], "queue_list_not_found", "{saved}");
}
