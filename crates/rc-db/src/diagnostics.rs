//! The process's diagnostics (RC-010-11): `tracing` to the console, `warn` and `error` also into
//! the error log, and the retention sweep that keeps the database small.
//!
//! One task does the database's share, after rDownloader's `rd-diagnostics/src/sink.rs`: it
//! stores the layer's lines in batches - one transaction per batch, so a chatty minute costs the
//! writer a handful of transactions - and runs the sweep at start, every [`SWEEP_INTERVAL`] and
//! after every [`SWEEP_AFTER_LINES`] stored lines, reading the retention settings anew each time.
//! A line that repeats the one stored before it is dropped: a renderer that fails on every frame
//! once wrote 15 000 identical lines a second, 41 million rows and 12 GB in an evening.

use std::time::Duration;

use tokio::{runtime::Handle, sync::mpsc, time::MissedTickBehavior};
use tracing::Level;
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    filter::{LevelFilter, filter_fn},
    layer::{Identity, SubscriberExt as _},
    util::SubscriberInitExt as _,
};

use crate::{
    Db,
    error_log::{
        NewErrorLogEntry,
        layer::{self, UNSTORED_TARGET},
    },
};

/// How often the retention sweep runs after the one at start.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Lines stored since the last sweep after which the sink sweeps at once: a flood is cut back to
/// the row limit long before the next [`SWEEP_INTERVAL`].
pub(crate) const SWEEP_AFTER_LINES: usize = 5_000;

/// Lines one write stores at most.
const STORE_BATCH: usize = 200;

/// The console's filter when `RUST_LOG` names none.
const CONSOLE_DEFAULT: &str = "warn";

/// Crates whose `trace` lines carry secrets, dropped whatever `RUST_LOG` says: suppaftp traces
/// every FTP command through the `log` facade, `PASS <password>` included, and the subscriber
/// bridges `log` into `tracing`.
const SECRET_TRACE_TARGETS: &[&str] = &["suppaftp"];

/// Whether a line of `target` at `level` may carry a secret, and is never written.
fn carries_secrets(target: &str, level: &Level) -> bool {
    *level == Level::TRACE
        && SECRET_TRACE_TARGETS.iter().any(|name| {
            target
                .strip_prefix(name)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
        })
}

/// Installs the process's `tracing` subscriber - the console (stderr, filtered by `RUST_LOG`,
/// `warn` by default, never a secret-bearing `trace` line) and the error log (`warn` and `error` only, masked, into `db`) - and
/// starts the task that stores the lines and runs the retention sweep.
///
/// Call once, inside the tokio runtime, after the database is open. Answers whether the
/// subscriber was installed: `false` when the process has one already (the sweep runs anyway)
/// or no runtime is current (then nothing starts).
pub fn install_diagnostics(db: &Db) -> bool {
    install_diagnostics_with(db, Identity::new())
}

/// [`install_diagnostics`] with one more layer of the caller's, which sees every event its own
/// filter lets through: the window's watch for a lost graphics device.
pub fn install_diagnostics_with<L>(db: &Db, extra: L) -> bool
where
    L: Layer<Registry> + Send + Sync + 'static,
{
    let Ok(runtime) = Handle::try_current() else {
        return false;
    };
    let (error_log, lines) = layer::channel();
    let console = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(CONSOLE_DEFAULT)),
        )
        .with_filter(filter_fn(|meta| {
            !carries_secrets(meta.target(), meta.level())
        }));
    let installed = tracing_subscriber::registry()
        .with(extra)
        .with(console)
        .with(error_log.with_filter(LevelFilter::WARN))
        .try_init()
        .is_ok();
    // Not installed, the layer is gone and `lines` closed: the task only sweeps.
    runtime.spawn(run(lines, db.clone()));
    installed
}

/// Stores lines as they come and sweeps on the interval, for the life of the process.
async fn run(lines: mpsc::Receiver<NewErrorLogEntry>, db: Db) {
    let mut lines = Some(lines);
    let mut buffer = Vec::with_capacity(STORE_BATCH);
    let mut recent = Recent::default();
    let mut sweep = tokio::time::interval(SWEEP_INTERVAL);
    sweep.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            received = receive(&mut lines, &mut buffer) => {
                if received == 0 {
                    // Every sender is gone.
                    lines = None;
                } else if take(&db, &mut buffer, &mut recent).await {
                    sweep.reset();
                }
            }
            _ = sweep.tick() => sweep_once(&db).await,
        }
    }
}

/// Waits for at least one line and takes what is ready; 0 once the channel is closed, never
/// with no channel.
async fn receive(
    lines: &mut Option<mpsc::Receiver<NewErrorLogEntry>>,
    buffer: &mut Vec<NewErrorLogEntry>,
) -> usize {
    match lines {
        Some(receiver) => receiver.recv_many(buffer, STORE_BATCH).await,
        None => std::future::pending().await,
    }
}

/// What the sink remembers between two batches.
#[derive(Debug, Default)]
pub(crate) struct Recent {
    /// The line stored last.
    last: Option<NewErrorLogEntry>,
    /// Lines stored since the last sweep.
    since_sweep: usize,
}

/// Stores the buffer without the lines that repeat the one before them, and sweeps once
/// [`SWEEP_AFTER_LINES`] were stored since the last sweep; whether it swept.
pub(crate) async fn take(db: &Db, buffer: &mut Vec<NewErrorLogEntry>, recent: &mut Recent) -> bool {
    buffer.retain(|line| {
        let repeat = recent.last.as_ref() == Some(line);
        if !repeat {
            recent.last = Some(line.clone());
        }
        !repeat
    });
    recent.since_sweep += buffer.len();
    store(db, buffer).await;
    if recent.since_sweep < SWEEP_AFTER_LINES {
        return false;
    }
    recent.since_sweep = 0;
    sweep_once(db).await;
    true
}

/// Writes the buffer and empties it. Lines that cannot be written are dropped, not retried: a
/// store that fails must not grow a backlog in memory.
pub(crate) async fn store(db: &Db, buffer: &mut Vec<NewErrorLogEntry>) {
    if let Err(error) = db.append_error_log(buffer).await {
        tracing::warn!(
            target: UNSTORED_TARGET,
            %error,
            lines = buffer.len(),
            "error log lines could not be stored"
        );
    }
    buffer.clear();
}

/// One retention sweep against the settings as they are now.
pub(crate) async fn sweep_once(db: &Db) {
    let settings = match db.retention_settings().await {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!(%error, "the retention settings cannot be read");
            return;
        }
    };
    match db.prune(&settings).await {
        Ok(report) => tracing::debug!(
            deleted = report.deleted,
            freed_pages = report.freed_pages,
            "retention sweep"
        ),
        Err(error) => tracing::warn!(%error, "the retention sweep failed"),
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
