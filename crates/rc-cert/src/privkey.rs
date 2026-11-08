//! Private key files, read only for what may be shown: the format, the algorithm, whether and
//! how the key is encrypted, and its public half where the file stores it. No private number is
//! copied out of the bytes handed in, and an encrypted key is never decrypted.

use crate::der::{self, Elements, tag};
use crate::oids::{self, encoded};
use crate::pubkey::PublicKey;
use crate::ssh;

/// What is shown of one private key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateKeyInfo {
    /// `PKCS#8`, `PKCS#1`, `SEC1`, `OpenSSH`, ...: technical names, the same in every language.
    pub format: &'static str,
    /// The algorithm, where the file names it even without a public half.
    pub algorithm: Option<String>,
    /// `None` when not encrypted; else the cipher, as far as the file names it.
    pub encrypted: Option<String>,
    /// The public half, where the file stores it.
    pub public: Option<PublicKey>,
    /// The OpenSSH fingerprint of an OpenSSH key, taken from its stored public blob.
    pub ssh_fingerprint: Option<String>,
    /// Why there is no public half, when there is none.
    pub no_public: Option<NoPublic>,
}

/// Why a private key comes without its public half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoPublic {
    /// The key is encrypted, and it is not decrypted.
    Encrypted,
    /// The file does not store it, and computing it would mean computing with the private key.
    NotStored,
}

const ENCRYPTED: NoPublic = NoPublic::Encrypted;
const NOT_STORED: NoPublic = NoPublic::NotStored;

impl PrivateKeyInfo {
    fn new(format: &'static str) -> Self {
        Self {
            format,
            algorithm: None,
            encrypted: None,
            public: None,
            ssh_fingerprint: None,
            no_public: None,
        }
    }

    fn with_public(mut self, public: Option<PublicKey>, why_not: NoPublic) -> Self {
        if let Some(key) = &public {
            self.algorithm = Some(key.algorithm());
        }
        self.no_public = public.is_none().then_some(why_not);
        self.public = public;
        self
    }
}

/// A private key from a PEM block: its label, its headers and its decoded body. `None` when the
/// label is no private key's or the body is not what the label says.
#[must_use]
pub fn from_pem(
    label: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Option<Vec<PrivateKeyInfo>> {
    let legacy = headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("Proc-Type") && value.contains("ENCRYPTED"));
    let algorithm = match label {
        "PRIVATE KEY" => return pkcs8(body).map(|key| vec![key]),
        "ENCRYPTED PRIVATE KEY" => return encrypted_pkcs8(body).map(|key| vec![key]),
        "OPENSSH PRIVATE KEY" => return openssh(body),
        "RSA PRIVATE KEY" => "RSA",
        "EC PRIVATE KEY" => "EC",
        "DSA PRIVATE KEY" => "DSA",
        _ => return None,
    };
    if legacy {
        let cipher = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("DEK-Info"))
            .and_then(|(_, value)| value.split(',').next())
            .map_or_else(
                || "?".to_owned(),
                |cipher| crate::show::clean(cipher.trim()),
            );
        let mut key = PrivateKeyInfo::new("OpenSSL legacy PEM");
        key.algorithm = Some(algorithm.to_owned());
        key.encrypted = Some(cipher);
        key.no_public = Some(ENCRYPTED);
        return Some(vec![key]);
    }
    let key = match algorithm {
        "RSA" => PrivateKeyInfo::new("PKCS#1").with_public(Some(rsa_public(body)?), NOT_STORED),
        "EC" => sec1(body, None)?,
        _ => PrivateKeyInfo::new("OpenSSL DSA").with_public(Some(dsa_public(body)?), NOT_STORED),
    };
    Some(vec![key])
}

/// The public half of a PKCS#1 `RSAPrivateKey`: `SEQUENCE { version, n, e, d, ... }`, of which
/// only `n` and `e` are read.
fn rsa_public(body: &[u8]) -> Option<PublicKey> {
    let (content, _) = der::sequence(body)?;
    let mut fields = Elements::new(content);
    fields.pop(tag::INTEGER)?;
    let n = der::magnitude(fields.pop(tag::INTEGER)?).to_vec();
    let e = der::magnitude(fields.pop(tag::INTEGER)?).to_vec();
    (!n.is_empty()).then_some(PublicKey::Rsa { n, e })
}

/// The public half of OpenSSL's DSA key: `SEQUENCE { 0, p, q, g, y, x }`; `x` is not read.
fn dsa_public(body: &[u8]) -> Option<PublicKey> {
    let (content, _) = der::sequence(body)?;
    let mut fields = Elements::new(content);
    fields.pop(tag::INTEGER)?;
    let mut number = || Some(der::magnitude(fields.pop(tag::INTEGER)?).to_vec());
    Some(PublicKey::Dsa {
        p: number()?,
        q: number()?,
        g: number()?,
        y: number()?,
    })
}

