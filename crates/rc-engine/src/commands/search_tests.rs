use std::fs;

use tempfile::TempDir;

use crate::test_support;
use crate::{Command, Engine, Outcome, SearchEvent, SearchHit, SearchQuery, SearchStats};

/// `root/` with `f00.txt` ... `f24.txt` and `sub/g0.txt` ... `sub/g4.txt`.
fn tree() -> TempDir {
    let names: Vec<String> = (0..25)
        .map(|n| format!("f{n:02}.txt"))
        .chain((0..5).map(|n| format!("sub/g{n}.txt")))
        .collect();
    let entries: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), "x")).collect();
    test_support::tree(&entries)
}

fn query(name: &str) -> SearchQuery {
    SearchQuery {
        name: name.to_owned(),
        ..SearchQuery::default()
    }
}

async fn page(
    engine: &Engine,
    query: &SearchQuery,
    limit: Option<usize>,
    cursor: Option<String>,
) -> (Vec<SearchHit>, Option<String>, SearchStats) {
    let outcome = engine
        .dispatch_mcp(Command::SearchFiles(crate::SearchRequest {
            panel: None,
            paths: Vec::new(),
            query: query.clone(),
            limit,
            cursor,
        }))
        .await
        .expect("search");
    match outcome {
        Outcome::SearchResults {
            hits,
            next_cursor,
            stats,
        } => (hits, next_cursor, stats),
        other => panic!("search results, got {other:?}"),
    }
}

fn names(hits: &[SearchHit]) -> Vec<String> {
    hits.iter().map(|hit| hit.entry.name.clone()).collect()
}

#[tokio::test]
async fn pages_add_up_to_the_whole_search() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let all = query("*.txt");
    let (whole, cursor, stats) = page(&engine, &all, None, None).await;
    assert_eq!(whole.len(), 30);
    assert_eq!(cursor, None);
    assert_eq!(stats.hits, 30);
    assert_eq!(stats.dirs, 2);

    let mut paged = Vec::new();
    let mut cursor = None;
    let mut pages = 0;
    loop {
        let (hits, next, _) = page(&engine, &all, Some(12), cursor).await;
        pages += 1;
        paged.extend(hits);
        match next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(names(&paged), names(&whole));
}

#[tokio::test]
async fn a_full_page_stops_the_walk() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let (hits, next, stats) = page(&engine, &query("*.txt"), Some(5), None).await;
    assert_eq!(
        names(&hits),
        ["f00.txt", "f01.txt", "f02.txt", "f03.txt", "f04.txt"]
    );
    assert!(next.is_some());
    assert_eq!(stats.dirs, 1, "the walk stopped before the subfolder");
}

/// Page k+1 goes on with the walk of page k: a file of an earlier page removed meanwhile moves
/// no hit. A cursor whose walk is not kept any more - here one without the walk's id, as from
/// an older run - walks again from the start and skips the pages before (RC-080-50).
#[tokio::test]
async fn the_next_page_goes_on_with_the_walk_of_the_page_before() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let all = query("*.txt");
    let (first, cursor, _) = page(&engine, &all, Some(5), None).await;
    assert_eq!(names(&first)[0], "f00.txt");
    let cursor = cursor.expect("a next page");
    fs::remove_file(root.path().join("f00.txt")).expect("remove");
    let (second, _, _) = page(&engine, &all, Some(5), Some(cursor.clone())).await;
    assert_eq!(
        names(&second),
        ["f05.txt", "f06.txt", "f07.txt", "f08.txt", "f09.txt"]
    );
    let (again, _, _) = page(&engine, &all, Some(5), Some(cursor.clone())).await;
    assert_eq!(names(&again)[0], "f06.txt", "taken: walked again");
    let without_id = cursor.rsplit_once('-').expect("an id").0.to_owned();
    let (walked, _, _) = page(&engine, &all, Some(5), Some(without_id)).await;
    assert_eq!(names(&walked), names(&again));
}

#[tokio::test]
async fn a_cursor_of_another_search_is_refused() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let (_, cursor, _) = page(&engine, &query("*.txt"), Some(5), None).await;
    for cursor in [cursor.expect("a next page"), "garbage".to_owned()] {
        let error = engine
            .dispatch_mcp(Command::SearchFiles(crate::SearchRequest {
                panel: None,
                paths: Vec::new(),
                query: query("f*"),
                limit: Some(5),
                cursor: Some(cursor),
            }))
            .await
            .expect_err("refused");
        assert_eq!(error.code(), "invalid_search");
    }
}

#[tokio::test]
async fn start_folders_resolve_against_the_panel_and_must_be_folders() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let search = |paths: Vec<String>| {
        Command::SearchFiles(crate::SearchRequest {
            panel: None,
            paths,
            query: query("g*"),
            limit: None,
            cursor: None,
        })
    };
    let Outcome::SearchResults { hits, .. } = engine
        .dispatch(search(vec!["sub".to_owned()]))
        .await
        .expect("search")
    else {
        panic!("search results");
    };
    assert_eq!(hits.len(), 5);
    assert_eq!(hits[0].path.path, root.path().join("sub").join("g0.txt"));
    let missing = engine.dispatch(search(vec!["gone".to_owned()])).await;
    assert_eq!(missing.expect_err("missing").code(), "not_found");
    let file = engine.dispatch(search(vec!["f00.txt".to_owned()])).await;
    assert_eq!(file.expect_err("a file").code(), "not_a_directory");
}

#[tokio::test]
async fn the_stream_reports_the_same_hits_as_the_pages() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let all = query("*1*");
    let (paged, _, _) = page(&engine, &all, None, None).await;
    let mut handle = engine.start_search(None, &[], &all).await.expect("start");
    let mut streamed = Vec::new();
    loop {
        match handle.next().await.expect("events until Finished") {
            SearchEvent::Hit(hit) => streamed.push(hit),
            SearchEvent::Finished { cancelled, .. } => {
                assert!(!cancelled);
                break;
            }
            SearchEvent::Progress { .. } | SearchEvent::Skipped { .. } => {}
        }
    }
    assert_eq!(streamed, paged);
    assert_eq!(handle.next().await, None);
}

#[tokio::test]
async fn a_cancelled_stream_finishes_as_cancelled() {
    // More hits than the channel holds: the walk waits on the full channel, so it cannot
    // finish before the cancel.
    let root = tree();
    for n in 0..400 {
        fs::write(root.path().join(format!("h{n:03}")), b"").expect("write");
    }
    let engine = test_support::headless(root.path()).await;
    let mut handle = engine
        .start_search(None, &[], &SearchQuery::default())
        .await
        .expect("start");
    handle.cancel();
    let mut last = None;
    while let Some(event) = handle.next().await {
        last = Some(event);
    }
    assert!(matches!(
        last,
        Some(SearchEvent::Finished {
            cancelled: true,
            ..
        })
    ));
}

#[test]
fn the_command_reads_with_defaults() {
    let command: Command = serde_json::from_value(serde_json::json!({
        "command": "files.search",
        "params": { "query": { "name": "*.rs", "content": { "text": "fn main" } } }
    }))
    .expect("parses");
    let Command::SearchFiles(crate::SearchRequest { query, paths, .. }) = command else {
        panic!("files.search");
    };
    assert!(paths.is_empty());
    assert_eq!(query.name, "*.rs");
    assert_eq!(query.content.expect("content").text, "fn main");
}
