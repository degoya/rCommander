use std::{
    io::Write,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use async_trait::async_trait;
use secrecy::SecretString;
use tokio::io::{AsyncReadExt as _, DuplexStream, duplex};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::login_script::{LoginScript, Pattern, SecretRef, Step, TEMPLATES};

const SECRET: &str = "hunter2-sudo-canary";

/// Answers every secret with [`SECRET`], or fails as told.
struct Secrets(Result<&'static str, SecretError>);

#[async_trait]
impl SecretSource for Secrets {
    async fn secret(&self, _: SecretRef) -> Result<SecretString, SecretError> {
        self.0.map(SecretString::from)
    }
}

fn wait(pattern: &str, timeout_ms: Option<u64>) -> Step {
    Step::WaitFor {
        pattern: Pattern::new(pattern).expect("pattern"),
        timeout_ms,
    }
}

fn script(steps: Vec<Step>) -> LoginScript {
    LoginScript {
        steps,
        ..LoginScript::default()
    }
}

/// Reads from the server's end until what arrived ends with `needle`.
async fn read_until(server: &mut DuplexStream, needle: &[u8]) -> Vec<u8> {
    let mut seen = Vec::new();
    let mut byte = [0_u8; 1];
    while !seen.ends_with(needle) {
        let read = tokio::io::AsyncReadExt::read(server, &mut byte)
            .await
            .expect("read");
        assert_ne!(read, 0, "the stream ended before {needle:?}");
        seen.push(byte[0]);
    }
    seen
}

async fn say(server: &mut DuplexStream, text: &str) {
    tokio::io::AsyncWriteExt::write_all(server, text.as_bytes())
        .await
        .expect("write");
    tokio::io::AsyncWriteExt::flush(server)
        .await
        .expect("flush");
}

/// Every log line of the test, at every level.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into()
    }
}

#[tokio::test]
async fn the_sudo_template_runs_to_the_end_and_its_secret_goes_only_into_the_stream() {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer({
            let captured = captured.clone();
            move || captured.clone()
        })
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let sudo = TEMPLATES
        .iter()
        .find(|template| template.id == "sudo_i")
        .expect("the sudo template")
        .script();
    let (mut ours, mut server) = duplex(4096);
    let mut events = Vec::new();
    let run = async {
        run_script_observed(
            &sudo,
            &mut ours,
            &Secrets(Ok(SECRET)),
            &CancellationToken::new(),
            &mut |event| events.push(event),
        )
        .await
    };
    let host = async {
        say(
            &mut server,
            "Last login: today\r\n\x1b[01;32muser@box\x1b[00m:~$ ",
        )
        .await;
        read_until(&mut server, b"sudo -i\r").await;
        say(&mut server, "[sudo] password for user: ").await;
        let typed = read_until(&mut server, b"\r").await;
        say(&mut server, "\r\nroot@box:~# ").await;
        typed
    };
    let (outcome, typed) = tokio::join!(run, host);

    assert_eq!(outcome.expect("runs"), ScriptOutcome::Completed);
    assert_eq!(typed, format!("{SECRET}\r").into_bytes());
    let log = captured.text();
    assert!(
        log.contains("login script step"),
        "the capture works: {log}"
    );
    assert!(!log.contains(SECRET), "{log}");
    let events = format!("{events:?}");
    assert!(!events.contains(SECRET), "{events}");
    assert!(events.contains("SecretSent { step: 2 }"), "{events}");
}

#[tokio::test]
async fn a_prompt_that_never_comes_times_out_at_its_step() {
    let (mut ours, mut server) = duplex(4096);
    let steps = vec![wait("(?i)password.*:", Some(50))];
    let run = async {
        run_script(
            &script(steps),
            &mut ours,
            &Secrets(Ok(SECRET)),
            &CancellationToken::new(),
        )
        .await
    };
    let host = say(&mut server, "Welcome\r\n$ ");
    let (outcome, ()) = tokio::join!(run, host);
    assert_eq!(outcome.expect("runs"), ScriptOutcome::TimedOut { step: 0 });
    drop(server);
}

