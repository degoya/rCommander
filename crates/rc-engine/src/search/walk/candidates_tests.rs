use std::{path::Path, sync::Arc};

use rc_vfs::{BackendId, LocalFs};

use crate::search::{ContentQuery, SearchCancel, SearchEvent, SearchQuery, walk::Walk};
use crate::test_support;

/// Everything's candidates (some outside the start folder, one gone) judged by `query`: the hits
/// below `root` with `/`, and the skipped.
async fn judged(root: &Path, candidates: &[&str], query: &SearchQuery) -> (Vec<String>, u64) {
    let paths = candidates
        .iter()
        .map(|candidate| {
            if candidate.starts_with('/') {
                Path::new(candidate).to_path_buf()
            } else {
                root.join(candidate)
            }
        })
        .collect();
    let mut walk = Walk::new(
        Arc::new(LocalFs::new()),
        BackendId::Local,
        vec![root.to_path_buf()],
        query.compile().expect("valid"),
        SearchCancel::default(),
    )
    .over_candidates(paths);
    let mut found = Vec::new();
    while let Some(event) = walk.next().await {
        match event {
            SearchEvent::Hit(hit) => {
                let below = hit.path.path.strip_prefix(root).expect("below the root");
                found.push(below.to_string_lossy().replace('\\', "/"));
            }
            SearchEvent::Finished { stats, cancelled } => {
                assert!(!cancelled);
                assert_eq!(stats.dirs, 0, "nothing is listed");
                return (found, stats.skipped);
            }
            SearchEvent::Progress { .. }
            | SearchEvent::Skipped { .. }
            | SearchEvent::Duplicates(_) => {}
        }
    }
    panic!("the walk ended without Finished");
}

fn tree() -> tempfile::TempDir {
    test_support::tree(&[
        ("a.txt", "alpha error"),
        ("b.log", "all fine"),
        ("docs/guide.md", "an error here"),
        ("docs/deep/x.txt", "error"),
        ("node_modules/pkg.txt", "error"),
    ])
}

const CANDIDATES: &[&str] = &[
    "a.txt",
    "b.log",
    "docs",
    "docs/deep/x.txt",
    "docs/guide.md",
    "gone.txt",
    "node_modules/pkg.txt",
    "/elsewhere/x.txt",
];

/// Exclusions, depth and the start folder hold for Everything's candidates as for the walk; a
/// candidate that is gone is no hit and no error.
#[tokio::test]
async fn candidates_are_judged_as_the_walk_would_meet_them() {
    let root = tree();
    let query = SearchQuery {
        name: "*.txt | node_modules\\".to_owned(),
        ..SearchQuery::default()
    };
    let (found, skipped) = judged(root.path(), CANDIDATES, &query).await;
    assert_eq!(found, ["a.txt", "docs/deep/x.txt"]);
    assert_eq!(skipped, 0);

    let shallow = SearchQuery {
        max_depth: Some(1),
        ..query
    };
    assert_eq!(judged(root.path(), CANDIDATES, &shallow).await.0, ["a.txt"]);

    let folders = SearchQuery {
        name: "docs".to_owned(),
        ..SearchQuery::default()
    };
    assert_eq!(judged(root.path(), CANDIDATES, &folders).await.0, ["docs"]);
}

/// The content is read from the candidate, as the walk reads it.
#[tokio::test]
async fn the_content_of_a_candidate_is_searched() {
    let root = tree();
    let query = SearchQuery {
        content: Some(ContentQuery {
            text: "ERROR".to_owned(),
            ..ContentQuery::default()
        }),
        ..SearchQuery::default()
    };
    assert_eq!(
        judged(root.path(), CANDIDATES, &query).await.0,
        [
            "a.txt",
            "docs/deep/x.txt",
            "docs/guide.md",
            "node_modules/pkg.txt"
        ]
    );
}
