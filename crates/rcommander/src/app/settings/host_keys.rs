//! The settings' Host keys page (RC-050-07): the SSH host keys and FTPS certificates rCommander
//! trusts, in one table - server, port, type, SHA-256 fingerprint, when it was trusted - with a
//! search, Remove after a question, and the import of the user's own `known_hosts`.
//!
//! Every action is a registry command run as a [`TrustStoreCall`]: `hostkeys.list` and
//! `certificates.list` with the search as their filter (so a hashed name is found by its host, as
//! the engine finds it), `hostkeys.remove` and `certificates.remove`, and
//! `hostkeys.import_known_hosts`. Nothing here trusts a key: a new or changed one is trusted only
//! in the question of the connection that meets it, and an import never replaces a stored key.
//! The engine's `KnownHostsChanged` and `TrustedCertificatesChanged` read the table again, so a
//! key trusted or removed elsewhere shows at once.
//!
//! Keys while the page is shown: the arrows select a row, Delete asks about removing it; while
//! the question is open Enter removes and Escape keeps the key.

use rc_db::{KnownHost, TrustedCertificate};
use rc_engine::{Chord, Command, Engine, EngineEvent, ImportReport, Outcome};

use super::{Page, SettingsMessage, wrap};
use crate::{
    app::{App, Effect, Message},
    i18n::{error_text, t},
};

/// A stored entry, by its table and id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Key(i64),
    Certificate(i64),
}

/// A row of the table: a trusted SSH host key or FTPS certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    Key(KnownHost),
    Certificate(TrustedCertificate),
}

impl Stored {
    pub fn item(&self) -> Item {
        match self {
            Self::Key(key) => Item::Key(key.id),
            Self::Certificate(certificate) => Item::Certificate(certificate.id),
        }
    }

    /// The server as stored: a name, or a hashed name `|1|salt|hash`.
    pub fn host(&self) -> &str {
        match self {
            Self::Key(key) => &key.host,
            Self::Certificate(certificate) => &certificate.host,
        }
    }

    /// The port; `None` for a hashed name, whose port is inside the hash.
    pub fn port(&self) -> Option<u16> {
        match self {
            Self::Key(key) if key.is_hashed() => None,
            Self::Key(key) => Some(key.port),
            Self::Certificate(certificate) => Some(certificate.port),
        }
    }

    pub fn hashed(&self) -> bool {
        matches!(self, Self::Key(key) if key.is_hashed())
    }

    pub fn fingerprint(&self) -> &str {
        match self {
            Self::Key(key) => &key.fingerprint_sha256,
            Self::Certificate(certificate) => &certificate.fingerprint_sha256,
        }
    }

    /// When it was trusted (UTC, ISO 8601).
    pub fn added(&self) -> &str {
        match self {
            Self::Key(key) => &key.added_at,
            Self::Certificate(certificate) => &certificate.added_at,
        }
    }

    /// The server as the page names it: `host:port`, or the words for a hashed name.
    pub fn server(&self) -> String {
        match self.port() {
            _ if self.hashed() => t!("settings.host_keys.hashed").to_owned(),
            Some(port) => format!("{}:{port}", self.host()),
            None => self.host().to_owned(),
        }
    }
}

/// What the page keeps while the dialog is open.
#[derive(Debug, Default)]
pub struct HostKeysPage {
    /// The search, as typed.
    pub filter: String,
    /// The stored entries that match it, by server; `None` until read.
    pub entries: Option<Vec<Stored>>,
    pub selected: Option<Item>,
    /// The entry whose removal waits for a yes.
    pub asking: Option<Item>,
    /// A removal or an import runs; their buttons wait.
    pub busy: bool,
}

impl HostKeysPage {
    pub fn entry(&self, item: Item) -> Option<&Stored> {
        self.entries
            .as_deref()?
            .iter()
            .find(|stored| stored.item() == item)
    }

    /// The selection one row up (`-1`) or down (`1`); the first row when none is selected.
    fn step(&mut self, by: isize) {
        let Some(entries) = self
            .entries
            .as_deref()
            .filter(|entries| !entries.is_empty())
        else {
            return;
        };
        let at = self
            .selected
            .and_then(|item| entries.iter().position(|stored| stored.item() == item));
        let next = match at {
            None => 0,
            Some(at) => at.saturating_add_signed(by).min(entries.len() - 1),
        };
        self.selected = Some(entries[next].item());
    }
}

