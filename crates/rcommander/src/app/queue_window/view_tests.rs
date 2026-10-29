use rc_engine::{DeleteMode, JobKind, JobOptions, JobSnapshot, JobState, Progress, VfsPath};

use super::{kind_label, live, route, running_together, single};

#[test]
fn a_route_names_the_first_source_how_many_more_and_the_target() {
    let copy = JobKind::Copy {
        sources: vec![VfsPath::local("/a/x.txt"), VfsPath::local("/a/y.txt")],
        target: VfsPath::local("/out"),
    };
    let shown = route(&copy);
    assert!(shown.starts_with("/a/x.txt and 1 more"), "{shown}");
    assert!(shown.ends_with("/out"), "{shown}");
    let delete = JobKind::Delete {
        sources: vec![VfsPath::local("/a/x.txt")],
        mode: DeleteMode::Trash,
    };
    assert_eq!(route(&delete), "/a/x.txt");
    let mkdir = JobKind::Mkdir {
        path: VfsPath::local("/a/new"),
    };
    assert_eq!(route(&mkdir), "/a/new");
    assert_eq!(kind_label(&copy), "Copy");
}

fn running(progress: Progress) -> JobSnapshot {
    JobSnapshot {
        id: 1,
        kind: JobKind::Copy {
            sources: vec![VfsPath::local("/src/films")],
            target: VfsPath::local("/out"),
        },
        options: JobOptions::default(),
        state: JobState::Running,
        progress,
        question: None,
        skipped: 0,
        failures: Vec::new(),
        report: None,
        resumable: false,
        batch: None,
    }
}

/// RC-010-14: a running job shows the file it is on, its bytes, its speed and the time left as
/// the queue reports them; before the speed is measured only the bytes, and a job not on a file
/// shows nothing.
#[test]
fn a_running_job_shows_its_file_bytes_speed_and_time_left() {
    let mut progress = Progress {
        files_total: 3,
        files_done: 1,
        bytes_total: 4 * 1024 * 1024,
        bytes_done: 1024 * 1024,
        current: Some(VfsPath::local("/src/films/b.mkv")),
        bytes_per_second: 0,
        eta_seconds: None,
        ..Progress::default()
    };
    let (file, shown) = live(&running(progress.clone())).expect("on a file");
    assert!(file.ends_with("b.mkv"), "{file}");
    assert!(!shown.contains("/s"), "no speed yet: {shown}");
    assert!(shown.contains(" of "), "{shown}");

    progress.bytes_per_second = 2 * 1024 * 1024;
    progress.eta_seconds = Some(90);
    let (_, shown) = live(&running(progress.clone())).expect("on a file");
    assert!(shown.contains("/s"), "{shown}");
    assert!(shown.contains("left"), "{shown}");

    let mut paused = running(progress.clone());
    paused.state = JobState::Paused;
    assert_eq!(live(&paused), None);
    progress.current = None;
    assert_eq!(live(&running(progress)), None);
}

/// RC-010-26: the speed together shows once more than one job runs, as the sum of their speeds;
/// a paused job's speed is not in it.
#[test]
fn the_speed_together_is_the_running_jobs_sum() {
    let entry = |state: JobState, speed: u64| {
        let mut job = running(Progress {
            bytes_per_second: speed,
            ..Progress::default()
        });
        job.state = state;
        rc_engine::QueueEntry {
            job,
            lane: None,
            place: None,
            list: rc_engine::DEFAULT_LIST,
            at_once: true,
        }
    };
    let one = [entry(JobState::Running, 80), entry(JobState::Paused, 50)];
    assert_eq!(running_together(&one), None);
    let two = [
        entry(JobState::Running, 80),
        entry(JobState::Running, 40),
        entry(JobState::Paused, 50),
    ];
    assert_eq!(running_together(&two), Some((2, 120)));
}

/// RC-010-26: the lane column says something only when the jobs differ in it.
#[test]
fn one_lane_for_all_is_one_lane() {
    let lanes = |names: &[&str]| {
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };
    assert!(single(&lanes(&["Local", "Local"])));
    assert!(!single(&lanes(&["Local", "Runs at once"])));
    assert!(!single(&lanes(&["Local", "srv01"])));
    assert!(!single(&[]));
}
