//! Drawing the host-key dialogs: a medium card over the dimmed window that takes every click, or
//! a window of its own (RC-080-23, `dialog_windows/questions.rs`). The changed-key dialog and its
//! replace step carry a red border and, on the card, a red title. The button the keyboard is on is
//! the primary one: Enter takes it.

use iced::{
    Element, Length,
    widget::{Column, column, row, text},
};
use rc_engine::HostKeyQuestionKind;

use super::{Choice, HostKeyMsg, Shown, Step};
use crate::{
    app::{
        App, Message,
        trust::view::{detail as fact, finish, layer},
    },
    i18n::t,
    theme::{self, Tokens, fit::prose, icon::Icon},
};

/// The width of a detail's label.
const LABEL_WIDTH: f32 = 120.0;

impl App {
    /// The dialog of the shown question, a layer for the window's dialog stack.
    pub(in crate::app) fn host_key_layer(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let title = self.host_key_title()?;
        Some(layer(title, self.host_key_body(tokens)?, tokens))
    }

    /// The shown question's title: in its card's head, or in its window's title bar.
    pub(in crate::app) fn host_key_title(&self) -> Option<&'static str> {
        let dialogs = &self.host_keys;
        Some(match (&dialogs.shown()?.question.kind, dialogs.step()) {
            (HostKeyQuestionKind::Changed { .. }, Step::ConfirmReplace) => {
                t!("hostkeys.confirm_title")
            }
            (HostKeyQuestionKind::Changed { .. }, Step::Ask) => t!("hostkeys.changed_title"),
            (HostKeyQuestionKind::Unknown, _) => t!("hostkeys.unknown_title"),
        })
    }

    /// The shown question's facts and buttons, and whether the key changed - its card and its
    /// window carry the danger border then.
    pub(in crate::app) fn host_key_body(
        &self,
        tokens: &'static Tokens,
    ) -> Option<(Element<'_, Message>, bool)> {
        let dialogs = &self.host_keys;
        let shown = dialogs.shown()?;
        let warning = matches!(shown.question.kind, HostKeyQuestionKind::Changed { .. });
        let body = match (&shown.question.kind, dialogs.step()) {
            (HostKeyQuestionKind::Changed { .. }, Step::ConfirmReplace) => {
                confirm_replace(shown, tokens)
            }
            (HostKeyQuestionKind::Changed { .. }, Step::Ask) => changed(shown, tokens),
            (HostKeyQuestionKind::Unknown, _) => unknown(shown, tokens),
        };
        let body = finish(
            body,
            dialogs,
            t!("hostkeys.busy"),
            |count| t!("hostkeys.waiting", count = count),
            label,
            |id, choice| Message::HostKey(HostKeyMsg::Choose(id, choice)),
            tokens,
        );
        Some((body, warning))
    }
}

/// The first-sighting (TOFU) dialog: what to compare before trusting the key.
fn unknown<'a>(shown: &'a Shown, tokens: &'static Tokens) -> Column<'a, Message> {
    let question = &shown.question;
    let details = column![
        detail(t!("hostkeys.host"), server(shown), tokens),
        detail(
            t!("hostkeys.algorithm"),
            &question.offered.algorithm,
            tokens
        ),
        detail(
            t!("hostkeys.fingerprint"),
            &question.offered.fingerprint,
            tokens
        ),
    ]
    .spacing(6)
    .width(Length::Fill);
    let mut facts = row![details].spacing(12);
    if let Some(art) = &shown.randomart {
        facts = facts.push(mono(art.as_str()).color(tokens.text_secondary));
    }
    let mut body = column![prose(t!("hostkeys.unknown_text"))];
    // A known host offering a new key type: an impostor would do just that (RC-080-12).
    if !question.stored_types.is_empty() {
        let types = question.stored_types.join(", ");
        body = body.push(prose(t!("hostkeys.other_types", types = types)).color(theme::DANGER));
    }
    body.push(facts)
}

/// The warning about a changed key: both fingerprints, no way to say yes.
fn changed<'a>(shown: &'a Shown, tokens: &'static Tokens) -> Column<'a, Message> {
    column![
        prose(t!("hostkeys.changed_text")),
        fingerprints(shown, tokens),
    ]
}

/// The explicit replace action of a changed key.
fn confirm_replace<'a>(shown: &'a Shown, tokens: &'static Tokens) -> Column<'a, Message> {
    column![
        prose(t!("hostkeys.confirm_text")),
        fingerprints(shown, tokens),
    ]
}

/// Server, key type and the stored and the presented fingerprint of a changed key.
fn fingerprints<'a>(shown: &'a Shown, tokens: &'static Tokens) -> Element<'a, Message> {
    let question = &shown.question;
    let mut details = column![
        detail(t!("hostkeys.host"), server(shown), tokens),
        detail(
            t!("hostkeys.algorithm"),
            &question.offered.algorithm,
            tokens
        ),
    ]
    .spacing(6);
    if let HostKeyQuestionKind::Changed { stored } = &question.kind {
        details = details
            .push(detail(
                t!("hostkeys.stored_fingerprint"),
                &stored.fingerprint_sha256,
                tokens,
            ))
            .push(detail(
                t!("hostkeys.stored_since"),
                &stored.added_at,
                tokens,
            ));
    }
    details
        .push(detail(
            t!("hostkeys.offered_fingerprint"),
            &question.offered.fingerprint,
            tokens,
        ))
        .into()
}

/// `host:port` of the question.
fn server(shown: &Shown) -> String {
    format!("{}:{}", shown.question.host, shown.question.port)
}

/// One labelled fact, the value broken anywhere.
fn detail<'a>(
    label: &'static str,
    value: impl text::IntoFragment<'a>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    fact(label, value, LABEL_WIDTH, text::Wrapping::Glyph, tokens)
}

/// A button's icon and words.
fn label(choice: Choice) -> (Option<Icon>, &'static str) {
    match choice {
        Choice::ConnectOnce => (Some(Icon::Connect), t!("hostkeys.connect_once")),
        Choice::TrustAndSave => (Some(Icon::Save), t!("hostkeys.trust_and_save")),
        Choice::Cancel => (Some(Icon::Close), t!("hostkeys.cancel")),
        Choice::Replace => (None, t!("hostkeys.replace")),
        Choice::ConfirmReplace => (None, t!("hostkeys.confirm_replace")),
        Choice::Back => (None, t!("hostkeys.back")),
    }
}

fn mono<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content)
        .size(theme::TEXT_SIZE)
        .font(theme::MONO_FONT)
        .wrapping(text::Wrapping::None)
}
