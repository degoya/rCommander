//! Drawing the Columns tab (RC-100-90): the list of the column sets with its buttons, the marked
//! set's editor beside it - name, the table of its columns, the folder masks, Save - and the last
//! note. Every control is a stop of the dialog's focus ring: the list and each pick list one, the
//! buttons through `theme::layout`'s helpers, the text fields by themselves.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, button, column, pick_list, row, text_input},
};
use rc_engine::{Align, Field};

use super::{ColumnsMsg, ColumnsPage, Editor, page_message};
use crate::{
    app::{
        Message,
        settings::view::{hint, label},
    },
    i18n::t,
    theme::{
        self, DANGER, Tokens, focus,
        icon::Icon,
        layout::{glyph_button, primary_button, secondary_button, secondary_icon_button},
    },
};

/// A field in a pick list, shown by its translated name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldChoice(Field);

impl fmt::Display for FieldChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(field_name(self.0))
    }
}

/// A field's name in the window's language.
fn field_name(field: Field) -> &'static str {
    match field {
        Field::Name => t!("column.field.name"),
        Field::Ext => t!("column.field.ext"),
        Field::Size => t!("column.field.size"),
        Field::Packed => t!("column.field.packed"),
        Field::Modified => t!("column.field.modified"),
        Field::Created => t!("column.field.created"),
        Field::Accessed => t!("column.field.accessed"),
        Field::Attributes => t!("column.field.attributes"),
        Field::Rights => t!("column.field.rights"),
        Field::Owner => t!("column.field.owner"),
        Field::Group => t!("column.field.group"),
        Field::Comment => t!("column.field.comment"),
        Field::Git => t!("column.field.git"),
    }
}

/// An alignment in a pick list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlignChoice(Align);

impl fmt::Display for AlignChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            Align::Left => t!("settings.columns.align.left"),
            Align::Right => t!("settings.columns.align.right"),
        })
    }
}

/// The tab: the sets and their buttons, the editor, the note.
pub fn view<'a>(page: &'a ColumnsPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut body = Column::new()
        .spacing(12)
        .push(label(t!("settings.columns.sets")))
        .push(list(page, tokens))
        .push(buttons(page))
        .push(hint(t!("settings.columns.help"), tokens));
    if let Some(editor) = &page.editor {
        body = body.push(editor_view(page, editor, tokens));
    }
    if let Some((note, error)) = &page.note {
        let colour = if *error {
            DANGER
        } else {
            tokens.text_secondary
        };
        body = body.push(theme::fit::prose(note.as_str()).color(colour));
    }
    body.into()
}

