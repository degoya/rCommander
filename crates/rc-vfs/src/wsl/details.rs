//! The default distribution, the versions and the share host, and the list put together from
//! them.

use super::WslDistro;
use crate::{
    VfsPath,
    paths::{WSL_HOST, WSL_LEGACY_HOST},
};

/// The first Windows build that serves `\\wsl.localhost` (an Insider build of Windows 10; every
/// Windows 11 build is later). Earlier builds know only `\\wsl$`.
const LOCALHOST_FIRST_BUILD: u32 = 21354;

/// What is known beside the names: the default distribution and each one's WSL version.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Details {
    /// The default distribution's name.
    pub(crate) default: Option<String>,
    /// Name and WSL version (1 or 2) of each distribution whose version is known.
    pub(crate) versions: Vec<(String, u8)>,
}

impl Details {
    fn is_default(&self, name: &str) -> bool {
        self.default
            .as_deref()
            .is_some_and(|default| default.eq_ignore_ascii_case(name))
    }

    fn version(&self, name: &str) -> Option<u8> {
        self.versions
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, version)| *version)
    }
}

/// One subkey of `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss`: a distribution as WSL
/// registered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LxssEntry {
    /// The subkey's name, a GUID in braces.
    pub(crate) guid: String,
    /// The value `DistributionName`.
    pub(crate) name: String,
    /// The value `Version`.
    pub(crate) version: Option<u32>,
}

/// The details in the registry: `DefaultDistribution` names the default's subkey by its GUID.
pub(crate) fn from_registry(default_guid: Option<&str>, entries: &[LxssEntry]) -> Details {
    let bare = |guid: &str| guid.trim().trim_matches(['{', '}']).to_ascii_lowercase();
    let default = default_guid.map(bare).and_then(|wanted| {
        entries
            .iter()
            .find(|entry| bare(&entry.guid) == wanted)
            .map(|entry| entry.name.clone())
    });
    let versions = entries
        .iter()
        .filter_map(|entry| match entry.version {
            Some(1) => Some((entry.name.clone(), 1)),
            Some(2) => Some((entry.name.clone(), 2)),
            _ => None,
        })
        .collect();
    Details { default, versions }
}

/// The UNC host the distributions are reached under on Windows build `build`: `wsl.localhost`
/// where Windows serves it, `wsl$` before that build and when the build is unknown, since every
/// version still accepts it.
pub(crate) fn host_for_build(build: Option<u32>) -> &'static str {
    match build {
        Some(build) if build >= LOCALHOST_FIRST_BUILD => WSL_HOST,
        _ => WSL_LEGACY_HOST,
    }
}

/// The distributions `names` lists, in its order, with what `details` knows, each rooted at
/// `\\<host>\<name>\`.
pub(crate) fn assemble(names: Vec<String>, details: &Details, host: &str) -> Vec<WslDistro> {
    names
        .into_iter()
        .map(|name| WslDistro {
            is_default: details.is_default(&name),
            version: details.version(&name),
            root: VfsPath::local(format!(r"\\{host}\{name}\")),
            name,
        })
        .collect()
}

#[cfg(test)]
#[path = "details_tests.rs"]
mod tests;
