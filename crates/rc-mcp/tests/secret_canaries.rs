//! The canaries (RC-050-03): a planted password, private key and passphrase are stored, read
//! back by a fake connection and then looked for everywhere they could leak - every log line at
//! every level, every MCP answer, every engine event and the database files on disk. None may
//! turn up anywhere.
//!
//! One test in its own binary, because it installs the process-wide log subscriber.

use std::{
    io,
    sync::{Arc, Mutex, PoisonError},
};

use rc_db::{DB_FILE, Db};
use rc_engine::{
    Command, ConnectionId, ConnectionPatch, Engine, Outcome, SecretKind, SecretParam, SecretRef,
};

use rc_mcp::{McpContext, RcMcpServer};
use rc_vault::ExposeSecret as _;
use rmcp::{
    RoleClient, ServiceExt as _,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
};
use serde_json::{Value, json};

#[path = "../src/test_support.rs"]
mod test_support;

use test_support::{Built, Fixture};

const PASSWORD: &str = "canary-password-5f1e9c";
const MASTER: &str = "canary-master-0c7d55";
/// An encrypted RSA key (`rc-remote`'s test fixture) and its passphrase.
const KEY_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-remote/tests/fixtures/client_key_rsapem"
);
const PASSPHRASE: &str = "fixture-passphrase";
/// Its fingerprint as `ssh-keygen -lf` prints it.
const FINGERPRINT: &str = "SHA256:6KajqcW1Orb7TSswcvfa2oFhUNaLC46DsrFV+00r028";

fn key() -> String {
    std::fs::read_to_string(KEY_FILE).expect("the key fixture")
}

/// What must turn up nowhere: the password, the master password, the passphrase and every line
/// of key material.
fn canaries() -> Vec<String> {
    let mut canaries = vec![
        PASSWORD.to_owned(),
        MASTER.to_owned(),
        PASSPHRASE.to_owned(),
    ];
    canaries.extend(
        key()
            .lines()
            .filter(|line| !line.starts_with("-----") && !line.contains(": ") && line.len() >= 16)
            .map(str::to_owned),
    );
    canaries
}

/// Everything every log line wrote, at every level and from every crate.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl Log {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

