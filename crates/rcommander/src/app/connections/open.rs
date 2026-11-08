//! Opening the connection manager and closing it when it asks (`Ctrl+F`).

use super::{
    App, Command, ConnectionManager, Effect, Focus, Then, credentials, keys, remote, run, sources,
    start, view,
};

impl App {
    pub(super) fn open_connections(&mut self) -> Vec<Effect> {
        // In a window of its own, Ctrl+F brings it to the front (RC-080-15).
        if self.connections.is_some() {
            return self
                .dialog_window_front(super::super::dialog_windows::DialogWindow::Connections);
        }
        // The keyboard in the list, on the connection used last; typing there filters at once
        // (owner, 2026-10-26; `start.rs`).
        self.connections = Some(ConnectionManager {
            sessions: self.engine.ssh_sessions(),
            focus: Focus::Tree,
            start: self.connections_cursor,
            ..ConnectionManager::default()
        });
        vec![
            keys::focus_id(view::TREE_STOP_ID),
            start::read_focus_setting(),
            run(Then::Tree, Command::ListConnections),
            run(Then::Drives, Command::ListDrives { refresh: false }),
            run(Then::Vault, Command::GetVaultStatus),
            run(Then::Scripts, Command::ListLoginScripts),
            remote::list_certificates(),
            sources::list_sources(),
            credentials::read_setting(),
        ]
    }

    pub(super) fn close_connections_if_asked(&mut self) {
        if let Some(manager) = &self.connections {
            self.connections_cursor = manager.tree.cursor;
        }
        if self
            .connections
            .as_ref()
            .is_some_and(|manager| manager.close)
        {
            self.connections = None;
        }
    }
}
