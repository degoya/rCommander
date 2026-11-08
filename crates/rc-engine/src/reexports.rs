//! The types of the crates under the engine that its callers use through it: the window and
//! `rc-mcp` name `rc_engine::VfsPath`, `rc_engine::JobSnapshot`, ... and need no dependency of
//! their own on `rc-ops`, `rc-vfs`, `rc-db` and the rest (re-exported whole by `lib.rs`).

pub use rc_checksum::{
    Algorithm, ChecksumError, CombineReport, CreateReport, FileChecksum, Layout, MAX_PARTS,
    SplitReport, VerifyEntry, VerifyReport, VerifyStatus, VerifySummary, plain_name,
};
pub use rc_db::{
    BackupError, BackupInfo, BackupReport, ErrorLogEntry, KnownHost, LogLevel, RestoreReport,
    RetentionLimits, RetentionStatus,
};
pub use rc_ops::{AT_ONCE, DEFAULT_LIST, QueueEntry, QueueList, QueueListId, Reorder};
pub use rc_ops::{
    Answer, ApplyTo, AttributeFailure, AttributeReport, ConflictChoice, DeleteMode, DirUsage,
    ErrorChoice, ErrorPolicy, Failure, JobChange, JobId, JobKind, JobOptions, JobSnapshot,
    JobState, Progress, Question,
};
pub use rc_ops::{EntryInfo, JobReport, check_name};
/// The login-script model and engine: templates, `run_script`, `rehearse`.
pub use rc_remote::login_script;
pub use rc_remote::ssh::{AuthKind, AuthUsed, ExecOutput, PtyStream, SshChannel, open_pty_shell};
pub use rc_remote::{
    AgentError, AgentIdentity, AgentSource, AgentSourceKind, OfferedKey, WindowsAgents,
};
pub use rc_rename::{
    CaseMode, Change, Conflict, CounterSettings, InvalidName, MAX_NAMES_BYTES, MaskField,
    RenameContext, RenameError, RenameItem, RenamePlan, RenameRule, SUBFOLDER_SEPARATOR,
    SearchReplace, UndoMismatch, UndoProblem, apply_target_mask, names_from_bytes, names_from_text,
    names_to_bytes, nested_path,
};
pub use rc_term::{
    CursorPos, FollowStatus, PromptState, ScreenText, SessionState, ShellProfile, TermError,
    TermId, TermSession, TermSize, WSL_SHELL,
};
pub use rc_vault::{VaultStateKind, VaultStatus};
pub use rc_vfs::{
    ArchiveEntryInfo, BackendId, Details, Entry, EntryKind, LinkKind, LinkKinds, NetworkPath,
    Owner, PropertyCapabilities, PropertyChange, UnsafeName, VfsPath, WslDistro, network_path,
    unix_seconds,
};
/// What stops a comparison the window runs ([`crate::Engine::diff_paths`],
/// [`crate::Engine::plan_sync`]).
pub use tokio_util::sync::CancellationToken;
// Packing and unpacking (RC-030-03, RC-030-04), for the window's dialogs and the MCP tools.
pub use crate::commands::default_archive_name;
pub use rc_ops::{ExtractSettings, PackFormat, PackSettings, UpdateSettings};
/// Total Commander's selection masks, for the window's colours by file type (RC-100-43).
pub use rc_ops::{Meter, mask::Mask};
// Secrets taken from a source instead of the vault (RC-100-34).
pub use crate::secret_sources::{SecretSourceError, SecretSourceRef, SecretSourcesOutcome};
// Shared credentials in the connection tree (RC-050-12).
pub use crate::commands::SHARED_CREDENTIALS_KEY;
pub use crate::connections::{Credential, CredentialDraft, CredentialId, CredentialPatch};
pub use crate::registry::{CredentialFields, SaveCredentialRequest};
// The class a call really has, from its parameters (RC-080-42).
pub use crate::registry::{ESCALATIONS, Escalation, escalation};
// The engine's own types re-exported here to keep `lib.rs` under 500 lines.
pub use crate::state::{
    CursorView, FileDigest, ListMode, Outcome, PanelId, PanelSnapshot, PanelState, PanelView,
    Selection,
};
/// The self-update (RC-100-05 ... RC-100-07).
pub use crate::updates::{
    DownloadState, LastUpdate, REFUSAL_CODES as UPDATE_REFUSAL_CODES, UpdateNotice, UpdateRefusal,
    UpdateStatus, notice_of as update_notice_of,
};
pub use rc_update::Offer as UpdateOffer;
// The engine's own types moved here from `lib.rs`, to keep it under 500 lines.
pub use crate::archives::{ArchiveInfo, archive_file, shown_path};
pub use crate::certs::{
    CertificateAnswer, CertificateError, CertificateQuestion, CertificateQuestionKind,
};
pub use crate::comments::{DESCRIPTION_FILE, MAX_COMMENT};
pub use crate::hotlist::HotlistEntry;
// The workspaces (RC-100-68).
pub use crate::keymap::{
    MAX_WORKSPACE_KEY_DIGITS, WORKSPACE_KEY_PREFIX, workspace_key, workspace_key_name,
};
pub use crate::registry::{CreateWorkspaceRequest, UpdateWorkspaceRequest};
pub use crate::workspaces::{
    ApplyMode, PanelSpec, TabSpec, TermSpec, Workspace, WorkspaceError, WorkspaceId, WorkspaceSpec,
    WorkspacesOutcome,
};
// Jump hosts and local port forwards of SSH connections (RC-110-03).
pub use crate::connections::{Forward, ForwardDraft, LOOPBACK, MAX_FORWARDS, MAX_JUMPS};
pub use crate::forwards::{
    ForwardChange, ForwardError, ForwardView, ForwardsOutcome, MAX_FORWARD_CONNECTIONS, is_loopback,
};
pub use crate::hostkeys::HopLabel;
pub use crate::registry::SaveForwardRequest;
// The column sets and the comments view (RC-100-90).
pub use crate::columns::{
    Align, AutoDefault, Column, ColumnSet, ColumnSetError, ColumnSetId, ColumnSetSpec,
    ColumnSetsOutcome, Field, FolderComments, PanelColumns,
};
// The button bars (RC-100-87).
pub use crate::button_bars::{
    BarButton, BarItem, BarKind, BarPlace, BarSpec, ButtonAction, ButtonBar, ButtonBarError,
    ButtonBarId, ButtonBarsOutcome, IconRef,
};
// The own commands and the Start menu (RC-100-86).
pub use crate::keymap::{
    MAX_START_KEY_DIGITS, START_KEY_PREFIX, own_command_key, start_key, start_key_name,
};
pub use crate::user_commands::{
    AskRequest, AskSource, CommandKind, CommandRef, Show, StartItem, StartItemDraft, StartItemId,
    StartItemKind, UserCommand, UserCommandDraft, UserCommandError, UserCommandId,
    UserCommandsOutcome,
};
// The internal associations and the Lister's files (RC-100-91).
pub use crate::associations::{AssociationVerb, Associations, FileAssociation};
pub use crate::commands::viewer_window::ListerFiles;

