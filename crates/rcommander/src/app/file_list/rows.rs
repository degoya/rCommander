//! A file list's rows: an entry or `..` in the full view, a name in the brief view. Every cell
//! is clipped to its column (`docs/design.md`, Tables): a name too long for its column is
//! shortened at its end or in the middle (`appearance.name_cut`), the whole name in a tooltip
//! after `appearance.name_tip_delay`; the fixed columns keep their widths.
//! Every other row is striped while `appearance.alternate_rows` is on. The rows are in the
//! list's chosen font (RC-010-17): its size sets the row's height and the fixed columns' widths.
//! The widths come in from the list (`columns::split`), worked out once for all its rows.

use iced::{
    Alignment, Color, Element, Font, Length,
    alignment::Horizontal,
    widget::{Space, container, image::Handle, row, text},
};
use rc_engine::{ArchiveInfo, DirSize, Entry};

use super::columns::{Column, ICON_GAP, SPACING, name_room};
use crate::{
    app::{Message, archive_view, panel},
    i18n::t,
    theme::{
        self, Tokens, fit,
        fonts::{AreaFont, Names},
    },
};

#[derive(Debug, Clone, Copy)]
pub(super) struct RowState {
    pub cursor: bool,
    pub active: bool,
    pub selected: bool,
    /// Every other row of the listing, while the stripes are on.
    pub alternate: bool,
    /// The list's font.
    pub font: AreaFont,
    /// How a name too long for its column is shortened.
    pub names: Names,
}

/// The list's columns and the width of each (`columns::split`), the same for all its rows.
#[derive(Debug, Clone, Copy)]
pub(super) struct Layout<'a> {
    pub columns: &'a [Column],
    pub sizes: &'a [f32],
}

impl Layout<'_> {
    /// The name's own width, after the icon when `icon`; nothing before the list is measured.
    fn name_width(self, icon: bool) -> f32 {
        let cell = self
            .columns
            .iter()
            .zip(self.sizes)
            .find(|(column, _)| **column == Column::Name)
            .map_or(0.0, |(_, size)| *size);
        name_room(cell, icon)
    }

    /// The fixed columns with their widths and the text `value` gives each at its width.
    fn values(self, mut value: impl FnMut(Column, f32) -> String) -> Vec<(String, f32, Column)> {
        self.columns
            .iter()
            .zip(self.sizes)
            .filter(|(column, _)| **column != Column::Name)
            .map(|(column, size)| (value(*column, *size), *size, *column))
            .collect()
    }
}

/// One entry's row; inside an archive (`archive`) with its packed size and its marks, and a
/// folder with its counted size (Space, Alt+Shift+Enter) when it has one.
pub(super) fn entry_row<'a>(
    entry: &'a Entry,
    icon: Option<Handle>,
    archive: Option<&ArchiveInfo>,
    counted: Option<&DirSize>,
    state: RowState,
    layout: Layout<'_>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let info = archive.and_then(|archive| archive.get(&entry.name));
    let (mark, warn) = archive_view::marks(info);
    let values = layout.values(|column, width| match column {
        Column::Name => String::new(),
        Column::Ext => panel::ext(entry).to_owned(),
        Column::Size => match counted {
            _ if !entry.is_dir_like() => crate::format::size(entry.size),
            Some(DirSize::Bytes(bytes)) => crate::format::size(*bytes),
            Some(DirSize::Counting) => t!("panel.dir_counting").to_owned(),
            None => t!("panel.dir_marker").to_owned(),
        },
        Column::Packed => archive_view::packed(info),
        Column::Date => crate::format::date(entry.modified),
        Column::Rights => crate::format::rights(&entry.kind, entry.attributes.mode),
        Column::Owner => owner(entry, width, state),
    });
    let cells = Cells {
        name_width: layout.name_width(icon.is_some()),
        icon,
        hidden: state.names.mark_hidden && is_hidden(entry),
        name: panel::stem(entry),
        full: &entry.name,
        mark,
        warn,
        rule: theme::colour_rules::name_colour(&entry.name, entry.is_dir_like(), tokens.dark),
        values,
    };
    list_row(cells, state, tokens)
}

