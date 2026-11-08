//! The report as data: one [`Section`] per object with its fields, each a [`Field`] and a
//! [`Value`]. The viewer puts the fields' names and the words among the values into the
//! interface's language; names, numbers, dates, fingerprints and algorithm names are shown as
//! they are. Nothing private is in a field: a private key's section has its format, algorithm,
//! encryption and the fingerprints of its public half.

use crate::cert::{Certificate, Extensions, Request};
use crate::checks::Clock;
use crate::object::{Body, Damage, Object, Origin, Unsupported};
use crate::privkey::{NoPublic, PrivateKeyInfo};
use crate::pubkey::PublicKey;
use crate::show;
use crate::ssh::KeyLine;

/// What an object is: its section's heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Certificate,
    Request,
    PrivateKey,
    PublicKey,
    SshKey,
    Unsupported,
    Damaged,
}

/// The fields a section may have, in the order they are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    FoundAt,
    Subject,
    Issuer,
    Serial,
    Version,
    NotBefore,
    NotAfter,
    DaysLeft,
    AltNames,
    KeyUsage,
    ExtendedKeyUsage,
    BasicConstraints,
    RequestedExtensions,
    Format,
    Algorithm,
    Encrypted,
    Type,
    PublicKey,
    PublicKeySha256,
    SshFingerprint,
    SignatureAlgorithm,
    Sha1Fingerprint,
    Sha256Fingerprint,
    SubjectKeyId,
    AuthorityKeyId,
    Comment,
    Options,
}

/// A field's value, or the value beside a section's heading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Shown as it is: a name, a fingerprint, a technical name.
    Text(String),
    Number(i64),
    /// A Unix time, shown as `YYYY-MM-DD HH:MM:SS UTC`.
    Date(i64),
    Origin(Origin),
    /// An algorithm (`RSA`, `EC P-256`) and its size in bits.
    Key {
        algorithm: String,
        bits: Option<u32>,
    },
    /// No public key could be read.
    NoKey,
    /// Why a private key's public half is not shown.
    NoPublic(NoPublic),
    /// Not encrypted (`None`), or encrypted with this cipher.
    Encrypted(Option<String>),
    BasicConstraints {
        ca: bool,
        path_length: Option<u32>,
    },
    /// A request that asks for no extension.
    NoExtensions,
    Unsupported(Unsupported),
    Damaged(Damage),
}

/// One object's section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// The object's number, from 1, as the checks name it.
    pub number: usize,
    pub kind: Kind,
    /// Beside the heading: a certificate's common name, a key's algorithm, ...
    pub value: Option<Value>,
    pub fields: Vec<(Field, Value)>,
}

/// The sections of `objects`, numbered from 1.
#[must_use]
pub fn sections(objects: &[Object], clock: Clock) -> Vec<Section> {
    objects
        .iter()
        .enumerate()
        .map(|(index, object)| section(index + 1, object, clock))
        .collect()
}

fn section(number: usize, object: &Object, clock: Clock) -> Section {
    let mut fields = vec![(Field::FoundAt, Value::Origin(object.origin))];
    let (kind, value) = match &object.body {
        Body::Certificate(cert) => {
            certificate(cert, clock, &mut fields);
            let name = cert
                .common_name
                .clone()
                .unwrap_or_else(|| cert.subject.clone());
            (Kind::Certificate, Some(Value::Text(name)))
        }
        Body::Request(request) => {
            self::request(request, &mut fields);
            let name = request
                .common_name
                .clone()
                .unwrap_or_else(|| request.subject.clone());
            (Kind::Request, Some(Value::Text(name)))
        }
        Body::PrivateKey(key) => {
            private_key(key, &mut fields);
            (Kind::PrivateKey, key.algorithm.clone().map(Value::Text))
        }
        Body::PublicKey(key) => {
            key_fields(Some(key), &mut fields);
            (Kind::PublicKey, Some(key.describe()))
        }
        Body::SshKey(line) => {
            ssh_key(line, &mut fields);
            (Kind::SshKey, Some(Value::Text(line.kind.clone())))
        }
        Body::Unsupported(what) => (Kind::Unsupported, Some(Value::Unsupported(what.clone()))),
        Body::Damaged(why) => (Kind::Damaged, Some(Value::Damaged(*why))),
    };
    Section {
        number,
        kind,
        value,
        fields,
    }
}

