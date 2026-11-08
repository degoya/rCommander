//! Compare by content beyond reading (RC-100-94): `compare.by_content`, which starts the external
//! compare tool of the settings (Total Commander's `CompareTool=`) or else opens the diff window;
//! `compare.apply_blocks`, which copies blocks of differing lines to the other side and saves it;
//! and the engine calls of the diff window's edit mode ([`Engine::edit_paths`],
//! [`Engine::save_compared`]).
//!
//! The tool gets two local files: a file on a server or inside an archive goes as a read-only
//! copy under the temporary folder (`unpack_to_temp`, removed at exit), and what the tool changes
//! there is not written back. The program and its arguments are protected settings; every
//! argument is one argv element of its own, no shell reads them.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use rc_vfs::{BackendId, VfsPath, unix_seconds};
use tokio_util::sync::CancellationToken;

use super::archive::refuse_archive_change;
use crate::{
    Engine, EngineError, EngineEvent, Outcome,
    compare::{
        self, ApplyBlocksRequest, ByContentRequest, CompareSide, EditSession, FileStamp, Origin,
        ReadOnly, Side, edit, external, save,
    },
    dispatch::call_token,
    read_guard,
    settings::{COMPARE_ARGS_KEY, COMPARE_TOOL_KEY, stored_setting},
    shown_path,
};

impl Engine {
    /// `compare.by_content`: the two files given, else picked as `compare.files` picks them,
    /// in the external compare tool when one is set, else announced for the diff window.
    pub(crate) async fn compare_by_content(
        &self,
        request: ByContentRequest,
    ) -> Result<Outcome, EngineError> {
        let (left, right) = match (request.left, request.right) {
            (Some(left), Some(right)) => (left, right),
            _ => self.files_to_compare(None, None)?,
        };
        match self.compare_tool().await {
            Some((tool, args)) => {
                let copies = self.start_compare_tool(&tool, &args, &left, &right).await?;
                self.publish(EngineEvent::CompareToolStarted { copies });
            }
            None => self.publish(EngineEvent::CompareFiles { left, right }),
        }
        Ok(Outcome::Panel(self.snapshot(None)))
    }

    /// The external compare tool and its arguments; `None` when none is set.
    async fn compare_tool(&self) -> Option<(String, String)> {
        let db = self.db().ok()?;
        let text = |value: Option<serde_json::Value>| {
            value
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .unwrap_or_default()
        };
        let tool = text(stored_setting(db, COMPARE_TOOL_KEY).await);
        if tool.trim().is_empty() {
            return None;
        }
        Some((tool, text(stored_setting(db, COMPARE_ARGS_KEY).await)))
    }

