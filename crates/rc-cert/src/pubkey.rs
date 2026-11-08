//! Public keys, whatever file they come from: a certificate's or a request's
//! SubjectPublicKeyInfo, a `PUBLIC KEY` or `RSA PUBLIC KEY` block, the public half a private key
//! file stores, an OpenSSH key. Each is written back as one canonical SubjectPublicKeyInfo, whose
//! SHA-256 is the fingerprint shown and whose equality says that a key belongs to a certificate.
//! Only public numbers are ever held here.

use base64::Engine as _;
use sha2::{Digest as _, Sha256};

use crate::checks::Level;
use crate::der::{self, Elements, tag};
use crate::oids::{self, encoded};
use crate::report::Value;
use crate::show;

/// A public key, its numbers as unsigned magnitudes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicKey {
    Rsa {
        n: Vec<u8>,
        e: Vec<u8>,
    },
    /// A named curve (its encoded identifier) and the encoded point.
    Ec {
        curve: Vec<u8>,
        point: Vec<u8>,
    },
    Ed25519(Vec<u8>),
    Ed448(Vec<u8>),
    X25519(Vec<u8>),
    X448(Vec<u8>),
    Dsa {
        p: Vec<u8>,
        q: Vec<u8>,
        g: Vec<u8>,
        y: Vec<u8>,
    },
    /// An algorithm the viewer does not take apart: its dotted identifier and the
    /// SubjectPublicKeyInfo as it was found.
    Other {
        algorithm: String,
        spki: Vec<u8>,
    },
}

impl PublicKey {
    /// From a DER SubjectPublicKeyInfo.
    #[must_use]
    pub fn from_spki(spki: &[u8]) -> Option<Self> {
        let (content, rest) = der::sequence(spki)?;
        let whole = spki.get(..spki.len().saturating_sub(rest.len()))?;
        let mut fields = Elements::new(content);
        let mut algorithm = Elements::new(fields.pop(tag::SEQUENCE)?);
        let oid = algorithm.pop(tag::OID)?;
        let bits = der::whole_bits(fields.pop(tag::BIT_STRING)?)?;
        let key = match oid {
            encoded::RSA => Self::rsa_from_pkcs1(bits)?,
            encoded::EC => Self::Ec {
                curve: algorithm.pop(tag::OID)?.to_vec(),
                point: bits.to_vec(),
            },
            encoded::ED25519 => Self::Ed25519(bits.to_vec()),
            encoded::ED448 => Self::Ed448(bits.to_vec()),
            encoded::X25519 => Self::X25519(bits.to_vec()),
            encoded::X448 => Self::X448(bits.to_vec()),
            encoded::DSA => {
                let mut params = Elements::new(algorithm.pop(tag::SEQUENCE)?);
                Self::Dsa {
                    p: der::magnitude(params.pop(tag::INTEGER)?).to_vec(),
                    q: der::magnitude(params.pop(tag::INTEGER)?).to_vec(),
                    g: der::magnitude(params.pop(tag::INTEGER)?).to_vec(),
                    y: der::magnitude(
                        der::read(bits)
                            .and_then(|(y, _)| (y.tag == tag::INTEGER).then_some(y.content))?,
                    )
                    .to_vec(),
                }
            }
            other => Self::Other {
                algorithm: oids::dotted(other).unwrap_or_else(|| "?".to_owned()),
                spki: whole.to_vec(),
            },
        };
        Some(key)
    }

    /// From a PKCS#1 `RSAPublicKey` (`SEQUENCE { n, e }`).
    #[must_use]
    pub fn rsa_from_pkcs1(der: &[u8]) -> Option<Self> {
        let (content, _) = der::sequence(der)?;
        let mut fields = Elements::new(content);
        let n = der::magnitude(fields.pop(tag::INTEGER)?).to_vec();
        let e = der::magnitude(fields.pop(tag::INTEGER)?).to_vec();
        (!n.is_empty()).then_some(Self::Rsa { n, e })
    }

    /// The canonical DER SubjectPublicKeyInfo.
    #[must_use]
    pub fn to_spki(&self) -> Vec<u8> {
        let (algorithm, key) = match self {
            Self::Rsa { n, e } => {
                let mut numbers = der::integer(n);
                numbers.extend(der::integer(e));
                let mut algorithm = der::element(tag::OID, encoded::RSA);
                algorithm.extend(der::element(tag::NULL, &[]));
                (algorithm, der::element(tag::SEQUENCE, &numbers))
            }
            Self::Ec { curve, point } => {
                let mut algorithm = der::element(tag::OID, encoded::EC);
                algorithm.extend(der::element(tag::OID, curve));
                (algorithm, point.clone())
            }
            Self::Ed25519(key) => (der::element(tag::OID, encoded::ED25519), key.clone()),
            Self::Ed448(key) => (der::element(tag::OID, encoded::ED448), key.clone()),
            Self::X25519(key) => (der::element(tag::OID, encoded::X25519), key.clone()),
            Self::X448(key) => (der::element(tag::OID, encoded::X448), key.clone()),
            Self::Dsa { p, q, g, y } => {
                let mut params = der::integer(p);
                params.extend(der::integer(q));
                params.extend(der::integer(g));
                let mut algorithm = der::element(tag::OID, encoded::DSA);
                algorithm.extend(der::element(tag::SEQUENCE, &params));
                (algorithm, der::integer(y))
            }
            Self::Other { spki, .. } => return spki.clone(),
        };
        let mut content = der::element(tag::SEQUENCE, &algorithm);
        content.extend(der::bit_string(&key));
        der::element(tag::SEQUENCE, &content)
    }

