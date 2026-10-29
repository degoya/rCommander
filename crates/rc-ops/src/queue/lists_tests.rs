use super::{DEFAULT_LIST, ListBoard, MAX_LIST_NAME, check_list_name};

#[test]
fn a_list_name_is_trimmed_and_checked() {
    assert_eq!(
        check_list_name("  Backup NAS ").ok().as_deref(),
        Some("Backup NAS")
    );
    for bad in ["", "   ", "a\tb", "line\nbreak"] {
        assert_eq!(
            check_list_name(bad).map_err(|error| error.code()),
            Err("queue_name_invalid"),
            "{bad:?}"
        );
    }
    let longest = "x".repeat(MAX_LIST_NAME);
    assert!(check_list_name(&longest).is_ok());
    assert!(check_list_name(&format!("{longest}x")).is_err());
}

#[test]
fn the_board_starts_with_the_started_default_list() {
    let board = ListBoard::default();
    assert_eq!(board.started(DEFAULT_LIST).ok(), Some(true));
    assert_eq!(
        board.started(7).map_err(|error| error.code()),
        Err("queue_list_not_found")
    );
}
