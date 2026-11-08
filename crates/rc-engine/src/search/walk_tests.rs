use std::{path::Path, sync::Arc};

use rc_vfs::{ArchiveLimits, BackendId, LocalFs};
use tempfile::TempDir;

use super::Walk;
use crate::archives::tests::fixture;
use crate::search::{ContentQuery, Filters, SearchCancel, SearchEvent, SearchQuery, SearchStats};
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
            SearchEvent::Progress { .. }
            | SearchEvent::Skipped { .. }
            | SearchEvent::Duplicates(_) => {}
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
                SearchEvent::Progress { .. }
                | SearchEvent::Finished { .. }
                | SearchEvent::Duplicates(_) => {}
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

/// A folder with `plain.txt`, each multi-entry fixture of rc-vfs (zip, 7z, the tars) and the
/// archives a search has to skip.
fn archives(names: &[&str]) -> TempDir {
    let root = test_support::tree(&[("plain.txt", "hello archive")]);
    for name in names {
        std::fs::copy(fixture(name), root.path().join(name)).expect("copy the fixture");
    }
    root
}

const TREES: &[&str] = &[
    "tree.7z",
    "tree.tar",
    "tree.tar.bz2",
    "tree.tar.gz",
    "tree.tar.xz",
    "tree.tar.zst",
    "tree.zip",
];

/// The hits as `archive!/path` (or the path below `root`), the skipped with their codes.
async fn archive_hits(
    root: &Path,
    query: &SearchQuery,
    limits: ArchiveLimits,
    guard: Option<crate::read_guard::ReadGuard>,
) -> (Vec<String>, Vec<&'static str>) {
    let mut walk = walk(root, query, SearchCancel::default())
        .with_archive_limits(limits)
        .guarded(guard);
    let (mut found, mut skipped) = (Vec::new(), Vec::new());
    while let Some(event) = walk.next().await {
        match event {
            SearchEvent::Hit(hit) => found.push(match crate::archive_file(&hit.path) {
                Some(file) => format!(
                    "{}!{}",
                    file.path.file_name().expect("a name").to_string_lossy(),
                    hit.path.path.to_string_lossy().replace('\\', "/")
                ),
                None => hit
                    .path
                    .path
                    .strip_prefix(root)
                    .expect("below")
                    .to_string_lossy()
                    .into(),
            }),
            SearchEvent::Skipped { code, .. } => skipped.push(code),
            SearchEvent::Progress { .. }
            | SearchEvent::Finished { .. }
            | SearchEvent::Duplicates(_) => {}
        }
    }
    (found, skipped)
}

fn in_archives(query: SearchQuery) -> SearchQuery {
    SearchQuery {
        archives: true,
        ..query
    }
}

/// Every multi-entry format is searched like a folder: by name, size, date and content.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finds_names_and_text_inside_archives() {
    let root = archives(TREES);
    let limits = ArchiveLimits::default();
    let by_name = archive_hits(root.path(), &in_archives(named("readme.md")), limits, None).await;
    let expected: Vec<String> = TREES
        .iter()
        .map(|name| format!("{name}!/docs/readme.md"))
        .collect();
    assert_eq!(by_name, (expected, Vec::new()));

    // The fixtures' entries are 3000 bytes at most and dated 2024-05-17 12:34:56 UTC.
    let sized = in_archives(SearchQuery {
        filters: Filters {
            min_size: Some(3000),
            modified_before: Some(1_715_949_296),
            ..Filters::default()
        },
        ..named("*")
    });
    let (found, _) = archive_hits(root.path(), &sized, limits, None).await;
    let expected: Vec<String> = TREES
        .iter()
        .map(|name| format!("{name}!/deep/nested/data.bin"))
        .collect();
    assert_eq!(found, expected);

    let text = in_archives(SearchQuery {
        content: Some(ContentQuery {
            text: "INSIDE AN ARCHIVE".to_owned(),
            ..ContentQuery::default()
        }),
        // `*.md`: the plain `tree.tar` holds the text unpacked, so it is a hit of its own.
        ..named("*.md")
    });
    let (found, skipped) = archive_hits(root.path(), &text, limits, None).await;
    let expected: Vec<String> = TREES
        .iter()
        .map(|name| format!("{name}!/docs/readme.md"))
        .collect();
    assert_eq!((found, skipped), (expected, Vec::new()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archives_stay_closed_by_default() {
    let root = archives(&["tree.zip"]);
    let (found, _) = archive_hits(root.path(), &named("*"), ArchiveLimits::default(), None).await;
    assert_eq!(
        found,
        ["plain.txt", "tree.zip"],
        "the archive is a file, not entered"
    );
    let top = in_archives(SearchQuery {
        max_depth: Some(0),
        ..named("*")
    });
    let (found, _) = archive_hits(root.path(), &top, ArchiveLimits::default(), None).await;
    assert_eq!(
        found,
        ["plain.txt", "tree.zip"],
        "an archive is one level below its folder"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_archives_are_not_entered() {
    let root = archives(&[]);
    let inner = std::fs::read(fixture("tree.zip")).expect("the fixture");
    let outer = test_support::stored_zip(&[("inner.zip", &inner), ("note.txt", b"x")]);
    std::fs::write(root.path().join("outer.zip"), outer).expect("write");
    let query = in_archives(named("*.zip;hello.txt;note.txt"));
    let (found, skipped) = archive_hits(root.path(), &query, ArchiveLimits::default(), None).await;
    assert_eq!(
        found,
        ["outer.zip", "outer.zip!/inner.zip", "outer.zip!/note.txt"]
    );
    assert_eq!(skipped, Vec::<&str>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_password_or_a_bomb_skips_and_goes_on() {
    let root = archives(&["bomb.tar.gz", "encrypted-aes.zip", "tree.zip"]);
    let strict = ArchiveLimits {
        max_ratio: 100,
        ratio_threshold: 1 << 20,
        ..ArchiveLimits::default()
    };
    let text = in_archives(SearchQuery {
        content: Some(ContentQuery {
            text: "hello".to_owned(),
            ..ContentQuery::default()
        }),
        ..named("*.txt")
    });
    let (found, mut skipped) = archive_hits(root.path(), &text, strict, None).await;
    skipped.sort_unstable();
    assert_eq!(skipped, ["archive_limit", "password_required"]);
    assert_eq!(
        found,
        [
            "plain.txt",
            "tree.zip!/hello.txt",
            "tree.zip!/conformance/a.txt"
        ],
        "the search goes on after both"
    );
}

/// The MCP read guard keeps an archive closed: opening it would read it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_the_guard_keeps_is_not_opened() {
    fn secret_zip(path: &Path) -> Option<&'static str> {
        (path.file_name()? == "tree.zip").then_some("a test secret")
    }
    let root = archives(&["tree.zip", "tree.7z"]);
    let guard = crate::read_guard::ReadGuard::new(secret_zip, &BackendId::Local);
    let query = in_archives(named("hello.txt"));
    let (found, skipped) =
        archive_hits(root.path(), &query, ArchiveLimits::default(), Some(guard)).await;
    assert_eq!(found, ["tree.7z!/hello.txt"]);
    assert_eq!(skipped, ["sensitive_path"]);
}
