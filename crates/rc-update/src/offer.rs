//! Which verified release, if any, this installation is offered (RC-100-05).
//!
//! * **No downgrade.** Only a version of higher SemVer precedence than the running one is
//!   offered, also after a switch from beta back to stable.
//! * **No beta on the stable channel.**
//! * **The beta channel sees stable releases too**, so `1.1.0-beta.2` is offered `1.1.0`.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/offer.rs` at commit 26c8592a6:
//! no installer kinds, no agent, no changelog link.

use chrono::{DateTime, Utc};
use semver::{Prerelease, Version};
use serde::{Deserialize, Serialize};

use crate::manifest::{Artifact, Channel, UpdateManifest, kind};

/// What the window shows about an available update.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub version: String,
    pub channel: Channel,
    pub released_at: DateTime<Utc>,
    /// Plain text, one `- ` point per line.
    pub notes: String,
    /// The file for this platform and architecture, when the release has one.
    pub artifact: Option<Artifact>,
    /// Whether installing it changes the database schema; `true` when the manifest does not say.
    #[serde(default = "changes_schema_by_default")]
    pub schema_change: bool,
}

const fn changes_schema_by_default() -> bool {
    true
}

/// The artifact an installation takes: its platform, architecture and kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target {
    pub platform: &'static str,
    pub arch: &'static str,
    pub kind: &'static str,
}

impl Target {
    /// The running build's: the zip on Windows and macOS, the `.tar.gz` on Linux.
    #[must_use]
    pub fn current() -> Self {
        let platform = match std::env::consts::OS {
            "macos" => "macos",
            "windows" => "windows",
            _ => "linux",
        };
        Self {
            platform,
            arch: std::env::consts::ARCH,
            kind: if platform == "linux" {
                kind::TAR_GZ
            } else {
                kind::ZIP
            },
        }
    }
}

/// `text` as SemVer, with or without the tag's leading `v`.
#[must_use]
pub fn parse_version(text: &str) -> Option<Version> {
    Version::parse(text.trim().trim_start_matches('v')).ok()
}

/// SemVer precedence: build metadata does not count, a release outranks its pre-releases.
fn precedence(version: &Version) -> (u64, u64, u64, Prerelease) {
    (
        version.major,
        version.minor,
        version.patch,
        version.pre.clone(),
    )
}

/// Whether `candidate` is newer than `current`; anything unparseable is not.
#[must_use]
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => precedence(&candidate) > precedence(&current),
        _ => false,
    }
}

/// The newest release among the verified `manifests` that `channel` may offer over `current`,
/// with the artifact for `target`; `None` when this installation is up to date.
#[must_use]
pub fn newest_offer(
    current: &str,
    channel: Channel,
    manifests: &[UpdateManifest],
    target: &Target,
) -> Option<Offer> {
    let current = parse_version(current)?;
    manifests
        .iter()
        .filter_map(|manifest| Some((manifest, manifest.semver().ok()?)))
        .filter(|(manifest, version)| match channel {
            Channel::Stable => manifest.channel == Channel::Stable && version.pre.is_empty(),
            Channel::Beta => true,
        })
        .filter(|(_, version)| precedence(version) > precedence(&current))
        .max_by(|(_, left), (_, right)| precedence(left).cmp(&precedence(right)))
        .map(|(manifest, _)| Offer {
            version: manifest.version.clone(),
            channel: manifest.channel,
            released_at: manifest.released_at,
            notes: manifest.notes.clone(),
            artifact: manifest
                .artifacts
                .iter()
                .find(|artifact| {
                    artifact.platform == target.platform
                        && artifact.arch == target.arch
                        && artifact.kind == target.kind
                })
                .cloned(),
            schema_change: manifest.changes_schema(),
        })
}

#[cfg(test)]
#[path = "offer_tests.rs"]
mod tests;
