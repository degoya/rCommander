//! The host-key dialogs through `apply`, against an engine over a database: the question the
//! engine files opens the dialog, the keyboard starts on the safe button, a changed key takes no
//! yes and is replaced only through its own step.

use rc_engine::{EngineError, OfferedKey, PanelId};
use tempfile::TempDir;

use super::{Choice, HostKeyMsg, Step};
use crate::app::{Message, harness::Harness};

const ED25519: &str = include_str!("../../../rc-remote/tests/fixtures/known_hosts/ed25519.pub");
const OTHER: &str = include_str!("../../../rc-remote/tests/fixtures/known_hosts/other.pub");

const HOST: &str = "files.example.org";
const PORT: u16 = 2222;

fn key(pub_line: &str) -> OfferedKey {
    let mut fields = pub_line.split_whitespace();
    OfferedKey::from_openssh(fields.next().expect("type"), fields.next().expect("key"))
        .expect("a valid key")
}

/// A connection presents `pub_line`: the engine's verdict, and the app hears of the question.
async fn present(harness: &mut Harness, pub_line: &str) -> Result<(), EngineError> {
    let verdict = harness
        .app
        .engine()
        .check_host_key(HOST, PORT, key(pub_line))
        .await;
    harness.settle().await;
    verdict
}

/// The id of the question the dialog shows.
fn shown(harness: &Harness) -> u64 {
    harness
        .app
        .host_keys
        .shown()
        .map(|shown| shown.question.id)
        .expect("a dialog is open")
}

async fn choose(harness: &mut Harness, choice: Choice) {
    let id = shown(harness);
    harness
        .send(Message::HostKey(HostKeyMsg::Choose(id, choice)))
        .await;
}

/// A key stored through the first-sighting dialog.
async fn trusted(harness: &mut Harness, pub_line: &str) {
    let _ = present(harness, pub_line).await;
    choose(harness, Choice::TrustAndSave).await;
    assert!(harness.app.host_keys.shown().is_none());
}

#[tokio::test]
async fn an_unknown_key_opens_the_first_sighting_dialog_on_cancel() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;

    let verdict = present(&mut harness, ED25519).await;

    assert!(matches!(verdict, Err(EngineError::HostKeyUnknown { .. })));
    let dialogs = &harness.app.host_keys;
    let question = &dialogs.shown().expect("the dialog").question;
    assert_eq!((question.host.as_str(), question.port), (HOST, PORT));
    assert_eq!(
        dialogs.buttons(),
        [Choice::ConnectOnce, Choice::TrustAndSave, Choice::Cancel]
    );
    assert_eq!(dialogs.focused(), Some(Choice::Cancel));
    assert!(
        dialogs
            .shown()
            .is_some_and(|shown| shown.randomart.is_some())
    );

    // Enter alone cancels: nothing is stored, the next connection asks again.
    harness.press("Enter").await;
    assert!(harness.app.host_keys.shown().is_none());
    let info = harness.app.info.clone().expect("a status line");
    assert!(info.contains(HOST), "{info}");
    assert!(matches!(
        present(&mut harness, ED25519).await,
        Err(EngineError::HostKeyUnknown { .. })
    ));
}

#[tokio::test]
async fn trust_and_save_from_the_keyboard_stores_the_key() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = present(&mut harness, ED25519).await;

    harness.press("Shift+Tab").await;
    assert_eq!(harness.app.host_keys.focused(), Some(Choice::TrustAndSave));
    harness.press("Enter").await;

    assert!(harness.app.host_keys.shown().is_none());
    assert!(present(&mut harness, ED25519).await.is_ok());
    assert!(harness.app.host_keys.shown().is_none());
}

#[tokio::test]
async fn connect_once_lets_exactly_the_next_connection_through() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = present(&mut harness, ED25519).await;

    choose(&mut harness, Choice::ConnectOnce).await;

    assert!(harness.app.host_keys.shown().is_none());
    assert!(present(&mut harness, ED25519).await.is_ok());
    assert!(matches!(
        present(&mut harness, ED25519).await,
        Err(EngineError::HostKeyUnknown { .. })
    ));
    assert!(harness.app.host_keys.shown().is_some());
}

