use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use tempfile::TempDir;

use crate::search::{
    DatePick, DuplicateGroup, DuplicateQuery, DuplicateRule, DuplicateSelection, SearchStats,
    default_keep,
};
use crate::test_support;
use crate::{Command, Engine, FlatSource, Outcome, PanelId, PanelState};

/// Three groups of copies by contents: `one*` (3), `two*` (2), `three*` (2), and a loner. The
/// copies of `one` are 100, 200 and 300 seconds after the epoch, in folders `a`, `b` and `b/c`;
/// every other file is 1000 seconds after it.
fn tree() -> TempDir {
    let root = test_support::tree(&[
        ("a/one.txt", "1"),
        ("b/one.txt", "1"),
        ("b/c/one.txt", "1"),
        ("two.txt", "22"),
        ("a/two.txt", "22"),
        ("three.txt", "333"),
        ("b/three.txt", "333"),
        ("alone.txt", "4444"),
    ]);
    let times = [
        ("a/one.txt", 100),
        ("b/one.txt", 200),
        ("b/c/one.txt", 300),
        ("two.txt", 1000),
        ("a/two.txt", 1000),
        ("three.txt", 1000),
        ("b/three.txt", 1000),
        ("alone.txt", 1000),
    ];
    for (name, seconds) in times {
        let file = fs::File::options()
            .write(true)
            .open(root.path().join(name))
            .expect("open");
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .expect("set the time");
    }
    root
}

const BY_CONTENT: DuplicateQuery = DuplicateQuery {
    name: false,
    size: false,
    content: true,
};

async fn page(
    engine: &Engine,
    limit: Option<usize>,
    cursor: Option<String>,
) -> (Vec<DuplicateGroup>, Option<String>, SearchStats) {
    let outcome = engine
        .dispatch_mcp(Command::FindDuplicates(crate::DuplicatesRequest {
            panel: None,
            paths: Vec::new(),
            query: crate::SearchQuery::default(),
            duplicates: BY_CONTENT,
            limit,
            cursor,
        }))
        .await
        .expect("duplicates");
    match outcome {
        Outcome::DuplicateGroups {
            groups,
            next_cursor,
            stats,
        } => (groups, next_cursor, stats),
        other => panic!("duplicate groups, got {other:?}"),
    }
}

/// Each group as its size and its files' paths.
fn shape(groups: &[DuplicateGroup]) -> Vec<(u64, Vec<std::path::PathBuf>)> {
    groups
        .iter()
        .map(|group| {
            let paths = group
                .files
                .iter()
                .map(|file| file.path.path.clone())
                .collect();
            (group.size, paths)
        })
        .collect()
}

#[tokio::test]
async fn pages_of_groups_add_up_to_the_whole_result() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let (whole, cursor, stats) = page(&engine, None, None).await;
    assert_eq!(cursor, None);
    let sizes: Vec<u64> = whole.iter().map(|group| group.size).collect();
    assert_eq!(sizes, [1, 2, 3], "sorted by size");
    assert_eq!(stats.hits, 7);
    let mut paged = Vec::new();
    let mut cursor = None;
    loop {
        let (groups, next, _) = page(&engine, Some(1), cursor).await;
        assert_eq!(groups.len(), 1);
        paged.extend(groups);
        match next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    // Reading the files moves their access times, so the runs are held by sizes and paths.
    assert_eq!(shape(&paged), shape(&whole));
}

#[tokio::test]
async fn a_cursor_of_another_search_is_refused() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let (_, cursor, _) = page(&engine, Some(1), None).await;
    let error = engine
        .dispatch_mcp(Command::FindDuplicates(crate::DuplicatesRequest {
            panel: None,
            paths: Vec::new(),
            query: crate::SearchQuery::default(),
            duplicates: DuplicateQuery {
                name: true,
                ..BY_CONTENT
            },
            limit: None,
            cursor,
        }))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "invalid_search");
}

async fn settled(engine: &Engine) -> PanelState {
    for _ in 0..500 {
        let state = engine.panel(PanelId::Left);
        if state.flat.as_ref().is_some_and(|flat| !flat.loading) {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the flat view did not finish");
}

fn rel(parts: &[&str]) -> String {
    parts
        .iter()
        .collect::<std::path::PathBuf>()
        .display()
        .to_string()
}

/// The result fed to the panel with its groups, as the dialog feeds it.
async fn fed(engine: &Engine) -> PanelState {
    let (groups, _, _) = page(engine, None, None).await;
    let (mut paths, mut numbers) = (Vec::new(), Vec::new());
    for (number, group) in groups.iter().enumerate() {
        for file in &group.files {
            paths.push(file.path.path.display().to_string());
            numbers.push(u32::try_from(number).expect("few groups"));
        }
    }
    engine
        .dispatch(Command::FeedPaths {
            panel: None,
            root: None,
            paths,
            groups: numbers,
        })
        .await
        .expect("feed");
    settled(engine).await
}

#[tokio::test]
async fn all_older_copies_selected_leaving_the_newest() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let state = fed(&engine).await;
    assert_eq!(
        state.flat.as_ref().map(|flat| flat.source),
        Some(FlatSource::Duplicates)
    );
    assert_eq!(state.entries.len(), 7);
    let outcome = engine
        .dispatch(Command::SelectDuplicates {
            panel: None,
            spec: DuplicateSelection {
                rule: DuplicateRule::Date {
                    pick: DatePick::AllOlder,
                },
                leave_one: true,
                keep: default_keep(),
            },
        })
        .await
        .expect("select");
    let Outcome::Panel(snapshot) = outcome else {
        panic!("the panel's state");
    };
    let mut selected: Vec<String> = snapshot.selection.iter().cloned().collect();
    selected.sort();
    let mut expected = vec![rel(&["a", "one.txt"]), rel(&["b", "one.txt"])];
    expected.sort();
    assert_eq!(
        selected, expected,
        "the newest copy stays; the other groups have equal times, so no copy is older"
    );
}

#[tokio::test]
async fn selecting_duplicates_needs_a_duplicate_result() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let error = engine
        .dispatch(Command::SelectDuplicates {
            panel: None,
            spec: DuplicateSelection {
                rule: DuplicateRule::Mask {
                    mask: "*".to_owned(),
                    remove: false,
                },
                leave_one: false,
                keep: default_keep(),
            },
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "invalid_search");
}

#[tokio::test]
async fn groups_must_match_the_paths() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let error = engine
        .dispatch(Command::FeedPaths {
            panel: None,
            root: None,
            paths: vec!["two.txt".into(), "a/two.txt".into()],
            groups: vec![0],
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "invalid_search");
    assert!(Path::new(&rel(&["a", "two.txt"])).is_relative());
}
