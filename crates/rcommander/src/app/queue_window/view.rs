//! Drawing the queue window: a large dialog over the window (`theme::large_dialog`) with a
//! one-line row per job - kind, source and target, lane (or "Runs at once"), state, progress -, the marked rows in the
//! theme's selection, a running job marked by an accent stripe and its state in the accent, its
//! file under its path and its bytes, speed and time left at the right edge (RC-010-26), the
//! context menu over the list (`menu_view.rs`), the cursor job's counts or how many jobs are
//! marked under the list with the running jobs' speed together beside them, the keys, and one row
//! of buttons: the selection's at the left, the whole queue's - one pause or resume button - and
//! Close at the right.

use iced::{
    Element, Length,
    alignment::Horizontal,
    widget::{Column, Space, column, container, mouse_area, progress_bar, row, scrollable, stack},
};
use rc_engine::{JobKind, JobState, QueueEntry, Reorder, VfsPath, drive_of};

use super::{
    AllToggle, QueueMsg, QueueWindow,
    figures::{detail, live, running_together, share, single},
    rows,
};
use crate::{
    app::{App, Message},
    format,
    i18n::t,
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
        icon::Icon,
        layout::{
            self, button_row, primary_button, primary_icon_button, secondary_button,
            secondary_icon_button,
        },
    },
};

/// The progress column: the bar, then the percentage.
const BAR_LENGTH: f32 = 110.0;
const PERCENT_WIDTH: f32 = 44.0;
/// The accent stripe at the left of a running job's row.
const RUNNING_STRIPE: f32 = 3.0;

fn msg(message: QueueMsg) -> Message {
    Message::Queue(message)
}

