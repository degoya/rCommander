//! A clipboard image pasted into a terminal as a file path (RC-040-08, owner 2026-10-25): a
//! terminal carries text, not pictures, but Claude Code and others read an image from a path.
//! The window reads the clipboard (`terminal.paste_image` announces [`EngineEvent::PasteImage`])
//! and hands the PNG to [`Engine::place_terminal_image`], which puts it where the terminal's
//! program can read it - the temp folder for a local shell, the same file under `/mnt/...` for
//! WSL, `/tmp` on the server over SFTP for an SSH shell, uploaded without asking - and answers
//! the path to paste. The files go when the terminal closes or its program ends; local ones a
//! crash left behind go a day later.
//!
//! `/tmp` is shared on a server, so an image's name holds 128 random bits, the file is made
//! exclusively (a file or link someone put there first stops it) and is the owner's alone
//! (`0600`) before a byte of the picture is in it.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use rc_term::{TermError, TermId, WSL_SHELL, WslCwd, windows_to_wsl_path};
use rc_vfs::{BackendId, SetAttributes, VfsError, VfsPath, WriteMode};
use tokio::io::AsyncWriteExt;

use super::terminal::Terminal;
use crate::{ConnectionId, Engine, EngineError, EngineEvent, Outcome};

/// The folder under the temp folder that holds the local images.
const FOLDER: &str = "rcommander";
/// What every image's name starts with.
const PREFIX: &str = "rcommander-clip-";
/// Where an image goes on a server.
const SERVER_FOLDER: &str = "/tmp";
/// How long removing an image from a server may take before it is left.
const SERVER_WAIT: Duration = Duration::from_secs(5);
/// How old a local image a crash left behind is before the next paste removes it.
const STALE: Duration = Duration::from_secs(24 * 60 * 60);
/// An image is its owner's alone.
const PRIVATE: u32 = 0o600;

/// Where a terminal's program runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Local,
    Wsl,
    Server(ConnectionId),
}

/// Where an image is written and the path its program reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placement {
    pub file: VfsPath,
    pub shown: String,
}

/// An image's name, made of `random`: nobody can guess it to put a file or link there first.
pub(crate) fn file_name(random: u128) -> String {
    format!("{PREFIX}{random:032x}.png")
}

/// Where image `name` goes for a program at `place`; `temp` is the local temp folder.
pub(crate) fn placement(place: Place, temp: &Path, name: &str) -> Placement {
    let local = temp.join(FOLDER).join(name);
    match place {
        Place::Local => Placement {
            shown: local.display().to_string(),
            file: VfsPath::local(local),
        },
        Place::Wsl => Placement {
            shown: match windows_to_wsl_path(&local, None) {
                WslCwd::Path(path) => path,
                WslCwd::Home(_) => local.display().to_string(),
            },
            file: VfsPath::local(local),
        },
        Place::Server(connection) => {
            let path = format!("{SERVER_FOLDER}/{name}");
            Placement {
                file: VfsPath {
                    backend: BackendId::Sftp {
                        connection_id: connection.0,
                    },
                    path: PathBuf::from(&path),
                },
                shown: path,
            }
        }
    }
}

/// `path` as it is pasted: in double quotes when it holds a space.
pub(crate) fn quoted(path: &str) -> String {
    if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path.to_owned()
    }
}

/// Where the program of `entry` runs.
fn place_of(entry: &Terminal) -> Place {
    match &entry.remote {
        Some(remote) => Place::Server(remote.connection),
        None if entry.session.shell() == WSL_SHELL => Place::Wsl,
        None => Place::Local,
    }
}

impl Engine {
    /// `terminal.paste_image`: announces the paste for `terminal`, or the one the active panel
    /// shows; without one it does nothing.
    pub(crate) fn paste_image(&self, terminal: Option<TermId>) -> Outcome {
        let panel = self.resolve(None);
        let Some(terminal) = terminal.or_else(|| self.shown_terminal(panel)) else {
            return Outcome::Panel(self.snapshot(Some(panel)));
        };
        self.publish(EngineEvent::PasteImage { terminal });
        Outcome::Panel(self.snapshot(Some(panel)))
    }

