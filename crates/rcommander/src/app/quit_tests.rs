//! F10 closes a dialog as Escape does and quits without one; the question before quitting with
//! unfinished jobs; `keys.f10` narrows the key or switches it off (owner, 2026-10-18; RC-080-17).

use std::{fs, path::Path, time::Duration};

use rc_db::Db;
use rc_engine::{Command, EngineEvent, settings::F10_KEY};
use serde_json::json;
use tempfile::TempDir;

use super::{F10Mode, QuitMsg, f10, f10_mode};
use crate::app::{Message, harness::Harness, settings::SettingsMessage};

/// The left panel on `root` with `abc.txt`, the right one on `out`, and a database.
async fn harness(root: &Path, db: Db) -> Harness {
    fs::write(root.join("abc.txt"), b"abc").expect("write");
    fs::create_dir(root.join("out")).expect("mkdir");
    let mut harness = Harness::open_on_db(root, &root.join("out"), db).await;
    harness.settle().await;
    harness
}

fn key(keys: &str) -> Message {
    Message::Key(keys.parse().expect("a chord"))
}

/// A few dialogs of each kind: a function key's, one with a text field, a popup, a window.
fn dialogs() -> Vec<(&'static str, Message)> {
    vec![
        ("new folder", key("F7")),
        ("find files", key("Alt+F7")),
        ("command palette", key("Ctrl+Shift+P")),
        ("queue", key("Ctrl+Shift+Q")),
        ("settings", Message::Settings(SettingsMessage::Open)),
    ]
}

fn quitting(harness: &Harness) -> bool {
    harness.app.quit.leaving || harness.app.quit.asking.is_some()
}

#[tokio::test]
async fn f10_closes_every_dialog_as_escape_does_and_quits_nothing() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), Db::open_in_memory().await.expect("db")).await;
    for (name, open) in dialogs() {
        for close in [Message::Key(f10()), Message::KeyCaptured(f10())] {
            harness.send(open.clone()).await;
            assert!(harness.app.dialog_open(), "{name} opens");
            harness.send(close.clone()).await;
            harness.settle().await;
            assert!(!harness.app.dialog_open(), "{name} closes on {close:?}");
            assert!(!quitting(&harness), "{name}: F10 only closed it");
        }
    }
}

#[tokio::test]
async fn f10_without_a_dialog_quits() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), Db::open_in_memory().await.expect("db")).await;
    harness.press("F10").await;
    harness.settle().await;
    assert!(harness.app.quit.leaving, "on the way out");
    assert_eq!(harness.app.quit.asking, None, "nothing to ask about");
}

/// In a terminal F10 is the shell's: the widget captured it, and without a dialog it stays so.
#[tokio::test]
async fn a_captured_f10_without_a_dialog_stays_the_terminals() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), Db::open_in_memory().await.expect("db")).await;
    harness.send(Message::KeyCaptured(f10())).await;
    harness.settle().await;
    assert!(!quitting(&harness));
}

/// Jobs that run or wait (the engine counts them, `commands/quit_tests.rs`): the question
/// comes first, Enter and Escape and F10 cancel it, Quit ends the app.
#[tokio::test]
async fn unfinished_jobs_ask_first_and_cancel_is_the_default() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), Db::open_in_memory().await.expect("db")).await;
    let wanted = Message::Engine(EngineEvent::Quit { unfinished: 2 });
    for cancel in [key("Enter"), key("Escape"), Message::Key(f10())] {
        harness.send(wanted.clone()).await;
        assert_eq!(harness.app.quit.asking, Some(2), "the question is open");
        assert!(harness.app.dialog_open(), "it is a dialog");
        harness.send(cancel.clone()).await;
        assert!(!quitting(&harness), "{cancel:?} cancels");
    }
    harness.send(wanted).await;
    harness.send(key("F7")).await;
    assert!(
        !harness.app.file_ops.modal(),
        "no key reaches the panels below"
    );
    harness.send(Message::Quit(QuitMsg::Confirm)).await;
    assert_eq!(harness.app.quit.asking, None);
    assert!(harness.app.quit.leaving, "Quit ends the app");
}

