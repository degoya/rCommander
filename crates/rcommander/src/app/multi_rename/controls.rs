//! The dialog's small widgets: buttons, labelled rows, text fields, switches, one-line texts.

use iced::{
    Alignment, Element,
    widget::{Space, checkbox, row, text_input},
};

use super::{Field, Flag, MrMsg};
use crate::{
    app::Message,
    theme::{self, fit::line, focus, layout},
};

/// The labels' column.
const LABEL_WIDTH: f32 = 150.0;

pub(super) fn wrap(message: MrMsg) -> Message {
    Message::MultiRename(message)
}

/// A dialog button (`docs/design.md`); `None` draws it disabled.
pub(super) fn small_button<'a>(label: &'a str, message: Option<MrMsg>) -> Element<'a, Message> {
    layout::secondary_button(label, message.map(wrap))
}

pub(super) fn labelled<'a>(
    label: &'static str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![line(label).width(LABEL_WIDTH), content.into()]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

pub(super) fn indented<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![Space::new().width(LABEL_WIDTH), content.into()]
        .spacing(8)
        .into()
}

pub(super) fn field<'a>(
    value: &'a str,
    field: Field,
    placeholder: &'a str,
) -> Element<'a, Message> {
    text_input(placeholder, value)
        .id(field.id())
        .on_input(move |text| wrap(MrMsg::Text(field, text)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8])
        .into()
}

pub(super) fn field_sized(value: &str, field: Field, width: f32) -> Element<'_, Message> {
    text_input("", value)
        .id(field.id())
        .on_input(move |text| wrap(MrMsg::Text(field, text)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8])
        .width(width)
        .into()
}

pub(super) fn flag<'a>(label: &'static str, on: bool, flag: Flag) -> Element<'a, Message> {
    let check = checkbox(on)
        .label(label)
        .on_toggle(move |on| wrap(MrMsg::Flag(flag, on)))
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT);
    focus::toggle(check, label, Some(wrap(MrMsg::Flag(flag, !on))))
}
