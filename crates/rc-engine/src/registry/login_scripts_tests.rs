use crate::{
    Command, CommandClass, LoginScriptDraft, LoginScriptId, Pattern, REDACTED, SecretParam, Step,
    spec,
};

/// Storing or removing a script's secret is a person's decision; making a script run on a
/// connection runs it on a server.
#[test]
fn secrets_are_gui_only_and_assigning_a_script_runs_it() {
    for id in ["login_scripts.set_secret", "login_scripts.clear_secret"] {
        assert_eq!(
            spec(id).expect("a row").class,
            CommandClass::GuiOnly,
            "{id}"
        );
    }
    assert_eq!(
        spec("login_scripts.assign").expect("a row").class,
        CommandClass::Exec
    );
    assert_eq!(
        spec("login_scripts.delete").expect("a row").class,
        CommandClass::Destructive
    );
}

#[test]
fn a_script_secret_parameter_is_never_written_out() {
    let command = Command::SetLoginScriptSecret {
        script_id: LoginScriptId(1),
        label: "root".into(),
        value: SecretParam::from("hunter2-root-canary"),
    };
    let debug = format!("{command:?}");
    let json = serde_json::to_string(&command).expect("serializes");
    for text in [&debug, &json] {
        assert!(!text.contains("hunter2-root-canary"), "{text}");
        assert!(text.contains(REDACTED), "{text}");
    }
}

#[test]
fn a_new_script_reads_with_its_steps() {
    let command: Command = serde_json::from_str(
        r#"{"command":"login_scripts.create","params":{"script":{"name":"sudo","steps":[
            {"step":"send_text","text":"sudo -i","newline":true},
            {"step":"wait_for","pattern":"(?i)password.*:"},
            {"step":"send_secret","source":{"kind":"connection_password"}}
        ]}}}"#,
    )
    .expect("reads");
    let Command::CreateLoginScript { script } = command else {
        panic!("a create, got {command:?}");
    };
    assert_eq!(
        script,
        LoginScriptDraft {
            name: "sudo".into(),
            steps: vec![
                Step::SendText {
                    text: "sudo -i".into(),
                    newline: true
                },
                Step::WaitFor {
                    pattern: Pattern::new("(?i)password.*:").expect("pattern"),
                    timeout_ms: None
                },
                Step::SendSecret {
                    source: crate::ScriptSecretRef::ConnectionPassword,
                    newline: true
                },
            ],
            step_timeout_ms: None,
            total_timeout_ms: None,
        }
    );
    let invalid = serde_json::from_str::<Command>(
        r#"{"command":"login_scripts.create","params":{"script":{"name":"x","steps":[
            {"step":"wait_for","pattern":"("}]}}}"#,
    );
    assert!(invalid.is_err());
}
