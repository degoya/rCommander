//! The signed update manifest (RC-100-04): which version a channel offers, and the bytes it is
//! made of.
//!
//! A manifest is a [`SignedDocument`] under [`UPDATE_MANIFEST_DOMAIN`], signed by the compiled-in
//! root `rcommander-update-v1` (`sign/roots.rs`). The release step builds one per release
//! (`rc-pack update manifest build`, `scripts/sign-update.sh`) and attaches it as
//! [`Channel::file_name`]: `rcommander-update-stable.json` to a plain `vX.Y.Z` release,
//! `rcommander-update-beta.json` to a `vX.Y.Z-beta.N` pre-release.
//!
//! The order of the checks: size (before a byte is parsed), envelope, revocation and signature,
//! schema version (after the signature, so an unsigned document declaring a future version reads
//! as unsigned), channel (the signed channel must be the one asked for), freshness, content.
//! **Additive fields are tolerated, incompatible changes bump the schema** (rDownloader's
//! addendum of 2026-09-30): a later release adding a field must not break the check of every
//! installation in the field.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/manifest.rs` at commit 26c8592a6:
//! rCommander's domain, names and fields; no installer, agent or plugin lists.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    error::UpdateError,
    sign::{
        self, EMBEDDED_KEYS, REVOKED_DOCUMENTS, SignedDocument, SigningKey, TrustStore,
        VerifyError, replay,
    },
};

mod artifact;
pub use artifact::{Artifact, kind};

/// Domain separator for the manifest's signatures (`sign/envelope.rs`).
pub const UPDATE_MANIFEST_DOMAIN: &str = "rcommander.update-manifest.v1";
/// The manifest layout this build understands. Anything else is refused as a whole.
pub const UPDATE_MANIFEST_SCHEMA_VERSION: u32 = 1;
/// Largest signed manifest accepted, checked before a byte of it is parsed.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Longest release note, in characters: plain text the window shows as text.
pub const MAX_NOTES_CHARS: usize = 8000;
/// Most artifacts one manifest may list: five portable archives today.
pub const MAX_ARTIFACTS: usize = 16;
/// Largest artifact a manifest may declare; the Windows zip is ~40 MB.
pub(crate) const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;
/// How long a manifest built without an explicit window stays valid. Long, because a stable
/// release can stay the newest for months; a replay can only hide a newer version, never install
/// an older one, since nothing below the running version is offered.
pub const DEFAULT_VALIDITY_DAYS: i64 = 180;
/// Longest window between `issued_at` and `not_after`.
pub const MAX_VALIDITY_DAYS: i64 = 366;

/// Which releases an installation is offered (owner, 2026-10-20: stable and beta, no nightly).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Plain `X.Y.Z` releases only.
    #[default]
    Stable,
    /// `X.Y.Z-beta.N` pre-releases as well as the stable releases.
    Beta,
}

impl std::fmt::Display for Channel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Channel {
    /// The name in the settings, the manifest and `update.status`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }

    /// The channel `value` names, if any.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            _ => None,
        }
    }

    /// The channel a build of `version` reads until someone chooses one: beta for a
    /// pre-release (`1.1.0-beta.1`), stable otherwise (owner, 2026-10-20).
    #[must_use]
    pub fn of_version(version: &str) -> Self {
        match crate::offer::parse_version(version) {
            Some(parsed) if !parsed.pre.is_empty() => Self::Beta,
            _ => Self::Stable,
        }
    }

    /// The release asset this channel's manifest is published as.
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Stable => "rcommander-update-stable.json",
            Self::Beta => "rcommander-update-beta.json",
        }
    }
}

