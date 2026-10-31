//! The panels' state and what a command answers.

use std::{collections::BTreeMap, sync::Arc};

use rc_checksum::{Algorithm, CombineReport, CreateReport, SplitReport, VerifyReport};
use rc_db::KnownHost;
use rc_ops::JobSnapshot;
use rc_rename::RenamePlan;
use rc_term::{ScreenText, SessionState, ShellProfile, TermId};
use rc_vfs::{ArchiveItem, Entry, Vfs, VfsPath, WslDistro};
use serde::{Deserialize, Serialize};

use crate::{
    AgentIdentity, AgentSourceKind, ArchiveInfo, BackupReport, Connection, ConnectionFolder,
    ConnectionId, ConnectionKind, ConnectionTree, Drive, EngineError, ErrorLogEntry, ExecOutput,
    HostKeyAnswer, ImportReport, LoginScriptDeletion, LoginScriptId, LoginScriptInfo,
    PanelTerminal, QuickFilter, RestoreReport, RetentionStatus, SearchHit, SearchStats,
    SessionPrefs, SessionStatus, SessionTestReport, SettingValue, SortOrder, SshConfigReport,
    TerminalInfo, TreeItem, VaultStatus,
    compare::FileComparison,
    flat::FlatView,
    properties::{DirSize, PathUsage, PropertiesReport},
    sort::{names_for, sort_entries},
    sync::DirComparison,
    viewer::{HexWindow, TextWindow},
};

mod panel_id;
mod selection;

pub use panel_id::{CursorView, ListMode, PanelId, PanelView};
pub use selection::Selection;
pub(crate) use selection::{by_name, unlisted};

/// What a panel shows: a directory, its entries in display order, the cursor and the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelState {
    /// The directory shown.
    pub path: VfsPath,
    /// Its entries in `sort` order, directories first; hidden and system entries only when
    /// `show_hidden`. Shared, so a copy of the state for the window costs no copy of a long
    /// listing.
    pub entries: Arc<[Entry]>,
    /// Index of the entry under the cursor; 0 in an empty directory.
    pub cursor: usize,
    /// Names of the selected entries, shared with every copy of the state until it changes.
    pub selection: Selection,
    /// The file list or the terminal; kept when the panel moves to another directory.
    pub view: PanelView,
    /// The order of the entries; kept when the panel moves to another directory.
    pub sort: SortOrder,
    /// Full or brief; kept when the panel moves to another directory.
    pub mode: ListMode,
    /// Whether the entries include hidden and system ones (one switch for both panels).
    pub show_hidden: bool,
    /// The selection before the last file operation, which Num / brings back.
    pub remembered: Selection,
    /// The quick filter (Ctrl+S): while it is in force `entries` hold only what it matches.
    pub filter: QuickFilter,
    /// The flat view (Ctrl+B, or search hits fed to the panel) while the panel shows one:
    /// `entries` are then files from below `path`, each named by its path relative to it.
    /// Gone with the next directory the panel shows.
    pub flat: Option<FlatView>,
    /// Inside an archive, what it says about each entry (packed size, encrypted, unsafe name);
    /// `None` elsewhere.
    pub archive: Option<Arc<ArchiveInfo>>,
    /// Folders whose size was counted, or is being counted, by name (Space, Alt+Shift+Enter);
    /// empty again with every new listing.
    pub dir_sizes: BTreeMap<String, DirSize>,
    /// The run of a listing still arriving (RC-060-02): `entries` are what came so far, and more
    /// follow (`commands/panel_listing.rs`); `None` once the listing is complete.
    pub listing: Option<u64>,
}

impl PanelState {
    /// The state of a panel freshly showing `path`.
    pub(crate) async fn load(vfs: &Arc<dyn Vfs>, path: VfsPath) -> Result<Self, EngineError> {
        let entries = list_sorted(vfs, &path, SortOrder::default(), false).await?;
        Ok(Self::showing(path, entries))
    }

    pub(crate) fn showing(path: VfsPath, entries: Vec<Entry>) -> Self {
        Self {
            path,
            entries: entries.into(),
            cursor: 0,
            selection: Selection::default(),
            view: PanelView::Files,
            sort: SortOrder::default(),
            mode: ListMode::default(),
            show_hidden: false,
            remembered: Selection::default(),
            filter: QuickFilter::default(),
            flat: None,
            archive: None,
            dir_sizes: BTreeMap::new(),
            listing: None,
        }
    }

