//! The certificate dialogs through `apply`, against an engine over a database: the question a
//! refused handshake files opens the dialog, the keyboard starts on Cancel, a changed
//! certificate takes no yes and an expired one no "trust and save".

use rc_db::{Db, NewTrustedCertificate};
use rc_engine::{EngineError, PanelId};
use rc_remote::ftp::{CertificateRejection, fingerprint};
use tempfile::TempDir;

use super::{CertTrustMsg, Choice, date};
use crate::app::{Message, harness::Harness};

const NAS: &[u8] = include_bytes!("../../../rc-remote/tests/fixtures/tls/nas.der");
const OTHER: &[u8] = include_bytes!("../../../rc-remote/tests/fixtures/tls/other.der");

const HOST: &str = "nas.example";
const PORT: u16 = 990;

/// A handshake presenting `der` was refused: the engine's error, and the app hears of it.
async fn present(harness: &mut Harness, der: &[u8]) -> EngineError {
    let error = harness
        .app
        .engine()
        .certificate_refused(CertificateRejection {
            host: HOST.to_owned(),
            port: PORT,
            fingerprint: fingerprint(der),
            reason: "invalid peer certificate: UnknownIssuer".to_owned(),
            system_verified: false,
            end_entity: der.to_vec(),
        })
        .await;
    harness.settle().await;
    error
}

/// Stores `der` as the server's trusted certificate, valid until `not_after`.
async fn store(db: &Db, der: &[u8], not_after: i64) {
    db.add_trusted_certificate(&NewTrustedCertificate {
        host: HOST.to_owned(),
        port: PORT,
        fingerprint_sha256: fingerprint(der),
        subject: "CN=nas.example".to_owned(),
        issuer: "CN=nas.example".to_owned(),
        not_after,
    })
    .await
    .expect("add")
    .expect("stored");
}

fn shown(harness: &Harness) -> u64 {
    harness
        .app
        .cert_trust
        .shown()
        .map(|question| question.id)
        .expect("a dialog is open")
}

async fn choose(harness: &mut Harness, choice: Choice) {
    let id = shown(harness);
    harness
        .send(Message::CertTrust(CertTrustMsg::Choose(id, choice)))
        .await;
}

#[tokio::test]
async fn an_unknown_certificate_opens_the_dialog_and_enter_cancels() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;

    let error = present(&mut harness, NAS).await;

    assert_eq!(error.code(), "certificate_unknown");
    let dialogs = &harness.app.cert_trust;
    let question = dialogs.shown().expect("the dialog");
    assert_eq!(question.fingerprint, fingerprint(NAS));
    assert!(question.info.is_some());
    assert_eq!(
        dialogs.buttons(),
        [Choice::AcceptOnce, Choice::TrustAndSave, Choice::Cancel]
    );
    assert_eq!(dialogs.focused(), Some(Choice::Cancel));

    harness.press("Enter").await;
    assert!(harness.app.cert_trust.shown().is_none());
    let info = harness.app.info.clone().expect("a status line");
    assert!(info.contains(HOST), "{info}");
    assert_eq!(
        present(&mut harness, NAS).await.code(),
        "certificate_unknown",
        "nothing stored"
    );
}

#[tokio::test]
async fn trust_and_save_from_the_keyboard_stores_the_certificate() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = present(&mut harness, NAS).await;

    harness.press("Shift+Tab").await;
    assert_eq!(harness.app.cert_trust.focused(), Some(Choice::TrustAndSave));
    harness.press("Enter").await;

    assert!(harness.app.cert_trust.shown().is_none());
    let listed = harness
        .app
        .engine()
        .dispatch(rc_engine::Command::ListCertificates { filter: None })
        .await
        .expect("list");
    let rc_engine::Outcome::TrustedCertificates { certificates } = listed else {
        panic!("not a list");
    };
    assert_eq!(certificates.len(), 1);
    assert_eq!(certificates[0].fingerprint_sha256, fingerprint(NAS));
}

#[tokio::test]
async fn a_changed_certificate_offers_only_cancel() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = present(&mut harness, NAS).await;
    choose(&mut harness, Choice::TrustAndSave).await;

    let error = present(&mut harness, OTHER).await;

    assert_eq!(error.code(), "certificate_changed");
    let dialogs = &harness.app.cert_trust;
    assert_eq!(dialogs.buttons(), [Choice::Cancel]);
    choose(&mut harness, Choice::TrustAndSave).await;
    choose(&mut harness, Choice::AcceptOnce).await;
    assert!(harness.app.cert_trust.shown().is_some(), "no yes was sent");
    assert!(!harness.app.cert_trust.busy());

    harness.press("Escape").await;
    assert!(harness.app.cert_trust.shown().is_none());
    assert_eq!(
        present(&mut harness, OTHER).await.code(),
        "certificate_changed"
    );
}

#[tokio::test]
async fn an_expired_certificate_offers_accept_once_and_cancel() {
    let root = TempDir::new().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    store(&db, NAS, 1).await;
    let mut harness = Harness::on_db(root.path(), db).await;

    let error = present(&mut harness, NAS).await;

    assert_eq!(error.code(), "certificate_expired");
    assert_eq!(
        harness.app.cert_trust.buttons(),
        [Choice::AcceptOnce, Choice::Cancel]
    );
    assert_eq!(harness.app.cert_trust.focused(), Some(Choice::Cancel));
    choose(&mut harness, Choice::AcceptOnce).await;
    assert!(harness.app.cert_trust.shown().is_none());
}

/// While a dialog is open the panels do not move under it, and a stale click is dropped.
#[tokio::test]
async fn the_keys_belong_to_the_dialog_and_a_stale_click_is_dropped() {
    let root = TempDir::new().expect("temp dir");
    std::fs::create_dir(root.path().join("sub")).expect("mkdir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = present(&mut harness, NAS).await;
    let before = harness.app.pane(PanelId::Left).cursor();

    harness.press("ArrowDown").await;
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), before);

    let id = shown(&harness);
    harness
        .send(Message::CertTrust(CertTrustMsg::Choose(
            id + 1,
            Choice::TrustAndSave,
        )))
        .await;
    assert_eq!(shown(&harness), id);
    assert!(!harness.app.cert_trust.busy());
}

#[tokio::test]
async fn missed_questions_are_read_again() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = Harness::with_db(root.path()).await;
    let _ = harness
        .app
        .engine()
        .certificate_refused(CertificateRejection {
            host: HOST.to_owned(),
            port: PORT,
            fingerprint: fingerprint(NAS),
            system_verified: false,
            end_entity: NAS.to_vec(),
            ..CertificateRejection::default()
        })
        .await;
    harness.miss_events();
    assert!(harness.app.cert_trust.shown().is_none());

    harness.send(Message::EngineLagged).await;

    assert!(harness.app.cert_trust.shown().is_some());
}

#[test]
fn dates_read_as_utc() {
    assert_eq!(date(1_577_836_800), "2020-01-01 00:00 UTC");
}
