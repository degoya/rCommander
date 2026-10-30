//! The terminal in a panel through `apply`, without a window: the toggle, the keys a terminal
//! keeps from the keymap, the session bar, the shell picker, the paste question and the end of
//! a shell. Tests that start a shell run on Unix (`sh`), as rc-engine's.

use rc_engine::{PanelId, PanelView, TermId, VfsPath};
use rc_term_widget::Event as WidgetEvent;
use tempfile::TempDir;

use super::{Picker, TermMessage};
use crate::{
    app::{Message, harness::Harness, terminal_view::paste_question},
    i18n::t,
    term,
};

fn root() -> TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir(root.path().join("sub")).expect("mkdir");
    root
}

/// RC-080-53: a one-line paste with a control character is asked about its control characters,
/// not as "1 lines"; one with line breaks counts them.
#[tokio::test]
async fn a_one_line_paste_with_a_control_character_asks_about_it() {
    let root = root();
    let mut harness = Harness::headless(root.path()).await;
    let pasted = WidgetEvent::Paste(TermId(7), "sleep 9\u{3}".into());
    harness
        .send(Message::Terminal(TermMessage::Widget(pasted)))
        .await;
    let (_, text) = harness.app.terms.paste.as_ref().expect("asked");
    assert_eq!(paste_question(text), t!("terminal.paste_control"));
    assert_eq!(
        paste_question("ls\nrm -rf sub"),
        t!("terminal.paste_question", lines = 2)
    );
}

#[tokio::test]
async fn the_paste_question_takes_every_key_until_answered() {
    let root = root();
    let mut harness = Harness::headless(root.path()).await;
    let pasted = WidgetEvent::Paste(TermId(7), "ls\nrm -rf sub".into());
    harness
        .send(Message::Terminal(TermMessage::Widget(pasted)))
        .await;
    assert!(harness.app.terms.paste.is_some());
    for keys in ["Tab", "ArrowDown", "Ctrl+[Backquote]", "Shift+Enter"] {
        harness.press(keys).await;
    }
    assert_eq!(
        harness.app.active,
        PanelId::Left,
        "Tab went to the question"
    );
    assert_eq!(harness.app.pane(PanelId::Left).state.view, PanelView::Files);
    assert!(harness.app.terms.paste.is_some());
    harness.press("Escape").await;
    assert!(harness.app.terms.paste.is_none());
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Right);
}

/// A server panel's terminal is a shell on that server: the local shells are never offered.
#[tokio::test]
async fn a_server_panel_opens_its_terminal_without_the_shell_picker() {
    let root = root();
    let mut harness = Harness::headless(root.path()).await;
    harness.app.pane_mut(PanelId::Left).state.path = VfsPath {
        backend: rc_vfs::BackendId::Sftp { connection_id: 4 },
        path: "/home/me".into(),
    };
    harness
        .send(Message::Terminal(TermMessage::Choose(PanelId::Left)))
        .await;
    assert_eq!(harness.app.terms.picker, None);
}

/// Ctrl+Shift+O-umlaut asks for the shell when there is more than one; the picker takes the
/// keys.
#[tokio::test]
async fn ctrl_shift_backquote_opens_the_shell_picker() {
    if term::shells().len() < 2 {
        // One shell: Ctrl+Shift+O-umlaut starts it without asking (and needs a shell to
        // test).
        return;
    }
    let root = root();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Shift+[Semicolon]").await;
    assert_eq!(
        harness.app.terms.picker,
        Some(Picker {
            panel: PanelId::Left,
            selected: 0
        })
    );
    harness.press("ArrowDown").await;
    harness.press("Tab").await;
    assert_eq!(
        harness.app.terms.picker.map(|picker| picker.selected),
        Some(1)
    );
    assert_eq!(harness.app.active, PanelId::Left);
    harness.press("Escape").await;
    assert_eq!(harness.app.terms.picker, None);
    assert!(
        harness
            .app
            .engine()
            .panel_terminals(PanelId::Left)
            .is_empty()
    );
}

#[cfg(unix)]
mod with_a_shell {
    use std::time::Duration;

    use rc_engine::{Chord, Command};

    use super::*;
    use crate::app::Effect;

    fn chord(keys: &str) -> Chord {
        keys.parse().expect("a chord")
    }

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

    fn view(harness: &Harness, panel: PanelId) -> PanelView {
        harness.app.pane(panel).state.view
    }

    fn sh() -> usize {
        term::shells()
            .iter()
            .position(|profile| profile.id == "sh")
            .expect("sh on a Unix PATH")
    }

    /// Picks `sh` for `panel` as the bar's `+` does; the new session's id.
    async fn open_sh(harness: &mut Harness, panel: PanelId) -> TermId {
        harness
            .send(Message::Terminal(TermMessage::Choose(panel)))
            .await;
        harness
            .send(Message::Terminal(TermMessage::Pick(sh())))
            .await;
        harness
            .app
            .engine()
            .shown_terminal(panel)
            .expect("the panel shows it")
    }

