//! The MCP boundary of the server inventories (RC-100-39): listing and syncing reach MCP,
//! setting a source up does not, and no answer carries its token.

use rc_engine::{
    Command, Outcome, SecretParam,
    inventory::{InventoryOutcome, InventorySecret, InventorySourceRequest},
};
use rc_plugin_host::{
    key_fingerprint,
    test_support::{inventory_package, signing_key},
    testbed::inventory::{HostData, InventoryBed, SSH},
};
use serde_json::json;

use crate::{
    RcMcpServer,
    coverage::{COVERAGE, Coverage},
    test_support::{Built, Fixture, call, connect},
};

const ID: &str = "019d0000-0000-7000-8000-0000000000c1";
const CANARY: &str = "canary-token-5521";

fn one_host() -> InventoryBed {
    let mut bed = InventoryBed::new();
    let host = HostData {
        external_id: bed.text("17"),
        name: bed.text("web-01"),
        address: bed.text("web-01.example.net"),
        port: 22,
        protocol: SSH,
        user: None,
        folder: bed.text("dc1"),
        tags: (0, 0),
        note: None,
    };
    bed.hosts(&[host])
}

#[tokio::test]
async fn listing_and_syncing_reach_mcp_and_setting_up_does_not() {
    let names = RcMcpServer::tool_names();
    let tools: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with("inventory_"))
        .collect();
    assert_eq!(tools, vec!["inventory_refresh", "inventory_sources"]);
    for (id, coverage) in COVERAGE
        .iter()
        .filter(|(id, _)| id.starts_with("inventory."))
    {
        assert_eq!(
            matches!(coverage, Coverage::Covered(_)),
            ["inventory.sources", "inventory.refresh"].contains(id),
            "{id}"
        );
    }

    let Built {
        root,
        engine,
        context,
        ..
    } = Fixture::new().with_created_vault().build().await;
    assert!(engine.attach_plugins(root.path().join("data"), None));
    let package = root.path().join("inventory.rcplug");
    let key = signing_key(9);
    let bytes = inventory_package(ID, "tests-v1", &key, &one_host().build());
    std::fs::write(&package, &bytes).expect("write");
    engine
        .dispatch(Command::InstallPlugin {
            path: package.display().to_string(),
            fingerprint: Some(key_fingerprint(&key.verifying_key())),
            digest: rc_plugin_host::sha256_hex(&bytes),
        })
        .await
        .expect("the window installs");
    let request = InventorySourceRequest {
        id: None,
        name: "NetBox".into(),
        plugin_id: ID.into(),
        address: "https://netbox.example.net/".into(),
        folder_id: None,
        secrets: vec![InventorySecret {
            slot: "api_token".into(),
            value: SecretParam::from(CANARY),
        }],
    };
    let refused = engine
        .dispatch_mcp(Command::SaveInventorySource(request.clone()))
        .await
        .expect_err("GUI only");
    assert_eq!(refused.code(), "gui_only");
    let saved = engine
        .dispatch(Command::SaveInventorySource(request))
        .await
        .expect("the window saves");
    let Outcome::Inventory(saved) = saved else {
        panic!("an inventory outcome");
    };
    let InventoryOutcome::Saved { source } = *saved else {
        panic!("a saved source");
    };

    let client = connect(&context).await;
    let listed = call(&client, "inventory_sources", json!({})).await;
    assert_eq!(listed["sources"][0]["name"], json!("NetBox"));
    assert_eq!(listed["sources"][0]["slots"], json!(["api_token"]));
    assert_eq!(listed["plugins"][0]["id"], json!(ID));
    let planned = call(
        &client,
        "inventory_refresh",
        json!({ "source": source.id.0, "preview": true }),
    )
    .await;
    assert_eq!(planned["created"], json!(1), "{planned}");
    assert_eq!(planned["preview"], json!(true), "{planned}");
    for answer in [&listed, &planned] {
        assert!(!answer.to_string().contains(CANARY), "{answer}");
    }
}
