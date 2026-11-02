use super::*;

const TRIPLE_RAW_F: Quote = Quote {
    double: true,
    triple: true,
    raw: true,
    fields: true,
};
const PLAIN: Quote = Quote {
    double: false,
    triple: false,
    raw: false,
    fields: false,
};

#[test]
fn every_kind_comes_back_as_it_was_pushed() {
    let mut frames = Frames::default();
    assert_eq!(frames.top(), None);
    for frame in [
        Frame::Str(TRIPLE_RAW_F),
        Frame::Field,
        Frame::Brackets(1),
        Frame::Str(PLAIN),
        Frame::Spec,
    ] {
        assert!(frames.push(frame));
        assert_eq!(frames.top(), Some(frame));
    }
    frames.pop();
    assert_eq!(frames.top(), Some(Frame::Str(PLAIN)));
    frames.pop();
    frames.pop();
    assert_eq!(frames.top(), Some(Frame::Field));
    frames.field_to_spec();
    assert_eq!(frames.top(), Some(Frame::Spec));
    frames.pop();
    assert_eq!(frames.top(), Some(Frame::Str(TRIPLE_RAW_F)));
    frames.pop();
    frames.pop();
    assert_eq!(frames, Frames::default());
}

#[test]
fn brackets_share_one_slot_and_close_one_at_a_time() {
    let mut frames = Frames::default();
    for _ in 0..100 {
        frames.open_bracket();
    }
    // 63 in the first slot, 37 in the second.
    assert_eq!(frames.depth(), 2);
    assert_eq!(frames.top(), Some(Frame::Brackets(37)));
    for _ in 0..99 {
        frames.close_bracket();
    }
    assert_eq!(frames.top(), Some(Frame::Brackets(1)));
    frames.close_bracket();
    assert_eq!(frames.depth(), 0);
    // A closer with nothing open, or a field on top, closes nothing.
    frames.close_bracket();
    assert_eq!(frames.depth(), 0);
    frames.push(Frame::Field);
    frames.close_bracket();
    assert_eq!(frames.top(), Some(Frame::Field));
}

#[test]
fn past_the_slots_frames_are_counted_as_brackets() {
    let mut frames = Frames::default();
    for _ in 0..SLOTS {
        assert!(!frames.full());
        assert!(frames.push(Frame::Str(PLAIN)));
    }
    assert!(frames.full());
    assert!(!frames.push(Frame::Field));
    assert_eq!(frames.top(), Some(Frame::Brackets(1)));
    frames.open_bracket();
    assert_eq!(frames.depth(), SLOTS + 2);
    frames.close_bracket();
    frames.close_bracket();
    assert_eq!(frames.top(), Some(Frame::Str(PLAIN)));
    assert!(Frame::Str(PLAIN).is_text() && Frame::Spec.is_text());
    assert!(!Frame::Field.is_text() && !Frame::Brackets(3).is_text());
}