impl App {
    /// `base` with the queue window over it, while it is open.
    pub(crate) fn queue_layer<'a>(
        &'a self,
        base: Element<'a, Message>,
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        // In a window of its own it draws there (`queue_window_view`).
        let Some(window) = self
            .queue_window
            .as_ref()
            .filter(|_| !self.dialog_windows.queue_own)
        else {
            return base;
        };
        let head = Message::titled_head(self.queue_title());
        // A large dialog: the list gets the window's room.
        let layer = theme::large_dialog(head, self.queue_card(window, tokens), tokens);
        stack![base, layer].into()
    }

    /// The queue's own window (RC-010-14): the same card, the system's title bar its head.
    pub(crate) fn queue_window_view(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        match &self.queue_window {
            Some(window) => layout::window_body(self.queue_card(window, tokens), tokens),
            None => Space::new().into(),
        }
    }

    /// The window's title: the list shown and how many jobs it holds.
    pub(crate) fn queue_title(&self) -> String {
        self.queue_window
            .as_ref()
            .map_or_else(String::new, |window| {
                t!(
                    "queue.title",
                    name = super::lists_view::shown_name(window),
                    count = window.entries.len()
                )
            })
    }

    fn queue_card<'a>(
        &'a self,
        window: &'a QueueWindow,
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        let lanes: Vec<String> = window
            .entries
            .iter()
            .map(|entry| self.entry_lane(entry))
            .collect();
        // One lane for every job says nothing: its room goes to the paths (RC-010-26).
        let one_lane = self.file_ops.hide_single_lane && single(&lanes);
        // Only the rows around the offset are built; the others keep their room (RC-080-45).
        let heights: Vec<f32> = window
            .entries
            .iter()
            .map(|entry| rows::height(&entry.job))
            .collect();
        let shown = rows::shown(&heights, window.scrolled);
        let mut built = Column::new().spacing(rows::SPACING);
        let at = shown.rows.start;
        let chosen = window.entries[shown.rows.clone()]
            .iter()
            .zip(lanes.into_iter().skip(at));
        for (index, (entry, lane)) in chosen.enumerate() {
            let cursor = window.selected == Some(entry.job.id);
            let marked = window.marked.contains(&entry.job.id);
            let alternate = theme::alternate(at + index);
            let lane = (!one_lane).then_some(lane);
            built = built.push(self.entry_row(entry, lane, (cursor, marked, alternate), tokens));
        }
        let listed = column![
            Space::new().height(shown.above),
            built,
            Space::new().height(shown.below),
        ];
        let list: Element<'a, Message> = if window.entries.is_empty() {
            container(line(t!("queue.empty")).color(tokens.text_secondary))
                .height(Length::Fill)
                .into()
        } else {
            // The jobs are one stop of the ring; the window moves the selection itself.
            let jobs = focus::choice(
                scrollable(listed)
                    .height(Length::Fill)
                    .on_scroll(|viewport| msg(QueueMsg::Scrolled(viewport.absolute_offset().y))),
                "queue.jobs",
                None,
                None,
            );
            super::menu_view::with_menu(jobs, window, tokens)
        };
        let selected = window.selected.and_then(|job| window.entry(job));
        // The cursor job's counts and speed, or how many are marked: the rows stay one line.
        let detail = if window.marked.is_empty() {
            selected
                .and_then(|entry| detail(&entry.job))
                .unwrap_or_default()
        } else {
            t!("queue.marked", count = window.marked.len())
        };
        // Several jobs running: their speed together at the right (RC-010-26).
        let together = running_together(&window.entries)
            .filter(|_| !self.file_ops.hide_total_speed)
            .map(|(count, speed)| {
                t!(
                    "queue.running_together",
                    count = count,
                    speed = format::size(speed)
                )
            })
            .unwrap_or_default();
        let mut body = column![
            super::lists_view::lists_bar(window, tokens),
            list,
            row![
                fit::prose(detail)
                    .color(tokens.text_secondary)
                    .width(Length::Fill),
                fit::prose(together).color(tokens.text_secondary),
            ]
            .spacing(16),
        ]
        .spacing(10);
        if let Some(confirm) = &window.confirm {
            let (question, yes, no) = confirm.texts();
            // Enter says yes.
            body = body.push(button_row(
                Some(primary_icon_button(
                    Icon::Close,
                    yes,
                    Some(msg(QueueMsg::Confirm)),
                )),
                [secondary_button(no, Some(msg(QueueMsg::Keep)))],
                [Element::from(fit::prose(question).color(theme::DANGER))],
            ));
        }
        body.push(fit::help_line(t!("queue.keys"), tokens))
            .push(self.queue_buttons(window, selected))
            .into()
    }

    /// A job's lane as its column shows it. A job run at once waits for no lane: the column
    /// marks its group (RC-010-25).
    fn entry_lane(&self, entry: &QueueEntry) -> String {
        if entry.at_once {
            return t!("queue.at_once").to_owned();
        }
        entry
            .job
            .kind
            .lane()
            .map(|path| self.lane_label(path))
            .unwrap_or_default()
    }

    /// One job's row; `lane` is `None` while the window leaves its column out.
    fn entry_row<'a>(
        &'a self,
        entry: &'a QueueEntry,
        lane: Option<String>,
        (cursor, marked, alternate): (bool, bool, bool),
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        let job = &entry.job;
        let share = share(job);
        let lane_shown = lane.is_some();
        let state = match entry.place {
            // Put back after a restart, or paused once its connection retries were spent.
            _ if job.resumable => t!("queue.state.interrupted").to_owned(),
            Some(place) => t!("queue.waiting_place", place = place + 1),
            None => state_label(job.state).to_owned(),
        };
        let running = job.state == JobState::Running;
        // A running job's file under its path, its bytes, speed and time left at the row's right
        // edge, as they come in: the same columns as the row above, the figures laid over them.
        let live = live(job).map(|(file, progress)| {
            let mut cells: Vec<Element<'a, Message>> = vec![
                Space::new().width(Length::FillPortion(2)).into(),
                fit::path_cell(file, 8, true, tokens),
            ];
            if lane_shown {
                cells.push(Space::new().width(Length::FillPortion(3)).into());
            }
            cells.extend([
                Space::new().width(Length::FillPortion(3)).into(),
                Space::new().width(BAR_LENGTH).into(),
                Space::new().width(PERCENT_WIDTH).into(),
            ]);
            let columns = fit::table_row(false, tokens, cells);
            let figures = container(line(progress).color(tokens.text_secondary))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Right)
                .align_y(iced::Alignment::Center);
            stack![columns, figures]
        });
        let state_color = if running { tokens.accent } else { tokens.text };
        let mut cells = vec![
            fit::cell(kind_label(&job.kind), 2, false, tokens),
            fit::path_cell(route(&job.kind), 8, false, tokens),
        ];
        cells.extend(lane.map(|lane| fit::path_cell(lane, 3, true, tokens)));
        cells.extend([
            fit::cut(state, theme::UI_FONT, Some(state_color), 3, tokens),
            progress_bar(0.0..=1.0, share)
                .length(BAR_LENGTH)
                .girth(6)
                .into(),
            line(format::percent(share)).width(PERCENT_WIDTH).into(),
        ]);
        // Every row keeps the stripe's room, so the columns stay in line.
        let stripe = container(Space::new())
            .width(RUNNING_STRIPE)
            .height(Length::Fill)
            .style(move |_| {
                container::Style::default().background(if running {
                    tokens.accent
                } else {
                    iced::Color::TRANSPARENT
                })
            });
        let content = Column::new()
            .push(fit::table_row(false, tokens, cells))
            .extend(live.map(Element::from));
        mouse_area(
            // The row's frame below carries the selection and the stripe.
            container(row![stripe, content].spacing(3).height(Length::Shrink))
                .padding([0, 6])
                .width(Length::Fill)
                .style(move |_| theme::striped_row(tokens, cursor, true, marked, alternate)),
        )
        .on_press(msg(QueueMsg::Select(job.id)))
        .on_right_press(msg(QueueMsg::RowMenu(job.id)))
        .into()
    }

    /// One row: the selection's buttons at the left (disabled where they do not apply to it) and
    /// "Move to list", then Answer (Enter, while the cursor's job asks), the whole queue's buttons
    /// and Close. Pause and Resume take the cursor's job.
    fn queue_buttons<'a>(
        &'a self,
        window: &'a QueueWindow,
        selected: Option<&'a QueueEntry>,
    ) -> Element<'a, Message> {
        let chosen = window.chosen_entries();
        let waiting = chosen.iter().any(|entry| entry.place.is_some());
        let live = selected.filter(|entry| !entry.job.state.is_finished());
        let pause = if live.is_some_and(|entry| entry.job.state == JobState::Paused) {
            secondary_icon_button(
                Icon::Resume,
                t!("queue.resume"),
                live.map(|entry| msg(QueueMsg::Resume(entry.job.id))),
            )
        } else {
            secondary_icon_button(
                Icon::Pause,
                t!("queue.pause"),
                live.map(|entry| msg(QueueMsg::Pause(entry.job.id))),
            )
        };
        // Waiting jobs leave the queue; started ones are cancelled after a yes.
        let remove = match chosen[..] {
            [one] if super::selection::started(one) => t!("ops.cancel"),
            _ => t!("queue.remove"),
        };
        let reorder = |label: &'static str, step: Reorder| {
            secondary_button(label, waiting.then(|| msg(QueueMsg::Move(step))))
        };
        let mut job = vec![
            reorder(t!("queue.up"), Reorder::Up),
            reorder(t!("queue.down"), Reorder::Down),
            reorder(t!("queue.front"), Reorder::Front),
            pause,
            secondary_button(remove, (!chosen.is_empty()).then(|| msg(QueueMsg::Remove))),
        ];
        job.extend(super::lists_view::move_to(window));
        let answer = selected
            .filter(|entry| {
                entry.job.question.is_some() && self.file_ops.jobs.get(entry.job.id).is_some()
            })
            .map(|entry| {
                primary_button(
                    t!("queue.answer"),
                    Some(msg(QueueMsg::Answer(entry.job.id))),
                )
            });
        let (icon, label, all) = match window.all {
            AllToggle::Resume => (
                Icon::Resume,
                t!("queue.resume_all"),
                Some(msg(QueueMsg::ResumeAll)),
            ),
            AllToggle::Pause => (
                Icon::Pause,
                t!("queue.pause_all"),
                Some(msg(QueueMsg::PauseAll)),
            ),
        };
        let queue = [
            secondary_icon_button(icon, label, all),
            secondary_button(
                t!("queue.clear_finished"),
                Some(msg(QueueMsg::ClearFinished)),
            ),
            secondary_icon_button(Icon::Close, t!("queue.close"), Some(msg(QueueMsg::Close))),
        ];
        button_row(answer, queue, job)
    }

    /// The lane a job writes to: the local file systems as one, a server or a share by its
    /// drive bar label.
    fn lane_label(&self, path: &VfsPath) -> String {
        if path.backend == rc_vfs::BackendId::Local {
            return t!("queue.lane_local").to_owned();
        }
        drive_of(&self.drive_bar.drives, path).map_or_else(
            || rc_engine::shown_path(path).display().to_string(),
            |drive| drive.label.clone(),
        )
    }
}

