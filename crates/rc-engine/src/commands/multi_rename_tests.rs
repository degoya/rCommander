use std::{fs, path::Path, time::Duration};

use rc_db::Db;
use rc_ops::{JobSnapshot, JobState};
use rc_rename::{Change, Conflict, RenameRule, SearchReplace};
use rc_vfs::VfsPath;

use super::MultiRenameRequest;
use crate::test_support;
use crate::{Command, Engine, Outcome};

/// A folder holding each name with its own name as content, and an engine on it with a database.
async fn engine(names: &[&str]) -> (tempfile::TempDir, Engine) {
    let entries: Vec<(&str, &str)> = names.iter().map(|name| (*name, *name)).collect();
    test_support::engine_with_db(&entries).await
}

fn content(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join(name)).expect("read")
}

/// `find` replaced by `replace`, `|` terms each by its own, in the names without extension.
fn replacing(names: &[&str], find: &str, replace: &str) -> MultiRenameRequest {
    MultiRenameRequest {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
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

async fn finished(engine: &Engine, outcome: Outcome) -> JobSnapshot {
    let Outcome::RenameRun { job, .. } = outcome else {
        panic!("a rename run, got {outcome:?}");
    };
    tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
        .await
        .expect("in time")
        .expect("known")
}

#[tokio::test]
async fn the_preview_names_changes_and_conflicts_and_renames_nothing() {
    let (root, engine) = engine(&["a.txt", "b.txt", "c.txt", "keep.txt"]).await;
    let outcome = engine
        .dispatch(Command::MultiRenamePreview(replacing(
            &["a.txt", "b.txt", "c.txt"],
            "a|b|c",
            "b|a|keep",
        )))
        .await
        .expect("previews");
    let Outcome::RenamePreview { plan, .. } = outcome else {
        panic!("a preview, got {outcome:?}");
    };
    let news: Vec<&str> = plan.items.iter().map(|item| item.new.as_str()).collect();
    assert_eq!(news, ["b.txt", "a.txt", "keep.txt"]);
    assert_eq!(plan.items[0].change, Change::Rename);
    assert_eq!(plan.items[0].conflict, None, "a swap is no conflict");
    assert_eq!(plan.items[2].conflict, Some(Conflict::TargetExists));
    assert_eq!(content(root.path(), "a.txt"), "a.txt");
}

#[tokio::test]
async fn without_names_the_selection_is_renamed_in_panel_order() {
    let (_root, engine) = engine(&["x1.txt", "x2.txt", "x3.txt"]).await;
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["x3.txt".into(), "x1.txt".into()],
        })
        .await
        .expect("selects");
    let mut request = replacing(&[], "", "");
    request.rule.name_mask = "pic[C]".to_owned();
    let Outcome::RenamePreview { plan, .. } = engine
        .dispatch(Command::MultiRenamePreview(request))
        .await
        .expect("previews")
    else {
        panic!("a preview");
    };
    let pairs: Vec<(&str, &str)> = plan
        .items
        .iter()
        .map(|item| (item.old.as_str(), item.new.as_str()))
        .collect();
    assert_eq!(pairs, [("x1.txt", "pic1.txt"), ("x3.txt", "pic2.txt")]);
}

#[tokio::test]
async fn names_that_are_not_in_the_folder_are_refused() {
    let (_root, engine) = engine(&["a.txt"]).await;
    let refused = engine
        .dispatch(Command::MultiRenamePreview(replacing(
            &["../a.txt"],
            "a",
            "b",
        )))
        .await
        .expect_err("not an entry");
    assert_eq!(refused.code(), "entry_not_found");
}

#[tokio::test]
async fn a_swap_is_applied_and_undone() {
    let (root, engine) = engine(&["a.txt", "b.txt"]).await;
    let applied = engine
        .dispatch(Command::MultiRenameApply(replacing(
            &["a.txt", "b.txt"],
            "a|b",
            "b|a",
        )))
        .await
        .expect("queued");
    let done = finished(&engine, applied).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(root.path(), "a.txt"), "b.txt");
    assert_eq!(content(root.path(), "b.txt"), "a.txt");

    let undone = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect("undo queued");
    let done = finished(&engine, undone).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(root.path(), "a.txt"), "a.txt");
    assert_eq!(content(root.path(), "b.txt"), "b.txt");

    let again = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect_err("undone already");
    assert_eq!(again.code(), "rename_nothing_to_undo");
}