#[derive(Debug, Clone)]
pub enum HostKeysMsg {
    Filter(String),
    Select(Item),
    /// Remove pressed: the question about the selected entry.
    Remove,
    ConfirmRemove,
    KeepIt,
    /// Import the user's own `known_hosts`.
    Import,
    Answered(Box<TrustAnswer>),
}

pub fn page_message(message: HostKeysMsg) -> Message {
    wrap(SettingsMessage::HostKeys(message))
}

/// A command for the engine; its answer comes back to the page.
#[derive(Debug)]
pub enum TrustStoreCall {
    List { filter: String },
    Remove(Item),
    Import,
}

/// What became of a [`TrustStoreCall`].
#[derive(Debug, Clone)]
pub enum TrustAnswer {
    /// The entries matching `filter`, or the code of the refusal.
    Listed {
        filter: String,
        result: Result<Vec<Stored>, &'static str>,
    },
    Removed(Result<(), &'static str>),
    Imported(Result<ImportReport, &'static str>),
}

impl TrustStoreCall {
    pub async fn run(self, engine: Engine) -> Message {
        let answer = match self {
            Self::List { filter } => TrustAnswer::Listed {
                result: list(&engine, &filter).await,
                filter,
            },
            Self::Remove(item) => {
                let command = match item {
                    Item::Key(id) => Command::RemoveHostKey { id },
                    Item::Certificate(id) => Command::RemoveCertificate { id },
                };
                TrustAnswer::Removed(dispatch(&engine, command).await.map(|_| ()))
            }
            Self::Import => {
                let command = Command::ImportKnownHosts { path: None };
                TrustAnswer::Imported(match dispatch(&engine, command).await {
                    Ok(Outcome::KnownHostsImported(report)) => Ok(report),
                    Ok(_) => Err("database_error"),
                    Err(code) => Err(code),
                })
            }
        };
        page_message(HostKeysMsg::Answered(Box::new(answer)))
    }
}

async fn dispatch(engine: &Engine, command: Command) -> Result<Outcome, &'static str> {
    engine.dispatch(command).await.map_err(|error| error.code())
}

/// The keys, then the certificates matching `filter`, sorted by server, port and type.
async fn list(engine: &Engine, filter: &str) -> Result<Vec<Stored>, &'static str> {
    let filter = Some(filter.to_owned()).filter(|filter| !filter.trim().is_empty());
    let keys = match dispatch(
        engine,
        Command::ListHostKeys {
            filter: filter.clone(),
        },
    )
    .await?
    {
        Outcome::KnownHosts { keys } => keys,
        _ => return Err("database_error"),
    };
    let certificates = match dispatch(engine, Command::ListCertificates { filter }).await? {
        Outcome::TrustedCertificates { certificates } => certificates,
        _ => return Err("database_error"),
    };
    let mut entries: Vec<Stored> = keys
        .into_iter()
        .map(Stored::Key)
        .chain(certificates.into_iter().map(Stored::Certificate))
        .collect();
    entries.sort_by(|a, b| order(a).cmp(&order(b)));
    Ok(entries)
}

/// Hashed names last, then by server, port and key type; a certificate before the keys.
fn order(stored: &Stored) -> (bool, &str, Option<u16>, &str) {
    let kind = match stored {
        Stored::Key(key) => key.algorithm.as_str(),
        Stored::Certificate(_) => "",
    };
    (stored.hashed(), stored.host(), stored.port(), kind)
}

/// The note an import leaves: how many keys came, stayed, differ and were skipped.
pub fn import_note(report: &ImportReport) -> String {
    t!(
        "settings.host_keys.imported",
        path = report.path,
        added = report.added,
        unchanged = report.unchanged,
        conflicts = report.conflicts.len(),
        skipped = report.skipped.len()
    )
}

fn call(call: TrustStoreCall) -> Vec<Effect> {
    vec![Effect::TrustStore(call)]
}

