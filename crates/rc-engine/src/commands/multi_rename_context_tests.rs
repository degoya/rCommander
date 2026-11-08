//! What the request brings besides the rule (RC-100-96): the dialog's moments, the clipboard,
//! the local formats, and the sizes, folders and name lists the plan reads.

use std::fs;

use chrono::{NaiveDate, NaiveDateTime};
use rc_rename::{Change, Conflict, RenameContext, RenamePlan, RenameRule};

use super::{MultiRenameRequest, with_moments};
use crate::{Command, Engine, Outcome, test_support};

fn moment(hour: u32, minute: u32) -> Option<NaiveDateTime> {
    NaiveDate::from_ymd_opt(2026, 11, 7).and_then(|date| date.and_hms_opt(hour, minute, 0))
}

fn masked(names: &[&str], name_mask: &str, context: RenameContext) -> MultiRenameRequest {
    MultiRenameRequest {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        rule: RenameRule {
            name_mask: name_mask.to_owned(),
            ..RenameRule::default()
        },
        context,
        ..MultiRenameRequest::default()
    }
}

async fn preview(engine: &Engine, request: MultiRenameRequest) -> RenamePlan {
    let outcome = engine
        .dispatch(Command::MultiRenamePreview(request))
        .await
        .expect("previews");
    let Outcome::RenamePreview { plan, .. } = outcome else {
        panic!("a preview, got {outcome:?}");
    };
    plan
}

fn news(plan: &RenamePlan) -> Vec<&str> {
    plan.items.iter().map(|item| item.new.as_str()).collect()
}

#[tokio::test]
async fn t2_is_the_dialog_open_time() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    let context = RenameContext {
        opened_at: moment(8, 15),
        changed_at: moment(9, 30),
        ..RenameContext::default()
    };
    let plan = preview(&engine, masked(&["a.txt"], "[N]_[T2][hm]", context)).await;
    assert_eq!(news(&plan), ["a_0815.txt"]);
}

#[tokio::test]
async fn t3_is_the_last_option_change() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    let context = RenameContext {
        opened_at: moment(8, 15),
        changed_at: moment(9, 30),
        ..RenameContext::default()
    };
    let plan = preview(&engine, masked(&["a.txt"], "[N]_[T3][hm]", context)).await;
    assert_eq!(news(&plan), ["a_0930.txt"]);
}

#[test]
fn an_agent_without_moments_gets_the_time_of_the_call() {
    let given = RenameContext {
        opened_at: moment(8, 15),
        ..RenameContext::default()
    };
    let now = std::time::SystemTime::now();
    let filled = with_moments(&given, now);
    assert_eq!(filled.opened_at, moment(8, 15));
    assert_eq!(filled.changed_at, Some(rc_rename::local_time(now)));
}

#[tokio::test]
async fn size_folders_clipboard_and_local_formats_reach_the_plan() {
    let (root, engine) = test_support::engine_with_db(&[("a.txt", "12345")]).await;
    fs::create_dir(root.path().join("release.v2")).expect("mkdir");
    let context = RenameContext {
        clipboard: Some("Trip: day 1\r\n".to_owned()),
        date_format: Some("dd.MM.yyyy".to_owned()),
        ..RenameContext::default()
    };
    let plan = preview(
        &engine,
        masked(&["a.txt", "release.v2"], "[I][N]_[S]_[X]", context),
    )
    .await;
    assert_eq!(
        news(&plan),
        ["a_5_Trip_ day 1.txt", "release.v2_0_Trip_ day 1"]
    );
    let empty = engine
        .dispatch(Command::MultiRenamePreview(masked(
            &["a.txt"],
            "[X]",
            RenameContext::default(),
        )))
        .await
        .expect_err("[X] without a clipboard");
    assert_eq!(empty.code(), "rename_clipboard_empty");
}

#[tokio::test]
async fn names_from_a_file_rename_in_order() {
    let (root, engine) =
        test_support::engine_with_db(&[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")]).await;
    let names =
        rc_rename::names_from_bytes(b"\xEF\xBB\xBFthird.txt\r\nfirst.txt\r\n").expect("reads");
    let request = MultiRenameRequest {
        names: vec!["c.txt".to_owned(), "a.txt".to_owned()],
        rule: RenameRule {
            names: Some(names),
            ..RenameRule::default()
        },
        ..MultiRenameRequest::default()
    };
    let outcome = engine
        .dispatch(Command::MultiRenameApply(request))
        .await
        .expect("queued");
    let Outcome::RenameRun { job, .. } = outcome else {
        panic!("a rename run, got {outcome:?}");
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        engine.ops().wait(job.id),
    )
    .await
    .expect("in time")
    .expect("known");
    let read = |name: &str| fs::read_to_string(root.path().join(name)).ok();
    assert_eq!(read("third.txt").as_deref(), Some("c"));
    assert_eq!(read("first.txt").as_deref(), Some("a"));
    assert_eq!(read("b.txt").as_deref(), Some("b"));
    // The rule kept as "last used" carries no names.
    let Outcome::RenamePresets { last, .. } = engine
        .dispatch(Command::MultiRenamePresetList)
        .await
        .expect("lists")
    else {
        panic!("the presets");
    };
    assert_eq!(last.map(|rule| rule.names), Some(None));
}

#[tokio::test]
async fn too_few_names_is_a_conflict() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "a"), ("b.txt", "b")]).await;
    let request = MultiRenameRequest {
        names: vec!["a.txt".to_owned(), "b.txt".to_owned()],
        rule: RenameRule {
            names: Some(vec!["x.txt".to_owned()]),
            ..RenameRule::default()
        },
        ..MultiRenameRequest::default()
    };
    let plan = preview(&engine, request.clone()).await;
    assert_eq!(plan.items[0].change, Change::Rename);
    assert_eq!(plan.items[1].conflict, Some(Conflict::NamesMissing));
    let refused = engine
        .dispatch(Command::MultiRenameApply(request))
        .await
        .expect_err("conflicts");
    assert_eq!(refused.code(), "rename_plan_conflicts");
}
