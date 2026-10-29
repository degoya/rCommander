//! The lanes of [`super::Scheduling::PerTargetBackend`]: one job at a time per target backend,
//! the others waiting in an order the caller may change (RC-010-14), like Total Commander's
//! background transfer manager. A server's lane - the connection a transfer waits for - runs up
//! to [`Limits::per_connection`] jobs, and no more than [`Limits::total`] run in all lanes
//! together (RC-060-05). A free place goes to the first waiting job of its lane that is neither
//! paused nor cancelled; a paused waiting job lets the ones behind it go first. The lanes and
//! limits are the queue's, not a list's (RC-010-15): the jobs of every list wait in one lane per
//! backend, and a stopped list's jobs let the others pass like paused ones.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rc_vfs::BackendId;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::JobShared;
use crate::{JobId, OpsError, staging::is_remote};

/// Where [`crate::Queue::reorder`] moves a waiting job among the waiting jobs of its lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reorder {
    /// One place nearer the start.
    Up,
    /// One place further back.
    Down,
    /// First: it starts as soon as the lane is free.
    Front,
    /// Last: every other waiting job of the lane starts first.
    Back,
}

/// How many jobs run at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Limits {
    /// In the lane of one server; at least one.
    pub(super) per_connection: usize,
    /// In all lanes together; at least one.
    pub(super) total: usize,
}

/// One lane: the jobs that hold it and the ones waiting, in the order they start.
struct Lane {
    backend: BackendId,
    running: Vec<JobId>,
    waiting: Vec<Waiting>,
}

struct Waiting {
    job: Arc<JobShared>,
    turn: oneshot::Sender<()>,
}

/// Every lane in use, in the order they were opened, and the limits they share.
struct Board {
    lanes: Vec<Lane>,
    limits: Limits,
}

pub(super) struct Lanes {
    board: Mutex<Board>,
}

/// A job's place in its lane: dropping it - the job ended, or ended while it waited - hands the
/// lane to the next waiting job.
pub(super) struct Turn {
    lanes: Arc<Lanes>,
    backend: BackendId,
    job: JobId,
}

impl Drop for Turn {
    fn drop(&mut self) {
        self.lanes.leave(&self.backend, self.job);
    }
}

