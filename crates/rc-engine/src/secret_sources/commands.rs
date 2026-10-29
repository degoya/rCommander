//! The `secrets.source*` commands: the installed sources and their state, a search in one, and
//! a connection's references set and removed. Listing and searching are reads - titles, users and
//! addresses, never a value; `secrets.source_map` and `secrets.source_unmap` are `GuiOnly`.

use rc_db::{MAX_SOURCE_ENTRY_ID_CHARS, MAX_SOURCE_LABEL_CHARS};
use rc_plugin_api::secrets::{SecretEntry, SourceState};
use serde::{Deserialize, Serialize};

use super::{PLUGIN_SOURCE_PREFIX, SecretSourceError, SecretSourceRef, WINCRED_SOURCE};
use crate::{ConnectionId, Engine, EngineError, Outcome, SecretKind};

/// The most entries one search answers.
pub const MAX_SOURCE_RESULTS: u32 = 50;

/// One installed source and whether it answers now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretSourceInfo {
    /// `plugin:<id>`: what a reference names.
    pub source: String,
    pub name: String,
    /// `ready`, `locked`, `unreachable`, `unlicensed`, or `failed` with the host's `code`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// One entry a search found: what the person picks it by, never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntry {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl From<SecretEntry> for SourceEntry {
    fn from(entry: SecretEntry) -> Self {
        Self {
            id: entry.id,
            title: entry.title,
            user: entry.user,
            url: entry.url,
        }
    }
}

/// The answer of a `secrets.source*` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum SecretSourcesOutcome {
    /// `secrets.sources`: the installed sources by name, and the connection's references when
    /// one was named.
    Sources {
        sources: Vec<SecretSourceInfo>,
        mappings: Vec<SecretSourceRef>,
    },
    /// `secrets.source_search`: the entries found.
    Entries {
        source: String,
        entries: Vec<SourceEntry>,
    },
    /// The connection's references after `secrets.source_map` or `secrets.source_unmap`.
    Mappings {
        connection_id: ConnectionId,
        mappings: Vec<SecretSourceRef>,
    },
}

fn answer(outcome: SecretSourcesOutcome) -> Outcome {
    Outcome::SecretSources(Box::new(outcome))
}

/// Whether `text` holds a control character; a reference keeps none.
fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// What a reference must be to be stored, from the window or from an import: a source name of
/// the form `plugin:<id>` or `wincred`, an entry id and a label within bounds, no control
/// character. Whether the source is installed is not asked here.
pub(crate) fn check_reference(reference: &SecretSourceRef) -> Result<(), EngineError> {
    let invalid = |reason| Err(SecretSourceError::Invalid { reason }.into());
    let named = reference.source == WINCRED_SOURCE
        || reference
            .source
            .strip_prefix(PLUGIN_SOURCE_PREFIX)
            .is_some_and(|id| {
                id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
            });
    if !named {
        return invalid("the source is not a plugin or the Windows credential manager");
    }
    let id_chars = reference.entry_id.chars().count();
    if id_chars == 0 || id_chars > MAX_SOURCE_ENTRY_ID_CHARS {
        return invalid("the entry id is empty or too long");
    }
    if reference.entry_label.chars().count() > MAX_SOURCE_LABEL_CHARS {
        return invalid("the label is too long");
    }
    if has_control(&reference.entry_id) || has_control(&reference.entry_label) {
        return invalid("a control character");
    }
    Ok(())
}

impl Engine {
    /// `secrets.sources`: each installed, enabled source asked for its state.
    pub(crate) async fn list_secret_sources(
        &self,
        connection: Option<ConnectionId>,
    ) -> Result<Outcome, EngineError> {
        let mappings = match connection {
            Some(id) => {
                let db = self.db()?;
                db.connection(id).await?;
                db.secret_sources(id).await?
            }
            None => Vec::new(),
        };
        let mut sources = Vec::new();
        if self.wincred_offered().await {
            sources.push(SecretSourceInfo {
                source: WINCRED_SOURCE.to_owned(),
                name: Self::wincred_source().name,
                state: SourceState::Ready.as_str().to_owned(),
                code: None,
            });
        }
        for package in self.installed_sources().await {
            let source = format!("{PLUGIN_SOURCE_PREFIX}{}", package.manifest.id);
            let name = package.manifest.name.clone();
            let (state, code) = match self.build_source(package).await {
                Ok(loaded) => match loaded.status().await {
                    Ok(state) => (state.as_str().to_owned(), None),
                    Err(failure) => (
                        "failed".to_owned(),
                        Some(failure.code().unwrap_or("plugin_failed").to_owned()),
                    ),
                },
                Err(EngineError::SecretSource(SecretSourceError::Failed { code, .. })) => {
                    ("failed".to_owned(), Some(code))
                }
                Err(error) => return Err(error),
            };
            sources.push(SecretSourceInfo {
                source,
                name,
                state,
                code,
            });
        }
        sources
            .sort_by(|left, right| (&left.name, &left.source).cmp(&(&right.name, &right.source)));
        Ok(answer(SecretSourcesOutcome::Sources { sources, mappings }))
    }

    /// `secrets.source_search`: at most [`MAX_SOURCE_RESULTS`] entries of `source` matching
    /// `query`. A source that is not ready answers why, as a connect would.
    pub(crate) async fn search_secret_source(
        &self,
        source: &str,
        query: &str,
    ) -> Result<Outcome, EngineError> {
        let loaded = self.load_source(source).await?;
        match loaded.search(query, MAX_SOURCE_RESULTS).await {
            Ok(entries) => Ok(answer(SecretSourcesOutcome::Entries {
                source: source.to_owned(),
                entries: entries.into_iter().map(SourceEntry::from).collect(),
            })),
            Err(failure) => {
                let code = failure.code().unwrap_or("plugin_failed").to_owned();
                let state = loaded.status().await.ok();
                Err(SecretSourceError::from_state(loaded.name, state, failure.kind, code).into())
            }
        }
    }

    /// `secrets.source_map`: the secret `kind` of connection `id` is taken from `entry_id` of
    /// `source` from now on. The source has to be installed; it need not be ready.
    pub(crate) async fn map_secret_source(
        &self,
        reference: SecretSourceRef,
    ) -> Result<Outcome, EngineError> {
        check_reference(&reference)?;
        let known = if reference.source == WINCRED_SOURCE {
            self.wincred_offered().await
        } else {
            self.installed_sources().await.iter().any(|package| {
                reference.source.strip_prefix(PLUGIN_SOURCE_PREFIX)
                    == Some(package.manifest.id.as_str())
            })
        };
        if !known {
            return Err(SecretSourceError::Missing {
                name: reference.source,
            }
            .into());
        }
        let id = reference.connection_id;
        {
            // A reference makes the connection hold a secret (`connections.rs`).
            let _writes = self.inner.secret_writes.lock().await;
            self.db()?.set_secret_source(&reference).await?;
        }
        self.forget_remote(id).await;
        self.mappings(id).await
    }

    /// `secrets.source_unmap`: the secret `kind` of connection `id` comes from the vault again.
    pub(crate) async fn unmap_secret_source(
        &self,
        id: ConnectionId,
        kind: SecretKind,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        db.connection(id).await?;
        if db.remove_secret_source(id, kind).await? {
            self.forget_remote(id).await;
        }
        self.mappings(id).await
    }

    async fn mappings(&self, id: ConnectionId) -> Result<Outcome, EngineError> {
        let mappings = self.db()?.secret_sources(id).await?;
        Ok(answer(SecretSourcesOutcome::Mappings {
            connection_id: id,
            mappings,
        }))
    }
}
