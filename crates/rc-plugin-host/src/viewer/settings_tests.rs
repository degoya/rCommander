//! Plugin settings in the contract (RC-100-14): a 0.2.0 viewer reads its declared settings, one
//! importing `settings` without declaring any is refused, a 0.1.0 package still installs and runs,
//! and a viewer of the current contract (0.3.0, RC-100-33) runs beside them.

use rc_plugin_api::API_VERSION;

use super::tests::{code, open, plugin, plugin_with};
use super::{ViewerOptions, ViewerPlugin};
use crate::{
    MemorySink, PluginInstaller,
    failure::codes,
    installed::{self, PluginState},
    package_plugin,
    test_fixtures::{PLUGIN_ID, SETTINGS, viewer_manifest, viewer_manifest_0_2},
    test_support::{signing_key, viewer_package},
    testbed::{self, Export, Testbed},
};

const KEY_ID: &str = "example-release-v1";

/// The contract `settings` arrived with; its components are built against the frozen copy.
const V0_2: &str = "0.2.0";

/// What `text` writes when `settings.setting` answers `none`.
const NONE: &str = "<none>";

/// A 0.2.0 viewer whose `text` writes what `settings.setting(name)` answers.
fn reading(name: &str) -> Testbed {
    let mut bed = Testbed::new().contract(V0_2);
    let name = bed.data(name.as_bytes());
    let none = bed.data(NONE.as_bytes());
    let encoding = bed.data(b"utf-8");
    bed.body(Export::Text, testbed::echo_setting(name, none, encoding))
}

/// What the viewer reads for `name` when the person stored `stored`.
async fn read(name: &str, stored: &[(&str, &str)]) -> String {
    let options = ViewerOptions {
        settings: stored
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
        ..ViewerOptions::default()
    };
    let manifest = viewer_manifest_0_2(&["text"], SETTINGS);
    let plugin = plugin_with(&manifest, &reading(name), options);
    let mut session = open(&plugin).await.expect("open");
    let sink = MemorySink::new();
    session.text(Box::new(sink.clone())).await.expect("text");
    String::from_utf8(sink.contents()).expect("UTF-8")
}

/// The person's value, else the default; `none` for a declared setting with neither and for a
/// name the manifest does not declare, even one stored under the plugin's prefix.
#[tokio::test]
async fn a_viewer_reads_its_declared_settings() {
    assert_eq!(read("format", &[("format", "pretty")]).await, "pretty");
    assert_eq!(read("format", &[]).await, "as_is");
    // A value the manifest does not accept is not handed over; the default is.
    assert_eq!(read("format", &[("format", "compact")]).await, "as_is");
    assert_eq!(read("label", &[("label", "Totals")]).await, "Totals");
    assert_eq!(read("label", &[]).await, NONE);
    assert_eq!(read("separator", &[("separator", ";")]).await, NONE);
}

/// A component importing `settings` while its manifest declares no `[[settings]]` is refused -
/// when loaded and when packaged - like an ungranted `http`.
#[test]
fn importing_settings_without_declaring_any_is_refused() {
    let component = reading("format").build();
    let undeclared = viewer_manifest_0_2(&["text"], "");
    let failure = ViewerPlugin::new(&undeclared, &component, ViewerOptions::default())
        .err()
        .expect("refused");
    assert_eq!(failure.code(), Some(codes::PLUGIN_DENIED));
    assert_eq!(
        failure.params.get("import").map(String::as_str),
        Some("rcommander:plugin/settings@0.2.0")
    );

    let manifest =
        crate::test_support::viewer_manifest(PLUGIN_ID, "1.0.0", KEY_ID, &signing_key(7))
            .replace("api_version = \"0.1.0\"", "api_version = \"0.2.0\"");
    let declared_only = manifest
        .split("\n[[settings]]")
        .next()
        .expect("the manifest before its settings");
    assert!(package_plugin(declared_only.as_bytes(), &component, &[], None).is_err());
    assert!(package_plugin(manifest.as_bytes(), &component, &[], None).is_ok());
}

/// A viewer that declares settings but never calls `setting` imports nothing more, and runs.
#[tokio::test]
async fn a_declared_but_unread_setting_needs_no_import() {
    let manifest = viewer_manifest_0_2(&["text"], SETTINGS);
    let plugin = plugin(&manifest, &Testbed::new().contract(V0_2));
    let mut session = open(&plugin).await.expect("open");
    let sink = MemorySink::new();
    session.text(Box::new(sink.clone())).await.expect("text");
    assert_eq!(sink.contents(), b"hello");
}

/// The contract version is the manifest's: a 0.2.0 component under a 0.1.0 manifest, or the
/// other way round, is refused instead of half linked.
#[test]
fn a_component_of_another_contract_than_its_manifest_is_refused() {
    let current = Testbed::new().contract(V0_2).build();
    let refused = ViewerPlugin::new(
        &viewer_manifest(&["text"]),
        &current,
        ViewerOptions::default(),
    );
    assert_eq!(code(refused), codes::PLUGIN_DENIED);
    let refused = ViewerPlugin::new(
        &viewer_manifest_0_2(&["text"], ""),
        &Testbed::new().build(),
        ViewerOptions::default(),
    );
    assert_eq!(code(refused), codes::PLUGIN_DENIED);
}

/// A package built against 0.1.0 - before `settings` - installs, lists and its viewer runs.
#[tokio::test]
async fn a_0_1_0_package_installs_and_runs() {
    let data = tempfile::tempdir().expect("temp dir");
    let mut state = PluginState::default();
    state
        .trusted_keys
        .insert(KEY_ID.to_owned(), crate::public_key_base64(&signing_key(7)));
    state.write(data.path()).expect("state");
    let installer = PluginInstaller::new(
        installed::plugins_root(data.path()),
        state.verifier().expect("verifier"),
    );
    let package = viewer_package(PLUGIN_ID, "1.0.0", KEY_ID, &signing_key(7), true);
    installer.install_bytes(&package).expect("install");

    let listed = installed::list(data.path()).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].manifest.api_version, "0.1.0");
    let plugin = ViewerPlugin::new(
        &listed[0].manifest,
        &listed[0].component,
        ViewerOptions::default(),
    )
    .expect("a 0.1.0 viewer loads");
    let mut session = open(&plugin).await.expect("open");
    let sink = MemorySink::new();
    session.text(Box::new(sink.clone())).await.expect("text");
    assert_eq!(sink.contents(), b"hello");
}

/// A viewer built against the current contract, under a manifest naming it, opens and writes.
#[tokio::test]
async fn a_viewer_of_the_current_contract_runs() {
    let text = crate::test_fixtures::viewer_manifest_text(&["text"], SETTINGS).replace(
        "api_version = \"0.1.0\"",
        &format!("api_version = \"{API_VERSION}\""),
    );
    let manifest = crate::manifest::parse_manifest(text.as_bytes()).expect("a valid manifest");
    let plugin = plugin(&manifest, &Testbed::new().contract(API_VERSION));
    let mut session = open(&plugin).await.expect("open");
    let sink = MemorySink::new();
    session.text(Box::new(sink.clone())).await.expect("text");
    assert_eq!(sink.contents(), b"hello");
}
