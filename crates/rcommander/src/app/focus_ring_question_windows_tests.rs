//! The focus ring of the small questions in windows of their own (RC-080-16, RC-080-23): each
//! question drawn as its window draws it walks its ring without the head's X - the title bar has
//! it - and its window starts on its first control. The MCP call's window starts on Deny, its one
//! stop; Allow stays a button only a click presses.

use std::time::Duration;

use iced::{widget::Id, window};
use rc_engine::{Command, CommandClass, EngineEvent, McpAsk, PanelId};

use super::probe::{Screen, harness, host_key_question, kinds, walk_round};
use crate::app::harness::headless_engine;
use crate::{
    app::{
        Message,
        archive_ops::{ArchiveDialog, PasswordPrompt, Typed},
        dialog_windows::first_field,
        harness::Harness,
        mcp_confirm,
    },
    theme::focus::Kind,
};

/// The top question's window, drawn as the window draws it: its stops and its buttons out of the
/// ring, after the keyboard went to its first control.
async fn question_ring(harness: &mut Harness, question: &str) -> (Vec<Option<Kind>>, usize) {
    harness.app.dialog_windows.own = true;
    let kind = *harness.app.wanted_windows().last().expect("a window");
    assert!(kind.is_question(), "{question}: {kind:?}");
    let id = window::Id::unique();
    let books = &mut harness.app.dialog_windows;
    let geometry = books.geometry(kind, harness.app.window.geometry());
    books.opened(kind, id, geometry);
    let mut screen = Screen::new(harness.app.window_view(id)).await;
    let stops = walk_round(&mut screen, question);
    if let Some(field) = first_field(kind) {
        screen.focus(field);
        if field == mcp_confirm::DENY_ID {
            assert_eq!(screen.at(), Some(0), "{question}: Deny has the keyboard");
        } else {
            assert_eq!(
                screen.focused_id(),
                Some(Id::new(field)),
                "{question}: its first control has the keyboard"
            );
        }
    }
    (kinds(&stops), screen.loose_buttons())
}

#[tokio::test]
async fn the_question_windows_walk_their_rings_without_a_head() {
    let (_root, mut harness) = harness().await;
    host_key_question(&mut harness).await;
    let (stops, loose) = question_ring(&mut harness, "host key window").await;
    assert_eq!(stops, [Some(Kind::Button); 3]);
    assert_eq!(loose, 0, "the title bar has the X");

    let (_root, mut harness) = self::harness().await;
    harness
        .send(Message::Engine(EngineEvent::Quit { unfinished: 1 }))
        .await;
    let (stops, loose) = question_ring(&mut harness, "quit window").await;
    assert_eq!(stops, [Some(Kind::Button); 2], "Cancel and Quit");
    assert_eq!(loose, 0);

    let (_root, mut harness) = self::harness().await;
    harness
        .send(Message::Engine(EngineEvent::VaultUnused))
        .await;
    let (stops, loose) = question_ring(&mut harness, "vault offer window").await;
    assert_eq!(stops, [Some(Kind::Button); 2], "Remove and Keep");
    assert_eq!(loose, 0);

    let (_root, mut harness) = self::harness().await;
    let refused = vec![Command::ListDrives { refresh: false }];
    harness
        .send(Message::VaultRefused(PanelId::Left, refused))
        .await;
    let (stops, loose) = question_ring(&mut harness, "unlock window").await;
    assert_eq!(stops[0], None, "the password field first");
    assert_eq!(loose, 0);

    let (_root, mut harness) = self::harness().await;
    harness.app.archive = Some(ArchiveDialog::Password(PasswordPrompt {
        job: 1,
        archive: "a.zip".to_owned(),
        wrong: false,
        typed: Typed::default(),
    }));
    let (stops, loose) = question_ring(&mut harness, "archive password window").await;
    assert_eq!(stops[0], None, "the password field first");
    assert_eq!(loose, 0);
}

/// The MCP call's window has Deny as its one stop and the keyboard on it as it opens; Allow is
/// out of the ring, so no key typed as the window appears allows the call.
#[tokio::test]
async fn the_mcp_question_window_starts_on_deny() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    engine.attach_window();
    let mut harness = Harness::new(engine);
    let engine = harness.app.engine().clone();
    let ask = McpAsk {
        tool: "terminal_open".to_owned(),
        class: CommandClass::Exec,
        arguments: Vec::new(),
        panel: None,
        cursor: false,
        session: None,
    };
    let _call = tokio::spawn(async move { engine.ask_mcp(ask, Duration::from_secs(30)).await });
    while harness.app.engine().mcp_questions().is_empty() {
        tokio::task::yield_now().await;
    }
    harness.settle().await;
    let (stops, loose) = question_ring(&mut harness, "MCP window").await;
    assert_eq!(stops, [Some(Kind::Button)], "Deny is the one stop");
    assert_eq!(loose, 1, "Allow, which only a click presses; no head");
}
