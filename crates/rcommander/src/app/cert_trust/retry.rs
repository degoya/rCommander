//! The connection a certificate or plain FTP question stopped (RC-110-37): the window's own
//! attempt waits for the person's answer instead of failing in the panel's status line, and a yes
//! runs it again in the same panel.
//!
//! A window job ([`crate::app::run_job`]) or a job for the operations queue
//! ([`crate::app::file_ops::Submit`]) refused with a filed question (`ftp_plaintext_unconfirmed`,
//! `certificate_unknown`, `certificate_changed`, `certificate_expired`) comes back as
//! [`CertTrustMsg::Refused`](super::CertTrustMsg::Refused) with the question's id: no error, the
//! dialog asks. "Accept once", "Trust and save" and their plain FTP counterparts run the attempt
//! again - the refused command and those after it - while its panel still shows what it showed
//! then; a panel that moved meanwhile drops it, as "Cancel" does. An MCP call never comes here:
//! it fails with the code, and only a person decides in the window.

use rc_engine::{
    CertificateError, CertificateQuestionKind, Command, Engine, EngineError, PanelId, PanelView,
    VfsPath,
};

use crate::app::{App, Effect, file_ops::Submit};

/// How many stopped attempts the window keeps at most, as many questions as it keeps.
const MAX_ATTEMPTS: usize = 32;

/// What a stopped attempt runs again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Again {
    /// A window job: the refused command and those after it.
    Job(Vec<Command>),
    /// A job for the operations queue: a copy or a move onto a server.
    Ops(Box<Submit>),
}

/// A connection attempt waiting for the answer to its question.
#[derive(Debug)]
struct Attempt {
    question: u64,
    panel: PanelId,
    /// What the panel showed when the attempt failed; anything else at the answer drops it.
    at: (VfsPath, PanelView),
    again: Again,
}

/// The attempts the open questions stopped, oldest first.
#[derive(Debug, Default)]
pub struct Attempts(Vec<Attempt>);

/// The open question a failed connection filed for a person: an untrusted, changed or expired
/// certificate, or a plain FTP login's password in clear. `None` for any other failure and for a
/// refusal whose question is no longer open.
pub fn asked(engine: &Engine, error: &EngineError) -> Option<u64> {
    let EngineError::Certificate(refusal) = error else {
        return None;
    };
    let (host, port, fingerprint) = match refusal {
        CertificateError::Unknown {
            host,
            port,
            fingerprint,
        }
        | CertificateError::Changed {
            host,
            port,
            fingerprint,
            ..
        }
        | CertificateError::Expired {
            host,
            port,
            fingerprint,
        } => (host, *port, Some(fingerprint)),
        CertificateError::Plaintext { host, port } => (host, *port, None),
        _ => return None,
    };
    engine
        .certificate_questions()
        .into_iter()
        .find(|question| {
            question.host == *host
                && question.port == port
                && match fingerprint {
                    Some(fingerprint) => {
                        question.kind != CertificateQuestionKind::Plaintext
                            && question.fingerprint == *fingerprint
                    }
                    None => question.kind == CertificateQuestionKind::Plaintext,
                }
        })
        .map(|question| question.id)
}

impl App {
    /// A connection on `panel` the question `question` stopped: the panel's status line stays
    /// quiet - the dialog asks - and the attempt waits for the answer.
    pub(in crate::app) fn cert_refused(
        &mut self,
        question: u64,
        panel: PanelId,
        again: Again,
    ) -> Vec<Effect> {
        let state = &self.pane(panel).state;
        let at = (state.path.clone(), state.view);
        let attempts = &mut self.cert_attempts.0;
        attempts.push(Attempt {
            question,
            panel,
            at,
            again,
        });
        if attempts.len() > MAX_ATTEMPTS {
            attempts.remove(0);
        }
        Vec::new()
    }

    /// The engine took the answer to question `id`, or no longer has the question: its attempts
    /// go. With `connect` (a yes) each runs again whose panel still shows what it showed then.
    pub(super) fn cert_attempts_answered(&mut self, id: u64, connect: bool) -> Vec<Effect> {
        let (answered, kept): (Vec<Attempt>, Vec<Attempt>) =
            std::mem::take(&mut self.cert_attempts.0)
                .into_iter()
                .partition(|attempt| attempt.question == id);
        self.cert_attempts.0 = kept;
        if !connect {
            return Vec::new();
        }
        answered
            .into_iter()
            .filter(|attempt| {
                let state = &self.pane(attempt.panel).state;
                state.path == attempt.at.0 && state.view == attempt.at.1
            })
            .map(|attempt| match attempt.again {
                Again::Job(commands) => Effect::Dispatch(attempt.panel, commands),
                Again::Ops(submit) => Effect::Ops(*submit),
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
