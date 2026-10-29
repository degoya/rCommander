//! The canary sweep (RC-080-03): a password, a private key with its passphrase, a login script's
//! secret and the master password are stored the person's way and used for real logins against
//! the in-process SSH server; then every tool the server offers is called - without arguments,
//! and with the connection's id, panel and an `sftp://` path where it takes them - while the vault
//! is unlocked and again after it locked. None of the canaries may turn up, plain, as hex or as
//! base64, in any answer, any engine event, any log line at any level from any crate, or any file
//! the data directory holds afterwards (the database, its WAL, the backups `db_backup` wrote).
//!
//! The last part plants each kind of leak on purpose and shows the same checks find it: a sweep
//! that can never fail proves nothing. One test in its own binary, because it installs the
//! process-wide log subscriber.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use rc_db::{DB_FILE, Db};
use rc_engine::{Command, ConnectionId, LoginScriptId, SecretKind, SecretParam};
use rc_mcp::{McpContext, RcMcpServer};
use rc_remote::{
    OfferedKey,
    testkit::{CLIENT_RSA_PASSPHRASE, ServerOptions, SftpServe, TestServer, host_key_a},
    trust,
};
use rmcp::{
    RoleClient, ServiceExt as _,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
};
use serde_json::{Map, Value, json};
use tokio::sync::broadcast::error::RecvError;
use tracing_subscriber::util::SubscriberInitExt as _;

#[path = "../src/test_support.rs"]
mod test_support;
use test_support::{Built, Fixture};

const PASSWORD: &str = "canary-sweep-password-8d21";
const MASTER: &str = "canary-sweep-master-77ae";
const SCRIPT_SECRET: &str = "canary-sweep-script-3f90";
/// The encrypted RSA key of `rc-remote`'s fixtures, which the test server accepts.
const KEY_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-remote/tests/fixtures/client_key_rsapem"
);
/// How long one call may take; a call that takes longer is dropped, the sweep goes on.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// Every canary: the secrets, and every line of the key's body.
fn canaries() -> Vec<String> {
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
    canaries
}

/// Each canary as it would leak: plain, as hex and as base64.
fn needles() -> Vec<String> {
    canaries()
        .into_iter()
        .flat_map(|canary| {
            let hex: String = canary.bytes().map(|byte| format!("{byte:02x}")).collect();
            let base64 = base64(canary.as_bytes());
            [canary, hex, base64]
        })
        .collect()
}

/// Standard base64 without padding: a leaked value's encoding, wherever it starts.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let joined = chunk.iter().enumerate().fold(0_usize, |acc, (at, byte)| {
            acc | (usize::from(*byte) << (16 - 8 * at))
        });
        for at in 0..=chunk.len() {
            let index = (joined >> (18 - 6 * at)) & 0x3f;
            text.push(char::from(ALPHABET[index]));
        }
    }
    text
}

/// The needles `haystack` carries.
fn leaks(haystack: &[u8]) -> Vec<String> {
    needles()
        .into_iter()
        .filter(|needle| {
            haystack
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
        })
        .collect()
}

/// Everything every log line wrote, at every level and from every crate, `log` records included.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl Log {
    fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
    // `try_init` also bridges the `log` facade (russh logs through it), as the app does.
    subscriber.try_init().expect("the only subscriber");
    log
}

/// Every file below `dir`.
fn files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// The files below `dir` that carry a needle, with the needles.
fn leaking_files(dir: &Path) -> Vec<(PathBuf, Vec<String>)> {
    let mut found = Vec::new();
    files(dir, &mut found);
    found
        .into_iter()
        .filter_map(|path| {
            let leaked = leaks(&std::fs::read(&path).unwrap_or_default());
            (!leaked.is_empty()).then_some((path, leaked))
        })
        .collect()
}

