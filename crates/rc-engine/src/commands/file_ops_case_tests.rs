//! Shift+F6 to a name in the other case where the backend says case does not count but the
//! folder tells it apart (RC-110-22): the other file is a conflict and stays. Linux only - its
//! disk tells case apart, which the test needs beside a backend that says otherwise.

use std::{fs, sync::Arc, time::Duration};

use async_trait::async_trait;
use rc_ops::{Backend, ErrorPolicy, JobOptions, JobState};
use rc_vfs::{BackendId, Capabilities, LocalFs, Vfs};

use crate::{Command, PanelId, test_support};

/// The local disk saying case does not count, as the Windows build says of a WSL share.
struct Blind(LocalFs);

#[async_trait]
impl rc_vfs::testing::Forward for Blind {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    fn capabilities(&self) -> Capabilities {
        let mut capabilities = self.0.capabilities();
        capabilities.case_sensitive = false;
        capabilities
    }
}

#[tokio::test]
async fn a_rename_to_the_other_case_never_replaces_the_file_that_has_it() {
    let root = test_support::tree(&[("out/", ""), ("makefile", "mine"), ("Makefile", "theirs")]);
    let engine = test_support::open(root.path(), &root.path().join("out")).await;
    engine.ops().register(
        BackendId::Local,
        Backend::new(Arc::new(Blind(LocalFs::new()))),
    );
    let rename = Command::OpsMoveTo {
        panel: Some(PanelId::Left),
        sources: vec!["makefile".into()],
        target: "Makefile".into(),
        options: JobOptions {
            on_error: ErrorPolicy::Cancel,
            ..JobOptions::default()
        },
        queue: None,
    };
    let accepted = engine.dispatch(rename).await.expect("accepted");
    let jobs = test_support::expect_jobs(accepted);
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(jobs[0].id))
        .await
        .expect("in time")
        .expect("known");
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "already_exists");
    assert_eq!(
        fs::read(root.path().join("Makefile")).expect("read"),
        b"theirs"
    );
    assert_eq!(
        fs::read(root.path().join("makefile")).expect("read"),
        b"mine"
    );
    assert_eq!(
        fs::read_dir(root.path()).expect("list").count(),
        3,
        "no temporary name is left"
    );
}
