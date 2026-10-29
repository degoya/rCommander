use rc_engine::{JobKind, JobOptions, JobSnapshot, JobState, Progress, VfsPath};

use super::{SPACING, Shown, height, shown};
use crate::{app::queue_window::figures::live, theme::ROW_HEIGHT};

/// The height of the list as one column of every row would have.
fn whole(heights: &[f32]) -> f32 {
    heights.iter().sum::<f32>() + SPACING * (heights.len().saturating_sub(1)) as f32
}

/// The height of the list as built: the space above, the rows built, the space below.
fn built(heights: &[f32], shown: &Shown) -> f32 {
    shown.above + whole(&heights[shown.rows.clone()]) + shown.below
}

#[test]
fn a_short_list_is_built_whole() {
    let heights = [ROW_HEIGHT; 30];
    let shown = shown(&heights, 0.0);
    assert_eq!(shown.rows, 0..30);
    assert_eq!((shown.above, shown.below), (0.0, 0.0));
}

#[test]
fn a_long_list_builds_the_rows_around_its_offset_and_keeps_its_height() {
    let heights = vec![ROW_HEIGHT; 100_000];
    let at = |top: f32| shown(&heights, top);
    let start = at(0.0);
    assert_eq!(start.rows.start, 0);
    assert!(start.rows.len() < 300, "{:?}", start.rows);
    assert!((built(&heights, &start) - whole(&heights)).abs() < 1.0);

    let row = ROW_HEIGHT + SPACING;
    let middle = at(50_000.0 * row);
    assert!(middle.rows.contains(&50_000) && middle.rows.contains(&50_040));
    assert!(middle.rows.start > 49_900 && middle.rows.len() < 300);
    // Each row built stands where the whole column puts it.
    let first = middle.rows.start as f32;
    assert!((middle.above - first * row).abs() < 1.0);
    assert!((built(&heights, &middle) - whole(&heights)).abs() < 1.0);

    let end = at(f32::MAX);
    assert_eq!(
        end.rows.end, 100_000,
        "the last rows, however far the offset"
    );
    assert!((built(&heights, &end) - whole(&heights)).abs() < 1.0);
}

#[test]
fn rows_of_two_lines_count_twice() {
    let mut heights = vec![ROW_HEIGHT; 1_000];
    heights[..500].fill(2.0 * ROW_HEIGHT);
    let shown = shown(&heights, 500.0 * (2.0 * ROW_HEIGHT + SPACING));
    assert!(shown.rows.contains(&500));
    assert!((shown.above - shown.rows.start as f32 * (2.0 * ROW_HEIGHT + SPACING)).abs() < 1.0);
    assert!((built(&heights, &shown) - whole(&heights)).abs() < 1.0);
}

#[test]
fn a_running_job_on_a_file_has_two_lines() {
    let mut job = JobSnapshot {
        id: 1,
        kind: JobKind::Mkdir {
            path: VfsPath::local("/new"),
        },
        options: JobOptions::default(),
        state: JobState::Running,
        progress: Progress::default(),
        question: None,
        skipped: 0,
        failures: Vec::new(),
        report: None,
        resumable: false,
        batch: None,
    };
    // Two lines exactly where the row draws its file under its path.
    let lines = |job: &JobSnapshot| (height(job) / ROW_HEIGHT, live(job).is_some());
    assert_eq!(lines(&job), (1.0, false));
    job.progress.current = Some(VfsPath::local("/a.txt"));
    assert_eq!(lines(&job), (2.0, true));
    job.state = JobState::Paused;
    assert_eq!(lines(&job), (1.0, false));
}
