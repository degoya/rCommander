use super::*;

#[test]
fn a_checkpoint_every_64_lines_or_after_a_long_line() {
    let mut index = LineIndex::new(3);
    for line in 1..200u64 {
        // Line 100 is 100 KiB long: the line after it gets a checkpoint of its own.
        let offset = 3 + line * 10 + if line > 100 { 100 * 1024 } else { 0 };
        index.line_start(line, offset, LineState::Continued, line / 2);
    }
    index.finish(200);
    assert_eq!(index.lines(), 200);
    assert_eq!(index.at_or_before(63).line, 0);
    assert_eq!(index.at_or_before(63).offset, 3);
    assert_eq!(index.at_or_before(64).line, 64);
    assert_eq!(index.at_or_before(64).state, LineState::Continued);
    assert_eq!(index.at_or_before(100).line, 64);
    assert_eq!(index.at_or_before(150).line, 101);
}

#[test]
fn the_checkpoint_before_an_entry_counts_the_entries_before_it() {
    let mut index = LineIndex::new(0);
    // Every line an entry.
    for line in 1..300u64 {
        index.line_start(line, line * 10, LineState::Clear, line);
    }
    index.finish(300);
    assert_eq!(index.before_entry(0).line, 0);
    assert_eq!(index.before_entry(63).line, 0);
    assert_eq!(index.before_entry(64).line, 64);
    assert_eq!(index.before_entry(64).entries, 64);
    assert_eq!(index.before_entry(200).line, 192);
}

#[test]
fn the_index_stays_bounded() {
    let mut index = LineIndex::new(0);
    for line in 1..(MAX_CHECKPOINTS as u64 * 64 * 3) {
        index.line_start(line, line, LineState::Clear, 0);
    }
    index.finish(MAX_CHECKPOINTS as u64 * 64 * 3);
    assert!(index.memory_bytes() <= MAX_CHECKPOINTS * size_of::<Checkpoint>());
    let point = index.at_or_before(1_000_000);
    assert!(point.line <= 1_000_000 && point.line > 1_000_000 - 1024);
}
