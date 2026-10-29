//! RC-040-04 through the engine. The shell is `sh` with a prompt that carries the OSC 133 mark
//! (`PS1` set by a typed line), so the rules run against a real shell without any integration
//! of the app's own.

use tempfile::TempDir;

use crate::test_support;
use crate::test_support::run_panel;
use crate::{Command, Engine, PanelId, PanelSnapshot};

async fn engine() -> (TempDir, Engine) {
    test_support::engine(&[("a/", ""), ("b/", ""), ("sub/", "")]).await
}

async fn state(engine: &Engine, panel: PanelId) -> PanelSnapshot {
    run_panel(engine, Command::GetPanelState { panel: Some(panel) }).await
}

#[tokio::test]
async fn a_panel_showing_its_files_reports_no_terminal() {
    let (_root, engine) = engine().await;
    let snapshot = state(&engine, PanelId::Left).await;
    assert_eq!(snapshot.terminal, None);
    let json = serde_json::to_value(&snapshot).expect("json");
    assert!(json.get("terminal").is_none(), "{json}");
}

#[cfg(unix)]
mod with_a_shell {
    use std::time::Duration;

    use rc_db::Db;
    use serde_json::json;

    use super::*;
    use crate::test_support::expect_terminal;
    use crate::{FollowStatus, Outcome, PanelView, PromptState, TermId, VfsPath};

    async fn type_line(engine: &Engine, terminal: TermId, line: &str) {
        engine
            .dispatch(Command::SendInput {
                terminal,
                text: format!("{line}\r"),
            })
            .await
            .expect("input");
    }

    /// Waits until `check` holds for the left panel's state, for at most ten seconds.
    async fn until(engine: &Engine, check: impl Fn(&PanelSnapshot) -> bool, what: &str) {
        for _ in 0..1000 {
            if check(&state(engine, PanelId::Left).await) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{what} never happened");
    }

    /// Waits until the shell's screen has a line that is exactly `line`.
    async fn shows(engine: &Engine, terminal: TermId, line: &str) {
        for _ in 0..1000 {
            if let Ok(Outcome::Screen { screen, .. }) = engine
                .dispatch(Command::ReadScreen {
                    terminal,
                    scrollback: 100,
                })
                .await
                && screen.lines.iter().any(|shown| shown == line)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{line:?} never appeared");
    }

    #[tokio::test]
    async fn the_shell_follows_the_panel_at_its_prompt_and_the_panel_follows_the_shell() {
        let (root, engine) = engine().await;
        let folder = |name: &str| root.path().join(name).to_string_lossy().into_owned();
        let terminal = expect_terminal(
            engine
                .dispatch(Command::OpenTerminal {
                    panel: Some(PanelId::Left),
                    shell: Some("sh".into()),
                    distro: None,
                    cwd: None,
                    size: None,
                })
                .await
                .expect("open"),
        )
        .terminal;
        until(
            &engine,
            |s| s.view == PanelView::Terminal,
            "the terminal view",
        )
        .await;
        let prompt = |s: &PanelSnapshot| s.terminal.as_ref().map(|t| t.prompt);

        // No prompt marks: navigating types nothing, the status line says why.
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Left),
                path: "a".into(),
            })
            .await
            .expect("navigate");
        let left = state(&engine, PanelId::Left).await;
        assert_eq!(prompt(&left), Some(PromptState::Unknown));
        assert_eq!(
            left.terminal.and_then(|t| t.follow),
            Some(FollowStatus::NoIntegration)
        );

