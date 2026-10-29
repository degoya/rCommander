//! The terminal over the whole window through `apply`, without a window (RC-040-05): the
//! toggle, and the two panels back when the terminal leaves the panel or the panel the keyboard.
//! Tests that start a shell run on Unix (`sh`), as `terminal_pane_tests.rs`.

use rc_engine::{EngineEvent, PanelId};

use crate::app::{Message, harness::Harness};

/// A panel showing its files is never laid over the other, and only the active panel is.
#[tokio::test]
async fn only_the_active_panel_showing_its_terminal_is_maximized() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    harness.app.terms.maximized = Some(PanelId::Left);
    assert_eq!(harness.app.maximized(), None, "the panel shows its files");

    harness.app.terms.maximized = None;
    harness
        .send(Message::Engine(EngineEvent::MaximizeTerminal {
            panel: PanelId::Right,
        }))
        .await;
    assert_eq!(harness.app.active, PanelId::Left);
    assert_eq!(harness.app.terms.maximized, None, "not the active panel");
}

#[cfg(unix)]
mod with_a_shell {
    use std::time::Duration;

    use rc_engine::{Command, PanelView};
    use tempfile::TempDir;

    use super::*;
    use crate::app::terminal_pane::TermMessage;

    /// Settles until `done` holds; the shell runs on its own.
    async fn until(harness: &mut Harness, what: &str, done: impl Fn(&Harness) -> bool) {
        for _ in 0..1000 {
            harness.settle().await;
            if done(harness) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{what} never happened");
    }

    fn view(harness: &Harness) -> PanelView {
        harness.app.pane(PanelId::Left).state.view
    }

    fn root() -> TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    /// Ctrl+A-umlaut on the files opens the terminal and lays it over both panels; Ctrl+' (the
    /// same key on a US layout) from inside the terminal lays it back, and the session is the
    /// same throughout. Ctrl+O-umlaut to the files shows both panels, and back in the terminal
    /// it is not maximized again.
    #[tokio::test]
    async fn ctrl_ae_toggles_and_the_files_show_both_panels() {
        let root = root();
        let mut harness = Harness::headless(root.path()).await;
        harness.press("Ctrl+\u{e4}").await;
        assert_eq!(view(&harness), PanelView::Terminal);
        assert_eq!(harness.app.maximized(), Some(PanelId::Left));
        let session = harness.app.engine().shown_terminal(PanelId::Left);
        assert!(session.is_some());

        harness.press("Ctrl+'").await;
        assert_eq!(harness.app.maximized(), None, "the shell did not get it");
        assert_eq!(view(&harness), PanelView::Terminal);
        harness.press("Ctrl+\u{e4}").await;
        assert_eq!(harness.app.maximized(), Some(PanelId::Left));
        assert_eq!(
            harness.app.engine().shown_terminal(PanelId::Left),
            session,
            "the same shell"
        );
        assert_eq!(harness.app.engine().panel_terminals(PanelId::Left).len(), 1);

        harness.press("Ctrl+\u{f6}").await;
        assert_eq!(view(&harness), PanelView::Files);
        assert_eq!(harness.app.terms.maximized, None);
        harness.press("Ctrl+\u{f6}").await;
        assert_eq!(view(&harness), PanelView::Terminal);
        assert_eq!(harness.app.maximized(), None, "both panels until asked");
    }

    /// The maximized terminal's session closed: the panel shows its files, both panels again.
    #[tokio::test]
    async fn closing_the_maximized_terminal_shows_both_panels() {
        let root = root();
        let mut harness = Harness::headless(root.path()).await;
        harness.press("Ctrl+\u{e4}").await;
        assert_eq!(harness.app.maximized(), Some(PanelId::Left));
        let session = harness
            .app
            .engine()
            .shown_terminal(PanelId::Left)
            .expect("a session");
        harness
            .send(Message::Terminal(TermMessage::Close(
                PanelId::Left,
                session,
            )))
            .await;
        until(&mut harness, "the files after the close", |harness| {
            view(harness) == PanelView::Files
        })
        .await;
        assert_eq!(harness.app.maximized(), None);
        assert_eq!(harness.app.terms.maximized, None);
    }

    /// The other panel made active from elsewhere (as an MCP call does): both panels again, so
    /// the keyboard never goes to a hidden panel.
    #[tokio::test]
    async fn another_active_panel_shows_both_panels() {
        let root = root();
        let mut harness = Harness::headless(root.path()).await;
        harness.press("Ctrl+\u{e4}").await;
        assert_eq!(harness.app.maximized(), Some(PanelId::Left));
        harness
            .app
            .engine()
            .clone()
            .dispatch(Command::SwitchActivePanel {
                panel: Some(PanelId::Right),
            })
            .await
            .expect("switches");
        harness.settle().await;
        assert_eq!(harness.app.active, PanelId::Right);
        assert_eq!(harness.app.terms.maximized, None);
    }
}
