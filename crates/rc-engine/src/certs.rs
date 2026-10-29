//! The decisions about FTPS certificates that wait for a person (RC-070-02), the counterpart of
//! the SSH host-key questions (`hostkeys.rs`).
//!
//! Every FTPS connection gets the certificate stored for its server and, once, the one a person
//! accepted for a single session ([`Engine::certificate_trust`]). A handshake the trust refused
//! fails the connection with `certificate_unknown`, `certificate_changed` or
//! `certificate_expired` - the fingerprint in the error - and becomes a [`CertificateQuestion`],
//! announced as [`EngineEvent::CertificateQuestion`] ([`Engine::certificate_refused`]). The GUI's
//! dialog answers it with `certificates.answer` (accept once, trust and save, cancel), and the
//! person connects again; an "accept once" is spent by that connection.
//!
//! A changed certificate takes no yes at all: the stored one is removed first, on purpose and
//! apart (`certificates.remove`), and the next connection asks as for a new server. An expired
//! one that was trusted takes an "accept once", never a silent yes. Neither the answer nor the
//! removal has an MCP tool: an agent sees the coded error and lists the stored certificates.

use rc_db::TrustedCertificate;
use rc_remote::{
    ftp::{
        CertificateInfo, CertificateRejection, CertificateVerdict, PinnedCertificates,
        certificate_verdict, describe_certificate, unix_now,
    },
    hostkey::normalize_host,
};
use serde::{Deserialize, Serialize};

use crate::{
    Engine, EngineError, EngineEvent,
    question_desk::{Question, QuestionDesk},
};

/// A certificate a person has to decide about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateQuestion {
    /// The id the answer names.
    pub id: u64,
    /// The host as the connection named it (lower case).
    pub host: String,
    pub port: u16,
    /// SHA-256 of the presented certificate, `AB:CD:...`.
    pub fingerprint: String,
    /// Why the system roots refused it; empty when they vouched for it (a changed certificate).
    pub reason: String,
    /// Subject, issuer and validity; `None` for a certificate that does not parse.
    pub info: Option<CertificateInfo>,
    pub kind: CertificateQuestionKind,
}

/// Why the certificate needs a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CertificateQuestionKind {
    /// Nothing is stored for this server: the first-contact dialog.
    Unknown,
    /// Another certificate is stored: a hard stop; only removing the stored one goes on.
    Changed { stored: TrustedCertificate },
    /// The stored certificate, past its validity: a warning; "accept once" or cancel.
    Expired { stored: TrustedCertificate },
}

/// The person's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificateAnswer {
    /// Accept exactly this certificate for the next session to this server, store nothing.
    AcceptOnce,
    /// Store it as the server's trusted certificate.
    TrustAndSave,
    /// Do not connect.
    Cancel,
}

/// Why a certificate decision failed; each with a stable code.
#[derive(Debug, thiserror::Error)]
pub enum CertificateError {
    /// The server's certificate is not trusted yet. The fingerprint is public - it is what a
    /// person compares - and only the app can trust it.
    #[error(
        "the FTPS server {host}:{port} presented a certificate that is not trusted ({fingerprint}); \
         a person has to decide about it in the rCommander window"
    )]
    Unknown {
        host: String,
        port: u16,
        fingerprint: String,
    },
    /// Another certificate is stored for the server: stopped.
    #[error(
        "the FTPS server {host}:{port} presented another certificate ({fingerprint}) than the \
         stored one ({stored_fingerprint}); the connection is stopped, only a person can remove \
         the stored certificate in the rCommander window"
    )]
    Changed {
        host: String,
        port: u16,
        fingerprint: String,
        stored_fingerprint: String,
    },
    /// The stored certificate's validity ended.
    #[error(
        "the trusted certificate of the FTPS server {host}:{port} ({fingerprint}) has expired; a \
         person has to decide about it in the rCommander window"
    )]
    Expired {
        host: String,
        port: u16,
        fingerprint: String,
    },
    /// No open certificate question has this id.
    #[error("no open certificate question {question}")]
    QuestionNotFound { question: u64 },
    /// "Trust and save" for a certificate whose subject and validity cannot be read.
    #[error("the certificate cannot be read; it can be accepted once, not stored")]
    Unreadable,
    /// No trusted certificate has this id.
    #[error("no trusted certificate {id}")]
    NotFound { id: i64 },
}

