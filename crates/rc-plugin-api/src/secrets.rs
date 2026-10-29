//! The native side of the `secrets-plugin` world (RC-100-33): the records of `interface secrets`
//! and the [`Secrets`] trait a password manager's logic implements against a [`SecretsHost`].
//!
//! A secret source answers rCommander: it hands a value to the host for a connection the person
//! mapped, and receives none. It reaches its manager only through `http` on the loopback port its
//! manifest grants (`docs/plugins.md`, "Secret source").

use crate::Failure;

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
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
