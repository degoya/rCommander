//! The question before a plain FTP login sends its password in clear (RC-080-67,
//! `ftp.ask_plaintext`): no `PASS` reaches the server until a person allowed it - once, or for
//! the server until rCommander ends - and "cancel" sends nothing.

use rc_remote::ftp::test_server::{Options, TestServer};

use super::panel_tests::add_ftp;
use super::*;
use crate::{
    CertificateAnswer, CertificateQuestion, CertificateQuestionKind, Command, EngineEvent, Outcome,
    remote::sftp::tests::harness, settings::FTP_ASK_PLAINTEXT_KEY,
};

async fn ask(engine: &Engine, on: bool) {
    engine
        .dispatch(Command::SetSetting {
            key: FTP_ASK_PLAINTEXT_KEY.into(),
            value: serde_json::json!(on),
        })
        .await
        .expect("set");
}

/// The plaintext question filed since `events` subscribed.
fn asked(events: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> CertificateQuestion {
    std::iter::from_fn(|| events.try_recv().ok())
        .find_map(|event| match event {
            EngineEvent::CertificateQuestion { question }
                if question.kind == CertificateQuestionKind::Plaintext =>
            {
                Some(*question)
            }
            _ => None,
        })
        .expect("a plaintext question")
}

async fn answer(engine: &Engine, question: u64, answer: CertificateAnswer) {
    let outcome = engine
        .dispatch(Command::AnswerCertificate { question, answer })
        .await
        .expect("answered");
    assert!(matches!(outcome, Outcome::CertificateAnswered { .. }));
}

#[tokio::test]
async fn no_password_leaves_before_a_person_allowed_it() {
    let base = harness().await;
    let engine = &base.engine;
    ask(engine, true).await;
    let server = TestServer::start(Options::default()).await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "plain").await;

    // Asked: the server saw no password; "cancel" sends none either.
    let mut events = engine.subscribe();
    let refused = engine.connect_ftp(id, true).await.expect_err("asked");
    assert_eq!(refused.code(), "ftp_plaintext_unconfirmed");
    let question = asked(&mut events);
    assert_eq!(
        (question.host.as_str(), question.port),
        ("127.0.0.1", server.port)
    );
    assert!(question.info.is_none());
    answer(engine, question.id, CertificateAnswer::Cancel).await;
    let again = engine.connect_ftp(id, true).await.expect_err("asked again");
    assert_eq!(again.code(), "ftp_plaintext_unconfirmed");
    assert_eq!(server.passwords_received(), 0, "no PASS after a no");

    // "Accept once": this login, and the next one asks again.
    let question = asked(&mut events);
    answer(engine, question.id, CertificateAnswer::AcceptOnce).await;
    drop(engine.connect_ftp(id, true).await.expect("logged in"));
    assert_eq!(server.passwords_received(), 1);
    let next = engine
        .connect_ftp(id, true)
        .await
        .expect_err("once is once");
    assert_eq!(next.code(), "ftp_plaintext_unconfirmed");

    // "Trust and save": every login to the server until rCommander ends; nothing is stored.
    let question = asked(&mut events);
    answer(engine, question.id, CertificateAnswer::TrustAndSave).await;
    for _ in 0..2 {
        drop(engine.connect_ftp(id, true).await.expect("logged in"));
    }
    assert_eq!(server.passwords_received(), 3);
    let db = engine.db().expect("db");
    assert!(db.trusted_certificates().await.expect("list").is_empty());
}

#[tokio::test]
async fn off_the_login_goes_at_once_and_an_mcp_call_is_refused_alike() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "plain").await;
    ask(engine, false).await;
    drop(engine.connect_ftp(id, true).await.expect("no question"));
    assert_eq!(server.passwords_received(), 1);

    // On: an agent's call is refused the same way, and the person decides in the window.
    ask(engine, true).await;
    let refused = engine
        .dispatch_mcp(Command::TestConnection { id })
        .await
        .expect_err("asked");
    assert_eq!(refused.code(), "ftp_plaintext_unconfirmed");
    assert_eq!(server.passwords_received(), 1);
    assert_eq!(engine.certificate_questions().len(), 1);
}