#[tokio::test]
async fn a_changed_key_takes_no_yes_and_escape_cancels_it() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    trusted(&mut harness, ED25519).await;

    let verdict = present(&mut harness, OTHER).await;

    assert!(matches!(verdict, Err(EngineError::HostKeyChanged { .. })));
    let dialogs = &harness.app.host_keys;
    assert_eq!(dialogs.buttons(), [Choice::Replace, Choice::Cancel]);
    assert_eq!(dialogs.focused(), Some(Choice::Cancel));

    // The first-sighting answers are not this dialog's: nothing is sent, the stored key stays.
    choose(&mut harness, Choice::TrustAndSave).await;
    choose(&mut harness, Choice::ConnectOnce).await;
    assert!(harness.app.host_keys.shown().is_some());
    assert!(!harness.app.host_keys.busy());

    harness.press("Escape").await;
    assert!(harness.app.host_keys.shown().is_none());
    assert!(present(&mut harness, ED25519).await.is_ok());
    assert!(matches!(
        present(&mut harness, OTHER).await,
        Err(EngineError::HostKeyChanged { .. })
    ));
}

#[tokio::test]
async fn only_the_replace_step_replaces_a_changed_key() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    trusted(&mut harness, ED25519).await;
    let _ = present(&mut harness, OTHER).await;

    // Replacing is not a button of the warning's first step.
    choose(&mut harness, Choice::ConfirmReplace).await;
    assert_eq!(harness.app.host_keys.step(), Step::Ask);

    choose(&mut harness, Choice::Replace).await;
    let dialogs = &harness.app.host_keys;
    assert_eq!(dialogs.step(), Step::ConfirmReplace);
    assert_eq!(dialogs.buttons(), [Choice::ConfirmReplace, Choice::Back]);
    assert_eq!(dialogs.focused(), Some(Choice::Back));

    // Escape goes back to the warning; so does Enter in the replace step, which starts on Back.
    harness.press("Escape").await;
    assert_eq!(harness.app.host_keys.step(), Step::Ask);
    choose(&mut harness, Choice::Replace).await;
    harness.press("Enter").await;
    assert_eq!(harness.app.host_keys.step(), Step::Ask);
    assert!(harness.app.host_keys.shown().is_some());

    choose(&mut harness, Choice::Replace).await;
    choose(&mut harness, Choice::ConfirmReplace).await;

    assert!(harness.app.host_keys.shown().is_none());
    assert!(present(&mut harness, OTHER).await.is_ok());
    assert!(matches!(
        present(&mut harness, ED25519).await,
        Err(EngineError::HostKeyChanged { .. })
    ));
}

/// While a dialog is open the panels do not move under it, and a click meant for a dialog that
/// is no longer shown answers nothing.
#[tokio::test]
async fn the_keys_belong_to_the_dialog_and_a_stale_click_is_dropped() {
    let root = TempDir::new().expect("temp dir");
    std::fs::create_dir(root.path().join("sub")).expect("mkdir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = present(&mut harness, ED25519).await;
    let before = harness.app.pane(PanelId::Left).cursor();

    harness.press("ArrowDown").await;
    harness.press("Tab").await;
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), before);
    assert_eq!(harness.app.active, PanelId::Left);

    let id = shown(&harness);
    harness
        .send(Message::HostKey(HostKeyMsg::Choose(
            id + 1,
            Choice::TrustAndSave,
        )))
        .await;
    assert_eq!(shown(&harness), id);
    assert!(!harness.app.host_keys.busy());
}

/// A window that missed the engine's events reads the open questions again.
#[tokio::test]
async fn missed_questions_are_read_again() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = harness
        .app
        .engine()
        .check_host_key(HOST, PORT, key(ED25519))
        .await;
    harness.miss_events();
    assert!(harness.app.host_keys.shown().is_none());

    harness.send(Message::EngineLagged).await;

    assert!(harness.app.host_keys.shown().is_some());
}
