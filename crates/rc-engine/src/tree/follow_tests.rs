use rc_vfs::VfsPath;

use super::Follow;

#[test]
fn the_panel_follows_after_the_pause_of_the_last_move_only() {
    let mut follow = Follow::new(false);
    let first = follow
        .moved(VfsPath::local("/a"))
        .expect("a pause to wait for");
    let second = follow
        .moved(VfsPath::local("/b"))
        .expect("a pause to wait for");
    assert_eq!(follow.rested(first), None, "a later move came");
    assert_eq!(follow.rested(second), Some(VfsPath::local("/b")));
    assert!(!follow.waiting());
    assert_eq!(
        follow.moved(VfsPath::local("/b")),
        None,
        "the panel shows it already"
    );
}

#[test]
fn with_open_on_enter_only_enter_shows_the_folder() {
    let mut follow = Follow::new(true);
    assert_eq!(follow.moved(VfsPath::local("/a")), None);
    assert!(follow.waiting());
    assert_eq!(follow.rested(1), None);
    assert_eq!(
        follow.enter(VfsPath::local("/a")),
        VfsPath::local("/a"),
        "Enter shows the folder under the cursor"
    );
    assert!(!follow.waiting());
}

#[test]
fn a_panel_that_moved_by_itself_is_not_sent_back() {
    let mut follow = Follow::new(false);
    let turn = follow.moved(VfsPath::local("/a")).expect("a pause");
    follow.showing(&VfsPath::local("/a"));
    assert_eq!(follow.rested(turn), None);
    follow.set_on_enter(true);
    assert_eq!(follow.moved(VfsPath::local("/b")), None);
}
