//! `Engine::dispatch` for the `certificates` group (`registry/certificates.rs`).

/// The `certificates` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::AnswerCertificate { question, answer } => {
                $self.answer_certificate(question, answer).await
            }
            Command::ListCertificates { filter } => {
                $self.list_certificates(filter.as_deref()).await
            }
            Command::RemoveCertificate { id } => $self.remove_certificate(id).await,
        } }
    };
}
pub(crate) use arms;
