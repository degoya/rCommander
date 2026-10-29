//! The FTPS certificate dialogs (RC-070-02): the engine's [`CertificateQuestion`]s, one at a
//! time, over everything else in the window but an MCP call's question - the host-key dialogs'
//! pattern (`hostkeys.rs`).
//!
//! An unknown certificate opens the first-contact dialog: server, SHA-256 fingerprint, subject,
//! issuer, validity and why the system did not trust it, with "Accept once", "Trust and save"
//! and "Cancel". A trusted certificate past its validity opens a warning with "Accept once" and
//! "Cancel" only. A changed certificate opens a loud warning with the stored and the presented
//! fingerprint and "Cancel" alone: the stored certificate is removed on purpose, apart from this
//! dialog, before another one can be trusted. The keyboard starts on Cancel - Enter alone never
//! trusts anything.
//!
//! The answers are the engine's `GuiOnly` command `certificates.answer`; the dialog only sends
//! the choices it shows. While a question is open the dialog has the keys - Tab and the arrows
//! move between the buttons, Enter chooses, Escape cancels - and the clicks.

mod view;

use rc_engine::{
    CertificateAnswer, CertificateQuestion, CertificateQuestionKind, Chord, Command, Engine,
    EngineEvent, Outcome,
};

use super::{
    App, Effect, Message,
    trust::{Pressed, Question, QuestionQueue},
};
use crate::i18n::t;

/// The error code of an answer to a question the engine no longer has.
const QUESTION_GONE: &str = "certificate_question_not_found";

/// A button of a certificate dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Accept this certificate for the next session to the server, store nothing.
    AcceptOnce,
    /// Store it as the server's trusted certificate.
    TrustAndSave,
    /// Do not connect.
    Cancel,
}

impl Question for CertificateQuestion {
    /// One step: no dialog asks twice.
    type Step = ();
    type Choice = Choice;

    fn id(&self) -> u64 {
        self.id
    }

    fn buttons(&self, (): ()) -> &'static [Choice] {
        match &self.kind {
            CertificateQuestionKind::Unknown => {
                &[Choice::AcceptOnce, Choice::TrustAndSave, Choice::Cancel]
            }
            CertificateQuestionKind::Expired { .. } => &[Choice::AcceptOnce, Choice::Cancel],
            CertificateQuestionKind::Changed { .. } => &[Choice::Cancel],
        }
    }
}

/// The open questions and the dialog over the first of them.
pub type CertTrustDialogs = QuestionQueue<CertificateQuestion>;

/// A Unix time as the dialog shows it, `YYYY-MM-DD HH:MM UTC`.
pub fn date(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0).map_or_else(
        || seconds.to_string(),
        |time| time.format("%Y-%m-%d %H:%M UTC").to_string(),
    )
}

/// What a dialog's controls and the engine's answer say.
#[derive(Debug, Clone)]
pub enum CertTrustMsg {
    /// A button of the dialog of question `id`; a click on a dialog no longer shown is dropped.
    Choose(u64, Choice),
    /// The engine's answer to question `id` for `server` (`host:port`): the outcome, or the
    /// code of the failure. Boxed like the host-key dialog's: an outcome is large.
    Finished {
        id: u64,
        server: String,
        result: Box<Result<Outcome, &'static str>>,
    },
}

/// An answer of the dialog for the engine.
pub struct CertTrustJob {
    question: u64,
    /// `host:port`, for the status bar: the question may be closed before the reply arrives.
    server: String,
    answer: CertificateAnswer,
}

impl CertTrustJob {
    /// Runs the answer: off the window's thread in the app, inline in the tests.
    pub async fn run(self, engine: Engine) -> CertTrustMsg {
        let result = engine
            .dispatch(Command::AnswerCertificate {
                question: self.question,
                answer: self.answer,
            })
            .await
            .map_err(|error| error.code());
        CertTrustMsg::Finished {
            id: self.question,
            server: self.server,
            result: Box::new(result),
        }
    }
}

impl App {
    /// The questions the engine files and closes, and - while one is open - the keys and clicks,
    /// which belong to its dialog. `None` for a message the dialogs leave to the rest.
    pub(super) fn cert_trust_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::CertificateQuestion { question }) => {
                self.cert_trust.ask(question.as_ref().clone());
                Some(Vec::new())
            }
            Message::Engine(EngineEvent::CertificateQuestionClosed { question }) => {
                self.cert_trust.close(*question);
                Some(Vec::new())
            }
            Message::EngineLagged => {
                self.cert_trust.resync(self.engine.certificate_questions());
                None
            }
            _ if self.cert_trust.shown().is_none() => None,
            Message::Key(chord) => Some(self.cert_trust_key(chord)),
            // A key a terminal or a prompt under the dialog took runs nothing else either.
            Message::KeyCaptured(_) | Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// A message of the dialogs.
    pub(super) fn on_cert_trust(&mut self, message: CertTrustMsg) -> Vec<Effect> {
        match message {
            CertTrustMsg::Choose(id, choice) => {
                if self.cert_trust.shown_id() == Some(id) {
                    return self.cert_trust_choice(choice);
                }
            }
            CertTrustMsg::Finished { id, server, result } => {
                self.cert_trust_finished(id, &server, *result);
            }
        }
        Vec::new()
    }

    /// A key while a dialog is open: Tab and the arrows move, Enter chooses, Escape cancels.
    fn cert_trust_key(&mut self, chord: &Chord) -> Vec<Effect> {
        match self.cert_trust.key(chord) {
            Some(Pressed::Escape) => self.cert_trust_choice(Choice::Cancel),
            Some(Pressed::Chosen(choice)) => self.cert_trust_choice(choice),
            None => Vec::new(),
        }
    }

    /// `choice` on the shown question - only a button its dialog shows.
    fn cert_trust_choice(&mut self, choice: Choice) -> Vec<Effect> {
        let dialogs = &mut self.cert_trust;
        let Some(shown) = dialogs.answerable(choice) else {
            return Vec::new();
        };
        let id = shown.id;
        let server = format!("{}:{}", shown.host, shown.port);
        let answer = match choice {
            Choice::AcceptOnce => CertificateAnswer::AcceptOnce,
            Choice::TrustAndSave => CertificateAnswer::TrustAndSave,
            Choice::Cancel => CertificateAnswer::Cancel,
        };
        dialogs.answering(id);
        vec![Effect::CertTrust(CertTrustJob {
            question: id,
            server,
            answer,
        })]
    }

    /// The engine's answer: the question goes and the status bar says what happened, or the
    /// dialog stays with the reason. The engine's close event may have come first.
    fn cert_trust_finished(
        &mut self,
        id: u64,
        server: &str,
        result: Result<Outcome, &'static str>,
    ) {
        let result = result.map(|outcome| match outcome {
            Outcome::CertificateAnswered {
                answer: CertificateAnswer::AcceptOnce,
                ..
            } => t!("certs.info_once", host = server),
            Outcome::CertificateAnswered {
                answer: CertificateAnswer::TrustAndSave,
                ..
            } => t!("certs.info_saved", host = server),
            // A cancel, the only answer left.
            _ => t!("certs.info_cancelled", host = server),
        });
        if let Some(info) = self.cert_trust.finished(id, result, QUESTION_GONE) {
            self.info = Some(info);
        }
    }
}

#[cfg(test)]
#[path = "cert_trust_tests.rs"]
mod tests;
