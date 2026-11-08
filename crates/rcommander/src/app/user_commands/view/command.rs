//! The command's fields both forms share (RC-100-86): what it is - a program, internal rows or
//! another own command -, the command, and for a program its parameters, start folder, window and
//! "Run as administrator" (`*` in Total Commander), each with a line of help; and the building
//! blocks of both forms - the internal associations' window (RC-100-91) shares them. Each control
//! is a stop of the focus ring.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, button, checkbox, container, pick_list, row, text_input},
};
use rc_engine::{CommandKind, CommandRef, Show};

use super::wrap;
use crate::{
    app::{
        Message,
        user_commands::{UcMsg, form::CmdMsg},
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The width of the labels in front of the controls.
const LABEL_WIDTH: f32 = 140.0;

/// A value and its text in a list.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::app) struct Labeled<T>(pub T, pub String);

impl<T> fmt::Display for Labeled<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.1)
    }
}

/// A one-line text field of the dialog's look.
pub(in crate::app) fn field<'a>(
    placeholder: &str,
    value: &str,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8])
}

/// `control` behind its label.
pub(in crate::app) fn labeled<'a>(
    label: &'static str,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        fit::label(label).width(LABEL_WIDTH),
        container(control).width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// A line of help under a control, in from the labels.
pub(in crate::app) fn help<'a>(
    words: &'static str,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    indented(fit::help_line(words, tokens))
}

/// `control` under the controls, in from the labels: a checkbox that carries its own label.
fn indented<'a>(control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![Space::new().width(LABEL_WIDTH), control.into()]
        .spacing(8)
        .into()
}

/// A choice among `options` with `current` chosen, a stop of the ring named `label`: the arrows
/// choose the option before and after, as a closed combo box does.
pub(in crate::app) fn choice<'a, T>(
    options: Vec<Labeled<T>>,
    current: &T,
    on_select: impl Fn(T) -> Message + 'a,
    label: &str,
) -> Element<'a, Message>
where
    T: Clone + PartialEq + 'a,
{
    let selected = options.iter().find(|option| option.0 == *current).cloned();
    let pick = move |picked: Labeled<T>| on_select(picked.0);
    let moves = focus::moves(&options, selected.as_ref(), &pick);
    let list = pick_list(options, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(260);
    focus::Stop::pick(list, label, moves).into()
}

/// A small button with its icon, a stop of the focus ring with the access key its title marks;
/// `None` draws it disabled.
pub(super) fn small<'a>(
    icon: Option<Icon>,
    title: &'static str,
    message: Option<UcMsg>,
) -> Element<'a, Message> {
    let message = message.map(wrap);
    let face = button(layout::marked_icon_button(icon, title))
        .padding([4, 10])
        .style(button::secondary)
        .on_press_maybe(message.clone());
    layout::keyed(focus::Stop::button(face, title, message), title)
}

/// The key of what the form shows and "Add key..." (`key` its key as the palette prints it);
/// nothing for a new one, whose name the key would take before it is stored.
pub(super) fn key_row<'a>(stored: bool, key: Option<String>) -> Option<Element<'a, Message>> {
    if !stored {
        return None;
    }
    let shown = key.unwrap_or_else(|| t!("user_commands.key_none").to_owned());
    let line = row![
        fit::label(shown).width(Length::Fill),
        layout::secondary_button(t!("user_commands.add_key"), Some(wrap(UcMsg::AddKey))),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    Some(labeled(t!("user_commands.field.key"), line))
}

/// The command's fields over `action`, each change through `to`: the kind, the command - with
/// what it holds for the kind -, and for a program its parameters, folder, window and elevation.
pub(in crate::app) fn fields<'a>(
    action: &CommandRef,
    to: fn(CmdMsg) -> Message,
    tokens: &'static Tokens,
) -> Column<'a, Message> {
    let kinds = vec![
        Labeled(
            CommandKind::Program,
            t!("user_commands.kind.program").to_owned(),
        ),
        Labeled(
            CommandKind::Internal,
            t!("user_commands.kind.internal").to_owned(),
        ),
        Labeled(CommandKind::Own, t!("user_commands.kind.own").to_owned()),
    ];
    let kind = choice(
        kinds,
        &action.kind,
        move |kind| to(CmdMsg::Kind(kind)),
        "user_commands.kind",
    );
    let command = field("", &action.command).on_input(move |typed| to(CmdMsg::Command(typed)));
    let command_help = match action.kind {
        CommandKind::Program => t!("user_commands.help.program"),
        CommandKind::Internal => t!("user_commands.help.internal"),
        CommandKind::Own => t!("user_commands.help.own"),
    };
    let mut column = Column::new()
        .spacing(8)
        .push(labeled(t!("user_commands.field.kind"), kind))
        .push(labeled(t!("user_commands.field.command"), command))
        .push(help(command_help, tokens));
    if action.kind != CommandKind::Program {
        return column;
    }
    let params = field("", &action.params).on_input(move |typed| to(CmdMsg::Params(typed)));
    let folder = field(t!("user_commands.folder_placeholder"), &action.folder)
        .on_input(move |typed| to(CmdMsg::Folder(typed)));
    let shows = vec![
        Labeled(Show::Normal, t!("user_commands.show.normal").to_owned()),
        Labeled(
            Show::Minimized,
            t!("user_commands.show.minimized").to_owned(),
        ),
        Labeled(
            Show::Maximized,
            t!("user_commands.show.maximized").to_owned(),
        ),
    ];
    let show = choice(
        shows,
        &action.show,
        move |show| to(CmdMsg::Show(show)),
        "user_commands.show",
    );
    let label = t!("user_commands.elevated");
    let on = action.elevated;
    let elevated = checkbox(on)
        .label(label)
        .on_toggle(move |on| to(CmdMsg::Elevated(on)))
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT);
    let elevated = focus::toggle(elevated, label, Some(to(CmdMsg::Elevated(!on))));
    column = column
        .push(labeled(t!("user_commands.field.params"), params))
        .push(help(t!("user_commands.help.params"), tokens))
        .push(labeled(t!("user_commands.field.folder"), folder))
        .push(labeled(t!("user_commands.field.show"), show))
        .push(indented(elevated));
    if cfg!(not(windows)) {
        column = column.push(help(t!("user_commands.help.elevated_windows_only"), tokens));
    }
    column
}
