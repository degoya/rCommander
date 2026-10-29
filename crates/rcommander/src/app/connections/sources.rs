//! Under the form of a stored connection: where each of its secrets comes from - the vault, or
//! an entry of a secret source (RC-100-34). Choosing a source opens a search in it; picking an
//! entry stores the reference at once (`secrets.source_map`), choosing the vault again removes it
//! (`secrets.source_unmap`). Only titles, users and addresses are shown, never a value.
//!
//! The panel shows only while a source is installed; the sources and their state are read when
//! the manager opens, a connection's references when its form shows.

use rc_db::MAX_SOURCE_LABEL_CHARS;
use rc_engine::{
    Command, ConnectionId, Outcome, SecretKind, SecretSourceRef, SecretSourcesOutcome,
    secret_sources::{SecretSourceInfo, SourceEntry},
};

use super::{ConnectionManager, jobs::Then, run};
use crate::{app::Effect, i18n::error_text};

/// The search field of the panel.
pub const SOURCE_QUERY_FIELD: &str = "rcommander.connections.source_query";

/// What the panel's controls say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMsg {
    /// The choice of where a secret of `kind` comes from: the vault (`None`) or a source.
    From(SecretKind, Option<String>),
    Query(String),
    Search,
    /// An entry of the search's results, by its place.
    Pick(usize),
}

/// The search in one source for one secret of the connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceSearch {
    pub kind: Option<SecretKind>,
    pub source: String,
    pub query: String,
    pub entries: Vec<SourceEntry>,
    /// Searched at least once, so an empty list means nothing was found.
    pub searched: bool,
    pub error: Option<String>,
    /// The number of the search asked last, whose answer alone is taken (RC-080-39).
    pub asked: u32,
}

/// The sources, the shown connection's references and a search.
#[derive(Debug, Default)]
pub struct SourcePanel {
    pub sources: Vec<SecretSourceInfo>,
    /// The connection whose references are held.
    pub connection: Option<ConnectionId>,
    pub mappings: Vec<SecretSourceRef>,
    pub search: Option<SourceSearch>,
    /// Counts the searches asked, across the searches opened.
    pub searches: u32,
}

impl SourcePanel {
    /// Where the secret `kind` comes from: the source's name, `None` for the vault.
    pub fn mapping(&self, kind: SecretKind) -> Option<&SecretSourceRef> {
        self.mappings.iter().find(|mapping| mapping.kind == kind)
    }
}

/// Reads the installed sources and their state.
pub(super) fn list_sources() -> Effect {
    run(
        Then::Sources,
        Command::ListSecretSources {
            connection_id: None,
        },
    )
}

/// What the form shows as a reference's label: the title and the user, cut to what is kept.
fn entry_label(entry: &SourceEntry) -> String {
    let label = match &entry.user {
        Some(user) => format!("{} ({user})", entry.title),
        None => entry.title.clone(),
    };
    label.chars().take(MAX_SOURCE_LABEL_CHARS).collect()
}

impl ConnectionManager {
    /// The references follow the form: the stored connection it shows now has its own read.
    pub(super) fn sources_follow(&mut self) -> Vec<Effect> {
        let shown = self.form.as_ref().and_then(|form| form.id());
        if self.sources.sources.is_empty() || shown == self.sources.connection {
            return Vec::new();
        }
        self.sources.connection = shown;
        self.sources.mappings.clear();
        self.sources.search = None;
        shown
            .map(|id| {
                run(
                    Then::Sources,
                    Command::ListSecretSources {
                        connection_id: Some(id),
                    },
                )
            })
            .into_iter()
            .collect()
    }

    pub(super) fn on_source(&mut self, message: SourceMsg) -> Vec<Effect> {
        let Some(id) = self.sources.connection else {
            return Vec::new();
        };
        match message {
            SourceMsg::From(kind, None) => {
                self.sources.search = None;
                if self.sources.mapping(kind).is_none() {
                    return Vec::new();
                }
                let command = Command::UnmapSecretSource {
                    connection_id: id,
                    kind,
                };
                vec![run(Then::SourceMapped, command)]
            }
            SourceMsg::From(kind, Some(source)) => {
                self.sources.search = Some(SourceSearch {
                    kind: Some(kind),
                    source,
                    ..SourceSearch::default()
                });
                vec![super::keys::focus_id(SOURCE_QUERY_FIELD)]
            }
            SourceMsg::Query(text) => {
                if let Some(search) = &mut self.sources.search {
                    search.query = text;
                }
                Vec::new()
            }
            SourceMsg::Search => match &mut self.sources.search {
                Some(search) => {
                    search.error = None;
                    self.sources.searches = self.sources.searches.wrapping_add(1);
                    search.asked = self.sources.searches;
                    let command = Command::SearchSecretSource {
                        source: search.source.clone(),
                        query: search.query.clone(),
                    };
                    vec![run(Then::SourceFound(search.asked), command)]
                }
                None => Vec::new(),
            },
            SourceMsg::Pick(at) => {
                let Some(search) = &self.sources.search else {
                    return Vec::new();
                };
                let (Some(kind), Some(entry)) = (search.kind, search.entries.get(at)) else {
                    return Vec::new();
                };
                let reference = SecretSourceRef {
                    connection_id: id,
                    kind,
                    source: search.source.clone(),
                    entry_id: entry.id.clone(),
                    entry_label: entry_label(entry),
                };
                vec![run(Then::SourceMapped, Command::MapSecretSource(reference))]
            }
        }
    }

    /// The engine answered a command of the panel.
    pub(super) fn source_answer(
        &mut self,
        then: Then,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        // The answer of an earlier search, or of one in another source, is left out: its
        // entries would be stored with the shown search's source.
        if let Then::SourceFound(asked) = then
            && self
                .sources
                .search
                .as_ref()
                .is_none_or(|search| search.asked != asked)
        {
            return Vec::new();
        }
        let outcome = match result {
            Ok(Outcome::SecretSources(outcome)) => *outcome,
            Ok(_) => return Vec::new(),
            Err(code) => {
                let text = error_text(code);
                match (then, &mut self.sources.search) {
                    (Then::SourceFound(_), Some(search)) => search.error = Some(text),
                    _ => self.note = Some((text, true)),
                }
                return Vec::new();
            }
        };
        match outcome {
            SecretSourcesOutcome::Sources { sources, mappings } => {
                self.sources.sources = sources;
                if let Some(shown) = self.sources.connection {
                    // An answer for a connection the form no longer shows is left out.
                    self.sources.mappings = mappings
                        .into_iter()
                        .filter(|mapping| mapping.connection_id == shown)
                        .collect();
                } else {
                    return self.sources_follow();
                }
            }
            SecretSourcesOutcome::Entries { source, entries } => {
                if let Some(search) = &mut self.sources.search
                    && search.source == source
                {
                    search.entries = entries;
                    search.searched = true;
                }
            }
            SecretSourcesOutcome::Mappings {
                connection_id,
                mappings,
            } if Some(connection_id) == self.sources.connection => {
                self.sources.mappings = mappings;
                self.sources.search = None;
            }
            SecretSourcesOutcome::Mappings { .. } => {}
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
