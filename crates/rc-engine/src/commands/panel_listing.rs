//! A listing that runs long is shown as it arrives (RC-060-02): a server folder of 100 000
//! entries shows its first ones while the rest is still on its way.
//!
//! The backend sends its entries in batches ([`rc_vfs::Vfs::list_batches`]; SFTP one per
//! `READDIR` answer, every other backend the whole listing at once). A listing that ends within
//! [`FIRST_SHOWN`] is shown complete, as before; a longer one is shown with what arrived by then,
//! marked as still arriving ([`crate::PanelState::listing`]), and a task of its own takes in the
//! rest. Like the flat view's (`flat.rs`), it shows what arrived sorted in the panel's order every
//! [`FLUSH_EVERY`] or so - less often as the list grows - and once more at the end, sorted
//! outside the lock; the cursor stays on its entry, the selection and the counted folder sizes
//! stay. A panel that shows anything else by then - another folder, a refresh, a flat view, a
//! tab - ends the task, and with its receiver gone the backend stops reading (SFTP after the
//! block in flight, its handle closed). A listing that breaks off after it was shown keeps what
//! arrived and says why ([`EngineEvent::ListingFailed`]).

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use rc_vfs::{Entry, Vfs, VfsError, VfsPath};
use tokio::{
    sync::mpsc::{self, Receiver, error::TryRecvError},
    task::JoinHandle,
    time::timeout,
};

use super::flat::{ENTRIES_PER_MS, FLUSH_EVERY};
use crate::{
    Engine, EngineError, EngineEvent, PanelChange, PanelId, PanelState, SortOrder,
    flat::next_run,
    sort::{names_for, sort_entries},
    state::is_hidden,
};

/// How long a listing may run before the panel shows what has arrived.
const FIRST_SHOWN: Duration = Duration::from_millis(200);
/// Batches the backend reads ahead of the panel.
const BATCHES_AHEAD: usize = 8;

type Lister = JoinHandle<rc_vfs::Result<()>>;

/// The rest of a listing that is shown before its end.
pub(crate) struct Arriving {
    run: u64,
    /// The folder listed, for the error of a listing that breaks off.
    dir: PathBuf,
    /// Everything that arrived, hidden entries left out unless shown.
    found: Vec<Entry>,
    receiver: Receiver<Vec<Entry>>,
    lister: Lister,
    show_hidden: bool,
}

/// Where the cursor goes while the listing arrives.
pub(crate) struct Placing {
    /// The folder just left, which takes the cursor once it arrives.
    pub(crate) wanted: Option<String>,
    /// The entry the listing put the cursor on; a cursor elsewhere was moved by someone.
    pub(crate) placed: Option<String>,
}

impl Engine {
    /// The entries of `path` in `sort` order: all of them, or - when the listing runs longer than
    /// [`FIRST_SHOWN`] - those that arrived by then, with the rest still arriving.
    pub(crate) async fn first_entries(
        &self,
        vfs: &Arc<dyn Vfs>,
        path: &VfsPath,
        sort: SortOrder,
        show_hidden: bool,
    ) -> Result<(Vec<Entry>, Option<Arriving>), EngineError> {
        let (sender, mut receiver) = mpsc::channel(BATCHES_AHEAD);
        let lister: Lister = tokio::spawn({
            let (vfs, dir) = (Arc::clone(vfs), path.path.clone());
            async move { vfs.list_batches(&dir, sender).await }
        });
        let started = Instant::now();
        let (mut found, mut arrived) = (Vec::new(), false);
        let complete = loop {
            let left = FIRST_SHOWN.saturating_sub(started.elapsed());
            let next = if !arrived {
                receiver.recv().await
            } else if left.is_zero() {
                match receiver.try_recv() {
                    Ok(batch) => Some(batch),
                    Err(TryRecvError::Disconnected) => None,
                    Err(TryRecvError::Empty) => break false,
                }
            } else {
                match timeout(left, receiver.recv()).await {
                    Ok(next) => next,
                    Err(_) => continue,
                }
            };
            match next {
                Some(batch) => {
                    keep(&mut found, batch, show_hidden);
                    arrived = true;
                }
                None => break true,
            }
        };
        if complete {
            ended(lister.await, &path.path)?;
            sort_entries(&mut found, sort, names_for(path));
            return Ok((found, None));
        }
        let mut shown = found.clone();
        sort_entries(&mut shown, sort, names_for(path));
        let arriving = Arriving {
            run: next_run(),
            dir: path.path.clone(),
            found,
            receiver,
            lister,
            show_hidden,
        };
        Ok((shown, Some(arriving)))
    }
}

impl Arriving {
    /// The run the panel shows while the rest arrives.
    pub(crate) fn run(&self) -> u64 {
        self.run
    }

    /// Takes in the rest of the listing for panel `panel`, in a task of its own.
    pub(crate) fn follow(self, engine: Engine, panel: PanelId, placing: Placing) {
        let follower = Follower {
            engine,
            panel,
            arriving: self,
            placing,
            last_flush: Instant::now(),
            dirty: false,
        };
        tokio::spawn(follower.run());
    }
}

