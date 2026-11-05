//! A plugin none of whose versions verifies, as the list shows it (RC-100-79).

use rc_db::Db;
use rc_plugin_host::{
    PluginInstaller, PluginVerifier, installed, public_key_base64,
    test_support::{signing_key, viewer_package},
};

use crate::{Command, Outcome, PluginsOutcome, test_support};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";
const KEY_ID: &str = "older-author-v1";

#[tokio::test]
async fn a_plugin_under_a_key_not_trusted_shows_its_name_version_kind_and_why() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(root.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let data = root.path().join("data");
    assert!(engine.attach_plugins(data.clone(), None));
    // Installed under a key an older build trusted; this one trusts no such key.
    let older = PluginVerifier::new(false);
    older
        .trust_key_base64(KEY_ID.to_owned(), &public_key_base64(&signing_key(5)))
        .expect("trust");
    let package = viewer_package(ID, "1.0.0", KEY_ID, &signing_key(5), true);
    PluginInstaller::new(installed::plugins_root(&data), older)
        .install_bytes(&package)
        .expect("install");

    let outcome = engine
        .dispatch(Command::ListPlugins { id: None })
        .await
        .expect("list");
    let Outcome::Plugins(outcome) = outcome else {
        panic!("a plugins outcome, got {outcome:?}");
    };
    let PluginsOutcome::List { plugins, .. } = *outcome else {
        panic!("a list");
    };
    let plugin = &plugins[0];
    assert_eq!(
        (
            plugin.name.as_str(),
            plugin.kind.as_str(),
            plugin.version.as_deref()
        ),
        ("CSV table", "viewer", None),
        "named by its manifest, and nothing runs"
    );
    assert_eq!(plugin.refused.as_deref(), Some("plugin_key_untrusted"));
    assert!(
        plugin.signed,
        "signed, by a key not trusted here: never \"unsigned\""
    );
    assert_eq!(plugin.versions[0].version, "1.0.0");
    assert_eq!(
        plugin.versions[0].refused.as_deref(),
        Some("plugin_key_untrusted")
    );
}
