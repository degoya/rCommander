//! A lost graphics device (owner, 2026-10-22). When the driver resets or the GPU goes away, wgpu
//! answers every frame with `SurfaceError::Other`; iced 0.14 logs "Error Other when presenting
//! surface." and asks every window to draw again at once. The window stays as it was, a core runs
//! at full load, and the error log once took 15 000 lines a second, 42 million in an evening.
//! iced does not recover from it; a new process does.
//!
//! [`layer`] watches the `tracing` events for that line: [`STORM`] of them within [`WINDOW`] and
//! the window hears it once ([`lost`]). It asks with the system's own message box - its own
//! surface draws nothing any more (`platform/gpu_loss.rs`) - unless `appearance.gpu_loss_notice`
//! is off. Yes stores the tabs and places as a quit does, and [`relaunch_if_wanted`] starts the
//! program again once the window has closed.

use std::{
    sync::{
        LazyLock, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::sync::Notify;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, Registry, filter::LevelFilter, layer::Context};

/// The words of iced's line for a present that failed (`iced_winit`, `run_action`).
const PRESENT_FAILED: &str = "when presenting surface";

/// Failed presents within [`WINDOW`] that mean a lost device; a lone timeout asks nothing.
const STORM: u32 = 100;

/// The time [`STORM`] failed presents must fall into.
const WINDOW: Duration = Duration::from_secs(1);

/// Told once the storm is reached; keeps the news until the window listens.
static LOST: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Whether the window closed to be started again.
static RESTART: AtomicBool = AtomicBool::new(false);

/// The watch for `rc_db::install_diagnostics_with`; only error events reach it.
pub fn layer() -> impl Layer<Registry> + Send + Sync {
    Watch::new(|| LOST.notify_one()).with_filter(LevelFilter::ERROR)
}

/// Resolves when the device is lost, at most once per process.
pub async fn lost() {
    LOST.notified().await;
}

/// The window closes to be started again.
pub fn want_restart() {
    RESTART.store(true, Ordering::Relaxed);
}

/// Starts the program again with its arguments when the window closed for a restart.
pub fn relaunch_if_wanted() {
    if !RESTART.load(Ordering::Relaxed) {
        return;
    }
    let Ok(program) = std::env::current_exe() else {
        return;
    };
    // console-window: allow (rcommander is a GUI program and opens no console of its own)
    let started = std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .spawn();
    if let Err(error) = started {
        tracing::warn!(%error, "the restart after a lost graphics device failed");
    }
}

/// Counts iced's failed presents and calls `on_storm` once they come as a storm.
struct Watch<F> {
    storm: Mutex<Storm>,
    on_storm: F,
}

impl<F> Watch<F> {
    fn new(on_storm: F) -> Self {
        Self {
            storm: Mutex::new(Storm::default()),
            on_storm,
        }
    }
}

impl<S: Subscriber, F: Fn() + Send + Sync + 'static> Layer<S> for Watch<F> {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut text = MessageText::default();
        event.record(&mut text);
        if !text.0.contains(PRESENT_FAILED) {
            return;
        }
        let reached = self
            .storm
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .seen(Instant::now());
        if reached {
            (self.on_storm)();
        }
    }
}

/// Failed presents in the current [`WINDOW`].
#[derive(Debug, Default)]
struct Storm {
    since: Option<Instant>,
    count: u32,
    reached: bool,
}

impl Storm {
    /// Counts a failed present at `now`; `true` the one time [`STORM`] is reached.
    fn seen(&mut self, now: Instant) -> bool {
        if self.reached {
            return false;
        }
        match self.since {
            Some(since) if now.duration_since(since) < WINDOW => self.count += 1,
            _ => {
                self.since = Some(now);
                self.count = 1;
            }
        }
        self.reached = self.count >= STORM;
        self.reached
    }
}

/// An event's `message` field.
#[derive(Default)]
struct MessageText(String);

impl Visit for MessageText {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

#[cfg(test)]
#[path = "gpu_loss_tests.rs"]
mod tests;