/// The `..` row: the way up, drawn like a directory.
pub(super) fn parent_row<'a>(
    icon: Option<Handle>,
    state: RowState,
    layout: Layout<'_>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let values = layout.values(|column, _| match column {
        Column::Size => t!("panel.dir_marker").to_owned(),
        _ => String::new(),
    });
    let cells = Cells {
        name_width: layout.name_width(icon.is_some()),
        icon,
        hidden: false,
        name: t!("panel.parent"),
        full: t!("panel.parent"),
        mark: String::new(),
        warn: false,
        rule: None,
        values,
    };
    list_row(cells, state, tokens)
}

/// `user:group` as `chown` writes it, shortened in the middle when a long name does not fit
/// the column `width` wide (`docs/design.md`); empty where the backend names no owner.
fn owner(entry: &Entry, width: f32, state: RowState) -> String {
    let Some(owner) = &entry.owner else {
        return String::new();
    };
    let text = format!("{}:{}", owner.user, owner.group);
    state.font.shorten(&text, width).into_owned()
}

/// The icon and the texts of a row's columns.
struct Cells<'a> {
    icon: Option<Handle>,
    /// A hidden or system entry: its icon carries a `!` (`appearance.mark_hidden`).
    hidden: bool,
    name: &'a str,
    /// The whole name with its extension, for the tooltip of a shortened name.
    full: &'a str,
    /// The name column's width; 0 while the list has not been measured.
    name_width: f32,
    /// Marks behind the name inside an archive: encrypted, unsafe stored name.
    mark: String,
    /// The stored name was unsafe: name and marks are drawn as a warning.
    warn: bool,
    /// The colour a rule by file type gives the name (RC-100-43).
    rule: Option<Color>,
    /// The fixed columns' texts in their order, each with its width and its column.
    values: Vec<(String, f32, Column)>,
}

/// The accent pill left of the cursor's row in the active panel.
fn pill(state: RowState, tokens: &'static Tokens) -> Element<'static, Message> {
    container(Space::new().width(3).height(14))
        .style(move |_| {
            if state.cursor && state.active {
                theme::pill(tokens)
            } else {
                container::Style::default()
            }
        })
        .into()
}

/// A row's frame: its height, its colours for cursor, selection and stripe.
fn framed<'a>(
    content: impl Into<Element<'a, Message>>,
    state: RowState,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    container(content)
        .height(state.font.row_height())
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| {
            theme::striped_row(
                tokens,
                state.cursor,
                state.active,
                state.selected,
                state.alternate,
            )
        })
        .into()
}

fn list_row<'a>(
    cells: Cells<'a>,
    state: RowState,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (font, size) = (state.font.font, state.font.size);
    let secondary = if state.selected {
        tokens.marked
    } else {
        tokens.list_text_secondary
    };
    let room = cells.name_width - state.font.width(&cells.mark);
    let rule = cells.rule;
    let name = shortened_name(
        cells.name,
        cells.full,
        room,
        (state, rule),
        tokens,
        |name| {
            let mark = sized(cells.mark.clone(), font, size);
            if cells.warn {
                row![name.color(theme::DANGER), mark.color(theme::DANGER)]
            } else {
                row![name, mark.color(secondary)]
            }
        },
    );
    let mut columns: Vec<Element<'a, Message>> =
        vec![pill(state, tokens), named(cells.icon, cells.hidden, name)];
    for (value, width, column) in cells.values {
        let colour = value_colour(column, cells.rule, state.selected, secondary);
        let value = sized(value, font, size).color(colour);
        columns.push(fixed(value, width, column.right()));
    }
    let columns = row(columns)
        .spacing(SPACING)
        .padding([0, 6])
        .align_y(Alignment::Center);
    framed(columns, state, tokens)
}

/// The colour of a fixed column's text: the extension takes the colour a rule by file type gives
/// the name, as it belongs to it (owner, 2026-10-22); a marked row and every other column keep
/// `secondary`.
fn value_colour(column: Column, rule: Option<Color>, selected: bool, secondary: Color) -> Color {
    match rule {
        Some(rule) if column == Column::Ext && !selected => rule,
        _ => secondary,
    }
}

