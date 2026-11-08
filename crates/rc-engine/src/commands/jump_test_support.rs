//! An engine with in-process SSH servers for the tests of jump hosts and port forwards
//! (RC-110-03): servers on loopback, connections to them with their passwords in the vault, a
//! jump host each may name.

use rc_db::Db;
use rc_remote::{
    OfferedKey,
    testkit::{PASSWORD, ServerOptions, TestServer, host_key_a},
    trust,
};

use crate::test_support;
use crate::{Command, ConnectionDraft, ConnectionId, Engine, Outcome, SecretKind, SecretParam};

/// An engine with an unlocked vault and no SSH agent.
pub(super) struct Bench {
    _root: tempfile::TempDir,
    pub engine: Engine,
    pub db: Db,
}

impl Bench {
    pub(super) async fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let db = Db::open_in_memory().await.expect("db");
        let engine = test_support::headless(root.path()).await;
        engine.attach_db(db.clone());
        engine.attach_agent_sources(Vec::new());
        engine.attach_vault(test_support::fast_vault(db.clone()).await);
        engine
            .dispatch(Command::CreateVault {
                password: SecretParam::from("correct horse battery"),
            })
            .await
            .expect("vault created");
        Self {
            _root: root,
            engine,
            db,
        }
    }

    /// A server with host key a; one that carries `direct-tcpip` channels when `jump`.
    pub(super) async fn server(jump: bool) -> TestServer {
        TestServer::start(ServerOptions {
            direct_tcpip: jump,
            ..ServerOptions::new(host_key_a())
        })
        .await
    }

    /// A connection `name` to the server on `port`, through `jump`, with [`PASSWORD`] stored;
    /// the key a for `127.0.0.1:port` trusted when `trusted`.
    pub(super) async fn connection(
        &self,
        name: &str,
        port: u16,
        jump: Option<ConnectionId>,
        trusted: bool,
    ) -> ConnectionId {
        let draft = ConnectionDraft {
            port: Some(i64::from(port)),
            username: Some("tester".to_owned()),
            jump_id: jump,
            ..ConnectionDraft::new(name, "127.0.0.1")
        };
        let Ok(Outcome::Connection { connection }) = self
            .engine
            .dispatch(Command::CreateConnection {
                folder_id: None,
                connection: draft,
            })
            .await
        else {
            panic!("a connection");
        };
        self.engine
            .dispatch(test_support::store_secret(
                connection.id,
                SecretKind::Password,
                SecretParam::from(PASSWORD),
            ))
            .await
            .expect("password stored");
        if trusted {
            self.trust(port).await;
        }
        connection.id
    }

    /// Trusts host key a for `127.0.0.1:port`.
    pub(super) async fn trust(&self, port: u16) {
        let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
        trust::trust(&self.db, "127.0.0.1", port, offered)
            .await
            .expect("trusted");
    }

    /// Stores `value` under the setting `key`, as the settings dialog does: protected ones
    /// (`ssh.forwards_lan`) included.
    pub(super) async fn set(&self, key: &str, value: bool) {
        self.engine
            .set_protected_setting(key, &serde_json::Value::Bool(value))
            .await
            .expect("setting stored");
    }
}

/// A port of 127.0.0.1 that nothing listens on (bound and let go).
pub(super) fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    listener.local_addr().expect("address").port()
}
