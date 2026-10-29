//! The drive bar and the drive menu, as Total Commander has them.
//!
//! A bar of drive buttons sits above each panel, or one above both (the setting
//! `drive_bar.layout`, toggled by the button at the bar's end and kept across restarts); a click
//! shows the drive in that panel, or with one bar in the active one. Under it each panel has its
//! drive line (`line.rs`, RC-010-22): the current drive, its volume name and free space, and the
//! buttons to the root and one folder up; a click on the drive there or Alt+F1 and Alt+F2 open the
//! drive menu, which the keyboard runs alone (`menu.rs`). The entries are the
//! engine's (`drives.list`): drives, then WSL distributions, then saved servers. An SSH server
//! opens over SFTP in its home directory (RC-060-02), an FTP or FTPS server over FTP where its
//! login starts (RC-070-03). A network drive shows the folder it is mapped to; the drive menu, a
//! right click on a drive and the command palette map and disconnect network drives
//! (`actions.rs`, `network.rs`, RC-010-16). In the drive menu Shift+F10 and the context-menu key
//! open the context menu of the drive under the cursor (RC-080-16).
//!
//! The list is read off the job queue, so a slow drive never holds a key press: on start, every
//! few seconds (new drives appear), and when the engine announces a change.

pub mod actions;
mod line;
mod menu;
pub mod network;
mod network_view;
mod overflow;
mod view;

use std::{
    hash::{Hash, Hasher},
    time::Duration,
};

use iced::{
    Subscription, Task,
    futures::{Stream, stream},
};
use rc_db::Db;
use rc_engine::{Chord, Command, Drive, EngineEvent, NETWORK_DRIVES, Outcome, PanelId, drive_of};
use serde::{Deserialize, Serialize};

pub(super) use self::line::concerns as line_concerns;
pub(super) use self::network_view::{body as network_body, title as network_title};
pub(super) use self::view::{above_panel, above_panels, with_menu};
use self::{
    actions::{DriveAction, DriveContext, context_actions, menu_actions},
    line::{LineSettings, LineStep},
    menu::{DriveMenu, MenuAction},
    network::{NetworkDialog, NetworkMsg},
};
use super::{App, Effect, Message, scroll};

/// The settings key of the layout.
pub const LAYOUT_KEY: &str = "drive_bar.layout";

/// The id of the drive menu's scrollable, which keeps the menu's cursor in view.
const MENU_SCROLL_ID: &str = "drive-bar.menu";

/// How often the drives are read again, for a stick plugged in or a share connected.
const POLL: Duration = Duration::from_secs(3);

/// Where the drive bars sit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveBarLayout {
    /// A bar above each panel (Total Commander's default).
    #[default]
    PerPanel,
    /// One bar above both panels; it acts on the active panel.
    Shared,
}

impl DriveBarLayout {
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::PerPanel => Self::Shared,
            Self::Shared => Self::PerPanel,
        }
    }
}

