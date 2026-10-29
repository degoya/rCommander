//! The `archive.*` commands of RC-030-03 and RC-030-04: unpacking and packing as jobs of the
//! operations queue, with sources and targets taken from the panels as Total Commander's Alt+F9
//! and Alt+F5 take them. The password of an encrypted archive reaches its job only through
//! `archive.password`, which is `GuiOnly`: MCP can neither give nor read one.

use std::path::Path;

use rc_ops::{ExtractSettings, JobId, JobKind, JobOptions, JobSpec, PackSettings};
use rc_vfs::{ArchivePassword, VfsPath};

use super::{ops::sources_of, panel::resolve_path};
use crate::{
    Engine, EngineError, Outcome, PanelId, PanelState, SecretParam, mcp_questions::check_mcp_pin,
};

/// The archive name Total Commander suggests: one file's name without its ending, one folder's
/// name, or for several entries the name of the directory they are in.
#[must_use]
pub fn default_archive_name(panel: &PanelState, sources: &[VfsPath]) -> String {
    let stem = match sources {
        [one] => {
            let name = one
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let is_dir = panel
                .entries
                .iter()
                .any(|entry| entry.name == name && entry.is_dir_like());
            if is_dir {
                name
            } else {
                Path::new(&name)
                    .file_stem()
                    .map_or(name.clone(), |stem| stem.to_string_lossy().into_owned())
            }
        }
        _ => panel
            .path
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    if stem.is_empty() {
        "archive".to_owned()
    } else {
        stem
    }
}

impl Engine {
    /// `archive.extract`: the archives of `panel` (the active one by default) into `target`, or
    /// into the other panel's directory; at once or queued ([`Engine::tool_list`]).
    pub(crate) async fn archive_extract(
        &self,
        panel: Option<PanelId>,
        archives: &[String],
        target: Option<&str>,
        settings: ExtractSettings,
        options: JobOptions,
    ) -> Result<Outcome, EngineError> {
        let list = self.tool_list().await;
        let id = self.resolve(panel);
        self.remember_selection(id, archives);
        let (archives, target) = {
            let state = self.state();
            let other = &state.panel(id.other()).path;
            // The folders the MCP write guard judged against (RC-080-42).
            check_mcp_pin(id, &state.panel(id).path)?;
            check_mcp_pin(id.other(), other)?;
            let target = target.map_or_else(|| other.clone(), |path| resolve_path(other, path));
            (sources_of(state.panel(id), archives)?, target)
        };
        let kind = JobKind::Extract {
            archives,
            target,
            settings,
        };
        Ok(Outcome::job(self.inner.ops.submit_into(
            JobSpec { kind, options },
            None,
            list,
        )?))
    }

    /// `archive.pack`: the sources of `panel` into `target`, or into an archive named after them
    /// in the other panel's directory; at once or queued ([`Engine::tool_list`]).
    pub(crate) async fn archive_pack(
        &self,
        panel: Option<PanelId>,
        sources: &[String],
        target: Option<&str>,
        settings: PackSettings,
        options: JobOptions,
    ) -> Result<Outcome, EngineError> {
        let list = self.tool_list().await;
        let id = self.resolve(panel);
        self.remember_selection(id, sources);
        let (sources, target) = {
            let state = self.state();
            let from = state.panel(id);
            let other = &state.panel(id.other()).path;
            check_mcp_pin(id, &from.path)?;
            check_mcp_pin(id.other(), other)?;
            let sources = sources_of(from, sources)?;
            let target = match target {
                Some(path) => resolve_path(other, path),
                None if settings.each_own_archive => other.clone(),
                None => other.join(format!(
                    "{}.{}",
                    default_archive_name(from, &sources),
                    settings.format.extension()
                )),
            };
            (sources, target)
        };
        let kind = JobKind::Pack {
            sources,
            target,
            settings,
        };
        Ok(Outcome::job(self.inner.ops.submit_into(
            JobSpec { kind, options },
            None,
            list,
        )?))
    }

    /// `archive.password`: hands the typed password (or none, skip) to the job that asks.
    pub(crate) fn archive_password(
        &self,
        job: JobId,
        password: Option<&SecretParam>,
    ) -> Result<Outcome, EngineError> {
        let password = password.map(|typed| ArchivePassword::new(typed.secret().clone()));
        Ok(Outcome::job(self.inner.ops.answer_password(job, password)?))
    }
}

#[cfg(test)]
#[path = "archive_ops_tests.rs"]
mod tests;