    /// Starts `tool` on the two files, without waiting; answers whether either went as a copy.
    async fn start_compare_tool(
        &self,
        tool: &str,
        args: &str,
        left: &VfsPath,
        right: &VfsPath,
    ) -> Result<bool, EngineError> {
        let program = PathBuf::from(tool);
        let name = program.file_name().map_or_else(
            || tool.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let left_file = self.local_file(left).await?;
        let right_file = self.local_file(right).await?;
        let arguments = external::arguments(args, &left_file, &right_file)
            .map_err(|_| EngineError::OpenFailed { name: name.clone() })?;
        start(&program, &arguments).map_err(|_| EngineError::OpenFailed { name })?;
        Ok(left.backend != BackendId::Local || right.backend != BackendId::Local)
    }

    /// A file a program can open: a local one as it is, any other as a read-only copy under the
    /// temporary folder.
    async fn local_file(&self, path: &VfsPath) -> Result<PathBuf, EngineError> {
        if path.backend == BackendId::Local {
            return Ok(path.path.clone());
        }
        self.open_backend(path)?;
        self.open_server(path).await?;
        self.unpack_to_temp(path).await
    }

    /// `compare.apply_blocks`: both files compared again under the request's options, the
    /// blocks named copied in its direction, the side they land on saved - only when both files
    /// are still as `expect_left` and `expect_right` say. Answers the comparison afterwards.
    /// Under MCP both files pass the read guard first, as for `compare.files`.
    pub(crate) async fn apply_blocks(
        &self,
        request: ApplyBlocksRequest,
    ) -> Result<Outcome, EngineError> {
        let (left, right) = self.files_to_compare(Some(&request.left), Some(&request.right))?;
        for side in [&left, &right] {
            if let Some(guard) = read_guard::current(&side.backend) {
                guard.check(&side.path).await?;
            }
        }
        let (_, to) = request.direction.sides();
        let target = match to {
            CompareSide::Left => &left,
            CompareSide::Right => &right,
        };
        refuse_archive_change(target)?;
        let (left_vfs, right_vfs) = (self.vfs(&left)?, self.vfs(&right)?);
        let cancel = call_token();
        let (left_side, right_side) = (
            Side {
                vfs: &*left_vfs,
                path: &left,
            },
            Side {
                vfs: &*right_vfs,
                path: &right,
            },
        );
        let mut session = save::load(left_side, right_side, &cancel)
            .await?
            .ok_or_else(|| not_editable(target, "it is binary or too large to compare by line"))?;
        for (side, expected) in [
            (CompareSide::Left, request.expect_left),
            (CompareSide::Right, request.expect_right),
        ] {
            let origin = session.origin(side);
            if !as_expected(origin, expected) {
                return Err(EngineError::CompareChanged {
                    name: shown_path(&origin.path).display().to_string(),
                });
            }
        }
        if let Some(reason) = session.read_only(to) {
            return Err(not_editable(target, read_only_reason(reason)));
        }
        let (left_text, right_text) = session.files();
        let diff = compare::diff_texts(left_text, right_text, &request.options).await?;
        let all = edit::blocks(&diff);
        let chosen = request
            .blocks
            .iter()
            .map(|&block| {
                all.get(block)
                    .cloned()
                    .ok_or(EngineError::CompareBlockNotFound { block })
            })
            .collect::<Result<Vec<_>, _>>()?;
        session.copy_blocks(&diff, &chosen, request.direction);
        if session.dirty(to) {
            let bytes = session.encoded(to).map_err(|_| {
                not_editable(target, "a copied line holds a character its encoding lacks")
            })?;
            let vfs = match to {
                CompareSide::Left => &left_vfs,
                CompareSide::Right => &right_vfs,
            };
            save::save(&**vfs, session.origin(to), &bytes, false).await?;
        }
        let comparison =
            compare::compare_files(left_side, right_side, &request.options, 0, &cancel).await?;
        Ok(Outcome::FileComparison(comparison))
    }

    /// The two files of the diff window as an [`EditSession`] ([`save::load`]): `None` when
    /// either is binary or too large to compare by line.
    ///
    /// # Errors
    ///
    /// Either file cannot be read, or `cancel` was cancelled.
    pub async fn edit_paths(
        &self,
        left: &VfsPath,
        right: &VfsPath,
        cancel: &CancellationToken,
    ) -> Result<Option<EditSession>, EngineError> {
        let (left_vfs, right_vfs) = (self.vfs(left)?, self.vfs(right)?);
        save::load(
            Side {
                vfs: &*left_vfs,
                path: left,
            },
            Side {
                vfs: &*right_vfs,
                path: right,
            },
            cancel,
        )
        .await
    }

    /// Writes `bytes` as the file of `origin` for the edit mode ([`save::save`]): refused with
    /// `compare_changed` when it changed since, unless `force`.
    ///
    /// # Errors
    ///
    /// `compare_changed`, `archive_read_only`, or what the file system answers.
    pub async fn save_compared(
        &self,
        origin: &Origin,
        bytes: &[u8],
        force: bool,
    ) -> Result<Origin, EngineError> {
        let vfs = self.vfs(&origin.path)?;
        save::save(&*vfs, origin, bytes, force).await
    }
}

/// The file is as a comparison reported it.
fn as_expected(origin: &Origin, expected: FileStamp) -> bool {
    origin.size == expected.size && origin.modified.and_then(unix_seconds) == expected.modified_unix
}

fn not_editable(path: &VfsPath, reason: &'static str) -> EngineError {
    EngineError::CompareNotEditable {
        name: shown_path(path).display().to_string(),
        reason,
    }
}

fn read_only_reason(reason: ReadOnly) -> &'static str {
    match reason {
        ReadOnly::Archive => "it is inside an archive",
        ReadOnly::TooLarge => "it is larger than 4 MiB",
        ReadOnly::NotLossless => "written back it would not keep its bytes",
    }
}

/// Starts `program` with `arguments`, each one argument, without waiting and without a console
/// window; tokio reaps it when it ends.
#[cfg(not(test))]
fn start(program: &Path, arguments: &[OsString]) -> std::io::Result<()> {
    use rc_process::NoConsoleWindow as _;
    use std::process::Stdio;
    tokio::process::Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .no_console_window()
        .spawn()
        .map(drop)
}

/// The tests start nothing: the program and its arguments are recorded.
#[cfg(test)]
fn start(program: &Path, arguments: &[OsString]) -> std::io::Result<()> {
    use crate::recover::LockOrRecover as _;
    tests::STARTED
        .lock_or_recover("compare_edit.started")
        .push((program.to_path_buf(), arguments.to_vec()));
    Ok(())
}

#[cfg(test)]
#[path = "compare_edit_tests.rs"]
mod tests;
