//! The open connections in the drive bar (RC-110-38): every connection with a session open now
//! (`drives.connections`: SSH, FTP, FTPS, a quick connection too) is a button of its own and is
//! marked in the drive menu, with the setting `drive_bar.show_connections` on (the default). A
//! click shows the server where a panel last was on it (`panel.change_drive`, which reconnects a
//! session lost meanwhile as for any saved server); its context menu - a right click on the
//! button, Shift+F10 on its row of the drive menu - is the connection and "Disconnect"
//! (`menu::Scope::Connection`), which ends the session (`connections.disconnect`) also when no
//! panel shows it. The list is read with the drives and again on every session's start and end
//! ([`rc_engine::EngineEvent::Session`]), so a button comes and goes with its session.

use iced::Task;
use rc_engine::{Command, ConnectionId, DriveGroup, OpenConnection, Outcome, PanelId};

use super::{
    DriveBar, DriveMessage,
    actions::menu_actions,
    menu::{DriveMenu, Scope},
    place::{self, Button},
};
use crate::app::{App, Effect, Message};

impl DriveBar {
    /// Whether the entry `id` is an open connection the bar shows as one.
    pub fn is_open(&self, id: &str) -> bool {
        self.line.connections && self.connections.iter().any(|open| open.drive == id)
    }

    /// The entries as the bar shows them: the engine's, each server with a session open a button
    /// of its own, as a server marked for the drive bar is.
    pub(super) fn merge(&mut self) {
        let mut drives = self.listed.clone();
        for drive in &mut drives {
            if drive.group == DriveGroup::Servers && self.is_open(&drive.id) {
                drive.favourite = true;
            }
        }
        self.drives = drives;
    }
}

impl App {
    /// The open connections as the engine lists them now.
    pub(super) fn list_connections(&self) -> Effect {
        let engine = self.engine.clone();
        let listed = Task::perform(
            async move {
                match engine.dispatch(Command::ListOpenConnections).await {
                    Ok(Outcome::OpenConnections { connections }) => Ok(connections),
                    Ok(_) => Ok(Vec::new()),
                    Err(error) => Err(error.code()),
                }
            },
            |listed| Message::Drive(DriveMessage::Connections(listed)),
        );
        Effect::Task(listed)
    }

    /// The open connections are `listed`, or could not be read (the bar keeps what it had).
    pub(super) fn on_open_connections(
        &mut self,
        listed: Result<Vec<OpenConnection>, &'static str>,
    ) -> Vec<Effect> {
        if let Ok(connections) = listed {
            self.drive_bar.connections = connections;
            self.drive_bar.merge();
            self.drive_menu_follows();
        }
        Vec::new()
    }

    /// The open menu after the entries changed: the full menu's actions anew and its cursor on
    /// a line; a connection's context menu closes with its session.
    pub(super) fn drive_menu_follows(&mut self) {
        let bar = &mut self.drive_bar;
        let gone = matches!(
            &bar.menu,
            Some(DriveMenu { scope: Scope::Connection(id), .. }) if !bar.is_open(id)
        );
        if gone {
            bar.menu = None;
        }
        if let Some(menu) = &mut bar.menu {
            if menu.scope == Scope::All {
                menu.actions = menu_actions(&bar.drives, bar.network);
            }
            menu.clamp(&bar.drives);
        }
    }

    /// The context menu of the open connection `id` from `panel`'s bar or drive menu: in the
    /// drive menu's place when that is open, else under the connection's button. `None` when
    /// `id` is no open connection.
    pub(super) fn connection_context(&mut self, panel: PanelId, id: &str) -> Option<Vec<Effect>> {
        if !self.drive_bar.is_open(id) {
            return None;
        }
        let scope = Scope::Connection(id.to_owned());
        if self.drive_bar.menu.is_some() {
            let drives = &self.drive_bar.drives;
            self.drive_bar.menu = Some(DriveMenu::scoped(panel, scope, drives, None));
            return Some(Vec::new());
        }
        let button = place::id(panel, Button::Drive(id));
        Some(self.open_drive_menu(panel, scope, Some(button)))
    }

    /// "Disconnect" on the open connection `id`: its session ends; a panel on it goes back to a
    /// local folder (`connections.disconnect`), and the button goes with the session's end.
    pub(super) fn disconnect_drive(&mut self, panel: PanelId, id: &str) -> Vec<Effect> {
        self.drive_bar.menu = None;
        let Some(connection) = connection_of(id) else {
            return Vec::new();
        };
        vec![Effect::Dispatch(
            panel,
            vec![Command::DisconnectSession { id: connection }],
        )]
    }
}

/// The connection of the drive-bar entry `id` (`server:<connection id>`).
fn connection_of(id: &str) -> Option<ConnectionId> {
    id.strip_prefix("server:")?.parse().ok().map(ConnectionId)
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
