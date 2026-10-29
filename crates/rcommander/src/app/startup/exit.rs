//! The way out, bounded (RC-080-58): once the window has stored the tabs and places it ends
//! iced, then the MCP server gets [`MCP_LIMIT`], the app's runtime [`RUNTIME_LIMIT`], and the
//! process ends with `std::process::exit`. A step that hangs - a request the server still
//! answers, a blocking task on a share that does not answer - is given up, never waited for: a
//! process that outlived its window kept the terminals' job objects open, the shells with them,
//! and the exe locked.
//!
//! iced's own runtime is dropped inside `iced::daemon(..).run()` and waits there for every
//! blocking task started on it. So [`leave`] arms a guard thread as iced is told to end: when
//! iced has not returned within [`ICED_LIMIT`], the guard does what is left - the MCP server,
//! this run's copies of archive files, the restart after a lost graphics device - and ends the
//! process. Whichever of the two comes first finishes, once ([`claim`]). Everything the session
//! stores is stored before [`leave`] runs. The limits guard the exit and are no settings.
//!
//! Each phase writes a `debug` line, which stays out of the database's error log (`warn` and
//! `error` only).

use std::{
    future::Future,
    sync::{
        OnceLock, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use iced::Task;
use tokio::runtime::Handle;

use crate::{app::Message, mcp::McpServer};

/// How long the MCP server may take to stop.
const MCP_LIMIT: Duration = Duration::from_secs(2);

/// How long the app's runtime waits for its blocking tasks.
pub(super) const RUNTIME_LIMIT: Duration = Duration::from_secs(1);

/// How long iced may take to end after the window asked it to, before the guard finishes.
const ICED_LIMIT: Duration = Duration::from_secs(1);

/// How long the guard waits for the main thread's own finish: [`MCP_LIMIT`] and
/// [`RUNTIME_LIMIT`] with a second to spare.
const FINISH_LIMIT: Duration = Duration::from_secs(4);

/// What the guard needs to finish without the main thread, set by [`prepare`].
struct Finish {
    runtime: Handle,
    server: McpServer,
}

static FINISH: OnceLock<Finish> = OnceLock::new();

/// Whether the guard runs already.
static ARMED: AtomicBool = AtomicBool::new(false);

/// Whether the finish has been taken by the main thread or the guard.
static FINISHING: AtomicBool = AtomicBool::new(false);

/// Gives the guard the app's runtime and the MCP server; [`run`](super::run) calls it before
/// iced starts. Without it, [`leave`] arms nothing (the tests).
pub(super) fn prepare(runtime: Handle, server: McpServer) {
    let _ = FINISH.set(Finish { runtime, server });
}

/// Ends iced, with the guard armed; called once everything the session stores is stored.
pub(in crate::app) fn leave() -> Task<Message> {
    if let Some(finish) = FINISH.get()
        && !ARMED.swap(true, Ordering::SeqCst)
    {
        tracing::debug!("the session is stored; iced ends");
        let spawned = thread::Builder::new()
            .name("rc-exit-guard".to_owned())
            .spawn(move || guard(finish));
        if let Err(error) = spawned {
            tracing::warn!(%error, "the exit guard did not start");
        }
    }
    iced::exit()
}

/// Whether the caller finishes: true once, for the main thread or the guard.
pub(super) fn claim() -> bool {
    !FINISHING.swap(true, Ordering::SeqCst)
}

/// The main thread when the guard has taken the finish: it waits for the guard to end the
/// process.
pub(super) fn wait_for_guard() -> ! {
    tracing::debug!("iced ended after the guard took over; the guard ends the process");
    loop {
        thread::park();
    }
}

/// The MCP server stopped (or given up), this run's copies of archive files removed and the
/// restart after a lost graphics device started.
pub(super) fn finish(runtime: &Handle, server: &McpServer) {
    let server = server.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(server) = server {
        tracing::debug!(limit = ?MCP_LIMIT, "the MCP server stops");
        match runtime.block_on(within(MCP_LIMIT, server.shutdown())) {
            Some(Ok(())) => tracing::debug!("the MCP server stopped"),
            // Nothing is left to tell about a server that failed while the window closed.
            Some(Err(error)) => tracing::debug!(%error, "the MCP server failed as it stopped"),
            None => {
                tracing::debug!("the MCP server did not stop in time; it ends with the process")
            }
        }
    }
    // This run's copies of files opened from archives; one still held open stays for a later run.
    let removed = rc_engine::unpacked::remove_own();
    tracing::debug!(removed, "this run's copies of archive files removed");
    crate::gpu_loss::relaunch_if_wanted();
}

/// `future`'s output, or `None` once `limit` has passed: no step of the exit waits longer.
pub(super) async fn within<F: Future>(limit: Duration, future: F) -> Option<F::Output> {
    tokio::time::timeout(limit, future).await.ok()
}

/// The guard: finishes when iced hangs, and ends the process when the main thread's finish
/// does not.
fn guard(held: &Finish) {
    thread::sleep(ICED_LIMIT);
    if claim() {
        tracing::debug!(limit = ?ICED_LIMIT, "iced did not end in time; the guard finishes");
        finish(&held.runtime, &held.server);
    } else {
        thread::sleep(FINISH_LIMIT);
        tracing::debug!(limit = ?FINISH_LIMIT, "the exit did not end in time");
    }
    tracing::debug!("the process ends");
    std::process::exit(0);
}

#[cfg(test)]
#[path = "exit_tests.rs"]
mod tests;
