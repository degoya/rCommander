use tempfile::TempDir;

use crate::test_support;
use crate::{Command, Engine, EngineError, Outcome, TermId, TermSize};

async fn engine() -> (TempDir, Engine) {
    test_support::engine(&[("sub/", ""), ("notes.txt", "n")]).await
}

fn code(result: Result<Outcome, EngineError>) -> &'static str {
    match result {
        Ok(outcome) => panic!("an error, got {outcome:?}"),
        Err(error) => error.code(),
    }
}

fn open(shell: Option<&str>, cwd: Option<&str>) -> Command {
    Command::OpenTerminal {
        panel: None,
        shell: shell.map(Into::into),
        distro: None,
        cwd: cwd.map(Into::into),
        size: None,
    }
}

#[tokio::test]
async fn an_unknown_terminal_is_terminal_not_found() {
    let (_root, engine) = engine().await;
    let missing = TermId(u64::MAX);
    for command in [
        Command::SendInput {
            terminal: missing,
            text: "x".into(),
        },
        Command::ReadScreen {
            terminal: missing,
            scrollback: 0,
        },
        Command::CloseTerminal { terminal: missing },
    ] {
        assert_eq!(code(engine.dispatch(command).await), "terminal_not_found");
    }
}

#[tokio::test]
async fn open_checks_the_folder_the_shell_and_the_size_before_starting_anything() {
    let (_root, engine) = engine().await;
    assert_eq!(
        code(engine.dispatch(open(None, Some("missing"))).await),
        "not_found"
    );
    assert_eq!(
        code(engine.dispatch(open(None, Some("notes.txt"))).await),
        "not_a_directory"
    );
    assert_eq!(
        code(engine.dispatch(open(Some("no-such-shell"), None)).await),
        "shell_not_found"
    );
    let empty = Command::OpenTerminal {
        panel: None,
        shell: None,
        distro: None,
        cwd: None,
        size: Some(TermSize { rows: 0, cols: 80 }),
    };
    assert_eq!(code(engine.dispatch(empty).await), "terminal_invalid_size");
    let Ok(Outcome::Terminals { terminals, shells }) =
        engine.dispatch(Command::ListTerminals {}).await
    else {
        panic!("a list");
    };
    assert!(terminals.is_empty(), "nothing was started");
    assert!(!shells.is_empty(), "there is always a default shell");
}

/// Without WSL (every platform but Windows) no WSL profile is listed and a WSL shell, asked for
/// by shell or by distribution, is `wsl_unavailable` - before the folder is looked at.
#[cfg(not(windows))]
#[tokio::test]
async fn without_wsl_there_is_no_wsl_shell() {
    let (_root, engine) = engine().await;
    for (shell, distro) in [
        (Some("wsl"), None),
        (Some("wsl"), Some("Ubuntu")),
        (None, Some("Ubuntu")),
    ] {
        let command = Command::OpenTerminal {
            panel: None,
            shell: shell.map(Into::into),
            distro: distro.map(Into::into),
            cwd: Some("missing".into()),
            size: None,
        };
        assert_eq!(
            code(engine.dispatch(command).await),
            "wsl_unavailable",
            "{shell:?} {distro:?}"
        );
    }
    let Ok(Outcome::Terminals { shells, .. }) = engine.dispatch(Command::ListTerminals {}).await
    else {
        panic!("a list");
    };
    assert!(
        shells
            .iter()
            .all(|shell| shell.id != "wsl" && shell.distro.is_none()),
        "{shells:?}"
    );
}

/// The owner's Windows test (2026-10-18): the toggle on a panel in `\\wsl.localhost\Debian\...`
/// opened PowerShell. Asked for no shell, a folder in a distribution's share takes the WSL way -
/// `wsl_unavailable` here, where there is no WSL - and not the default local shell, which would
/// have looked for the folder first (`not_found`). A local shell asked for still wins: it takes
/// the local way and looks for the folder.
#[cfg(not(windows))]
#[tokio::test]
async fn a_panel_in_a_wsl_share_opens_the_distributions_shell() {
    let (_root, engine) = engine().await;
    let panel = crate::PanelId::Left;
    for folder in [r"\\wsl.localhost\Debian\home\x", r"\\wsl$\debian\home\x"] {
        let start = crate::VfsPath::local(folder);
        let wsl = engine
            .open_terminal_in(panel, start.clone(), None, None, None)
            .await;
        assert_eq!(code(wsl), "wsl_unavailable", "{folder}");
        let picked = engine
            .open_terminal_in(panel, start, Some("pwsh".into()), None, None)
            .await;
        assert_eq!(code(picked), "not_found", "{folder}");
    }
}