    /// The state after this one when the panel shows `path` with `entries`, listed in `sort`
    /// order with `show_hidden`: view, sort, mode and the remembered selection stay, and the
    /// quick filter in the same directory. Entries listed in another order than the panel has by
    /// now are sorted again.
    pub(crate) fn next(
        &self,
        path: VfsPath,
        mut entries: Vec<Entry>,
        sort: SortOrder,
        show_hidden: bool,
    ) -> Self {
        if sort != self.sort {
            sort_entries(&mut entries, self.sort, names_for(&path));
        }
        let mut next = Self {
            view: self.view,
            sort: self.sort,
            mode: self.mode,
            show_hidden,
            remembered: self.remembered.clone(),
            ..Self::showing(path, entries)
        };
        next.carry_filter(self);
        next
    }

    /// The entry under the cursor.
    #[must_use]
    pub fn cursor_entry(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    /// Index of the entry named `name`.
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.name == name)
    }

    pub(crate) fn snapshot(&self, panel: PanelId, active: bool) -> PanelSnapshot {
        PanelSnapshot {
            panel,
            active,
            path: self.path.clone(),
            cursor: self.cursor_entry().map(|entry| CursorView {
                index: self.cursor,
                name: entry.name.clone(),
            }),
            selection: self.selection.clone(),
            entry_count: self.entries.len(),
            view: self.view,
            sort: self.sort,
            mode: self.mode,
            show_hidden: self.show_hidden,
            filter: self.filter.pattern.clone(),
            flat: self.flat.clone(),
            terminal: None,
            loading: self.listing.is_some(),
        }
    }
}

/// The entries of `path` in `sort` order; hidden and system entries only with `show_hidden`.
pub(crate) async fn list_sorted(
    vfs: &Arc<dyn Vfs>,
    path: &VfsPath,
    sort: SortOrder,
    show_hidden: bool,
) -> Result<Vec<Entry>, EngineError> {
    let mut entries = vfs.list(&path.path).await?;
    if !show_hidden {
        entries.retain(|entry| !is_hidden(entry));
    }
    sort_entries(&mut entries, sort, names_for(path));
    Ok(entries)
}

/// Whether a panel hides `entry` unless hidden files are shown: a dot file, or on Windows the
/// hidden or the system attribute, which Total Commander's switch covers together.
pub(crate) fn is_hidden(entry: &Entry) -> bool {
    entry.hidden || entry.attributes.windows.is_some_and(|flags| flags.system)
}

/// A panel as a command reports it: everything but the entries themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelSnapshot {
    pub panel: PanelId,
    /// Whether keyboard input goes to this panel.
    pub active: bool,
    pub path: VfsPath,
    /// The entry under the cursor; `None` in an empty directory.
    pub cursor: Option<CursorView>,
    /// The selected names, sorted; the panel's own set, not a copy of it.
    pub selection: Selection,
    pub entry_count: usize,
    /// The file list or the terminal.
    pub view: PanelView,
    /// The order of the entries.
    pub sort: SortOrder,
    /// Full or brief.
    pub mode: ListMode,
    /// Whether hidden and system entries are listed.
    pub show_hidden: bool,
    /// The quick filter in force; `entry_count` counts only what it shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// The flat view the panel shows, if any: what it lists and whether it is still reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flat: Option<FlatView>,
    /// For a panel showing its terminal, in `panel.get_state`: the shell's folder and prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<PanelTerminal>,
    /// The listing is still arriving: `entry_count` counts what came so far (RC-060-02).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub loading: bool,
}

