//! The scene of the sweep: a database file in a data folder, a headless engine that answers with
//! confirmation codes, the in-process SSH server with an SFTP folder, and the canaries stored
//! the person's way - a connection's password, a private key with its passphrase, a login
//! script's secret, a shared credential's password and the master password. Every engine event
//! is collected from the start.

use std::sync::{Arc, Mutex, PoisonError};

use rc_db::{DB_FILE, Db};
use rc_engine::{Command, ConnectionId, Engine, LoginScriptId, SecretKind, SecretParam};
use rc_mcp::McpContext;
use rc_remote::{
    OfferedKey,
    testkit::{CLIENT_RSA_PASSPHRASE, ServerOptions, SftpServe, TestServer, host_key_a},
    trust,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{sync::broadcast::error::RecvError, task::JoinHandle};

use crate::{
    args::Targets,
    encodings::Needles,
    test_support::{Built, Fixture},
    transport::{self, Client},
};

pub const PASSWORD: &str = "canary-sweep-password-8d21";
pub const MASTER: &str = "canary-sweep-master-77ae";
const SCRIPT_SECRET: &str = "canary-sweep-script-3f90";
/// The encrypted RSA key of `rc-remote`'s fixtures, which the test server accepts.
const KEY_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-remote/tests/fixtures/client_key_rsapem"
);

/// Every secret canary: the secrets, and every line of the key's body. The MCP token is a
/// canary of its own (`main.rs`): it is stored in the clear by design, so it is looked for in
/// answers, events and logs, not in the data files.
pub fn secret_needles() -> Needles {
    let mut canaries = vec![
        PASSWORD.to_owned(),
        MASTER.to_owned(),
        SCRIPT_SECRET.to_owned(),
        CLIENT_RSA_PASSPHRASE.to_owned(),
    ];
    let key = std::fs::read_to_string(KEY_FILE).expect("the key fixture");
    canaries.extend(
        key.lines()
            .filter(|line| !line.starts_with("-----") && !line.contains(": ") && line.len() >= 16)
            .map(str::to_owned),
    );
    Needles::of(canaries.as_slice())
}

pub struct Scene {
    pub data: TempDir,
    pub _remote: TempDir,
    pub work: TempDir,
    pub engine: Engine,
    pub db: Db,
    pub context: McpContext,
    pub _server: TestServer,
    pub targets: Targets,
    /// The connection that logs in with the private key.
    pub keyed: i64,
    pub events: Arc<Mutex<Vec<String>>>,
    pub drain: JoinHandle<()>,
}

impl Scene {
    /// The events collected so far, each as JSON.
    pub fn events(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The id a creating tool answered with.
async fn create(client: &Client, tool: &str, arguments: Value) -> i64 {
    let answer = transport::call(client, tool, &arguments).await;
    let answer: Value = serde_json::from_str(&answer.text).expect("json");
    let text = answer["content"][0]["text"].as_str().expect("a text block");
    let body: Value = serde_json::from_str(text).expect("json");
    body["id"].as_i64().expect("an id")
}

/// `command` run, after its `Debug` and serde forms were found to show no secret.
async fn store(engine: &Engine, needles: &Needles, command: Command) {
    let shown = format!(
        "{command:?}{}",
        serde_json::to_string(&command).expect("json")
    );
    assert!(
        needles.found(shown.as_bytes()).is_empty(),
        "a command shows a secret"
    );
    engine.dispatch(command).await.expect("stored");
}

/// The scene, the canaries stored; `client` is connected in process.
pub async fn scene() -> (Scene, Client) {
    let data = tempfile::tempdir().expect("data dir");
    let remote = tempfile::tempdir().expect("remote dir");
    std::fs::write(remote.path().join("notes.txt"), b"nothing secret").expect("write");

    let db = Db::open(data.path().join(DB_FILE)).await.expect("db");
    let Built {
        root: work,
        engine,
        db,
        context,
    } = Fixture::new()
        .with_db(db)
        .with_codes()
        .with_agent_sources(Vec::new())
        .with_vault()
        .build()
        .await;
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut receiver = engine.subscribe();
    let sink = Arc::clone(&events);
    let drain = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => sink
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(serde_json::to_string(&event).expect("serializes")),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });

    let mut server_options = ServerOptions::new(host_key_a());
    server_options.password = Some(PASSWORD.to_owned());
    server_options.sftp = Some(SftpServe::new(remote.path()));
    let server = TestServer::start(server_options).await;
    let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
    trust::trust(&db, "127.0.0.1", server.port, offered)
        .await
        .expect("trusted");

    let client = transport::in_process(&context).await;
    // An agent adds the connections, a script and a shared credential; the person stores the
    // secrets.
    let port = server.port;
    let web = create(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "127.0.0.1", "port": port, "username": "tester", "auth_method": "password" }),
    )
    .await;
    let keyed = create(
        &client,
        "connections_create",
        json!({ "name": "keyed", "host": "127.0.0.1", "port": port, "username": "tester", "auth_method": "key" }),
    )
    .await;
    let script = create(&client, "login_scripts_create", json!({ "name": "root" })).await;
    let secret = |kind: SecretKind, value: &str| {
        vec![rc_engine::SecretChange::Set {
            kind,
            value: SecretParam::from(value),
        }]
    };
    let needles = secret_needles();
    for command in [
        Command::CreateVault {
            password: SecretParam::from(MASTER),
        },
        Command::SaveConnection(rc_engine::SaveConnectionRequest {
            fields: rc_engine::SaveFields::Update {
                id: ConnectionId(web),
                patch: rc_engine::ConnectionPatch::default(),
            },
            secrets: secret(SecretKind::Password, PASSWORD),
            login_script: None,
            favourite: None,
            credential: None,
        }),
        Command::ImportKey(rc_engine::ImportKeyRequest {
            connection_id: ConnectionId(keyed),
            path: KEY_FILE.to_owned(),
            passphrase: Some(SecretParam::from(CLIENT_RSA_PASSPHRASE)),
        }),
        Command::SetLoginScriptSecret {
            script_id: LoginScriptId(script),
            label: "root".to_owned(),
            value: SecretParam::from(SCRIPT_SECRET),
        },
    ] {
        store(&engine, &needles, command).await;
    }
    // A shared credential holds the password of a connection without one (RC-050-12).
    let team = create(
        &client,
        "credentials_save",
        json!({ "name": "team", "username": "tester" }),
    )
    .await;
    let mut draft = rc_engine::ConnectionDraft::new("team", "127.0.0.1");
    draft.port = Some(i64::from(port));
    for command in [
        Command::SaveCredential(rc_engine::SaveCredentialRequest {
            fields: rc_engine::CredentialFields::Update {
                id: rc_engine::CredentialId(team),
                patch: rc_engine::CredentialPatch::default(),
            },
            secrets: secret(SecretKind::Password, PASSWORD),
        }),
        Command::SaveConnection(rc_engine::SaveConnectionRequest {
            fields: rc_engine::SaveFields::Create {
                folder_id: None,
                connection: draft,
            },
            secrets: Vec::new(),
            login_script: None,
            favourite: None,
            credential: Some(Some(rc_engine::CredentialId(team))),
        }),
    ] {
        store(&engine, &needles, command).await;
    }
    let targets = Targets {
        web,
        script,
        credential: team,
        port,
        terminal: None,
        workspace: None,
    };
    let scene = Scene {
        data,
        _remote: remote,
        work,
        engine,
        db,
        context,
        _server: server,
        targets,
        keyed,
        events,
        drain,
    };
    (scene, client)
}
