//! `ops.delete`, `ops.mkdir`, `files.read_text` and `view.open` take a server's location
//! (`sftp://<connection>/<path>`) as `ops.copy` does, from a panel on a local folder (RC-060-05):
//! never joined onto the panel's directory.

use std::path::Path;

use rc_ops::{DeleteMode, ErrorPolicy, JobState};
use rc_vfs::BackendId;

use super::tests::{Harness, harness};
use crate::{Command, DeleteRequest, EngineEvent, Outcome, ReadJudge, viewer::ReadTextRequest};

const README: &str = "sftp://box/home/tester/readme.txt";

/// Runs `command`, a job of the queue, to its end.
async fn run_job(harness: &Harness, command: Command) {
    let queued = harness.engine.dispatch(command).await.expect("queued");
    let Outcome::Job(job) = queued else {
        panic!("a job, got {queued:?}");
    };
    let done = harness.engine.ops().wait(job.id).await.expect("known");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
}

fn read_text(path: &str) -> Command {
    Command::ReadText(ReadTextRequest {
        path: path.to_owned(),
        panel: None,
        offset: 0,
        max_bytes: None,
        encoding: None,
    })
}

/// A judge that refuses everything in a `.ssh` folder.
fn ssh_is_secret(path: &Path) -> Option<&'static str> {
    path.components()
        .any(|part| part.as_os_str() == ".ssh")
        .then_some("an SSH folder")
}

#[tokio::test]
async fn ops_mkdir_creates_a_directory_at_a_server_s_location() {
    let harness = harness().await;
    run_job(
        &harness,
        Command::OpsMkdir {
            panel: None,
            path: "sftp://box/home/tester/made".to_owned(),
        },
    )
    .await;
    assert!(harness.served.path().join("home/tester/made").is_dir());
}

#[tokio::test]
async fn ops_delete_removes_a_file_at_a_server_s_location() {
    let harness = harness().await;
    run_job(
        &harness,
        Command::OpsDelete(DeleteRequest {
            panel: None,
            sources: vec![README.to_owned()],
            mode: DeleteMode::Permanent,
            on_error: ErrorPolicy::Cancel,
            queue: None,
        }),
    )
    .await;
    assert!(
        !harness
            .served
            .path()
            .join("home/tester/readme.txt")
            .exists()
    );
}

#[tokio::test]
async fn files_read_text_reads_a_file_at_a_server_s_location() {
    let harness = harness().await;
    let read = harness
        .engine
        .dispatch_mcp(read_text(README))
        .await
        .expect("read");
    let Outcome::FileText(window) = read else {
        panic!("a text window, got {read:?}");
    };
    assert_eq!(
        window.path.backend,
        BackendId::Sftp {
            connection_id: harness.id.0
        }
    );
    assert_eq!(window.path.path, Path::new("/home/tester/readme.txt"));
    assert_eq!(window.text, "hello");
}

#[tokio::test]
async fn files_read_text_judges_the_server_s_path_under_mcp() {
    let harness = harness().await;
    let judge: ReadJudge = ssh_is_secret;
    assert!(harness.engine.attach_read_judge(judge));
    let refused = harness
        .engine
        .dispatch_mcp(read_text("sftp://box/home/tester/.ssh/id_ed25519"))
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "sensitive_path");
}

#[tokio::test]
async fn view_open_announces_a_file_at_a_server_s_location() {
    let harness = harness().await;
    let mut events = harness.engine.subscribe();
    harness
        .engine
        .dispatch(Command::ViewFile {
            panel: None,
            path: Some(README.to_owned()),
        })
        .await
        .expect("announced");
    let viewed = loop {
        match events.try_recv() {
            Ok(EngineEvent::ViewFile { path, .. }) => break path,
            Ok(_) => {}
            Err(error) => panic!("no view event: {error}"),
        }
    };
    assert_eq!(
        viewed.backend,
        BackendId::Sftp {
            connection_id: harness.id.0
        }
    );
    assert_eq!(viewed.path, Path::new("/home/tester/readme.txt"));
}
