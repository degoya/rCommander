use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use rc_vfs::{Attributes, Entry, EntryKind, LocalFs, VfsPath};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::{Naming, compare_times, plan};
use crate::{
    compare::Side,
    sync::{SyncAction, SyncMode, SyncOptions, SyncPlan},
};

fn base() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

/// A file at `root/path` with `content`, modified `offset` seconds after [`base`].
fn file(root: &Path, path: &str, content: &str, offset: u64) {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(&path, content).expect("write");
    fs::File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(base() + Duration::from_secs(offset))
        .expect("set mtime");
}

fn trees() -> (TempDir, TempDir) {
    (
        tempfile::tempdir().expect("temp dir"),
        tempfile::tempdir().expect("temp dir"),
    )
}

async fn run(left: &TempDir, right: &TempDir, options: &SyncOptions) -> SyncPlan {
    let local = LocalFs::new();
    let (left, right) = (VfsPath::local(left.path()), VfsPath::local(right.path()));
    plan(
        Side {
            vfs: &local,
            path: &left,
        },
        Side {
            vfs: &local,
            path: &right,
        },
        options,
        &CancellationToken::new(),
    )
    .await
    .expect("plans")
}

/// Every row as `path state action`, `/` between path parts.
fn lines(plan: &SyncPlan) -> Vec<String> {
    plan.rows
        .iter()
        .map(|row| {
            let path: Vec<String> = row
                .path
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            format!("{} {:?} {:?}", path.join("/"), row.state, row.action)
        })
        .collect()
}

#[tokio::test]
async fn new_changed_and_missing_files_are_found_in_both_directions() {
    let (left, right) = trees();
    for (root, newer_left, newer_right) in [(&left, 100, 0), (&right, 0, 100)] {
        file(root.path(), "same.txt", "s", 0);
        file(root.path(), "newer_left.txt", "l", newer_left);
        file(root.path(), "newer_right.txt", "r", newer_right);
    }
    file(left.path(), "grown.txt", "short", 0);
    file(right.path(), "grown.txt", "longer", 0);
    file(left.path(), "only_left.txt", "x", 0);
    file(left.path(), "sub/deep.txt", "x", 0);
    file(right.path(), "only_right.txt", "x", 0);
    file(right.path(), "rdir/x.txt", "x", 0);

    let both = run(&left, &right, &SyncOptions::default()).await;
    assert_eq!(
        lines(&both),
        [
            "grown.txt Different None",
            "newer_left.txt LeftNewer CopyToRight",
            "newer_right.txt RightNewer CopyToLeft",
            "only_left.txt LeftOnly CopyToRight",
            "only_right.txt RightOnly CopyToLeft",
            "same.txt Equal None",
            "rdir RightOnly CopyToLeft",
            "rdir/x.txt RightOnly CopyToLeft",
            "sub LeftOnly CopyToRight",
            "sub/deep.txt LeftOnly CopyToRight",
        ]
    );
    let summary = both.summary();
    assert_eq!(summary.copy_to_right.files, 3);
    assert_eq!(summary.copy_to_left.files, 3);
    assert_eq!(summary.different, 1);

    let one_way = run(
        &left,
        &right,
        &SyncOptions {
            mode: SyncMode::LeftToRight,
            ..SyncOptions::default()
        },
    )
    .await;
    let copying: Vec<String> = lines(&one_way)
        .into_iter()
        .filter(|line| !line.ends_with(" None"))
        .collect();
    assert_eq!(
        copying,
        [
            "newer_left.txt LeftNewer CopyToRight",
            "only_left.txt LeftOnly CopyToRight",
            "sub LeftOnly CopyToRight",
            "sub/deep.txt LeftOnly CopyToRight",
        ]
    );
}

#[tokio::test]
async fn the_mirror_deletes_what_only_the_target_has_and_only_when_confirmed() {
    let (left, right) = trees();
    file(left.path(), "keep.txt", "k", 0);
    file(right.path(), "keep.txt", "k", 0);
    file(right.path(), "extra.txt", "e", 0);
    let mirror = run(
        &left,
        &right,
        &SyncOptions {
            mode: SyncMode::MirrorToRight,
            ..SyncOptions::default()
        },
    )
    .await;
    assert_eq!(lines(&mirror)[0], "extra.txt RightOnly DeleteRight");
    let error = mirror.operations(false).expect_err("unconfirmed");
    assert_eq!(error.code(), "sync_deletes_unconfirmed");
    assert!(
        right.path().join("extra.txt").exists(),
        "planning deletes nothing"
    );
    assert_eq!(mirror.operations(true).expect("confirmed").len(), 1);
}

#[tokio::test]
async fn fat_times_two_seconds_apart_are_not_a_difference() {
    let (left, right) = trees();
    // FAT stores even seconds: a copy made at :01 reads back as :00 or :02.
    file(left.path(), "a.txt", "a", 1);
    file(right.path(), "a.txt", "a", 2);
    file(left.path(), "b.txt", "b", 3);
    file(right.path(), "b.txt", "b", 1);
    let tolerant = run(&left, &right, &SyncOptions::default()).await;
    assert_eq!(lines(&tolerant), ["a.txt Equal None", "b.txt Equal None"]);
    let exact = run(
        &left,
        &right,
        &SyncOptions {
            time_tolerance_secs: 0,
            ..SyncOptions::default()
        },
    )
    .await;
    assert_eq!(
        lines(&exact),
        ["a.txt RightNewer CopyToLeft", "b.txt LeftNewer CopyToRight"]
    );
}

