//! The import from Remote Desktop Manager of RC-050-11 as a page of the manager, after the
//! ssh_config import's: the export the person picks, the preview of a dry run with a box per
//! entry, then the import. New connections go into "Imported from Remote Desktop Manager", RDM's
//! folders inside; a second import finds its entries again by RDM's ID.
//!
//! No password is taken: RDM keeps them obfuscated with its own undocumented scheme, and the
//! page only says which entries have one, to be entered again in the connection. Escape and Back
//! return to the tree.

use std::collections::BTreeSet;

use rc_engine::{
    Chord, Command, Outcome, RdmImportRequest,
    import::rdm_report::{RdmImportReport, RdmRow},
};

use super::{
    ConnectionManager,
    import::{Phase, importable},
    jobs::Then,
    preview::{ImportPage, PageKey, page_key},
    run,
};
use crate::app::Effect;

/// The open page.
#[derive(Debug, Clone)]
pub struct RdmImportDialog {
    /// The export to read.
    pub path: String,
    pub report: Option<RdmImportReport>,
    /// The keys of the entries to import.
    pub chosen: BTreeSet<String>,
    /// The phase, the error and the path the preview was read from (RC-080-39).
    pub page: ImportPage<String>,
}

#[derive(Debug, Clone)]
pub enum RdmImportMsg {
    /// The button under the tree.
    Open,
    Path(String),
    /// Read the export.
    Read,
    Choose(String, bool),
    Run,
    /// Back to the tree.
    Back,
}

/// Whether an entry of the preview can be chosen: a new or a changed one.
pub fn row_importable(row: &RdmRow) -> bool {
    importable(&row.status)
}

impl RdmImportDialog {
    /// The path changed: the preview no longer belongs to it and is read again.
    fn set_path(&mut self, path: String) {
        if path == self.path {
            return;
        }
        self.path = path;
        self.page.changed();
        self.report = None;
        self.chosen.clear();
    }

    fn command(&self, path: &str, dry_run: bool) -> Command {
        Command::ImportRemoteDesktopManager(RdmImportRequest {
            path: path.trim().to_owned(),
            dry_run,
            only: (!dry_run).then(|| self.chosen.iter().cloned().collect()),
        })
    }
}

impl ConnectionManager {
    /// The page opens empty: there is no place RDM keeps its exports.
    fn open_rdm_import(&mut self) {
        self.rdm_import = Some(RdmImportDialog {
            path: String::new(),
            report: None,
            chosen: BTreeSet::new(),
            // Nothing read yet, nothing to import.
            page: ImportPage::new(Phase::Done),
        });
        self.pending = None;
    }

    pub(super) fn on_rdm_import(&mut self, message: RdmImportMsg) -> Vec<Effect> {
        if let RdmImportMsg::Open = message {
            self.open_rdm_import();
            return Vec::new();
        }
        let Some(dialog) = self.rdm_import.as_mut() else {
            return Vec::new();
        };
        // The source holds still while the import runs.
        let rereads = matches!(message, RdmImportMsg::Path(_) | RdmImportMsg::Read);
        if rereads && dialog.page.importing() {
            return Vec::new();
        }
        match message {
            RdmImportMsg::Open => {}
            RdmImportMsg::Path(path) => dialog.set_path(path),
            RdmImportMsg::Read => {
                if dialog.path.trim().is_empty() {
                    return Vec::new();
                }
                let command = dialog.command(&dialog.path, true);
                let read = dialog.page.read(dialog.path.clone());
                return vec![run(Then::RdmPreview(read), command)];
            }
            RdmImportMsg::Choose(key, on) => {
                if on {
                    dialog.chosen.insert(key);
                } else {
                    dialog.chosen.remove(&key);
                }
            }
            RdmImportMsg::Run => return self.run_rdm_import(),
            RdmImportMsg::Back => self.rdm_import = None,
        }
        Vec::new()
    }

    fn run_rdm_import(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.rdm_import.as_mut() else {
            return Vec::new();
        };
        let Some(path) = dialog.page.ready_source() else {
            return Vec::new();
        };
        if dialog.chosen.is_empty() {
            return Vec::new();
        }
        let command = dialog.command(path, false);
        dialog.page.phase = Phase::Importing;
        vec![run(Then::RdmImported, command)]
    }

    /// The preview or the import answered.
    pub(super) fn rdm_import_answer(
        &mut self,
        then: Then,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        let Some(dialog) = self.rdm_import.as_mut() else {
            return Vec::new();
        };
        // A preview of a path the page no longer shows.
        if let Then::RdmPreview(read) = then
            && !dialog.page.held.answers(read)
        {
            return Vec::new();
        }
        let report = match result {
            Ok(Outcome::RdmImported(report)) => report,
            Ok(_) => return Vec::new(),
            Err(code) => {
                dialog.page.failed(code, then == Then::RdmImported);
                if let Then::RdmPreview(_) = then {
                    dialog.report = None;
                    dialog.chosen.clear();
                }
                return Vec::new();
            }
        };
        if let Then::RdmPreview(_) = then {
            dialog.chosen = report
                .entries
                .iter()
                .filter(|row| row.selected && row_importable(row))
                .map(|row| row.key.clone())
                .collect();
            dialog.report = Some(report);
            dialog.page.phase = Phase::Ready;
            return Vec::new();
        }
        dialog.report = Some(report);
        dialog.page.phase = Phase::Done;
        Vec::new()
    }

    /// A key while the page is shown: Escape goes back to the tree, Enter imports.
    pub(super) fn rdm_import_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        match page_key(chord, captured) {
            Some(PageKey::Leave) => {
                self.rdm_import = None;
                Vec::new()
            }
            Some(PageKey::Import) => self.run_rdm_import(),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "rdm_import_tests.rs"]
mod tests;
