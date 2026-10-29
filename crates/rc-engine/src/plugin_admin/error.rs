//! Why a `plugins.*` command failed; each with a stable code.

/// A plugin command's failure.
#[derive(Debug, thiserror::Error)]
pub enum PluginAdminError {
    /// The engine has no plugin folder (`Engine::attach_plugins` was not called).
    #[error("plugins are not available in this run")]
    Unavailable,
    /// No installed plugin has this id.
    #[error("no installed plugin {id}")]
    NotFound { id: String },
    /// The package is refused: tampered, unsigned, by a revoked key, withdrawn, asking for an
    /// import its manifest does not grant ... - the host's code ([`rc_plugin_host::REFUSAL_CODES`]).
    #[error("{reason}")]
    Refused { code: &'static str, reason: String },
    /// Signed by a key nobody confirmed, and not confirmed with this call: the preview asks.
    #[error(
        "the package is signed by an untrusted key (fingerprint {fingerprint}); confirm it in the install preview"
    )]
    UntrustedKey { fingerprint: String },
    /// A rollback with no older version that verifies.
    #[error("plugin {id} has no previous version to go back to")]
    NoPreviousVersion { id: String },
    /// A plugin setting its manifest does not declare, or a value it does not take.
    #[error("plugin {id} has no setting {name} taking that value")]
    SettingRefused { id: String, name: String },
    /// A file could not be read or written.
    #[error("the plugin folder failed: {reason}")]
    Io { reason: String },
}

impl PluginAdminError {
    /// Every code of this enum, the host's refusals included.
    pub const ALL_CODES: &[&str] = &[
        "plugins_unavailable",
        "plugin_not_found",
        "plugin_no_previous_version",
        "plugin_setting_refused",
        "plugin_folder_failed",
        "plugin_key_untrusted",
        "plugin_invalid",
        "plugin_denied",
        "plugin_unsigned",
        "plugin_signature_invalid",
        "plugin_key_mismatch",
        "plugin_key_revoked",
        "plugin_digest_revoked",
        "plugin_needs_newer_app",
        "plugin_version_installed",
        "plugin_version_in_use",
        "plugin_key_changed",
        "plugin_package_changed",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "plugins_unavailable",
            Self::NotFound { .. } => "plugin_not_found",
            Self::Refused { code, .. } => code,
            Self::UntrustedKey { .. } => "plugin_key_untrusted",
            Self::NoPreviousVersion { .. } => "plugin_no_previous_version",
            Self::SettingRefused { .. } => "plugin_setting_refused",
            Self::Io { .. } => "plugin_folder_failed",
        }
    }

    /// A host failure: its refusal code when it carries one, a folder failure otherwise.
    pub(crate) fn from_host(error: &anyhow::Error) -> Self {
        let refused = error
            .chain()
            .any(|cause| cause.downcast_ref::<rc_plugin_host::Refusal>().is_some());
        if refused {
            Self::Refused {
                code: rc_plugin_host::refusal_code(error),
                reason: format!("{error:#}"),
            }
        } else {
            Self::Io {
                reason: format!("{error:#}"),
            }
        }
    }

    /// A verification's failure.
    pub(crate) fn from_verify(error: rc_plugin_host::VerifyError) -> Self {
        match error {
            rc_plugin_host::VerifyError::UntrustedKey { fingerprint, .. } => {
                Self::UntrustedKey { fingerprint }
            }
            rc_plugin_host::VerifyError::Other(error) => Self::Refused {
                code: rc_plugin_host::refusal_code(&error),
                reason: format!("{error:#}"),
            },
        }
    }
}

impl From<tokio::task::JoinError> for PluginAdminError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Io {
            reason: error.to_string(),
        }
    }
}
