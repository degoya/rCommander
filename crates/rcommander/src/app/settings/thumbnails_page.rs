//! The settings' Thumbnails tab (RC-100-89): the `thumbnails.*` rows, then the disk cache - what
//! it holds and the button that clears it. The cache belongs to the window, so the button sends
//! the window's own message; no engine command, nothing for MCP.

use iced::{Element, widget::column};

use super::view::{hint, label};
use crate::{
    app::{Message, thumbnails::ThumbsMsg},
    i18n::t,
    theme::{Tokens, layout},
};

/// The disk cache's block: the bytes it holds (`None`: not measured yet) and "Clear cache".
pub(super) fn cache<'a>(bytes: Option<u64>, tokens: &'static Tokens) -> Element<'a, Message> {
    let held = match bytes {
        Some(bytes) => t!(
            "settings.thumbnails.cache_size",
            size = crate::format::size(bytes)
        ),
        None => t!("settings.thumbnails.cache_measuring").to_owned(),
    };
    let clear = Some(Message::Thumbnails(ThumbsMsg::Clear));
    column![
        label(t!("settings.thumbnails.cache")),
        hint(held, tokens),
        layout::secondary_button(t!("settings.thumbnails.clear"), clear),
    ]
    .spacing(6)
    .into()
}
