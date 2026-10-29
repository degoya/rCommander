//! Drawing the synchronize-dirs dialog over the whole window: both folders, the form, the
//! filter, where the comparison stands, the rows on screen - name, both sides' size and date, the
//! action between them - the row of buttons, and the confirmation over it all.

use std::fmt;

use iced::{
    Alignment, Color, Element, Length,
    alignment::Horizontal,
    widget::{Column, column, container, mouse_area, pick_list, row, sensor, stack, text_input},
};
use rc_engine::sync::{
    ItemKind, PairState, RowFilter, SyncAction, SyncItem, SyncMode, SyncSummary,
};

use super::{
    CompareMsg, Confirm, Flag, Phase, SyncDialog,
    diff_view::{button, flag, path},
    wrap,
};
use crate::{
    app::Message,
    format,
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
        icon::Icon,
        layout,
    },
};

const SIZE_WIDTH: f32 = 90.0;
const DATE_WIDTH: f32 = 128.0;
const ACTION_WIDTH: f32 = 80.0;

/// The rows that fit a list of `height`.
pub fn rows_for(height: f32) -> usize {
    (height / theme::ROW_HEIGHT).floor().max(1.0) as usize
}

/// A mode for the pick list, with its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mode(SyncMode);

const MODES: [Mode; 5] = [
    Mode(SyncMode::Both),
    Mode(SyncMode::LeftToRight),
    Mode(SyncMode::RightToLeft),
    Mode(SyncMode::MirrorToRight),
    Mode(SyncMode::MirrorToLeft),
];

impl fmt::Display for Mode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            SyncMode::Both => t!("sync.mode_both"),
            SyncMode::LeftToRight => t!("sync.mode_left_to_right"),
            SyncMode::RightToLeft => t!("sync.mode_right_to_left"),
            SyncMode::MirrorToRight => t!("sync.mode_mirror_to_right"),
            SyncMode::MirrorToLeft => t!("sync.mode_mirror_to_left"),
        })
    }
}

/// The dialog over everything else, the confirmation over the dialog.
pub fn overlay<'a>(dialog: &'a SyncDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let head = Message::titled_head(t!("sync.title"));
    let shown = theme::large_dialog(head, body(dialog, tokens), tokens);
    match question(dialog, tokens) {
        Some(question) => stack![shown, question].into(),
        None => shown,
    }
}

/// The folders, the form, the rows and the buttons: the card's content over the window, the
/// whole of the compare window of its own (RC-080-15).
pub fn body<'a>(dialog: &'a SyncDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let folders = row![path(&dialog.left, tokens), path(&dialog.right, tokens)].spacing(8);
    column![
        folders,
        form(dialog),
        filters(dialog.filter, tokens),
        status(dialog, tokens),
        list(dialog, tokens),
        buttons(dialog),
    ]
    .spacing(10)
    .into()
}

/// The question before synchronizing, over the dialog while it is asked.
pub fn question<'a>(
    dialog: &'a SyncDialog,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    dialog
        .confirm
        .as_ref()
        .map(|confirm| confirmation(confirm, tokens))
}

