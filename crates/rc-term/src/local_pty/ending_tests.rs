//! The escalation's steps, with a short grace and no process.

use std::time::{Duration, Instant};

use super::{Escalation, Life, escalate};

const GRACE: Duration = Duration::from_millis(20);

fn over() -> Life {
    let life = Life::default();
    life.mark_exited();
    life.mark_drained();
    life
}

#[test]
fn a_terminal_that_is_over_gets_no_signal() {
    let mut sent = Vec::new();
    escalate(&over(), GRACE, |step| sent.push(step));
    assert!(sent.is_empty());
}

#[test]
fn a_program_that_ignores_everything_gets_term_then_kill() {
    let life = Life::default();
    let mut sent = Vec::new();
    let started = Instant::now();
    escalate(&life, GRACE, |step| sent.push(step));
    assert_eq!(sent, [Escalation::Terminate, Escalation::Kill]);
    assert!(started.elapsed() >= GRACE * 2, "each step waits its grace");
}

#[test]
fn a_program_that_ends_on_term_gets_no_kill() {
    let life = Life::default();
    let mut sent = Vec::new();
    escalate(&life, GRACE, |step| {
        sent.push(step);
        life.mark_exited();
        life.mark_drained();
    });
    assert_eq!(sent, [Escalation::Terminate]);
}

#[test]
fn a_reaped_program_whose_pty_is_still_held_is_not_over() {
    // A background job of the group keeps the PTY open after the shell ended.
    let life = Life::default();
    life.mark_exited();
    assert!(!life.wait_over(GRACE));
    let mut sent = Vec::new();
    escalate(&life, GRACE, |step| sent.push(step));
    assert_eq!(sent, [Escalation::Terminate, Escalation::Kill]);
}

#[test]
fn the_end_wakes_a_waiter_before_its_timeout() {
    let life = std::sync::Arc::new(Life::default());
    let marker = std::sync::Arc::clone(&life);
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        marker.mark_drained();
        marker.mark_exited();
    });
    let started = Instant::now();
    assert!(life.wait_over(Duration::from_secs(10)));
    assert!(started.elapsed() < Duration::from_secs(5));
    thread.join().expect("marker");
}

#[test]
fn nothing_runs_once_the_program_is_reaped() {
    let life = Life::default();
    let mut ran = false;
    life.unless_exited(|| ran = true);
    assert!(ran);
    life.mark_exited();
    let mut again = false;
    life.unless_exited(|| again = true);
    assert!(!again);
}
