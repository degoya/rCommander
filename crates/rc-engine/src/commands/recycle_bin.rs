//! Where a delete can go into a recycle bin, and F8 where it cannot (owner, 2026-10-18;
//! RC-080-17).
//!
//! Only the local backend has a bin, and only on Windows (`rc_ops::SystemTrash`); a server, an
//! archive, a network share and a WSL distribution have none. Without one F8 and Delete end in
//! `trash_unavailable`: at once on a server, per entry on a share. With the setting
//! `delete.permanent_without_trash` on, `ops.dialog` hands the window the permanent delete's
//! dialog instead of F8's - its question says the entries go for good, as Shift+Delete's does.
//! Nothing deletes without that question, and `ops.delete` itself (an agent's `ops_delete`
//! too) still answers `trash_unavailable` for the recycle bin there.

use rc_vfs::{BackendId, VfsPath};

use crate::{
    DriveKind, Engine, FileDialog, PanelId, drives::drive_key,
    settings::PERMANENT_WITHOUT_TRASH_KEY,
};

impl Engine {
    /// Whether a delete at `path` can go into a recycle bin: the queue has one for its backend,
    /// and it lies on no network share - neither by its form (`\\server\share`,
    /// `\\wsl.localhost\...`) nor on a network drive or WSL distribution of the last drive list.
    /// It reads no file system; a mapped drive the list does not have yet counts as local, and
    /// its entries then answer `trash_unavailable` as before.
    pub(crate) fn has_recycle_bin(&self, path: &VfsPath) -> bool {
        if !self.inner.ops.has_trash(&path.backend) {
            return false;
        }
        match drive_key(path, &self.inner.drives.roots()) {
            Some(key) if key.starts_with(r"\\") => false,
            Some(key) => !matches!(
                self.inner.drives.kind_of(&key),
                Some(DriveKind::Network | DriveKind::Wsl)
            ),
            None => true,
        }
    }

    /// The dialog `ops.dialog` opens for `panel`: F8's as the permanent delete's where the
    /// panel's folder has no recycle bin and the setting asks for it; every other one as it is.
    /// An archive keeps F8's: it is read-only, so its delete is refused either way.
    pub(crate) async fn delete_dialog(&self, panel: PanelId, dialog: FileDialog) -> FileDialog {
        if dialog != FileDialog::Delete {
            return dialog;
        }
        let path = self.state().panel(panel).path.clone();
        if matches!(path.backend, BackendId::Archive { .. })
            || self.has_recycle_bin(&path)
            || !self.setting_on(PERMANENT_WITHOUT_TRASH_KEY).await
        {
            return dialog;
        }
        FileDialog::DeletePermanent
    }
}

#[cfg(test)]
#[path = "recycle_bin_tests.rs"]
mod tests;
