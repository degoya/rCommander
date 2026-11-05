//! The key derivation: argon2id over the master password and a random salt, and the key it
//! yields, which lives in memory only and is zeroized when dropped.

use std::fmt;

use argon2::{Algorithm, Argon2, Params, Version};
use rand::Rng as _;
use rc_db::VaultMeta;
use zeroize::Zeroize as _;

use crate::recover::LogPanic as _;
use crate::{MasterPassword, Result, VaultError};

/// The bytes of a salt; a new one for every vault and every password change.
pub(crate) const SALT_LEN: usize = 16;

/// The length of the derived key in bytes, as the field cipher takes it.
pub(crate) const KEY_LEN: usize = 32;

/// The name `vault_meta.kdf` stores.
pub(crate) const KDF_NAME: &str = "argon2id";

/// argon2id's cost parameters. Stored with each vault and read back, never assumed, so raising
/// [`KdfParams::DEFAULT`] later still opens the vaults made before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory in KiB.
    pub m_cost_kib: u32,
    /// Passes over the memory.
    pub t_cost: u32,
    /// Lanes.
    pub p_cost: u32,
}

impl KdfParams {
    /// For new vaults and password changes: 64 MiB, three passes, one lane - about half a
    /// second to a second on a current desktop.
    pub const DEFAULT: Self = Self {
        m_cost_kib: 64 * 1024,
        t_cost: 3,
        p_cost: 1,
    };

    /// The largest values a stored row may name; anything above is a damaged or hostile file
    /// rather than a vault (a restored backup, RC-080-03), and would make unlocking allocate
    /// gigabytes or run for minutes. Sixteen times the default memory, five times its passes.
    const MAX: Self = Self {
        m_cost_kib: 1024 * 1024,
        t_cost: 16,
        p_cost: 16,
    };

    /// The most a sealed export may ask for (RC-080-68): a file from elsewhere is anyone's, so
    /// its cap is lower than the own database's [`KdfParams::MAX`] - four times the default
    /// memory, eight passes, four lanes. Above it the import is refused before argon2 runs
    /// ([`VaultError::TooCostly`]).
    pub const IMPORT_MAX: Self = Self {
        m_cost_kib: 256 * 1024,
        t_cost: 8,
        p_cost: 4,
    };

    /// The least a vault, a password change or a sealed export may use, and a stored row or an
    /// export may name (RC-080-42): OWASP's floor for argon2id, 19 MiB and two passes. Below it
    /// a row is damaged or an attacker's - cheap to guess against - not a vault this program
    /// made; [`KdfParams::DEFAULT`] lies well above it.
    pub const MIN: Self = Self {
        m_cost_kib: 19 * 1024,
        t_cost: 2,
        p_cost: 1,
    };

    /// argon2's own least: what the tests derive with, in milliseconds.
    const ARGON2_LEAST: Self = Self {
        m_cost_kib: 8,
        t_cost: 1,
        p_cost: 1,
    };

    /// The floor for making a key: [`KdfParams::MIN`]; in a debug build (the tests, which derive
    /// in milliseconds with 64 KiB and one pass) argon2's own least.
    fn make_floor() -> Self {
        if cfg!(debug_assertions) {
            Self::ARGON2_LEAST
        } else {
            Self::MIN
        }
    }

    /// The floor for reading what this process would have made with `ours` (RC-080-68): never
    /// above [`KdfParams::MIN`], and below it only as far as `ours` goes in a debug build. A
    /// release build reads nothing below [`KdfParams::MIN`]; a debug build of the program, which
    /// makes keys with [`KdfParams::DEFAULT`], neither. Only a process whose own parameters are
    /// weaker - the tests of every crate, which `#[cfg(test)]` here does not reach - reads keys
    /// as weak as its own.
    fn read_floor(ours: Self) -> Self {
        let least = Self::make_floor();
        let pick = |min: u32, ours: u32, least: u32| min.min(ours).max(least);
        Self {
            m_cost_kib: pick(Self::MIN.m_cost_kib, ours.m_cost_kib, least.m_cost_kib),
            t_cost: pick(Self::MIN.t_cost, ours.t_cost, least.t_cost),
            p_cost: pick(Self::MIN.p_cost, ours.p_cost, least.p_cost),
        }
    }

    /// The parameters of a stored row, read by a vault that makes its keys with `ours`.
    ///
    /// # Errors
    ///
    /// [`VaultError::BadParameters`]: another row version or KDF, or values out of range.
    pub(crate) fn from_meta(meta: &VaultMeta, ours: Self) -> Result<Self> {
        if meta.version != crate::vault::VAULT_VERSION || meta.kdf != KDF_NAME {
            return Err(VaultError::BadParameters);
        }
        let read = |value: i64| u32::try_from(value).map_err(|_| VaultError::BadParameters);
        let params = Self {
            m_cost_kib: read(meta.m_cost_kib)?,
            t_cost: read(meta.t_cost)?,
            p_cost: read(meta.p_cost)?,
        };
        params.within(Self::read_floor(ours), Self::MAX)
    }

