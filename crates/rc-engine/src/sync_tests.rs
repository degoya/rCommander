use std::path::PathBuf;

use rc_vfs::VfsPath;

use super::*;

fn file(size: u64) -> Option<SyncItem> {
    Some(SyncItem {
        kind: ItemKind::File,
        size,
        modified: None,
    })
}

fn row(path: &str, left: Option<SyncItem>, right: Option<SyncItem>, state: PairState) -> SyncRow {
    SyncRow {
        path: PathBuf::from(path),
        left,
        right,
        state,
        action: proposed_action(state, SyncMode::Both),
        partial: false,
    }
}

fn sample() -> SyncPlan {
    SyncPlan {
        left: VfsPath::local("/l"),
        right: VfsPath::local("/r"),
        options: SyncOptions::default(),
        rows: vec![
            row("a", file(1), file(1), PairState::Equal),
            row("b", file(10), None, PairState::LeftOnly),
            row("c", None, file(20), PairState::RightOnly),
            row("d", file(5), file(3), PairState::LeftNewer),
        ],
        skipped: 0,
    }
}

#[test]
fn each_mode_proposes_its_actions() {
    use PairState as S;
    use SyncAction as A;
    let table = [
        (
            SyncMode::Both,
            [
                A::None,
                A::CopyToRight,
                A::CopyToLeft,
                A::CopyToRight,
                A::CopyToLeft,
                A::None,
                A::None,
            ],
        ),
        (
            SyncMode::LeftToRight,
            [
                A::None,
                A::CopyToRight,
                A::None,
                A::CopyToRight,
                A::None,
                A::None,
                A::None,
            ],
        ),
        (
            SyncMode::RightToLeft,
            [
                A::None,
                A::None,
                A::CopyToLeft,
                A::None,
                A::CopyToLeft,
                A::None,
                A::None,
            ],
        ),
        (
            SyncMode::MirrorToRight,
            [
                A::None,
                A::CopyToRight,
                A::DeleteRight,
                A::CopyToRight,
                A::CopyToRight,
                A::CopyToRight,
                A::None,
            ],
        ),
        (
            SyncMode::MirrorToLeft,
            [
                A::None,
                A::DeleteLeft,
                A::CopyToLeft,
                A::CopyToLeft,
                A::CopyToLeft,
                A::CopyToLeft,
                A::None,
            ],
        ),
    ];
    let states = [
        S::Equal,
        S::LeftOnly,
        S::RightOnly,
        S::LeftNewer,
        S::RightNewer,
        S::Different,
        S::KindMismatch,
    ];
    for (mode, actions) in table {
        let proposed: Vec<A> = states
            .iter()
            .map(|&state| proposed_action(state, mode))
            .collect();
        assert_eq!(proposed, actions, "{mode:?}");
    }
}

#[test]
fn only_the_mirror_modes_delete() {
    let deleting = |mode| {
        [PairState::LeftOnly, PairState::RightOnly]
            .iter()
            .any(|&state| {
                matches!(
                    proposed_action(state, mode),
                    SyncAction::DeleteLeft | SyncAction::DeleteRight
                )
            })
    };
    assert!(!deleting(SyncMode::Both));
    assert!(!deleting(SyncMode::LeftToRight));
    assert!(!deleting(SyncMode::RightToLeft));
    assert!(deleting(SyncMode::MirrorToRight));
    assert!(deleting(SyncMode::MirrorToLeft));
}

#[test]
fn the_summary_counts_states_and_copied_bytes() {
    let summary = sample().summary();
    assert_eq!(summary.equal, 1);
    assert_eq!(summary.left_only, 1);
    assert_eq!(summary.right_only, 1);
    assert_eq!(summary.left_newer, 1);
    assert_eq!(
        summary.copy_to_right,
        Volume {
            files: 2,
            bytes: 15
        }
    );
    assert_eq!(
        summary.copy_to_left,
        Volume {
            files: 1,
            bytes: 20
        }
    );
}

#[test]
fn an_action_needs_its_side() {
    let mut plan = sample();
    plan.set_action(1, SyncAction::DeleteLeft)
        .expect("b is on the left");
    assert_eq!(plan.rows[1].action, SyncAction::DeleteLeft);
    let error = plan
        .set_action(1, SyncAction::CopyToLeft)
        .expect_err("b is not on the right");
    assert_eq!(error.code(), "sync_action_invalid");
    let error = plan.set_action(9, SyncAction::None).expect_err("no row 9");
    assert_eq!(error.code(), "sync_action_invalid");
}

#[test]
fn the_comparison_filters_and_limits_rows_and_keeps_their_index() {
    let plan = sample();
    let differences = plan.comparison(RowFilter::Differences, 10);
    let indices: Vec<usize> = differences.rows.iter().map(|shown| shown.index).collect();
    assert_eq!(indices, [1, 2, 3]);
    assert!(!differences.truncated);
    let right_only = plan.comparison(RowFilter::RightOnly, 10);
    assert_eq!(right_only.rows.len(), 1);
    assert_eq!(right_only.rows[0].row.path, PathBuf::from("c"));
    let limited = plan.comparison(RowFilter::All, 2);
    assert_eq!(limited.rows.len(), 2);
    assert!(limited.truncated);
}

#[test]
fn options_read_with_fat_tolerance_and_subdirs_by_default() {
    let options: SyncOptions = serde_json::from_str("{}").expect("deserializes");
    assert_eq!(options, SyncOptions::default());
    assert_eq!(options.time_tolerance_secs, FAT_TOLERANCE_SECS);
    assert!(options.subdirs);
    let request: DirCompareRequest =
        serde_json::from_str(r#"{"options":{"mode":"mirror_to_right"}}"#).expect("deserializes");
    assert_eq!(request.options.mode, SyncMode::MirrorToRight);
    assert_eq!(request.limit, DIR_COMPARE_LIMIT);
}
