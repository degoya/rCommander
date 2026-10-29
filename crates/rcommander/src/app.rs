//! The application: two panels over the engine, the keyboard, the terminals, the window.
//!
//! The window keeps no panel state of its own. A key press becomes a chord, the engine's
//! [`Keymap`] names the command, and the command goes to [`Engine::dispatch`] through one queue,
//! in the order the keys were pressed (`subscription.rs`). What changed comes back as an
//! [`EngineEvent`] - after a key and after an MCP call alike - and the panel copies the engine's
//! state ([`Pane::show`]); so an agent's `navigate` moves the panel on screen.
//!
//! [`App::apply`] is the one place state changes. It returns [`Effect`]s rather than running them,
//! so the tests drive the app through a real engine without a window; [`App::update`] runs them
//! (`effects.rs`); the messages and effects themselves are `message.rs`; the keys of the dialogs
//! after the intercepts (`intercepts.rs`) are `dialog_keys.rs`. Drawing is `view.rs`, the
//! windows and their dialog layers `dialog_windows.rs`, the start `startup.rs`; the drive bar and
//! its menu are `drive_bar.rs`, the database dialog `database_dialog.rs`, the connection manager
//! `connections.rs` with the login-script editor `login_scripts.rs`, the host-key dialogs
//! `hostkeys.rs`, the FTPS certificate dialogs `cert_trust.rs`, the question of an MCP call
//! `mcp_confirm.rs`, the offer to remove an unused vault `vault_offer.rs`; the folder tabs, the
//! history list and the hotlist are `tabs.rs`; the file operations of the function keys are
//! `file_ops.rs`; the command line, the quick search and the quick filter are `cmdline.rs`; the
//! quick connection (Ctrl+N) is `quick_connect.rs`, the "Run command" dialog of an SSH connection
//! `ssh_exec.rs`; the checksum, split and combine dialogs are `checksum/`; the multi-rename dialog
//! (Ctrl+M) is `multi_rename.rs`; drag and drop and files on the clipboard are `drag.rs`;
//! "Browse..." beside the path fields is `browse.rs`.

mod archive_ops;
mod archive_view;
mod browse;
mod cert_trust;
mod checksum;
mod cmdline;
mod compare;
mod connections;
mod context_menu;
mod database_dialog;
mod dialog_keys;
mod dialog_windows;
mod drag;
mod drive_bar;
mod effects;
mod escape;
mod events;
mod field_walk;
mod file_list;
mod file_ops;
#[cfg(test)]
pub(crate) mod harness;
mod hostkeys;
mod icons;
mod intercepts;
mod keys;
mod list_window;
mod login_scripts;
mod mcp_confirm;
mod menu_letter;
mod message;
mod mru;
mod multi_rename;
mod palette;
mod panel;
mod path_bar;
mod properties;
mod queue_window;
mod quick_connect;
mod quit;
mod resize;
mod scroll;
mod search;
mod settings;
mod ssh_exec;
mod startup;
mod status_bar;
mod subscription;
mod tabs;
mod terminal_copy;
mod terminal_image;
mod terminal_maximize;
mod terminal_pane;
mod terminal_view;
mod trust;
mod update_ui;
mod vault_ask;
mod vault_offer;
mod view;
mod viewer;
mod window_state;

use std::sync::{Arc, Mutex};

use iced::{Task, Theme, window};
use rc_db::Db;
use rc_engine::{Command, Engine, EngineEvent, Keymap, PanelId};
use tokio::sync::mpsc;

pub use effects::{run_in_order, run_job};
pub use message::{Effect, Job, Message};
pub use startup::run;
pub(crate) use startup::screenshots;

use self::{
    database_dialog::DatabaseDialog, keys::Prompt, message::JobQueue, panel::Pane,
    properties::PropsDialog, terminal_pane::TermPanes, window_state::WindowState,
};
use crate::{
    i18n,
    mcp::{McpServer, McpStatus},
    theme,
};

