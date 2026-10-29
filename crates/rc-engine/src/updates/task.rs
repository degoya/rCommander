//! The start of the self-update and its background work (RC-100-05, RC-100-07): the first check
//! two minutes after the start plus up to three of jitter, then every
//! `update.check_interval_hours`; the clean-up after a proven update ten minutes after the start.
//! Nothing here is awaited by the start.

use std::path::Path;
use std::time::Duration;

use rc_update::{install::InstanceLock, retention};

use super::{RUNNING, Started};
use crate::Engine;

/// The first check waits this long after the start ...
const FIRST_CHECK: Duration = Duration::from_secs(120);
/// ... plus up to this much, so installations do not ask at the same second.
const JITTER_SECS: u64 = 180;
/// The clean-up after a proven update waits this long after the start.
const CLEAN_UP_AFTER: Duration = Duration::from_secs(600);

impl Engine {
    /// Starts the self-update for the window (RC-100-06): takes this window's share of the
    /// instance lock, proves an update that waited for this start, and starts the checks and the
    /// clean-up in the background. Needs the database; a second call changes nothing.
    pub async fn start_updates(&self, executable: &Path) {
        let Some(data_dir) = self
            .db()
            .ok()
            .and_then(|db| db.path())
            .and_then(Path::parent)
            .map(Path::to_path_buf)
        else {
            return;
        };
        let instance = match InstanceLock::share(&data_dir) {
            Ok(lock) => lock,
            Err(error) => {
                tracing::warn!(%error, "the instance lock is not held; an update may not wait for this window");
                None
            }
        };
        let started = Started {
            installation: rc_update::install_kind::detect(executable),
            data_dir: data_dir.clone(),
            _instance: instance,
        };
        if self.desk().started.set(started).is_err() {
            return;
        }
        self.load_update_state().await;
        self.note_update_result(&data_dir).await;
        let engine = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(FIRST_CHECK + jitter()).await;
            loop {
                engine.scheduled_update_check().await;
                let hours = u64::from(engine.update_config().await.interval_hours.max(1));
                tokio::time::sleep(Duration::from_secs(hours * 60 * 60)).await;
            }
        });
        let engine = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(CLEAN_UP_AFTER).await;
            engine.clean_up_after_update().await;
        });
    }

    /// After a proven update: the newest database copy stays (and after
    /// `update.backup_retention_days` it goes too), the downloads go, the trash is swept.
    pub(crate) async fn clean_up_after_update(&self) {
        let Some(started) = self.desk().started.get() else {
            return;
        };
        let retention_days = self.update_config().await.retention_days;
        let data_dir = started.data_dir.clone();
        let install_dir = started.installation.install_dir.clone();
        let swept = tokio::task::spawn_blocking(move || {
            let policy = retention::Policy {
                proven: rc_update::install::recover::update_proven(&data_dir, RUNNING),
                retention_days,
                now: std::time::SystemTime::now(),
            };
            retention::sweep(&data_dir, &install_dir, policy)
        })
        .await;
        match swept {
            Ok(report) if !report.removed.is_empty() => {
                tracing::info!(
                    files = report.removed.len(),
                    bytes = report.freed_bytes,
                    "the copies of the last update were thinned out"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "the clean-up after the update did not run"),
        }
    }
}

/// Up to [`JITTER_SECS`] seconds, from the clock's nanoseconds: no generator needed for this.
fn jitter() -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| u64::from(since.subsec_nanos()))
        .unwrap_or(0);
    Duration::from_secs(nanos % (JITTER_SECS + 1))
}
