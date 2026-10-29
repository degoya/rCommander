//! The MCP boundary of the secret sources (RC-100-34): listing and searching reach MCP, mapping
//! does not, and no answer carries a value.

use rc_db::connections::ConnectionDraft;
use rc_engine::{Command, SecretKind, SecretRef, SecretSourceRef};
use rc_plugin_host::{
    key_fingerprint,
    test_support::{current_secrets_component, secrets_package, signing_key},
};
use serde_json::json;

use crate::{
    RcMcpServer,
    coverage::{COVERAGE, Coverage},
    test_support::{Built, Fixture, call, connect},
};

const ID: &str = "019d0000-0000-7000-8000-0000000000b1";
const SOURCE: &str = "plugin:019d0000-0000-7000-8000-0000000000b1";
/// What the test source's `get` answers (`testbed::secrets::CANARY`).
const CANARY: &str = "canary-pw-7731";

#[tokio::test]
async fn listing_and_searching_reach_mcp_and_mapping_does_not() {
    let names = RcMcpServer::tool_names();
    let tools: Vec<&String> = names
        .iter()
        .filter(|name| name.contains("sources"))
        .collect();
    assert_eq!(
        tools,
        vec!["credential_sources_list", "credential_sources_search"],
        "{names:?}"
    );
    for (id, coverage) in COVERAGE
        .iter()
        .filter(|(id, _)| id.starts_with("secrets.source"))
    {
        assert_eq!(
            matches!(coverage, Coverage::Covered(_)),
            ["secrets.sources", "secrets.source_search"].contains(id),
            "{id}"
        );
    }

    let Built {
        root,
        engine,
        db,
        context,
    } = Fixture::new().build().await;
    assert!(engine.attach_plugins(root.path().join("data"), None));
    let package = root.path().join("source.rcplug");
    let key = signing_key(9);
    std::fs::write(
        &package,
        secrets_package(ID, "tests-v1", &key, &current_secrets_component()),
    )
    .expect("write");
    engine
        .dispatch(Command::InstallPlugin {
            path: package.display().to_string(),
            fingerprint: Some(key_fingerprint(&key.verifying_key())),
            digest: rc_plugin_host::sha256_hex(&std::fs::read(&package).expect("read")),
        })
        .await
        .expect("the window installs");
    let connection = db
        .create_connection(None, &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("create")
        .id;
    let reference = SecretSourceRef {
        connection_id: connection,
        kind: SecretKind::Password,
        source: SOURCE.to_owned(),
        entry_id: "7c17".to_owned(),
        entry_label: "web (root)".to_owned(),
    };
    let refused = engine
        .dispatch_mcp(Command::MapSecretSource(reference.clone()))
        .await
        .expect_err("GUI only");
    assert_eq!(refused.code(), "gui_only");
    engine
        .dispatch(Command::MapSecretSource(reference))
        .await
        .expect("the window maps");
    // A connect read the value: from now on it is scrubbed from every answer.
    engine
        .resolve_secret(SecretRef {
            connection,
            kind: SecretKind::Password,
        })
        .await
        .expect("resolved")
        .expect("a value");

    let client = connect(&context).await;
    let listed = call(
        &client,
        "credential_sources_list",
        json!({ "connection_id": connection.0 }),
    )
    .await;
    assert_eq!(listed["sources"][0]["source"], json!(SOURCE));
    assert_eq!(listed["sources"][0]["state"], json!("ready"));
    assert_eq!(listed["mappings"][0]["entry_id"], json!("7c17"));
    let searched = call(
        &client,
        "credential_sources_search",
        json!({ "source": SOURCE, "query": CANARY }),
    )
    .await;
    assert_eq!(searched["entries"], json!([]), "{searched}");
    for answer in [&listed, &searched] {
        assert!(!answer.to_string().contains(CANARY), "{answer}");
    }
}
