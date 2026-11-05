//! The two ways into the server the sweep takes (RC-080-61): an in-process pipe, and the
//! Streamable HTTP transport on 127.0.0.1 with its bearer token. Besides the tool calls, the
//! HTTP server's own answers - refusals without, with a wrong or with the right token, a foreign
//! `Host` or `Origin`, a body that is no JSON-RPC, the health probe - are collected, so the token
//! is looked for in them as well.

use std::time::Duration;

use rc_mcp::{HttpHandle, McpContext, McpHttpConfig, McpToken, RcMcpServer, serve_http};
use rmcp::{
    RoleClient, ServiceExt as _,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::Value;

/// How long one call may take; a call that takes longer is dropped, the sweep goes on.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

pub type Client = RunningService<RoleClient, ()>;

/// A client on `context` over an in-process pipe.
pub async fn in_process(context: &McpContext) -> Client {
    let (server_side, client_side) = tokio::io::duplex(256 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// The HTTP server on `context` with `token`, on a free port, without an instance file.
pub async fn serve(context: &McpContext, token: &McpToken) -> HttpHandle {
    serve_http(
        context.clone(),
        McpHttpConfig {
            port: 0,
            token: token.clone(),
            instance_file: None,
        },
    )
    .await
    .expect("serves")
}

/// A client of `handle` that sends `token`.
pub async fn over_http(handle: &HttpHandle, token: &McpToken) -> Client {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(handle.mcp_url())
            .auth_header(token.expose().to_owned()),
    );
    ().serve(transport).await.expect("client over HTTP")
}

/// One call's answer.
pub struct Answer {
    /// The whole answer as text: error answers and structured content included; a call that
    /// failed at the protocol level or took too long, its error.
    pub text: String,
    /// The tool ran and answered without an error.
    pub ok: bool,
    /// The code the answer asks to be confirmed with, and the class it names.
    pub question: Option<(String, String)>,
}

/// `tool` with `arguments`.
pub async fn call(client: &Client, tool: &str, arguments: &Value) -> Answer {
    let request = CallToolRequestParams::new(tool.to_owned())
        .with_arguments(arguments.as_object().cloned().unwrap_or_default());
    match tokio::time::timeout(CALL_TIMEOUT, client.call_tool(request)).await {
        Ok(Ok(answer)) => read(&answer),
        Ok(Err(error)) => failed(error.to_string()),
        Err(_) => failed(format!("{tool} timed out")),
    }
}

fn failed(text: String) -> Answer {
    Answer {
        text,
        ok: false,
        question: None,
    }
}

fn read(answer: &CallToolResult) -> Answer {
    let body: Option<Value> = answer
        .content
        .first()
        .and_then(|content| content.as_text())
        .and_then(|text| serde_json::from_str(&text.text).ok());
    let question = body
        .as_ref()
        .filter(|body| body["confirmation_required"] == true)
        .and_then(|body| {
            let code = body["confirmation"].as_str()?;
            let class = body["class"].as_str()?;
            Some((code.to_owned(), class.to_owned()))
        });
    Answer {
        text: serde_json::to_string(answer).expect("serializes"),
        ok: answer.is_error != Some(true) && question.is_none(),
        question,
    }
}

/// The HTTP server's own answers, status, headers and body each: refusals, a broken request
/// with the token, the health probe.
pub async fn raw_answers(handle: &HttpHandle, token: &McpToken) -> Vec<String> {
    const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"sweep","version":"1"}}}"#;
    let bearer = format!("Bearer {}", token.expose());
    let wrong = format!("Bearer {}x", token.expose());
    let url = handle.mcp_url();
    let with_token_in_query = format!("{url}?token={}", token.expose());
    let health = format!("http://{}/health?nonce={}", handle.addr(), "ab".repeat(16));
    let client = reqwest::Client::new();
    let requests = [
        client.post(&url).body(INITIALIZE),
        client
            .post(&url)
            .header("authorization", &wrong)
            .body(INITIALIZE),
        client.post(&with_token_in_query).body(INITIALIZE),
        client
            .post(&url)
            .header("authorization", &bearer)
            .header("host", "attacker.example")
            .body(INITIALIZE),
        client
            .post(&url)
            .header("authorization", &bearer)
            .header("origin", "http://evil.example")
            .body(INITIALIZE),
        client
            .post(&url)
            .header("authorization", &bearer)
            .body("not json-rpc"),
        client.get(&url).header("authorization", &bearer),
        client.delete(&url).header("authorization", &bearer),
        client.get(&health),
        client.get(format!("http://{}/elsewhere", handle.addr())),
    ];
    let mut answers = Vec::new();
    for request in requests {
        let request = request
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .timeout(CALL_TIMEOUT);
        answers.push(match request.send().await {
            Ok(response) => {
                let head = format!("{} {:?}", response.status(), response.headers());
                let body = match response.text().await {
                    Ok(body) => body,
                    Err(error) => error.without_url().to_string(),
                };
                format!("{head} {body}")
            }
            // The URL that carries the token in its query is the test's own, not an answer.
            Err(error) => error.without_url().to_string(),
        });
    }
    answers
}