impl Lanes {
    pub(super) fn new(limits: Limits) -> Self {
        Self {
            board: Mutex::new(Board {
                lanes: Vec::new(),
                limits,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Board> {
        self.board.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// New limits; jobs already running keep running, waiting ones start where there is room.
    pub(super) fn set_limits(&self, limits: Limits) {
        let mut board = self.lock();
        board.limits = limits;
        start_waiting(&mut board);
    }

    /// Puts `job` at the end of the lane of `backend`. The receiver resolves when its turn comes,
    /// at once when the lane is free.
    pub(super) fn enter(
        self: &Arc<Self>,
        backend: &BackendId,
        job: Arc<JobShared>,
    ) -> (Turn, oneshot::Receiver<()>) {
        let (turn, receiver) = oneshot::channel();
        let id = job.id;
        {
            let mut board = self.lock();
            let lanes = &mut board.lanes;
            let at = match lanes.iter().position(|lane| &lane.backend == backend) {
                Some(at) => at,
                None => {
                    lanes.push(Lane {
                        backend: backend.clone(),
                        running: Vec::new(),
                        waiting: Vec::new(),
                    });
                    lanes.len() - 1
                }
            };
            lanes[at].waiting.push(Waiting { job, turn });
            start_waiting(&mut board);
        }
        let turn = Turn {
            lanes: Arc::clone(self),
            backend: backend.clone(),
            job: id,
        };
        (turn, receiver)
    }

    /// `job` is done with the lane of `backend`, or no longer waits for it.
    fn leave(&self, backend: &BackendId, job: JobId) {
        let mut board = self.lock();
        let Some(lane) = board.lanes.iter_mut().find(|lane| &lane.backend == backend) else {
            return;
        };
        lane.running.retain(|running| *running != job);
        lane.waiting.retain(|waiting| waiting.job.id != job);
        // The place freed may be the total's: every lane gets its chance.
        start_waiting(&mut board);
        board
            .lanes
            .retain(|lane| !lane.running.is_empty() || !lane.waiting.is_empty());
    }

    /// Lets every lane with room start its next jobs: after a waiting job was resumed.
    pub(super) fn kick(&self) {
        start_waiting(&mut self.lock());
    }

    /// Moves the waiting job `job` within its lane.
    pub(super) fn reorder(&self, job: JobId, step: Reorder) -> Result<(), OpsError> {
        let mut board = self.lock();
        let found = board.lanes.iter_mut().find_map(|lane| {
            let at = lane
                .waiting
                .iter()
                .position(|waiting| waiting.job.id == job)?;
            Some((lane, at))
        });
        let Some((lane, at)) = found else {
            return Err(OpsError::JobNotWaiting { job });
        };
        // Each list keeps its own order (RC-010-15): a step passes over other lists' jobs.
        let list = lane.waiting[at].job.list();
        let same = |waiting: &Waiting| waiting.job.list() == list;
        match step {
            Reorder::Up => {
                if let Some(before) = lane.waiting[..at].iter().rposition(same) {
                    lane.waiting.swap(at, before);
                }
            }
            Reorder::Down => {
                if let Some(after) = lane.waiting[at + 1..].iter().position(same) {
                    lane.waiting.swap(at, at + 1 + after);
                }
            }
            Reorder::Front => {
                let waiting = lane.waiting.remove(at);
                lane.waiting.insert(0, waiting);
            }
            Reorder::Back => {
                let waiting = lane.waiting.remove(at);
                lane.waiting.push(waiting);
            }
        }
        Ok(())
    }

    /// Moves the waiting jobs among `jobs` within their lanes, keeping their order among
    /// themselves ([`super::bulk::shift`]); the jobs among them that do not wait are passed over.
    pub(super) fn reorder_many(&self, jobs: &[JobId], step: Reorder) {
        let mut board = self.lock();
        for lane in &mut board.lanes {
            let chosen: Vec<bool> = lane
                .waiting
                .iter()
                .map(|waiting| jobs.contains(&waiting.job.id))
                .collect();
            if chosen.contains(&true) {
                super::bulk::shift(&mut lane.waiting, chosen, step, |waiting| {
                    waiting.job.list()
                });
            }
        }
    }

    /// Whether `job` waits for its turn in a lane.
    pub(super) fn is_waiting(&self, job: JobId) -> bool {
        self.lock()
            .lanes
            .iter()
            .any(|lane| lane.waiting.iter().any(|waiting| waiting.job.id == job))
    }

    /// The waiting jobs of every lane, lane by lane in the order they were opened, each lane's in
    /// the order they start.
    pub(super) fn waiting(&self) -> Vec<Vec<JobId>> {
        self.lock()
            .lanes
            .iter()
            .map(|lane| lane.waiting.iter().map(|waiting| waiting.job.id).collect())
            .collect()
    }
}

/// Hands each lane's free places, lane by lane in the order they were opened, to its first
/// waiting jobs that are neither paused, held by a stopped list nor cancelled, while the total
/// has room; a cancelled one leaves the lane itself.
fn start_waiting(board: &mut Board) {
    let limits = board.limits;
    let mut running: usize = board.lanes.iter().map(|lane| lane.running.len()).sum();
    for lane in &mut board.lanes {
        let room = if is_remote(&lane.backend) {
            limits.per_connection.max(1)
        } else {
            1
        };
        while running < limits.total.max(1) && lane.running.len() < room {
            let Some(at) = lane.waiting.iter().position(|waiting| {
                let control = &waiting.job.control;
                !control.is_paused() && !control.is_held() && !control.is_cancelled()
            }) else {
                break;
            };
            let waiting = lane.waiting.remove(at);
            if waiting.turn.send(()).is_ok() {
                lane.running.push(waiting.job.id);
                running += 1;
            }
        }
    }
}