    /// The algorithm's name: `RSA`, `EC P-256`, `Ed25519`, ...; an algorithm or curve without a
    /// name is its dotted identifier.
    #[must_use]
    pub fn algorithm(&self) -> String {
        match self {
            Self::Rsa { .. } => "RSA".to_owned(),
            Self::Ec { curve, .. } => match oids::curve(curve) {
                Some((name, _)) => format!("EC {name}"),
                None => format!(
                    "EC {}",
                    oids::dotted(curve).unwrap_or_else(|| "?".to_owned())
                ),
            },
            Self::Ed25519(_) => "Ed25519".to_owned(),
            Self::Ed448(_) => "Ed448".to_owned(),
            Self::X25519(_) => "X25519".to_owned(),
            Self::X448(_) => "X448".to_owned(),
            Self::Dsa { .. } => "DSA".to_owned(),
            Self::Other { algorithm, .. } => algorithm.clone(),
        }
    }

    /// The key's size in bits, where it has one.
    #[must_use]
    pub fn bits(&self) -> Option<u32> {
        match self {
            Self::Rsa { n, .. } => Some(der::bit_length(n)),
            Self::Ec { curve, .. } => oids::curve(curve).map(|(_, bits)| bits),
            Self::Ed25519(_) | Self::X25519(_) => Some(256),
            Self::Ed448(_) => Some(456),
            Self::X448(_) => Some(448),
            Self::Dsa { p, .. } => Some(der::bit_length(p)),
            Self::Other { .. } => None,
        }
    }

    /// Algorithm and size, as the report shows them.
    #[must_use]
    pub fn describe(&self) -> Value {
        Value::Key {
            algorithm: self.algorithm(),
            bits: self.bits(),
        }
    }

    /// SHA-256 of the canonical SubjectPublicKeyInfo, as hex pairs.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        show::hex(&Sha256::digest(self.to_spki()))
    }

    /// The key as OpenSSH writes it (`ssh-rsa`, `ssh-ed25519`, `ecdsa-sha2-nistp256`, ...), for
    /// the algorithms OpenSSH has.
    #[must_use]
    pub fn ssh_blob(&self) -> Option<Vec<u8>> {
        let mut blob = Vec::new();
        match self {
            Self::Rsa { n, e } => {
                put_string(&mut blob, b"ssh-rsa");
                put_mpint(&mut blob, e);
                put_mpint(&mut blob, n);
            }
            Self::Ed25519(key) => {
                put_string(&mut blob, b"ssh-ed25519");
                put_string(&mut blob, key);
            }
            Self::Ec { curve, point } => {
                let name: &[u8] = match curve.as_slice() {
                    encoded::P256 => b"nistp256",
                    encoded::P384 => b"nistp384",
                    encoded::P521 => b"nistp521",
                    _ => return None,
                };
                let mut kind = b"ecdsa-sha2-".to_vec();
                kind.extend_from_slice(name);
                put_string(&mut blob, &kind);
                put_string(&mut blob, name);
                put_string(&mut blob, point);
            }
            Self::Dsa { p, q, g, y } => {
                put_string(&mut blob, b"ssh-dss");
                for number in [p, q, g, y] {
                    put_mpint(&mut blob, number);
                }
            }
            Self::Ed448(_) | Self::X25519(_) | Self::X448(_) | Self::Other { .. } => return None,
        }
        Some(blob)
    }

    /// The OpenSSH fingerprint (`SHA256:...`), for the algorithms OpenSSH has.
    #[must_use]
    pub fn ssh_fingerprint(&self) -> Option<String> {
        self.ssh_blob().map(|blob| ssh_fingerprint(&blob))
    }

    /// Why the key is weak, if it is: RSA below 2048 bits, DSA, a curve below 256 bits.
    #[must_use]
    pub fn weakness(&self) -> Option<(Level, Weakness)> {
        match self {
            Self::Rsa { n, .. } if der::bit_length(n) < 2048 => {
                Some((Level::Error, Weakness::SmallRsa(der::bit_length(n))))
            }
            Self::Dsa { .. } => Some((Level::Warn, Weakness::Dsa)),
            Self::Ec { curve, .. } => match oids::curve(curve) {
                Some((name, bits)) if bits < 256 => Some((Level::Warn, Weakness::SmallCurve(name))),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Why a key is weak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weakness {
    /// RSA of this many bits, below 2048.
    SmallRsa(u32),
    /// DSA: OpenSSH and TLS no longer take it.
    Dsa,
    /// A named curve below 256 bits.
    SmallCurve(&'static str),
}

/// `SHA256:` and the unpadded Base64 of the SHA-256 of an OpenSSH key blob, as `ssh-keygen -l`.
#[must_use]
pub fn ssh_fingerprint(blob: &[u8]) -> String {
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(blob))
    )
}

/// Appends an SSH `string`: a 32-bit length and the bytes.
fn put_string(blob: &mut Vec<u8>, bytes: &[u8]) {
    let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    blob.extend_from_slice(&length.to_be_bytes());
    blob.extend_from_slice(bytes);
}

/// Appends an SSH `mpint` of a non-negative magnitude: a zero byte in front of a set top bit.
fn put_mpint(blob: &mut Vec<u8>, magnitude: &[u8]) {
    let digits = der::magnitude(magnitude);
    if digits.first().is_some_and(|&top| top & 0x80 != 0) {
        let mut padded = Vec::with_capacity(digits.len() + 1);
        padded.push(0);
        padded.extend_from_slice(digits);
        put_string(blob, &padded);
    } else {
        put_string(blob, digits);
    }
}

#[cfg(test)]
#[path = "pubkey_tests.rs"]
mod tests;
