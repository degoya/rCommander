//! [`Vault`]: the state machine over the database, the cipher and a clock.

use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use rc_db::{Db, SECRET_COLUMNS, SecretColumn, VaultMeta, VaultRemoval};

use crate::{
    Clock, FieldRef, MasterPassword, Result, SecretBytes, SecretString, SystemClock, Use,
    VaultError, VaultState, VaultStatus,
    aead::{self, VERIFIER_AAD, VERIFIER_PLAINTEXT},
    kdf::{self, KDF_NAME, KdfParams, VaultKey},
    state::{
        DEFAULT_IDLE_LOCK_MINUTES, DEFAULT_MAX_UNLOCKED_HOURS, IDLE_LOCK_KEY, MAX_UNLOCKED_KEY,
        Machine,
    },
};

/// The `vault_meta.version` this build writes.
pub(crate) const VAULT_VERSION: i64 = 1;

/// How a [`Vault`] runs. The defaults are the application's; tests pass a manual clock, cheap
/// KDF parameters and no failure delay.
#[derive(Clone)]
pub struct VaultOptions {
    /// Where the time comes from (idle lock).
    pub clock: Arc<dyn Clock>,
    /// The parameters of new keys: a new vault, a new master password. An existing vault is
    /// always opened with its stored parameters.
    pub kdf: KdfParams,
    /// The wait after the free failed attempts, doubling from there.
    pub failure_delay: Duration,
    /// The columns re-encrypted by a password change.
    pub secret_columns: &'static [SecretColumn],
}

impl Default for VaultOptions {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            kdf: KdfParams::DEFAULT,
            failure_delay: Duration::from_secs(1),
            secret_columns: SECRET_COLUMNS,
        }
    }
}

/// The vault of one database. Cheap to clone; clones share the state and the key.
#[derive(Clone)]
pub struct Vault {
    pub(crate) shared: Arc<Shared>,
}

pub(crate) struct Shared {
    pub(crate) db: Db,
    pub(crate) options: VaultOptions,
    machine: Mutex<Machine>,
    /// Serializes `create`, `unlock` and the password change; never held by a field operation.
    pub(crate) ops: tokio::sync::Mutex<()>,
    /// A fault for the atomicity test: the rewrite fails after this many values.
    #[cfg(test)]
    pub(crate) fail_rekey_after: Mutex<Option<usize>>,
}

impl Vault {
    /// The vault of `db` with the application's options: `Locked` when a vault row exists,
    /// `Absent` otherwise. Asks for nothing.
    ///
    /// # Errors
    ///
    /// The database cannot be read, or the idle-lock setting is not a number.
    pub async fn open(db: Db) -> Result<Self> {
        Self::open_with(db, VaultOptions::default()).await
    }

    /// [`Vault::open`] with other options.
    ///
    /// # Errors
    ///
    /// As [`Vault::open`], and the longest-unlocked setting is not a number.
    pub async fn open_with(db: Db, options: VaultOptions) -> Result<Self> {
        let exists = db.vault_meta().await?.is_some();
        let minutes = db
            .get::<u32>(IDLE_LOCK_KEY)
            .await?
            .unwrap_or(DEFAULT_IDLE_LOCK_MINUTES);
        let hours = db
            .get::<u32>(MAX_UNLOCKED_KEY)
            .await?
            .unwrap_or(DEFAULT_MAX_UNLOCKED_HOURS);
        let mut machine = Machine::new(exists, minutes);
        machine.set_max_unlocked_hours(hours);
        Ok(Self {
            shared: Arc::new(Shared {
                db,
                options,
                machine: Mutex::new(machine),
                ops: tokio::sync::Mutex::new(()),
                #[cfg(test)]
                fail_rekey_after: Mutex::new(None),
            }),
        })
    }

    pub(crate) fn machine(&self) -> MutexGuard<'_, Machine> {
        self.shared
            .machine
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn now(&self) -> Instant {
        self.shared.options.clock.now()
    }

    /// The state now; an idle lock that fell due is applied first.
    #[must_use]
    pub fn state(&self) -> VaultState {
        let now = self.now();
        let mut machine = self.machine();
        machine.lock_if_idle(now);
        machine.state()
    }

    /// What `vault.status` reports; an idle lock that fell due is applied first.
    #[must_use]
    pub fn status(&self) -> VaultStatus {
        let now = self.now();
        let mut machine = self.machine();
        machine.lock_if_idle(now);
        machine.status(now)
    }

