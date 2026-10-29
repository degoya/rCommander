//! The `login_scripts.*` commands: login scripts in `rcommander.db` and the script each
//! connection runs.
//!
//! A script is checked by rc-remote (bounds, patterns) before the repository writes it, and
//! read back through the same check. Storing a script's own secret needs the vault unlocked,
//! like a connection's (`vault_absent`, `vault_locked`); removing one does not, and the last
//! secret gone announces [`EngineEvent::VaultUnused`]. Through MCP no `send_secret` step is
//! added, changed or moved, nor the steps before it, and no script with one is assigned
//! (`Engine::refuse_mcp_secret_steps`).

use rc_db::login_scripts::{LoginScriptFields, SCRIPT_SECRET_COLUMN, SCRIPT_SECRETS_TABLE};
use rc_remote::login_script::{
    DEFAULT_STEP_TIMEOUT_MS, DEFAULT_TOTAL_TIMEOUT_MS, SCHEMA_VERSION, steps_to_json,
};
use rc_vault::{ExposeSecret as _, FieldRef};

use crate::{
    Command, ConnectionId, Engine, EngineError, EngineEvent, LoginScript, LoginScriptDraft,
    LoginScriptId, LoginScriptInfo, LoginScriptPatch, Outcome, SecretParam, Step, TreeChange,
    TreeItem,
};

impl Engine {
    /// `login_scripts.list`: every script, by name.
    pub(crate) async fn list_login_scripts(&self) -> Result<Outcome, EngineError> {
        let scripts: Vec<LoginScriptInfo> = self
            .db()?
            .login_scripts()
            .await?
            .into_iter()
            .map(LoginScriptInfo::from_record)
            .collect::<Result<_, _>>()?;
        Ok(Outcome::LoginScripts { scripts })
    }

    /// `login_scripts.get`.
    pub(crate) async fn get_login_script(&self, id: LoginScriptId) -> Result<Outcome, EngineError> {
        Ok(Outcome::LoginScript {
            script: self.login_script(id).await?,
        })
    }

    /// `login_scripts.create`.
    pub(crate) async fn create_login_script(
        &self,
        draft: &LoginScriptDraft,
    ) -> Result<Outcome, EngineError> {
        let script = LoginScript {
            steps: draft.steps.clone(),
            step_timeout_ms: draft.step_timeout_ms.unwrap_or(DEFAULT_STEP_TIMEOUT_MS),
            total_timeout_ms: draft.total_timeout_ms.unwrap_or(DEFAULT_TOTAL_TIMEOUT_MS),
        };
        script.validate()?;
        let steps = steps_to_json(&script.steps)?;
        let record = self
            .db()?
            .create_login_script(&fields(&draft.name, &script, &steps))
            .await?;
        let script = LoginScriptInfo::from_record(record)?;
        self.announce_script(script.id, TreeChange::Created);
        Ok(Outcome::LoginScript { script })
    }

    /// `login_scripts.update`: the fields `patch` names; `steps` replaces every step.
    pub(crate) async fn update_login_script(
        &self,
        id: LoginScriptId,
        patch: &LoginScriptPatch,
    ) -> Result<Outcome, EngineError> {
        let current = self.login_script(id).await?;
        let script = LoginScript {
            steps: patch.steps.clone().unwrap_or(current.steps),
            step_timeout_ms: patch.step_timeout_ms.unwrap_or(current.step_timeout_ms),
            total_timeout_ms: patch.total_timeout_ms.unwrap_or(current.total_timeout_ms),
        };
        script.validate()?;
        let steps = steps_to_json(&script.steps)?;
        let name = patch.name.as_deref().unwrap_or(&current.name);
        let record = {
            // A step that sends a secret makes the script's connections hold one.
            let _writes = self.inner.secret_writes.lock().await;
            self.db()?
                .update_login_script(id, &fields(name, &script, &steps))
                .await?
        };
        let script = LoginScriptInfo::from_record(record)?;
        self.announce_script(id, TreeChange::Updated);
        Ok(Outcome::LoginScript { script })
    }

    /// `login_scripts.delete`: with its secrets; its connections run none.
    pub(crate) async fn delete_login_script(
        &self,
        id: LoginScriptId,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let connections = db.login_script(id).await?.connections;
        let deletion = {
            let _writes = self.inner.secret_writes.lock().await;
            db.delete_login_script(id).await?
        };
        self.announce_script(id, TreeChange::Deleted);
        for connection in connections {
            self.announce(TreeItem::Connection(connection), TreeChange::Updated);
        }
        if deletion.secrets > 0 {
            self.announce_if_vault_unused().await;
        }
        Ok(Outcome::LoginScriptDeleted { id, deletion })
    }

    /// `login_scripts.assign`: the script `connection` runs after it logged in, or none.
    pub(crate) async fn assign_login_script(
        &self,
        connection: ConnectionId,
        script: Option<LoginScriptId>,
    ) -> Result<Outcome, EngineError> {
        let connection = self
            .db()?
            .set_connection_login_script(connection, script)
            .await?;
        self.announce(TreeItem::Connection(connection.id), TreeChange::Updated);
        if let Some(script) = script {
            self.announce_script(script, TreeChange::Updated);
        }
        Ok(Outcome::Connection { connection })
    }

