//! The database dialog (RC-010-12): back the database up to a file a person chooses, or check a
//! backup and stage it to replace the database at the next start; export settings, connections,
//! trusted keys and the hotlist to a file and import them from one (RC-080-19, `exchange.rs`).
//!
//! It has no key: the command palette's "database" row opens it, and so does Configuration >
//! Database... in the menu bar (RC-080-18), which runs that row. A path field with a
//! suggestion - `Documents\rCommander Backups\rcommander-<stamp>.db`, the folder made on demand -
//! and "Browse..." beside it, Windows' own save or open dialog (RC-010-18). The work is the
//! engine's: `db.backup_to` and `db.restore`, both `GuiOnly`; progress and the result (path and
//! size) show in the dialog and in the status bar. While the dialog is open the keys belong to
//! it: Enter confirms, Escape closes, everything else goes to the path field.

mod exchange;
mod exchange_view;
mod view;

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use iced::Task;
use rc_engine::{
    Chord, Command, Engine, Outcome,
    exchange::{EXTENSION, ExportParts, RowStatus},
    settings::{
        EXPORT_CONNECTIONS_KEY, EXPORT_ENCRYPTED_KEY, EXPORT_HOTLIST_KEY, EXPORT_SETTINGS_KEY,
        EXPORT_TRUST_KEY, stored_bool,
    },
};

pub use self::exchange::{ExchangeForm, ExchangeMsg, Part};
pub(super) use self::view::{PATH_FIELD, body, overlay, title};
use super::{App, Effect, Message, vault_ask::Ask};
use crate::{
    format,
    i18n::{error_text, t},
};

/// The folder in the documents the dialog suggests for backups.
const SUGGESTED_FOLDER: &str = "rCommander Backups";

/// Backing up or restoring the database, exporting or importing settings and connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogMode {
    Backup,
    Restore,
    Export,
    Import,
}

/// Where the dialog stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Editing,
    Running,
    Done(String),
    Failed(String),
}

/// The open dialog.
#[derive(Debug, Clone)]
pub struct DatabaseDialog {
    pub mode: DialogMode,
    /// A file, or for a backup also a folder (a generated name goes into it).
    pub path: String,
    /// Replace an existing backup file.
    pub overwrite: bool,
    pub phase: Phase,
    /// The export's and the import's fields.
    pub exchange: ExchangeForm,
    /// The suggested folder, made when the backup goes into it.
    folder: Option<PathBuf>,
}

impl DatabaseDialog {
    fn new(mode: DialogMode, folder: Option<PathBuf>) -> Self {
        let stamp = || chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let path = match (mode, &folder) {
            (DialogMode::Backup, Some(folder)) => folder
                .join(rc_db::backup_file_name(&stamp()))
                .display()
                .to_string(),
            (DialogMode::Export, Some(folder)) => folder
                .join(format!("rcommander-{}.{EXTENSION}", stamp()))
                .display()
                .to_string(),
            (DialogMode::Restore | DialogMode::Import, Some(folder)) => {
                format!("{}{MAIN_SEPARATOR}", folder.display())
            }
            (_, None) => String::new(),
        };
        Self {
            mode,
            path,
            overwrite: false,
            phase: Phase::Editing,
            exchange: ExchangeForm::default(),
            folder,
        }
    }
}

/// What the dialog's controls say.
#[derive(Debug, Clone)]
pub enum DatabaseMsg {
    /// The status bar's entry: the dialog opens on a backup.
    Open,
    /// The palette's, the menu bar's and the settings' entries: the dialog opens on `mode`.
    OpenMode(DialogMode),
    /// The export's and the import's fields (RC-080-19).
    Exchange(ExchangeMsg),
    Mode(DialogMode),
    Path(String),
    Overwrite(bool),
    Confirm,
    Close,
    /// The engine's answer: the outcome, or the code of the failure.
    Finished(Box<Result<Outcome, &'static str>>),
}

/// The commands of the dialog for the engine, run in order - the vault's answer before an
/// export or an import that needed it - and the suggested folder to make first.
pub struct DatabaseJob {
    commands: Vec<Command>,
    make_folder: Option<PathBuf>,
}

impl DatabaseJob {
    /// Runs the job: off the window's thread in the app, inline in the tests. The last
    /// command's answer, or the first failure's code.
    pub async fn run(self, engine: Engine) -> Result<Outcome, &'static str> {
        if let Some(folder) = self.make_folder {
            tokio::fs::create_dir_all(&folder)
                .await
                .map_err(|_| "backup_target_unwritable")?;
        }
        let mut last = Err("database_unavailable");
        for command in self.commands {
            last = Ok(engine
                .dispatch(command)
                .await
                .map_err(|error| error.code())?);
        }
        last
    }
}