fn form(dialog: &SyncDialog) -> Element<'_, Message> {
    let form = &dialog.form;
    let switch = |label: &'static str, on: bool, which: Flag| {
        layout::check(label, on, true, move |on| {
            wrap(CompareMsg::SyncFlag(which, on))
        })
    };
    let choose = |mode: Mode| wrap(CompareMsg::SyncMode(mode.0));
    let mode = pick_list(MODES, Some(Mode(form.mode)), choose)
        .text_size(theme::TEXT_SIZE)
        .width(230);
    let mode_label = t!("sync.mode");
    let mode = layout::keyed(
        focus::Stop::pick(
            mode,
            mode_label,
            focus::moves(&MODES, Some(&Mode(form.mode)), &choose),
        ),
        mode_label,
    );
    let (tolerance_label, masks_label) = (t!("sync.tolerance"), t!("sync.masks"));
    let tolerance = text_input("2", &form.tolerance)
        .on_input(|text| wrap(CompareMsg::SyncTolerance(text)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8])
        .width(60);
    let tolerance = layout::keyed(
        focus::Stop::field(tolerance, tolerance_label),
        tolerance_label,
    );
    let masks = text_input(t!("sync.masks_all"), &form.masks)
        .on_input(|text| wrap(CompareMsg::SyncMasks(text)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8]);
    let masks = layout::keyed(focus::Stop::field(masks, masks_label), masks_label);
    let tolerance_label = layout::caption(tolerance_label, dialog.invalid.then_some(theme::DANGER));
    column![
        row![
            layout::caption(mode_label, None),
            mode,
            switch(t!("sync.subdirs"), form.subdirs, Flag::Subdirs),
            switch(t!("sync.by_content"), form.by_content, Flag::ByContent),
            switch(t!("sync.ignore_date"), form.ignore_date, Flag::IgnoreDate),
            switch(
                t!("sync.ignore_dst_hour"),
                form.ignore_dst_hour,
                Flag::IgnoreDstHour
            ),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        row![
            tolerance_label,
            tolerance,
            layout::caption(masks_label, None),
            masks
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

/// Compare (Enter; Stop while it runs) as the primary, Synchronize and Close beside it.
fn buttons(dialog: &SyncDialog) -> Element<'_, Message> {
    let comparing = dialog.phase == Phase::Comparing;
    let compare = if comparing {
        layout::primary_icon_button(
            Icon::Stop,
            t!("sync.stop"),
            Some(wrap(CompareMsg::SyncStop)),
        )
    } else {
        layout::primary_button(t!("sync.compare"), Some(wrap(CompareMsg::SyncCompare)))
    };
    let can_run = dialog.plan.is_some() && !comparing && dialog.phase != Phase::Starting;
    layout::button_row(
        Some(compare),
        [
            button(
                t!("sync.synchronize"),
                can_run.then_some(CompareMsg::SyncAsk),
            ),
            layout::secondary_icon_button(
                Icon::Close,
                t!("sync.close"),
                Some(wrap(CompareMsg::SyncClose)),
            ),
        ],
        [],
    )
}

fn filters<'a>(current: RowFilter, tokens: &'static Tokens) -> Element<'a, Message> {
    let tab = |label: &'static str, filter: RowFilter| {
        let chosen = filter == current;
        let shown = container(line(label))
            .padding([4, 12])
            .style(move |_| theme::row(tokens, chosen, true, false));
        // focus-ring: pointer only (a tab of the filter pick stop below)
        mouse_area(shown).on_press(wrap(CompareMsg::SyncFilter(filter)))
    };
    let tabs = row![
        tab(t!("sync.show_differences"), RowFilter::Differences),
        tab(t!("sync.show_all"), RowFilter::All),
        tab(t!("sync.show_equal"), RowFilter::Equal),
        tab(t!("sync.show_left_only"), RowFilter::LeftOnly),
        tab(t!("sync.show_right_only"), RowFilter::RightOnly),
    ]
    .spacing(6);
    // The filters are one stop of the ring; the arrows choose (RC-080-16).
    focus::pick(
        tabs,
        "sync.filters",
        &[
            RowFilter::Differences,
            RowFilter::All,
            RowFilter::Equal,
            RowFilter::LeftOnly,
            RowFilter::RightOnly,
        ],
        Some(&current),
        |filter| wrap(CompareMsg::SyncFilter(filter)),
    )
}

/// Where the comparison stands; with a plan, what it found and what it would do.
fn status<'a>(dialog: &SyncDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let (message, color) = match (&dialog.phase, dialog.summary()) {
        _ if dialog.invalid => (t!("sync.invalid_tolerance").to_owned(), theme::DANGER),
        (Phase::Idle, _) => (t!("sync.idle").to_owned(), tokens.text_secondary),
        (Phase::Comparing, _) => (t!("sync.comparing").to_owned(), tokens.text_secondary),
        (Phase::Starting, _) => (t!("sync.starting").to_owned(), tokens.text_secondary),
        (Phase::Nothing, _) => (t!("sync.nothing").to_owned(), tokens.accent),
        (Phase::Failed(code), _) => (error_text(code), theme::DANGER),
        (Phase::Ready, Some(summary)) => (found(&summary, dialog), tokens.accent),
        (Phase::Ready, None) => (String::new(), tokens.accent),
    };
    fit::prose(message).color(color).into()
}

