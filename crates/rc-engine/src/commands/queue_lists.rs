//! The queue's named lists (RC-010-15): `queue.lists`, `queue.create`, `queue.rename`,
//! `queue.delete`, `queue.start`, `queue.pause`, `queue.assign`, `queue.assign_many`, and saved transfer lists with
//! `queue.save`, `queue.load` and `queue.delete_saved`.
//!
//! The lists themselves are the queue's (`rc-ops`); a saved list is a row of `rc-db` with its
//! copies and moves as JSON - sources, target, kind, options: paths and connection ids, never a
//! secret. Only locations on this machine or a saved connection are saved (a quick connection
//! means nothing after a restart, an archive is read-only). A saved list is read back like any
//! other input: an entry that does not read, or whose connection is gone, is named as not
//! loadable - never dropped in silence, never run. A list loaded is stopped until it is started.
//!
//! Under MCP (RC-080-42) a load is gated by the jobs it holds (`Engine::effective_class`: one
//! that overwrites makes it destructive), the MCP server judges every place an entry writes to
//! first ([`Engine::saved_queue_places`]), and an entry whose sources hold a file the read guard
//! keeps from MCP is not loaded (`sensitive_path`). A database error on one entry names that
//! entry and loads the others.

use rc_ops::{JobId, JobKind, JobSpec, QueueListId, check_list_name};
use rc_vfs::{BackendId, VfsPath};
use serde::{Deserialize, Serialize};

use crate::{
    ConnectionError, ConnectionId, Engine, EngineError, EngineEvent, Outcome, read_guard,
    transfers::{locations, plain},
};

/// A saved transfer list, without its jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedList {
    pub id: i64,
    pub name: String,
    /// Loaded again, stopped, when the app starts.
    pub restore_on_start: bool,
    /// When it was saved last, `strftime('%Y-%m-%dT%H:%M:%fZ')`.
    pub saved_at: String,
    /// How many jobs it holds.
    pub entries: u64,
}

impl From<rc_db::SavedQueue> for SavedList {
    fn from(saved: rc_db::SavedQueue) -> Self {
        Self {
            id: saved.id,
            name: saved.name,
            restore_on_start: saved.restore_on_start,
            saved_at: saved.saved_at,
            entries: saved.entries,
        }
    }
}

/// An entry of a saved list that was not loaded, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unloadable {
    /// Its place in the saved list, 0 first.
    pub position: usize,
    /// `connection_not_found` (the connection was deleted), `invalid_entry` (it does not read
    /// as a copy or move of plain locations), `sensitive_path` (under MCP, a source holds a file
    /// the read guard keeps from MCP), a database error's code, or the queue's refusal.
    pub code: String,
}

impl Engine {
    /// `queue.lists`: the saved lists only while a database is attached.
    pub(crate) async fn queue_lists(&self) -> Result<Outcome, EngineError> {
        let saved = match self.db() {
            Ok(db) => db.saved_queues().await?,
            Err(_) => Vec::new(),
        };
        Ok(Outcome::QueueLists {
            lists: self.inner.ops.lists(),
            saved: saved.into_iter().map(SavedList::from).collect(),
        })
    }

    /// The lists after a change, announced to the queue window.
    async fn lists_changed(&self) -> Result<Outcome, EngineError> {
        self.publish(EngineEvent::QueueChanged);
        self.queue_lists().await
    }

    /// `queue.create`: with `jobs`, every one of them is known and none runs at once
    /// (RC-010-25) before the list is made, and goes into it.
    pub(crate) async fn queue_create(
        &self,
        name: &str,
        jobs: &[JobId],
    ) -> Result<Outcome, EngineError> {
        self.inner.ops.check_movable(jobs)?;
        let list = self.inner.ops.create_list(name)?.id;
        if !jobs.is_empty() {
            self.inner.ops.assign_many(jobs, list)?;
        }
        self.lists_changed().await
    }

    /// `queue.rename`.
    pub(crate) async fn queue_rename(
        &self,
        list: QueueListId,
        name: &str,
    ) -> Result<Outcome, EngineError> {
        self.inner.ops.rename_list(list, name)?;
        self.lists_changed().await
    }

