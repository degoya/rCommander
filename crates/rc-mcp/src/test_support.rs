//! What the server's tests share (RC-080-24, RC-080-28): the set-up before the first call - a
//! temporary root, a headless engine on it, a database and the context over both, with the
//! deviations several files take ([`Fixture`]) - an rmcp client over an in-process pipe, and one
//! tool call read back as JSON.
//!
//! The integration tests under `tests/` include this file by `#[path]`: the two server types
//! come from `super`, the crate root in the library and the test's root there.

#![allow(
    dead_code,
    reason = "shared by the unit tests and, by path, the integration tests; each file uses a part"
)]

use std::{fs, path::Path, sync::Arc, time::Duration};

use rc_db::{Db, SECRET_COLUMNS};
use rc_engine::{
    AgentSource, Command, Engine, SecretParam, VfsPath, settings::MCP_HEADLESS_CODE_KEY,
};
use rc_vault::{KdfParams, ManualClock, Vault, VaultOptions};
use rmcp::{RoleClient, ServiceExt as _, model::CallToolRequestParams, service::RunningService};
use serde_json::Value;
use tempfile::TempDir;

use super::{McpContext, RcMcpServer};

/// The master password [`Fixture::with_created_vault`] sets.
pub(crate) const MASTER_PASSWORD: &str = "correct horse battery";

/// A new file `path` holding `text`, hidden as the platform hides it: a leading dot does on Unix,
/// on Windows only the hidden attribute does (a dot marks nothing there, as in Total Commander).
pub(crate) fn hidden_file(path: &Path, text: &str) {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        std::os::windows::fs::OpenOptionsExt::attributes(&mut options, FILE_ATTRIBUTE_HIDDEN);
    }
    let mut file = options.open(path).expect("create");
    std::io::Write::write_all(&mut file, text.as_bytes()).expect("write");
}

/// The set-up most test files take: a temporary root, a headless engine on it, an in-memory
/// database and the context over both. The `with_*` steps are the deviations several files
/// share; what one file alone needs stays in that file, on the [`Built`] parts.
pub(crate) struct Fixture {
    root: TempDir,
    db: Option<Db>,
    codes: bool,
    vault: VaultWanted,
    agent_sources: Option<Vec<AgentSource>>,
}

#[derive(Clone, Copy)]
enum VaultWanted {
    None,
    Attached,
    Created,
}

impl Fixture {
    /// A fresh temporary root; the engine starts on it in [`Fixture::build`].
    pub(crate) fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("temp dir"),
            db: None,
            codes: false,
            vault: VaultWanted::None,
            agent_sources: None,
        }
    }

    /// The root the engine will start on: the fixture files go here before `build`.
    pub(crate) fn root(&self) -> &Path {
        self.root.path()
    }

    /// `db` in place of the in-memory one: a database file the test reads back.
    pub(crate) fn with_db(mut self, db: Db) -> Self {
        self.db = Some(db);
        self
    }

    /// The person allowed confirmation codes: headless, a code stands in for the answer.
    pub(crate) fn with_codes(mut self) -> Self {
        self.codes = true;
        self
    }

    /// A vault that derives in milliseconds, attached and still absent: the test creates it.
    pub(crate) fn with_vault(mut self) -> Self {
        self.vault = VaultWanted::Attached;
        self
    }

    /// The vault attached and created with [`MASTER_PASSWORD`], so unlocked.
    pub(crate) fn with_created_vault(mut self) -> Self {
        self.vault = VaultWanted::Created;
        self
    }

    /// Where `agent.list_identities` looks, in place of the platform's agents.
    pub(crate) fn with_agent_sources(mut self, sources: Vec<AgentSource>) -> Self {
        self.agent_sources = Some(sources);
        self
    }

    /// The engine, the database and the context, in the order the files took them.
    pub(crate) async fn build(self) -> Built {
        let db = match self.db {
            Some(db) => db,
            None => Db::open_in_memory().await.expect("db"),
        };
        if self.codes {
            db.set(MCP_HEADLESS_CODE_KEY, &true)
                .await
                .expect("the person allowed codes");
        }
        let engine = headless(self.root.path()).await;
        assert!(
            engine.attach_db(db.clone()),
            "a fresh engine takes the database"
        );
        if let Some(sources) = self.agent_sources {
            engine.attach_agent_sources(sources);
        }
        if !matches!(self.vault, VaultWanted::None) {
            let options = VaultOptions {
                clock: Arc::new(ManualClock::new()),
                kdf: KdfParams {
                    m_cost_kib: 64,
                    t_cost: 1,
                    p_cost: 1,
                },
                failure_delay: Duration::ZERO,
                secret_columns: SECRET_COLUMNS,
            };
            engine.attach_vault(Vault::open_with(db.clone(), options).await.expect("vault"));
        }
        if matches!(self.vault, VaultWanted::Created) {
            engine
                .dispatch(Command::CreateVault {
                    password: SecretParam::from(MASTER_PASSWORD),
                })
                .await
                .expect("vault created");
        }
        let context = McpContext::new(engine.clone(), db.clone());
        Built {
            root: self.root,
            engine,
            db,
            context,
        }
    }
}

/// What [`Fixture::build`] hands out; `root` lives as long as the test holds it.
pub(crate) struct Built {
    pub(crate) root: TempDir,
    pub(crate) engine: Engine,
    pub(crate) db: Db,
    pub(crate) context: McpContext,
}

impl Built {
    /// A server on the context, called directly.
    pub(crate) fn server(&self) -> RcMcpServer {
        RcMcpServer::new(self.context.clone())
    }

    /// A client connected to a server on the context, see [`connect`].
    pub(crate) async fn client(&self) -> RunningService<RoleClient, ()> {
        connect(&self.context).await
    }
}

/// A headless engine on `root`, both panels there.
pub(crate) async fn headless(root: &Path) -> Engine {
    Engine::headless(VfsPath::local(root))
        .await
        .expect("engine")
}

/// A client connected to a server on `context` over an in-process pipe.
pub(crate) async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// `tool` called with `arguments`: its first text content, parsed as JSON.
pub(crate) async fn call(
    client: &RunningService<RoleClient, ()>,
    tool: &str,
    arguments: Value,
) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}
