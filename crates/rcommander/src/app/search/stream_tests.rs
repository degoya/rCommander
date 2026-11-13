//! The end of a search's events (RC-110-29): a search that says `Finished` ends with it, one
//! whose events just end is closed as stopped, with the counts of its last progress.

use rc_engine::{SearchEvent, SearchStats};

use super::heard;

fn stats(entries: u64) -> SearchStats {
    SearchStats {
        entries,
        ..SearchStats::default()
    }
}

#[test]
fn events_that_end_without_finished_close_the_search_as_stopped() {
    let progress = SearchEvent::Progress {
        dir: None,
        stats: stats(7),
    };
    let (event, goes_on) = heard(Some(progress.clone()), stats(0));
    assert_eq!(event, progress);
    assert_eq!(goes_on, Some(stats(7)), "the counts of the last progress");

    let (event, goes_on) = heard(None, stats(7));
    let closed = SearchEvent::Finished {
        stats: stats(7),
        cancelled: true,
    };
    assert_eq!(event, closed);
    assert_eq!(goes_on, None, "its last word");
}

#[test]
fn finished_is_the_last_word() {
    let finished = SearchEvent::Finished {
        stats: stats(9),
        cancelled: false,
    };
    let (event, goes_on) = heard(Some(finished.clone()), stats(7));
    assert_eq!(event, finished);
    assert_eq!(goes_on, None);
}
