//! The "Own commands" tab (RC-100-86): on the left the filter and the commands `em_<name>` - each
//! its name and its tooltip -, the delete question and the list's buttons; on the right the form
//! of the one at the cursor, or of a new one.

use iced::{
    Alignment, Element, Length,
    widget::{
        Column, Row, Space, column, container,
        image::{Handle, Image},
        mouse_area, row, scrollable, text,
    },
};
use rc_engine::UserCommand;

use super::{
    FILTER_FIELD, LIST_STOP_ID, NAME_FIELD,
    command::{self, field, labeled},
    delete_question, wrap,
};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        user_commands::{
            CommandsWindow, Pending, Shown, UcMsg,
            form::{OwnForm, OwnMsg},
            keys::Focus,
            list,
        },
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

fn own(message: OwnMsg) -> Message {
    wrap(UcMsg::Own(message))
}

/// The left side: the filter, the rows, the question, the buttons, the keys.
pub(super) fn list_side<'a>(
    window: &'a CommandsWindow,
    commands: &[UserCommand],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let filter = field(t!("user_commands.filter"), &window.filter)
        .id(FILTER_FIELD)
        .on_input(|text| wrap(UcMsg::Filter(text)));
    let active = matches!(window.focus, Focus::List | Focus::Filter);
    let shown = list::shown_commands(commands, &window.filter);
    let mut lines = Column::new().spacing(1);
    if shown.is_empty() {
        let empty = if commands.is_empty() {
            t!("user_commands.empty")
        } else {
            t!("user_commands.no_match")
        };
        lines = lines.push(fit::hint(empty, tokens));
    }
    for (at, id) in shown.iter().enumerate() {
        let Some(command) = commands.iter().find(|command| command.id == *id) else {
            continue;
        };
        let cursor = window.cursor == Some(*id);
        lines = lines.push(row_view(
            command,
            (cursor, active, theme::alternate(at)),
            tokens,
        ));
    }
    let list = container(scrollable(lines).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, active));
    // The list is one stop of the ring; the window moves its cursor (`keys.rs`).
    let list = focus::Stop::choice(list, LIST_STOP_ID, None, None).id(LIST_STOP_ID);
    let mut side = column![filter, Element::from(list)].spacing(8);
    if let Some(Pending::Delete(id)) = window.pending {
        let name = commands
            .iter()
            .find(|command| command.id == id)
            .map_or_else(String::new, |command| command.name.clone());
        let question = t!("user_commands.delete_question", name = name);
        side = side.push(delete_question(question, tokens));
    }
    let chosen = window.cursor.is_some();
    let buttons = Row::new()
        .push(command::small(
            Some(Icon::Add),
            t!("user_commands.new"),
            Some(UcMsg::New),
        ))
        .push(command::small(
            Some(Icon::Delete),
            t!("user_commands.delete"),
            chosen.then_some(UcMsg::Delete),
        ))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);
    side.push(buttons)
        .push(fit::help_line(t!("user_commands.keys"), tokens))
        .into()
}

/// One row: the name and the tooltip; a click puts the cursor on it. `state`: the cursor is on
/// it, the list has the keys, it is striped.
fn row_view<'a>(
    command: &UserCommand,
    state: (bool, bool, bool),
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (cursor, active, alternate) = state;
    let line = row![
        fit::cut(command.name.clone(), theme::UI_FONT, None, 2, tokens),
        fit::cut(
            command.tooltip.clone(),
            theme::UI_FONT,
            Some(tokens.text_secondary),
            3,
            tokens
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let shown = container(line)
        .padding([0, 6])
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, active, false, alternate));
    // focus-ring: pointer only (a row of the list, which is one stop of the ring)
    mouse_area(shown)
        .on_press(wrap(UcMsg::Select(command.id)))
        .into()
}

/// The right side: the form of the command at the cursor or of a new one - its name, its command,
/// its tooltip and its key -; a hint while there is none.
pub(super) fn form_side<'a>(
    window: &'a CommandsWindow,
    shown: &Shown,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(form) = &window.own else {
        return container(fit::hint(t!("user_commands.choose"), tokens))
            .padding(8)
            .into();
    };
    let title = if form.id.is_none() {
        t!("user_commands.new_title")
    } else {
        t!("user_commands.edit_title")
    };
    let rows = form_rows(form, shown, tokens);
    column![
        fit::label(title).font(theme::UI_FONT_STRONG),
        scrollable(rows.padding([0, 12])).height(Length::Fill),
    ]
    .spacing(10)
    .into()
}

fn form_rows<'a>(form: &'a OwnForm, shown: &Shown, tokens: &'static Tokens) -> Column<'a, Message> {
    let name = field("", &form.name)
        .id(NAME_FIELD)
        .on_input(|typed| own(OwnMsg::Name(typed)))
        .on_submit(wrap(UcMsg::Save));
    let tooltip = field(t!("user_commands.tooltip_placeholder"), &form.tooltip)
        .on_input(|typed| own(OwnMsg::Tooltip(typed)));
    let mut rows = Column::new()
        .spacing(8)
        .push(labeled(t!("user_commands.field.name"), name))
        .push(command::help(t!("user_commands.help.name"), tokens))
        .push(command::fields(
            &form.action,
            |message| own(OwnMsg::Cmd(message)),
            tokens,
        ))
        .push(labeled(t!("user_commands.field.tooltip"), tooltip))
        .push(labeled(
            t!("user_commands.field.icon"),
            symbol_row(form, shown),
        ))
        .push(command::help(t!("user_commands.help.icon"), tokens));
    if let Some(key) = command::key_row(form.id.is_some() && !form.dirty, shown.key.clone()) {
        rows = rows.push(key);
    }
    rows
}

/// The symbol: its file, the icon's index, Browse..., "Choose symbol..." (the collection,
/// RC-110-10) and the symbol at the bars' size as a button that runs the command shows it
/// (RC-100-87); the fields and the buttons are stops of the focus ring, the preview is none.
fn symbol_row<'a>(form: &'a OwnForm, shown: &Shown) -> Row<'a, Message> {
    let size = f32::from(shown.icon_size);
    let preview: Element<'a, Message> = match (&shown.icon, form.collection_glyph()) {
        (_, Some(glyph)) => text(glyph)
            .font(theme::symbols::FONT)
            .size(size)
            .line_height(text::LineHeight::Relative(1.0))
            .center()
            .width(size)
            .height(size)
            .into(),
        (Some(handle), None) => Image::<Handle>::new(handle.clone())
            .width(size)
            .height(size)
            .into(),
        (None, None) => Space::new().width(size).height(size).into(),
    };
    let mut symbol = Row::new()
        .push(preview)
        .push(
            field(t!("user_commands.icon_placeholder"), &form.icon_path)
                .on_input(|typed| own(OwnMsg::IconPath(typed))),
        )
        .push(
            field("0", &form.icon_index.to_string())
                .width(56)
                .on_input(|typed| own(OwnMsg::IconIndex(typed))),
        )
        .spacing(6)
        .align_y(Alignment::Center);
    if let Some(browse) = browse::button(PathField::OwnCommandIcon, &form.icon_path, true) {
        symbol = symbol.push(browse);
    }
    symbol.push(layout::secondary_button(
        t!("symbol_picker.open"),
        Some(wrap(UcMsg::ChooseSymbol)),
    ))
}
