//! A certificate report put into words (RC-110-08): `rc_cert`'s checks and sections as lines of
//! a label and a value in the interface's language, and those lines as the report's text. Names,
//! numbers, fingerprints and algorithm names stay as the file has them; dates are UTC.

use std::fmt::Write as _;

use rc_cert::{
    Report,
    checks::{Finding, Level, Signed},
    object::{Damage, Origin, Unsupported, What},
    privkey::NoPublic,
    pubkey::Weakness,
    report::{Field, Kind, Value},
    show::date,
};

use crate::i18n::t;

/// One line of a section: its label and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub value: String,
}

/// One section in words: the checks, or one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub value: Option<String>,
    pub lines: Vec<Line>,
}

/// The report's sections in words: the checks first, then each object.
pub fn sections(report: &Report) -> Vec<Section> {
    let checks = Section {
        title: t!("viewer.cert.checks").to_owned(),
        value: Some(level(report.worst()).to_owned()),
        lines: report
            .checks
            .iter()
            .map(|check| Line {
                label: level(check.level).to_owned(),
                value: finding(&check.finding),
            })
            .collect(),
    };
    let objects = report.sections.iter().map(|section| Section {
        title: t!(
            "viewer.cert.title",
            number = section.number,
            kind = kind(section.kind)
        ),
        value: section.value.as_ref().map(value),
        lines: section
            .fields
            .iter()
            .map(|(name, shown)| Line {
                label: field(*name).to_owned(),
                value: value(shown),
            })
            .collect(),
    });
    std::iter::once(checks).chain(objects).collect()
}

/// The sections as text: a heading line, then each line with its label padded to the longest.
pub fn text(sections: &[Section]) -> String {
    let width = sections
        .iter()
        .flat_map(|section| &section.lines)
        .map(|line| line.label.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&section.title);
        if let Some(value) = &section.value {
            out.push_str(" - ");
            out.push_str(value);
        }
        out.push('\n');
        for line in &section.lines {
            // Writing to a String cannot fail.
            let _ = writeln!(out, "  {:<width$} {}", line.label, line.value);
        }
    }
    out
}

fn level(level: Level) -> &'static str {
    match level {
        Level::Ok => t!("viewer.cert.level.ok"),
        Level::Info => t!("viewer.cert.level.info"),
        Level::Warn => t!("viewer.cert.level.warn"),
        Level::Error => t!("viewer.cert.level.error"),
    }
}

fn kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Certificate => t!("viewer.cert.kind.certificate"),
        Kind::Request => t!("viewer.cert.kind.request"),
        Kind::PrivateKey => t!("viewer.cert.kind.private_key"),
        Kind::PublicKey => t!("viewer.cert.kind.public_key"),
        Kind::SshKey => t!("viewer.cert.kind.ssh_key"),
        Kind::Unsupported => t!("viewer.cert.kind.unsupported"),
        Kind::Damaged => t!("viewer.cert.kind.damaged"),
    }
}

fn field(field: Field) -> &'static str {
    match field {
        Field::FoundAt => t!("viewer.cert.field.found_at"),
        Field::Subject => t!("viewer.cert.field.subject"),
        Field::Issuer => t!("viewer.cert.field.issuer"),
        Field::Serial => t!("viewer.cert.field.serial"),
        Field::Version => t!("viewer.cert.field.version"),
        Field::NotBefore => t!("viewer.cert.field.not_before"),
        Field::NotAfter => t!("viewer.cert.field.not_after"),
        Field::DaysLeft => t!("viewer.cert.field.days_left"),
        Field::AltNames => t!("viewer.cert.field.alt_names"),
        Field::KeyUsage => t!("viewer.cert.field.key_usage"),
        Field::ExtendedKeyUsage => t!("viewer.cert.field.extended_key_usage"),
        Field::BasicConstraints => t!("viewer.cert.field.basic_constraints"),
        Field::RequestedExtensions => t!("viewer.cert.field.requested_extensions"),
        Field::Format => t!("viewer.cert.field.format"),
        Field::Algorithm => t!("viewer.cert.field.algorithm"),
        Field::Encrypted => t!("viewer.cert.field.encrypted"),
        Field::Type => t!("viewer.cert.field.type"),
        Field::PublicKey => t!("viewer.cert.field.public_key"),
        Field::PublicKeySha256 => t!("viewer.cert.field.public_key_sha256"),
        Field::SshFingerprint => t!("viewer.cert.field.ssh_fingerprint"),
        Field::SignatureAlgorithm => t!("viewer.cert.field.signature_algorithm"),
        Field::Sha1Fingerprint => t!("viewer.cert.field.sha1"),
        Field::Sha256Fingerprint => t!("viewer.cert.field.sha256"),
        Field::SubjectKeyId => t!("viewer.cert.field.subject_key_id"),
        Field::AuthorityKeyId => t!("viewer.cert.field.authority_key_id"),
        Field::Comment => t!("viewer.cert.field.comment"),
        Field::Options => t!("viewer.cert.field.options"),
    }
}

