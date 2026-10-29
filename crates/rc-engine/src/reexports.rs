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
    CaseMode, Change, Conflict, CounterSettings, InvalidName, MaskField, RenameError, RenameItem,
    RenamePlan, RenameRule, SUBFOLDER_SEPARATOR, SearchReplace, UndoMismatch, UndoProblem,
    nested_path,
};
pub use rc_term::{
    CursorPos, FollowStatus, PromptState, ScreenText, SessionState, ShellProfile, TermError,
    TermId, TermSession, TermSize, WSL_SHELL,
};
pub use rc_vault::{VaultStateKind, VaultStatus};
pub use rc_vfs::{
    ArchiveEntryInfo, Details, Entry, EntryKind, LinkKind, LinkKinds, NetworkPath, Owner,
    PropertyCapabilities, PropertyChange, UnsafeName, VfsPath, WslDistro, network_path,
    unix_seconds,
};
/// What stops a comparison the window runs ([`crate::Engine::diff_paths`],
/// [`crate::Engine::plan_sync`]).
pub use tokio_util::sync::CancellationToken;
// Packing and unpacking (RC-030-03, RC-030-04), for the window's dialogs and the MCP tools.
pub use rc_ops::{ExtractSettings, PackFormat, PackSettings};
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