/// The stored layout; the default when there is none or it does not read.
pub async fn load_layout(db: &Db) -> DriveBarLayout {
    db.get::<DriveBarLayout>(LAYOUT_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Stores `layout`.
///
/// # Errors
///
/// The database write fails.
pub async fn save_layout(db: &Db, layout: DriveBarLayout) -> rc_db::Result<()> {
    db.set(LAYOUT_KEY, &layout).await
}

#[derive(Debug, Clone)]
pub enum DriveMessage {
    /// Time to read the drives again.
    Poll,
    /// The entries, or the error code of the list.
    Listed(Result<Vec<Drive>, &'static str>),
    /// A drive button or a menu row: the panel shows the drive with this id.
    Pick(PanelId, String),
    /// The drive line was clicked: the panel's drive menu opens.
    OpenMenu(PanelId),
    /// A folder of the connection tree was clicked: the menu opens at its servers.
    OpenFolder(PanelId, String),
    /// A click beside the open menu.
    CloseMenu,
    /// The button at the bar's end: one bar or one per panel.
    ToggleLayout,
    /// The layout is stored, or could not be.
    Stored,
    /// A right click on a drive button or a drive menu row: the drive's context menu opens.
    Context(PanelId, String),
    /// A network drive action runs for the panel, on this drive if one is named.
    Run(PanelId, DriveAction, Option<String>),
    /// The network drive dialogs.
    Network(NetworkMsg),
    /// `\` or `..` in a panel's drive line.
    Step(PanelId, LineStep),
    /// The drive line's settings, read at the start and after a change.
    Line(LineSettings),
}

/// What the window knows of the drives.
#[derive(Debug)]
pub struct DriveBar {
    /// The entries as last listed.
    pub drives: Vec<Drive>,
    pub layout: DriveBarLayout,
    /// The open drive menu.
    pub menu: Option<DriveMenu>,
    /// The open context menu of a drive.
    pub context: Option<DriveContext>,
    /// The open network drive dialog.
    pub dialog: Option<NetworkDialog>,
    /// What the drive lines show (`panels.drive_line_*`).
    pub line: LineSettings,
    /// A list is on its way: a poll meanwhile asks nothing.
    listing: bool,
    /// The engine announced a change while a list was on its way: list once more.
    again: bool,
    /// Whether the menus offer the network drive actions: where the platform maps network
    /// drives ([`NETWORK_DRIVES`]).
    pub network: bool,
    /// The network drive dialogs opened so far, each one's generation.
    dialogs: u64,
}

impl DriveBar {
    /// The generation of the network drive dialog that opens now.
    fn next_dialog(&mut self) -> u64 {
        self.dialogs += 1;
        self.dialogs
    }
}

impl Default for DriveBar {
    fn default() -> Self {
        Self {
            drives: Vec::new(),
            layout: DriveBarLayout::default(),
            menu: None,
            context: None,
            dialog: None,
            line: LineSettings::default(),
            listing: false,
            again: false,
            network: NETWORK_DRIVES,
            dialogs: 0,
        }
    }
}

/// Whether the drive bar takes `event` rather than the panels.
pub fn concerns(event: &EngineEvent) -> bool {
    matches!(event, EngineEvent::Drives | EngineEvent::DriveMenu { .. })
}

impl App {
    pub(super) fn on_drive(&mut self, message: DriveMessage) -> Vec<Effect> {
        match message {
            DriveMessage::Poll => return self.list_drives(),
            DriveMessage::Listed(listed) => {
                self.drive_bar.listing = false;
                // A failed list keeps what the bar had; the next poll tries again.
                if let Ok(drives) = listed {
                    self.drive_bar.drives = drives;
                }
                if let Some(menu) = &mut self.drive_bar.menu {
                    menu.actions = menu_actions(&self.drive_bar.drives, self.drive_bar.network);
                    menu.clamp(self.drive_bar.drives.len());
                }
                if std::mem::take(&mut self.drive_bar.again) {
                    return self.list_drives();
                }
            }
            DriveMessage::Pick(panel, id) => {
                self.drive_bar.menu = None;
                return self.pick_drive(panel, &id);
            }
            DriveMessage::OpenMenu(panel) => return self.open_drive_menu(panel),
            DriveMessage::OpenFolder(panel, name) => {
                let cursor = self
                    .drive_bar
                    .drives
                    .iter()
                    .position(|drive| drive.folder.first() == Some(&name))
                    .unwrap_or(0);
                let actions = menu_actions(&self.drive_bar.drives, self.drive_bar.network);
                self.drive_bar.menu = Some(DriveMenu {
                    panel,
                    cursor,
                    actions,
                });
                return self.menu_in_view();
            }
            DriveMessage::CloseMenu => {
                self.drive_bar.menu = None;
                self.drive_bar.context = None;
            }
            DriveMessage::Context(panel, id) => self.open_drive_context(panel, &id),
            DriveMessage::Run(panel, action, drive) => {
                return self.run_drive_action(panel, action, drive);
            }
            DriveMessage::Network(message) => return self.on_network(message),
            DriveMessage::ToggleLayout => {
                self.drive_bar.layout = self.drive_bar.layout.toggled();
                return self.store_layout();
            }
            DriveMessage::Step(panel, step) => {
                let commands = line::step_commands(step, panel, self.active);
                return vec![Effect::Dispatch(panel, commands)];
            }
            DriveMessage::Line(settings) => self.drive_bar.line = settings,
            DriveMessage::Stored => {}
        }
        Vec::new()
    }

    /// An event of [`concerns`]: the menu opens, or the drives are read again.
    pub(super) fn on_drive_event(&mut self, event: EngineEvent) -> Vec<Effect> {
        if let EngineEvent::DriveMenu { panel } = event {
            return self.open_drive_menu(panel);
        }
        self.list_drives()
    }

    /// A key while the drive menu is open: the menu has the keyboard.
    pub(super) fn drive_menu_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let Some(menu) = self.drive_bar.menu.as_mut() else {
            return Vec::new();
        };
        let panel = menu.panel;
        match menu.key(chord, &self.drive_bar.drives) {
            MenuAction::Stay => self.menu_in_view(),
            MenuAction::Close => {
                self.drive_bar.menu = None;
                Vec::new()
            }
            MenuAction::Pick(id) => {
                self.drive_bar.menu = None;
                self.pick_drive(panel, &id)
            }
            MenuAction::Run(action) => self.run_drive_action(panel, action, None),
            MenuAction::Switch(other) => self.open_drive_menu(other),
            MenuAction::Context(id) => {
                self.open_drive_context(panel, &id);
                Vec::new()
            }
        }
    }

    /// The drive under `panel`, as the bar's entries know it.
    pub fn current_drive(&self, panel: PanelId) -> Option<&Drive> {
        drive_of(&self.drive_bar.drives, &self.pane(panel).state.path)
    }

    fn open_drive_menu(&mut self, panel: PanelId) -> Vec<Effect> {
        let current = self.current_drive(panel).map(|drive| drive.id.clone());
        let actions = menu_actions(&self.drive_bar.drives, self.drive_bar.network);
        self.drive_bar.menu = Some(
            DriveMenu::new(panel, &self.drive_bar.drives, current.as_deref()).with_actions(actions),
        );
        self.menu_in_view()
    }

    /// The context menu of the drive `id`, when it offers anything, in place of the drive menu;
    /// a drive without one (a WSL distribution, a server) leaves both as they are, as a Win32
    /// menu entry without a context menu does.
    fn open_drive_context(&mut self, panel: PanelId, id: &str) {
        let actions = self
            .drive_bar
            .drives
            .iter()
            .find(|drive| drive.id == id)
            .map(|drive| context_actions(drive, self.drive_bar.network))
            .unwrap_or_default();
        if actions.is_empty() {
            return;
        }
        self.drive_bar.menu = None;
        self.drive_bar.context = Some(DriveContext {
            panel,
            drive: id.to_owned(),
            actions,
            cursor: 0,
        });
    }

    /// The open menu scrolled so its cursor is in view (a long list of servers after an
    /// import); nothing when it is closed.
    fn menu_in_view(&self) -> Vec<Effect> {
        let Some(menu) = &self.drive_bar.menu else {
            return Vec::new();
        };
        let (line, lines) = menu::cursor_line(&self.drive_bar.drives, &menu.actions, menu.cursor);
        vec![scroll::keep_in_view(MENU_SCROLL_ID, line, lines)]
    }

    /// `panel` shows the drive `id`: the panel becomes active, then changes the drive - one job,
    /// in this order. A server connects on the way (SFTP or FTP); what keeps it from opening is
    /// the panel's error.
    fn pick_drive(&mut self, panel: PanelId, id: &str) -> Vec<Effect> {
        if !self.drive_bar.drives.iter().any(|drive| drive.id == id) {
            return Vec::new();
        }
        let mut commands = Vec::with_capacity(2);
        if panel != self.active {
            commands.push(Command::SwitchActivePanel { panel: Some(panel) });
        }
        commands.push(Command::ChangeDrive {
            panel: Some(panel),
            drive: id.to_owned(),
        });
        vec![Effect::Dispatch(panel, commands)]
    }

    /// Reads the entries off the job queue; one list at a time.
    fn list_drives(&mut self) -> Vec<Effect> {
        if self.drive_bar.listing {
            self.drive_bar.again = true;
            return Vec::new();
        }
        self.drive_bar.listing = true;
        let engine = self.engine.clone();
        let listed = Task::perform(
            async move {
                match engine
                    .dispatch(Command::ListDrives { refresh: false })
                    .await
                {
                    Ok(Outcome::Drives { drives }) => Ok(drives),
                    Ok(_) => Ok(Vec::new()),
                    Err(error) => Err(error.code()),
                }
            },
            |listed| Message::Drive(DriveMessage::Listed(listed)),
        );
        vec![Effect::Task(listed)]
    }

    /// Reads what the drive lines show: at the start and after a change of one of their settings.
    pub(super) fn drive_line_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(line::load(db), |settings| {
            Message::Drive(DriveMessage::Line(settings))
        })
    }

    fn store_layout(&self) -> Vec<Effect> {
        let Some(db) = self.db.clone() else {
            return Vec::new();
        };
        let layout = self.drive_bar.layout;
        let store = async move {
            if let Err(error) = save_layout(&db, layout).await {
                tracing::warn!(%error, "the drive bar's layout was not stored");
            }
        };
        let stored = Task::perform(store, |()| Message::Drive(DriveMessage::Stored));
        vec![Effect::Task(stored)]
    }
}

/// The ticks that read the drives: one at once, then every [`POLL`].
pub(super) fn poll() -> Subscription<Message> {
    Subscription::run_with(DrivePoll, ticks)
}

/// The drive poll; one per window, so its identity is constant.
struct DrivePoll;

impl Hash for DrivePoll {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "rcommander.drive-poll".hash(state);
    }
}

fn ticks(_: &DrivePoll) -> impl Stream<Item = Message> + use<> {
    stream::unfold(true, |first| async move {
        if !first {
            tokio::time::sleep(POLL).await;
        }
        Some((Message::Drive(DriveMessage::Poll), false))
    })
}

#[cfg(test)]
#[path = "drive_bar_tests.rs"]
mod tests;
