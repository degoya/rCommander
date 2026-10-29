//! RC-100-12: the shared fields a bundled manifest leaves to `plugins/metadata.toml`, and the
//! third-party manifest that needs no such file. After rDownloader's
//! `crates/rd-plugin-host/src/manifest/complete_tests.rs` at commit 26c8592a6.

use std::path::PathBuf;

use super::{COMPLETED_MARKER, ManifestDefaults, complete_manifest};
use crate::{PluginManifest, parse_manifest};

const KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// A key other than the defaults' one: a real Ed25519 point.
fn other_key() -> String {
    crate::public_key_base64(&ed25519_dalek::SigningKey::from_bytes(&[7; 32]))
}

const DEFAULTS: &str = r#"# The shared fields.
manifest_version = 1
api_version = "0.1.0"
key_id = "rcommander-dev-v1"
public_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="

[metadata]
author = "DEGOYA.digital"
license = "MIT"
min_app_version = "0.1.0"
"#;

/// A bundled viewer's source: nothing the defaults supply.
const SOURCE: &str = r#"plugin_type = "viewer"
id = "019d0000-0000-7000-8000-0000000000a1"
name = "Fixture"
version = "1.0.0"

[match]
extensions = ["json"]

[viewer]
outputs = ["text"]

[capabilities]

[metadata]
description = "A fixture viewer."
"#;

fn defaults() -> ManifestDefaults {
    ManifestDefaults::parse(DEFAULTS).expect("the defaults parse")
}

fn manifest(text: &str) -> PluginManifest {
    parse_manifest(text.as_bytes()).unwrap_or_else(|error| panic!("{error:#}\n{text}"))
}

#[test]
fn every_missing_field_is_written_where_its_table_is() {
    let completed = complete_manifest(SOURCE, Some(&defaults()), None).expect("complete");
    let expected = format!(
        "{COMPLETED_MARKER}\nmanifest_version = 1\napi_version = \"0.1.0\"\n\
         key_id = \"rcommander-dev-v1\"\npublic_key = \"{KEY}\"\n"
    );
    assert!(completed.starts_with(&expected), "{completed}");
    assert!(
        completed.contains(
            "[metadata]\nauthor = \"DEGOYA.digital\"\nlicense = \"MIT\"\n\
             min_app_version = \"0.1.0\"\ndescription = \"A fixture viewer.\"\n"
        ),
        "{completed}"
    );
    let parsed = manifest(&completed);
    assert_eq!(parsed.key_id, "rcommander-dev-v1");
    assert_eq!(parsed.metadata.author, "DEGOYA.digital");
}

/// Packaging a source without its shared fields is refused, so no package lacks one.
#[test]
fn an_uncompleted_source_is_refused_as_a_manifest() {
    assert!(parse_manifest(SOURCE.as_bytes()).is_err());
}

#[test]
fn a_field_the_manifest_states_is_kept() {
    let own = SOURCE.replace(
        "description = \"A fixture viewer.\"\n",
        "description = \"A fixture viewer.\"\nmin_app_version = \"0.0.9\"\n",
    );
    let completed = complete_manifest(&own, Some(&defaults()), None).expect("complete");
    let parsed = manifest(&completed);
    assert_eq!(parsed.metadata.min_app_version.as_deref(), Some("0.0.9"));
    assert_eq!(completed.matches("min_app_version").count(), 1);
}

#[test]
fn a_complete_manifest_keeps_every_byte() {
    let complete = complete_manifest(SOURCE, Some(&defaults()), None).expect("complete");
    let again = complete_manifest(&complete, Some(&defaults()), Some(KEY)).expect("again");
    assert_eq!(again, complete);
    assert_eq!(
        complete_manifest(SOURCE, None, None).expect("no defaults"),
        SOURCE
    );
}

