//! [`EngineError::code`] and the list of the engine's own codes: one line per variant, apart
//! from the variants themselves (`error.rs`).

use rc_remote::{FtpError, SftpError};

use super::{
    CompareError, EngineError, ImportError, MultiRenameError, SessionError, SettingsError,
};

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
            /// [`crate::PluginAdminError::ALL_CODES`], a secret source's one of
            /// [`crate::SecretSourceError::ALL_CODES`], a workspace's one of
            /// [`crate::WorkspaceError::ALL_CODES`], an inventory's one of
            /// [`crate::inventory::InventorySourceError::ALL_CODES`], a port forward's one of
            /// [`crate::ForwardError::ALL_CODES`], a column set's one of
            /// [`crate::ColumnSetError::ALL_CODES`], an own command's one of
            /// [`crate::UserCommandError::ALL_CODES`] and a remote edit's one of
            /// [`crate::RemoteEditError::ALL_CODES`]; a jump host's failure keeps its own.
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
        EngineError::HostKeyRevoked { .. } => rc_remote::HOST_KEY_REVOKED,
        EngineError::HostKeyStale => "host_key_stale",
        EngineError::HostKeyQuestionNotFound { .. } => "host_key_question_not_found",
        EngineError::HostKeyNotChanged { .. } => "host_key_not_changed",
        EngineError::KnownHostNotFound { .. } => "known_host_not_found",
        EngineError::KnownHostsUnreadable { .. } => "known_hosts_unreadable",
        EngineError::Settings(SettingsError::RetentionUnknownTable { .. }) =>
            "retention_unknown_table",
        EngineError::Settings(SettingsError::RetentionOutOfRange { .. }) =>
            "retention_out_of_range",
        EngineError::MasterPasswordTooShort { .. } => "master_password_too_short",
        EngineError::SecretEmpty => "secret_empty",
        EngineError::KeyFormatUnknown => "key_format_unknown",
        EngineError::KeyPassphraseNeeded => "key_passphrase_needed",
        EngineError::KeyPassphraseWrong => "key_passphrase_wrong",
        EngineError::KeyImportOnly => "key_import_only",
        EngineError::TotpSeedInvalid { .. } => "totp_seed_invalid",
        EngineError::KeyFileUnreadable { .. } => "key_file_unreadable",
        EngineError::KeyPathRefused { .. } => "key_path_refused",
        EngineError::Import(ImportError::SshConfigPathRefused { .. }) => "ssh_config_path_refused",
        EngineError::Import(ImportError::SshConfigKeyMaterial { .. }) => "ssh_config_key_material",
        EngineError::Import(ImportError::SshConfigUnreadable { .. }) => "ssh_config_unreadable",
        EngineError::Import(ImportError::SshConfigHostUnknown { .. }) => "ssh_config_host_unknown",
        EngineError::Import(ImportError::TcIniPathRefused { .. }) => "tc_ini_path_refused",
        EngineError::Import(ImportError::TcIniUnreadable { .. }) => "tc_ini_unreadable",
        EngineError::Import(ImportError::TcIniTooLarge { .. }) => "tc_ini_too_large",
        EngineError::Import(ImportError::TcIniNotFound) => "tc_ini_not_found",
        EngineError::Import(ImportError::RdmPathRefused { .. }) => "rdm_path_refused",
        EngineError::Import(ImportError::RdmUnreadable { .. }) => "rdm_unreadable",
        EngineError::Import(ImportError::RdmTooLarge { .. }) => "rdm_too_large",
        EngineError::Import(ImportError::RdmEncrypted { .. }) => "rdm_encrypted",
        EngineError::Import(ImportError::RdmInvalid { .. }) => "rdm_invalid",
        EngineError::Import(ImportError::RdmNotAnExport { .. }) => "rdm_not_an_export",
        EngineError::InvalidSearch { .. } => "invalid_search",
        EngineError::Cancelled => "cancelled",
        EngineError::Compare(CompareError::CompareNeedsFiles) => "compare_needs_files",
        EngineError::Compare(CompareError::CompareChanged { .. }) => "compare_changed",
        EngineError::Compare(CompareError::CompareNotEditable { .. }) => "compare_not_editable",
        EngineError::Compare(CompareError::CompareBlockNotFound { .. }) =>
            "compare_block_not_found",
        EngineError::Compare(CompareError::SyncDeletesUnconfirmed { .. }) =>
            "sync_deletes_unconfirmed",
        EngineError::Compare(CompareError::SyncActionInvalid { .. }) => "sync_action_invalid",
        EngineError::Compare(CompareError::SyncRowNotFound { .. }) => "sync_row_not_found",
        EngineError::OpenFailed { .. } => "open_failed",
        EngineError::DriveNotFound { .. } => "drive_not_found",
        EngineError::DriveNotReady { .. } => "drive_not_ready",
        EngineError::Settings(SettingsError::SettingUnknown { .. }) => "setting_unknown",
        EngineError::Settings(SettingsError::SettingProtected { .. }) => "setting_protected",
        EngineError::Settings(SettingsError::SettingInvalid { .. }) => "setting_invalid",
        EngineError::SensitivePath { .. } => "sensitive_path",
        EngineError::EndpointChangeNeedsGui { .. } => "endpoint_change_needs_gui",
        EngineError::MultiRename(MultiRenameError::Busy) => "rename_busy",
        EngineError::MultiRename(MultiRenameError::NoChanges) => "rename_no_changes",
        EngineError::MultiRename(MultiRenameError::NothingToUndo) => "rename_nothing_to_undo",
        EngineError::MultiRename(MultiRenameError::UndoIncomplete) => "rename_undo_incomplete",
        EngineError::MultiRename(MultiRenameError::UndoMismatch { .. }) => "rename_undo_mismatch",
        EngineError::MultiRename(MultiRenameError::PresetInvalid { .. }) => "rename_preset_invalid",
        EngineError::MultiRename(MultiRenameError::PresetNotFound { .. }) =>
            "rename_preset_not_found",
        EngineError::Session(SessionError::ConnectionNotSsh { .. }) => "connection_not_ssh",
        EngineError::Session(SessionError::ExecInvalid { .. }) => "exec_invalid",
        EngineError::Session(SessionError::SessionPrefsInvalid { .. }) => "session_prefs_invalid",
        EngineError::PasswordNotStored { .. } => "password_not_stored",
        EngineError::Session(SessionError::FtpConnectionNotFound { .. }) =>
            "ftp_connection_not_found",
        EngineError::Session(SessionError::FtpConnectionAmbiguous { .. }) =>
            "ftp_connection_ambiguous",
        EngineError::McpQuestionNotFound { .. } => "mcp_question_not_found",
        EngineError::McpAnswerNotOffered { .. } => "mcp_answer_not_offered",
        EngineError::CommandLineInvalid { .. } => "command_line_invalid",
        EngineError::CommandLineLocalOnly => "command_line_local_only",
        EngineError::CommandLineFailed { .. } => "command_line_failed",
        EngineError::FilterInvalid { .. } => "filter_invalid",
        EngineError::ArchiveReadOnly => "archive_read_only",
        EngineError::NotAnArchive { .. } => "not_an_archive",
        EngineError::PropertyInvalid { .. } => "property_invalid",
        EngineError::CommentNotEncodable { .. } => "comment_not_encodable",
        EngineError::Session(SessionError::SftpConnectionNotFound { .. }) =>
            "sftp_connection_not_found",
        EngineError::Session(SessionError::SftpConnectionAmbiguous { .. }) =>
            "sftp_connection_ambiguous",
        EngineError::SavedQueueNotFound { .. } => "saved_queue_not_found",
        EngineError::SavedQueueChanged { .. } => "queue_changed",
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
        EngineError::Workspace(error) => error.code(),
        EngineError::Inventory(error) => error.code(),
        EngineError::Hop { source, .. } => source.code(),
        EngineError::Forward(error) => error.code(),
        EngineError::ColumnSet(error) => error.code(),
        EngineError::UserCommand(error) => error.code(),
        EngineError::RemoteEdit(error) => error.code(),
    }
}
