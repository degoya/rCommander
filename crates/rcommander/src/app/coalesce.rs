//! The window's job queue as the dispatch takes it (RC-110-67): every job that is waiting is taken
//! at once, and a run of the same cursor step for the same panel among them goes to the engine as
//! one step. A held arrow key during a slow listing piled up its presses and replayed them once
//! the listing was in, the cursor running on for seconds; now it moves one step. A job that is no
//! single cursor step - a click's job, Enter, a selection - is never merged and keeps its place, so
//! the commands still reach the engine in the order the keys were pressed. Nothing waits for more
//! jobs: while the engine keeps up, nothing piles up and every press is a step.

use std::collections::VecDeque;

use rc_engine::{Command, PanelId};
use tokio::sync::mpsc::UnboundedReceiver;

use super::Job;

/// The next job to run: the jobs `receiver` holds are taken into `waiting` first, merged as they
/// come; `None` once the queue is closed and nothing waits.
pub async fn next(
    receiver: &mut UnboundedReceiver<Job>,
    waiting: &mut VecDeque<Job>,
) -> Option<Job> {
    if waiting.is_empty() {
        let job = receiver.recv().await?;
        waiting.push_back(job);
    }
    while let Ok(job) = receiver.try_recv() {
        push(waiting, job);
    }
    waiting.pop_front()
}

/// `job` queued behind `waiting`: dropped when it is the same cursor step for the same panel as
/// the job before it.
pub fn push(waiting: &mut VecDeque<Job>, job: Job) {
    let step = cursor_step(&job);
    if step.is_some() && waiting.back().and_then(cursor_step) == step {
        return;
    }
    waiting.push_back(job);
}

/// The panel and the command of a job that is one `MoveCursor` and nothing else.
fn cursor_step(job: &Job) -> Option<(PanelId, &Command)> {
    match job.commands.as_slice() {
        [command @ Command::MoveCursor { .. }] => Some((job.panel, command)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "coalesce_tests.rs"]
mod tests;