        // A prompt with the mark: the waiting cd goes out at the first one.
        type_line(&engine, terminal, r#"PS1="$(printf '\033]133;A\007$ ')""#).await;
        until(
            &engine,
            |s| s.terminal.as_ref().is_some_and(|t| t.follow.is_none()),
            "the cd",
        )
        .await;
        type_line(&engine, terminal, "pwd").await;
        shows(&engine, terminal, &folder("a")).await;

        // A program runs: the cd waits for the prompt after it.
        type_line(&engine, terminal, "sleep 1").await;
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Left),
                path: "../b".into(),
            })
            .await
            .expect("navigate");
        let left = state(&engine, PanelId::Left).await;
        assert_eq!(
            left.terminal.and_then(|t| t.follow),
            Some(FollowStatus::Running)
        );
        until(
            &engine,
            |s| s.terminal.as_ref().is_some_and(|t| t.follow.is_none()),
            "the late cd",
        )
        .await;
        type_line(&engine, terminal, "pwd").await;
        shows(&engine, terminal, &folder("b")).await;

        // The shell reports another folder: turning to the files shows it.
        type_line(
            &engine,
            terminal,
            r#"cd ../sub && printf '\033]7;file://localhost%s\007' "$PWD""#,
        )
        .await;
        until(
            &engine,
            |s| s.terminal.as_ref().and_then(|t| t.folder.clone()) == Some(root.path().join("sub")),
            "the reported folder",
        )
        .await;
        let shown = engine
            .dispatch(Command::ShowFiles {
                panel: Some(PanelId::Left),
            })
            .await
            .expect("files");
        let Outcome::Panel(shown) = shown else {
            panic!("a panel");
        };
        assert_eq!(shown.view, PanelView::Files);
        assert_eq!(shown.path, VfsPath::local(root.path().join("sub")));

        engine
            .dispatch(Command::CloseTerminal { terminal })
            .await
            .expect("close");
    }

    #[tokio::test]
    async fn a_network_share_a_local_shell_reports_never_moves_the_panel() {
        let (root, engine) = engine().await;
        let terminal = expect_terminal(
            engine
                .dispatch(Command::OpenTerminal {
                    panel: Some(PanelId::Left),
                    shell: Some("sh".into()),
                    distro: None,
                    cwd: None,
                    size: None,
                })
                .await
                .expect("open"),
        )
        .terminal;
        until(
            &engine,
            |s| s.view == PanelView::Terminal,
            "the terminal view",
        )
        .await;
        // A share as OSC 9;9 and as OSC 7 (RC-080-10); the line after them shows they were read.
        type_line(
            &engine,
            terminal,
            r"printf '\033]9;9;\\\\attacker\\share\007\033]7;file:////attacker/share\007done\n'",
        )
        .await;
        shows(&engine, terminal, "done").await;
        let left = state(&engine, PanelId::Left).await;
        assert_eq!(left.terminal.and_then(|t| t.folder), None);
        let shown = engine
            .dispatch(Command::ShowFiles {
                panel: Some(PanelId::Left),
            })
            .await
            .expect("files");
        let Outcome::Panel(shown) = shown else {
            panic!("a panel");
        };
        assert_eq!(shown.path, VfsPath::local(root.path()));
        engine
            .dispatch(Command::CloseTerminal { terminal })
            .await
            .expect("close");
    }

    #[tokio::test]
    async fn open_terminal_here_starts_in_the_panels_folder_in_either_panel() {
        let (root, engine) = engine().await;
        assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
        for (key, value) in [
            ("terminal.default_shell", json!("sh")),
            ("terminal.shell_integration", json!(false)),
        ] {
            engine
                .dispatch(Command::SetSetting {
                    key: key.into(),
                    value,
                })
                .await
                .expect("setting");
        }
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Left),
                path: "a".into(),
            })
            .await
            .expect("navigate");
        let here = expect_terminal(
            engine
                .dispatch(Command::OpenTerminalHere {
                    panel: None,
                    other: false,
                })
                .await
                .expect("here"),
        );
        assert_eq!((here.panel, here.shell.as_str()), (PanelId::Left, "sh"));
        assert_eq!(here.cwd, root.path().join("a"));
        let other = expect_terminal(
            engine
                .dispatch(Command::OpenTerminalHere {
                    panel: None,
                    other: true,
                })
                .await
                .expect("other"),
        );
        assert_eq!(other.panel, PanelId::Right);
        assert_eq!(
            other.cwd,
            root.path().join("a"),
            "the active panel's folder"
        );
        let right = state(&engine, PanelId::Right).await;
        assert_eq!(right.view, PanelView::Terminal);
        assert_eq!(
            right.terminal.map(|t| (t.terminal, t.prompt)),
            Some((other.terminal, PromptState::Unknown))
        );
        for terminal in [here.terminal, other.terminal] {
            engine
                .dispatch(Command::CloseTerminal { terminal })
                .await
                .expect("close");
        }
    }
}
