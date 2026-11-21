//! Why a setting or a retention limit was not changed. Split off
//! [`EngineError`](super::EngineError) in RC-110-35; the codes are in `codes.rs`, the mapping
//! of [`rc_db::RetentionError`] in `convert.rs`.

/// A setting or a retention limit refused; each variant keeps its own code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SettingsError {
    /// `diagnostics.set_retention` named a table without retention limits.
    #[error("{table} is not a table with retention limits")]
    RetentionUnknownTable { table: String },
    /// `diagnostics.set_retention` asked for a limit outside the table's floor and ceiling.
    #[error("{key} must be between {min} and {max}, not {value}")]
    RetentionOutOfRange {
        key: &'static str,
        value: u64,
        min: u64,
        max: u64,
    },
    /// No setting has this key.
    #[error("{key} is not a setting")]
    SettingUnknown { key: String },
    /// Only a person in the rCommander window changes this setting (MCP and vault keys).
    #[error("{key} can only be changed in the rCommander window")]
    SettingProtected { key: String },
    /// The value does not fit the setting.
    #[error("{key}: {reason}")]
    SettingInvalid { key: String, reason: String },
}
