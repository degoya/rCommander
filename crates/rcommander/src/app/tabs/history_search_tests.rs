use rc_engine::{Chord, FrequentFolder, PanelId, VfsPath};

use super::*;

fn key(text: &str) -> Chord {
    text.parse().expect("chord")
}

fn dir(name: &str) -> VfsPath {
    VfsPath::local(format!("/{name}"))
}

fn opened() -> (Popup, HistorySearch) {
    let history = Popup::history(PanelId::Left, &[dir("src"), dir("docs"), dir("tests")], 2);
    let frequent = [
        FrequentFolder {
            path: dir("projects"),
            visits: 9,
        },
        FrequentFolder {
            path: dir("downloads"),
            visits: 4,
        },
    ];
    with_frequent(history, &frequent)
}

fn labels(popup: &Popup) -> Vec<&str> {
    popup.rows.iter().map(|row| row.label.as_str()).collect()
}

#[test]
fn the_most_visited_folders_follow_the_history() {
    let (popup, search) = opened();
    assert_eq!(
        popup.rows.len(),
        6,
        "three folders, a heading, two frequent ones"
    );
    assert!(popup.rows[3].action.is_none(), "the heading");
    assert_eq!(popup.rows[4].detail, "9");
    assert_eq!(popup.rows[5].action, Some(PopupAction::OpenFrequent(1)));
    assert!(search.owns(&popup));
    assert!(!search.owns(&Popup::close_all(PanelId::Left)));
}

#[test]
fn typing_filters_the_history() {
    let (mut popup, mut search) = opened();
    assert_eq!(search.key(&key("o"), &mut popup), Some(PopupKey::Stay));
    assert_eq!(search.key(&key("c"), &mut popup), Some(PopupKey::Stay));
    assert_eq!(labels(&popup), ["/docs"], "part of the path, case ignored");
    search.key(&key("Backspace"), &mut popup);
    search.key(&key("Backspace"), &mut popup);
    search.key(&key("d"), &mut popup);
    search.key(&key("o"), &mut popup);
    assert_eq!(
        labels(&popup),
        ["/docs", t!("history.frequent"), "/downloads"],
        "the heading stays above a matching folder"
    );
    assert_eq!(popup.cursor, 0);
    assert_eq!(
        popup.key(&key("Enter")),
        PopupKey::Take(PopupAction::HistoryJump(1))
    );
    // Escape empties the search first, then the list closes.
    assert_eq!(search.key(&key("Escape"), &mut popup), Some(PopupKey::Stay));
    assert_eq!(popup.rows.len(), 6);
    assert_eq!(search.key(&key("Escape"), &mut popup), None);
    assert_eq!(popup.key(&key("Escape")), PopupKey::Close);
}

#[test]
fn wildcards_match_the_whole_path() {
    let (mut popup, mut search) = opened();
    search.typed = "*s".into();
    search.apply(&mut popup);
    assert_eq!(
        labels(&popup),
        [
            "/tests",
            "/docs",
            t!("history.frequent"),
            "/projects",
            "/downloads"
        ]
    );
}
