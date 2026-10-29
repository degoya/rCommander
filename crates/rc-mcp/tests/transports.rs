//! Both transports end to end: the HTTP server on 127.0.0.1:0 with its token, `Host` and
//! `Origin` checks and its instance file, and the stdio proxy forwarding to it.

use std::{fs, path::Path};

use rc_engine::PanelId;
use rc_mcp::{
    HttpHandle, McpContext, McpHttpConfig, McpToken, RcMcpServer, instance, proxy_io, serve_http,
};
use rmcp::{
    RoleClient, ServiceExt as _,
    model::CallToolRequestParams,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use tempfile::TempDir;

#[path = "../src/test_support.rs"]
mod test_support;

use test_support::Fixture;

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#;

struct Running {
    root: TempDir,
    context: McpContext,
    token: McpToken,
    handle: HttpHandle,
}

async fn start() -> Running {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root().join("docs")).expect("mkdir");
    fs::write(fixture.root().join("docs").join("a.txt"), b"a").expect("write");
    let built = fixture.build().await;
    let token = McpToken::load_or_create(&built.db).await.expect("token");
    let handle = serve_http(
        built.context.clone(),
        McpHttpConfig {
            port: 0,
            token: token.clone(),
            instance_file: Some(built.root.path().join("data").join(instance::INSTANCE_FILE)),
        },
    )
    .await
    .expect("serves");
    Running {
        root: built.root,
        context: built.context,
        token,
        handle,
    }
}

fn instance_path(root: &Path) -> std::path::PathBuf {
    root.join("data").join(instance::INSTANCE_FILE)
}

async fn post(running: &Running, headers: &[(&str, &str)]) -> reqwest::Response {
    let mut request = reqwest::Client::new()
        .post(running.handle.mcp_url())
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(INITIALIZE);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.send().await.expect("answers")
}

async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(tool.to_owned())
                .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
        )
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

async fn exercise(client: &RunningService<RoleClient, ()>, running: &Running) {
    let listing = call(client, "list_dir", json!({ "path": "docs" })).await;
    assert_eq!(listing["entries"][0]["name"], "a.txt", "{listing}");
    let moved = call(client, "navigate", json!({ "path": "docs" })).await;
    assert_eq!(moved["entry_count"], 1, "{moved}");
    let state = call(client, "get_panel_state", json!({ "panel": "left" })).await;
    assert_eq!(state["cursor"]["name"], "a.txt", "{state}");
    assert!(
        running
            .context
            .engine()
            .panel(PanelId::Left)
            .path
            .path
            .ends_with("docs")
    );
}

#[tokio::test]
async fn the_tools_work_over_http_with_the_token() {
    let running = start().await;
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(running.handle.mcp_url())
            .auth_header(running.token.expose().to_owned()),
    );
    let client = ().serve(transport).await.expect("client");
    exercise(&client, &running).await;
    client.cancel().await.expect("closes");
    running.handle.shutdown().await.expect("stops");
}

#[tokio::test]
async fn a_request_without_or_with_a_wrong_token_is_refused() {
    let running = start().await;
    let none = post(&running, &[]).await;
    assert_eq!(none.status(), 401);
    assert_eq!(
        none.headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer")
    );
    let body: Value = none.json().await.expect("json");
    assert_eq!(body["code"], "unauthorized");
    assert_eq!(
        post(&running, &[("authorization", "Bearer not-the-token")])
            .await
            .status(),
        401
    );
    let bearer = format!("Bearer {}", running.token.expose());
    assert_eq!(
        post(&running, &[("authorization", bearer.as_str())])
            .await
            .status(),
        200
    );
    // The probe wants no token, only a nonce, and answers a proof that shows none of it.
    let health = |query: &str| {
        reqwest::Client::new()
            .get(format!("http://{}/health{query}", running.handle.addr()))
            .send()
    };
    assert_eq!(health("").await.expect("answers").status(), 400);
    assert_eq!(health("?nonce=xyz").await.expect("answers").status(), 400);
    let nonce = "0123456789abcdef".repeat(2);
    let proof = health(&format!("?nonce={nonce}"))
        .await
        .expect("answers")
        .text()
        .await
        .expect("body");
    assert!(proof.contains("\"proof\""), "{proof}");
    assert!(!proof.contains(running.token.expose()), "{proof}");
}

