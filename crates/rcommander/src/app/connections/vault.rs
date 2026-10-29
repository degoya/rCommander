//! The master-password prompt of RC-050-03, asked only when a secret is about to be stored or
//! read: "set a master password" when no vault exists yet (twice, with the warning that it
//! cannot be recovered; its length is the engine's to check, `rc_engine::MIN_MASTER_PASSWORD_LEN`
//! and `master_password_too_short`), "unlock" when one exists and is locked.
//! Nothing is stored before the prompt succeeds; Escape drops it and the save with it.
//!
//! A connection or a transfer from the window that a locked vault refuses asks for the unlock
//! as well: the engine announces `SecretNeeded`, the refused command comes back ([`Retry`]), and
//! whichever arrives first opens the prompt, which runs the command again once the vault is
//! open. Escape or a wrong password runs nothing. MCP never gets here: the engine announces
//! nothing for an agent's call, and its commands are no window jobs.

use rc_engine::{Command, PanelId, VaultStateKind};
use rc_vault::SecretString;

use super::{
    ConnectionManager,
    jobs::{KeyToTake, Then},
    run,
};
pub use crate::app::vault_ask::Ask;
use crate::{
    app::{
        App, Effect,
        file_ops::Submit,
        vault_ask::{Unsent, vault_command},
    },
    i18n::{error_text, t},
};

/// What runs once the vault is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterVault {
    /// The form is saved again.
    Save,
    /// Imported connections' key files go into the vault.
    TakeKeys(Vec<KeyToTake>),
    /// The passwords of the connections imported from Total Commander go into the vault.
    TcPasswords(Box<Command>),
    /// Nothing yet: a connection asked (`EngineEvent::SecretNeeded`) before its refused command
    /// came back, which turns this into [`AfterVault::Retry`].
    Nothing,
    /// The command a locked vault refused runs again.
    Retry(Retry),
}

/// A command a locked vault refused, to run again once the prompt opened the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Retry {
    /// A window job (`Message::VaultRefused`): the refused command and those after it, on
    /// behalf of this panel.
    Job(PanelId, Vec<Command>),
    /// The manager's Connect or Test, whose answer comes back as `Then`.
    Manager(Then, Box<Command>),
    /// A job for the operations queue (`Effect::Ops`): a copy or move to or from a server.
    Ops(Box<Submit>),
}

impl Retry {
    /// The command run again.
    pub fn effect(self) -> Effect {
        match self {
            Self::Job(panel, commands) => Effect::Dispatch(panel, commands),
            Self::Manager(then, command) => run(then, *command),
            Self::Ops(submit) => Effect::Ops(*submit),
        }
    }
}

/// The open prompt.
#[derive(Debug, Clone)]
pub struct VaultPrompt {
    pub ask: Ask,
    pub password: SecretString,
    /// The new password again (only when creating).
    pub repeat: SecretString,
    pub error: Option<String>,
    /// The engine is checking the password.
    pub busy: bool,
    pub after: AfterVault,
}

impl VaultPrompt {
    pub fn new(ask: Ask, after: AfterVault) -> Self {
        Self {
            ask,
            password: SecretString::from(""),
            repeat: SecretString::from(""),
            error: None,
            busy: false,
            after,
        }
    }

    /// The command that opens the vault, or why the typed passwords cannot be sent.
    pub fn command(&self) -> Result<Command, &'static str> {
        vault_command(self.ask, &self.password, &self.repeat).map_err(|unsent| match unsent {
            Unsent::Empty => t!("connections.vault.empty"),
            Unsent::Mismatch => t!("connections.vault.mismatch"),
        })
    }
}

impl ConnectionManager {
    /// A locked vault refused `retry`: the unlock prompt opens with it, or the open one that
    /// waits for a connection's command takes it; `None` when the open prompt runs something
    /// else already (the first refused command, a save) - then `retry` fails as before.
    pub(super) fn refused(&mut self, retry: Retry) -> Option<Vec<Effect>> {
        self.vault = Some(VaultStateKind::Locked);
        if self.prompt.is_none() {
            return Some(self.ask_vault(Ask::Unlock, AfterVault::Retry(retry)));
        }
        let prompt = self
            .prompt
            .as_mut()
            .filter(|prompt| prompt.after == AfterVault::Nothing)?;
        prompt.after = AfterVault::Retry(retry);
        Some(Vec::new())
    }
}

impl App {
    /// A connection from the window needs a stored secret and the vault is locked: the unlock
    /// prompt, in the open manager or alone. The engine sends this only for the window's own
    /// commands, never over MCP.
    pub(super) fn secret_needed(&mut self) -> Vec<Effect> {
        let manager = self.vault_prompt_host();
        manager.vault = Some(VaultStateKind::Locked);
        if manager.prompt.is_some() {
            return Vec::new();
        }
        manager.ask_vault(Ask::Unlock, AfterVault::Nothing)
    }

    /// A locked vault refused a window job: the unlock prompt runs `commands` again on behalf
    /// of `panel` once it opened the vault. One the open prompt cannot take is the panel's
    /// error, as any failed job.
    pub(in crate::app) fn vault_refused(
        &mut self,
        panel: PanelId,
        commands: Vec<Command>,
    ) -> Vec<Effect> {
        self.retry_after_unlock(panel, Retry::Job(panel, commands))
    }

    /// A locked vault refused a job for the operations queue: the unlock prompt submits it again
    /// once it opened the vault, as [`App::vault_refused`] runs a window job.
    pub(in crate::app) fn ops_refused(&mut self, submit: Submit) -> Vec<Effect> {
        self.retry_after_unlock(submit.panel, Retry::Ops(Box::new(submit)))
    }

    fn retry_after_unlock(&mut self, panel: PanelId, retry: Retry) -> Vec<Effect> {
        if let Some(effects) = self.vault_prompt_host().refused(retry) {
            return effects;
        }
        self.pane_mut(panel).error = Some(error_text("vault_locked"));
        Vec::new()
    }

    /// The open manager, or one opened for the prompt alone.
    fn vault_prompt_host(&mut self) -> &mut ConnectionManager {
        self.connections.get_or_insert_with(|| ConnectionManager {
            prompt_only: true,
            ..ConnectionManager::default()
        })
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
