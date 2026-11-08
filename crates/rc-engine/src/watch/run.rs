//! The watcher's loop (RC-110-02): it follows the panels through the engine's events, takes the
//! system's notices and the prompt from its channel, polls where it must, and reads a panel when
//! the bundling says so - one panel at a time, never two readings of one panel at once. It holds
//! the engine weakly and ends with it.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Instant, SystemTime},
};

use rc_vfs::VfsPath;
use serde_json::Value;
use tokio::sync::{
    broadcast::{self, error::RecvError},
    mpsc,
};

use super::{
    Signal,
    choose::{self, Mode, Shown, WatchSettings},
    coalesce::{Coalescer, QUIET},
    moves_panels,
    native::{self, Change, Native},
    poll::Poller,
};
use crate::{
    Engine, EngineError, EngineEvent, Inner, PanelId,
    drives::drive_key,
    settings::{
        WATCH_BACKGROUND_KEY, WATCH_CHANGES_KEY, WATCH_EXCLUDE_KEY, WATCH_KEY, WATCH_NETWORK_KEY,
        stored_bool, stored_setting,
    },
};

/// The settings the loop reads again when one of them is stored.
const WATCH_KEYS: [&str; 5] = [
    WATCH_KEY,
    WATCH_CHANGES_KEY,
    WATCH_NETWORK_KEY,
    WATCH_EXCLUDE_KEY,
    WATCH_BACKGROUND_KEY,
];

/// One panel as the loop sees it.
#[derive(Debug, Default)]
struct Slot {
    /// The folder it showed when the loop last looked.
    path: Option<VfsPath>,
    /// A plain file list of this machine: what the prompt reads.
    local: bool,
    /// How the settings say it is watched, and how it is: a folder the system refuses is
    /// polled.
    chosen: Option<Mode>,
    mode: Option<Mode>,
    /// The path the system names the folder's changes by.
    key: PathBuf,
    coalescer: Coalescer,
    poller: Option<Poller>,
    /// Not read before then: a listing still arrives.
    not_before: Option<Instant>,
    /// Nothing was reported since it was last read.
    quiet: bool,
}

impl Slot {
    fn due(&self) -> Option<Instant> {
        let due = self.coalescer.due()?;
        Some(self.not_before.map_or(due, |at| due.max(at)))
    }
}

struct Watcher {
    engine: Weak<Inner>,
    settings: WatchSettings,
    slots: [Slot; 2],
    native: Option<Native>,
    /// For the system watcher's handler.
    sender: mpsc::Sender<Signal>,
    overflow: Arc<AtomicBool>,
}

/// Runs the loop of `engine`'s watcher on the runtime.
pub(super) fn spawn(
    engine: &Engine,
    sender: mpsc::Sender<Signal>,
    signals: mpsc::Receiver<Signal>,
) {
    let watcher = Watcher {
        engine: Arc::downgrade(&engine.inner),
        settings: WatchSettings::default(),
        slots: Default::default(),
        native: None,
        sender,
        overflow: Arc::clone(&engine.inner.watch.overflow),
    };
    tokio::spawn(watcher.run(engine.subscribe(), signals));
}

impl Watcher {
    fn engine(&self) -> Option<Engine> {
        self.engine.upgrade().map(|inner| Engine { inner })
    }

    async fn run(
        mut self,
        mut events: broadcast::Receiver<EngineEvent>,
        mut signals: mpsc::Receiver<Signal>,
    ) {
        let Some(engine) = self.engine() else {
            return;
        };
        self.settings = settings_of(&engine).await;
        self.follow(&engine).await;
        self.publish(&engine);
        drop(engine);
        loop {
            let Some(wake) = self.engine().map(|engine| self.next_wake(&engine)) else {
                return;
            };
            let sleep = sleep_until(wake);
            let (mut follow, mut reload) = (false, false);
            tokio::select! {
                event = events.recv() => match event {
                    Ok(EngineEvent::SettingChanged { key }) => {
                        reload = WATCH_KEYS.contains(&key.as_str());
                    }
                    Ok(event) => follow = moves_panels(&event),
                    // Missed events: the panels are looked at again.
                    Err(RecvError::Lagged(_)) => follow = true,
                    Err(RecvError::Closed) => return,
                },
                signal = signals.recv() => match signal {
                    Some(signal) => self.take(signal, Instant::now()),
                    None => return,
                },
                () = sleep => {}
            }
            let Some(engine) = self.engine() else {
                return;
            };
            if reload {
                self.settings = settings_of(&engine).await;
            }
            if reload || follow {
                self.follow(&engine).await;
            }
            if self.overflow.swap(false, Ordering::AcqRel) {
                self.rescan(Instant::now(), true);
            }
            self.poll(&engine).await;
            self.read_due(&engine).await;
            self.publish(&engine);
        }
    }

