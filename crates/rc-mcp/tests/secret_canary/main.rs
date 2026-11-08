//! The canary sweep (RC-080-03, extended by RC-080-61): a password, a private key with its
//! passphrase, a login script's secret, a shared credential's password and the master password
//! are stored the person's way and used for real logins against the in-process SSH server; then
//! every tool the server offers is called - without arguments, and with arguments made from its
//! input schema (`args.rs`) - unlocked in process, unlocked over the Streamable HTTP transport
//! with its bearer token, locked, and unlocked again with every confirmation code redeemed that
//! acts only on the test server or the test's own data. None of the canaries may turn up, in any
//! of the forms of `encodings.rs`, in any answer, any engine event, any log line at any level
//! from any crate, any file the data directory holds afterwards (the database, its WAL, the
//! backups `db_backup` wrote) or any value of any table. The MCP token (`mcp.token`) is a canary
//! of the answers, the HTTP server's own refusals, the events and the logs; it is stored in the
//! clear by design, so not of the files.
//!
//! The last part plants each kind of leak on purpose and shows the same checks find it: a sweep
//! that can never fail proves nothing. One test with the server in its own binary, because it
//! installs the process-wide log subscriber.

#[path = "../../src/test_support.rs"]
mod test_support;

mod args;
mod checks;
mod encodings;
mod setup;
mod transport;

use std::{collections::BTreeSet, time::Duration};

use rc_db::DB_FILE;
use rc_engine::{Command, SecretParam};
use rc_mcp::{McpContext, McpToken, RcMcpServer};
use serde_json::{Map, Value, json};

use crate::{
    args::Targets,
    encodings::{Needles, forms},
    transport::{Answer, Client},
};

/// The executing tools whose question the sweep answers, as long as the call names the test
/// server's connection, its terminal or the workspace that shows it ([`on_test_server`]); the
/// login-script and connection tools escalate to exec only for what they store, and the
/// terminal tools that only list or show act on no machine, should their class ask.
const EXEC_CONFIRMED: &[&str] = &[
    "ssh_exec",
    "terminal_open",
    "terminal_send_input",
    "terminal_read_screen",
    "terminal_list",
    "terminal_show",
    "workspaces_apply",
    "login_scripts_assign",
    "login_scripts_create",
    "login_scripts_update",
    "connections_save",
];

/// The tools whose question the sweep never answers: they would act on the machine itself (a
/// program started, a network drive dropped).
const NEVER_CONFIRMED: &[&str] = &[
    "file_open",
    "drives_unmap_network",
    "user_commands_run",
    "start_menu_run",
    "button_bars_run",
];

/// The tools that must reach their core once their code is redeemed.
const CORE: &[&str] = &[
    "ssh_exec",
    "terminal_send_input",
    "terminal_read_screen",
    "workspaces_apply",
];

/// Whether a call of `tool` with `arguments` acts on the test server only.
fn on_test_server(tool: &str, arguments: &Value, targets: &Targets) -> bool {
    let names =
        |key: &str, value: Option<Value>| value.is_some_and(|value| arguments[key] == value);
    match tool {
        "ssh_exec" => names("connection_id", Some(json!(targets.web))),
        "terminal_open" => {
            names("connection_id", Some(json!(targets.web)))
                && arguments.get("shell").is_none_or(Value::is_null)
                && arguments.get("distro").is_none_or(Value::is_null)
        }
        "terminal_send_input" | "terminal_read_screen" => {
            names("terminal", targets.terminal.map(|id| json!(id)))
        }
        "workspaces_apply" => names("id", targets.workspace.map(|id| json!(id))),
        _ => EXEC_CONFIRMED.contains(&tool),
    }
}

/// Every answer of the sweep, and what the confirmed calls reached.
#[derive(Default)]
struct Sweep {
    answers: Vec<(String, String)>,
    /// The tools that answered without an error since the last [`BTreeSet::clear`].
    reached: BTreeSet<String>,
    /// The tools that asked, as `tool (class)`, which the sweep neither confirms nor keeps from
    /// the machine: an executing tool added since.
    unknown: BTreeSet<String>,
}

impl Sweep {
    /// `tool` called; with `confirm`, called again with the code its question answered with,
    /// where the sweep may answer it.
    async fn call(
        &mut self,
        client: &Client,
        tool: &str,
        arguments: &Value,
        confirm: bool,
        targets: &Targets,
    ) -> Answer {
        let mut answer = transport::call(client, tool, arguments).await;
        self.answers.push((tool.to_owned(), answer.text.clone()));
        if confirm
            && let Some((code, class)) = answer.question.clone()
            && self.may_confirm(tool, &class, arguments, targets)
        {
            let mut again = arguments.clone();
            again["confirmation"] = json!(code);
            answer = transport::call(client, tool, &again).await;
            self.answers.push((tool.to_owned(), answer.text.clone()));
        }
        if answer.ok {
            self.reached.insert(tool.to_owned());
        }
        answer
    }

