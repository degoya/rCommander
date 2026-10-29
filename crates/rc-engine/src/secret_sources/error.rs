//! Why a secret could not be taken from its source, or a reference was refused; each with a
//! stable code. The source's name is in the message so the person knows which one to look at.

/// A secret source's failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SecretSourceError {
    /// The source runs and is locked: the person unlocks it in the manager's own window.
    #[error("the secret source {name} is locked; unlock it in the password manager and try again")]
    Locked { name: String },
    /// The manager behind the source does not answer.
    #[error("the secret source {name} does not answer; start the password manager's service")]
    Unreachable { name: String },
    /// The plugin has no valid licence.
    #[error("the secret source {name} has no valid licence")]
    Unlicensed { name: String },
    /// The plugin failed by itself: crashed, ran out of time, answered nothing or something
    /// invalid, or is suspended; `code` is the host's (`plugin_failed`, `plugin_timeout`, ...).
    #[error("the secret source {name} failed ({code})")]
    Failed { name: String, code: String },
    /// The source is ready and has no such entry: removed or renamed in the manager.
    #[error("the secret source {name} has no such entry; pick it again in the connection")]
    EntryMissing { name: String },
    /// No installed, enabled secret source has this name (removed, switched off, plugins off,
    /// or `wincred` off or not on Windows).
    #[error("the secret source {name} is not installed or is switched off")]
    Missing { name: String },
    /// A reference that cannot be stored: an empty or overlong entry id or label, a control
    /// character.
    #[error("the secret source reference is refused: {reason}")]
    Invalid { reason: &'static str },
}

impl SecretSourceError {
    /// Every code of this enum.
    pub const ALL_CODES: &[&str] = &[
        "secret_source_locked",
        "secret_source_unreachable",
        "secret_source_unlicensed",
        "secret_source_failed",
        "secret_source_entry_missing",
        "secret_source_missing",
        "secret_source_invalid",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Locked { .. } => "secret_source_locked",
            Self::Unreachable { .. } => "secret_source_unreachable",
            Self::Unlicensed { .. } => "secret_source_unlicensed",
            Self::Failed { .. } => "secret_source_failed",
            Self::EntryMissing { .. } => "secret_source_entry_missing",
            Self::Missing { .. } => "secret_source_missing",
            Self::Invalid { .. } => "secret_source_invalid",
        }
    }
}
