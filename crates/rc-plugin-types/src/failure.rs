//! `Failure` and `FailureKind`, the native side of the WIT `types.failure` record.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-types/src/error.rs` at commit
//! 26c8592a6: the same shape (a class, a stable code, flat string parameters), the classes of
//! `docs/plugins.md` ("Shared interfaces") instead of a download scheduler's.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What went wrong, in the classes a file manager acts on. One case per WIT `failure-kind`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    NotFound,
    Exists,
    AccessDenied,
    NotSupported,
    BadData,
    WrongPassword,
    Cancelled,
    Io,
    Other,
}

impl FailureKind {
    /// The name the WIT contract and the logs use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Exists => "exists",
            Self::AccessDenied => "access_denied",
            Self::NotSupported => "not_supported",
            Self::BadData => "bad_data",
            Self::WrongPassword => "wrong_password",
            Self::Cancelled => "cancelled",
            Self::Io => "io",
            Self::Other => "other",
        }
    }
}

/// Flat, string-only parameters attached to a coded failure (counts, names, sizes).
pub type MessageParams = BTreeMap<String, String>;

/// A failure with an optional stable code (`<namespace>_<condition>` for the host's own,
/// `<slug>.<condition>` for a plugin's) that the window translates.
#[derive(Clone, Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
#[error("{}", self.code.as_deref().unwrap_or(self.kind.as_str()))]
pub struct Failure {
    pub kind: FailureKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: MessageParams,
}

impl Failure {
    /// A failure without a code: the class says everything.
    #[must_use]
    pub fn new(kind: FailureKind) -> Self {
        Self {
            kind,
            code: None,
            params: BTreeMap::new(),
        }
    }

    /// A failure carrying a stable translation code.
    #[must_use]
    pub fn coded(kind: FailureKind, code: &str) -> Self {
        Self {
            kind,
            code: Some(code.to_owned()),
            params: BTreeMap::new(),
        }
    }

    /// Attaches a translation parameter.
    #[must_use]
    pub fn with_param(mut self, key: &str, value: impl ToString) -> Self {
        self.params.insert(key.to_owned(), value.to_string());
        self
    }

    /// The code, if any.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;