pub struct App {
    engine: Engine,
    keymap: Keymap,
    jobs: mpsc::UnboundedSender<Job>,
    queue: JobQueue,
    /// The panels by [`PanelId::index`].
    pub panes: [Pane; 2],
    pub active: PanelId,
    pub dark: bool,
    /// Whether the light or dark look follows the system's.
    follow_system: bool,
    /// The terminal widgets and their dialogs; the sessions are the engine's.
    terms: TermPanes,
    hovered: Option<PanelId>,
    window: WindowState,
    /// Where the window's geometry is stored; none in the tests.
    db: Option<Db>,
    pub mcp: McpStatus,
    /// A start-up problem for the status bar.
    pub notice: Option<String>,
    /// The one-line prompt that has the keys, if one is open.
    pub prompt: Option<Prompt>,
    /// The masks typed for Num+ and Num-, newest first (`mru::MASKS`).
    mask_history: Vec<String>,
    /// The drives, the layout of the drive bars and the open drive menu.
    drive_bar: drive_bar::DriveBar,
    /// Whether the system's look is dark, whether followed or not.
    system_dark: bool,
    /// The settings dialog, while it is open.
    settings: Option<settings::SettingsDialog>,
    /// The tab each settings page shows, kept for the session (RC-100-20).
    settings_tabs: settings::Tabs,
    /// The MCP server, which the settings dialog restarts.
    mcp_server: McpServer,
    /// News for the status bar: a backup's progress and result, a restore done at the start.
    pub info: Option<String>,
    /// The database dialog, while it is open.
    pub database: Option<DatabaseDialog>,
    /// The open host-key questions and their dialog (RC-050-07).
    host_keys: hostkeys::HostKeyDialogs,
    /// The MCP calls waiting for the person, oldest first; the first is shown (RC-010-13).
    mcp_questions: Vec<rc_engine::McpQuestion>,
    /// The MCP calls a person allowed for a session, for the status bar (RC-010-27).
    mcp_grants: Vec<rc_engine::McpGrant>,
    /// The file icons of the lists.
    icons: icons::Icons,
    /// The tab rows, the open popup and the hotlist dialog.
    pub tabs: tabs::TabsUi,
    /// The file operations' dialogs and jobs.
    pub file_ops: file_ops::FileOps,
    /// The command line under the panels and the quick search (RC-010-08).
    cmdline: cmdline::CmdLine,
    /// The find-files dialog, open or closed with its last search kept.
    pub search: Option<search::SearchDialog>,
    /// The open Listers and the Quick View.
    viewers: viewer::Viewers,
    /// The open archive dialog: pack, unpack or a password.
    pub archive: Option<archive_ops::ArchiveDialog>,
    /// A properties dialog, while one is open.
    pub properties: Option<PropsDialog>,
    /// The connection manager, while it is open.
    pub connections: Option<connections::ConnectionManager>,
    /// Where the manager's cursor was when it closed last; it opens there again (RC-050-15).
    connections_cursor: Option<rc_engine::TreeItem>,
    /// The context menu, from the right click until it closes (RC-080-02).
    context_menu: Option<context_menu::ContextMenu>,
    /// The login-script editor, opened from the connection manager (RC-050-08).
    login_scripts: Option<login_scripts::Editor>,
    /// The quick connection dialog, while it is open.
    pub quick: Option<quick_connect::QuickDialog>,
    /// The "Run command" dialog, while it is open (RC-060-04).
    ssh_exec: Option<ssh_exec::ExecDialog>,
    /// The open FTPS certificate questions and their dialog (RC-070-02).
    cert_trust: cert_trust::CertTrustDialogs,
    /// The offer to remove the vault, open after the last secret went (RC-050-03).
    vault_offer: bool,
    /// A checksum dialog or the result list of checks, while one is open (RC-020-05).
    pub checksum: Option<checksum::ChecksumWindow>,
    /// The multi-rename dialog, while it is open or its run has not ended (RC-020-02).
    multi_rename: Option<multi_rename::MultiRenameDialog>,
    /// Drag and drop and files on the clipboard (RC-080-01).
    drag: drag::DragUi,
    /// The command palette, while it is open (RC-080-05).
    palette: Option<palette::Palette>,
    /// The diff window and the synchronize-dirs dialog (RC-020-04).
    pub compare: compare::CompareUi,
    /// The queue window, while it is open (RC-010-14).
    pub queue_window: Option<queue_window::QueueWindow>,
    /// The column widths, the panel divider and their drag (RC-010-20).
    resize: resize::Resize,
    /// The main window and the dialogs' windows of their own (RC-080-15).
    dialog_windows: dialog_windows::DialogWindows,
    /// What F10 does and the question before quitting (RC-080-17).
    quit: quit::Quit,
}

