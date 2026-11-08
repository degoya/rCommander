//! A file taken apart into the objects it holds: PEM blocks, one DER object, or OpenSSH public
//! key lines. Each becomes an [`Object`] with where it was found; what cannot be read becomes a
//! damaged or unsupported object rather than an error, so one bad block does not hide the rest.

use crate::cert::{self, Certificate, Request};
use crate::der::{self, Elements, tag};
use crate::oids::encoded;
use crate::pem;
use crate::privkey::{self, PrivateKeyInfo};
use crate::pubkey::PublicKey;
use crate::ssh::{self, KeyLine};

/// What one object is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Certificate(Box<Certificate>),
    Request(Box<Request>),
    PrivateKey(PrivateKeyInfo),
    /// A `PUBLIC KEY` or `RSA PUBLIC KEY` block, or a DER SubjectPublicKeyInfo.
    PublicKey(PublicKey),
    /// An OpenSSH public key line.
    SshKey(KeyLine),
    /// Recognised, but not shown: what it is.
    Unsupported(Unsupported),
    /// Not readable: why.
    Damaged(Damage),
}

/// What is recognised but not shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// A PKCS#12 / PFX container: no parser for it is linked.
    Pkcs12,
    /// A certificate revocation list.
    Crl,
    /// A PKCS#7 or CMS structure.
    Pkcs7,
    /// A PEM block of another label, cleaned to be shown.
    Block(String),
}

/// Why an object cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    /// A PEM block without its matching `END` line.
    NoEnd,
    /// A PEM block whose Base64 does not decode.
    BadBase64,
    /// A block or DER object that does not parse as what it says.
    Unreadable(What),
    /// DER that is no certificate, request or key.
    UnknownDer,
    /// A line of a key file that is no OpenSSH public key.
    NotAKeyLine,
}

/// What a damaged object claimed to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    Certificate,
    Request,
    PublicKey,
    PrivateKey,
}

/// Where an object was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The `block`th PEM block (from 1), starting on `line` (from 1).
    Pem { block: usize, line: usize },
    /// The file is one DER object.
    Der,
    /// A line (from 1) of an OpenSSH key file.
    Line(usize),
}

/// One object and where it was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub origin: Origin,
    pub body: Body,
}

/// The objects of a file, at most `max_objects`; the flag says whether more were left out.
/// Empty when the file holds nothing this viewer knows.
#[must_use]
pub fn parse(bytes: &[u8], max_objects: usize) -> (Vec<Object>, bool) {
    if pem::has_known_block(bytes) {
        let blocks = pem::blocks(bytes, max_objects.saturating_add(1));
        let more = blocks.len() > max_objects;
        let mut objects = Vec::new();
        for (index, block) in blocks.into_iter().take(max_objects).enumerate() {
            let origin = Origin::Pem {
                block: index + 1,
                line: block.line,
            };
            for body in from_block(&block) {
                objects.push(Object { origin, body });
            }
        }
        return (objects, more);
    }
    if is_der(bytes) {
        let object = Object {
            origin: Origin::Der,
            body: from_der(bytes),
        };
        return (vec![object], false);
    }
    key_lines(bytes, max_objects)
}

/// Whether the bytes are one DER SEQUENCE that spans the whole file.
#[must_use]
pub fn is_der(bytes: &[u8]) -> bool {
    der::sequence(bytes).is_some_and(|(_, rest)| rest.is_empty())
}

/// Whether some line of the bytes is an OpenSSH public key.
#[must_use]
pub fn has_key_line(bytes: &[u8]) -> bool {
    bytes
        .split(|&byte| byte == b'\n')
        .filter_map(|line| std::str::from_utf8(line).ok())
        .any(|line| ssh::key_line(line).is_some())
}

