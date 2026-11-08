//! X.509 certificates and certificate requests (PKCS#10), read by x509-parser and copied into
//! owned values the report and the checks use. The signature is named, never verified. Key
//! usages are named as RFC 5280 does (`digitalSignature`, `serverAuth`): technical names, the
//! same in every language.

use std::net::{Ipv4Addr, Ipv6Addr};

use sha1::Sha1;
use sha2::{Digest as _, Sha256};
use x509_parser::certification_request::X509CertificationRequest;
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::prelude::FromDer as _;
use x509_parser::x509::{AlgorithmIdentifier, X509Name};

use crate::oids::{self, Strength};
use crate::pubkey::PublicKey;
use crate::show;

/// The most names of a SAN extension shown.
const MAX_NAMES: usize = 256;

/// The extensions both kinds show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extensions {
    pub alt_names: Vec<String>,
    pub key_usage: Option<String>,
    pub extended_key_usage: Option<String>,
    /// Basic constraints: CA or not, and the path length.
    pub basic_constraints: Option<(bool, Option<u32>)>,
    pub subject_key_id: Option<Vec<u8>>,
    pub authority_key_id: Option<Vec<u8>>,
}

/// A signature algorithm as shown and judged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub strength: Strength,
}

/// One certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    pub subject: String,
    pub issuer: String,
    /// The encoded names, compared to link a chain.
    pub subject_raw: Vec<u8>,
    pub issuer_raw: Vec<u8>,
    pub common_name: Option<String>,
    pub serial: String,
    pub version: u32,
    pub not_before: i64,
    pub not_after: i64,
    pub key: Option<PublicKey>,
    pub signature: Signature,
    pub sha1: String,
    pub sha256: String,
    pub extensions: Extensions,
}

impl Certificate {
    /// Subject and issuer are the same name, and the key identifiers (where both are given)
    /// agree.
    #[must_use]
    pub fn self_signed(&self) -> bool {
        self.subject_raw == self.issuer_raw
            && match (
                &self.extensions.authority_key_id,
                &self.extensions.subject_key_id,
            ) {
                (Some(authority), Some(subject)) => authority == subject,
                _ => true,
            }
    }

    /// Basic constraints say CA.
    #[must_use]
    pub fn is_ca(&self) -> bool {
        matches!(self.extensions.basic_constraints, Some((true, _)))
    }
}

/// One certificate request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub subject: String,
    pub common_name: Option<String>,
    pub key: Option<PublicKey>,
    pub signature: Signature,
    pub extensions: Extensions,
}

/// A DER certificate; `None` when it is none.
#[must_use]
pub fn certificate(der: &[u8]) -> Option<Certificate> {
    let (_, cert) = x509_parser::parse_x509_certificate(der).ok()?;
    let raw = cert.as_raw();
    let tbs = &cert.tbs_certificate;
    Some(Certificate {
        subject: name(&tbs.subject),
        issuer: name(&tbs.issuer),
        subject_raw: tbs.subject.as_raw().to_vec(),
        issuer_raw: tbs.issuer.as_raw().to_vec(),
        common_name: common_name(&tbs.subject),
        serial: cert.raw_serial_as_string().to_uppercase(),
        version: tbs.version.0.saturating_add(1),
        not_before: tbs.validity.not_before.timestamp(),
        not_after: tbs.validity.not_after.timestamp(),
        key: PublicKey::from_spki(tbs.subject_pki.raw),
        signature: signature(&cert.signature_algorithm),
        sha1: show::hex(&Sha1::digest(raw)),
        sha256: show::hex(&Sha256::digest(raw)),
        extensions: extensions(cert.extensions().iter().map(|ext| ext.parsed_extension())),
    })
}

/// A DER certificate request; `None` when it is none.
#[must_use]
pub fn request(der: &[u8]) -> Option<Request> {
    let (_, csr) = X509CertificationRequest::from_der(der).ok()?;
    let info = &csr.certification_request_info;
    Some(Request {
        subject: name(&info.subject),
        common_name: common_name(&info.subject),
        key: PublicKey::from_spki(info.subject_pki.raw),
        signature: signature(&csr.signature_algorithm),
        extensions: csr
            .requested_extensions()
            .map(extensions)
            .unwrap_or_default(),
    })
}