    /// With `panels.watch_background` off and another program in front, nothing is read.
    fn paused(&self, engine: &Engine) -> bool {
        !self.settings.background && engine.inner.watch.background.load(Ordering::Acquire)
    }

    /// When the loop has something to do without being woken: a reading or a poll.
    fn next_wake(&self, engine: &Engine) -> Option<Instant> {
        let reads = if self.paused(engine) {
            None
        } else {
            self.slots.iter().filter_map(Slot::due).min()
        };
        let polls = self
            .slots
            .iter()
            .filter_map(|slot| slot.poller.as_ref().map(Poller::next))
            .min();
        reads.into_iter().chain(polls).min()
    }

    /// What `signal` means for the panels at `now`.
    fn take(&mut self, signal: Signal, now: Instant) {
        match signal {
            Signal::Changed { paths, change } => {
                for id in PanelId::BOTH {
                    let slot = &self.slots[id.index()];
                    let concerned = slot.mode == Some(Mode::Native)
                        && paths.iter().any(|path| native::concerns(&slot.key, path));
                    if !concerned {
                        continue;
                    }
                    let counts = match change {
                        Change::Structure => true,
                        Change::Content => self.settings.changes,
                        // The listing is searched only while no reading waits: once per
                        // reading, not once per notice of a burst.
                        Change::Created => {
                            self.settings.changes
                                || slot.coalescer.pending()
                                || self.brings_new(id, &paths)
                        }
                    };
                    let slot = &mut self.slots[id.index()];
                    slot.quiet = false;
                    if counts {
                        slot.coalescer.changed(now);
                    }
                }
            }
            Signal::Rescan => self.rescan(now, false),
            Signal::Prompt => {
                for slot in self.slots.iter_mut().filter(|slot| slot.local) {
                    slot.coalescer.changed(now);
                }
            }
            // The loop only had to wake: a reading held back may be due now.
            Signal::Foreground => {}
        }
    }

    /// Whether a creation notice about `paths` brings panel `id` an entry it does not list yet;
    /// without an engine, yes.
    fn brings_new(&self, id: PanelId, paths: &[PathBuf]) -> bool {
        let Some(engine) = self.engine() else {
            return true;
        };
        let key = &self.slots[id.index()].key;
        let state = engine.state();
        let panel = state.panel(id);
        paths
            .iter()
            .filter(|path| native::concerns(key, path))
            .any(|path| native::brings_new(key, path, |name| panel.index_of(name).is_some()))
    }

    /// Every watched panel is read once; with `all` (a full channel, which may have held a
    /// prompt) every local file list.
    fn rescan(&mut self, now: Instant, all: bool) {
        for slot in &mut self.slots {
            if slot.mode.is_some() || (all && slot.local) {
                slot.quiet = false;
                slot.coalescer.changed(now);
            }
        }
    }

    /// Looks at what the panels show; a panel that shows another folder, or the same one in
    /// another way, starts over (its showing read it).
    async fn follow(&mut self, engine: &Engine) {
        for id in PanelId::BOTH {
            let (path, files, plain) = {
                let state = engine.state();
                let shown = Shown::from(state.panel(id));
                (shown.path.clone(), shown.files, shown.plain)
            };
            let shown = Shown {
                path: &path,
                files,
                plain,
            };
            let local = choose::local_list(shown);
            let chosen = choose::mode(shown, network_of(engine, &path), &self.settings);
            let slot = &mut self.slots[id.index()];
            if slot.path.as_ref() == Some(&path) && slot.local == local && slot.chosen == chosen {
                continue;
            }
            *slot = Slot {
                key: native::watch_key(&path.path),
                path: Some(path),
                local,
                chosen,
                mode: chosen,
                quiet: true,
                ..Slot::default()
            };
        }
        self.watch_natively();
        let now = Instant::now();
        for slot in &mut self.slots {
            if slot.mode != Some(Mode::Poll) || slot.poller.is_some() {
                continue;
            }
            if let Some(path) = &slot.path {
                let modified = modified_of(engine, path).await;
                slot.poller = Some(Poller::new(modified, now));
            }
        }
    }

    /// The system watches the folders of the slots watched natively; one it refuses is polled.
    /// No such folder, no system watcher.
    fn watch_natively(&mut self) {
        let wanted: BTreeSet<PathBuf> = self
            .slots
            .iter()
            .filter(|slot| slot.mode == Some(Mode::Native))
            .map(|slot| slot.key.clone())
            .collect();
        if wanted.is_empty() {
            self.native = None;
            return;
        }
        if self.native.is_none() {
            self.native = Native::new(self.sender.clone(), Arc::clone(&self.overflow))
                .map_err(|error| {
                    tracing::warn!(%error, "the file watcher does not start; folders are polled");
                })
                .ok();
        }
        let refused = match &mut self.native {
            Some(native) => native.set(&wanted),
            None => wanted,
        };
        for slot in &mut self.slots {
            if slot.mode == Some(Mode::Native) && refused.contains(&slot.key) {
                slot.mode = Some(Mode::Poll);
            }
        }
    }

