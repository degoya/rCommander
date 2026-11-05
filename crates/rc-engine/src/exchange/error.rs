//! The failures of an export and an import, each with a stable code. No message carries a
//! password, a secret or a value of the file: what was wrong with a file is said by place.

/// What stopped an export or an import; nothing was written then.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExchangeError {
    /// No part was chosen.
    #[error("choose at least one part to export")]
    NothingChosen,
    /// An export password was given, but empty.
    #[error("the export password is empty")]
    PasswordEmpty,
    /// The export password of a file with secrets is shorter than a master password may be.
    #[error("an export password for passwords and keys needs at least {min} characters")]
    PasswordShort { min: usize },
    /// Secrets in an unencrypted file without the person's confirmation that they are in clear.
    #[error("passwords and keys go into an unencrypted file only after its warning")]
    SecretsUnencrypted,
    /// The file to write exists and was not to be replaced.
    #[error("{path} exists already")]
    TargetExists { path: String },
    /// The file cannot be written.
    #[error("cannot write {path}: {source}")]
    TargetUnwritable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The file to import cannot be read, or is not a regular file.
    #[error("cannot read {path}: {source}")]
    FileUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The file to import is larger than an import reads.
    #[error("the file is larger than {max} bytes; it is not read")]
    FileTooLarge { max: u64 },
    /// The file is no export of this format, or a part of it breaks a cap.
    #[error("not a valid rCommander export: {reason}")]
    FileInvalid { reason: String },
    /// The file was written by a newer rCommander.
    #[error("the export has format version {version}, this build reads version 1")]
    VersionUnknown { version: u32 },
    /// The file is sealed and no export password was given.
    #[error("the export is encrypted; enter its export password")]
    PasswordNeeded,
    /// The export password does not open the file - or the file was changed.
    #[error("the export password is wrong, or the file was changed")]
    PasswordWrong,
    /// The sealed file asks more of argon2 than an import gives
    /// (`rc_vault::KdfParams::IMPORT_MAX`); nothing was derived.
    #[error("the export's key derivation asks more memory or time than an import allows")]
    KdfTooCostly,
}

impl ExchangeError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "exchange_nothing_chosen",
        "exchange_password_empty",
        "exchange_password_short",
        "exchange_secrets_unencrypted",
        "exchange_target_exists",
        "exchange_target_unwritable",
        "exchange_file_unreadable",
        "exchange_file_too_large",
        "exchange_file_invalid",
        "exchange_version_unknown",
        "exchange_password_needed",
        "exchange_password_wrong",
        "exchange_kdf_too_costly",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NothingChosen => "exchange_nothing_chosen",
            Self::PasswordEmpty => "exchange_password_empty",
            Self::PasswordShort { .. } => "exchange_password_short",
            Self::SecretsUnencrypted => "exchange_secrets_unencrypted",
            Self::TargetExists { .. } => "exchange_target_exists",
            Self::TargetUnwritable { .. } => "exchange_target_unwritable",
            Self::FileUnreadable { .. } => "exchange_file_unreadable",
            Self::FileTooLarge { .. } => "exchange_file_too_large",
            Self::FileInvalid { .. } => "exchange_file_invalid",
            Self::VersionUnknown { .. } => "exchange_version_unknown",
            Self::PasswordNeeded => "exchange_password_needed",
            Self::PasswordWrong => "exchange_password_wrong",
            Self::KdfTooCostly => "exchange_kdf_too_costly",
        }
    }
}
