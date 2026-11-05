//! Which groups the escalation signals.

use super::targets;

#[test]
fn the_program_group_and_the_foreground_group() {
    assert_eq!(targets(4000, Some(4100), true, 10), [4000, 4100]);
}

#[test]
fn a_foreground_group_that_is_the_program_is_signalled_once() {
    assert_eq!(targets(4000, Some(4000), true, 10), [4000]);
    assert_eq!(targets(4000, None, true, 10), [4000]);
}

#[test]
fn never_group_0_1_or_our_own() {
    assert!(targets(1, Some(0), true, 10).is_empty());
    assert_eq!(targets(4000, Some(10), true, 10), [4000]);
    assert!(targets(10, None, true, 10).is_empty());
}

#[test]
fn with_end_programs_on_close_off_no_group() {
    assert!(targets(4000, Some(4100), false, 10).is_empty());
}
