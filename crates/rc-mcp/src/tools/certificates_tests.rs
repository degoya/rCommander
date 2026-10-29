//! The MCP boundary of certificate trust: trusted certificates can be listed - never trusted,
//! accepted or removed, by no tool, the `connections` group's included.

use rc_db::NewTrustedCertificate;
use rc_engine::{CertificateQuestion, Engine, EngineEvent};
use rc_remote::ftp::{CertificateRejection, fingerprint};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Map, Value, json};
use tempfile::TempDir;

use crate::test_support::{Fixture, connect};
use crate::{
    McpContext, RcMcpServer,
    coverage::{COVERAGE, Coverage},
};

const NAS: &[u8] = include_bytes!("../../../rc-remote/tests/fixtures/tls/nas.der");
const OTHER: &[u8] = include_bytes!("../../../rc-remote/tests/fixtures/tls/other.der");

async fn setup() -> (TempDir, McpContext) {
    let built = Fixture::new().with_codes().build().await;
    (built.root, built.context)
}

async fn list(client: &RunningService<RoleClient, ()>) -> Value {
    let result = client
        .call_tool(CallToolRequestParams::new("certificates_list".to_owned()))
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

/// A question about `der` for `nas.example:990`, filed as a refused handshake files it.
async fn pending(engine: &Engine, der: &[u8]) -> CertificateQuestion {
    let mut events = engine.subscribe();
    let _ = engine
        .certificate_refused(CertificateRejection {
            host: "nas.example".to_owned(),
            port: 990,
            fingerprint: fingerprint(der),
            reason: "invalid peer certificate: UnknownIssuer".to_owned(),
            system_verified: false,
            end_entity: der.to_vec(),
        })
        .await;
    loop {
        if let EngineEvent::CertificateQuestion { question } = events.try_recv().expect("an event")
        {
            return *question;
        }
    }
}

#[tokio::test]
async fn only_listing_reaches_mcp() {
    let names = RcMcpServer::tool_names();
    let certificate_tools: Vec<&String> =
        names.iter().filter(|name| name.contains("cert")).collect();
    assert_eq!(certificate_tools, vec!["certificates_list"], "{names:?}");
    for id in ["certificates.answer", "certificates.remove"] {
        assert!(
            COVERAGE
                .iter()
                .any(|(row, coverage)| *row == id && matches!(coverage, Coverage::Omitted(_))),
            "{id} has no MCP tool"
        );
    }
}

/// Every tool of the `connections` group, and the certificate list, called with every argument
/// an answer could carry: the question stays open and nothing is trusted.
#[tokio::test]
async fn no_connections_tool_accepts_a_certificate() {
    let (_root, context) = setup().await;
    let engine = context.engine().clone();
    let question = pending(&engine, NAS).await;
    let client = connect(&context).await;

    let mut called = 0;
    for name in RcMcpServer::tool_names() {
        if !(name.starts_with("connections_") || name.starts_with("certificates_")) {
            continue;
        }
        for answer in ["accept_once", "trust_and_save", "trust", "accept"] {
            let arguments: Map<String, Value> = json!({
                "question": question.id,
                "id": question.id,
                "answer": answer,
                "fingerprint": question.fingerprint,
                "host": "nas.example",
                "port": 990,
                "trust": true,
            })
            .as_object()
            .cloned()
            .unwrap_or_default();
            // Refused, failed or answered: none of it may trust the certificate.
            let _ = client
                .call_tool(CallToolRequestParams::new(name.clone()).with_arguments(arguments))
                .await;
        }
        called += 1;
    }
    assert!(called > 5, "the connections group has its tools: {called}");

    assert_eq!(engine.certificate_questions(), vec![question]);
    assert_eq!(list(&client).await["certificates"], json!([]));
}

#[tokio::test]
async fn the_list_shows_what_a_person_trusted() {
    let (_root, context) = setup().await;
    context
        .db
        .add_trusted_certificate(&NewTrustedCertificate {
            host: "nas.example".to_owned(),
            port: 990,
            fingerprint_sha256: fingerprint(OTHER),
            subject: "CN=other.example".to_owned(),
            issuer: "CN=other.example".to_owned(),
            not_after: 4_000_000_000,
        })
        .await
        .expect("add")
        .expect("stored");
    let client = connect(&context).await;

    let listed = list(&client).await;

    let certificates = listed["certificates"].as_array().expect("certificates");
    assert_eq!(certificates.len(), 1, "{listed}");
    assert_eq!(certificates[0]["host"], "nas.example");
    assert_eq!(certificates[0]["port"], 990);
    assert_eq!(
        certificates[0]["fingerprint_sha256"],
        fingerprint(OTHER).as_str()
    );
}
