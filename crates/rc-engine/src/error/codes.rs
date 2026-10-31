//! [`EngineError::code`] and the list of the engine's own codes: one line per variant, apart
//! from the variants themselves (`error.rs`).

use rc_remote::{FtpError, SftpError};

use super::EngineError;

/// Writes [`EngineError::ALL_CODES`] and [`EngineError::code`] from one list (RC-080-48): each
/// own variant with its code, once and in the order of `ALL_CODES`, and the variants that carry
/// another error with the way their code is read from it.
macro_rules! codes {
    (
        own { $($own:pat => $code:expr,)* }
        carried { $($carried:pat => $read:expr,)* }
    ) => {
        impl EngineError {
            /// Every code of the engine's own variants; a file-system failure carries one of
            /// [`rc_vfs::VfsError::ALL_CODES`] instead, a terminal's one of
            /// [`rc_term::TermError::ALL_CODES`], the connection tree's one of
            /// [`rc_db::connections::ConnectionError::ALL_CODES`], an operation one of
            /// [`rc_ops::OpsError::ALL_CODES`], an SSH agent's one of
            /// [`rc_remote::AgentError::ALL_CODES`], the vault's one of
            /// [`rc_vault::VaultError::ALL_CODES`], a checksum run's one of
            /// [`rc_checksum::ChecksumError::ALL_CODES`], a backup one of
            /// [`rc_db::BackupError::ALL_CODES`], an export or import one of
            /// [`crate::exchange::ExchangeError::ALL_CODES`], a multi-rename rule's one of
            /// [`rc_rename::RenameError::ALL_CODES`], a login script's one of
            /// [`rc_db::login_scripts::LoginScriptError::ALL_CODES`] or
            /// [`rc_remote::login_script::ScriptInvalid::ALL_CODES`], an SSH session's one of
            /// [`rc_remote::SshError::ALL_CODES`], an FTP session's one of
            /// [`rc_remote::FtpError::ALL_CODES`], an SFTP session's one of
            /// [`rc_remote::SftpError::ALL_CODES`], a certificate decision's one of
            /// [`crate::CertificateError::ALL_CODES`], a network drive's one of
            /// [`crate::NetworkDriveError::ALL_CODES`], a plugin command's one of
            /// [`crate::PluginAdminError::ALL_CODES`] and a secret source's one of
            /// [`crate::SecretSourceError::ALL_CODES`].
            pub const ALL_CODES: &[&str] = &[$($code,)*];

            /// The stable code.
            #[must_use]
            pub fn code(&self) -> &'static str {
                match self {
                    $($carried => $read,)*
                    $($own => $code,)*
                }
            }
        }
    };
}

