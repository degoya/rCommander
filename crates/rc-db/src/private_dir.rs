//! The data folder is the user's alone on Unix (RC-080-42): the database in it holds the MCP
//! token and the vault's ciphertext, the instance file beside it the server's port. The database
//! file and SQLite's files beside it are owner-only too (RC-080-68), not only through the folder.

use std::{io, path::Path};

/// Creates `dir` with its missing parents; on Unix every folder it creates is owner-only
/// (`0700`), and the platform's data folder `data_dir` is made owner-only also when it was
/// there before (an older rCommander created it `0755`). A folder the person named elsewhere is
/// left as it is. On Windows the folders inherit the ACL of their parent - in a profile the
/// person's own.
pub(crate) async fn create_private_dir(dir: &Path, data_dir: Option<&Path>) -> io::Result<()> {
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
            tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode & 0o700)).await?;
        }
    }
    #[cfg(not(unix))]
    let _ = data_dir;
    Ok(())
}

/// Makes the database at `path` owner-only (`0600`) on Unix before it is opened (RC-080-68): a
/// missing file is created so, and the file and SQLite's files beside it (`-wal`, `-shm`,
/// `-journal`) are tightened when an older rCommander or another program left them open to
/// others. SQLite gives the `-wal` and `-shm` it creates the database file's permissions. A
/// symbolic link is left as it is, never followed. On Windows the files inherit the ACL of the
/// private folder.
pub(crate) async fn secure_database_files(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
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
                tokio::fs::set_permissions(&file, owner_only).await?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
#[path = "private_dir_tests.rs"]
mod tests;
