//! The native side of the `secrets-plugin` world (RC-100-33): the records of `interface secrets`
//! and the [`Secrets`] trait a password manager's logic implements against a [`SecretsHost`].
//!
//! A secret source answers rCommander: it hands a value to the host for a connection the person
//! mapped, and receives none. It reaches its manager only through `http` on the loopback port its
//! manifest grants (`docs/plugins.md`, "Secret source").
//!
//! A PAM system lends its secret instead (contract 0.5.0, RC-100-38): [`Secrets::checkout`] for a
//! session, with a reason when the system asks for one, and [`Secrets::checkin`] when the session
//! ends. rCommander asks only a source whose manifest says `[secrets] checkout = true`; every
//! other one keeps the defaults, which answer `not-supported`.

use std::fmt;

use crate::{Failure, FailureKind};

/// The failure code a source answers `checkout` with when the PAM system wants a reason and none
/// was given: the window asks the person for one, and the next connect hands it over.
pub const REASON_REQUIRED: &str = "checkout.reason_required";

/// Whether the manager answers, as `status` reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceState {
    /// Searches and reads.
    Ready,
    /// Running but locked; the person unlocks it in the manager's own client.
    Locked,
    /// Not running or not answering.
    Unreachable,
    /// The plugin's licence is missing, expired or for another plugin (RC-100-36).
    Unlicensed,
}

impl SourceState {
    /// The name the WIT enum uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Locked => "locked",
            Self::Unreachable => "unreachable",
            Self::Unlicensed => "unlicensed",
        }
    }
}

/// Which secret of an entry `get` returns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretField {
    Password,
    PrivateKey,
    Passphrase,
    /// The current one-time code, never the seed.
    Totp,
}

impl SecretField {
    /// The name the WIT enum uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "private-key",
            Self::Passphrase => "passphrase",
            Self::Totp => "totp",
        }
    }
}

/// One entry a search finds: what the person picks it by, never a value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretEntry {
    /// The manager's own id, handed back to `get`.
    pub id: String,
    pub title: String,
    pub user: Option<String>,
    pub url: Option<String>,
}

/// A secret lent for a session, as `checkout` answers it.
#[derive(Clone, Eq, PartialEq)]
pub struct Lease {
    /// The system's own id of the lease, handed back to `checkin`.
    pub id: String,
    /// The secret itself.
    pub value: String,
    /// When the system takes it back by itself, in Unix seconds; `None` when it does not.
    pub expires: Option<u64>,
}

impl fmt::Debug for Lease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Lease")
            .field("id", &self.id)
            .field("value", &"[redacted]")
            .field("expires", &self.expires)
            .finish()
    }
}

/// The answer to one `http.request`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What a secret source may ask of its host: the `host`, `settings` and `http` interfaces of
/// its world.
pub trait SecretsHost {
    /// Writes a line to rCommander's log; the host caps and masks it.
    fn log(&mut self, level: &str, message: &str);
    /// The value of the manifest's setting `name`; `None` for a name it does not declare.
    fn setting(&mut self, name: &str) -> Option<String>;
    /// One request to the granted target; a redirect is answered, never followed.
    fn http(
        &mut self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<HttpResponse, Failure>;
}

/// The logic of a secret source: `interface secrets`, one method per export. A store lives for
/// one call, so nothing carries over between them.
pub trait Secrets {
    /// Whether the manager answers now.
    fn status(&mut self, host: &mut dyn SecretsHost) -> SourceState;

    /// At most `limit` entries matching `query`, with their titles, users and addresses only.
    fn search(
        &mut self,
        host: &mut dyn SecretsHost,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SecretEntry>, Failure>;

    /// The secret `field` of entry `id`.
    fn get(
        &mut self,
        host: &mut dyn SecretsHost,
        id: &str,
        field: SecretField,
    ) -> Result<String, Failure>;

    /// Lends the secret `field` of entry `id` for a session; `reason` is the person's, when the
    /// system asked for one ([`REASON_REQUIRED`]). Only a PAM system answers it.
    fn checkout(
        &mut self,
        _host: &mut dyn SecretsHost,
        _id: &str,
        _field: SecretField,
        _reason: Option<&str>,
    ) -> Result<Lease, Failure> {
        Err(Failure::new(FailureKind::NotSupported))
    }

    /// Gives the lease `lease_id` back; the system may change the secret then.
    fn checkin(&mut self, _host: &mut dyn SecretsHost, _lease_id: &str) -> Result<(), Failure> {
        Err(Failure::new(FailureKind::NotSupported))
    }
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