#[tokio::test]
async fn a_plan_with_conflicts_or_without_changes_renames_nothing() {
    let (root, engine) = engine(&["a.txt", "c.txt"]).await;
    let refused = engine
        .dispatch(Command::MultiRenameApply(replacing(&["a.txt"], "a", "c")))
        .await
        .expect_err("c.txt stays");
    assert_eq!(refused.code(), "rename_plan_conflicts");
    assert_eq!(content(root.path(), "a.txt"), "a.txt");
    assert_eq!(content(root.path(), "c.txt"), "c.txt");

    let unchanged = engine
        .dispatch(Command::MultiRenameApply(replacing(&["a.txt"], "z", "y")))
        .await
        .expect_err("no change");
    assert_eq!(unchanged.code(), "rename_no_changes");
    let nothing = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect_err("nothing ran");
    assert_eq!(nothing.code(), "rename_nothing_to_undo");
}

#[tokio::test]
async fn an_undo_that_no_longer_fits_renames_nothing() {
    let (root, engine) = engine(&["a.txt", "b.txt"]).await;
    let applied = engine
        .dispatch(Command::MultiRenameApply(replacing(
            &["a.txt", "b.txt"],
            "a|b",
            "x|y",
        )))
        .await
        .expect("queued");
    finished(&engine, applied).await;
    fs::remove_file(root.path().join("y.txt")).expect("someone removes it");

    let refused = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect_err("y.txt is gone");
    assert_eq!(refused.code(), "rename_undo_mismatch");
    assert!(refused.to_string().contains("y.txt"), "{refused}");
    assert_eq!(content(root.path(), "x.txt"), "a.txt", "nothing was undone");
}

#[tokio::test]
async fn the_undo_log_survives_a_restart() {
    let root = tempfile::tempdir().expect("temp dir");
    let folder = root.path().join("files");
    fs::create_dir(&folder).expect("mkdir");
    fs::write(folder.join("old.txt"), "old.txt").expect("write");
    let db_path = root.path().join("rcommander.db");

    let engine = test_support::headless(&folder).await;
    let db = Db::open(&db_path).await.expect("db");
    engine.attach_db(db.clone());
    let applied = engine
        .dispatch(Command::MultiRenameApply(replacing(
            &["old.txt"],
            "old",
            "new",
        )))
        .await
        .expect("queued");
    finished(&engine, applied).await;
    drop(engine);
    db.close().await;

    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine after the restart");
    engine.attach_db(Db::open(&db_path).await.expect("db again"));
    let undone = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect("undo queued");
    let done = finished(&engine, undone).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(&folder, "old.txt"), "old.txt");
}

#[tokio::test]
async fn an_undo_after_a_crash_reads_the_last_step_off_the_folder() {
    let (root, engine) = engine(&["a.txt", "b.txt"]).await;
    // A swap that crashed after its second rename, before marking it.
    fs::rename(
        root.path().join("a.txt"),
        root.path().join(".rc-rename-c-1"),
    )
    .expect("step 0");
    fs::rename(root.path().join("b.txt"), root.path().join("a.txt")).expect("step 1");
    let db = engine.db().expect("db").clone();
    let dir = serde_json::to_string(&VfsPath::local(root.path())).expect("json");
    db.log_rename_run(
        "c",
        &dir,
        &[
            ("a.txt", ".rc-rename-c-1"),
            ("b.txt", "a.txt"),
            (".rc-rename-c-1", "b.txt"),
        ],
    )
    .await
    .expect("log");
    db.mark_rename_step("c", 0, true).await.expect("mark");

    let undone = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect("undo queued");
    let done = finished(&engine, undone).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(root.path(), "a.txt"), "a.txt");
    assert_eq!(content(root.path(), "b.txt"), "b.txt");
    assert!(!root.path().join(".rc-rename-c-1").exists());
    let run = db.latest_rename_run().await.expect("read").expect("run");
    assert_eq!(run.marked(), 0, "the undo cleared every mark");
}

