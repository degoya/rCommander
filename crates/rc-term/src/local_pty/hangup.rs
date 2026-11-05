//! Unix: the steps after a closed terminal's hang-up as signals (RC-080-62), through `nix`, so
//! without `unsafe` of our own.
//!
//! `portable-pty` starts the program as the leader of a session and a process group of its own
//! (`setsid`), so its id is its group's id. With `terminal.end_programs_on_close` on (the
//! default) the steps go to that group (a shell's background jobs without job control are in
//! it) and to the terminal's foreground group at the close, where an interactive shell runs the
//! program it waits for. A process detached with `setsid` is in neither and runs on. With the
//! setting off they go to the program alone, by its id and only while it is not reaped.
//!
//! A group's id is not given to another process while the group has a member; the escalation
//! only starts for a program that still ran at the close and ends [`END_GRACE`] twice later, so
//! an emptied group's id would have to come round the whole id space in that time.

use std::sync::Arc;

use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::{Pid, getpgrp},
};

use super::ending::{END_GRACE, Escalation, Life, escalate};

/// Starts the escalation for the program `program` (its id) after the hang-up; `foreground` is
/// the terminal's foreground group at the close.
pub(super) fn after_hangup(
    life: &Arc<Life>,
    program: u32,
    foreground: Option<i32>,
    whole_tree: bool,
) {
    let Ok(program) = i32::try_from(program) else {
        return;
    };
    let groups = targets(program, foreground, whole_tree, getpgrp().as_raw());
    let life = Arc::clone(life);
    let started = super::spawn_thread("rc-term-end", move || {
        escalate(&life, END_GRACE, |step| send(&life, program, &groups, step));
    });
    if let Err(error) = started {
        tracing::warn!(%error, "cannot watch the end of a closed terminal's program");
    }
}

/// The groups the steps go to; none when only the program itself is to end. Never group 0 or
/// 1 (`killpg` reads 0 as the caller's own group) and never rCommander's own group `own`.
pub(super) fn targets(
    program: i32,
    foreground: Option<i32>,
    whole_tree: bool,
    own: i32,
) -> Vec<i32> {
    if !whole_tree {
        return Vec::new();
    }
    let mut groups = vec![program];
    if let Some(foreground) = foreground
        && foreground != program
    {
        groups.push(foreground);
    }
    groups.retain(|&group| group > 1 && group != own);
    groups
}

fn send(life: &Life, program: i32, groups: &[i32], step: Escalation) {
    let signal = match step {
        Escalation::Terminate => Signal::SIGTERM,
        Escalation::Kill => Signal::SIGKILL,
    };
    tracing::debug!(?step, "the closed terminal's program is still running");
    if groups.is_empty() {
        if program > 1 {
            life.unless_exited(|| {
                let _ = kill(Pid::from_raw(program), signal);
            });
        }
        return;
    }
    for &group in groups {
        // A group that has ended already is no error.
        let _ = killpg(Pid::from_raw(group), signal);
    }
}

#[cfg(test)]
#[path = "hangup_tests.rs"]
mod tests;
