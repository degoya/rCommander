use super::*;

#[test]
fn frames_push_and_pop_in_order_and_compare_by_what_is_open() {
    let mut frames = Frames::default();
    assert_eq!(frames.top(), None);
    let string = Frame::Str(Str::plain(StrKind::Quoted));
    assert!(frames.push(string));
    assert!(frames.push(Frame::Field { depth: 0, close: 1 }));
    assert_eq!(frames.field(), Some((0, 1)));
    frames.set_top(Frame::Field { depth: 2, close: 1 });
    assert_eq!(frames.field(), Some((2, 1)));
    frames.pop();
    assert_eq!(frames.top(), Some(string));
    assert_eq!(frames.field(), None);
    // A popped slot is cleared: the same frames, the same state.
    let mut only = Frames::default();
    only.push(string);
    assert_eq!(frames, only);
    frames.pop();
    frames.pop();
    assert_eq!(frames, Frames::default());
}

#[test]
fn a_full_stack_refuses_more() {
    let mut frames = Frames::default();
    for _ in 0..SLOTS {
        assert!(frames.push(Frame::Comment(1)));
    }
    assert!(frames.full());
    assert!(!frames.push(Frame::Spec(1)));
    assert_eq!(frames.depth(), SLOTS);
    assert_eq!(frames.top(), Some(Frame::Comment(1)));
}

/// The state is copied into every checkpoint: it stays a few dozen bytes.
#[test]
fn the_line_state_stays_small() {
    assert!(size_of::<crate::lexer::LineState>() <= 64);
}
