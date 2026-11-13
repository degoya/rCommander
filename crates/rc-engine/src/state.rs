//! The panels' state and what a command answers.

use std::{collections::BTreeMap, sync::Arc};

use rc_checksum::{Algorithm, CombineReport, CreateReport, SplitReport, VerifyReport};
use rc_db::{KnownHost, RevokedHostKey};
use rc_ops::JobSnapshot;
use rc_rename::RenamePlan;
use rc_term::{ScreenText, SessionState, ShellProfile, TermId};
use rc_vfs::{ArchiveItem, Entry, Vfs, VfsPath, WslDistro};
use serde::{Deserialize, Serialize};

use crate::{
    AgentIdentity, AgentSourceKind, ArchiveInfo, BackupReport, Connection, ConnectionFolder,
    ConnectionId, ConnectionKind, ConnectionTree, Drive, EngineError, ErrorLogEntry, ExecOutput,
    HostKeyAnswer, ImportReport, LoginScriptDeletion, LoginScriptId, LoginScriptInfo, QuickFilter,
    RestoreReport, RetentionStatus, SearchHit, SearchStats, SessionPrefs, SessionStatus,
    SessionTestReport, SettingValue, SortOrder, SshConfigReport, TerminalInfo, TreeItem,
    VaultStatus,
    compare::FileComparison,
    flat::FlatView,
    properties::{DirSize, PathUsage, PropertiesReport},
    sort::{names_for, sort_entries},
    sync::DirComparison,
    viewer::{HexWindow, TextWindow},
};

mod file_digest;
mod panel_id;
mod selection;
mod snapshot;

pub use file_digest::FileDigest;
pub use panel_id::{CursorView, ListMode, PanelId, PanelView};
pub use selection::Selection;
pub(crate) use selection::{by_name, unlisted};
pub use snapshot::PanelSnapshot;

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
    /// Full, brief, a column set or the comments; kept when the panel moves to another
    /// directory, unless a column set's folder mask switches it (RC-100-90).
    pub mode: ListMode,
    /// How the mode was chosen and the folder's comments while the mode shows them (RC-100-90).
    pub columns: crate::columns::PanelColumns,
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
    /// empty again with every new listing. Shared like `entries`: a copy of the state costs no
    /// copy of the map, and two states with the same map are told by the pointer (RC-110-29).
    /// Written through `Arc::make_mut`, so a state handed out keeps what it had.
    pub dir_sizes: Arc<BTreeMap<String, DirSize>>,
    /// The run of a listing still arriving (RC-060-02): `entries` are what came so far, and more
    /// follow (`commands/panel_listing.rs`); `None` once the listing is complete.
    pub listing: Option<u64>,
    /// The folder's git marks and branch once git answered (RC-100-97); kept while the panel
    /// reads the same folder again, until the new answer.
    pub git: Option<Arc<crate::git::GitView>>,
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
            columns: crate::columns::PanelColumns::default(),
            show_hidden: false,
            remembered: Selection::default(),
            filter: QuickFilter::default(),
            flat: None,
            archive: None,
            dir_sizes: Arc::default(),
            listing: None,
            git: None,
        }
    }

    /// The state after this one when the panel shows `path` with `entries`, listed in `sort`
    /// order with `show_hidden`: view, sort, mode and the remembered selection stay, and the
    /// quick filter and the comments in the same directory. Entries listed in another order than
    /// the panel has by now are sorted again.
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
        let same_folder = path == self.path;
        let mut next = Self {
            view: self.view,
            sort: self.sort,
            mode: self.mode,
            columns: self.columns.next(same_folder),
            show_hidden,
            remembered: self.remembered.clone(),
            ..Self::showing(path, entries)
        };
        if same_folder {
            next.git.clone_from(&self.git);
        }
        next.carry_filter(self);
        next.order_by_comments();
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
}