#[tokio::test]
async fn without_a_database_nothing_is_renamed() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::write(root.path().join("a.txt"), "a").expect("write");
    let engine = test_support::headless(root.path()).await;
    let refused = engine
        .dispatch(Command::MultiRenameApply(replacing(&["a.txt"], "a", "b")))
        .await
        .expect_err("no undo log");
    assert_eq!(refused.code(), "database_unavailable");
    assert!(root.path().join("a.txt").exists());
}

#[test]
fn the_undo_log_keeps_what_rc_rename_promises() {
    let table = rc_db::retained_table("rename_log").expect("retained");
    assert_eq!(
        table.defaults.max_rows,
        u64::from(rc_rename::UNDO_LOG_MAX_STEPS)
    );
    assert_eq!(
        table.defaults.max_age_days,
        rc_rename::UNDO_LOG_MAX_AGE_DAYS
    );
}

#[tokio::test]
async fn two_applies_at_the_same_moment_start_one_run() {
    let (root, engine) = engine(&["a.txt", "b.txt"]).await;
    let (first, second) = tokio::join!(
        engine.dispatch(Command::MultiRenameApply(replacing(&["a.txt"], "a", "x"))),
        engine.dispatch(Command::MultiRenameApply(replacing(&["b.txt"], "b", "y"))),
    );
    let (started, refused) = match (first, second) {
        (Ok(outcome), Err(error)) | (Err(error), Ok(outcome)) => (outcome, error),
        (first, second) => panic!("one run and one refusal, got {first:?} and {second:?}"),
    };
    assert_eq!(refused.code(), "rename_busy");
    let done = finished(&engine, started).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let renamed = [root.path().join("x.txt"), root.path().join("y.txt")]
        .iter()
        .filter(|path| path.exists())
        .count();
    assert_eq!(renamed, 1, "only the run that started renamed");

    // The claim is free again once the job has ended, after a refusal as after a run.
    let next = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect("the next run starts");
    finished(&engine, next).await;
}

/// `sub\[N]` moves into the subfolder (made when missing), a name the subfolder holds already is
/// a conflict, the undo brings the file back, and the run's rule is the one last used.
#[tokio::test]
async fn a_subfolder_in_the_mask_moves_and_the_undo_moves_back() {
    let (root, engine) = engine(&["a.txt", "b.txt"]).await;
    fs::create_dir(root.path().join("old")).expect("mkdir");
    fs::write(root.path().join("old").join("b.txt"), "taken").expect("write");
    let request = |names: &[&str], mask: &str| MultiRenameRequest {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        rule: RenameRule {
            name_mask: mask.to_owned(),
            ..RenameRule::default()
        },
        ..MultiRenameRequest::default()
    };
    let Outcome::RenamePreview { plan, .. } = engine
        .dispatch(Command::MultiRenamePreview(request(
            &["a.txt", "b.txt"],
            r"old\[N]",
        )))
        .await
        .expect("previews")
    else {
        panic!("a preview");
    };
    assert_eq!(plan.items[0].new, "old/a.txt");
    assert_eq!(plan.items[0].conflict, None);
    assert_eq!(plan.items[1].conflict, Some(Conflict::TargetExists));

    let applied = engine
        .dispatch(Command::MultiRenameApply(request(&["a.txt"], r"new\[N]")))
        .await
        .expect("queued");
    let done = finished(&engine, applied).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(&root.path().join("new"), "a.txt"), "a.txt");
    assert!(!root.path().join("a.txt").exists());
    let Outcome::RenamePresets { last, .. } = engine
        .dispatch(Command::MultiRenamePresetList)
        .await
        .expect("lists")
    else {
        panic!("the presets");
    };
    assert_eq!(last.map(|rule| rule.name_mask), Some(r"new\[N]".to_owned()));

    let undone = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect("undo queued");
    let done = finished(&engine, undone).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(content(root.path(), "a.txt"), "a.txt");
}

