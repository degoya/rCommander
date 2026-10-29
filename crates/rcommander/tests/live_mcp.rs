//! `rcommander mcp` served headless against a real OpenSSH server in Docker (RC-080-14): the
//! binary itself, started as an agent starts it, with its own data folder and no window. What a
//! person decides is done beforehand by the engine as the window does it (the host key trusted
//! and saved; for `ssh_exec` the setting that allows confirmation codes without a window);
//! everything after that goes over MCP's stdio: the connection tree, a listing of the server,
//! a copy up and one down byte for byte, `ssh_exec` refused without a person and run with the
//! code, and a server whose key nobody trusted refused.
//!
//! Every test is `#[ignore]` and needs the servers: `scripts/live/sftp/run.sh` starts them and
//! runs this target beside `live_sftp`; without them a test says so and returns.

use std::{
    path::{Path, PathBuf},
    process::{Command as Process, Stdio},
    time::{Duration, Instant},
};

use rc_db::Db;
use rc_engine::{
    AuthMethod, Command, ConnectionDraft, Engine, HostKeyAnswer, Outcome, VfsPath,
    settings::MCP_HEADLESS_CODE_KEY,
};
use rmcp::{RoleClient, ServiceExt as _, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

/// The server every test here uses (`a` of `scripts/live/sftp/ctl.sh`, the first port).
const ROLE: &str = "a";
/// A server whose host key no test here trusts (`b`, the second port).
const UNTRUSTED: &str = "b";
const CONNECTION: &str = "live-mcp";

/// What `run.sh` exports.
struct Live {
    ctl: PathBuf,
    host: String,
    user: String,
    base: u16,
}

fn live() -> Option<Live> {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let Some(ctl) = var("RC_LIVE_SFTP_CTL") else {
        eprintln!("skipped: RC_LIVE_SFTP_CTL is not set (scripts/live/sftp/run.sh sets it)");
        return None;
    };
    Some(Live {
        ctl: PathBuf::from(ctl),
        host: var("RC_LIVE_SFTP_HOST").unwrap_or_else(|| "127.0.0.1".to_owned()),
        user: var("RC_LIVE_SFTP_USER").unwrap_or_else(|| "tester".to_owned()),
        base: var("RC_LIVE_SFTP_PORT_BASE").map_or(2201, |port| port.parse().expect("a port")),
    })
}

impl Live {
    fn port(&self, role: &str) -> u16 {
        match role {
            ROLE => self.base,
            UNTRUSTED => self.base + 1,
            other => panic!("no port for {other}"),
        }
    }

    /// `ctl.sh` with `args`; its standard output.
    fn ctl(&self, args: &[&str]) -> String {
        let output = Process::new(&self.ctl)
            .args(args)
            .output()
            .expect("ctl.sh runs");
        assert!(
            output.status.success(),
            "ctl.sh {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("text")
    }

    fn exec(&self, command: &str) -> String {
        self.ctl(&["exec", ROLE, command])
    }
}

/// One test's world: a home with the data folder `rcommander mcp` opens, a key pair whose
/// public half the server accepts, and a folder on the server.
struct World {
    home: TempDir,
    key: PathBuf,
    remote: String,
}

impl World {
    async fn new(live: &Live, name: &str) -> Self {
        let home = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a home");
        std::fs::create_dir_all(home.path().join("work")).expect("work folder");
        let key = home.path().join("id_live");
        let made = Process::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", name, "-f"])
            .arg(&key)
            .status()
            .expect("ssh-keygen runs");
        assert!(made.success(), "ssh-keygen");
        let public = std::fs::read_to_string(key.with_extension("pub")).expect("public key");
        let user = &live.user;
        live.exec(&format!(
            "install -d -m 700 -o {user} -g {user} /home/{user}/.ssh \
             && echo '{}' >> /home/{user}/.ssh/authorized_keys \
             && chown {user}:{user} /home/{user}/.ssh/authorized_keys \
             && chmod 600 /home/{user}/.ssh/authorized_keys",
            public.trim()
        ));
        let remote = format!("/home/{user}/{name}");
        live.exec(&format!(
            "rm -rf '{remote}' && mkdir -p '{remote}' && echo hello > '{remote}/seen.txt' \
             && chown -R {user}:{user} '{remote}'"
        ));
        Self { home, key, remote }
    }

    /// Where `rcommander mcp` looks for its database with `XDG_DATA_HOME` set to the home's
    /// `data` (`directories`' project path on Linux).
    fn db_path(&self) -> PathBuf {
        self.home.path().join("data/rcommander/rcommander.db")
    }

    fn work(&self) -> PathBuf {
        self.home.path().join("work")
    }

    /// What a person does in the window first: the connection with its key file saved, the
    /// unknown host key trusted and saved. The database stays open for later settings.
    async fn as_the_window(&self, live: &Live) -> Db {
        std::fs::create_dir_all(self.db_path().parent().expect("a parent")).expect("data folder");
        let db = Db::open(self.db_path()).await.expect("db");
        let engine = Engine::headless(VfsPath::local(self.work()))
            .await
            .expect("engine");
        assert!(engine.attach_db(db.clone()));
        engine.attach_agent_sources(Vec::new());
        let draft = ConnectionDraft {
            port: Some(i64::from(live.port(ROLE))),
            username: Some(live.user.clone()),
            auth_method: AuthMethod::Key,
            key_path: Some(self.key.to_string_lossy().into_owned()),
            ..ConnectionDraft::new(CONNECTION, live.host.clone())
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
        let first = engine
            .dispatch(Command::ConnectSession { id: connection.id })
            .await
            .expect_err("an unknown key first");
        assert_eq!(first.code(), "host_key_unknown", "{first}");
        let question = engine.host_key_questions().pop().expect("a question");
        engine
            .dispatch(Command::AnswerHostKey {
                question: question.id,
                answer: HostKeyAnswer::TrustAndSave,
            })
            .await
            .expect("trusted");
        engine
            .dispatch(Command::ConnectSession { id: connection.id })
            .await
            .expect("the key file logs in");
        db
    }

    /// `rcommander mcp` in the work folder, with this home's data folder and no SSH agent, and
    /// an MCP client on its stdio.
    async fn mcp(&self) -> (RunningService<RoleClient, ()>, tokio::process::Child) {
        let home = self.home.path();
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_rcommander"))
            .arg("mcp")
            .current_dir(self.work())
            .env("HOME", home)
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env_remove("SSH_AUTH_SOCK")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .expect("rcommander mcp starts");
        let stdout = child.stdout.take().expect("stdout");
        let stdin = child.stdin.take().expect("stdin");
        let client = ().serve((stdout, stdin)).await.expect("an MCP session");
        (client, child)
    }
}

/// A tool's answer as JSON, and whether it was an error.
async fn call(
    client: &RunningService<RoleClient, ()>,
    tool: &str,
    arguments: Value,
) -> (Value, bool) {
    let result = client
        .call_tool(
            CallToolRequestParams::new(tool.to_owned())
                .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
        )
        .await
        .expect("the call is answered");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("a text answer");
    let value = serde_json::from_str(&text).unwrap_or_else(|_| panic!("{tool}: not JSON: {text}"));
    (value, result.is_error.unwrap_or(false))
}

async fn ok(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let (value, error) = call(client, tool, arguments).await;
    assert!(!error, "{tool} failed: {value}");
    value
}

/// Follows a queued job with `ops_list` until it is over; asserts it completed without failures.
/// `ops_copy` answers with the job, `ops_list` with `{"jobs": [...]}`.
async fn completes(client: &RunningService<RoleClient, ()>, queued: &Value) {
    let job = queued["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a job: {queued}"));
    let started = Instant::now();
    loop {
        let listed = ok(client, "ops_list", json!({ "job": job })).await;
        let snapshot = &listed["jobs"][0];
        assert_eq!(snapshot["id"], job, "{listed}");
        let state = snapshot["state"].as_str().unwrap_or_default().to_owned();
        if matches!(state.as_str(), "completed" | "cancelled" | "failed") {
            assert_eq!(state, "completed", "{snapshot}");
            assert_eq!(snapshot["failures"], json!([]), "{snapshot}");
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "never over: {snapshot}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn sha256(path: &Path) -> String {
    let output = Process::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum");
    assert!(output.status.success(), "sha256sum {}", path.display());
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .expect("a digest")
        .to_owned()
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn headless_mcp_lists_and_copies_both_ways_and_refuses_an_untrusted_server() {
    let Some(live) = live() else { return };
    let world = World::new(&live, "t-mcp-copy").await;
    let _db = world.as_the_window(&live).await;
    let (client, _child) = world.mcp().await;

    let tree = ok(&client, "connections_list", json!({})).await;
    let saved = tree.to_string();
    assert!(
        saved.contains(CONNECTION),
        "the headless server opened another database than {}: {tree}",
        world.db_path().display()
    );
    let keys = ok(&client, "hostkeys_list", json!({})).await;
    assert!(
        keys.to_string().contains(&live.port(ROLE).to_string()),
        "{keys}"
    );

    let remote = format!("sftp://{CONNECTION}{}", world.remote);
    let listing = ok(&client, "list_dir", json!({ "path": remote })).await;
    assert!(listing.to_string().contains("seen.txt"), "{listing}");

    // Up: a file of the work folder into the server's folder; the server's own sha256sum.
    let local = world.work().join("up.bin");
    // 4 MiB that do not repeat in short runs.
    let bytes: Vec<u8> = (0..1024 * 1024_u32)
        .flat_map(|i| i.wrapping_mul(2_654_435_761).to_le_bytes())
        .collect();
    std::fs::write(&local, &bytes).expect("a local file");
    let up = ok(
        &client,
        "ops_copy",
        json!({ "sources": [local.to_string_lossy()], "target": remote }),
    )
    .await;
    completes(&client, &up).await;
    let on_server = live.ctl(&["sha256", ROLE, &format!("{}/up.bin", world.remote)]);
    assert_eq!(on_server.trim(), sha256(&local));

    // Down: the server's file into a local folder, byte for byte.
    let down_dir = world.work().join("down");
    std::fs::create_dir_all(&down_dir).expect("a local folder");
    let down = ok(
        &client,
        "ops_copy",
        json!({ "sources": [format!("{remote}/up.bin")], "target": down_dir.to_string_lossy() }),
    )
    .await;
    completes(&client, &down).await;
    assert_eq!(sha256(&down_dir.join("up.bin")), sha256(&local));

    // A connection the agent saves itself, to a server whose key nobody trusted: no question
    // can be answered over MCP, so the test is refused.
    let created = ok(
        &client,
        "connections_create",
        json!({
            "name": "live-mcp-untrusted",
            "host": live.host,
            "port": live.port(UNTRUSTED),
            "username": live.user,
            "auth_method": "key",
            "key_path": world.key.to_string_lossy(),
        }),
    )
    .await;
    let id = created["connection"]["id"]
        .as_i64()
        .or_else(|| created["id"].as_i64())
        .unwrap_or_else(|| panic!("an id: {created}"));
    let (refused, error) = call(&client, "connections_test", json!({ "id": id })).await;
    assert!(error, "{refused}");
    assert_eq!(refused["code"], "host_key_unknown", "{refused}");

    client.cancel().await.expect("the session closes");
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn headless_ssh_exec_is_refused_without_a_person_and_runs_with_the_code() {
    let Some(live) = live() else { return };
    let world = World::new(&live, "t-mcp-exec").await;
    let db = world.as_the_window(&live).await;
    let (client, _child) = world.mcp().await;

    let tree = ok(&client, "connections_list", json!({})).await;
    let id = tree["connections"]
        .as_array()
        .and_then(|all| all.iter().find(|c| c["name"] == CONNECTION))
        .and_then(|connection| connection["id"].as_i64())
        .unwrap_or_else(|| panic!("{CONNECTION} in {tree}"));
    let arguments =
        json!({ "connection_id": id, "command": format!("cat {}/seen.txt", world.remote) });

    let (refused, error) = call(&client, "ssh_exec", arguments.clone()).await;
    assert!(error, "{refused}");
    assert_eq!(refused["code"], "headless_ask_denied", "{refused}");

    // The person allows confirmation codes without a window (Settings > MCP).
    db.set(MCP_HEADLESS_CODE_KEY, &true)
        .await
        .expect("the setting");
    let question = ok(&client, "ssh_exec", arguments.clone()).await;
    assert_eq!(question["confirmation_required"], true, "{question}");
    let code = question["confirmation"]
        .as_str()
        .expect("a code")
        .to_owned();
    let mut confirmed = arguments;
    confirmed["confirmation"] = json!(code);
    let ran = ok(&client, "ssh_exec", confirmed.clone()).await;
    assert_eq!(ran["exit_code"], 0, "{ran}");
    assert_eq!(
        ran["stdout"].as_str().map(str::trim),
        Some("hello"),
        "{ran}"
    );
    // The code is spent.
    let (again, _) = call(&client, "ssh_exec", confirmed).await;
    assert_ne!(
        again["stdout"].as_str().map(str::trim),
        Some("hello"),
        "{again}"
    );

    client.cancel().await.expect("the session closes");
}
