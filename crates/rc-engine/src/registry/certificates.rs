//! The FTPS certificates a person trusts (RC-070-02).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "certificates.answer",
        class: CommandClass::GuiOnly,
        summary: "Answer the question about an FTPS certificate: accept once, trust and save, or cancel",
    },
    CommandSpec {
        id: "certificates.list",
        class: CommandClass::Read,
        summary: "The trusted FTPS certificates, optionally filtered by host, subject, issuer or fingerprint",
    },
    CommandSpec {
        id: "certificates.remove",
        class: CommandClass::GuiOnly,
        summary: "Forget a trusted FTPS certificate; the next connection asks again",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// The person's answer to a certificate question (RC-070-02): the window's dialog only.
            "certificates.answer" => AnswerCertificate {
                /// The question's id ([`crate::CertificateQuestion::id`]).
                question: u64,
                answer: crate::CertificateAnswer,
            },
            "certificates.list" => ListCertificates {
                /// Only certificates whose host, subject, issuer or fingerprint contains this.
                #[serde(default)]
                filter: Option<String>,
            },
            /// The deliberate step before a changed certificate can be trusted: the window's only.
            "certificates.remove" => RemoveCertificate {
                /// The stored certificate's id ([`rc_db::TrustedCertificate::id`]).
                id: i64,
            },
        } }
    };
}
pub(super) use commands;
