//! The background bar above the function keys (RC-010-14; owner, 2026-10-19): how many jobs run
//! in the background and, opened, the first `queue.strip_rows` of them, each with its bar, Pause
//! and Cancel; the queue window has them all. One Show brings the first the window follows into
//! the progress window - every job of a batch shows the same operation there -, Pause all (or
//! Resume all) and Cancel all steer every job of the bar at once.
//!
//! The bar previews the queue window (RC-010-25, owner 2026-10-20): its jobs are the window's in
//! the window's order (`queue_window/shown.rs`) - every list's and the ones run at once -, the
//! job in the progress window aside, which is on screen already. "N more in the queue" counts
//! the rest; with `queue.show_at_once` off the window leaves out the jobs run at once, which the
//! bar still shows, so the line says "in the background" then.
//!
//! The settings of the file operations are read here too: the function-key bar, F7's name, the
//! bar's rows and whether the queue window lists the jobs run at once.

use iced::{
    Alignment, Element, Length, Task,
    widget::{Column, Space, container, mouse_area, progress_bar, row},
};
use rc_engine::{
    Command, JobId, JobState, QueueEntry,
    settings::{
        FKEY_BAR_KEY, HIDE_SINGLE_LANE_KEY, MKDIR_PREFILL_KEY, SHOW_AT_ONCE_KEY, STRIP_ROWS_KEY,
        TOTAL_SPEED_KEY, stored_bool, stored_setting,
    },
};

use super::{
    FileOpsMsg,
    jobs::{self, Tracked, share_of},
    view::{action, kind_title},
    wrap,
};
use crate::{
    app::{App, Effect, Message},
    i18n::t,
    theme::{self, Tokens, fit},
};

/// How many jobs the bar lists while the setting is not read.
pub const DEFAULT_ROWS: usize = 5;

/// The settings the file operations read, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    /// `appearance.function_key_bar`: the function-key bar shows.
    pub bar: bool,
    /// `mkdir.prefill_cursor_name`: F7 offers the name under the cursor.
    pub prefill: bool,
    /// `queue.strip_rows`.
    pub rows: usize,
    /// `queue.show_at_once`: the queue window lists the jobs run at once.
    pub at_once: bool,
    /// `queue.total_speed`: the queue window shows its running jobs' speed together.
    pub total_speed: bool,
    /// `queue.hide_single_lane`: the queue window leaves out a lane column all its jobs share.
    pub hide_single_lane: bool,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            bar: true,
            prefill: true,
            rows: DEFAULT_ROWS,
            at_once: true,
            total_speed: true,
            hide_single_lane: true,
        }
    }
}

/// Whether the setting `key` is one of [`Stored`]'s.
pub fn concerns(key: &str) -> bool {
    [
        FKEY_BAR_KEY,
        MKDIR_PREFILL_KEY,
        STRIP_ROWS_KEY,
        SHOW_AT_ONCE_KEY,
        TOTAL_SPEED_KEY,
        HIDE_SINGLE_LANE_KEY,
    ]
    .contains(&key)
}

/// Every job of the bar at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum All {
    Pause,
    Resume,
    Cancel,
}

impl App {
    /// The jobs of the bar, in the queue window's order: every unfinished one but the job in the
    /// progress window.
    pub(super) fn background(&self) -> Vec<&QueueEntry> {
        let shown = self.file_ops.jobs.foreground().map(Tracked::id);
        self.file_ops
            .bar
            .iter()
            .filter(|entry| Some(entry.job.id) != shown)
            .collect()
    }

    /// A job's progress: its row in the bar and in the open queue window take the queue's state
    /// and progress of it - one row patched, no copy of the whole queue nor of the job's
    /// sources and failures (RC-080-45).
    pub(super) fn progress_rows(&mut self, job: JobId) {
        let in_bar = self.file_ops.bar.iter_mut().find(|e| e.job.id == job);
        let in_window = self
            .queue_window
            .as_mut()
            .and_then(|window| window.entries.iter_mut().find(|e| e.job.id == job));
        if in_bar.is_none() && in_window.is_none() {
            return;
        }
        let Ok((state, progress)) = self.engine.ops().job_progress(job) else {
            return;
        };
        for entry in in_bar.into_iter().chain(in_window) {
            entry.job.state = state;
            entry.job.progress = progress.clone();
        }
    }

    /// Pause all, Resume all or Cancel all: every job of the bar, the last first, so a job
    /// behind one that stops does not start in between.
    pub(super) fn steer_background(&self, all: All) -> Vec<Effect> {
        let commands: Vec<Command> = self
            .background()
            .into_iter()
            .filter(|entry| {
                let paused = entry.job.state == JobState::Paused;
                match all {
                    All::Pause => !paused,
                    All::Resume => paused,
                    All::Cancel => true,
                }
            })
            .map(|entry| entry.job.id)
            .rev()
            .map(|job| match all {
                All::Pause => Command::OpsPause { job },
                All::Resume => Command::OpsResume { job },
                All::Cancel => Command::OpsCancel { job },
            })
            .collect();
        if commands.is_empty() {
            return Vec::new();
        }
        vec![Effect::Dispatch(self.active, commands)]
    }

