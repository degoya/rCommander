use std::{fmt::Write as _, sync::Arc};

use rc_engine::{
    VfsPath,
    compare::{TextDiff, TextOptions, text::decode},
};

use super::{DiffOption, DiffWindow, Loaded, Shown};

/// 100 lines, `line 20` and `line 70` changed on the right.
fn window() -> DiffWindow {
    let (mut left, mut right) = (String::new(), String::new());
    for line in 0..100 {
        let _ = writeln!(left, "line {line}");
        let changed = if line == 20 || line == 70 { "x" } else { "" };
        let _ = writeln!(right, "line {line}{changed}");
    }
    let diff = TextDiff::compute(
        decode(left.as_bytes()).expect("text"),
        decode(right.as_bytes()).expect("text"),
        &TextOptions::default(),
    );
    let mut window = DiffWindow::new(VfsPath::local("/l/a"), VfsPath::local("/r/a"), 1);
    window.set_page(10);
    window.loaded(Ok(Loaded::Text(Arc::new(diff))));
    window
}

#[test]
fn a_loaded_diff_opens_at_the_first_difference_with_context() {
    let window = window();
    assert_eq!(window.changes, 2);
    assert_eq!(window.current, Some(20));
    assert_eq!(window.rows.top(), 17);
    assert_eq!(window.visible(), 17..27);
}

#[test]
fn next_and_previous_step_through_the_differences() {
    let mut window = window();
    assert!(window.next());
    assert_eq!(window.current, Some(70));
    assert!(!window.next(), "no difference after the last");
    assert_eq!(window.current, Some(70));
    assert!(window.previous());
    assert_eq!(window.current, Some(20));
    assert!(!window.previous());
}

#[test]
fn after_scrolling_the_next_jump_starts_from_the_screen() {
    let mut window = window();
    window.scroll(40);
    assert_eq!(window.rows.top(), 57);
    assert_eq!(window.current, None);
    assert!(window.previous());
    assert_eq!(window.current, Some(20));
    window.end();
    assert_eq!(window.rows.top(), 90, "the last page");
    window.scroll(5);
    assert_eq!(window.rows.top(), 90, "never past the end");
    window.home();
    assert!(window.next());
    assert_eq!(window.current, Some(20));
}

#[test]
fn a_restart_drops_the_answer_and_stops_the_old_comparison() {
    let mut window = window();
    let old = window.cancel.clone();
    window.set_option(DiffOption::Whitespace, true);
    window.restart(2);
    assert!(old.is_cancelled());
    assert!(!window.cancel.is_cancelled());
    assert!(window.options.ignore_whitespace);
    assert!(matches!(window.shown, Shown::Loading));
    assert_eq!(window.len(), 0);
    window.loaded(Err("not_found"));
    assert!(matches!(window.shown, Shown::Failed("not_found")));
    assert!(!window.next());
}
