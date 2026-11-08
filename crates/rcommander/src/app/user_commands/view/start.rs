//! The "Start menu" tab (RC-100-86): on the left the menu as a tree - each row indented by its
//! depth as the connection manager's tree is, a submenu with its sign, a separator as a line -,
//! the delete question and the buttons that add, delete and move rows, into the submenu above and
//! out of their own too; on the right the form of the row at the cursor, or of a new one: its
//! kind, its title and an entry's command.

use iced::{
    Alignment, Element, Length,
    widget::{Column, Row, Space, column, container, mouse_area, row, scrollable},
};
use rc_engine::{StartItem, StartItemKind};

use super::{
    TITLE_FIELD, TREE_STOP_ID,
    command::{self, Labeled, choice, field, labeled},
    delete_question, wrap,
};
use crate::{
    app::{
        Message,
        palette::user_commands::plain_title,
        user_commands::{
            CommandsWindow, Pending, UcMsg,
            form::{StartForm, StartMsg},
            keys::Focus,
            list::{self, Move},
        },
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// Pixels a level of the tree indents, as the connection manager's.
pub const INDENT: f32 = 16.0;

fn start(message: StartMsg) -> Message {
    wrap(UcMsg::Start(message))
}

/// How far a row of `depth` is indented.
pub fn indent(depth: usize) -> f32 {
    INDENT * depth as f32
}

/// The left side: the tree, the question, the buttons, the keys.
pub(super) fn tree_side<'a>(
    window: &'a CommandsWindow,
    items: &[StartItem],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let active = window.focus == Focus::List;
    let mut lines = Column::new().spacing(1);
    if items.is_empty() {
        lines = lines.push(fit::hint(t!("user_commands.start_empty"), tokens));
    }
    for (at, item) in items.iter().enumerate() {
        let cursor = window.start_cursor == Some(item.id);
        lines = lines.push(row_view(
            item,
            (cursor, active, theme::alternate(at)),
            tokens,
        ));
    }
    let tree = container(scrollable(lines).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, active));
    // The tree is one stop of the ring; the window moves its cursor (`keys.rs`).
    let tree = focus::Stop::choice(tree, TREE_STOP_ID, None, None).id(TREE_STOP_ID);
    let mut side = column![Element::from(tree)].spacing(8);
    if let Some(Pending::Delete(id)) = window.pending {
        let item = items.iter().find(|item| item.id == id);
        let question = match item {
            Some(item) if item.kind == StartItemKind::Submenu => t!(
                "user_commands.delete_submenu_question",
                title = plain_title(&item.title)
            ),
            Some(item) if item.kind == StartItemKind::Item => t!(
                "user_commands.delete_entry_question",
                title = plain_title(&item.title)
            ),
            _ => t!("user_commands.delete_separator_question").to_owned(),
        };
        side = side.push(delete_question(question, tokens));
    }
    let moves = window
        .start_cursor
        .map(|id| {
            [Move::Up, Move::Down, Move::Into, Move::Out]
                .map(|to| list::moved(items, id, to).is_some())
        })
        .unwrap_or_default();
    let can = |to: Move| {
        let at = match to {
            Move::Up => 0,
            Move::Down => 1,
            Move::Into => 2,
            Move::Out => 3,
        };
        moves[at].then_some(wrap(UcMsg::Move(to)))
    };
    let glyph = |icon: Icon, label: &'static str, to: Move| {
        layout::glyph_button(icon, label, can(to), tokens)
    };
    let buttons = Row::new()
        .push(command::small(
            Some(Icon::Add),
            t!("user_commands.add"),
            Some(UcMsg::New),
        ))
        .push(command::small(
            Some(Icon::Delete),
            t!("user_commands.delete"),
            window.start_cursor.map(|_| UcMsg::Delete),
        ))
        .push(glyph(Icon::MoveUp, t!("user_commands.move_up"), Move::Up))
        .push(glyph(
            Icon::MoveDown,
            t!("user_commands.move_down"),
            Move::Down,
        ))
        .push(glyph(
            Icon::Indent,
            t!("user_commands.move_into"),
            Move::Into,
        ))
        .push(glyph(
            Icon::Outdent,
            t!("user_commands.move_out"),
            Move::Out,
        ))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);
    side.push(buttons)
        .push(fit::help_line(t!("user_commands.start_keys"), tokens))
        .into()
}

