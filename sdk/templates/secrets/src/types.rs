//! The secrets world's shapes this plugin uses, mirrored from `wit/rcommander.wit`, so the logic
//! in `lib.rs` is plain Rust that `cargo test` runs without a WebAssembly toolchain.
//! `src/guest.rs` maps each to its WIT twin and nothing more.

/// `source-state`: whether the manager answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceState {
    Ready,
    Locked,
    Unreachable,
    Unlicensed,
}

/// `secret-field`: which secret of an entry `get` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretField {
    Password,
    PrivateKey,
    Passphrase,
    Totp,
}

impl SecretField {
    /// The name the WIT enum uses.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "private-key",
            Self::Passphrase => "passphrase",
            Self::Totp => "totp",
        }
    }
}

/// `secret-entry`: what the person picks an entry by, never a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretEntry {
    pub id: String,
    pub title: String,
    pub user: Option<String>,
    pub url: Option<String>,
}

/// `failure-kind`, the cases this source answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    NotFound,
    BadData,
    Io,
}

/// `failure`: a kind and an optional code from `locales/en.json`, which starts with the plugin's
/// id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub code: Option<String>,
}

impl Failure {
    /// A failure of `kind` without a code of its own.
    #[must_use]
    pub fn of(kind: FailureKind) -> Self {
        Self { kind, code: None }
    }
}

/// `http-response`, the parts this source reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The `http` interface: one `GET` to the granted loopback port. A redirect is answered, never
/// followed; a refused or failed request is an `Err`.
pub trait Http {
    fn get(&mut self, url: &str) -> Result<HttpResponse, Failure>;
}
