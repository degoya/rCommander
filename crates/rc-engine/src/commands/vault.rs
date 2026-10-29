//! The `vault.*` commands and the idle-lock timer.
//!
//! The engine holds the vault of its database once [`Engine::attach_vault`] gave it one, or opens
//! it on the attached database when a command first needs it; an engine with neither reports
//! `absent`. The master password is set by `vault.create` when the first secret is stored and
//! typed into `vault.unlock` when a connection needs one - both `GuiOnly`, like changing it and
//! removing the vault: MCP sees `vault.status` and `vault.lock` and nothing more.
//!
//! The vault locks by itself after the idle lock or the longest time unlocked (the timer here),
//! and when the session locks or the machine suspends (`session_lock.rs`). Another process that
//! changed the master password makes this one's key stale: the database refuses a secret written
//! with it, and the engine locks the vault and answers `vault_changed` (RC-080-11).

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use rc_db::{DbError, connections::ConnectionError, login_scripts::LoginScriptError};
use rc_vault::{Vault, VaultError, VaultStatus};

use crate::{Engine, EngineError, EngineEvent, Inner, Outcome, SecretParam, session_lock};

/// The longest the timer sleeps between two looks at the vault: a vault unlocked meanwhile, or
/// a changed idle setting, is noticed within this.
const IDLE_POLL: Duration = Duration::from_secs(30);

impl Engine {
    /// Gives the engine the vault of its database and starts the idle-lock timer and the
    /// listener for the session lock on the current Tokio runtime (none outside one: then only a
    /// use after the deadline locks). Answers `false`, changing nothing, when a vault is attached
    /// already.
    pub fn attach_vault(&self, vault: Vault) -> bool {
        if self.inner.vault.set(vault).is_err() {
            return false;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(idle_timer(Arc::downgrade(&self.inner)));
            runtime.spawn(session_lock::listen(
                Arc::downgrade(&self.inner),
                session_lock::subscribe(),
            ));
        }
        true
    }

    /// The attached vault, for the GUI that unlocks it and the code that reads secrets.
    #[must_use]
    pub fn vault(&self) -> Option<Vault> {
        self.inner.vault.get().cloned()
    }

    /// `vault.status`.
    pub(crate) fn vault_status(&self) -> Outcome {
        Outcome::Vault(
            self.inner
                .vault
                .get()
                .map_or_else(VaultStatus::absent, Vault::status),
        )
    }

    /// `vault.lock`: drops the key; announces it when there was one. The fingerprints of the
    /// secrets seen stay: a secret on a terminal screen stays hidden from MCP.
    pub(crate) fn lock_vault(&self) -> Outcome {
        if self.inner.vault.get().is_some_and(Vault::lock) {
            self.publish(EngineEvent::VaultLocked);
        }
        self.vault_status()
    }

    /// The attached vault, or the vault of the attached database, opened and attached now.
    pub(crate) async fn vault_handle(&self) -> Result<Vault, EngineError> {
        if let Some(vault) = self.vault() {
            return Ok(vault);
        }
        let opened = Vault::open(self.db()?.clone()).await?;
        // Another call may have attached one meanwhile; the first one stays.
        self.attach_vault(opened);
        self.vault().ok_or(EngineError::DatabaseUnavailable)
    }

    /// `vault.create`: the master password, set when the first secret is about to be stored.
    pub(crate) async fn create_vault(
        &self,
        password: &SecretParam,
    ) -> Result<Outcome, EngineError> {
        let password = password.master_password()?;
        self.vault_handle().await?.create(&password).await?;
        self.publish(EngineEvent::VaultUnlocked);
        Ok(self.vault_status())
    }

    /// `vault.unlock`: the master password, typed when a connection needs a stored secret.
    pub(crate) async fn unlock_vault(
        &self,
        password: &SecretParam,
    ) -> Result<Outcome, EngineError> {
        let password = rc_vault::MasterPassword::from(password.secret().clone());
        self.vault_handle().await?.unlock(&password).await?;
        self.publish(EngineEvent::VaultUnlocked);
        Ok(self.vault_status())
    }

    /// `vault.change_password`: every stored secret re-encrypted under the new password.
    pub(crate) async fn change_master_password(
        &self,
        current: &SecretParam,
        new: &SecretParam,
    ) -> Result<Outcome, EngineError> {
        let new = new.master_password()?;
        let current = rc_vault::MasterPassword::from(current.secret().clone());
        let _writes = self.inner.secret_writes.lock().await;
        self.vault_handle()
            .await?
            .change_password(&current, &new)
            .await?;
        Ok(self.vault_status())
    }

    /// `vault.remove`: the vault goes once no secret is stored; no master password is asked
    /// any more.
    pub(crate) async fn remove_vault(&self) -> Result<Outcome, EngineError> {
        let _writes = self.inner.secret_writes.lock().await;
        self.vault_handle().await?.remove().await?;
        self.publish(EngineEvent::VaultRemoved);
        Ok(self.vault_status())
    }

    /// What a failed secret write answers. When the database refused it because another
    /// process changed the master password since this one unlocked ([`DbError::VaultChanged`]),
    /// the stale key goes - the vault locks and says so - and the answer is `vault_changed`:
    /// nothing was written, and the next unlock takes the new password. Any other error stays.
    pub(crate) fn secret_write_failed(&self, error: EngineError) -> EngineError {
        let changed = matches!(
            &error,
            EngineError::Database(DbError::VaultChanged)
                | EngineError::Connection(ConnectionError::Db(DbError::VaultChanged))
                | EngineError::LoginScript(LoginScriptError::Db(DbError::VaultChanged))
        );
        if !changed {
            return error;
        }
        tracing::warn!("the master password was changed by another rCommander; vault locked");
        self.lock_vault();
        EngineError::Vault(VaultError::Changed)
    }
}

/// Locks the vault when its idle lock falls due, for as long as the engine lives. Holds the
/// engine only while looking, so dropping the last [`Engine`] ends it.
async fn idle_timer(inner: Weak<Inner>) {
    loop {
        let wait = {
            let Some(inner) = inner.upgrade() else {
                return;
            };
            let engine = Engine { inner };
            let Some(vault) = engine.inner.vault.get() else {
                return;
            };
            if vault.lock_if_idle() {
                engine.publish(EngineEvent::VaultLocked);
            }
            vault
                .idle_lock_in()
                .map_or(IDLE_POLL, |left| left.min(IDLE_POLL))
        };
        tokio::time::sleep(wait).await;
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
