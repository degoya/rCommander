//! `drives.list`, `panel.change_drive` and `panel.drive_menu`: the drive bar and the drive menu.
//!
//! The drive bar is the drives of the catalog, then the WSL distributions, then the servers:
//! every connection of the tree with its folders, marked as a favourite when the drive bar shows
//! it as a button of its own (the tree's root level unless the connection manager's marks say
//! otherwise, `favourites.rs`, RC-050-04); a folder is a button that opens the menu at all its
//! servers. Then the quick connections of this run (Ctrl+N), favourites all. An SSH server opens
//! over SFTP (RC-060-02), an FTP or FTPS server over FTP (RC-070-03): changing to it connects
//! and shows the directory last shown there, else the connection's start directory or where the
//! server's login starts.

use std::path::PathBuf;

use rc_vfs::{BackendId, VfsPath, WslDistro};

use super::favourites::Favourites;
use crate::{
    Connection, ConnectionId, ConnectionKind, Drive, DriveGroup, DriveKind, DriveState, Engine,
    EngineError, EngineEvent, FolderNode, Outcome, PanelId,
    drives::{self, drive_key},
    settings::{FOLLOW_ON_OWN_DRIVE_KEY, FOLLOW_OTHER_PANEL_KEY},
};

impl Engine {
    /// `drives.list`: every entry of the drive bar.
    pub(crate) async fn list_drives(&self, refresh: bool) -> Outcome {
        let drives = self.drive_entries(refresh).await;
        if refresh {
            self.publish(EngineEvent::Drives);
        }
        Outcome::Drives { drives }
    }

    /// `panel.change_drive`: the panel shows the other panel's folder when that panel is on the
    /// drive and this one is not (Total Commander; `drives.follow_other_panel`, on by default),
    /// else the directory it last showed on the drive, else the drive's root - each one that is
    /// gone is passed over. A drive letter is asked first, at most [`crate::DETAIL_TIMEOUT`]: one
    /// that does not answer leaves the panel where it is, after a network drive that answered
    /// "not ready" was connected again where Windows remembers it.
    pub(crate) async fn change_drive(
        &self,
        panel: Option<PanelId>,
        wanted: &str,
    ) -> Result<Outcome, EngineError> {
        self.change_drive_to(panel, wanted, false).await
    }

    /// [`Engine::change_drive`]; `typed` for `d:` in the command line, where a panel already on
    /// the drive follows the other panel too, with `drives.follow_on_own_drive` on (RC-010-29).
    pub(crate) async fn change_drive_to(
        &self,
        panel: Option<PanelId>,
        wanted: &str,
        typed: bool,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let entries = self.drive_entries(false).await;
        let drive = drives::find(&entries, wanted).ok_or_else(|| EngineError::DriveNotFound {
            drive: wanted.to_owned(),
        })?;
        let Some(mut root) = drive.root.clone() else {
            return Err(EngineError::BackendUnavailable);
        };
        let announce = self.announces_locked_vault();
        match root.backend {
            BackendId::Sftp { connection_id } => {
                root = self
                    .sftp_start(ConnectionId(connection_id), announce)
                    .await?;
            }
            BackendId::Ftp { connection_id } => {
                root = self
                    .ftp_start(ConnectionId(connection_id), announce)
                    .await?;
            }
            _ => {}
        }
        if drive.group == DriveGroup::Drives {
            let mut state = self.inner.drives.probe(&drive.id).await;
            if state == DriveState::NotReady
                && drive.kind == DriveKind::Network
                && self.restore_network_drive(&drive.id).await
            {
                state = self.inner.drives.probe(&drive.id).await;
            }
            // Its volume and space are new: the drive bar reads them.
            self.publish(EngineEvent::Drives);
            if state != DriveState::Ready {
                return Err(EngineError::DriveNotReady {
                    drive: drive.label.clone(),
                });
            }
        }
        let followed = self.followed_dir(id, &drive.id, typed).await;
        let remembered = self.state().last_dirs[id.index()]
            .get(&drive.id)
            .cloned()
            .filter(|dir| *dir != root && Some(dir) != followed.as_ref());
        for dir in followed.into_iter().chain(remembered) {
            if let Ok(outcome) = self.show_path(id, dir).await {
                return Ok(outcome);
            }
        }
        self.show_path(id, root).await
    }

    /// Connects the remembered network drive `id` that did not answer, as Explorer and Total
    /// Commander do on its first use after sign-in; whether it worked.
    async fn restore_network_drive(&self, id: &str) -> bool {
        let Some(letter) = id.chars().next() else {
            return false;
        };
        let source = self.inner.drives.source();
        let answer = tokio::task::spawn_blocking(move || source.restore_network(letter)).await;
        match answer {
            Ok(Ok(())) => true,
            Ok(Err(code)) => {
                tracing::debug!(drive = %id, code, "the network drive does not reconnect");
                false
            }
            Err(_) => false,
        }
    }

