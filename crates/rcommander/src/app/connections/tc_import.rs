//! The import from Total Commander of RC-050-09 as a page of the manager, after the ssh_config
//! import's: the preview of a dry run with a box per hotlist item, tab, connection, colour, rule
//! by file type and font (RC-050-14), then the import - and only when the person ticks "Take the passwords", the FTP passwords straight into
//! the vault, which is the one case the master password is asked for.
//!
//! The settings' Import page opens it (RC-050-10), as it takes more than connections: the manager
//! opens on the page alone, drawn over the settings and with the keys before them, and Back,
//! Escape or Close return to the settings. The manager keeps the page for its vault prompt.
//!
//! The files are found by the engine (`%APPDATA%\GHISLER\wincmd.ini`, Total Commander's registry
//! key, `wcx_ftp.ini` beside it); the person may type other paths instead, and the preview shows
//! which files it read.

use std::collections::BTreeSet;

use rc_engine::{
    Chord, Command, ImportStatus, Outcome, TcImportRequest, TcPasswordsRequest,
    import::{
        tc_password::TcPassword,
        total_commander_report::{
            TcImportReport, TcParts, TcPasswordsReport, TcRowStatus, TcTabsMode,
        },
    },
};

use super::{
    ConnectionManager,
    import::Phase,
    jobs::Then,
    preview::{ImportPage, PageKey, page_key},
    run,
    vault::{AfterVault, Ask},
};
use crate::{
    app::{App, Effect},
    i18n::{error_text, t},
};

/// The open page.
#[derive(Debug, Clone)]
pub struct TcImportDialog {
    /// Empty: found by the engine.
    pub wincmd: String,
    pub ftp: String,
    pub report: Option<TcImportReport>,
    /// The keys of the rows to import.
    pub chosen: BTreeSet<String>,
    pub replace_tabs: bool,
    /// Take the FTP passwords into the vault after the import.
    pub passwords: bool,
    /// What the vault took.
    pub stored: Option<TcPasswordsReport>,
    /// The phase, the error and the `wincmd.ini` and `wcx_ftp.ini` fields the preview was read
    /// with, which the import and its passwords take (RC-080-39).
    pub page: ImportPage<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum TcImportMsg {
    /// The settings' "Import from Total Commander..." button.
    Open,
    Wincmd(String),
    Ftp(String),
    /// Read the files again.
    Read,
    Choose(String, bool),
    ReplaceTabs(bool),
    Passwords(bool),
    Run,
    /// Back to the tree, or to the settings.
    Back,
}

/// The status's code, for its text `tc_import.status.<code>`.
pub fn row_status_code(status: &TcRowStatus) -> &'static str {
    match status {
        TcRowStatus::New => "new",
        TcRowStatus::Created => "created",
        TcRowStatus::Exists => "exists",
        TcRowStatus::Update => "update",
        TcRowStatus::Updated => "updated",
        TcRowStatus::Missing => "missing",
        TcRowStatus::Skipped { .. } => "skipped",
    }
}

/// Whether a row of the hotlist or the tabs can be chosen.
pub fn row_importable(status: &TcRowStatus) -> bool {
    matches!(
        status,
        TcRowStatus::New | TcRowStatus::Update | TcRowStatus::Missing
    )
}

/// The code of a password state, for its text `tc_import.password.<code>`.
pub fn password_code(state: TcPassword) -> &'static str {
    match state {
        TcPassword::None => "none",
        TcPassword::Present => "present",
        TcPassword::MasterPassword => "master_password",
        TcPassword::Unreadable => "unreadable",
    }
}

