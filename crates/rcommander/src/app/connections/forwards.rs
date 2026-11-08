//! Under the form of a stored SSH connection: its local port forwards (RC-110-03) - each with
//! its state and Start or Stop, Edit and Delete, and Add, which opens a small editor in the
//! panel (local address and port, target host and port, a name, whether it starts with the
//! session). Saving and starting are the person's here; the engine checks every field again.
//!
//! The local address offers `127.0.0.1` and `::1`; `0.0.0.0` and `::` only while
//! `ssh.forwards_lan` is on. A forward's list is read when the form shows its connection and
//! again whenever the engine announces a change of one of them.

use rc_engine::{
    Command, ConnectionId, ConnectionKind, ForwardDraft, ForwardView, ForwardsOutcome, Outcome,
    SaveForwardRequest,
};

use super::{ConnectionManager, jobs::Then, run};
use crate::{app::Effect, i18n::error_text};

/// The editor's local port field, which gets the keyboard when it opens.
pub const FORWARD_PORT_FIELD: &str = "rcommander.connections.forward_port";

/// What the panel's controls say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardMsg {
    Add,
    Edit(i64),
    Bind(String),
    Port(String),
    Host(String),
    TargetPort(String),
    Name(String),
    AutoStart(bool),
    Save,
    Cancel,
    Delete(i64),
    Start(i64),
    Stop(i64),
}

/// A forward being added (`id` `None`) or changed, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardEdit {
    pub id: Option<i64>,
    pub bind: String,
    pub port: String,
    pub host: String,
    pub target_port: String,
    pub name: String,
    pub auto_start: bool,
    pub error: Option<String>,
}

impl Default for ForwardEdit {
    fn default() -> Self {
        Self {
            id: None,
            bind: rc_engine::LOOPBACK.to_owned(),
            port: String::new(),
            host: String::new(),
            target_port: String::new(),
            name: String::new(),
            auto_start: false,
            error: None,
        }
    }
}

impl ForwardEdit {
    /// The forward as the editor would save it; `None` for a port that is no number.
    fn draft(&self) -> Option<ForwardDraft> {
        Some(ForwardDraft {
            bind_address: self.bind.clone(),
            bind_port: self.port.trim().parse().ok()?,
            target_host: self.host.trim().to_owned(),
            target_port: self.target_port.trim().parse().ok()?,
            auto_start: self.auto_start,
            name: Some(self.name.trim().to_owned()).filter(|name| !name.is_empty()),
        })
    }
}

/// The shown connection's forwards and the editor.
#[derive(Debug, Default)]
pub struct ForwardPanel {
    /// The connection whose forwards are held.
    pub connection: Option<ConnectionId>,
    pub forwards: Vec<ForwardView>,
    pub edit: Option<ForwardEdit>,
    /// `ssh.forwards_lan`: whether `0.0.0.0` and `::` are offered.
    pub lan: bool,
}

impl ForwardPanel {
    /// The local addresses offered, the editor's own first when it is another.
    pub fn binds(&self) -> Vec<String> {
        let mut binds: Vec<String> = vec![rc_engine::LOOPBACK.to_owned(), "::1".to_owned()];
        if self.lan {
            binds.extend(["0.0.0.0".to_owned(), "::".to_owned()]);
        }
        if let Some(edit) = &self.edit
            && !binds.contains(&edit.bind)
        {
            binds.insert(0, edit.bind.clone());
        }
        binds
    }
}

fn list(id: ConnectionId) -> Effect {
    run(
        Then::Forwards,
        Command::ListForwards {
            connection_id: Some(id),
        },
    )
}

impl ConnectionManager {
    /// The secret-source panel and this one follow the form.
    pub(super) fn panels_follow(&mut self) -> Vec<Effect> {
        let mut effects = self.sources_follow();
        effects.extend(self.forwards_follow());
        effects
    }

    /// The forwards follow the form: a stored SSH connection it shows now has its own read.
    fn forwards_follow(&mut self) -> Vec<Effect> {
        let shown = self
            .form
            .as_ref()
            .filter(|form| form.kind == ConnectionKind::Ssh && !form.is_credential())
            .and_then(|form| form.id());
        if shown == self.forwards.connection {
            return Vec::new();
        }
        self.forwards.connection = shown;
        self.forwards.forwards.clear();
        self.forwards.edit = None;
        let Some(id) = shown else {
            return Vec::new();
        };
        let lan = Command::GetSettings {
            key: Some(rc_engine::settings::FORWARDS_LAN_KEY.to_owned()),
        };
        vec![list(id), run(Then::ForwardsLan, lan)]
    }

