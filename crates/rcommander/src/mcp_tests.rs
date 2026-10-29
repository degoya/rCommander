//! The acceptance check of RC-010-01: an agent's `navigate`, over HTTP and over stdio, moves the
//! panel the window shows.

use std::{fs, path::PathBuf};

use rc_db::Db;
use rc_engine::PanelId;
use rc_mcp::{HttpHandle, McpToken, instance, proxy_io};
use rmcp::{
    RoleClient, ServiceExt as _,
    model::CallToolRequestParams,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::json;
use tempfile::TempDir;

use rc_mcp::config::{ENABLED_KEY, PORT_KEY};

use super::{McpServer, McpStatus, restart, start};
use crate::app::harness::Harness;
use crate::app::harness::headless_engine;

/// The app over `root/` (with `docs/a.txt`), its MCP server started as the window starts it.
struct Running {
    root: TempDir,
    harness: Harness,
    db: Db,
    handle: HttpHandle,
}

impl Running {
    fn instance_file(&self) -> PathBuf {
        self.root.path().join("data").join(instance::INSTANCE_FILE)
    }

    fn docs(&self) -> PathBuf {
        self.root.path().join("docs")
    }

    fn left_path(&self) -> PathBuf {
        self.harness.app.pane(PanelId::Left).state.path.path.clone()
    }
}

async fn running() -> Running {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("docs")).expect("mkdir");
    fs::write(root.path().join("docs").join("a.txt"), b"a").expect("write");
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    let harness = Harness::new(engine.clone());
    let instance_file = root.path().join("data").join(instance::INSTANCE_FILE);
    let (handle, status) = start(engine, db.clone(), Some(instance_file)).await;
    let handle = handle.expect("the server runs");
    assert_eq!(status, McpStatus::Serving(handle.addr()));
    Running {
        root,
        harness,
        db,
        handle,
    }
}

async fn navigate(client: &RunningService<RoleClient, ()>, path: &str) {
    let arguments = json!({ "path": path });
    // Whether it worked shows in the panel; that is what the tests look at.
    let _answer = client
        .call_tool(
            CallToolRequestParams::new("navigate".to_owned())
                .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
        )
        .await
        .expect("navigate answers");
}

#[tokio::test]
async fn an_agents_navigate_over_http_moves_the_panel_on_screen() {
    let mut running = running().await;
    assert_eq!(running.left_path(), running.root.path());
    let token = McpToken::load_or_create(&running.db)
        .await
        .expect("the server's token");
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(running.handle.mcp_url())
            .auth_header(token.expose().to_owned()),
    );
    let client = ().serve(transport).await.expect("client");
    navigate(&client, "docs").await;

    running.harness.settle().await;
    assert_eq!(running.left_path(), running.docs());
    assert_eq!(
        running.harness.app.pane(PanelId::Left).entries()[0].name,
        "a.txt"
    );
    client.cancel().await.expect("closes");
    running.handle.shutdown().await.expect("stops");
}

/// `rcommander mcp` from WSL forwards stdio to the running app; this is that path.
#[tokio::test]
async fn an_agents_navigate_over_stdio_moves_the_panel_on_screen() {
    let mut running = running().await;
    let target = instance::read(&running.instance_file()).expect("the instance file");
    let (client_side, proxy_side) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(proxy_side);
    let proxy = tokio::spawn(async move { proxy_io(&target, read, write).await });
    let client = ().serve(client_side).await.expect("client");
    navigate(&client, "docs").await;

    running.harness.settle().await;
    assert_eq!(running.left_path(), running.docs());
    client.cancel().await.expect("closes");
    proxy.await.expect("joins").expect("the proxy ends cleanly");
    let instance_file = running.instance_file();
    running.handle.shutdown().await.expect("stops");
    assert!(!instance_file.exists(), "the instance file goes with it");
}

#[tokio::test]
async fn the_setting_switches_the_server_off() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    db.set(ENABLED_KEY, &false).await.expect("stored");
    let (handle, status) = start(engine, db, None).await;
    assert!(handle.is_none());
    assert_eq!(status, McpStatus::Off);
}

#[tokio::test]
async fn a_port_in_use_leaves_the_app_running_and_says_why() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = taken.local_addr().expect("its address").port();
    let db = Db::open_in_memory().await.expect("db");
    db.set(PORT_KEY, &port).await.expect("stored");
    let (handle, status) = start(engine, db.clone(), None).await;
    assert!(handle.is_none());
    let McpStatus::Failed(reason) = status else {
        panic!("a failure, got {status:?}");
    };
    let token = McpToken::load_or_create(&db).await.expect("token");
    assert!(
        !reason.contains(token.expose()),
        "the reason never carries the token"
    );
}

/// The acceptance check of RC-010-09: switched off in the dialog, the endpoint stops answering
/// at once, without a restart of the app.
#[tokio::test]
async fn switched_off_the_endpoint_stops_answering() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    let (handle, status) = start(engine.clone(), db.clone(), None).await;
    let McpStatus::Serving(addr) = status else {
        panic!("serving, got {status:?}");
    };
    tokio::net::TcpStream::connect(addr)
        .await
        .expect("the endpoint answers");
    let server: McpServer = std::sync::Arc::new(std::sync::Mutex::new(handle));

    db.set(ENABLED_KEY, &false).await.expect("stored");
    let status = restart(server.clone(), engine, db, None).await;
    assert_eq!(status, McpStatus::Off);
    assert!(server.lock().expect("lock").is_none());
    assert!(
        tokio::net::TcpStream::connect(addr).await.is_err(),
        "nothing listens any more"
    );
}
