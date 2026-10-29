//! The data folder is the user's alone on Unix (RC-080-42): the database in it holds the MCP
//! token and the vault's ciphertext, the instance file beside it the server's port.

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

#[cfg(test)]
#[path = "private_dir_tests.rs"]
mod tests;
