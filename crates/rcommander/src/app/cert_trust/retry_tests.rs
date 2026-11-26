//! RC-110-37 through `apply`, against an engine over a database and a plain FTP connection nobody
//! listens on: the connection its question stopped shows no error and waits; "Send this time"
//! runs it again in the same panel, "Cancel" does not, nor does a yes after the panel moved, nor
//! one for an MCP call's question.

use rc_engine::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, ConnectionKind, JobOptions, Outcome,
    PanelId, TransferRequest,
};
use tempfile::TempDir;

use crate::{
    app::{
        Effect, Job, Message,
        cert_trust::{CertTrustMsg, Choice},
        connections::ConnMsg,
        file_ops::{FileOpsMsg, FollowUp, Submit},
        harness::{self, Harness},
        run_job,
    },
    i18n::t,
};

const MASTER: &str = "master-password-1";

/// A folder with a file and a subfolder, and the app over it with a database.
async fn harness() -> (TempDir, Harness) {
    let root = harness::tree(&[("a.txt", "a"), ("sub/", "")]);
    let harness = Harness::with_db(root.path()).await;
    (root, harness)
}

/// The plain FTP connection "nas" on a port nobody listens on, its password stored, with the
/// question before a password in clear on: a connection is asked before any socket opens, and
/// one a person allowed fails at the refused socket at once. Its id and `host:port`.
async fn plain_ftp(harness: &mut Harness) -> (ConnectionId, String) {
    let engine = harness.app.engine().clone();
    engine
        .dispatch(Command::CreateVault {
            password: MASTER.into(),
        })
        .await
        .expect("the vault");
    // Protected (RC-110-58): the window's way, as the settings dialog stores it.
    engine
        .set_protected_setting(
            rc_engine::settings::FTP_ASK_PLAINTEXT_KEY,
            &serde_json::json!(true),
        )
        .await
        .expect("the question on");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port();
    let connection = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        port: Some(i64::from(port)),
        username: Some("backup".into()),
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new("nas", "127.0.0.1")
    };
    let created = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection,
        })
        .await
        .expect("created");
    let Outcome::Connection { connection } = created else {
        panic!("a connection, got {created:?}");
    };
    engine
        .dispatch(harness::password_change(
            connection.id,
            Some("canary-password-4"),
        ))
        .await
        .expect("stored");
    harness.settle().await;
    (connection.id, format!("127.0.0.1:{port}"))
}

/// The drive change "Open in panel" sends for the connection `id`.
fn change(panel: PanelId, id: ConnectionId) -> Command {
    Command::ChangeDrive {
        panel: Some(panel),
        drive: format!("server:{id}"),
    }
}

/// "Open in panel" in the connection manager: the question stops the connection, the panel's
/// status line stays quiet, the dialog asks and the attempt waits. The question's id.
async fn open_in_panel(harness: &mut Harness, id: ConnectionId) -> u64 {
    harness.press("Ctrl+F").await;
    harness.send(Message::Connections(ConnMsg::Files(id))).await;
    let panel = harness.app.active;
    assert_eq!(
        harness.app.pane(panel).error,
        None,
        "no error while the question is open"
    );
    let question = harness.app.cert_trust.shown_id().expect("the dialog");
    assert_eq!(
        harness.app.engine().certificate_questions()[0].kind,
        rc_engine::CertificateQuestionKind::Plaintext
    );
    assert_eq!(harness.app.cert_attempts.0.len(), 1, "the attempt waits");
    question
}

/// `choice` on the shown dialog and the engine's answer: what the window does then.
async fn answer(harness: &mut Harness, choice: Choice) -> Vec<Effect> {
    let id = harness.app.cert_trust.shown_id().expect("a dialog");
    let chosen = harness
        .app
        .apply(Message::CertTrust(CertTrustMsg::Choose(id, choice)));
    let Ok([Effect::CertTrust(job)]) = <[Effect; 1]>::try_from(chosen) else {
        panic!("one answer for the engine");
    };
    let finished = job.run(harness.app.engine().clone()).await;
    harness.app.apply(Message::CertTrust(finished))
}

