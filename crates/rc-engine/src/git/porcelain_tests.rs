use super::{Change, Item, PorcelainError, parse};
use crate::git::{GitBranch, GitMark};

const CHANGES: &[u8] = include_bytes!("../../tests/fixtures/git_porcelain/branch_and_changes.bin");
const DETACHED: &[u8] = include_bytes!("../../tests/fixtures/git_porcelain/detached.bin");
const INITIAL: &[u8] = include_bytes!("../../tests/fixtures/git_porcelain/initial.bin");

fn mark(path: &str, mark: GitMark) -> Item {
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

/// Every record type once: changed, added, renamed (the new path, the old one skipped),
/// unmerged, deleted in the folder and in the index, a name with a space and one with an
/// umlaut, an untracked and an ignored folder.
#[test]
fn every_record_type_is_read() {
    let status = parse(CHANGES).expect("parsed");
    assert_eq!(
        status.branch,
        GitBranch {
            head: Some("main".into()),
            oid: Some("0123456789abcdef0123456789abcdef01234567".into()),
            upstream: Some("origin/main".into()),
            ahead: 1,
            behind: 2,
        }
    );
    assert_eq!(
        status.items,
        [
            mark("src/lib.rs", GitMark::Modified),
            mark("src/new file.rs", GitMark::Added),
            mark("docs/neu name.md", GitMark::Renamed),
            mark("src/conflict.rs", GitMark::Conflict),
            deleted("gone.txt"),
            deleted("src/removed.rs"),
            mark("src/\u{c4}nderung.txt", GitMark::Modified),
            mark("notes/", GitMark::Untracked),
            mark("src/scratch.txt", GitMark::Untracked),
            mark("target/", GitMark::Ignored),
        ]
    );
}

#[test]
fn a_detached_head_names_its_commit_and_no_branch() {
    let status = parse(DETACHED).expect("parsed");
    assert_eq!(status.branch.head, None);
    assert_eq!(
        status.branch.oid.as_deref(),
        Some("89abcdef0123456789abcdef0123456789abcdef")
    );
    assert_eq!(status.items, [mark("README.md", GitMark::Modified)]);
}

#[test]
fn a_repository_without_commits_has_no_commit() {
    let status = parse(INITIAL).expect("parsed");
    assert_eq!(status.branch.oid, None);
    assert_eq!(status.branch.head.as_deref(), Some("main"));
    assert_eq!(status.items, [mark("a.txt", GitMark::Untracked)]);
}

#[test]
fn an_empty_answer_is_a_clean_folder() {
    assert_eq!(parse(b"").expect("parsed").items, []);
}

/// The worktree's change wins over the index's where it weighs more: added, then changed again,
/// is changed.
#[test]
fn the_more_important_of_index_and_folder_counts() {
    let record = |xy: &str| {
        let line = format!(
            "1 {xy} N... 100644 100644 100644 {zero} {zero} f\0",
            zero = "0".repeat(40)
        );
        parse(line.as_bytes()).expect("parsed").items[0].change
    };
    assert_eq!(record("AM"), Change::Mark(GitMark::Modified));
    assert_eq!(record("A."), Change::Mark(GitMark::Added));
    assert_eq!(record(".A"), Change::Mark(GitMark::Added));
    assert_eq!(record("RM"), Change::Mark(GitMark::Modified));
    assert_eq!(record("MD"), Change::Deleted);
    assert_eq!(record("T."), Change::Mark(GitMark::Modified));
}

/// Every prefix of a real answer is either a complete answer or an error - never a panic, never
/// a record read from half a line.
#[test]
fn a_cut_answer_is_an_error_never_a_panic() {
    for end in 1..CHANGES.len() {
        let cut = &CHANGES[..end];
        let parsed = parse(cut);
        if cut.last() == Some(&0) {
            // Cut right after a rename's new path: its old one is missing.
            if let Err(error) = parsed {
                assert_eq!(error, PorcelainError::NoOrigin, "at {end}");
            }
        } else {
            assert_eq!(parsed, Err(PorcelainError::Truncated), "at {end}");
        }
    }
}

#[test]
fn malformed_records_are_refused() {
    assert_eq!(parse(b"\0"), Err(PorcelainError::Empty));
    assert_eq!(parse(b"? a\0\0? b\0"), Err(PorcelainError::Empty));
    assert_eq!(parse(b"?\0"), Err(PorcelainError::Empty));
    assert_eq!(parse(b"? \0"), Err(PorcelainError::Empty));
    assert_eq!(
        parse(b"1 .M N... 100644\0"),
        Err(PorcelainError::Malformed { kind: '1' })
    );
    assert_eq!(
        parse(b"1 .MX N... a b c d e f\0"),
        Err(PorcelainError::Malformed { kind: '1' })
    );
    assert_eq!(
        parse(b"u UU N... a b c d e f g\0"),
        Err(PorcelainError::Malformed { kind: 'u' })
    );
    assert_eq!(
        parse(b"# branch.ab +x -1\0"),
        Err(PorcelainError::Malformed { kind: '#' })
    );
}

#[test]
fn unknown_headers_and_records_are_skipped() {
    let status = parse(b"# stash 3\0# future thing\0z what\0? a\0").expect("parsed");
    assert_eq!(status.items, [mark("a", GitMark::Untracked)]);
}