    /// Creates the vault with `password` and leaves it unlocked. The only moment a master
    /// password comes into being: when the first secret is about to be stored.
    ///
    /// # Errors
    ///
    /// [`VaultError::EmptyPassword`], [`VaultError::Exists`] when the database has a vault
    /// already, or the database fails.
    pub async fn create(&self, password: &MasterPassword) -> Result<()> {
        if password.is_empty() {
            return Err(VaultError::EmptyPassword);
        }
        let _ops = self.shared.ops.lock().await;
        if self.shared.db.vault_meta().await?.is_some() {
            self.machine().exists();
            return Err(VaultError::Exists);
        }
        let (meta, key) = new_key(password, self.shared.options.kdf).await?;
        if !self.shared.db.insert_vault_meta(&meta).await? {
            self.machine().exists();
            return Err(VaultError::Exists);
        }
        self.shared.db.hold_vault_key(Some(meta.salt));
        let now = self.now();
        self.machine().unlocked(key, now);
        Ok(())
    }

    /// Unlocks with `password`, the parameters read from the vault's row. After a few failed
    /// attempts each further one waits, longer every time.
    ///
    /// # Errors
    ///
    /// [`VaultError::Absent`], [`VaultError::WrongPassword`] (for any wrong password, with no
    /// hint which part was wrong), [`VaultError::BadParameters`] for a damaged row, or the
    /// database fails.
    pub async fn unlock(&self, password: &MasterPassword) -> Result<()> {
        let _ops = self.shared.ops.lock().await;
        let (key, salt) = self.verified_key(password).await?;
        // Secret writes from now on hold this key's salt against the database's (RC-080-11).
        self.shared.db.hold_vault_key(Some(salt));
        let now = self.now();
        self.machine().unlocked(key, now);
        Ok(())
    }

    /// Removes the vault once its last secret is gone: the `vault_meta` row is deleted and the
    /// state is `Absent` again, so no master password is asked until a secret is stored anew.
    /// Asks for no password: there is nothing left to protect.
    ///
    /// # Errors
    ///
    /// [`VaultError::InUse`] while a secret column holds a value, [`VaultError::Absent`] when
    /// there is no vault, or the database fails.
    pub async fn remove(&self) -> Result<()> {
        let _ops = self.shared.ops.lock().await;
        match self
            .shared
            .db
            .remove_vault_meta(self.shared.options.secret_columns)
            .await?
        {
            VaultRemoval::Removed => {
                self.shared.db.hold_vault_key(None);
                self.machine().removed();
                Ok(())
            }
            VaultRemoval::Absent => {
                self.shared.db.hold_vault_key(None);
                self.machine().removed();
                Err(VaultError::Absent)
            }
            VaultRemoval::InUse { secrets } => Err(VaultError::InUse { secrets }),
        }
    }

    /// Drops the key from memory (zeroized); says whether the vault was unlocked.
    pub fn lock(&self) -> bool {
        self.machine().lock()
    }

    /// Counts as the person's use, which defers the idle lock.
    ///
    /// # Errors
    ///
    /// [`VaultError::Locked`] or [`VaultError::Absent`].
    pub fn touch(&self) -> Result<()> {
        let now = self.now();
        self.machine().key_for_use(now, Use::Person).map(|_| ())
    }

    /// Locks when the idle lock or the longest time unlocked has fallen due; says whether it
    /// locked. The engine's timer calls this.
    pub fn lock_if_idle(&self) -> bool {
        let now = self.now();
        self.machine().lock_if_idle(now)
    }