    /// The engine announced a change of a forward of `connection`.
    pub(super) fn forwards_changed(&mut self, connection: ConnectionId) -> Vec<Effect> {
        if self.forwards.connection == Some(connection) {
            vec![list(connection)]
        } else {
            Vec::new()
        }
    }

    pub(super) fn on_forward(&mut self, message: ForwardMsg) -> Vec<Effect> {
        let Some(id) = self.forwards.connection else {
            return Vec::new();
        };
        let panel = &mut self.forwards;
        let edit = |panel: &mut ForwardPanel, change: &dyn Fn(&mut ForwardEdit)| {
            if let Some(edit) = &mut panel.edit {
                change(edit);
                edit.error = None;
            }
            Vec::new()
        };
        match message {
            ForwardMsg::Add => {
                panel.edit = Some(ForwardEdit::default());
                vec![super::keys::focus_id(FORWARD_PORT_FIELD)]
            }
            ForwardMsg::Edit(forward) => {
                let Some(view) = panel
                    .forwards
                    .iter()
                    .find(|view| view.forward.id == forward)
                else {
                    return Vec::new();
                };
                let stored = &view.forward;
                panel.edit = Some(ForwardEdit {
                    id: Some(stored.id),
                    bind: stored.bind_address.clone(),
                    port: stored.bind_port.to_string(),
                    host: stored.target_host.clone(),
                    target_port: stored.target_port.to_string(),
                    name: stored.name.clone().unwrap_or_default(),
                    auto_start: stored.auto_start,
                    error: None,
                });
                vec![super::keys::focus_id(FORWARD_PORT_FIELD)]
            }
            ForwardMsg::Bind(bind) => edit(panel, &|edit| edit.bind.clone_from(&bind)),
            ForwardMsg::Port(text) => edit(panel, &|edit| edit.port.clone_from(&text)),
            ForwardMsg::Host(text) => edit(panel, &|edit| edit.host.clone_from(&text)),
            ForwardMsg::TargetPort(text) => edit(panel, &|edit| edit.target_port.clone_from(&text)),
            ForwardMsg::Name(text) => edit(panel, &|edit| edit.name.clone_from(&text)),
            ForwardMsg::AutoStart(on) => edit(panel, &|edit| edit.auto_start = on),
            ForwardMsg::Cancel => {
                panel.edit = None;
                Vec::new()
            }
            ForwardMsg::Save => {
                let Some(editing) = &mut panel.edit else {
                    return Vec::new();
                };
                let Some(forward) = editing.draft() else {
                    editing.error = Some(error_text("forward_invalid"));
                    return Vec::new();
                };
                let request = SaveForwardRequest {
                    connection_id: id,
                    id: editing.id,
                    forward,
                };
                vec![run(Then::ForwardDone, Command::SaveForward(request))]
            }
            ForwardMsg::Delete(forward) => {
                vec![run(
                    Then::ForwardDone,
                    Command::DeleteForward { id: forward },
                )]
            }
            ForwardMsg::Start(forward) => {
                vec![run(
                    Then::ForwardDone,
                    Command::StartForward { id: forward },
                )]
            }
            ForwardMsg::Stop(forward) => {
                vec![run(Then::ForwardDone, Command::StopForward { id: forward })]
            }
        }
    }

    /// The engine answered a command of the panel.
    pub(super) fn forward_answer(
        &mut self,
        then: Then,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        let panel = &mut self.forwards;
        match (then, result) {
            (Then::ForwardsLan, Ok(Outcome::Settings { settings })) => {
                panel.lan = settings.iter().any(|setting| {
                    setting.key == rc_engine::settings::FORWARDS_LAN_KEY
                        && setting.value == serde_json::Value::Bool(true)
                });
            }
            (Then::ForwardsLan, _) => panel.lan = false,
            (_, Ok(Outcome::Forwards(outcome))) => match *outcome {
                ForwardsOutcome::Listed { forwards } => {
                    // A list for a connection the form no longer shows is left out.
                    let shown = panel.connection;
                    panel.forwards = forwards
                        .into_iter()
                        .filter(|view| Some(view.forward.connection_id) == shown)
                        .collect();
                }
                ForwardsOutcome::Forward { .. } if then == Then::ForwardDone => {
                    panel.edit = None;
                }
                _ => {}
            },
            (_, Ok(_)) => {}
            (_, Err(code)) => match &mut panel.edit {
                Some(edit) => edit.error = Some(error_text(code)),
                None => self.note = Some((error_text(code), true)),
            },
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "forwards_tests.rs"]
mod tests;
