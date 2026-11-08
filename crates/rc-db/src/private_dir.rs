//! The data folder is the user's alone on Unix (RC-080-42): the database in it holds the MCP
//! token and the vault's ciphertext, the instance file beside it the server's port. The database
//! file and SQLite's files beside it are owner-only too (RC-080-68), not only through the folder.
//!
//! A portable copy's folder (RC-100-95) may be on a FAT or exFAT stick, which keeps no Unix
//! modes and refuses `chmod`: there the modes are tried ([`Modes::BestEffort`]) and a failure is
//! reported, not fatal - the stick is the person's to guard.

use std::{io, path::Path};

/// Whether the owner-only modes must hold or are only tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Modes {
    /// The user's data folder: a mode that cannot be set fails the open.
    Enforced,
    /// A portable copy's folder: a mode that cannot be set is reported.
    BestEffort,
}

impl Modes {
    /// What the `outcome` of a mode change means: whether the mode holds, or the error when it
    /// must and does not. Only Unix keeps modes to set.
    #[cfg(any(unix, test))]
    pub(crate) fn settle(self, outcome: io::Result<()>) -> io::Result<bool> {
        match (outcome, self) {
            (Ok(()), _) => Ok(true),
            (Err(_), Self::BestEffort) => Ok(false),
            (Err(error), Self::Enforced) => Err(error),
        }
    }
}

/// Creates `dir` with its missing parents; on Unix every folder it creates is owner-only
/// (`0700`), and the platform's data folder `data_dir` is made owner-only also when it was
/// there before (an older rCommander created it `0755`). A folder the person named elsewhere is
/// left as it is. On Windows the folders inherit the ACL of their parent - in a profile the
/// person's own. Whether every mode holds; `false` only with [`Modes::BestEffort`].
pub(crate) async fn create_private_dir(
    dir: &Path,
    data_dir: Option<&Path>,
    modes: Modes,
) -> io::Result<bool> {
    let mut builder = tokio::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(dir).await?;
    #[cfg(unix)]
    if data_dir == Some(dir) {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = tokio::fs::metadata(dir).await?.permissions().mode();
        if mode & 0o077 != 0 {
            let owner_only = std::fs::Permissions::from_mode(mode & 0o700);
            return modes.settle(tokio::fs::set_permissions(dir, owner_only).await);
        }
    }
    #[cfg(not(unix))]
    let _ = (data_dir, modes);
    Ok(true)
}

/// Makes the database at `path` owner-only (`0600`) on Unix before it is opened (RC-080-68): a
/// missing file is created so, and the file and SQLite's files beside it (`-wal`, `-shm`,
/// `-journal`) are tightened when an older rCommander or another program left them open to
/// others. SQLite gives the `-wal` and `-shm` it creates the database file's permissions. A
/// symbolic link is left as it is, never followed. On Windows the files inherit the ACL of the
/// private folder. Whether every mode holds; `false` only with [`Modes::BestEffort`].
pub(crate) async fn secure_database_files(path: &Path, modes: Modes) -> io::Result<bool> {
    #[cfg(unix)]
    let kept = {
        use std::os::unix::fs::PermissionsExt as _;
        let mut kept = true;
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .await
        {
            // An empty file is an empty database to SQLite.
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut name = path.as_os_str().to_owned();
            name.push(suffix);
            let file = std::path::PathBuf::from(name);
            let meta = match tokio::fs::symlink_metadata(&file).await {
                Ok(meta) => meta,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let mode = meta.permissions().mode();
            if meta.is_file() && mode & 0o177 != 0 {
                let owner_only = std::fs::Permissions::from_mode(mode & 0o600);
                kept &= modes.settle(tokio::fs::set_permissions(&file, owner_only).await)?;
            }
        }
        kept
    };
    #[cfg(not(unix))]
    let kept = {
        let _ = (path, modes);
        true
    };
    Ok(kept)
}

#[cfg(test)]
#[path = "private_dir_tests.rs"]
mod tests;