fn name(name: &X509Name<'_>) -> String {
    show::clean(&name.to_string())
}

fn common_name(name: &X509Name<'_>) -> Option<String> {
    name.iter_common_name()
        .next()
        .and_then(|value| value.as_str().ok())
        .map(show::clean)
}

fn signature(algorithm: &AlgorithmIdentifier<'_>) -> Signature {
    let (name, strength) = oids::signature(&algorithm.algorithm.to_id_string());
    Signature { name, strength }
}

/// The extensions shown, from either kind's parsed list.
fn extensions<'b, 'a: 'b, I>(parsed: I) -> Extensions
where
    I: Iterator<Item = &'b ParsedExtension<'a>>,
{
    let mut out = Extensions::default();
    for extension in parsed {
        match extension {
            ParsedExtension::SubjectAlternativeName(san) => {
                let room = MAX_NAMES.saturating_sub(out.alt_names.len());
                out.alt_names
                    .extend(san.general_names.iter().take(room).map(general_name));
            }
            ParsedExtension::KeyUsage(usage) => {
                let names: Vec<&str> = [
                    (usage.digital_signature(), "digitalSignature"),
                    (usage.non_repudiation(), "nonRepudiation"),
                    (usage.key_encipherment(), "keyEncipherment"),
                    (usage.data_encipherment(), "dataEncipherment"),
                    (usage.key_agreement(), "keyAgreement"),
                    (usage.key_cert_sign(), "keyCertSign"),
                    (usage.crl_sign(), "cRLSign"),
                    (usage.encipher_only(), "encipherOnly"),
                    (usage.decipher_only(), "decipherOnly"),
                ]
                .into_iter()
                .filter(|(set, _)| *set)
                .map(|(_, name)| name)
                .collect();
                out.key_usage = Some(names.join(", "));
            }
            ParsedExtension::ExtendedKeyUsage(usage) => {
                let mut names: Vec<String> = [
                    (usage.any, "anyExtendedKeyUsage"),
                    (usage.server_auth, "serverAuth"),
                    (usage.client_auth, "clientAuth"),
                    (usage.code_signing, "codeSigning"),
                    (usage.email_protection, "emailProtection"),
                    (usage.time_stamping, "timeStamping"),
                    (usage.ocsp_signing, "OCSPSigning"),
                ]
                .into_iter()
                .filter(|(set, _)| *set)
                .map(|(_, name)| name.to_owned())
                .collect();
                names.extend(usage.other.iter().map(|oid| oid.to_id_string()));
                out.extended_key_usage = Some(names.join(", "));
            }
            ParsedExtension::BasicConstraints(constraints) => {
                out.basic_constraints = Some((constraints.ca, constraints.path_len_constraint));
            }
            ParsedExtension::SubjectKeyIdentifier(id) => out.subject_key_id = Some(id.0.to_vec()),
            ParsedExtension::AuthorityKeyIdentifier(id) => {
                out.authority_key_id = id.key_identifier.as_ref().map(|id| id.0.to_vec());
            }
            _ => {}
        }
    }
    out
}

/// One alternative name, as OpenSSL writes it (`DNS:example.com`, `IP:192.0.2.1`).
fn general_name(name: &GeneralName<'_>) -> String {
    match name {
        GeneralName::DNSName(dns) => format!("DNS:{}", show::clean(dns)),
        GeneralName::RFC822Name(mail) => format!("email:{}", show::clean(mail)),
        GeneralName::URI(uri) => format!("URI:{}", show::clean(uri)),
        GeneralName::IPAddress(bytes) => match bytes.len() {
            4 => <[u8; 4]>::try_from(*bytes).map_or_else(
                |_| show::hex(bytes),
                |ip| format!("IP:{}", Ipv4Addr::from(ip)),
            ),
            16 => <[u8; 16]>::try_from(*bytes).map_or_else(
                |_| show::hex(bytes),
                |ip| format!("IP:{}", Ipv6Addr::from(ip)),
            ),
            _ => format!("IP:{}", show::hex(bytes)),
        },
        GeneralName::DirectoryName(directory) => format!("DirName:{}", self::name(directory)),
        other => show::clean(&other.to_string()),
    }
}

#[cfg(test)]
#[path = "cert_tests.rs"]
mod tests;
