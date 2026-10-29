//! Drawing the certificate dialogs: a medium card over the dimmed window that takes every click,
//! or a window of its own (RC-080-23, `dialog_windows/questions.rs`). The changed-certificate and
//! the expired-certificate warnings carry a red border and, on the card, a red title. The button
//! the keyboard is on is the primary one: Enter takes it.

use iced::{
    Element, Length,
    widget::{Column, column, text},
};
use rc_engine::{CertificateQuestion, CertificateQuestionKind};

use super::{CertTrustMsg, Choice, date};
use crate::{
    app::{
        App, Message,
        trust::view::{detail as fact, finish, layer},
    },
    i18n::t,
    theme::{Tokens, fit::prose, icon::Icon},
};

/// The width of a detail's label.
const LABEL_WIDTH: f32 = 130.0;

impl App {
    /// The dialog of the shown question, a layer for the window's dialog stack.
    pub(in crate::app) fn cert_trust_layer(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let title = self.cert_trust_title()?;
        Some(layer(title, self.cert_trust_body(tokens)?, tokens))
    }

    /// The shown question's title: in its card's head, or in its window's title bar.
    pub(in crate::app) fn cert_trust_title(&self) -> Option<&'static str> {
        Some(match &self.cert_trust.shown()?.kind {
            CertificateQuestionKind::Unknown => t!("certs.unknown_title"),
            CertificateQuestionKind::Changed { .. } => t!("certs.changed_title"),
            CertificateQuestionKind::Expired { .. } => t!("certs.expired_title"),
        })
    }

    /// The shown question's facts and buttons, and whether it warns - a changed or an expired
    /// certificate: its card and its window carry the danger border then.
    pub(in crate::app) fn cert_trust_body(
        &self,
        tokens: &'static Tokens,
    ) -> Option<(Element<'_, Message>, bool)> {
        let dialogs = &self.cert_trust;
        let question = dialogs.shown()?;
        let warning = !matches!(question.kind, CertificateQuestionKind::Unknown);
        let body = match &question.kind {
            CertificateQuestionKind::Unknown => t!("certs.unknown_text"),
            CertificateQuestionKind::Changed { .. } => t!("certs.changed_text"),
            CertificateQuestionKind::Expired { .. } => t!("certs.expired_text"),
        };
        let content: Column<'_, Message> = column![prose(body), details(question, tokens)];
        let content = finish(
            content,
            dialogs,
            t!("certs.busy"),
            |count| t!("certs.waiting", count = count),
            label,
            |id, choice| Message::CertTrust(CertTrustMsg::Choose(id, choice)),
            tokens,
        );
        Some((content, warning))
    }
}

/// Server, fingerprints, subject, issuer, validity and the system's reason.
fn details<'a>(question: &'a CertificateQuestion, tokens: &'static Tokens) -> Element<'a, Message> {
    let server = format!("{}:{}", question.host, question.port);
    let mut details = column![detail(t!("certs.server"), server, tokens)].spacing(6);
    match &question.kind {
        CertificateQuestionKind::Changed { stored } => {
            details = details
                .push(detail(
                    t!("certs.stored_fingerprint"),
                    &stored.fingerprint_sha256,
                    tokens,
                ))
                .push(detail(t!("certs.stored_since"), &stored.added_at, tokens))
                .push(detail(
                    t!("certs.offered_fingerprint"),
                    &question.fingerprint,
                    tokens,
                ));
        }
        CertificateQuestionKind::Unknown | CertificateQuestionKind::Expired { .. } => {
            details = details.push(detail(
                t!("certs.fingerprint"),
                &question.fingerprint,
                tokens,
            ));
        }
    }
    match &question.info {
        Some(info) => {
            details = details
                .push(detail(t!("certs.subject"), &info.subject, tokens))
                .push(detail(t!("certs.issuer"), &info.issuer, tokens))
                .push(detail(
                    t!("certs.valid_from"),
                    date(info.not_before),
                    tokens,
                ))
                .push(detail(
                    t!("certs.valid_until"),
                    date(info.not_after),
                    tokens,
                ));
        }
        None => details = details.push(prose(t!("certs.unreadable")).color(tokens.text_secondary)),
    }
    if !question.reason.is_empty() {
        details = details.push(detail(t!("certs.reason"), &question.reason, tokens));
    }
    details.width(Length::Fill).into()
}

/// One labelled fact, the value broken between words where it can.
fn detail<'a>(
    label: &'static str,
    value: impl text::IntoFragment<'a>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    fact(
        label,
        value,
        LABEL_WIDTH,
        text::Wrapping::WordOrGlyph,
        tokens,
    )
}

/// A button's icon and words.
fn label(choice: Choice) -> (Option<Icon>, &'static str) {
    match choice {
        Choice::AcceptOnce => (Some(Icon::Connect), t!("certs.accept_once")),
        Choice::TrustAndSave => (Some(Icon::Save), t!("certs.trust_and_save")),
        Choice::Cancel => (Some(Icon::Close), t!("certs.cancel")),
    }
}
