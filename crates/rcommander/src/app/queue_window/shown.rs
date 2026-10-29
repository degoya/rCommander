//! Which of the queue's jobs the queue window and the background bar under the panels show, in
//! one order (RC-010-25, owner 2026-10-20): the queue's own (`rc_ops::Queue::entries`), only
//! filtered here and never sorted again, so the bar's rows are the window's first ones.
//!
//! The window shows one list at a time: its jobs and, on the default list while
//! `queue.show_at_once` is on, the jobs run at once (F5, F6, F8 confirmed with OK) - a group of
//! their own ahead of the list's, the finished ones among the finished. A named list is the
//! person's own and shows its jobs alone. The bar shows every unfinished job of every list and
//! every job run at once, whatever the setting: off, a copy run at once would otherwise be shown
//! nowhere once its progress window went into the background.

use rc_engine::{DEFAULT_LIST, QueueEntry, QueueListId};

/// For whom the jobs are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// The window on the list `list`; `at_once` is `queue.show_at_once`.
    List { list: QueueListId, at_once: bool },
    /// The background bar: the unfinished jobs, of every list and run at once.
    Bar,
}

/// The entries of `entries` that `scope` shows, in their order.
pub fn shown(entries: &[QueueEntry], scope: Shown) -> impl Iterator<Item = &QueueEntry> {
    entries.iter().filter(move |entry| match scope {
        Shown::List { list, at_once } if entry.at_once => at_once && list == DEFAULT_LIST,
        Shown::List { list, .. } => entry.list == list,
        Shown::Bar => !entry.job.state.is_finished(),
    })
}

#[cfg(test)]
#[path = "shown_tests.rs"]
mod tests;
