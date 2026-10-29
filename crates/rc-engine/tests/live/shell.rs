//! The shell side of a connection: the `sudo -i` login script ends in a root shell on a real
//! `sudo`; "Run command" (`ssh.exec`) answers the output and the exit code.

use std::time::{Duration, Instant};

use rc_engine::{
    Command, Engine, LoginRunState, LoginScriptDraft, Outcome, SshExecRequest, TermId, TermSize,
    login_script::TEMPLATES,
};

use crate::live::{connect_trusting, connection, engine, workdir};
use crate::live_or_skip;

fn screen(engine: &Engine, terminal: TermId) -> String {
    let screen = engine
        .terminal_session(terminal)
        .expect("the session")
        .screen(200);
    [screen.scrollback, screen.lines].concat().join("\n")
}

async fn shows(engine: &Engine, terminal: TermId, needle: &str) {
    let started = Instant::now();
    loop {
        let text = screen(engine, terminal);
        if text.contains(needle) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{needle:?} never showed:\n{text}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn the_sudo_login_script_ends_in_a_root_shell() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let steps = TEMPLATES
        .iter()
        .find(|template| template.id == "sudo_i")
        .expect("the sudo -i template")
        .script()
        .steps;
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

    let opened = engine
        .dispatch(Command::OpenSshTerminal {
            connection_id: id,
            panel: None,
            cwd: None,
            size: Some(TermSize {
                rows: 30,
                cols: 100,
            }),
        })
        .await;
    let Ok(Outcome::Terminal(info)) = opened else {
        panic!("a terminal, got {opened:?}");
    };
    let started = Instant::now();
    let run = loop {
        let Ok(Outcome::Terminals { terminals, .. }) =
            engine.dispatch(Command::ListTerminals {}).await
        else {
            panic!("the terminals");
        };
        let run = terminals
            .into_iter()
            .find(|terminal| terminal.terminal == info.terminal)
            .and_then(|terminal| terminal.login_script)
            .expect("a run");
        if run.state.is_finished() {
            break run;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "still {run:?}:\n{}",
            screen(&engine, info.terminal)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let text = screen(&engine, info.terminal);
    assert_eq!(run.state, LoginRunState::Completed, "{text}");
    assert!(!text.contains(&live.password), "{text}");
    engine
        .terminal_session(info.terminal)
        .expect("the session")
        .write(b"echo uid=$(id -u)\r".to_vec())
        .expect("typed");
    shows(&engine, info.terminal, "uid=0").await;
    eprintln!("{}", screen(&engine, info.terminal));
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn run_command_answers_output_and_exit_code() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let ran = engine
        .dispatch(Command::SshExec(SshExecRequest {
            connection_id: id,
            command: "echo out; echo err >&2; whoami; exit 3".to_owned(),
            stdin: None,
            timeout_secs: Some(30),
            max_output_bytes: None,
        }))
        .await;
    let Ok(Outcome::SshExec(output)) = ran else {
        panic!("an exec, got {ran:?}");
    };
    assert_eq!(output.stdout, format!("out\n{}\n", live.user));
    assert_eq!(output.stderr, "err\n");
    assert_eq!(output.exit_code, Some(3));
    assert!(!output.timed_out);

    let with_input = engine
        .dispatch(Command::SshExec(SshExecRequest {
            connection_id: id,
            command: "wc -c".to_owned(),
            stdin: Some("12345".to_owned()),
            timeout_secs: Some(30),
            max_output_bytes: None,
        }))
        .await;
    let Ok(Outcome::SshExec(output)) = with_input else {
        panic!("an exec, got {with_input:?}");
    };
    assert_eq!(output.stdout.trim(), "5");
    assert_eq!(output.exit_code, Some(0));
}
