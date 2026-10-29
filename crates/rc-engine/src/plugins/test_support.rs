//! What the viewer adapter's tests share: manifests, the host's test-bed components built into
//! plugins, and a request to open with.

use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
};

use rc_plugin_host::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ExecutionLog, PluginManifest, ViewerOptions, ViewerPlugin,
    parse_manifest, testbed::Testbed,
};

use super::{InstalledViewer, OpenRequest, Purpose, Theme, ViewerPlugins};

/// 32 zero bytes in base64: a well-formed key, which is all the manifest checks.
const PUBLIC_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// What differs between the test manifests.
pub(crate) struct Spec<'a> {
    /// The last hex digits of the id, so two plugins differ.
    pub(crate) id: &'a str,
    pub(crate) name: &'a str,
    pub(crate) extensions: &'a [&'a str],
    /// Hex of a magic at offset 0, if any.
    pub(crate) magic: Option<&'a str>,
    pub(crate) max_size: Option<u64>,
    pub(crate) outputs: &'a [&'a str],
    /// `[limits]` lines, if any.
    pub(crate) limits: &'a str,
    /// The most bytes one output may write (`plugins.viewer_output_limit_mib`).
    pub(crate) output_limit: u64,
}

impl Default for Spec<'_> {
    fn default() -> Self {
        Self {
            id: "a1",
            name: "Test bed",
            extensions: &["csv"],
            magic: None,
            max_size: None,
            outputs: &["text"],
            limits: "",
            output_limit: DEFAULT_OUTPUT_LIMIT_BYTES,
        }
    }
}

/// A valid viewer manifest of `spec`.
pub(crate) fn manifest(spec: &Spec<'_>) -> PluginManifest {
    let list = |items: &[&str]| {
        items
            .iter()
            .map(|item| format!("\"{item}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let magic = spec
        .magic
        .map(|hex| format!("{{ offset = 0, hex = \"{hex}\" }}"))
        .unwrap_or_default();
    let max_size = spec
        .max_size
        .map(|max| format!("max_size = {max}"))
        .unwrap_or_default();
    let limits = if spec.limits.is_empty() {
        String::new()
    } else {
        format!("\n[limits]\n{}\n", spec.limits)
    };
    let text = format!(
        r#"
manifest_version = 1
plugin_type = "viewer"
api_version = "0.1.0"
id = "019d0000-0000-7000-8000-0000000000{id}"
name = "{name}"
version = "1.0.0"
key_id = "example-release-v1"
public_key = "{PUBLIC_KEY}"

[match]
extensions = [{extensions}]
magic = [{magic}]
{max_size}

[viewer]
outputs = [{outputs}]

[capabilities]

[metadata]
description = "A component the viewer adapter's tests run."
author = "rCommander"
{limits}
"#,
        id = spec.id,
        name = spec.name,
        extensions = list(spec.extensions),
        outputs = list(spec.outputs),
    );
    parse_manifest(text.as_bytes()).expect("a valid manifest")
}

/// The plugin of `spec` running `testbed`, its calls recorded in `log`.
pub(crate) fn plugin(
    spec: &Spec<'_>,
    testbed: &Testbed,
    log: &Arc<ExecutionLog>,
) -> InstalledViewer {
    let manifest = manifest(spec);
    let options = ViewerOptions {
        log: Arc::clone(log),
        output_limit: spec.output_limit,
        ..ViewerOptions::default()
    };
    let plugin = ViewerPlugin::new(&manifest, &testbed.build(), options).expect("a viewer");
    InstalledViewer::new(manifest, plugin)
}

/// A set of the plugin of `spec` running `testbed`.
pub(crate) fn one(spec: &Spec<'_>, testbed: &Testbed) -> ViewerPlugins {
    let log = Arc::new(ExecutionLog::new());
    let built = plugin(spec, testbed, &log);
    ViewerPlugins::from_plugins(vec![built], BTreeMap::new(), log)
}

/// A request from Quick View with a fresh flag.
pub(crate) fn request() -> OpenRequest {
    OpenRequest {
        purpose: Purpose::QuickView,
        theme: Theme {
            dark: true,
            background: "#202020".to_owned(),
            foreground: "#ffffff".to_owned(),
            accent: "#60cdff".to_owned(),
        },
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

/// The judge `rc_mcp::sensitive_path` is in the window, reduced to what the tests need: SSH keys
/// by name.
pub(crate) fn judge(path: &std::path::Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?;
    name.starts_with("id_").then_some(crate::SSH_PRIVATE_FILE)
}