#[tokio::test]
async fn send_this_time_connects_again_in_the_same_panel() {
    let (_root, mut harness) = harness().await;
    let (id, server) = plain_ftp(&mut harness).await;
    let panel = harness.app.active;
    open_in_panel(&mut harness, id).await;

    let effects = answer(&mut harness, Choice::AcceptOnce).await;

    let Ok([Effect::Dispatch(again, commands)]) = <[Effect; 1]>::try_from(effects) else {
        panic!("exactly one effect: the connection again");
    };
    assert_eq!(again, panel);
    assert_eq!(commands, vec![change(panel, id)]);
    assert!(harness.app.cert_attempts.0.is_empty());
    let info = harness.app.info.clone().expect("a status line");
    assert_eq!(
        info,
        t!("certs.plaintext_info_once", host = server.as_str())
    );
    // The yes is this connection's: it reaches the socket now, where nobody listens.
    let done = run_job(harness.app.engine(), Job { panel, commands }).await;
    let Message::Done(_, Err(code)) = done else {
        panic!("the connection failed at the socket");
    };
    assert_eq!(code, "ftp_connect_failed");
    assert!(harness.app.engine().certificate_questions().is_empty());
}

#[tokio::test]
async fn cancel_runs_nothing_again() {
    let (_root, mut harness) = harness().await;
    let (id, server) = plain_ftp(&mut harness).await;
    open_in_panel(&mut harness, id).await;

    let effects = answer(&mut harness, Choice::Cancel).await;

    assert!(effects.is_empty(), "no connection again");
    assert!(harness.app.cert_attempts.0.is_empty());
    let info = harness.app.info.clone().expect("a status line");
    assert_eq!(
        info,
        t!("certs.plaintext_info_cancelled", host = server.as_str())
    );
}

#[tokio::test]
async fn a_panel_that_moved_meanwhile_drops_the_attempt() {
    let (_root, mut harness) = harness().await;
    let (id, _) = plain_ftp(&mut harness).await;
    let panel = harness.app.active;
    open_in_panel(&mut harness, id).await;
    let before = harness.app.pane(panel).state.path.clone();
    harness
        .app
        .engine()
        .dispatch(Command::Navigate {
            panel: Some(panel),
            path: "sub".into(),
        })
        .await
        .expect("moved");
    harness.settle().await;
    assert_ne!(harness.app.pane(panel).state.path, before);

    let effects = answer(&mut harness, Choice::TrustAndSave).await;

    assert!(effects.is_empty(), "no jump behind the person's back");
    assert!(harness.app.cert_attempts.0.is_empty());
}

#[tokio::test]
async fn an_mcp_calls_question_runs_nothing_in_the_window() {
    let (_root, mut harness) = harness().await;
    let (id, _) = plain_ftp(&mut harness).await;
    let refused = harness
        .app
        .engine()
        .dispatch_mcp(Command::TestConnection { id })
        .await
        .expect_err("asked");
    assert_eq!(refused.code(), "ftp_plaintext_unconfirmed");
    harness.settle().await;
    assert!(harness.app.cert_attempts.0.is_empty());

    let effects = answer(&mut harness, Choice::AcceptOnce).await;

    assert!(effects.is_empty(), "the agent connects again itself");
}

#[tokio::test]
async fn a_copy_onto_the_server_is_submitted_again_after_the_yes() {
    let (_root, mut harness) = harness().await;
    plain_ftp(&mut harness).await;
    let panel = harness.app.active;
    let submit = Submit {
        panel,
        command: Command::OpsCopy(TransferRequest {
            panel: Some(panel),
            sources: vec!["a.txt".into()],
            target: Some("ftp://nas/upload".into()),
            options: JobOptions::default(),
            rename_mask: None,
        }),
        follow: FollowUp::Transfer { deselect: true },
        background: false,
    };
    let done = submit.clone().run(harness.app.engine().clone()).await;
    assert!(done.asked.is_some(), "a question stopped it");
    harness
        .send(Message::FileOps(FileOpsMsg::Submitted(Box::new(done))))
        .await;
    assert_eq!(harness.app.pane(panel).error, None);
    assert!(harness.app.cert_trust.shown().is_some());

    let effects = answer(&mut harness, Choice::AcceptOnce).await;

    let Ok([Effect::Ops(again)]) = <[Effect; 1]>::try_from(effects) else {
        panic!("exactly one effect: the copy submitted again");
    };
    assert_eq!(again, submit);
}
