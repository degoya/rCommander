//! The question before a large fetch (RC-080-01): a small card over the dimmed window that says
//! how much is downloaded into the temporary folder, with Download and Cancel (Enter, Escape); a
//! window of its own while the dialogs have theirs (RC-080-23, `dialog_windows/questions.rs`).

use iced::{
    Element,
    widget::{column, stack},
};

use super::{DragMsg, fetch::Fetch, wrap};
use crate::{
    app::{App, Message},
    i18n::t,
    theme::{self, Tokens, fit::label, icon::Icon, layout},
};

impl App {
    /// `window` with the size question over it while one is open and has no window of its own.
    pub(in crate::app) fn drag_layer<'a>(
        &'a self,
        window: Element<'a, Message>,
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        match self
            .drag
            .question
            .as_ref()
            .filter(|_| !self.dialog_windows.own)
        {
            Some(fetch) => stack![window, question(fetch, tokens)].into(),
            None => window,
        }
    }

    /// The size question's text and buttons while one is open, filling its window of its own.
    pub(in crate::app) fn drag_question_body(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        self.drag
            .question
            .as_ref()
            .map(|fetch| fetch_body(fetch, tokens))
    }
}

fn question<'a>(fetch: &Fetch, tokens: &'static Tokens) -> Element<'a, Message> {
    let head = Message::dialog_head(label(t!("drag.fetch_title")).font(theme::UI_FONT_STRONG));
    layout::dialog_card(
        head,
        fetch_body(fetch, tokens),
        layout::SMALL_DIALOG_WIDTH,
        tokens,
    )
}

/// The question's text and buttons: on its card, or filling a window of its own.
fn fetch_body<'a>(fetch: &Fetch, tokens: &'static Tokens) -> Element<'a, Message> {
    let count = fetch.names.len();
    let body = match fetch.size {
        Some(bytes) => t!(
            "drag.fetch_question",
            count = count,
            size = crate::format::size(bytes)
        ),
        None => t!("drag.fetch_question_unknown", count = count),
    };
    let buttons = layout::button_row(
        Some(layout::primary_button(
            t!("drag.fetch_start"),
            Some(wrap(DragMsg::Fetch(true))),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("drag.fetch_cancel"),
            Some(wrap(DragMsg::Fetch(false))),
        )],
        [],
    );
    column![
        label(body),
        label(t!("drag.fetch_note")).color(tokens.text_secondary),
        buttons,
    ]
    .spacing(10)
    .into()
}