/// One row: indented by its depth, a submenu's sign or an entry's dot and its title, a separator
/// a line; a click puts the cursor on it. `state`: the cursor is on it, the tree has the keys, it
/// is striped.
fn row_view<'a>(
    item: &StartItem,
    state: (bool, bool, bool),
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (cursor, active, alternate) = state;
    let (sign, font) = match item.kind {
        StartItemKind::Submenu => ("\u{25B8}", theme::UI_FONT_STRONG),
        StartItemKind::Item => ("\u{00B7}", theme::UI_FONT),
        StartItemKind::Separator => ("", theme::UI_FONT),
    };
    let body: Element<'a, Message> = match item.kind {
        StartItemKind::Separator => container(Space::new().height(1))
            .width(Length::Fill)
            .style(move |_| theme::divider(tokens))
            .into(),
        _ => fit::cut(plain_title(&item.title), font, None, 1, tokens),
    };
    let line = row![
        Space::new().width(indent(item.depth)),
        fit::label(sign).width(16).color(tokens.accent),
        body,
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let shown = container(line)
        .padding([0, 6])
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, active, false, alternate));
    // focus-ring: pointer only (a row of the tree, which is one stop of the ring)
    mouse_area(shown)
        .on_press(wrap(UcMsg::Select(item.id)))
        .into()
}

/// The right side: the form of the row at the cursor or of a new one - its kind, its title, an
/// entry's command and key -; a hint while there is none.
pub(super) fn form_side<'a>(
    window: &'a CommandsWindow,
    key: Option<String>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(form) = &window.start else {
        return container(fit::hint(t!("user_commands.start_choose"), tokens))
            .padding(8)
            .into();
    };
    let title = if form.id.is_none() {
        t!("user_commands.start_new_title")
    } else {
        t!("user_commands.start_edit_title")
    };
    column![
        fit::label(title).font(theme::UI_FONT_STRONG),
        scrollable(form_rows(form, key, tokens).padding([0, 12])).height(Length::Fill),
    ]
    .spacing(10)
    .into()
}

fn form_rows<'a>(
    form: &'a StartForm,
    key: Option<String>,
    tokens: &'static Tokens,
) -> Column<'a, Message> {
    let kinds = vec![
        Labeled(
            StartItemKind::Item,
            t!("user_commands.start_kind.item").to_owned(),
        ),
        Labeled(
            StartItemKind::Submenu,
            t!("user_commands.start_kind.submenu").to_owned(),
        ),
        Labeled(
            StartItemKind::Separator,
            t!("user_commands.start_kind.separator").to_owned(),
        ),
    ];
    let kind = choice(
        kinds,
        &form.kind,
        |kind| start(StartMsg::Kind(kind)),
        "user_commands.start_kind",
    );
    let mut rows = Column::new()
        .spacing(8)
        .push(labeled(t!("user_commands.field.start_kind"), kind));
    if form.kind == StartItemKind::Separator {
        return rows;
    }
    let title = field("", &form.title)
        .id(TITLE_FIELD)
        .on_input(|typed| start(StartMsg::Title(typed)))
        .on_submit(wrap(UcMsg::Save));
    rows = rows
        .push(labeled(t!("user_commands.field.title"), title))
        .push(command::help(t!("user_commands.help.title"), tokens));
    if !form.has_command() {
        return rows;
    }
    rows = rows.push(command::fields(
        &form.action,
        |message| start(StartMsg::Cmd(message)),
        tokens,
    ));
    if let Some(key) = command::key_row(form.id.is_some() && !form.dirty, key) {
        rows = rows.push(key);
    }
    rows
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;
