use std::{path::Path, sync::Arc};

use rc_vfs::{BackendId, LocalFs};
use tempfile::TempDir;

use super::Walk;
use crate::search::{ContentQuery, SearchCancel, SearchEvent, SearchQuery, SearchStats};
use crate::test_support;

/// ```text
/// root/
///   a.txt            "alpha error"
///   b.log            "all fine"
///   .hidden.txt      "error"
///   docs/
///     guide.md       "an error here"
///     notes.txt      ""
///     deep/
///       x.txt        "error"
///   node_modules/
///     pkg.txt        "error"
/// ```
fn tree() -> TempDir {
    test_support::tree(&[
        ("a.txt", "alpha error"),
        ("b.log", "all fine"),
        (".hidden.txt", "error"),
        ("docs/guide.md", "an error here"),
        ("docs/notes.txt", ""),
        ("docs/deep/x.txt", "error"),
        ("node_modules/pkg.txt", "error"),
    ])
}

fn walk(root: &Path, query: &SearchQuery, cancel: SearchCancel) -> Walk {
    Walk::new(
        Arc::new(LocalFs::new()),
        BackendId::Local,
        vec![root.to_path_buf()],
        query.compile().expect("valid query"),
        cancel,
    )
}

/// The hits as paths below `root` with `/`, in walk order, and the final stats.
async fn hits(root: &Path, query: &SearchQuery) -> (Vec<String>, SearchStats) {
    let mut walk = walk(root, query, SearchCancel::default());
    let mut found = Vec::new();
    while let Some(event) = walk.next().await {
        match event {
            SearchEvent::Hit(hit) => {
                let below = hit.path.path.strip_prefix(root).expect("below the root");
                found.push(below.to_string_lossy().replace('\\', "/"));
            }
            SearchEvent::Finished { stats, cancelled } => {
                assert!(!cancelled);
                return (found, stats);
            }
            SearchEvent::Progress { .. } | SearchEvent::Skipped { .. } => {}
        }
    }
    panic!("the walk ended without Finished");
}

fn named(name: &str) -> SearchQuery {
    SearchQuery {
        name: name.to_owned(),
        ..SearchQuery::default()
    }
}

#[tokio::test]
async fn masks_find_files_before_subdirectories_each_by_name() {
    let root = tree();
    let (found, stats) = hits(root.path(), &named("*.txt")).await;
    assert_eq!(
        found,
        [
            ".hidden.txt",
            "a.txt",
            "docs/notes.txt",
            "docs/deep/x.txt",
            "node_modules/pkg.txt"
        ]
    );
    assert_eq!(stats.dirs, 4);
    assert_eq!(stats.hits, 5);
}

#[tokio::test]
async fn exclusions_and_directory_exclusions() {
    let root = tree();
    let (found, _) = hits(root.path(), &named("*.txt | .* node_modules\\")).await;
    assert_eq!(found, ["a.txt", "docs/notes.txt", "docs/deep/x.txt"]);
    let (found, _) = hits(root.path(), &named("| *.txt;*.md")).await;
    assert_eq!(found, ["b.log", "docs", "node_modules", "docs/deep"]);
}

#[tokio::test]
async fn a_regex_on_names() {
    let root = tree();
    let query = SearchQuery {
        name: r"^[a-b]\.(txt|log)$".to_owned(),
        name_regex: true,
        ..SearchQuery::default()
    };
    assert_eq!(hits(root.path(), &query).await.0, ["a.txt", "b.log"]);
}

#[tokio::test]
async fn depth_limit_and_hidden_entries() {
    let root = tree();
    let top = SearchQuery {
        max_depth: Some(0),
        ..named("*.txt")
    };
    assert_eq!(hits(root.path(), &top).await.0, [".hidden.txt", "a.txt"]);
    let one = SearchQuery {
        max_depth: Some(1),
        ..named("*.txt")
    };
    assert_eq!(
        hits(root.path(), &one).await.0,
        [
            ".hidden.txt",
            "a.txt",
            "docs/notes.txt",
            "node_modules/pkg.txt"
        ]
    );
    if cfg!(unix) {
        let visible = SearchQuery {
            skip_hidden: true,
            max_depth: Some(0),
            ..named("*.txt")
        };
        assert_eq!(hits(root.path(), &visible).await.0, ["a.txt"]);
    }
}