fn found(summary: &SyncSummary, dialog: &SyncDialog) -> String {
    let mut text = t!(
        "sync.found",
        equal = format::count(summary.equal),
        different = format::count(
            summary.left_newer + summary.right_newer + summary.different + summary.kind_mismatch
        ),
        left_only = format::count(summary.left_only),
        right_only = format::count(summary.right_only)
    );
    let skipped = dialog.plan.as_ref().map_or(0, |plan| plan.skipped);
    if skipped > 0 {
        text.push_str(&t!("sync.skipped", count = format::count(skipped)));
    }
    text
}

/// The header and the rows on screen; the wheel scrolls, the list reports its size.
fn list<'a>(dialog: &'a SyncDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let secondary = |label: &'static str| line(label).color(tokens.text_secondary);
    let header = row![
        secondary(t!("sync.column_name")).width(Length::Fill),
        secondary(t!("column.size"))
            .width(SIZE_WIDTH)
            .align_x(Horizontal::Right),
        secondary(t!("column.date")).width(DATE_WIDTH),
        secondary(t!("sync.column_action"))
            .width(ACTION_WIDTH)
            .align_x(Horizontal::Center),
        secondary(t!("column.date")).width(DATE_WIDTH),
        secondary(t!("column.size"))
            .width(SIZE_WIDTH)
            .align_x(Horizontal::Right),
    ]
    .spacing(8)
    .padding([0, 6]);
    let rows = dialog
        .visible()
        .filter_map(|at| list_row(dialog, at, tokens));
    let rows = container(Column::with_children(rows))
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true);
    let rows = mouse_area(rows).on_scroll(|delta| {
        wrap(CompareMsg::SyncScroll(theme::wheel::rows(
            delta,
            theme::ROW_HEIGHT,
        )))
    });
    let rows = sensor(rows)
        .on_show(|size| wrap(CompareMsg::SyncViewport(size)))
        .on_resize(|size| wrap(CompareMsg::SyncViewport(size)));
    // The rows are one stop of the ring; the dialog moves their cursor with the arrows.
    let rows = focus::choice(rows, "sync.rows", None, None);
    column![header, rows].spacing(4).height(Length::Fill).into()
}

fn list_row<'a>(
    dialog: &'a SyncDialog,
    at: usize,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let (_, shown) = dialog.row(at)?;
    let (symbol, color) = action(shown.action, shown.state, tokens);
    let symbol = container(line(symbol).color(color))
        .width(ACTION_WIDTH)
        .align_x(Horizontal::Center);
    // focus-ring: pointer only (a row's action; Space on the rows stop cycles it)
    let symbol = mouse_area(symbol).on_press(wrap(CompareMsg::SyncCycle(at)));
    let cells = row![
        fit::path_cell(shown.path.display().to_string(), 1, false, tokens),
        line(size(shown.left.as_ref()))
            .width(SIZE_WIDTH)
            .align_x(Horizontal::Right),
        line(date(shown.left.as_ref())).width(DATE_WIDTH),
        symbol,
        line(date(shown.right.as_ref())).width(DATE_WIDTH),
        line(size(shown.right.as_ref()))
            .width(SIZE_WIDTH)
            .align_x(Horizontal::Right),
    ]
    .spacing(8)
    .padding([0, 6])
    .align_y(Alignment::Center);
    let cursor = at == dialog.rows.cursor();
    let framed = container(cells)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, true, false, theme::alternate(at)));
    Some(
        // focus-ring: pointer only (a row of the rows stop)
        mouse_area(framed)
            .on_press(wrap(CompareMsg::SyncClick(at)))
            .on_double_click(wrap(CompareMsg::SyncOpen(at)))
            .into(),
    )
}

