//! The `send_totp` step (RC-060-08): the code of the moment the step runs, checked before the
//! run like every secret.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use secrecy::SecretString;
use tokio::io::{AsyncReadExt as _, DuplexStream, duplex};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::login_script::{LoginScript, Pattern, ScriptInvalid, SecretRef, Step};

/// Hands out a new code at every ask, or fails as told; no other secret.
struct Codes {
    fail: Option<SecretError>,
    asked: AtomicUsize,
}

impl Codes {
    fn giving() -> Self {
        Self {
            fail: None,
            asked: AtomicUsize::new(0),
        }
    }

    fn failing(error: SecretError) -> Self {
        Self {
            fail: Some(error),
            asked: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl SecretSource for Codes {
    async fn secret(&self, _: SecretRef) -> Result<SecretString, SecretError> {
        Err(SecretError::Missing)
    }

    async fn totp_code(&self) -> Result<SecretString, SecretError> {
        let asked = self.asked.fetch_add(1, Ordering::SeqCst);
        match self.fail {
            Some(error) => Err(error),
            None => Ok(SecretString::from((111_111 * (asked + 1)).to_string())),
        }
    }
}

fn code_after_prompt(newline: bool) -> LoginScript {
    LoginScript {
        steps: vec![
            Step::WaitFor {
                pattern: Pattern::new("(?i)verification code: ").expect("pattern"),
                timeout_ms: Some(2_000),
            },
            Step::SendTotp { newline },
        ],
        ..LoginScript::default()
    }
}

async fn read_until(server: &mut DuplexStream, needle: &[u8]) -> Vec<u8> {
    let mut seen = Vec::new();
    let mut byte = [0_u8; 1];
    while !seen.ends_with(needle) {
        let read = tokio::io::AsyncReadExt::read(server, &mut byte)
            .await
            .expect("read");
        assert_ne!(read, 0, "the stream ended before {needle:?}");
        seen.extend_from_slice(&byte);
    }
    seen
}

#[tokio::test]
async fn the_code_of_the_moment_the_step_runs_is_sent() {
    let (mut ours, mut server) = duplex(4096);
    let codes = Codes::giving();
    let mut events = Vec::new();
    let script = code_after_prompt(true);
    let run = async {
        run_script_observed(
            &script,
            &mut ours,
            &codes,
            &CancellationToken::new(),
            &mut |event| events.push(event),
        )
        .await
    };
    let host = async {
        tokio::io::AsyncWriteExt::write_all(&mut server, b"Verification code: ")
            .await
            .expect("write");
        read_until(&mut server, b"\r").await
    };
    let (outcome, typed) = tokio::join!(run, host);
    assert_eq!(outcome.expect("runs"), ScriptOutcome::Completed);
    // The first code only showed one could be had; the step made its own.
    assert_eq!(typed, b"222222\r".to_vec());
    assert_eq!(codes.asked.load(Ordering::SeqCst), 2);
    let events = format!("{events:?}");
    assert!(events.contains("SecretSent { step: 1 }"), "{events}");
    assert!(!events.contains("222222"), "{events}");
}

#[tokio::test]
async fn a_missing_seed_names_its_step_and_a_locked_vault_sends_nothing() {
    let (mut ours, mut server) = duplex(4096);
    let error = run_script(
        &code_after_prompt(false),
        &mut ours,
        &Codes::failing(SecretError::Missing),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(error, ScriptError::SecretMissing { step: 1 }),
        "{error:?}"
    );
    let error = run_script(
        &code_after_prompt(false),
        &mut ours,
        &Codes::failing(SecretError::Locked),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code(), "vault_locked");
    drop(ours);
    let mut sent = Vec::new();
    server.read_to_end(&mut sent).await.expect("read");
    assert!(sent.is_empty(), "{sent:?}");
}

#[test]
fn a_code_is_sent_only_right_after_a_prompt() {
    let blind = LoginScript {
        steps: vec![Step::SendTotp { newline: true }],
        ..LoginScript::default()
    };
    assert_eq!(
        blind.validate(),
        Err(ScriptInvalid::SecretWithoutPrompt { step: 0 })
    );
    assert_eq!(code_after_prompt(true).validate(), Ok(()));
    assert!(code_after_prompt(true).sends_totp());
}

#[test]
fn the_step_reads_and_writes_as_send_totp_with_enter_by_default() {
    let step: Step = serde_json::from_str(r#"{"step":"send_totp"}"#).expect("reads");
    assert_eq!(step, Step::SendTotp { newline: true });
    let json = serde_json::to_string(&Step::SendTotp { newline: false }).expect("writes");
    assert_eq!(json, r#"{"step":"send_totp","newline":false}"#);
}
