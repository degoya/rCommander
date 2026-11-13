use iced_core::mouse::Button;

use super::ViewState;
use crate::input::{MIDDLE_BUTTON, RIGHT_BUTTON};

/// RC-110-28: only a button whose press went to the program is released for it, and once.
#[test]
fn a_release_goes_to_the_program_only_after_its_press_did() {
    let mut state = ViewState::default();
    assert!(!state.release(Button::Right), "pasted, not reported");
    state.held |= 1 << RIGHT_BUTTON;
    assert!(!state.release(Button::Middle));
    assert!(
        !state.release(Button::Left),
        "the left button is the drag's"
    );
    assert!(state.release(Button::Right));
    assert!(!state.release(Button::Right), "once");
    state.held |= 1 << MIDDLE_BUTTON;
    assert!(state.release(Button::Middle));
    assert_eq!(state.held, 0);
}
