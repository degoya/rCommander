use rc_engine::{Chord, JobState, QueueList, Reorder};

use super::{MenuAction, MenuKey, QueueMenu};
use crate::app::queue_window::{
    QueueWindow,
    selection::tests::{at_once, entry},
};

fn key(menu: &mut QueueMenu, keys: &str) -> MenuKey {
    let chord: Chord = keys.parse().expect("a chord");
    menu.key(&chord)
}

fn lists() -> Vec<QueueList> {
    [(0, ""), (1, "NAS")]
        .map(|(id, name)| QueueList {
            id,
            name: name.into(),
            started: true,
            jobs: 0,
        })
        .to_vec()
}

fn labels(menu: &QueueMenu) -> Vec<&str> {
    menu.items.iter().map(|item| item.label.as_str()).collect()
}

#[test]
fn the_menu_offers_every_action_and_dims_what_does_not_apply() {
    let window = QueueWindow {
        entries: vec![entry(1, JobState::Running, None)],
        selected: Some(1),
        lists: lists(),
        ..QueueWindow::default()
    };
    let menu = QueueMenu::new(&window);
    assert_eq!(
        labels(&menu),
        [
            "Move up",
            "Move down",
            "Move to the front",
            "Move to the end",
            "Move to NAS",
            "Move to a new list...",
            "Remove",
            "Select all",
            "Clear this list",
            "Clear all lists",
        ]
    );
    let enabled: Vec<bool> = menu.items.iter().map(|item| item.enabled).collect();
    assert_eq!(
        enabled,
        [
            false, false, false, false, true, true, true, true, true, true
        ],
        "a running job does not move within its lane"
    );
    assert_eq!(menu.cursor, 4, "on the first entry that applies");
    assert_eq!(menu.take(0), MenuKey::Stay, "a dimmed entry does nothing");
}

#[test]
fn the_keys_walk_past_dimmed_entries_and_take_one() {
    let window = QueueWindow {
        entries: vec![
            entry(1, JobState::Running, None),
            entry(2, JobState::Queued, Some(0)),
        ],
        selected: Some(2),
        lists: lists(),
        ..QueueWindow::default()
    };
    let mut menu = QueueMenu::new(&window);
    assert_eq!(menu.cursor, 0, "a waiting job moves");
    assert_eq!(key(&mut menu, "ArrowUp"), MenuKey::Stay);
    assert_eq!(menu.cursor, 0);
    key(&mut menu, "End");
    assert_eq!(
        key(&mut menu, "Enter"),
        MenuKey::Take(MenuAction::Clear { all_lists: true })
    );
    key(&mut menu, "Home");
    key(&mut menu, "ArrowDown");
    assert_eq!(
        key(&mut menu, "Space"),
        MenuKey::Take(MenuAction::Move(Reorder::Down))
    );
    assert_eq!(key(&mut menu, "Escape"), MenuKey::Close);
    assert_eq!(key(&mut menu, "Shift+F10"), MenuKey::Close);

    let empty = QueueMenu::new(&QueueWindow::default());
    let enabled: Vec<&str> = empty
        .items
        .iter()
        .filter(|item| item.enabled)
        .map(|item| item.label.as_str())
        .collect();
    assert_eq!(enabled, ["Clear all lists"], "an empty list");
}

/// A job run at once (RC-010-25) is paused and removed from the menu, but goes into no list.
#[test]
fn a_job_run_at_once_is_not_moved_into_a_list() {
    let window = QueueWindow {
        entries: vec![
            at_once(1, JobState::Running),
            entry(2, JobState::Queued, Some(0)),
        ],
        selected: Some(1),
        lists: lists(),
        ..QueueWindow::default()
    };
    let menu = QueueMenu::new(&window);
    let enabled = |label: &str| {
        menu.items
            .iter()
            .find(|item| item.label == label)
            .map(|item| item.enabled)
    };
    assert_eq!(enabled("Move to NAS"), Some(false));
    assert_eq!(enabled("Move to a new list..."), Some(false));
    assert_eq!(enabled("Move up"), Some(false));
    assert_eq!(enabled("Remove"), Some(true));
}
