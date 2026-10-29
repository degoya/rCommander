//! A stand-in for the system's drives, for the tests: drives that answer at once, slowly or
//! not at all, and a count of the questions; network drives mapped, disconnected and connected
//! again as Windows would, with files open on some.

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use super::{Details, DriveKind, DriveSource, DriveSpace, RawDrive};

#[derive(Default)]
pub(crate) struct FakeDrives {
    drives: Mutex<Vec<RawDrive>>,
    /// How long a drive takes to answer, by id.
    slow: HashMap<String, Duration>,
    /// Drives that answer "not ready".
    failing: Vec<String>,
    /// The ids asked, in order.
    asked: Mutex<Vec<String>>,
    /// Network drives with files open: disconnected only with force.
    busy: Vec<String>,
    /// Remembered network drives not connected: not ready until restored.
    disconnected: Mutex<Vec<String>>,
    /// How long the list takes.
    list_delay: Option<Duration>,
    /// How often the list was asked.
    listed: AtomicUsize,
}

impl FakeDrives {
    /// A drive `id` (also its label) of `kind` at `root`.
    pub(crate) fn with(self, id: &str, kind: DriveKind, root: impl Into<PathBuf>) -> Self {
        self.drives
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(RawDrive {
                id: id.to_owned(),
                label: id.to_owned(),
                kind,
                root: root.into(),
                remote: None,
            });
        self
    }

    /// A network drive `id` at `root`, mapped to `remote`.
    pub(crate) fn network(self, id: &str, root: impl Into<PathBuf>, remote: &str) -> Self {
        let fake = self.with(id, DriveKind::Network, root);
        if let Some(drive) = lock(&fake.drives).last_mut() {
            drive.remote = Some(remote.to_owned());
        }
        fake
    }

    /// Files on `id` are open.
    pub(crate) fn busy(mut self, id: &str) -> Self {
        self.busy.push(id.to_owned());
        self
    }

    /// The network drive `id` is remembered but not connected.
    pub(crate) fn disconnected(self, id: &str) -> Self {
        lock(&self.disconnected).push(id.to_owned());
        self
    }

    /// `id` takes `delay` to answer.
    pub(crate) fn slow(mut self, id: &str, delay: Duration) -> Self {
        self.slow.insert(id.to_owned(), delay);
        self
    }

    /// The list takes `delay`.
    pub(crate) fn slow_list(mut self, delay: Duration) -> Self {
        self.list_delay = Some(delay);
        self
    }

    /// How often the list was asked.
    pub(crate) fn listed(&self) -> usize {
        self.listed.load(Ordering::SeqCst)
    }

    /// `id` answers that it is not ready.
    pub(crate) fn failing(mut self, id: &str) -> Self {
        self.failing.push(id.to_owned());
        self
    }

    /// The drive `id` is gone.
    pub(crate) fn unplug(&self, id: &str) {
        self.drives
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|drive| drive.id != id);
    }

    /// How often `id` was asked.
    pub(crate) fn asked(&self, id: &str) -> usize {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|asked| *asked == id)
            .count()
    }
}

impl DriveSource for FakeDrives {
    fn drives(&self) -> Vec<RawDrive> {
        self.listed.fetch_add(1, Ordering::SeqCst);
        if let Some(delay) = self.list_delay {
            std::thread::sleep(delay);
        }
        lock(&self.drives).clone()
    }

    fn details(&self, root: &Path) -> io::Result<Details> {
        let id = lock(&self.drives)
            .clone()
            .into_iter()
            .find(|drive| drive.root == root)
            .map(|drive| drive.id)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(id.clone());
        if let Some(delay) = self.slow.get(&id) {
            std::thread::sleep(*delay);
        }
        if self.failing.contains(&id) || lock(&self.disconnected).contains(&id) {
            return Err(io::Error::other("no medium"));
        }
        Ok(Details {
            volume: Some(format!("Volume {id}")),
            space: Some(DriveSpace {
                free: 1_000,
                total: 4_000,
            }),
        })
    }

    /// Maps as Windows does: a letter in use is `ERROR_ALREADY_ASSIGNED`, a server `offline`
    /// `ERROR_BAD_NETPATH`, a share `cancel` the person closing the credential dialog.
    fn map_network(&self, letter: char, remote: &str, _reconnect: bool) -> Result<(), u32> {
        let id = format!("{letter}:");
        let mut drives = lock(&self.drives);
        if drives.iter().any(|drive| drive.id == id) {
            return Err(85);
        }
        if remote.starts_with(r"\\offline\") {
            return Err(53);
        }
        if remote.ends_with(r"\cancel") {
            return Err(1223);
        }
        drives.push(RawDrive {
            id: id.clone(),
            label: id,
            kind: DriveKind::Network,
            root: PathBuf::from(format!("{letter}:\\")),
            remote: Some(remote.to_owned()),
        });
        Ok(())
    }

    /// Disconnects as Windows does: `ERROR_NOT_CONNECTED` for a drive that is no network
    /// drive, `ERROR_OPEN_FILES` for a busy one without force.
    fn unmap_network(&self, letter: char, force: bool) -> Result<(), u32> {
        let id = format!("{letter}:");
        let mut drives = lock(&self.drives);
        let Some(at) = drives
            .iter()
            .position(|drive| drive.id == id && drive.kind == DriveKind::Network)
        else {
            return Err(2250);
        };
        if !force && self.busy.contains(&id) {
            return Err(2401);
        }
        drives.remove(at);
        Ok(())
    }

    /// Connects as Windows does: `ERROR_NOT_CONNECTED` for a drive that is no network drive,
    /// `ERROR_BAD_NETPATH` for a share on a server `offline`.
    fn restore_network(&self, letter: char) -> Result<(), u32> {
        let id = format!("{letter}:");
        let remote = lock(&self.drives)
            .iter()
            .find(|drive| drive.id == id && drive.kind == DriveKind::Network)
            .and_then(|drive| drive.remote.clone())
            .ok_or(2250u32)?;
        if remote.starts_with(r"\\offline\") {
            return Err(53);
        }
        lock(&self.disconnected).retain(|drive| *drive != id);
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
