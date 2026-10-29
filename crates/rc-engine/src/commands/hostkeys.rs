//! The `hostkeys.*` commands: the trust store in the settings (list, search, remove, import)
//! and the answers to the questions of `crate::hostkeys`.

use std::{io, path::PathBuf};

use rc_remote::{TrustError, hostkey::names_host, known_hosts_import, trust};
use serde::{Deserialize, Serialize};

use crate::{
    Engine, EngineError, EngineEvent, Outcome,
    hostkeys::{HostKeyAnswer, HostKeyQuestion, HostKeyQuestionKind},
};

/// What `hostkeys.import_known_hosts` stored and skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    /// The file read.
    pub path: String,
    /// New keys.
    pub added: usize,
    /// Keys stored already.
    pub unchanged: usize,
    /// Names stored with another key of the type, left as they are.
    pub conflicts: Vec<ImportConflict>,
    /// Lines and names that were not imported, and why.
    pub skipped: Vec<known_hosts_import::Skipped>,
}

/// A name of the file whose key differs from the stored one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportConflict {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    /// The fingerprint in the file; the stored one stays.
    pub fingerprint: String,
}

impl Engine {
    /// `hostkeys.list`: every stored key, or those whose name, type or fingerprint contains
    /// `filter` (ignoring case). A hashed name is found by the host it hashes, on port 22.
    pub(crate) async fn list_host_keys(
        &self,
        filter: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let mut keys = self.db()?.known_hosts().await?;
        if let Some(filter) = filter.map(str::trim).filter(|filter| !filter.is_empty()) {
            let needle = filter.to_lowercase();
            keys.retain(|key| {
                key.host.to_lowercase().contains(&needle)
                    || key.algorithm.to_lowercase().contains(&needle)
                    || key.fingerprint_sha256.to_lowercase().contains(&needle)
                    || (key.is_hashed()
                        && names_host(key, filter, rc_remote::hostkey::DEFAULT_PORT))
            });
        }
        Ok(Outcome::KnownHosts { keys })
    }

    /// `hostkeys.remove`: forgets a stored key; the next connection asks again.
    pub(crate) async fn remove_host_key(&self, id: i64) -> Result<Outcome, EngineError> {
        let key = self
            .db()?
            .remove_known_host(id)
            .await?
            .ok_or(EngineError::KnownHostNotFound { id })?;
        self.publish(EngineEvent::KnownHostsChanged);
        Ok(Outcome::KnownHostRemoved { key })
    }

    /// `hostkeys.import_known_hosts`: the keys of an OpenSSH `known_hosts` file, the user's
    /// own when `path` is `None`.
    pub(crate) async fn import_host_keys(
        &self,
        path: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let path = match path {
            Some(path) => PathBuf::from(path),
            None => known_hosts_import::default_path().ok_or_else(|| {
                EngineError::KnownHostsUnreadable {
                    path: "~/.ssh/known_hosts".to_owned(),
                    source: io::Error::new(io::ErrorKind::NotFound, "no home directory"),
                }
            })?,
        };
        let shown = path.display().to_string();
        let parsed = known_hosts_import::read(&path).await.map_err(|source| {
            EngineError::KnownHostsUnreadable {
                path: shown.clone(),
                source,
            }
        })?;
        let summary = trust::import(db, &parsed).await?;
        if summary.added > 0 {
            self.publish(EngineEvent::KnownHostsChanged);
        }
        Ok(Outcome::KnownHostsImported(ImportReport {
            path: shown,
            added: summary.added,
            unchanged: summary.unchanged,
            conflicts: summary
                .conflicts
                .into_iter()
                .map(|key| ImportConflict {
                    host: key.host,
                    port: key.port,
                    algorithm: key.algorithm,
                    fingerprint: key.fingerprint_sha256,
                })
                .collect(),
            skipped: parsed.skipped,
        }))
    }

    /// `hostkeys.answer`: the TOFU dialog's answer to an unknown key. A changed key takes no
    /// yes here - only `hostkeys.replace` - and keeps its question open.
    pub(crate) async fn answer_host_key(
        &self,
        id: u64,
        answer: HostKeyAnswer,
    ) -> Result<Outcome, EngineError> {
        let question = self.question(id)?;
        let stored = match (&question.kind, answer) {
            (_, HostKeyAnswer::Cancel) => None,
            (HostKeyQuestionKind::Changed { stored }, _) => {
                return Err(changed_error(&question, &stored.fingerprint_sha256));
            }
            (HostKeyQuestionKind::Unknown, HostKeyAnswer::ConnectOnce) => {
                self.inner.host_keys.allow_once(&question);
                None
            }
            (HostKeyQuestionKind::Unknown, HostKeyAnswer::TrustAndSave) => {
                let db = self.db()?;
                let stored =
                    match trust::trust(db, &question.host, question.port, question.offered.clone())
                        .await
                    {
                        Ok(stored) => stored,
                        Err(TrustError::Changed {
                            stored_fingerprint, ..
                        }) => return Err(changed_error(&question, &stored_fingerprint)),
                        Err(TrustError::Stale) => return Err(EngineError::HostKeyStale),
                        Err(TrustError::Db(error)) => return Err(error.into()),
                    };
                self.publish(EngineEvent::KnownHostsChanged);
                Some(stored)
            }
        };
        self.close_question(id);
        Ok(Outcome::HostKeyAnswered {
            question: id,
            answer,
            stored,
        })
    }

    /// `hostkeys.replace`: the explicit action that replaces the stored key of a changed-key
    /// question with the key the server presented. The connection is not resumed; it is tried
    /// again by the person.
    pub(crate) async fn replace_host_key(&self, id: u64) -> Result<Outcome, EngineError> {
        let question = self.question(id)?;
        let HostKeyQuestionKind::Changed { stored } = &question.kind else {
            return Err(EngineError::HostKeyNotChanged { question: id });
        };
        let key = match trust::replace(
            self.db()?,
            stored,
            &question.host,
            question.port,
            &question.offered,
        )
        .await
        {
            Ok(key) => key,
            Err(TrustError::Db(error)) => return Err(error.into()),
            Err(TrustError::Stale | TrustError::Changed { .. }) => {
                return Err(EngineError::HostKeyStale);
            }
        };
        self.close_question(id);
        self.publish(EngineEvent::KnownHostsChanged);
        Ok(Outcome::HostKeyReplaced { key })
    }

    fn question(&self, id: u64) -> Result<HostKeyQuestion, EngineError> {
        self.inner
            .host_keys
            .get(id)
            .ok_or(EngineError::HostKeyQuestionNotFound { question: id })
    }

    fn close_question(&self, id: u64) {
        self.inner.host_keys.close(id);
        self.publish(EngineEvent::HostKeyQuestionClosed { question: id });
    }
}

fn changed_error(question: &HostKeyQuestion, stored_fingerprint: &str) -> EngineError {
    EngineError::HostKeyChanged {
        host: question.host.clone(),
        port: question.port,
        algorithm: question.offered.algorithm.clone(),
        fingerprint: question.offered.fingerprint.clone(),
        stored_fingerprint: stored_fingerprint.to_owned(),
    }
}

#[cfg(test)]
#[path = "hostkeys_tests.rs"]
mod tests;
