//! One downloadable file of a release as the signed manifest lists it, and the rules every entry
//! obeys.
//!
//! Copied from rDownloader's `crates/rd-update/src/manifest/artifact.rs` at commit 26c8592a6
//! (RC-100-04); the kinds are rCommander's portable archives.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// One downloadable file. `platform`, `arch` and `kind` are open strings on purpose: a later
/// release that adds a platform must not make every older installation refuse the manifest;
/// what this build does not know it never selects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Artifact {
    /// `windows`, `linux` or `macos`.
    pub platform: String,
    /// `x86_64` or `aarch64`.
    pub arch: String,
    /// [`kind::ZIP`] or [`kind::TAR_GZ`].
    pub kind: String,
    /// Absolute `https://` URL under the source's download prefix.
    pub url: String,
    /// 64 lowercase hex characters.
    pub sha256: String,
    /// Exact byte size.
    pub size: u64,
}

/// The artifact kinds this build knows: the portable archives `scripts/package-*.sh` write.
pub mod kind {
    /// Windows and macOS (`rcommander-<v>-windows-x64.zip`, `...-macos-<arch>.zip`).
    pub const ZIP: &str = "zip";
    /// Linux (`rcommander-<v>-linux-<x64|arm64>.tar.gz`).
    pub const TAR_GZ: &str = "tar_gz";
}

impl Artifact {
    fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("platform", &self.platform),
            ("arch", &self.arch),
            ("kind", &self.kind),
        ] {
            if value.is_empty()
                || value.len() > 32
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            {
                return Err(format!("{name} {value:?} is not a lowercase identifier"));
            }
        }
        if self.url.len() > 1024 {
            return Err("the URL is longer than 1024 bytes".to_owned());
        }
        let url = url::Url::parse(&self.url).map_err(|error| format!("URL: {error}"))?;
        if url.scheme() != "https"
            || url.host_str().is_none_or(str::is_empty)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(format!("{} is not a plain https:// URL", self.url));
        }
        if !crate::install::steps::is_sha256(&self.sha256) {
            return Err("sha256 is not 64 lowercase hex characters".to_owned());
        }
        if self.size == 0 || self.size > super::MAX_ARTIFACT_BYTES {
            return Err(format!(
                "size {} is outside 1..={}",
                self.size,
                super::MAX_ARTIFACT_BYTES
            ));
        }
        Ok(())
    }
}

/// Refuses a list with an entry that breaks the rules above, or with one platform, architecture
/// and kind listed twice.
pub(super) fn validate_list(artifacts: &[Artifact]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for artifact in artifacts {
        let entry = format!("{}/{}/{}", artifact.platform, artifact.arch, artifact.kind);
        artifact
            .validate()
            .map_err(|reason| format!("{entry}: {reason}"))?;
        if !seen.insert((
            artifact.platform.as_str(),
            artifact.arch.as_str(),
            artifact.kind.as_str(),
        )) {
            return Err(format!("{entry} is listed twice"));
        }
    }
    Ok(())
}
