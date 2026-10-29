//! The certificate gate: what a refused handshake files, and the trust a new session gets.

use rc_db::{Db, NewTrustedCertificate};
use rc_remote::ftp::{CertificateRejection, CertificateTrust, TrustDecision, fingerprint};
use tempfile::TempDir;
use tokio::sync::broadcast::Receiver;

use crate::test_support;
use crate::{CertificateQuestion, CertificateQuestionKind, Engine, EngineEvent};

pub(crate) const NAS: &[u8] = include_bytes!("../../rc-remote/tests/fixtures/tls/nas.der");
pub(crate) const OTHER: &[u8] = include_bytes!("../../rc-remote/tests/fixtures/tls/other.der");

pub(crate) async fn engine_over(db: Db) -> (TempDir, Engine) {
    let (root, engine) = test_support::engine(&[]).await;
    assert!(engine.attach_db(db));
    (root, engine)
}

pub(crate) async fn engine() -> (TempDir, Engine) {
    engine_over(Db::open_in_memory().await.expect("db")).await
}

/// What a handshake to `NAS.example:990` presenting `der` reports when refused.
pub(crate) fn presented(der: &[u8], system_verified: bool) -> CertificateRejection {
    CertificateRejection {
        host: "NAS.example".to_owned(),
        port: 990,
        fingerprint: fingerprint(der),
        reason: if system_verified {
            String::new()
        } else {
            "invalid peer certificate: UnknownIssuer".to_owned()
        },
        system_verified,
        end_entity: der.to_vec(),
    }
}

/// The question the last refusal filed, from the event stream.
pub(crate) fn asked(events: &mut Receiver<EngineEvent>) -> CertificateQuestion {
    loop {
        if let EngineEvent::CertificateQuestion { question } = events.try_recv().expect("an event")
        {
            return *question;
        }
    }
}

pub(crate) async fn store(engine: &Engine, der: &[u8], not_after: i64) {
    engine
        .db()
        .expect("db")
        .add_trusted_certificate(&NewTrustedCertificate {
            host: "nas.example".to_owned(),
            port: 990,
            fingerprint_sha256: fingerprint(der),
            subject: "CN=nas.example".to_owned(),
            issuer: "CN=nas.example".to_owned(),
            not_after,
        })
        .await
        .expect("add")
        .expect("stored");
}

#[tokio::test]
async fn an_unknown_certificate_files_one_question_with_its_details() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();

    let error = engine.certificate_refused(presented(NAS, false)).await;

    assert_eq!(error.code(), "certificate_unknown");
    assert!(error.to_string().contains(&fingerprint(NAS)), "{error}");
    let question = asked(&mut events);
    assert_eq!(
        (question.host.as_str(), question.port),
        ("nas.example", 990)
    );
    assert_eq!(question.kind, CertificateQuestionKind::Unknown);
    let info = question.info.clone().expect("parsed");
    assert!(info.subject.contains("CN=nas.example"), "{info:?}");
    assert!(!question.reason.is_empty());

    // The same refusal again finds the open question.
    let _ = engine.certificate_refused(presented(NAS, false)).await;
    assert_eq!(engine.certificate_questions(), vec![question]);
}

#[tokio::test]
async fn another_certificate_than_the_stored_one_is_a_change_even_if_validly_signed() {
    let (_root, engine) = engine().await;
    store(&engine, NAS, i64::MAX).await;
    let mut events = engine.subscribe();

    let error = engine.certificate_refused(presented(OTHER, true)).await;

    assert_eq!(error.code(), "certificate_changed");
    assert!(matches!(
        asked(&mut events).kind,
        CertificateQuestionKind::Changed { .. }
    ));
}

#[tokio::test]
async fn an_expired_stored_certificate_is_a_warning_not_a_yes() {
    let (_root, engine) = engine().await;
    store(&engine, NAS, 1).await;
    let mut events = engine.subscribe();

    let (trust, _) = engine
        .certificate_trust("nas.example", 990)
        .await
        .expect("trust");
    assert_eq!(trust.decide(&presented(NAS, false)), TrustDecision::Refuse);
    let error = engine.certificate_refused(presented(NAS, false)).await;

    assert_eq!(error.code(), "certificate_expired");
    assert!(matches!(
        asked(&mut events).kind,
        CertificateQuestionKind::Expired { .. }
    ));
}

#[tokio::test]
async fn a_session_trusts_the_stored_certificate_and_the_roots() {
    let (_root, engine) = engine().await;
    let (trust, pinned) = engine
        .certificate_trust("NAS.example", 990)
        .await
        .expect("trust");
    assert_eq!(pinned, None);
    assert_eq!(trust.decide(&presented(NAS, false)), TrustDecision::Refuse);
    assert_eq!(trust.decide(&presented(OTHER, true)), TrustDecision::Trust);

    store(&engine, NAS, i64::MAX).await;
    let (trust, pinned) = engine
        .certificate_trust("NAS.example", 990)
        .await
        .expect("trust");
    assert!(pinned.is_some());
    assert_eq!(trust.decide(&presented(NAS, false)), TrustDecision::Trust);
    assert_eq!(trust.decide(&presented(OTHER, true)), TrustDecision::Refuse);
}

/// RC-080-22: certificate questions keep the shared desk's rules - one question per server,
/// certificate and kind whatever the reason, "accept once" grants the presented fingerprint, and
/// the engine keeps at most `MAX_QUESTIONS`, dropping the oldest.
#[tokio::test]
async fn certificate_questions_go_through_the_shared_desk() {
    use crate::question_desk::{MAX_QUESTIONS, Question};

    let question = CertificateQuestion {
        id: 7,
        host: "nas.example".to_owned(),
        port: 990,
        fingerprint: fingerprint(NAS),
        reason: "invalid peer certificate: UnknownIssuer".to_owned(),
        info: None,
        kind: CertificateQuestionKind::Unknown,
    };
    let later = CertificateQuestion {
        id: 0,
        reason: String::new(),
        ..question.clone()
    };
    assert!(question.same_as(&later));
    for other in [
        CertificateQuestion {
            fingerprint: fingerprint(OTHER),
            ..question.clone()
        },
        CertificateQuestion {
            port: 21,
            ..question.clone()
        },
    ] {
        assert!(!question.same_as(&other), "{other:?}");
    }
    let presented_fingerprint = fingerprint(NAS);
    assert_eq!(
        question.grant(),
        ("nas.example", 990, presented_fingerprint.as_str())
    );

    let (_root, engine) = engine().await;
    for n in 0..=MAX_QUESTIONS {
        let _ = engine
            .certificate_refused(CertificateRejection {
                host: format!("nas{n}.example"),
                ..presented(NAS, false)
            })
            .await;
    }
    let open = engine.certificate_questions();
    assert_eq!(open.len(), MAX_QUESTIONS);
    assert_eq!(open.first().map(|question| question.id), Some(2));
    assert_eq!(
        open.first().map(|question| question.host.as_str()),
        Some("nas1.example")
    );
}
