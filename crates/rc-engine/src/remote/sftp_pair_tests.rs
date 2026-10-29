//! F5 between two SFTP panels through the engine (RC-060-06): two servers, each its own
//! connection, and one server in both panels, which copies over the one session it has.

use rc_ops::{JobOptions, JobSnapshot, JobState};
use rc_remote::{
    OfferedKey,
    testkit::{PASSWORD, ServerOptions, SftpServe, TestServer, host_key_a},
    trust,
};

use super::tests::harness;
use crate::{
    Command, ConnectionDraft, ConnectionId, Engine, Outcome, PanelId, SecretKind, SecretParam,
    TransferRequest,
};

/// A second SFTP server serving `served` with `/home/tester` as home, as the connection `name`:
/// its key trusted, its password stored.
async fn add_server(
    engine: &Engine,
    served: &std::path::Path,
    name: &str,
) -> (TestServer, ConnectionId) {
    std::fs::create_dir_all(served.join("home/tester")).expect("home");
    let mut serve = SftpServe::new(served);
    serve.home = "/home/tester".to_owned();
    let server = TestServer::start(ServerOptions {
        sftp: Some(serve),
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let draft = ConnectionDraft {
        port: Some(i64::from(server.port)),
        username: Some("tester".to_owned()),
        ..ConnectionDraft::new(name, "127.0.0.1")
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
                value: SecretParam::from(PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
    let db = engine.db().expect("a database");
    trust::trust(db, "127.0.0.1", server.port, offered)
        .await
        .expect("trusted");
    (server, connection.id)
}

/// The server's drive button in `panel`.
async fn open_in(engine: &Engine, panel: PanelId, id: ConnectionId) {
    engine
        .dispatch(Command::ChangeDrive {
            panel: Some(panel),
            drive: format!("server:{id}"),
        })
        .await
        .expect("the server opens");
}

/// F5 on `name` in the left panel, into the right one; the job once it ended.
async fn copy_left_to_right(engine: &Engine, name: &str) -> JobSnapshot {
    let queued = engine
        .dispatch(Command::OpsCopy(TransferRequest {
            panel: Some(PanelId::Left),
            sources: vec![name.to_owned()],
            target: None,
            options: JobOptions::default(),
        }))
        .await
        .expect("queued");
    let Outcome::Job(job) = queued else {
        panic!("a job");
    };
    engine.ops().wait(job.id).await.expect("known")
}

/// `size` bytes no two neighbouring blocks of which are alike.
fn content(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| u8::try_from(index % 251).expect("below 251"))
        .collect()
}

#[tokio::test]
async fn f5_copies_a_folder_from_one_sftp_server_to_another() {
    let base = harness().await;
    let engine = &base.engine;
    let other = tempfile::tempdir().expect("served");
    let (_server, id) = add_server(engine, other.path(), "other").await;
    let data = content(600 * 1024);
    let folder = base.served.path().join("home/tester/docs");
    std::fs::write(folder.join("big.bin"), &data).expect("file");
    std::fs::write(folder.join("note.txt"), b"note").expect("file");
    open_in(engine, PanelId::Left, base.id).await;
    open_in(engine, PanelId::Right, id).await;

    let done = copy_left_to_right(engine, "docs").await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let arrived = other.path().join("home/tester/docs");
    let copied = std::fs::read(arrived.join("big.bin")).expect("copied");
    assert!(copied == data, "byte for byte");
    assert_eq!(
        std::fs::read(arrived.join("note.txt")).expect("copied"),
        b"note"
    );
    assert!(!arrived.join("big.bin.rcpart").exists());
}

#[tokio::test]
async fn f5_within_one_sftp_server_uses_its_one_connection() {
    let base = harness().await;
    let engine = &base.engine;
    let data = content(600 * 1024);
    std::fs::write(base.served.path().join("home/tester/big.bin"), &data).expect("file");
    open_in(engine, PanelId::Left, base.id).await;
    open_in(engine, PanelId::Right, base.id).await;
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "docs".to_owned(),
        })
        .await
        .expect("into docs");
    let connections = base.server.accepts();

    let done = copy_left_to_right(engine, "big.bin").await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let copied =
        std::fs::read(base.served.path().join("home/tester/docs/big.bin")).expect("copied");
    assert!(copied == data, "byte for byte");
    assert_eq!(
        base.server.accepts(),
        connections,
        "no second TCP connection"
    );
}
