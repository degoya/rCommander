use super::{overflows, shortened, status_parts};
use crate::theme::{
    STATUS_SIZE,
    ellipsis::{ELLIPSIS, estimate_width},
};

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

/// The message of the report (RC-110-17): a locked phone in a panel's status line.
const DEVICE_MESSAGE: &str = "The device does not share its files. Unlock the device and choose \
                              \u{201c}File transfer\u{201d} in its USB settings, or allow access \
                              to its data when it asks.";

#[test]
fn a_status_message_that_fits_is_whole_and_has_no_tooltip() {
    // Half a pixel over the estimate: the sum per character may round apart from the whole.
    let width = estimate_width(DEVICE_MESSAGE, STATUS_SIZE) + 0.5;
    let (shown, tip) = status_parts(DEVICE_MESSAGE, width, STATUS_SIZE);
    assert_eq!(shown, DEVICE_MESSAGE);
    assert_eq!(tip, None, "no tooltip that repeats what is shown");
    let (shown, tip) = status_parts("12 k of 3.4 M in 2 of 40 files", 800.0, STATUS_SIZE);
    assert_eq!(shown, "12 k of 3.4 M in 2 of 40 files");
    assert_eq!(tip, None);
}

#[test]
fn a_long_status_message_is_cut_at_its_end_with_the_whole_in_its_tooltip() {
    let width = 200.0;
    let (shown, tip) = status_parts(DEVICE_MESSAGE, width, STATUS_SIZE);
    assert!(shown.ends_with(ELLIPSIS), "{shown}");
    let start = shown.trim_end_matches(ELLIPSIS);
    assert!(!start.is_empty(), "the start stays");
    assert!(DEVICE_MESSAGE.starts_with(start), "{shown}");
    assert!(
        estimate_width(&shown, STATUS_SIZE) <= width + 0.01,
        "{shown}"
    );
    assert_eq!(tip.as_deref(), Some(DEVICE_MESSAGE));
}

#[test]
fn a_status_message_one_pixel_too_wide_is_cut() {
    let width = estimate_width(DEVICE_MESSAGE, STATUS_SIZE) - 1.0;
    let (shown, tip) = status_parts(DEVICE_MESSAGE, width, STATUS_SIZE);
    assert_ne!(shown, DEVICE_MESSAGE);
    assert!(tip.is_some());
}

#[test]
fn a_status_line_without_room_shows_the_ellipsis_and_an_empty_one_nothing() {
    let (shown, tip) = status_parts(DEVICE_MESSAGE, 0.0, STATUS_SIZE);
    assert_eq!(shown, ELLIPSIS);
    assert_eq!(tip.as_deref(), Some(DEVICE_MESSAGE));
    assert_eq!(status_parts("", 0.0, STATUS_SIZE), (String::new(), None));
}
