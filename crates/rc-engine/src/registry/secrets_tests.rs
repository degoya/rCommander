use crate::{Command, CommandClass, REDACTED, SecretParam, spec};

/// Storing, reading or removing a secret and every use of the master password is a person's
/// decision: MCP never runs one (`dispatch_mcp` refuses `gui_only`).
#[test]
fn secret_and_vault_changes_are_gui_only() {
    for id in [
        "secrets.import_key",
        "vault.create",
        "vault.unlock",
        "vault.change_password",
        "vault.remove",
    ] {
        assert_eq!(
            spec(id).expect("a row").class,
            CommandClass::GuiOnly,
            "{id}"
        );
    }
    assert_eq!(
        spec("vault.status").expect("a row").class,
        CommandClass::Read
    );
    assert_eq!(spec("vault.lock").expect("a row").class, CommandClass::Read);
}

#[test]
fn a_secret_parameter_is_never_written_out() {
    let command = Command::UnlockVault {
        password: SecretParam::from("hunter2-canary"),
    };
    let debug = format!("{command:?}");
    let json = serde_json::to_string(&command).expect("serializes");
    for text in [&debug, &json] {
        assert!(!text.contains("hunter2-canary"), "{text}");
        assert!(text.contains(REDACTED), "{text}");
    }
    let typed: Command = serde_json::from_str(
        r#"{"command":"vault.unlock","params":{"password":"hunter2-canary"}}"#,
    )
    .expect("deserializes");
    assert_eq!(typed, command);
}
