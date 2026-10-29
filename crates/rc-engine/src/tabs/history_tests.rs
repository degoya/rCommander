use super::*;

fn dir(name: &str) -> VfsPath {
    VfsPath::local(format!("/{name}"))
}

fn walked(names: &[&str]) -> History {
    let mut history = History::new(dir(names[0]));
    for name in &names[1..] {
        history.visit(&dir(name));
    }
    history
}

#[test]
fn a_visit_becomes_the_newest_folder() {
    let history = walked(&["a", "b", "c"]);
    assert_eq!(history.entries(), [dir("a"), dir("b"), dir("c")]);
    assert_eq!(history.current(), &dir("c"));
    assert_eq!(history.step(-1), Some((1, &dir("b"))));
    assert_eq!(history.step(1), None);
}

#[test]
fn the_same_folder_again_is_no_new_entry() {
    let mut history = walked(&["a", "b"]);
    history.visit(&dir("b"));
    assert_eq!(history.entries().len(), 2);
}

/// The acceptance check of RC-010-03: after going back, a new folder drops what lay ahead.
#[test]
fn back_then_a_new_folder_drops_the_forward_history() {
    let mut history = walked(&["a", "b", "c"]);
    assert!(history.go(0));
    assert_eq!(history.current(), &dir("a"));
    assert_eq!(history.step(1), Some((1, &dir("b"))));
    // Going back by the history only moves the place; the window's visit of the same folder
    // changes nothing.
    history.visit(&dir("a"));
    assert_eq!(history.entries().len(), 3);
    history.visit(&dir("x"));
    assert_eq!(history.entries(), [dir("a"), dir("x")]);
    assert_eq!(history.step(1), None);
}

#[test]
fn only_the_newest_folders_are_kept() {
    let names: Vec<String> = (0..HISTORY_LIMIT + 5).map(|n| n.to_string()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let history = walked(&refs);
    assert_eq!(history.entries().len(), HISTORY_LIMIT);
    assert_eq!(history.entries()[0], dir("5"));
    assert_eq!(history.pos(), HISTORY_LIMIT - 1);
}

#[test]
fn go_outside_the_entries_changes_nothing() {
    let mut history = walked(&["a", "b"]);
    assert!(!history.go(2));
    assert_eq!(history.current(), &dir("b"));
}

#[test]
fn steps_and_restore_are_each_others_inverse() {
    let mut history = walked(&["a", "b", "c", "d"]);
    history.go(1);
    let steps: Vec<(i64, VfsPath)> = history
        .steps()
        .map(|(step, path)| (step, path.clone()))
        .collect();
    assert_eq!(steps, [(-1, dir("a")), (1, dir("c")), (2, dir("d"))]);
    let back = steps
        .iter()
        .filter(|(step, _)| *step < 0)
        .map(|(_, path)| path.clone())
        .collect();
    let ahead = steps
        .iter()
        .filter(|(step, _)| *step > 0)
        .map(|(_, path)| path.clone())
        .collect();
    assert_eq!(History::restore(back, dir("b"), ahead), history);
}

#[test]
fn a_restored_history_keeps_the_nearest_folders() {
    let back: Vec<VfsPath> = (0..HISTORY_LIMIT + 3)
        .map(|n| dir(&n.to_string()))
        .collect();
    let history = History::restore(back, dir("here"), vec![dir("next")]);
    assert_eq!(history.entries().len(), HISTORY_LIMIT);
    assert_eq!(history.current(), &dir("here"));
    assert_eq!(
        history.step(-1).map(|(_, path)| path.clone()),
        Some(dir(&(HISTORY_LIMIT + 2).to_string()))
    );
    assert_eq!(history.step(1), None, "no room is left ahead");
}