    fn may_confirm(
        &mut self,
        tool: &str,
        class: &str,
        arguments: &Value,
        targets: &Targets,
    ) -> bool {
        if NEVER_CONFIRMED.contains(&tool) {
            return false;
        }
        if EXEC_CONFIRMED.contains(&tool) {
            return on_test_server(tool, arguments, targets);
        }
        if class == "destructive" {
            return true;
        }
        self.unknown.insert(format!("{tool} ({class})"));
        false
    }

    /// Every tool, with its own arguments and, unless `confirm`, without any.
    async fn pass(
        &mut self,
        client: &Client,
        schemas: &[(String, Map<String, Value>)],
        targets: &Targets,
        confirm: bool,
    ) {
        for (tool, schema) in schemas {
            if !confirm {
                self.call(client, tool, &json!({}), false, targets).await;
            }
            let arguments = args::arguments(tool, schema, targets).expect("checked above");
            self.call(client, tool, &arguments, confirm, targets).await;
        }
    }
}

/// The JSON of an answer's first text block.
fn body(answer: &Answer) -> Value {
    let whole: Value = serde_json::from_str(&answer.text).unwrap_or_default();
    whole["content"][0]["text"]
        .as_str()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default()
}

/// The core tools first, each confirmed: a terminal on the test server, typed into and read,
/// a command run there, the workspace that shows the terminal applied. Then every tool,
/// confirmed where the sweep may.
async fn confirmed_pass(
    sweep: &mut Sweep,
    client: &Client,
    schemas: &[(String, Map<String, Value>)],
    targets: &mut Targets,
) {
    let schema_of = |tool: &str| {
        schemas
            .iter()
            .find(|(name, _)| name == tool)
            .map(|(_, schema)| schema.clone())
            .expect("listed")
    };
    sweep.reached.clear();
    let open = args::arguments("terminal_open", &schema_of("terminal_open"), targets)
        .expect("checked above");
    let opened = sweep
        .call(client, "terminal_open", &open, true, targets)
        .await;
    assert!(opened.ok, "a terminal on the test server: {}", opened.text);
    targets.terminal = body(&opened)["terminal"].as_u64();
    let captured = sweep
        .call(
            client,
            "workspaces_capture",
            &json!({ "name": "sweep" }),
            true,
            targets,
        )
        .await;
    targets.workspace = body(&captured)["id"].as_i64();
    assert!(targets.workspace.is_some(), "{}", captured.text);
    for tool in CORE {
        let arguments = args::arguments(tool, &schema_of(tool), targets).expect("checked above");
        sweep.call(client, tool, &arguments, true, targets).await;
    }
    let reached = CORE
        .iter()
        .filter(|tool| sweep.reached.contains(**tool))
        .count();
    assert_eq!(
        reached,
        CORE.len(),
        "the core tools reached: {:?}",
        sweep.reached
    );
    sweep.pass(client, schemas, targets, true).await;
    assert!(
        sweep.unknown.is_empty(),
        "tools that ask and that the sweep neither confirms nor keeps from the machine: {:?}",
        sweep.unknown
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_canary_leaks_from_any_tool_transport_event_log_file_or_table() {
    let log = checks::capture_logs();
    let (scene, client) = setup::scene().await;
    let secrets = setup::secret_needles();
    let token = McpToken::load_or_create(&scene.db).await.expect("token");
    let token_needles = Needles::of(&[token.expose()]);
    let every = secrets.clone().with(&token_needles);
    assert!(
        every.len() > 100,
        "every form of every canary: {}",
        every.len()
    );
    let mut targets = scene.targets.clone();
    let mut sweep = Sweep::default();

    // Real logins with the stored secrets.
    let web = targets.web;
    for (tool, arguments) in [
        ("list_dir", json!({ "path": "sftp://web/" })),
        ("list_dir", json!({ "path": "sftp://team/" })),
        ("list_dir", json!({ "path": "sftp://keyed/" })),
        ("files_read_text", json!({ "path": args::REMOTE_FILE })),
        ("connections_test", json!({ "id": scene.keyed })),
        ("connections_connect", json!({ "id": web })),
        ("db_backup", json!({})),
    ] {
        sweep.call(&client, tool, &arguments, false, &targets).await;
    }
    assert!(
        sweep.answers[1].1.contains("notes.txt"),
        "the shared credential logs in: {}",
        sweep.answers[1].1
    );

    // Every tool's arguments come from its schema; one the generator cannot fill fails here.
    let listed = client.list_all_tools().await.expect("tools");
    assert!(listed.len() > 100, "the whole tool list: {}", listed.len());
    let schemas: Vec<(String, Map<String, Value>)> = listed
        .iter()
        .filter(|tool| tool.name != "vault_lock")
        .map(|tool| (tool.name.to_string(), (*tool.input_schema).clone()))
        .collect();
    let unfillable: Vec<String> = schemas
        .iter()
        .filter_map(|(tool, schema)| args::arguments(tool, schema, &targets).err())
        .collect();
    assert!(
        unfillable.is_empty(),
        "tools the sweep cannot make arguments for: {unfillable:?}"
    );

    // Unlocked, in process.
    sweep.pass(&client, &schemas, &targets, false).await;
    // Unlocked, over HTTP with the token; the token asked for; the HTTP server's own answers.
    let http = transport::serve(&scene.context, &token).await;
    let over_http = transport::over_http(&http, &token).await;
    for (tool, arguments) in [
        ("settings_get", json!({ "key": rc_mcp::TOKEN_KEY })),
        ("settings_get", json!({})),
        (
            "settings_set",
            json!({ "key": rc_mcp::TOKEN_KEY, "value": "x" }),
        ),
    ] {
        sweep
            .call(&over_http, tool, &arguments, false, &targets)
            .await;
    }
    sweep.pass(&over_http, &schemas, &targets, false).await;
    let raw = transport::raw_answers(&http, &token).await;
    assert!(
        raw.iter().any(|answer| answer.starts_with("401")),
        "the refusals were collected: {raw:?}"
    );
    sweep
        .answers
        .extend(raw.into_iter().map(|answer| ("http".to_owned(), answer)));
    // Locked, the same again.
    sweep
        .call(&client, "vault_lock", &json!({}), false, &targets)
        .await;
    sweep.pass(&client, &schemas, &targets, false).await;
    // Unlocked again (the person's way: MCP cannot), every code redeemed the sweep may redeem.
    scene
        .engine
        .dispatch(Command::UnlockVault {
            password: SecretParam::from(setup::MASTER),
        })
        .await
        .expect("unlocked");
    confirmed_pass(&mut sweep, &client, &schemas, &mut targets).await;

    for (tool, answer) in &sweep.answers {
        let leaked = every.found(answer.as_bytes());
        assert!(leaked.is_empty(), "{tool} answered {leaked:?}");
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let events = scene.events();
    assert!(!events.is_empty(), "the events were captured");
    for event in &events {
        assert!(
            every.found(event.as_bytes()).is_empty(),
            "an event leaks: {event}"
        );
    }
    let logged = log.bytes();
    assert!(
        String::from_utf8_lossy(&logged).contains("secret stored"),
        "the log was captured"
    );
    assert!(
        every.found(&logged).is_empty(),
        "the log leaks {:?}",
        every.found(&logged)
    );

    // Each check finds a leak planted on purpose: an answer, ...
    let planted_text = forms(setup::MASTER)[2].clone();
    let planted = scene.work.path().join("planted.txt");
    std::fs::write(&planted, &planted_text).expect("write");
    let read = transport::call(
        &client,
        "files_read_text",
        &json!({ "path": planted.display().to_string() }),
    )
    .await;
    assert_eq!(
        every.found(read.text.as_bytes()),
        std::slice::from_ref(&planted_text),
        "the answer check: {}",
        read.text
    );
    // ... a log line, the token's included ...
    tracing::warn!(
        "planted {}",
        encodings::url_encoded(setup::PASSWORD.as_bytes(), true)
    );
    tracing::warn!(
        "planted {}",
        encodings::hex(token.expose().as_bytes(), true)
    );
    let logged = log.bytes();
    assert!(!secrets.found(&logged).is_empty(), "the log check");
    assert!(!token_needles.found(&logged).is_empty(), "the token check");

    over_http.cancel().await.expect("closes");
    http.shutdown().await.expect("stops");
    scene.db.close().await;
    assert!(
        scene.data.path().join(rc_db::BACKUP_DIR).is_dir(),
        "db_backup wrote its copy"
    );
    let leaking = checks::leaking_files(scene.data.path(), &secrets);
    assert!(leaking.is_empty(), "files leak: {leaking:?}");
    // ... a table, any table: the token is in the settings, a planted table is read too ...
    let file = scene.data.path().join(DB_FILE);
    let scan = checks::scan_tables(&file, &secrets).await;
    assert!(scan.leaks.is_empty(), "tables leak: {:?}", scan.leaks);
    assert!(scan.tables.len() > 10, "every table: {:?}", scan.tables);
    let token_scan = checks::scan_tables(&file, &token_needles).await;
    assert!(
        token_scan
            .leaks
            .iter()
            .any(|(column, _)| column.starts_with("settings.")),
        "the table check reads the values: {:?}",
        token_scan.leaks
    );
    let copy = scene.work.path().join("planted.db");
    let planted_row = encodings::base64_groups(&setup::PASSWORD.as_bytes()[1..]);
    checks::plant_table(&file, &copy, &planted_row).await;
    let planted_scan = checks::scan_tables(&copy, &secrets).await;
    assert_eq!(
        planted_scan
            .leaks
            .iter()
            .map(|(column, _)| column.as_str())
            .collect::<Vec<_>>(),
        ["planted_by_the_sweep.note"],
        "the table check reads a table no migration made"
    );
    // ... and a file.
    std::fs::write(scene.data.path().join("planted.txt"), &planted_text).expect("write");
    assert_eq!(
        checks::leaking_files(scene.data.path(), &secrets).len(),
        1,
        "the file check"
    );

    scene.drain.abort();
    drop(client);
    drop(scene.context);
    drop(scene.engine);
}
