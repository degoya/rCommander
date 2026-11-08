//! The decisions about SSH host keys that wait for a person.
//!
//! A connection checks the key its server presented with [`Engine::check_host_key`]. A trusted
//! key passes. An unknown or changed one fails the connection with `host_key_unknown` or
//! `host_key_changed` - the fingerprint in the error - and becomes a [`HostKeyQuestion`],
//! announced as [`EngineEvent::HostKeyQuestion`]. The GUI's dialog answers it with the command
//! `hostkeys.answer` (connect once, trust and save, cancel), the connection is tried again, and
//! a "connect once" is spent by that retry.
//!
//! A key an imported `known_hosts` file revoked fails with `host_key_revoked` and files no
//! question at all: a hard stop, as OpenSSH makes it (RC-080-67).
//!
//! A changed key cannot be answered with either kind of yes: only the separate, explicit
//! `hostkeys.replace` swaps the stored key. Neither command has an MCP tool - an agent only
//! ever sees the coded error and the hint that a person has to decide in the app.

use rc_db::KnownHost;
use rc_remote::{OfferedKey, Verdict, hostkey::normalize_host, trust};
use serde::{Deserialize, Serialize};

use crate::{
    Engine, EngineError, EngineEvent,
    question_desk::{Question, QuestionDesk},
};

/// A host key a person has to decide about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostKeyQuestion {
    /// The id the answer names.
    pub id: u64,
    /// The host as the connection named it (lower case).
    pub host: String,
    pub port: u16,
    /// The key the server presented: type, `SHA256:` fingerprint, blob.
    pub offered: OfferedKey,
    pub kind: HostKeyQuestionKind,
    /// The key types already trusted for this host and port: an unknown key of another type
    /// for a known host - which an impostor offering another algorithm would present - says so
    /// in the dialog (RC-080-12).
    #[serde(default)]
    pub stored_types: Vec<String>,
    /// The hop of a chain of jump hosts the key was presented to (RC-110-03); `None` for a
    /// direct session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hop: Option<HopLabel>,
}

/// One session of a chain of jump hosts (RC-110-03): `index` of `total`, counted from the first
/// jump host to the target, and the name of its connection - "hop 1 of 3: bastion".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HopLabel {
    pub index: usize,
    pub total: usize,
    pub name: String,
}

/// Why the key needs a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostKeyQuestionKind {
    /// Nothing is stored for this host, port and key type: the first-sighting (TOFU) dialog.
    Unknown,
    /// Another key is stored: a hard stop with a warning; only `hostkeys.replace` goes on.
    Changed { stored: KnownHost },
}

/// The person's answer to an unknown key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKeyAnswer {
    /// Allow the next connection to this host with exactly this key, store nothing.
    ConnectOnce,
    /// Store the key as trusted.
    TrustAndSave,
    /// Do not connect.
    Cancel,
}

/// The open host-key questions and the "connect once" grants not used yet.
pub(crate) type HostKeyDesk = QuestionDesk<HostKeyQuestion>;

impl Question for HostKeyQuestion {
    fn id(&self) -> u64 {
        self.id
    }

    fn set_id(&mut self, id: u64) {
        self.id = id;
    }

    /// The same host, port, key and kind; the stored types the first one named stay.
    fn same_as(&self, other: &Self) -> bool {
        self.host == other.host
            && self.port == other.port
            && self.offered == other.offered
            && self.kind == other.kind
    }

    fn grant(&self) -> (&str, u16, &str) {
        (&self.host, self.port, &self.offered.fingerprint)
    }
}

impl Engine {
    /// The gate every SSH connection passes with the key its server presented, outside the
    /// `russh` callback (`rc_remote::describe` reads the key there).
    ///
    /// # Errors
    ///
    /// [`EngineError::HostKeyUnknown`] or [`EngineError::HostKeyChanged`], both after filing a
    /// [`HostKeyQuestion`] for the GUI; [`EngineError::HostKeyRevoked`] without one; database
    /// failures.
    pub async fn check_host_key(
        &self,
        host: &str,
        port: u16,
        offered: OfferedKey,
    ) -> Result<(), EngineError> {
        self.check_host_key_at(host, port, offered, None).await
    }

    /// [`Engine::check_host_key`] for the hop `hop` of a chain of jump hosts (RC-110-03): a
    /// question filed says which one.
    ///
    /// # Errors
    ///
    /// As [`Engine::check_host_key`].
    pub(crate) async fn check_host_key_at(
        &self,
        host: &str,
        port: u16,
        offered: OfferedKey,
        hop: Option<HopLabel>,
    ) -> Result<(), EngineError> {
        let host = normalize_host(host);
        let verdict = trust::verify(self.db()?, &host, port, offered).await?;
        match verdict {
            Verdict::Trusted(_) => Ok(()),
            Verdict::Revoked { offered, .. } => Err(EngineError::HostKeyRevoked {
                host,
                port,
                algorithm: offered.algorithm,
                fingerprint: offered.fingerprint,
            }),
            Verdict::Unknown(offered) => {
                if self
                    .inner
                    .host_keys
                    .take_allowance(&host, port, &offered.fingerprint)
                {
                    return Ok(());
                }
                let stored_types = trust::stored_types(self.db()?, &host, port).await?;
                let question = self.inner.host_keys.ask(HostKeyQuestion {
                    id: 0,
                    host,
                    port,
                    offered,
                    kind: HostKeyQuestionKind::Unknown,
                    stored_types,
                    hop,
                });
                let error = EngineError::HostKeyUnknown {
                    host: question.host.clone(),
                    port,
                    algorithm: question.offered.algorithm.clone(),
                    fingerprint: question.offered.fingerprint.clone(),
                };
                self.publish(EngineEvent::HostKeyQuestion {
                    question: Box::new(question),
                });
                Err(error)
            }
            Verdict::Changed { stored, offered } => {
                let stored_fingerprint = stored.fingerprint_sha256.clone();
                let question = self.inner.host_keys.ask(HostKeyQuestion {
                    id: 0,
                    host,
                    port,
                    offered,
                    kind: HostKeyQuestionKind::Changed { stored },
                    stored_types: Vec::new(),
                    hop,
                });
                let error = EngineError::HostKeyChanged {
                    host: question.host.clone(),
                    port,
                    algorithm: question.offered.algorithm.clone(),
                    fingerprint: question.offered.fingerprint.clone(),
                    stored_fingerprint,
                };
                self.publish(EngineEvent::HostKeyQuestion {
                    question: Box::new(question),
                });
                Err(error)
            }
        }
    }

    /// The key types trusted for `host` on `port`, which a connection's key exchange asks for
    /// first; none when the trust store cannot be read (the check after it still decides).
    pub(crate) async fn stored_host_key_types(&self, host: &str, port: u16) -> Vec<String> {
        let Ok(db) = self.db() else {
            return Vec::new();
        };
        trust::stored_types(db, &normalize_host(host), port)
            .await
            .unwrap_or_default()
    }

    /// The questions no one has answered yet, oldest first: what a window opened later shows.
    #[must_use]
    pub fn host_key_questions(&self) -> Vec<HostKeyQuestion> {
        self.inner.host_keys.open()
    }
}

#[cfg(test)]
#[path = "hostkeys_tests.rs"]
mod tests;
