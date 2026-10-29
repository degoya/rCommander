//! A job's task: its turn in the lane, the work, the end; a transfer is kept by the store from
//! its submission until it is over (RC-060-07), so a job still waiting for its turn is back after
//! a restart too (RC-080-04).

use std::sync::Arc;

use tokio::sync::oneshot;

use super::{JobShared, lanes::Turn};
use crate::{
    JobChange, JobKind, JobSpec, JobState,
    persist::is_kept,
    runner::{Runner, Setup, Stop},
};

/// The turn of a job in its lane: `Some(None)` without a lane - once its list lets it start -,
/// `None` when the job was cancelled or removed while it waited.
pub(super) async fn take_turn(
    job: &JobShared,
    lane: Option<(Turn, oneshot::Receiver<()>)>,
) -> Option<Option<Turn>> {
    match lane {
        // A lane holds a stopped list's jobs back itself (`lanes.rs`).
        None => job.control.released().await.ok().map(|()| None),
        Some((turn, ready)) => tokio::select! {
            biased;
            () = job.control.cancelled() => None,
            // The lane drops a sender only by sending the turn.
            _ = ready => Some(Some(turn)),
        },
    }
}

/// One job's task: its turn, the work, the end. The turn is held until the task ends, when the
/// lane passes to the next waiting job.
pub(super) async fn run(
    job: Arc<JobShared>,
    kind: JobKind,
    lane: Option<(Turn, oneshot::Receiver<()>)>,
    mut setup: Setup,
) {
    keep(&kind, &mut setup).await;
    let Some(turn) = take_turn(&job, lane).await else {
        // A job cancelled or removed while it waited never starts, and is not kept.
        if let (Some(store), Some(record)) = (&setup.resume.store, setup.resume.record) {
            store.forget(record).await;
        }
        job.finish(Err(Stop::Cancelled), None);
        return;
    };
    work(job, kind, turn, setup).await;
}

/// Records a transfer with the store, once: a submitted job when it is accepted, a loaded one
/// when its turn comes; a restored one has its record already. A job of a batch is kept with it.
pub(super) async fn keep(kind: &JobKind, setup: &mut Setup) {
    if setup.resume.record.is_some() || !is_kept(kind) {
        return;
    }
    if let Some(store) = setup.resume.store.clone() {
        let spec = JobSpec {
            kind: kind.clone(),
            options: setup.options,
        };
        let batch = setup.job.batch.as_ref().map(|batch| batch.id);
        setup.resume.record = store.save_in(&spec, batch).await;
    }
}

/// The work of a job whose turn has come; a kept transfer's record goes once it is over.
pub(super) async fn work(
    job: Arc<JobShared>,
    kind: JobKind,
    _turn: Option<Turn>,
    mut setup: Setup,
) {
    job.update(|snapshot| snapshot.state = JobState::Counting);
    job.emit(JobChange::Started);
    keep(&kind, &mut setup).await;
    let store = setup.resume.store.clone().filter(|_| is_kept(&kind));
    let record = setup.resume.record;
    let mut runner = Runner::new(setup);
    let outcome = runner.run(&kind).await;
    if let (Some(store), Some(record)) = (store, record) {
        store.forget(record).await;
    }
    job.finish(outcome, Some(runner.progress));
}
