//! The kept FTP session against calls that cross (RC-080-51): two logins at once keep one
//! session and close the other, a drop during a login keeps nothing of that login.

use rc_remote::ftp::test_server::{self, Options, TestServer};

use super::*;
use crate::{Command, ConnectionDraft, Outcome, SecretParam, remote::sftp::tests::harness};

/// The FTP connection `files` to `server`, its password stored.
async fn add_ftp(engine: &Engine, server: &TestServer) -> ConnectionId {
    let draft = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        port: Some(i64::from(server.port)),
        username: Some("tester".to_owned()),
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new("files", "127.0.0.1")
    };
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("a connection");
    };
    engine
        .dispatch(Command::SaveConnection(crate::SaveConnectionRequest {
            fields: crate::SaveFields::Update {
                id: connection.id,
                patch: crate::ConnectionPatch::default(),
            },
            secrets: vec![crate::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(test_server::PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    connection.id
}

/// Waits until the server has `count` control connections open, for at most five seconds.
async fn sessions_reach(server: &TestServer, count: usize) {
    for _ in 0..500 {
        if server.open_sessions() == count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!(
        "{} control connections open, not {count}",
        server.open_sessions()
    );
}

#[tokio::test]
async fn two_logins_at_once_keep_one_session_and_close_the_other() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    let id = add_ftp(engine, &server).await;
    let (first, second) = tokio::join!(engine.open_ftp(id, true), engine.open_ftp(id, true));
    let (first, second) = (first.expect("first"), second.expect("second"));
    assert_eq!(server.passwords_received(), 2, "both logged in");
    assert_eq!(first.home(), second.home());
    // The loser said QUIT; the kept one still lists.
    sessions_reach(&server, 1).await;
    drop((first, second));
    let kept = engine.open_ftp(id, true).await.expect("kept");
    assert_eq!(server.passwords_received(), 2, "no third login");
    kept.list(Path::new("/")).await.expect("lists");
}

#[tokio::test]
async fn a_session_dropped_during_its_login_is_not_kept() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    let id = add_ftp(engine, &server).await;
    let (opened, ()) = tokio::join!(engine.open_ftp(id, true), async {
        // The connection changes while the login runs, which drops what is kept.
        while server.open_sessions() == 0 {
            tokio::task::yield_now().await;
        }
        engine.forget_ftp(id).await;
    });
    opened.expect("logged in once more");
    assert_eq!(
        server.passwords_received(),
        2,
        "the login that started before the drop closes, the next one is kept"
    );
    sessions_reach(&server, 1).await;
    engine.open_ftp(id, true).await.expect("kept");
    assert_eq!(
        server.passwords_received(),
        2,
        "the second login is the kept one"
    );
}
