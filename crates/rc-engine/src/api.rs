//! The engine's own types at the crate's root: what the window and `rc-mcp` name as
//! `rc_engine::Command`, `rc_engine::EngineError`, ... (re-exported whole by `lib.rs`, as
//! `reexports.rs` is for the types of the crates under the engine; split off in RC-110-30).

pub use crate::commands::file_ops::{ExternalProgram, FileDialog};
pub use crate::commands::{
    DEFAULT_PAGE, ImportConflict, ImportReport, LoginRunState, LoginScriptRun, MAX_EXEC_COMMAND,
    MAX_EXEC_STDIN, MAX_PAGE, MAX_PRESET_NAME, MAX_PRESETS, MultiRenameRequest, PanelTerminal,
    RenamePreset, SSH_SHELL, SSH_TERM, SavedList, SearchHandle, Unloadable, cd_command,
    shell_quote,
};
pub use crate::commands::{
    file_filters::{MAX_SAVED_FILTERS, SavedFilter},
    filter::QuickFilter,
    terminal::TerminalInfo,
};
pub use crate::connections::{
    AuthMethod, Connection, ConnectionDraft, ConnectionError, ConnectionFolder, ConnectionId,
    ConnectionKind, ConnectionPatch, ConnectionTree, DefaultView, FolderDeletion, FolderId,
    FolderNode, FtpDataMode, FtpEncoding, ImportStatus, MAX_DEPTH, TreeChange, TreeItem,
};
pub use crate::drives::{
    DETAIL_TIMEOUT, Drive, DriveGroup, DriveKind, DriveSpace, DriveState, NETWORK_DRIVES,
    NetworkDriveError, devices::DEVICE_SCHEME, drive_of, suggest_letter,
};
pub use crate::error::EngineError;
pub use crate::event::{EngineEvent, PanelChange, TerminalChange};
pub use crate::file_filter::FileFilter;
pub use crate::flat::{FlatSource, FlatView, MAX_FLAT_ENTRIES};
pub use crate::hostkeys::{HostKeyAnswer, HostKeyQuestion, HostKeyQuestionKind};
pub use crate::import::ssh_config_apply::{
    SshConfigHost, SshConfigReport, SshConfigSkip, SshConfigWarning,
};
pub use crate::keymap::{
    Binding, Chord, KeyContext, Keymap, KeymapError, KeymapRow, Lock, MAX_OVERRIDE_TEXT,
    MAX_OVERRIDES, MAX_THEME_KEY_ID, Override, Overrides, PHYSICAL_KEYS, Refusal, Reserved,
    THEME_KEY_PREFIX, WindowKey, physical_code, theme_key, theme_key_name,
};
pub use crate::mcp_questions::{
    MAX_MCP_GRANTS, MAX_MCP_QUESTIONS, MAX_SHOWN_CHARS, McpAnswer, McpArgument, McpAsk,
    McpFrozenPanel, McpGrant, McpJudged, McpPanelContext, McpPin, McpQuestion, McpSessionOffer,
    McpVerdict, readable as mcp_readable, with_mcp_judged, with_mcp_pins,
};
pub use crate::plugin_admin::{
    KeyStatus, PackagePreview, Permissions, PluginAdminError, PluginInfo, PluginSettingInfo,
    PluginStart, PluginsOutcome, VersionInfo, bundled_plugins_dir,
};
pub use crate::properties::{DirSize, ItemProperties, NameFormat, PathUsage, PropertiesReport};
pub use crate::read_guard::{RDM_EXPORT_PASSWORDS, ReadJudge, SSH_PRIVATE_FILE, TC_FTP_PASSWORDS};
pub use crate::registry::{
    BackupToRequest, COMMANDS, ChecksumDialog, ChecksumFilesRequest, ChecksumJob, ChecksumRequest,
    CombineRequest, Command, CommandClass, CommandSpec, ConfigExportRequest, ConfigImportRequest,
    CursorStep, CursorTarget, DeleteRequest, DuplicatesRequest, ErrorLogRequest, ImportKeyRequest,
    MaskRequest, MoveConnectionRequest, MoveFolderRequest, QuickConnectRequest, RdmImportRequest,
    RetentionRequest, SaveConnectionRequest, SaveFields, SearchRequest, SecretChange, SplitRequest,
    SshConfigImportRequest, SshExecRequest, TcImportRequest, TcPasswordsRequest, TransferRequest,
    ZoomStep, spec,
};
pub use crate::remote::{
    FTP_SCHEME, ftp_location,
    sftp::{SFTP_SCHEME, sftp_location},
};
pub use crate::settings::{SETTINGS, SettingValue};
pub use crate::sort::{SortKey, SortOrder, extension, natural_cmp};
