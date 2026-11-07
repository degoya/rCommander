//! The form's one-time code seed (RC-060-08): a secret field of an SSH connection, typed or
//! pasted as an `otpauth://totp/...` link or a bare Base32 secret, never shown once stored.
//!
//! The seed is the connection's own, whatever shared credential it takes its user and other
//! secrets from: a second factor belongs to one account on one server. So a connection on a
//! shared credential still sends its seed changes. The engine checks the seed before it stores
//! it (`totp_seed_invalid`, shown at the field).

use rc_engine::{ConnectionKind, SecretChange, SecretKind, SecretParam};
use rc_vault::ExposeSecret as _;

use super::{Field, Form};
use crate::i18n::t;

impl Form {
    /// Whether the seed field shows: for an SSH connection, not for a shared credential.
    pub fn uses_totp(&self) -> bool {
        !self.is_credential() && self.kind == ConnectionKind::Ssh
    }

    /// The seed's controls, in Tab order: the field, and on a shared credential - whose own
    /// secrets have no switches - the switch that removes a stored seed.
    pub(super) fn totp_fields(&self) -> Vec<Field> {
        let mut fields = Vec::new();
        if self.uses_totp() {
            fields.push(Field::TotpSeed);
            if self.shared.is_some() && self.stored.has_totp_seed {
                fields.push(Field::Forget(SecretKind::TotpSeed));
            }
        }
        fields
    }

    /// The typed seed to store; `None` when nothing is typed.
    pub(super) fn totp_set(&self) -> Option<SecretChange> {
        let value = self.totp_seed.expose_secret();
        (self.uses_totp() && !value.is_empty()).then(|| SecretChange::Set {
            kind: SecretKind::TotpSeed,
            value: SecretParam::new(value.to_owned()),
        })
    }

    /// What a connection on a shared credential changes of its own: the seed alone, removed
    /// when marked, then the typed one.
    pub(super) fn own_totp_changes(&self) -> Vec<SecretChange> {
        let mut changes = Vec::new();
        if self.forget.contains(&SecretKind::TotpSeed) && self.stored.has_totp_seed {
            changes.push(SecretChange::Clear {
                kind: SecretKind::TotpSeed,
            });
        }
        changes.extend(self.totp_set());
        changes
    }
}

/// The field a typed secret of `kind` goes to.
pub fn secret_field(kind: SecretKind) -> Field {
    match kind {
        SecretKind::Passphrase => Field::Passphrase,
        SecretKind::TotpSeed => Field::TotpSeed,
        SecretKind::Password | SecretKind::Key => Field::Password,
    }
}

/// The line under the seed field: what to paste, and what it does.
pub fn totp_hint() -> &'static str {
    t!("connections.totp_hint")
}

#[cfg(test)]
#[path = "totp_tests.rs"]
mod tests;