    /// `login_scripts.set_secret`: stored under `label`, encrypted for its row.
    pub(crate) async fn set_login_script_secret(
        &self,
        script: LoginScriptId,
        label: &str,
        value: &SecretParam,
    ) -> Result<Outcome, EngineError> {
        let secret = value.secret();
        if secret.expose_secret().trim().is_empty() {
            return Err(EngineError::SecretEmpty);
        }
        let stored = {
            let _writes = self.inner.secret_writes.lock().await;
            let vault = self.vault_handle().await?;
            self.db()?
                .store_login_script_secret(script, label, |row| {
                    let field = FieldRef::new(SCRIPT_SECRETS_TABLE, SCRIPT_SECRET_COLUMN, row);
                    vault.encrypt_str(&field, secret)
                })
                .await
                .map_err(|error| self.secret_write_failed(error.into()))??
        };
        self.remember_secret(secret);
        tracing::debug!(%script, secret = stored.id, "login script secret stored");
        self.announce_script(script, TreeChange::Updated);
        self.get_login_script(script).await
    }

    /// `login_scripts.clear_secret`.
    pub(crate) async fn clear_login_script_secret(
        &self,
        script: LoginScriptId,
        id: i64,
    ) -> Result<Outcome, EngineError> {
        {
            let _writes = self.inner.secret_writes.lock().await;
            self.db()?.delete_login_script_secret(script, id).await?;
        }
        tracing::debug!(%script, secret = id, "login script secret removed");
        self.announce_script(script, TreeChange::Updated);
        self.announce_if_vault_unused().await;
        self.get_login_script(script).await
    }

    /// Refuses what MCP may not do to a script even with a confirmation: make a secret answer a
    /// prompt an agent chose. A `send_secret` step, the steps that lead to it and the script it
    /// is in reaching a connection are the person's. `gui_only` for a new script with a
    /// `send_secret` step, for new steps that change anything up to the last `send_secret`
    /// step or add one, and for assigning a script that has one - and for a `connections.save`
    /// that changes a secret, assigns such a script or points at a shared credential, and a
    /// `credentials.save` that changes a secret or the user (RC-050-12).
    pub(crate) async fn refuse_mcp_secret_steps(
        &self,
        command: &Command,
    ) -> Result<(), EngineError> {
        let refused = match command {
            Command::CreateLoginScript { script } => sends_secret(&script.steps),
            Command::UpdateLoginScript {
                id,
                patch:
                    LoginScriptPatch {
                        steps: Some(steps), ..
                    },
            } => !keeps_secret_prefix(&self.login_script(*id).await?.steps, steps),
            Command::AssignLoginScript {
                script_id: Some(id),
                ..
            } => sends_secret(&self.login_script(*id).await?.steps),
            // A connection's secrets are the person's to type (RC-080-25).
            // So is pointing a connection at a shared credential, or filling one (RC-050-12).
            Command::SaveConnection(save) => {
                !save.secrets.is_empty()
                    || matches!(save.credential, Some(Some(_)))
                    || match save.login_script {
                        Some(Some(id)) => sends_secret(&self.login_script(id).await?.steps),
                        _ => false,
                    }
            }
            Command::SaveCredential(save) => super::credential_needs_gui(save),
            _ => false,
        };
        if refused {
            return Err(EngineError::GuiOnly {
                command: command.id(),
            });
        }
        Ok(())
    }

    /// Whether the login script `id` sends a secret; one that cannot be read counts as one.
    pub(crate) async fn script_sends_secret(&self, id: LoginScriptId) -> bool {
        match self.login_script(id).await {
            Ok(script) => sends_secret(&script.steps),
            Err(_) => true,
        }
    }

    /// The script `id`, its steps read again.
    async fn login_script(&self, id: LoginScriptId) -> Result<LoginScriptInfo, EngineError> {
        let record = self.db()?.login_script(id).await?;
        Ok(LoginScriptInfo::from_record(record)?)
    }

    fn announce_script(&self, script: LoginScriptId, change: TreeChange) {
        self.publish(EngineEvent::LoginScript { script, change });
    }
}

fn sends_secret(steps: &[Step]) -> bool {
    steps
        .iter()
        .any(|step| matches!(step, Step::SendSecret { .. }))
}

/// Whether `new` keeps every step of `old` up to and including its last `send_secret` step, in
/// place, and adds no `send_secret` step after them.
fn keeps_secret_prefix(old: &[Step], new: &[Step]) -> bool {
    let kept = old
        .iter()
        .rposition(|step| matches!(step, Step::SendSecret { .. }))
        .map_or(0, |last| last + 1);
    new.get(..kept) == Some(&old[..kept]) && !sends_secret(&new[kept..])
}

/// The columns of `script` named `name`, its steps as `steps`.
fn fields<'a>(name: &'a str, script: &LoginScript, steps: &'a str) -> LoginScriptFields<'a> {
    LoginScriptFields {
        name,
        schema_version: SCHEMA_VERSION,
        steps,
        step_timeout_ms: script.step_timeout_ms,
        total_timeout_ms: script.total_timeout_ms,
    }
}

#[cfg(test)]
#[path = "login_scripts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "login_scripts_secret_tests.rs"]
mod secret_tests;