#[tokio::test]
async fn close_dialogs_keeps_f10_from_quitting_and_off_from_closing() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), Db::open_in_memory().await.expect("db")).await;

    harness
        .send(Message::Quit(QuitMsg::Mode(F10Mode::CloseDialogs)))
        .await;
    harness.press("F10").await;
    harness.settle().await;
    assert!(!quitting(&harness), "close_dialogs: no quit");
    harness.press("F7").await;
    harness.press("F10").await;
    assert!(
        !harness.app.dialog_open(),
        "close_dialogs: the dialog closes"
    );

    harness
        .send(Message::Quit(QuitMsg::Mode(F10Mode::Off)))
        .await;
    harness.press("F7").await;
    harness.press("F10").await;
    assert!(harness.app.dialog_open(), "off: the dialog stays");
    harness.press("Escape").await;
    harness.press("F10").await;
    harness.settle().await;
    assert!(!quitting(&harness), "off: no quit");
}

/// The setting as the window reads it, after a change through `settings.set` (the command an
/// agent's `settings_set` runs too).
#[tokio::test]
async fn the_stored_setting_reads_as_its_mode() {
    let root = TempDir::new().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let harness = harness(root.path(), db.clone()).await;
    assert_eq!(f10_mode(db.clone()).await, F10Mode::CloseAndQuit, "default");
    for (value, mode) in [
        ("close_dialogs", F10Mode::CloseDialogs),
        ("off", F10Mode::Off),
        ("close_and_quit", F10Mode::CloseAndQuit),
    ] {
        harness
            .app
            .engine()
            .dispatch(Command::SetSetting {
                key: F10_KEY.into(),
                value: json!(value),
            })
            .await
            .expect("stored");
        assert_eq!(f10_mode(db.clone()).await, mode, "{value}");
    }
}

/// A copy that waits for the answer to its conflict about `out/abc.txt`, started outside the
/// window (as an agent would), so no dialog of the window shows it.
async fn waiting_copy(harness: &mut Harness, root: &Path) {
    fs::write(root.join("out").join("abc.txt"), b"old").expect("write");
    let copy = Command::OpsCopy(rc_engine::TransferRequest {
        panel: Some(rc_engine::PanelId::Left),
        sources: vec!["abc.txt".into()],
        target: Some(root.join("out").to_string_lossy().into_owned()),
        options: rc_engine::JobOptions::default(),
    });
    let engine = harness.app.engine().clone();
    let Ok(rc_engine::Outcome::Job(job)) = engine.dispatch(copy).await else {
        panic!("a job");
    };
    for _ in 0..500 {
        if engine.ops().job(job.id).expect("known").question.is_some() {
            harness.settle().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {} asked nothing", job.id);
}

/// The main window's X and Alt+F4 (a close request) go the way of F10 (owner, 2026-10-18):
/// with a job waiting they ask; with the question switched off they end the app.
#[tokio::test]
async fn the_main_windows_close_request_asks_while_a_job_waits() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), Db::open_in_memory().await.expect("db")).await;
    waiting_copy(&mut harness, root.path()).await;
    assert!(
        !harness.app.dialog_open(),
        "the window follows no job of its own"
    );
    let main = iced::window::Id::unique();
    harness.send(Message::CloseRequested(main)).await;
    harness.settle().await;
    assert_eq!(harness.app.quit.asking, Some(1), "it asks");
    assert!(!harness.app.quit.leaving);
    harness.press("Escape").await;

    let off = Command::SetSetting {
        key: rc_engine::settings::CONFIRM_QUIT_KEY.into(),
        value: json!(false),
    };
    harness.app.engine().dispatch(off).await.expect("stored");
    harness.send(Message::CloseRequested(main)).await;
    harness.settle().await;
    assert_eq!(harness.app.quit.asking, None);
    assert!(harness.app.quit.leaving, "switched off: it ends");
}
