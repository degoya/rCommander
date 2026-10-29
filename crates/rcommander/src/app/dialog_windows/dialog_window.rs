//! The dialogs that open in windows of their own (RC-080-15, RC-080-23): each kind with whether
//! it locks the main window, whether the main window owns it, where its place is kept and its
//! sizes. Their books are `book.rs`.

use iced::Size;

use super::questions::Question;

/// A dialog that opens in a window of its own (RC-080-15): the settings, the queue when
/// `queue.window_own` is on (RC-010-14), and while `appearance.dialog_windows` is on the large
/// dialogs - the connection manager, find files, multi-rename, compare/synchronize (the diff
/// window over the synchronize dialog, in one window), the checksum results, the login-script
/// editor and each Lister, by the number of its viewer - and the standalone dialogs as Total
/// Commander has them (RC-080-23): the database dialog, the properties, the quick connection,
/// the network drive dialogs, packing and unpacking, the checksum forms, the hotlist dialog and
/// "Run command" -, and the file operations' dialogs (stage 2): copy and move (F5, F6, Shift+F5),
/// a new folder or text file (F7, Shift+F4), delete (F8, Shift+F8), a job's conflict and error
/// questions and its progress window - and the small questions, one at a time (`questions.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogWindow {
    Settings,
    Queue,
    Connections,
    Search,
    MultiRename,
    Compare,
    Checksum,
    LoginScripts,
    Lister(u64),
    Database,
    Properties,
    QuickConnect,
    Network,
    Archive,
    ChecksumForm,
    Hotlist,
    SshExec,
    Transfer,
    NewItem,
    Delete,
    Conflict,
    JobError,
    Progress,
    Question(Question),
}

impl DialogWindow {
    /// Every kept place once: `Lister(0)` stands for every Lister, which share one.
    pub const ALL: [Self; 32] = [
        Self::Settings,
        Self::Queue,
        Self::Connections,
        Self::Search,
        Self::MultiRename,
        Self::Compare,
        Self::Checksum,
        Self::LoginScripts,
        Self::Lister(0),
        Self::Database,
        Self::Properties,
        Self::QuickConnect,
        Self::Network,
        Self::Archive,
        Self::ChecksumForm,
        Self::Hotlist,
        Self::SshExec,
        Self::Transfer,
        Self::NewItem,
        Self::Delete,
        Self::Conflict,
        Self::JobError,
        Self::Progress,
        Self::Question(Question::Mcp),
        Self::Question(Question::HostKey),
        Self::Question(Question::Certificate),
        Self::Question(Question::VaultOffer),
        Self::Question(Question::Quit),
        Self::Question(Question::Unlock),
        Self::Question(Question::ArchivePassword),
        Self::Question(Question::LargeFetch),
        Self::Question(Question::SyncConfirm),
    ];

    /// Whether the main window takes no input while it is open: the settings, as Total
    /// Commander's configuration, multi-rename, whose run renames the panel's files under it
    /// (owner, 2026-10-18), and the standalone dialogs, modal in Total Commander too; a restore
    /// or an import rewrites the database the panels read (RC-080-23), and the file operations'
    /// dialogs, questions and progress window, whose main window waits in Total Commander too.
    /// Not the queue, the
    /// connection manager, find files, compare, the checksum results, the login-script editor
    /// and Lister, beside which the person works on.
    pub fn modal(self) -> bool {
        match self {
            Self::Settings
            | Self::MultiRename
            | Self::Database
            | Self::Properties
            | Self::QuickConnect
            | Self::Network
            | Self::Archive
            | Self::ChecksumForm
            | Self::Hotlist
            | Self::SshExec
            | Self::Transfer
            | Self::NewItem
            | Self::Delete
            | Self::Conflict
            | Self::JobError
            | Self::Progress => true,
            Self::Queue
            | Self::Connections
            | Self::Search
            | Self::Compare
            | Self::Checksum
            | Self::LoginScripts
            | Self::Lister(_) => false,
            Self::Question(question) => question.modal(),
        }
    }

    /// Whether it is a small question's window (`questions.rs`), not a dialog's.
    pub fn is_question(self) -> bool {
        matches!(self, Self::Question(_))
    }

    /// Whether the main window owns it - it stays in front of the main window and minimizes with
    /// it: every dialog's window but Lister's, which is a window of its own as in Total
    /// Commander and can lie behind the main window.
    pub fn owned(self) -> bool {
        !matches!(self, Self::Lister(_))
    }

    /// Whether the small questions (an MCP call, a host key, a certificate, the vault, quitting)
    /// draw in it while it has the keyboard: every dialog's window but the queue's, whose keys
    /// are its own before any question's (`queue_window/own.rs`).
    pub fn takes_questions(self) -> bool {
        self != Self::Queue
    }

    /// Whether it is a file operation's: its keys come before the other dialogs' while none of
    /// those has the keyboard (`dialog_windows/file_ops.rs`).
    pub fn file_ops(self) -> bool {
        matches!(
            self,
            Self::Transfer
                | Self::NewItem
                | Self::Delete
                | Self::Conflict
                | Self::JobError
                | Self::Progress
        )
    }

