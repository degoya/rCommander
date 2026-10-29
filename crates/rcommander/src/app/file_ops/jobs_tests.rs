use std::time::{Duration, Instant};

use rc_engine::{EntryInfo, JobOptions, Progress, VfsPath};

use super::*;

fn snapshot(id: JobId, state: JobState) -> JobSnapshot {
    JobSnapshot {
        id,
        kind: JobKind::Copy {
            sources: vec![VfsPath::local("/src/a.txt")],
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
        batch: None,
    }
}

fn conflict() -> Question {
    let side = |path: &str| EntryInfo {
        path: VfsPath::local(path),
        is_dir: false,
        size: 1,
        modified_unix: None,
    };
    Question::Conflict {
        source: side("/src/report.final.pdf"),
        target: side("/dst/report.final.pdf"),
        resumable: false,
    }
}

#[test]
fn a_job_follows_its_events_until_it_ends() {
    let mut jobs = Jobs::default();
    let adopted = jobs.adopt(
        snapshot(1, JobState::Queued),
        PanelId::Left,
        FollowUp::Refresh,
        true,
    );
    assert!(adopted.is_none());
    assert_eq!(jobs.foreground().map(Tracked::id), Some(1));
    jobs.apply(1, JobChange::Started);
    jobs.apply(
        1,
        JobChange::Progress {
            progress: Progress {
                files_total: 4,
                bytes_total: 100,
                bytes_done: 25,
                ..Progress::default()
            },
        },
    );
    let job = jobs.get(1).expect("followed");
    assert_eq!(job.snapshot.state, JobState::Running);
    assert!((job.share() - 0.25).abs() < f32::EPSILON);
    jobs.apply(
        1,
        JobChange::Question {
            question: conflict(),
        },
    );
    assert_eq!(jobs.asking().map(Tracked::id), Some(1));
    assert_eq!(
        jobs.get(1).map(|job| job.rename_to.as_str()),
        Some("report.final (2).pdf")
    );
    jobs.apply(1, JobChange::Answered);
    assert!(jobs.asking().is_none());
    jobs.apply(1, JobChange::Paused);
    assert!(jobs.get(1).is_some_and(Tracked::paused));
    let ended = jobs.apply(
        1,
        JobChange::Finished {
            state: JobState::Completed,
        },
    );
    assert_eq!(
        ended.map(|job| job.snapshot.state),
        Some(JobState::Completed)
    );
    assert!(jobs.is_empty());
}

#[test]
fn other_callers_jobs_are_not_followed() {
    let mut jobs = Jobs::default();
    assert!(
        jobs.apply(
            9,
            JobChange::Finished {
                state: JobState::Completed
            }
        )
        .is_none()
    );
    assert!(jobs.is_empty());
}

#[test]
fn a_job_finished_before_it_was_adopted_comes_back_at_once() {
    let mut jobs = Jobs::default();
    let done = jobs.adopt(
        snapshot(2, JobState::Completed),
        PanelId::Right,
        FollowUp::Refresh,
        false,
    );
    assert!(done.is_some());
    assert!(jobs.is_empty());
}

#[test]
fn the_queue_s_snapshot_brings_a_missed_question() {
    let mut jobs = Jobs::default();
    jobs.adopt(
        snapshot(3, JobState::Running),
        PanelId::Left,
        FollowUp::Refresh,
        true,
    );
    let mut waiting = snapshot(3, JobState::Waiting);
    waiting.question = Some(conflict());
    assert!(jobs.refresh(waiting).is_none());
    assert_eq!(jobs.asking().map(Tracked::id), Some(3));
}

#[test]
fn one_job_at_a_time_is_in_the_progress_window() {
    let mut jobs = Jobs::default();
    for id in [1, 2] {
        jobs.adopt(
            snapshot(id, JobState::Running),
            PanelId::Left,
            FollowUp::Refresh,
            false,
        );
    }
    assert!(jobs.foreground().is_none());
    jobs.set_foreground(2, true);
    jobs.set_foreground(1, true);
    assert_eq!(jobs.foreground().map(Tracked::id), Some(1));
    assert_eq!(jobs.background().map(Tracked::id).collect::<Vec<_>>(), [2]);
    jobs.set_foreground(1, false);
    assert_eq!(jobs.background().count(), 2);
}

fn moved(files_total: u64, bytes: (u64, u64), file: (u64, u64)) -> JobChange {
    JobChange::Progress {
        progress: Progress {
            files_total,
            bytes_done: bytes.0,
            bytes_total: bytes.1,
            file_bytes_done: file.0,
            file_bytes_total: file.1,
            ..Progress::default()
        },
    }
}

/// Total Commander's two bars (RC-010-07): the total covers both jobs of an F5 of two sources,
/// the first one's bytes kept once it is over, while the file bar starts again.
#[test]
fn the_total_covers_every_job_of_the_batch() {
    let mut jobs = Jobs::default();
    for (id, foreground) in [(1, true), (2, false)] {
        let mut queued = snapshot(id, JobState::Queued);
        queued.batch = Some(7);
        jobs.adopt(queued, PanelId::Left, FollowUp::Refresh, foreground);
    }
    jobs.apply(1, JobChange::Started);
    jobs.apply(1, moved(1, (100, 100), (100, 100)));
    let first = jobs.totals(1);
    assert_eq!((first.bytes_done, first.bytes_total), (100, 100));
    assert!(
        (first.share() - 0.5).abs() < f32::EPSILON,
        "the second job, not counted yet, weighs as the first: {first:?}"
    );
    jobs.apply(
        1,
        JobChange::Finished {
            state: JobState::Completed,
        },
    );
    assert_eq!(jobs.foreground().map(Tracked::id), Some(2));
    jobs.apply(2, JobChange::Started);
    jobs.apply(2, moved(1, (10, 100), (10, 100)));
    let both = jobs.totals(2);
    assert_eq!((both.bytes_done, both.bytes_total), (110, 200), "{both:?}");
    assert!(both.share() > 0.5, "{both:?}");
    let second = jobs.get(2).expect("followed");
    assert!((second.file_share() - 0.1).abs() < f32::EPSILON);
    jobs.apply(
        2,
        JobChange::Finished {
            state: JobState::Completed,
        },
    );
    assert!(jobs.is_empty());
    assert!(jobs.finished.is_empty(), "the batch is forgotten");
}

#[test]
fn a_job_of_no_batch_is_its_own_total() {
    let mut jobs = Jobs::default();
    jobs.adopt(
        snapshot(4, JobState::Running),
        PanelId::Left,
        FollowUp::Refresh,
        true,
    );
    jobs.apply(4, moved(3, (25, 100), (5, 20)));
    let job = jobs.get(4).expect("followed");
    let totals = jobs.totals(4);
    assert_eq!((totals.bytes_done, totals.bytes_total), (25, 100));
    assert!((totals.share() - job.share()).abs() < f32::EPSILON);
    assert!((job.file_share() - 0.25).abs() < f32::EPSILON);
    assert_eq!(
        jobs.totals(99),
        Totals::default(),
        "an unknown job has none"
    );
}

fn at(files: (u64, u64), bytes: (u64, u64)) -> JobChange {
    JobChange::Progress {
        progress: Progress {
            files_done: files.0,
            files_total: files.1,
            bytes_done: bytes.0,
            bytes_total: bytes.1,
            ..Progress::default()
        },
    }
}

/// The owner's 1000 files (RC-010-07): speed and time left cover the whole operation, measured
/// over the batch, and go on when its next job starts.
#[test]
fn speed_and_time_left_cover_the_whole_batch() {
    let start = Instant::now();
    let second = |tenths: u64| start + Duration::from_millis(tenths * 100);
    let mut jobs = Jobs::default();
    for (id, foreground) in [(1, true), (2, false)] {
        let mut queued = snapshot(id, JobState::Queued);
        queued.batch = Some(7);
        jobs.adopt(queued, PanelId::Left, FollowUp::Refresh, foreground);
    }
    jobs.apply_at(1, JobChange::Started, second(0));
    jobs.apply_at(1, at((0, 1), (0, 200)), second(0));
    jobs.apply_at(1, at((0, 1), (100, 200)), second(10));
    let early = jobs.timing(1, second(10));
    assert_eq!(early.bytes_per_second, 100);
    // 100 of the first job's 200 bytes left, the second guessed at 200: 300 bytes at 100/s.
    assert_eq!(early.eta_seconds, Some(3), "{early:?}");
    assert!(early.estimate, "the second job is not counted yet");
    assert_eq!(early.elapsed_seconds, Some(1));
    jobs.apply_at(1, at((1, 1), (200, 200)), second(20));
    jobs.apply_at(
        1,
        JobChange::Finished {
            state: JobState::Completed,
        },
        second(20),
    );
    // The next job starts after a gap, which counts as time.
    jobs.apply_at(2, JobChange::Started, second(25));
    jobs.apply_at(2, at((0, 1), (0, 200)), second(25));
    let next = jobs.timing(2, second(30));
    // 200 bytes and one file in 2.5 s: 80 bytes and 0.4 files a second, not unknown again.
    assert_eq!(next.bytes_per_second, 80, "{next:?}");
    // The second job's 200 bytes at 80/s, its one file at 0.4/s: 2.5 s either way.
    assert_eq!(next.eta_seconds, Some(2), "{next:?}");
    assert!(!next.estimate, "every job counted");
    assert_eq!(next.elapsed_seconds, Some(3), "since the first job started");
    jobs.apply_at(
        2,
        JobChange::Finished {
            state: JobState::Completed,
        },
        second(40),
    );
    assert!(jobs.paces.is_empty(), "the operation is forgotten");
}

#[test]
fn a_job_of_no_batch_keeps_its_own_speed() {
    let mut jobs = Jobs::default();
    jobs.adopt(
        snapshot(5, JobState::Running),
        PanelId::Left,
        FollowUp::Refresh,
        true,
    );
    jobs.apply(
        5,
        JobChange::Progress {
            progress: Progress {
                files_total: 1,
                bytes_total: 100,
                bytes_per_second: 42,
                eta_seconds: Some(9),
                ..Progress::default()
            },
        },
    );
    let timing = jobs.timing(5, Instant::now());
    assert_eq!((timing.bytes_per_second, timing.eta_seconds), (42, Some(9)));
    assert!(!timing.estimate);
    assert!(
        timing.elapsed_seconds.is_some(),
        "timed since it was adopted"
    );
}

/// RC-080-45: the jobs of one submission are sampled once per operation, through its first job
/// that is not queued; a queued batch is not sampled yet.
#[test]
fn the_jobs_adopted_together_are_measured_once_per_operation() {
    let mut jobs = Jobs::default();
    let in_batch = |id, state, batch| JobSnapshot {
        batch,
        ..snapshot(id, state)
    };
    let ids = [1, 2, 3, 4];
    let states = [
        (JobState::Queued, Some(5)),
        (JobState::Running, Some(5)),
        (JobState::Running, Some(5)),
        (JobState::Queued, Some(6)),
    ];
    for (id, (state, batch)) in ids.into_iter().zip(states) {
        let adopted = jobs.adopt_unmeasured(
            in_batch(id, state, batch),
            PanelId::Left,
            FollowUp::Refresh,
            false,
        );
        assert!(adopted.is_none());
    }
    assert!(jobs.paces.is_empty(), "not measured while adopted");
    jobs.measure_adopted(&ids);
    assert_eq!(jobs.paces.len(), 1, "batch 5 once, queued batch 6 not");
    assert!(jobs.timing(1, Instant::now()).elapsed_seconds.is_some());
    assert!(jobs.timing(4, Instant::now()).elapsed_seconds.is_none());
    assert!(jobs.batch_followed(Some(5)) && jobs.batch_followed(Some(6)));
    assert!(!jobs.batch_followed(Some(7)) && !jobs.batch_followed(None));
}
