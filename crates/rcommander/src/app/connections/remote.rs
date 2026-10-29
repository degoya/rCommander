//! What the manager opens on a stored connection: Enter or "Open in panel" shows the server in
//! the active panel, over SFTP (RC-060-02) or FTP (RC-070-03), "Terminal" or Shift+Enter starts a
//! shell on it (`terminal.open_ssh`, RC-060-03), Ctrl+Enter connects as "Connect" does, "Run command..." opens the dialog of one command
//! (RC-060-04). An FTPS connection
//! shows the certificate trusted for its server, which "Forget" removes (`certificates.list`
//! and `certificates.remove`, RC-070-02): the next connection asks again.

use iced::Task;
use rc_db::TrustedCertificate;
use rc_engine::{Command, ConnectionId, ConnectionKind, TreeItem};

use super::{ConnMsg, ConnectionManager, jobs::Then, run};
use crate::app::{App, Effect, Message};

impl ConnectionManager {
    /// Enter or a double click on a connection: the server opens in the active panel, over SFTP
    /// or FTP.
    pub(super) fn open_connection(&mut self, id: ConnectionId) -> Vec<Effect> {
        if self.tree.connection(id).is_none() {
            return Vec::new();
        }
        let files = Message::Connections(ConnMsg::Files(id));
        vec![Effect::Task(Task::done(files))]
    }

    /// Shift+Enter in the tree: "Terminal" for the SSH server at the cursor.
    pub(super) fn terminal_key(&mut self) -> Vec<Effect> {
        self.ssh_at_cursor()
            .map(|id| {
                let terminal = Message::Connections(ConnMsg::Terminal(id));
                vec![Effect::Task(Task::done(terminal))]
            })
            .unwrap_or_default()
    }

    /// Ctrl+Enter in the tree: "Connect" for the SSH server at the cursor.
    pub(super) fn connect_key(&mut self) -> Vec<Effect> {
        match self.ssh_at_cursor() {
            Some(id) => self.session_message(ConnMsg::Connect(id)),
            None => Vec::new(),
        }
    }

    /// The SSH server at the tree's cursor; none on another row, where its buttons do not show.
    fn ssh_at_cursor(&self) -> Option<ConnectionId> {
        let Some(TreeItem::Connection(id)) = self.tree.current() else {
            return None;
        };
        self.tree
            .connection(id)
            .is_some_and(|connection| connection.kind == ConnectionKind::Ssh)
            .then_some(id)
    }

    /// The certificates trusted for the server of the FTPS connection `id`; none for another
    /// kind.
    pub(super) fn certificates_of(&self, id: ConnectionId) -> Vec<&TrustedCertificate> {
        let Some(connection) = self.tree.connection(id) else {
            return Vec::new();
        };
        if !matches!(
            connection.kind,
            ConnectionKind::Ftps | ConnectionKind::FtpsImplicit
        ) {
            return Vec::new();
        }
        self.certificates
            .iter()
            .filter(|stored| {
                stored.host.eq_ignore_ascii_case(&connection.host) && stored.port == connection.port
            })
            .collect()
    }

    /// "Forget" at a trusted certificate; the engine's event reads the list again.
    pub(super) fn forget_certificate(&mut self, id: i64) -> Vec<Effect> {
        self.note = None;
        vec![run(Then::Changed, Command::RemoveCertificate { id })]
    }
}

/// Reads the trusted certificates into the open manager.
pub(super) fn list_certificates() -> Effect {
    run(
        Then::Certificates,
        Command::ListCertificates { filter: None },
    )
}

impl App {
    /// Open in panel, Terminal and Run command: the first two close the manager and act on the
    /// active panel, the third opens its dialog over the manager.
    pub(in crate::app) fn connection_action(&mut self, message: ConnMsg) -> Vec<Effect> {
        let panel = self.active;
        let command = match message {
            ConnMsg::Files(id) => Command::ChangeDrive {
                panel: Some(panel),
                drive: format!("server:{id}"),
            },
            ConnMsg::Terminal(id) => Command::OpenSshTerminal {
                connection_id: id,
                panel: Some(panel),
                cwd: None,
                size: None,
            },
            ConnMsg::RunCommand(id) => {
                let name = self
                    .connections
                    .as_ref()
                    .and_then(|manager| manager.tree.connection(id))
                    .map(|connection| connection.name.clone());
                return self.open_ssh_exec(id, name, String::new());
            }
            _ => return Vec::new(),
        };
        self.connections = None;
        vec![Effect::Dispatch(panel, vec![command])]
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
