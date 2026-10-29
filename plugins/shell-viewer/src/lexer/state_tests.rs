use super::*;

const KINDS: [Frame; 9] = [
    Frame::Subst,
    Frame::Group,
    Frame::DQuote,
    Frame::Param,
    Frame::Arith,
    Frame::ArithParen,
    Frame::Backtick,
    Frame::CasePattern,
    Frame::CaseBody,
];

#[test]
fn frames_keep_their_kinds_sixteen_deep() {
    let mut frames = Frames::default();
    for at in 0..16 {
        frames.push(KINDS[at % KINDS.len()]);
        assert_eq!(frames.top(), Some(KINDS[at % KINDS.len()]));
    }
    // Deeper ones are only counted, and taken for substitutions.
    frames.push(Frame::DQuote);
    assert_eq!(frames.top(), Some(Frame::Subst));
    frames.pop();
    for at in (0..16).rev() {
        assert_eq!(frames.top(), Some(KINDS[at % KINDS.len()]));
        frames.pop();
    }
    assert_eq!(frames.top(), None);
    frames.pop();
    assert_eq!(frames, Frames::default());
}

#[test]
fn replace_top_swaps_the_innermost_frame() {
    let mut frames = Frames::default();
    frames.push(Frame::Subst);
    frames.push(Frame::CasePattern);
    frames.replace_top(Frame::CaseBody);
    assert_eq!(frames.top(), Some(Frame::CaseBody));
    frames.pop();
    assert_eq!(frames.top(), Some(Frame::Subst));
}

#[test]
fn the_marker_hash_is_fnv_1a() {
    assert_eq!(Fnv::of(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(Fnv::of(b"a"), 0xaf63_dc4c_8601_ec8c);
    let mut hash = Fnv::start();
    b"EOF".iter().for_each(|byte| hash.add(*byte));
    assert_eq!(hash.finish(), Fnv::of(b"EOF"));
    assert_ne!(Fnv::of(b"EOF"), Fnv::of(b"EOG"));
}

#[test]
fn a_line_state_stays_small() {
    // 65536 checkpoints of it fit the index budget many times over.
    assert!(size_of::<LineState>() <= 40, "{}", size_of::<LineState>());
}
