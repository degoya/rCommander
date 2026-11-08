//! Changing an archive from the panels (RC-110-05), as Total Commander does: F5 (F6) with the
//! inside of an archive in the target panel opens the pack dialog in its add mode - the format
//! the archive's, fixed; the target the folder the other panel shows inside it; level, solid,
//! paths and "move to archive" - and F8 (Shift+F8, Del) inside an archive asks once whether the
//! entries go for good (there is no recycle bin inside an archive). Each runs as an archive
//! update job the window follows like a copy, both panels read again at its end.
//!
//! An archive that is not changed (a compressed tar, a single compressed file) still opens the
//! dialog; OK then shows the engine's `archive_not_modifiable`, as Total Commander says it.

use rc_engine::{
    BackendId, Command, FileDialog, JobOptions, PackFormat, PackSettings, PanelId, UpdateSettings,
    VfsPath, archive_file, shown_path,
};
use rc_vfs::ArchiveFormat;

use super::{ArchiveDialog, PackDialog, PackStep, job};
use crate::app::{App, Effect};

/// F8 inside an archive: the entries to remove, asked once.
#[derive(Debug, Clone)]
pub struct DeletePrompt {
    pub panel: PanelId,
    pub entries: Vec<String>,
    /// The archive file's name, for the question.
    pub archive: String,
}

/// The pack format of the archive `place` lies in; `None` outside an archive, or for one that is
/// not changed.
pub(super) fn format_of(place: &VfsPath) -> Option<PackFormat> {
    let BackendId::Archive { format, .. } = &place.backend else {
        return None;
    };
    match format {
        ArchiveFormat::Zip => Some(PackFormat::Zip),
        ArchiveFormat::SevenZ => Some(PackFormat::SevenZ),
        ArchiveFormat::Tar { compression: None } => Some(PackFormat::Tar),
        _ => None,
    }
}

impl App {
    /// The file dialog `kind` of `panel` when it concerns an archive: F5 and F6 into the inside
    /// of an archive open the pack dialog adding to it, F8 inside one its question. `None` for
    /// every other case, which opens the file dialog as it always did.
    pub(in crate::app) fn archive_route(
        &mut self,
        panel: PanelId,
        kind: FileDialog,
    ) -> Option<Vec<Effect>> {
        match kind {
            FileDialog::Copy | FileDialog::Move => {
                let target = self.pane(panel.other()).state.path.clone();
                matches!(target.backend, BackendId::Archive { .. })
                    .then(|| self.open_add_dialog(panel, &target, kind == FileDialog::Move))
            }
            FileDialog::Delete | FileDialog::DeletePermanent => {
                let here = self.pane(panel).state.path.clone();
                matches!(here.backend, BackendId::Archive { .. })
                    .then(|| self.open_delete_prompt(panel, &here))
            }
            _ => None,
        }
    }

    /// The pack dialog adding the selection (else the cursor's entry) of `panel` to `target`, the
    /// inside of an archive; with `delete_sources` "move to archive" (F6). Nothing to add - also
    /// the `..` row without a selection - opens nothing.
    fn open_add_dialog(
        &mut self,
        panel: PanelId,
        target: &VfsPath,
        delete_sources: bool,
    ) -> Vec<Effect> {
        let sources = self.pane(panel).chosen_names();
        if sources.is_empty() {
            return Vec::new();
        }
        let settings = PackSettings {
            format: format_of(target).unwrap_or_default(),
            delete_sources,
            ..PackSettings::default()
        };
        self.archive = Some(ArchiveDialog::Pack(PackDialog {
            panel,
            sources,
            name: shown_path(target).display().to_string(),
            settings,
            step: PackStep::Editing,
            base: target.clone(),
            adding: true,
        }));
        Vec::new()
    }

    /// OK in the add mode: once more when the sources go, then the job.
    pub(super) fn confirm_add(&mut self) -> Vec<Effect> {
        let Some(ArchiveDialog::Pack(pack)) = self.archive.as_mut() else {
            return Vec::new();
        };
        match pack.step {
            PackStep::Editing if pack.settings.delete_sources => {
                pack.step = PackStep::Confirm { exists: false };
                Vec::new()
            }
            PackStep::Checking => Vec::new(),
            PackStep::Editing | PackStep::Confirm { .. } => self.submit_add(),
        }
    }

    /// The `archive.add` of the dialog: into the place the other panel shows.
    fn submit_add(&mut self) -> Vec<Effect> {
        let Some(ArchiveDialog::Pack(pack)) = self.archive.take() else {
            return Vec::new();
        };
        let settings = pack.settings;
        let command = Command::ArchiveAdd {
            panel: Some(pack.panel),
            sources: pack.sources,
            // The other panel's place inside the archive, as the dialog showed it.
            target: None,
            settings: UpdateSettings {
                level: settings.level,
                solid: settings.solid,
                recurse: settings.recurse,
                keep_paths: settings.keep_paths,
                delete_sources: settings.delete_sources,
                store_links: settings.store_links,
                ..UpdateSettings::default()
            },
            options: JobOptions::default(),
        };
        vec![job(pack.panel, command)]
    }

    /// F8 inside an archive: the question for the selection (else the cursor's entry).
    fn open_delete_prompt(&mut self, panel: PanelId, here: &VfsPath) -> Vec<Effect> {
        let entries = self.pane(panel).chosen_names();
        if entries.is_empty() {
            return Vec::new();
        }
        let archive = archive_file(here)
            .and_then(|file| file.path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.archive = Some(ArchiveDialog::Delete(DeletePrompt {
            panel,
            entries,
            archive,
        }));
        Vec::new()
    }

    /// "Delete" in the question: the entries go, by their indices in the archive.
    pub(super) fn confirm_delete(&mut self) -> Vec<Effect> {
        let Some(ArchiveDialog::Delete(prompt)) = self.archive.take() else {
            return Vec::new();
        };
        let command = Command::ArchiveDelete {
            panel: Some(prompt.panel),
            entries: prompt.entries,
            options: JobOptions::default(),
        };
        vec![job(prompt.panel, command)]
    }
}

#[cfg(test)]
#[path = "add_tests.rs"]
mod tests;