/// What a command answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    /// A directory's entries, in panel order.
    Listing { path: VfsPath, entries: Vec<Entry> },
    /// A panel after the command.
    Panel(PanelSnapshot),
    /// A terminal session after the command.
    Terminal(TerminalInfo),
    /// The open terminal sessions, by id, and the shells a new one can run (the default first).
    Terminals {
        terminals: Vec<TerminalInfo>,
        shells: Vec<ShellProfile>,
    },
    /// What a terminal shows.
    Screen {
        terminal: TermId,
        state: SessionState,
        screen: ScreenText,
    },
    /// The installed WSL distributions, the default marked.
    WslDistros { distros: Vec<WslDistro> },
    /// The vault's state after the command; never key material.
    Vault(VaultStatus),
    /// The whole connection tree.
    ConnectionTree { tree: ConnectionTree },
    /// A connection after the command.
    Connection { connection: Connection },
    /// A connection folder after the command.
    Folder { folder: ConnectionFolder },
    /// A shared credential after the command (RC-050-12).
    Credential { credential: crate::Credential },
    /// What a delete removed: the node itself, and for a folder everything inside it.
    Deleted {
        item: TreeItem,
        folders: u64,
        connections: u64,
    },
    /// Stored SSH host keys.
    KnownHosts { keys: Vec<KnownHost> },
    /// The stored host key a command removed.
    KnownHostRemoved { key: KnownHost },
    /// What an import of a `known_hosts` file stored and skipped.
    KnownHostsImported(ImportReport),
    /// A host-key question answered; `stored` is the key saved by "trust and save".
    HostKeyAnswered {
        question: u64,
        answer: HostKeyAnswer,
        stored: Option<KnownHost>,
    },
    /// The key that replaced a changed one.
    HostKeyReplaced { key: KnownHost },
    /// Stored error-log lines, newest first.
    ErrorLog { entries: Vec<ErrorLogEntry> },
    /// What clearing the error log removed.
    ErrorLogCleared { removed: u64 },
    /// The limits of every retained table.
    Retention { tables: Vec<RetentionStatus> },
    /// A checked backup of the database: its file and size.
    DatabaseBackup(BackupReport),
    /// A checked backup staged to replace the database at the next start, and the backup of
    /// the state it replaces.
    RestoreStaged(RestoreReport),
    /// A job of the operations queue, as accepted or after the command.
    Job(Box<JobSnapshot>),
    /// The jobs of the operations queue.
    Jobs { jobs: Vec<JobSnapshot> },
    /// The operations queue in the queue window's order, each job with its lane and place.
    Queue { entries: Vec<rc_ops::QueueEntry> },
    /// The queue's lists and the saved transfer lists (RC-010-15).
    QueueLists {
        lists: Vec<rc_ops::QueueList>,
        saved: Vec<crate::SavedList>,
    },
    /// A list saved, and the jobs left out of it (no copy or move between plain locations).
    QueueSaved {
        saved: crate::SavedList,
        left_out: Vec<rc_ops::JobId>,
    },
    /// A saved list loaded into a new, stopped list: its jobs, and the entries not loaded.
    QueueLoaded {
        list: rc_ops::QueueList,
        jobs: Vec<JobSnapshot>,
        unloadable: Vec<crate::Unloadable>,
    },
    /// The keys the SSH agents hold: type, comment, fingerprint, never the key itself.
    AgentIdentities {
        /// The agents that answered, in the order asked.
        sources: Vec<AgentSourceKind>,
        identities: Vec<AgentIdentity>,
    },
    /// What an import of an ssh_config planned (`dry_run`) or created and updated.
    SshConfigImported(SshConfigReport),
    /// What an import from Total Commander planned (`dry_run`) or did.
    TcImported(crate::import::total_commander_report::TcImportReport),
    /// Which Total Commander passwords went into the vault - keys only.
    TcPasswordsImported(crate::import::total_commander_report::TcPasswordsReport),
    /// What an import from Remote Desktop Manager planned (`dry_run`) or did.
    RdmImported(crate::import::rdm_report::RdmImportReport),
    /// What an export of settings and connections wrote (RC-080-19).
    ConfigExported(crate::exchange::ExportReport),
    /// What an import of such an export planned (`dry_run`) or did.
    ConfigImported(crate::exchange::ExchangeReport),
    /// One page of a search: its hits in walk order, the cursor of the next page (`None` on
    /// the last one) and how far the walk got.
    SearchResults {
        hits: Vec<SearchHit>,
        next_cursor: Option<String>,
        stats: SearchStats,
    },
    /// Two files compared by content.
    FileComparison(FileComparison),
    /// Two directories compared, with the proposed synchronization.
    DirComparison(DirComparison),
    /// The digests of files (`checksum.compute`), in the order asked.
    Checksums {
        algorithm: Algorithm,
        files: Vec<FileDigest>,
    },
    /// The checksum files written in `dir` and the digests they hold.
    ChecksumsCreated { dir: VfsPath, report: CreateReport },
    /// A checksum file checked: one result per line and the counts.
    ChecksumsVerified { path: VfsPath, report: VerifyReport },
    /// A file split into parts in `dir`.
    Split { dir: VfsPath, report: SplitReport },
    /// Parts combined into a new file in `dir`.
    Combined { dir: VfsPath, report: CombineReport },
    /// A window of a file as text.
    FileText(TextWindow),
    /// A window of a file as hex rows.
    FileHex(HexWindow),
    /// The drive bar's entries: drives, WSL distributions, servers, in this order.
    Drives { drives: Vec<Drive> },
    /// Settings with their values: every one, or the one read or changed.
    Settings { settings: Vec<SettingValue> },
    /// The keys in force (RC-100-21), and the stored deviations a newer standard no longer fits.
    Keymap {
        rows: Vec<crate::KeymapRow>,
        stale: Vec<String>,
    },
    /// A multi-rename's preview in `dir`: each entry's new name, kind of change and conflict.
    RenamePreview { dir: VfsPath, plan: RenamePlan },
    /// A multi-rename or its undo, queued: the run of the undo log and its job.
    RenameRun {
        run: String,
        dir: VfsPath,
        job: Box<JobSnapshot>,
    },
    /// The multi-rename presets by name, and the rule of the last run.
    RenamePresets {
        presets: Vec<crate::RenamePreset>,
        last: Option<rc_rename::RenameRule>,
    },
    /// Every login script, by name.
    LoginScripts { scripts: Vec<LoginScriptInfo> },
    /// A login script after the command.
    LoginScript { script: LoginScriptInfo },
    /// What deleting a login script removed with it.
    LoginScriptDeleted {
        id: LoginScriptId,
        deletion: LoginScriptDeletion,
    },
    /// A connection's SSH session after `connections.connect`.
    Session(SessionStatus),
    /// `connections.disconnect`: whether a session or a setup was there to close.
    SessionClosed {
        connection_id: ConnectionId,
        was_open: bool,
    },
    /// What `connections.test` found: the method that logged in, the host key, the time.
    SessionTested(SessionTestReport),
    /// A connection's SSH session settings.
    SessionPrefs {
        connection_id: ConnectionId,
        prefs: SessionPrefs,
    },
    /// What `ssh.exec` left: the streams, the exit code or signal, whether it ran out of time.
    SshExec(ExecOutput),
    /// An FTP connection that connected and logged in (`connections.test`).
    ConnectionTested {
        connection: ConnectionId,
        /// Its kind, and with it the encryption the test used.
        protocol: ConnectionKind,
    },
    /// One entry (`files.stat`).
    Stat { path: VfsPath, entry: Entry },
    /// An MCP question answered (`mcp.answer`).
    McpAnswered {
        question: u64,
        answer: crate::McpAnswer,
    },
    /// The MCP calls allowed for a session taken back (`mcp.revoke`), and how many.
    McpRevoked { removed: usize },
    /// A panel's tabs, left to right.
    Tabs {
        panel: PanelId,
        tabs: Vec<crate::TabView>,
    },
    /// The active tab's folder history, oldest first, and the index of the folder shown.
    History {
        panel: PanelId,
        entries: Vec<VfsPath>,
        current: usize,
    },
    /// The directory hotlist in menu order: each hotlist folder before what it holds.
    Hotlist { items: Vec<crate::HotlistEntry> },
    /// A hotlist entry or folder after the command.
    HotlistItem { item: crate::HotlistEntry },
    /// What a hotlist delete removed, the row itself counted.
    HotlistDeleted { id: i64, removed: u64 },
    /// A directory inside an archive (`shown` as a path through the archive file): its entries
    /// in panel order, each with what the archive says about it.
    ArchiveListing {
        path: VfsPath,
        shown: std::path::PathBuf,
        entries: Vec<ArchiveItem>,
    },
    /// What the properties dialog shows (`file.properties`).
    Properties(PropertiesReport),
    /// What a change of attributes and times did.
    AttributesSet(rc_ops::AttributeReport),
    /// A link made: where it is, where it leads, what kind (`link_kind`: `kind` is the tag).
    LinkCreated {
        link: VfsPath,
        target: VfsPath,
        link_kind: rc_vfs::LinkKind,
    },
    /// Occupied space: each start and the sum.
    DirSizes {
        items: Vec<PathUsage>,
        total: rc_ops::DirUsage,
    },
    /// A comment after the change; `None` once removed.
    CommentSet {
        path: VfsPath,
        comment: Option<String>,
    },
    /// Text for the clipboard, one line per entry.
    ClipboardText { text: String },
    /// The trusted FTPS certificates (`certificates.list`).
    TrustedCertificates {
        certificates: Vec<rc_db::TrustedCertificate>,
    },
    /// The trusted certificate a command removed.
    TrustedCertificateRemoved {
        certificate: rc_db::TrustedCertificate,
    },
    /// A certificate question answered; `stored` is the certificate saved by "trust and save".
    CertificateAnswered {
        question: u64,
        answer: crate::CertificateAnswer,
        stored: Option<rc_db::TrustedCertificate>,
    },
    /// Which build runs (`app.about`).
    About(crate::about::About),
    /// The plugins, a package's preview or a plugin after a change (`plugins.*`, RC-100-09).
    Plugins(Box<crate::PluginsOutcome>),
    /// The secret sources, a search in one, or a connection's references (RC-100-34).
    SecretSources(Box<crate::SecretSourcesOutcome>),
    /// The self-update's state (`update.*`, RC-100-05).
    Update(Box<crate::UpdateStatus>),
    /// The colour themes after a `theme` command (RC-100-50).
    Themes(Box<crate::settings::themes::ThemesOutcome>),
}

impl Outcome {
    /// The outcome naming one job.
    pub(crate) fn job(snapshot: JobSnapshot) -> Self {
        Self::Job(Box::new(snapshot))
    }
}

/// One file's digest, or why it has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDigest {
    pub path: VfsPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
    /// The error code when the file could not be hashed (`not_found`, `is_a_directory`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}
