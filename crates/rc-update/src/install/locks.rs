//! The two locks of an update (RC-100-06), released by the operating system with the process -
//! never a process id that may have been reused.
//!
//! * [`UpdaterLock`] on `<data>/update/updater.lock`: held by the updater for its whole life;
//!   whether an updater runs is whether it can be taken.
//! * [`InstanceLock`] on `<data>/update/instance.lock`: every window holds it shared while it
//!   runs (`rcommander mcp` does not, so a client's server never blocks an update); the updater
//!   waits until it can take it alone, which is when the last window has ended.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::{INSTANCE_LOCK_FILE, LOCK_FILE, update_dir};

fn open(data: &Path, name: &str) -> Result<fs::File> {
    let directory = update_dir(data);
    fs::create_dir_all(&directory).with_context(|| format!("create {}", directory.display()))?;
    let path = directory.join(name);
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))
}

/// The updater's lock, released with the process.
#[derive(Debug)]
pub struct UpdaterLock {
    _file: fs::File,
}

impl UpdaterLock {
    /// Takes the lock; `None` while another updater holds it.
    ///
    /// # Errors
    ///
    /// When the lock file cannot be opened or locked.
    pub fn acquire(data: &Path) -> Result<Option<Self>> {
        let file = open(data, LOCK_FILE)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(error)) => Err(error).context("lock the updater"),
        }
    }
}

/// Whether an updater runs for this data directory right now.
#[must_use]
pub fn updater_running(data: &Path) -> bool {
    if !update_dir(data).join(LOCK_FILE).exists() {
        return false;
    }
    matches!(UpdaterLock::acquire(data), Ok(None))
}

/// A window's share of the instance lock, held for its life.
#[derive(Debug)]
pub struct InstanceLock {
    _file: fs::File,
}

impl InstanceLock {
    /// Takes a share of the lock for this window. `None` while an updater holds it alone (it
    /// lets go within moments, once it saw the old window end): the window runs without one.
    ///
    /// # Errors
    ///
    /// When the lock file cannot be opened or locked.
    pub fn share(data: &Path) -> Result<Option<Self>> {
        let file = open(data, INSTANCE_LOCK_FILE)?;
        match file.try_lock_shared() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(error)) => Err(error).context("lock the instance"),
        }
    }

    /// Waits up to `wait` until no window holds a share; whether none does. The lock is let go
    /// again at once: the new version takes its own share when it starts.
    ///
    /// # Errors
    ///
    /// When the lock file cannot be opened or locked.
    pub fn wait_until_free(data: &Path, wait: Duration) -> Result<bool> {
        let file = open(data, INSTANCE_LOCK_FILE)?;
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => {
                    file.unlock().context("unlock the instance")?;
                    return Ok(true);
                }
                Err(fs::TryLockError::WouldBlock) if started.elapsed() < wait => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(fs::TryLockError::WouldBlock) => return Ok(false),
                Err(fs::TryLockError::Error(error)) => {
                    return Err(error).context("lock the instance");
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "locks_tests.rs"]
mod tests;
