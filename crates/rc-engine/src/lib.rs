//! rCommander's GUI-free core: the two panels, the command registry every user action goes
//! through, and the event stream the GUI and the MCP server both watch.
//!
//! An [`Engine`] is cheap to clone (one `Arc` inside) and `Send + Sync`: the GUI holds one, the
//! MCP server another, and both see the same panels. Every change goes through
//! [`Engine::dispatch`] with a [`Command`] and is announced as an [`EngineEvent`] on the
//! broadcast channel of [`Engine::subscribe`], so an MCP `navigate` moves the panel the GUI
//! shows. Headless (`rcommander mcp` without a window) is the same engine with no GUI attached.
//!
//! The engine also owns the terminal sessions (`rc-term`): each belongs to a panel, which shows
//! one of them in place of its file list (RC-040-03); the `terminal.*` commands open, drive, show
//! and close them, `panel.toggle_terminal` and `panel.show_files` switch a panel's view. A panel
//! and its terminal follow each other's folder (RC-040-04, `commands/terminal_sync.rs`).
//! The database is attached once ([`Engine::attach_db`]): the SSH host-key trust store lives
//! there, and [`Engine::check_host_key`] is the gate every SSH connection passes; the
//! `diagnostics.*` commands read and clear its error log and change its retention limits there;
//! the `db.*` commands back it up and stage a restore.
//! Stored secrets are read only through [`Engine::resolve_secret`], which never asks a person by
//! itself (`secrets.rs`). The SSH agents' keys are listed by `agent.list_identities`, which never
//! touches the vault.
//! The SSH sessions of the connections are pooled here ([`sessions`], RC-060-01): one per
//! connection, every channel on it, `connections.connect`, `.disconnect` and `.test` steer them,
//! [`Engine::ssh_channel`] opens a channel for SFTP, the terminal and commands; `ssh.exec` runs
//! one command on one (RC-060-04).
//! `terminal.open_ssh` (RC-060-03) runs a shell on such a channel in a panel's terminal - an
//! SFTP panel's toggle opens one in the panel's folder - and plays the connection's login script
//! on it before the person types ([`LoginScriptRun`]).
//! The file search ([`search`]) walks any backend: `files.search` answers it page by page,
//! [`Engine::start_search`] streams it to the find-files dialog. A panel's flat view
//! ([`FlatView`]: Ctrl+B's branch view, search hits fed to the panel) lists files from a whole
//! branch, filled in the background (`panel.branch_view`, `panel.feed_paths`).
//! Compare by content ([`compare`]) and synchronize dirs ([`sync`]) work over any [`Vfs`]; the
//! `compare.*` commands run them to the end, the GUI calls them with a cancellation token.
//! The connections of the tree are file systems too (`remote.rs`): `ftp://<connection>/<path>`
//! lists and stats through an FTP session opened on first use with the stored password.
//! Copy, move, delete and mkdir are jobs of the operations queue ([`Engine::ops`], `rc-ops`):
//! an `ops.*` command answers at once with the queued job, and the job's progress, questions
//! and end arrive as [`EngineEvent::Job`]. Transfers to and from servers resume after a lost
//! connection and a restart (`transfers.rs`, [`Engine::restore_transfers`]; RC-060-07).
//! The settings are a typed schema over the database ([`settings`]): `settings.get` and
//! `settings.set`, the latter refusing every MCP and vault key.
//! Multi-rename (`multi_rename.*`, [`MultiRenameRequest`]) previews a rule with `rc-rename`,
//! carries the steps out as a rename job of the same queue and keeps every run in the undo log
//! of the database (`rename_log`), from which `multi_rename.undo` reverses the latest one; its
//! presets and the rule last used are two settings values (`multi_rename.preset_*`).
//! Login scripts ([`login_script`], RC-050-08) live in the database; the `login_scripts.*`
//! commands edit them and [`Engine::login_script_secrets`] gives a run its secrets.
//! The [`Keymap`] is data of this crate too: a key chord names a [`Command`], so the window holds
//! no key logic of its own; the keys in force are the standard's with the person's deviations,
//! the setting `keys.overrides` (`keymap.list`, `keymap.set`; RC-100-21).
//! The drive bar's entries are the engine's too (`drives.rs`): drives asked off the UI thread with
//! a time limit, the WSL distributions and the saved servers; `panel.change_drive` returns a panel
//! to the directory it last showed on that drive.
//! An MCP call the policy asks about waits for the person in the window ([`Engine::ask_mcp`],
//! `mcp_questions.rs`); only the window answers, with the `GuiOnly` command `mcp.answer`.
//! Each panel has folder tabs with a folder history, and the directory hotlist keeps favourite
//! folders ([`tabs`], `hotlist.rs`; RC-010-03).
//! The command line under the panels ([`command_line`]) runs through `cmdline.run`: `cd` and a
//! drive move the panel, any other line starts in the system's shell; the quick search and the
//! quick filter (`panel.filter`) match names as [`quick_search`] says.
//! Archives open in a panel like folders (`archives.rs`): each is a read-only backend of its own,
//! `..` at its root leaves it, and `archive.test` reads every entry through the operations queue.
//! A file opened from inside one gets a read-only copy in a temporary folder ([`unpacked`]),
//! which the app removes at exit, or at a later start once its process has ended.
//! Properties, attributes and times, links, occupied space, `descript.ion` comments and names
//! for the clipboard are the `file.*` commands of RC-020-06 (`commands/properties.rs`); the size
//! column's folder counts run in the background per panel (`commands/dir_count.rs`).
//! FTPS certificates the system roots do not vouch for wait for a person too (`certs.rs`,
//! RC-070-02): the trusted ones live in the database, `certificates.answer` and
//! `certificates.remove` are the window's alone.

