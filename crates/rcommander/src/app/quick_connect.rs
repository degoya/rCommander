//! The quick connection dialog (Ctrl+N, Total Commander's "New connection"): host, port, user,
//! a password or a key file with its passphrase, and "Save as connection" with a name.
//!
//! The typed secrets are [`SecretString`]s (wiped on drop, never in `Debug`) and leave the dialog
//! only in `connections.quick_connect`. Unsaved, the engine keeps them in memory for this run.
//! Saving stores them in the vault: only then, and only while the vault is not unlocked, the
//! dialog asks for the master password too (unlock, or create the vault before the first
//! secret, typed twice). An unknown host key is the host-key dialog's question; the connection fails with
//! `host_key_unknown` meanwhile and can be confirmed again once the key is trusted.

mod view;

use iced::Task;
use rc_engine::{Chord, Command, PanelId, QuickConnectRequest, SecretParam, VaultStateKind};
use rc_vault::{ExposeSecret as _, SecretString};

pub(super) use self::view::{HOST_FIELD, body, overlay};
use super::{
    App, Effect, Message,
    vault_ask::{Ask, Unsent, vault_command},
};
use crate::i18n::{error_text, t};

/// Where the dialog stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickPhase {
    Editing,
    Connecting,
    Failed(String),
}

/// The open dialog.
#[derive(Debug)]
pub struct QuickDialog {
    /// The panel that shows the server.
    pub panel: PanelId,
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: SecretString,
    pub key_path: String,
    pub passphrase: SecretString,
    pub save: bool,
    pub name: String,
    /// What the vault needs before a save can store a secret; `None` while it needs nothing.
    pub vault: Option<VaultStateKind>,
    pub master: SecretString,
    /// The new master password again, while the vault is created (RC-080-39).
    pub master_repeat: SecretString,
    pub phase: QuickPhase,
}

impl QuickDialog {
    fn new(panel: PanelId) -> Self {
        Self {
            panel,
            host: String::new(),
            port: String::new(),
            user: String::new(),
            password: SecretString::default(),
            key_path: String::new(),
            passphrase: SecretString::default(),
            save: false,
            name: String::new(),
            vault: None,
            master: SecretString::default(),
            master_repeat: SecretString::default(),
            phase: QuickPhase::Editing,
        }
    }

    /// Whether a save stores a secret.
    fn stores_secret(&self) -> bool {
        self.save
            && !(self.password.expose_secret().is_empty()
                && self.passphrase.expose_secret().is_empty())
    }
}

/// A typed secret as a command parameter; `None` when nothing was typed.
fn param(secret: &SecretString) -> Option<SecretParam> {
    let typed = secret.expose_secret();
    (!typed.is_empty()).then(|| SecretParam::from(typed))
}

/// What the dialog's controls say.
#[derive(Debug, Clone)]
pub enum QuickMsg {
    Host(String),
    Port(String),
    User(String),
    Password(SecretString),
    KeyPath(String),
    Passphrase(SecretString),
    Save(bool),
    Name(String),
    Master(SecretString),
    MasterRepeat(SecretString),
    Confirm,
    Close,
    /// The engine's answer: done, or the code of the first failure.
    Finished(Result<(), &'static str>),
}

impl App {
    /// Ctrl+N: the dialog opens for the active panel.
    pub(super) fn open_quick_connect(&mut self) {
        self.quick = Some(QuickDialog::new(self.active));
    }

