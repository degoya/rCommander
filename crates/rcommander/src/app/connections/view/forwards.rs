//! Under the form of a stored SSH connection: its port forwards with Start or Stop, Edit and
//! Delete, Add, and the editor of one (`connections/forwards.rs`, RC-110-03).

use iced::{
    Alignment, Element,
    widget::{Column, checkbox, pick_list, row},
};

use super::{field, hint, label, wrap};
use crate::{
    app::{
        Message,
        connections::{
            ConnMsg, ConnectionManager,
            forwards::{FORWARD_PORT_FIELD, ForwardEdit, ForwardMsg},
        },
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The stops of the editor's choices.
const BIND_STOP: &str = "rcommander.connections.forward_bind";
const AUTO_START_STOP: &str = "rcommander.connections.forward_auto_start";

fn forward(message: ForwardMsg) -> Message {
    wrap(ConnMsg::Forward(message))
}

/// The panel; nothing while the form shows no stored SSH connection.
pub(super) fn panel<'a>(
    manager: &'a ConnectionManager,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let panel = &manager.forwards;
    panel.connection?;
    let mut lines = Column::new()
        .spacing(6)
        .push(label(t!("connections.forwards.title")).font(theme::UI_FONT_STRONG))
        .push(hint(t!("connections.forwards.reach").to_owned(), tokens));
    for view in &panel.forwards {
        let id = view.forward.id;
        let mut text = view.label.clone();
        if let Some(name) = &view.forward.name {
            text = format!("{text} ({name})");
        }
        let state = if view.running {
            t!("connections.forwards.running", count = view.connections)
        } else {
            t!("connections.forwards.stopped").to_owned()
        };
        let toggle = if view.running {
            layout::secondary_icon_button(
                Icon::Disconnect,
                t!("connections.forwards.stop"),
                Some(forward(ForwardMsg::Stop(id))),
            )
        } else {
            layout::secondary_icon_button(
                Icon::Connect,
                t!("connections.forwards.start"),
                Some(forward(ForwardMsg::Start(id))),
            )
        };
        lines = lines.push(
            row![
                fit::cut(
                    format!("{text} \u{00b7} {state}"),
                    theme::UI_FONT,
                    None,
                    1,
                    tokens
                ),
                toggle,
                layout::secondary_button(
                    t!("connections.forwards.edit"),
                    Some(forward(ForwardMsg::Edit(id)))
                ),
                layout::secondary_button(
                    t!("connections.forwards.delete"),
                    Some(forward(ForwardMsg::Delete(id)))
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    match &panel.edit {
        Some(edit) => lines = lines.push(editor(edit, panel.binds(), tokens)),
        None => {
            lines = lines.push(layout::secondary_button(
                t!("connections.forwards.add"),
                Some(forward(ForwardMsg::Add)),
            ));
        }
    }
    Some(lines.into())
}

/// The editor of one forward: the local address and port, the target, a name, the start with
/// the session, its error, Save and Cancel.
fn editor<'a>(
    edit: &'a ForwardEdit,
    binds: Vec<String>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let selected = binds.iter().find(|bind| **bind == edit.bind).cloned();
    let pick = |bind: String| forward(ForwardMsg::Bind(bind));
    let moves = focus::moves(&binds, selected.as_ref(), &pick);
    let list = pick_list(binds, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(150);
    let bind = focus::Stop::pick(list, BIND_STOP, moves).id(BIND_STOP);
    let auto_start = checkbox(edit.auto_start)
        .label(t!("connections.forwards.auto_start"))
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .on_toggle(|on| forward(ForwardMsg::AutoStart(on)));
    let toggle = forward(ForwardMsg::AutoStart(!edit.auto_start));
    let auto_start =
        focus::Stop::toggle(auto_start, AUTO_START_STOP, Some(toggle)).id(AUTO_START_STOP);
    let save = forward(ForwardMsg::Save);
    let mut block = Column::new()
        .spacing(6)
        .push(
            row![
                label(t!("connections.forwards.local")),
                bind,
                field(t!("connections.forwards.port"), &edit.port)
                    .id(FORWARD_PORT_FIELD)
                    .width(90)
                    .on_input(|text| forward(ForwardMsg::Port(text)))
                    .on_submit(save.clone()),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .push(
            row![
                label(t!("connections.forwards.target")),
                field(t!("connections.forwards.host"), &edit.host)
                    .on_input(|text| forward(ForwardMsg::Host(text)))
                    .on_submit(save.clone()),
                field(t!("connections.forwards.port"), &edit.target_port)
                    .width(90)
                    .on_input(|text| forward(ForwardMsg::TargetPort(text)))
                    .on_submit(save.clone()),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .push(
            field(t!("connections.forwards.name"), &edit.name)
                .on_input(|text| forward(ForwardMsg::Name(text)))
                .on_submit(save.clone()),
        )
        .push(auto_start);
    if let Some(error) = &edit.error {
        block = block.push(fit::prose(error.as_str()).color(theme::DANGER));
    }
    block
        .push(
            row![
                layout::secondary_button(
                    t!("connections.forwards.cancel"),
                    Some(forward(ForwardMsg::Cancel))
                ),
                layout::primary_button(t!("connections.forwards.save"), Some(save)),
            ]
            .spacing(6),
        )
        .push(hint(t!("connections.forwards.lan_hint").to_owned(), tokens))
        .into()
}
