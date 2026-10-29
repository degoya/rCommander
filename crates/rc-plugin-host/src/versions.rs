//! Which installed version of a plugin runs.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/versions.rs` (RD-140-02) at
//! commit 26c8592a6, its rules unchanged:
//!
//! - The **default** version - the one that runs - is the chosen active version while it loaded
//!   and is not the staged one; otherwise the newest loaded version that is not staged.
//! - A **staged** version is never the default.
//! - Every other loaded version is **retained**: a previous version a rollback can return to.
//!
//! "Loaded" is the point: a withdrawn package, a revoked key or a tampered file never gets this
//! far, so it can be neither the default nor a rollback's target. Adapted: a version is pinned by
//! an open viewer instead of a download job (`installed::hold`), and an update installed beside a
//! running version waits for the next start as the engine's `pending` choice
//! (`docs/plugins.md`, "Installation and lifecycle").

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// What a person chose for one plugin id.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct VersionChoice {
    /// The version to run; `None` means the newest loaded one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    /// A version under test, never run as the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged: Option<String>,
}

/// The choices of every plugin that has one, by plugin id.
pub type VersionChoices = BTreeMap<String, VersionChoice>;

/// Where one loaded version stands, in the order consumers see them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionRole {
    /// The version that runs.
    Default,
    /// Kept: a rollback's target.
    Retained,
    /// Under test.
    Staged,
}

/// The default version out of the versions of one plugin that loaded; `None` only when every
/// one is the staged one.
#[must_use]
pub fn default_version<'a>(loaded: &[&'a str], choice: Option<&VersionChoice>) -> Option<&'a str> {
    let staged = choice.and_then(|choice| choice.staged.as_deref());
    if let Some(active) = choice.and_then(|choice| choice.active.as_deref())
        && Some(active) != staged
        && let Some(found) = loaded.iter().copied().find(|version| *version == active)
    {
        return Some(found);
    }
    loaded
        .iter()
        .copied()
        .filter(|version| Some(*version) != staged)
        .max_by(|left, right| compare_versions(left, right))
}

/// The role of `version`, given the plugin's default version and choice.
#[must_use]
pub fn version_role(
    version: &str,
    default: Option<&str>,
    choice: Option<&VersionChoice>,
) -> VersionRole {
    if default == Some(version) {
        VersionRole::Default
    } else if choice.and_then(|choice| choice.staged.as_deref()) == Some(version) {
        VersionRole::Staged
    } else {
        VersionRole::Retained
    }
}

/// The role of every `(plugin id, version)` pair, worked out per id.
#[must_use]
pub fn roles_by_id(
    pairs: &[(String, String)],
    choices: &VersionChoices,
) -> HashMap<(String, String), VersionRole> {
    let mut by_id: HashMap<&str, Vec<&str>> = HashMap::new();
    for (id, version) in pairs {
        by_id.entry(id.as_str()).or_default().push(version.as_str());
    }
    let mut roles = HashMap::new();
    for (id, versions) in by_id {
        let choice = choices.get(id);
        let default = default_version(&versions, choice);
        for version in versions {
            roles.insert(
                (id.to_owned(), version.to_owned()),
                version_role(version, default, choice),
            );
        }
    }
    roles
}

/// The newest loaded version below `current`: what a rollback returns to.
#[must_use]
pub fn previous_version<'a>(loaded: &[&'a str], current: &str) -> Option<&'a str> {
    loaded
        .iter()
        .copied()
        .filter(|version| compare_versions(version, current).is_lt())
        .max_by(|left, right| compare_versions(left, right))
}

/// SemVer ascending; a version that does not parse sorts below every one that does.
#[must_use]
pub fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    semver::Version::parse(left)
        .ok()
        .cmp(&semver::Version::parse(right).ok())
}

#[cfg(test)]
#[path = "versions_tests.rs"]
mod tests;