/// SEC1 `ECPrivateKey`: `SEQUENCE { 1, privateKey, [0] curve OPTIONAL, [1] publicKey OPTIONAL }`.
/// `curve` comes from PKCS#8's algorithm when the key itself leaves it out.
fn sec1(body: &[u8], curve: Option<&[u8]>) -> Option<PrivateKeyInfo> {
    let (content, _) = der::sequence(body)?;
    let mut fields = Elements::new(content);
    fields.pop(tag::INTEGER)?;
    fields.pop(tag::OCTET_STRING)?;
    let mut own_curve = None;
    let mut point = None;
    for field in fields {
        match field.tag {
            0xa0 => {
                own_curve = der::read(field.content)
                    .filter(|(oid, _)| oid.tag == tag::OID)
                    .map(|(oid, _)| oid.content);
            }
            0xa1 => {
                point = der::read(field.content)
                    .filter(|(bits, _)| bits.tag == tag::BIT_STRING)
                    .and_then(|(bits, _)| der::whole_bits(bits.content));
            }
            _ => {}
        }
    }
    let curve = own_curve.or(curve);
    let public = curve.zip(point).map(|(curve, point)| PublicKey::Ec {
        curve: curve.to_vec(),
        point: point.to_vec(),
    });
    let mut key = PrivateKeyInfo::new("SEC1").with_public(public, NOT_STORED);
    if key.algorithm.is_none() {
        key.algorithm = Some("EC".to_owned());
    }
    Some(key)
}

/// PKCS#8 `PrivateKeyInfo` / `OneAsymmetricKey`: `SEQUENCE { version, algorithm, privateKey,
/// [0] attributes OPTIONAL, [1] publicKey OPTIONAL }`.
fn pkcs8(body: &[u8]) -> Option<PrivateKeyInfo> {
    let (content, _) = der::sequence(body)?;
    let mut fields = Elements::new(content);
    let version = der::small_uint(fields.pop(tag::INTEGER)?)?;
    if version > 1 {
        return None;
    }
    let mut algorithm = Elements::new(fields.pop(tag::SEQUENCE)?);
    let oid = algorithm.pop(tag::OID)?;
    let inner = fields.pop(tag::OCTET_STRING)?;
    let stored = fields
        .find(|field| field.tag == 0x81)
        .and_then(|field| der::whole_bits(field.content));
    let mut key = match oid {
        encoded::RSA => PrivateKeyInfo::new("PKCS#8").with_public(rsa_public(inner), NOT_STORED),
        encoded::EC => {
            let curve = algorithm.pop(tag::OID);
            let mut key = sec1(inner, curve)?;
            key.format = "PKCS#8";
            key
        }
        encoded::DSA => {
            let mut key = PrivateKeyInfo::new("PKCS#8").with_public(None, NOT_STORED);
            key.algorithm = Some("DSA".to_owned());
            key
        }
        encoded::ED25519 | encoded::ED448 | encoded::X25519 | encoded::X448 => {
            let public = stored.map(|bits| match oid {
                encoded::ED25519 => PublicKey::Ed25519(bits.to_vec()),
                encoded::ED448 => PublicKey::Ed448(bits.to_vec()),
                encoded::X25519 => PublicKey::X25519(bits.to_vec()),
                _ => PublicKey::X448(bits.to_vec()),
            });
            let mut key = PrivateKeyInfo::new("PKCS#8").with_public(public, NOT_STORED);
            key.algorithm = Some(edwards_name(oid).to_owned());
            key
        }
        other => {
            let mut key = PrivateKeyInfo::new("PKCS#8").with_public(None, NOT_STORED);
            key.algorithm = Some(oids::dotted(other).unwrap_or_else(|| "?".to_owned()));
            key
        }
    };
    if key.public.is_none() && key.algorithm.is_none() {
        key.algorithm = Some("RSA".to_owned());
    }
    Some(key)
}

fn edwards_name(oid: &[u8]) -> &'static str {
    match oid {
        encoded::ED25519 => "Ed25519",
        encoded::ED448 => "Ed448",
        encoded::X25519 => "X25519",
        _ => "X448",
    }
}

/// PKCS#8 `EncryptedPrivateKeyInfo`: `SEQUENCE { algorithm, encryptedData }`. The scheme is
/// named (`PBES2: PBKDF2, AES-256-CBC`); nothing is decrypted.
fn encrypted_pkcs8(body: &[u8]) -> Option<PrivateKeyInfo> {
    let (content, _) = der::sequence(body)?;
    let mut fields = Elements::new(content);
    let mut algorithm = Elements::new(fields.pop(tag::SEQUENCE)?);
    fields.pop(tag::OCTET_STRING)?;
    let scheme = algorithm.pop(tag::OID)?;
    let mut name = oids::cipher(scheme);
    if let Some(params) = algorithm.pop(tag::SEQUENCE) {
        let parts: Vec<String> = Elements::new(params)
            .filter(|part| part.tag == tag::SEQUENCE)
            .filter_map(|part| {
                der::read(part.content)
                    .filter(|(oid, _)| oid.tag == tag::OID)
                    .map(|(oid, _)| oids::cipher(oid.content))
            })
            .collect();
        if !parts.is_empty() {
            name = format!("{name}: {}", parts.join(", "));
        }
    }
    let mut key = PrivateKeyInfo::new("PKCS#8 EncryptedPrivateKeyInfo");
    key.encrypted = Some(name);
    key.no_public = Some(ENCRYPTED);
    Some(key)
}

/// The keys of an `openssh-key-v1` container; their public halves are stored unencrypted.
fn openssh(body: &[u8]) -> Option<Vec<PrivateKeyInfo>> {
    let container = ssh::container(body)?;
    Some(
        container
            .keys
            .into_iter()
            .map(|(kind, public, fingerprint)| {
                let mut key = PrivateKeyInfo::new("OpenSSH").with_public(public, NOT_STORED);
                if key.algorithm.is_none() {
                    key.algorithm = Some(kind);
                }
                key.encrypted.clone_from(&container.cipher);
                key.ssh_fingerprint = Some(fingerprint);
                key
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "privkey_tests.rs"]
mod tests;
