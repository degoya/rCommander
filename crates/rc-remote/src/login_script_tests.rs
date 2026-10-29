use super::*;

fn wait(pattern: &str) -> Step {
    Step::WaitFor {
        pattern: Pattern::new(pattern).expect("pattern"),
        timeout_ms: None,
    }
}

#[test]
fn a_script_checks_its_bounds() {
    let mut script = LoginScript::default();
    script.validate().expect("an empty script is valid");

    script.steps = vec![Step::SendLine; MAX_STEPS + 1];
    assert_eq!(
        script.validate().err().map(|error| error.code()),
        Some("login_script_too_many_steps")
    );

    script.steps = vec![
        Step::SendLine,
        Step::SendText {
            text: "x".repeat(MAX_TEXT_CHARS + 1),
            newline: false,
        },
    ];
    assert_eq!(
        script.validate(),
        Err(ScriptInvalid::TextTooLong {
            step: 1,
            max: MAX_TEXT_CHARS
        })
    );

    script.steps = vec![Step::Sleep {
        ms: MAX_SLEEP_MS + 1,
    }];
    assert_eq!(
        script.validate().err().map(|error| error.code()),
        Some("login_script_timeout_invalid")
    );

    script.steps = vec![Step::WaitFor {
        pattern: Pattern::new("x").expect("pattern"),
        timeout_ms: Some(0),
    }];
    assert!(script.validate().is_err());

    script.steps.clear();
    script.total_timeout_ms = MAX_TIMEOUT_MS + 1;
    assert!(script.validate().is_err());
}

#[test]
fn steps_read_and_write_as_tagged_json_with_defaults() {
    let steps: Vec<Step> = serde_json::from_str(
        r#"[
            {"step": "send_text", "text": "sudo -i"},
            {"step": "wait_for", "pattern": "(?i)password.*:", "timeout_ms": 5000},
            {"step": "send_secret", "source": {"kind": "stored", "id": 4}},
            {"step": "send_line"},
            {"step": "sleep", "ms": 250}
        ]"#,
    )
    .expect("reads");
    assert_eq!(
        steps,
        [
            Step::SendText {
                text: "sudo -i".into(),
                newline: false
            },
            Step::WaitFor {
                pattern: Pattern::new("(?i)password.*:").expect("pattern"),
                timeout_ms: Some(5000)
            },
            Step::SendSecret {
                source: SecretRef::Stored { id: 4 },
                newline: true
            },
            Step::SendLine,
            Step::Sleep { ms: 250 },
        ]
    );
    let json = steps_to_json(&steps).expect("writes");
    assert_eq!(
        steps_from_json(SCHEMA_VERSION, &json).expect("reads"),
        steps
    );
}

#[test]
fn stored_steps_of_another_version_or_shape_are_refused() {
    assert_eq!(
        steps_from_json(SCHEMA_VERSION + 1, "[]"),
        Err(ScriptInvalid::SchemaUnknown {
            version: SCHEMA_VERSION + 1
        })
    );
    let malformed = steps_from_json(SCHEMA_VERSION, r#"[{"step": "launch_rockets"}]"#);
    assert_eq!(
        malformed.err().map(|error| error.code()),
        Some("login_script_malformed")
    );
    let bad_pattern = steps_from_json(SCHEMA_VERSION, r#"[{"step": "wait_for", "pattern": "("}]"#);
    assert!(bad_pattern.is_err());
}

#[test]
fn the_json_of_a_secret_step_names_where_the_secret_is_not_what_it_is() {
    let steps = vec![
        Step::SendSecret {
            source: SecretRef::ConnectionPassword,
            newline: true,
        },
        Step::SendSecret {
            source: SecretRef::Stored { id: 9 },
            newline: false,
        },
    ];
    let json = steps_to_json(&steps).expect("writes");
    assert_eq!(
        json,
        r#"[{"step":"send_secret","source":{"kind":"connection_password"},"newline":true},{"step":"send_secret","source":{"kind":"stored","id":9},"newline":false}]"#
    );
}

#[test]
fn the_secrets_a_script_sends_are_listed_once_in_order() {
    let secret = |source| Step::SendSecret {
        source,
        newline: true,
    };
    let script = LoginScript {
        steps: vec![
            secret(SecretRef::Stored { id: 2 }),
            wait("x"),
            secret(SecretRef::ConnectionPassword),
            secret(SecretRef::Stored { id: 2 }),
        ],
        ..LoginScript::default()
    };
    assert_eq!(
        script.secret_refs(),
        [SecretRef::Stored { id: 2 }, SecretRef::ConnectionPassword]
    );
}

#[test]
fn every_invalid_code_is_listed_once() {
    let mut codes = ScriptInvalid::ALL_CODES.to_vec();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), ScriptInvalid::ALL_CODES.len());
    for error in [
        ScriptInvalid::TooManySteps { max: 1 },
        ScriptInvalid::TextTooLong { step: 0, max: 1 },
        ScriptInvalid::TimeoutInvalid { step: None, max: 1 },
        ScriptInvalid::SecretWithoutPrompt { step: 0 },
        ScriptInvalid::Pattern {
            step: None,
            error: PatternError::Empty,
        },
        ScriptInvalid::SchemaUnknown { version: 2 },
        ScriptInvalid::Malformed {
            reason: String::new(),
        },
    ] {
        assert!(ScriptInvalid::ALL_CODES.contains(&error.code()), "{error}");
    }
}

/// RC-080-12 (X5): a secret goes only to the prompt a `wait_for` step right before it waited for;
/// typed blindly it would land on the command line, in the history, on the screen.
#[test]
fn a_secret_is_sent_only_right_after_a_wait_for() {
    let secret = || Step::SendSecret {
        source: SecretRef::ConnectionPassword,
        newline: true,
    };
    let text = || Step::SendText {
        text: "sudo -i".into(),
        newline: true,
    };
    for (steps, step) in [
        (vec![secret()], 0),
        (vec![text(), secret()], 1),
        (vec![wait("(?i)password.*:"), Step::SendLine, secret()], 2),
        (vec![wait("(?i)password.*:"), secret(), secret()], 2),
    ] {
        let script = LoginScript {
            steps,
            ..LoginScript::default()
        };
        assert_eq!(
            script.validate(),
            Err(ScriptInvalid::SecretWithoutPrompt { step })
        );
    }
    let prompted = LoginScript {
        steps: vec![text(), wait("(?i)password.*:"), secret(), wait("#")],
        ..LoginScript::default()
    };
    prompted.validate().expect("a secret after its prompt");
}