/// `batch` added to `found`, hidden entries only when they are shown.
fn keep(found: &mut Vec<Entry>, batch: Vec<Entry>, show_hidden: bool) {
    found.extend(
        batch
            .into_iter()
            .filter(|entry| show_hidden || !is_hidden(entry)),
    );
}

/// The listing task's end as an engine error; a task that did not finish is an I/O error on
/// `dir`.
fn ended(
    joined: Result<rc_vfs::Result<()>, tokio::task::JoinError>,
    dir: &Path,
) -> Result<(), EngineError> {
    match joined {
        Ok(listed) => Ok(listed?),
        Err(join) => Err(VfsError::Io {
            path: dir.to_path_buf(),
            source: io::Error::other(join),
        }
        .into()),
    }
}

/// The task that takes in the rest of one listing.
struct Follower {
    engine: Engine,
    panel: PanelId,
    arriving: Arriving,
    placing: Placing,
    last_flush: Instant,
    /// Entries arrived since the last flush.
    dirty: bool,
}

impl Follower {
    async fn run(mut self) {
        loop {
            match timeout(FLUSH_EVERY, self.arriving.receiver.recv()).await {
                Ok(Some(batch)) => {
                    let shown = self.arriving.show_hidden;
                    keep(&mut self.arriving.found, batch, shown);
                    self.dirty = true;
                }
                Ok(None) => break,
                // Nothing new: whether the panel still waits for it is asked all the same.
                Err(_) => {}
            }
            let still = if self.dirty && self.due() {
                self.flush(false)
            } else {
                self.shown()
            };
            // Returning drops the receiver: the backend stops reading.
            if !still {
                return;
            }
        }
        let failed = ended((&mut self.arriving.lister).await, &self.arriving.dir).err();
        if self.flush(true)
            && let Some(error) = failed
        {
            self.engine.publish(EngineEvent::ListingFailed {
                panel: self.panel,
                code: error.code().to_owned(),
            });
        }
    }

    /// Whether it is time to show what arrived: every [`FLUSH_EVERY`], less often for a long list.
    fn due(&self) -> bool {
        let every = FLUSH_EVERY.max(Duration::from_millis(
            (self.arriving.found.len() / ENTRIES_PER_MS) as u64,
        ));
        self.last_flush.elapsed() >= every
    }

    fn shows_run(&self, panel: &PanelState) -> bool {
        panel.listing == Some(self.arriving.run)
    }

    fn shown(&self) -> bool {
        let state = self.engine.state();
        self.shows_run(state.panel(self.panel))
    }

    /// What arrived, sorted in the panel's order, into the panel - with `done`, as the complete
    /// listing. `false` when the panel no longer shows this run. The next flush is due counted
    /// from the end of this one: a sort slower than the interval (a long list in a debug build)
    /// would otherwise run the flushes back to back, one block apart.
    fn flush(&mut self, done: bool) -> bool {
        loop {
            let (order, names) = {
                let state = self.engine.state();
                let panel = state.panel(self.panel);
                if !self.shows_run(panel) {
                    return false;
                }
                (panel.sort, names_for(&panel.path))
            };
            let sorted = self.dirty.then(|| {
                let mut sorted = self.arriving.found.clone();
                sort_entries(&mut sorted, order, names);
                sorted
            });
            {
                let mut state = self.engine.state();
                let panel = state.panel_mut(self.panel);
                if !self.shows_run(panel) {
                    return false;
                }
                if sorted.is_some() && panel.sort != order {
                    // Sorted another way meanwhile: the next flush has it, the last one waits.
                    if done {
                        continue;
                    }
                    self.last_flush = Instant::now();
                    return true;
                }
                match sorted {
                    Some(sorted) => {
                        let run = (!done).then_some(self.arriving.run);
                        *panel = grown(panel, sorted, run, &mut self.placing);
                    }
                    None if done => panel.listing = None,
                    None => {}
                }
            }
            self.dirty = false;
            self.engine.publish(EngineEvent::Panel {
                panel: self.panel,
                change: PanelChange::Listing,
            });
            self.last_flush = Instant::now();
            return true;
        }
    }
}

/// `panel` with the listing so far, `sorted`, still arriving under `run`. The cursor stays on its
/// entry; while nobody moved it, it goes to the folder just left once that arrived, else to the
/// top. The selection, the counted folders and the quick filter are kept.
fn grown(
    panel: &mut PanelState,
    sorted: Vec<Entry>,
    run: Option<u64>,
    placing: &mut Placing,
) -> PanelState {
    let cursor = panel.cursor_entry().map(|entry| entry.name.clone());
    let untouched = cursor == placing.placed;
    let mut next = panel.next(panel.path.clone(), sorted, panel.sort, panel.show_hidden);
    next.selection = std::mem::take(&mut panel.selection);
    next.dir_sizes = std::mem::take(&mut panel.dir_sizes);
    next.archive = panel.archive.take();
    next.listing = run;
    next.cursor = if untouched {
        placing
            .wanted
            .as_deref()
            .and_then(|name| next.index_of(name))
            .unwrap_or(0)
    } else {
        cursor.and_then(|name| next.index_of(&name)).unwrap_or(0)
    };
    placing.placed = next.cursor_entry().map(|entry| entry.name.clone());
    next
}

#[cfg(test)]
#[path = "panel_listing_tests.rs"]
mod tests;
