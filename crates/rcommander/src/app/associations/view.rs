//! Drawing the window (RC-100-91), laid out as the own commands' (`user_commands/view.rs`): on the
//! left the filter, the rules in the order they are tried - each its mask and description -, the
//! list's buttons and its keys; on the right the form of the rule at the cursor: its mask,
//! description and symbol, which of its commands is shown with the buttons that add and remove
//! one, a named command's name, and the command's fields shared with the own commands. The note
//! and the buttons below in one row. Always in a window of its own (`associations.rs`).

use iced::{
    Alignment, Element, Length,
    widget::{Column, Row, Space, column, container, mouse_area, row, scrollable, text},
};
use rc_engine::FileAssociation;

use super::{AssocMsg, AssocWindow, form::RuleMsg, keys::Focus};
use crate::{
    app::{
        Message,
        user_commands::{Labeled, choice, field, fields, help, labeled},
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The filter: the keyboard starts here.
pub const FILTER_FIELD: &str = "rcommander.associations.filter";
/// The rules' list as one stop of the focus ring.
pub const LIST_STOP_ID: &str = "rcommander.associations.list";
/// The rule's mask in its form: a new rule puts the keyboard here.
pub const MASK_FIELD: &str = "rcommander.associations.mask";

fn wrap(message: AssocMsg) -> Message {
    Message::Associations(message)
}

fn rule(message: RuleMsg) -> Message {
    wrap(AssocMsg::Edit(message))
}

fn command(message: crate::app::user_commands::CmdMsg) -> Message {
    rule(RuleMsg::Cmd(message))
}

/// The list, the form and the one row of buttons: the whole of the window.
pub fn body<'a>(window: &'a AssocWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let main = row![
        // The list and the form share the width, 2:3.
        container(list_side(window, tokens))
            .width(Length::FillPortion(2))
            .height(Length::Fill),
        container(form_side(window, tokens))
            .width(Length::FillPortion(3))
            .height(Length::Fill),
    ]
    .spacing(16)
    .height(Length::Fill);
    let note: Element<'a, Message> = match &window.note {
        Some((note, error)) => {
            let color = if *error {
                theme::DANGER
            } else {
                tokens.text_secondary
            };
            fit::label(note.as_str())
                .color(color)
                .wrapping(text::Wrapping::WordOrGlyph)
                .into()
        }
        None => Space::new().into(),
    };
    let buttons = Row::new()
        .push(container(note).width(Length::Fill))
        .push(layout::primary_icon_button(
            Icon::Save,
            t!("associations.save"),
            (window.dirty && !window.busy).then_some(wrap(AssocMsg::Save)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Close,
            t!("associations.close"),
            Some(wrap(AssocMsg::Close)),
        ))
        .spacing(12)
        .align_y(Alignment::Center);
    column![main, buttons].spacing(12).into()
}

/// The left side: the filter, the rules, the buttons, the keys.
fn list_side<'a>(window: &'a AssocWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let filter = field(t!("associations.filter"), &window.filter)
        .id(FILTER_FIELD)
        .on_input(|text| wrap(AssocMsg::Filter(text)));
    let active = matches!(window.focus, Focus::List | Focus::Filter);
    let shown = window.shown();
    let mut lines = Column::new().spacing(1);
    if shown.is_empty() {
        let empty = if window.rules.is_empty() {
            t!("associations.empty")
        } else {
            t!("associations.no_match")
        };
        lines = lines.push(fit::hint(empty, tokens));
    }
    for (row_at, at) in shown.iter().enumerate() {
        let cursor = window.cursor == Some(*at);
        let state = (cursor, active, theme::alternate(row_at));
        lines = lines.push(row_view(*at, &window.rules[*at], state, tokens));
    }
    let list = container(scrollable(lines).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, active));
    // The list is one stop of the ring; the window moves its cursor (`keys.rs`).
    let list = focus::Stop::choice(list, LIST_STOP_ID, None, None).id(LIST_STOP_ID);
    let chosen = window.cursor.is_some();
    let buttons = Row::new()
        .push(layout::secondary_icon_button(
            Icon::Add,
            t!("associations.new"),
            Some(wrap(AssocMsg::New)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Delete,
            t!("associations.delete"),
            chosen.then_some(wrap(AssocMsg::Delete)),
        ))
        .push(layout::secondary_icon_button(
            Icon::MoveUp,
            t!("associations.up"),
            chosen.then_some(wrap(AssocMsg::Move(true))),
        ))
        .push(layout::secondary_icon_button(
            Icon::MoveDown,
            t!("associations.down"),
            chosen.then_some(wrap(AssocMsg::Move(false))),
        ))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);
    column![filter, Element::from(list)]
        .spacing(8)
        .push(buttons)
        .push(fit::help_line(t!("associations.keys"), tokens))
        .into()
}