/// The signed payload. Unknown fields are ignored; see the module documentation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateManifest {
    /// Always [`UPDATE_MANIFEST_SCHEMA_VERSION`] for a manifest this build accepts.
    pub schema_version: u32,
    /// The publisher's monotonic counter for this channel: the signing time in Unix seconds.
    pub sequence: u64,
    /// When the publisher says it signed this.
    pub issued_at: DateTime<Utc>,
    /// After this instant the manifest is stale. Required: without it a first contact would
    /// accept a replay of any manifest ever published.
    pub not_after: DateTime<Utc>,
    /// The channel this manifest is published on.
    pub channel: Channel,
    /// The offered version, SemVer without a leading `v`; a pre-release on beta only.
    pub version: String,
    /// When the release was published.
    pub released_at: DateTime<Utc>,
    /// Plain-text release notes from the version's `RELEASE-NOTES.md` section, one `- ` point per
    /// line, at most [`MAX_NOTES_CHARS`] characters.
    #[serde(default)]
    pub notes: String,
    /// The downloadable files, at most one per platform, architecture and kind.
    pub artifacts: Vec<Artifact>,
    /// Whether this version's `crates/rc-db/migrations/` differ from the release before it on its
    /// channel. Absent reads as `true` ([`Self::changes_schema`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_change: Option<bool>,
}

impl UpdateManifest {
    /// The freshness fields, as [`replay::check`] wants them.
    #[must_use]
    pub fn freshness(&self) -> replay::Freshness {
        replay::Freshness {
            sequence: self.sequence,
            issued_at: self.issued_at,
            not_after: Some(self.not_after),
        }
    }

    /// Whether installing this version changes the database schema; `true` when unsaid.
    #[must_use]
    pub fn changes_schema(&self) -> bool {
        self.schema_change.unwrap_or(true)
    }

    /// The offered version, parsed.
    ///
    /// # Errors
    ///
    /// `update.invalid` for no SemVer version.
    pub fn semver(&self) -> Result<semver::Version, UpdateError> {
        crate::offer::parse_version(&self.version)
            .ok_or_else(|| invalid(format!("{} is not a SemVer version", self.version)))
    }

    /// Refuses a manifest this build must not act on: bounds, formats, duplicates, and a version
    /// that contradicts its channel.
    ///
    /// # Errors
    ///
    /// `update.schema_version_unsupported` or `update.invalid`.
    pub fn validate(&self) -> Result<(), UpdateError> {
        if self.schema_version != UPDATE_MANIFEST_SCHEMA_VERSION {
            return Err(UpdateError::SchemaVersion {
                saw: u64::from(self.schema_version),
                expected: UPDATE_MANIFEST_SCHEMA_VERSION,
            });
        }
        if self.sequence == 0 {
            return Err(invalid("the sequence starts at 1"));
        }
        if self.not_after <= self.issued_at {
            return Err(invalid("not_after is not after issued_at"));
        }
        if self.not_after - self.issued_at > Duration::days(MAX_VALIDITY_DAYS) {
            return Err(invalid(format!(
                "valid for longer than {MAX_VALIDITY_DAYS} days"
            )));
        }
        let version = self.semver()?;
        // A beta on stable is what a stable installation must never be offered; a plain
        // version on beta is a manifest in the wrong place, and which half was meant is no guess.
        match (self.channel, version.pre.is_empty()) {
            (Channel::Stable, false) => {
                return Err(invalid(format!(
                    "{} is a pre-release on the stable channel",
                    self.version
                )));
            }
            (Channel::Beta, true) => {
                return Err(invalid(format!(
                    "{} is not a pre-release but is on the beta channel",
                    self.version
                )));
            }
            _ => {}
        }
        if self.notes.chars().count() > MAX_NOTES_CHARS {
            return Err(invalid(format!(
                "the notes are longer than {MAX_NOTES_CHARS} characters"
            )));
        }
        if self.artifacts.len() > MAX_ARTIFACTS {
            return Err(invalid(format!("more than {MAX_ARTIFACTS} artifacts")));
        }
        artifact::validate_list(&self.artifacts).map_err(invalid)
    }
}

fn invalid(reason: impl Into<String>) -> UpdateError {
    UpdateError::Invalid(reason.into())
}