/// A path field's text as a request takes it.
fn typed(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

impl TcImportDialog {
    fn new() -> Self {
        Self {
            wincmd: String::new(),
            ftp: String::new(),
            report: None,
            chosen: BTreeSet::new(),
            replace_tabs: false,
            passwords: false,
            stored: None,
            page: ImportPage::new(Phase::Reading),
        }
    }

    /// The preview of the paths typed now: the effect that reads them.
    fn read(&mut self) -> Effect {
        let paths = (self.wincmd.clone(), self.ftp.clone());
        let command = self.command(&paths, true);
        run(Then::TcPreview(self.page.read(paths)), command)
    }

    /// A path changed: the preview no longer belongs to it and is read again.
    fn set_path(&mut self, ftp: bool, path: String) {
        let field = if ftp { &mut self.ftp } else { &mut self.wincmd };
        if *field == path {
            return;
        }
        *field = path;
        self.page.changed();
        self.report = None;
        self.chosen.clear();
        self.stored = None;
    }

    /// The request for the files `(wincmd, ftp)`.
    fn command(&self, (wincmd, ftp): &(String, String), dry_run: bool) -> Command {
        Command::ImportTotalCommander(TcImportRequest {
            wincmd_ini: typed(wincmd),
            ftp_ini: typed(ftp),
            parts: TcParts::default(),
            dry_run,
            tabs_mode: if self.replace_tabs {
                TcTabsMode::Replace
            } else {
                TcTabsMode::Append
            },
            only: (!dry_run).then(|| self.chosen.iter().cloned().collect()),
        })
    }

    /// The passwords to take after the import: of the chosen connections that have one.
    fn passwords_command(&self) -> Option<Command> {
        let report = self.report.as_ref()?;
        let (wincmd, _) = self.page.held.source()?;
        let only: Vec<String> = report
            .connections
            .iter()
            .filter(|row| row.password_state == TcPassword::Present)
            .filter(|row| {
                matches!(
                    row.status,
                    ImportStatus::Created { .. }
                        | ImportStatus::Updated { .. }
                        | ImportStatus::Unchanged { .. }
                )
            })
            .map(|row| row.key.clone())
            .filter(|key| self.chosen.contains(key))
            .collect();
        (self.passwords && !only.is_empty()).then(|| {
            Command::ImportTotalCommanderPasswords(TcPasswordsRequest {
                wincmd_ini: typed(wincmd),
                ftp_ini: report.ftp_ini.clone(),
                only: Some(only),
            })
        })
    }
}

impl App {
    /// The import from the settings: on the open manager, or on one opened for it alone.
    pub(super) fn open_tc_import(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.connections.is_none() {
            effects = self.open_connections();
            if let Some(manager) = &mut self.connections {
                manager.import_only = true;
            }
        }
        if let Some(manager) = &mut self.connections {
            effects.extend(manager.open_tc_import());
        }
        effects
    }
}

impl ConnectionManager {
    /// Whether the manager is drawn over the settings and has the keys before them: while the
    /// import page they opened is shown.
    pub fn over_settings(&self) -> bool {
        self.tc_import.is_some()
    }

    /// Back, Escape: the page closes, and the manager with it when it was opened for the page.
    fn leave_tc_import(&mut self) {
        self.tc_import = None;
        self.close = self.import_only;
    }

    /// The page opens on a preview of what the engine finds by itself.
    pub(super) fn open_tc_import(&mut self) -> Vec<Effect> {
        let mut dialog = TcImportDialog::new();
        let preview = dialog.read();
        self.tc_import = Some(dialog);
        self.pending = None;
        vec![preview]
    }

    pub(super) fn on_tc_import(&mut self, message: TcImportMsg) -> Vec<Effect> {
        let Some(dialog) = self.tc_import.as_mut() else {
            return Vec::new();
        };
        // The sources hold still while the import runs.
        let rereads = matches!(
            message,
            TcImportMsg::Wincmd(_)
                | TcImportMsg::Ftp(_)
                | TcImportMsg::Read
                | TcImportMsg::ReplaceTabs(_)
        );
        if rereads && dialog.page.importing() {
            return Vec::new();
        }
        match message {
            TcImportMsg::Open => {}
            TcImportMsg::Wincmd(path) => dialog.set_path(false, path),
            TcImportMsg::Ftp(path) => dialog.set_path(true, path),
            TcImportMsg::Read => return vec![dialog.read()],
            TcImportMsg::Choose(key, on) => {
                if on {
                    dialog.chosen.insert(key);
                } else {
                    dialog.chosen.remove(&key);
                }
            }
            TcImportMsg::ReplaceTabs(on) => {
                dialog.replace_tabs = on;
                // Which tabs are there already depends on it.
                return vec![dialog.read()];
            }
            TcImportMsg::Passwords(on) => dialog.passwords = on,
            TcImportMsg::Run => return self.run_tc_import(),
            TcImportMsg::Back => self.leave_tc_import(),
        }
        Vec::new()
    }

    fn run_tc_import(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.tc_import.as_mut() else {
            return Vec::new();
        };
        let Some(paths) = dialog.page.ready_source() else {
            return Vec::new();
        };
        if dialog.chosen.is_empty() {
            return Vec::new();
        }
        let command = dialog.command(paths, false);
        dialog.page.phase = Phase::Importing;
        vec![run(Then::TcImported, command)]
    }

    /// The preview, the import or the passwords answered.
    pub(super) fn tc_import_answer(
        &mut self,
        then: Then,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        let Some(dialog) = self.tc_import.as_mut() else {
            return Vec::new();
        };
        // A preview of paths the page no longer shows.
        if let Then::TcPreview(read) = then
            && !dialog.page.held.answers(read)
        {
            return Vec::new();
        }
        let report = match (then, result) {
            (_, Ok(Outcome::TcImported(report))) => report,
            (Then::TcPasswords, Ok(Outcome::TcPasswordsImported(stored))) => {
                self.note = Some((
                    t!("tc_import.passwords_stored", count = stored.stored.len()),
                    false,
                ));
                dialog.stored = Some(stored);
                return Vec::new();
            }
            (Then::TcPasswords, Err(code)) => {
                if let Some(ask) = Ask::for_code(code)
                    && let Some(command) = dialog.passwords_command()
                {
                    return self.ask_vault(ask, AfterVault::TcPasswords(Box::new(command)));
                }
                self.note = Some((error_text(code), true));
                return Vec::new();
            }
            (_, Ok(_)) => return Vec::new(),
            (then, Err(code)) => {
                dialog.page.failed(code, then == Then::TcImported);
                return Vec::new();
            }
        };
        if let Then::TcPreview(_) = then {
            dialog.chosen = selected_keys(&report);
            dialog.report = Some(report);
            dialog.page.phase = Phase::Ready;
            return Vec::new();
        }
        dialog.report = Some(report);
        dialog.page.phase = Phase::Done;
        let Some(command) = dialog.passwords_command() else {
            return Vec::new();
        };
        match self.vault.and_then(Ask::for_state) {
            Some(ask) => self.ask_vault(ask, AfterVault::TcPasswords(Box::new(command))),
            None => vec![run(Then::TcPasswords, command)],
        }
    }

    /// A key while the page is shown: Escape leaves it, Enter imports.
    pub(super) fn tc_import_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        match page_key(chord, captured) {
            Some(PageKey::Leave) => {
                if self.prompt.take().is_none() {
                    self.leave_tc_import();
                }
                Vec::new()
            }
            Some(PageKey::Import) if self.prompt.is_none() => self.run_tc_import(),
            _ => Vec::new(),
        }
    }
}

/// The keys the engine selected in a preview.
fn selected_keys(report: &TcImportReport) -> BTreeSet<String> {
    let hotlist = report.hotlist.iter().map(|row| (row.selected, &row.key));
    let tabs = report.tabs.iter().map(|row| (row.selected, &row.key));
    let connections = report
        .connections
        .iter()
        .map(|row| (row.selected, &row.key));
    let look = report.look.iter().map(|row| (row.selected, &row.key));
    hotlist
        .chain(tabs)
        .chain(connections)
        .chain(look)
        .filter(|(selected, _)| *selected)
        .map(|(_, key)| key.clone())
        .collect()
}

#[cfg(test)]
#[path = "tc_import_tests.rs"]
mod tests;
