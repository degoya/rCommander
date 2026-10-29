use std::sync::atomic::Ordering;

use rc_engine::{
    PanelId, VfsPath,
    viewer::{Direction, Match, SearchOutcome},
};
use tempfile::TempDir;

use super::{BarKind, FindOption};
use crate::app::harness::headless_engine;
use crate::{
    app::viewer::{
        keys::ListerKey,
        lister::{Lister, Request},
        load::{self, Found, Seek},
    },
    i18n,
};

async fn lister_on(bytes: &[u8]) -> (TempDir, Lister) {
    let root = tempfile::tempdir().expect("tempdir");
    let path = VfsPath::local(root.path().join("f"));
    std::fs::write(&path.path, bytes).expect("write");
    let engine = headless_engine(root.path()).await;
    let opened = load::open(engine, path.clone(), None).await.expect("opens");
    let mut lister = Lister::new(1, PanelId::Left, path);
    lister.opened(opened);
    (root, lister)
}

fn search_from(requests: &[Request]) -> Option<(u64, Direction)> {
    match requests {
        [
            Request::Search {
                from, direction, ..
            },
        ] => Some((*from, *direction)),
        _ => None,
    }
}

#[tokio::test]
async fn escape_closes_the_bar_first_then_the_viewer() {
    let (_root, mut lister) = lister_on(b"text\n").await;
    lister.key(ListerKey::Search);
    assert_eq!(lister.find.bar, Some(BarKind::Search));
    assert!(!lister.escape());
    assert_eq!(lister.find.bar, None);
    assert!(lister.escape());
}

#[tokio::test]
async fn an_empty_or_bad_search_says_why_and_reads_nothing() {
    let (_root, mut lister) = lister_on(b"text\n").await;
    lister.key(ListerKey::Search);
    assert!(lister.submit().is_empty());
    assert_eq!(lister.note, Some(i18n::error_text("search_empty")));
    lister.toggle(FindOption::Hex);
    lister.bar_input("4G".into());
    assert!(lister.submit().is_empty());
    assert_eq!(lister.note, Some(i18n::error_text("search_invalid_hex")));
}

#[tokio::test]
async fn a_match_is_shown_marked_and_f3_goes_on_from_it() {
    let (_root, mut lister) = lister_on(b"one needle two needle\n").await;
    lister.key(ListerKey::Search);
    lister.bar_input("NEEDLE".into());
    let started = lister.submit();
    assert_eq!(search_from(&started), Some((0, Direction::Forward)));
    assert_eq!(lister.find.bar, None);
    assert_eq!(lister.search_percent(), Some(0));
    assert!(lister.key(ListerKey::FindNext).is_empty(), "one at a time");
    lister.found(Found::Progress(11));
    assert_eq!(lister.search_percent(), Some(50));
    let found = Match { offset: 4, len: 6 };
    let shown = lister.found(Found::Done(Ok(SearchOutcome::Found(found))));
    assert!(matches!(
        shown.as_slice(),
        [Request::Page {
            seek: Seek::At(4),
            mark: Some(Match { offset: 4, len: 6 }),
            ..
        }]
    ));
    assert_eq!(lister.search_percent(), None);
    let next = lister.key(ListerKey::FindNext);
    assert_eq!(search_from(&next), Some((5, Direction::Forward)));
    lister.found(Found::Done(Ok(SearchOutcome::NotFound)));
    assert_eq!(
        lister.note.as_deref(),
        Some(i18n::lookup("viewer.not_found"))
    );
    let back = lister.key(ListerKey::FindPrevious);
    assert_eq!(search_from(&back), Some((4, Direction::Backward)));
}

#[tokio::test]
async fn escape_stops_a_running_search_and_keeps_the_viewer() {
    let (_root, mut lister) = lister_on(b"text\n").await;
    lister.key(ListerKey::Search);
    lister.bar_input("x".into());
    let started = lister.submit();
    let Some(Request::Search { cancel, .. }) = started.first() else {
        panic!("a search");
    };
    assert!(!lister.escape());
    assert!(cancel.load(Ordering::Relaxed));
    lister.found(Found::Done(Ok(SearchOutcome::Cancelled)));
    assert!(lister.escape());
}

/// The audit's D8: Enter in the bar while a search ran closed the bar and searched nothing. The
/// bar stays open with its text while the status line shows the running search; once it ended,
/// Enter searches.
#[tokio::test]
async fn enter_while_a_search_runs_keeps_the_bar_and_says_so() {
    let (_root, mut lister) = lister_on(b"one needle two\n").await;
    lister.key(ListerKey::Search);
    lister.bar_input("one".into());
    assert!(search_from(&lister.submit()).is_some());
    lister.key(ListerKey::Search);
    lister.bar_input("two".into());
    assert!(lister.submit().is_empty());
    assert_eq!(lister.find.bar, Some(BarKind::Search));
    assert_eq!(lister.find.text, "two");
    assert!(
        lister.search_percent().is_some(),
        "the status line shows the search"
    );
    lister.found(Found::Done(Ok(SearchOutcome::NotFound)));
    assert!(search_from(&lister.submit()).is_some(), "now it searches");
}

/// The audit's D8: closing the Lister let its search run on through the file.
#[tokio::test]
async fn closing_the_lister_stops_its_search() {
    let (_root, mut lister) = lister_on(b"text\n").await;
    lister.key(ListerKey::Search);
    lister.bar_input("x".into());
    let started = lister.submit();
    let Some(Request::Search { cancel, .. }) = started.first() else {
        panic!("a search");
    };
    let cancel = std::sync::Arc::clone(cancel);
    drop(lister);
    assert!(cancel.load(Ordering::Relaxed));
}

#[tokio::test]
async fn go_to_offset_takes_lister_spellings() {
    let (_root, mut lister) = lister_on(&[0u8; 64]).await;
    lister.key(ListerKey::GoTo);
    lister.bar_input("zz".into());
    assert!(lister.submit().is_empty());
    assert_eq!(lister.find.bar, Some(BarKind::GoTo));
    lister.bar_input("$20".into());
    let requests = lister.submit();
    assert!(matches!(
        requests.as_slice(),
        [Request::Page {
            seek: Seek::At(32),
            ..
        }]
    ));
    assert_eq!(lister.find.bar, None);
}
