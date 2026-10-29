//! A shell on an SSH server never moves the local panel (RC-080-10): what it prints is the
//! server's, so a folder it reports - a share of the attacker's, or a folder that happens to
//! exist on this machine - is not followed when the panel turns back to its files.

use super::tests::{eventually, harness, open_ssh, opened};
use crate::{Command, EngineError, Outcome, PanelSnapshot, PanelView};

#[tokio::test]
async fn a_folder_an_ssh_shell_reports_never_moves_the_local_panel() {
    let h = harness().await;
    let elsewhere = tempfile::tempdir().expect("temp dir");
    let info = opened(h.engine.dispatch(open_ssh(h.id, None)).await);
    let panel = |outcome: Result<Outcome, EngineError>| match outcome {
        Ok(Outcome::Panel(snapshot)) => snapshot,
        other => panic!("a panel, got {other:?}"),
    };
    let before: PanelSnapshot = panel(
        h.engine
            .dispatch(Command::GetPanelState {
                panel: Some(info.panel),
            })
            .await,
    );
    assert_eq!(before.view, PanelView::Terminal);

    // The test server echoes what it is typed, so these come back as the server's output: a
    // share first, then a folder of this machine, as OSC 9;9 and OSC 7.
    let folder = elsewhere.path().to_string_lossy().into_owned();
    for text in [
        "\x1b]9;9;\\\\attacker\\share\x07".to_owned(),
        format!("\x1b]9;9;{folder}\x07"),
        format!("\x1b]7;file://localhost{folder}\x07"),
    ] {
        h.engine
            .dispatch(Command::SendInput {
                terminal: info.terminal,
                text,
            })
            .await
            .expect("input");
    }
    let session = h.engine.terminal_session(info.terminal).expect("session");
    eventually("the server's folder report", || {
        session.reported_folder().as_deref() == Some(elsewhere.path())
    })
    .await;

    let shown = panel(
        h.engine
            .dispatch(Command::ShowFiles {
                panel: Some(info.panel),
            })
            .await,
    );
    assert_eq!(shown.view, PanelView::Files);
    assert_eq!(
        shown.path, before.path,
        "the local panel stays where it was"
    );
}
