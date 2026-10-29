//! The access keys of a dialog's controls beside its buttons (RC-080-16), marked in the catalogue
//! as the buttons' are (`&` before the letter, `theme::mnemonic`): a checkbox underlines the key
//! in its label while it shows and toggles on it ([`check`]); a text field, a pick list or an
//! option group answers to the key of the caption beside it ([`caption`], [`keyed`]) - the key
//! gives it the keyboard, as in Windows. Only catalogue texts come here, never a name from the
//! user's data (a connection, a script), so no `&` of a name becomes a key.

use iced::{
    Alignment, Color, Element,
    widget::{checkbox, mouse_area, row},
};

use super::{face, marked_label, tinted_label};
use crate::theme::{
    TEXT_SIZE, UI_FONT, focus,
    icon::Icon,
    mnemonic::{self, Label},
};

/// The box of a dialog's checkbox (`docs/design.md`: 14, its label at 13).
const CHECK_SIZE: f32 = 14.0;
/// The space between the box and its label, as the checkbox keeps it.
const CHECK_SPACING: f32 = CHECK_SIZE / 2.0;

/// A dialog's checkbox, a stop of the ring, with `label` beside it read for its access key: the
/// key toggles it, as Space does. `toggle` makes the message of a value; while not `enabled` the
/// checkbox is greyed out and no stop. While the key shows, the label is a text of its own (a
/// checkbox draws plain text only), and a click on it toggles as one on the box does.
pub fn check<'a, Message: Clone + 'a>(
    label: &'a str,
    on: bool,
    enabled: bool,
    toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    let marked = Label::parse(label);
    let letter = marked.letter;
    let toggled = enabled.then(|| toggle(!on));
    let mut boxed = checkbox(on).size(CHECK_SIZE);
    if enabled {
        boxed = boxed.on_toggle(toggle);
    }
    let content: Element<'a, Message> = if mnemonic::shown() && marked.split().is_some() {
        let text = marked_label(marked, true);
        let text = match toggled.clone() {
            Some(message) => mouse_area(text).on_press(message).into(),
            None => text,
        };
        row![boxed, text]
            .spacing(CHECK_SPACING)
            .align_y(Alignment::Center)
            .into()
    } else {
        boxed
            .label(marked.text)
            .text_size(TEXT_SIZE)
            .font(UI_FONT)
            .into()
    };
    focus::Stop::toggle(content, label, toggled)
        .letter(letter)
        .into()
}

/// The caption of a text field, a pick list or an option group: `label` without its mark, in
/// `color` (the text colour round it without one), its access key underlined while it shows. The
/// control answers to the key through [`keyed`].
pub fn caption<'a, Message: 'a>(label: &'a str, color: Option<Color>) -> Element<'a, Message> {
    tinted_label(Label::parse(label), mnemonic::shown(), color)
}

/// `stop` answering to the access key that `label` - its caption - marks: a text field
/// ([`focus::Stop::field`]), a pick list or an option group ([`focus::Stop::choice`]) gets the
/// keyboard by it; a button ([`focus::Stop::button`]) is pressed.
pub fn keyed<'a, Message: Clone + 'a>(
    stop: focus::Stop<'a, Message>,
    label: &str,
) -> Element<'a, Message> {
    stop.letter(Label::parse(label).letter).into()
}

/// The face of a button in a style of its own (a settings page's): `icon`, then `label` read for
/// its access key, underlined while it shows; the button's stop takes the key through [`keyed`].
pub fn marked_icon_button<'a, Message: 'a>(
    icon: impl Into<Option<Icon>>,
    label: &'a str,
) -> Element<'a, Message> {
    face(icon, marked_label(Label::parse(label), mnemonic::shown()))
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;
