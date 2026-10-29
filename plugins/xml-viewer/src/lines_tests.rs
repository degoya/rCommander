use super::*;
use crate::lexer::Mode;

fn start(index: &mut LineIndex, line: u64, offset: u64) {
    index.line_start(
        line,
        offset,
        LexState::at(Mode::Comment),
        LayoutState::default(),
    );
}

#[test]
fn a_checkpoint_every_so_many_lines_with_its_state() {
    let mut index = LineIndex::new(3);
    for line in 1..200 {
        start(&mut index, line, 3 + line * 10);
    }
    index.finish(200);
    assert_eq!(index.lines(), 200);
    assert_eq!(index.at_or_before(10).line, 0);
    assert_eq!(index.at_or_before(10).offset, 3);
    let point = index.at_or_before(130);
    assert_eq!((point.line, point.offset), (128, 3 + 1280));
    assert_eq!(point.lex.mode, Mode::Comment);
    assert_eq!(index.exact(64).map(|point| point.offset), Some(643));
    assert_eq!(index.exact(65), None);
}

#[test]
fn the_line_after_a_long_line_gets_a_pinned_checkpoint() {
    let mut index = LineIndex::new(0);
    start(&mut index, 1, 10);
    start(&mut index, 2, 10 + MAX_LINE_BYTES as u64 + 1);
    start(&mut index, 3, 20 + MAX_LINE_BYTES as u64 + 1);
    let point = index.exact(2).expect("pinned");
    assert!(point.pinned);
    assert_eq!(index.exact(3), None);
}

#[test]
fn thinning_keeps_memory_bounded_and_pinned_checkpoints() {
    let mut index = LineIndex::new(0);
    let mut offset = 0u64;
    let long_at = 1000u64;
    for line in 1..(MAX_CHECKPOINTS as u64 * 64 * 3) {
        offset += if line == long_at {
            MAX_LINE_BYTES as u64 + 1
        } else {
            1
        };
        start(&mut index, line, offset);
    }
    assert!(index.points.len() < MAX_CHECKPOINTS);
    assert!(index.exact(long_at).is_some_and(|point| point.pinned));
    assert_eq!(index.at_or_before(0).line, 0);
}