    /// The other panel's folder when it lies on the drive `drive` and the panel `id`'s does not,
    /// with `drives.follow_other_panel` on; for a `typed` drive with `drives.follow_on_own_drive`
    /// on, also when the panel is on that drive. A drive is named as [`drive_key`] names it, so a
    /// WSL distribution and a server count as drives too.
    async fn followed_dir(&self, id: PanelId, drive: &str, typed: bool) -> Option<VfsPath> {
        if !self.setting_on(FOLLOW_OTHER_PANEL_KEY).await {
            return None;
        }
        let roots = self.inner.drives.roots();
        let (own, other) = {
            let state = self.state();
            (
                state.panel(id).path.clone(),
                state.panel(id.other()).path.clone(),
            )
        };
        let on_drive = |path: &VfsPath| {
            drive_key(path, &roots).is_some_and(|key| key.eq_ignore_ascii_case(drive))
        };
        let own_too = typed && self.setting_on(FOLLOW_ON_OWN_DRIVE_KEY).await;
        (on_drive(&other) && (own_too || !on_drive(&own))).then_some(other)
    }

    /// `panel.drive_menu`: announces that the panel's drive menu is wanted.
    pub(crate) fn drive_menu(&self, panel: Option<PanelId>) -> Outcome {
        let snapshot = self.snapshot(panel);
        self.publish(EngineEvent::DriveMenu {
            panel: snapshot.panel,
        });
        Outcome::Panel(snapshot)
    }

    /// The drives, the WSL distributions and the servers, in this order; their roots are kept
    /// for the last directory per drive.
    async fn drive_entries(&self, refresh: bool) -> Vec<Drive> {
        let mut entries = self.inner.drives.list(refresh).await;
        let distros = if refresh {
            self.inner.wsl.refresh().await
        } else {
            self.inner.wsl.distros().await
        };
        match distros {
            Ok(distros) => entries.extend(distros.into_iter().map(wsl_entry)),
            Err(error) => tracing::debug!(%error, "no WSL distributions for the drive bar"),
        }
        entries.extend(self.server_entries().await);
        entries.extend(self.quick_drives());
        self.inner.drives.set_roots(drives::roots_of(&entries));
        entries
    }

    /// The connections of the tree, each with its folders and its mark: the root level's first,
    /// then each folder's in the tree's order; none without a database.
    async fn server_entries(&self) -> Vec<Drive> {
        let Ok(db) = self.db() else {
            return Vec::new();
        };
        let favourites = self.favourites().await;
        match db.connection_tree().await {
            Ok(tree) => {
                let mut entries = Vec::new();
                servers_of(
                    &tree.connections,
                    &tree.folders,
                    &[],
                    &favourites,
                    &mut entries,
                );
                entries
            }
            Err(error) => {
                tracing::warn!(%error, "the servers of the drive bar cannot be read");
                Vec::new()
            }
        }
    }
}

/// The entries of `connections` in the folder `folder`, then those of `folders` and below.
fn servers_of(
    connections: &[Connection],
    folders: &[FolderNode],
    folder: &[String],
    favourites: &Favourites,
    entries: &mut Vec<Drive>,
) {
    entries.extend(connections.iter().map(|connection| Drive {
        id: format!("server:{}", connection.id),
        group: DriveGroup::Servers,
        kind: DriveKind::Server,
        label: connection.name.clone(),
        // Its start is the server's to say; `change_drive` asks for it.
        root: Some(VfsPath {
            backend: server_backend(connection),
            path: PathBuf::from("/"),
        }),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: folder.to_vec(),
        favourite: favourites.contains(connection),
        remote: None,
    }));
    for node in folders {
        let mut inner = folder.to_vec();
        inner.push(node.folder.name.clone());
        servers_of(
            &node.connections,
            &node.folders,
            &inner,
            favourites,
            entries,
        );
    }
}

/// How a server's files are reached: SFTP on an SSH connection, FTP on the others.
fn server_backend(connection: &Connection) -> BackendId {
    let connection_id = connection.id.0;
    match connection.kind {
        ConnectionKind::Ssh => BackendId::Sftp { connection_id },
        ConnectionKind::Ftp | ConnectionKind::Ftps | ConnectionKind::FtpsImplicit => {
            BackendId::Ftp { connection_id }
        }
    }
}

/// A distribution's entry; its id is the key of its share (`\\wsl.localhost\<name>`).
fn wsl_entry(distro: WslDistro) -> Drive {
    let id = drive_key(&distro.root, &[]).unwrap_or_else(|| format!("wsl:{}", distro.name));
    Drive {
        id,
        group: DriveGroup::Wsl,
        kind: DriveKind::Wsl,
        label: distro.name,
        root: Some(distro.root),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: None,
    }
}

#[cfg(test)]
#[path = "drives_tests.rs"]
mod tests;
