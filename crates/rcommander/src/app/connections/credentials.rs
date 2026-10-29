//! Shared credentials in the manager (RC-050-12): a new one from the tree's button, its form
//! saved in one `credentials.save`, and the setting `vault.shared_credentials` that offers them.
//!
//! An entry's form is the connection's form with fewer fields ([`Form::new_credential`]): name,
//! user and secrets. Saving it asks for the master password first when it stores a secret and
//! the vault is not open, as a connection's does.

use rc_engine::{Command, Outcome, SHARED_CREDENTIALS_KEY, TreeItem};

use super::{
    ConnectionManager, Focus,
    form::{Field, Form},
    jobs::Then,
    keys::focus_id,
    run,
    vault::{AfterVault, Ask},
};
use crate::{
    app::Effect,
    i18n::{error_text, t},
};

/// Reads whether shared credentials are offered.
pub(super) fn read_setting() -> Effect {
    run(
        Then::SharedSetting,
        Command::GetSettings {
            key: Some(SHARED_CREDENTIALS_KEY.to_owned()),
        },
    )
}

impl ConnectionManager {
    /// The button: an empty credential's form for the folder at the cursor.
    pub(super) fn new_credential(&mut self) -> Vec<Effect> {
        if !self.tree.offer_credentials {
            return Vec::new();
        }
        let folder = self.tree.target_folder(self.tree.current());
        self.form = Some(Form::new_credential(folder));
        self.pending = None;
        self.focus = Focus::Field(Field::Name);
        vec![focus_id(Field::Name.id())]
    }

    /// Saves a credential's form; a secret to store waits for an open vault.
    pub(super) fn save_credential(&mut self) -> Vec<Effect> {
        let Some(form) = self.form.as_mut() else {
            return Vec::new();
        };
        let plan = match form.credential_plan() {
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
        vec![run(Then::CredentialSaved, Command::SaveCredential(plan))]
    }

    /// The answers about shared credentials: a renamed or moved one, a saved form, the setting.
    pub(super) fn credential_answer(
        &mut self,
        then: Then,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        match (then, result) {
            (Then::Select, Ok(Outcome::Credential { credential })) => {
                self.tree.cursor = Some(TreeItem::Credential(credential.id));
                self.refresh_form();
            }
            (Then::CredentialSaved, Ok(Outcome::Credential { credential })) => {
                self.busy = false;
                if let Some(form) = &mut self.form {
                    form.saved();
                }
                self.tree.cursor = Some(TreeItem::Credential(credential.id));
                self.note = Some((t!("connections.saved").to_owned(), false));
                return vec![run(Then::Tree, Command::ListConnections)];
            }
            (Then::CredentialSaved, Err(code)) => {
                self.busy = false;
                return self.save_failed(code);
            }
            (Then::SharedSetting, Ok(Outcome::Settings { settings })) => {
                let on = settings
                    .iter()
                    .find(|setting| setting.key == SHARED_CREDENTIALS_KEY)
                    .and_then(|setting| setting.value.as_bool());
                if let Some(on) = on {
                    self.tree.offer_credentials = on;
                    // A credential's form goes with its row; a connection's offers them or not.
                    self.refresh_form();
                }
            }
            (_, Err(code)) => self.note = Some((error_text(code), true)),
            (_, Ok(_)) => {}
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
