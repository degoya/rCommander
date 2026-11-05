use rc_vault::{ExposeSecret as _, SecretString};
use serde_json::json;

use super::{LoginRunState, LoginScriptRun, Prefetched};
use crate::{LoginScriptId, ScriptSecretRef, SecretError, SecretSource as _};

#[test]
fn a_run_reads_as_its_script_and_a_state() {
    let run = LoginScriptRun {
        script: LoginScriptId(4),
        name: "to root".into(),
        steps: 4,
        state: LoginRunState::TimedOut { step: 1 },
    };
    assert_eq!(
        serde_json::to_value(&run).expect("json"),
        json!({ "script": 4, "name": "to root", "steps": 4, "state": "timed_out", "step": 1 })
    );
    assert!(run.state.is_finished());
    assert!(!LoginRunState::Running { step: 0 }.is_finished());
}

#[tokio::test]
async fn prefetched_secrets_answer_as_they_were_read() {
    let secrets = Prefetched(vec![
        (
            ScriptSecretRef::ConnectionPassword,
            Ok(SecretString::from("canary-4711")),
        ),
        (ScriptSecretRef::Stored { id: 2 }, Err(SecretError::Missing)),
    ]);
    let password = secrets
        .secret(ScriptSecretRef::ConnectionPassword)
        .await
        .expect("read");
    assert_eq!(password.expose_secret(), "canary-4711");
    assert_eq!(
        secrets
            .secret(ScriptSecretRef::Stored { id: 2 })
            .await
            .err(),
        Some(SecretError::Missing)
    );
    assert_eq!(
        secrets
            .secret(ScriptSecretRef::Stored { id: 3 })
            .await
            .err(),
        Some(SecretError::Missing),
        "one the script does not send"
    );
}

/// RC-080-65: a tap that fell behind and was cut off ends the run as a failure with its own
/// code, not as the shell's end; a run that got through stays one.
#[test]
fn a_tap_cut_off_ends_the_run_as_a_failure() {
    use rc_remote::login_script::ScriptOutcome;

    use super::finished_state;
    let closed = || Ok(ScriptOutcome::StreamClosed { step: 1 });
    assert_eq!(
        finished_state(closed(), true),
        LoginRunState::Failed {
            code: "login_script_output_overflow".into()
        }
    );
    assert_eq!(
        finished_state(closed(), false),
        LoginRunState::StreamClosed { step: 1 }
    );
    assert_eq!(
        finished_state(Ok(ScriptOutcome::Completed), true),
        LoginRunState::Completed
    );
}