#[tokio::test]
async fn a_path_out_of_the_folder_is_never_renamed() {
    let (root, engine) = engine(&["a.txt"]).await;
    for mask in [r"..\[N]", r"C:\[N]", r"\\server\[N]", r"a\\[N]"] {
        let request = MultiRenameRequest {
            names: vec!["a.txt".to_owned()],
            rule: RenameRule {
                name_mask: mask.to_owned(),
                ..RenameRule::default()
            },
            ..MultiRenameRequest::default()
        };
        let refused = engine
            .dispatch(Command::MultiRenameApply(request))
            .await
            .expect_err("a conflict");
        assert_eq!(refused.code(), "rename_plan_conflicts", "{mask}");
    }
    assert_eq!(content(root.path(), "a.txt"), "a.txt");
}

/// The owner's test list for Ctrl+M (RC-080-14): `[N]_[C]`, `[N2-5]`, `[E]`, `[Y][M][D]`, the
/// case, a regex search and the counter's start, step and width; a conflict in the preview;
/// applied, then undone.
#[tokio::test]
async fn the_test_list_masks_preview_apply_and_undo() {
    let files = ["beach day.JPG", "holiday photo.jpg", "mountain.png"];
    let (root, engine) = engine(&files).await;
    // 2024-05-06 12:00 UTC: the same day in every time zone the date is rendered in.
    for name in files {
        let file = fs::File::options()
            .write(true)
            .open(root.path().join(name))
            .expect("open");
        let noon = std::time::UNIX_EPOCH + Duration::from_secs(1_714_996_800);
        file.set_modified(noon).expect("time");
    }
    let request = |rule: RenameRule| MultiRenameRequest {
        names: files.iter().map(|name| (*name).to_owned()).collect(),
        rule,
        ..MultiRenameRequest::default()
    };
    let preview = async |rule: RenameRule| {
        let outcome = engine.dispatch(Command::MultiRenamePreview(request(rule)));
        let Outcome::RenamePreview { plan, .. } = outcome.await.expect("previews") else {
            panic!("a preview");
        };
        plan.items
    };
    let news = |items: &[rc_rename::RenameItem]| -> Vec<String> {
        items.iter().map(|item| item.new.clone()).collect()
    };

    let plain = RenameRule {
        name_mask: "[N]_[C]".to_owned(),
        ..RenameRule::default()
    };
    let items = preview(plain).await;
    assert_eq!(
        news(&items),
        ["beach day_1.JPG", "holiday photo_2.jpg", "mountain_3.png"]
    );
    let same = RenameRule {
        name_mask: "same".to_owned(),
        extension_mask: "x".to_owned(),
        ..RenameRule::default()
    };
    let items = preview(same).await;
    assert_eq!(items[0].conflict, Some(Conflict::Duplicate { other: 1 }));
    assert_eq!(items[1].conflict, Some(Conflict::Duplicate { other: 0 }));

    let full = RenameRule {
        name_mask: "[N2-5]_[C]_[Y][M][D]".to_owned(),
        extension_mask: "[E]".to_owned(),
        case: rc_rename::CaseMode::Lower,
        search: Some(SearchReplace {
            find: r"^(\w{4})_(\d{3})".to_owned(),
            replace: "$2-$1".to_owned(),
            regex: true,
            ..SearchReplace::default()
        }),
        counter: rc_rename::CounterSettings {
            start: 10,
            step: 5,
            width: 3,
        },
    };
    let wanted = [
        "010-each_20240506.jpg",
        "015-olid_20240506.jpg",
        "020-ount_20240506.png",
    ];
    let items = preview(full.clone()).await;
    assert_eq!(news(&items), wanted);
    assert!(
        items.iter().all(|item| item.conflict.is_none()),
        "{items:?}"
    );

    let applied = engine.dispatch(Command::MultiRenameApply(request(full)));
    let done = finished(&engine, applied.await.expect("queued")).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    for (old, new) in files.iter().zip(wanted) {
        assert_eq!(content(root.path(), new), *old);
    }
    let undone = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect("undo");
    assert_eq!(finished(&engine, undone).await.state, JobState::Completed);
    for name in files {
        assert_eq!(content(root.path(), name), name, "the old name is back");
    }
}
