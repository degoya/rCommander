//! What a plugin author runs against their own package before publishing it
//! (`rcommander plugin conformance`, RC-100-12).
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/conformance.rs` at commit
//! 26c8592a6. The point is the same: "it worked on my machine" and "this rCommander will run it"
//! become one question. Everything checked here is something the host would otherwise tell the
//! author only through a person's bug report: a manifest field out of bounds, an import the
//! manifest never granted, a world that does not match, and one behavioural question per kind
//! (`questions.rs`) - for a viewer, what it does with an empty file; for a secret source, what it
//! does when its service does not answer (RC-100-33).
//!
//! Adapted: rCommander's kinds (the viewer in stage 1; `docs/plugins.md`, "SDK" names the
//! question of each later kind), no network to refuse (no viewer is granted one), and English
//! as the only required language, which verification already holds - rDownloader's
//! `locales_required` has nothing to ask yet.
//!
//! Deliberately not here: whether the plugin shows a real file correctly. A package that passes
//! every check can still be wrong about its format; the report says what was verified, not that
//! the plugin is good.

use serde::Serialize;

mod questions;

use crate::{
    PluginType, PluginVerifier, SecretsOptions, SecretsPlugin, VerifiedPackage, ViewerOptions,
    ViewerPlugin, net_http,
};

/// What the `package` check covers, named because a run stops there when it fails.
const PACKAGE_ABOUT: &str = "The package verifies as it would on install: archive, manifest, API \
     version, locales, signature, limits, and every import the manifest grants";
const WORLD_ABOUT: &str = "The component exports the world its plugin type requires";

/// One verified property.
#[derive(Debug, Serialize)]
pub struct ConformanceCheck {
    /// Stable identifier, so a CI job can act on a specific failure.
    pub id: &'static str,
    /// What this check is about, in one line.
    pub about: &'static str,
    pub passed: bool,
    /// Why it failed, or what it found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The result of one conformance run.
#[derive(Debug, Serialize)]
pub struct ConformanceReport {
    pub plugin_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub plugin_type: Option<String>,
    pub passed: bool,
    pub checks: Vec<ConformanceCheck>,
}

impl ConformanceReport {
    fn push(&mut self, id: &'static str, about: &'static str, result: Result<(), String>) {
        let (passed, detail) = match result {
            Ok(()) => (true, None),
            Err(detail) => (false, Some(detail)),
        };
        self.passed &= passed;
        self.checks.push(ConformanceCheck {
            id,
            about,
            passed,
            detail,
        });
    }
}

/// Checks one `.rcplug` and reports every property separately.
///
/// A failing check never stops the run where the next one can still be asked: an author wants
/// the whole list, not the first thing that went wrong.
pub async fn check_package(bytes: &[u8], verifier: &PluginVerifier) -> ConformanceReport {
    let mut report = ConformanceReport {
        plugin_id: None,
        name: None,
        version: None,
        plugin_type: None,
        passed: true,
        checks: Vec::new(),
    };

    // Verification covers the archive, the manifest, the API version, the locales, the
    // signature, the limits and the component's imports in one step, exactly as installation
    // does - so it is one check, not six that would always pass after it.
    let package = match verifier.verify_bytes(bytes) {
        Ok(package) => {
            report.push("package", PACKAGE_ABOUT, Ok(()));
            package
        }
        Err(error) => {
            report.push("package", PACKAGE_ABOUT, Err(format!("{error:#}")));
            return report;
        }
    };
    let manifest = &package.manifest;
    report.plugin_id = Some(manifest.id.clone());
    report.name = Some(manifest.name.clone());
    report.version = Some(manifest.version.clone());
    report.plugin_type = Some(manifest.plugin_type.as_str().to_owned());

    // Instantiating against the world the type declares is the only way to find out that the
    // exports line up: a component missing one compiles and fails at the first call - on a
    // person's file.
    match instantiate(&package) {
        Ok(built) => {
            report.push("world", WORLD_ABOUT, Ok(()));
            let answer = questions::ask(&built, manifest).await;
            report.push(answer.id, answer.about, answer.result);
        }
        Err(detail) => report.push("world", WORLD_ABOUT, Err(detail)),
    }
    report
}

/// A built plugin of a kind with a question.
pub(crate) enum Built {
    Viewer(Box<ViewerPlugin>),
    Secrets(Box<SecretsPlugin>),
}

/// Builds the package as the host would.
fn instantiate(package: &VerifiedPackage) -> Result<Built, String> {
    match package.manifest.plugin_type {
        PluginType::Viewer => ViewerPlugin::new(
            &package.manifest,
            &package.component,
            ViewerOptions::default(),
        )
        .map(|viewer| Built::Viewer(Box::new(viewer)))
        .map_err(|failure| format!("the component is refused: {failure}")),
        // Asked without a service: the port it is granted is refused, so the run reaches nothing.
        PluginType::Secrets => {
            let refused_ports = net_http(&package.manifest)
                .ok()
                .flatten()
                .map(|grant| vec![grant.loopback])
                .unwrap_or_default();
            SecretsPlugin::new(
                &package.manifest,
                &package.component,
                SecretsOptions {
                    refused_ports,
                    ..SecretsOptions::default()
                },
            )
            .map(|source| Built::Secrets(Box::new(source)))
            .map_err(|failure| format!("the component is refused: {failure}"))
        }
        PluginType::Unknown(ref other) => Err(format!("unknown plugin type `{other}`")),
    }
}

#[cfg(test)]
#[path = "conformance_tests.rs"]
mod tests;
