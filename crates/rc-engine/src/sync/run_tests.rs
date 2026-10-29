use std::path::PathBuf;

use rc_ops::{ConflictChoice, DeleteMode, ErrorPolicy, JobKind};
use rc_vfs::VfsPath;

use super::{RowAction, SyncRun};
use crate::sync::{
    ItemKind, PairState, SyncAction, SyncItem, SyncMode, SyncOptions, SyncPlan, SyncRow,
    proposed_action,
};

/// A row with the action `mode` proposes; `l`/`r` name the kind on each side ("f", "d", "").
fn row(path: &str, l: &str, r: &str, state: PairState, mode: SyncMode) -> SyncRow {
    let side = |kind: &str| {
        let kind = match kind {
            "f" => ItemKind::File,
            "d" => ItemKind::Dir,
            _ => return None,
        };
        Some(SyncItem {
            kind,
            size: 1,
            modified: None,
        })
    };
    SyncRow {
        path: PathBuf::from(path),
        left: side(l),
        right: side(r),
        state,
        action: proposed_action(state, mode),
        partial: false,
    }
}

/// `b.txt` newer on the left, `c.txt` only on the right, `new/` with `new/a.txt` only on the
/// left (partial: something below it was masked out).
fn plan(mode: SyncMode) -> SyncPlan {
    let mut rows = vec![
        row("b.txt", "f", "f", PairState::LeftNewer, mode),
        row("c.txt", "", "f", PairState::RightOnly, mode),
        row("new", "d", "", PairState::LeftOnly, mode),
        row("new/a.txt", "f", "", PairState::LeftOnly, mode),
    ];
    rows[2].partial = true;
    SyncPlan {
        left: VfsPath::local("/l"),
        right: VfsPath::local("/r"),
        options: SyncOptions {
            mode,
            ..SyncOptions::default()
        },
        rows,
        skipped: 0,
    }
}

fn wanted(path: &str, action: SyncAction) -> RowAction {
    RowAction {
        path: PathBuf::from(path),
        action,
    }
}

fn actions(plan: &SyncPlan) -> Vec<SyncAction> {
    plan.rows.iter().map(|row| row.action).collect()
}

#[test]
fn assigned_actions_go_over_the_proposal_or_over_nothing() {
    let mut over = plan(SyncMode::Both);
    over.assign(true, &[wanted("c.txt", SyncAction::DeleteRight)])
        .expect("allowed");
    assert_eq!(
        actions(&over),
        [
            SyncAction::CopyToRight,
            SyncAction::DeleteRight,
            SyncAction::CopyToRight,
            SyncAction::CopyToRight,
        ]
    );
    let mut only = plan(SyncMode::Both);
    only.assign(false, &[wanted("b.txt", SyncAction::CopyToLeft)])
        .expect("allowed");
    assert_eq!(
        actions(&only),
        [
            SyncAction::CopyToLeft,
            SyncAction::None,
            SyncAction::None,
            SyncAction::None,
        ]
    );
}

#[test]
fn an_unknown_path_or_an_impossible_action_changes_nothing() {
    let mut kept = plan(SyncMode::Both);
    let before = actions(&kept);
    let error = kept
        .assign(false, &[wanted("gone.txt", SyncAction::CopyToRight)])
        .expect_err("no such row");
    assert_eq!(error.code(), "sync_row_not_found");
    let error = kept
        .assign(false, &[wanted("c.txt", SyncAction::CopyToRight)])
        .expect_err("no left side to copy");
    assert_eq!(error.code(), "sync_action_invalid");
    assert_eq!(actions(&kept), before);
}

#[test]
fn the_jobs_carry_the_run_settings_and_skip_a_directory_a_copy_makes() {
    let run = SyncRun {
        on_conflict: Some(ConflictChoice::Overwrite),
        on_error: ErrorPolicy::Skip,
        verify: true,
        ..SyncRun::default()
    };
    let jobs = plan(SyncMode::Both).jobs(&run).expect("no deletes");
    let kinds: Vec<_> = jobs.iter().map(|job| job.kind.clone()).collect();
    // `new` is partial: no whole copy, and its mkdir is left to the copy into it.
    assert_eq!(
        kinds,
        [
            JobKind::Copy {
                sources: vec![VfsPath::local("/l/b.txt")],
                target: VfsPath::local("/r"),
            },
            JobKind::Copy {
                sources: vec![VfsPath::local("/r/c.txt")],
                target: VfsPath::local("/l"),
            },
            JobKind::Copy {
                sources: vec![VfsPath::local("/l/new/a.txt")],
                target: VfsPath::local("/r/new"),
            },
        ]
    );
    assert!(jobs.iter().all(|job| job.options.verify
        && job.options.on_conflict == Some(ConflictChoice::Overwrite)
        && job.options.on_error == ErrorPolicy::Skip));
}

#[test]
fn an_empty_directory_still_gets_its_mkdir() {
    let mut empty = plan(SyncMode::Both);
    empty
        .assign(true, &[wanted("new/a.txt", SyncAction::None)])
        .expect("allowed");
    let jobs = empty.jobs(&SyncRun::default()).expect("no deletes");
    assert!(jobs.iter().any(|job| job.kind
        == JobKind::Mkdir {
            path: VfsPath::local("/r/new")
        }));
}

#[test]
fn deletes_run_only_confirmed_and_in_the_chosen_mode() {
    let mirror = plan(SyncMode::MirrorToRight);
    let error = mirror
        .jobs(&SyncRun::default())
        .expect_err("deletes need a yes");
    assert_eq!(error.code(), "sync_deletes_unconfirmed");
    let run = SyncRun {
        deletes_confirmed: true,
        delete_mode: DeleteMode::Permanent,
        ..SyncRun::default()
    };
    let jobs = mirror.jobs(&run).expect("confirmed");
    let last = jobs.last().expect("a delete");
    assert_eq!(
        last.kind,
        JobKind::Delete {
            sources: vec![VfsPath::local("/r/c.txt")],
            mode: DeleteMode::Permanent,
        }
    );
    assert_eq!(last.options.on_conflict, None);
}