codes! {
    own {
        EngineError::EntryNotFound { .. } => "entry_not_found",
        EngineError::CursorOutOfRange { .. } => "cursor_out_of_range",
        EngineError::BackendUnavailable => "backend_unavailable",
        EngineError::VaultLocked => "vault_locked",
        EngineError::DatabaseUnavailable => "database_unavailable",
        EngineError::GuiOnly { .. } => "gui_only",
        EngineError::Database(_) => "database_error",
        EngineError::HostKeyUnknown { .. } => rc_remote::HOST_KEY_UNKNOWN,
        EngineError::HostKeyChanged { .. } => rc_remote::HOST_KEY_CHANGED,
        EngineError::HostKeyStale => "host_key_stale",
        EngineError::HostKeyQuestionNotFound { .. } => "host_key_question_not_found",
        EngineError::HostKeyNotChanged { .. } => "host_key_not_changed",
        EngineError::KnownHostNotFound { .. } => "known_host_not_found",
        EngineError::KnownHostsUnreadable { .. } => "known_hosts_unreadable",
        EngineError::RetentionUnknownTable { .. } => "retention_unknown_table",
        EngineError::RetentionOutOfRange { .. } => "retention_out_of_range",
        EngineError::MasterPasswordTooShort { .. } => "master_password_too_short",
        EngineError::SecretEmpty => "secret_empty",
        EngineError::KeyFormatUnknown => "key_format_unknown",
        EngineError::KeyPassphraseNeeded => "key_passphrase_needed",
        EngineError::KeyPassphraseWrong => "key_passphrase_wrong",
        EngineError::KeyImportOnly => "key_import_only",
        EngineError::KeyFileUnreadable { .. } => "key_file_unreadable",
        EngineError::KeyPathRefused { .. } => "key_path_refused",
        EngineError::SshConfigPathRefused { .. } => "ssh_config_path_refused",
        EngineError::SshConfigKeyMaterial { .. } => "ssh_config_key_material",
        EngineError::SshConfigUnreadable { .. } => "ssh_config_unreadable",
        EngineError::SshConfigHostUnknown { .. } => "ssh_config_host_unknown",
        EngineError::TcIniPathRefused { .. } => "tc_ini_path_refused",
        EngineError::TcIniUnreadable { .. } => "tc_ini_unreadable",
        EngineError::TcIniTooLarge { .. } => "tc_ini_too_large",
        EngineError::TcIniNotFound => "tc_ini_not_found",
        EngineError::RdmPathRefused { .. } => "rdm_path_refused",
        EngineError::RdmUnreadable { .. } => "rdm_unreadable",
        EngineError::RdmTooLarge { .. } => "rdm_too_large",
        EngineError::RdmEncrypted { .. } => "rdm_encrypted",
        EngineError::RdmInvalid { .. } => "rdm_invalid",
        EngineError::RdmNotAnExport { .. } => "rdm_not_an_export",
        EngineError::InvalidSearch { .. } => "invalid_search",
        EngineError::Cancelled => "cancelled",
        EngineError::CompareNeedsFiles => "compare_needs_files",
        EngineError::SyncDeletesUnconfirmed { .. } => "sync_deletes_unconfirmed",
        EngineError::SyncActionInvalid { .. } => "sync_action_invalid",
        EngineError::SyncRowNotFound { .. } => "sync_row_not_found",
        EngineError::OpenFailed { .. } => "open_failed",
        EngineError::DriveNotFound { .. } => "drive_not_found",
        EngineError::DriveNotReady { .. } => "drive_not_ready",
        EngineError::SettingUnknown { .. } => "setting_unknown",
        EngineError::SettingProtected { .. } => "setting_protected",
        EngineError::SettingInvalid { .. } => "setting_invalid",
        EngineError::SensitivePath { .. } => "sensitive_path",
        EngineError::EndpointChangeNeedsGui { .. } => "endpoint_change_needs_gui",
        EngineError::RenameBusy => "rename_busy",
        EngineError::RenameNoChanges => "rename_no_changes",
        EngineError::RenameNothingToUndo => "rename_nothing_to_undo",
        EngineError::RenameUndoIncomplete => "rename_undo_incomplete",
        EngineError::RenameUndoMismatch { .. } => "rename_undo_mismatch",
        EngineError::RenamePresetInvalid { .. } => "rename_preset_invalid",
        EngineError::RenamePresetNotFound { .. } => "rename_preset_not_found",
        EngineError::ConnectionNotSsh { .. } => "connection_not_ssh",
        EngineError::ExecInvalid { .. } => "exec_invalid",
        EngineError::SessionPrefsInvalid { .. } => "session_prefs_invalid",
        EngineError::PasswordNotStored { .. } => "password_not_stored",
        EngineError::FtpConnectionNotFound { .. } => "ftp_connection_not_found",
        EngineError::FtpConnectionAmbiguous { .. } => "ftp_connection_ambiguous",
        EngineError::McpQuestionNotFound { .. } => "mcp_question_not_found",
        EngineError::McpAnswerNotOffered { .. } => "mcp_answer_not_offered",
        EngineError::CommandLineInvalid { .. } => "command_line_invalid",
        EngineError::CommandLineLocalOnly => "command_line_local_only",
        EngineError::CommandLineFailed { .. } => "command_line_failed",
        EngineError::FilterInvalid { .. } => "filter_invalid",
        EngineError::ArchiveReadOnly => "archive_read_only",
        EngineError::NotAnArchive { .. } => "not_an_archive",
        EngineError::PropertyInvalid { .. } => "property_invalid",
        EngineError::SftpConnectionNotFound { .. } => "sftp_connection_not_found",
        EngineError::SftpConnectionAmbiguous { .. } => "sftp_connection_ambiguous",
        EngineError::SavedQueueNotFound { .. } => "saved_queue_not_found",
        EngineError::QueueListEmpty => "queue_list_empty",
        EngineError::PanelChanged { .. } => "panel_changed",
        EngineError::ThemeUnknown { .. } => "theme_unknown",
        EngineError::ThemeWrongMode { .. } => "theme_wrong_mode",
        EngineError::ThemeBuiltin { .. } => "theme_builtin",
        EngineError::ThemeInUse { .. } => "theme_in_use",
        EngineError::ThemeNameTaken { .. } => "theme_name_taken",
        EngineError::ThemeInvalid { .. } => "theme_invalid",
    }
    carried {
        // A session failure inside a file-system call keeps the FTP or SFTP code; a command
        // an FTP server does not implement is named (`ftp_unsupported_epsv`, ...).
        EngineError::Vfs(error) => FtpError::from_vfs(error)
            .map(FtpError::code)
            .or_else(|| SftpError::from_vfs(error).map(SftpError::code))
            .or_else(|| rc_remote::ftp::unsupported_code(error))
            .unwrap_or_else(|| error.code()),
        EngineError::Ops(error) => error.code(),
        EngineError::Term(error) => error.code(),
        EngineError::Connection(error) => error.code(),
        EngineError::Backup(error) => error.code(),
        EngineError::Exchange(error) => error.code(),
        EngineError::Agent(error) => error.code(),
        EngineError::Vault(error) => error.code(),
        EngineError::Checksum(error) => error.code(),
        EngineError::Rename(error) => error.code(),
        EngineError::LoginScript(error) => error.code(),
        EngineError::LoginScriptInvalid(error) => error.code(),
        EngineError::Ssh(error) => error.code(),
        EngineError::Ftp(error) => error.code(),
        EngineError::Tabs(error) => error.code(),
        EngineError::Sftp(error) => error.code(),
        EngineError::Certificate(error) => error.code(),
        EngineError::NetworkDrive(error) => error.code(),
        EngineError::Plugin(error) => error.code(),
        EngineError::SecretSource(error) => error.code(),
        EngineError::Update(error) => error.code(),
    }
}