#[test]
fn the_signing_key_supplies_the_public_half_and_never_the_name() {
    let same = complete_manifest(SOURCE, Some(&defaults()), Some(KEY)).expect("same key");
    assert_eq!(
        same,
        complete_manifest(SOURCE, Some(&defaults()), None).expect("defaults alone")
    );
    let other_key = other_key();
    let other = complete_manifest(SOURCE, Some(&defaults()), Some(&other_key)).expect("other");
    assert!(other.contains(&format!("public_key = \"{other_key}\"\n")));
    assert!(!other.contains(KEY));
    let keyed_only = complete_manifest(SOURCE, None, Some(&other_key)).expect("key alone");
    assert!(keyed_only.starts_with(&format!(
        "{COMPLETED_MARKER}\npublic_key = \"{other_key}\"\n"
    )));
    assert!(!keyed_only.contains("key_id"));
}

#[test]
fn a_manifest_without_metadata_gets_the_table() {
    let bare = SOURCE.replace("\n[metadata]\ndescription = \"A fixture viewer.\"\n", "");
    let completed = complete_manifest(&bare, Some(&defaults()), None).expect("complete");
    assert!(
        completed.ends_with(
            "\n[metadata]\nauthor = \"DEGOYA.digital\"\nlicense = \"MIT\"\nmin_app_version = \"0.1.0\"\n"
        ),
        "{completed}"
    );
    let table: toml::Table = toml::from_str(&completed).expect("TOML");
    assert_eq!(table["metadata"]["author"].as_str(), Some("DEGOYA.digital"));
}

#[test]
fn the_defaults_hold_shared_fields_only() {
    for (text, refusal) in [
        (
            "version = \"1.0.0\"\n",
            "`version` is not a field plugins share",
        ),
        (
            "[metadata]\ndescription = \"x\"\n",
            "`description` is not a field plugins share in [metadata]",
        ),
        (
            "[limits]\nfuel = 1\n",
            "top-level fields and [metadata] only",
        ),
        ("key_id = \n", "not TOML"),
    ] {
        let error = ManifestDefaults::parse(text).expect_err(text);
        assert!(format!("{error:#}").contains(refusal), "{text}: {error:#}");
    }
}

fn workspace_root() -> PathBuf {
    let crate_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR at run time");
    PathBuf::from(crate_dir).join("../..")
}

/// Every bundled manifest becomes a valid package manifest with `plugins/metadata.toml`; the
/// key the build signs with supplies `public_key`.
#[test]
fn every_bundled_manifest_completes_to_a_valid_one() {
    let plugins = workspace_root().join("plugins");
    let defaults = ManifestDefaults::read(&plugins.join("metadata.toml")).expect("defaults");
    let mut checked = 0;
    for entry in std::fs::read_dir(&plugins).expect("plugins") {
        let path = entry.expect("entry").path().join("manifest.toml");
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let completed = complete_manifest(&source, Some(&defaults), Some(KEY))
            .unwrap_or_else(|error| panic!("{}: {error:#}", path.display()));
        manifest(&completed);
        checked += 1;
    }
    assert!(checked >= 1, "no manifest under {}", plugins.display());
}

/// A third party has no `plugins/metadata.toml`: a scaffolded manifest is complete and valid on
/// its own, and packaging it changes none of its bytes.
#[test]
fn a_third_party_manifest_is_complete_without_the_shared_file() {
    let templates = workspace_root().join("sdk/templates");
    let other_key = other_key();
    let mut checked = 0;
    for entry in std::fs::read_dir(&templates).expect("templates") {
        let path = entry.expect("entry").path().join("manifest.toml");
        let Ok(template) = std::fs::read_to_string(&path) else {
            continue;
        };
        let scaffolded = template
            .replace("{{PLUGIN_NAME}}", "Demo")
            .replace("{{PLUGIN_SLUG}}", "demo")
            .replace("{{PLUGIN_ID}}", "019d0000-0000-7000-8000-00000000abcd")
            .replace("REPLACE_WITH_YOUR_BASE64_ED25519_PUBLIC_KEY", &other_key);
        let packaged = complete_manifest(&scaffolded, None, Some(&other_key))
            .unwrap_or_else(|error| panic!("{}: {error:#}", path.display()));
        assert_eq!(packaged, scaffolded, "{}", path.display());
        manifest(&packaged);
        checked += 1;
    }
    assert!(checked >= 1, "no template under {}", templates.display());
}