fn size(item: Option<&SyncItem>) -> String {
    match item {
        Some(item) if item.kind == ItemKind::Dir => t!("panel.dir_marker").to_owned(),
        Some(item) => format::size(item.size),
        None => String::new(),
    }
}

fn date(item: Option<&SyncItem>) -> String {
    item.map_or_else(String::new, |item| format::date(item.modified))
}

/// A row's action as drawn; without one, how the two sides compare.
fn action(action: SyncAction, state: PairState, tokens: &'static Tokens) -> (&'static str, Color) {
    match action {
        SyncAction::CopyToRight => (t!("sync.action_copy_right"), tokens.accent),
        SyncAction::CopyToLeft => (t!("sync.action_copy_left"), tokens.accent),
        SyncAction::DeleteRight => (t!("sync.action_delete_right"), theme::DANGER),
        SyncAction::DeleteLeft => (t!("sync.action_delete_left"), theme::DANGER),
        SyncAction::None => match state {
            PairState::Equal => (t!("sync.state_equal"), tokens.text_secondary),
            PairState::LeftOnly | PairState::RightOnly => ("", tokens.text_secondary),
            _ => (t!("sync.state_different"), tokens.text_secondary),
        },
    }
}

/// The question before the jobs start, on its card over the dialog.
fn confirmation<'a>(confirm: &'a Confirm, tokens: &'static Tokens) -> Element<'a, Message> {
    Message::overlay(
        t!("sync.confirm_title"),
        confirmation_body(confirm),
        layout::SMALL_DIALOG_WIDTH,
        tokens,
    )
}

/// The question before synchronizing while it is asked, filling a window of its own (RC-080-23,
/// `dialog_windows/questions.rs`).
pub fn confirm_body(dialog: &SyncDialog) -> Option<Element<'_, Message>> {
    dialog.confirm.as_ref().map(confirmation_body)
}

/// A switch of the question with its access key.
fn keyed_switch<'a>(label: &'static str, on: bool, which: Flag) -> Element<'a, Message> {
    layout::check(label, on, true, move |on| {
        wrap(CompareMsg::SyncFlag(which, on))
    })
}

/// What the question says and asks, and its buttons: on its card, or filling its window.
fn confirmation_body(confirm: &Confirm) -> Element<'_, Message> {
    let summary = &confirm.summary;
    let switch = |label: String, on: bool, which: Flag| {
        flag(label, on, move |on| wrap(CompareMsg::SyncFlag(which, on)))
    };
    let mut body = Column::new().spacing(10);
    let right = summary.copy_to_right;
    if right.files > 0 {
        body = body.push(fit::prose(t!(
            "sync.confirm_copy_right",
            files = format::count(right.files),
            size = format::size(right.bytes)
        )));
    }
    let left = summary.copy_to_left;
    if left.files > 0 {
        body = body.push(fit::prose(t!(
            "sync.confirm_copy_left",
            files = format::count(left.files),
            size = format::size(left.bytes)
        )));
    }
    if confirm.has_deletes() {
        let deletes = t!(
            "sync.confirm_deletes",
            right = format::count(summary.delete_right),
            left = format::count(summary.delete_left)
        );
        body = body
            .push(switch(deletes, confirm.deletes, Flag::Deletes))
            .push(keyed_switch(
                t!("sync.confirm_permanent"),
                confirm.permanent,
                Flag::Permanent,
            ));
    }
    body = body.push(keyed_switch(
        t!("sync.confirm_ask_overwrite"),
        confirm.ask_overwrite,
        Flag::AskOverwrite,
    ));
    // Enter runs.
    body.push(layout::button_row(
        Some(layout::primary_button(
            t!("sync.confirm_run"),
            Some(wrap(CompareMsg::SyncRun)),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("sync.confirm_cancel"),
            Some(wrap(CompareMsg::SyncCancelConfirm)),
        )],
        [],
    ))
    .into()
}
