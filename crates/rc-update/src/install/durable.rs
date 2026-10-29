//! File-system steps that have to survive a stop half-way; the portable switch is built from them.
//! Blocking calls: the updater and a start run them before anything else is busy.
//!
//! Copied from rDownloader's `crates/rd-files/src/durable.rs` at commit 26c8592a6 (RC-100-06).

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// How often [`rename`] tries again while a handle is still being closed.
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// How long a rename waits for a handle that is still being closed (Windows): a process that just
/// ended, or a virus scanner looking at a new executable, holds its files a moment longer.
pub(crate) const RELEASE_WAIT: Duration = Duration::from_secs(20);

/// Whether anything is at `path`, a dangling link included.
#[must_use]
pub(crate) fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Removes the file, link or directory tree at `path`; nothing there is no error.
pub(crate) fn remove_any(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
    .with_context(|| format!("remove {}", path.display()))
}

/// Renames `from` to `to`, retrying a sharing or access violation on Windows for up to
/// [`RELEASE_WAIT`]; elsewhere the first answer is the answer.
pub(crate) fn rename(from: &Path, to: &Path) -> Result<()> {
    rename_within(from, to, RELEASE_WAIT)
}

fn rename_within(from: &Path, to: &Path, release_wait: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        match fs::rename(from, to) {
            Err(error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32))
                    && started.elapsed() < release_wait =>
            {
                std::thread::sleep(RETRY_INTERVAL);
            }
            result => {
                return result
                    .with_context(|| format!("move {} to {}", from.display(), to.display()));
            }
        }
    }
}

/// Makes a rename or a new file in `directory` durable; on Windows a directory cannot be opened
/// this way, and `MoveFileEx` without write-through is what there is.
pub(crate) fn sync_directory(directory: &Path) {
    #[cfg(unix)]
    let _ = fs::File::open(directory).and_then(|handle| handle.sync_all());
    #[cfg(not(unix))]
    let _ = directory;
}

/// Replaces `path` with `bytes` so a stop leaves the old content or the new one: written and
/// synced beside it as `<name>.tmp`, renamed over it, the folder synced.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    let temporary = std::path::PathBuf::from(name);
    {
        let mut file = fs::File::create(&temporary)
            .with_context(|| format!("create {}", temporary.display()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    rename(&temporary, path)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent);
    }
    Ok(())
}

#[cfg(test)]
#[path = "durable_tests.rs"]
mod tests;