pub mod about;
mod archives;
mod certs;
pub mod command_line;
mod commands;
mod comments;
pub mod compare;
mod connections;
mod dispatch;
mod drives;
mod error;
mod event;
pub mod exchange;
mod flat;
mod hostkeys;
mod hotlist;
pub mod import;
mod keymap;
mod login_scripts;
mod mask;
mod mcp_questions;
mod plugin_admin;
pub mod plugins;
mod properties;
mod question_desk;
pub mod quick_search;
mod read_guard;
mod reexports;
mod registry;
mod remote;
pub mod search;
pub mod secret_sources;
mod secrets;
pub mod session_lock;
pub mod sessions;
pub mod settings;
mod sort;
mod state;
pub mod sync;
pub mod tabs;
#[cfg(test)]
mod test_support;
mod transfers;
pub mod unpacked;
mod updates;
pub mod viewer;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
    time::Duration,
};

use rc_db::Db;
use rc_ops::{Backend, EventSink, Queue, QueueConfig, SystemTrash};
use rc_vault::Vault;
use rc_vfs::{BackendId, IoCancel, LocalFs, TimeoutFs, Timeouts, Vfs, WslCatalog};
use tokio::sync::broadcast;

pub use archives::{ArchiveInfo, archive_file, shown_path};
pub use certs::{
    CertificateAnswer, CertificateError, CertificateQuestion, CertificateQuestionKind,
};
pub use commands::file_ops::{ExternalProgram, FileDialog};
pub use commands::filter::QuickFilter;
pub use commands::terminal::TerminalInfo;
pub use commands::{
    DEFAULT_PAGE, ImportConflict, ImportReport, LoginRunState, LoginScriptRun, MAX_EXEC_COMMAND,
    MAX_EXEC_STDIN, MAX_PAGE, MAX_PRESET_NAME, MAX_PRESETS, MultiRenameRequest, PanelTerminal,
    RenamePreset, SSH_SHELL, SSH_TERM, SavedList, SearchHandle, Unloadable, cd_command,
    shell_quote,
};
pub use comments::{DESCRIPTION_FILE, MAX_COMMENT};
pub use connections::{
    AuthMethod, Connection, ConnectionDraft, ConnectionError, ConnectionFolder, ConnectionId,
    ConnectionKind, ConnectionPatch, ConnectionTree, DefaultView, FolderDeletion, FolderId,
    FolderNode, FtpDataMode, FtpEncoding, ImportStatus, MAX_DEPTH, TreeChange, TreeItem,
};
pub use drives::{
    DETAIL_TIMEOUT, Drive, DriveGroup, DriveKind, DriveSpace, DriveState, NETWORK_DRIVES,
    NetworkDriveError, drive_of, suggest_letter,
};
pub use error::EngineError;
pub use event::{EngineEvent, PanelChange, TerminalChange};
pub use flat::{FlatSource, FlatView, MAX_FLAT_ENTRIES};
pub use hostkeys::{HostKeyAnswer, HostKeyQuestion, HostKeyQuestionKind};
pub use hotlist::HotlistEntry;
pub use import::ssh_config_apply::{
    SshConfigHost, SshConfigReport, SshConfigSkip, SshConfigWarning,
};
pub use keymap::{
    Binding, Chord, KeyContext, Keymap, KeymapError, KeymapRow, Lock, MAX_OVERRIDE_TEXT,
    MAX_OVERRIDES, Override, Overrides, PHYSICAL_KEYS, Refusal, Reserved, WindowKey, physical_code,
};
pub use login_scripts::{
    LoginScript, LoginScriptDeletion, LoginScriptDraft, LoginScriptError, LoginScriptId,
    LoginScriptInfo, LoginScriptPatch, LoginScriptRecord, Pattern, PatternError, SCHEMA_VERSION,
    ScriptInvalid, ScriptSecretLabel, ScriptSecretRef, ScriptSecrets, SecretError, SecretSource,
    Step,
};
pub use mcp_questions::{
    MAX_MCP_GRANTS, MAX_MCP_QUESTIONS, MAX_SHOWN_CHARS, McpAnswer, McpArgument, McpAsk,
    McpFrozenPanel, McpGrant, McpPanelContext, McpPin, McpQuestion, McpSessionOffer, McpVerdict,
    readable as mcp_readable, with_mcp_pins,
};
pub use plugin_admin::{
    KeyStatus, PackagePreview, Permissions, PluginAdminError, PluginInfo, PluginSettingInfo,
    PluginStart, PluginsOutcome, VersionInfo, bundled_plugins_dir,
};
pub use properties::{DirSize, ItemProperties, NameFormat, PathUsage, PropertiesReport};
/// Total Commander's selection masks, for the window's colours by file type (RC-100-43).
pub use rc_ops::{Meter, mask::Mask};
pub use read_guard::{RDM_EXPORT_PASSWORDS, ReadJudge, SSH_PRIVATE_FILE, TC_FTP_PASSWORDS};
pub use registry::{
    BackupToRequest, COMMANDS, ChecksumDialog, ChecksumFilesRequest, ChecksumJob, ChecksumRequest,
    CombineRequest, Command, CommandClass, CommandSpec, ConfigExportRequest, ConfigImportRequest,
    CursorStep, CursorTarget, DeleteRequest, ErrorLogRequest, ImportKeyRequest, MaskRequest,
    MoveConnectionRequest, MoveFolderRequest, QuickConnectRequest, RdmImportRequest,
    RetentionRequest, SaveConnectionRequest, SaveFields, SearchRequest, SecretChange, SplitRequest,
    SshConfigImportRequest, SshExecRequest, TcImportRequest, TcPasswordsRequest, TransferRequest,
    spec,
};
pub use remote::{
    FTP_SCHEME, ftp_location,
    sftp::{SFTP_SCHEME, sftp_location},
};
pub use search::{SearchCancel, SearchEvent, SearchHit, SearchQuery, SearchStats};
pub use secrets::{
    KeyFormat, KeyInfo, MAX_KEY_BYTES, MIN_MASTER_PASSWORD_LEN, REDACTED, SecretFlags, SecretKind,
    SecretParam, SecretRef, StoredKey,
};
pub use sessions::{
    ChannelLease, SessionChange, SessionChannel, SessionPrefs, SessionStatus, SessionTestReport,
    SshState,
};
pub use settings::{SETTINGS, SettingValue};
pub use sort::{SortKey, SortOrder, extension, natural_cmp};
pub use tabs::{
    HISTORY_LIMIT, Relocated, TabId, TabView, TabsError,
    requests::{
        AddHotlistFolderRequest, AddHotlistRequest, HistoryJumpRequest, LockTabRequest,
        MoveHotlistRequest, MoveTabRequest, NextTabRequest, OpenHotlistRequest, OpenTabRequest,
        PanelRequest, SelectTabRequest, SwapRequest, TabRequest, TransferDirRequest,
        UpdateHotlistRequest,
    },
};
// Packing and unpacking (RC-030-03, RC-030-04), for the window's dialogs and the MCP tools.
pub use commands::default_archive_name;
pub use reexports::*;

