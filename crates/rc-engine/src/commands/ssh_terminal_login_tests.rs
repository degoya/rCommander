//! Login scripts on an SSH terminal against the shell server's pretend `sudo`: a root shell from
//! the vault's password that never reaches a log, an event or the screen; a prompt that never
//! comes; Ctrl+C; a locked vault from the window and from MCP; a secret that is not stored.

use std::{
    io::Write,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use rc_remote::{
    login_script::TEMPLATES,
    testkit::{PASSWORD, SUDO_PROMPT},
};

use super::tests::{Harness, eventually, harness, open_ssh, opened, screen, shows};
use crate::{
    Command, ConnectionId, Engine, EngineEvent, LoginRunState, LoginScriptDraft, LoginScriptRun,
    Outcome, Pattern, ScriptSecretRef, Step, TermId, TerminalChange,
};

/// Every log line of a test, at every level.
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

/// `steps` as a script that connection `id` runs.
async fn assign(engine: &Engine, id: ConnectionId, steps: Vec<Step>) {
    let Ok(Outcome::LoginScript { script }) = engine
        .dispatch(Command::CreateLoginScript {
            script: LoginScriptDraft {
                name: "to root".into(),
                steps,
                step_timeout_ms: None,
                total_timeout_ms: None,
            },
        })
        .await
    else {
        panic!("a script");
    };
    engine
        .dispatch(Command::AssignLoginScript {
            connection_id: id,
            script_id: Some(script.id),
        })
        .await
        .expect("assigned");
}

fn sudo_i() -> Vec<Step> {
    TEMPLATES
        .iter()
        .find(|template| template.id == "sudo_i")
        .expect("the sudo template")
        .script()
        .steps
}

fn wait_in_vain(timeout_ms: u64) -> Vec<Step> {
    vec![Step::WaitFor {
        pattern: Pattern::new("never-printed").expect("pattern"),
        timeout_ms: Some(timeout_ms),
    }]
}

/// The run of `terminal` once it is over.
async fn finished(engine: &Engine, terminal: TermId) -> LoginScriptRun {
    let started = std::time::Instant::now();
    loop {
        let Ok(Outcome::Terminals { terminals, .. }) =
            engine.dispatch(Command::ListTerminals {}).await
        else {
            panic!("the terminals");
        };
        let run = terminals
            .into_iter()
            .find(|info| info.terminal == terminal)
            .and_then(|info| info.login_script)
            .expect("a run");
        if run.state.is_finished() {
            return run;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "still {run:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn type_in(harness: &Harness, terminal: TermId, text: &str) -> Option<&'static str> {
    let session = harness.engine.terminal_session(terminal).expect("session");
    session
        .write(text.as_bytes().to_vec())
        .err()
        .map(|error| error.code())
}

#[tokio::test]
async fn sudo_from_the_vault_ends_in_a_root_shell_and_the_password_goes_nowhere_else() {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer({
            let captured = captured.clone();
            move || captured.clone()
        })
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let harness = harness().await;
    assign(&harness.engine, harness.id, sudo_i()).await;
    let mut events = harness.engine.subscribe();
    let info = opened(
        harness
            .engine
            .dispatch(open_ssh(harness.id, Some("/root/my files")))
            .await,
    );
    let run = info.login_script.expect("the run");
    assert_eq!(run.name, "to root");
    assert_eq!(run.steps, 4);
    assert!(matches!(run.state, LoginRunState::Running { .. }));

    let run = finished(&harness.engine, info.terminal).await;
    assert_eq!(run.state, LoginRunState::Completed);
    shows(&harness.engine, info.terminal, "root@test:~#").await;
    eventually("the cd came after the script", || {
        harness.server.cwd().as_deref() == Some("/root/my files")
    })
    .await;
    assert_eq!(
        harness.server.lines(),
        ["sudo -i", "<password>", "cd '/root/my files'"]
    );
    assert_eq!(type_in(&harness, info.terminal, "whoami\r"), None);
    shows(&harness.engine, info.terminal, "\nroot\n").await;

    let text = screen(&harness.engine, info.terminal);
    assert!(text.contains(SUDO_PROMPT.trim_end()), "{text}");
    assert!(!text.contains(PASSWORD), "{text}");
    assert!(
        harness
            .engine
            .scrub_secrets(&format!("echo {PASSWORD}"))
            .is_some(),
        "the MCP mask knows the password"
    );
    let log = captured.text();
    assert!(log.contains("login script"), "the capture works: {log}");
    assert!(!log.contains(PASSWORD), "{log}");
    let seen: Vec<EngineEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(seen.iter().any(|event| matches!(
        event,
        EngineEvent::Terminal {
            change: TerminalChange::LoginScript,
            ..
        }
    )));
    let seen = format!("{seen:?}");
    assert!(!seen.contains(PASSWORD), "{seen}");
}

#[tokio::test]
async fn a_prompt_that_never_comes_times_out_visibly_and_the_shell_stays_usable() {
    let harness = harness().await;
    assign(&harness.engine, harness.id, wait_in_vain(1500)).await;
    let info = opened(
        harness
            .engine
            .dispatch(open_ssh(harness.id, Some("/srv")))
            .await,
    );
    assert_eq!(
        type_in(&harness, info.terminal, "ls\r"),
        Some("terminal_busy"),
        "the person waits for the script"
    );
    let run = finished(&harness.engine, info.terminal).await;
    assert_eq!(run.state, LoginRunState::TimedOut { step: 0 });
    assert_eq!(type_in(&harness, info.terminal, "whoami\r"), None);
    shows(&harness.engine, info.terminal, "\ntester\n").await;
    assert_eq!(
        harness.server.lines(),
        ["whoami"],
        "no cd after a failed run"
    );
    assert_eq!(harness.server.cwd(), None);
}

#[tokio::test]
async fn ctrl_c_cancels_the_script() {
    let harness = harness().await;
    assign(&harness.engine, harness.id, wait_in_vain(60_000)).await;
    let info = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    assert_eq!(
        type_in(&harness, info.terminal, "\x03"),
        Some("terminal_busy")
    );
    let run = finished(&harness.engine, info.terminal).await;
    assert_eq!(run.state, LoginRunState::Cancelled);
    assert_eq!(type_in(&harness, info.terminal, "whoami\r"), None);
    shows(&harness.engine, info.terminal, "\ntester\n").await;
}

#[tokio::test]
async fn a_locked_vault_stops_the_open_and_only_the_window_is_asked() {
    let harness = harness().await;
    assign(&harness.engine, harness.id, sudo_i()).await;
    harness
        .engine
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    let asked = |events: &mut tokio::sync::broadcast::Receiver<EngineEvent>| {
        std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. }))
    };

    let mut events = harness.engine.subscribe();
    let from_mcp =
        crate::read_guard::as_mcp(harness.engine.dispatch(open_ssh(harness.id, None))).await;
    assert_eq!(
        from_mcp.err().map(|error| error.code()),
        Some("vault_locked")
    );
    assert!(
        !asked(&mut events),
        "an agent's call opens no unlock dialog"
    );

    let from_window = harness.engine.dispatch(open_ssh(harness.id, None)).await;
    assert_eq!(
        from_window.err().map(|error| error.code()),
        Some("vault_locked")
    );
    assert!(asked(&mut events), "the window asks for the vault");
    assert_eq!(harness.server.accepts(), 0, "nothing reached the server");
}

#[tokio::test]
async fn a_secret_that_is_not_stored_fails_the_run_and_leaves_the_shell() {
    let harness = harness().await;
    assign(
        &harness.engine,
        harness.id,
        vec![
            Step::WaitFor {
                pattern: Pattern::new("(?i)password.*:").expect("pattern"),
                timeout_ms: None,
            },
            Step::SendSecret {
                source: ScriptSecretRef::Stored { id: 99 },
                newline: true,
            },
        ],
    )
    .await;
    let info = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    let run = finished(&harness.engine, info.terminal).await;
    assert_eq!(
        run.state,
        LoginRunState::Failed {
            code: "login_script_secret_missing".into()
        }
    );
    assert_eq!(type_in(&harness, info.terminal, "whoami\r"), None);
    shows(&harness.engine, info.terminal, "\ntester\n").await;
}
