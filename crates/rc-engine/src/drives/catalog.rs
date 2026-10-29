//! The drives and what they answered, asked off the UI thread with a time limit.
//!
//! Each question about a drive's volume name and space runs on the blocking pool. A list waits
//! for the answers at most the catalog's timeout; a drive that has not answered by then shows as
//! [`DriveState::Waiting`], its question keeps running, and its answer is announced as
//! [`EngineEvent::Drives`] when it comes. While a drive has a question open it is not asked again,
//! so a hanging share costs one blocked thread, not one per refresh. The list itself is asked the
//! same way: while one list question is open no other starts, the last list stands in, and the
//! late list is stored and announced when it comes (RC-080-47).

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rc_vfs::VfsPath;
use tokio::sync::{broadcast, mpsc};

use super::{Details, Drive, DriveGroup, DriveKind, DriveSource, DriveState, RawDrive};
use crate::EngineEvent;

/// How long an answer of a local drive stays good before a list asks again. Network drives are
/// asked again only on request.
const LOCAL_MAX_AGE: Duration = Duration::from_secs(30);

/// The drives, their answers, and the roots of the last drive bar.
pub(crate) struct DriveCatalog {
    source: Arc<dyn DriveSource>,
    timeout: Duration,
    /// Shared with the questions still running.
    known: Arc<Mutex<Known>>,
    /// A list question is open.
    listing: Arc<AtomicBool>,
    events: broadcast::Sender<EngineEvent>,
    /// The id and root of every entry of the last drive bar (WSL included), for
    /// [`super::drive_key`]; read without waiting on any drive.
    roots: Mutex<Vec<(String, PathBuf)>>,
}

#[derive(Default)]
struct Known {
    /// The drives as last listed.
    drives: Vec<RawDrive>,
    /// What each drive answered, by id.
    slots: HashMap<String, Slot>,
}

#[derive(Default)]
struct Slot {
    answer: Option<Answer>,
    answered: Option<Instant>,
    /// A question is open.
    waiting: bool,
}

#[derive(Clone)]
enum Answer {
    Ready(Details),
    NotReady,
}

impl DriveCatalog {
    /// A catalog over `source` that waits at most `timeout` for a drive and announces late
    /// answers on `events`.
    pub(crate) fn new(
        source: Arc<dyn DriveSource>,
        timeout: Duration,
        events: broadcast::Sender<EngineEvent>,
    ) -> Self {
        Self {
            source,
            timeout,
            known: Arc::new(Mutex::new(Known::default())),
            listing: Arc::new(AtomicBool::new(false)),
            events,
            roots: Mutex::new(Vec::new()),
        }
    }

    /// The drives there now. Local drives without a recent answer are asked, network drives only
    /// with `refresh`; the call waits at most the timeout.
    pub(crate) async fn list(&self, refresh: bool) -> Vec<Drive> {
        self.ask_list().await;
        let drives = lock(&self.known).drives.clone();
        let now = Instant::now();
        let asked: Vec<RawDrive> = {
            let known = lock(&self.known);
            drives
                .iter()
                .filter(|drive| due(known.slots.get(&drive.id), drive.kind, refresh, now))
                .cloned()
                .collect()
        };
        self.ask(asked).await;
        let known = lock(&self.known);
        drives.iter().map(|drive| entry(drive, &known)).collect()
    }

    /// Asks the drive `id` now, unless a question to it is still open, and says what it answered
    /// within the timeout.
    pub(crate) async fn probe(&self, id: &str) -> DriveState {
        let drive = {
            let known = lock(&self.known);
            if known.slots.get(id).is_some_and(|slot| slot.waiting) {
                return DriveState::Waiting;
            }
            known.drives.iter().find(|drive| drive.id == id).cloned()
        };
        let Some(drive) = drive else {
            return DriveState::Unknown;
        };
        self.ask(vec![drive.clone()]).await;
        entry(&drive, &lock(&self.known)).state
    }

    /// Where the drives come from, for mapping and disconnecting a network drive.
    pub(crate) fn source(&self) -> Arc<dyn DriveSource> {
        Arc::clone(&self.source)
    }