fn value(value: &Value) -> String {
    match value {
        Value::Text(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Date(seconds) => date(*seconds),
        Value::Origin(Origin::Pem { block, line }) => {
            t!("viewer.cert.origin.pem", block = block, line = line)
        }
        Value::Origin(Origin::Der) => t!("viewer.cert.origin.der").to_owned(),
        Value::Origin(Origin::Line(line)) => t!("viewer.cert.origin.line", line = line),
        Value::Key {
            algorithm,
            bits: Some(bits),
        } => t!("viewer.cert.key_bits", algorithm = algorithm, bits = bits),
        Value::Key {
            algorithm,
            bits: None,
        } => algorithm.clone(),
        Value::NoKey => t!("viewer.cert.no_key").to_owned(),
        Value::NoPublic(why) => no_public(*why).to_owned(),
        Value::Encrypted(None) => t!("viewer.cert.encrypted.no").to_owned(),
        Value::Encrypted(Some(cipher)) => t!("viewer.cert.encrypted.yes", cipher = cipher),
        Value::BasicConstraints { ca: false, .. } => t!("viewer.cert.not_ca").to_owned(),
        Value::BasicConstraints {
            ca: true,
            path_length: None,
        } => t!("viewer.cert.ca").to_owned(),
        Value::BasicConstraints {
            ca: true,
            path_length: Some(length),
        } => t!("viewer.cert.ca_path", length = length),
        Value::NoExtensions => t!("viewer.cert.no_extensions").to_owned(),
        Value::Unsupported(what) => unsupported(what),
        Value::Damaged(why) => damage(*why).to_owned(),
    }
}

fn no_public(why: NoPublic) -> &'static str {
    match why {
        NoPublic::Encrypted => t!("viewer.cert.no_public.encrypted"),
        NoPublic::NotStored => t!("viewer.cert.no_public.not_stored"),
    }
}

fn unsupported(what: &Unsupported) -> String {
    match what {
        Unsupported::Pkcs12 => t!("viewer.cert.unsupported.pkcs12").to_owned(),
        Unsupported::Crl => t!("viewer.cert.unsupported.crl").to_owned(),
        Unsupported::Pkcs7 => t!("viewer.cert.unsupported.pkcs7").to_owned(),
        Unsupported::Block(label) => t!("viewer.cert.unsupported.block", label = label),
    }
}

fn damage(why: Damage) -> &'static str {
    match why {
        Damage::NoEnd => t!("viewer.cert.damage.no_end"),
        Damage::BadBase64 => t!("viewer.cert.damage.bad_base64"),
        Damage::Unreadable(What::Certificate) => t!("viewer.cert.damage.certificate"),
        Damage::Unreadable(What::Request) => t!("viewer.cert.damage.request"),
        Damage::Unreadable(What::PublicKey) => t!("viewer.cert.damage.public_key"),
        Damage::Unreadable(What::PrivateKey) => t!("viewer.cert.damage.private_key"),
        Damage::UnknownDer => t!("viewer.cert.damage.unknown_der"),
        Damage::NotAKeyLine => t!("viewer.cert.damage.not_a_key_line"),
    }
}

fn weakness(weakness: Weakness) -> String {
    match weakness {
        Weakness::SmallRsa(bits) => t!("viewer.cert.weak.rsa", bits = bits),
        Weakness::Dsa => t!("viewer.cert.weak.dsa").to_owned(),
        Weakness::SmallCurve(curve) => t!("viewer.cert.weak.curve", curve = curve),
    }
}

