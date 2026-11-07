//! The one-time code of a connection's seed (RC-060-08), as an SSH login and a login script's
//! `send_totp` step ask for it.
//!
//! The seed is the connection's own secret of kind [`SecretKind::TotpSeed`], read from the vault
//! only - a shared credential and a secret source never stand in for it (a code from a password
//! manager, source `totp` of RC-100-33, is the licensed plugin's and not wired here). The code is
//! made from it at the moment it is asked for and remembered for the MCP mask, as every secret
//! read is, so neither the seed nor a code still on a terminal screen reaches an agent.

use rc_remote::totp::TotpSeed;
use rc_vault::{ExposeSecret as _, SecretString};

use crate::{ConnectionId, Engine, EngineError, SecretKind, SecretRef};

impl Engine {
    /// The current code of connection `connection`'s seed; `None` when it has none. A locked
    /// vault is announced as the seed needed when `announce` is set, as for every secret.
    ///
    /// # Errors
    ///
    /// [`EngineError::VaultLocked`] while a seed is stored and the vault is locked; a stored
    /// value that is no seed ([`EngineError::TotpSeedInvalid`]); what reading a secret fails with.
    pub(crate) async fn totp_code(
        &self,
        connection: ConnectionId,
        announce: bool,
    ) -> Result<Option<SecretString>, EngineError> {
        let secret = SecretRef {
            connection,
            kind: SecretKind::TotpSeed,
        };
        let Some(stored) = self.vault_secret(secret, announce).await? else {
            return Ok(None);
        };
        let seed = TotpSeed::parse(stored.expose_secret()).map_err(|error| {
            EngineError::TotpSeedInvalid {
                reason: error.to_string(),
            }
        })?;
        let code = seed.code_now();
        self.remember_secret(&code);
        tracing::debug!(%connection, "a one-time code was made");
        Ok(Some(code))
    }
}

/// Checks a seed about to be stored: `Ok` when it is one.
pub(crate) fn check_seed(value: &SecretString) -> Result<(), EngineError> {
    TotpSeed::parse(value.expose_secret())
        .map(drop)
        .map_err(|error| EngineError::TotpSeedInvalid {
            reason: error.to_string(),
        })
}

#[cfg(test)]
#[path = "totp_tests.rs"]
mod tests;