/// How many events a slow subscriber may fall behind before it misses some
/// ([`broadcast::error::RecvError::Lagged`]); it then re-reads the state it cares about.
const EVENT_BUFFER: usize = 256;

/// The rows of a page until a window reports its own ([`Engine::set_page_rows`]).
const DEFAULT_PAGE_ROWS: usize = 20;

/// This machine's drives, each call held to the default time limits: a share that does not
/// answer never holds its caller.
fn local_fs() -> TimeoutFs {
    TimeoutFs::new(Arc::new(LocalFs::new()), Timeouts::default())
}

/// The core. Clones share everything.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

struct Inner {
    /// This machine's drives as the panels, the dialogs and MCP see them: every call within its
    /// time limit, stopped by `io_cancel` (RC-080-04).
    local: Arc<dyn Vfs>,
    /// Stops the calls on `local` that wait (`panel.cancel_io`); the queue's copies have their
    /// own time limits and their own cancel.
    io_cancel: IoCancel,
    /// The operations queue: copy, move, delete, mkdir.
    ops: Queue,
    state: Mutex<State>,
    /// The terminal sessions, by id; apart from `state` so a terminal never waits on a panel.
    terminals: Mutex<BTreeMap<TermId, commands::terminal::Terminal>>,
    events: broadcast::Sender<EngineEvent>,
    /// The WSL distributions, read on first use.
    wsl: WslCatalog,
    /// The vault of the engine's database, once attached ([`Engine::attach_vault`]).
    vault: OnceLock<Vault>,
    /// The database, attached once ([`Engine::attach_db`]).
    db: OnceLock<Db>,
    /// The host-key questions waiting for a person.
    host_keys: hostkeys::HostKeyDesk,
    /// Where `agent.list_identities` looks when fixed ([`Engine::attach_agent_sources`]).
    agent_sources: OnceLock<Vec<AgentSource>>,
    /// The fingerprints (never values) of the secrets seen, for [`Engine::scrub_secrets`].
    known_secrets: Mutex<secrets::Fingerprints>,
    /// Serializes storing a secret and removing the vault, so no secret lands in a removed one,
    /// and every write that makes a connection hold a secret with MCP's endpoint check.
    secret_writes: tokio::sync::Mutex<()>,
    /// The drives and what they answered.
    drives: drives::DriveCatalog,
    /// The judge MCP dispatches run under ([`Engine::attach_read_judge`]).
    read_judge: OnceLock<ReadJudge>,
    /// Taken while a multi-rename apply or undo starts its job (`multi_rename.rs`).
    rename_claim: std::sync::atomic::AtomicBool,
    /// The SSH sessions, one per connection.
    sessions: sessions::SessionPool,
    /// The open sessions of remote connections (FTP), by backend.
    remotes: remote::Remotes,
    /// The MCP calls waiting for the person in the window (RC-010-13).
    mcp_questions: mcp_questions::McpDesk,
    /// The archives open as backends (RC-030-02).
    archives: archives::ArchiveCatalog,
    /// The FTPS certificate questions waiting for a person (RC-070-02).
    certificates: certs::CertificateDesk,
    /// The plugin folder, its execution log and the plugins' health (RC-100-09).
    plugins: OnceLock<plugin_admin::PluginDesk>,
    /// The walks of `files.search` waiting for their next page (RC-080-50).
    search_pages: Mutex<commands::search::SearchPages>,
    /// The self-update: its state, the download, the hand-over (RC-100-05 ... RC-100-07).
    updates: updates::UpdateDesk,
}

