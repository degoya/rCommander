//! The undo chain and the next step (RC-100-96).

use std::{fs, time::Duration};

use rc_db::LoggedStep;
use rc_ops::JobState;
use rc_rename::{RenameRule, SearchReplace};

use super::after_run;
use crate::commands::MultiRenameRequest;
use crate::{Command, Engine, Outcome, test_support};

fn step(from: &str, to: &str, done: bool) -> LoggedStep {
    LoggedStep {
        from: from.to_owned(),
        to: to.to_owned(),
        done,
    }
}

fn owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn names_follow_the_steps_done_through_temporary_names() {
    // A swap of a and b through a temporary name, and c renamed to d.
    let steps = [
        step("a", ".rc-rename-1-0", true),
        step("b", "a", true),
        step(".rc-rename-1-0", "b", true),
        step("c", "d", true),
    ];
    assert_eq!(
        after_run(owned(&["a", "b", "c", "x"]), &steps),
        ["b", "a", "d", "x"]
    );
    // Steps not done change nothing; an entry moved into a subfolder has left.
    let steps = [step("a", "sub/a", true), step("b", "c", false)];
    assert_eq!(after_run(owned(&["a", "b"]), &steps), ["b"]);
}

/// `find` replaced by `replace` in `names`.
fn replacing(names: &[String], find: &str, replace: &str) -> MultiRenameRequest {
    MultiRenameRequest {
        names: names.to_vec(),
        rule: RenameRule {
            search: Some(SearchReplace {
                find: find.to_owned(),
                replace: replace.to_owned(),
                case_sensitive: true,
                ..SearchReplace::default()
            }),
            ..RenameRule::default()
        },
        ..MultiRenameRequest::default()
    }
}

async fn run(engine: &Engine, command: Command) -> JobState {
    let outcome = engine.dispatch(command).await.expect("queued");
    let Outcome::RenameRun { job, .. } = outcome else {
        panic!("a rename run, got {outcome:?}");
    };
    tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
        .await
        .expect("in time")
        .expect("known")
        .state
}

async fn next_step(engine: &Engine, names: &[String]) -> Vec<String> {
    let outcome = engine
        .dispatch(Command::MultiRenameNextStep {
            names: names.to_vec(),
        })
        .await
        .expect("answers");
    let Outcome::RenameNextStep { names } = outcome else {
        panic!("the next step, got {outcome:?}");
    };
    names
}

#[tokio::test]
async fn next_step_chains_two_rules() {
    let (root, engine) =
        test_support::engine_with_db(&[("img1.jpg", "1"), ("img2.jpg", "2")]).await;
    let first = owned(&["img2.jpg", "img1.jpg"]);
    // Without a run the names are as given.
    assert_eq!(next_step(&engine, &first).await, first);
    let step_one = Command::MultiRenameApply(replacing(&first, "img", "photo"));
    assert_eq!(run(&engine, step_one).await, JobState::Completed);
    let second = next_step(&engine, &first).await;
    assert_eq!(second, ["photo2.jpg", "photo1.jpg"]);
    let step_two = Command::MultiRenameApply(replacing(&second, "photo", "holiday_"));
    assert_eq!(run(&engine, step_two).await, JobState::Completed);
    let read = |name: &str| fs::read_to_string(root.path().join(name)).ok();
    assert_eq!(read("holiday_1.jpg").as_deref(), Some("1"));
    assert_eq!(read("holiday_2.jpg").as_deref(), Some("2"));

    // Each step is undone on its own, the latest first.
    assert_eq!(
        run(&engine, Command::MultiRenameUndo).await,
        JobState::Completed
    );
    assert_eq!(read("photo1.jpg").as_deref(), Some("1"));
    assert_eq!(
        run(&engine, Command::MultiRenameUndo).await,
        JobState::Completed
    );
    assert_eq!(read("img1.jpg").as_deref(), Some("1"));
    assert_eq!(read("img2.jpg").as_deref(), Some("2"));
    let nothing = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect_err("both undone");
    assert_eq!(nothing.code(), "rename_nothing_to_undo");
}
