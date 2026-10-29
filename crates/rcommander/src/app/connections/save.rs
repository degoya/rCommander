//! The form's messages and saving it in one command (`connections.save`): the fields, the
//! secrets - after the master-password prompt when the vault is not open - the login script, the
//! shared credential and the mark, all stored or none. A shared credential's form saves through
//! `credentials.rs`.

use rc_engine::{Command, ConnectionId, Outcome, SecretKind, TreeItem, VaultStateKind};

use super::{
    ConnJob, ConnMsg, ConnectionManager, Focus,
    form::Field,
    jobs::Then,
    run,
    vault::{AfterVault, Ask, VaultPrompt},
};
use crate::{
    app::Effect,
    i18n::{error_text, t},
};

impl ConnectionManager {
    /// A message of the form or of the master-password prompt.
    pub(super) fn form_message(&mut self, message: ConnMsg) -> Vec<Effect> {
        if let Some(effects) = self.prompt_message(&message) {
            return effects;
        }
        let Some(form) = self.form.as_mut() else {
            return Vec::new();
        };
        let field = match message {
            ConnMsg::Edit(edit) => form.edit(edit),
            ConnMsg::Secret(kind, value) => {
                form.secret(kind, value);
                if kind == SecretKind::Passphrase {
                    Field::Passphrase
                } else {
                    Field::Password
                }
            }
            ConnMsg::Kind(kind) => {
                form.set_kind(kind);
                Field::Kind
            }
            ConnMsg::Auth(auth) => {
                form.set_auth(auth);
                Field::Auth
            }
            ConnMsg::View(view) => {
                form.set_view(view);
                Field::View
            }
            ConnMsg::Script(script) => {
                form.set_script(script);
                Field::Script
            }
            ConnMsg::Shared(shared) => {
                form.set_shared(shared);
                Field::Credential
            }
            ConnMsg::Toggle(field) => {
                form.toggle(field);
                field
            }
            ConnMsg::Reveal(reveal) => {
                // Typed secrets show in clear only while the vault is not locked.
                form.reveal = reveal && self.vault != Some(VaultStateKind::Locked);
                return Vec::new();
            }
            ConnMsg::Save => return self.save(),
            _ => return Vec::new(),
        };
        self.focus = Focus::Field(field);
        Vec::new()
    }

    /// Saves the form: invalid fields stop it at the field; secrets wait for an open vault.
    pub(super) fn save(&mut self) -> Vec<Effect> {
        if self.busy {
            return Vec::new();
        }
        let Some(form) = self.form.as_mut() else {
            return Vec::new();
        };
        if form.is_credential() {
            return self.save_credential();
        }
        let plan = match form.plan() {
            Ok(plan) => plan,
            Err(invalid) => {
                form.errors = invalid
                    .into_iter()
                    .map(|(field, code)| (field, error_text(code)))
                    .collect();
                self.note = Some((t!("connections.invalid").to_owned(), true));
                return Vec::new();
            }
        };
        form.errors.clear();
        if plan.stores_secrets()
            && let Some(ask) = self.vault.and_then(Ask::for_state)
        {
            return self.ask_vault(ask, AfterVault::Save);
        }
        self.busy = true;
        self.note = None;
        vec![Effect::Connections(ConnJob::Save(Box::new(plan)))]
    }

    pub(super) fn saved(&mut self, result: Result<ConnectionId, &'static str>) -> Vec<Effect> {
        self.busy = false;
        match result {
            Ok(id) => {
                if let Some(form) = &mut self.form {
                    form.saved();
                }
                self.tree.cursor = Some(TreeItem::Connection(id));
                self.note = Some((t!("connections.saved").to_owned(), false));
                // The tree's events bring the stored state; read it in any case.
                vec![run(Then::Tree, Command::ListConnections)]
            }
            Err(code) => self.save_failed(code),
        }
    }

    /// A refused save: nothing was stored, the message stands at its field.
    pub(super) fn save_failed(&mut self, code: &'static str) -> Vec<Effect> {
        if let Some(ask) = Ask::for_code(code) {
            self.vault = Some(ask.state());
            return self.ask_vault(ask, AfterVault::Save);
        }
        let text = error_text(code);
        let field = self.form.as_ref().and_then(|form| form.field_of(code));
        match (field, self.form.as_mut()) {
            (Some(field), Some(form)) => {
                form.errors.retain(|(at, _)| *at != field);
                form.errors.push((field, text));
                self.note = Some((t!("connections.invalid").to_owned(), true));
            }
            _ => self.note = Some((text, true)),
        }
        Vec::new()
    }

    /// Opens the master-password prompt; `after` runs once the vault is open.
    pub(super) fn ask_vault(&mut self, ask: Ask, after: AfterVault) -> Vec<Effect> {
        self.prompt = Some(VaultPrompt::new(ask, after));
        self.focus = Focus::Tree;
        vec![super::keys::focus_id(super::view::VAULT_FIELD)]
    }

    /// The prompt's own messages; `None` for the form's.
    fn prompt_message(&mut self, message: &ConnMsg) -> Option<Vec<Effect>> {
        match message {
            ConnMsg::VaultInput(repeat, value) => {
                if let Some(prompt) = self.prompt.as_mut().filter(|prompt| !prompt.busy) {
                    if *repeat {
                        prompt.repeat = value.clone();
                    } else {
                        prompt.password = value.clone();
                    }
                    prompt.error = None;
                }
                Some(Vec::new())
            }
            ConnMsg::VaultSubmit => {
                let Some(prompt) = self.prompt.as_mut().filter(|prompt| !prompt.busy) else {
                    return Some(Vec::new());
                };
                Some(match prompt.command() {
                    Ok(command) => {
                        prompt.busy = true;
                        vec![run(Then::VaultOpened, command)]
                    }
                    Err(text) => {
                        prompt.error = Some(text.to_owned());
                        Vec::new()
                    }
                })
            }
            ConnMsg::VaultCancel => {
                self.prompt = None;
                self.close = self.prompt_only;
                Some(Vec::new())
            }
            _ => None,
        }
    }

    /// The vault opened, or the password was refused.
    pub(super) fn vault_opened(&mut self, result: Result<Outcome, &'static str>) -> Vec<Effect> {
        let Some(prompt) = self.prompt.as_mut() else {
            return Vec::new();
        };
        prompt.busy = false;
        match result {
            Ok(outcome) => {
                self.vault = Some(match outcome {
                    Outcome::Vault(status) => status.state,
                    _ => VaultStateKind::Unlocked,
                });
                let after = prompt.after.clone();
                self.prompt = None;
                match after {
                    AfterVault::Save => self.save(),
                    AfterVault::TakeKeys(keys) => {
                        vec![Effect::Connections(ConnJob::TakeKeys(keys))]
                    }
                    AfterVault::TcPasswords(command) => {
                        vec![run(Then::TcPasswords, *command)]
                    }
                    AfterVault::Nothing => {
                        self.close = self.prompt_only;
                        self.note = Some((t!("connections.vault.unlocked").to_owned(), false));
                        Vec::new()
                    }
                    // The connection or transfer the locked vault refused runs again, once.
                    AfterVault::Retry(retry) => {
                        self.close = self.prompt_only;
                        self.note = Some((t!("connections.vault.unlocked").to_owned(), false));
                        vec![retry.effect()]
                    }
                }
            }
            Err(code) => {
                // Created meanwhile, elsewhere: it is unlocked instead.
                if code == "vault_exists" {
                    prompt.ask = Ask::Unlock;
                }
                prompt.error = Some(error_text(code));
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
