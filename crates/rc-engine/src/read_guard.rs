//! The read guard MCP dispatches run under: files whose contents never leave through MCP.
//!
//! The MCP server attaches its judge once ([`Engine::attach_read_judge`]);
//! [`Engine::dispatch_mcp`] then runs each command with it in a task-local, and every command
//! that reads files it found by itself - a content search, the checksums of a folder or of a
//! checksum file's lines, a comparison by content - asks [`current`] about each of them. A
//! command the window dispatches runs without one: a person may read any file. The same
//! task-local marks a command as an MCP one ([`from_mcp`]) for the decisions that stay a
//! person's, judge or not (the endpoint of a stored secret, `commands/connections.rs`).
//!
//! The same judge guards the jobs an MCP call submits or loads while they run (RC-080-42): the
//! queue is given [`QueueGuards`] with the judge, marks those jobs, and judges each file again
//! right before the job opens it (`rc_ops::ReadGuards`).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::BackendId;

use crate::{Command, Engine, EngineError, Outcome};

/// Why the file at a path must not be read through MCP, or `None` when it may; judged by its
/// name and folders (`rc_mcp::guard::sensitive_path`).
pub type ReadJudge = fn(&Path) -> Option<&'static str>;

/// The reason Total Commander's `wcx_ftp.ini` is refused (`rc_mcp::guard`): its passwords are
/// obfuscated with a fixed key stream, as good as clear text. The import from Total Commander
/// reads the file anyway, under exactly this reason - it never answers a value (RC-080-12).
pub const TC_FTP_PASSWORDS: &str = "Total Commander's stored FTP passwords";

/// The reason an export of Remote Desktop Manager (`*.rdm`) is refused (`rc_mcp::guard`): it
/// keeps its passwords merely obfuscated (`SafePassword`). The import from Remote Desktop Manager
/// reads the file anyway, under exactly this reason - it never answers a password (RC-050-11).
pub const RDM_EXPORT_PASSWORDS: &str = "a Remote Desktop Manager export with its stored passwords";

/// The reason a file in `.ssh` is refused (`rc_mcp::guard`): a key, or another file of the folder
/// but `config`, `known_hosts` and public keys. The `~/.ssh/config` import follows an `Include`
/// in `.ssh` under this one reason, as its own key guard is the stricter judge there (RC-080-12).
pub const SSH_PRIVATE_FILE: &str = "an SSH key or another private file of .ssh";

/// The reason a copy of the database under another name is refused, worded as `rc_mcp`'s guard
/// words it.
const DATABASE_CONTENT: &str = "rCommander's own data";

tokio::task_local! {
    /// Set for every MCP dispatch; the judge when one is attached.
    static MCP: Option<ReadJudge>;
}

/// The judge of the running MCP dispatch, for files of one backend.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReadGuard {
    judge: ReadJudge,
    /// Whether links can be followed on the backend to judge where they really lead.
    local: bool,
}

impl ReadGuard {
    pub(crate) fn new(judge: ReadJudge, backend: &BackendId) -> Self {
        Self {
            judge,
            local: matches!(backend, BackendId::Local),
        }
    }