    /// Whether it is a job's question, the window on top of the file operations' others.
    pub fn asks(self) -> bool {
        matches!(self, Self::Conflict | Self::JobError)
    }

    /// Whether its window is hidden when it closes and shown again at the next opening, so a
    /// dialog asked for again and again (F7, F8, a conflict per file) opens without a window
    /// to make (RC-080-23). Not the progress window: the time its window takes to be made keeps
    /// a job of a moment from flashing one.
    pub fn reused(self) -> bool {
        self.file_ops() && self != Self::Progress
    }

    /// The setting its position and size are kept under, as JSON like `window.geometry`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Settings => "window.dialog.settings",
            Self::Queue => "window.dialog.queue",
            Self::Connections => "window.dialog.connections",
            Self::Search => "window.dialog.search",
            Self::MultiRename => "window.dialog.multi_rename",
            Self::Compare => "window.dialog.compare",
            Self::Checksum => "window.dialog.checksum",
            Self::LoginScripts => "window.dialog.login_scripts",
            Self::Lister(_) => "window.dialog.lister",
            Self::Database => "window.dialog.database",
            Self::Properties => "window.dialog.properties",
            Self::QuickConnect => "window.dialog.quick_connect",
            Self::Network => "window.dialog.network_drive",
            Self::Archive => "window.dialog.archive",
            Self::ChecksumForm => "window.dialog.checksum_form",
            Self::Hotlist => "window.dialog.hotlist",
            Self::SshExec => "window.dialog.ssh_exec",
            Self::Transfer => "window.dialog.transfer",
            Self::NewItem => "window.dialog.new_item",
            Self::Delete => "window.dialog.delete",
            Self::Conflict => "window.dialog.conflict",
            Self::JobError => "window.dialog.job_error",
            Self::Progress => "window.dialog.progress",
            Self::Question(question) => question.key(),
        }
    }

    /// Its size at its first opening.
    pub fn default_size(self) -> Size {
        match self {
            Self::Settings => Size::new(900.0, 640.0),
            Self::Queue => Size::new(960.0, 520.0),
            Self::Connections => Size::new(1000.0, 680.0),
            Self::Search => Size::new(860.0, 640.0),
            Self::MultiRename => Size::new(980.0, 720.0),
            Self::Compare => Size::new(1100.0, 720.0),
            Self::Checksum => Size::new(760.0, 560.0),
            Self::LoginScripts => Size::new(1000.0, 700.0),
            Self::Lister(_) => Size::new(900.0, 640.0),
            Self::Database => Size::new(760.0, 560.0),
            Self::Properties => Size::new(720.0, 600.0),
            Self::QuickConnect => Size::new(720.0, 540.0),
            Self::Network => Size::new(520.0, 320.0),
            Self::Archive => Size::new(720.0, 560.0),
            Self::ChecksumForm => Size::new(720.0, 460.0),
            Self::Hotlist => Size::new(760.0, 520.0),
            Self::SshExec => Size::new(760.0, 640.0),
            Self::Transfer => Size::new(720.0, 400.0),
            Self::NewItem => Size::new(520.0, 230.0),
            Self::Delete => Size::new(720.0, 240.0),
            Self::Conflict => Size::new(720.0, 460.0),
            Self::JobError => Size::new(720.0, 260.0),
            Self::Progress => Size::new(720.0, 280.0),
            Self::Question(question) => question.default_size(),
        }
    }

    /// The smallest it can be made: its pages and its buttons still fit.
    pub fn min_size(self) -> Size {
        match self {
            Self::Settings => Size::new(640.0, 440.0),
            Self::Queue => Size::new(640.0, 320.0),
            Self::Connections => Size::new(720.0, 480.0),
            Self::Search => Size::new(620.0, 460.0),
            Self::MultiRename => Size::new(760.0, 560.0),
            Self::Compare => Size::new(720.0, 440.0),
            Self::Checksum => Size::new(520.0, 360.0),
            Self::LoginScripts => Size::new(760.0, 520.0),
            Self::Lister(_) => Size::new(420.0, 300.0),
            Self::Database => Size::new(560.0, 380.0),
            Self::Properties => Size::new(520.0, 380.0),
            Self::QuickConnect => Size::new(520.0, 440.0),
            Self::Network => Size::new(420.0, 260.0),
            Self::Archive => Size::new(520.0, 400.0),
            Self::ChecksumForm => Size::new(520.0, 320.0),
            Self::Hotlist => Size::new(600.0, 380.0),
            Self::SshExec => Size::new(560.0, 440.0),
            Self::Transfer => Size::new(520.0, 360.0),
            Self::NewItem => Size::new(420.0, 200.0),
            Self::Delete => Size::new(480.0, 200.0),
            Self::Conflict => Size::new(600.0, 420.0),
            Self::JobError => Size::new(520.0, 220.0),
            Self::Progress => Size::new(520.0, 250.0),
            Self::Question(question) => question.min_size(),
        }
    }
}