/// One finding as a sentence.
fn finding(finding: &Finding) -> String {
    match finding {
        Finding::ValidUntil {
            object,
            until,
            days,
        } => t!(
            "viewer.cert.finding.valid_until",
            object = object,
            date = date(*until),
            days = days
        ),
        Finding::ExpiresSoon {
            object,
            until,
            days,
        } => t!(
            "viewer.cert.finding.expires_soon",
            object = object,
            date = date(*until),
            days = days
        ),
        Finding::Expired {
            object,
            until,
            days,
        } => t!(
            "viewer.cert.finding.expired",
            object = object,
            date = date(*until),
            days = days
        ),
        Finding::NotYetValid { object, from, days } => t!(
            "viewer.cert.finding.not_yet_valid",
            object = object,
            date = date(*from),
            days = days
        ),
        Finding::SelfSignedRoot { object } => {
            t!("viewer.cert.finding.self_signed_root", object = object)
        }
        Finding::IssuedBy { object, issuer } => t!(
            "viewer.cert.finding.issued_by",
            object = object,
            issuer = issuer
        ),
        Finding::IssuerBefore { object, issuer } => t!(
            "viewer.cert.finding.issuer_before",
            object = object,
            issuer = issuer
        ),
        Finding::IssuerNotCa { object, issuer } => t!(
            "viewer.cert.finding.issuer_not_ca",
            object = object,
            issuer = issuer
        ),
        Finding::KeyIdMismatch { object, issuer } => t!(
            "viewer.cert.finding.key_id_mismatch",
            object = object,
            issuer = issuer
        ),
        Finding::IssuerMissing {
            object,
            name,
            broken: false,
        } => t!(
            "viewer.cert.finding.issuer_missing",
            object = object,
            name = name
        ),
        Finding::IssuerMissing {
            object,
            name,
            broken: true,
        } => t!(
            "viewer.cert.finding.issuer_missing_broken",
            object = object,
            name = name
        ),
        Finding::WeakSignature {
            object,
            signed,
            algorithm,
            broken,
        } => signature(*object, *signed, algorithm, *broken),
        Finding::KeyEncrypted { object, cipher } => t!(
            "viewer.cert.finding.key_encrypted",
            object = object,
            cipher = cipher
        ),
        Finding::KeyNotEncrypted { object } => {
            t!("viewer.cert.finding.key_not_encrypted", object = object)
        }
        Finding::KeyNotComparable { object, why } => t!(
            "viewer.cert.finding.key_not_comparable",
            object = object,
            why = no_public(*why)
        ),
        Finding::KeyBelongs {
            object,
            holder,
            signed: Signed::Certificate,
        } => t!(
            "viewer.cert.finding.key_belongs_certificate",
            object = object,
            holder = holder
        ),
        Finding::KeyBelongs {
            object,
            holder,
            signed: Signed::Request,
        } => t!(
            "viewer.cert.finding.key_belongs_request",
            object = object,
            holder = holder
        ),
        Finding::KeyMatchesNothing { object } => {
            t!("viewer.cert.finding.key_matches_nothing", object = object)
        }
        Finding::WeakKey { object, weakness } => t!(
            "viewer.cert.finding.weak_key",
            object = object,
            weakness = self::weakness(*weakness)
        ),
        Finding::Unsupported { object, what } => t!(
            "viewer.cert.finding.unsupported",
            object = object,
            what = unsupported(what)
        ),
        Finding::Damaged { object, why } => t!(
            "viewer.cert.finding.damaged",
            object = object,
            why = damage(*why)
        ),
        Finding::MoreObjects { shown } => t!("viewer.cert.finding.more_objects", shown = shown),
        Finding::SignaturesNotVerified => {
            t!("viewer.cert.finding.signatures_not_verified").to_owned()
        }
        Finding::NothingToCheck => t!("viewer.cert.finding.nothing").to_owned(),
    }
}

/// A weak (SHA-1) or broken (MD5, MD2) signature of a certificate or a request.
fn signature(object: usize, signed: Signed, algorithm: &str, broken: bool) -> String {
    match (signed, broken) {
        (Signed::Certificate, false) => t!(
            "viewer.cert.finding.weak_signature_certificate",
            object = object,
            algorithm = algorithm
        ),
        (Signed::Certificate, true) => t!(
            "viewer.cert.finding.broken_signature_certificate",
            object = object,
            algorithm = algorithm
        ),
        (Signed::Request, false) => t!(
            "viewer.cert.finding.weak_signature_request",
            object = object,
            algorithm = algorithm
        ),
        (Signed::Request, true) => t!(
            "viewer.cert.finding.broken_signature_request",
            object = object,
            algorithm = algorithm
        ),
    }
}

#[cfg(test)]
#[path = "cert_words_tests.rs"]
mod tests;