/// Whether the engine's `event` changes what the page lists.
pub fn concerns(event: &EngineEvent) -> bool {
    matches!(
        event,
        EngineEvent::KnownHostsChanged | EngineEvent::TrustedCertificatesChanged
    )
}

impl App {
    /// The page was turned to: its table is read (again).
    pub(super) fn host_keys_shown(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        if dialog.page != Page::HostKeys {
            return Vec::new();
        }
        let filter = dialog.host_keys.filter.clone();
        call(TrustStoreCall::List { filter })
    }

    /// A key or a certificate was trusted, removed or imported: the shown page reads its table
    /// again.
    pub(in crate::app) fn trust_store_changed(&mut self, event: &EngineEvent) -> Vec<Effect> {
        let shown = self.settings.as_ref().is_some_and(|dialog| {
            dialog.page == Page::HostKeys && dialog.host_keys.entries.is_some()
        });
        if !concerns(event) || !shown {
            return Vec::new();
        }
        self.host_keys_shown()
    }

    /// A message of the page.
    pub(super) fn host_keys_message(&mut self, message: HostKeysMsg) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let page = &mut dialog.host_keys;
        match message {
            HostKeysMsg::Filter(filter) => {
                page.filter = filter.clone();
                return call(TrustStoreCall::List { filter });
            }
            HostKeysMsg::Select(item) => page.selected = Some(item),
            HostKeysMsg::Remove => {
                if !page.busy {
                    page.asking = page.selected.filter(|item| page.entry(*item).is_some());
                }
            }
            HostKeysMsg::KeepIt => page.asking = None,
            HostKeysMsg::ConfirmRemove => {
                if let Some(item) = page.asking.take() {
                    page.busy = true;
                    dialog.note = None;
                    return call(TrustStoreCall::Remove(item));
                }
            }
            HostKeysMsg::Import => {
                if !page.busy {
                    page.busy = true;
                    dialog.note = None;
                    return call(TrustStoreCall::Import);
                }
            }
            HostKeysMsg::Answered(answer) => match *answer {
                // An answer for an older search is dropped: a newer one is on its way.
                TrustAnswer::Listed { filter, .. } if filter != page.filter => {}
                TrustAnswer::Listed {
                    result: Ok(entries),
                    ..
                } => {
                    page.selected = page
                        .selected
                        .filter(|item| entries.iter().any(|stored| stored.item() == *item));
                    page.asking = page.asking.filter(|item| Some(*item) == page.selected);
                    page.entries = Some(entries);
                }
                TrustAnswer::Listed {
                    result: Err(code), ..
                }
                | TrustAnswer::Removed(Err(code))
                | TrustAnswer::Imported(Err(code)) => {
                    page.busy = false;
                    dialog.note = Some((error_text(code), true));
                }
                // The engine's event reads the table again.
                TrustAnswer::Removed(Ok(())) => {
                    page.busy = false;
                    dialog.note = Some((t!("settings.host_keys.removed").to_owned(), false));
                }
                TrustAnswer::Imported(Ok(report)) => {
                    page.busy = false;
                    dialog.note = Some((import_note(&report), !report.conflicts.is_empty()));
                }
            },
        }
        Vec::new()
    }

    /// The page's keys while it is shown; `None` for a key the dialog handles itself (Escape
    /// without a question, Enter, Tab).
    pub(super) fn host_keys_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let dialog = self.settings.as_mut()?;
        if dialog.page != Page::HostKeys || !chord.is_plain() {
            return None;
        }
        let page = &mut dialog.host_keys;
        let key = chord.key.as_str();
        if page.asking.is_some() {
            return match key {
                "Enter" => Some(self.host_keys_message(HostKeysMsg::ConfirmRemove)),
                "Escape" => Some(self.host_keys_message(HostKeysMsg::KeepIt)),
                _ => Some(Vec::new()),
            };
        }
        match key {
            "ArrowUp" => page.step(-1),
            "ArrowDown" => page.step(1),
            "Delete" => return Some(self.host_keys_message(HostKeysMsg::Remove)),
            _ => return None,
        }
        Some(Vec::new())
    }
}

#[cfg(test)]
#[path = "host_keys_tests.rs"]
mod tests;
