use super::{overflows, shortened};
use crate::theme::ellipsis::ELLIPSIS;

#[test]
fn a_path_that_fits_needs_no_tooltip() {
    assert_eq!(shortened(r"C:\a.txt", 400.0), None);
}

#[test]
fn a_long_path_is_shortened_in_the_middle_for_its_tooltip() {
    let path = r"G:\--- incoming ---\Melanie C\Reason\03-melanie_c-what_if.mp3";
    let shown = shortened(path, 160.0).expect("too long for 160 px");
    assert!(shown.starts_with(r"G:\"), "{shown}");
    assert!(shown.ends_with(".mp3"), "{shown}");
    assert!(shown.contains(ELLIPSIS), "{shown}");
}

#[test]
fn no_room_at_all_still_counts_as_shortened() {
    assert_eq!(shortened("abc", 0.0).as_deref(), Some(ELLIPSIS));
}

#[test]
fn a_cell_gets_a_tooltip_only_when_its_value_does_not_fit() {
    assert!(!overflows("server", 400.0), "fits: no tooltip");
    assert!(overflows(
        "a-very-long-connection-name-that-cannot-fit",
        40.0
    ));
    assert!(!overflows("", 0.0), "nothing to show");
}