/// `Documents\rCommander Backups`, where the platform names a documents folder.
fn suggested_folder() -> Option<PathBuf> {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(|docs| docs.join(SUGGESTED_FOLDER)))
}

impl App {
    /// A message of the dialog.
    pub(super) fn on_database(&mut self, message: DatabaseMsg) -> Vec<Effect> {
        match message {
            DatabaseMsg::Open => return self.open_database(DialogMode::Backup),
            DatabaseMsg::OpenMode(mode) => return self.open_database(mode),
            DatabaseMsg::Mode(mode) => {
                if let Some(dialog) = self.database.as_mut()
                    && dialog.phase != Phase::Running
                {
                    let (parts, encrypted) = (dialog.exchange.parts, dialog.exchange.encrypted);
                    *dialog = DatabaseDialog::new(mode, dialog.folder.clone());
                    dialog.exchange.parts = parts;
                    dialog.exchange.encrypted = encrypted;
                }
            }
            DatabaseMsg::Exchange(ExchangeMsg::Read) => return self.read_export(),
            DatabaseMsg::Exchange(message) => {
                if let Some(dialog) = self.editable() {
                    dialog.exchange.update(message);
                    if let Phase::Failed(_) = dialog.phase {
                        dialog.phase = Phase::Editing;
                    }
                }
            }
            DatabaseMsg::Path(path) => {
                if let Some(dialog) = self.editable() {
                    dialog.path = path;
                    dialog.phase = Phase::Editing;
                    // Another file: its preview no longer holds.
                    dialog.exchange.report = None;
                }
            }
            DatabaseMsg::Overwrite(overwrite) => {
                if let Some(dialog) = self.editable() {
                    dialog.overwrite = overwrite;
                    dialog.phase = Phase::Editing;
                }
            }
            DatabaseMsg::Confirm => return self.confirm_database(),
            // A running job goes on; its result lands in the status bar.
            DatabaseMsg::Close => self.database = None,
            DatabaseMsg::Finished(result) => self.database_finished(*result),
        }
        Vec::new()
    }

