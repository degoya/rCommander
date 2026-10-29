//! Drawing the file operations: the dialogs, the conflict and error questions and the progress
//! window over the window (one layer, the dialog host's look), and Shift+F6's editor in its row;
//! the background bar above the function keys is `strip.rs`.

use iced::{
    Alignment, Element, Length,
    widget::{
        Column, column, container, mouse_area, pick_list, progress_bar, radio, row, stack,
        text_input,
    },
};
use rc_engine::{FileDialog, JobKind, PanelId};

use super::{
    Dialog, FIELD_ID, FileOpsMsg, Preset, RENAME_ID,
    jobs::{Timing, Totals, Tracked},
    question::Card,
    strip, wrap,
};
use crate::{
    app::{App, Message},
    format,
    i18n::{error_text, t},
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

impl App {
    /// `base` with the file operations' card over it: a question first, else the open dialog,
    /// else the progress window; none while they have windows of their own (RC-080-23).
    pub(crate) fn file_ops_layer<'a>(
        &'a self,
        base: Element<'a, Message>,
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        if self.dialog_windows.own {
            return base;
        }
        let ops = &self.file_ops;
        let ((title, card), width) = if let Some(job) = ops.jobs.asking() {
            (
                super::question::card(job, tokens),
                layout::MEDIUM_DIALOG_WIDTH,
            )
        } else if let Some(dialog) = &ops.dialog {
            (dialog_card(dialog, tokens), dialog_width(dialog))
        } else if let Some(job) = ops.jobs.foreground() {
            let totals = ops.jobs.totals(job.id());
            let timing = ops.jobs.timing(job.id(), std::time::Instant::now());
            (
                progress_card(job, (totals, timing), tokens),
                layout::MEDIUM_DIALOG_WIDTH,
            )
        } else {
            return base;
        };
        let head = Message::dialog_head(title);
        stack![base, layout::dialog_card(head, card, width, tokens)].into()
    }

    /// Above the status bar: the jobs in the background, then the function keys.
    pub(crate) fn file_ops_bottom(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        let mut parts = Column::new().spacing(4);
        let background = self.background();
        if !background.is_empty() {
            let jobs = &self.file_ops.jobs;
            let look = strip::Look {
                expanded: self.file_ops.expanded,
                rows: self.file_ops.strip_rows.unwrap_or(strip::DEFAULT_ROWS),
                // Show brings forward a job the window follows; an agent's has no window.
                show: background
                    .iter()
                    .map(|entry| entry.job.id)
                    .find(|id| jobs.get(*id).is_some()),
                at_once_hidden: self.file_ops.hide_at_once,
            };
            parts = parts.push(strip::background_bar(&background, &look, tokens));
        }
        if !self.file_ops.hide_bar {
            parts = parts.push(self.fkey_bar(tokens));
        }
        parts.into()
    }

    /// Shift+F6's editor in place of row `row` of `side`, while it renames that entry.
    pub(crate) fn rename_editor(
        &self,
        side: PanelId,
        row: usize,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let rename = self.file_ops.rename.as_ref()?;
        let index = self.pane(side).entry_index(row)?;
        if rename.panel != side || rename.index != index {
            return None;
        }
        let field = text_input("", &rename.text)
            .id(RENAME_ID)
            .on_input(|text| wrap(FileOpsMsg::RenameInput(text)))
            .on_submit(wrap(FileOpsMsg::RenameSubmit))
            .size(theme::TEXT_SIZE)
            .font(theme::UI_FONT)
            .padding([1, 4]);
        Some(
            container(field)
                .height(theme::ROW_HEIGHT)
                .width(Length::Fill)
                .align_y(Alignment::Center)
                .padding([0, 6])
                .style(move |_| theme::row(tokens, true, true, false))
                .into(),
        )
    }
}

/// A copy or a move is a form (medium); creating or deleting asks a question (small).
fn dialog_width(dialog: &Dialog) -> f32 {
    // A delete has three buttons and F2's list too.
    if dialog.queues() {
        layout::MEDIUM_DIALOG_WIDTH
    } else {
        layout::SMALL_DIALOG_WIDTH
    }
}

