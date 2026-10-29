//! Drawing a drive's context menu and the network drive dialogs (RC-010-16).
//!
//! The context menu is a card where the drive menu opens, over the panel it was opened for,
//! titled with the drive; a click beside it closes it. A dialog is a small card over the dimmed
//! window that takes every click (`theme::layout`), or a window of its own (RC-080-23,
//! `dialog_windows.rs`).

use iced::{
    Alignment, Element, Length,
    mouse::Interaction,
    widget::{Space, column, container, mouse_area, pick_list, row, stack, text_input},
};
use rc_engine::PanelId;

use super::{
    DriveMessage,
    actions::{DriveAction, DriveContext},
    network::{MapForm, NetworkDialog, NetworkForm, NetworkMsg, Phase, REMOTE_FIELD, UnmapForm},
    view::{MENU_TOP, card_style, display_name},
};
use crate::{
    app::{App, Message, dialog_windows::DialogWindow},
    i18n::t,
    theme::{
        self, Tokens,
        fit::{self, line, prose},
        focus,
        icon::Icon,
        layout,
    },
};

const CONTEXT_WIDTH: f32 = 300.0;
const LABEL_WIDTH: f32 = 120.0;

/// `base` with the open context menu or dialog over it.
pub(super) fn with_network<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    // In a window of its own (RC-080-23), the dialog draws there.
    let dialog = app.drive_bar.dialog.as_ref();
    if let Some(dialog) = dialog.filter(|_| !app.windowed(DialogWindow::Network)) {
        let card = body(dialog, tokens);
        let layer = Message::overlay(title(dialog), card, layout::SMALL_DIALOG_WIDTH, tokens);
        return stack![base, layer].into();
    }
    match &app.drive_bar.context {
        Some(context) => stack![base, context_layer(app, context, tokens)].into(),
        None => base,
    }
}

/// The dialog's title: in its card's head, or in its window's title bar.
pub fn title(dialog: &NetworkDialog) -> &'static str {
    match dialog.form {
        NetworkForm::Map(_) => t!("network.map_title"),
        NetworkForm::Unmap(_) => t!("network.unmap_title"),
    }
}

/// The dialog's fields and buttons: on its card, or filling a window of its own.
pub fn body<'a>(dialog: &'a NetworkDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    match &dialog.form {
        NetworkForm::Map(form) => map_card(form, tokens),
        NetworkForm::Unmap(form) => unmap_card(form, tokens),
    }
}

/// An action as a row of the drive menu or of a context menu; `drive` is the context menu's.
pub(super) fn action_row<'a>(
    action: DriveAction,
    panel: PanelId,
    drive: Option<String>,
    cursor: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let line = row![
        line(action_symbol(action)).width(22),
        line(action.label()).width(Length::Fill),
    ]
    .spacing(6)
    .padding([0, 8])
    .align_y(Alignment::Center);
    let face = container(line)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(move |_| theme::row(tokens, cursor, true, false));
    // focus-ring: pointer only (a row of the drive menu, which the arrows move through)
    mouse_area(face)
        .on_press(Message::Drive(DriveMessage::Run(panel, action, drive)))
        .interaction(Interaction::Pointer)
        .into()
}

fn action_symbol(action: DriveAction) -> &'static str {
    match action {
        DriveAction::MapNetwork => "\u{2795}",
        DriveAction::UnmapNetwork => "\u{2716}",
    }
}

