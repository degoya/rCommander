use super::*;

#[test]
fn a_checkpoint_every_64_lines_or_after_a_long_line() {
    let mut index = LineIndex::new(0);
    for line in 1..200u64 {
        // Line 100 is 100 KiB long: the line after it gets a checkpoint of its own.
        let offset = line * 10 + if line > 100 { 100 * 1024 } else { 0 };
        let state = LineState {
            quoted: line % 2 == 0,
            column: 3,
        };
        index.start(line, offset, state);
    }
    index.finish(200);
    assert_eq!(index.count(), 200);
    assert_eq!(index.at_or_before(63).at, 0);
    assert_eq!(index.at_or_before(64).at, 64);
    assert!(index.at_or_before(64).state.quoted);
    assert_eq!(index.at_or_before(100).at, 64);
    assert_eq!(index.at_or_before(150).at, 101);
    assert_eq!(index.exact(101).map(|point| point.at), Some(101));
    assert_eq!(index.exact(102), None);
}

#[test]
fn the_first_record_moves_past_blank_lines() {
    let mut index = RecordIndex::new(0);
    index.start(0, 4, ());
    index.start(1, 10, ());
    index.finish(2);
    assert_eq!(index.at_or_before(1).offset, 4);
}

#[test]
fn the_index_thins_itself_and_stays_bounded() {
    let mut index = RecordIndex::new(3);
    let records = 64 * MAX_CHECKPOINTS as u64 * 5;
    for record in 1..records {
        index.start(record, 3 + record * 40, ());
    }
    index.finish(records);
    assert!(index.points.len() < MAX_CHECKPOINTS);
    assert!(index.memory_bytes() <= MAX_CHECKPOINTS * size_of::<Checkpoint<()>>());
    assert_eq!(index.at_or_before(0).offset, 3);
    // Every checkpoint left is still where its record starts.
    assert!(
        index
            .points
            .iter()
            .all(|point| point.offset == 3 + point.at * 40)
    );
    let last = index.at_or_before(records - 1);
    assert!(records - 1 - last.at < index.every_count);
}
