//! The grants of an inventory (RC-100-39): `net_http = { entered = true }` and its `secrets`
//! slots, checked when the manifest is read and named in the install preview.

use super::{describe_capability, secret_slots};
use crate::{manifest::parse_manifest, test_fixtures::inventory_manifest_text};

fn read(text: &str) -> anyhow::Result<crate::PluginManifest> {
    parse_manifest(text.as_bytes())
}

fn with_grants(grants: &str) -> String {
    inventory_manifest_text(false, "")
        .replace("[capabilities]\n", &format!("[capabilities]\n{grants}\n"))
}

#[test]
fn an_inventory_with_its_grants_is_read() {
    let manifest = read(&inventory_manifest_text(true, "")).expect("valid");
    assert_eq!(secret_slots(&manifest).expect("slots"), ["api_token"]);
    assert_eq!(
        describe_capability(&manifest, "net_http"),
        "net_http entered address"
    );
    assert_eq!(
        describe_capability(&manifest, "secrets"),
        "secrets api_token"
    );
    read(&inventory_manifest_text(false, "")).expect("an inventory without a network is valid");
}

#[test]
fn wrong_grants_are_refused() {
    for grants in [
        // A slot means nothing without the request that carries it.
        "secrets = [\"api_token\"]",
        "net_http = { loopback = 8087 }",
        "net_http = { entered = false }",
        "net_http = { entered = true, domains = [\"x\"] }",
        "net_http = { entered = true }\nsecrets = [\"API-Token\"]",
        "net_http = { entered = true }\nsecrets = [\"a\", \"a\"]",
        "net_http = { entered = true }\nsecrets = []",
        "net_http = { entered = true }\nsecrets = [\"a\", \"b\", \"c\", \"d\", \"e\"]",
        "tool = { binary = \"op\" }",
    ] {
        assert!(read(&with_grants(grants)).is_err(), "{grants}");
    }
}

#[test]
fn an_inventory_names_its_own_contract_and_no_file() {
    let api = format!("api_version = \"{}\"", rc_plugin_api::API_VERSION);
    let old = inventory_manifest_text(true, "").replace(&api, "api_version = \"0.3.0\"");
    assert!(read(&old).is_err());
    let claiming = inventory_manifest_text(true, "\n[match]\nextensions = [\"csv\"]\n");
    assert!(read(&claiming).is_err());
}

/// A secret source keeps its loopback grant; the inventory's shape is not one for it.
#[test]
fn a_secret_source_cannot_take_an_entered_address() {
    let text = crate::test_fixtures::secrets_manifest_text(None, "").replace(
        "[capabilities]\n",
        "[capabilities]\nnet_http = { entered = true }\n",
    );
    assert!(read(&text).is_err());
}
