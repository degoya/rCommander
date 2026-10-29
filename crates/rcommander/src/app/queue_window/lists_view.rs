//! Drawing the queue window's lists (RC-010-15): under the title, the shown list with its state and
//! its buttons, the saved lists with Load and Delete, the name field while one is asked for, and
//! what the last list action did; in the job buttons, "Move to list".

use iced::{
    Alignment, Element, Length,
    widget::{Column, checkbox, column, pick_list, row, text_input},
};
use rc_engine::DEFAULT_LIST;

use super::{
    QueueMsg, QueueWindow,
    lists::{
        ListChoice, ListMsg, NAME_ID, NameAction, Note, SavedChoice, list_choices, list_name,
        saved_choices,
    },
};
use crate::{
    app::Message,
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
        layout::{self, button_row, secondary_button},
    },
};

fn msg(message: ListMsg) -> Message {
    Message::Queue(QueueMsg::Lists(message))
}

/// The lists' part of the card.
pub(super) fn lists_bar<'a>(
    window: &'a QueueWindow,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let shown = window.lists.iter().find(|list| list.id == window.list);
    let choices = list_choices(&window.lists);
    let chosen = choices
        .iter()
        .find(|choice| choice.id == window.list)
        .cloned();
    let started = shown.is_some_and(|list| list.started);
    let state = if started {
        t!("queue.list.started")
    } else {
        t!("queue.list.stopped")
    };
    // The default list is neither renamed nor deleted.
    let own = (window.list != DEFAULT_LIST).then_some(());
    let start = if started {
        secondary_button(t!("queue.list.stop"), Some(msg(ListMsg::Start(false))))
    } else {
        secondary_button(t!("queue.list.start"), Some(msg(ListMsg::Start(true))))
    };
    let show = |choice: ListChoice| msg(ListMsg::Show(choice.id));
    let moves = focus::moves(&choices, chosen.as_ref(), &show);
    let picker = pick_list(choices, chosen, show)
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
        .width(220);
    let list = t!("queue.list");
    let lists = row![
        layout::caption(list, None),
        layout::keyed(focus::Stop::pick(picker, list, moves), list),
        line(state).color(tokens.text_secondary),
        start,
        secondary_button(
            t!("queue.list.new"),
            Some(msg(ListMsg::Name(NameAction::Create)))
        ),
        secondary_button(
            t!("queue.list.rename"),
            own.map(|()| msg(ListMsg::Name(NameAction::Rename)))
        ),
        secondary_button(t!("queue.list.delete"), own.map(|()| msg(ListMsg::Delete))),
        secondary_button(
            t!("queue.list.save"),
            Some(msg(ListMsg::Name(NameAction::Save)))
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .wrap();
    let saved = saved_choices(&window.saved);
    let saved_chosen = saved
        .iter()
        .find(|choice| Some(choice.id) == window.saved_choice)
        .cloned();
    let picked = window.saved_choice.map(|_| ());
    let choose = |choice: SavedChoice| msg(ListMsg::ChooseSaved(choice.id));
    let moves = focus::moves(&saved, saved_chosen.as_ref(), &choose);
    let saved_picker = pick_list(saved, saved_chosen, choose)
        .placeholder(t!("queue.saved_none"))
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
        .width(260);
    let saved_label = t!("queue.saved");
    let saved = row![
        layout::caption(saved_label, None),
        layout::keyed(
            focus::Stop::pick(saved_picker, saved_label, moves),
            saved_label
        ),
        secondary_button(t!("queue.load"), picked.map(|()| msg(ListMsg::Load))),
        secondary_button(
            t!("queue.delete_saved"),
            picked.map(|()| msg(ListMsg::DeleteSaved))
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .wrap();
    let mut bar: Column<'a, Message> = column![lists, saved].spacing(8);
    if let Some(naming) = &window.naming {
        let label = match naming.action {
            NameAction::Create => t!("queue.name.create"),
            NameAction::Rename => t!("queue.name.rename"),
            NameAction::Save => t!("queue.name.save"),
            NameAction::Gather => t!("queue.name.gather"),
        };
        let mut field = row![
            line(label),
            text_input("", &naming.text)
                .id(NAME_ID)
                .on_input(|text| msg(ListMsg::NameInput(text)))
                .on_submit(msg(ListMsg::NameSubmit))
                .font(theme::UI_FONT)
                .size(theme::TEXT_SIZE)
                .padding([6, 8])
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if naming.action == NameAction::Save {
            let restore = checkbox(naming.restore_on_start)
                .label(t!("queue.name.restore_on_start"))
                .size(14)
                .text_size(theme::TEXT_SIZE)
                .font(theme::UI_FONT)
                .on_toggle(|on| msg(ListMsg::RestoreOnStart(on)));
            field = field.push(focus::toggle(
                restore,
                t!("queue.name.restore_on_start"),
                Some(msg(ListMsg::RestoreOnStart(!naming.restore_on_start))),
            ));
        }
        let field = field
            .push(secondary_button(
                t!("queue.name.ok"),
                Some(msg(ListMsg::NameSubmit)),
            ))
            .push(secondary_button(
                t!("queue.name.cancel"),
                Some(msg(ListMsg::NameCancel)),
            ));
        bar = bar.push(field);
    }
    if window.confirm_delete_saved.is_some() {
        bar = bar.push(button_row(
            None,
            [
                secondary_button(
                    t!("queue.delete_saved_yes"),
                    Some(msg(ListMsg::ConfirmDeleteSaved)),
                ),
                secondary_button(t!("queue.keep_saved"), Some(msg(ListMsg::KeepSaved))),
            ],
            [Element::from(
                fit::prose(t!("queue.confirm_delete_saved")).color(theme::DANGER),
            )],
        ));
    }
    if let Some(note) = &window.note {
        let (text, danger) = match note {
            Note::Saved { name, left_out: 0 } => (t!("queue.note.saved", name = name), false),
            Note::Saved { name, left_out } => (
                t!("queue.note.saved_left_out", name = name, count = left_out),
                false,
            ),
            Note::Loaded {
                jobs,
                unloadable: 0,
            } => (t!("queue.note.loaded", count = jobs), false),
            Note::Loaded { jobs, unloadable } => (
                t!(
                    "queue.note.loaded_unloadable",
                    count = jobs,
                    unloadable = unloadable
                ),
                true,
            ),
            Note::Refused(code) => (error_text(code), true),
        };
        let color = if danger {
            theme::DANGER
        } else {
            tokens.text_secondary
        };
        bar = bar.push(fit::prose(text).color(color));
    }
    bar.into()
}

/// "Move to list" for the selected jobs: the lists other than the shown one; nothing happens
/// without a job selected.
pub(super) fn move_to<'a>(window: &'a QueueWindow) -> Option<Element<'a, Message>> {
    let others: Vec<_> = list_choices(&window.lists)
        .into_iter()
        .filter(|choice| choice.id != window.list)
        .collect();
    if others.is_empty() {
        return None;
    }
    let assign = |choice: ListChoice| msg(ListMsg::Assign(choice.id));
    // ArrowDown moves the jobs to the first other list, as a closed combo box chooses.
    let moves = focus::moves(&others, None, &assign);
    let picker = pick_list(others, None::<ListChoice>, assign)
        .placeholder(t!("queue.move_to"))
        .text_size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
        .width(180);
    Some(focus::Stop::pick(picker, t!("queue.move_to"), moves).into())
}

/// The title's name of the shown list.
pub(super) fn shown_name(window: &QueueWindow) -> String {
    window
        .lists
        .iter()
        .find(|list| list.id == window.list)
        .map(list_name)
        .unwrap_or_default()
}
