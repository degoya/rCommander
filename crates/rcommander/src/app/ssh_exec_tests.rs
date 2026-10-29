//! The "Run command" dialog through `apply`: a run and its answer, a cancel, a stale answer, and
//! the keys while it is open. The run itself is the engine's (`ssh.exec`), tested there.

use iced::widget::text_editor::{Action, Edit};
use rc_engine::{ConnectionId, ExecOutput};
use tempfile::TempDir;

use super::{ExecMsg, Stream};
use crate::app::{Message, harness::Harness};

async fn send(harness: &mut Harness, message: ExecMsg) {
    harness.send(Message::SshExec(message)).await;
}

async fn opened() -> (TempDir, Harness) {
    let (root, mut harness) = Harness::temp().await;
    send(
        &mut harness,
        ExecMsg::Open {
            connection: ConnectionId(7),
            name: "web".to_owned(),
        },
    )
    .await;
    send(&mut harness, ExecMsg::Command("uptime".to_owned())).await;
    (root, harness)
}

fn output(stdout: &str) -> Box<ExecOutput> {
    Box::new(ExecOutput {
        stdout: stdout.to_owned(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        exit_code: Some(0),
        exit_signal: None,
        timed_out: false,
        duration_ms: 5,
    })
}

#[tokio::test]
async fn a_run_shows_its_answer_and_ignores_a_stale_one() {
    let (_root, mut harness) = opened().await;
    send(&mut harness, ExecMsg::Run).await;
    let dialog = harness.app.ssh_exec.as_ref().expect("open");
    assert!(dialog.is_running());
    assert_eq!(dialog.connection, ConnectionId(7));

    send(&mut harness, ExecMsg::Finished(99, Ok(output("old")))).await;
    assert!(
        harness
            .app
            .ssh_exec
            .as_ref()
            .is_some_and(|d| d.is_running())
    );

    send(&mut harness, ExecMsg::Finished(1, Ok(output("up 3 days")))).await;
    let dialog = harness.app.ssh_exec.as_ref().expect("open");
    assert!(!dialog.is_running());
    assert!(
        matches!(&dialog.result, Some(Ok(output)) if output.stdout == "up 3 days"),
        "the answer is shown"
    );
}

#[tokio::test]
async fn cancel_ends_the_run_and_its_late_answer_is_dropped() {
    let (_root, mut harness) = opened().await;
    send(&mut harness, ExecMsg::Run).await;
    send(&mut harness, ExecMsg::Cancel).await;
    send(&mut harness, ExecMsg::Finished(1, Ok(output("late")))).await;
    let dialog = harness.app.ssh_exec.as_ref().expect("open");
    assert!(!dialog.is_running());
    assert!(matches!(dialog.result, Some(Err("cancelled"))));
}

#[tokio::test]
async fn an_empty_command_does_not_run() {
    let (_root, mut harness) = opened().await;
    send(&mut harness, ExecMsg::Command("   ".to_owned())).await;
    send(&mut harness, ExecMsg::Run).await;
    assert!(
        harness
            .app
            .ssh_exec
            .as_ref()
            .is_some_and(|d| !d.is_running())
    );
}

#[tokio::test]
async fn keys_stay_in_the_dialog_and_escape_closes_it() {
    let (_root, mut harness) = opened().await;
    send(&mut harness, ExecMsg::Run).await;
    harness.press("Tab").await;
    assert!(harness.app.ssh_exec.is_some(), "a key does not close it");
    harness.press("Escape").await;
    assert!(
        harness.app.ssh_exec.is_none(),
        "Escape closes it and drops the run"
    );
}

/// RC-080-53: the output is laid out once, as a read-only editor per stream - an edit is
/// dropped, selecting works.
#[tokio::test]
async fn the_output_is_read_only_and_selectable() {
    let (_root, mut harness) = opened().await;
    send(&mut harness, ExecMsg::Run).await;
    send(&mut harness, ExecMsg::Finished(1, Ok(output("one\ntwo")))).await;
    let typed = Action::Edit(Edit::Insert('x'));
    send(&mut harness, ExecMsg::Scroll(Stream::Out, typed)).await;
    send(
        &mut harness,
        ExecMsg::Scroll(Stream::Out, Action::SelectAll),
    )
    .await;
    let dialog = harness.app.ssh_exec.as_ref().expect("open");
    let [stdout, stderr] = &**dialog.shown.as_ref().expect("shown");
    assert_eq!(stdout.text(), "one\ntwo");
    assert_eq!(stdout.selection().as_deref(), Some("one\ntwo"));
    assert_eq!(stderr.text(), "");

    send(&mut harness, ExecMsg::Run).await;
    let dialog = harness.app.ssh_exec.as_ref().expect("open");
    assert!(dialog.shown.is_none(), "a new run drops the old output");
}