#[tokio::test]
async fn the_whole_script_times_out_too() {
    let (mut ours, _server) = duplex(4096);
    let mut slow = script(vec![
        Step::SendLine,
        Step::Sleep { ms: 10 },
        wait("never", Some(60_000)),
    ]);
    slow.total_timeout_ms = 80;
    let outcome = run_script(
        &slow,
        &mut ours,
        &Secrets(Ok(SECRET)),
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(outcome.expect("runs"), ScriptOutcome::TimedOut { step: 2 });

    let mut sleepy = script(vec![Step::Sleep { ms: 5_000 }]);
    sleepy.total_timeout_ms = 30;
    let outcome = run_script(
        &sleepy,
        &mut ours,
        &Secrets(Ok(SECRET)),
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(outcome.expect("runs"), ScriptOutcome::TimedOut { step: 0 });
}

#[tokio::test]
async fn a_prompt_split_over_two_blocks_is_found() {
    let (mut ours, mut server) = duplex(4096);
    let run = async {
        run_script(
            &script(vec![wait("Password: $", Some(2_000))]),
            &mut ours,
            &Secrets(Ok(SECRET)),
            &CancellationToken::new(),
        )
        .await
    };
    let host = async {
        say(&mut server, "Pass").await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        say(&mut server, "word: ").await;
    };
    let (outcome, ()) = tokio::join!(run, host);
    assert_eq!(outcome.expect("runs"), ScriptOutcome::Completed);
}

#[tokio::test]
async fn colour_sequences_do_not_stop_a_match_even_split_between_blocks() {
    let (mut ours, mut server) = duplex(4096);
    let steps = vec![
        wait("Password: ", Some(2_000)),
        wait(r"root@box:~# $", Some(2_000)),
    ];
    let run = async {
        run_script(
            &script(steps),
            &mut ours,
            &Secrets(Ok(SECRET)),
            &CancellationToken::new(),
        )
        .await
    };
    let host = async {
        say(&mut server, "\x1b[1;31mPass\x1b[0mword\x1b[32m:\x1b[0m ").await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        say(&mut server, "\x1b]0;root@box\x07\x1b[1;3").await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        say(&mut server, "1mroot@box\x1b[0m:~# ").await;
    };
    let (outcome, ()) = tokio::join!(run, host);
    assert_eq!(outcome.expect("runs"), ScriptOutcome::Completed);
}

#[tokio::test]
async fn the_person_can_cancel_at_any_time() {
    let (mut ours, _server) = duplex(4096);
    let cancel = CancellationToken::new();
    let run = async {
        run_script(
            &script(vec![wait("never", Some(60_000))]),
            &mut ours,
            &Secrets(Ok(SECRET)),
            &cancel,
        )
        .await
    };
    let stop = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();
    };
    let (outcome, ()) = tokio::join!(run, stop);
    assert_eq!(outcome.expect("runs"), ScriptOutcome::Cancelled);

    let sleeping = run_script(
        &script(vec![Step::Sleep { ms: 50_000 }]),
        &mut ours,
        &Secrets(Ok(SECRET)),
        &cancel,
    )
    .await;
    assert_eq!(sleeping.expect("runs"), ScriptOutcome::Cancelled);
}

#[tokio::test]
async fn a_locked_vault_is_answered_before_anything_is_sent() {
    let (mut ours, mut server) = duplex(4096);
    let steps = vec![
        Step::SendText {
            text: "sudo -i".into(),
            newline: true,
        },
        wait("(?i)password.*:", None),
        Step::SendSecret {
            source: SecretRef::ConnectionPassword,
            newline: true,
        },
    ];
    let error = run_script(
        &script(steps),
        &mut ours,
        &Secrets(Err(SecretError::Locked)),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ScriptError::VaultLocked), "{error:?}");
    assert_eq!(error.code(), "vault_locked");
    drop(ours);
    let mut sent = Vec::new();
    server.read_to_end(&mut sent).await.expect("read");
    assert!(sent.is_empty(), "{sent:?}");
}

#[tokio::test]
async fn a_secret_not_stored_names_its_step() {
    let (mut ours, _server) = duplex(4096);
    let steps = vec![
        wait("(?i)password.*:", None),
        Step::SendSecret {
            source: SecretRef::Stored { id: 7 },
            newline: false,
        },
    ];
    let error = run_script(
        &script(steps),
        &mut ours,
        &Secrets(Err(SecretError::Missing)),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(error, ScriptError::SecretMissing { step: 1 }),
        "{error:?}"
    );
    assert_eq!(error.code(), "login_script_secret_missing");
}

#[tokio::test]
async fn a_closed_stream_ends_the_run_at_its_step() {
    let (mut ours, server) = duplex(4096);
    drop(server);
    let outcome = run_script(
        &script(vec![wait("x", None)]),
        &mut ours,
        &Secrets(Ok(SECRET)),
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(
        outcome.expect("runs"),
        ScriptOutcome::StreamClosed { step: 0 }
    );
}

#[tokio::test]
async fn an_invalid_script_is_refused() {
    let (mut ours, _server) = duplex(4096);
    let error = run_script(
        &script(vec![Step::Sleep { ms: 0 }]),
        &mut ours,
        &Secrets(Ok(SECRET)),
        &CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code(), "login_script_timeout_invalid");
}

#[test]
fn the_buffer_keeps_only_the_newest_output() {
    let mut output = Output::default();
    output.push(&vec![b'a'; BUFFER_LIMIT]);
    output.push(b"tail# ");
    assert_eq!(output.text.len(), BUFFER_LIMIT);
    assert_eq!(output.consumed, 6);
    let prompt = Pattern::new(r"#\s*$").expect("pattern");
    assert!(output.take_match(&prompt));
    assert_eq!(output.consumed, BUFFER_LIMIT + 6);
    assert!(output.text.is_empty());
    assert!(!output.take_match(&prompt), "a prompt matches once");
}

#[test]
fn every_own_code_is_listed() {
    assert_eq!(ScriptError::VaultLocked.code(), ScriptError::ALL_CODES[0]);
    assert_eq!(
        ScriptError::SecretMissing { step: 0 }.code(),
        ScriptError::ALL_CODES[1]
    );
    assert_eq!(
        ScriptError::SecretFailed {
            code: "vault_undecryptable"
        }
        .code(),
        "vault_undecryptable"
    );
}
