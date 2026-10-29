//! `ssh.exec` against the in-process SSH server, on the session tests' harness (declared from
//! `sessions.rs`, whose harness it shares).

use super::tests::harness;
use crate::{Command, ConnectionId, Outcome, SshExecRequest};

fn request(id: ConnectionId, command: &str) -> SshExecRequest {
    SshExecRequest {
        connection_id: id,
        command: command.to_owned(),
        stdin: None,
        timeout_secs: None,
        max_output_bytes: None,
    }
}

#[tokio::test]
async fn exec_answers_the_streams_and_the_exit_code() {
    let harness = harness(None, true).await;
    let outcome = harness
        .engine
        .dispatch(Command::SshExec(request(
            harness.id,
            "sh -c 'echo out; echo err >&2; exit 3'",
        )))
        .await
        .expect("ran");
    let Outcome::SshExec(output) = outcome else {
        panic!("an exec outcome: {outcome:?}");
    };
    assert_eq!(output.stdout, "out\n");
    assert_eq!(output.stderr, "err\n");
    assert_eq!(output.exit_code, Some(3));
    assert_eq!(
        harness
            .engine
            .ssh_session(harness.id)
            .map(|status| status.channels),
        Some(0),
        "the channel's lease is given back"
    );
}

#[tokio::test]
async fn a_command_over_its_time_leaves_the_session_usable() {
    let harness = harness(None, true).await;
    let slow = SshExecRequest {
        timeout_secs: Some(1),
        ..request(harness.id, "sh -c 'sleep 30'")
    };
    let Ok(Outcome::SshExec(output)) = harness.engine.dispatch(Command::SshExec(slow)).await else {
        panic!("a timeout is an answer");
    };
    assert!(output.timed_out);
    let again = harness
        .engine
        .dispatch(Command::SshExec(request(harness.id, "sh -c 'exit 0'")))
        .await;
    assert!(
        matches!(again, Ok(Outcome::SshExec(ref output)) if output.exit_code == Some(0)),
        "{again:?}"
    );
    assert_eq!(harness.server.accepts(), 1, "one session for both");
}

#[tokio::test]
async fn standard_input_reaches_the_command() {
    let harness = harness(None, true).await;
    let with_input = SshExecRequest {
        stdin: Some("line one\n".to_owned()),
        ..request(harness.id, "cat")
    };
    let Ok(Outcome::SshExec(output)) = harness.engine.dispatch(Command::SshExec(with_input)).await
    else {
        panic!("ran");
    };
    assert_eq!(output.stdout, "line one\n");
}

#[tokio::test]
async fn requests_out_of_bounds_are_refused_before_connecting() {
    let harness = harness(None, true).await;
    let refused = [
        request(harness.id, "  "),
        request(harness.id, "echo a\0b"),
        request(harness.id, &"x".repeat(crate::MAX_EXEC_COMMAND + 1)),
        SshExecRequest {
            stdin: Some("y".repeat(crate::MAX_EXEC_STDIN + 1)),
            ..request(harness.id, "cat")
        },
        SshExecRequest {
            timeout_secs: Some(0),
            ..request(harness.id, "true")
        },
        SshExecRequest {
            timeout_secs: Some(3601),
            ..request(harness.id, "true")
        },
        SshExecRequest {
            max_output_bytes: Some(0),
            ..request(harness.id, "true")
        },
        SshExecRequest {
            max_output_bytes: Some(4 * 1024 * 1024 + 1),
            ..request(harness.id, "true")
        },
    ];
    for request in refused {
        let answer = harness.engine.dispatch(Command::SshExec(request)).await;
        assert_eq!(
            answer.as_ref().map_err(crate::EngineError::code).err(),
            Some("exec_invalid"),
            "{answer:?}"
        );
    }
    assert_eq!(harness.server.accepts(), 0, "nothing was connected");
}