#[tokio::test]
async fn content_and_not_containing() {
    let root = tree();
    let content = |not_containing| SearchQuery {
        content: Some(ContentQuery {
            text: "ERROR".to_owned(),
            not_containing,
            ..ContentQuery::default()
        }),
        ..named("| node_modules\\ .*")
    };
    assert_eq!(
        hits(root.path(), &content(false)).await.0,
        ["a.txt", "docs/guide.md", "docs/deep/x.txt"]
    );
    assert_eq!(
        hits(root.path(), &content(true)).await.0,
        ["b.log", "docs/notes.txt"],
        "directories are never content hits"
    );
}

#[tokio::test]
async fn a_cancelled_walk_finishes_at_once() {
    let root = tree();
    let cancel = SearchCancel::default();
    let mut walk = walk(root.path(), &named("*"), cancel.clone());
    assert!(matches!(walk.next().await, Some(SearchEvent::Hit(_))));
    cancel.cancel();
    assert!(matches!(
        walk.next().await,
        Some(SearchEvent::Finished {
            cancelled: true,
            ..
        })
    ));
    assert_eq!(walk.next().await, None);
}

#[tokio::test]
async fn a_missing_start_folder_is_skipped_with_its_code() {
    let root = tree();
    let mut walk = walk(
        &root.path().join("gone"),
        &named("*"),
        SearchCancel::default(),
    );
    assert!(matches!(
        walk.next().await,
        Some(SearchEvent::Skipped {
            code: "not_found",
            ..
        })
    ));
    assert!(matches!(
        walk.next().await,
        Some(SearchEvent::Finished { stats, cancelled: false }) if stats.skipped == 1
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_cycles_end() {
    use std::{fs, os::unix::fs::symlink};

    let root = tree();
    // A link up to the root, a relative one to its own folder, and two that point at each
    // other's folders.
    symlink(root.path(), root.path().join("docs/up")).expect("symlink");
    symlink(".", root.path().join("docs/deep/self")).expect("symlink");
    fs::create_dir(root.path().join("p")).expect("mkdir");
    fs::create_dir(root.path().join("q")).expect("mkdir");
    symlink("../q", root.path().join("p/to_q")).expect("symlink");
    symlink("../p", root.path().join("q/to_p")).expect("symlink");

    let unfollowed = hits(root.path(), &named("x.txt")).await.0;
    assert_eq!(unfollowed, ["docs/deep/x.txt"]);

    let followed = SearchQuery {
        follow_symlinks: true,
        ..named("x.txt")
    };
    let found = hits(root.path(), &followed).await.0;
    assert_eq!(
        found,
        ["docs/deep/x.txt"],
        "no link leads back into the tree"
    );

    let links = SearchQuery {
        follow_symlinks: true,
        ..named("to_*")
    };
    let found = hits(root.path(), &links).await.0;
    assert_eq!(
        found,
        ["p/to_q", "p/to_q/to_p", "p/to_q/to_p/to_q", "q/to_p"],
        "every link is a hit by name; each real folder is entered once"
    );
}

/// A test read judge: `a.txt` is secret.
fn a_is_secret(path: &Path) -> Option<&'static str> {
    (path.file_name()? == "a.txt").then_some("a test secret")
}

/// With a read guard a content search never reads a guarded file: it is no hit whether the text
/// is in it or not (`not_containing`), and only counts as skipped.
#[tokio::test]
async fn a_guarded_file_is_skipped_by_a_content_search_whatever_it_holds() {
    let root = tree();
    let guard = crate::read_guard::ReadGuard::new(a_is_secret, &BackendId::Local);
    for not_containing in [false, true] {
        let query = SearchQuery {
            content: Some(ContentQuery {
                text: if not_containing { "absent" } else { "alpha" }.to_owned(),
                not_containing,
                ..ContentQuery::default()
            }),
            ..named("a.txt")
        };
        let mut walk = walk(root.path(), &query, SearchCancel::default()).guarded(Some(guard));
        let (mut found, mut refused) = (0, Vec::new());
        while let Some(event) = walk.next().await {
            match event {
                SearchEvent::Hit(_) => found += 1,
                SearchEvent::Skipped { code, .. } => refused.push(code),
                SearchEvent::Progress { .. } | SearchEvent::Finished { .. } => {}
            }
        }
        assert_eq!(found, 0, "not_containing {not_containing}");
        assert_eq!(refused, ["sensitive_path"]);
        assert_eq!(walk.stats().hits, 0);
        assert_eq!(walk.stats().skipped, 1);
    }
    let (found, _) = hits(root.path(), &named("a.txt")).await;
    assert_eq!(
        found,
        ["a.txt"],
        "a name search lists it, without reading it"
    );
}