    /// How long until the vault locks by itself - the idle lock or the longest time unlocked,
    /// whichever comes first - while unlocked and either is set.
    #[must_use]
    pub fn idle_lock_in(&self) -> Option<Duration> {
        let now = self.now();
        self.machine()
            .lock_deadline()
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    /// The idle lock in minutes; `0` never locks.
    #[must_use]
    pub fn idle_lock_minutes(&self) -> u32 {
        self.machine().idle_lock_minutes()
    }

    /// Sets and stores the idle lock in minutes; `0` never locks.
    ///
    /// # Errors
    ///
    /// The setting cannot be written; the lock in memory is unchanged then.
    pub async fn set_idle_lock_minutes(&self, minutes: u32) -> Result<()> {
        self.shared.db.set(IDLE_LOCK_KEY, &minutes).await?;
        self.machine().set_idle_lock_minutes(minutes);
        Ok(())
    }

    /// Sets and stores the longest time the vault stays unlocked, in hours from the unlock;
    /// `0` sets no limit.
    ///
    /// # Errors
    ///
    /// The setting cannot be written; the limit in memory is unchanged then.
    pub async fn set_max_unlocked_hours(&self, hours: u32) -> Result<()> {
        self.shared.db.set(MAX_UNLOCKED_KEY, &hours).await?;
        self.machine().set_max_unlocked_hours(hours);
        Ok(())
    }

    /// `plaintext` encrypted for `field`, ready to store in its column. Counts as the person's
    /// use: only the window stores secrets.
    ///
    /// # Errors
    ///
    /// [`VaultError::Locked`] or [`VaultError::Absent`].
    pub fn encrypt(&self, field: &FieldRef<'_>, plaintext: &[u8]) -> Result<Vec<u8>> {
        let now = self.now();
        let mut machine = self.machine();
        aead::seal(
            machine.key_for_use(now, Use::Person)?,
            &field.associated_data(),
            plaintext,
        )
    }

    /// [`Vault::encrypt`] for a text secret.
    ///
    /// # Errors
    ///
    /// As [`Vault::encrypt`].
    pub fn encrypt_str(&self, field: &FieldRef<'_>, secret: &SecretString) -> Result<Vec<u8>> {
        use secrecy::ExposeSecret as _;
        self.encrypt(field, secret.expose_secret().as_bytes())
    }

    /// The plaintext of `value` read from `field`. Counts as the person's use.
    ///
    /// # Errors
    ///
    /// [`VaultError::Locked`], [`VaultError::Absent`], [`VaultError::Malformed`], or
    /// [`VaultError::Undecryptable`] - among others for a value copied from another field.
    pub fn decrypt(&self, field: &FieldRef<'_>, value: &[u8]) -> Result<SecretBytes> {
        let now = self.now();
        let mut machine = self.machine();
        let plaintext = aead::open(
            machine.key_for_use(now, Use::Person)?,
            &field.associated_data(),
            value,
        )?;
        Ok(SecretBytes::from(plaintext.to_vec()))
    }

    /// [`Vault::decrypt`] for a text secret.
    ///
    /// # Errors
    ///
    /// As [`Vault::decrypt`], and [`VaultError::Malformed`] when the plaintext is not UTF-8.
    pub fn decrypt_string(&self, field: &FieldRef<'_>, value: &[u8]) -> Result<SecretString> {
        self.decrypt_string_for(field, value, Use::Person)
    }

    /// [`Vault::decrypt_string`] for `by`: an agent's read ([`Use::Agent`]) does not defer the
    /// idle lock.
    ///
    /// # Errors
    ///
    /// As [`Vault::decrypt_string`].
    pub fn decrypt_string_for(
        &self,
        field: &FieldRef<'_>,
        value: &[u8],
        by: Use,
    ) -> Result<SecretString> {
        let now = self.now();
        let mut machine = self.machine();
        let plaintext = aead::open(
            machine.key_for_use(now, by)?,
            &field.associated_data(),
            value,
        )?;
        let text = std::str::from_utf8(&plaintext).map_err(|_| VaultError::Malformed)?;
        Ok(SecretString::from(text.to_owned()))
    }

    /// The key of `password` checked against the vault's verifier, with the salt it was derived
    /// under; the failure delay before and the failure count after. The caller holds `ops`.
    pub(crate) async fn verified_key(
        &self,
        password: &MasterPassword,
    ) -> Result<(VaultKey, Vec<u8>)> {
        let Some(meta) = self.shared.db.vault_meta().await? else {
            return Err(VaultError::Absent);
        };
        self.machine().exists();
        let params = KdfParams::from_meta(&meta)?;
        let delay = self
            .machine()
            .pending_delay(self.shared.options.failure_delay);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let key = kdf::derive(password, meta.salt.clone(), params).await?;
        match aead::open(&key, VERIFIER_AAD, &meta.verifier) {
            Ok(plaintext) if plaintext.as_slice() == VERIFIER_PLAINTEXT => Ok((key, meta.salt)),
            _ => {
                self.machine().failed(self.shared.options.failure_delay);
                Err(VaultError::WrongPassword)
            }
        }
    }
}

/// A fresh salt, the key of `password` under it and the row that describes both.
pub(crate) async fn new_key(
    password: &MasterPassword,
    params: KdfParams,
) -> Result<(VaultMeta, VaultKey)> {
    // Never weaker than the floor, whatever the options say (RC-080-42).
    let params = params.checked()?;
    let salt = kdf::new_salt().to_vec();
    let key = kdf::derive(password, salt.clone(), params).await?;
    let verifier = aead::seal(&key, VERIFIER_AAD, VERIFIER_PLAINTEXT)?;
    let meta = VaultMeta {
        version: VAULT_VERSION,
        kdf: KDF_NAME.to_owned(),
        m_cost_kib: i64::from(params.m_cost_kib),
        t_cost: i64::from(params.t_cost),
        p_cost: i64::from(params.p_cost),
        salt,
        verifier,
    };
    Ok((meta, key))
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
