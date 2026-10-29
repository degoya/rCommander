//! Under the form of a stored connection: what opens on an SSH server, and the certificate
//! trusted for an FTPS server (`connections/remote.rs`).

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, button, row},
};
use rc_engine::ConnectionId;

use super::{hint, wrap};
use crate::{
    app::{
        Message,
        connections::{ConnMsg, ConnectionManager},
    },
    i18n::t,
    theme::{Tokens, focus, icon::Icon, layout},
};

/// Open in panel, Terminal and Run command for the SSH connection `id`.
pub(super) fn ssh_actions<'a>(id: ConnectionId) -> Element<'a, Message> {
    let action = |icon: Option<Icon>, text: &'static str, message: ConnMsg| {
        let message = wrap(message);
        let face = button(layout::marked_icon_button(icon, text))
            .padding([4, 10])
            .style(button::secondary)
            .on_press(message.clone());
        layout::keyed(focus::Stop::button(face, text, Some(message)), text)
    };
    row![
        Space::new().width(Length::Fill),
        action(
            Some(Icon::Open),
            t!("connections.open_files"),
            ConnMsg::Files(id)
        ),
        action(
            Some(Icon::Terminal),
            t!("connections.open_terminal"),
            ConnMsg::Terminal(id)
        ),
        action(None, t!("connections.run_command"), ConnMsg::RunCommand(id)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// The certificates trusted for the FTPS connection `id`'s server, each with Forget; a line that
/// says none is trusted yet.
pub(super) fn certificates<'a>(
    manager: &'a ConnectionManager,
    id: ConnectionId,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let trusted = manager.certificates_of(id);
    if trusted.is_empty() {
        return hint(t!("connections.certificate_none").to_owned(), tokens).into();
    }
    let mut lines = Column::new().spacing(4);
    for stored in trusted {
        let text = t!(
            "connections.certificate",
            fingerprint = stored.fingerprint_sha256,
            since = stored.added_at
        );
        lines = lines.push(
            row![
                hint(text, tokens).width(Length::Fill),
                focus::button(
                    button(layout::icon_button(
                        Icon::Delete,
                        t!("connections.forget_certificate")
                    ))
                    .padding([4, 10])
                    .style(button::secondary)
                    .on_press(wrap(ConnMsg::ForgetCertificate(stored.id))),
                    t!("connections.forget_certificate"),
                    Some(wrap(ConnMsg::ForgetCertificate(stored.id))),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    lines.into()
}
