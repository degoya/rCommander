//! The installed WSL distributions, for the drive bar and the WSL terminal.
//!
//! On Windows the names come from `wsl.exe --list --quiet` (decoded from UTF-16LE in
//! `wsl/decode.rs`), the default and the versions from the registry; why each source is the one
//! used is in `wsl/windows.rs`. Every distribution is a plain UNC share (`\\wsl.localhost\<distro>\`,
//! `\\wsl$\<distro>\` on older Windows 10 builds), so its [`WslDistro::root`] is a local path
//! [`crate::LocalFs`] lists like any other. Elsewhere the list is empty.
//!
//! WSL that is missing, switched off, without a distribution or not answering within
//! [`QUERY_TIMEOUT`] is an empty list with a line in the log - never an error for the user.
//! [`WslCatalog`] keeps the last list; `wsl.refresh` reads it again.

// The parsing and the process call compile and are tested on every platform; only the Windows
// build calls them.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows build calls it")
)]
mod decode;
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows build calls it")
)]
mod details;
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the Windows build calls it")
)]
mod query;
#[cfg(windows)]
mod windows;

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Result, VfsPath};

/// How long one `wsl.exe` call may take before it is killed and nothing is listed.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// One installed distribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WslDistro {
    /// The name as WSL spells it, e.g. `Ubuntu-24.04`.
    pub name: String,
    /// Whether `wsl.exe` without `-d` starts this one.
    pub is_default: bool,
    /// WSL 1 or 2, when known.
    pub version: Option<u8>,
    /// The distribution's `/` as a local path: `\\wsl.localhost\<name>\`.
    pub root: VfsPath,
}

/// The installed distributions, read now; empty when there is no WSL to ask (and on every
/// platform but Windows).
///
/// # Errors
///
/// None so far: every way WSL can be absent is an empty list. The `Result` leaves room for a
/// failure the caller should see.
pub async fn list_distros() -> Result<Vec<WslDistro>> {
    Ok(system_list(QUERY_TIMEOUT).await)
}

#[cfg(windows)]
use windows::list as system_list;

/// No WSL outside Windows.
#[cfg(not(windows))]
async fn system_list(_limit: Duration) -> Vec<WslDistro> {
    Vec::new()
}

/// Where a [`WslCatalog`] reads the list from: the system, or a test's stand-in.
#[async_trait]
trait DistroSource: Send + Sync {
    async fn list(&self) -> Result<Vec<WslDistro>>;
}

struct System;

#[async_trait]
impl DistroSource for System {
    async fn list(&self) -> Result<Vec<WslDistro>> {
        list_distros().await
    }
}

/// The distribution list, read once and kept until [`WslCatalog::refresh`].
///
/// One read at a time: a second caller waits for the read in progress and gets its result
/// instead of starting `wsl.exe` again.
pub struct WslCatalog {
    source: Box<dyn DistroSource>,
    cached: tokio::sync::Mutex<Option<Vec<WslDistro>>>,
}

impl WslCatalog {
    /// A catalog that has read nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::with_source(Box::new(System))
    }

    fn with_source(source: Box<dyn DistroSource>) -> Self {
        Self {
            source,
            cached: tokio::sync::Mutex::new(None),
        }
    }

    /// The kept list, read first if there is none.
    ///
    /// # Errors
    ///
    /// The read failed; nothing is kept then.
    pub async fn distros(&self) -> Result<Vec<WslDistro>> {
        let mut cached = self.cached.lock().await;
        if let Some(distros) = cached.as_ref() {
            return Ok(distros.clone());
        }
        let distros = self.source.list().await?;
        *cached = Some(distros.clone());
        Ok(distros)
    }

    /// The list read again, and kept.
    ///
    /// # Errors
    ///
    /// The read failed; the list kept before stays.
    pub async fn refresh(&self) -> Result<Vec<WslDistro>> {
        let mut cached = self.cached.lock().await;
        let distros = self.source.list().await?;
        *cached = Some(distros.clone());
        Ok(distros)
    }
}

impl Default for WslCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for WslCatalog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WslCatalog")
            .field("cached", &self.cached)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "wsl_tests.rs"]
mod tests;
