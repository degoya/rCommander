use crate::{Command, CommandClass, ConnectionId, SshExecRequest, spec};

#[test]
fn ssh_exec_reads_with_defaults_and_is_exec() {
    let command: Command = serde_json::from_str(
        r#"{"command":"ssh.exec","params":{"connection_id":3,"command":"uptime"}}"#,
    )
    .expect("deserializes");
    assert_eq!(
        command,
        Command::SshExec(SshExecRequest {
            connection_id: ConnectionId(3),
            command: "uptime".to_owned(),
            stdin: None,
            timeout_secs: None,
            max_output_bytes: None,
        })
    );
    assert_eq!(command.id(), "ssh.exec");
    assert_eq!(
        spec("ssh.exec").map(|spec| spec.class),
        Some(CommandClass::Exec)
    );
}

/// A request is logged: its `Debug` masks the command and leaves the input out.
#[test]
fn debug_masks_the_command_and_hides_the_input() {
    let request = SshExecRequest {
        connection_id: ConnectionId(1),
        command: "deploy token=hunter2".to_owned(),
        stdin: Some("s3cret-input".to_owned()),
        timeout_secs: None,
        max_output_bytes: None,
    };
    let shown = format!("{request:?}");
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(!shown.contains("s3cret-input"), "{shown}");
    assert!(shown.contains("stdin_bytes: Some(12)"), "{shown}");
}
