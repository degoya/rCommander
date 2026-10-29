//! The answers to certificate questions, the list and the removal; "trust and save" outlives a
//! restart, "accept once" does not.

use rc_db::Db;
use rc_remote::ftp::{CertificateTrust, TrustDecision, fingerprint};

use crate::{
    CertificateAnswer, Command, Engine, EngineError, Outcome,
    certs::tests::{NAS, OTHER, asked, engine, engine_over, presented, store},
};

async fn answer(engine: &Engine, question: u64, answer: CertificateAnswer) -> Outcome {
    engine
        .dispatch(Command::AnswerCertificate { question, answer })
        .await
        .expect("answered")
}

async fn decision(engine: &Engine, der: &[u8]) -> TrustDecision {
    let (trust, _) = engine
        .certificate_trust("nas.example", 990)
        .await
        .expect("trust");
    trust.decide(&presented(der, false))
}

#[tokio::test]
async fn trust_and_save_outlives_a_restart_and_accept_once_does_not() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("rcommander.db");
    {
        let (_root, engine) = engine_over(Db::open(&path).await.expect("db")).await;
        let mut events = engine.subscribe();
        let _ = engine.certificate_refused(presented(NAS, false)).await;
        let question = asked(&mut events);
        let Outcome::CertificateAnswered { stored, .. } =
            answer(&engine, question.id, CertificateAnswer::TrustAndSave).await
        else {
            panic!("not an answer");
        };
        assert_eq!(stored.expect("stored").fingerprint_sha256, fingerprint(NAS));
        assert!(engine.certificate_questions().is_empty());

        let _ = engine.certificate_refused(presented(OTHER, false)).await;
        let question = asked(&mut events);
        let refused = engine
            .dispatch(Command::AnswerCertificate {
                question: question.id,
                answer: CertificateAnswer::AcceptOnce,
            })
            .await
            .expect_err("a changed certificate takes no yes");
        assert_eq!(refused.code(), "certificate_changed");
    }
    let (_root, engine) = engine_over(Db::open(&path).await.expect("db")).await;
    assert_eq!(decision(&engine, NAS).await, TrustDecision::Trust);
    assert_eq!(decision(&engine, OTHER).await, TrustDecision::Refuse);
    assert!(
        engine.certificate_questions().is_empty(),
        "questions live in memory"
    );
}

#[tokio::test]
async fn accept_once_lets_exactly_the_next_session_through() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    let _ = engine.certificate_refused(presented(NAS, false)).await;
    let question = asked(&mut events);

    answer(&engine, question.id, CertificateAnswer::AcceptOnce).await;

    assert_eq!(decision(&engine, NAS).await, TrustDecision::Trust);
    assert_eq!(decision(&engine, NAS).await, TrustDecision::Refuse, "spent");
    let listed = engine
        .dispatch(Command::ListCertificates { filter: None })
        .await
        .expect("list");
    assert_eq!(
        listed,
        Outcome::TrustedCertificates {
            certificates: Vec::new()
        },
        "nothing stored"
    );
}

#[tokio::test]
async fn an_expired_certificate_takes_accept_once_but_not_trust_and_save() {
    let (_root, engine) = engine().await;
    store(&engine, NAS, 1).await;
    let mut events = engine.subscribe();
    let _ = engine.certificate_refused(presented(NAS, false)).await;
    let question = asked(&mut events);

    let refused = engine
        .dispatch(Command::AnswerCertificate {
            question: question.id,
            answer: CertificateAnswer::TrustAndSave,
        })
        .await
        .expect_err("no silent yes");
    assert_eq!(refused.code(), "certificate_expired");
    answer(&engine, question.id, CertificateAnswer::AcceptOnce).await;
    assert_eq!(decision(&engine, NAS).await, TrustDecision::Trust);
}

#[tokio::test]
async fn a_changed_certificate_goes_on_only_after_the_stored_one_is_removed() {
    let (_root, engine) = engine().await;
    store(&engine, NAS, i64::MAX).await;
    let mut events = engine.subscribe();
    let _ = engine.certificate_refused(presented(OTHER, false)).await;
    let changed = asked(&mut events);
    for choice in [
        CertificateAnswer::AcceptOnce,
        CertificateAnswer::TrustAndSave,
    ] {
        let refused = engine
            .dispatch(Command::AnswerCertificate {
                question: changed.id,
                answer: choice,
            })
            .await
            .expect_err("no yes");
        assert_eq!(refused.code(), "certificate_changed");
    }
    assert_eq!(engine.certificate_questions(), vec![changed.clone()]);
    answer(&engine, changed.id, CertificateAnswer::Cancel).await;

    let Outcome::TrustedCertificates { certificates } = engine
        .dispatch(Command::ListCertificates {
            filter: Some("NAS.EXAMPLE".into()),
        })
        .await
        .expect("list")
    else {
        panic!("not a list");
    };
    assert_eq!(certificates.len(), 1);
    let removed = engine
        .dispatch(Command::RemoveCertificate {
            id: certificates[0].id,
        })
        .await
        .expect("removed");
    assert!(matches!(removed, Outcome::TrustedCertificateRemoved { .. }));
    let gone = engine
        .dispatch(Command::RemoveCertificate {
            id: certificates[0].id,
        })
        .await
        .expect_err("gone");
    assert_eq!(gone.code(), "trusted_certificate_not_found");

    let error = engine.certificate_refused(presented(OTHER, false)).await;
    assert_eq!(error.code(), "certificate_unknown", "asked as a new server");
    let question = asked(&mut events);
    answer(&engine, question.id, CertificateAnswer::TrustAndSave).await;
    assert_eq!(decision(&engine, OTHER).await, TrustDecision::Trust);
}

#[tokio::test]
async fn mcp_can_list_but_neither_answer_nor_remove() {
    let (_root, engine) = engine().await;
    store(&engine, NAS, i64::MAX).await;
    let mut events = engine.subscribe();
    let _ = engine.certificate_refused(presented(OTHER, false)).await;
    let question = asked(&mut events);

    for command in [
        Command::AnswerCertificate {
            question: question.id,
            answer: CertificateAnswer::TrustAndSave,
        },
        Command::RemoveCertificate { id: 1 },
    ] {
        let refused = engine.dispatch_mcp(command).await.expect_err("GUI only");
        assert!(matches!(refused, EngineError::GuiOnly { .. }), "{refused}");
    }
    let listed = engine
        .dispatch_mcp(Command::ListCertificates { filter: None })
        .await
        .expect("read");
    assert!(matches!(
        listed,
        Outcome::TrustedCertificates { certificates } if certificates.len() == 1
    ));
    assert_eq!(engine.certificate_questions(), vec![question]);
}