/// The context menu on its panel's side, under the drive bar.
fn context_layer<'a>(
    app: &'a App,
    context: &'a DriveContext,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let title = app
        .drive_bar
        .drives
        .iter()
        .find(|drive| drive.id == context.drive)
        .map_or_else(|| context.drive.clone(), display_name);
    let mut items: Vec<Element<'a, Message>> = vec![
        container(line(title).color(tokens.text_secondary))
            .padding([4, 8])
            .into(),
    ];
    items.extend(context.actions.iter().enumerate().map(|(index, action)| {
        action_row(
            *action,
            context.panel,
            Some(context.drive.clone()),
            index == context.cursor,
            tokens,
        )
    }));
    let card = container(column(items).padding(4))
        .width(CONTEXT_WIDTH)
        .style(move |_| card_style(tokens));
    let beside = Space::new().width(Length::FillPortion(1));
    let card = container(card).width(Length::FillPortion(1));
    let placed = match context.panel {
        PanelId::Left => row![card, beside],
        PanelId::Right => row![beside, card],
    };
    // focus-ring: pointer only (the layer that closes the menu)
    mouse_area(
        container(placed)
            .padding(iced::Padding {
                top: MENU_TOP,
                right: 16.0,
                bottom: 16.0,
                left: 16.0,
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::Drive(DriveMessage::CloseMenu))
    .on_right_press(Message::Drive(DriveMessage::CloseMenu))
    .into()
}

fn map_card<'a>(form: &'a MapForm, tokens: &'static Tokens) -> Element<'a, Message> {
    let editing = form.phase != Phase::Running;
    let mut remote = text_input(t!("network.folder_hint"), &form.remote)
        .id(REMOTE_FIELD)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([4, 8]);
    if editing {
        remote = remote.on_input(|typed| network(NetworkMsg::Remote(typed)));
    }
    let letter: Element<'a, Message> = if form.letters.is_empty() {
        prose(t!("network.no_letter"))
            .color(tokens.text_secondary)
            .into()
    } else {
        let choose = |letter| network(NetworkMsg::Letter(letter));
        let list = pick_list(form.letters.as_slice(), form.letter, choose)
            .text_size(theme::TEXT_SIZE)
            .width(90);
        let drive = t!("network.letter");
        layout::keyed(
            focus::Stop::pick(
                list,
                drive,
                focus::moves(&form.letters, form.letter.as_ref(), &choose),
            ),
            drive,
        )
    };
    let reconnect = layout::check(t!("network.reconnect"), form.reconnect, editing, |on| {
        network(NetworkMsg::Reconnect(on))
    });
    let folder = t!("network.folder");
    let remote = layout::keyed(focus::Stop::field(remote, folder), folder);
    let mut body = column![
        labelled(folder, remote, tokens),
        labelled(t!("network.letter"), letter, tokens),
        reconnect,
        prose(t!("network.map_note")).color(tokens.text_secondary),
    ]
    .spacing(8);
    body = push_phase(body, &form.phase, t!("network.waiting"), tokens);
    let connect = (editing && form.letter.is_some()).then_some(NetworkMsg::Confirm);
    body.push(buttons(t!("network.connect"), connect)).into()
}

fn unmap_card<'a>(form: &'a UnmapForm, tokens: &'static Tokens) -> Element<'a, Message> {
    let editing = form.phase != Phase::Running;
    let mut body = column![].spacing(8);
    if form.drives.is_empty() {
        body = body.push(prose(t!("network.none")).color(tokens.text_secondary));
        return body.push(buttons(t!("network.disconnect"), None)).into();
    }
    let rows = form.drives.iter().enumerate().map(|(index, (id, remote))| {
        let face = container(
            row![
                line(id.as_str()).width(40),
                fit::path_line(remote.as_str(), tokens.text, tokens)
            ]
            .spacing(8)
            .padding([0, 8])
            .align_y(Alignment::Center),
        )
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(move |_| theme::row(tokens, index == form.cursor, true, false));
        // focus-ring: pointer only (a row of the drives choice stop)
        mouse_area(face)
            .on_press(network(NetworkMsg::Choose(index)))
            .interaction(Interaction::Pointer)
            .into()
    });
    // The drives are one stop of the ring; the dialog moves the choice with the arrows.
    body = body.push(focus::choice(column(rows), "network.drives", None, None));
    let (question, confirm) = match form.chosen() {
        Some(drive) if form.in_use => (
            t!("network.in_use", drive = drive),
            t!("network.disconnect_anyway"),
        ),
        _ => (
            t!("network.unmap_text").to_owned(),
            t!("network.disconnect"),
        ),
    };
    let question = prose(question);
    body = body.push(if form.in_use {
        question.color(theme::DANGER)
    } else {
        question
    });
    body = push_phase(body, &form.phase, t!("network.disconnecting"), tokens);
    body.push(buttons(confirm, editing.then_some(NetworkMsg::Confirm)))
        .into()
}

/// What the dialog is waiting for, or why the last try failed.
fn push_phase<'a>(
    body: iced::widget::Column<'a, Message>,
    phase: &'a Phase,
    waiting: &'static str,
    tokens: &'static Tokens,
) -> iced::widget::Column<'a, Message> {
    match phase {
        Phase::Editing => body,
        Phase::Running => body.push(prose(waiting).color(tokens.text_secondary)),
        Phase::Failed(error) => body.push(prose(error.as_str()).color(theme::DANGER)),
    }
}

/// The confirming button (the primary, disabled without `message`) and Cancel, on the right.
fn buttons<'a>(confirm: &'static str, message: Option<NetworkMsg>) -> Element<'a, Message> {
    layout::button_row(
        Some(layout::primary_button(confirm, message.map(network))),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("network.cancel"),
            Some(network(NetworkMsg::Close)),
        )],
        [],
    )
}

fn labelled<'a>(
    label: &'static str,
    control: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    row![
        container(layout::caption(label, Some(tokens.text_secondary))).width(LABEL_WIDTH),
        control
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn network(message: NetworkMsg) -> Message {
    Message::Drive(DriveMessage::Network(message))
}