    /// A message of the dialog.
    pub(super) fn on_quick_connect(&mut self, message: QuickMsg) -> Vec<Effect> {
        match message {
            QuickMsg::Confirm => return self.confirm_quick(),
            QuickMsg::Close => self.quick = None,
            QuickMsg::Finished(Ok(())) => self.quick = None,
            QuickMsg::Finished(Err(code)) => {
                if let Some(dialog) = self.quick.as_mut() {
                    dialog.phase = QuickPhase::Failed(error_text(code));
                }
            }
            edit => {
                let Some(dialog) = self
                    .quick
                    .as_mut()
                    .filter(|dialog| dialog.phase != QuickPhase::Connecting)
                else {
                    return Vec::new();
                };
                dialog.phase = QuickPhase::Editing;
                match edit {
                    QuickMsg::Host(host) => dialog.host = host,
                    QuickMsg::Port(port) => dialog.port = port,
                    QuickMsg::User(user) => dialog.user = user,
                    QuickMsg::Password(password) => dialog.password = password,
                    QuickMsg::KeyPath(path) => dialog.key_path = path,
                    QuickMsg::Passphrase(passphrase) => dialog.passphrase = passphrase,
                    QuickMsg::Save(save) => dialog.save = save,
                    QuickMsg::Name(name) => dialog.name = name,
                    QuickMsg::Master(master) => dialog.master = master,
                    QuickMsg::MasterRepeat(repeat) => dialog.master_repeat = repeat,
                    _ => {}
                }
            }
        }
        Vec::new()
    }

    /// A key while the dialog is open: Enter connects, Escape closes, Tab and Shift+Tab walk the
    /// fields; the fields take the rest.
    pub(super) fn quick_connect_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if let Some(effects) = super::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        match chord.key.as_str() {
            "Escape" => self.quick = None,
            "Enter" => return self.confirm_quick(),
            _ => {}
        }
        Vec::new()
    }

    /// Connects: unlocks or creates the vault first when a save stores a secret, then
    /// `connections.quick_connect`, as one job off the window's thread.
    fn confirm_quick(&mut self) -> Vec<Effect> {
        let vault_state = self
            .engine
            .vault()
            .map_or(VaultStateKind::Absent, |vault| vault.status().state);
        let Some(dialog) = self
            .quick
            .as_mut()
            .filter(|dialog| dialog.phase != QuickPhase::Connecting)
        else {
            return Vec::new();
        };
        let port = match dialog.port.trim() {
            "" => None,
            // Port 0 is no port, as the connection form says (RC-080-53).
            typed => match typed.parse::<u16>() {
                Ok(port) if port != 0 => Some(port),
                _ => {
                    dialog.phase = QuickPhase::Failed(error_text("port_invalid"));
                    return Vec::new();
                }
            },
        };
        let mut commands = Vec::with_capacity(2);
        if dialog.stores_secret()
            && let Some(ask) = Ask::for_state(vault_state)
        {
            // The master password is asked for only now, and only once it is needed; asked again
            // when the vault changed meanwhile, so a new one is always typed twice.
            if dialog.vault != Some(vault_state) || dialog.master.expose_secret().is_empty() {
                dialog.vault = Some(vault_state);
                return Vec::new();
            }
            match vault_command(ask, &dialog.master, &dialog.master_repeat) {
                Ok(command) => commands.push(command),
                // Nothing typed was asked again above: only the repetition can differ.
                Err(Unsent::Empty | Unsent::Mismatch) => {
                    dialog.phase = QuickPhase::Failed(t!("connections.vault.mismatch").to_owned());
                    return Vec::new();
                }
            }
        }
        commands.push(Command::QuickConnect(QuickConnectRequest {
            panel: Some(dialog.panel),
            host: dialog.host.clone(),
            port,
            user: dialog.user.clone(),
            password: param(&dialog.password),
            key_path: Some(dialog.key_path.clone()).filter(|path| !path.trim().is_empty()),
            passphrase: param(&dialog.passphrase),
            save_as: dialog.save.then(|| dialog.name.clone()),
        }));
        dialog.master = SecretString::default();
        dialog.master_repeat = SecretString::default();
        dialog.phase = QuickPhase::Connecting;
        let engine = self.engine.clone();
        let job = Task::perform(
            async move { super::run_in_order(&engine, commands).await },
            |result| Message::QuickConnect(QuickMsg::Finished(result)),
        );
        vec![Effect::Task(job)]
    }
}

#[cfg(test)]
#[path = "quick_connect_tests.rs"]
mod tests;