/// What a job does.
fn kind_label(kind: &JobKind) -> &'static str {
    match kind {
        JobKind::Copy { .. } | JobKind::CopyAs { .. } => t!("queue.kind.copy"),
        JobKind::Move { .. } | JobKind::MoveAs { .. } => t!("queue.kind.move"),
        JobKind::Delete { .. } => t!("queue.kind.delete"),
        JobKind::Mkdir { .. } => t!("queue.kind.mkdir"),
        JobKind::Rename { .. } => t!("queue.kind.rename"),
        JobKind::Extract { .. } => t!("queue.kind.extract"),
        JobKind::Pack { .. } => t!("queue.kind.pack"),
        _ => t!("queue.kind.other"),
    }
}

/// The first source, how many more, and the target: `a.txt and 2 more -> D:\out`.
fn route(kind: &JobKind) -> String {
    let (first, count, target) = match kind {
        JobKind::Copy { sources, target }
        | JobKind::Move { sources, target }
        | JobKind::Pack {
            sources, target, ..
        } => (sources.first(), sources.len(), Some(target)),
        JobKind::Extract {
            archives, target, ..
        } => (archives.first(), archives.len(), Some(target)),
        JobKind::Delete { sources, .. } => (sources.first(), sources.len(), None),
        JobKind::CopyAs { source, target } | JobKind::MoveAs { source, target } => {
            (Some(source), 1, Some(target))
        }
        JobKind::Rename { dir, .. } => (Some(dir), 1, None),
        _ => (None, 0, kind.lane()),
    };
    let shown = |path: &VfsPath| rc_engine::shown_path(path).display().to_string();
    let from = first.map(shown).unwrap_or_default();
    let from = if count > 1 {
        t!("queue.sources", first = from, more = count - 1)
    } else {
        from
    };
    match target {
        Some(target) if !from.is_empty() => t!("queue.route", from = from, to = shown(target)),
        Some(target) => shown(target),
        None => from,
    }
}

fn state_label(state: JobState) -> &'static str {
    match state {
        JobState::Queued => t!("queue.state.queued"),
        JobState::Counting => t!("queue.state.counting"),
        JobState::Running => t!("queue.state.running"),
        JobState::Paused => t!("ops.paused"),
        JobState::Waiting => t!("queue.state.asking"),
        JobState::Completed => t!("queue.state.completed"),
        JobState::Cancelled => t!("queue.state.cancelled"),
        JobState::Failed => t!("queue.state.failed"),
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