/// The trust store of the compiled-in root, or [`UpdateError::NotConfigured`] while this build
/// carries none (the placeholder): a clear state instead of a signature error on every check.
///
/// # Errors
///
/// `update.not_configured`, or `update.failed` for a malformed table.
pub fn release_trust(now: DateTime<Utc>) -> Result<TrustStore, UpdateError> {
    if sign::keys_at(EMBEDDED_KEYS, now).is_empty() {
        return Err(UpdateError::NotConfigured);
    }
    sign::trust_store_at(EMBEDDED_KEYS, REVOKED_DOCUMENTS, now).map_err(UpdateError::Other)
}

/// Verifies a manifest fetched for `channel` against the compiled-in root.
///
/// `floor` is the highest sequence this installation accepted on `channel` (`None` on a first
/// contact). A manifest below it is a replay; one at it is the manifest already accepted - the
/// periodic check fetches the same one until the next release - not a replay of an older one,
/// since the publisher's counter is the signing time.
///
/// # Errors
///
/// See [`UpdateError`].
pub fn verify(
    bytes: &[u8],
    channel: Channel,
    floor: Option<u64>,
    now: DateTime<Utc>,
) -> Result<UpdateManifest, UpdateError> {
    verify_with(bytes, &release_trust(now)?, channel, floor, now)
}

/// [`verify`] against an explicit trust store: a test's, or the one [`release_trust`] built.
///
/// # Errors
///
/// See [`UpdateError`].
pub fn verify_with(
    bytes: &[u8],
    trust: &TrustStore,
    channel: Channel,
    floor: Option<u64>,
    now: DateTime<Utc>,
) -> Result<UpdateManifest, UpdateError> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(UpdateError::TooLarge);
    }
    let document = SignedDocument::parse(bytes)
        .map_err(|error| UpdateError::Malformed(format!("{error:#}")))?;
    let payload: serde_json::Value =
        document
            .verify(UPDATE_MANIFEST_DOMAIN, trust)
            .map_err(|error| match error {
                VerifyError::UntrustedKey { .. } => UpdateError::Untrusted(error.to_string()),
                VerifyError::BadSignature { .. } => UpdateError::BadSignature(error.to_string()),
                VerifyError::Revoked => UpdateError::Revoked,
                VerifyError::Other(other) => UpdateError::Malformed(format!("{other:#}")),
            })?;
    let saw = payload
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| UpdateError::Malformed("schema_version is missing".to_owned()))?;
    if saw != u64::from(UPDATE_MANIFEST_SCHEMA_VERSION) {
        return Err(UpdateError::SchemaVersion {
            saw,
            expected: UPDATE_MANIFEST_SCHEMA_VERSION,
        });
    }
    let manifest: UpdateManifest = serde_json::from_value(payload)
        .map_err(|error| UpdateError::Malformed(error.to_string()))?;
    if manifest.channel != channel {
        return Err(UpdateError::WrongChannel {
            expected: channel,
            saw: manifest.channel,
        });
    }
    if let Some(floor) = floor
        && manifest.sequence < floor
    {
        return Err(replay::StaleError::Replayed {
            saw: manifest.sequence,
            known: floor,
        }
        .into());
    }
    // The floor is checked above; this is expiry and the future-timestamp bound.
    replay::check(manifest.freshness(), None, now)?;
    manifest.validate()?;
    Ok(manifest)
}

/// Signs `manifest` under `key_id` and returns the document as it is published; refuses one
/// [`verify_with`] would refuse for its content or size.
///
/// # Errors
///
/// The manifest is invalid or the signed document too large.
pub fn sign(key_id: &str, key: &SigningKey, manifest: &UpdateManifest) -> anyhow::Result<Vec<u8>> {
    manifest.validate()?;
    let document = sign::sign_document(UPDATE_MANIFEST_DOMAIN, key_id, key, manifest)?;
    let bytes = serde_json::to_vec_pretty(&document)?;
    anyhow::ensure!(
        bytes.len() <= MAX_MANIFEST_BYTES,
        "the signed manifest is {} bytes, more than the {MAX_MANIFEST_BYTES} a reader accepts",
        bytes.len()
    );
    Ok(bytes)
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
pub(crate) mod tests;
