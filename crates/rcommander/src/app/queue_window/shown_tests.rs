use rc_engine::{DEFAULT_LIST, JobId, JobState, QueueEntry};

use super::{Shown, shown};
use crate::app::queue_window::selection::tests::{at_once, entry};

/// As `Queue::entries` orders them: two jobs run at once, one running and one waiting of the
/// default list, one waiting in list 7, then a finished job of the default list and a finished
/// one run at once.
fn queue() -> Vec<QueueEntry> {
    let mut named = entry(5, JobState::Queued, Some(1));
    named.list = 7;
    vec![
        at_once(1, JobState::Running),
        at_once(2, JobState::Queued),
        entry(3, JobState::Running, None),
        entry(4, JobState::Queued, Some(0)),
        named,
        entry(6, JobState::Completed, None),
        at_once(7, JobState::Cancelled),
    ]
}

fn ids(entries: &[QueueEntry], scope: Shown) -> Vec<JobId> {
    shown(entries, scope).map(|entry| entry.job.id).collect()
}

#[test]
fn the_default_list_shows_the_jobs_run_at_once_first_and_their_finished_ones_with_the_finished() {
    let list = Shown::List {
        list: DEFAULT_LIST,
        at_once: true,
    };
    assert_eq!(ids(&queue(), list), [1, 2, 3, 4, 6, 7]);
}

#[test]
fn with_the_setting_off_the_window_shows_the_list_alone() {
    let list = Shown::List {
        list: DEFAULT_LIST,
        at_once: false,
    };
    assert_eq!(ids(&queue(), list), [3, 4, 6]);
}

#[test]
fn a_named_list_shows_its_own_jobs_alone() {
    let list = Shown::List {
        list: 7,
        at_once: true,
    };
    assert_eq!(ids(&queue(), list), [5]);
}

#[test]
fn the_bar_shows_every_unfinished_job_in_the_window_s_order() {
    assert_eq!(ids(&queue(), Shown::Bar), [1, 2, 3, 4, 5]);
}