/// One rule: its mask and its description; a click puts the cursor on it. `state`: the cursor
/// is on it, the list has the keys, it is striped.
fn row_view<'a>(
    at: usize,
    shown: &FileAssociation,
    state: (bool, bool, bool),
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (cursor, active, alternate) = state;
    let mask = if shown.mask.trim().is_empty() {
        t!("associations.no_mask").to_owned()
    } else {
        shown.mask.clone()
    };
    let line = row![
        fit::cut(mask, theme::UI_FONT, None, 2, tokens),
        fit::cut(
            shown.description.clone(),
            theme::UI_FONT,
            Some(tokens.text_secondary),
            3,
            tokens
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let face = container(line)
        .padding([0, 6])
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, active, false, alternate));
    // focus-ring: pointer only (a row of the list, which is one stop of the ring)
    mouse_area(face).on_press(wrap(AssocMsg::Select(at))).into()
}

/// The right side: the form of the rule at the cursor; a hint while there is none.
fn form_side<'a>(window: &'a AssocWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    let Some(shown) = window.rule() else {
        return container(fit::hint(t!("associations.choose"), tokens))
            .padding(8)
            .into();
    };
    let mask = field(t!("associations.mask_placeholder"), &shown.mask)
        .id(MASK_FIELD)
        .on_input(|typed| rule(RuleMsg::Mask(typed)))
        .on_submit(wrap(AssocMsg::Save));
    let description =
        field("", &shown.description).on_input(|typed| rule(RuleMsg::Description(typed)));
    let icon = field(t!("associations.icon_placeholder"), &shown.icon)
        .on_input(|typed| rule(RuleMsg::Icon(typed)));
    let rows = Column::new()
        .spacing(8)
        .push(labeled(t!("associations.field.mask"), mask))
        .push(help(t!("associations.help.mask"), tokens))
        .push(labeled(t!("associations.field.description"), description))
        .push(labeled(t!("associations.field.icon"), icon))
        .push(commands(window, tokens));
    column![
        fit::label(t!("associations.rule_title")).font(theme::UI_FONT_STRONG),
        scrollable(rows.padding([0, 12])).height(Length::Fill),
    ]
    .spacing(10)
    .into()
}

/// Which command the form shows, the buttons that add and remove one, a named command's name, and
/// the command's own fields.
fn commands<'a>(window: &'a AssocWindow, tokens: &'static Tokens) -> Column<'a, Message> {
    let names = window.command_names();
    let options: Vec<Labeled<usize>> = names
        .iter()
        .enumerate()
        .map(|(at, name)| {
            let shown = match at {
                0 => t!("associations.open_command").to_owned(),
                _ if name.trim().is_empty() => t!("associations.unnamed").to_owned(),
                _ => name.clone(),
            };
            Labeled(at, shown)
        })
        .collect();
    let picked = choice(
        options,
        &window.command,
        |at| rule(RuleMsg::Command(at)),
        "associations.command",
    );
    let buttons = row![
        layout::secondary_icon_button(
            Icon::Add,
            t!("associations.add_command"),
            Some(rule(RuleMsg::AddCommand)),
        ),
        layout::secondary_icon_button(
            Icon::Delete,
            t!("associations.remove_command"),
            Some(rule(RuleMsg::RemoveCommand)),
        ),
    ]
    .spacing(6);
    let mut column = Column::new()
        .spacing(8)
        .push(labeled(t!("associations.field.command"), picked))
        .push(row![Space::new().width(Length::Fill), buttons]);
    if window.command == 0 {
        column = column.push(help(t!("associations.help.open"), tokens));
    } else {
        let name = names.get(window.command).map_or("", String::as_str);
        let name = field("", name).on_input(|typed| rule(RuleMsg::Name(typed)));
        column = column.push(labeled(t!("associations.field.name"), name));
    }
    column
        .push(fields(&window.action(), command, tokens))
        .push(help(t!("associations.help.file_parameter"), tokens))
}
