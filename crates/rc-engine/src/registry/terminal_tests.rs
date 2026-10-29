use crate::{Command, CommandClass, TermId, spec};

#[test]
fn a_terminal_command_reads_with_defaults() {
    let open: Command =
        serde_json::from_str(r#"{"command":"terminal.open","params":{}}"#).expect("deserializes");
    assert_eq!(
        open,
        Command::OpenTerminal {
            panel: None,
            shell: None,
            distro: None,
            cwd: None,
            size: None
        }
    );
    let input: Command = serde_json::from_str(
        r#"{"command":"terminal.send_input","params":{"terminal":7,"text":"ls\r"}}"#,
    )
    .expect("deserializes");
    assert_eq!(
        input,
        Command::SendInput {
            terminal: TermId(7),
            text: "ls\r".into()
        }
    );
}

/// A command that starts or feeds a process is `exec`, so MCP always asks first; ending one is
/// `destructive`.
#[test]
fn terminal_commands_have_the_classes_their_effects_need() {
    let class = |id: &str| spec(id).expect("a row").class;
    assert_eq!(class("terminal.open"), CommandClass::Exec);
    // It starts a shell in a panel that shows its files (RC-040-05).
    assert_eq!(class("terminal.maximize"), CommandClass::Exec);
    assert_eq!(class("terminal.send_input"), CommandClass::Exec);
    assert_eq!(class("terminal.close"), CommandClass::Destructive);
    assert_eq!(class("terminal.list"), CommandClass::Read);
    assert_eq!(class("terminal.read_screen"), CommandClass::Read);
}
