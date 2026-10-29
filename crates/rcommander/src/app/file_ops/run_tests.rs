use rc_engine::{
    Command, JobChange, JobId, JobKind, JobOptions, JobSnapshot, JobState, PanelId, Progress,
    VfsPath,
};

use super::{FollowUp, Submitted};
use crate::app::{Effect, harness::Harness};

fn job(id: JobId, state: JobState, batch: Option<u64>) -> JobSnapshot {
    JobSnapshot {
        id,
        kind: JobKind::Copy {
            sources: vec![VfsPath::local(format!("/src/{id}.txt"))],
            target: VfsPath::local("/dst"),
        },
        options: JobOptions::default(),
        state,
        progress: Progress::default(),
        question: None,
        skipped: 0,
        failures: Vec::new(),
        report: None,
        resumable: false,
        batch,
    }
}

/// The `panel.refresh` commands among `effects`.
fn refreshes(effects: &[Effect]) -> usize {
    effects
        .iter()
        .map(|effect| match effect {
            Effect::Dispatch(_, commands) => commands
                .iter()
                .filter(|command| matches!(command, Command::Refresh { .. }))
                .count(),
            _ => 0,
        })
        .sum()
}

fn finished() -> JobChange {
    JobChange::Finished {
        state: JobState::Completed,
    }
}

/// RC-080-45: the jobs of one batch read both panels again once, when the last one is over -
/// not twice per job.
#[tokio::test]
async fn the_panels_are_read_again_once_per_batch() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    for id in [901, 902, 903] {
        let snapshot = job(id, JobState::Running, Some(7));
        let jobs = &mut harness.app.file_ops.jobs;
        let _ = jobs.adopt(snapshot, PanelId::Left, FollowUp::Refresh, id == 901);
    }
    assert_eq!(refreshes(&harness.app.on_job(901, finished())), 0);
    assert_eq!(refreshes(&harness.app.on_job(903, finished())), 0);
    assert_eq!(
        refreshes(&harness.app.on_job(902, finished())),
        2,
        "both panels, once"
    );
    let snapshot = job(904, JobState::Running, None);
    let jobs = &mut harness.app.file_ops.jobs;
    let _ = jobs.adopt(snapshot, PanelId::Left, FollowUp::Refresh, true);
    assert_eq!(
        refreshes(&harness.app.on_job(904, finished())),
        2,
        "a job of no batch is its own"
    );
}

/// A batch whose jobs were all over before the window adopted them reads the panels again
/// once; one job of it still running holds the reading back until it ends.
#[tokio::test]
async fn a_batch_over_at_once_reads_the_panels_again_once() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let submitted = |snapshots: Vec<JobSnapshot>| Submitted {
        panel: PanelId::Left,
        follow: FollowUp::Refresh,
        background: false,
        result: Ok(snapshots),
        refused: None,
    };
    let over: Vec<JobSnapshot> = (1..=50)
        .map(|id| job(1000 + id, JobState::Completed, Some(8)))
        .collect();
    assert_eq!(refreshes(&harness.app.submitted(submitted(over))), 2);
    let mixed = vec![
        job(2001, JobState::Completed, Some(9)),
        job(2002, JobState::Running, Some(9)),
    ];
    assert_eq!(refreshes(&harness.app.submitted(submitted(mixed))), 0);
    let jobs = &harness.app.file_ops.jobs;
    assert_eq!(jobs.foreground().map(|job| job.id()), Some(2002));
    assert_eq!(refreshes(&harness.app.on_job(2002, finished())), 2);
}
