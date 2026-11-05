//! A multi-rename that fails in the middle of a swap (RC-080-64): rolled back by its own job, or
//! with `rename.undo_on_failure` off - and when the roll-back fails too - the parked entry named
//! in the job's report.

use std::{fs, path::Path, sync::Arc, time::Duration};

use async_trait::async_trait;
use rc_ops::{Backend, JobReport, JobSnapshot, JobState};
use rc_rename::{RenameRule, SearchReplace, TEMP_PREFIX};
use rc_vfs::{BackendId, LocalFs, Vfs, VfsError};
use serde_json::json;

use super::MultiRenameRequest;
use crate::{Command, Engine, Outcome, settings::RENAME_UNDO_ON_FAILURE_KEY, test_support};

/// The local disk refusing the swap's middle step - the rename between two plain names, as an
/// open file refuses it on Windows - and with `back_too` every rename off a temporary name.
struct Refusing {
    inner: LocalFs,
    back_too: bool,
}

fn temporary(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(TEMP_PREFIX))
}

#[async_trait]
impl rc_vfs::testing::Forward for Refusing {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        let plain = !temporary(from) && !temporary(to);
        if plain || (self.back_too && temporary(from)) {
            return Err(VfsError::PermissionDenied {
                path: from.to_path_buf(),
            });
        }
        self.inner.rename(from, to).await
    }
}

/// `a.txt` and `b.txt` holding their names, on the refusing disk.
async fn engine(back_too: bool) -> (tempfile::TempDir, Engine) {
    let (root, engine) =
        test_support::engine_with_db(&[("a.txt", "a.txt"), ("b.txt", "b.txt")]).await;
    engine.ops().register(
        BackendId::Local,
        Backend::new(Arc::new(Refusing {
            inner: LocalFs::new(),
            back_too,
        })),
    );
    (root, engine)
}

async fn swap(engine: &Engine) -> JobSnapshot {
    let request = MultiRenameRequest {
        names: vec!["a.txt".to_owned(), "b.txt".to_owned()],
        rule: RenameRule {
            search: Some(SearchReplace {
                find: "a|b".to_owned(),
                replace: "b|a".to_owned(),
                case_sensitive: true,
                ..SearchReplace::default()
            }),
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
    tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
        .await
        .expect("in time")
        .expect("known")
}

fn names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root)
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The one parked entry of `done`'s report: its temporary name holds the old one's contents.
fn one_parked(root: &Path, done: &JobSnapshot) {
    let Some(JobReport::RenameStopped {
        rolled_back: false,
        parked,
    }) = &done.report
    else {
        panic!("a report of parked entries, got {done:?}");
    };
    assert_eq!(parked.len(), 1, "{parked:?}");
    let entry = &parked[0];
    assert!(entry.temp.starts_with(TEMP_PREFIX), "{entry:?}");
    assert_ne!(entry.old, entry.new, "{entry:?}");
    assert_eq!(
        fs::read_to_string(root.join(&entry.temp)).expect("parked"),
        entry.old
    );
}

#[tokio::test]
async fn a_failed_swap_is_rolled_back_and_leaves_nothing_hidden() {
    let (root, engine) = engine(false).await;
    let done = swap(&engine).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(
        done.failures[0].code, "permission_denied",
        "the first error"
    );
    assert_eq!(
        done.report,
        Some(JobReport::RenameStopped {
            rolled_back: true,
            parked: Vec::new(),
        })
    );
    assert_eq!(
        names(root.path()),
        ["a.txt", "b.txt"],
        "no temporary name left"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("a.txt")).expect("a"),
        "a.txt"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("b.txt")).expect("b"),
        "b.txt"
    );
    let nothing = engine
        .dispatch(Command::MultiRenameUndo)
        .await
        .expect_err("the run is undone already");
    assert_eq!(nothing.code(), "rename_nothing_to_undo");
}

#[tokio::test]
async fn a_roll_back_that_fails_too_names_the_parked_entry() {
    let (root, engine) = engine(true).await;
    let done = swap(&engine).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    one_parked(root.path(), &done);
}

#[tokio::test]
async fn with_the_setting_off_nothing_is_undone_but_the_entry_is_named() {
    let (root, engine) = engine(false).await;
    engine
        .dispatch(Command::SetSetting {
            key: RENAME_UNDO_ON_FAILURE_KEY.into(),
            value: json!(false),
        })
        .await
        .expect("an agent may switch it");
    let done = swap(&engine).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    one_parked(root.path(), &done);
}