    /// The parameters for a new key when none exceeds [`KdfParams::MAX`] and none lies below the
    /// floor ([`KdfParams::MIN`] in a release build): a vault, a password change and a sealed
    /// export make keys only within these bounds.
    ///
    /// # Errors
    ///
    /// [`VaultError::BadParameters`] for a value out of its bounds.
    pub(crate) fn checked(self) -> Result<Self> {
        self.within(Self::make_floor(), Self::MAX)
    }

    /// The parameters a sealed export names, read by a process that seals with `ours`: refused
    /// with [`VaultError::TooCostly`] above [`KdfParams::IMPORT_MAX`], before anything is
    /// derived, and with [`VaultError::BadParameters`] below the read floor.
    ///
    /// # Errors
    ///
    /// As above.
    pub(crate) fn checked_import(self, ours: Self) -> Result<Self> {
        if self.above(Self::IMPORT_MAX) {
            return Err(VaultError::TooCostly);
        }
        self.within(Self::read_floor(ours), Self::IMPORT_MAX)
    }

    /// Whether any value exceeds its counterpart in `max`.
    pub(crate) fn above(self, max: Self) -> bool {
        self.m_cost_kib > max.m_cost_kib || self.t_cost > max.t_cost || self.p_cost > max.p_cost
    }

    /// The parameters when each lies between `min` and `max`.
    fn within(self, min: Self, max: Self) -> Result<Self> {
        let fits = |value: u32, min: u32, max: u32| (min..=max).contains(&value);
        if fits(self.m_cost_kib, min.m_cost_kib, max.m_cost_kib)
            && fits(self.t_cost, min.t_cost, max.t_cost)
            && fits(self.p_cost, min.p_cost, max.p_cost)
        {
            Ok(self)
        } else {
            Err(VaultError::BadParameters)
        }
    }
}

/// The derived key. Boxed so moving it copies no key bytes; zeroized on drop.
pub(crate) struct VaultKey(Box<[u8; KEY_LEN]>);

impl VaultKey {
    pub(crate) fn bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// A key from fixed bytes, for the cipher tests.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Box::new(bytes))
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.0.zeroize();
        #[cfg(test)]
        wipe_hook::record(self.0.as_slice());
    }
}

impl fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaultKey([REDACTED])")
    }
}

/// Counts dropped keys whose bytes were all zero afterwards, per thread: the lock test reads it
/// instead of scanning memory.
#[cfg(test)]
pub(crate) mod wipe_hook {
    use std::cell::Cell;

    thread_local! {
        static WIPED: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn record(bytes: &[u8]) {
        if bytes.iter().all(|byte| *byte == 0) {
            WIPED.with(|wiped| wiped.set(wiped.get() + 1));
        }
    }

    pub(crate) fn wiped() -> usize {
        WIPED.with(Cell::get)
    }
}

/// A new random salt.
pub(crate) fn new_salt() -> [u8; SALT_LEN] {
    let mut salt = [0_u8; SALT_LEN];
    rand::rng().fill_bytes(&mut salt);
    salt
}

/// The key of `password` under `salt` and `params`. CPU-bound for about a second with the
/// default parameters: call it off the async runtime (`spawn_blocking`).
///
/// # Errors
///
/// [`VaultError::BadParameters`] when argon2 refuses the parameters or the salt.
pub(crate) fn derive_key(
    password: &MasterPassword,
    salt: &[u8],
    params: KdfParams,
) -> Result<VaultKey> {
    let argon_params = Params::new(
        params.m_cost_kib,
        params.t_cost,
        params.p_cost,
        Some(KEY_LEN),
    )
    .map_err(|_| VaultError::BadParameters)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut key = VaultKey(Box::new([0_u8; KEY_LEN]));
    argon
        .hash_password_into(password.as_bytes(), salt, key.0.as_mut_slice())
        .map_err(|_| VaultError::BadParameters)?;
    Ok(key)
}

/// [`derive_key`] on the blocking pool: it holds a core for about a second. The one way the
/// vault and the export envelope derive a key (RC-080-48).
///
/// # Errors
///
/// As [`derive_key`]; [`VaultError::Interrupted`] when the derivation did not finish.
pub(crate) async fn derive(
    password: &MasterPassword,
    salt: Vec<u8>,
    params: KdfParams,
) -> Result<VaultKey> {
    let password = password.duplicate();
    tokio::task::spawn_blocking(move || derive_key(&password, &salt, params))
        .await
        .log_panic("vault.derive_key")
        .map_err(|_| VaultError::Interrupted)?
}

#[cfg(test)]
#[path = "kdf_tests.rs"]
mod tests;