/// A name in the brief view: a directory in brackets, as in Total Commander; `None` is `..`.
/// `room` is the name's width in its column.
pub(super) fn brief_cell<'a>(
    entry: Option<&'a Entry>,
    icon: Option<Handle>,
    state: RowState,
    room: f32,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let label = match entry {
        Some(entry) if entry.is_dir_like() => t!("panel.brief_dir", name = entry.name),
        Some(entry) => entry.name.clone(),
        None => t!("panel.brief_dir", name = t!("panel.parent")),
    };
    let rule = entry.and_then(|entry| {
        theme::colour_rules::name_colour(&entry.name, entry.is_dir_like(), tokens.dark)
    });
    let name = shortened_name(&label, &label, room, (state, rule), tokens, |name| {
        row![name]
    });
    let hidden = state.names.mark_hidden && entry.is_some_and(is_hidden);
    let content = row![pill(state, tokens), named(icon, hidden, name)]
        .spacing(6)
        .padding([0, 6])
        .align_y(Alignment::Center);
    framed(content, state, tokens)
}

/// `name` in its column, `room` wide: shortened at its end or in the middle when it does not fit
/// in the list's font (the whole `full` name then in a tooltip after the delay), clipped at the
/// column's edge either way.
/// `parts` puts the name's text into its row (with the archive marks); `rule` is the colour a
/// rule by file type gives the name.
fn shortened_name<'a>(
    name: &str,
    full: &str,
    room: f32,
    (state, rule): (RowState, Option<Color>),
    tokens: &'static Tokens,
    parts: impl FnOnce(text::Text<'a>) -> iced::widget::Row<'a, Message>,
) -> Element<'a, Message> {
    // Unmeasured (0): the whole name, clipped.
    let shown = match (room > 0.0, state.names.cut_end) {
        (true, true) => state.font.shorten_end(name, room),
        (true, false) => state.font.shorten(name, room),
        (false, _) => name.into(),
    };
    let shortened = shown != name;
    // A marked name changes its colour only, as in Total Commander: a heavier weight the chosen
    // font lacks would be drawn in another font, and the row would jump.
    let text = sized(shown.into_owned(), state.font.font, state.font.size);
    let text = match (state.selected, rule) {
        (true, _) => text.color(tokens.marked),
        (false, Some(rule)) => text.color(rule),
        (false, None) => text,
    };
    let clipped = container(parts(text)).width(Length::Fill).clip(true);
    if shortened {
        fit::tip_after(clipped, full.to_owned(), state.names.tip_delay, tokens)
    } else {
        clipped.into()
    }
}

/// A fixed column's cell: `width` wide, clipped, right-aligned when `right`.
fn fixed<'a>(content: text::Text<'a>, width: f32, right: bool) -> Element<'a, Message> {
    let cell = container(content).width(width).clip(true);
    if right {
        cell.align_x(Horizontal::Right).into()
    } else {
        cell.into()
    }
}

/// Whether `entry` is hidden or a system file: what Total Commander marks with `!`.
fn is_hidden(entry: &Entry) -> bool {
    entry.hidden
        || entry
            .attributes
            .windows
            .is_some_and(|flags| flags.hidden || flags.system)
}

/// A name with the row's icon before it, when icons are on; `hidden` marks the icon.
fn named<'a>(
    icon: Option<Handle>,
    hidden: bool,
    name: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let name: Element<'a, Message> = name.into();
    match icon {
        Some(icon) => row![crate::app::icons::view(icon, hidden), name]
            .spacing(ICON_GAP)
            .align_y(Alignment::Center)
            .into(),
        None => name,
    }
}

/// One cell at the interface's size: a single line, clipped by its column.
pub(super) fn cell<'a>(content: impl text::IntoFragment<'a>, font: Font) -> text::Text<'a> {
    sized(content, font, theme::TEXT_SIZE)
}

/// One cell at `size`: a single line, clipped by its column.
pub(super) fn sized<'a>(
    content: impl text::IntoFragment<'a>,
    font: Font,
    size: f32,
) -> text::Text<'a> {
    text(content)
        .size(size)
        .font(font)
        .wrapping(text::Wrapping::None)
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