    /// Reads the function-key bar, whether F7 offers the name under the cursor, how many jobs
    /// the background bar lists, whether the queue window lists the jobs run at once, whether it
    /// shows their speed together and whether it leaves out a lane column all its jobs share.
    pub(in crate::app) fn file_ops_settings_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                // What is not stored keeps the default (RC-080-53: one place for it).
                let mut stored = Stored::default();
                let switches = [
                    (&mut stored.bar, FKEY_BAR_KEY),
                    (&mut stored.prefill, MKDIR_PREFILL_KEY),
                    (&mut stored.at_once, SHOW_AT_ONCE_KEY),
                    (&mut stored.total_speed, TOTAL_SPEED_KEY),
                    (&mut stored.hide_single_lane, HIDE_SINGLE_LANE_KEY),
                ];
                for (value, key) in switches {
                    *value = stored_bool(&db, key).await;
                }
                if let Some(rows) = stored_setting(&db, STRIP_ROWS_KEY)
                    .await
                    .and_then(|value| value.as_u64())
                    .and_then(|rows| usize::try_from(rows).ok())
                {
                    stored.rows = rows;
                }
                stored
            },
            |stored| wrap(FileOpsMsg::Settings(stored)),
        )
    }
}

/// What the opened bar lists of `jobs`: its first `rows`, and how many more there are.
pub(super) fn preview<'a, 'b>(
    jobs: &'b [&'a QueueEntry],
    rows: usize,
) -> (&'b [&'a QueueEntry], usize) {
    let listed = &jobs[..rows.min(jobs.len())];
    (listed, jobs.len() - listed.len())
}

/// The line under the rows for `more` jobs not listed: in the queue window's queue, or - while
/// the window leaves out the jobs run at once (`at_once_hidden`) - in the background.
pub(super) fn more_line(more: usize, at_once_hidden: bool) -> Option<String> {
    match (more, at_once_hidden) {
        (0, _) => None,
        (more, false) => Some(t!("ops.more_in_queue", count = more)),
        (more, true) => Some(t!("ops.more_in_background", count = more)),
    }
}

/// How the bar is drawn: opened, its rows, the job Show brings forward, and whether the queue
/// window leaves out the jobs run at once.
pub(super) struct Look {
    pub expanded: bool,
    pub rows: usize,
    pub show: Option<JobId>,
    pub at_once_hidden: bool,
}

/// The background jobs: how many and the buttons for all of them, and on a click the first
/// `rows` each with its bar and its buttons.
pub(super) fn background_bar<'a>(
    jobs: &[&'a QueueEntry],
    look: &Look,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let marker = if look.expanded {
        "\u{25be}"
    } else {
        "\u{25b8}"
    };
    let header = mouse_area(
        row![
            fit::line(marker).color(tokens.accent),
            fit::line(t!("ops.background", count = jobs.len())).color(tokens.text_secondary),
        ]
        .spacing(6),
    )
    .on_press(wrap(FileOpsMsg::Expand(!look.expanded)));
    let pause = if jobs.iter().all(|entry| entry.job.state == JobState::Paused) {
        action(t!("ops.resume_all"), FileOpsMsg::All(All::Resume), tokens)
    } else {
        action(t!("ops.pause_all"), FileOpsMsg::All(All::Pause), tokens)
    };
    let mut header = row![header, Space::new().width(Length::Fill), pause];
    header = header.push(action(
        t!("ops.cancel_all"),
        FileOpsMsg::All(All::Cancel),
        tokens,
    ));
    if let Some(first) = look.show {
        header = header.push(action(
            t!("ops.show"),
            FileOpsMsg::Foreground(first),
            tokens,
        ));
    }
    // The queue window, with every job and its order (RC-010-14).
    let queue = action(
        t!("ops.queue_window"),
        FileOpsMsg::Key(Box::new(Command::QueueWindow {})),
        tokens,
    );
    let header = header.push(queue).spacing(8).align_y(Alignment::Center);
    let mut lines = Column::new().spacing(4).push(header);
    if look.expanded {
        let (listed, more) = preview(jobs, look.rows);
        for &entry in listed {
            lines = lines.push(background_row(entry, tokens));
        }
        if let Some(more) = more_line(more, look.at_once_hidden) {
            lines = lines.push(fit::line(more).color(tokens.text_secondary));
        }
    }
    container(lines)
        .padding([4, 12])
        .width(Length::Fill)
        .style(move |_| theme::panel(tokens, false))
        .into()
}

fn background_row<'a>(entry: &'a QueueEntry, tokens: &'static Tokens) -> Element<'a, Message> {
    let job = &entry.job;
    let id = job.id;
    let what = jobs::sources_of(&job.kind)
        .first()
        .and_then(|source| source.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let pause = if job.state == JobState::Paused {
        action(t!("ops.resume"), FileOpsMsg::Resume(id), tokens)
    } else {
        action(t!("ops.pause"), FileOpsMsg::Pause(id), tokens)
    };
    let share = share_of(job);
    row![
        fit::line(kind_title(&job.kind)),
        fit::path_line(what, tokens.text_secondary, tokens),
        progress_bar(0.0..=1.0, share).length(160).girth(6),
        fit::line(crate::format::percent(share)),
        pause,
        action(t!("ops.cancel"), FileOpsMsg::Abort(id), tokens),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
#[path = "strip_tests.rs"]
mod tests;