    /// Writes `png` where the program of `terminal` can read it and answers the path to paste,
    /// quoted when it holds a space. The file goes when the terminal closes or ends.
    ///
    /// # Errors
    ///
    /// `term_not_found` for a terminal that is gone; the file system's error when the file cannot
    /// be written (on a server: the connection's).
    pub async fn place_terminal_image(
        &self,
        terminal: TermId,
        png: Vec<u8>,
    ) -> Result<String, EngineError> {
        let place = self
            .terminals()
            .get(&terminal)
            .map(place_of)
            .ok_or(TermError::NotFound(terminal))?;
        let temp = std::env::temp_dir();
        sweep_once(&temp);
        let name = file_name(rand::random());
        let placement = placement(place, &temp, &name);
        self.write_image(&placement.file, &png).await?;
        let kept = match self.terminals().get_mut(&terminal) {
            Some(entry) => {
                entry.images.push(placement.file.clone());
                true
            }
            None => false,
        };
        if !kept {
            // Closed while the image was written: nothing will remove it later.
            self.remove_images(vec![placement.file]);
            return Err(TermError::NotFound(terminal).into());
        }
        Ok(quoted(&placement.shown))
    }

    async fn write_image(&self, file: &VfsPath, png: &[u8]) -> Result<(), EngineError> {
        if file.backend == BackendId::Local {
            if let Some(folder) = file.path.parent() {
                tokio::fs::create_dir_all(folder)
                    .await
                    .map_err(|error| VfsError::from_io(error, folder))?;
            }
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(PRIVATE);
            let written = async {
                let mut local = options.open(&file.path).await?;
                local.write_all(png).await?;
                local.flush().await
            };
            return written
                .await
                .map_err(|error| VfsError::from_io(error, &file.path).into());
        }
        let vfs = self.open_vfs(file, false).await?;
        let mut stream = vfs.open_write(&file.path, WriteMode::CreateNew).await?;
        let private = SetAttributes {
            mode: Some(PRIVATE),
            readonly: None,
        };
        if let Err(error) = vfs.set_attributes(&file.path, private).await {
            // Readable by everyone on the server: the picture never goes in.
            drop(stream);
            let _ = vfs.remove_file(&file.path).await;
            return Err(error.into());
        }
        let written = async {
            stream.write_all(png).await?;
            stream.shutdown().await
        };
        written
            .await
            .map_err(|error| VfsError::from_io(error, &file.path).into())
    }

    /// Removes the images of a terminal that closed or ended, in the background; a failure is
    /// only logged.
    pub(super) fn remove_images(&self, images: Vec<VfsPath>) {
        if images.is_empty() {
            return;
        }
        let engine = self.clone();
        tokio::spawn(async move {
            for file in images {
                let removed = if file.backend == BackendId::Local {
                    tokio::fs::remove_file(&file.path)
                        .await
                        .map_err(|error| VfsError::from_io(error, &file.path).into())
                } else {
                    let removing = async {
                        let vfs = engine.open_vfs(&file, false).await?;
                        vfs.remove_file(&file.path).await?;
                        Ok::<(), EngineError>(())
                    };
                    tokio::time::timeout(SERVER_WAIT, removing)
                        .await
                        .unwrap_or_else(|_| Err(EngineError::BackendUnavailable))
                };
                if let Err(error) = removed {
                    tracing::warn!(code = error.code(), "a pasted image could not be removed");
                }
            }
        });
    }
}

/// Removes, once per run, the local images a crash left behind more than [`STALE`] ago.
fn sweep_once(temp: &Path) {
    static SWEPT: OnceLock<()> = OnceLock::new();
    let folder = temp.join(FOLDER);
    SWEPT.get_or_init(|| {
        tokio::spawn(async move { sweep(&folder, SystemTime::now()).await });
    });
}

/// Removes the images in `folder` older than [`STALE`] at `now`.
pub(crate) async fn sweep(folder: &Path, now: SystemTime) {
    let Ok(mut entries) = tokio::fs::read_dir(folder).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(PREFIX) {
            continue;
        }
        let modified = entry.metadata().await.and_then(|meta| meta.modified());
        let stale = modified
            .ok()
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age > STALE);
        if stale {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

#[cfg(test)]
#[path = "terminal_image_tests.rs"]
mod tests;
