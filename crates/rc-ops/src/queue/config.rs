//! What the queue is given: the backends it works on and its settings.

use std::{sync::Arc, time::Duration};

use rc_vfs::{ArchiveSettings, Vfs};
use serde::{Deserialize, Serialize};

use super::lanes::Limits;
use crate::{resume::RetryPolicy, trash::Trash};

/// A file system the queue can work on, and its recycle bin if it has one.
#[derive(Clone)]
pub struct Backend {
    pub vfs: Arc<dyn Vfs>,
    pub trash: Option<Arc<dyn Trash>>,
}

impl Backend {
    /// A backend without a recycle bin.
    #[must_use]
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self { vfs, trash: None }
    }

    /// The same backend with `trash` as its recycle bin.
    #[must_use]
    pub fn with_trash(mut self, trash: Option<Arc<dyn Trash>>) -> Self {
        self.trash = trash;
        self
    }
}

/// When jobs run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scheduling {
    /// One job at a time per target backend, in the order submitted unless the caller
    /// reorders the waiting ones ([`Queue::reorder`]): two copies onto one disk do not compete, a
    /// copy to a server runs beside a local one. A server's connection takes a few at once
    /// ([`QueueConfig::per_connection`]), and all lanes together at most
    /// [`QueueConfig::max_running`].
    #[default]
    PerTargetBackend,
    /// Every job at once.
    Parallel,
}

/// The queue's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueConfig {
    pub scheduling: Scheduling,
    /// Bytes per read and write of a copy.
    pub buffer_size: usize,
    /// Jobs that run at once on one server's connection (RC-060-05).
    pub per_connection: usize,
    /// Jobs that run at once in all lanes together.
    pub max_running: usize,
    /// The least time between two progress events of one job.
    pub progress_interval: Duration,
    /// Finished jobs kept for [`Queue::jobs`]; older ones are forgotten.
    pub keep_finished: usize,
    /// How archives are read when unpacking: the limits against archive bombs, the zip code page.
    pub archive: ArchiveSettings,
    /// How a transfer meets a lost connection: retries, then a pause (RC-060-07).
    pub retry: RetryPolicy,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            scheduling: Scheduling::default(),
            buffer_size: DEFAULT_BUFFER_SIZE,
            per_connection: DEFAULT_PER_CONNECTION,
            max_running: DEFAULT_MAX_RUNNING,
            progress_interval: Duration::from_millis(100),
            keep_finished: 100,
            archive: ArchiveSettings::default(),
            retry: RetryPolicy::default(),
        }
    }
}

/// Bytes per read and write of a copy unless configured: 256 KiB.
pub const DEFAULT_BUFFER_SIZE: usize = 256 * 1024;
/// Jobs at once on one server's connection unless configured.
pub const DEFAULT_PER_CONNECTION: usize = 2;
/// Jobs at once in all lanes unless configured.
pub const DEFAULT_MAX_RUNNING: usize = 4;

impl QueueConfig {
    pub(super) fn limits(&self) -> Limits {
        Limits {
            per_connection: self.per_connection,
            total: self.max_running,
        }
    }
}