    #[tokio::test]
    async fn the_toggle_switches_and_the_terminal_keeps_the_panel_keys() {
        let root = root();
        let mut harness = Harness::headless(root.path()).await;
        harness.press("Ctrl+[Semicolon]").await;
        assert_eq!(view(&harness, PanelId::Left), PanelView::Terminal);
        assert_eq!(harness.terminals_shown, [PanelId::Left]);
        let tabs = harness.app.engine().panel_terminals(PanelId::Left);
        assert_eq!(tabs.len(), 1);
        assert_eq!(
            tabs[0].cwd,
            root.path(),
            "the shell starts in the panel's folder"
        );
        let session = tabs[0].terminal;

        // Through `apply` (what `update` runs): no panel key reaches the keymap.
        for keys in [
            "Tab",
            "F5",
            "Ctrl+C",
            "ArrowDown",
            "ArrowUp",
            "Enter",
            "Backspace",
            "Ctrl+Tab",
            "Ctrl+Shift+Tab",
            "Ctrl+Shift+[Semicolon]",
            "Ctrl+Shift+[Backquote]",
        ] {
            let effects = harness.app.apply(Message::Key(chord(keys)));
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::Dispatch(..))),
                "{keys} reached the keymap"
            );
        }
        assert_eq!(harness.app.active, PanelId::Left);
        assert_eq!(harness.app.terms.picker, None);
        assert_eq!(view(&harness, PanelId::Right), PanelView::Files);

        harness.press("Ctrl+[Backquote]").await;
        assert_eq!(view(&harness, PanelId::Left), PanelView::Files);
        harness.press("Tab").await;
        assert_eq!(
            harness.app.active,
            PanelId::Right,
            "Tab is the panels' again"
        );
        harness.press("Tab").await;
        harness.press("Ctrl+[Backquote]").await;
        assert_eq!(view(&harness, PanelId::Left), PanelView::Terminal);
        assert_eq!(
            harness.app.engine().shown_terminal(PanelId::Left),
            Some(session),
            "the same shell"
        );
    }

    #[tokio::test]
    async fn two_terminals_in_a_panel_switch_and_close_one_by_one() {
        let root = root();
        let mut harness = Harness::headless(root.path()).await;
        let first = open_sh(&mut harness, PanelId::Right).await;
        let second = open_sh(&mut harness, PanelId::Right).await;
        assert_ne!(first, second);
        assert_eq!(view(&harness, PanelId::Right), PanelView::Terminal);
        assert_eq!(view(&harness, PanelId::Left), PanelView::Files);
        harness
            .send(Message::Terminal(TermMessage::Select(
                PanelId::Right,
                first,
            )))
            .await;
        assert_eq!(
            harness.app.engine().shown_terminal(PanelId::Right),
            Some(first)
        );
        harness
            .send(Message::Terminal(TermMessage::Close(PanelId::Right, first)))
            .await;
        assert_eq!(
            harness.app.engine().shown_terminal(PanelId::Right),
            Some(second)
        );
        assert_eq!(view(&harness, PanelId::Right), PanelView::Terminal);
        harness
            .send(Message::Terminal(TermMessage::Close(
                PanelId::Right,
                second,
            )))
            .await;
        until(&mut harness, "the files after the last close", |harness| {
            view(harness, PanelId::Right) == PanelView::Files
        })
        .await;
    }

    /// `exit` in the last session: the panel shows its files, the exit code in its status
    /// line, and the window closes the ended session.
    #[tokio::test]
    async fn exit_in_the_last_shell_returns_to_the_files() {
        let root = root();
        let mut harness = Harness::headless(root.path()).await;
        // As an MCP client opens one: it appears in the panel it names.
        harness
            .app
            .engine()
            .clone()
            .dispatch(Command::OpenTerminal {
                panel: Some(PanelId::Right),
                shell: Some("sh".into()),
                distro: None,
                cwd: Some("sub".into()),
                size: None,
            })
            .await
            .expect("opens");
        harness.settle().await;
        assert_eq!(view(&harness, PanelId::Right), PanelView::Terminal);
        let terminal = harness
            .app
            .engine()
            .shown_terminal(PanelId::Right)
            .expect("shown");
        harness
            .app
            .engine()
            .clone()
            .dispatch(Command::SendInput {
                terminal,
                text: "exit 3\r".into(),
            })
            .await
            .expect("types");
        until(&mut harness, "the files after exit", |harness| {
            view(harness, PanelId::Right) == PanelView::Files
                && harness
                    .app
                    .engine()
                    .panel_terminals(PanelId::Right)
                    .is_empty()
        })
        .await;
        assert_eq!(
            harness.app.pane(PanelId::Right).error.as_deref(),
            Some("The shell ended with exit code 3.")
        );
        assert_eq!(view(&harness, PanelId::Left), PanelView::Files);
    }

    /// RC-080-16: Ctrl+Shift+A, which the terminal leaves to the window, lists the panel's
    /// sessions; Enter shows one, Delete closes the one under the cursor, Escape closes the list.
    #[tokio::test]
    async fn ctrl_shift_a_lists_the_sessions_and_switches_and_closes_them() {
        let root = root();
        let mut harness = Harness::headless(root.path()).await;
        let first = open_sh(&mut harness, PanelId::Left).await;
        let second = open_sh(&mut harness, PanelId::Left).await;
        assert_eq!(harness.app.active, PanelId::Left);
        harness.press("Ctrl+Shift+A").await;
        let popup = harness
            .app
            .tabs
            .popup
            .as_ref()
            .expect("the list of sessions");
        assert_eq!(popup.rows.len(), 2);
        assert_eq!(popup.cursor, 1, "on the session shown");
        harness.press("ArrowUp").await;
        harness.press("Enter").await;
        assert!(harness.app.tabs.popup.is_none());
        assert_eq!(
            harness.app.engine().shown_terminal(PanelId::Left),
            Some(first)
        );
        harness.press("Ctrl+Shift+A").await;
        harness.press("Escape").await;
        assert!(harness.app.tabs.popup.is_none());
        harness.press("Ctrl+Shift+A").await;
        harness.press("Delete").await;
        assert_eq!(
            harness.app.engine().shown_terminal(PanelId::Left),
            Some(second)
        );
        assert_eq!(harness.app.engine().panel_terminals(PanelId::Left).len(), 1);
    }
}
