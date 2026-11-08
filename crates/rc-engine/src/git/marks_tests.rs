use super::{FolderMarks, summarize};
use crate::git::{Change, GitMark, Item};

fn item(path: &str, mark: GitMark) -> Item {
    Item {
        path: path.to_owned(),
        change: Change::Mark(mark),
    }
}

fn deleted(path: &str) -> Item {
    Item {
        path: path.to_owned(),
        change: Change::Deleted,
    }
}

fn marks(summary: &FolderMarks) -> Vec<(&str, GitMark)> {
    summary
        .marks
        .iter()
        .map(|(name, mark)| (name.as_str(), *mark))
        .collect()
}

/// At the root: files mark themselves, a sub-folder takes its most important child, a folder
/// named whole its own mark, a deleted file only counts.
#[test]
fn a_sub_folder_takes_its_most_important_child() {
    let items = [
        item("README.md", GitMark::Modified),
        item("src/a.rs", GitMark::Untracked),
        item("src/deep/b.rs", GitMark::Conflict),
        item("src/c.rs", GitMark::Added),
        item("docs/x.md", GitMark::Renamed),
        item("docs/y.md", GitMark::Added),
        item("notes/", GitMark::Untracked),
        deleted("gone.txt"),
        deleted("lib/old.rs"),
    ];
    let summary = summarize(&items, "", true, false);
    assert_eq!(
        marks(&summary),
        [
            ("README.md", GitMark::Modified),
            ("docs", GitMark::Added),
            ("lib", GitMark::Modified),
            ("notes", GitMark::Untracked),
            ("src", GitMark::Conflict),
        ]
    );
    assert_eq!(summary.deleted, 1);
    assert_eq!(summary.whole, None);
}

/// The order of importance: conflict, modified, added, renamed, untracked.
#[test]
fn the_priority_holds() {
    let order = [
        GitMark::Untracked,
        GitMark::Renamed,
        GitMark::Added,
        GitMark::Modified,
        GitMark::Conflict,
    ];
    for (index, low) in order.iter().enumerate() {
        for high in &order[index + 1..] {
            let items = [item("d/low", *low), item("d/high", *high)];
            let summary = summarize(&items, "", true, false);
            assert_eq!(marks(&summary), [("d", *high)], "{low:?} under {high:?}");
        }
    }
}

/// In a sub-folder only its own paths count, relative to it.
#[test]
fn a_sub_folder_sees_only_its_own_paths() {
    let items = [
        item("src/lib.rs", GitMark::Modified),
        item("src/app/view.rs", GitMark::Added),
        item("srcx/other.rs", GitMark::Conflict),
        item("README.md", GitMark::Modified),
        deleted("src/old.rs"),
    ];
    let summary = summarize(&items, "src", true, false);
    assert_eq!(
        marks(&summary),
        [("app", GitMark::Added), ("lib.rs", GitMark::Modified)]
    );
    assert_eq!(summary.deleted, 1);
}

/// Ignored entries only with `panels.git_ignored`; something ignored deeper down gives no dot.
#[test]
fn ignored_entries_only_with_the_switch() {
    let items = [
        item("target/", GitMark::Ignored),
        item("src/gen/", GitMark::Ignored),
        item("debug.log", GitMark::Ignored),
    ];
    let with = summarize(&items, "", true, false);
    assert_eq!(
        marks(&with),
        [
            ("debug.log", GitMark::Ignored),
            ("target", GitMark::Ignored)
        ]
    );
    let without = summarize(&items, "", false, false);
    assert_eq!(marks(&without), []);
}

/// Inside a folder git names whole - untracked, or ignored above it - every entry has its mark.
#[test]
fn a_folder_named_whole_marks_every_entry() {
    let untracked = summarize(&[item("notes/", GitMark::Untracked)], "notes", true, false);
    assert_eq!(untracked.whole, Some(GitMark::Untracked));
    let ignored = summarize(
        &[item("target/", GitMark::Ignored)],
        "target/debug",
        true,
        false,
    );
    assert_eq!(ignored.whole, Some(GitMark::Ignored));
    let elsewhere = summarize(&[item("target/", GitMark::Ignored)], "targets", true, false);
    assert_eq!(elsewhere.whole, None);
}

/// Where names ignore case, the folder and the entries are matched in lower case.
#[test]
fn names_fold_their_case_where_the_system_does() {
    let items = [item("Src/Main.rs", GitMark::Modified)];
    let folded = summarize(&items, "src", true, true);
    assert_eq!(marks(&folded), [("main.rs", GitMark::Modified)]);
    let exact = summarize(&items, "src", true, false);
    assert_eq!(marks(&exact), []);
}
