//! The Structure view's state (RC-100-41): offered only with a plugin's comparison - not without
//! a licence -, filtered by kind, and a change opened at its line in the text view.

use std::{fmt::Write as _, sync::Arc};

use rc_engine::{
    VfsPath,
    compare::{
        EditSession, Origin, TextDiff, TextOptions,
        edit::SideFile,
        structure::{Change, ChangeKind, CompareSummary, Comparison, StructureOutcome},
        text::{TextFile, decode},
    },
};

use super::super::{DiffWindow, Loaded};
use super::{DiffView, Structure, StructureNote};

/// 100 lines, `line 20` and `line 70` changed on the right.
fn window() -> DiffWindow {
    let (mut left, mut right) = (String::new(), String::new());
    for line in 0..100 {
        let _ = writeln!(left, "line {line}");
        let changed = if line == 20 || line == 70 { "x" } else { "" };
        let _ = writeln!(right, "line {line}{changed}");
    }
    let (left, right) = (
        decode(left.as_bytes()).expect("text"),
        decode(right.as_bytes()).expect("text"),
    );
    let side = |file: &TextFile, name: &str| SideFile {
        file: file.clone(),
        origin: Origin {
            path: VfsPath::local(name),
            size: 0,
            modified: None,
        },
        read_only: None,
    };
    let session = EditSession::new(side(&left, "/l/a.json"), side(&right, "/r/a.json"));
    let diff = TextDiff::compute(left, right, &TextOptions::default());
    let mut window = DiffWindow::new(VfsPath::local("/l/a.json"), VfsPath::local("/r/a.json"), 1);
    window.set_page(10);
    window.loaded(Ok(Loaded::Text(Arc::new(diff), Box::new(session))));
    window
}

fn change(kind: ChangeKind, path: &str, line: u32) -> Change {
    Change {
        kind,
        path: path.to_owned(),
        column: None,
        old_value: Some("1".to_owned()),
        new_value: Some("2".to_owned()),
        old_line: Some(line),
        new_line: Some(line),
    }
}

fn compared() -> StructureOutcome {
    let changes = vec![
        change(ChangeKind::Changed, "/a", 71),
        change(ChangeKind::Added, "/b", 21),
    ];
    let mut summary = CompareSummary::default();
    for change in &changes {
        summary.count(change.kind);
    }
    StructureOutcome::Compared {
        plugin: "Data compare".to_owned(),
        plugin_id: "019d0000-0000-7000-8000-0000000000d1".to_owned(),
        comparison: Comparison {
            summary,
            changes,
            notices: Vec::new(),
        },
        notices: Vec::new(),
    }
}

#[test]
fn the_view_is_offered_only_with_a_comparison() {
    let mut window = window();
    assert!(matches!(window.structure, Structure::Running));
    window.view = DiffView::Structure;
    window.structure_loaded(Ok(StructureOutcome::Unlicensed {
        plugin: "Data compare".to_owned(),
    }));
    assert!(matches!(window.structure, Structure::Absent(None)));
    assert_eq!(
        window.view,
        DiffView::Text,
        "no licence: the text view alone"
    );
    assert!(!window.showing_structure());

    window.structure_loaded(Ok(compared()));
    assert!(window.pane().is_some());
    window.view = DiffView::Structure;
    assert!(window.showing_structure());
}

#[test]
fn a_limit_or_a_failure_leaves_a_note() {
    let mut window = window();
    window.structure_loaded(Ok(StructureOutcome::TooLarge { limit_mib: 16 }));
    assert!(matches!(
        window.structure,
        Structure::Absent(Some(StructureNote::TooLarge(16)))
    ));
    window.structure_loaded(Ok(StructureOutcome::NoPlugin));
    assert!(matches!(window.structure, Structure::Absent(None)));
}

#[test]
fn the_filter_hides_a_kind() {
    let mut window = window();
    window.structure_loaded(Ok(compared()));
    let Structure::Compared(pane) = &mut window.structure else {
        panic!("compared");
    };
    assert_eq!(pane.shown(), vec![0, 1]);
    pane.filter(ChangeKind::Changed, false);
    assert_eq!(pane.shown(), vec![1]);
    pane.filter(ChangeKind::Changed, true);
    assert_eq!(pane.shown(), vec![0, 1]);
}

#[test]
fn a_change_opens_at_its_line_in_the_text_view() {
    let mut window = window();
    window.structure_loaded(Ok(compared()));
    window.view = DiffView::Structure;
    assert!(window.jump_to_change(0));
    assert_eq!(window.view, DiffView::Text);
    let row = window.current.expect("a row jumped to");
    let diff = window.diff().expect("the text diff");
    assert_eq!(
        diff.rows[row].left,
        Some(70),
        "old line 71 is line index 70"
    );
    assert!(window.visible().contains(&row));
    assert!(!window.jump_to_change(9), "no such change");
}
