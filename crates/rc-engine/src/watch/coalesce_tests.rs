//! The bundling rule on a clock the test moves: one change, a flood, a slow folder.

use std::time::{Duration, Instant};

use super::{Coalescer, MIN_GAP, QUIET};

/// Feeds the `changes` (offsets from the start, in order) and reads whenever a reading is due,
/// each taking `took`, up to `until`; the offsets the readings began at.
fn readings(changes: &[Duration], took: Duration, until: Duration) -> Vec<Duration> {
    let start = Instant::now();
    let mut coalescer = Coalescer::default();
    let mut began = Vec::new();
    let mut busy: Option<Instant> = None;
    let mut catch_up = |coalescer: &mut Coalescer, now: Instant, began: &mut Vec<Duration>| {
        loop {
            if let Some(end) = busy.filter(|end| *end <= now) {
                coalescer.finished(end);
                busy = None;
            }
            match coalescer.due() {
                Some(due) if busy.is_none() && due <= now => {
                    coalescer.started(due);
                    began.push(due - start);
                    busy = Some(due + took);
                }
                _ => break,
            }
        }
    };
    for offset in changes {
        let now = start + *offset;
        catch_up(&mut coalescer, now, &mut began);
        coalescer.changed(now);
    }
    catch_up(&mut coalescer, start + until, &mut began);
    began
}

#[test]
fn one_change_is_read_once_after_the_quiet_time() {
    let began = readings(
        &[Duration::ZERO],
        Duration::from_millis(20),
        Duration::from_secs(5),
    );
    assert_eq!(began, [QUIET]);
}

#[test]
fn ten_thousand_changes_in_two_seconds_are_at_most_four_readings() {
    let changes: Vec<Duration> = (0..10_000u64)
        .map(|index| Duration::from_micros(200 * index))
        .collect();
    let began = readings(&changes, Duration::from_millis(50), Duration::from_secs(10));
    assert!(!began.is_empty() && began.len() <= 4, "{began:?}");
    // The flood is read while it lasts, not only after it.
    assert!(began[0] <= MIN_GAP, "{began:?}");
}

#[test]
fn after_a_reading_of_three_seconds_the_next_waits_six() {
    let took = Duration::from_secs(3);
    let changes = [Duration::ZERO, Duration::from_millis(500)];
    let began = readings(&changes, took, Duration::from_secs(30));
    assert_eq!(began.len(), 2, "{began:?}");
    let pause = began[1] - (began[0] + took);
    assert!(pause >= Duration::from_secs(6), "{began:?}");
}

#[test]
fn nothing_changed_is_nothing_read() {
    assert_eq!(Coalescer::default().due(), None);
    assert!(readings(&[], Duration::ZERO, Duration::from_secs(60)).is_empty());
}

#[test]
fn a_change_while_reading_waits_for_the_end() {
    let start = Instant::now();
    let mut coalescer = Coalescer::default();
    coalescer.changed(start);
    coalescer.started(start + QUIET);
    coalescer.changed(start + QUIET + Duration::from_millis(10));
    assert_eq!(coalescer.due(), None, "a reading runs");
    let end = start + Duration::from_millis(400);
    coalescer.finished(end);
    assert_eq!(coalescer.due(), Some(end + MIN_GAP));
}

#[test]
fn a_change_waits_until_its_reading_begins() {
    let now = Instant::now();
    let mut coalescer = Coalescer::default();
    assert!(!coalescer.pending());
    coalescer.changed(now);
    assert!(coalescer.pending());
    coalescer.started(now + QUIET);
    assert!(!coalescer.pending());
}