/// The sets in their order, the marked one marked; the arrows mark the one before and after.
fn list<'a>(page: &'a ColumnsPage, tokens: &'static Tokens) -> Element<'a, Message> {
    if page.sets.is_empty() {
        return hint(t!("settings.columns.empty"), tokens).into();
    }
    let ids: Vec<i64> = page.sets.iter().map(|set| set.id).collect();
    let mut rows = Column::new().spacing(2);
    for set in &page.sets {
        let marked = page.marked == Some(set.id);
        let masks = set.spec.masks.join("; ");
        let line = row![
            label(set.name.as_str()).width(Length::Fill),
            hint(masks, tokens).width(Length::FillPortion(1)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        rows = rows.push(
            button(line)
                .width(Length::Fill)
                .padding([4, 8])
                .style(move |_, status| theme::page_entry(tokens, marked, status))
                .on_press(page_message(ColumnsMsg::Choose(set.id))),
        );
    }
    focus::pick(
        rows.width(Length::Fill),
        "settings.columns.list",
        &ids,
        page.marked.as_ref(),
        |id| page_message(ColumnsMsg::Choose(id)),
    )
}

/// New, Copy, Up, Down and Delete.
fn buttons(page: &ColumnsPage) -> Element<'_, Message> {
    let ready = !page.busy;
    let on = |enabled: bool, message: ColumnsMsg| (ready && enabled).then(|| page_message(message));
    let at = page.marked.and_then(|id| page.index_of(id));
    let last = page.sets.len().saturating_sub(1);
    row![
        secondary_icon_button(
            Icon::Add,
            t!("settings.columns.new"),
            on(
                page.sets.len() < rc_engine::columns::MAX_SETS,
                ColumnsMsg::New
            )
        ),
        secondary_icon_button(
            Icon::Copy,
            t!("settings.columns.copy"),
            on(at.is_some(), ColumnsMsg::Copy)
        ),
        secondary_icon_button(
            Icon::MoveUp,
            t!("settings.columns.up"),
            on(at.is_some_and(|at| at > 0), ColumnsMsg::Up)
        ),
        secondary_icon_button(
            Icon::MoveDown,
            t!("settings.columns.down"),
            on(at.is_some_and(|at| at < last), ColumnsMsg::Down)
        ),
        secondary_icon_button(
            Icon::Delete,
            t!("settings.columns.delete"),
            on(at.is_some(), ColumnsMsg::Delete)
        ),
    ]
    .spacing(8)
    .into()
}

/// The marked set's name, its columns, its masks and Save.
fn editor_view<'a>(
    page: &'a ColumnsPage,
    editor: &'a Editor,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut table = Column::new().spacing(4).push(
        row![
            hint(t!("settings.columns.field"), tokens).width(170),
            hint(t!("settings.columns.title"), tokens).width(Length::Fill),
            hint(t!("settings.columns.width"), tokens).width(70),
            hint(t!("settings.columns.align"), tokens).width(110),
        ]
        .spacing(6),
    );
    let last = editor.rows.len().saturating_sub(1);
    for (at, column) in editor.rows.iter().enumerate() {
        table = table.push(column_row(
            at,
            column,
            at == last,
            editor.rows.len(),
            tokens,
        ));
    }
    let more = (editor.rows.len() < rc_engine::columns::MAX_COLUMNS)
        .then(|| page_message(ColumnsMsg::AddRow));
    column![
        label(t!("settings.columns.name")),
        text_input(t!("settings.columns.name"), &editor.name)
            .on_input(|text| page_message(ColumnsMsg::Name(text)))
            .width(320),
        label(t!("settings.columns.columns")),
        table,
        secondary_icon_button(Icon::Add, t!("settings.columns.add"), more),
        label(t!("settings.columns.masks")),
        text_input(t!("settings.columns.masks_placeholder"), &editor.masks)
            .on_input(|text| page_message(ColumnsMsg::Masks(text)))
            .width(Length::Fill),
        hint(t!("settings.columns.masks_help"), tokens),
        primary_button(
            t!("settings.columns.save"),
            (!page.busy).then(|| page_message(ColumnsMsg::Save))
        ),
    ]
    .spacing(6)
    .into()
}

/// One column of the editor's table: field, title, width, alignment, and its buttons.
fn column_row<'a>(
    at: usize,
    column: &'a super::Row,
    last: bool,
    count: usize,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let fields: Vec<FieldChoice> = Field::ALL.into_iter().map(FieldChoice).collect();
    let field = FieldChoice(column.field);
    let on_field = move |choice: FieldChoice| page_message(ColumnsMsg::Field(at, choice.0));
    let field_moves = focus::moves(&fields, Some(&field), &on_field);
    let field_pick = pick_list(fields, Some(field), on_field).width(170);
    let aligns = [AlignChoice(Align::Left), AlignChoice(Align::Right)];
    let align = AlignChoice(column.align);
    let on_align = move |choice: AlignChoice| page_message(ColumnsMsg::Align(at, choice.0));
    let align_moves = focus::moves(&aligns, Some(&align), &on_align);
    let align_pick = pick_list(aligns, Some(align), on_align).width(110);
    row![
        focus::Stop::pick(
            field_pick,
            &format!("settings.columns.field.{at}"),
            field_moves
        ),
        text_input(t!("settings.columns.title_placeholder"), &column.title)
            .on_input(move |text| page_message(ColumnsMsg::Title(at, text)))
            .width(Length::Fill),
        text_input("", &column.width)
            .on_input(move |text| page_message(ColumnsMsg::Width(at, text)))
            .width(70),
        focus::Stop::pick(
            align_pick,
            &format!("settings.columns.align.{at}"),
            align_moves
        ),
        glyph_button(
            Icon::MoveUp,
            t!("settings.columns.row_up"),
            (at > 0).then(|| page_message(ColumnsMsg::RowUp(at))),
            tokens,
        ),
        glyph_button(
            Icon::MoveDown,
            t!("settings.columns.row_down"),
            (!last).then(|| page_message(ColumnsMsg::RowDown(at))),
            tokens,
        ),
        secondary_button(
            t!("settings.columns.remove"),
            (count > 1).then(|| page_message(ColumnsMsg::RowRemove(at)))
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}
