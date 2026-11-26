//! Where the panels start: `RC_START_DIR`, else each panel's start folder setting, else the home
//! directory.
//!
//! A stored start folder is looked at before the window opens, both at once and each for at most
//! [`DETAIL_TIMEOUT`], as the drive bar waits for a drive (RC-110-64): a folder on a share whose
//! server is gone would otherwise hold the window for the operating system's network timeout.
//! One that does not answer in time gives way to the home directory, and the window says so.
//!
//! The engine opens at home; the start folders are looked at while the stored tabs' folders are
//! looked for, side by side under the same limit, so a dead share under both holds the start
//! once, not twice ([`place_panels`]). Then the tabs are shown, and a panel that got none moves
//! to its start folder.

use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use rc_db::Db;
use rc_engine::{
    Command, DETAIL_TIMEOUT, Engine, PanelId, Relocated,
    settings::{LEFT_START_KEY, RIGHT_START_KEY, stored_setting},
};
use rc_vfs::{Entry, LocalFs, Vfs as _};

/// The folders both panels start in, and a stored one that did not answer in time.
#[derive(Debug)]
pub(super) struct StartFolders {
    pub(super) left: PathBuf,
    pub(super) right: PathBuf,
    /// The first stored start folder that did not answer within [`DETAIL_TIMEOUT`].
    pub(super) silent: Option<PathBuf>,
}

/// What [`place_panels`] did besides placing the panels: for the window's first status line.
#[derive(Debug, Default)]
pub(super) struct Placed {
    /// The first stored start folder that did not answer in time.
    pub(super) silent: Option<PathBuf>,
    /// The restored tabs whose folder was gone, and where they are now.
    pub(super) relocated: Vec<Relocated>,
}

/// Puts both panels of `engine`, opened at `home`, where they start: the stored tabs when
/// `restore_tabs` (RC-010-03), else - and for a panel none of whose tabs came back - the start
/// folder. The start folders and the tabs' folders are looked at side by side, each look given
/// up after [`DETAIL_TIMEOUT`].
pub(super) async fn place_panels(
    engine: &Engine,
    db: Option<&Db>,
    home: &Path,
    restore_tabs: bool,
) -> Placed {
    let tabs = async {
        if restore_tabs {
            Some(engine.look_for_tabs().await)
        } else {
            None
        }
    };
    let (start, looked) = tokio::join!(start_folders(db, home), tabs);
    let mut placed = Placed {
        silent: start.silent,
        relocated: Vec::new(),
    };
    let mut restored = [false; 2];
    match looked {
        Some(Ok(looked)) => {
            restored = PanelId::BOTH.map(|panel| looked.restores(panel));
            match engine.show_looked_tabs(looked).await {
                Ok(relocated) => placed.relocated = relocated,
                Err(error) => {
                    tracing::warn!(code = error.code(), "the last run's tabs were not restored");
                }
            }
        }
        Some(Err(error)) => {
            tracing::warn!(code = error.code(), "the last run's tabs were not restored");
        }
        None => {}
    }
    for (panel, folder) in [(PanelId::Left, start.left), (PanelId::Right, start.right)] {
        if restored[panel.index()] || folder == home {
            continue;
        }
        let path = folder.to_string_lossy().into_owned();
        let navigate = Command::Navigate {
            panel: Some(panel),
            path,
        };
        if let Err(error) = engine.dispatch(navigate).await {
            tracing::warn!(
                code = error.code(),
                "a start folder did not open; the panel stays at home"
            );
        }
    }
    placed
}

/// What a look at a stored start folder came to.
#[derive(Debug, PartialEq, Eq)]
enum Looked {
    /// It is a folder: the panel starts there.
    Folder(PathBuf),
    /// None is set, it is gone or it is no folder: the panel starts at home.
    Passed,
    /// It did not answer in time: the panel starts at home, and the window says so.
    Silent(PathBuf),
}

/// Where both panels start; `home` for a panel whose start folder is unset, gone or silent.
async fn start_folders(db: Option<&Db>, home: &Path) -> StartFolders {
    if std::env::var_os("RC_START_DIR").is_some() {
        return StartFolders {
            left: home.to_path_buf(),
            right: home.to_path_buf(),
            silent: None,
        };
    }
    let left = stored_folder(db, LEFT_START_KEY).await;
    let right = stored_folder(db, RIGHT_START_KEY).await;
    let (left, right) = tokio::join!(
        look_locally(left, DETAIL_TIMEOUT),
        look_locally(right, DETAIL_TIMEOUT)
    );
    let mut silent = None;
    let mut chosen = |looked: Looked| match looked {
        Looked::Folder(folder) => folder,
        Looked::Passed => home.to_path_buf(),
        Looked::Silent(folder) => {
            tracing::warn!(
                "a stored start folder did not answer in time; the panel starts at home"
            );
            silent.get_or_insert(folder);
            home.to_path_buf()
        }
    };
    let left = chosen(left);
    let right = chosen(right);
    StartFolders {
        left,
        right,
        silent,
    }
}

/// The start folder setting `key` names; `None` when it is unset or empty.
async fn stored_folder(db: Option<&Db>, key: &str) -> Option<PathBuf> {
    let stored = stored_setting(db?, key).await?;
    stored
        .as_str()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// `folder` looked at on the local disk, for at most `limit`.
async fn look_locally(folder: Option<PathBuf>, limit: Duration) -> Looked {
    let Some(folder) = folder else {
        return Looked::Passed;
    };
    let local = LocalFs::new();
    look(&folder, local.stat(&folder), limit).await
}

/// What `stat`, the look at `folder`, says within `limit`. A call that is still waiting then
/// is dropped; the operating system's call ends on its blocking thread when it ends.
async fn look(
    folder: &Path,
    stat: impl Future<Output = rc_vfs::Result<Entry>>,
    limit: Duration,
) -> Looked {
    match tokio::time::timeout(limit, stat).await {
        Ok(Ok(entry)) if entry.is_dir_like() => Looked::Folder(folder.to_path_buf()),
        Ok(_) => Looked::Passed,
        Err(_) => Looked::Silent(folder.to_path_buf()),
    }
}

/// Where both panels start without a start folder: `RC_START_DIR` when set, else the home
/// directory.
pub(super) fn start_directory() -> PathBuf {
    std::env::var_os("RC_START_DIR")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
        .unwrap_or_else(root_directory)
}

/// The start when the home directory cannot be listed.
pub(super) fn root_directory() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    }
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod tests;
