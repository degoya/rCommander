//! A kept SFTP backend and its release once unused (RC-080-51). The backend holds two channels
//! of the pooled session, which kept the session from ever closing for being idle. A backend
//! the engine has not handed out for the session's idle time, and that no call or job holds,
//! is dropped: its channels end as unused since that last use, so the session closes after its
//! idle time as one without SFTP does. The next use opens a new backend.

use std::{
    sync::{
        Arc, Mutex, PoisonError, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rc_remote::SftpVfs;

use super::backend;
use crate::{ConnectionId, Engine, Inner, sessions::ChannelLease};

/// The references the engine itself holds to a kept backend: the SFTP map, the open backends
/// and the operations queue's registry. Any more means a call or a job has it.
const KEPT_REFS: usize = 3;
/// The longest wait between two looks at a kept backend.
const POLL: Duration = Duration::from_secs(5);

/// When the engine last handed a kept backend out, and whether it was released for not being
/// used since.
#[derive(Debug)]
pub(crate) struct Use {
    at: Mutex<Instant>,
    released: AtomicBool,
}

impl Use {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            at: Mutex::new(Instant::now()),
            released: AtomicBool::new(false),
        })
    }

    /// Notes a use now.
    pub(crate) fn touch(&self) {
        *self.at.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
    }

    fn at(&self) -> Instant {
        *self.at.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A kept backend with its use.
#[derive(Clone)]
pub(crate) struct KeptSftp {
    pub vfs: Arc<SftpVfs>,
    pub used: Arc<Use>,
}

/// The leases of a backend's two channels; they end as unused since the last use when the
/// backend was released for that, else as any lease does.
pub(crate) struct Leases {
    leases: Vec<ChannelLease>,
    used: Arc<Use>,
}

impl Leases {
    pub(crate) fn new(leases: Vec<ChannelLease>, used: Arc<Use>) -> Self {
        Self { leases, used }
    }
}

impl Drop for Leases {
    fn drop(&mut self) {
        if self.used.released.load(Ordering::SeqCst) {
            let since = self.used.at();
            for lease in self.leases.drain(..) {
                lease.end_unused(since);
            }
        }
    }
}

/// Watches the kept backend `vfs` of connection `id` until it is gone or released: released once
/// nobody but the engine holds it and the engine has not handed it out for `idle`.
pub(crate) async fn release_when_unused(
    engine: Weak<Inner>,
    id: ConnectionId,
    vfs: Weak<SftpVfs>,
    used: Arc<Use>,
    idle: Duration,
) {
    let poll = (idle / 2).clamp(Duration::from_millis(100), POLL);
    loop {
        tokio::time::sleep(poll).await;
        match vfs.strong_count() {
            0 => return,
            held if held > KEPT_REFS => {
                used.touch();
                continue;
            }
            _ => {}
        }
        if used.at().elapsed() < idle {
            continue;
        }
        let Some(inner) = engine.upgrade() else {
            return;
        };
        if (Engine { inner }).release_sftp(id, &vfs, &used) {
            return;
        }
    }
}

impl Engine {
    /// Drops the kept backend `vfs` of `id` as unused; `false` when a caller took it meanwhile
    /// (it stays), `true` when it is released or no longer the kept one.
    fn release_sftp(&self, id: ConnectionId, vfs: &Weak<SftpVfs>, used: &Use) -> bool {
        {
            // Handed out only under this lock: a count of the engine's own means nobody has it.
            let mut kept = self.inner.remotes.sftp();
            match kept.get(&id.0) {
                Some(current) if std::ptr::eq(Arc::as_ptr(&current.vfs), vfs.as_ptr()) => {}
                _ => return true,
            }
            if vfs.strong_count() > KEPT_REFS {
                return false;
            }
            used.released.store(true, Ordering::SeqCst);
            kept.remove(&id.0);
        }
        tracing::debug!(connection = %id, "SFTP unused for the idle time; its channels close");
        self.inner.remotes.open().remove(&backend(id));
        self.inner.ops.unregister(&backend(id));
        true
    }
}
