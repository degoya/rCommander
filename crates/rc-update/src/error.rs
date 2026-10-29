//! Why a manifest, a check or a download was refused: [`UpdateError::code`] is the stable code the
//! window translates (`update.error.<code>` in `locales/en.json`) and `update.status` answers.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/manifest.rs` (`UpdateError`) at
//! commit 26c8592a6 (RC-100-04): rCommander's codes for a missing proxy login and for space.

use crate::{manifest::Channel, manifest::MAX_MANIFEST_BYTES, sign::replay::StaleError};

/// Why a manifest, a check or a download was refused.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("this build carries no update signing key; the update check is not configured")]
    NotConfigured,
    #[error("no update manifest is published for this channel yet")]
    NotPublished,
    #[error("the update manifest could not be fetched: {0}")]
    Fetch(String),
    #[error("the proxy asks for a login, which the update check does not give")]
    ProxyAuthRequired,
    #[error("the update manifest is larger than {MAX_MANIFEST_BYTES} bytes")]
    TooLarge,
    #[error("the update manifest cannot be read: {0}")]
    Malformed(String),
    #[error("the update manifest is not signed by a trusted update key: {0}")]
    Untrusted(String),
    #[error("the update manifest's signature does not verify: {0}")]
    BadSignature(String),
    #[error("the update manifest has been revoked")]
    Revoked,
    #[error("the update manifest declares schema version {saw}, this build reads {expected}")]
    SchemaVersion { saw: u64, expected: u32 },
    #[error("the update manifest is for the {saw} channel, not {expected}")]
    WrongChannel { expected: Channel, saw: Channel },
    #[error("the update manifest is stale: {0}")]
    Stale(#[from] StaleError),
    #[error("the update manifest is invalid: {0}")]
    Invalid(String),
    #[error("the download failed: {0}")]
    Download(String),
    #[error("the download is {saw} bytes, the manifest says {expected}")]
    SizeMismatch { expected: u64, saw: u64 },
    #[error("the downloaded file is not the one the manifest describes")]
    DigestMismatch,
    #[error("{free} bytes are free where the update goes, it needs {required}")]
    NoSpace { free: u64, required: u64 },
    #[error("the download was cancelled")]
    Cancelled,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Every code [`UpdateError::code`] answers, for the i18n test that each has a text.
pub const UPDATE_ERROR_CODES: &[&str] = &[
    "update.not_configured",
    "update.not_published",
    "update.fetch_failed",
    "update.proxy_auth_required",
    "update.too_large",
    "update.malformed",
    "update.untrusted",
    "update.bad_signature",
    "update.revoked",
    "update.schema_version_unsupported",
    "update.wrong_channel",
    "update.stale",
    "update.invalid",
    "update.download_failed",
    "update.size_mismatch",
    "update.digest_mismatch",
    "update.no_space",
    "update.cancelled",
    "update.failed",
];

impl UpdateError {
    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "update.not_configured",
            Self::NotPublished => "update.not_published",
            Self::Fetch(_) => "update.fetch_failed",
            Self::ProxyAuthRequired => "update.proxy_auth_required",
            Self::TooLarge => "update.too_large",
            Self::Malformed(_) => "update.malformed",
            Self::Untrusted(_) => "update.untrusted",
            Self::BadSignature(_) => "update.bad_signature",
            Self::Revoked => "update.revoked",
            Self::SchemaVersion { .. } => "update.schema_version_unsupported",
            Self::WrongChannel { .. } => "update.wrong_channel",
            Self::Stale(_) => "update.stale",
            Self::Invalid(_) => "update.invalid",
            Self::Download(_) => "update.download_failed",
            Self::SizeMismatch { .. } => "update.size_mismatch",
            Self::DigestMismatch => "update.digest_mismatch",
            Self::NoSpace { .. } => "update.no_space",
            Self::Cancelled => "update.cancelled",
            Self::Other(_) => "update.failed",
        }
    }

    /// Whether this says something about the manifest's integrity rather than about reaching
    /// it: what a check reports even when another channel's manifest verified.
    #[must_use]
    pub(crate) fn is_integrity(&self) -> bool {
        matches!(
            self,
            Self::TooLarge
                | Self::Malformed(_)
                | Self::Untrusted(_)
                | Self::BadSignature(_)
                | Self::Revoked
                | Self::SchemaVersion { .. }
                | Self::WrongChannel { .. }
                | Self::Stale(_)
                | Self::Invalid(_)
        )
    }
}
