//! The dialog's work for the engine, each off the window's thread: a preview (debounced - the
//! last input wins), a preset command, a run or its undo, the log of a run; and the name lists
//! (RC-100-96): a names file read, the editor's file written and the editor started, the next
//! step's names.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use rc_db::{Db, LoggedStep};
use rc_engine::{
    Command, Engine, EngineError, ExternalProgram, JobSnapshot, MAX_NAMES_BYTES, MaskField,
    MultiRenameRequest, Outcome, RenameError, RenamePlan, RenamePreset, RenameRule, VfsPath,
};
use tokio::io::AsyncWriteExt;

use super::MrMsg;
use crate::i18n::error_text;

/// How long typing pauses before the preview is made.
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// Why a preview could not be made: the message, and for a mask that does not parse the mask
/// and the position (1-based) in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewError {
    pub text: String,
    pub mask: Option<(MaskField, usize)>,
}

impl PreviewError {
    fn of(error: &EngineError) -> Self {
        let mask = match error {
            EngineError::Rename(RenameError::Mask { field, error }) => {
                Some((*field, error.position + 1))
            }
            _ => None,
        };
        Self {
            text: error_text(error.code()),
            mask,
        }
    }
}

/// A run or an undo the engine queued: its tag in the undo log, its folder and its job.
#[derive(Debug, Clone)]
pub struct Queued {
    pub run: String,
    pub dir: VfsPath,
    pub job: JobSnapshot,
}

/// What comes back from a [`Call`].
#[derive(Debug, Clone)]
pub enum Answer {
    /// The preview of the input numbered so.
    Preview(u64, Box<Result<RenamePlan, PreviewError>>),
    /// A later input came while this one waited: nothing was asked.
    Stale,
    /// The presets after a preset command, and the rule last used; or the error code.
    Presets(Result<(Vec<RenamePreset>, Option<RenameRule>), &'static str>),
    /// A run (`undo: false`) or an undo queued, or the error code.
    Queued {
        undo: bool,
        result: Result<Box<Queued>, &'static str>,
    },
    /// The log of a run written, or why not.
    Logged(Result<usize, String>),
    /// A name list read from a file, or why not.
    Names(Result<Vec<String>, String>),
    /// The names written for the editor, which was started on them; or why not.
    Editing(Result<PathBuf, String>),
    /// The names of the entries after the latest run, or the error code.
    NextStep(Result<Vec<String>, &'static str>),
}

/// Work of the dialog for the engine.
pub enum Call {
    Preview {
        generation: u64,
        /// The number of the latest input; a preview of an older one is not made.
        latest: Arc<AtomicU64>,
        debounce: bool,
        request: MultiRenameRequest,
    },
    Presets(Command),
    Run {
        undo: bool,
        command: Command,
    },
    /// Appends the pairs of run `run` that were carried out to the file `path`.
    Log {
        db: Option<Db>,
        run: String,
        dir: VfsPath,
        pairs: Vec<(String, String)>,
        path: String,
    },
    /// Reads the names file `path`, removed afterwards with `remove` (the editor's).
    ReadNames {
        path: PathBuf,
        remove: bool,
    },
    /// Writes `names` to the new file `path` and starts the editor on it.
    EditNames {
        path: PathBuf,
        names: Vec<String>,
    },
    /// Removes the editor's file of a list not taken.
    Discard(PathBuf),
    /// The names of `names` after the latest run.
    NextStep(Vec<String>),
}

impl Call {
    /// Runs the call: off the window's thread in the app, inline in the tests.
    pub async fn run(self, engine: Engine) -> MrMsg {
        MrMsg::Answer(match self {
            Self::Preview {
                generation,
                latest,
                debounce,
                request,
            } => {
                if debounce {
                    tokio::time::sleep(DEBOUNCE).await;
                }
                if latest.load(Ordering::Acquire) != generation {
                    return MrMsg::Answer(Answer::Stale);
                }
                let plan = match engine.dispatch(Command::MultiRenamePreview(request)).await {
                    Ok(Outcome::RenamePreview { plan, .. }) => Ok(plan),
                    Ok(_) => Err(PreviewError {
                        text: error_text("io"),
                        mask: None,
                    }),
                    Err(error) => Err(PreviewError::of(&error)),
                };
                Answer::Preview(generation, Box::new(plan))
            }
            Self::Presets(command) => Answer::Presets(match engine.dispatch(command).await {
                Ok(Outcome::RenamePresets { presets, last }) => Ok((presets, last)),
                Ok(_) => Err("io"),
                Err(error) => Err(error.code()),
            }),
            Self::Run { undo, command } => {
                let result = match engine.dispatch(command).await {
                    Ok(Outcome::RenameRun { run, dir, job }) => Ok(Box::new(Queued {
                        run,
                        dir,
                        job: *job,
                    })),
                    Ok(_) => Err("io"),
                    Err(error) => Err(error.code()),
                };
                Answer::Queued { undo, result }
            }
            Self::Log {
                db,
                run,
                dir,
                pairs,
                path,
            } => Answer::Logged(write_log(db, &run, &dir, &pairs, &path).await),
            Self::ReadNames { path, remove } => Answer::Names(read_names(&path, remove).await),
            Self::EditNames { path, names } => {
                Answer::Editing(edit_names(&engine, path, &names).await)
            }
            Self::Discard(path) => {
                remove_names_file(&path).await;
                Answer::Stale
            }
            Self::NextStep(names) => Answer::NextStep(
                match engine
                    .dispatch(Command::MultiRenameNextStep { names })
                    .await
                {
                    Ok(Outcome::RenameNextStep { names }) => Ok(names),
                    Ok(_) => Err("io"),
                    Err(error) => Err(error.code()),
                },
            ),
        })
    }
}

/// The names of the file `path` (BOM, UTF-8, UTF-16 or ANSI; one per line), the file removed
/// after with `remove`.
pub async fn read_names(path: &Path, remove: bool) -> Result<Vec<String>, String> {
    let read = async {
        let size = tokio::fs::metadata(path)
            .await
            .map_err(|error| error.to_string())?
            .len();
        if !usize::try_from(size).is_ok_and(|size| size <= MAX_NAMES_BYTES) {
            return Err(error_text("rename_names_too_large"));
        }
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|error| error.to_string())?;
        rc_engine::names_from_bytes(&bytes).map_err(|error| error_text(error.code()))
    };
    let names = read.await;
    if remove {
        remove_names_file(path).await;
    }
    names
}

/// Writes `names` for the editor to the new file `path` - UTF-8 with a byte-order mark, CRLF on
/// Windows - and starts `files.editor` on it (`file.open_external`). A file there already is
/// not overwritten.
async fn edit_names(engine: &Engine, path: PathBuf, names: &[String]) -> Result<PathBuf, String> {
    let bytes = rc_engine::names_to_bytes(names, cfg!(windows));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .map_err(|error| error.to_string())?;
    let written = async {
        file.write_all(&bytes).await?;
        file.flush().await
    };
    if let Err(error) = written.await {
        remove_names_file(&path).await;
        return Err(error.to_string());
    }
    let open = Command::OpenExternal {
        panel: None,
        path: Some(path.display().to_string()),
        program: ExternalProgram::Editor,
        create: false,
        cursor_only: false,
    };
    match engine.dispatch(open).await {
        Ok(_) => Ok(path),
        Err(error) => {
            remove_names_file(&path).await;
            Err(error_text(error.code()))
        }
    }
}

/// Removes the editor's file; one that cannot be removed is left in the temporary folder.
async fn remove_names_file(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        tracing::warn!(%error, "the multi-rename names file was not removed");
    }
}