    /// A key while the dialog is open: Enter confirms (or closes a finished dialog), Escape
    /// closes; the path field takes the rest.
    pub(super) fn database_key(&mut self, chord: &Chord) -> Vec<Effect> {
        // Tab and Shift+Tab walk the focus ring (RC-080-16).
        if let Some(effects) = super::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        match chord.key.as_str() {
            "Escape" => self.database = None,
            "Enter" => {
                let finished = self
                    .database
                    .as_ref()
                    .is_some_and(|dialog| matches!(dialog.phase, Phase::Done(_)));
                let unread = self.database.as_ref().is_some_and(|dialog| {
                    dialog.mode == DialogMode::Import && dialog.exchange.report.is_none()
                });
                if finished {
                    self.database = None;
                } else if unread {
                    return self.read_export();
                } else {
                    return self.confirm_database();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// The dialog, unless its job runs.
    fn editable(&mut self) -> Option<&mut DatabaseDialog> {
        self.database
            .as_mut()
            .filter(|dialog| dialog.phase != Phase::Running)
    }

    fn confirm_database(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.editable() else {
            return Vec::new();
        };
        let path = dialog.path.trim().to_owned();
        if path.is_empty() || matches!(dialog.phase, Phase::Done(_)) {
            return Vec::new();
        }
        let (commands, make_folder, running) = match dialog.mode {
            DialogMode::Backup => {
                // Only the suggested folder is made; a typed one has to exist.
                let make_folder = dialog
                    .folder
                    .clone()
                    .filter(|folder| Path::new(&path).parent() == Some(folder.as_path()));
                let command = Command::BackupDatabaseTo(rc_engine::BackupToRequest {
                    path,
                    overwrite: dialog.overwrite,
                });
                (vec![command], make_folder, t!("db.running_backup"))
            }
            DialogMode::Restore => (
                vec![Command::RestoreDatabase { path }],
                None,
                t!("db.running_restore"),
            ),
            DialogMode::Export => match dialog.exchange.export(path, dialog.overwrite) {
                Ok(commands) => (commands, None, t!("exchange.running_export")),
                Err(problem) => {
                    dialog.exchange.problem = Some(problem);
                    return Vec::new();
                }
            },
            // Only the rows of a preview are imported.
            DialogMode::Import if !exchange_view::can_import(dialog) => return Vec::new(),
            DialogMode::Import => match dialog.exchange.import(path, false) {
                Ok(commands) => (commands, None, t!("exchange.running_import")),
                Err(problem) => {
                    dialog.exchange.problem = Some(problem);
                    return Vec::new();
                }
            },
        };
        dialog.phase = Phase::Running;
        self.info = Some(running.to_owned());
        vec![Effect::Database(Box::new(DatabaseJob {
            commands,
            make_folder,
        }))]
    }

    /// The dialog opens on `mode`; over the settings, which it replaces (their Import/Export page
    /// offers it only while nothing in them is unsaved). An export starts with the last one's
    /// parts and encryption.
    fn open_database(&mut self, mode: DialogMode) -> Vec<Effect> {
        self.settings = None;
        self.database = Some(DatabaseDialog::new(mode, suggested_folder()));
        let Some(db) = self.db.clone().filter(|_| mode == DialogMode::Export) else {
            return Vec::new();
        };
        let read = async move {
            let parts = ExportParts {
                settings: stored_bool(&db, EXPORT_SETTINGS_KEY).await,
                connections: stored_bool(&db, EXPORT_CONNECTIONS_KEY).await,
                trust: stored_bool(&db, EXPORT_TRUST_KEY).await,
                hotlist: stored_bool(&db, EXPORT_HOTLIST_KEY).await,
            };
            let encrypted = stored_bool(&db, EXPORT_ENCRYPTED_KEY).await;
            (parts, encrypted)
        };
        vec![Effect::Task(Task::perform(read, |(parts, encrypted)| {
            Message::Database(DatabaseMsg::Exchange(ExchangeMsg::Choices(
                parts, encrypted,
            )))
        }))]
    }

    /// The import's Read: the file previewed, nothing written.
    fn read_export(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.editable() else {
            return Vec::new();
        };
        let path = dialog.path.trim().to_owned();
        if dialog.mode != DialogMode::Import || path.is_empty() {
            return Vec::new();
        }
        let Ok(commands) = dialog.exchange.import(path, true) else {
            return Vec::new();
        };
        dialog.exchange.report = None;
        dialog.phase = Phase::Running;
        vec![Effect::Database(Box::new(DatabaseJob {
            commands,
            make_folder: None,
        }))]
    }

    fn database_finished(&mut self, result: Result<Outcome, &'static str>) {
        let phase = match result {
            Ok(Outcome::DatabaseBackup(report)) => Phase::Done(t!(
                "db.backup_done",
                path = report.path.display(),
                size = format::size(report.size)
            )),
            Ok(Outcome::RestoreStaged(report)) => Phase::Done(t!(
                "db.restore_staged",
                backup = report.safety_backup.path.display()
            )),
            Ok(Outcome::ConfigExported(report)) => {
                self.exchange_answered(None);
                Phase::Done(t!(
                    "exchange.export_done",
                    path = report.path,
                    size = format::size(report.bytes)
                ))
            }
            Ok(Outcome::ConfigImported(report)) => {
                self.exchange_answered(None);
                let done = !report.dry_run;
                let written = report
                    .rows
                    .iter()
                    .filter(|row| matches!(row.status, RowStatus::Created | RowStatus::Updated))
                    .count();
                if let Some(dialog) = self.database.as_mut() {
                    dialog.exchange.previewed(report);
                }
                if done {
                    Phase::Done(t!("exchange.import_done", count = written))
                } else {
                    Phase::Editing
                }
            }
            Err(code) if Ask::for_code(code).is_some() => {
                self.exchange_answered(Ask::for_code(code));
                Phase::Failed(error_text(code))
            }
            // The two commands answer nothing else.
            Ok(_) => Phase::Done(String::new()),
            Err(code) => Phase::Failed(error_text(code)),
        };
        self.info = match &phase {
            Phase::Done(text) | Phase::Failed(text) => Some(text.clone()),
            Phase::Editing | Phase::Running => None,
        };
        if let Some(dialog) = self.database.as_mut() {
            dialog.phase = phase;
        }
    }

    /// The vault answered (`None`) or asks `ask`; a typed master password goes either way.
    fn exchange_answered(&mut self, ask: Option<Ask>) {
        if let Some(dialog) = self.database.as_mut() {
            let form = &mut dialog.exchange;
            form.vault = ask;
            form.master = rc_vault::SecretString::from("");
            form.master_repeat = rc_vault::SecretString::from("");
        }
    }
}

#[cfg(test)]
#[path = "database_dialog_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "database_dialog_exchange_tests.rs"]
mod exchange_tests;
