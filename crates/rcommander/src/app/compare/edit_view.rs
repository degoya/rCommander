//! Drawing the edit mode (RC-100-94): the one row of buttons with the edit mode's own, the
//! question that takes its place, the line under the paths, and the field a line is typed in.

use iced::{
    Alignment, Element, Length,
    widget::{row, text_input},
};
use rc_engine::compare::{CompareSide, Direction, ReadOnly};

use super::{
    Answer, Ask, CompareMsg, DiffWindow, EditMsg, Note, Shown, diff_view::button,
    edit_keys::FIELD_ID, wrap,
};
use crate::{
    app::Message,
    format,
    i18n::{error_text, t},
    theme::{self, Tokens, fit::line, icon::Icon, layout},
};

fn edit(message: EditMsg) -> CompareMsg {
    CompareMsg::Edit(message)
}

fn side_name(side: CompareSide) -> &'static str {
    match side {
        CompareSide::Left => t!("compare.side.left"),
        CompareSide::Right => t!("compare.side.right"),
    }
}

/// The row of buttons: Compare, Edit mode, the jumps and - in the edit mode - Copy ->, Copy <-,
/// Undo and Save at the left, Close at the right; the question in its place while one is asked.
pub fn buttons<'a>(window: &'a DiffWindow, tokens: &'static Tokens) -> Element<'a, Message> {
    if let Some(ask) = window.ask {
        return question(ask, tokens);
    }
    let text_view = matches!(window.shown, Shown::Text(_));
    let fresh = window.fresh().is_some();
    let editing = window.edit.is_some() && window.field.is_none();
    let session = window.session.as_ref();
    let can = |side: CompareSide| editing && fresh && session.is_some_and(|s| s.editable(side));
    let mut extra = vec![
        button(t!("compare.reload"), Some(edit(EditMsg::Reload))),
        button(
            t!("compare.edit_mode"),
            session.is_some().then_some(edit(EditMsg::Toggle)),
        ),
        button(
            t!("compare.previous"),
            text_view.then_some(CompareMsg::DiffPrevious),
        ),
        button(
            t!("compare.next"),
            text_view.then_some(CompareMsg::DiffNext),
        ),
    ];
    if window.edit.is_some() {
        let undo = editing && session.is_some_and(|s| s.can_undo());
        extra.extend([
            button(
                t!("compare.copy_right"),
                can(CompareSide::Right).then_some(edit(EditMsg::Copy(Direction::LeftToRight))),
            ),
            button(
                t!("compare.copy_left"),
                can(CompareSide::Left).then_some(edit(EditMsg::Copy(Direction::RightToLeft))),
            ),
            button(
                t!("compare.undo"),
                undo.then_some(edit(EditMsg::Undo(true))),
            ),
            button(
                t!("compare.save"),
                window.dirty().then_some(edit(EditMsg::Save)),
            ),
        ]);
    }
    layout::button_row(
        None,
        [layout::secondary_icon_button(
            Icon::Close,
            t!("compare.close"),
            Some(wrap(CompareMsg::DiffClose)),
        )],
        extra,
    )
}

/// A question over the buttons: its text and its answers, the first the default.
fn question<'a>(ask: Ask, tokens: &'static Tokens) -> Element<'a, Message> {
    let answer = |answer| Some(wrap(edit(EditMsg::Answer(answer))));
    let (text, first, second) = match ask {
        Ask::Unsaved(_) => (
            t!("compare.ask_unsaved").to_owned(),
            layout::primary_button(t!("compare.answer_save"), answer(Answer::Save)),
            layout::secondary_button(t!("compare.answer_discard"), answer(Answer::Discard)),
        ),
        Ask::Changed(side) => (
            t!("compare.ask_changed", side = side_name(side)),
            layout::primary_button(t!("compare.answer_overwrite"), answer(Answer::Overwrite)),
            layout::secondary_button(t!("compare.answer_reload"), answer(Answer::Reload)),
        ),
    };
    let cancel = layout::secondary_button(t!("compare.answer_cancel"), answer(Answer::Cancel));
    let label: Element<'a, Message> = line(text).color(tokens.accent).into();
    layout::button_row(Some(first), [second, cancel], [label])
}

/// What the edit mode says under the paths: the cursor's line and side, changes not saved, a
/// side that stays read-only, a save that failed.
pub fn note<'a>(window: &DiffWindow, _tokens: &'static Tokens) -> Option<Element<'a, Message>> {
    let text = match window.note {
        Some(Note::ReadOnly(side, why)) => {
            let side = side_name(side);
            match why {
                ReadOnly::Archive => t!("compare.read_only.archive", side = side),
                ReadOnly::TooLarge => t!("compare.read_only.too_large", side = side),
                ReadOnly::NotLossless => t!("compare.read_only.not_lossless", side = side),
            }
        }
        Some(Note::Unmappable(side, unmappable)) => t!(
            "compare.unmappable",
            line = format::count(unmappable.line as u64 + 1),
            side = side_name(side),
            character = unmappable.character
        ),
        Some(Note::Failed(code)) => t!("compare.save_failed", error = error_text(code)),
        None => return None,
    };
    Some(line(text).color(theme::DANGER).into())
}

/// The cursor's place and whether changes are not saved, in the edit mode.
pub fn status(window: &DiffWindow) -> Option<String> {
    let cursor = window.edit?;
    let mut text = t!(
        "compare.edit_status",
        row = format::count(cursor.row as u64 + 1),
        side = side_name(cursor.side)
    );
    if window.dirty() {
        text.push_str(" - ");
        text.push_str(t!("compare.unsaved"));
    }
    Some(text)
}

/// The field the cursor's line is typed in, while it is.
pub fn field<'a>(window: &'a DiffWindow, tokens: &'static Tokens) -> Option<Element<'a, Message>> {
    let field = window.field.as_ref()?;
    let input = text_input("", &field.text)
        .id(FIELD_ID)
        .font(theme::MONO_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8])
        .on_input(|text| wrap(edit(EditMsg::Field(text))))
        .on_submit(wrap(edit(EditMsg::Commit)))
        .width(Length::Fill);
    let label = if field.insert {
        t!("compare.field_insert")
    } else {
        t!("compare.field_change")
    };
    Some(
        row![line(label).color(tokens.text_secondary), input]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
    )
}