    /// The roots of the last drive bar.
    pub(crate) fn roots(&self) -> Vec<(String, PathBuf)> {
        self.roots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set_roots(&self, roots: Vec<(String, PathBuf)>) {
        *self.roots.lock().unwrap_or_else(PoisonError::into_inner) = roots;
    }

    /// The kind of the drive `id` as last listed; `None` for one the list does not have.
    pub(crate) fn kind_of(&self, id: &str) -> Option<DriveKind> {
        lock(&self.known)
            .drives
            .iter()
            .find(|drive| drive.id == id)
            .map(|drive| drive.kind)
    }

    /// Asks for the drive list, unless a question for it is still open, and waits at most the
    /// timeout. The answer is stored when it comes, a late one announced: the list the caller
    /// reads afterwards is the new one when it came in time, the last one otherwise.
    async fn ask_list(&self) {
        if self.listing.swap(true, Ordering::SeqCst) {
            tracing::debug!("the drive list is still being asked; the last one stays");
            return;
        }
        let source = Arc::clone(&self.source);
        let known = Arc::clone(&self.known);
        let listing = Arc::clone(&self.listing);
        let events = self.events.clone();
        // Set once this call stops waiting: a list after that is announced.
        let late = Arc::new(AtomicBool::new(false));
        let late_list = Arc::clone(&late);
        let asked = tokio::task::spawn_blocking(move || {
            // Closed however the question ends, a panic of the source included.
            let _open = OpenQuestion(listing);
            let drives = source.drives();
            {
                let mut known = lock(&known);
                known
                    .slots
                    .retain(|id, slot| slot.waiting || drives.iter().any(|d| &d.id == id));
                known.drives = drives;
            }
            if late_list.load(Ordering::SeqCst) {
                // No subscriber is not an error: a headless engine may have none.
                let _ = events.send(EngineEvent::Drives);
            }
        });
        if tokio::time::timeout(self.timeout, asked).await.is_err() {
            tracing::debug!("the drive list did not come in time; the last one stays");
        }
        late.store(true, Ordering::SeqCst);
    }

    /// Asks every drive of `drives` at once and waits for the answers, at most the timeout.
    async fn ask(&self, drives: Vec<RawDrive>) {
        if drives.is_empty() {
            return;
        }
        {
            let mut known = lock(&self.known);
            for drive in &drives {
                known.slots.entry(drive.id.clone()).or_default().waiting = true;
            }
        }
        // Set once this call stops waiting: an answer after that is announced.
        let late = Arc::new(AtomicBool::new(false));
        let (done, mut answers) = mpsc::channel::<()>(drives.len());
        for drive in drives {
            let source = Arc::clone(&self.source);
            let known = Arc::clone(&self.known);
            let events = self.events.clone();
            let late = Arc::clone(&late);
            let done = done.clone();
            tokio::spawn(async move {
                let RawDrive { id, root, .. } = drive;
                let asked = tokio::task::spawn_blocking(move || source.details(&root)).await;
                let answer = match asked {
                    Ok(Ok(details)) => Answer::Ready(details),
                    Ok(Err(error)) => {
                        tracing::debug!(drive = %id, %error, "the drive is not ready");
                        Answer::NotReady
                    }
                    Err(_) => Answer::NotReady,
                };
                {
                    let mut known = lock(&known);
                    let slot = known.slots.entry(id).or_default();
                    slot.answer = Some(answer);
                    slot.answered = Some(Instant::now());
                    slot.waiting = false;
                }
                if late.load(Ordering::SeqCst) {
                    // No subscriber is not an error: a headless engine may have none.
                    let _ = events.send(EngineEvent::Drives);
                }
                let _ = done.send(()).await;
            });
        }
        drop(done);
        let deadline = tokio::time::Instant::now() + self.timeout;
        while let Ok(Some(())) = tokio::time::timeout_at(deadline, answers.recv()).await {}
        late.store(true, Ordering::SeqCst);
    }
}

/// Marks a question as open until it drops.
struct OpenQuestion(Arc<AtomicBool>);

impl Drop for OpenQuestion {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Whether a list asks the drive: never while a question is open; with `refresh` always;
/// a network drive only then; a local one without an answer or with an old one.
fn due(slot: Option<&Slot>, kind: DriveKind, refresh: bool, now: Instant) -> bool {
    if slot.is_some_and(|slot| slot.waiting) {
        return false;
    }
    if refresh {
        return true;
    }
    if kind == DriveKind::Network {
        return false;
    }
    match slot.and_then(|slot| slot.answered) {
        Some(answered) => now.duration_since(answered) >= LOCAL_MAX_AGE,
        None => true,
    }
}

/// The drive bar's entry for `drive`, with the last answer it gave. A drive asked again shows
/// its previous answer until the new one comes.
fn entry(drive: &RawDrive, known: &Known) -> Drive {
    let slot = known.slots.get(&drive.id);
    let (state, details) = match slot.and_then(|slot| slot.answer.clone()) {
        Some(Answer::Ready(details)) => (DriveState::Ready, details),
        Some(Answer::NotReady) => (DriveState::NotReady, Details::default()),
        None if slot.is_some_and(|slot| slot.waiting) => (DriveState::Waiting, Details::default()),
        None => (DriveState::Unknown, Details::default()),
    };
    Drive {
        id: drive.id.clone(),
        group: DriveGroup::Drives,
        kind: drive.kind,
        label: drive.label.clone(),
        root: Some(VfsPath::local(&drive.root)),
        state,
        volume: details.volume,
        space: details.space,
        folder: Vec::new(),
        favourite: false,
        remote: drive.remote.clone(),
    }
}

fn lock(known: &Mutex<Known>) -> MutexGuard<'_, Known> {
    known.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
