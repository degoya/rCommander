//! What the engine announces after a change.

use serde::{Deserialize, Serialize};

use rc_ops::{JobChange, JobId};

use crate::{
    ConnectionId, FileDialog, HostKeyQuestion, LoginScriptId, PanelId, SecretRef, SessionChange,
    TermId, TreeChange, TreeItem, VfsPath,
};

/// One change, sent to every [`crate::Engine::subscribe`] receiver after the state changed. An
/// event names what changed; the receiver reads the new state from the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    /// A panel changed.
    Panel { panel: PanelId, change: PanelChange },
    /// Another panel became the active one.
    ActivePanel { panel: PanelId },
    /// A terminal session changed; its screen changes are on the session's own stream
    /// (`TermSession::subscribe`), not here.
    Terminal {
        terminal: TermId,
        change: TerminalChange,
    },
    /// The WSL distributions were read again (`wsl.refresh`); the list may have changed.
    WslDistros,
    /// The vault was locked: by `vault.lock` or by the idle lock. Its key is gone from memory.
    VaultLocked,
    /// A connection or folder of the connection tree changed; the connection manager and the
    /// drive bar read the tree again.
    ConnectionTree { item: TreeItem, change: TreeChange },
    /// An SSH host key waits for a person: the TOFU dialog for an unknown key, the hard stop
    /// for a changed one. Answered with `hostkeys.answer` or `hostkeys.replace`.
    /// Boxed: the question is far larger than every other event.
    HostKeyQuestion { question: Box<HostKeyQuestion> },
    /// A host-key question was answered or replaced; a dialog showing it closes.
    HostKeyQuestionClosed { question: u64 },
    /// The stored host keys changed (trusted, replaced, removed, imported).
    KnownHostsChanged,
    /// A job of the operations queue changed: progress, a question, its end.
    Job { job: JobId, change: JobChange },
    /// The vault was created or unlocked (`vault.create`, `vault.unlock`): stored secrets can be
    /// read; a connection that waited for the master password is tried again.
    VaultUnlocked,
    /// The vault was removed (`vault.remove`): no secret is stored and no master password is
    /// asked until one is stored again.
    VaultRemoved,
    /// The last stored secret is gone while a vault exists: the GUI offers to remove the vault
    /// (`vault.remove`).
    VaultUnused,
    /// A connection needs a stored secret and the vault is locked: the GUI asks for the master
    /// password (`vault.unlock`) and tries the connection again; without an answer only that
    /// connection fails, with `vault_locked`. Never sent while nothing is stored.
    SecretNeeded { secret: SecretRef },
    /// The drives were read again, or a drive answered late; the drive bar reads them again
    /// (`drives.list`).
    Drives,
    /// The drive menu of a panel is wanted (`panel.drive_menu`, Alt+F1 and Alt+F2); the window
    /// opens it.
    DriveMenu { panel: PanelId },
    /// A setting was stored (`settings.set` or the settings dialog); the window applies it.
    SettingChanged { key: String },
    /// A login script was created, changed (its steps, a secret, a connection that runs it) or
    /// deleted; the editor reads it again.
    LoginScript {
        script: LoginScriptId,
        change: TreeChange,
    },
    /// A connection's SSH session came up, was closed, or was lost - or an FTP connection's
    /// kept session was closed (`connections.disconnect`); the connection manager reads its
    /// state again (`Engine::ssh_session`).
    Session {
        connection: ConnectionId,
        change: SessionChange,
    },
    /// An MCP call waits for the person's answer (RC-010-13): the window shows the question and
    /// comes to the front. Answered with `mcp.answer`. Boxed like the host-key question.
    McpQuestion { question: Box<crate::McpQuestion> },
    /// An MCP question was answered, timed out or given up by its call; its dialog closes.
    McpQuestionClosed { question: u64 },
    /// The MCP calls allowed for a session changed: one granted, run out or revoked
    /// (RC-010-27); the status bar reads them again (`Engine::mcp_grants`).
    McpGrantsChanged,
    /// The directory hotlist changed; an open hotlist reads it again (`hotlist.list`).
    HotlistChanged,
    /// A dialog of the file operations is wanted (`ops.dialog`, F5 to F8); the window opens it.
    FileDialog { panel: PanelId, dialog: FileDialog },
    /// The find-files dialog is wanted for a panel (`files.find_dialog`, Alt+F7); the window
    /// opens it.
    FindFiles { panel: PanelId },
    /// A file is to be shown in the window's viewer (`view.open`, F3); `panel` is the one it
    /// was opened from, whose next and previous files the viewer can step to.
    ViewFile { panel: PanelId, path: VfsPath },
    /// The Quick View of a panel is to be switched (`panel.toggle_quick_view`, Ctrl+Q): the
    /// other panel shows the file under this panel's cursor, or its own view again.
    QuickView { panel: PanelId },
    /// The terminal of a panel is to be laid over both panels, or the two panels shown again
    /// (`terminal.maximize`, Ctrl+A-umlaut, RC-040-05); the panel shows its terminal already.
    /// Which panel is maximized is the window's state.
    MaximizeTerminal { panel: PanelId },
    /// The clipboard's image is to be pasted into `terminal` as a file path
    /// (`terminal.paste_image`, Ctrl+Alt+V, RC-040-08): the window reads the clipboard and calls
    /// [`crate::Engine::place_terminal_image`].
    PasteImage { terminal: TermId },
    /// The connection manager is wanted (`connections.manager`, Ctrl+F); the window opens it.
    ConnectionManager,
    /// The context menu of a panel's selection is wanted (`panel.context_menu`, Shift+F10 and
    /// the context-menu key); the window opens it at the cursor's row.
    ContextMenu { panel: PanelId },
    /// An FTPS certificate waits for a person (RC-070-02): the first-contact dialog, the hard
    /// stop for a changed one, the warning for an expired one. Answered with
    /// `certificates.answer`. Boxed like the host-key question.
    CertificateQuestion {
        question: Box<crate::CertificateQuestion>,
    },
    /// A certificate question was answered; a dialog showing it closes.
    CertificateQuestionClosed { question: u64 },
    /// The trusted certificates changed (trusted, removed).
    TrustedCertificatesChanged,
    /// The installed plugins changed (RC-100-09): one was installed, switched on or off, rolled
    /// back, resumed or removed, its own settings changed, or the start installed bundled ones.
    /// A cache of loaded plugins drops what it holds; the `plugins.*` settings announce
    /// themselves as [`EngineEvent::SettingChanged`].
    PluginsChanged,
    /// A session to a plain-FTP server came up: its user, password and data cross the network
    /// in clear; the window says so (RC-080-12).
    PlainFtp {
        connection: ConnectionId,
        name: String,
    },
    /// A checksum dialog is wanted for a panel (`checksum.dialog`, RC-020-05); the window opens
    /// it for the panel's selection.
    ChecksumDialog {
        panel: PanelId,
        dialog: crate::ChecksumDialog,
    },
    /// The multi-rename dialog is wanted for a panel (`multi_rename.open_dialog`, Ctrl+M); the
    /// window opens it on the panel's selection.
    MultiRenameDialog { panel: PanelId },
    /// The diff window is wanted for two files (`compare.open_diff`); the window opens it and
    /// compares them.
    CompareFiles { left: VfsPath, right: VfsPath },
    /// The synchronize-dirs dialog is wanted (`sync.open_dialog`) for the two panels' folders;
    /// the window opens it.
    SyncDirs { left: VfsPath, right: VfsPath },
    /// The queue window is wanted (`queue.window`); the window opens it.
    QueueWindow,
    /// The app is to end (`app.quit`, F10, RC-080-17). `unfinished` is the number of jobs that
    /// run or wait, which the window asks about first; 0 when there are none or the person
    /// switched the question off (`app.confirm_quit_with_transfers`).
    Quit { unfinished: usize },
    /// The queue's order or list changed without a job event (`queue.move`, `queue.remove`,
    /// `queue.clear_finished`); an open queue window reads it again.
    QueueChanged,
    /// A listing shown before its end broke off (RC-060-02): the panel keeps what arrived, the
    /// window says why. `code` is the error's code, as a failed command answers it.
    ListingFailed { panel: PanelId, code: String },
    /// The self-update's state changed (RC-100-05): a check ended, a download started or ended,
    /// the update was handed to the updater, a notice was put away; `update.status` has it.
    Update,
    /// A download of an update came this far.
    UpdateProgress { received: u64, total: u64 },
}

/// What about a panel changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelChange {
    /// It shows another directory (entries, cursor and selection are new too).
    Navigated,
    /// The cursor moved.
    Cursor,
    /// The selection changed.
    Selection,
    /// It switched between its file list and its terminal.
    View,
    /// Its entries were sorted again, or it changed between the full and the brief view.
    Listing,
    /// Its tabs changed without it showing another folder: closed, locked, one added in the
    /// background.
    Tabs,
}

/// What about a terminal session changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalChange {
    /// It was opened.
    Opened,
    /// Its program set a new title.
    Title,
    /// It has a new size.
    Resized,
    /// Its program ended; the session stays listed until it is closed.
    Exited,
    /// It was closed and is gone.
    Closed,
    /// Its shell reported another folder (RC-040-04).
    Folder,
    /// It was asked to follow its panel's folder, or a waiting `cd` was typed at its prompt: the
    /// follow status may have changed (`TermSession::follow_status`).
    Follow,
    /// Its login script began a step or ended (RC-060-03); `terminal.list` has the run.
    LoginScript,
}