struct State {
    left: PanelState,
    right: PanelState,
    active: PanelId,
    /// Rows per page of each panel, by [`PanelId::index`].
    page_rows: [usize; 2],
    /// Whether the panels list hidden and system entries (`panel.toggle_hidden`).
    show_hidden: bool,
    /// The directory each panel last showed on each drive, by [`PanelId::index`] and drive id.
    last_dirs: [BTreeMap<String, VfsPath>; 2],
    /// The terminal session each panel shows in its terminal view, by [`PanelId::index`]; kept
    /// while the panel shows its files, so the toggle returns to the same shell.
    shown: [Option<TermId>; 2],
    /// Both panels' tabs; the panel's own state is its active tab's.
    tabs: tabs::Tabs,
    /// The running folder counts of each panel's size column, by [`PanelId::index`].
    counting: [Option<tokio_util::sync::CancellationToken>; 2],
}

impl State {
    fn panel_mut(&mut self, id: PanelId) -> &mut PanelState {
        match id {
            PanelId::Left => &mut self.left,
            PanelId::Right => &mut self.right,
        }
    }

    fn panel(&self, id: PanelId) -> &PanelState {
        match id {
            PanelId::Left => &self.left,
            PanelId::Right => &self.right,
        }
    }
}

impl Engine {
    /// An engine whose panels show `left` and `right`, the left one active.
    ///
    /// # Errors
    ///
    /// Either directory cannot be listed.
    pub async fn open(left: VfsPath, right: VfsPath) -> Result<Self, EngineError> {
        Self::open_with(left, right, drives::system(), DETAIL_TIMEOUT).await
    }