/// The pairs of `pairs` whose rename was carried out: those whose new name is the target of a
/// step marked done. Every pair's last step ends on its new name, and no temporary name is a
/// pair's new name, so this holds for a run that stopped half way too.
pub fn done_pairs(pairs: &[(String, String)], steps: &[LoggedStep]) -> Vec<(String, String)> {
    let reached: HashSet<&str> = steps
        .iter()
        .filter(|step| step.done)
        .map(|step| step.to.as_str())
        .collect();
    pairs
        .iter()
        .filter(|(_, new)| reached.contains(new.as_str()))
        .cloned()
        .collect()
}

/// The log's lines: `<old path> -> <new path>`, one per pair.
pub fn log_text(dir: &VfsPath, pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(old, new)| {
            format!(
                "{} -> {}\n",
                rc_engine::nested_path(&dir.path, old).display(),
                rc_engine::nested_path(&dir.path, new).display()
            )
        })
        .collect()
}

/// Appends the pairs of `run` that were carried out, as the undo log marks them, to the file
/// `path`; how many.
async fn write_log(
    db: Option<Db>,
    run: &str,
    dir: &VfsPath,
    pairs: &[(String, String)],
    path: &str,
) -> Result<usize, String> {
    let db = db.ok_or_else(|| error_text("database_unavailable"))?;
    let logged = db
        .latest_rename_run()
        .await
        .map_err(|_| error_text("database_error"))?
        .filter(|logged| logged.run == run)
        .ok_or_else(|| error_text("rename_undo_incomplete"))?;
    let done = done_pairs(pairs, &logged.steps);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Path::new(path))
        .await
        .map_err(|error| error.to_string())?;
    file.write_all(log_text(dir, &done).as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    file.flush().await.map_err(|error| error.to_string())?;
    Ok(done.len())
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
