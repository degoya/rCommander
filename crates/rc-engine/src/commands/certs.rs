//! The `certificates.*` commands: the FTPS certificates a person trusted (list, remove) and the
//! answers to the questions of `crate::certs`.

use rc_db::NewTrustedCertificate;

use crate::{
    Engine, EngineError, EngineEvent, Outcome,
    certs::{
        CertificateAnswer, CertificateError, CertificateQuestion, CertificateQuestionKind,
        question_error,
    },
};

impl Engine {
    /// `certificates.list`: every trusted certificate, or those whose host, subject, issuer or
    /// fingerprint contains `filter` (ignoring case).
    pub(crate) async fn list_certificates(
        &self,
        filter: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let mut certificates = self.db()?.trusted_certificates().await?;
        if let Some(filter) = filter.map(str::trim).filter(|filter| !filter.is_empty()) {
            let needle = filter.to_lowercase();
            certificates.retain(|stored| {
                [
                    &stored.host,
                    &stored.subject,
                    &stored.issuer,
                    &stored.fingerprint_sha256,
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(&needle))
            });
        }
        Ok(Outcome::TrustedCertificates { certificates })
    }

    /// `certificates.remove`: forgets a trusted certificate; the next connection asks again.
    /// The deliberate step before a changed certificate can be trusted.
    pub(crate) async fn remove_certificate(&self, id: i64) -> Result<Outcome, EngineError> {
        let certificate = self
            .db()?
            .remove_trusted_certificate(id)
            .await?
            .ok_or(EngineError::Certificate(CertificateError::NotFound { id }))?;
        self.publish(EngineEvent::TrustedCertificatesChanged);
        Ok(Outcome::TrustedCertificateRemoved { certificate })
    }

    /// `certificates.answer`: the dialog's answer. A changed certificate takes no yes - its
    /// question stays open - and an expired one only "accept once".
    pub(crate) async fn answer_certificate(
        &self,
        id: u64,
        answer: CertificateAnswer,
    ) -> Result<Outcome, EngineError> {
        let question = self
            .inner
            .certificates
            .get(id)
            .ok_or(EngineError::Certificate(
                CertificateError::QuestionNotFound { question: id },
            ))?;
        let stored = match (&question.kind, answer) {
            (_, CertificateAnswer::Cancel) => None,
            (CertificateQuestionKind::Changed { .. }, _)
            | (CertificateQuestionKind::Expired { .. }, CertificateAnswer::TrustAndSave) => {
                return Err(question_error(&question));
            }
            (_, CertificateAnswer::AcceptOnce) => {
                self.inner.certificates.allow_once(&question);
                None
            }
            (CertificateQuestionKind::Unknown, CertificateAnswer::TrustAndSave) => {
                Some(self.store_certificate(&question).await?)
            }
        };
        self.inner.certificates.close(id);
        self.publish(EngineEvent::CertificateQuestionClosed { question: id });
        Ok(Outcome::CertificateAnswered {
            question: id,
            answer,
            stored,
        })
    }

    /// Stores the certificate of a first-contact question; one stored since for the server is
    /// never overwritten.
    async fn store_certificate(
        &self,
        question: &CertificateQuestion,
    ) -> Result<rc_db::TrustedCertificate, EngineError> {
        let info = question
            .info
            .as_ref()
            .ok_or(EngineError::Certificate(CertificateError::Unreadable))?;
        let db = self.db()?;
        let new = NewTrustedCertificate {
            host: question.host.clone(),
            port: question.port,
            fingerprint_sha256: question.fingerprint.clone(),
            subject: info.subject.clone(),
            issuer: info.issuer.clone(),
            not_after: info.not_after,
        };
        if let Some(stored) = db.add_trusted_certificate(&new).await? {
            self.publish(EngineEvent::TrustedCertificatesChanged);
            return Ok(stored);
        }
        match db
            .trusted_certificate_for(&question.host, question.port)
            .await?
        {
            Some(stored) if stored.fingerprint_sha256 == question.fingerprint => Ok(stored),
            Some(stored) => Err(EngineError::Certificate(CertificateError::Changed {
                host: question.host.clone(),
                port: question.port,
                fingerprint: question.fingerprint.clone(),
                stored_fingerprint: stored.fingerprint_sha256,
            })),
            // Removed again in between: nothing to compare with, try once more.
            None => db
                .add_trusted_certificate(&new)
                .await?
                .ok_or(EngineError::Certificate(
                    CertificateError::QuestionNotFound {
                        question: question.id,
                    },
                )),
        }
    }
}

#[cfg(test)]
#[path = "certs_tests.rs"]
mod tests;
