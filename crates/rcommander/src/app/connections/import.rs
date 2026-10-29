//! The ssh_config import of RC-050-06 as a page of the manager: the preview of a dry run, a box
//! per host, the target folder, then the import - and, per host and only when asked, its key
//! file taken into the vault, which is the one case the master password is asked for. An
//! encrypted key asks for its passphrase in its host's row and is taken again with it; a key
//! that cannot be taken says why in its row, and the other keys go on.
//!
//! The source is the person's own `~/.ssh/config` (an empty path) or one typed or filled in
//! from a WSL distribution's share; the engine reads only `~/.ssh/config` or a file in
//! `~/.ssh/config.d/` of either, and refuses everything else (`ssh_config_path_refused`).

mod wsl;

use std::path::PathBuf;

use rc_engine::{
    Chord, Command, ConnectionId, FolderId, ImportStatus, Outcome, SecretParam, SshConfigReport,
};
use rc_vault::{ExposeSecret as _, SecretString};

use self::wsl::wsl_config;
pub use self::wsl::wsl_roots;
use super::{
    ConnJob, ConnectionManager,
    jobs::{KeyToTake, Then},
    preview::{ImportPage, PageKey, page_key},
    run,
    vault::{AfterVault, Ask},
};
use crate::{app::Effect, i18n::t};

pub use super::preview::Phase;

/// The open import page.
#[derive(Debug, Clone)]
pub struct ImportDialog {
    /// Empty: the person's own `~/.ssh/config`.
    pub path: String,
    /// The WSL distributions by name and share, to fill the path from.
    pub wsl: Vec<(String, PathBuf)>,
    /// Where new connections go; the root's "Imported from ssh config" for `None`.
    pub folder: Option<FolderId>,
    pub report: Option<SshConfigReport>,
    /// Per host of the report: import it.
    pub chosen: Vec<bool>,
    /// Per host of the report: take its key file into the vault.
    pub take_key: Vec<bool>,
    /// The keys tried, by host alias: taken, or why not.
    pub keys: Vec<KeyRow>,
    /// The phase, the error and the path the preview was read from (RC-080-39).
    pub page: ImportPage<String>,
}

/// A host's key on its way into the vault.
#[derive(Debug, Clone)]
pub struct KeyRow {
    pub alias: String,
    pub result: Result<(), &'static str>,
    /// Typed for an encrypted key; dropped with each answer, never shown.
    pub passphrase: SecretString,
}

impl KeyRow {
    /// Whether the key waits for its passphrase.
    pub fn wants_passphrase(&self) -> bool {
        matches!(
            self.result,
            Err("key_passphrase_needed" | "key_passphrase_wrong")
        )
    }
}

#[derive(Debug, Clone)]
pub enum ImportMsg {
    /// The manager's "Import ssh config..." button.
    Open,
    Path(String),
    /// The path of the WSL distribution at this place of the list.
    Wsl(usize),
    /// Read the file again.
    Read,
    Choose(usize, bool),
    TakeKey(usize, bool),
    /// The passphrase typed for the key of the host at this place.
    Passphrase(usize, SecretString),
    /// Take that host's key again, with its passphrase.
    TakeAgain(usize),
    Folder(Option<FolderId>),
    Run,
    /// Back to the tree.
    Back,
}

impl ImportDialog {
    /// The aliases chosen for the import.
    fn chosen_hosts(&self) -> Vec<String> {
        let Some(report) = &self.report else {
            return Vec::new();
        };
        report
            .hosts
            .iter()
            .zip(&self.chosen)
            .filter(|(_, chosen)| **chosen)
            .map(|(host, _)| host.alias.clone())
            .collect()
    }

    /// Whether the host at `at` has a connection already, imported before.
    pub fn has_connection(&self, at: usize) -> bool {
        self.report
            .as_ref()
            .and_then(|report| report.hosts.get(at))
            .is_some_and(|host| imported_id(&host.status).is_some())
    }

    /// The key row of the host at `at`.
    pub fn key_row(&self, at: usize) -> Option<&KeyRow> {
        let alias = &self.report.as_ref()?.hosts.get(at)?.alias;
        self.keys.iter().find(|row| row.alias == *alias)
    }

    /// The preview of the path typed now: the effect that reads it.
    fn read(&mut self) -> Effect {
        let read = self.page.read(self.path.clone());
        run(Then::Preview(read), self.command(&self.path, true))
    }

    /// The path changed: the preview no longer belongs to it and is read again.
    fn set_path(&mut self, path: String) {
        if path == self.path {
            return;
        }
        self.path = path;
        self.page.changed();
        self.forget_report();
    }

    /// No preview shown: its hosts, choices and keys go with it.
    fn forget_report(&mut self) {
        self.report = None;
        self.chosen.clear();
        self.take_key.clear();
        self.keys.clear();
    }

