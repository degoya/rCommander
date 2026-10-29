use tempfile::TempDir;

use crate::test_support;
use crate::{
    Command, Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, PanelView, TermId,
};

async fn engine() -> (TempDir, Engine) {
    test_support::engine(&[("sub/", "")]).await
}

fn code(result: Result<Outcome, EngineError>) -> &'static str {
    match result {
        Ok(outcome) => panic!("an error, got {outcome:?}"),
        Err(error) => error.code(),
    }
}

#[tokio::test]
async fn show_files_needs_no_terminal_and_announces_the_view() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    let Ok(Outcome::Panel(snapshot)) = engine
        .dispatch(Command::ShowFiles {
            panel: Some(PanelId::Right),
        })
        .await
    else {
        panic!("a panel");
    };
    assert_eq!(
        (snapshot.panel, snapshot.view),
        (PanelId::Right, PanelView::Files)
    );
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::Panel {
            panel: PanelId::Right,
            change: PanelChange::View
        }
    );
    assert_eq!(engine.shown_terminal(PanelId::Right), None);
    assert!(engine.panel_terminals(PanelId::Right).is_empty());
    let missing = Command::ShowTerminal {
        terminal: TermId(u64::MAX),
    };
    assert_eq!(code(engine.dispatch(missing).await), "terminal_not_found");
}

#[cfg(unix)]
mod with_a_shell {
    use std::time::Duration;

    use super::*;
    use crate::test_support::expect_terminal;
    use crate::{SessionState, VfsPath};

    async fn open_sh(engine: &Engine, panel: PanelId) -> TermId {
        let outcome = engine
            .dispatch(Command::OpenTerminal {
                panel: Some(panel),
                shell: Some("sh".into()),
                distro: None,
                cwd: None,
                size: None,
            })
            .await
            .expect("open");
        let info = expect_terminal(outcome);
        assert!(info.shown, "a new terminal is shown at once");
        info.terminal
    }

    async fn toggle(engine: &Engine, panel: PanelId) -> PanelView {
        let toggled = Command::ToggleTerminal { panel: Some(panel) };
        test_support::run_panel(engine, toggled).await.view
    }