    /// Reads the time of each polled folder that is due; a changed one is to be read.
    async fn poll(&mut self, engine: &Engine) {
        for slot in &mut self.slots {
            let (Some(path), Some(poller)) = (&slot.path, &mut slot.poller) else {
                continue;
            };
            if !poller.wants_stat(Instant::now()) {
                continue;
            }
            let modified = modified_of(engine, path).await;
            let now = Instant::now();
            if poller.seen(modified, now) {
                slot.coalescer.changed(now);
            }
        }
    }

    /// Reads each panel whose reading is due, as it is now: a listing still arriving is not
    /// cut short (the reading follows it), a panel that moved on is the next follow's.
    async fn read_due(&mut self, engine: &Engine) {
        if self.paused(engine) {
            return;
        }
        for id in PanelId::BOTH {
            let index = id.index();
            let now = Instant::now();
            if !self.slots[index].due().is_some_and(|due| due <= now) {
                continue;
            }
            let (path, local, arriving) = {
                let state = engine.state();
                let panel = state.panel(id);
                let local = choose::local_list(Shown::from(panel));
                (panel.path.clone(), local, panel.listing.is_some())
            };
            let slot = &mut self.slots[index];
            if arriving || !local || slot.path.as_ref() != Some(&path) {
                slot.not_before = Some(now + QUIET);
                continue;
            }
            slot.not_before = None;
            slot.coalescer.started(now);
            slot.quiet = true;
            engine.inner.watch.reads[index].fetch_add(1, Ordering::AcqRel);
            let result = engine.refresh(Some(id)).await;
            self.slots[index].coalescer.finished(Instant::now());
            if let Err(error) = result {
                engine.left_behind(id, &path, &error).await;
            }
        }
    }

    /// Tells the window which panels have a quiet system watcher ([`Engine::watch_quiet`]).
    fn publish(&self, engine: &Engine) {
        for (slot, quiet) in self.slots.iter().zip(&engine.inner.watch.quiet) {
            quiet.store(
                slot.mode == Some(Mode::Native) && slot.quiet,
                Ordering::Release,
            );
        }
    }
}

impl Engine {
    /// Reading `path` again failed. When the folder is gone, the panel goes to the nearest
    /// folder above it that is still there; any other failure waits for the next reading.
    async fn left_behind(&self, id: PanelId, path: &VfsPath, error: &EngineError) {
        if self.inner.local.stat(&path.path).await.is_ok() {
            tracing::debug!(code = error.code(), "a watched folder was not read again");
            return;
        }
        let mut up = path.parent();
        while let Some(folder) = up {
            let there = self
                .inner
                .local
                .stat(&folder.path)
                .await
                .is_ok_and(|entry| entry.is_dir_like());
            if there {
                if self.panel_path(id) == *path
                    && let Err(error) = self.show_path(id, folder).await
                {
                    tracing::warn!(
                        code = error.code(),
                        "the panel did not leave a deleted folder"
                    );
                }
                return;
            }
            up = folder.parent();
        }
    }
}

/// Waits until `at`; for ever without one.
async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
        None => std::future::pending().await,
    }
}

/// The watcher's settings in `engine`'s database; the defaults without one.
async fn settings_of(engine: &Engine) -> WatchSettings {
    let Ok(db) = engine.db() else {
        return WatchSettings::default();
    };
    let exclude = stored_setting(db, WATCH_EXCLUDE_KEY)
        .await
        .as_ref()
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    WatchSettings {
        on: stored_bool(db, WATCH_KEY).await,
        changes: stored_bool(db, WATCH_CHANGES_KEY).await,
        network: stored_bool(db, WATCH_NETWORK_KEY).await,
        exclude,
        background: stored_bool(db, WATCH_BACKGROUND_KEY).await,
    }
}

/// Whether `path` lies on another machine, by its form or its drive in the drive bar.
fn network_of(engine: &Engine, path: &VfsPath) -> bool {
    let drive = drive_key(path, &engine.inner.drives.roots())
        .and_then(|key| engine.inner.drives.kind_of(&key));
    choose::on_network(path, drive)
}

/// The modification time of the folder `path`; `None` when it does not answer.
async fn modified_of(engine: &Engine, path: &VfsPath) -> Option<SystemTime> {
    engine
        .inner
        .local
        .stat(&path.path)
        .await
        .ok()
        .and_then(|entry| entry.modified)
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
