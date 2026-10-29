use std::{path::PathBuf, sync::Arc};

use rc_engine::{
    ConflictChoice, DeleteMode, VfsPath,
    sync::{
        ItemKind, PairState, RowFilter, SyncAction, SyncItem, SyncMode, SyncOptions, SyncPlan,
        SyncRow, proposed_action,
    },
};

use super::{Phase, SyncDialog};

fn row(path: &str, left: bool, right: bool, state: PairState, mode: SyncMode) -> SyncRow {
    let item = |there: bool| {
        there.then_some(SyncItem {
            kind: ItemKind::File,
            size: 3,
            modified: None,
        })
    };
    SyncRow {
        path: PathBuf::from(path),
        left: item(left),
        right: item(right),
        state,
        action: proposed_action(state, mode),
        partial: false,
    }
}

/// `a` equal, `b` only left, `c` only right, `d` newer on the left.
fn plan(mode: SyncMode) -> Arc<SyncPlan> {
    Arc::new(SyncPlan {
        left: VfsPath::local("/l"),
        right: VfsPath::local("/r"),
        options: SyncOptions {
            mode,
            ..SyncOptions::default()
        },
        rows: vec![
            row("a", true, true, PairState::Equal, mode),
            row("b", true, false, PairState::LeftOnly, mode),
            row("c", false, true, PairState::RightOnly, mode),
            row("d", true, true, PairState::LeftNewer, mode),
        ],
        skipped: 0,
    })
}

fn dialog(mode: SyncMode) -> SyncDialog {
    let mut dialog = SyncDialog::new(VfsPath::local("/l"), VfsPath::local("/r"));
    dialog.form.mode = mode;
    dialog.start(1).expect("options");
    dialog.planned(Ok(plan(mode)));
    dialog
}

fn names(dialog: &SyncDialog) -> Vec<String> {
    (0..dialog.shown.len())
        .filter_map(|at| dialog.row(at))
        .map(|(_, row)| row.path.display().to_string())
        .collect()
}

#[test]
fn the_filter_shows_differences_first_and_the_other_views_on_demand() {
    let mut dialog = dialog(SyncMode::Both);
    assert_eq!(dialog.phase, Phase::Ready);
    assert_eq!(names(&dialog), ["b", "c", "d"]);
    dialog.set_filter(RowFilter::All);
    assert_eq!(names(&dialog), ["a", "b", "c", "d"]);
    dialog.set_filter(RowFilter::LeftOnly);
    assert_eq!(names(&dialog), ["b"]);
    dialog.set_filter(RowFilter::RightOnly);
    assert_eq!(names(&dialog), ["c"]);
    dialog.set_filter(RowFilter::Equal);
    assert_eq!(names(&dialog), ["a"]);
}

#[test]
fn a_click_cycles_through_the_actions_the_row_allows() {
    let mut dialog = dialog(SyncMode::Both);
    // `c`, only on the right: copy to the left, delete on the right, nothing.
    let action = |dialog: &SyncDialog| dialog.row(1).map(|(_, row)| row.action);
    assert_eq!(action(&dialog), Some(SyncAction::CopyToLeft));
    dialog.cycle(1);
    assert_eq!(action(&dialog), Some(SyncAction::DeleteRight));
    dialog.cycle(1);
    assert_eq!(action(&dialog), Some(SyncAction::None));
    dialog.cycle(1);
    assert_eq!(action(&dialog), Some(SyncAction::CopyToLeft));
    assert_eq!(names(&dialog), ["b", "c", "d"], "the row stays shown");
}

#[test]
fn the_tolerance_must_be_a_number_of_seconds() {
    let mut dialog = SyncDialog::new(VfsPath::local("/l"), VfsPath::local("/r"));
    dialog.form.tolerance = "two".into();
    assert_eq!(dialog.start(1), None);
    assert!(dialog.invalid);
    assert_eq!(dialog.phase, Phase::Idle);
    dialog.form.tolerance = " 3 ".into();
    let options = dialog.start(2).expect("reads");
    assert_eq!(options.time_tolerance_secs, 3);
    assert!(!dialog.invalid);
    assert_eq!(dialog.phase, Phase::Comparing);
}

#[test]
fn a_new_comparison_stops_the_running_one() {
    let mut dialog = dialog(SyncMode::Both);
    let old = dialog.cancel.clone();
    dialog.start(2).expect("options");
    assert!(old.is_cancelled());
    assert!(dialog.plan.is_none());
    assert!(dialog.shown.is_empty());
    dialog.planned(Err("cancelled"));
    assert_eq!(dialog.phase, Phase::Failed("cancelled"));
}

#[test]
fn the_confirmation_runs_the_deletes_only_while_ticked() {
    let mut dialog = dialog(SyncMode::MirrorToRight);
    assert!(dialog.ask());
    let confirm = dialog.confirm.as_ref().expect("asked");
    assert!(confirm.has_deletes());
    let (plan, run) = dialog.confirmed().expect("confirmed");
    assert!(run.deletes_confirmed);
    assert_eq!(run.delete_mode, DeleteMode::Trash);
    assert_eq!(run.on_conflict, None, "a replaced file asks");
    assert_eq!(plan.rows[2].action, SyncAction::DeleteRight);

    let confirm = dialog.confirm.as_mut().expect("asked");
    confirm.deletes = false;
    confirm.permanent = true;
    confirm.ask_overwrite = false;
    let (plan, run) = dialog.confirmed().expect("confirmed");
    assert!(!run.deletes_confirmed);
    assert_eq!(run.on_conflict, Some(ConflictChoice::Overwrite));
    assert_eq!(plan.rows[2].action, SyncAction::None);
    assert_eq!(plan.rows[1].action, SyncAction::CopyToRight);
    assert_eq!(
        dialog.plan.as_ref().map(|plan| plan.rows[2].action),
        Some(SyncAction::DeleteRight),
        "the dialog's plan keeps its delete"
    );
}

#[test]
fn a_plan_without_actions_asks_nothing() {
    let mut dialog = dialog(SyncMode::Both);
    for at in 0..dialog.shown.len() {
        while dialog.row(at).map(|(_, row)| row.action) != Some(SyncAction::None) {
            dialog.cycle(at);
        }
    }
    assert!(!dialog.ask());
    assert_eq!(dialog.phase, Phase::Nothing);
    assert!(dialog.confirm.is_none());
}

#[test]
fn the_cursor_stays_on_screen() {
    let mut dialog = dialog(SyncMode::Both);
    dialog.set_filter(RowFilter::All);
    dialog.set_page(2);
    dialog.step(3);
    assert_eq!((dialog.rows.cursor(), dialog.rows.top()), (3, 2));
    dialog.step(10);
    assert_eq!(dialog.rows.cursor(), 3, "clamped to the last row");
    dialog.point_at(0);
    assert_eq!((dialog.rows.cursor(), dialog.rows.top()), (0, 0));
    dialog.scroll(9);
    assert_eq!(dialog.rows.top(), 2);
    assert_eq!(dialog.visible(), 2..4);
}