impl io::Write for Log {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn capture_logs() -> Log {
    let log = Log::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("the only subscriber");
    log
}

fn assert_clean(place: &str, text: &str) {
    for canary in canaries() {
        assert!(!text.contains(&canary), "{place} carries {canary}: {text}");
    }
}

async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// One tool call's answer.
async fn call(
    client: &RunningService<RoleClient, ()>,
    tool: &str,
    arguments: Value,
) -> CallToolResult {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call")
}

/// The whole answer as text, error answers and structured content included.
fn whole(answer: &CallToolResult) -> String {
    serde_json::to_string(answer).expect("serializes")
}

/// The JSON a tool answered with.
fn body(answer: &CallToolResult) -> Value {
    let text = answer
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("a text block");
    serde_json::from_str(&text).expect("json")
}

async fn dispatch(engine: &Engine, command: Command) -> Outcome {
    let debug = format!("{command:?}");
    let serialized = serde_json::to_string(&command).expect("serializes");
    assert_clean("a command's Debug", &debug);
    assert_clean("a serialized command", &serialized);
    engine.dispatch(command).await.expect("dispatch")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_canary_leaks_into_logs_answers_events_or_the_database_file() {
    let log = capture_logs();
    let fixture = Fixture::new();
    let db = Db::open(fixture.root().join(DB_FILE)).await.expect("db");
    let Built {
        root: dir,
        engine,
        db,
        context,
    } = fixture.with_db(db).with_vault().build().await;
    let mut events = engine.subscribe();
    let client = connect(&context).await;

    // An agent adds the connection; the person stores its secrets in the window.
    let created = body(
        &call(
            &client,
            "connections_create",
            json!({ "name": "web", "host": "example.org", "auth_method": "key" }),
        )
        .await,
    );
    let web = ConnectionId(created["id"].as_i64().expect("an id"));
    dispatch(
        &engine,
        Command::CreateVault {
            password: SecretParam::from(MASTER),
        },
    )
    .await;
    dispatch(
        &engine,
        Command::SaveConnection(rc_engine::SaveConnectionRequest {
            fields: rc_engine::SaveFields::Update {
                id: web,
                patch: rc_engine::ConnectionPatch::default(),
            },
            secrets: vec![rc_engine::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }),
    )
    .await;
    dispatch(
        &engine,
        Command::ImportKey(rc_engine::ImportKeyRequest {
            connection_id: web,
            path: KEY_FILE.to_owned(),
            passphrase: Some(SecretParam::from(PASSPHRASE)),
        }),
    )
    .await;
    // A fake connection reads all three.
    for (kind, expected) in [
        (SecretKind::Password, PASSWORD.to_owned()),
        (SecretKind::Key, key()),
        (SecretKind::Passphrase, PASSPHRASE.to_owned()),
    ] {
        let secret = engine
            .resolve_secret(SecretRef {
                connection: web,
                kind,
            })
            .await
            .expect("resolves")
            .expect("stored");
        assert_eq!(secret.expose_secret(), expected);
    }

    // Everything an agent can read while the vault is unlocked.
    let mut answers = Vec::new();
    for (tool, arguments) in [
        ("connections_list", json!({})),
        ("connections_get", json!({ "id": web.0 })),
        ("vault_status", json!({})),
        // An agent offering a secret of its own is refused without an echo of the value.
        (
            "connections_update",
            json!({ "id": web.0, "password": "agent-offered-value" }),
        ),
        ("connections_get", json!({ "id": web.0 + 100 })),
    ] {
        answers.push(call(&client, tool, arguments).await);
    }
    for answer in &answers {
        let text = whole(answer);
        assert_clean("an MCP answer", &text);
        assert!(!text.contains("agent-offered-value"), "{text}");
    }
    let got = body(&answers[1]);
    assert_eq!(got["has_password"], true, "{got}");
    assert_eq!(got["has_key"], true, "{got}");
    assert_eq!(got["has_passphrase"], true, "{got}");
    assert_eq!(
        got["key"],
        json!({
            "type": "ssh-rsa",
            "fingerprint": FINGERPRINT,
            "comment": null,
            "stored": true,
        }),
        "MCP sees the key's type, fingerprint and comment only"
    );
    assert_clean(
        "an MCP answer",
        &whole(&call(&client, "vault_lock", json!({})).await),
    );

    // After the lock a connection asks again, and still nothing leaks.
    let locked = engine
        .resolve_secret(SecretRef {
            connection: web,
            kind: SecretKind::Password,
        })
        .await
        .expect_err("locked");
    assert_eq!(locked.code(), "vault_locked");

    let events: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| serde_json::to_string(&event).expect("serializes"))
        .collect();
    assert!(!events.is_empty());
    for event in &events {
        assert_clean("an engine event", event);
    }

    drop(client);
    drop(context);
    drop(engine);
    db.close().await;
    for entry in std::fs::read_dir(dir.path()).expect("the data directory") {
        let path = entry.expect("an entry").path();
        // A SQLite side file can vanish between the listing and the read under load; a file
        // that is gone keeps nothing.
        let bytes = match std::fs::read(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            read => read.expect("readable"),
        };
        for canary in canaries() {
            assert!(
                !bytes
                    .windows(canary.len())
                    .any(|window| window == canary.as_bytes()),
                "{} carries {canary}",
                path.display()
            );
        }
    }

    let logged = log.text();
    assert!(
        logged.contains("connection saved") && logged.contains("private key taken into the vault"),
        "the log was captured: {logged}"
    );
    assert_clean("the log", &logged);
}

/// A secret the person pasted where nothing marks it (a connection's notes) is still replaced
/// in an MCP answer, word for word, while the vault is unlocked.
#[tokio::test]
async fn a_stored_secret_pasted_elsewhere_is_scrubbed_from_answers() {
    let Built {
        root: _dir,
        engine,
        context,
        ..
    } = Fixture::new().with_vault().build().await;
    let client = connect(&context).await;
    let created = body(
        &call(
            &client,
            "connections_create",
            json!({ "name": "web", "host": "example.org" }),
        )
        .await,
    );
    let web = ConnectionId(created["id"].as_i64().expect("an id"));
    dispatch(
        &engine,
        Command::CreateVault {
            password: SecretParam::from(MASTER),
        },
    )
    .await;
    dispatch(
        &engine,
        Command::SaveConnection(rc_engine::SaveConnectionRequest {
            fields: rc_engine::SaveFields::Update {
                id: web,
                patch: rc_engine::ConnectionPatch::default(),
            },
            secrets: vec![rc_engine::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }),
    )
    .await;
    // Notes are no secret field: the person's own paste carries the password, so this command
    // goes past the helper that holds every command's Debug clean.
    engine
        .dispatch(Command::UpdateConnection {
            id: web,
            patch: ConnectionPatch {
                notes: Some(format!("root pw {PASSWORD}")),
                ..ConnectionPatch::default()
            },
        })
        .await
        .expect("notes stored");
    let got = body(&call(&client, "connections_get", json!({ "id": web.0 })).await);
    assert_eq!(got["notes"], "root pw [redacted]", "{got}");
}
