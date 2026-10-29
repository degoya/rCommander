//! The quick connection dialog through `apply`: Ctrl+N opens it, the keys belong to it, the
//! master password is asked only for a save that stores a secret, and the engine's answer
//! closes it or shows why not.

use rc_engine::{PanelId, VaultStateKind};
use rc_vault::SecretString;

use super::{QuickMsg, QuickPhase};
use crate::{
    app::{Effect, Message, harness::Harness},
    i18n::t,
};

fn apply(harness: &mut Harness, message: QuickMsg) -> Vec<Effect> {
    harness.app.apply(Message::QuickConnect(message))
}

fn fill(harness: &mut Harness) {
    for message in [
        QuickMsg::Host("files.example".to_owned()),
        QuickMsg::User("tester".to_owned()),
        QuickMsg::Password(SecretString::from("typed-secret")),
    ] {
        apply(harness, message);
    }
}

fn phase(harness: &Harness) -> QuickPhase {
    harness
        .app
        .quick
        .as_ref()
        .map(|dialog| dialog.phase.clone())
        .expect("the dialog is open")
}

#[tokio::test]
async fn ctrl_n_opens_the_dialog_for_the_active_panel_and_escape_closes_it() {
    let (_root, mut harness) = Harness::temp().await;
    harness.press("Ctrl+N").await;
    let dialog = harness.app.quick.as_ref().expect("open");
    assert_eq!(dialog.panel, harness.app.active);
    assert_eq!(dialog.phase, QuickPhase::Editing);
    harness.press("Escape").await;
    assert!(harness.app.quick.is_none());
}

/// The X in the card's head does what Escape does (RC-080-15).
#[tokio::test]
async fn the_close_x_closes_the_dialog_as_escape_does() {
    let (_root, mut harness) = Harness::temp().await;
    harness.press("Ctrl+N").await;
    assert!(harness.app.quick.is_some());
    harness.send(Message::close_dialog()).await;
    assert!(harness.app.quick.is_none());
}

#[tokio::test]
async fn connecting_without_saving_asks_no_master_password() {
    let (_root, mut harness) = Harness::temp().await;
    harness.press("Ctrl+N").await;
    fill(&mut harness);
    let effects = apply(&mut harness, QuickMsg::Confirm);
    assert!(matches!(effects.as_slice(), [Effect::Task(_)]), "one job");
    assert_eq!(phase(&harness), QuickPhase::Connecting);
    assert!(
        harness
            .app
            .quick
            .as_ref()
            .is_some_and(|dialog| dialog.vault.is_none())
    );
    apply(&mut harness, QuickMsg::Finished(Ok(())));
    assert!(
        harness.app.quick.is_none(),
        "done: the panel shows the server"
    );
}

#[tokio::test]
async fn saving_a_password_asks_for_the_master_password_first() {
    let (_root, mut harness) = Harness::temp().await;
    harness.press("Ctrl+N").await;
    fill(&mut harness);
    apply(&mut harness, QuickMsg::Save(true));
    apply(&mut harness, QuickMsg::Name("box".to_owned()));
    let effects = apply(&mut harness, QuickMsg::Confirm);
    assert!(
        effects.is_empty(),
        "nothing runs before the master password"
    );
    let dialog = harness.app.quick.as_ref().expect("open");
    // No vault exists in an engine without a database: it is created with the first secret.
    assert_eq!(dialog.vault, Some(VaultStateKind::Absent));
    apply(
        &mut harness,
        QuickMsg::Master(SecretString::from("correct horse battery")),
    );
    // A new master password is typed twice (RC-080-39): unequal, nothing runs.
    apply(
        &mut harness,
        QuickMsg::MasterRepeat(SecretString::from("correct horse")),
    );
    assert!(apply(&mut harness, QuickMsg::Confirm).is_empty());
    assert_eq!(
        phase(&harness),
        QuickPhase::Failed(t!("connections.vault.mismatch").to_owned())
    );
    apply(
        &mut harness,
        QuickMsg::MasterRepeat(SecretString::from("correct horse battery")),
    );
    let effects = apply(&mut harness, QuickMsg::Confirm);
    assert!(matches!(effects.as_slice(), [Effect::Task(_)]));
    assert_eq!(phase(&harness), QuickPhase::Connecting);
    let shown = format!("{:?}", harness.app.quick);
    assert!(!shown.contains("correct horse"), "{shown}");
}

#[tokio::test]
async fn a_failure_is_shown_and_a_wrong_port_never_leaves_the_dialog() {
    let (_root, mut harness) = Harness::temp().await;
    harness.press("Ctrl+N").await;
    fill(&mut harness);
    apply(&mut harness, QuickMsg::Port("70000".to_owned()));
    assert!(apply(&mut harness, QuickMsg::Confirm).is_empty());
    assert!(matches!(phase(&harness), QuickPhase::Failed(_)));
    // RC-080-53: port 0 is no port either.
    apply(&mut harness, QuickMsg::Port("0".to_owned()));
    assert!(apply(&mut harness, QuickMsg::Confirm).is_empty());
    assert!(matches!(phase(&harness), QuickPhase::Failed(_)));

    apply(&mut harness, QuickMsg::Port("2222".to_owned()));
    assert_eq!(phase(&harness), QuickPhase::Editing);
    apply(&mut harness, QuickMsg::Confirm);
    apply(&mut harness, QuickMsg::Finished(Err("host_key_unknown")));
    let QuickPhase::Failed(text) = phase(&harness) else {
        panic!("failed");
    };
    assert!(!text.is_empty());
    assert_eq!(
        harness.app.pane(PanelId::Left).error,
        None,
        "the dialog says it"
    );
}

/// Tab and Shift+Tab walk the dialog's fields (Windows test 2026-10-18); Enter still connects
/// from any of them.
#[tokio::test]
async fn tab_walks_the_fields_and_reaches_no_panel() {
    let (_root, mut harness) = Harness::temp().await;
    harness.press("Ctrl+N").await;
    harness.press("Tab").await;
    harness.press("Tab").await;
    harness.press("Shift+Tab").await;
    assert_eq!(harness.fields_walked, [false, false, true]);
    assert_eq!(
        harness.app.active,
        PanelId::Left,
        "Tab did not reach the panels"
    );
    assert!(harness.app.quick.is_some());
}