/// RC-080-12 (M9): after a crash another program may hold the port the instance file names. The
/// probe sends it no token, and an answer that only claims to be rCommander is no rCommander.
#[tokio::test]
async fn the_probe_sends_no_token_to_whoever_holds_the_port() {
    let running = start().await;
    let written = instance::read(&instance_path(running.root.path())).expect("instance");
    running.handle.shutdown().await.expect("stops");

    let impostor = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("binds");
    let stale = instance::Instance {
        port: impostor.local_addr().expect("addr").port(),
        ..written.clone()
    };
    let heard = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let (mut stream, _) = impostor.accept().await.expect("accepts");
        let mut request = vec![0_u8; 8192];
        let read = stream.read(&mut request).await.expect("reads");
        let body = r#"{"app":"rcommander","proof":"00"}"#;
        let answer = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(answer.as_bytes()).await.expect("writes");
        String::from_utf8_lossy(&request[..read]).into_owned()
    });
    assert!(
        !instance::probe(&stale).await,
        "an impostor fails the probe"
    );
    let request = heard.await.expect("joins");
    assert!(request.starts_with("GET /health?nonce="), "{request}");
    assert!(!request.contains(&written.token), "the token stayed home");
    assert!(
        !request.to_ascii_lowercase().contains("authorization"),
        "{request}"
    );
}

#[tokio::test]
async fn a_foreign_origin_or_host_is_refused_even_with_the_token() {
    let running = start().await;
    let bearer = format!("Bearer {}", running.token.expose());
    let foreign = post(
        &running,
        &[
            ("authorization", bearer.as_str()),
            ("origin", "http://evil.example"),
        ],
    )
    .await;
    assert_eq!(foreign.status(), 403);
    let body: Value = foreign.json().await.expect("json");
    assert_eq!(body["code"], "origin_rejected");
    let rebound = post(
        &running,
        &[
            ("authorization", bearer.as_str()),
            ("host", "attacker.example"),
        ],
    )
    .await;
    assert_eq!(rebound.status(), 403);
    let local = post(
        &running,
        &[
            ("authorization", bearer.as_str()),
            ("origin", "http://localhost:3000"),
        ],
    )
    .await;
    assert_eq!(local.status(), 200);
}

#[tokio::test]
async fn the_instance_file_names_the_server_and_goes_with_it() {
    let running = start().await;
    let path = instance_path(running.root.path());
    let written = instance::read(&path).expect("instance file");
    assert_eq!(written.port, running.handle.addr().port());
    assert_eq!(written.token, running.token.expose());
    assert_eq!(written.pid, std::process::id());
    assert!(!format!("{written:?}").contains(running.token.expose()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&path).expect("stat").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(instance::probe(&written).await);
    let wrong = instance::Instance {
        token: "nope".into(),
        ..written.clone()
    };
    assert!(!instance::probe(&wrong).await);

    let root = running.root;
    running.handle.shutdown().await.expect("stops");
    assert!(!instance_path(root.path()).exists());
    assert!(!instance::probe(&written).await);
}

#[tokio::test]
async fn stdio_is_forwarded_to_a_running_instance() {
    let running = start().await;
    let target = instance::read(&instance_path(running.root.path())).expect("instance");
    let (client_side, proxy_side) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(proxy_side);
    let proxy = tokio::spawn(async move { proxy_io(&target, read, write).await });

    let client = ().serve(client_side).await.expect("client");
    exercise(&client, &running).await;
    client.cancel().await.expect("closes");
    proxy.await.expect("joins").expect("the proxy ends cleanly");
    running.handle.shutdown().await.expect("stops");
}

/// The app ends while the agent's client stays connected: the forwarding `rcommander mcp` ends
/// too, so no process is left holding the program's file (owner 2026-10-25).
#[tokio::test]
async fn the_proxy_ends_when_the_running_instance_does() {
    let running = start().await;
    let target = instance::read(&instance_path(running.root.path())).expect("instance");
    let (client_side, proxy_side) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(proxy_side);
    let proxy = tokio::spawn(async move { proxy_io(&target, read, write).await });
    let client = ().serve(client_side).await.expect("client");
    exercise(&client, &running).await;

    running.handle.shutdown().await.expect("stops");
    let ended = tokio::time::timeout(std::time::Duration::from_secs(15), proxy).await;
    assert!(ended.is_ok(), "the proxy still runs after the app ended");
    drop(client);
}