#[cfg(unix)]
mod with_a_shell {
    use std::time::Duration;

    use super::*;
    use crate::test_support::expect_terminal;
    use crate::{EngineEvent, PanelId, SessionState, TerminalChange};

    /// The screen of `terminal` once a line is exactly `line`.
    async fn shows_line(engine: &Engine, terminal: TermId, line: &str) -> Vec<String> {
        shows(engine, terminal, |shown| shown == line, line).await
    }

    /// Waits for the shell's prompt, the first text on the screen. Typed before it, the input
    /// is echoed ahead of the prompt and the output lands on the prompt's line.
    async fn prompt(engine: &Engine, terminal: TermId) {
        shows(
            engine,
            terminal,
            |shown| !shown.trim().is_empty(),
            "a prompt",
        )
        .await;
    }

    async fn shows(
        engine: &Engine,
        terminal: TermId,
        wanted: impl Fn(&str) -> bool,
        what: &str,
    ) -> Vec<String> {
        for _ in 0..1000 {
            let Ok(Outcome::Screen { screen, .. }) = engine
                .dispatch(Command::ReadScreen {
                    terminal,
                    scrollback: 100,
                })
                .await
            else {
                panic!("a screen");
            };
            if screen.lines.iter().any(|shown| wanted(shown)) {
                return screen.lines;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{what:?} never appeared");
    }

    #[tokio::test]
    async fn a_shell_opens_in_the_panels_folder_takes_input_and_closes() {
        let (root, engine) = engine().await;
        let mut events = engine.subscribe();
        let opened = expect_terminal(
            engine
                .dispatch(Command::OpenTerminal {
                    panel: Some(PanelId::Right),
                    shell: Some("sh".into()),
                    distro: None,
                    cwd: Some("sub".into()),
                    size: None,
                })
                .await
                .expect("open"),
        );
        assert_eq!(opened.panel, PanelId::Right);
        assert_eq!(opened.shell, "sh");
        assert_eq!(opened.cwd, root.path().join("sub"));
        assert_eq!((opened.rows, opened.cols), (24, 80));
        assert_eq!(opened.state, SessionState::Running);
        assert_eq!(opened.notice, None);
        assert!(engine.terminal_session(opened.terminal).is_some());

        let terminal = opened.terminal;
        prompt(&engine, terminal).await;
        engine
            .dispatch(Command::SendInput {
                terminal,
                text: "printf 'in%s\\n' put\r".into(),
            })
            .await
            .expect("input");
        shows_line(&engine, terminal, "input").await;

        let Ok(Outcome::Terminals { terminals, .. }) =
            engine.dispatch(Command::ListTerminals {}).await
        else {
            panic!("a list");
        };
        assert_eq!(terminals.len(), 1);

        let closed = expect_terminal(
            engine
                .dispatch(Command::CloseTerminal { terminal })
                .await
                .expect("close"),
        );
        assert!(closed.state.is_exited(), "{closed:?}");
        assert!(engine.terminal_session(terminal).is_none());

        let mut changes = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let EngineEvent::Terminal { change, .. } = event {
                changes.push(change);
            }
        }
        assert_eq!(changes.first(), Some(&TerminalChange::Opened));
        assert_eq!(changes.last(), Some(&TerminalChange::Closed));
    }

    #[tokio::test]
    async fn an_ended_shell_is_listed_as_exited_until_closed() {
        let (_root, engine) = engine().await;
        let mut events = engine.subscribe();
        let terminal =
            expect_terminal(engine.dispatch(open(Some("sh"), None)).await.expect("open")).terminal;
        engine
            .dispatch(Command::SendInput {
                terminal,
                text: "exit 3\r".into(),
            })
            .await
            .expect("input");
        let exited = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(EngineEvent::Terminal {
                    change: TerminalChange::Exited,
                    ..
                }) = events.recv().await
                {
                    break;
                }
            }
        })
        .await;
        assert!(exited.is_ok(), "the engine announced the end");
        let Ok(Outcome::Terminals { terminals, .. }) =
            engine.dispatch(Command::ListTerminals {}).await
        else {
            panic!("a list");
        };
        assert_eq!(terminals[0].state, SessionState::Exited { code: Some(3) });
        assert_eq!(
            code(
                engine
                    .dispatch(Command::SendInput {
                        terminal,
                        text: "x".into(),
                    })
                    .await
            ),
            "terminal_exited"
        );
        engine
            .dispatch(Command::CloseTerminal { terminal })
            .await
            .expect("close");
    }
}
