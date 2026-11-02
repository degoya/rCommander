use super::*;

#[test]
fn a_checkpoint_every_64_lines_or_after_a_long_line() {
    let mut index = LineIndex::new(0);
    for line in 1..200u64 {
        // Line 100 is 100 KiB long: the line after it gets a checkpoint of its own.
        let offset = line * 10 + if line > 100 { 100 * 1024 } else { 0 };
        index.line_start(line, offset, LineState::default());
    }
    index.finish(200);
    assert_eq!(index.lines(), 200);
    assert_eq!(index.at_or_before(63).line, 0);
    assert_eq!(index.at_or_before(64).line, 64);
    assert_eq!(index.at_or_before(100).line, 64);
    assert_eq!(index.at_or_before(150).line, 101);
}

#[test]
fn the_index_thins_itself_and_stays_bounded() {
    let mut index = LineIndex::new(3);
    let lines = 64 * MAX_CHECKPOINTS as u64 * 5;
    for line in 1..lines {
        index.line_start(line, 3 + line * 40, LineState::default());
    }
    index.finish(lines);
    assert!(index.points.len() < MAX_CHECKPOINTS);
    assert!(index.memory_bytes() <= MAX_CHECKPOINTS * size_of::<Checkpoint>());
    assert_eq!(index.at_or_before(0).offset, 3);
    // Every checkpoint left is still where its line starts.
    assert!(
        index
            .points
            .iter()
            .all(|point| point.offset == 3 + point.line * 40)
    );
    let last = index.at_or_before(lines - 1);
    assert!(lines - 1 - last.line < index.every_lines);
}