pub(super) fn dialog_card<'a>(dialog: &'a Dialog, tokens: &'static Tokens) -> Card<'a> {
    let name = dialog.single.clone().unwrap_or_default();
    let count = dialog.count;
    let title = match (dialog.kind, dialog.single.is_some()) {
        (FileDialog::Copy, true) => t!("ops.copy_one", name = name),
        (FileDialog::Copy, false) => t!("ops.copy_many", count = count),
        (FileDialog::CopyHere, _) => t!("ops.copy_here", name = name),
        (FileDialog::Move, true) => t!("ops.move_one", name = name),
        (FileDialog::Move, false) => t!("ops.move_many", count = count),
        (FileDialog::Mkdir, _) => t!("ops.mkdir").to_owned(),
        (FileDialog::NewText, _) => t!("ops.new_text").to_owned(),
        (FileDialog::Delete, true) => t!("ops.delete_one", name = name),
        (FileDialog::Delete, false) => t!("ops.delete_many", count = count),
        (FileDialog::DeletePermanent, true) => t!("ops.delete_one_permanent", name = name),
        (FileDialog::DeletePermanent, false) => t!("ops.delete_many_permanent", count = count),
        (FileDialog::Rename, _) => String::new(),
    };
    // The title names the file: it wraps rather than leave the card.
    let title = fit::prose(title).font(theme::UI_FONT_STRONG);
    let mut body = column![].spacing(10);
    if dialog.has_field() {
        body = body.push(
            text_input("", &dialog.text)
                .id(FIELD_ID)
                .on_input(|text| wrap(FileOpsMsg::Input(text)))
                .on_submit(wrap(FileOpsMsg::Start { background: false }))
                .font(theme::UI_FONT)
                .size(theme::TEXT_SIZE)
                .padding([6, 8]),
        );
    }
    match dialog.kind {
        FileDialog::DeletePermanent => {
            body = body.push(fit::prose(t!("ops.delete_permanent_warning")).color(theme::DANGER));
        }
        FileDialog::Mkdir => {
            body = body.push(fit::prose(t!("ops.mkdir_hint")).color(tokens.text_secondary));
        }
        _ => {}
    }
    if dialog.transfers() {
        let presets = Preset::ALL.into_iter().map(|preset| {
            radio(preset_label(preset), preset, Some(dialog.preset), |p| {
                wrap(FileOpsMsg::Preset(p))
            })
            .size(14)
            .text_size(theme::TEXT_SIZE)
            .into()
        });
        // The presets are one stop of the ring; the arrows choose (RC-080-16).
        let preset = t!("ops.preset");
        let presets = layout::keyed(
            focus::Stop::pick(
                row(presets).spacing(12).wrap(),
                preset,
                focus::moves(&Preset::ALL, Some(&dialog.preset), &|preset| {
                    wrap(FileOpsMsg::Preset(preset))
                }),
            ),
            preset,
        );
        body = body
            .push(layout::caption(preset, Some(tokens.text_secondary)))
            .push(presets)
            .push(layout::check(t!("ops.verify"), dialog.verify, true, |on| {
                wrap(FileOpsMsg::Verify(on))
            }))
            .push(layout::check(
                t!("ops.keep_attributes"),
                dialog.keep_attributes,
                true,
                |on| wrap(FileOpsMsg::KeepAttributes(on)),
            ));
    }
    // F2's queue list, once there is more than the default one (RC-010-15); a delete has it too.
    if dialog.queues() && dialog.lists.len() > 1 {
        let choices = crate::app::queue_window::list_choices(&dialog.lists);
        let chosen = choices
            .iter()
            .find(|choice| choice.id == dialog.queue)
            .cloned();
        let choose =
            |choice: crate::app::queue_window::ListChoice| wrap(FileOpsMsg::QueueList(choice.id));
        let moves = focus::moves(&choices, chosen.as_ref(), &choose);
        let list = pick_list(choices, chosen, choose)
            .text_size(theme::TEXT_SIZE)
            .font(theme::UI_FONT)
            .width(Length::Fill);
        body = body.push(
            row![
                fit::line(t!("ops.queue_list")),
                focus::Stop::pick(list, t!("ops.queue_list"), moves),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    if let Some(code) = dialog.error {
        body = body.push(fit::prose(error_text(code)).color(theme::DANGER));
    }
    // TC's order: OK (Enter), F2 Queue, Cancel.
    let mut secondary = Vec::new();
    if dialog.queues() {
        secondary.push(layout::secondary_icon_button(
            Icon::Queue,
            t!("ops.queue"),
            Some(wrap(FileOpsMsg::Start { background: true })),
        ));
    }
    secondary.push(layout::secondary_icon_button(
        Icon::Close,
        t!("ops.cancel"),
        Some(wrap(FileOpsMsg::Close)),
    ));
    let ok = layout::primary_icon_button(
        ok_icon(dialog.kind),
        t!("ops.ok"),
        Some(wrap(FileOpsMsg::Start { background: false })),
    );
    let body = body.push(layout::button_row(Some(ok), secondary, []));
    (title.into(), body.into())
}

/// The icon of a file dialog's OK: its action.
fn ok_icon(kind: FileDialog) -> Icon {
    match kind {
        FileDialog::Copy | FileDialog::CopyHere => Icon::Copy,
        FileDialog::Move => Icon::Move,
        FileDialog::Mkdir => Icon::NewFolder,
        FileDialog::NewText => Icon::Add,
        FileDialog::Delete | FileDialog::DeletePermanent => Icon::Delete,
        FileDialog::Rename => Icon::Rename,
    }
}

fn preset_label(preset: Preset) -> &'static str {
    match preset {
        Preset::Ask => t!("ops.preset.ask"),
        Preset::Overwrite => t!("ops.preset.overwrite"),
        Preset::Skip => t!("ops.preset.skip"),
        Preset::OverwriteOlder => t!("ops.preset.older"),
        Preset::Rename => t!("ops.preset.rename"),
    }
}

/// What a job does, for its title.
pub(super) fn kind_title(kind: &JobKind) -> &'static str {
    match kind {
        JobKind::Copy { .. } | JobKind::CopyAs { .. } => t!("ops.title.copy"),
        JobKind::Move { .. } | JobKind::MoveAs { .. } => t!("ops.title.move"),
        JobKind::Delete { .. } => t!("ops.title.delete"),
        JobKind::ChecksumCreate { .. } => t!("ops.title.checksum_create"),
        JobKind::ChecksumVerify { .. } => t!("ops.title.checksum_verify"),
        JobKind::Split { .. } => t!("ops.title.split"),
        JobKind::Combine { .. } => t!("ops.title.combine"),
        JobKind::Pack { .. } => t!("ops.title.pack"),
        JobKind::Extract { .. } => t!("ops.title.extract"),
        _ => t!("ops.title.other"),
    }
}

/// The progress window as Total Commander's: the file at work with its bar, the total of the
/// whole operation - every job of its batch - with its own, its speed and times (RC-010-07).
pub(super) fn progress_card<'a>(
    job: &'a Tracked,
    (totals, timing): (Totals, Timing),
    tokens: &'static Tokens,
) -> Card<'a> {
    let id = job.id();
    let progress = &job.snapshot.progress;
    let current = progress
        .current
        .as_ref()
        .map(|path| path.path.display().to_string())
        .unwrap_or_default();
    let counts = t!(
        "ops.progress_counts",
        files_done = format::count(totals.files_done),
        files = format::count(totals.files_total),
        bytes_done = format::size(totals.bytes_done),
        bytes = format::size(totals.bytes_total),
    );
    let file_bytes = t!(
        "ops.progress_file_bytes",
        done = format::size(progress.file_bytes_done),
        total = format::size(progress.file_bytes_total),
    );
    let share = totals.share();
    let speed = if job.paused() {
        t!("ops.paused").to_owned()
    } else {
        timing.speed_line()
    };
    let pause = if job.paused() {
        layout::secondary_icon_button(
            Icon::Resume,
            t!("ops.progress_resume"),
            Some(wrap(FileOpsMsg::Resume(id))),
        )
    } else {
        layout::secondary_icon_button(
            Icon::Pause,
            t!("ops.progress_pause"),
            Some(wrap(FileOpsMsg::Pause(id))),
        )
    };
    // No primary: Enter has no action in a progress window, Escape and F2 send it to the background.
    let buttons = layout::button_row(
        None,
        [
            pause,
            layout::secondary_button(
                t!("ops.to_background"),
                Some(wrap(FileOpsMsg::Background(id))),
            ),
            layout::secondary_icon_button(
                Icon::Close,
                t!("ops.cancel"),
                Some(wrap(FileOpsMsg::Abort(id))),
            ),
        ],
        [],
    );
    let title = fit::line(kind_title(&job.snapshot.kind)).font(theme::UI_FONT_STRONG);
    let body = column![
        // The card's full width, shortened in the middle (owner, 2026-10-18).
        fit::path_line(current, tokens.text_secondary, tokens),
        labelled_bar(
            t!("ops.progress_file"),
            file_bytes,
            job.file_share(),
            tokens
        ),
        labelled_bar(
            t!("ops.progress_total"),
            timing.total_figures(share),
            share,
            tokens
        ),
        row![
            fit::line(counts).width(Length::Fill),
            fit::line(speed).color(tokens.text_secondary)
        ]
        .spacing(12),
        buttons,
    ]
    .spacing(10);
    (title.into(), body.into())
}

/// One bar of the progress window under its label, its figures at the right.
fn labelled_bar<'a>(
    label: &'static str,
    figures: String,
    share: f32,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    column![
        row![
            fit::line(label).width(Length::Fill),
            fit::line(figures).color(tokens.text_secondary),
        ]
        .spacing(12),
        progress_bar(0.0..=1.0, share).girth(8),
    ]
    .spacing(6)
    .into()
}

/// Seconds as `1:05:09` or `5:09`.
pub(in crate::app) fn duration(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A button drawn like a function key.
pub(super) fn action<'a>(
    label: &'static str,
    message: FileOpsMsg,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    // focus-ring: pointer only (a button of the background bar, no dialog's; the queue window steers by keyboard)
    mouse_area(
        container(fit::line(label))
            .padding([5, 14])
            .style(move |_| theme::key_button(tokens)),
    )
    .on_press(wrap(message))
    .into()
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