pub use crate::remote_edit::{
    MAX_REMOTE_EDITS, RemoteEditChange, RemoteEditError, RemoteEditId, RemoteEditInfo,
    RemoteEditQuestion, RemoteEditState, UploadChoice,
};
pub use crate::search::{SearchCancel, SearchEvent, SearchHit, SearchQuery, SearchStats};
pub use crate::secrets::{
    KeyFormat, KeyInfo, MAX_KEY_BYTES, MIN_MASTER_PASSWORD_LEN, REDACTED, SecretFlags, SecretKind,
    SecretParam, SecretRef, StoredKey,
};
pub use crate::sessions::{
    ChannelLease, SessionChange, SessionChannel, SessionPrefs, SessionStatus, SessionTestReport,
    SshState,
};

pub use crate::login_scripts::{
    LoginScript, LoginScriptDeletion, LoginScriptDraft, LoginScriptError, LoginScriptId,
    LoginScriptInfo, LoginScriptPatch, LoginScriptRecord, Pattern, PatternError, SCHEMA_VERSION,
    ScriptInvalid, ScriptSecretLabel, ScriptSecretRef, ScriptSecrets, SecretError, SecretSource,
    Step,
};

pub use crate::tabs::{
    FrequentFolder, HISTORY_LIMIT, Relocated, TabId, TabView, TabsError,
    requests::{
        AddHotlistFolderRequest, AddHotlistRequest, HistoryJumpRequest, LockTabRequest,
        MoveHotlistRequest, MoveTabRequest, NextTabRequest, OpenHotlistRequest, OpenTabRequest,
        PanelRequest, SelectTabRequest, SwapRequest, TabRequest, TransferDirRequest,
        UpdateHotlistRequest,
    },
};
