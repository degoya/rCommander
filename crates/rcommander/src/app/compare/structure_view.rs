//! Drawing the diff window's Structure view (RC-100-41): the switch between the views, the note
//! when a structural comparison did not come, and the pane - what the plugin found, its notices,
//! the filter by kind with each kind's count, and the changes on screen as a table (path,
//! column, kind, old and new value), the cursor framed.

use iced::{
    Alignment, Color, Element, Length,
    widget::{Column, Row, column, container, mouse_area, row, sensor},
};
use rc_engine::compare::structure::{Change, ChangeKind, Comparison};

use super::{
    DiffWindow,
    diff_view::flag,
    structure::{DiffView, Structure, StructureMsg, StructureNote, StructurePane, wrap},
};
use crate::{
    app::Message,
    format,
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{self, line},
        focus, layout,
    },
};

const KIND_WIDTH: f32 = 90.0;

/// The rows that fit a list of `height`.
pub fn rows_for(height: f32) -> usize {
    (height / theme::ROW_HEIGHT).floor().max(1.0) as usize
}

/// The switch to the Structure view, offered only when a plugin compared the files.
pub(super) fn switch<'a>(window: &DiffWindow) -> Option<Element<'a, Message>> {
    window.pane()?;
    let on = window.view == DiffView::Structure;
    Some(layout::check(
        t!("compare.structure_view"),
        on,
        true,
        |on| {
            wrap(StructureMsg::View(if on {
                DiffView::Structure
            } else {
                DiffView::Text
            }))
        },
    ))
}

/// Why the files were not compared by structure, when the person should know.
pub(super) fn note<'a>(
    window: &DiffWindow,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let Structure::Absent(Some(note)) = &window.structure else {
        return None;
    };
    let text = match note {
        StructureNote::TooLarge(limit) => {
            t!("compare.structure_too_large", limit = format::count(*limit))
        }
        StructureNote::Failed { plugin, code } => t!(
            "compare.structure_failed",
            plugin = plugin.as_str(),
            error = error_text(code)
        ),
    };
    Some(fit::prose(text).color(tokens.text_secondary).into())
}

/// The plugin's changes: what it found, its notices, the filter, the table.
pub(super) fn pane<'a>(pane: &'a StructurePane, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut parts = Column::new().spacing(6).height(Length::Fill);
    parts = parts.push(line(summary(pane)).color(tokens.accent));
    for notice in &pane.notices {
        parts = parts.push(fit::prose(notice.as_str()).color(theme::DANGER));
    }
    let summary = &pane.comparison.summary;
    let filters = ChangeKind::ALL.iter().map(|kind| {
        let kind = *kind;
        let label = t!(
            "compare.kind_count",
            kind = kind_label(kind),
            count = format::count(u64::from(summary.of(kind)))
        );
        flag(label, !pane.hidden.contains(&kind), move |on| {
            wrap(StructureMsg::Filter(kind, on))
        })
    });
    parts = parts.push(
        Row::with_children(filters)
            .spacing(12)
            .align_y(Alignment::Center),
    );
    parts.push(table(pane, tokens)).into()
}

/// "Data compare: 3 changes" with the counts, or that the data is the same; a list cut short
/// says how many it shows.
fn summary(pane: &StructurePane) -> String {
    let comparison: &Comparison = &pane.comparison;
    let mut text = if comparison.is_equal() {
        t!("compare.structure_equal", plugin = pane.plugin.as_str())
    } else {
        t!(
            "compare.structure_summary",
            plugin = pane.plugin.as_str(),
            count = format::count(comparison.summary.total())
        )
    };
    if comparison.summary.truncated {
        text.push_str("   ");
        text.push_str(&t!(
            "compare.structure_truncated",
            shown = format::count(comparison.changes.len() as u64)
        ));
    }
    text
}

fn kind_label(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => t!("compare.kind.added"),
        ChangeKind::Removed => t!("compare.kind.removed"),
        ChangeKind::Changed => t!("compare.kind.changed"),
        ChangeKind::Moved => t!("compare.kind.moved"),
    }
}

fn kind_color(kind: ChangeKind, tokens: &'static Tokens) -> Color {
    let palette = theme::iced_theme(tokens).palette();
    match kind {
        ChangeKind::Added => palette.success,
        ChangeKind::Removed => palette.danger,
        ChangeKind::Changed => palette.warning,
        ChangeKind::Moved => tokens.accent,
    }
}

/// The header and the changes on screen, the wheel scrolling them; the list reports its size.
fn table<'a>(pane: &'a StructurePane, tokens: &'static Tokens) -> Element<'a, Message> {
    let secondary = |label: &'static str| line(label).color(tokens.text_secondary);
    let header = row![
        secondary(t!("compare.column.path")).width(Length::FillPortion(3)),
        secondary(t!("compare.column.column")).width(Length::FillPortion(1)),
        secondary(t!("compare.column.kind")).width(KIND_WIDTH),
        secondary(t!("compare.column.old")).width(Length::FillPortion(2)),
        secondary(t!("compare.column.new")).width(Length::FillPortion(2)),
    ]
    .spacing(8)
    .padding([0, 6]);
    let shown = pane.shown();
    let cursor = pane.rows.cursor();
    let rows = pane
        .rows
        .visible(shown.len())
        .filter_map(|at| {
            let index = *shown.get(at)?;
            let change = pane.comparison.changes.get(index)?;
            Some(table_row(change, index, at, at == cursor, tokens))
        })
        .collect::<Vec<_>>();
    let rows = container(Column::with_children(rows))
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true);
    let rows = mouse_area(rows).on_scroll(|delta| {
        wrap(StructureMsg::Scroll(theme::wheel::rows(
            delta,
            theme::ROW_HEIGHT,
        )))
    });
    let rows = sensor(rows)
        .on_show(|size| wrap(StructureMsg::Viewport(size)))
        .on_resize(|size| wrap(StructureMsg::Viewport(size)));
    // The changes are one stop of the ring; the arrows move their cursor, Enter opens one.
    let rows = focus::choice(rows, "compare.structure_rows", None, None);
    column![header, rows].spacing(4).height(Length::Fill).into()
}

/// One change: its cells on one line each, cut with the whole value in a tooltip.
fn table_row<'a>(
    change: &'a Change,
    index: usize,
    at: usize,
    cursor: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let value = |value: &Option<String>| value.clone().unwrap_or_default();
    let cells = row![
        fit::cut(change.path.clone(), theme::MONO_FONT, None, 3, tokens),
        fit::cell(value(&change.column), 1, false, tokens),
        container(line(kind_label(change.kind)).color(kind_color(change.kind, tokens)))
            .width(KIND_WIDTH),
        fit::cut(value(&change.old_value), theme::MONO_FONT, None, 2, tokens),
        fit::cut(value(&change.new_value), theme::MONO_FONT, None, 2, tokens),
    ]
    .spacing(8)
    .padding([0, 6])
    .align_y(Alignment::Center);
    let framed = container(cells)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, true, false, theme::alternate(at)));
    // focus-ring: pointer only (a row of the changes stop; Enter opens the one under the cursor)
    mouse_area(framed)
        .on_press(wrap(StructureMsg::Jump(index)))
        .into()
}
