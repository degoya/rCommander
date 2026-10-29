//! The window's messages, the effects a message leaves, and the jobs of commands queued for the
//! engine (`app.rs` applies them, `effects.rs` runs them).

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use iced::{Size, Task, mouse::ScrollDelta, theme::Mode as SystemMode, widget::text, window};
use rc_engine::{Chord, Command, EngineEvent, PanelId, SortKey};
use tokio::sync::mpsc;

use super::{
    archive_ops, browse, cert_trust, checksum, cmdline, compare, connections, context_menu,
    database_dialog::{DatabaseJob, DatabaseMsg},
    dialog_windows, drag, drive_bar, file_ops, hostkeys, icons, login_scripts, mcp_confirm,
    multi_rename, palette,
    properties::{PropsJob, PropsMsg},
    queue_window, quick_connect, quit, resize, search, settings, ssh_exec, tabs,
    terminal_pane::TermMessage,
    update_ui::UpdateMsg,
    vault_offer::VaultOfferMsg,
    viewer,
    window_state::WindowChange,
};
use crate::theme::{
    Tokens,
    layout::{self, Head},
};

#[derive(Debug, Clone)]
pub enum Message {
    /// A key press, spelled for the keymap.
    Key(Chord),
    /// A job of commands finished: the error code of its first failure, if any.
    Done(PanelId, Result<(), &'static str>),
    /// A locked vault refused a job's command: it and the commands after it, run again once
    /// the master password opened the vault.
    VaultRefused(PanelId, Vec<Command>),
    /// The engine changed - after a key, a click or an MCP call.
    Engine(EngineEvent),
    /// Engine events were missed; every panel is read again.
    EngineLagged,
    /// The list area of a panel was measured.
    Viewport(PanelId, Size),
    Scroll(PanelId, ScrollDelta),
    /// The scroll bar's thumb was dragged or its track clicked: the list shows from this row.
    ScrollTo(PanelId, usize),
    /// A row was clicked.
    Click(PanelId, usize),
    /// A row was double-clicked.
    Open(PanelId, usize),
    /// The pointer entered or left a panel.
    Hover(PanelId),
    Unhover(PanelId),
    /// A mouse button went down anywhere: the panel under the pointer becomes active.
    Press,
    SystemTheme(SystemMode),
    /// The terminals: a widget's redraw or paste, the session bar, the shell picker.
    Terminal(TermMessage),
    /// A window moved or changed its size; it counts once it is known not to be maximized.
    WindowChanged(window::Id, WindowChange),
    WindowSeen(window::Id, WindowChange, bool),
    /// A window's X or Alt+F4: the main window ends the app, a dialog's window closes the dialog.
    CloseRequested(window::Id),
    /// The window's geometry is stored, or could not be: the app ends.
    Saved,
    /// A key press a widget used already (a terminal, a prompt): only `always` bindings run.
    KeyCaptured(Chord),
    /// A click on a column header: sort by it, or turn the direction around.
    SortBy(PanelId, SortKey),
    /// A click on a segment of the path bar: show that directory.
    PathSegment(PanelId, PathBuf),
    /// A click beside the path's segments: type a path.
    EditPath(PanelId),
    /// The text of the open prompt changed.
    PromptInput(String),
    /// Enter in the open prompt.
    PromptSubmit,
    /// A click on the status line's hidden-files switch.
    ToggleHidden,
    /// A kept mask clicked beside the mask prompt's field.
    PromptRecall(String),
    /// The drive bar and the drive menu.
    Drive(drive_bar::DriveMessage),
    /// The settings dialog (RC-010-09).
    Settings(settings::SettingsMessage),
    /// The database dialog: back up, restore.
    Database(DatabaseMsg),
    /// The host-key dialogs (RC-050-07).
    HostKey(hostkeys::HostKeyMsg),
    /// The question of an MCP call: allow, allow for the session, deny (RC-010-13).
    McpConfirm(mcp_confirm::McpConfirmMsg),
    /// The file icons: their setting, loaded icons (RC-010-10).
    Icons(icons::IconsMsg),
    /// The tab rows, the history and hotlist popups, the hotlist dialog (RC-010-03).
    Tabs(tabs::TabsMessage),
    /// The file operations: their dialogs, jobs and function-key bar (RC-010-07).
    FileOps(file_ops::FileOpsMsg),
    /// The command line, the quick search and the quick filter (RC-010-08).
    CmdLine(cmdline::CmdLineMsg),
    /// The find-files dialog (RC-020-03).
    Search(search::SearchMsg),
    /// The viewer: Lister (F3) and Quick View (Ctrl+Q), RC-020-07.
    Viewer(viewer::ViewerMessage),
    /// The archive dialogs: pack, unpack, a password (RC-030-03, RC-030-04).
    Archive(archive_ops::ArchiveMsg),
    /// The checksum, split and combine dialogs and the result list (RC-020-05).
    Checksum(checksum::ChecksumMsg),
    /// The properties, space, link and comment dialogs (RC-020-06).
    Properties(PropsMsg),
    /// The connection manager (RC-050-04).
    Connections(connections::ConnMsg),
    /// The context menu: a right click, its entries, the Explorer's menu closing (RC-080-02).
    ContextMenu(context_menu::ContextMessage),
    /// The login-script editor (RC-050-08), and its Close button.
    LoginScripts(login_scripts::Message),
    CloseLoginScripts,
    /// The quick connection dialog (Ctrl+N, RC-060-02).
    QuickConnect(quick_connect::QuickMsg),
    /// The "Run command" dialog of an SSH connection (RC-060-04).
    SshExec(ssh_exec::ExecMsg),
    /// The FTPS certificate dialogs (RC-070-02).
    CertTrust(cert_trust::CertTrustMsg),
    /// The offer to remove an unused vault: remove or keep (RC-050-03).
    VaultOffer(VaultOfferMsg),
    /// The multi-rename dialog: its fields, buttons and the engine's answers (RC-020-02).
    MultiRename(multi_rename::MrMsg),
    /// Drag and drop, files dropped onto the window, files on the clipboard (RC-080-01).
    Drag(drag::DragMsg),
    /// The command palette (RC-080-05).
    Palette(palette::PaletteMsg),
    /// The diff window and the synchronize-dirs dialog (RC-020-04).
    Compare(compare::CompareMsg),
    /// The queue window (RC-010-14).
    Queue(queue_window::QueueMsg),
    /// "Browse..." beside a path field and the file dialog's answer (RC-010-18).
    Browse(browse::BrowseMsg),
    /// The column widths and the panel divider: their grips and drag (RC-010-20).
    Resize(resize::ResizeMsg),
    /// F10's mode and the question before quitting (RC-080-17).
    Quit(quit::QuitMsg),
    /// The setting `appearance.dialog_windows`, read: the large dialogs in windows of their own
    /// (RC-080-15).
    DialogWindows(bool),
    /// A dialog's window on its way to the screen: ready, then shown (RC-080-23).
    DialogWindow(dialog_windows::DialogWindowMsg),
    /// The setting `keys.underline_access_keys`, read: the access keys of the dialogs' buttons
    /// underlined always, not only while Alt is held (RC-080-16).
    AccessKeys(bool),
    /// The self-update: its notice, the Updates page, the background (RC-100-05).
    Update(UpdateMsg),
}

impl Message {
    /// What the close X in a dialog's head sends (RC-080-15): Escape, so the X does what Escape
    /// does in every dialog - the top dialog takes it, as it takes the key.
    pub fn close_dialog() -> Self {
        Self::Key(super::escape::escape())
    }