/// The entries of `path` in `sort` order; hidden and system entries only with `show_hidden`, never
/// what the ignore list hides.
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
    // The ignore list hides whatever the hidden-files switch says (RC-100-92).
    crate::ignore_list::retain_shown(path, &mut entries);
    sort_entries(&mut entries, sort, names_for(path));
    Ok(entries)
}

/// Whether a panel hides `entry` unless hidden files are shown: a dot file, or on Windows the
/// hidden or the system attribute, which Total Commander's switch covers together.
pub(crate) fn is_hidden(entry: &Entry) -> bool {
    entry.hidden || entry.attributes.windows.is_some_and(|flags| flags.system)
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
    /// The host keys a `known_hosts` file revoked (RC-080-67).
    RevokedHostKeys { keys: Vec<RevokedHostKey> },
    /// The revoked host key a person removed.
    RevokedHostKeyRemoved { key: RevokedHostKey },
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
    /// A checked backup staged to replace the database at the next start; the old one backed up.
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
    /// A search page: hits in walk order, the next page's cursor (`None` at the end), the walk.
    SearchResults {
        hits: Vec<SearchHit>,
        next_cursor: Option<String>,
        stats: SearchStats,
    },
    /// A page of duplicate groups (RC-100-98), in the order of the first criterion; the next
    /// page's cursor (`None` at the end) and the walk.
    DuplicateGroups {
        groups: Vec<crate::search::DuplicateGroup>,
        next_cursor: Option<String>,
        stats: SearchStats,
    },
    /// What a folder holds, by size (RC-100-98): its subfolders to a depth and its largest files.
    DiskUsage(Box<crate::disk_usage::DiskUsageReport>),
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
    /// The drive bar's entries: drives, devices, WSL distributions, servers, in this order.
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
    /// The names of a multi-rename's entries after the latest run: the next step's sources.
    RenameNextStep { names: Vec<String> },
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
    /// The active tab's folder history, oldest first, and the index of the folder shown; and
    /// the most visited folders, most first (RC-100-92), which `history.open_frequent` opens.
    History {
        panel: PanelId,
        entries: Vec<VfsPath>,
        current: usize,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        frequent: Vec<crate::tabs::FrequentFolder>,
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
    /// What a `workspaces.*` command answers (RC-100-68).
    Workspaces(Box<crate::WorkspacesOutcome>),
    /// The inventory sources, a sync or its preview, a source saved (`inventory.*`, RC-100-39).
    Inventory(Box<crate::inventory::InventoryOutcome>),
    /// A connection's port forwards with their state, or every running one (RC-110-03).
    Forwards(Box<crate::ForwardsOutcome>),
    /// What a `column_sets.*` command answers (RC-100-90).
    ColumnSets(Box<crate::ColumnSetsOutcome>),
    /// What a `usercmd.*` or `start_menu.*` command answers (RC-100-86).
    UserCommands(Box<crate::UserCommandsOutcome>),
    /// The git marks and branch a panel knows (`panel.git_status`, RC-100-97); `None` without
    /// an answer of git (switched off, no repository, not yet answered).
    GitStatus {
        panel: PanelId,
        git: Option<Box<crate::git::GitView>>,
    },
    /// The server files being edited through copies, oldest first (`remote_edit.*`, RC-110-01).
    RemoteEdits { edits: Vec<crate::RemoteEditInfo> },
    /// The saved file filters by name (`filters.*`, RC-100-92).
    FileFilters {
        saved: Vec<crate::commands::file_filters::SavedFilter>,
    },
    /// A selection read from a file or a text (RC-100-92): the panel after it, how many names
    /// it selected and how many it named that the panel does not list.
    SelectionLoaded {
        panel: PanelSnapshot,
        selected: usize,
        missing: usize,
    },
    /// The selection written to a file (RC-100-92): where, and how many names.
    SelectionWritten { path: VfsPath, names: usize },
    /// What a `button_bar.*` command answers (RC-100-87).
    ButtonBars(Box<crate::ButtonBarsOutcome>),
}