async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(256 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// One call's whole answer as text, error answers and structured content included; a call that
/// fails at the protocol level or takes too long answers its error.
async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> String {
    let request = CallToolRequestParams::new(tool.to_owned())
        .with_arguments(arguments.as_object().cloned().unwrap_or_default());
    match tokio::time::timeout(CALL_TIMEOUT, client.call_tool(request)).await {
        Ok(Ok(answer)) => whole(&answer),
        Ok(Err(error)) => error.to_string(),
        Err(_) => format!("{tool} timed out"),
    }
}

fn whole(answer: &CallToolResult) -> String {
    serde_json::to_string(answer).expect("serializes")
}

/// The id a creating tool answered with.
async fn create(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> i64 {
    let answer = client
        .call_tool(
            CallToolRequestParams::new(tool.to_owned())
                .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
        )
        .await
        .expect("call");
    let text = answer
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("a text block");
    let body: Value = serde_json::from_str(&text).expect("json");
    body["id"].as_i64().expect("an id")
}

/// Every tool with its parameter names.
async fn tools(client: &RunningService<RoleClient, ()>) -> Vec<(String, Vec<String>)> {
    client
        .list_all_tools()
        .await
        .expect("tools")
        .into_iter()
        .map(|tool| {
            let names = tool
                .input_schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| properties.keys().cloned().collect())
                .unwrap_or_default();
            (tool.name.into_owned(), names)
        })
        .collect()
}

/// The arguments of a tool that point it at the canaries: the connection, the script, a panel,
/// the server's files.
fn aimed(tool: &str, names: &[String], web: i64, script: i64) -> Value {
    let mut arguments = Map::new();
    let id = if tool.starts_with("login_scripts_") {
        script
    } else {
        web
    };
    for (name, value) in [
        ("id", json!(id)),
        ("connection_id", json!(web)),
        ("script_id", json!(script)),
        ("panel", json!("left")),
        ("path", json!("sftp://web/notes.txt")),
    ] {
        if names.iter().any(|known| known == name) {
            arguments.insert(name.to_owned(), value);
        }
    }
    Value::Object(arguments)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_canary_leaks_from_any_tool_event_log_or_file() {
    let log = capture_logs();
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
        .with_agent_sources(Vec::new())
        .with_vault()
        .build()
        .await;
    let collected = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut events = engine.subscribe();
    let sink = Arc::clone(&collected);
    let drain = tokio::spawn(async move {
        loop {
            match events.recv().await {
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

    let client = connect(&context).await;
    let mut answers: Vec<(String, String)> = Vec::new();

    // An agent adds the connections and a script; the person stores their secrets.
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
    for command in [
        Command::CreateVault {
            password: SecretParam::from(MASTER),
        },
        Command::SaveConnection(rc_engine::SaveConnectionRequest {
            fields: rc_engine::SaveFields::Update {
                id: ConnectionId(web),
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
        let shown = format!(
            "{command:?}{}",
            serde_json::to_string(&command).expect("json")
        );
        assert!(
            leaks(shown.as_bytes()).is_empty(),
            "a command shows a secret"
        );
        engine.dispatch(command).await.expect("stored");
    }
    // A shared credential holds the password of a connection without one of its own (RC-050-12).
    let team = create(
        &client,
        "credentials_save",
        json!({ "name": "team", "username": "tester" }),
    )
    .await;
    let shared = rc_engine::SaveCredentialRequest {
        fields: rc_engine::CredentialFields::Update {
            id: rc_engine::CredentialId(team),
            patch: rc_engine::CredentialPatch::default(),
        },
        secrets: vec![rc_engine::SecretChange::Set {
            kind: SecretKind::Password,
            value: SecretParam::from(PASSWORD),
        }],
    };
    engine
        .dispatch(Command::SaveCredential(shared))
        .await
        .expect("stored");
    let mut draft = rc_engine::ConnectionDraft::new("team", "127.0.0.1");
    draft.port = Some(i64::from(port));
    let on_team = rc_engine::SaveConnectionRequest {
        fields: rc_engine::SaveFields::Create {
            folder_id: None,
            connection: draft,
        },
        secrets: Vec::new(),
        login_script: None,
        favourite: None,
        credential: Some(Some(rc_engine::CredentialId(team))),
    };
    engine
        .dispatch(Command::SaveConnection(on_team))
        .await
        .expect("pointed");

    // Real logins with the stored secrets, then every tool, unlocked.
    for (tool, arguments) in [
        ("list_dir", json!({ "path": "sftp://web/" })),
        ("list_dir", json!({ "path": "sftp://team/" })),
        ("list_dir", json!({ "path": "sftp://keyed/" })),
        ("files_read_text", json!({ "path": "sftp://web/notes.txt" })),
        ("connections_test", json!({ "id": keyed })),
        ("connections_connect", json!({ "id": web })),
        ("db_backup", json!({})),
    ] {
        answers.push((tool.to_owned(), call(&client, tool, arguments).await));
    }
    assert!(
        answers[1].1.contains("notes.txt"),
        "the shared credential logs in: {}",
        answers[1].1
    );
    let listed = tools(&client).await;
    assert!(listed.len() > 100, "the whole tool list: {}", listed.len());
    for (tool, names) in &listed {
        answers.push((tool.clone(), call(&client, tool, json!({})).await));
        let arguments = aimed(tool, names, web, script);
        answers.push((tool.clone(), call(&client, tool, arguments).await));
    }
    // Locked, the same again.
    answers.push((
        "vault_lock".to_owned(),
        call(&client, "vault_lock", json!({})).await,
    ));
    for (tool, names) in &listed {
        answers.push((tool.clone(), call(&client, tool, json!({})).await));
        let arguments = aimed(tool, names, web, script);
        answers.push((tool.clone(), call(&client, tool, arguments).await));
    }

    for (tool, answer) in &answers {
        let leaked = leaks(answer.as_bytes());
        assert!(leaked.is_empty(), "{tool} answered {leaked:?}");
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let events = collected
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert!(!events.is_empty(), "the events were captured");
    for event in &events {
        assert!(
            leaks(event.as_bytes()).is_empty(),
            "an event leaks: {event}"
        );
    }
    let logged = log.bytes();
    assert!(
        String::from_utf8_lossy(&logged).contains("secret stored"),
        "the log was captured"
    );
    assert!(
        leaks(&logged).is_empty(),
        "the log leaks {:?}",
        leaks(&logged)
    );
    // Each check finds a leak planted on purpose: an answer, ...
    let hex: String = MASTER.bytes().map(|byte| format!("{byte:02x}")).collect();
    let planted = work.path().join("planted.txt");
    std::fs::write(&planted, &hex).expect("write");
    let read = call(
        &client,
        "files_read_text",
        json!({ "path": planted.display().to_string() }),
    )
    .await;
    assert_eq!(
        leaks(read.as_bytes()),
        std::slice::from_ref(&hex),
        "the answer check: {read}"
    );
    assert!(!leaks(base64(SCRIPT_SECRET.as_bytes()).as_bytes()).is_empty());
    // ... a log line ...
    tracing::warn!("planted {PASSWORD}");
    assert!(
        leaks(&log.bytes()).contains(&PASSWORD.to_owned()),
        "the log check"
    );

    db.close().await;
    assert!(
        data.path().join(rc_db::BACKUP_DIR).is_dir(),
        "db_backup wrote its copy"
    );
    let leaking = leaking_files(data.path());
    assert!(leaking.is_empty(), "files leak: {leaking:?}");
    // ... and a file.
    std::fs::write(data.path().join("planted.txt"), &hex).expect("write");
    assert_eq!(leaking_files(data.path()).len(), 1, "the file check");

    drain.abort();
    drop(client);
    drop(context);
    drop(engine);
    drop(server);
}