    /// The head of a dialog: `title` and the X that sends [`Self::close_dialog`].
    pub fn dialog_head<'a>(title: impl Into<iced::Element<'a, Self>>) -> Head<'a, Self> {
        Head::new(title, Self::close_dialog())
    }

    /// [`Self::dialog_head`] with `title` as most dialogs draw it: one line in the strong font
    /// ([`layout::dialog_title`]).
    pub fn titled_head<'a>(title: impl text::IntoFragment<'a>) -> Head<'a, Self> {
        Self::dialog_head(layout::dialog_title(title))
    }

    /// A small or medium dialog over the window: [`Self::titled_head`] over `content` on a card
    /// at most `width` wide ([`layout::dialog_card`]).
    pub fn overlay<'a>(
        title: impl text::IntoFragment<'a>,
        content: impl Into<iced::Element<'a, Self>>,
        width: f32,
        tokens: &'static Tokens,
    ) -> iced::Element<'a, Self> {
        layout::dialog_card(Self::titled_head(title), content, width, tokens)
    }
}

/// What a message leaves to do outside the app's state.
pub enum Effect {
    /// Commands for the engine, run in order; a failure lands in this panel's status line.
    Dispatch(PanelId, Vec<Command>),
    /// The panel's view is its terminal: start the shell if there is none, give it the keys.
    ShowTerminal(PanelId),
    /// The keyboard to this panel's terminal, if it shows one.
    FocusTerminal(PanelId),
    Task(Task<Message>),
    /// Tab (`back`: Shift+Tab) in a dialog: the keyboard to the next stop of its focus ring
    /// (`field_walk.rs`, RC-080-16); `then` turns the id of the stop that has it then into a
    /// message, for a dialog that follows the ring.
    WalkFields {
        back: bool,
        then: Option<fn(Option<iced::widget::Id>) -> Message>,
    },
    /// A backup or a restore for the database dialog; its outcome comes back to the dialog.
    Database(Box<DatabaseJob>),
    /// An answer of a host-key dialog; the engine's reply comes back to the dialog.
    HostKey(Box<hostkeys::HostKeyJob>),
    /// A job of the file operations; the job the engine accepted comes back to them.
    Ops(file_ops::Submit),
    /// A command of the properties dialogs; its outcome comes back to them.
    Properties(PropsJob),
    /// Work of the connection manager; its answer comes back to the manager.
    Connections(connections::ConnJob),
    /// An answer of a certificate dialog; the engine's reply comes back to the dialog.
    CertTrust(cert_trust::CertTrustJob),
    /// Work of the multi-rename dialog; its answer comes back to the dialog.
    MultiRename(multi_rename::Call),
    /// A command of the queue window's lists; its answer comes back to the window.
    QueueList(queue_window::ListCall),
    /// A command of the settings' Host keys page; its answer comes back to the page.
    TrustStore(settings::TrustStoreCall),
    /// A command of the settings' Plugins page; its answer comes back to the page.
    Plugins(settings::PluginsCall),
}

/// Commands queued for the engine on a panel's behalf.
#[derive(Debug)]
pub struct Job {
    pub panel: PanelId,
    pub commands: Vec<Command>,
}

/// The receiving end of the job queue, until the dispatch subscription takes it.
pub(super) type JobQueue = Arc<Mutex<Option<mpsc::UnboundedReceiver<Job>>>>;
