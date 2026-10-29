use std::path::PathBuf;

use rc_vfs::VfsPath;

use super::SyncOperation;
use crate::sync::{
    ItemKind, PairState, SyncAction, SyncItem, SyncOptions, SyncPlan, SyncRow, proposed_action,
};

fn item(kind: ItemKind) -> Option<SyncItem> {
    Some(SyncItem {
        kind,
        size: 1,
        modified: None,
    })
}

/// A row with the action `mode` proposes; `l`/`r` name the kind on each side ("f", "d", "").
fn row(path: &str, l: &str, r: &str, state: PairState, options: &SyncOptions) -> SyncRow {
    let side = |kind: &str| match kind {
        "f" => item(ItemKind::File),
        "d" => item(ItemKind::Dir),
        _ => None,
    };
    SyncRow {
        path: PathBuf::from(path),
        left: side(l),
        right: side(r),
        state,
        action: proposed_action(state, options.mode),
        partial: false,
    }
}

fn plan(options: SyncOptions, rows: Vec<SyncRow>) -> SyncPlan {
    SyncPlan {
        left: VfsPath::local("/l"),
        right: VfsPath::local("/r"),
        options,
        rows,
        skipped: 0,
    }
}

fn at(path: &str) -> VfsPath {
    VfsPath::local(path)
}

/// `new/` and `new/a.txt` only on the left, `b.txt` newer on the left, `c.txt` only on the
/// right, `old/x` only on the right.
fn rows(options: &SyncOptions) -> Vec<SyncRow> {
    vec![
        row("b.txt", "f", "f", PairState::LeftNewer, options),
        row("c.txt", "", "f", PairState::RightOnly, options),
        row("new", "d", "", PairState::LeftOnly, options),
        row("new/a.txt", "f", "", PairState::LeftOnly, options),
        row("old", "", "d", PairState::RightOnly, options),
        row("old/x", "", "f", PairState::RightOnly, options),
    ]
}

#[test]
fn both_ways_copies_each_way_and_a_one_sided_directory_whole() {
    let options = SyncOptions::default();
    let operations = plan(options.clone(), rows(&options))
        .operations(false)
        .expect("no deletes");
    assert_eq!(
        operations,
        [
            SyncOperation::Copy {
                sources: vec![at("/l/b.txt")],
                target: at("/r"),
                overwrite: true,
            },
            SyncOperation::Copy {
                sources: vec![at("/r/c.txt"), at("/r/old")],
                target: at("/l"),
                overwrite: false,
            },
            SyncOperation::Copy {
                sources: vec![at("/l/new")],
                target: at("/r"),
                overwrite: false,
            },
        ]
    );
}

#[test]
fn mirroring_deletes_nothing_without_a_confirmation() {
    let options = SyncOptions {
        mode: crate::sync::SyncMode::MirrorToRight,
        ..SyncOptions::default()
    };
    let mirror = plan(options.clone(), rows(&options));
    let error = mirror.operations(false).expect_err("deletes need a yes");
    assert_eq!(error.code(), "sync_deletes_unconfirmed");
    let operations = mirror.operations(true).expect("confirmed");
    assert_eq!(
        operations.last(),
        Some(&SyncOperation::Delete {
            sources: vec![at("/r/c.txt"), at("/r/old")],
        })
    );
}

#[test]
fn a_partial_directory_is_created_and_filled_file_by_file() {
    let options = SyncOptions::default();
    let mut rows = rows(&options);
    rows[2].partial = true;
    let operations = plan(options, rows).operations(false).expect("no deletes");
    assert_eq!(operations[0], SyncOperation::Mkdir { path: at("/r/new") });
    assert!(operations.contains(&SyncOperation::Copy {
        sources: vec![at("/l/new/a.txt")],
        target: at("/r/new"),
        overwrite: false,
    }));
}

#[test]
fn a_directory_with_a_row_set_otherwise_is_not_deleted_whole() {
    let options = SyncOptions {
        mode: crate::sync::SyncMode::MirrorToRight,
        ..SyncOptions::default()
    };
    let mut keep = plan(options.clone(), rows(&options));
    keep.set_action(5, SyncAction::None).expect("allowed");
    let operations = keep.operations(true).expect("confirmed");
    // `old/x` stays, so `old` stays too; only `c.txt` goes.
    assert_eq!(
        operations.last(),
        Some(&SyncOperation::Delete {
            sources: vec![at("/r/c.txt")],
        })
    );
}

#[test]
fn rows_left_alone_give_no_operations() {
    let options = SyncOptions::default();
    let mut idle = plan(options.clone(), rows(&options));
    for index in 0..idle.rows.len() {
        idle.set_action(index, SyncAction::None).expect("allowed");
    }
    assert!(idle.operations(false).expect("nothing").is_empty());
}

/// RC-080-12 (X2): a row a synchronization writes into a local folder keeps Windows' rules there:
/// on Windows `notes: draft` would be a stream and the override a spoofed name; elsewhere the
/// names are kept.
#[test]
fn a_row_name_windows_cannot_hold_is_refused_on_a_local_windows_side() {
    let options = SyncOptions::default();
    for name in ["notes: draft.txt", "invoice\u{202e}fdp.exe", "con"] {
        let rows = vec![row(name, "f", "", PairState::LeftOnly, &options)];
        let result = plan(options.clone(), rows).operations(false);
        assert_eq!(result.is_err(), cfg!(windows), "{name}: {result:?}");
        if let Err(error) = result {
            assert_eq!(error.code(), "invalid_name", "{name}");
        }
    }
}