fn text(fields: &mut Vec<(Field, Value)>, field: Field, value: &str) {
    fields.push((field, Value::Text(value.to_owned())));
}

fn certificate(cert: &Certificate, clock: Clock, fields: &mut Vec<(Field, Value)>) {
    text(fields, Field::Subject, &cert.subject);
    text(fields, Field::Issuer, &cert.issuer);
    text(fields, Field::Serial, &cert.serial);
    fields.push((Field::Version, Value::Number(i64::from(cert.version))));
    fields.push((Field::NotBefore, Value::Date(cert.not_before)));
    fields.push((Field::NotAfter, Value::Date(cert.not_after)));
    let left = show::days_between(clock.now, cert.not_after).max(0);
    fields.push((Field::DaysLeft, Value::Number(left)));
    extensions(&cert.extensions, fields);
    key_fields(cert.key.as_ref(), fields);
    text(fields, Field::SignatureAlgorithm, &cert.signature.name);
    text(fields, Field::Sha1Fingerprint, &cert.sha1);
    text(fields, Field::Sha256Fingerprint, &cert.sha256);
    if let Some(id) = &cert.extensions.subject_key_id {
        text(fields, Field::SubjectKeyId, &show::hex(id));
    }
    if let Some(id) = &cert.extensions.authority_key_id {
        text(fields, Field::AuthorityKeyId, &show::hex(id));
    }
}

fn request(request: &Request, fields: &mut Vec<(Field, Value)>) {
    text(fields, Field::Subject, &request.subject);
    key_fields(request.key.as_ref(), fields);
    text(fields, Field::SignatureAlgorithm, &request.signature.name);
    if request.extensions == Extensions::default() {
        fields.push((Field::RequestedExtensions, Value::NoExtensions));
    } else {
        extensions(&request.extensions, fields);
    }
}

fn extensions(extensions: &Extensions, fields: &mut Vec<(Field, Value)>) {
    if !extensions.alt_names.is_empty() {
        let names = show::clean(&extensions.alt_names.join(", "));
        fields.push((Field::AltNames, Value::Text(names)));
    }
    if let Some(usage) = &extensions.key_usage {
        text(fields, Field::KeyUsage, usage);
    }
    if let Some(usage) = &extensions.extended_key_usage {
        text(fields, Field::ExtendedKeyUsage, usage);
    }
    if let Some((ca, path_length)) = extensions.basic_constraints {
        fields.push((
            Field::BasicConstraints,
            Value::BasicConstraints { ca, path_length },
        ));
    }
}

/// A public key's fields: algorithm and size, the SubjectPublicKeyInfo's SHA-256, and the
/// OpenSSH fingerprint where OpenSSH has the algorithm.
fn key_fields(key: Option<&PublicKey>, fields: &mut Vec<(Field, Value)>) {
    let Some(key) = key else {
        fields.push((Field::PublicKey, Value::NoKey));
        return;
    };
    fields.push((Field::PublicKey, key.describe()));
    text(fields, Field::PublicKeySha256, &key.fingerprint());
    if let Some(fingerprint) = key.ssh_fingerprint() {
        fields.push((Field::SshFingerprint, Value::Text(fingerprint)));
    }
}

fn private_key(key: &PrivateKeyInfo, fields: &mut Vec<(Field, Value)>) {
    text(fields, Field::Format, key.format);
    if let Some(algorithm) = &key.algorithm {
        text(fields, Field::Algorithm, algorithm);
    }
    fields.push((Field::Encrypted, Value::Encrypted(key.encrypted.clone())));
    match &key.public {
        Some(public) => key_fields(Some(public), fields),
        None => fields.push((
            Field::PublicKey,
            Value::NoPublic(key.no_public.unwrap_or(NoPublic::NotStored)),
        )),
    }
    if key.public.is_none()
        && let Some(fingerprint) = &key.ssh_fingerprint
    {
        text(fields, Field::SshFingerprint, fingerprint);
    }
}

fn ssh_key(line: &KeyLine, fields: &mut Vec<(Field, Value)>) {
    text(fields, Field::Type, &line.kind);
    if let Some(key) = &line.key {
        fields.push((Field::PublicKey, key.describe()));
    }
    text(fields, Field::SshFingerprint, &line.fingerprint);
    if let Some(comment) = &line.comment {
        text(fields, Field::Comment, comment);
    }
    if let Some(options) = &line.options {
        text(fields, Field::Options, options);
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