impl App {
    /// An app over `engine`, its panels as the engine has them, with no database and no MCP
    /// server (the start adds both).
    pub fn new(engine: Engine, keymap: Keymap) -> Self {
        let (jobs, queue) = mpsc::unbounded_channel();
        let panes = PanelId::BOTH.map(|panel| Pane::new(engine.panel(panel)));
        let tab_rows = tabs::TabsUi::new(&engine);
        Self {
            active: engine.active_panel(),
            engine,
            keymap,
            jobs,
            queue: Arc::new(Mutex::new(Some(queue))),
            panes,
            dark: false,
            follow_system: false,
            terms: TermPanes::default(),
            hovered: None,
            window: WindowState::new(None),
            db: None,
            mcp: McpStatus::Off,
            notice: None,
            prompt: None,
            mask_history: Vec::new(),
            drive_bar: drive_bar::DriveBar::default(),
            system_dark: false,
            settings: None,
            settings_tabs: settings::Tabs::default(),
            mcp_server: McpServer::default(),
            info: None,
            database: None,
            host_keys: hostkeys::HostKeyDialogs::default(),
            mcp_questions: Vec::new(),
            mcp_grants: Vec::new(),
            icons: icons::Icons::default(),
            tabs: tab_rows,
            file_ops: file_ops::FileOps::default(),
            cmdline: cmdline::CmdLine::default(),
            search: None,
            viewers: viewer::Viewers::default(),
            archive: None,
            properties: None,
            connections: None,
            connections_cursor: None,
            context_menu: None,
            login_scripts: None,
            quick: None,
            ssh_exec: None,
            cert_trust: cert_trust::CertTrustDialogs::default(),
            vault_offer: false,
            checksum: None,
            multi_rename: None,
            drag: drag::DragUi::default(),
            palette: None,
            compare: compare::CompareUi::default(),
            queue_window: None,
            resize: resize::Resize::default(),
            dialog_windows: dialog_windows::DialogWindows::default(),
            quit: quit::Quit::default(),
        }
    }