    fn command(&self, path: &str, dry_run: bool) -> Command {
        let path = path.trim();
        Command::ImportSshConfig(rc_engine::SshConfigImportRequest {
            path: (!path.is_empty()).then(|| path.to_owned()),
            dry_run,
            hosts: (!dry_run).then(|| self.chosen_hosts()),
            folder_id: self.folder,
        })
    }
}

/// The connection an import created or found for a host; none for a new or refused one.
pub fn imported_id(status: &ImportStatus) -> Option<ConnectionId> {
    match status {
        ImportStatus::Created { id }
        | ImportStatus::Updated { id }
        | ImportStatus::Unchanged { id }
        | ImportStatus::Update { id } => Some(*id),
        ImportStatus::New | ImportStatus::Refused { .. } => None,
    }
}

/// Whether a host of the preview can be imported: a new one, or one with changes.
pub fn importable(status: &ImportStatus) -> bool {
    matches!(status, ImportStatus::New | ImportStatus::Update { .. })
}

/// The status's code, for its text `ssh_import.status.<code>`.
pub fn status_code(status: &ImportStatus) -> &'static str {
    match status {
        ImportStatus::New => "new",
        ImportStatus::Created { .. } => "created",
        ImportStatus::Update { .. } => "update",
        ImportStatus::Updated { .. } => "updated",
        ImportStatus::Unchanged { .. } => "unchanged",
        ImportStatus::Refused { .. } => "refused",
    }
}

impl ConnectionManager {
    /// The page opens on a preview of the person's own config.
    pub(super) fn open_import(&mut self, wsl: Vec<(String, PathBuf)>) -> Vec<Effect> {
        let mut dialog = ImportDialog {
            path: String::new(),
            wsl,
            folder: None,
            report: None,
            chosen: Vec::new(),
            take_key: Vec::new(),
            keys: Vec::new(),
            page: ImportPage::new(Phase::Reading),
        };
        let preview = dialog.read();
        self.import = Some(dialog);
        self.pending = None;
        vec![preview]
    }

    pub(super) fn on_import(&mut self, message: ImportMsg) -> Vec<Effect> {
        let Some(dialog) = self.import.as_mut() else {
            return Vec::new();
        };
        // The source holds still while the import runs.
        let rereads = matches!(
            message,
            ImportMsg::Path(_) | ImportMsg::Wsl(_) | ImportMsg::Read | ImportMsg::Folder(_)
        );
        if rereads && dialog.page.importing() {
            return Vec::new();
        }
        match message {
            ImportMsg::Open => {}
            ImportMsg::Path(path) => dialog.set_path(path),
            ImportMsg::Wsl(at) => {
                if let Some(path) = dialog.wsl.get(at).map(|(_, root)| wsl_config(root)) {
                    dialog.set_path(path);
                }
            }
            ImportMsg::Read => return vec![dialog.read()],
            ImportMsg::Choose(at, on) => {
                if let Some(chosen) = dialog.chosen.get_mut(at) {
                    *chosen = on;
                }
                // A host left out has no connection whose key could be taken.
                if !on
                    && !dialog.has_connection(at)
                    && let Some(take) = dialog.take_key.get_mut(at)
                {
                    *take = false;
                }
            }
            ImportMsg::TakeKey(at, on) => {
                if let Some(take) = dialog.take_key.get_mut(at) {
                    *take = on;
                }
            }
            ImportMsg::Passphrase(at, typed) => {
                let alias = dialog
                    .report
                    .as_ref()
                    .and_then(|report| report.hosts.get(at))
                    .map(|host| host.alias.clone());
                if let Some(row) = dialog
                    .keys
                    .iter_mut()
                    .find(|row| Some(&row.alias) == alias.as_ref())
                {
                    row.passphrase = typed;
                }
            }
            ImportMsg::TakeAgain(at) => return self.take_again(at),
            ImportMsg::Folder(folder) => {
                dialog.folder = folder;
                return vec![dialog.read()];
            }
            ImportMsg::Run => return self.run_import(),
            ImportMsg::Back => self.import = None,
        }
        Vec::new()
    }

    fn run_import(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.import.as_mut() else {
            return Vec::new();
        };
        let Some(path) = dialog.page.ready_source() else {
            return Vec::new();
        };
        if dialog.chosen_hosts().is_empty() {
            return Vec::new();
        }
        let command = dialog.command(path, false);
        dialog.page.phase = Phase::Importing;
        vec![run(Then::Imported, command)]
    }

    /// The preview or the import answered.
    pub(super) fn import_answer(
        &mut self,
        then: Then,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        let Some(dialog) = self.import.as_mut() else {
            return Vec::new();
        };
        // A preview of a path the page no longer shows.
        if let Then::Preview(read) = then
            && !dialog.page.held.answers(read)
        {
            return Vec::new();
        }
        let report = match result {
            Ok(Outcome::SshConfigImported(report)) => report,
            Ok(_) => return Vec::new(),
            Err(code) => {
                // A failed preview shows no hosts of an earlier one (RC-080-53).
                if then != Then::Imported {
                    dialog.forget_report();
                }
                dialog.page.failed(code, then == Then::Imported);
                return Vec::new();
            }
        };
        if let Then::Preview(_) = then {
            dialog.chosen = report
                .hosts
                .iter()
                .map(|host| host.selected && importable(&host.status))
                .collect();
            dialog.take_key = vec![false; report.hosts.len()];
            dialog.keys.clear();
            dialog.report = Some(report);
            dialog.page.phase = Phase::Ready;
            return Vec::new();
        }
        dialog.report = Some(report);
        dialog.page.phase = Phase::Done;
        let keys = keys_to_take(dialog, None);
        self.take_keys(keys)
    }

