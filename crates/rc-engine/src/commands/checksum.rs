//! The `checksum.*` commands and `file.split` / `file.combine`, over `rc-checksum`.
//!
//! Paths resolve against a panel's directory like `panel.list_dir`. Each run gets its own
//! [`Control`]; dropping the command's future (an MCP call cancelled, the window closed) stops
//! it, and whatever it had created is removed. A queue with pause and an explicit cancel is
//! `rc-ops`' (RC-010-06), which runs these same functions with its own token. Under MCP a run
//! hashes no file the read guard keeps from MCP (`read_guard.rs`), however it reached it: a
//! folder's contents or a checksum file's lines.
//!
//! The window's dialogs (`checksum.dialog`) run the same work through `checksum.queue`, as a job
//! of that queue: pause, cancel and progress in the progress window, the result the finished
//! job's report. MCP cannot submit such a job - it would run outside the call and its read
//! guard - and keeps the four commands above, which answer with their report.

use rc_checksum::{Algorithm, ChecksumError, Control, Layout};
use rc_ops::{JobKind, JobOptions, JobSpec};
use rc_vfs::{BackendId, VfsPath};

use super::panel::resolve_path;
use crate::{
    ChecksumDialog, ChecksumFilesRequest, ChecksumJob, CombineRequest, Engine, EngineError,
    EngineEvent, FileDigest, Outcome, PanelId, SplitRequest, read_guard,
};

/// Where `checksum.create` writes its lines: beside each file, or into one file.
pub(crate) fn layout(per_file: bool, file_name: Option<String>) -> Layout {
    if per_file {
        Layout::PerFile
    } else {
        Layout::Combined { file_name }
    }
}

/// A run's control: with the read guard of the MCP dispatch this runs in, if any.
fn control(backend: &BackendId) -> Control {
    let control = Control::new();
    match read_guard::current(backend) {
        Some(guard) => control.with_guard(guard.for_checksums()),
        None => control,
    }
}

impl Engine {
    /// `target` resolved against the directory of `panel` (the active one by default); inside
    /// [`Engine::pin_mcp_panels`] the folder pinned, so the MCP write guard judged exactly this
    /// path (RC-080-42).
    fn resolve_from(&self, panel: Option<PanelId>, target: &str) -> VfsPath {
        let id = self.resolve(panel);
        resolve_path(&self.mcp_folder(id), target)
    }

    /// `target` resolved, or the directory of `panel` when `None`.
    fn dir_or_panel(&self, panel: Option<PanelId>, target: Option<&str>) -> VfsPath {
        match target {
            Some(target) => self.resolve_from(panel, target),
            None => {
                // Resolved first: `resolve` takes the state lock itself.
                let id = self.resolve(panel);
                self.mcp_folder(id)
            }
        }
    }