    /// Why `path` must not be read: judged as it really is on the local backend (links and
    /// junctions followed), then by the name it was reached by; a local copy of rCommander's
    /// database under any other name is known by its content, as `rc_mcp`'s guard knows it
    /// (RC-080-09).
    pub(crate) fn refuses(&self, path: &Path) -> Option<&'static str> {
        // One metadata call per file the command is about to read in full anyway; a SQLite file
        // is read once more, to its schema.
        let real = self
            .local
            .then(|| std::fs::canonicalize(path).ok())
            .flatten();
        real.as_deref()
            .and_then(self.judge)
            .or_else(|| (self.judge)(path))
            .or_else(|| {
                real.as_deref()
                    .is_some_and(rc_db::looks_like_rcommander_database)
                    .then_some(DATABASE_CONTENT)
            })
    }

    /// [`ReadGuard::refuses`] from an async path: on the local backend the links followed and the
    /// database probe touch the file system - a network drive, perhaps - so they run on the
    /// blocking pool, not on a thread of the runtime.
    pub(crate) async fn refuses_off_thread(self, path: &Path) -> Option<&'static str> {
        if !self.local {
            return self.refuses(path);
        }
        let owned = path.to_path_buf();
        match tokio::task::spawn_blocking(move || self.refuses(&owned)).await {
            Ok(reason) => reason,
            // The blocking pool is shutting down: judged here, as it always was.
            Err(_) => self.refuses(path),
        }
    }

    /// [`ReadGuard::refuses_off_thread`] as the engine's `sensitive_path` error.
    pub(crate) async fn check(self, path: &Path) -> Result<(), EngineError> {
        match self.refuses_off_thread(path).await {
            Some(reason) => Err(EngineError::SensitivePath {
                path: path.display().to_string(),
                reason,
            }),
            None => Ok(()),
        }
    }

    /// [`ReadGuard::check`] for each of `paths`, in their order, in one go: on the local backend
    /// one task of the blocking pool judges them all, not one task each. The first path refused
    /// is the error.
    pub(crate) async fn check_all(self, paths: Vec<PathBuf>) -> Result<(), EngineError> {
        let refused = if self.local {
            let paths: Arc<[PathBuf]> = paths.into();
            let judged = Arc::clone(&paths);
            match tokio::task::spawn_blocking(move || self.first_refused(&judged)).await {
                Ok(refused) => refused,
                // The blocking pool is shutting down: judged here, as `check` does.
                Err(_) => self.first_refused(&paths),
            }
        } else {
            self.first_refused(&paths)
        };
        match refused {
            Some((path, reason)) => Err(EngineError::SensitivePath {
                path: path.display().to_string(),
                reason,
            }),
            None => Ok(()),
        }
    }

    /// The first of `paths` that must not be read, with why.
    fn first_refused(&self, paths: &[PathBuf]) -> Option<(PathBuf, &'static str)> {
        paths
            .iter()
            .find_map(|path| self.refuses(path).map(|reason| (path.clone(), reason)))
    }

    /// The guard as a checksum run takes it ([`rc_checksum::Control::with_guard`]).
    pub(crate) fn for_checksums(self) -> rc_checksum::ReadGuard {
        Arc::new(move |path: &Path| self.refuses(path))
    }
}

/// The guard of the MCP dispatch this runs in, for files of `backend`; `None` outside one or
/// without an attached judge.
pub(crate) fn current(backend: &BackendId) -> Option<ReadGuard> {
    MCP.try_with(|judge| judge.map(|judge| ReadGuard::new(judge, backend)))
        .ok()
        .flatten()
}

/// Whether this runs inside an MCP dispatch ([`Engine::dispatch_mcp`]).
pub(crate) fn from_mcp() -> bool {
    MCP.try_with(|_| ()).is_ok()
}

/// Runs `future` as an MCP dispatch would, without a judge: for tests of a command's MCP rules
/// that call the engine's method directly.
#[cfg(test)]
pub(crate) async fn as_mcp<F: std::future::Future>(future: F) -> F::Output {
    MCP.scope(None, future).await
}

/// The judge as the operations queue takes it: a job submitted inside an MCP dispatch is
/// guarded, and reads its files under the judge for their backend.
struct QueueGuards(ReadJudge);

impl rc_ops::ReadGuards for QueueGuards {
    fn applies(&self) -> bool {
        from_mcp()
    }

    fn guard(&self, backend: &BackendId) -> rc_ops::ReadGuard {
        ReadGuard::new(self.0, backend).for_checksums()
    }
}

impl Engine {
    /// Gives the engine the judge its MCP dispatches run under, and the queue the same judge for
    /// the jobs they submit (RC-080-42). Answers `false` when one was attached already (the first
    /// one stays).
    pub fn attach_read_judge(&self, judge: ReadJudge) -> bool {
        let attached = self.inner.read_judge.set(judge).is_ok();
        if attached {
            self.inner.ops.set_read_guards(Arc::new(QueueGuards(judge)));
        }
        attached
    }

    /// Runs `command` as an MCP command, under `judge` when there is one.
    pub(crate) async fn dispatch_judged(
        &self,
        command: Command,
        judge: Option<ReadJudge>,
    ) -> Result<Outcome, EngineError> {
        MCP.scope(judge, self.dispatch(command)).await
    }
}

#[cfg(test)]
#[path = "read_guard_tests.rs"]
mod tests;
