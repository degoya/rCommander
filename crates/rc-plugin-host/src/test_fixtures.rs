//! Manifests the unit tests share.

use crate::manifest::{PluginManifest, parse_manifest};

/// 32 zero bytes in base64: a well-formed key, which is all the manifest checks.
pub(crate) const PUBLIC_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// A valid viewer manifest offering `outputs`, followed by `extra` (more TOML at the top level
/// goes before the first table, so `extra` holds tables only).
pub(crate) fn viewer_manifest_text(outputs: &[&str], extra: &str) -> String {
    let outputs = outputs
        .iter()
        .map(|output| format!("\"{output}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"
manifest_version = 1
plugin_type = "viewer"
api_version = "0.1.0"
id = "019d0000-0000-7000-8000-0000000000a1"
name = "Test bed"
version = "1.0.0"
key_id = "example-release-v1"
public_key = "{PUBLIC_KEY}"

[match]
extensions = ["csv", "tsv"]
magic = []
max_size = 268435456

[viewer]
outputs = [{outputs}]

[capabilities]

[metadata]
description = "A component the host's tests run."
author = "rCommander"
homepage = "https://example.net/test-bed"
license = "MIT"
min_app_version = "0.1.0"
{extra}
"#
    )
}

/// The parsed manifest of [`viewer_manifest_text`] without extras.
pub(crate) fn viewer_manifest(outputs: &[&str]) -> PluginManifest {
    parse_manifest(viewer_manifest_text(outputs, "").as_bytes()).expect("a valid manifest")
}

/// Two `[[settings]]` rows: a choice with a default, a text without one.
pub(crate) const SETTINGS: &str = r#"
[[settings]]
key = "format"
type = "choice"
values = ["as_is", "pretty"]
default = "as_is"

[[settings]]
key = "label"
type = "text"
"#;

/// [`viewer_manifest_text`] built against contract 0.2.0 (RC-100-14), followed by `extra`.
pub(crate) fn viewer_manifest_0_2(outputs: &[&str], extra: &str) -> PluginManifest {
    let text = viewer_manifest_text(outputs, extra)
        .replace("api_version = \"0.1.0\"", "api_version = \"0.2.0\"");
    parse_manifest(text.as_bytes()).expect("a valid manifest")
}

/// The same with `[limits]` set to `limits` (TOML lines).
pub(crate) fn viewer_manifest_with_limits(outputs: &[&str], limits: &str) -> PluginManifest {
    let text = viewer_manifest_text(outputs, &format!("\n[limits]\n{limits}\n"));
    parse_manifest(text.as_bytes()).expect("a valid manifest")
}

/// The plugin id the package tests install.
pub(crate) const PLUGIN_ID: &str = "019d0000-0000-7000-8000-0000000000a1";

/// A `.rcplug` made of exactly `members`, as a tampering hand would make one.
pub(crate) fn zip_package(members: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write as _;

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in members {
        writer.start_file(*name, options).expect("member");
        writer.write_all(bytes).expect("write member");
    }
    writer.finish().expect("archive").into_inner()
}

/// The members of `package`, by name.
pub(crate) fn members(package: &[u8]) -> Vec<(String, Vec<u8>)> {
    use std::io::Read as _;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(package)).expect("archive");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names
        .into_iter()
        .map(|name| {
            let mut bytes = Vec::new();
            archive
                .by_name(&name)
                .expect("member")
                .read_to_end(&mut bytes)
                .expect("read member");
            (name, bytes)
        })
        .collect()
}

/// `package` with the member `name` changed by `change`, every other member as signed.
pub(crate) fn tampered(package: &[u8], name: &str, change: impl Fn(&mut Vec<u8>)) -> Vec<u8> {
    let mut members = members(package);
    for (member, bytes) in &mut members {
        if member == name {
            change(bytes);
        }
    }
    let borrowed: Vec<(&str, &[u8])> = members
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    zip_package(&borrowed)
}

/// A package whose `manifest` and `component` are signed by `key` as they are, without the
/// packager's checks: what an author bypassing them would publish.
pub(crate) fn signed_by_hand(
    manifest: &str,
    component: &[u8],
    key: &ed25519_dalek::SigningKey,
) -> Vec<u8> {
    let digest = crate::package_digest(manifest.as_bytes(), component, &[]);
    let signature = crate::sign_detached(key, &digest);
    zip_package(&[
        ("manifest.toml", manifest.as_bytes()),
        ("component.wasm", component),
        ("signature.ed25519", signature.as_bytes()),
    ])
}

/// A valid secret source's manifest granting `loopback`, or no `net_http` for `None`, followed by
/// `extra` (tables only).
pub(crate) fn secrets_manifest_text(loopback: Option<u16>, extra: &str) -> String {
    let grant = loopback.map_or_else(String::new, |port| {
        format!("net_http = {{ loopback = {port} }}")
    });
    format!(
        r#"
manifest_version = 1
plugin_type = "secrets"
api_version = "{api}"
id = "019d0000-0000-7000-8000-0000000000b1"
name = "Test source"
version = "1.0.0"
key_id = "example-release-v1"
public_key = "{PUBLIC_KEY}"

[capabilities]
{grant}

[metadata]
description = "A secret source the host's tests run."
author = "rCommander"

[limits]
timeout_milliseconds = 1000
{extra}"#,
        api = rc_plugin_api::API_VERSION,
    )
}

/// [`secrets_manifest_text`] parsed.
pub(crate) fn secrets_manifest(loopback: Option<u16>) -> PluginManifest {
    parse_manifest(secrets_manifest_text(loopback, "").as_bytes()).expect("a valid manifest")
}
