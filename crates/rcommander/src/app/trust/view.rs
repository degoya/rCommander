//! The drawn pieces of a trust dialog: the medium card over the dimmed window (or the window of
//! its own, RC-080-23), red when it warns; the labelled facts; the lines under them - the answer
//! on its way, why the last one failed, how many questions wait; and the one button row, where
//! the button the keyboard is on is the primary one: Enter takes it.

use iced::{
    Element, Length,
    widget::{Column, row, text},
};

use super::{Question, QuestionQueue};
use crate::{
    app::Message,
    theme::{
        self, Tokens,
        fit::{line, prose},
        icon::Icon,
        layout,
    },
};

/// The dialog's card for the window's dialog stack: `title` in its head, red when `warning`.
pub fn layer<'a>(
    title: &'static str,
    (body, warning): (Element<'a, Message>, bool),
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut title = line(title).font(theme::UI_FONT_STRONG);
    if warning {
        title = title.color(theme::DANGER);
    }
    layout::warning_card(
        Message::dialog_head(title),
        body,
        layout::MEDIUM_DIALOG_WIDTH,
        warning,
        tokens,
    )
}

/// One labelled fact `label_width` wide; the value in the terminal font, so a fingerprint reads
/// character by character, over as many lines as it needs.
pub fn detail<'a>(
    label: &'static str,
    value: impl text::IntoFragment<'a>,
    label_width: f32,
    wrapping: text::Wrapping,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    row![
        line(label).width(label_width).color(tokens.text_secondary),
        text(value)
            .size(theme::TEXT_SIZE)
            .font(theme::MONO_FONT)
            .wrapping(wrapping)
            .width(Length::Fill),
    ]
    .spacing(8)
    .into()
}

/// `body` with the lines under the facts - `busy` while the answer is on its way, the reason
/// the last one failed, `waiting` for the questions behind - and the buttons: each named by
/// `label`, sending `choose` with the shown question's id.
pub fn finish<'a, Q: Question>(
    mut body: Column<'a, Message>,
    queue: &'a QuestionQueue<Q>,
    busy: &'static str,
    waiting: impl FnOnce(usize) -> String,
    label: impl Fn(Q::Choice) -> (Option<Icon>, &'static str),
    choose: impl Fn(u64, Q::Choice) -> Message,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    if queue.busy() {
        body = body.push(prose(busy).color(tokens.text_secondary));
    }
    if let Some(note) = queue.note() {
        body = body.push(prose(note).color(theme::DANGER));
    }
    if queue.waiting() > 0 {
        body = body.push(prose(waiting(queue.waiting())).color(tokens.text_secondary));
    }
    if let Some(id) = queue.shown_id() {
        body = body.push(buttons(queue, id, label, choose));
    }
    body.spacing(10).into()
}

/// The buttons of the current step in one row at the right; the one the keyboard is on is the
/// primary.
fn buttons<'a, Q: Question>(
    queue: &QuestionQueue<Q>,
    id: u64,
    label: impl Fn(Q::Choice) -> (Option<Icon>, &'static str),
    choose: impl Fn(u64, Q::Choice) -> Message,
) -> Element<'a, Message> {
    let enabled = !queue.busy();
    let buttons = queue.buttons().iter().map(|&choice| {
        let (icon, label) = label(choice);
        let message = enabled.then(|| choose(id, choice));
        if queue.focused() == Some(choice) {
            layout::primary_icon_button(icon, label, message)
        } else {
            layout::secondary_icon_button(icon, label, message)
        }
    });
    layout::button_row(None, buttons.collect::<Vec<_>>(), [])
}
