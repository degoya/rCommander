use rc_engine::{JobId, JobKind, JobOptions, JobSnapshot, JobState, Progress, QueueEntry, VfsPath};

use super::started;
use crate::app::queue_window::QueueWindow;

/// A copy, job `id`, in `state`, at `place` among the waiting ones (`None`: it waits for no lane).
pub(crate) fn entry(id: JobId, state: JobState, place: Option<usize>) -> QueueEntry {
    QueueEntry {
        job: JobSnapshot {
            id,
            kind: JobKind::Copy {
                sources: vec![VfsPath::local("/src/a.txt")],
                target: VfsPath::local("/out"),
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
        },
        lane: None,
        place,
        list: 0,
        at_once: false,
    }
}

/// [`entry`], run at once (RC-010-25): in no list, waiting for no place.
pub(crate) fn at_once(id: JobId, state: JobState) -> QueueEntry {
    QueueEntry {
        list: rc_engine::AT_ONCE,
        at_once: true,
        ..entry(id, state, None)
    }
}

/// A window over five waiting jobs, 1 to 5, the cursor on the first.
pub(crate) fn five() -> QueueWindow {
    QueueWindow {
        entries: (1..=5)
            .map(|id| entry(id, JobState::Queued, Some(id as usize - 1)))
            .collect(),
        selected: Some(1),
        ..QueueWindow::default()
    }
}

fn marked(window: &QueueWindow) -> Vec<JobId> {
    window.marked.iter().copied().collect()
}

#[test]
fn a_click_selects_one_ctrl_adds_and_removes_shift_takes_the_range() {
    let mut window = five();
    assert_eq!(window.chosen(), [1], "nothing marked: the cursor's job");
    window.click(3, false, false);
    assert_eq!((window.chosen(), window.selected), (vec![3], Some(3)));
    assert!(window.marked.is_empty());

    window.click(5, true, false);
    assert_eq!(window.chosen(), [3, 5], "Ctrl adds to the one selected");
    window.click(3, true, false);
    assert_eq!(window.chosen(), [5], "and takes away");
    window.click(2, false, true);
    assert_eq!(
        window.chosen(),
        [2, 3],
        "from the job clicked last, upwards too"
    );
    window.click(5, false, true);
    assert_eq!(window.chosen(), [3, 4, 5], "the anchor stays");
    window.click(4, false, false);
    assert_eq!(window.chosen(), [4], "a plain click starts over");
}

#[test]
fn space_marks_and_moves_on_shift_arrows_extend_ctrl_a_takes_all() {
    let mut window = five();
    window.toggle_and_step();
    window.toggle_and_step();
    window.step(1);
    window.anchor = window.selected;
    assert_eq!((marked(&window), window.selected), (vec![1, 2], Some(4)));
    assert_eq!(window.chosen(), [1, 2], "the marks, not the cursor");
    window.toggle_and_step();
    assert_eq!(marked(&window), [1, 2, 4]);

    window.click(2, false, false);
    window.extend(1);
    window.extend(1);
    assert_eq!(window.chosen(), [2, 3, 4]);
    window.extend(-1);
    assert_eq!(window.chosen(), [2, 3], "back towards the anchor");
    window.select_all();
    assert_eq!(window.chosen(), [1, 2, 3, 4, 5]);
}

#[test]
fn marks_follow_the_jobs_still_listed() {
    let mut window = five();
    window.select_all();
    window.anchor = Some(5);
    window.entries.retain(|entry| entry.job.id % 2 == 1);
    window.keep_listed();
    assert_eq!(marked(&window), [1, 3, 5]);
    window.entries.truncate(2);
    window.keep_listed();
    assert_eq!((marked(&window), window.anchor), (vec![1, 3], None));
    window.unmark();
    assert_eq!(window.chosen(), [1]);
}

#[test]
fn a_job_counts_as_started_once_it_ran_or_did_part_of_its_work() {
    assert!(!started(&entry(1, JobState::Queued, Some(0))));
    assert!(started(&entry(1, JobState::Running, None)));
    assert!(started(&entry(1, JobState::Waiting, None)), "it asks");
    assert!(!started(&entry(1, JobState::Completed, None)));
    let mut interrupted = entry(1, JobState::Paused, Some(0));
    assert!(!started(&interrupted), "paused before its turn");
    interrupted.job.resumable = true;
    assert!(started(&interrupted));
}