    /// `checksum.compute`: each file's digest, or its error code; only a cancel fails the whole.
    pub(crate) async fn compute_checksums(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
        algorithm: Algorithm,
    ) -> Result<Outcome, EngineError> {
        if paths.is_empty() {
            return Err(ChecksumError::NoFiles.into());
        }
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let path = self.resolve_from(panel, path);
            let vfs = self.vfs(&path)?;
            let control = control(&path.backend);
            let (hex, code) =
                match rc_checksum::hash_file(&vfs, &path.path, algorithm, &control).await {
                    Ok(hex) => (Some(hex), None),
                    Err(ChecksumError::Cancelled) => return Err(ChecksumError::Cancelled.into()),
                    Err(error) => (None, Some(error.code().to_owned())),
                };
            files.push(FileDigest { path, hex, code });
        }
        Ok(Outcome::Checksums { algorithm, files })
    }

    /// `checksum.create`.
    pub(crate) async fn create_checksum_files(
        &self,
        panel: Option<PanelId>,
        dir: Option<&str>,
        names: &[String],
        algorithm: Algorithm,
        layout: &Layout,
    ) -> Result<Outcome, EngineError> {
        let dir = self.dir_or_panel(panel, dir);
        let vfs = self.vfs(&dir)?;
        let report = rc_checksum::create(
            &vfs,
            &dir.path,
            names,
            algorithm,
            layout,
            &control(&dir.backend),
        )
        .await?;
        Ok(Outcome::ChecksumsCreated { dir, report })
    }

    /// `checksum.verify`.
    pub(crate) async fn verify_checksum_file(
        &self,
        panel: Option<PanelId>,
        path: &str,
    ) -> Result<Outcome, EngineError> {
        let path = self.resolve_from(panel, path);
        let vfs = self.vfs(&path)?;
        let report = rc_checksum::verify(&vfs, &path.path, &control(&path.backend)).await?;
        Ok(Outcome::ChecksumsVerified { path, report })
    }

    /// `file.split`: the parts go to `target_dir`, else beside the file.
    pub(crate) async fn split_file(
        &self,
        panel: Option<PanelId>,
        path: &str,
        part_size: u64,
        target_dir: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let source = self.resolve_from(panel, path);
        let dir = self.target_dir(panel, &source, target_dir);
        let (source_vfs, target_vfs) = (self.vfs(&source)?, self.vfs(&dir)?);
        let report = rc_checksum::split(
            &source_vfs,
            &source.path,
            &target_vfs,
            &dir.path,
            part_size,
            &control(&source.backend),
        )
        .await?;
        Ok(Outcome::Split { dir, report })
    }

    /// `file.combine`: the file goes to `target_dir`, else beside the parts.
    pub(crate) async fn combine_files(
        &self,
        panel: Option<PanelId>,
        path: &str,
        target_dir: Option<&str>,
        target_name: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let first = self.resolve_from(panel, path);
        let dir = self.target_dir(panel, &first, target_dir);
        let (source_vfs, target_vfs) = (self.vfs(&first)?, self.vfs(&dir)?);
        let report = rc_checksum::combine(
            &source_vfs,
            &first.path,
            &target_vfs,
            &dir.path,
            target_name,
            &control(&first.backend),
        )
        .await?;
        Ok(Outcome::Combined { dir, report })
    }

    /// `checksum.dialog`: announces that a checksum dialog is wanted for the panel.
    pub(crate) fn checksum_dialog(
        &self,
        panel: Option<PanelId>,
        dialog: ChecksumDialog,
    ) -> Outcome {
        let id = self.resolve(panel);
        self.publish(EngineEvent::ChecksumDialog { panel: id, dialog });
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// `checksum.queue`: the work of a dialog as a job, its paths resolved as the four commands
    /// resolve them.
    pub(crate) async fn queue_checksum_job(
        &self,
        job: ChecksumJob,
    ) -> Result<Outcome, EngineError> {
        let list = self.tool_list().await;
        if read_guard::from_mcp() {
            return Err(EngineError::GuiOnly {
                command: "checksum.queue",
            });
        }
        let kind = match job {
            ChecksumJob::Create(ChecksumFilesRequest {
                panel,
                dir,
                names,
                algorithm,
                per_file,
                file_name,
            }) => {
                if names.is_empty() {
                    return Err(ChecksumError::NoFiles.into());
                }
                JobKind::ChecksumCreate {
                    dir: self.dir_or_panel(panel, dir.as_deref()),
                    names,
                    algorithm,
                    layout: layout(per_file, file_name),
                }
            }
            ChecksumJob::Verify { panel, path } => JobKind::ChecksumVerify {
                file: self.resolve_from(panel, &path),
            },
            ChecksumJob::Split(SplitRequest {
                panel,
                path,
                part_size,
                target_dir,
            }) => {
                let source = self.resolve_from(panel, &path);
                let target = self.dialog_target(panel, &source, target_dir.as_deref());
                JobKind::Split {
                    source,
                    target,
                    part_size,
                }
            }
            ChecksumJob::Combine(CombineRequest {
                panel,
                path,
                target_dir,
                target_name,
            }) => {
                let first = self.resolve_from(panel, &path);
                let target = self.dialog_target(panel, &first, target_dir.as_deref());
                JobKind::Combine {
                    first,
                    target,
                    name: target_name,
                }
            }
        };
        let options = JobOptions::default();
        Ok(Outcome::job(self.inner.ops.submit_into(
            JobSpec { kind, options },
            None,
            list,
        )?))
    }

    /// A dialog's target folder: resolved against the other panel's folder, which the dialog
    /// offers as Total Commander does - so a path typed there means that panel's file system -
    /// or the folder of `file` when `None`.
    fn dialog_target(
        &self,
        panel: Option<PanelId>,
        file: &VfsPath,
        target: Option<&str>,
    ) -> VfsPath {
        match target {
            Some(target) => {
                let id = self.resolve(panel);
                let other = self.state().panel(id.other()).path.clone();
                resolve_path(&other, target)
            }
            None => file.parent().unwrap_or_else(|| file.clone()),
        }
    }

    /// `target` resolved, or the folder of `file` when `None`.
    fn target_dir(&self, panel: Option<PanelId>, file: &VfsPath, target: Option<&str>) -> VfsPath {
        match target {
            Some(target) => self.resolve_from(panel, target),
            None => file.parent().unwrap_or_else(|| file.clone()),
        }
    }
}

#[cfg(test)]
#[path = "checksum_tests.rs"]
mod tests;