#[test]
fn an_hour_apart_is_equal_only_when_daylight_saving_is_ignored() {
    let at = |seconds| Some(base() + Duration::from_secs(seconds));
    let plain = SyncOptions::default();
    let dst = SyncOptions {
        ignore_dst_hour: true,
        ..SyncOptions::default()
    };
    assert_eq!(
        compare_times(at(3601), at(0), &plain),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(
        compare_times(at(3601), at(0), &dst),
        Some(std::cmp::Ordering::Equal)
    );
    assert_eq!(
        compare_times(at(0), at(3700), &dst),
        Some(std::cmp::Ordering::Less)
    );
    assert_eq!(compare_times(None, at(0), &plain), None);
}

#[tokio::test]
async fn content_decides_when_asked_for() {
    let (left, right) = trees();
    file(left.path(), "same_size.txt", "aaaa", 0);
    file(right.path(), "same_size.txt", "bbbb", 0);
    file(left.path(), "touched.txt", "same", 0);
    file(right.path(), "touched.txt", "same", 500);
    let by_date = run(&left, &right, &SyncOptions::default()).await;
    assert_eq!(
        lines(&by_date),
        [
            "same_size.txt Equal None",
            "touched.txt RightNewer CopyToLeft"
        ]
    );
    let by_content = run(
        &left,
        &right,
        &SyncOptions {
            by_content: true,
            ..SyncOptions::default()
        },
    )
    .await;
    assert_eq!(
        lines(&by_content),
        ["same_size.txt Different None", "touched.txt Equal None"]
    );
    let by_size = run(
        &left,
        &right,
        &SyncOptions {
            ignore_date: true,
            ..SyncOptions::default()
        },
    )
    .await;
    assert_eq!(
        lines(&by_size),
        ["same_size.txt Equal None", "touched.txt Equal None"]
    );
}

#[tokio::test]
async fn masks_and_subdirs_leave_rows_out_and_mark_the_directory_partial() {
    let (left, right) = trees();
    file(left.path(), "a.rs", "x", 0);
    file(left.path(), "a.bak", "x", 0);
    file(left.path(), "src/b.rs", "x", 0);
    file(left.path(), "src/b.bak", "x", 0);
    file(left.path(), "target/c.rs", "x", 0);
    let masked = run(
        &left,
        &right,
        &SyncOptions {
            masks: "*.rs | *.bak target\\".into(),
            ..SyncOptions::default()
        },
    )
    .await;
    assert_eq!(
        lines(&masked),
        [
            "a.rs LeftOnly CopyToRight",
            "src LeftOnly CopyToRight",
            "src/b.rs LeftOnly CopyToRight",
        ]
    );
    assert!(masked.rows[1].partial, "src/b.bak was left out");
    let flat = run(
        &left,
        &right,
        &SyncOptions {
            subdirs: false,
            ..SyncOptions::default()
        },
    )
    .await;
    assert_eq!(
        lines(&flat),
        ["a.bak LeftOnly CopyToRight", "a.rs LeftOnly CopyToRight"]
    );
}

#[tokio::test]
async fn a_file_against_a_directory_is_a_mismatch_with_no_action() {
    let (left, right) = trees();
    file(left.path(), "x", "file", 0);
    file(right.path(), "x/inner.txt", "dir", 0);
    let mismatch = run(&left, &right, &SyncOptions::default()).await;
    assert_eq!(lines(&mismatch), ["x KindMismatch None"]);
    assert!(!mismatch.rows[0].allows(SyncAction::CopyToRight));
}

#[tokio::test]
async fn a_root_that_is_no_directory_is_an_error_and_cancelling_stops() {
    let (left, right) = trees();
    file(left.path(), "f", "x", 0);
    let local = LocalFs::new();
    let (not_a_dir, dir) = (
        VfsPath::local(left.path().join("f")),
        VfsPath::local(right.path()),
    );
    let side = |path| Side { vfs: &local, path };
    let error = plan(
        side(&not_a_dir),
        side(&dir),
        &SyncOptions::default(),
        &CancellationToken::new(),
    )
    .await
    .expect_err("not a directory");
    assert_eq!(error.code(), "not_a_directory");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = plan(side(&dir), side(&dir), &SyncOptions::default(), &cancel)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code(), "cancelled");
}

fn entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        kind: EntryKind::File,
        size: 0,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

#[test]
fn names_pair_exactly_first_then_ignoring_case_on_a_case_insensitive_side() {
    let naming = Naming {
        ignore_case: true,
        right_spelling: true,
    };
    let pairs = naming.pair(
        vec![entry("Readme.md"), entry("a"), entry("A")],
        vec![entry("README.md"), entry("A")],
    );
    let shown: Vec<(String, Option<String>, Option<String>)> = pairs
        .into_iter()
        .map(|(name, l, r)| (name, l.map(|e| e.name), r.map(|e| e.name)))
        .collect();
    let pair = |name: &str, l: Option<&str>, r: Option<&str>| {
        (name.to_owned(), l.map(str::to_owned), r.map(str::to_owned))
    };
    assert_eq!(
        shown,
        [
            pair("A", Some("A"), Some("A")),
            pair("README.md", Some("Readme.md"), Some("README.md")),
            pair("a", Some("a"), None),
        ]
    );
    let exact = Naming {
        ignore_case: false,
        right_spelling: false,
    }
    .pair(vec![entry("x")], vec![entry("X")]);
    assert_eq!(exact.len(), 2);
}
