//! Rename jobs: a multi-rename's schedule carried out against an in-memory backend - swaps,
//! rings and case-only changes without losing a file, the journal, a cancel undone, and a name
//! that appeared since the plan.

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rc_ops::{ErrorPolicy, JobChange, JobKind, JobOptions, JobSpec, JobState, StepJournal};
use rc_rename::{
    FsRules, PlanInput, RenameRule, RenameSource, RenameStep, SearchReplace, plan, schedule,
    undo_steps,
};
use rc_vfs::MemoryFs;
use support::{Harness, exists, mem, read, write};

/// The steps that rename `names` in `/d` by replacing the `|` terms of `find` with those of
/// `replace` (with the extension when `whole`).
fn steps_for(
    names: &[&str],
    find: &str,
    replace: &str,
    whole: bool,
    fs: FsRules,
) -> Vec<RenameStep> {
    let sources: Vec<RenameSource> = names
        .iter()
        .map(|name| RenameSource {
            name: (*name).to_owned(),
            modified: None,
        })
        .collect();
    let existing: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    let rule = RenameRule {
        search: Some(SearchReplace {
            find: find.to_owned(),
            replace: replace.to_owned(),
            case_sensitive: true,
            include_extension: whole,
            ..SearchReplace::default()
        }),
        ..RenameRule::default()
    };
    let input = PlanInput {
        dir: "/d",
        sources: &sources,
        existing: &existing,
        fs,
    };
    let plan = plan(&input, &rule).expect("the rule plans");
    assert!(!plan.has_conflicts(), "{plan:?}");
    schedule(&plan, "t").expect("schedules").steps
}

fn spec(steps: Vec<RenameStep>) -> JobSpec {
    JobSpec {
        kind: JobKind::rename(mem(1, "/d"), steps),
        options: JobOptions {
            on_error: ErrorPolicy::Cancel,
            ..JobOptions::default()
        },
    }
}

/// `/d/<name>` holding its own name.
async fn files(fs: &MemoryFs, names: &[&str]) {
    for name in names {
        write(fs, &format!("/d/{name}"), name.as_bytes()).await;
    }
}

async fn content(fs: &MemoryFs, name: &str) -> String {
    String::from_utf8(read(fs, &format!("/d/{name}")).await).expect("utf-8")
}

/// A journal that records the steps it is told of, and fails at `fail_at`.
#[derive(Default)]
struct Recorder {
    done: Mutex<Vec<usize>>,
    fail_at: Option<usize>,
}

impl Recorder {
    fn recorded(&self) -> Vec<usize> {
        self.done.lock().expect("done").clone()
    }
}

#[async_trait]
impl StepJournal for Recorder {
    async fn done(&self, index: usize) -> Result<(), String> {
        if self.fail_at == Some(index) {
            return Err("the disk is gone".to_owned());
        }
        self.done.lock().expect("done").push(index);
        Ok(())
    }
}

#[tokio::test]
async fn a_swap_and_a_ring_lose_no_file() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    files(&fs, &["a.txt", "b.txt", "x.txt", "y.txt", "z.txt"]).await;

    let swap = steps_for(&["a.txt", "b.txt"], "a|b", "b|a", false, FsRules::UNIX);
    let done = harness.run(spec(swap)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        done.progress.files_done, 3,
        "two renames and a temporary name"
    );
    assert_eq!(content(&fs, "a.txt").await, "b.txt");
    assert_eq!(content(&fs, "b.txt").await, "a.txt");

    let ring = steps_for(
        &["x.txt", "y.txt", "z.txt"],
        "x|y|z",
        "y|z|x",
        false,
        FsRules::UNIX,
    );
    let done = harness.run(spec(ring)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(&fs, "y.txt").await, "x.txt");
    assert_eq!(content(&fs, "z.txt").await, "y.txt");
    assert_eq!(content(&fs, "x.txt").await, "z.txt");
    assert!(
        !exists(&fs, "/d/.rc-rename-t-1").await,
        "no temporary name is left"
    );
}

#[tokio::test]
async fn a_case_only_change_goes_through_a_temporary_name() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    files(&fs, &["readme.TXT"]).await;
    let steps = steps_for(
        &["readme.TXT"],
        "readme.TXT",
        "README.txt",
        true,
        FsRules::WINDOWS,
    );
    assert_eq!(steps.len(), 2, "{steps:?}");
    let done = harness.run(spec(steps)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(&fs, "README.txt").await, "readme.TXT");
    assert!(!exists(&fs, "/d/readme.TXT").await);
}

#[tokio::test]
async fn the_journal_hears_every_step_in_order() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    files(&fs, &["a.txt", "b.txt"]).await;
    let recorder = Arc::new(Recorder::default());
    let swap = steps_for(&["a.txt", "b.txt"], "a|b", "b|a", false, FsRules::UNIX);
    let journal: Arc<dyn StepJournal> = recorder.clone();
    let id = harness
        .queue
        .submit_journaled(spec(swap), Some(journal))
        .expect("accepted")
        .id;
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(recorder.recorded(), vec![0, 1, 2]);
    let snapshot = serde_json::to_value(&done.kind).expect("serializes");
    assert_eq!(snapshot["op"], "rename");
    assert_eq!(snapshot["count"], 3);
    assert!(snapshot.get("steps").is_none(), "{snapshot}");
}