    /// `queue.delete`.
    pub(crate) async fn queue_delete(&self, list: QueueListId) -> Result<Outcome, EngineError> {
        self.inner.ops.delete_list(list)?;
        self.lists_changed().await
    }

    /// `queue.start`.
    pub(crate) async fn queue_start(&self, list: QueueListId) -> Result<Outcome, EngineError> {
        self.inner.ops.start_list(list)?;
        self.lists_changed().await
    }

    /// `queue.pause`.
    pub(crate) async fn queue_pause(&self, list: QueueListId) -> Result<Outcome, EngineError> {
        self.inner.ops.pause_list(list)?;
        self.lists_changed().await
    }

    /// `queue.assign`.
    pub(crate) fn queue_assign(
        &self,
        job: JobId,
        list: QueueListId,
    ) -> Result<Outcome, EngineError> {
        let entries = self.inner.ops.assign(job, list)?;
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::Queue { entries })
    }

    /// `queue.assign_many`.
    pub(crate) fn queue_assign_many(
        &self,
        jobs: &[JobId],
        list: QueueListId,
    ) -> Result<Outcome, EngineError> {
        let entries = self.inner.ops.assign_many(jobs, list)?;
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::Queue { entries })
    }

    /// `queue.save`: the list's copies and moves between plain locations, under `name` or the
    /// list's own name; every other job is left out and named.
    pub(crate) async fn queue_save(
        &self,
        list: QueueListId,
        name: Option<String>,
        restore_on_start: bool,
    ) -> Result<Outcome, EngineError> {
        let name = match name {
            Some(name) => check_list_name(&name)?,
            None => check_list_name(&self.inner.ops.list(list)?.name)?,
        };
        let (specs, mut left_out) = self.inner.ops.list_specs(list)?;
        let mut texts = Vec::with_capacity(specs.len());
        for (job, spec) in specs {
            let json = locations(&spec.kind)
                .into_iter()
                .all(plain)
                .then(|| serde_json::to_string(&spec).ok())
                .flatten();
            match json {
                Some(json) => texts.push(json),
                None => left_out.push(job),
            }
        }
        if texts.is_empty() {
            return Err(EngineError::QueueListEmpty);
        }
        let saved = self
            .db()?
            .save_queue(&name, restore_on_start, &texts)
            .await?;
        self.publish(EngineEvent::QueueChanged);
        left_out.sort_unstable();
        Ok(Outcome::QueueSaved {
            saved: saved.into(),
            left_out,
        })
    }

    /// `queue.load`: a new list named like the saved one, stopped, with every entry that reads
    /// and whose connections exist, in the saved order.
    pub(crate) async fn queue_load(&self, saved: i64) -> Result<Outcome, EngineError> {
        let (row, texts) = self
            .db()?
            .saved_queue(saved)
            .await?
            .ok_or(EngineError::SavedQueueNotFound { saved })?;
        self.apply_transfer_settings().await;
        let list = self.inner.ops.create_list(&row.name)?.id;
        let mut jobs = Vec::new();
        let mut unloadable = Vec::new();
        for (position, text) in texts.iter().enumerate() {
            // An error names its entry; the list never stops half loaded.
            let loadable = match self.loadable(text).await {
                Ok(loadable) => loadable,
                Err(error) => Err(error.code()),
            };
            let refused = match loadable {
                Ok(spec) => match self.inner.ops.load(spec, list) {
                    Ok(job) => {
                        jobs.push(job);
                        continue;
                    }
                    Err(error) => error.code(),
                },
                Err(code) => code,
            };
            unloadable.push(Unloadable {
                position,
                code: refused.to_owned(),
            });
        }
        if !unloadable.is_empty() {
            tracing::warn!(
                saved,
                count = unloadable.len(),
                "entries of a saved queue list were not loaded"
            );
        }
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::QueueLoaded {
            list: self.inner.ops.list(list)?,
            jobs,
            unloadable,
        })
    }

    /// The job of a saved entry, or the code that says why it cannot be loaded.
    async fn loadable(&self, text: &str) -> Result<Result<JobSpec, &'static str>, EngineError> {
        let Ok(spec) = serde_json::from_str::<JobSpec>(text) else {
            return Ok(Err("invalid_entry"));
        };
        let places = locations(&spec.kind);
        if places.is_empty() || !places.iter().all(|place| plain(place)) {
            return Ok(Err("invalid_entry"));
        }
        let db = self.db()?;
        for place in places {
            if let BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } =
                place.backend
            {
                match db.connection(ConnectionId(connection_id)).await {
                    Ok(_) => {}
                    Err(ConnectionError::ConnectionNotFound { .. }) => {
                        return Ok(Err("connection_not_found"));
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        if read_guard::from_mcp() {
            let sources = match &spec.kind {
                JobKind::Copy { sources, .. } | JobKind::Move { sources, .. } => sources.clone(),
                JobKind::CopyAs { source, .. } | JobKind::MoveAs { source, .. } => {
                    vec![source.clone()]
                }
                _ => Vec::new(),
            };
            if let Err(error) = self.refuse_saved(&sources).await {
                return Ok(Err(error.code()));
            }
        }
        Ok(Ok(spec))
    }

    /// [`Engine::refuse_guarded`] for the sources of a saved entry: a local source with
    /// everything below it, a server's by its path - its backend is not opened for a load.
    async fn refuse_saved(&self, sources: &[VfsPath]) -> Result<(), EngineError> {
        let (local, remote): (Vec<VfsPath>, Vec<VfsPath>) = sources
            .iter()
            .cloned()
            .partition(|source| source.backend == BackendId::Local);
        self.refuse_guarded(&local).await?;
        for source in &remote {
            if let Some(guard) = read_guard::current(&source.backend) {
                guard.check(&source.path).await?;
            }
        }
        Ok(())
    }

    /// Every place an entry of the saved list `saved` writes to, in order, as `(source, place)`:
    /// a copy's or move's source and where it lands below the target, or the target a copy-as
    /// names. Entries that do not read are left out - they are never loaded. For the MCP server's
    /// write guard, which judges them before `queue.load` (RC-080-42).
    ///
    /// # Errors
    ///
    /// No database is attached, it cannot be read, or there is no such list
    /// (`saved_queue_not_found`).
    pub async fn saved_queue_places(
        &self,
        saved: i64,
    ) -> Result<Vec<(VfsPath, VfsPath)>, EngineError> {
        let (_, texts) = self
            .db()?
            .saved_queue(saved)
            .await?
            .ok_or(EngineError::SavedQueueNotFound { saved })?;
        let mut places = Vec::new();
        for spec in texts
            .iter()
            .filter_map(|text| serde_json::from_str::<JobSpec>(text).ok())
        {
            match spec.kind {
                JobKind::Copy { sources, target } | JobKind::Move { sources, target } => {
                    for source in sources {
                        let place = match source.path.file_name() {
                            Some(name) => target.join(name),
                            None => target.clone(),
                        };
                        places.push((source, place));
                    }
                }
                JobKind::CopyAs { source, target } | JobKind::MoveAs { source, target } => {
                    places.push((source, target));
                }
                _ => {}
            }
        }
        Ok(places)
    }

    /// `queue.delete_saved`.
    pub(crate) async fn queue_delete_saved(&self, saved: i64) -> Result<Outcome, EngineError> {
        if !self.db()?.delete_saved_queue(saved).await? {
            return Err(EngineError::SavedQueueNotFound { saved });
        }
        self.lists_changed().await
    }

    /// Loads every saved list marked "restore at start" into a new, stopped list; what each
    /// load answered. The window calls it once at the start, after [`Engine::restore_transfers`];
    /// a headless `rcommander mcp` does not.
    ///
    /// # Errors
    ///
    /// No database is attached, or it cannot be read.
    pub async fn restore_queue_lists(&self) -> Result<Vec<Outcome>, EngineError> {
        let mut loaded = Vec::new();
        for saved in self.db()?.saved_queues().await? {
            if saved.restore_on_start {
                loaded.push(self.queue_load(saved.id).await?);
            }
        }
        Ok(loaded)
    }
}

#[cfg(test)]
#[path = "queue_lists_tests.rs"]
mod tests;
