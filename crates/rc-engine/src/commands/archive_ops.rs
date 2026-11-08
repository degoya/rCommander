//! The `archive.*` commands of RC-030-03 and RC-030-04: unpacking and packing as jobs of the
//! operations queue, with sources and targets taken from the panels as Total Commander's Alt+F9
//! and Alt+F5 take them. The password of an encrypted archive reaches its job only through
//! `archive.password`, which is `GuiOnly`: MCP can neither give nor read one.
//!
//! Changing an archive (RC-110-05): `archive.add` is F5 with an archive in the target panel,
//! `archive.delete` F8 inside one - each an archive update job of the queue
//! (`JobKind::ArchiveUpdate`), entries removed by the index the archive backend lists them by.

use std::path::Path;

use rc_ops::{
    ExtractSettings, JobId, JobKind, JobOptions, JobSpec, OpsError, PackSettings, UpdateSettings,
};
use rc_vfs::{ArchiveFormat, ArchivePassword, BackendId, VfsPath};

use super::{archive::not_an_archive, ops::sources_of, panel::resolve_path};
use crate::{
    Engine, EngineError, Outcome, PanelId, PanelState, SecretParam, mcp_questions::check_mcp_pin,
    settings::VERIFY_UPDATE_KEY,
};

/// Refuses an archive that is not changed: a compressed tar or a single compressed file, as
/// Total Commander refuses them.
fn check_modifiable(format: ArchiveFormat, archive: &VfsPath) -> Result<(), EngineError> {
    match format {
        ArchiveFormat::Zip | ArchiveFormat::SevenZ | ArchiveFormat::Tar { compression: None } => {
            Ok(())
        }
        _ => Err(EngineError::Ops(OpsError::ArchiveNotModifiable {
            path: archive.path.clone(),
        })),
    }
}

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

    /// `archive.add` (RC-110-05): the sources of `panel` into the archive `target` names - a
    /// place inside an archive, or an archive file (its root) - by default the other panel's
    /// location, which must lie inside an archive; at once or queued ([`Engine::tool_list`]).
    pub(crate) async fn archive_add(
        &self,
        panel: Option<PanelId>,
        sources: &[String],
        target: Option<&str>,
        mut settings: UpdateSettings,
        options: JobOptions,
    ) -> Result<Outcome, EngineError> {
        let list = self.tool_list().await;
        let id = self.resolve(panel);
        self.remember_selection(id, sources);
        let (sources, location) = {
            let state = self.state();
            let from = state.panel(id);
            let other = &state.panel(id.other()).path;
            check_mcp_pin(id, &from.path)?;
            check_mcp_pin(id.other(), other)?;
            let location = target.map_or_else(|| other.clone(), |path| resolve_path(other, path));
            (sources_of(from, sources)?, location)
        };
        let (archive, under) = self.update_target(&location).await?;
        settings.verify = self.setting_on(VERIFY_UPDATE_KEY).await;
        let kind = JobKind::ArchiveUpdate {
            archive,
            add: sources,
            under,
            remove: Vec::new(),
            settings,
        };
        Ok(Outcome::job(self.inner.ops.submit_into(
            JobSpec { kind, options },
            None,
            list,
        )?))
    }

    /// `archive.delete` (RC-110-05): entries of `panel`'s directory inside an archive, gone for
    /// good - each by the index the archive lists it by, a folder with every entry below it, so
    /// a name cleaned for the listing never stands for another entry.
    pub(crate) async fn archive_delete(
        &self,
        panel: Option<PanelId>,
        entries: &[String],
        options: JobOptions,
    ) -> Result<Outcome, EngineError> {
        let list = self.tool_list().await;
        let id = self.resolve(panel);
        self.remember_selection(id, entries);
        let (here, targets) = {
            let state = self.state();
            let panel = state.panel(id);
            check_mcp_pin(id, &panel.path)?;
            (panel.path.clone(), sources_of(panel, entries)?)
        };
        let BackendId::Archive { outer, format } = &here.backend else {
            return Err(not_an_archive(&here));
        };
        check_modifiable(*format, outer)?;
        let fs = self.archive_fs(&here.backend)?;
        let mut remove = Vec::new();
        for target in &targets {
            if target.backend != here.backend {
                return Err(not_an_archive(target));
            }
            remove.extend(fs.entry_indices(&target.path).await?);
        }
        remove.sort_unstable();
        remove.dedup();
        if remove.is_empty() {
            return Err(EngineError::Ops(OpsError::NoSources));
        }
        let settings = UpdateSettings {
            verify: self.setting_on(VERIFY_UPDATE_KEY).await,
            ..UpdateSettings::default()
        };
        let kind = JobKind::ArchiveUpdate {
            archive: (**outer).clone(),
            add: Vec::new(),
            under: String::new(),
            remove,
            settings,
        };
        Ok(Outcome::job(self.inner.ops.submit_into(
            JobSpec { kind, options },
            None,
            list,
        )?))
    }

    /// The archive file and the folder inside it that `location` names: a place inside an
    /// archive, an archive file (its root), or a typed path through one (`C:\a.zip\docs`).
    async fn update_target(&self, location: &VfsPath) -> Result<(VfsPath, String), EngineError> {
        let inside = if matches!(location.backend, BackendId::Archive { .. }) {
            location.clone()
        } else if let Ok(Some(root)) = self.archive_root(location).await {
            root
        } else {
            self.through_archive(location)
                .await?
                .ok_or_else(|| not_an_archive(location))?
        };
        let BackendId::Archive { outer, format } = &inside.backend else {
            return Err(not_an_archive(location));
        };
        check_modifiable(*format, outer)?;
        let under = inside.path.to_string_lossy().replace('\\', "/");
        Ok(((**outer).clone(), under))
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