    /// The key of the host at `at` again, with the passphrase typed in its row.
    fn take_again(&mut self, at: usize) -> Vec<Effect> {
        let Some(dialog) = self.import.as_ref() else {
            return Vec::new();
        };
        let Some(row) = dialog.key_row(at).filter(|row| row.wants_passphrase()) else {
            return Vec::new();
        };
        if row.passphrase.expose_secret().is_empty() {
            return Vec::new();
        }
        let keys = keys_to_take(dialog, Some(std::slice::from_ref(&row.alias)));
        self.take_keys(keys)
    }

    /// `keys` into the vault, after the master-password prompt when the vault is not open.
    fn take_keys(&mut self, keys: Vec<KeyToTake>) -> Vec<Effect> {
        if keys.is_empty() {
            return Vec::new();
        }
        match self.vault.and_then(Ask::for_state) {
            Some(ask) => self.ask_vault(ask, AfterVault::TakeKeys(keys)),
            None => vec![Effect::Connections(ConnJob::TakeKeys(keys))],
        }
    }

    /// The keys are in the vault, or why one is not: each answer goes into its host's row.
    pub(super) fn keys_taken(
        &mut self,
        taken: Vec<(String, Result<(), &'static str>)>,
    ) -> Vec<Effect> {
        // The vault was locked meanwhile - before whichever key, not only the first
        // (RC-080-53): the others are answered, and those keys are tried again after the ask.
        let (locked, answered): (Vec<_>, Vec<_>) = taken
            .into_iter()
            .partition(|(_, result)| result.is_err_and(|code| Ask::for_code(code).is_some()));
        self.record_keys(answered);
        let Some(ask) = locked
            .iter()
            .find_map(|(_, result)| result.err().and_then(Ask::for_code))
        else {
            return Vec::new();
        };
        self.vault = Some(ask.state());
        let aliases: Vec<String> = locked.into_iter().map(|(alias, _)| alias).collect();
        let keys = self
            .import
            .as_ref()
            .map(|dialog| keys_to_take(dialog, Some(&aliases)))
            .unwrap_or_default();
        self.ask_vault(ask, AfterVault::TakeKeys(keys))
    }

    /// Each answer into its host's row, and the note over all rows.
    fn record_keys(&mut self, taken: Vec<(String, Result<(), &'static str>)>) {
        if taken.is_empty() {
            return;
        }
        let Some(dialog) = self.import.as_mut() else {
            return;
        };
        for (alias, result) in taken {
            let answered = KeyRow {
                alias,
                result,
                passphrase: SecretString::from(""),
            };
            match dialog
                .keys
                .iter_mut()
                .find(|row| row.alias == answered.alias)
            {
                Some(row) => *row = answered,
                None => dialog.keys.push(answered),
            }
        }
        let failed = dialog.keys.iter().filter(|row| row.result.is_err()).count();
        let tried = dialog.keys.len();
        self.note = Some(if failed == 0 {
            (t!("connections.import.keys_taken", count = tried), false)
        } else {
            (t!("connections.import.keys_failed", count = failed), true)
        });
    }

    /// A key while the page is shown: Escape goes back to the tree, Enter imports.
    pub(super) fn import_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        match page_key(chord, captured) {
            Some(PageKey::Leave) => {
                if self.prompt.take().is_none() {
                    self.import = None;
                }
                Vec::new()
            }
            Some(PageKey::Import) if self.prompt.is_none() => self.run_import(),
            _ => Vec::new(),
        }
    }
}

/// The key files of the imported hosts marked for the vault - of those `only` names, or all -
/// each with the passphrase typed in its row.
fn keys_to_take(dialog: &ImportDialog, only: Option<&[String]>) -> Vec<KeyToTake> {
    let Some(report) = &dialog.report else {
        return Vec::new();
    };
    report
        .hosts
        .iter()
        .zip(&dialog.take_key)
        .filter(|(host, take)| **take && only.is_none_or(|only| only.contains(&host.alias)))
        .filter_map(|(host, _)| {
            let passphrase = dialog
                .keys
                .iter()
                .find(|row| row.alias == host.alias)
                .map(|row| row.passphrase.expose_secret())
                .filter(|typed| !typed.is_empty())
                .map(|typed| SecretParam::new(typed.to_owned()));
            Some(KeyToTake {
                alias: host.alias.clone(),
                id: imported_id(&host.status)?,
                path: host.identity_files.first()?.clone(),
                passphrase,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