    /// [`Engine::open`] over the drives of `source`, which may take `timeout` to answer.
    pub(crate) async fn open_with(
        left: VfsPath,
        right: VfsPath,
        source: Arc<dyn drives::DriveSource>,
        timeout: Duration,
    ) -> Result<Self, EngineError> {
        let io_cancel = IoCancel::new();
        let local: Arc<dyn Vfs> = Arc::new(local_fs().with_cancel(io_cancel.clone()));
        let left = PanelState::load(&local, left).await?;
        let right = PanelState::load(&local, right).await?;
        let tabs = tabs::Tabs::new(&left, &right);
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let sink: EventSink = {
            let events = events.clone();
            // No subscriber is not an error: a headless engine may have none.
            Arc::new(move |event: rc_ops::JobEvent| {
                let _ = events.send(EngineEvent::Job {
                    job: event.job,
                    change: event.change,
                });
            })
        };
        let ops = Queue::new(QueueConfig::default(), sink);
        let drives = drives::DriveCatalog::new(source, timeout, events.clone());
        ops.register(
            BackendId::Local,
            Backend::new(Arc::new(local_fs())).with_trash(SystemTrash::for_this_platform()),
        );
        let engine = Self {
            inner: Arc::new(Inner {
                local,
                io_cancel,
                ops,
                state: Mutex::new(State {
                    left,
                    right,
                    active: PanelId::Left,
                    page_rows: [DEFAULT_PAGE_ROWS; 2],
                    show_hidden: false,
                    last_dirs: Default::default(),
                    shown: [None; 2],
                    tabs,
                    counting: Default::default(),
                }),
                terminals: Mutex::new(BTreeMap::new()),
                events,
                wsl: WslCatalog::new(),
                vault: OnceLock::new(),
                db: OnceLock::new(),
                host_keys: hostkeys::HostKeyDesk::default(),
                agent_sources: OnceLock::new(),
                known_secrets: Mutex::new(secrets::Fingerprints::default()),
                secret_writes: tokio::sync::Mutex::new(()),
                drives,
                read_judge: OnceLock::new(),
                rename_claim: std::sync::atomic::AtomicBool::new(false),
                sessions: sessions::SessionPool::default(),
                remotes: remote::Remotes::default(),
                mcp_questions: mcp_questions::McpDesk::default(),
                archives: archives::ArchiveCatalog::default(),
                certificates: certs::CertificateDesk::default(),
                plugins: OnceLock::new(),
                search_pages: Mutex::default(),
                updates: updates::UpdateDesk::default(),
            }),
        };
        engine.install_transfer_hooks();
        Ok(engine)
    }