    /// Waits until `panel` shows `view` with `shown` as its session.
    async fn until_shows(engine: &Engine, panel: PanelId, view: PanelView, shown: Option<TermId>) {
        for _ in 0..1000 {
            if engine.panel(panel).view == view && engine.shown_terminal(panel) == shown {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "{panel:?} shows {:?} with {:?}, not {view:?} with {shown:?}",
            engine.panel(panel).view,
            engine.shown_terminal(panel)
        );
    }

    #[tokio::test]
    async fn the_toggle_starts_a_shell_in_the_panels_folder_and_keeps_it() {
        let (root, engine) = engine().await;
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Left),
                path: "sub".into(),
            })
            .await
            .expect("navigates");
        assert_eq!(toggle(&engine, PanelId::Left).await, PanelView::Terminal);
        let tabs = engine.panel_terminals(PanelId::Left);
        assert_eq!(tabs.len(), 1, "{tabs:?}");
        assert_eq!(tabs[0].cwd, root.path().join("sub"));
        assert!(tabs[0].shown);
        let first = tabs[0].terminal;
        assert_eq!(engine.panel(PanelId::Right).view, PanelView::Files);

        assert_eq!(toggle(&engine, PanelId::Left).await, PanelView::Files);
        let session = engine.terminal_session(first).expect("still open");
        assert_eq!(
            session.state(),
            SessionState::Running,
            "files hide the shell"
        );
        assert_eq!(toggle(&engine, PanelId::Left).await, PanelView::Terminal);
        assert_eq!(engine.shown_terminal(PanelId::Left), Some(first));
        assert_eq!(
            engine.panel_terminals(PanelId::Left).len(),
            1,
            "no second shell"
        );
    }

    /// `terminal_open` from MCP names a panel: that panel shows it, the other keeps its files.
    #[tokio::test]
    async fn an_opened_terminal_appears_in_its_panel() {
        let (_root, engine) = engine().await;
        let terminal = open_sh(&engine, PanelId::Right).await;
        assert_eq!(engine.panel(PanelId::Right).view, PanelView::Terminal);
        assert_eq!(engine.shown_terminal(PanelId::Right), Some(terminal));
        assert_eq!(engine.panel(PanelId::Left).view, PanelView::Files);
        assert!(engine.panel_terminals(PanelId::Left).is_empty());
    }

    #[tokio::test]
    async fn two_terminals_of_a_panel_switch_and_close_one_by_one() {
        let (_root, engine) = engine().await;
        let first = open_sh(&engine, PanelId::Left).await;
        let second = open_sh(&engine, PanelId::Left).await;
        let tabs: Vec<(TermId, bool)> = engine
            .panel_terminals(PanelId::Left)
            .iter()
            .map(|tab| (tab.terminal, tab.shown))
            .collect();
        assert_eq!(tabs, [(first, false), (second, true)]);

        engine
            .dispatch(Command::ShowTerminal { terminal: first })
            .await
            .expect("shows");
        assert_eq!(engine.shown_terminal(PanelId::Left), Some(first));
        engine
            .dispatch(Command::ShowFiles { panel: None })
            .await
            .expect("files");
        assert_eq!(engine.shown_terminal(PanelId::Left), Some(first), "kept");

        engine
            .dispatch(Command::CloseTerminal { terminal: first })
            .await
            .expect("closes");
        assert_eq!(engine.shown_terminal(PanelId::Left), Some(second));
        assert_eq!(engine.panel(PanelId::Left).view, PanelView::Files);
        assert_eq!(toggle(&engine, PanelId::Left).await, PanelView::Terminal);
        engine
            .dispatch(Command::CloseTerminal { terminal: second })
            .await
            .expect("closes");
        until_shows(&engine, PanelId::Left, PanelView::Files, None).await;
    }

    /// The owner's Windows test (2026-10-18): a PowerShell tab opened on `C:\` was shown again
    /// after the panel went to `\\wsl.localhost\debian\home`. The toggle reuses only a shell of
    /// the folder's system; for the WSL share it starts the distribution's shell (here, without
    /// WSL, `wsl_unavailable`) and keeps the local tab. Back on a local folder, the local shell
    /// is shown again and nothing new starts.
    #[tokio::test]
    async fn the_toggle_does_not_reuse_a_local_shell_in_a_wsl_share() {
        let (root, engine) = engine().await;
        let panel = PanelId::Left;
        let local = open_sh(&engine, panel).await;
        assert_eq!(toggle(&engine, panel).await, PanelView::Files);

        engine.state().panel_mut(panel).path = VfsPath::local(r"\\wsl.localhost\debian\home");
        let toggled = engine
            .dispatch(Command::ToggleTerminal { panel: Some(panel) })
            .await;
        assert_eq!(
            code(toggled),
            "wsl_unavailable",
            "a WSL shell was asked for"
        );
        let tabs = engine.panel_terminals(panel);
        assert_eq!(tabs.len(), 1, "{tabs:?}");
        assert_eq!(tabs[0].terminal, local);
        assert_eq!(tabs[0].state, SessionState::Running, "the old tab stays");
        assert_eq!(engine.panel(panel).view, PanelView::Files);

        engine.state().panel_mut(panel).path = VfsPath::local(root.path());
        assert_eq!(toggle(&engine, panel).await, PanelView::Terminal);
        assert_eq!(engine.shown_terminal(panel), Some(local));
        assert_eq!(engine.panel_terminals(panel).len(), 1, "no second shell");
    }

    #[tokio::test]
    async fn exit_in_the_last_shell_turns_the_panel_to_its_files() {
        let (_root, engine) = engine().await;
        let first = open_sh(&engine, PanelId::Left).await;
        let second = open_sh(&engine, PanelId::Left).await;
        let exit = |terminal| Command::SendInput {
            terminal,
            text: "exit\r".into(),
        };
        engine.dispatch(exit(second)).await.expect("input");
        until_shows(&engine, PanelId::Left, PanelView::Terminal, Some(first)).await;
        engine.dispatch(exit(first)).await.expect("input");
        until_shows(&engine, PanelId::Left, PanelView::Files, None).await;
        let shown = Command::ShowTerminal { terminal: first };
        assert_eq!(code(engine.dispatch(shown).await), "terminal_exited");
    }

    /// `terminal.maximize` (RC-040-05) on a panel showing its files turns it to its terminal -
    /// the same session - and then asks the window; on a panel showing its terminal it only
    /// asks, nothing new starts.
    #[tokio::test]
    async fn maximize_turns_to_the_terminal_then_asks_the_window() {
        let (_root, engine) = engine().await;
        let panel = PanelId::Left;
        let shell = open_sh(&engine, panel).await;
        assert_eq!(toggle(&engine, panel).await, PanelView::Files);
        let mut events = engine.subscribe();
        let maximize = || Command::MaximizeTerminal { panel: None };
        let Ok(Outcome::Panel(snapshot)) = engine.dispatch(maximize()).await else {
            panic!("a panel");
        };
        assert_eq!(
            (snapshot.panel, snapshot.view),
            (panel, PanelView::Terminal)
        );
        assert_eq!(engine.shown_terminal(panel), Some(shell), "the same shell");
        let view = EngineEvent::Panel {
            panel,
            change: PanelChange::View,
        };
        let asked = EngineEvent::MaximizeTerminal { panel };
        assert_eq!(announced(&mut events), [view, asked.clone()]);

        engine.dispatch(maximize()).await.expect("maximizes");
        assert_eq!(announced(&mut events), [asked]);
        assert_eq!(engine.panel_terminals(panel).len(), 1, "no second shell");
    }

    /// The view changes and the window's requests announced so far; a session's own events
    /// (its folder, its follow status) are left out.
    fn announced(events: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> Vec<EngineEvent> {
        std::iter::from_fn(|| events.try_recv().ok())
            .filter(|event| {
                matches!(
                    event,
                    EngineEvent::Panel { .. } | EngineEvent::MaximizeTerminal { .. }
                )
            })
            .collect()
    }
}
