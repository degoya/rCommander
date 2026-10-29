//! A connection's SSH session in the manager (RC-050-03): its state under the form, Connect and
//! Disconnect, and Test for every kind. The sessions are the engine's (`Engine::ssh_sessions`),
//! read again whenever it announces a change; Ctrl+Shift+F closes the active panel's connection,
//! as in Total Commander.

use rc_engine::{Chord, Command, ConnectionId, Outcome, PanelView, SessionStatus};
use rc_vfs::BackendId;

use super::{ConnMsg, ConnectionManager, jobs::Then, run, vault::Retry};
use crate::{
    app::{App, Effect},
    i18n::{error_text, t},
};

impl ConnectionManager {
    /// Test, Connect or Disconnect for a stored connection.
    pub(super) fn session_message(&mut self, message: ConnMsg) -> Vec<Effect> {
        let (then, command) = match message {
            ConnMsg::Test(id) => (Then::Tested(id), Command::TestConnection { id }),
            ConnMsg::Connect(id) => (Then::Connect(id), Command::ConnectSession { id }),
            ConnMsg::Disconnect(id) => (Then::Changed, Command::DisconnectSession { id }),
            _ => return Vec::new(),
        };
        self.note = None;
        vec![run(then, command)]
    }

    /// A locked vault refused Connect or Test of `id`: the unlock prompt runs it again once it
    /// opened the vault; the note says so when the open prompt runs something else already.
    pub(super) fn session_refused(&mut self, then: Then, id: ConnectionId) -> Vec<Effect> {
        let command = match then {
            Then::Tested(_) => Command::TestConnection { id },
            _ => Command::ConnectSession { id },
        };
        self.refused(Retry::Manager(then, Box::new(command)))
            .unwrap_or_else(|| {
                self.note = Some((error_text("vault_locked"), true));
                Vec::new()
            })
    }

    /// How a test went, as the note under the form.
    pub(super) fn tested(&mut self, result: Result<Outcome, &'static str>) {
        self.note = Some(match result {
            Ok(_) => (t!("connections.tested").to_owned(), false),
            Err(code) => (error_text(code), true),
        });
    }

    /// The session of connection `id`, when one is up or being set up.
    pub(super) fn session(&self, id: ConnectionId) -> Option<&SessionStatus> {
        self.sessions
            .iter()
            .find(|status| status.connection_id == id)
    }
}

impl App {
    /// The engine announced a change of a session: the open manager reads them again.
    pub(in crate::app) fn sessions_changed(&mut self) {
        let sessions = self.engine.ssh_sessions();
        if let Some(manager) = self.connections.as_mut() {
            manager.sessions = sessions;
        }
    }

    /// Ctrl+Shift+F in the file list: the connection the active panel shows is closed. A panel
    /// on a local folder has none; the key does nothing there.
    pub(in crate::app) fn disconnect_key(
        &self,
        chord: &Chord,
        view: PanelView,
    ) -> Option<Vec<Effect>> {
        if !(chord.ctrl && chord.shift && !chord.alt && chord.key == "f")
            || view != PanelView::Files
        {
            return None;
        }
        let (BackendId::Ftp { connection_id } | BackendId::Sftp { connection_id }) =
            self.pane(self.active).state.path.backend
        else {
            return Some(Vec::new());
        };
        let disconnect = Command::DisconnectSession {
            id: ConnectionId(connection_id),
        };
        Some(vec![Effect::Dispatch(self.active, vec![disconnect])])
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