    /// An engine for a run without a window: both panels on `start`.
    ///
    /// # Errors
    ///
    /// `start` cannot be listed.
    pub async fn headless(start: VfsPath) -> Result<Self, EngineError> {
        Self::open(start.clone(), start).await
    }

    /// Gives the engine its database; the trust store and later the connections live there.
    /// Answers `false` when one was attached already (the first one stays).
    pub fn attach_db(&self, db: Db) -> bool {
        self.inner.db.set(db).is_ok()
    }

    /// The attached database.
    pub(crate) fn db(&self) -> Result<&Db, EngineError> {
        self.inner.db.get().ok_or(EngineError::DatabaseUnavailable)
    }

    /// A receiver of every change from now on.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events.subscribe()
    }

    /// The operations queue, where backends are registered and jobs can be awaited.
    #[must_use]
    pub fn ops(&self) -> &Queue {
        &self.inner.ops
    }

    /// The panel keyboard input goes to.
    #[must_use]
    pub fn active_panel(&self) -> PanelId {
        self.state().active
    }

    /// How many rows of `panel` fit in the window, what `panel.move_cursor` pages by. A layout
    /// fact the window reports, not a user action: no command, no event.
    pub fn set_page_rows(&self, panel: PanelId, rows: usize) {
        self.state().page_rows[panel.index()] = rows.max(1);
    }

    /// The backend of `path`.
    fn vfs(&self, path: &VfsPath) -> Result<Arc<dyn Vfs>, EngineError> {
        match path.backend {
            BackendId::Local => Ok(Arc::clone(&self.inner.local)),
            BackendId::Archive { .. } => Ok(self.archive_fs(&path.backend)?),
            // A remote backend once it is open (`Engine::open_vfs` opens one); a quick
            // connection's never for an MCP call.
            _ => {
                if let BackendId::Sftp { connection_id } = path.backend {
                    self.quick_for_call(ConnectionId(connection_id))?;
                }
                self.inner
                    .remotes
                    .open()
                    .get(&path.backend)
                    .cloned()
                    .ok_or(EngineError::BackendUnavailable)
            }
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn resolve(&self, panel: Option<PanelId>) -> PanelId {
        panel.unwrap_or_else(|| self.active_panel())
    }

    fn snapshot(&self, panel: Option<PanelId>) -> PanelSnapshot {
        let state = self.state();
        let id = panel.unwrap_or(state.active);
        state.panel(id).snapshot(id, id == state.active)
    }

    fn publish(&self, event: EngineEvent) {
        // No subscriber is not an error: a headless engine may have none.
        let _ = self.inner.events.send(event);
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