impl CertificateError {
    /// Every code. `certificate_unknown` is also the FTP client's own.
    pub const ALL_CODES: &[&str] = &[
        "certificate_unknown",
        "certificate_changed",
        "certificate_expired",
        "certificate_question_not_found",
        "certificate_unreadable",
        "trusted_certificate_not_found",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unknown { .. } => rc_remote::ftp::CERTIFICATE_UNKNOWN,
            Self::Changed { .. } => "certificate_changed",
            Self::Expired { .. } => "certificate_expired",
            Self::QuestionNotFound { .. } => "certificate_question_not_found",
            Self::Unreadable => "certificate_unreadable",
            Self::NotFound { .. } => "trusted_certificate_not_found",
        }
    }
}

/// The open certificate questions and the "accept once" grants not used yet.
pub(crate) type CertificateDesk = QuestionDesk<CertificateQuestion>;

impl Question for CertificateQuestion {
    fn id(&self) -> u64 {
        self.id
    }

    fn set_id(&mut self, id: u64) {
        self.id = id;
    }

    /// The same server, certificate and kind; the reason and details the first one had stay.
    fn same_as(&self, other: &Self) -> bool {
        self.host == other.host
            && self.port == other.port
            && self.fingerprint == other.fingerprint
            && self.kind == other.kind
    }

    fn grant(&self) -> (&str, u16, &str) {
        (&self.host, self.port, &self.fingerprint)
    }
}

impl Engine {
    /// The trust of a new FTPS session to `host` on `port`: the certificate stored for it and the
    /// "accept once" waiting for it (spent here), and the stored row's id.
    ///
    /// # Errors
    ///
    /// The database fails.
    pub(crate) async fn certificate_trust(
        &self,
        host: &str,
        port: u16,
    ) -> Result<(PinnedCertificates, Option<i64>), EngineError> {
        let host = normalize_host(host);
        let stored = self.db()?.trusted_certificate_for(&host, port).await?;
        let id = stored.as_ref().map(|stored| stored.id);
        let once = self.inner.certificates.take_allowance_for(&host, port);
        Ok((PinnedCertificates::new(stored, once), id))
    }

    /// What a connection whose handshake refused `rejection` fails with: the coded error, after
    /// filing a [`CertificateQuestion`] for the GUI. Public so a test can file one.
    pub async fn certificate_refused(&self, rejection: CertificateRejection) -> EngineError {
        let host = normalize_host(&rejection.host);
        let stored = match self.db() {
            Ok(db) => match db.trusted_certificate_for(&host, rejection.port).await {
                Ok(stored) => stored,
                Err(error) => return error.into(),
            },
            Err(error) => return error,
        };
        let kind = match certificate_verdict(stored.as_ref(), &rejection, unix_now()) {
            CertificateVerdict::Unknown => CertificateQuestionKind::Unknown,
            CertificateVerdict::Changed { stored } => CertificateQuestionKind::Changed { stored },
            CertificateVerdict::Expired { stored } => CertificateQuestionKind::Expired { stored },
            // Trusted meanwhile (another window stored it): no question, the plain refusal.
            CertificateVerdict::Trusted => {
                return rc_remote::FtpError::CertificateUnknown(Box::new(rejection)).into();
            }
        };
        let question = self.inner.certificates.ask(CertificateQuestion {
            id: 0,
            host,
            port: rejection.port,
            info: describe_certificate(&rejection.end_entity),
            fingerprint: rejection.fingerprint,
            reason: rejection.reason,
            kind,
        });
        let error = question_error(&question);
        self.publish(EngineEvent::CertificateQuestion {
            question: Box::new(question),
        });
        error
    }

    /// The certificate questions no one has answered yet, oldest first.
    #[must_use]
    pub fn certificate_questions(&self) -> Vec<CertificateQuestion> {
        self.inner.certificates.open()
    }
}

/// The error a connection fails with for `question`.
pub(crate) fn question_error(question: &CertificateQuestion) -> EngineError {
    let (host, port, fingerprint) = (
        question.host.clone(),
        question.port,
        question.fingerprint.clone(),
    );
    EngineError::Certificate(match &question.kind {
        CertificateQuestionKind::Unknown => CertificateError::Unknown {
            host,
            port,
            fingerprint,
        },
        CertificateQuestionKind::Changed { stored } => CertificateError::Changed {
            host,
            port,
            fingerprint,
            stored_fingerprint: stored.fingerprint_sha256.clone(),
        },
        CertificateQuestionKind::Expired { .. } => CertificateError::Expired {
            host,
            port,
            fingerprint,
        },
    })
}

#[cfg(test)]
#[path = "certs_tests.rs"]
pub(crate) mod tests;
