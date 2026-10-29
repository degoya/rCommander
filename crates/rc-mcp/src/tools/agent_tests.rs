//! The MCP boundary of the SSH agents: key type, comment, fingerprint - never key bytes.

use rc_engine::AgentSource;
use rmcp::model::CallToolResult;
use serde_json::Value;

use crate::RcMcpServer;
use crate::test_support::Fixture;

fn body(result: &CallToolResult) -> (String, Value) {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    let value = serde_json::from_str(&text).expect("json");
    (text, value)
}

async fn server(agent: AgentSource) -> (tempfile::TempDir, RcMcpServer) {
    let built = Fixture::new().with_agent_sources(vec![agent]).build().await;
    let server = built.server();
    (built.root, server)
}

#[tokio::test]
async fn without_an_agent_the_tool_answers_agent_not_found() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_root, server) = server(AgentSource::UnixSocket(dir.path().join("none.sock"))).await;
    let result = server.ssh_agent_identities().await.expect("answer");
    assert_eq!(result.is_error, Some(true));
    let (_, value) = body(&result);
    assert_eq!(value["code"], "agent_not_found");
}

/// An ed25519 key's wire blob: the type and 32 key bytes, which must never reach an answer.
#[cfg(unix)]
fn blob() -> Vec<u8> {
    let mut blob = Vec::new();
    string(&mut blob, b"ssh-ed25519");
    string(&mut blob, &[7_u8; 32]);
    blob
}

#[cfg(unix)]
fn string(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("short");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
}

/// An agent on `path` that answers one listing with [`blob`].
#[cfg(unix)]
fn fake_agent(path: &std::path::Path) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::UnixListener::bind(path).expect("bind");
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut len = [0_u8; 4];
        stream.read_exact(&mut len).await.expect("request length");
        let mut request = vec![0_u8; u32::from_be_bytes(len) as usize];
        stream.read_exact(&mut request).await.expect("request");
        // SSH_AGENT_IDENTITIES_ANSWER with one key.
        let mut answer = vec![12_u8];
        answer.extend_from_slice(&1_u32.to_be_bytes());
        string(&mut answer, &blob());
        string(&mut answer, b"me@box");
        let mut frame = Vec::new();
        string(&mut frame, &answer);
        stream.write_all(&frame).await.expect("answer");
    });
}

#[cfg(unix)]
#[tokio::test]
async fn the_tool_lists_type_comment_and_fingerprint_and_no_key_bytes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    fake_agent(&socket);
    let (_root, server) = server(AgentSource::UnixSocket(socket)).await;
    let result = server.ssh_agent_identities().await.expect("answer");
    assert_ne!(result.is_error, Some(true));
    let (text, value) = body(&result);
    assert_eq!(value["sources"], serde_json::json!(["ssh_auth_sock"]));
    let identity = &value["identities"][0];
    let mut fields: Vec<&str> = identity
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        ["algorithm", "comment", "fingerprint_sha256", "source"]
    );
    assert_eq!(identity["algorithm"], "ssh-ed25519");
    assert_eq!(identity["comment"], "me@box");
    assert!(
        identity["fingerprint_sha256"]
            .as_str()
            .is_some_and(|fingerprint| fingerprint.starts_with("SHA256:"))
    );
    // The base64 of every ed25519 blob starts so; the 32 key bytes are 0x07 each.
    assert!(!text.contains("AAAAC3NzaC1lZDI1NTE5"), "{text}");
    assert!(!text.contains("BwcHBwcHBwcHBwcH"), "{text}");
}

#[test]
fn no_tool_adds_removes_or_signs_with_an_agent_key() {
    let names = RcMcpServer::tool_names();
    assert!(names.contains(&"ssh_agent_identities".to_owned()));
    let agent_tools: Vec<&String> = names.iter().filter(|name| name.contains("agent")).collect();
    assert_eq!(agent_tools, ["ssh_agent_identities"]);
}