fn from_block(block: &pem::Block) -> Vec<Body> {
    let body = match &block.body {
        Ok(body) => body,
        Err(why) => return vec![Body::Damaged(*why)],
    };
    let damaged = |what: What| vec![Body::Damaged(Damage::Unreadable(what))];
    match block.label.as_str() {
        "CERTIFICATE" | "TRUSTED CERTIFICATE" | "X509 CERTIFICATE" => cert::certificate(body)
            .map_or_else(
                || damaged(What::Certificate),
                |cert| vec![Body::Certificate(Box::new(cert))],
            ),
        "CERTIFICATE REQUEST" | "NEW CERTIFICATE REQUEST" => cert::request(body).map_or_else(
            || damaged(What::Request),
            |request| vec![Body::Request(Box::new(request))],
        ),
        "PUBLIC KEY" => PublicKey::from_spki(body).map_or_else(
            || damaged(What::PublicKey),
            |key| vec![Body::PublicKey(key)],
        ),
        "RSA PUBLIC KEY" => PublicKey::rsa_from_pkcs1(body).map_or_else(
            || damaged(What::PublicKey),
            |key| vec![Body::PublicKey(key)],
        ),
        label if label.contains("PRIVATE KEY") => {
            match privkey::from_pem(label, &block.headers, body) {
                Some(keys) if !keys.is_empty() => keys.into_iter().map(Body::PrivateKey).collect(),
                _ => damaged(What::PrivateKey),
            }
        }
        "X509 CRL" => vec![Body::Unsupported(Unsupported::Crl)],
        "PKCS7" | "CMS" => vec![Body::Unsupported(Unsupported::Pkcs7)],
        label => vec![Body::Unsupported(Unsupported::Block(crate::show::clean(
            label,
        )))],
    }
}

/// One DER object: a certificate, a request, a PKCS#12 container, a public or a private key.
fn from_der(bytes: &[u8]) -> Body {
    if let Some(cert) = cert::certificate(bytes) {
        return Body::Certificate(Box::new(cert));
    }
    if let Some(request) = cert::request(bytes) {
        return Body::Request(Box::new(request));
    }
    if is_pkcs12(bytes) {
        return Body::Unsupported(Unsupported::Pkcs12);
    }
    if let Some(key) = PublicKey::from_spki(bytes) {
        return Body::PublicKey(key);
    }
    for label in ["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"] {
        if let Some(mut keys) = privkey::from_pem(label, &[], bytes)
            && let Some(key) = keys.pop()
        {
            return Body::PrivateKey(key);
        }
    }
    Body::Damaged(Damage::UnknownDer)
}

/// `PFX ::= SEQUENCE { version 3, authSafe ContentInfo { data or signedData, ... }, ... }`.
fn is_pkcs12(bytes: &[u8]) -> bool {
    let Some((content, _)) = der::sequence(bytes) else {
        return false;
    };
    let mut fields = Elements::new(content);
    let version = fields.pop(tag::INTEGER).and_then(der::small_uint);
    let content_type = fields
        .pop(tag::SEQUENCE)
        .and_then(|info| Elements::new(info).pop(tag::OID));
    version == Some(3)
        && content_type.is_some_and(|oid| oid.get(..8) == encoded::PKCS7_DATA.get(..8))
}

/// OpenSSH public key lines; blank lines and `#` comments are skipped, anything else that is no
/// key is a damaged object of its own.
fn key_lines(bytes: &[u8], max_objects: usize) -> (Vec<Object>, bool) {
    let mut objects = Vec::new();
    let mut found = false;
    for (index, line) in bytes.split(|&byte| byte == b'\n').enumerate() {
        let text = String::from_utf8_lossy(line);
        let text = text.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        if objects.len() >= max_objects {
            return (objects, true);
        }
        let body = match ssh::key_line(text) {
            Some(key) => {
                found = true;
                Body::SshKey(key)
            }
            None => Body::Damaged(Damage::NotAKeyLine),
        };
        objects.push(Object {
            origin: Origin::Line(index + 1),
            body,
        });
    }
    if !found {
        objects.clear();
    }
    (objects, false)
}

#[cfg(test)]
#[path = "object_tests.rs"]
mod tests;