#[tokio::test]
async fn a_cancel_half_way_is_undone_completely() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    let names = ["a.txt", "b.txt", "c.txt", "d.txt"];
    files(&fs, &names).await;
    let ring = steps_for(&names, "a|b|c|d", "b|c|d|a", false, FsRules::UNIX);
    assert_eq!(ring.len(), 5);
    harness.on_event(|queue, event| {
        if let JobChange::Progress { progress } = &event.change
            && progress.files_done == 2
        {
            let _ = queue.cancel(event.job);
        }
    });
    let recorder = Arc::new(Recorder::default());
    let journal: Arc<dyn StepJournal> = recorder.clone();
    let id = harness
        .queue
        .submit_journaled(spec(ring.clone()), Some(journal))
        .expect("accepted")
        .id;
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    let recorded = recorder.recorded();
    assert_eq!(recorded, vec![0, 1], "every step done is recorded");

    harness.on_event(|_, _| {});
    let undone = harness.run(spec(undo_steps(&ring[..recorded.len()]))).await;
    assert_eq!(undone.state, JobState::Completed, "{undone:?}");
    for name in names {
        assert_eq!(content(&fs, name).await, name, "{name} is back");
    }
}

#[tokio::test]
async fn a_name_taken_since_the_plan_is_never_overwritten() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    files(&fs, &["a.txt"]).await;
    let steps = steps_for(&["a.txt"], "a", "b", false, FsRules::UNIX);
    // Someone writes b.txt between the preview and the run.
    write(&fs, "/d/b.txt", b"theirs").await;
    let done = harness.run(spec(steps)).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "already_exists");
    assert_eq!(content(&fs, "b.txt").await, "theirs");
    assert_eq!(content(&fs, "a.txt").await, "a.txt");
}

#[tokio::test]
async fn a_journal_that_fails_stops_the_run_before_the_next_step() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    files(&fs, &["a.txt", "b.txt"]).await;
    let swap = steps_for(&["a.txt", "b.txt"], "a|b", "b|a", false, FsRules::UNIX);
    let recorder = Arc::new(Recorder {
        fail_at: Some(0),
        ..Recorder::default()
    });
    let journal: Arc<dyn StepJournal> = recorder.clone();
    let id = harness
        .queue
        .submit_journaled(spec(swap), Some(journal))
        .expect("accepted")
        .id;
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "journal_failed");
    assert_eq!(done.progress.files_done, 0);
    assert_eq!(
        content(&fs, "b.txt").await,
        "b.txt",
        "the second step never ran"
    );
}

#[tokio::test]
async fn a_nested_name_moves_into_a_subfolder_made_for_it_and_back() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    files(&fs, &["a.txt", "b.txt"]).await;
    let step = |from: &str, to: &str| RenameStep {
        from: from.to_owned(),
        to: to.to_owned(),
    };
    let steps = vec![step("a.txt", "sub/a.txt"), step("b.txt", "sub/deep/b.txt")];
    let done = harness.run(spec(steps.clone())).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(&fs, "sub/a.txt").await, "a.txt");
    assert_eq!(content(&fs, "sub/deep/b.txt").await, "b.txt");
    assert!(!exists(&fs, "/d/a.txt").await);

    let done = harness.run(spec(undo_steps(&steps))).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(&fs, "a.txt").await, "a.txt");
    assert_eq!(content(&fs, "b.txt").await, "b.txt");
    assert!(exists(&fs, "/d/sub/deep").await, "made folders stay");
}

/// RC-080-52: steps are held below their directory when the job is submitted, whoever made
/// them - a `..`, a root or an empty part is refused before anything is renamed.
#[tokio::test]
async fn steps_that_leave_the_directory_are_refused_at_submit() {
    let harness = Harness::start();
    let fs = harness.memory(1);
    files(&fs, &["a.txt"]).await;
    for (from, to) in [
        ("a.txt", "../x.txt"),
        ("a.txt", "sub/../../x.txt"),
        ("a.txt", "/x.txt"),
        ("a.txt", "sub//x.txt"),
        ("../a.txt", "b.txt"),
        ("a.txt", ""),
    ] {
        let step = RenameStep {
            from: from.to_owned(),
            to: to.to_owned(),
        };
        let refused = harness.queue.submit(spec(vec![step])).expect_err(to);
        assert_eq!(refused.code(), "invalid_name", "{from} -> {to}");
    }
    let nested = RenameStep {
        from: "a.txt".to_owned(),
        to: "sub/b.txt".to_owned(),
    };
    let done = harness.run(spec(vec![nested])).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(exists(&fs, "/d/sub/b.txt").await);
}
