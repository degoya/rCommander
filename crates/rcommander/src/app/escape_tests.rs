//! Escape cancels every dialog (owner, 2026-10-18): each dialog kind the keys open, closed once
//! by the X in its head (Escape) and once by an Escape a text field captured.

use std::{fs, path::Path};

use rc_engine::{Command, CursorTarget, PanelId};
use tempfile::TempDir;

use super::escape;
use crate::app::{
    Message, database_dialog::DatabaseMsg, harness::Harness, settings::SettingsMessage,
};

/// The left panel on `root` with `abc.txt` under the cursor, the right one on `out`, and a
/// database for the dialogs that read one.
async fn harness(root: &Path) -> Harness {
    fs::write(root.join("abc.txt"), b"abc").expect("write");
    fs::create_dir(root.join("out")).expect("mkdir");
    let mut harness = Harness::open_with_db(root, &root.join("out")).await;
    harness
        .app
        .engine()
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Left),
            target: CursorTarget::Name("abc.txt".into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
    harness
}

fn key(keys: &str) -> Message {
    Message::Key(keys.parse().expect("a chord"))
}

/// Every dialog kind and what opens it.
fn dialogs() -> Vec<(&'static str, Message)> {
    vec![
        ("copy", key("F5")),
        ("move", key("F6")),
        ("new folder", key("F7")),
        ("delete", key("F8")),
        ("find files", key("Alt+F7")),
        ("quick connection", key("Ctrl+N")),
        ("connection manager", key("Ctrl+F")),
        ("command palette", key("Ctrl+Shift+P")),
        ("queue", key("Ctrl+Shift+Q")),
        ("properties", key("Alt+Enter")),
        ("context menu", key("Shift+F10")),
        ("synchronize", key("Alt+Shift+F2")),
        ("checksums", key("Ctrl+Shift+F9")),
        ("pack", key("Alt+F5")),
        ("settings", Message::Settings(SettingsMessage::Open)),
        ("database", Message::Database(DatabaseMsg::Open)),
    ]
}

#[tokio::test]
async fn escape_and_the_close_x_close_every_dialog_also_from_a_text_field() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    let captured = Message::KeyCaptured(escape());
    for (name, open) in dialogs() {
        for close in [Message::close_dialog(), captured.clone()] {
            harness.send(open.clone()).await;
            assert!(harness.app.dialog_open(), "{name} opens");
            harness.send(close.clone()).await;
            harness.settle().await;
            assert!(!harness.app.dialog_open(), "{name} closes on {close:?}");
        }
    }
}

/// Without a dialog a captured Escape stays captured: it was a terminal's or the command line's.
#[tokio::test]
async fn a_captured_escape_without_a_dialog_stays_captured() {
    let root = TempDir::new().expect("temp dir");
    let harness = harness(root.path()).await;
    assert!(!harness.app.dialog_open());
    let message = harness.app.escape_to_dialog(Message::KeyCaptured(escape()));
    assert!(matches!(message, Message::KeyCaptured(_)));
}
