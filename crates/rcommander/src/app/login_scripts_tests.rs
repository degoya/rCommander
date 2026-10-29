use rc_engine::{
    Command, LoginScriptId, LoginScriptInfo, Pattern, ScriptSecretLabel, ScriptSecretRef, Step,
    login_script::TEMPLATES,
};

use super::*;

fn saved(steps: Vec<Step>) -> LoginScriptInfo {
    LoginScriptInfo {
        id: LoginScriptId(7),
        name: "sudo".into(),
        steps,
        step_timeout_ms: 15_000,
        total_timeout_ms: 120_000,
        secrets: vec![ScriptSecretLabel {
            id: 3,
            label: "root".into(),
        }],
        connections: Vec::new(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[test]
fn an_invalid_pattern_shows_at_its_field_and_is_not_saved() {
    let mut editor = Editor::default();
    editor.apply(Message::Name("sudo".into()));
    editor.apply(Message::Add(StepKind::WaitFor));
    editor.apply(Message::Text(0, "(?i)password(".into()));
    assert_eq!(editor.draft.steps[0].error, Some(draft::PATTERN_INVALID));
    assert!(matches!(editor.apply(Message::Save), Action::None));
    assert_eq!(
        editor.error.as_deref(),
        Some("login_script_pattern_invalid")
    );

    editor.apply(Message::Text(0, "(?i)password.*:".into()));
    assert_eq!(editor.draft.steps[0].error, None);
    let Action::Dispatch(Command::CreateLoginScript { script }) = editor.apply(Message::Save)
    else {
        panic!("a create");
    };
    assert_eq!(script.name, "sudo");
    assert_eq!(
        script.steps,
        [Step::WaitFor {
            pattern: Pattern::new("(?i)password.*:").expect("pattern"),
            timeout_ms: None
        }]
    );
}

#[test]
fn a_bad_number_is_not_saved_either() {
    let mut editor = Editor::default();
    editor.apply(Message::Name("x".into()));
    editor.apply(Message::Add(StepKind::Sleep));
    editor.apply(Message::Sleep(0, "soon".into()));
    assert!(matches!(editor.apply(Message::Save), Action::None));
    assert_eq!(
        editor.error.as_deref(),
        Some("login_script_timeout_invalid")
    );
}

#[test]
fn a_template_fills_the_form_and_saves_as_its_steps() {
    let mut editor = Editor::default();
    editor.apply(Message::Template("sudo_i"));
    assert_eq!(editor.draft.steps.len(), 4);
    assert!(!editor.draft.name.is_empty());
    let Action::Dispatch(Command::CreateLoginScript { script }) = editor.apply(Message::Save)
    else {
        panic!("a create");
    };
    assert_eq!(script.steps, TEMPLATES[0].script().steps);
}

#[test]
fn steps_are_added_moved_and_removed() {
    let mut editor = Editor::default();
    for kind in [StepKind::SendLine, StepKind::Sleep, StepKind::SendSecret] {
        editor.apply(Message::Add(kind));
    }
    editor.apply(Message::Up(2));
    editor.apply(Message::Down(0));
    let kinds: Vec<StepKind> = editor.draft.steps.iter().map(|row| row.kind).collect();
    assert_eq!(
        kinds,
        [StepKind::SendSecret, StepKind::SendLine, StepKind::Sleep]
    );
    editor.apply(Message::Remove(1));
    editor.apply(Message::Up(0));
    editor.apply(Message::Down(1));
    editor.apply(Message::Remove(9));
    let kinds: Vec<StepKind> = editor.draft.steps.iter().map(|row| row.kind).collect();
    assert_eq!(kinds, [StepKind::SendSecret, StepKind::Sleep]);
}

#[test]
fn a_saved_script_offers_its_secrets_and_updates_on_save() {
    let mut editor = Editor::default();
    let prompt = Step::WaitFor {
        pattern: Pattern::new("(?i)password.*:").expect("pattern"),
        timeout_ms: None,
    };
    let steps = vec![
        prompt.clone(),
        Step::SendSecret {
            source: ScriptSecretRef::Stored { id: 3 },
            newline: true,
        },
    ];
    editor.apply(Message::Saved(Ok(saved(steps))));
    assert_eq!(editor.scripts.len(), 1);
    assert_eq!(editor.draft.steps[1].source, SourceChoice::Stored(3));
    editor.apply(Message::Source(1, SourceChoice::ConnectionPassword));
    let Action::Dispatch(Command::UpdateLoginScript { id, patch }) = editor.apply(Message::Save)
    else {
        panic!("an update");
    };
    assert_eq!(id, LoginScriptId(7));
    assert_eq!(
        patch.steps,
        Some(vec![
            prompt,
            Step::SendSecret {
                source: ScriptSecretRef::ConnectionPassword,
                newline: true
            }
        ])
    );
}

#[test]
fn a_typed_secret_is_never_in_debug_and_leaves_with_the_store() {
    let mut editor = Editor::default();
    editor.apply(Message::SecretValue("hunter2-typed".into()));
    assert!(matches!(editor.apply(Message::StoreSecret), Action::None));
    assert_eq!(editor.error.as_deref(), Some("login_script_not_saved"));

    editor.apply(Message::Saved(Ok(saved(Vec::new()))));
    editor.apply(Message::SecretLabel("root".into()));
    let message = Message::SecretValue("hunter2-typed".into());
    assert!(!format!("{message:?}").contains("hunter2"));
    editor.apply(message);
    assert!(!format!("{editor:?}").contains("hunter2"));
    let action = editor.apply(Message::StoreSecret);
    assert!(!format!("{action:?}").contains("hunter2"));
    let Action::Dispatch(Command::SetLoginScriptSecret {
        script_id, label, ..
    }) = action
    else {
        panic!("a store");
    };
    assert_eq!((script_id, label.as_str()), (LoginScriptId(7), "root"));
    assert!(editor.secret_label.is_empty());
    assert!(rc_vault::ExposeSecret::expose_secret(&editor.secret_value).is_empty());
}

#[test]
fn the_test_run_takes_the_sample_with_its_escapes() {
    let mut editor = Editor::default();
    editor.apply(Message::Template("sudo_i"));
    editor.apply(Message::Sample(r"$ sudo -i\r\n\e[1mPassword:\e[0m ".into()));
    let Action::Rehearse(script, sample) = editor.apply(Message::Rehearse) else {
        panic!("a test run");
    };
    assert_eq!(script.steps.len(), 4);
    assert_eq!(sample, "$ sudo -i\r\n\u{1b}[1mPassword:\u{1b}[0m ");
}

#[test]
fn escapes_and_lines() {
    assert_eq!(unescape(r"a\\n\x\"), "a\\n\\x\\");
    assert_eq!(view::line_of("a\nb\nc", 0), 1);
    assert_eq!(view::line_of("a\nb\nc", 3), 2);
    assert_eq!(view::line_of("a\nb\nc", 99), 3);
}

#[test]
fn every_message_without_a_target_changes_nothing() {
    let mut editor = Editor::default();
    for message in [
        Message::Delete,
        Message::RemoveSecret(1),
        Message::Text(4, "x".into()),
        Message::Select(LoginScriptId(1)),
        Message::Template("no_such_template"),
    ] {
        assert!(matches!(editor.apply(message), Action::None));
    }
    assert_eq!(editor.draft, Draft::default());
}

/// Every template sends its secret to a password prompt: no warning (RC-050-13).
#[test]
fn the_templates_warn_nowhere() {
    for template in TEMPLATES {
        let draft = Draft::from_template(template);
        for at in 0..draft.steps.len() {
            assert_eq!(draft.secret_warning(at), None, "{} step {at}", template.id);
        }
    }
}

/// A secret after a root prompt's "Wait for" - the fifth step the owner's script had - is
/// warned about, one without a "Wait for" before it too; a password or passphrase prompt is not.
#[test]
fn a_secret_without_a_password_prompt_before_it_is_warned_about() {
    let mut draft = Draft::from_template(&TEMPLATES[0]);
    draft.steps.push(StepRow::new(StepKind::SendSecret));
    let last = draft.steps.len() - 1;
    assert_eq!(draft.secret_warning(last), Some(SecretWarning::NoPrompt));

    let mut lone = Draft::default();
    lone.steps.push(StepRow::new(StepKind::SendSecret));
    assert_eq!(lone.secret_warning(0), Some(SecretWarning::NoWait));
    lone.steps.insert(0, StepRow::new(StepKind::SendText));
    assert_eq!(lone.secret_warning(1), Some(SecretWarning::NoWait));

    for pattern in [
        "(?i)passphrase",
        "(?i)passw",
        "(?i)verification code",
        "PIN:",
    ] {
        let mut wait = StepRow::new(StepKind::WaitFor);
        wait.set_text(pattern.to_owned());
        lone.steps[0] = wait;
        assert_eq!(lone.secret_warning(1), None, "{pattern}");
    }
    assert_eq!(lone.secret_warning(0), None, "a Wait for is no secret");
}

/// The editor warns until the setting is read as off.
#[test]
fn the_setting_switches_the_warning() {
    let (mut editor, _) = Editor::open();
    assert!(editor.warn_prompt);
    editor.apply(Message::WarnPrompt(false));
    assert!(!editor.warn_prompt);
}
