//! One behavioural question per plugin kind (`docs/plugins.md`, "SDK"): stage 1 asks the viewer
//! `viewer_empty_source`, contract 0.3.0 the secret source `secrets_no_service` (RC-100-33).
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/conformance/questions.rs` at
//! commit 26c8592a6 (RD-1280-19): instantiating a component proves its exports line up with its
//! world; it says nothing about what the plugin answers. Each question asks the built component
//! one thing a plugin of its kind must get right without a network and that no real input can
//! stand in for later. Every question splits in two: the calls into the component ([`ask`]) and
//! a judge over their outcome ([`judge_empty_source`]), which the tests feed the answer a broken
//! component would give. A trap is never a refusal: a plugin that crashes on an input is wrong
//! about it, whatever it meant to say.
//!
//! The `id` of each question is its stable code, the one a CI job acts on.

use std::sync::{Arc, atomic::AtomicBool};

use rc_plugin_api::viewer::{Output, Purpose, Theme};
use rc_plugin_types::Failure;

use super::Built;
use crate::{BytesSource, MemorySink, PluginManifest, SecretsPlugin, ViewerPlugin, failure::codes};

/// The codes of a call that did not answer, or answered against the rules: the plugin trapped,
/// ran out of time, fuel or memory, or broke a rule of its output.
const BROKEN_CODES: [&str; 5] = [
    codes::PLUGIN_CRASHED,
    codes::PLUGIN_TIMED_OUT,
    codes::PLUGIN_OUT_OF_FUEL,
    codes::PLUGIN_OUT_OF_MEMORY,
    codes::PLUGIN_BAD_OUTPUT,
];

/// One answered question.
pub(super) struct Answer {
    pub(super) id: &'static str,
    pub(super) about: &'static str,
    pub(super) result: Result<(), String>,
}

/// Asks the built plugin its kind's question.
pub(super) async fn ask(built: &Built, manifest: &PluginManifest) -> Answer {
    match built {
        Built::Viewer(viewer) => Answer {
            id: "viewer_empty_source",
            about: "The viewer answers an empty file of a type it claims without crashing: \
                    probe, open and the first output it names end in an answer or a refusal",
            result: empty_source(viewer, manifest).await,
        },
        Built::Secrets(source) => Answer {
            id: "secrets_no_service",
            about: "The secret source answers while its service does not: status and a search \
                    end in an answer or a refusal, not a crash",
            result: no_service(source).await,
        },
    }
}

/// `status` and `search` with every request refused, as when the manager is not running.
async fn no_service(source: &SecretsPlugin) -> Result<(), String> {
    judge_empty_source("status", source.status().await.map(|_| ()))?;
    judge_empty_source("search", source.search("conformance", 5).await.map(|_| ()))
}

/// `probe`, `open` and the first output of an empty file named like one the manifest claims.
async fn empty_source(viewer: &ViewerPlugin, manifest: &PluginManifest) -> Result<(), String> {
    let name = manifest.matching.extensions.first().map_or_else(
        || "conformance".to_owned(),
        |ext| format!("conformance.{ext}"),
    );
    let source = || Arc::new(BytesSource::new(&name, Vec::new()));
    judge_empty_source("probe", viewer.probe(source()).await.map(|_| ()))?;
    let theme = Theme {
        dark: false,
        background: "#ffffff".to_owned(),
        foreground: "#1b1b1b".to_owned(),
        accent: "#0078d4".to_owned(),
    };
    let opened = viewer
        .open(
            source(),
            Purpose::Lister,
            &theme,
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    let mut session = match opened {
        Ok(session) => session,
        // A refusal of an empty file is an answer; only a broken call fails the question.
        Err(failure) => return judge_empty_source("open", Err(failure)),
    };
    let Some(first) = session.document().outputs.first().copied() else {
        return Err("open named no output".to_owned());
    };
    let pages = session.document().pages.unwrap_or(0);
    let outcome = match first {
        Output::Text | Output::Highlighted | Output::Markdown => {
            session.text(Box::new(MemorySink::new())).await.map(|_| ())
        }
        Output::Image => session.image(Box::new(MemorySink::new())).await.map(|_| ()),
        Output::Tree => session.children(None).await.map(|_| ()),
        Output::Pages if pages == 0 => Ok(()),
        Output::Pages => session
            .page(0, 64, 64, Box::new(MemorySink::new()))
            .await
            .map(|_| ()),
    };
    judge_empty_source(first.as_str(), outcome)
}

/// An answer or a refusal passes; a trap, an exhausted budget or an answer against the rules
/// does not.
pub(super) fn judge_empty_source(call: &str, outcome: Result<(), Failure>) -> Result<(), String> {
    match outcome {
        Ok(()) => Ok(()),
        Err(failure) => match failure.code.as_deref() {
            Some(code) if BROKEN_CODES.contains(&code) => {
                let reason = failure
                    .params
                    .get("reason")
                    .or_else(|| failure.params.get("what"))
                    .map(|value| format!(" ({value})"))
                    .unwrap_or_default();
                Err(format!("`{call}` of an empty file ended in {code}{reason}"))
            }
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
#[path = "questions_tests.rs"]
mod tests;