    #[cfg(test)]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn pane(&self, panel: PanelId) -> &Pane {
        &self.panes[panel.index()]
    }

    fn pane_mut(&mut self, panel: PanelId) -> &mut Pane {
        &mut self.panes[panel.index()]
    }

    /// The main window's title, fixed as Total Commander's (owner, 2026-10-18): name, version
    /// and author; the panels show the path. A dialog's window has the dialog's.
    fn title(&self, id: window::Id) -> String {
        self.window_title(id)
    }

    fn theme(&self, _window: window::Id) -> Option<Theme> {
        Some(theme::iced_theme(theme::tokens(self.dark)))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let effects = self.apply(message);
        let mut tasks: Vec<Task<Message>> = effects
            .into_iter()
            .map(|effect| self.perform(effect))
            .collect();
        tasks.push(self.request_icons());
        tasks.push(self.sync_dialog_windows());
        Task::batch(tasks)
    }

    /// Changes the app's state for `message` and says what is left to do.
    pub fn apply(&mut self, message: Message) -> Vec<Effect> {
        // A key recorded for the Shortcuts tab is that key, F10 and Escape too (RC-100-21).
        if let Some(effects) = self.shortcut_recording(&message) {
            return effects;
        }
        // F10 closes a dialog as Escape does, or quits, as `keys.f10` says (RC-080-17).
        let Some(message) = self.f10_key(message) else {
            return Vec::new();
        };
        let message = self.escape_to_dialog(message);
        if let Some(effects) = self.intercept(&message) {
            return effects;
        }
        if let Some(effects) = self.dialog_key(&message) {
            return effects;
        }
        match message {
            Message::Engine(event) if archive_ops::concerns(&event) => {
                return self.on_archive_event(event);
            }
            Message::Key(chord) => {
                // The paste question and the shell choice have the keys while they are open.
                if let Some(effects) = self.terminal_key(&chord) {
                    return effects;
                }
                return self.on_key(chord, false);
            }
            Message::Done(panel, Err(code)) => {
                self.pane_mut(panel).error = Some(i18n::error_text(code));
            }
            Message::Done(_, Ok(())) => {}
            Message::VaultRefused(panel, commands) => return self.vault_refused(panel, commands),
            Message::Engine(event) if connections::concerns(&event) => {
                let mut effects = self.on_connections_event(&event);
                effects.extend(self.trust_store_changed(&event));
                if let EngineEvent::LoginScript { .. } = event {
                    effects.extend(self.login_scripts_changed());
                }
                if drive_bar::concerns(&event) {
                    effects.extend(self.on_drive_event(event));
                }
                return effects;
            }
            Message::Engine(event) if drive_bar::concerns(&event) => {
                return self.on_drive_event(event);
            }
            Message::Engine(EngineEvent::SettingChanged { key })
                if key == rc_engine::settings::FILE_ICONS_KEY =>
            {
                return vec![Effect::Task(self.load_icon_mode())];
            }
            Message::Engine(EngineEvent::FileDialog { panel, dialog }) => {
                return self.open_file_dialog(panel, dialog);
            }
            Message::Engine(EngineEvent::Job { job, change }) => {
                self.archive_test_result(job, &change);
                self.checksum_result(job, &change);
                let renamed = self.multi_rename_job(job, &change);
                let mut effects = self.drag_job(job, &change);
                effects.extend(self.on_job(job, change));
                effects.extend(renamed);
                return effects;
            }
            Message::Engine(EngineEvent::SettingChanged { key })
                if file_ops::settings_concern(&key) =>
            {
                return vec![Effect::Task(self.file_ops_settings_task())];
            }
            Message::Engine(EngineEvent::SettingChanged { key }) if cmdline::concerns(&key) => {
                return self.cmdline_settings();
            }
            Message::Engine(EngineEvent::SettingChanged { key }) if resize::concerns(&key) => {
                return vec![Effect::Task(self.resize_task())];
            }
            Message::Engine(EngineEvent::SettingChanged { key }) => {
                return self.setting_changed(&key);
            }
            Message::Engine(EngineEvent::ContextMenu { panel }) => {
                return self.context_menu_wanted(panel);
            }
            Message::Engine(EngineEvent::ChecksumDialog { panel, dialog }) => {
                return self.open_checksum_dialog(panel, dialog);
            }
            Message::Engine(event) if queue_window::concerns(&event) => {
                return self.on_queue_event(&event);
            }
            Message::Engine(event) => return self.on_engine(event),
            Message::EngineLagged => return self.reload(),
            Message::Viewport(panel, size) => {
                let rows = (size.height / theme::fonts::list().row_height()).floor() as usize;
                self.pane_mut(panel).set_width(size.width);
                self.pane_mut(panel).set_page_rows(rows);
                self.engine.set_page_rows(panel, rows);
            }
            Message::Scroll(panel, delta) => {
                let rows = theme::wheel::rows(delta, theme::fonts::list().row_height());
                self.pane_mut(panel).scroll(rows);
            }
            Message::ScrollTo(panel, top) => self.pane_mut(panel).scroll_to(top),
            Message::Click(panel, row) => return vec![self.click(panel, row)],
            Message::Open(panel, row) => return self.double_click(panel, row),
            Message::Hover(panel) => self.hovered = Some(panel),
            Message::Unhover(panel) => {
                if self.hovered == Some(panel) {
                    self.hovered = None;
                }
            }
            Message::Press => {
                if let Some(panel) = self.hovered.filter(|panel| *panel != self.active) {
                    let activate = Command::SwitchActivePanel { panel: Some(panel) };
                    return vec![Effect::Dispatch(panel, vec![activate])];
                }
            }
            Message::SystemTheme(mode) => {
                self.system_theme(mode);
                self.restyle_terminals();
            }
            Message::Terminal(message) => return self.on_terminal(message),
            Message::WindowChanged(id, change) => {
                let seen =
                    window::is_maximized(id).map(move |max| Message::WindowSeen(id, change, max));
                return vec![Effect::Task(seen)];
            }
            Message::WindowSeen(id, change, maximized) => self.window_seen(id, change, maximized),
            Message::CloseRequested(id) => return self.close_requested(id),
            Message::Saved => return vec![Effect::Task(startup::leave())],
            Message::KeyCaptured(chord) => return self.on_key(chord, true),
            Message::SortBy(panel, key) => {
                let sort = Command::Sort {
                    panel: Some(panel),
                    key,
                    descending: None,
                };
                return vec![Effect::Dispatch(panel, vec![sort])];
            }
            Message::PathSegment(panel, path) => {
                let navigate = Command::Navigate {
                    panel: Some(panel),
                    path: path.to_string_lossy().into_owned(),
                };
                return vec![Effect::Dispatch(panel, vec![navigate])];
            }
            Message::EditPath(panel) => return self.open_prompt(panel, keys::PromptKind::Path),
            Message::PromptInput(text) => self.prompt_input(text),
            Message::PromptSubmit => return self.submit_prompt(),
            Message::PromptRecall(mask) => return self.prompt_recall(mask),
            Message::ToggleHidden => {
                let toggle = Command::ToggleHidden { show: None };
                return vec![Effect::Dispatch(self.active, vec![toggle])];
            }
            Message::Drive(message) => return self.on_drive(message),
            Message::Settings(message) => return self.settings_message(message),
            Message::Database(message) => return self.on_database(message),
            Message::HostKey(message) => return self.on_host_key(message),
            Message::McpConfirm(message) => return self.on_mcp_confirm(message),
            Message::Icons(message) => return self.on_icons(message),
            Message::Tabs(message) => return self.on_tabs(message),
            Message::FileOps(message) => return self.on_file_ops(message),
            Message::CmdLine(message) => return self.on_cmdline(message),
            Message::Search(message) => return self.on_search(message),
            Message::Viewer(message) => return self.on_viewer(message),
            Message::Archive(message) => return self.on_archive(message),
            Message::Checksum(message) => return self.on_checksum(message),
            Message::Properties(message) => return self.on_properties(message),
            Message::Connections(message) => return self.on_connections(message),
            Message::ContextMenu(message) => return self.on_context_menu(message),
            Message::LoginScripts(message) => return self.on_login_scripts(message),
            Message::MultiRename(message) => return self.on_multi_rename(message),
            Message::Drag(message) => return self.on_drag(message),
            Message::Palette(message) => return self.on_palette(message),
            Message::Compare(message) => return self.on_compare(message),
            Message::Queue(message) => return self.on_queue(message),
            Message::Browse(message) => return self.on_browse(message),
            Message::Resize(message) => return self.on_resize(message),
            // The intercept closes the editor; without one open there is nothing to close. The
            // self-update's messages are its intercept's (`update_ui.rs`).
            Message::CloseLoginScripts | Message::Update(_) => {}
            Message::QuickConnect(message) => return self.on_quick_connect(message),
            Message::SshExec(message) => return self.on_ssh_exec(message),
            Message::CertTrust(message) => return self.on_cert_trust(message),
            Message::VaultOffer(message) => return self.on_vault_offer(message),
            Message::Quit(message) => return self.on_quit(message),
            Message::DialogWindows(own) => self.dialog_windows.own = own,
            Message::DialogWindow(message) => return self.on_dialog_window(message),
            Message::AccessKeys(always) => theme::mnemonic::set_always(always),
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "app_selection_tests.rs"]
mod selection_tests;
